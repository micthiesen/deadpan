use deadpan_core::{
    AssetId, EndpointPolicy, ExactRatio, IndexedSourceFrame, InstancePath, IterationId,
    ProjectFrame, ProjectId, RevisionId, SourceFrameId, SourceFrameIndex, SourcePoint,
    SourceTimeBase,
};
use serde::Serialize;

use crate::{LookupStats, PlanError};

/// A picture request against original media identities. Decoder/proxy selection
/// remains a separate explicit operation; no rounded proxy coordinate appears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Picture {
    Source {
        asset: AssetId,
        point: SourcePoint,
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
        time_base: SourceTimeBase,
        /// Exact original presentation-frame coordinate, after all retimes.
        position: ExactRatio,
        /// Half-open frame selection floors only at the original-media boundary.
        frame: SourceFrameId,
    },
}

impl Picture {
    /// Select an original presentation frame using a measured source index.
    /// Asset and exact timestamp clock must match; endpoint holding is opt-in.
    /// Accepted artifacts address original presentation ordinals directly and
    /// never apply an endpoint fallback to a missing accepted frame.
    pub fn select_source_frame<'a>(
        &self,
        index: &'a SourceFrameIndex,
        endpoints: EndpointPolicy,
    ) -> Result<&'a IndexedSourceFrame, PlanError> {
        let (asset, time_base) = match self {
            Self::Source { asset, point } | Self::Freeze { asset, point } => {
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
            Self::Source { point, .. } | Self::Freeze { point, .. } => {
                Ok(index.select(*point, endpoints)?)
            }
            Self::Accepted { frame, .. } => usize::try_from(frame.0)
                .ok()
                .and_then(|number| index.frames().get(number))
                .ok_or(PlanError::MissingSourceFrame { frame: *frame }),
            _ => Err(PlanError::NoSourceFrame),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PictureSample {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub project_frame: ProjectFrame,
    /// For ordinary pictures this targets the Source or Hold. For a Repeat gap
    /// it targets the Repeat, with only its ancestor repeats in the path. Thus
    /// this remains a valid core InstancePath in either case.
    pub instance: InstancePath,
    /// A gap belongs to the preceding stable iteration, never to a shifting
    /// numeric play position. The last iteration has no following gap.
    pub gap_after: Option<IterationId>,
    /// Exact coordinate within the sampled Source, Hold, or gap recipe.
    pub local_position: ExactRatio,
    pub picture: Picture,
    pub lookup: LookupStats,
}
