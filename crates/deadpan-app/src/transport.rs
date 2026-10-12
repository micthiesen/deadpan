//! Audio-clock admission and picture scheduling without a native window or device.

use std::sync::Arc;

use deadpan_core::{AudioSample, FrameRate, ProjectFrame, ProjectId, RevisionId};
use deadpan_output::Generation;
use deadpan_playback::{ContentIdentity, Original, Phase, Sound, Update, Window};

/// The immutable clock and picture identity captured when audition starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Domain {
    Sequence { rate: FrameRate, frames: i64 },
    Original(Arc<Original>),
    Sound(Arc<Sound>),
    AudioRange(Arc<deadpan_playback::AudioRange>),
}

impl Domain {
    pub fn is_sound(&self) -> bool {
        matches!(self, Self::Sound(_))
    }

    pub fn is_audio_only(&self) -> bool {
        matches!(self, Self::Sound(_) | Self::AudioRange(_))
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
            Self::AudioRange(range) => Ok(range.duration_samples()),
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
            Self::AudioRange(range) => {
                let sample = i64::try_from(frame).map_err(|e| e.to_string())?;
                if sample > range.duration_samples().0 {
                    return Err("Source range cursor exceeds its selected duration.".into());
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
            Self::AudioRange(range) => {
                if sample.0 < 0 || sample > range.duration_samples() {
                    return Err("Source range position exceeds its selected duration.".into());
                }
                u64::try_from(sample.0).map_err(|e| e.to_string())
            }
        }
    }

    pub fn sample_at_selection_boundary(&self, frame: u64) -> Result<AudioSample, String> {
        match self {
            Self::Sequence { .. } | Self::Sound(_) | Self::AudioRange(_) => {
                self.sample_at_boundary(frame)
            }
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
    pub content: ContentIdentity,
}

/// Borrowed authored-content identity for checked resume admission.
pub struct ContentRef<'a> {
    pub session: u64,
    pub project: &'a ProjectId,
    pub revision: &'a RevisionId,
    pub content: &'a ContentIdentity,
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
    pub content: ContentIdentity,
    domain: Domain,
    window: Window,
    /// Monotonic device delivery coordinate. In a loop this can exceed the
    /// window's end; use `content_sample` for the position in the source/edit.
    pub sample: AudioSample,
    pub generation: Option<Generation>,
    pub phase: Phase,
    pub preparation: Option<deadpan_playback::Preparation>,
    requested_frame: Option<u64>,
    submitted_picture: Option<u64>,
}

/// A paused device estimate survives only while its editorial location is
/// unchanged. Resume retains its exact sample, including the fractional frame.
pub struct Resume {
    ticket: u64,
    session: u64,
    project: ProjectId,
    revision: RevisionId,
    content: ContentIdentity,
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

    /// Whether this paused position belongs to `content` of `revision`.
    pub fn shows(&self, revision: &RevisionId, content: &ContentIdentity) -> bool {
        &self.revision == revision && &self.content == content
    }

    /// The same paused position for other content with identical timing in
    /// the same session and project, such as an AI comparison switching
    /// between the committed pause and a proposed variant. The exact sample,
    /// frame and window are kept; the old device generation is not.
    pub fn retarget(self, revision: RevisionId, content: ContentIdentity) -> Self {
        Self {
            revision,
            content,
            generation: None,
            ..self
        }
    }

    pub fn matches(&self, update: &Update) -> bool {
        self.ticket == update.ticket
            && self.session == update.session
            && self.project == update.project_id
            && self.revision == update.revision_id
            && self.content == update.content
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
            ContentRef {
                session,
                project,
                revision,
                content: &ContentIdentity::Committed,
            },
            &self.domain,
            &self.window,
            frame,
        )
    }

    pub fn sample_for_domain(
        &self,
        identity: ContentRef<'_>,
        domain: &Domain,
        window: &Window,
        frame: u64,
    ) -> Option<AudioSample> {
        let content_sample = window.sample(self.sample)?;
        (self.session == identity.session
            && &self.project == identity.project
            && &self.revision == identity.revision
            && &self.content == identity.content
            && &self.domain == domain
            && &self.window == window
            && self.frame == frame
            && domain.frame_at_sample(content_sample).ok() == Some(frame))
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
            content: identity.content,
            domain,
            window,
            sample: delivery_start,
            generation: None,
            phase: Phase::Preparing,
            preparation: None,
            requested_frame: None,
            submitted_picture: None,
        };
        run.position()?;
        Ok(run)
    }

    pub fn domain(&self) -> &Domain {
        &self.domain
    }

    pub fn preparation_label(&self) -> String {
        let Some(preparation) = self.preparation else {
            return "Preparing".into();
        };
        Self::preparation_status_label(preparation)
    }

