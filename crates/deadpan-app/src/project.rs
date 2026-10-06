//! Single-writer project service and bounded background source preparation.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use deadpan_core::{
    AssetId, AudioEdgePolicy, AudioSample, FrameDuration, FrameRange, HoldAudio, NodeId,
    ProjectDocument, ProjectFrame, ProjectId, RevisionId, SoundId, SourceAudio, SourceFrameIndex,
    SourceQualificationId,
};
pub use deadpan_media::audio_index::AudioLayoutInterpretation;
use deadpan_plan::RenderPlan;
use deadpan_store::original_media::{OriginalImportHandle, OriginalMediaRecord, OriginalOwnership};
use deadpan_store::single_source::SingleSourceState;
use deadpan_store::source_registration::SourceQualificationReceipt;

use crate::library::ProjectLibrary;

pub mod gain;
pub mod generation;
pub mod macros;
pub mod marks;
mod pause;
pub mod registers;
pub mod render_history;
pub mod retime;
mod scope;
pub mod scoped;
pub mod semantic;
mod service;
pub mod slice;
pub mod slip;
pub mod sound;
pub mod splice;
pub mod targets;
#[cfg(test)]
mod tests;
pub mod trim;
mod worker;

pub use scope::SequenceScope;

pub struct RegisteredSource {
    pub asset: AssetId,
    pub label: String,
    pub receipt: Arc<SourceQualificationReceipt>,
    pub original: OriginalMediaRecord,
    pub video_index: Option<Arc<SourceFrameIndex>>,
    /// Prepared off the UI thread, including the complete measured A/V union.
    pub original_audition: Option<Arc<deadpan_playback::Original>>,
    /// Audio-only catalog clock, prepared off the UI thread from measured spans.
    pub sound_audition: Option<Arc<deadpan_playback::Sound>>,
}

/// A stored transcript of the Original, carried across edits.
#[derive(Debug)]
pub struct OriginalTranscript {
    pub key: deadpan_store::TranscriptKey,
    /// The words as people corrected them: what display, search, motions and
    /// objects use.
    pub transcript: deadpan_analysis::Transcript,
    /// The recognizer's rebuildable proposal.
    pub proposal: Arc<deadpan_analysis::Transcript>,
    /// Where each corrected word came from.
    pub origins: Vec<deadpan_analysis::WordOrigin>,
    /// Recognized words replaced by corrections.
    pub replaced: usize,
    /// Correction regions that do not apply to this transcript.
    pub skipped: usize,
    /// Why stored corrections could not be read or applied, if they could not.
    pub corrections_error: Option<String>,
}

impl OriginalTranscript {
    pub fn new(words: deadpan_cli::speech::StoredWords) -> Self {
        Self {
            key: words.key,
            transcript: words.corrected.transcript,
            proposal: Arc::new(words.proposal),
            origins: words.corrected.origins,
            replaced: words.corrected.replaced,
            skipped: words.corrected.skipped,
            corrections_error: words.corrections_error,
        }
    }

    /// Why word operators must refuse: corrections that failed or do not
    /// apply are never silently replaced by the recognizer's words.
    pub fn problem(&self) -> Option<String> {
        deadpan_cli::speech::correction_problem(
            "words",
            self.corrections_error.as_deref(),
            self.skipped,
        )
    }

    /// Whether a word was set by a person.
    pub fn corrected(&self, word: usize) -> bool {
        matches!(
            self.origins.get(word),
            Some(deadpan_analysis::WordOrigin::Corrected(_))
        )
    }

    /// The corrected words with their origins, for building a correction.
    pub fn current(&self) -> deadpan_analysis::CorrectedTranscript {
        deadpan_analysis::CorrectedTranscript {
            transcript: self.transcript.clone(),
            origins: self.origins.clone(),
            replaced: self.replaced,
            skipped: self.skipped,
        }
    }
}

/// Stored speech activity of the Original, carried across edits.
#[derive(Debug)]
pub struct OriginalActivity {
    pub key: deadpan_store::SpeechActivityKey,
    pub activity: deadpan_analysis::SpeechActivity,
    /// Detected pauses as people corrected them.
    pub pauses: deadpan_analysis::CorrectedPauses,
    /// Why stored corrections could not be read, if they could not.
    pub corrections_error: Option<String>,
}

impl OriginalActivity {
    /// Why pause operators must refuse; see [`OriginalTranscript::problem`].
    pub fn problem(&self) -> Option<String> {
        deadpan_cli::speech::correction_problem(
            "pauses",
            self.corrections_error.as_deref(),
            self.pauses.skipped,
        )
    }
}

/// One correction change. The analyses the person saw are carried by
/// identity, so the service refuses a change computed against a transcript or
/// detection that has since been replaced.
#[derive(Debug, Clone)]
pub struct CorrectionRequest {
    pub expected_session: u64,
    pub attempt: u64,
    pub key: deadpan_store::CorrectionsKey,
    pub expected_version: u64,
    pub change: deadpan_store::CorrectionChange,
    pub transcript: Option<Arc<OriginalTranscript>>,
    pub activity: Option<Arc<OriginalActivity>>,
}

