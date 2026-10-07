//! Retained, recomputable face geometry and mouth measurements. Detection and
//! association coverage is evidence, not a guarantee of identity or silence.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use deadpan_analysis::generated_geometry::{self, GeneratedGeometryReport, RawLandmarkBatch};
use deadpan_core::GeneratedObjectRef;
use deadpan_jobs::BridgeGenerationPlan;
use deadpan_jobs::landmarks::RuntimeReport;
use deadpan_media::CanonicalMedia;
use deadpan_media::protocol::VideoContract;
use serde::{Deserialize, Serialize};

use crate::landmark_inspection::{InspectionTimings, picture_pts};
use crate::{
    BridgeContext, ConditioningGeometry, ConditioningReceipt, QualificationError,
    RetainedConditioning,
};

const PROFILE: &str = "deadpan-face-mouth-1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeGeometryReport {
    schema_version: u32,
    profile: String,
    native: VideoContract,
    native_object: GeneratedObjectRef,
    context_object: GeneratedObjectRef,
    left_object: GeneratedObjectRef,
    right_object: GeneratedObjectRef,
    geometry: ConditioningGeometry,
    runtime: RuntimeReport,
    timings: InspectionTimings,
    observations: RawLandmarkBatch,
    assessment: GeneratedGeometryReport,
}

impl BridgeGeometryReport {
    pub(crate) fn geometry(&self) -> ConditioningGeometry {
        self.geometry
    }
    pub(crate) fn inspection_timings(&self) -> InspectionTimings {
        self.timings
    }
    pub fn assessment(&self) -> &GeneratedGeometryReport {
        &self.assessment
    }

