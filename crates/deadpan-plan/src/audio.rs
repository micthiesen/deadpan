use std::ops::Range;

use deadpan_core::{
    AudioSample, ExactRatio, FrameDuration, HoldAudio, InsertionBias, InstancePath, IterationId,
    MIX_SAMPLE_RATE, NodeId, PitchPolicy, ProjectFrame, ProjectId, RepeatInstance, RetimePurpose,
    RevisionId, SourceAudio, SourcePoint, TailEffect, TimeError,
};
use serde::Serialize;

use super::audio_bound::BindingTarget;
use super::audio_boundary::BoundaryOwner;
use super::audio_domain::{AudioDomainSeed, AudioWalkSeed, DomainGap};
use super::{AudioBoundaries, AudioBoundaryKind, AudioDefinitionSelector};
use super::{AudioProcessingSpan, AudioSignalContent, AudioStage};
use super::{CompiledKind, LookupStats, RenderPlan};
use crate::{AudioBoundaryRule, AudioEnvelope, AudioSampleGrid, AudioSampleMap, PlanError};

#[path = "audio_capture.rs"]
pub(in crate::plan) mod capture;

/// Hard caps bound both returned occurrence paths and structural search work.
/// A caller needing more material pages through contiguous sample ranges.
#[derive(Debug, Clone, Copy)]
pub struct AudioQueryLimits {
    pub maximum_spans: usize,
    pub maximum_work: usize,
}

impl AudioQueryLimits {
    pub fn validate(self) -> Result<(), PlanError> {
        if self.maximum_spans == 0
            || self.maximum_spans > 4096
            || self.maximum_work == 0
            || self.maximum_work > 65_536
        {
            return Err(PlanError::InvalidAudioLimits);
        }
        Ok(())
    }
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
    /// An authored Source crop. Its endpoints constrain physical sampling,
    /// but do not suppress the decay of a downstream Preserve stage.
    OutsideSourceSelection,
    /// Unlike an absent source voice, an authored silent Hold suppresses tails.
    SilentHold,
}

/// Exact source-clock filter support. Transparent partitions crop allocation,
/// not this domain; ordinary authored crops continue to exclude outside taps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceSamplingSupport {
    pub start: SourcePoint,
    pub end: SourcePoint,
}

impl SourceSamplingSupport {
    pub(super) fn from_local(
        source: &SourceAudio,
        start: ExactRatio,
        duration: ExactRatio,
        support: Range<ExactRatio>,
    ) -> Result<Self, TimeError> {
        Ok(Self {
            start: source_point_from_local(source, start, duration, support.start)?,
            end: source_point_from_local(source, start, duration, support.end)?,
        })
    }
}

