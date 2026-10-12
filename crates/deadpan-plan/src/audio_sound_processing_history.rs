//! Complete independent input for a copied temporal treatment owner.

use super::*;

/// Preparation support on transported physical labels, separate from current
/// delivery. It cannot grant a current output interval or a Hold allowance.
/// Current owner treatments still consume the complete shared input bus.
#[derive(Debug, Clone)]
pub struct AudioSoundProcessingHistory<'plan> {
    plan: &'plan RenderPlan,
    route: crate::AudioSoundRoute<AudioSample>,
    policy: AudioDomain<'plan>,
    original_allocation: Range<ExactRatio>,
    original_audible: Range<ExactRatio>,
    original_grid: AudioSampleGrid<AudioSample>,
    inherited_hard_edges: (bool, bool),
}

impl<'plan> AudioSourceOccurrence<'plan> {
    /// Expose complete retained input only when both occurrences were resolved
    /// within checked temporal-owner contexts and every window still selects
    /// the complete operand. Exterior/new owners consume delivered output.
    pub fn complete_processing_history(
        &self,
        original: &AudioSourceOccurrence<'_>,
        alias: &crate::AudioOccurrenceAlias<'_>,
        delivery: &crate::AudioSoundRoute<AudioSample>,
    ) -> Result<Option<AudioSoundProcessingHistory<'plan>>, PlanError> {
        self.validate_window_route(original, alias, delivery)?;
        let Some(policy) = self.policy_domain.as_deref() else {
            return Ok(None);
        };
        if original.policy_domain.is_none() {
            return Ok(None);
        }
        let Some(route) = delivery.complete_processing_history()? else {
            return Ok(None);
        };
        let (extent, _) = route.occurrence_output()?;
        let translation =
            route
                .occurrence_transitions()?
                .next()
                .ok_or(PlanError::InvalidPlan(
                    "missing processing history translation",
                ))??;
        let shift = extent.start.checked_sub(self.extent.start)?;
        let mut policy = policy.clone();
        // Sound-owner policy seeds enter a complete structural domain without
        // carried ancestors. Refuse a different seed shape rather than leaving
        // an unshifted constraint behind in a newly phased preparation clock.
        if policy.placement.is_some()
            || policy.seed.envelope.is_some()
            || !policy.seed.constraints.is_empty()
            || !policy.seed.editorial.is_empty()
            || !policy.seed.retimes.is_empty()
            || policy.seed.gap.is_some()
        {
            return Err(PlanError::InvalidPlan(
                "sound preparation policy carries an outer constraint",
            ));
        }
        policy.seed.transform.project_origin =
            policy.seed.transform.project_origin.checked_add(shift)?;
        policy.seed.extent = policy.seed.extent.start.checked_add(shift)?
            ..policy.seed.extent.end.checked_add(shift)?;
        policy.seed.grid = original
            .grid
            .translated_samples(translation.sample_shift)?
            .translated_frames(translation.frame_shift)?;
        policy.samples = policy.seed.grid.boundary(policy.seed.extent.start)?
            ..policy.seed.grid.boundary(policy.seed.extent.end)?;
        policy.visible = policy.samples.clone();
        Ok(Some(AudioSoundProcessingHistory {
            plan: self.plan,
            route,
            policy,
            original_allocation: original.extent(),
            original_audible: original.audible_extent(),
            original_grid: original.grid,
            inherited_hard_edges: self.inherited_hard_edges,
        }))
    }
}

impl AudioSoundProcessingHistory<'_> {
    pub fn route(&self) -> &crate::AudioSoundRoute<AudioSample> {
        &self.route
    }

    pub fn gate_fades(
        &self,
        edges: (deadpan_core::AudioEdgePolicy, deadpan_core::AudioEdgePolicy),
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<crate::AudioSoundGateQuery, PlanError> {
        crate::audio_sound_event::windowed_occurrence_gate_fades(
            self.plan,
            crate::audio_sound_event::RoutedOccurrenceEnvelope {
                domain: Some(&self.policy),
                anchor: self.route.identity_anchor(),
                original_allocation: self.original_allocation.clone(),
                current_allocation: self.route.occurrence_output()?.0,
                original_audible: self.original_audible.clone(),
                grid: self.original_grid,
                inherited_hard_edges: self.inherited_hard_edges,
            },
            &self.route,
            edges,
            samples,
            limits,
        )
    }
}
