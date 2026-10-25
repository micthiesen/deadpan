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
//! Picture proposals run on a second bounded job thread the same way: one
//! indexed picture, the supervised `deadpan-track picture-proposals` worker, and
//! admitted faces and salient regions. Nothing is saved until the person
//! accepts one. Camera saves the chosen target independently; the explicit
//! `:zoom … target=face:N` or `target=region:N` saves it with framing in one edit.

#[cfg(any(test, feature = "ui-harness"))]
use std::sync::Arc;
use std::time::Instant;

use deadpan_cli::faces::DetectedFace;
use deadpan_cli::picture_proposals::{
    DetectedSalientRegion, DetectorRuntime, PictureProposals, ProposalSelection,
};
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
    /// Delete one target nothing references, as one undoable edit expecting
    /// `revision`.
    Delete {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        id: TargetId,
    },
    /// Cancel the job started with ticket `job`.
    Cancel { ticket: u64, session: u64, job: u64 },
    /// Analyse one indexed picture without editing. The selected detectors
    /// share one verified input and report one complete proposal batch.
    DetectPictureProposals {
        ticket: u64,
        session: u64,
        revision: RevisionId,
        asset: AssetId,
        pts: i64,
        selection: ProposalSelection,
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

/// The latest picture analysis of this session: proposals only.
#[derive(Clone, Debug)]
pub struct ProposalJob {
    pub ticket: u64,
    pub session: u64,
    /// The head revision the picture was resolved against.
    pub revision: RevisionId,
    pub asset: AssetId,
    /// The indexed picture analysed.
    pub pts: i64,
    pub selection: ProposalSelection,
    /// None while detection runs.
    pub outcome: Option<ProposalOutcome>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProposalOutcome {
    /// Complete admitted collections, each ordered geometrically.
    Found(ProposedTargets),
    /// The detector or Original is missing; nothing was proposed.
    Unavailable(String),
    Failed(String),
    Cancelled,
}

/// Explicitly distinguish deterministic replay data from worker provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalOrigin {
    Worker {
        faces: Option<DetectorRuntime>,
        salient_regions: Option<DetectorRuntime>,
    },
    #[cfg(any(test, feature = "ui-harness"))]
    Scripted,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProposedTargets {
    pub faces: Option<Vec<DetectedFace>>,
    pub salient_regions: Option<Vec<DetectedSalientRegion>>,
    pub origin: ProposalOrigin,
}

impl ProposedTargets {
    pub fn from_worker(proposals: PictureProposals) -> Self {
        let (faces, face_runtime) = proposals.faces.map_or((None, None), |found| {
            (Some(found.faces), Some(found.runtime))
        });
        let (salient_regions, region_runtime) =
            proposals.salient_regions.map_or((None, None), |found| {
                (Some(found.regions), Some(found.runtime))
            });
        Self {
            faces,
            salient_regions,
            origin: ProposalOrigin::Worker {
                faces: face_runtime,
                salient_regions: region_runtime,
            },
        }
    }
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
    pub proposals: Option<ProposalJob>,
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
    /// Both mode and geometry are admitted normally. These results carry
    /// explicit scripted origin and invent no Vision runtime or revision.
    #[cfg(any(test, feature = "ui-harness"))]
    ScriptedProposals(Arc<ProposalScript>),
}

#[cfg(any(test, feature = "ui-harness"))]
#[derive(Debug)]
pub struct ProposalScript(std::sync::Mutex<std::collections::VecDeque<ProposalRun>>);

#[cfg(any(test, feature = "ui-harness"))]
#[derive(Clone, Debug)]
pub enum ProposalRun {
    #[cfg(feature = "ui-harness")]
    Worker,
    Proposals {
        selection: ProposalSelection,
        faces: Option<Vec<DetectedFace>>,
        salient_regions: Option<Vec<DetectedSalientRegion>>,
        delay: std::time::Duration,
    },
    Failed {
        reason: String,
        delay: std::time::Duration,
    },
}

#[cfg(any(test, feature = "ui-harness"))]
impl ProposalScript {
    pub fn new(runs: impl IntoIterator<Item = ProposalRun>) -> Self {
        Self(std::sync::Mutex::new(runs.into_iter().collect()))
    }

    pub fn next(&self) -> Option<ProposalRun> {
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

/// Labels of the beats whose framing follows `id`, directly or through a
/// Repeat's escalating zoom, in node order. Deleting a used target would
/// leave that framing invalid.
pub fn users(document: &deadpan_core::ProjectDocument, id: &TargetId) -> Vec<String> {
    document
        .nodes()
        .iter()
        .filter(|(_, node)| {
            matches!(
                &node.framing,
                Some(deadpan_core::Framing {
                    value: deadpan_core::FramingValue::Follow { target, .. },
                    ..
                }) if target == id
            ) || matches!(
                &node.kind,
                deadpan_core::NodeKind::Repeat {
                    escalation: Some(deadpan_core::RepeatEscalation {
                        zoom: Some(deadpan_core::ZoomStep { target: Some(target), .. }),
                        ..
                    }),
                    ..
                } if target == id
            )
        })
        .map(|(node, beat)| {
            if beat.label.is_empty() {
                node.as_str().to_owned()
            } else {
                beat.label.clone()
            }
        })
        .collect()
}
