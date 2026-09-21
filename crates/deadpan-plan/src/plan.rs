use std::collections::BTreeMap;

use deadpan_core::{
    AssetId, ExactRatio, FrameDuration, FrameRange, HoldRecipe, HoldVideo, InsertionBias,
    InstancePath, NodeId, NodeKind, PresentationBasis, ProjectDocument, ProjectFrame, ProjectId,
    RepeatInstance, RepeatLayout, RevisionId, SourceFrameId, SourcePoint, SourceTimeBase,
    SourceVideo, TimeError,
};
use serde::Serialize;

use crate::{Picture, PictureSample, PlanError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct StorageStats {
    pub authored_nodes: usize,
    pub sequence_prefix_entries: usize,
    pub iteration_run_entries: usize,
    pub repeat_segment_entries: usize,
    pub sparse_override_entries: usize,
    /// Sum of authored counts, not an allocation or an expanded occurrence count.
    pub referenced_plays: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct LookupStats {
    pub visited_nodes: usize,
    pub sequence_comparisons: usize,
    pub iteration_run_comparisons: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanMetadata {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub root: NodeId,
    pub presentation_basis: PresentationBasis,
    pub duration: FrameDuration,
    pub storage: StorageStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Source,
    Sequence,
    Hold,
    Repeat,
    Retime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeInspection {
    pub id: NodeId,
    pub label: String,
    pub kind: NodeType,
    pub duration: FrameDuration,
}

/// Deterministic node-ID order, independent of compilation traversal order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanInspection {
    pub metadata: PlanMetadata,
    pub nodes: Vec<NodeInspection>,
}

/// A validated, owned snapshot. There are no mutation methods or media handles.
/// Storage is O(authored nodes + child edges + compact iteration runs).
#[derive(Debug, Clone)]
pub struct RenderPlan {
    metadata: PlanMetadata,
    nodes: Vec<PlanNode>,
    by_id: BTreeMap<NodeId, usize>,
    root: usize,
}

#[derive(Debug, Clone)]
struct PlanNode {
    inspection: NodeInspection,
    kind: CompiledKind,
}

#[derive(Debug, Clone)]
enum CompiledKind {
    Source {
        video: SourceVideo,
    },
    Sequence {
        entries: Vec<SequenceEntry>,
    },
    Hold {
        video: CompiledHold,
    },
    Repeat {
        layout: RepeatLayout,
        gap: Option<CompiledHold>,
    },
    Retime {
        child: usize,
        start: ExactRatio,
        scale: ExactRatio,
    },
}

#[derive(Debug, Clone)]
struct SequenceEntry {
    child: usize,
    start: i64,
    end: i64,
}

#[derive(Debug, Clone)]
enum CompiledHold {
    Background,
    Freeze {
        asset: AssetId,
        point: SourcePoint,
    },
    Accepted {
        asset: AssetId,
        time_base: SourceTimeBase,
        frames: FrameRange,
    },
}

impl CompiledHold {
    fn compile(recipe: &HoldRecipe, document: &ProjectDocument) -> Result<Self, PlanError> {
        Ok(match &recipe.video {
            HoldVideo::Background => Self::Background,
            HoldVideo::Freeze { asset, timestamp } => Self::Freeze {
                asset: asset.clone(),
                point: SourcePoint {
                    ticks: ExactRatio::integer(timestamp.ticks),
                    time_base: timestamp.time_base,
                },
            },
            HoldVideo::Accepted { asset, frames } => Self::Accepted {
                asset: asset.clone(),
                time_base: document
                    .assets()
                    .get(asset)
                    .and_then(|record| record.video)
                    .ok_or(PlanError::InvalidPlan(
                        "accepted artifact has no video clock",
                    ))?
                    .start()
                    .time_base,
                frames: *frames,
            },
            HoldVideo::Generated { accepted } => Self::Accepted {
                asset: accepted.artifact.sampled_asset.clone(),
                time_base: document
                    .assets()
                    .get(&accepted.artifact.sampled_asset)
                    .and_then(|record| record.video)
                    .ok_or(PlanError::InvalidPlan(
                        "generated artifact has no video clock",
                    ))?
                    .start()
                    .time_base,
                // The master already materializes the retained bridge map.
                // Resizing selects its prefix, without resampling that map.
                frames: FrameRange::new(ProjectFrame(0), ProjectFrame(recipe.duration.frames()))?,
            },
        })
    }

    fn picture(&self, local: ExactRatio) -> Result<Picture, PlanError> {
        Ok(match self {
            Self::Background => Picture::Background,
            Self::Freeze { asset, point } => Picture::Freeze {
                asset: asset.clone(),
                point: *point,
            },
            Self::Accepted {
                asset,
                time_base,
                frames,
            } => {
                let position = ExactRatio::integer(frames.start().0).checked_add(local)?;
                if position.compare_integer(frames.start().0).is_lt()
                    || !position.compare_integer(frames.end().0).is_lt()
                {
                    return Err(PlanError::InvalidPlan(
                        "accepted picture exceeds authored artifact range",
                    ));
                }
                Picture::Accepted {
                    asset: asset.clone(),
                    time_base: *time_base,
                    position,
                    frame: SourceFrameId(
                        u64::try_from(position.floor()).map_err(|_| TimeError::Overflow)?,
                    ),
                }
            }
        })
    }
}

impl RenderPlan {
    pub fn compile(document: &ProjectDocument) -> Result<Self, PlanError> {
        let durations = document.durations()?;
        let by_id: BTreeMap<_, _> = document
            .nodes()
            .keys()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect();
        let mut storage = StorageStats {
            authored_nodes: by_id.len(),
            ..StorageStats::default()
        };
        let mut nodes = Vec::with_capacity(by_id.len());
        for (id, node) in document.nodes() {
            let duration = durations[id];
            let (kind, node_type) = match &node.kind {
                NodeKind::Source { source } => (
                    CompiledKind::Source {
                        video: source.video.clone(),
                    },
                    NodeType::Source,
                ),
                NodeKind::Sequence { children } => {
                    let mut end = FrameDuration::ZERO;
                    let mut entries = Vec::with_capacity(children.len());
                    for child in children {
                        let start = end.frames();
                        end = end.checked_add(durations[child])?;
                        entries.push(SequenceEntry {
                            child: by_id[child],
                            start,
                            end: end.frames(),
                        });
                    }
                    storage.sequence_prefix_entries += entries.len();
                    (CompiledKind::Sequence { entries }, NodeType::Sequence)
                }
                NodeKind::Hold { recipe } => (
                    CompiledKind::Hold {
                        video: CompiledHold::compile(recipe, document)?,
                    },
                    NodeType::Hold,
                ),
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                } => {
                    let overrides = document.overrides().get(id);
                    let layout = RepeatLayout::compile(
                        iterations,
                        child,
                        overrides,
                        gap.as_ref()
                            .map_or(FrameDuration::ZERO, |recipe| recipe.duration),
                        &durations,
                    )?;
                    storage.iteration_run_entries += iterations.segment_count();
                    storage.repeat_segment_entries += layout.segment_count();
                    storage.sparse_override_entries += overrides.map_or(0, |entries| entries.len());
                    storage.referenced_plays += u64::from(iterations.len());
                    (
                        CompiledKind::Repeat {
                            layout,
                            gap: gap
                                .as_ref()
                                .map(|recipe| CompiledHold::compile(recipe, document))
                                .transpose()?,
                        },
                        NodeType::Repeat,
                    )
                }
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    ..
                } => (
                    CompiledKind::Retime {
                        child: by_id[child],
                        start: ExactRatio::integer(mapping.start().0),
                        scale: ExactRatio::new(
                            i128::from(mapping.duration().frames()),
                            i128::from(duration.frames()),
                        )?,
                    },
                    NodeType::Retime,
                ),
            };
            nodes.push(PlanNode {
                inspection: NodeInspection {
                    id: id.clone(),
                    label: node.label.clone(),
                    kind: node_type,
                    duration,
                },
                kind,
            });
        }
        let root = by_id[document.root()];
        Ok(Self {
            metadata: PlanMetadata {
                project_id: document.project_id().clone(),
                revision_id: document.revision_id().clone(),
                root: document.root().clone(),
                presentation_basis: document.presentation_basis().clone(),
                duration: durations[document.root()],
                storage,
            },
            nodes,
            by_id,
            root,
        })
    }

    pub fn metadata(&self) -> &PlanMetadata {
        &self.metadata
    }

    pub fn duration(&self) -> FrameDuration {
        self.metadata.duration
    }

    pub fn node_duration(&self, id: &NodeId) -> Option<FrameDuration> {
        self.by_id
            .get(id)
            .map(|index| self.nodes[*index].inspection.duration)
    }

    pub fn inspect(&self) -> PlanInspection {
        PlanInspection {
            metadata: self.metadata.clone(),
            nodes: self
                .nodes
                .iter()
                .map(|node| node.inspection.clone())
                .collect(),
        }
    }

    /// Sample the center of a half-open project frame. Structural descent costs
    /// O(depth * log(max(children, runs + overrides))); repeat counts do not affect storage.
    /// Arithmetic overflow fails explicitly instead of rounding an intermediate.
    pub fn picture(&self, frame: ProjectFrame) -> Result<PictureSample, PlanError> {
        if frame.0 < 0 || frame.0 >= self.duration().frames() {
            return Err(PlanError::FrameOutOfRange {
                frame,
                duration: self.duration(),
            });
        }
        let mut local = ExactRatio::new(i128::from(frame.0) * 2 + 1, 2)?;
        let mut current = self.root;
        let mut repeats = Vec::new();
        let mut lookup = LookupStats::default();
        let (picture, gap_after) = loop {
            let node = &self.nodes[current];
            lookup.visited_nodes += 1;
            if local.compare_integer(0).is_lt()
                || !local
                    .compare_integer(node.inspection.duration.frames())
                    .is_lt()
            {
                return Err(PlanError::InvalidPlan(
                    "local picture coordinate exceeds node duration",
                ));
            }
            match &node.kind {
                CompiledKind::Source { video } => {
                    let picture = match video {
                        SourceVideo::Stream { asset, span } => {
                            let scale = ExactRatio::new(
                                i128::from(span.end().ticks) - i128::from(span.start().ticks),
                                i128::from(node.inspection.duration.frames()),
                            )?;
                            Picture::Source {
                                asset: asset.clone(),
                                point: SourcePoint {
                                    ticks: ExactRatio::integer(span.start().ticks)
                                        .checked_add(local.checked_mul(scale)?)?,
                                    time_base: span.start().time_base,
                                },
                            }
                        }
                        SourceVideo::Still { asset } => Picture::Still {
                            asset: asset.clone(),
                        },
                        SourceVideo::Blank => Picture::Blank,
                    };
                    break (picture, None);
                }
                CompiledKind::Hold { video } => break (video.picture(local)?, None),
                CompiledKind::Sequence { entries } => {
                    let index = upper_bound(
                        entries.len(),
                        |index| !local.compare_integer(entries[index].end).is_lt(),
                        &mut lookup.sequence_comparisons,
                    );
                    let entry = entries
                        .get(index)
                        .ok_or(PlanError::InvalidPlan("sequence prefix index has no child"))?;
                    local = local.checked_sub(ExactRatio::integer(entry.start))?;
                    current = entry.child;
                }
                CompiledKind::Retime {
                    child,
                    start,
                    scale,
                } => {
                    local = start.checked_add(local.checked_mul(*scale)?)?;
                    current = *child;
                }
                CompiledKind::Repeat { layout, gap } => {
                    let location = layout.locate(local, InsertionBias::Right)?;
                    lookup.iteration_run_comparisons += location.comparisons;
                    local = location.position;
                    if location.in_gap {
                        let gap = gap
                            .as_ref()
                            .ok_or(PlanError::InvalidPlan("repeat gap recipe is missing"))?;
                        break (gap.picture(local)?, Some(location.play.iteration));
                    }
                    repeats.push(RepeatInstance {
                        node: node.inspection.id.clone(),
                        iteration: location.play.iteration,
                    });
                    current = self.by_id[&location.play.child];
                }
            }
        };
        Ok(PictureSample {
            project_id: self.metadata.project_id.clone(),
            revision_id: self.metadata.revision_id.clone(),
            project_frame: frame,
            instance: InstancePath {
                node: self.nodes[current].inspection.id.clone(),
                repeats,
            },
            gap_after,
            local_position: local,
            picture,
            lookup,
        })
    }
}

/// First element for which a monotonic predicate is false, with measured work.
fn upper_bound(
    length: usize,
    mut preceding: impl FnMut(usize) -> bool,
    comparisons: &mut usize,
) -> usize {
    let mut left = 0;
    let mut right = length;
    while left < right {
        let middle = left + (right - left) / 2;
        *comparisons += 1;
        if preceding(middle) {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    left
}
