//! Final raw Original/occurrence policy on its already selected output grid.
//! The borrowed visitor neither prepares PCM nor constructs a provider graph.

use super::*;
use deadpan_plan::AudioSpan;
use point_suppression::ops::{MaskContext, MaskWorkPhase, RangeBuilder, begin_ranges};

/// Ordered operations at the final policy boundary. A metadata consumer can
/// record masks without evaluating the retained-envelope numeric operation.
/// Such a successful visit assumes that independent numeric validation passes;
/// it does not prove PCM validity or permit source access.
pub(super) enum Event<'query> {
    SilentHold {
        range: Range<AudioSample>,
        offsets: Range<usize>,
    },
    RetainedEnvelope {
        span: &'query AudioSpan,
        offsets: Range<usize>,
    },
    Exhausted(Range<AudioSample>),
    Suppressed {
        range: Range<AudioSample>,
        offsets: Range<usize>,
    },
}

/// Visit the exact final mask/envelope decisions used by read_queries_borrowed.
/// No processing-child masks or creative-fade values are folded into this mask.
///
/// The production read supplies a no-op charge hook because it replaces the
/// same existing visits once, retaining their WorkControl::check positions.
/// A cold metadata caller must bill additional work through `charge` before
/// each validation/emission. Incomplete/errors retain already spent work. Its
/// scratch/callback storage requires its own admission; this adapter allocates
/// none and retains no plan/identity handle.
///
/// Every explicit policy range is validated before the first Suppressed event,
/// preserving the read's all-or-nothing policy-zeroing boundary. Earlier
/// flattened envelope operations precede that validation as they did before.
pub(super) fn visit<'query>(
    queries: RootReadQueriesRef<'query, '_>,
    start: AudioSample,
    frames: usize,
    control: WorkControl<'_>,
    mut charge: impl FnMut() -> Result<(), StageAudioError>,
    mut emit: impl FnMut(Event<'query>) -> Result<(), StageAudioError>,
) -> Result<(), StageAudioError> {
    for span in &queries.flattened.spans {
        control.check()?;
        charge()?;
        let offsets = span_offsets(start, frames, &span.samples)?;
        if is_silent_hold(&span.content) {
            charge()?;
            emit(Event::SilentHold {
                range: span.samples.clone(),
                offsets,
            })?;
        } else {
            charge()?;
            emit(Event::RetainedEnvelope { span, offsets })?;
            // The numeric read applies its retained envelope at the preceding
            // event before evaluating exhaustion, preserving error order.
            let slots = crate::edges::exhausted_range_slots(span)?;
            for range in slots.into_iter().flatten() {
                charge()?;
                emit(Event::Exhausted(range))?;
            }
        }
    }
    // Share whole-list validation and exact typed offset conversion with the
    // signal reader and cold input adapters, before any policy zeroing.
    policy_ranges::visit(
        start,
        frames,
        &queries.policy.suppressed,
        |sample| sample.0,
        &mut charge,
        |range, offsets| {
            emit(Event::Suppressed {
                range: range.clone(),
                offsets,
            })
        },
    )?;
    Ok(())
}

fn span_offsets(
    start: AudioSample,
    frames: usize,
    range: &Range<AudioSample>,
) -> Result<Range<usize>, StageAudioError> {
    let left = range
        .start
        .0
        .checked_sub(start.0)
        .and_then(|offset| usize::try_from(offset).ok())
        .ok_or(StageAudioError::Range)?;
    let right = range
        .end
        .0
        .checked_sub(start.0)
        .and_then(|offset| usize::try_from(offset).ok())
        .ok_or(StageAudioError::Range)?;
    if left > right || right > frames {
        return Err(StageAudioError::Range);
    }
    Ok(left..right)
}

/// Literal pre-owned consumer retained for exact compatibility observations.
#[cfg(test)]
pub(super) struct Applied {
    pub(super) suppressed: Vec<Range<AudioSample>>,
    pub(super) exhausted: Vec<Range<AudioSample>>,
}

