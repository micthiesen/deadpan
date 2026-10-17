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
//! - **Admission conditions** hold heavy work at its admission boundary:
//!   model jobs wait while the edit plays, on battery power, in Low Power
//!   Mode, under thermal pressure or under memory pressure, and automatic
//!   scans wait under critical memory pressure ([`Gates`]). The person may
//!   run a held job anyway ([`Jobs::run_anyway`]); nothing running is ever
//!   stopped by a condition.
//! - **Sleep.** [`Lifecycle`] invalidates AI pause generation and transcription
//!   across sleep and wake. Each owner interrupts and drains its worker;
//!   Run anyway cannot revive it. Other job kinds retain their sleep policies.
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

mod lifecycle;
pub use lifecycle::Lifecycle;

/// How often a waiting job rechecks its own cancel flag. Coordinator events
/// (admission, cancellation, foreground changes, shutdown) wake it at once.
const WAIT_POLL: Duration = Duration::from_millis(50);

/// How long one observation of power, thermal and memory conditions stays
/// current. A waiting job thread samples again once it is older, off the
/// UI thread and outside the board lock.
const CONDITIONS_TTL: Duration = Duration::from_secs(15);

/// An estimate needs this much measured progress since its baseline...
const ESTIMATE_MIN_PROGRESS: f32 = 0.05;
/// ...over at least this much time, or none is shown.
const ESTIMATE_MIN_TIME: Duration = Duration::from_secs(5);

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

/// How much memory pressure macOS reports (`kern.memorystatus_vm_pressure_level`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub enum MemoryPressure {
    #[default]
    Normal,
    Warning,
    Critical,
}

impl MemoryPressure {
    /// The kernel's level: 1 normal, 2 warning, 4 critical. Unknown values
    /// read as the nearest stricter level, never as relief.
    pub fn from_level(level: u32) -> Self {
        match level {
            0 | 1 => Self::Normal,
            2 | 3 => Self::Warning,
            _ => Self::Critical,
        }
    }
}

/// Conditions a kind waits out before it is admitted. They hold work at its
/// admission boundary only: a job already running is never stopped by them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Gates {
    /// Battery power, Low Power Mode or thermal pressure.
    pub power: bool,
    /// Original or edit playback.
    pub playback: bool,
    /// The memory pressure at which new work of this kind waits.
    pub memory: Option<MemoryPressure>,
}

impl Gates {
    const fn any(self) -> bool {
        self.power || self.playback || self.memory.is_some()
    }
}

/// One observation of the machine: the same power and thermal reading the
/// seek proxies use, plus memory pressure.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Conditions {
    /// Why power or temperature asks for restraint ("running on battery",
    /// "Low Power Mode is on", "the Mac is under thermal pressure").
    pub power: Option<String>,
    pub memory: MemoryPressure,
    /// Observed system headroom, not a sum of model reservations.
    pub memory_headroom: Option<deadpan_cli::system_conditions::MemoryHeadroom>,
}

/// Samples [`Conditions`]. It may block (it runs `pmset`); only waiting job
/// threads call it, never the UI or the writer.
pub type Probe = Arc<dyn Fn() -> Conditions + Send + Sync>;

/// The production probe: the proxies' power and thermal guard and the
/// kernel's memory pressure level, both read through bounded system tools.
pub fn system_probe() -> Probe {
    Arc::new(|| Conditions {
        power: deadpan_cli::system_conditions::power_guard(),
        memory: deadpan_cli::system_conditions::memory_pressure_level()
            .map_or(MemoryPressure::Normal, MemoryPressure::from_level),
        memory_headroom: deadpan_cli::system_conditions::memory_headroom(),
    })
}

/// One kind's fixed scheduling contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub priority: Priority,
    pub resource: Option<Resource>,
    pub yields: Yields,
    /// Admission conditions; see [`Gates`].
    pub gates: Gates,
}

impl JobKind {
    fn interrupted_by_sleep(self) -> bool {
        matches!(self, Self::AiPause | Self::Transcription)
    }
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
            Self::AiPause => (
                Priority::Requested,
                Some(Resource::Inference),
                Yields {
                    playback: true,
                    render: true,
                    ..NONE
                },
            ),
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
        let gates = match resource {
            // A large model: never loaded on battery, in Low Power Mode,
            // under thermal pressure, while the edit plays or once memory
            // is under pressure (§18.4).
            Some(Resource::Inference) => Gates {
                power: true,
                playback: true,
                memory: Some(MemoryPressure::Warning),
            },
            // A whole-Original decode: only critical pressure holds it.
            Some(Resource::Scan) => Gates {
                power: false,
                playback: false,
                memory: Some(MemoryPressure::Critical),
            },
            None => Gates {
                power: false,
                playback: false,
                memory: None,
            },
        };
        Rules {
            priority,
            resource,
            yields,
            gates,
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
    /// Selected model manifest's estimate, checked only when claiming inference.
    pub estimated_memory_bytes: Option<u64>,
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
    /// The owner's own identity for this job (an AI job's ticket), so the
    /// panel can address the owner's request to exactly this job.
    pub tag: Option<u64>,
}

impl JobSpec {
    pub fn new(kind: JobKind, session: Option<u64>) -> Self {
        Self {
            kind,
            estimated_memory_bytes: None,
            session,
            detail: String::new(),
            cancel: None,
            cancellable: true,
            deferred: false,
            tag: None,
        }
    }

    pub fn tag(mut self, tag: u64) -> Self {
        self.tag = Some(tag);
        self
    }

