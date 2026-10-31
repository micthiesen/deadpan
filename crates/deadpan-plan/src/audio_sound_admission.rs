//! Allocation-site admission for current-sound contexts and Root/Domain
//! flattened, processing, recursive policy and creative fade queries, including cold Original preparation.
//! Standalone retained creative fades share this admission; recursive signal and
//! raw-domain preparation remain separate.

use super::*;
use crate::audio_allocated::admission::{arc, array, identity, reserve, validate};
use std::alloc::Layout;

/// Trusted accounting policy. Lease Drop must be independent of this mutable
/// policy, allocation-free, and release only its own issuer's live charge.
/// This is accounting infrastructure, not a media or ancestry certificate.
pub trait AudioSoundContextAdmission {
    /// False only for compatibility entrypoints: no new resource inspections.
    const METERED: bool = true;
    type Error: From<PlanError>;
    type Issuer: Copy + Eq;
    type Lease;

    fn issuer(&self) -> Self::Issuer;
    fn spend(&mut self, work: usize) -> Result<(), Self::Error>;
    fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, Self::Error>;
    /// Validated same-issuer leases only. Infallible, non-unwinding, no allocation.
    /// Disarm source without releasing its charge; retain both amounts in target.
    fn merge(&mut self, target: &mut Self::Lease, source: &mut Self::Lease);
}

pub(in crate::plan) struct Unmetered;
impl AudioSoundContextAdmission for Unmetered {
    const METERED: bool = false;
    type Error = PlanError;
    type Issuer = ();
    type Lease = ();
    fn issuer(&self) {}
    fn spend(&mut self, _: usize) -> Result<(), PlanError> {
        Ok(())
    }
    fn reserve(&mut self, _: Layout) -> Result<(), PlanError> {
        Ok(())
    }
    fn merge(&mut self, _: &mut (), _: &mut ()) {}
}

#[derive(Default)]
pub(in crate::plan) struct Values<'plan> {
    pub(in crate::plan) chain: Vec<usize>,
    pub(in crate::plan) edges: Vec<Edge>,
    pub(in crate::plan) owner: Option<InstancePath>,
    pub(in crate::plan) processing: Option<InstancePath>,
    pub(in crate::plan) seed_repeats: Vec<RepeatInstance>,
    pub(in crate::plan) policy_instance: Option<InstancePath>,
    pub(in crate::plan) domain: Option<AudioDomain<'plan>>,
    pub(in crate::plan) policy: Option<Arc<AudioDomain<'plan>>>,
    pub(in crate::plan) context: Option<AudioSoundOwnerContext<'plan>>,
    shared: Option<Arc<AudioSoundOwnerContext<'plan>>>,
    fallback: Option<Arc<AudioDomain<'plan>>>,
}
struct Charges<L> {
    chain: Option<L>,
    edges: Option<L>,
    retained: Option<L>,
}

