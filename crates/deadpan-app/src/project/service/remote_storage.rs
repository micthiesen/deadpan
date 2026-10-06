//! Command-line storage cleanup and clock confirmation on an open project.
//!
//! These can take minutes on a long history, so, like the automatic
//! retention pass, planning and reference scans run on read-only opens in
//! their own threads while this service keeps serving edits, and only the
//! short rechecked writes run on the writer: the planned expiry through
//! `apply_generation_expiry`, then exactly the scanned files through
//! `clean_previewed_storage`. Dry runs never touch the writer. The admitted
//! request is answered when the job finishes, on the endpoint that admitted
//! it. A file plan (`--plan`, Storage R) needs no scan and runs on the
//! writer at once, as the panel's R does.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use deadpan_cli::host::ConnectionTicket;
use deadpan_cli::live_project::{LiveError, Reply as HostReply, ShortOperation};
use deadpan_cli::storage::{self as cli_storage, ExpiryStatus, PlannedExpiry};
use deadpan_store::storage::{CleanupOutcome, DEFAULT_GRACE, RemovedEntry};
use serde_json::Value;

use super::Service;
use super::headless::owner_changed;

/// A writer step waits at most this long for running jobs to finish.
const MAX_WAIT: Duration = Duration::from_secs(240);

type Reply<T> = mpsc::Receiver<Result<T, LiveError>>;

/// Whether `command` runs as an off-thread job with a later reply.
pub(super) fn deferred(command: &ShortOperation) -> bool {
    matches!(
        command,
        ShortOperation::CleanStorage { plan: None, .. }
            | ShortOperation::ConfirmVariantClock { .. }
    )
}

#[derive(Clone, Copy)]
enum Kind {
    Clean { grace: Duration, expire: bool },
    Confirm,
}

enum Phase {
    /// A dry run, entirely on a read-only open.
    Previewing(Reply<Value>),
    Planning(Reply<PlannedExpiry>),
    /// A plan waiting for an idle writer.
    Expiring(PlannedExpiry),
    Scanning(Reply<CleanupOutcome>),
    /// Scanned files waiting for an idle writer.
    Removing(Vec<RemovedEntry>),
}

pub(super) struct Job {
    owner: uuid::Uuid,
    ticket: ConnectionTicket,
    session: u64,
    package: std::path::PathBuf,
    kind: Kind,
    phase: Phase,
    /// The applied expiry, once the writer applied it.
    status: Option<ExpiryStatus>,
    /// When the current writer step began waiting.
    waiting: Option<Instant>,
}

fn poll<T>(reply: &Reply<T>) -> Option<Result<T, LiveError>> {
    match reply.try_recv() {
        Ok(result) => Some(result),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => Some(Err(LiveError::new(
            "StorageFailed",
            "The storage scan stopped unexpectedly",
        ))),
    }
}

