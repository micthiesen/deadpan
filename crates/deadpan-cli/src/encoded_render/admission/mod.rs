//! Current-runtime deterministic encoder admission. A probe is never a project
//! render or a publishable candidate. Stored observations cannot restore the
//! live result, and this boundary does not change a durable render intent.

mod content;
mod host;
pub mod protocol;
pub(crate) mod worker;

pub use content::{ProbeContentReport, ProbeMarkerObservation};
pub use host::{
    AdmissionFailure, AdmissionRequest, AutomaticEncodedCandidate, QualifiedEncoder, qualify,
};

use deadpan_core::{
    AudioSample, ColorPolicy, ExactRatio, FrameRange, FrameRate, ProjectFrame, ProjectId,
    RevisionId,
};
use deadpan_encode::{EncodeLimits, probe::EncoderProbe};
use deadpan_jobs::{Sha256, process::ProcessLimits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};

use super::{
    protocol::{EncodedFailure, EncodedManifest, EncodedRenderContract, EncoderChoice},
    runtime::RuntimeFingerprint,
    verification::{VerificationLimits, VerificationReport},
};
use crate::render_worker::protocol::{RenderContract, RenderIdentity, RenderTimeBase};

pub const PRIVATE_WORKER_ARGUMENT: &str = "--render-probe-worker";
pub const POLICY_VERSION: u32 = 1;
pub const MAX_PROBE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_PROBE_PACKETS: u64 = 1024;

/// Only these engineering inputs vary. Generator clocks, bitrate, GOP and
/// content are derived by the versioned probe, never supplied as loose claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeSpec {
    pub raster: [u32; 2],
    pub frame_rate: [u32; 2],
    pub choice: EncoderChoice,
}

impl ProbeSpec {
    pub fn generator(&self) -> Result<EncoderProbe, String> {
        if self.raster[0] < 14 || self.raster[1] < 16 {
            return Err("encoder admission requires at least 14x16 pixels to distinguish every probe picture and color region".into());
        }
        EncoderProbe::new(self.raster, self.frame_rate).map_err(|error| error.to_string())
    }

    pub fn contract(&self) -> Result<EncodedRenderContract, String> {
        let probe = self.generator()?;
        let native = probe
            .contract(self.choice.mode, self.choice.b_frames)
            .map_err(|error| error.to_string())?;
        let frames = i64::try_from(native.video_frames()).map_err(|_| "probe frame overflow")?;
        let rate = FrameRate::new(self.frame_rate[0], self.frame_rate[1])
            .map_err(|error| error.to_string())?;
        let range = FrameRange::new(ProjectFrame(0), ProjectFrame(frames))
            .map_err(|error| error.to_string())?;
        let contract = EncodedRenderContract {
            picture: RenderContract {
                project_id: ProjectId::new("encoder-admission-probe-v1")
                    .map_err(|error| error.to_string())?,
                revision_id: RevisionId::new("deterministic-probe-v1")
                    .map_err(|error| error.to_string())?,
                range,
                canvas: self.raster,
                raster: self.raster,
                frame_rate: rate,
                color_policy: ColorPolicy::SdrRec709,
                time_base: RenderTimeBase {
                    numerator: 1,
                    denominator: self.frame_rate[0],
                },
                frame_count: native.video_frames(),
                terminal_pts: frames
                    .checked_mul(i64::from(self.frame_rate[1]))
                    .ok_or("probe PTS overflow")?,
                project_audio_start: AudioSample(0),
                project_audio_end: AudioSample(
                    i64::try_from(native.audio_samples()).map_err(|_| "probe sample overflow")?,
                ),
                relative_aspect_error: ExactRatio::ZERO,
            },
            choice: self.choice,
        };
        contract.validate()?;
        Ok(contract)
    }