/// A person's corrections of the Original's transcript and pauses, stored
/// apart from the rebuildable analyses with their own Undo and Redo.
#[derive(Debug)]
pub struct OriginalCorrections {
    pub key: deadpan_store::CorrectionsKey,
    /// None until the first correction.
    pub stored: Option<deadpan_store::StoredCorrections>,
    /// Why stored corrections could not be read, if they could not.
    pub error: Option<String>,
}

impl OriginalCorrections {
    pub fn version(&self) -> u64 {
        self.stored.as_ref().map_or(0, |stored| stored.version)
    }
}

/// Stored shot analysis of the Original's pictures, carried across edits.
#[derive(Debug)]
pub struct OriginalShots {
    pub key: deadpan_store::ShotAnalysisKey,
    pub analysis: deadpan_analysis::ShotAnalysis,
}

/// The outcome of one background transcript save, matched by attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptSave {
    pub session: u64,
    pub attempt: u64,
    pub error: Option<String>,
}

pub struct Workspace {
    pub session: u64,
    pub path: PathBuf,
    pub document: Arc<ProjectDocument>,
    pub plan: Arc<RenderPlan>,
    pub sources: BTreeMap<AssetId, Arc<RegisteredSource>>,
    pub originals: OriginalImportHandle,
    pub generated: deadpan_store::generated_media::GeneratedReadHandle,
    pub can_undo: bool,
    pub can_redo: bool,
    /// None identifies a preserved generic/legacy project.
    pub single_source: Option<SingleSourceState>,
    /// Full measured Original stream union in the current project-frame clock.
    /// This is not the video's decoded presentation-frame count.
    pub original_duration: Option<FrameDuration>,
    /// Loaded when the project opens and replaced when a transcript is saved;
    /// annotations never change with document revisions.
    pub transcript: Option<Arc<OriginalTranscript>>,
    /// Loaded when the project opens and replaced when speech activity is
    /// saved; like the transcript, it never changes with document revisions.
    pub speech_activity: Option<Arc<OriginalActivity>>,
    /// Loaded when the project opens and replaced when shot analysis is
    /// saved; like the other analyses it never changes with revisions.
    pub shot_analysis: Option<Arc<OriginalShots>>,
    /// Manual corrections of the transcript and pauses, loaded when the
    /// project opens and replaced when they change; never revisions.
    pub corrections: Option<Arc<OriginalCorrections>>,
    /// Receipt-derived automatic SDR/HDR branch of this committed revision.
    pub color: deadpan_cli::picture::OutputColorDecision,
}

impl Workspace {
    /// The committed revision's automatic SDR/HDR branch, derived by the
    /// project service from the store's receipts for every registered
    /// picture asset (loaded or not) exactly as export derives it.
    pub fn color_decision(&self) -> deadpan_cli::picture::OutputColorDecision {
        self.color
    }

    /// Viewing-condition label for an HDR-bearing project, None for SDR-only
    /// projects.
    pub fn color_label(&self) -> Option<&'static str> {
        use deadpan_cli::picture::ColorDecisionPipeline;
        self.color
            .hdr_sources
            .then(|| self.color.pipeline().preview_label(true))
    }

    /// The same committed workspace with a newly saved transcript.
    pub fn with_transcript(&self, transcript: Arc<OriginalTranscript>) -> Self {
        Self {
            transcript: Some(transcript),
            ..self.annotated()
        }
    }

    /// The same committed workspace with newly saved speech activity.
    pub fn with_speech_activity(&self, activity: Arc<OriginalActivity>) -> Self {
        Self {
            speech_activity: Some(activity),
            ..self.annotated()
        }
    }

    /// The same committed workspace with a newly saved shot analysis.
    pub fn with_shot_analysis(&self, shots: Arc<OriginalShots>) -> Self {
        Self {
            shot_analysis: Some(shots),
            ..self.annotated()
        }
    }

    /// The same committed workspace with changed corrections and the
    /// analyses they correct.
    pub fn with_corrections(
        &self,
        corrections: Arc<OriginalCorrections>,
        transcript: Option<Arc<OriginalTranscript>>,
        speech_activity: Option<Arc<OriginalActivity>>,
    ) -> Self {
        Self {
            corrections: Some(corrections),
            transcript,
            speech_activity,
            ..self.annotated()
        }
    }

    /// A copy of this committed workspace, for replacing one annotation.
    fn annotated(&self) -> Self {
        // Exhaustive destructuring: a new field must be carried here explicitly.
        let Self {
            session,
            path,
            document,
            plan,
            sources,
            originals,
            generated,
            can_undo,
            can_redo,
            single_source,
            original_duration,
            transcript,
            speech_activity,
            shot_analysis,
            corrections,
            color,
        } = self;
        Self {
            color: *color,
            session: *session,
            path: path.clone(),
            document: Arc::clone(document),
            plan: Arc::clone(plan),
            sources: sources.clone(),
            originals: originals.clone(),
            generated: generated.clone(),
            can_undo: *can_undo,
            can_redo: *can_redo,
            single_source: single_source.clone(),
            original_duration: *original_duration,
            transcript: transcript.clone(),
            speech_activity: speech_activity.clone(),
            shot_analysis: shot_analysis.clone(),
            corrections: corrections.clone(),
        }
    }

    /// Media capabilities stay anchored in this committed workspace, even when
    /// an independently validated gain proposal uses them for audition.
    pub fn playback_snapshot(&self) -> deadpan_playback::Snapshot {
        deadpan_playback::Snapshot::committed(
            self.session,
            self.document.clone(),
            self.sources
                .iter()
                .map(|(asset, source)| {
                    (
                        asset.clone(),
                        deadpan_playback::SourceEntry {
                            receipt: source.receipt.clone(),
                            original: source.original.clone(),
                        },
                    )
                })
                .collect(),
            self.originals.clone(),
        )
    }
}