    pub(crate) fn validate(
        &self,
        plan: &BridgeGenerationPlan,
        native: &GeneratedObjectRef,
        conditioning: &ConditioningReceipt,
    ) -> Result<(), QualificationError> {
        if self.schema_version != 1
            || self.profile != PROFILE
            || self.native != contract(plan)
            || &self.native_object != native
            || &self.context_object != conditioning.manifest().object()
            || &self.left_object != conditioning.left().object()
            || &self.right_object != conditioning.right().object()
            || self.timings.decode_millis > self.timings.elapsed_millis
            || self.timings.vision_millis > self.timings.elapsed_millis
            || self.observations.boundaries.is_none()
        {
            return Err(invalid(
                "report differs from its captured inputs or contract",
            ));
        }
        self.runtime.validate().map_err(invalid)?;
        let crop = self.geometry.presentation;
        self.assessment
            .validate_recomputed(
                &self.observations,
                &picture_pts(self.native)?,
                [self.native.width, self.native.height],
                [crop.x, crop.y, crop.width, crop.height],
                [
                    self.geometry.left_content.is_some(),
                    self.geometry.right_content.is_some(),
                ],
            )
            .map_err(invalid)?;
        if let Some(rejection) = self.assessment.geometry.rejection {
            return Err(invalid(format!(
                "gross face geometry drift beginning at native frame {} over {} consecutive frames: center {:.4}, log size {:.4}, eye/nose {}; limits {:.4}, {:.4}, {:.4}",
                rejection.first_ordinal,
                rejection.consecutive_frames,
                rejection.center_residual,
                rejection.log_size_residual,
                rejection
                    .feature_residual
                    .map_or_else(|| "unavailable".into(), |value| format!("{value:.4}")),
                self.assessment.thresholds.center_residual,
                self.assessment.thresholds.log_size_residual,
                self.assessment.thresholds.feature_residual,
            )));
        }
        if let Some(rejection) = self.assessment.mouth.rejection {
            return Err(invalid(format!(
                "gross mouth motion: first changed native frame {}, over {} consecutive changes; aperture step {:.4} eye distances, limit {:.4}",
                rejection.first_after_ordinal,
                rejection.consecutive_changes,
                rejection.aperture_step,
                self.assessment.mouth_thresholds.aperture_step,
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_context(
        &self,
        context: &BridgeContext,
    ) -> Result<(), QualificationError> {
        if context.geometry() != Some(&self.geometry) {
            return Err(invalid(
                "geometry differs from retained conditioning context",
            ));
        }
        Ok(())
    }
}

pub(crate) fn measure(
    executable: &Path,
    native: &mut CanonicalMedia,
    conditioning: &mut RetainedConditioning,
    plan: &BridgeGenerationPlan,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<(BridgeGeometryReport, crate::BridgeRegionReport), QualificationError> {
    let geometry = conditioning
        .context()
        .geometry()
        .copied()
        .ok_or_else(|| invalid("fresh inspection requires captured geometry"))?;
    let contract = contract(plan);
    let region_seeds = crate::region::captured_seeds(conditioning.context(), contract)?;
    let observed = crate::landmark_inspection::inspect(
        executable,
        native,
        conditioning,
        contract,
        region_seeds,
        deadline,
        cancelled,
    )?;
    let crop = geometry.presentation;
    let assessment = generated_geometry::analyze(
        &observed.batch.landmarks,
        &picture_pts(contract)?,
        [contract.width, contract.height],
        [crop.x, crop.y, crop.width, crop.height],
        [
            geometry.left_content.is_some(),
            geometry.right_content.is_some(),
        ],
    )
    .map_err(invalid)?;
    let region = crate::region::from_observations(
        plan,
        native.object(),
        conditioning,
        observed.batch.region,
        observed.region_runtime,
        observed.timings,
    )?;
    let report = BridgeGeometryReport {
        schema_version: 1,
        profile: PROFILE.into(),
        native: contract,
        native_object: native.object().clone(),
        context_object: conditioning.receipt().manifest().object().clone(),
        left_object: conditioning.receipt().left().object().clone(),
        right_object: conditioning.receipt().right().object().clone(),
        geometry,
        runtime: observed.runtime,
        timings: observed.timings,
        observations: observed.batch.landmarks,
        assessment,
    };
    report.validate(plan, native.object(), conditioning.receipt())?;
    report.validate_context(conditioning.context())?;
    Ok((report, region))
}

pub(crate) fn contract(plan: &BridgeGenerationPlan) -> VideoContract {
    VideoContract {
        width: plan.native_dimensions().width(),
        height: plan.native_dimensions().height(),
        frames: plan.native_frame_count(),
        rate_num: plan.native_frame_rate().numerator(),
        rate_den: plan.native_frame_rate().denominator(),
    }
}

fn invalid(error: impl std::fmt::Display) -> QualificationError {
    QualificationError::Quality(format!("{PROFILE}: {error}"))
}

#[cfg(test)]
pub(crate) fn test_report(
    plan: &BridgeGenerationPlan,
    native: &GeneratedObjectRef,
    conditioning: &ConditioningReceipt,
    geometry: ConditioningGeometry,
) -> BridgeGeometryReport {
    use deadpan_analysis::generated_geometry::{
        BoundaryObservations, FaceObservationSet, FrameObservation,
    };
    let contract = contract(plan);
    let pts = picture_pts(contract).unwrap();
    let empty = || FaceObservationSet::Detected { faces: Vec::new() };
    let observations = RawLandmarkBatch {
        schema_version: 1,
        boundaries: Some(BoundaryObservations {
            left: empty(),
            right: empty(),
        }),
        frames: pts
            .iter()
            .enumerate()
            .map(|(index, &pts)| FrameObservation {
                ordinal: index as u32,
                pts,
                observation: empty(),
            })
            .collect(),
    };
    let crop = geometry.presentation;
    let assessment = generated_geometry::analyze(
        &observations,
        &pts,
        [contract.width, contract.height],
        [crop.x, crop.y, crop.width, crop.height],
        [
            geometry.left_content.is_some(),
            geometry.right_content.is_some(),
        ],
    )
    .unwrap();
    BridgeGeometryReport {
        schema_version: 1,
        profile: PROFILE.into(),
        native: contract,
        native_object: native.clone(),
        context_object: conditioning.manifest().object().clone(),
        left_object: conditioning.left().object().clone(),
        right_object: conditioning.right().object().clone(),
        geometry,
        runtime: RuntimeReport {
            engine: deadpan_jobs::landmarks::ENGINE.into(),
            request_revision: deadpan_jobs::landmarks::REQUEST_REVISION,
            constellation: deadpan_jobs::landmarks::CONSTELLATION,
        },
        timings: InspectionTimings {
            decode_millis: 0,
            vision_millis: 0,
            elapsed_millis: 0,
        },
        observations,
        assessment,
    }
}
