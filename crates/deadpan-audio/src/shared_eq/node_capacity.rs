//! Request-local admission for growing the shared EQ node container.
//!
//! The replacement allocation is deliberately deferred until after native
//! filter construction. Its capacity is admitted before that work begins.

use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;

/// A prepared replacement allocation for the shared EQ node vector.
///
/// Keep `value` before `lease`: on every error or drop path, the allocation is
/// released before its capacity admission is returned to the request ledger.
pub(super) struct PendingNodeCapacity<'work, T> {
    value: Option<Vec<T>>,
    old_len: usize,
    old_capacity: usize,
    old_layout: Layout,
    accounted_frames: u64,
    target_capacity: usize,
    target_layout: Layout,
    target_frames: u64,
    lease: Option<sound_context_admission::Lease<'work>>,
}

pub(super) fn prepare<'work, T>(
    controller: &mut StageAudio,
    current: &Vec<T>,
    accounted_frames: u64,
    maximum: usize,
    control: WorkControl<'work>,
) -> Result<PendingNodeCapacity<'work, T>, StageAudioError> {
    if std::mem::size_of::<T>() == 0 {
        return Err(StageAudioError::Unsupported("zero-sized shared EQ node"));
    }

    let old_layout = Layout::array::<T>(current.capacity()).map_err(|_| StageAudioError::Range)?;
    let old_frames = capacity_frames(old_layout)?;
    if old_frames != accounted_frames {
        return Err(StageAudioError::Range);
    }

    let required_capacity = current.len().checked_add(1).ok_or(StageAudioError::Range)?;
    if required_capacity > maximum || current.capacity() > maximum {
        return Err(StageAudioError::Limit("temporal owner count"));
    }

    let grows = required_capacity > current.capacity();
    let target_capacity = if grows {
        let doubled = current
            .capacity()
            .checked_mul(2)
            .ok_or(StageAudioError::Range)?;
        4_usize.max(doubled).min(maximum)
    } else {
        current.capacity()
    };
    if target_capacity < required_capacity {
        return Err(StageAudioError::Limit("temporal owner count"));
    }

    let target_layout = Layout::array::<T>(target_capacity).map_err(|_| StageAudioError::Range)?;
    let target_frames = capacity_frames(target_layout)?;
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

    Ok(PendingNodeCapacity {
        value: None,
        old_len: current.len(),
        old_capacity: current.capacity(),
        old_layout,
        accounted_frames,
        target_capacity,
        target_layout,
        target_frames,
        lease,
    })
}

impl<'work, T> PendingNodeCapacity<'work, T> {
    pub(super) fn new_frames(&self) -> u64 {
        self.lease
            .as_ref()
            .map_or(0, sound_context_admission::Lease::frames)
    }

    pub(super) fn retired_frames(&self) -> u64 {
        if self.lease.is_some() {
            self.accounted_frames
        } else {
            0
        }
    }

    pub(super) fn capacity_frames_after(&self) -> u64 {
        if self.lease.is_some() {
            self.target_frames
        } else {
            self.accounted_frames
        }
    }

    pub(super) fn lease(&self) -> Option<&sound_context_admission::Lease<'work>> {
        self.lease.as_ref()
    }

    pub(super) fn lease_mut(&mut self) -> Option<&mut sound_context_admission::Lease<'work>> {
        self.lease.as_mut()
    }

    /// Allocate the admitted replacement storage only when the node vector
    /// must grow. The actual allocator capacity is checked before it escapes.
    pub(super) fn allocate(&mut self) -> Result<(), StageAudioError> {
        if self.lease.is_none() {
            return Ok(());
        }
        if self.value.is_some() {
            return Err(StageAudioError::Range);
        }
        let replacement = Vec::with_capacity(self.target_capacity);
        let actual_layout =
            Layout::array::<T>(replacement.capacity()).map_err(|_| StageAudioError::Range)?;
        if actual_layout != self.target_layout
            || capacity_frames(actual_layout)? != self.new_frames()
        {
            drop(replacement);
            return Err(StageAudioError::Range);
        }
        self.value = Some(replacement);
        Ok(())
    }

    /// Charge the actual values that will be moved out of the old allocation.
    pub(super) fn prepay_moves(&self, control: WorkControl<'work>) -> Result<(), StageAudioError> {
        if self.lease.is_some() && self.old_len != 0 {
            control.spend_plan_work(self.old_len)?;
        }
        Ok(())
    }

    pub(super) fn validate_current(
        &self,
        current: &Vec<T>,
        accounted_frames: u64,
    ) -> Result<(), StageAudioError> {
        if current.len() != self.old_len
            || current.capacity() != self.old_capacity
            || accounted_frames != self.accounted_frames
        {
            return Err(StageAudioError::Range);
        }
        if self.lease.is_some() && self.value.is_none() {
            return Err(StageAudioError::Range);
        }
        let layout = Layout::array::<T>(current.capacity()).map_err(|_| StageAudioError::Range)?;
        if layout != self.old_layout || capacity_frames(layout)? != accounted_frames {
            return Err(StageAudioError::Range);
        }
        Ok(())
    }

    /// Infallible publication tail. The caller preflights and invokes this
    /// only after the filter and every other fallible operation has succeeded.
    pub(super) fn publish(&mut self, current: &mut Vec<T>, node: T) {
        if let Some(mut replacement) = self.value.take() {
            for value in current.drain(..) {
                replacement.push(value);
            }
            replacement.push(node);
            std::mem::swap(current, &mut replacement);
            drop(replacement);
        } else {
            current.push(node);
        }
    }
}

#[cfg(test)]
#[path = "node_capacity_tests.rs"]
mod tests;

fn capacity_frames(layout: Layout) -> Result<u64, StageAudioError> {
    let bytes = u64::try_from(layout.size()).map_err(|_| StageAudioError::Range)?;
    Ok(bytes.div_ceil(std::mem::size_of::<[f32; 2]>() as u64))
}
