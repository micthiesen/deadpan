//! Automatic, closing and explicit backups, and restore, for the project
//! this service owns (docs/BACKUPS.md).
//!
//! A backup copies the committed database through SQLite's backup API on its
//! own thread with its own read-only connection, then verifies and publishes
//! it under `Backups/`. The writer is never borrowed, so editing continues
//! while it runs. One backup runs at a time. The service starts one:
//!
//! - periodically, once the head differs from what the latest backup holds
//!   and the policy interval has passed since the session opened or the last
//!   backup finished;
//! - when a session closes (another project opens, Close, quit) with saved
//!   edits the latest backup does not hold. That copy runs detached; the
//!   service waits for it only briefly when the app quits;
//! - when asked (`B` in the Storage panel).
//!
//! Restoring runs on this thread because it replaces the writer's database:
//! it is refused while any job of the session runs, backs up the current
//! state first, and then starts a new session so that nothing derived from
//! the replaced revisions survives.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use deadpan_store::AccessMode;
use deadpan_store::backups::{
    BackupLimits, BackupOutcome, BackupPolicy, BackupReason, create_backup, list_backups,
};

use super::super::backups::{Reply, Request, SettingsStatus, SettingsUpdate, Update, age};
use super::{Result, Service, recovery, registers, snapshot};

/// How long quitting waits for a closing backup still copying.
const EXIT_WAIT: Duration = Duration::from_millis(1500);

type Outcome = mpsc::Receiver<std::result::Result<BackupOutcome, String>>;

enum SettingsJob {
    Load {
        ticket: u64,
        path: PathBuf,
    },
    Save {
        ticket: u64,
        settings: deadpan_cli::backup_settings::Settings,
        path: PathBuf,
    },
}

impl SettingsJob {
    fn ticket(&self) -> u64 {
        match self {
            Self::Load { ticket, .. } | Self::Save { ticket, .. } => *ticket,
        }
    }
}

enum SettingsResult {
    Loaded {
        ticket: u64,
        result: std::result::Result<deadpan_cli::backup_settings::Loaded, String>,
    },
    Saved {
        ticket: u64,
        settings: deadpan_cli::backup_settings::Settings,
        result: std::result::Result<deadpan_cli::backup_settings::SaveOutcome, String>,
    },
}

struct SettingsWorker {
    jobs: SyncSender<SettingsJob>,
    results: Receiver<SettingsResult>,
    _handle: JoinHandle<()>,
}

