//! Exact structural coverage plus bounded visual transition rejection.
//!
//! The shot rule is a rejection heuristic, never a guarantee of scene identity.
//! Model inputs and support are relative to the anchor so editorial group moves
//! do not change their identity. This report does not itself grant job relevance
//! or accepted-media authority; those paths must independently recapture it.

use std::time::Instant;

use deadpan_analysis::{
    CONTEXT_SHOT_RULE, ContextShotQualification, MAX_CONTEXT_SHOT_SIGNATURES, PictureSignature,
    context_seam_change, context_shot_window, qualify_context,
};
use deadpan_core::{EndpointPolicy, ExtensionDirection, SourceFrameIndex};
use deadpan_plan::{DefinitionPictureSpan, Picture, PictureClockSlope, ScopedHoldContext};
use deadpan_store::generation_pictures::GenerationPictureIdentity;
use serde::Serialize;

use super::*;

const CAPTURE_POLICY: &str = "deadpan-extension-context-1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionContextInputIdentity {
    pub relative_position: ExactRatio,
    pub picture: GenerationPictureIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionContextSupport {
    pub relative_start: ExactRatio,
    pub relative_end_exclusive: ExactRatio,
    pub first: GenerationPictureIdentity,
    pub last: GenerationPictureIdentity,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionContextMeasurement {
    /// The provider and first covered ordinal, including its immutable receipt.
    pub source: GenerationPictureIdentity,
    pub qualification: ContextShotQualification,
}

/// Host observations for this immutable capture, separate from the worker's
/// manifest. No absolute revision or editorial framing enters input identity.
#[derive(Debug, Clone, Serialize)]
pub struct ExtensionContextContinuity {
    pub capture_policy: &'static str,
    pub shot_rule: &'static str,
    pub inputs: Vec<ExtensionContextInputIdentity>,
    pub opposite: Option<ExtensionContextInputIdentity>,
    pub support: Vec<ExtensionContextSupport>,
    pub decoded_pictures: usize,
    pub measurements: Vec<ExtensionContextMeasurement>,
}

pub(super) fn qualify_extension_context(
    session: &mut ProjectPictureSession,
    context: &ScopedHoldContext,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<ExtensionContextContinuity, String> {
    let anchor = match context.direction {
        ExtensionDirection::FromLeft => context.pictures.last(),
        ExtensionDirection::FromRight => context.pictures.first(),
    }
    .ok_or("Extension context is empty.")?
    .position;
    let opposite = match context.direction {
        ExtensionDirection::FromLeft => context.boundaries.right.as_ref(),
        ExtensionDirection::FromRight => context.boundaries.left.as_ref(),
    };
    let identities = session.context_picture_identities(
        context
            .pictures
            .iter()
            .chain(opposite)
            .map(|sample| &sample.picture),
    )?;
    let mut identities = identities.into_iter();
    let inputs = context
        .pictures
        .iter()
        .map(|sample| {
            Ok(ExtensionContextInputIdentity {
                relative_position: sample
                    .position
                    .checked_sub(anchor)
                    .map_err(|e| e.to_string())?,
                picture: identities.next().ok_or("Context identity is missing.")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let opposite = opposite
        .map(|sample| {
            Ok::<_, String>(ExtensionContextInputIdentity {
                relative_position: sample
                    .position
                    .checked_sub(anchor)
                    .map_err(|e| e.to_string())?,
                picture: identities.next().ok_or("Opposite identity is missing.")?,
            })
        })
        .transpose()?;
    let terminal = DefinitionPictureSpan {
        start: context.coverage.terminal.clone(),
        end_exclusive: context.coverage.terminal.position,
        clock: PictureClockSlope::Constant,
    };
    let spans = context
        .coverage
        .spans
        .iter()
        .chain(std::iter::once(&terminal));
    let bases =
        session.context_picture_identities(spans.clone().map(|span| &span.start.picture))?;
    let mut cache: Vec<(GenerationPictureIdentity, PictureSignature)> = Vec::new();
    let mut reads_left = MAX_CONTEXT_SHOT_SIGNATURES;
    let mut support = Vec::new();
    let mut previous: Option<(GenerationPictureIdentity, PictureSignature)> = None;
    let mut measurements = Vec::new();
    for (span, base) in spans.zip(bases) {
        check(cancelled, deadline)?;
        let (first, last, first_signature, last_signature) = match base {
            GenerationPictureIdentity::AuthoredBlack => {
                let signature = PictureSignature::from_rgba(&vec![0; 32 * 18 * 4], 32, 18, 128)
                    .map_err(|e| e.to_string())?;
                (base.clone(), base, signature.clone(), signature)
            }
            _ => {
                reads_left = reads_left
                    .checked_sub(1)
                    .ok_or("Temporal context exceeds its total picture-read budget.")?;
                let mut reader = session.context_picture_reader(
                    &span.start.picture,
                    cancelled,
                    deadline,
                    reads_left.max(1),
                )?;
                let distance = span
                    .end_exclusive
                    .checked_sub(span.start.position)
                    .map_err(|e| e.to_string())?;
                let (first, last) =
                    selected_ordinals(&span.start.picture, span.clock, distance, reader.index())?;
                let count = reader.index().frames().len();
                let requested = first.min(last)..=first.max(last);
                // Even a one-picture structural fragment needs padded blend
                // detection. Splitting a fade into tiny beats must not make
                // it disappear from the context policy.
                let window =
                    context_shot_window(requested.clone(), count).map_err(|e| e.to_string())?;
                let mut signatures = Vec::with_capacity(window.len());
                for ordinal in window.clone() {
                    check(cancelled, deadline)?;
                    let identity = at_frame(&base, SourceFrameId(ordinal as u64));
                    let signature = if let Some((_, signature)) =
                        cache.iter().find(|(key, _)| key == &identity)
                    {
                        signature.clone()
                    } else {
                        reads_left = reads_left
                            .checked_sub(1)
                            .ok_or("Temporal context exceeds its total picture-read budget.")?;
                        let frame = reader.frame(SourceFrameId(ordinal as u64))?;
                        decoded_boundary(reader.info(), SourceFrameId(ordinal as u64), &frame)?;
                        let raster = rgba(&frame)?;
                        let signature = PictureSignature::from_rgba(
                            raster.as_raw(),
                            raster.width(),
                            raster.height(),
                            raster.width() as usize * 4,
                        )
                        .map_err(|e| e.to_string())?;
                        cache.push((identity, signature.clone()));
                        signature
                    };
                    signatures.push(signature);
                }
                let result = qualify_context(&signatures, window.start, count, requested)
                    .map_err(|e| e.to_string())?;
                if result.transition.is_some() {
                    return Err("The extension context crosses a detected picture transition. Shorten or move the context to one shot.".into());
                }
                measurements.push(ExtensionContextMeasurement {
                    source: at_frame(&base, SourceFrameId(first.min(last) as u64)),
                    qualification: result,
                });
                (
                    at_frame(&base, SourceFrameId(first as u64)),
                    at_frame(&base, SourceFrameId(last as u64)),
                    signatures[first - window.start].clone(),
                    signatures[last - window.start].clone(),
                )
            }
        };
        if let Some((before, signature)) = &previous {
            check_seam_identity(before, &first)?;
            if context_seam_change(signature, &first_signature).is_some() {
                return Err("The extension context crosses an abrupt picture seam.".into());
            }
        }
        previous = Some((last.clone(), last_signature));
        support.push(ExtensionContextSupport {
            relative_start: span
                .start
                .position
                .checked_sub(anchor)
                .map_err(|e| e.to_string())?,
            relative_end_exclusive: span
                .end_exclusive
                .checked_sub(anchor)
                .map_err(|e| e.to_string())?,
            first,
            last,
        });
    }
    check(cancelled, deadline)?;
    Ok(ExtensionContextContinuity {
        capture_policy: CAPTURE_POLICY,
        shot_rule: CONTEXT_SHOT_RULE,
        inputs,
        opposite,
        support,
        decoded_pictures: MAX_CONTEXT_SHOT_SIGNATURES - reads_left,
        measurements,
    })
}

fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), String> {
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        Err("The AI extension was cancelled.".into())
    } else if Instant::now() >= deadline {
        Err("The AI extension capture deadline elapsed.".into())
    } else {
        Ok(())
    }
}

fn at_frame(
    identity: &GenerationPictureIdentity,
    frame: SourceFrameId,
) -> GenerationPictureIdentity {
    match identity {
        GenerationPictureIdentity::Original { qualification, .. } => {
            GenerationPictureIdentity::Original {
                qualification: qualification.clone(),
                frame,
            }
        }
        GenerationPictureIdentity::Generated {
            sampled_object,
            content_aspect,
            ..
        } => GenerationPictureIdentity::Generated {
            sampled_object: sampled_object.clone(),
            frame,
            content_aspect: *content_aspect,
        },
        GenerationPictureIdentity::AuthoredBlack => GenerationPictureIdentity::AuthoredBlack,
    }
}

fn check_seam_identity(
    before: &GenerationPictureIdentity,
    after: &GenerationPictureIdentity,
) -> Result<(), String> {
    let jump = match (before, after) {
        (
            GenerationPictureIdentity::Original {
                qualification: a,
                frame: af,
            },
            GenerationPictureIdentity::Original {
                qualification: b,
                frame: bf,
            },
        ) => a != b || af.0.abs_diff(bf.0) > 1,
        (
            GenerationPictureIdentity::Generated {
                sampled_object: a,
                frame: af,
                content_aspect: ac,
            },
            GenerationPictureIdentity::Generated {
                sampled_object: b,
                frame: bf,
                content_aspect: bc,
            },
        ) if a == b => ac != bc || af.0.abs_diff(bf.0) > 1,
        _ => false,
    };
    if jump {
        Err("The extension context contains a discontinuous source-picture mapping.".into())
    } else {
        Ok(())
    }
}

/// All measured ordinals touched by a canonical affine span. Its final point
/// is excluded for forward motion and included by the following span/terminal.
/// Reverse motion approaches the final timestamp from above instead.
fn selected_ordinals(
    picture: &Picture,
    clock: PictureClockSlope,
    distance: ExactRatio,
    index: &SourceFrameIndex,
) -> Result<(usize, usize), String> {
    let first = picture
        .select_source_frame(index)
        .map_err(|e| e.to_string())?
        .identity
        .0;
    if distance.compare_integer(0).is_lt() {
        return Err("Context span runs backwards in its definition.".into());
    }
    let last = match (picture, clock) {
        (_, PictureClockSlope::Constant) => first,
        (
            Picture::Source {
                point,
                selection,
                endpoints,
                ..
            },
            PictureClockSlope::SourceTicks(slope),
        ) => {
            let end = point
                .ticks
                .checked_add(distance.checked_mul(slope).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if *endpoints == EndpointPolicy::Reject
                && (end.compare(selection.start().ticks).is_lt()
                    || end.compare(selection.end().ticks).is_gt())
            {
                return Err("Context span exceeds its exact source selection.".into());
            }
            let mut endpoint = picture.clone();
            let Picture::Source {
                point, endpoints, ..
            } = &mut endpoint
            else {
                unreachable!()
            };
            point.ticks = end;
            *endpoints = EndpointPolicy::HoldAdjacent;
            let selected = endpoint
                .select_source_frame(index)
                .map_err(|e| e.to_string())?;
            if slope.compare_integer(0).is_gt()
                && distance.compare_integer(0).is_gt()
                && end.compare_integer(selected.pts).is_eq()
                && selected.identity.0 > first
            {
                selected.identity.0 - 1
            } else {
                selected.identity.0
            }
        }
        (Picture::Accepted { position, .. }, PictureClockSlope::AcceptedFrames(slope)) => {
            let end = position
                .checked_add(distance.checked_mul(slope).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let final_frame =
                if slope.compare_integer(0).is_gt() && distance.compare_integer(0).is_gt() {
                    end.ceil()
                        .map_err(|e| e.to_string())?
                        .checked_sub(1)
                        .ok_or("Context accepted endpoint overflowed.")?
                } else {
                    end.floor()
                };
            u64::try_from(final_frame).map_err(|_| "Context accepted frame is out of range.")?
        }
        _ => return Err("Context span clock disagrees with its provider.".into()),
    };
    let first = usize::try_from(first).map_err(|_| "Context first ordinal overflowed.")?;
    let last = usize::try_from(last).map_err(|_| "Context last ordinal overflowed.")?;
    if first >= index.frames().len() || last >= index.frames().len() {
        return Err("Context span exceeds its measured picture index.".into());
    }
    Ok((first, last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{
        AssetId, ExactSourceSpan, IndexedSourceFrame, SourcePoint, SourceSpan, SourceTimeBase,
        SourceTimestamp, TerminalProvenance,
    };

    #[test]
    fn measured_vfr_support_respects_direction_exact_endpoints_and_clamps() {
        let asset = AssetId::new("vfr").unwrap();
        let base = SourceTimeBase::new(1, 1000).unwrap();
        let index = SourceFrameIndex::new(
            asset.clone(),
            base,
            [10, 20, 35, 70]
                .into_iter()
                .enumerate()
                .map(|(i, pts)| IndexedSourceFrame {
                    identity: SourceFrameId(i as u64),
                    pts,
                    reported_duration: None,
                    keyframe: true,
                    seek_from: None,
                    decode_timestamp: None,
                })
                .collect(),
            100,
            TerminalProvenance::Explicit,
        )
        .unwrap();
        let stamp = |ticks| SourceTimestamp {
            ticks,
            time_base: base,
        };
        let span = SourceSpan::new(stamp(10), stamp(100)).unwrap();
        let picture = |ticks| Picture::Source {
            asset: asset.clone(),
            point: SourcePoint {
                ticks: ExactRatio::integer(ticks),
                time_base: base,
            },
            span,
            selection: ExactSourceSpan::from(span),
            endpoints: EndpointPolicy::Reject,
        };
        for (start, delta, expected) in [
            (20, 15, (1, 1)),
            (20, 16, (1, 2)),
            (35, -15, (2, 1)),
            (70, -36, (3, 1)),
            (20, 0, (1, 1)),
            (70, 30, (3, 3)),
        ] {
            assert_eq!(
                selected_ordinals(
                    &picture(start),
                    PictureClockSlope::SourceTicks(ExactRatio::integer(delta)),
                    ExactRatio::integer(1),
                    &index
                )
                .unwrap(),
                expected
            );
        }
        assert!(
            selected_ordinals(
                &picture(70),
                PictureClockSlope::SourceTicks(ExactRatio::integer(31)),
                ExactRatio::integer(1),
                &index
            )
            .is_err()
        );
        let mut held = picture(70);
        if let Picture::Source { endpoints, .. } = &mut held {
            *endpoints = EndpointPolicy::HoldAdjacent;
        }
        assert_eq!(
            selected_ordinals(
                &held,
                PictureClockSlope::SourceTicks(ExactRatio::integer(300)),
                ExactRatio::integer(1),
                &index
            )
            .unwrap(),
            (3, 3)
        );
    }
}
