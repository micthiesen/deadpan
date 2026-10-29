//! Actual probe regressions against the frozen old full walk and Point signal.
use super::*;
use crate::plan::audio_owner_input::AdmittedAudioOwnerRetainedProbe;

fn old_probe<'a>(frozen: &'a Value, name: &str) -> &'a Value {
    &frozen["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case"] == name && v["observation"]["kind"] == "actual_retained_probe_result")
        .unwrap()["observation"]
}
fn normalize_core(
    actual: &mut Value,
    old: &Value,
    events: &[Value],
    prefix: &str,
    additional: usize,
) {
    let root = format!("{prefix}/resolved/work");
    if old.pointer(&root).is_none() {
        return;
    }
    let lattice = format!("{prefix}/resolved/lattice/work");
    let receipts = events
        .iter()
        .filter(|v| v["observation"]["kind"] == "actual_capture_core_receipt")
        .collect::<Vec<_>>();
    assert_eq!(receipts.len(), 1);
    let receipt = &receipts[0]["observation"];
    let total = number(actual.pointer(&root).unwrap());
    let old_total = number(old.pointer(&root).unwrap());
    assert_eq!(total, number(&receipt["total"]));
    assert_eq!(total, number(&receipt["resolved_work"]));
    assert_eq!(old_total, number(&receipt["canonical"]));
    let extra = number(&receipt["additional"]);
    assert_eq!(old_total.checked_add(extra), Some(total));
    assert!(extra <= additional);
    let nested = number(actual.pointer(&lattice).unwrap());
    let old_nested = number(old.pointer(&lattice).unwrap());
    assert_eq!(nested, number(&receipt["lattice_work"]));
    assert!(nested >= old_nested && nested - old_nested <= extra && nested <= total);
    *actual.pointer_mut(&root).unwrap() = json!(old_total);
    *actual.pointer_mut(&lattice).unwrap() = json!(old_nested);
}
fn check<const M: bool>(parent: Parent<'_, '_>, case: &Value, frozen: &Value) {
    let name = case["name"].as_str().unwrap();
    let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
    let member = number(&case["selected_member_index"]);
    let mut p = policy::<M>(usize::MAX);
    let source = admitted(parent, at, Default::default(), &mut p)
        .unwrap()
        .capture(member, Default::default(), &mut p)
        .unwrap();
    let before = p.0.borrow().forwarded;
    let (probe, events) = capture_baseline::observe_case(name, "actual_probe", || {
        source
            .input()
            .unwrap()
            .retained_probe_at_admitted(at, Default::default(), &mut p)
    });
    let probe = probe.unwrap();
    let old = old_probe(frozen, name);
    assert_eq!(
        probe.canonical_units(),
        number(&old["work"]),
        "old P canonical {name}"
    );
    assert_eq!(
        probe.work(),
        probe.canonical_units() + probe.additional_units()
    );
    assert_eq!(probe.was_forwarded(), M);
    assert_eq!(
        p.0.borrow().forwarded - before,
        if M { probe.work() } else { 0 }
    );
    let mut actual = probe
        .input()
        .map(|v| v.capture_baseline_snapshot(parent.plan()))
        .unwrap_or(Value::Null);
    if M && !actual.is_null() {
        normalize_core(
            &mut actual,
            &old["retained_input"],
            &events,
            "/processing/content/value",
            probe.additional_units(),
        );
        actual["work"] = old["work"].clone();
    }
    assert_eq!(
        actual, old["retained_input"],
        "complete actual retained {name}"
    );
    let walked = events
        .iter()
        .find(|v| v["observation"]["kind"] == "actual_retained_probe_walk")
        .unwrap();
    let old_walk = frozen["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case"] == name && v["observation"]["kind"] == "actual_retained_probe_walk")
        .unwrap();
    assert_eq!(
        walked["observation"]["editorial"],
        old_walk["observation"]["editorial"]
    );
    let mut span = walked["observation"]["walk"].clone();
    if M {
        normalize_core(
            &mut span,
            &old_walk["observation"]["walk"],
            &events,
            "/span/value/content/value",
            probe.additional_units(),
        );
    }
    assert_eq!(
        span, old_walk["observation"]["walk"],
        "complete discarded/retained span {name}"
    );
    if let Some(retained) = probe.input()
        && let AudioBoundDomain::Point(point) = retained.domain()
    {
        let frozen_point: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/owner-probe-point-baseline-v1/supplement.json"
        ))
        .unwrap();
        assert_eq!(
            point.probe_baseline_snapshot(parent.plan()),
            frozen_point["point_private_signals"][0]["point"]
        );
    }
    let source_live = p.0.borrow().live;
    drop(probe);
    assert!(p.0.borrow().live <= source_live);
    drop(source);
    assert_eq!(p.0.borrow().live, 0);
}
#[test]
fn actual_probe_all_sixteen_cases_both_modes_full_old_fields_and_private_point() {
    let mut count = 0;
    parent_cases(|parent, case, frozen| {
        check::<false>(parent, case, frozen);
        check::<true>(parent, case, frozen);
        count += 1;
    });
    assert_eq!(count, 16);
}

