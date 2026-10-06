//! Truthful recovery and storage-failure wording, and the launch journal that
//! tells the next launch whether Deadpan closed its project normally.
//!
//! Nothing here edits a project. Every committed edit is already durable in
//! SQLite (see `docs/RECOVERY.md`); these helpers only explain failures and
//! offer the person the last consistent state.

use std::io::{Read as _, Write as _};
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use deadpan_store::StoreError;
use serde::{Deserialize, Serialize};

const DISK_FULL: &str = "Not saved: the disk is full.";
const READ_ONLY: &str = "Not saved: this project cannot be written";
const DENIED: &str = "Not saved: macOS denied access to the project files.";

/// The person-facing explanation of a store error, with a suggested action
/// for the codes in Section 28.2. Other errors keep their own wording. The
/// storage headlines carry the classification inside the message itself,
/// so `storage_code` recovers it on any thread that sees the message.
pub fn describe_store_error(error: &StoreError) -> String {
    match (error.code(), error) {
        (_, StoreError::UnsupportedSchema(version))
            if *version > deadpan_store::DATABASE_SCHEMA_VERSION =>
        {
            format!(
                "This project was saved by a newer Deadpan (format {version}; this build reads format {}). Nothing was changed. Open it with that newer version.",
                deadpan_store::DATABASE_SCHEMA_VERSION
            )
        }
        (_, StoreError::NewerSchema { .. }) => format!(
            "{error}. Open it with that newer Deadpan to edit it; this build can only show it."
        ),
        (_, StoreError::UnsupportedSchema(version)) => format!(
            "This project uses development format {version}, which this build no longer opens (it reads format {}). Nothing was changed and no backup was made. Create a new project from its Original to continue.",
            deadpan_store::DATABASE_SCHEMA_VERSION
        ),
        (_, StoreError::ReadOnlyLocation(_)) => error.to_string(),
        ("DiskFull", _) => format!(
            "{DISK_FULL} Your last saved edit is intact. Free space on the disk that holds this project, then repeat the action. ({error})"
        ),
        ("ProjectReadOnly", _) => format!(
            "{READ_ONLY} (read-only file or volume). Your last saved edit is intact. Check its permissions or copy it to a writable folder, then reopen it. ({error})"
        ),
        ("PermissionDenied", _) => format!(
            "{DENIED} Your last saved edit is intact. Restore access to the project folder, then repeat the action. ({error})"
        ),
        ("OriginalOffline", _) => format!(
            "The Original's file is missing. Locate it with :relink; nothing in the project was changed. ({error})"
        ),
        ("OriginalContentMismatch", _) => format!(
            "That file is not this project's Original: its content differs, so it was not used. Choose the exact original file. ({error})"
        ),
        ("ProjectAlreadyOpen", _) => format!(
            "{error}. If Deadpan is open in another window or a headless command is running, finish there first."
        ),
        _ => error.to_string(),
    }
}

/// The storage failure a message reports, from its headline, or from
/// SQLite's own wording when a store error reached the message unexplained.
pub fn storage_code(message: &str) -> Option<&'static str> {
    if message.contains(DISK_FULL) || message.contains("database or disk is full") {
        Some("DiskFull")
    } else if message.contains(READ_ONLY)
        || message.contains("attempt to write a readonly database")
    {
        Some("ProjectReadOnly")
    } else if message.contains(DENIED) {
        Some("PermissionDenied")
    } else {
        None
    }
}

/// A persistent "Not saved" state. It clears when a later save succeeds or
/// the project closes; it never claims the failed action was saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageAlert {
    pub session: u64,
    pub code: &'static str,
    /// The committed revision that remained current after the failure.
    pub revision: Option<deadpan_core::RevisionId>,
    pub message: String,
}

impl StorageAlert {
    pub fn headline(&self) -> &'static str {
        match self.code {
            "DiskFull" => "Not saved · disk full",
            "ProjectReadOnly" => "Not saved · project is read-only",
            _ => "Not saved · access denied",
        }
    }
}

/// One running app instance with a project open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Instance {
    pid: u32,
    /// Distinguishes a later process that reuses the same ID.
    launch: String,
    /// Raw path bytes, so non-UTF-8 paths survive.
    project: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchRecord {
    version: u32,
    /// Instances that opened a project and have not closed it.
    open: Vec<Instance>,
}

const LAUNCH_VERSION: u32 = 2;
const MAX_LAUNCH_BYTES: u64 = 64 * 1024;
const MAX_INSTANCES: usize = 16;

/// `Application Support/Deadpan/session.json` (0600, atomic replace under a
/// lock): each running instance's open project path. No project content.
#[derive(Clone, Debug)]
pub struct LaunchJournal {
    path: PathBuf,
    pid: u32,
    launch: String,
}

