//! Intrinsic Preserve preparation placed on an explicit absolute output grid.

use super::*;
use deadpan_plan::{AudioProjectedRoot, AudioRootPlacement, AudioSampleMap};

/// One projected physical domain on the timeline's absolute RoundEven grid.
/// This is raw time-mapped PCM, before edge fades, voice effects and mastering.
/// Allocation and exact lookup phase are independent, including after a resume.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectedRootBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<AudioDefinitionSelector>,
    pub instance: InstancePath,
    pub placement: AudioRootPlacement,
    pub extent: Range<ExactRatio>,
    pub allocation: Range<AudioSample>,
    pub sampling: AudioSampleMap<AudioSample>,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

impl StageAudio {
    /// Prepare the full intrinsic history once, then sample it using a checked
    /// root placement or continued phase. Re-evaluate exact output policy on its
    /// owning RoundEven grid; do not stretch or relabel a PointCeil mask.
    pub fn read_projected_root(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioProjectedRoot<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<ProjectedRootBlock, StageAudioError> {
        check_cancel(cancelled)?;
        if !domain.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        validate_timeout(timeout)?;
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(StageAudioError::Range)?,
        );
        let allocation = domain.samples();
        if frames == 0
            || frames > MAX_OUTPUT_FRAMES
            || start < allocation.start
            || end > allocation.end
        {
            return Err(StageAudioError::Range);
        }
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        self.read_projected_root_controlled(provider, domain, start, frames, control)
    }

    pub(super) fn read_projected_root_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioProjectedRoot<'_>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<ProjectedRootBlock, StageAudioError> {
        control.check()?;
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(StageAudioError::Range)?,
        );
        let allocation = domain.samples();
        let policy = domain.policy(start..end, control.query_limits()?)?;
        control.spend_plan_work(policy.work)?;
        for content in &policy.contents {
            control.preflight_content(content, &self.plan)?;
        }
        // Placement can be valid exact geometry but exceed the resampler's
        // admitted phase/rate range. Reject it before preparing any source PCM.
        let sampling = domain.sampling();
        let recipe =
            projected_root_recipe(domain, self.plan.metadata().presentation_basis.frame_rate)?;
        // A selected output window cannot conceal unsupported full preparation
        // history. This admission also checks hidden ordinary and Bound stages.
        self.preflight_projected(domain.projection(), control, 1)?;
        let prepared = self.prepare_projected(domain.projection(), provider, control, 1)?;
        let mut samples =
            sample_prepared(&prepared.samples, recipe, start, frames, control.cancelled)?;
        apply_suppression(start, &mut samples, &policy.suppressed, |sample| sample.0)?;
        control.check()?;
        let descriptor = domain.projection().stage().descriptor();
        Ok(ProjectedRootBlock {
            schema_version: 1,
            stage: "projected_root_pcm_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            definition: descriptor.definition.clone(),
            instance: descriptor.instance.clone(),
            placement: domain.placement().clone(),
            extent: domain.extent(),
            allocation,
            sampling,
            start,
            samples,
            suppressed: merged_suppression(policy.suppressed),
        })
    }
}

pub(super) fn projected_root_recipe(
    domain: &AudioProjectedRoot<'_>,
    rate: FrameRate,
) -> Result<ResampleRecipe, StageAudioError> {
    let allocation = domain.samples();
    let sampling = domain.sampling();
    stage_recipe(
        usize::try_from(domain.projection().output_policy().sample_count()?.0)
            .map_err(|_| TimeError::Overflow)?,
        allocation.clone(),
        sampling.local_at(allocation.start)?,
        sampling.local_frames_per_sample(),
        rate,
    )
}
