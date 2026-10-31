use super::*;

struct CompositionPolicy<const M: bool> {
    inner: Policy,
    issuer_reads: Cell<usize>,
    merges: usize,
    rotate_at: Option<usize>,
}
impl<const M: bool> AudioBindingAdmission for CompositionPolicy<M> {
    const METERED: bool = M;
    type Error = Failure;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.issuer_reads.set(self.issuer_reads.get() + 1);
        self.inner.issuer()
    }
    fn issuer_mismatch(&self) -> Failure {
        self.inner.issuer_mismatch()
    }
    fn spend(&mut self, units: usize) -> Result<(), Failure> {
        self.inner.spend(units)
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, Failure> {
        self.inner.reserve(layout)
    }
    fn merge(&mut self, into: &mut Lease, from: &mut Lease) {
        self.inner.merge(into, from);
        self.merges += 1;
        if self.rotate_at == Some(self.merges) {
            self.inner.0.borrow_mut().issuer += 1;
        }
    }
}
fn composing<const M: bool>(policy: &Policy) -> CompositionPolicy<M> {
    CompositionPolicy {
        inner: policy.clone(),
        issuer_reads: Cell::new(0),
        merges: 0,
        rotate_at: None,
    }
}
fn counters(p: &Policy) -> (usize, usize, usize) {
    let state = p.0.borrow();
    (state.live, state.work, state.reserves)
}
fn lanes<T>(value: &AudioBindingValue<T, usize, Lease>) -> (usize, usize) {
    let (payload, buffer) = value.charges();
    (
        payload.map_or(0, |lease| lease.bytes),
        buffer.map_or(0, |lease| lease.bytes),
    )
}
fn consume<T>(value: &mut AudioBindingValue<T, usize, Lease>, p: &Policy) -> usize {
    let receipt = value.take_work(p).unwrap();
    assert!(receipt.was_forwarded());
    let units = receipt.units();
    assert!(value.take_work(p).is_err());
    units
}

#[derive(Clone)]
struct DropWatch {
    count: Rc<Cell<usize>>,
    all: Rc<Cell<bool>>,
}
impl DropWatch {
    fn new() -> Self {
        Self {
            count: Rc::new(Cell::new(0)),
            all: Rc::new(Cell::new(false)),
        }
    }
    fn install(&self, p: &Policy) {
        p.0.borrow_mut().watch = Some(Rc::clone(&self.all));
    }
    fn assert_both(&self) {
        assert_eq!(self.count.get(), 2);
        assert!(self.all.get());
    }
}
struct Watched {
    _nodes: Vec<NodeId>,
    watch: DropWatch,
}
impl Drop for Watched {
    fn drop(&mut self) {
        self.watch.count.set(self.watch.count.get() + 1);
        self.watch.all.set(self.watch.count.get() == 2);
    }
}
fn watched(
    p: &mut Policy,
    watch: &DropWatch,
    consumed: bool,
) -> AudioBindingValue<Watched, usize, Lease> {
    let mut value = value_with_both_charges(p)
        .map_value(p, |nodes| Watched {
            _nodes: nodes,
            watch: watch.clone(),
        })
        .unwrap();
    if consumed {
        consume(&mut value, p);
    }
    value
}

