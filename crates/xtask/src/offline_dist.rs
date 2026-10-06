//! `cargo xtask offline-dist`: a full offline distribution (specification
//! §14.1) built from an existing `cargo xtask bundle` output.
//!
//! The distribution is a new folder holding a `ditto` copy of `Deadpan.app`
//! (signature intact and re-verified), each requested approved model pack as
//! the pax archive written by the copied app's own `deadpan-cli models
//! export`, the pack licenses, a `distribution.json` manifest, `README.txt`
//! and `SHA256SUMS`. It is assembled in a hidden sibling directory and
//! published by rename, so an existing folder is never replaced and a failure
//! publishes nothing.
//!
//! `cargo xtask offline-dist-verify` checks the manifest and checksums,
//! verifies the app signature and imports every pack with the distribution's
//! own CLI into a fresh root, from a scrubbed environment. See
//! docs/PACKAGING.md.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::{Value, json};

use crate::bundle::{APP_NAME, Result, run_tool, sha256_file};

pub const MANIFEST: &str = "distribution.json";
pub const SUMS: &str = "SHA256SUMS";
pub const README: &str = "README.txt";
const SCHEMA: u64 = 1;
const KIND: &str = "deadpan-offline-distribution";
/// Files `cargo xtask bundle` writes beside the app, carried when present.
const BUNDLE_SIDECARS: [&str; 3] = [
    "Deadpan.app.SHA256SUMS",
    "Deadpan.sbom.cdx.json",
    "Deadpan.release.json",
];
const BUNDLE_SUMS: &str = "Deadpan.app.SHA256SUMS";
/// The pack operation whose smoke test needs the bundled AI runtime.
const AI_RUNTIME_OPERATION: &str = "bridge_hold";

const USAGE: &str = "usage: cargo xtask offline-dist --app <Deadpan.app> --output <new directory> --pack <id>[=<models root>]... [--models-root <directory>]";
const VERIFY_USAGE: &str =
    "usage: cargo xtask offline-dist-verify <distribution directory> [--keep]";

/// One license of an included pack, as the app's compiled manifest states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct License {
    pub id: String,
    pub spdx: Option<String>,
    pub title: String,
    pub acceptance_required: bool,
    pub redistribution: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    pub id: String,
    pub version: String,
    pub title: String,
    /// Distribution-relative path of the pax archive.
    pub archive: String,
    pub bytes: u64,
    pub sha256: String,
    /// The pack's installed size (sum of its files).
    pub pack_bytes: u64,
    pub licenses: Vec<License>,
    /// Distribution-relative path of the license texts.
    pub license_file: String,
    pub requires_ai_runtime: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Distribution {
    pub app_version: String,
    pub git_commit: String,
    pub git_changes: bool,
    pub signature: String,
    pub ai_runtime: bool,
    pub created_unix: u64,
    /// Sidecars copied from the bundle output, in [`BUNDLE_SIDECARS`] order.
    pub sidecars: Vec<String>,
    pub packs: Vec<Pack>,
}