pub(super) fn source_point_from_local(
    source: &SourceAudio,
    start: ExactRatio,
    duration: ExactRatio,
    local: ExactRatio,
) -> Result<SourcePoint, TimeError> {
    let fraction = local.checked_sub(start)?.checked_div(duration)?;
    let ticks =
        ExactRatio::integer(source.span.start().ticks).checked_add(fraction.checked_mul(
            ExactRatio::integer(source.span.end().ticks - source.span.start().ticks),
        )?)?;
    Ok(SourcePoint {
        ticks,
        time_base: source.span.start().time_base,
    })
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
        /// Full measured-stream affine origin after the independent 48 kHz
        /// offset. A selected audible window never restarts this phase.
        start: ExactRatio,
        duration: ExactRatio,
        support: SourceSamplingSupport,
    },
    /// Retained user-selected source range; looping/crossfades belong to DSP.
    RoomTone {
        source: SourceAudio,
        /// Full intrinsic Hold duration in its local clock, before any crop.
        duration: FrameDuration,
    },
    /// Retain the policy and maximum in the Hold's local clock. A renderer must
    /// implement this explicitly, never substitute ordinary speech or silence.
    /// Live: the renderer feeds the effect with the processed Original sound
    /// heard over two seconds before this occurrence's root position.
    Tail {
        maximum: FrameDuration,
        effect: TailEffect,
        input: deadpan_core::TailInput,
        /// Full intrinsic Hold duration in its local clock, before any crop.
        duration: FrameDuration,
    },
    /// The source span played backwards from the Hold's local start, then
    /// silence. `duration` is the full intrinsic Hold duration, before crops.
    Reverse {
        source: SourceAudio,
        duration: FrameDuration,
    },
    /// A synthesized sine for the whole intrinsic Hold duration.
    Tone {
        frequency_hz: u32,
        level: deadpan_core::GainDb,
        duration: FrameDuration,
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
            HoldAudio::Tail {
                maximum,
                effect,
                input,
            } => Self::Tail {
                maximum: *maximum,
                effect: *effect,
                input: *input,
                duration,
            },
            HoldAudio::Reverse { source } => Self::Reverse {
                source: source.clone(),
                duration,
            },
            HoldAudio::Tone {
                frequency_hz,
                level,
            } => Self::Tone {
                frequency_hz: *frequency_hz,
                level: *level,
                duration,
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
    /// Relative instance paths are scoped to this definition when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<AudioDefinitionSelector>,
    /// Portion requested by this query. All spans partition the query exactly.
    pub samples: Range<AudioSample>,
    /// Full interval after structural/placement clipping, independent of query
    /// partitioning. Both endpoints are rounded once from the project origin.
    pub allocated_samples: Range<AudioSample>,
    pub project_extent: Range<ExactRatio>,
    /// Meaningful envelope domain, which can extend beyond a transparent
    /// partition's allocation. Its original length determines fade width.
    pub envelope_extent: Range<ExactRatio>,
    pub envelope_samples: Range<AudioSample>,
    /// Meaningful fade progress, independent of current allocation. A resumed
    /// fragment can exhaust this retained envelope before its new allocation ends.
    pub envelope: AudioEnvelope,
    /// Owners of the envelope edges, never of a query or transparent partition.
    pub boundaries: AudioBoundaries,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub transform: AudioTransform,
    pub grid: AudioSampleGrid<AudioSample>,
    pub sampling: AudioSampleMap<AudioSample>,
    pub retimes: Vec<AudioRetimeStage>,
    pub content: AudioContent,
}

impl AudioSpan {
    /// Exact original source coordinate at an output sample boundary. Rounding
    /// can place an edge sample slightly outside the authored source interval;
    /// this method does not clamp it or authorize reading excluded source PCM.
    pub fn source_point(&self, sample: AudioSample) -> Result<SourcePoint, PlanError> {
        self.source_point_from_local(self.sampling.local_at(sample)?)
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
            ..
        } = &self.content
        else {
            return Err(PlanError::NoSourceAudio);
        };
        Ok(source_point_from_local(source, *start, *duration, local)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioQuery {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub samples: Range<AudioSample>,
    pub spans: Vec<AudioSpan>,
    pub lookup: LookupStats,
    #[serde(skip)]
    pub work: usize,
}

pub(super) struct Budget {
    pub(super) remaining: usize,
    pub(super) lookup: LookupStats,
}

// Sequence/Repeat bounds describe derived allocation, not a new crop of a
// retained partition. Keep their policies only when a real envelope edge is
// exactly coincident. Capture paths after descent to avoid cloning every prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EnvelopeConstraint {
    pub(super) placement_support: bool,
    pub(super) range: Range<ExactRatio>,
    pub(super) node: usize,
    pub(super) repeat_count: usize,
    pub(super) gap_after: Option<IterationId>,
    pub(super) kinds: (AudioBoundaryKind, AudioBoundaryKind),
}

/// Creative boundaries use the current output clock. They never restrict raw
/// support or the retained sampling/envelope domain.
#[derive(Debug, Clone)]
pub(super) struct EditorialConstraint {
    pub(super) boundary: EnvelopeConstraint,
    pub(super) edges: deadpan_core::AudioEditorialEdges,
    /// Only transported outer markers use sample probes. A resumed binding can
    /// deliver a different incident inner voice at the authored boundary.
    pub(super) incident_samples: Option<(ExactRatio, ExactRatio)>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct EditorialCapture {
    pub(super) editorial: Vec<EditorialConstraint>,
    pub(super) constraints: Vec<EnvelopeConstraint>,
    pub(super) repeats: Vec<RepeatInstance>,
}

pub(super) enum AudioWalkSpan<'plan> {
    Leaf(AudioSpan),
    Stage(AudioProcessingSpan<'plan>),
}

impl<'plan> AudioWalkSpan<'plan> {
    pub(super) fn into_processing(self) -> AudioProcessingSpan<'plan> {
        match self {
            Self::Stage(span) => span,
            Self::Leaf(span) => AudioProcessingSpan {
                definition: span.definition,
                samples: span.samples,
                allocated_samples: span.allocated_samples,
                project_extent: span.project_extent,
                instance: span.instance,
                gap_after: span.gap_after,
                transform: span.transform,
                grid: span.grid,
                sampling: span.sampling,
                retimes: span.retimes,
                content: AudioSignalContent::Leaf(span.content),
            },
        }
    }
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
        limits.validate()?;
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
            work: limits.maximum_work - budget.remaining,
        })
    }

    fn audio_span(&self, sample: AudioSample, budget: &mut Budget) -> Result<AudioSpan, PlanError> {
        match self.audio_walk(sample, budget, None, false, false, None)? {
            AudioWalkSpan::Leaf(span) => Ok(span),
            AudioWalkSpan::Stage(_) => {
                Err(PlanError::InvalidPlan("flattened audio retained a stage"))
            }
        }
    }

    pub(super) fn audio_walk<'plan>(
        &'plan self,
        sample: AudioSample,
        budget: &mut Budget,
        seed: Option<&AudioWalkSeed>,
        stop_at_preserve: bool,
        stop_at_bindings: bool,
        capture: Option<&mut Option<AudioDomainSeed>>,
    ) -> Result<AudioWalkSpan<'plan>, PlanError> {
        self.audio_walk_inner(
            sample,
            budget,
            seed,
            stop_at_preserve,
            stop_at_bindings,
            (capture, None, None),
        )
    }

    pub(super) fn audio_walk_editorial<'plan>(
        &'plan self,
        sample: AudioSample,
        budget: &mut Budget,
        seed: Option<&AudioWalkSeed>,
        stops: (bool, bool),
        editorial: &mut EditorialCapture,
    ) -> Result<AudioWalkSpan<'plan>, PlanError> {
        self.audio_walk_inner(
            sample,
            budget,
            seed,
            stops.0,
            stops.1,
            (None, Some(editorial), None),
        )
    }

    pub(super) fn audio_walk_owner_input(
        &self,
        sample: AudioSample,
        budget: &mut Budget,
        owner: &super::AudioOwnerClock<'_>,
        capture: &mut Option<AudioDomainSeed>,
        seed: Option<&AudioWalkSeed>,
    ) -> Result<(), PlanError> {
        #[cfg(test)]
        let before = budget.remaining;
        let walked = self.audio_walk_inner(
            sample,
            budget,
            seed,
            true,
            true,
            (Some(&mut *capture), None, Some(owner)),
        );
        #[cfg(test)]
        super::audio_owners::tests::legacy_snapshot_v1::capture_baseline::capture(
            self,
            sample,
            owner,
            capture.as_ref(),
            &walked,
            (before, budget.remaining),
        );
        // Preserve the actual complete walk and its original discard point.
        walked?;
        Ok(())
    }

    fn audio_walk_inner<'plan>(
        &'plan self,
        sample: AudioSample,
        budget: &mut Budget,
        seed: Option<&AudioWalkSeed>,
        stop_at_preserve: bool,
        stop_at_bindings: bool,
        captures: (
            Option<&mut Option<AudioDomainSeed>>,
            Option<&mut EditorialCapture>,
            Option<&super::AudioOwnerClock<'_>>,
        ),
    ) -> Result<AudioWalkSpan<'plan>, PlanError> {
        use super::audio_owners::admission::{self, Memory, Owned, Unmetered};
        let (mut capture, editorial, owner) = captures;
        let mut slot = Owned {
            value: capture.as_deref_mut().and_then(Option::take),
            memory: Memory::empty(()),
        };
        let mut policy = Unmetered;
        let mut admitted = admission::Budget::new(budget, &mut policy);
        let result = self.audio_walk_inner_admitted(
            sample,
            &mut admitted,
            seed,
            stop_at_preserve,
            stop_at_bindings,
            (
                if capture.is_some() {
                    Some(&mut slot)
                } else {
                    None
                },
                editorial,
                owner,
            ),
        );
        if let Some(capture) = capture {
            *capture = slot.value;
        }
        admission::outer::<_, Unmetered>(result.map(capture::WalkResult::unmetered))
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
