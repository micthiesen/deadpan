//! Construction policy for actual binding queries, independent of media hosts.
use super::*;
use std::alloc::Layout;

pub trait AudioBindingAdmission {
    const METERED: bool = true;
    type Error;
    type Issuer: Copy + Eq;
    type Lease;
    fn issuer(&self) -> Self::Issuer;
    fn issuer_mismatch(&self) -> Self::Error;
    fn spend(&mut self, units: usize) -> Result<(), Self::Error>;
    fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, Self::Error>;
    /// Trusted infallible, allocation-free merge; disarm source without release.
    fn merge(&mut self, into: &mut Self::Lease, from: &mut Self::Lease);
}
#[derive(Debug)]
pub enum AudioBindingBuildError<E> {
    Document(DocumentError),
    Policy(E),
}
impl<E> From<DocumentError> for AudioBindingBuildError<E> {
    fn from(e: DocumentError) -> Self {
        Self::Document(e)
    }
}
impl<E> From<TimeError> for AudioBindingBuildError<E> {
    fn from(e: TimeError) -> Self {
        Self::Document(e.into())
    }
}
pub(crate) type BuildResult<T, E> = Result<T, AudioBindingBuildError<E>>;
pub(crate) type AdmittedResult<T, P> = BuildResult<
    AudioBindingValue<T, <P as AudioBindingAdmission>::Issuer, <P as AudioBindingAdmission>::Lease>,
    <P as AudioBindingAdmission>::Error,
>;
pub(crate) type ProjectedSupport = (crate::FrozenAudioProjection, std::ops::Range<ExactRatio>);
pub(crate) type ProjectedAllocation = (
    crate::FrozenAudioProjection,
    Option<std::ops::Range<ExactRatio>>,
);

pub(crate) struct Unmetered;
impl AudioBindingAdmission for Unmetered {
    const METERED: bool = false;
    type Error = DocumentError;
    type Issuer = ();
    type Lease = ();
    fn issuer(&self) {}
    fn issuer_mismatch(&self) -> DocumentError {
        invalid("audio binding issuer mismatch")
    }
    fn spend(&mut self, _: usize) -> Result<(), DocumentError> {
        Ok(())
    }
    fn reserve(&mut self, _: Layout) -> Result<(), DocumentError> {
        Ok(())
    }
    fn merge(&mut self, _: &mut (), _: &mut ()) {}
}
pub(crate) fn legacy<T>(value: BuildResult<T, DocumentError>) -> Result<T, DocumentError> {
    value.map_err(|e| match e {
        AudioBindingBuildError::Document(e) | AudioBindingBuildError::Policy(e) => e,
    })
}
/// Linear provenance in both modes; false means the host still owes the debit.
pub struct AudioBindingForwardedWork<I> {
    issuer: I,
    canonical_units: usize,
    additional_units: usize,
    controller_debited: bool,
}
impl<I: Copy> AudioBindingForwardedWork<I> {
    pub fn issuer(&self) -> I {
        self.issuer
    }
    pub fn canonical_units(&self) -> usize {
        self.canonical_units
    }
    pub fn additional_units(&self) -> usize {
        self.additional_units
    }
    pub fn units(&self) -> usize {
        self.canonical_units + self.additional_units
    }
    pub fn was_forwarded(&self) -> bool {
        self.controller_debited
    }
}
/// Owned values drop before their independent resource leases.
pub struct AudioBindingValue<T, I, L> {
    pub(crate) value: Option<T>,
    issuer: I,
    pub(crate) lease: Option<L>,
    pub(crate) buffer: Option<L>,
    work: Option<AudioBindingForwardedWork<I>>,
    metered: bool,
    charge_transferred: bool,
}
pub type AdmittedAudioBinding<I, L> = AudioBindingValue<ResolvedAudioBinding, I, L>;
/// A checked exclusive borrow for one host-owned charge publication.
/// Dropping an unused token leaves the value and its charges retryable.
/// The temporary admission-policy borrow does not survive preflight.
pub struct AudioBindingChargeTransfer<'value, T, I, L> {
    value: &'value mut AudioBindingValue<T, I, L>,
}
impl<T, I, L> AudioBindingChargeTransfer<'_, T, I, L> {
    /// The same independent value and buffer charges, without taking either.
    pub fn charges(&self) -> (Option<&L>, Option<&L>) {
        (self.value.lease.as_ref(), self.value.buffer.as_ref())
    }

    /// One trusted infallible, non-unwinding, allocation-free publication.
    /// All host identity, arithmetic and ledger checks must already be done.
    /// The callback borrows both actual lease fields and may mark their charge
    /// transferred into its retained-value ledger. Core does not remove them.
    /// No policy callback, validation, work debit or refund occurs here.
    pub fn commit(self, publish: impl FnOnce(Option<&mut L>, Option<&mut L>)) {
        self.value.charge_transferred = true;
        publish(self.value.lease.as_mut(), self.value.buffer.as_mut());
    }
}