impl Distribution {
    pub fn to_json(&self) -> Value {
        json!({
            "schema": SCHEMA,
            "kind": KIND,
            "created_unix": self.created_unix,
            "app": {
                "path": APP_NAME,
                "version": self.app_version,
                "git_commit": self.git_commit,
                "git_changes": self.git_changes,
                "signature": self.signature,
                "ai_runtime": self.ai_runtime,
            },
            "bundle_files": self.sidecars,
            "readme": README,
            "checksums": SUMS,
            "packs": self.packs.iter().map(|pack| json!({
                "pack_id": pack.id,
                "pack_version": pack.version,
                "title": pack.title,
                "archive": pack.archive,
                "bytes": pack.bytes,
                "sha256": pack.sha256,
                "pack_bytes": pack.pack_bytes,
                "requires_ai_runtime": pack.requires_ai_runtime,
                "license_file": pack.license_file,
                "licenses": pack.licenses.iter().map(|license| json!({
                    "id": license.id,
                    "spdx": license.spdx,
                    "title": license.title,
                    "acceptance_required": license.acceptance_required,
                    "redistribution": license.redistribution,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(value: &Value) -> Result<Self> {
        if value["schema"].as_u64() != Some(SCHEMA) || value["kind"] != KIND {
            return Err(format!(
                "{MANIFEST} is not a schema-{SCHEMA} {KIND} manifest"
            ));
        }
        let app = &value["app"];
        if app["path"] != APP_NAME {
            return Err(format!("{MANIFEST}: app.path must be {APP_NAME}"));
        }
        let sidecars = array(&value["bundle_files"], "bundle_files")?
            .iter()
            .map(|name| {
                let name = text(name, "bundle_files[]")?;
                if BUNDLE_SIDECARS.contains(&name.as_str()) {
                    Ok(name)
                } else {
                    Err(format!("{MANIFEST}: unexpected bundle file {name}"))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let packs = array(&value["packs"], "packs")?
            .iter()
            .map(|pack| {
                let licenses = array(&pack["licenses"], "licenses")?
                    .iter()
                    .map(|license| {
                        Ok(License {
                            id: text(&license["id"], "license id")?,
                            spdx: license["spdx"].as_str().map(str::to_owned),
                            title: text(&license["title"], "license title")?,
                            acceptance_required: flag(
                                &license["acceptance_required"],
                                "acceptance_required",
                            )?,
                            redistribution: flag(&license["redistribution"], "redistribution")?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let parsed = Pack {
                    id: text(&pack["pack_id"], "pack_id")?,
                    version: text(&pack["pack_version"], "pack_version")?,
                    title: text(&pack["title"], "title")?,
                    archive: relative(&text(&pack["archive"], "archive")?)?,
                    bytes: number(&pack["bytes"], "bytes")?,
                    sha256: hex(&text(&pack["sha256"], "sha256")?)?,
                    pack_bytes: number(&pack["pack_bytes"], "pack_bytes")?,
                    licenses,
                    license_file: relative(&text(&pack["license_file"], "license_file")?)?,
                    requires_ai_runtime: flag(&pack["requires_ai_runtime"], "requires_ai_runtime")?,
                };
                if parsed.archive != archive_name(&parsed.id, &parsed.version) {
                    return Err(format!(
                        "{MANIFEST}: pack {} archive must be {}",
                        parsed.id,
                        archive_name(&parsed.id, &parsed.version)
                    ));
                }
                Ok(parsed)
            })
            .collect::<Result<Vec<_>>>()?;
        if packs.is_empty() {
            return Err(format!("{MANIFEST} lists no packs"));
        }
        Ok(Self {
            app_version: text(&app["version"], "app.version")?,
            git_commit: text(&app["git_commit"], "app.git_commit")?,
            git_changes: flag(&app["git_changes"], "app.git_changes")?,
            signature: text(&app["signature"], "app.signature")?,
            ai_runtime: flag(&app["ai_runtime"], "app.ai_runtime")?,
            created_unix: number(&value["created_unix"], "created_unix")?,
            sidecars,
            packs,
        })
    }
}

fn text(value: &Value, field: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{MANIFEST}: {field} must be a string"))
}

fn flag(value: &Value, field: &str) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| format!("{MANIFEST}: {field} must be a boolean"))
}

fn number(value: &Value, field: &str) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| format!("{MANIFEST}: {field} must be an unsigned integer"))
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| format!("{MANIFEST}: {field} must be an array"))
}

fn hex(value: &str) -> Result<String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value.to_owned())
    } else {
        Err(format!("not a lowercase SHA-256: {value:?}"))
    }
}

/// A plain relative path with no `..`, root or `.` components.
fn relative(path: &str) -> Result<String> {
    let valid = !path.is_empty()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if valid {
        Ok(path.to_owned())
    } else {
        Err(format!("not a contained relative path: {path:?}"))
    }
}

pub fn archive_name(id: &str, version: &str) -> String {
    format!("Packs/{id}-{version}.tar")
}

fn license_name(id: &str, version: &str) -> String {
    format!("Licenses/{id}-{version}.txt")
}

/// `sha256sum`-format lines for `files` (relative to `directory`), in order.
pub fn write_sums(directory: &Path, files: &[String], destination: &Path) -> Result<()> {
    let mut sums = String::new();
    for file in files {
        sums.push_str(&format!(
            "{}  {}\n",
            sha256_file(&directory.join(relative(file)?))?,
            file
        ));
    }
    fs::write(destination, sums).map_err(|e| format!("{}: {e}", destination.display()))
}

/// Parse `sha256sum` text into (hash, contained relative path) pairs.
pub fn parse_sums(text: &str) -> Result<Vec<(String, String)>> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (hash, path) = line
                .split_once("  ")
                .ok_or_else(|| format!("malformed checksum line: {line:?}"))?;
            Ok((self::hex(hash)?, relative(path)?))
        })
        .collect()
}

/// Check every line of a checksum file. Returns the covered paths.
fn check_sums(directory: &Path, name: &str) -> Result<Vec<String>> {
    let path = directory.join(name);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let entries = parse_sums(&text)?;
    if entries.is_empty() {
        return Err(format!("{name} is empty"));
    }
    let mut covered = Vec::with_capacity(entries.len());
    for (hash, file) in entries {
        let target = directory.join(&file);
        let metadata = fs::symlink_metadata(&target)
            .map_err(|e| format!("{name}: {file} is missing ({e})"))?;
        // The bundle's own checksums hash through its internal links (the AI
        // runtime's Python has some); a link must resolve to a file inside.
        let contained_link = metadata.is_symlink()
            && fs::canonicalize(&target)
                .is_ok_and(|resolved| resolved.starts_with(directory) && resolved.is_file());
        if !metadata.is_file() && !contained_link {
            return Err(format!("{name}: {file} is not a regular file"));
        }
        if sha256_file(&target)? != hash {
            return Err(format!("{name}: {file} does not match its SHA-256"));
        }
        covered.push(file);
    }
    Ok(covered)
}

/// Check a distribution's files without running anything: the manifest, the
/// top-level checksums (which must cover the manifest, README, every archive
/// and license file), each archive's size and hash, and the bundle's own
/// checksums when carried.
pub fn check_files(directory: &Path) -> Result<Distribution> {
    let manifest_path = directory.join(MANIFEST);
    let bytes =
        fs::read(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("{MANIFEST} is not JSON: {e}"))?;
    let distribution = Distribution::from_json(&value)?;
    let covered = check_sums(directory, SUMS)?;
    let mut required = vec![MANIFEST.to_owned(), README.to_owned()];
    for pack in &distribution.packs {
        required.push(pack.archive.clone());
        required.push(pack.license_file.clone());
        let archive = directory.join(&pack.archive);
        let size = fs::metadata(&archive)
            .map_err(|e| format!("{}: {e}", pack.archive))?
            .len();
        if size != pack.bytes {
            return Err(format!(
                "{} is {size} bytes; {MANIFEST} says {}",
                pack.archive, pack.bytes
            ));
        }
        if sha256_file(&archive)? != pack.sha256 {
            return Err(format!(
                "{} does not match the SHA-256 in {MANIFEST}",
                pack.archive
            ));
        }
    }
    for file in &required {
        if !covered.contains(file) {
            return Err(format!("{SUMS} does not cover {file}"));
        }
    }
    if distribution.sidecars.iter().any(|name| name == BUNDLE_SUMS) {
        let app_files = check_sums(directory, BUNDLE_SUMS)?;
        if !app_files
            .iter()
            .all(|file| file.starts_with(&format!("{APP_NAME}/")))
        {
            return Err(format!("{BUNDLE_SUMS} names a file outside {APP_NAME}"));
        }
    }
    Ok(distribution)
}

/// The hidden sibling an output is assembled in. Refuses an existing output.
pub fn staging_for(output: &Path) -> Result<PathBuf> {
    if output.exists() || output.is_symlink() {
        return Err(format!(
            "{} already exists; an offline distribution is never replaced, choose a new --output",
            output.display()
        ));
    }
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("--output needs a UTF-8 final component")?;
    let parent = match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let parent = fs::canonicalize(&parent).map_err(|e| e.to_string())?;
    Ok(parent.join(format!(".{name}.staging-{}", std::process::id())))
}

/// A copied app's own CLI, run with only `HOME`, `PATH` and `TMPDIR`.
struct Scrubbed {
    cli: PathBuf,
    home: PathBuf,
    tmp: PathBuf,
}

impl Scrubbed {
    fn new(app: &Path, root: &Path) -> Result<Self> {
        let home = root.join("home");
        let tmp = root.join("tmp");
        for directory in [&home, &tmp] {
            fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        }
        Ok(Self {
            cli: app.join("Contents/MacOS/deadpan-cli"),
            home,
            tmp,
        })
    }

    fn run(&self, arguments: &[&str]) -> Result<String> {
        let result = Command::new(&self.cli)
            .args(arguments)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .env("TMPDIR", &self.tmp)
            .current_dir(&self.home)
            .output()
            .map_err(|e| format!("failed to start {}: {e}", self.cli.display()))?;
        let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
        if result.status.success() {
            Ok(stdout)
        } else {
            Err(format!(
                "deadpan-cli {} failed ({}): {}{}",
                arguments.join(" "),
                result.status,
                stdout.trim(),
                String::from_utf8_lossy(&result.stderr).trim()
            ))
        }
    }

    /// The final JSON value of a command's output (single or line-delimited).
    fn json(&self, arguments: &[&str]) -> Result<Value> {
        let stdout = self.run(arguments)?;
        serde_json::from_str(&stdout)
            .or_else(|_| {
                // Line-delimited events end with a pretty-printed result.
                let start = stdout.rfind("\n{\n").map_or(0, |index| index + 1);
                serde_json::from_str(&stdout[start..])
            })
            .map_err(|e| {
                format!(
                    "deadpan-cli {} output is not JSON ({e})",
                    arguments.join(" ")
                )
            })
    }

    /// The version of `id` the app selects in `root`. Apps with signed
    /// updates list every catalog version and mark the selected one; older
    /// apps list one row per pack.
    fn pack_entry(&self, root: &Path, id: &str) -> Result<Value> {
        let list = self.json(&["models", "list", "--root", path_str(root)?])?;
        let rows: Vec<&Value> = list["packs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|pack| pack["pack_id"] == id)
            .collect();
        rows.iter()
            .find(|pack| pack["selected"] == true)
            .or_else(|| rows.iter().find(|pack| pack.get("selected").is_none()))
            .map(|pack| (*pack).clone())
            .ok_or_else(|| format!("{id} is not an approved pack compiled into this Deadpan.app"))
    }
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| format!("non-UTF-8 path {}", path.display()))
}

fn codesign_verify(app: &Path) -> Result<()> {
    run_tool(
        "codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            app.as_os_str(),
        ],
    )
}

/// The app must be a `cargo xtask bundle` output (Info.plist marker).
fn check_packaged(app: &Path) -> Result<()> {
    let plist = app.join("Contents/Info.plist");
    let result = Command::new("plutil")
        .args(["-extract", "DeadpanPackaging", "raw", "-o", "-"])
        .arg(&plist)
        .output()
        .map_err(|e| format!("failed to start plutil: {e}"))?;
    if result.status.success()
        && String::from_utf8_lossy(&result.stdout).starts_with("xtask-bundle")
    {
        Ok(())
    } else {
        Err(format!(
            "{} is not a `cargo xtask bundle` app (no DeadpanPackaging marker)",
            app.display()
        ))
    }
}

fn provenance(app: &Path) -> Result<Value> {
    let path = app.join("Contents/Resources/build-provenance.json");
    serde_json::from_slice(&fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn has_ai_runtime(app: &Path) -> bool {
    app.join("Contents/Resources/ai-runtime/runtime.json")
        .is_file()
}

fn default_models_root() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?)
            .join("Library/Application Support/Deadpan/Models"),
    )
}

struct Options {
    app: PathBuf,
    output: PathBuf,
    packs: Vec<(String, Option<PathBuf>)>,
    models_root: Option<PathBuf>,
}

fn parse(arguments: &[String]) -> Result<Options> {
    let (mut app, mut output, mut packs, mut models_root) = (None, None, Vec::new(), None);
    let mut rest = arguments;
    while let Some((flag, tail)) = rest.split_first() {
        let Some((value, tail)) = tail.split_first() else {
            return Err(USAGE.into());
        };
        match flag.as_str() {
            "--app" => app = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--models-root" => models_root = Some(PathBuf::from(value)),
            "--pack" => {
                let (id, root) = match value.split_once('=') {
                    Some((id, root)) => (id, Some(PathBuf::from(root))),
                    None => (value.as_str(), None),
                };
                if id.is_empty() || packs.iter().any(|(existing, _)| existing == id) {
                    return Err(format!("--pack {value}: empty or repeated pack id"));
                }
                packs.push((id.to_owned(), root));
            }
            _ => return Err(USAGE.into()),
        }
        rest = tail;
    }
    if packs.is_empty() {
        return Err(format!("name at least one --pack\n{USAGE}"));
    }
    Ok(Options {
        app: app.ok_or(USAGE)?,
        output: output.ok_or(USAGE)?,
        packs,
        models_root,
    })
}

const README_TEXT: &str = "Deadpan offline distribution
============================

This folder installs Deadpan and its model packs without a network
connection.

1. Copy Deadpan.app to /Applications (or anywhere) and open it.
2. In Deadpan, open Models (type :models, or choose Models... in the Deadpan
   menu), choose Install from archive..., and pick the pack's .tar file in
   Packs/. Read and accept its licenses when asked. The app verifies every
   file's size and SHA-256 and runs the pack's smoke test before activating
   it; nothing is downloaded.

   From Terminal instead:

     Deadpan.app/Contents/MacOS/deadpan-cli models license <pack>
     Deadpan.app/Contents/MacOS/deadpan-cli models import <pack> Packs/<pack>-<version>.tar [--accept-license]

   (--accept-license only after reading the licenses; the default models
   folder is ~/Library/Application Support/Deadpan/Models.)

Contents:

- distribution.json  app version and commit, and each pack's archive,
                      size, SHA-256 and licenses
- SHA256SUMS          checksums of this folder's files (shasum -a 256 -c SHA256SUMS)
- Packs/              model packs as uncompressed tar archives
- Licenses/           each pack's licenses, notices and use restrictions
- Deadpan.app.SHA256SUMS, Deadpan.sbom.cdx.json, Deadpan.release.json
                      the app build's checksums, SBOM and release record,
                      when the build produced them

Redistributing this folder, or any pack in it, must keep its Licenses/ file.
";

/// The app's own `models license` text, followed by the standard text of
/// each SPDX license the bundle carries and the app has no compiled text for.
fn license_text(scrubbed: &Scrubbed, app: &Path, root: &Path, entry: &Value) -> Result<String> {
    let id = entry["pack_id"].as_str().unwrap_or_default();
    let mut text = format!(
        "Licenses for model pack {id} version {}\n{}\n\n",
        entry["pack_version"].as_str().unwrap_or_default(),
        entry["title"].as_str().unwrap_or_default()
    );
    // Name the exact version when the app supports it, so the text matches
    // the exported archive even when a signed update is selected.
    let version = entry["pack_version"].as_str().unwrap_or_default();
    let license = if entry.get("selected").is_some() {
        scrubbed.run(&[
            "models",
            "license",
            id,
            "--version",
            version,
            "--root",
            path_str(root)?,
        ])?
    } else {
        scrubbed.run(&["models", "license", id])?
    };
    text.push_str(&license);
    for license in entry["licenses"].as_array().into_iter().flatten() {
        let Some(spdx) = license["spdx"].as_str() else {
            continue;
        };
        let standard = app.join(format!("Contents/Resources/Notices/spdx/{spdx}.txt"));
        if !spdx.starts_with("LicenseRef-") && standard.is_file() {
            let body = fs::read_to_string(&standard).map_err(|e| e.to_string())?;
            let heading = format!("Standard {spdx} license text");
            text.push_str(&format!(
                "\n{heading}\n{}\n\nIt applies to these weights with the copyright holders named in the attribution above.\n\n{body}\n",
                "=".repeat(heading.len())
            ));
        }
    }
    Ok(text)
}

pub fn run(arguments: &[String]) -> Result<()> {
    let options = parse(arguments)?;
    let started = Instant::now();
    let source_app =
        fs::canonicalize(&options.app).map_err(|e| format!("{}: {e}", options.app.display()))?;
    check_packaged(&source_app)?;
    codesign_verify(&source_app)?;
    let output = if options.output.is_absolute() {
        options.output.clone()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(&options.output)
    };
    let staging = staging_for(&output)?;
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let outcome = assemble(&options, &source_app, &staging).and_then(|distribution| {
        let _ = fs::remove_dir_all(staging.join(".work"));
        fs::rename(&staging, &output).map_err(|e| format!("publish {}: {e}", output.display()))?;
        Ok(distribution)
    });
    if outcome.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    let distribution = outcome?;
    let total: u64 = distribution.packs.iter().map(|pack| pack.bytes).sum();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "distribution": output,
            "app_version": distribution.app_version,
            "git_commit": distribution.git_commit,
            "packs": distribution.packs.iter().map(|pack| json!({
                "pack_id": pack.id, "pack_version": pack.version,
                "archive": pack.archive, "bytes": pack.bytes, "sha256": pack.sha256,
            })).collect::<Vec<_>>(),
            "archive_bytes": total,
            "seconds": started.elapsed().as_secs_f64(),
        }))
        .map_err(|e| e.to_string())?
    );
    Ok(())
}

