//! Private backing-Arc destruction and actual Core post-map error regression.
use super::*;
use deadpan_core::ProjectDocument;
use std::{alloc::Layout, cell::RefCell, rc::Rc, sync::Weak};

#[derive(Default)]
struct Counts<'plan> {
    live: usize,
    peak: usize,
    cap: usize,
    calls: usize,
    work: usize,
    epoch: u64,
    issuer_calls: usize,
    flip_at: Option<usize>,
    fail_reservation: Option<usize>,
    weak: Option<Weak<Option<AudioOwnerRetainedInput<'plan>>>>,
    assert_dead: bool,
}
struct Policy<'plan>(Rc<RefCell<Counts<'plan>>>);
struct Lease<'plan> {
    state: Rc<RefCell<Counts<'plan>>>,
    bytes: usize,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if self.bytes == 0 {
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.assert_dead {
            assert!(
                state.weak.as_ref().unwrap().upgrade().is_none(),
                "actual retained Arc must die before every live Plan/Core lease"
            );
        }
        state.live -= self.bytes;
    }
}
impl<'plan> AudioSoundContextAdmission for Policy<'plan> {
    type Error = PlanError;
    type Issuer = (*const RefCell<Counts<'plan>>, u64);
    type Lease = Lease<'plan>;
    fn issuer(&self) -> Self::Issuer {
        let mut state = self.0.borrow_mut();
        state.issuer_calls += 1;
        if state.flip_at == Some(state.issuer_calls) {
            state.epoch += 1;
        }
        (Rc::as_ptr(&self.0), state.epoch)
    }
    fn spend(&mut self, n: usize) -> std::result::Result<(), PlanError> {
        self.0.borrow_mut().work += n;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> std::result::Result<Self::Lease, PlanError> {
        let mut s = self.0.borrow_mut();
        s.calls += 1;
        if s.fail_reservation == Some(s.calls) {
            return Err(PlanError::AudioQueryLimit("probe test scheduled refusal"));
        }
        let live = s.live.checked_add(layout.size()).unwrap();
        if live > s.cap {
            return Err(PlanError::AudioQueryLimit("probe test residency"));
        }
        s.live = live;
        s.peak = s.peak.max(live);
        Ok(Lease {
            state: Rc::clone(&self.0),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, a: &mut Self::Lease, b: &mut Self::Lease) {
        assert!(Rc::ptr_eq(&a.state, &self.0) && Rc::ptr_eq(&b.state, &self.0));
        a.bytes += b.bytes;
        b.bytes = 0;
    }
}
fn policy<'plan>(cap: usize) -> Policy<'plan> {
    Policy(Rc::new(RefCell::new(Counts {
        cap,
        ..Default::default()
    })))
}
fn fixture() -> RenderPlan {
    let frozen: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let f = frozen["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["fixture"] == "nonunity_shift_fractional_point_ceil_source44100")
        .unwrap();
    RenderPlan::compile(&ProjectDocument::from_json(&f["document"].to_string()).unwrap()).unwrap()
}
fn input(plan: &RenderPlan) -> AudioOwnerInput<'_> {
    let candidates = plan
        .audio_owner_input_candidates_at(AudioSample(0), Default::default())
        .unwrap();
    let member = candidates.query().spans()[0]
        .owners()
        .iter()
        .position(|v| v.instance().node.as_str() == "voice")
        .unwrap();
    candidates
        .capture(member, Default::default())
        .unwrap()
        .into_input()
        .unwrap()
}
#[test]
fn guarded_and_plain_final_shells_drop_actual_arc_before_each_lease() {
    let plan = fixture();
    let input = input(&plan);
    let at = AudioSample(0);
    let mut p = policy(usize::MAX);
    let probe = input
        .retained_probe_at_admitted(at, Default::default(), &mut p)
        .unwrap();
    let original = probe
        .into_shared_admitted(Default::default(), &mut p)
        .unwrap();
    assert!(original.leases()[0].is_some() && original.leases()[1].is_some());
    p.0.borrow_mut().weak = Some(Arc::downgrade(&original.retained().unwrap().0));
    p.0.borrow_mut().assert_dead = true;
    let work = p.0.borrow().work;
    drop(original);
    assert_eq!(p.0.borrow().live, 0);
    assert_eq!(p.0.borrow().work, work);

    let plain = input
        .retained_probe_at(at, Default::default())
        .unwrap()
        .into_input()
        .unwrap();
    let layout =
        crate::audio_allocated::admission::arc::<Option<AudioOwnerRetainedInput<'_>>>().unwrap();
    let mut p = policy(layout.size());
    let original = plain
        .into_shared_admitted(Default::default(), &mut p)
        .unwrap();
    assert_eq!(p.0.borrow().calls, 1);
    assert_eq!(p.0.borrow().peak, layout.size());
    assert_eq!(p.0.borrow().live, layout.size());
    p.0.borrow_mut().weak = Some(Arc::downgrade(&original.retained().unwrap().0));
    p.0.borrow_mut().assert_dead = true;
    drop(original);
    assert_eq!(p.0.borrow().live, 0);
}
#[test]
fn actual_core_post_map_rejection_drops_initialized_retained_before_both_charge_layers() {
    let plan = fixture();
    let input = input(&plan);
    let mut p = policy(usize::MAX);
    let probe = input
        .retained_probe_at_admitted(AudioSample(0), Default::default(), &mut p)
        .unwrap();
    let spent = p.0.borrow().work;
    let state = Rc::clone(&p.0);
    let mut local = Budget {
        remaining: 65_536,
        lookup: LookupStats::default(),
    };
    let mut budget = QueryBudget::new(&mut local, &mut p);
    let mut shell = Owned {
        value: None,
        memory: budget.memory(),
    };
    budget
        .reserve(
            &mut shell.memory,
            crate::audio_allocated::admission::arc::<Option<AudioOwnerRetainedInput<'_>>>()
                .unwrap(),
        )
        .unwrap();
    shell.value = Some(Arc::new(None));
    // This test executes the real Core map of the actual probe value with the
    // SAME allocation-free initializer. Only after initialization, the test
    // inserts a weak witness and changes policy identity to exercise post-map.
    let result = probe.value.map(&mut budget, |retained| {
        let shared = initialize_shared(shell.value.take().unwrap(), retained);
        let mut state = state.borrow_mut();
        state.weak = Some(Arc::downgrade(&shared.0));
        state.assert_dead = true;
        state.epoch += 1;
        shared
    });
    assert!(matches!(
        result,
        Err(Error::Policy(PlanError::AudioMetadataIssuerMismatch))
    ));
    drop(shell);
    assert_eq!(p.0.borrow().live, 0);
    assert!(p.0.borrow().work >= spent);
}
#[test]
fn accepted_probe_arc_refusal_releases_inner_graph_without_refunding_work() {
    let plan = fixture();
    let input = input(&plan);
    let mut p = policy(usize::MAX);
    let probe = input
        .retained_probe_at_admitted(AudioSample(0), Default::default(), &mut p)
        .unwrap();
    let spent = p.0.borrow().work;
    let live = p.0.borrow().live;
    let shell =
        crate::audio_allocated::admission::arc::<Option<AudioOwnerRetainedInput<'_>>>().unwrap();
    p.0.borrow_mut().cap = live + shell.size() - 1;
    assert!(matches!(
        probe.into_shared_admitted(Default::default(), &mut p),
        Err(PlanError::AudioQueryLimit("probe test residency"))
    ));
    assert_eq!(p.0.borrow().live, 0);
    assert!(p.0.borrow().work > spent);
}
#[test]
fn legacy_two_lane_transfer_refuses_guarded_core_before_callback() {
    let plan = fixture();
    let input = input(&plan);
    let mut p = policy(usize::MAX);
    let probe = input
        .retained_probe_at_admitted(AudioSample(0), Default::default(), &mut p)
        .unwrap();
    let mut original = probe
        .into_shared_admitted(Default::default(), &mut p)
        .unwrap();
    let before = (p.0.borrow().live, p.0.borrow().calls, p.0.borrow().work);
    let mut called = false;
    assert!(matches!(
        crate::plan::audio_domain::transfer_context_charges(
            p.issuer(),
            Some(&mut original),
            None,
            |_, _| called = true
        ),
        Err(PlanError::InvalidPlan(
            "guarded retained context requires preflight transfer"
        ))
    ));
    assert!(!called);
    assert_eq!(
        (p.0.borrow().live, p.0.borrow().calls, p.0.borrow().work),
        before
    );
    drop(original);
    assert_eq!(p.0.borrow().live, 0);
}

fn policy_counters(p: &Policy<'_>) -> (usize, usize) {
    let state = p.0.borrow();
    (state.calls, state.work)
}
#[test]
fn final_shell_foreign_and_unstable_issuer_refuse_before_work_or_reservation() {
    let plan = fixture();
    let input = input(&plan);
    let mut source = policy(usize::MAX);
    let probe = input
        .retained_probe_at_admitted(AudioSample(0), Default::default(), &mut source)
        .unwrap();
    let source_before = policy_counters(&source);
    let mut foreign = policy(usize::MAX);
    assert!(matches!(
        probe.into_shared_admitted(Default::default(), &mut foreign),
        Err(PlanError::AudioMetadataIssuerMismatch)
    ));
    assert_eq!(policy_counters(&source), source_before);
    assert_eq!(policy_counters(&foreign), (0, 0));
    assert_eq!(source.0.borrow().live, 0);
    assert_eq!(foreign.0.borrow().live, 0);

    let mut p = policy(usize::MAX);
    let probe = input
        .retained_probe_at_admitted(AudioSample(0), Default::default(), &mut p)
        .unwrap();
    let before = policy_counters(&p);
    // First sharing check sees the original identity; QueryBudget::new sees a
    // changed one. The fixed budget/Plan check must refuse BEFORE added work.
    let flip = p.0.borrow().issuer_calls + 2;
    p.0.borrow_mut().flip_at = Some(flip);
    assert!(matches!(
        probe.into_shared_admitted(Default::default(), &mut p),
        Err(PlanError::AudioMetadataIssuerMismatch)
    ));
    assert_eq!(policy_counters(&p), before);
    assert_eq!(p.0.borrow().live, 0);
}
#[test]
fn actual_stored_core_issuer_is_validated_before_any_shell_reservation() {
    let plan = fixture();
    let input = input(&plan);
    let mut source = policy(usize::MAX);
    let mut probe = input
        .retained_probe_at_admitted(AudioSample(0), Default::default(), &mut source)
        .unwrap();
    assert!(probe.value.charges()[1].is_some());
    let mut foreign = policy(usize::MAX);
    // Compose mismatched guard lanes at the PRIVATE accounting seam. The real
    // actual Core value/issuer remain unchanged. Keep the original Plan lease
    // outside the failed call so its actual data cannot outlive its charge.
    let plan_charge = std::mem::replace(
        &mut probe.value._memory,
        crate::plan::audio_owners::admission::Memory::empty(foreign.issuer()),
    );
    let retained_plan_bytes = plan_charge.lease.as_ref().unwrap().bytes;
    let source_before = policy_counters(&source);
    assert!(matches!(
        probe.into_shared_admitted(Default::default(), &mut foreign),
        Err(PlanError::AudioMetadataIssuerMismatch)
    ));
    assert_eq!(
        policy_counters(&foreign),
        (0, 0),
        "actual Core refusal precedes new callbacks"
    );
    assert_eq!(policy_counters(&source), source_before);
    assert_eq!(source.0.borrow().live, retained_plan_bytes);
    drop(plan_charge);
    assert_eq!(source.0.borrow().live, 0);
    assert_eq!(foreign.0.borrow().live, 0);
}
#[test]
fn real_late_walker_policy_failure_observes_populated_charged_editorial_then_cleans_up() {
    use crate::plan::audio_owners::tests::legacy_snapshot_v1::capture_baseline;
    let plan = fixture();
    let input = input(&plan);
    let mut ample = policy(usize::MAX);
    let probe = input
        .retained_probe_at_admitted(AudioSample(0), Default::default(), &mut ample)
        .unwrap();
    let calls = ample.0.borrow().calls;
    drop(probe);
    assert_eq!(ample.0.borrow().live, 0);
    let mut late = None;
    for call in 1..=calls {
        let mut p = policy(usize::MAX);
        p.0.borrow_mut().fail_reservation = Some(call);
        let (result, events) =
            capture_baseline::observe_case("late_editorial_policy", "actual_probe", || {
                input.retained_probe_at_admitted(AudioSample(0), Default::default(), &mut p)
            });
        assert!(matches!(
            result,
            Err(PlanError::AudioQueryLimit("probe test scheduled refusal"))
        ));
        assert_eq!(p.0.borrow().live, 0);
        let work = p.0.borrow().work;
        assert!(work > 0, "failed real work remains charged");
        if let Some(event) = events.iter().find(|v| {
            v["observation"]["kind"] == "actual_retained_probe_policy_refusal"
                && v["observation"]["editorial_charged"] == true
                && !v["observation"]["editorial"]["constraints"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        }) {
            assert!(
                !event["observation"]["editorial"]["editorial"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                event["observation"]["actual_probe_work_spent"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
            late = Some(call);
            break;
        }
    }
    assert!(
        late.is_some(),
        "actual boundary/span allocation fails after the real editorial slot is populated and charged"
    );
}
#[test]
fn actual_original_core_and_sound_share_one_preflight_callback_without_extra_work() {
    let plan = fixture();
    let input = input(&plan);
    let mut p = policy(usize::MAX);
    let probe = input
        .retained_probe_at_admitted(AudioSample(0), Default::default(), &mut p)
        .unwrap();
    let mut original = probe
        .into_shared_admitted(Default::default(), &mut p)
        .unwrap();
    let mut sound = plan
        .audio_sound_instance_context_admitted(input.owner().instance(), Default::default(), &mut p)
        .unwrap()
        .into_shared(&mut p)
        .unwrap();
    assert!(sound.lease().is_some());
    let before = policy_counters(&p);
    let total = p.0.borrow().live;
    {
        let token = crate::plan::audio_domain::preflight_context_transfer(
            Some(&mut original),
            Some(&mut sound),
            &mut p,
        )
        .unwrap();
        let leases = token.leases();
        assert!(
            leases[0].is_some()
                && leases[1].is_some()
                && leases[2].is_none()
                && leases[3].is_some()
        );
        assert_eq!(
            leases.into_iter().flatten().map(|v| v.bytes).sum::<usize>(),
            total
        );
        // Unused token ends here; the actual Core transfer remains retryable.
    }
    assert_eq!(policy_counters(&p), before);
    let token = crate::plan::audio_domain::preflight_context_transfer(
        Some(&mut original),
        Some(&mut sound),
        &mut p,
    )
    .unwrap();
    let mut callbacks = 0;
    token.commit(|plan, core, buffer, sound| {
        assert!(plan.unwrap().bytes > 0 && core.unwrap().bytes > 0);
        assert!(buffer.is_none());
        assert!(sound.unwrap().bytes > 0);
        callbacks += 1;
    });
    assert_eq!(callbacks, 1);
    assert_eq!(policy_counters(&p), before);
    drop(sound);
    p.0.borrow_mut().weak = Some(Arc::downgrade(&original.retained().unwrap().0));
    p.0.borrow_mut().assert_dead = true;
    drop(original);
    assert_eq!(p.0.borrow().live, 0);
    assert_eq!(policy_counters(&p), before);
}
