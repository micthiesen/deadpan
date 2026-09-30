use super::*;

/// The trusted host chooses the executable and explicit engineering limits.
/// Constructing a coordinator does not qualify a public/automatic export path.
#[derive(Debug, Clone)]
pub struct WorkflowConfig {
    pub package: PathBuf,
    pub runtime: RenderWorkerRuntime,
    pub encode_limits: EncodedWorkerLimits,
    pub verification_limits: VerificationLimits,
    pub media_limits: RenderMediaLimits,
}

/// Exact cancellation target. A delayed request cannot cancel a later run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkflowIdentity {
    pub job_id: RequestId,
    pub attempt_id: AttemptId,
    pub cancellation_token: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct PublicationRequest {
    pub destination: PathBuf,
    pub publication_id: RequestId,
    pub operation_id: AttemptId,
    pub cancellation_token: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct StartRender {
    pub revision: RevisionId,
    pub range: Option<FrameRange>,
    pub identity: WorkflowIdentity,
    pub policy: RenderPolicy,
    pub publication: PublicationRequest,
    pub deadline: Instant,
}

#[derive(Debug, Clone)]
pub struct RetryRender {
    pub identity: WorkflowIdentity,
    /// None explicitly starts a new encode of the job's immutable intent.
    /// Some rehashes this checkpoint and independently verifies it again.
    pub checkpoint_attempt_id: Option<AttemptId>,
    pub publication: PublicationRequest,
    pub deadline: Instant,
}

#[derive(Debug, Clone)]
pub struct ReconcileRender {
    pub publication_id: RequestId,
    pub identity: WorkflowIdentity,
    pub operation_id: AttemptId,
    pub cancellation_token: CancellationToken,
    pub deadline: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStage {
    Idle,
    Capturing,
    Qualifying,
    Encoding,
    Verifying,
    PreparingPublication,
    CommittingReport,
    CommittingMovie,
    Reconciling,
    Cancelling,
    Releasing,
    Finished,
    /// The slot stays occupied: cleanup or a durable transition is unresolved.
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowOutcome {
    Published,
    PublishedUnconfirmed,
    NotPublished,
    Cancelled,
    Failed,
    Unresolved,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum WorkflowProgress {
    Qualification {
        choice: crate::encoded_render::protocol::EncoderChoice,
        completed_frames: u64,
        total_frames: u64,
    },
    Encoding {
        completed_frames: u64,
        total_frames: u64,
        completed_audio_samples: u64,
        total_audio_samples: u64,
    },
    Verification(VerificationProgress),
    Publication(PublicationStage),
}

/// Last status is retained after completion. Receipt and commit knowledge are
/// independent of the final SQLite write, which may itself fail.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStatus {
    pub identity: Option<WorkflowIdentity>,
    pub stage: WorkflowStage,
    pub intent: Option<RenderIntent>,
    pub attempt: Option<StoredRenderAttempt>,
    pub publication: Option<StoredPublication>,
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

impl Default for WorkflowStatus {
    fn default() -> Self {
        Self {
            identity: None,
            stage: WorkflowStage::Idle,
            intent: None,
            attempt: None,
            publication: None,
            progress: None,
            cancellation_requested: false,
            outcome: None,
            diagnostic: None,
            journal_diagnostic: None,
            retained: RetainedPublicationArtifacts::default(),
            receipt: None,
            observed_movie_commit: false,
            cleanup_confirmed: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("a render workflow still owns the execution slot")]
    Busy,
    #[error("render workflow identity does not match the current run")]
    Identity,
    #[error("render revision changed before start admission")]
    StaleRevision,
    #[error("invalid render workflow: {0}")]
    Configuration(String),
    #[error("render workflow requires recovery: {0}")]
    Unresolved(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
