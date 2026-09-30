//! Strict evidence messages for one isolated picture preparation attempt.
//!
//! A deserialized contract grants no media authority. The child reconstructs
//! the committed contract from the host-selected package and compares every
//! field before preparing pictures; the host independently verifies the result.

use std::io::{Read, Write};

use deadpan_core::{
    AudioSample, ColorPolicy, ExactRatio, FrameRange, FrameRate, ProjectId, RevisionId,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, Diagnostic, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
    process::{ResponseKind, SupervisorError, WorkerProtocol},
    protocol::{read_frame, write_frame},
};
use deadpan_media::source_import_timing::nearest_even_dimension;
use deadpan_render::validate_working_readback_dimensions;
use serde::{Deserialize, Serialize};

use crate::export_picture::ExportPictureContract;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_PICTURE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_PICTURE_FRAMES: u64 = 100_000;
pub const MAX_TIMEOUT_MILLIS: u64 = 24 * 60 * 60 * 1_000;
pub const OUTPUT_SCOPE: &str = "output";
pub const PICTURE_REF: &str = "output/pictures.i420";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderIdentity {
    pub request_id: RequestId,
    pub attempt_id: AttemptId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderTimeBase {
    pub numerator: u32,
    pub denominator: u32,
}

/// Serialized claims, never a deserializer for the trusted picture contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderContract {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub range: FrameRange,
    pub canvas: [u32; 2],
    pub raster: [u32; 2],
    pub frame_rate: FrameRate,
    pub color_policy: ColorPolicy,
    pub time_base: RenderTimeBase,
    pub frame_count: u64,
    pub terminal_pts: i64,
    pub project_audio_start: AudioSample,
    pub project_audio_end: AudioSample,
    pub relative_aspect_error: ExactRatio,
}

impl RenderContract {
    pub fn from_contract(contract: &ExportPictureContract) -> Self {
        Self {
            project_id: contract.project_id().clone(),
            revision_id: contract.revision_id().clone(),
            range: contract.range(),
            canvas: contract.canvas(),
            raster: contract.raster(),
            frame_rate: contract.frame_rate(),
            color_policy: contract.color_policy(),
            time_base: RenderTimeBase {
                numerator: contract.time_base().numerator(),
                denominator: contract.time_base().denominator(),
            },
            frame_count: contract.frame_count(),
            terminal_pts: contract.terminal_pts(),
            project_audio_start: contract.project_audio_start(),
            project_audio_end: contract.project_audio_end(),
            relative_aspect_error: contract.relative_aspect_error(),
        }
    }

    pub fn matches(&self, contract: &ExportPictureContract) -> bool {
        self == &Self::from_contract(contract)
    }