/// A project an instance that is no longer running left open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchOffer {
    pub project: PathBuf,
    pid: u32,
    launch: String,
}

impl LaunchJournal {
    /// The user's journal. Replay and tests use `at` with a private path.
    pub fn user() -> Result<Self, String> {
        Ok(Self::at(
            crate::keymap_file::application_support_directory()?.join("Deadpan/session.json"),
        ))
    }

    pub fn at(path: PathBuf) -> Self {
        Self {
            path,
            pid: std::process::id(),
            launch: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// A journal writer posing as an instance whose process has exited, for
    /// reproducing what a crashed launch leaves behind.
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn for_exited_process(path: PathBuf) -> Result<Self, String> {
        let mut child = std::process::Command::new("/usr/bin/true")
            .spawn()
            .map_err(|error| error.to_string())?;
        let pid = child.id();
        child.wait().map_err(|error| error.to_string())?;
        Ok(Self {
            pid,
            ..Self::at(path)
        })
    }

    /// The newest project left open by an instance whose process is gone,
    /// with its package still present. A running instance is never offered.
    pub fn unclean_offer(&self) -> Option<LaunchOffer> {
        let record = self.read()?;
        record.open.iter().rev().find_map(|instance| {
            let project = PathBuf::from(std::ffi::OsString::from_vec(instance.project.clone()));
            (instance.launch != self.launch
                && !process_running(instance.pid)
                && project.join("project.sqlite").is_file())
            .then(|| LaunchOffer {
                project,
                pid: instance.pid,
                launch: instance.launch.clone(),
            })
        })
    }

    /// The offer was answered: forget that instance's record.
    pub fn dismiss(&self, offer: &LaunchOffer) -> Result<(), String> {
        self.update(|record| {
            record
                .open
                .retain(|instance| !(instance.pid == offer.pid && instance.launch == offer.launch));
        })
    }

    pub fn record_open(&self, project: &Path) -> Result<(), String> {
        self.update(|record| {
            record
                .open
                .retain(|instance| instance.launch != self.launch);
            record.open.push(Instance {
                pid: self.pid,
                launch: self.launch.clone(),
                project: project.as_os_str().as_bytes().to_vec(),
            });
            let excess = record.open.len().saturating_sub(MAX_INSTANCES);
            record.open.drain(..excess);
        })
    }

    /// A normal close removes only this instance's record.
    pub fn record_closed(&self) -> Result<(), String> {
        self.update(|record| {
            record
                .open
                .retain(|instance| instance.launch != self.launch)
        })
    }

    /// Projects every instance currently records as open.
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn open_projects(&self) -> Vec<PathBuf> {
        self.read().map_or_else(Vec::new, |record| {
            record
                .open
                .into_iter()
                .map(|instance| PathBuf::from(std::ffi::OsString::from_vec(instance.project)))
                .collect()
        })
    }

    fn read(&self) -> Option<LaunchRecord> {
        let file = std::fs::File::open(&self.path).ok()?;
        let mut bytes = Vec::new();
        file.take(MAX_LAUNCH_BYTES).read_to_end(&mut bytes).ok()?;
        let record: LaunchRecord = serde_json::from_slice(&bytes).ok()?;
        (record.version == LAUNCH_VERSION).then_some(record)
    }

    /// Read-modify-write under an exclusive lock shared by every instance.
    fn update(&self, change: impl FnOnce(&mut LaunchRecord)) -> Result<(), String> {
        let directory = self
            .path
            .parent()
            .ok_or("The launch journal has no directory")?;
        let failed = |error: std::io::Error| {
            format!(
                "Cannot record the open project in {}: {error}",
                self.path.display()
            )
        };
        std::fs::create_dir_all(directory).map_err(failed)?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join("session.lock"))
            .map_err(failed)?;
        lock.lock().map_err(failed)?;
        let mut record = self.read().unwrap_or_default();
        record.version = LAUNCH_VERSION;
        change(&mut record);
        let temporary = directory.join(format!(".session-{}.tmp", uuid::Uuid::new_v4()));
        let written = (|| -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            serde_json::to_writer(&mut file, &record)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)
        })();
        let _ = lock.unlock();
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(failed(error));
        }
        Ok(())
    }
}

