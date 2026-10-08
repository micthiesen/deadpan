//! Bounded status/actions and an owned, synchronous drain for a closed project.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_core::RevisionId;
use deadpan_jobs::JobState;
use deadpan_store::ProjectStore;
use deadpan_store::generation_preparations::{
    PreparationFailure, PreparationId, PreparationState, StoredGenerationPreparation,
};

use crate::CliError;
use crate::generation::attempt::{self, GenerationError};
use crate::generation::runtime::BridgeRuntime;
use crate::live_project::{self, ShortOperation};

fn usage() -> CliError {
    CliError::Usage("usage: ai-replacements <project.deadpan> [--after <id> | --id <id> | --retry <id> --expected <revision> | --discard <id> --sequence <n> | --run [--limit 1-16]]".into())
}

fn id(value: &str) -> Result<PreparationId, CliError> {
    PreparationId::new(value.to_owned()).map_err(|error| CliError::Usage(error.to_string()))
}

pub(crate) fn run(arguments: &[&str]) -> Result<(), CliError> {
    let (path, operation) = match arguments {
        [path] => (
            *path,
            ShortOperation::GenerationPreparations { after: None },
        ),
        [path, "--after", after] => (
            *path,
            ShortOperation::GenerationPreparations {
                after: Some(id(after)?),
            },
        ),
        [path, "--id", preparation] => (
            *path,
            ShortOperation::GenerationPreparation {
                id: id(preparation)?,
            },
        ),
        [path, "--retry", preparation, "--expected", revision] => (
            *path,
            ShortOperation::RetryGenerationPreparation {
                id: id(preparation)?,
                expected_revision: RevisionId::new(*revision)?,
            },
        ),
        [path, "--discard", preparation, "--sequence", sequence] => (
            *path,
            ShortOperation::CancelGenerationPreparation {
                id: id(preparation)?,
                expected_sequence: sequence.parse().map_err(|_| usage())?,
            },
        ),
        [path, "--run"] => return drain(Path::new(path), 1),
        [path, "--run", "--limit", limit] => {
            let limit: usize = limit.parse().map_err(|_| usage())?;
            if !(1..=16).contains(&limit) {
                return Err(usage());
            }
            return drain(Path::new(path), limit);
        }
        _ => return Err(usage()),
    };
    crate::write_json(&live_project::dispatch_short(
        Path::new(path),
        None,
        operation,
    )?)
}

fn next_queued(
    store: &ProjectStore,
) -> Result<Option<StoredGenerationPreparation>, GenerationError> {
    let mut after = None;
    loop {
        let page = store.generation_preparations(after.as_ref(), 64)?;
        if let Some(preparation) = page
            .iter()
            .find(|entry| entry.state == PreparationState::Queued)
        {
            return Ok(Some(preparation.clone()));
        }
        if page.len() < 64 {
            return Ok(None);
        }
        after = page.last().map(|entry| entry.id.clone());
    }
}

fn drain(path: &Path, limit: usize) -> Result<(), CliError> {
    let Some(mut store) = crate::generation::command::writer(path)? else {
        return Err(GenerationError::Invalid(
            "The open app owns this queue and processes queued replacements automatically. Use ai-replacements without --run to inspect it.".into()).into());
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register_conditional_shutdown(signal, 130, Arc::clone(&cancelled))?;
        signal_hook::flag::register(signal, Arc::clone(&cancelled))?;
    }
    let mut reports = Vec::new();
    let mut failure = None;
    for _ in 0..limit {
        if cancelled.load(Ordering::Acquire) {
            failure = Some(GenerationError::Cancelled);
            break;
        }
        let Some(preparation) = next_queued(&store)? else {
            break;
        };
        let head = store.head_revision()?;
        let claim = store.claim_generation_preparation(&preparation.id, &head)?;
        let prepared = (|| {
            let mut options = super::resolve_options(path, &claim.preparation, &cancelled)?;
            if let Some(capture) = claim.preparation.intent.capture {
                options
                    .validate_resolved_conditioning(capture.conditioning())
                    .map_err(|error| GenerationError::Inputs(error.to_string()))?;
                options.mode = capture.conditioning().into();
            }
            let resolution = crate::generation::prepare::resolve_at(
                path,
                &claim.preparation.current_revision,
                &claim.preparation.target,
                &options,
                &cancelled,
            )
            .map_err(GenerationError::Inputs)?;
            let runtime = BridgeRuntime::from_environment_for(resolution.operation)?;
            let inputs = crate::generation::prepare::with_runtime(
                path,
                &claim.preparation.current_revision,
                &claim.preparation.target,
                &options,
                &runtime,
                &cancelled,
            )
            .map_err(GenerationError::Inputs)?;
            if cancelled.load(Ordering::Acquire) {
                return Err(GenerationError::Cancelled);
            }
            let random = uuid::Uuid::new_v4();
            let mut bytes = [0; 4];
            bytes.copy_from_slice(&random.as_bytes()[..4]);
            let allocated = attempt::allocate_preparation_with_provider(
                &mut store,
                &claim,
                inputs,
                runtime.provider(u64::from(u32::from_le_bytes(bytes))),
            )?;
            Ok::<_, GenerationError>((runtime, allocated))
        })();
        let (runtime, allocated) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let reason: String = error.to_string().chars().take(512).collect();
                let state = if cancelled.load(Ordering::Acquire) {
                    PreparationFailure::Cancelled(reason.clone())
                } else {
                    PreparationFailure::Unavailable(reason.clone())
                };
                store.finish_generation_preparation(&claim, state)?;
                reports.push(serde_json::json!({"preparation":store.generation_preparation(&preparation.id)?,"error":reason}));
                failure = Some(error);
                break;
            }
        };
        let (attempt, finished) = crate::generation::command::run_one(
            &mut store, path, &allocated, &runtime, &cancelled,
        )?;
        reports.push(serde_json::json!({"preparation":store.generation_preparation(&preparation.id)?,"attempt":attempt}));
        if finished.state != JobState::Ready {
            failure = Some(if finished.state == JobState::Cancelled {
                GenerationError::Cancelled
            } else {
                GenerationError::Failed(format!(
                    "Replacement attempt ended in {:?}.",
                    finished.state
                ))
            });
            break;
        }
    }
    crate::write_json(&serde_json::json!({"protocol":1,"replacements":reports,"limit":limit}))?;
    match failure {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}
