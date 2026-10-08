//! Internal consistency of retained input observations. Authoritative store
//! recapture still has to prove the Original index and the actual picture map.

use std::io;

use deadpan_jobs::{
    ExtensionCapturePolicy, GenerationCaptureSpec, GenerationInputBinding, GenerationInputSupport,
    GenerationInputs, GenerationPictureIdentity, MAX_INPUT_BINDING_BYTES,
    RelativeGenerationPicture,
};

use super::*;

pub(super) fn validate(
    binding: &GenerationInputBinding,
    plan: &ExtensionGenerationPlan,
    context: &[ExtensionContextPicture],
    presentation: RasterRect,
    opposite: &ExtensionOppositeSeam,
    region: &ExtensionRegionCapture,
) -> Result<(), QualificationError> {
    validate_size(binding)?;
    let GenerationInputs::Extension {
        capture,
        samples,
        opposite: bound_opposite,
        support,
        terminal,
    } = &binding.inputs
    else {
        return Err(extension_error("input binding is not an extension"));
    };
    let expected_capture = GenerationCaptureSpec::Extension {
        direction: plan.direction(),
        native_rate: plan.native_frame_rate(),
        context_frames: plan.context_frame_count(),
        policy: ExtensionCapturePolicy::TemporalContextV1,
    };
    if *capture != expected_capture
        || binding.duration != plan.project_frames()
        || binding.frame_rate != plan.project_frame_rate()
        || samples.len() != context.len()
        || u32::try_from(samples.len()).ok() != Some(plan.context_frame_count())
        || context.len() > MAXIMUM_EXTENSION_CONTEXT_FRAMES
    {
        return Err(extension_error(
            "input binding differs from the extension plan",
        ));
    }
    let dimensions = plan.native_dimensions();
    let native = [dimensions.width(), dimensions.height()];
    if presentation != canvas_presentation(binding.canvas, native)? {
        return Err(extension_error(
            "input canvas differs from the presentation rectangle",
        ));
    }
    let anchor = match plan.direction() {
        ExtensionDirection::FromLeft => context.last(),
        ExtensionDirection::FromRight => context.first(),
    }
    .ok_or_else(|| extension_error("input binding has no context anchor"))?;
    for (sample, item) in samples.iter().zip(context) {
        validate_sample(sample, &item.picture, &anchor.picture)?;
    }
    match (bound_opposite, opposite) {
        (None, ExtensionOppositeSeam::Absent) => {}
        (Some(sample), ExtensionOppositeSeam::PresentUnconditioned { picture, .. }) => {
            validate_sample(sample, picture, &anchor.picture)?
        }
        _ => {
            return Err(extension_error(
                "input binding has a different opposite seam",
            ));
        }
    }
    validate_support(samples, support, terminal)?;
    validate_region(binding, region, anchor, presentation, native)
}

