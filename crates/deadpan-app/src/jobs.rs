//! The app's background job coordinator (DP-18; specification §18.3 to §18.5).
//!
//! Every background job registers here: AI pause generation, tracking, face
//! detection, transcription, pause and shot analysis, seek proxies, renders,
//! YouTube imports, model-pack installs and portable copies. The coordinator
//! is deliberately small. It does not run work, own threads or replace any
//! job's own contract (session/revision capture, stale-result rejection,
//! durable records); it only decides *when* each job may proceed:
//!
//! - **Priority classes** order admission: realtime audio and the current
//!   preview frame ([`Priority::Interactive`]) and editing on the project
//!   writer ([`Priority::Edit`]) are never jobs and never wait for one;
//!   renders precede user-requested analysis, which precedes background
//!   generation, automatic analysis and proxies.
//! - **Resource budgets** bound concurrency: at most one large-model
//!   inference ([`Resource::Inference`]) and one automatic whole-Original
//!   analysis scan ([`Resource::Scan`]) run at once. A job that cannot run
//!   is *queued*, visibly and cancellably, rather than refused.
//! - **Yield rules** pause running work at its own safe boundaries: scans
//!   and proxies wait while the edit plays, a render runs or tracking runs;
//!   proxies also wait while an AI model runs. A job checks [`JobHandle::checkpoint`] (or polls
//!   [`JobHandle::pause_reason`]); nothing is preempted mid-step.
//! - **Cancellation and drain.** [`Jobs::cancel`] sets the job's own
//!   cooperative cancel flag; [`Jobs::shutdown`] releases every queued or
//!   paused waiter so each job's existing shutdown can join its thread.
//!
//! Jobs never take the project writer while queued or paused, so no
//! background job can delay an edit through the coordinator. See
//! `docs/JOBS.md`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How often a waiting job rechecks its own cancel flag. Coordinator events
/// (admission, cancellation, foreground changes, shutdown) wake it at once.
const WAIT_POLL: Duration = Duration::from_millis(50);

/// Kinds of background work. Each kind has one fixed priority, resource and
/// set of yield rules ([`JobKind::rules`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JobKind {
    Render,
    Tracking,
    Faces,
    Transcription,
    AiPause,
    SpeechActivity,
    Shots,
    Proxy,
    YoutubeImport,
    ModelPack,
    PortableCopy,
}

/// Admission order. Lower values win. `Interactive` and `Edit` are never
/// assigned to a job; they name the work that jobs must never delay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// Realtime audio and current-frame preview/seek.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "names the order jobs never outrank")
    )]
    Interactive,
    /// Direct editing commands on the project writer.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "names the order jobs never outrank")
    )]
    Edit,
    /// A user-requested render.
    Render,
    /// Analysis or a transfer the person asked for and is waiting on.
    Requested,
    /// AI generation, automatic analysis, proxies and other maintenance.
    Background,
}

/// A bounded shared resource. Jobs without one are admitted at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Resource {
    /// A large local model (LTX generation, whisper). One at a time: two
    /// resident models compete for unified memory and the GPU.
    Inference,
    /// An automatic whole-Original analysis scan (shots, pauses). One at a
    /// time: they are background work and decode the same Original.
    Scan,
}

impl Resource {
    pub const fn capacity(self) -> usize {
        match self {
            Self::Inference => 1,
            Self::Scan => 1,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Inference => "AI model",
            Self::Scan => "analysis",
        }
    }
}

/// Conditions a running job waits out at its next safe boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Yields {
    pub playback: bool,
    pub render: bool,
    pub inference: bool,
    /// Tracking or face detection (Vision over decoded pictures).
    pub tracking: bool,
}

/// One kind's fixed scheduling contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub priority: Priority,
    pub resource: Option<Resource>,
    pub yields: Yields,
}

impl JobKind {
    #[cfg(test)]
    pub const ALL: [Self; 11] = [
        Self::Render,
        Self::Tracking,
        Self::Faces,
        Self::Transcription,
        Self::AiPause,
        Self::SpeechActivity,
        Self::Shots,
        Self::Proxy,
        Self::YoutubeImport,
        Self::ModelPack,
        Self::PortableCopy,
    ];

