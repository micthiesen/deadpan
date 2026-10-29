//! Authored tile-cache capacity and additive inspection/write admission.
//! Private sorted entries retain their full allocator-reported Vec capacity.

use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;

type Entry = (i64, Arc<WideBlock>);

/// Private keys are strictly increasing. Every mutation is one checked insert.
/// The stored counter covers the full actual Vec capacity, including slack.
pub(super) struct TileIndex {
    entries: Vec<Entry>,
    capacity_frames: u64,
}

impl TileIndex {
    pub(super) fn new() -> Self {
        Self {
            entries: Vec::new(),
            capacity_frames: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Call only after eq_tile's existing control and absolute-depth checks.
    /// Borrowed lookup preserves relative-depth validation before Arc::clone.
    /// No graph borrow or lookup location may survive recursive computation.
    pub(super) fn get<const METER_WORK: bool>(
        &self,
        key: i64,
        control: WorkControl<'_>,
    ) -> Result<Option<&Arc<WideBlock>>, StageAudioError> {
        match self.search::<METER_WORK>(key, control)? {
            Ok(position) => Ok(Some(&self.entries[position].1)),
            Err(_) => Ok(None),
        }
    }

    /// One spend immediately before each actual key comparison. False mode is
    /// solely for canonical-work reconciliation: same actual vector algorithm,
    /// allocations and admission, with only these NEW spends disabled.
    fn search<const METER_WORK: bool>(
        &self,
        key: i64,
        control: WorkControl<'_>,
    ) -> Result<Result<usize, usize>, StageAudioError> {
        let mut low = 0;
        let mut high = self.entries.len();
        while low < high {
            let middle = low + (high - low) / 2;
            spend::<METER_WORK>(control, 1)?;
            match self.entries[middle].0.cmp(&key) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Equal => return Ok(Ok(middle)),
                std::cmp::Ordering::Greater => high = middle,
            }
        }
        Ok(Err(low))
    }

    #[cfg(test)]
    pub(super) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    #[cfg(test)]
    pub(super) fn capacity_frames(&self) -> u64 {
        self.capacity_frames
    }
}

/// Replacement storage must physically drop before the lease on error/drop.
/// This value is prepared LATE, after all recursive tile reads and filtering.
pub(super) struct PendingInsert<'work> {
    value: Option<Vec<Entry>>,
    key: i64,
    position: usize,
    old_len: usize,
    old_capacity: usize,
    old_layout: Layout,
    accounted_frames: u64,
    grows: bool,
    target_capacity: usize,
    target_layout: Layout,
    target_frames: u64,
    entry_operations: usize,
    lease: Option<sound_context_admission::Lease<'work>>,
}

pub(super) fn prepare<'work, const METER_WORK: bool>(
    controller: &mut StageAudio,
    current: &TileIndex,
    key: i64,
    control: WorkControl<'work>,
) -> Result<PendingInsert<'work>, StageAudioError> {
    let old_capacity = current.entries.capacity();
    let old_layout = Layout::array::<Entry>(old_capacity).map_err(|_| StageAudioError::Range)?;
    let accounted_frames = capacity_frames(old_layout)?;
    if current.capacity_frames != accounted_frames {
        return Err(StageAudioError::Range);
    }
    // Repeat lookup after recursion. An earlier miss/position is stale evidence.
    let position = match current.search::<METER_WORK>(key, control)? {
        Ok(_) => return Err(StageAudioError::Range),
        Err(position) => position,
    };
    let old_len = current.entries.len();
    let required = old_len.checked_add(1).ok_or(StageAudioError::Range)?;
    let grows = required > old_capacity;
    let target_capacity = growth_capacity(old_capacity, required)?;
    let target_layout =
        Layout::array::<Entry>(target_capacity).map_err(|_| StageAudioError::Range)?;
    let target_frames = capacity_frames(target_layout)?;
    // Growth moves every OLD entry exactly once. Spare insert shifts its suffix.
    // Both paths write the new entry once; this is a separate, explicit fee.
    let relocations = if grows {
        old_len
    } else {
        old_len
            .checked_sub(position)
            .ok_or(StageAudioError::Range)?
    };
    let entry_operations = relocations.checked_add(1).ok_or(StageAudioError::Range)?;
    let lease = if grows {
        let mut admission = sound_context_admission::Admission::new(controller, control);
        let lease = admission.reserve(target_layout)?;
        if lease.frames() != target_frames {
            return Err(StageAudioError::Range);
        }
        Some(lease)
    } else {
        None
    };
    Ok(PendingInsert {
        value: None,
        key,
        position,
        old_len,
        old_capacity,
        old_layout,
        accounted_frames,
        grows,
        target_capacity,
        target_layout,
        target_frames,
        entry_operations,
        lease,
    })
}

