//! Compare against the independently frozen OLD query output, never regenerated.
use super::*;
use crate::AudioSoundContextAdmission;
use std::{alloc::Layout, cell::RefCell, rc::Rc};

#[derive(Default)]
struct State {
    live: usize,
    peak: usize,
    cap: usize,
    calls: usize,
    forwarded: usize,
    fail_work: bool,
    fail_after: Option<usize>,
    fail_on_reservation: Option<usize>,
    rotate_at: Option<usize>,
}
struct Policy<const M: bool>(Rc<RefCell<State>>);
struct Token {
    state: Rc<RefCell<State>>,
    bytes: usize,
}
impl Drop for Token {
    fn drop(&mut self) {
        self.state.borrow_mut().live -= self.bytes;
    }
}
impl<const M: bool> AudioSoundContextAdmission for Policy<M> {
    const METERED: bool = M;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Token;
    fn issuer(&self) -> usize {
        Rc::as_ptr(&self.0) as usize
    }
    fn spend(&mut self, n: usize) -> Result<(), PlanError> {
        let rotate = {
            let mut state = self.0.borrow_mut();
            if state.fail_work || state.fail_after.is_some_and(|n| state.forwarded >= n) {
                return Err(PlanError::AudioQueryLimit("test cancellation"));
            }
            state.forwarded += n;
            let rotate = state
                .rotate_at
                .is_some_and(|limit| state.forwarded >= limit);
            if rotate {
                state.rotate_at = None;
            }
            rotate.then_some(state.cap)
        };
        if let Some(cap) = rotate {
            self.0 = Rc::new(RefCell::new(State {
                cap,
                ..Default::default()
            }));
        }
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Token, PlanError> {
        let mut state = self.0.borrow_mut();
        state.calls += 1;
        if state.fail_on_reservation == Some(state.calls) {
            return Err(PlanError::AudioQueryLimit("test reservation refusal"));
        }
        let live = state.live.checked_add(layout.size()).unwrap();
        if live > state.cap {
            return Err(PlanError::AudioQueryLimit("test allocation refusal"));
        }
        state.live = live;
        state.peak = state.peak.max(live);
        Ok(Token {
            state: Rc::clone(&self.0),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, a: &mut Token, b: &mut Token) {
        assert!(Rc::ptr_eq(&a.state, &self.0) && Rc::ptr_eq(&b.state, &self.0));
        a.bytes += b.bytes;
        b.bytes = 0;
    }
}
fn policy<const M: bool>(cap: usize) -> Policy<M> {
    Policy(Rc::new(RefCell::new(State {
        cap,
        ..Default::default()
    })))
}
fn number(value: &Value) -> usize {
    usize::try_from(value.as_u64().unwrap()).unwrap()
}
fn interval(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}
fn admitted<'a, 'plan: 'a, const M: bool>(
    parent: Parent<'a, 'plan>,
    at: AudioSample,
    limits: AudioQueryLimits,
    policy: &mut Policy<M>,
) -> Result<crate::AdmittedAudioOwnerInputCandidates<'a, 'plan, usize, Token>, PlanError> {
    match parent {
        Parent::Root(plan) => plan.audio_owner_input_candidates_at_admitted(at, limits, policy),
        Parent::Domain(domain) => domain.owner_input_candidates_at_admitted(at, limits, policy),
    }
}
fn check_case(parent: Parent<'_, '_>, case: &Value) {
    let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
    let member = number(&case["selected_member_index"]);
    let old_q = number(&case["legacy_q"]);
    let old_c = number(&case["legacy_c"]);
    let wide = parent
        .query(interval(&case["window"]), Default::default())
        .unwrap();
    assert_eq!(
        query(&wide, parent.plan(), |v| v.0),
        case["ordered_window_query"],
        "legacy entire window remains frozen"
    );
    for metered in [false, true] {
        if metered {
            check_mode::<true>(parent, case, at, member, old_q, old_c);
        } else {
            check_mode::<false>(parent, case, at, member, old_q, old_c);
        }
    }
}
fn check_mode<const M: bool>(
    parent: Parent<'_, '_>,
    case: &Value,
    at: AudioSample,
    member: usize,
    old_q: usize,
    old_c: usize,
) {
    let mut p = policy::<M>(usize::MAX);
    let candidates = admitted(parent, at, Default::default(), &mut p).unwrap();
    assert_eq!(candidates.canonical_units(), old_q);
    let added = candidates.additional_units();
    let total = candidates.work();
    assert_eq!(total, old_q + added);
    assert_eq!(candidates.was_forwarded(), M);
    assert_eq!(p.0.borrow().forwarded, if M { total } else { 0 });
    assert_eq!(M, added > 0);
    let mut actual = query(candidates.query(), parent.plan(), |v| v.0);
    actual["work"] = json!(old_q);
    assert_eq!(
        actual, case["observed_query"],
        "all ordered owner/provenance/grid fields compare to OLD oracle"
    );
    let peak = p.0.borrow().peak;
    if M {
        assert!(p.0.borrow().live > 0);
        assert!(added >= p.0.borrow().calls);
    }
    let capture = candidates
        .capture(member, Default::default(), &mut p)
        .unwrap();
    let capture_added = capture.capture_additional_units();
    assert_eq!(capture.capture_canonical_units(), old_c);
    assert_eq!(capture.query_canonical_units(), old_q);
    assert_eq!(capture.query_additional_units(), added);
    assert_eq!(capture.was_forwarded(), M);
    let input = capture.input().unwrap();
    assert_eq!(input.work(), total + old_c + capture_added);
    let mut actual = super::input(input);
    actual["total_work"] = json!(old_q + old_c);
    assert_eq!(
        actual, case["legacy_input"],
        "full captured private Domain/seed is frozen"
    );
    if M {
        assert!(
            p.0.borrow().live > 0,
            "actual captured source Input stays charged"
        );
    }
    assert_eq!(
        p.0.borrow().forwarded,
        if M { total + old_c + capture_added } else { 0 },
        "Q and C each forward exactly once"
    );
    drop(capture);
    assert_eq!(
        p.0.borrow().live,
        0,
        "captured Input releases with its charge"
    );
    if !M {
        assert_eq!(p.0.borrow().calls, 0);
        return;
    }
    let mut exact = policy::<true>(peak);
    let query = admitted(parent, at, Default::default(), &mut exact).unwrap();
    assert_eq!(exact.0.borrow().peak, peak);
    drop(query);
    assert_eq!(exact.0.borrow().live, 0);
    let mut short = policy::<true>(peak - 1);
    assert!(matches!(
        admitted(parent, at, Default::default(), &mut short),
        Err(PlanError::AudioQueryLimit("test allocation refusal"))
    ));
    assert_eq!(
        short.0.borrow().live,
        0,
        "every partial query/bound/domain/recursive scope releases"
    );
}

fn kind_named(value: &str) -> AudioOwnerKind {
    match value {
        "Node" => AudioOwnerKind::Node,
        "DefaultGap" => AudioOwnerKind::DefaultGap,
        _ => panic!("unexpected frozen owner kind {value}"),
    }
}

fn retained_from_case<'plan>(
    plan: &'plan RenderPlan,
    fixture: &Value,
    case: &Value,
) -> crate::AudioOwnerRetainedInput<'plan> {
    let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
    let alias = case["selected_alias"].as_str().unwrap();
    let kind = kind_named(case["selected_kind"].as_str().unwrap());
    let input = if case["parent_kind"] == "Root" {
        let observed = plan
            .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
            .unwrap();
        let owner = observed.spans()[0]
            .owners()
            .iter()
            .find(|owner| owner.instance.node.as_str() == alias && owner.kind == kind)
            .unwrap();
        plan.audio_owner_input(owner, at, Default::default())
            .unwrap()
    } else {
        let selected_index = number(&fixture["witnesses"]["selected_verified_old_member_index"]);
        let outer = plan
            .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
            .unwrap();
        let retained_owner = &outer.spans()[0].owners()[selected_index];
        let retained_context = plan
            .audio_owner_input(retained_owner, at, Default::default())
            .unwrap();
        let domain = retained_context.context();
        let observed = domain
            .gain_owners(at..AudioSample(at.0 + 1), Default::default())
            .unwrap();
        let owner = observed.spans()[0]
            .owners()
            .iter()
            .find(|owner| owner.instance.node.as_str() == alias && owner.kind == kind)
            .unwrap();
        domain.owner_input(owner, at, Default::default()).unwrap()
    };
    input
        .retained_at(at, Default::default())
        .unwrap()
        .expect("frozen actual Some retained probe")
}

