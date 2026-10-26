//! The AI interpreter and its descendants cannot open network connections.
//! This fixed profile leaves files, standard I/O and Metal available. It is
//! independent of process-group ownership and artifact containment.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::BridgeRuntime;

const SANDBOX: &str = "/usr/bin/sandbox-exec";
const PROFILE: &str = "(version 1)(allow default)(deny network*)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    Inference,
    Check,
}

/// Pass these arguments unchanged to the existing owned process runner.
#[derive(Debug)]
pub struct WorkerLaunch {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("the packaged AI host is damaged: {0}")]
    InvalidHost(std::io::Error),
    #[error("the AI runtime could not be admitted: {0}")]
    Admission(std::io::Error),
    #[error("AI network isolation requires macOS")]
    UnsupportedPlatform,
    #[error("the AI Python executable must be an absolute regular file")]
    InvalidPython,
    #[error("the macOS AI network-isolation launcher is unavailable: {0}")]
    Unavailable(std::io::Error),
}

impl LaunchError {
    pub(crate) fn io_kind(&self) -> Option<std::io::ErrorKind> {
        match self {
            Self::InvalidHost(error) | Self::Admission(error) | Self::Unavailable(error) => {
                Some(error.kind())
            }
            Self::UnsupportedPlatform | Self::InvalidPython => None,
        }
    }
}

impl BridgeRuntime {
    /// Both inference and model-pack checks use this launch boundary,
    /// including development and explicit developer runtimes. There is no
    /// fallback to executing Python without the network restriction.
    pub fn worker_launch(
        &self,
        configuration: &Path,
        mode: WorkerMode,
    ) -> Result<WorkerLaunch, LaunchError> {
        self.worker_launch_interruptible(
            configuration,
            mode,
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(60),
        )
    }

