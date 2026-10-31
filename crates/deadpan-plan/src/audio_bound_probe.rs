//! Actual raw-domain construction beneath the one complete retained probe.
//! Canonical work is zero here; new admitted inspections share its remainder.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::audio_allocated::admission::array;
use crate::plan::audio_owners::admission::{Budget as QueryBudget, Owned, Result, outer};

type OwnedResult<T, A> = Result<
    Owned<T, <A as AudioSoundContextAdmission>::Issuer, <A as AudioSoundContextAdmission>::Lease>,
    A,
>;
type RawResult<'plan, A> = OwnedResult<AudioBoundDomain<'plan>, A>;

enum RawDomainValue<'plan, I, L> {
    Ordinary(AudioBoundDomain<'plan>),
    Admitted(Owned<AudioBoundDomain<'plan>, I, L>),
}

/// A fresh raw-domain result with its independent construction lease.
/// This carrier has no Clone or detach operation; borrow domain() while it lives.
pub struct AdmittedAudioBoundDomain<'plan, I, L> {
    value: RawDomainValue<'plan, I, L>,
    canonical: usize,
    additional: usize,
    forwarded: bool,
}

impl<'plan, I, L> AdmittedAudioBoundDomain<'plan, I, L> {
    pub fn domain(&self) -> &AudioBoundDomain<'plan> {
        match &self.value {
            RawDomainValue::Ordinary(value) => value,
            RawDomainValue::Admitted(value) => &value.value,
        }
    }
    pub fn canonical_units(&self) -> usize {
        self.canonical
    }
    pub fn additional_units(&self) -> usize {
        self.additional
    }
    pub fn total_units(&self) -> usize {
        self.canonical + self.additional
    }
    pub fn was_forwarded(&self) -> bool {
        self.forwarded
    }
}

fn repeats<A: AudioSoundContextAdmission>(
    values: &[RepeatInstance],
    budget: &mut QueryBudget<'_, A>,
) -> OwnedResult<Vec<RepeatInstance>, A> {
    let mut result = Owned {
        value: Vec::new(),
        memory: budget.memory(),
    };
    if !A::METERED {
        result.value = values.to_vec();
        return Ok(result);
    }
    budget.reserve(&mut result.memory, array::<RepeatInstance>(values.len())?)?;
    result.value = Vec::with_capacity(values.len());
    assert_eq!(result.value.capacity(), values.len());
    for value in values {
        budget.additional(1)?;
        result.value.push(budget.repeat(&mut result.memory, value)?);
    }
    Ok(result)
}

