use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use deadpan_core::{
    AssetId, CapturedFraming, EndpointPolicy, ExactRatio, FrameDuration, FrameRange, HoldAudio,
    HoldRecipe, HoldVideo, InsertionBias, InstancePath, NodeId, NodeKind, PitchPolicy,
    PresentationBasis, ProjectDocument, ProjectFrame, ProjectId, RepeatInstance, RepeatLayout,
    RetimePurpose, RevisionId, SourceAudio, SourceFrameId, SourcePoint, SourceSpan, SourceTimeBase,
    SourceVideo, TimeError,
};
use serde::Serialize;

use crate::{Picture, PictureFraming, PictureSample, PlanError};

#[path = "picture_definition.rs"]
mod picture_definition;
pub use picture_definition::{
    DefinitionPictureCoverage, DefinitionPictureSample, DefinitionPictureSpan,
    HoldContextUnavailable, MAX_DEFINITION_PICTURE_SPANS, MAX_HOLD_CONTEXT_BATCH,
    MAX_HOLD_CONTEXT_FRAMES, PictureClockSlope, ScopedHoldBoundaries, ScopedHoldContext,
    ScopedHoldContextObservation, ScopedHoldContextRequest,
};
use picture_definition::{PictureBudget, PictureContinuity, PictureWalk};
#[path = "picture_definition_index.rs"]
mod picture_definition_index;
use picture_definition_index::DefinitionIndex;

#[path = "audio.rs"]
mod audio;
pub use audio::*;
#[path = "audio_domain.rs"]
mod audio_domain;
pub use audio_domain::{AudioDomain, AudioRootPlacement};
#[path = "audio_point_domain.rs"]
mod audio_point_domain;
pub use audio_point_domain::AudioPointDomain;
#[path = "audio_definition.rs"]
mod audio_definition;
pub use audio_definition::{AudioDefinition, AudioDefinitionSelector};
#[path = "audio_boundary.rs"]
mod audio_boundary;
pub use audio_boundary::{AudioBoundaries, AudioBoundaryKind, AudioBoundaryOrigin};
#[path = "audio_signal.rs"]
mod audio_signal;
pub use audio_signal::*;
#[path = "audio_bound.rs"]
mod audio_bound;
#[path = "audio_context.rs"]
mod audio_context;
pub use audio_bound::{AudioBound, AudioBoundDomain};
#[path = "audio_policy.rs"]
mod audio_policy;
pub use audio_policy::AudioPolicyQuery;
#[path = "audio_hold_policy.rs"]
mod audio_hold_policy;
pub use audio_hold_policy::{AudioHoldIssuer, AudioHoldPolicyQuery, AudioHoldRule};
#[path = "audio_fades.rs"]
mod audio_fades;
pub use audio_fades::{AudioFadeEdge, AudioFadeQuery, AudioFadeSpan};
#[path = "audio_owners.rs"]
mod audio_owners;
pub use audio_owners::{
    AudioOwnerClock, AudioOwnerClockOrigin, AudioOwnerKind, AudioOwnerQuery, AudioOwnerSpan,
    AudioOwnerSupport,
};

#[path = "audio_owner_occurrences.rs"]
mod audio_owner_occurrences;
pub use audio_owner_occurrences::{
    AudioOwnerOccurrence, AudioOwnerOccurrenceMap, AudioOwnerOccurrences,
};

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
/// Storage is O(authored nodes + child edges + compact iteration runs + catalog assets).
#[derive(Debug, Clone)]
pub struct RenderPlan {
    metadata: PlanMetadata,
    nodes: Vec<PlanNode>,
    by_id: BTreeMap<NodeId, usize>,
    root: usize,
    // Catalog-only sounds are retained alongside structural dependencies.
    // These contracts still require explicit host admission.
    audio_assets: BTreeMap<AssetId, deadpan_core::AssetRecord>,
    sounds: BTreeMap<deadpan_core::SoundId, deadpan_core::SoundEvent>,
    beat_sounds: BTreeMap<NodeId, BTreeMap<deadpan_core::SoundId, deadpan_core::BeatSound>>,
    sound_routes: BTreeMap<deadpan_core::SoundId, deadpan_core::RootSoundRoute>,
    sound_allowances: BTreeMap<deadpan_core::SoundId, deadpan_core::SoundHoldAllowances>,
    /// Attention targets that `Follow` framing resolves in source time.
    targets: Arc<BTreeMap<deadpan_core::TargetId, deadpan_core::AttentionTarget>>,
    compiled_sounds: BTreeMap<deadpan_core::SoundId, crate::audio_sound_event::CompiledRootSound>,
    // Frozen admission stays distinct even when its catalog is empty.
    audio_context: bool,
    has_audio_treatments: bool,
    has_audio_editorial_edges: bool,
    audio_bindings: deadpan_core::AudioBindingState,
    // A checked structural snapshot used as the live side of sound-clock
    // correspondence proofs. Historical clocks live in `audio_bindings`.
    sound_clock_layout: Option<Arc<deadpan_core::FrozenAudioLayout>>,
    parents: Vec<Option<usize>>,
    definition_index: Option<Arc<DefinitionIndex>>,
}

/// One journal reference paired with the live processing scope. The proof is
/// constructed once per sound/reference, then reused to remap concrete Repeat
/// occurrences without revisiting either complete subtree.
#[derive(Debug, Clone)]
pub struct AudioSoundClockScope<'plan> {
    live_plan: &'plan RenderPlan,
    live_owner: NodeId,
    timing: &'plan deadpan_core::AudioTimingId,
    historical_scope: &'plan NodeId,
    historical_owner: &'plan NodeId,
    historical_layout: &'plan deadpan_core::FrozenAudioLayout,
    correspondence: deadpan_core::SoundClockCorrespondence,
    recipe: crate::AudioSourceVoiceRecipe,
}

