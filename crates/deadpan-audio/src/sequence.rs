//! Plan-driven source PCM, before voice effects and the master bus. Unsupported
//! authored processing fails explicitly; it is never replaced with raw speech.
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{
    AssetId, AudioSample, ExactRatio, NodeId, PitchPolicy, ProjectId, RevisionId,
    SourceAudioMapping, SourcePoint, SourceTimestamp, TimeError,
};
use deadpan_plan::{AudioContent, AudioQueryLimits, AudioSpan, PlanError, RenderPlan};
use serde::Serialize;

use crate::{MAX_OUTPUT_FRAMES, PreparationError, PreparedSource, ResampleRecipe, check_cancel};

/// The host resolves an asset in the exact immutable revision being read. It
/// must verify the revision's complete qualification receipt and original byte
/// identity before returning a PreparedSource. Alias-only/latest-head caches
/// violate this contract. The audio crate owns no database or authored state.
pub trait AudioSourceProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SequenceAudioError {
    #[error("source PCM inspection requires 1..256 samples inside the sequence")]
    Range,
    #[error("source PCM stage cannot render {feature} at node {node}")]
    Unsupported { node: NodeId, feature: &'static str },
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Preparation(#[from] PreparationError),
    #[error(transparent)]
    Time(#[from] TimeError),
}

/// This is not the final mix: no edge fade, authored effect, master limiter or
/// monitoring gain has been applied. Consumers must retain the stage identity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceStageBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
}

pub struct SequenceAudio {
    plan: Arc<RenderPlan>,
}

impl SequenceAudio {
    pub fn new(plan: Arc<RenderPlan>) -> Self {
        Self { plan }
    }

    pub fn plan(&self) -> &RenderPlan {
        &self.plan
    }

    /// Stage a complete source block atomically. Every leaf keeps its full
    /// allocated interval as the phase origin, independent of this query's
    /// partition. File reads and filtering belong on preparation workers.
    pub fn read_sources(
        &self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<SourceStageBlock, SequenceAudioError> {
        check_cancel(cancelled)?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(SequenceAudioError::Range)?;
        if frames == 0
            || frames > MAX_OUTPUT_FRAMES
            || start.0 < 0
            || end > self.plan.audio_duration()?.0
        {
            return Err(SequenceAudioError::Range);
        }
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(PreparationError::InvalidRecipe("audio read time budget").into());
        }
        let query = self.plan.audio(
            start..AudioSample(end),
            AudioQueryLimits {
                maximum_spans: MAX_OUTPUT_FRAMES as usize,
                ..Default::default()
            },
        )?;
        // Reject unsupported authored processing anywhere in this query before
        // calling a provider or publishing any samples from an earlier leaf.
        for span in &query.spans {
            check_cancel(cancelled)?;
            self.preflight(span)?;
        }
        let metadata = self.plan.metadata();
        let mut samples = Vec::with_capacity(frames as usize);
        for span in query.spans {
            check_cancel(cancelled)?;
            let count = u32::try_from(span.samples.end.0 - span.samples.start.0)
                .map_err(|_| SequenceAudioError::Range)?;
            if let AudioContent::Source { source, .. } = &span.content {
                let prepared = provider.source(
                    &metadata.project_id,
                    &metadata.revision_id,
                    &source.asset,
                    cancelled,
                )?;
                if let Some(recipe) = source_recipe(&span, prepared.index().stream().sample_rate)? {
                    samples.extend(
                        prepared
                            .prepare(recipe, span.samples.start, count, timeout, cancelled)?
                            .samples,
                    );
                } else {
                    // The exact selected interval contains no original sample
                    // positions. All filter taps are outside the authored trim.
                    samples.resize(samples.len() + count as usize, [0.0; 2]);
                }
            } else {
                // Preflight admitted only explicit plan silence here.
                samples.resize(samples.len() + count as usize, [0.0; 2]);
            }
        }
        check_cancel(cancelled)?;
        Ok(SourceStageBlock {
            schema_version: 1,
            stage: "source_pcm_before_effects",
            project_id: metadata.project_id.clone(),
            revision_id: metadata.revision_id.clone(),
            start,
            samples,
        })
    }

    fn preflight(&self, span: &AudioSpan) -> Result<(), SequenceAudioError> {
        let unsupported = |feature| SequenceAudioError::Unsupported {
            node: span.instance.node.clone(),
            feature,
        };
        match &span.content {
            AudioContent::Silence { .. } => Ok(()),
            AudioContent::RoomTone { .. } => Err(unsupported("room tone")),
            AudioContent::Tail { .. } => Err(unsupported("effect tails")),
            AudioContent::Source {
                source, duration, ..
            } => {
                if let Some(stage) = span.retimes.iter().find(|stage| {
                    stage.pitch == PitchPolicy::Preserve
                        && stage.child_frames_per_local_frame != ExactRatio::ONE
                }) {
                    return Err(SequenceAudioError::Unsupported {
                        node: stage.node.clone(),
                        feature: "pitch-preserving retime",
                    });
                }
                // Source placement describes time, but has no independent pitch
                // policy. Do not invent tape-speed semantics for an implicit
                // rate change. Explicit FollowSpeed Retime stages are admitted.
                let natural = SourceAudioMapping::natural_rate(
                    source.span,
                    self.plan.metadata().presentation_basis.frame_rate,
                )?
                .duration_frames(self.plan.metadata().duration)?;
                if natural != *duration {
                    return Err(unsupported("source rate mapping without a pitch policy"));
                }
                Ok(())
            }
        }
    }
}

fn source_recipe(
    span: &AudioSpan,
    sample_rate: u32,
) -> Result<Option<ResampleRecipe>, SequenceAudioError> {
    let AudioContent::Source { source, .. } = &span.content else {
        return Err(PlanError::NoSourceAudio.into());
    };
    let original_start = original_sample(source.span.start(), sample_rate)?;
    let original_end = original_sample(source.span.end(), sample_rate)?;
    // A structural crop can start/end between original samples. Keep only
    // sample positions inside that exact half-open interval, before filter I/O.
    let clipped_start = source_samples(
        span.source_point_at_project_frame(span.project_extent.start)?,
        sample_rate,
    )?
    .ceil()?;
    let clipped_end = source_samples(
        span.source_point_at_project_frame(span.project_extent.end)?,
        sample_rate,
    )?
    .ceil()?;
    let left = clipped_start.max(i128::from(original_start));
    let right = clipped_end.min(i128::from(original_end));
    if left >= right {
        return Ok(None);
    }
    let origin = span.allocated_samples.start;
    let source_origin = source_samples(span.source_point(origin)?, sample_rate)?;
    let source_step = source_samples(
        span.source_point(AudioSample(
            origin.0.checked_add(1).ok_or(TimeError::Overflow)?,
        ))?,
        sample_rate,
    )?
    .checked_sub(source_origin)?;
    Ok(Some(ResampleRecipe::new(
        i64::try_from(left).map_err(|_| TimeError::Overflow)?
            ..i64::try_from(right).map_err(|_| TimeError::Overflow)?,
        source_origin,
        origin,
        source_step,
        span.allocated_samples.clone(),
    )?))
}

pub(crate) fn source_samples(point: SourcePoint, rate: u32) -> Result<ExactRatio, TimeError> {
    point.ticks.checked_mul(ExactRatio::new(
        i128::from(point.time_base.numerator()) * i128::from(rate),
        i128::from(point.time_base.denominator()),
    )?)
}

pub(crate) fn original_sample(point: SourceTimestamp, rate: u32) -> Result<i64, PreparationError> {
    let samples = source_samples(
        SourcePoint {
            ticks: ExactRatio::integer(point.ticks),
            time_base: point.time_base,
        },
        rate,
    )?;
    if samples.denominator() != 1 {
        return Err(PreparationError::InvalidRecipe(
            "source trim is not on original sample boundaries",
        ));
    }
    Ok(i64::try_from(samples.numerator()).map_err(|_| TimeError::Overflow)?)
}