impl SettingsWorker {
    fn start() -> std::io::Result<Self> {
        let (job_sender, job_receiver) = mpsc::sync_channel::<SettingsJob>(1);
        let (result_sender, result_receiver) = mpsc::sync_channel::<SettingsResult>(1);
        let handle = std::thread::Builder::new()
            .name("deadpan-backup-settings".into())
            .spawn(move || {
                while let Ok(job) = job_receiver.recv() {
                    let result = match job {
                        SettingsJob::Load { ticket, path } => SettingsResult::Loaded {
                            ticket,
                            result: deadpan_cli::backup_settings::Settings::load_from(&path)
                                .map_err(|error| error.to_string()),
                        },
                        SettingsJob::Save {
                            ticket,
                            settings,
                            path,
                        } => SettingsResult::Saved {
                            ticket,
                            result: settings.save_to(&path).map_err(|error| error.to_string()),
                            settings,
                        },
                    };
                    if result_sender.send(result).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            jobs: job_sender,
            results: result_receiver,
            _handle: handle,
        })
    }

    fn submit(&self, job: SettingsJob) -> std::result::Result<(), String> {
        self.jobs
            .try_send(job)
            .map_err(|error| format!("Backup settings worker is busy: {error}"))
    }

    fn try_recv(&self) -> std::result::Result<SettingsResult, TryRecvError> {
        self.results.try_recv()
    }
}

struct Running {
    session: u64,
    /// The database change version when the copy started.
    version: Option<i64>,
    reason: BackupReason,
    ticket: Option<u64>,
    cancel: Arc<AtomicBool>,
    receiver: Outcome,
    handle: JoinHandle<()>,
}

pub(super) struct State {
    policy: BackupPolicy,
    settings: SettingsUpdate,
    settings_worker: Option<SettingsWorker>,
    settings_in_flight: Option<u64>,
    pending_settings: VecDeque<SettingsJob>,
    running: Option<Running>,
    /// The database change version the latest backup holds, per session;
    /// `None` until a backup of this session or the existing folder says
    /// otherwise. Any commit changes it, authored or operational.
    covered: Option<(u64, i64)>,
    /// Watches this session's database for commits.
    monitor: Option<(u64, deadpan_store::backups::ChangeMonitor)>,
    /// Consecutive failed automatic backups; each doubles the wait.
    failures: u32,
    /// When the session opened or the latest backup finished.
    since: Instant,
    /// Closing backups of earlier sessions, still copying.
    detached: Vec<JoinHandle<()>>,
    update: Update,
}

impl Default for State {
    fn default() -> Self {
        let settings_worker = SettingsWorker::start().ok();
        let settings = SettingsUpdate::default();
        let policy = settings.settings.policy_without_pruning();
        let mut state = Self {
            policy,
            settings,
            settings_worker,
            settings_in_flight: None,
            pending_settings: VecDeque::new(),
            running: None,
            covered: None,
            monitor: None,
            failures: 0,
            since: Instant::now(),
            detached: Vec::new(),
            update: Update::default(),
        };
        if state.settings_worker.is_some() {
            match deadpan_cli::backup_settings::default_path() {
                Ok(path) => state.queue_settings(SettingsJob::Load { ticket: 0, path }),
                Err(error) => {
                    state.settings.status = SettingsStatus::Failed(error.to_string());
                }
            }
        } else {
            state.settings.status = SettingsStatus::Failed(
                "The backup settings worker could not start; backups will keep every copy.".into(),
            );
        }
        state
    }
}

impl State {
    /// A backup is copying the database.
    pub(super) fn running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn update(&self) -> Update {
        let mut update = self.update.clone();
        update.settings = self.settings.clone();
        #[cfg(any(test, feature = "ui-harness"))]
        {
            update.owned_workers_active_for_check = self
                .running
                .as_ref()
                .is_some_and(|running| !running.handle.is_finished())
                || self.detached.iter().any(|handle| !handle.is_finished());
        }
        update
    }

    fn queue_settings(&mut self, job: SettingsJob) {
        let ticket = job.ticket();
        if self.settings_in_flight.is_some() {
            match &job {
                SettingsJob::Load { .. } => {
                    // Keep an explicit save ahead of the newest reload. A
                    // repeated panel-open request can replace an older load,
                    // but it must never silently discard a Save request.
                    self.pending_settings
                        .retain(|pending| matches!(pending, SettingsJob::Save { .. }));
                    self.pending_settings.push_back(job);
                }
                SettingsJob::Save { .. } => {
                    // The latest explicit settings value supersedes queued
                    // work; a reload before it would only observe the file
                    // before this save is published.
                    self.pending_settings.clear();
                    self.pending_settings.push_back(job);
                }
            }
            return;
        }
        let Some(worker) = &self.settings_worker else {
            self.settings.ticket = ticket;
            self.settings.trusted = false;
            self.settings.status = SettingsStatus::Failed(
                "The backup settings worker is unavailable; backups will keep every copy.".into(),
            );
            return;
        };
        match worker.submit(job) {
            Ok(()) => self.settings_in_flight = Some(ticket),
            Err(error) => {
                self.settings.ticket = ticket;
                self.settings.status = SettingsStatus::Failed(error);
            }
        }
    }

    fn request_settings_load(&mut self, ticket: u64, path: PathBuf) {
        self.settings.ticket = ticket;
        self.settings.trusted = false;
        self.settings.status = SettingsStatus::Loading;
        self.policy = self.settings.settings.policy_without_pruning();
        self.queue_settings(SettingsJob::Load { ticket, path });
    }

    fn request_settings_save(
        &mut self,
        ticket: u64,
        settings: deadpan_cli::backup_settings::Settings,
        path: PathBuf,
    ) {
        self.settings.ticket = ticket;
        self.settings.status = SettingsStatus::Saving;
        self.queue_settings(SettingsJob::Save {
            ticket,
            settings,
            path,
        });
    }

