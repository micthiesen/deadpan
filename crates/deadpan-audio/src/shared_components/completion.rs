//! Completed current-input policy, authenticated by the population producer.
//! This grants no source, numeric-validity or recursive-resource authority.

use super::*;

pub(in crate::stages) struct CompletedPolicy<'view, 'plan> {
    source: &'view Components<'plan>,
}

pub(super) fn seal<'view, 'plan>(
    source: &'view Components<'plan>,
    _: shared_eq::PopulationComplete,
    control: WorkControl<'_>,
) -> Result<CompletedPolicy<'view, 'plan>, StageAudioError> {
    control.spend_plan_work(1)?;
    if source.frames == 0 {
        return Err(StageAudioError::Range);
    }
    source
        .predicted
        .as_ref()
        .ok_or(PlanError::InvalidPlan("completed prediction absent"))?
        .validate_for(source.routes.len(), source.frames)?;
    source
        .start
        .0
        .checked_add(i64::try_from(source.frames).map_err(|_| StageAudioError::Range)?)
        .ok_or(TimeError::Overflow)?;
    Ok(CompletedPolicy { source })
}

impl<'plan> CompletedPolicy<'_, 'plan> {
    pub(super) fn flags(&self) -> &permissions::SamplePermissions {
        self.source
            .predicted
            .as_ref()
            .expect("sealed immutable population retains its prediction")
    }

    pub(super) fn validate(
        &self,
        routes: &[Route<'plan>],
        start: AudioSample,
        frames: usize,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        control.spend_plan_work(1)?;
        if !std::ptr::eq(routes, self.source.routes.as_slice())
            || start != self.source.start
            || frames != self.source.frames
        {
            return Err(PlanError::InvalidPlan("completed contribution context differs").into());
        }
        self.flags().validate_for(routes.len(), frames)
    }

    pub(super) fn active(
        &self,
        route: usize,
        control: WorkControl<'_>,
    ) -> Result<bool, StageAudioError> {
        control.spend_plan_work(1)?;
        if route >= self.source.routes.len() {
            return Err(StageAudioError::Range);
        }
        self.flags().any_words(route, control)
    }

    /// Caller admits this complete order-vector header and future payload
    /// before visitation. Other callback/recursive scratch is separate. The
    /// cold walk borrows clocks and performs no identity/PCM/native allocation.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Cold admission awaits recursive resource proof")
    )]
    pub(super) fn visit_resident_frames(&self) -> Result<u64, StageAudioError> {
        let bytes = self
            .source
            .owners
            .len()
            .checked_mul(size_of::<&Owner<'_>>())
            .and_then(|n| n.checked_add(size_of::<Vec<&Owner<'_>>>()))
            .ok_or(StageAudioError::Limit("cold contribution owner order"))?;
        u64::try_from(bytes)
            .map(|bytes| bytes.div_ceil(size_of::<[f32; 2]>() as u64))
            .map_err(|_| StageAudioError::Range)
    }

    /// Visit the same successful numerical schedule without numerical state.
    /// The callback is provisional until later independent observation checks
    /// succeed. No failed batch may be retried through a scalar read.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Cold admission awaits recursive resource proof")
    )]
    pub(super) fn visit_demands(
        &self,
        control: WorkControl<'_>,
        mut demand: impl for<'phase> FnMut(
            visitors::TemporalDemand<'phase, 'plan>,
        ) -> Result<(), StageAudioError>,
    ) -> Result<(), StageAudioError> {
        let source = self.source;
        let order = member_phases::cold_order(
            &source.routes,
            &source.owners,
            self,
            (source.start, source.frames),
            control,
        )?;
        let mut owners: Vec<&Owner<'plan>> = Vec::with_capacity(source.owners.len());
        // Stable descending depth, matching ordered_owners. The fallible
        // insertion walk bills before every actual owner/depth inspection;
        // capacity is admitted once and no sorting scratch is allocated.
        for owner in source.owners.values() {
            control.spend_plan_work(1)?;
            let mut index = 0;
            while index < owners.len() {
                control.spend_plan_work(1)?;
                if owners[index].depth < owner.depth {
                    break;
                }
                index += 1;
            }
            owners.insert(index, owner);
        }
        let mut visit_owner = |owner, at| {
            member_phases::run_cold_owner(
                &source.routes,
                member_phases::OwnerPhase {
                    permitted: self.flags(),
                    owner,
                    start: source.start,
                    frames: source.frames,
                    send: source.send,
                },
                &order,
                at,
                control,
                &mut demand,
            )
        };
        if order.is_block() {
            for owner in owners {
                control.spend_plan_work(1)?;
                visit_owner(owner, None)?;
            }
        } else {
            for at in 0..source.frames {
                control.spend_plan_work(1)?;
                for owner in &owners {
                    control.spend_plan_work(1)?;
                    visit_owner(*owner, Some(at))?;
                }
            }
        }
        control.check()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "completion_tests.rs"]
pub(super) mod tests;
