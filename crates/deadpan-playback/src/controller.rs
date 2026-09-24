use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use deadpan_core::{AudioSample, ProjectId, RevisionId};
use deadpan_output::{
    ClockPosition, DeliveryClock, DeviceReport, Feed, FeedError, Generation, RenderStatus,
    StopToken,
};

use crate::Snapshot;
use crate::preparation::{self, Batch, Reply};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Playing,
    Stopped,
    Ended,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub ticket: u64,
    pub session: u64,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub phase: Phase,
    pub sample: Option<AudioSample>,
    pub generation: Option<Generation>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    #[error("monitor gain must be finite and between zero and one")]
    InvalidGain,
    #[error("playback start must be nonnegative")]
    InvalidStart,
    #[error("playback engine has shut down")]
    Shutdown,
    #[error("playback request identities are exhausted")]
    Exhausted,
}

pub(crate) struct Job {
    pub epoch: u64,
    pub ticket: u64,
    pub snapshot: Arc<Snapshot>,
    pub start: AudioSample,
    pub gain: f32,
    pub cancelled: AtomicBool,
}

impl Job {
    fn update(
        &self,
        phase: Phase,
        generation: Option<Generation>,
        sample: Option<AudioSample>,
        error: Option<String>,
    ) -> Update {
        Update {
            ticket: self.ticket,
            session: self.snapshot.session,
            project_id: self.snapshot.document.project_id().clone(),
            revision_id: self.snapshot.document.revision_id().clone(),
            phase,
            sample,
            generation,
            error,
        }
    }
}

enum Intent {
    Play(Arc<Job>),
    Stop,
}

pub(crate) struct State {
    epoch: u64,
    shutdown: bool,
    intent: Option<Intent>,
    current: Option<Arc<Job>>,
    token: Option<StopToken>,
    pub prep: Option<Arc<Job>>,
    pub reply: Option<Reply>,
    update: Option<Update>,
}
impl State {
    pub(crate) fn shutdown(&self) -> bool {
        self.shutdown
    }
}

pub(crate) struct Shared {
    state: Mutex<State>,
    pub wake: Condvar,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Shared {
    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
    pub(crate) fn publish(&self, job: &Job, update: Update) {
        let mut state = self.lock();
        if state
            .current
            .as_ref()
            .is_some_and(|current| current.epoch == job.epoch)
            && !state.shutdown
            && !matches!(state.intent, Some(Intent::Stop))
        {
            let changed = state.update.as_ref() != Some(&update);
            state.update = Some(update);
            drop(state);
            if changed {
                (self.repaint)();
            }
        }
    }
    fn stop(&self, shutdown: bool) {
        let mut state = self.lock();
        if let Some(job) = &state.current {
            job.cancelled.store(true, Ordering::Release);
        }
        if let Some(token) = &state.token {
            token.stop();
        }
        state.shutdown |= shutdown;
        state.intent = Some(Intent::Stop);
        state.prep = None;
        state.reply = None;
        drop(state);
        self.wake.notify_all();
    }
    fn worker_failed(&self, worker: &str) {
        let mut state = self.lock();
        if let Some(token) = &state.token {
            token.stop();
        }
        if let Some(job) = &state.current {
            job.cancelled.store(true, Ordering::Release);
            state.update = Some(job.update(
                Phase::Failed,
                state.token.as_ref().map(StopToken::generation),
                None,
                Some(format!("{worker} worker terminated unexpectedly")),
            ));
        }
        state.shutdown = true;
        state.intent = Some(Intent::Stop);
        state.prep = None;
        state.reply = None;
        drop(state);
        self.wake.notify_all();
        (self.repaint)();
    }
}

/// Bounded revocation for lifecycle callbacks. It keeps no device, SQLite
/// connection, or shutdown ownership and never waits for worker teardown.
#[derive(Clone)]
pub struct StopHandle {
    shared: Arc<Shared>,
}
impl StopHandle {
    pub fn stop(&self) {
        self.shared.stop(false);
    }
}

/// Owns two persistent threads. `stop`/`shutdown` revoke the current output
/// generation immediately; cooperative media teardown completes on its worker.
/// Dropping the engine requests shutdown without blocking a native UI thread.
pub struct Engine {
    shared: Arc<Shared>,
}

impl Engine {
    pub fn new(repaint: Arc<dyn Fn() + Send + Sync>) -> io::Result<Self> {
        Self::with_factory(repaint, Box::new(open_device))
    }

