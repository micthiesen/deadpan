//! The bridge runtime: the private Python environment, the pinned LTX source,
//! the model pack, FFmpeg, the worker adapter and the native media and
//! landmark workers that qualify its output.
//!
//! A packaged `Deadpan.app` carries the runtime in
//! `Contents/Resources/ai-runtime` (built by `cargo xtask bundle` from
//! `tools/ai-runtime`) and takes the model data from the installed
//! `ltx-2.3-q4-bridge` pack in the models root. It never reads
//! `DEADPAN_BRIDGE_*` variables, development defaults, Homebrew or the
//! checkout, unless a developer opts in with `DEADPAN_DEVELOPER_BRIDGE=1`; the
//! packaged app then uses only explicitly set variables (docs/PACKAGING.md).
//!
//! Development builds and developer wrapper bundles locate a development
//! runtime through environment variables, each with a default:
//!
//! | Variable | Default |
//! | --- | --- |
//! | `DEADPAN_BRIDGE_PYTHON` | `<runtime source>/.venv/bin/python3` |
//! | `DEADPAN_BRIDGE_RUNTIME_SOURCE` | `~/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-<commit>`, else the original `/private/tmp` checkout |
//! | `DEADPAN_BRIDGE_MODEL_CACHE` | the installed bridge pack, else `~/Library/Caches/Deadpan/ltx-qualification` |
//! | `DEADPAN_BRIDGE_FFMPEG` | `/opt/homebrew/bin/ffmpeg` |
//! | `DEADPAN_BRIDGE_FFPROBE` | `/opt/homebrew/bin/ffprobe` |
//! | `DEADPAN_BRIDGE_WORKER` | `tools/model-qualification/worker.py` in this checkout |
//!
//! `deadpan-media-worker` and `deadpan-track` must be installed beside the
//! current executable.
//! The worker verifies the runtime source tree, checks the selected manifest
//! against its pinned component/configuration contract and verifies every
//! model file on each attempt; this module checks only component presence.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_models::packs::{InstalledPack, PackManifest, PackStore, approved_pack};
use sha2::Digest;

mod launch;
pub use launch::{LaunchError, WorkerLaunch, WorkerMode};
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod network_tests;

pub const PYTHON: &str = "DEADPAN_BRIDGE_PYTHON";
pub const RUNTIME_SOURCE: &str = "DEADPAN_BRIDGE_RUNTIME_SOURCE";
pub const MODEL_CACHE: &str = "DEADPAN_BRIDGE_MODEL_CACHE";
pub const FFMPEG: &str = "DEADPAN_BRIDGE_FFMPEG";
pub const FFPROBE: &str = "DEADPAN_BRIDGE_FFPROBE";
pub const WORKER: &str = "DEADPAN_BRIDGE_WORKER";
/// Explicit developer opt-in to the bridge variables inside a packaged app.
pub const DEVELOPER_OPT_IN: &str = "DEADPAN_DEVELOPER_BRIDGE";
/// The model pack that supplies the bridge model data.
pub const BRIDGE_PACK: &str = "ltx-2.3-q4-bridge";
/// The bundled runtime below `Contents`.
pub const BUNDLED_RUNTIME: &str = "Resources/ai-runtime";
/// Bundled runtime layout, relative to its directory.
pub const BUNDLED_PYTHON: &str = "python/bin/python3.12";
pub const BUNDLED_SOURCE: &str = "ltx-2-mlx";
pub const BUNDLED_WORKER: &str = "worker/worker.py";
pub const BUNDLED_FFMPEG: &str = "bin/ffmpeg";
pub const BUNDLED_FFPROBE: &str = "bin/ffprobe";

/// How the bridge runtime may be located in this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// Development build: variables, then development defaults.
    Development,
    /// Packaged app with the developer opt-in: explicit variables only.
    PackagedExplicit,
    /// Packaged app: the runtime inside the bundle and the installed pack.
    Bundled { runtime: PathBuf },
}

impl Lookup {
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Development => {
                "development runtime from DEADPAN_BRIDGE_* variables and development defaults"
            }
            Self::PackagedExplicit => {
                "developer opt-in: explicit DEADPAN_BRIDGE_* variables only; the bundled runtime is ignored"
            }
            Self::Bundled { .. } => {
                "bundled private runtime with the installed model pack; environment variables are ignored"
            }
        }
    }
}

/// The lookup for the running executable.
pub fn lookup() -> Lookup {
    match crate::bundle::packaged_contents() {
        None => Lookup::Development,
        Some(_) if std::env::var_os(DEVELOPER_OPT_IN).is_some_and(|value| value == "1") => {
            Lookup::PackagedExplicit
        }
        Some(contents) => Lookup::Bundled {
            runtime: contents.join(BUNDLED_RUNTIME),
        },
    }
}

