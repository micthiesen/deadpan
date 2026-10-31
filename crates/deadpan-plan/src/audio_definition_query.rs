//! Actual root-domain constructor reused by admitted owner-envelope conversion.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::plan::audio_owners::admission::{Budget as QueryBudget, Owned, Result};
impl<'plan> AudioDefinition<'plan> {
    pub(in crate::plan) fn root_domain_on_grid_query<A: AudioSoundContextAdmission>(
        &self,
        placement: AudioRootPlacement,
        grid_origin: ExactRatio,
        rule: AudioBoundaryRule,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<Owned<AudioDomain<'plan>, A::Issuer, A::Lease>, A> {
        let support = placement.local_support();
        let rate = self.plan.metadata.presentation_basis.frame_rate;
        let frames_per_sample = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let transform = AudioTransform {
            project_origin: placement.origin(),
            project_frames_per_local_frame: placement.root_frames_per_local_frame(),
            project_frames_per_sample: frames_per_sample,
        };
        let project_at = |point: ExactRatio| {
            placement
                .origin()
                .checked_add(point.checked_mul(placement.root_frames_per_local_frame())?)
        };
        let extent = project_at(support.start)?..project_at(support.end)?;
        let grid = AudioSampleGrid::<AudioSample>::new(grid_origin, frames_per_sample, rule)?;
        let samples = grid.boundary(extent.start)?..grid.boundary(extent.end)?;
        let mut memory = budget.memory();
        let definition = budget.definition_ref(&mut memory, Some(&self.selector))?;
        let mut capacity = budget.memory();
        if A::METERED {
            budget.reserve(
                &mut capacity,
                crate::audio_allocated::admission::array::<EnvelopeConstraint>(1)?,
            )?;
        }
        let constraints = vec![EnvelopeConstraint {
            placement_support: true,
            range: extent.clone(),
            node: self.root,
            repeat_count: 0,
            gap_after: None,
            kinds: (AudioBoundaryKind::NodeStart, AudioBoundaryKind::NodeEnd),
        }];
        budget.check_memory(&memory)?;
        budget.check_memory(&capacity)?;
        budget.merge_checked(&mut memory, &mut capacity)?;
        let gap = budget.gap(&mut memory, &self.gap)?;
        let node = budget.node(&mut memory, self.root())?;
        let value = AudioDomain {
            plan: self.plan,
            seed: AudioWalkSeed {
                definition,
                bypass_binding: None,
                node: self.root,
                transform,
                grid,
                extent: extent.clone(),
                envelope: Some(extent.clone()),
                constraints,
                editorial: Vec::new(),
                suppress_entry_editorial: Default::default(),
                repeats: Vec::new(),
                retimes: Vec::new(),
                gap,
            },
            samples: samples.clone(),
            visible: samples,
            instance: InstancePath {
                node,
                repeats: Vec::new(),
            },
            placement: Some(placement),
        };
        Ok(Owned { value, memory })
    }
}
