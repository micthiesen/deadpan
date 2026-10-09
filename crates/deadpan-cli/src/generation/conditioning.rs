//! Conditioning inputs for a bridge Hold: the pictures on each side of it,
//! contained into the provider's native raster, and the context manifest that
//! binds them to the bridge plan.
//!
//! Each boundary is sampled in the Hold's containing authored definition,
//! before outer Repeat/Retime owners, through the committed revision's shared
//! qualified decoder path. Each is fitted whole inside the native raster
//! (Lanczos, black bars), as the qualification probe prepared its inputs.
//! Editorial framing around the Hold is not composed into these pictures yet.
//!
//! The version-5 manifest records, for each side, what the picture path
//! actually showed: an Original frame (asset, receipt, measured index identity,
//! exact source PTS and the decoder's measured stream colour, pixel format and
//! geometry), a frame of an accepted generated Hold (its artifact objects and
//! the same measurements), or authored black. It declares the model's colour
//! space (canonical full-range sRGB BT.709 RGB) and the conversion applied to
//! each decoded picture. Conditioning refuses a picture whose measured colour
//! no stated conversion covers (`deadpan_models::model_input_conversion`).
//! It also retains the exact presentation crop and fitted content rectangles.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_core::{
    BoundaryQueryLimits, ExactRatio, FrameDuration, NodeId, RevisionId, ScopedNodeTarget,
    SourceFrameId,
};
use deadpan_jobs::{
    BridgeGenerationPlan, ConditioningMode, GenerationOptions, HoldConstraints, MotionAmount,
    Sha256, VideoSpec, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_models::{
    BoundaryClock, BoundaryPicture, BridgeBoundaries, BridgeColor, BridgeContext, BridgeMatrix,
    BridgePrimaries, BridgeRange, BridgeTransfer, CANONICAL_BRIDGE_COLOR, ConditioningGeometry,
    DecodedBoundary, MeasuredStream, ModelInputConversion, RasterRect, RegionCapture,
    model_input_conversion,
};
use deadpan_render::{Rgba8Frame, SampleDepth};
use deadpan_source::{ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, SourceStreamInfo};
use image::{ImageBuffer, Rgb, RgbImage, RgbaImage, imageops};
use sha2::Digest;

use crate::picture::{PreparedPicture, ProjectPictureSession};

#[path = "conditioning/continuity.rs"]
mod continuity;
pub use continuity::{ExtensionContextContinuity, ExtensionContextMeasurement};

mod extension;
pub use extension::{
    ExtensionInputs, prepare_extension_scoped_with_options, prepare_extension_scoped_with_provider,
};

mod inputs;
pub use inputs::PreparedInputs;

/// The conversion applied to every decoded boundary picture: the decoder's
/// full-range RGB8 (declared matrix and range applied) with BT.709 primaries,
/// with sRGB codes retained and BT.709 transfer converted to sRGB before fitting.
pub const INPUT_COLOR_INTERPRETATION: &str = "decoded-full-range-rgb8-bt709-primaries-srgb-unchanged-or-inverse-bt709-oetf-to-srgb-before-fitting";
/// The bridge model's declared input/output colour space.
pub const MODEL_COLOR_SPACE: BridgeColor = CANONICAL_BRIDGE_COLOR;
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

/// One prepared boundary: the model-input PNG and what the project showed.
#[derive(Debug, Clone)]
pub struct PreparedBoundary {
    pub png: Vec<u8>,
    pub picture: BoundaryPicture,
    pub content_rect: Option<RasterRect>,
}

/// Prepare the inputs for `hold` at `revision` of the project at `package`.
pub fn prepare(
    package: &Path,
    revision: &RevisionId,
    hold: &NodeId,
    cancelled: &AtomicBool,
) -> Result<BridgeInputs, String> {
    prepare_with_options(
        package,
        revision,
        hold,
        &GenerationOptions::default(),
        cancelled,
    )
}

pub fn prepare_with_options(
    package: &Path,
    revision: &RevisionId,
    hold: &NodeId,
    options: &GenerationOptions,
    cancelled: &AtomicBool,
) -> Result<BridgeInputs, String> {
    prepare_scoped_with_options(
        package,
        revision,
        &ScopedNodeTarget {
            node: hold.clone(),
            repeats: Vec::new(),
        },
        options,
        cancelled,
    )
}

/// Prepare the complete Hold recipe in its authored definition, including a
/// dormant Default. Scope validation and boundary lookup never choose a root
/// occurrence. Missing definition-edge neighbors are explicit refusals.
pub fn prepare_scoped_with_options(
    package: &Path,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
    options: &GenerationOptions,
    cancelled: &AtomicBool,
) -> Result<BridgeInputs, String> {
    prepare_bridge_scoped(package, revision, target, None, options, cancelled)
}

/// Capture the exact supplied Bridge plan. The caller independently selects
/// its provider capability; this function checks the saved Hold and capture
/// raster before decoding and never substitutes the development envelope.
pub fn prepare_bridge_scoped_with_plan(
    package: &Path,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
    plan: &BridgeGenerationPlan,
    options: &GenerationOptions,
    cancelled: &AtomicBool,
) -> Result<BridgeInputs, String> {
    if plan.native_dimensions() != super::native_dimensions() {
        return Err("Bridge capture requires the supported 768×320 native raster.".into());
    }
    prepare_bridge_scoped(package, revision, target, Some(plan), options, cancelled)
}

fn prepare_bridge_scoped(
    package: &Path,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
    selected_plan: Option<&BridgeGenerationPlan>,
    options: &GenerationOptions,
    cancelled: &AtomicBool,
) -> Result<BridgeInputs, String> {
    // Decoding polls `cancelled` itself; check between the uncancellable
    // steps too so a host shutdown is never held by a finished-but-unused step.
    let check = || {
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            Err("The AI pause was cancelled.".to_owned())
        } else {
            Ok(())
        }
    };
    check()?;
    let mut session = ProjectPictureSession::open_revision(package, revision, None, cancelled)
        .map_err(|error| error.to_string())?;
    check()?;
    let document = session.document().clone();
    let target_id = options.region_target.resolve(None);
    let captured_region = target_id
        .as_ref()
        .map(|id| {
            document
                .targets()
                .get(id)
                .map(|record| (id, record))
                .ok_or_else(|| format!("Region target {id} is not saved in this revision."))
        })
        .transpose()?;
    let boundaries = session
        .plan()
        .scoped_hold_boundaries(target, BoundaryQueryLimits::default())
        .map_err(|error| error.to_string())?;
    let duration = boundaries.duration;
    if selected_plan.is_none() && duration.frames() > super::MAX_BRIDGE_PROJECT_FRAMES {
        return Err(format!(
            "AI pauses are limited to {} frames for now; this one has {}.",
            super::MAX_BRIDGE_PROJECT_FRAMES,
            duration.frames()
        ));
    }
    check()?;
    let left_sample = boundaries.left.as_ref().ok_or(
        "An AI bridge needs a picture before the pause in its authored definition; this Hold starts at the definition edge.",
    )?;
    let right_sample = boundaries.right.as_ref().ok_or(
        "An AI bridge needs a picture after the pause in its authored definition; this Hold ends at the definition edge.",
    )?;
    let rate = document.presentation_basis().frame_rate;
    let plan = if let Some(plan) = selected_plan {
        if plan.project_frames() != duration || plan.project_frame_rate() != rate {
            return Err(
                "The selected Bridge plan differs from the saved Hold duration or project rate."
                    .into(),
            );
        }
        plan.clone()
    } else {
        BridgeGenerationPlan::for_conditioning(
            ConditioningMode::Bridge,
            FrameDuration::new(duration.frames()).map_err(|error| error.to_string())?,
            rate,
            &super::development_capability(),
            super::native_dimensions(),
        )
        .map_err(|error| error.to_string())?
    };
    let mut constraints = HoldConstraints {
        video: VideoSpec::new(duration, rate, super::NATIVE_WIDTH, super::NATIVE_HEIGHT)
            .map_err(|error| error.to_string())?,
        conditioning: ConditioningMode::Bridge,
        motion: MotionAmount::Still,
        instructions: None,
        region_target: None,
    };
    options.apply_to(&mut constraints);
    let basis = document.presentation_basis();
    let region = canvas_region([basis.width, basis.height]);
    let left = boundary(
        &mut session,
        &boundaries.definition,
        left_sample.position,
        region,
        "before",
        cancelled,
    )?;
    check()?;
    let right = boundary(
        &mut session,
        &boundaries.definition,
        right_sample.position,
        region,
        "after",
        cancelled,
    )?;
    check()?;
    let presentation = RasterRect::centered(
        region.0,
        region.1,
        [super::NATIVE_WIDTH, super::NATIVE_HEIGHT],
    )
    .map_err(str::to_owned)?;
    assemble_captured(
        plan,
        constraints,
        left,
        right,
        presentation,
        captured_region,
    )
}

