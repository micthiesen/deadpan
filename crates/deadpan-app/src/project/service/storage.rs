//! Storage cleanup on the project writer: the Storage panel's explicit
//! removal and the automatic AI variant retention pass.
//!
//! The Storage panel previews cleanup on a read-only open in its own
//! thread. Only the confirmed removal runs here, because it needs the
//! writable store this service owns: it rescans references (the safety
//! check, which blocks other project commands for its duration, typically
//! well under a second and longer for very long histories), waits at most
//! two seconds for the render namespace lock, and removes only previewed
//! entries that are still unreferenced. It is refused while any job of this
//! session could be publishing media whose row is not yet committed; the
//! default grace period additionally keeps every object changed in the last
//! day.

use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use deadpan_store::generation_retention::{
    ClockAnomaly, DEFAULT_VARIANT_RETENTION, ExpiryMode, ExpiryPlan,
};
use deadpan_store::storage::{CleanupOutcome, CleanupPolicy, DEFAULT_GRACE, RemovedEntry};

use super::Service;
use crate::project::{ClockConfirmation, RetentionPassState, RetentionPassStatus};

/// How often a long session repeats the automatic pass, once idle.
pub(super) const RETENTION_PASS_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

type Reply<T> = mpsc::Receiver<Result<T, String>>;

/// Where this session's current pass is.
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    /// Planning expiry on a read-only open, off the writer.
    Planning(Reply<ExpiryPlan>),
    /// A trusted plan waiting for an idle writer.
    Applying(ExpiryPlan),
    /// Finding removable `Media/Generated` objects on a read-only open.
    Scanning {
        expired: u64,
        reply: Reply<CleanupOutcome>,
    },
    /// Previewed removable objects waiting for an idle writer.
    Removing {
        expired: u64,
        previewed: Vec<RemovedEntry>,
    },
}

/// The automatic retention pass: when the service is first idle after a
/// writable session opens, then every [`RETENTION_PASS_INTERVAL`]. Planning
/// (including any reference scan) and finding removable files run on
/// read-only opens in their own threads; the writer only applies the
/// rechecked expiry in one short transaction and, when files were found,
/// removes exactly those through the previewed-cleanup path with every
/// safety check of explicit cleanup. A clock anomaly expires nothing and
/// removes nothing.
#[derive(Default)]
pub(super) struct Retention {
    session: Option<u64>,
    due: Option<Instant>,
    phase: Phase,
    status: Option<RetentionPassStatus>,
    confirmation: Option<ClockConfirmation>,
}

impl Retention {
    pub(super) fn begin_session(&mut self, session: u64) {
        *self = Self {
            session: Some(session),
            due: Some(Instant::now()),
            ..Self::default()
        };
    }

