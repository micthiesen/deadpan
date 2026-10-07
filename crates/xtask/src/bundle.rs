//! `cargo xtask bundle`: a self-contained, relocatable `Deadpan.app`.
//!
//! The bundle carries release builds of every executable in `Contents/MacOS`
//! (workers keep resolving beside the running executable), the pinned LGPL
//! FFmpeg libraries in `Contents/Frameworks` with bundle-relative install
//! names, the pinned yt-dlp and Deno helpers as a read-only baseline in
//! `Contents/Resources/helpers`, notices, an SBOM and build provenance. Code is
//! signed inside-out with the hardened runtime: ad hoc by default, or with a
//! Developer ID identity. Notarization runs only with an explicit notary
//! profile. See docs/PACKAGING.md.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub mod ai_runtime;
pub mod deno_notices;
pub mod macho;
pub mod notices;
pub mod spdx;
pub mod verify;

pub type Result<T> = std::result::Result<T, String>;

pub const APP_NAME: &str = "Deadpan.app";
pub const DEFAULT_BUNDLE_ID: &str = "dev.deadpan.Deadpan";
const MAIN_EXECUTABLE: &str = "deadpan-app";
/// Deno Land Inc.'s Developer ID team, whose signature the Deno helper keeps.
const DENO_TEAM: &str = "2H4KBF436B";
const HELPER_MANIFEST_SCHEMA: u32 = 1;

const USAGE: &str = "usage: cargo xtask bundle --output <directory> [--helpers-from <managed-helper-root>] [--identity <signing identity>] [--notary-profile <keychain profile>] [--allow-dirty] [--no-build] [--without-ai-runtime] [--ai-runtime-cache <directory>] [--ltx-checkout <directory>] [--allow-gpl-ai-codec]";

struct Options {
    output: PathBuf,
    helpers_from: Option<PathBuf>,
    identity: Option<String>,
    notary_profile: Option<String>,
    allow_dirty: bool,
    build: bool,
    /// Bundle the private AI runtime (default).
    ai_runtime: bool,
    ai_runtime_cache: Option<PathBuf>,
    ltx_checkout: Option<PathBuf>,
    /// The owner's explicit decision to distribute the runtime's GPL
    /// ffmpeg/ffprobe under a Developer ID (specification §27.2).
    allow_gpl_ai_codec: bool,
}

fn parse(arguments: &[String]) -> Result<Options> {
    let mut options = Options {
        output: PathBuf::new(),
        helpers_from: None,
        identity: None,
        notary_profile: None,
        allow_dirty: false,
        build: true,
        ai_runtime: true,
        ai_runtime_cache: None,
        ltx_checkout: None,
        allow_gpl_ai_codec: false,
    };
    let mut output = None;
    let mut rest = arguments;
    while let Some((flag, tail)) = rest.split_first() {
        let value = || {
            tail.first()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
        };
        match flag.as_str() {
            "--output" => output = Some(PathBuf::from(value()?)),
            "--helpers-from" => options.helpers_from = Some(PathBuf::from(value()?)),
            "--identity" => options.identity = Some(value()?),
            "--notary-profile" => options.notary_profile = Some(value()?),
            "--ai-runtime-cache" => options.ai_runtime_cache = Some(PathBuf::from(value()?)),
            "--ltx-checkout" => options.ltx_checkout = Some(PathBuf::from(value()?)),
            "--without-ai-runtime" | "--allow-gpl-ai-codec" => {
                if flag == "--without-ai-runtime" {
                    options.ai_runtime = false;
                } else {
                    options.allow_gpl_ai_codec = true;
                }
                rest = tail;
                continue;
            }
            "--no-build" => {
                options.build = false;
                rest = tail;
                continue;
            }
            "--allow-dirty" => {
                options.allow_dirty = true;
                rest = tail;
                continue;
            }
            _ => return Err(USAGE.into()),
        }
        rest = &tail[1..];
    }
    options.output = output.ok_or(USAGE)?;
    if options.notary_profile.is_some() && options.identity.is_none() {
        return Err("--notary-profile requires --identity with a Developer ID Application certificate; ad hoc signatures cannot be notarized".into());
    }
    if options.identity.as_deref() == Some("-") {
        return Err("omit --identity for ad hoc signing".into());
    }
    // Only a Developer ID build is a distribution; personal identities (such
    // as the dotfiles local signing certificate) keep the GPL tools in place.
    if options
        .identity
        .as_deref()
        .is_some_and(|identity| identity.contains("Developer ID"))
        && options.ai_runtime
        && !options.allow_gpl_ai_codec
    {
        return Err("the AI runtime's ffmpeg/ffprobe are GPL-2.0-or-later (libx264); a Developer ID build distributes them only with the owner's explicit --allow-gpl-ai-codec, or pass --without-ai-runtime (docs/PACKAGING.md)".into());
    }
    Ok(options)
}

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("xtask lives in the workspace")
}

