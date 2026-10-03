//! Authored root sound recipes evaluated on the actual project sample grid.

use std::{collections::BTreeMap, ops::Range, sync::Arc};

use deadpan_core::{
    AudioEdgePolicy, AudioSample, ExactRatio, FrameDuration, MIX_SAMPLE_RATE, RootSoundRoute,
    SoundEvent, SoundHoldAllowances, SoundId, TimeError,
};

use crate::{
    AudioBoundaryRule, AudioContent, AudioHoldIssuer, AudioQueryLimits, AudioRootSource,
    AudioRoutedRoot, AudioSampleGrid, AudioSignalTape, AudioSignalTapeRun, AudioSoundRoute,
    AudioSourceVoiceRecipe, AudioSpan, PlanError, RenderPlan, SilenceReason,
};

/// A checked authored sound on the absolute RoundEven grid. The private tape
/// adapter evaluates source phase directly on that grid; output gates resolve
/// current Hold policy and this contribution's explicit allowances. It
/// never converts a rendered PointCeil sample array by relabeling its indices.
#[derive(Debug, Clone)]
pub struct AudioRootSound<'plan> {
    plan: &'plan RenderPlan,
    event: &'plan SoundEvent,
    input: Option<AudioSignalTape<'plan>>,
    allowances: Option<&'plan SoundHoldAllowances>,
    routed: Option<AudioRoutedRoot<'plan>>,
    projection: Option<&'plan CompiledRootSound>,
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
        let journal = self.sound_routes().get(id);
        let recipe_extent = journal.map_or(self.duration(), |route| route.recipe_extent);
        let owner_edges = self.root_audio_edges();
        let start_edge = if selection.start == ExactRatio::ZERO
            && owner_edges.node_start == AudioEdgePolicy::Hard
        {
            AudioEdgePolicy::Hard
        } else {
            event.start_edge
        };
        let end_edge = if selection.end == ExactRatio::integer(recipe_extent.frames())
            && owner_edges.node_end == AudioEdgePolicy::Hard
        {
            AudioEdgePolicy::Hard
        } else {
            event.end_edge
        };
        let recipe = AudioSourceVoiceRecipe {
            source: event.source.clone(),
            mapping: event.mapping,
            offset: event.offset,
        };
        let (input, routed) = if journal.is_some() {
            let projection = self
                .compiled_root_sound(id)
                .ok_or(PlanError::InvalidPlan("compiled root sound is absent"))?;
            let capture = AudioRootSource::new(
                self,
                recipe,
                ExactRatio::integer(recipe_extent.frames()),
                projection.route.recipe_grid(),
            )?;
            (
                None,
                Some(AudioRoutedRoot::source_shared(
                    capture,
                    Arc::clone(&projection.route),
                )?),
            )
        } else {
            let full = ExactRatio::ZERO..ExactRatio::integer(self.duration().frames());
            let voice = self.audio_signal().source_voice(recipe)?;
            let tape = AudioSignalTape::new(
                self,
                full.clone(),
                vec![AudioSignalTapeRun::new(
                    full.clone(),
                    full.clone(),
                    voice.input_signal(),
                )],
            )?;
            (
                Some(tape.remap_policy_window(
                    full.clone(),
                    full,
                    ExactRatio::ZERO,
                    step,
                    AudioBoundaryRule::RoundEven,
                )?),
                None,
            )
        };
        Ok(AudioRootSound {
            plan: self,
            event,
            input,
            allowances: self.sound_allowances().get(id),
            routed,
            projection: self.compiled_root_sound(id),
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

    /// Complete selected allocation on the retained recipe clock. Routed output
    /// can contain gaps; query its route and gate envelopes for current support.
    pub fn audible_samples(&self) -> Range<AudioSample> {
        self.audible.clone()
    }

    /// Complete preparation input before current Hold gates. Its labels are absolute root samples and
    /// its boundary rule is RoundEven despite the tape's generic signal label.
    /// Consumers must not use intrinsic PointCeil allocation for this view.
    pub fn root_input_tape(&self) -> Option<&AudioSignalTape<'plan>> {
        self.input.as_ref()
    }

    pub fn routed_input(&self) -> Option<&AudioRoutedRoot<'plan>> {
        self.routed.as_ref()
    }

    /// Whether the transported authored selection covers one current root
    /// sample, before Hold suppression or gain. Route gaps never gain support
    /// from an allowance. This bounded indexed lookup neither decodes media nor
    /// promises a nonzero waveform value.
    pub fn selects_sample(&self, sample: AudioSample) -> Result<bool, PlanError> {
        if sample.0 < 0 || sample >= self.plan.audio_duration()? {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        let Some(projection) = self.projection else {
            return Ok(self.audible.contains(&sample));
        };
        let index = projection
            .islands
            .partition_point(|island| island.support.end <= sample);
        Ok(projection
            .islands
            .get(index)
            .is_some_and(|island| island.support.contains(&sample)))
    }

    /// Whether any retained authored selection overlaps this half-open root
    /// range, before Hold suppression or gain. Empty ranges select nothing.
    /// The indexed lookup examines only the first possible island; route gaps
    /// never acquire support from an allowance.
    pub fn selects_range(&self, samples: Range<AudioSample>) -> Result<bool, PlanError> {
        if samples.start.0 < 0
            || samples.end < samples.start
            || samples.end > self.plan.audio_duration()?
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        if samples.is_empty() {
            return Ok(false);
        }
        let Some(projection) = self.projection else {
            return Ok(!self.audible.is_empty()
                && self.audible.start < samples.end
                && samples.start < self.audible.end);
        };
        let index = projection
            .islands
            .partition_point(|island| island.support.end <= samples.start);
        Ok(projection
            .islands
            .get(index)
            .is_some_and(|island| !island.support.is_empty() && island.support.start < samples.end))
    }

    fn allows_hold(
        &self,
        span: &AudioSpan,
        work: &mut usize,
        maximum: usize,
    ) -> Result<bool, PlanError> {
        let Some(allowances) = self.allowances else {
            return Ok(false);
        };
        // Account for path copying and the bounded binary membership search.
        // The core wrapper keeps the entries sorted and rejects duplicates.
        let comparisons = usize::BITS - allowances.len().leading_zeros();
        for _ in 0..span.instance.repeats.len() + 1 {
            charge(work, maximum)?;
        }
        for _ in 0..comparisons {
            charge(work, maximum)?;
        }
        let issuer = AudioHoldIssuer::from_span(
            span.definition.clone(),
            span.instance.clone(),
            span.gap_after.clone(),
        );
        Ok(issuer
            .sound_issuer()
            .is_some_and(|issuer| allowances.contains(&issuer)))
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
        if let Some(projection) = self.projection {
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
            return self.routed_gate_fades(samples, limits, projection);
        }
        CurrentEnvelope {
            plan: self.plan,
            audible: self.audible.clone(),
            selection: self.selection.clone(),
            grid: self.grid,
            start_edge: self.start_edge,
            end_edge: self.end_edge,
        }
        .query(samples, limits, |span, work, maximum| {
            self.allows_hold(span, work, maximum)
        })
    }
}