fn retained_from_root_member<'plan>(
    plan: &'plan RenderPlan,
    at: AudioSample,
    member_index: usize,
) -> crate::AudioOwnerRetainedInput<'plan> {
    let observed = plan
        .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
        .unwrap();
    let owner = &observed.spans()[0].owners()[member_index];
    plan.audio_owner_input(owner, at, Default::default())
        .unwrap()
        .retained_at(at, Default::default())
        .unwrap()
        .expect("frozen actual Some retained probe")
}

fn assert_retained_query_fields(
    admitted: &crate::AdmittedAudioOwnerQuery<'_, usize, Token>,
    expected: &Value,
    plan: &RenderPlan,
) {
    assert_eq!(admitted.canonical_units(), number(&expected["work"]));
    assert_eq!(
        admitted
            .canonical_units()
            .checked_add(admitted.additional_units()),
        Some(admitted.work())
    );
    // Only the expected total-work view changes. The immutable old canonical
    // amount and every mapped owner, origin, span and lookup remain checked.
    let mut expected = expected.clone();
    expected["work"] = json!(admitted.work());
    assert_eq!(
        crate::plan::audio_owners::tests::legacy_snapshot_v1::query(
            admitted.query(),
            plan,
            |sample| sample.0,
        ),
        expected,
        "retained owner clocks, exact maps and rewritten origins stay frozen"
    );
}

