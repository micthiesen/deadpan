//! Prepared gate queries stay present exactly when processed event output uses them.
use super::*;
use deadpan_core::{
    AssetRecord, AudioBindingState, AudioEdgePolicy, AudioTimingRecord, AudioTreatments, BeatNode,
    BeatSound, ColorPolicy, GainDb, HoldAudio, HoldRecipe, HoldVideo, PresentationBasis,
    ProjectDocument, Saturation, SoundClockJournal, SoundClockReference, SoundEvent, SoundId,
    SoundOverflowPolicy, SourceQualificationId, SourceSpan, SourceTimeBase, SourceTimestamp,
    TailEffect, TailSends,
};
use deadpan_plan::{AudioRoutedRootInput, AudioSoundGateQuery};

fn id(value: &str) -> deadpan_core::NodeId {
    deadpan_core::NodeId::new(value).unwrap()
}

fn asset() -> AssetId {
    AssetId::new("sound").unwrap()
}

fn sound() -> SoundId {
    SoundId::new("event").unwrap()
}

fn span(end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn hold(frames: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Gate fixture",
        HoldRecipe {
            duration: FrameDuration::new(frames).unwrap(),
            video: HoldVideo::Background,
            picture_context: None,
            audio,
        },
    )
}

fn fixture(root: bool, clocked: bool, treatments: AudioTreatments) -> Arc<RenderPlan> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("post-gates").unwrap(),
        RevisionId::new("live").unwrap(),
        PresentationBasis {
            width: 64,
            height: 64,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Root", vec![id("lead"), id("scope"), id("tail")]),
        ),
        (id("scope"), BeatNode::sequence("Scope", vec![id("owner")])),
        (id("lead"), hold(32, HoldAudio::Silence)),
        (
            id("owner"),
            hold(
                512,
                HoldAudio::Tone {
                    frequency_hz: 440,
                    level: GainDb::UNITY,
                },
            ),
        ),
        (id("tail"), hold(32, HoldAudio::Silence)),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset(),
        AssetRecord {
            label: "Independent source".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span(8192)),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    let source = SourceAudio {
        asset: asset(),
        span: span(512),
    };
    let mapping = SourceAudioMapping::natural_rate(source.span, rate).unwrap();
    if root {
        wire["sounds"] = serde_json::to_value(BTreeMap::from([(
            sound(),
            SoundEvent {
                owner: id("root"),
                label: "Root event".into(),
                source,
                mapping,
                offset: AudioSample(32),
                treatments,
                start_edge: AudioEdgePolicy::Automatic,
                end_edge: AudioEdgePolicy::Automatic,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]))
        .unwrap();
    } else {
        wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
            id("owner"),
            BTreeMap::from([(
                sound(),
                BeatSound {
                    label: "Attached event".into(),
                    source,
                    mapping,
                    offset: AudioSample(0),
                    treatments,
                    start_edge: AudioEdgePolicy::Automatic,
                    end_edge: AudioEdgePolicy::Automatic,
                    overflow: SoundOverflowPolicy::Reject,
                },
            )]),
        )]))
        .unwrap();
    }
    if clocked {
        let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let timing = AudioTimingId {
            allocation: RevisionId::new("capture").unwrap(),
            ordinal: 0,
        };
        let bindings = AudioBindingState::new_with_sound_clocks(
            vec![AudioTimingRecord {
                id: timing.clone(),
                layout: deadpan_core::FrozenAudioLayout::capture(&before).unwrap(),
            }],
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::from([(
                id("owner"),
                BTreeMap::from([(
                    sound(),
                    SoundClockJournal::new(
                        id("scope"),
                        vec![SoundClockReference::new(timing, id("scope"), id("owner"))],
                    )
                    .unwrap(),
                )]),
            )]),
        )
        .unwrap();
        wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    }
    Arc::new(RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap())
}

