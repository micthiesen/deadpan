//! One current Original query preparation, consumed by its exact read.
//! This is neither media admission nor a cache: queries borrow existing plan
//! storage and the captured Domain identity remains borrowed, never cloned.

use super::*;

#[derive(Clone, Copy)]
enum Origin<'context, 'plan> {
    Root(&'plan RenderPlan),
    Domain(&'context AudioDomain<'plan>),
}

pub(super) struct PreparedOriginalRequest<'context, 'plan> {
    origin: Origin<'context, 'plan>,
    start: AudioSample,
    frames: u32,
    edge_fades: bool,
    queries: RootReadQueries<'plan>,
}

impl<'plan> PreparedOriginalRequest<'_, 'plan> {
    pub(super) fn queries(&self) -> RootReadQueriesRef<'_, 'plan> {
        self.queries.as_borrowed()
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
    pub(super) fn prepare_original_root<'plan>(
        &self,
        plan: &'plan RenderPlan,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
        edge_fades: bool,
    ) -> Result<PreparedOriginalRequest<'plan, 'plan>, StageAudioError> {
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
        let flattened = plan.audio(start..AudioSample(end), control.query_limits()?)?;
        control.spend_plan_work(flattened.work)?;
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
            queries: RootReadQueries {
                flattened,
                processing,
                policy,
                fades,
            },
        })
    }

    pub(super) fn prepare_original_domain<'context, 'plan>(
        &self,
        domain: &'context AudioDomain<'plan>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
        edge_fades: bool,
    ) -> Result<PreparedOriginalRequest<'context, 'plan>, StageAudioError> {
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
        let flattened = domain.audio(start..end, control.query_limits()?)?;
        control.spend_plan_work(flattened.work)?;
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
            queries: RootReadQueries {
                flattened,
                processing,
                policy,
                fades,
            },
        })
    }

    /// Consume the already billed query allocation. Existing read preflight,
    /// live source/fingerprint checks, depth and final policy remain mandatory.
    pub(super) fn read_prepared_original(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        request: PreparedOriginalRequest<'_, '_>,
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
        self.read_queries_borrowed(
            provider,
            request.start,
            request.frames,
            control,
            depth,
            request.queries(),
        )
    }
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