impl<'work> PendingInsert<'work> {
    pub(super) fn new_frames(&self) -> u64 {
        self.lease
            .as_ref()
            .map_or(0, sound_context_admission::Lease::frames)
    }

    pub(super) fn retired_frames(&self) -> u64 {
        if self.grows { self.accounted_frames } else { 0 }
    }

    #[cfg(test)]
    pub(super) fn capacity_frames_after(&self) -> u64 {
        self.target_frames
    }

    pub(super) fn lease(&self) -> Option<&sound_context_admission::Lease<'work>> {
        self.lease.as_ref()
    }

    pub(super) fn lease_mut(&mut self) -> Option<&mut sound_context_admission::Lease<'work>> {
        self.lease.as_mut()
    }

    /// No old value is moved here. Refusal leaves current storage unchanged.
    pub(super) fn allocate(&mut self) -> Result<(), StageAudioError> {
        if !self.grows {
            return Ok(());
        }
        if self.value.is_some() {
            return Err(StageAudioError::Range);
        }
        let mut replacement = Vec::new();
        replacement
            .try_reserve_exact(self.target_capacity)
            .map_err(|_| StageAudioError::Limit("Authored tile index allocation"))?;
        let actual_layout =
            Layout::array::<Entry>(replacement.capacity()).map_err(|_| StageAudioError::Range)?;
        if actual_layout != self.target_layout
            || capacity_frames(actual_layout)? != self.new_frames()
        {
            drop(replacement);
            return Err(StageAudioError::Range);
        }
        self.value = Some(replacement);
        Ok(())
    }

    /// Prepay actual relocation + new-write count. Accepted work is not refunded
    /// if a later allocation, cancellation, validation or handoff refuses.
    pub(super) fn prepay_moves<const METER_WORK: bool>(
        &self,
        control: WorkControl<'work>,
    ) -> Result<(), StageAudioError> {
        spend::<METER_WORK>(control, self.entry_operations)
    }

    /// Validate everything before drain/insert. Every neighbor key inspection is
    /// also paid. Private mutation preserves sortedness, so no free full scan.
    pub(super) fn validate_current<const METER_WORK: bool>(
        &self,
        current: &TileIndex,
        control: WorkControl<'work>,
    ) -> Result<(), StageAudioError> {
        if current.entries.len() != self.old_len
            || current.entries.capacity() != self.old_capacity
            || current.capacity_frames != self.accounted_frames
            || self.position > self.old_len
            || (self.grows && self.value.is_none())
        {
            return Err(StageAudioError::Range);
        }
        let layout = Layout::array::<Entry>(current.entries.capacity())
            .map_err(|_| StageAudioError::Range)?;
        if layout != self.old_layout || capacity_frames(layout)? != self.accounted_frames {
            return Err(StageAudioError::Range);
        }
        if let Some(replacement) = &self.value {
            let actual = Layout::array::<Entry>(replacement.capacity())
                .map_err(|_| StageAudioError::Range)?;
            if actual != self.target_layout || capacity_frames(actual)? != self.new_frames() {
                return Err(StageAudioError::Range);
            }
        }
        if let Some(left) = self.position.checked_sub(1) {
            spend::<METER_WORK>(control, 1)?;
            if current.entries[left].0 >= self.key {
                return Err(StageAudioError::Range);
            }
        }
        if self.position < self.old_len {
            spend::<METER_WORK>(control, 1)?;
            if current.entries[self.position].0 <= self.key {
                return Err(StageAudioError::Range);
            }
        }
        Ok(())
    }

    /// Infallible audited tail. Parent must finish Arc creation/validation,
    /// arithmetic, cancellation, issuer/sum checks and aggregate handoff BEFORE
    /// invoking this, then disarm the named tile leases without further failure.
    /// Calling twice or without validation is a parent invariant violation.
    pub(super) fn publish(&mut self, current: &mut TileIndex, block: Arc<WideBlock>) {
        if self.grows {
            let mut replacement = self.value.take().expect("validated tile replacement");
            let mut new_entry = Some((self.key, block));
            for (at, entry) in current.entries.drain(..).enumerate() {
                if at == self.position {
                    replacement.push(new_entry.take().expect("one tile insertion"));
                }
                replacement.push(entry);
            }
            if let Some(entry) = new_entry {
                replacement.push(entry);
            }
            std::mem::swap(&mut current.entries, &mut replacement);
            drop(replacement); // Old storage is physically free before retirement.
        } else {
            current.entries.insert(self.position, (self.key, block));
        }
        current.capacity_frames = self.target_frames;
    }
}

