//! Mach-O load-command inspection, FFmpeg relocation and the bundle audit.
//!
//! Inspection uses Apple's `otool`; edits use `install_name_tool`. Every
//! FFmpeg reference is rewritten to `@rpath/<install name>`, executables gain
//! one `@executable_path/../Frameworks` search path, libraries gain
//! `@loader_path`, and absolute build-prefix search paths are removed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::{Result, output, run_tool};

pub const EXECUTABLE_RPATH: &str = "@executable_path/../Frameworks";
pub const LIBRARY_RPATH: &str = "@loader_path";

/// Load commands that macOS itself provides on every supported system.
const SYSTEM_PREFIXES: [&str; 2] = ["/System/Library/", "/usr/lib/"];

pub fn is_macho(path: &Path) -> Result<bool> {
    use std::io::Read;
    let mut magic = [0u8; 4];
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    match file.read_exact(&mut magic) {
        Ok(()) => Ok(matches!(
            magic,
            [0xcf, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xcf]
                | [0xca, 0xfe, 0xba, 0xbe]
                | [0xbe, 0xba, 0xfe, 0xca]
        )),
        Err(_) => Ok(false),
    }
}

/// Install names from `otool -D` text: one per architecture of a library;
/// architecture headers end in `:`.
pub fn parse_install_names(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.ends_with(':'))
        .map(str::to_owned)
        .collect()
}

/// Dependencies from `otool -L` text. Each architecture block lists the
/// library's own `LC_ID_DYLIB` first; only that first entry is dropped, and
/// only when it is one of `ids`, so a real dependency is never subtracted.
pub fn parse_dependencies(text: &str, ids: &BTreeSet<String>) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut first_in_block = true;
    for line in text.lines() {
        if !line.starts_with('\t') {
            first_in_block = true;
            continue;
        }
        let Some(name) = line.trim().split(" (compatibility").next() else {
            continue;
        };
        if !(first_in_block && ids.contains(name)) {
            result.insert(name.to_owned());
        }
        first_in_block = false;
    }
    result
}

/// `LC_RPATH` values from `otool -l` text, across architectures.
pub fn parse_rpaths(text: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut in_rpath = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("cmd ") {
            in_rpath = line == "cmd LC_RPATH";
        } else if in_rpath && let Some(rest) = line.strip_prefix("path ") {
            let value = rest.rsplit_once(" (offset").map_or(rest, |(path, _)| path);
            result.insert(value.to_owned());
            in_rpath = false;
        }
    }
    result
}

pub fn install_names(path: &Path) -> Result<BTreeSet<String>> {
    Ok(parse_install_names(&output(
        "otool",
        &["-D".as_ref(), path.as_os_str()],
    )?))
}

/// Every dependent library named by any architecture slice.
pub fn dependencies(path: &Path) -> Result<BTreeSet<String>> {
    let ids = install_names(path)?;
    let text = output("otool", &["-L".as_ref(), path.as_os_str()])?;
    Ok(parse_dependencies(&text, &ids))
}

/// Every `LC_RPATH` in any architecture slice.
pub fn rpaths(path: &Path) -> Result<BTreeSet<String>> {
    Ok(parse_rpaths(&output(
        "otool",
        &["-l".as_ref(), path.as_os_str()],
    )?))
}

/// An absolute, already-normalized path under a macOS system prefix.
pub fn is_system(reference: &str) -> bool {
    let path = Path::new(reference);
    path.is_absolute()
        && lexical_normalize(path) == path
        && SYSTEM_PREFIXES
            .iter()
            .any(|prefix| reference.starts_with(prefix))
}

fn file_name(reference: &str) -> Result<String> {
    Path::new(reference)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("library reference without a file name: {reference}"))
}