    pub fn estimated_memory_bytes(mut self, bytes: u64) -> Self {
        self.estimated_memory_bytes = Some(bytes);
        self
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
    /// Its resource has room, but an admission condition holds it (see
    /// [`Gates`]) or a hard memory capacity limit. Only soft conditions can be overridden.
    Held(String),
    Running,
    /// The external worker has not yet acknowledged a safe boundary.
    Pausing(String),
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
            Self::Held(reason) => format!("Waiting while {reason}"),
            Self::Running => "Running".into(),
            Self::Pausing(reason) => format!("Waiting for an inference boundary: {reason}"),
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
    /// Remaining time, only from the job's own measured progress over
    /// enough time; `None` while indeterminate, paused or too early.
    pub remaining: Option<Duration>,
    pub cancellable: bool,
    pub tag: Option<u64>,
    /// Whether a current soft admission condition can be overridden.
    pub can_run_anyway: bool,
}

/// What a job that is not yet admitted waits for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Waiting {
    /// The jobs holding its resource.
    Holder(String),
    /// An admission condition ([`Gates`]).
    Condition(String),
}

impl Waiting {
    /// "for X to finish" or "while X", to follow "waits".
    pub fn phrase(&self) -> String {
        match self {
            Self::Holder(holder) => format!("for {holder} to finish"),
            Self::Condition(reason) => format!("while {reason}"),
        }
    }
}

/// Progress measured since a baseline: the first fraction of the current
/// stage, reset by a new stage, a fraction going back, a pause or a requeue.
#[derive(Clone, Debug, PartialEq)]
struct Measured {
    stage: String,
    since: Instant,
    from: f32,
    at: Instant,
    fraction: f32,
}

impl Measured {
    /// Remaining time at `now` from the measured rate, or `None` when too
    /// little progress or time was measured, it is complete, or the job has
    /// already outlasted the projection (an estimate is never invented).
    fn remaining(&self, now: Instant) -> Option<Duration> {
        let done = self.fraction - self.from;
        let span = self.at.saturating_duration_since(self.since);
        if done < ESTIMATE_MIN_PROGRESS || span < ESTIMATE_MIN_TIME || self.fraction >= 1.0 {
            return None;
        }
        let projected = span.mul_f64(f64::from(1.0 - self.fraction) / f64::from(done));
        let since_report = now.saturating_duration_since(self.at);
        (since_report < projected).then(|| projected - since_report)
    }
}

/// Why a waiting job stopped waiting without being admitted or resumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stopped {
    Cancelled,
    ShuttingDown,
    Interrupted,
}

impl std::fmt::Display for Stopped {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "cancelled",
            Self::ShuttingDown => "Deadpan is closing",
            Self::Interrupted => "system sleep interrupted the job",
        })
    }
}

struct Entry {
    estimated_memory_bytes: Option<u64>,
    lifecycle_epoch: u64,
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
    pause_requested: Option<String>,
    progress: Progress,
    measured: Option<Measured>,
    /// The person chose to run it despite its admission conditions.
    forced: bool,
    tag: Option<u64>,
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
    lifecycle: Lifecycle,
    next: u64,
    entries: BTreeMap<JobId, Entry>,
    foreground: Foreground,
    conditions: Conditions,
    /// When `conditions` was last sampled by the probe.
    sampled: Option<Instant>,
    /// A thread is sampling now; others keep waiting on the old reading.
    sampling: bool,
    /// Invalidates an observation started before model release or probe changes.
    observation_epoch: u64,
    /// Whether a probe is installed. Without one, tests and replays explicitly
    /// record conditions.
    probed: bool,
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
            if self.held(&self.entries[&id]).is_some() {
                continue;
            }
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

    /// Whether the last observation is current enough to admit gated work.
    fn fresh(&self) -> bool {
        !self.probed
            || self
                .sampled
                .is_some_and(|sampled| sampled.elapsed() < CONDITIONS_TTL)
    }

    /// A hard capacity limit: Run anyway never bypasses unknown or insufficient memory.
    fn memory_admission_reason(&self, entry: &Entry) -> Option<String> {
        if entry.claims
            && entry.rules().resource == Some(Resource::Inference)
            && (self.probed || self.conditions.memory_headroom.is_some())
        {
            if !self.fresh() {
                return Some("Deadpan checks power and memory".into());
            }
            let Some(estimate) = entry.estimated_memory_bytes.filter(|bytes| *bytes > 0) else {
                return Some("the model's memory estimate is unavailable".into());
            };
            let Some(headroom) = &self.conditions.memory_headroom else {
                return Some("available memory could not be measured".into());
            };
            if !headroom.permits(estimate) {
                return Some(format!(
                    "available memory is {}; the model estimates {} plus {} reserved for the Mac",
                    crate::model_packs::format_bytes(headroom.available_bytes),
                    crate::model_packs::format_bytes(estimate),
                    crate::model_packs::format_bytes(headroom.reserve_bytes()),
                ));
            }
        }
        None
    }

    fn can_run_anyway(&self, entry: &Entry) -> bool {
        entry.admitted.is_none()
            && !entry.cancelling
            && !(entry.kind.interrupted_by_sleep()
                && self.lifecycle.interrupted_since(entry.lifecycle_epoch))
            && self.fresh()
            && self.memory_admission_reason(entry).is_none()
            && self.held(entry).is_some()
    }

