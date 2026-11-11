//! Discard-only output policies retain admission through validation and progress.
use super::*;

impl StageAudio {
    pub(super) fn stage_validation_policy<'work>(
        &mut self,
        output: SignalInput<'_, '_>,
        samples: Range<SignalSample>,
        control: WorkControl<'work>,
    ) -> Result<SignalPolicyInput<'work>, StageAudioError> {
        let limits = control.query_limits()?;
        let query = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            match output {
                SignalInput::Signal(signal) => {
                    signal.policy_admitted(samples, limits, &mut admission)?
                }
                SignalInput::Tape(tape) => tape.policy_admitted(samples, limits, &mut admission)?,
                SignalInput::PreserveTape(tape) => {
                    tape.policy_after_preserve_admitted(samples, limits, &mut admission)?
                }
                SignalInput::Mix(_) => {
                    return Err(PlanError::InvalidPlan("mix requires scoped voice policy").into());
                }
            }
        };
        Ok(SignalPolicyInput::Admitted(query))
    }
}
