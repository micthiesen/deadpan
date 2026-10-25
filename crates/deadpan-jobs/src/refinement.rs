//! Bounded native phoneme-emission inference over the shared worker transport.
//! The host owns captured PCM and text alignment; this worker sees no transcript
//! and cannot mutate annotations. Completed bytes remain untrusted until the
//! host snapshots and validates the exact float32 tensor artifact.

pub use crate::transcription::ModelInput;
use crate::{
    process::{ResponseKind, SupervisorError, WorkerProtocol},
    protocol::{
        AttemptId, CancellationToken, Diagnostic, RequestId, Sha256, WorkspaceArtifact,
        WorkspaceRef, read_frame, write_frame,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Component,
};

pub const VERSION: u32 = 1;
pub const ENGINE_ID: &str = "deadpan-phoneme-alpha06-ort-cpu-1";
pub const ORT_VERSION: &str = "1.31.0";
pub const CHECKPOINT_BYTES: u64 = 57_562_900;
pub const CHECKPOINT_SHA256: &str =
    "4070c3b89fb27129e2999f550ddb55c747ec77da843fa8a520989b8c5d09723f";
pub const STATISTICS_BYTES: u64 = 3_905;
pub const STATISTICS_SHA256: &str =
    "be2f80418ef66bad30dcd5556e194f95234d75c5d2b94a92d7a766d31900df87";
pub const GRAPH_SHA256: &str = "fe1e1890f968e7c19c0d4dde3ac7e15dfca15fd3bf2f9d5c41b2a5d1f120c57d";
pub const SAMPLE_RATE: u32 = 16_000;
pub const MIN_SAMPLES: u64 = 401;
pub const MAX_SAMPLES: u64 = 480_000;
pub const LABELS: u32 = 41;
pub const MAX_OUTPUT_FRAMES: u64 = 1501;
pub const MAX_OUTPUT_BYTES: u64 = MAX_OUTPUT_FRAMES * LABELS as u64 * 4;
pub const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000;
pub const OUTPUT_FILE: &str = "emissions.f32le";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    Infer {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
        checkpoint: Box<ModelInput>,
        statistics: Box<ModelInput>,
        /// Raw mono 16 kHz little-endian float32, directly below input/.
        audio: WorkspaceArtifact,
        output_scope: WorkspaceRef,
        max_output_bytes: u64,
        timeout_ms: u64,
    },
    Cancel {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
    },
}

fn validate_model(model: &ModelInput, bytes: u64, hash: &str) -> Result<(), String> {
    let text = model.path.to_str().ok_or("model path must be UTF-8")?;
    if !model.path.is_absolute()
        || text.len() > 4096
        || text.contains('\0')
        || model
            .path
            .components()
            .any(|v| matches!(v, Component::ParentDir | Component::CurDir))
    {
        return Err("model path must be a bounded absolute path without dot components".into());
    }
    if model.byte_length != bytes || model.sha256.as_str() != hash {
        return Err("model input differs from the compiled pin".into());
    }
    Ok(())
}

pub fn output_frames(samples: u64) -> Result<u32, String> {
    if !(MIN_SAMPLES..=MAX_SAMPLES).contains(&samples) {
        return Err("phoneme context must contain 401..480000 samples".into());
    }
    u32::try_from((samples / 160 + 1).div_ceil(2)).map_err(|_| "output frame overflow".into())
}

impl HostMessage {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Infer {
                protocol,
                checkpoint,
                statistics,
                audio,
                output_scope,
                max_output_bytes,
                timeout_ms,
                ..
            } => {
                version(*protocol)?;
                validate_model(checkpoint, CHECKPOINT_BYTES, CHECKPOINT_SHA256)?;
                validate_model(statistics, STATISTICS_BYTES, STATISTICS_SHA256)?;
                if !audio.byte_length().is_multiple_of(4) {
                    return Err("phoneme PCM must contain whole float32 samples".into());
                }
                let expected =
                    u64::from(output_frames(audio.byte_length() / 4)?) * u64::from(LABELS) * 4;
                let reference = std::path::Path::new(audio.reference().as_str());
                if reference.parent() != Some(std::path::Path::new("input")) {
                    return Err("phoneme PCM must be directly below input/".into());
                }
                if output_scope.as_str() == "input" || output_scope.as_str().starts_with("input/") {
                    return Err("phoneme output scope overlaps inputs".into());
                }
                if *max_output_bytes < expected || *max_output_bytes > MAX_OUTPUT_BYTES {
                    return Err("phoneme output budget outside bounds".into());
                }
                if !(1..=MAX_TIMEOUT_MS).contains(timeout_ms) {
                    return Err("phoneme timeout outside bounds".into());
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => version(*protocol),
        }
    }

