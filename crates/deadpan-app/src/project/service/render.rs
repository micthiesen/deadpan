//! Native ownership of the shared render workflow and exact preview commits.

use deadpan_cli::encoded_render::workflow::{
    RenderWorkflow, WorkflowConfig, WorkflowError, WorkflowIdentity,
};
use deadpan_cli::render_worker::RenderWorkerRuntime;

use super::*;
use crate::project::{
    ProjectRenderCommandOutcome, ProjectRenderContext, ProjectRenderError, ProjectRenderLimits,
    ProjectRenderOperation, ProjectRenderRequest, ProjectRenderStatus,
};

pub(super) struct NativeRender {
    pub(super) workflow: RenderWorkflow,
    pub(super) context: ProjectRenderContext,
    pub(super) revision: RevisionId,
}

pub(super) struct PreparedOpen {
    pub store: ProjectStore,
    pub workspace: Workspace,
    pub registers: Arc<super::super::registers::Bank>,
    pub message: String,
    pub report: Arc<super::super::OpenReport>,
}

pub(super) enum PendingSessionChange {
    Close,
    Open(Box<PreparedOpen>),
    CreateFromSource(PathBuf),
    #[cfg(test)]
    Create(PathBuf),
    Shutdown,
}

impl Service {
    /// True completes the short user command; false retains its admission while
    /// a requested session replacement drains the old writer's render work.
    pub(super) fn dispatch_request(&mut self, request: ProjectRequest) -> bool {
        if let Some(refusal) = self.read_only_refusal(&request) {
            // Ticketed backup requests are answered on their own channel too.
            if let ProjectRequest::Backup(
                super::super::backups::Request::Now { ticket, .. }
                | super::super::backups::Request::Restore { ticket, .. },
            ) = &request
            {
                self.answer_backup(*ticket, Err(refusal.clone()));
            }
            self.error = Some(refusal);
            self.message = None;
            return true;
        }
        if let ProjectRequest::Backup(request) = request {
            self.backup_command(request);
            return true;
        }
        if let ProjectRequest::CaptureOriginal(request) = request {
            self.capture_original_command(request);
            return true;
        }
        if let ProjectRequest::CaptureEditSlice(request) = request {
            self.capture_edit_slice_command(request);
            return true;
        }
        if let ProjectRequest::RenderHistory(request) = request {
            self.render_history_command(request);
            return true;
        }
        if let ProjectRequest::Render(request) = request {
            self.render_command(request);
            return true;
        }
        #[cfg(any(test, feature = "ui-harness"))]
        if matches!(request, ProjectRequest::Edit { .. })
            && self.shared.storage_failure.swap(false, Ordering::AcqRel)
        {
            let error = display(StoreError::Io(std::io::Error::from(
                std::io::ErrorKind::StorageFull,
            )));
            self.error = Some(error);
            self.message = None;
            return true;
        }
        let outcome = if self.render.is_some() || self.generation.active() || self.targets.active()
        {
            self.defer_session_change(request)
        } else {
            self.command(request).map(|()| true)
        };
        match outcome {
            Ok(complete) => {
                self.error = None;
                complete
            }
            Err(error) => {
                self.error = Some(error);
                self.message = None;
                true
            }
        }
    }

    fn defer_session_change(&mut self, request: ProjectRequest) -> Result<bool> {
        let replacing = matches!(
            &request,
            ProjectRequest::Close
                | ProjectRequest::Open(_)
                | ProjectRequest::CreateFromSource { .. }
        );
        #[cfg(test)]
        let replacing = replacing || matches!(&request, ProjectRequest::Create(_));
        if replacing {
            self.committed = None;
            self.room_tone = None;
            self.room_tone_error = None;
            self.gain = None;
        }
        let pending = match request {
            ProjectRequest::Close => PendingSessionChange::Close,
            ProjectRequest::Open(path) => match self.prepare_open(path, false)? {
                Some(prepared) => PendingSessionChange::Open(Box::new(prepared)),
                None => {
                    self.message = Some("Project is already open".into());
                    return Ok(true);
                }
            },
            ProjectRequest::CreateFromSource { path } => {
                if self.active.is_some() || self.host_preparation_active() {
                    return Err(
                        "Wait for the current import to stop before creating a project".into(),
                    );
                }
                PendingSessionChange::CreateFromSource(path)
            }
            #[cfg(test)]
            ProjectRequest::Create(path) => PendingSessionChange::Create(path),
            request => return self.command(request).map(|()| true),
        };
        self.pending_session_change = Some(pending);
        self.cancel();
        self.cancel_render_for_transition();
        self.cancel_generation();
        self.cancel_tracking();
        Ok(false)
    }