fn recipes() -> [(AudioTreatments, bool); 4] {
    [
        (AudioTreatments::default(), false),
        (
            AudioTreatments::default()
                .with_trim(GainDb::new(-3000).unwrap())
                .unwrap(),
            false,
        ),
        (
            AudioTreatments::default()
                .with_saturation(Some(Saturation::new(GainDb::new(6000).unwrap()).unwrap()))
                .unwrap(),
            true,
        ),
        (
            AudioTreatments::default().with_wet_sends(
                TailSends::default()
                    .with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap())),
            ),
            true,
        ),
    ]
}

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

fn exact_budget(
    used: usize,
    mut prepare: impl FnMut(WorkControl<'_>) -> Result<(), StageAudioError>,
) {
    let cancelled = AtomicBool::new(false);
    for available in [used, used - 1] {
        let work = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ - available,
            ..Default::default()
        });
        let result = prepare(control(&cancelled, &work));
        if available == used {
            assert!(result.is_ok(), "exact measured work: {result:?}");
            assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        } else {
            assert!(result.is_err(), "one operation less must refuse");
        }
    }
}

fn assert_post_query(
    treatments: &AudioTreatments,
    expected_processing: bool,
    prepared: &[deadpan_plan::AudioSoundGateSpan],
    mut legacy: impl FnMut(AudioQueryLimits) -> Result<AudioSoundGateQuery, PlanError>,
) {
    let explicit = legacy(query_limits()).unwrap();
    assert!(explicit.work > 1);
    if expected_processing {
        assert_eq!(prepared, explicit.spans);
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let result =
            shared_eq::prepare_post_gates(treatments, control(&cancelled, &work), |limits| {
                legacy(limits)
            })
            .unwrap();
        assert_eq!(result, explicit.spans);
        assert_eq!(work.borrow().plan_work, 1 + explicit.work);
    } else {
        assert!(prepared.is_empty());
        // A real second gate query cannot fit the one operation needed for
        // deciding that no processed output exists. The helper must not run it.
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ - 1,
            ..Default::default()
        });
        let result =
            shared_eq::prepare_post_gates(treatments, control(&cancelled, &work), |limits| {
                legacy(limits)
            })
            .unwrap();
        assert!(result.is_empty());
        assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        assert!(
            legacy(AudioQueryLimits {
                maximum_work: 1,
                ..query_limits()
            })
            .is_err()
        );
    }
}

#[test]
fn post_gates_root_preparation_skips_only_unused_queries_with_exact_work() {
    for (treatments, processing) in recipes() {
        let plan = fixture(true, false, treatments.clone());
        let mut stage = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let prepared = stage
            .prepare_root_sounds(&plan, AudioSample(0), 256, control(&cancelled, &work))
            .unwrap();
        assert_eq!(prepared.len(), 1);
        assert!(!prepared[0].fades.is_empty());
        assert_post_query(&treatments, processing, &prepared[0].post_fades, |limits| {
            prepared[0]
                .voice
                .post_treatment_gates(AudioSample(0)..AudioSample(256), limits)
        });
        exact_budget(work.borrow().plan_work, |control| {
            StageAudio::new(Arc::clone(&plan))
                .prepare_root_sounds(&plan, AudioSample(0), 256, control)
                .map(|_| ())
        });
    }
}

