//! Resolve the authored operation before selecting its independently qualified
//! model, then capture pictures against that exact immutable plan.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_core::{
    BoundaryQueryLimits, FrameDuration, FrameRate, ProjectDocument, RevisionId, ScopedNodeTarget,
};
use deadpan_jobs::{ConditioningMode, ConditioningSupport, GenerationOptions, GenerationPlan};
use deadpan_models::packs::PackManifest;
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};

use super::conditioning::{self, PreparedInputs};
use super::runtime::{self, BridgeRuntime, SelectedGenerationProvider};

/// Endpoint existence in the Hold's authored definition, before outer timing.
/// This chooses the operation; the selected manifest separately admits it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub operation: ConditioningMode,
    pub opposite_boundary_present: Option<bool>,
    pub frames: FrameDuration,
    pub rate: FrameRate,
}

pub fn resolve(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    options: &GenerationOptions,
) -> Result<Resolution, String> {
    let plan = RenderPlan::compile(document).map_err(display)?;
    resolve_with_plan(
        &plan,
        document.presentation_basis().frame_rate,
        target,
        options,
    )
}

/// Reuse a workspace's compiled immutable plan on the project service thread.
pub fn resolve_with_plan(
    plan: &RenderPlan,
    rate: FrameRate,
    target: &ScopedNodeTarget,
    options: &GenerationOptions,
) -> Result<Resolution, String> {
    let boundaries = plan
        .scoped_hold_boundaries(target, BoundaryQueryLimits::default())
        .map_err(display)?;
    // These are the operations implemented by the host router. This does not
    // confer provider support: plan_for and selected_provider require the
    // actual selected pack's independent operation declaration.
    let operation = options
        .mode
        .resolve(
            boundaries.left.is_some(),
            boundaries.right.is_some(),
            ConditioningSupport {
                bridge: true,
                extend_from_left: true,
                extend_from_right: true,
            },
        )
        .map_err(display)?;
    Ok(Resolution {
        operation,
        opposite_boundary_present: match operation {
            ConditioningMode::Bridge => None,
            ConditioningMode::ExtendFromLeft => Some(boundaries.right.is_some()),
            ConditioningMode::ExtendFromRight => Some(boundaries.left.is_some()),
        },
        frames: boundaries.duration,
        rate,
    })
}

pub fn resolve_at(
    package: &Path,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
    options: &GenerationOptions,
    cancelled: &AtomicBool,
) -> Result<Resolution, String> {
    check_cancel(cancelled)?;
    let store = ProjectStore::open(package, AccessMode::ReadOnly).map_err(display)?;
    let document = store.snapshot_at(revision).map_err(display)?;
    let resolved = resolve(&document, target, options)?;
    check_cancel(cancelled)?;
    Ok(resolved)
}

/// Capture using the selected runtime's immutable manifest. This does not
/// allocate an attempt, run inference or alter authored state.
pub fn with_runtime(
    package: &Path,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
    options: &GenerationOptions,
    runtime: &BridgeRuntime,
    cancelled: &AtomicBool,
) -> Result<PreparedInputs, String> {
    with_manifest(
        package,
        revision,
        target,
        options,
        &runtime.model_manifest,
        cancelled,
    )
}

/// Also used by the deterministic worker seam, which supplies an explicit
/// compiled manifest rather than pretending that a model is installed.
pub fn with_manifest(
    package: &Path,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
    options: &GenerationOptions,
    manifest: &PackManifest,
    cancelled: &AtomicBool,
) -> Result<PreparedInputs, String> {
    let resolved = resolve_at(package, revision, target, options, cancelled)?;
    runtime::validate_controls_for_manifest(manifest, resolved.operation, options)?;
    let plan =
        runtime::plan_for_manifest(manifest, resolved.operation, resolved.frames, resolved.rate)?;
    let provider = runtime::selected_provider_for_manifest(manifest, &plan, 0)?;
    match (&plan, &provider) {
        (GenerationPlan::Bridge(plan), SelectedGenerationProvider::Bridge(_)) => {
            conditioning::prepare_bridge_scoped_with_plan(
                package, revision, target, plan, options, cancelled,
            )
            .map(Into::into)
        }
        (GenerationPlan::Extension(plan), SelectedGenerationProvider::Extension(selected)) => {
            conditioning::prepare_extension_scoped_with_provider(
                package, revision, target, plan, selected, options, cancelled,
            )
            .map(Into::into)
        }
        _ => Err("The selected provider and generation plan name different operations.".into()),
    }
}

pub fn pack_for(operation: ConditioningMode) -> &'static str {
    match operation {
        ConditioningMode::Bridge => runtime::BRIDGE_PACK,
        ConditioningMode::ExtendFromLeft | ConditioningMode::ExtendFromRight => {
            runtime::EXTENSION_PACK
        }
    }
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        Err("The AI pause was cancelled.".into())
    } else {
        Ok(())
    }
}

fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}
