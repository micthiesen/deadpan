//! Third-party notices and a CycloneDX software bill of materials.
//!
//! Rust components come from `cargo metadata` for the shipped binaries'
//! normal (non-dev, non-build) dependency closure on aarch64-apple-darwin.
//! License files are copied from each crate's published source, including
//! vendored native code below the crate root. Non-Rust components (FFmpeg and
//! the downloader helpers) use repository pins and vendored upstream notices
//! whose hashes `packaging/notices/sources.json` fixes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::spdx;
use super::{Result, output_in, sha256_file, sha256_hex};

/// Binaries shipped in the bundle.
pub const SHIPPED_PACKAGES: [&str; 5] = [
    "deadpan-app",
    "deadpan-cli",
    "deadpan-media-worker",
    "deadpan-transcribe",
    "deadpan-track",
];

const LICENSE_NAMES: [&str; 6] = [
    "LICENSE",
    "LICENCE",
    "COPYING",
    "NOTICE",
    "COPYRIGHT",
    "UNLICENSE",
];
const SKIPPED_DIRECTORIES: [&str; 9] = [
    "target",
    "tests",
    "test",
    "examples",
    "benches",
    ".git",
    ".github",
    "fuzz",
    "node_modules",
];
const LICENSE_FILE_LIMIT: u64 = 512 * 1024;
const SEARCH_DEPTH: usize = 3;

pub struct Crate {
    pub id: String,
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    /// Validated SPDX expression: the declared one with legacy `/`
    /// normalized, or a `LicenseRef-cargo-*` for a license-file-only crate.
    pub spdx: String,
    pub repository: Option<String>,
    pub authors: Vec<String>,
    pub workspace: bool,
    pub checksum: Option<String>,
    pub directory: PathBuf,
    pub license_file: Option<PathBuf>,
    pub dependencies: Vec<String>,
}

/// The shipped crates in name/version order.
pub fn shipped_crates(workspace: &Path) -> Result<Vec<Crate>> {
    let text = output_in(
        workspace,
        "cargo",
        &[
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            "aarch64-apple-darwin",
        ],
    )?;
    let metadata: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let checksums = lock_checksums(&workspace.join("Cargo.lock"))?;
    let members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .ok_or("cargo metadata has no workspace members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let packages: BTreeMap<&str, &Value> = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata has no packages")?
        .iter()
        .filter_map(|package| Some((package["id"].as_str()?, package)))
        .collect();
    let nodes: BTreeMap<&str, &Value> = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or("cargo metadata has no resolve graph")?
        .iter()
        .filter_map(|node| Some((node["id"].as_str()?, node)))
        .collect();
    let normal = |dependency: &Value| {
        dependency["dep_kinds"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind["kind"].is_null()))
    };
    let mut queue: Vec<&str> = packages
        .iter()
        .filter(|(id, package)| {
            members.contains(**id)
                && package["name"]
                    .as_str()
                    .is_some_and(|name| SHIPPED_PACKAGES.contains(&name))
        })
        .map(|(id, _)| *id)
        .collect();
    if queue.len() != SHIPPED_PACKAGES.len() {
        return Err("cargo metadata is missing a shipped workspace package".into());
    }
    let mut seen = BTreeSet::new();
    let mut crates = Vec::new();
    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        let package = packages
            .get(id)
            .ok_or_else(|| format!("unknown package {id}"))?;
        let dependencies: Vec<String> = nodes
            .get(id)
            .and_then(|node| node["deps"].as_array())
            .into_iter()
            .flatten()
            .filter(|dependency| normal(dependency))
            .filter_map(|dependency| dependency["pkg"].as_str())
            .map(str::to_owned)
            .collect();
        queue.extend(
            dependencies
                .iter()
                .filter_map(|dependency| packages.get_key_value(dependency.as_str()))
                .map(|(id, _)| *id),
        );
        let name = package["name"].as_str().unwrap_or_default().to_owned();
        let version = package["version"].as_str().unwrap_or_default().to_owned();
        let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap_or_default());
        let directory = manifest.parent().unwrap_or(Path::new("/")).to_owned();
        crates.push(Crate {
            id: id.to_owned(),
            checksum: checksums.get(&(name.clone(), version.clone())).cloned(),
            license: package["license"].as_str().map(str::to_owned),
            spdx: String::new(),
            repository: package["repository"].as_str().map(str::to_owned),
            authors: package["authors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            workspace: members.contains(id),
            license_file: package["license_file"]
                .as_str()
                .map(|file| directory.join(file)),
            directory,
            name,
            version,
            dependencies,
        });
    }
    crates.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    let identifiers = spdx_identifiers(workspace)?;
    let mut invalid = Vec::new();
    for item in &mut crates {
        match crate_expression(item, &identifiers) {
            Ok(expression) => item.spdx = expression,
            Err(error) => invalid.push(format!("{} {}: {error}", item.name, item.version)),
        }
    }
    if !invalid.is_empty() {
        return Err(format!(
            "invalid crate licenses:\n  {}",
            invalid.join("\n  ")
        ));
    }
    Ok(crates)
}