#[cfg(test)]
pub(super) fn apply(
    queries: RootReadQueriesRef<'_, '_>,
    start: AudioSample,
    samples: &mut [[f32; 2]],
    control: WorkControl<'_>,
) -> Result<Applied, StageAudioError> {
    let mut suppressed = Vec::new();
    let mut exhausted = Vec::new();
    visit(
        queries,
        start,
        samples.len(),
        control,
        || Ok(()),
        |event| {
            match event {
                Event::SilentHold { range, offsets } => {
                    samples[offsets].fill([0.0; 2]);
                    suppressed.push(range);
                }
                Event::RetainedEnvelope { span, offsets } => {
                    crate::edges::apply_retained_envelope(span, &mut samples[offsets], false)?;
                }
                Event::Exhausted(range) => exhausted.push(range),
                Event::Suppressed { range, offsets } => {
                    samples[offsets].fill([0.0; 2]);
                    suppressed.push(range);
                }
            }
            Ok(())
        },
    )?;
    Ok(Applied {
        suppressed,
        exhausted,
    })
}

pub(super) struct PendingApplied {
    pub(super) suppressed: PendingSuppression,
    pub(super) exhausted: Vec<Range<AudioSample>>,
}

pub(super) enum PendingSuppression {
    #[cfg(test)]
    Legacy(Vec<Range<AudioSample>>),
    Owned(Option<RangeBuilder<AudioSample>>),
}

impl PendingSuppression {
    pub(super) fn finish<C: MaskContext>(
        self,
        context: &C,
    ) -> Result<AudioSuppressionMask, StageAudioError> {
        match self {
            #[cfg(test)]
            Self::Legacy(ranges) => Ok(AudioSuppressionMask::plain(merged_suppression(ranges))),
            Self::Owned(None) => Ok(AudioSuppressionMask::default()),
            Self::Owned(Some(builder)) => builder.finish(context)?.merge(context),
        }
    }
}

// Count emissions of a successful visit, including empty SilentHold spans and
// duplicates. Do not move range/envelope validation ahead of its actual event.
fn suppression_capacity<C: MaskContext>(
    queries: RootReadQueriesRef<'_, '_>,
    context: &C,
) -> Result<usize, StageAudioError> {
    let mut capacity = queries.policy.suppressed.len();
    for span in &queries.flattened.spans {
        context.check()?;
        context.spend(MaskWorkPhase::Scan, 1)?;
        if is_silent_hold(&span.content) {
            capacity = capacity.checked_add(1).ok_or(StageAudioError::Range)?;
        }
    }
    Ok(capacity)
}

pub(super) fn apply_owned<C: MaskContext>(
    queries: RootReadQueriesRef<'_, '_>,
    start: AudioSample,
    samples: &mut [[f32; 2]],
    control: WorkControl<'_>,
    context: &mut C,
) -> Result<PendingApplied, StageAudioError> {
    #[cfg(test)]
    if super::root_mask_request::legacy() {
        let Applied {
            suppressed,
            exhausted,
        } = apply(queries, start, samples, control)?;
        return Ok(PendingApplied {
            suppressed: PendingSuppression::Legacy(suppressed),
            exhausted,
        });
    }
    let capacity = suppression_capacity(queries, context)?;
    let mut suppressed = None;
    let mut exhausted = Vec::new();
    visit(
        queries,
        start,
        samples.len(),
        control,
        || Ok(()),
        |event| {
            match event {
                Event::SilentHold { range, offsets } | Event::Suppressed { range, offsets } => {
                    samples[offsets].fill([0.0; 2]);
                    if suppressed.is_none() {
                        suppressed = Some(begin_ranges::<AudioSample, C>(context, capacity)?);
                    }
                    suppressed
                        .as_mut()
                        .expect("suppression event has funded destination")
                        .push(range, context)?;
                }
                Event::RetainedEnvelope { span, offsets } => {
                    crate::edges::apply_retained_envelope(span, &mut samples[offsets], false)?;
                }
                Event::Exhausted(range) => exhausted.push(range),
            }
            Ok(())
        },
    )?;
    Ok(PendingApplied {
        suppressed: PendingSuppression::Owned(suppressed),
        exhausted,
    })
}

#[cfg(test)]
#[path = "raw_policy/tests.rs"]
mod tests;
