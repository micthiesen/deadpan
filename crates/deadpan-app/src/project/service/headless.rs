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

pub(super) struct Host {
    endpoint: Endpoint,
    renders: Vec<Observation>,
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
        })
    }

    fn expire(&mut self) {
        let now = Instant::now();
        self.renders
            .retain(|entry| entry.expires.is_none_or(|end| now < end));
    }
}

impl Service {
    pub(super) fn pump_host(&mut self) {
        let Some(host) = &mut self.host else { return };
        host.expire();
        let requests = host.endpoint.poll();
        for incoming in requests {
            let reply = Request::from_value(incoming.payload)
                .and_then(|request| self.host_request(request.operation))
                .unwrap_or_else(|error| HostReply::Failed { error });
            let fallback = reply_failure(&reply);
            if let Some(host) = &mut self.host {
                let sent = serde_json::to_value(reply)
                    .ok()
                    .is_some_and(|value| host.endpoint.respond(incoming.ticket, value).is_ok());
                if !sent && let Ok(value) = serde_json::to_value(fallback) {
                    // Socket loss can still make the outcome unknowable. A
                    // locally rejected large reply must retain its receipt.
                    let _ = host.endpoint.respond(incoming.ticket, value);
                }
            }
            // A peer cannot consume an unbounded stream of admissions before
            // a render gets another chance to advance or observe cancellation.
            if self.pump_render() {
                self.publish();
            }
        }
    }

    fn host_request(&mut self, operation: Operation) -> std::result::Result<HostReply, LiveError> {
        let store = self.store.as_ref().ok_or_else(owner_changed)?;
        let host = self.host.as_ref().ok_or_else(owner_changed)?;
        store
            .check_writer_owner(host.endpoint.owner_handle())
            .map_err(LiveError::store)?;
        if self.shared.stopping.load(Ordering::Acquire) || self.pending_session_change.is_some() {
            return Err(owner_changed());
        }
        match operation {
            Operation::Inspect => Ok(HostReply::Context {
                context: RenderContext::from_document(&store.snapshot().map_err(LiveError::store)?),
                preview_active: self.shared.preview_active.load(Ordering::Acquire),
            }),
            Operation::RenderStatus { project_id, target } => {
                self.check_host_project(&project_id)?;
                self.host_render_status(&target)
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

    fn check_host_project(&self, project: &ProjectId) -> std::result::Result<(), LiveError> {
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
                .is_some_and(|update| update.committed.is_some())
            {
                return Err(LiveError::new(
                    "HostBusy",
                    "The native app has an unread edit receipt",
                ));
            }
        }
        let store = self.store.as_mut().ok_or_else(owner_changed)?;
        let (output, committed_revision) = execute_short(store, project, command)?;
        let mut refresh_error = None;
        if let Some(revision) = &committed_revision {
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
            refresh_error = if inject_failure {
                Some("Injected failure refreshing the committed headless edit".into())
            } else {
                self.refresh().err()
            };
            self.error = refresh_error.clone();
            self.message = Some(format!("Headless edit saved at revision {revision}"));
            self.publish();
        }
        // A failed workspace refresh cannot convert a successful transaction
        // into a retryable error or erase the acknowledged durable revision.
        Ok(HostReply::Completed {
            output,
            committed_revision,
            refresh_error,
        })
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
                public_render::output_summary(&document).map_err(public_error)?;
                ProjectRenderOperation::Start {
                    request: public_render::start_request(
                        &request.context,
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

fn owner_changed() -> LiveError {
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
    }
}

fn reply_failure(reply: &HostReply) -> HostReply {
    if let HostReply::Failed { error } = reply {
        let mut error = error.clone();
        error.message =
            "The operation failed; its full diagnostic exceeded transport capacity".into();
        if error.code.len() > 128 {
            error.code = "HostReplyLimit".into();
        }
        return HostReply::Failed { error };
    }
    let committed_revision = match reply {
        HostReply::Completed {
            committed_revision, ..
        } => committed_revision.clone(),
        _ => None,
    };
    HostReply::Failed {
        error: LiveError {
            committed_revision,
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
}
