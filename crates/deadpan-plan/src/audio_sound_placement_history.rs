//! One-shot evidence for exact placements already queried by the preparation host.

use super::*;

/// One checked placement in chronological order. This is the same scalar row
/// needed to build a delivery route; it grants no media or permission authority.
#[derive(Debug)]
pub struct AudioSoundPlacement {
    complete_start: ExactRatio,
    allocation: Range<ExactRatio>,
    anchor: ExactRatio,
    expands_history: bool,
}

impl AudioSoundPlacement {
    pub fn complete_start(&self) -> ExactRatio {
        self.complete_start
    }
    pub fn allocation(&self) -> Range<ExactRatio> {
        self.allocation.clone()
    }
    pub fn anchor(&self) -> ExactRatio {
        self.anchor
    }
    pub fn expands_history(&self) -> bool {
        self.expands_history
    }
}

struct Authorities<'query, 'current, 'original> {
    original: &'query AudioSourceOccurrence<'original>,
    current: &'query AudioSourceOccurrence<'current>,
    clocks: &'query [crate::AudioSoundClockScope<'current>],
    context: Option<&'query AudioSoundOwnerContext<'current>>,
}

/// A bounded sequence of individually admitted queries. Each `advance` runs
/// one remap or placement query after an initial charged allocation guard.
/// The host must admit and charge every step, including steps that discover an
/// absent occurrence. On error the
/// cursor is poisoned; a partial history can never become placement evidence.
/// It borrows plans and owns only the replacement scalar row vector and, while
/// a historical placement is pending, the two remapped paths the old loop used.
pub struct AudioSoundPlacementHistoryCursor<'query, 'current, 'original> {
    authorities: Authorities<'query, 'current, 'original>,
    rows: Vec<AudioSoundPlacement>,
    first_origin: Option<ExactRatio>,
    phase: Phase,
}

enum Phase {
    Initial,
    Instance(usize),
    Context(usize, InstancePath),
    Historical(usize, InstancePath, Option<InstancePath>),
    Current,
    Complete,
    Poisoned,
}

/// Finished, non-Clone placement evidence. Only the checked cursor can issue
/// this value. Consume it when certifying processing history; read-only rows
/// let the host construct the delivery route without copying this allocation.
pub struct AudioSoundPlacementHistory<'query, 'current, 'original> {
    authorities: Authorities<'query, 'current, 'original>,
    rows: Vec<AudioSoundPlacement>,
}

impl<'current> AudioSourceOccurrence<'current> {
    pub fn placement_history_cursor<'query, 'original>(
        &'query self,
        original: &'query AudioSourceOccurrence<'original>,
        clocks: &'query [crate::AudioSoundClockScope<'current>],
        context: Option<&'query AudioSoundOwnerContext<'current>>,
    ) -> Result<AudioSoundPlacementHistoryCursor<'query, 'current, 'original>, PlanError> {
        if clocks.is_empty() {
            return Err(PlanError::InvalidPlan("sound clock history is empty"));
        }
        Ok(AudioSoundPlacementHistoryCursor {
            authorities: Authorities {
                original,
                current: self,
                clocks,
                context,
            },
            rows: Vec::new(),
            first_origin: None,
            phase: Phase::Initial,
        })
    }
}

impl<'query, 'current, 'original> AudioSoundPlacementHistoryCursor<'query, 'current, 'original> {
    pub fn is_complete(&self) -> bool {
        matches!(self.phase, Phase::Complete)
    }

    pub fn advance(&mut self, maximum_work: usize) -> Result<usize, PlanError> {
        // Replace before any fallible operation: no error can leave a reusable
        // partial token, including a refusal of the supplied query budget.
        let phase = std::mem::replace(&mut self.phase, Phase::Poisoned);
        AudioQueryLimits {
            maximum_work,
            ..AudioQueryLimits::default()
        }
        .validate()?;
        let a = &self.authorities;
        match phase {
            Phase::Initial => {
                // A public caller can repeat a sealed scope arbitrarily. Bound
                // allocation by the actual canonical journal before reserving,
                // without walking the supplied slice or tightening the next
                // remap's independent query allowance. Six structural steps:
                // first member, owner/event lookups, count, capacity, reserve.
                const WORK: usize = 6;
                if maximum_work < WORK {
                    return Err(PlanError::AudioQueryLimit("sound placement history work"));
                }
                let first = &a.clocks[0];
                let journal = a
                    .current
                    .plan
                    .audio_bindings
                    .sound_clocks()
                    .get(&a.current.instance.node)
                    .and_then(|events| events.get(&first.live_sound))
                    .ok_or(PlanError::InvalidPlan("sound placement journal is absent"))?;
                if a.clocks.len() != journal.clocks().len() {
                    return Err(PlanError::InvalidPlan(
                        "sound placement requires its complete journal",
                    ));
                }
                let capacity = a.clocks.len().checked_add(1).ok_or(TimeError::Overflow)?;
                self.rows.reserve_exact(capacity);
                self.phase = Phase::Instance(0);
                Ok(WORK)
            }
            Phase::Instance(index) => {
                let (instance, work) = a.clocks[index]
                    .try_remap_instance_with_work(a.current.instance(), maximum_work)?;
                self.phase = match instance {
                    Some(instance) if a.context.is_some() => Phase::Context(index, instance),
                    Some(instance) => Phase::Historical(index, instance, None),
                    None => self.next(index),
                };
                Ok(work)
            }
            Phase::Context(index, instance) => {
                let (context, work) = a.clocks[index].try_remap_context_with_work(
                    a.context.expect("context phase has a context"),
                    maximum_work,
                )?;
                self.phase = Phase::Historical(index, instance, context);
                Ok(work)
            }
            Phase::Historical(index, instance, context) => {
                let placement = a.clocks[index]
                    .historical_layout()
                    .sound_occurrence_placement(&instance, context.as_ref(), maximum_work)?;
                let first = *self.first_origin.get_or_insert(placement.owner_origin);
                let complete_start = a
                    .original
                    .extent
                    .start
                    .checked_add(placement.owner_origin.checked_sub(first)?)?;
                self.rows.push(AudioSoundPlacement {
                    complete_start,
                    allocation: placement.allocation,
                    anchor: placement.delivery_start.checked_sub(complete_start)?,
                    expands_history: context.is_some(),
                });
                self.phase = self.next(index);
                Ok(placement.work)
            }
            Phase::Current => {
                let placement = a.clocks[0].live_layout().sound_occurrence_placement(
                    a.current.instance(),
                    a.context.map(|context| context.owner()),
                    maximum_work,
                )?;
                if placement.allocation != a.current.extent {
                    return Err(PlanError::InvalidPlan(
                        "sound placement differs from its current occurrence",
                    ));
                }
                let complete_start =
                    a.original
                        .extent
                        .start
                        .checked_add(placement.owner_origin.checked_sub(
                            self.first_origin.ok_or(PlanError::InvalidPlan(
                                "sound has no applicable historical clock",
                            ))?,
                        )?)?;
                self.rows.push(AudioSoundPlacement {
                    complete_start,
                    allocation: placement.allocation,
                    anchor: placement.delivery_start.checked_sub(complete_start)?,
                    expands_history: a.context.is_some(),
                });
                self.phase = Phase::Complete;
                Ok(placement.work)
            }
            Phase::Complete | Phase::Poisoned => Err(PlanError::InvalidPlan(
                "sound placement cursor is not pending",
            )),
        }
    }

