//! Discard-only policy ownership through inventory and recursive graph preflight.
use super::*;
use deadpan_plan::{AdmittedSignalPolicyQuery, SignalPolicyView};

pub(super) enum IntrinsicPolicy<'work> {
    Admitted(
        AdmittedSignalPolicyQuery<
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
    ),
}

impl IntrinsicPolicy<'_> {
    pub(super) fn view(&self) -> SignalPolicyView<'_> {
        match self {
            Self::Admitted(query) => query.view(),
        }
    }

    pub(super) fn was_forwarded(&self) -> bool {
        match self {
            Self::Admitted(query) => query.was_forwarded(),
        }
    }
}

impl StageAudio {
    pub(super) fn intrinsic_policy<'work>(
        &mut self,
        input: SignalInput<'_, '_>,
        samples: Range<SignalSample>,
        control: WorkControl<'work>,
    ) -> Result<IntrinsicPolicy<'work>, StageAudioError> {
        let limits = control.query_limits()?;
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
        Ok(IntrinsicPolicy::Admitted(query))
    }
}
