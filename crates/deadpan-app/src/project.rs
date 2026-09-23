//! Single-writer project service and bounded background source preparation.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use deadpan_core::{AssetId, FrameDuration, NodeId, ProjectDocument, RevisionId, SourceFrameIndex};
use deadpan_plan::RenderPlan;
use deadpan_store::original_media::{OriginalImportHandle, OriginalMediaRecord, OriginalOwnership};
use deadpan_store::single_source::SingleSourceState;
use deadpan_store::source_registration::SourceQualificationReceipt;

use crate::library::ProjectLibrary;

mod service;
#[cfg(test)]
mod tests;
mod worker;

pub struct RegisteredSource {
    pub asset: AssetId,
    pub label: String,
    pub receipt: Arc<SourceQualificationReceipt>,
    pub original: OriginalMediaRecord,
    pub video_index: Option<SourceFrameIndex>,
}

pub struct Workspace {
    pub session: u64,
    pub path: PathBuf,
    pub document: Arc<ProjectDocument>,
    pub plan: Arc<RenderPlan>,
    pub sources: BTreeMap<AssetId, Arc<RegisteredSource>>,
    pub originals: OriginalImportHandle,
    pub can_undo: bool,
    pub can_redo: bool,
    /// None identifies a preserved generic/legacy project.
    pub single_source: Option<SingleSourceState>,
    /// Full measured Original stream union in the current project-frame clock.
    /// This is not the video's decoded presentation-frame count.
    pub original_duration: Option<FrameDuration>,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedEdit {
    pub revision: RevisionId,
    /// None explicitly clears selection when deletion leaves an empty Sequence.
    pub selected_node: Option<NodeId>,
}

/// Authored root-beat operations. Nested occurrence editing requires a separate
/// concrete occurrence scope; the service rejects hidden or nested targets.
#[derive(Clone, Debug)]
pub enum ProjectEdit {
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
}

pub enum ProjectRequest {
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
        expected_revision: RevisionId,
        asset: AssetId,
        parent: NodeId,
        index: usize,
    },
    Edit {
        expected_session: u64,
        expected_revision: RevisionId,
        edit: ProjectEdit,
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

    fn start(
        wake: Arc<dyn Fn() + Send + Sync>,
        library: Option<ProjectLibrary>,
    ) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            busy: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
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

    /// Reject new commands, finish the admitted command and cancel preparation.
    /// Never waits on filesystem or decoder work; the UI waits for busy before exit.
    pub fn shutdown(&self) {
        self.shared.stopping.store(true, Ordering::Release);
    }
}

impl Drop for ProjectService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