    pub const fn rules(self) -> Rules {
        const NONE: Yields = Yields {
            playback: false,
            render: false,
            inference: false,
            tracking: false,
        };
        let (priority, resource, yields) = match self {
            Self::Render => (Priority::Render, None, NONE),
            // The person asked and is waiting; Vision runs per picture.
            Self::Tracking | Self::Faces => (Priority::Requested, None, NONE),
            Self::AiPause => (Priority::Requested, Some(Resource::Inference), NONE),
            // Automatic: runs whenever the Original lacks a transcript.
            Self::Transcription => (Priority::Background, Some(Resource::Inference), NONE),
            // An isolated VAD process with no safe pause point; short.
            Self::SpeechActivity => (Priority::Background, Some(Resource::Scan), NONE),
            Self::Shots => (
                Priority::Background,
                Some(Resource::Scan),
                Yields {
                    playback: true,
                    render: true,
                    inference: false,
                    tracking: true,
                },
            ),
            Self::Proxy => (
                Priority::Background,
                None,
                Yields {
                    playback: true,
                    render: true,
                    inference: true,
                    tracking: true,
                },
            ),
            Self::YoutubeImport | Self::ModelPack | Self::PortableCopy => {
                (Priority::Requested, None, NONE)
            }
        };
        Rules {
            priority,
            resource,
            yields,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Render => "Render",
            Self::Tracking => "Tracking",
            Self::Faces => "Face detection",
            Self::Transcription => "Transcription",
            Self::AiPause => "AI pause pictures",
            Self::SpeechActivity => "Pause detection",
            Self::Shots => "Shot detection",
            Self::Proxy => "Seek proxy",
            Self::YoutubeImport => "YouTube import",
            Self::ModelPack => "Model install",
            Self::PortableCopy => "Portable copy",
        }
    }
}

/// What the person is doing in the foreground.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Foreground {
    /// Audition or edit playback is preparing or delivering sound.
    pub playback: bool,
}

/// Coordinator-wide identity of one registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(u64);

impl JobId {
    #[cfg(test)]
    pub const fn for_test(id: u64) -> Self {
        Self(id)
    }
}

/// What [`Jobs::cancel`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelOutcome {
    /// The job's own cancel flag is set; it is draining.
    Signalled(JobKind, Option<u64>),
    /// The job is cancelled by its owner's request, not yet sent.
    NeedsRequest(JobKind, Option<u64>),
    NotCancellable(JobKind),
    /// It already finished.
    Gone,
}

/// What to register.
pub struct JobSpec {
    pub kind: JobKind,
    /// The project session the job belongs to; `None` for app-wide jobs.
    pub session: Option<u64>,
    /// A short description of the target, shown after the kind.
    pub detail: String,
    /// The job's own cooperative cancel flag, set by [`Jobs::cancel`].
    pub cancel: Option<Arc<AtomicBool>>,
    /// Whether the person can cancel it from the Jobs panel.
    pub cancellable: bool,
    /// Runs at once without its resource; claims it later with
    /// [`JobHandle::acquire`] (for example only around a model step).
    pub deferred: bool,
}

impl JobSpec {
    pub fn new(kind: JobKind, session: Option<u64>) -> Self {
        Self {
            kind,
            session,
            detail: String::new(),
            cancel: None,
            cancellable: true,
            deferred: false,
        }
    }

    /// Admit at once without the kind's resource; the job claims it later
    /// with [`JobHandle::acquire`] and may give it back with
    /// [`JobHandle::release`].
    pub fn deferred(mut self) -> Self {
        self.deferred = true;
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn cancel_flag(mut self, cancel: &Arc<AtomicBool>) -> Self {
        self.cancel = Some(Arc::clone(cancel));
        self
    }

    pub fn not_cancellable(mut self) -> Self {
        self.cancellable = false;
        self
    }
}

/// Reported progress: an actual stage and, only when the job measures it, a
/// fraction. Absent fractions show as indeterminate.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Progress {
    pub stage: String,
    pub fraction: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowState {
    /// Waiting for admission: its place among waiting jobs and why.
    Queued {
        position: usize,
        waiting_for: String,
    },
    Running,
    /// Admitted, but waiting at a safe boundary.
    Paused(String),
    /// Cancellation was requested; the job is draining.
    Cancelling,
}

impl RowState {
    pub fn label(&self) -> String {
        match self {
            Self::Queued {
                position,
                waiting_for,
            } => format!("Queued #{position}, waiting for {waiting_for}"),
            Self::Running => "Running".into(),
            Self::Paused(reason) => format!("Paused while {reason}"),
            Self::Cancelling => "Cancelling".into(),
        }
    }
}

/// One job as the Jobs panel shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct JobRow {
    pub id: JobId,
    pub kind: JobKind,
    pub session: Option<u64>,
    pub detail: String,
    pub state: RowState,
    pub progress: Progress,
    /// Running time once admitted, waiting time while queued.
    pub elapsed: Duration,
    pub cancellable: bool,
}

/// Why a waiting job stopped waiting without being admitted or resumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stopped {
    Cancelled,
    ShuttingDown,
}

impl std::fmt::Display for Stopped {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "cancelled",
            Self::ShuttingDown => "Deadpan is closing",
        })
    }
}

struct Entry {
    kind: JobKind,
    session: Option<u64>,
    detail: String,
    registered: Instant,
    /// When it last started waiting for admission.
    queued_at: Instant,
    admitted: Option<Instant>,
    /// Whether it holds (or waits for) its kind's resource. Deferred jobs
    /// run without it until they acquire it.
    claims: bool,
    cancel: Option<Arc<AtomicBool>>,
    cancellable: bool,
    cancelling: bool,
    paused: Option<String>,
    progress: Progress,
}

