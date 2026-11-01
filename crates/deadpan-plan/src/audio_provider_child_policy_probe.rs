//! Explicit-grid test seam; no public Tape or Point factory is added.
use super::*;
use serde_json::{Value, json};
pub(in crate::plan) fn run<A: AudioSoundContextAdmission>(
    signal: &AudioSignal<'_>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    rule: AudioBoundaryRule,
    endpoints: bool,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A> {
    limits.validate()?;
    let mut state = State::new(limits.maximum_spans, budget);
    let ranges = signal_policy(
        signal,
        samples.start.0..samples.end.0,
        rule,
        &mut state,
        budget,
        0,
        endpoints,
    )?;
    finish_signal(state, ranges, limits, budget)
}
pub(in crate::plan) fn snapshot<I: Copy + Eq, L>(value: &AdmittedSignalPolicyQuery<I, L>) -> Value {
    let AdmittedSignalPolicyQuery {
        mask,
        inventory,
        lookup,
        work,
        canonical,
        additional,
        forwarded,
    } = value;
    let FinalSignalMask { ranges, memory: _ } = mask;
    let SignalPolicyInventory {
        contents,
        _contents_capacity: _,
    } = &**inventory;
    let LookupStats {
        visited_nodes,
        sequence_comparisons,
        iteration_run_comparisons,
    } = lookup;
    json!({"result":"Ok","query":{"suppressed":ranges,"suppressed_capacity":ranges.capacity(),
        "contents":contents,"contents_capacity":contents.capacity(),"work":work,
        "lookup":{"visited_nodes":visited_nodes,"sequence_comparisons":sequence_comparisons,
            "iteration_run_comparisons":iteration_run_comparisons}},
        "canonical":canonical,"additional":additional,"forwarded":forwarded})
}
