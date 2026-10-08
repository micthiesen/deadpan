//! Exact structural coverage plus bounded visual transition rejection.
//!
//! The shot rule is a rejection heuristic, never a guarantee of scene identity.
//! Model inputs and support are relative to the anchor so editorial group moves
//! do not change their identity. This report does not itself grant job relevance
//! or accepted-media authority; those paths must independently recapture it.

use std::time::Instant;

use deadpan_analysis::{
    CONTEXT_SHOT_RULE, MAX_CONTEXT_SHOT_SIGNATURES, PictureSignature, context_seam_change,
    context_shot_window, encode_context_signatures, qualify_context,
};
pub use deadpan_models::ExtensionContextMeasurement;
use deadpan_models::ExtensionContinuityEvidence;
use deadpan_plan::{DefinitionPictureSpan, PictureClockSlope, ScopedHoldContext};
use deadpan_store::generation_inputs::{GenerationInputBinding, GenerationInputs};
use deadpan_store::generation_pictures::GenerationPictureIdentity;
use serde::Serialize;

use super::*;

const CAPTURE_POLICY: &str = "deadpan-extension-context-1";

/// Host observations for this immutable capture. The manifest retains their
/// descriptor and signatures so the policy can be repeated independently.
#[derive(Debug, Clone, Serialize)]
pub struct ExtensionContextContinuity {
    pub capture_policy: &'static str,
    pub shot_rule: &'static str,
    pub binding: GenerationInputBinding,
    pub decoded_pictures: usize,
    pub measurements: Vec<ExtensionContextMeasurement>,
}

pub(super) struct CapturedExtensionContinuity {
    pub report: ExtensionContextContinuity,
    pub evidence: ExtensionContinuityEvidence,
    pub signatures: Vec<u8>,
}

pub(super) fn qualify_extension_context(
    session: &mut ProjectPictureSession,
    context: &ScopedHoldContext,
    binding: GenerationInputBinding,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<CapturedExtensionContinuity, String> {
    let GenerationInputs::Extension {
        support,
        terminal: terminal_identity,
        ..
    } = &binding.inputs
    else {
        return Err("Temporal qualification requires an extension input binding.".into());
    };
    if support.len() != context.coverage.spans.len() {
        return Err("Temporal support differs from its captured input binding.".into());
    }
    let terminal = DefinitionPictureSpan::observation(
        context.coverage.terminal.clone(),
        context.coverage.terminal.position,
        PictureClockSlope::Constant,
    );
    let spans = context
        .coverage
        .spans
        .iter()
        .chain(std::iter::once(&terminal));
    let bases = support
        .iter()
        .map(|span| (&span.first, &span.last))
        .chain(std::iter::once((
            &terminal_identity.picture,
            &terminal_identity.picture,
        )));
    let mut cache: Vec<(GenerationPictureIdentity, PictureSignature)> = Vec::new();
    let mut reads_left = MAX_CONTEXT_SHOT_SIGNATURES;
    let mut previous: Option<(GenerationPictureIdentity, PictureSignature)> = None;
    let mut measurements = Vec::new();
    let mut source_picture_counts = Vec::new();
    for (span, (expected_first, expected_last)) in spans.zip(bases) {
        check(cancelled, deadline)?;
        let base = expected_first.clone();
        let (first, last, first_signature, last_signature) = match base {
            GenerationPictureIdentity::AuthoredBlack => {
                source_picture_counts.push(None);
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
                let (first, last) = span
                    .source_ordinals(reader.index())
                    .map_err(|e| e.to_string())?;
                let first =
                    usize::try_from(first.0).map_err(|_| "Context first ordinal overflowed.")?;
                let last =
                    usize::try_from(last.0).map_err(|_| "Context last ordinal overflowed.")?;
                let count = reader.index().frames().len();
                source_picture_counts.push(Some(count));
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
        if &first != expected_first || &last != expected_last {
            return Err("Decoded temporal support differs from its retained input binding.".into());
        }
        if let Some((before, signature)) = &previous {
            check_seam_identity(before, &first)?;
            if context_seam_change(signature, &first_signature).is_some() {
                return Err("The extension context crosses an abrupt picture seam.".into());
            }
        }
        previous = Some((last.clone(), last_signature));
    }
    check(cancelled, deadline)?;
    let signatures = encode_context_signatures(
        &cache
            .iter()
            .map(|(_, signature)| signature.clone())
            .collect::<Vec<_>>(),
    )
    .map_err(|error| error.to_string())?;
    let evidence = ExtensionContinuityEvidence::new(
        binding.clone(),
        cache.into_iter().map(|(identity, _)| identity).collect(),
        source_picture_counts,
        super::extension::artifact("inputs/continuity.bin", &signatures)?,
    )
    .map_err(|error| error.to_string())?;
    let recomputed = evidence
        .qualify_signatures(&signatures, cancelled, deadline)
        .map_err(|error| error.to_string())?;
    if recomputed != measurements {
        return Err("Retained continuity evidence differs from the live measurements.".into());
    }
    Ok(CapturedExtensionContinuity {
        evidence,
        signatures,
        report: ExtensionContextContinuity {
            capture_policy: CAPTURE_POLICY,
            shot_rule: CONTEXT_SHOT_RULE,
            binding,
            decoded_pictures: MAX_CONTEXT_SHOT_SIGNATURES - reads_left,
            measurements,
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{
        AssetId, EndpointPolicy, ExactSourceSpan, IndexedSourceFrame, SourceFrameIndex,
        SourcePoint, SourceSpan, SourceTimeBase, SourceTimestamp, TerminalProvenance,
    };
    use deadpan_plan::Picture;

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
        // Exercise the same shared plan query as the decoder-backed qualifier.
        let selected_ordinals =
            |picture: &Picture, clock: PictureClockSlope, distance, index: &SourceFrameIndex| {
                clock
                    .source_ordinals(picture, distance, index)
                    .map(|(first, last)| (first.0, last.0))
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
