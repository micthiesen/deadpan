//! Default-off borrowed ordinary preflight observations, outside native accounting.
use super::*;
use serde_json::{Value, json};
use std::alloc::Layout;

const MAX_ROWS: usize = 8192;
const MAX_ACTIVE: usize = 128;
type Observer = Box<dyn for<'a, 'w> FnMut(Value, &'a StageAudio, WorkControl<'w>) -> Value>;
struct Capture {
    rows: Vec<Value>,
    stack: Vec<usize>,
    next: usize,
    next_chunk: usize,
    chunks: BTreeMap<usize, Value>,
    attempted: usize,
    dropped: usize,
    invalid: bool,
    observer: Observer,
}
thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}
pub(super) fn begin(observer: Observer) {
    CAPTURE.with(|slot| {
        assert!(slot.borrow().is_none(), "preflight observer already active");
        *slot.borrow_mut() = Some(Capture {
            rows: Vec::new(),
            stack: Vec::new(),
            next: 0,
            next_chunk: 0,
            chunks: BTreeMap::new(),
            attempted: 0,
            dropped: 0,
            invalid: false,
            observer,
        });
    });
}
/// Callback-entry position only; this does not observe decoder I/O.
pub(super) fn position() -> Option<Value> {
    CAPTURE.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|capture| {
            let chunk = capture
                .stack
                .iter()
                .rev()
                .find_map(|id| capture.chunks.get(id));
            json!({"last_attempted_ordinal":capture.attempted,
                "current_caller":capture.stack.last(),"chunk":chunk,"active_stack":capture.stack})
        })
    })
}
pub(super) fn take() -> Value {
    CAPTURE.with(|slot| {
        let Some(capture) = slot.borrow_mut().take() else {
            return Value::Null;
        };
        json!({"attempted":capture.attempted,"dropped":capture.dropped,
            "invalid":capture.invalid,"active_at_take":capture.stack,"rows":capture.rows})
    })
}
fn push(capture: &mut Capture, mut row: Value) {
    capture.attempted += 1;
    if capture.rows.len() == MAX_ROWS {
        capture.dropped += 1;
        return;
    }
    row["ordinal"] = json!(capture.attempted);
    capture.rows.push(row);
}
pub(super) struct Scope(Option<usize>);
impl Drop for Scope {
    fn drop(&mut self) {
        let Some(id) = self.0 else { return };
        CAPTURE.with(|slot| {
            if let Some(capture) = slot.borrow_mut().as_mut() {
                if capture.stack.pop() != Some(id) { capture.invalid = true; }
                let chunk=capture.chunks.remove(&id);
                push(capture,json!({"phase":"scope_exit","caller":id,"chunk":chunk,
                    "meaning":"lexical exit; actual success/error is reported by caller, never inferred"}));
            }
        });
    }
}
pub(super) fn enter(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    kind: &'static str,
    depth: usize,
) -> Scope {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(capture) = slot.as_mut() else {
            return Scope(None);
        };
        if capture.stack.len() == MAX_ACTIVE {
            capture.invalid = true;
            return Scope(None);
        }
        let Some(id) = capture.next.checked_add(1) else {
            capture.invalid = true;
            return Scope(None);
        };
        capture.next = id;
        let parent = capture.stack.last().copied();
        let chunk = capture
            .stack
            .iter()
            .rev()
            .find_map(|id| capture.chunks.get(id))
            .cloned();
        capture.stack.push(id);
        let row = (capture.observer)(
            json!({"phase":"scope_enter","kind":kind,
            "depth":depth,"caller":id,"parent":parent,"chunk":chunk}),
            renderer,
            control,
        );
        push(capture, row);
        Scope(Some(id))
    })
}
pub(super) fn mark(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    phase: &'static str,
    value: impl FnOnce() -> Value,
) {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(capture) = slot.as_mut() else { return };
        let value = value();
        if matches!(phase, "input_chunk_enter" | "output_chunk_enter") {
            if let Some(id) = capture.next_chunk.checked_add(1) {
                capture.next_chunk = id;
                if let Some(caller) = capture.stack.last().copied() {
                    capture
                        .chunks
                        .insert(caller, json!({"id":id,"role":phase,"range":value["range"]}));
                } else {
                    capture.invalid = true;
                }
            } else {
                capture.invalid = true;
            }
        }
        let chunk = capture
            .stack
            .iter()
            .rev()
            .find_map(|id| capture.chunks.get(id));
        let row = json!({"phase":phase,"caller":capture.stack.last(),"chunk":chunk,"value":value});
        let row = (capture.observer)(row, renderer, control);
        push(capture, row);
    });
}
pub(super) fn mode(value: SignalInput<'_, '_>) -> &'static str {
    match value {
        SignalInput::Signal(_) => "Signal",
        SignalInput::Tape(_) => "Tape",
        SignalInput::PreserveTape(_) => "PreserveTape",
        SignalInput::Mix(_) => "Mix",
    }
}
fn layout<T>(capacity: usize) -> Value {
    let value = Layout::array::<T>(capacity).unwrap();
    json!({"capacity":capacity,"bytes":value.size(),"align":value.align()})
}
pub(super) fn policy_value(value: &deadpan_plan::AudioPolicyQuery<SignalSample>) -> Value {
    let deadpan_plan::AudioPolicyQuery {
        suppressed,
        contents,
        lookup,
        work,
    } = value;
    json!({"suppressed":suppressed,"suppressed_len":suppressed.len(),
        "suppressed_storage":layout::<Range<SignalSample>>(suppressed.capacity()),
        "contents":contents,"contents_len":contents.len(),
        "contents_storage":layout::<deadpan_plan::AudioContent>(contents.capacity()),
        "lookup":lookup,"work":work,"ownership":"ordinary unadmitted policy vectors"})
}
pub(super) fn projection_value(value: &AudioStageProjection<'_>, plan: &RenderPlan) -> Value {
    let tape = |t: &deadpan_plan::AudioSignalTape<'_>| {
        json!({"support":t.support(),"grid":t.grid(),
        "sample_count":t.sample_count().map(|v|v.0).map_err(|e|e.to_string()),
        "belongs_to_plan":t.belongs_to(plan),
        "private_run_inventory":"not public; actual constructed query values captured separately"})
    };
    json!({"stage":value.stage().descriptor(),"belongs_to_plan":value.belongs_to(plan),
        "stage_repeats_storage":layout::<deadpan_core::RepeatInstance>(value.stage().descriptor().instance.repeats.capacity()),
        "input_support":value.input_support(),
        "input_sample_count":value.input_sample_count().map(|v|v.0).map_err(|e|e.to_string()),
        "output_frames":value.output_frames(),"output_duration":value.output_duration(),
        "input_tape":value.input_tape().map(tape),"input_is_mix":value.input_mix().is_some(),
        "output_policy":tape(value.output_policy()),"has_source_key":value.source_key().is_some(),
        "has_owned_key":value.owned_key().is_some(),
        "private_identity":"opaque; no address or Debug replacement"})
}
pub(super) fn graph_value(value: &AudioSignalQuery<'_>, plan: &RenderPlan) -> Value {
    let AudioSignalQuery {
        project_id,
        revision_id,
        definition,
        samples,
        spans,
        lookup,
        work,
    } = value;
    json!({"project_id":project_id,"revision_id":revision_id,"definition":definition,
        "samples":samples,"lookup":lookup,"work":work,"spans_len":spans.len(),
        "spans_storage":layout::<deadpan_plan::AudioSignalSpan<'_>>(spans.capacity()),
        "spans":spans.iter().map(|span| {
            let content=match &span.content {
                AudioSignalContent::Leaf(content)=>json!({"kind":"Leaf","value":content}),
                AudioSignalContent::Stage(stage)=>json!({"kind":"Stage","descriptor":stage.descriptor(),
                    "belongs_to_plan":stage.belongs_to(plan),
                    "repeats_storage":layout::<deadpan_core::RepeatInstance>(stage.descriptor().instance.repeats.capacity())}),
                AudioSignalContent::Bound(bound)=>json!({"kind":"Bound","public_value":bound,
                    "work":bound.work(),"belongs_to_plan":bound.belongs_to(plan),
                    "private_constraints":"unavailable across Plan crate privacy; not exhaustive private Bound state"}),
                AudioSignalContent::ProjectedStage(projection)=>json!({"kind":"ProjectedStage",
                    "value":projection_value(projection,plan)}),
            };
            json!({"public_fields":span,"content_expanded":content,
                "retimes_storage":layout::<deadpan_plan::AudioRetimeStage>(span.retimes.capacity()),
                "repeats_storage":layout::<deadpan_core::RepeatInstance>(span.instance.repeats.capacity())})
        }).collect::<Vec<_>>()})
}
pub(super) fn mix_value(value: &deadpan_plan::AudioMixQuery<'_>, plan: &RenderPlan) -> Value {
    let deadpan_plan::AudioMixQuery {
        project_id,
        revision_id,
        samples,
        voices,
        suppressed,
        lookup,
        work,
    } = value;
    json!({"project_id":project_id,"revision_id":revision_id,"samples":samples,
        "voices_len":voices.len(),"voices_storage":layout::<deadpan_plan::AudioMixVoiceQuery<'_>>(voices.capacity()),
        "voices":voices.iter().map(|v|json!({"index":v.index,"signal":graph_value(&v.signal,plan),
            "policy":policy_value(&v.policy),"gates":v.gates,
            "gates_storage":layout::<Range<SignalSample>>(v.gates.capacity())})).collect::<Vec<_>>(),
        "suppressed":suppressed,"suppressed_storage":layout::<Range<SignalSample>>(suppressed.capacity()),
        "lookup":lookup,"work":work,"scope":"ordinary Mix control, outside selected factory mutation"})
}
pub(super) fn policy(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    signal: SignalInput<'_, '_>,
    range: Range<SignalSample>,
    value: &deadpan_plan::AudioPolicyQuery<SignalSample>,
    phase: &'static str,
    completed: usize,
) {
    mark(renderer, control, phase, || {
        json!({"mode":mode(signal),"range":range,
        "completed_contents":completed,"policy":policy_value(value)})
    });
}
pub(super) fn admitted_policy<I: Copy + Eq, L>(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    signal: SignalInput<'_, '_>,
    range: Range<SignalSample>,
    value: &deadpan_plan::AdmittedSignalPolicyQuery<I, L>,
    phase: &'static str,
    completed: usize,
) {
    mark(renderer, control, phase, || {
        let view = value.view();
        json!({"mode":mode(signal),"range":range,"completed_contents":completed,
            "policy":{"suppressed":view.suppressed,"suppressed_len":view.suppressed.len(),
                "contents":view.contents,"contents_len":view.contents.len(),
                "lookup":view.lookup,"work":view.work,
                "ownership":"actual admitted Signal policy owner retained through preflight",
                "capacities":"unexposed by the public borrowed SignalPolicyView"},
            "admission":{"canonical":value.canonical_units(),"additional":value.additional_units(),
                "total":value.work(),"forwarded":value.was_forwarded()}})
    });
}
pub(super) fn graph(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    value: &AudioSignalQuery<'_>,
    phase: &'static str,
) {
    mark(renderer, control, phase, || {
        graph_value(value, &renderer.plan)
    });
}
