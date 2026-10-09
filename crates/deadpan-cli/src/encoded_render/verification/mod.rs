//! Independent full-file SDR and HDR inspection in a separate supervised process.
//!
//! The verifier owns no project writer, encoder, destination or publication
//! authority. Its report binds actual decoded observations to private bytes.

use std::{io, sync::atomic::AtomicBool, time::Instant};

use deadpan_encode::{AUDIO_FRAME_SAMPLES, AUDIO_PRIMING_SAMPLES};
use deadpan_jobs::{CancellationToken, Sha256};
use serde::{Deserialize, Serialize};

use super::{EncodedCandidate, EncodedRenderError, protocol::EncodedRenderContract};
use crate::render_worker::protocol::RenderIdentity;

mod host;
mod inspect;
pub mod protocol;
pub(crate) mod worker;

pub use host::verify;

/// Shared complete file inspection for the isolated deterministic admission
/// probe. This returns observations and cannot create a publishable candidate.
pub(crate) use inspect::inspect as inspect_file;

pub const PRIVATE_WORKER_ARGUMENT: &str = "--render-verify-worker";

/// Complete coded planes before the visible crop. On this host VideoToolbox
/// H.264 encodes a 96x64 input as 192x96 (SPS 12x6 macroblocks, right/bottom
/// crop 48/16). HEVC uses coding tree blocks up to 64x64. The verifier still
/// requires the exact contracted visible raster and checks every output pixel.
/// Saturation makes an unvalidated extreme raster fail DecodeLimits validation.
pub(crate) fn decode_pixel_budget(raster: [u32; 2], hdr: bool) -> u64 {
    let (block, minimum) = if hdr { (64, [0, 0]) } else { (16, [192, 96]) };
    let [width, height] = std::array::from_fn::<_, 2, _>(|i| {
        u64::from(raster[i].max(minimum[i])).div_ceil(block) * block
    });
    width.saturating_mul(height)
}

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
    /// PQ output only: the declared `clli` and the content light recomputed
    /// from every decoded picture. Absent for SDR and HLG, whose serialized
    /// reports are therefore unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_light: Option<ContentLightEvidence>,
}

/// Largest admitted shortfall of a declared MaxCLL below the decoded
/// chroma-site 99th-percentile bound, in limited-range 10-bit PQ code values
/// (876 per unit signal). Lossy coding at dense saturated edges raised that
/// bound up to 20.5 codes above the true MaxCLL in the 2026-10-05
/// measurement; see docs/FINISHED_FILE_VERIFICATION.md.
pub const MAX_CLL_PQ_CODE_TOLERANCE: f64 = 32.0;
/// Largest admitted shortfall of a declared MaxFALL below the decoded
/// chroma-site frame-mean bound, in the same PQ code units. The measured
/// bound never exceeded the true MaxFALL.
pub const MAX_FALL_PQ_CODE_TOLERANCE: f64 = 8.0;

/// Content light sanity evidence for one PQ file; not CTA-861.3
/// verification. Declared values are the `clli` box in whole cd/m². Decoded
/// values are chroma-site lower bounds on the true MaxCLL (largest per-frame
/// 99th percentile of site light) and MaxFALL (largest per-frame
/// edge-weighted site mean) recomputed from every decoded picture, in
/// 1/1000 cd/m² rounded half up. A declaration below them (beyond the coding
/// tolerance) contradicts the emitted pictures. A 4:2:0 file cannot prove an
/// overstated declaration, so no upper bound is claimed. Only the declared
/// pair must satisfy MaxFALL <= MaxCLL: the decoded MaxFALL bound exceeds the
/// decoded MaxCLL percentile when sparse highlights carry most of the light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentLightEvidence {
    pub declared_max_cll: u16,
    pub declared_max_fall: u16,
    pub decoded_bound_max_cll_millinits: u32,
    pub decoded_bound_max_fall_millinits: u32,
}

