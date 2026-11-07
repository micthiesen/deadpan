//! Context-local continuity of explicitly certified Split contributions.

use super::*;

/// Two current delivery fragments can supply one temporal owner's input when
/// a complete, checked Split forest proves their common historical event.
/// This identity grants no output support, source access, or Hold permission.
/// A current-delivery proof can join prepared routes on an enclosing root bus;
/// it cannot expose any fragment's hidden historical input there.
/// Labels and live fragment treatments do not change this retained identity;
/// each contribution applies its own current policy before the owner's mix.
#[derive(Debug, Clone)]
pub struct AudioSoundPermissionFamily<'plan> {
    plan: &'plan RenderPlan,
    context: InstancePath,
    timing: deadpan_core::AudioTimingId,
    historical_context: InstancePath,
    historical_owner: InstancePath,
    sound: deadpan_core::SoundId,
    delivery: Option<(NodeId, Vec<deadpan_core::RepeatInstance>)>,
}

impl PartialEq for AudioSoundPermissionFamily<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.plan, other.plan)
            && self.context == other.context
            && self.timing == other.timing
            && self.historical_context == other.historical_context
            && self.historical_owner == other.historical_owner
            && self.sound == other.sound
            && self.delivery == other.delivery
    }
}

impl Eq for AudioSoundPermissionFamily<'_> {}

impl AudioSoundPermissionFamily<'_> {
    /// Charge before comparing independently prepared family identities.
    pub fn comparison_work(&self) -> usize {
        // Charge the actual bounded identity bytes, including scalar/tag
        // comparisons. Short identifiers do not require 128-byte comparisons.
        // Callers add both operands' costs before checking equality.
        fn repeats_cost(repeats: &[deadpan_core::RepeatInstance]) -> usize {
            repeats.iter().fold(1usize, |used, step| {
                used.saturating_add(step.node.as_str().len())
                    .saturating_add(step.iteration.allocation.as_str().len())
                    .saturating_add(3)
            })
        }
        fn instance_cost(instance: &InstancePath) -> usize {
            instance
                .node
                .as_str()
                .len()
                .saturating_add(repeats_cost(&instance.repeats))
                .saturating_add(1)
        }
        instance_cost(&self.context)
            .saturating_add(instance_cost(&self.historical_context))
            .saturating_add(instance_cost(&self.historical_owner))
            .saturating_add(self.timing.allocation.as_str().len())
            .saturating_add(self.sound.as_str().len())
            .saturating_add(8)
            .saturating_add(self.delivery.as_ref().map_or(1, |(forest, repeats)| {
                forest
                    .as_str()
                    .len()
                    .saturating_add(repeats_cost(repeats))
                    .saturating_add(2)
            }))
    }
}