/// Authority to compare a current occurrence with one proven historical alias.
/// Callers cannot construct this token from NodeIds or arbitrary plans.
#[derive(Debug, Clone)]
pub struct AudioOccurrenceAlias<'plan> {
    pub(crate) live_plan: &'plan RenderPlan,
    pub(crate) historical_plan: &'plan RenderPlan,
    pub(crate) live_instance: InstancePath,
    pub(crate) historical_instance: InstancePath,
    pub(crate) recipe: crate::AudioSourceVoiceRecipe,
}

/// A retained processing plan bound once to one journal reference and event
/// recipe. Occurrence aliases derive from this checked binding.
#[derive(Debug, Clone)]
pub struct AudioSoundClockBinding<'scope, 'plan> {
    scope: &'scope AudioSoundClockScope<'plan>,
    historical_plan: &'scope RenderPlan,
    recipe: crate::AudioSourceVoiceRecipe,
}

impl<'plan> AudioSoundClockScope<'plan> {
    pub fn timing(&self) -> &deadpan_core::AudioTimingId {
        self.timing
    }

    pub fn historical_layout(&self) -> &deadpan_core::FrozenAudioLayout {
        self.historical_layout
    }

    pub fn historical_scope(&self) -> &NodeId {
        self.historical_scope
    }

    pub fn historical_owner(&self) -> &NodeId {
        self.historical_owner
    }

    pub fn proof_work(&self) -> usize {
        self.correspondence.work()
    }

    pub fn remap_instance(&self, live: &InstancePath) -> Result<InstancePath, PlanError> {
        Ok(self
            .remap_instance_with_work(live, deadpan_core::MAX_DOCUMENT_NODES)?
            .0)
    }

    pub fn remap_instance_with_work(
        &self,
        live: &InstancePath,
        maximum_work: usize,
    ) -> Result<(InstancePath, usize), PlanError> {
        if self.correspondence.historical_node(&live.node).is_none() {
            return Err(PlanError::InvalidPlan(
                "sound clock occurrence is outside the live scope",
            ));
        }
        Ok(self
            .correspondence
            .remap_instance_with_work(live, maximum_work)?)
    }

    pub fn try_remap_instance_with_work(
        &self,
        live: &InstancePath,
        maximum_work: usize,
    ) -> Result<(Option<InstancePath>, usize), PlanError> {
        Ok(self
            .correspondence
            .try_remap_instance_with_work(live, maximum_work)?)
    }

    /// Bind one compiled plan to this exact frozen layout and sound recipe.
    /// The full layout and selected asset contract are compared once per read,
    /// not once per concrete Repeat occurrence.
    pub fn bind_historical_plan<'scope>(
        &'scope self,
        historical_plan: &'scope RenderPlan,
        maximum_work: usize,
    ) -> Result<(AudioSoundClockBinding<'scope, 'plan>, usize), PlanError> {
        let recipe = self.recipe.clone();
        let asset = recipe.source.asset.clone();
        let work = RenderPlan::sound_processing_layout_work(
            self.historical_layout,
            &BTreeSet::from([asset.clone()]),
        )?
        .checked_mul(2)
        .ok_or(TimeError::Overflow)?;
        if work > maximum_work {
            return Err(PlanError::AudioQueryLimit("sound plan binding work"));
        }
        if historical_plan.sound_clock_layout.as_deref() != Some(self.historical_layout)
            || historical_plan.metadata.presentation_basis.frame_rate
                != self.live_plan.metadata.presentation_basis.frame_rate
            || historical_plan.audio_context_assets().is_none()
            || historical_plan.audio_assets.get(&asset) != self.live_plan.audio_assets.get(&asset)
            || historical_plan
                .audio_assets
                .get(&asset)
                .is_none_or(|record| {
                    !record
                        .audio
                        .is_some_and(|span| span.contains_span(recipe.source.span))
                })
        {
            return Err(PlanError::InvalidPlan(
                "sound processing plan does not match its frozen clock and asset",
            ));
        }
        Ok((
            AudioSoundClockBinding {
                scope: self,
                historical_plan,
                recipe,
            },
            work,
        ))
    }
}

impl AudioSoundClockBinding<'_, '_> {
    /// Mint an alias only from the exact current and historical handles and
    /// recipe validated by this scope proof and plan binding.
    pub fn alias_occurrence<'a>(
        &'a self,
        historical: &'a crate::AudioSourceOccurrence<'_>,
        live: &'a crate::AudioSourceOccurrence<'_>,
        maximum_work: usize,
    ) -> Result<(AudioOccurrenceAlias<'a>, usize), PlanError> {
        let scope = self.scope;
        if !live.belongs_to(scope.live_plan)
            || live.instance().node != scope.live_owner
            || scope.correspondence.historical_node(&live.instance().node)
                != Some(scope.historical_owner)
            || !std::ptr::eq(historical.plan(), self.historical_plan)
            || historical.recipe() != &self.recipe
            || live.recipe() != &self.recipe
        {
            return Err(PlanError::InvalidAudioSourceOccurrence(
                "sound occurrences do not match their bound clock alias",
            ));
        }
        let (expected_historical, work) =
            scope.remap_instance_with_work(live.instance(), maximum_work)?;
        if historical.instance() != &expected_historical {
            return Err(PlanError::InvalidAudioSourceOccurrence(
                "historical sound occurrence has a foreign processing instance",
            ));
        }
        Ok((
            AudioOccurrenceAlias {
                live_plan: scope.live_plan,
                historical_plan: self.historical_plan,
                live_instance: live.instance().clone(),
                historical_instance: historical.instance().clone(),
                recipe: self.recipe.clone(),
            },
            work,
        ))
    }
}

#[derive(Debug, Clone)]
struct PlanNode {
    inspection: NodeInspection,
    kind: CompiledKind,
    audio_edges: deadpan_core::AudioEdgePolicies,
    audio_editorial_edges: deadpan_core::AudioEditorialEdges,
    audio_treatments: deadpan_core::AudioTreatments,
    framing: Option<deadpan_core::Framing>,
    /// Picture-only cutaways, each with its asset's full video context.
    cutaways: Vec<(deadpan_core::Cutaway, deadpan_core::SourceSpan)>,
    captions: Vec<deadpan_core::Caption>,
}

