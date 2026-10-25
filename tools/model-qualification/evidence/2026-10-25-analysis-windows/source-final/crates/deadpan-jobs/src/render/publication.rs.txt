//! Durable publication declarations. These values record host observations;
//! deserializing them never grants authority to rename or trust movie bytes.
use super::{
    BoundedCount, MAX_RENDER_COUNTER, RenderDiagnostic, RenderError, RenderIntent, version,
};
use crate::{AttemptId, CancellationToken, RequestId, Sha256};
use serde::{Deserialize, Serialize};
use std::path::{Component, PathBuf};

pub const MAX_PUBLICATION_FILESYSTEM_BYTES: usize = 128 * 1024;
pub const MAX_PUBLICATION_REPORT_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PUBLICATION_MOVIE_BYTES: u64 = 64 * 1024 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationIntent {
    #[serde(deserialize_with = "version")]
    pub schema_version: u32,
    pub publication_id: RequestId,
    pub job_id: RequestId,
    pub verified_attempt_id: AttemptId,
    pub destination: PathBuf,
}
impl PublicationIntent {
    pub fn validate(&self) -> Result<(), RenderError> {
        let path = self
            .destination
            .to_str()
            .ok_or(RenderError::Invalid("publication path must be UTF-8"))?;
        if self.schema_version != 1
            || !self.destination.is_absolute()
            || path.len() > 4096
            || path.chars().any(char::is_control)
        {
            return Err(RenderError::Invalid("invalid publication destination"));
        }
        if self.destination.components().count() > 128
            || self
                .destination
                .components()
                .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
        {
            return Err(RenderError::Invalid(
                "publication destination is not absolute and normalized",
            ));
        }
        let name = self.movie_name()?;
        if name.len() > 255
            || !name.ends_with(".mp4")
            || name.starts_with(".deadpan-")
            || name == ".mp4"
        {
            return Err(RenderError::Invalid("invalid publication movie filename"));
        }
        Ok(())
    }
    pub fn movie_name(&self) -> Result<&str, RenderError> {
        self.destination
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or(RenderError::Invalid(
                "publication movie filename is missing",
            ))
    }
    pub fn report_name(&self) -> String {
        format!("deadpan-render-{}.json", self.publication_id.as_str())
    }
    pub fn movie_partial_name(&self) -> String {
        format!(".deadpan-{}.movie.partial", self.publication_id.as_str())
    }
    pub fn report_partial_name(&self) -> String {
        format!(".deadpan-{}.report.partial", self.publication_id.as_str())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedPublicationEvidence {
    #[serde(deserialize_with = "version")]
    pub schema_version: u32,
    pub movie_sha256: Sha256,
    pub movie_bytes: u64,
    pub report_sha256: Sha256,
    pub report_bytes: u64,
    pub contains_generated_pictures: bool,
    pub filesystem: serde_json::Value,
}
impl PreparedPublicationEvidence {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.schema_version != 1
            || !(1..=MAX_PUBLICATION_MOVIE_BYTES).contains(&self.movie_bytes)
            || !(1..=MAX_PUBLICATION_REPORT_BYTES).contains(&self.report_bytes)
            || !self.filesystem.is_object()
        {
            return Err(RenderError::Invalid(
                "invalid publication prepared evidence",
            ));
        }
        serde_json::to_writer(
            &mut BoundedCount {
                bytes: 0,
                limit: MAX_PUBLICATION_FILESYSTEM_BYTES,
            },
            &self.filesystem,
        )?;
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationPhase {
    Intent,
    Prepared,
    ReportCommitting,
    ReportCommitted,
    MovieCommitting,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationOutcome {
    InProgress,
    Interrupted,
    Unresolved,
    Failed,
    Cancelled,
    Published,
    PublishedUnconfirmed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationOperationKind {
    Publish,
    Reconcile,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationIdentity {
    pub publication_id: RequestId,
    pub operation_id: AttemptId,
    pub cancellation_token: CancellationToken,
    pub expected_sequence: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationOperation {
    pub publication_id: RequestId,
    pub operation_id: AttemptId,
    pub cancellation_token: CancellationToken,
    pub ordinal: u64,
    pub kind: PublicationOperationKind,
    pub verified_attempt_id: AttemptId,
    pub active: bool,
    pub outcome: PublicationOutcome,
    pub diagnostic: Option<RenderDiagnostic>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredPublication {
    pub intent: PublicationIntent,
    pub render_intent: RenderIntent,
    pub encoding_attempt_id: AttemptId,
    pub movie_sha256: Sha256,
    pub movie_bytes: u64,
    pub prepared: Option<PreparedPublicationEvidence>,
    pub phase: PublicationPhase,
    pub outcome: PublicationOutcome,
    pub sequence: u64,
    pub observed_movie_commit: bool,
    pub cancellation_requested: bool,
    pub operation: PublicationOperation,
}
impl StoredPublication {
    pub fn identity(&self) -> PublicationIdentity {
        PublicationIdentity {
            publication_id: self.intent.publication_id.clone(),
            operation_id: self.operation.operation_id.clone(),
            cancellation_token: self.operation.cancellation_token.clone(),
            expected_sequence: self.sequence,
        }
    }
    pub fn validate(&self) -> Result<(), RenderError> {
        self.intent.validate()?;
        self.render_intent.validate()?;
        if self.intent.job_id != self.render_intent.job_id
            || self.operation.publication_id != self.intent.publication_id
            || !(1..=MAX_RENDER_COUNTER).contains(&self.sequence)
            || !(1..=MAX_RENDER_COUNTER).contains(&self.operation.ordinal)
            || !(1..=MAX_PUBLICATION_MOVIE_BYTES).contains(&self.movie_bytes)
        {
            return Err(RenderError::Invalid(
                "publication binding or counters disagree",
            ));
        }
        if (self.phase == PublicationPhase::Intent) != self.prepared.is_none()
            || self.outcome != self.operation.outcome
            || (self.outcome == PublicationOutcome::InProgress) != self.operation.active
            || self.operation.active && self.sequence == MAX_RENDER_COUNTER
        {
            return Err(RenderError::Invalid(
                "publication phase or operation state disagrees",
            ));
        }
        if self.observed_movie_commit
            && (!matches!(
                self.outcome,
                PublicationOutcome::Published
                    | PublicationOutcome::PublishedUnconfirmed
                    | PublicationOutcome::InProgress
            ) || self.phase != PublicationPhase::MovieCommitting)
            || matches!(
                self.outcome,
                PublicationOutcome::Published | PublicationOutcome::PublishedUnconfirmed
            ) && !self.observed_movie_commit
            || self.outcome == PublicationOutcome::Cancelled && !self.cancellation_requested
        {
            return Err(RenderError::Invalid(
                "publication commit knowledge disagrees",
            ));
        }
        if let Some(prepared) = &self.prepared {
            prepared.validate()?;
            if prepared.movie_sha256 != self.movie_sha256
                || prepared.movie_bytes != self.movie_bytes
            {
                return Err(RenderError::Invalid(
                    "prepared movie differs from retained checkpoint",
                ));
            }
        }
        if let Some(diagnostic) = &self.operation.diagnostic {
            diagnostic.validate()?;
        }
        if matches!(
            self.outcome,
            PublicationOutcome::Failed
                | PublicationOutcome::Interrupted
                | PublicationOutcome::Unresolved
                | PublicationOutcome::PublishedUnconfirmed
        ) != self.operation.diagnostic.is_some()
        {
            return Err(RenderError::Invalid(
                "publication diagnostic disagrees with outcome",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub enum PublicationCompletion {
    Failed(RenderDiagnostic),
    Cancelled,
    Published,
    PublishedUnconfirmed(RenderDiagnostic),
    Unresolved(RenderDiagnostic),
}
#[derive(Debug, Clone)]
pub enum PublicationReconciliation {
    Confirmed,
    CommittedUnconfirmed(RenderDiagnostic),
    NotPublished(RenderDiagnostic),
    Unresolved(RenderDiagnostic),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_intent_names_and_versions() {
        let mut value = serde_json::json!({"schema_version":1,"publication_id":"pub","job_id":"job","verified_attempt_id":"verify","destination":"/tmp/movie.mp4"});
        let intent: PublicationIntent = serde_json::from_value(value.clone()).unwrap();
        intent.validate().unwrap();
        assert_ne!(intent.movie_partial_name(), intent.report_partial_name());
        assert_ne!(intent.report_name(), intent.movie_name().unwrap());
        value["schema_version"] = 2.into();
        assert!(serde_json::from_value::<PublicationIntent>(value.clone()).is_err());
        value["schema_version"] = 1.into();
        value["extra"] = true.into();
        assert!(serde_json::from_value::<PublicationIntent>(value).is_err());
        for destination in [
            "relative.mp4",
            "/tmp/../movie.mp4",
            "/tmp/.deadpan-pub.movie.mp4",
            "/tmp/.mp4",
            "/tmp/movie.mov",
        ] {
            let mut invalid = intent.clone();
            invalid.destination = destination.into();
            assert!(invalid.validate().is_err());
        }
    }
}
