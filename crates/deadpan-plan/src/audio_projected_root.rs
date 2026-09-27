//! A live intrinsic Preserve output placed on the absolute project sample grid.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use deadpan_core::{AudioSample, ExactRatio, MIX_SAMPLE_RATE};
use serde::{Serialize, Serializer, ser::SerializeStruct};

use crate::{
    AudioBoundaryRule, AudioPolicyQuery, AudioQueryLimits, AudioRootPlacement, AudioSampleGrid,
    AudioSampleMap, AudioSignalTape, AudioStageProjection, PlanError, RenderPlan, SignalSample,
};

/// An intrinsic Preserve output placed on an absolute RoundEven project grid.
///
/// The projected PCM and policy retain their complete original context across
/// crops and resumes. A resume changes the output allocation and continues the
/// old exact map at `old_cut`; it does not restart the projection's phase.
#[derive(Clone)]
pub struct AudioProjectedRoot<'plan> {
    projection: Arc<AudioStageProjection<'plan>>,
    placement: AudioRootPlacement,
    extent: Range<ExactRatio>,
    meaningful_extent: Range<ExactRatio>,
    samples: Range<AudioSample>,
    meaningful_samples: Range<AudioSample>,
    sampling: AudioSampleMap<AudioSample>,
    /// Absolute root sample label represented by `sampling.anchor()` in the
    /// original, unresumed policy context. Uses i128 for checked composition.
    reference_sample_at_anchor: i128,
    root_grid: AudioSampleGrid<AudioSample>,
    /// Full-support output policy regridded directly onto the absolute root
    /// RoundEven grid. Shared by crop/resume handles without tree expansion.
    policy: Arc<AudioSignalTape<'plan>>,
}

impl fmt::Debug for AudioProjectedRoot<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioProjectedRoot")
            .field("projection", &self.projection)
            .field("placement", &self.placement)
            .field("extent", &self.extent)
            .field("meaningful_extent", &self.meaningful_extent)
            .field("samples", &self.samples)
            .field("sampling", &self.sampling)
            .finish()
    }
}

impl PartialEq for AudioProjectedRoot<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.projection == other.projection
            && self.placement == other.placement
            && self.extent == other.extent
            && self.meaningful_extent == other.meaningful_extent
            && self.samples == other.samples
            && self.meaningful_samples == other.meaningful_samples
            && self.sampling == other.sampling
            && self.reference_sample_at_anchor == other.reference_sample_at_anchor
    }
}

impl Eq for AudioProjectedRoot<'_> {}

impl Serialize for AudioProjectedRoot<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AudioProjectedRoot", 7)?;
        // AudioStageProjection's serializer deliberately emits only its stable
        // stage descriptor and duration, never the borrowed policy/input DAG.
        state.serialize_field("projection", self.projection.as_ref())?;
        state.serialize_field("placement", &self.placement)?;
        state.serialize_field("extent", &self.extent)?;
        state.serialize_field("meaningful_extent", &self.meaningful_extent)?;
        state.serialize_field("samples", &self.samples)?;
        state.serialize_field("meaningful_samples", &self.meaningful_samples)?;
        state.serialize_field("sampling", &self.sampling)?;
        state.end()
    }
}

impl<'plan> AudioProjectedRoot<'plan> {
    pub fn new(
        projection: Arc<AudioStageProjection<'plan>>,
        placement: AudioRootPlacement,
    ) -> Result<Self, PlanError> {
        let output_end = ExactRatio::integer(projection.output_frames());
        let local_support = placement.local_support();
        if local_support.start.compare_integer(0).is_lt()
            || local_support
                .end
                .checked_sub(output_end)?
                .compare_integer(0)
                .is_gt()
        {
            return Err(PlanError::InvalidAudioRootPlacement(
                "support is outside the projected Preserve output",
            ));
        }
        let rate = projection
            .stage()
            .plan()
            .metadata()
            .presentation_basis
            .frame_rate;
        let frames_per_sample = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?;
        let root_grid = AudioSampleGrid::<AudioSample>::new(
            ExactRatio::ZERO,
            frames_per_sample,
            AudioBoundaryRule::RoundEven,
        )?;
        let project_at = |local: ExactRatio| {
            placement
                .origin()
                .checked_add(local.checked_mul(placement.root_frames_per_local_frame())?)
        };
        let local_support = placement.local_support();
        let extent = project_at(local_support.start)?..project_at(local_support.end)?;
        let samples = root_grid.boundary(extent.start)?..root_grid.boundary(extent.end)?;
        let sampling = sampling_for(&root_grid, &placement, samples.start)?;
        let meaningful_samples = samples.clone();
        let policy = Arc::new(projection.output_policy().remap_policy_window(
            local_support,
            extent.clone(),
            ExactRatio::ZERO,
            frames_per_sample,
            AudioBoundaryRule::RoundEven,
        )?);
        let policy_support = policy.support();
        // The retained full policy context is queried on this same signed
        // carrier even when the selected root allocation is narrower.
        root_grid.boundary(policy_support.start)?;
        root_grid.boundary(policy_support.end)?;
        Ok(Self {
            projection,
            placement,
            extent: extent.clone(),
            meaningful_extent: extent,
            samples: samples.clone(),
            meaningful_samples,
            sampling,
            reference_sample_at_anchor: i128::from(samples.start.0),
            root_grid,
            policy,
        })
    }