#[derive(Debug, Clone)]
enum CompiledKind {
    Source {
        video: SourceVideo,
        start: ExactRatio,
        duration: ExactRatio,
        endpoints: EndpointPolicy,
        // Exact selected source ticks; the stream span owns their common clock.
        selection: Option<(ExactRatio, ExactRatio)>,
        audio: Option<CompiledSourceAudio>,
    },
    Sequence {
        entries: Vec<SequenceEntry>,
    },
    Hold {
        video: CompiledHold,
        picture_context: Option<Arc<CapturedFraming>>,
        audio: HoldAudio,
    },
    Repeat {
        default_child: usize,
        layout: RepeatLayout,
        gap: Option<CompiledHold>,
        gap_picture_context: Option<Arc<CapturedFraming>>,
        gap_audio: Option<HoldAudio>,
        gap_duration: FrameDuration,
        escalation: Option<deadpan_core::RepeatEscalation>,
    },
    Retime {
        child: usize,
        start: ExactRatio,
        scale: ExactRatio,
        pitch: PitchPolicy,
        purpose: RetimePurpose,
    },
}

#[derive(Debug, Clone)]
struct CompiledSourceAudio {
    source: SourceAudio,
    start: ExactRatio,
    duration: ExactRatio,
    selection: deadpan_core::ExactFrameRange,
    selected: bool,
}

impl CompiledSourceAudio {
    fn outside_reason(&self) -> SilenceReason {
        if self.selected {
            SilenceReason::OutsideSourceSelection
        } else {
            SilenceReason::OutsideSourcePlacement
        }
    }
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
        generated: Option<Arc<deadpan_core::GeneratedArtifact>>,
        time_base: SourceTimeBase,
        frames: FrameRange,
    },
    /// The span played at the project rate, backwards from its end or
    /// forward from its start.
    Original {
        asset: AssetId,
        /// Full measured picture stream, the affine context of the selection.
        context: SourceSpan,
        span: SourceSpan,
        ticks_per_frame: ExactRatio,
        reverse: bool,
        origin: ExactRatio,
    },
}

impl CompiledHold {
    fn picture_continuity(
        &self,
        local: ExactRatio,
        continuity: &mut PictureContinuity,
    ) -> Result<(), PlanError> {
        match self {
            Self::Background | Self::Freeze { .. } => Ok(()),
            Self::Accepted { .. } => {
                continuity.accepted();
                Ok(())
            }
            Self::Original {
                span,
                ticks_per_frame,
                reverse,
                origin,
                ..
            } => continuity.original_hold(
                local,
                ExactRatio::integer(span.start().ticks),
                ExactRatio::integer(span.end().ticks),
                *origin,
                *ticks_per_frame,
                *reverse,
            ),
        }
    }

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
                generated: None,
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
                generated: Some(Arc::new(accepted.artifact.clone())),
                time_base: document
                    .assets()
                    .get(&accepted.artifact.sampled_asset)
                    .and_then(|record| record.video)
                    .ok_or(PlanError::InvalidPlan(
                        "generated artifact has no video clock",
                    ))?
                    .start()
                    .time_base,
                // The master already materializes the retained sampling map.
                // Resizing selects its prefix, without resampling that map.
                frames: FrameRange::new(ProjectFrame(0), ProjectFrame(recipe.duration.frames()))?,
            },
            HoldVideo::Reverse {
                asset,
                span,
                origin,
            }
            | HoldVideo::Play {
                asset,
                span,
                origin,
            } => {
                let base = span.start().time_base;
                let rate = document.presentation_basis().frame_rate;
                let reverse = matches!(recipe.video, HoldVideo::Reverse { .. });
                Self::Original {
                    reverse,
                    origin: origin.unwrap_or_else(|| {
                        ExactRatio::integer(if reverse {
                            span.end().ticks
                        } else {
                            span.start().ticks
                        })
                    }),
                    asset: asset.clone(),
                    context: document
                        .assets()
                        .get(asset)
                        .and_then(|record| record.video)
                        .ok_or(PlanError::InvalidPlan(
                            "reversed picture has no video stream",
                        ))?,
                    span: *span,
                    // Source ticks per project frame:
                    // (fps_den / fps_num) / (tb_num / tb_den).
                    ticks_per_frame: ExactRatio::new(
                        i128::from(rate.denominator()) * i128::from(base.denominator()),
                        i128::from(rate.numerator()) * i128::from(base.numerator()),
                    )?,
                }
            }
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
                generated,
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
                    generated: generated.clone(),
                    time_base: *time_base,
                    position,
                    frame: SourceFrameId(
                        u64::try_from(position.floor()).map_err(|_| TimeError::Overflow)?,
                    ),
                }
            }
            Self::Original {
                asset,
                context,
                span,
                ticks_per_frame,
                reverse,
                origin,
            } => {
                let time_base = span.start().time_base;
                let start = ExactRatio::integer(span.start().ticks);
                let end = ExactRatio::integer(span.end().ticks);
                let elapsed = local.checked_mul(*ticks_per_frame)?;
                // Local positions are picture centers. Past the span's other
                // end the first (reversed) or last (forward) selected picture
                // holds under the adjacent-hold endpoint policy.
                let requested = if *reverse {
                    origin.checked_sub(elapsed)?
                } else {
                    origin.checked_add(elapsed)?
                };
                let point = if requested.compare(start).is_lt() {
                    start
                } else if requested.compare(end).is_gt() {
                    end
                } else {
                    requested
                };
                Picture::Source {
                    asset: asset.clone(),
                    span: *context,
                    selection: deadpan_core::ExactSourceSpan::new(
                        SourcePoint {
                            ticks: start,
                            time_base,
                        },
                        SourcePoint {
                            ticks: ExactRatio::integer(span.end().ticks),
                            time_base,
                        },
                    )?,
                    endpoints: deadpan_core::EndpointPolicy::HoldAdjacent,
                    point: SourcePoint {
                        ticks: point,
                        time_base,
                    },
                }
            }
        })
    }
}

