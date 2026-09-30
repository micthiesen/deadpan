//! Durable render declarations. Stored outcomes are historical evidence, never
//! a decoded-media capability or permission to publish a movie.

pub mod admission;
pub mod publication;

use crate::{AttemptId, CancellationToken, RequestId, Sha256};
use deadpan_core::{FrameRange, ProjectDocument, ProjectId, RevisionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use thiserror::Error;

pub const MAX_RENDER_REPORT_BYTES: usize = 256 * 1024;
pub const MAX_RENDER_DIAGNOSTIC_BYTES: usize = 4096;
pub const MAX_RENDER_COUNTER: u64 = i64::MAX as u64;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("invalid render metadata: {0}")]
    Invalid(&'static str),
    #[error("render document hashing cancelled")]
    Cancelled,
    #[error("render document hashing deadline exceeded")]
    Deadline,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn version<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let value = u32::deserialize(deserializer)?;
    if value != 1 {
        return Err(serde::de::Error::custom(
            "unsupported render schema version",
        ));
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderEncoder {
    Hardware,
    Software,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderBFrames {
    None,
    TargetTwo,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderSelection {
    ExplicitEngineering,
}

/// Engineering SDR policy, not the future automatic product selection policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderEngineeringPolicy {
    #[serde(deserialize_with = "version")]
    pub schema_version: u32,
    pub selection: RenderSelection,
    pub encoder: RenderEncoder,
    pub b_frames: RenderBFrames,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderAutomaticAlgorithm {
    AutomaticSdrV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderAutomaticSelection {
    Automatic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderAutomaticPolicy {
    #[serde(deserialize_with = "version")]
    pub schema_version: u32,
    pub selection: RenderAutomaticSelection,
    pub algorithm: RenderAutomaticAlgorithm,
}

/// Untagged serialization preserves the original engineering policy bytes.
/// Each alternative has a closed grammar; intent versions select exactly one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RenderPolicy {
    Engineering(RenderEngineeringPolicy),
    Automatic(RenderAutomaticPolicy),
}

impl From<RenderEngineeringPolicy> for RenderPolicy {
    fn from(value: RenderEngineeringPolicy) -> Self {
        Self::Engineering(value)
    }
}

impl From<RenderAutomaticPolicy> for RenderPolicy {
    fn from(value: RenderAutomaticPolicy) -> Self {
        Self::Automatic(value)
    }
}

impl RenderPolicy {
    pub const fn engineering(&self) -> Option<&RenderEngineeringPolicy> {
        match self {
            Self::Engineering(value) => Some(value),
            Self::Automatic(_) => None,
        }
    }

    pub const fn automatic(&self) -> Option<&RenderAutomaticPolicy> {
        match self {
            Self::Automatic(value) => Some(value),
            Self::Engineering(_) => None,
        }
    }

    pub const fn is_automatic(&self) -> bool {
        matches!(self, Self::Automatic(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RenderIntentWire")]
pub struct RenderIntent {
    pub schema_version: u32,
    pub job_id: RequestId,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub document_sha256: Sha256,
    pub range: FrameRange,
    pub policy: RenderPolicy,
}
impl RenderIntent {
    pub fn validate(&self) -> Result<(), RenderError> {
        let version_matches = match &self.policy {
            RenderPolicy::Engineering(policy) => {
                self.schema_version == 1 && policy.schema_version == 1
            }
            RenderPolicy::Automatic(policy) => {
                self.schema_version == 2 && policy.schema_version == 1
            }
        };
        if !version_matches {
            return Err(RenderError::Invalid("unsupported intent/policy version"));
        }
        if self.range.start().0 < 0 || self.range.start() == self.range.end() {
            return Err(RenderError::Invalid("empty or negative render range"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderIntentWire {
    schema_version: u32,
    job_id: RequestId,
    project_id: ProjectId,
    revision_id: RevisionId,
    document_sha256: Sha256,
    range: FrameRange,
    policy: RenderPolicy,
}

impl TryFrom<RenderIntentWire> for RenderIntent {
    type Error = RenderError;

    fn try_from(value: RenderIntentWire) -> Result<Self, Self::Error> {
        let result = Self {
            schema_version: value.schema_version,
            job_id: value.job_id,
            project_id: value.project_id,
            revision_id: value.revision_id,
            document_sha256: value.document_sha256,
            range: value.range,
            policy: value.policy,
        };
        result.validate()?;
        Ok(result)
    }
}

/// Frozen grammar for database <=41 and retained manifest version 1. This
/// parser cannot acquire future policy variants through the current sum type.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderIntentV1 {
    #[serde(deserialize_with = "version")]
    schema_version: u32,
    job_id: RequestId,
    project_id: ProjectId,
    revision_id: RevisionId,
    document_sha256: Sha256,
    range: FrameRange,
    policy: RenderEngineeringPolicy,
}

impl RenderIntentV1 {
    fn into_current(self) -> Result<RenderIntent, RenderError> {
        let result = RenderIntent {
            schema_version: self.schema_version,
            job_id: self.job_id,
            project_id: self.project_id,
            revision_id: self.revision_id,
            document_sha256: self.document_sha256,
            range: self.range,
            policy: RenderPolicy::Engineering(self.policy),
        };
        result.validate()?;
        Ok(result)
    }
}

pub fn deserialize_render_intent_v1<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<RenderIntent, D::Error> {
    RenderIntentV1::deserialize(deserializer)?
        .into_current()
        .map_err(serde::de::Error::custom)
}

pub fn parse_render_intent_v1(bytes: &[u8]) -> Result<RenderIntent, RenderError> {
    serde_json::from_slice::<RenderIntentV1>(bytes)?.into_current()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderAttemptIdentity {
    pub job_id: RequestId,
    pub attempt_id: AttemptId,
    pub cancellation_token: CancellationToken,
    pub expected_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderAttemptState {
    Queued,
    Encoding,
    EncodedRetained,
    Verifying,
    Verified,
    Cancelling,
    Cancelled,
    Failed,
    Interrupted,
}
impl RenderAttemptState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Verified | Self::Cancelled | Self::Failed | Self::Interrupted
        )
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Encoding => "encoding",
            Self::EncodedRetained => "encoded_retained",
            Self::Verifying => "verifying",
            Self::Verified => "verified",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

/// Validate one observed phase change. Cancellation completion is a separate
/// host action after owned worker/verifier teardown, never an acknowledgement.
pub fn validate_transition(
    from: RenderAttemptState,
    to: RenderAttemptState,
    has_checkpoint: bool,
) -> Result<(), RenderError> {
    use RenderAttemptState::*;
    let allowed = match (from, to) {
        (Queued, Encoding) => !has_checkpoint,
        (Encoding, EncodedRetained) | (EncodedRetained, Verifying) | (Verifying, Verified) => {
            has_checkpoint
        }
        (Queued, Verifying) => has_checkpoint,
        (Cancelling, Cancelled) => true,
        (Cancelling, Interrupted | Failed) => true,
        (_, Cancelling | Failed | Interrupted) => !from.is_terminal() && from != Cancelling,
        _ => false,
    };
    if !allowed {
        return Err(RenderError::Invalid("invalid render attempt transition"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderDiagnostic {
    pub code: String,
    pub detail: String,
}
impl RenderDiagnostic {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.code.is_empty()
            || self.code.len() > 128
            || self.detail.is_empty()
            || self.detail.len() > MAX_RENDER_DIAGNOSTIC_BYTES
            || self.code.chars().any(char::is_control)
            || self.detail.contains('\0')
        {
            return Err(RenderError::Invalid("invalid render diagnostic"));
        }
        Ok(())
    }
}

/// An opaque bounded report from a past verifier invocation. Deserializing this
/// does not recreate a live VerifiedCandidate or skip future byte admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderVerificationObservation {
    #[serde(deserialize_with = "version")]
    pub schema_version: u32,
    pub validator_id: String,
    pub validator_version: String,
    pub movie_sha256: Sha256,
    pub movie_byte_length: u64,
    pub report: serde_json::Value,
}
impl RenderVerificationObservation {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.schema_version != 1
            || self.movie_byte_length == 0
            || self.movie_byte_length > MAX_RENDER_COUNTER
        {
            return Err(RenderError::Invalid("invalid verification envelope"));
        }
        for value in [&self.validator_id, &self.validator_version] {
            if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
                return Err(RenderError::Invalid("invalid validator identity"));
            }
        }
        if !self.report.is_object() {
            return Err(RenderError::Invalid(
                "verification report must be an object",
            ));
        }
        let mut writer = BoundedCount {
            bytes: 0,
            limit: MAX_RENDER_REPORT_BYTES,
        };
        serde_json::to_writer(&mut writer, &self.report)?;
        Ok(())
    }
}
struct BoundedCount {
    bytes: usize,
    limit: usize,
}
impl Write for BoundedCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "render JSON byte bound",
            ));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Canonical whole-document identity shared by store and media hosts. This uses
/// exactly serde_json::to_writer(ProjectDocument), with no normalization layer.
pub fn document_sha256(
    document: &ProjectDocument,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Sha256, RenderError> {
    hash_document(
        document,
        DocumentHashControl::Bounded {
            cancelled,
            deadline,
        },
    )
}

/// Audit an already retained document without making validity depend on total
/// history size or machine speed. The canonical serialization byte bound still
/// applies. Worker operations must use `document_sha256` with their controls.
pub fn document_sha256_for_validation(document: &ProjectDocument) -> Result<Sha256, RenderError> {
    hash_document(document, DocumentHashControl::Validation)
}

#[derive(Clone, Copy)]
enum DocumentHashControl<'a> {
    Bounded {
        cancelled: &'a AtomicBool,
        deadline: Instant,
    },
    Validation,
}
impl DocumentHashControl<'_> {
    fn check(self) -> Result<(), RenderError> {
        if let Self::Bounded {
            cancelled,
            deadline,
        } = self
        {
            if cancelled.load(Ordering::Acquire) {
                return Err(RenderError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(RenderError::Deadline);
            }
        }
        Ok(())
    }
}

fn hash_document(
    document: &ProjectDocument,
    control: DocumentHashControl<'_>,
) -> Result<Sha256, RenderError> {
    struct HashWriter<'a> {
        hasher: Hasher,
        bytes: usize,
        control: DocumentHashControl<'a>,
    }
    impl Write for HashWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.control.check().is_err() {
                return Err(io::Error::other("render document hashing interrupted"));
            }
            if bytes.len() > deadpan_core::MAX_DOCUMENT_JSON_BYTES.saturating_sub(self.bytes) {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "render document hash byte bound",
                ));
            }
            self.bytes += bytes.len();
            self.hasher.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter {
        hasher: Hasher::new(),
        bytes: 0,
        control,
    };
    let serialized = serde_json::to_writer(&mut writer, document);
    control.check()?;
    serialized?;
    let hex: String = writer
        .hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Sha256::new(hex).map_err(|_| RenderError::Invalid("document hash"))
}

#[cfg(test)]
mod tests;