    /// The automatic check is planning, applying, scanning or removing.
    pub(super) fn running(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Run the automatic check again as soon as the writer is idle, after a
    /// remote cleanup or clock confirmation changed what it would find.
    pub(super) fn recheck_soon(&mut self) {
        if matches!(self.phase, Phase::Idle) && self.session.is_some() {
            self.due = Some(Instant::now());
        }
    }

    pub(super) fn status(&self, session: u64) -> Option<RetentionPassStatus> {
        if self.session != Some(session) {
            return None;
        }
        let state = self
            .status
            .as_ref()
            .filter(|status| status.session == session)
            .map(|status| status.state.clone());
        if state.is_none() && self.confirmation.is_none() {
            return None;
        }
        Some(RetentionPassStatus {
            session,
            state: state.unwrap_or_else(|| {
                RetentionPassState::Deferred("The automatic check has not run yet.".into())
            }),
            confirmation: self.confirmation.clone(),
        })
    }

    /// Record the state; true only when it is worth publishing an update.
    /// Housekeeping that does nothing (deferred, running, a pass that
    /// expired and removed nothing) changes the status quietly, so it never
    /// re-delivers feedback or makes a remote command wait for the UI; the
    /// next update carries it. Expiry, removal, a clock anomaly, a failure,
    /// or the end of an anomaly or failure are published.
    fn set(&mut self, session: u64, state: RetentionPassState) -> bool {
        let previous = self
            .status
            .as_ref()
            .filter(|status| status.session == session)
            .map(|status| status.state.clone());
        if previous.as_ref() == Some(&state) {
            return false;
        }
        let notable = |state: &RetentionPassState| match state {
            RetentionPassState::Done {
                expired,
                removed_files,
                ..
            } => *expired > 0 || *removed_files > 0,
            RetentionPassState::ClockAnomaly(_) | RetentionPassState::Failed(_) => true,
            RetentionPassState::Deferred(_) | RetentionPassState::Running => false,
        };
        let publish = notable(&state)
            || previous.as_ref().is_some_and(|previous| {
                matches!(
                    previous,
                    RetentionPassState::ClockAnomaly(_) | RetentionPassState::Failed(_)
                )
            });
        self.status = Some(RetentionPassStatus {
            session,
            state,
            confirmation: None,
        });
        publish
    }

    /// End the pass: the next one is due after the interval.
    fn finish(&mut self, session: u64, state: RetentionPassState) -> bool {
        self.phase = Phase::Idle;
        self.due = Some(Instant::now() + RETENTION_PASS_INTERVAL);
        self.set(session, state)
    }
}

fn clock_text(anomaly: ClockAnomaly) -> String {
    match anomaly {
        ClockAnomaly::Behind { .. } => "This Mac's clock is earlier than times this project already recorded, so no AI variant was expired and no file removed. Check the date and time; expiry resumes once the clock is right.".into(),
        ClockAnomaly::Ahead { .. } => format!(
            "More than {} days passed since this project's last retention check, or the clock jumped ahead, so no AI variant was expired and no file removed. Check the date and time, then close the project and run `project storage --clean` to confirm it.",
            DEFAULT_VARIANT_RETENTION.as_secs() / (24 * 60 * 60)
        ),
    }
}

fn spawn<T: Send + 'static>(
    name: &str,
    wake: std::sync::Arc<dyn Fn() + Send + Sync>,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<Reply<T>, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let _ = sender.send(work());
            wake();
        })
        .map_err(|error| error.to_string())?;
    Ok(receiver)
}

fn poll<T>(reply: &Reply<T>) -> Option<Result<T, String>> {
    match reply.try_recv() {
        Ok(result) => Some(result),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => {
            Some(Err("The storage scan stopped unexpectedly.".into()))
        }
    }
}

impl Service {
    pub(super) fn clean_storage(
        &mut self,
        ticket: u64,
        expected_session: u64,
        previewed: &[RemovedEntry],
    ) {
        let session = self.session;
        let result = if self
            .workspace
            .as_ref()
            .is_none_or(|workspace| workspace.session != expected_session)
        {
            Err("The project changed; open Storage again.".to_owned())
        } else if self.active.is_some()
            || self.relinking.is_some()
            || self.host_preparation_active()
            || self.render.is_some()
            || self.generation.active()
            || self.targets.active()
            || self.remote_storage_active()
        {
            Err("Wait for the current import, render, AI pause, tracking or command-line cleanup to finish, then clean up again.".to_owned())
        } else {
            match self.store.as_mut() {
                None => Err("Open a project first.".to_owned()),
                Some(store) => store
                    .clean_previewed_storage(DEFAULT_GRACE, previewed)
                    .map_err(|error| error.to_string()),
            }
        };
        self.storage_cleanup = Some(super::super::StorageCleanupStatus {
            ticket,
            session,
            result,
        });
    }
}