    pub(super) fn render_command(&mut self, request: ProjectRenderRequest) {
        let mut committed_revision = None;
        let result = self.admit_render(&request, &mut committed_revision);
        self.render_update
            .get_or_insert_with(Default::default)
            .command = Some(ProjectRenderCommandOutcome {
            ticket: request.ticket,
            context: request.context,
            committed_revision,
            result,
        });
        self.capture_render_status();
    }

    pub(super) fn admit_render(
        &mut self,
        request: &ProjectRenderRequest,
        committed_revision: &mut Option<RevisionId>,
    ) -> std::result::Result<WorkflowIdentity, ProjectRenderError> {
        if request.ticket == 0 {
            return Err(native_error(
                "RenderInvalidRequest",
                "Render request ticket must be nonzero",
            ));
        }
        let workspace = self.workspace.as_ref().ok_or_else(|| {
            native_error(
                "RenderContextChanged",
                "Open a project before requesting a render",
            )
        })?;
        if workspace.session != request.context.session
            || workspace.document.project_id() != &request.context.project
        {
            return Err(native_error(
                "RenderContextChanged",
                "Project session changed before the render request",
            ));
        }
        if self.pending_session_change.is_some() {
            return Err(native_error(
                "RenderContextChanged",
                "The project session is closing or changing",
            ));
        }
        if let ProjectRenderOperation::Cancel(identity) = &request.operation {
            let render = self.render.as_mut().ok_or_else(|| {
                native_error(
                    "RenderIdentityChanged",
                    "There is no matching render workflow",
                )
            })?;
            let store = self.store.as_mut().ok_or_else(|| {
                native_error("RenderContextChanged", "The project writer is unavailable")
            })?;
            render
                .workflow
                .cancel(store, identity)
                .map_err(workflow_error)?;
            return Ok(identity.clone());
        }
        if self
            .render
            .as_ref()
            .is_some_and(|render| render.workflow.is_active())
        {
            return Err(workflow_error(WorkflowError::Busy));
        }
        if let ProjectRenderOperation::Recover(recovery) = &request.operation {
            let operation = self.prepare_render_recovery(&request.context, recovery)?;
            return self.admit_render(
                &ProjectRenderRequest {
                    ticket: request.ticket,
                    context: request.context.clone(),
                    operation,
                },
                committed_revision,
            );
        }
        let (mut revision, limits) = match &request.operation {
            ProjectRenderOperation::Start { request, limits }
            | ProjectRenderOperation::CommitAndStart {
                request, limits, ..
            } => {
                if workspace.document.revision_id() != &request.revision {
                    return Err(workflow_error(WorkflowError::StaleRevision));
                }
                (request.revision.clone(), limits)
            }
            ProjectRenderOperation::Retry { request, limits } => {
                let revision = self
                    .store
                    .as_ref()
                    .ok_or_else(|| {
                        native_error("RenderContextChanged", "The project writer is unavailable")
                    })?
                    .render_job(&request.identity.job_id)
                    .map_err(|error| workflow_error(WorkflowError::Store(error)))?
                    .revision_id;
                (revision, limits)
            }
            ProjectRenderOperation::Reconcile { request, limits } => {
                let revision = self
                    .store
                    .as_ref()
                    .ok_or_else(|| {
                        native_error("RenderContextChanged", "The project writer is unavailable")
                    })?
                    .render_publication(&request.publication_id)
                    .map_err(|error| workflow_error(WorkflowError::Store(error)))?
                    .render_intent
                    .revision_id;
                (revision, limits)
            }
            ProjectRenderOperation::Cancel(_) => unreachable!("cancellation was handled above"),
            ProjectRenderOperation::Recover(_) => unreachable!("recovery was resolved above"),
        };
        let package = workspace.path.clone();
        if let ProjectRenderOperation::CommitAndStart { edit, .. } = &request.operation
            && !matches!(
                edit.as_ref(),
                ProjectEdit::SetFraming { .. }
                    | ProjectEdit::SetAudioTreatments { .. }
                    | ProjectEdit::HoldAudio { .. }
                    | ProjectEdit::Scoped {
                        edit: deadpan_core::ScopedNodeEdit::SetFraming { .. }
                            | deadpan_core::ScopedNodeEdit::SetAudioTreatments { .. }
                            | deadpan_core::ScopedNodeEdit::SetHoldAudio { .. },
                        ..
                    }
            )
        {
            return Err(native_error(
                "RenderInvalidRequest",
                "Commit and render requires a Camera, Gain or Hold audio preview",
            ));
        }
        let unchanged = match &request.operation {
            ProjectRenderOperation::CommitAndStart { edit, scope, cursor, request: start, .. } => match edit.as_ref() {
                ProjectEdit::Scoped { target, edit } => {
                    target.validate_request(workspace, request.context.session, &start.revision, scope, *cursor)
                        .map_err(|error| native_error("RenderInvalidRequest", error))?;
                    self.check_scoped_head(target).map_err(|error| native_error("RenderInvalidRequest", error))?;
                    super::scoped::unchanged(workspace, target, edit)
                        .map_err(|error| native_error("RenderInvalidRequest", error))?
                }
                ProjectEdit::SetFraming { node, framing } => workspace.document.nodes()
                    .get(node).is_some_and(|beat| &beat.framing == framing),
                ProjectEdit::SetAudioTreatments { node, treatments } => workspace.document.nodes()
                    .get(node).is_some_and(|beat| &beat.audio_treatments == treatments),
                ProjectEdit::HoldAudio { node, audio } => workspace.document.nodes()
                    .get(node).is_some_and(|beat| matches!(&beat.kind, NodeKind::Hold { recipe } if &recipe.audio == audio)),
                _ => false,
            },
            _ => false,
        };
        // A completed workflow must release its lease before a preview commits.
        // Unknown cleanup therefore cannot accidentally author a new revision.
        self.release_inactive_render()?;
        let committed_start = if let ProjectRenderOperation::CommitAndStart {
            edit,
            cursor,
            scope,
            request: start,
            ..
        } = &request.operation
        {
            self.committed = None;
            if unchanged {
                return Err(unchanged_preview());
            }
            #[cfg(test)]
            {
                self.render_preview_refresh_failure = self
                    .shared
                    .render_commit_refresh_failure
                    .swap(false, Ordering::AcqRel);
            }
            let edit_result = self.edit(
                request.context.session,
                start.revision.clone(),
                *cursor,
                scope.clone(),
                edit.as_ref().clone(),
            );
            #[cfg(test)]
            {
                self.render_preview_refresh_failure = false;
            }
            // This marker comes only from this synchronous typed edit's store
            // receipt. Workspace changes and matching nodes are not evidence.
            *committed_revision = self
                .committed
                .as_ref()
                .map(|commit| commit.revision.clone());
            if committed_revision.is_some() {
                self.room_tone = None;
                self.room_tone_error = None;
                self.gain = None;
            }
            edit_result.map_err(|error| native_error("RenderPreviewCommitFailed", error))?;
            revision = committed_revision
                .as_ref()
                .filter(|revision| *revision != &start.revision)
                .cloned()
                .ok_or_else(unchanged_preview)?;
            self.error = None;
            let mut start = start.clone();
            start.revision = revision.clone();
            Some(start)
        } else {
            None
        };
        let store = self.store.as_mut().ok_or_else(|| {
            native_error("RenderContextChanged", "The project writer is unavailable")
        })?;
        let config = native_config(package, limits)?;
        let workflow = RenderWorkflow::new(store, config).map_err(workflow_error)?;
        self.render = Some(NativeRender {
            workflow,
            context: request.context.clone(),
            revision,
        });
        let render = self
            .render
            .as_mut()
            .expect("render coordinator was installed");
        let identity = match &request.operation {
            ProjectRenderOperation::CommitAndStart { .. } => {
                let start = committed_start.expect("preview commit acknowledged above");
                let identity = start.identity.clone();
                render
                    .workflow
                    .start(store, start)
                    .map_err(workflow_error)?;
                identity
            }
            ProjectRenderOperation::Start { request, .. } => {
                render
                    .workflow
                    .start(store, request.clone())
                    .map_err(workflow_error)?;
                request.identity.clone()
            }
            ProjectRenderOperation::Retry { request, .. } => {
                render
                    .workflow
                    .retry(store, request.clone())
                    .map_err(workflow_error)?;
                request.identity.clone()
            }
            ProjectRenderOperation::Reconcile { request, .. } => {
                render
                    .workflow
                    .reconcile(store, request.clone())
                    .map_err(workflow_error)?;
                request.identity.clone()
            }
            ProjectRenderOperation::Cancel(_) => unreachable!("cancellation was handled above"),
            ProjectRenderOperation::Recover(_) => unreachable!("recovery was resolved above"),
        };
        self.render_update
            .get_or_insert_with(Default::default)
            .service_error = None;
        Ok(identity)
    }