    pub fn document_sha256(&self) -> Result<Sha256, String> {
        // Domain-separated recipe identity. This is not a project document hash.
        let probe = self.generator()?;
        let mut digest = Hasher::new();
        digest.update(b"deadpan-encoder-admission-probe-v1\0");
        digest.update(serde_json::to_vec(probe.config()).map_err(|error| error.to_string())?);
        let hex: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Sha256::new(hex).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeReport {
    pub schema_version: u32,
    pub spec: ProbeSpec,
    pub manifest: EncodedManifest,
    pub verification: VerificationReport,
    pub content: ProbeContentReport,
    pub runtime: RuntimeFingerprint,
}

impl ProbeReport {
    pub fn validate(&self, limits: EncodeLimits) -> Result<(), String> {
        self.runtime.validate()?;
        if self.schema_version != 2
            || self.manifest.contract != self.spec.contract()?
            || self.manifest.document_sha256 != self.spec.document_sha256()?
        {
            return Err("probe report changed its deterministic input".into());
        }
        self.manifest.validate_for(limits)?;
        self.verification.validate(VerificationLimits {
            maximum_bytes: limits.maximum_output_bytes,
            maximum_packets: limits.maximum_packets.min(1_000_000),
        })?;
        if self.verification.contract != self.manifest.contract
            || self.verification.document_sha256 != self.manifest.document_sha256
            || self.verification.movie_sha256 != *self.manifest.movie.sha256()
            || self.verification.movie_bytes != self.manifest.movie.byte_length()
            || self.verification.video_packets != self.manifest.report.video_packets
            || self.verification.audio_packets != self.manifest.report.audio_packets
        {
            return Err("probe verification changed its encoded bytes".into());
        }
        self.content.validate(&self.spec)
    }
}

/// Runtime facts captured afresh around admission. No host name or user path is
/// retained. Kernel build includes the running Apple kernel/architecture family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRuntime {
    pub helper_sha256: Sha256,
    pub helper_bytes: u64,
    pub system: String,
    pub kernel_release: String,
    pub kernel_build: String,
    pub machine: String,
}

impl AdmissionRuntime {
    pub fn validate(&self) -> Result<(), String> {
        if self.helper_bytes == 0 || self.helper_bytes > 512 * 1024 * 1024 {
            return Err("probe helper byte bound".into());
        }
        for value in [
            &self.system,
            &self.kernel_release,
            &self.kernel_build,
            &self.machine,
        ] {
            if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
                return Err("probe host runtime text bound".into());
            }
        }
        Ok(())
    }

    pub(super) fn matches_loaded(&self, loaded: &RuntimeFingerprint) -> Result<(), String> {
        self.validate()?;
        loaded.validate()?;
        if loaded.helper().sha256 != self.helper_sha256
            || loaded.helper().mapped.file_size != self.helper_bytes
            || loaded.system != self.system
            || loaded.kernel_release != self.kernel_release
            || loaded.kernel_build != self.kernel_build
            || loaded.machine != self.machine
        {
            return Err("loaded helper differs from the host-selected runtime".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RejectedProbe {
    pub identity: RenderIdentity,
    pub spec: ProbeSpec,
    pub failure: EncodedFailure,
    pub runtime: Option<RuntimeFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EncoderDecision {
    pub policy_version: u32,
    pub identity: RenderIdentity,
    pub runtime: AdmissionRuntime,
    pub rejected: Vec<RejectedProbe>,
    pub selected_identity: RenderIdentity,
    pub selected: ProbeReport,
}

/// One shared deadline covers all probes, decoding and snapshot hashing.
#[derive(Debug, Clone, Copy)]
pub struct AdmissionLimits {
    pub encode: EncodeLimits,
    pub process: ProcessLimits,
}

impl Default for AdmissionLimits {
    fn default() -> Self {
        use std::time::Duration;
        Self {
            encode: EncodeLimits {
                maximum_output_bytes: MAX_PROBE_BYTES,
                maximum_packets: MAX_PROBE_PACKETS,
                ..EncodeLimits::default()
            },
            process: ProcessLimits {
                maximum_duration: Duration::from_secs(120),
                cancellation_grace: Duration::from_millis(500),
                exit_grace: Duration::from_secs(2),
            },
        }
    }
}

#[cfg(test)]
mod tests;