fn check_retained_mode<const M: bool>(
    retained: &crate::AudioOwnerRetainedInput<'_>,
    expected: &Value,
    plan: &RenderPlan,
) {
    let range = interval(&expected["window"]);
    let mut ample = policy::<M>(usize::MAX);
    let admitted = retained
        .owners_admitted(range.clone(), Default::default(), &mut ample)
        .unwrap();
    assert_retained_query_fields(&admitted, expected, plan);
    assert_eq!(admitted.was_forwarded(), M);
    let peak = ample.0.borrow().peak;
    assert_eq!(
        ample.0.borrow().forwarded,
        if M { admitted.work() } else { 0 }
    );
    if M {
        assert!(ample.0.borrow().live > 0);
    } else {
        assert_eq!(ample.0.borrow().calls, 0);
        assert_eq!(admitted.additional_units(), 0);
    }
    drop(admitted);
    assert_eq!(ample.0.borrow().live, 0);

    if M {
        let mut exact = policy::<true>(peak);
        let result = retained
            .owners_admitted(range.clone(), Default::default(), &mut exact)
            .unwrap();
        assert_eq!(exact.0.borrow().peak, peak);
        drop(result);
        assert_eq!(exact.0.borrow().live, 0);
        for cap in 0..peak {
            let mut short = policy::<true>(cap);
            assert!(matches!(
                retained.owners_admitted(range.clone(), Default::default(), &mut short),
                Err(PlanError::AudioQueryLimit("test allocation refusal"))
            ));
            assert_eq!(short.0.borrow().live, 0, "reservation cap {cap}");
        }
    }
}

fn check_retained_snapshot_case(plan: &RenderPlan, fixture: &Value, case: &Value) {
    let retained = retained_from_case(plan, fixture, case);
    let expected = &case["retained_probe"]["input"]["observed_owners"];
    let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
    let expected_end = AudioSample(case["observation_window"]["end"].as_i64().unwrap());
    let legacy = retained
        .owners(at..expected_end, Default::default())
        .unwrap();
    assert_eq!(
        crate::plan::audio_owners::tests::legacy_snapshot_v1::query(&legacy, plan, |sample| sample
            .0,),
        *expected,
        "legacy wrapper matches immutable retained_probe.input.observed_owners"
    );
    drop(legacy);
    check_retained_mode::<false>(&retained, expected, plan);
    check_retained_mode::<true>(&retained, expected, plan);
}