/// Offline distributions carry compiled packs only: a selected signed
/// update would need its envelope and `models update --from` to install, and
/// the app's Install from archive… accepts only versions it already knows.
fn compiled_only(entry: &Value) -> Result<()> {
    match entry.get("origin").and_then(Value::as_str) {
        None | Some("compiled") => Ok(()),
        Some(origin) => Err(format!(
            "{} {} is a {origin} version; offline distributions carry compiled packs only. Roll the models root back to the compiled version (`deadpan-cli models rollback {}`) or rebuild the app with the new pack compiled in",
            entry["pack_id"].as_str().unwrap_or("?"),
            entry["pack_version"].as_str().unwrap_or("?"),
            entry["pack_id"].as_str().unwrap_or("?"),
        )),
    }
}

fn assemble(options: &Options, source_app: &Path, staging: &Path) -> Result<Distribution> {
    let app = staging.join(APP_NAME);
    run_tool("ditto", &[source_app.as_os_str(), app.as_os_str()])?;
    codesign_verify(&app)?;
    let ai_runtime = has_ai_runtime(&app);
    let provenance = provenance(&app)?;
    let scrubbed = Scrubbed::new(&app, &staging.join(".work"))?;
    fs::create_dir_all(staging.join("Packs")).map_err(|e| e.to_string())?;
    fs::create_dir_all(staging.join("Licenses")).map_err(|e| e.to_string())?;
    let mut packs = Vec::new();
    let mut covered = vec![MANIFEST.to_owned(), README.to_owned()];
    for (id, root) in &options.packs {
        let root = match root.clone().or_else(|| options.models_root.clone()) {
            Some(root) => root,
            None => default_models_root()?,
        };
        let root = fs::canonicalize(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        let entry = scrubbed.pack_entry(&root, id)?;
        compiled_only(&entry)?;
        if entry["installed"].as_str().is_none() {
            return Err(format!(
                "{id} version {} is not installed and verified in {}; install it there first (`deadpan-cli models install {id} --root ...` or `models import`)",
                entry["pack_version"].as_str().unwrap_or("?"),
                root.display()
            ));
        }
        let version = entry["pack_version"]
            .as_str()
            .ok_or("models list: pack_version")?
            .to_owned();
        let licenses = entry["licenses"]
            .as_array()
            .ok_or("models list: licenses")?
            .iter()
            .map(|license| License {
                id: license["id"].as_str().unwrap_or_default().to_owned(),
                spdx: license["spdx"].as_str().map(str::to_owned),
                title: license["title"].as_str().unwrap_or_default().to_owned(),
                acceptance_required: license["acceptance_required"] == true,
                redistribution: license["redistribution"] == true,
            })
            .collect::<Vec<_>>();
        if let Some(license) = licenses.iter().find(|license| !license.redistribution) {
            return Err(format!(
                "{id}: the {} does not permit redistribution",
                license.title
            ));
        }
        let requires_ai_runtime = entry["operations"]
            .as_array()
            .is_some_and(|operations| operations.iter().any(|op| op == AI_RUNTIME_OPERATION));
        if requires_ai_runtime && !ai_runtime {
            return Err(format!(
                "{id} needs the bundled AI runtime for its install smoke test, but this Deadpan.app was built --without-ai-runtime"
            ));
        }
        let archive = archive_name(id, &version);
        let destination = staging.join(&archive);
        let exported_at = Instant::now();
        let mut arguments = vec![
            "models",
            "export",
            id,
            path_str(&destination)?,
            "--root",
            path_str(&root)?,
        ];
        // Export exactly the listed version (apps with signed updates).
        if entry.get("selected").is_some() {
            arguments.extend(["--version", version.as_str()]);
        }
        let exported = scrubbed.json(&arguments)?;
        if exported["exported"] != id.as_str() {
            return Err(format!("unexpected export result for {id}: {exported}"));
        }
        let bytes = fs::metadata(&destination)
            .map_err(|e| format!("{archive}: {e}"))?
            .len();
        let sha256 = sha256_file(&destination)?;
        println!(
            "offline-dist: exported {archive} ({bytes} bytes) in {:.1} s",
            exported_at.elapsed().as_secs_f64()
        );
        let license_file = license_name(id, &version);
        fs::write(
            staging.join(&license_file),
            license_text(&scrubbed, &app, &root, &entry)?,
        )
        .map_err(|e| e.to_string())?;
        covered.push(archive.clone());
        covered.push(license_file.clone());
        packs.push(Pack {
            id: id.clone(),
            version,
            title: entry["title"].as_str().unwrap_or_default().to_owned(),
            archive,
            bytes,
            sha256,
            pack_bytes: entry["bytes"].as_u64().ok_or("models list: bytes")?,
            licenses,
            license_file,
            requires_ai_runtime,
        });
    }
    let bundle_directory = source_app.parent().ok_or("app without a parent")?;
    let mut sidecars = Vec::new();
    for name in BUNDLE_SIDECARS {
        let source = bundle_directory.join(name);
        if source.is_file() {
            fs::copy(&source, staging.join(name)).map_err(|e| format!("{name}: {e}"))?;
            sidecars.push(name.to_owned());
            covered.push(name.to_owned());
        }
    }
    let distribution = Distribution {
        app_version: provenance["version"]
            .as_str()
            .ok_or("build-provenance.json: version")?
            .to_owned(),
        git_commit: provenance["git_commit"]
            .as_str()
            .ok_or("build-provenance.json: git_commit")?
            .to_owned(),
        git_changes: provenance["git_changes"] == true,
        signature: provenance["signature"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned(),
        ai_runtime,
        created_unix: created_unix(),
        sidecars,
        packs,
    };
    fs::write(staging.join(README), README_TEXT).map_err(|e| e.to_string())?;
    fs::write(
        staging.join(MANIFEST),
        serde_json::to_vec_pretty(&distribution.to_json()).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    write_sums(staging, &covered, &staging.join(SUMS))?;
    // The same checks the verifier starts with, before anything is published.
    check_files(staging)?;
    Ok(distribution)
}

fn created_unix() -> u64 {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs())
        })
}

