//! Native AI pause (bridge Hold) generation: requests, progress and Ready
//! candidates published by the project service.
//!
//! Generation only proposes pictures. Conditioning and the worker run on one
//! bounded job thread per project; every durable transition is applied by the
//! service's writer. Only [`GenerationOperation::Accept`] edits the project,
//! as one ordinary undoable revision.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use deadpan_core::{FrameRange, NodeId, ProjectDocument, ProjectFrame, ProjectId, RevisionId};
use deadpan_jobs::{RequestId, WorkerStage};

use super::SequenceScope;

/// A user command for the AI pause workflow. Every variant names the session
/// it was issued in; a stale session or revision is refused by identity.
/// `ticket` identifies the command's reply in [`Update::reply`].
#[derive(Clone, Debug)]
pub enum GenerationOperation {
    /// Generate pictures for the captured Hold at the captured revision.
    Start {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        hold: NodeId,
    },
    /// Cancel the job started with ticket `job`.
    Cancel { ticket: u64, session: u64, job: u64 },
    /// Build the read-only acceptance preview of a Ready candidate.
    Preview {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        request: RequestId,
    },
    /// Accept the candidate as one undoable edit; the Hold stays selected.
    Accept {
        session: u64,
        revision: RevisionId,
        request: RequestId,
        hold: NodeId,
        cursor: ProjectFrame,
        scope: SequenceScope,
    },
    /// Hide the candidate for the rest of this session. Nothing is written:
    /// the Ready bundle stays retained until a new request for the Hold
    /// supersedes it, and it is offered again after reopening.
    Discard {
        ticket: u64,
        session: u64,
        request: RequestId,
    },
}

/// Where a running job is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Decoding the boundary pictures (read-only, job thread).
    Conditioning,
    /// Writing and capturing the conditioning inputs.
    Preparing,
    Stage(WorkerStage),
    Step {
        stage: WorkerStage,
        completed: u64,
        total: u64,
    },
    /// The worker exited cleanly; the host is qualifying its bundle.
    Qualifying,
    /// Cancellation was requested; waiting for the worker to be reaped.
    Cancelling,
}

impl Phase {
    /// A short readable stage name.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Conditioning => "Reading boundary pictures",
            Self::Preparing => "Preparing inputs",
            Self::Stage(stage) | Self::Step { stage, .. } => stage_label(*stage),
            Self::Qualifying => "Checking pictures",
            Self::Cancelling => "Cancelling",
        }
    }

    /// Completed and total steps, when the worker reports them.
    pub fn steps(&self) -> Option<(u64, u64)> {
        match self {
            Self::Step {
                completed, total, ..
            } => Some((*completed, *total)),
            _ => None,
        }
    }
}

pub fn stage_label(stage: WorkerStage) -> &'static str {
    match stage {
        WorkerStage::Preflight => "Checking the model runtime",
        WorkerStage::RuntimeLoading => "Loading the runtime",
        WorkerStage::ModelLoading => "Loading the model",
        WorkerStage::Conditioning => "Conditioning",
        WorkerStage::Inference => "Generating pictures",
        WorkerStage::Decoding => "Decoding",
        WorkerStage::Encoding => "Encoding",
        WorkerStage::WorkerValidation => "Validating",
    }
}

/// How a job ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The development runtime is missing; nothing was recorded.
    Unavailable(String),
    Failed(String),
    Cancelled,
    /// The attempt is Ready; the candidate is offered for the Hold.
    Ready(RequestId),
}

/// The latest job of this session, live or concluded.
#[derive(Clone, Debug)]
pub struct Job {
    pub ticket: u64,
    pub session: u64,
    pub hold: NodeId,
    /// The revision the inputs were prepared from.
    pub revision: RevisionId,
    pub started: Instant,
    /// The recorded request, once allocated.
    pub request: Option<RequestId>,
    pub phase: Phase,
    /// None while the job runs.
    pub outcome: Option<Outcome>,
}

impl Job {
    pub fn running(&self) -> bool {
        self.outcome.is_none()
    }
}