    /// The admission condition holding `entry`, if one does.
    fn held(&self, entry: &Entry) -> Option<String> {
        if entry.kind.interrupted_by_sleep()
            && self.lifecycle.interrupted_since(entry.lifecycle_epoch)
        {
            return Some("system sleep interrupted the job".into());
        }
        if let Some(reason) = self.memory_admission_reason(entry) {
            return Some(reason);
        }
        let gates = entry.rules().gates;
        if gates.any() && !self.fresh() {
            return Some("Deadpan checks power and memory".into());
        }
        if entry.forced || !gates.any() {
            return None;
        }
        if gates.playback && self.foreground.playback {
            return Some("the edit is playing".into());
        }
        if gates.power
            && let Some(reason) = &self.conditions.power
        {
            return Some(reason.clone());
        }
        match gates.memory {
            Some(at) if self.conditions.memory >= at => Some(match self.conditions.memory {
                MemoryPressure::Critical => "memory pressure is critical".into(),
                _ => "memory is under pressure".into(),
            }),
            _ => None,
        }
    }

    /// Why a waiting entry is not admitted yet.
    fn waiting_reason(&self, id: JobId) -> Waiting {
        let entry = &self.entries[&id];
        let full = entry
            .rules()
            .resource
            .is_some_and(|resource| self.running(resource) >= resource.capacity());
        match self.held(entry) {
            Some(reason) if !full => Waiting::Condition(reason),
            _ => Waiting::Holder(self.waiting_for(id)),
        }
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
    probe: Mutex<Option<Probe>>,
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
    /// Capture once outside native callbacks; notifications only mutate the
    /// returned atomic state and never take the coordinator lock.
    pub fn lifecycle(&self) -> Lifecycle {
        self.lock().lifecycle.clone()
    }

    pub fn new() -> Self {
        Self(Arc::new(Inner {
            board: Mutex::new(Board::default()),
            changed: Condvar::new(),
            probe: Mutex::new(None),
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
        let lifecycle_epoch = board.lifecycle.snapshot();
        board.entries.insert(
            id,
            Entry {
                estimated_memory_bytes: spec.estimated_memory_bytes,
                lifecycle_epoch,
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
                pause_requested: None,
                progress: Progress::default(),
                measured: None,
                forced: false,
                tag: spec.tag,
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
            board.admit();
            drop(board);
            self.notify();
        }
    }

    /// Install the probe that samples power, thermal and memory conditions
    /// (production: [`system_probe`]). Without one, the board admits gated
    /// work on the conditions explicitly recorded by tests and replays.
    pub fn set_probe(&self, probe: Option<Probe>) {
        *self
            .0
            .probe
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = probe.clone();
        let mut board = self.lock();
        board.probed = probe.is_some();
        board.observation_epoch = board.observation_epoch.wrapping_add(1);
        board.sampled = None;
        board.admit();
        drop(board);
        self.notify();
    }

    /// Record an observation of the machine; held jobs are admitted once it
    /// clears.
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn set_conditions(&self, conditions: Conditions) {
        let mut board = self.lock();
        board.conditions = conditions;
        board.observation_epoch = board.observation_epoch.wrapping_add(1);
        board.sampled = Some(Instant::now());
        board.admit();
        drop(board);
        self.notify();
    }

    /// Sample conditions through the probe when the last reading is stale.
    /// Called by waiting job threads only: the probe may block.
    fn refresh_conditions(&self) {
        let (probe, epoch) = {
            let mut board = self.lock();
            if !board.probed || board.fresh() || board.sampling {
                return;
            }
            board.sampling = true;
            let probe = self
                .0
                .probe
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone();
            (probe, board.observation_epoch)
        };
        let conditions = probe.map(|probe| probe());
        let mut board = self.lock();
        board.sampling = false;
        if epoch == board.observation_epoch
            && let Some(conditions) = conditions
        {
            board.conditions = conditions;
            board.sampled = Some(Instant::now());
            board.admit();
        }
        drop(board);
        self.notify();
    }

    /// The person chose to run a held job despite its admission conditions
    /// (battery, heat, memory pressure, playback). Memory capacity and fresh
    /// observations are required even with this override. It still waits for its resource:
    /// two models never load together. False when it is not waiting.
    pub fn run_anyway(&self, id: JobId) -> bool {
        let mut board = self.lock();
        if !board
            .entries
            .get(&id)
            .is_some_and(|entry| board.can_run_anyway(entry))
        {
            return false;
        }
        let entry = board.entries.get_mut(&id).expect("eligible entry");
        entry.forced = true;
        board.admit();
        drop(board);
        self.notify();
        true
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
        let now = Instant::now();
        let waiting = board.waiting();
        let mut rows: Vec<JobRow> = board
            .entries
            .iter()
            .map(|(id, entry)| {
                let state = if entry.cancelling {
                    RowState::Cancelling
                } else if entry.admitted.is_none() {
                    match board.waiting_reason(*id) {
                        Waiting::Condition(reason) => RowState::Held(reason),
                        Waiting::Holder(waiting_for) => RowState::Queued {
                            position: waiting.iter().position(|w| w == id).map_or(0, |p| p + 1),
                            waiting_for,
                        },
                    }
                } else if let Some(reason) = &entry.paused {
                    RowState::Paused(reason.clone())
                } else if let Some(reason) = &entry.pause_requested {
                    RowState::Pausing(reason.clone())
                } else {
                    RowState::Running
                };
                let remaining = (state == RowState::Running)
                    .then(|| entry.measured.as_ref()?.remaining(now))
                    .flatten();
                JobRow {
                    id: *id,
                    can_run_anyway: matches!(state, RowState::Held(_))
                        && board.can_run_anyway(entry),
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
                    remaining,
                    cancellable: entry.cancellable,
                    tag: entry.tag,
                }
            })
            .collect();
        rows.sort_by_key(|row| {
            (
                matches!(row.state, RowState::Queued { .. } | RowState::Held(_)),
                row.kind.rules().priority,
                row.id,
            )
        });
        rows
    }

    /// What the newest queued job of `kind` in `session` waits for, when one
    /// is queued: the job's own status line shows it.
    pub fn waiting_for(&self, kind: JobKind, session: Option<u64>) -> Option<String> {
        self.waiting(kind, session).map(|waiting| match waiting {
            Waiting::Holder(text) | Waiting::Condition(text) => text,
        })
    }

    /// What the newest waiting job of `kind` in `session` waits for.
    pub fn waiting(&self, kind: JobKind, session: Option<u64>) -> Option<Waiting> {
        self.waiting_where(|entry| entry.kind == kind && entry.session == session)
    }

    /// What the waiting job with the owner's `tag` waits for.
    pub fn waiting_tagged(&self, kind: JobKind, tag: u64) -> Option<Waiting> {
        self.waiting_where(|entry| entry.kind == kind && entry.tag == Some(tag))
    }

    fn waiting_where(&self, matches: impl Fn(&Entry) -> bool) -> Option<Waiting> {
        let board = self.lock();
        let (id, _) =
            board.entries.iter().rev().find(|(_, entry)| {
                matches(entry) && entry.admitted.is_none() && !entry.cancelling
            })?;
        Some(board.waiting_reason(*id))
    }

    /// Live jobs the person asked for (renders and requested work), which
    /// quitting would cancel. Automatic work resumes on its own next time.
    pub fn requested(&self) -> Vec<JobRow> {
        self.snapshot()
            .into_iter()
            .filter(|row| row.kind.rules().priority <= Priority::Requested)
            .collect()
    }

    /// Whether any of `ids` is still registered.
    pub fn any_live(&self, ids: &[JobId]) -> bool {
        let board = self.lock();
        ids.iter().any(|id| board.entries.contains_key(id))
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
    /// Request scheduling without claiming that the external worker has stopped.
    pub fn external_pause_requested(&self) -> bool {
        let mut board = self.jobs.lock();
        let reason = board.pause_reason(self.id);
        if let Some(entry) = board.entries.get_mut(&self.id) {
            entry.pause_requested.clone_from(&reason);
            if reason.is_some() {
                entry.measured = None;
            }
        }
        reason.is_some()
    }

    /// Record only a validated acknowledgment from the worker's compute thread.
    pub fn external_pause_acknowledged(&self, paused: bool) {
        let mut board = self.jobs.lock();
        if let Some(entry) = board.entries.get_mut(&self.id) {
            entry.paused = paused.then(|| {
                entry
                    .pause_requested
                    .clone()
                    .unwrap_or_else(|| "inference resumes at its next boundary".into())
            });
            entry.measured = None;
        }
        drop(board);
        self.jobs.notify();
    }

    #[cfg(test)]
    pub fn id(&self) -> JobId {
        self.id
    }

    fn stop_reason(board: &Board, entry: &Entry, cancel: Option<&AtomicBool>) -> Option<Stopped> {
        if board.closed {
            Some(Stopped::ShuttingDown)
        } else if entry.cancelling || cancel.is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
            Some(Stopped::Cancelled)
        } else if entry.kind.interrupted_by_sleep()
            && board.lifecycle.interrupted_since(entry.lifecycle_epoch)
        {
            Some(Stopped::Interrupted)
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
            if entry.rules().gates.any() && board.probed && !board.fresh() {
                // Sample outside the lock: the probe runs system tools.
                drop(board);
                self.jobs.refresh_conditions();
                board = self.jobs.lock();
                continue;
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
            if reason.is_some() {
                // Paused time is not progress: restart the measured rate.
                entry.measured = None;
            }
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
            if reason.is_some() {
                entry.measured = None;
            }
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
            entry.measured = None;
            board.admit();
        }
        drop(board);
        self.jobs.notify();
    }

    /// Give the resource back while the job keeps running without it (for
    /// example while the host publishes a finished model run).
    pub fn release(&self) {
        let mut board = self.jobs.lock();
        if let Some(entry) = board.entries.get_mut(&self.id) {
            entry.paused = None;
            entry.pause_requested = None;
        }
        if let Some(entry) = board.entries.get_mut(&self.id)
            && entry.claims
            && entry.admitted.is_some()
        {
            let released_model = entry.rules().resource == Some(Resource::Inference);
            entry.claims = false;
            if released_model && board.probed {
                // The previous sample can include the model that just exited.
                // A waiting thread resamples outside this lock before admission.
                board.sampled = None;
                board.observation_epoch = board.observation_epoch.wrapping_add(1);
            }
            board.admit();
        }
        drop(board);
        self.jobs.notify();
    }

    pub fn set_progress(&self, stage: impl Into<String>, fraction: Option<f32>) {
        self.set_progress_at(stage, fraction, Instant::now());
    }

    fn set_progress_at(&self, stage: impl Into<String>, fraction: Option<f32>, at: Instant) {
        let mut board = self.jobs.lock();
        if let Some(entry) = board.entries.get_mut(&self.id) {
            let stage = stage.into();
            let fraction = fraction
                .filter(|fraction| fraction.is_finite())
                .map(|fraction| fraction.clamp(0.0, 1.0));
            entry.measured = match (fraction, entry.measured.take()) {
                (None, _) => None,
                (Some(fraction), Some(measured))
                    if measured.stage == stage && fraction >= measured.fraction =>
                {
                    Some(Measured {
                        at,
                        fraction,
                        ..measured
                    })
                }
                (Some(fraction), _) => Some(Measured {
                    stage: stage.clone(),
                    since: at,
                    from: fraction,
                    at,
                    fraction,
                }),
            };
            entry.progress = Progress { stage, fraction };
        }
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        let mut board = self.jobs.lock();
        let held_model = board
            .entries
            .remove(&self.id)
            .is_some_and(|entry| entry.holds() == Some(Resource::Inference));
        if held_model && board.probed {
            board.sampled = None;
            board.observation_epoch = board.observation_epoch.wrapping_add(1);
        }
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

    #[test]
    fn sleep_invalidates_waiting_inference_even_after_wake_and_run_anyway() {
        let jobs = Jobs::new();
        let holder = jobs.register(spec(JobKind::Transcription));
        let waiting = jobs.register(spec(JobKind::AiPause));
        assert!(!waiting.admitted());
        let lifecycle = jobs.lifecycle();
        lifecycle.will_sleep();
        lifecycle.did_wake();
        drop(holder);
        assert!(!waiting.admitted());
        assert!(!jobs.run_anyway(waiting.id()));
        assert_eq!(waiting.wait_admitted(None), Err(Stopped::Interrupted));
        drop(waiting);
        let retried = jobs.register(spec(JobKind::AiPause));
        assert!(retried.admitted());
    }

    #[test]
    fn sleep_invalidates_running_inference_and_refuses_new_sleeping_work() {
        let jobs = Jobs::new();
        let running = jobs.register(spec(JobKind::AiPause));
        assert!(running.admitted());
        jobs.lifecycle().will_sleep();
        assert_eq!(running.checkpoint(None), Err(Stopped::Interrupted));
        let sleeping = jobs.register(spec(JobKind::AiPause));
        assert!(!sleeping.admitted());
        assert!(!jobs.run_anyway(sleeping.id()));
        assert_eq!(sleeping.wait_admitted(None), Err(Stopped::Interrupted));
        drop(running);
        assert!(!sleeping.admitted());
        jobs.lifecycle().did_wake();
        assert_eq!(sleeping.wait_admitted(None), Err(Stopped::Interrupted));
    }

    #[test]
    fn transcription_sleep_stops_queued_and_running_work_without_releasing_a_live_model() {
        let jobs = Jobs::new();
        let running = jobs.register(spec(JobKind::Transcription));
        let queued = jobs.register(spec(JobKind::Transcription));
        assert!(running.admitted());
        assert!(!queued.admitted());
        jobs.lifecycle().will_sleep();
        jobs.lifecycle().did_wake();
        assert_eq!(running.checkpoint(None), Err(Stopped::Interrupted));
        assert_eq!(queued.wait_admitted(None), Err(Stopped::Interrupted));
        assert!(!jobs.run_anyway(queued.id()));
        let fresh = jobs.register(spec(JobKind::Transcription));
        assert!(
            !fresh.admitted(),
            "interruption does not free a resident model"
        );
        drop(running);
        assert!(fresh.admitted());
        drop(queued);
        assert_eq!(fresh.checkpoint(None), Ok(()));
    }

    #[test]
    fn explicit_transcription_cancellation_keeps_its_outcome_across_sleep() {
        let jobs = Jobs::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let running = jobs.register(spec(JobKind::Transcription).cancel_flag(&cancel));
        assert!(matches!(
            jobs.cancel(running.id()),
            CancelOutcome::Signalled(..)
        ));
        jobs.lifecycle().will_sleep();
        jobs.lifecycle().did_wake();
        assert_eq!(running.checkpoint(None), Err(Stopped::Cancelled));
        let asleep = jobs.lifecycle();
        asleep.will_sleep();
        let new = jobs.register(spec(JobKind::Transcription));
        assert_eq!(new.wait_admitted(None), Err(Stopped::Interrupted));
        assert!(!jobs.run_anyway(new.id()));
    }

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
    fn inference_pause_waits_for_ack_and_retains_model_slot() {
        let jobs = Jobs::new();
        let ai = jobs.register(spec(JobKind::AiPause));
        let next = jobs.register(spec(JobKind::Transcription));
        jobs.set_foreground(Foreground { playback: true });
        assert!(ai.external_pause_requested());
        assert_eq!(
            state(&jobs, &ai),
            RowState::Pausing("the edit is playing".into())
        );
        assert!(!next.admitted());
        ai.external_pause_acknowledged(true);
        assert_eq!(
            state(&jobs, &ai),
            RowState::Paused("the edit is playing".into())
        );
        jobs.set_foreground(Foreground { playback: false });
        assert!(!ai.external_pause_requested());
        assert!(matches!(state(&jobs, &ai), RowState::Paused(_)));
        // Playback restarts before the worker acknowledges the earlier resume.
        // Its physical resume must remain visible until the next pause arrives.
        jobs.set_foreground(Foreground { playback: true });
        assert!(ai.external_pause_requested());
        ai.external_pause_acknowledged(false);
        assert_eq!(
            state(&jobs, &ai),
            RowState::Pausing("the edit is playing".into())
        );
        ai.external_pause_acknowledged(true);
        jobs.set_foreground(Foreground { playback: false });
        assert!(!ai.external_pause_requested());
        ai.external_pause_acknowledged(false);
        assert_eq!(state(&jobs, &ai), RowState::Running);
        assert!(!next.admitted());
        drop(ai);
        assert!(next.admitted());
    }

    #[test]
    fn inference_yields_to_render_and_cancel_overrides_acknowledged_pause() {
        let jobs = Jobs::new();
        let flag = Arc::new(AtomicBool::new(false));
        let ai = jobs.register(spec(JobKind::AiPause).cancel_flag(&flag));
        let render = jobs.register(spec(JobKind::Render));
        assert!(ai.external_pause_requested());
        ai.external_pause_acknowledged(true);
        assert_eq!(
            state(&jobs, &ai),
            RowState::Paused("a render is running".into())
        );
        jobs.cancel(ai.id());
        assert!(flag.load(Ordering::Acquire));
        assert_eq!(state(&jobs, &ai), RowState::Cancelling);
        drop(render);
        ai.release();
        assert_eq!(state(&jobs, &ai), RowState::Cancelling);
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

    fn battery() -> Conditions {
        Conditions {
            power: Some("running on battery".into()),
            memory: MemoryPressure::Normal,
            ..Default::default()
        }
    }

    #[test]
    fn model_jobs_wait_on_battery_heat_or_low_power_and_can_run_anyway() {
        let jobs = Jobs::new();
        jobs.set_conditions(battery());
        let ai = jobs.register(spec(JobKind::AiPause));
        let transcription = jobs.register(spec(JobKind::Transcription));
        // Work that loads no model is never held by power.
        let render = jobs.register(spec(JobKind::Render));
        let shots = jobs.register(spec(JobKind::Shots));
        assert!(render.admitted() && shots.admitted());
        assert!(!ai.admitted() && !transcription.admitted());
        assert_eq!(
            state(&jobs, &ai),
            RowState::Held("running on battery".into())
        );
        assert_eq!(
            state(&jobs, &ai).label(),
            "Waiting while running on battery"
        );
        assert_eq!(
            jobs.waiting(JobKind::AiPause, Some(1)),
            Some(Waiting::Condition("running on battery".into()))
        );
        // Run anyway lifts the condition for that job only, never the budget.
        assert!(jobs.run_anyway(ai.id()));
        assert!(ai.admitted());
        assert!(!transcription.admitted());
        assert_eq!(
            state(&jobs, &transcription),
            RowState::Queued {
                position: 1,
                waiting_for: "AI pause pictures".into()
            },
            "a forced model still holds the one model slot"
        );
        assert!(!jobs.run_anyway(ai.id()), "an admitted job is not waiting");
        drop(ai);
        assert_eq!(
            state(&jobs, &transcription),
            RowState::Held("running on battery".into())
        );
        jobs.set_conditions(Conditions {
            power: Some("the Mac is under thermal pressure".into()),
            ..Conditions::default()
        });
        assert_eq!(
            state(&jobs, &transcription).label(),
            "Waiting while the Mac is under thermal pressure"
        );
        jobs.set_conditions(Conditions::default());
        assert!(transcription.admitted());
        // Conditions never stop a job that is already running.
        jobs.set_conditions(battery());
        assert_eq!(state(&jobs, &transcription), RowState::Running);
        assert_eq!(transcription.pause_reason(), None);
    }

    #[test]
    fn inference_waits_for_playback_and_running_model_requests_a_boundary() {
        let jobs = Jobs::new();
        let running = jobs.register(spec(JobKind::AiPause).deferred());
        running.acquire(None).expect("admitted");
        jobs.set_foreground(Foreground { playback: true });
        // Foreground state alone cannot establish that dispatched work stopped.
        assert_eq!(state(&jobs, &running), RowState::Running);
        assert!(running.external_pause_requested());
        assert_eq!(
            state(&jobs, &running),
            RowState::Pausing("the edit is playing".into())
        );
        running.release();
        let next = jobs.register(spec(JobKind::Transcription).deferred());
        let waiter = std::thread::spawn(move || next.acquire(None).map(|()| next));
        std::thread::sleep(Duration::from_millis(80));
        assert!(
            !waiter.is_finished(),
            "the model waits while the edit plays"
        );
        assert!(
            jobs.snapshot()
                .iter()
                .any(|row| row.kind == JobKind::Transcription
                    && row.state == RowState::Held("the edit is playing".into()))
        );
        jobs.set_foreground(Foreground::default());
        let next = waiter
            .join()
            .expect("thread")
            .expect("admitted after playback");
        assert!(next.admitted());
        drop(running);
    }

    #[test]
    fn memory_pressure_holds_new_heavy_work_by_level() {
        let jobs = Jobs::new();
        jobs.set_conditions(Conditions {
            power: None,
            memory: MemoryPressure::Warning,
            ..Default::default()
        });
        let ai = jobs.register(spec(JobKind::AiPause));
        let shots = jobs.register(spec(JobKind::Shots));
        let tracking = jobs.register(spec(JobKind::Tracking));
        assert_eq!(
            state(&jobs, &ai),
            RowState::Held("memory is under pressure".into())
        );
        assert!(
            shots.admitted(),
            "a scan waits only under critical pressure"
        );
        assert!(tracking.admitted());
        drop(shots);
        jobs.set_conditions(Conditions {
            power: None,
            memory: MemoryPressure::Critical,
            ..Default::default()
        });
        let pauses = jobs.register(spec(JobKind::SpeechActivity));
        assert_eq!(
            state(&jobs, &pauses),
            RowState::Held("memory pressure is critical".into())
        );
        jobs.set_conditions(Conditions::default());
        assert!(ai.admitted() && pauses.admitted());
        assert_eq!(MemoryPressure::from_level(1), MemoryPressure::Normal);
        assert_eq!(MemoryPressure::from_level(2), MemoryPressure::Warning);
        assert_eq!(MemoryPressure::from_level(4), MemoryPressure::Critical);
        assert_eq!(MemoryPressure::from_level(9), MemoryPressure::Critical);
    }

    #[test]
    fn a_probe_is_sampled_by_the_waiting_thread_before_gated_admission() {
        let jobs = Jobs::new();
        let reading = Arc::new(Mutex::new(Conditions {
            memory_headroom: Some(deadpan_cli::system_conditions::MemoryHeadroom {
                total_bytes: 64 * 1024 * 1024 * 1024,
                available_bytes: 40 * 1024 * 1024 * 1024,
            }),
            ..battery()
        }));
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        jobs.set_probe(Some({
            let (reading, calls) = (reading.clone(), calls.clone());
            Arc::new(move || {
                calls.fetch_add(1, Ordering::AcqRel);
                reading.lock().unwrap().clone()
            })
        }));
        // Registration (possibly on the UI thread) never runs the probe.
        let ai = jobs.register(spec(JobKind::AiPause).estimated_memory_bytes(1024));
        let render = jobs.register(spec(JobKind::Render));
        assert!(render.admitted());
        assert_eq!(calls.load(Ordering::Acquire), 0);
        assert_eq!(
            state(&jobs, &ai),
            RowState::Held("Deadpan checks power and memory".into())
        );
        let id = ai.id();
        let waiter = std::thread::spawn(move || ai.wait_admitted(None).map(|()| ai));
        let deadline = Instant::now() + Duration::from_secs(2);
        while jobs
            .snapshot()
            .iter()
            .all(|row| row.state != RowState::Held("running on battery".into()))
        {
            assert!(Instant::now() < deadline, "the waiter never sampled");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(calls.load(Ordering::Acquire), 1, "one sample while fresh");
        assert!(jobs.run_anyway(id));
        let ai = waiter.join().expect("thread").expect("admitted");
        assert!(ai.admitted());
    }

    #[test]
    fn estimates_come_only_from_measured_progress_over_enough_time() {
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let measured = |from: f32, since: u64, fraction: f32, report: u64| Measured {
            stage: "Generating pictures".into(),
            since: at(since),
            from,
            at: at(report),
            fraction,
        };
        // 40% in 20 s: 30 s remain at the report, 25 s five seconds later.
        let rate = measured(0.0, 0, 0.4, 20);
        let seconds = |remaining: Option<Duration>| {
            remaining.map(|remaining| (remaining.as_secs_f64() * 1000.0).round() / 1000.0)
        };
        assert_eq!(seconds(rate.remaining(at(20))), Some(30.0));
        assert_eq!(seconds(rate.remaining(at(25))), Some(25.0));
        // Outlasting the projection hides it rather than show zero.
        assert_eq!(rate.remaining(at(51)), None);
        // Too early, too little progress, or complete.
        assert_eq!(measured(0.0, 0, 0.5, 4).remaining(at(4)), None);
        assert_eq!(measured(0.2, 0, 0.24, 30).remaining(at(30)), None);
        assert_eq!(measured(0.0, 0, 1.0, 30).remaining(at(30)), None);

        let jobs = Jobs::new();
        let job = jobs.register(spec(JobKind::Render));
        let row = |jobs: &Jobs| jobs.snapshot().remove(0);
        job.set_progress_at("Encoding", Some(0.1), at(0));
        job.set_progress_at("Encoding", Some(0.5), at(10));
        // The baseline is the first report: 0.4 in 10 s.
        let remaining = row(&jobs).remaining.expect("measured");
        assert!(remaining <= Duration::from_millis(12_500) && remaining > Duration::ZERO);
        // A new stage, an indeterminate report or a pause restarts it.
        job.set_progress_at("Verifying", Some(0.6), at(11));
        assert_eq!(row(&jobs).remaining, None);
        job.set_progress_at("Verifying", None, at(12));
        assert_eq!(row(&jobs).remaining, None);
        job.set_progress_at("Verifying", Some(0.0), at(0));
        job.set_progress_at("Verifying", Some(0.5), at(10));
        assert!(row(&jobs).remaining.is_some());
        job.set_progress_at("Verifying", f32::NAN.into(), at(11));
        assert_eq!(row(&jobs).progress.fraction, None);
        let shots = jobs.register(spec(JobKind::Shots));
        shots.set_progress_at("Scanning", Some(0.0), at(0));
        shots.set_progress_at("Scanning", Some(0.5), at(10));
        jobs.set_foreground(Foreground { playback: true });
        assert!(shots.pause_reason().is_some());
        jobs.set_foreground(Foreground::default());
        shots.set_progress_at("Scanning", Some(0.6), at(11));
        let shots_row = jobs
            .snapshot()
            .into_iter()
            .find(|row| row.kind == JobKind::Shots)
            .expect("listed");
        assert_eq!(shots_row.remaining, None, "a pause restarts the baseline");
    }

    #[test]
    fn requested_jobs_and_tags_are_listed_for_the_quit_question() {
        let jobs = Jobs::new();
        let ai = jobs.register(spec(JobKind::AiPause).tag(7));
        let proxy = jobs.register(spec(JobKind::Proxy));
        let render = jobs.register(spec(JobKind::Render));
        let requested: Vec<_> = jobs.requested().into_iter().map(|row| row.kind).collect();
        assert_eq!(requested, [JobKind::Render, JobKind::AiPause]);
        assert_eq!(
            jobs.snapshot()
                .into_iter()
                .find(|row| row.kind == JobKind::AiPause)
                .and_then(|row| row.tag),
            Some(7)
        );
        let ids = [ai.id(), render.id()];
        assert!(jobs.any_live(&ids));
        drop((ai, render));
        assert!(!jobs.any_live(&ids));
        drop(proxy);
    }
}

#[cfg(test)]
mod memory_admission_tests {
    use super::*;
    use deadpan_cli::system_conditions::MemoryHeadroom;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn conditions(available_bytes: u64) -> Conditions {
        Conditions {
            memory_headroom: Some(MemoryHeadroom {
                total_bytes: 64 * GIB,
                available_bytes,
            }),
            ..Conditions::default()
        }
    }

    fn model(bytes: u64) -> JobSpec {
        JobSpec::new(JobKind::AiPause, None).estimated_memory_bytes(bytes)
    }

    #[test]
    fn model_admission_requires_estimate_plus_reserve_at_exact_boundary() {
        let jobs = Jobs::new();
        jobs.set_conditions(conditions(29 * GIB - 1));
        let handle = jobs.register(model(21 * GIB).deferred());
        assert!(
            handle.admitted(),
            "preparation does not claim the model's memory"
        );
        handle.claim();
        assert!(!handle.admitted());
        jobs.set_conditions(conditions(29 * GIB));
        assert!(handle.admitted());
        // Admission only: changing headroom never evicts a resident model.
        jobs.set_conditions(conditions(0));
        assert!(handle.admitted());
        drop(handle);
        jobs.set_conditions(conditions(40 * GIB));
        let overflow = jobs.register(model(u64::MAX));
        assert!(!overflow.admitted());
        assert!(
            matches!(&jobs.snapshot()[0].state, RowState::Held(reason) if reason.contains("available memory"))
        );
    }

    #[test]
    fn model_release_requires_a_new_production_headroom_sample() {
        let jobs = Jobs::new();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        jobs.set_probe(Some({
            let calls = calls.clone();
            Arc::new(move || {
                calls.fetch_add(1, Ordering::Relaxed);
                conditions(40 * GIB)
            })
        }));
        jobs.set_conditions(conditions(40 * GIB));
        let first = jobs.register(model(21 * GIB));
        let second = jobs.register(model(21 * GIB));
        assert!(first.admitted());
        assert!(!second.admitted());
        first.release();
        assert!(
            !second.admitted(),
            "old headroom cannot admit the next model"
        );
        assert_eq!(
            calls.load(Ordering::Relaxed),
            0,
            "release never blocks on a probe"
        );
        jobs.refresh_conditions();
        assert!(second.admitted());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn probe_started_before_model_release_cannot_restore_old_observation() {
        let jobs = Jobs::new();
        let (started, observed) = std::sync::mpsc::sync_channel(1);
        let (resume, released) = std::sync::mpsc::sync_channel(1);
        let released = Mutex::new(released);
        let calls = std::sync::atomic::AtomicUsize::new(0);
        jobs.set_probe(Some(Arc::new(move || {
            if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                started.send(()).unwrap();
                released
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }
            conditions(40 * GIB)
        })));
        jobs.set_conditions(conditions(40 * GIB));
        let first = jobs.register(model(21 * GIB));
        let second = jobs.register(model(21 * GIB));
        jobs.lock().sampled = None;
        let probe = {
            let jobs = jobs.clone();
            std::thread::spawn(move || jobs.refresh_conditions())
        };
        observed.recv_timeout(Duration::from_secs(2)).unwrap();
        first.release();
        resume.send(()).unwrap();
        probe.join().unwrap();
        assert!(!second.admitted());
        assert!(jobs.lock().sampled.is_none());
        jobs.refresh_conditions();
        assert!(second.admitted());
    }

    #[test]
    fn unknown_insufficient_and_overflow_memory_cannot_be_overridden() {
        let jobs = Jobs::new();
        jobs.set_probe(Some(Arc::new(Conditions::default)));
        jobs.set_conditions(Conditions::default());
        let unknown = jobs.register(JobSpec::new(JobKind::AiPause, None));
        let no_reading = jobs.register(model(21 * GIB));
        assert!(!jobs.run_anyway(unknown.id()));
        assert!(!jobs.run_anyway(no_reading.id()));
        assert!(jobs.snapshot().iter().all(|row| !row.can_run_anyway));
        jobs.set_conditions(conditions(20 * GIB));
        assert!(!jobs.run_anyway(no_reading.id()));
        assert!(!no_reading.admitted());
        jobs.set_conditions(Conditions {
            memory_headroom: Some(MemoryHeadroom {
                total_bytes: 16 * GIB,
                available_bytes: 16 * GIB,
            }),
            ..Conditions::default()
        });
        assert!(
            !jobs.run_anyway(no_reading.id()),
            "a smaller Mac cannot load this model"
        );
        assert!(!no_reading.admitted());
        drop(no_reading);
        let overflow = jobs.register(model(u64::MAX));
        jobs.set_conditions(conditions(40 * GIB));
        assert!(!jobs.run_anyway(overflow.id()));
        assert!(!overflow.admitted());
        assert!(!jobs.run_anyway(unknown.id()));
    }

    #[test]
    fn pressure_override_requires_fresh_capacity_and_keeps_the_model_slot() {
        let jobs = Jobs::new();
        jobs.set_probe(Some(Arc::new(|| conditions(40 * GIB))));
        jobs.set_conditions(Conditions {
            memory: MemoryPressure::Warning,
            ..conditions(40 * GIB)
        });
        let first = jobs.register(model(21 * GIB));
        let second = jobs.register(model(21 * GIB));
        assert!(jobs.snapshot().iter().all(|row| row.can_run_anyway));
        assert!(jobs.run_anyway(first.id()));
        assert!(first.admitted());
        assert!(jobs.run_anyway(second.id()));
        assert!(
            !second.admitted(),
            "override never takes a resident model's slot"
        );
        first.release();
        assert!(
            !second.admitted(),
            "forced entries still require a fresh observation"
        );
        assert!(!jobs.run_anyway(second.id()));
        jobs.refresh_conditions();
        assert!(second.admitted());
    }
}