    /// Check all derivable claims before any source, GPU or artifact allocation.
    pub fn validate(&self) -> Result<(), String> {
        if self.color_policy != ColorPolicy::SdrRec709 {
            return Err("render pictures require the qualified SDR Rec.709 policy".into());
        }
        let frames =
            u64::try_from(self.range.duration().frames()).map_err(|error| error.to_string())?;
        if self.range.start().0 < 0
            || frames == 0
            || frames != self.frame_count
            || frames > MAX_PICTURE_FRAMES
        {
            return Err("render range must contain 1-100000 nonnegative project frames".into());
        }
        validate_working_readback_dimensions(self.canvas[0], self.canvas[1])
            .map_err(|error| error.to_string())?;
        let raster = [
            nearest_even_dimension(ExactRatio::integer(i64::from(self.canvas[0])))
                .map_err(|error| error.to_string())?,
            nearest_even_dimension(ExactRatio::integer(i64::from(self.canvas[1])))
                .map_err(|error| error.to_string())?,
        ];
        if self.raster != raster {
            return Err("render raster does not match the committed canvas rounding rule".into());
        }
        let canvas_aspect = ExactRatio::new(i128::from(self.canvas[0]), i128::from(self.canvas[1]))
            .map_err(|error| error.to_string())?;
        let aspect_error = ExactRatio::new(i128::from(raster[0]), i128::from(raster[1]))
            .and_then(|value| value.checked_div(canvas_aspect))
            .and_then(|value| value.checked_sub(ExactRatio::ONE))
            .map_err(|error| error.to_string())?;
        if self.relative_aspect_error != aspect_error {
            return Err("render aspect evidence does not match canvas and raster".into());
        }
        if i32::try_from(self.frame_rate.numerator()).is_err()
            || i32::try_from(self.frame_rate.denominator()).is_err()
            || self.time_base.numerator != 1
            || self.time_base.denominator != self.frame_rate.numerator()
        {
            return Err("render frame rate or output time base is unsupported".into());
        }
        let terminal_pts = self
            .range
            .duration()
            .frames()
            .checked_mul(i64::from(self.frame_rate.denominator()))
            .ok_or_else(|| "render terminal PTS overflow".to_owned())?;
        let audio_start = self
            .frame_rate
            .audio_boundary(self.range.start())
            .map_err(|error| error.to_string())?;
        let audio_end = self
            .frame_rate
            .audio_boundary(self.range.end())
            .map_err(|error| error.to_string())?;
        if self.terminal_pts != terminal_pts
            || self.project_audio_start != audio_start
            || self.project_audio_end != audio_end
        {
            return Err("render timestamps do not match the exact project origin".into());
        }
        if self.total_bytes()? > MAX_PICTURE_BYTES {
            return Err("render raw picture artifact exceeds 512 MiB".into());
        }
        Ok(())
    }

