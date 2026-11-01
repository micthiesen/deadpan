//! Discard-only policy ownership through inventory and recursive graph preflight.
use super::*;
use deadpan_plan::{AdmittedSignalPolicyQuery, SignalPolicyView};

pub(super) enum IntrinsicPolicy<'work> {
    #[cfg(test)]
    Ordinary(AudioPolicyQuery<SignalSample>),
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
            #[cfg(test)]
            Self::Ordinary(query) => SignalPolicyView {
                suppressed: &query.suppressed,
                contents: &query.contents,
                lookup: query.lookup,
                work: query.work,
            },
            Self::Admitted(query) => query.view(),
        }
    }

    pub(super) fn was_forwarded(&self) -> bool {
        match self {
            #[cfg(test)]
            Self::Ordinary(_) => false,
            Self::Admitted(query) => query.was_forwarded(),
        }
    }

    #[cfg(test)]
    pub(super) fn observe(
        &self,
        renderer: &StageAudio,
        control: WorkControl<'_>,
        input: SignalInput<'_, '_>,
        samples: Range<SignalSample>,
        phase: &'static str,
        completed: usize,
    ) {
        match self {
            Self::Ordinary(query) => {
                before::policy(renderer, control, input, samples, query, phase, completed);
            }
            Self::Admitted(query) => {
                before::admitted_policy(renderer, control, input, samples, query, phase, completed);
            }
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
        #[cfg(not(test))]
        {
            self.intrinsic_policy_inner(input, samples, control)
        }
        #[cfg(test)]
        {
            native_trace::factory_begin(control);
            let result = self.intrinsic_policy_inner(input, samples, control);
            match &result {
                Ok(IntrinsicPolicy::Admitted(query)) => native_trace::factory_success(
                    control,
                    query.view(),
                    query.canonical_units(),
                    query.additional_units(),
                    query.work(),
                    query.was_forwarded(),
                ),
                Ok(policy @ IntrinsicPolicy::Ordinary(_)) => {
                    native_trace::factory_ordinary(control, policy.view());
                }
                Err(error) => native_trace::factory_error(control, error),
            }
            result
        }
    }

    fn intrinsic_policy_inner<'work>(
        &mut self,
        input: SignalInput<'_, '_>,
        samples: Range<SignalSample>,
        control: WorkControl<'work>,
    ) -> Result<IntrinsicPolicy<'work>, StageAudioError> {
        let limits = control.query_limits()?;
        #[cfg(test)]
        if intrinsic_preflight_policy_request::legacy() {
            return Ok(IntrinsicPolicy::Ordinary(input.policy(samples, limits)?));
        }
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

#[cfg(test)]
pub(super) fn scope<'work>(
    control: WorkControl<'work>,
    input: SignalInput<'_, '_>,
    role: native_trace::Role,
    range: Range<SignalSample>,
    depth: usize,
) -> Option<native_trace::Scope<'work>> {
    let provider = match input {
        SignalInput::Signal(_) => native_trace::ProviderTag::Signal,
        SignalInput::Tape(_) => native_trace::ProviderTag::Tape,
        SignalInput::PreserveTape(_) => native_trace::ProviderTag::PreserveTape,
        SignalInput::Mix(_) => return None,
    };
    Some(native_trace::scope(control, provider, role, range, depth))
}
