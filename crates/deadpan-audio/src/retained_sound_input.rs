//! Checked reads on an independent sound's retained input clock. Creative
//! treatments consume this input before saved delivery routes and current gates.

use super::*;
use deadpan_plan::{AudioRoutedRoot, AudioRoutedRootInput};

/// Complete source or processed occurrence input at its original integral
/// sample labels. `allocation` is the retained recipe allocation, which can
/// differ from the live route and project duration. Neither route gaps nor
/// current consuming Hold gates erase this history. The input's own processing
/// and policy remain authoritative; this handle grants no output permission.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RetainedSoundInputBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub allocation: Range<AudioSample>,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
    pub exhausted: Vec<Range<AudioSample>>,
}

pub(super) struct PreparedRetainedSoundInput<'plan> {
    start: AudioSample,
    frames: u32,
    raw: Option<AudioProcessingQuery<'plan>>,
}

fn occurrence_raw_queries(processing: AudioProcessingQuery<'_>) -> RootReadQueries<'_> {
    let suppressed = processing
        .spans
        .iter()
        .filter(|span| {
            matches!(
                &span.content,
                AudioSignalContent::Leaf(AudioContent::Silence { .. })
            )
        })
        .map(|span| span.samples.clone())
        .collect();
    RootReadQueries {
        flattened: AudioQuery {
            project_id: processing.project_id.clone(),
            revision_id: processing.revision_id.clone(),
            samples: processing.samples.clone(),
            spans: Vec::new(),
            lookup: Default::default(),
            work: 0,
        },
        processing,
        policy: AudioPolicyQuery {
            suppressed,
            contents: Vec::new(),
            lookup: Default::default(),
            work: 0,
        },
        fades: None,
    }
}

impl StageAudio {
    /// Read a checked route's input before moving or cutting any physical sample
    /// labels. This is preparation input, not the audible routed sound. Hidden
    /// processing and dependencies are admitted even for a silent selection.
    pub fn read_retained_sound_input(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        domain: &AudioRoutedRoot<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<RetainedSoundInputBlock, StageAudioError> {
        let budget = PreparationBudget::new(timeout, cancelled)?;
        let control = budget.control();
        control.check()?;
        if !domain.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let allocation = domain.route().recipe_samples();
        super::routed::routed_end(start.0, frames, allocation.start.0..allocation.end.0)?;
        self.preflight_retained_sound_input(domain.input(), control)?;
        let query = self.prepare_retained_sound_query(domain.input(), start, frames, control)?;
        let reservation = u64::from(frames) * 3;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            let (dependencies, relative_depth) =
                self.admit_retained_sound_input(provider, domain.input(), control)?;
            let mut block =
                self.read_retained_sound_query(provider, domain.input(), query, control)?;
            block.dependencies.extend(dependencies);
            block.relative_depth = block.relative_depth.max(relative_depth);
            control.check()?;
            Ok(RetainedSoundInputBlock {
                schema_version: 1,
                stage: "retained_sound_input_before_event_effects_and_current_gates",
                project_id: self.plan.metadata().project_id.clone(),
                revision_id: self.plan.metadata().revision_id.clone(),
                allocation,
                start,
                samples: block.samples,
                suppressed: block.suppressed,
                exhausted: block.exhausted,
            })
        })();
        self.active_frames -= reservation;
        result
    }

    pub(super) fn preflight_retained_sound_input(
        &self,
        input: &AudioRoutedRootInput<'_>,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        match input {
            AudioRoutedRootInput::Source(root) => control.admit_dependency(&root.source().asset),
            AudioRoutedRootInput::Projected(root) => {
                projected_root::projected_root_recipe(
                    root,
                    self.plan.metadata().presentation_basis.frame_rate,
                )?;
                self.preflight_projected(root.projection(), control, 1)?;
                Ok(())
            }
            AudioRoutedRootInput::Occurrence(voice) => {
                control.admit_dependency(&voice.source().asset)?;
                if let Some(projection) = voice.processing_projection() {
                    self.preflight_projected(projection, control, 1)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn prepare_retained_sound_query<'plan>(
        &self,
        input: &AudioRoutedRootInput<'plan>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<PreparedRetainedSoundInput<'plan>, StageAudioError> {
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(TimeError::Overflow)?,
        );
        let raw = match input {
            AudioRoutedRootInput::Source(root) => {
                let query = root.query(start..end, control.query_limits()?)?;
                control.spend_plan_work(query.work)?;
                for span in &query.spans {
                    let AudioSignalContent::Leaf(content) = &span.content else {
                        return Err(
                            PlanError::InvalidPlan("raw root capture contains processing").into(),
                        );
                    };
                    control.preflight_content(content, &self.plan)?;
                }
                Some(query)
            }
            AudioRoutedRootInput::Projected(root) => {
                let policy = root.policy(start..end, control.query_limits()?)?;
                control.spend_plan_work(policy.work)?;
                for content in &policy.contents {
                    control.preflight_content(content, &self.plan)?;
                }
                None
            }
            AudioRoutedRootInput::Occurrence(voice) => {
                let query = voice.processing(start..end, control.query_limits()?)?;
                control.spend_plan_work(query.work)?;
                let queries = occurrence_raw_queries(query);
                self.preflight_root_queries(&queries, start, frames, control, 0)?;
                Some(queries.processing)
            }
        };
        Ok(PreparedRetainedSoundInput { start, frames, raw })
    }

    pub(super) fn admit_retained_sound_input(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        input: &AudioRoutedRootInput<'_>,
        control: WorkControl<'_>,
    ) -> Result<(Dependencies, usize), StageAudioError> {
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        let (asset, projection) = match input {
            AudioRoutedRootInput::Source(root) => (Some(&root.source().asset), None),
            AudioRoutedRootInput::Projected(root) => (None, Some(root.projection())),
            AudioRoutedRootInput::Occurrence(voice) => (
                Some(&voice.source().asset),
                voice.processing_projection().map(Arc::as_ref),
            ),
        };
        if let Some(projection) = projection {
            let prepared = self.prepare_projected(projection, provider, control, 1)?;
            dependencies.extend(prepared.dependencies.clone());
            relative_depth = 1 + prepared.relative_depth;
        }
        if let Some(asset) = asset {
            let source = resolve_source(provider, &self.plan, asset, control.cancelled)?;
            dependencies.insert(asset.clone(), control.observe(asset, source)?);
        }
        Ok((dependencies, relative_depth))
    }

    pub(super) fn read_retained_sound_query(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        input: &AudioRoutedRootInput<'_>,
        query: PreparedRetainedSoundInput<'_>,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        match input {
            AudioRoutedRootInput::Source(_) => self.read_root_source_query(
                provider,
                query
                    .raw
                    .ok_or(PlanError::InvalidPlan("missing raw root query"))?,
                control,
            ),
            AudioRoutedRootInput::Projected(root) => {
                let block = self.read_projected_root_controlled(
                    provider,
                    root,
                    query.start,
                    query.frames,
                    control,
                )?;
                Ok(ReadBlock {
                    start: block.start,
                    samples: block.samples,
                    suppressed: block.suppressed,
                    dependencies: Dependencies::new(),
                    relative_depth: 0,
                    exhausted: Vec::new(),
                })
            }
            AudioRoutedRootInput::Occurrence(_) => self.read_queries(
                provider,
                query.start,
                query.frames,
                control,
                0,
                occurrence_raw_queries(
                    query
                        .raw
                        .ok_or(PlanError::InvalidPlan("missing retained occurrence query"))?,
                ),
            ),
        }
    }
}
