//! Independent full-file SDR inspection in a separate supervised process.
//!
//! The verifier owns no project writer, encoder, destination or publication
//! authority. Its report binds actual decoded observations to private bytes.

use std::{io, sync::atomic::AtomicBool, time::Instant};

use deadpan_jobs::{CancellationToken, Sha256};
use serde::{Deserialize, Serialize};

use super::{EncodedCandidate, EncodedRenderError, protocol::EncodedRenderContract};
use crate::render_worker::protocol::RenderIdentity;

mod host;
mod inspect;
pub mod protocol;
pub(crate) mod worker;

pub use host::verify;

pub const PRIVATE_WORKER_ARGUMENT: &str = "--render-verify-worker";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationLimits {
    pub maximum_bytes: u64,
    pub maximum_packets: u64,
}

impl Default for VerificationLimits {
    fn default() -> Self {
        Self {
            maximum_bytes: 64 * 1024 * 1024 * 1024,
            maximum_packets: 1_000_000,
        }
    }
}

impl VerificationLimits {
    pub fn validate(self) -> Result<(), String> {
        if !(1..=64 * 1024 * 1024 * 1024).contains(&self.maximum_bytes)
            || !(1..=1_000_000).contains(&self.maximum_packets)
        {
            return Err("verification limits exceed qualified source admission bounds".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct VerificationRequest {
    pub identity: RenderIdentity,
    pub cancellation_token: CancellationToken,
    pub limits: VerificationLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStage {
    Packets,
    Pictures,
    ManualAudio,
    OrdinaryAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationProgress {
    pub stage: VerificationStage,
    pub completed: u64,
    pub total: u64,
}

/// Bounded observations, serialized only after the complete validator succeeds.
/// Deserialization creates claims; only the host-selected verifier can admit a
/// private candidate. This does not qualify arbitrary lossy content or a platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub policy_version: u32,
    pub contract: EncodedRenderContract,
    pub document_sha256: Sha256,
    pub movie_sha256: Sha256,
    pub movie_bytes: u64,
    pub video_frames: u64,
    pub audio_samples: u64,
    pub video_packets: u64,
    pub audio_packets: u64,
    pub gops: u64,
    pub fresh_gop_frames: u64,
    pub maximum_b_run: u32,
    pub runtime_versions: [u32; 3],
    pub movie_timescale: u32,
    pub video_edit_media_time: i64,
    pub audio_edit_media_time: i64,
    pub manual_first_sample: i64,
    pub manual_physical_samples: u64,
    pub ordinary_first_sample: i64,
    pub ordinary_physical_samples: u64,
}

impl VerificationReport {
    pub fn validate(&self, limits: VerificationLimits) -> Result<(), String> {
        limits.validate()?;
        let native = self.contract.native_contract()?;
        let packets = self
            .video_packets
            .checked_add(self.audio_packets)
            .ok_or("verification packet count overflow")?;
        if self.policy_version != 1
            || self.movie_bytes == 0
            || self.movie_bytes > limits.maximum_bytes
            || self.video_frames != native.video_frames()
            || self.audio_samples != native.audio_samples()
            || self.video_packets != self.video_frames
            || packets > limits.maximum_packets
            || self.audio_packets != self.audio_samples.div_ceil(1024) + 1
            || self.gops == 0
            || self.gops > self.video_frames
            || self.gops
                < self
                    .video_frames
                    .div_ceil(u64::from(native.policy().gop_frames) + 1)
            || self.fresh_gop_frames != self.video_frames
            || self.runtime_versions != [4_066_151, 4_064_103, 3_934_311]
            || self.movie_timescale != native.policy().movie_timescale
            || self.video_edit_media_time < 0
            || self.audio_edit_media_time != 1024
            || self.manual_first_sample != -1024
            || self.ordinary_first_sample != 0
            || self.manual_physical_samples != self.audio_packets * 1024
            || self.ordinary_physical_samples != self.audio_samples.div_ceil(1024) * 1024
        {
            return Err("verification report contradicts the exact output policy".into());
        }
        let maximum_b = match self.contract.choice.b_frames {
            deadpan_encode::BFramePolicy::None => 0,
            deadpan_encode::BFramePolicy::TargetTwo => 2,
        };
        let frame_ticks = i64::from(native.frame_rate()[1]);
        if self.maximum_b_run > maximum_b
            || self.video_edit_media_time % frame_ticks != 0
            || self.video_edit_media_time > i64::from(maximum_b) * frame_ticks
            || (maximum_b > 0
                && self.video_frames > u64::from(native.policy().gop_frames)
                && self.maximum_b_run == 0)
        {
            return Err("verification report exceeds the captured B-frame policy".into());
        }
        Ok(())
    }
}

/// Private bytes that passed this version's complete SDR structural/decode
/// checks. Runtime/content qualification and destination publication stay with
/// the application. No path or writable descriptor is exposed.
pub struct VerifiedCandidate {
    candidate: EncodedCandidate,
    report: VerificationReport,
}

/// Failed inspection retains the completed encode so the caller can retry
/// verification without rendering again. No failure promotes its bytes.
pub struct VerificationFailure {
    pub error: EncodedRenderError,
    pub candidate: EncodedCandidate,
}

impl std::fmt::Debug for VerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerificationFailure")
            .field("error", &self.error)
            .field("candidate_bytes", &self.candidate.byte_length())
            .finish()
    }
}

impl std::fmt::Display for VerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for VerificationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl VerifiedCandidate {
    pub fn report(&self) -> &VerificationReport {
        &self.report
    }
    pub fn candidate(&self) -> &EncodedCandidate {
        &self.candidate
    }
    pub fn copy_to(
        &mut self,
        sink: &mut impl io::Write,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<u64, EncodedRenderError> {
        self.candidate.copy_to(sink, cancelled, deadline)
    }
}