/// Bind two prepared boundaries and their captured crop with no region target.
pub fn assemble(
    plan: BridgeGenerationPlan,
    constraints: HoldConstraints,
    left: PreparedBoundary,
    right: PreparedBoundary,
    presentation: RasterRect,
) -> Result<BridgeInputs, String> {
    assemble_captured(plan, constraints, left, right, presentation, None)
}

fn assemble_captured(
    plan: BridgeGenerationPlan,
    constraints: HoldConstraints,
    left: PreparedBoundary,
    right: PreparedBoundary,
    presentation: RasterRect,
    target: Option<(&deadpan_core::TargetId, &deadpan_core::AttentionTarget)>,
) -> Result<BridgeInputs, String> {
    let artifact = |reference: &str, bytes: &[u8]| -> Result<WorkspaceArtifact, String> {
        WorkspaceArtifact::new(
            WorkspaceRef::new(reference).map_err(|error| error.to_string())?,
            sha256(bytes)?,
            bytes.len() as u64,
        )
        .map_err(|error| error.to_string())
    };
    let boundaries = BridgeBoundaries {
        left: left.picture,
        right: right.picture,
    };
    let geometry = ConditioningGeometry {
        presentation,
        left_content: left.content_rect,
        right_content: right.content_rect,
    };
    let region = target
        .map(|(id, record)| {
            RegionCapture::new(
                id.clone(),
                record,
                &boundaries,
                &geometry,
                [super::NATIVE_WIDTH, super::NATIVE_HEIGHT],
            )
        })
        .transpose()?
        .unwrap_or(RegionCapture::None);
    if constraints.region_target.as_ref() != region.target_id() {
        return Err("Region target controls differ from captured conditioning.".into());
    }
    let context = BridgeContext::new(
        plan.clone(),
        artifact(LEFT, &left.png)?,
        artifact(RIGHT, &right.png)?,
        INPUT_COLOR_INTERPRETATION,
        MODEL_COLOR_SPACE,
        boundaries,
        geometry,
    )
    .and_then(|context| context.with_region(region))
    .map_err(|error| error.to_string())?;
    let manifest = serde_json::to_vec(&context).map_err(|error| error.to_string())?;
    let manifest_sha256 = sha256(&manifest)?;
    Ok(BridgeInputs {
        plan,
        constraints,
        left_png: left.png,
        right_png: right.png,
        manifest,
        manifest_sha256,
    })
}

