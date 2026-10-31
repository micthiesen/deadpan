//! Test-only heap Layout lower bounds checked against the live admission ledger.
use super::*;
use deadpan_core::RepeatInstance;
use std::{alloc::Layout, cell::RefCell};

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
enum Phase {
    TemporaryDefinition,
    RootInitial,
    RootSelectorReplacement,
    RootRepeatReplacement,
    RootConstraintReplacement,
    RootInstanceReplacement,
    PointInitial,
    PointEvaluationReplacement,
}
pub(in crate::plan) const PHASES: usize = 8;
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::plan) struct Count {
    pub(in crate::plan) calls: usize,
    pub(in crate::plan) selector_pairs: usize,
    pub(in crate::plan) selector_ids: usize,
    pub(in crate::plan) nonempty_new_selectors: usize,
    pub(in crate::plan) empty_new_selectors: usize,
    pub(in crate::plan) logical_definition_ids: usize,
    pub(in crate::plan) max_heap_lower_bound: usize,
    pub(in crate::plan) min_live_bytes: usize,
    pub(in crate::plan) min_margin_bytes: usize,
    pub(in crate::plan) max_old_arrays: usize,
    pub(in crate::plan) max_new_arrays: usize,
}
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::plan) struct Summary {
    pub(in crate::plan) phases: [Count; PHASES],
}
thread_local! {
    static ACTIVE: RefCell<Option<Summary>> = const { RefCell::new(None) };
    static LIVE: RefCell<Option<Box<dyn Fn() -> usize>>> = const { RefCell::new(None) };
}
pub(in crate::plan) fn start() {
    ACTIVE.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Summary::default());
    });
}
pub(in crate::plan) fn finish() -> Summary {
    LIVE.with(|slot| assert!(slot.borrow().is_none()));
    ACTIVE.with(|slot| slot.borrow_mut().take().expect("active phase trace"))
}
struct LiveReset;
impl Drop for LiveReset {
    fn drop(&mut self) {
        LIVE.with(|slot| assert!(slot.borrow_mut().take().is_some()));
    }
}
pub(in crate::plan) fn with_live<R>(
    getter: impl Fn() -> usize + 'static,
    f: impl FnOnce() -> R,
) -> R {
    assert!(ACTIVE.with(|slot| slot.borrow().is_some()));
    LIVE.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Box::new(getter));
    });
    let _reset = LiveReset;
    f()
}
fn enabled() -> bool {
    ACTIVE.with(|slot| slot.borrow().is_some()) && LIVE.with(|slot| slot.borrow().is_some())
}
fn live_bytes() -> Option<usize> {
    LIVE.with(|slot| slot.borrow().as_ref().map(|get| get()))
}
fn array<T>(len: usize) -> usize {
    Layout::array::<T>(len).expect("actual Vec layout").size()
}
fn sum(parts: &[usize]) -> usize {
    parts
        .iter()
        .try_fold(0usize, |a, b| a.checked_add(*b))
        .expect("actual allocation lower-bound sum")
}
fn record(
    phase: Phase,
    old: usize,
    new: usize,
    selector_bytes: usize,
    selector_pairs: usize,
    selector_ids: usize,
    new_selectors: usize,
) {
    let Some(live) = live_bytes() else {
        return;
    };
    let lower = sum(&[old, new, selector_bytes]);
    assert!(
        live >= lower,
        "actual live admission bytes {live} must cover constructor-owned heap Layout lower bound {lower} at {phase:?}"
    );
    ACTIVE.with(|slot| {
        let mut state = slot.borrow_mut();
        let state = state.as_mut().expect("active phase summary");
        let count = &mut state.phases[phase as usize];
        count.calls += 1;
        count.selector_pairs += selector_pairs;
        count.selector_ids += selector_ids;
        count.nonempty_new_selectors += new_selectors;
        if matches!(
            phase,
            Phase::RootSelectorReplacement | Phase::PointEvaluationReplacement
        ) {
            count.empty_new_selectors += usize::from(new_selectors == 0);
        }
        count.logical_definition_ids += 1;
        count.max_heap_lower_bound = count.max_heap_lower_bound.max(lower);
        count.min_live_bytes = if count.calls == 1 {
            live
        } else {
            count.min_live_bytes.min(live)
        };
        count.min_margin_bytes = if count.calls == 1 {
            live - lower
        } else {
            count.min_margin_bytes.min(live - lower)
        };
        count.max_old_arrays = count.max_old_arrays.max(old);
        count.max_new_arrays = count.max_new_arrays.max(new);
    });
}
fn root_arrays(domain: &AudioDomain<'_>) -> usize {
    let seed = &domain.seed;
    sum(&[
        array::<crate::plan::audio::EnvelopeConstraint>(seed.constraints.capacity()),
        array::<crate::plan::audio::EditorialConstraint>(seed.editorial.capacity()),
        array::<RepeatInstance>(seed.repeats.capacity()),
        array::<crate::AudioRetimeStage>(seed.retimes.capacity()),
        array::<RepeatInstance>(domain.instance.repeats.capacity()),
    ])
}
fn selector_id_bytes(selector: &AudioDefinitionSelector) -> usize {
    let id = match selector {
        AudioDefinitionSelector::Node { node } => node,
        AudioDefinitionSelector::RepeatDefault { repeat }
        | AudioDefinitionSelector::RepeatGap { repeat } => repeat,
    };
    let bytes = id.as_str().len();
    assert!(
        bytes > 0,
        "validated NodeId has nonempty owned string storage"
    );
    bytes
}
pub(super) fn temporary_definition<A: AudioSoundContextAdmission>(
    definition: &AudioDefinition<'_>,
) {
    if !enabled() || !A::METERED {
        return;
    }
    let selector = definition.selector();
    record(
        Phase::TemporaryDefinition,
        0,
        0,
        selector_id_bytes(selector),
        0,
        1,
        0,
    );
}
pub(super) fn root_initial(domain: &AudioDomain<'_>, _definition: &AudioDefinition<'_>) {
    if !enabled() {
        return;
    }
    let mut selector_bytes = selector_id_bytes(_definition.selector());
    let mut selector_ids = 1;
    if let Some(selector) = domain.seed.definition.as_ref() {
        selector_bytes += selector_id_bytes(selector);
        selector_ids += 1;
    }
    let initial_pair = usize::from(domain.seed.definition.is_some());
    record(
        Phase::RootInitial,
        root_arrays(domain),
        0,
        selector_bytes,
        initial_pair,
        selector_ids,
        0,
    );
}
pub(super) fn root_selector(
    domain: &AudioDomain<'_>,
    next: Option<&AudioDefinitionSelector>,
    definition: &AudioDefinition<'_>,
) {
    if !enabled() {
        return;
    }
    assert!(
        domain.seed.definition.is_some(),
        "actual old Root selector is present"
    );
    let mut selector_bytes = selector_id_bytes(domain.seed.definition.as_ref().unwrap())
        + selector_id_bytes(definition.selector());
    let mut selector_ids = 2;
    let new_present = usize::from(next.is_some());
    if let Some(selector) = next {
        selector_bytes += selector_id_bytes(selector);
        selector_ids += 1;
    }
    record(
        Phase::RootSelectorReplacement,
        0,
        0,
        selector_bytes,
        new_present,
        selector_ids,
        new_present,
    );
}
pub(super) fn root_repeats(
    domain: &AudioDomain<'_>,
    next: &Vec<RepeatInstance>,
    _definition: &AudioDefinition<'_>,
) {
    if !enabled() {
        return;
    }
    record(
        Phase::RootRepeatReplacement,
        root_arrays(domain),
        array::<RepeatInstance>(next.capacity()),
        selector_id_bytes(_definition.selector()),
        0,
        1,
        0,
    );
}
pub(super) fn root_constraints(
    domain: &AudioDomain<'_>,
    next: &Vec<crate::plan::audio::EnvelopeConstraint>,
    _definition: &AudioDefinition<'_>,
) {
    if !enabled() {
        return;
    }
    record(
        Phase::RootConstraintReplacement,
        root_arrays(domain),
        array::<crate::plan::audio::EnvelopeConstraint>(next.capacity()),
        selector_id_bytes(_definition.selector()),
        0,
        1,
        0,
    );
}
pub(super) fn root_instance(
    domain: &AudioDomain<'_>,
    next: &deadpan_core::InstancePath,
    _definition: &AudioDefinition<'_>,
) {
    if !enabled() {
        return;
    }
    record(
        Phase::RootInstanceReplacement,
        root_arrays(domain),
        array::<RepeatInstance>(next.repeats.capacity()),
        selector_id_bytes(_definition.selector()),
        0,
        1,
        0,
    );
}
pub(super) fn point_initial(point: &AudioPointDomain<'_>, _definition: &AudioDefinition<'_>) {
    if !enabled() {
        return;
    }
    assert!(
        point.probe_structural_definition_present(),
        "actual initial Structural signal retains its selector"
    );
    let top_selector = point.probe_top_selector();
    assert_eq!(point.probe_structural_selector(), Some(top_selector));
    let old = array::<RepeatInstance>(point.probe_structural_repeat_capacity());
    record(
        Phase::PointInitial,
        old,
        0,
        selector_id_bytes(point.probe_structural_selector().unwrap())
            + selector_id_bytes(top_selector)
            + selector_id_bytes(_definition.selector()),
        1,
        3,
        0,
    );
}
pub(super) fn point_evaluation(
    point: &AudioPointDomain<'_>,
    _definition: &AudioDefinition<'_>,
    selector: &Option<AudioDefinitionSelector>,
    next: &Vec<RepeatInstance>,
) {
    if !enabled() {
        return;
    }
    assert!(
        point.probe_structural_definition_present(),
        "old Structural selector overlaps replacement evaluation"
    );
    let top_selector = point.probe_top_selector();
    assert_eq!(point.probe_structural_selector(), Some(top_selector));
    let old = array::<RepeatInstance>(point.probe_structural_repeat_capacity());
    let new = array::<RepeatInstance>(next.capacity());
    let new_present = usize::from(selector.is_some());
    assert!(
        selector.as_ref().is_none_or(|value| value == top_selector),
        "replacement selector, when present, is the actual Point top selector"
    );
    let mut selector_bytes = selector_id_bytes(point.probe_structural_selector().unwrap())
        + selector_id_bytes(top_selector)
        + selector_id_bytes(_definition.selector());
    let mut selector_ids = 3;
    if let Some(selector) = selector.as_ref() {
        selector_bytes += selector_id_bytes(selector);
        selector_ids += 1;
    }
    record(
        Phase::PointEvaluationReplacement,
        old,
        new,
        selector_bytes,
        new_present,
        selector_ids,
        new_present,
    );
}
