use super::*;
use crate::stages::shared_mix::SharedMix;
use deadpan_core::*;

/// Actual prepared sound queries populate the witness before any contribution
/// PCM exists. The WAV fixture is setup, not cold source-admission evidence.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(in crate::stages::shared_components) fn with_completed_fixture(
    dynamic: bool,
    gap: bool,
    skip: bool,
    stop: bool,
    run: impl FnOnce(CompletedPolicy<'_, '_>, WorkControl<'_>),
) {
    with_fixture(dynamic, gap, skip, stop, false, run)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn with_fixture(
    dynamic: bool,
    gap: bool,
    skip: bool,
    stop: bool,
    inactive_dynamic: bool,
    run: impl FnOnce(CompletedPolicy<'_, '_>, WorkControl<'_>),
) {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (base, provider) = crate::stages::controlled_reads::fixture_document_at_rate(rate);
    let mut wire = serde_json::to_value(&base).unwrap();
    let id = |value| NodeId::new(value).unwrap();
    let span = |end| {
        SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
            SourceTimestamp {
                ticks: end,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
        )
        .unwrap()
    };
    let mut leaf: BeatNode = serde_json::from_value(wire["nodes"]["a"].clone()).unwrap();
    let NodeKind::Source { source } = &mut leaf.kind else {
        panic!("Source fixture")
    };
    source.duration = FrameDuration::new(2).unwrap();
    source.audio.as_mut().unwrap().span = span(2);
    source.audio_mapping = SourceAudioMapping::natural_rate(span(2), rate).unwrap();
    let range = GainRange::new(ExactRatio::ZERO, ExactRatio::integer(2)).unwrap();
    let envelopes = if dynamic {
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                range,
                GainDb::UNITY,
                vec![GainSegment::new(range.end(), GainDb::UNITY, GainCurve::Step).unwrap()],
            )
            .unwrap(),
        ]
    } else {
        Vec::new()
    };
    let mut root = BeatNode::sequence("Root", vec![id("a")]);
    root.audio_treatments = AudioTreatments::with_equalizer_stages(
        vec![
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::Equalizer,
        ],
        Some(ClipGain::new(GainDb::UNITY, false, envelopes, vec![]).unwrap()),
        None,
        None,
        Some(
            EqualizerRecipe::new(
                [GainDb::new(3000).unwrap(), GainDb::UNITY, GainDb::UNITY],
                None,
                None,
            )
            .unwrap(),
        ),
    )
    .unwrap()
    .with_wet_sends(
        TailSends::default()
            .with_level(TailEffect::Reverb, Some(GainDb::UNITY))
            .with_level(TailEffect::Delay, Some(GainDb::UNITY)),
    );
    wire["nodes"] =
        serde_json::to_value(BTreeMap::from([(id("root"), root), (id("a"), leaf)])).unwrap();
    wire["assets"]["media"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    let sound = |end| {
        let source = SourceAudio {
            asset: AssetId::new("media").unwrap(),
            span: span(end),
        };
        SoundEvent {
            owner: id("root"),
            label: "Cold fixture".into(),
            mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
            source,
            offset: AudioSample(0),
            treatments: AudioTreatments::default(),
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        }
    };
    wire["sounds"] = serde_json::to_value(BTreeMap::from([
        (
            SoundId::new("five").unwrap(),
            sound(if gap { 1 } else { 2 }),
        ),
        (SoundId::new("two").unwrap(), sound(2)),
    ]))
    .unwrap();
    if inactive_dynamic {
        wire["sounds"].as_object_mut().unwrap().remove("five");
        let mut leaf: BeatNode = serde_json::from_value(wire["nodes"]["a"].clone()).unwrap();
        let envelope = GainEnvelope::new(
            GainClock::OwnerOutput,
            range,
            GainDb::UNITY,
            vec![GainSegment::new(range.end(), GainDb::UNITY, GainCurve::Step).unwrap()],
        )
        .unwrap();
        leaf.audio_treatments = AudioTreatments::with_equalizer_stages(
            vec![AudioTreatmentStage::ClipGain],
            Some(ClipGain::new(GainDb::UNITY, false, vec![envelope], vec![]).unwrap()),
            None,
            None,
            None,
        )
        .unwrap();
        wire["nodes"]["a"] = serde_json::to_value(leaf).unwrap();
        let event = sound(2);
        wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
            id("a"),
            BTreeMap::from([(
                SoundId::new("inactive").unwrap(),
                BeatSound {
                    label: event.label,
                    source: event.source,
                    mapping: event.mapping,
                    offset: event.offset,
                    treatments: event.treatments,
                    start_edge: event.start_edge,
                    end_edge: event.end_edge,
                    overflow: event.overflow,
                },
            )]),
        )]))
        .unwrap();
    }
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work: &work,
    };
    let samples = AudioSample(0)..AudioSample(2);
    let roots = renderer
        .prepare_root_sounds(&plan, samples.start, 2, control)
        .unwrap();
    assert_eq!(roots.len(), if inactive_dynamic { 1 } else { 2 });
    let retained = if inactive_dynamic {
        renderer
            .prepare_sound_processing_plans_in_context(&plan, samples.clone(), control, None)
            .unwrap()
    } else {
        BTreeMap::new()
    };
    let beats = if inactive_dynamic {
        renderer
            .prepare_beat_sounds(&plan, &retained, samples.clone(), control, None)
            .unwrap()
    } else {
        Vec::new()
    };
    let query = plan
        .audio_gain_owners(samples.clone(), control.query_limits().unwrap())
        .unwrap();
    control.spend_plan_work(query.work()).unwrap();
    let clock = query.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node == id("root"))
        .unwrap();
    let boundary = stop.then(|| shared_mix::Boundary {
        owner: OwnerKey::new(clock),
        before: clock.treatments().unwrap().order().len(),
    });
    let skipped = if skip {
        vec![authored_gain::identity(clock)]
    } else {
        Vec::new()
    };
    let mut mix = SharedMix::new(samples.start, 2, boundary, Some(2), true, None, &skipped);
    let mut beat_routes = Vec::new();
    for beat in &beats {
        for occurrence in &beat.occurrences {
            let route = mix
                .route(
                    occurrence.voice.gain_owners(),
                    occurrence.samples.clone(),
                    (5, Contribution::Sound(TailSends::default())),
                    control,
                )
                .unwrap();
            beat_routes.push((5, route));
        }
    }
    let ids = if inactive_dynamic {
        vec![2]
    } else {
        vec![5, 2]
    };
    let routes = roots
        .iter()
        .zip(ids.iter().copied())
        .map(|(_, voice)| {
            mix.route(
                std::slice::from_ref(clock),
                samples.clone(),
                (voice, Contribution::Sound(TailSends::default())),
                control,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    mix.begin_prediction().unwrap();
    let reserved = mix.resident_frames().unwrap();
    renderer.make_room(reserved, false, control).unwrap();
    renderer.active_frames += reserved;
    mix.allocate_policy();
    let policy = if inactive_dynamic {
        shared_eq::populate_inputs_for_test(
            &mut mix,
            shared_eq::PopulationFixtureInputs {
                original: None,
                roots: &roots,
                root_range: samples,
                root_ids: &ids,
                root_routes: &routes,
                beats: &beats,
                beat_routes: &beat_routes,
            },
            |id| id != 5,
            control,
        )
        .unwrap()
    } else {
        shared_eq::populate_roots_for_test(&mut mix, &roots, samples, &ids, &routes, control)
            .unwrap()
    };
    assert!(
        policy
            .source
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
    assert_eq!(provider.calls, 0);
    assert_eq!(work.borrow().source_checks, 0);
    assert_eq!(work.borrow().prepared_frames, 0);
    let scratch = policy.visit_resident_frames().unwrap();
    renderer.make_room(scratch, false, control).unwrap();
    renderer.active_frames += scratch;
    run(policy, control);
    // A cancellation/error fixture retains cleanup without rechecking its
    // intentionally refused control or restoring/refunding spent work.
    renderer.active_frames -= scratch + reserved;
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(provider.calls, 0);
}

#[test]
fn empty_completed_walk_still_checks_refusal_and_bills_its_entry() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    let mut mix = SharedMix::new(AudioSample(0), 2, None, None, true, None, &[]);
    mix.begin_prediction().unwrap();
    mix.allocate_policy();
    let policy = shared_eq::populate_roots_for_test(
        &mut mix,
        &[],
        AudioSample(0)..AudioSample(2),
        &[],
        &[],
        control,
    )
    .unwrap();
    assert!(policy.visit_resident_frames().unwrap() > 0);
    let before = work.borrow().plan_work;
    policy
        .visit_demands(control, |_| panic!("empty owner set"))
        .unwrap();
    assert!(work.borrow().plan_work > before);
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        policy
            .visit_demands(control, |_| panic!("cancelled"))
            .is_err()
    );
}

#[test]
fn failed_population_cannot_return_completion_and_windows_remain_exact() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    let mut mix = SharedMix::new(AudioSample(0), 2, None, None, true, None, &[]);
    mix.begin_prediction().unwrap();
    mix.allocate_policy();
    assert!(matches!(
        shared_eq::populate_roots_for_test(
            &mut mix,
            &[],
            AudioSample(0)..AudioSample(2),
            &[7],
            &[],
            control
        ),
        Err(StageAudioError::Plan(PlanError::InvalidPlan(
            "root sound prediction routes differ"
        )))
    ));
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ;
    assert!(matches!(
        shared_eq::populate_roots_for_test(
            &mut mix,
            &[],
            AudioSample(0)..AudioSample(2),
            &[],
            &[],
            control
        ),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    work.borrow_mut().plan_work = 0;
    let policy = shared_eq::populate_roots_for_test(
        &mut mix,
        &[],
        AudioSample(0)..AudioSample(2),
        &[],
        &[],
        control,
    )
    .unwrap();
    assert!(policy.active(0, control).is_err());
    assert!(
        policy
            .validate(&policy.source.routes, AudioSample(1), 2, control)
            .is_err()
    );
    assert!(
        policy
            .validate(&policy.source.routes, AudioSample(0), 3, control)
            .is_err()
    );
    policy
        .validate(&policy.source.routes, AudioSample(0), 2, control)
        .unwrap();
    assert_eq!(work.borrow().source_checks, 0);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn an_inactive_dynamic_beat_route_cannot_switch_the_active_constant_schedule() {
    with_fixture(false, false, false, false, true, |policy, control| {
        let source = policy.source;
        assert_eq!(source.routes.len(), 2);
        assert_eq!(source.routes[0].voice, 5);
        assert_eq!(source.routes[1].voice, 2);
        assert!(!Program::constant(&source.routes[0], &source.owners));
        assert!(Program::constant(&source.routes[1], &source.owners));
        assert!(!policy.active(0, control).unwrap());
        assert!(policy.active(1, control).unwrap());
        let mut calls = Vec::new();
        policy
            .visit_demands(control, |request| {
                assert!(!request.permissions.contains(5));
                assert!(request.permissions.contains(2));
                let (class, stage) = match request.kind {
                    Temporal::Equalizer(stage) => (0, stage),
                    Temporal::Wet(TailEffect::Reverb) => (1, 0),
                    Temporal::Wet(TailEffect::Delay) => (1, 1),
                };
                calls.push((class, stage, request.sample.0, request.voice.unwrap()));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            calls,
            vec![
                (0, 1, 0, 2),
                (0, 1, 1, 2),
                (1, 0, 0, 2),
                (1, 1, 0, 2),
                (1, 0, 1, 2),
                (1, 1, 1, 2)
            ]
        );
    });
}
