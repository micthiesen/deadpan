//! Active native owners through actual checked Point, Bound and public transfers.
use super::*;
use crate::{DomainSignalTransfer, RootSignalTransfer};

struct NativeTrace;
impl NativeTrace {
    fn start() -> Self {
        reset_receipts();
        point_suppression::ops::trace::reset();
        Self
    }
}
impl Drop for NativeTrace {
    fn drop(&mut self) {
        raw_domain_query_trace::disable();
        point_content_query_trace::disable();
        point_suppression::work_trace::disable();
        point_suppression::ops::trace::disable();
    }
}
fn compile_checked(setup: Checked) -> Arc<RenderPlan> {
    Arc::new(RenderPlan::compile(&setup.document.expect("checked public fixture")).unwrap())
}
fn native_trace() -> Value {
    serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap()
}
fn positive(trace: &Value, field: &str) {
    assert!(
        trace["plan"][field].as_u64().unwrap() > 0,
        "{field}: {trace}"
    );
}
fn provider() -> PublicProvider {
    PublicProvider {
        prepared: cases::prepared(cases::Wav::Stereo48000),
        calls: 0,
        events: Vec::new(),
    }
}
fn assert_signal_owner(
    renderer: &StageAudio,
    value: &crate::stages::SignalSuppressionMask,
) -> point_suppression::Observer {
    assert!(value.is_owned());
    assert!(!value.is_empty());
    assert_eq!(value.unaccounted_capacity(), 0);
    // All three sealed sample labels have the same pinned i64 range layout.
    assert_eq!(
        Layout::new::<Range<SignalSample>>(),
        Layout::new::<Range<ReferenceSample>>()
    );
    let observer = renderer
        .owned_suppression
        .observer()
        .expect("real initialized account");
    assert_eq!(
        observer.live_frames(),
        point_suppression::shell_frames().unwrap()
            + point_suppression::payload_frames(value.capacity()).unwrap()
    );
    observer
}
fn private_read(
    renderer: &mut StageAudio,
    domain: &AudioPointDomain<'_>,
    frames: u32,
    bound: bool,
) -> Result<(SignalBlock, usize), StageAudioError> {
    // Both provider and the actual request ledger end before the caller inspects output.
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut provider = provider();
    let value = if bound {
        let query = domain.signal_ref().query(
            SignalSample(0)..SignalSample(i64::from(frames)),
            query_limits(),
        )?;
        let actual_bound = query
            .spans
            .iter()
            .find_map(|span| match &span.content {
                AudioSignalContent::Bound(value) => Some(value),
                _ => None,
            })
            .expect("checked historical Preserve fixture must expose a genuine Bound");
        renderer.read_bound_at(
            actual_bound,
            &mut provider,
            actual_bound.reference_at_wide_offset(0)?,
            (frames, 0),
            control(&cancelled, &work),
        )?
    } else {
        renderer.read_point_domain_controlled(
            &mut provider,
            domain,
            domain.reference_samples().start,
            frames,
            control(&cancelled, &work),
            0,
        )?
    };
    Ok((value, provider.calls))
}
fn assert_private_survival(setup: Checked, node: &str, frames: u32, bound: bool, calls: usize) {
    let _trace = NativeTrace::start();
    let plan = compile_checked(setup);
    let domain = point(&plan, node).unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let (value, actual_calls) = private_read(&mut renderer, &domain, frames, bound).unwrap();
    assert_eq!(actual_calls, calls);
    let expected_end = if node == "a" { 16 } else { i64::from(frames) };
    assert_eq!(
        value.suppressed.as_slice(),
        &[SignalSample(0)..SignalSample(expected_end)]
    );
    let observer = assert_signal_owner(&renderer, &value.suppressed);
    let held = observer.live_frames();
    let snapshot = returned_signal(&value);
    let trace = native_trace();
    assert_eq!(trace["overflow"], false);
    positive(&trace, "accepted_payload_reservations");
    if !bound && matches!(node, "a" | "quiet") {
        // Plan already owns the final Vec; native admission adds only its Box.
        for field in [
            "accepted_map_units",
            "accepted_vector_units",
            "accepted_scan_units",
        ] {
            assert_eq!(trace["plan"][field], 0);
        }
        assert_eq!(trace["plan"]["accepted_payload_reservations"], 1);
        let vector =
            point_suppression::policy_handoff::vector_frames(value.suppressed.capacity()).unwrap();
        let storage =
            point_suppression::payload_frames(value.suppressed.capacity()).unwrap() - vector;
        assert_eq!(trace["plan"]["accepted_payload_frames"], storage);
        assert_eq!(trace["plan"]["accepted_storage_units"], 1);
    } else {
        positive(&trace, "accepted_map_units");
    }
    if bound {
        let raw = serde_json::to_value(raw_domain_query_trace::snapshot()).unwrap();
        assert_eq!(raw["nested_point_callbacks"], 1);
        positive(&trace, "accepted_zero_units");
        positive(&trace, "accepted_mark_units");
        positive(&trace, "accepted_scan_units");
    }
    drop(renderer);
    assert_eq!(observer.live_frames(), held);
    assert_eq!(returned_signal(&value), snapshot);
    drop(value);
    assert_eq!(
        observer.live_frames(),
        point_suppression::shell_frames().unwrap()
    );
}
fn returned_signal(value: &SignalBlock) -> Value {
    json!({
        "status":"Ok","kind":"SignalBlock",
        "f32_bits":value.samples.iter().map(|sample|sample.map(f32::to_bits)).collect::<Vec<_>>(),
        "sample_storage":layout::<[f32;2]>(value.samples.capacity()),
        "dependencies":dependencies(&value.dependencies),"relative_depth":value.relative_depth,
        "suppressed":signal_mask(&value.suppressed)
    })
}
#[test]
fn active_private_silent_point_owner_outlives_request_and_controller() {
    assert_private_survival(quiet_and_padded(), "quiet", 16, false, 0);
}
#[test]
fn active_private_selected_source_point_owner_outlives_request_and_controller() {
    assert_private_survival(quiet_and_padded(), "a", 32, false, 1);
}
#[test]
fn active_private_point_through_genuine_bound_retains_final_owner() {
    assert_private_survival(bound_preserve(), "b", 32, false, 1);
}
#[test]
fn active_genuine_bound_callback_flags_and_final_mask_are_owned() {
    assert_private_survival(bound_preserve(), "b", 32, true, 1);
}
fn check_public_transfer(physical: bool, source: bool) {
    let _trace = NativeTrace::start();
    let plan = compile_checked(quiet_and_padded());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut provider = provider();
    let cancelled = AtomicBool::new(false);
    let start = SignalSample(400);
    let output = start..SignalSample(432);
    let root_anchor = if source { 64 } else { 0 };
    let (samples, suppressed, identity) = if physical {
        let lookup = if source { 80 } else { 0 };
        let domain = plan
            .audio_domain_at(AudioSample(lookup), Default::default())
            .unwrap();
        let transfer = DomainSignalTransfer::new(
            domain,
            ExactRatio::integer(root_anchor),
            start,
            ExactRatio::ONE,
            output,
        )
        .unwrap();
        let value = renderer
            .read_domain_transferred(&mut provider, &transfer, start, 32, TIMEOUT, &cancelled)
            .unwrap();
        assert_eq!(value.start, start);
        assert_eq!(value.schema_version, 1);
        let identity = json!({
            "schema_version":value.schema_version,"stage":value.stage,
            "project_id":value.project_id,"revision_id":value.revision_id,
            "definition":value.definition,"placement":value.placement,
            "instance":value.instance,"gap_after":value.gap_after,"transfer":value.transfer
        });
        (value.samples, value.suppressed, identity)
    } else {
        let transfer = RootSignalTransfer::new(
            AudioSample(0)..plan.audio_duration().unwrap(),
            ExactRatio::integer(root_anchor),
            start,
            ExactRatio::ONE,
            output,
        )
        .unwrap();
        let value = renderer
            .read_transferred(&mut provider, &transfer, start, 32, TIMEOUT, &cancelled)
            .unwrap();
        assert_eq!(value.start, start);
        assert_eq!(value.schema_version, 1);
        let identity = json!({
            "schema_version":value.schema_version,"stage":value.stage,
            "project_id":value.project_id,"revision_id":value.revision_id,
            "transfer":value.transfer
        });
        (value.samples, value.suppressed, identity)
    };
    assert_eq!(provider.calls, usize::from(source));
    assert_eq!(samples.len(), 32);
    assert_eq!(
        suppressed.as_slice(),
        &[SignalSample(400)..SignalSample(if source { 416 } else { 432 })]
    );
    let observer = assert_signal_owner(&renderer, &suppressed);
    let held = observer.live_frames();
    let bits = samples
        .iter()
        .map(|sample| sample.map(f32::to_bits))
        .collect::<Vec<_>>();
    let mask_before = signal_mask(&suppressed);
    let identity_before = identity.clone();
    let trace = native_trace();
    assert_eq!(trace["overflow"], false);
    for phase in [
        "accepted_zero_units",
        "accepted_scan_units",
        "accepted_mark_units",
        "accepted_map_units",
        "accepted_payload_reservations",
    ] {
        positive(&trace, phase);
    }
    drop(provider);
    drop(renderer);
    assert_eq!(observer.live_frames(), held);
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.map(f32::to_bits))
            .collect::<Vec<_>>(),
        bits
    );
    assert_eq!(signal_mask(&suppressed), mask_before);
    assert_eq!(identity, identity_before);
    drop(suppressed);
    assert_eq!(
        observer.live_frames(),
        point_suppression::shell_frames().unwrap()
    );
}
#[test]
fn active_public_full_root_transfer_owns_quiet_and_selected_source_masks() {
    for source in [false, true] {
        check_public_transfer(false, source);
    }
}
#[test]
fn active_public_physical_domain_transfer_owns_quiet_and_selected_source_masks() {
    for source in [false, true] {
        check_public_transfer(true, source);
    }
}
#[test]
fn active_source_only_point_empty_policy_has_no_account_or_native_mask_fees() {
    let _trace = NativeTrace::start();
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = compile_checked(checked(
        vec![id("a")],
        BTreeMap::from([(id("a"), source(rate, 0, 64, 64))]),
    ));
    let domain = point(&plan, "a").unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let (value, calls) = private_read(&mut renderer, &domain, 32, false).unwrap();
    assert_eq!(calls, 1);
    assert!(value.suppressed.is_empty());
    assert_eq!(value.suppressed.capacity(), 0);
    assert!(!value.suppressed.is_owned());
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    assert!(renderer.owned_suppression.observer().is_none());
    let trace = native_trace();
    assert_eq!(trace["overflow"], false);
    for lane in ["plan", "cache"] {
        for (_, units) in trace[lane].as_object().unwrap() {
            assert_eq!(units.as_u64(), Some(0), "{trace}");
        }
    }
}
#[test]
fn active_held_private_point_and_genuine_bound_charge_limits_requests_and_recovers() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
        held_private_point_and_genuine_bound_charge_limits_requests_and_recovers_historical,
    );
}
fn held_private_point_and_genuine_bound_charge_limits_requests_and_recovers_historical() {
    for bound in [false, true] {
        let _trace = NativeTrace::start();
        let plan = compile_checked(if bound {
            bound_preserve()
        } else {
            quiet_and_padded()
        });
        let domain = point(&plan, if bound { "b" } else { "quiet" }).unwrap();
        let frames = if bound { 32 } else { 16 };
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let (initial, _) = private_read(&mut renderer, &domain, frames, bound).unwrap();
        drop(initial);
        let observer = renderer.owned_suppression.observer().unwrap();
        let shell = point_suppression::shell_frames().unwrap();
        assert_eq!(observer.live_frames(), shell);
        let mut first = None;
        for cap in u32::try_from(shell).unwrap()..=512 {
            renderer.limits.maximum_resident_frames = cap;
            match private_read(&mut renderer, &domain, frames, bound) {
                Ok((value, _)) => {
                    first = Some(value);
                    break;
                }
                Err(StageAudioError::Limit(_)) => assert_eq!(observer.live_frames(), shell),
                Err(error) => panic!("unexpected actual request failure at cap {cap}: {error:?}"),
            }
        }
        let held = first.expect("actual request fits within bounded existing production ceiling");
        assert_signal_owner(&renderer, &held.suppressed);
        let cap = renderer.limits.maximum_resident_frames;
        let mut held = vec![held];
        let mut refused = false;
        // A successful Bound read may warm DSP storage, making later reads
        // cheaper. Each escaped mask must still consume the unchanged cap.
        for _ in 0..cap {
            let live = observer.live_frames();
            match private_read(&mut renderer, &domain, frames, bound) {
                Ok((value, _)) => {
                    assert!(value.suppressed.is_owned());
                    assert_eq!(value.suppressed.unaccounted_capacity(), 0);
                    assert_eq!(
                        observer.live_frames(),
                        live + point_suppression::payload_frames(value.suppressed.capacity())
                            .unwrap()
                    );
                    held.push(value);
                }
                Err(StageAudioError::Limit(_)) => {
                    assert_eq!(observer.live_frames(), live);
                    refused = true;
                    break;
                }
                Err(error) => panic!("unexpected request failure at cap {cap}: {error:?}"),
            }
        }
        assert!(
            refused,
            "escaped native owners must eventually exhaust cap {cap}"
        );
        assert_eq!(renderer.limits.maximum_resident_frames, cap);
        drop(held);
        assert_eq!(observer.live_frames(), shell);
        let (recovered, _) = private_read(&mut renderer, &domain, frames, bound).unwrap();
        assert_eq!(renderer.limits.maximum_resident_frames, cap);
        assert_eq!(
            recovered.suppressed.as_slice(),
            &[SignalSample(0)..SignalSample(i64::from(frames))]
        );
        drop(recovered);
        assert_eq!(observer.live_frames(), shell);
    }
}
