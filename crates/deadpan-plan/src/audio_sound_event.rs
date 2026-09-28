//! Authored root sound recipes evaluated on the actual project sample grid.

use std::ops::Range;

use deadpan_core::{
    AudioEdgePolicy, AudioSample, ExactRatio, FrameDuration, MIX_SAMPLE_RATE, SoundEvent, SoundId,
    TimeError,
};

use crate::{
    AudioBoundaryRule, AudioContent, AudioQueryLimits, AudioSampleGrid, AudioSignalTape,
    AudioSignalTapeRun, AudioSourceVoiceRecipe, PlanError, RenderPlan, SilenceReason,
};

/// A checked authored sound on the absolute RoundEven grid. The private tape
/// adapter evaluates source phase and Hold policy directly on that grid; it
/// never converts a rendered PointCeil sample array by relabeling its indices.
#[derive(Debug, Clone)]
pub struct AudioRootSound<'plan> {
    plan: &'plan RenderPlan,
    event: &'plan SoundEvent,
    output: AudioSignalTape<'plan>,
    audible: Range<AudioSample>,
    selection: Range<ExactRatio>,
    grid: AudioSampleGrid<AudioSample>,
    start_edge: AudioEdgePolicy,
    end_edge: AudioEdgePolicy,
}

/// One combined event/gate envelope. Remote edges may use equivalent bounded
/// context, but query cuts never change its evaluated gain. Zero length denotes
/// silence; one surviving sample has unity edge gain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSoundGateSpan {
    pub samples: Range<AudioSample>,
    pub length: u64,
    pub progress_at_start: u64,
    pub start_edge: AudioEdgePolicy,
    pub end_edge: AudioEdgePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSoundGateQuery {
    pub spans: Vec<AudioSoundGateSpan>,
    pub work: usize,
}

// F=min(96,N/2) needs twice the maximum fade width to discover short islands.
const GATE_CONTEXT_SAMPLES: i64 = 192;

impl RenderPlan {
    pub fn root_sound(&self, id: &SoundId) -> Result<AudioRootSound<'_>, PlanError> {
        let event = self.sounds().get(id).ok_or(PlanError::InvalidPlan(
            "authored sound does not exist in this plan",
        ))?;
        if event.owner != self.metadata().root {
            return Err(PlanError::InvalidPlan("sound owner is not the root"));
        }
        let rate = self.metadata().presentation_basis.frame_rate;
        let step = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let grid = AudioSampleGrid::<AudioSample>::new(
            ExactRatio::ZERO,
            step,
            AudioBoundaryRule::RoundEven,
        )?;
        let selection =
            event
                .mapping
                .selection_frames_with_offset(FrameDuration::ZERO, event.offset, rate)?;
        let audible = grid.boundary(selection.start)?..grid.boundary(selection.end)?;
        let owner_edges = self.root_audio_edges();
        let start_edge = if selection.start == ExactRatio::ZERO
            && owner_edges.node_start == AudioEdgePolicy::Hard
        {
            AudioEdgePolicy::Hard
        } else {
            event.start_edge
        };
        let end_edge = if selection.end == ExactRatio::integer(self.duration().frames())
            && owner_edges.node_end == AudioEdgePolicy::Hard
        {
            AudioEdgePolicy::Hard
        } else {
            event.end_edge
        };
        let full = ExactRatio::ZERO..ExactRatio::integer(self.duration().frames());
        let voice = self.audio_signal().source_voice(AudioSourceVoiceRecipe {
            source: event.source.clone(),
            mapping: event.mapping,
            offset: event.offset,
        })?;
        let tape = AudioSignalTape::new(
            self,
            full.clone(),
            vec![AudioSignalTapeRun::new(
                full.clone(),
                full.clone(),
                voice.output_signal(),
            )],
        )?;
        let output = tape.remap_policy_window(
            full.clone(),
            full,
            ExactRatio::ZERO,
            step,
            AudioBoundaryRule::RoundEven,
        )?;
        Ok(AudioRootSound {
            plan: self,
            event,
            output,
            audible,
            selection: selection.start..selection.end,
            grid,
            start_edge,
            end_edge,
        })
    }
}

