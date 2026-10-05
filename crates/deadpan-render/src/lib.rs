//! Shared picture baseline: owned, bounded CPU RGBA8 upload, linear Rec.2020
//! interpretation/compositing, source geometry, and an explicit sRGB display
//! transform, plus bounded working readback and Rec.709 limited-range planar
//! YUV420 encoder pixels. Preview and offline callers share renderer targets.
//!
//! This does not implement a timeline, HDR input, tone mapping, an encoder,
//! ICC display management, effects beyond canvas framing, or native interop.

mod caption;
mod color;
mod export;
mod framing;
mod geometry;
mod gpu;
mod surface;

pub use caption::{CAPTION_STYLE_ID, CaptionLine, CaptionOverlay};
pub use color::{Primaries, SourceColor, Transfer, source_to_working, working_to_display};
pub use export::{
    MAX_WORKING_FRAME_BYTES, Rec709Yuv420Frame, WorkingRgba16Frame, Yuv420Policy,
    validate_working_readback_dimensions,
};
pub use framing::{
    FramingLayer, MAX_CAPTURED_CANVASES, MAX_CAPTURED_POSES, MAX_CAPTURED_SCOPES,
    MAX_FRAMING_LAYERS, MAX_FRAMING_SCOPES,
};
pub use geometry::{FitMode, PictureGeometry, reference_pixel, reference_pixel_with_geometry};
pub use gpu::{PictureRenderer, RenderTarget, WorkingReadback};
pub use surface::{
    FrameMetadata, MAX_DIMENSION, MAX_FRAME_BYTES, MAX_PIXELS, Rgba8Frame, Rotation,
    SampleAspectRatio,
};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error(transparent)]
    Framing(#[from] deadpan_core::FramingError),
    #[error("framing exceeds the supported active-layer or scope bound")]
    FramingLayers,
    #[error("framing geometry exceeds the supported finite spatial precision")]
    FramingGeometry,
    #[error("framing source target must be finite and within the uncropped source")]
    FramingPoint,
    #[error("framing scope is absent from this picture path")]
    FramingScope,
    #[error(
        "picture dimensions must be nonzero, at most {MAX_DIMENSION} per axis and {MAX_PIXELS} pixels"
    )]
    Dimensions,
    #[error(
        "RGBA8 rows require a four-byte-aligned stride, exact buffer length, and at most {MAX_FRAME_BYTES} bytes"
    )]
    Layout,
    #[error(
        "working RGBA16 rows require an eight-byte-aligned stride, exact length, and at most {MAX_WORKING_FRAME_BYTES} bytes"
    )]
    WorkingLayout,
    #[error("encoder YUV420 dimensions must both be even")]
    EncoderDimensions,
    #[error("working pixel ({x}, {y}) contains a nonfinite channel")]
    WorkingNonFinite { x: u32, y: u32 },
    #[error("working pixel ({x}, {y}) must be the canonical opaque composite (alpha 1)")]
    WorkingAlpha { x: u32, y: u32 },
    #[error("bounded picture allocation failed")]
    Allocation,
    #[error("sample aspect ratio must have nonzero numerator and denominator")]
    AspectRatio,
    #[error("picture allocation exceeds this GPU device's texture or buffer limit")]
    DeviceLimit,
    #[error(
        "the previous picture submission is still running; poll and retry or discard the stale frame"
    )]
    Busy,
    #[error("render target belongs to a different picture renderer")]
    ForeignTarget,
    #[error("a working readback or its cancelled GPU work is still outstanding")]
    ReadbackBusy,
    #[error("working readback was cancelled")]
    ReadbackCancelled,
    #[error("working readback exceeded its monotonic deadline")]
    ReadbackDeadline,
    #[error("working readback has already completed or failed")]
    ReadbackFinished,
    #[error("working readback failed: {0}")]
    Readback(String),
    #[error("GPU polling failed: {0}")]
    Poll(String),
    #[error("caption rendering failed: {0}")]
    Caption(&'static str),
    #[error("a caption overlay must match its render target raster")]
    CaptionRaster,
}
