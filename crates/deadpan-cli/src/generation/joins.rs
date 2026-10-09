//! Advisory measurement of an AI pause's two source joins (spec §12.5).
//!
//! An accepted bridge is joined to the Original by hard cuts at exact frames:
//! the frame before the pause shows the unchanged left picture L, the pause
//! shows the sampled master's frames 0..N-1, and the frame after it shows the
//! unchanged right picture R. Nothing is crossfaded, because a picture
//! crossfade would alter original frames. Whether each cut reads as
//! continuous therefore depends only on how close the first and last
//! generated frames are to the pictures they meet.
//!
//! [`measure_request_joins`] decodes the committed pictures on either side of
//! the Hold at the request's origin revision through the project picture
//! path (what the viewer shows there, before editorial framing), and the
//! sampled master's first and last frames cropped back to the canvas aspect
//! exactly as presentation crops them (`picture::fill_canvas_aspect`). Both
//! sides are compared in one space: the canvas content region of the native
//! raster ([`comparison_region`]), with each boundary picture fitted whole
//! into that region the way conditioning fits it and then cropped like a
//! generated frame. The result is a heuristic: it never accepts, rejects,
//! selects or reorders a variant, and a Smooth class does not prove an
//! invisible cut. The thresholds are uncalibrated against viewers.

use std::io::Cursor;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_core::{
    BoundaryQueryLimits, ExactRatio, ExtensionDirection, NodeId, RevisionId, ScopedNodeTarget,
};
use deadpan_render::{Rgba8Frame, Rotation, SampleDepth};
use deadpan_store::generated_media::GeneratedReadHandle;
use deadpan_store::generation_attempts::BundleValidationReceipt;
use image::{ImageBuffer, Rgb, RgbImage, imageops};
use serde::Serialize;

use crate::picture::{
    PreparedPicture, ProjectPictureError, ProjectPictureSession, aspect_region, fill_canvas_aspect,
    open_candidate_master, source_to_render_frame,
};

/// Below this mean absolute RGB difference (0..255) a join is Smooth.
pub const SMOOTH_BELOW: f64 = 6.0;
/// Below this, and at least [`SMOOTH_BELOW`], a join is Noticeable; at or
/// above it, a Jump.
pub const NOTICEABLE_BELOW: f64 = 20.0;

const FRAME_TIMEOUT: Duration = Duration::from_secs(15);
/// The largest native raster measured, in pixels per picture.
const MAX_PIXELS: u64 = 4096 * 4096;

/// A coarse, advisory reading of one join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinClass {
    Smooth,
    Noticeable,
    Jump,
}

impl JoinClass {
    /// The class of a mean absolute RGB difference on the 0..255 scale.
    pub fn of(mean_abs_diff: f64) -> Self {
        if mean_abs_diff < SMOOTH_BELOW {
            Self::Smooth
        } else if mean_abs_diff < NOTICEABLE_BELOW {
            Self::Noticeable
        } else {
            Self::Jump
        }
    }
}

/// The difference across one join inside the comparison region.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct JoinMeasure {
    /// Mean of |a - b| over the R, G and B samples of every region pixel,
    /// on the 0..255 scale.
    pub mean_abs_diff: f64,
    /// The largest single-channel difference in the region.
    pub max_abs_diff: u8,
    pub class: JoinClass,
}

/// An absent neighbor is distinct from a measured join against authored black.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "status", content = "measure", rename_all = "snake_case")]
pub enum JoinObservation {
    Absent,
    Conditioned(JoinMeasure),
    Unconditioned(JoinMeasure),
}

impl JoinObservation {
    pub fn measure(&self) -> Option<&JoinMeasure> {
        match self {
            Self::Absent => None,
            Self::Conditioned(measure) | Self::Unconditioned(measure) => Some(measure),
        }
    }
}

/// Available joins of one Ready variant, with conditioning roles explicit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct JoinReport {
    /// The committed picture before the pause against the sampled master's
    /// first frame.
    pub entry: JoinObservation,
    /// The sampled master's last frame against the committed picture after
    /// the pause.
    pub exit: JoinObservation,
    /// The compared region `[width, height]`: the canvas aspect inside the
    /// native raster, which is also the size of a presented generated frame.
    pub region: [u32; 2],
}

