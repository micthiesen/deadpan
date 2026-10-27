//! Constructor headroom and retained native storage have different lifetimes.

use super::*;
use deadpan_core::{
    AudioTreatments, BeatNode, ColorPolicy, GainDb, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    PresentationBasis, ProjectDocument, TailSends,
};

const BASE_RESIDENCY: u64 = 17;

fn fixture(effect: Option<TailEffect>) -> Arc<RenderPlan> {
    let id = |name: &str| NodeId::new(name).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("wet-residency").unwrap(),
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
    if let Some(effect) = effect {
        let mut root = BeatNode::sequence("Root", vec![id("tone")]);
        root.audio_treatments = AudioTreatments::default().with_wet_sends(
            TailSends::default().with_level(effect, Some(GainDb::new(-6000).unwrap())),
        );
        let tone = BeatNode::hold(
            "Tone",
            HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(512).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Tone {
                    frequency_hz: 440,
                    level: GainDb::new(-6000).unwrap(),
                },
            },
        );
        wire["nodes"] =
            serde_json::to_value(BTreeMap::from([(id("root"), root), (id("tone"), tone)])).unwrap();
    }
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    Arc::new(RenderPlan::compile(&document).unwrap())
}

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work,
    }
}

fn units(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(8)
}

#[test]
fn wet_processor_retains_native_bound_once_with_full_construction_admission() {
    let plan = fixture(None);
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let preparation = units(crate::FiniteWetEffect::preparation_bytes_bound(effect));
        let retained = units(crate::FiniteWetEffect::retained_bytes_bound(effect));
        let mut renderer = StageAudio::with_limits(
            Arc::clone(&plan),
            StageLimits {
                maximum_resident_frames: u32::try_from(BASE_RESIDENCY + preparation).unwrap(),
                ..StageLimits::default()
            },
        )
        .unwrap();
        renderer.active_frames = BASE_RESIDENCY;
        let mut filter = None;
        let mut resident = 0;
        let shape = crate::wet_effect_descriptor(effect);
        let input = vec![0.0; shape.history_frames + shape.output_frames];
        let mut left = vec![123.0; shape.output_frames];
        let mut right = vec![-321.0; shape.output_frames];
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ - (shape.history_frames + 1),
            ..ReadWork::default()
        });
        let control = control(&cancelled, &work);
        for _ in 0..2 {
            renderer
                .prepare_shared_wet(effect, &mut filter, &mut resident, control)
                .unwrap();
            assert_eq!(renderer.active_frames, BASE_RESIDENCY + retained);
            assert_eq!(resident, retained);
            assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
            filter
                .as_mut()
                .unwrap()
                .process_tile(&input, &input, &mut left, &mut right, &cancelled)
                .unwrap();
            assert!(left.iter().chain(&right).all(|value| *value == 0.0));
        }
    }
}

#[test]
fn wet_processor_requires_full_preparation_headroom_before_publication() {
    let plan = fixture(None);
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let preparation = units(crate::FiniteWetEffect::preparation_bytes_bound(effect));
        let retained = units(crate::FiniteWetEffect::retained_bytes_bound(effect));
        let shape = crate::wet_effect_descriptor(effect);
        for headroom in [retained, preparation - 1] {
            let mut renderer = StageAudio::with_limits(
                Arc::clone(&plan),
                StageLimits {
                    maximum_resident_frames: u32::try_from(BASE_RESIDENCY + headroom).unwrap(),
                    ..StageLimits::default()
                },
            )
            .unwrap();
            renderer.active_frames = BASE_RESIDENCY;
            let mut filter = None;
            let mut resident = 0;
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            assert!(matches!(
                renderer.prepare_shared_wet(
                    effect,
                    &mut filter,
                    &mut resident,
                    control(&cancelled, &work),
                ),
                Err(StageAudioError::Limit("resident PCM or stage cache"))
            ));
            assert_eq!(renderer.active_frames, BASE_RESIDENCY);
            assert_eq!(resident, 0);
            assert!(filter.is_none());
            assert_eq!(work.borrow().plan_work, shape.history_frames + 1);
        }
    }
}

