//! Tape traversal shares one work authority and retains each child's guards.
use super::*;
use crate::{AdmittedSignalPolicyQuery, AudioSignalTape};

impl AudioSignalTape<'_> {
    /// Policy on the Tape's stored grid with independent construction admission.
    pub fn policy_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        self.policy_admitted_inner(samples, limits, admission, false)
    }

    /// Explicit silence after Preserve, retaining the Tape's stored grid.
    pub fn policy_after_preserve_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> std::result::Result<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        self.policy_admitted_inner(samples, limits, admission, true)
    }

    fn policy_admitted_inner<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
        after_preserve: bool,
    ) -> std::result::Result<AdmittedSignalPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let query = if after_preserve {
                self.policy_after_preserve(samples, limits)
            } else {
                self.policy(samples, limits)
            }
            .map_err(A::Error::from)?;
            return outer::<_, A>(point::ordinary_tape(query, admission));
        }
        limits.validate()?;
        let mut local = LocalBudget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        outer::<_, A>((|| {
            let (state, ranges) = tape_policy(self, samples, limits, after_preserve, &mut budget)?;
            point::finish_tape(state, ranges, limits, &mut budget)
        })())
    }
}

fn tape_spend<A: AudioSoundContextAdmission>(
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    if budget.remaining == 0 {
        return Err(PlanError::AudioQueryLimit("audio tape work").into());
    }
    budget.canonical(1)
}

fn run_for<A: AudioSoundContextAdmission>(
    tape: &AudioSignalTape<'_>,
    sample: SignalSample,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<usize, A> {
    let mut left = 0;
    let mut right = tape.policy_run_count();
    while left < right {
        tape_spend(budget)?;
        let middle = left + (right - left) / 2;
        let run = tape.policy_run(middle).expect("observed Tape run");
        if run.samples.end <= sample {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    Ok(left)
}

fn extend_inventory<A: AudioSoundContextAdmission>(
    state: &mut State<A>,
    child: Inventory<A>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    struct Drain<I, L> {
        values: std::vec::IntoIter<AudioContent>,
        _capacity: Memory<I, L>,
    }
    let child = child.map(budget, |child| Drain {
        values: child.spans.into_iter(),
        _capacity: child.capacity,
    })?;
    let inventory = state
        .inventory
        .take()
        .expect("Tape inventory before child extension");
    let mut current = child
        .zip(inventory, budget)?
        .map(budget, |(query, inventory)| Current { query, inventory })?;
    while !current.query.values.as_slice().is_empty() {
        current = grow_inventory(current, budget)?;
        budget.additional(1)?;
        current = current.map(budget, |mut current| {
            current
                .inventory
                .spans
                .push(current.query.values.next().expect("observed child content"));
            current
        })?;
    }
    state.inventory = Some(current.map(budget, |current| current.inventory)?);
    Ok(())
}

fn add_tape_lookup(total: &mut LookupStats, next: LookupStats) -> Result<(), PlanError> {
    total.visited_nodes = total
        .visited_nodes
        .checked_add(next.visited_nodes)
        .ok_or(PlanError::AudioQueryLimit("audio tape lookup count"))?;
    total.sequence_comparisons = total
        .sequence_comparisons
        .checked_add(next.sequence_comparisons)
        .ok_or(PlanError::AudioQueryLimit("audio tape lookup count"))?;
    total.iteration_run_comparisons = total
        .iteration_run_comparisons
        .checked_add(next.iteration_run_comparisons)
        .ok_or(PlanError::AudioQueryLimit("audio tape lookup count"))?;
    Ok(())
}

fn tape_policy<A: AudioSoundContextAdmission>(
    tape: &AudioSignalTape<'_>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    after_preserve: bool,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(State<A>, Ranges<A>), A> {
    tape.validate_policy_range(&samples, limits)?;
    let mut state = State::new(limits.maximum_spans, budget);
    let mut ranges = empty_ranges(budget);
    let mut cursor = samples.start;
    while cursor < samples.end {
        let index = run_for(tape, cursor, budget)?;
        let run = tape.policy_run(index).ok_or(PlanError::InvalidPlan(
            "audio tape policy sample is uncovered",
        ))?;
        let end = samples.end.min(run.samples.end);
        if cursor < run.samples.start || end <= cursor {
            return Err(PlanError::InvalidPlan("audio tape policy did not advance").into());
        }
        tape_spend(budget)?;
        if budget.remaining == 0 {
            return Err(PlanError::AudioQueryLimit("audio tape policy work").into());
        }
        let child_limits = AudioQueryLimits {
            maximum_spans: limits.maximum_spans,
            maximum_work: budget.remaining,
        };
        // Ordinary Tape folds lookup only after a complete child. Work and
        // admission keep the same authority even while lookup is child-local.
        let mut parent_lookup = std::mem::take(&mut budget.lookup);
        let checkpoint = budget.checkpoint();
        let child: QueryResult<_, A> = (|| {
            let (child_state, child_ranges) = if let Some(view) = run.policy_override {
                tape_policy(view, cursor..end, child_limits, true, budget)?
            } else {
                let mut child_state = State::new(limits.maximum_spans, budget);
                let child_ranges = signal_policy(
                    run.signal,
                    cursor.0..end.0,
                    tape.grid().boundary_rule(),
                    &mut child_state,
                    budget,
                    0,
                    !after_preserve,
                )?;
                (child_state, child_ranges)
            };
            let child_ranges = point::normalize_signal_ranges(child_ranges, budget)?;
            let usage = budget.usage_since(checkpoint)?;
            Ok((child_state, child_ranges, usage))
        })();
        let child_lookup = std::mem::replace(&mut budget.lookup, parent_lookup);
        let (mut child_state, child_ranges, usage) = child?;
        if usage.lookup != child_lookup {
            return Err(PlanError::InvalidPlan("Tape child lookup provenance mismatch").into());
        }
        parent_lookup = budget.lookup;
        add_tape_lookup(&mut parent_lookup, child_lookup)?;
        budget.lookup = parent_lookup;
        extend_ranges(&mut ranges, child_ranges, budget)?;
        extend_inventory(
            &mut state,
            child_state
                .inventory
                .take()
                .expect("completed Tape child inventory"),
            budget,
        )?;
        cursor = end;
    }
    Ok((state, ranges))
}
