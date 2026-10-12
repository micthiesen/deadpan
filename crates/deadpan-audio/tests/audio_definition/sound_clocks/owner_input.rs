use super::*;

fn cropped() -> ProjectDocument {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    wire["nodes"].as_object_mut().unwrap().remove("lead");
    wire["nodes"]["crop"] =
        serde_json::to_value(retime("owner", 2, 1..3, PitchPolicy::FollowSpeed)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["crop", "tail"]);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn read_input(document: &ProjectDocument, owner: &NodeId, start: i64, count: u32) -> Vec<[f64; 2]> {
    let plan = compile(document, false);
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: owner.clone(),
                repeats: vec![],
            },
            Default::default(),
        )
        .unwrap();
    StageAudio::new(plan.clone())
        .read_beat_sound_owner_input(
            &mut ClockProvider::new(document),
            &context,
            AudioSample(start),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

#[test]
fn sound_owner_input_keeps_saved_crop_clock_after_fractional_move_and_copy() {
    let before = cropped();
    let returned = remove_prefix(&insert_prefix(&before));
    let slice = capture_repeat_slice(&returned, "root", &id("crop"), "capture-crop");
    let copied = paste_repeat_slice(&returned, "root", 2, &slice, "copied-crop");
    let copied_owner = copied
        .beat_sounds()
        .keys()
        .find(|owner| **owner != id("owner"))
        .unwrap();
    assert!(!copied.audio_bindings().sound_clocks().is_empty());
    // The normal retained crop starts at frame four (sample 6406). The
    // expanded owner's history starts at frame three (4805). Rounding the
    // expanded start would shift old PCM by 6407 instead of the saved 6406.
    let expected = bus(
        &mut StageAudio::new(compile(&returned, false)),
        &mut ClockProvider::new(&returned),
        1000,
        128,
    )
    .samples;
    let current = bus(
        &mut StageAudio::new(compile(&copied, false)),
        &mut ClockProvider::new(&copied),
        6406 + 1000,
        128,
    )
    .samples;
    assert_eq!(current, expected);
    let actual = read_input(&copied, copied_owner, 6406 + 1000, 128);
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|frame| frame.map(f64::from))
            .collect::<Vec<_>>()
    );
    // Full history before the copied crop still comes from the first saved
    // source clock, including PCM that ordinary delivery does not expose.
    for at in [-1500, -800, 0, 101, 1000, 3074, 4000, 4600] {
        let actual = read_input(&copied, copied_owner, 6406 + at, 128);
        let expected = read_input(&returned, &id("owner"), at, 128);
        assert_eq!(actual, expected, "complete saved history at {at}");
    }
}