fn with_input(run: impl FnOnce(&RenderPlan, &crate::AudioOwnerInput<'_>, AudioSample)) {
    let frozen = golden();
    let fixture=frozen["fixtures"].as_array().unwrap().iter().find(|v|
        v["fixture"]=="actual_source_split_with_preexisting_lattice_retained_root_round_even_clipped_window"
    ).unwrap();
    let case = &fixture["cases"][0];
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&fixture["document"].to_string()).unwrap())
            .unwrap();
    let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
    let source = plan
        .audio_owner_input_candidates_at(at, Default::default())
        .unwrap()
        .capture(number(&case["selected_member_index"]), Default::default())
        .unwrap()
        .into_input()
        .unwrap();
    run(&plan, &source, at);
}
fn successful<'plan>(
    input: &crate::AudioOwnerInput<'plan>,
    at: AudioSample,
    p: &mut Policy<true>,
) -> AdmittedAudioOwnerRetainedProbe<'plan, usize, Token> {
    input
        .retained_probe_at_admitted(at, Default::default(), p)
        .unwrap()
}
#[test]
fn real_probe_peak_exact_one_short_cancellation_and_no_work_refund() {
    with_input(|_, input, at| {
        let mut p = policy::<true>(usize::MAX);
        let probe = successful(input, at, &mut p);
        assert!(probe.input().is_some());
        let peak = p.0.borrow().peak;
        let retained = p.0.borrow().live;
        assert!(retained > 0);
        assert!(
            peak > retained,
            "discarded walker workspaces die before returned retained graph"
        );
        let work = p.0.borrow().forwarded;
        drop(probe);
        assert_eq!(p.0.borrow().live, 0);
        assert_eq!(p.0.borrow().forwarded, work);
        let mut exact = policy::<true>(peak);
        let probe = successful(input, at, &mut exact);
        assert_eq!(exact.0.borrow().peak, peak);
        assert_eq!(exact.0.borrow().forwarded, work);
        drop(probe);
        assert_eq!(exact.0.borrow().live, 0);
        let mut short = policy::<true>(peak - 1);
        assert!(
            input
                .retained_probe_at_admitted(at, Default::default(), &mut short)
                .is_err()
        );
        assert_eq!(short.0.borrow().live, 0);
        assert!(short.0.borrow().forwarded > 0);
        let mut cancelled = policy::<true>(usize::MAX);
        cancelled.0.borrow_mut().fail_work = true;
        assert!(matches!(
            input.retained_probe_at_admitted(at, Default::default(), &mut cancelled),
            Err(PlanError::AudioQueryLimit("test cancellation"))
        ));
        assert_eq!(cancelled.0.borrow().live, 0);
    });
}
#[test]
fn accepted_same_probe_shell_preflight_issuer_retry_and_one_joint_commit() {
    with_input(|plan, input, at| {
        let mut p = policy::<true>(usize::MAX);
        let probe = successful(input, at, &mut p);
        let bound_address = probe.input().unwrap().bound() as *const _;
        let before = p.0.borrow().live;
        let work = probe.work();
        let mut original = probe
            .into_shared_admitted(Default::default(), &mut p)
            .unwrap();
        assert!(p.0.borrow().live > before);
        assert!(p.0.borrow().forwarded > work);
        // Inner Box is MOVED into the actual shell; it has not been deep-cloned.
        assert_eq!(
            original.retained().unwrap().bound() as *const _,
            bound_address
        );
        assert!(original.retained().unwrap().belongs_to(plan));
        assert!(original.leases()[0].is_some());
        assert!(
            original.leases()[1].is_some(),
            "actual Core resolver lease remains owned"
        );
        let first_identity = original.retained().unwrap().as_ptr();
        let copy = original.retained().unwrap().clone();
        assert!(copy.ptr_eq(original.retained().unwrap()));
        assert_eq!(copy.as_ptr(), first_identity);
        drop(copy);
        let mut foreign = policy::<true>(usize::MAX);
        let calls = p.0.borrow().calls;
        assert!(
            crate::plan::audio_domain::preflight_context_transfer(
                Some(&mut original),
                None,
                &mut foreign
            )
            .is_err()
        );
        assert_eq!(p.0.borrow().calls, calls);
        assert_eq!(foreign.0.borrow().calls, 0);
        {
            let token = crate::plan::audio_domain::preflight_context_transfer(
                Some(&mut original),
                None,
                &mut p,
            )
            .unwrap();
            assert!(token.leases()[0].is_some() && token.leases()[1].is_some());
            // The token holds no mutable policy borrow.
            assert_eq!(p.issuer(), original_issuer(&p));
        }
        let token = crate::plan::audio_domain::preflight_context_transfer(
            Some(&mut original),
            None,
            &mut p,
        )
        .unwrap();
        let mut called = false;
        token.commit(|plan, core, buffer, sound| {
            assert!(plan.is_some() && core.is_some() && buffer.is_none() && sound.is_none());
            called = true;
        });
        assert!(called);
        assert!(
            crate::plan::audio_domain::preflight_context_transfer(
                Some(&mut original),
                None,
                &mut p
            )
            .is_err(),
            "committed Core cannot transfer twice"
        );
        drop(original);
        assert_eq!(p.0.borrow().live, 0);
    });
}
fn original_issuer(p: &Policy<true>) -> usize {
    p.issuer()
}
