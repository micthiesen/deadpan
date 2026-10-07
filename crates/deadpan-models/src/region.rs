//! Selected-region drift evidence bound to captured conditioning inputs.
//! An unavailable capture is retained explicitly and never replaced with a
//! content or presentation rectangle as the tracking seed.

use deadpan_analysis::generated_region::{
    self, GeneratedRegionReport, RawRegionBatch, RegionSeeds,
};
use deadpan_core::GeneratedObjectRef;
use deadpan_jobs::BridgeGenerationPlan;
use deadpan_jobs::landmarks::RegionRuntimeReport;
use deadpan_media::protocol::VideoContract;
use serde::{Deserialize, Serialize};

use crate::landmark_inspection::{InspectionTimings, picture_pts};
use crate::{
    BridgeBoundaries, BridgeContext, ConditioningGeometry, ConditioningReceipt, QualificationError,
    RegionCapture, RetainedConditioning,
};

const PROFILE: &str = "deadpan-selected-region-1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeRegionReport {
    schema_version: u32,
    profile: String,
    native: VideoContract,
    native_object: GeneratedObjectRef,
    context_object: GeneratedObjectRef,
    left_object: GeneratedObjectRef,
    right_object: GeneratedObjectRef,
    geometry: ConditioningGeometry,
    boundaries: BridgeBoundaries,
    capture: RegionCapture,
    evidence: RegionEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum RegionEvidence {
    Unavailable {
        reason: String,
    },
    Measured {
        runtime: RegionRuntimeReport,
        timings: InspectionTimings,
        observations: Box<RawRegionBatch>,
        assessment: Box<GeneratedRegionReport>,
    },
}

impl BridgeRegionReport {
    pub(crate) fn geometry(&self) -> ConditioningGeometry {
        self.geometry
    }

    pub fn capture(&self) -> &RegionCapture {
        &self.capture
    }

    pub fn assessment(&self) -> Option<&GeneratedRegionReport> {
        match &self.evidence {
            RegionEvidence::Measured { assessment, .. } => Some(assessment),
            RegionEvidence::Unavailable { .. } => None,
        }
    }

    pub fn unavailable_reason(&self) -> Option<&str> {
        match &self.evidence {
            RegionEvidence::Unavailable { reason } => Some(reason),
            RegionEvidence::Measured { .. } => None,
        }
    }

    pub(crate) fn inspection_timings(&self) -> Option<InspectionTimings> {
        match &self.evidence {
            RegionEvidence::Measured { timings, .. } => Some(*timings),
            RegionEvidence::Unavailable { .. } => None,
        }
    }

    pub(crate) fn validate(
        &self,
        plan: &BridgeGenerationPlan,
        native: &GeneratedObjectRef,
        conditioning: &ConditioningReceipt,
    ) -> Result<(), QualificationError> {
        if self.schema_version != 1
            || self.profile != PROFILE
            || self.native != crate::geometry::contract(plan)
            || &self.native_object != native
            || &self.context_object != conditioning.manifest().object()
            || &self.left_object != conditioning.left().object()
            || &self.right_object != conditioning.right().object()
        {
            return Err(invalid(
                "report differs from its captured inputs or contract",
            ));
        }
        let seeds = self
            .capture
            .seeds(
                &self.boundaries,
                &self.geometry,
                [self.native.width, self.native.height],
            )
            .map_err(invalid)?;
        match (&self.evidence, seeds) {
            (RegionEvidence::Unavailable { reason }, None)
                if self.capture.unavailable_reason().as_deref() == Some(reason.as_str()) =>
            {
                Ok(())
            }
            (
                RegionEvidence::Measured {
                    runtime,
                    timings,
                    observations,
                    assessment,
                },
                Some(seeds),
            ) => {
                if observations.seeds != seeds
                    || timings.decode_millis > timings.elapsed_millis
                    || timings.vision_millis > timings.elapsed_millis
                {
                    return Err(invalid(
                        "region observations differ from the captured seeds or timings",
                    ));
                }
                runtime.validate().map_err(invalid)?;
                let crop = self.geometry.presentation;
                assessment
                    .validate_recomputed(
                        observations,
                        &picture_pts(self.native)?,
                        [self.native.width, self.native.height],
                        [crop.x, crop.y, crop.width, crop.height],
                    )
                    .map_err(invalid)?;
                if let Some(rejection) = assessment.rejection {
                    return Err(invalid(format!(
                        "gross selected-region drift beginning at native frame {} over {} consecutive frames: center {:.4}, log size {:.4}; limits {:.4}, {:.4}",
                        rejection.first_ordinal,
                        rejection.consecutive_frames,
                        rejection.center_residual,
                        rejection.log_size_residual,
                        assessment.thresholds.center_residual,
                        assessment.thresholds.log_size_residual,
                    )));
                }
                Ok(())
            }
            _ => Err(invalid(
                "region evidence does not match its explicit capture availability",
            )),
        }
    }

