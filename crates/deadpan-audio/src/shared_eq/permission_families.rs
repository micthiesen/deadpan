//! Request-local canonical handles for complete, proved permission identities.
//! A handle grants only contribution aliasing. Source access, current support
//! and per-sample silence permissions remain with each original contribution.

use super::*;
use deadpan_plan::AudioSoundPermissionFamily;

#[derive(Clone)]
pub(super) struct FamilyToken<'plan>(Arc<AudioSoundPermissionFamily<'plan>>);

impl FamilyToken<'_> {
    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Default)]
pub(super) struct FamilyRegistry<'plan> {
    entries: Vec<FamilyToken<'plan>>,
}

fn family_bytes(family: &AudioSoundPermissionFamily<'_>) -> Result<usize, StageAudioError> {
    // The complete comparison bound counts every identifier byte and every
    // Repeat step across all three paths and the optional delivery path.
    // Cloning the typed family gives those Vecs length-sized allocations.
    // Budget a full step and allocator/header slack per counted unit, including
    // any separately owned identifier text, plus the family and Arc header.
    family
        .comparison_work()
        .checked_mul(std::mem::size_of::<deadpan_core::RepeatInstance>() + 256)
        .and_then(|bytes| bytes.checked_add(std::mem::size_of_val(family)))
        .and_then(|bytes| bytes.checked_add(2 * std::mem::size_of::<usize>()))
        .ok_or(StageAudioError::Limit("shared Split family residency"))
}

fn context_bytes(context: &InstancePath) -> Result<usize, StageAudioError> {
    // Cover one outer BTreeMap entry, its path Vec and identifier storage.
    let mut bytes = 1024usize
        .checked_add(std::mem::size_of_val(context))
        .and_then(|bytes| bytes.checked_add(context.node.as_str().len()))
        .ok_or(StageAudioError::Limit("shared Split family residency"))?;
    for step in &context.repeats {
        bytes = bytes
            .checked_add(std::mem::size_of_val(step) + 256)
            .and_then(|bytes| bytes.checked_add(step.node.as_str().len()))
            .and_then(|bytes| bytes.checked_add(step.iteration.allocation.as_str().len()))
            .ok_or(StageAudioError::Limit("shared Split family residency"))?;
    }
    Ok(bytes)
}

impl StageAudio {
    fn admit_family_bytes<'plan, 'work>(
        &mut self,
        graph: &mut Graph<'plan, 'work>,
        bytes: usize,
        control: WorkControl<'work>,
    ) -> Result<(), StageAudioError> {
        control.check()?;
        let resident = u64::try_from(bytes)
            .map_err(|_| StageAudioError::Limit("shared Split family residency"))?
            .div_ceil(std::mem::size_of::<[f32; 2]>() as u64);
        let graph_resident = graph
            .resident
            .checked_add(resident)
            .ok_or(StageAudioError::Limit("shared Split family residency"))?;
        let active = self
            .active_frames
            .checked_add(resident)
            .ok_or(StageAudioError::Limit("shared Split family residency"))?;
        self.make_room(resident, false, control)?;
        // The owning read's existing success/error cleanup releases this total.
        self.active_frames = active;
        graph.resident = graph_resident;
        Ok(())
    }

    pub(super) fn intern_permission_family<'plan, 'work>(
        &mut self,
        graph: &mut Graph<'plan, 'work>,
        family: &AudioSoundPermissionFamily<'plan>,
        control: WorkControl<'work>,
    ) -> Result<FamilyToken<'plan>, StageAudioError> {
        control.check()?;
        for canonical in &graph.canonical_families.entries {
            control.spend_plan_work(
                canonical
                    .0
                    .comparison_work()
                    .saturating_add(family.comparison_work()),
            )?;
            // This is the complete typed Eq, including plan, all contexts,
            // historical owner, timing, sound, forest and concrete Repeat plays.
            if canonical.0.as_ref() == family {
                control.spend_plan_work(1)?;
                return Ok(canonical.clone());
            }
        }
        if graph.canonical_families.entries.len() == shared_mix::MAX_VOICES {
            return Err(StageAudioError::Limit(
                "shared Split canonical family count",
            ));
        }
        control.spend_plan_work(family.comparison_work())?;
        let registry = if graph.canonical_families.entries.capacity() == 0 {
            shared_mix::MAX_VOICES
                .checked_mul(std::mem::size_of::<FamilyToken<'_>>())
                .ok_or(StageAudioError::Limit("shared Split family residency"))?
        } else {
            0
        };
        let bytes = family_bytes(family)?
            .checked_add(registry)
            .ok_or(StageAudioError::Limit("shared Split family residency"))?;
        self.admit_family_bytes(graph, bytes, control)?;
        if registry != 0 {
            // Admit the entire fixed hard-cap capacity before allocating it.
            graph.canonical_families.entries = Vec::with_capacity(shared_mix::MAX_VOICES);
        }
        let token = FamilyToken(Arc::new(family.clone()));
        graph.canonical_families.entries.push(token.clone());
        Ok(token)
    }

