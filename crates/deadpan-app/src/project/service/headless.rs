//! Authenticated requests run on the existing project owner. Socket progress
//! is bounded and replies never consume the native UI's update mailbox.

use std::time::Instant;

use deadpan_cli::{
    host::Endpoint,
    live_project::{LiveError, Operation, Reply as HostReply, Request, execute_short},
    render::{
        self as public_render, RenderContext, RenderOperation, RenderRequest, RenderStatus,
        WorkflowTarget,
    },
};

use super::*;
use crate::project::{
    ProjectRenderContext, ProjectRenderLimits, ProjectRenderOperation, ProjectRenderRequest,
};

const MAX_OBSERVERS: usize = 8;
const TERMINAL_RETENTION: Duration = Duration::from_secs(10 * 60);
/// How long a replaced owner's endpoint may take to write its last replies.
const RETIRED_DRAIN: Duration = Duration::from_secs(15);
/// Replaced owners whose last replies may drain at once.
const MAX_RETIRED: usize = 4;
/// How long shutdown waits for produced replies to be written.
const SHUTDOWN_DRAIN: Duration = Duration::from_secs(2);

mod preparation;

pub(super) struct Host {
    endpoint: Endpoint,
    renders: Vec<Observation>,
    preparations: Vec<preparation::Observation>,
}

struct Observation {
    status: RenderStatus,
    finished: bool,
    expires: Option<Instant>,
}

impl Host {
    pub(super) fn bind(store: &mut ProjectStore) -> Result<Self> {
        Ok(Self {
            endpoint: Endpoint::bind(store).map_err(display)?,
            renders: Vec::new(),
            preparations: Vec::new(),
        })
    }

    fn expire(&mut self) {
        let now = Instant::now();
        self.renders
            .retain(|entry| entry.expires.is_none_or(|end| now < end));
        self.preparations
            .retain(|entry| entry.expires.is_none_or(|end| now < end));
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        for entry in &self.preparations {
            entry.cancelled.store(true, Ordering::Release);
        }
    }
}

impl Service {
    /// Keep a replaced owner's endpoint only to write already admitted
    /// replies. Several can drain at once; the oldest beyond a small bound
    /// is dropped, which its client observes as an unknown outcome.
    pub(super) fn retire_host(&mut self) {
        if let Some(mut host) = self.host.take() {
            host.endpoint.retire();
            if self.retired_hosts.len() >= MAX_RETIRED {
                self.retired_hosts.remove(0);
            }
            self.retired_hosts
                .push((host, Instant::now() + RETIRED_DRAIN));
        }
    }

    /// Write retired endpoints' last replies; forget the finished ones.
    fn drain_retired_hosts(&mut self) {
        #[cfg(test)]
        if self.shared.retired_drain_paused.load(Ordering::Acquire) {
            return;
        }
        let now = Instant::now();
        self.retired_hosts.retain_mut(|(host, until)| {
            host.endpoint.poll();
            host.endpoint.draining() && now < *until
        });
    }

    /// On shutdown: give retired endpoints, and the current one once
    /// retired, a bounded chance to write replies already produced.
    pub(super) fn drain_hosts_before_exit(&mut self) {
        self.retire_host();
        self.abandon_remote_storage();
        let deadline = Instant::now() + SHUTDOWN_DRAIN;
        #[cfg(test)]
        self.shared
            .retired_drain_paused
            .store(false, Ordering::Release);
        while !self.retired_hosts.is_empty() && Instant::now() < deadline {
            self.drain_retired_hosts();
            std::thread::sleep(Duration::from_millis(1));
        }
        self.retired_hosts.clear();
    }

