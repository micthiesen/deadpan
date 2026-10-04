//! Versioned target-tracking worker messages over the shared process transport.
//!
//! The host copies the verified Original's bytes into the attempt workspace
//! below `input/` and names them by hash and length, with the qualified picture
//! stream it expects, an exact PTS range of indexed pictures, the selected
//! region in the displayed picture and a stride. The worker re-verifies the
//! bytes, decodes them through the pinned descriptor-only decoder, runs Apple
//! Vision's object tracker on every `stride`-th picture of the range, and
//! writes every decoded PTS and one raw observation per analysed picture as a
//! JSON artifact below the output scope. Observations are untrusted until the host snapshots, parses
//! and validates them against its own index and applies the tracking policy.

use std::io::{Read, Write};

use deadpan_analysis::{MAX_TRACK_PICTURES, MAX_TRACK_STRIDE, NormalizedRect};
use serde::{Deserialize, Serialize};

use crate::process::{ResponseKind, SupervisorError, WorkerProtocol};
use crate::protocol::{
    AttemptId, CancellationToken, Diagnostic, RequestId, WorkspaceArtifact, WorkspaceRef,
    read_frame, write_frame,
};

pub const VERSION: u32 = 1;
/// The largest Original the worker will hash and decode (the decoder's bound).
pub const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024 * 1024;
/// Bytes allowed per decoded PTS and per observation in the JSON artifact.
pub const DECODED_PTS_BYTES: u64 = 24;
pub const OBSERVATION_BYTES: u64 = 256;
/// The artifact bound at the picture limit.
pub const MAX_OBSERVATION_BYTES: u64 =
    MAX_TRACK_PICTURES as u64 * (DECODED_PTS_BYTES + OBSERVATION_BYTES);
const MAX_ENGINE_BYTES: usize = 96;
const MAX_TIMEOUT_MILLIS: u64 = 24 * 60 * 60 * 1_000;
const MAX_DIMENSION: u32 = 8_192;

/// The qualified picture stream the worker must find, in coded orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedStream {
    pub stream_index: u32,
    pub width: u32,
    pub height: u32,
    pub time_base_num: u32,
    pub time_base_den: u32,
    /// Clockwise quarter turns from coded to displayed orientation.
    pub rotation_quarter_turns: u8,
}

impl ExpectedStream {
    fn validate(&self) -> Result<(), String> {
        if !(1..=MAX_DIMENSION).contains(&self.width)
            || !(1..=MAX_DIMENSION).contains(&self.height)
            || self.time_base_num == 0
            || self.time_base_den == 0
            || self.rotation_quarter_turns > 3
        {
            return Err("expected picture stream is outside its bounds".into());
        }
        Ok(())
    }
}

/// Which tracker revision and level actually ran, for provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReport {
    pub engine: String,
    pub request_revision: u64,
    pub tracking_level: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    Track {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
        /// The Original's verified bytes, directly below `input/`.
        source: WorkspaceArtifact,
        stream: ExpectedStream,
        /// PTS of the first picture: the one the region was selected on.
        start_pts: i64,
        /// Exclusive PTS end of the range.
        end_pts: i64,
        /// Indexed pictures in `[start_pts, end_pts)`; the worker must decode
        /// exactly this many.
        pictures: u32,
        /// Analyse every `stride`-th picture, starting with the first.
        stride: u32,
        /// The selected region in the displayed picture, top-left origin.
        region: NormalizedRect,
        output_scope: WorkspaceRef,
        maximum_output_bytes: u64,
        timeout_millis: u64,
    },
    Cancel {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
    },
}

