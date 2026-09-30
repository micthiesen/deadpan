//! Bounded canonical PCM reads for one immutable offline output range.
//!
//! This uses the same limited authored bus as audition. It adds no monitoring
//! gain, source normalization, output padding, or replacement for unsupported
//! processing. Full voice effects and encoded-master qualification remain open.

use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_audio::LimitedAudioBlock;
use deadpan_core::{AudioSample, FrameRange, FrameRate, ProjectDocument, ProjectId, RevisionId};
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};

use super::{ProjectAudioError, ProjectAudioSession};

pub const MAX_OFFLINE_AUDIO_FRAMES: u32 = 8_192;
// The canonical audio engine admits at most sixty seconds of preparation for
// one read. This bound may shorten a read but can never extend the job deadline.
const MAX_READ_TIME: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum OfflineAudioError {
    #[error("offline audio preparation was cancelled")]
    Cancelled,
    #[error("offline audio preparation exceeded its shared monotonic deadline")]
    Deadline,
    #[error("offline audio range must stay inside the captured project interval")]
    Range,
    #[error("offline audio block differs from its captured revision or sample interval")]
    BlockMismatch,
    #[error(transparent)]
    Project(#[from] ProjectAudioError),
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
}

/// A fixed project revision and exact output sample interval. Preparation
/// keeps its real limiter/source halo on the absolute project grid. Only the
/// requested interval is returned, and no later read can renew the job budget.
pub struct OfflineAudioSession {
    session: ProjectAudioSession,
    range: FrameRange,
    samples: Range<AudioSample>,
    deadline: Instant,
}

impl OfflineAudioSession {
    pub fn open_revision(
        path: &Path,
        revision: &RevisionId,
        range: FrameRange,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Self, OfflineAudioError> {
        check_control(cancelled, deadline)?;
        let store = ProjectStore::open(path, AccessMode::ReadOnly);
        check_control(cancelled, deadline)?;
        let store = store.map_err(ProjectAudioError::from)?;
        let document = store.snapshot_at(revision);
        check_control(cancelled, deadline)?;
        let document = document.map_err(ProjectAudioError::from)?;
        let plan = RenderPlan::compile(&document);
        check_control(cancelled, deadline)?;
        let plan = plan.map_err(ProjectAudioError::from)?;
        if range.start().0 < 0
            || range.duration().frames() == 0
            || range.end().0 > plan.duration().frames()
        {
            return Err(OfflineAudioError::Range);
        }
        let rate = document.presentation_basis().frame_rate;
        let samples = rate.audio_boundary(range.start())?..rate.audio_boundary(range.end())?;
        let mut session = ProjectAudioSession::from_plan(store, document, plan);
        session.sources.deadline = Some(deadline);
        check_control(cancelled, deadline)?;
        Ok(Self {
            session,
            range,
            samples,
            deadline,
        })
    }

    pub fn project_id(&self) -> &ProjectId {
        self.session.sources.document.project_id()
    }

    pub fn revision(&self) -> &RevisionId {
        self.session.revision()
    }

    pub const fn range(&self) -> FrameRange {
        self.range
    }

    pub fn frame_rate(&self) -> FrameRate {
        self.session
            .sources
            .document
            .presentation_basis()
            .frame_rate
    }

    pub fn sample_range(&self) -> Range<AudioSample> {
        self.samples.clone()
    }

    pub fn sample_count(&self) -> u64 {
        // Both boundaries are nonnegative, monotonically rounded i64 values.
        self.samples.end.0.abs_diff(self.samples.start.0)
    }

    /// Retains the caller's original absolute deadline, including time spent
    /// opening the store, compiling the plan, hashing and between read calls.
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }

    /// The complete immutable snapshot used by this audio session. A worker can
    /// compare its bounded serialization/hash with the captured picture intent;
    /// this reference grants no authored-state mutation or alternate admission.
    pub fn document(&self) -> &ProjectDocument {
        &self.session.sources.document
    }

    /// Read up to 8192 stereo samples at an absolute project coordinate. This
    /// does not advance a hidden cursor or rebase PCM. The encoder separately
    /// derives output PTS by subtracting `sample_range().start`.
    pub fn read(
        &mut self,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<LimitedAudioBlock, OfflineAudioError> {
        check_control(cancelled, self.deadline)?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(OfflineAudioError::Range)?;
        if frames == 0
            || frames > MAX_OFFLINE_AUDIO_FRAMES
            || start < self.samples.start
            || end > self.samples.end.0
        {
            return Err(OfflineAudioError::Range);
        }
        let read_deadline = self.deadline.min(Instant::now() + MAX_READ_TIME);
        self.session.sources.deadline = Some(read_deadline);
        let remaining = read_deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(OfflineAudioError::Deadline)?;
        let result = self.session.limited.read(
            &mut self.session.sources,
            start,
            frames,
            remaining,
            cancelled,
        );
        // A downstream call can end with its own error or a cached result after
        // cancellation/deadline. Neither may publish a late block.
        check_control(cancelled, self.deadline)?;
        let block = result.map_err(ProjectAudioError::from)?;
        if block.project_id != *self.project_id()
            || block.revision_id != *self.revision()
            || block.start != start
            || block.samples.len()
                != usize::try_from(frames).map_err(|_| OfflineAudioError::Range)?
            || block
                .samples
                .iter()
                .flatten()
                .any(|sample| !sample.is_finite())
        {
            return Err(OfflineAudioError::BlockMismatch);
        }
        check_control(cancelled, self.deadline)?;
        Ok(block)
    }
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), OfflineAudioError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(OfflineAudioError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(OfflineAudioError::Deadline);
    }
    Ok(())
}