/// The installed selected bridge pack, including its exact manifest snapshot.
pub fn installed_pack(models_root: &Path) -> Option<InstalledPack> {
    PackStore::new(models_root.to_path_buf())
        .current(BRIDGE_PACK)
        .ok()
        .flatten()
}

/// `major.minor` of a version string such as `26.5.2`.
fn major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |minor| minor.parse().ok())?;
    Some((major, minor))
}

/// The running macOS version, from the system's own version file.
pub fn current_macos() -> Option<(u32, u32)> {
    let text = std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist").ok()?;
    let key = "<key>ProductVersion</key>";
    let rest = &text[text.find(key)? + key.len()..];
    let value = rest.split("<string>").nth(1)?.split("</string>").next()?;
    major_minor(value)
}

/// The bundled runtime's MLX build targets a minimum macOS (`runtime.json`
/// `minimum_macos`); on an older system AI pauses are unavailable rather than
/// failing inside the worker.
pub fn os_requirement(runtime: &Path, current: Option<(u32, u32)>) -> Option<String> {
    let minimum = std::fs::read(runtime.join("runtime.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|identity| identity["minimum_macos"].as_str().map(str::to_owned));
    // A bundled runtime without a readable requirement is damaged.
    let Some((minimum, required)) =
        minimum.and_then(|minimum| major_minor(&minimum).map(|required| (minimum, required)))
    else {
        return Some(format!(
            "the bundled AI runtime's identity is missing or unreadable at {} (reinstall Deadpan)",
            runtime.join("runtime.json").display()
        ));
    };
    match current {
        Some(current) if current >= required => None,
        Some((major, minor)) => Some(format!(
            "AI pauses need macOS {minimum} or later; this Mac runs macOS {major}.{minor}"
        )),
        None => Some(format!(
            "AI pauses need macOS {minimum} or later; this Mac's version is unknown"
        )),
    }
}

fn pack_os_requirement(manifest: &PackManifest, current: Option<(u32, u32)>) -> Option<String> {
    let required = manifest.constraints.hardware.minimum_macos;
    if current.is_some_and(|current| current >= (required.major, required.minor)) {
        return None;
    }
    Some(format!(
        "model pack {} {} requires macOS {}.{} or later; this Mac's version is {}",
        manifest.pack_id,
        manifest.pack_version,
        required.major,
        required.minor,
        current.map_or_else(
            || "unknown".into(),
            |(major, minor)| format!("{major}.{minor}")
        ),
    ))
}

/// The qualified `ltx-2-mlx` checkout and its private environment.
pub const RUNTIME_COMMIT: &str = "3392d75934120b7e69eefbe55893f7ef82be92a4";
/// The durable development checkout, relative to the home directory.
pub const CACHED_RUNTIME_SOURCE: &str =
    "Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4";
/// The original qualification checkout. macOS periodically clears old files
/// under `/private/tmp`, so it is only a fallback.
pub const TEMPORARY_RUNTIME_SOURCE: &str =
    "/private/tmp/deadpan-ltx2src-3392/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4";
const DEFAULT_WORKER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/model-qualification/worker.py"
);
/// The pinned model snapshots the worker loads, relative to the model cache.
pub const MODEL_DIRECTORIES: [&str; 2] = [
    "mlx_ltx_q4_pack/56a5866d638ecfe37c54d348e88938235185c2d4",
    "mlx_gemma_default_text_encoder/86cc6a8dedbc456dd0e4af01a9d09f396f77e558",
];
pub const MEDIA_WORKER: &str = "deadpan-media-worker";
/// Native landmark inspection worker installed beside this executable.
pub const LANDMARK_WORKER: &str = "deadpan-track";
/// A runtime check imports MLX and the LTX modules and reads headers only.
const CHECK_DEADLINE: Duration = Duration::from_secs(300);

/// Absolute paths to every part of the development runtime and its native
/// qualification workers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeRuntime {
    pub python: PathBuf,
    pub runtime_source: PathBuf,
    pub model_cache: PathBuf,
    /// Immutable manifest snapshot selected for this worker configuration.
    pub model_manifest: PackManifest,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub worker_script: PathBuf,
    pub media_worker: PathBuf,
    pub landmark_worker: PathBuf,
}

/// The runtime pieces that are missing, in user terms.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}: {}", if *bundled { "AI pauses are not ready" } else { "AI pauses need the development model runtime" }, missing.join("; "))]
pub struct RuntimeError {
    pub missing: Vec<String>,
    /// The packaged runtime was used; only an install can fix a missing pack.
    pub bundled: bool,
    /// The model pack is the missing piece.
    pub needs_model_pack: bool,
}

impl BridgeRuntime {
    /// Locate the runtime for this process: the bundled runtime in a
    /// packaged app, otherwise `DEADPAN_BRIDGE_*` and their defaults. Model
    /// data comes from the bridge pack in the default models root.
    pub fn from_environment() -> Result<Self, RuntimeError> {
        Self::from_environment_in(crate::models::default_root().ok().as_deref())
    }

