//! Actual checked native reads explain query intervals without residual fees.
use super::*;
use crate::stages::native_query_interval_trace as intervals;

const PHASES: [&str; 8] = [
    "accepted_account_units",
    "accepted_scan_units",
    "accepted_map_units",
    "accepted_zero_units",
    "accepted_mark_units",
    "accepted_vector_units",
    "accepted_storage_units",
    "accepted_host_residency_units",
];
struct Capture;
impl Capture {
    fn start() -> Self {
        reset_receipts();
        point_suppression::ops::trace::reset();
        intervals::reset();
        Self
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        intervals::disable();
        point_suppression::ops::trace::disable();
        raw_domain_query_trace::disable();
        point_content_query_trace::disable();
        point_suppression::work_trace::disable();
    }
}
fn n(value: &Value) -> u64 {
    value.as_u64().expect("literal unsigned observer counter")
}
fn delta(after: &Value, before: &Value) -> u64 {
    n(after)
        .checked_sub(n(before))
        .expect("monotonic literal counters")
}
fn total(values: impl IntoIterator<Item = u64>) -> u64 {
    values
        .into_iter()
        .try_fold(0_u64, |sum, value| sum.checked_add(value))
        .unwrap()
}
fn snapshot() -> Value {
    serde_json::to_value(intervals::snapshot()).unwrap()
}
fn native() -> Value {
    serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap()
}
fn work_lanes(work: &RefCell<ReadWork>) -> [u64; 2] {
    let work = work.borrow();
    [
        u64::try_from(work.plan_work).unwrap(),
        u64::try_from(work.cache_plan_work).unwrap(),
    ]
}
fn phase_deltas(row: &Value, lane: usize) -> [u64; 8] {
    std::array::from_fn(|phase| {
        delta(
            &row["native_after"]["lanes"][lane][phase],
            &row["native_before"]["lanes"][lane][phase],
        )
    })
}
fn work_delta(row: &Value, lane: usize) -> u64 {
    delta(&row["work_after"][lane], &row["work_before"][lane])
}
fn host_delta(row: &Value, lane: usize) -> u64 {
    delta(
        &row["host_after"]["units"][lane],
        &row["host_before"]["units"][lane],
    )
}
fn whole_units(value: &Value, lane: usize) -> u64 {
    let lane = if lane == 0 { "plan" } else { "cache" };
    total(PHASES.map(|phase| n(&value[lane][phase])))
}
fn complete(value: &Value) {
    assert_eq!(value["schema"], "native-mask-query-intervals-v1");
    assert_eq!(value["active_intervals"], 0);
    assert_eq!(value["dropped_intervals"], 0);
    for field in ["misnested", "overflow"] {
        assert_eq!(value[field], false, "{value}");
    }
    assert_eq!(
        value["phase_order"],
        json!([
            "account",
            "scan",
            "map",
            "zero",
            "mark",
            "allocate_vector",
            "allocate_storage",
            "host_residency",
        ])
    );
}
fn reconcile(value: &Value, whole: &Value, selected: usize) -> usize {
    complete(value);
    assert_eq!(whole["overflow"], false);
    let mut completed = 0;
    for row in value["rows"].as_array().unwrap() {
        assert_eq!(row["selected_lane"], selected);
        assert_eq!(row["foreign_work_overlap"], false, "{row}");
        assert_eq!(row["attributable"], true, "{row}");
        assert_eq!(row["native_before"]["enabled"], true);
        assert_eq!(row["native_after"]["enabled"], true);
        assert!(row["native_before"]["reset_epoch"].is_u64());
        assert_eq!(
            row["native_before"]["reset_epoch"],
            row["native_after"]["reset_epoch"]
        );
        assert!(row["host_before"]["reset_epoch"].is_u64());
        assert_eq!(
            row["host_before"]["reset_epoch"],
            row["host_after"]["reset_epoch"]
        );
        for lane in 0..2 {
            // Each row is inclusive. Compare it independently to whole-call
            // counters; parent and child rows must never be summed.
            assert!(
                total(phase_deltas(row, lane)) <= whole_units(whole, lane),
                "{row}"
            );
        }
        let claim = &row["claim"];
        if claim.is_null() {
            continue;
        }
        completed += 1;
        assert_eq!(
            n(&claim["canonical"]).checked_add(n(&claim["additional"])),
            Some(n(&claim["total"]))
        );
        for lane in 0..2 {
            let query = if lane == selected {
                n(&claim["expected_debit"])
            } else {
                0
            };
            let explicit = total([query, total(phase_deltas(row, lane)), host_delta(row, lane)]);
            assert_eq!(
                work_delta(row, lane),
                explicit,
                "literal query + independently accepted native phases + host eviction: {row}"
            );
        }
    }
    completed
}
fn plan(setup: Checked) -> Arc<RenderPlan> {
    Arc::new(RenderPlan::compile(&setup.document.expect("checked public fixture")).unwrap())
}
fn provider() -> PublicProvider {
    PublicProvider {
        prepared: cases::prepared(cases::Wav::Stereo48000),
        calls: 0,
        events: Vec::new(),
    }
}
fn hold_public(renderer: &mut StageAudio, plan: &RenderPlan) -> PointDomainAudioBlock {
    let domain = point(plan, "quiet").unwrap();
    renderer
        .read_point_domain(
            &mut provider(),
            &domain,
            ReferenceSample(16),
            16,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
}
fn point_read(
    renderer: &mut StageAudio,
    provider: &mut PublicProvider,
    domain: &AudioPointDomain<'_>,
    control: WorkControl<'_>,
) -> Result<SignalBlock, StageAudioError> {
    renderer.read_point_domain_controlled(
        provider,
        domain,
        domain.reference_samples().start,
        32,
        control,
        0,
    )
}
fn clean_query_lifetimes(work: &RefCell<ReadWork>) {
    assert_eq!(work.borrow().sound_context_frames, 0);
    let point = serde_json::to_value(point_content_query_trace::snapshot()).unwrap();
    let raw = serde_json::to_value(raw_domain_query_trace::snapshot()).unwrap();
    for value in [point, raw] {
        assert_eq!(value["active_depth"], 0);
        assert_eq!(value["live_results"], 0);
        assert_eq!(value["result_creations"], value["result_drops"]);
        assert_eq!(value["overflow"], false);
        // Existing invalid flags remain literal, without normalization.
    }
}

#[test]
fn actual_point_constructor_intervals_reconcile_both_lanes_and_exclude_native_tail() {
    for cache in [false, true] {
        let plan = plan(quiet_and_padded());
        let domain = point(&plan, "a").unwrap();
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let mut provider = provider();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&cancelled, &work);
        let scope = cache.then(|| control.cache_key_scope());
        let _capture = Capture::start();
        let output = point_read(&mut renderer, &mut provider, &domain, control).unwrap();
        assert!(output.suppressed.is_owned());
        assert_eq!(
            output.suppressed.as_slice(),
            &[SignalSample(0)..SignalSample(16)]
        );
        assert_eq!(provider.calls, 1);
        let value = snapshot();
        let whole = native();
        assert!(reconcile(&value, &whole, usize::from(cache)) > 0);
        let row = value["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "point_content")
            .expect("actual Point interval");
        assert!(row["claim"].is_object());
        assert_eq!(total(phase_deltas(row, usize::from(cache))), 0);
        assert!(whole_units(&whole, usize::from(cache)) > 0);
        assert_eq!(whole_units(&whole, usize::from(!cache)), 0);
        clean_query_lifetimes(&work);
        drop(output);
        drop(scope);
    }
}

#[test]
fn actual_held_public_point_preserves_36_equals_26_plus_10_and_old_invalid_flag() {
    for cache in [false, true] {
        let plan = plan(quiet_and_padded());
        let domain = point(&plan, "a").unwrap();
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let held = hold_public(&mut renderer, &plan);
        assert!(held.suppressed.is_owned());
        let held_live = renderer.owned_suppression.live_frames();
        let mut provider = provider();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&cancelled, &work);
        let scope = cache.then(|| control.cache_key_scope());
        let _capture = Capture::start();
        let output = point_read(&mut renderer, &mut provider, &domain, control).unwrap();
        assert_eq!(provider.calls, 1);
        let value = snapshot();
        let whole = native();
        let selected = usize::from(cache);
        assert!(reconcile(&value, &whole, selected) > 0);
        let row = value["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "point_content")
            .expect("actual held Point interval");
        assert_eq!(row["claim"]["canonical"], 2, "{row}");
        assert_eq!(row["claim"]["additional"], 24, "{row}");
        assert_eq!(row["claim"]["total"], 26, "{row}");
        assert_eq!(row["claim"]["expected_debit"], 26, "{row}");
        assert_eq!(row["claim"]["forwarded"], true);
        assert_eq!(row["claim"]["ordinary"], false);
        assert_eq!(work_delta(row, selected), 36, "{row}");
        let phases = phase_deltas(row, selected);
        assert_eq!(phases, [0, 0, 0, 0, 0, 0, 0, 10], "{row}");
        assert_eq!(host_delta(row, selected), 0, "{row}");
        let old = serde_json::to_value(point_content_query_trace::snapshot()).unwrap();
        assert_eq!(old["invalid"], true);
        let lane = if cache { "cache" } else { "plan" };
        assert_eq!(old[lane]["accepted_work"], 36);
        assert_eq!(old[lane]["canonical_units"], 2);
        assert_eq!(old[lane]["additional_units"], 24);
        assert!(whole_units(&whole, selected) > total(phases));
        clean_query_lifetimes(&work);
        drop(output);
        assert_eq!(renderer.owned_suppression.live_frames(), held_live);
        drop(held);
        assert_eq!(
            renderer.owned_suppression.live_frames(),
            point_suppression::shell_frames().unwrap()
        );
        drop(scope);
    }
}

#[test]
fn actual_genuine_bound_raw_and_point_intervals_reconcile_in_both_lanes() {
    for cache in [false, true] {
        let plan = plan(bound_preserve());
        let domain = point(&plan, "b").unwrap();
        let query = domain
            .signal_ref()
            .query(SignalSample(0)..SignalSample(32), query_limits())
            .unwrap();
        let bound = query
            .spans
            .iter()
            .find_map(|span| match &span.content {
                AudioSignalContent::Bound(bound) => Some(bound),
                _ => None,
            })
            .expect("public checked query must reach actual Preserve Bound");
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let mut provider = provider();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&cancelled, &work);
        let scope = cache.then(|| control.cache_key_scope());
        let _capture = Capture::start();
        let output = renderer
            .read_bound_at(
                bound,
                &mut provider,
                bound.reference_at_wide_offset(0).unwrap(),
                (32, 0),
                control,
            )
            .unwrap();
        assert!(output.suppressed.is_owned());
        assert!(!output.suppressed.is_empty());
        assert_eq!(provider.calls, 1);
        let value = snapshot();
        let whole = native();
        assert!(reconcile(&value, &whole, usize::from(cache)) >= 2);
        for kind in ["raw_domain", "point_content"] {
            assert!(
                value["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["kind"] == kind && row["claim"].is_object()),
                "{value}"
            );
        }
        let raw = serde_json::to_value(raw_domain_query_trace::snapshot()).unwrap();
        assert_eq!(raw["nested_point_callbacks"], 1);
        clean_query_lifetimes(&work);
        drop(output);
        assert_eq!(
            renderer.owned_suppression.live_frames(),
            point_suppression::shell_frames().unwrap()
        );
        drop(scope);
    }
}

#[test]
fn actual_held_point_plan_budget_partial_has_no_claim_and_keeps_literal_prefix() {
    let plan = plan(quiet_and_padded());
    let domain = point(&plan, "a").unwrap();
    let mut found = false;
    // Cache's real early guard is tested separately below. Only Plan permits
    // reducing the constructor's Core allowance to a partial accepted prefix.
    for remaining in 1..36 {
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let held = hold_public(&mut renderer, &plan);
        let held_live = renderer.owned_suppression.live_frames();
        let mut provider = provider();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&cancelled, &work);
        // Real accepted prior debit, before the operation boundary. No counter
        // assignment, surrogate ledger, canonical debit or phase is invented.
        control
            .spend_plan_work(MAX_PLAN_WORK_PER_READ - remaining)
            .unwrap();
        let before = work_lanes(&work);
        let _capture = Capture::start();
        let result = point_read(&mut renderer, &mut provider, &domain, control);
        let value = snapshot();
        let whole = native();
        reconcile(&value, &whole, 0);
        let partial = value["rows"].as_array().unwrap().iter().find(|row| {
            row["claim"].is_null()
                && row["attributable"] == true
                && work_delta(row, 0) > 0
                && total(phase_deltas(row, 0)) > 0
        });
        if let Some(row) = partial {
            assert!(
                matches!(
                    result,
                    Err(StageAudioError::Limit(_))
                        | Err(StageAudioError::Plan(PlanError::AudioQueryLimit(_)))
                ),
                "actual budget error required: {value}"
            );
            assert_eq!(provider.calls, 0);
            let after = work_lanes(&work);
            assert!(after[0] - before[0] <= u64::try_from(remaining).unwrap());
            assert_eq!(after[1], before[1]);
            assert_eq!(work_delta(row, 0), after[0] - before[0]);
            assert_eq!(work_delta(row, 1), 0);
            assert_eq!(whole_units(&whole, 0), total(phase_deltas(row, 0)));
            assert_eq!(whole_units(&whole, 1), 0);
            // No completed claim is fabricated from the remainder. The old
            // partial receipt is only the actual accepted constructor prefix.
            let old = serde_json::to_value(point_content_query_trace::snapshot()).unwrap();
            assert_eq!(old["plan"]["completed_queries"], 0);
            assert_eq!(old["plan"]["canonical_units"], 0);
            assert_eq!(old["plan"]["additional_units"], 0);
            assert_eq!(n(&old["plan"]["partial_work"]), work_delta(row, 0));
            assert_eq!(renderer.owned_suppression.live_frames(), held_live);
            clean_query_lifetimes(&work);
            found = true;
            drop(held);
            break;
        }
        drop(result);
        assert_eq!(renderer.owned_suppression.live_frames(), held_live);
        drop(held);
    }
    assert!(
        found,
        "bounded actual budget sweep must expose a native accepted partial prefix"
    );
}

