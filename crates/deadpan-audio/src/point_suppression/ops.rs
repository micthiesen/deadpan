//! Owned native mask destinations. Reservations retain no context borrow.
use super::{
    Charge, MaskSample, OwnedMask, StageAudioError, Storage, SuppressionMask, Value, frames_for,
    payload_frames_for,
};
use std::alloc::Layout;
use std::ops::Range;

/// Native work only; these units never redefine a Plan query receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaskWorkPhase {
    Account,
    Scan,
    Map,
    Zero,
    Mark,
    AllocateVector,
    AllocateStorage,
}

/// Shared by the native context and the lazy-account admission tail.
pub(in crate::stages) fn spend_work(
    control: super::WorkControl<'_>,
    phase: MaskWorkPhase,
    units: usize,
) -> Result<(), StageAudioError> {
    control.spend_plan_work(units)?;
    #[cfg(test)]
    crate::stages::native_query_interval_trace::accepted_work(control);
    #[cfg(test)]
    trace::spend_accepted(control.work.borrow().cache_key_scopes > 0, phase, units);
    #[cfg(not(test))]
    let _ = phase;
    Ok(())
}

/// The sole publisher reserves through the actual controller account and cap.
pub(crate) trait MaskContext {
    fn check(&self) -> Result<(), StageAudioError>;
    fn spend(&self, phase: MaskWorkPhase, units: usize) -> Result<(), StageAudioError>;
    fn reserve(&mut self, frames: u64) -> Result<Charge, StageAudioError>;
}

/// Fixed-capacity scratch; its Vec is physically destroyed before its ticket.
pub(crate) struct FlagBuffer {
    values: Vec<bool>,
    _charge: Option<Charge>,
}
impl FlagBuffer {
    pub(crate) fn as_slice(&self) -> &[bool] {
        &self.values
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [bool] {
        &mut self.values
    }
}

pub(crate) fn begin_flags<C: MaskContext>(
    context: &mut C,
    len: usize,
) -> Result<FlagBuffer, StageAudioError> {
    context.check()?;
    if len == 0 {
        return Ok(FlagBuffer {
            values: Vec::new(),
            _charge: None,
        });
    }
    let layout = Layout::array::<bool>(len).map_err(|_| StageAudioError::Range)?;
    let charge = context.reserve(frames_for(layout)?)?;
    let mut result = FlagBuffer {
        values: Vec::new(),
        _charge: Some(charge),
    };
    context.spend(MaskWorkPhase::AllocateVector, 1)?;
    result.values = Vec::with_capacity(len);
    assert_eq!(result.values.capacity(), len);
    for _ in 0..len {
        context.check()?;
        context.spend(MaskWorkPhase::Zero, 1)?;
        result.values.push(false);
    }
    context.check()?;
    Ok(result)
}

/// Its unboxed Storage already owns the complete future Vec and Box reservation.
pub(crate) struct RangeBuilder<S: MaskSample> {
    pending: Option<Storage<S>>,
}
impl<S: MaskSample> RangeBuilder<S> {
    pub(crate) fn len(&self) -> usize {
        self.pending.as_ref().map_or(0, |value| value.ranges.len())
    }

    pub(crate) fn capacity(&self) -> usize {
        self.pending
            .as_ref()
            .map_or(0, |value| value.ranges.capacity())
    }

    pub(crate) fn push<C: MaskContext>(
        &mut self,
        range: Range<S>,
        context: &C,
    ) -> Result<(), StageAudioError> {
        context.check()?;
        if self.len() >= self.capacity() {
            return Err(StageAudioError::Limit("funded suppression range capacity"));
        }
        context.spend(MaskWorkPhase::Map, 1)?;
        self.pending
            .as_mut()
            .expect("nonzero funded capacity has pending storage")
            .ranges
            .push(range);
        Ok(())
    }

    pub(crate) fn finish<C: MaskContext>(
        self,
        context: &C,
    ) -> Result<SuppressionMask<S>, StageAudioError> {
        context.check()?;
        let Some(pending) = self.pending else {
            return Ok(SuppressionMask::default());
        };
        context.spend(MaskWorkPhase::AllocateStorage, 1)?;
        let result = SuppressionMask {
            value: Value::Owned(OwnedMask {
                storage: Some(Box::new(pending)),
            }),
        };
        context.check()?;
        Ok(result)
    }
}

pub(crate) fn begin_ranges<S: MaskSample, C: MaskContext>(
    context: &mut C,
    capacity: usize,
) -> Result<RangeBuilder<S>, StageAudioError> {
    context.check()?;
    if capacity == 0 {
        return Ok(RangeBuilder { pending: None });
    }
    let charge = context.reserve(payload_frames_for::<S>(capacity)?)?;
    let mut pending = Storage {
        ranges: Vec::new(),
        charge,
    };
    context.spend(MaskWorkPhase::AllocateVector, 1)?;
    pending.ranges = Vec::with_capacity(capacity);
    assert_eq!(pending.ranges.capacity(), capacity);
    context.check()?;
    Ok(RangeBuilder {
        pending: Some(pending),
    })
}

#[cfg(test)]
#[path = "ops_trace.rs"]
pub(crate) mod trace;

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