    /// As [`Self::from_environment`] with an explicit models root.
    pub fn from_environment_in(models_root: Option<&Path>) -> Result<Self, RuntimeError> {
        let executable_directory = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        Self::resolve_with(
            |name| std::env::var_os(name),
            std::env::var_os("HOME").map(PathBuf::from),
            executable_directory.as_deref(),
            &lookup(),
            models_root,
        )
    }

    /// Resolve from an explicit environment, home and executable directory,
    /// with the development defaults.
    pub fn resolve(
        variable: impl Fn(&str) -> Option<OsString>,
        home: Option<PathBuf>,
        executable_directory: Option<&Path>,
    ) -> Result<Self, RuntimeError> {
        Self::resolve_with(
            variable,
            home,
            executable_directory,
            &Lookup::Development,
            None,
        )
    }

    /// The runtime for this process with explicit model data, such as a
    /// staged pack that a smoke test checks before activation.
    pub fn with_model_data(model_data: &Path) -> Result<Self, RuntimeError> {
        let manifest = approved_pack(BRIDGE_PACK).expect("compiled bridge pack");
        Self::with_model_manifest(model_data, &manifest)
    }

    /// Resolve a staged or selected pack against this immutable manifest.
    pub fn with_model_manifest(
        model_data: &Path,
        manifest: &PackManifest,
    ) -> Result<Self, RuntimeError> {
        let executable_directory = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        Self::resolve_inner(
            |name| std::env::var_os(name),
            std::env::var_os("HOME").map(PathBuf::from),
            executable_directory.as_deref(),
            &lookup(),
            None,
            Some(model_data),
            Some(manifest),
        )
    }

    /// As [`Self::resolve`] under an explicit [`Lookup`] and models root.
    pub fn resolve_with(
        variable: impl Fn(&str) -> Option<OsString>,
        home: Option<PathBuf>,
        executable_directory: Option<&Path>,
        lookup: &Lookup,
        models_root: Option<&Path>,
    ) -> Result<Self, RuntimeError> {
        Self::resolve_inner(
            variable,
            home,
            executable_directory,
            lookup,
            models_root,
            None,
            None,
        )
    }

    fn resolve_inner(
        variable: impl Fn(&str) -> Option<OsString>,
        home: Option<PathBuf>,
        executable_directory: Option<&Path>,
        lookup: &Lookup,
        models_root: Option<&Path>,
        model_data: Option<&Path>,
        model_manifest: Option<&PackManifest>,
    ) -> Result<Self, RuntimeError> {
        if let Lookup::Bundled { runtime } = lookup {
            return Self::bundled(
                runtime,
                executable_directory,
                models_root,
                model_data,
                model_manifest,
            );
        }
        let development_defaults = *lookup == Lookup::Development;
        let mut missing = Vec::new();
        let chosen = |name: &str, default: Option<PathBuf>| {
            variable(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .or(default.filter(|_| development_defaults))
        };
        let cached = home.as_ref().map(|home| home.join(CACHED_RUNTIME_SOURCE));
        let default_source = match &cached {
            Some(cached) if cached.join("packages").is_dir() => cached.clone(),
            _ if Path::new(TEMPORARY_RUNTIME_SOURCE)
                .join("packages")
                .is_dir() =>
            {
                TEMPORARY_RUNTIME_SOURCE.into()
            }
            _ => cached.unwrap_or_else(|| TEMPORARY_RUNTIME_SOURCE.into()),
        };
        let runtime_source = chosen(RUNTIME_SOURCE, Some(default_source));
        // Derived from the chosen source, so it applies to explicit sources too.
        let python = variable(PYTHON)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                runtime_source
                    .as_ref()
                    .map(|source| source.join(".venv/bin/python3"))
            });
        let explicit_model_cache = variable(MODEL_CACHE).filter(|value| !value.is_empty());
        let selected = if model_data.is_none() && explicit_model_cache.is_none() {
            models_root.and_then(installed_pack)
        } else {
            None
        };
        let selected_manifest = model_manifest
            .cloned()
            .or_else(|| selected.as_ref().map(|pack| pack.manifest.clone()))
            .or_else(|| approved_pack(BRIDGE_PACK))
            .expect("compiled bridge pack");
        let model_cache = model_data
            .map(Path::to_path_buf)
            .or_else(|| explicit_model_cache.map(PathBuf::from))
            .or_else(|| selected.map(|pack| pack.directory))
            .or_else(|| home.map(|home| home.join("Library/Caches/Deadpan/ltx-qualification")));
        let ffmpeg = chosen(FFMPEG, Some("/opt/homebrew/bin/ffmpeg".into()));
        let ffprobe = chosen(FFPROBE, Some("/opt/homebrew/bin/ffprobe".into()));
        let worker_script = chosen(WORKER, Some(DEFAULT_WORKER.into()));
        let media_worker = executable_directory.map(|directory| directory.join(MEDIA_WORKER));
        let landmark_worker = executable_directory.map(|directory| directory.join(LANDMARK_WORKER));