    /// Reply to an admitted request on the endpoint that admitted it, if it
    /// is still current or retired and draining.
    pub(super) fn respond_on(
        &mut self,
        owner: uuid::Uuid,
        ticket: deadpan_cli::host::ConnectionTicket,
        reply: HostReply,
    ) {
        let fallback = reply_failure(&reply);
        let endpoint = match &mut self.host {
            Some(host) if host.endpoint.owner_id() == owner => Some(&mut host.endpoint),
            _ => self
                .retired_hosts
                .iter_mut()
                .find(|(host, _)| host.endpoint.owner_id() == owner)
                .map(|(host, _)| &mut host.endpoint),
        };
        if let Some(endpoint) = endpoint {
            let sent = serde_json::to_value(reply)
                .ok()
                .is_some_and(|value| endpoint.respond(ticket, value).is_ok());
            if !sent && let Ok(value) = serde_json::to_value(fallback) {
                // Socket loss can still make the outcome unknowable. A
                // locally rejected large reply must retain its receipt.
                let _ = endpoint.respond(ticket, value);
            }
        }
    }

    pub(super) fn pump_host(&mut self) {
        self.drain_retired_hosts();
        #[cfg(test)]
        if self.shared.host_poll_paused.load(Ordering::Acquire) {
            return;
        }
        let Some(host) = &mut self.host else { return };
        host.expire();
        let source = host.endpoint.owner_id();
        let requests = host.endpoint.poll();
        for incoming in requests {
            // A restore earlier in this batch replaced the owner these
            // requests authenticated against; never run them on its successor.
            let current = self.host.as_ref().map(|host| host.endpoint.owner_id());
            let reply = if current == Some(source) {
                match Request::from_value(incoming.payload) {
                    Ok(Request {
                        operation:
                            Operation::Execute {
                                project_id,
                                command,
                            },
                        ..
                    }) if super::remote_storage::deferred(&command) => {
                        // Long storage work runs off this thread; the reply
                        // follows when it finishes.
                        self.start_remote_storage(source, incoming.ticket, &project_id, &command)
                            .err()
                            .map(|error| HostReply::Failed { error })
                    }
                    request => Some(
                        request
                            .and_then(|request| self.host_request(request.operation))
                            .unwrap_or_else(|error| HostReply::Failed { error }),
                    ),
                }
            } else {
                Some(HostReply::Failed {
                    error: owner_changed(),
                })
            };
            if let Some(reply) = reply {
                self.respond_on(source, incoming.ticket, reply);
            }
            // A peer cannot consume an unbounded stream of admissions before
            // a render gets another chance to advance or observe cancellation.
            if self.pump_render() {
                self.publish();
            }
        }
    }

    /// The owner still holds the writer this endpoint was bound to and is
    /// not closing or replacing its session.
    pub(super) fn check_host_admission(&self) -> std::result::Result<(), LiveError> {
        let store = self.store.as_ref().ok_or_else(owner_changed)?;
        let host = self.host.as_ref().ok_or_else(owner_changed)?;
        store
            .check_writer_owner(host.endpoint.owner_handle())
            .map_err(LiveError::store)?;
        if self.shared.stopping.load(Ordering::Acquire) || self.pending_session_change.is_some() {
            return Err(owner_changed());
        }
        Ok(())
    }