    pub(crate) fn validate_context(
        &self,
        context: &BridgeContext,
    ) -> Result<(), QualificationError> {
        if context.geometry() != Some(&self.geometry)
            || context.boundaries() != Some(&self.boundaries)
            || context.region() != Some(&self.capture)
        {
            return Err(invalid(
                "region capture or geometry differs from retained conditioning context",
            ));
        }
        Ok(())
    }
}

pub(crate) fn captured_seeds(
    context: &BridgeContext,
    contract: VideoContract,
) -> Result<Option<RegionSeeds>, QualificationError> {
    let capture = context
        .region()
        .ok_or_else(|| invalid("fresh inspection requires an explicit region capture"))?;
    let boundaries = context
        .boundaries()
        .ok_or_else(|| invalid("fresh inspection requires retained boundaries"))?;
    let geometry = context
        .geometry()
        .ok_or_else(|| invalid("fresh inspection requires captured geometry"))?;
    capture
        .seeds(boundaries, geometry, [contract.width, contract.height])
        .map_err(invalid)
}

pub(crate) fn from_observations(
    plan: &BridgeGenerationPlan,
    native: &GeneratedObjectRef,
    conditioning: &RetainedConditioning,
    observations: Option<RawRegionBatch>,
    runtime: Option<RegionRuntimeReport>,
    timings: InspectionTimings,
) -> Result<BridgeRegionReport, QualificationError> {
    assemble(
        plan,
        native,
        conditioning.receipt(),
        conditioning.context(),
        observations,
        runtime,
        timings,
    )
}

fn assemble(
    plan: &BridgeGenerationPlan,
    native: &GeneratedObjectRef,
    conditioning: &ConditioningReceipt,
    context: &BridgeContext,
    observations: Option<RawRegionBatch>,
    runtime: Option<RegionRuntimeReport>,
    timings: InspectionTimings,
) -> Result<BridgeRegionReport, QualificationError> {
    let contract = crate::geometry::contract(plan);
    let capture = context
        .region()
        .cloned()
        .ok_or_else(|| invalid("fresh inspection requires an explicit region capture"))?;
    let geometry = context
        .geometry()
        .copied()
        .ok_or_else(|| invalid("fresh inspection requires captured geometry"))?;
    let boundaries = context
        .boundaries()
        .cloned()
        .ok_or_else(|| invalid("fresh inspection requires retained boundaries"))?;
    let seeds = captured_seeds(context, contract)?;
    let evidence = match (seeds, observations, runtime) {
        (None, None, None) => RegionEvidence::Unavailable {
            reason: capture
                .unavailable_reason()
                .ok_or_else(|| invalid("unavailable capture omitted its reason"))?,
        },
        (Some(seeds), Some(observations), Some(runtime)) => {
            if observations.seeds != seeds {
                return Err(invalid(
                    "raw tracking seeds differ from the captured subject",
                ));
            }
            let crop = geometry.presentation;
            let assessment = generated_region::analyze(
                &observations,
                &picture_pts(contract)?,
                [contract.width, contract.height],
                [crop.x, crop.y, crop.width, crop.height],
            )
            .map_err(invalid)?;
            RegionEvidence::Measured {
                runtime,
                timings,
                observations: Box::new(observations),
                assessment: Box::new(assessment),
            }
        }
        _ => {
            return Err(invalid(
                "worker region coverage differs from the captured subject",
            ));
        }
    };
    let report = BridgeRegionReport {
        schema_version: 1,
        profile: PROFILE.into(),
        native: contract,
        native_object: native.clone(),
        context_object: conditioning.manifest().object().clone(),
        left_object: conditioning.left().object().clone(),
        right_object: conditioning.right().object().clone(),
        geometry,
        boundaries,
        capture,
        evidence,
    };
    report.validate(plan, native, conditioning)?;
    report.validate_context(context)?;
    Ok(report)
}

fn invalid(error: impl std::fmt::Display) -> QualificationError {
    QualificationError::Quality(format!("{PROFILE}: {error}"))
}

#[cfg(test)]
pub(crate) fn test_report(
    plan: &BridgeGenerationPlan,
    native: &GeneratedObjectRef,
    conditioning: &ConditioningReceipt,
    context: &BridgeContext,
) -> BridgeRegionReport {
    assemble(
        plan,
        native,
        conditioning,
        context,
        None,
        None,
        InspectionTimings {
            decode_millis: 0,
            vision_millis: 0,
            elapsed_millis: 0,
        },
    )
    .unwrap()
}

#[cfg(test)]
#[path = "region_tests.rs"]
mod tests;
