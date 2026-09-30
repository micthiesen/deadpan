//! Single-writer project service and bounded background source preparation.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use deadpan_core::{
    AssetId, AudioEdgePolicy, AudioSample, FrameDuration, HoldAudio, NodeId, ProjectDocument,
    ProjectFrame, ProjectId, RevisionId, SoundId, SourceAudio, SourceFrameIndex,
    SourceQualificationId,
};
use deadpan_plan::RenderPlan;
use deadpan_store::original_media::{OriginalImportHandle, OriginalMediaRecord, OriginalOwnership};
use deadpan_store::single_source::SingleSourceState;
use deadpan_store::source_registration::SourceQualificationReceipt;

use crate::library::ProjectLibrary;

pub mod gain;
mod pause;
pub mod retime;
mod scope;
mod service;
pub mod sound;
#[cfg(test)]
mod tests;
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
}

impl Workspace {
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

#[derive(Clone, Copy, Debug)]
pub enum ImportMedia {
    Video,
    FirstAudio,
    Audio { stream: u32 },
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
    /// Read-only source-range preparation, bound to the captured request.
    /// Consumers must admit its ticket, session and revision before using it.
    pub room_tone: Option<PreparedRoomTone>,
    /// A preparation rejection carries the request context, including stale
    /// contexts. Generic service errors cannot complete a range request.
    pub room_tone_error: Option<RoomToneFailure>,
    /// Both success and failure retain the exact uncommitted proposal identity.
    pub gain: Option<gain::ProposalUpdate>,
    /// Operational render feedback is retained independently of editor feedback.
    pub render: Option<ProjectRenderUpdate>,
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

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "engineering service API awaits automatic Render policy"
    )
)]
pub enum ProjectRenderOperation {
    Start {
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
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Render command feedback awaits automatic Render policy"
        )
    )]
    pub ticket: u64,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Render command feedback awaits automatic Render policy"
        )
    )]
    pub context: ProjectRenderContext,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Render command feedback awaits automatic Render policy"
        )
    )]
    pub result: Result<deadpan_cli::encoded_render::workflow::WorkflowIdentity, ProjectRenderError>,
}

#[derive(Clone, Debug)]
pub struct ProjectRenderStatus {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Render status identity awaits automatic Render policy"
        )
    )]
    pub context: ProjectRenderContext,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Render status identity awaits automatic Render policy"
        )
    )]
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
    Delete {
        id: SoundId,
    },
}

/// Node-targeted operations edit direct children of the captured Sequence scope.
/// InsertTime remains project-boundary based and resolves its actual owner.
#[derive(Clone, Debug)]
pub enum ProjectEdit {
    SetAudioTreatments {
        node: NodeId,
        treatments: deadpan_core::AudioTreatments,
    },
    SetFraming {
        node: NodeId,
        framing: Option<deadpan_core::Framing>,
    },
    InsertTime {
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
    Delete {
        node: NodeId,
    },
    HoldDuration {
        node: NodeId,
        duration: FrameDuration,
    },
    HoldAudio {
        node: NodeId,
        audio: HoldAudio,
    },
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
    pub index: usize,
}

pub enum ProjectRequest {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "engineering service API awaits automatic Render policy"
        )
    )]
    Render(ProjectRenderRequest),
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
        ownership: OriginalOwnership,
    },
    /// Retained for generic host fixtures and compatibility; native New uses
    /// CreateFromSource and never asks for an arbitrary package location.
    #[cfg(test)]
    Create(PathBuf),
    Open(PathBuf),
    Close,
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

struct Shared {
    busy: AtomicBool,
    stopping: AtomicBool,
    shutdown_complete: AtomicBool,
    #[cfg(test)]
    render_poll_paused: AtomicBool,
    update: Mutex<Option<ProjectUpdate>>,
    wake: Arc<dyn Fn() + Send + Sync>,
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
        let shared = Arc::new(Shared {
            busy: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            shutdown_complete: AtomicBool::new(false),
            #[cfg(test)]
            render_poll_paused: AtomicBool::new(false),
            update: Mutex::new(None),
            wake,
        });
        let (requests, receive) = mpsc::sync_channel(1);
        let (jobs, results, worker) = worker::spawn()?;
        let state = shared.clone();
        std::thread::Builder::new()
            .name("deadpan-project".into())
            .spawn(move || service::run(state, receive, jobs, results, worker, library))?;
        Ok(Self { requests, shared })
    }

    /// One pending/in-flight user command. Import preparation is independent.
    pub fn submit(&self, request: ProjectRequest) -> Result<(), String> {
        if self.shared.stopping.load(Ordering::Acquire) {
            return Err("Project service is shutting down".into());
        }
        self.shared
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Project command is busy".to_owned())?;
        // Once admitted, busy keeps the service alive through shutdown until
        // this command completes. A stop that won before admission rejects it.
        if self.shared.stopping.load(Ordering::Acquire) {
            self.shared.busy.store(false, Ordering::Release);
            return Err("Project service is shutting down".into());
        }
        if let Err(error) = self.requests.try_send(request) {
            self.shared.busy.store(false, Ordering::Release);
            return Err(format!("Project command was not queued: {error}"));
        }
        Ok(())
    }

    pub fn take_update(&self) -> Option<ProjectUpdate> {
        self.shared.update.try_lock().ok()?.take()
    }

    pub fn is_busy(&self) -> bool {
        self.shared.busy.load(Ordering::Acquire)
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