    pub fn frame_bytes(&self) -> Result<u64, String> {
        let [width, height] = self.raster;
        if width < 2 || height < 2 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            return Err("render I420 raster must have positive even axes".into());
        }
        validate_working_readback_dimensions(width, height).map_err(|error| error.to_string())?;
        u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(3))
            .map(|bytes| bytes / 2)
            .ok_or_else(|| "render frame byte length overflow".into())
    }

    pub fn total_bytes(&self) -> Result<u64, String> {
        self.frame_bytes()?
            .checked_mul(self.frame_count)
            .ok_or_else(|| "render artifact byte length overflow".into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RenderHostMessage {
    Prepare {
        protocol: u32,
        identity: RenderIdentity,
        cancellation_token: CancellationToken,
        contract: Box<RenderContract>,
        document_sha256: Sha256,
        output_scope: WorkspaceRef,
        maximum_output_bytes: u64,
        timeout_millis: u64,
    },
    Cancel {
        protocol: u32,
        identity: RenderIdentity,
        cancellation_token: CancellationToken,
    },
}

impl RenderHostMessage {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Prepare {
                protocol,
                contract,
                output_scope,
                maximum_output_bytes,
                timeout_millis,
                ..
            } => {
                validate_version(*protocol)?;
                contract.validate()?;
                if output_scope.as_str() != OUTPUT_SCOPE {
                    return Err("render output scope must be exactly output".into());
                }
                if *maximum_output_bytes == 0
                    || *maximum_output_bytes > MAX_PICTURE_BYTES
                    || contract.total_bytes()? > *maximum_output_bytes
                {
                    return Err("render output exceeds the admitted byte budget".into());
                }
                if *timeout_millis == 0 || *timeout_millis > MAX_TIMEOUT_MILLIS {
                    return Err("render timeout must be between 1 ms and 24 hours".into());
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => validate_version(*protocol),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPixelPolicy {
    I420Rec709LimitedLeft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderManifest {
    pub contract: RenderContract,
    pub document_sha256: Sha256,
    pub planes: WorkspaceArtifact,
    pub pixel_policy: RenderPixelPolicy,
}

impl RenderManifest {
    pub fn validate(&self) -> Result<(), String> {
        self.contract.validate()?;
        if self.planes.reference().as_str() != PICTURE_REF {
            return Err("render planes must be exactly output/pictures.i420".into());
        }
        if self.planes.byte_length() != self.contract.total_bytes()? {
            return Err("render planes length does not match the exact frame contract".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum RenderWorkerMessage {
    Progress {
        protocol: u32,
        identity: RenderIdentity,
        completed_frames: u64,
        total_frames: u64,
    },
    Completed {
        protocol: u32,
        identity: RenderIdentity,
        manifest: Box<RenderManifest>,
    },
    Failed {
        protocol: u32,
        identity: RenderIdentity,
        diagnostic: Diagnostic,
    },
    Cancelled {
        protocol: u32,
        identity: RenderIdentity,
    },
}

impl RenderWorkerMessage {
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
                ..
            } => {
                validate_version(*protocol)?;
                if *total_frames == 0
                    || *total_frames > MAX_PICTURE_FRAMES
                    || completed_frames > total_frames
                {
                    return Err("render progress is outside its bounded frame total".into());
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
            "unsupported render worker protocol version {protocol}"
        ));
    }
    Ok(())
}

/// Captures the attempt and its expected evidence for the process supervisor.
pub struct RenderProtocol {
    identity: RenderIdentity,
    cancellation_token: CancellationToken,
    contract: RenderContract,
    document_sha256: Sha256,
    maximum_output_bytes: u64,
}

impl WorkerProtocol for RenderProtocol {
    type Request = RenderHostMessage;
    type Response = RenderWorkerMessage;

    fn from_request(request: &Self::Request) -> Result<Self, SupervisorError> {
        request.validate().map_err(SupervisorError::Request)?;
        let RenderHostMessage::Prepare {
            identity,
            cancellation_token,
            contract,
            document_sha256,
            maximum_output_bytes,
            ..
        } = request
        else {
            return Err(SupervisorError::Request(
                "initial render message must prepare pictures".into(),
            ));
        };
        Ok(Self {
            identity: identity.clone(),
            cancellation_token: cancellation_token.clone(),
            contract: contract.as_ref().clone(),
            document_sha256: document_sha256.clone(),
            maximum_output_bytes: *maximum_output_bytes,
        })
    }

    fn cancellation(&self) -> Self::Request {
        RenderHostMessage::Cancel {
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
        let message: Option<RenderWorkerMessage> =
            read_frame(reader).map_err(|error| error.to_string())?;
        if let Some(message) = &message {
            message.validate()?;
        }
        Ok(message)
    }

    fn classify(&self, response: &Self::Response) -> Result<ResponseKind, String> {
        response.validate()?;
        if response.identity() != &self.identity {
            return Err("render response belongs to another request or attempt".into());
        }
        match response {
            RenderWorkerMessage::Progress { total_frames, .. } => {
                if *total_frames != self.contract.frame_count {
                    return Err("render progress does not match the captured frame count".into());
                }
                Ok(ResponseKind::Progress)
            }
            RenderWorkerMessage::Completed { manifest, .. } => {
                if manifest.contract != self.contract
                    || manifest.document_sha256 != self.document_sha256
                    || manifest.planes.byte_length() > self.maximum_output_bytes
                {
                    return Err("render completion does not match the captured request".into());
                }
                Ok(ResponseKind::Completed)
            }
            RenderWorkerMessage::Failed { .. } | RenderWorkerMessage::Cancelled { .. } => {
                Ok(ResponseKind::Terminal)
            }
        }
    }
}

pub fn read_host_message(reader: &mut impl Read) -> Result<Option<RenderHostMessage>, String> {
    let message: Option<RenderHostMessage> =
        read_frame(reader).map_err(|error| error.to_string())?;
    if let Some(message) = &message {
        message.validate()?;
    }
    Ok(message)
}

pub fn write_worker_message(
    writer: &mut impl Write,
    message: &RenderWorkerMessage,
) -> Result<(), String> {
    message.validate()?;
    write_frame(writer, message).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
