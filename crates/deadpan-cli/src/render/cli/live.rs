//! Observe one exact render through the authenticated project writer.
//! A lost reply never authorizes replay or a local writer fallback.

use super::*;
use crate::{
    host::Client,
    live_project::{self, LiveError, Operation, Reply},
};

const OBSERVATION_INTERVAL: Duration = Duration::from_millis(50);

struct Observation {
    status: RenderStatus,
    finished: bool,
}

pub(super) fn run(
    package: &Path,
    invocation: Invocation,
    parsed: Option<RenderRequest>,
    cancelled: &AtomicBool,
    output: &mut JsonOutput,
    error_output: &mut Option<JsonOutput>,
) -> Result<(), PublicRenderError> {
    let mut client = Client::discover(package)
        .map_err(|error| PublicRenderError::new(error.code, error.message))?
        .ok_or_else(|| {
            PublicRenderError::new(
                "RenderOwnerUnavailable",
                "The project has no available authenticated render owner; no request was sent",
            )
        })?;
    let is_cancel = parsed
        .as_ref()
        .is_some_and(|request| matches!(request.operation, RenderOperation::Cancel { .. }));
    let request = if is_cancel {
        parsed.expect("cancel request checked")
    } else {
        let (context, preview_active) = live_project::inspect(&mut client).map_err(remote_error)?;
        let request = invocation_request(invocation, parsed, context.clone())?;
        if request.context.project_id != context.project_id {
            return Err(PublicRenderError::new(
                "HostProjectChanged",
                "The request names a different project",
            ));
        }
        if preview_active && matches!(request.operation, RenderOperation::Start { .. }) {
            return Err(PublicRenderError::new(
                "RenderPreviewDecisionRequired",
                "Commit or discard the temporary preview in the native app before rendering",
            ));
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(PublicRenderError::new(
                "Cancelled",
                "Cancelled before render admission",
            ));
        }
        request
    };
    let observation = render_reply(
        live_project::request(
            &mut client,
            Operation::Render {
                request: request.clone(),
            },
        )
        .map_err(remote_error)?,
    )?;
    validate_initial(&request, &observation)?;
    let failure = output
        .send(&RenderEvent::Admitted {
            schema_version: SCHEMA_VERSION,
            request_id: request.request_id.clone(),
            status: Box::new(observation.status.clone()),
        })
        .err();
    observe(
        &mut client,
        &request,
        observation,
        cancelled,
        output,
        error_output,
        failure,
    )
}

fn render_reply(reply: Reply) -> Result<Observation, PublicRenderError> {
    match reply {
        Reply::Render { status, finished } => {
            if status.target.is_none()
                || status.stage == WorkflowStage::Idle
                || (finished
                    && (!status.cleanup_confirmed
                        || status.outcome.is_none()
                        || !matches!(
                            status.stage,
                            WorkflowStage::Finished | WorkflowStage::Unresolved
                        )))
            {
                return Err(unknown(
                    "The owner returned an inconsistent render observation",
                ));
            }
            Ok(Observation {
                status: *status,
                finished,
            })
        }
        _ => Err(unknown("The owner returned an unexpected render reply")),
    }
}

