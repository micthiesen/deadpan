//! Strict evidence for one process-isolated committed encoding attempt.
//!
//! The child reconstructs the committed picture/audio contract from its fixed
//! package argument. Neither deserialized claims nor a completed mux report
//! establish finished-file verification or destination publication authority.

use std::io::{Read, Write};

use deadpan_encode::{
    AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE, BFramePolicy, EncodeContract, EncodeLimits,
    EncodeReport, EncoderMode, MAX_AUDIO_SAMPLES, MAX_OUTPUT_BYTES, MAX_PACKET_BYTES, MAX_PACKETS,
    MAX_VIDEO_FRAMES,
};
use deadpan_jobs::{
    CancellationToken, Diagnostic, Sha256, WorkspaceArtifact, WorkspaceRef,
    process::{ResponseKind, SupervisorError, WorkerProtocol},
    protocol::{read_frame, write_frame},
};
use serde::{Deserialize, Serialize};

use crate::export_picture::ExportPictureContract;
pub use crate::render_worker::protocol::{RenderContract, RenderIdentity};

pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_TIMEOUT_MILLIS: u64 = 24 * 60 * 60 * 1_000;
pub const OUTPUT_SCOPE: &str = "output";
pub const MOVIE_REF: &str = "output/movie.mp4";
// Native encoder.c bounds mux tables and its two relocation buffers with this
// formula. The report must retain the exact packet budget chosen by the host.
const MOOV_BYTES_PER_PACKET: u64 = 128;
const MOOV_FIXED_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncoderChoice {
    pub mode: EncoderMode,
    pub b_frames: BFramePolicy,
}

/// Serialized evidence only. The trusted native contract has no deserializer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodedRenderContract {
    pub picture: RenderContract,
    pub choice: EncoderChoice,
}

impl EncodedRenderContract {
    pub fn from_contract(contract: &ExportPictureContract, choice: EncoderChoice) -> Self {
        Self {
            picture: RenderContract::from_contract(contract),
            choice,
        }
    }

