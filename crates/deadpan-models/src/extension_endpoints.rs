//! Checks on the two joins of the exact sampled extension that enters the edit.
//! Context pictures remain inputs; only sampled output pictures are decoded here.

use deadpan_core::{ExtensionDirection, GeneratedObjectRef};
use deadpan_jobs::{ExtensionGenerationPlan, MotionAmount};
use deadpan_media::protocol::VideoContract;
use serde::{Deserialize, Serialize};

use crate::quality::{FrameObservation, ObservationClock, QualityThresholds, admit_observation};
use crate::{
    BoundaryPicture, EndpointObservation, EndpointThresholds, ExtensionConditioningReceipt,
    ExtensionContext, ExtensionOppositeSeam, QualificationError, RasterRect,
};

#[path = "extension_endpoints/decode.rs"]
mod decode;
pub(crate) use decode::measure;

const PROFILE: &str = "deadpan-extension-endpoints-1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionJoinRole {
    Conditioned,
    Unconditioned,
}

/// An absent opposite seam has no measurements. Low-texture present seams keep
/// their unavailable motion observation, independently of their other checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionEndpointJoin {
    Absent {},
    Measured {
        role: ExtensionJoinRole,
        picture: Box<BoundaryPicture>,
        object: GeneratedObjectRef,
        #[serde(deserialize_with = "required_option")]
        content: Option<RasterRect>,
        endpoint: EndpointObservation,
        /// `after_frame` identifies the sampled ordinal at this join. Entry
        /// compares the PNG to output; exit compares output to the PNG.
        #[serde(deserialize_with = "strict_observation")]
        quality: FrameObservation,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionEndpointReport {
    schema_version: u32,
    profile: String,
    plan: ExtensionGenerationPlan,
    sampled: VideoContract,
    sampled_object: GeneratedObjectRef,
    conditioning: ExtensionConditioningReceipt,
    presentation: RasterRect,
    motion: MotionAmount,
    thresholds: EndpointThresholds,
    quality_thresholds: QualityThresholds,
    entry: ExtensionEndpointJoin,
    exit: ExtensionEndpointJoin,
}

impl ExtensionEndpointReport {
    pub fn entry(&self) -> &ExtensionEndpointJoin {
        &self.entry
    }

    pub fn exit(&self) -> &ExtensionEndpointJoin {
        &self.exit
    }

    pub fn presentation(&self) -> RasterRect {
        self.presentation
    }

    pub fn thresholds(&self) -> EndpointThresholds {
        self.thresholds
    }

    pub fn quality_thresholds(&self) -> QualityThresholds {
        self.quality_thresholds
    }

    pub(crate) fn validate(
        &self,
        plan: &ExtensionGenerationPlan,
        sampled_object: &GeneratedObjectRef,
        context: &ExtensionContext,
        conditioning: &ExtensionConditioningReceipt,
        motion: MotionAmount,
    ) -> Result<(), QualificationError> {
        let sampled = sampled_contract(plan)?;
        validate_binding(plan, context, conditioning)?;
        if self.schema_version != 1
            || self.profile != PROFILE
            || &self.plan != plan
            || self.sampled != sampled
            || &self.sampled_object != sampled_object
            || &self.conditioning != conditioning
            || self.presentation != context.presentation()
            || self.motion != motion
            || self.thresholds != EndpointThresholds::policy()
            || self.quality_thresholds != QualityThresholds::for_motion(motion)
        {
            return Err(invalid(
                "extension endpoint report differs from its inputs, plan or policy",
            ));
        }
        for (entry, join, ordinal) in [
            (true, &self.entry, 0),
            (false, &self.exit, sampled.frames - 1),
        ] {
            let expected = expected_join(context, conditioning, entry)?;
            let Some(expected) = expected else {
                if !matches!(join, ExtensionEndpointJoin::Absent {}) {
                    return Err(invalid("absent extension seam has invented measurements"));
                }
                continue;
            };
            let ExtensionEndpointJoin::Measured {
                role,
                picture,
                object,
                content,
                endpoint,
                quality,
            } = join
            else {
                return Err(invalid(
                    "present extension seam is missing its measurements",
                ));
            };
            if *role != expected.role
                || picture.as_ref() != expected.picture
                || object != expected.object
                || *content != expected.content
                || endpoint.sampled_frame != ordinal
                || endpoint.sampled_pts != sampled.matroska_pts(ordinal).map_err(invalid)?
                || quality.after_frame != ordinal
                || !quality.validate()
            {
                return Err(invalid(
                    "extension join differs from its retained picture or sampled clock",
                ));
            }
            let name = if entry { "entry" } else { "exit" };
            validate_gross(endpoint, self.thresholds, &format!("{role:?} {name}"))?;
            admit_observation(
                quality,
                self.quality_thresholds,
                f64::from(sampled.rate_den) / f64::from(sampled.rate_num),
                motion,
                &format!("{PROFILE} {role:?} {name}"),
                ObservationClock::Sampled,
            )?;
        }
        Ok(())
    }
}

fn validate_binding(
    plan: &ExtensionGenerationPlan,
    context: &ExtensionContext,
    receipt: &ExtensionConditioningReceipt,
) -> Result<(), QualificationError> {
    if context.plan() != plan
        || context.context().len() != receipt.context().len()
        || context
            .context()
            .iter()
            .zip(receipt.context())
            .any(|(picture, retained)| &picture.frame != retained.declaration())
        || context.continuity().signatures() != receipt.signatures().declaration()
    {
        return Err(invalid(
            "extension endpoint context differs from its plan or retained receipt",
        ));
    }
    match (context.opposite(), receipt.opposite()) {
        (ExtensionOppositeSeam::Absent, None) => Ok(()),
        (ExtensionOppositeSeam::PresentUnconditioned { frame, .. }, Some(retained))
            if frame == retained.declaration() =>
        {
            Ok(())
        }
        _ => Err(invalid(
            "extension opposite seam differs from its retained receipt",
        )),
    }
}

struct ExpectedJoin<'a> {
    role: ExtensionJoinRole,
    picture: &'a BoundaryPicture,
    object: &'a GeneratedObjectRef,
    content: Option<RasterRect>,
}