    fn pump_settings(&mut self) -> bool {
        let Some(worker) = &self.settings_worker else {
            return false;
        };
        let mut changed = false;
        match worker.try_recv() {
            Ok(result) => {
                let ticket = match &result {
                    SettingsResult::Loaded { ticket, .. }
                    | SettingsResult::Saved { ticket, .. } => *ticket,
                };
                self.settings_in_flight = None;
                if self.settings.ticket == ticket {
                    match result {
                        SettingsResult::Loaded { result, .. } => match result {
                            Ok(loaded) => {
                                let interval_changed = self.settings.settings.interval_minutes()
                                    != loaded.settings.interval_minutes();
                                self.settings.settings = loaded.settings;
                                self.settings.trusted = true;
                                self.settings.status = SettingsStatus::Ready {
                                    source: loaded.source,
                                    warning: None,
                                };
                                self.policy = self.settings.settings.policy();
                                if interval_changed {
                                    self.since = Instant::now();
                                    self.failures = 0;
                                }
                            }
                            Err(error) => {
                                let interval_changed =
                                    self.settings.settings.interval_minutes() != 15;
                                self.settings.settings =
                                    deadpan_cli::backup_settings::Settings::default();
                                self.settings.trusted = false;
                                self.settings.status = SettingsStatus::Failed(error);
                                self.policy = self.settings.settings.policy_without_pruning();
                                if interval_changed {
                                    self.since = Instant::now();
                                    self.failures = 0;
                                }
                            }
                        },
                        SettingsResult::Saved {
                            settings, result, ..
                        } => match result {
                            Ok(saved) => {
                                let interval_changed = self.settings.settings.interval_minutes()
                                    != settings.interval_minutes();
                                self.settings.settings = settings;
                                self.settings.trusted = true;
                                self.settings.status = SettingsStatus::Saved {
                                    warning: saved.warning,
                                };
                                self.policy = self.settings.settings.policy();
                                if interval_changed {
                                    self.since = Instant::now();
                                    self.failures = 0;
                                }
                            }
                            Err(error) => {
                                // The old file remains authoritative unless
                                // the save reports a completed replacement.
                                self.settings.status = SettingsStatus::Failed(error);
                            }
                        },
                    }
                    changed = true;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                if let Some(ticket) = self.settings_in_flight.take()
                    && self.settings.ticket == ticket
                {
                    self.settings.status = SettingsStatus::Failed(
                        "The backup settings worker stopped before replying.".into(),
                    );
                    changed = true;
                }
            }
        }
        if self.settings_in_flight.is_none()
            && let Some(job) = self.pending_settings.pop_front()
        {
            self.queue_settings(job);
            changed = true;
        }
        changed
    }

    /// Finish explicit settings writes, then wait briefly for closing backups.
    /// Settings contain at most 4 KiB and are replaced atomically. A Save
    /// already accepted by the service must finish before shutdown completes.
    /// A backup copy still
    /// running afterwards is abandoned with the process; its hidden staging
    /// file is removed by a later rotation and nothing is published.
    pub(super) fn finish_on_exit(&mut self) {
        while self.settings_in_flight.is_some() || !self.pending_settings.is_empty() {
            if !self.pump_settings() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if let Some(SettingsWorker {
            jobs,
            results,
            _handle,
        }) = self.settings_worker.take()
        {
            drop(jobs);
            drop(results);
            if _handle.join().is_err() {
                eprintln!("Backup settings worker failed during shutdown.");
            }
        }
        let deadline = Instant::now() + EXIT_WAIT;
        if let Some(running) = self.running.take() {
            running.cancel.store(true, Ordering::Release);
            self.detached.push(running.handle);
        }
        while Instant::now() < deadline && self.detached.iter().any(|handle| !handle.is_finished())
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        for handle in self.detached.drain(..) {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

fn spawn(
    package: PathBuf,
    reason: BackupReason,
    policy: BackupPolicy,
    cancel: Arc<AtomicBool>,
) -> std::io::Result<(Outcome, JoinHandle<()>)> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let handle = std::thread::Builder::new()
        .name("deadpan-backup".into())
        .spawn(move || {
            let result = create_backup(&package, reason, &policy, BackupLimits::default(), &cancel)
                .map_err(|error| describe(&error));
            let _ = sender.send(result);
        })?;
    Ok((receiver, handle))
}

fn describe(error: &deadpan_store::backups::BackupError) -> String {
    match error {
        deadpan_store::backups::BackupError::Store(store) => {
            crate::recovery::describe_store_error(store)
        }
        other => match other.code() {
            "DiskFull" => "The backup was not made: the disk is full. Your saved edits are intact; free space and it is tried again.".into(),
            _ => other.to_string(),
        },
    }
}

impl Service {
    fn backup_settings_path(&self) -> std::result::Result<PathBuf, String> {
        #[cfg(any(test, feature = "ui-harness"))]
        if let Some(path) = self
            .shared
            .backup_settings_path
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
        {
            return Ok(path);
        }
        deadpan_cli::backup_settings::default_path().map_err(|error| error.to_string())
    }

    /// The store revoked its writer ownership with the replaced state; the
    /// live command endpoint is bound again to the restored one.
    fn rebind_host_after_restore(&mut self) {
        self.retire_host();
        if let Some(store) = self.store.as_mut()
            && store.access_mode() == AccessMode::ReadWrite
        {
            match super::headless::Host::bind(store) {
                Ok(host) => self.host = Some(host),
                Err(error) => {
                    self.message = Some(format!(
                        "Headless commands cannot reach this project until it is reopened: {error}"
                    ));
                }
            }
        }
    }

    fn backup_interval(&self) -> Duration {
        #[cfg(any(test, feature = "ui-harness"))]
        {
            let override_ms = self.shared.backup_interval_ms.load(Ordering::Acquire);
            if override_ms > 0 {
                return Duration::from_millis(override_ms);
            }
        }
        self.backups.policy.interval
    }

    /// The interval, doubled for each consecutive automatic failure (at most
    /// sixteen times), so a full disk is not hammered.
    fn backup_wait(&self) -> Duration {
        self.backup_interval()
            .saturating_mul(1u32 << self.backups.failures.min(4))
    }

    /// A writable session's package and its current change version, if it
    /// can be backed up.
    fn backup_target(&self) -> Option<(u64, PathBuf, Option<i64>)> {
        let store = self.store.as_ref()?;
        if store.access_mode() != AccessMode::ReadWrite {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        let version = self
            .backups
            .monitor
            .as_ref()
            .filter(|(session, _)| *session == workspace.session)
            .and_then(|(_, monitor)| monitor.version().ok());
        Some((workspace.session, workspace.path.clone(), version))
    }

    /// Whether the latest backup holds the database as it is now. Unknown
    /// (no monitor) counts as not covered.
    fn backup_covered(&self, session: u64, version: Option<i64>) -> bool {
        version.is_some_and(|version| self.backups.covered == Some((session, version)))
    }

    /// A new session starts its own interval. If the package already has
    /// backups the head at open is treated as covered: it was saved by an
    /// earlier session, whose own backups bound what is unprotected.
    pub(super) fn answer_backup(&mut self, ticket: u64, result: Result<String>) {
        self.backups.update.reply = Some(Reply { ticket, result });
    }

    pub(super) fn begin_backup_session(&mut self) {
        // A copy of an earlier session finishes detached; it must not block
        // or answer for this one.
        if let Some(running) = self.backups.running.take() {
            if let Some(ticket) = running.ticket {
                self.backups.update.reply = Some(Reply {
                    ticket,
                    result: Err("The project changed while backing up; that backup continues for the previous project.".into()),
                });
            }
            self.backups.detached.push(running.handle);
        }
        self.backups.since = Instant::now();
        self.backups.failures = 0;
        self.backups.monitor = self
            .store
            .as_ref()
            .filter(|store| store.access_mode() == AccessMode::ReadWrite)
            .and_then(|store| store.change_monitor().ok())
            .map(|monitor| (self.session, monitor));
        self.backups.covered = self.backup_target().and_then(|(session, path, version)| {
            list_backups(&path)
                .ok()
                .filter(|backups| !backups.is_empty())
                .and(version)
                .map(|version| (session, version))
        });
        self.backups.update = Update {
            session: self.session,
            ..Update::default()
        };
    }

    fn start_backup(&mut self, reason: BackupReason, ticket: Option<u64>) -> Result<()> {
        if self.backups.running.is_some() {
            return Err("A backup is already being made.".into());
        }
        let (session, path, version) = self
            .backup_target()
            .ok_or("Open a project that can be saved to back it up.")?;
        let cancel = Arc::new(AtomicBool::new(false));
        let (receiver, handle) = spawn(path, reason, self.backups.policy, cancel.clone())
            .map_err(|error| format!("The backup could not start: {error}"))?;
        self.backups.running = Some(Running {
            session,
            version,
            reason,
            ticket,
            cancel,
            receiver,
            handle,
        });
        self.backups.update.running = Some(reason);
        Ok(())
    }

    /// Poll the running backup and start a periodic one when due. Returns
    /// whether published state changed.
    pub(super) fn pump_backups(&mut self) -> bool {
        let mut changed = self.backups.pump_settings();
        if let Some(running) = &self.backups.running {
            match running.receiver.try_recv() {
                Ok(result) => {
                    let running = self.backups.running.take().expect("checked");
                    let _ = running.handle.join();
                    self.backups.since = Instant::now();
                    self.backups.update.running = None;
                    let current = running.session == self.session;
                    match result {
                        Ok(outcome) => {
                            // Everything committed before the copy began is
                            // in it; later commits make the next one due.
                            if current && let Some(version) = running.version {
                                self.backups.covered = Some((running.session, version));
                            }
                            self.backups.failures = 0;
                            if current {
                                self.backups.update.failure = None;
                                self.backups.update.latest = Some((
                                    outcome.backup.clone(),
                                    outcome.revision_id.as_ref().map(|r| r.as_str().to_owned()),
                                ));
                            }
                            if let Some(ticket) = running.ticket {
                                self.backups.update.reply = Some(Reply {
                                    ticket,
                                    result: Ok("Backed up and verified.".into()),
                                });
                            }
                        }
                        Err(error) => {
                            if current && running.reason == BackupReason::Periodic {
                                self.backups.update.failure = Some(error.clone());
                                self.backups.failures = self.backups.failures.saturating_add(1);
                            }
                            if let Some(ticket) = running.ticket {
                                self.backups.update.reply = Some(Reply {
                                    ticket,
                                    result: Err(error),
                                });
                            }
                        }
                    }
                    changed = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    let running = self.backups.running.take().expect("checked");
                    let _ = running.handle.join();
                    self.backups.update.running = None;
                    changed = true;
                }
            }
        }
        #[cfg(any(test, feature = "ui-harness"))]
        let detached_before = self.backups.detached.len();
        self.backups.detached.retain(|handle| !handle.is_finished());
        #[cfg(any(test, feature = "ui-harness"))]
        {
            // Publish the completion observation even with no project open.
            changed |= self.backups.detached.len() != detached_before;
        }
        if self.backups.running.is_none()
            && let Some((session, _, version)) = self.backup_target()
            && !self.backup_covered(session, version)
            && self.backups.since.elapsed() >= self.backup_wait()
            && self.start_backup(BackupReason::Periodic, None).is_ok()
        {
            changed = true;
        }
        changed
    }

    /// The session is about to close or be replaced: back up saved edits the
    /// latest backup does not hold, on a detached thread.
    pub(super) fn backup_before_session_change(&mut self) {
        let target = self.backup_target();
        // Closing the writer must also release its idle read connection. A
        // retained monitor pins the old WAL after Close and can survive a
        // subsequent damaged-database replacement at the same path.
        self.backups.monitor = None;
        let Some((session, path, version)) = target else {
            return;
        };
        if self.backup_covered(session, version) {
            return;
        }
        if let Some(running) = self.backups.running.take() {
            // An older head's copy finishes on its own; answer its request.
            if let Some(ticket) = running.ticket {
                self.backups.update.reply = Some(Reply {
                    ticket,
                    result: Err("The project closed while backing up; the backup finishes in the background.".into()),
                });
            }
            self.backups.detached.push(running.handle);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok((_, handle)) = spawn(path, BackupReason::Close, self.backups.policy, cancel) {
            self.backups.detached.push(handle);
        }
        self.backups.covered = None;
    }

    pub(super) fn backup_command(&mut self, request: Request) {
        match request {
            Request::Now {
                ticket,
                expected_session,
            } => {
                let result = if expected_session != self.session {
                    Err("The project changed; back it up again.".to_owned())
                } else {
                    self.start_backup(BackupReason::Manual, Some(ticket))
                };
                if let Err(error) = result {
                    self.backups.update.reply = Some(Reply {
                        ticket,
                        result: Err(error),
                    });
                }
            }
            Request::Restore {
                ticket,
                expected_session,
                id,
            } => {
                let result = self.restore(expected_session, &id);
                self.backups.update.reply = Some(Reply { ticket, result });
            }
            Request::LoadSettings { ticket } => match self.backup_settings_path() {
                Ok(path) => self.backups.request_settings_load(ticket, path),
                Err(error) => {
                    self.backups.settings.ticket = ticket;
                    self.backups.settings.status = SettingsStatus::Failed(error);
                    self.backups.settings.trusted = false;
                    self.backups.policy = self.backups.settings.settings.policy_without_pruning();
                }
            },
            Request::SaveSettings { ticket, settings } => match self.backup_settings_path() {
                Ok(path) => self.backups.request_settings_save(ticket, settings, path),
                Err(error) => {
                    self.backups.settings.ticket = ticket;
                    self.backups.settings.status = SettingsStatus::Failed(error);
                }
            },
        }
    }

    fn restore(&mut self, expected_session: u64, id: &str) -> Result<String> {
        self.restore_outcome(expected_session, id)
            .map(|(message, _)| message)
            .map_err(|failure| failure.message)
    }

    /// A restore requested through the live endpoint: the same refusals,
    /// safety backup, verification and session replacement as Storage R.
    /// Its reply is written by the replaced owner's retired endpoint.
    pub(super) fn host_restore(
        &mut self,
        id: &str,
        expected: Option<&deadpan_core::RevisionId>,
    ) -> std::result::Result<serde_json::Value, deadpan_cli::live_project::LiveError> {
        use deadpan_cli::live_project::LiveError;
        // Like closing, a restore never discards work in progress: an open
        // draft or preview, or an edit receipt the app has not shown yet.
        // Pending key operators live in the UI and are cancelled by the
        // session change, as for a native restore.
        let unsaved = [
            (
                self.shared.preview_active.load(Ordering::Acquire),
                "a Camera, Gain or Room tone preview",
            ),
            (self.splice_draft.is_some(), "a Place slice proposal"),
            (self.slip_draft.is_some(), "a Slip proposal"),
            (self.trim_draft.is_some(), "a Trim draft"),
            (self.room_tone.is_some(), "a prepared room-tone range"),
            (self.gain.is_some(), "a Gain draft"),
            (
                self.committed.is_some(),
                "an edit the app has not shown yet",
            ),
        ];
        if let Some((_, what)) = unsaved.iter().find(|(open, _)| *open) {
            return Err(LiveError::new(
                "RestoreDraftOpen",
                format!(
                    "Deadpan has {what} open; apply or cancel it in the app, then restore again"
                ),
            ));
        }
        if let Some(expected) = expected {
            let current = self
                .store
                .as_ref()
                .ok_or_else(|| LiveError::new("HostOwnerChanged", "No project is open"))?
                .head_revision()
                .map_err(LiveError::store)?;
            if &current != expected {
                return Err(LiveError::store(
                    deadpan_store::StoreError::RevisionConflict {
                        expected: expected.as_str().into(),
                        current: current.as_str().into(),
                    },
                ));
            }
        }
        let session = self.session;
        let result = self.restore_outcome(session, id);
        let reply = match result {
            Ok((message, outcome)) => {
                self.message = Some(format!("{message} Requested from the command line."));
                Ok(serde_json::json!({ "protocol": 1, "restored": outcome }))
            }
            Err(failure) => {
                self.set_error(Some(failure.message.clone()));
                Err(LiveError {
                    committed_revision: failure.restored_revision,
                    ..LiveError::new(failure.code, failure.message)
                })
            }
        };
        self.publish();
        reply
    }

    fn restore_outcome(
        &mut self,
        expected_session: u64,
        id: &str,
    ) -> std::result::Result<(String, deadpan_store::backups::RestoreOutcome), RestoreFailure> {
        if expected_session != self.session || self.workspace.is_none() {
            return Err("The project changed; choose the backup again.".into());
        }
        if self.active.is_some()
            || self.relinking.is_some()
            || self.host_preparation_active()
            || self.render.is_some()
            || self.generation.active()
            || self.targets.active()
            || self.remote_storage_active()
        {
            return Err(
                "Wait for the current import, render, AI pause, tracking or command-line cleanup to finish, then restore."
                    .into(),
            );
        }
        if let Some(running) = self.backups.running.take() {
            running.cancel.store(true, Ordering::Release);
            let _ = running.handle.join();
            self.backups.update.running = None;
        }
        // A closing backup of an earlier head must not publish after the
        // restore and pose as its newest state.
        for handle in self.backups.detached.drain(..) {
            let _ = handle.join();
        }
        let path = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone())
            .ok_or("Open a project first")?;
        let store = self.store.as_mut().ok_or("Open a project first")?;
        let outcome = store.restore_backup(
            id,
            &self.backups.policy,
            BackupLimits::default(),
            &AtomicBool::new(false),
        );
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error @ deadpan_store::backups::BackupError::RestoredButUnverified { .. }) => {
                // The database already changed: never keep the old session
                // over it. Close and ask for a reopen.
                let deadpan_store::backups::BackupError::RestoredButUnverified { safety, .. } =
                    &error
                else {
                    unreachable!()
                };
                let failure = RestoreFailure {
                    code: "BackupRestoredUnverified",
                    message: format!(
                        "The project database was replaced, but the restored state could not be verified ({error}). The project was closed; reopen it. The state before restoring is in backup {safety}."
                    ),
                    restored_revision: None,
                };
                self.cancel();
                self.retire_host();
                self.store = None;
                self.workspace = None;
                self.cached = None;
                self.clear_copied_slice();
                self.clear_marks();
                return Err(failure);
            }
            Err(error) => {
                // Nothing changed. Handles may already be revoked; serve the
                // endpoint afresh.
                self.rebind_host_after_restore();
                return Err(describe(&error).into());
            }
        };
        self.rebind_host_after_restore();
        // A new session: drafts, selections, copies and caches of the
        // replaced revisions must not reach the restored project.
        let next = self
            .session
            .checked_add(1)
            .ok_or("Project session identities exhausted")?;
        self.cancel();
        let store = self.store.as_ref().ok_or("Open a project first")?;
        #[cfg(test)]
        let injected = self
            .shared
            .restore_show_failure
            .swap(false, Ordering::AcqRel);
        #[cfg(not(test))]
        let injected = false;
        let prepared = if injected {
            Err("Injected failure showing the restored project".to_owned())
        } else {
            snapshot(store, next, path, None)
        }
        .and_then(|workspace| {
            let registers = registers::restore(store, next)?;
            let report = Arc::new(recovery::open_report(store, &workspace));
            Ok((workspace, registers, report))
        });
        let (workspace, registers, report) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                // The database is already the backup's: never leave the old
                // session showing it. Close and ask for a reopen.
                self.retire_host();
                self.store = None;
                self.workspace = None;
                self.cached = None;
                self.clear_copied_slice();
                self.clear_marks();
                return Err(RestoreFailure {
                    code: "BackupRestoredNotShown",
                    message: format!(
                        "The backup was restored, but the project could not be shown ({error}). Reopen it; the state before restoring is in backup {}.",
                        outcome.safety.backup.id
                    ),
                    restored_revision: Some(outcome.restored.revision_id.clone()),
                });
            }
        };
        self.session = next;
        self.workspace = Some(Arc::new(workspace));
        self.registers = Some(registers);
        self.opened = Some(report);
        self.storage = None;
        self.storage_watch = Default::default();
        self.cached = None;
        self.clear_copied_slice();
        self.clear_marks();
        self.import = None;
        self.committed = None;
        self.begin_backup_session();
        self.auto_relinked.clear();
        self.relink_moved_originals();
        // The restored state is exactly the chosen backup's.
        self.backups.covered = self
            .backup_target()
            .and_then(|(session, _, version)| version.map(|version| (session, version)));
        self.backups.update.latest = Some((
            outcome.safety.backup.clone(),
            outcome
                .safety
                .revision_id
                .as_ref()
                .map(|revision| revision.as_str().to_owned()),
        ));
        let message = format!(
            "Restored the backup from {}. What you had before was backed up first; restore that backup to go back.",
            age(outcome.restored.info.created_unix_ms)
        );
        self.message = Some(message.clone());
        Ok((message, outcome))
    }
}

/// Why a restore did not leave the restored project showing. A refusal
/// changed nothing; the other codes mean the database already changed.
pub(super) struct RestoreFailure {
    code: &'static str,
    message: String,
    /// The head the database now holds, when known.
    restored_revision: Option<deadpan_core::RevisionId>,
}

impl From<String> for RestoreFailure {
    fn from(message: String) -> Self {
        Self {
            code: "BackupRestoreRefused",
            message,
            restored_revision: None,
        }
    }
}

impl From<&str> for RestoreFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn quitting_finishes_an_explicit_settings_save_before_releasing_the_worker() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("backups.json");
        let settings = deadpan_cli::backup_settings::Settings::new(25, 72, 3072).unwrap();
        let mut state = State::default();
        state.request_settings_save(1, settings.clone(), path.clone());
        state.finish_on_exit();
        assert_eq!(
            deadpan_cli::backup_settings::Settings::load_from(&path)
                .unwrap()
                .settings,
            settings,
        );
        assert!(matches!(
            state.settings.status,
            SettingsStatus::Saved { .. }
        ));
        assert!(state.settings_worker.is_none());
        assert!(state.settings_in_flight.is_none());
        assert!(state.pending_settings.is_empty());
    }