    pub fn preparation_status_label(preparation: deadpan_playback::Preparation) -> String {
        use deadpan_audio::PreparationActivity as Activity;
        let work = |label: &str, completed: u32, total: u32| {
            // These are this stage's processed samples, not a total-job estimate.
            format!(
                "{label} {}.{}/{}.{}s",
                completed / 48_000,
                completed / 4800 % 10,
                total / 48_000,
                total / 4800 % 10
            )
        };
        let activity = match preparation.activity {
            Activity::CheckingPlan => "Checking audio".into(),
            Activity::LoadingSource => "Verifying source".into(),
            Activity::DecodingSource => "Decoding source".into(),
            Activity::PreparingOutput => "Preparing output".into(),
            Activity::CheckingSavedAudio { completed, total } => {
                work("Checking saved audio", completed, total)
            }
            Activity::RestoringSavedAudio { completed, total } => {
                work("Restoring saved audio", completed, total)
            }
            Activity::SavingAudioCache => "Saving reusable audio".into(),
            Activity::CheckingStretch {
                completed, total, ..
            } => work("Checking stretch", completed, total),
            Activity::Stretching {
                completed, total, ..
            } => work("Stretching stage", completed, total),
            Activity::CheckingStretchInput {
                completed, total, ..
            } => work("Checking stretch input", completed, total),
            Activity::SavingStretch { .. } => "Saving stretch".into(),
        };
        format!(
            "Preparing · {activity} · {}.{}s elapsed",
            preparation.elapsed_millis / 1000,
            preparation.elapsed_millis / 100 % 10
        )
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
        if self.domain.is_audio_only() {
            return Err("Sound audition has no picture.".into());
        }
        Ok(self.position()?.picture)
    }

    /// Decode-ahead follows the captured audible loop, including context.
    /// Convert its exact sample horizon through the same domain mapping as
    /// presentation; Original VFR ordinals never use the project frame rate.
    pub fn loop_lookahead(
        &self,
    ) -> Result<Option<deadpan_media::lookahead::LoopLookAhead>, String> {
        use deadpan_media::lookahead::{LOOKAHEAD_HORIZON, LoopLookAhead};
        if !self.window.looping() || self.domain.is_audio_only() {
            return Ok(None);
        }
        let frame = |sample| {
            self.domain
                .frame_at_sample(AudioSample(sample))
                .and_then(|frame| i64::try_from(frame).map_err(|error| error.to_string()))
        };
        let start = self.window.start().0;
        let end = self.window.end().0;
        let first = frame(start)?;
        let excluded = frame(end - 1)?
            .checked_add(1)
            .ok_or("Loop picture boundary is not representable.")?;
        let content = self.content_sample()?.0;
        let horizon =
            i64::try_from(LOOKAHEAD_HORIZON.as_secs() * u64::from(deadpan_core::MIX_SAMPLE_RATE))
                .map_err(|error| error.to_string())?
                .min(end - start);
        let remaining = end - content;
        let mut through = if horizon < remaining {
            frame(content + horizon)?
        } else {
            excluded
                .checked_add(frame(start + horizon - remaining)? - first)
                .ok_or("Loop picture horizon is not representable.")?
        };
        if matches!(self.domain, Domain::Original(_)) && frame(content)? == excluded - 1 {
            // The last included picture can hold through an arbitrarily long
            // VFR duration, audio tail or frame enclosure. Coalescing will not
            // request it again as the clock advances, so this is the final
            // opportunity to prepare the seam, even beyond the usual horizon.
            through = through.max(excluded);
        }
        Ok(LoopLookAhead::new(first, excluded, through))
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
            content: self.content.clone(),
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
            || self.content != update.content
        {
            return Ok(None);
        }
        if let Some(generation) = self.generation
            && update.generation.is_some_and(|next| next != generation)
        {
            return Ok(None);
        }
        // A completed device generation cannot resume while its final picture
        // is finishing. Faults and explicit stops still revoke the handoff.
        if self.phase == Phase::Ended && matches!(update.phase, Phase::Preparing | Phase::Playing) {
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
        self.preparation = if update.phase == Phase::Preparing {
            update.preparation
        } else {
            None
        };
        if update.generation.is_some() {
            self.generation = update.generation;
        }
        Ok(Some(position.cursor))
    }

    /// Keep one decode/GPU submission in flight and coalesce intervening audio
    /// positions. Never repeatedly cancel a useful decode at the audio cadence.
    pub fn picture(&mut self, frame: u64, pipeline_busy: bool) -> Option<Generation> {
        if self.domain.is_audio_only()
            || !matches!(self.phase, Phase::Playing | Phase::Ended)
            || (self.phase == Phase::Ended && self.picture_frame().ok() != Some(frame))
            || pipeline_busy
            || self.requested_frame == Some(frame)
        {
            return None;
        }
        let generation = self.generation?;
        // Frames the heard clock passed while the previous picture was in
        // flight were never shown. A loop wrap is not a drop.
        if let Some(previous) = self.requested_frame
            && frame > previous + 1
        {
            deadpan_diagnostics::PLAYBACK_PICTURES
                .skipped
                .add(frame - previous - 1);
        }
        deadpan_diagnostics::PLAYBACK_PICTURES.requested.increment();
        self.requested_frame = Some(frame);
        self.submitted_picture = None;
        Some(generation)
    }

    /// Record the actual submitted request, not merely a scheduling decision.
    pub fn picture_submitted(&mut self, request: u64) {
        self.submitted_picture = Some(request);
    }

    /// Only this run's final included picture can complete automatic playback.
    pub fn terminal_picture_request(&self) -> Option<(Generation, u64)> {
        if self.phase != Phase::Ended || self.requested_frame != self.picture_frame().ok() {
            return None;
        }
        Some((self.generation?, self.submitted_picture?))
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
                content: ContentIdentity::Committed,
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