/// `interpretation` is the person's explicit speaker reading of a sound
/// whose channels declare no layout; such a sound is refused without it.
#[derive(Clone, Copy, Debug)]
pub enum ImportMedia {
    Video,
    FirstAudio {
        interpretation: Option<AudioLayoutInterpretation>,
    },
    Audio {
        stream: u32,
        interpretation: Option<AudioLayoutInterpretation>,
    },
}

impl ImportMedia {
    pub fn sound(stream: Option<u32>, interpretation: Option<AudioLayoutInterpretation>) -> Self {
        stream.map_or(Self::FirstAudio { interpretation }, |stream| Self::Audio {
            stream,
            interpretation,
        })
    }

    pub fn interpretation(self) -> Option<AudioLayoutInterpretation> {
        match self {
            Self::Video => None,
            Self::FirstAudio { interpretation } | Self::Audio { interpretation, .. } => {
                interpretation
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportStage {
    Retaining,
    Decoding,
    PreparingInsertion,
    Registering,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ImportStatus {
    pub path: PathBuf,
    pub stage: ImportStage,
    pub error: Option<String>,
    pub asset: Option<AssetId>,
}

pub struct ProjectUpdate {
    pub workspace: Option<Arc<Workspace>>,
    pub import: Option<ImportStatus>,
    pub error: Option<String>,
    pub message: Option<String>,
    /// Last selection-changing commit, retained through background progress.
    /// Cleared by the next user command; consumers deduplicate by revision.
    pub committed: Option<CommittedEdit>,
    /// Last semantic edit follows durable head transitions, including failed refreshes.
    pub semantic: Option<semantic::Snapshot>,
    pub macros: Option<macros::Update>,
    /// Successful macro receipt survives later queries and rejected attempts.
    pub saved_macro: Option<macros::Receipt>,
    /// Read-only source-range preparation, bound to the captured request.
    /// Consumers must admit its ticket, session and revision before using it.
    pub room_tone: Option<PreparedRoomTone>,
    /// A preparation rejection carries the request context, including stale
    /// contexts. Generic service errors cannot complete a range request.
    pub room_tone_error: Option<RoomToneFailure>,
    /// Both success and failure retain the exact uncommitted proposal identity.
    pub gain: Option<gain::ProposalUpdate>,
    /// Read-only linked Original proposals have independent, identity-tagged replies.
    pub splice: Option<splice::ProposalUpdate>,
    /// Exact placement commit acknowledgements survive proposal and query traffic.
    pub splice_commit: Option<splice::SpliceCommitUpdate>,
    /// Qualified Slip proposals retain exact target and draft/change identity.
    pub slip: Option<slip::ProposalUpdate>,
    pub slip_commit: Option<slip::CommitUpdate>,
    /// Last saved Slip in this session, independent of later failed requests.
    pub saved_slip: Option<slip::CommitReceipt>,
    /// Ordered Trim acknowledgment is separate from current proposal admission.
    pub trim: Option<trim::ProposalUpdate>,
    pub trim_commit: Option<trim::CommitUpdate>,
    /// Durable Trim success survives later failures and preview refresh errors.
    pub saved_trim: Option<trim::CommitReceipt>,
    /// History-neutral edited copies retain their source identity across replies.
    pub captured_slice: Option<slice::CaptureUpdate>,
    /// Durable contents are independent of matching copy feedback or selection.
    pub registers: Option<Arc<registers::Bank>>,
    pub captured_original: Option<registers::OriginalUpdate>,
    pub cut_slice: Option<slice::CutUpdate>,
    /// Last durable cut remains visible even after a newer cut is rejected.
    pub saved_cut: Option<slice::CutReceipt>,
    /// Mark metadata saves and navigation queries never retarget a selection.
    pub marks: marks::Update,
    /// Operational render feedback is retained independently of editor feedback.
    pub render: Option<ProjectRenderUpdate>,
    /// Bounded history replies retain their exact query and session independently
    /// of authored commits and live render progress.
    pub render_history: Option<render_history::Update>,
    /// Background transcript saves report here, never through the editor's
    /// command error, which the next dispatch clears.
    pub transcript_save: Option<TranscriptSave>,
    /// Background speech activity saves, reported like transcript saves.
    pub activity_save: Option<TranscriptSave>,
    /// Background shot analysis saves, reported like transcript saves.
    pub shot_save: Option<TranscriptSave>,
    /// Transcript and pause correction saves, matched by attempt.
    pub correction_save: Option<TranscriptSave>,
    /// AI pause jobs and Ready candidates, independent of editor feedback.
    pub generation: Option<generation::Update>,
    /// Target saves and tracking, independent of editor feedback.
    pub targets: Option<targets::Update>,
    /// What opening this session recovered and which originals are missing.
    /// Retained for the session; replaced after a relink.
    pub opened: Option<Arc<OpenReport>>,
    /// Persistent "Not saved" state after a disk-full or permission failure,
    /// independent of the next command's error.
    pub storage: Option<crate::recovery::StorageAlert>,
    /// The latest Original relink, matched by its request ticket.
    pub relink: Option<RelinkStatus>,
    /// The latest explicit storage cleanup, matched by its request ticket.
    pub storage_cleanup: Option<StorageCleanupStatus>,
}

/// The outcome of one explicit `:storage` cleanup request. Operational; it
/// never changes authored history.
#[derive(Clone, Debug)]
pub struct StorageCleanupStatus {
    pub ticket: u64,
    pub session: u64,
    pub result: Result<deadpan_store::storage::CleanupOutcome, String>,
}

/// What a writable open recovered, plus the presence of every registered
/// original's bytes. Operational; nothing here is authored history.
#[derive(Debug)]
pub struct OpenReport {
    pub session: u64,
    pub path: PathBuf,
    pub recovery: deadpan_store::recovery::OpenRecovery,
    pub originals: Vec<OriginalStatus>,
}

impl OpenReport {
    pub fn missing(&self) -> impl Iterator<Item = &OriginalStatus> {
        self.originals.iter().filter(|status| {
            !matches!(
                status.availability,
                deadpan_store::original_media::OriginalAvailability::Present
            )
        })
    }

    /// Something the person should hear about on opening.
    pub fn needs_attention(&self) -> bool {
        !self.recovery.is_clean() || self.missing().next().is_some()
    }
}

#[derive(Debug, Clone)]
pub struct OriginalStatus {
    pub label: String,
    /// The single-original project's Original, as opposed to a sound.
    pub primary: bool,
    pub record: OriginalMediaRecord,
    pub availability: deadpan_store::original_media::OriginalAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelinkStatus {
    pub ticket: u64,
    pub session: u64,
    pub label: String,
    pub state: RelinkState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelinkState {
    Verifying,
    /// The verified file now supplies the original's bytes.
    Restored,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRenderContext {
    pub session: u64,
    pub project: ProjectId,
}

/// Internal host configuration. These are not user-facing codec controls.
#[derive(Clone, Debug)]
pub struct ProjectRenderLimits {
    pub encode: deadpan_cli::encoded_render::EncodedWorkerLimits,
    pub verification: deadpan_cli::encoded_render::verification::VerificationLimits,
    pub media: deadpan_store::render_media::RenderMediaLimits,
}

pub enum ProjectRenderOperation {
    /// Resolve captured historical identities on the store owner, then use the
    /// ordinary Retry or Reconcile workflow with fresh operation identities.
    Recover(render_history::Recovery),
    Start {
        request: deadpan_cli::encoded_render::workflow::StartRender,
        limits: ProjectRenderLimits,
    },
    /// Commit the captured Camera, Gain or Hold audio preview, then render its
    /// exact receipt before another command can run. request.revision names the
    /// pre-edit revision; only a successful edit receipt may replace it.
    CommitAndStart {
        edit: Box<ProjectEdit>,
        cursor: ProjectFrame,
        scope: SequenceScope,
        request: deadpan_cli::encoded_render::workflow::StartRender,
        limits: ProjectRenderLimits,
    },
    Retry {
        request: deadpan_cli::encoded_render::workflow::RetryRender,
        limits: ProjectRenderLimits,
    },
    Reconcile {
        request: deadpan_cli::encoded_render::workflow::ReconcileRender,
        limits: ProjectRenderLimits,
    },
    Cancel(deadpan_cli::encoded_render::workflow::WorkflowIdentity),
}

pub struct ProjectRenderRequest {
    pub ticket: u64,
    pub context: ProjectRenderContext,
    pub operation: ProjectRenderOperation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRenderError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ProjectRenderCommandOutcome {
    pub ticket: u64,
    pub context: ProjectRenderContext,
    /// Exact durable preview commit, even if refresh or render admission failed.
    /// None means this command did not acknowledge an authored commit.
    pub committed_revision: Option<RevisionId>,
    pub result: Result<deadpan_cli::encoded_render::workflow::WorkflowIdentity, ProjectRenderError>,
}

#[derive(Clone, Debug)]
pub struct ProjectRenderStatus {
    pub context: ProjectRenderContext,
    pub revision: RevisionId,
    pub status: Arc<deadpan_cli::encoded_render::workflow::WorkflowStatus>,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectRenderUpdate {
    pub command: Option<ProjectRenderCommandOutcome>,
    pub workflow: Option<ProjectRenderStatus>,
    pub service_error: Option<ProjectRenderError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomToneSelection {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        ordinals: std::ops::Range<u64>,
    },
    Exact {
        source: SourceAudio,
        qualification: SourceQualificationId,
    },
}

#[derive(Clone)]
pub struct PreparedRoomTone {
    pub ticket: u64,
    pub session: u64,
    pub revision: RevisionId,
    pub source: SourceAudio,
    pub audition: Arc<deadpan_playback::AudioRange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomToneFailure {
    pub ticket: u64,
    pub session: u64,
    pub revision: RevisionId,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedEdit {
    pub revision: RevisionId,
    /// None explicitly clears selection when deletion leaves an empty Sequence.
    pub selected_node: Option<NodeId>,
    /// Spatial edits keep the stopped position instead of jumping to the beat start.
    pub preserve_cursor: bool,
    /// Explicit committed boundary for cursor-based edits. The UI may have
    /// navigated since submission; never infer this from its current cursor.
    pub cursor: Option<ProjectFrame>,
    /// Navigation scope captured when this edit was requested. Async completions
    /// must not restore a scope inferred from the current UI state.
    pub scope: SequenceScope,
    /// Sound commits preserve the editor's picture, beat and navigation context.
    pub sound: Option<SoundCommit>,
    /// Exact result selection retained until its matching workspace is visible.
    pub range_selection: Option<CommittedRangeSelection>,
    /// Exact branch identity before and after a scoped value edit.
    pub scoped: Option<scoped::Commit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedRangeSelection {
    pub session: u64,
    pub project: ProjectId,
    pub parent: NodeId,
    pub range: FrameRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundCommit {
    pub selected: Option<SoundId>,
}

#[derive(Clone, Debug)]
pub enum ProjectSoundEdit {
    Place {
        asset: AssetId,
        at: AudioSample,
    },
    Update {
        id: SoundId,
        gain_millidecibels: i32,
        start_edge: AudioEdgePolicy,
        end_edge: AudioEdgePolicy,
    },
    Move {
        id: SoundId,
        at: AudioSample,
    },
    /// Relative motion in exact project frames, without rounding each delta.
    Nudge {
        id: SoundId,
        frames: i64,
    },
    Allowance {
        id: SoundId,
        issuer: deadpan_core::SoundHoldIssuer,
        at: ProjectFrame,
        allowed: bool,
    },
    /// End the sound at this Edit frame boundary with a hard edge.
    Cut {
        id: SoundId,
        at: ProjectFrame,
    },
    Delete {
        id: SoundId,
    },
}

/// Ordinary node operations edit direct children of the captured Sequence scope.
/// Scoped value edits carry their independently validated branch target.
/// InsertTime remains project-boundary based and resolves its actual owner.
#[derive(Clone, Debug)]
pub enum ProjectEdit {
    Scoped {
        target: Box<scoped::Target>,
        edit: deadpan_core::ScopedNodeEdit,
    },
    SetAudioTreatments {
        node: NodeId,
        treatments: deadpan_core::AudioTreatments,
    },
    SetFraming {
        node: NodeId,
        framing: Option<deadpan_core::Framing>,
    },
    /// The native `,h` and `:hold` commit through the semantic InsertPause
    /// instruction; service tests keep exercising this direct form.
    #[cfg_attr(not(test), allow(dead_code, reason = "service tests only"))]
    InsertTime {
        at: ProjectFrame,
        duration: FrameDuration,
    },
    /// A silent pause with black picture (black-frame punctuation).
    #[cfg_attr(not(test), allow(dead_code, reason = "service tests only"))]
    InsertBlack {
        at: ProjectFrame,
        duration: FrameDuration,
    },
    Split {
        node: NodeId,
        /// Interior boundary in this selected beat's project-frame clock.
        at: FrameDuration,
    },
    Repeat {
        node: NodeId,
        plays: u32,
    },
    WrapRepeat {
        node: NodeId,
        plays: u32,
    },
    /// Replace a beat's picture-only cutaways without changing timing.
    SetCutaways {
        /// The selected beat in the current Sequence.
        node: NodeId,
        /// The Source or Hold under it, through unity Partitions.
        host: NodeId,
        cutaways: Vec<deadpan_core::Cutaway>,
    },
    /// Replace a beat's captions without changing timing or pictures.
    SetCaptions {
        node: NodeId,
        host: NodeId,
        captions: Vec<deadpan_core::Caption>,
    },
    Delete {
        node: NodeId,
    },
    /// Linked half-open project time inside the captured ordinary Sequence.
    DeleteRange {
        parent: NodeId,
        range: deadpan_core::FrameRange,
    },
    /// The native `:hold-duration` and `:retime`/`:pitch` (and `:audio-lag`,
    /// which has no direct form any more) commit as semantic instructions;
    /// service tests keep these direct forms as fixtures.
    #[cfg_attr(not(test), allow(dead_code, reason = "service tests only"))]
    HoldDuration {
        node: NodeId,
        duration: FrameDuration,
    },
    HoldAudio {
        node: NodeId,
        audio: HoldAudio,
    },
    #[cfg_attr(not(test), allow(dead_code, reason = "service tests only"))]
    Retime {
        node: NodeId,
        speed: deadpan_core::ExactRatio,
        pitch: deadpan_core::PitchPolicy,
        wrap: bool,
    },
}

pub struct MomentPaste {
    pub expected_session: u64,
    pub expected_revision: RevisionId,
    pub asset: AssetId,
    pub qualification: deadpan_core::SourceQualificationId,
    pub ordinals: std::ops::Range<u64>,
    pub scope: SequenceScope,
    pub parent: NodeId,
    pub destination: splice::Destination,
}

impl ProjectRequest {
    /// Background analysis saves use their own admission lane so they never
    /// occupy the single user-command slot. Each lane admits one request.
    pub fn is_annotation_save(&self) -> bool {
        matches!(
            self,
            Self::SaveTranscript { .. }
                | Self::SaveSpeechActivity { .. }
                | Self::SaveShotAnalysis { .. }
                | Self::SaveShotProgress { .. }
        )
    }
}

pub enum ProjectRequest {
    /// Store a validated transcript of the Original; never an edit.
    SaveTranscript {
        expected_session: u64,
        attempt: u64,
        key: deadpan_store::TranscriptKey,
        transcript: Arc<deadpan_analysis::Transcript>,
    },
    /// Store validated speech activity of the Original; never an edit.
    SaveSpeechActivity {
        expected_session: u64,
        attempt: u64,
        key: deadpan_store::SpeechActivityKey,
        activity: Arc<deadpan_analysis::SpeechActivity>,
    },
    /// Store a validated shot analysis of the Original; never an edit.
    SaveShotAnalysis {
        expected_session: u64,
        attempt: u64,
        key: deadpan_store::ShotAnalysisKey,
        analysis: Arc<deadpan_analysis::ShotAnalysis>,
    },
    /// Append a shot scan checkpoint's changed measures to the Original's
    /// saved progress, best effort; never an edit and no receipt.
    SaveShotProgress {
        expected_session: u64,
        key: deadpan_store::ShotAnalysisKey,
        progress: Arc<deadpan_analysis::ShotProgressTail>,
    },
    /// Apply, undo or redo a correction of the Original's transcript or
    /// pauses. Corrections live outside document history.
    ChangeCorrections(CorrectionRequest),
    Marks(marks::Request),
    /// AI pause generation. Only Accept edits the project.
    Generation(generation::GenerationOperation),
    /// Target saves and background tracking.
    Target(targets::Operation),
    Render(ProjectRenderRequest),
    RenderHistory(render_history::Request),
    /// Source first: native projects are always allocated in Documents/Deadpan.
    CreateFromSource {
        path: PathBuf,
    },
    InitializeSource {
        expected_session: u64,
        expected_revision: RevisionId,
        path: PathBuf,
    },
    /// Register an audio-only catalog entry, not a timeline or overlay insertion.
    /// None selects the first actual audio stream through guarded admission.
    /// Some selects an explicit absolute container stream index.
    ImportSound {
        expected_session: u64,
        expected_revision: RevisionId,
        path: PathBuf,
        stream: Option<u32>,
        /// Explicit speaker reading for channels without a declared layout.
        interpretation: Option<AudioLayoutInterpretation>,
        ownership: OriginalOwnership,
    },
    /// Retained for generic host fixtures and compatibility; native New uses
    /// CreateFromSource and never asks for an arbitrary package location.
    #[cfg(test)]
    Create(PathBuf),
    Open(PathBuf),
    Close,
    /// The person has seen this session's recovery report; the store may
    /// forget its retained findings. Never an edit.
    AcknowledgeRecovery {
        expected_session: u64,
    },
    /// Locate a missing original: a managed copy is restored from the chosen
    /// file and a linked one is relinked to it. Either way the file must have
    /// exactly the registered content.
    RelinkOriginal {
        ticket: u64,
        expected_session: u64,
        content: deadpan_store::original_media::OriginalContentId,
        expected_version: u64,
        path: PathBuf,
    },
    /// Remove exactly the previewed entries that a fresh scan on the writer
    /// still finds unreferenced and older than the default grace period.
    /// The preview itself runs off the writer on a read-only open. Refused
    /// while any import, relink, render, AI pause or tracking job could be
    /// writing media.
    CleanStorage {
        ticket: u64,
        expected_session: u64,
        previewed: Vec<deadpan_store::storage::RemovedEntry>,
    },
    Import {
        path: PathBuf,
        media: ImportMedia,
        ownership: OriginalOwnership,
    },
    CancelImport,
    Insert {
        expected_session: u64,
        expected_revision: RevisionId,
        asset: AssetId,
        scope: SequenceScope,
        parent: NodeId,
        index: usize,
    },
    PasteMoment(MomentPaste),
    CaptureOriginal(registers::OriginalRequest),
    CaptureEditSlice(slice::CaptureRequest),
    CutEditSlice(slice::CaptureRequest),
    CutFrames {
        capture: slice::CaptureRequest,
        attempt: semantic::CutAttempt,
    },
    Macro(macros::Operation),
    PasteEditedSlice(slice::Paste),
    PrepareSplice(splice::Proposal),
    CommitSplice(splice::ProposalId),
    AbandonSplice(splice::ProposalId),
    PrepareSlip(slip::Proposal),
    CommitSlip(slip::ProposalId),
    AbandonSlip(slip::ProposalId),
    PrepareTrim(trim::Proposal),
    CommitTrim(trim::ProposalId),
    AbandonTrim(trim::ProposalId),
    /// Resolve source samples and prepare an audition descriptor off the UI.
    /// This does not select a Hold, author a policy, or create history.
    PrepareRoomTone {
        expected_session: u64,
        expected_revision: RevisionId,
        ticket: u64,
        selection: RoomToneSelection,
    },
    PrepareGain(gain::Proposal),
    Edit {
        expected_session: u64,
        expected_revision: RevisionId,
        cursor: ProjectFrame,
        scope: SequenceScope,
        edit: ProjectEdit,
    },
    SoundEdit {
        expected_session: u64,
        expected_revision: RevisionId,
        edit: ProjectSoundEdit,
    },
    Undo {
        expected_revision: RevisionId,
    },
    Redo {
        expected_revision: RevisionId,
    },
}

/// One user command and one annotation save may be queued at once.
const REQUEST_LANES: usize = 2;

struct Shared {
    /// The single user-command slot.
    busy: AtomicBool,
    /// The single background annotation-save slot, queued independently.
    annotation: AtomicBool,
    /// Deterministic replay/tests: leave admitted requests in the mailbox.
    #[cfg(any(test, feature = "ui-harness"))]
    requests_held: AtomicBool,
    preview_active: AtomicBool,
    stopping: AtomicBool,
    shutdown_complete: AtomicBool,
    #[cfg(test)]
    render_poll_paused: AtomicBool,
    #[cfg(test)]
    render_commit_refresh_failure: AtomicBool,
    #[cfg(test)]
    host_refresh_failure: AtomicBool,
    #[cfg(test)]
    splice_commit_refresh_failure: AtomicBool,
    #[cfg(test)]
    slip_commit_refresh_failure: AtomicBool,
    #[cfg(test)]
    trim_commit_refresh_failure: AtomicBool,
    #[cfg(test)]
    workspace_refresh_failure: AtomicBool,
    /// Replay/tests: refuse the next authored edit as a full disk would.
    #[cfg(any(test, feature = "ui-harness"))]
    storage_failure: AtomicBool,
    update: Mutex<Option<ProjectUpdate>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// The app's background job coordinator, shared with the UI's jobs.
    job_board: crate::jobs::Jobs,
}

pub struct ProjectService {
    requests: mpsc::SyncSender<ProjectRequest>,
    shared: Arc<Shared>,
}

impl ProjectService {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> io::Result<Self> {
        Self::start(wake, None)
    }

    pub(crate) fn start(
        wake: Arc<dyn Fn() + Send + Sync>,
        library: Option<ProjectLibrary>,
    ) -> io::Result<Self> {
        Self::start_with(wake, library, generation::Backend::Environment)
    }

    /// Production always uses [`generation::Backend::Environment`].
    pub(crate) fn start_with(
        wake: Arc<dyn Fn() + Send + Sync>,
        library: Option<ProjectLibrary>,
        backend: generation::Backend,
    ) -> io::Result<Self> {
        Self::start_with_tracking(wake, library, backend, targets::Backend::Environment)
    }

    /// Production always uses both `Environment` backends.
    pub(crate) fn start_with_tracking(
        wake: Arc<dyn Fn() + Send + Sync>,
        library: Option<ProjectLibrary>,
        backend: generation::Backend,
        tracking: targets::Backend,
    ) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            busy: AtomicBool::new(false),
            annotation: AtomicBool::new(false),
            #[cfg(any(test, feature = "ui-harness"))]
            requests_held: AtomicBool::new(false),
            preview_active: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            shutdown_complete: AtomicBool::new(false),
            #[cfg(test)]
            render_poll_paused: AtomicBool::new(false),
            #[cfg(test)]
            render_commit_refresh_failure: AtomicBool::new(false),
            #[cfg(test)]
            host_refresh_failure: AtomicBool::new(false),
            #[cfg(test)]
            splice_commit_refresh_failure: AtomicBool::new(false),
            #[cfg(test)]
            slip_commit_refresh_failure: AtomicBool::new(false),
            #[cfg(test)]
            trim_commit_refresh_failure: AtomicBool::new(false),
            #[cfg(test)]
            workspace_refresh_failure: AtomicBool::new(false),
            #[cfg(any(test, feature = "ui-harness"))]
            storage_failure: AtomicBool::new(false),
            update: Mutex::new(None),
            wake,
            job_board: crate::jobs::Jobs::new(),
        });
        let (requests, receive) = mpsc::sync_channel(REQUEST_LANES);
        let (jobs, results, worker) = worker::spawn()?;
        let state = shared.clone();
        std::thread::Builder::new()
            .name("deadpan-project".into())
            .spawn(move || {
                service::run(
                    state, receive, jobs, results, worker, library, backend, tracking,
                )
            })?;
        Ok(Self { requests, shared })
    }

    /// One pending/in-flight user command plus, independently, one pending
    /// annotation save. A background save therefore never makes a user
    /// command busy; the user command waits behind it in the mailbox.
    /// Import preparation is independent of both.
    pub fn submit(&self, request: ProjectRequest) -> Result<(), String> {
        if self.shared.stopping.load(Ordering::Acquire) {
            return Err("Project service is shutting down".into());
        }
        let (lane, busy) = if request.is_annotation_save() {
            (
                &self.shared.annotation,
                "An analysis save is already queued",
            )
        } else {
            (&self.shared.busy, "Project command is busy")
        };
        lane.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| busy.to_owned())?;
        // Once admitted, a lane keeps the service alive through shutdown until
        // its request completes. A stop that won before admission rejects it.
        if self.shared.stopping.load(Ordering::Acquire) {
            lane.store(false, Ordering::Release);
            return Err("Project service is shutting down".into());
        }
        // Capacity equals the lane count, so an admitted request always fits.
        if let Err(error) = self.requests.try_send(request) {
            lane.store(false, Ordering::Release);
            return Err(format!("Project command was not queued: {error}"));
        }
        Ok(())
    }

    /// The background job coordinator every job of this app registers with.
    pub fn jobs(&self) -> &crate::jobs::Jobs {
        &self.shared.job_board
    }

    pub fn take_update(&self) -> Option<ProjectUpdate> {
        self.shared.update.try_lock().ok()?.take()
    }

    /// A user command is pending or in flight. Background annotation saves
    /// are reported separately and never block user commands.
    pub fn is_busy(&self) -> bool {
        self.shared.busy.load(Ordering::Acquire)
    }

    /// An analysis save is pending or in flight.
    pub fn annotation_busy(&self) -> bool {
        self.shared.annotation.load(Ordering::Acquire)
    }

    /// Leave newly admitted requests in the mailbox, creating a deterministic
    /// busy window for replay and tests. Release with `false`.
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn hold_requests_for_check(&self, held: bool) {
        self.shared.requests_held.store(held, Ordering::Release);
    }

    /// Publish before opening any temporary editor, and clear only after the
    /// UI confirms that all temporary editors have closed.
    pub fn set_preview_active(&self, active: bool) {
        self.shared.preview_active.store(active, Ordering::Release);
    }

    /// The next authored edit fails exactly as a full disk refuses a commit.
    /// Real ENOSPC is qualified by the store and service disk-image tests.
    #[cfg(feature = "ui-harness")]
    pub fn inject_storage_failure_for_check(&self) {
        self.shared.storage_failure.store(true, Ordering::Release);
    }

    #[cfg(feature = "ui-harness")]
    pub fn preview_active_for_check(&self) -> bool {
        self.shared.preview_active.load(Ordering::Acquire)
    }

    /// Reject new commands and start a nonblocking drain of owned work.
    pub fn shutdown(&self) {
        self.shared.stopping.store(true, Ordering::Release);
    }

    /// The writer and workers have been released after checked render draining.
    pub fn is_shutdown_complete(&self) -> bool {
        self.shared.shutdown_complete.load(Ordering::Acquire)
    }
}

impl Drop for ProjectService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
