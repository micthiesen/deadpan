//! Capture one-sided temporal inputs from one immutable project revision.
//!
//! This development envelope is deliberately separate from approved provider
//! capabilities. The continuity qualifier runs before PNG preparation; neither
//! this module nor its result queues work, publishes Ready, or writes history.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use deadpan_core::{AttentionTarget, ExtensionDirection, FrameRate, TargetId};
use deadpan_jobs::{
    AxisLimits, DimensionLimits, ExtensionCapability, ExtensionGenerationPlan, FrameCountFormula,
};
use deadpan_models::{
    ExtensionContext, ExtensionContextPicture, ExtensionOppositeSeam, ExtensionRegionCapture,
    MAXIMUM_EXTENSION_INPUT_BYTES,
};
use deadpan_plan::ScopedHoldContextRequest;

use super::continuity::{ExtensionContextContinuity, qualify_extension_context};
use super::*;
use crate::generation::{NATIVE_HEIGHT, NATIVE_WIDTH, native_dimensions};

const CONTEXT_FRAMES: u32 = 9;
const GENERATED_FRAMES: u32 = 8;
const MAXIMUM_MANIFEST_BYTES: usize = 1024 * 1024;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(120);
const OPPOSITE: &str = "inputs/opposite.png";

/// Retained preparation data for the bounded development extension path.
#[derive(Debug, Clone)]
pub struct ExtensionInputs {
    pub plan: ExtensionGenerationPlan,
    pub constraints: HoldConstraints,
    /// Chronological inputs, named `inputs/context-000.png` through `008.png`.
    pub context_pngs: Vec<Vec<u8>>,
    /// This optional seam is retained for checks, never supplied as conditioning.
    pub opposite_png: Option<Vec<u8>>,
    pub manifest: Vec<u8>,
    pub manifest_sha256: Sha256,
    pub continuity: ExtensionContextContinuity,
}

/// Capture native-spaced context inside the Hold's exact authored definition.
/// Definition edges remain absent, including dormant or shared Repeat scopes.
/// The single deadline is checked between bounded calls. Decoder calls retain
/// their own cooperative timeouts; this is not a preemptive wall-time guarantee.
pub fn prepare_extension_scoped_with_options(
    package: &Path,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
    direction: ExtensionDirection,
    options: &GenerationOptions,
    cancelled: &AtomicBool,
) -> Result<ExtensionInputs, String> {
    let deadline = Instant::now()
        .checked_add(CAPTURE_TIMEOUT)
        .ok_or("The AI extension capture deadline overflowed.")?;
    let control = CaptureControl {
        cancelled,
        deadline,
    };
    control.check()?;
    let mut session = ProjectPictureSession::open_revision(package, revision, None, cancelled)
        .map_err(|error| error.to_string())?;
    control.check()?;
    let document = session.document().clone();
    let rate = document.presentation_basis().frame_rate;
    validate_project_rate(rate)?;
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
    let context = session
        .plan()
        .scoped_hold_context(
            &ScopedHoldContextRequest {
                target: target.clone(),
                direction,
                native_rate: native_rate(),
                frame_count: CONTEXT_FRAMES,
            },
            BoundaryQueryLimits::default(),
        )
        .map_err(|error| error.to_string())?;
    control.check()?;
    let plan = development_plan(direction, context.boundaries.duration, rate)?;
    // Structural lookup alone does not establish continuous same-shot context.
    // The qualifier consumes the same immutable session, samples and deadline.
    let continuity = qualify_extension_context(&mut session, &context, cancelled, deadline)?;
    control.check()?;
    let mut constraints = HoldConstraints {
        video: VideoSpec::new(
            context.boundaries.duration,
            rate,
            NATIVE_WIDTH,
            NATIVE_HEIGHT,
        )
        .map_err(|error| error.to_string())?,
        conditioning: mode(direction),
        motion: MotionAmount::Still,
        instructions: None,
        region_target: None,
    };
    options.apply_to(&mut constraints);
    let basis = document.presentation_basis();
    let region = canvas_region([basis.width, basis.height]);
    let presentation = RasterRect::centered(region.0, region.1, [NATIVE_WIDTH, NATIVE_HEIGHT])
        .map_err(str::to_owned)?;
    let mut prepared = Vec::with_capacity(CONTEXT_FRAMES as usize);
    let mut input_bytes = 0_u64;
    for sample in &context.pictures {
        control.check()?;
        let picture = boundary(
            &mut session,
            &context.boundaries.definition,
            sample.position,
            region,
            "in the temporal context of",
            cancelled,
        )?;
        control.check()?;
        add_input_bytes(&mut input_bytes, picture.png.len())?;
        prepared.push(picture);
    }
    let opposite_sample = match direction {
        ExtensionDirection::FromLeft => context.boundaries.right.as_ref(),
        ExtensionDirection::FromRight => context.boundaries.left.as_ref(),
    };
    let opposite = opposite_sample
        .map(|sample| {
            control.check()?;
            let picture = boundary(
                &mut session,
                &context.boundaries.definition,
                sample.position,
                region,
                "at the unconditioned seam of",
                cancelled,
            )?;
            control.check()?;
            add_input_bytes(&mut input_bytes, picture.png.len())?;
            Ok::<_, String>(picture)
        })
        .transpose()?;
    let assembled = assemble_captured(
        &plan,
        &constraints,
        PreparedExtension {
            context: prepared,
            opposite,
            presentation,
        },
        captured_region,
        &control,
    )?;
    control.check()?;
    Ok(ExtensionInputs {
        plan,
        constraints,
        context_pngs: assembled.context_pngs,
        opposite_png: assembled.opposite_png,
        manifest: assembled.manifest,
        manifest_sha256: assembled.manifest_sha256,
        continuity,
    })
}

