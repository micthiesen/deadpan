use super::*;
use crate::encoded_render::{
    publication::{PublicationReceipt, RetainedPublicationArtifacts},
    workflow::{WorkflowOutcome, WorkflowProgress, WorkflowStage, WorkflowStatus},
};
use deadpan_core::{FrameRate, ProjectId, RevisionId};
use deadpan_jobs::render::{RenderAttemptState, RenderDiagnostic};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderContext {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
}
impl RenderContext {
    pub fn from_document(document: &ProjectDocument) -> Self {
        Self {
            project_id: document.project_id().clone(),
            revision_id: document.revision_id().clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowTarget {
    pub job_id: RequestId,
    pub attempt_id: AttemptId,
    pub cancellation_token: CancellationToken,
}
impl From<WorkflowTarget> for WorkflowIdentity {
    fn from(value: WorkflowTarget) -> Self {
        Self {
            job_id: value.job_id,
            attempt_id: value.attempt_id,
            cancellation_token: value.cancellation_token,
        }
    }
}
impl From<&WorkflowIdentity> for WorkflowTarget {
    fn from(value: &WorkflowIdentity) -> Self {
        Self {
            job_id: value.job_id.clone(),
            attempt_id: value.attempt_id.clone(),
            cancellation_token: value.cancellation_token.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub schema_version: u32,
    pub request_id: RequestId,
    pub context: RenderContext,
    pub operation: RenderOperation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum RenderOperation {
    Start {
        destination: PathBuf,
    },
    RetryCheckpoint {
        job_id: RequestId,
        encoding_attempt_id: AttemptId,
        destination: PathBuf,
    },
    Reencode {
        job_id: RequestId,
        destination: PathBuf,
    },
    Reconcile {
        publication_id: RequestId,
    },
    Cancel {
        target: WorkflowTarget,
    },
}
impl RenderRequest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, PublicRenderError> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(PublicRenderError::invalid("render request exceeds 16 KiB"));
        }
        let request: Self = serde_json::from_slice(bytes).map_err(PublicRenderError::invalid)?;
        if request.schema_version != SCHEMA_VERSION {
            return Err(PublicRenderError::new(
                "RenderProtocolUnsupported",
                "Render request schema_version must be 1",
            ));
        }
        Ok(request)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[serde(deny_unknown_fields)]
#[error("{code}: {message}")]
pub struct PublicRenderError {
    pub code: String,
    pub message: String,
    pub current_revision: Option<RevisionId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderOutputSummary {
    pub canvas: [u32; 2],
    pub raster: [u32; 2],
    pub frame_rate: FrameRate,
    pub frame_count: u64,
    pub audio_samples: i64,
    pub algorithm: RenderAutomaticAlgorithm,
    /// Encoded output branch: SdrRec709 (H.264) or HdrRec2020Pq/Hlg (HEVC Main10).
    pub color_policy: deadpan_core::ColorPolicy,
    pub color_reason: crate::picture::OutputColorReason,
    pub hdr_sources: bool,
    /// Declared HDR source peak used for tone mapping (SDR fallback output
    /// and the SDR preview of an HDR composite).
    pub tone_map_peak_nits: u32,
    /// PQ output only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mastering_display: Option<deadpan_core::MasteringDisplay>,
}

/// Compact live status. A stored verification report or decision never appears
/// as a live capability, and an admitted candidate is never labeled published.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderStatus {
    pub context: RenderContext,
    pub target: Option<WorkflowTarget>,
    pub stage: WorkflowStage,
    pub captured_revision: RevisionId,
    pub attempt_state: Option<RenderAttemptState>,
    pub checkpoint_attempt_id: Option<AttemptId>,
    pub publication_id: Option<RequestId>,
    pub progress: Option<WorkflowProgress>,
    pub cancellation_requested: bool,
    pub outcome: Option<WorkflowOutcome>,
    pub diagnostic: Option<RenderDiagnostic>,
    pub journal_diagnostic: Option<RenderDiagnostic>,
    pub retained: RetainedPublicationArtifacts,
    pub receipt: Option<PublicationReceipt>,
    pub observed_movie_commit: bool,
    pub cleanup_confirmed: bool,
}
impl RenderStatus {
    pub fn from_workflow(context: &RenderContext, status: &WorkflowStatus) -> Self {
        Self {
            context: context.clone(),
            target: status.identity.as_ref().map(WorkflowTarget::from),
            stage: status.stage,
            captured_revision: status.intent.as_ref().map_or_else(
                || context.revision_id.clone(),
                |intent| intent.revision_id.clone(),
            ),
            attempt_state: status.attempt.as_ref().map(|attempt| attempt.state),
            checkpoint_attempt_id: status
                .attempt
                .as_ref()
                .and_then(|attempt| attempt.checkpoint_attempt_id.clone()),
            publication_id: status
                .publication
                .as_ref()
                .map(|record| record.intent.publication_id.clone()),
            progress: status.progress.clone(),
            cancellation_requested: status.cancellation_requested,
            outcome: status.outcome,
            diagnostic: status.diagnostic.clone(),
            journal_diagnostic: status.journal_diagnostic.clone(),
            retained: status.retained.clone(),
            receipt: status.receipt.clone(),
            observed_movie_commit: status.observed_movie_commit,
            cleanup_confirmed: status.cleanup_confirmed,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RenderEvent {
    Admitted {
        schema_version: u32,
        request_id: RequestId,
        status: Box<RenderStatus>,
    },
    Progress {
        schema_version: u32,
        request_id: RequestId,
        status: Box<RenderStatus>,
    },
    Finished {
        schema_version: u32,
        request_id: RequestId,
        status: Box<RenderStatus>,
    },
    RecoveryRequired {
        schema_version: u32,
        request_id: RequestId,
        status: Box<RenderStatus>,
        error: PublicRenderError,
    },
}
