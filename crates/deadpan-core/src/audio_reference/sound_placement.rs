//! Exact placement of an independent voice's complete processing allocation.

use super::*;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenSoundPlacement {
    /// Affine project position of the event owner's local zero.
    pub owner_origin: ExactRatio,
    /// Visible output, or full treatment input within the requested context.
    pub allocation: Range<ExactRatio>,
    /// Ordinary delivery's entry boundary, even for expanded hidden input.
    pub delivery_start: ExactRatio,
    pub work: usize,
}

/// Exact affine clock of the requested concrete ancestor. This scalar result
/// carries no media authority or retained instance/path allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrozenSoundContextClock {
    pub origin: ExactRatio,
    pub frames_per_local_frame: ExactRatio,
}

impl FrozenSoundContextClock {
    fn from_projection(projection: &FrozenAudioProjection) -> Self {
        Self {
            origin: projection.origin,
            frames_per_local_frame: projection.frames_per_local_frame,
        }
    }
}

impl FrozenAudioLayout {
    /// Resolve a concrete sound occurrence without reconstructing a media plan.
    /// Opaque processing resets its allocation at the outermost Preserve/Shift;
    /// affine ancestors select output afterward. A checked ancestor context
    /// exposes history only through its own complete processing boundary.
    pub fn sound_occurrence_placement(
        &self,
        instance: &InstancePath,
        context: Option<&InstancePath>,
        maximum_work: usize,
    ) -> Result<FrozenSoundPlacement, DocumentError> {
        self.sound_occurrence_placement_inner::<false>(instance, context, maximum_work)
            .map(|(placement, _)| placement)
    }

    /// Resolve placement and the exact requested ancestor clock together. A
    /// projection already required by placement supplies the two scalar clock
    /// fields; a context below the opaque provider is projected once afterward.
    /// The placement's work includes every executed query and clock extraction.
    pub fn sound_occurrence_placement_and_context(
        &self,
        instance: &InstancePath,
        context: Option<&InstancePath>,
        maximum_work: usize,
    ) -> Result<(FrozenSoundPlacement, Option<FrozenSoundContextClock>), DocumentError> {
        // Returning from the inner call drops its chain and all owned
        // projection paths before a fallback can allocate another path.
        let (mut placement, mut clock) =
            self.sound_occurrence_placement_inner::<true>(instance, context, maximum_work)?;
        if let Some(context) = context.filter(|_| clock.is_none()) {
            let projection = self.project(
                context,
                ExactRatio::ZERO,
                None,
                maximum_work
                    .checked_sub(placement.work)
                    .filter(|left| *left > 0)
                    .ok_or_else(|| limit("sound placement work exhausted"))?,
            )?;
            spend(&mut placement.work, projection.work, maximum_work)?;
            spend(&mut placement.work, 1, maximum_work)?;
            clock = Some(FrozenSoundContextClock::from_projection(&projection));
        }
        Ok((placement, clock))
    }

    fn sound_occurrence_placement_inner<const CAPTURE_CONTEXT: bool>(
        &self,
        instance: &InstancePath,
        context: Option<&InstancePath>,
        maximum_work: usize,
    ) -> Result<(FrozenSoundPlacement, Option<FrozenSoundContextClock>), DocumentError> {
        let owner = self.project(instance, ExactRatio::ZERO, None, maximum_work)?;
        let mut work = owner.work;
        let mut chain = vec![(instance.node.clone(), instance.repeats.len())];
        let mut provider = 0;
        while chain.last().unwrap().0 != self.root {
            spend(&mut work, 1, maximum_work)?;
            let (child, depth) = chain.last().unwrap();
            let parent = &self.index.parents[child].0;
            let node = &self.nodes[parent];
            let depth = if matches!(node.kind, FrozenAudioKind::Repeat { .. }) {
                depth
                    .checked_sub(1)
                    .ok_or_else(|| invalid("sound placement omits a Repeat"))?
            } else {
                *depth
            };
            if matches!(node.kind, FrozenAudioKind::Retime { mapping, pitch, .. }
                if pitch.processes(mapping.duration() == node.duration))
            {
                provider = chain.len();
            }
            chain.push((parent.clone(), depth));
        }
        let mut context_index = None;
        let boundary = if let Some(context) = context {
            spend(&mut work, context.repeats.len() + chain.len(), maximum_work)?;
            let index = chain
                .iter()
                .position(|(node, depth)| {
                    node == &context.node
                        && instance.repeats.get(..*depth) == Some(context.repeats.as_slice())
                })
                .ok_or_else(|| invalid("sound placement context is not its concrete ancestor"))?;
            if CAPTURE_CONTEXT {
                context_index = Some(index);
            }
            index.max(provider)
        } else {
            chain.len() - 1
        };
        let mut context_clock = None;
        if CAPTURE_CONTEXT && context_index == Some(0) {
            spend(&mut work, 1, maximum_work)?;
            context_clock = Some(FrozenSoundContextClock::from_projection(&owner));
        }
        let mut project = |index: usize| -> Result<FrozenAudioProjection, DocumentError> {
            let (node, depth) = &chain[index];
            spend(&mut work, 1 + depth, maximum_work)?;
            let projection = self.project(
                &InstancePath {
                    node: node.clone(),
                    repeats: instance.repeats[..*depth].to_vec(),
                },
                ExactRatio::ZERO,
                None,
                maximum_work
                    .checked_sub(work)
                    .filter(|left| *left > 0)
                    .ok_or_else(|| limit("sound placement work exhausted"))?,
            )?;
            spend(&mut work, projection.work, maximum_work)?;
            if CAPTURE_CONTEXT && context_index == Some(index) {
                spend(&mut work, 1, maximum_work)?;
                context_clock = Some(FrozenSoundContextClock::from_projection(&projection));
            }
            Ok(projection)
        };
        let provider_projection = if provider == 0 {
            owner.clone()
        } else {
            project(provider)?
        };
        let extent =
            |projection: &FrozenAudioProjection| -> Result<Range<ExactRatio>, DocumentError> {
                Ok(projection.origin
                    ..projection.origin.checked_add(
                        projection
                            .local_extent
                            .checked_mul(projection.frames_per_local_frame)?,
                    )?)
            };
        let mut allocation = extent(&provider_projection)?;
        let mut delivery = allocation.clone();
        for index in provider + 1..chain.len() {
            let allowed = extent(&project(index)?)?;
            // Keep even an empty delivery's affine entry. It is the rounding
            // anchor for expanded history, not a clamped or restarted source.
            intersect(&mut delivery, &allowed);
            if index <= boundary {
                intersect(&mut allocation, &allowed);
            }
        }
        if !allocation.end.compare(allocation.start).is_gt() {
            return Err(invalid("sound processing allocation is not visible"));
        }
        Ok((
            FrozenSoundPlacement {
                owner_origin: owner.origin,
                allocation,
                delivery_start: delivery.start,
                work,
            },
            context_clock,
        ))
    }
}

#[cfg(test)]
mod tests;

fn intersect(value: &mut Range<ExactRatio>, allowed: &Range<ExactRatio>) {
    if value.start.compare(allowed.start).is_lt() {
        value.start = allowed.start;
    }
    if value.end.compare(allowed.end).is_gt() {
        value.end = allowed.end;
    }
}