impl Service {
    /// Why the writer must not run the retention pass now, if it must not.
    pub(super) fn retention_blocked(&self) -> Option<&'static str> {
        // A command the UI has submitted, or one being received, goes first.
        if self.shared.busy.load(std::sync::atomic::Ordering::Acquire) {
            Some("Waiting for the current command to finish.")
        } else {
            self.retention_jobs_running()
        }
    }

    /// A job that could be writing media or the database is running.
    pub(super) fn retention_jobs_running(&self) -> Option<&'static str> {
        if self.active.is_some() || self.relinking.is_some() || self.host_preparation_active() {
            Some("Waiting for the import or relink to finish.")
        } else if self.render.is_some() || self.pending_session_change.is_some() {
            Some("Waiting for the render to finish.")
        } else if self.generation.active() {
            Some("Waiting for the AI pause to finish.")
        } else if self.targets.active() {
            Some("Waiting for tracking to finish.")
        } else if self.backups.running() {
            Some("Waiting for the backup to finish.")
        } else if self.remote_storage_active() {
            Some("Waiting for the command-line storage operation to finish.")
        } else {
            None
        }
    }

    /// Advance this session's automatic retention pass. True when the
    /// change is worth publishing (see `Retention::set`).
    pub(super) fn pump_retention(&mut self) -> bool {
        let publish = self.advance_retention();
        #[cfg(test)]
        if let Some(session) = self.retention.session {
            *self
                .shared
                .retention_status
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = self.retention.status(session);
        }
        publish
    }

    fn advance_retention(&mut self) -> bool {
        let Some((session, package)) = self
            .workspace
            .as_ref()
            .map(|workspace| (workspace.session, workspace.path.clone()))
        else {
            return false;
        };
        if self.retention.session != Some(session)
            || self
                .store
                .as_ref()
                .is_none_or(|store| store.access_mode() != deadpan_store::AccessMode::ReadWrite)
        {
            return false;
        }
        let wake = self.shared.wake.clone();
        match std::mem::take(&mut self.retention.phase) {
            Phase::Idle => {
                if self.retention.due.is_none_or(|due| Instant::now() < due)
                    || !self
                        .shared
                        .automatic_retention
                        .load(std::sync::atomic::Ordering::Acquire)
                {
                    return false;
                }
                if let Some(reason) = self.retention_blocked() {
                    return self
                        .retention
                        .set(session, RetentionPassState::Deferred(reason.into()));
                }
                self.retention.due = None;
                #[cfg(test)]
                let shared = self.shared.clone();
                match spawn("deadpan-retention-plan", wake, move || {
                    #[cfg(test)]
                    while shared
                        .retention_paused
                        .load(std::sync::atomic::Ordering::Acquire)
                    {
                        shared
                            .retention_waiting
                            .store(true, std::sync::atomic::Ordering::Release);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    deadpan_store::ProjectStore::open(&package, deadpan_store::AccessMode::ReadOnly)
                        .and_then(|store| {
                            store.plan_generation_expiry(
                                SystemTime::now(),
                                DEFAULT_VARIANT_RETENTION,
                                ExpiryMode::Automatic,
                            )
                        })
                        .map_err(|error| error.to_string())
                }) {
                    Ok(reply) => {
                        self.retention.phase = Phase::Planning(reply);
                        self.retention.set(session, RetentionPassState::Running)
                    }
                    Err(error) => self
                        .retention
                        .finish(session, RetentionPassState::Failed(error)),
                }
            }
            Phase::Planning(reply) => match poll(&reply) {
                None => {
                    self.retention.phase = Phase::Planning(reply);
                    false
                }
                Some(Err(error)) => self
                    .retention
                    .finish(session, RetentionPassState::Failed(error)),
                // Never expire or remove anything on a clock it distrusts.
                Some(Ok(plan)) => match plan.clock_anomaly() {
                    Some(anomaly) => self.retention.finish(
                        session,
                        RetentionPassState::ClockAnomaly(clock_text(anomaly)),
                    ),
                    None => {
                        self.retention.phase = Phase::Applying(plan);
                        self.advance_retention()
                    }
                },
            },
            Phase::Applying(plan) => {
                if let Some(reason) = self.retention_blocked() {
                    self.retention.phase = Phase::Applying(plan);
                    return self
                        .retention
                        .set(session, RetentionPassState::Deferred(reason.into()));
                }
                let applied = match self.store.as_mut() {
                    None => Err("The project closed.".to_owned()),
                    Some(store) => store
                        .apply_generation_expiry(&plan, false)
                        .map_err(|error| error.to_string()),
                };
                let expired = match applied {
                    Ok(expiry) => expiry.expired.len() as u64,
                    Err(error) => {
                        return self
                            .retention
                            .finish(session, RetentionPassState::Failed(error));
                    }
                };
                if expired > 0 {
                    // Expired variants are no longer offered.
                    self.generation.variants_changed();
                }
                match spawn("deadpan-retention-scan", wake, move || {
                    deadpan_store::ProjectStore::open(&package, deadpan_store::AccessMode::ReadOnly)
                        .and_then(|store| {
                            store.preview_storage_cleanup_with(CleanupPolicy::generated_only(
                                DEFAULT_GRACE,
                                true,
                            ))
                        })
                        .map_err(|error| error.to_string())
                }) {
                    Ok(reply) => {
                        self.retention.phase = Phase::Scanning { expired, reply };
                        self.retention.set(session, RetentionPassState::Running)
                    }
                    Err(error) => self
                        .retention
                        .finish(session, RetentionPassState::Failed(error)),
                }
            }
            Phase::Scanning { expired, reply } => match poll(&reply) {
                None => {
                    self.retention.phase = Phase::Scanning { expired, reply };
                    false
                }
                Some(Err(error)) => self
                    .retention
                    .finish(session, RetentionPassState::Failed(error)),
                Some(Ok(outcome)) if outcome.removed.is_empty() => self.retention.finish(
                    session,
                    RetentionPassState::Done {
                        expired,
                        removed_files: 0,
                        removed_bytes: 0,
                        kept_files: 0,
                        finished: SystemTime::now(),
                    },
                ),
                Some(Ok(outcome)) => {
                    self.retention.phase = Phase::Removing {
                        expired,
                        previewed: outcome.removed,
                    };
                    self.advance_retention()
                }
            },
            Phase::Removing { expired, previewed } => {
                if let Some(reason) = self.retention_blocked() {
                    self.retention.phase = Phase::Removing { expired, previewed };
                    return self
                        .retention
                        .set(session, RetentionPassState::Deferred(reason.into()));
                }
                let result = match self.store.as_mut() {
                    None => Err("The project closed.".to_owned()),
                    Some(store) => store
                        .clean_previewed_storage_with(
                            CleanupPolicy::generated_only(DEFAULT_GRACE, false),
                            &previewed,
                        )
                        .map_err(|error| error.to_string()),
                };
                self.retention.finish(
                    session,
                    match result {
                        Ok(outcome) => RetentionPassState::Done {
                            expired,
                            removed_files: outcome.removed.len() as u64,
                            removed_bytes: outcome.removed_bytes,
                            kept_files: (outcome.in_use.len() + outcome.changed.len()) as u64,
                            finished: SystemTime::now(),
                        },
                        Err(error) => RetentionPassState::Failed(error),
                    },
                )
            }
        }
    }
}

