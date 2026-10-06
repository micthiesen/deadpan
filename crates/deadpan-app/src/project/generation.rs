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

use deadpan_core::GeneratedObjectRef;
use deadpan_core::{FrameRange, NodeId, ProjectDocument, ProjectFrame, ProjectId, RevisionId};
use deadpan_jobs::{AttemptId, RequestId, WorkerStage};

use super::SequenceScope;

/// The most variants one Generate adds, one attempt after another.
pub const MAX_VARIANTS: u8 = 4;

/// A user command for the AI pause workflow. Every variant names the session
/// it was issued in; a stale session or revision is refused by identity.
/// `ticket` identifies the command's reply in [`Update::reply`].
#[derive(Clone, Debug)]
pub enum GenerationOperation {
    /// Generate `variants` candidates for the captured Hold at the captured
    /// revision, one attempt after another. They are added to the Hold's
    /// current request when its boundary pictures are unchanged; otherwise
    /// the first records a new request.
    Start {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        hold: NodeId,
        variants: u8,
    },
    /// Cancel the job started with ticket `job`.
    Cancel { ticket: u64, session: u64, job: u64 },
    /// Choose which Ready variant of `request` Preview and Accept use. This
    /// is the store's operational selection, never an edit.
    Select {
        ticket: u64,
        session: u64,
        request: RequestId,
        attempt: AttemptId,
    },
    /// Build the read-only acceptance preview of a Ready variant, selecting it.
    /// `draft` is the preview's proposal identity, allocated from the
    /// workspace's one monotonic counter shared with every other proposed
    /// edit (Gain, Trim, Slip, Splice), so audition caches and playback
    /// attribution never confuse them.
    Preview {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        request: RequestId,
        attempt: AttemptId,
        draft: u64,
    },
    /// Accept the variant as one undoable edit; the Hold stays selected.
    Accept {
        session: u64,
        revision: RevisionId,
        request: RequestId,
        attempt: AttemptId,
        hold: NodeId,
        cursor: ProjectFrame,
        scope: SequenceScope,
    },
    /// Durably discard one Ready variant. The store marks it unavailable
    /// (its retained objects stay until a retention policy removes them); it
    /// is never offered again, including after reopening. Not undoable.
    Discard {
        ticket: u64,
        session: u64,
        request: RequestId,
        attempt: AttemptId,
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
    /// Variants this job generates, and the 1-based one in progress.
    pub variants: u8,
    pub variant: u8,
    /// Variants of this job that reached Ready.
    pub ready: u8,
    pub phase: Phase,
    /// None while the job runs.
    pub outcome: Option<Outcome>,
    /// What else the outcome means for this job's variants, such as Ready
    /// variants kept after a later one failed, was cancelled or could not
    /// start.
    pub note: Option<String>,
}

impl Job {
    pub fn running(&self) -> bool {
        self.outcome.is_none()
    }
}

/// One Ready variant: an attempt of the request with its own seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub attempt: AttemptId,
    /// The attempt ordinal within the request (1-based, gaps where an
    /// attempt failed, was cancelled or was discarded).
    pub ordinal: u64,
    pub seed: u64,
    /// The sampled master, at project rate, that thumbnails decode.
    pub sampled: GeneratedObjectRef,
    pub sampled_frames: u32,
    pub sampled_size: (u32, u32),
}

/// A current request's present Ready variants that the Hold has not
/// accepted, oldest first, and the one Preview and Accept use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub request: RequestId,
    pub hold: NodeId,
    /// The revision the request was conditioned from.
    pub origin: RevisionId,
    /// Project frames the sampled master covers.
    pub frames: i64,
    /// Never empty.
    pub variants: Vec<Variant>,
    /// Always one of `variants`: the store's selection when it is offered,
    /// otherwise the newest.
    pub selected: AttemptId,
}

impl Candidate {
    /// The selected variant's 0-based position.
    pub fn selected_index(&self) -> usize {
        self.variants
            .iter()
            .position(|variant| variant.attempt == self.selected)
            .unwrap_or(0)
    }
}

