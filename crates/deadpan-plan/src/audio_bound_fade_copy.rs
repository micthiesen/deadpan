//! Finite immutable Bound copies for the actual retained-fade constructor.
use super::*;
use crate::plan::audio::capture::storage;
use deadpan_core::{AudioClockRoot, IterationId, ResolvedAudioPlacement, ResolvedAudioResume};

type CopyResult<'plan, A> = Result<
    Owned<
        AudioBound<'plan>,
        <A as AudioSoundContextAdmission>::Issuer,
        <A as AudioSoundContextAdmission>::Lease,
    >,
    A,
>;

struct CopyFields {
    definition: Option<AudioDefinitionSelector>,
    instance: Option<InstancePath>,
    gap: Option<DomainGap>,
    clock: Option<AudioClockRoot>,
    lattice_instance: Option<InstancePath>,
    lattice_gap_after: Option<IterationId>,
    constraints: Vec<EnvelopeConstraint>,
}

impl<'plan> AudioBound<'plan> {
    pub(in crate::plan) fn copy_for_retained_fade_query<A: AudioSoundContextAdmission>(
        &self,
        budget: &mut QueryBudget<'_, A>,
    ) -> CopyResult<'plan, A> {
        if !A::METERED {
            return Ok(Owned {
                value: self.clone(),
                memory: budget.memory(),
            });
        }
        let Self {
            plan,
            node,
            definition,
            instance,
            gap,
            resolved,
            reference_at_anchor,
            reference_step,
            support,
            envelope_scale,
            constraints,
        } = self;
        let ResolvedAudioBinding {
            lattice,
            resume,
            work,
        } = resolved;
        let ResolvedAudioPlacement {
            clock,
            grid_rule,
            grid_origin,
            frames_per_sample,
            origin,
            frames_per_local_frame,
            local_duration,
            local_support,
            instance: lattice_instance,
            gap_after,
            birth,
            gap_birth,
            work: lattice_work,
        } = lattice;
        budget.additional(1)?;
        let mut copy = Owned {
            value: CopyFields {
                definition: None,
                instance: None,
                gap: None,
                clock: None,
                lattice_instance: None,
                lattice_gap_after: None,
                constraints: Vec::new(),
            },
            memory: budget.memory(),
        };
        copy.value.definition = budget.definition(&mut copy.memory, definition)?;
        copy.value.instance = Some(budget.instance_fixed(&mut copy.memory, instance)?);
        copy.value.gap = budget.gap(&mut copy.memory, gap)?;
        copy.value.clock = Some(match clock {
            AudioClockRoot::ProjectRootRoundEven => AudioClockRoot::ProjectRootRoundEven,
            AudioClockRoot::PreserveInputPointCeil { stage } => {
                AudioClockRoot::PreserveInputPointCeil {
                    stage: budget.node(&mut copy.memory, stage)?,
                }
            }
            AudioClockRoot::DefinitionPointCeil { root } => AudioClockRoot::DefinitionPointCeil {
                root: budget.node(&mut copy.memory, root)?,
            },
            AudioClockRoot::GapDefinitionPointCeil { repeat } => {
                AudioClockRoot::GapDefinitionPointCeil {
                    repeat: budget.node(&mut copy.memory, repeat)?,
                }
            }
        });
        copy.value.lattice_instance =
            Some(budget.instance_fixed(&mut copy.memory, lattice_instance)?);
        copy.value.lattice_gap_after = gap_after
            .as_ref()
            .map(|value| budget.iteration(&mut copy.memory, value))
            .transpose()?;
        copy.value.constraints = storage::fixed(
            constraints,
            &mut copy.memory,
            budget,
            |value, memory, budget| budget.constraint(memory, value),
        )?;
        let resume = resume.as_ref().map(
            |ResolvedAudioResume {
                 local_boundary,
                 reference_local_delta,
             }| {
                ResolvedAudioResume {
                    local_boundary: *local_boundary,
                    reference_local_delta: *reference_local_delta,
                }
            },
        );
        let value = AudioBound {
            plan,
            node: *node,
            definition: copy.value.definition,
            instance: copy.value.instance.expect("copied Bound instance"),
            gap: copy.value.gap,
            resolved: ResolvedAudioBinding {
                lattice: ResolvedAudioPlacement {
                    clock: copy.value.clock.expect("copied Bound clock"),
                    grid_rule: *grid_rule,
                    grid_origin: *grid_origin,
                    frames_per_sample: *frames_per_sample,
                    origin: *origin,
                    frames_per_local_frame: *frames_per_local_frame,
                    local_duration: *local_duration,
                    local_support: local_support.clone(),
                    instance: copy
                        .value
                        .lattice_instance
                        .expect("copied lattice instance"),
                    gap_after: copy.value.lattice_gap_after,
                    birth: *birth,
                    gap_birth: *gap_birth,
                    work: *lattice_work,
                },
                resume,
                work: *work,
            },
            reference_at_anchor: *reference_at_anchor,
            reference_step: *reference_step,
            support: support.clone(),
            envelope_scale: *envelope_scale,
            constraints: copy.value.constraints,
        };
        let copy = Owned {
            value,
            memory: copy.memory,
        };
        budget.check_memory(&copy.memory)?;
        Ok(copy)
    }
}