    #[test]
    fn a_failed_pre_replace_save_keeps_the_effective_policy() {
        let old = deadpan_cli::backup_settings::Settings::new(30, 96, 8192).unwrap();
        let attempted = deadpan_cli::backup_settings::Settings::new(5, 16, 512).unwrap();
        let mut state = State::default();
        let (jobs, _job_receiver) = mpsc::sync_channel(1);
        let (sender, results) = mpsc::sync_channel(1);
        sender
            .send(SettingsResult::Saved {
                ticket: 9,
                settings: attempted,
                result: Err("the temporary file could not be written".into()),
            })
            .unwrap();
        state.settings_worker = Some(SettingsWorker {
            jobs,
            results,
            _handle: std::thread::spawn(|| {}),
        });
        state.settings_in_flight = Some(9);
        state.pending_settings.clear();
        state.settings = SettingsUpdate {
            ticket: 9,
            settings: old.clone(),
            trusted: true,
            status: SettingsStatus::Saving,
        };
        state.policy = old.policy();

        assert!(state.pump_settings());
        assert_eq!(state.settings.settings, old);
        assert!(state.settings.trusted);
        assert_eq!(state.policy, old.policy());
        assert!(matches!(
            &state.settings.status,
            SettingsStatus::Failed(message) if message.contains("temporary file")
        ));
    }

    #[test]
    fn a_settings_reload_cannot_replace_a_queued_save() {
        let mut state = State {
            settings_in_flight: Some(1),
            ..State::default()
        };
        state.queue_settings(SettingsJob::Save {
            ticket: 2,
            settings: deadpan_cli::backup_settings::Settings::new(25, 72, 3072).unwrap(),
            path: PathBuf::from("settings.json"),
        });
        state.queue_settings(SettingsJob::Load {
            ticket: 3,
            path: PathBuf::from("settings.json"),
        });

        assert_eq!(state.pending_settings.len(), 2);
        assert!(matches!(
            state.pending_settings.front(),
            Some(SettingsJob::Save { ticket: 2, .. })
        ));
        assert!(matches!(
            state.pending_settings.back(),
            Some(SettingsJob::Load { ticket: 3, .. })
        ));
    }
}
