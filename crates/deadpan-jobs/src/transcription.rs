//! Versioned transcription worker messages over the shared process transport.
//!
//! The host prepares mono 16 kHz little-endian `f32` analysis PCM inside the
//! attempt workspace and selects a verified model file. The worker re-verifies
//! both before loading them, reports bounded progress, and writes raw recognizer
//! segments as one JSON artifact below the output scope. Large data travels only
//! by hashed artifact reference; a completed message is not a trusted transcript
//! until the host snapshots, parses and validates the artifact.
//!
//! The same worker also detects speech activity: `DetectSpeech` runs the
//! Silero voice activity detector over the same PCM and writes one
//! little-endian `f32` speech probability per [`VAD_HOP`] samples.

use std::io::{Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::process::{ResponseKind, SupervisorError, WorkerProtocol};
use crate::protocol::{
    AttemptId, CancellationToken, Diagnostic, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
    read_frame, write_frame,
};

pub const VERSION: u32 = 2;
/// Analysis PCM sample rate.
pub const SAMPLE_RATE: u32 = 16_000;
/// Three hours of mono analysis PCM.
pub const MAX_ANALYSIS_FRAMES: u64 = 3 * 60 * 60 * SAMPLE_RATE as u64;
pub const MAX_MODEL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MAX_TRANSCRIPT_BYTES: u64 = 64 * 1024 * 1024;
/// Analysis samples per speech probability.
pub const VAD_HOP: u64 = 512;
/// One `f32` probability per hop of the longest analysis audio.
pub const MAX_PROBABILITY_BYTES: u64 = MAX_ANALYSIS_FRAMES.div_ceil(VAD_HOP) * 4;
const MAX_PATH_BYTES: usize = 4_096;
const MAX_ENGINE_BYTES: usize = 64;
const MAX_TIMEOUT_MILLIS: u64 = 24 * 60 * 60 * 1_000;

/// A recognizer language: an ISO 639-1 code, or automatic detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Language {
    Automatic,
    Code(String),
}

impl Language {
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Automatic => None,
            Self::Code(code) => Some(code),
        }
    }
}

impl TryFrom<String> for Language {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value == "auto" {
            return Ok(Self::Automatic);
        }
        if value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_lowercase()) {
            return Ok(Self::Code(value));
        }
        Err("language must be auto or a two-letter lowercase code".into())
    }
}

impl From<Language> for String {
    fn from(value: Language) -> Self {
        match value {
            Language::Automatic => "auto".into(),
            Language::Code(code) => code,
        }
    }
}

/// A model file the host verified; the worker checks it again before loading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelInput {
    pub path: PathBuf,
    pub sha256: Sha256,
    pub byte_length: u64,
}

