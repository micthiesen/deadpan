use deadpan_core::{NodeId, ProjectId};
use thiserror::Error;

use crate::protocol::{
    CancellationToken, CandidateManifest, Diagnostic, MessageIdentity, RequestVersion, Sha256,
    StageProgress, WorkerFailure, WorkerMessage, WorkerStage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Preflight,
    Loading,
    Running,
    Validating,
    Ready,
    Failed,
    Cancelling,
    Cancelled,
}

impl JobState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Ready | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relevance {
    Current,
    Stale,
    Detached,
}

/// Generation dependencies used to decide whether a result still targets the
/// authored Hold. The originating revision remains on the request as provenance;
/// unrelated document revisions do not stale a generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetBinding {
    pub project_id: ProjectId,
    pub hold_id: NodeId,
    pub request_version: RequestVersion,
    pub context_sha256: Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobProgress {
    pub stage: WorkerStage,
    pub progress: StageProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFailureCode {
    SpawnFailed,
    WorkerExited,
    ProtocolViolation,
    Io,
    DeadlineExceeded,
    OutputValidationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFailure {
    pub code: HostFailureCode,
    pub detail: Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobFailure {
    Worker(WorkerFailure),
    Host(HostFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerEventOutcome {
    Applied,
    Duplicate,
    IgnoredDuringCancellation,
    CancelledAfterCompletion,
}

/// Pure state for one worker attempt.
///
/// Persistence and restart reconciliation are deferred to the store/supervisor.
/// Recovery must create a new attempt ID; it must never infer success from an
/// abandoned process or this in-memory state alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLifecycle {
    identity: MessageIdentity,
    cancellation_token: CancellationToken,
    target: TargetBinding,
    state: JobState,
    relevance: Relevance,
    worker_stage: Option<WorkerStage>,
    progress: Option<JobProgress>,
    candidate: Option<CandidateManifest>,
    failure: Option<JobFailure>,
}

impl JobLifecycle {
    pub fn new(
        identity: MessageIdentity,
        cancellation_token: CancellationToken,
        target: TargetBinding,
    ) -> Self {
        Self {
            identity,
            cancellation_token,
            target,
            state: JobState::Queued,
            relevance: Relevance::Current,
            worker_stage: None,
            progress: None,
            candidate: None,
            failure: None,
        }
    }

    pub fn identity(&self) -> &MessageIdentity {
        &self.identity
    }

    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }

    pub fn target(&self) -> &TargetBinding {
        &self.target
    }

    pub const fn state(&self) -> JobState {
        self.state
    }

    pub const fn relevance(&self) -> Relevance {
        self.relevance
    }

    pub const fn worker_stage(&self) -> Option<WorkerStage> {
        self.worker_stage
    }

    pub fn progress(&self) -> Option<&JobProgress> {
        self.progress.as_ref()
    }

    pub fn candidate(&self) -> Option<&CandidateManifest> {
        self.candidate.as_ref()
    }

    pub fn failure(&self) -> Option<&JobFailure> {
        self.failure.as_ref()
    }

    /// Monotonically updates relevance without rebinding the job. `None` means
    /// the target Hold no longer exists. Matching values never revive stale work.
    pub fn observe_target(&mut self, current: Option<&TargetBinding>) {
        match (self.relevance, current) {
            (Relevance::Detached, _) => {}
            (_, None) => self.relevance = Relevance::Detached,
            (Relevance::Current, Some(current)) if current != &self.target => {
                self.relevance = Relevance::Stale;
            }
            _ => {}
        }
    }

    pub fn request_cancel(
        &mut self,
        identity: &MessageIdentity,
        token: &CancellationToken,
    ) -> Result<WorkerEventOutcome, LifecycleError> {
        self.check_identity(identity)?;
        if token != &self.cancellation_token {
            return Err(LifecycleError::WrongCancellationToken);
        }
        match self.state {
            JobState::Cancelling | JobState::Cancelled => Ok(WorkerEventOutcome::Duplicate),
            state if state.is_terminal() => Err(LifecycleError::MessageAfterTerminal(state)),
            _ => {
                self.state = JobState::Cancelling;
                self.candidate = None;
                self.progress = None;
                Ok(WorkerEventOutcome::Applied)
            }
        }
    }

    pub fn apply_worker_message(
        &mut self,
        message: &WorkerMessage,
    ) -> Result<WorkerEventOutcome, LifecycleError> {
        self.check_identity(message.identity())?;
        if self.state.is_terminal() {
            return Err(LifecycleError::MessageAfterTerminal(self.state));
        }
        if self.state == JobState::Validating {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                event: "worker message after completion",
            });
        }

        match message {
            WorkerMessage::Stage { .. } | WorkerMessage::Progress { .. }
                if self.state == JobState::Cancelling =>
            {
                Ok(WorkerEventOutcome::IgnoredDuringCancellation)
            }
            WorkerMessage::Stage { stage, .. } => self.apply_stage(*stage),
            WorkerMessage::Progress {
                stage, progress, ..
            } => self.apply_progress(*stage, progress.clone()),
            WorkerMessage::Completed { candidate, .. } => {
                if self.state == JobState::Cancelling {
                    self.state = JobState::Cancelled;
                    self.candidate = None;
                    self.progress = None;
                    return Ok(WorkerEventOutcome::CancelledAfterCompletion);
                }
                if self.state != JobState::Running {
                    return Err(LifecycleError::InvalidTransition {
                        from: self.state,
                        event: "worker completion",
                    });
                }
                self.state = JobState::Validating;
                self.candidate = Some(candidate.clone());
                self.progress = None;
                Ok(WorkerEventOutcome::Applied)
            }
            WorkerMessage::Failed { failure, .. } => {
                self.state = JobState::Failed;
                self.failure = Some(JobFailure::Worker(failure.clone()));
                self.progress = None;
                Ok(WorkerEventOutcome::Applied)
            }
            WorkerMessage::Cancelled { .. } => {
                if self.state != JobState::Cancelling {
                    return Err(LifecycleError::InvalidTransition {
                        from: self.state,
                        event: "worker cancellation",
                    });
                }
                self.state = JobState::Cancelled;
                self.progress = None;
                Ok(WorkerEventOutcome::Applied)
            }
        }
    }

    /// Records a supervisor-owned failure such as spawn, protocol, process-exit,
    /// deadline, or host media-validation failure.
    pub fn host_failed(
        &mut self,
        identity: &MessageIdentity,
        failure: HostFailure,
    ) -> Result<(), LifecycleError> {
        self.check_identity(identity)?;
        if self.state.is_terminal() {
            return Err(LifecycleError::MessageAfterTerminal(self.state));
        }
        self.state = JobState::Failed;
        self.failure = Some(JobFailure::Host(failure));
        self.progress = None;
        self.candidate = None;
        Ok(())
    }

    /// Finalizes cancellation after the supervisor has stopped and reaped the
    /// worker. This method does not itself control a process.
    pub fn host_cancelled(
        &mut self,
        identity: &MessageIdentity,
        token: &CancellationToken,
    ) -> Result<(), LifecycleError> {
        self.check_identity(identity)?;
        if token != &self.cancellation_token {
            return Err(LifecycleError::WrongCancellationToken);
        }
        if self.state == JobState::Cancelled {
            return Ok(());
        }
        if self.state != JobState::Cancelling {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                event: "host cancellation completion",
            });
        }
        self.state = JobState::Cancelled;
        self.progress = None;
        self.candidate = None;
        Ok(())
    }

    /// Records that the host independently validated the exact candidate.
    /// Calling this does not accept it into the document.
    pub fn host_validation_succeeded(
        &mut self,
        identity: &MessageIdentity,
        candidate: &CandidateManifest,
    ) -> Result<(), LifecycleError> {
        self.check_identity(identity)?;
        if self.state.is_terminal() {
            return Err(LifecycleError::MessageAfterTerminal(self.state));
        }
        if self.state != JobState::Validating {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                event: "host validation",
            });
        }
        if self.candidate.as_ref() != Some(candidate) {
            return Err(LifecycleError::CandidateMismatch);
        }
        self.state = JobState::Ready;
        Ok(())
    }

    /// Ready/current means the host may offer an explicit acceptance command.
    /// The command layer must still commit a normal revision-aware transaction.
    pub fn can_authorize_acceptance(&self) -> bool {
        self.state == JobState::Ready
            && self.relevance == Relevance::Current
            && self.candidate.is_some()
    }

    fn apply_stage(&mut self, stage: WorkerStage) -> Result<WorkerEventOutcome, LifecycleError> {
        if self.state == JobState::Cancelling {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                event: "worker stage",
            });
        }
        if let Some(previous) = self.worker_stage {
            if stage < previous {
                return Err(LifecycleError::StageRegression {
                    previous,
                    next: stage,
                });
            }
            if stage == previous {
                return Ok(WorkerEventOutcome::Duplicate);
            }
        } else if stage != WorkerStage::Preflight {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                event: "worker stage before preflight",
            });
        }

        let next_state = state_for_stage(stage);
        if state_rank(next_state) < state_rank(self.state) {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                event: "worker stage regression",
            });
        }
        if !matches!(
            self.state,
            JobState::Queued | JobState::Preflight | JobState::Loading | JobState::Running
        ) {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                event: "worker stage",
            });
        }
        self.state = next_state;
        self.worker_stage = Some(stage);
        self.progress = None;
        Ok(WorkerEventOutcome::Applied)
    }

    fn apply_progress(
        &mut self,
        stage: WorkerStage,
        progress: StageProgress,
    ) -> Result<WorkerEventOutcome, LifecycleError> {
        if self.state != state_for_stage(stage) {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                event: "worker progress",
            });
        }
        if self.worker_stage != Some(stage) {
            return Err(LifecycleError::ProgressStageMismatch {
                active: self.worker_stage,
                received: stage,
            });
        }
        if let Some(previous) = &self.progress {
            if previous.progress.total() != progress.total() {
                return Err(LifecycleError::ProgressTotalChanged {
                    previous: previous.progress.total(),
                    next: progress.total(),
                });
            }
            if progress.completed() < previous.progress.completed() {
                return Err(LifecycleError::ProgressRegression {
                    previous: previous.progress.completed(),
                    next: progress.completed(),
                });
            }
            if progress == previous.progress {
                return Ok(WorkerEventOutcome::Duplicate);
            }
        }
        self.progress = Some(JobProgress { stage, progress });
        Ok(WorkerEventOutcome::Applied)
    }

    fn check_identity(&self, identity: &MessageIdentity) -> Result<(), LifecycleError> {
        if identity.request_id != self.identity.request_id {
            return Err(LifecycleError::WrongRequest);
        }
        if identity.attempt_id != self.identity.attempt_id {
            return Err(LifecycleError::WrongAttempt);
        }
        Ok(())
    }
}

