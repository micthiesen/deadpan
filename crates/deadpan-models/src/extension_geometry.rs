//! Host-bound single-anchor face, mouth and selected-region rejection evidence.
//! This inspection does not grant Ready, provenance or acceptance authority.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use deadpan_analysis::generated_extension::ExtensionCoverage;
use deadpan_analysis::generated_geometry::extension::{self as face, ExtensionGeometryReport};
use deadpan_analysis::generated_region::extension::{self as region, ExtensionRegionReport};
use deadpan_core::GeneratedObjectRef;
use deadpan_jobs::landmarks::{
    InspectionExtensionObservations, RegionRuntimeReport, RuntimeReport,
};
use deadpan_jobs::{ExtensionGenerationPlan, HostMessage};
use deadpan_media::CanonicalMedia;
use deadpan_media::protocol::VideoContract;
use serde::{Deserialize, Serialize};

use crate::landmark_inspection::{InspectionTimings, extension_picture_pts};
use crate::quality_input::Control;
use crate::{
    ExtensionConditioningReceipt, ExtensionContext, ExtensionGenerationBinding, QualificationError,
    RetainedExtensionConditioning,
};

const PROFILE: &str = "deadpan-extension-face-mouth-region-1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionGeometryChecks {
    schema_version: u32,
    profile: String,
    native: VideoContract,
    native_object: GeneratedObjectRef,
    context: ExtensionContext,
    conditioning: ExtensionConditioningReceipt,
    runtime: RuntimeReport,
    #[serde(deserialize_with = "required_option")]
    region_runtime: Option<RegionRuntimeReport>,
    timings: InspectionTimings,
    observations: InspectionExtensionObservations,
    geometry: ExtensionGeometryReport,
    #[serde(deserialize_with = "required_option")]
    region: Option<ExtensionRegionReport>,
    #[serde(deserialize_with = "required_option")]
    region_unavailable_reason: Option<String>,
}

impl ExtensionGeometryChecks {
    pub fn geometry(&self) -> &ExtensionGeometryReport {
        &self.geometry
    }
    pub fn region(&self) -> Option<&ExtensionRegionReport> {
        self.region.as_ref()
    }
    pub fn region_unavailable_reason(&self) -> Option<&str> {
        self.region_unavailable_reason.as_deref()
    }
    pub fn timings(&self) -> InspectionTimings {
        self.timings
    }

    /// Recompute policy from retained observations and bind it to the private
    /// native movie and original pre-launch inputs. Does not re-run detection.
    pub fn validate_for(
        &self,
        native: &CanonicalMedia,
        conditioning: &RetainedExtensionConditioning,
        request: &HostMessage,
    ) -> Result<(), QualificationError> {
        validate_inputs(native, conditioning, request)?;
        let binding = ExtensionGenerationBinding::from_request(request)?;
        self.validate_bound(
            &native.report().video,
            native.object(),
            conditioning.context(),
            conditioning.receipt(),
            &binding,
        )
    }

    /// Recompute saved policy and bind its inputs without opening media or
    /// rerunning Vision. The caller separately admits bytes to these identities.
    pub(crate) fn validate_bound(
        &self,
        native: &VideoContract,
        native_object: &GeneratedObjectRef,
        context: &ExtensionContext,
        receipt: &ExtensionConditioningReceipt,
        binding: &ExtensionGenerationBinding,
    ) -> Result<(), QualificationError> {
        receipt.validate_binding(context, binding)?;
        let plan = &binding.plan;
        if self.schema_version != 1
            || self.profile != PROFILE
            || *native != crate::extension_motion::native_contract(plan)
            || self.native != *native
            || &self.native_object != native_object
            || &self.context != context
            || &self.conditioning != receipt
            || self.timings.decode_millis > self.timings.elapsed_millis
            || self.timings.vision_millis > self.timings.elapsed_millis
        {
            return Err(invalid(
                "extension geometry differs from its retained inputs or native contract",
            ));
        }
        self.runtime.validate().map_err(invalid)?;
        let expected = extension_picture_pts(self.native)?;
        let coverage = coverage(plan)?;
        let raster = [self.native.width, self.native.height];
        let crop = self.context.presentation();
        let presentation = [crop.x, crop.y, crop.width, crop.height];
        let seed = self
            .context
            .region()
            .seed(self.context.anchor(), crop, raster)?;
        self.observations
            .validate(&expected, &coverage, seed.as_ref())
            .map_err(invalid)?;
        self.geometry
            .validate_recomputed(
                &self.observations.landmarks,
                &expected,
                raster,
                presentation,
                self.context.anchor().content.is_some(),
            )
            .map_err(invalid)?;
        match (
            &self.observations.region,
            &self.region,
            &self.region_runtime,
            seed,
        ) {
            (Some(observations), Some(assessment), Some(runtime), Some(_)) => {
                if self.region_unavailable_reason.is_some() {
                    return Err(invalid(
                        "measured extension region has an unavailable capture",
                    ));
                }
                runtime.validate().map_err(invalid)?;
                assessment
                    .validate_recomputed(observations, &expected, raster, presentation)
                    .map_err(invalid)?;
                if let Some(rejection) = assessment.rejection {
                    return Err(invalid(format!(
                        "gross selected-region drift at native frame {} over {} outward consecutive frames",
                        rejection.first_ordinal, rejection.consecutive_frames
                    )));
                }
            }
            (None, None, None, None)
                if self.region_unavailable_reason == self.context.region().unavailable_reason()
                    && self.region_unavailable_reason.is_some() => {}
            _ => {
                return Err(invalid(
                    "extension region evidence differs from its captured selection",
                ));
            }
        }
        if let Some(rejection) = self.geometry.geometry.rejection {
            return Err(invalid(format!(
                "gross face geometry drift at native frame {} over {} outward consecutive frames",
                rejection.first_ordinal, rejection.consecutive_frames
            )));
        }
        if let Some(rejection) = self.geometry.mouth.rejection {
            return Err(invalid(format!(
                "gross mouth motion at native frame {} over {} chronological changes",
                rejection.first_after_ordinal, rejection.consecutive_changes
            )));
        }
        Ok(())
    }
}