impl<'plan> AudioSourceOccurrence<'plan> {
    /// Establish continuity only from this occurrence's complete ordered
    /// journal and either a checked current delivery forest or a historical
    /// alias of the supplied context. Equal shapes or audio lineage do not
    /// establish a family. Current per-sample gates remain authoritative.
    pub fn split_permission_family(
        &self,
        clocks: &[crate::AudioSoundClockScope<'_>],
        context: &AudioSoundOwnerContext<'_>,
        limits: AudioQueryLimits,
    ) -> Result<(Option<AudioSoundPermissionFamily<'plan>>, usize), PlanError> {
        limits.validate()?;
        if !context.belongs_to(self.plan)
            || self
                .policy_domain
                .as_ref()
                .is_none_or(|policy| !Arc::ptr_eq(policy, &context.policy))
        {
            return Err(PlanError::InvalidPlan("foreign Split permission context"));
        }
        let mut work = clocks.len().checked_add(1).ok_or(TimeError::Overflow)?;
        let spend = |work: &mut usize, count: usize| -> Result<(), PlanError> {
            *work = work
                .checked_add(count)
                .filter(|used| *used <= limits.maximum_work)
                .ok_or(PlanError::AudioQueryLimit("Split permission family work"))?;
            Ok(())
        };
        spend(&mut work, 0)?;
        let Some(first) = clocks.first() else {
            return Ok((None, work));
        };
        let journal = self
            .plan
            .audio_bindings
            .sound_clocks()
            .get(&self.instance.node)
            .and_then(|events| events.get(&first.live_sound))
            .ok_or(PlanError::InvalidPlan("Split permission journal absent"))?;
        if clocks.len() != journal.clocks().len()
            || clocks.iter().enumerate().any(|(index, scope)| {
                !std::ptr::eq(scope.live_plan, self.plan)
                    || scope.live_owner != self.instance.node
                    || scope.live_sound != first.live_sound
                    || scope.journal_ordinal != index
                    || scope.recipe != *self.recipe()
                    || scope.timing() != journal.clocks()[index].timing()
            })
        {
            return Err(PlanError::InvalidPlan(
                "Split permission requires complete ordered journal",
            ));
        }
        let (delivered, used) = self.delivered_family(
            &first.live_sound,
            context,
            limits
                .maximum_work
                .checked_sub(work)
                .ok_or(TimeError::Overflow)?,
        )?;
        spend(&mut work, used)?;
        if delivered.is_some() {
            return Ok((delivered, work));
        }
        for scope in clocks {
            let mut certified = false;
            for split in scope.introduced_splits {
                spend(&mut work, split.work_count())?;
                certified |= split
                    .pieces
                    .iter()
                    .any(|piece| piece.full_owner == self.instance.node);
            }
            if !certified {
                continue;
            }
            let remaining = limits
                .maximum_work
                .checked_sub(work)
                .filter(|left| *left > 0)
                .ok_or(PlanError::AudioQueryLimit("Split permission family work"))?
                .min(deadpan_core::MAX_DOCUMENT_NODES);
            let (historical_context, used) =
                scope.try_remap_context_with_work(context, remaining)?;
            spend(&mut work, used)?;
            let Some(historical_context) = historical_context else {
                continue;
            };
            let remaining = limits
                .maximum_work
                .checked_sub(work)
                .filter(|left| *left > 0)
                .ok_or(PlanError::AudioQueryLimit("Split permission family work"))?
                .min(deadpan_core::MAX_DOCUMENT_NODES);
            let (historical_owner, used) =
                scope.try_remap_instance_with_work(self.instance(), remaining)?;
            spend(&mut work, used)?;
            let Some(historical_owner) = historical_owner else {
                continue;
            };
            spend(
                &mut work,
                context
                    .owner()
                    .repeats
                    .len()
                    .saturating_add(historical_context.repeats.len())
                    .saturating_add(historical_owner.repeats.len())
                    .saturating_add(8)
                    .saturating_mul(256),
            )?;
            return Ok((
                Some(AudioSoundPermissionFamily {
                    plan: self.plan,
                    context: context.owner().clone(),
                    timing: scope.timing().clone(),
                    historical_context,
                    historical_owner,
                    sound: first.live_sound.clone(),
                    delivery: None,
                }),
                work,
            ));
        }
        Ok((None, work))
    }

    /// The historical occurrence this voice delivers, when a processor above
    /// its Split pieces receives each piece's complete copy. Copies with one
    /// identity must sound once. `context`, if any, must enclose the forest.
    pub fn delivered_occurrence(
        &self,
        sound: &deadpan_core::SoundId,
        context: Option<&AudioSoundOwnerContext<'_>>,
        limits: AudioQueryLimits,
    ) -> Result<(Option<InstancePath>, usize), PlanError> {
        limits.validate()?;
        let Some(members) = self
            .plan
            .sound_delivery_forests
            .get(&(self.instance.node.clone(), sound.clone()))
        else {
            return Ok((None, 1));
        };
        let mut work = 1usize;
        for member in members.iter().filter(|member| member.below_processor) {
            let enclosed = context.is_none_or(|context| {
                std::iter::successors(Some(member.parent), |node| self.plan.parents[*node])
                    .any(|node| node == context.owner_node)
            });
            if !enclosed {
                continue;
            }
            let remaining = limits
                .maximum_work
                .checked_sub(work)
                .filter(|left| *left > 0)
                .ok_or(PlanError::AudioQueryLimit("sound delivery family work"))?
                .min(deadpan_core::MAX_DOCUMENT_NODES);
            let (outer, inner) = split_above_proof(&self.instance, member)?;
            let (historical, used) = member
                .proof
                .try_remap_instance_with_work(&inner, remaining)?;
            work = work.saturating_add(used);
            if let Some(mut historical) = historical {
                // Outer plays stay distinct occurrences of one inner copy.
                historical.repeats.splice(0..0, outer.iter().cloned());
                return Ok((Some(historical), work));
            }
        }
        Ok((None, work))
    }

    fn delivered_family(
        &self,
        sound: &deadpan_core::SoundId,
        context: &AudioSoundOwnerContext<'_>,
        maximum: usize,
    ) -> Result<(Option<AudioSoundPermissionFamily<'plan>>, usize), PlanError> {
        let mut work = 1usize;
        let spend = |work: &mut usize, count: usize| -> Result<(), PlanError> {
            *work = work
                .checked_add(count)
                .filter(|used| *used <= maximum)
                .ok_or(PlanError::AudioQueryLimit("sound delivery family work"))?;
            Ok(())
        };
        spend(&mut work, 0)?;
        let Some(members) = self
            .plan
            .sound_delivery_forests
            .get(&(self.instance.node.clone(), sound.clone()))
        else {
            return Ok((None, work));
        };
        for member in members {
            // The consuming context must enclose the whole current forest.
            // A copied inner owner still uses its existing processing proof.
            let mut ancestor = Some(member.parent);
            let mut inside = false;
            while let Some(node) = ancestor {
                spend(&mut work, 1)?;
                if node == context.owner_node {
                    inside = true;
                    break;
                }
                ancestor = self.plan.parents[node];
            }
            if !inside {
                continue;
            }
            let remaining = maximum
                .checked_sub(work)
                .filter(|left| *left > 0)
                .ok_or(PlanError::AudioQueryLimit("sound delivery family work"))?
                .min(deadpan_core::MAX_DOCUMENT_NODES);
            let (_, scoped) = split_above_proof(&self.instance, member)?;
            let (historical_owner, used) = member
                .proof
                .try_remap_instance_with_work(&scoped, remaining)?;
            spend(&mut work, used)?;
            let Some(historical_owner) = historical_owner else {
                continue;
            };
            let Some(repeats) = self.instance.repeats.get(..member.outer_repeats) else {
                return Err(PlanError::InvalidPlan("sound delivery forest Repeat path"));
            };
            spend(&mut work, repeats.len() + 1)?;
            let full = InstancePath {
                node: member.full.clone(),
                repeats: repeats[member.repeats_above_proof..].to_vec(),
            };
            let remaining = maximum
                .checked_sub(work)
                .filter(|left| *left > 0)
                .ok_or(PlanError::AudioQueryLimit("sound delivery family work"))?
                .min(deadpan_core::MAX_DOCUMENT_NODES);
            let (historical_context, used) = member
                .proof
                .try_remap_instance_with_work(&full, remaining)?;
            spend(&mut work, used)?;
            let Some(historical_context) = historical_context else {
                continue;
            };
            if historical_context.node != member.historical_full {
                return Err(PlanError::InvalidPlan(
                    "sound delivery forest historical context",
                ));
            }
            let family = AudioSoundPermissionFamily {
                plan: self.plan,
                context: context.owner().clone(),
                timing: member.timing.clone(),
                historical_context,
                historical_owner,
                sound: sound.clone(),
                delivery: Some((member.first.clone(), repeats.to_vec())),
            };
            spend(&mut work, family.comparison_work())?;
            return Ok((Some(family), work));
        }
        Ok((None, work))
    }
}

/// A live path divided at the delivery proof's top: Repeats above its processor,
/// and the path the proof can remap.
fn split_above_proof<'a>(
    instance: &'a InstancePath,
    member: &crate::plan::sound_delivery_forest::Member,
) -> Result<(&'a [deadpan_core::RepeatInstance], InstancePath), PlanError> {
    if member.repeats_above_proof > instance.repeats.len() {
        return Err(PlanError::InvalidPlan("sound delivery forest Repeat path"));
    }
    let (outer, inner) = instance.repeats.split_at(member.repeats_above_proof);
    Ok((
        outer,
        InstancePath {
            node: instance.node.clone(),
            repeats: inner.to_vec(),
        },
    ))
}
