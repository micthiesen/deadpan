use super::*;
use crate::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
struct State {
    live: usize,
    peak: usize,
    cap: usize,
    work: usize,
    work_cap: usize,
    issuer: usize,
    reserves: usize,
    fail_reserve: Option<usize>,
    watch: Option<Rc<Cell<bool>>>,
}
#[derive(Clone)]
struct Policy(Rc<RefCell<State>>);
#[derive(Debug, PartialEq)]
enum Failure {
    Memory,
    Work,
    Issuer,
    Injected,
}
struct Lease {
    state: Rc<RefCell<State>>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.bytes != 0 {
            let mut state = self.state.borrow_mut();
            if let Some(watch) = &state.watch {
                assert!(watch.get(), "value must drop before lease");
            }
            state.live -= self.bytes;
        }
    }
}
impl AudioBindingAdmission for Policy {
    type Error = Failure;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.0.borrow().issuer
    }
    fn issuer_mismatch(&self) -> Failure {
        Failure::Issuer
    }
    fn spend(&mut self, n: usize) -> Result<(), Failure> {
        let mut s = self.0.borrow_mut();
        let total = s.work.checked_add(n).ok_or(Failure::Work)?;
        if total > s.work_cap {
            return Err(Failure::Work);
        }
        s.work = total;
        Ok(())
    }
    fn reserve(&mut self, l: Layout) -> Result<Lease, Failure> {
        let mut s = self.0.borrow_mut();
        s.reserves += 1;
        if s.fail_reserve == Some(s.reserves) {
            return Err(Failure::Injected);
        }
        let total = s.live.checked_add(l.size()).ok_or(Failure::Memory)?;
        if total > s.cap {
            return Err(Failure::Memory);
        }
        s.live = total;
        s.peak = s.peak.max(total);
        Ok(Lease {
            state: Rc::clone(&self.0),
            bytes: l.size(),
        })
    }
    fn merge(&mut self, a: &mut Lease, b: &mut Lease) {
        assert!(Rc::ptr_eq(&a.state, &b.state));
        a.bytes += b.bytes;
        b.bytes = 0;
    }
}
fn policy(cap: usize) -> Policy {
    Policy(Rc::new(RefCell::new(State {
        cap,
        work_cap: usize::MAX,
        issuer: 1,
        ..State::default()
    })))
}
fn id(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn revision(s: &str) -> RevisionId {
    RevisionId::new(s).unwrap()
}
fn frames(v: i64) -> FrameDuration {
    FrameDuration::new(v).unwrap()
}
fn fixture(terms: usize) -> (AudioBindingState, InstancePath) {
    let mut document = ProjectDocument::new(
        ProjectId::new("binding-admission").unwrap(),
        revision("current"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let body = BeatNode::hold(
        "Body",
        HoldRecipe {
            duration: frames(4),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    );
    let mut repeated = BeatNode::sequence("Repeat", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: id("body"),
        iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
        gap: None,
        escalation: None,
    };
    document.nodes = BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("repeat")])),
        (id("repeat"), repeated),
        (id("body"), body),
    ])
    .into();
    document.validate().unwrap();
    let mut state = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: revision("captured"),
            ordinal: 0,
        },
    )
    .unwrap();
    if terms != 0 {
        let binding = state.bindings.get_mut(&id("body")).unwrap();
        binding.resume = Some(AudioResume {
            local_boundary: ExactRatio::ZERO,
            phase: AudioLocalPhase {
                constant: ExactRatio::ZERO,
                terms: (0..terms)
                    .map(|_| AudioPhaseTerm {
                        placement: binding.lattice.clone(),
                        from_local: ExactRatio::ZERO,
                        to_local: ExactRatio::ONE,
                    })
                    .collect(),
            },
        });
        binding.reanchors = (0..terms)
            .map(|_| AudioReanchorStep::for_allocation(binding.lattice.clone(), None))
            .collect();
    }
    (
        state,
        InstancePath {
            node: id("body"),
            repeats: vec![RepeatInstance {
                node: id("repeat"),
                iteration: IterationId {
                    allocation: revision("plays"),
                    ordinal: 1,
                },
            }],
        },
    )
}
fn admitted(
    state: &AudioBindingState,
    path: &InstancePath,
    p: &mut Policy,
) -> BuildResult<AdmittedAudioBinding<usize, Lease>, Failure> {
    state.resolve_in_admitted(
        &path.node,
        AudioBindingEnvironment::Occurrence(path),
        100_000,
        p,
    )
}
#[test]
fn semantic_error_order_and_earlier_policy_refusal_both_hold() {
    let (state, mut path) = fixture(0);
    path.repeats.push(path.repeats[0].clone());
    let old = state
        .resolve_in(
            &path.node,
            AudioBindingEnvironment::Occurrence(&path),
            100_000,
        )
        .unwrap_err();
    let mut enough = policy(usize::MAX);
    let error = admitted(&state, &path, &mut enough).err().unwrap();
    let AudioBindingBuildError::Document(error) = error else {
        panic!("expected original semantic failure")
    };
    assert_eq!(error, old);
    assert_eq!(enough.0.borrow().live, 0);
    let mut refused = policy(0);
    assert!(matches!(
        admitted(&state, &path, &mut refused),
        Err(AudioBindingBuildError::Policy(Failure::Memory))
    ));
    assert_eq!(refused.0.borrow().live, 0);
}

