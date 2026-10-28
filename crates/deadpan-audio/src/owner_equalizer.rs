//! Ordered EQ evaluation of one checked Original owner. This preparation API
//! does not store an authored EQ or claim to mix independently placed sounds.
//! The same private controller is usable by the eventual complete voice graph.

use deadpan_dsp::{
    FINITE_FILTER_INPUT_FRAMES as INPUT, FINITE_FILTER_OUTPUT_FRAMES as OUTPUT,
    FINITE_FILTER_RADIUS as RADIUS,
};
use deadpan_plan::{
    AudioOwnerInput, AudioOwnerQuery, AudioOwnerRetainedInput, AudioOwnerSupport,
    AudioTreatmentBoundary,
};

use super::*;

/// Worker-owned filter and its checked position in an existing owner's order.
/// Descendant treatments feed the owner; its prefix feeds EQ and its suffix
/// processes EQ output. Exterior owners and independent sounds are excluded.
/// Retained history is selected from the checked observation, never recovered
/// by inverting a nominal automation clock. No PCM or document is changed here.
pub struct PreparedOwnerEqualizer<'plan> {
    input: AudioOwnerInput<'plan>,
    retained: Option<AudioOwnerRetainedInput<'plan>>,
    boundary: AudioTreatmentBoundary<'plan>,
    filter: crate::Equalizer,
    work: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_counts_none_probe_and_preserves_actual_tone_pcm() {
        use crate::stages::authored_eq::eq_admission_fixture as fixture;
        use deadpan_core::{
            BeatNode, ColorPolicy, HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis,
            ProjectDocument,
        };

        // PreparedOwnerEqualizer applies an explicit recipe to raw owner input.
        // An already authored EQ in its suffix is intentionally unsupported by
        // the memoryless gain reader. Keep the oracle's recipe on another plan.
        let authored_plan = fixture::fixture();
        let authored_owner = fixture::owner(&authored_plan);
        let recipe = authored_owner.treatments().unwrap().equalizer().unwrap();
        let id = |name: &str| NodeId::new(name).unwrap();
        let document = ProjectDocument::new(
            ProjectId::new("eq-admission").unwrap(),
            RevisionId::new("revision").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(48_000, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let mut wire = serde_json::to_value(document).unwrap();
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (
                id("root"),
                BeatNode::sequence("Raw owner", vec![id("tone")]),
            ),
            (
                id("tone"),
                BeatNode::hold(
                    "Tone",
                    HoldRecipe {
                        picture_context: None,
                        duration: FrameDuration::new(512).unwrap(),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Tone {
                            frequency_hz: 440,
                            level: deadpan_core::GainDb::new(-6000).unwrap(),
                        },
                    },
                ),
            ),
        ]))
        .unwrap();
        let plan = Arc::new(
            RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap(),
        );
        let owner = fixture::owner(&plan);
        assert!(owner.treatments().unwrap().is_empty());
        let input = plan
            .audio_owner_input(&owner, AudioSample(128), Default::default())
            .unwrap();
        let probe = input
            .retained_probe_at(input.observed_at(), Default::default())
            .unwrap();
        assert!(probe.input().is_none());
        assert!(probe.work() > 0);
        let expected_work = input.work() + probe.work();
        let cancelled = AtomicBool::new(false);
        let mut equalizer = PreparedOwnerEqualizer::new(input, 0, recipe, &cancelled).unwrap();
        assert_eq!(equalizer.work, expected_work);

        let mut provider = fixture::NoSources(0);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let actual = renderer
            .read_owner_equalized_controlled(
                &mut provider,
                &mut equalizer,
                AudioSample(128),
                32,
                fixture::control(&cancelled, &work),
                0,
            )
            .unwrap();
        let mut oracle = StageAudio::new(Arc::clone(&authored_plan));
        let oracle_work = RefCell::new(ReadWork::default());
        let expected = oracle
            .read_authored_equalizers(
                &mut provider,
                AudioSample(128),
                32,
                fixture::control(&cancelled, &oracle_work),
                0,
                &[],
            )
            .unwrap();
        assert_eq!(
            actual
                .samples
                .iter()
                .map(|frame| frame.map(|value| (value as f32).to_bits()))
                .collect::<Vec<_>>(),
            expected
                .samples
                .iter()
                .map(|frame| frame.map(f32::to_bits))
                .collect::<Vec<_>>(),
        );
        assert_eq!(actual.suppressed, expected.suppressed);
        assert_eq!(provider.0, 0);
        assert_eq!(renderer.active_frames, 0);
        assert_eq!(oracle.active_frames, 0);
    }

    #[test]
    fn controlled_eq_shares_work_deadline_depth_and_retains_all_halo_dependencies() {
        let (document, mut provider) = super::super::controlled_reads::fixture_document_at_rate(
            FrameRate::new(48_000, 1).unwrap(),
        );
        let plan = Arc::new(RenderPlan::compile(&document).unwrap());
        let query = plan
            .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
            .unwrap();
        let input = plan
            .audio_owner_input(
                &query.spans()[0].owners()[0],
                AudioSample(0),
                Default::default(),
            )
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut eq = PreparedOwnerEqualizer::new(input, 0, Default::default(), &cancelled).unwrap();
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(10),
            work: &work,
        };
        for _ in 0..2 {
            *work.borrow_mut() = ReadWork::default();
            let block = renderer
                .read_owner_equalized_controlled(
                    &mut provider,
                    &mut eq,
                    AudioSample(0),
                    64,
                    control,
                    0,
                )
                .unwrap();
            assert!(
                block
                    .dependencies
                    .contains_key(&AssetId::new("media").unwrap())
            );
            assert!(block.relative_depth > 1, "the future halo reaches Preserve");
            assert!(work.borrow().plan_work > 0);
            assert!(work.borrow().prepared_frames >= 6144);
            assert!(provider.calls > 0, "also admit warm cached Preserve inputs");
            provider.calls = 0;
        }
        work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ;
        assert!(matches!(
            renderer.read_owner_equalized_controlled(
                &mut provider,
                &mut eq,
                AudioSample(0),
                1,
                control,
                0
            ),
            Err(StageAudioError::Limit(_))
        ));
        *work.borrow_mut() = ReadWork::default();
        assert!(matches!(
            renderer.read_owner_equalized_controlled(
                &mut provider,
                &mut eq,
                AudioSample(0),
                1,
                control,
                renderer.limits.maximum_depth
            ),
            Err(StageAudioError::Limit(_))
        ));
        assert!(matches!(
            renderer.read_owner_equalized_controlled(
                &mut provider,
                &mut eq,
                AudioSample(0),
                1,
                WorkControl {
                    deadline: Instant::now() - Duration::from_secs(1),
                    ..control
                },
                0
            ),
            Err(StageAudioError::Timeout)
        ));
        assert_eq!(provider.calls, 0);
        assert_eq!(renderer.active_frames, 0);
    }
}

