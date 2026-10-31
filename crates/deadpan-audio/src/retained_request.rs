//! Request-local retained metadata; both carriers outlive numerical reads.
use super::*;
use deadpan_plan::{AdmittedAudioFadeQuery, AdmittedAudioOwnerQuery, AudioOwnerQuery};

pub(super) enum Fade<'work> {
    Admitted(
        AdmittedAudioFadeQuery<
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
    ),
    #[cfg(test)]
    Legacy(AudioFadeQuery),
}
impl Fade<'_> {
    pub(super) fn query(&self) -> &AudioFadeQuery {
        match self {
            Self::Admitted(value) => value.query(),
            #[cfg(test)]
            Self::Legacy(value) => value,
        }
    }
}

pub(super) enum Owners<'plan, 'work> {
    Admitted(
        AdmittedAudioOwnerQuery<
            'plan,
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
    ),
    #[cfg(test)]
    Legacy(AudioOwnerQuery<'plan>),
}
impl<'plan> Owners<'plan, '_> {
    pub(super) fn query(&self) -> &AudioOwnerQuery<'plan> {
        match self {
            Self::Admitted(value) => value.query(),
            #[cfg(test)]
            Self::Legacy(value) => value,
        }
    }
}

impl StageAudio {
    pub(super) fn prepare_retained_fades<'work>(
        &mut self,
        input: &deadpan_plan::AudioOwnerRetainedInput<'_>,
        samples: Range<AudioSample>,
        control: WorkControl<'work>,
    ) -> Result<Fade<'work>, StageAudioError> {
        let limits = control.query_limits()?;
        #[cfg(test)]
        if legacy() {
            let query = input.fades(samples, limits)?;
            control.spend_plan_work(query.work)?;
            return Ok(Fade::Legacy(query));
        }
        #[cfg(test)]
        let mut receipt = trace::ScopeGuard::new_fade(control);
        let query = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            input.fades_admitted(samples, limits, &mut admission)?
        };
        #[cfg(test)]
        {
            receipt.completed(
                query.canonical_units(),
                query.additional_units(),
                query.work(),
                query.was_forwarded(),
            );
            drop(receipt);
        }
        Ok(Fade::Admitted(query))
    }

    pub(super) fn prepare_retained_owners<'plan, 'work>(
        &mut self,
        input: &deadpan_plan::AudioOwnerRetainedInput<'plan>,
        samples: Range<AudioSample>,
        control: WorkControl<'work>,
    ) -> Result<Owners<'plan, 'work>, StageAudioError> {
        let limits = control.query_limits()?;
        #[cfg(test)]
        if legacy() {
            let query = input.owners(samples, limits)?;
            control.spend_plan_work(query.work())?;
            return Ok(Owners::Legacy(query));
        }
        #[cfg(test)]
        let mut receipt = trace::ScopeGuard::new_owners(control);
        let query = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            input.owners_admitted(samples, limits, &mut admission)?
        };
        #[cfg(test)]
        {
            receipt.completed(
                query.canonical_units(),
                query.additional_units(),
                query.work(),
                query.was_forwarded(),
            );
            drop(receipt);
        }
        Ok(Owners::Admitted(query))
    }
}

#[cfg(test)]
#[path = "retained_request/trace.rs"]
pub(super) mod trace;

#[cfg(test)]
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn legacy() -> bool {
    LEGACY.with(std::cell::Cell::get)
}

/// Preserve both literal OLD retained constructors without changing Root fade mode.
#[cfg(test)]
pub(super) fn with_legacy_retained_queries<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|mode| mode.replace(true)));
    action()
}
