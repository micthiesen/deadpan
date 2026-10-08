//! Measured ordinal support for the canonical affine picture spans.

use deadpan_core::{
    EndpointPolicy, ExactRatio, FrameDuration, SourceFrameId, SourceFrameIndex, TimeError,
};

use super::{DefinitionPictureSpan, PictureClockSlope};
use crate::{Picture, PlanError};

impl DefinitionPictureSpan {
    /// First and last measured ordinals touched, in playback order. The right
    /// endpoint is excluded; a zero-length terminal observes its exact picture.
    /// Reverse source motion approaches its right endpoint from above. These
    /// facts do not assert that the span came from a particular immutable plan.
    pub fn source_ordinals(
        &self,
        index: &SourceFrameIndex,
    ) -> Result<(SourceFrameId, SourceFrameId), PlanError> {
        self.clock
            .source_ordinals(&self.start.picture, self.distance()?, index)
    }

    /// Resolve a sampled accepted master using its retained exact output count,
    /// without creating an index or interpreting its native/context pictures.
    pub fn accepted_ordinals(
        &self,
        output_frames: FrameDuration,
    ) -> Result<(SourceFrameId, SourceFrameId), PlanError> {
        self.clock
            .accepted_ordinals(&self.start.picture, self.distance()?, output_frames)
    }

    fn distance(&self) -> Result<ExactRatio, PlanError> {
        if self.start.position.compare_integer(0).is_lt() {
            return Err(PlanError::InvalidPlan(
                "picture span starts before its definition",
            ));
        }
        let distance = self.end_exclusive.checked_sub(self.start.position)?;
        valid_distance(distance)?;
        Ok(distance)
    }
}

impl PictureClockSlope {
    /// The same selection as `DefinitionPictureSpan::source_ordinals`, accepting
    /// the cheap raw descriptor separately so a host can rebind an authored
    /// Original alias to its qualified index without cloning the index or span.
    /// The picture asset and time base must match that index exactly.
    pub fn source_ordinals(
        self,
        picture: &Picture,
        distance: ExactRatio,
        index: &SourceFrameIndex,
    ) -> Result<(SourceFrameId, SourceFrameId), PlanError> {
        valid_distance(distance)?;
        let first = picture.select_source_frame(index)?.identity;
        if let Picture::Accepted { generated, .. } = picture {
            let frames = FrameDuration::new(
                i64::try_from(index.frames().len()).map_err(|_| TimeError::Overflow)?,
            )?;
            if generated
                .as_ref()
                .is_some_and(|artifact| artifact.sampling.output_frame_count() != frames)
            {
                return Err(PlanError::InvalidPlan(
                    "accepted sampled count differs from its measured index",
                ));
            }
            return self.accepted_ordinals(picture, distance, frames);
        }
        let last = match (picture, self) {
            (Picture::Source { .. } | Picture::Freeze { .. }, Self::Constant) => first,
            (
                Picture::Source {
                    point,
                    selection,
                    endpoints,
                    ..
                },
                Self::SourceTicks(slope),
            ) => {
                let end = point.ticks.checked_add(distance.checked_mul(slope)?)?;
                if *endpoints == EndpointPolicy::Reject
                    && (end.compare(selection.start().ticks).is_lt()
                        || end.compare(selection.end().ticks).is_gt())
                {
                    return Err(PlanError::InvalidPlan(
                        "picture span exceeds its exact source selection",
                    ));
                }
                let mut endpoint = picture.clone();
                let Picture::Source {
                    point, endpoints, ..
                } = &mut endpoint
                else {
                    unreachable!()
                };
                point.ticks = end;
                // The span's actual start was validated above under its own
                // policy. Holding here resolves the limiting picture at an
                // excluded selected-span endpoint, without admitting a fetch
                // outside a Reject span.
                *endpoints = EndpointPolicy::HoldAdjacent;
                let selected = endpoint.select_source_frame(index)?;
                if slope.compare_integer(0).is_gt()
                    && distance.compare_integer(0).is_gt()
                    && end.compare_integer(selected.pts).is_eq()
                    && selected.identity.0 > first.0
                {
                    SourceFrameId(selected.identity.0 - 1)
                } else {
                    selected.identity
                }
            }
            _ => {
                return Err(PlanError::InvalidPlan(
                    "picture span clock disagrees with its provider",
                ));
            }
        };
        Ok((first, last))
    }

    /// Canonical Accepted providers advance by a nonnegative frame-coordinate
    /// slope. Validate the redundant frame field and every touched ordinal
    /// against the retained sampled-output extent, never the native extent.
    pub fn accepted_ordinals(
        self,
        picture: &Picture,
        distance: ExactRatio,
        output_frames: FrameDuration,
    ) -> Result<(SourceFrameId, SourceFrameId), PlanError> {
        valid_distance(distance)?;
        let Picture::Accepted {
            position, frame, ..
        } = picture
        else {
            return Err(PlanError::InvalidPlan(
                "accepted support requires an Accepted picture",
            ));
        };
        let slope = match self {
            Self::Constant => ExactRatio::ZERO,
            Self::AcceptedFrames(slope) if !slope.compare_integer(0).is_lt() => slope,
            _ => {
                return Err(PlanError::InvalidPlan(
                    "accepted picture span has a noncanonical clock",
                ));
            }
        };
        if position.compare_integer(0).is_lt()
            || !position.compare_integer(output_frames.frames()).is_lt()
            || u64::try_from(position.floor()).ok() != Some(frame.0)
        {
            return Err(PlanError::InvalidPlan(
                "accepted picture disagrees with its sampled extent or ordinal",
            ));
        }
        let end = position.checked_add(distance.checked_mul(slope)?)?;
        let last = if slope.compare_integer(0).is_gt() && distance.compare_integer(0).is_gt() {
            end.ceil()?.checked_sub(1).ok_or(TimeError::Overflow)?
        } else {
            end.floor()
        };
        let last = u64::try_from(last).map_err(|_| TimeError::Overflow)?;
        if last >= u64::try_from(output_frames.frames()).map_err(|_| TimeError::Overflow)? {
            return Err(PlanError::MissingSourceFrame {
                frame: SourceFrameId(last),
            });
        }
        Ok((*frame, SourceFrameId(last)))
    }
}

fn valid_distance(distance: ExactRatio) -> Result<(), PlanError> {
    if distance.compare_integer(0).is_lt() {
        return Err(PlanError::InvalidPlan(
            "picture span runs backwards in its definition",
        ));
    }
    Ok(())
}