    fn host_request(&mut self, operation: Operation) -> std::result::Result<HostReply, LiveError> {
        self.check_host_admission()?;
        let store = self.store.as_ref().ok_or_else(owner_changed)?;
        match operation {
            Operation::Inspect => Ok(HostReply::Context {
                context: RenderContext::from_document(&store.snapshot().map_err(LiveError::store)?),
                preview_active: self.shared.preview_active.load(Ordering::Acquire),
            }),
            Operation::RenderStatus { project_id, target } => {
                self.check_host_project(&project_id)?;
                self.host_render_status(&target)
            }
            Operation::PreparationStatus { project_id, target } => {
                self.check_host_project(&project_id)?;
                self.host_preparation_status(&target)
            }
            Operation::CancelPreparation { project_id, target } => {
                self.check_host_project(&project_id)?;
                self.host_cancel_preparation(&target)
            }
            Operation::ReleasePreparationStatus { project_id, target } => {
                self.check_host_project(&project_id)?;
                self.host_release_preparation(&target)
            }
            Operation::GenerationStatus { project_id, job } => {
                self.check_host_project(&project_id)?;
                self.host_generation_status(job)
            }
            Operation::ReleaseGenerationStatus { project_id, job } => {
                self.check_host_project(&project_id)?;
                self.host_release_generation(job)
            }
            Operation::CancelGeneration { project_id, job } => {
                // Cancellation remains available while a command is queued.
                self.check_host_project(&project_id)?;
                self.host_cancel_generation(job)
            }
            Operation::ReleaseRenderStatus { project_id, target } => {
                self.check_host_project(&project_id)?;
                let HostReply::Render { finished, .. } = self.host_render_status(&target)? else {
                    unreachable!()
                };
                if !finished {
                    return Err(LiveError::new(
                        "RenderBusy",
                        "The render still owns unfinished work",
                    ));
                }
                self.host
                    .as_mut()
                    .ok_or_else(owner_changed)?
                    .renders
                    .retain(|entry| entry.status.target.as_ref() != Some(&target));
                Ok(HostReply::Released)
            }
            Operation::Render { request }
                if matches!(request.operation, RenderOperation::Cancel { .. }) =>
            {
                // Cancellation remains available while a normal UI command is
                // queued. Exact target identity is validated by the coordinator.
                self.host_render(request)
            }
            operation => {
                self.shared
                    .busy
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .map_err(|_| {
                        LiveError::new("HostBusy", "The project is processing another command")
                    })?;
                let result = if self.shared.stopping.load(Ordering::Acquire) {
                    Err(owner_changed())
                } else {
                    match operation {
                        Operation::Execute {
                            project_id,
                            command,
                        } => self.host_edit(&project_id, &command),
                        Operation::Render { request } => self.host_render(request),
                        Operation::Prepare {
                            project_id,
                            target,
                            command,
                        } => self.host_prepare(&project_id, target, *command),
                        Operation::Generate {
                            project_id,
                            request,
                        } => self.host_generate(&project_id, request),
                        _ => unreachable!("non-admitting operations handled above"),
                    }
                };
                // Only release the admission acquired above, never a queued
                // native command's busy flag.
                self.shared.busy.store(false, Ordering::Release);
                result
            }
        }
    }

    pub(super) fn check_host_project(
        &self,
        project: &ProjectId,
    ) -> std::result::Result<(), LiveError> {
        // Project identity cannot change inside an admitted store session.
        // Polling progress must not decode the entire document every 50 ms.
        if self
            .workspace
            .as_ref()
            .ok_or_else(owner_changed)?
            .document
            .project_id()
            != project
        {
            return Err(LiveError::new(
                "HostProjectChanged",
                "The request names another project",
            ));
        }
        Ok(())
    }

