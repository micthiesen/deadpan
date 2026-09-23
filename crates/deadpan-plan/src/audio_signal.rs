//! Point-sampled preparation support and root processing boundaries. Neither
//! query renders DSP. Borrowed stages retain their own uncut input history.

use std::ops::Range;

use deadpan_core::{
    AudioSample, ExactRatio, FrameDuration, InsertionBias, InstancePath, IterationId,
    MIX_SAMPLE_RATE, NodeId, PitchPolicy, ProjectId, RepeatInstance, RetimePurpose, RevisionId,
    SourcePoint, TimeError,
};
use serde::Serialize;

use super::audio::{Budget, intersect, maximum, minimum, source_point_from_local};
use super::{
    AudioContent, AudioQueryLimits, AudioRetimeStage, AudioTransform, CompiledKind, LookupStats,
    RenderPlan, SilenceReason, SourceSamplingSupport,
};
use crate::{AudioBoundaryRule, AudioSampleGrid, AudioSampleMap, PlanError};

/// Index in a temporary signal grid, never a project-output sample coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SignalSample(pub i64);

/// The current signal's frame clock and a leaf/stage's local frame clock.
/// Grid index k maps to `grid_origin + k * signal_frames_per_sample`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SignalTransform {
    pub signal_origin: ExactRatio,
    pub signal_frames_per_local_frame: ExactRatio,
    pub grid_origin: ExactRatio,
    pub signal_frames_per_sample: ExactRatio,
}

impl SignalTransform {
    fn grid(self, rule: AudioBoundaryRule) -> Result<AudioSampleGrid<SignalSample>, PlanError> {
        AudioSampleGrid::new(self.grid_origin, self.signal_frames_per_sample, rule)
    }

    pub fn signal_at(self, sample: SignalSample) -> Result<ExactRatio, TimeError> {
        self.grid_origin
            .checked_add(ExactRatio::integer(sample.0).checked_mul(self.signal_frames_per_sample)?)
    }

    pub fn local_at(self, sample: SignalSample) -> Result<ExactRatio, TimeError> {
        self.local_at_signal_frame(self.signal_at(sample)?)
    }

    pub fn local_at_signal_frame(self, frame: ExactRatio) -> Result<ExactRatio, TimeError> {
        frame
            .checked_sub(self.signal_origin)?
            .checked_div(self.signal_frames_per_local_frame)
    }

    fn signal_from_local(self, local: ExactRatio) -> Result<ExactRatio, TimeError> {
        self.signal_origin
            .checked_add(local.checked_mul(self.signal_frames_per_local_frame)?)
    }

    fn child(self, start: ExactRatio, scale: ExactRatio) -> Result<Self, TimeError> {
        Ok(Self {
            signal_origin: self.signal_from_local(start)?,
            signal_frames_per_local_frame: self.signal_frames_per_local_frame.checked_mul(scale)?,
            ..self
        })
    }
}

/// Stable authored processing identity. Selection is in the child's local
/// frame clock; rate consumes child frames per stage output frame. It contains
/// no ancestor crop or consumer-request offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioStageDescriptor {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub instance: InstancePath,
    pub child: NodeId,
    pub selection: Range<ExactRatio>,
    pub duration: FrameDuration,
    pub rate: ExactRatio,
    pub pitch: PitchPolicy,
}

/// A nonunity Preserve stage borrowed from exactly one immutable plan. Only
/// inspection data serializes; a descriptor cannot be deserialized into a live
/// stage or rebound to another plan.
#[derive(Debug, Clone, Serialize)]
pub struct AudioStage<'plan> {
    #[serde(skip)]
    plan: &'plan RenderPlan,
    #[serde(skip)]
    child: usize,
    #[serde(skip)]
    node: usize,
    #[serde(flatten)]
    descriptor: AudioStageDescriptor,
}

impl PartialEq for AudioStage<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.plan, other.plan) && self.descriptor == other.descriptor
    }
}
impl Eq for AudioStage<'_> {}

impl<'plan> AudioStage<'plan> {
    pub fn descriptor(&self) -> &AudioStageDescriptor {
        &self.descriptor
    }

