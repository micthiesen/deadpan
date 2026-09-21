//! Persistent exact source-frame access from a private hash-verified snapshot.
//! All calls belong on a media service thread, not a UI or audio callback.

use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{
    AssetId, IndexedSourceFrame, MAX_SOURCE_INDEX_FRAMES, SourceFrameId, SourceFrameIndex,
    SourceTimeBase, TerminalProvenance,
};
use deadpan_source::{
    DecodeControl, DecodeLimits, DecodedRgbaFrame, SourceDecoder, SourceStreamInfo,
};

use crate::ConversionError;
use crate::conversion::Deadline;
use crate::source_index::{SourceContentIdentity, SourceIndexError, SourceIndexSnapshot};
use crate::source_input::VerifiedSourceInput;

#[derive(Debug, Clone, Copy)]
pub struct SourceSessionLimits {
    pub decode: DecodeLimits,
    pub maximum_index_frames: usize,
    pub maximum_index_bytes: usize,
    pub maximum_seek_frames: usize,
    /// One deadline covers snapshot copying, probing and the complete index scan.
    pub opening_timeout: Duration,
}

impl Default for SourceSessionLimits {
    fn default() -> Self {
        Self {
            decode: DecodeLimits::default(),
            maximum_index_frames: MAX_SOURCE_INDEX_FRAMES,
            maximum_index_bytes: 128 * 1024 * 1024,
            maximum_seek_frames: 10_000,
            opening_timeout: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceSessionError {
    #[error("invalid source session limits: {0}")]
    Limits(&'static str),
    #[error("source has no decoded video frames")]
    Empty,
    #[error("the final frame has no measured positive duration; no endpoint was invented")]
    MissingTerminalDuration,
    #[error("source frame {0:?} is outside the retained index")]
    MissingFrame(SourceFrameId),
    #[error("decoded frame metadata differs from the retained source index")]
    IndexMismatch,
    #[error("source seek exceeded its decode-frame budget")]
    SeekLimit,
    #[error(transparent)]
    Snapshot(#[from] ConversionError),
    #[error(transparent)]
    Native(#[from] deadpan_source::SourceDecodeError),
    #[error(transparent)]
    Index(#[from] SourceIndexError),
    #[error(transparent)]
    Document(#[from] deadpan_core::DocumentError),
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct SourceSession {
    decoder: SourceDecoder,
    input: VerifiedSourceInput,
    decode_limits: DecodeLimits,
    reopen_decoder: bool,
    index: SourceIndexSnapshot,
    maximum_seek_frames: usize,
    last_frame: Option<SourceFrameId>,
}

impl SourceSession {
    /// Copies finite local input, validates complete SHA-256/length, then scans
    /// original presentation metadata. No writable snapshot handle escapes.
    /// Arbitrary blocking `Read` implementations cannot be preempted; native
    /// deadlines are cooperative. The host owns scheduling and cancellation.
    pub fn open_verified(
        source: &mut impl Read,
        identity: SourceContentIdentity,
        asset: AssetId,
        limits: SourceSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, SourceSessionError> {
        Self::validate_limits(identity, limits)?;
        let deadline = Deadline {
            end: Instant::now() + limits.opening_timeout,
            cancelled,
        };
        let input = VerifiedSourceInput::copy_with_deadline(source, identity, &deadline)?;
        Self::open_with_deadline(input, asset, limits, &deadline)
    }

    /// Opens an independent video decoder over an already verified snapshot.
    /// The opening deadline covers probing and indexing; copying was performed
    /// under the snapshot's own budget. Audio and video can share these bytes.
    pub fn open_input(
        input: VerifiedSourceInput,
        asset: AssetId,
        limits: SourceSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, SourceSessionError> {
        Self::validate_limits(input.identity(), limits)?;
        let deadline = Deadline {
            end: Instant::now() + limits.opening_timeout,
            cancelled,
        };
        Self::open_with_deadline(input, asset, limits, &deadline)
    }

    fn validate_limits(
        identity: SourceContentIdentity,
        limits: SourceSessionLimits,
    ) -> Result<(), SourceSessionError> {
        limits.decode.validate()?;
        if limits.maximum_index_frames == 0
            || limits.maximum_index_frames > MAX_SOURCE_INDEX_FRAMES
            || limits.maximum_index_bytes == 0
            || limits.maximum_index_bytes > 1024 * 1024 * 1024
            || limits.maximum_seek_frames == 0
            || limits.maximum_seek_frames > MAX_SOURCE_INDEX_FRAMES
            || limits.opening_timeout.is_zero()
            || limits.opening_timeout > Duration::from_secs(24 * 60 * 60)
            || identity.byte_length() > limits.decode.max_input_bytes
        {
            return Err(SourceSessionError::Limits(
                "invalid time, byte or frame budget",
            ));
        }
        Ok(())
    }

    fn open_with_deadline(
        input: VerifiedSourceInput,
        asset: AssetId,
        limits: SourceSessionLimits,
        deadline: &Deadline<'_>,
    ) -> Result<Self, SourceSessionError> {
        deadline.check()?;
        let identity = input.identity();
        let mut decoder =
            SourceDecoder::open(input.decoder_file()?, limits.decode, control(deadline)?)?;
        let info = decoder.info();
        let time_base = SourceTimeBase::new(info.time_base_num, info.time_base_den)?;
        let stream_index = info.stream_index;
        let mut frames = Vec::new();
        let mut seek_from = None;
        let maximum_frames = limits
            .maximum_index_frames
            .min(limits.maximum_index_bytes / std::mem::size_of::<IndexedSourceFrame>());
        while let Some(frame) = decoder.next_metadata(control(deadline)?)? {
            if frames.len() >= maximum_frames {
                return Err(SourceSessionError::Limits(
                    "presentation index exceeds budget",
                ));
            }
            if frames.len() == frames.capacity() {
                frames
                    .try_reserve_exact((maximum_frames - frames.len()).min(1024))
                    .map_err(|_| {
                        SourceSessionError::Limits("presentation index allocation failed")
                    })?;
            }
            if frames
                .last()
                .is_some_and(|previous: &IndexedSourceFrame| frame.pts <= previous.pts)
            {
                return Err(SourceSessionError::IndexMismatch);
            }
            let id = SourceFrameId(frames.len() as u64);
            if frame.keyframe {
                seek_from = Some(id);
            }
            frames.push(IndexedSourceFrame {
                identity: id,
                pts: frame.pts,
                reported_duration: frame.reported_duration,
                keyframe: frame.keyframe,
                seek_from,
                decode_timestamp: frame.decode_timestamp,
            });
        }
        deadline.check()?;
        let last = frames.last().ok_or(SourceSessionError::Empty)?;
        let duration = last
            .reported_duration
            .ok_or(SourceSessionError::MissingTerminalDuration)?;
        let terminal = last
            .pts
            .checked_add(duration)
            .ok_or(SourceSessionError::IndexMismatch)?;
        let index = SourceIndexSnapshot::new(
            identity,
            stream_index,
            SourceFrameIndex::new(
                asset,
                time_base,
                frames,
                terminal,
                TerminalProvenance::DecodedFrameDuration,
            )?,
        )?;
        Ok(Self {
            decoder,
            input,
            decode_limits: limits.decode,
            reopen_decoder: false,
            index,
            maximum_seek_frames: limits.maximum_seek_frames,
            last_frame: None,
        })
    }

    pub fn info(&self) -> &SourceStreamInfo {
        self.decoder.info()
    }

    pub fn index(&self) -> &SourceIndexSnapshot {
        &self.index
    }

    /// Reuses the decoder for adjacent forward steps. Random access seeks to the
    /// indexed keyframe, decodes metadata through preroll and copies only the
    /// requested picture. Returned bytes outlive further seeks and this session.
    pub fn frame(
        &mut self,
        id: SourceFrameId,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<DecodedRgbaFrame, SourceSessionError> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(SourceSessionError::Limits(
                "seek timeout must be in (0, 60 seconds]",
            ));
        }
        let deadline = Deadline {
            end: Instant::now() + timeout,
            cancelled,
        };
        let result = self.frame_inner(id, &deadline);
        if result.is_err() {
            // Native failures can poison decode state. Preserve the private
            // input and measured index, but reopen the decoder on the next request.
            self.reopen_decoder = true;
            self.last_frame = None;
        }
        result
    }

    fn frame_inner(
        &mut self,
        id: SourceFrameId,
        deadline: &Deadline<'_>,
    ) -> Result<DecodedRgbaFrame, SourceSessionError> {
        deadline.check()?;
        if self.reopen_decoder {
            let decoder = SourceDecoder::open(
                self.input.decoder_file()?,
                self.decode_limits,
                control(deadline)?,
            )?;
            if decoder.info() != self.decoder.info() {
                return Err(SourceSessionError::IndexMismatch);
            }
            self.decoder = decoder;
            self.reopen_decoder = false;
        }
        let index = self.index.index();
        let frame = usize::try_from(id.0)
            .ok()
            .and_then(|i| index.frames().get(i))
            .ok_or(SourceSessionError::MissingFrame(id))?
            .clone();
        if self.last_frame == Some(id) {
            return Ok(self.decoder.copy_current_rgba(control(deadline)?)?);
        }
        let adjacent = self
            .last_frame
            .is_some_and(|last| last.0.checked_add(1) == Some(id.0));
        self.last_frame = None;
        if !adjacent {
            let anchor = frame.seek_from.unwrap_or(SourceFrameId(0));
            let pts = index.frames()
                [usize::try_from(anchor.0).map_err(|_| SourceSessionError::IndexMismatch)?]
            .pts;
            self.decoder.seek(pts, control(deadline)?)?;
        }
        for _ in 0..self.maximum_seek_frames {
            let decoded = self
                .decoder
                .next_metadata(control(deadline)?)?
                .ok_or(SourceSessionError::IndexMismatch)?;
            if decoded.pts < frame.pts {
                continue;
            }
            if decoded.pts != frame.pts || decoded.reported_duration != frame.reported_duration {
                return Err(SourceSessionError::IndexMismatch);
            }
            let pixels = self.decoder.copy_current_rgba(control(deadline)?)?;
            deadline.check()?;
            self.last_frame = Some(id);
            return Ok(pixels);
        }
        Err(SourceSessionError::SeekLimit)
    }
}

fn control<'a>(deadline: &Deadline<'a>) -> Result<DecodeControl<'a>, SourceSessionError> {
    deadline.check()?;
    Ok(DecodeControl {
        timeout: deadline
            .end
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(60)),
        cancelled: deadline.cancelled,
    })
}
