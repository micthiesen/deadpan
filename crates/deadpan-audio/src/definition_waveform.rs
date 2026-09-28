//! Whole-definition overview measurement on the existing canonical PCM path.

use super::*;
use crate::waveform::WaveformBuilder;
use crate::{
    DefinitionWaveform, WAVEFORM_STAGE, WaveformCompletion, WaveformControl, WaveformDescriptor,
    WaveformError, WaveformMeasurement, WaveformStopReason,
};

impl StageAudio {
    /// Measure intrinsic owner audio before effects. All blocks share one
    /// cumulative admission/deadline budget; source open and DSP cancellation
    /// remain cooperative. Progress publishes at most once per 250 ms and owns
    /// only immutable, fully measured bins. The caller publishes the terminal
    /// result separately. No device, source or dependency capability escapes.
    pub fn measure_definition(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        definition: &AudioDefinition<'_>,
        control: WaveformControl<'_>,
        mut publish: impl FnMut(Arc<DefinitionWaveform>),
    ) -> Result<WaveformMeasurement, WaveformError> {
        if !definition.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDefinition.into());
        }
        let rate = self.plan.metadata().presentation_basis.frame_rate;
        let total_samples = definition.signal().sample_count()?;
        let descriptor = WaveformDescriptor {
            stage: WAVEFORM_STAGE,
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            definition: definition.selector().clone(),
            root: definition.root().clone(),
            frame_rate: rate,
            owner_duration: definition.duration(),
            sample_rate: deadpan_core::MIX_SAMPLE_RATE,
            grid: deadpan_plan::AudioSampleGrid::new(
                ExactRatio::ZERO,
                ExactRatio::new(
                    i128::from(rate.numerator()),
                    i128::from(rate.denominator()) * i128::from(deadpan_core::MIX_SAMPLE_RATE),
                )?,
                deadpan_plan::AudioBoundaryRule::PointCeil,
            )?,
            total_samples,
            leaf_stride: 0,
        };
        let mut builder = WaveformBuilder::new(descriptor, control.limits, control.memory)?;
        let budget = match PreparationBudget::new(control.limits.timeout(), control.cancelled) {
            Ok(budget) => budget,
            Err(error) => {
                return Ok(builder.finish(WaveformCompletion::Partial(stop_reason(error))));
            }
        };
        let total = u64::try_from(total_samples.0).map_err(|_| WaveformError::InvalidGeometry)?;
        let end = total.min(control.limits.maximum_samples());
        let mut published_end = SignalSample(0);
        let mut last_publication: Option<Instant> = None;
        while builder.examined_samples() < end {
            if let Err(error) = budget.check() {
                return Ok(builder.finish(WaveformCompletion::Partial(stop_reason(error))));
            }
            let start = SignalSample(
                i64::try_from(builder.examined_samples()).map_err(|_| WaveformError::Overflow)?,
            );
            let count =
                u32::try_from((end - builder.examined_samples()).min(u64::from(MAX_OUTPUT_FRAMES)))
                    .map_err(|_| WaveformError::Overflow)?;
            let block = match self
                .read_definition_controlled(provider, definition, start, count, &budget)
            {
                Ok(block) => block,
                Err(error) => {
                    return Ok(builder.finish(WaveformCompletion::Partial(stop_reason(error))));
                }
            };
            if block.samples.len() != usize::try_from(count).map_err(|_| WaveformError::Overflow)? {
                return Ok(builder.finish(WaveformCompletion::Partial(
                    WaveformStopReason::Preparation(
                        "definition audio returned an incomplete block".into(),
                    ),
                )));
            }
            if let Err(error) = builder.push(start, &block.samples) {
                return Ok(builder.finish(WaveformCompletion::Partial(
                    WaveformStopReason::Preparation(error.to_string()),
                )));
            }
            if builder.examined_samples() < end
                && builder.measured_end() != published_end
                && last_publication.is_none_or(|last| last.elapsed() >= Duration::from_millis(250))
                && let Ok(snapshot) = builder.snapshot(control.memory)
            {
                published_end = builder.measured_end();
                last_publication = Some(Instant::now());
                publish(snapshot);
            }
        }
        let completion = if end == total {
            WaveformCompletion::Complete
        } else {
            WaveformCompletion::Partial(WaveformStopReason::OutputLimit)
        };
        Ok(builder.finish(completion))
    }
}

fn stop_reason(error: StageAudioError) -> WaveformStopReason {
    if error.is_cancelled() {
        WaveformStopReason::Cancelled
    } else if matches!(error, StageAudioError::Timeout) {
        WaveformStopReason::Deadline
    } else {
        WaveformStopReason::Preparation(error.to_string())
    }
}
