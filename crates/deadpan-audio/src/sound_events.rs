//! Independent authored sounds enter the common bus after their own edges and
//! gain. The Original keeps its existing continuous processing and fade path.

use deadpan_core::AudioEdgePolicy;

use super::*;

struct PreparedSoundQuery<'plan> {
    voice: deadpan_plan::AudioRootSound<'plan>,
    input: PreparedSoundInput<'plan>,
    fades: Vec<deadpan_plan::AudioSoundGateSpan>,
}

enum PreparedSoundInput<'plan> {
    Direct {
        signal: AudioSignalQuery<'plan>,
        policy: AudioPolicyQuery<SignalSample>,
    },
    Routed(super::routed::PreparedRoutedRootQuery<'plan>),
}

impl StageAudio {
    pub(super) fn read_authored_bus(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        if self.plan.sounds().is_empty() {
            return self.read_controlled(provider, start, frames, control, true, 0);
        }
        let plan = Arc::clone(&self.plan);
        let mut voices = Vec::with_capacity(plan.sounds().len());
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(StageAudioError::Range)?;
        for id in plan.sounds().keys() {
            control.check()?;
            let voice = plan.root_sound(id)?;
            control.admit_dependency(&voice.event().source.asset)?;
            let input = if let Some(routed) = voice.routed_input() {
                PreparedSoundInput::Routed(
                    self.preflight_routed_root(routed, start, frames, control)?,
                )
            } else {
                let tape = voice
                    .root_input_tape()
                    .ok_or(PlanError::InvalidPlan("sound input is absent"))?;
                let range = SignalSample(start.0)..SignalSample(end);
                let signal = tape.query(range.clone(), control.query_limits()?)?;
                control.spend_plan_work(signal.work)?;
                let policy = tape.policy(range, control.query_limits()?)?;
                control.spend_plan_work(policy.work)?;
                for content in &policy.contents {
                    control.preflight_content(content, &plan)?;
                }
                self.preflight_signal_query(&signal, control, 0)?;
                PreparedSoundInput::Direct { signal, policy }
            };
            let fades = voice.gate_fades(start..AudioSample(end), control.query_limits()?)?;
            control.spend_plan_work(fades.work)?;
            validate_gate_envelopes(&fades.spans, start..AudioSample(end))?;
            voices.push(PreparedSoundQuery {
                voice,
                input,
                fades: fades.spans,
            });
        }
        // Reserve the retained Original, the f64 sum and the source reader's
        // two transient PCM buffers under the existing shared residency limit.
        let routed = voices
            .iter()
            .any(|voice| matches!(&voice.input, PreparedSoundInput::Routed(_)));
        let reservation = u64::from(frames) * if routed { 6 } else { 5 };
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            // All sound dependencies and static work are admitted first. The
            // Original's own preflight then sees the same cumulative budget.
            let original = self.read_controlled(provider, start, frames, control, true, 0)?;
            self.render_root_sounds(provider, original, voices, control)
        })();
        self.active_frames -= reservation;
        result
    }

    fn render_root_sounds(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        mut original: ReadBlock,
        voices: Vec<PreparedSoundQuery<'_>>,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        let mut sum: Vec<_> = original
            .samples
            .iter()
            .map(|s| [f64::from(s[0]), f64::from(s[1])])
            .collect();
        for PreparedSoundQuery {
            voice,
            input,
            fades,
        } in voices
        {
            control.check()?;
            let event = voice.event();
            // Even an out-of-range or silent-Hold query retains the dependency.
            // A cached bus must never hide a changed or revoked sound source.
            let source =
                resolve_source(provider, &self.plan, &event.source.asset, control.cancelled)?;
            original.dependencies.insert(
                event.source.asset.clone(),
                control.observe(&event.source.asset, source)?,
            );
            let mut block = match input {
                PreparedSoundInput::Direct { signal, policy } => {
                    let block = self.read_signal_queries(signal, policy, provider, control, 0)?;
                    ReadBlock {
                        start: original.start,
                        samples: block.samples,
                        dependencies: block.dependencies,
                        relative_depth: block.relative_depth,
                        suppressed: block
                            .suppressed
                            .into_iter()
                            .map(|range| AudioSample(range.start.0)..AudioSample(range.end.0))
                            .collect(),
                        exhausted: Vec::new(),
                    }
                }
                PreparedSoundInput::Routed(prepared) => {
                    self.read_routed_root_controlled(provider, prepared, control)?
                }
            };
            block.suppressed.extend(
                fades
                    .iter()
                    .filter(|span| span.length == 0)
                    .map(|span| span.samples.clone()),
            );
            block.suppressed = merged_suppression(block.suppressed);
            if block.samples.len() != sum.len() {
                return Err(PlanError::InvalidPlan("incomplete authored sound PCM").into());
            }
            let gain = 10.0_f64.powf(f64::from(event.gain_millidecibels) / 20_000.0);
            let edges = fades.into_iter().flat_map(|span| {
                (0..span.samples.end.0 - span.samples.start.0).map(move |index| {
                    crate::edges::edge_gain(
                        span.length,
                        i128::from(span.progress_at_start) + i128::from(index),
                        span.start_edge == AudioEdgePolicy::Automatic,
                        span.end_edge == AudioEdgePolicy::Automatic,
                    )
                })
            });
            for ((total, sample), edge) in sum.iter_mut().zip(block.samples).zip(edges) {
                for channel in 0..2 {
                    if !sample[channel].is_finite() {
                        return Err(PreparationError::InvalidSamples.into());
                    }
                    // Preserve the documented edge-before-gain order and make
                    // only one f32 conversion after the ordered bus sum.
                    total[channel] += f64::from(sample[channel] * edge) * gain;
                }
            }
            original.suppressed = intersect_suppression(&original.suppressed, &block.suppressed);
            original.dependencies.extend(block.dependencies);
        }
        original.samples = sum
            .into_iter()
            .map(|frame| {
                let value = [frame[0] as f32, frame[1] as f32];
                if value.iter().any(|sample| !sample.is_finite()) {
                    Err(PreparationError::InvalidSamples)
                } else {
                    Ok(value)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Exhaustion belongs to each contribution. There is no common source
        // endpoint mask after adding independent sound voices.
        original.exhausted.clear();
        control.check()?;
        Ok(original)
    }
}

fn validate_gate_envelopes(
    spans: &[deadpan_plan::AudioSoundGateSpan],
    requested: Range<AudioSample>,
) -> Result<(), StageAudioError> {
    let mut cursor = requested.start;
    for span in spans {
        if span.samples.start != cursor
            || span.samples.end <= cursor
            || span.samples.end > requested.end
        {
            return Err(PlanError::InvalidPlan("incomplete sound gate envelopes").into());
        }
        let count =
            u64::try_from(span.samples.end.0 - cursor.0).map_err(|_| StageAudioError::Range)?;
        if span.length != 0
            && span
                .progress_at_start
                .checked_add(count)
                .is_none_or(|end| end > span.length)
        {
            return Err(PlanError::InvalidPlan(
                "sound gate envelope exceeds its retained interval",
            )
            .into());
        }
        cursor = span.samples.end;
    }
    if cursor != requested.end {
        return Err(PlanError::InvalidPlan("incomplete sound gate envelopes").into());
    }
    Ok(())
}

fn intersect_suppression(
    left: &[Range<AudioSample>],
    right: &[Range<AudioSample>],
) -> Vec<Range<AudioSample>> {
    let mut result = Vec::new();
    let (mut a, mut b) = (0, 0);
    while a < left.len() && b < right.len() {
        let range = left[a].start.max(right[b].start)..left[a].end.min(right[b].end);
        if range.start < range.end {
            result.push(range);
        }
        if left[a].end < right[b].end {
            a += 1;
        } else {
            b += 1;
        }
    }
    result
}
