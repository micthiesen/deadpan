//! Versioned face-detection worker messages over the shared process transport.
//!
//! A sibling mode of the `deadpan-track` worker (launched with the single
//! argument [`WORKER_ARGUMENT`]). The host copies the verified Original's bytes
//! into the attempt workspace below `input/`, names them by hash and length
//! with the qualified picture stream it expects and the exact PTS of one
//! indexed picture. The worker re-verifies the bytes, decodes exactly that
//! picture through the pinned descriptor-only decoder, runs Apple Vision's face
//! rectangle detector on it and reports at most [`MAX_FACES`] rectangles in the
//! displayed picture inline. Faces are proposals: the host validates them
//! strictly and nothing edits a project unless a person uses one.

use std::io::{Read, Write};

use deadpan_analysis::NormalizedRect;
use serde::{Deserialize, Serialize};

use crate::process::{ResponseKind, SupervisorError, WorkerProtocol};
use crate::protocol::{
    AttemptId, CancellationToken, Diagnostic, RequestId, WorkspaceArtifact, read_frame, write_frame,
};
pub use crate::tracking::ExpectedStream;

pub const VERSION: u32 = 1;
/// The worker's command-line argument selecting this protocol.
pub const WORKER_ARGUMENT: &str = "detect-faces";
/// The most faces one picture may report. More are refused, not truncated.
pub const MAX_FACES: usize = 64;
const MAX_ENGINE_BYTES: usize = 96;
const MAX_TIMEOUT_MILLIS: u64 = 60 * 60 * 1_000;

/// One detected face in the displayed picture (top-left origin), with
/// Vision's confidence in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectedFace {
    pub region: NormalizedRect,
    pub confidence: f32,
}

impl DetectedFace {
    /// Left edge first, then top edge, then size: a total order on valid
    /// regions, so equal inputs always number faces the same way.
    pub fn order(&self, other: &Self) -> std::cmp::Ordering {
        let key = |face: &Self| {
            [
                face.region.x(),
                face.region.y(),
                face.region.width(),
                face.region.height(),
            ]
        };
        let (a, b) = (key(self), key(other));
        a.iter()
            .zip(b.iter())
            .map(|(a, b)| a.total_cmp(b))
            .find(|ordering| ordering.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| self.confidence.total_cmp(&other.confidence))
    }
}

/// Validate a reported face list: bounded, finite confidences in `[0, 1]`
/// and strictly ordered left to right (then top to bottom).
pub fn validate_faces(faces: &[DetectedFace]) -> Result<(), String> {
    if faces.len() > MAX_FACES {
        return Err(format!("more than {MAX_FACES} faces reported"));
    }
    if faces
        .iter()
        .any(|face| !face.confidence.is_finite() || !(0.0..=1.0).contains(&face.confidence))
    {
        return Err("face confidence outside [0, 1]".into());
    }
    if faces
        .windows(2)
        .any(|pair| pair[0].order(&pair[1]) != std::cmp::Ordering::Less)
    {
        return Err("faces are not ordered left to right, then top to bottom".into());
    }
    Ok(())
}

/// Which detector revision actually ran, for the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReport {
    pub engine: String,
    pub request_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    DetectFaces {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
        /// The Original's verified bytes, directly below `input/`.
        source: WorkspaceArtifact,
        stream: ExpectedStream,
        /// The exact PTS of the indexed picture to analyse.
        pts: i64,
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
            Self::DetectFaces {
                protocol,
                source,
                stream,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                stream.validate()?;
                let reference = source.reference().as_str();
                if !reference.starts_with("input/") || reference[6..].contains('/') {
                    return Err("detection source must be directly below input/".into());
                }
                if source.byte_length() == 0
                    || source.byte_length() > crate::tracking::MAX_SOURCE_BYTES
                {
                    return Err("detection source size outside its bound".into());
                }
                if !(1..=MAX_TIMEOUT_MILLIS).contains(timeout_millis) {
                    return Err("detection timeout outside its bound".into());
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => version(*protocol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Completed {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        /// The analysed picture's PTS, which must be the requested one.
        pts: i64,
        faces: Vec<DetectedFace>,
        runtime: RuntimeReport,
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
            Self::Completed {
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
            Self::Completed {
                protocol,
                faces,
                runtime,
                ..
            } => {
                let engine = &runtime.engine;
                if engine.is_empty()
                    || engine.len() > MAX_ENGINE_BYTES
                    || engine.chars().any(char::is_control)
                {
                    return Err("runtime label is invalid".into());
                }
                validate_faces(faces)?;
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
        Err("unsupported face-detection protocol".into())
    }
}

/// Host-side adapter binding every response to the captured attempt.
pub struct FaceProtocol {
    request: RequestId,
    attempt: AttemptId,
    token: CancellationToken,
    pts: i64,
}

impl WorkerProtocol for FaceProtocol {
    const WORKER_CLASS: deadpan_diagnostics::WorkerClass = deadpan_diagnostics::WorkerClass::Model;

    type Request = HostMessage;
    type Response = WorkerMessage;

    fn from_request(request: &HostMessage) -> Result<Self, SupervisorError> {
        request.validate().map_err(SupervisorError::Request)?;
        match request {
            HostMessage::DetectFaces {
                request,
                attempt,
                cancellation_token,
                pts,
                ..
            } => Ok(Self {
                request: request.clone(),
                attempt: attempt.clone(),
                token: cancellation_token.clone(),
                pts: *pts,
            }),
            HostMessage::Cancel { .. } => Err(SupervisorError::Request(
                "initial detection message must detect".into(),
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
            return Err("detection response belongs to another attempt".into());
        }
        match response {
            WorkerMessage::Completed { pts, .. } => {
                if *pts != self.pts {
                    return Err("worker analysed a different picture".into());
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