#[test]
fn retained_owner_queries_match_every_frozen_actual_some_probe_in_both_modes() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixtures = snapshot["fixtures"].as_array().unwrap();
    let mut count = 0;
    for fixture_index in [2usize, 3, 4] {
        let fixture = &fixtures[fixture_index];
        let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let plan = RenderPlan::compile(&doc).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            if case["retained_probe"]["result"] == "Some" {
                check_retained_snapshot_case(&plan, fixture, case);
                count += 1;
            }
        }
        if fixture_index == 4 {
            for row in fixture["witnesses"]["actual_old_owner_rows"]
                .as_array()
                .unwrap()
            {
                let at = AudioSample(3);
                let member = number(&row["member_index"]);
                let retained = retained_from_root_member(&plan, at, member);
                let expected = &row["actual_retained_probe"]["input"]["observed_owners"];
                let legacy = retained
                    .owners(at..AudioSample(4), Default::default())
                    .unwrap();
                assert_eq!(
                    crate::plan::audio_owners::tests::legacy_snapshot_v1::query(
                        &legacy,
                        &plan,
                        |sample| sample.0,
                    ),
                    *expected,
                    "all independently captured split rows remain literal oracles"
                );
                drop(legacy);
                check_retained_mode::<false>(&retained, expected, &plan);
                check_retained_mode::<true>(&retained, expected, &plan);
                count += 1;
            }
        }
    }
    assert_eq!(count, 7);
}

#[test]
fn retained_envelope_exact_work_preserves_second_zero_remainder_validation() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = &snapshot["fixtures"][3];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let case = &fixture["cases"][0];
    let retained = retained_from_case(&plan, fixture, case);

    let mut local = Budget {
        remaining: AudioQueryLimits::default().maximum_work,
        lookup: LookupStats::default(),
    };
    let mut measure = policy::<true>(usize::MAX);
    let mut budget = admission::Budget::new(&mut local, &mut measure);
    let envelope = retained
        .bound()
        .envelope_domain_query(&mut budget)
        .unwrap()
        .unwrap();
    let envelope_work = budget.canonical_used + budget.additional_used;
    drop(envelope);
    assert_eq!(measure.0.borrow().live, 0);

    let mut exact = policy::<true>(usize::MAX);
    assert!(matches!(
        retained.owners_admitted(
            AudioSample(0)..AudioSample(1),
            AudioQueryLimits {
                maximum_work: envelope_work,
                ..Default::default()
            },
            &mut exact,
        ),
        Err(PlanError::InvalidAudioLimits)
    ));
    assert_eq!(exact.0.borrow().forwarded, envelope_work);
    assert_eq!(exact.0.borrow().live, 0);
}

#[test]
fn retained_query_cancellation_keeps_accepted_work_and_releases_partial_leases() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = &snapshot["fixtures"][3];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let retained = retained_from_case(&plan, fixture, &fixture["cases"][0]);
    let mut cancelled = policy::<true>(usize::MAX);
    cancelled.0.borrow_mut().fail_after = Some(1);
    assert!(matches!(
        retained.owners_admitted(
            AudioSample(0)..AudioSample(1),
            Default::default(),
            &mut cancelled
        ),
        Err(PlanError::AudioQueryLimit("test cancellation"))
    ));
    assert!(cancelled.0.borrow().forwarded > 0);
    assert_eq!(cancelled.0.borrow().live, 0);
}

#[test]
fn retained_query_refuses_each_actual_reservation_index_and_releases_prior_leases() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = &snapshot["fixtures"][3];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let retained = retained_from_case(&plan, fixture, &fixture["cases"][0]);
    let range = interval(&fixture["cases"][0]["observation_window"]);

    let mut baseline = policy::<true>(usize::MAX);
    let complete = retained
        .owners_admitted(range.clone(), Default::default(), &mut baseline)
        .unwrap();
    let reservation_count = baseline.0.borrow().calls;
    assert!(reservation_count > 0);
    drop(complete);
    assert_eq!(baseline.0.borrow().live, 0);

    for index in 1..=reservation_count {
        let mut refusing = policy::<true>(usize::MAX);
        refusing.0.borrow_mut().fail_on_reservation = Some(index);
        assert!(
            matches!(
                retained.owners_admitted(range.clone(), Default::default(), &mut refusing),
                Err(PlanError::AudioQueryLimit("test reservation refusal"))
            ),
            "reservation index {index} of {reservation_count}"
        );
        assert_eq!(refusing.0.borrow().calls, index);
        assert_eq!(refusing.0.borrow().live, 0, "reservation index {index}");
    }
}

