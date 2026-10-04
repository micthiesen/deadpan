//! Decode every picture of a verified source once, in presentation order.
//!
//! Analyses that measure all pictures (shot detection) open a fresh native
//! decoder over the private verified snapshot and visit each converted RGBA
//! picture with its qualified index ordinal. Every decoded picture must carry
//! exactly the PTS and duration its ordinal has in the qualified index, and
//! the stream must end exactly after the last indexed picture; a mismatch,
//! gap or extra picture fails explicitly instead of being guessed around.
//! Pictures are not retained: the caller keeps whatever it reduces them to.
//! Call this on a media worker thread, never a UI or audio callback.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_source::{
    DecodeControl, DecodeLimits, DecodedRgbaFrame, SourceDecodeError, SourceDecoder,
};

use crate::source_input::VerifiedSourceInput;
use crate::source_qualification::QualifiedVideoSnapshot;

#[derive(Debug, thiserror::Error)]
pub enum PictureScanError {
    #[error("picture scan cancelled")]
    Cancelled,
    #[error("picture scan deadline elapsed")]
    Deadline,
    #[error("decoded pictures differ from the qualified index: {0}")]
    Mismatch(String),
    #[error("picture visitor failed: {0}")]
    Visit(String),
    #[error(transparent)]
    Native(#[from] SourceDecodeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Decode all pictures of `input` against its qualified `video` snapshot,
/// calling `visit(ordinal, picture)` for each in order. Returns the number of
/// pictures visited, which always equals the qualified index length.
pub fn scan_pictures<E: std::fmt::Display>(
    input: &VerifiedSourceInput,
    video: &QualifiedVideoSnapshot,
    limits: DecodeLimits,
    deadline: Instant,
    cancelled: &AtomicBool,
    mut visit: impl FnMut(usize, &DecodedRgbaFrame) -> Result<(), E>,
) -> Result<usize, PictureScanError> {
    let control = || -> Result<DecodeControl<'_>, PictureScanError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(PictureScanError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(PictureScanError::Deadline);
        }
        Ok(DecodeControl {
            timeout: remaining.min(Duration::from_secs(60)),
            cancelled,
        })
    };
    // A native failure observed after cancellation or the deadline reports
    // that cause rather than the decoder's own interruption code.
    let native = |error: SourceDecodeError| -> PictureScanError {
        if cancelled.load(Ordering::Acquire) {
            PictureScanError::Cancelled
        } else if Instant::now() >= deadline {
            PictureScanError::Deadline
        } else {
            PictureScanError::Native(error)
        }
    };
    let index = video.index();
    if input.identity() != index.content() {
        return Err(PictureScanError::Mismatch(
            "the snapshot is not the qualified content".into(),
        ));
    }
    let mut decoder =
        SourceDecoder::open(input.decoder_file()?, limits, control()?).map_err(native)?;
    let info = decoder.info();
    let expected = video.interpretation();
    if info.stream_index != index.stream_index()
        || info.stream_index != expected.stream_index
        || (info.width, info.height) != (expected.width, expected.height)
        || (info.time_base_num, info.time_base_den)
            != (expected.time_base_num, expected.time_base_den)
    {
        return Err(PictureScanError::Mismatch(
            "the decoder selected a different picture stream or interpretation".into(),
        ));
    }
    let frames = index.index().frames();
    let mut ordinal = 0;
    while let Some(picture) = decoder.next_rgba(control()?).map_err(native)? {
        let Some(indexed) = frames.get(ordinal) else {
            return Err(PictureScanError::Mismatch(format!(
                "picture {ordinal} decoded beyond the {} indexed pictures",
                frames.len()
            )));
        };
        if picture.metadata.pts != indexed.pts
            || picture.metadata.reported_duration != indexed.reported_duration
        {
            return Err(PictureScanError::Mismatch(format!(
                "picture {ordinal} decoded at PTS {} (duration {:?}), indexed at PTS {} (duration {:?})",
                picture.metadata.pts,
                picture.metadata.reported_duration,
                indexed.pts,
                indexed.reported_duration
            )));
        }
        visit(ordinal, &picture).map_err(|error| PictureScanError::Visit(error.to_string()))?;
        ordinal += 1;
    }
    if ordinal != frames.len() {
        return Err(PictureScanError::Mismatch(format!(
            "decoding ended after {ordinal} of {} indexed pictures",
            frames.len()
        )));
    }
    Ok(ordinal)
}
