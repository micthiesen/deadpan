//! Persistent exact source-frame access from a private hash-verified snapshot.
//! All calls belong on a media service thread, not a UI or audio callback.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use deadpan_core::{
    AssetId, IndexedSourceFrame, MAX_SOURCE_INDEX_FRAMES, SourceFrameId, SourceFrameIndex,
    SourceTimeBase, TerminalProvenance,
};
use deadpan_source::{
    DecodeControl, DecodeLimits, DecodedRgbaFrame, SourceDecoder, SourceFrameMetadata,
    SourceStreamInfo,
};

use crate::ConversionError;
use crate::conversion::Deadline;
use crate::source_index::{SourceContentIdentity, SourceIndexError, SourceIndexSnapshot};
use crate::source_input::VerifiedSourceInput;

#[derive(Debug, Clone, Copy)]
pub struct SourceSessionLimits {
    /// Limits of the decoder that serves pictures, including its codec thread
    /// count. Index measurement always uses one codec thread (see `measure`).
    pub decode: DecodeLimits,
    pub maximum_index_frames: usize,
    pub maximum_index_bytes: usize,
    pub maximum_seek_frames: usize,
    /// One deadline covers snapshot copying, probing and, for complete
    /// admission, the complete index scan.
    pub opening_timeout: Duration,
    /// Deadline of a progressive admission's background measurement. It runs
    /// at utility priority and may be starved under load, so it is long and
    /// relies on cancellation when its session is dropped.
    pub measurement_timeout: Duration,
}

impl Default for SourceSessionLimits {
    /// One codec thread: import qualification, export and other offline
    /// consumers. Interactive preview uses [`SourceSessionLimits::interactive`].
    fn default() -> Self {
        Self {
            decode: DecodeLimits::default(),
            maximum_index_frames: MAX_SOURCE_INDEX_FRAMES,
            maximum_index_bytes: 128 * 1024 * 1024,
            maximum_seek_frames: 10_000,
            opening_timeout: Duration::from_secs(300),
            measurement_timeout: Duration::from_secs(24 * 60 * 60),
        }
    }
}

impl SourceSessionLimits {
    /// Interactive preview: serving decoders use [`interactive_decode_threads`].
    pub fn interactive() -> Self {
        Self::with_threads(interactive_decode_threads())
    }

    /// Interactive preview of a `width`×`height` stream: serving decoders use
    /// [`interactive_decode_threads_for`] it.
    pub fn interactive_for(width: u32, height: u32) -> Self {
        Self::with_threads(interactive_decode_threads_for(width, height))
    }

    fn with_threads(threads: u32) -> Self {
        let defaults = Self::default();
        Self {
            decode: DecodeLimits {
                threads,
                ..defaults.decode
            },
            ..defaults
        }
    }
}

/// Codec threads of one interactive serving decoder: one per core, at most
/// [`MAX_INTERACTIVE_DECODE_THREADS`]. Threading changes latency and memory,
/// never the pictures. See docs/qualification/seek-2026-10-05.md.
pub fn interactive_decode_threads() -> u32 {
    cores().clamp(1, MAX_INTERACTIVE_DECODE_THREADS)
}

/// [`interactive_decode_threads`] for a stream of this raster: a picture
/// larger than 1920×1080 uses up to [`MAX_LARGE_INTERACTIVE_DECODE_THREADS`].
pub fn interactive_decode_threads_for(width: u32, height: u32) -> u32 {
    if u64::from(width) * u64::from(height) > 1920 * 1080 {
        cores().clamp(1, MAX_LARGE_INTERACTIVE_DECODE_THREADS)
    } else {
        interactive_decode_threads()
    }
}

fn cores() -> u32 {
    std::thread::available_parallelism()
        .map_or(1, |cores| u32::try_from(cores.get()).unwrap_or(u32::MAX))
}

/// Above 1080p a long-GOP seek is decode-bound for longer: 4K30 warm seek p95
/// was 213-217 ms at 12 threads and 170-172 ms at 16 (seek qualification,
/// 2026-10-05), for about 160 MB more peak memory. Sixteen is the native
/// adapter's limit.
pub const MAX_LARGE_INTERACTIVE_DECODE_THREADS: u32 = 16;

/// Measured on 2026-10-05 (seek qualification): 1080p long-GOP warm seek p95
/// was 64-71 ms at 8 threads, 47-61 ms at 12 and 38-51 ms at 16. Twelve
/// meets the 80 ms target with margin for less memory than 16.
pub const MAX_INTERACTIVE_DECODE_THREADS: u32 = 12;

