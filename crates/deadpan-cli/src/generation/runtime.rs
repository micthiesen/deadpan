//! The development bridge runtime: the private Python environment, the pinned
//! LTX source checkout and model data, FFmpeg, the worker adapter and the
//! media worker that qualifies its output.
//!
//! This is a developer runtime located through environment variables, not a
//! distributed one. Each variable has a development default:
//!
//! | Variable | Default |
//! | --- | --- |
//! | `DEADPAN_BRIDGE_PYTHON` | `<runtime source>/.venv/bin/python3` |
//! | `DEADPAN_BRIDGE_RUNTIME_SOURCE` | `~/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-<commit>`, else the original `/private/tmp` checkout |
//! | `DEADPAN_BRIDGE_MODEL_CACHE` | `~/Library/Caches/Deadpan/ltx-qualification` |
//! | `DEADPAN_BRIDGE_FFMPEG` | `/opt/homebrew/bin/ffmpeg` |
//! | `DEADPAN_BRIDGE_FFPROBE` | `/opt/homebrew/bin/ffprobe` |
//! | `DEADPAN_BRIDGE_WORKER` | `tools/model-qualification/worker.py` in this checkout |
//!
//! A packaged `Deadpan.app` carries no AI runtime and ignores all of this by
//! default, so it never silently depends on a build machine's checkout,
//! Homebrew installation or inherited variables. A developer may opt in with
//! `DEADPAN_DEVELOPER_BRIDGE=1`; the packaged app then uses only explicitly
//! set `DEADPAN_BRIDGE_*` variables, never the defaults (docs/PACKAGING.md).
//! Developer wrapper bundles keep the development behavior.
//!
//! `deadpan-media-worker` must be installed beside the current executable.
//! The worker verifies the runtime source tree and every model file against
//! its pinned manifests on each attempt; this module checks only presence.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const PYTHON: &str = "DEADPAN_BRIDGE_PYTHON";
pub const RUNTIME_SOURCE: &str = "DEADPAN_BRIDGE_RUNTIME_SOURCE";
pub const MODEL_CACHE: &str = "DEADPAN_BRIDGE_MODEL_CACHE";
pub const FFMPEG: &str = "DEADPAN_BRIDGE_FFMPEG";
pub const FFPROBE: &str = "DEADPAN_BRIDGE_FFPROBE";
pub const WORKER: &str = "DEADPAN_BRIDGE_WORKER";
/// Explicit developer opt-in to the bridge variables inside a packaged app.
pub const DEVELOPER_OPT_IN: &str = "DEADPAN_DEVELOPER_BRIDGE";

/// How the bridge runtime may be located in this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// Development build: variables, then development defaults.
    Development,
    /// Packaged app with the developer opt-in: explicit variables only.
    PackagedExplicit,
    /// Packaged app: no AI runtime.
    PackagedDisabled,
}

impl Lookup {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Development => {
                "development runtime from DEADPAN_BRIDGE_* variables and development defaults"
            }
            Self::PackagedExplicit => {
                "developer opt-in: explicit DEADPAN_BRIDGE_* variables only; not bundled"
            }
            Self::PackagedDisabled => "not bundled; the packaged application has no AI runtime",
        }
    }
}

/// The lookup for the running executable.
pub fn lookup() -> Lookup {
    if crate::bundle::packaged_contents().is_none() {
        Lookup::Development
    } else if std::env::var_os(DEVELOPER_OPT_IN).is_some_and(|value| value == "1") {
        Lookup::PackagedExplicit
    } else {
        Lookup::PackagedDisabled
    }
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

/// Absolute paths to every part of the development runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeRuntime {
    pub python: PathBuf,
    pub runtime_source: PathBuf,
    pub model_cache: PathBuf,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub worker_script: PathBuf,
    pub media_worker: PathBuf,
}

/// The runtime pieces that are missing, in user terms.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("AI pauses need the development model runtime: {}", missing.join("; "))]
pub struct RuntimeError {
    pub missing: Vec<String>,
}

impl BridgeRuntime {
    /// Locate the runtime from `DEADPAN_BRIDGE_*` and their defaults.
    pub fn from_environment() -> Result<Self, RuntimeError> {
        let executable_directory = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        Self::resolve_with(
            |name| std::env::var_os(name),
            std::env::var_os("HOME").map(PathBuf::from),
            executable_directory.as_deref(),
            lookup(),
        )
    }