    pub(crate) fn with_factory(
        repaint: Arc<dyn Fn() + Send + Sync>,
        factory: Factory,
    ) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                epoch: 0,
                shutdown: false,
                intent: None,
                current: None,
                token: None,
                prep: None,
                reply: None,
                update: None,
            }),
            wake: Condvar::new(),
            repaint,
        });
        let prep_shared = shared.clone();
        thread::Builder::new()
            .name("deadpan-pcm-preparation".into())
            .spawn(move || {
                if catch_unwind(AssertUnwindSafe(|| preparation::run(prep_shared.clone()))).is_err()
                {
                    prep_shared.worker_failed("audio preparation");
                }
            })?;
        let control_shared = shared.clone();
        if let Err(error) = thread::Builder::new()
            .name("deadpan-playback-control".into())
            .spawn(move || {
                if catch_unwind(AssertUnwindSafe(|| run(control_shared.clone(), factory))).is_err()
                {
                    control_shared.worker_failed("audio device control");
                }
            })
        {
            shared.stop(true);
            return Err(error);
        }
        Ok(Self { shared })
    }

    pub fn play(
        &self,
        ticket: u64,
        snapshot: Arc<Snapshot>,
        start: AudioSample,
        monitor_gain: f32,
    ) -> Result<(), RequestError> {
        if !monitor_gain.is_finite() || !(0.0..=1.0).contains(&monitor_gain) {
            return Err(RequestError::InvalidGain);
        }
        if start.0 < 0 {
            return Err(RequestError::InvalidStart);
        }
        let mut state = self.shared.lock();
        if state.shutdown {
            return Err(RequestError::Shutdown);
        }
        let epoch = state.epoch.checked_add(1).ok_or(RequestError::Exhausted)?;
        if let Some(job) = &state.current {
            job.cancelled.store(true, Ordering::Release);
        }
        if let Some(token) = &state.token {
            token.stop();
        }
        let job = Arc::new(Job {
            epoch,
            ticket,
            snapshot,
            start,
            gain: monitor_gain,
            cancelled: AtomicBool::new(false),
        });
        state.epoch = epoch;
        state.current = Some(job.clone());
        state.intent = Some(Intent::Play(job.clone()));
        state.prep = None;
        state.reply = None;
        state.update = Some(job.update(Phase::Preparing, None, None, None));
        drop(state);
        self.shared.wake.notify_all();
        (self.shared.repaint)();
        Ok(())
    }
    pub fn stop(&self) {
        self.shared.stop(false);
    }
    pub fn stop_handle(&self) -> StopHandle {
        StopHandle {
            shared: self.shared.clone(),
        }
    }
    pub fn poll(&self) -> Option<Update> {
        self.shared.lock().update.take()
    }
    pub fn shutdown(&self) {
        self.shared.stop(true);
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// The native stream is constructed, used, and dropped on this one thread. It
// need not implement Send. Tests inject a real queue with a headless callback.
pub(crate) trait Device {
    fn feed(&mut self) -> &mut Feed;
    fn start(&mut self) -> Result<(), String>;
    fn pause(&mut self) -> Result<(), String>;
    fn check_route(&self) -> Result<(), String>;
    fn pop_report(&mut self) -> Option<DeviceReport>;
    fn now_ns(&self) -> Option<u64>;
    fn dropped_reports(&self) -> u64;
    fn error_flags(&self) -> u64;
}
type Factory = Box<dyn FnMut() -> Result<Box<dyn Device>, String> + Send>;

#[cfg(target_os = "macos")]
impl Device for deadpan_output::DeviceOutput {
    fn feed(&mut self) -> &mut Feed {
        self.feed()
    }
    fn start(&mut self) -> Result<(), String> {
        self.start_device().map_err(|e| e.to_string())
    }
    fn pause(&mut self) -> Result<(), String> {
        self.pause_device().map_err(|e| e.to_string())
    }
    fn check_route(&self) -> Result<(), String> {
        self.check_route().map_err(|e| e.to_string())
    }
    fn pop_report(&mut self) -> Option<DeviceReport> {
        self.pop_report()
    }
    fn now_ns(&self) -> Option<u64> {
        self.clock_now_ns()
    }
    fn dropped_reports(&self) -> u64 {
        self.dropped_reports()
    }
    fn error_flags(&self) -> u64 {
        self.error_flags()
    }
}
#[cfg(target_os = "macos")]
fn open_device() -> Result<Box<dyn Device>, String> {
    deadpan_output::DeviceOutput::open_default()
        .map(|device| Box::new(device) as Box<dyn Device>)
        .map_err(|e| e.to_string())
}
#[cfg(not(target_os = "macos"))]
fn open_device() -> Result<Box<dyn Device>, String> {
    Err("native audition output requires macOS".into())
}

struct Active {
    job: Arc<Job>,
    device: Box<dyn Device>,
    generation: Generation,
    token: StopToken,
    batch: Option<Batch>,
    offset: usize,
    clock: Option<DeliveryClock>,
    activated: bool,
    queued: usize,
    finished: bool,
    terminal_report: bool,
    last_sample: Option<AudioSample>,
    last_report: Instant,
    last_clock: Option<(u64, Instant)>,
    last_route: Instant,
    uncovered: Option<Instant>,
}

impl Active {
    fn open(job: Arc<Job>, device: Box<dyn Device>, shared: &Shared) -> Result<Self, String> {
        let mut device = device;
        let generation = device
            .feed()
            .restart(job.start.0)
            .map_err(|e| e.to_string())?;
        let token = device
            .feed()
            .stop_token(generation)
            .map_err(|e| e.to_string())?;
        let mut state = shared.lock();
        if job.cancelled.load(Ordering::Acquire)
            || state.shutdown
            || !state
                .current
                .as_ref()
                .is_some_and(|current| current.epoch == job.epoch)
        {
            token.stop();
            return Err("playback request was superseded".into());
        }
        state.token = Some(token.clone());
        state.prep = Some(job.clone());
        drop(state);
        shared.wake.notify_all();
        shared.publish(
            &job,
            job.update(Phase::Preparing, Some(generation), None, None),
        );
        let now = Instant::now();
        Ok(Self {
            job,
            device,
            generation,
            token,
            batch: None,
            offset: 0,
            clock: None,
            activated: false,
            queued: 0,
            finished: false,
            terminal_report: false,
            last_sample: None,
            last_report: now,
            last_clock: None,
            last_route: now,
            uncovered: None,
        })
    }

    fn stop(&mut self) {
        self.job.cancelled.store(true, Ordering::Release);
        self.token.stop();
        // Native pause failure is reported by the caller only when no stronger
        // failure already explains this terminated generation.
    }

    fn tick(&mut self, shared: &Shared) -> Result<Option<Phase>, String> {
        if self.token.is_stopped() {
            return Ok(Some(Phase::Stopped));
        }
        if self.device.error_flags() != 0 {
            return Err("audio device faulted".into());
        }
        if self.device.dropped_reports() != 0 {
            return Err("audio delivery reports were lost".into());
        }
        if self.last_route.elapsed() >= Duration::from_millis(250) {
            self.device.check_route()?;
            self.last_route = Instant::now();
        }
        // Always drain, including old preparation/paused and terminal records.
        // Only this active generation contributes to the heard-position clock.
        for _ in 0..256 {
            let Some(report) = self.device.pop_report() else {
                break;
            };
            if report.render.generation != self.generation || !self.activated {
                continue;
            }
            if report.render.status == RenderStatus::Fault {
                return Err("audio output has faulted".into());
            }
            if self.terminal_report {
                continue;
            }
            self.last_report = Instant::now();
            let terminal = matches!(
                report.render.status,
                RenderStatus::Ended | RenderStatus::Starved
            );
            let clock = self
                .clock
                .as_mut()
                .ok_or("active playback has no delivery clock")?;
            clock.observe(report).map_err(|e| e.to_string())?;
            if terminal {
                self.terminal_report = true;
                // Stop producing, but retain already submitted prefix timing.
                // This is separate from external generation revocation.
                shared.lock().prep = None;
                self.job.cancelled.store(true, Ordering::Release);
                shared.wake.notify_all();
            }
        }
        if self.device.pop_report().is_some() {
            return Err("audio report drain exceeded its bounded inventory".into());
        }
        if !self.terminal_report {
            self.supply(shared)?;
        }
        if self.activated {
            let now = self
                .device
                .now_ns()
                .ok_or("audio stream clock is unavailable")?;
            if let Some((previous, progressed)) = self.last_clock {
                if now < previous {
                    return Err("audio stream clock moved backwards".into());
                }
                if now == previous && progressed.elapsed() > Duration::from_secs(1) {
                    return Err("audio stream clock stopped advancing".into());
                }
                if now > previous {
                    self.last_clock = Some((now, Instant::now()));
                }
            } else {
                self.last_clock = Some((now, Instant::now()));
            }
            if !self.terminal_report && self.last_report.elapsed() > Duration::from_secs(1) {
                return Err("audio device stopped reporting delivery".into());
            }
            match self
                .clock
                .as_ref()
                .ok_or("missing delivery clock")?
                .position(now)
            {
                ClockPosition::Content { sample } => {
                    self.uncovered = None;
                    self.last_sample = Some(AudioSample(sample));
                    shared.publish(
                        &self.job,
                        self.job.update(
                            Phase::Playing,
                            Some(self.generation),
                            self.last_sample,
                            None,
                        ),
                    );
                }
                ClockPosition::Terminal {
                    status: RenderStatus::Ended,
                    end_sample,
                } => {
                    self.last_sample = Some(AudioSample(end_sample));
                    return Ok(Some(Phase::Ended));
                }
                ClockPosition::Terminal { status, end_sample } => {
                    self.last_sample = Some(AudioSample(end_sample));
                    return Err(format!("audio output stopped: {status:?}"));
                }
                position @ (ClockPosition::Gap { .. } | ClockPosition::Pending) => {
                    // No known delivered interval: hold picture position.
                    let limit = if matches!(position, ClockPosition::Pending) {
                        Duration::from_secs(2)
                    } else {
                        Duration::from_millis(250)
                    };
                    if self.uncovered.get_or_insert_with(Instant::now).elapsed() > limit {
                        return Err("audio delivery clock has no covering content interval".into());
                    }
                }
            }
        }
        Ok(None)
    }

    fn supply(&mut self, shared: &Shared) -> Result<(), String> {
        if self.batch.is_none() && !self.finished {
            let reply = shared.lock().reply.take();
            if let Some(reply) = reply {
                shared.wake.notify_all();
                match reply {
                    Reply::Batch(batch) if batch.epoch == self.job.epoch => {
                        if self.clock.is_none() {
                            self.clock = Some(
                                DeliveryClock::new(self.generation, self.job.start.0, batch.end.0)
                                    .map_err(|e| e.to_string())?,
                            );
                        }
                        if batch.start.0 != self.device.feed().next_sample() {
                            return Err("prepared PCM is discontinuous".into());
                        }
                        self.batch = Some(batch);
                        self.offset = 0;
                    }
                    Reply::Failed { epoch, error } if epoch == self.job.epoch => return Err(error),
                    _ => {}
                }
            }
        }
        if let Some(batch) = &self.batch {
            while self.offset < batch.samples.len() {
                let end = (self.offset + deadpan_output::PACKET_FRAMES).min(batch.samples.len());
                match self
                    .device
                    .feed()
                    .submit(self.generation, &batch.samples[self.offset..end])
                {
                    Ok(()) => {
                        self.queued += end - self.offset;
                        self.offset = end;
                    }
                    Err(FeedError::Full) => break,
                    Err(error) => return Err(error.to_string()),
                }
            }
            if self.offset == batch.samples.len() {
                if batch.eos {
                    match self.device.feed().finish(self.generation) {
                        Ok(()) => self.finished = true,
                        Err(FeedError::Full) => return Ok(()),
                        Err(error) => return Err(error.to_string()),
                    }
                }
                self.batch = None;
            }
        }
        if !self.activated && (self.queued >= preparation::BATCH_FRAMES || self.finished) {
            self.device
                .feed()
                .activate(self.generation)
                .map_err(|e| e.to_string())?;
            self.device.start()?;
            self.activated = true;
            self.last_report = Instant::now();
        }
        Ok(())
    }
}

fn run(shared: Arc<Shared>, mut factory: Factory) {
    let mut active: Option<Active> = None;
    loop {
        let (intent, shutdown) = {
            let mut state = shared.lock();
            (state.intent.take(), state.shutdown)
        };
        if shutdown {
            if let Some(mut old) = active.take() {
                old.stop();
                let _ = old.device.pause();
            }
            return;
        }
        if let Some(intent) = intent {
            if let Some(mut old) = active.take() {
                old.stop();
                let pause_error = old.device.pause().err();
                if matches!(intent, Intent::Stop) {
                    let phase = if pause_error.is_some() {
                        Phase::Failed
                    } else {
                        Phase::Stopped
                    };
                    shared.publish(
                        &old.job,
                        old.job
                            .update(phase, Some(old.generation), old.last_sample, pause_error),
                    );
                }
            } else if matches!(intent, Intent::Stop) {
                let job = shared.lock().current.clone();
                if let Some(job) = job {
                    shared.publish(&job, job.update(Phase::Stopped, None, None, None));
                }
            }
            if let Intent::Play(job) = intent {
                if job.cancelled.load(Ordering::Acquire) {
                    continue;
                }
                match factory().and_then(|device| Active::open(job.clone(), device, &shared)) {
                    Ok(new) => active = Some(new),
                    Err(error) => {
                        shared.publish(&job, job.update(Phase::Failed, None, None, Some(error)))
                    }
                }
            }
        }
        if let Some(current) = &mut active {
            let result = current.tick(&shared);
            match result {
                Ok(None) => {}
                result => {
                    current.stop();
                    let pause_error = current.device.pause().err();
                    let (phase, error) = match result {
                        Ok(Some(phase)) if pause_error.is_none() => (phase, None),
                        Ok(_) => (Phase::Failed, pause_error),
                        Err(error) => (Phase::Failed, Some(error)),
                    };
                    shared.publish(
                        &current.job,
                        current.job.update(
                            phase,
                            Some(current.generation),
                            current.last_sample,
                            error,
                        ),
                    );
                    active = None;
                    shared.wake.notify_all();
                }
            }
        }
        let state = shared.lock();
        if state.intent.is_none() && !state.shutdown {
            let timeout = if active.is_some() {
                Duration::from_millis(2)
            } else {
                Duration::from_millis(20)
            };
            drop(
                shared
                    .wake
                    .wait_timeout(state, timeout)
                    .unwrap_or_else(|error| error.into_inner()),
            );
        }
    }
}