/// `cargo xtask offline-dist-verify`: file checks, signature, then an offline
/// import of every pack by the distribution's own CLI into a fresh root from
/// a scrubbed environment. `--accept-license` there stands for the owner's
/// prior acceptance, in a throwaway root that is deleted afterwards.
pub fn verify_command(arguments: &[String]) -> Result<()> {
    let (directory, keep) = match arguments {
        [directory] => (directory, false),
        [directory, flag] | [flag, directory] if flag == "--keep" => (directory, true),
        _ => return Err(VERIFY_USAGE.into()),
    };
    let directory = fs::canonicalize(directory).map_err(|e| format!("{directory}: {e}"))?;
    let started = Instant::now();
    let distribution = check_files(&directory)?;
    println!("offline-dist-verify: ok   manifest and checksums");
    let app = directory.join(APP_NAME);
    codesign_verify(&app)?;
    check_packaged(&app)?;
    println!("offline-dist-verify: ok   codesign --verify --deep --strict");
    let provenance = provenance(&app)?;
    if provenance["version"] != distribution.app_version.as_str()
        || provenance["git_commit"] != distribution.git_commit.as_str()
    {
        return Err(format!(
            "{APP_NAME} provenance does not match {MANIFEST} (version/commit)"
        ));
    }
    let ai_runtime = has_ai_runtime(&app);
    if ai_runtime != distribution.ai_runtime {
        return Err(format!(
            "{MANIFEST} says ai_runtime={}, but the app {}",
            distribution.ai_runtime,
            if ai_runtime { "has one" } else { "has none" }
        ));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let work = std::env::temp_dir()
        .canonicalize()
        .map_err(|e| e.to_string())?
        .join(format!(
            "deadpan-offline-verify-{}-{nanos}",
            std::process::id()
        ));
    let result = import_all(&directory, &app, &work, &distribution, ai_runtime);
    if keep || result.is_err() {
        println!("offline-dist-verify: kept {}", work.display());
    } else {
        let _ = fs::remove_dir_all(&work);
    }
    let packs = result?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "distribution": directory,
            "app_version": distribution.app_version,
            "git_commit": distribution.git_commit,
            "codesign": "verified",
            "checksums": "verified",
            "ai_runtime": ai_runtime,
            "environment": ["HOME=<fresh>", "PATH=/usr/bin:/bin", "TMPDIR=<fresh>"],
            "packs": packs,
            "seconds": started.elapsed().as_secs_f64(),
        }))
        .map_err(|e| e.to_string())?
    );
    Ok(())
}

