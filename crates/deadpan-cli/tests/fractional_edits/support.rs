use std::{
    fs::{self, File},
    io::Read,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_source::DecodeControl;
use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const WIRE_LIMIT: usize = 64 * 1024;
const STORAGE_LIMIT: u64 = 1024 * 1024 * 1024;
const OUTPUT_LIMIT: u64 = 128 * 1024 * 1024;

pub struct Run {
    pub root: PathBuf,
    pub deadline: Instant,
    pub cancelled: AtomicBool,
    pub cli_sha256: String,
    pub test_binary_sha256: String,
}

impl Run {
    pub fn new() -> Result<Self> {
        let base = Path::new("/tmp/deadpan-dp02-fractional-20261008");
        fs::create_dir_all(base)?;
        // Retain actual evidence, including a failing fixture, for diagnosis.
        let root = tempfile::Builder::new()
            .prefix("run-")
            .tempdir_in(base)?
            .keep();
        Ok(Self {
            root,
            deadline: Instant::now() + Duration::from_secs(30 * 60),
            cancelled: AtomicBool::new(false),
            cli_sha256: sha256(Path::new(env!("CARGO_BIN_EXE_deadpan-cli")))?,
            test_binary_sha256: sha256(&std::env::current_exe()?)?,
        })
    }

    pub fn check(&self, phase: &str) -> Result {
        if Instant::now() >= self.deadline {
            return Err(format!(
                "30-minute fractional-edit deadline during {phase}; {}",
                self.root.display()
            )
            .into());
        }
        Ok(())
    }

    pub fn control(&self) -> Result<DecodeControl<'_>> {
        self.check("native decode")?;
        Ok(DecodeControl {
            cancelled: &self.cancelled,
            timeout: self
                .deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(30)),
        })
    }

    pub fn check_storage(&self) -> Result<u64> {
        fn visit(path: &Path, count: &mut usize) -> Result<u64> {
            *count += 1;
            if *count > 4096 {
                return Err("fractional fixture exceeded 4096 filesystem entries".into());
            }
            let metadata = match fs::symlink_metadata(path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
                Err(error) => return Err(error.into()),
            };
            if metadata.is_file() {
                return Ok(metadata.len());
            }
            if !metadata.is_dir() {
                return Err(format!("unexpected fixture file type: {}", path.display()).into());
            }
            let mut size = 0_u64;
            // A supervised Render can atomically rename/remove its staging
            // directory during this size observation. Missing entries consume
            // no space at observation time; all other errors remain failures.
            let children = match fs::read_dir(path) {
                Ok(children) => children,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
                Err(error) => return Err(error.into()),
            };
            for child in children {
                size = size
                    .checked_add(visit(&child?.path(), count)?)
                    .ok_or("scratch size overflow")?;
            }
            Ok(size)
        }
        let bytes = visit(&self.root, &mut 0)?;
        if bytes > STORAGE_LIMIT {
            return Err(format!(
                "fractional fixture used {bytes} bytes, over 1 GiB: {}",
                self.root.display()
            )
            .into());
        }
        Ok(bytes)
    }

    pub fn write_json(&self, name: &str, value: &Value) -> Result {
        let bytes = serde_json::to_vec_pretty(value)?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            return Err("fractional evidence exceeded 128 MiB".into());
        }
        fs::write(self.root.join(name), bytes)?;
        Ok(())
    }

    /// File-backed diagnostics avoid pipe deadlocks and bound retained output.
    /// Keep the child unreaped until checked group cleanup has finished.
    pub fn command(&self, name: &str, command: &mut Command) -> Result<PathBuf> {
        self.check(name)?;
        self.write_json(&format!("{name}.command.json"), &serde_json::json!({
            "program": command.get_program().to_string_lossy(),
            "arguments": command.get_args().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),
        }))?;
        let stdout_path = self.root.join(format!("{name}.stdout"));
        let stderr_path = self.root.join(format!("{name}.stderr"));
        let stdout = File::create(&stdout_path)?;
        let stderr = File::create(&stderr_path)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone()?))
            .stderr(Stdio::from(stderr.try_clone()?))
            .process_group(0);
        let mut owned = OwnedCommand {
            child: deadpan_native_process::spawn(command)?,
            reap_attempted: false,
        };
        let outcome = (|| -> Result {
            let mut next_storage_check = Instant::now();
            loop {
                self.check(name)?;
                if stdout.metadata()?.len() > OUTPUT_LIMIT || stderr.metadata()?.len() > 1024 * 1024
                {
                    return Err(format!("{name} exceeded diagnostic output bound").into());
                }
                if Instant::now() >= next_storage_check {
                    self.check_storage()?;
                    next_storage_check = Instant::now() + Duration::from_secs(1);
                }
                if waitid(
                    WaitId::Pid(Pid::from_child(&owned.child)),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
                )?
                .is_some_and(|status| status.exited() || status.killed() || status.dumped())
                {
                    return Ok(());
                }
                std::thread::park_timeout(Duration::from_millis(20));
            }
        })();
        let cleanup = owned.finish(Instant::now() + Duration::from_secs(5));
        let status = match (outcome, cleanup) {
            (Ok(()), Ok(status)) => status,
            (Err(work), Err(cleanup)) => {
                return Err(format!("{name}: {work}; cleanup: {cleanup}").into());
            }
            (Err(error), _) | (_, Err(error)) => return Err(error),
        };
        if !status.success() {
            let mut diagnostic = String::new();
            File::open(&stderr_path)?
                .take(64 * 1024)
                .read_to_string(&mut diagnostic)?;
            return Err(format!(
                "{name}: {status}; {diagnostic}; stdout {}",
                stdout_path.display()
            )
            .into());
        }
        self.check_storage()?;
        Ok(stdout_path)
    }

    pub fn cli(&self, name: &str, arguments: &[&str]) -> Result<PathBuf> {
        self.command(
            name,
            Command::new(env!("CARGO_BIN_EXE_deadpan-cli")).args(arguments),
        )
    }
}