struct NoAdmission(Policy);
impl AudioBindingAdmission for NoAdmission {
    const METERED: bool = false;
    type Error = Failure;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.0.issuer()
    }
    fn issuer_mismatch(&self) -> Failure {
        Failure::Issuer
    }
    fn spend(&mut self, _: usize) -> Result<(), Failure> {
        panic!("unmetered host spend")
    }
    fn reserve(&mut self, _: Layout) -> Result<Lease, Failure> {
        panic!("unmetered host reserve")
    }
    fn merge(&mut self, _: &mut Lease, _: &mut Lease) {
        panic!("unmetered host merge")
    }
}

// Exercise the actual Build/Growing guards with both independent nonzero
// charge fields. This is a guard-ownership fixture, not audio/source authority.
fn value_with_both_charges(p: &mut Policy) -> AudioBindingValue<Vec<NodeId>, usize, Lease> {
    let mut raw = Work::new(1_000).unwrap();
    let mut additional = 0;
    let mut build = Build::new(&mut raw, &mut additional, p);
    let node = build.node(&id("transfer-payload")).unwrap();
    let mut growing = Growing::new();
    growing.push(node, &mut build).unwrap();
    let (values, buffer) = growing.finish(&mut build).unwrap();
    let result = build.finish_buffer(values, buffer);
    assert!(result.charges().0.is_some_and(|v| v.bytes > 0));
    assert!(result.charges().1.is_some_and(|v| v.bytes > 0));
    result
}