#[derive(Debug, thiserror::Error)]
pub enum JoinError {
    #[error("Measuring the AI pause joins was cancelled.")]
    Cancelled,
    #[error("the AI pause lacks a required conditioning neighbor in its authored definition")]
    NoBoundaries,
    #[error("join pictures disagree with the variant's receipt: {0}")]
    Shape(&'static str),
    #[error("join measurement limit exceeded: {0}")]
    Limits(&'static str),
    #[error("join pictures cannot be compared: {0}")]
    Unsupported(&'static str),
    #[error("join picture interpretation failed: {0}")]
    Aperture(String),
    #[error(transparent)]
    Picture(#[from] ProjectPictureError),
}

/// Packed 8-bit RGB, `width * height * 3` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbPicture {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// The single comparison space shared by every caller: the `canvas` aspect
/// centered in the `native` raster, as conditioning fills it and
/// presentation crops a generated frame back to it.
pub fn comparison_region(canvas: [u32; 2], native: [u32; 2]) -> [u32; 2] {
    aspect_region(canvas, native, [1, 1])
}

/// Measure both joins of a Ready variant of the bridge request for `hold`.
///
/// `package` is the project, `origin` the request's origin revision (the one
/// its conditioning was decoded from), `generated` the store's or a live
/// workspace's [`GeneratedReadHandle`] and `receipt` the variant's Ready
/// receipt. The boundary pictures are decoded from `origin` through
/// [`ProjectPictureSession`] (a read-only store, so this coexists with the
/// app's writer); the sampled master is read through a verified snapshot and
/// checked against the receipt by [`open_candidate_master`]. Blocking,
/// bounded (two boundary decodes, one master open, two master frames) and
/// cancellable: call it off the UI and audio threads. It writes nothing.
pub fn measure_request_joins(
    package: &Path,
    generated: &GeneratedReadHandle,
    origin: &RevisionId,
    hold: &NodeId,
    receipt: &BundleValidationReceipt,
    cancelled: &AtomicBool,
) -> Result<JoinReport, JoinError> {
    measure_scoped_request_joins(
        package,
        generated,
        origin,
        &ScopedNodeTarget {
            node: hold.clone(),
            repeats: Vec::new(),
        },
        receipt,
        cancelled,
    )
}

/// Measure the authored provider's joins before outer Repeat/Retime sampling.
/// This does not claim to measure a cropped or retimed presentation seam.
pub fn measure_scoped_request_joins(
    package: &Path,
    generated: &GeneratedReadHandle,
    origin: &RevisionId,
    target: &ScopedNodeTarget,
    receipt: &BundleValidationReceipt,
    cancelled: &AtomicBool,
) -> Result<JoinReport, JoinError> {
    check(cancelled)?;
    let video = receipt.sampled_video();
    let native = [video.width(), video.height()];
    if u64::from(native[0]) * u64::from(native[1]) > MAX_PIXELS {
        return Err(JoinError::Limits("native raster"));
    }
    let frames = u32::try_from(video.frames().frames())
        .ok()
        .filter(|frames| *frames > 0)
        .ok_or(JoinError::Shape("sampled frame count"))?;

    let mut session = ProjectPictureSession::open_revision(package, origin, None, cancelled)?;
    let boundaries = session
        .plan()
        .scoped_hold_boundaries(target, BoundaryQueryLimits::default())
        .map_err(ProjectPictureError::from)?;
    let left_position = boundaries.left.as_ref().map(|boundary| boundary.position);
    let right_position = boundaries.right.as_ref().map(|boundary| boundary.position);
    let direction = receipt.extension_plan().map(|plan| plan.direction());
    require_conditioning(direction, left_position.is_some(), right_position.is_some())?;
    if boundaries.duration.frames() != i64::from(frames) {
        return Err(JoinError::Shape(
            "Hold duration differs from the sampled master",
        ));
    }
    let basis = session.basis();
    let canvas = [basis.width, basis.height];
    let mut boundary = |position: ExactRatio| -> Result<Option<Rgba8Frame>, JoinError> {
        let prepared = session.prepare_definition(&boundaries.definition, position, cancelled)?;
        Ok(match prepared.picture {
            PreparedPicture::Frame { frame, .. } | PreparedPicture::Generated { frame, .. } => {
                Some(frame)
            }
            PreparedPicture::Background => None,
        })
    };
    let left = left_position.map(&mut boundary).transpose()?;
    let right = right_position.map(&mut boundary).transpose()?;
    drop(session);
    check(cancelled)?;

    let mut master = open_candidate_master(
        generated,
        receipt.sampled_object(),
        frames,
        (native[0], native[1]),
        cancelled,
    )?;
    let mut frame = |ordinal: usize| -> Result<Rgba8Frame, JoinError> {
        let id = master
            .index()
            .index()
            .frames()
            .get(ordinal)
            .ok_or(JoinError::Shape("sampled frame index"))?
            .identity;
        let decoded = master
            .frame(id, FRAME_TIMEOUT, cancelled)
            .map_err(ProjectPictureError::from)?;
        Ok(source_to_render_frame(decoded, master.info())?)
    };
    let first = frame(0)?;
    let last = frame(frames as usize - 1)?;
    generated
        .check_live(cancelled)
        .map_err(ProjectPictureError::from)?;
    measure_available_pictures(
        left.as_ref().map(|picture| picture.as_ref()),
        first,
        last,
        right.as_ref().map(|picture| picture.as_ref()),
        canvas,
        direction,
    )
}

/// Measure joins from already decoded pictures: the committed boundary
/// pictures (`None` for authored black) and the sampled master's first and
/// last frames at the native raster, uncropped. Pure and bounded by the
/// pictures' sizes.
pub fn measure_pictures(
    left: Option<&Rgba8Frame>,
    first: Rgba8Frame,
    last: Rgba8Frame,
    right: Option<&Rgba8Frame>,
    canvas: [u32; 2],
) -> Result<JoinReport, JoinError> {
    measure_available_pictures(Some(left), first, last, Some(right), canvas, None)
}

fn require_conditioning(
    direction: Option<ExtensionDirection>,
    left: bool,
    right: bool,
) -> Result<(), JoinError> {
    let present = match direction {
        None => left && right,
        Some(ExtensionDirection::FromLeft) => left,
        Some(ExtensionDirection::FromRight) => right,
    };
    if present {
        Ok(())
    } else {
        Err(JoinError::NoBoundaries)
    }
}

// Outer None means no neighbor; Some(None) means an authored black picture.
fn measure_available_pictures(
    left: Option<Option<&Rgba8Frame>>,
    first: Rgba8Frame,
    last: Rgba8Frame,
    right: Option<Option<&Rgba8Frame>>,
    canvas: [u32; 2],
    direction: Option<ExtensionDirection>,
) -> Result<JoinReport, JoinError> {
    require_conditioning(direction, left.is_some(), right.is_some())?;
    let native = [first.metadata().width, first.metadata().height];
    if [last.metadata().width, last.metadata().height] != native {
        return Err(JoinError::Shape("sampled frames differ in size"));
    }
    if u64::from(native[0]) * u64::from(native[1]) > MAX_PIXELS {
        return Err(JoinError::Limits("native raster"));
    }
    let region = comparison_region(canvas, native);
    let presented = |frame: Rgba8Frame| -> Result<RgbPicture, JoinError> {
        let cropped = rgb(&fill_canvas_aspect(frame, canvas)?)?;
        if [cropped.width, cropped.height] != region {
            return Err(JoinError::Shape("presented generated frame"));
        }
        Ok(cropped)
    };
    let first = presented(first)?;
    let last = presented(last)?;
    let measure = |boundary, generated: &RgbPicture, conditioned| {
        let Some(picture) = boundary else {
            return Ok(JoinObservation::Absent);
        };
        let comparison = boundary_in_region(picture, native, region)?;
        let result = compare(&comparison, generated, region)?;
        Ok::<_, JoinError>(if conditioned {
            JoinObservation::Conditioned(result)
        } else {
            JoinObservation::Unconditioned(result)
        })
    };
    Ok(JoinReport {
        entry: measure(
            left,
            &first,
            direction != Some(ExtensionDirection::FromRight),
        )?,
        exit: measure(
            right,
            &last,
            direction != Some(ExtensionDirection::FromLeft),
        )?,
        region,
    })
}

/// Compare two equal-size pictures inside the centered `region`.
pub fn compare(a: &RgbPicture, b: &RgbPicture, region: [u32; 2]) -> Result<JoinMeasure, JoinError> {
    if (a.width, a.height) != (b.width, b.height) {
        return Err(JoinError::Shape("compared pictures differ in size"));
    }
    for picture in [a, b] {
        if u64::try_from(picture.rgb.len()).ok()
            != Some(u64::from(picture.width) * u64::from(picture.height) * 3)
        {
            return Err(JoinError::Shape("picture layout"));
        }
    }
    let [width, height] = region;
    if width == 0 || height == 0 || width > a.width || height > a.height {
        return Err(JoinError::Shape("content region"));
    }
    let (left, top) = ((a.width - width) / 2, (a.height - height) / 2);
    let row_bytes = a.width as usize * 3;
    let mut total = 0_u64;
    let mut maximum = 0_u8;
    for y in top..top + height {
        let start = y as usize * row_bytes + left as usize * 3;
        let end = start + width as usize * 3;
        for (x, y) in a.rgb[start..end].iter().zip(&b.rgb[start..end]) {
            let difference = x.abs_diff(*y);
            total += u64::from(difference);
            maximum = maximum.max(difference);
        }
    }
    let samples = u64::from(width) * u64::from(height) * 3;
    let mean_abs_diff = total as f64 / samples as f64;
    Ok(JoinMeasure {
        mean_abs_diff,
        max_abs_diff: maximum,
        class: JoinClass::of(mean_abs_diff),
    })
}

/// A decoded RGBA8 frame as packed sRGB, matching conditioning's transfer
/// conversion. Comparing raw BT.709 codes with an sRGB master would report
/// a colour jump even when the pictures represent the same light.
pub fn rgb(frame: &Rgba8Frame) -> Result<RgbPicture, JoinError> {
    let metadata = frame.metadata();
    if frame.sample_depth() != SampleDepth::Eight {
        return Err(JoinError::Unsupported(
            "only eight-bit pictures are measured",
        ));
    }
    let codes = super::color::srgb_codes(metadata.color).map_err(JoinError::Unsupported)?;
    if metadata.clean_aperture.is_none() {
        // Ordinary pictures retain the direct RGB conversion and allocation.
        let (width, height) = (metadata.width, metadata.height);
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        for row in frame
            .bytes()
            .chunks_exact(metadata.row_stride_bytes as usize)
            .take(height as usize)
        {
            for pixel in row[..width as usize * 4].chunks_exact(4) {
                rgb.extend_from_slice(&[
                    codes[usize::from(pixel[0])],
                    codes[usize::from(pixel[1])],
                    codes[usize::from(pixel[2])],
                ]);
            }
        }
        return Ok(RgbPicture { width, height, rgb });
    }
    let image = super::conditioning::clean_rgba(frame).map_err(JoinError::Aperture)?;
    let (width, height) = image.dimensions();
    let rgb = image
        .pixels()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    Ok(RgbPicture { width, height, rgb })
}

/// A committed boundary picture in the comparison region: displayed at its
/// sample aspect, fitted whole (Lanczos, black bars) into the region of the
/// native raster as conditioning fits it, then cropped to that region as
/// presentation crops a generated frame. `None` is authored black.
fn boundary_in_region(
    picture: Option<&Rgba8Frame>,
    native: [u32; 2],
    region: [u32; 2],
) -> Result<RgbPicture, JoinError> {
    let [width, height] = native;
    let mut raster: RgbImage = ImageBuffer::from_pixel(width, height, Rgb([0, 0, 0]));
    if let Some(frame) = picture {
        let metadata = frame.metadata();
        if metadata.rotation != Rotation::None {
            return Err(JoinError::Unsupported("rotated pictures are not measured"));
        }
        let image = super::conditioning::rgba(frame).map_err(JoinError::Aperture)?;
        let (fitted_width, fitted_height) =
            fitted((image.width(), image.height()), (region[0], region[1]));
        let fitted = imageops::resize(
            &image,
            fitted_width,
            fitted_height,
            imageops::FilterType::Lanczos3,
        );
        let (left, top) = ((width - fitted_width) / 2, (height - fitted_height) / 2);
        for (x, y, pixel) in fitted.enumerate_pixels() {
            raster.put_pixel(left + x, top + y, Rgb([pixel[0], pixel[1], pixel[2]]));
        }
    }
    let (left, top) = ((width - region[0]) / 2, (height - region[1]) / 2);
    let row = width as usize * 3;
    let mut cropped = Vec::with_capacity(region[0] as usize * region[1] as usize * 3);
    for y in top..top + region[1] {
        let start = y as usize * row + left as usize * 3;
        cropped.extend_from_slice(&raster.as_raw()[start..start + region[0] as usize * 3]);
    }
    Ok(RgbPicture {
        width: region[0],
        height: region[1],
        rgb: cropped,
    })
}

/// `size` scaled to fit whole inside `bounds`.
fn fitted(size: (u32, u32), bounds: (u32, u32)) -> (u32, u32) {
    let scale = f64::min(
        f64::from(bounds.0) / f64::from(size.0),
        f64::from(bounds.1) / f64::from(size.1),
    );
    (
        ((f64::from(size.0) * scale).round() as u32).clamp(1, bounds.0),
        ((f64::from(size.1) * scale).round() as u32).clamp(1, bounds.1),
    )
}

/// Decode a PNG that must have exactly `size`, refusing larger headers
/// before allocating their pixels.
pub fn decode_png(bytes: &[u8], size: (u32, u32)) -> Result<RgbPicture, String> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(size.0);
    limits.max_image_height = Some(size.1);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| error.to_string())?
        .to_rgb8();
    if image.dimensions() != size {
        return Err("PNG raster differs".into());
    }
    Ok(RgbPicture {
        width: size.0,
        height: size.1,
        rgb: image.into_raw(),
    })
}

fn check(cancelled: &AtomicBool) -> Result<(), JoinError> {
    if cancelled.load(Ordering::Acquire) {
        Err(JoinError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> RgbPicture {
        RgbPicture {
            width,
            height,
            rgb: rgb.repeat((width * height) as usize),
        }
    }

    fn frame(picture: &RgbPicture) -> Rgba8Frame {
        Rgba8Frame::new(
            deadpan_render::FrameMetadata {
                clean_aperture: None,
                width: picture.width,
                height: picture.height,
                row_stride_bytes: picture.width * 4,
                sample_aspect_ratio: deadpan_render::SampleAspectRatio::SQUARE,
                rotation: Rotation::None,
                color: deadpan_render::SourceColor {
                    transfer: deadpan_render::Transfer::Srgb,
                    primaries: deadpan_render::Primaries::Rec709,
                },
                pts: deadpan_core::SourceTimestamp {
                    ticks: 0,
                    time_base: deadpan_core::SourceTimeBase::new(1, 24).unwrap(),
                },
            },
            picture
                .rgb
                .chunks_exact(3)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect(),
        )
        .unwrap()
    }

    fn png(picture: &RgbPicture) -> Vec<u8> {
        let image =
            image::RgbImage::from_raw(picture.width, picture.height, picture.rgb.clone()).unwrap();
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn classes_follow_the_documented_thresholds() {
        assert_eq!(JoinClass::of(0.0), JoinClass::Smooth);
        assert_eq!(JoinClass::of(5.999), JoinClass::Smooth);
        assert_eq!(JoinClass::of(SMOOTH_BELOW), JoinClass::Noticeable);
        assert_eq!(JoinClass::of(19.999), JoinClass::Noticeable);
        assert_eq!(JoinClass::of(NOTICEABLE_BELOW), JoinClass::Jump);
        assert_eq!(JoinClass::of(255.0), JoinClass::Jump);
    }

    #[test]
    fn identical_pictures_are_smooth_and_differences_are_exact_means() {
        let a = solid(8, 4, [10, 20, 30]);
        let measure = compare(&a, &a, [8, 4]).unwrap();
        assert_eq!(measure.mean_abs_diff, 0.0);
        assert_eq!(measure.max_abs_diff, 0);
        assert_eq!(measure.class, JoinClass::Smooth);
        // One channel differs by 30 everywhere: mean 10 over three channels.
        let b = solid(8, 4, [40, 20, 30]);
        let measure = compare(&a, &b, [8, 4]).unwrap();
        assert_eq!(measure.mean_abs_diff, 10.0);
        assert_eq!(measure.max_abs_diff, 30);
        assert_eq!(measure.class, JoinClass::Noticeable);
        let c = solid(8, 4, [255, 255, 255]);
        assert_eq!(compare(&a, &c, [8, 4]).unwrap().class, JoinClass::Jump);
    }

    #[test]
    fn extension_joins_distinguish_absent_opposite_from_authored_black() {
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            let generated = || frame(&solid(8, 4, [10, 10, 10]));
            for opposite in [None, Some(None)] {
                let (left, right) = match direction {
                    ExtensionDirection::FromLeft => (Some(None), opposite),
                    ExtensionDirection::FromRight => (opposite, Some(None)),
                };
                let report = measure_available_pictures(
                    left,
                    generated(),
                    generated(),
                    right,
                    [8, 4],
                    Some(direction),
                )
                .unwrap();
                let (conditioned, other) = match direction {
                    ExtensionDirection::FromLeft => (report.entry, report.exit),
                    ExtensionDirection::FromRight => (report.exit, report.entry),
                };
                assert!(matches!(conditioned, JoinObservation::Conditioned(_)));
                assert_eq!(conditioned.measure().unwrap().mean_abs_diff, 10.0);
                if opposite.is_some() {
                    assert!(matches!(other, JoinObservation::Unconditioned(_)));
                    assert_eq!(other.measure().unwrap().mean_abs_diff, 10.0);
                } else {
                    assert_eq!(other, JoinObservation::Absent);
                    assert_eq!(
                        serde_json::to_value(other).unwrap(),
                        serde_json::json!({"status":"absent"})
                    );
                }
            }
            let (left, right) = match direction {
                ExtensionDirection::FromLeft => (None, Some(None)),
                ExtensionDirection::FromRight => (Some(None), None),
            };
            assert!(matches!(
                measure_available_pictures(
                    left,
                    generated(),
                    generated(),
                    right,
                    [8, 4],
                    Some(direction)
                ),
                Err(JoinError::NoBoundaries)
            ));
            assert!(matches!(
                measure_available_pictures(left, generated(), generated(), right, [8, 4], None),
                Err(JoinError::NoBoundaries)
            ));
        }
    }

    #[test]
    fn bt709_boundaries_and_srgb_masters_compare_in_the_same_transfer() {
        let source = frame(&solid(4, 4, [20, 64, 128]));
        let mut metadata = *source.metadata();
        metadata.color.transfer = deadpan_render::Transfer::Rec709;
        let source = Rgba8Frame::new(metadata, source.bytes().to_vec()).unwrap();
        let master = || frame(&solid(4, 4, [36, 79, 140]));
        let report =
            measure_pictures(Some(&source), master(), master(), Some(&source), [4, 4]).unwrap();
        assert_eq!(report.entry.measure().unwrap().mean_abs_diff, 0.0);
        assert_eq!(report.exit.measure().unwrap().mean_abs_diff, 0.0);
        // Passing through the old codes is an observable colour error.
        let wrong = || frame(&solid(4, 4, [20, 64, 128]));
        let report =
            measure_pictures(Some(&source), wrong(), wrong(), Some(&source), [4, 4]).unwrap();
        assert_eq!(report.entry.measure().unwrap().class, JoinClass::Noticeable);
        assert_eq!(report.entry.measure().unwrap().max_abs_diff, 16);
        let mut metadata = *source.metadata();
        metadata.color.primaries = deadpan_render::Primaries::Rec2020;
        assert!(rgb(&Rgba8Frame::new(metadata, source.bytes().to_vec()).unwrap()).is_err());
    }

    #[test]
    fn only_the_centered_content_region_counts() {
        let mut a = solid(8, 4, [0, 0, 0]);
        let mut b = solid(8, 4, [255, 255, 255]);
        for y in 0..4 {
            for x in 2..6 {
                let at = (y * 8 + x) * 3;
                a.rgb[at..at + 3].copy_from_slice(&[90, 90, 90]);
                b.rgb[at..at + 3].copy_from_slice(&[90, 90, 90]);
            }
        }
        assert_eq!(compare(&a, &b, [4, 4]).unwrap().mean_abs_diff, 0.0);
        assert_eq!(compare(&a, &b, [8, 4]).unwrap().class, JoinClass::Jump);
        assert_eq!(comparison_region([1920, 1080], [768, 320]), [569, 320]);
    }

    /// A master whose letterboxed frames show the boundary picture is
    /// Smooth; one showing its inverse is a Jump. Bars outside the canvas
    /// region never count, because presentation crops them away.
    #[test]
    fn boundary_pictures_and_presented_masters_share_one_space() {
        let canvas = [16, 9];
        let native = [32, 10];
        let region = comparison_region(canvas, native);
        assert_eq!(region, [18, 10]);
        let picture = |rgb: [u8; 3]| solid(16, 9, rgb);
        let letterboxed = |rgb: [u8; 3], bars: [u8; 3]| {
            let mut raster = solid(32, 10, bars);
            for y in 0..10 {
                for x in 7..25 {
                    let at = (y * 32 + x) * 3;
                    raster.rgb[at..at + 3].copy_from_slice(&rgb);
                }
            }
            frame(&raster)
        };
        let left = frame(&picture([200, 100, 50]));
        let right = frame(&picture([20, 40, 60]));
        let smooth = measure_pictures(
            Some(&left),
            letterboxed([200, 100, 50], [255, 0, 255]),
            letterboxed([20, 40, 60], [0, 255, 0]),
            Some(&right),
            canvas,
        )
        .unwrap();
        assert_eq!(smooth.region, region);
        assert_eq!(
            smooth.entry.measure().unwrap().class,
            JoinClass::Smooth,
            "{smooth:?}"
        );
        assert_eq!(
            smooth.exit.measure().unwrap().class,
            JoinClass::Smooth,
            "{smooth:?}"
        );
        let jump = measure_pictures(
            Some(&left),
            letterboxed([55, 155, 205], [0, 0, 0]),
            letterboxed([235, 215, 195], [0, 0, 0]),
            Some(&right),
            canvas,
        )
        .unwrap();
        assert_eq!(
            jump.entry.measure().unwrap().class,
            JoinClass::Jump,
            "{jump:?}"
        );
        assert_eq!(
            jump.exit.measure().unwrap().class,
            JoinClass::Jump,
            "{jump:?}"
        );
        // Authored black meets black.
        let black = measure_pictures(
            None,
            letterboxed([0, 0, 0], [9, 9, 9]),
            letterboxed([0, 0, 0], [9, 9, 9]),
            None,
            canvas,
        )
        .unwrap();
        assert_eq!(black.entry.measure().unwrap().mean_abs_diff, 0.0);
        assert_eq!(black.exit.measure().unwrap().mean_abs_diff, 0.0);
    }

    #[test]
    fn malformed_inputs_are_refused() {
        let a = solid(8, 4, [0, 0, 0]);
        let b = solid(4, 4, [0, 0, 0]);
        assert!(matches!(compare(&a, &b, [4, 4]), Err(JoinError::Shape(_))));
        assert!(matches!(compare(&a, &a, [9, 4]), Err(JoinError::Shape(_))));
        assert!(matches!(compare(&a, &a, [0, 4]), Err(JoinError::Shape(_))));
        let short = RgbPicture {
            rgb: vec![0; 5],
            ..a.clone()
        };
        assert!(matches!(
            compare(&short, &a, [8, 4]),
            Err(JoinError::Shape(_))
        ));
        assert!(matches!(
            measure_pictures(None, frame(&a), frame(&b), None, [2, 1]),
            Err(JoinError::Shape(_))
        ));
    }

    #[test]
    fn pngs_decode_only_at_the_expected_raster() {
        let picture = solid(8, 4, [1, 2, 3]);
        let bytes = png(&picture);
        assert_eq!(decode_png(&bytes, (8, 4)).unwrap(), picture);
        assert!(decode_png(&bytes, (4, 4)).is_err());
        assert!(decode_png(&bytes, (16, 16)).is_err());
        assert!(decode_png(b"not a png", (8, 4)).is_err());
    }
}
