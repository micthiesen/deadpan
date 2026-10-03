//! One independent source voice through its checked current owner occurrence.

use super::*;
use deadpan_plan::AudioSourceOccurrence;

/// Raw independently time-mapped PCM on the absolute project sample grid.
/// Current Hold suppression is included; event edges, gain, allowances and
/// mixing into the authored bus remain separate responsibilities.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceOccurrenceBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub instance: InstancePath,
    pub allocation: Range<AudioSample>,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

impl StageAudio {
    /// Prepare this sound independently through every enclosing Preserve stage.
    /// The Original's decoder inputs, sample bindings and descriptor cache are
    /// not substituted for the sound's complete source recipe.
    pub fn read_source_voice_occurrence(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        voice: &AudioSourceOccurrence<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<SourceOccurrenceBlock, StageAudioError> {
        check_cancel(cancelled)?;
        if !voice.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        validate_timeout(timeout)?;
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(StageAudioError::Range)?,
        );
        let allocation = voice.samples();
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
        control.admit_dependency(&voice.source().asset)?;
        let queries = self.source_occurrence_queries(voice, start..end, control)?;
        let mut block = self.read_queries(provider, start, frames, control, 0, queries)?;
        // A fully masked query still depends on this recipe. Observe its live
        // source even when the processing query contains only allocated silence.
        let source = resolve_source(provider, &self.plan, &voice.source().asset, cancelled)?;
        block.dependencies.insert(
            voice.source().asset.clone(),
            control.observe(&voice.source().asset, source)?,
        );
        control.check()?;
        Ok(SourceOccurrenceBlock {
            schema_version: 1,
            stage: "source_occurrence_pcm_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            instance: voice.instance().clone(),
            allocation,
            start,
            samples: block.samples,
            suppressed: block.suppressed,
        })
    }

    pub(super) fn source_occurrence_queries<'plan>(
        &self,
        voice: &AudioSourceOccurrence<'plan>,
        samples: Range<AudioSample>,
        control: WorkControl<'_>,
    ) -> Result<RootReadQueries<'plan>, StageAudioError> {
        let processing = voice.processing(samples.clone(), control.query_limits()?)?;
        control.spend_plan_work(processing.work)?;
        let policy = voice.policy(samples.clone(), control.query_limits()?)?;
        control.spend_plan_work(policy.work)?;
        // There is no sequential Original contribution in this read. Its
        // endpoint envelopes and exhaustion masks cannot gate a separate voice.
        Ok(RootReadQueries {
            flattened: AudioQuery {
                project_id: self.plan.metadata().project_id.clone(),
                revision_id: self.plan.metadata().revision_id.clone(),
                samples,
                spans: Vec::new(),
                lookup: Default::default(),
                work: 0,
            },
            processing,
            policy,
            fades: None,
        })
    }
}
