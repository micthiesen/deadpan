//! Private workspaces and supervised helper processes for YouTube import.
//!
//! Each import owns one private temporary directory that holds the helpers'
//! HOME/TMPDIR, the inspected metadata, an optional cookie copy and the
//! downloads. It carries an exclusive lock for its owner's lifetime, so a
//! later run can remove the workspaces of processes that died without
//! cleaning up. Helpers run as process-group leaders with a cleared
//! environment, bounded output, a deadline, cancellation and a watchdog.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use rustix::fs::{FlockOperation, flock};
use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};

use super::ImportError;
use crate::CliError;

const PREFIX: &str = "deadpan-youtube-";
const LOCK: &str = "owner.lock";
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_COOKIE_BYTES: u64 = 1024 * 1024;
const CLEANUP_GRACE: Duration = Duration::from_secs(5);
/// Unlocked workspaces younger than this may still be initializing.
const SWEEP_AGE: Duration = Duration::from_secs(60 * 60);

/// The complete helper environment. Nothing is inherited from the caller.
pub fn environment(private: &Path) -> BTreeMap<OsString, OsString> {
    let path = |child: &str| private.join(child).into_os_string();
    BTreeMap::from([
        ("HOME".into(), path("home")),
        ("TMPDIR".into(), path("tmp")),
        ("XDG_CONFIG_HOME".into(), path("home/.config")),
        ("XDG_CACHE_HOME".into(), path("home/.cache")),
        ("XDG_DATA_HOME".into(), path("home/.local/share")),
        ("DENO_DIR".into(), path("home/.deno")),
        ("DENO_NO_UPDATE_CHECK".into(), "1".into()),
        ("NO_COLOR".into(), "1".into()),
        ("LANG".into(), "en_US.UTF-8".into()),
        ("LC_ALL".into(), "en_US.UTF-8".into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ])
}

/// A private directory with the helpers' HOME/TMPDIR layout, locked for its
/// owner's lifetime and removed on drop.
pub struct Workspace {
    directory: tempfile::TempDir,
    _lock: File,
}

impl Workspace {
    pub fn new() -> Result<Self, CliError> {
        Self::new_in(&std::env::temp_dir())
    }

    pub fn new_in(parent: &Path) -> Result<Self, CliError> {
        sweep(parent);
        let directory = tempfile::Builder::new().prefix(PREFIX).tempdir_in(parent)?;
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.path().join(LOCK))?;
        flock(&lock, FlockOperation::NonBlockingLockExclusive).map_err(std::io::Error::from)?;
        for child in ["home/.config", "home/.cache", "tmp", "work", "download"] {
            fs::create_dir_all(directory.path().join(child))?;
        }
        Ok(Self {
            directory,
            _lock: lock,
        })
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }
}