    pub(super) fn pump_render(&mut self) -> bool {
        #[cfg(test)]
        if self.shared.render_poll_paused.load(Ordering::Acquire) {
            return false;
        }
        let (Some(render), Some(store)) = (&mut self.render, &mut self.store) else {
            return false;
        };
        match render.workflow.poll(store) {
            Ok(changed) => {
                if changed {
                    self.capture_render_status();
                }
                changed
            }
            Err(error) => {
                let changed = self.render_service_error(workflow_error(error));
                self.capture_render_status();
                changed
            }
        }
    }

    pub(super) fn capture_render_status(&mut self) {
        let Some(render) = &self.render else {
            return;
        };
        if render.workflow.status().identity.is_some() {
            self.render_update
                .get_or_insert_with(Default::default)
                .workflow = Some(ProjectRenderStatus {
                context: render.context.clone(),
                revision: render.revision.clone(),
                status: Arc::new(render.workflow.status().clone()),
            });
        }
        self.retain_host_render_status();
    }

    fn render_service_error(&mut self, error: ProjectRenderError) -> bool {
        let update = self.render_update.get_or_insert_with(Default::default);
        let changed = update.service_error.as_ref() != Some(&error);
        update.service_error = Some(error);
        changed
    }

    fn cancel_render_for_transition(&mut self) {
        let (Some(render), Some(store)) = (&mut self.render, &mut self.store) else {
            return;
        };
        if render.workflow.is_active()
            && !render.workflow.status().cancellation_requested
            && let Some(identity) = render.workflow.status().identity.clone()
            && let Err(error) = render.workflow.cancel(store, &identity)
        {
            self.render_service_error(workflow_error(error));
        }
        self.capture_render_status();
    }