/// Sole reaping ownership survives a failed cleanup until the bounded checked
/// Drop retry. A failed wait is never retried against a potentially reused PID.
struct OwnedCommand {
    child: Child,
    reap_attempted: bool,
}

impl OwnedCommand {
    fn finish(&mut self, deadline: Instant) -> Result<ExitStatus> {
        if self.reap_attempted {
            return Err("fixture child reap already attempted".into());
        }
        let mut issues = Vec::new();
        let group = deadpan_native_process::terminate_owned_group(
            &self.child,
            deadline.min(Instant::now() + Duration::from_secs(2)),
        );
        let can_reap = match group {
            Ok(()) => true,
            Err(error) => {
                issues.push(format!("group cleanup: {error}"));
                match deadpan_native_process::terminate_owned_leader(&self.child, deadline) {
                    Ok(()) => true,
                    Err(error) => {
                        issues.push(format!("leader fallback: {error}"));
                        false
                    }
                }
            }
        };
        let mut status = None;
        if can_reap {
            self.reap_attempted = true;
            match self.child.wait() {
                Ok(value) => status = Some(value),
                Err(error) => issues.push(format!("leader reap: {error}")),
            }
        }
        if !issues.is_empty() {
            return Err(format!(
                "owned fixture process {}: {}",
                self.child.id(),
                issues.join("; ")
            )
            .into());
        }
        status.ok_or_else(|| "fixture cleanup did not prove an exited leader".into())
    }
}

impl Drop for OwnedCommand {
    fn drop(&mut self) {
        if !self.reap_attempted
            && let Err(error) = self.finish(Instant::now() + Duration::from_millis(250))
        {
            eprintln!("fractional fixture cleanup remains unconfirmed: {error}");
        }
    }
}

pub fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| format!("non-UTF8 fixture path {}", path.display()).into())
}

pub fn json_file(path: &Path) -> Result<Value> {
    if fs::metadata(path)?.len() > OUTPUT_LIMIT {
        return Err(format!("oversized JSON: {}", path.display()).into());
    }
    Ok(serde_json::from_reader(File::open(path)?)?)
}

pub fn sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