    fn next(&self, index: usize) -> Phase {
        if index + 1 < self.authorities.clocks.len() {
            Phase::Instance(index + 1)
        } else {
            Phase::Current
        }
    }

    pub fn finish(
        self,
    ) -> Result<AudioSoundPlacementHistory<'query, 'current, 'original>, PlanError> {
        if !self.is_complete() {
            return Err(PlanError::InvalidPlan(
                "sound placement history is incomplete",
            ));
        }
        Ok(AudioSoundPlacementHistory {
            authorities: self.authorities,
            rows: self.rows,
        })
    }
}

impl AudioSoundPlacementHistory<'_, '_, '_> {
    pub fn placements(&self) -> &[AudioSoundPlacement] {
        &self.rows
    }

    /// Exact immutable borrows need no repeated field comparisons. Equivalent
    /// cloned occurrences and fresh scope handles receive the complete semantic
    /// comparison. Both paths charge before inspecting their authorities.
    pub(super) fn validate(
        &self,
        original: &AudioSourceOccurrence<'_>,
        current: &AudioSourceOccurrence<'_>,
        clocks: &[crate::AudioSoundClockScope<'_>],
        context: &AudioSoundOwnerContext<'_>,
        maximum_work: usize,
    ) -> Result<usize, PlanError> {
        const EXACT_WORK: usize = 4;
        if maximum_work < EXACT_WORK {
            return Err(PlanError::AudioQueryLimit("sound placement authority work"));
        }
        let a = &self.authorities;
        // These four references are immutably borrowed for the proof's entire
        // lifetime. Slice identity includes its length; no owned plan or path
        // is substituted by a matching scalar or node name.
        let exact = std::ptr::eq(a.original, original)
            & std::ptr::eq(a.current, current)
            & std::ptr::eq(a.clocks, clocks)
            & a.context.is_some_and(|saved| std::ptr::eq(saved, context));
        if exact {
            return Ok(EXACT_WORK);
        }
        // Two occurrences (9 fields each), three context fields, one journal
        // length and six fields per member, plus the attempted identity checks.
        let work = a
            .clocks
            .len()
            .checked_mul(6)
            .and_then(|n| n.checked_add(22 + EXACT_WORK))
            .filter(|used| *used <= maximum_work)
            .ok_or(PlanError::AudioQueryLimit("sound placement authority work"))?;
        if !same_occurrence(a.original, original)
            || !same_occurrence(a.current, current)
            || a.context.is_none_or(|saved| {
                !std::ptr::eq(saved.plan, context.plan)
                    || saved.owner != context.owner
                    || !Arc::ptr_eq(&saved.policy, &context.policy)
            })
            || a.clocks.len() != clocks.len()
            || a.clocks.iter().zip(clocks).any(|(saved, scope)| {
                !std::ptr::eq(saved.live_plan, scope.live_plan)
                    || saved.live_owner != scope.live_owner
                    || saved.live_sound != scope.live_sound
                    || saved.journal_ordinal != scope.journal_ordinal
                    || saved.timing != scope.timing
                    || saved.recipe != scope.recipe
            })
        {
            return Err(PlanError::InvalidPlan(
                "sound placement history has foreign authorities",
            ));
        }
        Ok(work)
    }
}

fn same_occurrence(a: &AudioSourceOccurrence<'_>, b: &AudioSourceOccurrence<'_>) -> bool {
    std::ptr::eq(a.plan, b.plan)
        && a.instance == b.instance
        && a.recipe() == b.recipe()
        && a.extent == b.extent
        && a.audible_extent == b.audible_extent
        && a.routing_origin == b.routing_origin
        && a.grid == b.grid
        && a.inherited_hard_edges == b.inherited_hard_edges
        && match (&a.policy_domain, &b.policy_domain) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
}

#[cfg(test)]
#[path = "audio_sound_placement_history/tests.rs"]
mod tests;
