//! Post-DSP output policy stays admitted until the block has been suppressed.
use super::*;

impl StageAudio {
    pub(super) fn suppress_stage_block(
        &mut self,
        output: SignalInput<'_, '_>,
        start: SignalSample,
        samples: &mut [[f32; 2]],
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let end = SignalSample(
            start
                .0
                .checked_add(i64::try_from(samples.len()).map_err(|_| TimeError::Overflow)?)
                .ok_or(TimeError::Overflow)?,
        );
        let limits = control.query_limits()?;
        let policy = self.read_signal_policy_input(output, start..end, limits, control)?;
        if !policy.was_forwarded() {
            control.spend_plan_work(policy.view().work)?;
        }
        for content in policy.view().contents {
            control.preflight_content(content, &self.plan)?;
        }
        control.check()?;
        apply_suppression(start, samples, policy.view().suppressed, |sample| sample.0)?;
        drop(policy);
        Ok(())
    }
}
