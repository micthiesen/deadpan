//! Point policy final-mask destination preparation and allocation custody transfer.
use super::{
    MaskWorkPhase, OwnedMask, SignalSample, SignalSuppressionMask, StageAudio, StageAudioError,
    Storage, SuppressionMask, Value, frames_for,
};
use crate::stages::WorkControl;
use std::alloc::Layout;
use std::ops::Range;
use std::sync::atomic::Ordering;

/// Owns the existing charged destination Box until Plan either commits the
/// exact final Vec or drops this token on a preflight failure.
pub(in crate::stages) struct PreparedPolicyMask {
    value: PreparedValue,
}

enum PreparedValue {
    Empty,
    Owned {
        owner: OwnedMask<SignalSample>,
        vector_frames: u64,
        total_frames: u64,
    },
}

impl PreparedPolicyMask {
    pub(in crate::stages) fn vector_frames(&self) -> u64 {
        match &self.value {
            PreparedValue::Empty => 0,
            PreparedValue::Owned { vector_frames, .. } => *vector_frames,
        }
    }

    /// Infallible publication after all fallible work and lease checks.
    pub(in crate::stages) fn publish(
        self,
        ranges: Vec<Range<SignalSample>>,
    ) -> SignalSuppressionMask {
        match self.value {
            PreparedValue::Empty => SuppressionMask {
                value: Value::Plain(ranges),
            },
            PreparedValue::Owned {
                mut owner,
                vector_frames,
                total_frames,
            } => {
                // prepare_policy_mask is the only constructor of this private
                // variant and always installs Some(Box<Storage>) before it
                // returns. Resolve this access before changing any accounting.
                let storage = owner
                    .storage
                    .as_mut()
                    .expect("prepared policy mask owns its Storage");
                storage
                    .charge
                    .handle
                    .data()
                    .live_frames
                    .fetch_add(vector_frames, Ordering::AcqRel);
                storage.charge.frames = total_frames;
                storage.ranges = ranges;
                SuppressionMask {
                    value: Value::Owned(owner),
                }
            }
        }
    }
}

pub(in crate::stages) fn vector_frames(capacity: usize) -> Result<u64, StageAudioError> {
    let layout =
        Layout::array::<Range<SignalSample>>(capacity).map_err(|_| StageAudioError::Range)?;
    frames_for(layout)
}

pub(in crate::stages) fn prepare_policy_mask(
    renderer: &mut StageAudio,
    capacity: usize,
    control: WorkControl<'_>,
) -> Result<PreparedPolicyMask, StageAudioError> {
    if capacity == 0 {
        return Ok(PreparedPolicyMask {
            value: PreparedValue::Empty,
        });
    }

    control.check()?;
    let vector_frames = vector_frames(capacity)?;
    let storage_frames = frames_for(Layout::new::<Storage<SignalSample>>())?;
    let total_frames = storage_frames
        .checked_add(vector_frames)
        .ok_or(StageAudioError::Range)?;

    // This admission accounts only for the new Box<Storage>; the exact source
    // Vec remains separately charged by its Plan lease until publication.
    let charge = renderer.reserve_mask_storage(storage_frames, control)?;
    renderer
        .owned_suppression
        .live_frames()
        .checked_add(vector_frames)
        .ok_or(StageAudioError::Range)?;

    super::ops::spend_work(control, MaskWorkPhase::AllocateStorage, 1)?;
    let owner = OwnedMask {
        storage: Some(Box::new(Storage {
            ranges: Vec::new(),
            charge,
        })),
    };
    Ok(PreparedPolicyMask {
        value: PreparedValue::Owned {
            owner,
            vector_frames,
            total_frames,
        },
    })
}