    fn host_edit(
        &mut self,
        project: &ProjectId,
        command: &deadpan_cli::live_project::ShortOperation,
    ) -> std::result::Result<HostReply, LiveError> {
        if !command.is_preview() {
            let pending = self.shared.update.try_lock().map_err(|_| {
                LiveError::new("HostBusy", "The native app is receiving an edit result")
            })?;
            if pending
                .as_ref()
                .is_some_and(|update| self.unread_native_continuation(update))
            {
                return Err(LiveError::new(
                    "HostBusy",
                    "The native app has an unread edit or register receipt",
                ));
            }
        }
        use deadpan_cli::live_project::ShortOperation as Short;
        if matches!(command, Short::Take { .. })
            && self.shared.preview_active.load(Ordering::Acquire)
        {
            return Err(LiveError::new(
                "TakePreviewActive",
                "Finish or cancel the unsaved preview before changing takes.",
            ));
        }
        match command {
            Short::RestoreBackup {
                id,
                expected_revision,
            } => {
                self.check_host_project(project)?;
                return Ok(HostReply::Completed {
                    output: self.host_restore(id, expected_revision.as_ref())?,
                    committed_revision: None,
                    committed_registers: None,
                    refresh_error: None,
                });
            }
            // Only a file plan reaches here (Storage R); other cleanup runs
            // as an off-thread job (see `remote_storage`).
            Short::CleanStorage { .. } | Short::ConfirmVariantClock { .. } => {
                // The same refusal as the Storage panel's removal: no job may
                // be publishing media meanwhile.
                if let Some(reason) = self.retention_jobs_running() {
                    return Err(LiveError::new("StorageBusy", reason));
                }
                if self.retention.running() {
                    return Err(LiveError::new(
                        "StorageBusy",
                        "The automatic AI variant check is running; try again when it finishes",
                    ));
                }
                // Like the panel, an open project keeps at least the default
                // grace period; a shorter one needs the writer exclusively.
                if let Short::CleanStorage { grace_seconds, .. } = command
                    && *grace_seconds < deadpan_store::storage::DEFAULT_GRACE.as_secs()
                {
                    return Err(LiveError::new(
                        "StorageGraceRefused",
                        "Deadpan has this project open, so cleanup keeps at least the default 24-hour grace period. Close the project to use a shorter one.",
                    ));
                }
            }
            _ => {}
        }
        let execution = match command {
            deadpan_cli::live_project::ShortOperation::Macro { request } => {
                self.check_host_project(project)?;
                if &request.project_id != project {
                    return Err(LiveError::new(
                        "HostProjectChanged",
                        "Macro and host project identities differ",
                    ));
                }
                let prepared = deadpan_cli::macros::prepare(
                    self.store.as_ref().ok_or_else(owner_changed)?,
                    request,
                )?;
                let bank = self
                    .prepare_remote_macro_registers(&prepared)
                    .map_err(|error| LiveError::new("HostMacroPreparationFailed", error))?;
                let execution = deadpan_cli::macros::commit(
                    self.store.as_mut().ok_or_else(owner_changed)?,
                    &prepared,
                )?;
                if let Some(bank) = bank {
                    self.registers = Some(bank);
                }
                execution
            }
            _ => execute_short(
                self.store.as_mut().ok_or_else(owner_changed)?,
                project,
                command,
            )?,
        };
        let deadpan_cli::macros::Execution {
            output,
            committed_revision,
            committed_registers,
        } = execution;
        let mut refresh_error = None;
        if !command.is_preview() {
            self.capture_preparation_output(&output);
            self.host_operational_change(command, &output);
        }
        if let Some(revision) = &committed_revision {
            use deadpan_cli::live_project::ShortOperation;
            let preserved_from = match command {
                ShortOperation::History {
                    expected_revision, ..
                } => Some(expected_revision),
                ShortOperation::Edit { request, .. }
                    if matches!(
                        request.command,
                        Command::SetMark { .. } | Command::DeleteMark { .. }
                    ) =>
                {
                    Some(&request.expected_revision)
                }
                _ => None,
            };
            if let Some(before) = preserved_from {
                self.preserve_semantic(before, revision);
            }
            self.committed = None;
            self.room_tone = None;
            self.room_tone_error = None;
            self.gain = None;
        }
        if committed_revision.is_some() || committed_registers.is_some() {
            #[cfg(test)]
            let inject_failure = self
                .shared
                .host_refresh_failure
                .swap(false, Ordering::AcqRel);
            #[cfg(not(test))]
            let inject_failure = false;
            refresh_error = if inject_failure {
                Some("Injected failure refreshing the committed headless edit".into())
            } else {
                self.refresh().err()
            };
            self.set_error(refresh_error.clone());
            self.message = Some(match &committed_revision {
                Some(revision) => format!("Headless edit saved at revision {revision}"),
                None => "Headless macro saved in the register bank".into(),
            });
            self.publish();
        }
        // A failed workspace refresh cannot convert a successful transaction
        // into a retryable error or erase the acknowledged durable revision.
        Ok(HostReply::Completed {
            output,
            committed_revision,
            committed_registers,
            refresh_error,
        })
    }

