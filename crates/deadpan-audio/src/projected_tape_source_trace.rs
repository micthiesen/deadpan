//! Default-off J observations before the exclusive native handoff preflight.
use super::*;
use deadpan_plan::{AdmittedSignalPolicyQuery, SignalPolicyView};
use std::cell::{Cell, RefCell};

type Query<'work> = AdmittedSignalPolicyQuery<
    sound_context_admission::Issuer<'work>,
    sound_context_admission::Lease<'work>,
>;

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(in crate::stages) enum Phase {
    ContentPreflight { completed: usize },
    ControlChecked,
    BeforeHandoff,
}

#[allow(dead_code)]
pub(in crate::stages) enum Event<'value, 'work> {
    Query(&'value Query<'work>),
    Use {
        phase: Phase,
        view: SignalPolicyView<'value>,
    },
    PrepareMask {
        capacity: usize,
        lease_frames: Option<u64>,
    },
}

type Observer =
    Box<dyn for<'value, 'work> FnMut(Event<'value, 'work>, &'value StageAudio, WorkControl<'work>)>;

thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
    static HANDOFF: Cell<bool> = const { Cell::new(false) };
}

#[allow(dead_code)]
pub(in crate::stages) fn set_observer(observer: Option<Observer>) {
    OBSERVER.with(|current| *current.borrow_mut() = observer);
}

fn record<'value, 'work>(
    event: Event<'value, 'work>,
    renderer: &'value StageAudio,
    control: WorkControl<'work>,
) {
    OBSERVER.with(|observer| {
        if let Some(observer) = observer.borrow_mut().as_mut() {
            observer(event, renderer, control);
        }
    });
}

pub(in crate::stages) fn query<'value, 'work>(
    renderer: &'value StageAudio,
    control: WorkControl<'work>,
    policy: &'value Query<'work>,
) {
    record(Event::Query(policy), renderer, control);
}

pub(in crate::stages) fn observe<'value, 'work>(
    renderer: &'value StageAudio,
    control: WorkControl<'work>,
    phase: Phase,
    view: SignalPolicyView<'value>,
) {
    record(Event::Use { phase, view }, renderer, control);
}

pub(in crate::stages) fn before_handoff(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    view: SignalPolicyView<'_>,
) {
    observe(renderer, control, Phase::BeforeHandoff, view);
}

pub(in crate::stages) struct HandoffScope(bool);
impl Drop for HandoffScope {
    fn drop(&mut self) {
        HANDOFF.with(|current| current.set(self.0));
    }
}

pub(in crate::stages) fn handoff_scope() -> HandoffScope {
    HandoffScope(HANDOFF.with(|current| current.replace(true)))
}

pub(in crate::stages) fn prepare_mask(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    capacity: usize,
    lease_frames: Option<u64>,
) {
    if HANDOFF.with(Cell::get) {
        record(
            Event::PrepareMask {
                capacity,
                lease_frames,
            },
            renderer,
            control,
        );
    }
}
