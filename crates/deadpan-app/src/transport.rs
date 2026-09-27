//! Audio-clock admission and picture scheduling without a native window or device.

use std::sync::Arc;

use deadpan_core::{AudioSample, FrameRate, ProjectFrame, ProjectId, RevisionId};
use deadpan_output::Generation;
use deadpan_playback::{Original, Phase, Sound, Update, Window};

/// The immutable clock and picture identity captured when audition starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Domain {
    Sequence { rate: FrameRate, frames: i64 },
    Original(Arc<Original>),
    Sound(Arc<Sound>),
}

impl Domain {
    pub fn is_sound(&self) -> bool {
        matches!(self, Self::Sound(_))
    }

    pub fn end(&self) -> Result<AudioSample, String> {
        match self {
            Self::Sequence { rate, frames } => {
                if *frames < 0 {
                    return Err("Playback duration must be nonnegative.".into());
                }
                rate.audio_boundary(ProjectFrame(*frames))
                    .map_err(|error| error.to_string())
            }
            Self::Original(original) => Ok(original.end()),
            Self::Sound(sound) => Ok(sound.duration_samples()),
        }
    }

    pub fn sample_at_boundary(&self, frame: u64) -> Result<AudioSample, String> {
        match self {
            Self::Sequence { rate, frames } => {
                let frame = i64::try_from(frame)
                    .map_err(|_| "Playback frame is not representable.".to_owned())?;
                if frame > *frames {
                    return Err("Playback frame exceeds the sequence.".into());
                }
                rate.audio_boundary(ProjectFrame(frame))
                    .map_err(|error| error.to_string())
            }
            Self::Original(original) => original.sample_at_boundary(frame),
            Self::Sound(sound) => {
                let sample = i64::try_from(frame).map_err(|e| e.to_string())?;
                if sample > sound.duration_samples().0 {
                    return Err("Sound cursor exceeds its measured duration.".into());
                }
                Ok(AudioSample(sample))
            }
        }
    }

    pub fn frame_at_sample(&self, sample: AudioSample) -> Result<u64, String> {
        match self {
            Self::Sequence { rate, frames } => frame_at_sample(*rate, *frames, sample),
            Self::Original(original) => original.frame_at_sample(sample),
            Self::Sound(sound) => {
                if sample.0 < 0 || sample > sound.duration_samples() {
                    return Err("Sound position exceeds its measured duration.".into());
                }
                u64::try_from(sample.0).map_err(|e| e.to_string())
            }
        }
    }

    pub fn sample_at_selection_boundary(&self, frame: u64) -> Result<AudioSample, String> {
        match self {
            Self::Sequence { .. } | Self::Sound(_) => self.sample_at_boundary(frame),
            Self::Original(original) => original.sample_at_selection_boundary(frame),
        }
    }

    pub fn selection_window(
        &self,
        range: std::ops::Range<u64>,
        lead: AudioSample,
        follow: AudioSample,
    ) -> Result<Window, String> {
        if range.start >= range.end || lead.0 < 0 || follow.0 < 0 {
            return Err("Select a nonempty range and nonnegative loop context.".into());
        }
        let start = self.sample_at_selection_boundary(range.start)?.0;
        let end = self.sample_at_selection_boundary(range.end)?.0;
        // Clamp before addition, including arbitrarily large explicit context.
        let start = start - lead.0.min(start);
        let end = end + follow.0.min(self.end()?.0 - end);
        Window::new(AudioSample(start), AudioSample(end), true).map_err(|e| e.to_string())
    }
}

pub struct Identity {
    pub ticket: u64,
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
}

/// A cursor may name the excluded terminal boundary while the picture remains
/// the final included frame. Both coordinates belong to the captured domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub cursor: u64,
    pub picture: u64,
}

pub struct Run {
    pub ticket: u64,
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
    domain: Domain,
    window: Window,
    /// Monotonic device delivery coordinate. In a loop this can exceed the
    /// window's end; use `content_sample` for the position in the source/edit.
    pub sample: AudioSample,
    pub generation: Option<Generation>,
    pub phase: Phase,
    requested_frame: Option<u64>,
}

/// A paused device estimate survives only while its editorial location is
/// unchanged. Resume retains its exact sample, including the fractional frame.
pub struct Resume {
    ticket: u64,
    session: u64,
    project: ProjectId,
    revision: RevisionId,
    domain: Domain,
    window: Window,
    frame: u64,
    sample: AudioSample,
    generation: Option<Generation>,
}

impl Resume {
    pub fn domain(&self) -> &Domain {
        &self.domain
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn matches(&self, update: &Update) -> bool {
        self.ticket == update.ticket
            && self.session == update.session
            && self.project == update.project_id
            && self.revision == update.revision_id
            && !matches!((self.generation, update.generation), (Some(previous), Some(next)) if previous != next)
    }
    #[cfg(test)]
    pub fn sample_for(
        &self,
        session: u64,
        project: &ProjectId,
        revision: &RevisionId,
        frame: u64,
    ) -> Option<AudioSample> {
        if !matches!(self.domain, Domain::Sequence { .. }) || self.window.looping() {
            return None;
        }
        self.sample_for_domain(
            session,
            project,
            revision,
            &self.domain,
            &self.window,
            frame,
        )
    }

    pub fn sample_for_domain(
        &self,
        session: u64,
        project: &ProjectId,
        revision: &RevisionId,
        domain: &Domain,
        window: &Window,
        frame: u64,
    ) -> Option<AudioSample> {
        let content = window.sample(self.sample)?;
        (self.session == session
            && &self.project == project
            && &self.revision == revision
            && &self.domain == domain
            && &self.window == window
            && self.frame == frame
            && domain.frame_at_sample(content).ok() == Some(frame))
        .then_some(self.sample)
    }
}

impl Run {
    pub fn with_domain(
        identity: Identity,
        domain: Domain,
        window: Window,
        delivery_start: AudioSample,
    ) -> Result<Self, String> {
        if window.start() >= window.end() {
            return Err("Playback requires a nonempty window.".into());
        }
        if window.end() > domain.end()? {
            return Err("Playback window exceeds its captured domain.".into());
        }
        let run = Self {
            ticket: identity.ticket,
            session: identity.session,
            project: identity.project,
            revision: identity.revision,
            domain,
            window,
            sample: delivery_start,
            generation: None,
            phase: Phase::Preparing,
            requested_frame: None,
        };
        run.position()?;
        Ok(run)
    }

