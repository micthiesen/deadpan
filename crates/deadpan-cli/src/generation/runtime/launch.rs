//! The AI interpreter and its descendants cannot open network connections.
//! This fixed profile leaves files, standard I/O and Metal available. It is
//! independent of process-group ownership and artifact containment.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

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
    #[error("AI network isolation requires macOS")]
    UnsupportedPlatform,
    #[error("the AI Python executable must be an absolute regular file")]
    InvalidPython,
    #[error("the macOS AI network-isolation launcher is unavailable: {0}")]
    Unavailable(std::io::Error),
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
        if !cfg!(target_os = "macos") {
            return Err(LaunchError::UnsupportedPlatform);
        }
        launch(self, configuration, mode, Path::new(SANDBOX))
    }
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
            ffmpeg: PathBuf::new(),
            ffprobe: PathBuf::new(),
            worker_script: PathBuf::from("/private/worker with spaces.py"),
            media_worker: PathBuf::new(),
        }
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