/// Inspect after the generation worker has been stopped and reaped. `executable`
/// is selected by the host's vetted helper installation. Only retained anchor
/// bytes enter this detector; the opposite seam cannot become a target or seed.
pub fn inspect_extension_geometry(
    executable: &Path,
    native: &mut CanonicalMedia,
    conditioning: &mut RetainedExtensionConditioning,
    request: &HostMessage,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<ExtensionGeometryChecks, QualificationError> {
    let control = Control {
        deadline,
        cancelled,
    };
    control.remaining()?;
    let plan = validate_inputs(native, conditioning, request)?;
    let coverage = coverage(plan)?;
    let context = conditioning.context().clone();
    let contract = native.report().video;
    let raster = [contract.width, contract.height];
    let crop = context.presentation();
    let presentation = [crop.x, crop.y, crop.width, crop.height];
    let seed = context.region().seed(context.anchor(), crop, raster)?;
    let observed = crate::landmark_inspection::inspect_extension(
        executable,
        native,
        conditioning,
        coverage,
        seed,
        deadline,
        cancelled,
    )?;
    let expected = extension_picture_pts(contract)?;
    let geometry = face::analyze(
        &observed.batch.landmarks,
        &expected,
        raster,
        presentation,
        context.anchor().content.is_some(),
    )
    .map_err(invalid)?;
    let region = observed
        .batch
        .region
        .as_ref()
        .map(|batch| region::analyze(batch, &expected, raster, presentation))
        .transpose()
        .map_err(invalid)?;
    let report = ExtensionGeometryChecks {
        schema_version: 1,
        profile: PROFILE.into(),
        native: contract,
        native_object: native.object().clone(),
        region_unavailable_reason: context.region().unavailable_reason(),
        context,
        conditioning: conditioning.receipt().clone(),
        runtime: observed.runtime,
        region_runtime: observed.region_runtime,
        timings: observed.timings,
        observations: observed.batch,
        geometry,
        region,
    };
    report.validate_for(native, conditioning, request)?;
    control.remaining()?;
    Ok(report)
}

fn validate_inputs<'a>(
    native: &CanonicalMedia,
    conditioning: &RetainedExtensionConditioning,
    request: &'a HostMessage,
) -> Result<&'a ExtensionGenerationPlan, QualificationError> {
    request
        .validate()
        .map_err(|error| QualificationError::Request(error.to_string()))?;
    let HostMessage::GenerateExtension { plan, .. } = request else {
        return Err(invalid("geometry inspection requires an extension request"));
    };
    conditioning.validate_for(request)?;
    if native.report().video != crate::extension_motion::native_contract(plan) {
        return Err(invalid(
            "private native movie differs from the captured extension plan",
        ));
    }
    Ok(plan)
}

fn coverage(plan: &ExtensionGenerationPlan) -> Result<ExtensionCoverage, QualificationError> {
    let interval = plan.sampling_map().generated_interval();
    let result = ExtensionCoverage {
        direction: plan.direction(),
        start: u32::try_from(interval.start).map_err(invalid)?,
        end: u32::try_from(interval.end).map_err(invalid)?,
    };
    result
        .validate(&extension_picture_pts(
            crate::extension_motion::native_contract(plan),
        )?)
        .map_err(invalid)?;
    Ok(result)
}

fn invalid(reason: impl std::fmt::Display) -> QualificationError {
    QualificationError::Quality(format!("{PROFILE}: {reason}"))
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}
