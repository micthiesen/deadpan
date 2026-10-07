//! AI work queued atomically by pause insertion or accepted duration edits.
//!
//! Reading prior controls and conditioning are worker work. The writer claims
//! the durable preparation and admits its result against that exact claim.

use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::GeneratedObjectRef;
use deadpan_jobs::GenerationOptions;
use deadpan_models::StoredBridgeProvenance;
use deadpan_store::generated_media::{GeneratedReadHandle, GeneratedReadLimits};
use deadpan_store::generation_preparations::{
    PreparationControls, PreparationOrigin, StoredGenerationPreparation,
};
use deadpan_store::{AccessMode, ProjectStore};

use super::attempt::GenerationError;

mod command;
pub(crate) use command::run;

/// Recover the exact captured controls without consulting the current selection
/// or an installed model. A copied accepted Hold may have no request row, so its
/// verified retained provenance is the other authoritative controls source.
/// This reads metadata only and does not admit the old media for playback.
pub fn resolve_options(
    package: &Path,
    preparation: &StoredGenerationPreparation,
    cancelled: &AtomicBool,
) -> Result<GenerationOptions, GenerationError> {
    check_cancel(cancelled)?;
    match &preparation.origin {
        PreparationOrigin::InsertedPause { options }
        | PreparationOrigin::AcceptedExtension {
            controls: PreparationControls::Request { options, .. },
            ..
        } => Ok(options.clone()),
        PreparationOrigin::AcceptedExtension {
            accepted: artifact,
            controls: PreparationControls::AcceptedArtifact,
        } => {
            let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
            if store.snapshot_shared()?.project_id() != &preparation.project_id {
                return Err(invalid(
                    "Replacement preparation belongs to another project.",
                ));
            }
            let handle = store.generated_read_handle();
            let deadline = Instant::now() + Duration::from_secs(60);
            let bytes = metadata(
                &handle,
                &artifact.provenance,
                32 * 1024 * 1024,
                deadline,
                cancelled,
            )?;
            let stored = StoredBridgeProvenance::from_bytes(&bytes, &artifact.provenance)
                .map_err(invalid)?;
            let context = metadata(
                &handle,
                stored.context_object(),
                1024 * 1024,
                deadline,
                cancelled,
            )?;
            let evidence = stored
                .validate_for(artifact, &preparation.project_id, &context)
                .map_err(invalid)?;
            check_cancel(cancelled)?;
            Ok(evidence.generation_options())
        }
    }
}

fn metadata(
    handle: &GeneratedReadHandle,
    object: &GeneratedObjectRef,
    maximum: u64,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, GenerationError> {
    check_cancel(cancelled)?;
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| invalid("Reading accepted AI controls timed out."))?;
    let limits = GeneratedReadLimits::new(maximum, remaining).map_err(invalid)?;
    let snapshot = handle
        .snapshot(object, limits, cancelled)
        .map_err(invalid)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(usize::try_from(object.byte_length()).map_err(invalid)?)
        .map_err(invalid)?;
    snapshot
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(invalid)?;
    check_cancel(cancelled)?;
    if u64::try_from(bytes.len()).ok() != Some(object.byte_length()) || Instant::now() >= deadline {
        return Err(invalid(
            "Accepted AI controls changed length or exceeded the read deadline.",
        ));
    }
    Ok(bytes)
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), GenerationError> {
    if cancelled.load(Ordering::Acquire) {
        Err(GenerationError::Cancelled)
    } else {
        Ok(())
    }
}

fn invalid(error: impl std::fmt::Display) -> GenerationError {
    GenerationError::Inputs(error.to_string())
}