/// A current Ready bundle that the Hold has not accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub request: RequestId,
    pub hold: NodeId,
    /// The revision the request was conditioned from.
    pub origin: RevisionId,
    /// Project frames the sampled master covers.
    pub frames: i64,
}

/// The read-only document that accepting `request` would commit, issued by
/// the service from the store's own acceptance preview. Its fields are
/// private: only the service constructs it, and the preview worker admits it
/// against the exact committed base it was prepared from.
#[derive(Debug)]
pub struct CandidatePreview {
    session: u64,
    project: ProjectId,
    base: RevisionId,
    request: RequestId,
    hold: NodeId,
    range: FrameRange,
    document: Arc<ProjectDocument>,
}

impl CandidatePreview {
    pub(super) fn new(
        session: u64,
        base: &ProjectDocument,
        request: RequestId,
        hold: NodeId,
        range: FrameRange,
        document: Arc<ProjectDocument>,
    ) -> Result<Self, String> {
        if document.project_id() != base.project_id()
            || document.revision_id() == base.revision_id()
        {
            return Err("The AI preview does not match its project revision.".into());
        }
        Ok(Self {
            session,
            project: base.project_id().clone(),
            base: base.revision_id().clone(),
            request,
            hold,
            range,
            document,
        })
    }

    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn project(&self) -> &ProjectId {
        &self.project
    }

    /// The committed revision this preview applies to.
    pub fn base(&self) -> &RevisionId {
        &self.base
    }

    pub fn request(&self) -> &RequestId {
        &self.request
    }

    pub fn hold(&self) -> &NodeId {
        &self.hold
    }

    /// The Hold's project-frame range, unchanged by acceptance.
    pub fn range(&self) -> FrameRange {
        self.range
    }

    pub fn document(&self) -> &Arc<ProjectDocument> {
        &self.document
    }
}

/// Generation state for one project session, independent of editor feedback.
#[derive(Clone, Debug, Default)]
pub struct Update {
    pub session: u64,
    pub job: Option<Job>,
    /// Ready candidates by Hold for the published workspace revision.
    pub candidates: Arc<BTreeMap<NodeId, Candidate>>,
    /// The latest preview reply, tagged by request and base revision.
    pub preview: Option<Arc<CandidatePreview>>,
    /// The latest independent command's ticket and refusal, if any. Accept
    /// reports through the ordinary edit receipt instead.
    pub reply: Option<(u64, Option<String>)>,
}

/// How the project service runs the AI worker. Production always uses the
/// environment's development runtime and the real supervised worker.
#[derive(Clone, Default)]
pub enum Backend {
    #[default]
    Environment,
    /// Test and replay seam. Conditioning, request allocation and every
    /// durable transition are real; only the model worker is replaced by a
    /// deterministic script that can never produce a Ready bundle.
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted(Arc<ScriptQueue>),
}

/// Scripted runs in order, one per start; the last one repeats.
#[cfg(any(test, feature = "ui-harness"))]
#[derive(Debug)]
pub struct ScriptQueue(std::sync::Mutex<std::collections::VecDeque<Script>>);

#[cfg(any(test, feature = "ui-harness"))]
impl ScriptQueue {
    pub fn new(runs: impl IntoIterator<Item = Script>) -> Self {
        Self(std::sync::Mutex::new(runs.into_iter().collect()))
    }

    pub fn next(&self) -> Option<Script> {
        let mut runs = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if runs.len() > 1 {
            runs.pop_front()
        } else {
            runs.front().cloned()
        }
    }
}

#[cfg(any(test, feature = "ui-harness"))]
#[derive(Clone, Debug)]
pub struct Script {
    /// Report this runtime error instead of starting.
    pub unavailable: Option<String>,
    /// Inference steps reported before the ending.
    pub steps: u64,
    pub step_interval: std::time::Duration,
    pub ending: ScriptEnding,
}

#[cfg(any(test, feature = "ui-harness"))]
#[derive(Clone, Debug)]
pub enum ScriptEnding {
    /// Keep reporting the last step until cancelled.
    WaitForCancel,
    /// Conclude with this worker failure.
    Fail(String),
}