impl Entry {
    fn rules(&self) -> Rules {
        self.kind.rules()
    }

    /// The resource it holds now, if any. A cancelled job keeps holding it
    /// until it drains: its worker may still be using the GPU.
    fn holds(&self) -> Option<Resource> {
        if self.admitted.is_some() && self.claims {
            self.rules().resource
        } else {
            None
        }
    }
}

#[derive(Default)]
struct Board {
    next: u64,
    entries: BTreeMap<JobId, Entry>,
    foreground: Foreground,
    closed: bool,
    /// Admission order, for tests and diagnostics: every admitted id in order.
    #[cfg(test)]
    admissions: Vec<JobId>,
}

impl Board {
    fn running(&self, resource: Resource) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.holds() == Some(resource))
            .count()
    }

    /// Admitted jobs matching `kind`, including cancelled ones still
    /// draining: a cancelled render or model keeps using the machine until
    /// its worker has stopped, so yields last until then.
    fn any_running(&self, kind: impl Fn(&Entry) -> bool) -> bool {
        self.entries
            .values()
            .any(|entry| entry.admitted.is_some() && kind(entry))
    }

    /// Waiting entries in admission order: priority, then registration.
    fn waiting(&self) -> Vec<JobId> {
        let mut waiting: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.admitted.is_none() && !entry.cancelling)
            .map(|(id, entry)| (entry.rules().priority, *id))
            .collect();
        waiting.sort();
        waiting.into_iter().map(|(_, id)| id).collect()
    }

    /// Admit every waiting job whose resource has room, highest priority
    /// first. A lower-priority job never takes the last slot ahead of a
    /// higher-priority job waiting for the same resource.
    fn admit(&mut self) -> bool {
        if self.closed {
            return false;
        }
        let mut changed = false;
        for id in self.waiting() {
            let resource = self.entries[&id].rules().resource;
            let fits = resource.is_none_or(|resource| self.running(resource) < resource.capacity());
            if fits {
                let entry = self.entries.get_mut(&id).expect("waiting entry");
                entry.admitted = Some(Instant::now());
                #[cfg(test)]
                self.admissions.push(id);
                changed = true;
            }
        }
        changed
    }

    /// Why `entry` must wait at its next safe boundary, if it must.
    fn pause_reason(&self, id: JobId) -> Option<String> {
        let entry = self.entries.get(&id)?;
        let yields = entry.rules().yields;
        if yields.playback && self.foreground.playback {
            return Some("the edit is playing".into());
        }
        if yields.render && self.any_running(|other| other.kind == JobKind::Render) {
            return Some("a render is running".into());
        }
        if yields.inference && self.any_running(|other| other.holds() == Some(Resource::Inference))
        {
            return Some("an AI model is running".into());
        }
        if yields.tracking
            && self.any_running(|other| matches!(other.kind, JobKind::Tracking | JobKind::Faces))
        {
            return Some("tracking is running".into());
        }
        None
    }

    fn waiting_for(&self, id: JobId) -> String {
        let entry = &self.entries[&id];
        let Some(resource) = entry.rules().resource else {
            return "admission".into();
        };
        let holders: Vec<&'static str> = self
            .entries
            .values()
            .filter(|other| other.holds() == Some(resource))
            .map(|other| other.kind.label())
            .collect();
        if holders.is_empty() {
            format!("a higher-priority {} job", resource.label())
        } else {
            holders.join(" and ")
        }
    }
}

struct Inner {
    board: Mutex<Board>,
    changed: Condvar,
}

/// The shared coordinator. Cloning shares it.
#[derive(Clone)]
pub struct Jobs(Arc<Inner>);

impl Default for Jobs {
    fn default() -> Self {
        Self::new()
    }
}