    /// Native state that a remote operational change (outside authored
    /// history) makes stale: offered AI variants, analysis corrections and
    /// the retention status. Published at once, like a native change.
    fn host_operational_change(
        &mut self,
        command: &deadpan_cli::live_project::ShortOperation,
        output: &serde_json::Value,
    ) {
        use deadpan_cli::generation::variants::VariantAction;
        use deadpan_cli::live_project::ShortOperation as Short;
        let message = match command {
            Short::Take { .. } => {
                if output["outcome"]["changed"] == true && output["outcome"]["commit"].is_null() {
                    self.storage_watch.record_take_save();
                }
                match serde_json::from_value(output["outcome"]["catalog"].clone()) {
                    Ok(catalog) => self.take_catalog = Some(catalog),
                    Err(error) => self.set_error(Some(format!(
                        "Take saved, but its catalog reply could not be read: {error}"
                    ))),
                }
                // Restore publishes the catalog together with its refreshed
                // workspace, or with the retained post-commit refresh failure.
                if !output["outcome"]["commit"].is_null() {
                    return;
                }
                "Updated named takes from the command line."
            }
            Short::GenerationVariant {
                request,
                attempt,
                action,
            } => {
                self.generation
                    .remote_variant_changed(request, attempt, *action);
                match action {
                    VariantAction::Select => "Chose an AI variant from the command line.",
                    VariantAction::Discard => {
                        "Discarded an AI variant from the command line. The pause is unchanged."
                    }
                    VariantAction::Keep => "Kept an AI variant from the command line.",
                    VariantAction::Release => {
                        "Stopped keeping an AI variant from the command line."
                    }
                }
            }
            Short::RetryGenerationPreparation { .. }
            | Short::CancelGenerationPreparation { .. } => {
                self.generation.variants_changed();
                "Updated the replacement preparation from the command line. The pause's timing is unchanged."
            }
            Short::DismissInterruptedAttempt { .. } => {
                self.generation.variants_changed();
                "Discarded an interrupted AI attempt from the command line. The pause is unchanged."
            }
            Short::Corrections { .. } => {
                if let Err(error) = self.reload_corrections() {
                    self.set_error(Some(error));
                }
                "Saved a transcript correction from the command line."
            }
            Short::CleanStorage { .. } | Short::ConfirmVariantClock { .. } => {
                if output["variant_expiry"]["expired"]
                    .as_array()
                    .is_some_and(|expired| !expired.is_empty())
                {
                    self.generation.variants_changed();
                }
                self.retention.recheck_soon();
                if matches!(command, Short::CleanStorage { .. }) {
                    "Cleaned up project storage from the command line."
                } else {
                    "Confirmed the clock for AI variant retention from the command line."
                }
            }
            _ => return,
        };
        self.message = Some(message.into());
        self.publish();
    }