/// How each boundary picture became model input, read from a context
/// manifest. `None` on a side means nothing was decoded (authored black) or
/// the manifest predates measured evidence (schema 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConditioningColour {
    pub left: Option<ModelInputConversion>,
    pub right: Option<ModelInputConversion>,
    /// True for a schema-1 manifest, which states its interpretation only as text.
    pub unmeasured: bool,
}

impl ConditioningColour {
    /// Read the summary from manifest bytes (`BridgeInputs::manifest` or the
    /// retained manifest object). `None` if the bytes are not a valid context.
    pub fn from_manifest(manifest: &[u8]) -> Option<Self> {
        let context: BridgeContext = serde_json::from_slice(manifest).ok()?;
        let side = |picture: &BoundaryPicture| picture.decoded().map(|decoded| decoded.model_input);
        Some(match context.boundaries() {
            Some(boundaries) => Self {
                left: side(&boundaries.left),
                right: side(&boundaries.right),
                unmeasured: false,
            },
            None => Self {
                left: None,
                right: None,
                unmeasured: true,
            },
        })
    }

    /// Whether either side's codes were read as sRGB without a transfer
    /// conversion (BT.709 transfer passed as sRGB).
    pub fn approximate(self) -> bool {
        [self.left, self.right].contains(&Some(ModelInputConversion::Rec709CodesAsSrgb))
    }

    /// The report value: `{left, right, approximate}` with conversion names,
    /// `"authored_black"` for an undecoded side, or `{"unmeasured": true}`.
    pub fn to_json(self) -> serde_json::Value {
        if self.unmeasured {
            return serde_json::json!({"unmeasured": true, "approximate": false});
        }
        let name = |side: Option<ModelInputConversion>| {
            side.map_or(serde_json::json!("authored_black"), |conversion| {
                serde_json::to_value(conversion).unwrap_or_default()
            })
        };
        serde_json::json!({
            "left": name(self.left),
            "right": name(self.right),
            "approximate": self.approximate(),
        })
    }

    /// One line of plain text for the AI inspector.
    pub fn describe(self) -> String {
        if self.unmeasured {
            return "Conditioning colour was stated, not measured (older request).".into();
        }
        let side = |side: Option<ModelInputConversion>| match side {
            None => "black",
            Some(ModelInputConversion::SrgbCodesUnchanged) => "sRGB",
            Some(ModelInputConversion::Rec709CodesAsSrgb) => "BT.709 read as sRGB",
            Some(ModelInputConversion::Rec709ToSrgb) => "BT.709 converted to sRGB",
        };
        let sides = format!("before: {}, after: {}", side(self.left), side(self.right));
        if self.approximate() {
            format!("Model input colour includes an older approximation ({sides}).")
        } else {
            format!("Model input colour is sRGB ({sides}).")
        }
    }
}

