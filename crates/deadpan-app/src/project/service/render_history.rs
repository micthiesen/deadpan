//! Owner-thread history queries and public recovery request construction.

use std::time::Instant;

use deadpan_cli::render as public_render;
use deadpan_jobs::render::{RenderIntent, publication::StoredPublication};
use deadpan_store::render_jobs::StoredRenderAttempt;

use super::*;
use crate::project::{
    ProjectRenderContext, ProjectRenderError, ProjectRenderLimits, ProjectRenderOperation,
    render_history::{
        AttemptSummary, JobSummary, PAGE_SIZE, Page, PublicationSummary, Query, Recovery, Request,
        Update,
    },
};

impl Service {
    pub(super) fn render_history_command(&mut self, request: Request) {
        let result = self.read_render_history(&request);
        self.render_history = Some(Update {
            ticket: request.ticket,
            context: request.context,
            query: request.query,
            result,
        });
    }

    fn read_render_history(
        &self,
        request: &Request,
    ) -> std::result::Result<Page, ProjectRenderError> {
        if request.ticket == 0 {
            return Err(error(
                "RenderHistoryInvalidRequest",
                "Render history request ticket must be nonzero",
            ));
        }
        let workspace = self.workspace.as_ref().ok_or_else(context_changed)?;
        if workspace.session != request.context.session
            || workspace.document.project_id() != &request.context.project
            || self.pending_session_change.is_some()
        {
            return Err(context_changed());
        }
        let store = self.store.as_ref().ok_or_else(context_changed)?;
        read_page(store, &request.query).map_err(|failure| {
            error(
                "RenderHistoryQueryFailed",
                crate::recovery::describe_store_error(&failure),
            )
        })
    }

    /// Called only after admit_render has checked the captured session, ticket
    /// and available execution slot. Store-dependent builders never run on UI.
    pub(super) fn prepare_render_recovery(
        &self,
        context: &ProjectRenderContext,
        recovery: &Recovery,
    ) -> std::result::Result<ProjectRenderOperation, ProjectRenderError> {
        let store = self.store.as_ref().ok_or_else(context_changed)?;
        let workspace = self.workspace.as_ref().ok_or_else(context_changed)?;
        let context = public_render::RenderContext {
            project_id: context.project.clone(),
            // Recovery builders intentionally check project identity only;
            // their immutable historical job supplies the render revision.
            revision_id: workspace.document.revision_id().clone(),
        };
        let limits = public_render::default_limits().map_err(public_error)?;
        let limits = ProjectRenderLimits {
            encode: limits.encode,
            verification: limits.verification,
            media: limits.media,
        };
        match recovery {
            Recovery::Retry {
                job_id,
                checkpoint_attempt_id,
                destination,
            } => Ok(ProjectRenderOperation::Retry {
                request: public_render::retry_request(
                    store,
                    &context,
                    job_id,
                    checkpoint_attempt_id.as_ref(),
                    destination.clone(),
                    Instant::now(),
                )
                .map_err(public_error)?,
                limits,
            }),
            Recovery::Reconcile { publication_id } => Ok(ProjectRenderOperation::Reconcile {
                request: public_render::reconcile_request(
                    store,
                    &context,
                    publication_id,
                    Instant::now(),
                )
                .map_err(public_error)?,
                limits,
            }),
        }
    }
}

fn read_page(store: &ProjectStore, query: &Query) -> std::result::Result<Page, StoreError> {
    match query {
        Query::Jobs { after } => {
            let mut jobs = store.render_jobs(after.as_ref(), PAGE_SIZE + 1)?;
            let more = jobs.len() > PAGE_SIZE as usize;
            jobs.truncate(PAGE_SIZE as usize);
            let next_after = more.then(|| jobs.last().expect("full page").job_id.clone());
            Ok(Page::Jobs {
                items: jobs.into_iter().map(job_summary).collect(),
                next_after,
            })
        }
        Query::Attempts { job, after_ordinal } => {
            let intent = store.render_job(job)?;
            let mut attempts = store.render_attempts(job, *after_ordinal, PAGE_SIZE + 1)?;
            let more = attempts.len() > PAGE_SIZE as usize;
            attempts.truncate(PAGE_SIZE as usize);
            let next_after_ordinal = more.then(|| attempts.last().expect("full page").ordinal);
            Ok(Page::Attempts {
                job: job_summary(intent),
                items: attempts.into_iter().map(attempt_summary).collect(),
                next_after_ordinal,
            })
        }
        Query::Publications { after } => {
            let mut publications = store.render_publications(after.as_ref(), PAGE_SIZE + 1)?;
            let more = publications.len() > PAGE_SIZE as usize;
            publications.truncate(PAGE_SIZE as usize);
            let next_after = more.then(|| {
                publications
                    .last()
                    .expect("full page")
                    .intent
                    .publication_id
                    .clone()
            });
            Ok(Page::Publications {
                items: publications.into_iter().map(publication_summary).collect(),
                next_after,
            })
        }
    }
}

fn job_summary(intent: RenderIntent) -> JobSummary {
    JobSummary {
        job_id: intent.job_id,
        revision_id: intent.revision_id,
        range: intent.range,
        automatic: intent.policy.is_automatic(),
    }
}

fn attempt_summary(attempt: StoredRenderAttempt) -> AttemptSummary {
    AttemptSummary {
        attempt_id: attempt.attempt_id,
        ordinal: attempt.ordinal,
        state: attempt.state,
        checkpoint_attempt_id: attempt.checkpoint_attempt_id,
        cancellation_requested: attempt.cancellation_requested,
        diagnostic: attempt.diagnostic,
        verification_recorded: attempt.verification.is_some(),
    }
}

fn publication_summary(record: StoredPublication) -> PublicationSummary {
    PublicationSummary {
        publication_id: record.intent.publication_id,
        job_id: record.intent.job_id,
        revision_id: record.render_intent.revision_id,
        destination: record.intent.destination,
        automatic: record.render_intent.policy.is_automatic(),
        encoding_attempt_id: record.encoding_attempt_id,
        phase: record.phase,
        outcome: record.outcome,
        operation_active: record.operation.active,
        observed_movie_commit: record.observed_movie_commit,
        diagnostic: record.operation.diagnostic,
    }
}

fn error(code: &'static str, message: impl ToString) -> ProjectRenderError {
    ProjectRenderError {
        code,
        message: message.to_string(),
    }
}

fn context_changed() -> ProjectRenderError {
    error(
        "RenderContextChanged",
        "The project session is closed or changed before the request",
    )
}

fn public_error(failure: public_render::PublicRenderError) -> ProjectRenderError {
    let code = match failure.code.as_str() {
        "RenderEngineeringJob" => "RenderEngineeringJob",
        "RenderProjectChanged" => "RenderProjectChanged",
        "RenderInvalidLimits" => "RenderInvalidLimits",
        "RenderInvalidRequest" => "RenderInvalidRequest",
        _ => "RenderRecoveryRequestFailed",
    };
    error(code, display(failure))
}
