use std::ops::Range;

use deadpan_core::{
    AudioSample, ExactRatio, FrameDuration, HoldAudio, InsertionBias, InstancePath, IterationId,
    MIX_SAMPLE_RATE, NodeId, PitchPolicy, ProjectFrame, ProjectId, RepeatInstance, RevisionId,
    SourceAudio, SourcePoint, TimeError,
};
use serde::Serialize;

use super::audio_boundary::{AudioExtent, BoundaryOwner};
use super::{AudioBoundaries, AudioBoundaryKind};
use super::{CompiledKind, LookupStats, RenderPlan};
use crate::PlanError;

/// Hard caps bound both returned occurrence paths and structural search work.
/// A caller needing more material pages through contiguous sample ranges.
#[derive(Debug, Clone, Copy)]
pub struct AudioQueryLimits {
    pub maximum_spans: usize,
    pub maximum_work: usize,
}

impl Default for AudioQueryLimits {
    fn default() -> Self {
        Self {
            maximum_spans: 4096,
            maximum_work: 65_536,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SilenceReason {
    NoSourceAudio,
    OutsideSourcePlacement,
    /// Unlike an absent source voice, an authored silent Hold suppresses tails.
    SilentHold,
}

/// Authored instructions, not decoded PCM or a claim of supported DSP effects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AudioContent {
    Silence {
        reason: SilenceReason,
    },
    Source {
        source: SourceAudio,
        /// Exact local-frame placement after the independent 48 kHz offset.
        start: ExactRatio,
        duration: ExactRatio,
    },
    /// Retained user-selected source range; looping/crossfades belong to DSP.
    RoomTone {
        source: SourceAudio,
        /// Full intrinsic Hold duration in its local clock, before any crop.
        duration: FrameDuration,
    },
    /// Retain the policy and maximum in the Hold's local clock. A renderer must
    /// implement this explicitly, never substitute ordinary speech or silence.
    Tail {
        source: SourceAudio,
        maximum: FrameDuration,
    },
}

impl AudioContent {
    pub(super) fn from_hold(value: &HoldAudio, duration: FrameDuration) -> Self {
        match value {
            HoldAudio::Silence => Self::Silence {
                reason: SilenceReason::SilentHold,
            },
            HoldAudio::RoomTone { source } => Self::RoomTone {
                source: source.clone(),
                duration,
            },
            HoldAudio::Tail { source, maximum } => Self::Tail {
                source: source.clone(),
                maximum: *maximum,
            },
        }
    }
}

/// Mapping from the leaf's local frame clock to absolute project frames.
/// It stays exact even when the allocated sample interval rounds either edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AudioTransform {
    pub project_origin: ExactRatio,
    pub project_frames_per_local_frame: ExactRatio,
    pub project_frames_per_sample: ExactRatio,
}

impl AudioTransform {
    fn project_at(self, local: ExactRatio) -> Result<ExactRatio, TimeError> {
        self.project_origin
            .checked_add(local.checked_mul(self.project_frames_per_local_frame)?)
    }

    pub fn local_at(self, sample: AudioSample) -> Result<ExactRatio, TimeError> {
        ExactRatio::integer(sample.0)
            .checked_mul(self.project_frames_per_sample)?
            .checked_sub(self.project_origin)?
            .checked_div(self.project_frames_per_local_frame)
    }

    fn sample_boundary(self, project: ExactRatio) -> Result<AudioSample, TimeError> {
        let value = project
            .checked_div(self.project_frames_per_sample)?
            .round_even()?;
        Ok(AudioSample(
            i64::try_from(value).map_err(|_| TimeError::Overflow)?,
        ))
    }

    fn child(self, start: ExactRatio, scale: ExactRatio) -> Result<Self, TimeError> {
        Ok(Self {
            project_origin: self.project_at(start)?,
            project_frames_per_local_frame: self
                .project_frames_per_local_frame
                .checked_mul(scale)?,
            ..self
        })
    }
}

/// Outer-to-inner authored stages. Mixed Preserve/FollowSpeed policies must not
/// disappear when the exact timing affine maps are composed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioRetimeStage {
    pub node: NodeId,
    pub child_start: ExactRatio,
    pub child_frames_per_local_frame: ExactRatio,
    pub pitch: PitchPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioSpan {
    /// Portion requested by this query. All spans partition the query exactly.
    pub samples: Range<AudioSample>,
    /// Full interval after structural/placement clipping, independent of query
    /// partitioning. Both endpoints are rounded once from the project origin.
    pub allocated_samples: Range<AudioSample>,
    pub project_extent: Range<ExactRatio>,
    /// Original owners of each full extent edge, never of a query crop.
    pub boundaries: AudioBoundaries,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub transform: AudioTransform,
    pub retimes: Vec<AudioRetimeStage>,
    pub content: AudioContent,
}

impl AudioSpan {
    /// Exact original source coordinate at an output sample boundary. Rounding
    /// can place an edge sample slightly outside the authored source interval;
    /// this method does not clamp it or authorize reading excluded source PCM.
    pub fn source_point(&self, sample: AudioSample) -> Result<SourcePoint, PlanError> {
        self.source_point_from_local(self.transform.local_at(sample)?)
    }