fn validate_initial(
    request: &RenderRequest,
    observation: &Observation,
) -> Result<(), PublicRenderError> {
    let status = &observation.status;
    let target = status.target.as_ref().expect("render reply checked target");
    if status.context.project_id != request.context.project_id {
        return Err(unknown(
            "The owner returned another project's render status",
        ));
    }
    match &request.operation {
        RenderOperation::Cancel { target: expected } if target != expected => {
            return Err(unknown("The cancellation reply names another workflow"));
        }
        RenderOperation::RetryCheckpoint { job_id, .. }
        | RenderOperation::Reencode { job_id, .. }
            if &target.job_id != job_id =>
        {
            return Err(unknown("The retry reply names another render job"));
        }
        RenderOperation::Start { .. }
            if status.context != request.context
                || status.captured_revision != request.context.revision_id =>
        {
            return Err(unknown(
                "The admitted render changed its requested revision",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_next(previous: &RenderStatus, next: &RenderStatus) -> Result<(), PublicRenderError> {
    if next.context != previous.context
        || next.target != previous.target
        || next.captured_revision != previous.captured_revision
        || (previous.observed_movie_commit && !next.observed_movie_commit)
        || (previous.cancellation_requested && !next.cancellation_requested)
    {
        return Err(unknown(
            "The owner changed the observed workflow identity or lost a recorded outcome",
        ));
    }
    Ok(())
}

fn observe(
    client: &mut Client,
    request: &RenderRequest,
    mut observation: Observation,
    cancelled: &AtomicBool,
    output: &mut JsonOutput,
    error_output: &mut Option<JsonOutput>,
    mut failure: Option<PublicRenderError>,
) -> Result<(), PublicRenderError> {
    let target = observation
        .status
        .target
        .clone()
        .expect("render reply checked target");
    let project_id = request.context.project_id.clone();
    let mut cancellation_sent = matches!(request.operation, RenderOperation::Cancel { .. });
    let mut last_stage = observation.status.stage;
    let mut last_progress = Instant::now();
    let mut recovery_emitted = false;
    while !observation.finished {
        if (cancelled.load(Ordering::Acquire) || failure.is_some())
            && !cancellation_sent
            && !observation.status.cancellation_requested
        {
            cancellation_sent = true;
            let cancellation = RenderRequest {
                schema_version: SCHEMA_VERSION,
                request_id: fresh_job()?,
                context: observation.status.context.clone(),
                operation: RenderOperation::Cancel {
                    target: target.clone(),
                },
            };
            let next = live_project::request(
                client,
                Operation::Render {
                    request: cancellation,
                },
            )
            .map_err(remote_error)
            .and_then(render_reply);
            observation =
                accept_observation(next, &observation.status, request, output, error_output)?;
            if observation.finished {
                break;
            }
        }
        if observation.status.stage == WorkflowStage::Unresolved
            && !observation.status.cleanup_confirmed
        {
            if !recovery_emitted {
                let error = PublicRenderError::new(
                    "RenderCleanupUnconfirmed",
                    "Worker cleanup is unconfirmed; the native owner retains the writer",
                );
                if let Err(error) = send_important(
                    output,
                    error_output,
                    &RenderEvent::RecoveryRequired {
                        schema_version: SCHEMA_VERSION,
                        request_id: request.request_id.clone(),
                        status: Box::new(observation.status.clone()),
                        error,
                    },
                ) {
                    failure.get_or_insert(error);
                }
                recovery_emitted = true;
            }
        } else if observation.status.stage != last_stage
            || last_progress.elapsed() >= PROGRESS_INTERVAL
        {
            if let Err(error) = output.send(&RenderEvent::Progress {
                schema_version: SCHEMA_VERSION,
                request_id: request.request_id.clone(),
                status: Box::new(observation.status.clone()),
            }) {
                failure.get_or_insert(error);
            }
            last_stage = observation.status.stage;
            last_progress = Instant::now();
        }
        thread::park_timeout(OBSERVATION_INTERVAL);
        let next = live_project::request(
            client,
            Operation::RenderStatus {
                project_id: project_id.clone(),
                target: target.clone(),
            },
        )
        .map_err(remote_error)
        .and_then(render_reply);
        observation = accept_observation(next, &observation.status, request, output, error_output)?;
    }
    // The explicit owner flag includes worker Release acknowledgement. Neither
    // a Finished stage nor cleanup_confirmed alone establishes this boundary.
    if let Err(error) = send_important(
        output,
        error_output,
        &RenderEvent::Finished {
            schema_version: SCHEMA_VERSION,
            request_id: request.request_id.clone(),
            status: Box::new(observation.status.clone()),
        },
    ) {
        failure.get_or_insert(error);
    }
    // An independent cancel command is a second observer. Releasing here could
    // erase the initiating client's terminal status before it has polled it.
    if !matches!(request.operation, RenderOperation::Cancel { .. }) {
        match live_project::request(
            client,
            Operation::ReleaseRenderStatus { project_id, target },
        )
        .map_err(remote_error)
        {
            Ok(Reply::Released) => {}
            Ok(_) => {
                failure.get_or_insert_with(|| {
                    unknown("The owner returned an unexpected status release reply")
                });
            }
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    finish_result(&observation.status)
}

fn accept_observation(
    next: Result<Observation, PublicRenderError>,
    previous: &RenderStatus,
    request: &RenderRequest,
    output: &mut JsonOutput,
    error_output: &mut Option<JsonOutput>,
) -> Result<Observation, PublicRenderError> {
    let result = next.and_then(|next| {
        validate_next(previous, &next.status)?;
        Ok(next)
    });
    if let Err(error) = &result {
        // This is explicitly the last observed status, not a terminal inference
        // from a lost connection. The native owner may still be working.
        let _ = send_important(
            output,
            error_output,
            &RenderEvent::RecoveryRequired {
                schema_version: SCHEMA_VERSION,
                request_id: request.request_id.clone(),
                status: Box::new(previous.clone()),
                error: error.clone(),
            },
        );
    }
    result
}

fn remote_error(error: LiveError) -> PublicRenderError {
    let mut result = PublicRenderError::new(error.code, error.message);
    result.current_revision = error.current_revision;
    result
}

fn unknown(message: &str) -> PublicRenderError {
    PublicRenderError::new(
        "HostOutcomeUnknown",
        format!("{message}; no request was replayed. Inspect the native owner before retrying"),
    )
}

#[cfg(test)]
mod tests;