impl ModelInput {
    fn validate(&self) -> Result<(), String> {
        let text = self.path.to_str().ok_or("model path must be UTF-8")?;
        if !self.path.is_absolute() || text.len() > MAX_PATH_BYTES || text.contains('\0') {
            return Err("model path must be a bounded absolute path".into());
        }
        if self.byte_length == 0 || self.byte_length > MAX_MODEL_BYTES {
            return Err("model size outside its bound".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Metal,
    Cpu,
}

/// What actually ran, for provenance; never an authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReport {
    pub engine: String,
    pub backend: Backend,
    pub model_sha256: Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    Transcribe {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
        model: ModelInput,
        /// Mono 16 kHz little-endian f32 PCM below `input/`.
        audio: WorkspaceArtifact,
        language: Language,
        output_scope: WorkspaceRef,
        maximum_output_bytes: u64,
        timeout_millis: u64,
    },
    /// Run voice activity detection over the analysis PCM.
    DetectSpeech {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
        /// A Silero VAD model in ggml format.
        model: ModelInput,
        /// Mono 16 kHz little-endian f32 PCM below `input/`.
        audio: WorkspaceArtifact,
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

/// Bounds shared by both initial messages.
fn validate_job(
    model: &ModelInput,
    audio: &WorkspaceArtifact,
    output_scope: &WorkspaceRef,
    timeout_millis: u64,
) -> Result<(), String> {
    model.validate()?;
    let frames = audio.byte_length() / 4;
    if !audio.byte_length().is_multiple_of(4) || frames > MAX_ANALYSIS_FRAMES {
        return Err("analysis audio must be whole f32 frames within the bound".into());
    }
    if !audio.reference().as_str().starts_with("input/") {
        return Err("analysis audio must be a workspace input".into());
    }
    if output_scope.as_str() == "input" || output_scope.as_str().starts_with("input/") {
        return Err("output scope must be separate from inputs".into());
    }
    if !(1..=MAX_TIMEOUT_MILLIS).contains(&timeout_millis) {
        return Err("transcription timeout outside its bound".into());
    }
    Ok(())
}

impl HostMessage {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Transcribe {
                protocol,
                model,
                audio,
                output_scope,
                maximum_output_bytes,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                validate_job(model, audio, output_scope, *timeout_millis)?;
                if *maximum_output_bytes == 0 || *maximum_output_bytes > MAX_TRANSCRIPT_BYTES {
                    return Err("transcript byte budget outside its bound".into());
                }
                Ok(())
            }
            Self::DetectSpeech {
                protocol,
                model,
                audio,
                output_scope,
                maximum_output_bytes,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                validate_job(model, audio, output_scope, *timeout_millis)?;
                if *maximum_output_bytes == 0 || *maximum_output_bytes > MAX_PROBABILITY_BYTES {
                    return Err("probability byte budget outside its bound".into());
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
    Completed {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        transcript: WorkspaceArtifact,
        runtime: RuntimeReport,
        elapsed_millis: u64,
    },
    /// Speech probabilities as raw little-endian `f32`, one per [`VAD_HOP`].
    SpeechDetected {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        probabilities: WorkspaceArtifact,
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
    fn identity(&self) -> (&RequestId, &AttemptId) {
        match self {
            Self::Progress {
                request, attempt, ..
            }
            | Self::Completed {
                request, attempt, ..
            }
            | Self::SpeechDetected {
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
                    return Err("transcription progress exceeds 100 percent".into());
                }
                *protocol
            }
            Self::Completed {
                protocol, runtime, ..
            }
            | Self::SpeechDetected {
                protocol, runtime, ..
            } => {
                if runtime.engine.is_empty()
                    || runtime.engine.len() > MAX_ENGINE_BYTES
                    || runtime.engine.chars().any(char::is_control)
                {
                    return Err("runtime engine label is invalid".into());
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
        Err("unsupported transcription protocol".into())
    }
}

/// Which initial operation an attempt runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Transcribe,
    DetectSpeech,
}

/// Host-side adapter binding every response to the captured attempt.
pub struct TranscriptionProtocol {
    operation: Operation,
    request: RequestId,
    attempt: AttemptId,
    token: CancellationToken,
    model_sha256: Sha256,
    output_scope: WorkspaceRef,
    maximum_output_bytes: u64,
}

impl WorkerProtocol for TranscriptionProtocol {
    type Request = HostMessage;
    type Response = WorkerMessage;

    fn from_request(request: &HostMessage) -> Result<Self, SupervisorError> {
        request.validate().map_err(SupervisorError::Request)?;
        let (
            operation,
            request,
            attempt,
            cancellation_token,
            model,
            output_scope,
            maximum_output_bytes,
        ) = match request {
            HostMessage::Transcribe {
                request,
                attempt,
                cancellation_token,
                model,
                output_scope,
                maximum_output_bytes,
                ..
            } => (
                Operation::Transcribe,
                request,
                attempt,
                cancellation_token,
                model,
                output_scope,
                maximum_output_bytes,
            ),
            HostMessage::DetectSpeech {
                request,
                attempt,
                cancellation_token,
                model,
                output_scope,
                maximum_output_bytes,
                ..
            } => (
                Operation::DetectSpeech,
                request,
                attempt,
                cancellation_token,
                model,
                output_scope,
                maximum_output_bytes,
            ),
            HostMessage::Cancel { .. } => {
                return Err(SupervisorError::Request(
                    "initial transcription message must transcribe or detect speech".into(),
                ));
            }
        };
        Ok(Self {
            operation,
            request: request.clone(),
            attempt: attempt.clone(),
            token: cancellation_token.clone(),
            model_sha256: model.sha256.clone(),
            output_scope: output_scope.clone(),
            maximum_output_bytes: *maximum_output_bytes,
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
            return Err("transcription response belongs to another attempt".into());
        }
        match response {
            WorkerMessage::Progress { .. } => Ok(ResponseKind::Progress),
            WorkerMessage::Completed {
                transcript,
                runtime,
                ..
            } => {
                if self.operation != Operation::Transcribe {
                    return Err("worker returned a transcript for speech detection".into());
                }
                self.admit(transcript, runtime)?;
                Ok(ResponseKind::Completed)
            }
            WorkerMessage::SpeechDetected {
                probabilities,
                runtime,
                ..
            } => {
                if self.operation != Operation::DetectSpeech {
                    return Err("worker returned speech probabilities for transcription".into());
                }
                if !probabilities.byte_length().is_multiple_of(4) {
                    return Err("speech probabilities must be whole f32 values".into());
                }
                self.admit(probabilities, runtime)?;
                Ok(ResponseKind::Completed)
            }
            WorkerMessage::Failed { .. } => Ok(ResponseKind::Failed),
            WorkerMessage::Cancelled { .. } => Ok(ResponseKind::Terminal),
        }
    }
}

impl TranscriptionProtocol {
    /// The operation this attempt runs.
    pub fn operation(&self) -> Operation {
        self.operation
    }

    fn admit(&self, artifact: &WorkspaceArtifact, runtime: &RuntimeReport) -> Result<(), String> {
        let scope = format!("{}/", self.output_scope.as_str());
        if !artifact.reference().as_str().starts_with(&scope) {
            return Err("worker artifact is outside the output scope".into());
        }
        if artifact.byte_length() > self.maximum_output_bytes {
            return Err("worker artifact exceeds its byte budget".into());
        }
        if runtime.model_sha256 != self.model_sha256 {
            return Err("worker ran a different model".into());
        }
        Ok(())
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