    /// The consuming voice can be absent from a later halo input block. Resolve
    /// its complete family once at the existing exact (context, voice) key.
    pub(super) fn selected_split_families<'plan, 'work>(
        &mut self,
        graph: &mut Graph<'plan, 'work>,
        context: &AudioSoundOwnerContext<'plan>,
        selected: Permissions,
        control: WorkControl<'work>,
    ) -> Result<Vec<(usize, FamilyToken<'plan>)>, StageAudioError> {
        control.spend_plan_work(graph.voices.len())?;
        let mut selected_voices = Vec::new();
        for (voice, id) in &graph.voices {
            if selected.contains(*id)
                && let Voice::Beat(owner, sound, instance) = voice
            {
                control.spend_plan_work(3 + instance.repeats.len())?;
                selected_voices.push((*id, owner.clone(), sound.clone(), instance.clone()));
            }
        }
        let mut result = Vec::new();
        for (id, owner, sound, instance) in selected_voices {
            control.spend_plan_work(1 + context.owner().repeats.len())?;
            if !graph
                .permission_families
                .get(context.owner())
                .is_some_and(|families| families.contains_key(&id))
            {
                if graph.selected_family_count == shared_mix::MAX_VOICES {
                    return Err(StageAudioError::Limit("shared Split family count"));
                }
                let scopes = graph.plan.beat_sound_clock_scopes(
                    &owner,
                    &sound,
                    control.query_limits()?.maximum_work,
                )?;
                for scope in &scopes {
                    control.spend_plan_work(scope.proof_work())?;
                }
                let family = if scopes
                    .iter()
                    .any(|scope| scope.has_split_alias() || scope.has_delivery_forest())
                {
                    let event = graph
                        .plan
                        .beat_sounds()
                        .get(&owner)
                        .and_then(|events| events.get(&sound))
                        .ok_or(PlanError::InvalidPlan("selected Split event absent"))?;
                    let voice = context.source_voice_occurrence(
                        instance,
                        deadpan_plan::AudioSourceVoiceRecipe {
                            source: event.source.clone(),
                            mapping: event.mapping,
                            offset: event.offset,
                        },
                        control.query_limits()?,
                    )?;
                    control.spend_plan_work(voice.construction_work())?;
                    let (family, used) =
                        voice.split_permission_family(&scopes, context, control.query_limits()?)?;
                    control.spend_plan_work(used)?;
                    family
                        .as_ref()
                        .map(|family| self.intern_permission_family(graph, family, control))
                        .transpose()?
                } else {
                    None
                };
                let new_context = !graph.permission_families.contains_key(context.owner());
                let bytes = 1024usize
                    .checked_add(if new_context {
                        context_bytes(context.owner())?
                    } else {
                        0
                    })
                    .ok_or(StageAudioError::Limit("shared Split family residency"))?;
                self.admit_family_bytes(graph, bytes, control)?;
                if new_context {
                    graph
                        .permission_families
                        .insert(context.owner().clone(), BTreeMap::new());
                }
                graph
                    .permission_families
                    .get_mut(context.owner())
                    .expect("admitted family context")
                    .insert(id, family);
                graph.selected_family_count += 1;
            }
            if let Some(family) = &graph.permission_families[context.owner()][&id] {
                control.spend_plan_work(1)?;
                result.push((id, family.clone()));
            }
        }
        Ok(result)
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "permission_families_tests.rs"]
mod tests;