    pub fn projection(&self) -> &AudioStageProjection<'plan> {
        &self.projection
    }

    pub fn belongs_to(&self, plan: &RenderPlan) -> bool {
        self.projection.belongs_to(plan)
    }

    pub fn placement(&self) -> &AudioRootPlacement {
        &self.placement
    }

    /// Current exact project-frame allocation. This may extend beyond the
    /// original meaningful support after a resume; policy then reports silence.
    pub fn extent(&self) -> Range<ExactRatio> {
        self.extent.clone()
    }

    pub fn samples(&self) -> Range<AudioSample> {
        self.samples.clone()
    }

    pub fn sampling(&self) -> AudioSampleMap<AudioSample> {
        self.sampling
    }

    pub(crate) fn route_grid(&self) -> Result<AudioSampleGrid<AudioSample>, PlanError> {
        let full = ExactRatio::ZERO..ExactRatio::integer(self.projection.output_frames());
        if self.placement.local_support() != full
            || self.extent != self.meaningful_extent
            || self.samples != self.meaningful_samples
            || self.sampling != sampling_for(&self.root_grid, &self.placement, self.samples.start)?
            || self.reference_sample_at_anchor != i128::from(self.samples.start.0)
        {
            return Err(PlanError::InvalidPlan(
                "routed root requires its complete original projected capture",
            ));
        }
        // SoundRoute's recipe begins at frame zero, while its sample labels
        // retain the original absolute root clock and ties-to-even parity.
        AudioSampleGrid::new(
            ExactRatio::ZERO.checked_sub(self.extent.start)?,
            self.root_grid.frames_per_sample(),
            self.root_grid.boundary_rule(),
        )
    }

    /// Restrict allocation demand without changing the current sample map,
    /// intrinsic support, or output-policy filter context.
    pub fn crop(&self, extent: Range<ExactRatio>) -> Result<Self, PlanError> {
        if extent
            .start
            .checked_sub(self.extent.start)?
            .compare_integer(0)
            .is_lt()
            || extent
                .end
                .checked_sub(self.extent.end)?
                .compare_integer(0)
                .is_gt()
            || extent
                .end
                .checked_sub(extent.start)?
                .compare_integer(0)
                .is_le()
        {
            return Err(PlanError::InvalidAudioRootPlacement(
                "crop is outside the current projected root allocation",
            ));
        }
        let samples =
            self.root_grid.boundary(extent.start)?..self.root_grid.boundary(extent.end)?;
        let mut cropped = self.clone();
        cropped.extent = extent;
        cropped.samples = samples;
        Ok(cropped)
    }

    /// Continue the old local phase at `old_cut` on a new absolute allocation.
    /// Repeated resumes compose integer reference offsets rather than recovering
    /// a new phase from the placement's original coordinates.
    pub fn resume(
        &self,
        old_cut: AudioSample,
        extent: Range<ExactRatio>,
    ) -> Result<Self, PlanError> {
        if old_cut < self.samples.start
            || old_cut > self.samples.end
            || extent
                .end
                .checked_sub(extent.start)?
                .compare_integer(0)
                .is_le()
        {
            return Err(PlanError::InvalidAudioRootPlacement(
                "resume cut or allocation is outside its valid range",
            ));
        }
        let reference_cut = self.reference_at(old_cut)?;
        let samples =
            self.root_grid.boundary(extent.start)?..self.root_grid.boundary(extent.end)?;
        let sampling = AudioSampleMap::new(
            samples.start,
            self.sampling.local_at(old_cut)?,
            self.sampling.local_frames_per_sample(),
        )?;
        let mut resumed = self.clone();
        resumed.extent = extent;
        resumed.samples = samples;
        resumed.sampling = sampling;
        resumed.reference_sample_at_anchor = reference_cut;
        Ok(resumed)
    }

    /// Re-evaluate explicit Hold policy on the original absolute RoundEven
    /// grid. Source endpoint masks remain an input to Preserve, not output
    /// silence. Samples beyond the old meaningful support are suppressed.
    pub fn policy(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioPolicyQuery<AudioSample>, PlanError> {
        limits.validate()?;
        if samples.start < self.samples.start
            || samples.end < samples.start
            || samples.end > self.samples.end
        {
            return Err(PlanError::AudioRangeOutOfRange);
        }

        let reference_start = self.reference_at(samples.start)?;
        let reference_end = self.reference_at(samples.end)?;
        let meaningful_start = i128::from(self.meaningful_samples.start.0);
        let meaningful_end = i128::from(self.meaningful_samples.end.0);
        let valid_reference =
            reference_start.max(meaningful_start)..reference_end.min(meaningful_end);
        let offset = self.reference_offset()?;
        let valid_current = if valid_reference.start < valid_reference.end {
            i64_label(
                valid_reference
                    .start
                    .checked_sub(offset)
                    .ok_or(deadpan_core::TimeError::Overflow)?,
            )?
                ..i64_label(
                    valid_reference
                        .end
                        .checked_sub(offset)
                        .ok_or(deadpan_core::TimeError::Overflow)?,
                )?
        } else {
            samples.start.0..samples.start.0
        };

        let mut suppressed = Vec::new();
        if samples.start.0 < valid_current.start {
            suppressed.push(AudioSample(samples.start.0)..AudioSample(valid_current.start));
        }
        if valid_current.end < samples.end.0 {
            suppressed.push(AudioSample(valid_current.end)..samples.end);
        }

        let mut contents = Vec::new();
        let mut lookup = Default::default();
        let mut work = 0;
        if valid_current.start < valid_current.end {
            let root_policy = self.policy.policy_after_preserve(
                SignalSample(i64_label(
                    i128::from(valid_current.start)
                        .checked_add(offset)
                        .ok_or(deadpan_core::TimeError::Overflow)?,
                )?)
                    ..SignalSample(i64_label(
                        i128::from(valid_current.end)
                            .checked_add(offset)
                            .ok_or(deadpan_core::TimeError::Overflow)?,
                    )?),
                limits,
            )?;
            for range in root_policy.suppressed {
                let start = i64_label(
                    i128::from(range.start.0)
                        .checked_sub(offset)
                        .ok_or(deadpan_core::TimeError::Overflow)?,
                )?;
                let end = i64_label(
                    i128::from(range.end.0)
                        .checked_sub(offset)
                        .ok_or(deadpan_core::TimeError::Overflow)?,
                )?;
                suppressed.push(AudioSample(start)..AudioSample(end));
            }
            contents = root_policy.contents;
            lookup = root_policy.lookup;
            work = root_policy.work;
        }
        merge_audio_ranges(&mut suppressed);
        Ok(AudioPolicyQuery {
            suppressed,
            contents,
            lookup,
            work,
        })
    }

    fn reference_offset(&self) -> Result<i128, PlanError> {
        self.reference_sample_at_anchor
            .checked_sub(i128::from(self.sampling.anchor().0))
            .ok_or(deadpan_core::TimeError::Overflow.into())
    }

    fn reference_at(&self, sample: AudioSample) -> Result<i128, PlanError> {
        self.reference_sample_at_anchor
            .checked_add(i128::from(sample.0) - i128::from(self.sampling.anchor().0))
            .ok_or(deadpan_core::TimeError::Overflow.into())
    }
}

fn sampling_for(
    grid: &AudioSampleGrid<AudioSample>,
    placement: &AudioRootPlacement,
    anchor: AudioSample,
) -> Result<AudioSampleMap<AudioSample>, PlanError> {
    AudioSampleMap::new(
        anchor,
        grid.at(anchor)?
            .checked_sub(placement.origin())?
            .checked_div(placement.root_frames_per_local_frame())?,
        grid.frames_per_sample()
            .checked_div(placement.root_frames_per_local_frame())?,
    )
}

fn i64_label(value: i128) -> Result<i64, PlanError> {
    i64::try_from(value).map_err(|_| deadpan_core::TimeError::Overflow.into())
}

fn merge_audio_ranges(ranges: &mut Vec<Range<AudioSample>>) {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<AudioSample>> = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        if range.start >= range.end {
            continue;
        }
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    *ranges = merged;
}
