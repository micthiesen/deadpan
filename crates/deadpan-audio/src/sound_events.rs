//! Independent authored sounds enter the common bus after their own edges and
//! gain. The Original keeps its existing continuous processing and fade path.

use deadpan_core::AudioEdgePolicy;

use super::*;

pub(super) struct PreparedSoundQuery<'plan> {
    pub(super) id: &'plan deadpan_core::SoundId,
    pub(super) voice: deadpan_plan::AudioRootSound<'plan>,
    pub(super) input: PreparedSoundInput<'plan>,
    pub(super) fades: Vec<deadpan_plan::AudioSoundGateSpan>,
    pub(super) post_fades: Vec<deadpan_plan::AudioSoundGateSpan>,
}

pub(super) enum PreparedSoundInput<'plan> {
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
        authored_gain: bool,
    ) -> Result<ReadBlock, StageAudioError> {
        if authored_gain && (self.plan.has_audio_wet_sends() || self.plan.has_sound_treatments()) {
            return self.read_shared_equalizers(provider, start, frames, control);
        }
        if authored_gain
            && self.plan.has_audio_component_automation()
            && (!self.plan.sounds().is_empty() || !self.plan.beat_sounds().is_empty())
        {
            return self.read_shared_equalizers(provider, start, frames, control);
        }
        if authored_gain && self.plan.has_audio_equalizers() {
            if !self.plan.sounds().is_empty() || !self.plan.beat_sounds().is_empty() {
                return self.read_shared_equalizers(provider, start, frames, control);
            }
            return self.read_authored_equalizers(provider, start, frames, control, 0, &[]);
        }
        if self.plan.sounds().is_empty() && self.plan.beat_sounds().is_empty() {
            if !authored_gain || !self.plan.has_audio_treatments() {
                return self.read_controlled(provider, start, frames, control, true, 0);
            }
            // The f64 gain output coexists with the Original f32 buffer, which
            // is reused for final conversion under the common residency cap.
            let reservation = u64::from(frames) * 3;
            self.make_room(reservation, false, control)?;
            self.active_frames += reservation;
            let result = (|| {
                let mut original =
                    self.read_controlled(provider, start, frames, control, true, 0)?;
                let gained = authored_gain::original_samples(&self.plan, &original, control, true)?;
                authored_gain::write_finite_samples(&mut original.samples, gained)?;
                Ok(original)
            })();
            self.active_frames -= reservation;
            return result;
        }
        let plan = Arc::clone(&self.plan);
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(StageAudioError::Range)?;
        let voices = self.prepare_root_sounds(&plan, start, frames, control)?;
        let retained =
            self.prepare_sound_processing_plans(&plan, start..AudioSample(end), control)?;
        let mut beat_voices =
            self.prepare_beat_sounds(&plan, &retained, start..AudioSample(end), control, None)?;
        let mut mix = group_mix::GroupMix::plan(&plan, start, frames, authored_gain, control)?;
        for voice in &mut beat_voices {
            voice.route_mix(&mut mix, control)?;
        }
        if mix.needs_components() {
            // Planning has not opened media. Keep contributions through shared
            // stages when their downstream treatment clocks differ.
            return self.read_shared_equalizers(provider, start, frames, control);
        }
        // Reserve the retained Original, the f64 sum and the source reader's
        // two transient PCM buffers under the existing shared residency limit.
        let routed = voices
            .iter()
            .any(|voice| matches!(&voice.input, PreparedSoundInput::Routed(_)))
            || beat_voices.iter().any(|voice| voice.has_routed());
        let reservation = (u64::from(frames) * if routed { 6 } else { 5 })
            .checked_add(mix.resident_frames()?)
            .ok_or(StageAudioError::Limit("group mix PCM"))?;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            mix.allocate();
            // All sound dependencies and static work are admitted first. The
            // Original's own preflight then sees the same cumulative budget.
            let original = self.read_controlled(provider, start, frames, control, true, 0)?;
            self.render_root_sounds(provider, original, voices, beat_voices, control, mix)
        })();
        self.active_frames -= reservation;
        result
    }

    pub(super) fn prepare_root_sounds<'plan>(
        &mut self,
        plan: &'plan RenderPlan,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<Vec<PreparedSoundQuery<'plan>>, StageAudioError> {
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
                    control.preflight_content(content, plan)?;
                }
                self.preflight_signal_query(&signal, control, 0)?;
                PreparedSoundInput::Direct { signal, policy }
            };
            let fades = voice.gate_fades(start..AudioSample(end), control.query_limits()?)?;
            control.spend_plan_work(fades.work)?;
            validate_gate_envelopes(&fades.spans, start..AudioSample(end))?;
            let post_fades =
                shared_eq::prepare_post_gates(&voice.event().treatments, control, |limits| {
                    voice.post_treatment_gates(start..AudioSample(end), limits)
                })?;
            voices.push(PreparedSoundQuery {
                id,
                voice,
                input,
                fades: fades.spans,
                post_fades,
            });
        }
        Ok(voices)
    }

    pub(super) fn read_prepared_root_sound(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        input: PreparedSoundInput<'_>,
        start: AudioSample,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        match input {
            PreparedSoundInput::Direct { signal, policy } => {
                let block = self.read_signal_queries(signal, policy, provider, control, 0)?;
                Ok(ReadBlock {
                    start,
                    samples: block.samples,
                    dependencies: block.dependencies,
                    relative_depth: block.relative_depth,
                    suppressed: block
                        .suppressed
                        .into_iter()
                        .map(|range| AudioSample(range.start.0)..AudioSample(range.end.0))
                        .collect(),
                    exhausted: Vec::new(),
                })
            }
            PreparedSoundInput::Routed(prepared) => {
                self.read_routed_root_controlled(provider, prepared, control)
            }
        }
    }

    fn render_root_sounds(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        mut original: ReadBlock,
        voices: Vec<PreparedSoundQuery<'_>>,
        beat_voices: Vec<super::beat_sounds::PreparedBeatSound<'_, '_>>,
        control: WorkControl<'_>,
        mut mix: group_mix::GroupMix<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        mix.original(&original.samples, control)?;
        for PreparedSoundQuery {
            id: _,
            voice,
            input,
            fades,
            post_fades: _,
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
            let mut block =
                self.read_prepared_root_sound(provider, input, original.start, control)?;
            block.suppressed.extend(
                fades
                    .iter()
                    .filter(|span| span.length == 0)
                    .map(|span| span.samples.clone()),
            );
            block.suppressed = merged_suppression(block.suppressed);
            if block.samples.len() != original.samples.len() {
                return Err(PlanError::InvalidPlan("incomplete authored sound PCM").into());
            }
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
            for (offset, (sample, edge)) in block.samples.into_iter().zip(edges).enumerate() {
                mix.add_root(
                    offset,
                    sample.map(|value| value * edge),
                    event.treatments.trim().millidecibels(),
                    control,
                )?;
            }
            original.suppressed = intersect_suppression(&original.suppressed, &block.suppressed);
            original.dependencies.extend(block.dependencies);
        }
        self.render_beat_sounds(provider, &mut original, &mut mix, beat_voices, control)?;
        authored_gain::write_finite_samples(&mut original.samples, mix.finish(control)?)?;
        // Exhaustion belongs to each contribution. There is no common source
        // endpoint mask after adding independent sound voices.
        original.exhausted.clear();
        control.check()?;
        Ok(original)
    }
}

pub(super) fn validate_gate_envelopes(
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

pub(super) fn intersect_suppression(
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
