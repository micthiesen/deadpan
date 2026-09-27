//! Project-to-content boundary descent. Keep every authored owner and its local
//! clock, unlike mark relocation, which deliberately ignores grouping scopes.

use super::{AnchorError, AnchorErrorCode, AnchorIndex, InsertionBias, SequenceChild};
use crate::{
    DocumentError, DocumentErrorCode, ExactRatio, FrameDuration, InstancePath, IterationId,
    MAX_DOCUMENT_DEPTH, NodeKind, ProjectId, RepeatInstance, RevisionId,
};
use serde::{Deserialize, Serialize};

/// An exact project boundary, not a sampled picture center. Bias selects the
/// content before or after a seam. No rounding occurs during descent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryLocationRequest {
    pub project_id: ProjectId,
    pub expected_revision: RevisionId,
    pub position: ExactRatio,
    pub bias: InsertionBias,
}

/// Per-query work bounds, after construction of the immutable AnchorIndex.
/// Zero permits no work of that kind. Counts cover emitted scopes and binary
/// Sequence/Repeat prefix comparisons, not document validation or map lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryQueryLimits {
    pub max_scopes: usize,
    pub max_comparisons: usize,
}

impl Default for BoundaryQueryLimits {
    fn default() -> Self {
        Self {
            max_scopes: MAX_DOCUMENT_DEPTH + 1,
            max_comparisons: 8192,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundaryScope {
    pub instance: InstancePath,
    pub entry: BoundaryEntry,
    /// Boundary in this owner's full output clock, including any retained
    /// domain hidden by an ancestor crop. This is never a frame-center clock.
    pub position: ExactRatio,
    pub duration: FrameDuration,
}

/// Structural edge from the preceding scope. Repeat identities are retained
/// in `instance.repeats`; a gap is never mistaken for its preceding play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoundaryEntry {
    Root,
    Sequence { index: usize },
    Retime,
    RepeatPlay,
    RepeatGap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoundaryTerminal {
    /// The last scope is a Source or Hold, including an explicitly owned gap.
    Node,
    /// An implicit Repeat gap has no authored node. The last scope names its
    /// Repeat occurrence; `after` belongs to that Repeat, not to its ancestors.
    Gap {
        after: IterationId,
        position: ExactRatio,
        duration: FrameDuration,
    },
    /// Outward bias has no neighboring content. Only the root scope is emitted.
    ProjectStart,
    ProjectEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundaryLocation {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub position: ExactRatio,
    pub bias: InsertionBias,
    /// Root first, provider last. Sequence, Repeat and Retime owners remain
    /// present even when transparent or unframed. Paths name stable plays.
    pub scopes: Vec<BoundaryScope>,
    pub terminal: BoundaryTerminal,
    pub comparisons: usize,
}

impl AnchorIndex<'_> {
    /// Resolve an exact boundary through compact structure. This reports
    /// content ownership only; it neither selects an insertion parent nor
    /// isolates a play, mutates a document, or allocates any authored identity.
    pub fn locate_boundary(
        &self,
        request: &BoundaryLocationRequest,
        limits: BoundaryQueryLimits,
    ) -> Result<BoundaryLocation, AnchorError> {
        self.check_revision(&request.project_id, &request.expected_revision)?;
        let mut node = self.document.root();
        let mut position = request.position;
        let root_duration = self.durations[node];
        super::within(
            position,
            root_duration.frames(),
            AnchorErrorCode::OutOfRange,
        )?;
        let mut scopes = Vec::new();
        let mut repeats = Vec::new();
        let mut comparisons = 0;
        let mut entry = BoundaryEntry::Root;
        let terminal = loop {
            if scopes.len() == limits.max_scopes {
                return Err(limit("boundary scope budget exhausted"));
            }
            let duration = self.durations[node];
            scopes.push(BoundaryScope {
                instance: InstancePath {
                    node: node.clone(),
                    repeats: repeats.clone(),
                },
                entry,
                position,
                duration,
            });
            if node == self.document.root() {
                if duration == FrameDuration::ZERO {
                    break match request.bias {
                        InsertionBias::Left => BoundaryTerminal::ProjectStart,
                        InsertionBias::Right => BoundaryTerminal::ProjectEnd,
                    };
                }
                if position == ExactRatio::ZERO && request.bias == InsertionBias::Left {
                    break BoundaryTerminal::ProjectStart;
                }
                if position.compare_integer(duration.frames()).is_eq()
                    && request.bias == InsertionBias::Right
                {
                    break BoundaryTerminal::ProjectEnd;
                }
            }
            match &self.document.nodes()[node].kind {
                NodeKind::Source { .. } | NodeKind::Hold { .. } => break BoundaryTerminal::Node,
                NodeKind::Sequence { .. } => {
                    let child = sequence_child(
                        &self.sequences[node],
                        position,
                        request.bias,
                        &mut comparisons,
                        limits.max_comparisons,
                    )?;
                    position =
                        position.checked_sub(ExactRatio::integer(self.parents[&child.node].1))?;
                    entry = BoundaryEntry::Sequence { index: child.index };
                    node = &child.node;
                }
                NodeKind::Retime {
                    child,
                    mapping,
                    duration,
                    ..
                } => {
                    position = position
                        .checked_mul(ExactRatio::new(
                            i128::from(mapping.duration().frames()),
                            i128::from(duration.frames()),
                        )?)?
                        .checked_add(ExactRatio::integer(mapping.start().0))?;
                    node = child;
                    entry = BoundaryEntry::Retime;
                }
                NodeKind::Repeat { .. } => {
                    let location = self.repeats[node]
                        .locate_bounded(
                            position,
                            request.bias,
                            limits.max_comparisons - comparisons,
                        )
                        .map_err(layout_error)?;
                    comparisons += location.comparisons;
                    position = location.position;
                    if location.in_gap && location.play.gap_child.is_none() {
                        break BoundaryTerminal::Gap {
                            after: location.play.iteration,
                            position,
                            duration: location.play.gap_after,
                        };
                    }
                    repeats.push(RepeatInstance {
                        node: node.clone(),
                        iteration: location.play.iteration,
                    });
                    let child = if location.in_gap {
                        entry = BoundaryEntry::RepeatGap;
                        location
                            .play
                            .gap_child
                            .as_ref()
                            .ok_or_else(|| invalid("selected Repeat gap has no owned branch"))?
                    } else {
                        entry = BoundaryEntry::RepeatPlay;
                        &location.play.child
                    };
                    node = self
                        .document
                        .nodes()
                        .get_key_value(child)
                        .ok_or_else(|| invalid("selected Repeat branch is missing"))?
                        .0;
                }
            }
        };
        Ok(BoundaryLocation {
            project_id: request.project_id.clone(),
            revision_id: request.expected_revision.clone(),
            position: request.position,
            bias: request.bias,
            scopes,
            terminal,
            comparisons,
        })
    }
}

fn sequence_child<'a>(
    entries: &'a [SequenceChild],
    position: ExactRatio,
    bias: InsertionBias,
    comparisons: &mut usize,
    maximum: usize,
) -> Result<&'a SequenceChild, AnchorError> {
    let mut start = 0;
    let mut end = entries.len();
    while start < end {
        if *comparisons == maximum {
            return Err(limit("boundary comparison budget exhausted"));
        }
        let middle = start + (end - start) / 2;
        *comparisons += 1;
        let order = position.compare_integer(entries[middle].end);
        let preceding = match bias {
            InsertionBias::Left => order.is_gt(),
            InsertionBias::Right => !order.is_lt(),
        };
        if preceding {
            start = middle + 1;
        } else {
            end = middle;
        }
    }
    entries
        .get(start)
        .ok_or_else(|| invalid("Sequence boundary has no neighboring content"))
}

fn layout_error(error: DocumentError) -> AnchorError {
    let code = match error.code {
        DocumentErrorCode::LimitExceeded => AnchorErrorCode::QueryLimit,
        DocumentErrorCode::TimingOverflow => AnchorErrorCode::TimingOverflow,
        _ => AnchorErrorCode::InvalidDocument,
    };
    AnchorError::new(code, error.to_string())
}

fn invalid(message: &str) -> AnchorError {
    AnchorError::new(AnchorErrorCode::InvalidDocument, message)
}

fn limit(message: &str) -> AnchorError {
    AnchorError::new(AnchorErrorCode::QueryLimit, message)
}