/// Decode the exact definition position and retain its explicit clock.
fn boundary(
    session: &mut ProjectPictureSession,
    definition: &NodeId,
    position: ExactRatio,
    region: (u32, u32),
    side: &str,
    cancelled: &AtomicBool,
) -> Result<PreparedBoundary, String> {
    let prepared = session
        .prepare_definition(definition, position, cancelled)
        .map_err(|error| error.to_string())?;
    let clock = BoundaryClock::Definition {
        project_id: prepared.sample.project_id.clone(),
        revision_id: prepared.sample.revision_id.clone(),
        definition: prepared.sample.definition.clone(),
        position: prepared.sample.position,
    };
    let refuse =
        |reason: String| format!("The picture {side} the pause cannot condition it: {reason}.");
    let (picture, image) = match &prepared.picture {
        PreparedPicture::Frame {
            asset,
            qualification,
            id,
            frame: decoded,
        } => {
            let info = session
                .source_info()
                .ok_or("the decoded Original's stream is not retained")?;
            let picture = decoded_boundary(info, *id, decoded).map_err(refuse)?;
            (
                BoundaryPicture::Original {
                    clock: clock.clone(),
                    asset: asset.clone(),
                    qualification: qualification.clone(),
                    picture,
                },
                Some(rgba(decoded)?),
            )
        }
        PreparedPicture::Generated {
            artifact,
            id,
            frame: decoded,
        } => {
            let info = session
                .source_info()
                .ok_or("the decoded generated master's stream is not retained")?;
            let picture = decoded_boundary(info, *id, decoded).map_err(refuse)?;
            (
                BoundaryPicture::Generated {
                    clock: clock.clone(),
                    sampled_asset: artifact.sampled_asset.clone(),
                    sampled_object: artifact.sampled_object.clone(),
                    provenance: artifact.provenance.clone(),
                    picture,
                },
                Some(rgba(decoded)?),
            )
        }
        PreparedPicture::Background => (
            BoundaryPicture::AuthoredBlack {
                clock: clock.clone(),
            },
            None,
        ),
    };
    let (raster, content_rect) = contain(image.as_ref(), region)?;
    Ok(PreparedBoundary {
        png: encode(&raster)?,
        picture,
        content_rect,
    })
}

/// The decoder's description of the stream `frame` was decoded from.
pub fn measured_stream(info: &SourceStreamInfo, frame: &Rgba8Frame) -> MeasuredStream {
    let color = info.color;
    MeasuredStream {
        codec: info.codec.clone(),
        pixel_format: info.pixel_format.clone(),
        width: info.width,
        height: info.height,
        clean_aperture: info.clean_aperture,
        sample_aspect: [info.sample_aspect_num, info.sample_aspect_den],
        rotation_quarter_turns: info.rotation_quarter_turns,
        decoded_sample_bits: match frame.sample_depth() {
            SampleDepth::Eight => 8,
            SampleDepth::Sixteen => 16,
        },
        color: BridgeColor {
            transfer: match color.transfer {
                ColorTransfer::Bt709 => BridgeTransfer::Bt709,
                ColorTransfer::Srgb => BridgeTransfer::Srgb,
                ColorTransfer::Linear => BridgeTransfer::Linear,
                ColorTransfer::Pq => BridgeTransfer::Pq,
                ColorTransfer::Hlg => BridgeTransfer::Hlg,
            },
            primaries: match color.primaries {
                ColorPrimaries::Bt709 => BridgePrimaries::Bt709,
                ColorPrimaries::Bt2020 => BridgePrimaries::Bt2020,
                ColorPrimaries::DisplayP3 => BridgePrimaries::DisplayP3,
            },
            matrix: match color.matrix {
                ColorMatrix::Rgb => BridgeMatrix::Rgb,
                ColorMatrix::Bt709 => BridgeMatrix::Bt709,
                ColorMatrix::Bt601 => BridgeMatrix::Bt601,
                ColorMatrix::Bt2020NonConstant => BridgeMatrix::Bt2020Ncl,
            },
            range: match color.range {
                ColorRange::Limited => BridgeRange::Limited,
                ColorRange::Full => BridgeRange::Full,
            },
        },
    }
}

/// The measured evidence for one decoded picture, refusing colour that the
/// stated model-input conversion does not cover.
fn decoded_boundary(
    info: &SourceStreamInfo,
    id: SourceFrameId,
    frame: &Rgba8Frame,
) -> Result<DecodedBoundary, String> {
    let stream = measured_stream(info, frame);
    let model_input = model_input_conversion(&stream).map_err(|refusal| refusal.to_string())?;
    Ok(DecodedBoundary {
        source_frame: id,
        pts: frame.metadata().pts,
        stream,
        model_input,
    })
}

/// The decoded picture at its display aspect, as straight RGBA.
/// [`decoded_boundary`] has already refused rotated, HDR and deep pictures.
pub(super) fn rgba(frame: &Rgba8Frame) -> Result<RgbaImage, String> {
    let metadata = frame.metadata();
    let extent = metadata.clean_aperture.map_or(
        deadpan_core::ExactRatio::integer(i64::from(metadata.width)),
        |rect| rect.rect()[2],
    );
    let aspect = metadata.sample_aspect_ratio;
    let display = extent
        .checked_mul(
            deadpan_core::ExactRatio::new(
                i128::from(aspect.numerator()),
                i128::from(aspect.denominator()),
            )
            .map_err(|e| e.to_string())?,
        )
        .and_then(|v| v.checked_add(deadpan_core::ExactRatio::new(1, 2).expect("half")))
        .map_err(|e| e.to_string())?
        .floor()
        .max(1);
    let display = u32::try_from(display).map_err(|_| "conditioning display width overflowed")?;
    if display > deadpan_render::MAX_DIMENSION
        || u64::from(display) * u64::from(metadata.height) > deadpan_render::MAX_PIXELS
    {
        return Err("conditioning display aspect exceeds the bounded picture raster".into());
    }
    let image = clean_rgba(frame)?;
    if display == image.width() {
        return Ok(image);
    }
    Ok(imageops::resize(
        &image,
        display,
        image.height(),
        imageops::FilterType::Lanczos3,
    ))
}

