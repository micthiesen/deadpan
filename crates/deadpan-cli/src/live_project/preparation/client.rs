//! Each observation uses a fresh bounded connection to the same captured owner.
//! No command replay or rediscovery is permitted after admission is attempted.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::ProjectId;
use serde_json::Value;

use super::{
    PreparationCommand, PreparationReceipt, PreparationState, PreparationStatus, PreparationTarget,
};
use crate::host::Client;
use crate::live_project::{self, LiveError, Operation, Reply};
use crate::{CliError, write_json};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const PREPARATION_LIMIT: Duration = Duration::from_secs(15 * 60);
const CANCELLATION_DRAIN_LIMIT: Duration = Duration::from_secs(5 * 60);

pub(crate) fn run(package: &Path, command: PreparationCommand) -> Result<(), CliError> {
    command.validate()?;
    let signals = Signals::register()?;
    let mut client = Client::discover(package).map_err(LiveError::from)?.ok_or_else(|| {
        LiveError::new("HostOwnerUnavailable", "The project writer has no available authenticated endpoint; no preparation was sent")
    })?;
    let project_id = live_project::inspect(&mut client)?.0.project_id;
    let target = PreparationTarget::fresh();
    let status = observe(
        &mut client,
        &project_id,
        &target,
        command,
        &signals.cancelled,
    )?;
    finish(status, write_json, || {
        // Losing a best-effort archive release cannot change the completed
        // operation or its already observed terminal failure.
        let _ = live_project::request(
            &mut client,
            Operation::ReleasePreparationStatus {
                project_id: project_id.clone(),
                target: target.clone(),
            },
        );
    })
}

fn finish(
    status: PreparationStatus,
    emit: impl FnOnce(&Value) -> Result<(), CliError>,
    release: impl FnOnce(),
) -> Result<(), CliError> {
    let known_failure = matches!(
        status.state,
        PreparationState::Failed { .. } | PreparationState::Cancelled {}
    );
    let completion_error = match &status.state {
        PreparationState::Completed {
            completion_error, ..
        } => completion_error.clone(),
        _ => None,
    };
    let output = match terminal_output(status) {
        Ok(output) => output,
        Err(error) => {
            // Failed/cancelled observations contain no committed receipt. They
            // must release their archive instead of exhausting the owner cap.
            if known_failure {
                release();
            }
            return Err(error.into());
        }
    };
    // Preserve the owner's terminal receipt if the final stdout write fails.
    // No output is produced while work is active, so a blocked pipe never
    // prevents polling, cancellation or worker drain.
    emit(&output)?;
    release();
    if let Some(mut error) = completion_error {
        error.message = format!(
            "{}; the operation was published and its receipt is in stdout; inspect it before repeating",
            error.message
        );
        return Err(error.into());
    }
    Ok(())
}

fn terminal_output(status: PreparationStatus) -> Result<Value, LiveError> {
    match status.state {
        PreparationState::Completed {
            mut output,
            receipt,
            committed_revision,
            inventory_changed,
            completion_error,
            refresh_error,
        } => {
            if refresh_error.is_some()
                || completion_error.is_some()
                || output.get("host_reply_detail_omitted").is_some()
            {
                let object = output.as_object_mut().ok_or_else(|| {
                    unknown(
                        &status.target,
                        "Committed preparation output is not an object",
                    )
                })?;
                if let Some(error) = refresh_error {
                    object.insert("host_refresh_error".into(), Value::String(error));
                }
                if let Some(error) = completion_error {
                    object.insert(
                        "completion_error".into(),
                        serde_json::to_value(error).map_err(LiveError::json)?,
                    );
                }
                object.insert(
                    "preparation_receipt".into(),
                    serde_json::to_value(receipt).map_err(LiveError::json)?,
                );
                object.insert(
                    "committed_revision".into(),
                    serde_json::to_value(committed_revision).map_err(LiveError::json)?,
                );
                object.insert("inventory_changed".into(), Value::Bool(inventory_changed));
            }
            Ok(output)
        }
        PreparationState::Failed { error } => Err(error),
        PreparationState::Cancelled {} => Err(LiveError::new(
            "PreparationCancelled",
            "Preparation cancelled; the owner confirmed worker completion",
        )),
        _ => Err(unknown(
            &status.target,
            "Preparation has no terminal result",
        )),
    }
}