    pub fn domain(&self) -> &Domain {
        &self.domain
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn content_sample(&self) -> Result<AudioSample, String> {
        self.window
            .sample(self.sample)
            .ok_or_else(|| "Playback position exceeds its captured window.".into())
    }

    pub fn lap(&self) -> Result<u64, String> {
        self.window
            .lap(self.sample)
            .ok_or_else(|| "Playback position exceeds its captured window.".into())
    }

    pub fn position(&self) -> Result<Position, String> {
        self.position_at(self.sample)
    }

    pub fn picture_frame(&self) -> Result<u64, String> {
        if self.domain.is_sound() {
            return Err("Sound audition has no picture.".into());
        }
        Ok(self.position()?.picture)
    }

    fn position_at(&self, delivery: AudioSample) -> Result<Position, String> {
        let content = self
            .window
            .sample(delivery)
            .ok_or_else(|| "Playback position exceeds its captured window.".to_owned())?;
        let cursor = self.domain.frame_at_sample(content)?;
        let picture =
            if !self.window.looping() && content == self.window.end() {
                let picture_sample =
                    AudioSample(content.0.checked_sub(1).ok_or_else(|| {
                        "Playback picture boundary is not representable.".to_owned()
                    })?);
                self.domain.frame_at_sample(picture_sample)?
            } else {
                cursor
            };
        Ok(Position { cursor, picture })
    }

    pub fn resume(&self, frame: u64) -> Resume {
        Resume {
            ticket: self.ticket,
            session: self.session,
            project: self.project.clone(),
            revision: self.revision.clone(),
            domain: self.domain.clone(),
            window: self.window,
            frame,
            sample: self.sample,
            generation: self.generation,
        }
    }
    /// Admit only this immutable playback request. An old callback cannot move
    /// the cursor after stop, seek, reopen, edit or a new device generation.
    /// The returned coordinate belongs to this run's captured domain.
    pub fn receive(&mut self, update: &Update) -> Result<Option<u64>, String> {
        if self.ticket != update.ticket
            || self.session != update.session
            || self.project != update.project_id
            || self.revision != update.revision_id
        {
            return Ok(None);
        }
        if let Some(generation) = self.generation
            && update.generation.is_some_and(|next| next != generation)
        {
            return Ok(None);
        }
        if matches!(update.phase, Phase::Playing | Phase::Ended) && update.generation.is_none() {
            return Err("Playback has no admitted output generation.".into());
        }
        let sample = update.sample.unwrap_or(self.sample);
        if sample < self.sample {
            return Err("The playback clock moved backwards. Audition stopped.".into());
        }
        let position = self.position_at(sample)?;
        if update.phase == Phase::Ended {
            if self.window.looping() {
                return Err("Looping playback ended unexpectedly.".into());
            }
            if update.sample != Some(self.window.end()) {
                return Err("Playback ended before its window boundary.".into());
            }
        }
        self.sample = sample;
        self.phase = update.phase;
        if update.generation.is_some() {
            self.generation = update.generation;
        }
        Ok(Some(position.cursor))
    }

    /// Keep one decode/GPU submission in flight and coalesce intervening audio
    /// positions. Never repeatedly cancel a useful decode at the audio cadence.
    pub fn picture(&mut self, frame: u64, pipeline_busy: bool) -> Option<Generation> {
        if self.domain.is_sound()
            || self.phase != Phase::Playing
            || pipeline_busy
            || self.requested_frame == Some(frame)
        {
            return None;
        }
        let generation = self.generation?;
        self.requested_frame = Some(frame);
        Some(generation)
    }

    #[cfg(test)]
    pub fn new(
        ticket: u64,
        session: u64,
        project: ProjectId,
        revision: RevisionId,
        rate: FrameRate,
        frames: i64,
        sample: AudioSample,
    ) -> Self {
        let domain = Domain::Sequence { rate, frames };
        let window = Window::new(AudioSample(0), domain.end().unwrap(), false).unwrap();
        Self::with_domain(
            Identity {
                ticket,
                session,
                project,
                revision,
            },
            domain,
            window,
            sample,
        )
        .expect("valid sequence transport fixture")
    }
}

/// Invert the exact origin-based ties-to-even allocation. Floating point fps
/// multiplication picks the wrong picture near NTSC audio boundaries.
pub fn frame_at_sample(rate: FrameRate, frames: i64, sample: AudioSample) -> Result<u64, String> {
    if frames < 0 || sample.0 < 0 {
        return Err("Playback position must be nonnegative.".into());
    }
    let boundary = |frame| {
        rate.audio_boundary(ProjectFrame(frame))
            .map_err(|error| error.to_string())
    };
    let end = boundary(frames)?;
    if sample > end {
        return Err("Playback position exceeds the sequence.".into());
    }
    if sample == end {
        return Ok(frames as u64);
    }
    let (mut low, mut high) = (0_i64, frames);
    while low < high {
        let mid = low + (high - low) / 2 + (high - low) % 2;
        if boundary(mid)? <= sample {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    Ok(low as u64)
}

#[cfg(test)]
mod tests;
