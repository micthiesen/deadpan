//! Gross splice discontinuity checks on the exact sampled interval that enters
//! the edit. These do not replace audition or the advisory join readings.

use deadpan_core::GeneratedObjectRef;
use deadpan_jobs::BridgeGenerationPlan;
use deadpan_media::protocol::VideoContract;
use serde::{Deserialize, Serialize};

use crate::{
    BridgeContext, ConditioningGeometry, ConditioningReceipt, QualificationError, RasterRect,
};

#[path = "endpoint_decode.rs"]
mod decode;
pub(crate) use decode::measure;

const PROFILE: &str = "deadpan-endpoints-1";
pub(crate) const MAX_SAMPLED_FRAMES: u32 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointThresholds {
    pub gross_mean_difference: f64,
    pub gross_cell_difference: f64,
    pub minimum_gross_fraction: f64,
}

impl EndpointThresholds {
    fn policy() -> Self {
        Self {
            gross_mean_difference: 64.0,
            gross_cell_difference: 64.0,
            minimum_gross_fraction: 0.75,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointObservation {
    pub sampled_frame: u32,
    /// Exact canonical Matroska PTS, in milliseconds.
    pub sampled_pts: i64,
    pub mean_absolute_rgb_difference: f64,
    /// Fraction of presentation pixels belonging to cells with gross changes.
    pub gross_cell_fraction: f64,
}

/// Evidence for both actual edit joins, bound to the sampled movie and the
/// retained conditioning objects. Geometry is checked again against the
/// retained context when admitting accepted footage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeEndpointReport {
    schema_version: u32,
    profile: String,
    sampled: VideoContract,
    sampled_object: GeneratedObjectRef,
    context_object: GeneratedObjectRef,
    left_object: GeneratedObjectRef,
    right_object: GeneratedObjectRef,
    geometry: ConditioningGeometry,
    thresholds: EndpointThresholds,
    entry: EndpointObservation,
    exit: EndpointObservation,
}

impl BridgeEndpointReport {
    pub(crate) fn geometry(&self) -> ConditioningGeometry {
        self.geometry
    }
    pub fn entry(&self) -> EndpointObservation {
        self.entry
    }

    pub fn exit(&self) -> EndpointObservation {
        self.exit
    }

    pub fn thresholds(&self) -> EndpointThresholds {
        self.thresholds
    }

    pub(crate) fn validate(
        &self,
        plan: &BridgeGenerationPlan,
        sampled_object: &GeneratedObjectRef,
        conditioning: &ConditioningReceipt,
    ) -> Result<(), QualificationError> {
        let sampled = sampled_contract(plan)?;
        if self.schema_version != 1
            || self.profile != PROFILE
            || self.sampled != sampled
            || &self.sampled_object != sampled_object
            || &self.context_object != conditioning.manifest().object()
            || &self.left_object != conditioning.left().object()
            || &self.right_object != conditioning.right().object()
            || self.thresholds != EndpointThresholds::policy()
        {
            return Err(invalid(
                "endpoint report differs from its objects, plan or policy",
            ));
        }
        let native = [sampled.width, sampled.height];
        let centered = |rect: RasterRect| -> Result<(), QualificationError> {
            rect.validate_within(native).map_err(invalid)?;
            if RasterRect::centered(rect.width, rect.height, native).map_err(invalid)? != rect {
                return Err(invalid("endpoint comparison geometry is not centered"));
            }
            Ok(())
        };
        let presentation = self.geometry.presentation;
        centered(presentation)?;
        for content in [self.geometry.left_content, self.geometry.right_content]
            .into_iter()
            .flatten()
        {
            centered(content)?;
            // All additions are safe after `validate_within` checked them.
            if content.x < presentation.x
                || content.y < presentation.y
                || content.x + content.width > presentation.x + presentation.width
                || content.y + content.height > presentation.y + presentation.height
            {
                return Err(invalid(
                    "endpoint content lies outside its presentation crop",
                ));
            }
        }
        for (name, observation, ordinal) in [
            ("entry", self.entry, 0),
            ("exit", self.exit, sampled.frames - 1),
        ] {
            if observation.sampled_frame != ordinal
                || observation.sampled_pts != sampled.matroska_pts(ordinal).map_err(invalid)?
                || !(0.0..=255.0).contains(&observation.mean_absolute_rgb_difference)
                || !(0.0..=1.0).contains(&observation.gross_cell_fraction)
                || observation.mean_absolute_rgb_difference + 1e-9
                    < observation.gross_cell_fraction * self.thresholds.gross_cell_difference
                || observation.mean_absolute_rgb_difference
                    > observation.gross_cell_fraction * 255.0
                        + (1.0 - observation.gross_cell_fraction)
                            * self.thresholds.gross_cell_difference
                        + 1e-9
            {
                return Err(invalid(format!(
                    "{name} endpoint report is incomplete or inconsistent"
                )));
            }
            if observation.mean_absolute_rgb_difference >= self.thresholds.gross_mean_difference
                && observation.gross_cell_fraction >= self.thresholds.minimum_gross_fraction
            {
                return Err(invalid(format!(
                    "{PROFILE}: gross {name} discontinuity at sampled frame {ordinal}: mean RGB difference {:.3}/255, gross coverage {:.3}; limits {:.3}/{:.3}",
                    observation.mean_absolute_rgb_difference,
                    observation.gross_cell_fraction,
                    self.thresholds.gross_mean_difference,
                    self.thresholds.minimum_gross_fraction,
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn validate_context(
        &self,
        context: &BridgeContext,
    ) -> Result<(), QualificationError> {
        if context.geometry() != Some(&self.geometry) {
            return Err(invalid(
                "endpoint geometry differs from its retained conditioning context",
            ));
        }
        Ok(())
    }
}

fn sampled_contract(plan: &BridgeGenerationPlan) -> Result<VideoContract, QualificationError> {
    let dimensions = plan.native_dimensions();
    let frames = u32::try_from(plan.project_frames().frames()).map_err(invalid)?;
    if !(1..=MAX_SAMPLED_FRAMES).contains(&frames) {
        return Err(invalid(
            "sampled frame count exceeds the endpoint inspection budget",
        ));
    }
    Ok(VideoContract {
        width: dimensions.width(),
        height: dimensions.height(),
        frames,
        rate_num: plan.project_frame_rate().numerator(),
        rate_den: plan.project_frame_rate().denominator(),
    })
}

fn invalid(reason: impl ToString) -> QualificationError {
    QualificationError::Quality(reason.to_string())
}

#[cfg(test)]
pub(crate) fn test_report(
    plan: &BridgeGenerationPlan,
    sampled_object: &GeneratedObjectRef,
    conditioning: &ConditioningReceipt,
    geometry: ConditioningGeometry,
) -> BridgeEndpointReport {
    let sampled = sampled_contract(plan).unwrap();
    let observation = |sampled_frame| EndpointObservation {
        sampled_frame,
        sampled_pts: sampled.matroska_pts(sampled_frame).unwrap(),
        mean_absolute_rgb_difference: 0.0,
        gross_cell_fraction: 0.0,
    };
    BridgeEndpointReport {
        schema_version: 1,
        profile: PROFILE.into(),
        sampled,
        sampled_object: sampled_object.clone(),
        context_object: conditioning.manifest().object().clone(),
        left_object: conditioning.left().object().clone(),
        right_object: conditioning.right().object().clone(),
        geometry,
        thresholds: EndpointThresholds::policy(),
        entry: observation(0),
        exit: observation(sampled.frames - 1),
    }
}

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;
