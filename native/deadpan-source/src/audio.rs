//! Linear source audio decoding through a persistent descriptor-only FFmpeg context.
//!
//! Supply an immutable host snapshot. Operations perform I/O and allocation and
//! belong on a worker, never the audio callback. Cancellation and deadlines are
//! cooperative. The decoder preserves source rate and channel order, performs no
//! resampling, downmix, gain, clipping, or automatic skip/padding removal, and
//! never derives a terminal endpoint from a declared container duration.
//!
//! FFmpeg's `AV_CODEC_FLAG2_SKIP_MANUAL` returns untrimmed frames with its reported
//! skip side data. That evidence is exposed separately from decoded sample counts.
//! A zero skip record, absent skip record, codec padding observation, or container
//! duration is not by itself proof that every decoded sample is presentation data.
//!
//! Repository fixtures qualify AAC-LC/MP4 and signed16 little-endian PCM/WAVE.
//! A strict MP4/WAV header guard rejects unsafe size/table declarations before
//! FFmpeg allocation. Other container grammars require separate qualification.

use std::{fs::File, time::Instant};

use crate::{DecodeControl, SourceDecodeError};

#[path = "audio_input.rs"]
mod input;

#[derive(Clone, Copy, Debug)]
pub struct AudioDecodeLimits {
    pub max_input_bytes: u64,
    pub max_frames: u64,
    pub max_packets: u64,
    /// Total decoded samples per channel, including priming and padding.
    pub max_decoded_samples: u64,
    pub max_io_bytes_per_call: u64,
    pub max_samples_per_frame: u32,
    pub max_channels: u32,
    pub max_sample_rate: u32,
    pub max_packets_per_frame: u32,
    /// MP4 sample sizes are checked before demuxing; WAV packet sizes are capped.
    /// Returned payload plus side data is checked again before codec submission.
    pub max_packet_bytes: u32,
}

impl Default for AudioDecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024 * 1024,
            max_frames: 10_000_000,
            max_packets: 40_000_000,
            max_decoded_samples: 1_000_000_000_000,
            max_io_bytes_per_call: 256 * 1024 * 1024,
            max_samples_per_frame: 65_536,
            max_channels: 32,
            max_sample_rate: 384_000,
            max_packets_per_frame: 10_000,
            max_packet_bytes: 16 * 1024 * 1024,
        }
    }
}

impl AudioDecodeLimits {
    /// Validate hard bounds before copying a source snapshot or opening a decoder.
    pub fn validate(self) -> Result<(), SourceDecodeError> {
        if !(1..=64 * 1024 * 1024 * 1024).contains(&self.max_input_bytes)
            || !(1..=10_000_000).contains(&self.max_frames)
            || !(1..=40_000_000).contains(&self.max_packets)
            || !(1..=1_000_000_000_000).contains(&self.max_decoded_samples)
            || !(1..=1024 * 1024 * 1024).contains(&self.max_io_bytes_per_call)
            || !(1..=65_536).contains(&self.max_samples_per_frame)
            || !(1..=32).contains(&self.max_channels)
            || !(1..=384_000).contains(&self.max_sample_rate)
            || !(1..=10_000).contains(&self.max_packets_per_frame)
            || !(1..=16 * 1024 * 1024).contains(&self.max_packet_bytes)
        {
            return Err(SourceDecodeError::InvalidConfiguration(
                "audio decode limits exceed hard bounds",
            ));
        }
        Ok(())
    }
}

/// Source channel interpretation. An unspecified layout preserves channel slots
/// without assigning speakers. Native masks use FFmpeg's AVChannel speaker bits,
/// in ascending bit order. Custom and ambisonic orders are not qualified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioChannelLayout {
    Unspecified { channels: u32 },
    Native { channels: u32, mask: u64 },
}

