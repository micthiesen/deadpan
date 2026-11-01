//! Default-off borrowed observations; JSON storage is outside native admission.
use super::*;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

const MAX_ROWS: usize = 256;
thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}
#[derive(Default)]
struct Capture {
    attempted: usize,
    rows: Vec<Value>,
}
pub(super) fn begin() {
    CAPTURE.with(|slot| *slot.borrow_mut() = Some(Capture::default()));
}
pub(super) fn take() -> Value {
    CAPTURE.with(|slot| {
        let value = slot.borrow_mut().take().unwrap_or_default();
        json!({"attempted":value.attempted,"dropped":value.attempted-value.rows.len(),"rows":value.rows})
    })
}
pub(super) fn record(
    signal: SignalInput<'_, '_>,
    control: WorkControl<'_>,
    start: SignalSample,
    samples: &[[f32; 2]],
    policy: &deadpan_plan::AudioPolicyQuery<SignalSample>,
    phase: &'static str,
    completed_contents: usize,
) {
    record_inner(
        signal,
        control,
        start,
        samples.len(),
        Some(samples),
        policy,
        phase,
        completed_contents,
    );
}
pub(super) fn record_preflight(
    signal: SignalInput<'_, '_>,
    control: WorkControl<'_>,
    start: SignalSample,
    end: SignalSample,
    policy: &deadpan_plan::AudioPolicyQuery<SignalSample>,
    phase: &'static str,
) {
    record_inner(
        signal,
        control,
        start,
        usize::try_from(end.0 - start.0).unwrap(),
        None,
        policy,
        phase,
        if phase.ends_with("accepted") {
            policy.contents.len()
        } else {
            0
        },
    );
}
#[allow(clippy::too_many_arguments)]
fn record_inner(
    signal: SignalInput<'_, '_>,
    control: WorkControl<'_>,
    start: SignalSample,
    frames: usize,
    samples: Option<&[[f32; 2]]>,
    policy: &deadpan_plan::AudioPolicyQuery<SignalSample>,
    phase: &'static str,
    completed_contents: usize,
) {
    CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(capture) = slot.as_mut() else { return };
        capture.attempted += 1;
        if capture.rows.len() == MAX_ROWS { return; }
        let mode = match signal {
            SignalInput::Signal(_) => "Signal",
            SignalInput::Tape(_) => "Tape",
            SignalInput::PreserveTape(_) => "PreserveTape",
            SignalInput::Mix(_) => "Mix",
        };
        let work = control.work.borrow();
        let deadpan_plan::AudioPolicyQuery { suppressed, contents, lookup, work: canonical } = policy;
        capture.rows.push(json!({
            "mode":mode,"phase":phase,"start":start.0,"frames":frames,
            "completed_contents":completed_contents,
            "policy":{"suppressed":suppressed.iter().map(|r|[r.start.0,r.end.0]).collect::<Vec<_>>(),
                "suppressed_len":suppressed.len(),"suppressed_capacity":suppressed.capacity(),
                "contents":contents,"contents_len":contents.len(),"contents_capacity":contents.capacity(),
                "lookup":lookup,"canonical_work":canonical,
                "source_ownership":"ordinary unadmitted Vec and inventory; no source lease exists"},
            "f32_bits":samples.map(|values|values.iter().map(|v|v.map(f32::to_bits)).collect::<Vec<_>>()),
            "accepted_work":{"plan":work.plan_work,"cache":work.cache_plan_work},
            "sound_context_frames":work.sound_context_frames,
            "projected_resident_frames":work.projected_resident_frames,
            "projected_disk_bytes":work.projected_disk_bytes,
            "cancelled":control.cancelled.load(Ordering::Relaxed),
        }));
    });
}