/// Convert the backing codes first, then sample the clean image, retaining
/// its unrotated pixel aspect. Join measurements use this same conversion.
pub(super) fn clean_rgba(frame: &Rgba8Frame) -> Result<RgbaImage, String> {
    let metadata = frame.metadata();
    if frame.sample_depth() != SampleDepth::Eight {
        return Err("only eight-bit pictures can condition an AI pause".into());
    }
    let codes = super::color::srgb_codes(metadata.color)?;
    let (width, height) = (metadata.width, metadata.height);
    let stride = metadata.row_stride_bytes as usize;
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for row in frame.bytes().chunks_exact(stride).take(height as usize) {
        for pixel in row[..width as usize * 4].chunks_exact(4) {
            pixels.extend_from_slice(&[
                codes[usize::from(pixel[0])],
                codes[usize::from(pixel[1])],
                codes[usize::from(pixel[2])],
                pixel[3],
            ]);
        }
    }
    if let Some(aperture) = metadata.clean_aperture {
        return deadpan_media::analysis_picture::aperture_rgba8(
            &pixels,
            [width, height],
            width as usize * 4,
            aperture.rect(),
            || Ok(()),
        );
    }
    RgbaImage::from_raw(width, height, pixels).ok_or_else(|| "decoded frame layout".into())
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
fn contain(
    picture: Option<&RgbaImage>,
    region: (u32, u32),
) -> Result<(RgbImage, Option<RasterRect>), String> {
    let (width, height) = (super::NATIVE_WIDTH, super::NATIVE_HEIGHT);
    RasterRect::centered(region.0, region.1, [width, height]).map_err(str::to_owned)?;
    let mut canvas: RgbImage = ImageBuffer::from_pixel(width, height, Rgb([0, 0, 0]));
    let Some(picture) = picture else {
        return Ok((canvas, None));
    };
    if picture.width() == 0 || picture.height() == 0 {
        return Err("cannot fit an empty conditioning picture".into());
    }
    let (fitted_width, fitted_height) = fitted((picture.width(), picture.height()), region);
    let content = RasterRect::centered(fitted_width, fitted_height, [width, height])
        .map_err(str::to_owned)?;
    let fitted = imageops::resize(
        picture,
        fitted_width,
        fitted_height,
        imageops::FilterType::Lanczos3,
    );
    for (x, y, pixel) in fitted.enumerate_pixels() {
        canvas.put_pixel(
            content.x + x,
            content.y + y,
            Rgb([pixel[0], pixel[1], pixel[2]]),
        );
    }
    Ok((canvas, Some(content)))
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

pub(crate) fn sha256(bytes: &[u8]) -> Result<Sha256, String> {
    let hex: String = sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Sha256::new(hex).map_err(|error| error.to_string())
}

/// Authored-black boundaries around `plan`'s Hold for tests whose prepared
/// pictures are opaque bytes: their manifest is well formed, but the PNGs are
/// not the black they record.
#[cfg(test)]
pub(crate) fn opaque_boundaries(
    plan: &BridgeGenerationPlan,
    left: Vec<u8>,
    right: Vec<u8>,
) -> (PreparedBoundary, PreparedBoundary) {
    let end = 15 + plan.project_frames().frames();
    (
        PreparedBoundary {
            png: left,
            picture: BoundaryPicture::AuthoredBlack {
                clock: BoundaryClock::Project { frame: 14 },
            },
            content_rect: None,
        },
        PreparedBoundary {
            png: right,
            picture: BoundaryPicture::AuthoredBlack {
                clock: BoundaryClock::Project { frame: end },
            },
            content_rect: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(color: deadpan_source::ColorMetadata) -> SourceStreamInfo {
        SourceStreamInfo {
            nominal_frame_duration_ns: None,
            bwdif_fields: false,
            clean_aperture: None,
            width: 4,
            height: 2,
            stream_index: 0,
            time_base_num: 1,
            time_base_den: 24,
            sample_aspect_num: 1,
            sample_aspect_den: 1,
            rotation_quarter_turns: 0,
            color,
            codec: "ffv1".into(),
            pixel_format: "yuv444p".into(),
            stream_start: Some(0),
            stream_duration: None,
            container_start: None,
            container_duration: None,
            audio_streams: Vec::new(),
        }
    }

    fn sdr(transfer: ColorTransfer, primaries: ColorPrimaries) -> deadpan_source::ColorMetadata {
        deadpan_source::ColorMetadata {
            range: ColorRange::Limited,
            matrix: ColorMatrix::Bt601,
            transfer,
            primaries,
            mastering: None,
            content_light: None,
            ignored_static: deadpan_source::IgnoredStaticMetadata::NONE,
        }
    }

    #[test]
    fn colour_summary_reports_bt709_conversion_and_retained_approximations() {
        let plan = BridgeGenerationPlan::for_conditioning(
            ConditioningMode::Bridge,
            FrameDuration::new(12).unwrap(),
            deadpan_core::FrameRate::new(30, 1).unwrap(),
            &super::super::development_capability(),
            super::super::native_dimensions(),
        )
        .unwrap();
        let constraints = HoldConstraints {
            video: VideoSpec::new(
                FrameDuration::new(12).unwrap(),
                deadpan_core::FrameRate::new(30, 1).unwrap(),
                super::super::NATIVE_WIDTH,
                super::super::NATIVE_HEIGHT,
            )
            .unwrap(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
            instructions: None,
            region_target: None,
        };
        let picture = frame(&RgbImage::from_pixel(4, 2, Rgb([1, 2, 3])));
        let (mut left, right) = opaque_boundaries(&plan, b"l".to_vec(), b"r".to_vec());
        left.picture = BoundaryPicture::Original {
            clock: BoundaryClock::Project { frame: 14 },
            asset: deadpan_core::AssetId::new("original").unwrap(),
            qualification: deadpan_core::SourceQualificationId::new("a".repeat(64)).unwrap(),
            picture: decoded_boundary(
                &info(sdr(ColorTransfer::Bt709, ColorPrimaries::Bt709)),
                SourceFrameId(26),
                &picture,
            )
            .unwrap(),
        };
        let presentation = RasterRect::new(
            0,
            0,
            super::super::NATIVE_WIDTH,
            super::super::NATIVE_HEIGHT,
        )
        .unwrap();
        left.content_rect = Some(RasterRect::centered(640, 320, [768, 320]).unwrap());
        let inputs = assemble(plan.clone(), constraints, left, right, presentation).unwrap();
        let colour = ConditioningColour::from_manifest(&inputs.manifest).unwrap();
        assert!(!colour.approximate());
        assert_eq!(
            colour.to_json(),
            serde_json::json!({"left": "rec709_to_srgb", "right": "authored_black", "approximate": false})
        );
        assert_eq!(
            colour.describe(),
            "Model input colour is sRGB (before: BT.709 converted to sRGB, after: black)."
        );
        let mut retained: serde_json::Value = serde_json::from_slice(&inputs.manifest).unwrap();
        retained["boundaries"]["left"]["original"]["picture"]["model_input"] =
            serde_json::json!("rec709_codes_as_srgb");
        let retained =
            ConditioningColour::from_manifest(&serde_json::to_vec(&retained).unwrap()).unwrap();
        assert!(retained.approximate());
        assert_eq!(
            retained.describe(),
            "Model input colour includes an older approximation (before: BT.709 read as sRGB, after: black)."
        );
        let mixed = ConditioningColour {
            left: Some(ModelInputConversion::Rec709CodesAsSrgb),
            right: Some(ModelInputConversion::Rec709ToSrgb),
            unmeasured: false,
        };
        assert_eq!(
            mixed.describe(),
            "Model input colour includes an older approximation (before: BT.709 read as sRGB, after: BT.709 converted to sRGB)."
        );
        let legacy = BridgeContext::legacy_v1(
            plan,
            WorkspaceArtifact::new(WorkspaceRef::new(LEFT).unwrap(), sha256(b"l").unwrap(), 1)
                .unwrap(),
            WorkspaceArtifact::new(WorkspaceRef::new(RIGHT).unwrap(), sha256(b"r").unwrap(), 1)
                .unwrap(),
            "stated",
        )
        .unwrap();
        let legacy =
            ConditioningColour::from_manifest(&serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(legacy.unmeasured && !legacy.approximate());
        assert_eq!(legacy.to_json()["unmeasured"], true);
        assert!(ConditioningColour::from_manifest(b"{}").is_none());
    }

    #[test]
    fn measured_colour_is_recorded_and_uncovered_colour_refuses() {
        let picture = frame(&RgbImage::from_pixel(4, 2, Rgb([1, 2, 3])));
        let measured = decoded_boundary(
            &info(sdr(ColorTransfer::Bt709, ColorPrimaries::Bt709)),
            SourceFrameId(7),
            &picture,
        )
        .unwrap();
        assert_eq!(measured.source_frame, SourceFrameId(7));
        assert_eq!(measured.pts, picture.metadata().pts);
        assert_eq!(
            measured.stream.color,
            BridgeColor {
                transfer: BridgeTransfer::Bt709,
                primaries: BridgePrimaries::Bt709,
                matrix: BridgeMatrix::Bt601,
                range: BridgeRange::Limited,
            }
        );
        assert_eq!(measured.stream.pixel_format, "yuv444p");
        assert_eq!(measured.stream.decoded_sample_bits, 8);
        assert_eq!(
            measured.model_input,
            deadpan_models::ModelInputConversion::Rec709ToSrgb
        );
        for (transfer, primaries, reason) in [
            (
                ColorTransfer::Bt709,
                ColorPrimaries::DisplayP3,
                "display_p3 primaries",
            ),
            (
                ColorTransfer::Bt709,
                ColorPrimaries::Bt2020,
                "bt2020 primaries",
            ),
            (ColorTransfer::Linear, ColorPrimaries::Bt709, "linear-light"),
            (ColorTransfer::Pq, ColorPrimaries::Bt2020, "HDR"),
        ] {
            let error =
                decoded_boundary(&info(sdr(transfer, primaries)), SourceFrameId(7), &picture)
                    .unwrap_err();
            assert!(error.contains(reason), "{error}");
        }
        let mut rotated = info(sdr(ColorTransfer::Srgb, ColorPrimaries::Bt709));
        rotated.rotation_quarter_turns = 1;
        assert!(
            decoded_boundary(&rotated, SourceFrameId(7), &picture)
                .unwrap_err()
                .contains("rotated")
        );
    }

    fn frame(image: &RgbImage) -> Rgba8Frame {
        let rgba: Vec<u8> = image
            .pixels()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect();
        Rgba8Frame::new(
            deadpan_render::FrameMetadata {
                clean_aperture: None,
                width: image.width(),
                height: image.height(),
                row_stride_bytes: image.width() * 4,
                sample_aspect_ratio: deadpan_render::SampleAspectRatio::SQUARE,
                rotation: deadpan_render::Rotation::None,
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
    fn conditioning_and_join_measurements_sample_the_same_fractional_clean_image() {
        use deadpan_core::ExactRatio;
        let mut ramp = RgbImage::new(10, 8);
        for (x, y, pixel) in ramp.enumerate_pixels_mut() {
            *pixel = Rgb([(x * 20) as u8, (y * 20) as u8, 0]);
        }
        let backing = frame(&ramp);
        let mut metadata = *backing.metadata();
        let bounds = [(1, 2), (5, 4), (8, 1), (11, 2)].map(|(n, d)| ExactRatio::new(n, d).unwrap());
        metadata.clean_aperture = Some(deadpan_render::CleanAperture::new(bounds).unwrap());
        let picture = Rgba8Frame::new(metadata, backing.bytes().to_vec()).unwrap();
        let conditioning = rgba(&picture).unwrap();
        let joins = super::super::joins::rgb(&picture).unwrap();
        assert_eq!(conditioning.dimensions(), (8, 6));
        assert_eq!((joins.width, joins.height), (8, 6));
        for (x, y, pixel) in conditioning.enumerate_pixels() {
            let expected = [
                ((f64::from(x) + 0.5) * 20.0).round() as u8,
                ((0.75 + (f64::from(y) + 0.5) * 5.5 / 6.0) * 20.0).round() as u8,
                0,
            ];
            assert_eq!(&pixel.0[..3], &expected);
            let offset = (y * 8 + x) as usize * 3;
            assert_eq!(&joins.rgb[offset..offset + 3], &expected);
        }
        let mut stream = info(sdr(ColorTransfer::Srgb, ColorPrimaries::Bt709));
        stream.width = 10;
        stream.height = 8;
        stream.clean_aperture = Some(bounds);
        let boundary = decoded_boundary(&stream, SourceFrameId(0), &picture).unwrap();
        assert_eq!(boundary.stream.clean_aperture, Some(bounds));
        let encoded = serde_json::to_vec(&boundary).unwrap();
        let roundtrip: DecodedBoundary = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(roundtrip, boundary);
    }

    #[test]
    fn prepared_png_converts_bt709_and_keeps_srgb_padding_and_alpha_separate() {
        let original = frame(&RgbImage::from_pixel(2, 2, Rgb([20, 64, 128])));
        let mut metadata = *original.metadata();
        metadata.color.transfer = deadpan_render::Transfer::Rec709;
        metadata.row_stride_bytes = 12;
        let padded = [
            20, 64, 128, 255, 20, 64, 128, 71, 199, 199, 199, 199, 20, 64, 128, 255, 20, 64, 128,
            71, 199, 199, 199, 199,
        ];
        let decoded = Rgba8Frame::new(metadata, padded.to_vec()).unwrap();
        let prepared = rgba(&decoded).unwrap();
        assert_eq!(prepared.get_pixel(0, 0).0, [36, 79, 140, 255]);
        assert_eq!(prepared.get_pixel(1, 1).0, [36, 79, 140, 71]);
        assert_eq!(prepared.len(), 16, "row padding is not picture content");
        let png = encode(&contain(Some(&prepared), (320, 320)).unwrap().0).unwrap();
        let input = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(input.get_pixel(384, 160).0, [36, 79, 140]);
        assert_eq!(input.get_pixel(0, 160).0, [0, 0, 0]);
        assert_eq!(
            rgba(&original).unwrap().get_pixel(0, 0).0,
            [20, 64, 128, 255]
        );
    }

    #[test]
    fn conditioning_and_the_presentation_crop_cancel_out() {
        let orange = image::Rgba([200, 100, 50, 255]);
        // A 16:9 picture on a 16:9 canvas fills the canvas region: bars only
        // at the sides of the 2.4:1 raster.
        let region = canvas_region([1920, 1080]);
        assert_eq!(region, (569, 320));
        let (canvas, content) =
            contain(Some(&RgbaImage::from_pixel(1920, 1080, orange)), region).unwrap();
        assert_eq!(content, Some(RasterRect::new(99, 0, 569, 320).unwrap()));
        assert_eq!(canvas.dimensions(), (768, 320));
        assert_eq!(canvas.get_pixel(0, 160), &Rgb([0, 0, 0]), "left bar");
        assert_eq!(canvas.get_pixel(384, 160), &Rgb([200, 100, 50]), "picture");
        let (black, black_content) = contain(None, region).unwrap();
        assert_eq!(black.get_pixel(384, 160), &Rgb([0, 0, 0]));
        assert_eq!(black_content, None);
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
        let (narrow, content) =
            contain(Some(&RgbaImage::from_pixel(640, 480, orange)), region).unwrap();
        assert_eq!(content, Some(RasterRect::new(170, 0, 427, 320).unwrap()));
        assert_eq!(narrow.get_pixel(384 - 260, 160), &Rgb([0, 0, 0]));
        assert_eq!(narrow.get_pixel(384, 160), &Rgb([200, 100, 50]));
    }

    #[test]
    fn captured_rectangles_match_portrait_odd_and_anamorphic_preparation() {
        let orange = image::Rgba([200, 100, 50, 255]);
        for (canvas_size, picture_size) in [
            ([1080, 1920], [1080, 1920]),
            ([1001, 999], [641, 479]),
            ([3, 1], [4, 3]),
        ] {
            let region = canvas_region(canvas_size);
            let presentation = RasterRect::centered(region.0, region.1, [768, 320]).unwrap();
            let (raster, content) = contain(
                Some(&RgbaImage::from_pixel(
                    picture_size[0],
                    picture_size[1],
                    orange,
                )),
                region,
            )
            .unwrap();
            let content = content.unwrap();
            assert!(content.x >= presentation.x && content.y >= presentation.y);
            assert!(content.x + content.width <= presentation.x + presentation.width);
            assert!(content.y + content.height <= presentation.y + presentation.height);
            for (x, y, pixel) in raster.enumerate_pixels() {
                let inside = (content.x..content.x + content.width).contains(&x)
                    && (content.y..content.y + content.height).contains(&y);
                assert_eq!(
                    *pixel,
                    if inside {
                        Rgb([200, 100, 50])
                    } else {
                        Rgb([0, 0, 0])
                    }
                );
            }
            let cropped = crate::picture::fill_canvas_aspect(frame(&raster), canvas_size).unwrap();
            assert_eq!(
                (cropped.metadata().width, cropped.metadata().height),
                (presentation.width, presentation.height)
            );
        }

        // Apply sample aspect before fitting, as the production rgba path does.
        let source = frame(&RgbImage::from_pixel(1440, 1080, Rgb([200, 100, 50])));
        let mut metadata = *source.metadata();
        metadata.sample_aspect_ratio = deadpan_render::SampleAspectRatio::new(4, 3).unwrap();
        let anamorphic = Rgba8Frame::new(metadata, source.bytes().to_vec()).unwrap();
        let expanded = rgba(&anamorphic).unwrap();
        assert_eq!(expanded.dimensions(), (1920, 1080));
        let (_, content) = contain(Some(&expanded), canvas_region([1920, 1080])).unwrap();
        assert_eq!(content, Some(RasterRect::new(99, 0, 569, 320).unwrap()));

        // Real decoded black has content; it is not an authored background.
        let (_, black_content) = contain(Some(&RgbaImage::new(4, 2)), (768, 320)).unwrap();
        assert_eq!(
            black_content,
            Some(RasterRect::new(64, 0, 640, 320).unwrap())
        );
        assert!(contain(Some(&RgbaImage::new(0, 2)), (768, 320)).is_err());
        assert!(contain(None, (769, 320)).is_err());
        assert!(contain(None, (768, 0)).is_err());
    }

    #[test]
    fn conditioning_refuses_extreme_sample_aspect_before_display_allocation() {
        let source = frame(&RgbImage::from_pixel(720, 480, Rgb([200, 100, 50])));
        let mut metadata = *source.metadata();
        for (numerator, denominator) in [(65_535, 1), (u32::MAX, 1)] {
            metadata.sample_aspect_ratio =
                deadpan_render::SampleAspectRatio::new(numerator, denominator).unwrap();
            let hostile = Rgba8Frame::new(metadata, source.bytes().to_vec()).unwrap();
            assert!(rgba(&hostile).unwrap_err().contains("display"));
        }
        metadata.sample_aspect_ratio = deadpan_render::SampleAspectRatio::new(8, 9).unwrap();
        let narrow = Rgba8Frame::new(metadata, source.bytes().to_vec()).unwrap();
        assert_eq!(rgba(&narrow).unwrap().dimensions(), (640, 480));
    }
}
