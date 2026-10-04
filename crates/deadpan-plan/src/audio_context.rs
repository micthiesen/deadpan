//! Compile retained raw audio without inventing a picture-bearing document.
use std::collections::BTreeSet;

use deadpan_core::{
    ColorPolicy, FrozenAudioContext, FrozenAudioInput, FrozenAudioKind, FrozenAudioLayout,
    MAX_DOCUMENT_SOUNDS,
};

use super::*;

impl RenderPlan {
    /// Conservative structural work charge for compiling a frozen sound layout.
    /// Includes every node, authored child/override edge, compact Repeat segment,
    /// and selected asset validation/copy. It is independent of Repeat play count.
    pub fn sound_processing_layout_work(
        layout: &FrozenAudioLayout,
        assets: &BTreeSet<deadpan_core::AssetId>,
    ) -> Result<usize, PlanError> {
        layout.validate()?;
        let mut work = layout.nodes().len();
        for node in layout.nodes().values() {
            let child_work = match &node.kind {
                FrozenAudioKind::Sequence { children } => children.len(),
                FrozenAudioKind::Repeat { .. } | FrozenAudioKind::Retime { .. } => 1,
                FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. } => 0,
            };
            work = work.checked_add(child_work).ok_or(TimeError::Overflow)?;
            if let FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
                work = work
                    .checked_add(iterations.segment_count())
                    .ok_or(TimeError::Overflow)?;
            }
        }
        for overrides in layout
            .overrides()
            .values()
            .chain(layout.gap_overrides().values())
        {
            work = work
                .checked_add(overrides.len())
                .ok_or(TimeError::Overflow)?;
        }
        work.checked_add(assets.len())
            .ok_or(TimeError::Overflow.into())
    }

    /// Compile the retained structural clock for independent BeatSound recipes.
    /// Only explicitly selected, qualified sound assets are retained. Physical
    /// Source audio, Hold carriers, treatments, and current output policies are
    /// deliberately absent; the independent voice supplies the processing input.
    pub fn compile_sound_processing_layout(
        &self,
        layout: &FrozenAudioLayout,
        assets: &BTreeSet<deadpan_core::AssetId>,
    ) -> Result<Self, PlanError> {
        layout.validate()?;
        if layout.rate() != self.metadata.presentation_basis.frame_rate {
            return Err(PlanError::InvalidPlan(
                "sound processing layout has a foreign frame rate",
            ));
        }
        if assets.is_empty() || assets.len() > MAX_DOCUMENT_SOUNDS {
            return Err(PlanError::InvalidPlan(
                "sound processing layout needs a bounded nonempty asset set",
            ));
        }
        let mut audio_assets = BTreeMap::new();
        for asset_id in assets {
            let asset = self
                .audio_assets
                .get(asset_id)
                .ok_or(PlanError::InvalidPlan(
                    "sound processing asset is absent from the live catalog",
                ))?;
            if asset.audio.is_none() || asset.source_qualification.is_none() {
                return Err(PlanError::InvalidPlan(
                    "sound processing asset must be qualified audio",
                ));
            }
            audio_assets.insert(asset_id.clone(), asset.clone());
        }

        let by_id: BTreeMap<_, _> = layout
            .nodes()
            .keys()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect();
        let durations: BTreeMap<_, _> = layout
            .nodes()
            .iter()
            .map(|(id, node)| (id.clone(), node.duration))
            .collect();
        let mut storage = StorageStats {
            authored_nodes: by_id.len(),
            ..StorageStats::default()
        };
        let mut nodes = Vec::with_capacity(by_id.len());
        for (id, node) in layout.nodes() {
            let (kind, node_type) = match &node.kind {
                FrozenAudioKind::Source { .. } => (
                    CompiledKind::Source {
                        video: SourceVideo::Blank,
                        start: ExactRatio::ZERO,
                        duration: ExactRatio::integer(node.duration.frames()),
                        endpoints: EndpointPolicy::HoldAdjacent,
                        selection: None,
                        audio: None,
                    },
                    NodeType::Source,
                ),
                FrozenAudioKind::Sequence { children } => {
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
                FrozenAudioKind::Hold { .. } => (
                    CompiledKind::Hold {
                        video: CompiledHold::Background,
                        picture_context: None,
                        audio: HoldAudio::Silence,
                    },
                    NodeType::Hold,
                ),
                FrozenAudioKind::Repeat {
                    child,
                    iterations,
                    gap_duration,
                    ..
                } => {
                    let overrides = layout.overrides().get(id);
                    let gap_overrides = layout.gap_overrides().get(id);
                    let repeat = RepeatLayout::compile_with_gap_overrides(
                        iterations,
                        child,
                        overrides,
                        *gap_duration,
                        gap_overrides,
                        &durations,
                    )?;
                    storage.iteration_run_entries += iterations.segment_count();
                    storage.repeat_segment_entries += repeat.segment_count();
                    storage.sparse_override_entries += overrides.map_or(0, |entries| entries.len())
                        + gap_overrides.map_or(0, |entries| entries.len());
                    storage.referenced_plays += u64::from(iterations.len());
                    let gap = *gap_duration != FrameDuration::ZERO;
                    (
                        CompiledKind::Repeat {
                            default_child: by_id[child],
                            layout: repeat,
                            gap: gap.then_some(CompiledHold::Background),
                            gap_picture_context: None,
                            gap_duration: *gap_duration,
                            gap_audio: None,
                        },
                        NodeType::Repeat,
                    )
                }
                FrozenAudioKind::Retime {
                    child,
                    mapping,
                    pitch,
                    purpose,
                } => (
                    CompiledKind::Retime {
                        child: by_id[child],
                        start: ExactRatio::integer(mapping.start().0),
                        scale: ExactRatio::new(
                            i128::from(mapping.duration().frames()),
                            i128::from(node.duration.frames()),
                        )?,
                        pitch: *pitch,
                        purpose: *purpose,
                    },
                    NodeType::Retime,
                ),
            };
            nodes.push(PlanNode {
                inspection: NodeInspection {
                    id: id.clone(),
                    label: String::new(),
                    kind: node_type,
                    duration: node.duration,
                },
                kind,
                audio_edges: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_treatments: Default::default(),
                framing: None,
            });
        }

        let mut parents = vec![None; nodes.len()];
        for (id, node) in layout.nodes() {
            let parent = by_id[id];
            let children: &[deadpan_core::NodeId] = match &node.kind {
                FrozenAudioKind::Sequence { children } => children,
                FrozenAudioKind::Repeat { child, .. } | FrozenAudioKind::Retime { child, .. } => {
                    std::slice::from_ref(child)
                }
                FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. } => &[],
            };
            for child in children {
                parents[by_id[child]] = Some(parent);
            }
        }
        for (owner, overrides) in layout.overrides().iter().chain(layout.gap_overrides()) {
            let parent = by_id[owner];
            for (_, child) in overrides.iter() {
                parents[by_id[child]] = Some(parent);
            }
        }

        Ok(Self {
            metadata: PlanMetadata {
                project_id: self.metadata.project_id.clone(),
                revision_id: self.metadata.revision_id.clone(),
                root: layout.root().clone(),
                presentation_basis: self.metadata.presentation_basis.clone(),
                duration: layout.duration(),
                storage,
            },
            nodes,
            root: by_id[layout.root()],
            by_id,
            audio_assets,
            sounds: BTreeMap::new(),
            beat_sounds: BTreeMap::new(),
            sound_routes: BTreeMap::new(),
            sound_allowances: BTreeMap::new(),
            compiled_sounds: BTreeMap::new(),
            audio_context: true,
            has_audio_treatments: false,
            has_audio_editorial_edges: false,
            audio_bindings: Default::default(),
            parents,
        })
    }

    /// Compile the complete captured audio tree. The result retains the context's
    /// media contracts for explicit host admission and rejects picture evaluation.
    /// Captured revision names and serialized qualification IDs alone never admit
    /// a decoder. Sources without audio remain Sources with absent placement;
    /// they must not become explicit silent Holds after a Preserve stage.
    pub fn compile_audio_context(context: &FrozenAudioContext) -> Result<Self, PlanError> {
        context.validate()?;
        let layout = context.layout();
        let by_id: BTreeMap<_, _> = layout
            .nodes()
            .keys()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect();
        let durations: BTreeMap<_, _> = layout
            .nodes()
            .iter()
            .map(|(id, node)| (id.clone(), node.duration))
            .collect();
        let mut storage = StorageStats {
            authored_nodes: by_id.len(),
            ..StorageStats::default()
        };
        let mut nodes = Vec::with_capacity(by_id.len());
        for (id, node) in layout.nodes() {
            let (kind, node_type) = match &node.kind {
                FrozenAudioKind::Source { .. } => {
                    let audio = match context.inputs().get(id) {
                        Some(FrozenAudioInput::Source {
                            source,
                            mapping,
                            offset,
                        }) => Some(CompiledSourceAudio {
                            source: source.clone(),
                            start: mapping.start_frames_with_offset(*offset, layout.rate())?,
                            duration: mapping.duration_frames(node.duration)?,
                            selection: mapping.selection_frames_with_offset(
                                node.duration,
                                *offset,
                                layout.rate(),
                            )?,
                            selected: matches!(
                                mapping,
                                deadpan_core::SourceAudioMapping::SelectedPlacement { .. }
                            ),
                        }),
                        None => None,
                        _ => return Err(PlanError::InvalidPlan("invalid retained Source input")),
                    };
                    (
                        CompiledKind::Source {
                            video: SourceVideo::Blank,
                            start: ExactRatio::ZERO,
                            duration: ExactRatio::integer(node.duration.frames()),
                            endpoints: EndpointPolicy::HoldAdjacent,
                            selection: None,
                            audio,
                        },
                        NodeType::Source,
                    )
                }
                FrozenAudioKind::Sequence { children } => {
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
                FrozenAudioKind::Hold { audio } => (
                    CompiledKind::Hold {
                        video: CompiledHold::Background,
                        picture_context: None,
                        audio: retained_hold_audio(context, id, *audio)?,
                    },
                    NodeType::Hold,
                ),
                FrozenAudioKind::Repeat {
                    child,
                    iterations,
                    gap_duration,
                    gap_audio,
                } => {
                    let overrides = layout.overrides().get(id);
                    let gap_overrides = layout.gap_overrides().get(id);
                    let repeat = RepeatLayout::compile_with_gap_overrides(
                        iterations,
                        child,
                        overrides,
                        *gap_duration,
                        gap_overrides,
                        &durations,
                    )?;
                    storage.iteration_run_entries += iterations.segment_count();
                    storage.repeat_segment_entries += repeat.segment_count();
                    storage.sparse_override_entries += overrides.map_or(0, |entries| entries.len())
                        + gap_overrides.map_or(0, |entries| entries.len());
                    storage.referenced_plays += u64::from(iterations.len());
                    let gap = *gap_duration != FrameDuration::ZERO;
                    (
                        CompiledKind::Repeat {
                            default_child: by_id[child],
                            layout: repeat,
                            gap: gap.then_some(CompiledHold::Background),
                            gap_picture_context: None,
                            gap_duration: *gap_duration,
                            gap_audio: gap
                                .then(|| retained_hold_audio(context, id, *gap_audio))
                                .transpose()?,
                        },
                        NodeType::Repeat,
                    )
                }
                FrozenAudioKind::Retime {
                    child,
                    mapping,
                    pitch,
                    purpose,
                } => (
                    CompiledKind::Retime {
                        child: by_id[child],
                        start: ExactRatio::integer(mapping.start().0),
                        scale: ExactRatio::new(
                            i128::from(mapping.duration().frames()),
                            i128::from(node.duration.frames()),
                        )?,
                        pitch: *pitch,
                        purpose: *purpose,
                    },
                    NodeType::Retime,
                ),
            };
            nodes.push(PlanNode {
                inspection: NodeInspection {
                    id: id.clone(),
                    label: String::new(),
                    kind: node_type,
                    duration: node.duration,
                },
                kind,
                audio_edges: node.edges,
                audio_editorial_edges: node.editorial_edges,
                audio_treatments: context
                    .audio_treatments()
                    .get(id)
                    .cloned()
                    .unwrap_or_default(),
                framing: None,
            });
        }
        Ok(Self {
            sounds: BTreeMap::new(),
            beat_sounds: BTreeMap::new(),
            sound_allowances: BTreeMap::new(),
            sound_routes: BTreeMap::new(),
            compiled_sounds: BTreeMap::new(),
            metadata: PlanMetadata {
                project_id: context.project_id().clone(),
                revision_id: context.revision_id().clone(),
                root: layout.root().clone(),
                // This neutral geometry is never eligible for picture rendering.
                presentation_basis: PresentationBasis {
                    width: 16,
                    height: 16,
                    frame_rate: layout.rate(),
                    color_policy: ColorPolicy::SdrRec709,
                },
                duration: layout.duration(),
                storage,
            },
            nodes,
            root: by_id[layout.root()],
            by_id,
            audio_assets: context.assets().clone(),
            audio_context: true,
            has_audio_treatments: !context.audio_treatments().is_empty(),
            has_audio_editorial_edges: layout
                .nodes()
                .values()
                .any(|node| !node.editorial_edges.is_empty()),
            audio_bindings: Default::default(),
            // Context schema 1 cannot carry bindings. Definition exclusions
            // are therefore immaterial in these retained legacy operands.
            parents: vec![None; layout.nodes().len()],
        })
    }
}

fn retained_hold_audio(
    context: &FrozenAudioContext,
    id: &NodeId,
    audio: deadpan_core::ReferenceAudibility,
) -> Result<HoldAudio, PlanError> {
    use deadpan_core::ReferenceAudibility;
    let source = || {
        if let Some(FrozenAudioInput::Hold { source }) = context.inputs().get(id) {
            Ok(source.clone())
        } else {
            Err(PlanError::InvalidPlan("missing retained Hold input"))
        }
    };
    Ok(match audio {
        ReferenceAudibility::Silence => HoldAudio::Silence,
        ReferenceAudibility::RoomTone => HoldAudio::RoomTone { source: source()? },
        ReferenceAudibility::Tail { maximum } => HoldAudio::Tail {
            source: source()?,
            maximum,
        },
    })
}
