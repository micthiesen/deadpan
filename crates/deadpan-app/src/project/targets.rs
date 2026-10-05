//! Attention targets in the native workspace: saving keyboard-made rectangles
//! and corrections, and tracking a saved target in the background.
//!
//! Saving is an ordinary reversible `SetTarget` edit on the writer. Tracking
//! runs on one bounded job thread per project: it opens the package read-only,
//! resolves the range and copies the verified Original (`prepare_tracking`),
//! then runs the supervised worker. The writer saves the result expecting the
//! revision the command was entered at, so an edit made meanwhile refuses the
//! save instead of being overwritten. Session replacement and shutdown cancel
//! the job and wait until it has drained.
//!
//! Face proposals run on a second bounded job thread the same way: one
//! indexed picture, the supervised `deadpan-track detect-faces` worker, and
//! admitted faces published as proposals. Nothing is saved until the person
//! uses one (`:zoom … target=face:N`), which saves the face as a target and
//! frames the beat toward it in one undoable transaction.

#[cfg(any(test, feature = "ui-harness"))]
use std::sync::Arc;
use std::time::Instant;

use deadpan_cli::faces::DetectedFace;
use deadpan_core::{AssetId, AttentionTarget, Framing, NodeId, RevisionId, TargetId, TargetRegion};

/// A user command for targets. Every variant names the session it was issued
/// in; `ticket` identifies its reply in [`Update::reply`].
#[derive(Clone, Debug)]
pub enum Operation {
    /// Add or replace one target as one undoable edit expecting `revision`.
    Save {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        id: TargetId,
        target: Box<AttentionTarget>,
    },
    /// Track or correct a saved target; the result is saved expecting
    /// `revision`, the head when the command was entered.
    Track {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        id: TargetId,
        mode: TrackMode,
    },
    /// Cancel the job started with ticket `job`.
    Cancel { ticket: u64, session: u64, job: u64 },
    /// Propose faces in one indexed picture of `asset` in the background.
    /// Detection never edits; [`FaceJob`] reports the proposals.
    DetectFaces {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        asset: AssetId,
        pts: i64,
    },
    /// Save a chosen face as target `id` and set `node`'s framing toward it
    /// in one undoable transaction expecting `revision`.
    SaveFramed {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        scope: crate::project::SequenceScope,
        cursor: deadpan_core::ProjectFrame,
        node: NodeId,
        id: TargetId,
        target: Box<AttentionTarget>,
        framing: Option<Box<Framing>>,
    },
}

/// The latest face detection of this session: proposals only.
#[derive(Clone, Debug)]
pub struct FaceJob {
    pub ticket: u64,
    pub session: u64,
    /// The head revision the picture was resolved against.
    pub revision: RevisionId,
    pub asset: AssetId,
    /// The indexed picture analysed.
    pub pts: i64,
    /// None while detection runs.
    pub outcome: Option<FaceOutcome>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FaceOutcome {
    /// Admitted proposals, ordered left to right, then top to bottom.
    Found(Vec<DetectedFace>),
    /// The detector or Original is missing; nothing was proposed.
    Unavailable(String),
    Failed(String),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackMode {
    /// Track from the target's initial rectangle over its span. Without
    /// `through_shots`, a stored shot analysis is required and the first shot
    /// boundary ends tracking.
    Track { through_shots: bool },
    /// Re-track only the range a correction at indexed picture `at` governs,
    /// from `region`.
    Correct { at: i64, region: TargetRegion },
}

/// Where a running job is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Resolving the range and copying the verified Original.
    Preparing,
    /// The worker's monotonic percentage.
    Tracking(u8),
    /// Cancellation was requested; waiting for the worker to stop.
    Cancelling,
}

impl Phase {
    pub fn label(self) -> String {
        match self {
            Self::Preparing => "Preparing pictures".into(),
            Self::Tracking(percent) => format!("Tracking {percent}%"),
            Self::Cancelling => "Cancelling".into(),
        }
    }
}

/// How a job ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Saved as one undoable edit.
    Saved {
        revision: RevisionId,
        samples: usize,
        tolerance: u32,
    },
    /// Nothing was saved. The tracker, Original or shot analysis is missing.
    Unavailable(String),
    Failed(String),
    Cancelled,
}