/// Shared current-clock envelope for an independently prepared occurrence.
/// It has no persisted Hold allowance. Original source cuts do not create edges.
pub(crate) fn occurrence_gate_fades(
    plan: &RenderPlan,
    selection: Range<ExactRatio>,
    start_edge: AudioEdgePolicy,
    end_edge: AudioEdgePolicy,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Result<AudioSoundGateQuery, PlanError> {
    let rate = plan.metadata().presentation_basis.frame_rate;
    let grid = AudioSampleGrid::<AudioSample>::new(
        ExactRatio::ZERO,
        ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?,
        AudioBoundaryRule::RoundEven,
    )?;
    CurrentEnvelope {
        plan,
        audible: grid.boundary(selection.start)?..grid.boundary(selection.end)?,
        selection,
        grid,
        start_edge,
        end_edge,
    }
    .query(samples, limits, |_, _, _| Ok(false))
}

struct CurrentEnvelope<'plan> {
    plan: &'plan RenderPlan,
    audible: Range<AudioSample>,
    selection: Range<ExactRatio>,
    grid: AudioSampleGrid<AudioSample>,
    start_edge: AudioEdgePolicy,
    end_edge: AudioEdgePolicy,
}

impl CurrentEnvelope<'_> {
    fn query(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        mut allows_hold: impl FnMut(&AudioSpan, &mut usize, usize) -> Result<bool, PlanError>,
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
        if self.audible.start >= self.audible.end
            || self.audible.start >= samples.end
            || self.audible.end <= samples.start
        {
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
            if allows_hold(&span, &mut work, limits.maximum_work)? {
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

/// A semantic boundary and its retained physical label are deliberately
/// separate. Moving a half-sample boundary can change RoundEven parity; an edit
/// transports the old sample label instead of rounding its moved frame again.
#[derive(Debug, Clone, Copy)]
struct RoutedBoundary {
    at: ExactRatio,
    label: i128,
    policy: AudioEdgePolicy,
}

#[derive(Debug, Clone)]
struct RoutedIsland {
    support: Range<AudioSample>,
    start: RoutedBoundary,
    end: RoutedBoundary,
}

impl AudioRootSound<'_> {
    fn routed_gate_fades(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        projection: &CompiledRootSound,
    ) -> Result<AudioSoundGateQuery, PlanError> {
        let mut work = 0;
        let root_end = self.plan.audio_duration()?;
        let context = AudioSample(samples.start.0.saturating_sub(GATE_CONTEXT_SAMPLES).max(0))
            ..AudioSample(
                samples
                    .end
                    .0
                    .saturating_add(GATE_CONTEXT_SAMPLES)
                    .min(root_end.0),
            );
        let query = self.plan.audio(
            context,
            AudioQueryLimits {
                maximum_spans: limits.maximum_spans.saturating_add(384).min(4096),
                maximum_work: limits
                    .maximum_work
                    .checked_sub(work)
                    .filter(|remaining| *remaining > 0)
                    .ok_or(PlanError::AudioQueryLimit("sound gate work"))?,
            },
        )?;
        work = work
            .checked_add(query.work)
            .filter(|work| *work <= limits.maximum_work)
            .ok_or(PlanError::AudioQueryLimit("sound gate work"))?;
        let mut gates: Vec<(RoutedBoundary, RoutedBoundary)> = Vec::new();
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
            if self.allows_hold(&span, &mut work, limits.maximum_work)? {
                continue;
            }
            let start = RoutedBoundary {
                at: span.project_extent.start,
                label: i128::from(self.grid.boundary(span.project_extent.start)?.0),
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
            let end = RoutedBoundary {
                at: span.project_extent.end,
                label: i128::from(self.grid.boundary(span.project_extent.end)?.0),
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
                && start.label <= previous.1.label
            {
                previous.1 = end;
            } else {
                gates.push((start, end));
            }
        }
        let mut spans = Vec::new();
        // The immutable islands are ordered and disjoint. Charge every binary
        // search comparison, then visit only islands intersecting this read.
        let mut low = 0;
        let mut high = projection.islands.len();
        while low < high {
            charge(&mut work, limits.maximum_work)?;
            let middle = low + (high - low) / 2;
            if projection.islands[middle].support.end <= samples.start {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        for island in &projection.islands[low..] {
            charge(&mut work, limits.maximum_work)?;
            if island.support.start >= samples.end {
                break;
            }
            let mut cursor = island.start;
            for &(start, end) in &gates {
                charge(&mut work, limits.maximum_work)?;
                if end.label < cursor.label && end.at != cursor.at {
                    continue;
                }
                if island.end.label < start.label && island.end.at != start.at {
                    break;
                }
                append_routed_envelope(
                    &mut spans,
                    &samples,
                    &island.support,
                    cursor,
                    routed_earlier(island.end, start)?,
                    limits,
                    &mut work,
                )?;
                cursor = routed_later(cursor, end)?;
                if cursor.label >= island.end.label {
                    break;
                }
            }
            append_routed_envelope(
                &mut spans,
                &samples,
                &island.support,
                cursor,
                island.end,
                limits,
                &mut work,
            )?;
        }
        let end = spans.last().map_or(samples.start, |span| span.samples.end);
        if end < samples.end {
            push_span(&mut spans, silent(end..samples.end), limits, &mut work)?;
        }
        // Apply the actual Hold mask independently of the virtual envelope
        // endpoints. Retained sample progress and exact semantic provenance
        // are different clocks; neither can permit PCM inside a current Hold.
        let spans = clip_current_hold_samples(spans, &gates, limits, &mut work)?;
        Ok(AudioSoundGateQuery { spans, work })
    }
}

fn clip_current_hold_samples(
    envelopes: Vec<AudioSoundGateSpan>,
    gates: &[(RoutedBoundary, RoutedBoundary)],
    limits: AudioQueryLimits,
    work: &mut usize,
) -> Result<Vec<AudioSoundGateSpan>, PlanError> {
    let mut clipped = Vec::new();
    let mut gate_index = 0;
    for span in envelopes {
        charge(work, limits.maximum_work)?;
        let mut cursor = span.samples.start;
        while gate_index < gates.len() && gates[gate_index].1.label <= i128::from(cursor.0) {
            charge(work, limits.maximum_work)?;
            gate_index += 1;
        }
        let mut index = gate_index;
        while index < gates.len() && gates[index].0.label < i128::from(span.samples.end.0) {
            charge(work, limits.maximum_work)?;
            let (start, end) = gates[index];
            let first = sample_label(start.label.max(i128::from(cursor.0)))?;
            let last = sample_label(end.label.min(i128::from(span.samples.end.0)))?;
            if cursor < first {
                push_envelope_slice(&mut clipped, &span, cursor..first, limits, work)?;
            }
            if first < last {
                push_span(&mut clipped, silent(first..last), limits, work)?;
            }
            cursor = cursor.max(last);
            if cursor == span.samples.end {
                break;
            }
            index += 1;
        }
        if cursor < span.samples.end {
            push_envelope_slice(&mut clipped, &span, cursor..span.samples.end, limits, work)?;
        }
    }
    Ok(clipped)
}
fn push_envelope_slice(
    spans: &mut Vec<AudioSoundGateSpan>,
    envelope: &AudioSoundGateSpan,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    work: &mut usize,
) -> Result<(), PlanError> {
    let progress_at_start = if envelope.length == 0 {
        0
    } else {
        envelope
            .progress_at_start
            .checked_add(
                u64::try_from(samples.start.0 - envelope.samples.start.0)
                    .map_err(|_| TimeError::Overflow)?,
            )
            .ok_or(TimeError::Overflow)?
    };
    push_span(
        spans,
        AudioSoundGateSpan {
            samples,
            progress_at_start,
            ..envelope.clone()
        },
        limits,
        work,
    )
}

fn sample_label(label: i128) -> Result<AudioSample, PlanError> {
    Ok(AudioSample(
        i64::try_from(label).map_err(|_| TimeError::Overflow)?,
    ))
}
fn move_boundary(
    boundary: RoutedBoundary,
    frame_shift: ExactRatio,
    shift: i128,
) -> Result<RoutedBoundary, PlanError> {
    Ok(RoutedBoundary {
        at: boundary.at.checked_add(frame_shift)?,
        label: boundary
            .label
            .checked_add(shift)
            .ok_or(TimeError::Overflow)?,
        policy: boundary.policy,
    })
}
fn routed_earlier(a: RoutedBoundary, b: RoutedBoundary) -> Result<RoutedBoundary, PlanError> {
    if a.at == b.at {
        return Ok(routed_coincident(a, b));
    }
    if a.label < b.label || (a.label == b.label && less(a.at, b.at)?) {
        Ok(a)
    } else {
        Ok(b)
    }
}
fn routed_later(a: RoutedBoundary, b: RoutedBoundary) -> Result<RoutedBoundary, PlanError> {
    if a.at == b.at {
        return Ok(routed_coincident(a, b));
    }
    if a.label > b.label || (a.label == b.label && less(b.at, a.at)?) {
        Ok(a)
    } else {
        Ok(b)
    }
}
fn routed_coincident(a: RoutedBoundary, b: RoutedBoundary) -> RoutedBoundary {
    // The new clipping boundary owns its physical label. Only exact semantic
    // coincidence combines Hard; equal rounded labels use exact ordering.
    RoutedBoundary {
        policy: if a.policy == AudioEdgePolicy::Hard || b.policy == AudioEdgePolicy::Hard {
            AudioEdgePolicy::Hard
        } else {
            AudioEdgePolicy::Automatic
        },
        ..b
    }
}
fn append_routed_envelope(
    spans: &mut Vec<AudioSoundGateSpan>,
    requested: &Range<AudioSample>,
    support: &Range<AudioSample>,
    start: RoutedBoundary,
    end: RoutedBoundary,
    limits: AudioQueryLimits,
    work: &mut usize,
) -> Result<(), PlanError> {
    if start.label >= end.label {
        return Ok(());
    }
    let cursor = spans
        .last()
        .map_or(requested.start, |span| span.samples.end);
    let first = start
        .label
        .max(i128::from(support.start.0))
        .max(i128::from(requested.start.0))
        .max(i128::from(cursor.0));
    let last = end
        .label
        .min(i128::from(support.end.0))
        .min(i128::from(requested.end.0));
    if first >= last {
        return Ok(());
    }
    let samples = sample_label(first)?..sample_label(last)?;
    if cursor < samples.start {
        push_span(spans, silent(cursor..samples.start), limits, work)?;
    }
    push_span(
        spans,
        AudioSoundGateSpan {
            length: u64::try_from(
                end.label
                    .checked_sub(start.label)
                    .ok_or(TimeError::Overflow)?,
            )
            .map_err(|_| TimeError::Overflow)?,
            progress_at_start: u64::try_from(
                i128::from(samples.start.0)
                    .checked_sub(start.label)
                    .ok_or(TimeError::Overflow)?,
            )
            .map_err(|_| TimeError::Overflow)?,
            samples,
            start_edge: start.policy,
            end_edge: end.policy,
        },
        limits,
        work,
    )
}

// Admission bounds for immutable envelope projection. These bound edit/island
// visits and retained entries, never output frames or request size. A normal
// audio query does not pay the chronological construction cost again.
const MAX_SOUND_COMPILE_WORK: usize = 16_777_216;
const MAX_SOUND_COMPILE_ISLANDS: usize = 16_384;

#[derive(Debug, Clone)]
pub(crate) struct CompiledRootSound {
    route: Arc<AudioSoundRoute<AudioSample>>,
    islands: Vec<RoutedIsland>,
}

impl RenderPlan {
    pub(crate) fn compile_root_sounds(
        &self,
    ) -> Result<BTreeMap<SoundId, CompiledRootSound>, PlanError> {
        let mut work = 0;
        let mut count = 0usize;
        let mut compiled = BTreeMap::new();
        for (id, journal) in self.sound_routes() {
            let event = self
                .sounds()
                .get(id)
                .ok_or(PlanError::InvalidPlan("routed sound is absent"))?;
            let grids = std::iter::once(journal.recipe_grid)
                .chain(journal.edits.iter().map(|edit| edit.grid))
                .map(|grid| {
                    AudioSampleGrid::<AudioSample>::new(
                        grid.frame_origin,
                        grid.frames_per_sample()?,
                        AudioBoundaryRule::RoundEven,
                    )
                })
                .collect::<Result<Vec<_>, PlanError>>()?;
            let grid = grids[0];
            let selection = event.mapping.selection_frames_with_offset(
                FrameDuration::ZERO,
                event.offset,
                journal.recipe_grid.frame_rate,
            )?;
            let audible = grid.boundary(selection.start)?..grid.boundary(selection.end)?;
            let edges = self.root_audio_edges();
            let start_edge = if selection.start == ExactRatio::ZERO
                && edges.node_start == AudioEdgePolicy::Hard
            {
                AudioEdgePolicy::Hard
            } else {
                event.start_edge
            };
            let end_edge = if selection.end == ExactRatio::integer(journal.recipe_extent.frames())
                && edges.node_end == AudioEdgePolicy::Hard
            {
                AudioEdgePolicy::Hard
            } else {
                event.end_edge
            };
            let islands = compile_islands(
                journal,
                grid,
                selection.start..selection.end,
                audible,
                start_edge,
                end_edge,
                &mut work,
            )?;
            count = count
                .checked_add(islands.len())
                .filter(|count| *count <= MAX_SOUND_COMPILE_ISLANDS)
                .ok_or(PlanError::AudioQueryLimit("compiled sound islands"))?;
            let route = AudioSoundRoute::<AudioSample>::new(journal.compile()?, grids)?;
            compiled.insert(
                id.clone(),
                CompiledRootSound {
                    route: Arc::new(route),
                    islands,
                },
            );
        }
        Ok(compiled)
    }
}

fn compile_islands(
    journal: &RootSoundRoute,
    grid: AudioSampleGrid<AudioSample>,
    selection: Range<ExactRatio>,
    audible: Range<AudioSample>,
    start_edge: AudioEdgePolicy,
    end_edge: AudioEdgePolicy,
    work: &mut usize,
) -> Result<Vec<RoutedIsland>, PlanError> {
    let mut islands = vec![RoutedIsland {
        support: audible.clone(),
        start: RoutedBoundary {
            at: selection.start,
            label: i128::from(audible.start.0),
            policy: start_edge,
        },
        end: RoutedBoundary {
            at: selection.end,
            label: i128::from(audible.end.0),
            policy: end_edge,
        },
    }];
    let mut extent = journal.recipe_extent.frames();
    let mut old_grid = grid;
    // The compact edit journal, rather than a final flattened map, retains
    // both earlier cuts and every intermediate physical sample allocation.
    for edit in &journal.edits {
        charge_compile(work)?;
        let next_grid = AudioSampleGrid::<AudioSample>::new(
            edit.grid.frame_origin,
            edit.grid.frames_per_sample()?,
            AudioBoundaryRule::RoundEven,
        )?;
        let projection = edit
            .operation
            .projection(extent)
            .map_err(|_| PlanError::InvalidPlan("invalid root sound edit projection"))?;
        let new_extent = projection.output_duration().frames();
        let mut next = Vec::new();
        for keep in projection.keeps() {
            let old = keep.input.start().0..keep.input.end().0;
            let destination = keep.output.start().0..keep.output.end().0;
            let start_cut = keep.start_cut.then_some(edit.cuts.after);
            let end_cut = keep.end_cut.then_some(edit.cuts.before);
            let old_start = old_grid.boundary(ExactRatio::integer(old.start))?;
            let old_end = old_grid.boundary(ExactRatio::integer(old.end))?;
            let destination_start = next_grid.boundary(ExactRatio::integer(destination.start))?;
            let destination_end = next_grid.boundary(ExactRatio::integer(destination.end))?;
            let shift = i128::from(destination_start.0) - i128::from(old_start.0);
            let frame_shift = ExactRatio::integer(destination.start)
                .checked_sub(ExactRatio::integer(old.start))?;
            for island in &islands {
                charge_compile(work)?;
                let first = island.support.start.max(old_start);
                let last = island.support.end.min(old_end);
                if first >= last {
                    continue;
                }
                let mut start = island.start;
                let mut end = island.end;
                if let Some(policy) = start_cut {
                    start = routed_later(
                        start,
                        RoutedBoundary {
                            at: ExactRatio::integer(old.start),
                            label: i128::from(old_start.0),
                            policy,
                        },
                    )?;
                }
                if let Some(policy) = end_cut {
                    end = routed_earlier(
                        end,
                        RoutedBoundary {
                            at: ExactRatio::integer(old.end),
                            label: i128::from(old_end.0),
                            policy,
                        },
                    )?;
                }
                // Intermediate translated labels can exceed i64 although
                // their destination-clipped allocation is representable.
                let first = (i128::from(first.0) + shift).max(i128::from(destination_start.0));
                let last = (i128::from(last.0) + shift).min(i128::from(destination_end.0));
                if first >= last {
                    continue;
                }
                let support = sample_label(first)?..sample_label(last)?;
                if next.len() >= MAX_SOUND_COMPILE_ISLANDS {
                    return Err(PlanError::AudioQueryLimit("compiled sound islands"));
                }
                next.push(RoutedIsland {
                    support,
                    start: move_boundary(start, frame_shift, shift)?,
                    end: move_boundary(end, frame_shift, shift)?,
                });
            }
        }
        islands = next;
        extent = new_extent;
        old_grid = next_grid;
    }
    Ok(islands)
}

fn charge_compile(work: &mut usize) -> Result<(), PlanError> {
    *work = work
        .checked_add(1)
        .filter(|count| *count <= MAX_SOUND_COMPILE_WORK)
        .ok_or(PlanError::AudioQueryLimit(
            "sound envelope construction work",
        ))?;
    Ok(())
}
