//! Retained project-root audibility for an integer-sample resume. This runs
//! after PCM lookup and before later effects, alongside current Hold suppression.
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use deadpan_core::AudioSample;
use deadpan_plan::{
    AudioQueryLimits, AudioReferencePlan, PlanError, ReferenceAudioContent, ReferenceSample,
    SilenceReason,
};

use crate::{MAX_OUTPUT_FRAMES, PreparationError, check_cancel};

#[derive(Debug, thiserror::Error)]
pub enum ReferencePolicyError {
    #[error("retained policy requires a valid bounded output range and silence intervals")]
    Range,
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Preparation(#[from] PreparationError),
}

/// A concrete root-output resume: one new output sample advances one sample in
/// the retained root policy clock. It owns the admitted frozen timing plan,
/// independently of live sibling IDs. It does not resample audio, apply fades,
/// bind an authored edit, or convert this clock into a Preserve input grid.
#[derive(Clone)]
pub struct RetainedRootPolicy {
    reference: Arc<AudioReferencePlan>,
    reference_anchor: AudioSample,
    output_anchor: AudioSample,
    output: Range<AudioSample>,
}

impl RetainedRootPolicy {
    pub fn new(
        reference: Arc<AudioReferencePlan>,
        reference_anchor: AudioSample,
        output_anchor: AudioSample,
        output: Range<AudioSample>,
    ) -> Result<Self, ReferencePolicyError> {
        let reference_end = reference.root_clock().sample_count()?.0;
        if output.start.0 < 0
            || output.start >= output.end
            || reference_anchor.0 < 0
            || reference_anchor.0 > reference_end
        {
            return Err(ReferencePolicyError::Range);
        }
        Ok(Self {
            reference,
            reference_anchor,
            output_anchor,
            output,
        })
    }

    /// Zero retained silent Holds and current explicit silence, returning their
    /// merged output ranges for downstream policy handling. All validation and
    /// reference queries finish before modifying PCM. Current intervals may
    /// extend beyond this query; only their intersection is applied.
    pub fn apply(
        &self,
        start: AudioSample,
        samples: &mut [[f32; 2]],
        current_silence: &[Range<AudioSample>],
        limits: AudioQueryLimits,
        cancelled: &AtomicBool,
    ) -> Result<Vec<Range<AudioSample>>, ReferencePolicyError> {
        check_cancel(cancelled)?;
        limits.validate()?;
        let end = start
            .0
            .checked_add(i64::try_from(samples.len()).map_err(|_| ReferencePolicyError::Range)?)
            .map(AudioSample)
            .ok_or(ReferencePolicyError::Range)?;
        if samples.is_empty()
            || samples.len() > MAX_OUTPUT_FRAMES as usize
            || start < self.output.start
            || end > self.output.end
            || current_silence.len() > MAX_OUTPUT_FRAMES as usize
            || current_silence
                .iter()
                .any(|range| range.start.0 < 0 || range.start > range.end)
        {
            return Err(ReferencePolicyError::Range);
        }
        let clock = self.reference.root_clock();
        let reference_end = i128::from(clock.sample_count()?.0);
        let shift = i128::from(self.reference_anchor.0) - i128::from(self.output_anchor.0);
        let old_start = i128::from(start.0) + shift;
        let old_end = i128::from(end.0) + shift;
        let mut muted = Vec::new();
        // Out-of-domain samples are silence; wide arithmetic lets a completely
        // out-of-range demand be handled without narrowing an impossible index.
        if old_start < 0 {
            let count = (-old_start)
                .min(i128::try_from(samples.len()).map_err(|_| ReferencePolicyError::Range)?);
            muted.push(
                start
                    ..AudioSample(
                        start.0 + i64::try_from(count).map_err(|_| ReferencePolicyError::Range)?,
                    ),
            );
        }
        if old_end > reference_end {
            let count = (old_end - reference_end)
                .min(i128::try_from(samples.len()).map_err(|_| ReferencePolicyError::Range)?);
            muted.push(
                AudioSample(end.0 - i64::try_from(count).map_err(|_| ReferencePolicyError::Range)?)
                    ..end,
            );
        }
        let retained_start = old_start.max(0);
        let retained_end = old_end.min(reference_end);
        if retained_start < retained_end {
            let query = clock.query(
                ReferenceSample(
                    i64::try_from(retained_start).map_err(|_| ReferencePolicyError::Range)?,
                )
                    ..ReferenceSample(
                        i64::try_from(retained_end).map_err(|_| ReferencePolicyError::Range)?,
                    ),
                limits,
            )?;
            for span in query.spans {
                // Missing input is not an instruction to suppress processed
                // decay. Match the existing root and Preserve policy passes.
                if matches!(
                    span.content,
                    ReferenceAudioContent::Silence {
                        reason: SilenceReason::SilentHold
                    }
                ) {
                    let left = i64::try_from(i128::from(span.samples.start.0) - shift)
                        .map_err(|_| ReferencePolicyError::Range)?;
                    let right = i64::try_from(i128::from(span.samples.end.0) - shift)
                        .map_err(|_| ReferencePolicyError::Range)?;
                    muted.push(AudioSample(left)..AudioSample(right));
                }
            }
        }
        for range in current_silence {
            let left = range.start.max(start);
            let right = range.end.min(end);
            if left < right {
                muted.push(left..right);
            }
        }
        muted.sort_unstable_by_key(|range| range.start);
        let mut merged: Vec<Range<AudioSample>> = Vec::new();
        for range in muted {
            if let Some(last) = merged.last_mut().filter(|last| range.start <= last.end) {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        let mut offsets = Vec::with_capacity(merged.len());
        for range in &merged {
            let left = usize::try_from(range.start.0 - start.0)
                .map_err(|_| ReferencePolicyError::Range)?;
            let right =
                usize::try_from(range.end.0 - start.0).map_err(|_| ReferencePolicyError::Range)?;
            if left > right || right > samples.len() {
                return Err(ReferencePolicyError::Range);
            }
            offsets.push(left..right);
        }
        check_cancel(cancelled)?;
        for range in offsets {
            samples[range].fill([0.0; 2]);
        }
        Ok(merged)
    }
}
