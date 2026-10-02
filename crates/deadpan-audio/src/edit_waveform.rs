//! Root-window measurement retains the complete authored preparation context.

use super::*;
use crate::waveform::EditWaveformBuilder;
use crate::{
    EditWaveform, EditWaveformDescriptor, EditWaveformMeasurement, EditWaveformStage,
    WaveformCompletion, WaveformControl, WaveformError, WaveformStopReason,
};

impl StageAudio {
    /// Measure the implemented authored bus before the common limiter. Reads
    /// retain the full plan and share one deadline/work budget. The selected
    /// window limits returned peaks, not source filters, envelopes or DSP input.
    pub fn measure_edit_window(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        samples: Range<AudioSample>,
        control: WaveformControl<'_>,
        mut publish: impl FnMut(Arc<EditWaveform>),
    ) -> Result<EditWaveformMeasurement, WaveformError> {
        let metadata = self.plan.metadata();
        let rate = metadata.presentation_basis.frame_rate;
        let descriptor = EditWaveformDescriptor {
            stage: EditWaveformStage::AuthoredBusBeforeLimiter,
            project_id: metadata.project_id.clone(),
            revision_id: metadata.revision_id.clone(),
            root: metadata.root.clone(),
            frame_rate: rate,
            project_duration: self.plan.duration(),
            sample_rate: deadpan_core::MIX_SAMPLE_RATE,
            grid: deadpan_plan::AudioSampleGrid::new(
                ExactRatio::ZERO,
                ExactRatio::new(
                    i128::from(rate.numerator()),
                    i128::from(rate.denominator()) * i128::from(deadpan_core::MIX_SAMPLE_RATE),
                )?,
                deadpan_plan::AudioBoundaryRule::RoundEven,
            )?,
            samples: samples.clone(),
            leaf_stride: 0,
        };
        let mut builder = EditWaveformBuilder::new(descriptor, control.limits, control.memory)?;
        let budget = match PreparationBudget::new(control.limits.timeout(), control.cancelled) {
            Ok(budget) => budget,
            Err(error) => {
                return Ok(builder.finish(WaveformCompletion::Partial(stop_reason(error))));
            }
        };
        let total = u64::try_from(samples.end.0 - samples.start.0)
            .map_err(|_| WaveformError::InvalidGeometry)?;
        let end = total.min(control.limits.maximum_samples());
        let mut published_end = samples.start;
        let mut last_publication: Option<Instant> = None;
        while builder.examined_samples() < end {
            if let Err(error) = budget.check() {
                return Ok(builder.finish(WaveformCompletion::Partial(stop_reason(error))));
            }
            let start = AudioSample(
                samples
                    .start
                    .0
                    .checked_add(
                        i64::try_from(builder.examined_samples())
                            .map_err(|_| WaveformError::Overflow)?,
                    )
                    .ok_or(WaveformError::Overflow)?,
            );
            let count =
                u32::try_from((end - builder.examined_samples()).min(u64::from(MAX_OUTPUT_FRAMES)))
                    .map_err(|_| WaveformError::Overflow)?;
            let block = match self.prepare_bus(provider, start, count, &budget) {
                Ok(prepared) => prepared.block,
                Err(error) => {
                    return Ok(builder.finish(WaveformCompletion::Partial(stop_reason(error))));
                }
            };
            if block.start != start
                || block.samples.len()
                    != usize::try_from(count).map_err(|_| WaveformError::Overflow)?
            {
                return Ok(builder.finish(WaveformCompletion::Partial(
                    WaveformStopReason::Preparation(
                        "authored bus returned an incomplete or shifted block".into(),
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
        Ok(builder.finish(if end == total {
            WaveformCompletion::Complete
        } else {
            WaveformCompletion::Partial(WaveformStopReason::OutputLimit)
        }))
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
