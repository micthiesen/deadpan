//! All independently prepared occurrences of one recipe in a checked window.

use super::*;
use deadpan_core::NodeId;
use deadpan_plan::AudioSourceOccurrences;

/// Raw additive output before event effects or mastering. Each occurrence uses
/// the same authored recipe but has its own complete processing history.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceOccurrencesBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub owner: NodeId,
    pub window: Range<AudioSample>,
    pub occurrences: Vec<InstancePath>,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

struct PreparedOccurrence<'plan> {
    instance: InstancePath,
    samples: Range<AudioSample>,
    queries: RootReadQueries<'plan>,
}

impl StageAudio {
    /// Read a subwindow without recreating the batch's processing identities.
    /// Every occurrence shares one deadline, work budget and PCM residency cap.
    /// Current Hold gates apply independently after processing; all remaining
    /// samples sum in f64 before finite f32 conversion. Nothing is normalized.
    pub fn read_source_voice_occurrences(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        batch: &AudioSourceOccurrences<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<SourceOccurrencesBlock, StageAudioError> {
        check_cancel(cancelled)?;
        if !batch.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        validate_timeout(timeout)?;
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(StageAudioError::Range)?,
        );
        let window = batch.samples();
        if frames == 0 || frames > MAX_OUTPUT_FRAMES || start < window.start || end > window.end {
            return Err(StageAudioError::Range);
        }
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        control.admit_dependency(&batch.source().asset)?;
        control.spend_plan_work(batch.voices().len())?;
        let mut reads = Vec::new();
        for voice in batch.voices() {
            control.check()?;
            let extent = voice.samples();
            let samples = start.max(extent.start)..end.min(extent.end);
            if samples.start >= samples.end {
                continue;
            }
            let queries = self.source_occurrence_queries(voice, samples.clone(), control)?;
            self.preflight_root_queries(&queries, samples.start, count(&samples)?, control, 0)?;
            reads.push(PreparedOccurrence {
                instance: voice.instance().clone(),
                samples,
                queries,
            });
        }
        // All static histories pass admission before any source is opened.
        // Reserve the f64 sum plus the reader's two transient f32 buffers.
        let reservation = u64::from(frames) * 4;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = self.render_source_occurrences(provider, batch, start..end, reads, control);
        self.active_frames -= reservation;
        result
    }

    fn render_source_occurrences(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        batch: &AudioSourceOccurrences<'_>,
        samples: Range<AudioSample>,
        reads: Vec<PreparedOccurrence<'_>>,
        control: WorkControl<'_>,
    ) -> Result<SourceOccurrencesBlock, StageAudioError> {
        let mut sum = vec![[0.0_f64; 2]; count(&samples)? as usize];
        let mut suppressed = vec![samples.clone()];
        let mut occurrences = Vec::with_capacity(reads.len());
        for read in reads {
            control.spend_plan_work(count(&read.samples)? as usize)?;
            let block = self.read_queries(
                provider,
                read.samples.start,
                count(&read.samples)?,
                control,
                0,
                read.queries,
            )?;
            let offset = usize::try_from(read.samples.start.0 - samples.start.0)
                .map_err(|_| StageAudioError::Range)?;
            let expected = count(&read.samples)? as usize;
            if block.samples.len() != expected {
                return Err(PlanError::InvalidPlan("incomplete source occurrence PCM").into());
            }
            for (total, frame) in sum[offset..offset + expected].iter_mut().zip(block.samples) {
                for channel in 0..2 {
                    if !frame[channel].is_finite() {
                        return Err(PreparationError::InvalidSamples.into());
                    }
                    total[channel] += f64::from(frame[channel]);
                }
            }
            let mut silent = block.suppressed;
            if samples.start < read.samples.start {
                silent.push(samples.start..read.samples.start);
            }
            if read.samples.end < samples.end {
                silent.push(read.samples.end..samples.end);
            }
            suppressed =
                sound_events::intersect_suppression(&suppressed, &merged_suppression(silent));
            occurrences.push(read.instance);
        }
        // Empty, exhausted and wholly gated windows still depend on the source.
        // Defer this unconditional observation until preparation has passed its
        // runtime reservations, as in the single-occurrence reader.
        let asset = &batch.source().asset;
        let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
        control.observe(asset, source)?;
        let mut output = vec![[0.0; 2]; sum.len()];
        authored_gain::write_finite_samples(&mut output, sum)?;
        control.check()?;
        Ok(SourceOccurrencesBlock {
            schema_version: 1,
            stage: "source_occurrences_pcm_before_effects",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            owner: batch.owner().clone(),
            window: batch.samples(),
            occurrences,
            start: samples.start,
            samples: output,
            suppressed,
        })
    }
}