    /// Resolve from an explicit environment, home and executable directory,
    /// with the development defaults.
    pub fn resolve(
        variable: impl Fn(&str) -> Option<OsString>,
        home: Option<PathBuf>,
        executable_directory: Option<&Path>,
    ) -> Result<Self, RuntimeError> {
        Self::resolve_with(variable, home, executable_directory, Lookup::Development)
    }

    /// As [`Self::resolve`] under an explicit [`Lookup`].
    pub fn resolve_with(
        variable: impl Fn(&str) -> Option<OsString>,
        home: Option<PathBuf>,
        executable_directory: Option<&Path>,
        lookup: Lookup,
    ) -> Result<Self, RuntimeError> {
        if lookup == Lookup::PackagedDisabled {
            return Err(RuntimeError {
                missing: vec![format!(
                    "this packaged Deadpan has no AI model runtime (developers: set {DEVELOPER_OPT_IN}=1 with explicit DEADPAN_BRIDGE_* paths)"
                )],
            });
        }
        let development_defaults = lookup == Lookup::Development;
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
        let model_cache = chosen(
            MODEL_CACHE,
            home.map(|home| home.join("Library/Caches/Deadpan/ltx-qualification")),
        );
        let ffmpeg = chosen(FFMPEG, Some("/opt/homebrew/bin/ffmpeg".into()));
        let ffprobe = chosen(FFPROBE, Some("/opt/homebrew/bin/ffprobe".into()));
        let worker_script = chosen(WORKER, Some(DEFAULT_WORKER.into()));
        let media_worker = executable_directory.map(|directory| directory.join(MEDIA_WORKER));

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
            ffmpeg: require("ffmpeg", FFMPEG, ffmpeg, false),
            ffprobe: require("ffprobe", FFPROBE, ffprobe, false),
            worker_script: require("worker adapter", WORKER, worker_script, false),
            media_worker: require(
                "deadpan-media-worker beside this executable",
                "",
                media_worker,
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
            for directory in MODEL_DIRECTORIES {
                if !runtime.model_cache.join(directory).is_dir() {
                    missing.push(format!(
                        "pinned model snapshot {directory} is not in {}",
                        runtime.model_cache.display()
                    ));
                }
            }
        }
        if missing.is_empty() {
            Ok(runtime)
        } else {
            Err(RuntimeError { missing })
        }
    }

    /// The worker's `--runtime-config` document.
    pub fn worker_configuration(&self) -> Vec<u8> {
        let path = |path: &Path| path.to_string_lossy().into_owned();
        serde_json::to_vec_pretty(&serde_json::json!({
            "runtime_source": path(&self.runtime_source),
            "model_cache": path(&self.model_cache),
            "ffmpeg": path(&self.ffmpeg),
            "ffprobe": path(&self.ffprobe),
        }))
        .expect("string map serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn file(path: &Path) {
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
        let configuration: serde_json::Value =
            serde_json::from_slice(&runtime.worker_configuration()).unwrap();
        assert_eq!(
            configuration
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["ffmpeg", "ffprobe", "model_cache", "runtime_source"]
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
            "pinned model snapshot mlx_gemma_default_text_encoder",
        ] {
            assert!(message.contains(expected), "{expected} in {message}");
        }
        assert!(
            !message.contains("mlx_ltx_q4_pack"),
            "present snapshot not reported"
        );
        assert_eq!(error.missing.len(), 7);
    }

    #[test]
    fn bundles_use_only_explicit_variables() {
        let root = tempfile::tempdir().unwrap();
        // Without the opt-in, even explicit variables are ignored.
        let source = root.path().join("source");
        let variables = BTreeMap::from([(RUNTIME_SOURCE, source.clone())]);
        let error = BridgeRuntime::resolve_with(
            environment(&variables),
            Some(root.path().into()),
            None,
            Lookup::PackagedDisabled,
        )
        .unwrap_err();
        assert_eq!(error.missing.len(), 1);
        assert!(error.missing[0].contains(DEVELOPER_OPT_IN));
        let error = BridgeRuntime::resolve_with(
            |_| None,
            Some(root.path().into()),
            None,
            Lookup::PackagedExplicit,
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
            Lookup::PackagedExplicit,
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