impl RenderPlan {
    pub fn compile(document: &ProjectDocument) -> Result<Self, PlanError> {
        let durations = document.durations_shared()?;
        let by_id: BTreeMap<_, _> = document
            .nodes()
            .keys()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect();
        // A transient hashed index of each node's position and duration for
        // the child references below; `by_id` stays the plan's sorted map.
        let mut index: std::collections::HashMap<&NodeId, (usize, FrameDuration)> =
            std::collections::HashMap::with_capacity(by_id.len());
        for (position, (id, duration)) in durations.iter().enumerate() {
            index.insert(id, (position, *duration));
        }
        if index.len() != by_id.len() || durations.keys().ne(document.nodes().keys()) {
            return Err(PlanError::InvalidPlan(
                "durations do not describe the nodes",
            ));
        }
        let position = |id: &NodeId| index[id].0;
        let mut storage = StorageStats {
            authored_nodes: by_id.len(),
            ..StorageStats::default()
        };
        let mut nodes = Vec::with_capacity(by_id.len());
        for (id, node) in document.nodes() {
            let duration = index[id].1;
            let (kind, node_type) = match &node.kind {
                NodeKind::Source { source } => (
                    CompiledKind::Source {
                        video: source.video.clone(),
                        start: source.video_mapping.start_frames(),
                        duration: source.video_mapping.duration_frames(source.duration)?,
                        endpoints: source.video_mapping.endpoints(),
                        selection: match &source.video {
                            SourceVideo::Stream { span, .. } => {
                                let selected = source
                                    .video_mapping
                                    .selection_in_source(*span, source.duration)?;
                                Some((selected.start().ticks, selected.end().ticks))
                            }
                            SourceVideo::Still { .. } | SourceVideo::Blank => None,
                        },
                        audio: source
                            .audio
                            .as_ref()
                            .map(|audio| {
                                Ok::<_, TimeError>(CompiledSourceAudio {
                                    source: audio.clone(),
                                    start: source.audio_mapping.start_frames_with_offset(
                                        source.audio_offset,
                                        document.presentation_basis().frame_rate,
                                    )?,
                                    duration: source
                                        .audio_mapping
                                        .duration_frames(source.duration)?,
                                    selection: source.audio_mapping.selection_frames_with_offset(
                                        source.duration,
                                        source.audio_offset,
                                        document.presentation_basis().frame_rate,
                                    )?,
                                    selected: matches!(
                                        source.audio_mapping,
                                        deadpan_core::SourceAudioMapping::SelectedPlacement { .. }
                                    ),
                                })
                            })
                            .transpose()?,
                    },
                    NodeType::Source,
                ),
                NodeKind::Sequence { children } => {
                    let mut end = FrameDuration::ZERO;
                    let mut entries = Vec::with_capacity(children.len());
                    for child in children {
                        let start = end.frames();
                        let (child, duration) = index[child];
                        end = end.checked_add(duration)?;
                        entries.push(SequenceEntry {
                            child,
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
                        picture_context: recipe
                            .picture_context
                            .as_ref()
                            .map(|context| Arc::new(context.clone())),
                        audio: recipe.audio.clone(),
                    },
                    NodeType::Hold,
                ),
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                    escalation,
                } => {
                    let overrides = document.overrides().get(id);
                    let gap_overrides = document.gap_overrides().get(id);
                    let layout = RepeatLayout::compile_with_gap_overrides(
                        iterations,
                        child,
                        overrides,
                        gap.as_ref()
                            .map_or(FrameDuration::ZERO, |recipe| recipe.duration),
                        gap_overrides,
                        &durations,
                    )?;
                    storage.iteration_run_entries += iterations.segment_count();
                    storage.repeat_segment_entries += layout.segment_count();
                    storage.sparse_override_entries += overrides.map_or(0, |entries| entries.len())
                        + gap_overrides.map_or(0, |entries| entries.len());
                    storage.referenced_plays += u64::from(iterations.len());
                    (
                        CompiledKind::Repeat {
                            default_child: position(child),
                            layout,
                            gap_audio: gap.as_ref().map(|recipe| recipe.audio.clone()),
                            gap_duration: gap
                                .as_ref()
                                .map_or(FrameDuration::ZERO, |recipe| recipe.duration),
                            gap_picture_context: gap
                                .as_ref()
                                .and_then(|recipe| recipe.picture_context.as_ref())
                                .map(|context| Arc::new(context.clone())),
                            gap: gap
                                .as_ref()
                                .map(|recipe| CompiledHold::compile(recipe, document))
                                .transpose()?,
                            escalation: *escalation,
                        },
                        NodeType::Repeat,
                    )
                }
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                    purpose,
                } => (
                    CompiledKind::Retime {
                        child: position(child),
                        pitch: *pitch,
                        purpose: *purpose,
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
                audio_edges: node.audio_edges,
                audio_editorial_edges: node.audio_editorial_edges,
                audio_treatments: node.audio_treatments.clone(),
                framing: node.framing.clone(),
                cutaways: node
                    .cutaways
                    .iter()
                    .map(|cutaway| {
                        document
                            .assets()
                            .get(&cutaway.asset)
                            .and_then(|asset| asset.video)
                            .map(|span| (cutaway.clone(), span))
                            .ok_or(PlanError::InvalidPlan("cutaway asset has no video"))
                    })
                    .collect::<Result<_, _>>()?,
                captions: node.captions.clone(),
            });
        }
        let root = position(document.root());
        let mut parents = vec![None; nodes.len()];
        for (parent, (id, node)) in document.nodes().iter().enumerate() {
            let overrides = document
                .overrides()
                .get(id)
                .into_iter()
                .chain(document.gap_overrides().get(id))
                .flat_map(|entries| entries.iter().map(|(_, root)| root));
            for child in node.kind.children().iter().chain(overrides) {
                parents[position(child)] = Some(parent);
            }
        }
        let definition_index = Some(Arc::new(DefinitionIndex::compile(
            document, &nodes, &by_id, &parents, root,
        )?));
        let mut plan = Self {
            metadata: PlanMetadata {
                project_id: document.project_id().clone(),
                revision_id: document.revision_id().clone(),
                root: document.root().clone(),
                presentation_basis: document.presentation_basis().clone(),
                duration: index[document.root()].1,
                storage,
            },
            nodes,
            by_id,
            root,
            parents,
            definition_index,
            audio_bindings: document.audio_bindings().clone(),
            sound_clock_layout: if document.audio_bindings().sound_clocks().is_empty() {
                None
            } else {
                Some(Arc::new(deadpan_core::FrozenAudioLayout::capture(
                    document,
                )?))
            },
            audio_assets: document.assets().clone(),
            sounds: document.sounds().clone(),
            beat_sounds: document.beat_sounds().clone(),
            sound_routes: document.sound_routes().clone(),
            sound_allowances: document.sound_allowances().clone(),
            targets: Arc::new(document.targets().clone()),
            compiled_sounds: BTreeMap::new(),
            audio_context: false,
            has_audio_treatments: document.nodes().values().any(|node| {
                !node.audio_treatments.is_empty()
                    || matches!(
                        &node.kind,
                        NodeKind::Repeat {
                            escalation: Some(escalation),
                            ..
                        } if escalation.gain_step != deadpan_core::GainDb::UNITY
                    )
            }),
            has_audio_editorial_edges: document
                .nodes()
                .values()
                .any(|node| !node.audio_editorial_edges.is_empty()),
        };
        plan.compiled_sounds = plan.compile_root_sounds()?;
        Ok(plan)
    }

    pub fn metadata(&self) -> &PlanMetadata {
        &self.metadata
    }

    pub fn sounds(&self) -> &BTreeMap<deadpan_core::SoundId, deadpan_core::SoundEvent> {
        &self.sounds
    }

    pub fn beat_sounds(
        &self,
    ) -> &BTreeMap<NodeId, BTreeMap<deadpan_core::SoundId, deadpan_core::BeatSound>> {
        &self.beat_sounds
    }

    pub fn sound_routes(&self) -> &BTreeMap<deadpan_core::SoundId, deadpan_core::RootSoundRoute> {
        &self.sound_routes
    }

    pub fn sound_allowances(
        &self,
    ) -> &BTreeMap<deadpan_core::SoundId, deadpan_core::SoundHoldAllowances> {
        &self.sound_allowances
    }

    pub(crate) fn compiled_root_sound(
        &self,
        id: &deadpan_core::SoundId,
    ) -> Option<&crate::audio_sound_event::CompiledRootSound> {
        self.compiled_sounds.get(id)
    }

    /// Retained contracts are serialized intent, not permission to read media.
    /// A source provider must explicitly compare each with its host receipt.
    pub fn audio_context_assets(&self) -> Option<&BTreeMap<AssetId, deadpan_core::AssetRecord>> {
        self.audio_context.then_some(&self.audio_assets)
    }

    /// Frozen layouts retained for one beat-owned sound, in chronological
    /// pre-edit order. The live plan supplies the final clock separately.
    pub fn beat_sound_clock_layouts<'a>(
        &'a self,
        owner: &deadpan_core::NodeId,
        sound: &deadpan_core::SoundId,
    ) -> Result<
        Vec<(
            &'a deadpan_core::AudioTimingId,
            &'a deadpan_core::FrozenAudioLayout,
        )>,
        PlanError,
    > {
        let Some(journal) = self
            .audio_bindings
            .sound_clocks()
            .get(owner)
            .and_then(|sounds| sounds.get(sound))
        else {
            return Ok(Vec::new());
        };
        journal
            .clocks()
            .iter()
            .map(|reference| {
                let layout = self
                    .audio_bindings
                    .timings()
                    .get(reference.timing())
                    .ok_or(PlanError::InvalidPlan(
                        "beat sound clock refers to a missing frozen layout",
                    ))?;
                Ok((reference.timing(), layout))
            })
            .collect()
    }

