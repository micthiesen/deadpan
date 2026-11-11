//! Unique ownership of the public Point output mask, independent of its controller.
use super::*;
use serde::{Serialize, Serializer};
use std::alloc::Layout;
use std::fmt;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const FRAME_BYTES: u64 = std::mem::size_of::<[f32; 2]>() as u64;

struct AccountData {
    live_frames: AtomicU64,
}

// The pinned standard library's Arc allocation has these two counters before T.
// The qualified allocation-layout probe is part of this unit's evidence.
#[repr(C)]
struct ArcShell {
    strong: AtomicUsize,
    weak: AtomicUsize,
    data: AccountData,
}

struct Handle(Option<Arc<AccountData>>);
impl Handle {
    fn data(&self) -> &AccountData {
        self.0.as_ref().expect("live closed account handle")
    }

    fn duplicate(&self) -> Self {
        Self(Some(Arc::clone(
            self.0.as_ref().expect("live closed account handle"),
        )))
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        {
            if let Some(handle) = self.0.take()
                && let Some(data) = Arc::into_inner(handle)
            {
                // into_inner has freed the Arc allocation before returning its data.
                let shell =
                    frames_for(Layout::new::<ArcShell>()).expect("qualified Arc shell size");
                let previous = data.live_frames.fetch_sub(shell, Ordering::AcqRel);
                debug_assert_eq!(previous, shell, "all payload charges precede their handles");
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Account {
    handle: Option<Handle>,
}
impl Account {
    pub(super) fn live_frames(&self) -> u64 {
        self.handle.as_ref().map_or(0, |handle| {
            handle.data().live_frames.load(Ordering::Acquire)
        })
    }

    fn initialize_after_admission(&mut self, shell: u64) {
        debug_assert!(self.handle.is_none());
        self.handle = Some(Handle(Some(Arc::new(AccountData {
            live_frames: AtomicU64::new(shell),
        }))));
    }

    fn charge_after_admission(&mut self, frames: u64) -> Result<Charge, StageAudioError> {
        let handle = self.handle.as_ref().expect("admitted account shell");
        // This mutable controller is the sole publisher. Other threads only drop.
        handle
            .data()
            .live_frames
            .load(Ordering::Acquire)
            .checked_add(frames)
            .ok_or(StageAudioError::Range)?;
        handle
            .data()
            .live_frames
            .fetch_add(frames, Ordering::AcqRel);
        Ok(Charge {
            handle: handle.duplicate(),
            frames,
        })
    }

    #[cfg(test)]
    pub(super) fn observer(&self) -> Option<Observer> {
        self.handle
            .as_ref()
            .map(|handle| Observer(handle.duplicate()))
    }
}

#[cfg(test)]
pub(super) struct Observer(Handle);
#[cfg(test)]
impl Observer {
    pub(super) fn live_frames(&self) -> u64 {
        self.0.data().live_frames.load(Ordering::Acquire)
    }
}
#[cfg(test)]
impl Clone for Observer {
    fn clone(&self) -> Self {
        Self(self.0.duplicate())
    }
}

pub(crate) struct Charge {
    handle: Handle,
    frames: u64,
}
impl Drop for Charge {
    fn drop(&mut self) {
        let previous = self
            .handle
            .data()
            .live_frames
            .fetch_sub(self.frames, Ordering::AcqRel);
        debug_assert!(previous >= self.frames);
    }
}

mod sealed {
    pub trait Sample {}
    impl Sample for super::ReferenceSample {}
    impl Sample for super::SignalSample {}
    impl Sample for super::AudioSample {}
}

pub trait MaskSample: sealed::Sample + Copy + Eq + fmt::Debug + Serialize {}
impl MaskSample for ReferenceSample {}
impl MaskSample for SignalSample {}
impl MaskSample for AudioSample {}

struct Storage<S: MaskSample> {
    ranges: Vec<Range<S>>,
    charge: Charge,
}

struct OwnedMask<S: MaskSample> {
    storage: Option<Box<Storage<S>>>,
}

// Keep physical Box deallocation ahead of both the Vec and its charge.
#[inline(never)]
#[expect(
    clippy::boxed_local,
    reason = "Deallocation ordering is part of the admission contract."
)]
fn unbox<T>(boxed: Box<T>) -> T {
    *boxed
}
impl<S: MaskSample> Drop for OwnedMask<S> {
    fn drop(&mut self) {
        if let Some(storage) = self.storage.take() {
            let Storage { ranges, charge } = unbox(storage);
            drop(ranges);
            drop(charge);
        }
    }
}

enum Value<S: MaskSample> {
    Plain(Vec<Range<S>>),
    Owned(OwnedMask<S>),
}

/// Immutable suppression ranges whose admitted storage may outlive StageAudio.
pub struct SuppressionMask<S: MaskSample> {
    value: Value<S>,
}
pub type PointSuppressionMask = SuppressionMask<ReferenceSample>;
pub type SignalSuppressionMask = SuppressionMask<SignalSample>;
pub type AudioSuppressionMask = SuppressionMask<AudioSample>;

#[path = "point_suppression/ops.rs"]
pub(crate) mod ops;
pub(crate) use ops::{
    FlagBuffer, MaskContext, MaskWorkPhase, RangeBuilder, begin_flags, begin_ranges,
};

#[path = "point_suppression/mutations.rs"]
mod mutations;
#[path = "point_suppression/projections.rs"]
mod projections;

#[path = "point_suppression/policy_handoff.rs"]
pub(in crate::stages) mod policy_handoff;

pub(in crate::stages) fn internal_legacy() -> bool {
    #[cfg(test)]
    return super::internal_mask_request::legacy();
    #[cfg(not(test))]
    false
}

impl<S: MaskSample> SuppressionMask<S> {
    #[cfg(test)]
    pub(in crate::stages) fn into_legacy_vector(self) -> Vec<Range<S>> {
        match self.value {
            Value::Plain(ranges) => ranges,
            Value::Owned(_) => panic!("a historical Vec replay cannot detach an owned mask"),
        }
    }

    pub fn as_slice(&self) -> &[Range<S>] {
        self.ranges()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Range<S>> {
        self.as_slice().iter()
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.ranges().capacity()
    }

    fn ranges(&self) -> &Vec<Range<S>> {
        match &self.value {
            Value::Plain(value) => value,
            Value::Owned(value) => {
                &value
                    .storage
                    .as_ref()
                    .expect("live unique mask storage")
                    .ranges
            }
        }
    }
}
impl<S: MaskSample> Default for SuppressionMask<S> {
    fn default() -> Self {
        Self {
            value: Value::Plain(Vec::new()),
        }
    }
}
impl<S: MaskSample> Deref for SuppressionMask<S> {
    type Target = [Range<S>];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
impl<'a, S: MaskSample> IntoIterator for &'a SuppressionMask<S> {
    type Item = &'a Range<S>;
    type IntoIter = std::slice::Iter<'a, Range<S>>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<S: MaskSample> PartialEq for SuppressionMask<S> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl<S: MaskSample> PartialEq<Vec<Range<S>>> for SuppressionMask<S> {
    fn eq(&self, other: &Vec<Range<S>>) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl<S: MaskSample> PartialEq<SuppressionMask<S>> for Vec<Range<S>> {
    fn eq(&self, other: &SuppressionMask<S>) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl<S: MaskSample> Eq for SuppressionMask<S> {}
impl<S: MaskSample> fmt::Debug for SuppressionMask<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}
impl<S: MaskSample> Serialize for SuppressionMask<S> {
    fn serialize<T: Serializer>(&self, serializer: T) -> Result<T::Ok, T::Error> {
        self.as_slice().serialize(serializer)
    }
}

fn frames_for(layout: Layout) -> Result<u64, StageAudioError> {
    Ok(u64::try_from(layout.size())
        .map_err(|_| StageAudioError::Range)?
        .div_ceil(FRAME_BYTES))
}
pub(super) fn shell_frames() -> Result<u64, StageAudioError> {
    frames_for(Layout::new::<ArcShell>())
}
pub(super) fn payload_frames(capacity: usize) -> Result<u64, StageAudioError> {
    payload_frames_for::<ReferenceSample>(capacity)
}
#[cfg(test)]
pub(in crate::stages) fn audio_payload_frames(capacity: usize) -> Result<u64, StageAudioError> {
    payload_frames_for::<AudioSample>(capacity)
}
fn payload_frames_for<S: MaskSample>(capacity: usize) -> Result<u64, StageAudioError> {
    let vector = Layout::array::<Range<S>>(capacity).map_err(|_| StageAudioError::Range)?;
    frames_for(vector)?
        .checked_add(frames_for(Layout::new::<Storage<S>>())?)
        .ok_or(StageAudioError::Range)
}

#[cfg(test)]
thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
pub(super) fn with_legacy_point_suppression<T>(action: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LEGACY.with(|legacy| legacy.set(self.0));
        }
    }
    let _restore = Restore(LEGACY.with(|legacy| legacy.replace(true)));
    super::internal_mask_request::with_legacy_internal_masks(action)
}

#[derive(Clone, Copy)]
enum WorkPhase {
    Account,
    Vector,
    Storage,
    Range,
}
fn spend(control: WorkControl<'_>, phase: WorkPhase) -> Result<(), StageAudioError> {
    control.spend_plan_work(1)?;
    #[cfg(test)]
    work_trace::spent(phase);
    #[cfg(not(test))]
    let _ = phase;
    Ok(())
}

impl StageAudio {
    pub(super) fn reserve_mask_storage(
        &mut self,
        frames: u64,
        control: WorkControl<'_>,
    ) -> Result<Charge, StageAudioError> {
        control.check()?;
        if self.owned_suppression.handle.is_none() {
            let shell = shell_frames()?;
            ops::spend_work(control, ops::MaskWorkPhase::Account, 1)?;
            self.make_room(shell, false, control)?;
            control.check()?;
            self.owned_suppression.initialize_after_admission(shell);
            #[cfg(test)]
            ops::trace::shell_initialized(control.work.borrow().cache_key_scopes > 0, shell);
        }
        self.make_room(frames, false, control)?;
        control.check()?;
        let charge = self.owned_suppression.charge_after_admission(frames)?;
        #[cfg(test)]
        ops::trace::reservation_accepted(control.work.borrow().cache_key_scopes > 0, frames);
        Ok(charge)
    }

    pub(super) fn map_point_suppression(
        &mut self,
        input: SuppressionMask<SignalSample>,
        domain: &AudioPointDomain<'_>,
        control: WorkControl<'_>,
    ) -> Result<PointSuppressionMask, StageAudioError> {
        match input.value {
            Value::Plain(ranges) => self.admit_point_suppression(ranges, domain, control),
            Value::Owned(owner) => SuppressionMask {
                value: Value::Owned(owner),
            }
            .try_map(
                |range| {
                    Ok::<_, PlanError>(
                        domain.reference_at_signal(range.start)?
                            ..domain.reference_at_signal(range.end)?,
                    )
                },
                &mut self.masks(control),
            ),
        }
    }

    pub(super) fn admit_point_suppression(
        &mut self,
        input: Vec<Range<SignalSample>>,
        domain: &AudioPointDomain<'_>,
        control: WorkControl<'_>,
    ) -> Result<PointSuppressionMask, StageAudioError> {
        #[cfg(test)]
        let mut trace = work_trace::Scope::new();
        #[cfg(test)]
        if LEGACY.with(std::cell::Cell::get) {
            // Keep the original Vec iterator/collect specialization in this lane.
            let ranges = input
                .into_iter()
                .map(|range| {
                    Ok::<_, PlanError>(
                        domain.reference_at_signal(range.start)?
                            ..domain.reference_at_signal(range.end)?,
                    )
                })
                .collect::<Result<_, _>>()?;
            trace.ordinary();
            return Ok(PointSuppressionMask {
                value: Value::Plain(ranges),
            });
        }
        control.check()?;
        if input.is_empty() {
            #[cfg(test)]
            trace.empty();
            return Ok(PointSuppressionMask::default());
        }

        let count = input.len();
        let payload = payload_frames(count)?;
        if self.owned_suppression.handle.is_none() {
            let shell = shell_frames()?;
            spend(control, WorkPhase::Account)?;
            self.make_room(shell, false, control)?;
            control.check()?;
            self.owned_suppression.initialize_after_admission(shell);
        }
        spend(control, WorkPhase::Vector)?;
        self.make_room(payload, false, control)?;
        control.check()?;
        let charge = self.owned_suppression.charge_after_admission(payload)?;

        // On every mapping/check error the destination Vec drops before charge.
        let mut pending = Storage {
            ranges: Vec::new(),
            charge,
        };
        pending.ranges = Vec::with_capacity(count);
        assert_eq!(pending.ranges.capacity(), count);
        for range in &input {
            spend(control, WorkPhase::Range)?;
            let start = domain.reference_at_signal(range.start)?;
            let end = domain.reference_at_signal(range.end)?;
            pending.ranges.push(start..end);
        }
        spend(control, WorkPhase::Storage)?;
        let result = PointSuppressionMask {
            value: Value::Owned(OwnedMask {
                storage: Some(Box::new(pending)),
            }),
        };
        control.check()?;
        drop(input);
        #[cfg(test)]
        trace.completed();
        Ok(result)
    }
}

#[cfg(test)]
pub(super) mod work_trace {
    use super::WorkPhase;
    use serde::Serialize;
    use std::cell::Cell;

    #[derive(Clone, Copy, Default, Serialize)]
    pub(in crate::stages) struct Snapshot {
        constructor_calls: usize,
        completed: usize,
        failed: usize,
        empty: usize,
        ordinary: usize,
        accepted_account_allocation_units: usize,
        accepted_vector_allocation_units: usize,
        accepted_storage_allocation_units: usize,
        accepted_range_mapping_units: usize,
        accepted_residency_units: usize,
        overflow: bool,
    }
    thread_local! {
        static ENABLED: Cell<bool> = const { Cell::new(false) };
        static TRACE: Cell<Snapshot> = Cell::new(Snapshot::default());
    }
    fn update(action: impl FnOnce(&mut Snapshot)) {
        if ENABLED.with(Cell::get) {
            TRACE.with(|trace| {
                let mut value = trace.get();
                action(&mut value);
                trace.set(value);
            });
        }
    }
    fn add(value: &mut usize, overflow: &mut bool) {
        match value.checked_add(1) {
            Some(next) => *value = next,
            None => *overflow = true,
        }
    }
    pub(in crate::stages) fn residency_spent() {
        update(|value| add(&mut value.accepted_residency_units, &mut value.overflow));
    }
    pub(super) fn spent(phase: WorkPhase) {
        update(|value| {
            let target = match phase {
                WorkPhase::Account => &mut value.accepted_account_allocation_units,
                WorkPhase::Vector => &mut value.accepted_vector_allocation_units,
                WorkPhase::Storage => &mut value.accepted_storage_allocation_units,
                WorkPhase::Range => &mut value.accepted_range_mapping_units,
            };
            add(target, &mut value.overflow);
        });
    }
    pub(super) struct Scope {
        tracked: bool,
        finished: bool,
    }
    impl Scope {
        pub(super) fn new() -> Self {
            let tracked = ENABLED.with(Cell::get);
            update(|value| add(&mut value.constructor_calls, &mut value.overflow));
            Self {
                tracked,
                finished: false,
            }
        }
        fn finish(&mut self, action: impl FnOnce(&mut Snapshot)) {
            if self.tracked {
                TRACE.with(|trace| {
                    let mut value = trace.get();
                    action(&mut value);
                    trace.set(value);
                });
            }
            self.finished = true;
        }
        pub(super) fn completed(&mut self) {
            self.finish(|value| add(&mut value.completed, &mut value.overflow));
        }
        pub(super) fn empty(&mut self) {
            self.finish(|value| add(&mut value.empty, &mut value.overflow));
        }
        pub(super) fn ordinary(&mut self) {
            self.finish(|value| add(&mut value.ordinary, &mut value.overflow));
        }
    }
    impl Drop for Scope {
        fn drop(&mut self) {
            if self.tracked && !self.finished {
                TRACE.with(|trace| {
                    let mut value = trace.get();
                    add(&mut value.failed, &mut value.overflow);
                    trace.set(value);
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned<S: MaskSample>(account: &mut Account, ranges: Vec<Range<S>>) -> SuppressionMask<S> {
        if account.handle.is_none() {
            account.initialize_after_admission(shell_frames().unwrap());
        }
        let charge = account
            .charge_after_admission(payload_frames_for::<S>(ranges.capacity()).unwrap())
            .unwrap();
        SuppressionMask {
            value: Value::Owned(OwnedMask {
                storage: Some(Box::new(Storage { ranges, charge })),
            }),
        }
    }

    #[test]
    fn mask_values_are_immutable_and_serialize_like_the_prior_vector() {
        let mut account = Account::default();
        let ranges = vec![
            ReferenceSample(-7)..ReferenceSample(-2),
            ReferenceSample(4)..ReferenceSample(9),
        ];
        let expected = serde_json::to_value(&ranges).unwrap();
        let plain = PointSuppressionMask {
            value: Value::Plain(ranges.clone()),
        };
        let mask = owned(&mut account, ranges);
        assert_eq!(mask, plain);
        assert_eq!(format!("{mask:?}"), format!("{:?}", plain.as_slice()));
        assert_eq!(serde_json::to_value(&mask).unwrap(), expected);
        assert_eq!(mask.len(), 2);
        assert!(!mask.is_empty());
        assert_eq!(mask.capacity(), 2);
        assert_eq!((&mask).into_iter().len(), 2);
        assert_eq!(mask[0], ReferenceSample(-7)..ReferenceSample(-2));
        assert_eq!(mask.get(2), None);
    }

    #[test]
    fn mask_keeps_account_alive_after_controller_and_observer_copies_drop() {
        let mut account = Account::default();
        let mask = owned(&mut account, vec![ReferenceSample(0)..ReferenceSample(3)]);
        let observer = account.observer().unwrap();
        let another = observer.clone();
        let expected = shell_frames().unwrap() + payload_frames(mask.capacity()).unwrap();
        assert_eq!(observer.live_frames(), expected);
        drop(account);
        drop(another);
        assert_eq!(observer.live_frames(), expected);
        assert_eq!(mask.as_slice(), [ReferenceSample(0)..ReferenceSample(3)]);
        drop(mask);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
        drop(observer);
    }

    #[test]
    fn masks_dropping_concurrently_release_only_their_own_storage() {
        let mut account = Account::default();
        let masks = (0..16)
            .map(|value| {
                owned(
                    &mut account,
                    vec![ReferenceSample(value)..ReferenceSample(value + 1)],
                )
            })
            .collect::<Vec<_>>();
        let observer = account.observer().unwrap();
        let shell = shell_frames().unwrap();
        assert_eq!(
            observer.live_frames(),
            shell + 16 * payload_frames(1).unwrap()
        );
        drop(account);
        std::thread::scope(|scope| {
            for mask in masks {
                scope.spawn(move || drop(mask));
            }
        });
        assert_eq!(observer.live_frames(), shell);
    }

    #[test]
    fn concurrent_last_closed_handles_use_into_inner_without_weak_handles() {
        let mut account = Account::default();
        let masks = (0..32)
            .map(|value| {
                owned(
                    &mut account,
                    vec![ReferenceSample(value)..ReferenceSample(value + 1)],
                )
            })
            .collect::<Vec<_>>();
        std::thread::scope(|scope| {
            scope.spawn(move || drop(account));
            for mask in masks {
                scope.spawn(move || drop(mask));
            }
        });
    }

    #[test]
    fn empty_default_has_no_account_or_payload_allocation() {
        let account = Account::default();
        let mask = PointSuppressionMask::default();
        assert_eq!(account.live_frames(), 0);
        assert!(account.observer().is_none());
        assert!(mask.is_empty());
        assert_eq!(mask.capacity(), 0);
        assert_eq!(serde_json::to_value(mask).unwrap(), serde_json::json!([]));
    }

    #[test]
    fn pinned_mask_and_account_layouts_preserve_the_public_vector_shell() {
        assert_eq!(
            std::mem::size_of::<PointSuppressionMask>(),
            std::mem::size_of::<Vec<Range<ReferenceSample>>>()
        );
        assert_eq!(
            std::mem::align_of::<PointSuppressionMask>(),
            std::mem::align_of::<Vec<Range<ReferenceSample>>>()
        );
        assert_eq!(std::mem::size_of::<AccountData>(), 8);
        assert_eq!(Layout::new::<ArcShell>().size(), 24);
        assert_eq!(Layout::new::<Storage<ReferenceSample>>().size(), 40);
        assert_eq!(std::mem::size_of::<Charge>(), 16);
        assert_eq!(shell_frames().unwrap(), 3);
        assert_eq!(payload_frames(1).unwrap(), 7);
    }

    #[test]
    fn distinct_sample_labels_share_account_without_relabelling_ranges() {
        fn check_layout<S: MaskSample>() {
            assert_eq!(
                Layout::new::<SuppressionMask<S>>(),
                Layout::new::<Vec<Range<S>>>()
            );
            assert_eq!(Layout::new::<Storage<S>>().size(), 40);
            assert_eq!(payload_frames_for::<S>(1).unwrap(), 7);
        }
        check_layout::<ReferenceSample>();
        check_layout::<SignalSample>();
        check_layout::<AudioSample>();

        let mut account = Account::default();
        let signal = owned(&mut account, vec![SignalSample(-7)..SignalSample(9)]);
        let audio = owned(&mut account, vec![AudioSample(11)..AudioSample(19)]);
        let reference = owned(&mut account, vec![ReferenceSample(23)..ReferenceSample(29)]);
        let observer = account.observer().unwrap();
        drop(account);
        assert_eq!(observer.live_frames(), 24);
        assert_eq!(signal.as_slice(), [SignalSample(-7)..SignalSample(9)]);
        assert_eq!(audio.as_slice(), [AudioSample(11)..AudioSample(19)]);
        assert_eq!(
            reference.as_slice(),
            [ReferenceSample(23)..ReferenceSample(29)]
        );
        drop(signal);
        assert_eq!(observer.live_frames(), 17);
        drop(audio);
        assert_eq!(observer.live_frames(), 10);
        drop(reference);
        assert_eq!(observer.live_frames(), 3);
    }
}