#[test]
fn wet_processor_cancel_timeout_and_work_refusals_do_not_publish_or_charge_residency() {
    let plan = fixture(None);
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let shape = crate::wet_effect_descriptor(effect);
        for failure in 0..3 {
            let mut renderer = StageAudio::new(Arc::clone(&plan));
            renderer.active_frames = BASE_RESIDENCY;
            let mut filter = None;
            let mut resident = 0;
            let cancelled = AtomicBool::new(failure == 0);
            let spent = if failure == 2 {
                MAX_PLAN_WORK_PER_READ - shape.history_frames
            } else {
                0
            };
            let work = RefCell::new(ReadWork {
                plan_work: spent,
                ..ReadWork::default()
            });
            let mut control = control(&cancelled, &work);
            if failure == 1 {
                control.deadline = Instant::now() - Duration::from_secs(1);
            }
            let error = renderer
                .prepare_shared_wet(effect, &mut filter, &mut resident, control)
                .unwrap_err();
            match failure {
                0 => assert!(error.is_cancelled()),
                1 => assert!(matches!(error, StageAudioError::Timeout)),
                _ => assert!(matches!(
                    error,
                    StageAudioError::Limit("plan work per read")
                )),
            }
            assert_eq!(work.borrow().plan_work, spent);
            assert_eq!(renderer.active_frames, BASE_RESIDENCY);
            assert_eq!(resident, 0);
            assert!(filter.is_none());
        }
    }
}

struct NoSources(usize);

impl AudioSourceProvider for NoSources {
    fn source(
        &mut self,
        _: &ProjectId,
        _: &RevisionId,
        _: &AssetId,
        _: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.0 += 1;
        Err(PreparationError::SourceUnavailable(
            "no media in fixture".into(),
        ))
    }
}

#[test]
fn shared_wet_graph_cleanup_restores_active_residency_on_success_and_error() {
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let plan = fixture(Some(effect));
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        renderer.active_frames = BASE_RESIDENCY;
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let mut provider = NoSources(0);
        let block = renderer
            .read_shared_input(
                &mut provider,
                AudioSample(128),
                1,
                control(&cancelled, &work),
                InputSelection::default(),
            )
            .unwrap();
        assert_eq!(block.samples.len(), 1);
        assert!(block.relative_depth > 0, "must prepare a temporal wet tile");
        assert_eq!(renderer.active_frames, BASE_RESIDENCY);
        assert_eq!(provider.0, 0);

        let successful_work = work.borrow().plan_work;
        assert!(successful_work > 1);
        let one_short = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ - (successful_work - 1),
            ..ReadWork::default()
        });
        let mut cold = StageAudio::new(plan);
        cold.active_frames = BASE_RESIDENCY;
        // The nonidentity -6 dB send applies a billed transfer after the wet
        // callback has constructed and published its processor. Reproduce the
        // cold schedule with one less unit to exercise cleanup of that retained
        // graph storage, rather than only an entry-time refusal.
        assert!(matches!(
            cold.read_shared_input(
                &mut provider,
                AudioSample(128),
                1,
                control(&cancelled, &one_short),
                InputSelection::default(),
            ),
            Err(StageAudioError::Limit("plan work per read"))
        ));
        assert_eq!(one_short.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        assert_eq!(cold.active_frames, BASE_RESIDENCY);
        assert_eq!(provider.0, 0);

        let exhausted = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ,
            ..ReadWork::default()
        });
        assert!(matches!(
            renderer.read_shared_input(
                &mut provider,
                AudioSample(128),
                1,
                control(&cancelled, &exhausted),
                InputSelection::default(),
            ),
            Err(StageAudioError::Limit("plan work per read"))
        ));
        assert_eq!(renderer.active_frames, BASE_RESIDENCY);
        assert_eq!(provider.0, 0);
    }
}