        let mut require = |label: &str, variable: &str, path: Option<PathBuf>, directory: bool| {
            let found = path.as_ref().and_then(|path| {
                let path = std::fs::canonicalize(path).ok()?;
                let metadata = std::fs::metadata(&path).ok()?;
                (if directory {
                    metadata.is_dir()
                } else {
                    metadata.is_file()
                })
                .then_some(path)
            });
            if found.is_none() {
                let shown =
                    path.map_or_else(|| "no location".into(), |path| path.display().to_string());
                missing.push(if variable.is_empty() {
                    format!("{label} not found at {shown}")
                } else {
                    format!("{label} not found at {shown} (set {variable})")
                });
            }
            found.unwrap_or_default()
        };
        let runtime = Self {
            // A virtual environment's interpreter is a symlink whose own
            // location selects the environment: keep it, absolute but unresolved.
            python: {
                let found = require("Python environment", PYTHON, python.clone(), false);
                match python.and_then(|path| std::path::absolute(path).ok()) {
                    Some(path) if !found.as_os_str().is_empty() => path,
                    _ => found,
                }
            },
            runtime_source: require("LTX runtime source", RUNTIME_SOURCE, runtime_source, true),
            model_cache: require("model data", MODEL_CACHE, model_cache, true),
            model_manifest: selected_manifest,
            ffmpeg: require("ffmpeg", FFMPEG, ffmpeg, false),
            ffprobe: require("ffprobe", FFPROBE, ffprobe, false),
            worker_script: require("worker adapter", WORKER, worker_script, false),
            media_worker: require(
                "deadpan-media-worker beside this executable",
                "",
                media_worker,
                false,
            ),
            landmark_worker: require(
                "deadpan-track beside this executable",
                "",
                landmark_worker,
                false,
            ),
        };
        if !runtime.runtime_source.as_os_str().is_empty()
            && !runtime
                .runtime_source
                .join("packages/ltx-core-mlx")
                .is_dir()
        {
            missing.push(format!(
                "LTX runtime source files are missing from {} (check out ltx-2-mlx {RUNTIME_COMMIT} there)",
                runtime.runtime_source.display()
            ));
        }
        if !runtime.model_cache.as_os_str().is_empty() {
            for directory in model_component_directories(&runtime.model_manifest) {
                if !runtime.model_cache.join(&directory).is_dir() {
                    missing.push(format!(
                        "model component {directory} is not in {}",
                        runtime.model_cache.display()
                    ));
                }
            }
        }
        if missing.is_empty() {
            Ok(runtime)
        } else {
            Err(RuntimeError {
                missing,
                bundled: false,
                needs_model_pack: false,
            })
        }
    }

    /// The runtime inside a packaged bundle and the installed model pack. No
    /// variable, default or other location is consulted.
    fn bundled(
        runtime: &Path,
        executable_directory: Option<&Path>,
        models_root: Option<&Path>,
        model_data: Option<&Path>,
        model_manifest: Option<&PackManifest>,
    ) -> Result<Self, RuntimeError> {
        let mut missing = Vec::new();
        let mut part = |label: &str, path: PathBuf, directory: bool| {
            let found =
                std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir() == directory);
            if !found {
                missing.push(format!(
                    "the bundled {label} is missing at {} (reinstall Deadpan)",
                    path.display()
                ));
            }
            path
        };
        let python = part("Python", runtime.join(BUNDLED_PYTHON), false);
        let runtime_source = part("LTX source", runtime.join(BUNDLED_SOURCE), true);
        let worker_script = part("worker", runtime.join(BUNDLED_WORKER), false);
        let ffmpeg = part("ffmpeg", runtime.join(BUNDLED_FFMPEG), false);
        let ffprobe = part("ffprobe", runtime.join(BUNDLED_FFPROBE), false);
        let media_worker = part(
            "media worker",
            executable_directory
                .map(|directory| directory.join(MEDIA_WORKER))
                .unwrap_or_default(),
            false,
        );
        let landmark_worker = part(
            "landmark worker",
            executable_directory
                .map(|directory| directory.join(LANDMARK_WORKER))
                .unwrap_or_default(),
            false,
        );
        if let Some(problem) = os_requirement(runtime, current_macos()) {
            missing.push(problem);
        }
        let selected = if model_data.is_none() {
            models_root.and_then(installed_pack)
        } else {
            None
        };
        let selected_manifest = model_manifest
            .cloned()
            .or_else(|| selected.as_ref().map(|pack| pack.manifest.clone()))
            .or_else(|| approved_pack(BRIDGE_PACK))
            .expect("compiled bridge pack");
        let model_cache = model_data
            .map(Path::to_path_buf)
            .or_else(|| selected.map(|pack| pack.directory));
        let needs_model_pack = model_cache.is_none();
        if let Some(problem) = pack_os_requirement(&selected_manifest, current_macos()) {
            missing.push(problem);
        }
        if needs_model_pack {
            let size = approved_pack(BRIDGE_PACK)
                .map_or(0, |pack| pack.total_bytes())
                .div_ceil(100_000_000);
            missing.push(format!(
                "install the AI model pack ({}.{} GB) from Models… or with `deadpan-cli models install {BRIDGE_PACK} --accept-license`",
                size / 10,
                size % 10
            ));
        }
        if missing.is_empty() {
            Ok(Self {
                python,
                runtime_source,
                model_cache: model_cache.unwrap_or_default(),
                model_manifest: selected_manifest,
                ffmpeg,
                ffprobe,
                worker_script,
                media_worker,
                landmark_worker,
            })
        } else {
            Err(RuntimeError {
                missing,
                bundled: true,
                needs_model_pack,
            })
        }
    }

    /// Run the worker's `--check`: the pinned source imports, Metal runs and
    /// every model file is present with a readable header. Used as a model
    /// pack's smoke test before activation; it runs no inference.
    pub fn check(&self, cancelled: &AtomicBool) -> Result<serde_json::Value, String> {
        use crate::youtube::runner::{HelperCommand, Workspace, run_helper};
        let workspace = Workspace::new().map_err(|error| error.to_string())?;
        let configuration = workspace.path().join("runtime.json");
        std::fs::write(&configuration, self.worker_configuration())
            .map_err(|error| error.to_string())?;
        let launch = self
            .worker_launch(&configuration, WorkerMode::Check)
            .map_err(|error| error.to_string())?;
        let run = run_helper(
            HelperCommand {
                executable: &launch.executable,
                arguments: &launch.arguments,
                private: workspace.path(),
                current_dir: workspace.path(),
                max_stdout: 64 * 1024,
                overflow: crate::youtube::ImportError::new(
                    "ModelPackFailed",
                    "the AI runtime check printed too much",
                ),
                timeout: CHECK_DEADLINE,
            },
            cancelled,
            || Ok(()),
        )
        .map_err(|error| error.to_string())?;
        if !run.status.success() {
            let tail: String = run
                .stderr
                .lines()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!(
                "the AI runtime check failed ({}): {tail}",
                run.status
            ));
        }
        serde_json::from_slice(&run.stdout)
            .map_err(|error| format!("the AI runtime check printed no report: {error}"))
    }

    /// The worker's `--runtime-config` document.
    pub fn worker_configuration(&self) -> Vec<u8> {
        let path = |path: &Path| path.to_string_lossy().into_owned();
        serde_json::to_vec_pretty(&serde_json::json!({
            "runtime_source": path(&self.runtime_source),
            "model_cache": path(&self.model_cache),
            "ffmpeg": path(&self.ffmpeg),
            "ffprobe": path(&self.ffprobe),
            "model_pack": {
                "pack_id": &self.model_manifest.pack_id,
                "pack_version": &self.model_manifest.pack_version,
                "model_family": &self.model_manifest.model_family,
                "runtime_id": &self.model_manifest.runtime_id,
                "runtime_versions": &self.model_manifest.runtime_versions,
                "operations": &self.model_manifest.operations,
                "files": self.model_manifest.files.iter().map(|file| serde_json::json!({
                    "name": &file.name,
                    "sha256": &file.sha256,
                    "bytes": file.bytes,
                })).collect::<Vec<_>>(),
            },
            "model_manifest_sha256": self.model_manifest_sha256(),
        }))
        .expect("string map serializes")
    }

    /// Provider identity captured with this runtime, including the selected
    /// pack version and the exact worker compatibility version.
    pub fn provider(&self, seed: u64) -> deadpan_jobs::ProviderSelection {
        serde_json::from_value(serde_json::json!({
            "pack_id": &self.model_manifest.pack_id,
            "pack_version": &self.model_manifest.pack_version,
            "runtime_id": &self.model_manifest.runtime_id,
            "runtime_version": self.model_manifest.runtime_versions[0],
            "seed": seed,
        }))
        .expect("admitted bridge manifest has a protocol-compatible identity")
    }

    /// Stable identity of the complete selected manifest, including its
    /// licenses, source URLs, file hashes and runtime compatibility.
    pub fn model_manifest_sha256(&self) -> String {
        let bytes =
            serde_json::to_vec(&self.model_manifest).expect("validated model manifest serializes");
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

fn model_component_directories(manifest: &PackManifest) -> Vec<String> {
    let mut directories = std::collections::BTreeSet::new();
    for file in &manifest.files {
        let mut parts = file.name.split('/');
        if let (Some(component), Some(revision)) = (parts.next(), parts.next()) {
            directories.insert(format!("{component}/{revision}"));
        }
    }
    directories.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    pub(super) fn file(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    fn environment(entries: &BTreeMap<&str, PathBuf>) -> impl Fn(&str) -> Option<OsString> {
        move |name| entries.get(name).map(|path| path.clone().into_os_string())
    }

    #[test]
    fn a_complete_runtime_resolves_to_canonical_paths() {
        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        let source = root_path.join("source");
        let python = source.join(".venv/bin/python3");
        file(&python);
        std::fs::create_dir_all(source.join("packages/ltx-core-mlx")).unwrap();
        let cache = root_path.join("cache");
        for directory in MODEL_DIRECTORIES {
            std::fs::create_dir_all(cache.join(directory)).unwrap();
        }
        let tools = root_path.join("tools");
        for name in ["ffmpeg", "ffprobe", "worker.py"] {
            file(&tools.join(name));
        }
        let executables = root_path.join("bin");
        file(&executables.join(MEDIA_WORKER));
        file(&executables.join(LANDMARK_WORKER));
        let variables = BTreeMap::from([
            (RUNTIME_SOURCE, source.clone()),
            (MODEL_CACHE, cache.clone()),
            (FFMPEG, tools.join("ffmpeg")),
            (FFPROBE, tools.join("ffprobe")),
            (WORKER, tools.join("worker.py")),
        ]);
        let runtime =
            BridgeRuntime::resolve(environment(&variables), None, Some(&executables)).unwrap();
        assert_eq!(runtime.python, python, "Python defaults inside the source");
        // The environment's interpreter symlink is kept, not resolved.
        let linked = root_path.join("linked-python");
        std::os::unix::fs::symlink(&python, &linked).unwrap();
        let mut with_link = variables.clone();
        with_link.insert(PYTHON, linked.clone());
        let resolved =
            BridgeRuntime::resolve(environment(&with_link), None, Some(&executables)).unwrap();
        assert_eq!(resolved.python, linked);
        assert_eq!(runtime.model_cache, cache);
        assert_eq!(runtime.media_worker, executables.join(MEDIA_WORKER));
        assert_eq!(runtime.landmark_worker, executables.join(LANDMARK_WORKER));
        assert_eq!(runtime.model_manifest.pack_id, BRIDGE_PACK);
        assert_eq!(runtime.provider(17).pack_version.as_str(), "1");
        let configuration: serde_json::Value =
            serde_json::from_slice(&runtime.worker_configuration()).unwrap();
        assert_eq!(
            configuration
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            [
                "ffmpeg",
                "ffprobe",
                "model_cache",
                "model_manifest_sha256",
                "model_pack",
                "runtime_source"
            ]
        );
    }

    #[test]
    fn every_missing_piece_is_named_with_its_variable() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        std::fs::create_dir_all(cache.join(MODEL_DIRECTORIES[0])).unwrap();
        let variables = BTreeMap::from([
            (RUNTIME_SOURCE, root.path().join("absent")),
            (MODEL_CACHE, cache),
            (FFMPEG, root.path().join("ffmpeg")),
            (FFPROBE, root.path().join("ffprobe")),
            (WORKER, root.path().join("worker.py")),
        ]);
        let error = BridgeRuntime::resolve(environment(&variables), None, None).unwrap_err();
        let message = error.to_string();
        assert!(message.starts_with("AI pauses need the development model runtime: "));
        for expected in [
            "Python environment not found",
            "(set DEADPAN_BRIDGE_RUNTIME_SOURCE)",
            "(set DEADPAN_BRIDGE_FFMPEG)",
            "(set DEADPAN_BRIDGE_FFPROBE)",
            "(set DEADPAN_BRIDGE_WORKER)",
            "deadpan-media-worker beside this executable not found at no location",
            "deadpan-track beside this executable not found at no location",
            "model component mlx_gemma_default_text_encoder",
        ] {
            assert!(message.contains(expected), "{expected} in {message}");
        }
        assert!(
            !message.contains("mlx_ltx_q4_pack"),
            "present snapshot not reported"
        );
        assert_eq!(error.missing.len(), 8);
    }

    /// A bundled runtime layout and, optionally, an installed bridge pack.
    fn bundled_layout(root: &Path, with_pack: bool) -> (PathBuf, PathBuf, PathBuf) {
        let runtime = root.join("Deadpan.app/Contents").join(BUNDLED_RUNTIME);
        for file in [
            BUNDLED_PYTHON,
            BUNDLED_WORKER,
            BUNDLED_FFMPEG,
            BUNDLED_FFPROBE,
        ] {
            self::file(&runtime.join(file));
        }
        std::fs::create_dir_all(runtime.join(BUNDLED_SOURCE)).unwrap();
        std::fs::write(runtime.join("runtime.json"), r#"{"minimum_macos": "15.0"}"#).unwrap();
        let executables = root.join("Deadpan.app/Contents/MacOS");
        file(&executables.join(MEDIA_WORKER));
        file(&executables.join(LANDMARK_WORKER));
        let models = root.join("Models");
        if with_pack {
            let manifest = approved_pack(BRIDGE_PACK).unwrap();
            let directory = models.join(&manifest.pack_id).join(&manifest.pack_version);
            for pack_file in &manifest.files {
                let path = directory.join(&pack_file.name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::File::create(&path)
                    .unwrap()
                    .set_len(pack_file.bytes)
                    .unwrap();
            }
            let receipt = serde_json::json!({
                "pack_id": manifest.pack_id, "pack_version": manifest.pack_version,
                "files": manifest.files.iter().map(|f| serde_json::json!({
                    "name": f.name, "sha256": f.sha256, "bytes": f.bytes})).collect::<Vec<_>>(),
            });
            std::fs::write(directory.join("receipt.json"), receipt.to_string()).unwrap();
        }
        (runtime, executables, models)
    }

    #[test]
    fn a_packaged_app_uses_only_its_bundled_runtime_and_installed_pack() {
        let root = tempfile::tempdir().unwrap();
        let (runtime, executables, models) = bundled_layout(root.path(), true);
        // Variables naming another runtime are ignored entirely.
        let elsewhere = root.path().join("elsewhere");
        let variables = BTreeMap::from([
            (RUNTIME_SOURCE, elsewhere.clone()),
            (PYTHON, elsewhere.join("python")),
            (MODEL_CACHE, elsewhere.clone()),
            (FFMPEG, elsewhere.join("ffmpeg")),
            (WORKER, elsewhere.join("worker.py")),
        ]);
        let lookup = Lookup::Bundled {
            runtime: runtime.clone(),
        };
        let resolved = BridgeRuntime::resolve_with(
            environment(&variables),
            Some(root.path().into()),
            Some(&executables),
            &lookup,
            Some(&models),
        )
        .unwrap();
        assert_eq!(resolved.python, runtime.join(BUNDLED_PYTHON));
        assert_eq!(resolved.runtime_source, runtime.join(BUNDLED_SOURCE));
        assert_eq!(resolved.worker_script, runtime.join(BUNDLED_WORKER));
        assert_eq!(resolved.ffmpeg, runtime.join(BUNDLED_FFMPEG));
        assert_eq!(resolved.model_cache, models.join(BRIDGE_PACK).join("1"));
        for directory in MODEL_DIRECTORIES {
            assert!(resolved.model_cache.join(directory).is_dir());
        }

        // Without the pack the error says how to install it, and nothing else.
        let empty = root.path().join("no-models");
        let error = BridgeRuntime::resolve_with(
            environment(&variables),
            None,
            Some(&executables),
            &lookup,
            Some(&empty),
        )
        .unwrap_err();
        assert!(error.bundled && error.needs_model_pack);
        assert_eq!(error.missing.len(), 1, "{error}");
        assert!(
            error
                .to_string()
                .starts_with("AI pauses are not ready: install the AI model pack (36.2 GB)"),
            "{error}"
        );
        for absent in [
            "/opt/homebrew",
            "model-qualification",
            "/private/tmp",
            "elsewhere",
        ] {
            assert!(!error.to_string().contains(absent), "{absent} in {error}");
        }

        // A damaged bundle names its missing parts.
        std::fs::remove_file(runtime.join(BUNDLED_FFMPEG)).unwrap();
        let error =
            BridgeRuntime::resolve_with(|_| None, None, Some(&executables), &lookup, Some(&models))
                .unwrap_err();
        assert!(!error.needs_model_pack);
        assert!(
            error.missing[0].starts_with("the bundled ffmpeg is missing"),
            "{error}"
        );
        file(&runtime.join(BUNDLED_FFMPEG));
        std::fs::remove_file(executables.join(LANDMARK_WORKER)).unwrap();
        let error =
            BridgeRuntime::resolve_with(|_| None, None, Some(&executables), &lookup, Some(&models))
                .unwrap_err();
        assert!(
            error
                .missing
                .iter()
                .any(|problem| problem.contains("bundled landmark worker is missing")),
            "{error}"
        );
    }

    #[test]
    fn the_bundled_runtime_names_its_minimum_macos() {
        let root = tempfile::tempdir().unwrap();
        // Without a readable identity the bundle is damaged: fail closed.
        assert!(
            os_requirement(root.path(), Some((26, 0)))
                .is_some_and(|problem| problem.contains("identity is missing"))
        );
        std::fs::write(
            root.path().join("runtime.json"),
            r#"{"minimum_macos": "x"}"#,
        )
        .unwrap();
        assert!(os_requirement(root.path(), Some((26, 0))).is_some());
        std::fs::write(
            root.path().join("runtime.json"),
            r#"{"minimum_macos": "26.0"}"#,
        )
        .unwrap();
        assert_eq!(os_requirement(root.path(), Some((26, 5))), None);
        assert_eq!(os_requirement(root.path(), Some((27, 0))), None);
        assert_eq!(
            os_requirement(root.path(), Some((15, 7))).as_deref(),
            Some("AI pauses need macOS 26.0 or later; this Mac runs macOS 15.7")
        );
        assert!(os_requirement(root.path(), None).is_some());
        assert_eq!(major_minor("26.5.2"), Some((26, 5)));
        assert_eq!(major_minor("15"), Some((15, 0)));
        assert!(current_macos().is_some_and(|(major, _)| major >= 15));
    }

    #[test]
    fn development_builds_prefer_an_installed_pack_over_the_qualification_cache() {
        let root = tempfile::tempdir().unwrap();
        let (_, _, models) = bundled_layout(root.path(), true);
        let error = BridgeRuntime::resolve_with(
            |_| None,
            Some(root.path().join("home")),
            None,
            &Lookup::Development,
            Some(&models),
        )
        .unwrap_err();
        assert!(
            !error
                .missing
                .iter()
                .any(|line| line.starts_with("model data")),
            "{error}"
        );
        assert!(
            !error
                .missing
                .iter()
                .any(|line| line.contains("pinned model snapshot"))
        );
    }

    #[test]
    fn selected_manifest_keeps_its_os_and_picture_constraints() {
        let mut pack = approved_pack(BRIDGE_PACK).unwrap();
        assert!(pack_os_requirement(&pack, Some((26, 0))).is_none());
        assert!(
            pack_os_requirement(&pack, Some((15, 7)))
                .unwrap()
                .contains("requires macOS 26.0")
        );
        assert!(
            pack_os_requirement(&pack, None)
                .unwrap()
                .contains("unknown")
        );
        pack.constraints.hardware.minimum_macos.minor = 6;
        assert!(pack_os_requirement(&pack, Some((26, 5))).is_some());
        assert!(pack_os_requirement(&pack, Some((26, 6))).is_none());
        let capability = super::super::development_capability();
        assert_eq!(
            capability.dimensions().width().minimum(),
            super::super::NATIVE_WIDTH
        );
        assert_eq!(
            capability.dimensions().height().minimum(),
            super::super::NATIVE_HEIGHT
        );
        assert_eq!(
            i64::from(pack.constraints.bridge.unwrap().maximum_project_frames),
            super::super::MAX_BRIDGE_PROJECT_FRAMES
        );
    }

    #[test]
    fn bundles_use_only_explicit_variables() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let variables = BTreeMap::from([(RUNTIME_SOURCE, source.clone())]);
        let error = BridgeRuntime::resolve_with(
            |_| None,
            Some(root.path().into()),
            None,
            &Lookup::PackagedExplicit,
            None,
        )
        .unwrap_err();
        let message = error.to_string();
        for absent in ["/opt/homebrew", "model-qualification", "/private/tmp"] {
            assert!(!message.contains(absent), "{absent} in {message}");
        }
        for expected in [
            "(set DEADPAN_BRIDGE_RUNTIME_SOURCE)",
            "(set DEADPAN_BRIDGE_MODEL_CACHE)",
            "(set DEADPAN_BRIDGE_FFMPEG)",
            "(set DEADPAN_BRIDGE_WORKER)",
        ] {
            assert!(message.contains(expected), "{expected} in {message}");
        }
        // An explicit source still supplies its own environment's interpreter.
        file(&source.join(".venv/bin/python3"));
        let error = BridgeRuntime::resolve_with(
            environment(&variables),
            None,
            None,
            &Lookup::PackagedExplicit,
            None,
        )
        .unwrap_err();
        assert!(!error.to_string().contains("Python environment not found"));
    }

    #[test]
    fn a_cleared_source_checkout_is_reported() {
        let root = tempfile::tempdir().unwrap();
        // The environment survives but the source files are gone, as after
        // macOS clears old files under /private/tmp.
        let source = root.path().join("source");
        file(&source.join(".venv/bin/python3"));
        let variables = BTreeMap::from([(RUNTIME_SOURCE, source)]);
        let error = BridgeRuntime::resolve(environment(&variables), None, None).unwrap_err();
        assert!(
            error
                .missing
                .iter()
                .any(|line| line.starts_with("LTX runtime source files are missing from")),
            "{error}"
        );
    }

    #[test]
    fn a_missing_home_leaves_the_model_cache_unresolved() {
        let error = BridgeRuntime::resolve(|_| None, None, None).unwrap_err();
        assert!(
            error.missing.iter().any(|line| line
                == "model data not found at no location (set DEADPAN_BRIDGE_MODEL_CACHE)")
        );
    }
}