pub fn output_in(directory: &Path, program: &str, arguments: &[&str]) -> Result<String> {
    let result = Command::new(program)
        .args(arguments)
        .current_dir(directory)
        .output()
        .map_err(|e| format!("failed to start {program}: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "{program} {} failed ({}): {}",
            arguments.join(" "),
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    String::from_utf8(result.stdout).map_err(|e| e.to_string())
}

pub fn output<S: AsRef<OsStr>>(program: &str, arguments: &[S]) -> Result<String> {
    let result = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|e| format!("failed to start {program}: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "{program} failed ({}): {}{}",
            result.status,
            String::from_utf8_lossy(&result.stdout).trim(),
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    String::from_utf8(result.stdout).map_err(|e| e.to_string())
}

/// Run a tool whose combined output is only diagnostic.
pub fn run_tool<S: AsRef<OsStr>>(program: &str, arguments: &[S]) -> Result<()> {
    let result = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|e| format!("failed to start {program}: {e}"))?;
    if result.status.success() {
        Ok(())
    } else {
        let shown: Vec<String> = arguments
            .iter()
            .map(|a| a.as_ref().to_string_lossy().into_owned())
            .collect();
        Err(format!(
            "{program} {} failed ({}): {}{}",
            shown.join(" "),
            result.status,
            String::from_utf8_lossy(&result.stdout).trim(),
            String::from_utf8_lossy(&result.stderr).trim()
        ))
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|e| format!("chmod {}: {e}", path.display()))
}

fn copy(source: &Path, destination: &Path, mode: u32) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(source, destination).map_err(|e| format!("copy {}: {e}", source.display()))?;
    set_mode(destination, mode)
}

/// RFC 3339 UTC time for `seconds` since the Unix epoch.
fn rfc3339(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest / 60 % 60,
        rest % 60
    )
}

fn now() -> String {
    let seconds = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs())
        });
    rfc3339(seconds)
}

struct Signer<'a> {
    identity: Option<&'a str>,
}

impl Signer<'_> {
    fn sign(
        &self,
        path: &Path,
        identifier: Option<&str>,
        entitlements: Option<&Path>,
    ) -> Result<()> {
        let mut arguments: Vec<&OsStr> =
            vec!["--force".as_ref(), "--options".as_ref(), "runtime".as_ref()];
        arguments.push(if self.identity.is_some() {
            "--timestamp".as_ref()
        } else {
            "--timestamp=none".as_ref()
        });
        arguments.extend([OsStr::new("-s"), OsStr::new(self.identity.unwrap_or("-"))]);
        if let Some(identifier) = identifier {
            arguments.extend([OsStr::new("-i"), OsStr::new(identifier)]);
        }
        if let Some(entitlements) = entitlements {
            arguments.extend(["--entitlements".as_ref(), entitlements.as_os_str()]);
        }
        arguments.push(path.as_os_str());
        run_tool("codesign", &arguments)
    }

    fn kind(&self) -> &'static str {
        if self.identity.is_some() {
            "developer-id"
        } else {
            "ad-hoc"
        }
    }
}

/// `codesign -dv` fields for one file.
fn signature_details(path: &Path) -> Result<String> {
    let result = Command::new("codesign")
        .args(["-dv", "--verbose=2"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!("{} is not signed", path.display()));
    }
    Ok(String::from_utf8_lossy(&result.stderr).into_owned())
}

/// The pinned helpers as `downloader status` verified them.
struct SourceHelper {
    name: String,
    version: String,
    license: String,
    path: PathBuf,
    sha256: String,
    bytes: u64,
}

fn verified_helpers(cli: &Path, root: &Path) -> Result<Vec<SourceHelper>> {
    let text = output(
        cli.to_str().ok_or("non-UTF-8 CLI path")?,
        &[
            "downloader",
            "status",
            "--root",
            root.to_str().ok_or("non-UTF-8 helper root")?,
        ],
    )?;
    let report: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut helpers = Vec::new();
    for helper in report["helpers"]
        .as_array()
        .ok_or("downloader status has no helpers")?
    {
        let name = helper["name"].as_str().unwrap_or_default();
        if helper["verified"] != true {
            return Err(format!(
                "{name} under {} is not the verified pinned release ({}); run `deadpan-cli downloader install --root {}` first",
                root.display(),
                helper["problem"].as_str().unwrap_or("not installed"),
                root.display()
            ));
        }
        helpers.push(SourceHelper {
            name: name.into(),
            version: helper["version"].as_str().unwrap_or_default().into(),
            license: helper["license"].as_str().unwrap_or_default().into(),
            path: PathBuf::from(helper["path"].as_str().unwrap_or_default()),
            sha256: helper["pinned_sha256"].as_str().unwrap_or_default().into(),
            bytes: helper["pinned_bytes"].as_u64().unwrap_or_default(),
        });
    }
    if helpers
        .iter()
        .map(|helper| helper.name.as_str())
        .collect::<Vec<_>>()
        != ["yt-dlp", "deno"]
    {
        return Err("downloader status reported an unexpected helper set".into());
    }
    Ok(helpers)
}

/// `#define NAME "value"` or `project(... VERSION value)` from a vendored file.
fn vendored_version(file: &Path, marker: &str) -> Option<String> {
    let text = fs::read_to_string(file).ok()?;
    let line = text.lines().find(|line| line.contains(marker))?;
    let rest = &line[line.find(marker)? + marker.len()..];
    let value: String = rest
        .trim_start_matches([' ', '"'])
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    (!value.is_empty()).then_some(value)
}