/// Progress of the complete fresh index measurement behind a progressive
/// admission. A completely measured session is always `Verified`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexMeasurement {
    /// Pictures are served after each matches the receipt's measured index;
    /// the complete fresh measurement is still running.
    Measuring,
    /// The fresh measurement equals the receipt's index and stream metadata.
    Verified,
    /// A fresh decode of these verified bytes disagrees with the receipt or
    /// cannot be decoded. Permanent for these bytes and this decoder build.
    Mismatch(String),
    /// The measurement stopped before a verdict (deadline, cancellation or
    /// I/O). A new admission may measure again.
    Interrupted(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SourceSessionError {
    #[error("a fresh decode of this source disagrees with its import record: {0}")]
    MeasurementMismatch(String),
    #[error("source index verification was interrupted: {0}")]
    MeasurementInterrupted(String),
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

impl SourceSessionError {
    /// Interruption rather than a verdict about the bytes: deadline,
    /// cancellation or I/O. Retrying with a new admission may succeed.
    pub fn is_interruption(&self) -> bool {
        match self {
            Self::MeasurementInterrupted(_)
            | Self::Io(_)
            | Self::Snapshot(ConversionError::Deadline | ConversionError::Cancelled)
            | Self::Snapshot(ConversionError::Io(_))
            | Self::Native(deadpan_source::SourceDecodeError::Io(_)) => true,
            Self::Native(deadpan_source::SourceDecodeError::Native { code, .. }) => {
                matches!(
                    code.as_str(),
                    "deadline_exceeded" | "cancelled" | "io_failure"
                )
            }
            _ => false,
        }
    }
}

pub struct SourceSession {
    decoder: SourceDecoder,
    input: VerifiedSourceInput,
    decode_limits: DecodeLimits,
    reopen_decoder: bool,
    index: Arc<SourceIndexSnapshot>,
    maximum_seek_frames: usize,
    last_frame: Option<SourceFrameId>,
    /// An incremental move of the decoder toward a later picture, decoded in
    /// bounded steps between other work (see [`SourceSession::begin_reposition`]).
    reposition: Option<Reposition>,
    /// Shared with [`SourceSession::companion`] decoders, so a mismatch or
    /// interruption fails every decoder of these bytes. The last owner's
    /// drop cancels and joins the measurement.
    measurement: Option<Arc<BackgroundMeasurement>>,
}

/// Everything needed to open another serving decoder over the same verified
/// bytes, retained index and complete-measurement state, on any thread:
/// a look-ahead decoder (see [`crate::lookahead`]). Its pictures are checked
/// against the shared index exactly as the original session's are.
#[derive(Clone)]
pub struct CompanionSpec {
    input: VerifiedSourceInput,
    index: Arc<SourceIndexSnapshot>,
    info: SourceStreamInfo,
    decode_limits: DecodeLimits,
    maximum_seek_frames: usize,
    measurement: Option<Arc<BackgroundMeasurement>>,
}

impl CompanionSpec {
    /// Open the companion decoder. Opening probes the shared private
    /// snapshot again; it does not copy, hash or measure anything.
    pub fn open(
        &self,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<SourceSession, SourceSessionError> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(SourceSessionError::Limits(
                "open timeout must be in (0, 60 seconds]",
            ));
        }
        let deadline = Deadline {
            end: Instant::now() + timeout,
            cancelled,
        };
        let decoder = SourceDecoder::open(
            self.input.decoder_file()?,
            self.decode_limits,
            control(&deadline)?,
        )?;
        if decoder.info() != &self.info || decoder.info().stream_index != self.index.stream_index()
        {
            return Err(SourceSessionError::IndexMismatch);
        }
        Ok(SourceSession {
            decoder,
            input: self.input.clone(),
            decode_limits: self.decode_limits,
            reopen_decoder: false,
            index: Arc::clone(&self.index),
            maximum_seek_frames: self.maximum_seek_frames,
            last_frame: None,
            reposition: None,
            measurement: self.measurement.clone(),
        })
    }
}

/// The state of an unfinished [`SourceSession::begin_reposition`]: the
/// decoder was positioned at `anchor` with non-reference pictures before
/// `target` skipped, and has decoded preroll through `position`.
#[derive(Clone, Copy, Debug)]
struct Reposition {
    target: SourceFrameId,
    anchor: SourceFrameId,
    position: Option<SourceFrameId>,
    decoded: usize,
}

/// How the decoder would reach a requested picture from where it stands,
/// in Original picture ordinals. Callers predict the cost of an exact
/// picture from it; [`SourceSession::frame`] follows the same plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodePlan {
    /// The decoder's current picture: converted again, nothing decoded.
    Current,
    /// Decode forward from the current picture without seeking: `pictures`
    /// pictures, the requested one last.
    Forward { pictures: u64 },
    /// Finish an unfinished reposition: `remaining` preroll ordinals before
    /// its target (non-reference pictures skipped), then `after` pictures
    /// from the target to the requested one.
    Resume { remaining: u64, after: u64 },
    /// Seek to the keyframe anchor and decode `ordinals` ordinals through the
    /// requested picture (non-reference preroll pictures skipped).
    Seek { ordinals: u64 },
}