impl<'plan> AudioBound<'plan> {
    pub(in crate::plan) fn raw_domain_admitted<A: AudioSoundContextAdmission>(
        &self,
        budget: &mut QueryBudget<'_, A>,
    ) -> RawResult<'plan, A> {
        let lattice = &self.resolved.lattice;
        let support = self.support.clone();
        if !support
            .end
            .checked_sub(support.start)?
            .compare_integer(0)
            .is_gt()
        {
            return Ok(Owned {
                value: AudioBoundDomain::Empty,
                memory: budget.memory(),
            });
        }
        // Preserve definition_handle's gap-before-selector owning copy order.
        // This temporary definition dies before the returned domain's charge.
        let mut definition = Owned {
            value: None,
            memory: budget.memory(),
        };
        let gap = budget.gap(&mut definition.memory, &self.gap)?;
        let node = budget.node(&mut definition.memory, &self.instance.node)?;
        definition.value = Some(AudioDefinition {
            plan: self.plan,
            root: self.node,
            gap,
            selector: if self.gap.is_some() {
                AudioDefinitionSelector::RepeatGap { repeat: node }
            } else {
                AudioDefinitionSelector::Node { node }
            },
        });
        let definition_value = definition.value.as_ref().expect("raw definition");
        let placement =
            AudioRootPlacement::new(lattice.origin, lattice.frames_per_local_frame, support)?;
        // These exact physical checks were in both actual in_*_clock calls.
        definition_value.validate_physical_placement(&placement)?;
        #[cfg(test)]
        phase_trace::temporary_definition::<A>(definition_value);
        match lattice.grid_rule {
            AudioBindingGridRule::RootRoundEven => {
                let mut domain = definition_value.root_domain_on_grid_query(
                    placement,
                    ExactRatio::ZERO,
                    crate::AudioBoundaryRule::RoundEven,
                    budget,
                )?;
                #[cfg(test)]
                if A::METERED {
                    phase_trace::root_initial(&domain.value, definition_value);
                }
                if !A::METERED {
                    self.set_domain_evaluation(&mut domain.value);
                } else {
                    self.set_root_evaluation_admitted(&mut domain, budget, definition_value)?;
                }
                Ok(Owned {
                    value: AudioBoundDomain::Root(domain.value),
                    memory: domain.memory,
                })
            }
            AudioBindingGridRule::PointCeil => {
                let mut domain = AudioPointDomain::new_admitted(
                    definition_value,
                    placement,
                    lattice.grid_origin,
                    budget,
                )?;
                #[cfg(test)]
                if A::METERED {
                    phase_trace::point_initial(&domain.value, definition_value);
                }
                // The old call evaluates both copies before replacing either
                // embedded Signal field. Keep the old/new values overlapped.
                let mut evaluation = Owned {
                    value: None,
                    memory: budget.memory(),
                };
                let selector = budget.definition(&mut evaluation.memory, &self.definition)?;
                let mut copied_repeats = repeats(&self.instance.repeats, budget)?;
                #[cfg(test)]
                phase_trace::point_evaluation(
                    &domain.value,
                    definition_value,
                    &selector,
                    &copied_repeats.value,
                );
                budget.check_memory(&evaluation.memory)?;
                budget.check_memory(&copied_repeats.memory)?;
                budget.merge_checked(&mut evaluation.memory, &mut copied_repeats.memory)?;
                evaluation.value = Some((selector, copied_repeats.value));
                budget.check_memory(&domain.memory)?;
                budget.check_memory(&evaluation.memory)?;
                let (selector, copied_repeats) =
                    evaluation.value.take().expect("copied evaluation");
                // No fallible operation between charge merge and owning move.
                budget.merge_checked(&mut domain.memory, &mut evaluation.memory)?;
                domain
                    .value
                    .set_evaluation(selector, copied_repeats, Some(self.binding_target()));
                Ok(Owned {
                    value: AudioBoundDomain::Point(domain.value),
                    memory: domain.memory,
                })
            }
        }
    }

    /// Rebuild the actual raw domain under this caller's admission policy.
    /// The historical fixed local allowance remains 65,536; Bound/Core work
    /// is not forwarded a second time.
    pub fn raw_domain_query<A: AudioSoundContextAdmission>(
        &self,
        admission: &mut A,
    ) -> std::result::Result<AdmittedAudioBoundDomain<'plan, A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            return Ok(AdmittedAudioBoundDomain {
                value: RawDomainValue::Ordinary(self.raw_domain()?),
                canonical: 0,
                additional: 0,
                forwarded: false,
            });
        }

        let mut local = Budget {
            remaining: 65_536,
            lookup: crate::LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        let value = self.raw_domain_admitted(&mut budget);
        let canonical = budget.canonical_used;
        let additional = budget.additional_used;
        let value = outer::<_, A>(value)?;
        outer::<_, A>(budget.check_memory(&value.memory))?;
        outer::<_, A>(budget.check())?;
        if canonical != 0 {
            return Err(
                PlanError::InvalidPlan("raw-domain canonical work must remain zero").into(),
            );
        }
        Ok(AdmittedAudioBoundDomain {
            value: RawDomainValue::Admitted(value),
            canonical,
            additional,
            forwarded: true,
        })
    }

    fn set_root_evaluation_admitted<A: AudioSoundContextAdmission>(
        &self,
        domain: &mut Owned<AudioDomain<'plan>, A::Issuer, A::Lease>,
        budget: &mut QueryBudget<'_, A>,
        _definition: &AudioDefinition<'plan>,
    ) -> Result<(), A> {
        // Each real replacement is charged while its old value is still live.
        // Cleared owning edges remain a conservative per-domain bound until
        // this returned domain drops, not a dead walker/workspace charge.
        let next_definition = budget.definition(&mut domain.memory, &self.definition)?;
        #[cfg(test)]
        phase_trace::root_selector(&domain.value, next_definition.as_ref(), _definition);
        domain.value.seed.definition = next_definition;
        domain.value.seed.bypass_binding = Some(self.binding_target());
        let mut copied_repeats = repeats(&self.instance.repeats, budget)?;
        #[cfg(test)]
        phase_trace::root_repeats(&domain.value, &copied_repeats.value, _definition);
        budget.check_memory(&domain.memory)?;
        budget.check_memory(&copied_repeats.memory)?;
        budget.merge_checked(&mut domain.memory, &mut copied_repeats.memory)?;
        domain.value.seed.repeats = copied_repeats.value;
        if !self.constraints.is_empty() {
            let count = domain
                .value
                .seed
                .constraints
                .len()
                .checked_add(self.constraints.len())
                .ok_or(deadpan_core::TimeError::Overflow)?;
            let mut next = Owned {
                value: Vec::new(),
                memory: budget.memory(),
            };
            budget.reserve(&mut next.memory, array::<EnvelopeConstraint>(count)?)?;
            next.value = Vec::with_capacity(count);
            assert_eq!(next.value.capacity(), count);
            for value in domain.value.seed.constraints.drain(..) {
                budget.additional(1)?;
                next.value.push(value);
            }
            for value in &self.constraints {
                next.value.push(budget.constraint(&mut next.memory, value)?);
            }
            #[cfg(test)]
            phase_trace::root_constraints(&domain.value, &next.value, _definition);
            budget.check_memory(&domain.memory)?;
            budget.check_memory(&next.memory)?;
            budget.merge_checked(&mut domain.memory, &mut next.memory)?;
            domain.value.seed.constraints = next.value;
        }
        let mut instance = Owned {
            value: None,
            memory: budget.memory(),
        };
        instance.value = Some(budget.instance_fixed(&mut instance.memory, &self.instance)?);
        #[cfg(test)]
        phase_trace::root_instance(&domain.value, instance.value.as_ref().unwrap(), _definition);
        budget.check_memory(&domain.memory)?;
        budget.check_memory(&instance.memory)?;
        budget.merge_checked(&mut domain.memory, &mut instance.memory)?;
        domain.value.instance = instance.value.expect("copied instance");
        Ok(())
    }
}

#[cfg(test)]
#[path = "audio_bound_probe_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "audio_bound_probe_phase_trace_tests.rs"]
pub(in crate::plan) mod phase_trace;