#[test]
fn actual_cache_budget_guard_refuses_before_interval_without_inventing_partial_claim() {
    let plan = plan(quiet_and_padded());
    let domain = point(&plan, "a").unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let held = hold_public(&mut renderer, &plan);
    let held_live = renderer.owned_suppression.live_frames();
    let mut provider = provider();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&cancelled, &work);
    let scope = control.cache_key_scope();
    let allowance = query_limits().maximum_work;
    control
        .spend_plan_work(MAX_PLAN_WORK_PER_READ - allowance + 1)
        .unwrap();
    let before = work_lanes(&work);
    let _capture = Capture::start();
    let result = point_read(&mut renderer, &mut provider, &domain, control);
    assert!(matches!(
        result,
        Err(StageAudioError::Limit(CACHE_KEY_WORK_LIMIT))
    ));
    let value = snapshot();
    let whole = native();
    complete(&value);
    assert!(value["rows"].as_array().unwrap().is_empty());
    assert_eq!(work_lanes(&work), before);
    assert_eq!(provider.calls, 0);
    assert_eq!(whole_units(&whole, 0), 0);
    assert_eq!(whole_units(&whole, 1), 0);
    assert_eq!(renderer.owned_suppression.live_frames(), held_live);
    clean_query_lifetimes(&work);
    drop(held);
    drop(scope);
}