impl<T, I: Copy + Eq, L> AudioBindingValue<T, I, L> {
    pub fn value(&self) -> &T {
        self.value.as_ref().expect("unmoved binding value")
    }
    pub fn issuer(&self) -> I {
        self.issuer
    }
    /// Borrow the actual independent value and buffer charges for host planning.
    /// This does not validate a policy or grant ancestry/media authority.
    pub fn charges(&self) -> (Option<&L>, Option<&L>) {
        (self.lease.as_ref(), self.buffer.as_ref())
    }

    /// Preflight a single committed handoff without spending or taking charges.
    /// The returned token borrows only this value, never the admission policy.
    /// Failed checks and an unused token leave a later valid preflight possible.
    pub fn preflight_charge_transfer<'value, P>(
        &'value mut self,
        policy: &P,
    ) -> BuildResult<AudioBindingChargeTransfer<'value, T, I, L>, P::Error>
    where
        P: AudioBindingAdmission<Issuer = I, Lease = L>,
    {
        self.validate(policy)?;
        if self.charge_transferred {
            return Err(invalid("binding charge transfer already committed").into());
        }
        Ok(AudioBindingChargeTransfer { value: self })
    }

    fn validate<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        &self,
        p: &P,
    ) -> BuildResult<(), P::Error> {
        if self.issuer != p.issuer() || self.metered != P::METERED {
            return Err(AudioBindingBuildError::Policy(p.issuer_mismatch()));
        }
        Ok(())
    }
    pub fn take_work<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        &mut self,
        p: &P,
    ) -> BuildResult<AudioBindingForwardedWork<I>, P::Error> {
        self.validate(p)?;
        self.work
            .take()
            .ok_or_else(|| invalid("binding receipt already consumed").into())
    }
    /// Trusted infallible, non-unwinding and allocation-free ownership move.
    pub fn map_value<U, P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        mut self,
        p: &P,
        map: impl FnOnce(T) -> U,
    ) -> BuildResult<AudioBindingValue<U, I, L>, P::Error> {
        self.validate(p)?;
        let value = map(self.value.take().expect("unmoved binding value"));
        let result = AudioBindingValue {
            value: Some(value),
            issuer: self.issuer,
            lease: self.lease.take(),
            buffer: self.buffer.take(),
            work: self.work.take(),
            metered: self.metered,
            charge_transferred: self.charge_transferred,
        };
        result.validate(p)?;
        Ok(result)
    }
}
impl<T> AudioBindingValue<T, (), ()> {
    pub(crate) fn into_legacy(mut self) -> T {
        self.value.take().expect("legacy binding value")
    }
}
pub(crate) struct Build<'a, P: AudioBindingAdmission> {
    pub(crate) work: &'a mut Work,
    extra: &'a mut usize,
    policy: &'a mut P,
    issuer: P::Issuer,
    charge: Option<P::Lease>,
    before: usize,
    extra_before: usize,
    receipt: bool,
    projection: bool,
}
impl<'a, P: AudioBindingAdmission> Build<'a, P> {
    pub(crate) fn new(work: &'a mut Work, extra: &'a mut usize, policy: &'a mut P) -> Self {
        let before = work.used;
        let extra_before = *extra;
        let issuer = policy.issuer();
        Self {
            work,
            extra,
            policy,
            issuer,
            charge: None,
            before,
            extra_before,
            receipt: true,
            projection: false,
        }
    }
    pub(crate) fn projection(work: &'a mut Work, extra: &'a mut usize, policy: &'a mut P) -> Self {
        let mut value = Self::new(work, extra, policy);
        value.projection = true;
        value
    }
    fn local(&mut self, n: usize) -> Result<(), DocumentError> {
        if self.projection {
            self.work.used = self
                .work
                .used
                .checked_add(n)
                .filter(|v| *v <= self.work.maximum)
                .ok_or_else(|| {
                    DocumentError::new(
                        DocumentErrorCode::LimitExceeded,
                        "frozen projection work exhausted",
                    )
                })?;
            Ok(())
        } else {
            self.work.spend(n)
        }
    }
    pub(crate) fn fork(&mut self) -> Build<'_, P> {
        let mut result = Build::new(self.work, self.extra, self.policy);
        result.issuer = self.issuer;
        result.receipt = false;
        result.projection = self.projection;
        result
    }
    pub(crate) fn policy(&mut self) -> &mut P {
        self.policy
    }
    pub(crate) fn validate(&self) -> BuildResult<(), P::Error> {
        if self.issuer != self.policy.issuer() {
            return Err(AudioBindingBuildError::Policy(
                self.policy.issuer_mismatch(),
            ));
        }
        Ok(())
    }
    pub(crate) fn used(&self) -> usize {
        self.work.used
    }
    pub(crate) fn remaining(&self) -> BuildResult<usize, P::Error> {
        Ok(self.work.remaining()?)
    }
    pub(crate) fn spend(&mut self, n: usize) -> BuildResult<(), P::Error> {
        self.validate()?;
        self.local(n)?;
        if P::METERED {
            self.policy
                .spend(n)
                .map_err(AudioBindingBuildError::Policy)?;
        }
        self.validate()
    }
    pub(crate) fn additional(&mut self, n: usize) -> BuildResult<(), P::Error> {
        if P::METERED && n != 0 {
            self.validate()?;
            self.local(n)?;
            *self.extra = self
                .extra
                .checked_add(n)
                .ok_or_else(|| limit("binding additional work overflow"))?;
            self.policy
                .spend(n)
                .map_err(AudioBindingBuildError::Policy)?;
            self.validate()?;
        }
        Ok(())
    }
    pub(crate) fn account_child(
        &mut self,
        r: AudioBindingForwardedWork<P::Issuer>,
    ) -> BuildResult<(), P::Error> {
        self.validate()?;
        if r.issuer != self.issuer || r.controller_debited != P::METERED {
            return Err(AudioBindingBuildError::Policy(
                self.policy.issuer_mismatch(),
            ));
        }
        self.local(r.units())?;
        *self.extra = self
            .extra
            .checked_add(r.additional_units)
            .ok_or_else(|| limit("binding additional work overflow"))?;
        Ok(())
    }
    pub(crate) fn reserve(&mut self, layout: Layout) -> BuildResult<(), P::Error> {
        self.validate()?;
        if P::METERED && layout.size() != 0 {
            let mut lease = self
                .policy
                .reserve(layout)
                .map_err(AudioBindingBuildError::Policy)?;
            self.validate()?;
            if let Some(into) = &mut self.charge {
                self.policy.merge(into, &mut lease);
                self.validate()?;
            } else {
                self.charge = Some(lease);
            }
        }
        Ok(())
    }
    pub(crate) fn array<T>(&mut self, n: usize) -> BuildResult<(), P::Error> {
        self.additional(1)?;
        self.reserve(Layout::array::<T>(n).map_err(|_| limit("binding metadata layout"))?)
    }
    fn identity(&mut self, text: &str) -> BuildResult<(), P::Error> {
        self.additional(1)?;
        let (layout, _) = Layout::new::<[usize; 2]>()
            .extend(Layout::for_value(text))
            .map_err(|_| limit("binding metadata layout"))?;
        self.reserve(layout.pad_to_align())
    }
    pub(crate) fn node(&mut self, v: &NodeId) -> BuildResult<NodeId, P::Error> {
        if P::METERED {
            self.identity(v.as_str())?;
        }
        Ok(v.clone())
    }
    pub(crate) fn iteration(&mut self, v: &IterationId) -> BuildResult<IterationId, P::Error> {
        if P::METERED {
            self.additional(1)?;
            self.identity(v.allocation.as_str())?;
        }
        Ok(v.clone())
    }
    pub(crate) fn instance(&mut self, v: &InstancePath) -> BuildResult<InstancePath, P::Error> {
        Ok(InstancePath {
            node: self.node(&v.node)?,
            repeats: self.repeats(&v.repeats)?,
        })
    }
    pub(crate) fn repeats(
        &mut self,
        s: &[RepeatInstance],
    ) -> BuildResult<Vec<RepeatInstance>, P::Error> {
        if !P::METERED {
            return Ok(s.to_vec());
        }
        self.array::<RepeatInstance>(s.len())?;
        let mut v = Vec::with_capacity(s.len());
        assert_eq!(v.capacity(), s.len());
        for r in s {
            self.additional(1)?;
            v.push(RepeatInstance {
                node: self.node(&r.node)?,
                iteration: self.iteration(&r.iteration)?,
            });
        }
        Ok(v)
    }
    pub(crate) fn clock(&mut self, v: &AudioClockRoot) -> BuildResult<AudioClockRoot, P::Error> {
        self.additional(1)?;
        Ok(match v {
            AudioClockRoot::ProjectRootRoundEven => AudioClockRoot::ProjectRootRoundEven,
            AudioClockRoot::PreserveInputPointCeil { stage } => {
                AudioClockRoot::PreserveInputPointCeil {
                    stage: self.node(stage)?,
                }
            }
            AudioClockRoot::DefinitionPointCeil { root } => AudioClockRoot::DefinitionPointCeil {
                root: self.node(root)?,
            },
            AudioClockRoot::GapDefinitionPointCeil { repeat } => {
                AudioClockRoot::GapDefinitionPointCeil {
                    repeat: self.node(repeat)?,
                }
            }
        })
    }
    pub(crate) fn finish<T>(mut self, value: T) -> AudioBindingValue<T, P::Issuer, P::Lease> {
        let units = self.work.used - self.before;
        let extra = *self.extra - self.extra_before;
        AudioBindingValue {
            value: Some(value),
            issuer: self.issuer,
            lease: self.charge.take(),
            buffer: None,
            work: self.receipt.then_some(AudioBindingForwardedWork {
                issuer: self.issuer,
                canonical_units: units - extra,
                additional_units: extra,
                controller_debited: P::METERED,
            }),
            metered: P::METERED,
            charge_transferred: false,
        }
    }
    pub(crate) fn finish_buffer<T>(
        self,
        value: Vec<T>,
        buffer: Option<P::Lease>,
    ) -> AudioBindingValue<Vec<T>, P::Issuer, P::Lease> {
        let mut result = self.finish(value);
        result.buffer = buffer;
        result
    }
    pub(crate) fn merge_value<T>(
        &mut self,
        value: &mut AudioBindingValue<T, P::Issuer, P::Lease>,
    ) -> BuildResult<(), P::Error> {
        value.validate(self.policy)?;
        if let Some(mut lease) = value.lease.take() {
            if let Some(target) = &mut self.charge {
                self.policy.merge(target, &mut lease);
            } else {
                self.charge = Some(lease);
            }
            self.validate()?;
        }
        Ok(())
    }
}
pub(crate) enum ScratchSet<'a, I, L> {
    Legacy(BTreeSet<&'a NodeId>),
    Admitted {
        values: Vec<&'a NodeId>,
        issuer: I,
        charge: Option<L>,
        capacity: usize,
    },
}
impl<'a, I: Copy + Eq, L> ScratchSet<'a, I, L> {
    pub(crate) fn new<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        b: &Build<'_, P>,
        capacity: usize,
    ) -> Self {
        if P::METERED {
            Self::Admitted {
                values: Vec::new(),
                issuer: b.issuer,
                charge: None,
                capacity,
            }
        } else {
            Self::Legacy(BTreeSet::new())
        }
    }
    pub(crate) fn extend_capacity(&mut self, n: usize) {
        if let Self::Admitted { capacity, .. } = self {
            *capacity = n;
        }
    }
    fn position<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        &self,
        value: &NodeId,
        b: &mut Build<'_, P>,
    ) -> BuildResult<Result<usize, usize>, P::Error> {
        let Self::Admitted { values, issuer, .. } = self else {
            unreachable!()
        };
        b.validate()?;
        if *issuer != b.issuer {
            return Err(AudioBindingBuildError::Policy(b.policy.issuer_mismatch()));
        }
        let (mut left, mut right) = (0, values.len());
        while left < right {
            b.additional(1)?;
            let at = left + (right - left) / 2;
            match values[at].cmp(value) {
                std::cmp::Ordering::Less => left = at + 1,
                std::cmp::Ordering::Greater => right = at,
                std::cmp::Ordering::Equal => return Ok(Ok(at)),
            }
        }
        Ok(Err(left))
    }
    pub(crate) fn contains<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        &self,
        v: &NodeId,
        b: &mut Build<'_, P>,
    ) -> BuildResult<bool, P::Error> {
        match self {
            Self::Legacy(values) => Ok(values.contains(v)),
            _ => Ok(self.position(v, b)?.is_ok()),
        }
    }
    pub(crate) fn insert<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        &mut self,
        v: &'a NodeId,
        b: &mut Build<'_, P>,
    ) -> BuildResult<bool, P::Error> {
        if let Self::Legacy(values) = self {
            return Ok(values.insert(v));
        }
        let at = match self.position(v, b)? {
            Ok(_) => return Ok(false),
            Err(at) => at,
        };
        let Self::Admitted {
            values,
            charge,
            capacity,
            ..
        } = self
        else {
            unreachable!()
        };
        if values.len() == values.capacity() {
            if *capacity <= values.len() {
                return Err(limit("binding set capacity").into());
            }
            let mut storage = b.fork();
            storage.array::<&NodeId>(*capacity)?;
            let mut grown = Vec::with_capacity(*capacity);
            assert_eq!(grown.capacity(), *capacity);
            storage.additional(values.len())?;
            grown.append(values);
            let old = std::mem::replace(values, grown);
            drop(old);
            drop(charge.take());
            let mut guard = storage.finish(());
            *charge = guard.lease.take();
        }
        b.additional(values.len() - at + 1)?;
        values.insert(at, v);
        Ok(true)
    }
    pub(crate) fn values(&self) -> impl Iterator<Item = &'a NodeId> {
        enum Iter<'b, 'a> {
            Legacy(std::collections::btree_set::Iter<'b, &'a NodeId>),
            Admitted(std::slice::Iter<'b, &'a NodeId>),
        }
        impl<'a> Iterator for Iter<'_, 'a> {
            type Item = &'a NodeId;
            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    Self::Legacy(v) => v.next().copied(),
                    Self::Admitted(v) => v.next().copied(),
                }
            }
        }
        match self {
            Self::Legacy(v) => Iter::Legacy(v.iter()),
            Self::Admitted { values, .. } => Iter::Admitted(values.iter()),
        }
    }
}

