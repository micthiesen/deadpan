//! Frozen automatic SDR admission observations. These bounded declarations
//! preserve what was measured; deserialization grants no live encoder, worker
//! cleanup, decoded-media or publication authority.

use super::{
    BoundedCount, RenderAutomaticAlgorithm, RenderBFrames, RenderEncoder, RenderError, RenderIntent,
};
use crate::{AttemptId, Diagnostic, RequestId, Sha256, WorkspaceArtifact};
use deadpan_core::{
    AudioSample, ColorPolicy, ExactRatio, FrameRange, FrameRate, ProjectFrame, ProjectId,
    RevisionId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};

pub const MAX_RENDER_DECISION_BYTES: usize = 64 * 1024;
pub const MAX_RENDER_PROBES: usize = 4;
const MAX_PROBE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PROBE_PACKETS: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeIdentity {
    pub request_id: RequestId,
    pub attempt_id: AttemptId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderOutputTimeBase {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderPictureContract {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub range: FrameRange,
    pub canvas: [u32; 2],
    pub raster: [u32; 2],
    pub frame_rate: FrameRate,
    pub color_policy: ColorPolicy,
    pub time_base: RenderOutputTimeBase,
    pub frame_count: u64,
    pub terminal_pts: i64,
    pub project_audio_start: AudioSample,
    pub project_audio_end: AudioSample,
    pub relative_aspect_error: ExactRatio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderEncoderChoice {
    pub mode: RenderEncoder,
    pub b_frames: RenderBFrames,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeContract {
    pub picture: RenderPictureContract,
    pub choice: RenderEncoderChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeManifest {
    pub contract: RenderProbeContract,
    pub document_sha256: Sha256,
    pub movie: WorkspaceArtifact,
    pub report: RenderEncodeReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeFailure {
    pub kind: RenderProbeFailureKind,
    pub diagnostic: Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderEncoderInfo {
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
    pub requested_mode: RenderEncoder,
    pub video_bitrate: u64,
    pub audio_bitrate: u64,
    pub maximum_moov_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderEncodeReport {
    pub info: RenderEncoderInfo,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRuntimeFileTime {
    pub seconds: i64,
    pub nanoseconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderMappedImageIdentity {
    pub kind: RenderRuntimeImageKind,
    pub device: u64,
    pub inode: u64,
    pub uuid: [u8; 16],
    pub file_size: u64,
    pub modification_time: RenderRuntimeFileTime,
    pub change_time: RenderRuntimeFileTime,
    pub birth_time: RenderRuntimeFileTime,
    pub generation: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRuntimePlatform {
    pub os_build: String,
    pub hardware_model: String,
    pub cpu_family: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRuntimeImageFingerprint {
    pub mapped: RenderMappedImageIdentity,
    pub sha256: Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRuntimeFingerprint {
    pub schema_version: u32,
    pub platform: RenderRuntimePlatform,
    pub system: String,
    pub kernel_release: String,
    pub kernel_build: String,
    pub machine: String,
    pub images: [RenderRuntimeImageFingerprint; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderSdrSettings {
    pub video_bitrate: u64,
    pub audio_bitrate: u64,
    pub gop_frames: u32,
    pub b_frames: u32,
    pub movie_timescale: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeVerification {
    pub policy_version: u32,
    pub contract: RenderProbeContract,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeMarker {
    pub expected_samples: [u64; 2],
    pub observed_samples: [u64; 2],
    pub observed_peaks: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeContent {
    pub schema_version: u32,
    pub video_frames: u64,
    pub plane_samples: [u64; 3],
    pub maximum_plane_error: [u8; 3],
    pub absolute_plane_error: [u64; 3],
    pub squared_plane_error: [u64; 3],
    pub worst_frame_mean_absolute_error_milli: [u32; 3],
    pub worst_frame_mean_squared_error_milli: [u32; 3],
    pub audio_samples: u64,
    pub markers: [RenderProbeMarker; 3],
    pub unexpected_audio_peak: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeSpec {
    pub raster: [u32; 2],
    pub frame_rate: [u32; 2],
    pub choice: RenderEncoderChoice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeReport {
    pub schema_version: u32,
    pub spec: RenderProbeSpec,
    pub manifest: RenderProbeManifest,
    pub verification: RenderProbeVerification,
    pub content: RenderProbeContent,
    pub runtime: RenderRuntimeFingerprint,
    pub settings: RenderSdrSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderAdmissionRuntime {
    pub helper_sha256: Sha256,
    pub helper_bytes: u64,
    pub system: String,
    pub kernel_release: String,
    pub kernel_build: String,
    pub machine: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RenderRuntimeImageKind {
    Helper,
    Avcodec,
    Avformat,
    Avutil,
    Swscale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RenderEncodeFailureKind {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "stage",
    content = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RenderProbeFailureKind {
    Control,
    Contract,
    Source,
    Picture,
    Audio,
    Output,
    Encoder(RenderEncodeFailureKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "stage",
    content = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RenderAdmissionFailureKind {
    Configuration,
    Protocol,
    Worker,
    WorkerFault,
    Cancelled,
    Deadline,
    Picture,
    Output,
    Audio,
    Encode(RenderEncodeFailureKind),
    Render,
    Supervisor,
    Artifact,
    RetainedMedia,
    Io,
    Json,
    UnresolvedCleanup,
    WorkerFailure(RenderProbeFailureKind),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderAdmissionFailure {
    pub kind: RenderAdmissionFailureKind,
    pub diagnostic: Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum RenderProbeOutcome {
    Succeeded {
        report: Box<RenderProbeReport>,
    },
    Rejected {
        failure: RenderProbeFailure,
        #[serde(deserialize_with = "required_option")]
        runtime: Option<Box<RenderRuntimeFingerprint>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderProbeObservation {
    pub ordinal: u32,
    pub identity: RenderProbeIdentity,
    pub spec: RenderProbeSpec,
    pub result: RenderProbeOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum RenderDecisionOutcome {
    Selected { probe_ordinal: u32 },
    Rejected { failure: RenderAdmissionFailure },
    Aborted { failure: RenderAdmissionFailure },
}

/// One immutable observation from an original encoding attempt. The complete
/// project output is distinct from each short synthetic probe's contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderEncodingDecision {
    pub schema_version: u32,
    pub job_id: RequestId,
    pub encoding_attempt_id: AttemptId,
    pub algorithm: RenderAutomaticAlgorithm,
    pub document_sha256: Sha256,
    pub output: RenderPictureContract,
    #[serde(deserialize_with = "required_option")]
    pub runtime: Option<RenderAdmissionRuntime>,
    pub probes: Vec<RenderProbeObservation>,
    pub outcome: RenderDecisionOutcome,
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(deserializer)
}

fn ensure(valid: bool, reason: &'static str) -> Result<(), RenderError> {
    if valid {
        Ok(())
    } else {
        Err(RenderError::Invalid(reason))
    }
}

impl RenderEncodingDecision {
    pub fn from_json(bytes: &[u8]) -> Result<Self, RenderError> {
        ensure(
            bytes.len() <= MAX_RENDER_DECISION_BYTES,
            "automatic decision byte bound",
        )?;
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        Ok(value)
    }

    pub fn selected(&self) -> Option<&RenderProbeReport> {
        let RenderDecisionOutcome::Selected { probe_ordinal } = self.outcome else {
            return None;
        };
        let probe = self.probes.get(usize::try_from(probe_ordinal).ok()?)?;
        match &probe.result {
            RenderProbeOutcome::Succeeded { report } => Some(report),
            RenderProbeOutcome::Rejected { .. } => None,
        }
    }

    pub const fn is_selected(&self) -> bool {
        matches!(self.outcome, RenderDecisionOutcome::Selected { .. })
    }

    pub const fn terminal_failure(&self) -> Option<&RenderAdmissionFailure> {
        match &self.outcome {
            RenderDecisionOutcome::Rejected { failure }
            | RenderDecisionOutcome::Aborted { failure } => Some(failure),
            RenderDecisionOutcome::Selected { .. } => None,
        }
    }

    pub fn cancelled(&self) -> bool {
        self.terminal_failure().is_some_and(|failure| {
            matches!(
                failure.kind,
                RenderAdmissionFailureKind::Cancelled
                    | RenderAdmissionFailureKind::Encode(RenderEncodeFailureKind::Cancelled)
                    | RenderAdmissionFailureKind::WorkerFailure(RenderProbeFailureKind::Encoder(
                        RenderEncodeFailureKind::Cancelled
                    ))
            )
        })
    }

    pub fn unresolved_cleanup(&self) -> bool {
        self.terminal_failure().is_some_and(|failure| {
            matches!(failure.kind, RenderAdmissionFailureKind::UnresolvedCleanup)
        })
    }

    pub fn validate_for(
        &self,
        intent: &RenderIntent,
        attempt: &AttemptId,
    ) -> Result<(), RenderError> {
        intent.validate()?;
        self.validate()?;
        ensure(
            intent
                .policy
                .automatic()
                .is_some_and(|policy| policy.algorithm == self.algorithm)
                && self.job_id == intent.job_id
                && &self.encoding_attempt_id == attempt
                && self.document_sha256 == intent.document_sha256
                && self.output.project_id == intent.project_id
                && self.output.revision_id == intent.revision_id
                && self.output.range == intent.range,
            "automatic decision belongs to a different intent or encoding owner",
        )
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        ensure(
            self.schema_version == 1 && self.probes.len() <= MAX_RENDER_PROBES,
            "automatic decision version or probe bound",
        )?;
        self.output.validate()?;
        if let Some(runtime) = &self.runtime {
            runtime.validate()?;
        }
        let rate = [
            self.output.frame_rate.numerator(),
            self.output.frame_rate.denominator(),
        ];
        let settings =
            RenderSdrSettings::automatic_sdr_v1(self.output.raster, rate, RenderBFrames::None)?;
        let allow_b = settings.gop_frames > 2;
        let mut next = Some(RenderEncoderChoice {
            mode: RenderEncoder::Hardware,
            b_frames: if allow_b {
                RenderBFrames::TargetTwo
            } else {
                RenderBFrames::None
            },
        });
        let mut previous_runtime = None;
        let mut succeeded = None;
        for (ordinal, probe) in self.probes.iter().enumerate() {
            ensure(
                usize::try_from(probe.ordinal).ok() == Some(ordinal)
                    && probe.identity.request_id == self.job_id
                    && probe.identity.attempt_id != self.encoding_attempt_id
                    && !self.probes[..ordinal]
                        .iter()
                        .any(|previous| previous.identity.attempt_id == probe.identity.attempt_id)
                    && probe.spec.raster == self.output.raster
                    && probe.spec.frame_rate == rate
                    && next.as_ref() == Some(&probe.spec.choice),
                "automatic probe order, identity or output binding differs",
            )?;
            probe.spec.config()?;
            let observed_runtime = match &probe.result {
                RenderProbeOutcome::Succeeded { report } => {
                    ensure(
                        ordinal + 1 == self.probes.len() && report.spec == probe.spec,
                        "selected probe is not the final matching observation",
                    )?;
                    report.validate()?;
                    succeeded = Some(probe.ordinal);
                    next = None;
                    Some(&report.runtime)
                }
                RenderProbeOutcome::Rejected { failure, runtime } => {
                    next = next_choice(&probe.spec.choice, failure.kind, allow_b);
                    if ordinal + 1 < self.probes.len() {
                        ensure(
                            next.is_some() && runtime.is_some(),
                            "fallback requires a typed eligible rejection and checked runtime",
                        )?;
                    }
                    runtime.as_deref()
                }
            };
            if let Some(runtime) = observed_runtime {
                runtime.validate()?;
                // An aborted final rejection may be exactly where the host
                // detected changed runtime evidence. Retain that observation;
                // it authorizes no subsequent fallback or selection.
                let final_aborted = ordinal + 1 == self.probes.len()
                    && matches!(self.outcome, RenderDecisionOutcome::Aborted { .. });
                if !final_aborted {
                    if let Some(host) = &self.runtime {
                        host.matches_loaded(runtime)?;
                    }
                    if let Some(previous) = previous_runtime {
                        ensure(
                            previous == runtime,
                            "loaded encoder runtime changed between probes",
                        )?;
                    }
                }
                previous_runtime = Some(runtime);
            }
        }
        match &self.outcome {
            RenderDecisionOutcome::Selected { probe_ordinal } => {
                ensure(
                    self.runtime.is_some() && succeeded == Some(*probe_ordinal),
                    "decision selection lacks its full successful probe",
                )?;
            }
            RenderDecisionOutcome::Rejected { failure } => {
                ensure(
                    succeeded.is_none(),
                    "rejected decision contains a successful probe",
                )?;
                let Some(RenderProbeObservation {
                    result: RenderProbeOutcome::Rejected { failure: last, .. },
                    ..
                }) = self.probes.last()
                else {
                    return Err(RenderError::Invalid(
                        "rejected decision has no rejected probe",
                    ));
                };
                ensure(
                    failure.kind == RenderAdmissionFailureKind::WorkerFailure(last.kind)
                        && failure.diagnostic == last.diagnostic
                        && next.is_none(),
                    "rejected outcome differs from its terminal worker observation",
                )?;
            }
            RenderDecisionOutcome::Aborted { .. } => {
                ensure(
                    succeeded.is_none(),
                    "aborted decision contains a selected probe",
                )?;
            }
        }
        let mut size = BoundedCount {
            bytes: 0,
            limit: MAX_RENDER_DECISION_BYTES,
        };
        serde_json::to_writer(&mut size, self)?;
        Ok(())
    }
}

/// Frozen AutomaticSdrV1 transitions. Generic codec/open, I/O, capacity,
/// verification, cancellation and supervision faults cannot select a fallback.
fn next_choice(
    choice: &RenderEncoderChoice,
    failure: RenderProbeFailureKind,
    allow_b: bool,
) -> Option<RenderEncoderChoice> {
    let RenderProbeFailureKind::Encoder(kind) = failure else {
        return None;
    };
    match (choice.mode, choice.b_frames, kind) {
        (_, RenderBFrames::TargetTwo, RenderEncodeFailureKind::VideoTimestampOrder) => {
            Some(RenderEncoderChoice {
                mode: choice.mode,
                b_frames: RenderBFrames::None,
            })
        }
        (RenderEncoder::Hardware, _, RenderEncodeFailureKind::EncoderUnavailable)
        | (
            RenderEncoder::Hardware,
            RenderBFrames::None,
            RenderEncodeFailureKind::VideoTimestampOrder,
        ) => Some(RenderEncoderChoice {
            mode: RenderEncoder::Software,
            b_frames: if allow_b {
                RenderBFrames::TargetTwo
            } else {
                RenderBFrames::None
            },
        }),
        _ => None,
    }
}

impl RenderPictureContract {
    pub fn nearest_even_raster(canvas: [u32; 2]) -> Result<[u32; 2], RenderError> {
        validate_raster(canvas, false)?;
        let raster = canvas.map(|axis| (axis - axis % 2).max(2));
        validate_raster(raster, true)?;
        Ok(raster)
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        let frames = self.range.duration().frames();
        let terminal = frames.checked_mul(i64::from(self.frame_rate.denominator()));
        let raster = Self::nearest_even_raster(self.canvas)?;
        let aspect = ExactRatio::new(i128::from(raster[0]), i128::from(raster[1]))
            .and_then(|value| {
                value.checked_div(ExactRatio::new(
                    i128::from(self.canvas[0]),
                    i128::from(self.canvas[1]),
                )?)
            })
            .and_then(|value| value.checked_sub(ExactRatio::ONE))
            .map_err(|_| RenderError::Invalid("output aspect arithmetic"))?;
        let start = self
            .frame_rate
            .audio_boundary(self.range.start())
            .map_err(|_| RenderError::Invalid("output audio start overflow"))?;
        let end = self
            .frame_rate
            .audio_boundary(self.range.end())
            .map_err(|_| RenderError::Invalid("output audio end overflow"))?;
        ensure(
            self.range.start().0 >= 0
                && frames > 0
                && u64::try_from(frames).ok() == Some(self.frame_count)
                && self.frame_count <= 1_000_000
                && self.raster == raster
                && self.color_policy == ColorPolicy::SdrRec709
                && self.relative_aspect_error == aspect
                && self.time_base.numerator == 1
                && self.time_base.denominator == self.frame_rate.numerator()
                && terminal == Some(self.terminal_pts)
                && self.project_audio_start == start
                && self.project_audio_end == end
                && end
                    .0
                    .checked_sub(start.0)
                    .is_some_and(|samples| (1..=4_147_200_000).contains(&samples))
                && i128::from(self.terminal_pts)
                    <= 86_400 * i128::from(self.frame_rate.numerator()),
            "automatic output geometry or exact clocks differ",
        )?;
        RenderSdrSettings::automatic_sdr_v1(
            self.raster,
            [self.frame_rate.numerator(), self.frame_rate.denominator()],
            RenderBFrames::None,
        )?;
        Ok(())
    }
}

fn validate_raster(raster: [u32; 2], even: bool) -> Result<(), RenderError> {
    ensure(
        raster
            .into_iter()
            .all(|axis| (1..=8_192).contains(&axis) && (!even || axis.is_multiple_of(2)))
            && u64::from(raster[0]) * u64::from(raster[1]) <= 16_777_216,
        "automatic output raster exceeds the shared picture bounds",
    )
}

impl RenderSdrSettings {
    /// Frozen companion to native EncodeContract::new_v1. Version changes must
    /// add a resolver; retained values never follow a new default policy.
    pub fn automatic_sdr_v1(
        raster: [u32; 2],
        rate: [u32; 2],
        b_frames: RenderBFrames,
    ) -> Result<Self, RenderError> {
        validate_raster(raster, true)?;
        let [n, d] = rate;
        ensure(
            n > 0
                && d > 0
                && i32::try_from(n).is_ok()
                && i32::try_from(d).is_ok()
                && gcd(n, d) == 1
                && u64::from(n) <= 60 * u64::from(d),
            "automatic frame rate is unsupported",
        )?;
        let timescale = u64::from(n / gcd(n, 48_000)) * 48_000;
        ensure(
            timescale <= 2_147_483_647,
            "automatic movie timescale overflow",
        )?;
        let gop_denominator = 2 * u64::from(d);
        let gop = (u64::from(n) / gop_denominator
            + u64::from(u64::from(n) % gop_denominator * 2 >= gop_denominator))
        .max(1);
        Ok(Self {
            video_bitrate: bitrate_v1(
                u64::from(raster[0]) * u64::from(raster[1]),
                u64::from(n) > 30 * u64::from(d),
            ),
            audio_bitrate: 384_000,
            gop_frames: u32::try_from(gop).map_err(|_| RenderError::Invalid("GOP overflow"))?,
            b_frames: match b_frames {
                RenderBFrames::None => 0,
                RenderBFrames::TargetTwo => 2,
            },
            movie_timescale: u32::try_from(timescale)
                .map_err(|_| RenderError::Invalid("timescale overflow"))?,
        })
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn bitrate_v1(pixels: u64, high_rate: bool) -> u64 {
    const CLASSES: [(u64, u64, u64); 7] = [
        (360, 1_500_000, 2_000_000),
        (480, 3_000_000, 4_000_000),
        (720, 5_000_000, 7_500_000),
        (1080, 8_000_000, 12_000_000),
        (1440, 16_000_000, 24_000_000),
        (2160, 45_000_000, 68_000_000),
        (4320, 160_000_000, 240_000_000),
    ];
    let choose = |value: (u64, u64, u64)| if high_rate { value.2 } else { value.1 };
    let area = pixels * 9;
    if area <= CLASSES[0].0 * CLASSES[0].0 * 16 {
        return choose(CLASSES[0]);
    }
    for pair in CLASSES.windows(2) {
        let [lower, upper] = [pair[0], pair[1]];
        let low = lower.0 * lower.0 * 16;
        let high = upper.0 * upper.0 * 16;
        if area <= high {
            // The shared 16M-pixel bound keeps this product below u64::MAX.
            return choose(lower)
                + ((area - low) * (choose(upper) - choose(lower)) + (high - low) / 2)
                    / (high - low);
        }
    }
    choose(CLASSES[CLASSES.len() - 1])
}

fn runtime_text(value: &str) -> Result<(), RenderError> {
    ensure(
        !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control),
        "runtime text bound",
    )
}

impl RenderAdmissionRuntime {
    pub fn validate(&self) -> Result<(), RenderError> {
        ensure(
            (1..=512 * 1024 * 1024).contains(&self.helper_bytes),
            "runtime helper byte bound",
        )?;
        for value in [
            &self.system,
            &self.kernel_release,
            &self.kernel_build,
            &self.machine,
        ] {
            runtime_text(value)?;
        }
        Ok(())
    }

    pub fn matches_loaded(&self, loaded: &RenderRuntimeFingerprint) -> Result<(), RenderError> {
        self.validate()?;
        loaded.validate()?;
        ensure(
            self.helper_sha256 == loaded.images[0].sha256
                && self.helper_bytes == loaded.images[0].mapped.file_size
                && self.system == loaded.system
                && self.kernel_release == loaded.kernel_release
                && self.kernel_build == loaded.kernel_build
                && self.machine == loaded.machine,
            "host and loaded encoder runtime differ",
        )
    }
}

impl RenderRuntimeFingerprint {
    pub fn validate(&self) -> Result<(), RenderError> {
        ensure(
            self.schema_version == 1 && self.platform.cpu_family != 0,
            "runtime version or CPU identity",
        )?;
        for value in [&self.platform.os_build, &self.platform.hardware_model] {
            ensure(
                !value.is_empty()
                    && value.len() <= 63
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b".,_-".contains(&byte)),
                "runtime platform identity",
            )?;
        }
        for value in [
            &self.system,
            &self.kernel_release,
            &self.kernel_build,
            &self.machine,
        ] {
            runtime_text(value)?;
        }
        for (image, kind) in self.images.iter().zip([
            RenderRuntimeImageKind::Helper,
            RenderRuntimeImageKind::Avcodec,
            RenderRuntimeImageKind::Avformat,
            RenderRuntimeImageKind::Avutil,
            RenderRuntimeImageKind::Swscale,
        ]) {
            let mapped = &image.mapped;
            ensure(
                mapped.kind == kind
                    && (1..=u64::from(u32::MAX)).contains(&mapped.device)
                    && mapped.inode != 0
                    && mapped.uuid != [0; 16]
                    && (1..=512 * 1024 * 1024).contains(&mapped.file_size)
                    && [
                        &mapped.modification_time,
                        &mapped.change_time,
                        &mapped.birth_time,
                    ]
                    .into_iter()
                    .all(|time| time.nanoseconds < 1_000_000_000),
                "runtime mapped-image identity, order or bound",
            )?;
        }
        Ok(())
    }
}

// Field order is part of the domain-separated recipe hash used by the actual
// EncoderProbe v1. Keep this frozen with that implementation, including names.
#[derive(Serialize)]
struct ProbeConfigV1 {
    version: u32,
    raster: [u32; 2],
    frame_rate: [u32; 2],
    video_frames: u64,
    audio_samples: u64,
    gop_frames: u32,
    picture_bytes: u64,
}

impl RenderProbeSpec {
    fn config(&self) -> Result<ProbeConfigV1, RenderError> {
        ensure(
            self.raster[0] >= 14 && self.raster[1] >= 16,
            "probe raster cannot distinguish its fixed content",
        )?;
        let settings = RenderSdrSettings::automatic_sdr_v1(
            self.raster,
            self.frame_rate,
            self.choice.b_frames,
        )?;
        let frames = u64::from(settings.gop_frames) * 3 + 1;
        ensure(
            frames <= 121
                && frames * u64::from(self.frame_rate[1]) <= 8 * u64::from(self.frame_rate[0]),
            "probe duration or frame bound",
        )?;
        let rate = FrameRate::new(self.frame_rate[0], self.frame_rate[1])
            .map_err(|_| RenderError::Invalid("probe rate"))?;
        let samples = rate
            .audio_boundary(ProjectFrame(
                i64::try_from(frames).map_err(|_| RenderError::Invalid("probe frames"))?,
            ))
            .map_err(|_| RenderError::Invalid("probe samples"))?;
        Ok(ProbeConfigV1 {
            version: 1,
            raster: self.raster,
            frame_rate: self.frame_rate,
            video_frames: frames,
            audio_samples: u64::try_from(samples.0)
                .map_err(|_| RenderError::Invalid("probe samples"))?,
            gop_frames: settings.gop_frames,
            picture_bytes: u64::from(self.raster[0]) * u64::from(self.raster[1]) * 3 / 2,
        })
    }

    pub fn document_sha256(&self) -> Result<Sha256, RenderError> {
        let mut hasher = Hasher::new();
        hasher.update(b"deadpan-encoder-admission-probe-v1\0");
        hasher.update(serde_json::to_vec(&self.config()?)?);
        let hex: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Sha256::new(hex).map_err(|_| RenderError::Invalid("probe recipe hash"))
    }

    pub fn contract(&self) -> Result<RenderProbeContract, RenderError> {
        let config = self.config()?;
        let frames =
            i64::try_from(config.video_frames).map_err(|_| RenderError::Invalid("probe frames"))?;
        Ok(RenderProbeContract {
            choice: self.choice.clone(),
            picture: RenderPictureContract {
                project_id: ProjectId::new("encoder-admission-probe-v1")
                    .map_err(|_| RenderError::Invalid("probe project"))?,
                revision_id: RevisionId::new("deterministic-probe-v1")
                    .map_err(|_| RenderError::Invalid("probe revision"))?,
                range: FrameRange::new(ProjectFrame(0), ProjectFrame(frames))
                    .map_err(|_| RenderError::Invalid("probe range"))?,
                canvas: config.raster,
                raster: config.raster,
                frame_rate: FrameRate::new(config.frame_rate[0], config.frame_rate[1])
                    .map_err(|_| RenderError::Invalid("probe rate"))?,
                color_policy: ColorPolicy::SdrRec709,
                time_base: RenderOutputTimeBase {
                    numerator: 1,
                    denominator: config.frame_rate[0],
                },
                frame_count: config.video_frames,
                terminal_pts: frames * i64::from(config.frame_rate[1]),
                project_audio_start: AudioSample(0),
                project_audio_end: AudioSample(
                    i64::try_from(config.audio_samples)
                        .map_err(|_| RenderError::Invalid("probe audio"))?,
                ),
                relative_aspect_error: ExactRatio::ZERO,
            },
        })
    }
}

impl RenderProbeReport {
    pub fn validate(&self) -> Result<(), RenderError> {
        self.runtime.validate()?;
        let config = self.spec.config()?;
        let contract = self.spec.contract()?;
        let recipe = self.spec.document_sha256()?;
        let settings = RenderSdrSettings::automatic_sdr_v1(
            self.spec.raster,
            self.spec.frame_rate,
            self.spec.choice.b_frames,
        )?;
        ensure(
            self.schema_version == 2
                && self.settings == settings
                && self.manifest.contract == contract
                && self.manifest.document_sha256 == recipe
                && self.verification.contract == contract
                && self.verification.document_sha256 == recipe
                && self.manifest.movie.reference().as_str() == "output/movie.mp4"
                && self.manifest.movie.byte_length() == self.manifest.report.output_bytes
                && self.verification.movie_sha256 == *self.manifest.movie.sha256()
                && self.verification.movie_bytes == self.manifest.movie.byte_length(),
            "probe report changed deterministic input, controls or movie identity",
        )?;
        validate_encode_report(
            &self.manifest.report,
            &config,
            &settings,
            self.spec.choice.mode,
        )?;
        validate_verification(
            &self.verification,
            &self.manifest.report,
            &config,
            &settings,
        )?;
        validate_content(&self.content, &config)
    }
}

fn validate_encode_report(
    report: &RenderEncodeReport,
    config: &ProbeConfigV1,
    settings: &RenderSdrSettings,
    mode: RenderEncoder,
) -> Result<(), RenderError> {
    let packets = report
        .video_packets
        .checked_add(report.audio_packets)
        .ok_or(RenderError::Invalid("probe packet overflow"))?;
    ensure(
        report.video_frames == config.video_frames
            && report.audio_samples == config.audio_samples
            && report.video_packets == config.video_frames
            && report.audio_packets == config.audio_samples.div_ceil(1024) + 1
            && packets <= MAX_PROBE_PACKETS
            && (1..=MAX_PROBE_BYTES).contains(&report.output_bytes)
            && report.packet_bytes >= packets
            && report.packet_bytes <= report.output_bytes
            && report.packet_bytes <= packets * 32 * 1024 * 1024
            && report.video_duration_from_contract_packets <= report.video_packets
            && report.faststart_read_opens == 1
            && report.faststart_read_closes == 1
            && report.video_eof
            && report.audio_eof,
        "probe encode counts, byte bounds, drain or fast-start differ",
    )?;
    let info = &report.info;
    let moov_packets = info
        .maximum_moov_bytes
        .checked_sub(1_048_576)
        .filter(|bytes| bytes.is_multiple_of(128))
        .map(|bytes| bytes / 128);
    ensure(
        info.abi_version == 1
            && [
                info.avcodec_version,
                info.avformat_version,
                info.avutil_version,
            ]
            .into_iter()
            .all(|version| (1..=0x00ff_ffff).contains(&version))
            && info.movie_timescale == settings.movie_timescale
            && info.video_time_base_num == 1
            && info.video_time_base_den == config.frame_rate[0]
            && info.audio_time_base_num == 1
            && info.audio_time_base_den == 48_000
            && info.audio_frame_size == 1024
            && info.video_profile == 100
            && info.audio_profile == 1
            && info.requested_mode == mode
            && u32::try_from(info.video_has_b_frames).is_ok_and(|value| value <= settings.b_frames)
            && u32::try_from(info.video_max_b_frames).ok() == Some(settings.b_frames)
            && u32::try_from(info.video_gop_size).ok() == Some(settings.gop_frames)
            && (0..=8192).contains(&info.audio_initial_padding)
            && (0..=8192).contains(&info.audio_trailing_padding)
            && info.video_bitrate == settings.video_bitrate
            && info.audio_bitrate == settings.audio_bitrate
            && moov_packets.is_some_and(|value| value <= MAX_PROBE_PACKETS && value > packets),
        "probe queried codec, clock or policy differs",
    )
}

fn validate_verification(
    report: &RenderProbeVerification,
    encoded: &RenderEncodeReport,
    config: &ProbeConfigV1,
    settings: &RenderSdrSettings,
) -> Result<(), RenderError> {
    let ticks = i64::from(config.frame_rate[1]);
    ensure(
        report.policy_version == 1
            && report.video_frames == config.video_frames
            && report.audio_samples == config.audio_samples
            && report.video_packets == encoded.video_packets
            && report.audio_packets == encoded.audio_packets
            && report.gops > 0
            && report.gops <= config.video_frames
            && report.gops
                >= config
                    .video_frames
                    .div_ceil(u64::from(settings.gop_frames) + 1)
            && report.fresh_gop_frames == config.video_frames
            && report.runtime_versions == [4_066_151, 4_064_103, 3_934_311]
            && report.movie_timescale == settings.movie_timescale
            && report.video_edit_media_time >= 0
            && report.video_edit_media_time % ticks == 0
            && report.video_edit_media_time <= i64::from(settings.b_frames) * ticks
            && report.audio_edit_media_time == 1024
            && report.manual_first_sample == -1024
            && report.ordinary_first_sample == 0
            && report.manual_physical_samples == report.audio_packets * 1024
            && report.ordinary_physical_samples == config.audio_samples.div_ceil(1024) * 1024
            && report.maximum_b_run <= settings.b_frames
            && (settings.b_frames == 0
                || config.video_frames <= u64::from(settings.gop_frames)
                || report.maximum_b_run != 0),
        "probe full-file verification differs from frozen policy",
    )
}

fn milli(error: u64, samples: u64) -> Result<u32, RenderError> {
    ensure(samples != 0, "probe plane has no samples")?;
    u32::try_from((u128::from(error) * 1000).div_ceil(u128::from(samples)))
        .map_err(|_| RenderError::Invalid("probe statistic overflow"))
}

fn validate_content(
    report: &RenderProbeContent,
    config: &ProbeConfigV1,
) -> Result<(), RenderError> {
    let pixels = u64::from(config.raster[0]) * u64::from(config.raster[1]);
    let plane_samples =
        [pixels, pixels / 4, pixels / 4].map(|samples| samples * config.video_frames);
    ensure(
        report.schema_version == 1
            && report.video_frames == config.video_frames
            && report.audio_samples == config.audio_samples
            && report.plane_samples == plane_samples
            && report.unexpected_audio_peak.is_finite()
            && (0.0..=0.15).contains(&report.unexpected_audio_peak),
        "probe content fixture, extent or audio noise differs",
    )?;
    for (plane, samples) in plane_samples.into_iter().enumerate() {
        let maximum = report.maximum_plane_error[plane];
        let absolute = report.absolute_plane_error[plane];
        let squared = report.squared_plane_error[plane];
        let mae = milli(absolute, samples)?;
        let mse = milli(squared, samples)?;
        ensure(
            maximum <= 48
                && mae <= 1_500
                && mse <= 16_000
                && u128::from(absolute) <= u128::from(samples) * u128::from(maximum)
                && u128::from(squared) <= u128::from(absolute) * u128::from(maximum)
                && squared >= absolute
                && absolute >= u64::from(maximum)
                && squared >= u64::from(maximum).pow(2)
                && u128::from(absolute).pow(2) <= u128::from(samples) * u128::from(squared)
                && report.worst_frame_mean_absolute_error_milli[plane] <= 1_500
                && report.worst_frame_mean_squared_error_milli[plane] <= 16_000
                && report.worst_frame_mean_absolute_error_milli[plane] >= mae
                && report.worst_frame_mean_squared_error_milli[plane] >= mse,
            "probe decoded picture errors exceed fixed limits",
        )?;
    }
    let samples = config.audio_samples;
    ensure(samples >= 400, "probe audio fixture is too short")?;
    let markers = [
        [100, 137],
        [samples / 2, samples / 2 + 37],
        [samples - 200, samples - 163],
    ];
    for (observation, expected) in report.markers.iter().zip(markers) {
        ensure(
            observation.expected_samples == expected && observation.observed_samples == expected,
            "probe audio event changed exact sample position",
        )?;
        for (channel, peak) in observation.observed_peaks.iter().enumerate() {
            ensure(
                peak.is_finite() && peak.abs() >= 0.15 && peak.is_sign_positive() == (channel == 0),
                "probe audio event missing or changed channel/sign",
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