impl<'plan> PreparedOwnerEqualizer<'plan> {
    pub fn new(
        input: AudioOwnerInput<'plan>,
        before_stage: usize,
        recipe: deadpan_core::EqualizerRecipe,
        cancelled: &AtomicBool,
    ) -> Result<Self, StageAudioError> {
        check_cancel(cancelled)?;
        let boundary = input.owner().treatment_boundary(before_stage)?;
        let at = input.observed_at();
        let mut work = input.work();
        let probe = input.retained_probe_at(at, AudioQueryLimits::default())?;
        work += probe.work();
        let retained = if let Some(retained) = probe.into_input() {
            let query = retained.owners(
                at..AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?),
                AudioQueryLimits::default(),
            )?;
            work += query.work();
            // An ancestor of a bound child owns its complete current domain,
            // not the child's retained operand. Only adopt history containing
            // this exact checked owner and its retained provenance.
            query
                .spans()
                .iter()
                .any(|span| span.owners().iter().any(|owner| boundary.matches(owner)))
                .then_some(retained)
        } else {
            None
        };
        Ok(Self {
            input,
            retained,
            boundary,
            filter: crate::Equalizer::new(recipe, cancelled)?,
            work,
        })
    }

    pub fn boundary(&self) -> &AudioTreatmentBoundary<'plan> {
        &self.boundary
    }
    pub fn recipe(&self) -> deadpan_core::EqualizerRecipe {
        self.filter.recipe()
    }

    fn range(&self, samples: Range<AudioSample>) -> Range<AudioSample> {
        if self.retained.is_some() {
            samples
        } else {
            let support = self.input.context().root_samples();
            samples.start.max(support.start)..samples.end.min(support.end)
        }
    }

    fn owners(
        &self,
        samples: Range<AudioSample>,
        control: WorkControl<'_>,
    ) -> Result<AudioOwnerQuery<'plan>, StageAudioError> {
        let query = if let Some(retained) = &self.retained {
            retained.owners(samples, control.query_limits()?)?
        } else {
            self.input
                .context()
                .gain_owners(samples, control.query_limits()?)?
        };
        control.spend_plan_work(query.work())?;
        for span in query.spans() {
            deadpan_core::validate_audio_treatment_layers(
                span.owners().iter().filter_map(|owner| owner.treatments()),
            )?;
        }
        Ok(query)
    }
}