    /// Validated historical processing-scope aliases for one live BeatSound.
    /// Proof construction is bounded by `maximum_work`; concrete instance
    /// remapping is separately charged by the caller for each occurrence.
    pub fn beat_sound_clock_scopes<'a>(
        &'a self,
        owner: &NodeId,
        sound: &deadpan_core::SoundId,
        maximum_work: usize,
    ) -> Result<Vec<AudioSoundClockScope<'a>>, PlanError> {
        let Some(journal) = self
            .audio_bindings
            .sound_clocks()
            .get(owner)
            .and_then(|sounds| sounds.get(sound))
        else {
            return Ok(Vec::new());
        };
        let current_layout = self
            .sound_clock_layout
            .as_deref()
            .ok_or(PlanError::InvalidPlan(
                "beat sound clock has no live frozen layout",
            ))?;
        let mut scopes = Vec::with_capacity(journal.clocks().len());
        let event = self
            .beat_sounds
            .get(owner)
            .and_then(|events| events.get(sound))
            .ok_or(PlanError::InvalidPlan(
                "sound clock has no matching live beat sound",
            ))?;
        let recipe = crate::AudioSourceVoiceRecipe {
            source: event.source.clone(),
            mapping: event.mapping,
            offset: event.offset,
        };
        let mut remaining_work = maximum_work;
        for reference in journal.clocks() {
            let historical_layout = self
                .audio_bindings
                .timings()
                .get(reference.timing())
                .ok_or(PlanError::InvalidPlan(
                    "beat sound clock refers to a missing frozen layout",
                ))?;
            let correspondence = historical_layout.sound_clock_correspondence_with_repeats(
                current_layout,
                reference.scope(),
                journal.scope(),
                reference.repeats(),
                remaining_work,
            )?;
            remaining_work = remaining_work.checked_sub(correspondence.work()).ok_or(
                PlanError::AudioQueryLimit("sound clock correspondence work"),
            )?;
            if correspondence.historical_node(owner) != Some(reference.owner()) {
                return Err(PlanError::InvalidPlan(
                    "beat sound clock owner is outside its proven scope",
                ));
            }
            scopes.push(AudioSoundClockScope {
                live_plan: self,
                live_owner: owner.clone(),
                timing: reference.timing(),
                historical_scope: reference.scope(),
                historical_owner: reference.owner(),
                historical_layout,
                correspondence,
                recipe: recipe.clone(),
            });
        }
        Ok(scopes)
    }

    pub fn duration(&self) -> FrameDuration {
        self.metadata.duration
    }

    pub fn node_duration(&self, id: &NodeId) -> Option<FrameDuration> {
        self.by_id
            .get(id)
            .map(|index| self.nodes[*index].inspection.duration)
    }

    /// The project frames of a node that appears exactly once, reached only
    /// through ordinary Sequences; `None` under a Repeat, a Retime or an
    /// override, where a node has several or remapped occurrences.
    pub fn single_occurrence_range(&self, id: &NodeId) -> Option<deadpan_core::FrameRange> {
        let mut current = *self.by_id.get(id)?;
        let mut start = 0_i64;
        while let Some(parent) = self.parents[current] {
            let CompiledKind::Sequence { entries } = &self.nodes[parent].kind else {
                return None;
            };
            let entry = entries.iter().find(|entry| entry.child == current)?;
            start = start.checked_add(entry.start)?;
            current = parent;
        }
        if current != self.root {
            return None;
        }
        let end = start.checked_add(
            self.nodes[*self.by_id.get(id)?]
                .inspection
                .duration
                .frames(),
        )?;
        deadpan_core::FrameRange::new(ProjectFrame(start), ProjectFrame(end)).ok()
    }

    pub(crate) fn root_audio_edges(&self) -> deadpan_core::AudioEdgePolicies {
        self.nodes[self.root].audio_edges
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
        self.sample_picture(frame, true)
    }

    /// The picture a frame would show without cutaways: the provider whose
    /// sound plays there. Analyses that follow the heard Original use this.
    pub fn provider_picture(&self, frame: ProjectFrame) -> Result<PictureSample, PlanError> {
        self.sample_picture(frame, false)
    }

    /// Center each `Follow` layer on its target at the picture's source time.
    ///
    /// `framing` runs from the root inward; the picture enters at the last
    /// layer. The target's point is carried outward through every inner posed
    /// layer (`p' = (p - center) * scale + 1/2` per axis), innermost first, so
    /// an outer follow sees where the subject is after inner framing. A layer
    /// keeps its fallback where the picture is not the target's asset, lies
    /// outside its span, or the centered pose is out of range.
    fn resolve_follows(
        &self,
        framing: &mut [PictureFraming],
        follows: &[(usize, &deadpan_core::TargetId, ExactRatio)],
        picture: &Picture,
        captured_geometry: bool,
    ) {
        // A pause's captured geometry sits between its picture and every
        // framing layer; follows keep their fallback there until that
        // geometry is mapped too.
        if captured_geometry {
            return;
        }
        let Some((asset, point)) = picture.follow_point() else {
            return;
        };
        for &(index, target, scale) in follows.iter().rev() {
            let Some(found) = self.targets.get(target) else {
                continue;
            };
            let inner = framing[index + 1..].iter().rev().map(|layer| layer.pose);
            if let Some(pose) = follow_pose(found, asset, point, inner, scale) {
                framing[index].pose = Some(pose);
            }
        }
    }

    fn sample_picture(
        &self,
        frame: ProjectFrame,
        cutaways: bool,
    ) -> Result<PictureSample, PlanError> {
        if self.audio_context {
            return Err(PlanError::AudioOnlyContext);
        }
        if frame.0 < 0 || frame.0 >= self.duration().frames() {
            return Err(PlanError::FrameOutOfRange {
                frame,
                duration: self.duration(),
            });
        }
        let position = ExactRatio::new(i128::from(frame.0) * 2 + 1, 2)?;
        let sample = self.walk_picture_at(
            self.root,
            position,
            cutaways,
            &mut PictureBudget::unlimited(),
            None,
        )?;
        Ok(PictureSample {
            project_id: self.metadata.project_id.clone(),
            revision_id: self.metadata.revision_id.clone(),
            project_frame: frame,
            instance: sample.instance,
            gap_after: sample.gap_after,
            local_position: sample.local_position,
            picture: sample.picture,
            picture_context: sample.picture_context,
            framing: sample.framing,
            captions: sample.captions,
            lookup: sample.lookup,
        })
    }

    fn sample_definition_picture(
        &self,
        definition: usize,
        position: ExactRatio,
        cutaways: bool,
        budget: &mut PictureBudget,
    ) -> Result<DefinitionPictureSample, PlanError> {
        self.sample_definition_picture_with_continuity(definition, position, cutaways, budget, None)
    }

    fn sample_definition_picture_with_continuity(
        &self,
        definition: usize,
        position: ExactRatio,
        cutaways: bool,
        budget: &mut PictureBudget,
        continuity: Option<&mut PictureContinuity>,
    ) -> Result<DefinitionPictureSample, PlanError> {
        let sample = self.walk_picture_at(definition, position, cutaways, budget, continuity)?;
        let hold_provider = self.definition_hold_witness(definition, position, &sample);
        let sample = DefinitionPictureSample {
            project_id: self.metadata.project_id.clone(),
            revision_id: self.metadata.revision_id.clone(),
            definition: self.nodes[definition].inspection.id.clone(),
            position,
            instance: sample.instance,
            gap_after: sample.gap_after,
            local_position: sample.local_position,
            picture: sample.picture,
            picture_context: sample.picture_context,
            framing: sample.framing,
            captions: sample.captions,
            lookup: sample.lookup,
            hold_provider,
        };
        budget.retain_sample(&sample)?;
        Ok(sample)
    }

    fn walk_picture_at(
        &self,
        definition: usize,
        position: ExactRatio,
        cutaways: bool,
        budget: &mut PictureBudget,
        mut continuity: Option<&mut PictureContinuity>,
    ) -> Result<PictureWalk, PlanError> {
        if self.audio_context {
            return Err(PlanError::AudioOnlyContext);
        }
        let root = &self.nodes[definition].inspection;
        if position.compare_integer(0).is_lt()
            || !position.compare_integer(root.duration.frames()).is_lt()
        {
            return Err(PlanError::DefinitionPictureOutOfRange {
                definition: root.id.clone(),
                position: Box::new(position),
                duration: root.duration,
            });
        }
        let previous_lookup = budget.lookup;
        let mut local = position;
        let mut current = definition;
        let mut repeats = Vec::new();
        let mut framing = Vec::new();
        let mut captions = Vec::new();
        // The zero-based play of the innermost enclosing Repeat, for reveals.
        let mut play = None;
        // Follow layers resolve once the picture's source time is known.
        let mut follows = Vec::new();
        let (picture, picture_context, gap_after) = loop {
            let node = &self.nodes[current];
            budget.visit()?;
            if local.compare_integer(0).is_lt()
                || !local
                    .compare_integer(node.inspection.duration.frames())
                    .is_lt()
            {
                return Err(PlanError::InvalidPlan(
                    "local picture coordinate exceeds node duration",
                ));
            }
            if let Some(continuity) = continuity.as_deref_mut() {
                continuity.limit(
                    local,
                    ExactRatio::integer(node.inspection.duration.frames()),
                )?;
            }
            if let Some(deadpan_core::Framing {
                value: deadpan_core::FramingValue::Follow { target, scale, .. },
                ..
            }) = &node.framing
            {
                follows.push((framing.len(), target, *scale));
            }
            framing.push(PictureFraming {
                instance: InstancePath {
                    node: node.inspection.id.clone(),
                    repeats: repeats.clone(),
                },
                local_position: local,
                duration: node.inspection.duration,
                pose: node
                    .framing
                    .as_ref()
                    .map(|framing| framing.evaluate(local, node.inspection.duration))
                    .transpose()?,
                escalation: false,
            });
            captions.extend(
                node.captions
                    .iter()
                    .filter(|caption| caption.shows(local, play))
                    .map(|caption| crate::PictureCaption {
                        text: caption.text.clone(),
                        placement: caption.placement,
                    }),
            );
            // A cutaway replaces this beat's provider picture inside its range;
            // this beat's framing and its ancestors' still apply.
            let mut visible_cutaway = None;
            if cutaways {
                for (cutaway, context) in &node.cutaways {
                    if continuity.is_some() {
                        budget.sequence_comparison()?;
                    }
                    if local.compare_integer(cutaway.range.start().0).is_lt() {
                        if let Some(continuity) = continuity.as_deref_mut() {
                            continuity
                                .limit(local, ExactRatio::integer(cutaway.range.start().0))?;
                        }
                        break;
                    }
                    if !local.compare_integer(cutaway.range.end().0).is_lt() {
                        continue;
                    }
                    let point = if cutaway.removed {
                        None
                    } else {
                        cutaway.picture_point(local, self.metadata.presentation_basis.frame_rate)?
                    };
                    if let Some(continuity) = continuity.as_deref_mut() {
                        let visible = continuity.cutaway(
                            cutaway,
                            local,
                            self.metadata.presentation_basis.frame_rate,
                        )?;
                        debug_assert_eq!(visible, cutaway.removed || point.is_some());
                    }
                    if cutaway.removed || point.is_some() {
                        visible_cutaway = Some((cutaway, context, point));
                    }
                    break;
                }
            }
            if let Some((cutaway, context, point)) = visible_cutaway {
                if cutaway.removed {
                    // A video-only delete exposes the project background.
                    break (Picture::Background, None, None);
                }
                let point = point.ok_or(PlanError::InvalidPlan("cutaway picture"))?;
                break (
                    Picture::Source {
                        asset: cutaway.asset.clone(),
                        span: *context,
                        selection: cutaway.selection,
                        endpoints: deadpan_core::EndpointPolicy::HoldAdjacent,
                        point,
                    },
                    None,
                    None,
                );
            }
            match &node.kind {
                CompiledKind::Source {
                    video,
                    start,
                    duration,
                    endpoints,
                    selection,
                    ..
                } => {
                    let picture = match video {
                        SourceVideo::Stream { asset, span } => {
                            let scale = ExactRatio::integer(span.end().ticks - span.start().ticks)
                                .checked_div(*duration)?;
                            if let Some(continuity) = continuity.as_deref_mut() {
                                continuity.source(scale)?;
                            }
                            let (selected_start, selected_end) = selection.ok_or(
                                PlanError::InvalidPlan("source picture has no selected interval"),
                            )?;
                            let time_base = span.start().time_base;
                            Picture::Source {
                                asset: asset.clone(),
                                span: *span,
                                selection: deadpan_core::ExactSourceSpan::new(
                                    SourcePoint {
                                        ticks: selected_start,
                                        time_base,
                                    },
                                    SourcePoint {
                                        ticks: selected_end,
                                        time_base,
                                    },
                                )?,
                                endpoints: *endpoints,
                                point: SourcePoint {
                                    ticks: ExactRatio::integer(span.start().ticks).checked_add(
                                        local.checked_sub(*start)?.checked_mul(scale)?,
                                    )?,
                                    time_base: span.start().time_base,
                                },
                            }
                        }
                        SourceVideo::Still { asset } => Picture::Still {
                            asset: asset.clone(),
                        },
                        SourceVideo::Blank => Picture::Blank,
                    };
                    break (picture, None, None);
                }
                CompiledKind::Hold {
                    video,
                    picture_context,
                    ..
                } => {
                    if let Some(continuity) = continuity.as_deref_mut() {
                        video.picture_continuity(local, continuity)?;
                    }
                    break (video.picture(local)?, picture_context.clone(), None);
                }
                CompiledKind::Sequence { entries } => {
                    let index = upper_bound(
                        entries.len(),
                        |index| !local.compare_integer(entries[index].end).is_lt(),
                        budget,
                    )?;
                    let entry = entries
                        .get(index)
                        .ok_or(PlanError::InvalidPlan("sequence prefix index has no child"))?;
                    if let Some(continuity) = continuity.as_deref_mut() {
                        continuity.limit(local, ExactRatio::integer(entry.end))?;
                    }
                    local = local.checked_sub(ExactRatio::integer(entry.start))?;
                    current = entry.child;
                }
                CompiledKind::Retime {
                    child,
                    start,
                    scale,
                    ..
                } => {
                    if let Some(continuity) = continuity.as_deref_mut() {
                        continuity.retime(*scale)?;
                    }
                    local = start.checked_add(local.checked_mul(*scale)?)?;
                    current = *child;
                }
                CompiledKind::Repeat {
                    layout,
                    gap,
                    gap_picture_context,
                    escalation,
                    ..
                } => {
                    let location = layout
                        .locate_bounded(local, InsertionBias::Right, budget.comparisons_left())
                        .map_err(|error| {
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                                PlanError::PictureQueryLimit("comparisons")
                            } else {
                                PlanError::Document(error)
                            }
                        })?;
                    budget.repeat_comparisons(location.comparisons)?;
                    if let Some(continuity) = continuity.as_deref_mut() {
                        let end = location
                            .play
                            .start
                            .checked_add(location.play.duration.frames())
                            .and_then(|end| {
                                if location.in_gap {
                                    end.checked_add(location.play.gap_after.frames())
                                } else {
                                    Some(end)
                                }
                            })
                            .ok_or(TimeError::Overflow)?;
                        continuity.limit(local, ExactRatio::integer(end))?;
                    }
                    local = location.position;
                    play = Some(location.play.index);
                    // A play and the gap following it share that play's
                    // escalation, applied inside the Repeat's own framing.
                    if let Some(pose) = escalation
                        .map(|escalation| escalation.pose(location.play.index))
                        .transpose()
                        .map_err(|_| PlanError::InvalidPlan("repeat escalation scale"))?
                        .flatten()
                    {
                        framing.push(PictureFraming {
                            instance: InstancePath {
                                node: node.inspection.id.clone(),
                                repeats: repeats.clone(),
                            },
                            local_position: local,
                            duration: node.inspection.duration,
                            pose: Some(pose),
                            escalation: true,
                        });
                    }
                    if location.in_gap {
                        if let Some(child) = location.play.gap_child {
                            repeats.push(RepeatInstance {
                                node: node.inspection.id.clone(),
                                iteration: location.play.iteration,
                            });
                            current = self.by_id[&child];
                            continue;
                        }
                        let gap = gap
                            .as_ref()
                            .ok_or(PlanError::InvalidPlan("repeat gap recipe is missing"))?;
                        if let Some(continuity) = continuity.as_deref_mut() {
                            gap.picture_continuity(local, continuity)?;
                        }
                        break (
                            gap.picture(local)?,
                            gap_picture_context.clone(),
                            Some(location.play.iteration),
                        );
                    }
                    repeats.push(RepeatInstance {
                        node: node.inspection.id.clone(),
                        iteration: location.play.iteration,
                    });
                    current = self.by_id[&location.play.child];
                }
            }
        };
        self.resolve_follows(&mut framing, &follows, &picture, picture_context.is_some());
        framing.reverse();
        Ok(PictureWalk {
            instance: InstancePath {
                node: self.nodes[current].inspection.id.clone(),
                repeats,
            },
            gap_after,
            local_position: local,
            picture,
            picture_context,
            framing,
            captions,
            lookup: budget.since(previous_lookup),
        })
    }
}