fn spend<const METER_WORK: bool>(
    control: WorkControl<'_>,
    amount: usize,
) -> Result<(), StageAudioError> {
    if METER_WORK {
        control.spend_plan_work(amount)?;
        #[cfg(test)]
        record_fee(amount);
    }
    Ok(())
}

fn capacity_frames(layout: Layout) -> Result<u64, StageAudioError> {
    let bytes = u64::try_from(layout.size()).map_err(|_| StageAudioError::Range)?;
    Ok(bytes.div_ceil(std::mem::size_of::<[f32; 2]>() as u64))
}

// Geometric strategy only: no independent tile-count cap is introduced.
fn growth_capacity(old_capacity: usize, required: usize) -> Result<usize, StageAudioError> {
    if required <= old_capacity {
        return Ok(old_capacity);
    }
    let target = 4_usize.max(old_capacity.checked_mul(2).ok_or(StageAudioError::Range)?);
    if target < required {
        return Err(StageAudioError::Range);
    }
    Ok(target)
}

// Test observers do not allocate, inspect keys, refund work, or change limits.
// Record only after a successful production spend. Nested measurement scopes
// return their own accepted amount and retain that amount in the outer scope.
#[cfg(test)]
std::thread_local! {
    static FEE_TRACE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn record_fee(amount: usize) {
    FEE_TRACE.with(|trace| {
        if let Some(current) = trace.get() {
            trace.set(Some(
                current
                    .checked_add(amount)
                    .expect("bounded accepted tile work"),
            ));
        }
    });
}

#[cfg(test)]
pub(super) fn with_fee_trace<R>(action: impl FnOnce() -> R) -> (R, usize) {
    struct Scope {
        previous: Option<usize>,
    }
    impl Drop for Scope {
        fn drop(&mut self) {
            FEE_TRACE.with(|trace| {
                let current = trace.replace(self.previous);
                if let (Some(previous), Some(current)) = (self.previous, current) {
                    trace.set(Some(
                        previous
                            .checked_add(current)
                            .expect("bounded nested tile work"),
                    ));
                }
            });
        }
    }
    let scope = Scope {
        previous: FEE_TRACE.with(|trace| trace.replace(Some(0))),
    };
    let result = action();
    let accepted = FEE_TRACE.with(|trace| trace.get().expect("active tile fee trace"));
    drop(scope);
    (result, accepted)
}

#[cfg(test)]
#[path = "tile_index_tests.rs"]
mod tests;
