//! One current Original query preparation, consumed by its exact read.
//! Flattened metadata retains its allocation-site charge through the read.
//! Other queries and the captured Domain identity keep their existing borrows.

use super::*;
use deadpan_plan::AdmittedAudioQuery;

/// The immutable flattened value owns its allocation-site charge. Processing,
/// policy and fades still use their existing, unadmitted constructors.
enum Flattened<'work> {
    Admitted {
        query: AdmittedAudioQuery<
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
        issuer: sound_context_admission::Issuer<'work>,
    },
    #[cfg(test)]
    Legacy(AudioQuery),
}

impl Flattened<'_> {
    fn query(&self) -> &AudioQuery {
        match self {
            Self::Admitted { query, .. } => query.query(),
            #[cfg(test)]
            Self::Legacy(query) => query,
        }
    }

    fn validate_issuer(&self, control: WorkControl<'_>) -> Result<(), StageAudioError> {
        match self {
            Self::Admitted { issuer, .. } => {
                // This new comparison has its own bill; legacy work is unchanged.
                control.spend_plan_work(1)?;
                #[cfg(test)]
                flattened_work_trace::issuer_spent(control);
                if *issuer != sound_context_admission::issuer(control) {
                    return Err(PlanError::AudioMetadataIssuerMismatch.into());
                }
                Ok(())
            }
            #[cfg(test)]
            Self::Legacy(_) => Ok(()),
        }
    }
}

struct PreparedQueries<'plan, 'work> {
    flattened: Flattened<'work>,
    processing: AudioProcessingQuery<'plan>,
    policy: AudioPolicyQuery<AudioSample>,
    fades: Option<AudioFadeQuery>,
}

#[derive(Clone, Copy)]
enum Origin<'context, 'plan> {
    Root(&'plan RenderPlan),
    Domain(&'context AudioDomain<'plan>),
}

pub(super) struct PreparedOriginalRequest<'context, 'plan, 'work> {
    origin: Origin<'context, 'plan>,
    start: AudioSample,
    frames: u32,
    edge_fades: bool,
    queries: PreparedQueries<'plan, 'work>,
}

impl<'plan> PreparedOriginalRequest<'_, 'plan, '_> {
    pub(super) fn queries(&self) -> RootReadQueriesRef<'_, 'plan> {
        RootReadQueriesRef {
            flattened: self.queries.flattened.query(),
            processing: &self.queries.processing,
            policy: &self.queries.policy,
            fades: self.queries.fades.as_ref(),
        }
    }

    pub(super) fn validate_issuer(&self, control: WorkControl<'_>) -> Result<(), StageAudioError> {
        self.queries.flattened.validate_issuer(control)
    }

    pub(super) fn matches_root(
        &self,
        plan: &RenderPlan,
        start: AudioSample,
        frames: u32,
        edge_fades: bool,
    ) -> bool {
        matches!(self.origin, Origin::Root(saved) if std::ptr::eq(saved, plan))
            && self.start == start
            && self.frames == frames
            && self.edge_fades == edge_fades
    }

    pub(super) fn matches_domain(
        &self,
        plan: &RenderPlan,
        domain: &AudioDomain<'_>,
        start: AudioSample,
        frames: u32,
        edge_fades: bool,
    ) -> bool {
        matches!(self.origin, Origin::Domain(saved)
            if saved.belongs_to(plan) && std::ptr::eq(saved, domain))
            && self.start == start
            && self.frames == frames
            && self.edge_fades == edge_fades
    }
}

