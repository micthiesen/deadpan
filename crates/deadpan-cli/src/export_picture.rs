//! One committed revision to bounded, timestamped SDR encoder pictures.
//!
//! Run on a preparation worker. The host supplies its renderer and monotonic
//! deadline. This boundary creates no thread, encoder, output file or authored
//! edit. Product final rendering still requires process isolation, complete
//! audio/effect support, mux verification and atomic publication.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use deadpan_core::ColorPolicy;
use deadpan_core::{
    AssetId, CapturedFraming, GeneratedArtifact, IterationId, SourceFrameId, SourceQualificationId,
    SourceTimestamp,
};
use deadpan_render::{
    FitMode, FrameLight, HdrTransfer, PictureRenderer, Rec709Yuv420Frame, Rec2100Yuv420P10Frame,
    RenderError, RenderTarget,
};

use crate::picture::{
    ColorDecisionPipeline, PreparedPicture, PreparedProjectPicture, ProjectPictureError,
    ProjectPictureSession,
};

mod contract;
pub use contract::{ExportPictureContract, OutputFrameOrdinal, OutputFrameTiming, OutputTimeBase};

#[derive(Debug, thiserror::Error)]
pub enum ExportPictureError {
    #[error(transparent)]
    Picture(#[from] ProjectPictureError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
    #[error(transparent)]
    Geometry(#[from] deadpan_media::source_import_timing::ImportTimingError),
    #[error("invalid captured output picture contract: {0}")]
    InvalidContract(&'static str),
    #[error("output frame is outside the captured half-open range")]
    Range,
    #[error("output picture preparation was cancelled")]
    Cancelled,
    #[error("output picture preparation exceeded its monotonic deadline")]
    Deadline,
    #[error("release the previous output picture before preparing another")]
    OutstandingFrame,
}

/// Source coordinates are provenance. They never replace the output timestamp.
#[derive(Debug)]
pub enum ExportPictureSource {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        id: SourceFrameId,
        pts: SourceTimestamp,
    },
    Generated {
        artifact: Arc<GeneratedArtifact>,
        id: SourceFrameId,
        pts: SourceTimestamp,
    },
    Background,
}

/// Encoder pixels at the contract's branch boundary: limited Rec.709 I420 for
/// SDR output, or planar ten-bit Rec.2100 with its CTA-861.3 light statistics
/// for HDR output. Both come from the same composed working target.
#[derive(Debug)]
pub enum ExportPixels {
    Sdr(Rec709Yuv420Frame),
    Hdr {
        pixels: Rec2100Yuv420P10Frame,
        light: FrameLight,
    },
}

impl ExportPixels {
    pub fn width(&self) -> u32 {
        match self {
            Self::Sdr(pixels) => pixels.width(),
            Self::Hdr { pixels, .. } => pixels.width(),
        }
    }
    pub fn height(&self) -> u32 {
        match self {
            Self::Sdr(pixels) => pixels.height(),
            Self::Hdr { pixels, .. } => pixels.height(),
        }
    }
    /// Tight encoder input: I420 bytes, or little-endian ten-bit samples.
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Sdr(pixels) => pixels.bytes(),
            Self::Hdr { pixels, .. } => pixels.bytes(),
        }
    }
    pub const fn sdr(&self) -> Option<&Rec709Yuv420Frame> {
        match self {
            Self::Sdr(pixels) => Some(pixels),
            Self::Hdr { .. } => None,
        }
    }
    pub const fn hdr(&self) -> Option<(&Rec2100Yuv420P10Frame, FrameLight)> {
        match self {
            Self::Sdr(_) => None,
            Self::Hdr { pixels, light } => Some((pixels, *light)),
        }
    }
}

struct CompletedPermit(Arc<AtomicBool>);

impl CompletedPermit {
    fn acquire(outstanding: &Arc<AtomicBool>) -> Result<Self, ExportPictureError> {
        outstanding
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ExportPictureError::OutstandingFrame)?;
        Ok(Self(Arc::clone(outstanding)))
    }
}

impl Drop for CompletedPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// One owned, complete I420 result. Drop it after the encoder consumes its
/// planes to release this session's completed-result budget. It may outlive
/// the producing session; its immutable contract and bytes remain owned.
pub struct ExportPictureFrame {
    contract: Arc<ExportPictureContract>,
    timing: OutputFrameTiming,
    source: ExportPictureSource,
    pixels: ExportPixels,
    framing_scopes: usize,
    picture_context: Option<Arc<CapturedFraming>>,
    gap_after: Option<IterationId>,
    _permit: CompletedPermit,
}

impl ExportPictureFrame {
    pub fn contract(&self) -> &ExportPictureContract {
        &self.contract
    }
    pub const fn timing(&self) -> OutputFrameTiming {
        self.timing
    }
    pub const fn source(&self) -> &ExportPictureSource {
        &self.source
    }
    pub const fn pixels(&self) -> &ExportPixels {
        &self.pixels
    }
    pub const fn framing_scopes(&self) -> usize {
        self.framing_scopes
    }
    pub fn picture_context(&self) -> Option<&CapturedFraming> {
        self.picture_context.as_deref()
    }
    pub fn gap_after(&self) -> Option<IterationId> {
        self.gap_after.clone()
    }
}

/// Fixed input revision, canvas, output clock, GPU target and one completed
/// frame. There is no unbounded output queue. Calls are synchronous and must
/// stay off the UI/audio threads. Cancellation/deadline checks surround each
/// bounded native operation; they cannot preempt SQLite, decode or GPU calls.
/// An outer process deadline remains required for final-render isolation.
pub struct ExportPictureSession {
    pictures: ProjectPictureSession,
    contract: Arc<ExportPictureContract>,
    renderer: PictureRenderer,
    target: RenderTarget,
    outstanding: Arc<AtomicBool>,
    captions: crate::picture::CaptionMemo,
}