fn validate_size(binding: &GenerationInputBinding) -> Result<(), QualificationError> {
    struct LimitedBytes(usize);
    impl io::Write for LimitedBytes {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self
                .0
                .checked_sub(bytes.len())
                .ok_or_else(|| io::Error::other("input binding exceeds its byte limit"))?;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // Count through a bounded sink instead of retaining a second serialized
    // copy of the descriptor, including a potentially large target record.
    serde_json::to_writer(LimitedBytes(MAX_INPUT_BINDING_BYTES), binding)
        .map_err(|error| extension_error(&error.to_string()))
}

fn canvas_presentation(
    canvas: [u32; 2],
    native: [u32; 2],
) -> Result<RasterRect, QualificationError> {
    if canvas.into_iter().any(|side| !(1..=65_536).contains(&side)) {
        return Err(extension_error(
            "input canvas dimensions must be in 1..=65536",
        ));
    }
    let [width, height] = native;
    let [aspect_width, aspect_height] = canvas.map(u128::from);
    // Match conditioning::canvas_region / picture::aspect_region: keep the
    // long fitted side, round the shorter side half up, then center by floor.
    let half_up = |numerator: u128, denominator: u128, bound: u32| {
        u32::try_from((2 * numerator + denominator) / (2 * denominator))
            .map(|side| side.clamp(1, bound))
            .map_err(|_| extension_error("input canvas fit overflows"))
    };
    let raster = u128::from(width) * aspect_height;
    let wanted = u128::from(height) * aspect_width;
    let fitted = if raster > wanted {
        [
            half_up(u128::from(height) * aspect_width, aspect_height, width)?,
            height,
        ]
    } else if raster < wanted {
        [
            width,
            half_up(u128::from(width) * aspect_height, aspect_width, height)?,
        ]
    } else {
        native
    };
    RasterRect::centered(fitted[0], fitted[1], native).map_err(extension_error)
}

fn validate_sample(
    sample: &RelativeGenerationPicture,
    picture: &BoundaryPicture,
    anchor: &BoundaryPicture,
) -> Result<(), QualificationError> {
    if sample.position != clock_difference(anchor, picture)? {
        return Err(extension_error(
            "input sample differs from its relative definition coordinate",
        ));
    }
    validate_identity(&sample.picture)?;
    let matches = match (&sample.picture, picture) {
        (
            GenerationPictureIdentity::Original {
                qualification,
                frame,
            },
            BoundaryPicture::Original {
                qualification: expected,
                picture,
                ..
            },
        ) => qualification == expected && *frame == picture.source_frame,
        (
            GenerationPictureIdentity::Generated {
                sampled_object,
                frame,
                ..
            },
            BoundaryPicture::Generated {
                sampled_object: expected,
                picture,
                ..
            },
        ) => sampled_object == expected && *frame == picture.source_frame,
        (GenerationPictureIdentity::AuthoredBlack, BoundaryPicture::AuthoredBlack { .. }) => true,
        _ => false,
    };
    if !matches {
        return Err(extension_error(
            "input sample differs from its decoded picture identity",
        ));
    }
    Ok(())
}

fn validate_identity(identity: &GenerationPictureIdentity) -> Result<(), QualificationError> {
    if let GenerationPictureIdentity::Generated {
        content_aspect: Some([width, height]),
        ..
    } = identity
    {
        let (mut divisor, mut remainder) = (*width, *height);
        while remainder != 0 {
            (divisor, remainder) = (remainder, divisor % remainder);
        }
        if *width == 0 || *height == 0 || divisor != 1 {
            return Err(extension_error(
                "generated content aspect must be positive and reduced",
            ));
        }
    }
    // BoundaryPicture does not retain the generated crop recipe. A well-formed
    // ratio here does not prove that recipe; authoritative recapture must do so.
    Ok(())
}

fn validate_support(
    samples: &[RelativeGenerationPicture],
    support: &[GenerationInputSupport],
    terminal: &RelativeGenerationPicture,
) -> Result<(), QualificationError> {
    let first = samples
        .first()
        .ok_or_else(|| extension_error("input support has no samples"))?;
    if samples.last() != Some(terminal)
        || samples
            .windows(2)
            .any(|pair| !pair[0].position.compare(pair[1].position).is_lt())
    {
        return Err(extension_error(
            "input terminal or chronological sample order differs",
        ));
    }
    let mut next = first.position;
    for span in support {
        if span.start != next
            || !span.start.compare(span.end_exclusive).is_lt()
            || span.end_exclusive.compare(terminal.position).is_gt()
        {
            return Err(extension_error(
                "input support must cover the context without gaps or overlap",
            ));
        }
        validate_identity(&span.first)?;
        validate_identity(&span.last)?;
        if !same_provider(&span.first, &span.last) {
            return Err(extension_error(
                "input support changes provider inside a span",
            ));
        }
        for sample in samples {
            if sample.position.compare(span.start).is_lt()
                || !sample.position.compare(span.end_exclusive).is_lt()
            {
                continue;
            }
            if (sample.position == span.start && sample.picture != span.first)
                || !inside_provider_span(&sample.picture, &span.first, &span.last)
            {
                return Err(extension_error(
                    "input sample differs from its half-open support span",
                ));
            }
        }
        next = span.end_exclusive;
    }
    if next != terminal.position {
        return Err(extension_error(
            "input support does not reach the terminal sample",
        ));
    }
    Ok(())
}

fn same_provider(left: &GenerationPictureIdentity, right: &GenerationPictureIdentity) -> bool {
    match (left, right) {
        (
            GenerationPictureIdentity::Original {
                qualification: left,
                ..
            },
            GenerationPictureIdentity::Original {
                qualification: right,
                ..
            },
        ) => left == right,
        (
            GenerationPictureIdentity::Generated {
                sampled_object: left,
                content_aspect: left_aspect,
                ..
            },
            GenerationPictureIdentity::Generated {
                sampled_object: right,
                content_aspect: right_aspect,
                ..
            },
        ) => left == right && left_aspect == right_aspect,
        (GenerationPictureIdentity::AuthoredBlack, GenerationPictureIdentity::AuthoredBlack) => {
            true
        }
        _ => false,
    }
}

fn inside_provider_span(
    sample: &GenerationPictureIdentity,
    first: &GenerationPictureIdentity,
    last: &GenerationPictureIdentity,
) -> bool {
    if !same_provider(sample, first) {
        return false;
    }
    let ordinal = |identity: &GenerationPictureIdentity| match identity {
        GenerationPictureIdentity::Original { frame, .. }
        | GenerationPictureIdentity::Generated { frame, .. } => Some(frame.0),
        GenerationPictureIdentity::AuthoredBlack => None,
    };
    match (ordinal(sample), ordinal(first), ordinal(last)) {
        (Some(sample), Some(first), Some(last)) => {
            (first.min(last)..=first.max(last)).contains(&sample)
        }
        (None, None, None) => true,
        _ => false,
    }
}

fn validate_region(
    binding: &GenerationInputBinding,
    region: &ExtensionRegionCapture,
    anchor: &ExtensionContextPicture,
    presentation: RasterRect,
    native: [u32; 2],
) -> Result<(), QualificationError> {
    match (region, &binding.region) {
        (ExtensionRegionCapture::None, None) => Ok(()),
        (ExtensionRegionCapture::Selected { target, .. }, Some(bound)) if target == &bound.id => {
            let record = bound.record.as_ref().ok_or_else(|| {
                extension_error("selected input region has no retained target record")
            })?;
            record
                .validate_shape()
                .map_err(|error| extension_error(&error.to_string()))?;
            let expected =
                ExtensionRegionCapture::new(target.clone(), record, anchor, presentation, native)?;
            if &expected != region {
                return Err(extension_error(
                    "input target record differs from the captured region",
                ));
            }
            Ok(())
        }
        _ => Err(extension_error(
            "input binding has a different region selection",
        )),
    }
}

#[cfg(test)]
#[path = "binding/tests.rs"]
mod tests;