#[test]
fn charge_transfer_borrows_both_actual_lanes_without_policy_work() {
    let mut p = policy(usize::MAX);
    let mut value = value_with_both_charges(&mut p);
    let before = (p.0.borrow().work, p.0.borrow().reserves);
    let addresses = {
        let (lease, buffer) = value.charges();
        (
            std::ptr::from_ref(lease.unwrap()),
            std::ptr::from_ref(buffer.unwrap()),
        )
    };
    {
        let unused = value.preflight_charge_transfer(&p).unwrap();
        let (lease, buffer) = unused.charges();
        assert_eq!(std::ptr::from_ref(lease.unwrap()), addresses.0);
        assert_eq!(std::ptr::from_ref(buffer.unwrap()), addresses.1);
        // Unused token ends its borrow; it must not consume transfer state.
    }
    assert_eq!(value.value().as_slice(), &[id("transfer-payload")]);
    let callbacks = Cell::new(0);
    value
        .preflight_charge_transfer(&p)
        .unwrap()
        .commit(|lease, buffer| {
            assert_eq!(std::ptr::from_mut(lease.unwrap()).cast_const(), addresses.0);
            assert_eq!(
                std::ptr::from_mut(buffer.unwrap()).cast_const(),
                addresses.1
            );
            callbacks.set(callbacks.get() + 1);
        });
    assert_eq!(callbacks.get(), 1);
    assert_eq!((p.0.borrow().work, p.0.borrow().reserves), before);
    let Err(AudioBindingBuildError::Document(error)) = value.preflight_charge_transfer(&p) else {
        panic!("committed transfer must be refused");
    };
    assert_eq!(
        error,
        super::invalid("binding charge transfer already committed")
    );
    // Charge publication neither consumes nor resets the existing work receipt.
    let work = value.take_work(&p).unwrap();
    assert_eq!(work.units(), before.0);
    assert!(value.take_work(&p).is_err());
    drop(value);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn charge_transfer_wrong_issuer_or_mode_leaves_valid_retry_possible() {
    let mut p = policy(usize::MAX);
    let mut value = value_with_both_charges(&mut p);
    let wrong = policy(usize::MAX);
    wrong.0.borrow_mut().issuer = 2;
    assert!(matches!(
        value.preflight_charge_transfer(&wrong),
        Err(AudioBindingBuildError::Policy(Failure::Issuer))
    ));
    assert!(matches!(
        value.preflight_charge_transfer(&NoAdmission(p.clone())),
        Err(AudioBindingBuildError::Policy(Failure::Issuer))
    ));
    let before = (p.0.borrow().live, p.0.borrow().work, p.0.borrow().reserves);
    value
        .preflight_charge_transfer(&p)
        .unwrap()
        .commit(|lease, buffer| {
            assert!(lease.is_some() && buffer.is_some());
        });
    assert_eq!(
        (p.0.borrow().live, p.0.borrow().work, p.0.borrow().reserves),
        before
    );
    assert_eq!(wrong.0.borrow().work, 0);
    assert_eq!(wrong.0.borrow().reserves, 0);
    drop(value);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn charge_transfer_token_outlives_only_the_temporary_policy_borrow() {
    let mut p = policy(usize::MAX);
    let mut value = value_with_both_charges(&mut p);
    let token = {
        let temporary_policy = p.clone();
        value.preflight_charge_transfer(&temporary_policy).unwrap()
    };
    // This must compile while the value token is live. Neither the short
    // temporary policy nor a shared borrow of p is retained by the token.
    let _: &mut Policy = &mut p;
    let before = (p.0.borrow().work, p.0.borrow().reserves);
    token.commit(|lease, buffer| {
        assert!(lease.is_some() && buffer.is_some());
    });
    assert_eq!((p.0.borrow().work, p.0.borrow().reserves), before);
    drop(value);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn charge_transfer_map_preserves_unused_and_committed_state() {
    let mut p = policy(usize::MAX);
    let value = value_with_both_charges(&mut p);
    let mut value = value.map_value(&p, |nodes| (nodes, 17)).unwrap();
    {
        let unused = value.preflight_charge_transfer(&p).unwrap();
        assert!(unused.charges().0.is_some());
    }
    let mut value = value.map_value(&p, |(nodes, _)| nodes).unwrap();
    value
        .preflight_charge_transfer(&p)
        .unwrap()
        .commit(|_, _| {});
    let before = (p.0.borrow().work, p.0.borrow().reserves);
    let mut value = value.map_value(&p, |nodes| (nodes, 23)).unwrap();
    assert!(matches!(
        value.preflight_charge_transfer(&p),
        Err(AudioBindingBuildError::Document(_))
    ));
    assert_eq!(value.value().1, 23);
    assert_eq!((p.0.borrow().work, p.0.borrow().reserves), before);
    assert!(value.charges().0.is_some() && value.charges().1.is_some());
    drop(value);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn charge_transfer_keeps_value_before_both_independent_charge_drops() {
    struct Payload {
        _nodes: Vec<NodeId>,
        dropped: Rc<Cell<bool>>,
    }
    impl Drop for Payload {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }
    let mut p = policy(usize::MAX);
    let value = value_with_both_charges(&mut p);
    let dropped = Rc::new(Cell::new(false));
    let mut value = value
        .map_value(&p, |nodes| Payload {
            _nodes: nodes,
            dropped: Rc::clone(&dropped),
        })
        .unwrap();
    p.0.borrow_mut().watch = Some(Rc::clone(&dropped));
    value
        .preflight_charge_transfer(&p)
        .unwrap()
        .commit(|lease, buffer| {
            assert!(lease.is_some_and(|v| v.bytes > 0));
            assert!(buffer.is_some_and(|v| v.bytes > 0));
        });
    assert!(!dropped.get());
    assert!(p.0.borrow().live > 0);
    drop(value);
    assert!(dropped.get());
    // Existing Lease::drop asserts the payload marker separately for EACH lane.
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn charge_transfer_host_can_disarm_both_fields_without_extracting_or_dropping_value() {
    let mut p = policy(usize::MAX);
    let mut value = value_with_both_charges(&mut p);
    let nodes = value.value().as_ptr();
    let (lease, buffer) = value.charges();
    let total = lease
        .unwrap()
        .bytes
        .checked_add(buffer.unwrap().bytes)
        .unwrap();
    let remaining = p.0.borrow().live.checked_sub(total).unwrap();
    let host_retained = Cell::new(0);
    let before = (p.0.borrow().work, p.0.borrow().reserves);
    let token = value.preflight_charge_transfer(&p).unwrap();
    token.commit(|lease, buffer| {
        // The host preflighted all arithmetic before this publication tail.
        p.0.borrow_mut().live = remaining;
        host_retained.set(total);
        lease.unwrap().bytes = 0;
        buffer.unwrap().bytes = 0;
    });
    assert_eq!(value.value().as_ptr(), nodes);
    assert_eq!(value.value().as_slice(), &[id("transfer-payload")]);
    assert!(value.charges().0.is_some_and(|v| v.bytes == 0));
    assert!(value.charges().1.is_some_and(|v| v.bytes == 0));
    assert_eq!((p.0.borrow().work, p.0.borrow().reserves), before);
    assert_eq!(p.0.borrow().live, 0);
    drop(value);
    assert_eq!(host_retained.get(), total);
    // The actual value has now dropped; only the host releases its new lane.
    host_retained.set(0);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn charge_transfer_empty_compatibility_value_still_commits_only_once() {
    let mut policy = Unmetered;
    let mut raw = Work::new(1).unwrap();
    let mut additional = 0;
    let build = Build::new(&mut raw, &mut additional, &mut policy);
    let mut value = build.finish(41);
    assert!(value.charges().0.is_none() && value.charges().1.is_none());
    value
        .preflight_charge_transfer(&policy)
        .unwrap()
        .commit(|lease, buffer| {
            assert!(lease.is_none() && buffer.is_none());
        });
    let mut value = value.map_value(&policy, |number| number + 1).unwrap();
    assert_eq!(*value.value(), 42);
    assert!(matches!(
        value.preflight_charge_transfer(&policy),
        Err(AudioBindingBuildError::Document(_))
    ));
    let work = value.take_work(&policy).unwrap();
    assert!(!work.was_forwarded());
    assert_eq!(work.units(), 0);
}

#[path = "composition_tests.rs"]
mod composition;
