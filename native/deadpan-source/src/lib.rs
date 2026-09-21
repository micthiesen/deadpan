//! Persistent software source decoding behind a descriptor-only FFmpeg boundary.
//!
//! The caller must supply an immutable host snapshot. Owning a regular `File`
//! keeps its descriptor alive; it does not prevent another writer changing it.
//! Calls perform I/O and allocation and belong on a media worker, never the UI or
//! audio callback. Deadlines are cooperative around FFmpeg calls, not preemptive.
//! Encoded RGB values retain their source transfer and primaries. No gamma or
//! gamut conversion, deinterlacing, tone mapping, or orientation is performed.

use std::{fs::File, sync::atomic::AtomicBool, time::Duration};

pub mod audio;

#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub max_input_bytes: u64,
    /// Frames decoded since the most recent successful seek, including scan calls.
    pub max_frames: u64,
    /// Demuxed packets since the most recent successful seek.
    pub max_packets: u64,
    pub max_io_bytes_per_call: u64,
    /// Coded/visible pixels per frame. Also bounds owned RGBA bytes to four times this value.
    pub max_pixels: u64,
    pub max_dimension: u32,
    pub max_packets_per_frame: u32,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024 * 1024,
            max_frames: 10_000_000,
            max_packets: 40_000_000,
            max_io_bytes_per_call: 256 * 1024 * 1024,
            max_pixels: 16_777_216,
            max_dimension: 8192,
            max_packets_per_frame: 10_000,
        }
    }
}

