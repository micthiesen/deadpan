//! Requests and published state of project backups (docs/BACKUPS.md).
//!
//! The project service backs up the committed database on its own thread:
//! periodically while edits are being saved, when a session closes with
//! edits since the last backup, and when asked. A restore replaces the
//! database on the writer and starts a new session. None of this is an
//! authored edit.

use deadpan_cli::backup_settings::{Settings, Source as SettingsSource};
use deadpan_store::backups::{BackupInfo, BackupReason};

#[derive(Clone, Debug)]
pub enum Request {
    /// Back up now (`B` in the Storage panel).
    Now { ticket: u64, expected_session: u64 },
    /// Replace the project with the named backup, after backing up the
    /// current state.
    Restore {
        ticket: u64,
        expected_session: u64,
        id: String,
    },
    /// Read the global per-user policy, independent of project/session state.
    LoadSettings { ticket: u64 },
    /// Validate and persist the global per-user policy.
    SaveSettings { ticket: u64, settings: Settings },
}

/// What the latest backup work of this app did, for the current session.
#[derive(Clone, Debug, Default)]
pub struct Update {
    pub session: u64,
    /// Test/replay observation of all owned backup threads, including copies
    /// detached by Close. A closed workspace alone does not mean they finished.
    #[cfg(any(test, feature = "ui-harness"))]
    pub owned_workers_active_for_check: bool,
    /// The newest backup this app published for the project, with the
    /// revision it holds.
    pub latest: Option<(BackupInfo, Option<String>)>,
    /// A backup copying now, and why.
    pub running: Option<BackupReason>,
    /// The last automatic backup failed; cleared by the next success.
    pub failure: Option<String>,
    /// The reply to an explicit request, matched by its ticket.
    pub reply: Option<Reply>,
    /// Global backup settings; never scoped to the open project session.
    pub settings: SettingsUpdate,
}

#[derive(Clone, Debug)]
pub enum SettingsStatus {
    Loading,
    Ready {
        source: SettingsSource,
        warning: Option<String>,
    },
    Saving,
    Saved {
        warning: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct SettingsUpdate {
    pub ticket: u64,
    pub settings: Settings,
    /// False means the app could not trust the saved file and must not prune.
    pub trusted: bool,
    pub status: SettingsStatus,
}

impl Default for SettingsUpdate {
    fn default() -> Self {
        Self {
            ticket: 0,
            settings: Settings::default(),
            trusted: false,
            status: SettingsStatus::Loading,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Reply {
    pub ticket: u64,
    pub result: Result<String, String>,
}

/// "3 min ago" style age of a backup, from its creation time.
pub fn age(created_unix_ms: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(created_unix_ms);
    let seconds = now.saturating_sub(created_unix_ms) / 1000;
    match seconds {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86_399 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}