/// Owns every intermediate value BEFORE its leases, including failed transitions.
/// Its issuer is an opaque policy identity, never content or Plan equality.
pub struct AdmittedSoundContext<'plan, I, L> {
    values: Values<'plan>,
    issuer: I,
    remaining: usize,
    charge: Option<L>,
}
pub(in crate::plan) struct SoundBuild<'a, 'plan, A: AudioSoundContextAdmission> {
    pub(in crate::plan) values: Values<'plan>,
    charges: Charges<A::Lease>,
    issuer: A::Issuer,
    budget: BuildBudget,
    admission: &'a mut A,
}
fn spend<A: AudioSoundContextAdmission>(
    budget: &mut BuildBudget,
    admission: &mut A,
    issuer: A::Issuer,
    count: usize,
    extra: bool,
) -> Result<(), A::Error> {
    validate(issuer, admission)?;
    if !extra || A::METERED {
        budget.spend(count)?;
        if A::METERED {
            admission.spend(count)?;
        }
        validate(issuer, admission)?;
    }
    Ok(())
}
fn copy_repeats<A: AudioSoundContextAdmission>(
    source: &[RepeatInstance],
    charges: &mut Option<A::Lease>,
    budget: &mut BuildBudget,
    admission: &mut A,
    issuer: A::Issuer,
    copy_paid: bool,
) -> Result<Vec<RepeatInstance>, A::Error> {
    reserve(
        charges,
        admission,
        issuer,
        array::<RepeatInstance>(source.len())?,
    )?;
    // Rust 1.97.1 Global/RawVec report exactly this requested logical capacity.
    let mut result = Vec::with_capacity(source.len());
    assert_eq!(result.capacity(), source.len());
    for value in source {
        if !copy_paid {
            spend(budget, admission, issuer, 1, true)?;
        }
        if A::METERED {
            spend(budget, admission, issuer, 1, true)?;
            reserve(charges, admission, issuer, identity(value.node.as_str())?)?;
            spend(budget, admission, issuer, 1, true)?;
            reserve(
                charges,
                admission,
                issuer,
                identity(value.iteration.allocation.as_str())?,
            )?;
        }
        result.push(value.clone());
    }
    Ok(result)
}
fn copy_instance<A: AudioSoundContextAdmission>(
    node: &NodeId,
    repeats: &[RepeatInstance],
    charges: &mut Option<A::Lease>,
    budget: &mut BuildBudget,
    admission: &mut A,
    issuer: A::Issuer,
    copy_paid: bool,
) -> Result<InstancePath, A::Error> {
    if A::METERED {
        spend(budget, admission, issuer, 1, true)?;
        reserve(charges, admission, issuer, identity(node.as_str())?)?;
    }
    Ok(InstancePath {
        node: node.clone(),
        repeats: copy_repeats(repeats, charges, budget, admission, issuer, copy_paid)?,
    })
}
impl<'a, 'plan, A: AudioSoundContextAdmission> SoundBuild<'a, 'plan, A> {
    pub(in crate::plan) fn new(admission: &'a mut A, remaining: usize) -> Self {
        Self {
            values: Values::default(),
            charges: Charges {
                chain: None,
                edges: None,
                retained: None,
            },
            issuer: admission.issuer(),
            budget: BuildBudget {
                maximum: remaining,
                remaining,
                maximum_runs: 1,
                runs: 0,
            },
            admission,
        }
    }
    pub(in crate::plan) fn remaining(&self) -> usize {
        self.budget.remaining
    }
    pub(in crate::plan) fn used(&self) -> usize {
        self.budget.maximum - self.budget.remaining
    }
    pub(in crate::plan) fn spend(&mut self, amount: usize) -> Result<(), A::Error> {
        spend(&mut self.budget, self.admission, self.issuer, amount, false)
    }
    pub(in crate::plan) fn reserve_chain(&mut self, capacity: usize) -> Result<(), A::Error> {
        reserve(
            &mut self.charges.chain,
            self.admission,
            self.issuer,
            array::<usize>(capacity)?,
        )?;
        self.values.chain = Vec::with_capacity(capacity);
        assert_eq!(self.values.chain.capacity(), capacity);
        Ok(())
    }
    pub(in crate::plan) fn reserve_edges(&mut self, count: usize) -> Result<(), A::Error> {
        reserve(
            &mut self.charges.edges,
            self.admission,
            self.issuer,
            array::<Edge>(count)?,
        )?;
        self.values.edges = Vec::with_capacity(count);
        assert_eq!(self.values.edges.capacity(), count);
        Ok(())
    }
    pub(in crate::plan) fn drop_chain(&mut self) {
        drop(std::mem::take(&mut self.values.chain));
        drop(self.charges.chain.take());
    }
    pub(in crate::plan) fn drop_edges(&mut self) {
        drop(std::mem::take(&mut self.values.edges));
        drop(self.charges.edges.take());
    }
    pub(in crate::plan) fn copy_initial_paths(
        &mut self,
        owner: &InstancePath,
        processing: &NodeId,
        count: usize,
    ) -> Result<(), A::Error> {
        let prefix = &owner.repeats[..count];
        self.values.processing = Some(copy_instance(
            processing,
            prefix,
            &mut self.charges.retained,
            &mut self.budget,
            self.admission,
            self.issuer,
            true,
        )?);
        self.values.seed_repeats = copy_repeats(
            prefix,
            &mut self.charges.retained,
            &mut self.budget,
            self.admission,
            self.issuer,
            false,
        )?;
        self.values.policy_instance = Some(copy_instance(
            processing,
            prefix,
            &mut self.charges.retained,
            &mut self.budget,
            self.admission,
            self.issuer,
            false,
        )?);
        self.values.owner = Some(copy_instance(
            &owner.node,
            &owner.repeats,
            &mut self.charges.retained,
            &mut self.budget,
            self.admission,
            self.issuer,
            true,
        )?);
        Ok(())
    }
    pub(in crate::plan) fn reserve_domain(&mut self) -> Result<(), A::Error> {
        reserve(
            &mut self.charges.retained,
            self.admission,
            self.issuer,
            arc::<AudioDomain<'plan>>()?,
        )
    }
    pub(in crate::plan) fn finish(mut self) -> AdmittedSoundContext<'plan, A::Issuer, A::Lease> {
        AdmittedSoundContext {
            values: std::mem::take(&mut self.values),
            issuer: self.issuer,
            remaining: self.budget.remaining,
            charge: self.charges.retained.take(),
        }
    }
}
impl<'plan, I: Copy + Eq, L> AdmittedSoundContext<'plan, I, L> {
    pub fn issuer(&self) -> I {
        self.issuer
    }
    /// Borrow the live charge; ownership remains paired with this carrier.
    pub fn lease(&self) -> Option<&L> {
        self.charge.as_ref()
    }
    pub(crate) fn lease_mut(&mut self) -> Option<&mut L> {
        self.charge.as_mut()
    }
    pub fn context(&self) -> &AudioSoundOwnerContext<'plan> {
        self.values
            .shared
            .as_deref()
            .or(self.values.context.as_ref())
            .expect("context carrier")
    }
    pub fn shared(&self) -> &Arc<AudioSoundOwnerContext<'plan>> {
        self.values.shared.as_ref().expect("shared context carrier")
    }
    pub fn processing_domain(&self) -> Option<&Arc<AudioDomain<'plan>>> {
        self.values.fallback.as_ref()
    }
    fn resume<'a, A>(mut self, admission: &'a mut A) -> Result<SoundBuild<'a, 'plan, A>, A::Error>
    where
        A: AudioSoundContextAdmission<Issuer = I, Lease = L>,
    {
        validate(self.issuer, admission)?;
        let mut build = SoundBuild::new(admission, self.remaining);
        build.values = std::mem::take(&mut self.values);
        build.charges.retained = self.charge.take();
        Ok(build)
    }
    pub fn into_shared<A>(self, admission: &mut A) -> Result<Self, A::Error>
    where
        A: AudioSoundContextAdmission<Issuer = I, Lease = L>,
    {
        let mut build = self.resume(admission)?;
        if build.values.shared.is_none() {
            spend(&mut build.budget, build.admission, build.issuer, 1, true)?;
            reserve(
                &mut build.charges.retained,
                build.admission,
                build.issuer,
                arc::<AudioSoundOwnerContext<'plan>>()?,
            )?;
            let value = build.values.context.take().expect("bare context carrier");
            build.values.shared = Some(Arc::new(value));
        }
        Ok(build.finish())
    }
    /// Clone only the constructor's checked closed current processing Domain.
    pub fn with_processing_domain<A>(self, admission: &mut A) -> Result<Self, A::Error>
    where
        A: AudioSoundContextAdmission<Issuer = I, Lease = L>,
    {
        let mut build = self.into_shared(admission)?.resume(admission)?;
        if build.values.fallback.is_some() {
            return Ok(build.finish());
        }
        let sound = build
            .values
            .shared
            .as_ref()
            .expect("shared context carrier");
        let AudioDomain {
            plan,
            seed,
            samples,
            visible,
            instance,
            placement,
        } = sound.processing_domain();
        let AudioWalkSeed {
            definition,
            bypass_binding,
            node,
            transform,
            grid,
            extent,
            envelope,
            constraints,
            editorial,
            suppress_entry_editorial,
            repeats,
            retimes,
            gap,
        } = seed;
        macro_rules! check {
            ($test:expr) => {{
                spend(&mut build.budget, build.admission, build.issuer, 1, true)?;
                if !($test) {
                    return Err(PlanError::AudioSoundContextShapeChanged.into());
                }
            }};
        }
        check!(definition.is_none());
        check!(bypass_binding.is_none());
        check!(envelope.is_none());
        check!(constraints.is_empty());
        check!(editorial.is_empty());
        check!(retimes.is_empty());
        check!(gap.is_none());
        check!(placement.is_none());
        check!(*suppress_entry_editorial == deadpan_core::AudioEditorialEdges::default());
        check!(*node == sound.processing_node);
        spend(
            &mut build.budget,
            build.admission,
            build.issuer,
            repeats.len() + instance.repeats.len(),
            true,
        )?;
        check!(repeats == &sound.processing_instance.repeats);
        check!(instance == &sound.processing_instance);
        // Only scalar recipe fields are copied before the admitted path copies.
        let plan = *plan;
        let node = *node;
        let transform = *transform;
        let grid = *grid;
        let extent = extent.clone();
        let samples = samples.clone();
        let visible = visible.clone();
        build.values.seed_repeats = copy_repeats(
            repeats,
            &mut build.charges.retained,
            &mut build.budget,
            build.admission,
            build.issuer,
            false,
        )?;
        build.values.policy_instance = Some(copy_instance(
            &instance.node,
            &instance.repeats,
            &mut build.charges.retained,
            &mut build.budget,
            build.admission,
            build.issuer,
            false,
        )?);
        build.values.domain = Some(AudioDomain {
            plan,
            seed: AudioWalkSeed {
                definition: None,
                bypass_binding: None,
                node,
                transform,
                grid,
                extent,
                envelope: None,
                constraints: Vec::new(),
                editorial: Vec::new(),
                suppress_entry_editorial: Default::default(),
                repeats: std::mem::take(&mut build.values.seed_repeats),
                retimes: Vec::new(),
                gap: None,
            },
            samples,
            visible,
            instance: build
                .values
                .policy_instance
                .take()
                .expect("domain instance"),
            placement: None,
        });
        build.reserve_domain()?;
        build.values.fallback = Some(Arc::new(build.values.domain.take().expect("domain")));
        Ok(build.finish())
    }
    /// Private synchronous consumers must publish owning clones before disarming.
    /// The callback must be infallible, non-unwinding and allocate nothing.
    pub fn transfer_charge(
        &mut self,
        issuer: I,
        transfer: impl FnOnce(Option<&mut L>),
    ) -> Result<(), PlanError> {
        if self.issuer != issuer {
            return Err(PlanError::AudioMetadataIssuerMismatch);
        }
        transfer(self.charge.as_mut());
        Ok(())
    }
}
impl<'plan> AdmittedSoundContext<'plan, (), ()> {
    pub(in crate::plan) fn into_unmetered(mut self) -> AudioSoundOwnerContext<'plan> {
        self.values
            .context
            .take()
            .expect("bare compatibility context")
    }
}

#[cfg(test)]
#[path = "audio_sound_admission_tests.rs"]
mod tests;