/// Copy the transitive FFmpeg libraries that `executables` load from `prefix`
/// into `frameworks` and rewrite every reference. Any other non-system
/// reference is refused. Returns the copied library paths.
pub fn bundle_libraries(
    executables: &[PathBuf],
    frameworks: &Path,
    prefix: &Path,
) -> Result<Vec<PathBuf>> {
    let canonical = fs::canonicalize(prefix).map_err(|e| format!("{}: {e}", prefix.display()))?;
    let from_prefix = |reference: &str| {
        let normalized = lexical_normalize(Path::new(reference));
        Path::new(reference).is_absolute()
            && (normalized.starts_with(prefix) || normalized.starts_with(&canonical))
    };
    fs::create_dir_all(frameworks).map_err(|e| e.to_string())?;
    // Reference string -> bundled file name, for every image's rewrite.
    let mut bundled: BTreeMap<String, String> = BTreeMap::new();
    let mut libraries = Vec::new();
    let mut queue: Vec<PathBuf> = executables.to_vec();
    while let Some(image) = queue.pop() {
        for reference in dependencies(&image)? {
            if is_system(&reference) {
                continue;
            }
            if !from_prefix(&reference) {
                return Err(format!(
                    "{} references {reference}, which is neither a system library nor in the pinned FFmpeg prefix",
                    image.display()
                ));
            }
            if bundled.contains_key(&reference) {
                continue;
            }
            let name = file_name(&reference)?;
            let destination = frameworks.join(&name);
            if !destination.exists() {
                let source =
                    fs::canonicalize(&reference).map_err(|e| format!("{reference}: {e}"))?;
                fs::copy(&source, &destination)
                    .map_err(|e| format!("copy {}: {e}", source.display()))?;
                set_mode(&destination, 0o644)?;
                libraries.push(destination.clone());
                queue.push(destination);
            }
            bundled.insert(reference, name);
        }
    }
    for library in &libraries {
        let name = file_name(&library.to_string_lossy())?;
        rewrite(library, Some(&name), &bundled, LIBRARY_RPATH)?;
    }
    for executable in executables {
        rewrite(executable, None, &bundled, EXECUTABLE_RPATH)?;
    }
    libraries.sort();
    Ok(libraries)
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|e| format!("chmod {}: {e}", path.display()))
}

/// Point FFmpeg references at `@rpath`, remove absolute search paths and add
/// `search` when the image loads any bundled library.
fn rewrite(
    image: &Path,
    id: Option<&str>,
    bundled: &BTreeMap<String, String>,
    search: &str,
) -> Result<()> {
    let mut arguments: Vec<String> = Vec::new();
    if let Some(id) = id {
        arguments.extend(["-id".into(), format!("@rpath/{id}")]);
    }
    let dependencies = dependencies(image)?;
    let mut loads_bundled = false;
    for reference in &dependencies {
        if let Some(name) = bundled.get(reference) {
            arguments.extend([
                "-change".into(),
                reference.clone(),
                format!("@rpath/{name}"),
            ]);
            loads_bundled = true;
        }
    }
    let existing = rpaths(image)?;
    for rpath in &existing {
        if !rpath.starts_with('@') {
            arguments.extend(["-delete_rpath".into(), rpath.clone()]);
        }
    }
    if loads_bundled && !existing.contains(search) {
        arguments.extend(["-add_rpath".into(), search.into()]);
    }
    if arguments.is_empty() {
        return Ok(());
    }
    let mut command: Vec<&std::ffi::OsStr> = arguments.iter().map(|a| a.as_ref()).collect();
    command.push(image.as_os_str());
    run_tool("install_name_tool", &command)
}

/// One Mach-O file's audited load references.
pub struct AuditedImage {
    pub path: PathBuf,
    pub dependencies: usize,
    pub bundled: Vec<String>,
}

pub struct Audit {
    pub images: Vec<AuditedImage>,
    pub problems: Vec<String>,
    /// Informational: absolute build-host paths embedded as plain bytes, such
    /// as source locations in panic messages. These are not load references.
    pub embedded_build_paths: Vec<(PathBuf, String, usize)>,
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other),
        }
    }
    result
}

fn files(directory: &Path, into: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .map_err(|e| format!("{}: {e}", directory.display()))?
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            files(&path, into)?;
        } else {
            into.push(path);
        }
    }
    Ok(())
}