#[test]
fn retained_query_validates_limits_and_range_before_admission() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = &snapshot["fixtures"][3];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let retained = retained_from_case(&plan, fixture, &fixture["cases"][0]);
    let mut admission = policy::<true>(usize::MAX);
    assert!(matches!(
        retained.owners_admitted(
            AudioSample(1)..AudioSample(0),
            AudioQueryLimits {
                maximum_spans: 0,
                ..Default::default()
            },
            &mut admission,
        ),
        Err(PlanError::InvalidAudioLimits)
    ));
    assert!(matches!(
        retained.owners_admitted(
            AudioSample(1)..AudioSample(0),
            Default::default(),
            &mut admission,
        ),
        Err(PlanError::AudioRangeOutOfRange)
    ));
    assert_eq!(admission.0.borrow().forwarded, 0);
    assert_eq!(admission.0.borrow().calls, 0);
}

#[test]
fn retained_multi_span_query_keeps_legacy_fields_and_shared_accounting() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = &snapshot["fixtures"][2];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let retained = retained_from_case(&plan, fixture, &fixture["cases"][0]);
    let range = AudioSample(1)..AudioSample(9);
    let legacy = retained.owners(range.clone(), Default::default()).unwrap();
    let expected =
        crate::plan::audio_owners::tests::legacy_snapshot_v1::query(&legacy, &plan, |sample| {
            sample.0
        });
    assert!(legacy.spans().len() > 1);
    drop(legacy);
    let mut admission = policy::<true>(usize::MAX);
    let admitted = retained
        .owners_admitted(range, Default::default(), &mut admission)
        .unwrap();
    assert_retained_query_fields(&admitted, &expected, &plan);
    assert_eq!(
        admitted.canonical_units() + admitted.additional_units(),
        admitted.work()
    );
    assert_eq!(admission.0.borrow().forwarded, admitted.work());
}

#[test]
fn retained_query_preserves_inactive_spans_before_the_retained_allocation() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = &snapshot["fixtures"][3];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let retained = retained_from_case(&plan, fixture, &fixture["cases"][0]);
    let range = AudioSample(-10)..AudioSample(-9);
    let legacy = retained.owners(range.clone(), Default::default()).unwrap();
    assert_eq!(legacy.spans().len(), 1);
    assert_eq!(legacy.spans()[0].support(), AudioOwnerSupport::Inactive);
    assert!(legacy.spans()[0].owners().is_empty());
    let expected =
        crate::plan::audio_owners::tests::legacy_snapshot_v1::query(&legacy, &plan, |sample| {
            sample.0
        });
    drop(legacy);

    let mut admission = policy::<true>(usize::MAX);
    let admitted = retained
        .owners_admitted(range, Default::default(), &mut admission)
        .unwrap();
    assert_retained_query_fields(&admitted, &expected, &plan);
    assert_eq!(admission.0.borrow().forwarded, admitted.work());
}

#[test]
fn retained_query_detects_actual_mid_envelope_issuer_change_and_releases_leases() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = &snapshot["fixtures"][3];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let retained = retained_from_case(&plan, fixture, &fixture["cases"][0]);

    let mut local = Budget {
        remaining: AudioQueryLimits::default().maximum_work,
        lookup: LookupStats::default(),
    };
    let mut measure = policy::<true>(usize::MAX);
    let mut budget = admission::Budget::new(&mut local, &mut measure);
    let envelope = retained
        .bound()
        .envelope_domain_query(&mut budget)
        .unwrap()
        .unwrap();
    let envelope_work = budget.canonical_used + budget.additional_used;
    drop(envelope);
    assert_eq!(measure.0.borrow().live, 0);

    let mut changing = policy::<true>(usize::MAX);
    let old_issuer = Rc::clone(&changing.0);
    changing.0.borrow_mut().rotate_at = Some(envelope_work);
    assert!(matches!(
        retained.owners_admitted(
            AudioSample(0)..AudioSample(1),
            Default::default(),
            &mut changing
        ),
        Err(PlanError::AudioMetadataIssuerMismatch)
    ));
    assert_eq!(old_issuer.borrow().live, 0);
    assert_eq!(changing.0.borrow().live, 0);
}

