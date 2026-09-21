use std::collections::{BTreeMap, BTreeSet};

use deadpan_jobs::{NativeCandidateManifest, Sha256, WorkspaceRef};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{GenerationBinding, QualificationError};

// Required provenance fields are typed. Additional backend diagnostics are
// preserved in the original bytes, but never authorize media or model identity.
#[derive(Deserialize)]
struct WorkerProvenance {
    schema_version: u32,
    request_binding: GenerationBinding,
    runtime_commit: String,
    pack_revision: String,
    gemma_revision: String,
    adapter_sources_sha256: BTreeMap<String, Sha256>,
    loaded_ltx_sources_sha256: BTreeMap<String, Sha256>,
    verified_assets: Vec<AssetClaim>,
    prompt_version: String,
    prompt: String,
    seed: u64,
    context: crate::BridgeContext,
    configuration: Map<String, Value>,
    model_color_interpretation: String,
    temporal_interpolation: String,
    conditioning_preprocessing: String,
    native_sha256: Sha256,
    native_bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetClaim {
    repository: String,
    path: WorkspaceRef,
    size: u64,
    sha256: Sha256,
}

pub(crate) fn validate(
    value: Value,
    binding: &GenerationBinding,
    declaration: &NativeCandidateManifest,
    conditioning: &crate::BridgeContext,
) -> Result<(), QualificationError> {
    let report: WorkerProvenance = serde_json::from_value(value)?;
    let invalid = |reason: &str| QualificationError::Provenance(reason.into());
    if report.schema_version != 2 || &report.request_binding != binding {
        return Err(invalid(
            "worker request binding/schema differs from persisted intent",
        ));
    }
    if report.seed != binding.provider.seed
        || &report.native_sha256 != declaration.native.sha256()
        || report.native_bytes != declaration.native.byte_length()
        || &report.context != conditioning
    {
        return Err(invalid(
            "worker provenance contradicts its request or native declaration",
        ));
    }
    for revision in [
        &report.runtime_commit,
        &report.pack_revision,
        &report.gemma_revision,
    ] {
        if revision.len() != 40
            || !revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(invalid(
                "source/model revisions must be full lowercase commit hashes",
            ));
        }
    }
    for (text, maximum) in [
        (&report.prompt_version, 256),
        (&report.prompt, 64 * 1024),
        (&report.model_color_interpretation, 4096),
        (&report.temporal_interpolation, 4096),
        (&report.conditioning_preprocessing, 4096),
    ] {
        if text.trim().is_empty() || text.len() > maximum || text.contains('\0') {
            return Err(invalid("missing or oversized provenance description"));
        }
    }
    if report.configuration.is_empty() || report.configuration.len() > 256 {
        return Err(invalid("missing or oversized model configuration"));
    }
    for sources in [
        &report.adapter_sources_sha256,
        &report.loaded_ltx_sources_sha256,
    ] {
        if sources.is_empty()
            || sources.len() > 4096
            || sources
                .keys()
                .any(|path| WorkspaceRef::new(path.clone()).is_err())
        {
            return Err(invalid("missing or invalid runtime source receipts"));
        }
    }
    if report.verified_assets.is_empty() || report.verified_assets.len() > 2048 {
        return Err(invalid("missing or oversized model asset receipts"));
    }
    let mut identities = BTreeSet::new();
    for asset in report.verified_assets {
        if WorkspaceRef::new(asset.repository.clone()).is_err()
            || asset.size == 0
            || !identities.insert((asset.repository, asset.path))
        {
            return Err(invalid("invalid or duplicate model asset receipt"));
        }
        // Deserialization checked the digest; retaining it does not independently
        // attest the worker's claim that these bytes were loaded by its runtime.
        let _ = asset.sha256;
    }
    // Exact prepared input bytes are independently retained by the host. The
    // worker's source/model loading claims still require installed-pack attestation.
    Ok(())
}
