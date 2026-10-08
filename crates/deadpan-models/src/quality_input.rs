//! Bounded retained PNG reads shared by generated-output quality checks.

use std::io::{Read, Seek};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::{ConditioningObject, QualificationError};

const MAX_PNG_BYTES: u64 = 64 * 1024 * 1024;

fn invalid(reason: impl ToString) -> QualificationError {
    QualificationError::Quality(reason.to_string())
}

pub(crate) struct Control<'a> {
    pub(crate) deadline: Instant,
    pub(crate) cancelled: &'a AtomicBool,
}

impl Control<'_> {
    pub(crate) fn remaining(&self) -> Result<Duration, QualificationError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(QualificationError::Cancelled);
        }
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            Err(QualificationError::Deadline)
        } else {
            Ok(remaining)
        }
    }
}

pub(crate) fn read_png(
    object: &mut ConditioningObject,
    width: u32,
    height: u32,
    control: &Control<'_>,
) -> Result<image::RgbImage, QualificationError> {
    control.remaining()?;
    let length = object.object().byte_length();
    if length == 0 || length > MAX_PNG_BYTES {
        return Err(invalid("conditioning PNG exceeds the bounded read limit"));
    }
    object.rewind()?;
    let result = (|| {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(usize::try_from(length).map_err(invalid)?)
            .map_err(invalid)?;
        let mut chunk = [0; 64 * 1024];
        loop {
            control.remaining()?;
            let count = object.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            if bytes
                .len()
                .checked_add(count)
                .is_none_or(|total| total as u64 > length)
            {
                return Err(invalid("retained conditioning PNG grew during reading"));
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        if bytes.len() as u64 != length {
            return Err(invalid("retained conditioning PNG length differs"));
        }
        decode_png(&bytes, width, height, control)
    })();
    // Publication consumes the same object later. Restore it even on failure.
    object.rewind()?;
    result
}

pub(crate) fn decode_png(
    bytes: &[u8],
    width: u32,
    height: u32,
    control: &Control<'_>,
) -> Result<image::RgbImage, QualificationError> {
    control.remaining()?;
    let result = deadpan_media::conditioning_png::decode_rgb8(bytes, width, height);
    control.remaining()?;
    result.map_err(invalid)
}