    fn host_render(&mut self, request: RenderRequest) -> std::result::Result<HostReply, LiveError> {
        self.check_host_project(&request.context.project_id)?;
        let cancel = matches!(&request.operation, RenderOperation::Cancel { .. });
        if !cancel && self.shared.preview_active.load(Ordering::Acquire) {
            return Err(LiveError::new(
                "RenderPreviewDecisionRequired",
                "A temporary Camera, Gain or Room tone preview is open. Commit or discard it in the app before rendering.",
            ));
        }
        if !cancel && self.host.as_ref().ok_or_else(owner_changed)?.renders.len() >= MAX_OBSERVERS {
            return Err(LiveError::new(
                "HostRenderLimit",
                "Release a retained render status or wait for its ten-minute expiry",
            ));
        }
        let workspace = self.workspace.as_ref().ok_or_else(owner_changed)?;
        let store = self.store.as_ref().ok_or_else(owner_changed)?;
        let context = ProjectRenderContext {
            session: workspace.session,
            project: request.context.project_id.clone(),
        };
        let limits = public_render::default_limits().map_err(public_error)?;
        let limits = ProjectRenderLimits {
            encode: limits.encode,
            verification: limits.verification,
            media: limits.media,
        };
        let operation = match request.operation {
            RenderOperation::Start { destination } => {
                let document = store.snapshot().map_err(LiveError::store)?;
                if document.revision_id() != &request.context.revision_id {
                    return Err(LiveError {
                        current_revision: Some(document.revision_id().clone()),
                        ..LiveError::new(
                            "RenderRevisionChanged",
                            "The captured committed revision changed",
                        )
                    });
                }
                let summary = public_render::committed_output_summary(store, &document)
                    .map_err(public_error)?;
                ProjectRenderOperation::Start {
                    request: public_render::start_request(
                        &request.context,
                        summary.algorithm,
                        destination,
                        Instant::now(),
                    )
                    .map_err(public_error)?,
                    limits,
                }
            }
            RenderOperation::RetryCheckpoint {
                job_id,
                encoding_attempt_id,
                destination,
            } => ProjectRenderOperation::Retry {
                request: public_render::retry_request(
                    store,
                    &request.context,
                    &job_id,
                    Some(&encoding_attempt_id),
                    destination,
                    Instant::now(),
                )
                .map_err(public_error)?,
                limits,
            },
            RenderOperation::Reencode {
                job_id,
                destination,
            } => ProjectRenderOperation::Retry {
                request: public_render::retry_request(
                    store,
                    &request.context,
                    &job_id,
                    None,
                    destination,
                    Instant::now(),
                )
                .map_err(public_error)?,
                limits,
            },
            RenderOperation::Reconcile { publication_id } => ProjectRenderOperation::Reconcile {
                request: public_render::reconcile_request(
                    store,
                    &request.context,
                    &publication_id,
                    Instant::now(),
                )
                .map_err(public_error)?,
                limits,
            },
            RenderOperation::Cancel { target } => ProjectRenderOperation::Cancel(target.into()),
        };
        let mut committed = None;
        let result = self.admit_render(
            &ProjectRenderRequest {
                ticket: 1,
                context,
                operation,
            },
            &mut committed,
        );
        self.capture_render_status();
        self.publish();
        let identity = result.map_err(|error| LiveError::new(error.code, error.message))?;
        let target = WorkflowTarget::from(&identity);
        let reply = self.host_render_status(&target)?;
        if !cancel && let HostReply::Render { status, finished } = &reply {
            self.host
                .as_mut()
                .ok_or_else(owner_changed)?
                .renders
                .push(Observation {
                    status: *status.clone(),
                    finished: *finished,
                    expires: finished.then(|| Instant::now() + TERMINAL_RETENTION),
                });
        }
        Ok(reply)
    }

    fn host_render_status(
        &self,
        target: &WorkflowTarget,
    ) -> std::result::Result<HostReply, LiveError> {
        if let Some(render) = &self.render
            && render
                .workflow
                .status()
                .identity
                .as_ref()
                .map(WorkflowTarget::from)
                .as_ref()
                == Some(target)
        {
            let context = RenderContext {
                project_id: render.context.project.clone(),
                revision_id: render.revision.clone(),
            };
            return Ok(HostReply::Render {
                status: Box::new(RenderStatus::from_workflow(
                    &context,
                    render.workflow.status(),
                )),
                finished: render.workflow.can_release_writer(),
            });
        }
        let entry = self
            .host
            .as_ref()
            .ok_or_else(owner_changed)?
            .renders
            .iter()
            .find(|entry| entry.status.target.as_ref() == Some(target))
            .ok_or_else(|| {
                LiveError::new(
                    "RenderStatusUnavailable",
                    "The target is unknown or its retained status expired",
                )
            })?;
        Ok(HostReply::Render {
            status: Box::new(entry.status.clone()),
            finished: entry.finished,
        })
    }