fn native_components(workspace: &Path, crates: &[notices::Crate]) -> Result<Vec<Value>> {
    let mut components = Vec::new();
    let find = |name: &str| crates.iter().find(|item| item.name == name);
    if let Some(whisper) = find("whisper-rs-sys") {
        let version = vendored_version(
            &whisper.directory.join("whisper.cpp/CMakeLists.txt"),
            "project(\"whisper.cpp\" VERSION",
        )
        .ok_or("cannot read the vendored whisper.cpp version")?;
        components.push(json!({
            "type": "library", "bom-ref": "native:whisper.cpp", "name": "whisper.cpp",
            "version": version, "licenses": [{ "license": { "id": "MIT" } }],
            "properties": [{ "name": "deadpan:vendored-by", "value": format!("whisper-rs-sys {}", whisper.version) }],
        }));
    }
    if let Some(sqlite) = find("libsqlite3-sys") {
        let version = vendored_version(
            &sqlite.directory.join("sqlite3/sqlite3.h"),
            "#define SQLITE_VERSION ",
        )
        .ok_or("cannot read the bundled SQLite version")?;
        components.push(json!({
            "type": "library", "bom-ref": "native:sqlite", "name": "SQLite",
            "version": version, "licenses": [{ "expression": "blessing" }],
            "properties": [{ "name": "deadpan:vendored-by", "value": format!("libsqlite3-sys {}", sqlite.version) }],
        }));
    }
    let pins: Value = serde_json::from_slice(
        &fs::read(workspace.join("native/deadpan-dsp/vendor/pins.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for (key, name) in [
        ("stretch", "signalsmith-stretch"),
        ("linear", "signalsmith-linear"),
    ] {
        let pin = &pins[key];
        components.push(json!({
            "type": "library", "bom-ref": format!("native:{name}"), "name": name,
            "version": pin["version"], "licenses": [{ "license": { "id": pin["license"] } }],
            "hashes": [{ "alg": "SHA-256", "content": pin["archive_sha256"] }],
            "externalReferences": [{ "type": "distribution", "url": pin["archive_url"] }],
            "properties": [{ "name": "deadpan:revision", "value": pin["revision"] }],
        }));
    }
    // The caption font embedded in deadpan-render.
    let font = workspace.join("assets/brand/source/Inter-Variable.ttf");
    components.push(json!({
        "type": "file", "bom-ref": "font:inter", "name": "Inter",
        "version": "4.001", "licenses": [{ "license": { "id": "OFL-1.1" } }],
        "hashes": [{ "alg": "SHA-256", "content": sha256_file(&font)? }],
        "properties": [
            { "name": "deadpan:embedded-by", "value": "deadpan-render (captions)" },
            { "name": "deadpan:notice", "value": "inter/OFL.txt" },
        ],
    }));
    Ok(components)
}

fn info_plist(contents: &Path, bundle_id: &str, version: &str, build: &str) -> Result<()> {
    let info = json!({
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": "Deadpan",
        "CFBundleExecutable": MAIN_EXECUTABLE,
        "CFBundleIdentifier": bundle_id,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": "Deadpan",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": build,
        "CFBundleIconName": "Deadpan",
        "CFBundleIconFile": "Deadpan.icns",
        "CFBundleSupportedPlatforms": ["MacOSX"],
        "LSApplicationCategoryType": "public.app-category.video",
        "LSMinimumSystemVersion": "15.0",
        "LSArchitecturePriority": ["arm64"],
        "LSRequiresNativeExecution": true,
        "NSHighResolutionCapable": true,
        "NSSupportsAutomaticGraphicsSwitching": true,
        "NSHumanReadableCopyright": "Deadpan is MIT licensed. Third-party notices are in Contents/Resources/Notices.",
        // Marks a packaged bundle for runtime lookup (deadpan_cli::bundle).
        "DeadpanPackaging": "xtask-bundle-1",
        // New projects live in Documents/Deadpan; sources and projects may
        // also be reopened from these protected folders and volumes.
        "NSDocumentsFolderUsageDescription": "Deadpan keeps your projects in Documents/Deadpan.",
        "NSDesktopFolderUsageDescription": "Deadpan opens videos, sounds and projects you keep on the Desktop.",
        "NSDownloadsFolderUsageDescription": "Deadpan opens videos, sounds and projects you keep in Downloads.",
        "NSRemovableVolumesUsageDescription": "Deadpan opens videos, sounds and projects on removable drives.",
    });
    let path = contents.join("Info.plist");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&info).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    run_tool(
        "plutil",
        &["-convert".as_ref(), "xml1".as_ref(), path.as_os_str()],
    )?;
    run_tool("plutil", &["-lint".as_ref(), path.as_os_str()])?;
    fs::write(contents.join("PkgInfo"), b"APPL????").map_err(|e| e.to_string())
}

fn compile_icons(workspace: &Path, resources: &Path, scratch: &Path) -> Result<()> {
    let compiled = scratch.join("icons");
    fs::create_dir_all(&compiled).map_err(|e| e.to_string())?;
    let script = "import sys\nfrom pathlib import Path\nsys.path.insert(0, sys.argv[1])\nfrom brand.native_icons import compile_icons\ncompile_icons(Path(sys.argv[2]), Path(sys.argv[3]))\n";
    run_tool(
        "python3",
        &[
            OsStr::new("-c"),
            OsStr::new(script),
            workspace.join("tools").as_os_str(),
            workspace
                .join("assets/brand/macos/Deadpan.icon")
                .as_os_str(),
            compiled.as_os_str(),
        ],
    )?;
    for name in ["Assets.car", "Deadpan.icns"] {
        copy(&compiled.join(name), &resources.join(name), 0o644)?;
    }
    Ok(())
}

fn git(workspace: &Path, arguments: &[&str]) -> String {
    output_in(workspace, "git", arguments)
        .map(|text| text.trim().to_owned())
        .unwrap_or_default()
}

/// Cargo's configured target directory, from `cargo metadata`.
fn cargo_target_directory(workspace: &Path) -> Result<PathBuf> {
    let text = output_in(
        workspace,
        "cargo",
        &["metadata", "--format-version", "1", "--no-deps", "--locked"],
    )?;
    let metadata: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    metadata["target_directory"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| "cargo metadata has no target directory".into())
}

/// Path remapping for release bundle builds, so binaries carry no checkout,
/// Cargo home or user name. Rust uses `--remap-path-prefix`; C/C++ built by
/// `cc`/CMake uses `-ffile-prefix-map`.
fn remaps(workspace: &Path) -> Vec<(PathBuf, &'static str)> {
    let mut remaps = vec![(workspace.to_owned(), "/deadpan")];
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
    if let Some(cargo_home) = cargo_home {
        remaps.push((cargo_home, "/cargo"));
    }
    remaps
}

/// Release-build the shipped binaries into `target` with path remapping and
/// return each executable as Cargo reports it.
fn build(workspace: &Path, target: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let remaps = remaps(workspace);
    let rust: Vec<String> = remaps
        .iter()
        .map(|(from, to)| format!("--remap-path-prefix={}={to}", from.display()))
        .collect();
    let c: Vec<String> = remaps
        .iter()
        .map(|(from, to)| format!("-ffile-prefix-map={}={to}", from.display()))
        .collect();
    let with_existing = |name: &str| {
        let mut flags = std::env::var(name).unwrap_or_default();
        for flag in &c {
            flags.push(' ');
            flags.push_str(flag);
        }
        flags.trim().to_owned()
    };
    let mut arguments = vec![
        "build".to_owned(),
        "--release".into(),
        "--locked".into(),
        "--bins".into(),
        "--message-format=json-render-diagnostics".into(),
        "--target-dir".into(),
        target.display().to_string(),
    ];
    for package in notices::SHIPPED_PACKAGES {
        arguments.extend(["-p".into(), package.into()]);
    }
    println!("bundle: cargo {}", arguments.join(" "));
    let result = Command::new("cargo")
        .args(&arguments)
        .current_dir(workspace)
        .env("CARGO_ENCODED_RUSTFLAGS", rust.join("\x1f"))
        .env("CFLAGS", with_existing("CFLAGS"))
        .env("CXXFLAGS", with_existing("CXXFLAGS"))
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!("release build failed ({})", result.status));
    }
    let mut executables = BTreeMap::new();
    for line in String::from_utf8_lossy(&result.stdout).lines() {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-artifact" {
            continue;
        }
        if let (Some(name), Some(executable)) = (
            message["target"]["name"].as_str(),
            message["executable"].as_str(),
        ) && notices::SHIPPED_PACKAGES.contains(&name)
        {
            executables.insert(name.to_owned(), PathBuf::from(executable));
        }
    }
    Ok(executables)
}