impl Service {
    fn spawn_storage<T: Send + 'static>(
        &self,
        package: std::path::PathBuf,
        work: impl FnOnce(deadpan_store::ProjectStore) -> Result<T, LiveError> + Send + 'static,
    ) -> Result<Reply<T>, LiveError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let wake = self.shared.wake.clone();
        #[cfg(test)]
        let shared = self.shared.clone();
        std::thread::Builder::new()
            .name("deadpan-remote-storage".into())
            .spawn(move || {
                #[cfg(test)]
                while shared
                    .remote_storage_paused
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    shared
                        .remote_storage_waiting
                        .store(true, std::sync::atomic::Ordering::Release);
                    std::thread::sleep(Duration::from_millis(1));
                }
                let result = deadpan_store::ProjectStore::open(
                    &package,
                    deadpan_store::AccessMode::ReadOnly,
                )
                .map_err(LiveError::store)
                .and_then(work);
                let _ = sender.send(result);
                wake();
            })
            .map_err(|error| LiveError::new("StorageFailed", error))?;
        Ok(receiver)
    }

    /// Admit one command-line cleanup or clock confirmation. Its reply is
    /// sent by [`Self::pump_remote_storage`] when it finishes.
    pub(super) fn start_remote_storage(
        &mut self,
        owner: uuid::Uuid,
        ticket: ConnectionTicket,
        project: &deadpan_core::ProjectId,
        command: &ShortOperation,
    ) -> Result<(), LiveError> {
        self.check_host_admission()?;
        self.check_host_project(project)?;
        let (kind, dry_run) = match command {
            ShortOperation::CleanStorage {
                grace_seconds,
                expire_variants,
                dry_run,
                plan: None,
            } => (
                Kind::Clean {
                    grace: Duration::from_secs(*grace_seconds),
                    expire: *expire_variants,
                },
                *dry_run,
            ),
            ShortOperation::ConfirmVariantClock { dry_run } => (Kind::Confirm, *dry_run),
            _ => unreachable!("only deferred storage commands"),
        };
        if self.remote_storage.is_some() {
            return Err(LiveError::new(
                "StorageBusy",
                "Another command-line storage operation is running on this project",
            ));
        }
        if !dry_run {
            if self.retention.running() {
                return Err(LiveError::new(
                    "StorageBusy",
                    "The automatic AI variant check is running; try again when it finishes",
                ));
            }
            if let Some(reason) = self.retention_jobs_running() {
                return Err(LiveError::new("StorageBusy", reason));
            }
            // Like the panel, an open project keeps at least the default
            // grace period; a shorter one needs the writer exclusively.
            if let Kind::Clean { grace, .. } = kind
                && grace < DEFAULT_GRACE
            {
                return Err(LiveError::new(
                    "StorageGraceRefused",
                    "Deadpan has this project open, so cleanup keeps at least the default 24-hour grace period. Close the project to use a shorter one.",
                ));
            }
        }
        let workspace = self.workspace.as_ref().ok_or_else(owner_changed)?;
        let (session, package) = (workspace.session, workspace.path.clone());
        let phase = if dry_run {
            Phase::Previewing(
                self.spawn_storage(package.clone(), move |mut store| match kind {
                    Kind::Clean { grace, expire } => {
                        cli_storage::clean_project(&mut store, grace, expire, true, None)
                    }
                    Kind::Confirm => cli_storage::confirm_clock(&mut store, true),
                })?,
            )
        } else {
            Phase::Planning(self.spawn_storage(package.clone(), move |store| {
                let planned = match kind {
                    Kind::Clean { expire: false, .. } => PlannedExpiry::NotRequested,
                    _ => cli_storage::plan_expiry(&store)?,
                };
                if matches!(kind, Kind::Confirm) {
                    cli_storage::require_planned(&planned)?;
                }
                Ok(planned)
            })?)
        };
        self.remote_storage = Some(Job {
            owner,
            ticket,
            session,
            package,
            kind,
            phase,
            status: None,
            waiting: None,
        });
        Ok(())
    }

    /// Advance the command-line storage job. True when native state changed.
    pub(super) fn pump_remote_storage(&mut self) -> bool {
        let Some(mut job) = self.remote_storage.take() else {
            return false;
        };
        let reply = match self.advance_remote_storage(&mut job) {
            Ok(None) => {
                self.remote_storage = Some(job);
                return false;
            }
            Ok(Some(output)) => HostReply::Completed {
                output,
                committed_revision: None,
                committed_registers: None,
                refresh_error: None,
            },
            Err(error) => HostReply::Failed {
                error: partial(error, job.status.as_ref()),
            },
        };
        let changed = job.status.is_some();
        if changed {
            if matches!(&job.status, Some(ExpiryStatus::Applied { expiry }) if !expiry.expired.is_empty())
            {
                self.generation.variants_changed();
            }
            self.retention.recheck_soon();
            if matches!(reply, HostReply::Completed { .. }) {
                self.message = Some(
                    match job.kind {
                        Kind::Clean { .. } => "Cleaned up project storage from the command line.",
                        Kind::Confirm => {
                            "Confirmed the clock for AI variant retention from the command line."
                        }
                    }
                    .into(),
                );
            }
        }
        self.respond_on(job.owner, job.ticket, reply);
        changed
    }

    /// One step. `Ok(Some)` finishes with that output.
    fn advance_remote_storage(&mut self, job: &mut Job) -> Result<Option<Value>, LiveError> {
        loop {
            match &job.phase {
                Phase::Previewing(reply) => return poll(reply).transpose(),
                Phase::Planning(reply) => match poll(reply) {
                    None => return Ok(None),
                    Some(planned) => job.phase = Phase::Expiring(planned?),
                },
                Phase::Expiring(planned) => {
                    if !self.remote_writer_ready(job.session, &mut job.waiting)? {
                        return Ok(None);
                    }
                    let store = self.store.as_mut().ok_or_else(owner_changed)?;
                    let status = cli_storage::settle_expiry(store, planned, false)?;
                    job.status = Some(status.clone());
                    match job.kind {
                        Kind::Confirm => return Ok(Some(cli_storage::clock_output(&status))),
                        Kind::Clean { grace, .. } => {
                            job.phase = Phase::Scanning(self.spawn_storage(
                                job.package.clone(),
                                move |store| {
                                    store
                                        .preview_storage_cleanup(grace)
                                        .map_err(LiveError::store)
                                },
                            )?);
                        }
                    }
                }
                Phase::Scanning(reply) => match poll(reply) {
                    None => return Ok(None),
                    Some(scanned) => job.phase = Phase::Removing(scanned?.removed),
                },
                Phase::Removing(previewed) => {
                    if !self.remote_writer_ready(job.session, &mut job.waiting)? {
                        return Ok(None);
                    }
                    let Kind::Clean { grace, .. } = job.kind else {
                        unreachable!("only cleanup removes files")
                    };
                    let store = self.store.as_mut().ok_or_else(owner_changed)?;
                    // Rechecks references and each file's identity, as the
                    // Storage panel's R does.
                    let cleanup = store
                        .clean_previewed_storage(grace, previewed)
                        .map_err(LiveError::store)?;
                    let status = job.status.clone().unwrap_or(ExpiryStatus::NotRequested);
                    return Ok(Some(cli_storage::clean_output(&status, &cleanup, None)));
                }
            }
        }
    }

    /// The same session still owns the writer and no job could be publishing
    /// media; waiting longer than [`MAX_WAIT`] fails.
    fn remote_writer_ready(
        &self,
        session: u64,
        waiting: &mut Option<Instant>,
    ) -> Result<bool, LiveError> {
        if self.check_host_admission().is_err()
            || self
                .workspace
                .as_ref()
                .is_none_or(|workspace| workspace.session != session)
        {
            return Err(owner_changed());
        }
        match self.retention_blocked() {
            None => Ok(true),
            Some(reason) => {
                let since = *waiting.get_or_insert_with(Instant::now);
                if since.elapsed() >= MAX_WAIT {
                    return Err(LiveError::new("StorageBusy", reason));
                }
                Ok(false)
            }
        }
    }

    /// The project is closing: answer an unfinished job truthfully.
    pub(super) fn abandon_remote_storage(&mut self) {
        if let Some(job) = self.remote_storage.take() {
            let error = partial(
                LiveError::new(
                    "HostOwnerChanged",
                    "The project closed before the storage operation finished",
                ),
                job.status.as_ref(),
            );
            self.respond_on(job.owner, job.ticket, HostReply::Failed { error });
        }
    }

    /// A command-line storage job is running; native retention and restores wait.
    pub(super) fn remote_storage_active(&self) -> bool {
        self.remote_storage.is_some()
    }
}

/// Name an expiry that was already applied when a later step failed.
fn partial(mut error: LiveError, status: Option<&ExpiryStatus>) -> LiveError {
    if let Some(ExpiryStatus::Applied { expiry }) = status {
        error.message = format!(
            "{} (AI variant expiry was already applied: {} expired)",
            error.message,
            expiry.expired.len()
        );
    }
    error
}
