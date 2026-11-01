//! Discard-only output policies retain admission through validation and progress.
use super::*;

impl StageAudio {
    pub(super) fn stage_validation_policy<'work>(
        &mut self,
        output: SignalInput<'_, '_>,
        samples: Range<SignalSample>,
        control: WorkControl<'work>,
    ) -> Result<SignalPolicyInput<'work>, StageAudioError> {
        #[cfg(not(test))]
        {
            self.stage_validation_policy_inner(output, samples, control)
        }
        #[cfg(test)]
        {
            stage_validation_policy_native_trace::factory_begin(control);
            let result = self.stage_validation_policy_inner(output, samples, control);
            match &result {
                Ok(SignalPolicyInput::Admitted(query)) => {
                    stage_validation_policy_native_trace::factory_success(
                        control,
                        query.view(),
                        query.canonical_units(),
                        query.additional_units(),
                        query.work(),
                        query.was_forwarded(),
                    );
                }
                Ok(policy @ SignalPolicyInput::Ordinary(_)) => {
                    stage_validation_policy_native_trace::factory_ordinary(control, policy.view());
                }
                Err(error) => stage_validation_policy_native_trace::factory_error(control, error),
            }
            result
        }
    }

    fn stage_validation_policy_inner<'work>(
        &mut self,
        output: SignalInput<'_, '_>,
        samples: Range<SignalSample>,
        control: WorkControl<'work>,
    ) -> Result<SignalPolicyInput<'work>, StageAudioError> {
        #[cfg(not(test))]
        let limits = control.query_limits()?;
        #[cfg(test)]
        let limits = stage_validation_policy_before_trace::limits_result(
            self,
            control,
            control.query_limits(),
        )?;
        #[cfg(test)]
        if stage_validation_policy_request::legacy() {
            return Ok(output.policy(samples, limits)?.into());
        }
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

#[cfg(test)]
pub(super) fn scope<'work>(
    control: WorkControl<'work>,
    output: SignalInput<'_, '_>,
    range: Range<SignalSample>,
    depth: usize,
) -> Option<stage_validation_policy_native_trace::Scope<'work>> {
    use stage_validation_policy_native_trace::{ProviderTag, Role};
    let provider = match output {
        SignalInput::Signal(_) => ProviderTag::Signal,
        SignalInput::Tape(_) => ProviderTag::Tape,
        SignalInput::PreserveTape(_) => ProviderTag::PreserveTape,
        SignalInput::Mix(_) => return None,
    };
    Some(stage_validation_policy_native_trace::scope(
        control,
        provider,
        Role::Output,
        range,
        depth,
    ))
}