/// The pose a `Follow` layer resolves to: the target's region center at
/// `point`, carried outward through every inner layer's pose (innermost
/// first, `p' = (p - center) * scale + 1/2` per axis), at `scale`, quantized
/// to the framing grid. `None` where the picture is not the target's asset,
/// lies outside its span, or the centered pose is out of range; the layer then
/// keeps its fallback. Hosts previewing a changed inner pose use this to
/// re-resolve a follow exactly as the plan does.
pub fn follow_pose(
    target: &deadpan_core::AttentionTarget,
    asset: &AssetId,
    point: SourcePoint,
    inner: impl IntoIterator<Item = Option<deadpan_core::FramingPose>>,
    scale: ExactRatio,
) -> Option<deadpan_core::FramingPose> {
    if &target.asset != asset {
        return None;
    }
    let (region, _) = target.region_at(point)?;
    let half = ExactRatio::new(1, 2).expect("constant ratio");
    let [x, y] = inner
        .into_iter()
        .try_fold(region.center_ratio(), |position, pose| {
            let Some(pose) = pose else {
                return Some(position);
            };
            let through = |value: ExactRatio, center: ExactRatio| {
                value
                    .checked_sub(center)
                    .and_then(|offset| offset.checked_mul(pose.scale))
                    .and_then(|offset| offset.checked_add(half))
                    .ok()
            };
            Some([
                through(position[0], pose.center_x)?,
                through(position[1], pose.center_y)?,
            ])
        })?;
    deadpan_core::FramingPose::new(x, y, scale)
        .and_then(|pose| pose.quantized())
        .ok()
}

/// First element for which a monotonic predicate is false, with measured work.
fn upper_bound(
    length: usize,
    mut preceding: impl FnMut(usize) -> bool,
    budget: &mut PictureBudget,
) -> Result<usize, PlanError> {
    let mut left = 0;
    let mut right = length;
    while left < right {
        let middle = left + (right - left) / 2;
        budget.sequence_comparison()?;
        if preceding(middle) {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    Ok(left)
}
