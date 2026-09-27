//! Additive raw preparation on a checked common point grid. Each voice owns
//! its policy; a muted or exhausted voice cannot erase another voice's PCM.

use deadpan_plan::AudioMixQuery;

use super::*;

impl StageAudio {
    /// Sum checked scoped voices before creative effects or mastering. Gates
    /// select explicit voice indices, while bus suppression is their policy
    /// intersection. This does not place an authored sound event.
    pub fn read_mix(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        mix: &AudioSignalMix<'_>,
        start: SignalSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<TapeAudioBlock, StageAudioError> {
        check_cancel(cancelled)?;
        if !mix.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        validate_timeout(timeout)?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(StageAudioError::Range)?;
        if start.0 < 0 || frames == 0 || frames > MAX_OUTPUT_FRAMES || end > mix.sample_count()?.0 {
            return Err(StageAudioError::Range);
        }
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        let block = self.read_mix_controlled(mix, provider, start, frames, control, 0)?;
        control.check()?;
        Ok(TapeAudioBlock {
            schema_version: 1,
            stage: "scoped_mix_preparation_pcm_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            support: mix.support(),
            start,
            samples: block.samples,
            suppressed: block.suppressed,
        })
    }

    pub(super) fn preflight_mix_query(
        &self,
        query: &AudioMixQuery<'_>,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<usize, StageAudioError> {
        let mut relative_depth = 0;
        for voice in &query.voices {
            control.check()?;
            for content in &voice.policy.contents {
                control.preflight_content(content, &self.plan)?;
            }
            relative_depth =
                relative_depth.max(self.preflight_signal_query(&voice.signal, control, depth)?);
        }
        Ok(relative_depth)
    }

    pub(super) fn read_mix_controlled(
        &mut self,
        mix: &AudioSignalMix<'_>,
        provider: &mut impl AudioSourceProvider,
        start: SignalSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<SignalBlock, StageAudioError> {
        control.check()?;
        if depth > self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        let end = SignalSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(StageAudioError::Range)?,
        );
        let query = mix.query(start..end, control.query_limits()?)?;
        control.spend_plan_work(query.work)?;
        // No voice may resolve media before later voices and their hidden
        // intrinsic histories have passed admission, including fully gated ones.
        self.preflight_mix_query(&query, control, depth)?;
        // Stereo f64 accumulation occupies two f32 frame equivalents. The voice
        // reader also holds its output allocation while copying a source/child
        // span into it, so reserve two more buffers across child preparation.
        let reservation = u64::from(frames) * 4;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = self.render_mix_query(query, provider, start, frames, control, depth);
        self.active_frames -= reservation;
        result
    }

    fn render_mix_query(
        &mut self,
        query: AudioMixQuery<'_>,
        provider: &mut impl AudioSourceProvider,
        start: SignalSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<SignalBlock, StageAudioError> {
        let mut sum = vec![[0.0_f64; 2]; frames as usize];
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        for voice in query.voices {
            control.check()?;
            let mut block =
                self.read_signal_queries(voice.signal, voice.policy, provider, control, depth)?;
            if block.samples.len() != sum.len() {
                return Err(PlanError::InvalidPlan("incomplete mix voice PCM").into());
            }
            apply_suppression(start, &mut block.samples, &voice.gates, |sample| sample.0)?;
            for (total, sample) in sum.iter_mut().zip(block.samples) {
                for channel in 0..2 {
                    if !sample[channel].is_finite() {
                        return Err(PreparationError::InvalidSamples.into());
                    }
                    total[channel] += f64::from(sample[channel]);
                    if !total[channel].is_finite() {
                        return Err(PreparationError::InvalidSamples.into());
                    }
                }
            }
            dependencies.extend(block.dependencies);
            relative_depth = relative_depth.max(block.relative_depth);
        }
        let samples = sum
            .into_iter()
            .map(|frame| {
                let frame = [frame[0] as f32, frame[1] as f32];
                if frame.iter().any(|value| !value.is_finite()) {
                    Err(PreparationError::InvalidSamples)
                } else {
                    Ok(frame)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        control.check()?;
        Ok(SignalBlock {
            samples,
            dependencies,
            suppressed: query.suppressed,
            relative_depth,
        })
    }
}