    pub(super) fn retain_host_render_status(&mut self) {
        let (Some(host), Some(render)) = (&mut self.host, &self.render) else {
            return;
        };
        let target = render
            .workflow
            .status()
            .identity
            .as_ref()
            .map(WorkflowTarget::from);
        let Some(entry) = host
            .renders
            .iter_mut()
            .find(|entry| entry.status.target == target)
        else {
            return;
        };
        let context = RenderContext {
            project_id: render.context.project.clone(),
            revision_id: render.revision.clone(),
        };
        entry.status = RenderStatus::from_workflow(&context, render.workflow.status());
        entry.finished = render.workflow.can_release_writer();
        if entry.finished && entry.expires.is_none() {
            entry.expires = Some(Instant::now() + TERMINAL_RETENTION);
        }
    }
}

impl Service {
    /// A native edit, copy, cut or Macro receipt in the pending update that
    /// the UI has not taken yet. Receipts it already took and that background
    /// publishes merely re-send are read.
    pub(super) fn unread_native_continuation(&self, update: &ProjectUpdate) -> bool {
        update.continuation_key().unread_since(
            &self
                .shared
                .delivered
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        )
    }
}

pub(super) fn owner_changed() -> LiveError {
    LiveError::new(
        "HostOwnerChanged",
        "The project owner is closing or unavailable",
    )
}

fn public_error(error: public_render::PublicRenderError) -> LiveError {
    LiveError {
        code: error.code,
        message: error.message,
        current_revision: error.current_revision,
        committed_revision: None,
        committed_registers: None,
    }
}

