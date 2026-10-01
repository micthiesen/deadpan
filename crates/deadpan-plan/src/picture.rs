use deadpan_core::{
    AssetId, CapturedFraming, EndpointPolicy, ExactRatio, ExactSourceSpan, FrameDuration,
    FramingPose, GeneratedArtifact, IndexedSourceFrame, InstancePath, IterationId, ProjectFrame,
    ProjectId, RevisionId, SourceFrameId, SourceFrameIndex, SourcePoint, SourceSpan,
    SourceTimeBase,
};
use serde::{Serialize, Serializer};
use std::sync::Arc;

fn serialize_shared<T: Serialize, S>(
    context: &Option<Arc<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    context.as_deref().serialize(serializer)
}

use crate::{LookupStats, PlanError};

/// A picture request against original media identities. Decoder/proxy selection
/// remains a separate explicit operation; no rounded proxy coordinate appears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Picture {
    Source {
        asset: AssetId,
        point: SourcePoint,
        /// Full affine source context retained independently of its selection.
        span: SourceSpan,
        /// Exact selected picture interval. Endpoint holding never exposes
        /// otherwise available pictures outside this half-open window.
        selection: ExactSourceSpan,
        endpoints: EndpointPolicy,
    },
    Still {
        asset: AssetId,
    },
    Blank,
    Background,
    Freeze {
        asset: AssetId,
        point: SourcePoint,
    },
    Accepted {
        asset: AssetId,
        /// Durable identity from this exact effective Hold/gap recipe. Legacy
        /// Accepted providers have no such authority. Kept shared across seeks.
        #[serde(
            skip_serializing_if = "Option::is_none",
            serialize_with = "serialize_shared"
        )]
        generated: Option<Arc<GeneratedArtifact>>,
        time_base: SourceTimeBase,
        /// Exact original presentation-frame coordinate, after all retimes.
        position: ExactRatio,
        /// Half-open frame selection floors only at the original-media boundary.
        frame: SourceFrameId,
    },
}

impl Picture {
    /// Select an original presentation frame using a measured source index.
    /// Asset and exact timestamp clock must match. Sources retain their authored
    /// span and endpoint policy; Freeze points must be inside the measured index.
    /// Accepted artifacts address original presentation ordinals directly and
    /// never apply an endpoint fallback to a missing accepted frame.
    pub fn select_source_frame<'a>(
        &self,
        index: &'a SourceFrameIndex,
    ) -> Result<&'a IndexedSourceFrame, PlanError> {
        let (asset, time_base) = match self {
            Self::Source { asset, point, .. } | Self::Freeze { asset, point } => {
                (asset, point.time_base)
            }
            Self::Accepted {
                asset, time_base, ..
            } => (asset, *time_base),
            Self::Still { .. } | Self::Blank | Self::Background => {
                return Err(PlanError::NoSourceFrame);
            }
        };
        if asset != index.asset() {
            return Err(PlanError::IndexAssetMismatch {
                expected: asset.clone(),
                actual: index.asset().clone(),
            });
        }
        if time_base != index.time_base() {
            return Err(PlanError::IndexClockMismatch {
                expected: time_base,
                actual: index.time_base(),
            });
        }
        match self {
            Self::Source {
                point,
                span,
                selection,
                endpoints,
                ..
            } => {
                // A smaller visible window cannot admit an incomplete index
                // for the retained affine context. It must also belong to that
                // context, even when a caller constructs a Picture directly.
                index.select_in_span(selection.start(), *span, EndpointPolicy::Reject)?;
                if selection
                    .end()
                    .ticks
                    .compare_integer(span.end().ticks)
                    .is_gt()
                {
                    return Err(PlanError::InvalidPlan(
                        "picture selection exceeds its source context",
                    ));
                }
                Ok(index.select_in_exact_span(*point, *selection, *endpoints)?)
            }
            Self::Freeze { point, .. } => Ok(index.select(*point, EndpointPolicy::Reject)?),
            Self::Accepted { frame, .. } => usize::try_from(frame.0)
                .ok()
                .and_then(|number| index.frames().get(number))
                .ok_or(PlanError::MissingSourceFrame { frame: *frame }),
            _ => Err(PlanError::NoSourceFrame),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PictureFraming {
    /// The operation's exact scope. Identity scopes remain present so a preview
    /// can replace an operation at its original position in the composition.
    pub instance: InstancePath,
    pub local_position: ExactRatio,
    pub duration: FrameDuration,
    pub pose: Option<FramingPose>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PictureSample {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub project_frame: ProjectFrame,
    /// Ordinary pictures, including explicit gap branches, target the Source
    /// or Hold. A configured Repeat gap targets the Repeat with only ancestor
    /// repeats in the path.
    pub instance: InstancePath,
    /// A gap belongs to the preceding stable iteration, never to a shifting
    /// numeric play position. The last iteration has no following gap.
    pub gap_after: Option<IterationId>,
    /// Exact coordinate within the sampled Source, Hold, or default gap recipe.
    pub local_position: ExactRatio,
    pub picture: Picture,
    /// Geometry retained by an inserted Hold, independent of its provider.
    /// Arc keeps per-frame sampling from copying bounded authored context.
    #[serde(serialize_with = "serialize_shared")]
    pub picture_context: Option<Arc<CapturedFraming>>,
    /// Provider to root, including scopes without authored framing. A Repeat
    /// default gap has no provider node scope; consumers first apply an identity
    /// provider clip, then this list beginning with the Repeat's operation.
    /// An explicit gap branch retains its provider and ancestor scopes.
    pub framing: Vec<PictureFraming>,
    pub lookup: LookupStats,
}
