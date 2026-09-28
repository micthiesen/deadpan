//! Owner gain is evaluated after the complete Original processor and its
//! existing edges. It never changes Preserve inputs or grants silence policy.

use deadpan_core::{AudioTreatments, EvaluatedGain, GAIN_NUMERIC_SCALE, GainError};
use deadpan_plan::AudioOwnerSupport;

use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct GainSum {
    millidecibels_q32: i128,
    muted: bool,
}

impl GainSum {
    fn add(&mut self, value: EvaluatedGain) -> Result<(), GainError> {
        // Core values are exact reduced Q32 millidecibels. Add on the common
        // wide grid; converting each factor to amplitude loses cancellation
        // and can overflow even when the final summed dB is finite.
        let scale = i128::from(GAIN_NUMERIC_SCALE);
        let denominator = value.millidecibels.denominator();
        if scale % denominator != 0 {
            return Err(GainError::Overflow);
        }
        let value_q32 = value
            .millidecibels
            .numerator()
            .checked_mul(scale / denominator)
            .ok_or(GainError::Overflow)?;
        self.millidecibels_q32 = self
            .millidecibels_q32
            .checked_add(value_q32)
            .ok_or(GainError::Overflow)?;
        self.muted |= value.muted;
        Ok(())
    }

    fn treatment(
        &mut self,
        treatment: &AudioTreatments,
        local: ExactRatio,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        if !treatment.is_empty() {
            control.spend_plan_work(treatment.record_count())?;
            self.add(treatment.evaluate(local)?)?;
        }
        Ok(())
    }

    pub(super) fn apply(self, sample: [f32; 2]) -> Result<[f64; 2], StageAudioError> {
        if sample.iter().any(|value| !value.is_finite()) {
            return Err(PreparationError::InvalidSamples.into());
        }
        if self.muted {
            return Ok([0.0; 2]);
        }
        if self.millidecibels_q32 == 0 {
            return Ok(sample.map(f64::from));
        }
        let gain =
            10.0_f64.powf(self.millidecibels_q32 as f64 / GAIN_NUMERIC_SCALE as f64 / 20_000.0);
        let result = sample.map(|value| f64::from(value) * gain);
        if !gain.is_finite() || result.iter().any(|value| !value.is_finite()) {
            return Err(PreparationError::InvalidSamples.into());
        }
        Ok(result)
    }
}

pub(super) fn original_samples(
    plan: &RenderPlan,
    original: &ReadBlock,
    control: WorkControl<'_>,
    authored: bool,
) -> Result<Vec<[f64; 2]>, StageAudioError> {
    if !authored || !plan.has_audio_treatments() {
        return Ok(original
            .samples
            .iter()
            .map(|frame| frame.map(f64::from))
            .collect());
    }
    let end = original
        .start
        .0
        .checked_add(i64::try_from(original.samples.len()).map_err(|_| StageAudioError::Range)?)
        .ok_or(StageAudioError::Range)?;
    let query =
        plan.audio_gain_owners(original.start..AudioSample(end), control.query_limits()?)?;
    control.spend_plan_work(query.work())?;
    let mut result = Vec::with_capacity(original.samples.len());
    for span in query.spans() {
        control.check()?;
        let treatments = span.owners().iter().filter_map(|owner| owner.treatments());
        deadpan_core::validate_audio_treatment_layers(treatments)?;
        for at in span.samples().start.0..span.samples().end.0 {
            let index =
                usize::try_from(at - original.start.0).map_err(|_| StageAudioError::Range)?;
            let sample = original.samples[index];
            if span.support() == AudioOwnerSupport::Inactive {
                if sample.iter().any(|value| *value != 0.0) {
                    return Err(StageAudioError::Unsupported(
                        "nonzero Original outside meaningful owner-clock support",
                    ));
                }
                result.push(sample.map(f64::from));
                continue;
            }
            let mut gain = GainSum::default();
            for owner in span.owners() {
                if let Some(treatment) = owner.treatments()
                    && !treatment.is_empty()
                {
                    gain.treatment(
                        treatment,
                        owner.sampling().local_at(AudioSample(at))?,
                        control,
                    )?;
                }
            }
            result.push(gain.apply(sample)?);
        }
    }
    if result.len() != original.samples.len() {
        return Err(PlanError::InvalidPlan("incomplete authored gain owners").into());
    }
    Ok(result)
}

pub(super) fn sound_gain(
    plan: &RenderPlan,
    at: AudioSample,
    event_millidecibels: i32,
    control: WorkControl<'_>,
    authored: bool,
) -> Result<GainSum, StageAudioError> {
    let mut gain = GainSum::default();
    gain.add(EvaluatedGain {
        millidecibels: ExactRatio::integer(i64::from(event_millidecibels)),
        muted: false,
    })?;
    if authored && !plan.root_audio_treatments().is_empty() {
        let rate = plan.metadata().presentation_basis.frame_rate;
        let local = ExactRatio::new(
            i128::from(at.0) * i128::from(rate.numerator()),
            i128::from(deadpan_core::MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        gain.treatment(plan.root_audio_treatments(), local, control)?;
    }
    Ok(gain)
}

pub(super) fn write_finite_samples(
    output: &mut [[f32; 2]],
    samples: Vec<[f64; 2]>,
) -> Result<(), StageAudioError> {
    if output.len() != samples.len() {
        return Err(PlanError::InvalidPlan("incomplete authored gain PCM").into());
    }
    // Reuse the private Original buffer. The resident peak is that f32 buffer
    // plus the f64 bus, with no fourth frame-sized conversion allocation.
    // A failure may alter this private buffer; callers publish only on success.
    for (target, frame) in output.iter_mut().zip(samples) {
        let value = frame.map(|sample| sample as f32);
        if value.iter().any(|sample| !sample.is_finite()) {
            return Err(PreparationError::InvalidSamples.into());
        }
        *target = value;
    }
    Ok(())
}
