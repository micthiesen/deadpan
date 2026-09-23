//! Authored exceptions to automatic fades at exact structural audio boundaries.

use serde::{Deserialize, Serialize};

use crate::{DocumentError, DocumentErrorCode, NodeKind};

/// The side of the original constraint, even when that constraint borders a
/// silent span on the opposite side. Repeat gap settings apply to every gap of
/// this authored Repeat; they are not an individual gap override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBoundaryKind {
    NodeStart,
    NodeEnd,
    SourcePlacementStart,
    SourcePlacementEnd,
    RepeatGapStart,
    RepeatGapEnd,
}

impl AudioBoundaryKind {
    pub fn supports(self, kind: &NodeKind) -> bool {
        match self {
            Self::NodeStart | Self::NodeEnd => true,
            Self::SourcePlacementStart | Self::SourcePlacementEnd => {
                matches!(kind, NodeKind::Source { .. })
            }
            Self::RepeatGapStart | Self::RepeatGapEnd => {
                matches!(kind, NodeKind::Repeat { .. })
            }
        }
    }
}

/// Automatic uses the shared short fade. Hard is an explicit creative exception.
/// Any Hard constraint at an exactly coincident boundary suppresses its fade;
/// Automatic does not override another owner's explicit Hard exception.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioEdgePolicy {
    #[default]
    Automatic,
    Hard,
}

/// All six fields are required when a policy object is present. BeatNode omits
/// the whole object when every edge is Automatic, keeping legacy default
/// migration within the original JSON byte limits. An absent gap or selected
/// audio does not erase its owning node's stored choices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioEdgePolicies {
    pub node_start: AudioEdgePolicy,
    pub node_end: AudioEdgePolicy,
    pub source_placement_start: AudioEdgePolicy,
    pub source_placement_end: AudioEdgePolicy,
    pub repeat_gap_start: AudioEdgePolicy,
    pub repeat_gap_end: AudioEdgePolicy,
}

impl AudioEdgePolicies {
    pub(crate) fn is_automatic(&self) -> bool {
        *self == Self::default()
    }

    pub fn get(self, edge: AudioBoundaryKind) -> AudioEdgePolicy {
        match edge {
            AudioBoundaryKind::NodeStart => self.node_start,
            AudioBoundaryKind::NodeEnd => self.node_end,
            AudioBoundaryKind::SourcePlacementStart => self.source_placement_start,
            AudioBoundaryKind::SourcePlacementEnd => self.source_placement_end,
            AudioBoundaryKind::RepeatGapStart => self.repeat_gap_start,
            AudioBoundaryKind::RepeatGapEnd => self.repeat_gap_end,
        }
    }

    pub(crate) fn set(&mut self, edge: AudioBoundaryKind, policy: AudioEdgePolicy) {
        *match edge {
            AudioBoundaryKind::NodeStart => &mut self.node_start,
            AudioBoundaryKind::NodeEnd => &mut self.node_end,
            AudioBoundaryKind::SourcePlacementStart => &mut self.source_placement_start,
            AudioBoundaryKind::SourcePlacementEnd => &mut self.source_placement_end,
            AudioBoundaryKind::RepeatGapStart => &mut self.repeat_gap_start,
            AudioBoundaryKind::RepeatGapEnd => &mut self.repeat_gap_end,
        } = policy;
    }

    pub(crate) fn validate(self, kind: &NodeKind) -> Result<(), DocumentError> {
        for edge in [
            AudioBoundaryKind::SourcePlacementStart,
            AudioBoundaryKind::SourcePlacementEnd,
            AudioBoundaryKind::RepeatGapStart,
            AudioBoundaryKind::RepeatGapEnd,
        ] {
            if self.get(edge) != AudioEdgePolicy::Automatic && !edge.supports(kind) {
                return Err(DocumentError::new(
                    DocumentErrorCode::InvalidTree,
                    "audio edge policy does not belong to this node kind",
                ));
            }
        }
        Ok(())
    }
}
