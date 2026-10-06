//! One bounded preparation on the existing import worker. Terminal observations
//! belong to the authenticated owner, independently of the native UI mailbox.

use deadpan_cli::live_project::preparation::{
    self, PreparationCommand, PreparationState, PreparationStatus, PreparationTarget,
    PreparedOperation,
};

use super::*;

const PREPARATION_DEADLINE: Duration = Duration::from_secs(15 * 60);

pub(super) struct Observation {
    status: PreparationStatus,
    command: PreparationCommand,
    worker_id: u64,
    session: u64,
    pub(super) cancelled: Arc<AtomicBool>,
    prepared: Option<PreparedOperation>,
    deadline: Instant,
    timed_out: bool,
    pub(super) expires: Option<Instant>,
}

impl Observation {
    fn finish(&mut self, state: PreparationState) {
        debug_assert!(state.is_terminal());
        self.prepared = None;
        self.status.state = state;
        self.expires = Some(Instant::now() + TERMINAL_RETENTION);
    }

    fn finish_cancelled(&mut self) {
        self.finish(if self.timed_out {
            PreparationState::Failed {
                error: LiveError::new(
                    "HostPreparationTimeout",
                    "Preparation exceeded its deadline and has stopped",
                ),
            }
        } else {
            PreparationState::Cancelled {}
        });
    }

    fn request_cancel(&mut self) {
        if !self.status.is_terminal() {
            self.cancelled.store(true, Ordering::Release);
            self.status.state = PreparationState::Cancelling {};
        }
    }
}

impl Service {
    pub(in crate::project::service) fn host_preparation_active(&self) -> bool {
        self.host_preparing.is_some()
            || self.host.as_ref().is_some_and(|host| {
                host.preparations
                    .iter()
                    .any(|entry| !entry.status.is_terminal())
            })
    }

    pub(super) fn host_prepare(
        &mut self,
        project: &ProjectId,
        target: PreparationTarget,
        command: PreparationCommand,
    ) -> std::result::Result<HostReply, LiveError> {
        self.check_host_project(project)?;
        target.validate()?;
        command.validate()?;
        let host = self.host.as_ref().ok_or_else(owner_changed)?;
        if host.preparations.iter().any(|entry| {
            entry.status.target.operation_id == target.operation_id
                || entry.status.target.cancellation_token == target.cancellation_token
        }) {
            return Err(LiveError::new(
                "HostPreparationIdentityUsed",
                "This operation identity was already admitted; inspect its status",
            ));
        }
        if host.preparations.len() >= MAX_OBSERVERS {
            return Err(LiveError::new(
                "HostPreparationLimit",
                "Release a completed preparation status or wait for its expiry",
            ));
        }
        if self.active.is_some() || self.host_preparation_active() {
            return Err(LiveError::new(
                "HostPreparationBusy",
                "The import worker is still preparing or draining another operation",
            ));
        }
        let workspace = self.workspace.as_ref().ok_or_else(owner_changed)?;
        let session = workspace.session;
        let handle = workspace.originals.clone();
        let id = self.serial.checked_add(1).ok_or_else(|| {
            LiveError::new("HostPreparationLimit", "Preparation identities exhausted")
        })?;
        let work = preparation::admit(self.store.as_mut().ok_or_else(owner_changed)?, &command)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.jobs
            .try_send(Job {
                id,
                handle,
                cancelled: cancelled.clone(),
                work: Work::Host(Box::new(work)),
            })
            .map_err(|_| {
                LiveError::new("HostPreparationBusy", "The import worker is unavailable")
            })?;
        let status = PreparationStatus {
            target,
            state: PreparationState::Preparing {},
        };
        self.serial = id;
        self.host_preparing = Some((id, cancelled.clone()));
        self.host
            .as_mut()
            .ok_or_else(owner_changed)?
            .preparations
            .push(Observation {
                status: status.clone(),
                command,
                worker_id: id,
                session,
                cancelled,
                prepared: None,
                deadline: Instant::now() + PREPARATION_DEADLINE,
                timed_out: false,
                expires: None,
            });
        Ok(HostReply::Preparation {
            status: Box::new(status),
        })
    }

    pub(super) fn host_preparation_status(
        &self,
        target: &PreparationTarget,
    ) -> std::result::Result<HostReply, LiveError> {
        target.validate()?;
        let entry = self
            .host
            .as_ref()
            .ok_or_else(owner_changed)?
            .preparations
            .iter()
            .find(|entry| &entry.status.target == target)
            .ok_or_else(missing)?;
        Ok(HostReply::Preparation {
            status: Box::new(entry.status.clone()),
        })
    }

    pub(super) fn host_cancel_preparation(
        &mut self,
        target: &PreparationTarget,
    ) -> std::result::Result<HostReply, LiveError> {
        target.validate()?;
        self.host
            .as_mut()
            .ok_or_else(owner_changed)?
            .preparations
            .iter_mut()
            .find(|entry| &entry.status.target == target)
            .ok_or_else(missing)?
            .request_cancel();
        self.host_preparation_status(target)
    }

    pub(super) fn host_release_preparation(
        &mut self,
        target: &PreparationTarget,
    ) -> std::result::Result<HostReply, LiveError> {
        let HostReply::Preparation { status } = self.host_preparation_status(target)? else {
            unreachable!()
        };
        if !status.is_terminal() {
            return Err(LiveError::new(
                "HostPreparationBusy",
                "Preparation has not stopped or committed yet",
            ));
        }
        self.host
            .as_mut()
            .ok_or_else(owner_changed)?
            .preparations
            .retain(|entry| &entry.status.target != target);
        Ok(HostReply::Released)
    }