/// Remove workspaces, including any cookie copies, left by import processes
/// that ended without cleanup (for example after SIGKILL). A workspace whose
/// lock is held belongs to a live import and is kept; one without a lock file
/// is removed only once it is old enough not to be initializing.
pub fn sweep(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_str().is_some_and(|name| name.starts_with(PREFIX)) {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() || !owned_by_this_user(&metadata) {
            continue;
        }
        let abandoned = match File::open(path.join(LOCK)) {
            Ok(lock) => flock(&lock, FlockOperation::NonBlockingLockExclusive).is_ok(),
            Err(_) => metadata
                .modified()
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .is_some_and(|age| age >= SWEEP_AGE),
        };
        if abandoned {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

fn owned_by_this_user(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.uid() == rustix::process::getuid().as_raw()
}

/// An owner-only private copy of an explicit cookies file, deleted on drop.
pub struct PrivateCookies {
    path: PathBuf,
}

impl PrivateCookies {
    pub fn copy(source: &Path, workspace: &Workspace) -> Result<Self, ImportError> {
        let unusable = || {
            ImportError::new(
                "YouTubeCookiesInvalid",
                "the cookies file must be a readable regular Netscape cookies file of at most 1 MiB",
            )
        };
        let metadata = fs::metadata(source).map_err(|_| unusable())?;
        if !metadata.is_file() || metadata.len() > MAX_COOKIE_BYTES {
            return Err(unusable());
        }
        let mut bytes = Vec::new();
        File::open(source)
            .and_then(|file| file.take(MAX_COOKIE_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|_| unusable())?;
        if bytes.len() as u64 > MAX_COOKIE_BYTES {
            return Err(unusable());
        }
        let cookies = Self {
            path: workspace.path().join("cookies.txt"),
        };
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&cookies.path)
            .and_then(|mut file| file.write_all(&bytes))
            .map_err(|_| unusable())?;
        Ok(cookies)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateCookies {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct HelperRun {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

#[derive(Default)]
struct Captured {
    bytes: Vec<u8>,
    overflow: bool,
}

/// One supervised helper launch.
pub struct HelperCommand<'a> {
    pub executable: &'a Path,
    pub arguments: &'a [OsString],
    /// The private workspace supplying HOME, TMPDIR and XDG directories.
    pub private: &'a Path,
    pub current_dir: &'a Path,
    pub max_stdout: usize,
    /// The failure reported when stdout exceeds `max_stdout`.
    pub overflow: ImportError,
    pub timeout: Duration,
}

/// Run one helper as a process-group leader with bounded output, deadline,
/// cancellation and a watchdog. The group is torn down before the leader is
/// reaped, including after a normal exit, and output pipes get a bounded
/// grace to close.
pub fn run_helper(
    command: HelperCommand<'_>,
    cancelled: &AtomicBool,
    mut watch: impl FnMut() -> Result<(), ImportError>,
) -> Result<HelperRun, CliError> {
    let mut child = deadpan_native_process::spawn(
        Command::new(command.executable)
            .args(command.arguments)
            .env_clear()
            .envs(environment(command.private))
            .current_dir(command.current_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0),
    )?;
    let stdout = Arc::new(Mutex::new(Captured::default()));
    let stderr = Arc::new(Mutex::new(Captured::default()));
    let pumps = [
        pump(
            child.stdout.take(),
            Arc::clone(&stdout),
            command.max_stdout,
            false,
        ),
        pump(
            child.stderr.take(),
            Arc::clone(&stderr),
            MAX_STDERR_BYTES,
            true,
        ),
    ];
    let deadline = Instant::now() + command.timeout;
    let mut failure = None;
    let mut observation = Ok(());
    loop {
        match leader_exited(&child) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => {
                observation = Err(error);
                break;
            }
        }
        if cancelled.load(Ordering::Acquire) {
            failure = Some(ImportError::new("ImportCancelled", "import was cancelled"));
        } else if Instant::now() >= deadline {
            failure = Some(ImportError::new(
                "DownloaderTimeout",
                format!(
                    "the downloader did not finish within {} s",
                    command.timeout.as_secs()
                ),
            ));
        } else if stdout.lock().map(|c| c.overflow).unwrap_or(true) {
            failure = Some(command.overflow.clone());
        } else if let Err(error) = watch() {
            failure = Some(error);
        }
        if failure.is_some() {
            break;
        }
        std::thread::park_timeout(Duration::from_millis(50));
    }
    let cleanup = stop_group(&child);
    // Reap only a leader whose exit is confirmed without reaping; otherwise
    // waiting could block on, or later signal, a process this run no longer
    // controls. Its pipes then stay with their detached pump threads.
    if !matches!(leader_exited(&child), Ok(true)) {
        return Err(cleanup
            .err()
            .unwrap_or_else(|| std::io::Error::other("helper leader did not stop"))
            .into());
    }
    let status = child.wait();
    // A descendant that escaped the group can keep a pipe open; never wait
    // for it indefinitely.
    let drained = Instant::now() + CLEANUP_GRACE;
    while pumps.iter().flatten().any(|pump| !pump.is_finished()) && Instant::now() < drained {
        std::thread::park_timeout(Duration::from_millis(10));
    }
    let closed = pumps.iter().flatten().all(JoinHandle::is_finished);
    for pump in pumps.into_iter().flatten().filter(JoinHandle::is_finished) {
        let _ = pump.join();
    }
    if let Some(failure) = failure {
        return Err(failure.into());
    }
    observation?;
    cleanup?;
    let status = status?;
    if !closed {
        return Err(ImportError::new(
            "DownloaderFailed",
            "a process left behind by the downloader kept its output open",
        )
        .into());
    }
    let take = |captured: &Arc<Mutex<Captured>>| {
        captured
            .lock()
            .map(|mut captured| std::mem::take(&mut captured.bytes))
            .unwrap_or_default()
    };
    Ok(HelperRun {
        status,
        stdout: take(&stdout),
        stderr: String::from_utf8_lossy(&take(&stderr)).into_owned(),
    })
}

/// Whether the owned leader has exited, observed without reaping it.
fn leader_exited(child: &Child) -> std::io::Result<bool> {
    Ok(waitid(
        WaitId::Pid(Pid::from_child(child)),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )?
    .is_some())
}

fn stop_group(child: &Child) -> std::io::Result<()> {
    let deadline = Instant::now() + CLEANUP_GRACE;
    #[cfg(target_os = "macos")]
    let group = deadpan_native_process::terminate_owned_group(child, deadline);
    #[cfg(target_os = "linux")]
    let group = deadpan_native_process::signal_owned_group(child)
        .and_then(|()| deadpan_native_process::terminate_owned_leader(child, deadline));
    // Group inspection failure must not strand the leader.
    group.or_else(|error| {
        deadpan_native_process::terminate_owned_leader(child, deadline)?;
        Err(error)
    })
}

fn pump(
    source: Option<impl Read + Send + 'static>,
    target: Arc<Mutex<Captured>>,
    limit: usize,
    keep_tail: bool,
) -> Option<JoinHandle<()>> {
    let mut source = source?;
    std::thread::Builder::new()
        .name("deadpan-helper-pipe".into())
        .spawn(move || {
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let count = match source.read(&mut buffer) {
                    Ok(0) | Err(_) => return,
                    Ok(count) => count,
                };
                let Ok(mut captured) = target.lock() else {
                    return;
                };
                if keep_tail {
                    captured.bytes.extend_from_slice(&buffer[..count]);
                    let excess = captured.bytes.len().saturating_sub(limit);
                    captured.bytes.drain(..excess);
                } else if captured.bytes.len() + count > limit {
                    captured.overflow = true;
                } else {
                    captured.bytes.extend_from_slice(&buffer[..count]);
                }
            }
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_removes_only_abandoned_workspaces() {
        let parent = tempfile::tempdir().unwrap();
        let live = Workspace::new_in(parent.path()).unwrap();
        let abandoned = parent.path().join(format!("{PREFIX}dead"));
        fs::create_dir_all(abandoned.join("download")).unwrap();
        fs::write(abandoned.join(LOCK), b"").unwrap();
        fs::write(abandoned.join("cookies.txt"), b"secret").unwrap();
        let initializing = parent.path().join(format!("{PREFIX}young"));
        fs::create_dir(&initializing).unwrap();
        let unrelated = parent.path().join("other");
        fs::create_dir(&unrelated).unwrap();

        // Creating another workspace sweeps first.
        let second = Workspace::new_in(parent.path()).unwrap();
        assert!(!abandoned.exists());
        assert!(live.path().exists() && second.path().exists());
        assert!(initializing.exists() && unrelated.exists());
        let path = live.path().to_owned();
        drop(live);
        assert!(!path.exists());
    }

    #[test]
    fn stdout_overflow_reports_the_callers_code() {
        let workspace = Workspace::new().unwrap();
        let error = run_helper(
            HelperCommand {
                executable: Path::new("/usr/bin/yes"),
                arguments: &[],
                private: workspace.path(),
                current_dir: workspace.path(),
                max_stdout: 1024,
                overflow: ImportError::new("DownloaderOutputTooLarge", "too much"),
                timeout: Duration::from_secs(30),
            },
            &AtomicBool::new(false),
            || Ok(()),
        )
        .err()
        .unwrap();
        assert!(matches!(error, CliError::Import(e) if e.code == "DownloaderOutputTooLarge"));
    }

    #[test]
    fn escaped_descendants_cannot_hold_the_run_open() {
        let workspace = Workspace::new().unwrap();
        // The descendant leaves the process group and keeps stdout open.
        let started = Instant::now();
        let error = run_helper(
            HelperCommand {
                executable: Path::new("/bin/sh"),
                arguments: &[
                    "-c".into(),
                    "/usr/bin/perl -e 'setpgrp(0,0); sleep 20' &".into(),
                ],
                private: workspace.path(),
                current_dir: workspace.path(),
                max_stdout: 1024,
                overflow: ImportError::new("DownloaderOutputTooLarge", "too much"),
                timeout: Duration::from_secs(30),
            },
            &AtomicBool::new(false),
            || Ok(()),
        )
        .err()
        .unwrap();
        assert!(matches!(error, CliError::Import(e) if e.code == "DownloaderFailed"));
        assert!(started.elapsed() < Duration::from_secs(15));
    }
}
