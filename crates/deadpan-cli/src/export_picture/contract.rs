//! Geometry and exact output timestamps captured from one committed session.

use deadpan_core::{
    AudioSample, ColorPolicy, ExactRatio, FrameRange, FrameRate, PresentationBasis, ProjectFrame,
    ProjectId, RevisionId, TimeError,
};
use deadpan_media::source_import_timing::nearest_even_dimension;
use deadpan_render::validate_working_readback_dimensions;
use serde::Serialize;

use crate::picture::ProjectPictureSession;

use super::ExportPictureError;

/// Position on the output frame clock, starting at zero for the captured range.
/// It is distinct from both the absolute project frame and source ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutputFrameOrdinal(pub u64);

/// Exact seconds per output timestamp tick. For a project rate N/D this is
/// 1/N, with a frame duration of D ticks. Source time bases remain independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutputTimeBase {
    numerator: u32,
    denominator: u32,
}

impl OutputTimeBase {
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

/// One validated output ordinal and its absolute project coordinate. PTS is
/// relative to the captured range; it never comes from a decoded source PTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutputFrameTiming {
    output_frame: OutputFrameOrdinal,
    project_frame: ProjectFrame,
    pts: i64,
    duration: i64,
}

impl OutputFrameTiming {
    pub const fn output_frame(self) -> OutputFrameOrdinal {
        self.output_frame
    }

    pub const fn project_frame(self) -> ProjectFrame {
        self.project_frame
    }

    pub const fn pts(self) -> i64 {
        self.pts
    }

    pub const fn duration(self) -> i64 {
        self.duration
    }
}

/// Immutable automatic picture contract admitted from a committed session.
/// Serialization is evidence only: there is no deserializer or public field
/// constructor that can grant a different revision, raster or output clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportPictureContract {
    project_id: ProjectId,
    revision_id: RevisionId,
    range: FrameRange,
    canvas: [u32; 2],
    raster: [u32; 2],
    frame_rate: FrameRate,
    color_policy: ColorPolicy,
    time_base: OutputTimeBase,
    frame_count: u64,
    terminal_pts: i64,
    project_audio_start: AudioSample,
    project_audio_end: AudioSample,
    relative_aspect_error: ExactRatio,
}

impl ExportPictureContract {
    /// Capture exact output geometry and clocks from an immutable project view.
    /// This allocates no renderer and does not admit decoded media or an output file.
    pub fn capture(session: &ProjectPictureSession) -> Result<Self, ExportPictureError> {
        Self::from_captured(
            session.project_id(),
            session.revision(),
            session.basis(),
            session.range(),
        )
    }

    fn from_captured(
        project_id: &ProjectId,
        revision_id: &RevisionId,
        basis: &PresentationBasis,
        range: FrameRange,
    ) -> Result<Self, ExportPictureError> {
        if range.start().0 < 0 || range.duration().frames() == 0 {
            return Err(ExportPictureError::Range);
        }
        if basis.color_policy != ColorPolicy::SdrRec709 {
            return Err(ExportPictureError::InvalidContract(
                "HDR output requires a qualified tone-mapping/output path",
            ));
        }
        validate_working_readback_dimensions(basis.width, basis.height)?;
        let raster = [
            nearest_even_dimension(ExactRatio::integer(i64::from(basis.width)))?,
            nearest_even_dimension(ExactRatio::integer(i64::from(basis.height)))?,
        ];
        validate_working_readback_dimensions(raster[0], raster[1])?;
        let canvas_aspect = ExactRatio::new(i128::from(basis.width), i128::from(basis.height))?;
        let raster_aspect = ExactRatio::new(i128::from(raster[0]), i128::from(raster[1]))?;
        let relative_aspect_error = raster_aspect
            .checked_div(canvas_aspect)?
            .checked_sub(ExactRatio::ONE)?;

        let frame_rate = basis.frame_rate;
        if i32::try_from(frame_rate.numerator()).is_err()
            || i32::try_from(frame_rate.denominator()).is_err()
        {
            return Err(ExportPictureError::InvalidContract(
                "project frame rate exceeds the encoder rational component bound",
            ));
        }
        let frames = range.duration().frames();
        let terminal_pts = frames
            .checked_mul(i64::from(frame_rate.denominator()))
            .ok_or(TimeError::Overflow)?;
        let project_audio_start = frame_rate.audio_boundary(range.start())?;
        let project_audio_end = frame_rate.audio_boundary(range.end())?;
        Ok(Self {
            project_id: project_id.clone(),
            revision_id: revision_id.clone(),
            range,
            canvas: [basis.width, basis.height],
            raster,
            frame_rate,
            color_policy: basis.color_policy,
            time_base: OutputTimeBase {
                numerator: 1,
                denominator: frame_rate.numerator(),
            },
            frame_count: u64::try_from(frames).map_err(|_| TimeError::Overflow)?,
            terminal_pts,
            project_audio_start,
            project_audio_end,
            relative_aspect_error,
        })
    }

    pub fn project_id(&self) -> &ProjectId {
        &self.project_id
    }

    pub fn revision_id(&self) -> &RevisionId {
        &self.revision_id
    }

    pub const fn range(&self) -> FrameRange {
        self.range
    }

    /// Unchanged authored canvas. Framing is evaluated in these coordinates.
    pub const fn canvas(&self) -> [u32; 2] {
        self.canvas
    }

    /// Even square-pixel output geometry. Each axis is the nearest even value,
    /// choosing down on a tie and retaining a minimum of two pixels. The shared
    /// import rule bounds each change to one pixel, including one-pixel axes.
    pub const fn raster(&self) -> [u32; 2] {
        self.raster
    }

    pub const fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    pub const fn color_policy(&self) -> ColorPolicy {
        self.color_policy
    }

    pub const fn time_base(&self) -> OutputTimeBase {
        self.time_base
    }

    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Exclusive output end in `time_base` ticks, without final-frame rounding.
    pub const fn terminal_pts(&self) -> i64 {
        self.terminal_pts
    }

    /// Absolute 48 kHz boundary from the common project origin.
    pub const fn project_audio_start(&self) -> AudioSample {
        self.project_audio_start
    }

    /// Absolute 48 kHz boundary, independently rounded from the common origin.
    pub const fn project_audio_end(&self) -> AudioSample {
        self.project_audio_end
    }

    /// `(raster aspect / committed canvas aspect) - 1`, retaining its sign.
    pub const fn relative_aspect_error(&self) -> ExactRatio {
        self.relative_aspect_error
    }

    pub fn timing(
        &self,
        output_frame: OutputFrameOrdinal,
    ) -> Result<OutputFrameTiming, ExportPictureError> {
        if output_frame.0 >= self.frame_count {
            return Err(ExportPictureError::Range);
        }
        let ordinal = i64::try_from(output_frame.0).map_err(|_| TimeError::Overflow)?;
        let duration = i64::from(self.frame_rate.denominator());
        let pts = ordinal.checked_mul(duration).ok_or(TimeError::Overflow)?;
        let project_frame = self
            .range
            .start()
            .0
            .checked_add(ordinal)
            .map(ProjectFrame)
            .ok_or(TimeError::Overflow)?;
        Ok(OutputFrameTiming {
            output_frame,
            project_frame,
            pts,
            duration,
        })
    }
}

#[cfg(test)]
mod tests;
