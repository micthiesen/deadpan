//! Final Original sharing only. Source Input/query/capture/retained construction
//! remains outside these new allocation reservations.
use super::super::audio::{EditorialConstraint, EnvelopeConstraint};
use super::*;
use crate::audio_allocated::admission::{arc, array, identity, reserve, validate};
use crate::{AdmittedSoundContext, AudioOwnerRetainedInput, AudioSoundContextAdmission};
use deadpan_core::{NodeId, RevisionId};

#[derive(Default)]
struct Values<'plan> {
    definition: Option<AudioDefinitionSelector>,
    constraints: Vec<EnvelopeConstraint>,
    editorial: Vec<EditorialConstraint>,
    repeats: Vec<RepeatInstance>,
    retimes: Vec<AudioRetimeStage>,
    gap: Option<DomainGap>,
    seed: Option<AudioWalkSeed>,
    instance: Option<InstancePath>,
    domain: Option<AudioDomain<'plan>>,
    retained: Option<AudioOwnerRetainedInput<'plan>>,
    shared_domain: Option<std::sync::Arc<AudioDomain<'plan>>>,
    shared_retained: Option<std::sync::Arc<AudioOwnerRetainedInput<'plan>>>,
}
/// Owns final shared values before their charge. This is not an ancestry,
/// source-admission or whole-construction allocation certificate.
pub struct AdmittedOriginalContext<'plan, I, L> {
    values: Values<'plan>,
    issuer: I,
    charge: Option<L>,
}
struct Scope<'a, 'plan, A: AudioSoundContextAdmission> {
    values: Values<'plan>,
    issuer: A::Issuer,
    charge: Option<A::Lease>,
    remaining: usize,
    admission: &'a mut A,
}
macro_rules! inspected {
    ($scope:expr, $value:expr) => {{
        $scope.step()?;
        $value
    }};
}
impl<'a, 'plan, A: AudioSoundContextAdmission> Scope<'a, 'plan, A> {
    fn new(admission: &'a mut A, limits: AudioQueryLimits) -> Result<Self, A::Error> {
        limits.validate()?;
        Ok(Self {
            values: Values::default(),
            issuer: admission.issuer(),
            charge: None,
            remaining: limits.maximum_work,
            admission,
        })
    }
    fn step(&mut self) -> Result<(), A::Error> {
        validate(self.issuer, self.admission)?;
        if A::METERED {
            self.remaining = self
                .remaining
                .checked_sub(1)
                .ok_or(PlanError::AudioQueryLimit("Original sharing inspections"))?;
            self.admission.spend(1)?;
            validate(self.issuer, self.admission)?;
        }
        Ok(())
    }
    fn reserve(&mut self, layout: std::alloc::Layout) -> Result<(), A::Error> {
        reserve(&mut self.charge, self.admission, self.issuer, layout)
    }
    fn node_id(&mut self, id: &NodeId) -> Result<NodeId, A::Error> {
        self.step()?;
        if A::METERED {
            self.reserve(identity(id.as_str())?)?;
        }
        Ok(id.clone())
    }
    fn revision_id(&mut self, id: &RevisionId) -> Result<RevisionId, A::Error> {
        self.step()?;
        if A::METERED {
            self.reserve(identity(id.as_str())?)?;
        }
        Ok(id.clone())
    }
    fn iteration(&mut self, value: &IterationId) -> Result<IterationId, A::Error> {
        let IterationId {
            allocation,
            ordinal,
        } = value;
        Ok(IterationId {
            allocation: self.revision_id(allocation)?,
            ordinal: inspected!(self, *ordinal),
        })
    }
    fn optional_iteration(
        &mut self,
        value: &Option<IterationId>,
    ) -> Result<Option<IterationId>, A::Error> {
        self.step()?;
        value
            .as_ref()
            .map(|value| self.iteration(value))
            .transpose()
    }
    fn definition(
        &mut self,
        value: &Option<AudioDefinitionSelector>,
    ) -> Result<Option<AudioDefinitionSelector>, A::Error> {
        self.step()?;
        value
            .as_ref()
            .map(|value| {
                Ok(match value {
                    AudioDefinitionSelector::Node { node } => AudioDefinitionSelector::Node {
                        node: self.node_id(node)?,
                    },
                    AudioDefinitionSelector::RepeatDefault { repeat } => {
                        AudioDefinitionSelector::RepeatDefault {
                            repeat: self.node_id(repeat)?,
                        }
                    }
                    AudioDefinitionSelector::RepeatGap { repeat } => {
                        AudioDefinitionSelector::RepeatGap {
                            repeat: self.node_id(repeat)?,
                        }
                    }
                })
            })
            .transpose()
    }
    fn constraint(&mut self, value: &EnvelopeConstraint) -> Result<EnvelopeConstraint, A::Error> {
        let EnvelopeConstraint {
            placement_support,
            range,
            node,
            repeat_count,
            gap_after,
            kinds,
        } = value;
        Ok(EnvelopeConstraint {
            placement_support: inspected!(self, *placement_support),
            range: inspected!(self, range.clone()),
            node: inspected!(self, *node),
            repeat_count: inspected!(self, *repeat_count),
            gap_after: self.optional_iteration(gap_after)?,
            kinds: inspected!(self, *kinds),
        })
    }
    fn constraints(
        &mut self,
        values: &[EnvelopeConstraint],
    ) -> Result<Vec<EnvelopeConstraint>, A::Error> {
        self.step()?;
        self.reserve(array::<EnvelopeConstraint>(values.len())?)?;
        let mut result = Vec::with_capacity(values.len());
        assert_eq!(result.capacity(), values.len());
        for value in values {
            self.step()?;
            result.push(self.constraint(value)?);
        }
        Ok(result)
    }
    fn editorial(
        &mut self,
        values: &[EditorialConstraint],
    ) -> Result<Vec<EditorialConstraint>, A::Error> {
        self.step()?;
        self.reserve(array::<EditorialConstraint>(values.len())?)?;
        let mut result = Vec::with_capacity(values.len());
        assert_eq!(result.capacity(), values.len());
        for value in values {
            self.step()?;
            let EditorialConstraint {
                boundary,
                edges,
                incident_samples,
            } = value;
            self.step()?;
            result.push(EditorialConstraint {
                boundary: self.constraint(boundary)?,
                edges: inspected!(self, *edges),
                incident_samples: inspected!(self, *incident_samples),
            });
        }
        Ok(result)
    }
    fn repeats(&mut self, values: &[RepeatInstance]) -> Result<Vec<RepeatInstance>, A::Error> {
        self.step()?;
        self.reserve(array::<RepeatInstance>(values.len())?)?;
        let mut result = Vec::with_capacity(values.len());
        assert_eq!(result.capacity(), values.len());
        for value in values {
            self.step()?;
            let RepeatInstance { node, iteration } = value;
            result.push(RepeatInstance {
                node: self.node_id(node)?,
                iteration: self.iteration(iteration)?,
            });
        }
        Ok(result)
    }
    fn retimes(&mut self, values: &[AudioRetimeStage]) -> Result<Vec<AudioRetimeStage>, A::Error> {
        self.step()?;
        self.reserve(array::<AudioRetimeStage>(values.len())?)?;
        let mut result = Vec::with_capacity(values.len());
        assert_eq!(result.capacity(), values.len());
        for value in values {
            self.step()?;
            let AudioRetimeStage {
                node,
                child_start,
                child_frames_per_local_frame,
                pitch,
            } = value;
            result.push(AudioRetimeStage {
                node: self.node_id(node)?,
                child_start: inspected!(self, *child_start),
                child_frames_per_local_frame: inspected!(self, *child_frames_per_local_frame),
                pitch: inspected!(self, *pitch),
            });
        }
        Ok(result)
    }
    fn gap(&mut self, value: &Option<DomainGap>) -> Result<Option<DomainGap>, A::Error> {
        self.step()?;
        value
            .as_ref()
            .map(|value| {
                let DomainGap { after, duration } = value;
                Ok(DomainGap {
                    after: self.optional_iteration(after)?,
                    duration: inspected!(self, *duration),
                })
            })
            .transpose()
    }
    fn instance(&mut self, value: &InstancePath) -> Result<InstancePath, A::Error> {
        let InstancePath { node, repeats } = value;
        Ok(InstancePath {
            node: self.node_id(node)?,
            repeats: self.repeats(repeats)?,
        })
    }
    fn placement(
        &mut self,
        value: &Option<AudioRootPlacement>,
    ) -> Result<Option<AudioRootPlacement>, A::Error> {
        self.step()?;
        value
            .as_ref()
            .map(|value| {
                let AudioRootPlacement {
                    origin,
                    root_frames_per_local_frame,
                    local_support,
                } = value;
                Ok(AudioRootPlacement {
                    origin: inspected!(self, *origin),
                    root_frames_per_local_frame: inspected!(self, *root_frames_per_local_frame),
                    local_support: inspected!(self, local_support.clone()),
                })
            })
            .transpose()
    }
    fn seed(&mut self, value: &AudioWalkSeed) -> Result<AudioWalkSeed, A::Error> {
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
        } = value;
        self.values.definition = self.definition(definition)?;
        let bypass_binding = inspected!(self, *bypass_binding);
        let node = inspected!(self, *node);
        let transform = inspected!(self, *transform);
        let grid = inspected!(self, *grid);
        let extent = inspected!(self, extent.clone());
        let envelope = inspected!(self, envelope.clone());
        self.values.constraints = self.constraints(constraints)?;
        self.values.editorial = self.editorial(editorial)?;
        let suppress_entry_editorial = inspected!(self, *suppress_entry_editorial);
        self.values.repeats = self.repeats(repeats)?;
        self.values.retimes = self.retimes(retimes)?;
        self.values.gap = self.gap(gap)?;
        Ok(AudioWalkSeed {
            definition: self.values.definition.take(),
            bypass_binding,
            node,
            transform,
            grid,
            extent,
            envelope,
            constraints: std::mem::take(&mut self.values.constraints),
            editorial: std::mem::take(&mut self.values.editorial),
            suppress_entry_editorial,
            repeats: std::mem::take(&mut self.values.repeats),
            retimes: std::mem::take(&mut self.values.retimes),
            gap: self.values.gap.take(),
        })
    }
    fn finish(mut self) -> AdmittedOriginalContext<'plan, A::Issuer, A::Lease> {
        AdmittedOriginalContext {
            values: std::mem::take(&mut self.values),
            issuer: self.issuer,
            charge: self.charge.take(),
        }
    }
}
impl<'plan> AudioDomain<'plan> {
    /// Admit only this finite new copy and Arc. The borrowed source Domain,
    /// source Input and all earlier query/capture construction stay live.
    pub fn clone_shared_admitted<A: AudioSoundContextAdmission>(
        &self,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedOriginalContext<'plan, A::Issuer, A::Lease>, A::Error> {
        let mut scope = Scope::new(admission, limits)?;
        let Self {
            plan,
            seed,
            samples,
            visible,
            instance,
            placement,
        } = self;
        let plan = inspected!(scope, *plan);
        scope.step()?;
        scope.values.seed = Some(scope.seed(seed)?);
        let samples = inspected!(scope, samples.clone());
        let visible = inspected!(scope, visible.clone());
        scope.step()?;
        scope.values.instance = Some(scope.instance(instance)?);
        let placement = scope.placement(placement)?;
        scope.values.domain = Some(AudioDomain {
            plan,
            seed: scope.values.seed.take().expect("copied seed"),
            samples,
            visible,
            instance: scope.values.instance.take().expect("copied instance"),
            placement,
        });
        scope.reserve(arc::<AudioDomain<'plan>>()?)?;
        scope.values.shared_domain = Some(std::sync::Arc::new(
            scope.values.domain.take().expect("copied Domain"),
        ));
        Ok(scope.finish())
    }
}
impl<'plan> AudioOwnerRetainedInput<'plan> {
    /// Move the existing retained graph into a newly admitted Arc shell.
    /// This does not admit the already constructed processing/editorial/domain graph.
    pub fn into_shared_admitted<A: AudioSoundContextAdmission>(
        self,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedOriginalContext<'plan, A::Issuer, A::Lease>, A::Error> {
        let mut scope = Scope::new(admission, limits)?;
        scope.values.retained = Some(self);
        scope.step()?;
        scope.reserve(arc::<AudioOwnerRetainedInput<'plan>>()?)?;
        scope.values.shared_retained = Some(std::sync::Arc::new(
            scope.values.retained.take().expect("retained input"),
        ));
        Ok(scope.finish())
    }
}
impl<'plan, I: Copy + Eq, L> AdmittedOriginalContext<'plan, I, L> {
    pub fn issuer(&self) -> I {
        self.issuer
    }
    pub fn lease(&self) -> Option<&L> {
        self.charge.as_ref()
    }
    pub fn domain(&self) -> Option<&std::sync::Arc<AudioDomain<'plan>>> {
        self.values.shared_domain.as_ref()
    }
    pub fn retained(&self) -> Option<&std::sync::Arc<AudioOwnerRetainedInput<'plan>>> {
        self.values.shared_retained.as_ref()
    }
}
/// Validate ALL final-sharing issuers before invoking ONE infallible publication.
/// No allocation, callback or fallible step may intervene between publication
/// and the consumer's preflighted joint ledger commit. Owning values stay paired.
pub fn transfer_context_charges<'plan, I: Copy + Eq, L>(
    issuer: I,
    original: Option<&mut AdmittedOriginalContext<'plan, I, L>>,
    sound: Option<&mut AdmittedSoundContext<'plan, I, L>>,
    transfer: impl FnOnce(Option<&mut L>, Option<&mut L>),
) -> Result<(), PlanError> {
    if original
        .as_ref()
        .is_some_and(|value| value.issuer() != issuer)
        || sound.as_ref().is_some_and(|value| value.issuer() != issuer)
    {
        return Err(PlanError::AudioMetadataIssuerMismatch);
    }
    transfer(
        original.and_then(|value| value.charge.as_mut()),
        sound.and_then(AdmittedSoundContext::lease_mut),
    );
    Ok(())
}

#[cfg(test)]
#[path = "audio_context_sharing_tests.rs"]
mod tests;
