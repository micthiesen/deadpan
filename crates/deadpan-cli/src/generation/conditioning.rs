//! Conditioning inputs for a bridge Hold: the pictures on each side of it,
//! contained into the provider's native raster, and the context manifest that
//! binds them to the bridge plan.
//!
//! The left picture is the project frame before the Hold and the right one the
//! frame after it, decoded from the committed revision through the shared
//! project picture path. Each is fitted whole inside the native raster
//! (Lanczos, black bars), as the qualification probe prepared its inputs.
//! Editorial framing around the Hold is not composed into these pictures yet;
//! the manifest records the colour interpretation as a stated assumption.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_core::{FrameDuration, NodeId, NodeKind, ProjectFrame, RevisionId};
use deadpan_jobs::{
    BridgeGenerationPlan, ConditioningMode, HoldConstraints, MotionAmount, Sha256, VideoSpec,
    WorkspaceArtifact, WorkspaceRef,
};
use deadpan_models::BridgeContext;
use deadpan_plan::RenderPlan;
use deadpan_render::{Rgba8Frame, Rotation};
use image::{ImageBuffer, Rgb, RgbImage, RgbaImage, imageops};
use sha2::Digest;

use crate::picture::{PreparedPicture, ProjectPictureSession};

/// Decoded full-range Rec.709 SDR RGB, passed to the model as sRGB.
pub const INPUT_COLOR_INTERPRETATION: &str = "rec709-sdr-full-range-rgb8-interpreted-as-srgb";
pub const LEFT: &str = "inputs/left.png";
pub const RIGHT: &str = "inputs/right.png";
pub const MANIFEST: &str = "inputs/context.json";

/// Everything a bridge request needs before its worker runs.
#[derive(Debug, Clone)]
pub struct BridgeInputs {
    pub plan: BridgeGenerationPlan,
    pub constraints: HoldConstraints,
    pub left_png: Vec<u8>,
    pub right_png: Vec<u8>,
    pub manifest: Vec<u8>,
    pub manifest_sha256: Sha256,
}

/// Prepare the inputs for `hold` at `revision` of the project at `package`.
pub fn prepare(
    package: &Path,
    revision: &RevisionId,
    hold: &NodeId,
    cancelled: &AtomicBool,
) -> Result<BridgeInputs, String> {
    let mut session = ProjectPictureSession::open_revision(package, revision, None, cancelled)
        .map_err(|error| error.to_string())?;
    let document = session.document().clone();
    let NodeKind::Hold { recipe } = &document
        .nodes()
        .get(hold)
        .ok_or("The Hold is no longer in this revision.")?
        .kind
    else {
        return Err("AI pictures fill a pause (Hold) beat.".into());
    };
    let duration = recipe.duration;
    if duration.frames() > super::MAX_BRIDGE_PROJECT_FRAMES {
        return Err(format!(
            "AI pauses are limited to {} frames for now; this one has {}.",
            super::MAX_BRIDGE_PROJECT_FRAMES,
            duration.frames()
        ));
    }
    let plan_picture = RenderPlan::compile(&document).map_err(|error| error.to_string())?;
    let range = plan_picture
        .single_occurrence_range(hold)
        .ok_or("AI pictures need a pause that plays once, outside Repeats and speed changes.")?;
    let total = plan_picture.duration().frames();
    if range.start().0 == 0 || range.end().0 >= total {
        return Err(
            "An AI bridge needs pictures on both sides of the pause; it cannot start or end the edit."
                .into(),
        );
    }
    let rate = document.presentation_basis().frame_rate;
    let plan = BridgeGenerationPlan::for_conditioning(
        ConditioningMode::Bridge,
        FrameDuration::new(duration.frames()).map_err(|error| error.to_string())?,
        rate,
        &super::development_capability(),
        super::native_dimensions(),
    )
    .map_err(|error| error.to_string())?;
    let constraints = HoldConstraints {
        video: VideoSpec::new(duration, rate, super::NATIVE_WIDTH, super::NATIVE_HEIGHT)
            .map_err(|error| error.to_string())?,
        conditioning: ConditioningMode::Bridge,
        motion: MotionAmount::Still,
    };
    let basis = document.presentation_basis();
    let region = canvas_region([basis.width, basis.height]);
    let left_png = boundary_png(
        &mut session,
        ProjectFrame(range.start().0 - 1),
        region,
        cancelled,
    )?;
    let right_png = boundary_png(&mut session, range.end(), region, cancelled)?;
    assemble(plan, constraints, left_png, right_png)
}