fn state_for_stage(stage: WorkerStage) -> JobState {
    match stage {
        WorkerStage::Preflight => JobState::Preflight,
        WorkerStage::RuntimeLoading | WorkerStage::ModelLoading => JobState::Loading,
        WorkerStage::Conditioning
        | WorkerStage::Inference
        | WorkerStage::Decoding
        | WorkerStage::Encoding
        | WorkerStage::WorkerValidation => JobState::Running,
    }
}

fn state_rank(state: JobState) -> u8 {
    match state {
        JobState::Queued => 0,
        JobState::Preflight => 1,
        JobState::Loading => 2,
        JobState::Running => 3,
        JobState::Validating => 4,
        JobState::Ready => 5,
        JobState::Failed | JobState::Cancelling | JobState::Cancelled => u8::MAX,
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("worker message has the wrong request ID")]
    WrongRequest,
    #[error("worker message has the wrong attempt ID")]
    WrongAttempt,
    #[error("cancel message has the wrong cancellation token")]
    WrongCancellationToken,
    #[error("{event} is invalid while job is {from:?}")]
    InvalidTransition { from: JobState, event: &'static str },
    #[error("message received after terminal state {0:?}")]
    MessageAfterTerminal(JobState),
    #[error("worker stage regressed from {previous:?} to {next:?}")]
    StageRegression {
        previous: WorkerStage,
        next: WorkerStage,
    },
    #[error("progress for {received:?} does not match active stage {active:?}")]
    ProgressStageMismatch {
        active: Option<WorkerStage>,
        received: WorkerStage,
    },
    #[error("stage progress regressed from {previous} to {next}")]
    ProgressRegression { previous: u64, next: u64 },
    #[error("stage progress total changed from {previous} to {next}")]
    ProgressTotalChanged { previous: u64, next: u64 },
    #[error("host validated a different candidate than the worker completed")]
    CandidateMismatch,
}
