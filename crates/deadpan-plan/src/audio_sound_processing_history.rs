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
        if self.policy_domain.is_none() {
            return Ok(None);
        }
        if original.policy_domain.is_none() {
            return Ok(None);
        }
        let Some(route) = delivery.complete_processing_history()? else {
            return Ok(None);
        };
        self.processing_history_with_route(original, route)
            .map(Some)
    }

    /// Preserve delivered input before a processing owner existed, then expose
    /// its retained input through subsequent complete placements. The complete
    /// ordered journal and exact context determine birth internally; a caller
    /// cannot supply an earlier boundary to revive previously clipped samples.
    /// `binding` must certify the journal's first applicable scope and the
    /// original occurrence's exact historical plan and recipe.
    #[allow(clippy::too_many_arguments)] // Keep the independent checked authorities explicit.
    pub fn complete_processing_history_after_birth(
        &self,
        original: &AudioSourceOccurrence<'_>,
        alias: &crate::AudioOccurrenceAlias<'_>,
        binding: &crate::AudioSoundClockBinding<'_, '_>,
        delivery: &crate::AudioSoundRoute<AudioSample>,
        clocks: &[crate::AudioSoundClockScope<'_>],
        context: &AudioSoundOwnerContext<'_>,
        limits: AudioQueryLimits,
    ) -> Result<(Option<AudioSoundProcessingHistory<'plan>>, usize), PlanError> {
        limits.validate()?;
        self.validate_window_route(original, alias, delivery)?;
        if !context.belongs_to(self.plan)
            || self
                .policy_domain
                .as_ref()
                .is_none_or(|policy| !Arc::ptr_eq(policy, &context.policy))
        {
            return Err(PlanError::InvalidPlan(
                "sound processing birth has a foreign context",
            ));
        }
        let mut work = 0usize;
        let spend = |work: &mut usize, count: usize| -> Result<(), PlanError> {
            *work = work
                .checked_add(count)
                .filter(|used| *used <= limits.maximum_work)
                .ok_or(PlanError::AudioQueryLimit("sound processing birth work"))?;
            Ok(())
        };
        let remaining = |work: usize| -> Result<usize, PlanError> {
            limits
                .maximum_work
                .checked_sub(work)
                .filter(|left| *left > 0)
                .ok_or(PlanError::AudioQueryLimit("sound processing birth work"))
        };
        spend(
            &mut work,
            clocks.len().checked_add(1).ok_or(TimeError::Overflow)?,
        )?;
        let first_scope = clocks
            .first()
            .ok_or(PlanError::InvalidPlan("missing sound birth journal"))?;
        let journal = self
            .plan
            .audio_bindings
            .sound_clocks()
            .get(&self.instance.node)
            .and_then(|events| events.get(&first_scope.live_sound))
            .ok_or(PlanError::InvalidPlan("sound birth journal is absent"))?;
        if clocks.len() != journal.clocks().len()
            || clocks.iter().enumerate().any(|(index, scope)| {
                !std::ptr::eq(scope.live_plan, self.plan)
                    || scope.live_owner != self.instance.node
                    || scope.live_sound != first_scope.live_sound
                    || scope.journal_ordinal != index
                    || scope.recipe != *self.recipe()
                    || scope.timing() != journal.clocks()[index].timing()
            })
        {
            return Err(PlanError::InvalidPlan(
                "sound birth requires its complete ordered journal",
            ));
        }
        let extent = original.extent();
        let step = original.grid.frames_per_sample();
        let mut certified_split = false;
        for scope in clocks {
            for split in scope.introduced_splits {
                spend(&mut work, split.work_count())?;
                certified_split |= split
                    .pieces
                    .iter()
                    .any(|piece| piece.full_owner == self.instance.node);
            }
        }
        let mut context_origins = Vec::new();
        if certified_split {
            spend(
                &mut work,
                clocks.len().checked_add(1).ok_or(TimeError::Overflow)?,
            )?;
            context_origins.reserve(clocks.len() + 1);
        }
        let mut first_origin = None;
        let mut first_applicable = None;
        let mut placement_index = 0usize;
        let mut birth = None;
        for scope in clocks {
            let (instance, used) = scope.try_remap_instance_with_work(
                self.instance(),
                remaining(work)?.min(deadpan_core::MAX_DOCUMENT_NODES),
            )?;
            spend(&mut work, used)?;
            let Some(instance) = instance else { continue };
            let (historical_context, used) = scope.try_remap_context_with_work(
                context,
                remaining(work)?.min(deadpan_core::MAX_DOCUMENT_NODES),
            )?;
            spend(&mut work, used)?;
            if first_applicable.is_none() {
                // The caller already bound this exact immutable plan and
                // recipe before minting the occurrence alias. Verify the first
                // applicable journal member instead of validating its whole
                // frozen layout again. Separately retrieved scope handles from
                // the same plan are equivalent by this complete identity.
                // Charge the four scope-identity checks, two historical-plan
                // checks, and one recipe comparison before inspecting them.
                spend(&mut work, 7)?;
                if !std::ptr::eq(binding.scope.live_plan, scope.live_plan)
                    || binding.scope.live_owner != scope.live_owner
                    || binding.scope.live_sound != scope.live_sound
                    || binding.scope.journal_ordinal != scope.journal_ordinal
                    || !std::ptr::eq(binding.historical_plan, alias.historical_plan)
                    || !std::ptr::eq(binding.historical_plan, original.plan())
                    || binding.recipe != alias.recipe
                {
                    return Err(PlanError::InvalidPlan(
                        "sound birth has a foreign initial binding",
                    ));
                }
                if instance != *original.instance()
                    || historical_context.is_some() != original.policy_domain.is_some()
                {
                    return Err(PlanError::InvalidPlan(
                        "sound birth has another initial occurrence",
                    ));
                }
                first_applicable = Some(scope);
            }
            if historical_context.is_some() {
                birth.get_or_insert(placement_index);
            } else if birth.is_some() {
                // A disappearing context has no continuous complete suffix.
                return Ok((None, work));
            }
            if certified_split {
                let projected = historical_context
                    .as_ref()
                    .map(|context| {
                        scope
                            .historical_layout()
                            .project(
                                context,
                                ExactRatio::ZERO,
                                None,
                                remaining(work)?.min(deadpan_core::MAX_DOCUMENT_NODES),
                            )
                            .map_err(PlanError::from)
                    })
                    .transpose()?;
                if let Some(projected) = &projected {
                    spend(&mut work, projected.work)?;
                }
                context_origins.push(projected.map(|p| (p.origin, p.frames_per_local_frame)));
            }
            let placement = scope.historical_layout().sound_occurrence_placement(
                &instance,
                historical_context.as_ref(),
                remaining(work)?.min(deadpan_core::MAX_DOCUMENT_NODES),
            )?;
            spend(&mut work, placement.work)?;
            let origin = *first_origin.get_or_insert(placement.owner_origin);
            let complete_start = extent
                .start
                .checked_add(placement.owner_origin.checked_sub(origin)?)?;
            let grid = AudioSampleGrid::new(
                ExactRatio::ZERO.checked_sub(complete_start)?,
                step,
                AudioBoundaryRule::RoundEven,
            )?;
            let window = crate::AudioOccurrenceWindow {
                selection: placement.allocation.start.checked_sub(complete_start)?
                    ..placement.allocation.end.checked_sub(complete_start)?,
                anchor: placement.delivery_start.checked_sub(complete_start)?,
            };
            if !delivery.occurrence_placement_matches(placement_index, &grid, &window) {
                return Err(PlanError::InvalidPlan(
                    "sound birth route differs from its journal",
                ));
            }
            placement_index = placement_index.checked_add(1).ok_or(TimeError::Overflow)?;
        }
        let scope =
            first_applicable.ok_or(PlanError::InvalidPlan("sound birth has no occurrence"))?;
        let placement = scope.live_layout().sound_occurrence_placement(
            self.instance(),
            Some(context.owner()),
            remaining(work)?.min(deadpan_core::MAX_DOCUMENT_NODES),
        )?;
        spend(&mut work, placement.work)?;
        if certified_split {
            let projected = scope.live_layout().project(
                context.owner(),
                ExactRatio::ZERO,
                None,
                remaining(work)?.min(deadpan_core::MAX_DOCUMENT_NODES),
            )?;
            spend(&mut work, projected.work)?;
            context_origins.push(Some((projected.origin, projected.frames_per_local_frame)));
        }
        let complete_start = extent
            .start
            .checked_add(placement.owner_origin.checked_sub(
                first_origin.ok_or(PlanError::InvalidPlan("sound birth has no initial origin"))?,
            )?)?;
        let grid = AudioSampleGrid::new(
            ExactRatio::ZERO.checked_sub(complete_start)?,
            step,
            AudioBoundaryRule::RoundEven,
        )?;
        let window = crate::AudioOccurrenceWindow {
            selection: placement.allocation.start.checked_sub(complete_start)?
                ..placement.allocation.end.checked_sub(complete_start)?,
            anchor: placement.delivery_start.checked_sub(complete_start)?,
        };
        if placement.allocation != self.extent()
            || delivery.occurrence_placement_count() != placement_index + 1
            || !delivery.occurrence_placement_matches(placement_index, &grid, &window)
        {
            return Err(PlanError::InvalidPlan(
                "sound birth has another current placement",
            ));
        }
        let birth = birth.unwrap_or(placement_index);
        let (mut route, used) =
            delivery.processing_history_from_boundary(birth, remaining(work)?)?;
        spend(&mut work, used)?;
        if route.is_none() && certified_split {
            // Every historical scope has already proved this complete current
            // Split forest. Retain its real piece windows. A complete context
            // owns one physical clock across the pieces, rather than copying
            // each fragment at its newly rounded local entry boundary.
            let (origin, scale) = context_origins
                .get(birth)
                .copied()
                .flatten()
                .ok_or(PlanError::InvalidPlan("Split context birth is absent"))?;
            if scale != ExactRatio::ONE {
                return Ok((None, work));
            }
            let grid = AudioSampleGrid::<AudioSample>::new(
                ExactRatio::ZERO,
                step,
                AudioBoundaryRule::RoundEven,
            )?;
            let first = grid.boundary(origin)?.0;
            spend(&mut work, context_origins.len())?;
            let mut shifts = vec![0; context_origins.len()];
            for (index, projected) in context_origins.iter().enumerate().skip(birth) {
                let Some((origin, scale)) = projected else {
                    return Ok((None, work));
                };
                if *scale != ExactRatio::ONE {
                    return Ok((None, work));
                }
                shifts[index] = i128::from(grid.boundary(*origin)?.0) - i128::from(first);
            }
            let (continued, used) = delivery.processing_history_with_context_shifts(
                birth,
                &shifts,
                remaining(work)?,
            )?;
            spend(&mut work, used)?;
            route = continued;
        }
        let history = route
            .map(|route| self.processing_history_with_route(original, route))
            .transpose()?;
        Ok((history, work))
    }

    fn processing_history_with_route(
        &self,
        original: &AudioSourceOccurrence<'_>,
        route: crate::AudioSoundRoute<AudioSample>,
    ) -> Result<AudioSoundProcessingHistory<'plan>, PlanError> {
        let policy = self.policy_domain.as_deref().ok_or(PlanError::InvalidPlan(
            "sound processing history requires a current owner context",
        ))?;
        let (extent, _) = route.occurrence_output()?;
        let mut sample_shift = 0i128;
        let mut frame_shift = ExactRatio::ZERO;
        for translation in route.occurrence_transitions()? {
            let translation = translation?;
            sample_shift = sample_shift
                .checked_add(translation.sample_shift)
                .ok_or(TimeError::Overflow)?;
            frame_shift = frame_shift.checked_add(translation.frame_shift)?;
        }
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
            .translated_samples(sample_shift)?
            .translated_frames(frame_shift)?;
        policy.samples = policy.seed.grid.boundary(policy.seed.extent.start)?
            ..policy.seed.grid.boundary(policy.seed.extent.end)?;
        policy.visible = policy.samples.clone();
        Ok(AudioSoundProcessingHistory {
            plan: self.plan,
            route,
            policy,
            original_allocation: original.extent(),
            original_audible: original.audible_extent(),
            original_grid: original.grid,
            inherited_hard_edges: self.inherited_hard_edges,
        })
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