    pub fn native_contract(&self) -> Result<EncodeContract, String> {
        self.picture.validate_for_encoding()?;
        let audio_samples = self
            .picture
            .project_audio_end
            .0
            .checked_sub(self.picture.project_audio_start.0)
            .and_then(|samples| u64::try_from(samples).ok())
            .ok_or_else(|| "encoded audio interval exceeds exact sample bounds".to_owned())?;
        EncodeContract::new(
            self.picture.raster,
            [
                self.picture.frame_rate.numerator(),
                self.picture.frame_rate.denominator(),
            ],
            self.picture.frame_count,
            audio_samples,
            self.choice.mode,
            self.choice.b_frames,
        )
        .map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.native_contract().map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum EncodedHostMessage {
    Prepare {
        protocol: u32,
        identity: RenderIdentity,
        cancellation_token: CancellationToken,
        contract: Box<EncodedRenderContract>,
        document_sha256: Sha256,
        output_scope: WorkspaceRef,
        limits: EncodeLimits,
        timeout_millis: u64,
    },
    Cancel {
        protocol: u32,
        identity: RenderIdentity,
        cancellation_token: CancellationToken,
    },
}

impl EncodedHostMessage {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Prepare {
                protocol,
                contract,
                output_scope,
                limits,
                timeout_millis,
                ..
            } => {
                validate_version(*protocol)?;
                let native = contract.native_contract()?;
                limits
                    .validate_for(&native)
                    .map_err(|error| error.to_string())?;
                if output_scope.as_str() != OUTPUT_SCOPE {
                    return Err("encoded output scope must be exactly output".into());
                }
                if !(1..=MAX_TIMEOUT_MILLIS).contains(timeout_millis) {
                    return Err("encoded timeout must be between 1 ms and 24 hours".into());
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => validate_version(*protocol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodedManifest {
    pub contract: EncodedRenderContract,
    pub document_sha256: Sha256,
    pub movie: WorkspaceArtifact,
    pub report: EncodeReport,
}

impl EncodedManifest {
    /// Admit bounded, internally consistent claims. This does not decode or
    /// verify the movie, its hash, emitted codec properties or audible timing.
    pub fn validate(&self) -> Result<(), String> {
        let native = self.contract.native_contract()?;
        if self.movie.reference().as_str() != MOVIE_REF {
            return Err("encoded movie must be exactly output/movie.mp4".into());
        }
        if self.movie.byte_length() != self.report.output_bytes {
            return Err("encoded movie length differs from its native report".into());
        }
        validate_report(&native, &self.report)
    }

    /// Bind the native allocation and count claims to the host's exact budgets.
    /// Aggregate packet bytes cannot prove each packet's size; native admission
    /// and the later independent verifier remain responsible for actual bytes.
    pub fn validate_for(&self, limits: EncodeLimits) -> Result<(), String> {
        self.validate()?;
        let native = self.contract.native_contract()?;
        limits
            .validate_for(&native)
            .map_err(|error| error.to_string())?;
        let packets = self
            .report
            .video_packets
            .checked_add(self.report.audio_packets)
            .ok_or_else(|| "encoded packet count overflow".to_owned())?;
        let aggregate = packets
            .checked_mul(limits.maximum_packet_bytes)
            .ok_or_else(|| "encoded packet byte budget overflow".to_owned())?;
        if self.report.output_bytes > limits.maximum_output_bytes
            || packets > limits.maximum_packets
            || self.report.packet_bytes > aggregate
            || self.report.info.maximum_moov_bytes != moov_bound(limits.maximum_packets)?
        {
            return Err("encoded report differs from the host's admitted limits".into());
        }
        Ok(())
    }
}

fn validate_report(contract: &EncodeContract, report: &EncodeReport) -> Result<(), String> {
    let expected_audio_packets = contract
        .audio_samples()
        .div_ceil(u64::from(AUDIO_FRAME_SAMPLES))
        + 1;
    let packets = report
        .video_packets
        .checked_add(report.audio_packets)
        .ok_or_else(|| "encoded packet count overflow".to_owned())?;
    if report.video_frames != contract.video_frames()
        || report.audio_samples != contract.audio_samples()
        || report.video_packets != contract.video_frames()
        || report.audio_packets != expected_audio_packets
        || packets > MAX_PACKETS
        || !(1..=MAX_OUTPUT_BYTES).contains(&report.output_bytes)
        || report.packet_bytes < packets
        || report.packet_bytes > report.output_bytes
        || report.packet_bytes
            > packets
                .checked_mul(MAX_PACKET_BYTES)
                .ok_or_else(|| "encoded packet byte count overflow".to_owned())?
        || report.video_duration_from_contract_packets > report.video_packets
        || report.faststart_read_opens != 1
        || report.faststart_read_closes != 1
        || !report.video_eof
        || !report.audio_eof
    {
        return Err(
            "encoded counts, drain or fast-start claims differ from the captured input".into(),
        );
    }
    let info = &report.info;
    let policy = contract.policy();
    let has_b = u32::try_from(info.video_has_b_frames).ok();
    let max_b = u32::try_from(info.video_max_b_frames).ok();
    let gop = u32::try_from(info.video_gop_size).ok();
    let moov_packets = info
        .maximum_moov_bytes
        .checked_sub(MOOV_FIXED_BYTES)
        .filter(|bytes| bytes.is_multiple_of(MOOV_BYTES_PER_PACKET))
        .map(|bytes| bytes / MOOV_BYTES_PER_PACKET);
    if info.abi_version != 1
        || [
            info.avcodec_version,
            info.avformat_version,
            info.avutil_version,
        ]
        .into_iter()
        .any(|version| !(1..=0x00ff_ffff).contains(&version))
        || info.movie_timescale != policy.movie_timescale
        || info.video_time_base_num != 1
        || info.video_time_base_den != contract.frame_rate()[0]
        || info.audio_time_base_num != 1
        || info.audio_time_base_den != AUDIO_SAMPLE_RATE
        || info.audio_frame_size != AUDIO_FRAME_SAMPLES
        || info.video_profile != 100
        || info.audio_profile != 1
        || info.requested_mode != contract.mode()
        || has_b.is_none_or(|value| value > policy.b_frames)
        || max_b != Some(policy.b_frames)
        || gop != Some(policy.gop_frames)
        || !(0..=8_192).contains(&info.audio_initial_padding)
        || !(0..=8_192).contains(&info.audio_trailing_padding)
        || info.video_bitrate != policy.video_bitrate
        || info.audio_bitrate != policy.audio_bitrate
        || moov_packets.is_none_or(|value| value > MAX_PACKETS || value <= packets)
    {
        return Err("encoded codec, clock, policy or allocation claims are inconsistent".into());
    }
    Ok(())
}

fn moov_bound(packets: u64) -> Result<u64, String> {
    packets
        .checked_mul(MOOV_BYTES_PER_PACKET)
        .and_then(|bytes| bytes.checked_add(MOOV_FIXED_BYTES))
        .ok_or_else(|| "encoded MP4 metadata bound overflow".to_owned())
}

/// The failed boundary, carried separately from human diagnostic text. These
/// observations never authorize a fallback or establish process cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "stage",
    content = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EncodedFailureKind {
    Control,
    Contract,
    Source,
    Picture,
    Audio,
    Output,
    Encoder(deadpan_encode::EncodeFailureKind),
}

impl EncodedFailureKind {
    /// Stable operational diagnostic code, independent of arbitrary messages.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Control => "render_control_failed",
            Self::Contract => "render_contract_failed",
            Self::Source => "render_source_failed",
            Self::Picture => "render_picture_failed",
            Self::Audio => "render_audio_failed",
            Self::Output => "render_output_failed",
            Self::Encoder(deadpan_encode::EncodeFailureKind::EncoderUnavailable) => {
                "video_encoder_unavailable"
            }
            Self::Encoder(deadpan_encode::EncodeFailureKind::VideoTimestampOrder) => {
                "video_timestamp_order"
            }
            Self::Encoder(_) => "render_encoder_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodedFailure {
    pub kind: EncodedFailureKind,
    pub diagnostic: Diagnostic,
}

impl std::fmt::Display for EncodedFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {}",
            self.kind.code(),
            self.diagnostic.as_str()
        )
    }
}

impl std::error::Error for EncodedFailure {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum EncodedWorkerMessage {
    Progress {
        protocol: u32,
        identity: RenderIdentity,
        completed_frames: u64,
        total_frames: u64,
        completed_audio_samples: u64,
        total_audio_samples: u64,
    },
    Completed {
        protocol: u32,
        identity: RenderIdentity,
        manifest: Box<EncodedManifest>,
    },
    Failed {
        protocol: u32,
        identity: RenderIdentity,
        failure: EncodedFailure,
    },
    Cancelled {
        protocol: u32,
        identity: RenderIdentity,
    },
}

impl EncodedWorkerMessage {
    pub fn identity(&self) -> &RenderIdentity {
        match self {
            Self::Progress { identity, .. }
            | Self::Completed { identity, .. }
            | Self::Failed { identity, .. }
            | Self::Cancelled { identity, .. } => identity,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Progress {
                protocol,
                completed_frames,
                total_frames,
                completed_audio_samples,
                total_audio_samples,
                ..
            } => {
                validate_version(*protocol)?;
                if !(1..=MAX_VIDEO_FRAMES).contains(total_frames)
                    || !(1..=MAX_AUDIO_SAMPLES).contains(total_audio_samples)
                    || completed_frames > total_frames
                    || completed_audio_samples > total_audio_samples
                {
                    return Err(
                        "encoded progress is outside its bounded frame/sample totals".into(),
                    );
                }
                Ok(())
            }
            Self::Completed {
                protocol, manifest, ..
            } => {
                validate_version(*protocol)?;
                manifest.validate()
            }
            Self::Failed { protocol, .. } | Self::Cancelled { protocol, .. } => {
                validate_version(*protocol)
            }
        }
    }
}

fn validate_version(protocol: u32) -> Result<(), String> {
    if protocol != PROTOCOL_VERSION {
        return Err(format!(
            "unsupported encoded render worker protocol version {protocol}"
        ));
    }
    Ok(())
}

/// Expected immutable inputs for supervision, independent of worker claims.
pub struct EncodedProtocol {
    identity: RenderIdentity,
    cancellation_token: CancellationToken,
    contract: EncodedRenderContract,
    audio_samples: u64,
    document_sha256: Sha256,
    limits: EncodeLimits,
}

impl WorkerProtocol for EncodedProtocol {
    type Request = EncodedHostMessage;
    type Response = EncodedWorkerMessage;

    fn from_request(request: &Self::Request) -> Result<Self, SupervisorError> {
        request.validate().map_err(SupervisorError::Request)?;
        let EncodedHostMessage::Prepare {
            identity,
            cancellation_token,
            contract,
            document_sha256,
            limits,
            ..
        } = request
        else {
            return Err(SupervisorError::Request(
                "initial encoded message must prepare a movie".into(),
            ));
        };
        let native = contract
            .native_contract()
            .map_err(SupervisorError::Request)?;
        Ok(Self {
            identity: identity.clone(),
            cancellation_token: cancellation_token.clone(),
            contract: contract.as_ref().clone(),
            audio_samples: native.audio_samples(),
            document_sha256: document_sha256.clone(),
            limits: *limits,
        })
    }

    fn cancellation(&self) -> Self::Request {
        EncodedHostMessage::Cancel {
            protocol: PROTOCOL_VERSION,
            identity: self.identity.clone(),
            cancellation_token: self.cancellation_token.clone(),
        }
    }

    fn write_request(writer: &mut impl Write, request: &Self::Request) -> Result<(), String> {
        request.validate()?;
        write_frame(writer, request).map_err(|error| error.to_string())
    }

    fn read_response(reader: &mut impl Read) -> Result<Option<Self::Response>, String> {
        let message: Option<EncodedWorkerMessage> =
            read_frame(reader).map_err(|error| error.to_string())?;
        if let Some(message) = &message {
            message.validate()?;
        }
        Ok(message)
    }

    fn classify(&self, response: &Self::Response) -> Result<ResponseKind, String> {
        response.validate()?;
        if response.identity() != &self.identity {
            return Err("encoded response belongs to another request or attempt".into());
        }
        match response {
            EncodedWorkerMessage::Progress {
                total_frames,
                total_audio_samples,
                ..
            } => {
                if *total_frames != self.contract.picture.frame_count
                    || *total_audio_samples != self.audio_samples
                {
                    return Err(
                        "encoded progress differs from the captured frame/sample counts".into(),
                    );
                }
                Ok(ResponseKind::Progress)
            }
            EncodedWorkerMessage::Completed { manifest, .. } => {
                if manifest.contract != self.contract
                    || manifest.document_sha256 != self.document_sha256
                {
                    return Err("encoded completion differs from the captured request".into());
                }
                manifest.validate_for(self.limits)?;
                Ok(ResponseKind::Completed)
            }
            EncodedWorkerMessage::Failed { .. } => Ok(ResponseKind::Failed),
            EncodedWorkerMessage::Cancelled { .. } => Ok(ResponseKind::Terminal),
        }
    }
}

pub fn read_host_message(reader: &mut impl Read) -> Result<Option<EncodedHostMessage>, String> {
    let message: Option<EncodedHostMessage> =
        read_frame(reader).map_err(|error| error.to_string())?;
    if let Some(message) = &message {
        message.validate()?;
    }
    Ok(message)
}

pub fn write_worker_message(
    writer: &mut impl Write,
    message: &EncodedWorkerMessage,
) -> Result<(), String> {
    message.validate()?;
    write_frame(writer, message).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
