//! A borrowed policy view keeps the exact final Point lease alive through PCM use.
use super::*;
use deadpan_plan::{AdmittedSignalPolicyQuery, SignalPolicyView};

pub(super) fn legacy() -> bool {
    #[cfg(test)]
    return super::point_policy_request::legacy();
    #[cfg(not(test))]
    false
}

pub(super) enum SignalPolicyInput<'work> {
    Ordinary(AudioPolicyQuery<SignalSample>),
    Admitted(
        AdmittedSignalPolicyQuery<
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
    ),
}
impl From<AudioPolicyQuery<SignalSample>> for SignalPolicyInput<'_> {
    fn from(query: AudioPolicyQuery<SignalSample>) -> Self {
        Self::Ordinary(query)
    }
}
impl<'work> SignalPolicyInput<'work> {
    pub(super) fn view(&self) -> SignalPolicyView<'_> {
        match self {
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
            Self::Ordinary(_) => false,
            Self::Admitted(query) => query.was_forwarded(),
        }
    }
    pub(super) fn into_suppression(
        self,
        renderer: &mut StageAudio,
        control: WorkControl<'work>,
    ) -> Result<SignalSuppressionMask, StageAudioError> {
        match self {
            Self::Ordinary(query) => Ok(SignalSuppressionMask::plain(super::merged_suppression(
                query.suppressed,
            ))),
            Self::Admitted(query) => {
                let mut admission = sound_context_admission::Admission::new(renderer, control);
                query.into_suppression(&mut admission)
            }
        }
    }
}