fn native_rate() -> FrameRate {
    FrameRate::new(24, 1).expect("constant extension native rate")
}

fn mode(direction: ExtensionDirection) -> ConditioningMode {
    match direction {
        ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
        ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
    }
}

fn development_plan(
    direction: ExtensionDirection,
    duration: FrameDuration,
    rate: FrameRate,
) -> Result<ExtensionGenerationPlan, String> {
    validate_project_rate(rate)?;
    // floor(P/3) is the exact whole-project-frame bound for 8/24 seconds.
    // No rounding of the user's authored interval or floating point is used.
    let maximum = u64::from(rate.numerator()) / (3 * u64::from(rate.denominator()));
    if duration == FrameDuration::ZERO
        || maximum == 0
        || duration.frames() > i64::try_from(maximum).map_err(|error| error.to_string())?
    {
        return Err("Development AI extensions require a positive Hold of at most 1/3 second; the authored duration was not changed.".into());
    }
    let capability = ExtensionCapability::new(
        native_rate(),
        CONTEXT_FRAMES,
        FrameCountFormula::new(8, 0, GENERATED_FRAMES, GENERATED_FRAMES)
            .map_err(|error| error.to_string())?,
        DimensionLimits::new(
            AxisLimits::new(NATIVE_WIDTH, NATIVE_WIDTH, 64).map_err(|error| error.to_string())?,
            AxisLimits::new(NATIVE_HEIGHT, NATIVE_HEIGHT, 64).map_err(|error| error.to_string())?,
        ),
        FrameDuration::new(i64::try_from(maximum).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    ExtensionGenerationPlan::new(direction, duration, rate, &capability, native_dimensions())
        .map_err(|error| error.to_string())
}

fn validate_project_rate(rate: FrameRate) -> Result<(), String> {
    let numerator = u64::from(rate.numerator());
    let denominator = u64::from(rate.denominator());
    if numerator < denominator || numerator > 120 * denominator {
        return Err(
            "Development AI extensions require a project frame rate from 1 through 120 fps.".into(),
        );
    }
    Ok(())
}

struct CaptureControl<'a> {
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl CaptureControl<'_> {
    fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err("The AI extension was cancelled.".into())
        } else if Instant::now() >= self.deadline {
            Err(
                "AI extension input preparation exceeded its 120-second development deadline."
                    .into(),
            )
        } else {
            Ok(())
        }
    }
}

struct PreparedExtension {
    context: Vec<PreparedBoundary>,
    opposite: Option<PreparedBoundary>,
    presentation: RasterRect,
}

struct AssembledExtension {
    context_pngs: Vec<Vec<u8>>,
    opposite_png: Option<Vec<u8>>,
    manifest: Vec<u8>,
    manifest_sha256: Sha256,
}