pub fn spdx_identifiers(workspace: &Path) -> Result<spdx::Identifiers> {
    let root = workspace.join("packaging/notices/spdx");
    let read = |name: &str| fs::read_to_string(root.join(name)).map_err(|e| format!("{name}: {e}"));
    Ok(spdx::Identifiers::parse(
        &read("license-ids.txt")?,
        &read("exception-ids.txt")?,
    ))
}

fn crate_expression(item: &Crate, identifiers: &spdx::Identifiers) -> Result<String> {
    match (&item.license, &item.license_file) {
        (Some(license), _) => {
            let normalized = spdx::normalize(license);
            spdx::parse(&normalized, identifiers)?;
            Ok(normalized)
        }
        (None, Some(_)) => Ok(format!(
            "LicenseRef-cargo-{}-{}",
            item.name,
            item.version
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '.' {
                    c
                } else {
                    '-'
                })
                .collect::<String>()
        )),
        (None, None) => Err("declares neither a license nor a license file".into()),
    }
}

/// `(name, version) -> sha256` for registry packages in Cargo.lock.
fn lock_checksums(lock: &Path) -> Result<BTreeMap<(String, String), String>> {
    let text = fs::read_to_string(lock).map_err(|e| format!("{}: {e}", lock.display()))?;
    let mut result = BTreeMap::new();
    for block in text.split("[[package]]").skip(1) {
        let field = |key: &str| {
            block.lines().find_map(|line| {
                line.strip_prefix(&format!("{key} = \""))
                    .and_then(|rest| rest.strip_suffix('"'))
                    .map(str::to_owned)
            })
        };
        if let (Some(name), Some(version), Some(checksum)) =
            (field("name"), field("version"), field("checksum"))
        {
            result.insert((name, version), checksum);
        }
    }
    Ok(result)
}

fn is_license_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let source_code = Path::new(&upper).extension().is_some_and(|extension| {
        [
            "RS", "C", "H", "CC", "CPP", "HPP", "M", "PY", "JS", "TOML", "JSON", "YML",
        ]
        .iter()
        .any(|code| extension == *code)
    });
    // Font licenses include OFL.txt and the Ubuntu Font Licence UFL.txt.
    !source_code
        && (LICENSE_NAMES.iter().any(|prefix| upper.starts_with(prefix))
            || upper.contains("LICENSE")
            || upper.contains("LICENCE")
            || upper == "OFL.TXT"
            || upper == "UFL.TXT")
}

/// A `.txt` file named after a sibling font carries that font's license.
fn is_font_license(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "txt")
        && ["ttf", "otf"]
            .iter()
            .any(|font| path.with_extension(font).is_file())
}