/// Incremental buffer: old+new allocation coexist before the old lease releases.
pub(crate) struct Growing<T, I, L> {
    pub(crate) values: Vec<T>,
    issuer: Option<I>,
    buffer: Option<L>,
}
impl<T, I: Copy + Eq, L> Growing<T, I, L> {
    pub(crate) fn new() -> Self {
        Self {
            values: Vec::new(),
            issuer: None,
            buffer: None,
        }
    }
    pub(crate) fn push<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        &mut self,
        value: T,
        build: &mut Build<'_, P>,
    ) -> BuildResult<(), P::Error> {
        if !P::METERED {
            self.values.push(value);
            return Ok(());
        }
        if self.issuer.is_some_and(|v| v != build.issuer) {
            return Err(AudioBindingBuildError::Policy(
                build.policy.issuer_mismatch(),
            ));
        }
        if self.values.len() == self.values.capacity() {
            let capacity = self.values.capacity().saturating_mul(2).max(4);
            let mut storage = build.fork();
            storage.array::<T>(capacity)?;
            let mut grown = Vec::with_capacity(capacity);
            assert_eq!(grown.capacity(), capacity);
            storage.additional(self.values.len())?;
            grown.append(&mut self.values);
            let old = std::mem::replace(&mut self.values, grown);
            drop(old);
            drop(self.buffer.take());
            let mut guard = storage.finish(());
            self.buffer = guard.lease.take();
            self.issuer = Some(guard.issuer);
        }
        self.values.push(value);
        Ok(())
    }
    pub(crate) fn finish<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        mut self,
        build: &mut Build<'_, P>,
    ) -> BuildResult<(Vec<T>, Option<L>), P::Error> {
        build.validate()?;
        if self.issuer.is_some_and(|v| v != build.issuer) {
            return Err(AudioBindingBuildError::Policy(
                build.policy.issuer_mismatch(),
            ));
        }
        Ok((std::mem::take(&mut self.values), self.buffer.take()))
    }
}

#[cfg(test)]
mod tests;