fn is_conditioned(plan: &ExtensionGenerationPlan, entry: bool) -> bool {
    entry == (plan.direction() == ExtensionDirection::FromLeft)
}

fn expected_join<'a>(
    context: &'a ExtensionContext,
    receipt: &'a ExtensionConditioningReceipt,
    entry: bool,
) -> Result<Option<ExpectedJoin<'a>>, QualificationError> {
    if is_conditioned(context.plan(), entry) {
        let retained = match context.plan().direction() {
            ExtensionDirection::FromLeft => receipt.context().last(),
            ExtensionDirection::FromRight => receipt.context().first(),
        }
        .ok_or_else(|| invalid("extension anchor receipt is missing"))?;
        Ok(Some(ExpectedJoin {
            role: ExtensionJoinRole::Conditioned,
            picture: &context.anchor().picture,
            object: retained.object(),
            content: context.anchor().content,
        }))
    } else {
        match (context.opposite(), receipt.opposite()) {
            (ExtensionOppositeSeam::Absent, None) => Ok(None),
            (
                ExtensionOppositeSeam::PresentUnconditioned {
                    picture, content, ..
                },
                Some(retained),
            ) => Ok(Some(ExpectedJoin {
                role: ExtensionJoinRole::Unconditioned,
                picture,
                object: retained.object(),
                content: *content,
            })),
            _ => Err(invalid("extension opposite seam receipt is missing")),
        }
    }
}

fn validate_gross(
    observation: &EndpointObservation,
    thresholds: EndpointThresholds,
    name: &str,
) -> Result<(), QualificationError> {
    if !(0.0..=255.0).contains(&observation.mean_absolute_rgb_difference)
        || !(0.0..=1.0).contains(&observation.gross_cell_fraction)
        || observation.mean_absolute_rgb_difference + 1e-9
            < observation.gross_cell_fraction * thresholds.gross_cell_difference
        || observation.mean_absolute_rgb_difference
            > observation.gross_cell_fraction * 255.0
                + (1.0 - observation.gross_cell_fraction) * thresholds.gross_cell_difference
                + 1e-9
    {
        return Err(invalid(
            "extension gross endpoint observation is inconsistent",
        ));
    }
    if observation.mean_absolute_rgb_difference >= thresholds.gross_mean_difference
        && observation.gross_cell_fraction >= thresholds.minimum_gross_fraction
    {
        return Err(invalid(format!(
            "{PROFILE}: gross {name} discontinuity at sampled frame {}: mean RGB difference {:.3}/255, gross coverage {:.3}; limits {:.3}/{:.3}",
            observation.sampled_frame,
            observation.mean_absolute_rgb_difference,
            observation.gross_cell_fraction,
            thresholds.gross_mean_difference,
            thresholds.minimum_gross_fraction,
        )));
    }
    Ok(())
}

pub(crate) fn sampled_contract(
    plan: &ExtensionGenerationPlan,
) -> Result<VideoContract, QualificationError> {
    let dimensions = plan.native_dimensions();
    let frames = u32::try_from(plan.project_frames().frames()).map_err(invalid)?;
    if !(1..=crate::endpoints::MAX_SAMPLED_FRAMES).contains(&frames) {
        return Err(invalid(
            "sampled extension exceeds the endpoint inspection budget",
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

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn strict_observation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<FrameObservation, D::Error> {
    crate::extension_motion::ObservationWire::deserialize(deserializer).map(Into::into)
}

fn invalid(reason: impl ToString) -> QualificationError {
    QualificationError::Quality(reason.to_string())
}

#[cfg(test)]
#[path = "extension_endpoint_tests.rs"]
mod tests;