/// Every regular file in the bundle, sorted.
pub fn bundle_files(app: &Path) -> Result<Vec<PathBuf>> {
    let mut all = Vec::new();
    files(app, &mut all)?;
    Ok(all)
}

/// Prove that every Mach-O in `app` loads only system libraries or libraries
/// inside the bundle, through bundle-relative search paths.
pub fn audit(app: &Path, build_markers: &[String]) -> Result<Audit> {
    let app = fs::canonicalize(app).map_err(|e| format!("{}: {e}", app.display()))?;
    let contents = app.join("Contents");
    let main_rpaths = rpaths(&contents.join("MacOS/deadpan-app"))?;
    let mut audit = Audit {
        images: Vec::new(),
        problems: Vec::new(),
        embedded_build_paths: Vec::new(),
    };
    for path in bundle_files(&app)? {
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            let target = fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if !target.starts_with(&app) {
                audit
                    .problems
                    .push(format!("{} links outside the bundle", path.display()));
            }
            continue;
        }
        if !is_macho(&path)? {
            continue;
        }
        let relative = path.strip_prefix(&app).unwrap_or(&path).to_owned();
        let in_macos = path.parent() == Some(contents.join("MacOS").as_path());
        let own_rpaths = rpaths(&path)?;
        let executable_directory = if in_macos {
            path.parent().unwrap().to_owned()
        } else {
            contents.join("MacOS")
        };
        let loader_directory = path.parent().unwrap().to_owned();
        let expand = |value: &str| -> Option<PathBuf> {
            if let Some(rest) = value.strip_prefix("@executable_path/") {
                Some(lexical_normalize(&executable_directory.join(rest)))
            } else {
                value.strip_prefix("@loader_path").map(|rest| {
                    lexical_normalize(&loader_directory.join(rest.trim_start_matches('/')))
                })
            }
        };
        for rpath in &own_rpaths {
            match expand(rpath) {
                Some(directory) if directory.starts_with(&app) => {}
                Some(directory) => audit.problems.push(format!(
                    "{}: LC_RPATH {rpath} escapes the bundle to {}",
                    relative.display(),
                    directory.display()
                )),
                None => audit.problems.push(format!(
                    "{}: LC_RPATH {rpath} is not bundle-relative",
                    relative.display()
                )),
            }
        }
        let search: Vec<PathBuf> = own_rpaths
            .iter()
            .chain(
                if in_macos { None } else { Some(&main_rpaths) }
                    .into_iter()
                    .flatten(),
            )
            .filter_map(|rpath| expand(rpath))
            .collect();
        let dependencies = dependencies(&path)?;
        let mut bundled = Vec::new();
        for reference in &dependencies {
            if is_system(reference) {
                continue;
            }
            let resolved = if let Some(rest) = reference.strip_prefix("@rpath/") {
                search
                    .iter()
                    .map(|directory| lexical_normalize(&directory.join(rest)))
                    .find(|candidate| candidate.is_file())
            } else {
                expand(reference).filter(|candidate| candidate.is_file())
            };
            // Resolve symbolic links too, so a link cannot leave the bundle.
            let resolved = resolved.map(|file| fs::canonicalize(&file).unwrap_or(file));
            match resolved {
                Some(file) if file.starts_with(&app) => bundled.push(
                    file.strip_prefix(&app)
                        .unwrap_or(&file)
                        .display()
                        .to_string(),
                ),
                Some(file) => audit.problems.push(format!(
                    "{}: {reference} resolves outside the bundle to {}",
                    relative.display(),
                    file.display()
                )),
                None => audit.problems.push(format!(
                    "{}: {reference} is not a system library and does not resolve inside the bundle",
                    relative.display()
                )),
            }
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        for marker in build_markers {
            let count = bytes
                .windows(marker.len())
                .filter(|window| *window == marker.as_bytes())
                .count();
            if count > 0 {
                audit
                    .embedded_build_paths
                    .push((relative.clone(), marker.clone(), count));
            }
        }
        audit.images.push(AuditedImage {
            path: relative,
            dependencies: dependencies.len(),
            bundled,
        });
    }
    Ok(audit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_relative_paths_normalize_lexically() {
        assert_eq!(
            lexical_normalize(Path::new("/A.app/Contents/MacOS/../Frameworks/libx.dylib")),
            PathBuf::from("/A.app/Contents/Frameworks/libx.dylib")
        );
    }

    #[test]
    fn otool_text_parsers_handle_universal_files_and_ids() {
        let ids = parse_install_names(
            "/p/libfoo.1.dylib (architecture x86_64):\n/p/lib/libfoo.1.dylib\n/p/libfoo.1.dylib (architecture arm64):\n/p/lib/libfoo.1.dylib\n",
        );
        assert_eq!(ids, BTreeSet::from(["/p/lib/libfoo.1.dylib".to_owned()]));
        let listing = "/p/libfoo.1.dylib (architecture x86_64):\n\t/p/lib/libfoo.1.dylib (compatibility version 1.0.0, current version 1.0.0)\n\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1.0.0)\n/p/libfoo.1.dylib (architecture arm64):\n\t/p/lib/libfoo.1.dylib (compatibility version 1.0.0, current version 1.0.0)\n\t/p/lib/libbar.1.dylib (compatibility version 1.0.0, current version 1.0.0)\n";
        assert_eq!(
            parse_dependencies(listing, &ids),
            BTreeSet::from([
                "/usr/lib/libSystem.B.dylib".to_owned(),
                "/p/lib/libbar.1.dylib".to_owned()
            ])
        );
        // An executable has no id; a dependency equal to some other image's
        // name is never subtracted.
        let executable = "/p/tool:\n\t/p/lib/libfoo.1.dylib (compatibility version 1.0.0, current version 1.0.0)\n";
        assert_eq!(parse_dependencies(executable, &BTreeSet::new()).len(), 1);
        let load = "Load command 12\n          cmd LC_RPATH\n      cmdsize 48\n         path @executable_path/../Frameworks (offset 12)\nLoad command 13\n          cmd LC_FUNCTION_STARTS\n      cmdsize 16\n";
        assert_eq!(
            parse_rpaths(load),
            BTreeSet::from(["@executable_path/../Frameworks".to_owned()])
        );
    }

    #[test]
    fn system_references_must_be_normalized() {
        assert!(is_system("/usr/lib/libSystem.B.dylib"));
        assert!(is_system(
            "/System/Library/Frameworks/Metal.framework/Versions/A/Metal"
        ));
        assert!(!is_system("/usr/lib/../../opt/homebrew/lib/libx.dylib"));
        assert!(!is_system("/usr/local/lib/libx.dylib"));
        assert!(!is_system("@rpath/libx.dylib"));
    }

    fn clang() -> Option<String> {
        output("xcrun", &["--find", "clang"])
            .ok()
            .map(|path| path.trim().to_owned())
    }

    fn compile(_clang: &str, arguments: &[&str]) {
        let mut full = vec!["--sdk", "macosx", "clang"];
        full.extend_from_slice(arguments);
        run_tool("xcrun", &full).unwrap();
    }

    /// A real prefix library pair and executable relocated into a synthetic
    /// bundle, audited, signed and run; then two escapes the audit rejects.
    #[test]
    fn relocation_and_audit_on_real_mach_o_files() {
        let Some(clang) = clang() else {
            eprintln!("skipped: clang is unavailable");
            return;
        };
        let root = std::env::temp_dir().join(format!("xtask-macho-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let prefix = root.join("prefix");
        let library = prefix.join("lib");
        fs::create_dir_all(&library).unwrap();
        let source = root.join("src");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("foo.c"), "int foo(void) { return 40; }\n").unwrap();
        fs::write(
            source.join("bar.c"),
            "int foo(void);\nint bar(void) { return foo() + 2; }\n",
        )
        .unwrap();
        fs::write(
            source.join("main.c"),
            "#include <stdio.h>\nint bar(void);\nint main(void) { printf(\"%d\\n\", bar()); return 0; }\n",
        )
        .unwrap();
        let path = |p: PathBuf| p.display().to_string();
        let foo = path(library.join("libfoo.1.dylib"));
        let bar = path(library.join("libbar.1.dylib"));
        compile(
            &clang,
            &[
                "-dynamiclib",
                "-o",
                &foo,
                "-install_name",
                &foo,
                &path(source.join("foo.c")),
            ],
        );
        compile(
            &clang,
            &[
                "-dynamiclib",
                "-o",
                &bar,
                "-install_name",
                &bar,
                &path(source.join("bar.c")),
                &foo,
            ],
        );
        let app = root.join("Test.app");
        let macos = app.join("Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        let executable = macos.join("deadpan-app");
        let rpath = format!("-Wl,-rpath,{}", library.display());
        compile(
            &clang,
            &[
                "-o",
                &path(executable.clone()),
                &path(source.join("main.c")),
                &bar,
                &rpath,
            ],
        );

        let frameworks = app.join("Contents/Frameworks");
        let copied =
            bundle_libraries(std::slice::from_ref(&executable), &frameworks, &prefix).unwrap();
        assert_eq!(copied.len(), 2);
        assert!(
            rpaths(&executable)
                .unwrap()
                .iter()
                .all(|rpath| rpath.starts_with('@'))
        );
        assert!(
            dependencies(&executable)
                .unwrap()
                .contains("@rpath/libbar.1.dylib")
        );
        let audit = audit(&app, &[prefix.display().to_string()]).unwrap();
        assert!(audit.problems.is_empty(), "{:?}", audit.problems);
        assert_eq!(audit.images.len(), 3);
        // Signed after editing, the relocated program runs without the prefix.
        fs::remove_dir_all(&prefix).unwrap();
        for image in copied.iter().chain([&executable]) {
            run_tool(
                "codesign",
                &[
                    "--force".as_ref(),
                    "-s".as_ref(),
                    "-".as_ref(),
                    image.as_os_str(),
                ],
            )
            .unwrap();
        }
        assert_eq!(
            output(&path(executable.clone()), &[] as &[&str])
                .unwrap()
                .trim(),
            "42"
        );

        // An rpath that leaves the bundle is a problem.
        let libfoo = frameworks.join("libfoo.1.dylib");
        run_tool(
            "install_name_tool",
            &[
                "-add_rpath".as_ref(),
                "@loader_path/../../../..".as_ref(),
                libfoo.as_os_str(),
            ],
        )
        .unwrap();
        let escaped = super::audit(&app, &[]).unwrap();
        assert!(
            escaped
                .problems
                .iter()
                .any(|p| p.contains("escapes the bundle")),
            "{:?}",
            escaped.problems
        );
        // A non-system absolute reference is a problem.
        run_tool(
            "install_name_tool",
            &[
                "-delete_rpath".as_ref(),
                "@loader_path/../../../..".as_ref(),
                libfoo.as_os_str(),
            ],
        )
        .unwrap();
        run_tool(
            "install_name_tool",
            &[
                "-change".as_ref(),
                "@rpath/libfoo.1.dylib".as_ref(),
                "/usr/lib/../local/libfoo.1.dylib".as_ref(),
                frameworks.join("libbar.1.dylib").as_os_str(),
            ],
        )
        .unwrap();
        let external = super::audit(&app, &[]).unwrap();
        assert!(
            external
                .problems
                .iter()
                .any(|p| p.contains("/usr/lib/../local/libfoo.1.dylib")),
            "{:?}",
            external.problems
        );
        // Relocation refuses a reference outside the prefix.
        let other = root.join("Other.app/Contents");
        fs::create_dir_all(other.join("MacOS")).unwrap();
        let stray = other.join("MacOS/tool");
        fs::copy(&executable, &stray).unwrap();
        run_tool(
            "install_name_tool",
            &[
                "-change".as_ref(),
                "@rpath/libbar.1.dylib".as_ref(),
                "/opt/homebrew/lib/libbar.dylib".as_ref(),
                stray.as_os_str(),
            ],
        )
        .unwrap();
        let refused = bundle_libraries(
            &[stray],
            &other.join("Frameworks"),
            &root.join("prefix-absent"),
        );
        assert!(refused.is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