impl Jobs {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            board: Mutex::new(Board::default()),
            changed: Condvar::new(),
        }))
    }

    fn lock(&self) -> MutexGuard<'_, Board> {
        // A panicking job thread must not disable the coordinator; every
        // mutation below leaves the board consistent between statements.
        self.0
            .board
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn notify(&self) {
        self.0.changed.notify_all();
    }

    /// Register a job. It is admitted at once when its resource has room,
    /// otherwise queued. After [`Jobs::shutdown`] the handle is stopped.
    pub fn register(&self, spec: JobSpec) -> JobHandle {
        let mut board = self.lock();
        board.next += 1;
        let id = JobId(board.next);
        let closed = board.closed;
        let now = Instant::now();
        board.entries.insert(
            id,
            Entry {
                kind: spec.kind,
                session: spec.session,
                detail: spec.detail,
                registered: now,
                queued_at: now,
                admitted: (spec.deferred && !closed).then_some(now),
                claims: !spec.deferred,
                cancel: spec.cancel,
                cancellable: spec.cancellable,
                cancelling: closed,
                paused: None,
                progress: Progress::default(),
            },
        );
        board.admit();
        drop(board);
        self.notify();
        JobHandle {
            jobs: self.clone(),
            id,
        }
    }

    /// Record the foreground state; paused jobs resume when it clears.
    pub fn set_foreground(&self, foreground: Foreground) {
        let mut board = self.lock();
        if board.foreground != foreground {
            board.foreground = foreground;
            drop(board);
            self.notify();
        }
    }

    /// Request cancellation from the Jobs panel. A job registered with its
    /// cooperative cancel flag is signalled and shown `Cancelling` at once.
    /// A job without one (a render, a download, a model install) is
    /// cancelled by its owner's own request: the host sends it and calls
    /// [`Jobs::mark_cancelling`] only once that request was accepted.
    pub fn cancel(&self, id: JobId) -> CancelOutcome {
        let mut board = self.lock();
        let Some(entry) = board.entries.get_mut(&id) else {
            return CancelOutcome::Gone;
        };
        if !entry.cancellable {
            return CancelOutcome::NotCancellable(entry.kind);
        }
        let (kind, session) = (entry.kind, entry.session);
        let Some(cancel) = &entry.cancel else {
            return CancelOutcome::NeedsRequest(kind, session);
        };
        cancel.store(true, Ordering::Release);
        entry.cancelling = true;
        // A cancelled waiter frees nothing; a cancelled runner keeps its
        // resource until it drains.
        drop(board);
        self.notify();
        CancelOutcome::Signalled(kind, session)
    }

    /// Show a job as draining after its owner accepted a cancel request.
    pub fn mark_cancelling(&self, id: JobId) {
        let mut board = self.lock();
        if let Some(entry) = board.entries.get_mut(&id) {
            entry.cancelling = true;
        }
        drop(board);
        self.notify();
    }

    /// Cancel every job of a project session before `bound`. Session
    /// identities only increase, so passing the UI's current session (or one
    /// past the last it showed, once no project is open) never cancels a job
    /// the service started for a session the UI has not seen yet. Each job's
    /// own reset does the same; this releases queued waiters at once.
    pub fn cancel_sessions_before(&self, bound: u64) {
        let mut board = self.lock();
        let mut changed = false;
        for entry in board.entries.values_mut() {
            let earlier = entry.session.is_some_and(|session| session < bound);
            // Jobs without a cancel flag (a render) are cancelled by their
            // owner's own session change; marking them here would claim a
            // cancellation nobody requested.
            if let Some(cancel) = entry
                .cancel
                .as_ref()
                .filter(|_| earlier && !entry.cancelling)
            {
                cancel.store(true, Ordering::Release);
                entry.cancelling = true;
                changed = true;
            }
        }
        drop(board);
        if changed {
            self.notify();
        }
    }

    /// Refuse new admissions and release every waiter. Each job's own
    /// shutdown then cancels and joins its thread.
    pub fn shutdown(&self) {
        let mut board = self.lock();
        board.closed = true;
        for entry in board.entries.values_mut() {
            entry.cancelling = true;
            if let Some(cancel) = &entry.cancel {
                cancel.store(true, Ordering::Release);
            }
        }
        drop(board);
        self.notify();
    }

    /// Every registered job, running ones first, then queued in admission
    /// order.
    pub fn snapshot(&self) -> Vec<JobRow> {
        let board = self.lock();
        let waiting = board.waiting();
        let mut rows: Vec<JobRow> = board
            .entries
            .iter()
            .map(|(id, entry)| {
                let state = if entry.cancelling {
                    RowState::Cancelling
                } else if entry.admitted.is_none() {
                    RowState::Queued {
                        position: waiting.iter().position(|w| w == id).map_or(0, |p| p + 1),
                        waiting_for: board.waiting_for(*id),
                    }
                } else if let Some(reason) = &entry.paused {
                    RowState::Paused(reason.clone())
                } else {
                    RowState::Running
                };
                JobRow {
                    id: *id,
                    kind: entry.kind,
                    session: entry.session,
                    detail: entry.detail.clone(),
                    state,
                    progress: entry.progress.clone(),
                    elapsed: if entry.admitted.is_some() {
                        entry.registered.elapsed()
                    } else {
                        entry.queued_at.elapsed()
                    },
                    cancellable: entry.cancellable,
                }
            })
            .collect();
        rows.sort_by_key(|row| {
            (
                matches!(row.state, RowState::Queued { .. }),
                row.kind.rules().priority,
                row.id,
            )
        });
        rows
    }

    /// What the newest queued job of `kind` in `session` waits for, when one
    /// is queued: the job's own status line shows it.
    pub fn waiting_for(&self, kind: JobKind, session: Option<u64>) -> Option<String> {
        let board = self.lock();
        let (id, _) = board.entries.iter().rev().find(|(_, entry)| {
            entry.kind == kind
                && entry.session == session
                && entry.admitted.is_none()
                && !entry.cancelling
        })?;
        Some(board.waiting_for(*id))
    }

    /// The registered job of `kind`, if any (the newest).
    #[cfg(feature = "ui-harness")]
    pub fn row_of(&self, kind: JobKind, session: Option<u64>) -> Option<JobRow> {
        self.snapshot()
            .into_iter()
            .filter(|row| row.kind == kind && row.session == session)
            .max_by_key(|row| row.id)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.lock().entries.is_empty()
    }

    #[cfg(test)]
    fn admissions(&self) -> Vec<JobId> {
        self.lock().admissions.clone()
    }
}