impl<'plan> AudioRootSound<'plan> {
    pub fn event(&self) -> &'plan SoundEvent {
        self.event
    }

    pub fn audible_samples(&self) -> Range<AudioSample> {
        self.audible.clone()
    }

    /// Internal preparation carrier: its labels are absolute root samples and
    /// its boundary rule is RoundEven despite the tape's generic signal label.
    /// Consumers must not use intrinsic PointCeil allocation for this view.
    pub fn root_output_tape(&self) -> &AudioSignalTape<'plan> {
        &self.output
    }

    /// Combine this event's edges with current silent-Hold gates. Original cuts
    /// never create a sound edge. Intersections use exact frames before rounding;
    /// only exactly coincident Hard choices override an automatic boundary.
    ///
    /// The query inspects at most 192 output samples of context on either side.
    /// Synthetic context boundaries never fade, and cannot shorten a 96-sample
    /// fade inside the returned interval. Work includes structural lookup and
    /// envelope projection. Returned spans partition the requested interval.
    pub fn gate_fades(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioSoundGateQuery, PlanError> {
        limits.validate()?;
        let root_end = self.plan.audio_duration()?;
        if samples.start.0 < 0 || samples.end < samples.start || samples.end > root_end {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        if samples.is_empty() {
            return Ok(AudioSoundGateQuery {
                spans: Vec::new(),
                work: 0,
            });
        }
        if self.audible.start >= samples.end || self.audible.end <= samples.start {
            return Ok(AudioSoundGateQuery {
                spans: vec![silent(samples)],
                work: 1,
            });
        }
        let context = AudioSample(samples.start.0.saturating_sub(GATE_CONTEXT_SAMPLES).max(0))
            ..AudioSample(
                samples
                    .end
                    .0
                    .saturating_add(GATE_CONTEXT_SAMPLES)
                    .min(root_end.0),
            );
        let context_start = self.grid.at(context.start)?;
        let context_end = self.grid.at(context.end)?;
        let mut left = Boundary {
            at: self.selection.start,
            policy: self.start_edge,
        };
        let mut right = Boundary {
            at: self.selection.end,
            policy: self.end_edge,
        };
        if self.audible.start < context.start {
            left = Boundary {
                at: context_start,
                policy: AudioEdgePolicy::Hard,
            };
        }
        if self.audible.end > context.end {
            right = Boundary {
                at: context_end,
                policy: AudioEdgePolicy::Hard,
            };
        }
        let query = self.plan.audio(
            context,
            AudioQueryLimits {
                // Halo spans own at least one sample. They share the work limit,
                // without consuming the caller's returned-span allowance.
                maximum_spans: limits.maximum_spans.saturating_add(384).min(4096),
                maximum_work: limits.maximum_work,
            },
        )?;
        let mut work = query.work;
        let mut gates: Vec<Gate> = Vec::new();
        for span in query.spans {
            charge(&mut work, limits.maximum_work)?;
            if !matches!(
                span.content,
                AudioContent::Silence {
                    reason: SilenceReason::SilentHold
                }
            ) {
                continue;
            }
            // Retained envelope provenance applies only at the same exact
            // current boundary. A transparent partition is not a new recipe.
            let start = Boundary {
                at: span.project_extent.start,
                policy: if span.project_extent.start == span.envelope_extent.start
                    && span
                        .boundaries
                        .start
                        .iter()
                        .any(|origin| origin.policy == AudioEdgePolicy::Hard)
                {
                    AudioEdgePolicy::Hard
                } else {
                    AudioEdgePolicy::Automatic
                },
            };
            let end = Boundary {
                at: span.project_extent.end,
                policy: if span.project_extent.end == span.envelope_extent.end
                    && span
                        .boundaries
                        .end
                        .iter()
                        .any(|origin| origin.policy == AudioEdgePolicy::Hard)
                {
                    AudioEdgePolicy::Hard
                } else {
                    AudioEdgePolicy::Automatic
                },
            };
            if let Some(previous) = gates.last_mut()
                && self.grid.boundary(start.at)? <= self.grid.boundary(previous.end.at)?
            {
                // A sampleless gap creates no audible island. Internal Hard
                // choices cannot override the outer gate boundaries.
                previous.end = end;
            } else {
                gates.push(Gate { start, end });
            }
        }
        let mut spans = Vec::new();
        let mut cursor = left;
        for gate in gates {
            charge(&mut work, limits.maximum_work)?;
            if less(gate.end.at, cursor.at)? {
                continue;
            }
            if less(right.at, gate.start.at)? {
                break;
            }
            let end = earlier(gate.start, right)?;
            self.append_envelope(&mut spans, &samples, cursor, end, limits, &mut work)?;
            cursor = later(cursor, gate.end)?;
            if !less(cursor.at, right.at)? {
                break;
            }
        }
        self.append_envelope(&mut spans, &samples, cursor, right, limits, &mut work)?;
        let end = spans.last().map_or(samples.start, |span| span.samples.end);
        if end < samples.end {
            push_span(&mut spans, silent(end..samples.end), limits, &mut work)?;
        }
        Ok(AudioSoundGateQuery { spans, work })
    }

    fn append_envelope(
        &self,
        spans: &mut Vec<AudioSoundGateSpan>,
        requested: &Range<AudioSample>,
        start: Boundary,
        end: Boundary,
        limits: AudioQueryLimits,
        work: &mut usize,
    ) -> Result<(), PlanError> {
        if !less(start.at, end.at)? {
            return Ok(());
        }
        let first = self.grid.boundary(start.at)?;
        let last = self.grid.boundary(end.at)?;
        let samples = first.max(requested.start)..last.min(requested.end);
        if samples.is_empty() {
            return Ok(());
        }
        let cursor = spans
            .last()
            .map_or(requested.start, |span| span.samples.end);
        if cursor < samples.start {
            push_span(spans, silent(cursor..samples.start), limits, work)?;
        }
        push_span(
            spans,
            AudioSoundGateSpan {
                progress_at_start: u64::try_from(samples.start.0 - first.0)
                    .map_err(|_| TimeError::Overflow)?,
                samples,
                length: u64::try_from(last.0 - first.0).map_err(|_| TimeError::Overflow)?,
                start_edge: start.policy,
                end_edge: end.policy,
            },
            limits,
            work,
        )
    }
}

#[derive(Clone, Copy)]
struct Boundary {
    at: ExactRatio,
    policy: AudioEdgePolicy,
}
struct Gate {
    start: Boundary,
    end: Boundary,
}

fn less(a: ExactRatio, b: ExactRatio) -> Result<bool, TimeError> {
    Ok(a.checked_sub(b)?.compare_integer(0).is_lt())
}
fn earlier(a: Boundary, b: Boundary) -> Result<Boundary, TimeError> {
    if less(a.at, b.at)? {
        Ok(a)
    } else if less(b.at, a.at)? {
        Ok(b)
    } else {
        Ok(coincident(a, b))
    }
}
fn later(a: Boundary, b: Boundary) -> Result<Boundary, TimeError> {
    if less(a.at, b.at)? {
        Ok(b)
    } else if less(b.at, a.at)? {
        Ok(a)
    } else {
        Ok(coincident(a, b))
    }
}
fn coincident(a: Boundary, b: Boundary) -> Boundary {
    Boundary {
        at: a.at,
        policy: if a.policy == AudioEdgePolicy::Hard || b.policy == AudioEdgePolicy::Hard {
            AudioEdgePolicy::Hard
        } else {
            AudioEdgePolicy::Automatic
        },
    }
}
fn silent(samples: Range<AudioSample>) -> AudioSoundGateSpan {
    AudioSoundGateSpan {
        samples,
        length: 0,
        progress_at_start: 0,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
    }
}
fn charge(work: &mut usize, maximum: usize) -> Result<(), PlanError> {
    *work = work
        .checked_add(1)
        .filter(|next| *next <= maximum)
        .ok_or(PlanError::AudioQueryLimit("sound gate work"))?;
    Ok(())
}
fn push_span(
    spans: &mut Vec<AudioSoundGateSpan>,
    span: AudioSoundGateSpan,
    limits: AudioQueryLimits,
    work: &mut usize,
) -> Result<(), PlanError> {
    charge(work, limits.maximum_work)?;
    if spans.len() == limits.maximum_spans {
        return Err(PlanError::AudioQueryLimit("sound gate spans"));
    }
    spans.push(span);
    Ok(())
}