#[test]
fn resolved_and_two_lane_values_zip_without_new_work_or_allocation_requests() {
    let (state, path) = fixture(2);
    let mut p = policy(usize::MAX);
    let mut resolved = admitted(&state, &path, &mut p).unwrap();
    let expected = resolved.value().clone();
    let mut nodes = value_with_both_charges(&mut p);
    let node_buffer = nodes.value().as_ptr();
    let a = lanes(&resolved);
    let b = lanes(&nodes);
    let billed = consume(&mut resolved, &p) + consume(&mut nodes, &p);
    assert_eq!(p.0.borrow().work, billed);
    let before = counters(&p);
    {
        let mut state = p.0.borrow_mut();
        state.cap = state.live;
        state.work_cap = state.work;
    }
    let mut adapter = composing::<true>(&p);
    let mut result = resolved.zip_value(nodes, &mut adapter).unwrap();
    assert_eq!(result.value().0, expected);
    assert_eq!(result.value().1.as_ptr(), node_buffer);
    assert_eq!(lanes(&result), (a.0 + b.0, a.1 + b.1));
    assert_eq!(counters(&p), before);
    assert!(result.take_work(&p).is_err(), "zip must not mint a receipt");
    result.validate_composition(&p).unwrap();
    let result = result
        .map_value(&p, |(resolved, nodes)| (nodes, resolved))
        .unwrap();
    assert_eq!(result.value().0.as_ptr(), node_buffer);
    assert_eq!(result.value().1, expected);
    assert_eq!(counters(&p), before);
    drop(result);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn both_independent_lanes_survive_zip_and_all_values_drop_before_charges() {
    let mut p = policy(usize::MAX);
    let watch = DropWatch::new();
    let left = watched(&mut p, &watch, true);
    let right = watched(&mut p, &watch, true);
    let a = lanes(&left);
    let b = lanes(&right);
    watch.install(&p);
    let before = counters(&p);
    let mut adapter = composing::<true>(&p);
    let result = left.zip_value(right, &mut adapter).unwrap();
    assert_eq!(adapter.merges, 2);
    assert_eq!(adapter.issuer_reads.get(), 3);
    assert_eq!(lanes(&result), (a.0 + b.0, a.1 + b.1));
    assert_eq!(counters(&p), before);
    assert_eq!(watch.count.get(), 0);
    drop(result);
    watch.assert_both();
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn composition_preflight_is_read_only_and_receipt_consumption_is_linear() {
    let mut p = policy(usize::MAX);
    let mut value = value_with_both_charges(&mut p);
    let before = counters(&p);
    let address = value.value().as_ptr();
    for _ in 0..2 {
        let Err(AudioBindingBuildError::Document(error)) = value.validate_composition(&p) else {
            panic!("unconsumed work must prevent composition");
        };
        assert_eq!(
            error.message,
            "binding composition requires consumed work receipt"
        );
        assert_eq!(counters(&p), before);
        assert_eq!(value.value().as_ptr(), address);
    }
    assert_eq!(consume(&mut value, &p), before.1);
    value.validate_composition(&p).unwrap();
    value.validate_composition(&p).unwrap();
    assert_eq!(counters(&p), before);
    assert_eq!(value.value().as_ptr(), address);
    drop(value);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn either_pending_receipt_refuses_before_merge_and_drops_both_values_first() {
    for pending in [0, 1] {
        let mut p = policy(usize::MAX);
        let watch = DropWatch::new();
        let left = watched(&mut p, &watch, pending != 0);
        let right = watched(&mut p, &watch, pending != 1);
        watch.install(&p);
        let before = counters(&p);
        let mut adapter = composing::<true>(&p);
        let Err(AudioBindingBuildError::Document(error)) = left.zip_value(right, &mut adapter)
        else {
            panic!("pending input receipt must be rejected");
        };
        assert_eq!(
            error.message,
            "binding composition requires consumed work receipt"
        );
        assert_eq!(adapter.merges, 0);
        watch.assert_both();
        assert_eq!(counters(&p), (0, before.1, before.2));
    }
}

#[test]
fn either_transferred_input_refuses_before_merge_and_preserves_drop_order() {
    for transferred in [0, 1] {
        let mut p = policy(usize::MAX);
        let watch = DropWatch::new();
        let mut left = watched(&mut p, &watch, true);
        let mut right = watched(&mut p, &watch, true);
        let selected = if transferred == 0 {
            &mut left
        } else {
            &mut right
        };
        selected
            .preflight_charge_transfer(&p)
            .unwrap()
            .commit(|_, _| {});
        watch.install(&p);
        let before = counters(&p);
        let mut adapter = composing::<true>(&p);
        let Err(AudioBindingBuildError::Document(error)) = left.zip_value(right, &mut adapter)
        else {
            panic!("transferred input must be rejected");
        };
        assert_eq!(error.message, "binding charge transfer already committed");
        assert_eq!(adapter.merges, 0);
        watch.assert_both();
        assert_eq!(counters(&p), (0, before.1, before.2));
    }
}

#[test]
fn either_foreign_issuer_refuses_before_merge_and_drops_all_values_first() {
    for foreign in [0, 1] {
        let mut p = policy(usize::MAX);
        let mut other = policy(usize::MAX);
        other.0.borrow_mut().issuer = 2;
        let watch = DropWatch::new();
        let left = watched(if foreign == 0 { &mut other } else { &mut p }, &watch, true);
        let right = watched(if foreign == 1 { &mut other } else { &mut p }, &watch, true);
        watch.install(&p);
        watch.install(&other);
        let before = counters(&p);
        let foreign_before = counters(&other);
        let mut adapter = composing::<true>(&p);
        assert!(matches!(
            left.zip_value(right, &mut adapter),
            Err(AudioBindingBuildError::Policy(Failure::Issuer))
        ));
        assert_eq!(adapter.merges, 0);
        watch.assert_both();
        assert_eq!(counters(&p), (0, before.1, before.2));
        assert_eq!(counters(&other), (0, foreign_before.1, foreign_before.2));
    }
}

#[test]
fn wrong_policy_mode_and_either_mixed_input_mode_refuse_before_callbacks() {
    for metered_left in [false, true] {
        let mut p = policy(usize::MAX);
        let mut metered = value_with_both_charges(&mut p);
        consume(&mut metered, &p);
        let mut no_admission = NoAdmission(p.clone());
        let mut raw = Work::new(1).unwrap();
        let mut additional = 0;
        let mut plain = Build::new(&mut raw, &mut additional, &mut no_admission).finish(17);
        let receipt = plain.take_work(&no_admission).unwrap();
        assert!(!receipt.was_forwarded());
        let before = counters(&p);
        let mut adapter = composing::<true>(&p);
        if metered_left {
            assert!(matches!(
                metered.zip_value(plain, &mut adapter),
                Err(AudioBindingBuildError::Policy(Failure::Issuer))
            ));
        } else {
            assert!(matches!(
                plain.zip_value(metered, &mut adapter),
                Err(AudioBindingBuildError::Policy(Failure::Issuer))
            ));
        }
        assert_eq!(adapter.merges, 0);
        assert_eq!(counters(&p), (0, before.1, before.2));
    }
    let mut p = policy(usize::MAX);
    let mut left = value_with_both_charges(&mut p);
    let mut right = value_with_both_charges(&mut p);
    consume(&mut left, &p);
    consume(&mut right, &p);
    let before = counters(&p);
    let mut wrong_mode = composing::<false>(&p);
    assert!(matches!(
        left.zip_value(right, &mut wrong_mode),
        Err(AudioBindingBuildError::Policy(Failure::Issuer))
    ));
    assert_eq!(wrong_mode.merges, 0);
    assert_eq!(counters(&p), (0, before.1, before.2));
}

#[test]
fn issuer_rotation_after_either_actual_merge_releases_after_both_values() {
    for rotate_at in [1, 2] {
        let mut p = policy(usize::MAX);
        let watch = DropWatch::new();
        let left = watched(&mut p, &watch, true);
        let right = watched(&mut p, &watch, true);
        watch.install(&p);
        let before = counters(&p);
        let mut adapter = composing::<true>(&p);
        adapter.rotate_at = Some(rotate_at);
        assert!(matches!(
            left.zip_value(right, &mut adapter),
            Err(AudioBindingBuildError::Policy(Failure::Issuer))
        ));
        assert_eq!(adapter.merges, rotate_at);
        assert_eq!(adapter.issuer_reads.get(), rotate_at + 1);
        watch.assert_both();
        assert_eq!(counters(&p), (0, before.1, before.2));
    }
}

#[test]
fn composition_checks_foreign_mode_without_consuming_retryable_state() {
    let mut p = policy(usize::MAX);
    let mut value = value_with_both_charges(&mut p);
    consume(&mut value, &p);
    let wrong = policy(usize::MAX);
    wrong.0.borrow_mut().issuer = 2;
    let before = counters(&p);
    assert!(matches!(
        value.validate_composition(&wrong),
        Err(AudioBindingBuildError::Policy(Failure::Issuer))
    ));
    assert!(matches!(
        value.validate_composition(&NoAdmission(p.clone())),
        Err(AudioBindingBuildError::Policy(Failure::Issuer))
    ));
    value.validate_composition(&p).unwrap();
    assert_eq!(counters(&p), before);
    drop(value);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn empty_first_value_keeps_both_right_lanes_without_a_merge_callback() {
    let mut p = policy(usize::MAX);
    let mut raw = Work::new(1).unwrap();
    let mut additional = 0;
    let mut left = Build::new(&mut raw, &mut additional, &mut p).finish(17);
    assert_eq!(consume(&mut left, &p), 0);
    let mut right = value_with_both_charges(&mut p);
    consume(&mut right, &p);
    let expected = lanes(&right);
    let before = counters(&p);
    let mut adapter = composing::<true>(&p);
    let result = left.zip_value(right, &mut adapter).unwrap();
    assert_eq!(adapter.merges, 0);
    assert_eq!(result.value().0, 17);
    assert_eq!(result.value().1, vec![id("transfer-payload")]);
    assert_eq!(lanes(&result), expected);
    assert_eq!(counters(&p), before);
    drop(result);
    assert_eq!(p.0.borrow().live, 0);
}

#[test]
fn empty_unmetered_values_zip_with_no_leases_or_new_receipt() {
    let mut p = Unmetered;
    let mut left_work = Work::new(1).unwrap();
    let mut left_extra = 0;
    let mut left = Build::new(&mut left_work, &mut left_extra, &mut p).finish(17);
    let mut right_work = Work::new(1).unwrap();
    let mut right_extra = 0;
    let mut right = Build::new(&mut right_work, &mut right_extra, &mut p).finish(25);
    assert_eq!(left.take_work(&p).unwrap().units(), 0);
    assert_eq!(right.take_work(&p).unwrap().units(), 0);
    let mut result = left.zip_value(right, &mut p).unwrap();
    assert_eq!(*result.value(), (17, 25));
    assert!(result.charges().0.is_none() && result.charges().1.is_none());
    assert!(result.take_work(&p).is_err());
    result.validate_composition(&p).unwrap();
    let result = result.map_value(&p, |(a, b)| a + b).unwrap();
    assert_eq!(*result.value(), 42);
}