fn reply_failure(reply: &HostReply) -> HostReply {
    if let HostReply::Preparation { status } = reply {
        let mut status = status.clone();
        match &mut status.state {
            deadpan_cli::live_project::preparation::PreparationState::Completed {
                output,
                refresh_error,
                completion_error,
                ..
            } => {
                *output = serde_json::json!({
                    "protocol": 1,
                    "host_reply_detail_omitted": true,
                    "message": "The operation completed. Its exact receipt is retained; inspect before repeating a mutation.",
                });
                if refresh_error.is_some() {
                    *refresh_error = Some(
                        "Native workspace refresh failed after the completed operation".into(),
                    );
                }
                if let Some(error) = completion_error {
                    error.message = "The operation was published, but final durability could not be confirmed. Inspect its retained receipt before repeating.".into();
                    if error.code.len() > 128 {
                        error.code = "HostPublishedUnconfirmed".into();
                    }
                }
            }
            deadpan_cli::live_project::preparation::PreparationState::Failed { error } => {
                error.message =
                    "Preparation failed; its diagnostic exceeded transport capacity".into();
                if error.code.len() > 128 {
                    error.code = "HostReplyLimit".into();
                }
            }
            _ => {}
        }
        return HostReply::Preparation { status };
    }
    if let HostReply::Failed { error } = reply {
        let mut error = error.clone();
        error.message =
            "The operation failed; its full diagnostic exceeded transport capacity".into();
        if error.code.len() > 128 {
            error.code = "HostReplyLimit".into();
        }
        return HostReply::Failed { error };
    }
    let (committed_revision, committed_registers) = match reply {
        HostReply::Completed {
            committed_revision,
            committed_registers,
            ..
        } => (committed_revision.clone(), committed_registers.clone()),
        _ => (None, None),
    };
    HostReply::Failed {
        error: LiveError {
            committed_revision,
            committed_registers: committed_registers.map(Box::new),
            ..LiveError::new(
                "HostReplyLimit",
                "The operation executed but its full reply exceeded transport capacity. Inspect the project before repeating a mutation.",
            )
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_host_reply_preserves_commit_even_after_refresh_failure() {
        let revision = RevisionId::new("saved-before-reply-limit").unwrap();
        let reply = HostReply::Completed {
            output: serde_json::json!({"committed":true,"outcome":{"large":"detail"}}),
            committed_revision: Some(revision.clone()),
            committed_registers: None,
            refresh_error: Some("UI refresh failed".into()),
        };
        let HostReply::Failed { error } = reply_failure(&reply) else {
            panic!("compact error")
        };
        assert_eq!(error.code, "HostReplyLimit");
        assert_eq!(error.committed_revision, Some(revision));
        assert!(error.message.contains("Inspect the project"));
    }

    #[test]
    fn compact_host_error_keeps_conflict_identity_without_claiming_a_commit() {
        let revision = RevisionId::new("actual-current-revision").unwrap();
        let reply = HostReply::Failed {
            error: LiveError {
                current_revision: Some(revision.clone()),
                ..LiveError::new("RevisionConflict", "full conflict detail")
            },
        };
        let HostReply::Failed { error } = reply_failure(&reply) else {
            panic!("compact error")
        };
        assert_eq!(error.code, "RevisionConflict");
        assert_eq!(error.current_revision, Some(revision));
        assert!(error.committed_revision.is_none());
    }

    #[test]
    fn compact_host_reply_preserves_exact_bank_only_and_authored_macro_receipts() {
        use deadpan_cli::macros::RegisterReceipt;

        for authored in [false, true] {
            let bank = RegisterReceipt {
                project_id: ProjectId::new("saved-macro-project").unwrap(),
                revision_id: RevisionId::new("saved-macro-revision").unwrap(),
                bank_version: 37,
            };
            let revision = authored.then(|| bank.revision_id.clone());
            let reply = HostReply::Completed {
                output: serde_json::json!({"large":"x".repeat(1_000_000)}),
                committed_revision: revision.clone(),
                committed_registers: Some(bank.clone()),
                refresh_error: Some("Workspace refresh failed after saving".into()),
            };
            let compact = reply_failure(&reply);
            assert!(serde_json::to_vec(&compact).unwrap().len() < 4096);
            let HostReply::Failed { error } = compact else {
                panic!("expected bounded receipt")
            };
            assert_eq!(error.code, "HostReplyLimit");
            assert_eq!(error.committed_revision, revision);
            assert_eq!(error.committed_registers.as_deref(), Some(&bank));
            assert!(error.message.contains("Inspect the project"));
            let repeated = reply_failure(&HostReply::Failed {
                error: error.clone(),
            });
            let HostReply::Failed { error: repeated } = repeated else {
                panic!("expected retained failure receipt")
            };
            assert_eq!(repeated.committed_registers, error.committed_registers);
            assert_eq!(repeated.committed_revision, error.committed_revision);
        }
    }

    #[test]
    fn compact_preparation_reply_retains_published_checkpoint_and_durability_failure() {
        use deadpan_cli::live_project::preparation::{
            PreparationReceipt, PreparationState, PreparationStatus, PreparationTarget,
        };
        let target = PreparationTarget::fresh();
        let receipt = PreparationReceipt::Checkpoint {
            path: PathBuf::from("/project.deadpan/Snapshots/retained.sqlite3"),
            project_id: ProjectId::new("checkpoint-project").unwrap(),
            revision_id: RevisionId::new("captured-checkpoint").unwrap(),
        };
        let reply = HostReply::Preparation {
            status: Box::new(PreparationStatus {
                target: target.clone(),
                state: PreparationState::Completed {
                    output: serde_json::json!({"large": "x".repeat(1_000_000)}),
                    receipt: receipt.clone(),
                    committed_revision: None,
                    inventory_changed: false,
                    completion_error: Some(LiveError::new(
                        "CheckpointPublishedUnconfirmed",
                        "directory sync failed",
                    )),
                    refresh_error: None,
                },
            }),
        };
        let compact = reply_failure(&reply);
        assert!(serde_json::to_vec(&compact).unwrap().len() < 4096);
        let HostReply::Preparation { status } = compact else {
            panic!("preparation receipt")
        };
        assert_eq!(status.target, target);
        let PreparationState::Completed {
            output,
            receipt: retained,
            committed_revision,
            inventory_changed,
            completion_error,
            ..
        } = status.state
        else {
            panic!("completed publication")
        };
        assert_eq!(retained, receipt);
        assert_eq!(output["host_reply_detail_omitted"], true);
        assert!(committed_revision.is_none() && !inventory_changed);
        assert_eq!(
            completion_error.unwrap().code,
            "CheckpointPublishedUnconfirmed"
        );
    }
}
