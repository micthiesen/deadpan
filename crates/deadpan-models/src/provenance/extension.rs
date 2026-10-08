//! Strict claims for the current extension adapter envelope. Diagnostic fields
//! remain in the original retained bytes and grant no independent authority.

use deadpan_core::{ExactRatio, ExtensionDirection};
use deadpan_jobs::{
    ExtensionGenerationPlan, NativeCandidateManifest, ProviderPackId, ProviderPackVersion,
    RuntimeId, RuntimeVersion, Sha256,
};
use serde::Deserialize;
use serde_json::Value;

use super::{WorkerClaims, validate_claims};
use crate::{ExtensionContext, ExtensionGenerationBinding, QualificationError};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Extension,
}

#[derive(Deserialize)]
struct WorkerProvenance {
    schema_version: u32,
    operation: Operation,
    direction: ExtensionDirection,
    request_binding: ExtensionGenerationBinding,
    context: ExtensionContext,
    generated_interval: [i64; 2],
    timing: Timing,
    pack_id: ProviderPackId,
    pack_version: ProviderPackVersion,
    runtime_id: RuntimeId,
    runtime_version: RuntimeVersion,
    model_manifest_sha256: Sha256,
    seed: u64,
    native_sha256: Sha256,
    native_bytes: u64,
    #[serde(flatten)]
    claims: WorkerClaims,
}

/// Exact fractions emitted by worker_media.extension_timing. Context duration
/// counts pictures, while anchor span counts the intervals between centers.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Timing {
    requested_duration: ExactRatio,
    generated_duration: ExactRatio,
    native_movie_duration: ExactRatio,
    context_duration: ExactRatio,
    context_anchor_span: ExactRatio,
    speed_conversion: ExactRatio,
    retime_deviation: ExactRatio,
}

impl Timing {
    fn for_plan(plan: &ExtensionGenerationPlan) -> Self {
        Self {
            requested_duration: plan.requested_duration(),
            generated_duration: plan.generated_duration(),
            native_movie_duration: plan.native_movie_duration(),
            context_duration: plan.context_duration(),
            context_anchor_span: plan.context_anchor_span(),
            speed_conversion: plan.speed(),
            retime_deviation: plan.retime_deviation(),
        }
    }
}

pub(crate) fn validate(
    value: Value,
    binding: &ExtensionGenerationBinding,
    declaration: &NativeCandidateManifest,
    conditioning: &ExtensionContext,
) -> Result<(), QualificationError> {
    binding.validate()?;
    declaration.validate().map_err(invalid)?;
    conditioning.validate_definition_binding(&binding.project_id, &binding.revision_id)?;
    let report: WorkerProvenance = serde_json::from_value(value)?;
    let Operation::Extension = report.operation;
    if report.schema_version != 3 || &report.request_binding != binding {
        return Err(invalid(
            "worker extension binding/schema differs from persisted intent",
        ));
    }
    let provider = &binding.provider;
    if report.direction != binding.plan.direction()
        || &report.context != conditioning
        || conditioning.plan() != &binding.plan
        || conditioning.region().target_id() != binding.constraints.region_target.as_ref()
        || report.seed != provider.seed
        || report.pack_id != provider.pack_id
        || report.pack_version != provider.pack_version
        || report.runtime_id != provider.runtime_id
        || report.runtime_version != provider.runtime_version
        || &declaration.provider != provider
        || &report.native_sha256 != declaration.native.sha256()
        || report.native_bytes != declaration.native.byte_length()
    {
        return Err(invalid(
            "worker extension claims contradict the retained request, context or native declaration",
        ));
    }
    let interval = binding.plan.sampling_map().generated_interval();
    if report.generated_interval != [interval.start, interval.end]
        || report.timing != Timing::for_plan(&binding.plan)
    {
        return Err(invalid(
            "worker extension generated interval or exact timing differs from the plan",
        ));
    }
    let dimensions = binding.plan.native_dimensions();
    if declaration.video.frames().frames() != i64::from(binding.plan.native_frame_count())
        || declaration.video.frame_rate() != binding.plan.native_frame_rate()
        || declaration.video.width() != dimensions.width()
        || declaration.video.height() != dimensions.height()
    {
        return Err(invalid(
            "worker extension native video declaration differs from the plan",
        ));
    }
    // A typed digest is a required bounded loading claim. This does not bind
    // it to an installed pack or independently attest loaded model bytes.
    let _ = report.model_manifest_sha256;
    validate_claims(&report.claims)
}

fn invalid(reason: impl std::fmt::Display) -> QualificationError {
    QualificationError::Provenance(format!("extension: {reason}"))
}

#[cfg(test)]
mod tests;