/// The read-only document that accepting `request` would commit, issued by
/// the service from the store's own acceptance preview. Its fields are
/// private: only the service constructs it, and the preview worker admits it
/// against the exact committed base it was prepared from.
pub struct CandidatePreview {
    session: u64,
    project: ProjectId,
    base: RevisionId,
    request: RequestId,
    attempt: AttemptId,
    hold: NodeId,
    range: FrameRange,
    document: Arc<ProjectDocument>,
    /// The same proposed document admitted for audition against the exact
    /// committed base's sources: the pause's own sound, unchanged by
    /// accepting pictures, plays with the candidate's pictures.
    audio: Arc<deadpan_playback::Snapshot>,
}

/// What the service supplies to issue a [`CandidatePreview`].
pub(super) struct PreviewParts {
    pub session: u64,
    pub request: RequestId,
    pub attempt: AttemptId,
    pub hold: NodeId,
    pub range: FrameRange,
    pub document: Arc<ProjectDocument>,
    pub audio: Arc<deadpan_playback::Snapshot>,
}

impl std::fmt::Debug for CandidatePreview {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CandidatePreview")
            .field("session", &self.session)
            .field("project", &self.project)
            .field("base", &self.base)
            .field("request", &self.request)
            .field("attempt", &self.attempt)
            .field("hold", &self.hold)
            .field("range", &self.range)
            .field("content", &self.audio.content)
            .finish_non_exhaustive()
    }
}

impl CandidatePreview {
    pub(super) fn new(base: &ProjectDocument, parts: PreviewParts) -> Result<Self, String> {
        let PreviewParts {
            session,
            request,
            attempt,
            hold,
            range,
            document,
            audio,
        } = parts;
        if document.project_id() != base.project_id()
            || document.revision_id() == base.revision_id()
            || !Arc::ptr_eq(&audio.document, &document)
            || !matches!(&audio.content, deadpan_playback::ContentIdentity::Proposed { base_revision, .. } if base_revision == base.revision_id())
        {
            return Err("The AI preview does not match its project revision.".into());
        }
        Ok(Self {
            session,
            project: base.project_id().clone(),
            base: base.revision_id().clone(),
            request,
            attempt,
            hold,
            range,
            document,
            audio,
        })
    }

    /// The Ready variant this preview shows.
    pub fn attempt(&self) -> &AttemptId {
        &self.attempt
    }

    /// The proposed document's audition snapshot.
    pub fn audio(&self) -> &Arc<deadpan_playback::Snapshot> {
        &self.audio
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
    /// deterministic script. A Ready ending runs the synthetic worker
    /// (`deadpan_cli::generation::attempt::synthetic`), whose footage is
    /// qualified and published by the production path.
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted(Arc<ScriptQueue>),
}

/// Scripted runs in order, one per attempt (each variant takes the next);
/// the last one repeats.
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
    /// Produce synthetic footage that host qualification, publication and
    /// Ready then treat exactly as a real worker's. Needs an `ffmpeg` with
    /// `libx264rgb` (`DEADPAN_BRIDGE_FFMPEG`, else the Homebrew path) and
    /// `deadpan-media-worker` (`DEADPAN_MEDIA_WORKER`, else beside the
    /// executable or its parent directory); without them the attempt fails
    /// and says so.
    Ready,
}

/// The synthetic worker's external tools, from the environment or their
/// development locations.
#[cfg(any(test, feature = "ui-harness"))]
pub fn synthetic_tools()
-> Result<deadpan_cli::generation::attempt::synthetic::SyntheticWorker, String> {
    let ffmpeg = std::env::var_os("DEADPAN_BRIDGE_FFMPEG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    let media_worker = std::env::var_os("DEADPAN_MEDIA_WORKER")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let executable = std::env::current_exe().ok()?;
            let directory = executable.parent()?;
            [directory, directory.parent()?]
                .into_iter()
                .map(|directory| directory.join(deadpan_cli::generation::runtime::MEDIA_WORKER))
                .find(|path| path.is_file())
        })
        .ok_or(
            "The scripted Ready worker needs deadpan-media-worker (set DEADPAN_MEDIA_WORKER).",
        )?;
    if !ffmpeg.is_file() || !media_worker.is_file() {
        return Err(format!(
            "The scripted Ready worker needs ffmpeg at {} (set DEADPAN_BRIDGE_FFMPEG) and deadpan-media-worker at {}.",
            ffmpeg.display(),
            media_worker.display()
        ));
    }
    Ok(
        deadpan_cli::generation::attempt::synthetic::SyntheticWorker {
            ffmpeg,
            media_worker,
        },
    )
}
