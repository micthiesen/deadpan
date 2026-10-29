//! Keep a successful physical-parent query alive while its active starts are used.

use super::*;

pub(super) enum OwnerQuery<'context, 'plan, I, L> {
    SpanStart(deadpan_plan::AdmittedAudioOwnerSpanStartCandidates<'context, 'plan, I, L>),
    Retained(deadpan_plan::AdmittedAudioOwnerQuery<'plan, I, L>),
    #[cfg(test)]
    Reference(deadpan_plan::AdmittedAudioOwnerQuery<'plan, I, L>),
}

impl<'context, 'plan, I: Copy + Eq, L> OwnerQuery<'context, 'plan, I, L> {
    pub(super) fn query(&self) -> &AudioOwnerQuery<'plan> {
        match self {
            Self::SpanStart(value) => value.query(),
            Self::Retained(value) => value.query(),
            #[cfg(test)]
            Self::Reference(value) => value.query(),
        }
    }

    pub(super) fn source(
        &self,
        span: usize,
    ) -> node_description::Source<'_, 'context, 'plan, I, L> {
        match self {
            Self::SpanStart(candidates) => node_description::Source::SpanStart { candidates, span },
            Self::Retained(_) => node_description::Source::Direct,
            #[cfg(test)]
            Self::Reference(_) => node_description::Source::Direct,
        }
    }
}

impl<'plan> Context<'plan> {
    pub(super) fn span_start_owners_admitted<
        A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
    >(
        &self,
        plan: &'plan RenderPlan,
        range: Range<AudioSample>,
        control: WorkControl<'_>,
        admission: &mut A,
    ) -> Result<OwnerQuery<'_, 'plan, A::Issuer, A::Lease>, StageAudioError> {
        #[cfg(test)]
        if direct_reference() {
            return Ok(OwnerQuery::Reference(
                self.owners_admitted(plan, range, control, admission)?,
            ));
        }
        let limits = control.query_limits()?;
        let query = match self {
            Self::Root => OwnerQuery::SpanStart(
                plan.audio_owner_span_start_candidates_admitted(range, limits, admission)?,
            ),
            Self::Domain(domain) => OwnerQuery::SpanStart(
                domain.owner_span_start_candidates_admitted(range, limits, admission)?,
            ),
            Self::Retained(input) => {
                OwnerQuery::Retained(input.owners_admitted(range, limits, admission)?)
            }
        };
        let (forwarded, work) = match &query {
            OwnerQuery::SpanStart(value) => {
                let receipt = value.shared_query_work();
                (receipt.was_forwarded(), receipt.work())
            }
            OwnerQuery::Retained(value) => (value.was_forwarded(), value.work()),
            #[cfg(test)]
            OwnerQuery::Reference(_) => unreachable!("reference query returned above"),
        };
        if forwarded != A::METERED {
            return Err(PlanError::InvalidPlan("wide owner query accounting mode differs").into());
        }
        if !forwarded {
            control.spend_plan_work(work)?;
        }
        Self::validate_owner_query_layers(query.query())?;
        Ok(query)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Default)]
pub(super) struct CaptureTrace {
    pub(super) direct_point_work: usize,
    pub(super) direct_point_queries: usize,
    pub(super) certified_work: usize,
    pub(super) certified_captures: usize,
}

#[cfg(test)]
thread_local! {
    static DIRECT_REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CAPTURE_TRACE: std::cell::Cell<CaptureTrace> = const {
        std::cell::Cell::new(CaptureTrace {
            direct_point_work: 0, direct_point_queries: 0,
            certified_work: 0, certified_captures: 0,
        })
    };
}

#[cfg(test)]
fn direct_reference() -> bool {
    DIRECT_REFERENCE.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn with_direct_reference<R>(action: impl FnOnce() -> R) -> R {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            DIRECT_REFERENCE.with(|value| value.set(self.0));
        }
    }
    let _reset = Reset(DIRECT_REFERENCE.with(|value| value.replace(true)));
    action()
}

#[cfg(test)]
pub(super) fn reset_trace() {
    CAPTURE_TRACE.with(|value| value.set(CaptureTrace::default()));
}

#[cfg(test)]
pub(super) fn trace() -> CaptureTrace {
    CAPTURE_TRACE.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn record_direct_point_query(canonical: usize, additional: usize) {
    CAPTURE_TRACE.with(|value| {
        let mut trace = value.get();
        trace.direct_point_work = trace
            .direct_point_work
            .checked_add(canonical.checked_add(additional).unwrap())
            .unwrap();
        trace.direct_point_queries = trace.direct_point_queries.checked_add(1).unwrap();
        value.set(trace);
    });
}

#[cfg(test)]
pub(super) fn record_certified_capture(work: usize) {
    CAPTURE_TRACE.with(|value| {
        let mut trace = value.get();
        trace.certified_work = trace.certified_work.checked_add(work).unwrap();
        trace.certified_captures = trace.certified_captures.checked_add(1).unwrap();
        value.set(trace);
    });
}
