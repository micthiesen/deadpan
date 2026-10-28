//! Final raw Original/occurrence policy on its already selected output grid.
//! The borrowed visitor neither prepares PCM nor constructs a provider graph.

use super::*;
use deadpan_plan::AudioSpan;

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
        let offsets = offsets(start, frames, &span.samples, false)?;
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
    // apply_suppression validated all offsets before writing any policy zero.
    // Two bounded borrowed walks keep that boundary without its temporary Vec.
    for range in &queries.policy.suppressed {
        charge()?;
        offsets(start, frames, range, true)?;
    }
    for range in &queries.policy.suppressed {
        charge()?;
        let offsets = offsets(start, frames, range, true)?;
        emit(Event::Suppressed {
            range: range.clone(),
            offsets,
        })?;
    }
    Ok(())
}

fn offsets(
    start: AudioSample,
    frames: usize,
    range: &Range<AudioSample>,
    nonempty: bool,
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
    if left > right || (nonempty && left == right) || right > frames {
        return Err(StageAudioError::Range);
    }
    Ok(left..right)
}

/// The real final policy consumer. PCM remains at its original operation
/// boundaries; only the decisions above are shared with metadata consumers.
pub(super) struct Applied {
    pub(super) suppressed: Vec<Range<AudioSample>>,
    pub(super) exhausted: Vec<Range<AudioSample>>,
}

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

#[cfg(test)]
#[path = "raw_policy/tests.rs"]
mod tests;