    pub(super) fn begin_render_shutdown(&mut self) -> bool {
        if matches!(
            self.pending_session_change,
            Some(PendingSessionChange::Shutdown)
        ) {
            return false;
        }
        self.pending_session_change = Some(PendingSessionChange::Shutdown);
        self.cancel();
        self.cancel_render_for_transition();
        self.cancel_generation();
        self.cancel_tracking();
        true
    }

    fn release_inactive_render(&mut self) -> std::result::Result<(), ProjectRenderError> {
        let (Some(render), Some(store)) = (&mut self.render, &mut self.store) else {
            return Ok(());
        };
        if !render.workflow.can_release_writer() {
            return Err(workflow_error(WorkflowError::Busy));
        }
        let result = render.workflow.drain(store);
        let cleanup_confirmed = render.workflow.status().cleanup_confirmed;
        self.capture_render_status();
        if let Err(error) = result {
            let error = workflow_error(error);
            self.render_service_error(error.clone());
            if !cleanup_confirmed {
                return Err(error);
            }
        }
        self.render = None;
        Ok(())
    }

    pub(super) fn finish_session_change(&mut self) -> bool {
        if self.pending_session_change.is_none() {
            return false;
        }
        // The AI and tracking jobs must be reaped and their outcomes recorded first.
        if self.generation.active() || self.targets.active() {
            return false;
        }
        if self
            .render
            .as_ref()
            .is_some_and(|render| !render.workflow.can_release_writer())
        {
            return false;
        }
        if let Err(error) = self.release_inactive_render() {
            return self.render_service_error(error);
        }
        let pending = self
            .pending_session_change
            .take()
            .expect("pending change checked");
        if matches!(
            pending,
            PendingSessionChange::Close | PendingSessionChange::Shutdown
        ) {
            self.backup_before_session_change();
        }
        let outcome = match pending {
            PendingSessionChange::Close => {
                self.cancel();
                self.host = None;
                self.store = None;
                self.workspace = None;
                self.cached = None;
                self.clear_copied_slice();
                self.clear_marks();
                self.import = None;
                self.message = Some("Project closed".into());
                Ok(())
            }
            PendingSessionChange::Shutdown => {
                self.cancel();
                self.host = None;
                self.store = None;
                self.cached = None;
                // Retain the admitted command's immutable completion in the
                // final mailbox; the loop releases its workspace after publish.
                Ok(())
            }
            PendingSessionChange::Open(prepared) => self.install_open(*prepared),
            PendingSessionChange::CreateFromSource(path) => self.create_from_source(path),
            #[cfg(test)]
            PendingSessionChange::Create(path) => self.open(path, true),
        };
        if let Err(error) = outcome {
            self.error = Some(error);
            self.message = None;
        }
        self.shared.busy.store(false, Ordering::Release);
        true
    }
}