impl DecodeLimits {
    /// Check the native hard bounds before copying input or opening descriptors.
    /// The C boundary independently checks these limits before using them.
    pub fn validate(self) -> Result<(), SourceDecodeError> {
        if !(1..=64 * 1024 * 1024 * 1024).contains(&self.max_input_bytes)
            || !(1..=10_000_000).contains(&self.max_frames)
            || !(1..=40_000_000).contains(&self.max_packets)
            || !(1..=1024 * 1024 * 1024).contains(&self.max_io_bytes_per_call)
            || !(1..=8192 * 8192).contains(&self.max_pixels)
            || !(1..=8192).contains(&self.max_dimension)
            || !(1..=10_000).contains(&self.max_packets_per_frame)
        {
            return Err(SourceDecodeError::InvalidConfiguration(
                "source decode limits exceed hard bounds",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct DecodeControl<'a> {
    /// Positive, at most 60 seconds, checked on I/O and between codec operations.
    pub timeout: Duration,
    pub cancelled: &'a AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRange {
    Limited,
    Full,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMatrix {
    Rgb,
    Bt709,
    Bt601,
    Bt2020NonConstant,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTransfer {
    Bt709,
    Srgb,
    Linear,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorPrimaries {
    Bt709,
    Bt2020,
    DisplayP3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorMetadata {
    pub range: ColorRange,
    pub matrix: ColorMatrix,
    pub transfer: ColorTransfer,
    pub primaries: ColorPrimaries,
}

pub const MAX_SOURCE_AUDIO_STREAMS: usize = 32;

/// Bounded container/probe observations for one audio stream. This does not
/// establish decoded sample bounds, media validity, or editorial readiness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAudioStreamInfo {
    pub stream_index: u32,
    pub codec: String,
    pub time_base_num: u32,
    pub time_base_den: u32,
    /// Container/probe observation only, not a decoded sample boundary.
    pub stream_start: Option<i64>,
    /// Container/probe observation only, not a measured audio span.
    pub stream_duration: Option<i64>,
    pub sample_rate: Option<u32>,
    pub channel_count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceStreamInfo {
    pub width: u32,
    pub height: u32,
    pub stream_index: u32,
    pub time_base_num: u32,
    pub time_base_den: u32,
    /// Missing SAR is represented as square pixels, FFmpeg's unspecified default.
    pub sample_aspect_num: u32,
    pub sample_aspect_den: u32,
    /// Clockwise quarter turns. Pixels remain in original coded orientation.
    pub rotation_quarter_turns: u8,
    pub color: ColorMetadata,
    pub codec: String,
    pub pixel_format: String,
    /// Original stream ticks. These are observations, not trusted frame endpoints.
    pub stream_start: Option<i64>,
    pub stream_duration: Option<i64>,
    /// Original container microseconds. May include audio or other stream extents.
    pub container_start: Option<i64>,
    pub container_duration: Option<i64>,
    /// Complete bounded inventory of admitted audio streams. These are probe
    /// observations only; no audio stream was decoded or qualified as ready.
    pub audio_streams: Vec<SourceAudioStreamInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFrameMetadata {
    /// Original decoded frame PTS in `SourceStreamInfo`'s time base. Never guessed.
    pub pts: i64,
    /// Only a positive decoder-reported duration. No nominal-rate substitution.
    pub reported_duration: Option<i64>,
    pub keyframe: bool,
    pub decode_timestamp: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedRgbaFrame {
    pub metadata: SourceFrameMetadata,
    pub width: u32,
    pub height: u32,
    pub row_stride_bytes: usize,
    /// Owned packed RGBA8, full RGB range; transfer and primaries remain unchanged.
    pub rgba: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceDecodeError {
    #[error("source I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid source decode configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error("source decode {code}: {message}")]
    Native { code: String, message: String },
}

/// One persistent demuxer and decoder; each operation requires exclusive ownership.
/// Not `Sync`. Move it between workers if needed, but never call it concurrently.
/// After a decode/seek failure this session is poisoned; reopen the snapshot.
/// A pre-call cancellation or invalid timeout does not consume decoder state.
pub struct SourceDecoder {
    inner: ffi::Decoder,
    info: SourceStreamInfo,
    max_pixels: u64,
}

impl SourceDecoder {
    pub fn open(
        file: File,
        limits: DecodeLimits,
        control: DecodeControl<'_>,
    ) -> Result<Self, SourceDecodeError> {
        let (inner, info) = ffi::Decoder::open(file, limits, control)?;
        Ok(Self {
            inner,
            info,
            max_pixels: limits.max_pixels,
        })
    }
    pub fn info(&self) -> &SourceStreamInfo {
        &self.info
    }
    /// Decode one presented frame without allocating or converting RGB pixels.
    pub fn next_metadata(
        &mut self,
        control: DecodeControl<'_>,
    ) -> Result<Option<SourceFrameMetadata>, SourceDecodeError> {
        self.inner.next(control, None)
    }
    pub fn next_rgba(
        &mut self,
        control: DecodeControl<'_>,
    ) -> Result<Option<DecodedRgbaFrame>, SourceDecodeError> {
        self.rgba(control, false)
    }
    /// Convert the last decoded frame without advancing the persistent decoder.
    pub fn copy_current_rgba(
        &mut self,
        control: DecodeControl<'_>,
    ) -> Result<DecodedRgbaFrame, SourceDecodeError> {
        self.rgba(control, true)?
            .ok_or(SourceDecodeError::InvalidConfiguration("no current frame"))
    }
    fn rgba(
        &mut self,
        control: DecodeControl<'_>,
        copy: bool,
    ) -> Result<Option<DecodedRgbaFrame>, SourceDecodeError> {
        // Reject cancelled/invalid requests before allocating the owned picture.
        ffi::preflight(control)?;
        if u64::from(self.info.width) * u64::from(self.info.height) > self.max_pixels {
            return Err(SourceDecodeError::InvalidConfiguration(
                "RGBA picture exceeds the configured pixel budget",
            ));
        }
        let stride = usize::try_from(self.info.width)
            .ok()
            .and_then(|v| v.checked_mul(4))
            .ok_or(SourceDecodeError::InvalidConfiguration(
                "RGBA stride overflow",
            ))?;
        let size = usize::try_from(self.info.height)
            .ok()
            .and_then(|v| v.checked_mul(stride))
            .ok_or(SourceDecodeError::InvalidConfiguration(
                "RGBA size overflow",
            ))?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(size)
            .map_err(|_| SourceDecodeError::InvalidConfiguration("RGBA allocation failed"))?;
        rgba.resize(size, 0);
        let metadata = if copy {
            self.inner.copy(control, &mut rgba)?
        } else {
            self.inner.next(control, Some(&mut rgba))?
        };
        Ok(metadata.map(|metadata| DecodedRgbaFrame {
            metadata,
            width: self.info.width,
            height: self.info.height,
            row_stride_bytes: stride,
            rgba,
        }))
    }
    /// Backward keyframe seek in original stream ticks. Decode forward to the
    /// desired indexed PTS; the first returned frame need not be the target.
    pub fn seek(&mut self, pts: i64, control: DecodeControl<'_>) -> Result<(), SourceDecodeError> {
        self.inner.seek(pts, control)
    }
}

// All unsafe ownership and synchronous callback lifetime handling lives here.
#[allow(unsafe_code)]
mod ffi {
    use super::*;
    use std::{
        cell::Cell,
        ffi::{c_char, c_int, c_void},
        marker::PhantomData,
        os::fd::AsRawFd,
        ptr::NonNull,
        sync::atomic::Ordering,
    };

    #[repr(C)]
    struct Limits {
        max_input_bytes: u64,
        max_frames: u64,
        max_packets: u64,
        max_io_bytes_per_call: u64,
        max_pixels: u64,
        max_dimension: u32,
        max_packets_per_frame: u32,
    }
    const MAX_AUDIO_STREAMS: usize = MAX_SOURCE_AUDIO_STREAMS;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct AudioInfo {
        stream_index: i32,
        time_base_num: i32,
        time_base_den: i32,
        stream_start: i64,
        stream_duration: i64,
        sample_rate: i32,
        channel_count: i32,
        codec: [c_char; 32],
    }
    #[repr(C)]
    #[derive(Default)]
    struct Info {
        width: i32,
        height: i32,
        stream_index: i32,
        time_base_num: i32,
        time_base_den: i32,
        sar_num: i32,
        sar_den: i32,
        rotation: i32,
        range: i32,
        matrix: i32,
        transfer: i32,
        primaries: i32,
        stream_start: i64,
        stream_duration: i64,
        container_start: i64,
        container_duration: i64,
        codec: [c_char; 32],
        pixel_format: [c_char; 32],
        audio_stream_count: u32,
        audio_streams: [AudioInfo; MAX_AUDIO_STREAMS],
    }
    #[repr(C)]
    #[derive(Default)]
    struct Frame {
        pts: i64,
        duration: i64,
        dts: i64,
        keyframe: i32,
    }
    #[repr(C)]
    struct Error {
        code: [c_char; 48],
        message: [c_char; 256],
    }
    impl Default for Error {
        fn default() -> Self {
            Self {
                code: [0; 48],
                message: [0; 256],
            }
        }
    }
    impl Error {
        fn into_error(self) -> SourceDecodeError {
            SourceDecodeError::Native {
                code: string(&self.code),
                message: string(&self.message),
            }
        }
    }
    fn string(bytes: &[c_char]) -> String {
        String::from_utf8_lossy(
            &bytes
                .iter()
                .take_while(|&&v| v != 0)
                .map(|&v| v as u8)
                .collect::<Vec<_>>(),
        )
        .into_owned()
    }
    type Cancel = extern "C" fn(*const c_void) -> c_int;
    unsafe extern "C" {
        fn deadpan_source_open(
            fd: c_int,
            length: i64,
            limits: *const Limits,
            timeout_ms: u64,
            cancelled: Cancel,
            opaque: *const c_void,
            out: *mut *mut c_void,
            info: *mut Info,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_source_next(
            source: *mut c_void,
            timeout_ms: u64,
            cancelled: Cancel,
            opaque: *const c_void,
            frame: *mut Frame,
            rgba: *mut u8,
            rgba_length: usize,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_source_copy(
            source: *mut c_void,
            timeout_ms: u64,
            cancelled: Cancel,
            opaque: *const c_void,
            frame: *mut Frame,
            rgba: *mut u8,
            rgba_length: usize,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_source_seek(
            source: *mut c_void,
            pts: i64,
            timeout_ms: u64,
            cancelled: Cancel,
            opaque: *const c_void,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_source_close(source: *mut c_void);
    }
    extern "C" fn cancelled(opaque: *const c_void) -> c_int {
        // SAFETY: C only invokes this synchronously during an exported call.
        // Every call keeps the borrowed AtomicBool alive and clears its pointer
        // before returning. AtomicBool may also be set from another thread.
        i32::from(unsafe { &*opaque.cast::<AtomicBool>() }.load(Ordering::Relaxed))
    }
    fn control(value: DecodeControl<'_>) -> Result<(u64, *const c_void), SourceDecodeError> {
        if value.timeout.is_zero() || value.timeout > Duration::from_secs(60) {
            return Err(SourceDecodeError::InvalidConfiguration(
                "timeout must be positive and at most 60 seconds",
            ));
        }
        if value.cancelled.load(Ordering::Relaxed) {
            return Err(SourceDecodeError::Native {
                code: "cancelled".into(),
                message: "decode was cancelled before starting".into(),
            });
        }
        // Round upward to preserve a positive sub-millisecond deadline.
        let millis = value.timeout.as_millis()
            + u128::from(!value.timeout.subsec_nanos().is_multiple_of(1_000_000));
        Ok((
            u64::try_from(millis).expect("bounded timeout"),
            std::ptr::from_ref(value.cancelled).cast(),
        ))
    }
    pub(super) fn preflight(value: DecodeControl<'_>) -> Result<(), SourceDecodeError> {
        control(value).map(|_| ())
    }
    pub(super) struct Decoder {
        pointer: NonNull<c_void>,
        // Drop frees C state before this descriptor is closed.
        _file: File,
        _not_sync: PhantomData<Cell<()>>,
    }
    // SAFETY: all FFmpeg state is owned by this instance, no process-global
    // callbacks/state are installed, and access requires &mut self. Moving the
    // instance preserves the heap context and descriptor addresses.
    unsafe impl Send for Decoder {}
    impl Drop for Decoder {
        fn drop(&mut self) {
            unsafe { deadpan_source_close(self.pointer.as_ptr()) };
        }
    }
    impl Decoder {
        pub(super) fn open(
            file: File,
            limits: DecodeLimits,
            ctl: DecodeControl<'_>,
        ) -> Result<(Self, SourceStreamInfo), SourceDecodeError> {
            let (timeout, opaque) = control(ctl)?;
            limits.validate()?;
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Err(SourceDecodeError::InvalidConfiguration(
                    "input must be a regular file",
                ));
            }
            let length = i64::try_from(meta.len()).map_err(|_| {
                SourceDecodeError::InvalidConfiguration("input exceeds signed file offsets")
            })?;
            let limits = Limits {
                max_input_bytes: limits.max_input_bytes,
                max_frames: limits.max_frames,
                max_packets: limits.max_packets,
                max_io_bytes_per_call: limits.max_io_bytes_per_call,
                max_pixels: limits.max_pixels,
                max_dimension: limits.max_dimension,
                max_packets_per_frame: limits.max_packets_per_frame,
            };
            let mut pointer = std::ptr::null_mut();
            let mut info = Info::default();
            let mut error = Error::default();
            // SAFETY: valid bounded structs and a regular owned descriptor. C
            // owns only its allocated context, not the file; failure frees it.
            let result = unsafe {
                deadpan_source_open(
                    file.as_raw_fd(),
                    length,
                    &limits,
                    timeout,
                    cancelled,
                    opaque,
                    &mut pointer,
                    &mut info,
                    &mut error,
                )
            };
            if result != 1 {
                return Err(error.into_error());
            }
            let inner = Self {
                pointer: NonNull::new(pointer).expect("successful C open returns context"),
                _file: file,
                _not_sync: PhantomData,
            };
            let color = ColorMetadata {
                range: match info.range {
                    1 => ColorRange::Limited,
                    2 => ColorRange::Full,
                    _ => return Err(invalid_native()),
                },
                matrix: match info.matrix {
                    0 => ColorMatrix::Rgb,
                    1 => ColorMatrix::Bt709,
                    5 | 6 => ColorMatrix::Bt601,
                    9 => ColorMatrix::Bt2020NonConstant,
                    _ => return Err(invalid_native()),
                },
                transfer: match info.transfer {
                    1 => ColorTransfer::Bt709,
                    8 => ColorTransfer::Linear,
                    13 => ColorTransfer::Srgb,
                    _ => return Err(invalid_native()),
                },
                primaries: match info.primaries {
                    1 => ColorPrimaries::Bt709,
                    9 => ColorPrimaries::Bt2020,
                    12 => ColorPrimaries::DisplayP3,
                    _ => return Err(invalid_native()),
                },
            };
            let positive = |v: i32| {
                u32::try_from(v)
                    .ok()
                    .filter(|&v| v > 0)
                    .ok_or_else(invalid_native)
            };
            let audio_count = usize::try_from(info.audio_stream_count)
                .ok()
                .filter(|count| *count <= MAX_AUDIO_STREAMS)
                .ok_or_else(invalid_native)?;
            let mut stream_indices = std::collections::BTreeSet::new();
            let video_stream_index =
                u32::try_from(info.stream_index).map_err(|_| invalid_native())?;
            stream_indices.insert(video_stream_index);
            let mut audio_streams = Vec::new();
            audio_streams
                .try_reserve_exact(audio_count)
                .map_err(|_| invalid_native())?;
            for audio in &info.audio_streams[..audio_count] {
                let stream_index =
                    u32::try_from(audio.stream_index).map_err(|_| invalid_native())?;
                if !stream_indices.insert(stream_index) {
                    return Err(invalid_native());
                }
                let optional_positive = |value: i32| {
                    if value == 0 {
                        Ok(None)
                    } else {
                        positive(value).map(Some)
                    }
                };
                audio_streams.push(SourceAudioStreamInfo {
                    stream_index,
                    codec: nonempty_string(&audio.codec)?,
                    time_base_num: positive(audio.time_base_num)?,
                    time_base_den: positive(audio.time_base_den)?,
                    stream_start: timestamp(audio.stream_start),
                    stream_duration: duration(audio.stream_duration),
                    sample_rate: optional_positive(audio.sample_rate)?,
                    channel_count: optional_positive(audio.channel_count)?,
                });
            }
            let value = SourceStreamInfo {
                width: positive(info.width)?,
                height: positive(info.height)?,
                stream_index: video_stream_index,
                time_base_num: positive(info.time_base_num)?,
                time_base_den: positive(info.time_base_den)?,
                sample_aspect_num: positive(info.sar_num)?,
                sample_aspect_den: positive(info.sar_den)?,
                rotation_quarter_turns: u8::try_from(info.rotation)
                    .ok()
                    .filter(|v| *v < 4)
                    .ok_or_else(invalid_native)?,
                color,
                codec: string(&info.codec),
                pixel_format: string(&info.pixel_format),
                stream_start: timestamp(info.stream_start),
                stream_duration: duration(info.stream_duration),
                container_start: timestamp(info.container_start),
                container_duration: duration(info.container_duration),
                audio_streams,
            };
            Ok((inner, value))
        }
        pub(super) fn next(
            &mut self,
            ctl: DecodeControl<'_>,
            rgba: Option<&mut [u8]>,
        ) -> Result<Option<SourceFrameMetadata>, SourceDecodeError> {
            self.read(ctl, rgba, false)
        }
        pub(super) fn copy(
            &mut self,
            ctl: DecodeControl<'_>,
            rgba: &mut [u8],
        ) -> Result<Option<SourceFrameMetadata>, SourceDecodeError> {
            self.read(ctl, Some(rgba), true)
        }
        fn read(
            &mut self,
            ctl: DecodeControl<'_>,
            rgba: Option<&mut [u8]>,
            copy: bool,
        ) -> Result<Option<SourceFrameMetadata>, SourceDecodeError> {
            let (timeout, opaque) = control(ctl)?;
            let mut frame = Frame::default();
            let mut error = Error::default();
            let (buffer, length) =
                rgba.map_or((std::ptr::null_mut(), 0), |v| (v.as_mut_ptr(), v.len()));
            // SAFETY: this uniquely borrowed context and output buffers remain
            // alive throughout the synchronous call; C checks exact pixel size.
            let function = if copy {
                deadpan_source_copy
            } else {
                deadpan_source_next
            };
            let result = unsafe {
                function(
                    self.pointer.as_ptr(),
                    timeout,
                    cancelled,
                    opaque,
                    &mut frame,
                    buffer,
                    length,
                    &mut error,
                )
            };
            match result {
                1 => Ok(Some(SourceFrameMetadata {
                    pts: frame.pts,
                    reported_duration: duration(frame.duration),
                    keyframe: frame.keyframe != 0,
                    decode_timestamp: timestamp(frame.dts),
                })),
                0 => Ok(None),
                _ => Err(error.into_error()),
            }
        }
        pub(super) fn seek(
            &mut self,
            pts: i64,
            ctl: DecodeControl<'_>,
        ) -> Result<(), SourceDecodeError> {
            let (timeout, opaque) = control(ctl)?;
            let mut error = Error::default();
            // SAFETY: same exclusive context and synchronous control lifetime.
            if unsafe {
                deadpan_source_seek(
                    self.pointer.as_ptr(),
                    pts,
                    timeout,
                    cancelled,
                    opaque,
                    &mut error,
                )
            } != 1
            {
                return Err(error.into_error());
            }
            Ok(())
        }
    }
    fn timestamp(value: i64) -> Option<i64> {
        (value != i64::MIN).then_some(value)
    }
    fn duration(value: i64) -> Option<i64> {
        (value > 0).then_some(value)
    }
    fn nonempty_string(bytes: &[c_char]) -> Result<String, SourceDecodeError> {
        let value = string(bytes);
        if value.is_empty() {
            Err(invalid_native())
        } else {
            Ok(value)
        }
    }
    fn invalid_native() -> SourceDecodeError {
        SourceDecodeError::Native {
            code: "invalid_report".into(),
            message: "native decoder returned an invalid validated report".into(),
        }
    }
}
