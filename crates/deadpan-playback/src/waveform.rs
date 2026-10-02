//! Bounded definition and admitted Edit-window analysis on one PCM preparation worker.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_audio::{DefinitionWaveform, WaveformMemory};
use deadpan_core::{NodeId, ProjectId, RevisionId};

use crate::controller::{Engine, Shared};
use crate::{ContentIdentity, Snapshot};

mod edit;
pub use edit::{EditWaveformRequest, EditWaveformUpdate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveformTicket(u64);

impl WaveformTicket {
    pub fn value(self) -> u64 {
        self.0
    }
}

pub struct WaveformRequest {
    pub snapshot: Arc<Snapshot>,
    pub owner: NodeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveformStatus {
    Queued,
    Measuring,
    Complete,
    Partial,
    Interrupted,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct WaveformUpdate {
    pub ticket: WaveformTicket,
    pub session: u64,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub owner: NodeId,
    pub status: WaveformStatus,
    pub waveform: Option<Arc<DefinitionWaveform>>,
    /// Published coverage during progress; terminal updates also count a
    /// successfully examined but incomplete leaf that remains unknown in paint.
    pub examined_samples: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WaveformRequestError {
    #[error("waveform analysis requires a committed base snapshot")]
    Proposed,
    #[error("invalid waveform snapshot admission: {0}")]
    InvalidAdmission(String),
    #[error("Edit waveform range must be nonnegative and ordered")]
    InvalidRange,
    #[error("playback engine has shut down")]
    Shutdown,
    #[error("waveform request identities are exhausted")]
    Exhausted,
}

pub(crate) enum Target {
    Definition(NodeId),
    Edit {
        base: Arc<Snapshot>,
        samples: std::ops::Range<deadpan_core::AudioSample>,
        limits: deadpan_audio::WaveformLimits,
    },
}

#[derive(Clone)]
pub(crate) enum AnalysisUpdate {
    Definition(WaveformUpdate),
    Edit(EditWaveformUpdate),
}
impl AnalysisUpdate {
    fn status(&self) -> WaveformStatus {
        match self {
            Self::Definition(v) => v.status,
            Self::Edit(v) => v.status,
        }
    }
    fn failure(&mut self, status: WaveformStatus, reason: &str) {
        match self {
            Self::Definition(v) => {
                v.status = status;
                v.error = Some(reason.into());
            }
            Self::Edit(v) => {
                v.status = status;
                v.error = Some(reason.into());
            }
        }
    }
    fn retain_progress(&mut self, previous: Self) {
        match (self, previous) {
            (Self::Definition(v), Self::Definition(p)) => {
                v.waveform = p.waveform;
                v.examined_samples = p.examined_samples;
            }
            (Self::Edit(v), Self::Edit(p)) => {
                v.waveform = p.waveform;
                v.examined_samples = p.examined_samples;
            }
            _ => {}
        }
    }
}
impl From<WaveformUpdate> for AnalysisUpdate {
    fn from(value: WaveformUpdate) -> Self {
        Self::Definition(value)
    }
}
impl From<EditWaveformUpdate> for AnalysisUpdate {
    fn from(value: EditWaveformUpdate) -> Self {
        Self::Edit(value)
    }
}

pub(crate) struct Job {
    pub ticket: WaveformTicket,
    pub snapshot: Arc<Snapshot>,
    pub target: Target,
    pub cancelled: AtomicBool,
    finished: AtomicBool,
}

impl Job {
    fn update(&self, status: WaveformStatus) -> AnalysisUpdate {
        match &self.target {
            Target::Definition(_) => self.definition_update(status).into(),
            Target::Edit { .. } => self.edit_update(status).into(),
        }
    }
    pub(crate) fn definition_update(&self, status: WaveformStatus) -> WaveformUpdate {
        let Target::Definition(owner) = &self.target else {
            unreachable!("definition waveform job")
        };
        WaveformUpdate {
            ticket: self.ticket,
            session: self.snapshot.session,
            project_id: self.snapshot.document.project_id().clone(),
            revision_id: self.snapshot.document.revision_id().clone(),
            owner: owner.clone(),
            status,
            waveform: None,
            examined_samples: 0,
            error: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    serial: u64,
    current: Option<Arc<Job>>,
    pending: Option<Arc<Job>>,
    running: Option<Arc<Job>>,
    reply: Option<AnalysisUpdate>,
    pub memory: WaveformMemory,
}

impl State {
    fn request(
        &mut self,
        request: WaveformRequest,
    ) -> Result<WaveformTicket, WaveformRequestError> {
        self.request_target(request.snapshot, Target::Definition(request.owner))
    }

    fn request_target(
        &mut self,
        snapshot: Arc<Snapshot>,
        target: Target,
    ) -> Result<WaveformTicket, WaveformRequestError> {
        let serial = self
            .serial
            .checked_add(1)
            .ok_or(WaveformRequestError::Exhausted)?;
        if let Some(job) = &self.current {
            job.cancelled.store(true, Ordering::Release);
        }
        let job = Arc::new(Job {
            ticket: WaveformTicket(serial),
            snapshot,
            target,
            cancelled: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        });
        self.serial = serial;
        self.reply = Some(job.update(WaveformStatus::Queued));
        self.pending = Some(job.clone());
        self.current = Some(job);
        Ok(WaveformTicket(serial))
    }

    pub(crate) fn take_pending(&mut self) -> Option<Arc<Job>> {
        let job = self.pending.take()?;
        if job.cancelled.load(Ordering::Acquire) {
            return None;
        }
        self.running = Some(job.clone());
        Some(job)
    }

    pub(crate) fn interrupt(&mut self, reason: &str) -> bool {
        // A superseded job can still be unwinding on the one preparation worker.
        if let Some(job) = &self.running {
            job.cancelled.store(true, Ordering::Release);
        }
        self.pending = None;
        let Some(job) = &self.current else {
            return false;
        };
        if job.finished.load(Ordering::Acquire) {
            return false;
        }
        job.cancelled.store(true, Ordering::Release);
        let mut update = job.update(WaveformStatus::Interrupted);
        update.failure(WaveformStatus::Interrupted, reason);
        if let Some(previous) = self.reply.take() {
            update.retain_progress(previous);
        }
        self.reply = Some(update);
        true
    }

    pub(crate) fn fail(&mut self, reason: &str) {
        self.interrupt(reason);
        if let Some(update) = &mut self.reply
            && update.status() == WaveformStatus::Interrupted
        {
            update.failure(WaveformStatus::Unavailable, reason);
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.interrupt("Waveform analysis stopped with the playback engine");
        self.current = None;
        self.reply = None;
        self.running = None;
    }
}

impl Shared {
    pub(crate) fn publish_waveform(
        &self,
        job: &Job,
        update: impl Into<AnalysisUpdate>,
        terminal: bool,
    ) {
        let mut update = update.into();
        let mut state = self.lock();
        if terminal {
            job.finished.store(true, Ordering::Release);
            if state
                .waveform
                .running
                .as_ref()
                .is_some_and(|running| running.ticket == job.ticket)
            {
                state.waveform.running = None;
            }
        }
        if state.shutdown()
            || !state
                .waveform
                .current
                .as_ref()
                .is_some_and(|current| current.ticket == job.ticket)
        {
            return;
        }
        if job.cancelled.load(Ordering::Acquire) {
            if !terminal {
                return;
            }
            update.failure(
                WaveformStatus::Interrupted,
                "Waveform analysis was interrupted",
            );
        }
        if let AnalysisUpdate::Edit(update) = &mut update {
            // This is the last worker-side check, after measurement and cache
            // release. It also protects partial/progress and empty results.
            update.retain_live_media(&job.snapshot);
        }
        state.waveform.reply = Some(update);
        drop(state);
        self.repaint();
    }
}

impl Engine {
    /// Queue one captured definition. A newer request replaces queued work;
    /// active output and its scheduled terminal prefix keep analysis deferred.
    pub fn request_waveform(
        &self,
        request: WaveformRequest,
    ) -> Result<WaveformTicket, WaveformRequestError> {
        if request.snapshot.content != ContentIdentity::Committed {
            return Err(WaveformRequestError::Proposed);
        }
        request
            .snapshot
            .validate_admission()
            .map_err(|error| WaveformRequestError::InvalidAdmission(error.to_string()))?;
        let mut state = self.shared.lock();
        if state.shutdown() {
            return Err(WaveformRequestError::Shutdown);
        }
        let ticket = state.waveform.request(request)?;
        drop(state);
        self.shared.wake.notify_all();
        self.shared.repaint();
        Ok(ticket)
    }

    /// Closing a draft revokes its exact ticket without affecting playback.
    pub fn cancel_waveform(&self, ticket: WaveformTicket) {
        let mut state = self.shared.lock();
        if state
            .waveform
            .current
            .as_ref()
            .is_some_and(|job| job.ticket == ticket)
        {
            state.waveform.interrupt("Waveform request cancelled");
            state.waveform.current = None;
            state.waveform.reply = None;
        }
        drop(state);
        self.shared.wake.notify_all();
    }

    pub fn poll_waveform(&self) -> Option<WaveformUpdate> {
        let mut state = self.shared.lock();
        if !matches!(state.waveform.reply, Some(AnalysisUpdate::Definition(_))) {
            return None;
        }
        match state.waveform.reply.take() {
            Some(AnalysisUpdate::Definition(update)) => Some(update),
            _ => unreachable!("checked reply kind"),
        }
    }
}