fn assemble_captured(
    plan: &ExtensionGenerationPlan,
    constraints: &HoldConstraints,
    prepared: PreparedExtension,
    target: Option<(&TargetId, &AttentionTarget)>,
    control: &CaptureControl<'_>,
) -> Result<AssembledExtension, String> {
    control.check()?;
    if development_plan(
        plan.direction(),
        plan.project_frames(),
        plan.project_frame_rate(),
    )? != *plan
        || prepared.context.len() != CONTEXT_FRAMES as usize
        || constraints.conditioning != mode(plan.direction())
        || constraints.video.frames() != plan.project_frames()
        || constraints.video.frame_rate() != plan.project_frame_rate()
        || constraints.video.width() != NATIVE_WIDTH
        || constraints.video.height() != NATIVE_HEIGHT
    {
        return Err(
            "Extension inputs differ from the captured plan or development envelope.".into(),
        );
    }
    let mut bytes = 0_u64;
    for frame in prepared.context.iter().chain(prepared.opposite.iter()) {
        add_input_bytes(&mut bytes, frame.png.len())?;
    }
    let mut context = Vec::with_capacity(prepared.context.len());
    let mut context_pngs = Vec::with_capacity(prepared.context.len());
    for (index, frame) in prepared.context.into_iter().enumerate() {
        control.check()?;
        context.push(ExtensionContextPicture {
            picture: frame.picture,
            frame: artifact(&format!("inputs/context-{index:03}.png"), &frame.png)?,
            content: frame.content_rect,
        });
        context_pngs.push(frame.png);
    }
    let anchor = match plan.direction() {
        ExtensionDirection::FromLeft => context.last(),
        ExtensionDirection::FromRight => context.first(),
    }
    .ok_or("The AI extension has no conditioning anchor.")?;
    let region = target
        .map(|(id, record)| {
            ExtensionRegionCapture::new(
                id.clone(),
                record,
                anchor,
                prepared.presentation,
                [NATIVE_WIDTH, NATIVE_HEIGHT],
            )
            .map_err(|error| error.to_string())
        })
        .transpose()?
        .unwrap_or(ExtensionRegionCapture::None);
    if constraints.region_target.as_ref() != region.target_id() {
        return Err("Region target controls differ from captured extension conditioning.".into());
    }
    let (opposite, opposite_png) = match prepared.opposite {
        Some(frame) => (
            ExtensionOppositeSeam::PresentUnconditioned {
                picture: Box::new(frame.picture),
                frame: artifact(OPPOSITE, &frame.png)?,
                content: frame.content_rect,
            },
            Some(frame.png),
        ),
        None => (ExtensionOppositeSeam::Absent, None),
    };
    let context = ExtensionContext::new(
        plan.clone(),
        context,
        prepared.presentation,
        opposite,
        INPUT_COLOR_INTERPRETATION,
        MODEL_COLOR_SPACE,
        region,
    )
    .map_err(|error| error.to_string())?;
    control.check()?;
    let manifest = serde_json::to_vec(&context).map_err(|error| error.to_string())?;
    if manifest.len() > MAXIMUM_MANIFEST_BYTES {
        return Err("Extension context manifest exceeds 1 MiB.".into());
    }
    let manifest_sha256 = sha256(&manifest)?;
    control.check()?;
    Ok(AssembledExtension {
        context_pngs,
        opposite_png,
        manifest,
        manifest_sha256,
    })
}

fn artifact(reference: &str, bytes: &[u8]) -> Result<WorkspaceArtifact, String> {
    WorkspaceArtifact::new(
        WorkspaceRef::new(reference).map_err(|error| error.to_string())?,
        sha256(bytes)?,
        u64::try_from(bytes.len()).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn add_input_bytes(total: &mut u64, length: usize) -> Result<(), String> {
    *total = total
        .checked_add(u64::try_from(length).map_err(|error| error.to_string())?)
        .filter(|value| *value <= MAXIMUM_EXTENSION_INPUT_BYTES)
        .ok_or("Extension conditioning PNGs exceed the aggregate 16 MiB input limit.")?;
    Ok(())
}

#[cfg(test)]
#[path = "extension_tests.rs"]
mod tests;