fn native_config(
    package: PathBuf,
    limits: &ProjectRenderLimits,
) -> std::result::Result<WorkflowConfig, ProjectRenderError> {
    let executable = std::env::current_exe()
        .map_err(|error| native_error("RenderRuntimeUnavailable", error.to_string()))?;
    Ok(WorkflowConfig {
        package,
        runtime: RenderWorkerRuntime {
            executable,
            arguments: Vec::new(),
            environment: BTreeMap::new(),
        },
        encode_limits: limits.encode,
        verification_limits: limits.verification,
        media_limits: limits.media,
    })
}

fn native_error(code: &'static str, message: impl ToString) -> ProjectRenderError {
    ProjectRenderError {
        code,
        message: message.to_string(),
    }
}

fn unchanged_preview() -> ProjectRenderError {
    native_error(
        "RenderPreviewUnchanged",
        "The preview made no edit; request a render of the committed revision explicitly",
    )
}

fn workflow_error(error: WorkflowError) -> ProjectRenderError {
    let code = match &error {
        WorkflowError::Busy => "RenderBusy",
        WorkflowError::Identity => "RenderIdentityChanged",
        WorkflowError::StaleRevision => "RenderRevisionChanged",
        WorkflowError::Configuration(_) => "RenderInvalidRequest",
        WorkflowError::Unresolved(_) => "RenderRecoveryRequired",
        WorkflowError::Store(_) => "RenderStoreFailure",
        WorkflowError::Io(_) => "RenderIoFailure",
    };
    native_error(code, error)
}

impl Service {
    /// Keep the render registered with the job coordinator while it owns
    /// the render slot, so proxies and analysis yield and the Jobs panel
    /// lists it. Cancellation stays the render's own request.
    pub(super) fn reconcile_render_registration(&mut self) {
        if self.render.is_none() {
            self.render_registration = None;
            return;
        }
        let session = self.session;
        let board = &self.shared.job_board;
        let handle = self.render_registration.get_or_insert_with(|| {
            board.register(
                crate::jobs::JobSpec::new(crate::jobs::JobKind::Render, Some(session))
                    .detail("a saved edit"),
            )
        });
        let Some(status) = self
            .render_update
            .as_ref()
            .and_then(|update| update.workflow.as_ref())
        else {
            return;
        };
        use deadpan_cli::encoded_render::workflow::WorkflowProgress;
        let fraction = |done: u64, total: u64| (total > 0).then(|| done as f32 / total as f32);
        let (stage, fraction) = match &status.status.progress {
            Some(WorkflowProgress::Qualification {
                completed_frames,
                total_frames,
                ..
            }) => (
                "Qualifying the encoder",
                fraction(*completed_frames, *total_frames),
            ),
            Some(WorkflowProgress::Encoding {
                completed_frames,
                total_frames,
                ..
            }) => ("Encoding", fraction(*completed_frames, *total_frames)),
            Some(WorkflowProgress::Verification(_)) => ("Verifying the movie", None),
            Some(WorkflowProgress::Publication(_)) => ("Saving the movie", None),
            None => ("Preparing", None),
        };
        handle.set_progress(stage, fraction);
    }
}
