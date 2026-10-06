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

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use deadpan_store::AccessMode;
use deadpan_store::backups::{
    BackupLimits, BackupOutcome, BackupPolicy, BackupReason, create_backup, list_backups,
};

use super::super::backups::{Reply, Request, Update, age};
use super::{Result, Service, recovery, registers, snapshot};

/// How long quitting waits for a closing backup still copying.
const EXIT_WAIT: Duration = Duration::from_millis(1500);

type Outcome = mpsc::Receiver<std::result::Result<BackupOutcome, String>>;

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
        Self {
            policy: BackupPolicy::default(),
            running: None,
            covered: None,
            monitor: None,
            failures: 0,
            since: Instant::now(),
            detached: Vec::new(),
            update: Update::default(),
        }
    }
}

impl State {
    pub(super) fn update(&self) -> Update {
        self.update.clone()
    }

    /// Wait briefly for closing backups when the app quits. A copy still
    /// running afterwards is abandoned with the process; its hidden staging
    /// file is removed by a later rotation and nothing is published.
    pub(super) fn finish_on_exit(&mut self) {
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
    /// The store revoked its writer ownership with the replaced state; the
    /// live command endpoint is bound again to the restored one.
    fn rebind_host_after_restore(&mut self) {
        self.host = None;
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
        let mut changed = false;
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
        self.backups.detached.retain(|handle| !handle.is_finished());
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
        let Some((session, path, version)) = self.backup_target() else {
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
        }
    }

    fn restore(&mut self, expected_session: u64, id: &str) -> Result<String> {
        if expected_session != self.session || self.workspace.is_none() {
            return Err("The project changed; choose the backup again.".into());
        }
        if self.active.is_some()
            || self.relinking.is_some()
            || self.host_preparation_active()
            || self.render.is_some()
            || self.generation.active()
            || self.targets.active()
        {
            return Err(
                "Wait for the current import, render, AI pause or tracking to finish, then restore."
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
                let message = error.to_string();
                self.cancel();
                self.host = None;
                self.store = None;
                self.workspace = None;
                self.cached = None;
                self.clear_copied_slice();
                self.clear_marks();
                return Err(message);
            }
            Err(error) => {
                // Handles may already be revoked; serve the endpoint afresh.
                self.rebind_host_after_restore();
                return Err(describe(&error));
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
        let prepared = snapshot(store, next, path, None).and_then(|workspace| {
            let registers = registers::restore(store, next)?;
            let report = Arc::new(recovery::open_report(store, &workspace));
            Ok((workspace, registers, report))
        });
        let (workspace, registers, report) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                // The database is already the backup's: never leave the old
                // session showing it. Close and ask for a reopen.
                self.host = None;
                self.store = None;
                self.workspace = None;
                self.cached = None;
                self.clear_copied_slice();
                self.clear_marks();
                return Err(format!(
                    "The backup was restored, but the project could not be shown ({error}). Reopen it; the state before restoring is in backup {}.",
                    outcome.safety.backup.id
                ));
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
        Ok(message)
    }
}
