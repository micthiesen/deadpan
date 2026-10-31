//! Actual finite copies beneath the shared capture walker. Canonical copy work
//! is prepaid at the original aggregate call sites; these helpers add only new
//! metered inspections and reservations.
use super::*;
use crate::AudioSoundContextAdmission;
use crate::audio_allocated::admission::array;
use crate::plan::audio_owners::admission::{Budget as QueryBudget, Memory, Result};

pub(super) fn copy_vec<T: Clone, A: AudioSoundContextAdmission>(
    values: &[T],
    memory: &mut Memory<A::Issuer, A::Lease>,
    capacity: &mut Memory<A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
    mut copy: impl FnMut(&T, &mut Memory<A::Issuer, A::Lease>, &mut QueryBudget<'_, A>) -> Result<T, A>,
) -> Result<Vec<T>, A> {
    budget.check_memory(memory)?;
    if !A::METERED {
        return Ok(values.to_vec());
    }
    budget.reserve(capacity, array::<T>(values.len())?)?;
    let mut result = Vec::with_capacity(values.len());
    assert_eq!(result.capacity(), values.len());
    for value in values {
        budget.additional(1)?;
        result.push(copy(value, memory, budget)?);
    }
    Ok(result)
}
pub(in crate::plan) fn fixed<T: Clone, A: AudioSoundContextAdmission>(
    values: &[T],
    memory: &mut Memory<A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
    copy: impl FnMut(&T, &mut Memory<A::Issuer, A::Lease>, &mut QueryBudget<'_, A>) -> Result<T, A>,
) -> Result<Vec<T>, A> {
    let mut capacity = budget.memory();
    let result = copy_vec(values, memory, &mut capacity, budget, copy)?;
    budget.check_memory(memory)?;
    budget.check_memory(&capacity)?;
    budget.merge_checked(memory, &mut capacity)?;
    Ok(result)
}
pub(in crate::plan) fn editorial<A: AudioSoundContextAdmission>(
    value: &EditorialConstraint,
    memory: &mut Memory<A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> Result<EditorialConstraint, A> {
    let EditorialConstraint {
        boundary,
        edges,
        incident_samples,
    } = value;
    Ok(EditorialConstraint {
        boundary: budget.constraint(memory, boundary)?,
        edges: *edges,
        incident_samples: *incident_samples,
    })
}
pub(in crate::plan) fn retime<A: AudioSoundContextAdmission>(
    value: &super::super::AudioRetimeStage,
    memory: &mut Memory<A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> Result<super::super::AudioRetimeStage, A> {
    let super::super::AudioRetimeStage {
        node,
        child_start,
        child_frames_per_local_frame,
        pitch,
    } = value;
    Ok(super::super::AudioRetimeStage {
        node: budget.node(memory, node)?,
        child_start: *child_start,
        child_frames_per_local_frame: *child_frames_per_local_frame,
        pitch: *pitch,
    })
}
pub(in crate::plan) fn hold<A: AudioSoundContextAdmission>(
    value: &HoldAudio,
    duration: FrameDuration,
    memory: &mut Memory<A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> Result<AudioContent, A> {
    if !A::METERED {
        return Ok(AudioContent::from_hold(value, duration));
    }
    budget.additional(1)?;
    Ok(match value {
        HoldAudio::Silence => AudioContent::Silence {
            reason: SilenceReason::SilentHold,
        },
        HoldAudio::RoomTone { source } => AudioContent::RoomTone {
            source: budget.source(memory, source)?,
            duration,
        },
        HoldAudio::Tail {
            maximum,
            effect,
            input,
        } => AudioContent::Tail {
            maximum: *maximum,
            effect: *effect,
            input: *input,
            duration,
        },
        HoldAudio::Reverse { source } => AudioContent::Reverse {
            source: budget.source(memory, source)?,
            duration,
        },
        HoldAudio::Tone {
            frequency_hz,
            level,
        } => AudioContent::Tone {
            frequency_hz: *frequency_hz,
            level: *level,
            duration,
        },
    })
}
