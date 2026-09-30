//! Bounded native H.264/AAC encoding from already composed I420 and mastered PCM.
//!
//! The caller supplies a fresh private read/write file, exact committed clocks,
//! and finite planar stereo. This adapter owns no project, decoder, GPU or audio
//! device. Calls belong inside the supervised render process, never UI/audio
//! callbacks. One deadline spans all calls; native checks cannot preempt a stuck
//! driver or filesystem call. Successful muxing still requires independent full
//! emitted-file verification before publication.

use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

mod policy;
pub mod probe;
mod progress;
pub use policy::*;
pub use progress::NextInput;

// All pointer, descriptor and synchronous callback lifetime handling stays in
// this narrow module. The public wrapper and policy forbid unsafe operations.
#[allow(unsafe_code)]
mod ffi;

#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error("invalid encoder configuration: {0}")]
    Configuration(&'static str),
    #[error("invalid encoder input: {0}")]
    Input(&'static str),
    #[error("encoder session is poisoned by an earlier failed operation")]
    Poisoned,
    #[error("encoding was cancelled")]
    Cancelled,
    #[error("encoding exceeded its shared monotonic deadline")]
    Deadline,
    #[error("native encoder {code}: {message}")]
    Native { code: String, message: String },
    #[error("native encoder returned inconsistent evidence: {0}")]
    Evidence(&'static str),
    #[error("encoder file I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// Stable failure facts for the supervising host. A kind is not permission to
/// retry, select another encoder or treat process cleanup as complete.
/// Unknown native failures remain Native; diagnostic prose is never parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum EncodeFailureKind {
    Configuration,
    Input,
    Poisoned,
    Cancelled,
    Deadline,
    /// The named video encoder is conclusively absent. Generic codec-open
    /// failures, missing AAC and unsupported output do not establish this.
    EncoderUnavailable,
    /// An actual video packet had PTS before DTS and was rejected before muxing.
    VideoTimestampOrder,
    Capacity,
    Io,
    Evidence,
    Native,
}

impl EncodeError {
    pub fn kind(&self) -> EncodeFailureKind {
        match self {
            Self::Configuration(_) => EncodeFailureKind::Configuration,
            Self::Input(_) => EncodeFailureKind::Input,
            Self::Poisoned => EncodeFailureKind::Poisoned,
            Self::Cancelled => EncodeFailureKind::Cancelled,
            Self::Deadline => EncodeFailureKind::Deadline,
            Self::Evidence(_) => EncodeFailureKind::Evidence,
            Self::Io(_) => EncodeFailureKind::Io,
            Self::Native { code, .. } => match code.as_str() {
                "video_encoder_unavailable" => EncodeFailureKind::EncoderUnavailable,
                "video_timestamp_order" => EncodeFailureKind::VideoTimestampOrder,
                "cancelled" => EncodeFailureKind::Cancelled,
                "deadline_exceeded" => EncodeFailureKind::Deadline,
                "invalid_config" | "invalid_control" | "invalid_descriptor" => {
                    EncodeFailureKind::Configuration
                }
                "incomplete_input" | "input_order" | "invalid_pcm" | "invalid_pixels" => {
                    EncodeFailureKind::Input
                }
                "allocation_failure" | "output_too_large" | "packet_limit" => {
                    EncodeFailureKind::Capacity
                }
                "output_io" | "output_seek" => EncodeFailureKind::Io,
                "encoder_unsupported" | "incomplete_output" | "invalid_packet" => {
                    EncodeFailureKind::Evidence
                }
                _ => EncodeFailureKind::Native,
            },
        }
    }
}

/// Queried codec/mux properties. These do not establish actual B frames, GOP
/// independence, color interpretation, decoder delay or finished-file quality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncoderInfo {
    pub abi_version: u32,
    pub avcodec_version: u32,
    pub avformat_version: u32,
    pub avutil_version: u32,
    pub movie_timescale: u32,
    pub video_time_base_num: u32,
    pub video_time_base_den: u32,
    pub audio_time_base_num: u32,
    pub audio_time_base_den: u32,
    pub audio_frame_size: u32,
    pub video_profile: i32,
    pub video_has_b_frames: i32,
    pub video_max_b_frames: i32,
    pub video_gop_size: i32,
    pub audio_profile: i32,
    pub audio_initial_padding: i32,
    pub audio_trailing_padding: i32,
    pub requested_mode: EncoderMode,
    pub video_bitrate: u64,
    pub audio_bitrate: u64,
    pub maximum_moov_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodeReport {
    pub info: EncoderInfo,
    pub video_frames: u64,
    pub audio_samples: u64,
    pub video_packets: u64,
    pub audio_packets: u64,
    pub output_bytes: u64,
    pub packet_bytes: u64,
    /// Encoder packets with absent duration, filled from the exact authored
    /// frame contract. PTS/DTS are retained; nonzero conflicting duration fails.
    pub video_duration_from_contract_packets: u64,
    pub faststart_read_opens: u32,
    pub faststart_read_closes: u32,
    pub video_eof: bool,
    pub audio_eof: bool,
}

pub struct EncodedOutput {
    file: File,
    report: EncodeReport,
}

impl EncodedOutput {
    pub fn report(&self) -> &EncodeReport {
        &self.report
    }
    /// The descriptor is rewound; the caller still owns verification and publication.
    pub fn into_parts(self) -> (File, EncodeReport) {
        (self.file, self.report)
    }
}

struct Control<'a> {
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl Control<'_> {
    fn check(&self) -> Result<(), EncodeError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(EncodeError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(EncodeError::Deadline);
        }
        Ok(())
    }

    fn remaining_millis(&self) -> Result<u64, EncodeError> {
        self.check()?;
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(EncodeError::Deadline);
        }
        if remaining > Duration::from_secs(86_400) {
            return Err(EncodeError::Configuration(
                "encoder deadline must be within 24 hours",
            ));
        }
        let millis =
            remaining.as_millis() + u128::from(!remaining.subsec_nanos().is_multiple_of(1_000_000));
        u64::try_from(millis).map_err(|_| EncodeError::Configuration("encoder timeout overflow"))
    }
}

