//! Coalesced idle preparation on the existing PCM worker. No device ownership.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use deadpan_audio::{PreparationActivity, PreparationMonitor};
use deadpan_core::AudioSample;

use crate::{ContentIdentity, Engine, Snapshot, Target};

/// A stopped cursor may prepare at most thirty seconds ahead. This is an
/// output horizon, not a promise about the cost of a continuous nested stage.
pub const WARM_HORIZON_FRAMES: i64 = 30 * 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarmTicket(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarmStatus {
    Queued,
    Preparing,
    Complete,
    Interrupted,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct WarmUpdate {
    pub ticket: WarmTicket,
    pub status: WarmStatus,
    /// Examined output horizon. LRU caches may retain only part of it; this is
    /// never a readiness token or a substitute for current source admission.
    pub prepared_until: AudioSample,
    pub activity: PreparationActivity,
    pub elapsed_millis: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WarmRequestError {
    #[error("audio warming requires a nonnegative cursor in committed content")]
    InvalidTarget,
    #[error("audio warming waits until playback is quiescent")]
    PlaybackActive,
    #[error("playback engine has shut down")]
    Shutdown,
    #[error("audio warming request identities are exhausted")]
    Exhausted,
}

pub(crate) struct Job {
    pub ticket: WarmTicket,
    pub snapshot: Arc<Snapshot>,
    pub target: Target,
    pub start: AudioSample,
    pub cancelled: AtomicBool,
    pub promoted: AtomicBool,
    pub monitor: Arc<PreparationMonitor>,
    started: Instant,
}

#[derive(Default)]
pub(crate) struct State {
    serial: u64,
    current: Option<Arc<Job>>,
    pending: Option<Arc<Job>>,
    running: Option<Arc<Job>>,
    reply: Option<WarmUpdate>,
}

impl State {
    pub(crate) fn saved_audio_status(&self) -> Option<deadpan_audio::SavedAudioStatus> {
        self.current
            .as_ref()
            .and_then(|job| job.monitor.saved_audio_status())
    }

    pub(crate) fn take_pending(&mut self) -> Option<Arc<Job>> {
        let job = self.pending.take()?;
        if job.cancelled.load(Ordering::Acquire) {
            return None;
        }
        self.running = Some(Arc::clone(&job));
        if let Some(reply) = &mut self.reply {
            reply.status = WarmStatus::Preparing;
        }
        Some(job)
    }

    pub(crate) fn interrupt(&mut self, reason: &str) {
        if let Some(job) = &self.current {
            job.cancelled.store(true, Ordering::Release);
        }
        if let Some(job) = &self.running {
            job.cancelled.store(true, Ordering::Release);
        }
        self.pending = None;
        if let Some(reply) = &mut self.reply {
            reply.status = WarmStatus::Interrupted;
            reply.error = Some(reason.into());
        }
    }

    /// Matching playback finishes only the currently executing canonical read.
    /// Its complete stage cache survives; no second cold processor is started.
    pub(crate) fn promote(
        &mut self,
        snapshot: &Snapshot,
        target: &Target,
        start: AudioSample,
    ) -> Option<Arc<Job>> {
        let matching = self
            .running
            .as_ref()
            .filter(|job| {
                !job.cancelled.load(Ordering::Acquire)
                    && job.start == start
                    && job.target == *target
                    && job.snapshot.same_inputs(snapshot)
            })
            .cloned();
        if let Some(job) = &matching {
            job.promoted.store(true, Ordering::Release);
            self.pending = None;
        } else {
            self.interrupt("Audio warming interrupted by audition");
        }
        matching
    }
}

impl Engine {
    /// Prepare a bounded horizon while output is quiescent. A new request
    /// replaces pending work. It never opens an output device or moves a clock.
    pub fn warm_audio(
        &self,
        snapshot: Arc<Snapshot>,
        target: Target,
        start: AudioSample,
    ) -> Result<WarmTicket, WarmRequestError> {
        if start.0 < 0 || snapshot.content != ContentIdentity::Committed {
            return Err(WarmRequestError::InvalidTarget);
        }
        let mut state = self.shared.lock();
        if state.shutdown() {
            return Err(WarmRequestError::Shutdown);
        }
        if !state.can_analyze() {
            return Err(WarmRequestError::PlaybackActive);
        }
        let serial = state
            .warming
            .serial
            .checked_add(1)
            .ok_or(WarmRequestError::Exhausted)?;
        state
            .warming
            .interrupt("Audio warming replaced by a newer cursor");
        let job = Arc::new(Job {
            ticket: WarmTicket(serial),
            snapshot,
            target,
            start,
            cancelled: AtomicBool::new(false),
            promoted: AtomicBool::new(false),
            monitor: Arc::new(PreparationMonitor::new(
                PreparationActivity::PreparingOutput,
            )),
            started: Instant::now(),
        });
        state.warming.serial = serial;
        state.warming.reply = Some(WarmUpdate {
            ticket: job.ticket,
            status: WarmStatus::Queued,
            prepared_until: start,
            activity: PreparationActivity::CheckingPlan,
            elapsed_millis: 0,
            error: None,
        });
        state.warming.pending = Some(Arc::clone(&job));
        state.warming.current = Some(job);
        drop(state);
        self.shared.wake.notify_all();
        Ok(WarmTicket(serial))
    }

    pub fn cancel_warming(&self) {
        self.shared
            .lock()
            .warming
            .interrupt("Audio warming cancelled");
        self.shared.wake.notify_all();
    }

    pub fn audio_warming(&self, ticket: WarmTicket) -> Option<WarmUpdate> {
        let state = self.shared.lock();
        let mut update = state.warming.reply.as_ref()?.clone();
        if update.ticket != ticket {
            return None;
        }
        if let Some(job) = &state.warming.current {
            update.activity = job.monitor.activity();
            if matches!(update.status, WarmStatus::Queued | WarmStatus::Preparing) {
                update.elapsed_millis =
                    u64::try_from(job.started.elapsed().as_millis()).unwrap_or(u64::MAX) / 100
                        * 100;
            }
        }
        Some(update)
    }
}

impl crate::controller::Shared {
    pub(crate) fn publish_warming(
        &self,
        job: &Job,
        status: WarmStatus,
        prepared_until: AudioSample,
        error: Option<String>,
    ) {
        let mut state = self.lock();
        if !matches!(status, WarmStatus::Preparing)
            && state
                .warming
                .running
                .as_ref()
                .is_some_and(|current| current.ticket == job.ticket)
        {
            state.warming.running = None;
        }
        if state.shutdown()
            || job.cancelled.load(Ordering::Acquire)
            || !state
                .warming
                .current
                .as_ref()
                .is_some_and(|current| current.ticket == job.ticket)
        {
            return;
        }
        state.warming.reply = Some(WarmUpdate {
            ticket: job.ticket,
            status,
            prepared_until,
            activity: job.monitor.activity(),
            elapsed_millis: u64::try_from(job.started.elapsed().as_millis()).unwrap_or(u64::MAX)
                / 100
                * 100,
            error,
        });
        drop(state);
        self.repaint();
    }
}