#[test]
fn independent_old_snapshot_covers_all_fourteen_actual_query_capture_cases() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(expected["schema"], "SnapshotV1");
    let mut count = 0;
    for fixture in expected["fixtures"].as_array().unwrap() {
        let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let plan = RenderPlan::compile(&doc).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            if case["parent_kind"] == "Root" {
                check_case(Parent::Root(&plan), case);
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
                    panic!("actual frozen hidden reference")
                };
                let seeded = reference
                    .gain_owners(AudioSample(3)..AudioSample(4), Default::default())
                    .unwrap();
                let wrong = plan
                    .audio_gain_owners(AudioSample(3)..AudioSample(4), Default::default())
                    .unwrap();
                assert_ne!(
                    query(&seeded, reference.plan, |v| v.0),
                    query(&wrong, &plan, |v| v.0),
                    "root substitution MUST fail"
                );
                check_case(Parent::Domain(reference), case);
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
                check_case(Parent::Domain(captured.context()), case);
            }
            count += 1;
        }
    }
    assert_eq!(count, 14);
}
#[test]
fn issuer_refusal_precedes_capture_semantics_and_absence_needs_no_new_limits() {
    let doc = document(48_000, &["voice"], vec![("voice", hold(4))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let parent = Parent::Root(&plan);
    let at = AudioSample(1);
    let mut a = policy::<true>(usize::MAX);
    let mut b = policy::<true>(usize::MAX);
    let query = admitted(parent, at, Default::default(), &mut a).unwrap();
    assert!(matches!(
        query.capture(
            usize::MAX,
            AudioQueryLimits {
                maximum_work: 0,
                ..Default::default()
            },
            &mut b
        ),
        Err(PlanError::AudioMetadataIssuerMismatch)
    ));
    assert_eq!(a.0.borrow().live, 0);
    assert_eq!(b.0.borrow().calls, 0);
    assert_eq!(b.0.borrow().forwarded, 0);
    let query = admitted(parent, at, Default::default(), &mut a).unwrap();
    let q = query.work();
    let receipt = query.without_selection(&mut a).unwrap();
    assert_eq!(receipt.work(), q);
    assert_eq!(receipt.capture_work(), 0);
    assert!(receipt.input().is_none());
    assert_eq!(a.0.borrow().live, 0);
}
#[test]
fn seeded_invalid_limits_keep_semantic_order_but_earlier_admission_can_refuse() {
    // The seed must own a real Repeat path. A standalone Hold's seed
    // has no owning metadata allocation before the inherited validation.
    let doc = document(
        48_000,
        &["repeat"],
        vec![("voice", hold(4)), ("repeat", repeat("voice", 2, 1))],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let observed = plan
        .audio_gain_owners(AudioSample(1)..AudioSample(2), Default::default())
        .unwrap();
    let input = plan
        .audio_owner_input(
            observed.spans()[0].owners().last().unwrap(),
            AudioSample(1),
            Default::default(),
        )
        .unwrap();
    let domain = input.context();
    assert!(
        !domain.seed.repeats.is_empty(),
        "checked Repeat-child capture provides a genuinely owning seed path"
    );
    let invalid = AudioQueryLimits {
        maximum_spans: 0,
        ..Default::default()
    };
    assert!(matches!(
        domain.gain_owners(AudioSample(1)..AudioSample(2), invalid),
        Err(PlanError::InvalidAudioLimits)
    ));
    let mut ample = policy::<true>(usize::MAX);
    assert!(matches!(
        domain.owner_input_candidates_at_admitted(AudioSample(1), invalid, &mut ample),
        Err(PlanError::InvalidAudioLimits)
    ));
    assert!(ample.0.borrow().calls > 0);
    assert_eq!(ample.0.borrow().live, 0);
    let mut short = policy::<true>(0);
    assert!(matches!(
        domain.owner_input_candidates_at_admitted(AudioSample(1), invalid, &mut short),
        Err(PlanError::AudioQueryLimit("test allocation refusal"))
    ));
    assert_eq!(short.0.borrow().live, 0);
    let mut cancelled = policy::<true>(usize::MAX);
    cancelled.0.borrow_mut().fail_work = true;
    assert!(matches!(
        domain.owner_input_candidates_at_admitted(
            AudioSample(1),
            Default::default(),
            &mut cancelled
        ),
        Err(PlanError::AudioQueryLimit("test cancellation"))
    ));
    assert_eq!(cancelled.0.borrow().live, 0);
}

#[path = "owner_capture_admission_tests.rs"]
mod capture;

#[test]
fn synthetic_empty_support_matches_legacy_branch_for_empty_and_nonempty_requests() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = &snapshot["fixtures"][3];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let actual = retained_from_case(&plan, fixture, &fixture["cases"][0]);
    // Synthetic private-state branch coverage only. The real captured binding
    // remains intact except its meaningful support is replaced with 0..0.
    let retained = actual.synthetic_empty_support_for_test();
    assert!(matches!(retained.domain(), crate::AudioBoundDomain::Empty));

    for range in [
        AudioSample(0)..AudioSample(0),
        AudioSample(0)..AudioSample(1),
    ] {
        let legacy = retained.owners(range.clone(), Default::default()).unwrap();
        assert!(legacy.work() > 0);
        if range.is_empty() {
            assert!(legacy.spans().is_empty());
        } else {
            assert_eq!(legacy.spans().len(), 1);
            assert_eq!(legacy.spans()[0].support(), AudioOwnerSupport::Inactive);
            assert!(legacy.spans()[0].owners().is_empty());
        }
        let expected =
            crate::plan::audio_owners::tests::legacy_snapshot_v1::query(&legacy, &plan, |sample| {
                sample.0
            });
        let old_work = legacy.work();
        drop(legacy);

        for metered in [false, true] {
            if metered {
                let mut policy = policy::<true>(usize::MAX);
                let admitted = retained
                    .owners_admitted(range.clone(), Default::default(), &mut policy)
                    .unwrap();
                assert_retained_query_fields(&admitted, &expected, &plan);
                assert!(admitted.was_forwarded());
                assert_eq!(policy.0.borrow().forwarded, admitted.work());
                let peak = policy.0.borrow().peak;
                if range.is_empty() {
                    assert_eq!(policy.0.borrow().calls, 0);
                    assert_eq!(peak, 0);
                } else {
                    assert!(policy.0.borrow().calls > 0);
                    for cap in 0..peak {
                        let mut refusing = self::policy::<true>(cap);
                        assert!(matches!(
                            retained.owners_admitted(
                                range.clone(),
                                Default::default(),
                                &mut refusing,
                            ),
                            Err(PlanError::AudioQueryLimit("test allocation refusal"))
                        ));
                        assert_eq!(refusing.0.borrow().live, 0);
                    }
                }
                drop(admitted);
                assert_eq!(policy.0.borrow().live, 0);
            } else {
                let mut policy = policy::<false>(usize::MAX);
                let admitted = retained
                    .owners_admitted(range.clone(), Default::default(), &mut policy)
                    .unwrap();
                assert_eq!(admitted.canonical_units(), old_work);
                assert_eq!(admitted.additional_units(), 0);
                assert_eq!(admitted.work(), old_work);
                assert!(!admitted.was_forwarded());
                assert_eq!(
                    crate::plan::audio_owners::tests::legacy_snapshot_v1::query(
                        admitted.query(),
                        &plan,
                        |sample| sample.0,
                    ),
                    expected
                );
                assert_eq!(policy.0.borrow().forwarded, 0);
                assert_eq!(policy.0.borrow().calls, 0);
            }
        }
    }
}