/// Progress of one [`SourceSession::advance_reposition`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepositionProgress {
    /// Yielded before the target; `ordinals` preroll ordinals were passed.
    Pending { ordinals: u64 },
    /// The decoder's current picture is now the target.
    Reached { ordinals: u64 },
}

/// One owned thread measuring the complete index on its own single-threaded
/// decoder over the shared verified bytes. Dropping the session and its
/// companions cancels it and joins: cancellation is observed on every packet
/// read and between codec calls, so the join waits for at most one
/// single-threaded codec call. No measurement outlives every session.
struct BackgroundMeasurement {
    cancelled: Arc<AtomicBool>,
    state: Arc<(Mutex<IndexMeasurement>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for BackgroundMeasurement {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn poisoned() -> IndexMeasurement {
    IndexMeasurement::Interrupted("measurement state was poisoned".into())
}

impl BackgroundMeasurement {
    fn state(&self) -> IndexMeasurement {
        self.state
            .0
            .lock()
            .map_or_else(|_| poisoned(), |state| state.clone())
    }

    /// Block until the state leaves `Measuring` or `timeout` elapses.
    fn wait(&self, timeout: Duration) -> IndexMeasurement {
        let (lock, finished) = &*self.state;
        let Ok(state) = lock.lock() else {
            return poisoned();
        };
        finished
            .wait_timeout_while(state, timeout, |state| {
                *state == IndexMeasurement::Measuring
            })
            .map_or_else(|_| poisoned(), |(state, _)| state.clone())
    }
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
        let day = Duration::from_secs(24 * 60 * 60);
        if limits.maximum_index_frames == 0
            || limits.maximum_index_frames > MAX_SOURCE_INDEX_FRAMES
            || limits.maximum_index_bytes == 0
            || limits.maximum_index_bytes > 1024 * 1024 * 1024
            || limits.maximum_seek_frames == 0
            || limits.maximum_seek_frames > MAX_SOURCE_INDEX_FRAMES
            || limits.opening_timeout.is_zero()
            || limits.opening_timeout > day
            || limits.measurement_timeout.is_zero()
            || limits.measurement_timeout > day
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
        let mut measuring = SourceDecoder::open(
            input.decoder_file()?,
            measurement_limits(limits.decode),
            control(deadline)?,
        )?;
        let index = measure_index(&mut measuring, input.identity(), asset, limits, deadline)?;
        // Serve from a threaded decoder when configured; its pictures equal
        // the single-threaded measurement's, which defined the index.
        let decoder = if limits.decode.threads == 1 {
            measuring
        } else {
            drop(measuring);
            let decoder =
                SourceDecoder::open(input.decoder_file()?, limits.decode, control(deadline)?)?;
            if decoder.info().stream_index != index.stream_index() {
                return Err(SourceSessionError::IndexMismatch);
            }
            decoder
        };
        Ok(Self {
            decoder,
            input,
            decode_limits: limits.decode,
            reopen_decoder: false,
            index: Arc::new(index),
            maximum_seek_frames: limits.maximum_seek_frames,
            last_frame: None,
            reposition: None,
            measurement: None,
        })
    }

    /// Progressive admission for interactive preview. The caller supplies the
    /// receipt's measured index and stream metadata for these exact bytes.
    /// After copying and verifying the bytes and opening the decoder, pictures
    /// are served immediately, each only after its decoded PTS and duration
    /// (and every decoded preroll picture's) match the receipt index. A second,
    /// single-threaded decoder meanwhile measures the complete index afresh on
    /// a utility-priority background thread and compares it entry by entry;
    /// a mismatch or interruption fails the request in flight and every later
    /// one (`MeasurementMismatch` / `MeasurementInterrupted`). Export and
    /// other offline paths keep `open_verified`, which measures first.
    pub fn open_admitted(
        source: &mut impl Read,
        expected: Arc<SourceIndexSnapshot>,
        expected_info: &SourceStreamInfo,
        limits: SourceSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, SourceSessionError> {
        Self::validate_limits(expected.content(), limits)?;
        let frames = expected.index().frames().len();
        if frames > limits.maximum_index_frames
            || frames > limits.maximum_index_bytes / std::mem::size_of::<IndexedSourceFrame>()
        {
            return Err(SourceSessionError::Limits(
                "presentation index exceeds budget",
            ));
        }
        let deadline = Deadline {
            end: Instant::now() + limits.opening_timeout,
            cancelled,
        };
        let input = VerifiedSourceInput::copy_with_deadline(source, expected.content(), &deadline)?;
        Self::admitted_with_deadline(input, expected, expected_info, limits, &deadline)
    }

    /// [`Self::open_admitted`] over already verified bytes, such as the
    /// project store's private Original snapshot, without copying them again.
    /// Serving and the background measurement are otherwise identical.
    pub fn open_admitted_input(
        input: VerifiedSourceInput,
        expected: Arc<SourceIndexSnapshot>,
        expected_info: &SourceStreamInfo,
        limits: SourceSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, SourceSessionError> {
        Self::validate_limits(expected.content(), limits)?;
        let frames = expected.index().frames().len();
        if frames > limits.maximum_index_frames
            || frames > limits.maximum_index_bytes / std::mem::size_of::<IndexedSourceFrame>()
        {
            return Err(SourceSessionError::Limits(
                "presentation index exceeds budget",
            ));
        }
        if input.identity() != expected.content() {
            return Err(SourceSessionError::IndexMismatch);
        }
        let deadline = Deadline {
            end: Instant::now() + limits.opening_timeout,
            cancelled,
        };
        Self::admitted_with_deadline(input, expected, expected_info, limits, &deadline)
    }

    fn admitted_with_deadline(
        input: VerifiedSourceInput,
        expected: Arc<SourceIndexSnapshot>,
        expected_info: &SourceStreamInfo,
        limits: SourceSessionLimits,
        deadline: &Deadline<'_>,
    ) -> Result<Self, SourceSessionError> {
        let decoder =
            SourceDecoder::open(input.decoder_file()?, limits.decode, control(deadline)?)?;
        if decoder.info() != expected_info || decoder.info().stream_index != expected.stream_index()
        {
            return Err(SourceSessionError::IndexMismatch);
        }
        let measurement = BackgroundMeasurement::start(
            input.clone(),
            Arc::clone(&expected),
            expected_info.clone(),
            limits,
        )?;
        Ok(Self {
            decoder,
            input,
            decode_limits: limits.decode,
            reopen_decoder: false,
            index: expected,
            maximum_seek_frames: limits.maximum_seek_frames,
            last_frame: None,
            reposition: None,
            measurement: Some(Arc::new(measurement)),
        })
    }

    /// Serve pictures from already verified bytes against a supplied measured
    /// index, without copying or measuring again. Every returned picture and
    /// every decoded preroll picture must still equal its index entry, so a
    /// wrong index fails rather than mislabelling pictures. It establishes no
    /// whole-index verification: use it only where the index itself was
    /// verified independently, such as sampling an Original while checking a
    /// derived preview proxy.
    pub fn open_input_indexed(
        input: VerifiedSourceInput,
        expected: Arc<SourceIndexSnapshot>,
        expected_info: &SourceStreamInfo,
        limits: SourceSessionLimits,
        cancelled: &AtomicBool,
    ) -> Result<Self, SourceSessionError> {
        Self::validate_limits(input.identity(), limits)?;
        if input.identity() != expected.content() {
            return Err(SourceSessionError::IndexMismatch);
        }
        let deadline = Deadline {
            end: Instant::now() + limits.opening_timeout,
            cancelled,
        };
        let decoder =
            SourceDecoder::open(input.decoder_file()?, limits.decode, control(&deadline)?)?;
        if decoder.info() != expected_info || decoder.info().stream_index != expected.stream_index()
        {
            return Err(SourceSessionError::IndexMismatch);
        }
        Ok(Self {
            decoder,
            input,
            decode_limits: limits.decode,
            reopen_decoder: false,
            index: expected,
            maximum_seek_frames: limits.maximum_seek_frames,
            last_frame: None,
            reposition: None,
            measurement: None,
        })
    }

    /// Serve with `threads` codec threads from the next picture on, reopening
    /// the serving decoder once. Pictures do not depend on the count; use it
    /// when the raster, and so [`interactive_decode_threads_for`], is known
    /// only after opening.
    pub fn set_serving_threads(&mut self, threads: u32) -> Result<(), SourceSessionError> {
        if threads == self.decode_limits.threads {
            return Ok(());
        }
        let limits = DecodeLimits {
            threads,
            ..self.decode_limits
        };
        limits.validate()?;
        self.decode_limits = limits;
        self.reopen_decoder = true;
        self.last_frame = None;
        self.reposition = None;
        Ok(())
    }

    /// The verified private bytes this session decodes.
    pub fn input(&self) -> &VerifiedSourceInput {
        &self.input
    }

    /// The specification of a companion decoder: same bytes, index, serving
    /// limits and shared complete-measurement state.
    pub fn companion(&self) -> CompanionSpec {
        CompanionSpec {
            input: self.input.clone(),
            index: Arc::clone(&self.index),
            info: self.decoder.info().clone(),
            decode_limits: self.decode_limits,
            maximum_seek_frames: self.maximum_seek_frames,
            measurement: self.measurement.clone(),
        }
    }

    /// State of the complete fresh index measurement.
    pub fn measurement(&self) -> IndexMeasurement {
        self.measurement
            .as_ref()
            .map_or(IndexMeasurement::Verified, |measurement| {
                measurement.state()
            })
    }

    /// Wait up to `timeout` for the complete measurement to finish; the
    /// returned state is still `Measuring` if it has not.
    pub fn wait_measured(&self, timeout: Duration) -> IndexMeasurement {
        self.measurement
            .as_ref()
            .map_or(IndexMeasurement::Verified, |measurement| {
                measurement.wait(timeout)
            })
    }

    fn check_measurement(&self) -> Result<(), SourceSessionError> {
        match self.measurement() {
            IndexMeasurement::Mismatch(reason) => {
                Err(SourceSessionError::MeasurementMismatch(reason))
            }
            IndexMeasurement::Interrupted(reason) => {
                Err(SourceSessionError::MeasurementInterrupted(reason))
            }
            IndexMeasurement::Measuring | IndexMeasurement::Verified => Ok(()),
        }
    }
}

/// Index measurement always decodes with one codec thread, so a measured
/// index (receipts included) never depends on the serving thread count.
fn measurement_limits(decode: DecodeLimits) -> DecodeLimits {
    DecodeLimits {
        threads: 1,
        ..decode
    }
}

impl BackgroundMeasurement {
    fn start(
        input: VerifiedSourceInput,
        expected: Arc<SourceIndexSnapshot>,
        expected_info: SourceStreamInfo,
        limits: SourceSessionLimits,
    ) -> Result<Self, SourceSessionError> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let state = Arc::new((Mutex::new(IndexMeasurement::Measuring), Condvar::new()));
        let thread = {
            let cancelled = Arc::clone(&cancelled);
            let state = Arc::clone(&state);
            std::thread::Builder::new()
                .name("deadpan-index-measure".into())
                .spawn(move || {
                    // Interactive seeks on the serving decoder take priority.
                    deadpan_source::lower_current_thread_priority();
                    let outcome =
                        measure_against(&input, &expected, &expected_info, limits, &cancelled);
                    if let Ok(mut current) = state.0.lock() {
                        *current = outcome;
                    }
                    state.1.notify_all();
                })?
        };
        Ok(Self {
            cancelled,
            state,
            thread: Some(thread),
        })
    }
}

/// Measure afresh and compare entry by entry with the receipt's index,
/// without materializing a second copy of it.
fn measure_against(
    input: &VerifiedSourceInput,
    expected: &SourceIndexSnapshot,
    expected_info: &SourceStreamInfo,
    limits: SourceSessionLimits,
    cancelled: &AtomicBool,
) -> IndexMeasurement {
    let deadline = Deadline {
        end: Instant::now() + limits.measurement_timeout,
        cancelled,
    };
    let frames = expected.index().frames();
    let measured = (|| {
        let mut decoder = SourceDecoder::open(
            input.decoder_file()?,
            measurement_limits(limits.decode),
            control(&deadline)?,
        )?;
        if decoder.info() != expected_info
            || input.identity() != expected.content()
            || decoder.info().stream_index != expected.stream_index()
        {
            return Err(SourceSessionError::IndexMismatch);
        }
        let scanned = scan(&mut decoder, limits, &deadline, |frame| {
            if frames.get(usize::try_from(frame.identity.0).unwrap_or(usize::MAX)) == Some(&frame) {
                Ok(())
            } else {
                Err(SourceSessionError::IndexMismatch)
            }
        })?;
        let index = expected.index();
        if scanned.count != frames.len()
            || scanned.time_base != index.time_base()
            || scanned.terminal != index.terminal_end()
            || index.terminal_provenance() != TerminalProvenance::DecodedFrameDuration
        {
            return Err(SourceSessionError::IndexMismatch);
        }
        Ok(())
    })();
    match measured {
        Ok(()) => IndexMeasurement::Verified,
        Err(error) if error.is_interruption() => IndexMeasurement::Interrupted(error.to_string()),
        Err(SourceSessionError::IndexMismatch) => IndexMeasurement::Mismatch(
            "the freshly measured index or stream metadata differs from the receipt".into(),
        ),
        Err(error) => IndexMeasurement::Mismatch(error.to_string()),
    }
}

struct Scanned {
    time_base: SourceTimeBase,
    count: usize,
    terminal: i64,
}

/// Decode every presented frame's metadata, without RGB conversion, passing
/// each indexed entry to `push` in order. The final frame needs a measured
/// duration; no endpoint is invented.
fn scan(
    decoder: &mut SourceDecoder,
    limits: SourceSessionLimits,
    deadline: &Deadline<'_>,
    mut push: impl FnMut(IndexedSourceFrame) -> Result<(), SourceSessionError>,
) -> Result<Scanned, SourceSessionError> {
    let info = decoder.info();
    let time_base = SourceTimeBase::new(info.time_base_num, info.time_base_den)?;
    let temporal_fields = info.bwdif_fields;
    let maximum_frames = limits
        .maximum_index_frames
        .min(limits.maximum_index_bytes / std::mem::size_of::<IndexedSourceFrame>());
    let mut count = 0_usize;
    let mut seek_from = None;
    let mut newest_keyframe = None;
    let mut last: Option<(i64, Option<i64>)> = None;
    while let Some(frame) = decoder.next_metadata(control(deadline)?)? {
        if count >= maximum_frames {
            return Err(SourceSessionError::Limits(
                "presentation index exceeds budget",
            ));
        }
        if last.is_some_and(|(pts, _)| frame.pts <= pts) {
            return Err(SourceSessionError::IndexMismatch);
        }
        let id = SourceFrameId(count as u64);
        if frame.keyframe {
            // The first fields of a GOP need the preceding coded picture.
            // Keep an older keyframe as the measured temporal preroll anchor.
            seek_from = if temporal_fields {
                newest_keyframe.or(Some(id))
            } else {
                Some(id)
            };
            newest_keyframe = Some(id);
        }
        push(IndexedSourceFrame {
            identity: id,
            pts: frame.pts,
            reported_duration: frame.reported_duration,
            keyframe: frame.keyframe,
            seek_from,
            decode_timestamp: frame.decode_timestamp,
        })?;
        last = Some((frame.pts, frame.reported_duration));
        count += 1;
    }
    deadline.check()?;
    let (pts, duration) = last.ok_or(SourceSessionError::Empty)?;
    let duration = duration.ok_or(SourceSessionError::MissingTerminalDuration)?;
    let terminal = pts
        .checked_add(duration)
        .ok_or(SourceSessionError::IndexMismatch)?;
    Ok(Scanned {
        time_base,
        count,
        terminal,
    })
}

/// Measure the complete identity-bound index with `decoder`.
fn measure_index(
    decoder: &mut SourceDecoder,
    identity: SourceContentIdentity,
    asset: AssetId,
    limits: SourceSessionLimits,
    deadline: &Deadline<'_>,
) -> Result<SourceIndexSnapshot, SourceSessionError> {
    let stream_index = decoder.info().stream_index;
    let maximum_frames = limits
        .maximum_index_frames
        .min(limits.maximum_index_bytes / std::mem::size_of::<IndexedSourceFrame>());
    let mut frames: Vec<IndexedSourceFrame> = Vec::new();
    let scanned = scan(decoder, limits, deadline, |frame| {
        if frames.len() == frames.capacity() {
            frames
                .try_reserve_exact((maximum_frames - frames.len()).min(1024))
                .map_err(|_| SourceSessionError::Limits("presentation index allocation failed"))?;
        }
        frames.push(frame);
        Ok(())
    })?;
    Ok(SourceIndexSnapshot::new(
        identity,
        stream_index,
        SourceFrameIndex::new(
            asset,
            scanned.time_base,
            frames,
            scanned.terminal,
            TerminalProvenance::DecodedFrameDuration,
        )?,
    )?)
}

impl SourceSession {
    pub fn info(&self) -> &SourceStreamInfo {
        self.decoder.info()
    }

    pub fn index(&self) -> &SourceIndexSnapshot {
        &self.index
    }

    /// Whether `id` is a single step from the decoder's current picture:
    /// the same picture, the next or the previous one in presentation order.
    /// The next one needs no keyframe seek; the previous one does, but a
    /// step is shown exactly rather than through a coarser tier. With
    /// picture reordering even the next picture may decode several others.
    pub fn is_step(&self, id: SourceFrameId) -> bool {
        !self.reopen_decoder
            && self
                .last_frame
                .is_some_and(|last| last.0.abs_diff(id.0) <= 1)
    }

    /// How [`Self::frame`] would reach `id` from the decoder's current state.
    ///
    /// Forward decoding is ordinary sequential decoding, so it is exact from
    /// any current picture; it is chosen over a seek when it passes at most
    /// half as many ordinals (a seek skips non-reference preroll pictures,
    /// forward decoding decodes every picture) or for the next picture. An
    /// unfinished reposition continues when its target is at or before `id`
    /// and finishing it passes no more ordinals than a new seek.
    pub fn decode_plan(&self, id: SourceFrameId) -> DecodePlan {
        let Some(frame) = usize::try_from(id.0)
            .ok()
            .and_then(|position| self.index.index().frames().get(position))
        else {
            return DecodePlan::Seek { ordinals: 1 };
        };
        let anchor = frame.seek_from.unwrap_or(SourceFrameId(0));
        let seek = id.0.saturating_sub(anchor.0).saturating_add(1);
        if self.reopen_decoder {
            return DecodePlan::Seek { ordinals: seek };
        }
        if let Some(reposition) = &self.reposition
            && id.0 >= reposition.target.0
        {
            let next = reposition
                .position
                .map_or(reposition.anchor.0, |position| position.0.saturating_add(1));
            let remaining = reposition.target.0.saturating_sub(next);
            let after = id.0 - reposition.target.0 + 1;
            if remaining.saturating_add(after) <= seek {
                return DecodePlan::Resume { remaining, after };
            }
        }
        if let Some(last) = self.last_frame {
            if last == id {
                return DecodePlan::Current;
            }
            if last.0 < id.0 {
                let pictures = id.0 - last.0;
                if pictures == 1 || pictures <= seek / 2 {
                    return DecodePlan::Forward { pictures };
                }
            }
        }
        DecodePlan::Seek { ordinals: seek }
    }

    /// The decoder's current picture, which [`Self::frame`] returns without
    /// decoding; None after a failure, a seek in progress or a reposition
    /// not yet at its target.
    pub fn current(&self) -> Option<SourceFrameId> {
        if self.reopen_decoder {
            None
        } else {
            self.last_frame
        }
    }

    /// Whether the next picture first reopens the decoder after a failure
    /// or cancellation; its cost then is not the plan's alone.
    pub fn reopening(&self) -> bool {
        self.reopen_decoder
    }

    /// The target of an unfinished reposition, if any.
    pub fn repositioning(&self) -> Option<SourceFrameId> {
        self.reposition.map(|reposition| reposition.target)
    }

    /// The retained index, shared rather than copied.
    pub fn shared_index(&self) -> Arc<SourceIndexSnapshot> {
        Arc::clone(&self.index)
    }

    /// Reuses the decoder for forward steps (see [`Self::decode_plan`]).
    /// Random access seeks to the indexed keyframe, decodes metadata through
    /// preroll and copies only the requested picture. Returned bytes outlive
    /// further seeks and this session.
    /// The picture-path representation: packed RGBA8 (`sample_bits == 8`) for
    /// eight-bit sources, unchanged, and little-endian RGBA64 (`sample_bits == 16`)
    /// of the nonlinear R'G'B' for ten-bit SDR and HDR sources.
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
        self.check_measurement()?;
        let deadline = Deadline {
            end: Instant::now() + timeout,
            cancelled,
        };
        let result = self
            .frame_inner(id, &deadline)
            .and_then(|frame| self.check_measurement().map(|()| frame));
        if result.is_err() {
            self.poison();
        }
        result
    }

    /// Native failures can poison decode state. Preserve the private input
    /// and measured index, but reopen the decoder on the next request.
    fn poison(&mut self) {
        self.reopen_decoder = true;
        self.last_frame = None;
        self.reposition = None;
    }

    fn reopen_if_needed(&mut self, deadline: &Deadline<'_>) -> Result<(), SourceSessionError> {
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
        Ok(())
    }

    fn frame_inner(
        &mut self,
        id: SourceFrameId,
        deadline: &Deadline<'_>,
    ) -> Result<DecodedRgbaFrame, SourceSessionError> {
        deadline.check()?;
        self.reopen_if_needed(deadline)?;
        let index = Arc::clone(&self.index);
        let index = index.index();
        let frame = usize::try_from(id.0)
            .ok()
            .and_then(|i| index.frames().get(i))
            .ok_or(SourceSessionError::MissingFrame(id))?
            .clone();
        let plan = self.decode_plan(id);
        if plan == DecodePlan::Current {
            return self.copy_picture(deadline);
        }
        self.last_frame = None;
        let mut decoded = match plan {
            DecodePlan::Resume { .. } => self.reposition.map_or(0, |state| state.decoded),
            _ => 0,
        };
        self.reposition = None;
        if let DecodePlan::Seek { .. } = plan {
            let anchor = frame.seek_from.unwrap_or(SourceFrameId(0));
            let pts = index.frames()
                [usize::try_from(anchor.0).map_err(|_| SourceSessionError::IndexMismatch)?]
            .pts;
            // Preroll pictures before the target are decoded only as far as
            // later pictures reference them; none of them is returned.
            self.decoder.seek_to(pts, frame.pts, control(deadline)?)?;
        }
        while decoded < self.maximum_seek_frames {
            decoded += 1;
            let metadata = self
                .decoder
                .next_metadata(control(deadline)?)?
                .ok_or(SourceSessionError::IndexMismatch)?;
            if metadata.pts < frame.pts {
                check_preroll(index, &metadata)?;
                continue;
            }
            if !same_frame(&metadata, &frame) {
                return Err(SourceSessionError::IndexMismatch);
            }
            let pixels = self.copy_picture(deadline)?;
            deadline.check()?;
            self.last_frame = Some(id);
            return Ok(pixels);
        }
        Err(SourceSessionError::SeekLimit)
    }

    /// Start moving the decoder toward `target` without returning a picture:
    /// seek to its keyframe anchor with non-reference preroll skipped, then
    /// decode in [`Self::advance_reposition`] steps between other work. A
    /// later [`Self::frame`] at or after the target finishes the move; one
    /// before it seeks afresh. Every decoded picture is checked against the
    /// index exactly as in [`Self::frame`].
    pub fn begin_reposition(
        &mut self,
        target: SourceFrameId,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<(), SourceSessionError> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(SourceSessionError::Limits(
                "seek timeout must be in (0, 60 seconds]",
            ));
        }
        self.check_measurement()?;
        let deadline = Deadline {
            end: Instant::now() + timeout,
            cancelled,
        };
        let result = (|| {
            deadline.check()?;
            self.reopen_if_needed(&deadline)?;
            let index = self.index.index();
            let frame = usize::try_from(target.0)
                .ok()
                .and_then(|i| index.frames().get(i))
                .ok_or(SourceSessionError::MissingFrame(target))?;
            let anchor = frame.seek_from.unwrap_or(SourceFrameId(0));
            let pts = index.frames()
                [usize::try_from(anchor.0).map_err(|_| SourceSessionError::IndexMismatch)?]
            .pts;
            let target_pts = frame.pts;
            self.last_frame = None;
            self.reposition = None;
            self.decoder.seek_to(pts, target_pts, control(&deadline)?)?;
            self.reposition = Some(Reposition {
                target,
                anchor,
                position: None,
                decoded: 0,
            });
            Ok(())
        })();
        if result.is_err() {
            self.poison();
        }
        result
    }

    /// Decode the unfinished reposition's next pictures until its target is
    /// the current picture or `should_yield` (checked before each picture)
    /// returns true. Each step is one native decoder call, so a yield comes
    /// within one picture's decode. Fails after the session's seek budget.
    pub fn advance_reposition(
        &mut self,
        timeout: Duration,
        cancelled: &AtomicBool,
        should_yield: &mut dyn FnMut() -> bool,
    ) -> Result<RepositionProgress, SourceSessionError> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(SourceSessionError::Limits(
                "seek timeout must be in (0, 60 seconds]",
            ));
        }
        let Some(mut state) = self.reposition else {
            return Err(SourceSessionError::Limits("no reposition is in progress"));
        };
        self.check_measurement()?;
        let deadline = Deadline {
            end: Instant::now() + timeout,
            cancelled,
        };
        let index = Arc::clone(&self.index);
        let index = index.index();
        let start = state
            .position
            .map_or(state.anchor.0, |position| position.0 + 1);
        let result = (|| {
            let frame = usize::try_from(state.target.0)
                .ok()
                .and_then(|i| index.frames().get(i))
                .ok_or(SourceSessionError::MissingFrame(state.target))?;
            loop {
                let reached = state
                    .position
                    .map_or(state.anchor.0, |position| position.0 + 1);
                if should_yield() {
                    return Ok(RepositionProgress::Pending {
                        ordinals: reached - start,
                    });
                }
                if state.decoded >= self.maximum_seek_frames {
                    return Err(SourceSessionError::SeekLimit);
                }
                state.decoded += 1;
                let metadata = self
                    .decoder
                    .next_metadata(control(&deadline)?)?
                    .ok_or(SourceSessionError::IndexMismatch)?;
                if metadata.pts < frame.pts {
                    state.position = Some(check_preroll(index, &metadata)?);
                    self.reposition = Some(state);
                    continue;
                }
                if !same_frame(&metadata, frame) {
                    return Err(SourceSessionError::IndexMismatch);
                }
                self.reposition = None;
                self.last_frame = Some(state.target);
                return Ok(RepositionProgress::Reached {
                    ordinals: state.target.0 + 1 - start,
                });
            }
        })();
        let result = result.and_then(|progress| self.check_measurement().map(|()| progress));
        if result.is_err() {
            self.poison();
        }
        result
    }

    fn copy_picture(
        &mut self,
        deadline: &Deadline<'_>,
    ) -> Result<DecodedRgbaFrame, SourceSessionError> {
        Ok(
            if matches!(
                self.decoder.info().pixel_format.as_str(),
                "yuv420p10le" | "yuv422p10le"
            ) {
                self.decoder.copy_current_rgba16(control(deadline)?)?
            } else {
                self.decoder.copy_current_rgba(control(deadline)?)?
            },
        )
    }
}

/// A decoded preroll picture must be an indexed picture; its ordinal.
fn check_preroll(
    index: &SourceFrameIndex,
    decoded: &SourceFrameMetadata,
) -> Result<SourceFrameId, SourceSessionError> {
    let position = index
        .frames()
        .binary_search_by_key(&decoded.pts, |indexed| indexed.pts)
        .map_err(|_| SourceSessionError::IndexMismatch)?;
    if !same_frame(decoded, &index.frames()[position]) {
        return Err(SourceSessionError::IndexMismatch);
    }
    Ok(SourceFrameId(
        u64::try_from(position).map_err(|_| SourceSessionError::IndexMismatch)?,
    ))
}

/// Decoded picture identity equals the measured entry: PTS and reported
/// duration, which come from the picture's own packet. The key flag and decode
/// timestamp are excluded: H.264 reports the DTS of the packet that released a
/// reordered picture, and recovery-point key marking depends on where decoding
/// started, so both can differ after a seek or skipped preroll.
fn same_frame(decoded: &SourceFrameMetadata, indexed: &IndexedSourceFrame) -> bool {
    decoded.pts == indexed.pts && decoded.reported_duration == indexed.reported_duration
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