fn observe(
    client: &mut Client,
    project: &ProjectId,
    target: &PreparationTarget,
    command: PreparationCommand,
    cancelled: &AtomicBool,
) -> Result<PreparationStatus, LiveError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(LiveError::new(
            "PreparationCancelled",
            "Cancelled before preparation admission",
        ));
    }
    let mut status = status_reply(
        live_project::request(
            client,
            Operation::Prepare {
                project_id: project.clone(),
                target: target.clone(),
                command: Box::new(command.clone()),
            },
        )
        .map_err(|error| context_error(error, target))?,
        target,
    )?;
    validate_completion(project, &command, &status)?;
    let deadline = Instant::now() + PREPARATION_LIMIT;
    let mut cancellation_deadline = None;
    while !status.is_terminal() {
        if (cancelled.load(Ordering::Acquire) || Instant::now() >= deadline)
            && cancellation_deadline.is_none()
        {
            cancellation_deadline = Some(Instant::now() + CANCELLATION_DRAIN_LIMIT);
            let next = status_reply(
                live_project::request(
                    client,
                    Operation::CancelPreparation {
                        project_id: project.clone(),
                        target: target.clone(),
                    },
                )
                .map_err(|error| context_error(error, target))?,
                target,
            )?;
            validate_next(&status, &next)?;
            validate_completion(project, &command, &next)?;
            status = next;
            if status.is_terminal() {
                break;
            }
        }
        if cancellation_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(unknown(
                target,
                "The owner has not confirmed cancellation drain within five minutes; work may still be active and must not be replayed",
            ));
        }
        std::thread::park_timeout(POLL_INTERVAL);
        let next = status_reply(
            live_project::request(
                client,
                Operation::PreparationStatus {
                    project_id: project.clone(),
                    target: target.clone(),
                },
            )
            .map_err(|error| context_error(error, target))?,
            target,
        )?;
        validate_next(&status, &next)?;
        validate_completion(project, &command, &next)?;
        status = next;
    }
    Ok(status)
}

fn validate_completion(
    project: &ProjectId,
    command: &PreparationCommand,
    status: &PreparationStatus,
) -> Result<(), LiveError> {
    let PreparationState::Completed {
        output,
        receipt,
        committed_revision,
        inventory_changed,
        ..
    } = &status.state
    else {
        return Ok(());
    };
    let valid = match (command, receipt) {
        (
            PreparationCommand::Retain { .. },
            PreparationReceipt::Retained {
                location_version, ..
            },
        ) => *location_version > 0 && committed_revision.is_none() && *inventory_changed,
        (
            PreparationCommand::Relink {
                content,
                expected_version,
                ..
            },
            PreparationReceipt::Relinked {
                content: retained,
                location_version,
            },
        ) => {
            content == retained
                && (*expected_version == *location_version
                    || expected_version.checked_add(1) == Some(*location_version))
                && committed_revision.is_none()
                && *inventory_changed
        }
        (
            PreparationCommand::Register { registration, .. },
            PreparationReceipt::Registered {
                asset,
                qualification,
            },
        ) => {
            // Registration may reuse a qualified asset with another identity:
            // new_asset_id is an allocation proposal, not the resolved result.
            // When detail is present, it must agree with the retained receipt.
            let detail_matches = output.get("host_reply_detail_omitted")
                == Some(&Value::Bool(true))
                || (output.pointer("/outcome/asset_id").and_then(Value::as_str)
                    == Some(asset.as_str())
                    && output
                        .pointer("/outcome/qualification")
                        .and_then(Value::as_str)
                        == Some(qualification.as_str()));
            committed_revision
                .as_ref()
                .is_none_or(|revision| revision == &registration.new_revision)
                && !inventory_changed
                && detail_matches
        }
        (PreparationCommand::Checkpoint {}, PreparationReceipt::Checkpoint { project_id, .. }) => {
            project == project_id && committed_revision.is_none() && !inventory_changed
        }
        _ => false,
    };
    if !valid || !output.is_object() || output.get("protocol") != Some(&Value::from(1)) {
        return Err(unknown(
            &status.target,
            "The owner returned an inconsistent preparation receipt",
        ));
    }
    Ok(())
}
fn status_reply(reply: Reply, target: &PreparationTarget) -> Result<PreparationStatus, LiveError> {
    match reply {
        Reply::Preparation { status } if &status.target == target => Ok(*status),
        _ => Err(unknown(
            target,
            "The owner returned an unexpected preparation reply or target",
        )),
    }
}
fn validate_next(previous: &PreparationStatus, next: &PreparationStatus) -> Result<(), LiveError> {
    if next.target != previous.target
        || matches!(
            (&previous.state, &next.state),
            (
                PreparationState::AwaitingCommit {},
                PreparationState::Preparing {}
            ) | (
                PreparationState::Cancelling {},
                PreparationState::Preparing {} | PreparationState::AwaitingCommit {}
            )
        )
    {
        return Err(unknown(
            &previous.target,
            "The owner changed the preparation identity or moved its state backwards",
        ));
    }
    Ok(())
}
fn context_error(mut error: LiveError, target: &PreparationTarget) -> LiveError {
    error.message = format!(
        "{} (preparation {}; cancellation token {}; no replay was attempted)",
        error.message, target.operation_id, target.cancellation_token
    );
    error
}
fn unknown(target: &PreparationTarget, message: &str) -> LiveError {
    context_error(
        LiveError::new("HostPreparationOutcomeUnknown", message),
        target,
    )
}

struct Signals {
    cancelled: Arc<AtomicBool>,
    handlers: Vec<signal_hook::SigId>,
}
impl Signals {
    fn register() -> Result<Self, LiveError> {
        let mut result = Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            handlers: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            result.handlers.push(
                signal_hook::flag::register(signal, result.cancelled.clone())
                    .map_err(|error| LiveError::new("IoFailure", error))?,
            );
        }
        Ok(result)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            signal_hook::low_level::unregister(handler);
        }
    }
}

#[cfg(test)]
mod tests;