    /// The caller's budget covers admission as well as the worker that follows.
    /// Heavy package verification runs here on preparation/CLI threads, never
    /// during lightweight runtime selection on the UI or project writer.
    pub fn worker_launch_interruptible(
        &self,
        configuration: &Path,
        mode: WorkerMode,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<WorkerLaunch, LaunchError> {
        check_control(cancelled, deadline)?;
        let contents =
            crate::bundle::checked_packaged_contents().map_err(LaunchError::InvalidHost)?;
        if !cfg!(target_os = "macos") {
            return Err(LaunchError::UnsupportedPlatform);
        }
        if let Some(contents) = contents {
            let paths = crate::bundle::AiRuntimePaths {
                python: &self.python,
                worker_script: &self.worker_script,
                runtime_source: &self.runtime_source,
                ffmpeg: &self.ffmpeg,
                ffprobe: &self.ffprobe,
            };
            let external = std::env::var_os(super::DEVELOPER_OPT_IN)
                .is_some_and(|value| value == "1")
                && all_external(
                    &contents,
                    &[
                        &self.python,
                        &self.worker_script,
                        &self.runtime_source,
                        &self.ffmpeg,
                        &self.ffprobe,
                    ],
                    || check_control(cancelled, deadline),
                )?;
            if external {
                crate::bundle::qualify_ai_host(cancelled, deadline)
            } else {
                crate::bundle::qualify_ai_runtime(paths, cancelled, deadline)
            }
            .map_err(LaunchError::Admission)?;
        }
        check_control(cancelled, deadline)?;
        let launch = launch(self, configuration, mode, Path::new(SANDBOX))?;
        check_control(cancelled, deadline)?;
        Ok(launch)
    }
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), LaunchError> {
    let error = if cancelled.load(Ordering::Acquire) {
        Some((
            std::io::ErrorKind::Interrupted,
            "AI runtime admission cancelled",
        ))
    } else if Instant::now() >= deadline {
        Some((
            std::io::ErrorKind::TimedOut,
            "AI runtime admission deadline elapsed",
        ))
    } else {
        None
    };
    match error {
        Some((kind, message)) => Err(LaunchError::Admission(std::io::Error::new(kind, message))),
        None => Ok(()),
    }
}

/// Only a wholly external, explicitly selected developer runtime can opt out
/// of the private inventory. Canonical resolution prevents aliases into the
/// application from turning bundled code into an apparent external runtime.
fn all_external(
    contents: &Path,
    paths: &[&Path],
    mut check: impl FnMut() -> Result<(), LaunchError>,
) -> Result<bool, LaunchError> {
    let app = contents.parent().ok_or_else(|| {
        LaunchError::Admission(std::io::Error::other("AI application has no parent"))
    })?;
    check()?;
    let canonical_app = app.canonicalize().map_err(LaunchError::Admission)?;
    for path in paths {
        check()?;
        if path.starts_with(app) {
            return Ok(false);
        }
        let resolved = path.canonicalize().map_err(LaunchError::Admission)?;
        check()?;
        if resolved.starts_with(&canonical_app) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn launch(
    runtime: &BridgeRuntime,
    configuration: &Path,
    mode: WorkerMode,
    sandbox: &Path,
) -> Result<WorkerLaunch, LaunchError> {
    // Wrapping must not weaken the supervisor's original executable check.
    if !runtime.python.is_absolute() || !runtime.python.is_file() {
        return Err(LaunchError::InvalidPython);
    }
    if !std::fs::metadata(sandbox)
        .map_err(LaunchError::Unavailable)?
        .is_file()
    {
        return Err(LaunchError::Unavailable(std::io::Error::other(
            "not a regular file",
        )));
    }
    let mut arguments = vec![
        "-p".into(),
        PROFILE.into(),
        runtime.python.clone().into_os_string(),
        "-I".into(),
        // The bundled runtime is signed and read-only; bytecode is precompiled.
        "-B".into(),
        runtime.worker_script.clone().into_os_string(),
        "--runtime-config".into(),
        configuration.as_os_str().to_owned(),
    ];
    if mode == WorkerMode::Check {
        arguments.push("--check".into());
    }
    Ok(WorkerLaunch {
        executable: sandbox.to_owned(),
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(python: PathBuf) -> BridgeRuntime {
        BridgeRuntime {
            python,
            runtime_source: PathBuf::new(),
            model_cache: PathBuf::new(),
            model_manifest: deadpan_models::packs::approved_pack("ltx-2.3-q4-bridge").unwrap(),
            ffmpeg: PathBuf::new(),
            ffprobe: PathBuf::new(),
            worker_script: PathBuf::from("/private/worker with spaces.py"),
            media_worker: PathBuf::new(),
            landmark_worker: PathBuf::new(),
        }
    }

    #[test]
    fn cancellation_and_expired_budget_refuse_before_runtime_lookup_or_launch() {
        let runtime = runtime(PathBuf::from("/missing-python"));
        for (cancelled, deadline, expected) in [
            (
                true,
                Instant::now() + Duration::from_secs(60),
                std::io::ErrorKind::Interrupted,
            ),
            (false, Instant::now(), std::io::ErrorKind::TimedOut),
        ] {
            for mode in [WorkerMode::Check, WorkerMode::Inference] {
                let error = runtime
                    .worker_launch_interruptible(
                        Path::new("/missing-configuration"),
                        mode,
                        &AtomicBool::new(cancelled),
                        deadline,
                    )
                    .unwrap_err();
                assert_eq!(error.io_kind(), Some(expected));
            }
        }
    }

    #[test]
    fn developer_inventory_opt_out_rejects_bundled_components_and_aliases() {
        let scratch = tempfile::tempdir().unwrap();
        let contents = scratch.path().join("Deadpan.app/Contents");
        std::fs::create_dir_all(&contents).unwrap();
        let bundled = contents.join("runtime.py");
        let external = scratch.path().join("developer.py");
        let alias = scratch.path().join("runtime-alias.py");
        std::fs::write(&bundled, b"bundled").unwrap();
        std::fs::write(&external, b"external").unwrap();
        std::os::unix::fs::symlink(&bundled, &alias).unwrap();
        assert!(all_external(&contents, &[&external], || Ok(())).unwrap());
        for paths in [
            vec![bundled.as_path()],
            vec![alias.as_path()],
            vec![external.as_path(), bundled.as_path()],
        ] {
            assert!(!all_external(&contents, &paths, || Ok(())).unwrap());
        }
        // A package-owned alias escaping the app is still bundled selection.
        let escape = contents.join("escape.py");
        std::os::unix::fs::symlink(&external, &escape).unwrap();
        assert!(!all_external(&contents, &[&escape], || Ok(())).unwrap());
    }

    #[test]
    fn ai_network_launcher_refuses_missing_isolation_and_invalid_python() {
        let scratch = tempfile::tempdir().unwrap();
        let python = scratch.path().join("python");
        std::fs::write(&python, b"stand-in").unwrap();
        assert!(matches!(
            launch(
                &runtime(python.clone()),
                Path::new("/runtime.json"),
                WorkerMode::Check,
                &scratch.path().join("missing-sandbox")
            ),
            Err(LaunchError::Unavailable(_))
        ));
        assert!(matches!(
            launch(
                &runtime(PathBuf::from("python")),
                Path::new("/runtime.json"),
                WorkerMode::Inference,
                &python
            ),
            Err(LaunchError::InvalidPython)
        ));
        let launch = launch(
            &runtime(python.clone()),
            Path::new("/private/config with spaces.json"),
            WorkerMode::Check,
            &python,
        )
        .unwrap();
        assert_eq!(launch.arguments[0], "-p");
        assert_eq!(launch.arguments[1], PROFILE);
        assert_eq!(launch.arguments[2], python.as_os_str());
        assert_eq!(launch.arguments[5], "/private/worker with spaces.py");
        assert_eq!(launch.arguments[7], "/private/config with spaces.json");
        assert_eq!(launch.arguments[8], "--check");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn ai_network_launcher_refuses_unsupported_platforms() {
        assert!(matches!(
            runtime(PathBuf::from("/usr/bin/python3"))
                .worker_launch(Path::new("/runtime.json"), WorkerMode::Inference),
            Err(LaunchError::UnsupportedPlatform)
        ));
    }
}