/// One exclusive native session. It is neither Send nor Sync until the native
/// driver and callback ownership across threads are independently qualified.
/// Any rejected push or native failure poisons the session; no later finish can
/// turn partial input into success. Drop closes native state before its file.
pub struct EncoderSession<'a> {
    inner: ffi::Encoder,
    contract: EncodeContract,
    limits: EncodeLimits,
    info: EncoderInfo,
    progress: progress::Progress,
    control: Control<'a>,
}

impl<'a> EncoderSession<'a> {
    pub fn open(
        output: File,
        contract: EncodeContract,
        limits: EncodeLimits,
        cancelled: &'a AtomicBool,
        deadline: Instant,
    ) -> Result<Self, EncodeError> {
        limits.validate_for(&contract)?;
        let control = Control {
            cancelled,
            deadline,
        };
        control.remaining_millis()?;
        let (inner, info) = ffi::Encoder::open(output, &contract, limits, &control)?;
        control.check()?;
        Ok(Self {
            inner,
            contract,
            limits,
            info,
            progress: progress::Progress::default(),
            control,
        })
    }

    pub fn contract(&self) -> &EncodeContract {
        &self.contract
    }
    pub fn info(&self) -> &EncoderInfo {
        &self.info
    }
    pub const fn accepted_pictures(&self) -> u64 {
        self.progress.pictures
    }
    pub const fn accepted_audio_samples(&self) -> u64 {
        self.progress.audio
    }

    pub fn next_input(&mut self) -> Result<NextInput, EncodeError> {
        if self.progress.poisoned {
            return Err(EncodeError::Poisoned);
        }
        if let Err(error) = self.control.check() {
            self.progress.poisoned = true;
            return Err(error);
        }
        self.progress.next(&self.contract)
    }

    /// Consume exactly one tight limited-range Y/Cb/Cr frame at its output PTS.
    pub fn push_picture(
        &mut self,
        ordinal: u64,
        pts: i64,
        duration: i64,
        bytes: &[u8],
    ) -> Result<(), EncodeError> {
        let inner = &mut self.inner;
        let control = &self.control;
        self.progress
            .picture(&self.contract, (ordinal, pts, duration), bytes, || {
                control.check()?;
                inner.picture(ordinal, pts, duration, bytes, control)?;
                control.check()
            })
    }

    /// Consume the next 1024 samples, or the exact short final block. Values are
    /// finite mastered samples; this boundary never clips, normalizes or shifts.
    pub fn push_audio(
        &mut self,
        first_sample: u64,
        left: &[f32],
        right: &[f32],
    ) -> Result<(), EncodeError> {
        let inner = &mut self.inner;
        let control = &self.control;
        self.progress
            .audio(&self.contract, first_sample, left, right, || {
                control.check()?;
                inner.audio(first_sample, left, right, control)?;
                control.check()
            })
    }

    pub fn finish(mut self) -> Result<EncodedOutput, EncodeError> {
        let inner = &mut self.inner;
        let control = &self.control;
        let report = self.progress.finish(&self.contract, || {
            control.check()?;
            let report = inner.finish(&self.contract, self.limits, control)?;
            control.check()?;
            Ok(report)
        })?;
        let mut file = self.inner.into_file();
        self.control.check()?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() != report.output_bytes {
            return Err(EncodeError::Evidence(
                "finished descriptor length differs from native report",
            ));
        }
        file.seek(SeekFrom::Start(0))?;
        self.control.check()?;
        Ok(EncodedOutput { file, report })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_control_deadline_shrinks_and_cancellation_has_priority() {
        let cancelled = AtomicBool::new(false);
        let control = Control {
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(2),
        };
        let first = control.remaining_millis().unwrap();
        assert!((1..=2000).contains(&first));
        assert!(control.remaining_millis().unwrap() <= first);
        let expired = Control {
            cancelled: &cancelled,
            deadline: Instant::now(),
        };
        assert!(matches!(expired.check(), Err(EncodeError::Deadline)));
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            control.remaining_millis(),
            Err(EncodeError::Cancelled)
        ));
        assert!(matches!(expired.check(), Err(EncodeError::Cancelled)));
    }

    #[test]
    fn invalid_limits_cancel_and_expired_budget_fail_before_native_open() {
        let contract = EncodeContract::new(
            [2, 2],
            [60, 1],
            1,
            800,
            EncoderMode::Hardware,
            BFramePolicy::TargetTwo,
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        let invalid = EncodeLimits {
            maximum_output_bytes: 0,
            ..Default::default()
        };
        assert!(matches!(
            EncoderSession::open(
                tempfile::tempfile().unwrap(),
                contract.clone(),
                invalid,
                &cancelled,
                deadline
            ),
            Err(EncodeError::Configuration(_))
        ));
        assert!(matches!(
            EncoderSession::open(
                tempfile::tempfile().unwrap(),
                contract.clone(),
                EncodeLimits::default(),
                &AtomicBool::new(true),
                deadline
            ),
            Err(EncodeError::Cancelled)
        ));
        assert!(matches!(
            EncoderSession::open(
                tempfile::tempfile().unwrap(),
                contract,
                EncodeLimits::default(),
                &cancelled,
                Instant::now()
            ),
            Err(EncodeError::Deadline)
        ));
    }
}
