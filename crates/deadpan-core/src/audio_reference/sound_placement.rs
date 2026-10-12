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
        let boundary = if let Some(context) = context {
            spend(&mut work, context.repeats.len() + chain.len(), maximum_work)?;
            chain
                .iter()
                .position(|(node, depth)| {
                    node == &context.node
                        && instance.repeats.get(..*depth) == Some(context.repeats.as_slice())
                })
                .ok_or_else(|| invalid("sound placement context is not its concrete ancestor"))?
                .max(provider)
        } else {
            chain.len() - 1
        };
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
                        ExactRatio::integer(projection.local_duration.frames())
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
        Ok(FrozenSoundPlacement {
            owner_origin: owner.origin,
            allocation,
            delivery_start: delivery.start,
            work,
        })
    }
}

fn intersect(value: &mut Range<ExactRatio>, allowed: &Range<ExactRatio>) {
    if value.start.compare(allowed.start).is_lt() {
        value.start = allowed.start;
    }
    if value.end.compare(allowed.end).is_gt() {
        value.end = allowed.end;
    }
}
