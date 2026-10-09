//! Shared picture baseline: owned, bounded CPU RGBA8/RGBA64 upload, linear
//! Rec.2020 interpretation/compositing (working 1.0 = 203 cd/m^2), source
//! geometry, an SDR/HDR color branch with reference-white-preserving tone
//! mapping, and an explicit sRGB display transform, plus bounded working
//! readback, Rec.709 limited-range 8-bit and Rec.2100 PQ/HLG limited-range
//! 10-bit planar YUV420 encoder pixels.
//! Preview and offline callers share renderer targets.
//!
//! This does not implement a timeline, an encoder, ICC/HDR display management,
//! effects beyond canvas framing, or native interop.

mod caption;
mod color;
mod export;
mod framing;
mod geometry;
mod gpu;
mod surface;
mod tone;

pub use caption::{CAPTION_STYLE_ID, CaptionLine, CaptionOverlay};
pub use color::{
    HDR_REFERENCE_WHITE_NITS, HLG_NOMINAL_PEAK_NITS, HLG_SYSTEM_GAMMA, PQ_PEAK_NITS, Primaries,
    SourceColor, Transfer, hlg_inverse_oetf, hlg_inverse_ootf, hlg_oetf, hlg_ootf, pq_code_to_nits,
    pq_eotf, pq_inverse_eotf, source_to_working, working_to_display,
};
pub use export::{
    FrameLight, MAX_WORKING_FRAME_BYTES, Rec709Yuv420Frame, Rec2100Yuv420P10Frame,
    WorkingRgba16Frame, Yuv420P10Policy, Yuv420Policy, rec2100_p10_to_working,
    validate_working_readback_dimensions, working_to_rec2100_nonlinear,
};
pub use framing::{
    FramingLayer, MAX_CAPTURED_CANVASES, MAX_CAPTURED_POSES, MAX_CAPTURED_SCOPES,
    MAX_FRAMING_LAYERS, MAX_FRAMING_SCOPES,
};
pub use geometry::{
    FitMode, PictureGeometry, reference_pixel, reference_pixel_with_geometry,
    reference_pixel_with_pipeline, reference_working_with_geometry,
};
pub use gpu::{PictureRenderer, RenderTarget, WorkingReadback};
pub use surface::{
    CleanAperture, FrameMetadata, MAX_DIMENSION, MAX_FRAME_BYTES, MAX_FRAME16_BYTES, MAX_PIXELS,
    Rgba8Frame, Rotation, SampleAspectRatio, SampleDepth,
};
pub use tone::{
    ColorPipeline, HdrTransfer, OutputColor, ToneMap, source_to_working_with, tone_map_highlights,
    working_to_display_with,
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
    #[error("clean aperture must be positive and contained in the backing raster")]
    CleanAperture,
    #[error(
        "RGBA8 rows require a four-byte-aligned stride, exact buffer length, and at most {MAX_FRAME_BYTES} bytes"
    )]
    Layout,
    #[error(
        "RGBA64 rows require an eight-byte-aligned stride, exact buffer length, and at most {MAX_FRAME16_BYTES} bytes"
    )]
    Layout16,
    #[error("tone-map source peak must be 203 to 10000 cd/m^2")]
    ToneMapPeak,
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