impl ExportPictureSession {
    pub fn new(
        pictures: ProjectPictureSession,
        mut renderer: PictureRenderer,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Self, ExportPictureError> {
        check_control(cancelled, deadline)?;
        // Preview and export share this branch and tone map for the revision.
        renderer.set_color_pipeline(pictures.color_decision().pipeline());
        let contract = Arc::new(ExportPictureContract::capture(&pictures)?);
        let [width, height] = contract.raster();
        let target = renderer.create_target(width, height)?;
        check_control(cancelled, deadline)?;
        Ok(Self {
            pictures,
            contract,
            renderer,
            target,
            outstanding: Arc::new(AtomicBool::new(false)),
            captions: Default::default(),
        })
    }

    pub fn contract(&self) -> &ExportPictureContract {
        &self.contract
    }

    /// Ordinals are relative to the captured range, beginning at output PTS
    /// zero. Absolute project and source coordinates remain separately visible.
    /// A failed request publishes nothing and does not advance any clock.
    pub fn prepare(
        &mut self,
        ordinal: OutputFrameOrdinal,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<ExportPictureFrame, ExportPictureError> {
        check_control(cancelled, deadline)?;
        let timing = self.contract.timing(ordinal)?;
        let permit = CompletedPermit::acquire(&self.outstanding)?;
        let prepared = self.pictures.prepare(timing.project_frame(), cancelled)?;
        check_control(cancelled, deadline)?;
        validate_prepared(&self.contract, timing, &prepared)?;
        while !self.renderer.is_idle()? {
            wait_for_progress(cancelled, deadline)?;
        }
        check_control(cancelled, deadline)?;
        let layers = prepared.render_layers()?;
        let captions = self.captions.overlay(
            &prepared.captions,
            prepared.canvas,
            [self.target.width(), self.target.height()],
        )?;
        let source = match prepared.picture {
            PreparedPicture::Frame {
                asset,
                qualification,
                id,
                frame,
            } => {
                self.renderer.render_composed_captioned(
                    &frame,
                    &self.target,
                    prepared.picture_context.as_deref(),
                    prepared.canvas,
                    FitMode::Fit,
                    &layers,
                    captions,
                )?;
                ExportPictureSource::Original {
                    asset,
                    qualification,
                    id,
                    pts: frame.metadata().pts,
                }
            }
            PreparedPicture::Generated {
                artifact,
                id,
                frame,
            } => {
                self.renderer.render_composed_captioned(
                    &frame,
                    &self.target,
                    prepared.picture_context.as_deref(),
                    prepared.canvas,
                    FitMode::Fit,
                    &layers,
                    captions,
                )?;
                ExportPictureSource::Generated {
                    artifact,
                    id,
                    pts: frame.metadata().pts,
                }
            }
            PreparedPicture::Background => {
                self.renderer
                    .render_background_captioned(&self.target, captions)?;
                ExportPictureSource::Background
            }
        };
        check_control(cancelled, deadline)?;
        let mut pending = loop {
            match self
                .renderer
                .begin_working_readback(&self.target, cancelled, deadline)
            {
                Ok(pending) => break pending,
                Err(RenderError::ReadbackBusy) => wait_for_progress(cancelled, deadline)?,
                Err(error) => return Err(error.into()),
            }
        };
        let working = loop {
            check_control(cancelled, deadline)?;
            if let Some(frame) = pending.poll(cancelled)? {
                break frame;
            }
            wait_for_progress(cancelled, deadline)?;
        };
        check_control(cancelled, deadline)?;
        let pixels = match self.contract.color_policy() {
            ColorPolicy::SdrRec709 => ExportPixels::Sdr(Rec709Yuv420Frame::from_working(&working)?),
            ColorPolicy::HdrRec2020Pq => {
                let (pixels, light) =
                    Rec2100Yuv420P10Frame::from_working(&working, HdrTransfer::Pq)?;
                ExportPixels::Hdr { pixels, light }
            }
            ColorPolicy::HdrRec2020Hlg => {
                let (pixels, light) =
                    Rec2100Yuv420P10Frame::from_working(&working, HdrTransfer::Hlg)?;
                ExportPixels::Hdr { pixels, light }
            }
        };
        check_control(cancelled, deadline)?;
        Ok(ExportPictureFrame {
            contract: Arc::clone(&self.contract),
            timing,
            source,
            pixels,
            framing_scopes: prepared.framing.len(),
            picture_context: prepared.picture_context,
            gap_after: prepared.gap_after,
            _permit: permit,
        })
    }
}

fn validate_prepared(
    contract: &ExportPictureContract,
    timing: OutputFrameTiming,
    picture: &PreparedProjectPicture,
) -> Result<(), ExportPictureError> {
    if &picture.project_id != contract.project_id()
        || &picture.revision_id != contract.revision_id()
        || picture.project_frame != timing.project_frame()
        || picture.canvas != contract.canvas()
        || picture.frame_rate != contract.frame_rate()
    {
        return Err(ExportPictureError::InvalidContract(
            "prepared picture changed captured identity",
        ));
    }
    Ok(())
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), ExportPictureError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(ExportPictureError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(ExportPictureError::Deadline);
    }
    Ok(())
}

fn wait_for_progress(cancelled: &AtomicBool, deadline: Instant) -> Result<(), ExportPictureError> {
    check_control(cancelled, deadline)?;
    // Poll a dedicated preparation worker without a CPU-burning spin loop.
    std::thread::park_timeout(Duration::from_millis(1));
    check_control(cancelled, deadline)
}

#[cfg(test)]
mod tests;