impl StageAudio {
    pub(super) fn prepare_original_root<'plan, 'work>(
        &mut self,
        plan: &'plan RenderPlan,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'work>,
        edge_fades: bool,
    ) -> Result<PreparedOriginalRequest<'plan, 'plan, 'work>, StageAudioError> {
        control.check()?;
        if !std::ptr::eq(plan, self.plan.as_ref()) {
            return Err(StageAudioError::ForeignDomain);
        }
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(StageAudioError::Range)?;
        if start.0 < 0
            || frames == 0
            || frames > MAX_OUTPUT_FRAMES
            || end > plan.audio_duration()?.0
        {
            return Err(StageAudioError::Range);
        }
        let flattened =
            self.prepare_flattened(Origin::Root(plan), start..AudioSample(end), control)?;
        let processing = plan.audio_processing(start..AudioSample(end), control.query_limits()?)?;
        control.spend_plan_work(processing.work)?;
        let policy = plan.audio_policy(start..AudioSample(end), control.query_limits()?)?;
        control.spend_plan_work(policy.work)?;
        let fades = if edge_fades {
            let query = plan.audio_fades(start..AudioSample(end), control.query_limits()?)?;
            control.spend_plan_work(query.work)?;
            Some(query)
        } else {
            None
        };
        Ok(PreparedOriginalRequest {
            origin: Origin::Root(plan),
            start,
            frames,
            edge_fades,
            queries: PreparedQueries {
                flattened,
                processing,
                policy,
                fades,
            },
        })
    }

    pub(super) fn prepare_original_domain<'context, 'plan, 'work>(
        &mut self,
        domain: &'context AudioDomain<'plan>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'work>,
        edge_fades: bool,
    ) -> Result<PreparedOriginalRequest<'context, 'plan, 'work>, StageAudioError> {
        control.check()?;
        if !domain.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let end = start
            .0
            .checked_add(i64::from(frames))
            .map(AudioSample)
            .ok_or(StageAudioError::Range)?;
        if frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        let flattened = self.prepare_flattened(Origin::Domain(domain), start..end, control)?;
        let (processing, policy) = prepare_domain_policy(domain, start..end, control)?;
        let fades = if edge_fades {
            let query = domain.fades(start..end, control.query_limits()?)?;
            control.spend_plan_work(query.work)?;
            Some(query)
        } else {
            None
        };
        Ok(PreparedOriginalRequest {
            origin: Origin::Domain(domain),
            start,
            frames,
            edge_fades,
            queries: PreparedQueries {
                flattened,
                processing,
                policy,
                fades,
            },
        })
    }

    fn prepare_flattened<'work>(
        &mut self,
        origin: Origin<'_, '_>,
        samples: Range<AudioSample>,
        control: WorkControl<'work>,
    ) -> Result<Flattened<'work>, StageAudioError> {
        let limits = control.query_limits()?;
        #[cfg(test)]
        if legacy_flattened_queries() {
            let query = match origin {
                Origin::Root(plan) => plan.audio(samples, limits)?,
                Origin::Domain(domain) => domain.audio(samples, limits)?,
            };
            control.spend_plan_work(query.work)?;
            return Ok(Flattened::Legacy(query));
        }
        let issuer = sound_context_admission::issuer(control);
        #[cfg(test)]
        let mut flat_work_trace = flattened_work_trace::ScopeGuard::new(control);
        let query = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            match origin {
                Origin::Root(plan) => plan.audio_admitted(samples, limits, &mut admission)?,
                Origin::Domain(domain) => domain.audio_admitted(samples, limits, &mut admission)?,
            }
        };
        #[cfg(test)]
        {
            flat_work_trace.completed(
                query.canonical_units(),
                query.additional_units(),
                query.work(),
                query.was_forwarded(),
            );
            drop(flat_work_trace);
        }
        // The constructor forwarded every actual bill. Its total work must not
        // be charged again; only the query's lease outlives the adapter borrow.
        Ok(Flattened::Admitted { query, issuer })
    }

    /// Consume the already billed query allocation. Existing read preflight,
    /// live source/fingerprint checks, depth and final policy remain mandatory.
    pub(super) fn read_prepared_original(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        request: PreparedOriginalRequest<'_, '_, '_>,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<ReadBlock, StageAudioError> {
        let matches = match request.origin {
            Origin::Root(_) => request.matches_root(
                &self.plan,
                request.start,
                request.frames,
                request.edge_fades,
            ),
            Origin::Domain(domain) => request.matches_domain(
                &self.plan,
                domain,
                request.start,
                request.frames,
                request.edge_fades,
            ),
        };
        if !matches {
            return Err(StageAudioError::ForeignDomain);
        }
        request.validate_issuer(control)?;
        let result = self.read_queries_borrowed(
            provider,
            request.start,
            request.frames,
            control,
            depth,
            request.queries(),
        );
        // The raw result owns no query metadata. Release that metadata and its
        // charge here, before a caller can keep the returned PCM alive.
        drop(request);
        result
    }
}

#[cfg(test)]
#[path = "original_request/flattened_work_trace.rs"]
pub(super) mod flattened_work_trace;

#[cfg(test)]
thread_local! {
    static LEGACY_FLATTENED_QUERIES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn legacy_flattened_queries() -> bool {
    LEGACY_FLATTENED_QUERIES.with(std::cell::Cell::get)
}

/// Preserve literal old flattened-query observations without changing the
/// production constructor or admitting an uncharged compatibility query.
#[cfg(test)]
pub(super) fn with_legacy_flattened_queries<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY_FLATTENED_QUERIES.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(LEGACY_FLATTENED_QUERIES.with(|mode| mode.replace(true)));
    action()
}

fn prepare_domain_policy<'plan>(
    domain: &AudioDomain<'plan>,
    samples: Range<AudioSample>,
    control: WorkControl<'_>,
) -> Result<(AudioProcessingQuery<'plan>, AudioPolicyQuery<AudioSample>), StageAudioError> {
    #[cfg(test)]
    if policy_trace::legacy() {
        let processing = domain.processing(samples.clone(), control.query_limits()?)?;
        control.spend_plan_work(processing.work)?;
        let policy = domain.policy(samples.clone(), control.query_limits()?)?;
        control.spend_plan_work(policy.work)?;
        policy_trace::record(domain, samples, &processing, &policy);
        return Ok((processing, policy));
    }
    let processing = domain.processing_for_policy(samples.clone(), control.query_limits()?)?;
    // This is the actual first walk's one charge. Finishing policy receives
    // the fresh remaining allowance and reports only the work it performs.
    control.spend_plan_work(processing.query().work)?;
    let completed = processing.finish_policy(domain, samples.clone(), control.query_limits()?)?;
    let (processing, policy) = completed.into_queries();
    control.spend_plan_work(policy.work)?;
    #[cfg(test)]
    policy_trace::record(domain, samples, &processing, &policy);
    Ok((processing, policy))
}

#[cfg(test)]
#[path = "original_request/domain_policy_trace.rs"]
mod policy_trace;
#[cfg(test)]
pub(super) use policy_trace::{
    DomainPolicyTrace, domain_policy_trace, reset_domain_policy_trace, with_domain_policy_trace,
};

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "original_request/tests.rs"]
mod tests;
