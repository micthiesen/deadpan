//! Provider authority comes from the selected immutable pack manifest.

use deadpan_core::{ExtensionDirection, FrameDuration, FrameRate};
use deadpan_jobs::{
    BridgeGenerationPlan, ConditioningMode, GenerationPlan, NativeDimensions, ProviderSelection,
};
use deadpan_models::packs::{Operation, PackManifest};
use deadpan_models::{SelectedBridgeProvider, SelectedExtensionProvider};

use super::{BRIDGE_PACK, EXTENSION_PACK};

mod constraints;
pub use constraints::{validate_constraints_for_manifest, validate_controls_for_manifest};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub enum SelectedGenerationProvider {
    Bridge(SelectedBridgeProvider),
    Extension(SelectedExtensionProvider),
}

impl SelectedGenerationProvider {
    pub fn selection(&self) -> &ProviderSelection {
        match self {
            Self::Bridge(provider) => provider.selection(),
            Self::Extension(provider) => provider.selection(),
        }
    }
}

pub(super) fn pack_id(mode: ConditioningMode) -> &'static str {
    match mode {
        ConditioningMode::Bridge => BRIDGE_PACK,
        ConditioningMode::ExtendFromLeft | ConditioningMode::ExtendFromRight => EXTENSION_PACK,
    }
}

pub(super) fn manifest_mode(manifest: &PackManifest) -> Result<ConditioningMode, String> {
    manifest.validate().map_err(|error| error.to_string())?;
    let (mode, operation, version) = match manifest.pack_id.as_str() {
        BRIDGE_PACK => (
            ConditioningMode::Bridge,
            Operation::BridgeHold,
            "0.15.8+deadpan5",
        ),
        EXTENSION_PACK => (
            ConditioningMode::ExtendFromLeft,
            Operation::ExtensionHold,
            "0.15.8+deadpan-extension1",
        ),
        _ => return Err("the selected pack is not supported by the generation runtime".into()),
    };
    if manifest.operations != [operation]
        || manifest.runtime_id != "ltx-mlx"
        || manifest.model_family != "ltx-2.3"
        || manifest.runtime_versions != [version]
    {
        return Err("the selected pack operation or runtime compatibility is unsupported".into());
    }
    Ok(mode)
}

pub(super) fn provider(manifest: &PackManifest, seed: u64) -> Result<ProviderSelection, String> {
    manifest_mode(manifest)?;
    serde_json::from_value(serde_json::json!({
        "pack_id": manifest.pack_id,
        "pack_version": manifest.pack_version,
        "runtime_id": manifest.runtime_id,
        "runtime_version": manifest.runtime_versions[0],
        "seed": seed,
    }))
    .map_err(|error| error.to_string())
}

/// Resolve dimensions and legal internal duration from this manifest only.
pub fn plan_for_manifest(
    manifest: &PackManifest,
    mode: ConditioningMode,
    frames: FrameDuration,
    rate: FrameRate,
) -> Result<GenerationPlan, String> {
    if pack_id(manifest_mode(manifest)?) != pack_id(mode) {
        return Err("requested conditioning differs from the selected pack operation".into());
    }
    let plan = match mode {
        ConditioningMode::Bridge => {
            let constraints = manifest
                .constraints
                .bridge
                .as_ref()
                .ok_or("selected pack has no Bridge capability")?;
            let dimensions =
                NativeDimensions::new(constraints.width.minimum, constraints.height.minimum)
                    .map_err(|error| error.to_string())?;
            BridgeGenerationPlan::new(
                frames,
                rate,
                &constraints
                    .capability()
                    .map_err(|error| error.to_string())?,
                dimensions,
            )
            .map(GenerationPlan::Bridge)
            .map_err(|error| error.to_string())?
        }
        ConditioningMode::ExtendFromLeft | ConditioningMode::ExtendFromRight => {
            let constraints = manifest
                .constraints
                .extension
                .as_ref()
                .ok_or("selected pack has no Extension capability")?;
            let dimensions =
                NativeDimensions::new(constraints.width.minimum, constraints.height.minimum)
                    .map_err(|error| error.to_string())?;
            let direction = if mode == ConditioningMode::ExtendFromLeft {
                ExtensionDirection::FromLeft
            } else {
                ExtensionDirection::FromRight
            };
            constraints
                .plan(direction, frames, rate, dimensions)
                .map(GenerationPlan::Extension)
                .map_err(|error| error.to_string())?
        }
    };
    selected_provider_for_manifest(manifest, &plan, 0)?;
    Ok(plan)
}

/// Pure selection for runtime execution and scripted host tests. The caller
/// supplies the immutable selected manifest; candidate metadata is not authority.
pub fn selected_provider_for_manifest(
    manifest: &PackManifest,
    plan: &GenerationPlan,
    seed: u64,
) -> Result<SelectedGenerationProvider, String> {
    let mode = manifest_mode(manifest)?;
    if pack_id(mode) != pack_id(plan.conditioning()) {
        return Err("generation plan operation differs from the selected pack".into());
    }
    let rate = plan.project_frame_rate();
    if u64::from(rate.numerator()) < u64::from(rate.denominator())
        || u64::from(rate.numerator()) > 120 * u64::from(rate.denominator())
    {
        return Err("generation requires a project frame rate from 1 through 120 fps".into());
    }
    let selection = provider(manifest, seed)?;
    match plan {
        GenerationPlan::Bridge(plan) => {
            let constraints = manifest
                .constraints
                .bridge
                .as_ref()
                .ok_or("selected pack has no Bridge capability")?;
            if plan.project_frames().frames() > i64::from(constraints.maximum_project_frames) {
                return Err("Bridge output exceeds the selected pack's frame bound".into());
            }
            let capability = constraints
                .capability()
                .map_err(|error| error.to_string())?;
            plan.validate_for(&capability)
                .map_err(|error| error.to_string())?;
            Ok(SelectedGenerationProvider::Bridge(
                SelectedBridgeProvider::new(selection, capability),
            ))
        }
        GenerationPlan::Extension(plan) => {
            let capability = manifest
                .constraints
                .extension
                .as_ref()
                .ok_or("selected pack has no Extension capability")?
                .capability(rate)
                .map_err(|error| error.to_string())?;
            plan.validate_for(&capability)
                .map_err(|error| error.to_string())?;
            Ok(SelectedGenerationProvider::Extension(
                SelectedExtensionProvider::new(selection, capability),
            ))
        }
    }
}