/// Signed f64 owner output before exterior owners, sound mixing and mastering.
/// Suppression is enforced after filtering, so a silent Hold cannot acquire an
/// incoming EQ tail. Mute remains an ordered treatment, not silence permission.
#[derive(Debug, Clone, Serialize)]
pub struct OwnerEqualizedBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub engine: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub definition: Option<AudioDefinitionSelector>,
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub before_stage: usize,
    pub recipe: deadpan_core::EqualizerRecipe,
    pub start: AudioSample,
    pub samples: Vec<[f64; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

struct EqualizedRead {
    samples: Vec<[f64; 2]>,
    suppressed: Vec<Range<AudioSample>>,
    dependencies: Dependencies,
    relative_depth: usize,
}

impl StageAudio {
    pub fn read_owner_equalized(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        equalizer: &mut PreparedOwnerEqualizer<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<OwnerEqualizedBlock, StageAudioError> {
        check_cancel(cancelled)?;
        validate_timeout(timeout)?;
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        let result =
            self.read_owner_equalized_controlled(provider, equalizer, start, frames, control, 0)?;
        Ok(OwnerEqualizedBlock {
            schema_version: 1,
            stage: "original_owner_eq_before_exterior_treatments_and_sound_mix",
            engine: crate::EQ_FILTER_ID,
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            definition: equalizer.boundary.owner().definition().cloned(),
            instance: equalizer.boundary.owner().instance().clone(),
            gap_after: equalizer.boundary.owner().gap_after().cloned(),
            before_stage: equalizer.boundary.before_stage(),
            recipe: equalizer.recipe(),
            start,
            samples: result.samples,
            suppressed: result.suppressed,
        })
    }

    fn read_owner_equalized_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        equalizer: &mut PreparedOwnerEqualizer<'_>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<EqualizedRead, StageAudioError> {
        control.check()?;
        if !equalizer.input.context().belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        if depth >= self.limits.maximum_depth {
            return Err(StageAudioError::Limit("nested stage depth"));
        }
        if frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(StageAudioError::Range)?;
        control.spend_plan_work(equalizer.work)?;
        let reservation = u64::from(frames) * 2;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            let mut result = EqualizedRead {
                samples: Vec::with_capacity(frames as usize),
                suppressed: Vec::new(),
                dependencies: Dependencies::new(),
                relative_depth: 1,
            };
            let mut at = start.0;
            while at < end {
                let tile = at
                    .div_euclid(OUTPUT as i64)
                    .checked_mul(OUTPUT as i64)
                    .ok_or(StageAudioError::Range)?;
                let tile_end = tile
                    .checked_add(OUTPUT as i64)
                    .ok_or(StageAudioError::Range)?;
                let block =
                    self.equalize_owner_tile(provider, equalizer, tile, control, depth + 1)?;
                let next = end.min(tile_end);
                let offset = usize::try_from(at - tile).map_err(|_| StageAudioError::Range)?;
                let count = usize::try_from(next - at).map_err(|_| StageAudioError::Range)?;
                result
                    .samples
                    .extend_from_slice(&block.samples[offset..offset + count]);
                result
                    .suppressed
                    .extend(block.suppressed.into_iter().filter_map(|range| {
                        let range =
                            range.start.max(AudioSample(at))..range.end.min(AudioSample(next));
                        (range.start < range.end).then_some(range)
                    }));
                result.dependencies.extend(block.dependencies);
                result.relative_depth = result.relative_depth.max(block.relative_depth);
                at = next;
            }
            result.suppressed = merged_suppression(result.suppressed);
            control.check()?;
            Ok(result)
        })();
        self.active_frames -= reservation;
        result
    }

    fn equalize_owner_tile(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        equalizer: &mut PreparedOwnerEqualizer<'_>,
        tile: i64,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<EqualizedRead, StageAudioError> {
        let begin = tile
            .checked_sub(RADIUS as i64)
            .ok_or(StageAudioError::Range)?;
        let end = begin
            .checked_add(INPUT as i64)
            .ok_or(StageAudioError::Range)?;
        // f64 planar input, planar output, interleaved output and one raw chunk.
        // Fixed native FFT state is separate, as for the stretch adapter.
        let reservation = ((INPUT + OUTPUT * 2) * 2 + MAX_OUTPUT_FRAMES as usize) as u64;
        self.reserve_budget(INPUT as u32, OUTPUT as u32, reservation, 0, control)?;
        let result = (|| {
            let mut left = vec![0.0; INPUT];
            let mut right = vec![0.0; INPUT];
            let mut admitted = [false; OUTPUT];
            let mut dependencies = Dependencies::new();
            let mut relative_depth = 1;
            let range = equalizer.range(AudioSample(begin)..AudioSample(end));
            let mut at = range.start;
            while at < range.end {
                control.check()?;
                let stop = AudioSample(
                    at.0.saturating_add(i64::from(MAX_OUTPUT_FRAMES))
                        .min(range.end.0),
                );
                let owners = equalizer.owners(at..stop, control)?;
                let count = u32::try_from(stop.0 - at.0).map_err(|_| StageAudioError::Range)?;
                let block = if let Some(retained) = &equalizer.retained {
                    self.read_retained_controlled(
                        provider,
                        retained,
                        at,
                        count,
                        control,
                        (depth, false),
                    )?
                } else {
                    self.read_context_controlled(
                        provider,
                        equalizer.input.context(),
                        at,
                        count,
                        control,
                        (depth, true),
                    )?
                };
                for span in owners.spans() {
                    let Some(index) = span
                        .owners()
                        .iter()
                        .position(|owner| equalizer.boundary.matches(owner))
                    else {
                        continue;
                    };
                    if span.support() == AudioOwnerSupport::Inactive {
                        continue;
                    }
                    for sample in span.samples().start.0..span.samples().end.0 {
                        let mut before = authored_gain::GainSum::default();
                        for owner in span.owners()[index + 1..].iter().rev() {
                            before.owner(owner, AudioSample(sample), control)?;
                        }
                        before.owner_boundary(
                            &span.owners()[index],
                            AudioSample(sample),
                            &equalizer.boundary,
                            true,
                            control,
                        )?;
                        let value = before.apply(block.samples[(sample - at.0) as usize])?;
                        left[(sample - begin) as usize] = value[0];
                        right[(sample - begin) as usize] = value[1];
                        if (tile..tile + OUTPUT as i64).contains(&sample) {
                            admitted[(sample - tile) as usize] = !block
                                .suppressed
                                .iter()
                                .chain(&block.exhausted)
                                .any(|range| range.contains(&AudioSample(sample)));
                        }
                    }
                }
                dependencies.extend(block.dependencies);
                relative_depth = relative_depth.max(block.relative_depth + 1);
                at = stop;
            }
            let mut output_left = vec![0.0; OUTPUT];
            let mut output_right = vec![0.0; OUTPUT];
            control.check()?;
            equalizer.filter.process_tile(
                &left,
                &right,
                &mut output_left,
                &mut output_right,
                control.cancelled,
            )?;
            let mut samples = vec![[0.0; 2]; OUTPUT];
            let range = equalizer.range(AudioSample(tile)..AudioSample(tile + OUTPUT as i64));
            if range.start < range.end {
                let owners = equalizer.owners(range, control)?;
                for span in owners.spans() {
                    let Some(owner) = span
                        .owners()
                        .iter()
                        .find(|owner| equalizer.boundary.matches(owner))
                    else {
                        continue;
                    };
                    for at in span.samples().start.0..span.samples().end.0 {
                        let index = (at - tile) as usize;
                        if !admitted[index] {
                            continue;
                        }
                        let mut after = authored_gain::GainSum::default();
                        after.owner_boundary(
                            owner,
                            AudioSample(at),
                            &equalizer.boundary,
                            false,
                            control,
                        )?;
                        samples[index] =
                            after.apply_wide([output_left[index], output_right[index]])?;
                    }
                }
            }
            let mut suppressed = Vec::new();
            for (index, allowed) in admitted.into_iter().enumerate() {
                if !allowed {
                    suppressed.push(
                        AudioSample(tile + index as i64)..AudioSample(tile + index as i64 + 1),
                    );
                }
            }
            control.check()?;
            Ok(EqualizedRead {
                samples,
                suppressed: merged_suppression(suppressed),
                dependencies,
                relative_depth,
            })
        })();
        self.active_frames -= reservation;
        result
    }
}
