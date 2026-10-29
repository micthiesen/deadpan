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
        let mut state = self.0.borrow_mut();
        if state.fail_work {
            return Err(PlanError::AudioQueryLimit("test cancellation"));
        }
        state.forwarded += n;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Token, PlanError> {
        let mut state = self.0.borrow_mut();
        state.calls += 1;
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
    let input = candidates
        .capture(member, Default::default(), &mut p)
        .unwrap()
        .into_input()
        .unwrap();
    assert_eq!(
        input.work(),
        total + old_c,
        "legacy capture C remains unchanged/unpaid"
    );
    let mut actual = super::input(&input);
    actual["total_work"] = json!(old_q + old_c);
    assert_eq!(
        actual, case["legacy_input"],
        "full captured private Domain/seed is frozen"
    );
    assert_eq!(
        p.0.borrow().live,
        0,
        "query lease released only after actual capture consumed query"
    );
    assert_eq!(
        p.0.borrow().forwarded,
        if M { total } else { 0 },
        "capture never debits Q again"
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