    pub(in crate::project::service) fn cancel_host_preparation(&mut self) {
        if let Some((_, cancelled)) = &self.host_preparing {
            cancelled.store(true, Ordering::Release);
        }
        if let Some(host) = &mut self.host {
            for entry in &mut host.preparations {
                entry.request_cancel();
            }
        }
    }

    /// A result from the old session still drains the one worker slot, but can
    /// never supply a result to its replacement owner.
    pub(in crate::project::service) fn host_preparation_result(
        &mut self,
        reply: super::super::Reply,
    ) -> Option<super::super::Reply> {
        if self
            .host_preparing
            .as_ref()
            .is_none_or(|(id, _)| *id != reply.id)
        {
            return Some(reply);
        }
        self.host_preparing = None;
        let entry = self.host.as_mut().and_then(|host| {
            host.preparations
                .iter_mut()
                .find(|entry| entry.worker_id == reply.id)
        })?;
        if entry.cancelled.load(Ordering::Acquire) || entry.session != self.session {
            entry.finish_cancelled();
            return None;
        }
        match reply.result {
            Ok(Prepared::Host(Ok(prepared))) => {
                entry.prepared = Some(*prepared);
                entry.status.state = PreparationState::AwaitingCommit {};
            }
            Ok(Prepared::Host(Err(error))) => entry.finish(PreparationState::Failed { error }),
            Err(error) => entry.finish(PreparationState::Failed {
                error: LiveError::new("HostPreparationFailed", error),
            }),
            Ok(_) => entry.finish(PreparationState::Failed {
                error: LiveError::new(
                    "HostPreparationFailed",
                    "Worker returned an unrelated result",
                ),
            }),
        }
        None
    }

    pub(in crate::project::service) fn host_preparation_disconnected(&mut self) {
        let Some((id, cancelled)) = self.host_preparing.take() else {
            return;
        };
        cancelled.store(true, Ordering::Release);
        if let Some(entry) = self.host.as_mut().and_then(|host| {
            host.preparations
                .iter_mut()
                .find(|entry| entry.worker_id == id)
        }) {
            entry.finish(PreparationState::Failed {
                error: LiveError::new(
                    "HostPreparationFailed",
                    "Import worker stopped before replying",
                ),
            });
        }
    }

    pub(in crate::project::service) fn pump_host_preparation(&mut self) {
        let Some(host) = &mut self.host else { return };
        for entry in &mut host.preparations {
            if entry.status.is_terminal() {
                continue;
            }
            if Instant::now() >= entry.deadline && !entry.cancelled.load(Ordering::Acquire) {
                entry.timed_out = true;
                entry.request_cancel();
            }
            if self.shared.stopping.load(Ordering::Acquire) || self.pending_session_change.is_some()
            {
                entry.request_cancel();
            }
            if entry.cancelled.load(Ordering::Acquire)
                && self
                    .host_preparing
                    .as_ref()
                    .is_none_or(|(id, _)| *id != entry.worker_id)
            {
                entry.finish_cancelled();
            }
        }
        let Some(index) = host
            .preparations
            .iter()
            .position(|entry| entry.prepared.is_some())
        else {
            return;
        };
        if self
            .shared
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        // An unrelated native completion must reach the UI before publishing
        // this result. Do not consume, overwrite or borrow its selection.
        let available = self.shared.update.try_lock().is_ok_and(|pending| {
            pending
                .as_ref()
                .is_none_or(|update| !self.unread_native_continuation(update))
        });
        if available {
            self.finish_host_preparation(index);
        }
        self.shared.busy.store(false, Ordering::Release);
    }

    fn finish_host_preparation(&mut self, index: usize) {
        let Some(host) = &mut self.host else { return };
        let entry = &mut host.preparations[index];
        let result = (|| {
            let store = self.store.as_mut().ok_or_else(owner_changed)?;
            store
                .check_writer_owner(host.endpoint.owner_handle())
                .map_err(LiveError::store)?;
            if entry.session != self.session
                || self.shared.stopping.load(Ordering::Acquire)
                || self.pending_session_change.is_some()
            {
                return Err(owner_changed());
            }
            preparation::commit(
                store,
                &entry.command,
                entry
                    .prepared
                    .take()
                    .expect("prepared result selected above"),
                &entry.cancelled,
            )
        })();
        let state = match result {
            Err(error) => PreparationState::Failed { error },
            Ok(outcome) => {
                let refresh_error =
                    if outcome.inventory_changed || outcome.committed_revision.is_some() {
                        self.cached = None;
                        self.committed = None;
                        self.room_tone = None;
                        self.room_tone_error = None;
                        self.gain = None;
                        #[cfg(test)]
                        let inject_failure = self
                            .shared
                            .host_refresh_failure
                            .swap(false, Ordering::AcqRel);
                        #[cfg(not(test))]
                        let inject_failure = false;
                        let error = if inject_failure {
                            Some("Injected failure refreshing completed preparation".into())
                        } else {
                            self.refresh().err()
                        };
                        self.error = error.clone();
                        self.message = Some("Headless media operation saved".into());
                        self.publish();
                        error
                    } else {
                        None
                    };
                PreparationState::Completed {
                    output: outcome.output,
                    receipt: outcome.receipt,
                    committed_revision: outcome.committed_revision,
                    inventory_changed: outcome.inventory_changed,
                    completion_error: outcome.completion_error,
                    refresh_error,
                }
            }
        };
        if let Some(host) = &mut self.host {
            host.preparations[index].finish(state);
        }
    }
}

fn missing() -> LiveError {
    LiveError::new(
        "HostPreparationMissing",
        "No retained operation matches this exact target",
    )
}