    /// Exact source coordinate at a project-frame coordinate, including a
    /// fractional clipped edge. This is distinct from the rounded allocation
    /// of output samples and does not authorize reading excluded source PCM.
    pub fn source_point_at_project_frame(
        &self,
        frame: ExactRatio,
    ) -> Result<SourcePoint, PlanError> {
        self.source_point_from_local(
            frame
                .checked_sub(self.transform.project_origin)?
                .checked_div(self.transform.project_frames_per_local_frame)?,
        )
    }

    fn source_point_from_local(&self, local: ExactRatio) -> Result<SourcePoint, PlanError> {
        let AudioContent::Source {
            source,
            start,
            duration,
        } = &self.content
        else {
            return Err(PlanError::NoSourceAudio);
        };
        let fraction = local.checked_sub(*start)?.checked_div(*duration)?;
        let ticks =
            ExactRatio::integer(source.span.start().ticks).checked_add(fraction.checked_mul(
                ExactRatio::integer(source.span.end().ticks - source.span.start().ticks),
            )?)?;
        Ok(SourcePoint {
            ticks,
            time_base: source.span.start().time_base,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioQuery {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub samples: Range<AudioSample>,
    pub spans: Vec<AudioSpan>,
    pub lookup: LookupStats,
}

pub(super) struct Budget {
    pub(super) remaining: usize,
    pub(super) lookup: LookupStats,
}

impl Budget {
    pub(super) fn spend(&mut self, amount: usize) -> Result<(), PlanError> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or(PlanError::AudioQueryLimit("structural work"))?;
        Ok(())
    }
}

impl RenderPlan {
    pub fn audio_duration(&self) -> Result<AudioSample, PlanError> {
        Ok(self
            .metadata
            .presentation_basis
            .frame_rate
            .audio_boundary(ProjectFrame(self.duration().frames()))?)
    }

    /// Bounded interval lookup. Each returned span is resolved by indexed descent
    /// at the next unfilled sample, so skipped repeat plays are never expanded.
    /// No decoding, channel inference, DSP, or media I/O happens here.
    pub fn audio(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioQuery, PlanError> {
        if limits.maximum_spans == 0
            || limits.maximum_spans > 4096
            || limits.maximum_work == 0
            || limits.maximum_work > 65_536
        {
            return Err(PlanError::InvalidAudioLimits);
        }
        if samples.start.0 < 0
            || samples.end < samples.start
            || samples.end > self.audio_duration()?
        {
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
            let mut span = self.audio_span(cursor, &mut budget)?;
            span.samples = cursor..span.allocated_samples.end.min(samples.end);
            cursor = span.samples.end;
            spans.push(span);
        }
        Ok(AudioQuery {
            project_id: self.metadata.project_id.clone(),
            revision_id: self.metadata.revision_id.clone(),
            samples,
            spans,
            lookup: budget.lookup,
        })
    }

    fn audio_span(&self, sample: AudioSample, budget: &mut Budget) -> Result<AudioSpan, PlanError> {
        let rate = self.metadata.presentation_basis.frame_rate;
        let mut transform = AudioTransform {
            project_origin: ExactRatio::ZERO,
            project_frames_per_local_frame: ExactRatio::integer(1),
            project_frames_per_sample: ExactRatio::new(
                i128::from(rate.numerator()),
                i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
            )?,
        };
        // round_even(edge) > n iff edge > n+1/2, or edge == n+1/2
        // and n is odd. This selects the allocated half-open sample interval,
        // including ties, even when a retime compresses millions of leaves below
        // one sample. Sampling at n itself would choose the wrong leaf.
        let probe = ExactRatio::new(i128::from(sample.0) * 2 + 1, 2)?
            .checked_mul(transform.project_frames_per_sample)?;
        let bias = if sample.0 % 2 == 0 {
            InsertionBias::Right
        } else {
            InsertionBias::Left
        };
        let mut extent =
            AudioExtent::new(ExactRatio::ZERO..ExactRatio::integer(self.duration().frames()));
        let mut current = self.root;
        let mut repeats = Vec::new();
        let mut retimes = Vec::new();
        let (content, gap_after) = loop {
            budget.spend(1)?;
            budget.lookup.visited_nodes += 1;
            let node = &self.nodes[current];
            let owner = BoundaryOwner {
                node: &node.inspection.id,
                repeats: &repeats,
                gap_after: None,
                policies: node.audio_edges,
            };
            extent.intersect(
                transform.project_origin
                    ..transform
                        .project_at(ExactRatio::integer(node.inspection.duration.frames()))?,
                owner,
                (AudioBoundaryKind::NodeStart, AudioBoundaryKind::NodeEnd),
                budget,
            )?;
            let local = probe
                .checked_sub(transform.project_origin)?
                .checked_div(transform.project_frames_per_local_frame)?;
            match &node.kind {
                CompiledKind::Source { audio: None, .. } => {
                    break (
                        AudioContent::Silence {
                            reason: SilenceReason::NoSourceAudio,
                        },
                        None,
                    );
                }
                CompiledKind::Source {
                    audio: Some(audio), ..
                } => {
                    let start = transform.project_at(audio.start)?;
                    let end = transform.project_at(audio.start.checked_add(audio.duration)?)?;
                    let content = if sample < transform.sample_boundary(start)? {
                        extent.clip_end(
                            start,
                            owner,
                            AudioBoundaryKind::SourcePlacementStart,
                            budget,
                        )?;
                        AudioContent::Silence {
                            reason: SilenceReason::OutsideSourcePlacement,
                        }
                    } else if sample >= transform.sample_boundary(end)? {
                        extent.clip_start(
                            end,
                            owner,
                            AudioBoundaryKind::SourcePlacementEnd,
                            budget,
                        )?;
                        AudioContent::Silence {
                            reason: SilenceReason::OutsideSourcePlacement,
                        }
                    } else {
                        extent.intersect(
                            start..end,
                            owner,
                            (
                                AudioBoundaryKind::SourcePlacementStart,
                                AudioBoundaryKind::SourcePlacementEnd,
                            ),
                            budget,
                        )?;
                        AudioContent::Source {
                            source: audio.source.clone(),
                            start: audio.start,
                            duration: audio.duration,
                        }
                    };
                    break (content, None);
                }
                CompiledKind::Hold { audio, .. } => {
                    break (
                        AudioContent::from_hold(audio, node.inspection.duration),
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
                        .ok_or(PlanError::InvalidPlan("audio sequence has no child"))?;
                    transform = transform
                        .child(ExactRatio::integer(entry.start), ExactRatio::integer(1))?;
                    current = entry.child;
                }
                CompiledKind::Retime {
                    child,
                    start,
                    scale,
                    pitch,
                } => {
                    retimes.push(AudioRetimeStage {
                        node: node.inspection.id.clone(),
                        child_start: *start,
                        child_frames_per_local_frame: *scale,
                        pitch: *pitch,
                    });
                    let inverse = ExactRatio::integer(1).checked_div(*scale)?;
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
                            .ok_or(PlanError::InvalidPlan("audio repeat gap is missing"))?;
                        let start = location
                            .play
                            .start
                            .checked_add(location.play.duration.frames())
                            .ok_or(TimeError::Overflow)?;
                        transform =
                            transform.child(ExactRatio::integer(start), ExactRatio::integer(1))?;
                        extent.intersect(
                            transform.project_origin
                                ..transform.project_at(ExactRatio::integer(
                                    location.play.gap_after.frames(),
                                ))?,
                            BoundaryOwner {
                                gap_after: Some(&location.play.iteration),
                                ..owner
                            },
                            (
                                AudioBoundaryKind::RepeatGapStart,
                                AudioBoundaryKind::RepeatGapEnd,
                            ),
                            budget,
                        )?;
                        break (
                            AudioContent::from_hold(audio, location.play.gap_after),
                            Some(location.play.iteration),
                        );
                    }
                    transform = transform.child(
                        ExactRatio::integer(location.play.start),
                        ExactRatio::integer(1),
                    )?;
                    repeats.push(RepeatInstance {
                        node: node.inspection.id.clone(),
                        iteration: location.play.iteration,
                    });
                    current = self.by_id[&location.play.child];
                }
            }
        };
        let allocated_samples = transform.sample_boundary(extent.range.start)?
            ..transform.sample_boundary(extent.range.end)?;
        if !allocated_samples.contains(&sample) {
            return Err(PlanError::InvalidPlan("audio interval did not advance"));
        }
        Ok(AudioSpan {
            samples: allocated_samples.clone(),
            allocated_samples,
            project_extent: extent.range,
            boundaries: extent.boundaries,
            instance: InstancePath {
                node: self.nodes[current].inspection.id.clone(),
                repeats,
            },
            gap_after,
            transform,
            retimes,
            content,
        })
    }
}

pub(super) fn minimum(a: ExactRatio, b: ExactRatio) -> Result<ExactRatio, TimeError> {
    Ok(if a.checked_sub(b)?.compare_integer(0).is_le() {
        a
    } else {
        b
    })
}
pub(super) fn maximum(a: ExactRatio, b: ExactRatio) -> Result<ExactRatio, TimeError> {
    Ok(if a.checked_sub(b)?.compare_integer(0).is_ge() {
        a
    } else {
        b
    })
}
pub(super) fn intersect(
    a: Range<ExactRatio>,
    b: Range<ExactRatio>,
) -> Result<Range<ExactRatio>, TimeError> {
    Ok(maximum(a.start, b.start)?..minimum(a.end, b.end)?)
}