impl ContentLightEvidence {
    pub(crate) fn new(
        declared: deadpan_source::ContentLight,
        decoded: inspect::DecodedLight,
    ) -> Result<Self, String> {
        let millinits = |nits: f64| {
            if !(0.0..=10_000.0).contains(&nits) {
                return Err("decoded content light is outside the PQ range".to_owned());
            }
            // Bounded nonnegative value at most 10^7 after the check above.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok((nits * 1_000.0 + 0.5).floor() as u32)
        };
        let evidence = Self {
            declared_max_cll: declared.max_cll,
            declared_max_fall: declared.max_fall,
            decoded_bound_max_cll_millinits: millinits(decoded.max_cll)?,
            decoded_bound_max_fall_millinits: millinits(decoded.max_fall)?,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    /// Signed PQ code values by which `decoded_nits` exceeds `declared_nits`.
    pub fn pq_code_shortfall(declared_nits: f64, decoded_nits: f64) -> f64 {
        (deadpan_render::pq_inverse_eotf(decoded_nits)
            - deadpan_render::pq_inverse_eotf(declared_nits))
            * 876.0
    }

    pub fn validate(&self) -> Result<(), String> {
        let decoded = [
            self.decoded_bound_max_cll_millinits,
            self.decoded_bound_max_fall_millinits,
        ]
        .map(|value| f64::from(value) / 1_000.0);
        if self.declared_max_cll > 10_000
            || self.declared_max_fall > self.declared_max_cll
            || self.decoded_bound_max_cll_millinits > 10_000_000
            || self.decoded_bound_max_fall_millinits > 10_000_000
        {
            return Err("ExportVerificationFailed: content light exceeds CTA-861.3 bounds".into());
        }
        if Self::pq_code_shortfall(f64::from(self.declared_max_cll), decoded[0])
            > MAX_CLL_PQ_CODE_TOLERANCE
            || Self::pq_code_shortfall(f64::from(self.declared_max_fall), decoded[1])
                > MAX_FALL_PQ_CODE_TOLERANCE
        {
            return Err(format!(
                "ExportVerificationFailed: declared clli {}/{} is below the decoded pictures' {:.3}/{:.3} cd/m²",
                self.declared_max_cll, self.declared_max_fall, decoded[0], decoded[1]
            ));
        }
        Ok(())
    }
}

impl VerificationReport {
    pub fn validate(&self, limits: VerificationLimits) -> Result<(), String> {
        self.validate_policy(limits, 2)
    }

    /// Historical observations retain their matching encoder ABI and complete
    /// picture, audio and HDR checks, without granting live verifier authority.
    pub(super) fn validate_retained(
        &self,
        limits: VerificationLimits,
        encoder_abi: u32,
    ) -> Result<(), String> {
        match encoder_abi {
            version @ (1 | 2) => self.validate_policy(limits, version),
            _ => Err("unsupported retained verification policy".into()),
        }
    }

    fn validate_policy(
        &self,
        limits: VerificationLimits,
        policy_version: u32,
    ) -> Result<(), String> {
        limits.validate()?;
        let priming = if policy_version == 1 {
            AUDIO_FRAME_SAMPLES
        } else {
            AUDIO_PRIMING_SAMPLES
        };
        let native = self.contract.native_contract()?;
        let packets = self
            .video_packets
            .checked_add(self.audio_packets)
            .ok_or("verification packet count overflow")?;
        if self.policy_version != policy_version
            || self.movie_bytes == 0
            || self.movie_bytes > limits.maximum_bytes
            || self.video_frames != native.video_frames()
            || self.audio_samples != native.audio_samples()
            || self.video_packets != self.video_frames
            || packets > limits.maximum_packets
            || self.audio_packets
                != self.audio_samples.div_ceil(u64::from(AUDIO_FRAME_SAMPLES))
                    + u64::from(priming / AUDIO_FRAME_SAMPLES)
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
            || self.audio_edit_media_time != i64::from(priming)
            || self.manual_first_sample != -i64::from(priming)
            || self.ordinary_first_sample != 0
            || self.manual_physical_samples != self.audio_packets * u64::from(AUDIO_FRAME_SAMPLES)
            || self.ordinary_physical_samples
                != self.audio_samples.div_ceil(u64::from(AUDIO_FRAME_SAMPLES))
                    * u64::from(AUDIO_FRAME_SAMPLES)
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
        let pq = native.video_format() == deadpan_encode::VideoFormat::HevcMain10Rec2100Pq;
        match &self.content_light {
            Some(light) if pq => light.validate()?,
            None if !pq => {}
            _ => return Err("content light evidence is required for PQ output only".into()),
        }
        Ok(())
    }
}

/// Private bytes that passed this version's complete SDR or HDR structural/decode
/// checks. Runtime/content qualification and destination publication stay with
/// the application. No path or writable descriptor is exposed.
pub struct VerifiedCandidate {
    candidate: EncodedCandidate,
    report: VerificationReport,
    verification_identity: RenderIdentity,
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
    /// The live verifier invocation that admitted these bytes. Stored reports
    /// cannot reconstruct this value or substitute a different retry identity.
    pub fn verification_identity(&self) -> &RenderIdentity {
        &self.verification_identity
    }
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
