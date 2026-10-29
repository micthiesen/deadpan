//! Real capture/DSP-boundary semantics from the independently frozen OLD walk.
use super::*;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::capture_baseline;

fn golden() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn parent_cases(mut check: impl FnMut(Parent<'_, '_>, &Value, &Value)) {
    let frozen = golden();
    for fixture in frozen["fixtures"].as_array().unwrap() {
        let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let plan = RenderPlan::compile(&doc).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            if case["parent_kind"] == "Root" {
                check(Parent::Root(&plan), case, &frozen);
            } else if case["name"] == "actual_hidden_reference_group" {
                let observed = plan
                    .audio_gain_owners(AudioSample(1)..AudioSample(2), Default::default())
                    .unwrap();
                let group = observed.spans()[0]
                    .owners()
                    .iter()
                    .find(|v| v.instance.node.as_str() == "group")
                    .unwrap();
                let captured = plan
                    .audio_owner_input(group, AudioSample(1), Default::default())
                    .unwrap();
                let retained = captured
                    .retained_at(AudioSample(1), Default::default())
                    .unwrap()
                    .unwrap();
                let AudioBoundDomain::Root(reference) = retained.domain() else {
                    panic!("actual hidden reference")
                };
                check(Parent::Domain(reference), case, &frozen);
            } else {
                let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
                let observed = plan
                    .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
                    .unwrap();
                let owner = observed.spans()[0]
                    .owners()
                    .iter()
                    .find(|v| {
                        v.instance.node.as_str() == case["selected_alias"].as_str().unwrap()
                            && kind(v.kind) == case["selected_kind"].as_str().unwrap()
                    })
                    .unwrap();
                let captured = plan
                    .audio_owner_input(owner, at, Default::default())
                    .unwrap();
                check(Parent::Domain(captured.context()), case, &frozen);
            }
        }
    }
    for fixture in frozen["supplemental"].as_array().unwrap() {
        let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let plan = RenderPlan::compile(&doc).unwrap();
        check(Parent::Root(&plan), &fixture["case"], &frozen);
    }
}
fn check_capture<const M: bool>(parent: Parent<'_, '_>, case: &Value, frozen: &Value) {
    let name = case["name"].as_str().unwrap();
    let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
    let member = number(&case["selected_member_index"]);
    let old_q = number(&case["legacy_q"]);
    let old_c = number(&case["legacy_c"]);
    let old_event = frozen["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case"] == name && v["phase"] == "candidate_capture_ample")
        .unwrap();
    let mut p = policy::<M>(usize::MAX);
    let (result, events) = capture_baseline::observe_case(name, "admitted_capture_ample", || {
        let query = admitted(parent, at, Default::default(), &mut p).unwrap();
        capture_baseline::phase("actual_selected_capture");
        query.capture(member, Default::default(), &mut p)
    });
    let result = result.unwrap();
    assert_eq!(result.query_canonical_units(), old_q);
    assert_eq!(result.capture_canonical_units(), old_c);
    assert_eq!(result.query_work(), old_q + result.query_additional_units());
    assert_eq!(
        result.capture_work(),
        old_c + result.capture_additional_units()
    );
    assert_eq!(result.was_forwarded(), M);
    assert_eq!(result.work(), result.query_work() + result.capture_work());
    assert_eq!(p.0.borrow().forwarded, if M { result.work() } else { 0 });
    let input = result.input().unwrap();
    assert_eq!(input.work(), result.work());
    let mut actual = super::super::input(input);
    actual["total_work"] = json!(old_q + old_c);
    assert_eq!(actual, case["legacy_input"]);
    let observed = events
        .iter()
        .find(|v| v["observation"]["kind"] == "actual_audio_walk_owner_input")
        .unwrap();
    for key in ["at", "selected_observation", "capture_slot"] {
        assert_eq!(
            observed["observation"][key], old_event["observation"][key],
            "frozen actual captured slot and selected provenance: {name}/{key}"
        );
    }
    let mut actual_walk = observed["observation"]["walk"].clone();
    let old_walk = &old_event["observation"]["walk"];
    let root = "/discarded_span/value/content/value/resolved/work";
    let lattice = "/discarded_span/value/content/value/resolved/lattice/work";
    if M && old_walk.pointer(root).is_some() {
        let receipts = events
            .iter()
            .filter(|v| {
                v["phase"] == "actual_selected_capture"
                    && v["observation"]["kind"] == "actual_capture_core_receipt"
            })
            .collect::<Vec<_>>();
        assert_eq!(
            receipts.len(),
            1,
            "one actual opaque Bound stops the capture walk"
        );
        let receipt = &receipts[0]["observation"];
        let actual_root = number(actual_walk.pointer(root).unwrap());
        let old_root = number(old_walk.pointer(root).unwrap());
        let actual_lattice = number(actual_walk.pointer(lattice).unwrap());
        let old_lattice = number(old_walk.pointer(lattice).unwrap());
        assert_eq!(actual_root, number(&receipt["resolved_work"]));
        assert_eq!(actual_root, number(&receipt["total"]));
        assert_eq!(old_root, number(&receipt["canonical"]));
        let core_added = number(&receipt["additional"]);
        assert_eq!(old_root.checked_add(core_added), Some(actual_root));
        assert_eq!(receipt["forwarded"], true);
        assert!(core_added <= result.capture_additional_units());
        // The nested template fork has no separate receipt. This is an
        // observed bounded delta, not a fabricated per-lattice category split.
        assert_eq!(actual_lattice, number(&receipt["lattice_work"]));
        assert!(actual_lattice >= old_lattice);
        assert!(actual_lattice - old_lattice <= core_added);
        assert!(actual_lattice <= actual_root);
        *actual_walk.pointer_mut(root).unwrap() = json!(old_root);
        *actual_walk.pointer_mut(lattice).unwrap() = json!(old_lattice);
    }
    assert_eq!(
        actual_walk, *old_walk,
        "all actual complete discarded-span fields except checked total-work provenance: {name}"
    );
    if M {
        assert!(result.capture_additional_units() > 0);
        assert!(
            p.0.borrow().live > 0,
            "source Input stays charged while borrowed"
        );
    } else {
        assert_eq!(result.capture_additional_units(), 0);
        assert_eq!(p.0.borrow().calls, 0);
    }
    let spent = p.0.borrow().forwarded;
    drop(result);
    assert_eq!(p.0.borrow().live, 0);
    assert_eq!(
        p.0.borrow().forwarded,
        spent,
        "drop releases memory, never work"
    );
}
#[test]
fn full_old_discarded_spans_seeds_and_inputs_match_both_modes_all_sixteen_cases() {
    let mut count = 0;
    parent_cases(|parent, case, frozen| {
        check_capture::<false>(parent, case, frozen);
        check_capture::<true>(parent, case, frozen);
        count += 1;
    });
    assert_eq!(count, 16);
}
#[test]
fn actual_selected_capture_peak_exact_and_one_byte_short_cleanup() {
    let frozen = golden();
    let fixture=frozen["fixtures"].as_array().unwrap().iter().find(|v|
        v["fixture"]=="actual_source_split_with_preexisting_lattice_retained_root_round_even_clipped_window").unwrap();
    let case = &fixture["cases"][0];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let parent = Parent::Root(&plan);
    let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
    let member = number(&case["selected_member_index"]);
    let mut p = policy::<true>(usize::MAX);
    let candidates = admitted(parent, at, Default::default(), &mut p).unwrap();
    let query_peak = p.0.borrow().peak;
    let capture = candidates
        .capture(member, Default::default(), &mut p)
        .unwrap();
    let peak = p.0.borrow().peak;
    let spent = p.0.borrow().forwarded;
    assert!(
        peak > query_peak,
        "actual full capture must overlap its already-live query"
    );
    assert!(p.0.borrow().live > 0);
    drop(capture);
    assert_eq!(p.0.borrow().live, 0);
    let mut exact = policy::<true>(peak);
    let capture = admitted(parent, at, Default::default(), &mut exact)
        .unwrap()
        .capture(member, Default::default(), &mut exact)
        .unwrap();
    assert_eq!(exact.0.borrow().peak, peak);
    assert_eq!(exact.0.borrow().forwarded, spent);
    drop(capture);
    assert_eq!(exact.0.borrow().live, 0);
    let mut short = policy::<true>(peak - 1);
    let (result, events) = capture_baseline::observe_case("peak_short", "actual", || {
        admitted(parent, at, Default::default(), &mut short)
            .unwrap()
            .capture(member, Default::default(), &mut short)
    });
    assert!(matches!(
        result,
        Err(PlanError::AudioQueryLimit("test allocation refusal"))
    ));
    assert!(
        events.iter().any(
            |v| v["observation"]["kind"] == "actual_capture_policy_refusal"
                && !v["observation"]["capture_slot"].is_null()
        ),
        "late allocation refusal must observe an actual independently guarded captured seed"
    );
    assert_eq!(short.0.borrow().live, 0);
    assert!(short.0.borrow().forwarded > 0);
}
#[test]
fn capture_cancellation_keeps_accepted_work_and_releases_consumed_query() {
    let doc = document(48_000, &["voice"], vec![("voice", hold(4))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let mut p = policy::<true>(usize::MAX);
    let candidates = admitted(
        Parent::Root(&plan),
        AudioSample(1),
        Default::default(),
        &mut p,
    )
    .unwrap();
    let paid = p.0.borrow().forwarded;
    p.0.borrow_mut().fail_work = true;
    assert!(matches!(
        candidates.capture(0, Default::default(), &mut p),
        Err(PlanError::AudioQueryLimit("test cancellation"))
    ));
    assert_eq!(p.0.borrow().forwarded, paid);
    assert_eq!(p.0.borrow().live, 0);
}

fn exact_work<const M: bool>() {
    let doc = document(48_000, &["voice"], vec![("voice", hold(4))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let parent = Parent::Root(&plan);
    let at = AudioSample(1);
    let mut ample = policy::<M>(usize::MAX);
    let capture = admitted(parent, at, Default::default(), &mut ample)
        .unwrap()
        .capture(0, Default::default(), &mut ample)
        .unwrap();
    let c = capture.capture_work();
    let q = capture.query_work();
    drop(capture);
    let mut exact = policy::<M>(usize::MAX);
    let capture = admitted(parent, at, Default::default(), &mut exact)
        .unwrap()
        .capture(
            0,
            AudioQueryLimits {
                maximum_work: c,
                ..Default::default()
            },
            &mut exact,
        )
        .unwrap();
    assert_eq!(capture.capture_work(), c);
    drop(capture);
    assert_eq!(exact.0.borrow().live, 0);
    let mut short = policy::<M>(usize::MAX);
    let query = admitted(parent, at, Default::default(), &mut short).unwrap();
    assert!(matches!(
        query.capture(
            0,
            AudioQueryLimits {
                maximum_work: c - 1,
                ..Default::default()
            },
            &mut short
        ),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    assert_eq!(short.0.borrow().live, 0);

    let mut zero = policy::<M>(usize::MAX);
    let query = admitted(
        parent,
        at,
        AudioQueryLimits {
            maximum_work: q,
            ..Default::default()
        },
        &mut zero,
    )
    .unwrap();
    assert!(matches!(
        query.capture(0, Default::default(), &mut zero),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    assert_eq!(zero.0.borrow().live, 0);

    let mut absent = policy::<M>(usize::MAX);
    let query = admitted(parent, at, Default::default(), &mut absent).unwrap();
    let calls = absent.0.borrow().calls;
    let work = absent.0.borrow().forwarded;
    absent.0.borrow_mut().fail_work = true;
    let none = query.without_selection(&mut absent).unwrap();
    assert!(none.input().is_none());
    assert_eq!(none.capture_work(), 0);
    assert_eq!(none.capture_additional_units(), 0);
    assert_eq!(absent.0.borrow().calls, calls);
    assert_eq!(absent.0.borrow().forwarded, work);
    assert_eq!(absent.0.borrow().live, 0);
}
#[test]
fn exact_capture_work_one_short_internal_zero_and_absence_preserve_both_modes() {
    exact_work::<false>();
    exact_work::<true>();
}
