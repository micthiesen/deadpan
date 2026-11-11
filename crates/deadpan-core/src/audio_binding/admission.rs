//! Construction policy for actual binding queries, independent of media hosts.
use super::*;
use std::alloc::Layout;

pub(crate) trait AudioBindingAdmission {
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
pub(crate) enum AudioBindingBuildError<E> {
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
pub(crate) struct AudioBindingForwardedWork<I> {
    canonical_units: usize,
    additional_units: usize,
    issuer: std::marker::PhantomData<I>,
}
impl<I: Copy> AudioBindingForwardedWork<I> {
    pub(crate) fn units(&self) -> usize {
        self.canonical_units + self.additional_units
    }
}
/// Owned values drop before their independent resource leases.
pub(crate) struct AudioBindingValue<T, I, L> {
    pub(crate) value: Option<T>,
    issuer: I,
    work: Option<AudioBindingForwardedWork<I>>,
    metered: bool,
    lease: std::marker::PhantomData<L>,
}
impl<T, I: Copy + Eq, L> AudioBindingValue<T, I, L> {
    fn validate<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        &self,
        p: &P,
    ) -> BuildResult<(), P::Error> {
        if self.issuer != p.issuer() || self.metered != P::METERED {
            return Err(AudioBindingBuildError::Policy(p.issuer_mismatch()));
        }
        Ok(())
    }
    pub(crate) fn take_work<P: AudioBindingAdmission<Issuer = I, Lease = L>>(
        &mut self,
        p: &P,
    ) -> BuildResult<AudioBindingForwardedWork<I>, P::Error> {
        self.validate(p)?;
        self.work
            .take()
            .ok_or_else(|| invalid("binding receipt already consumed").into())
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
    pub(crate) fn finish<T>(mut self, value: T) -> AudioBindingValue<T, P::Issuer, P::Lease> {
        let units = self.work.used - self.before;
        let extra = *self.extra - self.extra_before;
        // The construction charge is released with this builder.
        drop(self.charge.take());
        AudioBindingValue {
            value: Some(value),
            issuer: self.issuer,
            work: self.receipt.then_some(AudioBindingForwardedWork {
                canonical_units: units - extra,
                additional_units: extra,
                issuer: std::marker::PhantomData,
            }),
            metered: P::METERED,
            lease: std::marker::PhantomData,
        }
    }
}
