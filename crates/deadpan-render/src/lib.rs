//! Shared picture baseline: owned, bounded CPU RGBA8 upload, linear Rec.2020
//! interpretation/compositing, source geometry, and an explicit sRGB display
//! transform. Preview and offline callers use the same renderer and targets.
//!
//! This does not implement a timeline, HDR input, tone mapping, encoder pixel
//! conversion, ICC display management, effects beyond canvas framing, or native interop.

mod color;
mod framing;
mod geometry;
mod gpu;
mod surface;

pub use color::{Primaries, SourceColor, Transfer, source_to_working, working_to_display};
pub use framing::{FramingLayer, MAX_FRAMING_LAYERS, MAX_FRAMING_SCOPES};
pub use geometry::{FitMode, PictureGeometry, reference_pixel, reference_pixel_with_geometry};
pub use gpu::{PictureRenderer, RenderTarget};
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
    #[error("sample aspect ratio must have nonzero numerator and denominator")]
    AspectRatio,
    #[error("picture dimensions exceed this GPU device's texture limit")]
    DeviceLimit,
    #[error(
        "the previous picture submission is still running; poll and retry or discard the stale frame"
    )]
    Busy,
    #[error("render target belongs to a different picture renderer")]
    ForeignTarget,
    #[error("GPU polling failed: {0}")]
    Poll(String),
}