#[test]
fn sound_owner_input_preserves_nested_saved_preserve_and_current_edges() {
    let before = saved(&occurrence_document(), "owner", 3840);
    let moved = insert_prefix(&before);
    let plan = compile(&moved, false);
    let query = plan
        .audio_gain_owners(AudioSample(100)..AudioSample(101), Default::default())
        .unwrap();
    let owner = query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == id("owner"))
        .unwrap();
    let context = plan
        .audio_sound_owner_context(owner, Default::default())
        .unwrap();
    let mut reader = StageAudio::new(plan.clone());
    let mut provider = ClockProvider::new(&moved);
    for at in [18, 100, 700, 1150] {
        let actual = reader
            .read_beat_sound_owner_input(
                &mut provider,
                &context,
                AudioSample(at),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        let expected = bus(&mut StageAudio::new(plan.clone()), &mut provider, at, 128);
        assert_eq!(
            actual.samples,
            expected
                .samples
                .iter()
                .map(|frame| frame.map(f64::from))
                .collect::<Vec<_>>()
        );
        assert_eq!(actual.suppressed, expected.suppressed);
    }
    assert!(provider.fixture.context_calls > 0);
    provider.fixture.unavailable = true;
    assert!(
        reader
            .read_beat_sound_owner_input(
                &mut provider,
                &context,
                AudioSample(100),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .is_err()
    );
}

#[test]
fn sound_owner_input_admits_limits_before_media_and_recovers_after_failure() {
    let document = insert_prefix(&ntsc(AudioEdgePolicy::Hard));
    let plan = compile(&document, false);
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: id("owner"),
                repeats: vec![],
            },
            Default::default(),
        )
        .unwrap();
    let mut provider = ClockProvider::new(&document);
    let mut reader = StageAudio::with_limits(
        plan.clone(),
        StageLimits {
            maximum_resident_frames: 1280,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        reader.read_beat_sound_owner_input(
            &mut provider,
            &context,
            AudioSample(4805),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(provider.fixture.calls, 0);
    assert!(
        reader
            .read_beat_sound_owner_input(
                &mut provider,
                &context,
                AudioSample(4805),
                32,
                TIMEOUT,
                &AtomicBool::new(true),
            )
            .is_err()
    );
    assert_eq!(provider.fixture.calls, 0);
    assert!(
        reader
            .read_beat_sound_owner_input(
                &mut provider,
                &context,
                AudioSample(i64::MAX),
                32,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .is_err()
    );
    let mut foreign = StageAudio::new(compile(&document, false));
    assert!(matches!(
        foreign.read_beat_sound_owner_input(
            &mut provider,
            &context,
            AudioSample(4805),
            32,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    assert_eq!(provider.fixture.calls, 0);
    let actual = reader
        .read_beat_sound_owner_input(
            &mut provider,
            &context,
            AudioSample(4805),
            32,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples.len(), 32);
    assert!(provider.fixture.calls > 0);
    provider.fixture.unavailable = true;
    assert!(
        reader
            .read_beat_sound_owner_input(
                &mut provider,
                &context,
                AudioSample(4805),
                32,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .is_err()
    );
    provider.fixture.unavailable = false;
    assert_eq!(
        reader
            .read_beat_sound_owner_input(
                &mut provider,
                &context,
                AudioSample(4805),
                32,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples,
        actual.samples
    );
}

#[test]
fn sound_owner_input_saved_hidden_history_keeps_current_hold_fades() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let nodes = [
        ("a", picture_only(rate, 128)),
        ("pause", hold(64, HoldAudio::Silence)),
        ("b", picture_only(rate, 128)),
        (
            "owner",
            BeatNode::sequence("Sound group", vec![id("a"), id("pause"), id("b")]),
        ),
    ];
    let mut wire =
        serde_json::to_value(document(rate, &["owner"], nodes, BTreeMap::new())).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let whole = saved(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        "owner",
        320,
    );
    let mut wire = serde_json::to_value(whole).unwrap();
    for edge in ["start_edge", "end_edge"] {
        wire["beat_sounds"]["owner"]["effect"][edge] =
            serde_json::to_value(AudioEdgePolicy::Automatic).unwrap();
    }
    let whole = ProjectDocument::from_json(&wire.to_string()).unwrap();
    wire["nodes"]["crop"] =
        serde_json::to_value(retime("owner", 150, 100..250, PitchPolicy::FollowSpeed)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["crop"]);
    let moved = insert_prefix(&ProjectDocument::from_json(&wire.to_string()).unwrap());
    let plan = compile(&moved, false);
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: id("owner"),
                repeats: vec![],
            },
            Default::default(),
        )
        .unwrap();
    assert_eq!(context.samples(), AudioSample(-99)..AudioSample(221));
    let actual = StageAudio::new(plan.clone())
        .read_beat_sound_owner_input(
            &mut ClockProvider::new(&moved),
            &context,
            AudioSample(-60),
            250,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    let reference = bus(
        &mut StageAudio::new(compile(&whole, false)),
        &mut ClockProvider::new(&whole),
        39,
        250,
    );
    assert_eq!(
        actual.samples,
        reference
            .samples
            .iter()
            .map(|frame| frame.map(f64::from))
            .collect::<Vec<_>>()
    );
    assert_eq!(actual.suppressed, vec![AudioSample(29)..AudioSample(93)]);
}

#[test]
fn shared_eq_saved_crop_neutral_recipe_preserves_current_delivery_edges() {
    let before = remove_prefix(&insert_prefix(&cropped()));
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::default()
            .with_equalizer(Some(EqualizerRecipe::default()))
            .unwrap(),
    )
    .unwrap();
    let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut reader = StageAudio::new(compile(&treated, false));
    let mut original = StageAudio::new(compile(&before, false));
    for at in [0, 101, 1000, 3074] {
        let actual = bus(&mut reader, &mut ClockProvider::new(&treated), at, 128).samples;
        let expected = bus(&mut original, &mut ClockProvider::new(&before), at, 128).samples;
        assert_eq!(actual, expected, "neutral saved crop at {at}");
    }
}

#[test]
fn shared_eq_saved_copied_crop_filters_complete_history_and_rechecks_sources() {
    let returned = remove_prefix(&insert_prefix(&cropped()));
    let slice = capture_repeat_slice(&returned, "root", &id("crop"), "capture-eq-crop");
    let copied = paste_repeat_slice(&returned, "root", 2, &slice, "eq-copied-crop");
    let owner = copied
        .beat_sounds()
        .keys()
        .find(|owner| **owner != id("owner"))
        .unwrap();
    let recipe = EqualizerRecipe::new(
        [6000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )
    .unwrap();
    let mut wire = serde_json::to_value(&copied).unwrap();
    wire["nodes"][owner.as_str()]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::default()
            .with_equalizer(Some(recipe))
            .unwrap(),
    )
    .unwrap();
    let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&copied, false);
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: owner.clone(),
                repeats: vec![],
            },
            Default::default(),
        )
        .unwrap();
    let support = context.samples();
    let mut input = StageAudio::new(plan.clone());
    let mut oracle_provider = ClockProvider::new(&copied);
    let mut filter = deadpan_audio::Equalizer::new(recipe, &AtomicBool::new(false)).unwrap();
    let mut renderer = StageAudio::new(compile(&treated, false));
    let mut provider = ClockProvider::new(&treated);
    for at in [6406, 6507, 7406, 9480] {
        let tile = at / 1024 * 1024;
        let mut left = vec![0.0; 5120];
        let mut right = vec![0.0; 5120];
        let first = (tile - 2048).max(support.start.0);
        let end = (tile + 3072).min(support.end.0);
        for start in (first..end).step_by(256) {
            let count = (end - start).min(256) as u32;
            let raw = input
                .read_beat_sound_owner_input(
                    &mut oracle_provider,
                    &context,
                    AudioSample(start),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            for (offset, frame) in raw.samples.into_iter().enumerate() {
                left[(start - tile + 2048) as usize + offset] = frame[0];
                right[(start - tile + 2048) as usize + offset] = frame[1];
            }
        }
        let mut out_left = vec![0.0; 1024];
        let mut out_right = vec![0.0; 1024];
        filter
            .process_tile(
                &left,
                &right,
                &mut out_left,
                &mut out_right,
                &AtomicBool::new(false),
            )
            .unwrap();
        let actual = bus(&mut renderer, &mut provider, at, 32).samples;
        for (index, frame) in actual.iter().enumerate() {
            let offset = (at - tile) as usize + index;
            assert!(
                (f64::from(frame[0]) - out_left[offset]).abs() < 1e-7,
                "left at {at}+{index}"
            );
            assert!(
                (f64::from(frame[1]) - out_right[offset]).abs() < 1e-7,
                "right at {at}+{index}"
            );
        }
    }
    provider.fixture.unavailable = true;
    assert!(
        renderer
            .prepare_authored_bus(
                &mut provider,
                AudioSample(7406),
                32,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
}