/// License files at the crate root and in vendored subdirectories.
fn license_files(directory: &Path, depth: usize, into: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(|entry| entry.ok()).collect();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if depth < SEARCH_DEPTH && !SKIPPED_DIRECTORIES.contains(&name.as_str()) {
                license_files(&entry.path(), depth + 1, into);
            }
        } else if kind.is_file()
            && (is_license_name(&name) || is_font_license(&entry.path()))
            && entry
                .metadata()
                .is_ok_and(|metadata| metadata.len() <= LICENSE_FILE_LIMIT)
        {
            into.push(entry.path());
        }
    }
}

/// One upstream notice file vendored under `packaging/notices`.
pub struct VendoredNotice {
    pub path: String,
    pub component: String,
    pub source: String,
    pub file: PathBuf,
}

/// The vendored notices after checking each against its recorded hash.
pub fn vendored_notices(workspace: &Path) -> Result<Vec<VendoredNotice>> {
    let root = workspace.join("packaging/notices");
    let sources: Value = serde_json::from_slice(
        &fs::read(root.join("sources.json")).map_err(|e| format!("notice sources: {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for entry in sources["files"]
        .as_array()
        .ok_or("notice sources has no files")?
    {
        let path = entry["path"].as_str().ok_or("notice without a path")?;
        let file = root.join(path);
        let digest = sha256_file(&file)?;
        if Some(digest.as_str()) != entry["sha256"].as_str() {
            return Err(format!(
                "{} does not match its recorded SHA-256",
                file.display()
            ));
        }
        result.push(VendoredNotice {
            path: path.into(),
            component: entry["component"].as_str().unwrap_or_default().into(),
            source: entry["source"].as_str().unwrap_or_default().into(),
            file,
        });
    }
    Ok(result)
}

/// Facts about the shipped FFmpeg build.
pub struct Ffmpeg {
    pub version: String,
    pub tag: String,
    pub commit: String,
    pub archive_url: String,
    pub archive_sha256: String,
    pub configuration: String,
    /// Bundled library file name -> SHA-256 of the shipped (signed) file.
    pub libraries: Vec<(String, String)>,
}

/// One downloader helper as shipped.
pub struct Helper {
    pub name: String,
    pub version: String,
    pub license: String,
    pub upstream_sha256: String,
    pub sha256: String,
    pub path: String,
    pub signature: String,
}

/// Write `THIRD_PARTY_NOTICES.txt` and copy the vendored notice files.
/// Returns the crates that carry no license file.
pub fn write_notices(
    directory: &Path,
    workspace: &Path,
    crates: &[Crate],
    vendored: &[VendoredNotice],
    ffmpeg: &Ffmpeg,
    helpers: &[Helper],
) -> Result<Vec<String>> {
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let identifiers = spdx_identifiers(workspace)?;
    let spdx_texts = workspace.join("packaging/notices/spdx");
    let has_text = |id: &str| !id.contains('/') && spdx_texts.join(format!("{id}.txt")).is_file();
    for notice in vendored
        .iter()
        .filter(|notice| !notice.path.ends_with("-ids.txt"))
    {
        let destination = directory.join(&notice.path);
        fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(&notice.file, &destination).map_err(|e| e.to_string())?;
    }
    let mut text = String::new();
    let rule = "=".repeat(78);
    let _ = writeln!(
        text,
        "Deadpan third-party notices\n\nDeadpan's own code is MIT licensed (below). This application also contains\nthe components listed here under their own licenses. The full texts of\nnon-Rust upstream notices are in this directory beside this file.\n"
    );
    let deadpan_license =
        fs::read_to_string(workspace.join("LICENSE")).map_err(|e| e.to_string())?;
    let _ = writeln!(text, "{rule}\nDeadpan\n{rule}\n{deadpan_license}");

    let _ = writeln!(
        text,
        "{rule}\nFFmpeg {version} (Contents/Frameworks)\n{rule}\n\
FFmpeg is licensed under the GNU Lesser General Public License version 2.1 or\n\
later (ffmpeg/COPYING.LGPLv2.1, ffmpeg/LICENSE.md). Deadpan dynamically links\n\
these libraries, built from the unmodified source below; no GPL, nonfree or\n\
version-3 components are enabled. For relocation their install names and\n\
library references were rewritten to @rpath and they were re-signed; their code\n\
is otherwise as built. They are separate files in Contents/Frameworks and may be\n\
replaced by a compatible build of the same release (re-sign the bundle\n\
afterwards; macOS requires valid code signatures).\n\n\
Corresponding source: {url}\n\
  SHA-256 {sha}, release tag {tag}, commit {commit}\n\
Build configuration: {configuration}\n",
        version = ffmpeg.version,
        url = ffmpeg.archive_url,
        sha = ffmpeg.archive_sha256,
        tag = ffmpeg.tag,
        commit = ffmpeg.commit,
        configuration = ffmpeg.configuration,
    );
    for (name, sha) in &ffmpeg.libraries {
        let _ = writeln!(text, "  {name}  sha256 {sha}");
    }
    text.push('\n');

    for helper in helpers {
        let _ = writeln!(
            text,
            "{rule}\n{} {} ({})\n{rule}\nLicense: {}. Pinned upstream executable SHA-256 {}.\nShipped file SHA-256 {} ({} signature).",
            helper.name,
            helper.version,
            helper.path,
            helper.license,
            helper.upstream_sha256,
            helper.sha256,
            helper.signature
        );
        for notice in vendored
            .iter()
            .filter(|notice| notice.path.starts_with(&format!("{}/", helper.name)))
        {
            let _ = writeln!(
                text,
                "See {} ({}; from {}).",
                notice.path, notice.component, notice.source
            );
        }
        if helper.name == "deno" {
            let _ = writeln!(
                text,
                "The deno executable statically links V8 and its third-party libraries (ICU,\nAbseil, libc++, simdutf, Highway and others), the Rust standard library and\nDeno's Rust crates; their notices are aggregated in {} with\nthe source revisions in {}.",
                super::deno_notices::BUNDLED_NOTICES,
                super::deno_notices::BUNDLED_MANIFEST
            );
        }
        if helper.name == "yt-dlp" {
            let _ = writeln!(
                text,
                "The yt-dlp executable embeds Python, its libraries and yt-dlp-ejs; their\nnotices are aggregated in yt-dlp/THIRD_PARTY_LICENSES.txt and\nyt-dlp-ejs/LICENSE."
            );
        }
        text.push('\n');
    }

    let _ = writeln!(
        text,
        "{rule}\nInter 4.001 (variable font, embedded in the application)\n{rule}\nThe unmodified Inter variable font draws captions. It is licensed under the\nSIL Open Font License 1.1; the full text is inter/OFL.txt beside this file.\n"
    );
    let _ = writeln!(
        text,
        "{rule}\nRust crates and vendored native code\n{rule}\nGenerated from Cargo metadata for the shipped binaries' normal dependency\nclosure ({} crates). Each entry reproduces the license files published with the\ncrate, including vendored native sources (for example whisper.cpp, SQLite and\nSignalsmith). Identical texts are reproduced once and referenced afterwards.\n",
        crates.len()
    );
    let mut printed: BTreeMap<String, String> = BTreeMap::new();
    let mut missing = Vec::new();
    let mut unsatisfied = Vec::new();
    for item in crates.iter().filter(|item| !item.workspace) {
        let mut files = Vec::new();
        license_files(&item.directory, 0, &mut files);
        if let Some(file) = &item.license_file
            && !files.contains(file)
        {
            files.push(file.clone());
        }
        let _ = writeln!(
            text,
            "{}\n{} {}\nLicense: {}\nAuthors: {}\nRepository: {}\n",
            "-".repeat(78),
            item.name,
            item.version,
            if item.license.is_some() {
                item.spdx.clone()
            } else {
                format!("{} (license file below)", item.spdx)
            },
            if item.authors.is_empty() {
                "(not declared)".into()
            } else {
                item.authors.join(", ")
            },
            item.repository.as_deref().unwrap_or("(not declared)")
        );
        if files.is_empty() {
            let expression = spdx::parse(&item.spdx, &identifiers)?;
            if !spdx::satisfiable(&expression, &has_text) {
                unsatisfied.push(format!("{} {} ({})", item.name, item.version, item.spdx));
            }
            missing.push(format!("{} {}", item.name, item.version));
            let _ = writeln!(
                text,
                "Copyright (c) the {} authors{}.\nThe published crate contains no license file. It is used under its declared\nlicense above; the standard texts are in spdx/ beside this file.\n",
                item.name,
                if item.authors.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", item.authors.join(", "))
                }
            );
        }
        for file in files {
            let Ok(body) = fs::read_to_string(&file) else {
                continue;
            };
            let relative = file
                .strip_prefix(&item.directory)
                .unwrap_or(&file)
                .display()
                .to_string();
            let digest = sha256_hex(body.as_bytes());
            let label = format!("{} {} {relative}", item.name, item.version);
            match printed.get(&digest) {
                Some(first) => {
                    let _ = writeln!(text, "[{relative}: identical to {first} above]\n");
                }
                None => {
                    let _ = writeln!(text, "[{relative}]\n{}\n", body.trim_end());
                    printed.insert(digest, label);
                }
            }
        }
    }
    if !unsatisfied.is_empty() {
        return Err(format!(
            "crates without license files need a bundled SPDX text for their license; add it under packaging/notices/spdx:\n  {}",
            unsatisfied.join("\n  ")
        ));
    }
    let _ = writeln!(text, "{rule}\nWorkspace crates (MIT, Deadpan)\n{rule}");
    for item in crates.iter().filter(|item| item.workspace) {
        let mut files = Vec::new();
        license_files(&item.directory, 0, &mut files);
        let _ = writeln!(text, "{} {}", item.name, item.version);
        for file in files {
            let Ok(body) = fs::read_to_string(&file) else {
                continue;
            };
            let relative = file
                .strip_prefix(&item.directory)
                .unwrap_or(&file)
                .display()
                .to_string();
            let digest = sha256_hex(body.as_bytes());
            match printed.get(&digest) {
                Some(first) => {
                    let _ = writeln!(text, "[{relative}: identical to {first} above]");
                }
                None => {
                    let _ = writeln!(text, "[{relative}]\n{}\n", body.trim_end());
                    printed.insert(digest, format!("{} {relative}", item.name));
                }
            }
        }
    }
    fs::write(directory.join("THIRD_PARTY_NOTICES.txt"), text).map_err(|e| e.to_string())?;
    Ok(missing)
}

/// CycloneDX 1.5 JSON for the shipped bundle.
pub fn sbom(
    version: &str,
    timestamp: &str,
    crates: &[Crate],
    ffmpeg: &Ffmpeg,
    helpers: &[Helper],
    native: &[Value],
) -> Value {
    let reference = |id: &str| format!("cargo:{id}");
    let mut components: Vec<Value> = crates
        .iter()
        .map(|item| {
            let mut component = json!({
                "type": if item.workspace { "application" } else { "library" },
                "bom-ref": reference(&item.id),
                "name": item.name,
                "version": item.version,
                "purl": format!("pkg:cargo/{}@{}", item.name, item.version),
            });
            component["licenses"] = json!([{ "expression": item.spdx }]);
            if let Some(checksum) = &item.checksum {
                component["hashes"] = json!([{ "alg": "SHA-256", "content": checksum }]);
            }
            if let Some(repository) = &item.repository {
                component["externalReferences"] = json!([{ "type": "vcs", "url": repository }]);
            }
            if item.workspace {
                component["properties"] =
                    json!([{ "name": "deadpan:source", "value": "workspace" }]);
            }
            component
        })
        .collect();
    components.push(json!({
        "type": "library",
        "bom-ref": "ffmpeg",
        "name": "FFmpeg",
        "version": ffmpeg.version,
        "licenses": [{ "license": { "id": "LGPL-2.1-or-later" } }],
        "purl": format!("pkg:generic/ffmpeg@{}?download_url={}", ffmpeg.version, ffmpeg.archive_url),
        "hashes": [{ "alg": "SHA-256", "content": ffmpeg.archive_sha256 }],
        "externalReferences": [
            { "type": "distribution", "url": ffmpeg.archive_url },
            { "type": "vcs", "url": format!("https://git.ffmpeg.org/ffmpeg.git#{}", ffmpeg.commit) },
        ],
        "properties": [{ "name": "deadpan:configuration", "value": ffmpeg.configuration }],
        "components": ffmpeg.libraries.iter().map(|(name, sha)| json!({
            "type": "file",
            "name": format!("Contents/Frameworks/{name}"),
            "hashes": [{ "alg": "SHA-256", "content": sha }],
        })).collect::<Vec<_>>(),
    }));
    for helper in helpers {
        components.push(json!({
            "type": "application",
            "bom-ref": format!("helper:{}", helper.name),
            "name": helper.name,
            "version": helper.version,
            "licenses": [{ "license": { "id": helper.license } }],
            "hashes": [{ "alg": "SHA-256", "content": helper.upstream_sha256 }],
            "properties": [
                { "name": "deadpan:path", "value": helper.path },
                { "name": "deadpan:shipped-sha256", "value": helper.sha256 },
                { "name": "deadpan:signature", "value": helper.signature },
            ],
        }));
    }
    components.extend(native.iter().cloned());
    let dependencies: Vec<Value> = crates
        .iter()
        .map(|item| {
            json!({
                "ref": reference(&item.id),
                "dependsOn": item.dependencies.iter().map(|id| reference(id)).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "timestamp": timestamp,
            "tools": { "components": [{ "type": "application", "name": "cargo xtask bundle" }] },
            "component": {
                "type": "application",
                "bom-ref": "deadpan",
                "name": "Deadpan",
                "version": version,
                "licenses": [{ "license": { "id": "MIT" } }],
            },
        },
        "components": components,
        "dependencies": dependencies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_names_match_common_spellings() {
        for name in [
            "LICENSE",
            "LICENSE-MIT",
            "license.txt",
            "COPYING.LGPLv2.1",
            "NOTICE",
            "UNLICENSE",
            "Licence.md",
        ] {
            assert!(is_license_name(name), "{name}");
        }
        for name in [
            "README.md",
            "Cargo.toml",
            "lib.rs",
            "license.rs",
            "licenses.json",
        ] {
            assert!(!is_license_name(name), "{name}");
        }
    }

    fn sample_crate(name: &str, license: Option<&str>, dependencies: &[&str]) -> Crate {
        Crate {
            id: format!("{name}-id"),
            name: name.into(),
            version: "1.0.0".into(),
            license: license.map(str::to_owned),
            spdx: String::new(),
            repository: Some("https://example.invalid/repo".into()),
            authors: vec!["Example Author".into()],
            workspace: name == "deadpan-app",
            checksum: Some("ab".repeat(32)),
            directory: PathBuf::from("/nonexistent"),
            license_file: license
                .is_none()
                .then(|| PathBuf::from("/nonexistent/LICENSE.txt")),
            dependencies: dependencies.iter().map(|id| format!("{id}-id")).collect(),
        }
    }

    #[test]
    fn crate_expressions_normalize_and_reference_license_files() {
        let ids = spdx_identifiers(&super::super::workspace_root()).unwrap();
        let legacy = sample_crate("bitflags", Some("MIT/Apache-2.0"), &[]);
        assert_eq!(
            crate_expression(&legacy, &ids).unwrap(),
            "MIT OR Apache-2.0"
        );
        let file_only = sample_crate("odd", None, &[]);
        assert_eq!(
            crate_expression(&file_only, &ids).unwrap(),
            "LicenseRef-cargo-odd-1.0.0"
        );
        let bogus = sample_crate("bogus", Some("Proprietary-Thing"), &[]);
        assert!(crate_expression(&bogus, &ids).is_err());
        let neither = Crate {
            license_file: None,
            ..sample_crate("neither", None, &[])
        };
        assert!(crate_expression(&neither, &ids).is_err());
    }

    #[test]
    fn sbom_is_cyclonedx_with_valid_spdx_and_closed_references() {
        let ids = spdx_identifiers(&super::super::workspace_root()).unwrap();
        let mut crates = vec![
            sample_crate("deadpan-app", Some("MIT"), &["bitflags", "odd"]),
            sample_crate("bitflags", Some("MIT/Apache-2.0"), &[]),
            sample_crate("odd", None, &[]),
        ];
        for item in &mut crates {
            item.spdx = crate_expression(item, &ids).unwrap();
        }
        let ffmpeg = Ffmpeg {
            version: "8.0.3".into(),
            tag: "n8.0.3".into(),
            commit: "c".repeat(40),
            archive_url: "https://ffmpeg.org/releases/ffmpeg-8.0.3.tar.xz".into(),
            archive_sha256: "a".repeat(64),
            configuration: "--prefix=<build prefix> --disable-gpl".into(),
            libraries: vec![("libavutil.60.dylib".into(), "b".repeat(64))],
        };
        let helpers = [Helper {
            name: "yt-dlp".into(),
            version: "2026.08.19".into(),
            license: "Unlicense".into(),
            upstream_sha256: "c".repeat(64),
            sha256: "d".repeat(64),
            path: "Contents/Resources/helpers/yt-dlp/2026.08.19/yt-dlp_macos".into(),
            signature: "resigned".into(),
        }];
        let native = [json!({
            "type": "library", "bom-ref": "native:sqlite", "name": "SQLite",
            "version": "3.53.2", "licenses": [{ "expression": "blessing" }],
        })];
        let bom = sbom(
            "0.1.0",
            "2026-10-04T00:00:00Z",
            &crates,
            &ffmpeg,
            &helpers,
            &native,
        );
        assert_eq!(bom["bomFormat"], "CycloneDX");
        assert_eq!(bom["specVersion"], "1.5");
        let components = bom["components"].as_array().unwrap();
        let mut references = BTreeSet::new();
        for component in components {
            assert!(component["name"].is_string() && component["type"].is_string());
            assert!(references.insert(component["bom-ref"].as_str().unwrap().to_owned()));
            for license in component["licenses"].as_array().unwrap() {
                let expression = license["expression"]
                    .as_str()
                    .or_else(|| license["license"]["id"].as_str())
                    .unwrap();
                spdx::parse(expression, &ids).unwrap();
            }
            for hash in component["hashes"].as_array().into_iter().flatten() {
                assert_eq!(hash["alg"], "SHA-256");
                assert_eq!(hash["content"].as_str().unwrap().len(), 64);
            }
        }
        for dependency in bom["dependencies"].as_array().unwrap() {
            assert!(references.contains(dependency["ref"].as_str().unwrap()));
            for target in dependency["dependsOn"].as_array().unwrap() {
                assert!(references.contains(target.as_str().unwrap()), "{target}");
            }
        }
    }

    #[test]
    fn lock_checksums_read_registry_entries_only() {
        let directory = std::env::temp_dir().join(format!("xtask-lock-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let lock = directory.join("Cargo.lock");
        fs::write(
            &lock,
            "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"abc\"\n\n[[package]]\nname = \"local\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let checksums = lock_checksums(&lock).unwrap();
        fs::remove_dir_all(&directory).unwrap();
        assert_eq!(checksums.len(), 1);
        assert_eq!(checksums[&("a".into(), "1.0.0".into())], "abc");
    }
}
