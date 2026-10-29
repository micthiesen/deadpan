//! Test-only receipts for the actual Domain preparation calls. Production never
//! scans a completed tape or retains a trace, and no receipt changes its bill.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::stages) struct DomainPolicyTrace {
    pub domain_instance: InstancePath,
    pub domain_root_samples: Range<AudioSample>,
    pub domain_visible_samples: Range<AudioSample>,
    pub samples: Range<AudioSample>,
    pub processing_work: usize,
    pub processing_lookup: deadpan_plan::LookupStats,
    pub policy_work: usize,
    pub policy_lookup: deadpan_plan::LookupStats,
    pub content_only_eligible: bool,
}

#[derive(Default)]
struct State {
    legacy: bool,
    rows: Option<Vec<DomainPolicyTrace>>,
}
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub(super) fn legacy() -> bool {
    STATE.with(|state| state.borrow().legacy)
}

pub(super) fn record(
    domain: &AudioDomain<'_>,
    samples: Range<AudioSample>,
    processing: &AudioProcessingQuery<'_>,
    policy: &AudioPolicyQuery<AudioSample>,
) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(rows) = &mut state.rows else {
            return;
        };
        rows.push(DomainPolicyTrace {
            domain_instance: domain.instance().clone(),
            domain_root_samples: domain.root_samples(),
            domain_visible_samples: domain.visible_samples(),
            samples,
            processing_work: processing.work,
            processing_lookup: processing.lookup,
            policy_work: policy.work,
            policy_lookup: policy.lookup,
            content_only_eligible: processing.spans.iter().all(|span| {
                matches!(
                    span.content,
                    AudioSignalContent::Leaf(_) | AudioSignalContent::Bound(_)
                )
            }),
        });
    });
}

pub(in crate::stages) fn domain_policy_trace() -> Vec<DomainPolicyTrace> {
    STATE.with(|state| state.borrow().rows.clone().unwrap_or_default())
}

pub(in crate::stages) fn reset_domain_policy_trace() {
    STATE.with(|state| state.borrow_mut().rows = Some(Vec::new()));
}

/// The override follows this thread and is restored even when an assertion
/// unwinds. The old arm executes the original public queries and their charges.
pub(in crate::stages) fn with_domain_policy_trace<R>(
    legacy: bool,
    operation: impl FnOnce() -> R,
) -> (R, Vec<DomainPolicyTrace>) {
    struct Reset(Option<State>);
    impl Drop for Reset {
        fn drop(&mut self) {
            let prior = self.0.take().unwrap();
            STATE.with(|state| *state.borrow_mut() = prior);
        }
    }
    let prior = STATE.with(|state| {
        state.replace(State {
            legacy,
            rows: Some(Vec::new()),
        })
    });
    let _reset = Reset(Some(prior));
    let result = operation();
    let rows = domain_policy_trace();
    (result, rows)
}
