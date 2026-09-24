//! Compile retained raw audio without inventing a picture-bearing document.
use deadpan_core::{ColorPolicy, FrozenAudioContext, FrozenAudioInput, FrozenAudioKind};

use super::*;

impl RenderPlan {
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
                    let repeat = RepeatLayout::compile(
                        iterations,
                        child,
                        overrides,
                        *gap_duration,
                        &durations,
                    )?;
                    storage.iteration_run_entries += iterations.segment_count();
                    storage.repeat_segment_entries += repeat.segment_count();
                    storage.sparse_override_entries += overrides.map_or(0, |entries| entries.len());
                    storage.referenced_plays += u64::from(iterations.len());
                    let gap = *gap_duration != FrameDuration::ZERO;
                    (
                        CompiledKind::Repeat {
                            default_child: by_id[child],
                            layout: repeat,
                            gap: gap.then_some(CompiledHold::Background),
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
            });
        }
        Ok(Self {
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
            audio_context_assets: Some(context.assets().clone()),
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
