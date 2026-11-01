//! Read-signal policy ownership spans preflight, PCM use and mask handoff.
use super::*;

impl StageAudio {
    pub(super) fn read_signal_policy_input<'work>(
        &mut self,
        input: SignalInput<'_, '_>,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        control: WorkControl<'work>,
    ) -> Result<SignalPolicyInput<'work>, StageAudioError> {
        let query = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            match input {
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