/// Bind two prepared pictures to `plan` in a context manifest.
pub fn assemble(
    plan: BridgeGenerationPlan,
    constraints: HoldConstraints,
    left_png: Vec<u8>,
    right_png: Vec<u8>,
) -> Result<BridgeInputs, String> {
    let artifact = |reference: &str, bytes: &[u8]| -> Result<WorkspaceArtifact, String> {
        WorkspaceArtifact::new(
            WorkspaceRef::new(reference).map_err(|error| error.to_string())?,
            sha256(bytes)?,
            bytes.len() as u64,
        )
        .map_err(|error| error.to_string())
    };
    let context = BridgeContext::new(
        plan.clone(),
        artifact(LEFT, &left_png)?,
        artifact(RIGHT, &right_png)?,
        INPUT_COLOR_INTERPRETATION,
    )
    .map_err(|error| error.to_string())?;
    let manifest = serde_json::to_vec(&context).map_err(|error| error.to_string())?;
    let manifest_sha256 = sha256(&manifest)?;
    Ok(BridgeInputs {
        plan,
        constraints,
        left_png,
        right_png,
        manifest,
        manifest_sha256,
    })
}

fn boundary_png(
    session: &mut ProjectPictureSession,
    frame: ProjectFrame,
    region: (u32, u32),
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let prepared = session
        .prepare(frame, cancelled)
        .map_err(|error| error.to_string())?;
    let picture = match &prepared.picture {
        PreparedPicture::Frame { frame, .. } | PreparedPicture::Generated { frame, .. } => {
            Some(rgba(frame)?)
        }
        PreparedPicture::Background => None,
    };
    encode(&contain(picture.as_ref(), region))
}

/// The decoded picture at its display aspect, as straight RGBA.
fn rgba(frame: &Rgba8Frame) -> Result<RgbaImage, String> {
    let metadata = frame.metadata();
    if metadata.rotation != Rotation::None {
        return Err("Rotated Originals cannot condition an AI pause yet.".into());
    }
    let (width, height) = (metadata.width, metadata.height);
    let stride = metadata.row_stride_bytes as usize;
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for row in frame.bytes().chunks_exact(stride).take(height as usize) {
        pixels.extend_from_slice(&row[..width as usize * 4]);
    }
    let image = RgbaImage::from_raw(width, height, pixels).ok_or("decoded frame layout")?;
    // Non-square pixels stretch horizontally to their display width.
    let sar = metadata.sample_aspect_ratio.as_f64();
    if (sar - 1.0).abs() < f64::EPSILON {
        return Ok(image);
    }
    let display = ((f64::from(width) * sar).round() as u32).max(1);
    Ok(imageops::resize(
        &image,
        display,
        height,
        imageops::FilterType::Lanczos3,
    ))
}