/// The latest tracking job of this session, live or concluded.
#[derive(Clone, Debug)]
pub struct Job {
    pub ticket: u64,
    pub session: u64,
    pub target: TargetId,
    pub label: String,
    pub correction: bool,
    /// The revision tracking started from and the save expects.
    pub revision: RevisionId,
    pub started: Instant,
    pub phase: Phase,
    /// None while the job runs.
    pub outcome: Option<Outcome>,
}

impl Job {
    pub fn running(&self) -> bool {
        self.outcome.is_none()
    }
}

/// The latest durable target save of this session, from a direct save or a
/// tracking job. Camera continues on `revision` only when it was open on
/// `base`; any other revision change still revokes its draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub session: u64,
    pub base: RevisionId,
    pub revision: RevisionId,
    pub id: TargetId,
}

/// Target state for one project session, independent of editor feedback.
#[derive(Clone, Debug, Default)]
pub struct Update {
    pub session: u64,
    pub job: Option<Job>,
    /// The latest independent command's ticket and refusal, if any.
    pub reply: Option<(u64, Option<String>)>,
    pub saved: Option<Saved>,
    pub faces: Option<FaceJob>,
}

/// How the project service tracks. Production always uses the worker
/// installed beside the executable.
#[derive(Clone, Default)]
pub enum Backend {
    #[default]
    Environment,
    /// Test and replay seam. Range resolution, the verified Original copy,
    /// the tracking policy, compaction and the revision-guarded save are
    /// real; only the Vision worker is replaced by deterministic observations.
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted(Arc<ScriptQueue>),
    /// Test and replay seam for face proposals: picture resolution, the
    /// verified Original copy and the host's face admission are real; each
    /// detection takes the next run, and only `Faces` runs replace the Vision
    /// worker. Tracking uses the installed worker.
    #[cfg(any(test, feature = "ui-harness"))]
    ScriptedFaces(Arc<FaceScript>),
}

/// Face detection runs in order, one per start; the last one repeats.
#[cfg(any(test, feature = "ui-harness"))]
#[derive(Debug)]
pub struct FaceScript(std::sync::Mutex<std::collections::VecDeque<FaceRun>>);

#[cfg(any(test, feature = "ui-harness"))]
#[derive(Clone, Debug)]
pub enum FaceRun {
    /// The installed `deadpan-track` worker with Apple Vision.
    #[cfg_attr(not(feature = "ui-harness"), allow(dead_code, reason = "replay only"))]
    Worker,
    /// Report these faces after `delay`, through the host's admission.
    Faces {
        faces: Vec<DetectedFace>,
        delay: std::time::Duration,
    },
}

#[cfg(any(test, feature = "ui-harness"))]
impl FaceScript {
    pub fn new(runs: impl IntoIterator<Item = FaceRun>) -> Self {
        Self(std::sync::Mutex::new(runs.into_iter().collect()))
    }

    pub fn next(&self) -> Option<FaceRun> {
        let mut runs = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if runs.len() > 1 {
            runs.pop_front()
        } else {
            runs.front().cloned()
        }
    }
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
    /// Report this tracker error instead of starting.
    pub unavailable: Option<String>,
    /// Progress reports before the ending, `step_interval` apart.
    pub steps: u8,
    pub step_interval: std::time::Duration,
    pub ending: ScriptEnding,
}

#[cfg(any(test, feature = "ui-harness"))]
#[derive(Clone, Debug)]
pub enum ScriptEnding {
    /// Confident observations moving the rectangle right by this fraction of
    /// the picture width per analysed picture.
    Moving { step: f64 },
    /// Keep reporting the last step until cancelled.
    #[cfg_attr(not(test), allow(dead_code, reason = "service tests only"))]
    WaitForCancel,
    /// Conclude with this worker failure.
    #[cfg_attr(not(test), allow(dead_code, reason = "service tests only"))]
    Fail(String),
}

/// The engine recorded in a scripted target's provenance.
#[cfg(any(test, feature = "ui-harness"))]
pub const SCRIPTED_ENGINE: &str = "Scripted replay tracker";