/// A registration, owned by the job (normally its thread). Dropping it
/// removes the job and admits the next waiter.
pub struct JobHandle {
    jobs: Jobs,
    id: JobId,
}

impl JobHandle {
    #[cfg(test)]
    pub fn id(&self) -> JobId {
        self.id
    }

    fn stop_reason(board: &Board, entry: &Entry, cancel: Option<&AtomicBool>) -> Option<Stopped> {
        if board.closed {
            Some(Stopped::ShuttingDown)
        } else if entry.cancelling || cancel.is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
            Some(Stopped::Cancelled)
        } else {
            None
        }
    }

    /// Whether the job may run now (it may also be paused).
    pub fn admitted(&self) -> bool {
        let board = self.jobs.lock();
        board
            .entries
            .get(&self.id)
            .is_some_and(|entry| entry.admitted.is_some())
    }

    /// Block until admitted. Returns early when the coordinator or the job's
    /// own `cancel` flag stops it; a queued job holds no resource meanwhile.
    pub fn wait_admitted(&self, cancel: Option<&AtomicBool>) -> Result<(), Stopped> {
        let mut board = self.jobs.lock();
        loop {
            let Some(entry) = board.entries.get(&self.id) else {
                return Err(Stopped::Cancelled);
            };
            if let Some(stopped) = Self::stop_reason(&board, entry, cancel) {
                return Err(stopped);
            }
            if entry.admitted.is_some() {
                return Ok(());
            }
            board = self
                .jobs
                .0
                .changed
                .wait_timeout(board, WAIT_POLL)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }

    /// A safe boundary: wait while a yield rule holds. Returns early when the
    /// job is stopped.
    pub fn checkpoint(&self, cancel: Option<&AtomicBool>) -> Result<(), Stopped> {
        let mut board = self.jobs.lock();
        loop {
            let Some(entry) = board.entries.get(&self.id) else {
                return Err(Stopped::Cancelled);
            };
            if let Some(stopped) = Self::stop_reason(&board, entry, cancel) {
                if let Some(entry) = board.entries.get_mut(&self.id) {
                    entry.paused = None;
                }
                return Err(stopped);
            }
            let reason = board.pause_reason(self.id);
            let entry = board.entries.get_mut(&self.id).expect("checked above");
            entry.paused.clone_from(&reason);
            if reason.is_none() {
                return Ok(());
            }
            board = self
                .jobs
                .0
                .changed
                .wait_timeout(board, WAIT_POLL)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }

    /// For jobs that pause an external worker themselves (for example by
    /// suspending its process group): why to pause now, recorded for the
    /// panel. `None` when it may run.
    pub fn pause_reason(&self) -> Option<String> {
        let mut board = self.jobs.lock();
        let reason = board.pause_reason(self.id);
        if let Some(entry) = board.entries.get_mut(&self.id) {
            entry.paused.clone_from(&reason);
        }
        reason
    }

    /// Claim a deferred job's resource and wait for it, as
    /// [`JobHandle::wait_admitted`] does. A no-op wait when it already
    /// holds it.
    pub fn acquire(&self, cancel: Option<&AtomicBool>) -> Result<(), Stopped> {
        self.claim();
        self.wait_admitted(cancel)
    }

    /// Claim a deferred job's resource without waiting: it is admitted now
    /// when the resource has room, otherwise queued.
    pub fn claim(&self) {
        let mut board = self.jobs.lock();
        if let Some(entry) = board.entries.get_mut(&self.id)
            && !entry.claims
        {
            entry.claims = true;
            entry.admitted = None;
            entry.queued_at = Instant::now();
            board.admit();
        }
        drop(board);
        self.jobs.notify();
    }

    /// Give the resource back while the job keeps running without it (for
    /// example while the host publishes a finished model run).
    pub fn release(&self) {
        let mut board = self.jobs.lock();
        if let Some(entry) = board.entries.get_mut(&self.id)
            && entry.claims
            && entry.admitted.is_some()
        {
            entry.claims = false;
            board.admit();
        }
        drop(board);
        self.jobs.notify();
    }

    pub fn set_progress(&self, stage: impl Into<String>, fraction: Option<f32>) {
        let mut board = self.jobs.lock();
        if let Some(entry) = board.entries.get_mut(&self.id) {
            entry.progress = Progress {
                stage: stage.into(),
                fraction: fraction.map(|fraction| fraction.clamp(0.0, 1.0)),
            };
        }
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        let mut board = self.jobs.lock();
        board.entries.remove(&self.id);
        board.admit();
        drop(board);
        self.jobs.notify();
    }
}

/// What a crash leaves, for tests and replays: a copy of a live package
/// whose database is taken through SQLite's backup API (never a raw copy of
/// an open database) while the writer still owns it, so active attempts stay
/// nonterminal and the writer's session marker remains.
#[cfg(any(test, feature = "ui-harness"))]
pub(crate) fn crash_copy(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    fn files(from: &std::path::Path, to: &std::path::Path, root: bool) -> Result<(), String> {
        std::fs::create_dir(to).map_err(|error| format!("create {}: {error}", to.display()))?;
        for entry in std::fs::read_dir(from).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry.file_name();
            let skipped = name.to_string_lossy();
            // The live database and the live writer's lock and endpoint.
            if root
                && (skipped.starts_with("project.sqlite")
                    || skipped == ".writer.lock"
                    || skipped == ".host.json")
            {
                continue;
            }
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            let target = to.join(&name);
            if kind.is_dir() {
                files(&entry.path(), &target, false)?;
            } else if kind.is_file() {
                std::fs::copy(entry.path(), &target).map_err(|error| error.to_string())?;
            } else {
                return Err(format!(
                    "package contains a link: {}",
                    entry.path().display()
                ));
            }
        }
        Ok(())
    }
    files(from, to, true)?;
    let source = rusqlite::Connection::open_with_flags(
        from.join("project.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| error.to_string())?;
    source
        .backup(rusqlite::MAIN_DB, to.join("project.sqlite"), None)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: JobKind) -> JobSpec {
        JobSpec::new(kind, Some(1))
    }

    fn state(jobs: &Jobs, handle: &JobHandle) -> RowState {
        jobs.snapshot()
            .into_iter()
            .find(|row| row.id == handle.id())
            .expect("registered")
            .state
    }

    #[test]
    fn priority_classes_are_ordered_as_the_specification_lists_them() {
        assert!(Priority::Interactive < Priority::Edit);
        assert!(Priority::Edit < Priority::Render);
        assert!(Priority::Render < Priority::Requested);
        assert!(Priority::Requested < Priority::Background);
        for kind in JobKind::ALL {
            assert!(
                kind.rules().priority >= Priority::Render,
                "{kind:?} must never outrank editing or preview"
            );
        }
    }

    #[test]
    fn at_most_one_inference_runs_and_the_next_is_queued_not_refused() {
        let jobs = Jobs::new();
        let ai = jobs.register(spec(JobKind::AiPause));
        let transcription = jobs.register(spec(JobKind::Transcription));
        assert!(ai.admitted());
        assert!(!transcription.admitted());
        assert_eq!(
            state(&jobs, &transcription),
            RowState::Queued {
                position: 1,
                waiting_for: "AI pause pictures".into()
            }
        );
        assert_eq!(
            jobs.waiting_for(JobKind::Transcription, Some(1)).as_deref(),
            Some("AI pause pictures")
        );
        drop(ai);
        assert!(transcription.admitted());
        assert_eq!(jobs.waiting_for(JobKind::Transcription, Some(1)), None);
    }

    #[test]
    fn a_freed_slot_goes_to_the_highest_priority_waiter() {
        let jobs = Jobs::new();
        let first = jobs.register(spec(JobKind::Transcription));
        let background = jobs.register(spec(JobKind::Transcription));
        let requested = jobs.register(spec(JobKind::AiPause));
        assert_eq!(
            state(&jobs, &requested),
            RowState::Queued {
                position: 1,
                waiting_for: "Transcription".into()
            }
        );
        drop(first);
        assert!(requested.admitted() && !background.admitted());
        drop(requested);
        assert!(background.admitted());
    }

    #[test]
    fn automatic_scans_take_turns_and_requested_work_is_never_queued() {
        let jobs = Jobs::new();
        let shots = jobs.register(spec(JobKind::Shots));
        let pauses = jobs.register(spec(JobKind::SpeechActivity));
        let tracking = jobs.register(spec(JobKind::Tracking));
        let faces = jobs.register(spec(JobKind::Faces));
        let render = jobs.register(spec(JobKind::Render));
        let proxy = jobs.register(spec(JobKind::Proxy));
        assert!(shots.admitted() && !pauses.admitted());
        assert!(tracking.admitted() && faces.admitted());
        assert!(render.admitted() && proxy.admitted());
        drop(shots);
        assert!(pauses.admitted());
        assert_eq!(jobs.admissions().len(), 6);
    }

    #[test]
    fn yield_rules_pause_at_boundaries_and_resume() {
        let jobs = Jobs::new();
        let shots = jobs.register(spec(JobKind::Shots));
        let proxy = jobs.register(spec(JobKind::Proxy));
        let ai = jobs.register(spec(JobKind::AiPause));
        assert_eq!(shots.pause_reason(), None);
        assert_eq!(
            proxy.pause_reason().as_deref(),
            Some("an AI model is running")
        );
        assert_eq!(ai.pause_reason(), None, "inference never pauses mid-step");
        drop(ai);
        assert_eq!(proxy.pause_reason(), None);
        let render = jobs.register(spec(JobKind::Render));
        assert_eq!(proxy.pause_reason().as_deref(), Some("a render is running"));
        assert_eq!(shots.pause_reason().as_deref(), Some("a render is running"));
        drop(render);
        let tracking = jobs.register(spec(JobKind::Tracking));
        assert_eq!(shots.pause_reason().as_deref(), Some("tracking is running"));
        assert_eq!(tracking.pause_reason(), None);
        drop(tracking);
        jobs.set_foreground(Foreground { playback: true });
        assert_eq!(shots.pause_reason().as_deref(), Some("the edit is playing"));
        assert_eq!(
            state(&jobs, &shots),
            RowState::Paused("the edit is playing".into())
        );
        drop(shots);
        let waiter = {
            let jobs = jobs.clone();
            std::thread::spawn(move || {
                let handle = jobs.register(spec(JobKind::Shots));
                let started = Instant::now();
                handle.checkpoint(None).map(|()| started.elapsed())
            })
        };
        std::thread::sleep(Duration::from_millis(120));
        assert!(!waiter.is_finished(), "the scan waits while the edit plays");
        jobs.set_foreground(Foreground::default());
        let waited = waiter.join().expect("thread").expect("resumed");
        assert!(waited >= Duration::from_millis(100));
    }

    #[test]
    fn cancelling_a_queued_job_releases_its_waiter_and_sets_its_flag() {
        let jobs = Jobs::new();
        let _ai = jobs.register(spec(JobKind::AiPause));
        let flag = Arc::new(AtomicBool::new(false));
        let queued = jobs.register(spec(JobKind::Transcription).cancel_flag(&flag));
        let id = queued.id();
        let waiter = std::thread::spawn(move || queued.wait_admitted(None));
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(
            jobs.cancel(id),
            CancelOutcome::Signalled(JobKind::Transcription, Some(1))
        );
        assert_eq!(waiter.join().expect("thread"), Err(Stopped::Cancelled));
        assert!(flag.load(Ordering::Acquire));
        assert_eq!(
            jobs.snapshot().len(),
            1,
            "the dropped waiter left the board"
        );
    }

    #[test]
    fn a_waiter_observes_its_own_cancel_flag() {
        let jobs = Jobs::new();
        let _ai = jobs.register(spec(JobKind::AiPause));
        let flag = Arc::new(AtomicBool::new(false));
        let queued = jobs.register(spec(JobKind::AiPause));
        let waiter = {
            let flag = Arc::clone(&flag);
            std::thread::spawn(move || queued.wait_admitted(Some(&flag)))
        };
        flag.store(true, Ordering::Release);
        let started = Instant::now();
        assert_eq!(waiter.join().expect("thread"), Err(Stopped::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_cancelled_runner_keeps_its_slot_until_it_drains() {
        let jobs = Jobs::new();
        let flag = Arc::new(AtomicBool::new(false));
        let ai = jobs.register(spec(JobKind::AiPause).cancel_flag(&flag));
        let next = jobs.register(spec(JobKind::AiPause));
        let proxy = jobs.register(spec(JobKind::Proxy));
        jobs.cancel(ai.id());
        assert_eq!(
            proxy.pause_reason().as_deref(),
            Some("an AI model is running"),
            "a draining model still yields the proxy"
        );
        assert_eq!(state(&jobs, &ai), RowState::Cancelling);
        assert!(!next.admitted(), "two models never load together");
        drop(ai);
        assert!(next.admitted());
    }

    #[test]
    fn shutdown_releases_every_waiter_and_refuses_new_admission() {
        let jobs = Jobs::new();
        let _ai = jobs.register(spec(JobKind::AiPause));
        let queued = jobs.register(spec(JobKind::AiPause));
        let shots = jobs.register(spec(JobKind::Shots));
        jobs.set_foreground(Foreground { playback: true });
        let waiters = [
            std::thread::spawn(move || queued.wait_admitted(None)),
            std::thread::spawn(move || shots.checkpoint(None)),
        ];
        std::thread::sleep(Duration::from_millis(60));
        jobs.shutdown();
        for waiter in waiters {
            assert_eq!(waiter.join().expect("thread"), Err(Stopped::ShuttingDown));
        }
        let late = jobs.register(spec(JobKind::Render));
        assert!(!late.admitted());
        assert_eq!(late.wait_admitted(None), Err(Stopped::ShuttingDown));
    }

    #[test]
    fn earlier_sessions_are_cancelled_and_app_wide_jobs_are_kept() {
        let jobs = Jobs::new();
        let flag = Arc::new(AtomicBool::new(false));
        let old = jobs.register(JobSpec::new(JobKind::Shots, Some(1)).cancel_flag(&flag));
        let install = jobs.register(JobSpec::new(JobKind::ModelPack, None));
        // The service may start a job for a session the UI has not seen yet.
        let newer_flag = Arc::new(AtomicBool::new(false));
        let newer =
            jobs.register(JobSpec::new(JobKind::Tracking, Some(3)).cancel_flag(&newer_flag));
        jobs.cancel_sessions_before(2);
        assert!(flag.load(Ordering::Acquire));
        assert_eq!(state(&jobs, &old), RowState::Cancelling);
        assert_eq!(state(&jobs, &install), RowState::Running);
        assert_eq!(state(&jobs, &newer), RowState::Running);
        jobs.cancel_sessions_before(4);
        assert_eq!(state(&jobs, &newer), RowState::Cancelling);
        assert_eq!(state(&jobs, &install), RowState::Running);
    }

    #[test]
    fn uncancellable_jobs_refuse_panel_cancellation() {
        let jobs = Jobs::new();
        let copy = jobs.register(spec(JobKind::PortableCopy).not_cancellable());
        assert_eq!(
            jobs.cancel(copy.id()),
            CancelOutcome::NotCancellable(JobKind::PortableCopy)
        );
        assert_eq!(state(&jobs, &copy), RowState::Running);
    }

    #[test]
    fn progress_is_clamped_and_rows_list_running_before_queued() {
        let jobs = Jobs::new();
        let ai = jobs.register(spec(JobKind::AiPause).detail("pause 3"));
        let queued = jobs.register(spec(JobKind::Transcription));
        let render = jobs.register(spec(JobKind::Render));
        ai.set_progress("Running", Some(1.5));
        let rows = jobs.snapshot();
        assert_eq!(
            rows.iter().map(|row| row.kind).collect::<Vec<_>>(),
            [JobKind::Render, JobKind::AiPause, JobKind::Transcription]
        );
        assert_eq!(rows[1].progress.fraction, Some(1.0));
        assert_eq!(rows[1].detail, "pause 3");
        drop((queued, render));
    }

    #[test]
    fn owner_cancelled_jobs_are_marked_only_after_their_request() {
        let jobs = Jobs::new();
        let render = jobs.register(spec(JobKind::Render));
        let shots = jobs.register(spec(JobKind::Shots));
        assert_eq!(
            jobs.cancel(render.id()),
            CancelOutcome::NeedsRequest(JobKind::Render, Some(1))
        );
        assert_eq!(state(&jobs, &render), RowState::Running);
        // A session change cannot claim to cancel it either.
        jobs.cancel_sessions_before(5);
        assert_eq!(state(&jobs, &render), RowState::Running);
        jobs.mark_cancelling(render.id());
        assert_eq!(state(&jobs, &render), RowState::Cancelling);
        assert_eq!(
            shots.pause_reason().as_deref(),
            Some("a render is running"),
            "a draining render still holds yields"
        );
        drop(render);
        assert_eq!(shots.pause_reason(), None);
        assert_eq!(jobs.cancel(JobId(999)), CancelOutcome::Gone);
    }

    #[test]
    fn deferred_jobs_claim_the_model_only_around_their_model_step() {
        let jobs = Jobs::new();
        let transcription = jobs.register(spec(JobKind::Transcription).deferred());
        assert_eq!(state(&jobs, &transcription), RowState::Running);
        // Its CPU phase holds nothing: generation starts at once.
        let ai = jobs.register(spec(JobKind::AiPause));
        assert!(ai.admitted());
        let waiter = {
            let jobs = jobs.clone();
            std::thread::spawn(move || {
                transcription.acquire(None)?;
                let held = jobs.snapshot().into_iter().any(|row| {
                    row.kind == JobKind::Transcription && row.state == RowState::Running
                });
                transcription.release();
                Ok::<_, Stopped>((held, transcription))
            })
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while jobs.waiting_for(JobKind::Transcription, Some(1)).is_none() {
            assert!(Instant::now() < deadline, "transcription never queued");
            std::thread::yield_now();
        }
        // AI generation keeps its slot until it releases it after its run.
        ai.release();
        let (held, transcription) = waiter.join().expect("thread").expect("admitted");
        assert!(held);
        // Released again: the AI job may claim it back.
        ai.acquire(None).expect("reacquired");
        assert!(ai.admitted());
        drop(transcription);
    }
}