#[test]
fn post_gates_current_and_routed_beats_preserve_required_queries_and_work() {
    for clocked in [false, true] {
        for (treatments, processing) in recipes() {
            let plan = fixture(false, clocked, treatments.clone());
            let mut stage = StageAudio::new(Arc::clone(&plan));
            let cancelled = AtomicBool::new(false);
            let discovery_work = RefCell::new(ReadWork::default());
            let retained = stage
                .prepare_sound_processing_plans(
                    &plan,
                    AudioSample(0)..AudioSample(256),
                    control(&cancelled, &discovery_work),
                )
                .unwrap();
            let work = RefCell::new(ReadWork::default());
            let prepared = stage
                .prepare_beat_sounds(
                    &plan,
                    &retained,
                    AudioSample(0)..AudioSample(256),
                    control(&cancelled, &work),
                    None,
                )
                .unwrap();
            assert_eq!(prepared.len(), 1);
            assert_eq!(prepared[0].occurrences.len(), 1);
            let occurrence = &prepared[0].occurrences[0];
            assert!(!occurrence.fades.is_empty());
            match &occurrence.input {
                beat_sounds::PreparedBeatInput::Current(_) => {
                    assert!(!clocked);
                    assert_post_query(&treatments, processing, &occurrence.post_fades, |limits| {
                        occurrence.voice.gate_fades(
                            AudioEdgePolicy::Hard,
                            AudioEdgePolicy::Hard,
                            occurrence.samples.clone(),
                            limits,
                        )
                    });
                }
                beat_sounds::PreparedBeatInput::Routed {
                    query,
                    plan: historical,
                } => {
                    assert!(clocked);
                    let AudioRoutedRootInput::Occurrence(original) = query.domain.input() else {
                        panic!("expected retained occurrence")
                    };
                    let scopes = plan
                        .beat_sound_clock_scopes(
                            &id("owner"),
                            &sound(),
                            deadpan_core::MAX_DOCUMENT_NODES,
                        )
                        .unwrap();
                    let (binding, _) = scopes[0]
                        .bind_historical_plan(historical, deadpan_core::MAX_DOCUMENT_NODES)
                        .unwrap();
                    let (alias, _) = binding
                        .alias_occurrence(
                            original,
                            &occurrence.voice,
                            deadpan_core::MAX_DOCUMENT_NODES,
                        )
                        .unwrap();
                    assert_post_query(&treatments, processing, &occurrence.post_fades, |limits| {
                        occurrence.voice.routed_window_gate_fades_from(
                            original,
                            &alias,
                            query.domain.route(),
                            (AudioEdgePolicy::Hard, AudioEdgePolicy::Hard),
                            occurrence.samples.clone(),
                            limits,
                        )
                    });
                }
            }
            exact_budget(work.borrow().plan_work, |control| {
                StageAudio::new(Arc::clone(&plan))
                    .prepare_beat_sounds(
                        &plan,
                        &retained,
                        AudioSample(0)..AudioSample(256),
                        control,
                        None,
                    )
                    .map(|_| ())
            });
        }
    }
}

#[test]
fn post_gates_skip_is_lazy_and_required_query_refusals_still_propagate() {
    let cancelled = AtomicBool::new(false);
    for (treatments, processing) in recipes() {
        let work = RefCell::new(ReadWork::default());
        let called = std::cell::Cell::new(false);
        let result = shared_eq::prepare_post_gates(&treatments, control(&cancelled, &work), |_| {
            called.set(true);
            Err(PlanError::InvalidPlan("post gate query refusal"))
        });
        assert_eq!(called.get(), processing);
        assert_eq!(result.is_err(), processing);
        assert_eq!(work.borrow().plan_work, 1);

        let exhausted = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ,
            ..Default::default()
        });
        let called = std::cell::Cell::new(false);
        let result =
            shared_eq::prepare_post_gates(&treatments, control(&cancelled, &exhausted), |_| {
                called.set(true);
                Err(PlanError::InvalidPlan("query must not run without work"))
            });
        assert!(matches!(
            result,
            Err(StageAudioError::Limit("plan work per read"))
        ));
        assert!(!called.get());
        assert_eq!(exhausted.borrow().plan_work, MAX_PLAN_WORK_PER_READ);

        // The first normal gate query remains unconditional in both cases.
        let plan = fixture(true, false, treatments);
        let work = RefCell::new(ReadWork::default());
        assert!(
            StageAudio::new(Arc::clone(&plan))
                .prepare_root_sounds(&plan, AudioSample(-1), 17, control(&cancelled, &work))
                .is_err()
        );
    }
}