impl HostMessage {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Track {
                protocol,
                source,
                stream,
                start_pts,
                end_pts,
                pictures,
                stride,
                output_scope,
                maximum_output_bytes,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                stream.validate()?;
                let reference = source.reference().as_str();
                if !reference.starts_with("input/") || reference[6..].contains('/') {
                    return Err("tracking source must be directly below input/".into());
                }
                if source.byte_length() == 0 || source.byte_length() > MAX_SOURCE_BYTES {
                    return Err("tracking source size outside its bound".into());
                }
                if start_pts >= end_pts {
                    return Err("tracking range is empty".into());
                }
                if *pictures == 0 || *pictures as usize > MAX_TRACK_PICTURES {
                    return Err("tracking picture count outside its bound".into());
                }
                if !(1..=MAX_TRACK_STRIDE).contains(stride) {
                    return Err("tracking stride outside its bound".into());
                }
                if output_scope.as_str() == "input" || output_scope.as_str().starts_with("input/") {
                    return Err("output scope must be separate from inputs".into());
                }
                if *maximum_output_bytes == 0 || *maximum_output_bytes > MAX_OBSERVATION_BYTES {
                    return Err("observation byte budget outside its bound".into());
                }
                if !(1..=MAX_TIMEOUT_MILLIS).contains(timeout_millis) {
                    return Err("tracking timeout outside its bound".into());
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => version(*protocol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Progress {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        percent: u8,
    },
    /// A JSON `deadpan_analysis::RawTrack`: every decoded range PTS in order
    /// and one raw observation per analysed picture.
    Completed {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        observations: WorkspaceArtifact,
        runtime: RuntimeReport,
        /// Pictures decoded in the range and pictures given to the tracker.
        decoded: u32,
        analysed: u32,
        /// Time spent verifying, decoding and seeking, and in Vision.
        decode_millis: u64,
        vision_millis: u64,
        elapsed_millis: u64,
    },
    Failed {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        diagnostic: Diagnostic,
    },
    Cancelled {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
    },
}

impl WorkerMessage {
    fn identity(&self) -> (&RequestId, &AttemptId) {
        match self {
            Self::Progress {
                request, attempt, ..
            }
            | Self::Completed {
                request, attempt, ..
            }
            | Self::Failed {
                request, attempt, ..
            }
            | Self::Cancelled {
                request, attempt, ..
            } => (request, attempt),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let protocol = match self {
            Self::Progress {
                protocol, percent, ..
            } => {
                if *percent > 100 {
                    return Err("tracking progress exceeds 100 percent".into());
                }
                *protocol
            }
            Self::Completed {
                protocol,
                runtime,
                decoded,
                analysed,
                ..
            } => {
                for label in [&runtime.engine, &runtime.tracking_level] {
                    if label.is_empty()
                        || label.len() > MAX_ENGINE_BYTES
                        || label.chars().any(char::is_control)
                    {
                        return Err("runtime label is invalid".into());
                    }
                }
                if analysed > decoded || *decoded as usize > MAX_TRACK_PICTURES {
                    return Err("tracking counts are inconsistent".into());
                }
                *protocol
            }
            Self::Failed { protocol, .. } | Self::Cancelled { protocol, .. } => *protocol,
        };
        version(protocol)
    }
}

fn version(value: u32) -> Result<(), String> {
    if value == VERSION {
        Ok(())
    } else {
        Err("unsupported tracking protocol".into())
    }
}

/// Host-side adapter binding every response to the captured attempt.
pub struct TrackingProtocol {
    request: RequestId,
    attempt: AttemptId,
    token: CancellationToken,
    output_scope: WorkspaceRef,
    maximum_output_bytes: u64,
    pictures: u32,
    stride: u32,
}

impl WorkerProtocol for TrackingProtocol {
    type Request = HostMessage;
    type Response = WorkerMessage;

    fn from_request(request: &HostMessage) -> Result<Self, SupervisorError> {
        request.validate().map_err(SupervisorError::Request)?;
        match request {
            HostMessage::Track {
                request,
                attempt,
                cancellation_token,
                output_scope,
                maximum_output_bytes,
                pictures,
                stride,
                ..
            } => Ok(Self {
                request: request.clone(),
                attempt: attempt.clone(),
                token: cancellation_token.clone(),
                output_scope: output_scope.clone(),
                maximum_output_bytes: *maximum_output_bytes,
                pictures: *pictures,
                stride: *stride,
            }),
            HostMessage::Cancel { .. } => Err(SupervisorError::Request(
                "initial tracking message must track".into(),
            )),
        }
    }

    fn cancellation(&self) -> HostMessage {
        HostMessage::Cancel {
            protocol: VERSION,
            request: self.request.clone(),
            attempt: self.attempt.clone(),
            cancellation_token: self.token.clone(),
        }
    }

    fn write_request(writer: &mut impl Write, request: &HostMessage) -> Result<(), String> {
        request.validate()?;
        write_frame(writer, request).map_err(|error| error.to_string())
    }

    fn read_response(reader: &mut impl Read) -> Result<Option<WorkerMessage>, String> {
        let value: Option<WorkerMessage> = read_frame(reader).map_err(|error| error.to_string())?;
        if let Some(value) = &value {
            value.validate()?;
        }
        Ok(value)
    }

    fn classify(&self, response: &WorkerMessage) -> Result<ResponseKind, String> {
        response.validate()?;
        if response.identity() != (&self.request, &self.attempt) {
            return Err("tracking response belongs to another attempt".into());
        }
        match response {
            WorkerMessage::Progress { .. } => Ok(ResponseKind::Progress),
            WorkerMessage::Completed {
                observations,
                decoded,
                analysed,
                ..
            } => {
                let scope = format!("{}/", self.output_scope.as_str());
                if !observations.reference().as_str().starts_with(&scope) {
                    return Err("worker artifact is outside the output scope".into());
                }
                if observations.byte_length() > self.maximum_output_bytes {
                    return Err("worker artifact exceeds its byte budget".into());
                }
                if *decoded != self.pictures || *analysed != self.pictures.div_ceil(self.stride) {
                    return Err("worker did not analyse the requested pictures".into());
                }
                Ok(ResponseKind::Completed)
            }
            WorkerMessage::Failed { .. } => Ok(ResponseKind::Failed),
            WorkerMessage::Cancelled { .. } => Ok(ResponseKind::Terminal),
        }
    }
}

/// Worker side: read the next host message, validated.
pub fn read_host(reader: &mut impl Read) -> Result<Option<HostMessage>, String> {
    let value: Option<HostMessage> = read_frame(reader).map_err(|error| error.to_string())?;
    if let Some(value) = &value {
        value.validate()?;
    }
    Ok(value)
}

/// Worker side: write one validated message.
pub fn write_worker(writer: &mut impl Write, value: &WorkerMessage) -> Result<(), String> {
    value.validate()?;
    write_frame(writer, value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
