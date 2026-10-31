//! One current Original query preparation, consumed by its exact read.
//! Flattened, processing, policy and conditional fades retain allocation-site charges through the read.
//! The captured Domain identity keeps its existing borrow.

use super::*;
use deadpan_plan::{
    AdmittedAudioFadeQuery, AdmittedAudioPolicyQuery, AdmittedAudioProcessingQuery,
    AdmittedAudioQuery,
};

/// The immutable flattened value owns its allocation-site charge.
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

enum Processing<'plan, 'work> {
    Admitted {
        query: AdmittedAudioProcessingQuery<
            'plan,
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
        issuer: sound_context_admission::Issuer<'work>,
    },
    #[cfg(test)]
    Legacy(AudioProcessingQuery<'plan>),
}

impl<'plan> Processing<'plan, '_> {
    fn query(&self) -> &AudioProcessingQuery<'plan> {
        match self {
            Self::Admitted { query, .. } => query.query(),
            #[cfg(test)]
            Self::Legacy(query) => query,
        }
    }

    fn validate_issuer(&self, control: WorkControl<'_>) -> Result<(), StageAudioError> {
        match self {
            Self::Admitted { issuer, .. } => {
                control.spend_plan_work(1)?;
                #[cfg(test)]
                flattened_work_trace::processing_issuer_spent(control);
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

enum Policy<'work> {
    Admitted {
        query: AdmittedAudioPolicyQuery<
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
        issuer: sound_context_admission::Issuer<'work>,
    },
    #[cfg(test)]
    Legacy(AudioPolicyQuery<AudioSample>),
}

impl Policy<'_> {
    fn query(&self) -> &AudioPolicyQuery<AudioSample> {
        match self {
            Self::Admitted { query, .. } => query.query(),
            #[cfg(test)]
            Self::Legacy(query) => query,
        }
    }

    fn validate_issuer(&self, control: WorkControl<'_>) -> Result<(), StageAudioError> {
        match self {
            Self::Admitted { issuer, .. } => {
                control.spend_plan_work(1)?;
                #[cfg(test)]
                flattened_work_trace::policy_issuer_spent(control);
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

enum Fade<'work> {
    Admitted {
        query: AdmittedAudioFadeQuery<
            sound_context_admission::Issuer<'work>,
            sound_context_admission::Lease<'work>,
        >,
        issuer: sound_context_admission::Issuer<'work>,
    },
    #[cfg(test)]
    Legacy(AudioFadeQuery),
}

impl Fade<'_> {
    fn query(&self) -> &AudioFadeQuery {
        match self {
            Self::Admitted { query, .. } => query.query(),
            #[cfg(test)]
            Self::Legacy(query) => query,
        }
    }

    fn validate_issuer(&self, control: WorkControl<'_>) -> Result<(), StageAudioError> {
        match self {
            Self::Admitted { issuer, .. } => {
                control.spend_plan_work(1)?;
                #[cfg(test)]
                flattened_work_trace::fade_issuer_spent(control);
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
    processing: Processing<'plan, 'work>,
    policy: Policy<'work>,
    fades: Option<Fade<'work>>,
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
            processing: self.queries.processing.query(),
            policy: self.queries.policy.query(),
            fades: self.queries.fades.as_ref().map(Fade::query),
        }
    }

    pub(super) fn validate_issuer(&self, control: WorkControl<'_>) -> Result<(), StageAudioError> {
        self.queries.flattened.validate_issuer(control)?;
        self.queries.processing.validate_issuer(control)?;
        self.queries.policy.validate_issuer(control)?;
        if let Some(fades) = &self.queries.fades {
            fades.validate_issuer(control)?;
        }
        Ok(())
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
        let processing = self.prepare_processing_root(plan, start..AudioSample(end), control)?;
        let policy = self.prepare_root_policy(plan, start..AudioSample(end), control)?;
        let fades = if edge_fades {
            Some(self.prepare_fades(Origin::Root(plan), start..AudioSample(end), control)?)
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
        let (processing, policy) = self.prepare_domain_policy(domain, start..end, control)?;
        let fades = if edge_fades {
            Some(self.prepare_fades(Origin::Domain(domain), start..end, control)?)
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

    fn prepare_processing_root<'plan, 'work>(
        &mut self,
        plan: &'plan RenderPlan,
        samples: Range<AudioSample>,
        control: WorkControl<'work>,
    ) -> Result<Processing<'plan, 'work>, StageAudioError> {
        let limits = control.query_limits()?;
        #[cfg(test)]
        if legacy_processing_queries() {
            let query = plan.audio_processing(samples, limits)?;
            control.spend_plan_work(query.work)?;
            return Ok(Processing::Legacy(query));
        }
        let issuer = sound_context_admission::issuer(control);
        #[cfg(test)]
        let mut processing_work_trace = flattened_work_trace::ScopeGuard::new_processing(control);
        let query = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            plan.audio_processing_admitted(samples, limits, &mut admission)?
        };
        #[cfg(test)]
        {
            processing_work_trace.completed(
                query.canonical_units(),
                query.additional_units(),
                query.work(),
                query.was_forwarded(),
            );
            drop(processing_work_trace);
        }
        Ok(Processing::Admitted { query, issuer })
    }

    fn prepare_root_policy<'work>(
        &mut self,
        plan: &RenderPlan,
        samples: Range<AudioSample>,
        control: WorkControl<'work>,
    ) -> Result<Policy<'work>, StageAudioError> {
        #[cfg(test)]
        if legacy_policy_queries() {
            let query = plan.audio_policy(samples, control.query_limits()?)?;
            control.spend_plan_work(query.work)?;
            return Ok(Policy::Legacy(query));
        }

        let limits = control.query_limits()?;
        let issuer = sound_context_admission::issuer(control);
        #[cfg(test)]
        let mut policy_work_trace = flattened_work_trace::ScopeGuard::new_policy(control);
        let query = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            plan.audio_policy_admitted(samples, limits, &mut admission)?
        };
        #[cfg(test)]
        {
            policy_work_trace.completed(
                query.canonical_units(),
                query.additional_units(),
                query.work(),
                query.was_forwarded(),
            );
            drop(policy_work_trace);
        }
        Ok(Policy::Admitted { query, issuer })
    }

    fn prepare_domain_policy<'plan, 'work>(
        &mut self,
        domain: &AudioDomain<'plan>,
        samples: Range<AudioSample>,
        control: WorkControl<'work>,
    ) -> Result<(Processing<'plan, 'work>, Policy<'work>), StageAudioError> {
        #[cfg(test)]
        if policy_trace::legacy() {
            let processing = domain.processing(samples.clone(), control.query_limits()?)?;
            control.spend_plan_work(processing.work)?;
            let policy = domain.policy(samples.clone(), control.query_limits()?)?;
            control.spend_plan_work(policy.work)?;
            policy_trace::record(domain, samples, &processing, &policy, (processing.work, 0));
            return Ok((Processing::Legacy(processing), Policy::Legacy(policy)));
        }
        #[cfg(test)]
        if legacy_processing_queries() {
            let processing =
                domain.processing_for_policy(samples.clone(), control.query_limits()?)?;
            control.spend_plan_work(processing.query().work)?;
            let completed =
                processing.finish_policy(domain, samples.clone(), control.query_limits()?)?;
            let (processing, policy) = completed.into_queries();
            control.spend_plan_work(policy.work)?;
            policy_trace::record(domain, samples, &processing, &policy, (processing.work, 0));
            return Ok((Processing::Legacy(processing), Policy::Legacy(policy)));
        }
        let limits = control.query_limits()?;
        let issuer = sound_context_admission::issuer(control);
        #[cfg(test)]
        let mut processing_work_trace = flattened_work_trace::ScopeGuard::new_processing(control);
        let processing = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            domain.processing_for_policy_admitted(samples.clone(), limits, &mut admission)?
        };
        #[cfg(test)]
        {
            let query = processing.query();
            processing_work_trace.completed(
                query.canonical_units(),
                query.additional_units(),
                query.work(),
                query.was_forwarded(),
            );
            drop(processing_work_trace);
        }
        #[cfg(test)]
        if legacy_policy_queries() {
            let completed =
                processing.finish_policy(domain, samples.clone(), control.query_limits()?)?;
            let (query, policy) = completed.into_queries();
            control.spend_plan_work(policy.work)?;
            let receipt = (query.canonical_units(), query.additional_units());
            let processing = Processing::Admitted { query, issuer };
            policy_trace::record(domain, samples, processing.query(), &policy, receipt);
            return Ok((processing, Policy::Legacy(policy)));
        }

        let limits = control.query_limits()?;
        #[cfg(test)]
        let mut policy_work_trace = flattened_work_trace::ScopeGuard::new_policy(control);
        let completed = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            processing.finish_policy_admitted(domain, samples.clone(), limits, &mut admission)?
        };
        let (query, policy) = completed.into_queries();
        #[cfg(test)]
        {
            policy_work_trace.completed(
                policy.canonical_units(),
                policy.additional_units(),
                policy.work(),
                policy.was_forwarded(),
            );
            drop(policy_work_trace);
        }
        #[cfg(test)]
        let receipt = (query.canonical_units(), query.additional_units());
        let processing = Processing::Admitted { query, issuer };
        #[cfg(test)]
        policy_trace::record_admitted(
            domain,
            samples,
            processing.query(),
            policy.query(),
            receipt,
            (policy.canonical_units(), policy.additional_units()),
        );
        Ok((
            processing,
            Policy::Admitted {
                query: policy,
                issuer,
            },
        ))
    }

    fn prepare_fades<'work>(
        &mut self,
        origin: Origin<'_, '_>,
        samples: Range<AudioSample>,
        control: WorkControl<'work>,
    ) -> Result<Fade<'work>, StageAudioError> {
        let limits = control.query_limits()?;
        #[cfg(test)]
        if legacy_fade_queries() {
            let query = match origin {
                Origin::Root(plan) => plan.audio_fades(samples, limits)?,
                Origin::Domain(domain) => domain.fades(samples, limits)?,
            };
            control.spend_plan_work(query.work)?;
            return Ok(Fade::Legacy(query));
        }
        let issuer = sound_context_admission::issuer(control);
        #[cfg(test)]
        let mut fade_work_trace = flattened_work_trace::ScopeGuard::new_fade(control);
        let query = {
            let mut admission = sound_context_admission::Admission::new(self, control);
            match origin {
                Origin::Root(plan) => plan.audio_fades_admitted(samples, limits, &mut admission)?,
                Origin::Domain(domain) => domain.fades_admitted(samples, limits, &mut admission)?,
            }
        };
        #[cfg(test)]
        {
            fade_work_trace.completed(
                query.canonical_units(),
                query.additional_units(),
                query.work(),
                query.was_forwarded(),
            );
            drop(fade_work_trace);
        }
        Ok(Fade::Admitted { query, issuer })
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
thread_local! {
    static LEGACY_PROCESSING_QUERIES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
thread_local! {
    static LEGACY_POLICY_QUERIES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
thread_local! {
    static LEGACY_FADE_QUERIES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn legacy_fade_queries() -> bool {
    LEGACY_FADE_QUERIES.with(std::cell::Cell::get)
}

/// Keep earlier product proofs on the literal ordinary conditional-fade path.
#[cfg(test)]
pub(super) fn with_legacy_fade_queries<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY_FADE_QUERIES.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(LEGACY_FADE_QUERIES.with(|mode| mode.replace(true)));
    action()
}

#[cfg(test)]
fn legacy_policy_queries() -> bool {
    LEGACY_POLICY_QUERIES.with(std::cell::Cell::get)
}

/// Keep the literal ordinary policy constructor available independently of
/// flattened and processing compatibility modes.
#[cfg(test)]
pub(super) fn with_legacy_policy_queries<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY_POLICY_QUERIES.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(LEGACY_POLICY_QUERIES.with(|mode| mode.replace(true)));
    action()
}

#[cfg(test)]
fn legacy_processing_queries() -> bool {
    LEGACY_PROCESSING_QUERIES.with(std::cell::Cell::get)
}

/// Explicit old processing mode, independent of flattened-query compatibility.
#[cfg(test)]
pub(super) fn with_legacy_processing_queries<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY_PROCESSING_QUERIES.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(LEGACY_PROCESSING_QUERIES.with(|mode| mode.replace(true)));
    action()
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

#[cfg(test)]
#[path = "original_request/domain_policy_trace.rs"]
mod policy_trace;

#[cfg(test)]
/// Select the literal OLD Original and nested retained constructors.
pub(super) fn with_legacy_original_queries<T>(action: impl FnOnce() -> T) -> T {
    super::raw_domain_request::with_legacy_raw_domain(|| {
        super::retained_request::with_legacy_retained_queries(|| {
            with_legacy_fade_queries(|| {
                with_legacy_policy_queries(|| {
                    with_legacy_processing_queries(|| with_legacy_flattened_queries(action))
                })
            })
        })
    })
}
#[cfg(test)]
pub(super) use policy_trace::{
    DomainPolicyTrace, domain_policy_trace, reset_domain_policy_trace, with_domain_policy_trace,
};

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "original_request/tests.rs"]
mod tests;