/// Whether a process with this ID exists (EPERM still means it exists). A
/// reused ID reads as running, which only suppresses an offer.
fn process_running(pid: u32) -> bool {
    let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    else {
        return false;
    };
    !matches!(
        rustix::process::test_kill_process(pid),
        Err(rustix::io::Errno::SRCH)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_failures_carry_their_classification_in_the_message() {
        let full = StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL),
            Some("database or disk is full".into()),
        ));
        let text = describe_store_error(&full);
        assert!(text.starts_with("Not saved: the disk is full."));
        assert!(text.contains("last saved edit is intact"));
        assert_eq!(storage_code(&text), Some("DiskFull"));
        // Wrapped or unexplained, the classification survives.
        assert_eq!(
            storage_code(&format!("Cut saved, but: {text}")),
            Some("DiskFull")
        );
        assert_eq!(storage_code(&full.to_string()), Some("DiskFull"));
        let quota = StoreError::Io(std::io::Error::from(std::io::ErrorKind::QuotaExceeded));
        assert_eq!(
            storage_code(&describe_store_error(&quota)),
            Some("DiskFull")
        );

        let read_only =
            StoreError::Io(std::io::Error::from(std::io::ErrorKind::ReadOnlyFilesystem));
        let text = describe_store_error(&read_only);
        assert!(text.starts_with("Not saved: this project cannot"));
        assert_eq!(storage_code(&text), Some("ProjectReadOnly"));

        let denied = StoreError::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        let text = describe_store_error(&denied);
        assert!(text.starts_with("Not saved: macOS denied"));
        assert_eq!(storage_code(&text), Some("PermissionDenied"));

        // Ordinary failures neither alarm nor change their wording.
        let ordinary = StoreError::NothingToUndo;
        assert_eq!(describe_store_error(&ordinary), ordinary.to_string());
        assert_eq!(storage_code(&describe_store_error(&ordinary)), None);
        // An export destination's own permissions are not project storage.
        assert_eq!(storage_code("destination_io: Permission denied"), None);
    }

    #[test]
    fn obsolete_and_newer_formats_explain_that_nothing_changed() {
        let old = describe_store_error(&StoreError::UnsupportedSchema(38));
        assert!(old.contains("development format 38"));
        assert!(old.contains("Nothing was changed"));
        let newer = describe_store_error(&StoreError::UnsupportedSchema(
            deadpan_store::DATABASE_SCHEMA_VERSION + 1,
        ));
        assert!(newer.contains("newer Deadpan"));
        assert!(newer.contains("Nothing was changed"));
        let newer = describe_store_error(&StoreError::NewerSchema {
            found: deadpan_store::DATABASE_SCHEMA_VERSION + 1,
            supported: deadpan_store::DATABASE_SCHEMA_VERSION,
        });
        assert!(newer.contains("read-only"), "{newer}");
        assert!(newer.contains("nothing in it has been changed"), "{newer}");
    }

    fn package(root: &Path, name: &str) -> PathBuf {
        let project = root.join(name);
        std::fs::create_dir(&project).unwrap();
        std::fs::write(project.join("project.sqlite"), b"").unwrap();
        project
    }

    #[test]
    fn only_a_project_left_open_by_a_gone_instance_is_offered() {
        use std::os::unix::fs::PermissionsExt as _;
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("Support/Deadpan/session.json");
        let current = LaunchJournal::at(path.clone());
        assert_eq!(current.unclean_offer(), None);
        let project = package(scratch.path(), "Clip.deadpan");

        // A running instance (this process) with a project open is never
        // offered to another instance, and its record survives their closes.
        let running = LaunchJournal::at(path.clone());
        running.record_open(&project).unwrap();
        assert_eq!(current.unclean_offer(), None);
        current.record_closed().unwrap();
        assert_eq!(current.open_projects(), vec![project.clone()]);
        running.record_closed().unwrap();
        assert!(current.open_projects().is_empty());

        // An instance whose process is gone left its project open.
        let crashed = LaunchJournal::for_exited_process(path.clone()).unwrap();
        crashed.record_open(&project).unwrap();
        let offer = current.unclean_offer().expect("crashed instance offered");
        assert_eq!(offer.project, project);
        current.dismiss(&offer).unwrap();
        assert_eq!(current.unclean_offer(), None);

        // Non-UTF-8 path bytes round-trip (APFS itself refuses such names).
        let raw = PathBuf::from(std::ffi::OsString::from_vec(
            b"/Volumes/caf\xe9.deadpan".to_vec(),
        ));
        crashed.record_open(&raw).unwrap();
        assert_eq!(current.open_projects(), vec![raw]);
        // A removed package is not offered.
        assert_eq!(current.unclean_offer(), None);
        crashed.record_open(&project).unwrap();
        std::fs::remove_dir_all(&project).unwrap();
        assert_eq!(current.unclean_offer(), None);

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        // A damaged journal is ignored.
        std::fs::write(&path, b"{").unwrap();
        assert_eq!(current.unclean_offer(), None);
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