fn import_all(
    directory: &Path,
    app: &Path,
    work: &Path,
    distribution: &Distribution,
    ai_runtime: bool,
) -> Result<Vec<Value>> {
    let scrubbed = Scrubbed::new(app, work)?;
    let root = work.join("models");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut reports = Vec::new();
    for pack in &distribution.packs {
        if pack.requires_ai_runtime && !ai_runtime {
            println!(
                "offline-dist-verify: skip {}: its smoke test needs the AI runtime this app lacks",
                pack.id
            );
            reports.push(json!({
                "pack_id": pack.id, "pack_version": pack.version,
                "skipped": "the app was built --without-ai-runtime",
            }));
            continue;
        }
        let archive = directory.join(&pack.archive);
        let started = Instant::now();
        let imported = scrubbed.json(&[
            "models",
            "import",
            &pack.id,
            path_str(&archive)?,
            "--root",
            path_str(&root)?,
            "--accept-license",
        ])?;
        let seconds = started.elapsed().as_secs_f64();
        let installed = imported["installed"]
            .as_str()
            .ok_or_else(|| format!("{}: import reported no directory: {imported}", pack.id))?;
        if imported["already_installed"] == true {
            return Err(format!("{}: the fresh root already had it", pack.id));
        }
        let entry = scrubbed.pack_entry(&root, &pack.id)?;
        let listed = entry["installed"].as_str().unwrap_or_default();
        let inside = Path::new(installed).starts_with(&root);
        if listed != installed || !inside || entry["pack_version"] != pack.version.as_str() {
            return Err(format!(
                "{}: `models list` does not confirm {installed} (listed {listed:?})",
                pack.id
            ));
        }
        println!(
            "offline-dist-verify: ok   {} {} imported in {seconds:.1} s",
            pack.id, pack.version
        );
        reports.push(json!({
            "pack_id": pack.id,
            "pack_version": pack.version,
            "archive_bytes": pack.bytes,
            "installed": installed,
            "import_seconds": seconds,
            "licenses_accepted_for_test": pack.licenses.iter()
                .filter(|license| license.acceptance_required)
                .map(|license| license.id.clone())
                .collect::<Vec<_>>(),
        }));
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    #[test]
    fn signed_update_versions_are_refused() {
        let compiled = serde_json::json!({"pack_id": "whisper-base-en", "pack_version": "2", "origin": "compiled", "selected": true});
        assert!(super::compiled_only(&compiled).is_ok());
        // Apps without signed updates list no origin.
        assert!(
            super::compiled_only(&serde_json::json!({"pack_id": "x", "pack_version": "1"})).is_ok()
        );
        let update = serde_json::json!({"pack_id": "whisper-base-en", "pack_version": "3", "origin": "signed_update", "selected": true});
        let error = super::compiled_only(&update).unwrap_err();
        assert!(error.contains("compiled packs only"), "{error}");
    }

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "xtask-offline-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn sample(archive_bytes: u64, sha256: String) -> Distribution {
        Distribution {
            app_version: "0.1.0".into(),
            git_commit: "c3479acc".into(),
            git_changes: false,
            signature: "ad-hoc".into(),
            ai_runtime: false,
            created_unix: 1,
            sidecars: vec![],
            packs: vec![Pack {
                id: "whisper-base-en".into(),
                version: "2".into(),
                title: "English transcription".into(),
                archive: archive_name("whisper-base-en", "2"),
                bytes: archive_bytes,
                sha256,
                pack_bytes: 148_849_309,
                licenses: vec![License {
                    id: "mit".into(),
                    spdx: Some("MIT".into()),
                    title: "MIT License".into(),
                    acceptance_required: false,
                    redistribution: true,
                }],
                license_file: license_name("whisper-base-en", "2"),
                requires_ai_runtime: false,
            }],
        }
    }

    /// A complete distribution folder without an app.
    fn fixture(name: &str) -> PathBuf {
        let directory = scratch(name);
        fs::create_dir_all(directory.join("Packs")).unwrap();
        fs::create_dir_all(directory.join("Licenses")).unwrap();
        let archive = archive_name("whisper-base-en", "2");
        fs::write(directory.join(&archive), b"pack bytes").unwrap();
        fs::write(directory.join(license_name("whisper-base-en", "2")), "MIT").unwrap();
        fs::write(directory.join(README), README_TEXT).unwrap();
        let distribution = sample(10, sha256_file(&directory.join(&archive)).unwrap());
        fs::write(
            directory.join(MANIFEST),
            serde_json::to_vec_pretty(&distribution.to_json()).unwrap(),
        )
        .unwrap();
        let files = [
            MANIFEST.into(),
            README.into(),
            archive,
            license_name("whisper-base-en", "2"),
        ];
        write_sums(&directory, &files, &directory.join(SUMS)).unwrap();
        directory
    }

    #[test]
    fn manifest_round_trips() {
        let distribution = sample(10, "a".repeat(64));
        let value = distribution.to_json();
        assert_eq!(value["schema"], 1);
        assert_eq!(value["packs"][0]["archive"], "Packs/whisper-base-en-2.tar");
        assert_eq!(
            value["packs"][0]["licenses"][0]["acceptance_required"],
            false
        );
        assert_eq!(Distribution::from_json(&value).unwrap(), distribution);
    }

    #[test]
    fn manifest_rejects_escapes_and_wrong_schema() {
        let mut value = sample(10, "a".repeat(64)).to_json();
        value["packs"][0]["license_file"] = json!("../outside.txt");
        assert!(Distribution::from_json(&value).is_err());
        let mut value = sample(10, "a".repeat(64)).to_json();
        value["packs"][0]["archive"] = json!("Packs/other.tar");
        assert!(Distribution::from_json(&value).is_err());
        let mut value = sample(10, "a".repeat(64)).to_json();
        value["schema"] = json!(2);
        assert!(Distribution::from_json(&value).is_err());
        let mut value = sample(10, "A".repeat(64)).to_json();
        value["packs"][0]["sha256"] = json!("A".repeat(64));
        assert!(Distribution::from_json(&value).is_err());
    }

    #[test]
    fn sums_parse_and_refuse_bad_lines() {
        let hash = "0".repeat(64);
        let parsed = parse_sums(&format!("{hash}  Packs/a.tar\n{hash}  {MANIFEST}\n")).unwrap();
        assert_eq!(parsed[0], (hash.clone(), "Packs/a.tar".into()));
        assert!(parse_sums(&format!("{hash}  /etc/passwd\n")).is_err());
        assert!(parse_sums(&format!("{hash}  ../x\n")).is_err());
        assert!(parse_sums("nothex  file\n").is_err());
    }

    #[test]
    fn intact_fixture_verifies() {
        let directory = fixture("intact");
        let distribution = check_files(&directory).unwrap();
        assert_eq!(distribution.packs[0].bytes, 10);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn tampered_archive_is_refused() {
        let directory = fixture("tampered");
        fs::write(
            directory.join(archive_name("whisper-base-en", "2")),
            b"pack bytez",
        )
        .unwrap();
        let error = check_files(&directory).unwrap_err();
        assert!(error.contains("does not match"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_file_is_refused() {
        let directory = fixture("missing");
        fs::remove_file(directory.join(license_name("whisper-base-en", "2"))).unwrap();
        let error = check_files(&directory).unwrap_err();
        assert!(error.contains("missing"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn escaping_link_is_refused() {
        let directory = fixture("link");
        let outside = scratch("link-target");
        let archive = directory.join(archive_name("whisper-base-en", "2"));
        fs::rename(&archive, outside.join("pack.tar")).unwrap();
        std::os::unix::fs::symlink(outside.join("pack.tar"), &archive).unwrap();
        let error = check_files(&directory).unwrap_err();
        assert!(error.contains("not a regular file"), "{error}");
        fs::remove_dir_all(directory).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn uncovered_archive_is_refused() {
        let directory = fixture("uncovered");
        // Checksums that omit the archive, although every listed hash matches.
        write_sums(
            &directory,
            &[MANIFEST.into(), README.into()],
            &directory.join(SUMS),
        )
        .unwrap();
        let error = check_files(&directory).unwrap_err();
        assert!(error.contains("does not cover"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn edited_manifest_is_refused_by_sums() {
        let directory = fixture("edited");
        let path = directory.join(MANIFEST);
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["app"]["version"] = json!("9.9.9");
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        assert!(check_files(&directory).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn existing_output_is_refused() {
        let directory = scratch("existing");
        let error = staging_for(&directory).unwrap_err();
        assert!(error.contains("never replaced"), "{error}");
        let fresh = directory.join("dist");
        let staging = staging_for(&fresh).unwrap();
        assert_eq!(staging.parent().unwrap(), directory.as_path());
        assert!(
            staging
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".dist.staging-")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn options_require_packs_and_refuse_repeats() {
        let arguments = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(parse(&arguments(&["--app", "a", "--output", "o"])).is_err());
        assert!(
            parse(&arguments(&[
                "--app", "a", "--output", "o", "--pack", "x", "--pack", "x=/r"
            ]))
            .is_err()
        );
        let parsed = parse(&arguments(&[
            "--app", "a", "--output", "o", "--pack", "x=/r", "--pack", "y",
        ]))
        .unwrap();
        assert_eq!(parsed.packs[0], ("x".into(), Some(PathBuf::from("/r"))));
        assert_eq!(parsed.packs[1], ("y".into(), None));
    }
}
