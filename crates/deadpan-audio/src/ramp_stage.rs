//! A continuous ramp is a processing stage like Preserve: its complete child
//! input is read on the input point grid and processed once, as a whole
//! stage. Tape pitch resamples at each output sample's exact quadratic
//! position. Preserved pitch drives the canonical stretcher with the same
//! exact quadratic as its block schedule (`deadpan_dsp::RampSchedule`): every
//! 256-sample call consumes the input between two rounded exact positions,
//! independently of how this stage or its consumers size their reads. Its
//! cache key is the stage descriptor, whose pitch policy carries the ramp and
//! its pitch choice, so it never shares an entry with a constant-rate stage.
use super::*;
use crate::ramp_resample::RampSampler;
use deadpan_dsp::RampSchedule;

impl StageAudio {
    pub(super) fn prepare_ramp_stage(
        &mut self,
        stage: &AudioStage<'_>,
        ramp: deadpan_core::SpeedRamp,
        key: PreparedKey,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Arc<PreparedStage>, StageAudioError> {
        let input_signal = stage.input_signal();
        let output_signal = stage.output_signal();
        let input_count = u32::try_from(input_signal.sample_count()?.0)
            .map_err(|_| StageAudioError::Limit("input frames"))?;
        let output_count = u32::try_from(output_signal.sample_count()?.0)
            .map_err(|_| StageAudioError::Limit("output frames"))?;
        let descriptor = stage.descriptor();
        let length = descriptor
            .selection
            .end
            .checked_sub(descriptor.selection.start)?;
        if length.denominator() != 1 || descriptor.selection.start.denominator() != 1 {
            return Err(
                PlanError::InvalidPlan("a speed ramp needs an integral input range").into(),
            );
        }
        let sampler = RampSampler::new(
            ramp,
            i64::try_from(length.numerator()).map_err(|_| TimeError::Overflow)?,
            descriptor.duration.frames(),
            self.plan.metadata().presentation_basis.frame_rate,
            input_count,
            output_count,
        )?;
        let reservation = self.reserve_stream(input_count, output_count, control)?;
        if stage.descriptor().pitch.ramp_preserves_pitch() {
            let (linear, quadratic, denominator) = sampler.coefficients();
            let result = RampSchedule::new(linear, quadratic, denominator, output_count)
                .and_then(|schedule| StreamRecipe::ramp(input_count, schedule))
                .map_err(StageAudioError::from)
                .and_then(|recipe| {
                    self.build_preserve_ramp_stage(
                        recipe,
                        (&input_signal).into(),
                        (&output_signal).into(),
                        provider,
                        control,
                        depth,
                    )
                });
            self.release_stream(reservation);
            return self.publish(key, result?, control);
        }
        let result = self.build_ramp_stage(
            &sampler,
            (&input_signal).into(),
            (&output_signal).into(),
            output_count,
            provider,
            control,
            depth,
        );
        self.release_stream(reservation);
        self.publish(key, result?, control)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_ramp_stage(
        &mut self,
        sampler: &RampSampler,
        input: SignalInput<'_, '_>,
        output: SignalInput<'_, '_>,
        output_count: u32,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<PreparedBlock, StageAudioError> {
        let mut samples = PcmWriter::new(output_count, self.limits.maximum_memory_stage_frames)?;
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        // One sliding input window; the ramp's position only increases.
        let mut window_start = 0_i64;
        let mut window: Vec<[f32; 2]> = Vec::new();
        let progress = self.preparation_monitor.as_ref().map(|monitor| {
            monitor.enter(PreparationActivity::Stretching {
                depth,
                completed: 0,
                total: output_count,
            })
        });
        let mut at = 0_u32;
        while at < output_count {
            control.check()?;
            if let Some(progress) = &progress {
                progress.report(PreparationActivity::Stretching {
                    depth,
                    completed: at,
                    total: output_count,
                });
            }
            let frames = (output_count - at).min(MAX_OUTPUT_FRAMES);
            let start = i64::from(at);
            let need = sampler.required_input(start, start + i64::from(frames))?;
            let drop = (need.start - window_start).clamp(0, window.len() as i64) as usize;
            window.drain(..drop);
            window_start += drop as i64;
            if window.is_empty() {
                window_start = need.start;
            }
            let window_end = window_start + window.len() as i64;
            if need.end > window_end {
                let read = u32::try_from(window_end).map_err(|_| TimeError::Overflow)?
                    ..u32::try_from(need.end).map_err(|_| TimeError::Overflow)?;
                let block = self.read_stage_input(input, read, provider, control, depth)?;
                dependencies.extend(block.dependencies);
                relative_depth = relative_depth.max(block.relative_depth);
                window.extend(block.samples);
            }
            let mut block = sampler.render(start, frames as usize, window_start, &window)?;
            if block.iter().flatten().any(|sample| !sample.is_finite()) {
                return Err(crate::PreparationError::InvalidSamples.into());
            }
            self.suppress_stage_block(output, SignalSample(start), &mut block, control)?;
            samples.push(&block, control)?;
            at += frames;
        }
        Ok(PreparedBlock {
            samples: samples.finish(control)?,
            dependencies,
            relative_depth,
        })
    }

    /// The stretcher reports the exact input window of each read; the stage
    /// reads it once, in order, and checks the windows stay contiguous.
    fn build_preserve_ramp_stage(
        &mut self,
        recipe: StreamRecipe,
        input: SignalInput<'_, '_>,
        output: SignalInput<'_, '_>,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<PreparedBlock, StageAudioError> {
        let output_count = recipe.output_frames();
        let mut samples = PcmWriter::new(output_count, self.limits.maximum_memory_stage_frames)?;
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        let mut input_end = 0;
        let mut dsp = StreamingStretch::new(recipe)?;
        let progress = self.preparation_monitor.as_ref().map(|monitor| {
            monitor.enter(PreparationActivity::Stretching {
                depth,
                completed: 0,
                total: output_count,
            })
        });
        while dsp.position() < output_count {
            control.check()?;
            let at = dsp.position();
            if let Some(progress) = &progress {
                progress.report(PreparationActivity::Stretching {
                    depth,
                    completed: at,
                    total: output_count,
                });
            }
            let frames = (output_count - at).min(MAX_OUTPUT_FRAMES) as usize;
            let range = dsp.required_input(frames)?;
            if !range.is_empty() && range.start != input_end {
                return Err(deadpan_dsp::DspError::NativeReport.into());
            }
            let block = self.read_stage_input(input, range.clone(), provider, control, depth)?;
            if !range.is_empty() {
                input_end = range.end;
            }
            dependencies.extend(block.dependencies);
            relative_depth = relative_depth.max(block.relative_depth);
            let (input_left, input_right): (Vec<_>, Vec<_>) = block
                .samples
                .into_iter()
                .map(|frame| (frame[0], frame[1]))
                .unzip();
            let mut left = [0.0; MAX_OUTPUT_FRAMES as usize];
            let mut right = [0.0; MAX_OUTPUT_FRAMES as usize];
            let read = dsp.read(
                range.start,
                [&input_left, &input_right],
                [&mut left[..frames], &mut right[..frames]],
                control.cancelled,
            )?;
            if read != frames {
                return Err(deadpan_dsp::DspError::NativeReport.into());
            }
            let mut block: Vec<[f32; 2]> = left[..read]
                .iter()
                .zip(&right[..read])
                .map(|(&l, &r)| [l, r])
                .collect();
            self.suppress_stage_block(output, SignalSample(i64::from(at)), &mut block, control)?;
            samples.push(&block, control)?;
        }
        Ok(PreparedBlock {
            samples: samples.finish(control)?,
            dependencies,
            relative_depth,
        })
    }
}
