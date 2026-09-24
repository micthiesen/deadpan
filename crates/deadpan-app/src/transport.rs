//! Audio-clock admission and picture scheduling without a window or device.

use deadpan_core::{AudioSample, FrameRate, ProjectFrame, ProjectId, RevisionId};
use deadpan_output::Generation;
use deadpan_playback::{Phase, Update};

pub struct Run {
    pub ticket: u64,
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
    pub rate: FrameRate,
    pub frames: i64,
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
    frame: u64,
    sample: AudioSample,
}

impl Resume {
    pub fn matches(&self, update: &Update) -> bool {
        self.ticket == update.ticket
            && self.session == update.session
            && self.project == update.project_id
            && self.revision == update.revision_id
    }
    pub fn sample_for(
        &self,
        session: u64,
        project: &ProjectId,
        revision: &RevisionId,
        frame: u64,
    ) -> Option<AudioSample> {
        (self.session == session
            && &self.project == project
            && &self.revision == revision
            && self.frame == frame)
            .then_some(self.sample)
    }
}

impl Run {
    pub fn resume(&self, frame: u64) -> Resume {
        Resume {
            ticket: self.ticket,
            session: self.session,
            project: self.project.clone(),
            revision: self.revision.clone(),
            frame,
            sample: self.sample,
        }
    }
    /// Admit only this immutable playback request. An old callback cannot move
    /// the cursor after stop, seek, reopen, edit or a new device generation.
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
        let frame = frame_at_sample(self.rate, self.frames, sample)?;
        if update.phase == Phase::Ended && frame != self.frames as u64 {
            return Err("Playback ended before the sequence boundary.".into());
        }
        self.sample = sample;
        self.phase = update.phase;
        if update.generation.is_some() {
            self.generation = update.generation;
        }
        Ok(Some(frame))
    }

    /// Keep one decode/GPU submission in flight and coalesce intervening audio
    /// positions. Never repeatedly cancel a useful decode at the audio cadence.
    pub fn picture(&mut self, frame: u64, pipeline_busy: bool) -> Option<Generation> {
        if self.phase != Phase::Playing || pipeline_busy || self.requested_frame == Some(frame) {
            return None;
        }
        let generation = self.generation?;
        self.requested_frame = Some(frame);
        Some(generation)
    }

    pub fn new(
        ticket: u64,
        session: u64,
        project: ProjectId,
        revision: RevisionId,
        rate: FrameRate,
        frames: i64,
        sample: AudioSample,
    ) -> Self {
        Self {
            ticket,
            session,
            project,
            revision,
            rate,
            frames,
            sample,
            generation: None,
            phase: Phase::Preparing,
            requested_frame: None,
        }
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