impl AudioChannelLayout {
    pub fn channels(self) -> u32 {
        match self {
            Self::Unspecified { channels } | Self::Native { channels, .. } => channels,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioStreamInfo {
    pub stream_index: u32,
    pub codec: String,
    pub time_base_num: u32,
    pub time_base_den: u32,
    pub sample_rate: u32,
    /// Opened decoder's layout, including AAC AudioSpecificConfig evidence.
    /// This may be more specific than the container's bare channel count.
    pub channel_layout: AudioChannelLayout,
    pub sample_format: AudioSampleFormat,
    /// Container header observations in the stream time base, not decoded bounds.
    pub stream_start: Option<i64>,
    pub stream_duration: Option<i64>,
    /// Codec parameter observations in samples per channel. Zero is reported
    /// faithfully and does not prove absence of priming, padding, or preroll.
    pub initial_padding: u32,
    pub trailing_padding: u32,
    pub seek_preroll: u32,
}

/// Actual decoder output representation, before the owned float copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioSampleFormat {
    Signed16,
    Float32Planar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioSkipSamples {
    pub leading: u32,
    pub trailing: u32,
    /// FFmpeg's raw reason byte: 0 is padding silence, 1 is convergence.
    pub leading_reason: u8,
    pub trailing_reason: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFrameMetadata {
    /// Original frame.pts in the selected stream time base. Missing PTS fails.
    pub pts: i64,
    pub decode_timestamp: Option<i64>,
    /// Positive original frame.duration, without sample-count substitution.
    pub reported_duration: Option<i64>,
    /// Decoded samples per channel, including any reported priming or padding.
    pub nb_samples: u32,
    pub sample_rate: u32,
    pub sample_format: AudioSampleFormat,
    pub channel_layout: AudioChannelLayout,
    /// Exact frame skip side data under manual skip mode; no trimming applied.
    pub skip_samples: Option<AudioSkipSamples>,
    /// Original frame discard flag; manual AAC priming can carry this flag.
    pub discard: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudioFrame {
    pub metadata: AudioFrameMetadata,
    /// Owned interleaved samples in original channel order. PCM16 is converted
    /// exactly as sample / 32768; AAC's decoded float amplitude is preserved.
    pub samples: Vec<f32>,
}

/// One persistent demuxer and one software decoder, movable but not `Sync`.
/// Decode failures poison the context. Invalid/pre-cancelled calls and copying
/// before a successful decode do not advance or poison it. There is no seek API:
/// sample-exact compressed-audio seeking is not established by this boundary.
pub struct AudioDecoder {
    inner: ffi::Decoder,
    info: AudioStreamInfo,
    current: Option<AudioFrameMetadata>,
}

#[cfg(test)]
mod opening_budget_tests {
    use super::*;
    use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};

    #[test]
    fn header_guard_and_ffmpeg_share_one_opening_io_allowance() {
        let file = File::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/audio-fixtures/pcm-stereo-48000.wav"),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let control = DecodeControl {
            timeout: Duration::from_secs(2),
            cancelled: &cancelled,
        };
        let defaults = AudioDecodeLimits::default();
        let used = input::validate(&file, 0, defaults, control).unwrap();
        assert!(used > 0);
        // Find the measured minimum for FFmpeg alone, without depending on a
        // particular AVIO buffer fill strategy. These are trusted fixture bytes.
        let admitted = |bytes| {
            ffi::Decoder::open(
                file.try_clone().unwrap(),
                0,
                AudioDecodeLimits {
                    max_io_bytes_per_call: bytes,
                    ..defaults
                },
                0,
                control,
            )
            .is_ok()
        };
        let mut low = 1;
        let mut high = 1024 * 1024;
        assert!(admitted(high));
        while low < high {
            let middle = low + (high - low) / 2;
            if admitted(middle) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let limits = AudioDecodeLimits {
            max_io_bytes_per_call: low,
            ..defaults
        };
        assert!(admitted(low));
        assert_eq!(input::validate(&file, 0, limits, control).unwrap(), used);
        let error = AudioDecoder::open(file.try_clone().unwrap(), 0, limits, control)
            .err()
            .unwrap();
        assert!(matches!(error, SourceDecodeError::Native{code,..} if code=="resource_limit"));
        let mut decoder = AudioDecoder::open(
            file,
            0,
            AudioDecodeLimits {
                max_io_bytes_per_call: low + used,
                ..defaults
            },
            control,
        )
        .unwrap();
        assert!(decoder.next_metadata(control).unwrap().is_some());
    }
}

impl AudioDecoder {
    pub fn open(
        file: File,
        selected_stream: u32,
        limits: AudioDecodeLimits,
        control: DecodeControl<'_>,
    ) -> Result<Self, SourceDecodeError> {
        let started = Instant::now();
        ffi::preflight(control)?;
        limits.validate()?;
        let preflight_io_bytes = input::validate(&file, selected_stream, limits, control)?;
        let timeout = control
            .timeout
            .checked_sub(started.elapsed())
            .filter(|timeout| !timeout.is_zero())
            .ok_or_else(|| SourceDecodeError::Native {
                code: "deadline_exceeded".into(),
                message: "audio header validation exhausted the opening budget".into(),
            })?;
        let (inner, info) = ffi::Decoder::open(
            file,
            selected_stream,
            limits,
            preflight_io_bytes,
            DecodeControl { timeout, ..control },
        )?;
        Ok(Self {
            inner,
            info,
            current: None,
        })
    }

    pub fn info(&self) -> &AudioStreamInfo {
        &self.info
    }

    /// Decode and retain one frame without allocating an owned PCM output.
    pub fn next_metadata(
        &mut self,
        control: DecodeControl<'_>,
    ) -> Result<Option<AudioFrameMetadata>, SourceDecodeError> {
        ffi::preflight(control)?;
        self.current = None;
        self.current = self.inner.next(control)?;
        Ok(self.current)
    }

    /// Copy retained samples without advancing the decoder. The returned buffer
    /// survives subsequent decode operations and destruction of the decoder.
    pub fn copy_current_interleaved_f32(
        &mut self,
        control: DecodeControl<'_>,
    ) -> Result<DecodedAudioFrame, SourceDecodeError> {
        let started = Instant::now();
        ffi::preflight(control)?;
        let expected = self.current.ok_or(SourceDecodeError::InvalidConfiguration(
            "no current audio frame",
        ))?;
        let length = usize::try_from(expected.nb_samples)
            .ok()
            .and_then(|count| count.checked_mul(expected.channel_layout.channels() as usize))
            .ok_or(SourceDecodeError::InvalidConfiguration(
                "audio output sample count overflow",
            ))?;
        let mut samples = Vec::new();
        samples.try_reserve_exact(length).map_err(|_| {
            SourceDecodeError::InvalidConfiguration("audio output allocation failed")
        })?;
        samples.resize(length, 0.0);
        // Allocation and conversion share the caller's one cooperative budget.
        let timeout = control
            .timeout
            .checked_sub(started.elapsed())
            .filter(|timeout| !timeout.is_zero())
            .ok_or_else(|| SourceDecodeError::Native {
                code: "deadline_exceeded".into(),
                message: "audio output allocation exhausted the call budget".into(),
            })?;
        let metadata = self.inner.copy(
            DecodeControl {
                timeout,
                cancelled: control.cancelled,
            },
            &mut samples,
        )?;
        if metadata != expected {
            return Err(SourceDecodeError::Native {
                code: "invalid_report".into(),
                message: "audio copy changed retained frame metadata".into(),
            });
        }
        Ok(DecodedAudioFrame { metadata, samples })
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
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

    #[repr(C)]
    struct Limits {
        max_input_bytes: u64,
        max_frames: u64,
        max_packets: u64,
        max_decoded_samples: u64,
        max_io_bytes_per_call: u64,
        max_samples_per_frame: u32,
        max_channels: u32,
        max_sample_rate: u32,
        max_packets_per_frame: u32,
        max_packet_bytes: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Layout {
        channels: i32,
        order: i32,
        mask: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Info {
        stream_index: i32,
        time_base_num: i32,
        time_base_den: i32,
        sample_rate: i32,
        channel_layout: Layout,
        stream_start: i64,
        stream_duration: i64,
        initial_padding: i32,
        trailing_padding: i32,
        seek_preroll: i32,
        sample_format: i32,
        codec: [c_char; 32],
    }

    #[repr(C)]
    #[derive(Default)]
    struct Frame {
        pts: i64,
        duration: i64,
        dts: i64,
        nb_samples: i32,
        sample_rate: i32,
        sample_format: i32,
        channel_layout: Layout,
        skip_present: u32,
        leading: u32,
        trailing: u32,
        leading_reason: u32,
        trailing_reason: u32,
        discard: u32,
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

    type Cancel = extern "C" fn(*const c_void) -> c_int;
    unsafe extern "C" {
        fn deadpan_audio_open(
            fd: c_int,
            length: i64,
            selected_stream: u32,
            limits: *const Limits,
            preflight_io_bytes: u64,
            timeout_ms: u64,
            cancelled: Cancel,
            opaque: *const c_void,
            out: *mut *mut c_void,
            info: *mut Info,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_audio_next(
            source: *mut c_void,
            timeout_ms: u64,
            cancelled: Cancel,
            opaque: *const c_void,
            frame: *mut Frame,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_audio_copy(
            source: *mut c_void,
            timeout_ms: u64,
            cancelled: Cancel,
            opaque: *const c_void,
            frame: *mut Frame,
            samples: *mut f32,
            length: usize,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_audio_close(source: *mut c_void);
    }

    extern "C" fn cancelled(opaque: *const c_void) -> c_int {
        // SAFETY: every C entrypoint borrows this AtomicBool synchronously and
        // clears the callback and pointer before returning, including failures.
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
                message: "audio decode was cancelled before starting".into(),
            });
        }
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
        _file: File,
        _not_sync: PhantomData<Cell<()>>,
    }

    // SAFETY: the instance exclusively owns its heap FFmpeg context and File,
    // installs no process-global callbacks, and requires &mut self for calls.
    unsafe impl Send for Decoder {}

    impl Drop for Decoder {
        fn drop(&mut self) {
            // SAFETY: successful open creates this unique context; its File is
            // still alive here and closes only after the C state is freed.
            unsafe { deadpan_audio_close(self.pointer.as_ptr()) };
        }
    }

    impl Decoder {
        pub(super) fn open(
            file: File,
            selected_stream: u32,
            limits: AudioDecodeLimits,
            preflight_io_bytes: u64,
            ctl: DecodeControl<'_>,
        ) -> Result<(Self, AudioStreamInfo), SourceDecodeError> {
            let (timeout, opaque) = control(ctl)?;
            limits.validate()?;
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                return Err(SourceDecodeError::InvalidConfiguration(
                    "audio input must be a regular file",
                ));
            }
            let length = i64::try_from(metadata.len()).map_err(|_| {
                SourceDecodeError::InvalidConfiguration("audio input exceeds signed file offsets")
            })?;
            let limits = Limits {
                max_input_bytes: limits.max_input_bytes,
                max_frames: limits.max_frames,
                max_packets: limits.max_packets,
                max_decoded_samples: limits.max_decoded_samples,
                max_io_bytes_per_call: limits.max_io_bytes_per_call,
                max_samples_per_frame: limits.max_samples_per_frame,
                max_channels: limits.max_channels,
                max_sample_rate: limits.max_sample_rate,
                max_packets_per_frame: limits.max_packets_per_frame,
                max_packet_bytes: limits.max_packet_bytes,
            };
            let mut pointer = std::ptr::null_mut();
            let mut info = Info::default();
            let mut error = Error::default();
            // SAFETY: bounded live input/output structures, an owned regular
            // descriptor, and a synchronous cancellation callback. C frees its
            // allocations on failed open and never owns or reopens the File.
            let result = unsafe {
                deadpan_audio_open(
                    file.as_raw_fd(),
                    length,
                    selected_stream,
                    &limits,
                    preflight_io_bytes,
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
                pointer: NonNull::new(pointer).ok_or_else(invalid_report)?,
                _file: file,
                _not_sync: PhantomData,
            };
            let codec = string(&info.codec);
            if !matches!(codec.as_str(), "aac" | "pcm_s16le") {
                return Err(invalid_report());
            }
            let value = AudioStreamInfo {
                stream_index: nonnegative(info.stream_index)?,
                codec,
                time_base_num: positive(info.time_base_num)?,
                time_base_den: positive(info.time_base_den)?,
                sample_rate: positive(info.sample_rate)?,
                channel_layout: info.channel_layout.convert()?,
                sample_format: sample_format(info.sample_format)?,
                stream_start: (info.stream_start != i64::MIN).then_some(info.stream_start),
                stream_duration: (info.stream_duration > 0).then_some(info.stream_duration),
                initial_padding: nonnegative(info.initial_padding)?,
                trailing_padding: nonnegative(info.trailing_padding)?,
                seek_preroll: nonnegative(info.seek_preroll)?,
            };
            if value.stream_index != selected_stream {
                return Err(invalid_report());
            }
            Ok((inner, value))
        }

        pub(super) fn next(
            &mut self,
            ctl: DecodeControl<'_>,
        ) -> Result<Option<AudioFrameMetadata>, SourceDecodeError> {
            let (timeout, opaque) = control(ctl)?;
            let mut frame = Frame::default();
            let mut error = Error::default();
            // SAFETY: exclusive context ownership and synchronous live outputs
            // and cancellation state; next retains its own decoded AVFrame.
            let result = unsafe {
                deadpan_audio_next(
                    self.pointer.as_ptr(),
                    timeout,
                    cancelled,
                    opaque,
                    &mut frame,
                    &mut error,
                )
            };
            match result {
                1 => Ok(Some(frame.convert()?)),
                0 => Ok(None),
                _ => Err(error.into_error()),
            }
        }

        pub(super) fn copy(
            &mut self,
            ctl: DecodeControl<'_>,
            samples: &mut [f32],
        ) -> Result<AudioFrameMetadata, SourceDecodeError> {
            let (timeout, opaque) = control(ctl)?;
            let mut frame = Frame::default();
            let mut error = Error::default();
            // SAFETY: a valid uniquely borrowed output slice lives for this
            // call; C checks its exact sample count before writing any samples.
            let result = unsafe {
                deadpan_audio_copy(
                    self.pointer.as_ptr(),
                    timeout,
                    cancelled,
                    opaque,
                    &mut frame,
                    samples.as_mut_ptr(),
                    samples.len(),
                    &mut error,
                )
            };
            if result != 1 {
                return Err(error.into_error());
            }
            frame.convert()
        }
    }

    impl Layout {
        fn convert(self) -> Result<AudioChannelLayout, SourceDecodeError> {
            let channels = positive(self.channels)?;
            if channels > 32 {
                return Err(invalid_report());
            }
            match self.order {
                0 if self.mask == 0 => Ok(AudioChannelLayout::Unspecified { channels }),
                1 if self.mask.count_ones() == channels => Ok(AudioChannelLayout::Native {
                    channels,
                    mask: self.mask,
                }),
                _ => Err(invalid_report()),
            }
        }
    }

    impl Frame {
        fn convert(self) -> Result<AudioFrameMetadata, SourceDecodeError> {
            if self.pts == i64::MIN || self.skip_present > 1 || self.discard > 1 {
                return Err(invalid_report());
            }
            let nb_samples = positive(self.nb_samples)?;
            let sample_rate = positive(self.sample_rate)?;
            if nb_samples > 65_536 || sample_rate > 384_000 {
                return Err(invalid_report());
            }
            Ok(AudioFrameMetadata {
                pts: self.pts,
                decode_timestamp: (self.dts != i64::MIN).then_some(self.dts),
                reported_duration: (self.duration > 0).then_some(self.duration),
                nb_samples,
                sample_rate,
                sample_format: sample_format(self.sample_format)?,
                channel_layout: self.channel_layout.convert()?,
                discard: self.discard != 0,
                skip_samples: if self.skip_present == 0 {
                    None
                } else {
                    Some(AudioSkipSamples {
                        leading: self.leading,
                        trailing: self.trailing,
                        leading_reason: u8::try_from(self.leading_reason)
                            .map_err(|_| invalid_report())?,
                        trailing_reason: u8::try_from(self.trailing_reason)
                            .map_err(|_| invalid_report())?,
                    })
                },
            })
        }
    }

    fn sample_format(value: i32) -> Result<AudioSampleFormat, SourceDecodeError> {
        match value {
            1 => Ok(AudioSampleFormat::Signed16),
            8 => Ok(AudioSampleFormat::Float32Planar),
            _ => Err(invalid_report()),
        }
    }

    fn nonnegative(value: i32) -> Result<u32, SourceDecodeError> {
        u32::try_from(value).map_err(|_| invalid_report())
    }

    fn positive(value: i32) -> Result<u32, SourceDecodeError> {
        nonnegative(value)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(invalid_report)
    }

    fn string(bytes: &[c_char]) -> String {
        String::from_utf8_lossy(
            &bytes
                .iter()
                .take_while(|&&value| value != 0)
                .map(|&value| value as u8)
                .collect::<Vec<_>>(),
        )
        .into_owned()
    }

    fn invalid_report() -> SourceDecodeError {
        SourceDecodeError::Native {
            code: "invalid_report".into(),
            message: "native audio decoder returned an invalid validated report".into(),
        }
    }
}