impl Service {
    /// The Storage panel's clock confirmation; see
    /// [`crate::project::ProjectRequest::ConfirmVariantClock`].
    pub(super) fn confirm_variant_clock(
        &mut self,
        ticket: u64,
        expected_session: u64,
        expected_revision: &deadpan_core::RevisionId,
        plan: &ExpiryPlan,
    ) {
        let result = self.confirmed_clock(expected_session, expected_revision, plan);
        if result
            .as_ref()
            .is_ok_and(|expiry| !expiry.expired.is_empty())
        {
            // Expired variants are no longer offered.
            self.generation.variants_changed();
        }
        self.retention.confirmation = Some(ClockConfirmation { ticket, result });
    }

    fn confirmed_clock(
        &mut self,
        expected_session: u64,
        expected_revision: &deadpan_core::RevisionId,
        plan: &ExpiryPlan,
    ) -> Result<deadpan_store::generation_retention::VariantExpiry, String> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
        if workspace.session != expected_session
            || workspace.document.revision_id() != expected_revision
        {
            return Err("The project changed; press E again to review.".into());
        }
        if plan.mode() != ExpiryMode::Explicit {
            return Err("Only a reviewed explicit plan confirms the clock.".into());
        }
        // This request holds the command admission itself, so only jobs
        // refuse it.
        if let Some(reason) = self.retention_jobs_running() {
            return Err(format!("Not confirmed. {reason}"));
        }
        let store = self.store.as_mut().ok_or("Open a project first.")?;
        if store.access_mode() != deadpan_store::AccessMode::ReadWrite {
            return Err("This project is open read-only.".into());
        }
        // Never confirm a clock behind the project's own records.
        if let Some(ClockAnomaly::Behind { .. }) = store
            .generation_retention_clock(SystemTime::now(), DEFAULT_VARIANT_RETENTION)
            .map_err(|error| error.to_string())?
        {
            return Err(
                "This Mac's clock is earlier than times this project recorded; correct the date and time first.".into(),
            );
        }
        store
            .apply_generation_expiry(plan, false)
            .map_err(|error| error.to_string())
    }
}
