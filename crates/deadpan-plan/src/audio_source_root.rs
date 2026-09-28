//! Complete raw source recipes evaluated directly on a retained RoundEven grid.

use std::ops::Range;

use deadpan_core::{AudioSample, ExactRatio, FrameDuration, MIX_SAMPLE_RATE, SourceAudio};

use crate::{
    AudioBoundaryRule, AudioProcessingQuery, AudioProcessingSpan, AudioQueryLimits,
    AudioSampleGrid, AudioSampleMap, AudioSignal, AudioSignalTape, AudioSignalTapeRun,
    AudioSourceVoiceRecipe, AudioTransform, PlanError, RenderPlan, SignalSample,
};

/// A complete raw root capture. Its source phase, selected filter support and
/// original owner extent survive later changes to the live root duration.
/// Current Hold gates and creative envelopes belong to the consuming sound.
#[derive(Debug, Clone)]
pub struct AudioRootSource<'plan> {
    plan: &'plan RenderPlan,
    source: SourceAudio,
    extent: ExactRatio,
    grid: AudioSampleGrid<AudioSample>,
    input: AudioSignalTape<'plan>,
}

impl<'plan> AudioRootSource<'plan> {
    pub fn new(
        plan: &'plan RenderPlan,
        recipe: AudioSourceVoiceRecipe,
        extent: ExactRatio,
        grid: AudioSampleGrid<AudioSample>,
    ) -> Result<Self, PlanError> {
        let rate = plan.metadata().presentation_basis.frame_rate;
        let step = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        if grid.frame_origin() != ExactRatio::ZERO
            || grid.frames_per_sample() != step
            || grid.boundary_rule() != AudioBoundaryRule::RoundEven
        {
            return Err(PlanError::InvalidPlan(
                "raw root capture requires its absolute project RoundEven grid",
            ));
        }
        let selected = recipe.mapping.selection_frames_with_offset(
            FrameDuration::ZERO,
            recipe.offset,
            rate,
        )?;
        if selected.start.compare_integer(0).is_lt()
            || !selected
                .end
                .checked_sub(selected.start)?
                .compare_integer(0)
                .is_gt()
            || selected.end.checked_sub(extent)?.compare_integer(0).is_gt()
        {
            return Err(PlanError::InvalidPlan(
                "raw root selection exceeds its original owner extent",
            ));
        }
        let source = recipe.source.clone();
        let voice = AudioSignal::source_voice_capture(plan, recipe, extent)?;
        let full = ExactRatio::ZERO..extent;
        let input = AudioSignalTape::new(
            plan,
            full.clone(),
            vec![AudioSignalTapeRun::new(
                full.clone(),
                full.clone(),
                voice.input_signal(),
            )],
        )?
        .remap_policy_window(
            full.clone(),
            full,
            ExactRatio::ZERO,
            step,
            AudioBoundaryRule::RoundEven,
        )?;
        // Check the complete allocation before admitting the capture.
        grid.boundary(extent)?;
        Ok(Self {
            plan,
            source,
            extent,
            grid,
            input,
        })
    }

    pub fn source(&self) -> &SourceAudio {
        &self.source
    }
    pub fn extent(&self) -> ExactRatio {
        self.extent
    }
    pub fn samples(&self) -> Result<Range<AudioSample>, PlanError> {
        Ok(self.grid.boundary(ExactRatio::ZERO)?..self.grid.boundary(self.extent)?)
    }
    pub fn grid(&self) -> AudioSampleGrid<AudioSample> {
        self.grid
    }
    pub fn plan(&self) -> &'plan RenderPlan {
        self.plan
    }
    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        std::ptr::eq(self.plan, plan)
    }

    /// Evaluate source maps on RoundEven before producing any PCM. Rebranding
    /// this private carrier does not relabel a previously sampled PointCeil array.
    pub fn query(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioProcessingQuery<'plan>, PlanError> {
        let query = self.input.query(
            SignalSample(samples.start.0)..SignalSample(samples.end.0),
            limits,
        )?;
        let spans = query
            .spans
            .into_iter()
            .map(|span| {
                Ok(AudioProcessingSpan {
                    definition: span.definition,
                    samples: AudioSample(span.samples.start.0)..AudioSample(span.samples.end.0),
                    allocated_samples: AudioSample(span.allocated_samples.start.0)
                        ..AudioSample(span.allocated_samples.end.0),
                    project_extent: span.signal_extent,
                    instance: span.instance,
                    gap_after: span.gap_after,
                    transform: AudioTransform {
                        project_origin: span.transform.signal_origin,
                        project_frames_per_local_frame: span
                            .transform
                            .signal_frames_per_local_frame,
                        project_frames_per_sample: span.transform.signal_frames_per_sample,
                    },
                    grid: self.grid,
                    sampling: AudioSampleMap::new(
                        AudioSample(span.sampling.anchor().0),
                        span.sampling.local_at_anchor(),
                        span.sampling.local_frames_per_sample(),
                    )?,
                    retimes: span.retimes,
                    content: span.content,
                })
            })
            .collect::<Result<Vec<_>, PlanError>>()?;
        Ok(AudioProcessingQuery {
            project_id: query.project_id,
            revision_id: query.revision_id,
            samples,
            spans,
            lookup: query.lookup,
            work: query.work,
        })
    }
}