/// The FFmpeg configure line without build-host paths; every flag is kept.
pub fn public_configuration(configuration: &str) -> String {
    configuration
        .split(' ')
        .map(|token| {
            if token.starts_with("--prefix=") {
                "--prefix=<build prefix>".to_owned()
            } else if token.starts_with("--sysroot=") {
                "--sysroot=<macOS SDK>".to_owned()
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Source state recorded in provenance.
struct SourceState {
    commit: String,
    changes: bool,
}

pub fn run(arguments: &[String]) -> Result<()> {
    let options = parse(arguments)?;
    let workspace = workspace_root();
    let prefix = std::env::var_os("DEADPAN_FFMPEG_PREFIX")
        .map(PathBuf::from)
        .filter(|prefix| prefix.is_absolute())
        .ok_or(
            "DEADPAN_FFMPEG_PREFIX must name the absolute pinned FFmpeg prefix (build time only)",
        )?;
    let prefix_lib = prefix.join("lib");
    let helpers_root = match &options.helpers_from {
        Some(root) => root.clone(),
        None => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?)
            .join("Library/Application Support/Deadpan/helpers"),
    };
    if !helpers_root.is_absolute() {
        return Err("--helpers-from must be absolute".into());
    }
    let source = SourceState {
        commit: git(&workspace, &["rev-parse", "HEAD"]),
        changes: !git(&workspace, &["status", "--porcelain"]).is_empty(),
    };
    if source.changes {
        if options.identity.is_some() && !options.allow_dirty {
            return Err("refusing to sign a release from a working tree with uncommitted or untracked changes; commit them or pass --allow-dirty".into());
        }
        println!(
            "bundle: warning: the working tree has uncommitted or untracked changes; build-provenance.json records this"
        );
    }
    fs::create_dir_all(&options.output).map_err(|e| e.to_string())?;
    let output_directory = fs::canonicalize(&options.output).map_err(|e| e.to_string())?;
    let app = output_directory.join(APP_NAME);
    if app.exists() || app.is_symlink() {
        return Err(format!(
            "{} already exists; choose a new --output",
            app.display()
        ));
    }

    let target = cargo_target_directory(&workspace)?.join("bundle");
    let executables = if options.build {
        build(&workspace, &target)?
    } else {
        notices::SHIPPED_PACKAGES
            .iter()
            .map(|name| ((*name).to_owned(), target.join("release").join(name)))
            .collect()
    };

    let ai = if options.ai_runtime {
        let cache = match &options.ai_runtime_cache {
            Some(cache) => cache.clone(),
            None => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?)
                .join("Library/Caches/Deadpan/build-inputs"),
        };
        let started = std::time::Instant::now();
        let built = ai_runtime::build(&workspace, &cache, options.ltx_checkout.as_deref())?;
        println!(
            "bundle: AI runtime {} ({:.0} MiB) ready in {:.1} s",
            built.runtime.display(),
            built.report["bytes"].as_f64().unwrap_or(0.0) / 1_048_576.0,
            started.elapsed().as_secs_f64()
        );
        Some(built)
    } else {
        None
    };

    let staging = output_directory.join(format!(".{APP_NAME}.staging-{}", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    let staged_app = staging.join(APP_NAME);
    // Nothing is published unless assembly, verification and any requested
    // notarization all succeed; failures leave only the removed staging area.
    let outcome = assemble(
        &options,
        &workspace,
        &prefix,
        &prefix_lib,
        &helpers_root,
        &executables,
        &staging,
        &source,
        ai.as_ref(),
    )
    .and_then(|summary| {
        if let (Some(identity), Some(profile)) = (&options.identity, &options.notary_profile) {
            notarize(&staged_app, &staging, identity, profile)?;
        }
        fs::rename(&staged_app, &app).map_err(|e| format!("publish {}: {e}", app.display()))?;
        Ok(summary)
    });
    let _ = fs::remove_dir_all(&staging);
    let summary = outcome?;

    // Checksums of the final (stapled) bundle and a release record, outside it.
    let mut sums = String::new();
    for file in macho::bundle_files(&app)? {
        let relative = file.strip_prefix(&output_directory).unwrap_or(&file);
        sums.push_str(&format!(
            "{}  {}\n",
            sha256_file(&file)?,
            relative.display()
        ));
    }
    fs::write(output_directory.join("Deadpan.app.SHA256SUMS"), sums).map_err(|e| e.to_string())?;
    fs::copy(
        app.join("Contents/Resources/Notices/sbom.cdx.json"),
        output_directory.join("Deadpan.sbom.cdx.json"),
    )
    .map_err(|e| e.to_string())?;
    let notarized = options.notary_profile.is_some();
    fs::write(
        output_directory.join("Deadpan.release.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": 1,
            "git_commit": source.commit,
            "git_changes": source.changes,
            "signature": if options.identity.is_some() { "developer-id" } else { "ad-hoc" },
            "notarized": notarized,
            "stapled": notarized,
            "checksums": "Deadpan.app.SHA256SUMS",
            "sbom": "Deadpan.sbom.cdx.json",
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let size = output("du", &["-sk", app.to_str().ok_or("non-UTF-8 path")?])?;
    let kib: u64 = size
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    println!("{summary}");
    println!(
        "bundle: {} ({:.1} MiB on disk)",
        app.display(),
        kib as f64 / 1024.0
    );
    if !notarized {
        println!(
            "bundle: not notarized. Deadpan is a personal app and needs no notarization; sign with --identity (e.g. the dotfiles local signing certificate) when a stable signature is needed. See docs/PACKAGING.md"
        );
    }
    Ok(())
}

fn strip_attributes(app: &Path) -> Result<()> {
    run_tool("xattr", &["-cr".as_ref(), app.as_os_str()])
}

/// Run the staged bundle's own CLI, as a packaged app, against its baseline.
fn check_staged_helpers(app: &Path, staging: &Path) -> Result<()> {
    let home = staging.join("home");
    fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let result = Command::new(app.join("Contents/MacOS/deadpan-cli"))
        .args(["downloader", "status"])
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(&home)
        .output()
        .map_err(|e| e.to_string())?;
    let report: Value = serde_json::from_slice(&result.stdout)
        .map_err(|e| format!("staged downloader status: {e}"))?;
    let verified = report["source"] == "bundled"
        && report["helpers"]
            .as_array()
            .is_some_and(|helpers| helpers.iter().all(|helper| helper["verified"] == true));
    if verified {
        Ok(())
    } else {
        Err(format!(
            "the staged bundle's helpers do not verify: {report}"
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    options: &Options,
    workspace: &Path,
    prefix: &Path,
    prefix_lib: &Path,
    helpers_root: &Path,
    built: &BTreeMap<String, PathBuf>,
    staging: &Path,
    source: &SourceState,
    ai: Option<&ai_runtime::Built>,
) -> Result<String> {
    let app = staging.join(APP_NAME);
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let frameworks = contents.join("Frameworks");
    let resources = contents.join("Resources");
    let helpers_directory = resources.join("helpers");
    let notices_directory = resources.join("Notices");
    for directory in [&macos, &frameworks, &helpers_directory, &notices_directory] {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    }
    let signer = Signer {
        identity: options.identity.as_deref(),
    };

    // Executables.
    let mut executables = Vec::new();
    for name in notices::SHIPPED_PACKAGES {
        let source = built
            .get(name)
            .filter(|path| path.is_file())
            .ok_or_else(|| format!("no built {name}; build without --no-build"))?;
        let destination = macos.join(name);
        copy(source, &destination, 0o755)?;
        executables.push(destination);
    }

    // FFmpeg libraries.
    let libraries = macho::bundle_libraries(&executables, &frameworks, prefix_lib)?;
    println!(
        "bundle: {} FFmpeg libraries -> Contents/Frameworks",
        libraries.len()
    );

    // Downloader helpers, verified by the CLI against the compiled pins.
    let source_helpers = verified_helpers(&built["deadpan-cli"], helpers_root)?;
    for helper in &source_helpers {
        let destination = helpers_directory
            .join(&helper.name)
            .join(&helper.version)
            .join(
                helper
                    .path
                    .file_name()
                    .ok_or("helper without a file name")?,
            );
        copy(&helper.path, &destination, 0o755)?;
        if sha256_file(&destination)? != helper.sha256
            || fs::metadata(&destination).map_err(|e| e.to_string())?.len() != helper.bytes
        {
            return Err(format!("copied {} does not match its pin", helper.name));
        }
    }
    // The private AI runtime, assembled from pinned inputs.
    let ai_directory = match ai {
        Some(built) => Some(ai_runtime::install(built, &contents)?),
        None => None,
    };
    // Copies carry no quarantine, provenance or Finder metadata.
    strip_attributes(&app)?;

    let mut shipped_helpers = Vec::new();
    let mut manifest_entries = Vec::new();
    for helper in &source_helpers {
        let file_name = helper
            .path
            .file_name()
            .ok_or("helper without a file name")?;
        let relative = Path::new(&helper.name)
            .join(&helper.version)
            .join(file_name);
        let destination = helpers_directory.join(&relative);
        let signature = if helper.name == "deno" {
            // Keep Deno's own Developer ID signature, hardened runtime and
            // entitlements (JIT); the application gains no exception for it.
            run_tool(
                "codesign",
                &[
                    "--verify".as_ref(),
                    "--strict".as_ref(),
                    destination.as_os_str(),
                ],
            )?;
            let details = signature_details(&destination)?;
            if !details.contains(&format!("TeamIdentifier={DENO_TEAM}"))
                || !details.contains("runtime")
            {
                return Err(
                    "the pinned Deno executable lacks Deno Land's hardened Developer ID signature"
                        .into(),
                );
            }
            "upstream"
        } else {
            // Remove the upstream ad hoc signature first so the new one is
            // freshly allocated with zero padding; stale signature bytes
            // would otherwise remain after the new superblob.
            run_tool(
                "codesign",
                &["--remove-signature".as_ref(), destination.as_os_str()],
            )?;
            // No entitlements: measured unnecessary (docs/PACKAGING.md).
            signer.sign(
                &destination,
                Some(&format!("{DEFAULT_BUNDLE_ID}.{}", helper.name)),
                None,
            )?;
            "resigned"
        };
        let shipped = sha256_file(&destination)?;
        let bytes = fs::metadata(&destination).map_err(|e| e.to_string())?.len();
        manifest_entries.push(json!({
            "name": helper.name,
            "version": helper.version,
            "executable": file_name.to_string_lossy(),
            "upstream_sha256": helper.sha256,
            "upstream_bytes": helper.bytes,
            "sha256": shipped,
            "bytes": bytes,
            "signature": signature,
        }));
        shipped_helpers.push(notices::Helper {
            name: helper.name.clone(),
            version: helper.version.clone(),
            license: helper.license.clone(),
            upstream_sha256: helper.sha256.clone(),
            sha256: shipped,
            path: Path::new("Contents/Resources/helpers")
                .join(&relative)
                .display()
                .to_string(),
            signature: signature.into(),
        });
    }
    fs::write(
        helpers_directory.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": HELPER_MANIFEST_SCHEMA,
            "helpers": manifest_entries,
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // Nested code, inside-out: libraries, then auxiliary executables. The main
    // executable is signed with the bundle, which seals every resource.
    for library in &libraries {
        signer.sign(library, None, None)?;
    }
    let ai_code = match &ai_directory {
        Some(directory) => {
            // No entitlements: measured unnecessary for MLX/Metal and Python
            // under the hardened runtime (docs/PACKAGING.md).
            let signed = ai_runtime::sign(directory, &|path, identifier| {
                signer.sign(path, identifier, None)
            })?;
            println!(
                "bundle: AI runtime signed ({} libraries, {} executables)",
                signed.0, signed.1
            );
            Some(signed)
        }
        None => None,
    };
    for executable in executables
        .iter()
        .filter(|path| !path.ends_with(MAIN_EXECUTABLE))
    {
        let name = executable.file_name().unwrap().to_string_lossy();
        signer.sign(
            executable,
            Some(&format!("{DEFAULT_BUNDLE_ID}.{name}")),
            None,
        )?;
    }

    // Notices, SBOM and provenance.
    let crates = notices::shipped_crates(workspace)?;
    let vendored = notices::vendored_notices(workspace)?;
    let ffmpeg_pins: Value = serde_json::from_slice(
        &fs::read(workspace.join("tools/media-qualification/compatible/pins.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let version_text = output(
        prefix
            .join("bin/ffprobe")
            .to_str()
            .ok_or("non-UTF-8 prefix")?,
        &["-version"],
    )?;
    let pin = &ffmpeg_pins["ffmpeg"];
    let ffmpeg_version = pin["version"].as_str().unwrap_or_default().to_owned();
    if !version_text.starts_with(&format!("ffprobe version {ffmpeg_version} ")) {
        return Err(format!(
            "the FFmpeg prefix is not the pinned {ffmpeg_version}"
        ));
    }
    let configuration = version_text
        .lines()
        .find_map(|line| line.strip_prefix("configuration: "))
        .unwrap_or_default()
        .to_owned();
    for forbidden in ["--enable-gpl", "--enable-nonfree", "--enable-version3"] {
        if configuration.contains(forbidden) {
            return Err(format!("the FFmpeg prefix was configured with {forbidden}"));
        }
    }
    let configuration = public_configuration(&configuration);
    let ffmpeg = notices::Ffmpeg {
        version: ffmpeg_version,
        tag: pin["tag"].as_str().unwrap_or_default().into(),
        commit: pin["commit"].as_str().unwrap_or_default().into(),
        archive_url: pin["archive_url"].as_str().unwrap_or_default().into(),
        archive_sha256: pin["archive_sha256"].as_str().unwrap_or_default().into(),
        configuration: configuration.clone(),
        libraries: libraries
            .iter()
            .map(|library| {
                Ok((
                    library.file_name().unwrap().to_string_lossy().into_owned(),
                    sha256_file(library)?,
                ))
            })
            .collect::<Result<_>>()?,
    };
    let missing = notices::write_notices(
        &notices_directory,
        workspace,
        &crates,
        &vendored,
        &ffmpeg,
        &shipped_helpers,
    )?;
    if let Some(built) = ai {
        ai_runtime::write_notices(built, workspace, &notices_directory)?;
    }
    // Aggregated notices for everything statically linked into Deno.
    let identifiers = notices::spdx_identifiers(workspace)?;
    let deno_helper = shipped_helpers
        .iter()
        .find(|helper| helper.name == "deno")
        .ok_or("no Deno helper was bundled")?;
    let notice_sets = workspace.join("packaging/notices");
    let deno = deno_notices::DenoNotices::load(
        &deno_notices::set_directory(&notice_sets, &deno_helper.version)?,
        &identifiers,
    )?;
    if deno.version != deno_helper.version || deno.executable_sha256 != deno_helper.upstream_sha256
    {
        return Err(format!(
            "the vendored Deno notice set ({} {}) does not describe the bundled Deno {} {}",
            deno.version, deno.executable_sha256, deno_helper.version, deno_helper.upstream_sha256
        ));
    }
    deno.install(&notices_directory)?;
    let version = crates
        .iter()
        .find(|item| item.name == MAIN_EXECUTABLE)
        .map(|item| item.version.clone())
        .unwrap_or_default();
    let timestamp = now();
    let mut native = native_components(workspace, &crates)?;
    if let (Some(built), Some(directory)) = (ai, &ai_directory) {
        native.extend(ai_runtime::sbom_components(built, directory)?);
    }
    let mut sbom = notices::sbom(
        &version,
        &timestamp,
        &crates,
        &ffmpeg,
        &shipped_helpers,
        &native,
    );
    deno.attach_to_sbom(&mut sbom, &identifiers)?;
    fs::write(
        notices_directory.join("sbom.cdx.json"),
        serde_json::to_vec_pretty(&sbom).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // Sealed inside the signature, so it records intent, not the later
    // notarization outcome; Deadpan.release.json beside the bundle does.
    let provenance = json!({
        "schema": 2,
        "built_at": timestamp,
        "source_date_epoch": std::env::var("SOURCE_DATE_EPOCH").ok(),
        "version": version,
        "git_commit": source.commit,
        "git_changes": source.changes,
        "rustc": output("rustc", &["-vV"]).unwrap_or_default().trim(),
        "xcode": output("xcodebuild", &["-version"]).unwrap_or_default().trim(),
        "macos_sdk": output("xcrun", &["--show-sdk-version"]).unwrap_or_default().trim(),
        "deployment_target": "15.0",
        "profile": "release",
        "path_remapping": options.build.then(|| remaps(workspace).iter().map(|(_, to)| *to).collect::<Vec<_>>()),
        "ffmpeg_configuration": configuration,
        "signature": signer.kind(),
        "notarization": if options.notary_profile.is_some() { "requested" } else { "none" },
        "ai_runtime": ai.map(|built| json!({
            "runtime_id": built.report["runtime_id"],
            "runtime_version": built.report["runtime_version"],
            "cache_key": built.report["cache_key"],
            "python": built.report["python"]["version"],
            "wheels": built.report["wheels"].as_array().map_or(0, Vec::len),
            "ltx_commit": built.report["ltx_source"]["commit"],
            "ffmpeg_configuration": built.report["ffmpeg"]["configuration"],
            "gpl_programs": ["Contents/Resources/ai-runtime/bin/ffmpeg", "Contents/Resources/ai-runtime/bin/ffprobe"],
            "gpl_distribution_approved": options.allow_gpl_ai_codec,
        })),
        "limitations": [
            "Not notarized unless Deadpan.release.json beside the bundle says so.",
            if ai.is_some() {
                "Model weights are not bundled; they install as verified model packs. The AI runtime's ffmpeg/ffprobe are GPL-2.0-or-later (libx264)."
            } else {
                "Built without the AI runtime; AI pauses are unavailable."
            },
            "Application updates use complete verified bundles with the previous build retained for rollback. Helpers and model packs use independent signed manifests and rollback.",
        ],
    });
    fs::write(
        resources.join("build-provenance.json"),
        serde_json::to_vec_pretty(&provenance).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    info_plist(&contents, DEFAULT_BUNDLE_ID, &version, &version)?;
    compile_icons(workspace, &resources, staging)?;

    // The bundle: signs the main executable and seals resources.
    strip_attributes(&app)?;
    signer.sign(&app, Some(DEFAULT_BUNDLE_ID), None)?;
    run_tool(
        "codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            "--verbose=2".as_ref(),
            app.as_os_str(),
        ],
    )?;
    let markers = build_markers(prefix);
    let audit = macho::audit(&app, &markers)?;
    if !audit.problems.is_empty() {
        return Err(format!(
            "bundle audit failed:\n  {}",
            audit.problems.join("\n  ")
        ));
    }
    let deno_problems = deno_notices::audit_bundle(&app, &notice_sets, &identifiers);
    if !deno_problems.is_empty() {
        return Err(format!(
            "Deno notice audit failed:\n  {}",
            deno_problems.join("\n  ")
        ));
    }
    check_staged_helpers(&app, staging)?;
    let ai_check = match &ai_directory {
        Some(directory) => {
            let checked = ai_runtime::check_staged(directory, staging)?;
            println!("bundle: staged AI runtime runs: {checked}");
            format!(
                ", AI runtime {} libraries and {} executables",
                ai_code.map_or(0, |code| code.0),
                ai_code.map_or(0, |code| code.1)
            )
        }
        None => ", no AI runtime".into(),
    };
    Ok(format!(
        "bundle: {} Mach-O files audited, {} crates in notices ({} without a license file), signature {}{ai_check}",
        audit.images.len(),
        crates.len(),
        missing.len(),
        signer.kind()
    ))
}

/// Build-host locations that must never be load references.
pub fn build_markers(prefix: &Path) -> Vec<String> {
    let mut markers = vec![
        prefix.display().to_string(),
        "/opt/homebrew".into(),
        "/usr/local/".into(),
    ];
    if let Ok(canonical) = fs::canonicalize(prefix) {
        markers.push(canonical.display().to_string());
    }
    if let Some(home) = std::env::var_os("HOME") {
        markers.push(PathBuf::from(home).display().to_string());
    }
    markers.sort();
    markers.dedup();
    markers
}

/// Developer ID distribution: notarize, staple and assess the staged bundle.
/// Runs only when both `--identity` and `--notary-profile` are given; the
/// caller publishes only after this succeeds.
fn notarize(app: &Path, staging: &Path, identity: &str, profile: &str) -> Result<()> {
    let details = signature_details(app)?;
    if !details.contains("Authority=Developer ID Application") {
        return Err(format!(
            "{identity} did not produce a Developer ID Application signature"
        ));
    }
    let archive = staging.join("Deadpan-notarize.zip");
    run_tool(
        "ditto",
        &[
            "-c".as_ref(),
            "-k".as_ref(),
            "--keepParent".as_ref(),
            app.as_os_str(),
            archive.as_os_str(),
        ],
    )?;
    run_tool(
        "xcrun",
        &[
            "notarytool".as_ref(),
            "submit".as_ref(),
            archive.as_os_str(),
            "--keychain-profile".as_ref(),
            profile.as_ref(),
            "--wait".as_ref(),
        ],
    )?;
    run_tool(
        "xcrun",
        &["stapler".as_ref(), "staple".as_ref(), app.as_os_str()],
    )?;
    run_tool(
        "xcrun",
        &["stapler".as_ref(), "validate".as_ref(), app.as_os_str()],
    )?;
    run_tool(
        "spctl",
        &[
            "--assess".as_ref(),
            "--type".as_ref(),
            "execute".as_ref(),
            "-vv".as_ref(),
            app.as_os_str(),
        ],
    )
}

/// `cargo xtask bundle-audit <Deadpan.app>`
pub fn audit_command(arguments: &[String]) -> Result<()> {
    let [app] = arguments else {
        return Err("usage: cargo xtask bundle-audit <Deadpan.app>".into());
    };
    let markers: Vec<String> = std::env::var_os("DEADPAN_FFMPEG_PREFIX")
        .map(PathBuf::from)
        .map(|prefix| build_markers(&prefix))
        .unwrap_or_else(|| vec!["/opt/homebrew".into(), "/usr/local/".into()]);
    let audit = macho::audit(Path::new(app), &markers)?;
    print_audit(&audit);
    let workspace = workspace_root();
    let deno = deno_notices::audit_bundle(
        Path::new(app),
        &workspace.join("packaging/notices"),
        &notices::spdx_identifiers(&workspace)?,
    );
    for problem in &deno {
        println!("audit: PROBLEM: Deno notices: {problem}");
    }
    if deno.is_empty() {
        println!("audit: Deno notices match the vendored notice set");
    }
    match (audit.problems.len(), deno.len()) {
        (0, 0) => Ok(()),
        (load, notices) => Err(format!(
            "{load} load-reference problems, {notices} Deno notice problems"
        )),
    }
}

pub fn print_audit(audit: &macho::Audit) {
    for image in &audit.images {
        println!(
            "audit: {} - {} dependencies; bundled: {}",
            image.path.display(),
            image.dependencies,
            if image.bundled.is_empty() {
                "none".into()
            } else {
                image.bundled.join(", ")
            }
        );
    }
    for (path, marker, count) in &audit.embedded_build_paths {
        println!(
            "audit: info: {} embeds {count} non-load string(s) containing {marker}",
            path.display()
        );
    }
    for problem in &audit.problems {
        println!("audit: PROBLEM: {problem}");
    }
    println!(
        "audit: {} Mach-O files, {} problems",
        audit.images.len(),
        audit.problems.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_configuration_drops_only_host_paths() {
        let line = "--prefix=/private/tmp/x/prefix --enable-shared --sysroot=/Applications/Xcode.app/SDKs/MacOSX.sdk --extra-cflags='-arch arm64 -mmacosx-version-min=15.0'";
        assert_eq!(
            public_configuration(line),
            "--prefix=<build prefix> --enable-shared --sysroot=<macOS SDK> --extra-cflags='-arch arm64 -mmacosx-version-min=15.0'"
        );
    }

    #[test]
    fn rfc3339_matches_known_instants() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_791_072_000), "2026-10-04T00:00:00Z");
    }

    #[test]
    fn notarization_requires_an_identity() {
        let arguments: Vec<String> = ["--output", "/tmp/x", "--notary-profile", "p"]
            .map(String::from)
            .into();
        assert!(parse(&arguments).is_err());
        let arguments: Vec<String> = ["--output", "/tmp/x", "--no-build"]
            .map(String::from)
            .into();
        let options = parse(&arguments).unwrap();
        assert!(!options.build && options.identity.is_none());
        assert!(parse(&["--bogus".to_owned()]).is_err());
    }

    #[test]
    fn vendored_versions_read_defines_and_cmake() {
        let directory = std::env::temp_dir().join(format!("xtask-version-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let header = directory.join("sqlite3.h");
        fs::write(&header, "#define SQLITE_VERSION        \"3.50.4\"\n").unwrap();
        let cmake = directory.join("CMakeLists.txt");
        fs::write(
            &cmake,
            "cmake_minimum_required(VERSION 3.5)\nproject(\"whisper.cpp\" VERSION 1.8.3)\n",
        )
        .unwrap();
        assert_eq!(
            vendored_version(&header, "#define SQLITE_VERSION ").as_deref(),
            Some("3.50.4")
        );
        assert_eq!(
            vendored_version(&cmake, "project(\"whisper.cpp\" VERSION").as_deref(),
            Some("1.8.3")
        );
        fs::remove_dir_all(&directory).unwrap();
    }
}