/// The project canvas fitted whole inside the native raster.
///
/// Presentation fits each picture into the canvas, and an accepted bridge is
/// cropped back to the canvas aspect (`picture::fill_canvas_aspect`), so
/// conditioning places the picture exactly where that crop will find it.
fn canvas_region(canvas: [u32; 2]) -> (u32, u32) {
    let [width, height] =
        crate::picture::aspect_region(canvas, [super::NATIVE_WIDTH, super::NATIVE_HEIGHT], [1, 1]);
    (width, height)
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

/// Fit the whole picture inside the centered canvas `region` of the native
/// raster, on black.
fn contain(picture: Option<&RgbaImage>, region: (u32, u32)) -> RgbImage {
    let (width, height) = (super::NATIVE_WIDTH, super::NATIVE_HEIGHT);
    let mut canvas: RgbImage = ImageBuffer::from_pixel(width, height, Rgb([0, 0, 0]));
    let Some(picture) = picture else {
        return canvas;
    };
    let (fitted_width, fitted_height) = fitted((picture.width(), picture.height()), region);
    let fitted = imageops::resize(
        picture,
        fitted_width,
        fitted_height,
        imageops::FilterType::Lanczos3,
    );
    let (left, top) = ((width - fitted_width) / 2, (height - fitted_height) / 2);
    for (x, y, pixel) in fitted.enumerate_pixels() {
        canvas.put_pixel(left + x, top + y, Rgb([pixel[0], pixel[1], pixel[2]]));
    }
    canvas
}

fn encode(image: &RgbImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> Result<Sha256, String> {
    let hex: String = sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Sha256::new(hex).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(image: &RgbImage) -> Rgba8Frame {
        let rgba: Vec<u8> = image
            .pixels()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect();
        Rgba8Frame::new(
            deadpan_render::FrameMetadata {
                width: image.width(),
                height: image.height(),
                row_stride_bytes: image.width() * 4,
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
            rgba,
        )
        .unwrap()
    }

    #[test]
    fn conditioning_and_the_presentation_crop_cancel_out() {
        let orange = image::Rgba([200, 100, 50, 255]);
        // A 16:9 picture on a 16:9 canvas fills the canvas region: bars only
        // at the sides of the 2.4:1 raster.
        let region = canvas_region([1920, 1080]);
        assert_eq!(region, (569, 320));
        let canvas = contain(Some(&RgbaImage::from_pixel(1920, 1080, orange)), region);
        assert_eq!(canvas.dimensions(), (768, 320));
        assert_eq!(canvas.get_pixel(0, 160), &Rgb([0, 0, 0]), "left bar");
        assert_eq!(canvas.get_pixel(384, 160), &Rgb([200, 100, 50]), "picture");
        assert_eq!(contain(None, region).get_pixel(384, 160), &Rgb([0, 0, 0]));
        assert_eq!(&encode(&canvas).unwrap()[1..4], b"PNG");
        // Cropping the generated raster to the canvas aspect removes exactly
        // those bars.
        let cropped = crate::picture::fill_canvas_aspect(frame(&canvas), [1920, 1080]).unwrap();
        assert_eq!(
            (cropped.metadata().width, cropped.metadata().height),
            (569, 320)
        );
        assert!(
            cropped
                .bytes()
                .chunks_exact(4)
                .all(|pixel| pixel[..3] == [200, 100, 50]),
            "no bar survives the crop"
        );
        // Odd and anamorphic canvases: conditioning's region and the crop of
        // a square-pixel native raster agree to the pixel.
        for canvas in [[1001, 999], [720, 480], [1080, 1920], [3, 1], [2400, 1000]] {
            let (width, height) = canvas_region(canvas);
            let mut native = RgbImage::from_pixel(768, 320, Rgb([0, 0, 0]));
            let (left, top) = ((768 - width) / 2, (320 - height) / 2);
            for y in top..top + height {
                for x in left..left + width {
                    native.put_pixel(x, y, Rgb([9, 9, 9]));
                }
            }
            let cropped = crate::picture::fill_canvas_aspect(frame(&native), canvas).unwrap();
            assert_eq!(
                (cropped.metadata().width, cropped.metadata().height),
                (width, height),
                "{canvas:?}"
            );
            assert!(
                cropped.bytes().chunks_exact(4).all(|pixel| pixel[0] == 9),
                "{canvas:?}"
            );
        }
        // Non-square pixels: a 1440x1080 raster at 4:3 SAR displays 16:9.
        assert_eq!(
            crate::picture::aspect_region([1920, 1080], [1440, 1080], [4, 3]),
            [1440, 1080]
        );
        assert_eq!(
            crate::picture::aspect_region([4, 3], [1440, 1080], [4, 3]),
            [1080, 1080]
        );
        // A 4:3 picture keeps its own pillar bars inside the 16:9 region, as
        // presentation shows it on the canvas.
        let narrow = contain(Some(&RgbaImage::from_pixel(640, 480, orange)), region);
        assert_eq!(narrow.get_pixel(384 - 260, 160), &Rgb([0, 0, 0]));
        assert_eq!(narrow.get_pixel(384, 160), &Rgb([200, 100, 50]));
    }
}