    pub fn input_signal(&self) -> AudioSignal<'plan> {
        AudioSignal {
            plan: self.plan,
            root: self.child,
            support: self.descriptor.selection.clone(),
            constrain_support: true,
            repeats: self.descriptor.instance.repeats.clone(),
        }
    }

    /// The stage's full intrinsic output domain. Flattened policy queries on
    /// this grid retain silent Holds even when they owned no input-grid sample.
    pub fn output_signal(&self) -> AudioSignal<'plan> {
        AudioSignal {
            plan: self.plan,
            root: self.node,
            support: ExactRatio::ZERO..ExactRatio::integer(self.descriptor.duration.frames()),
            constrain_support: false,
            repeats: self.descriptor.instance.repeats.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AudioSignalContent<'plan> {
    Leaf(AudioContent),
    Stage(AudioStage<'plan>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioSignalSpan<'plan> {
    pub samples: Range<SignalSample>,
    pub allocated_samples: Range<SignalSample>,
    /// Exact structural extent in this signal's root frame clock, before grid
    /// allocation. Source filter support is retained separately in the content.
    pub signal_extent: Range<ExactRatio>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub transform: SignalTransform,
    pub grid: AudioSampleGrid<SignalSample>,
    pub sampling: AudioSampleMap<SignalSample>,
    /// Traversed transparent stages, outer to inner; an opaque Stage carries
    /// its own processing policy in its descriptor instead of this list.
    pub retimes: Vec<AudioRetimeStage>,
    pub content: AudioSignalContent<'plan>,
}

impl AudioSignalSpan<'_> {
    pub fn source_point(&self, sample: SignalSample) -> Result<SourcePoint, PlanError> {
        source_point(&self.content, self.sampling.local_at(sample)?)
    }

    pub fn source_point_at_signal_frame(
        &self,
        frame: ExactRatio,
    ) -> Result<SourcePoint, PlanError> {
        source_point(&self.content, self.transform.local_at_signal_frame(frame)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioSignalQuery<'plan> {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub samples: Range<SignalSample>,
    pub spans: Vec<AudioSignalSpan<'plan>>,
    pub lookup: LookupStats,
}

/// Root output allocation with stateful processing boundaries retained. Its
/// sample coordinates and allocation match `RenderPlan::audio`, not a virtual
/// point grid. A returned stage may have support beyond this ancestor crop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioProcessingSpan<'plan> {
    pub samples: Range<AudioSample>,
    pub allocated_samples: Range<AudioSample>,
    pub project_extent: Range<ExactRatio>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub transform: AudioTransform,
    pub grid: AudioSampleGrid<AudioSample>,
    pub sampling: AudioSampleMap<AudioSample>,
    pub retimes: Vec<AudioRetimeStage>,
    pub content: AudioSignalContent<'plan>,
}

impl AudioProcessingSpan<'_> {
    pub fn source_point(&self, sample: AudioSample) -> Result<SourcePoint, PlanError> {
        source_point(&self.content, self.sampling.local_at(sample)?)
    }

    pub fn source_point_at_project_frame(
        &self,
        frame: ExactRatio,
    ) -> Result<SourcePoint, PlanError> {
        source_point(
            &self.content,
            frame
                .checked_sub(self.transform.project_origin)?
                .checked_div(self.transform.project_frames_per_local_frame)?,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioProcessingQuery<'plan> {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub samples: Range<AudioSample>,
    pub spans: Vec<AudioProcessingSpan<'plan>>,
    pub lookup: LookupStats,
}

#[derive(Debug, Clone)]
pub struct AudioSignal<'plan> {
    plan: &'plan RenderPlan,
    root: usize,
    support: Range<ExactRatio>,
    // An authored stage input selection trims filter support. Root allocation
    // itself does not: a root Partition retains its complete child's support.
    constrain_support: bool,
    repeats: Vec<RepeatInstance>,
}

impl RenderPlan {
    /// Virtual root signal support, including samples that own no final root
    /// allocation. Its point-grid count must never set an exported duration.
    pub fn audio_signal(&self) -> AudioSignal<'_> {
        AudioSignal {
            plan: self,
            root: self.root,
            support: ExactRatio::ZERO..ExactRatio::integer(self.duration().frames()),
            constrain_support: false,
            repeats: Vec::new(),
        }
    }

    pub fn audio_processing(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioProcessingQuery<'_>, PlanError> {
        let query = self.audio_signal().query_inner(
            SignalSample(samples.start.0)..SignalSample(samples.end.0),
            limits,
            AudioBoundaryRule::RoundEven,
            true,
        )?;
        Ok(AudioProcessingQuery {
            project_id: query.project_id,
            revision_id: query.revision_id,
            samples,
            spans: query
                .spans
                .into_iter()
                .map(|span| {
                    Ok(AudioProcessingSpan {
                        samples: AudioSample(span.samples.start.0)..AudioSample(span.samples.end.0),
                        allocated_samples: AudioSample(span.allocated_samples.start.0)
                            ..AudioSample(span.allocated_samples.end.0),
                        project_extent: span.signal_extent,
                        instance: span.instance,
                        gap_after: span.gap_after,
                        transform: AudioTransform {
                            project_origin: span.transform.signal_origin,
                            project_frames_per_local_frame: span
                                .transform
                                .signal_frames_per_local_frame,
                            project_frames_per_sample: span.transform.signal_frames_per_sample,
                        },
                        grid: AudioSampleGrid::new(
                            span.grid.frame_origin(),
                            span.grid.frames_per_sample(),
                            span.grid.boundary_rule(),
                        )?,
                        sampling: AudioSampleMap::new(
                            AudioSample(span.sampling.anchor().0),
                            span.sampling.local_at_anchor(),
                            span.sampling.local_frames_per_sample(),
                        )?,
                        retimes: span.retimes,
                        content: span.content,
                    })
                })
                .collect::<Result<_, PlanError>>()?,
            lookup: query.lookup,
        })
    }
}

impl<'plan> AudioSignal<'plan> {
    pub fn support(&self) -> Range<ExactRatio> {
        self.support.clone()
    }

    pub fn sample_count(&self) -> Result<SignalSample, PlanError> {
        Ok(self
            .transform()?
            .grid(AudioBoundaryRule::PointCeil)?
            .boundary(self.support.end)?)
    }

    pub fn query(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        self.query_inner(samples, limits, AudioBoundaryRule::PointCeil, true)
    }

    /// Resolve structural policies through all retimes on the same point grid.
    /// This is policy/source inspection, not permission to bypass Preserve DSP.
    pub fn query_flattened(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        self.query_inner(samples, limits, AudioBoundaryRule::PointCeil, false)
    }

    fn transform(&self) -> Result<SignalTransform, TimeError> {
        let rate = self.plan.metadata.presentation_basis.frame_rate;
        Ok(SignalTransform {
            signal_origin: ExactRatio::ZERO,
            signal_frames_per_local_frame: ExactRatio::ONE,
            grid_origin: self.support.start,
            signal_frames_per_sample: ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
        })
    }

    fn query_inner(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        rule: AudioBoundaryRule,
        stop_at_preserve: bool,
    ) -> Result<AudioSignalQuery<'plan>, PlanError> {
        if limits.maximum_spans == 0
            || limits.maximum_spans > 4096
            || limits.maximum_work == 0
            || limits.maximum_work > 65_536
        {
            return Err(PlanError::InvalidAudioLimits);
        }
        let grid = self.transform()?.grid(rule)?;
        let count = grid.boundary(self.support.end)?;
        if samples.start.0 < 0 || samples.end < samples.start || samples.end > count {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let mut budget = Budget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut spans = Vec::new();
        let mut cursor = samples.start;
        while cursor < samples.end {
            if spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("span count"));
            }
            let mut span = self.span(cursor, grid, stop_at_preserve, &mut budget)?;
            span.samples = cursor..span.allocated_samples.end.min(samples.end);
            cursor = span.samples.end;
            spans.push(span);
        }
        Ok(AudioSignalQuery {
            project_id: self.plan.metadata.project_id.clone(),
            revision_id: self.plan.metadata.revision_id.clone(),
            samples,
            spans,
            lookup: budget.lookup,
        })
    }

    fn span(
        &self,
        sample: SignalSample,
        grid: AudioSampleGrid<SignalSample>,
        stop_at_preserve: bool,
        budget: &mut Budget,
    ) -> Result<AudioSignalSpan<'plan>, PlanError> {
        let mut transform = self.transform()?;
        let (probe, bias) = grid.probe(sample)?;
        let mut extent = self.support.clone();
        let mut sampling_extent = self.constrain_support.then(|| self.support.clone());
        let mut current = self.root;
        let mut repeats = self.repeats.clone();
        let mut retimes = Vec::new();
        let (content, gap_after) = loop {
            budget.spend(1)?;
            budget.lookup.visited_nodes += 1;
            let node = &self.plan.nodes[current];
            let node_extent = transform.signal_origin
                ..transform
                    .signal_from_local(ExactRatio::integer(node.inspection.duration.frames()))?;
            extent = intersect(extent, node_extent.clone())?;
            if !matches!(
                node.kind,
                CompiledKind::Retime {
                    purpose: RetimePurpose::Partition,
                    ..
                } | CompiledKind::Sequence { .. }
                    | CompiledKind::Repeat { .. }
            ) {
                sampling_extent = Some(match sampling_extent {
                    Some(previous) => intersect(previous, node_extent)?,
                    None => node_extent,
                });
            }
            let local = transform.local_at_signal_frame(probe)?;
            match &node.kind {
                CompiledKind::Source { audio: None, .. } => {
                    break (
                        AudioSignalContent::Leaf(AudioContent::Silence {
                            reason: SilenceReason::NoSourceAudio,
                        }),
                        None,
                    );
                }
                CompiledKind::Source {
                    audio: Some(audio), ..
                } => {
                    let start = transform.signal_from_local(audio.start)?;
                    let end =
                        transform.signal_from_local(audio.start.checked_add(audio.duration)?)?;
                    let content = if sample < grid.boundary(start)? {
                        extent.end = minimum(extent.end, start)?;
                        AudioContent::Silence {
                            reason: SilenceReason::OutsideSourcePlacement,
                        }
                    } else if sample >= grid.boundary(end)? {
                        extent.start = maximum(extent.start, end)?;
                        AudioContent::Silence {
                            reason: SilenceReason::OutsideSourcePlacement,
                        }
                    } else {
                        extent = intersect(extent, start..end)?;
                        let support = intersect(
                            sampling_extent
                                .clone()
                                .ok_or(PlanError::InvalidPlan("source has no sampling domain"))?,
                            start..end,
                        )?;
                        AudioContent::Source {
                            source: audio.source.clone(),
                            start: audio.start,
                            duration: audio.duration,
                            support: SourceSamplingSupport::from_local(
                                &audio.source,
                                audio.start,
                                audio.duration,
                                transform.local_at_signal_frame(support.start)?
                                    ..transform.local_at_signal_frame(support.end)?,
                            )?,
                        }
                    };
                    break (AudioSignalContent::Leaf(content), None);
                }
                CompiledKind::Hold { audio, .. } => {
                    break (
                        AudioSignalContent::Leaf(AudioContent::from_hold(
                            audio,
                            node.inspection.duration,
                        )),
                        None,
                    );
                }
                CompiledKind::Sequence { entries } => {
                    let mut left = 0;
                    let mut right = entries.len();
                    while left < right {
                        budget.spend(1)?;
                        budget.lookup.sequence_comparisons += 1;
                        let middle = left + (right - left) / 2;
                        let comparison = local.compare_integer(entries[middle].end);
                        let preceding = comparison.is_gt()
                            || (comparison.is_eq() && bias == InsertionBias::Right);
                        if preceding {
                            left = middle + 1;
                        } else {
                            right = middle;
                        }
                    }
                    let entry = entries
                        .get(left)
                        .ok_or(PlanError::InvalidPlan("audio signal sequence has no child"))?;
                    transform =
                        transform.child(ExactRatio::integer(entry.start), ExactRatio::ONE)?;
                    current = entry.child;
                }
                CompiledKind::Retime {
                    child,
                    start,
                    scale,
                    pitch,
                    purpose,
                } => {
                    if stop_at_preserve
                        && *pitch == PitchPolicy::Preserve
                        && *scale != ExactRatio::ONE
                    {
                        let descriptor = AudioStageDescriptor {
                            project_id: self.plan.metadata.project_id.clone(),
                            revision_id: self.plan.metadata.revision_id.clone(),
                            instance: InstancePath {
                                node: node.inspection.id.clone(),
                                repeats: repeats.clone(),
                            },
                            child: self.plan.nodes[*child].inspection.id.clone(),
                            selection: *start
                                ..start.checked_add(scale.checked_mul(ExactRatio::integer(
                                    node.inspection.duration.frames(),
                                ))?)?,
                            duration: node.inspection.duration,
                            rate: *scale,
                            pitch: *pitch,
                        };
                        break (
                            AudioSignalContent::Stage(AudioStage {
                                plan: self.plan,
                                child: *child,
                                node: current,
                                descriptor,
                            }),
                            None,
                        );
                    }
                    if *purpose != RetimePurpose::Partition {
                        retimes.push(AudioRetimeStage {
                            node: node.inspection.id.clone(),
                            child_start: *start,
                            child_frames_per_local_frame: *scale,
                            pitch: *pitch,
                        });
                    }
                    let inverse = ExactRatio::ONE.checked_div(*scale)?;
                    transform = transform.child(
                        ExactRatio::ZERO.checked_sub(start.checked_mul(inverse)?)?,
                        inverse,
                    )?;
                    current = *child;
                }
                CompiledKind::Repeat {
                    layout, gap_audio, ..
                } => {
                    let location = layout
                        .locate_bounded(local, bias, budget.remaining)
                        .map_err(|error| {
                            if error.code == deadpan_core::DocumentErrorCode::LimitExceeded {
                                PlanError::AudioQueryLimit("structural work")
                            } else {
                                error.into()
                            }
                        })?;
                    budget.spend(location.comparisons)?;
                    budget.lookup.iteration_run_comparisons += location.comparisons;
                    if location.in_gap {
                        let audio = gap_audio
                            .as_ref()
                            .ok_or(PlanError::InvalidPlan("audio signal repeat gap is missing"))?;
                        let start = location
                            .play
                            .start
                            .checked_add(location.play.duration.frames())
                            .ok_or(TimeError::Overflow)?;
                        transform = transform.child(ExactRatio::integer(start), ExactRatio::ONE)?;
                        extent = intersect(
                            extent,
                            transform.signal_origin
                                ..transform.signal_from_local(ExactRatio::integer(
                                    location.play.gap_after.frames(),
                                ))?,
                        )?;
                        break (
                            AudioSignalContent::Leaf(AudioContent::from_hold(
                                audio,
                                location.play.gap_after,
                            )),
                            Some(location.play.iteration),
                        );
                    }
                    transform = transform
                        .child(ExactRatio::integer(location.play.start), ExactRatio::ONE)?;
                    repeats.push(RepeatInstance {
                        node: node.inspection.id.clone(),
                        iteration: location.play.iteration,
                    });
                    current = self.plan.by_id[&location.play.child];
                }
            }
        };
        let allocated_samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
        if !allocated_samples.contains(&sample) {
            return Err(PlanError::InvalidPlan(
                "audio signal interval did not advance",
            ));
        }
        let sampling = AudioSampleMap::new(
            allocated_samples.start,
            transform.local_at(allocated_samples.start)?,
            transform
                .signal_frames_per_sample
                .checked_div(transform.signal_frames_per_local_frame)?,
        )?;
        Ok(AudioSignalSpan {
            samples: allocated_samples.clone(),
            allocated_samples,
            signal_extent: extent,
            instance: InstancePath {
                node: self.plan.nodes[current].inspection.id.clone(),
                repeats,
            },
            gap_after,
            transform,
            grid,
            sampling,
            retimes,
            content,
        })
    }
}

fn source_point(
    content: &AudioSignalContent<'_>,
    local: ExactRatio,
) -> Result<SourcePoint, PlanError> {
    let AudioSignalContent::Leaf(AudioContent::Source {
        source,
        start,
        duration,
        ..
    }) = content
    else {
        return Err(PlanError::NoSourceAudio);
    };
    Ok(source_point_from_local(source, *start, *duration, local)?)
}
