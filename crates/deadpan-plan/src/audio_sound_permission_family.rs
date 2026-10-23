//! Context-local continuity of explicitly certified Split contributions.

use super::*;

/// Two current delivery fragments can supply one temporal owner's input when
/// a complete, checked Split forest proves their common historical event.
/// This identity grants no output support, source access, or Hold permission.
/// It is valid only inside the named current owner, never on the root bus.
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
}

impl PartialEq for AudioSoundPermissionFamily<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.plan, other.plan)
            && self.context == other.context
            && self.timing == other.timing
            && self.historical_context == other.historical_context
            && self.historical_owner == other.historical_owner
            && self.sound == other.sound
    }
}

impl Eq for AudioSoundPermissionFamily<'_> {}

impl AudioSoundPermissionFamily<'_> {
    /// Charge before comparing independently prepared family identities.
    pub fn comparison_work(&self) -> usize {
        self.context
            .repeats
            .len()
            .saturating_add(self.historical_context.repeats.len())
            .saturating_add(self.historical_owner.repeats.len())
            // Each repeat step compares two IDs, each bounded to 128 bytes.
            // The extra eight conservatively cover the fixed node, timing
            // and sound identifiers.
            .saturating_add(8)
            .saturating_mul(256)
    }
}

impl<'plan> AudioSourceOccurrence<'plan> {
    /// Establish continuity only from this occurrence's complete ordered
    /// journal, an explicit Split witness, and a genuine historical alias of
    /// the supplied current context. Equal shapes or audio lineage do not
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
                }),
                work,
            ));
        }
        Ok((None, work))
    }
}