    /// Only a cancel carrying all three captured identities can stop an attempt.
    pub fn cancels(
        &self,
        request: &RequestId,
        attempt: &AttemptId,
        token: &CancellationToken,
    ) -> bool {
        matches!(self, Self::Cancel { request: r, attempt: a, cancellation_token: t, .. }
            if r == request && a == attempt && t == token)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Cpu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReport {
    pub engine: String,
    pub ort_version: String,
    pub backend: Backend,
    pub checkpoint_sha256: Sha256,
    pub statistics_sha256: Sha256,
    pub graph_sha256: Sha256,
}

impl RuntimeReport {
    pub fn validate(&self) -> Result<(), String> {
        if self.engine != ENGINE_ID
            || self.ort_version != ORT_VERSION
            || self.checkpoint_sha256.as_str() != CHECKPOINT_SHA256
            || self.statistics_sha256.as_str() != STATISTICS_SHA256
            || self.graph_sha256.as_str() != GRAPH_SHA256
        {
            return Err("phoneme runtime differs from compiled pins".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Completed {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        emissions: WorkspaceArtifact,
        input_samples: u32,
        frames: u32,
        runtime: RuntimeReport,
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
    pub fn validate(&self) -> Result<(), String> {
        let protocol = match self {
            Self::Completed {
                protocol,
                input_samples,
                frames,
                emissions,
                runtime,
                ..
            } => {
                runtime.validate()?;
                if *frames != output_frames(u64::from(*input_samples))?
                    || emissions.byte_length() != u64::from(*frames) * u64::from(LABELS) * 4
                {
                    return Err("phoneme output shape or byte count mismatch".into());
                }
                *protocol
            }
            Self::Failed { protocol, .. } | Self::Cancelled { protocol, .. } => *protocol,
        };
        version(protocol)
    }

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
}

fn version(value: u32) -> Result<(), String> {
    if value == VERSION {
        Ok(())
    } else {
        Err("unsupported phoneme worker protocol".into())
    }
}

/// Admission for one captured context and immutable runtime pins.
pub struct RefinementProtocol {
    request: RequestId,
    attempt: AttemptId,
    token: CancellationToken,
    input_samples: u32,
    output_scope: WorkspaceRef,
    max_output_bytes: u64,
}

impl WorkerProtocol for RefinementProtocol {
    const WORKER_CLASS: deadpan_diagnostics::WorkerClass = deadpan_diagnostics::WorkerClass::Model;
    type Request = HostMessage;
    type Response = WorkerMessage;

    fn from_request(message: &HostMessage) -> Result<Self, SupervisorError> {
        message.validate().map_err(SupervisorError::Request)?;
        let HostMessage::Infer {
            request,
            attempt,
            cancellation_token,
            audio,
            output_scope,
            max_output_bytes,
            ..
        } = message
        else {
            return Err(SupervisorError::Request(
                "initial phoneme message must infer".into(),
            ));
        };
        Ok(Self {
            request: request.clone(),
            attempt: attempt.clone(),
            token: cancellation_token.clone(),
            input_samples: u32::try_from(audio.byte_length() / 4)
                .map_err(|_| SupervisorError::Request("sample count overflow".into()))?,
            output_scope: output_scope.clone(),
            max_output_bytes: *max_output_bytes,
        })
    }
    fn cancellation(&self) -> HostMessage {
        HostMessage::Cancel {
            protocol: VERSION,
            request: self.request.clone(),
            attempt: self.attempt.clone(),
            cancellation_token: self.token.clone(),
        }
    }
    fn write_request(writer: &mut impl Write, message: &HostMessage) -> Result<(), String> {
        message.validate()?;
        write_frame(writer, message).map_err(|e| e.to_string())
    }
    fn read_response(reader: &mut impl Read) -> Result<Option<WorkerMessage>, String> {
        let message: Option<WorkerMessage> = read_frame(reader).map_err(|e| e.to_string())?;
        if let Some(message) = &message {
            message.validate()?;
        }
        Ok(message)
    }
    fn classify(&self, message: &WorkerMessage) -> Result<ResponseKind, String> {
        message.validate()?;
        if message.identity() != (&self.request, &self.attempt) {
            return Err("phoneme response belongs to another attempt".into());
        }
        match message {
            WorkerMessage::Completed {
                emissions,
                input_samples,
                ..
            } => {
                if *input_samples != self.input_samples
                    || emissions.reference().as_str()
                        != format!("{}/{OUTPUT_FILE}", self.output_scope.as_str())
                    || emissions.byte_length() > self.max_output_bytes
                {
                    return Err("phoneme output differs from captured input or scope".into());
                }
                Ok(ResponseKind::Completed)
            }
            WorkerMessage::Failed { .. } => Ok(ResponseKind::Failed),
            WorkerMessage::Cancelled { .. } => Ok(ResponseKind::Terminal),
        }
    }
}

pub fn read_host(reader: &mut impl Read) -> Result<Option<HostMessage>, String> {
    let message: Option<HostMessage> = read_frame(reader).map_err(|e| e.to_string())?;
    if let Some(message) = &message {
        message.validate()?;
    }
    Ok(message)
}

pub fn write_worker(writer: &mut impl Write, message: &WorkerMessage) -> Result<(), String> {
    message.validate()?;
    write_frame(writer, message).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
