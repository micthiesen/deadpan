use super::*;

fn capture_range(
    document: &ProjectDocument,
    parent: &str,
    first: i64,
    last: i64,
    name: &str,
) -> CapturedEditSlice {
    let slice = CapturedEditSlice::capture(
        document,
        &id(parent),
        FrameRange::new(ProjectFrame(first), ProjectFrame(last)).unwrap(),
        AudioTimingId {
            allocation: RevisionId::new(name).unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    slice.validate_capture(document).unwrap();
    CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap()
}

fn render(document: &ProjectDocument, first: i64, last: i64) -> Vec<[f32; 2]> {
    let mut reader = StageAudio::new(compile(document, false));
    let mut provider = ClockProvider::new(document);
    let rate = document.presentation_basis().frame_rate;
    let boundary = |frame: i64| {
        i64::try_from(
            ExactRatio::new(
                i128::from(frame) * 48_000 * i128::from(rate.denominator()),
                i128::from(rate.numerator()),
            )
            .unwrap()
            .round_even()
            .unwrap(),
        )
        .unwrap()
    };
    let first = boundary(first);
    let last = boundary(last);
    let mut output = Vec::new();
    for sample in (first..last).step_by(256) {
        output.extend(
            bus(
                &mut reader,
                &mut provider,
                sample,
                (last - sample).min(256) as u32,
            )
            .samples,
        );
    }
    output
}

fn compare_copy(original: &[[f32; 2]], actual: &[[f32; 2]], treated: bool, context: &str) {
    assert!(!actual.is_empty());
    assert!(actual.len().abs_diff(original.len()) <= 1);
    let mut worst = (0.0_f32, 0, 0, 0.0, 0.0);
    for (index, actual) in actual.iter().enumerate() {
        let expected = original.get(index).copied().unwrap_or([0.0; 2]);
        for channel in 0..2 {
            assert!(actual[channel].is_finite() && expected[channel].is_finite());
            if treated {
                let difference = (actual[channel] - expected[channel]).abs();
                if difference > worst.0 {
                    worst = (
                        difference,
                        index,
                        channel,
                        actual[channel],
                        expected[channel],
                    );
                }
            } else {
                assert_eq!(
                    actual[channel], expected[channel],
                    "sample={index} channel={channel}"
                );
            }
        }
    }
    assert!(
        worst.0 < 1e-6,
        "{context}: worst difference/sample/channel/actual/expected={worst:?}"
    );
}

fn partial_copy(treated: bool, shared: bool) {
    for edge in [AudioEdgePolicy::Hard, AudioEdgePolicy::Automatic] {
        let mut wire = serde_json::to_value(ntsc(edge)).unwrap();
        let rate = FrameRate::new(30_000, 1001).unwrap();
        // No original Hold gate at either destination. This isolates the
        // event's transported envelope from deliberately current policies.
        wire["nodes"]["lead"] = serde_json::to_value(picture_only(rate, 2)).unwrap();
        if treated {
            let eq = EqualizerRecipe::new(
                [6000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
                None,
                None,
            )
            .unwrap();
            let treatment = AudioTreatments::default()
                .with_equalizer(Some(eq))
                .unwrap()
                .with_wet_sends(
                    TailSends::default()
                        .with_level(TailEffect::Reverb, Some(GainDb::new(-6000).unwrap())),
                );
            wire["beat_sounds"]["owner"]["effect"]["treatments"] =
                serde_json::to_value(treatment).unwrap();
            if shared {
                wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(
                    AudioTreatments::default().with_equalizer(Some(eq)).unwrap(),
                )
                .unwrap();
            }
        }
        let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let expected = render(&before, 3, 6);
        assert!(expected.iter().flatten().any(|value| value.abs() > 0.001));
        let slice = capture_range(&before, "root", 3, 6, "capture-partial");
        let once = paste_repeat_slice(&before, "root", 3, &slice, "partial-copy");
        assert_eq!(once.duration().unwrap().frames(), 11);
        let actual = render(&once, 8, 11);
        compare_copy(&expected, &actual, treated, &format!("first {edge:?}"));

        let recopy = capture_range(&once, "partial-copy-node-0", 9, 11, "recapture-partial");
        let expected = render(&once, 9, 11);
        let twice = paste_repeat_slice(&once, "root", 4, &recopy, "partial-recopy");
        compare_copy(
            &expected,
            &render(&twice, 11, 13),
            treated,
            &format!("recopy {edge:?}"),
        );
        let expected = render(&twice, 11, 13);
        let deleted = edit(
            &twice,
            "remove-original-owner",
            Command::DeleteRipple {
                node: id("owner"),
                timing: AudioTimingId {
                    allocation: RevisionId::new("remove-original-owner").unwrap(),
                    ordinal: 0,
                },
            },
        );
        compare_copy(
            &expected,
            &render(&deleted, 7, 9),
            treated,
            &format!("delete {edge:?}"),
        );
        assert!(!deleted.beat_sounds().contains_key(&id("owner")));
        assert_eq!(deleted.beat_sounds().len(), 2);
    }
}

#[test]
fn partial_sound_copy_and_recopy_keep_dry_samples_and_virtual_edges() {
    partial_copy(false, false);
}

#[test]
fn partial_sound_copy_and_recopy_keep_event_processing_history() {
    partial_copy(true, false);
}

#[test]
fn partial_sound_copy_and_recopy_keep_shared_processing_history() {
    partial_copy(true, true);
}

#[test]
fn partial_range_of_preserve_owner_still_rejects_before_sound_capture() {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    let rate = FrameRate::new(30_000, 1001).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(picture_only(rate, 2)).unwrap();
    wire["nodes"]["inner"] =
        serde_json::to_value(retime("owner", 6, 0..4, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["outer"] =
        serde_json::to_value(retime("inner", 4, 0..6, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer", "tail"]);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let error = CapturedEditSlice::capture(
        &before,
        &id("root"),
        FrameRange::new(ProjectFrame(3), ProjectFrame(6)).unwrap(),
        AudioTimingId {
            allocation: RevisionId::new("capture-nested-partial").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap_err();
    assert!(error.message.contains("Range endpoints require"), "{error}");
}

#[test]
fn new_copy_wrapper_equalizer_uses_delivered_sound_as_its_input() {
    let before = ntsc(AudioEdgePolicy::Hard);
    let slice = capture_range(&before, "root", 3, 6, "capture-wrapper");
    let copied = paste_repeat_slice(&before, "root", 3, &slice, "wrapper-copy");
    let dry = render(&copied, 8, 11);
    let eq = EqualizerRecipe::new(
        [6000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )
    .unwrap();
    let mut filter = deadpan_audio::Equalizer::new(eq, &AtomicBool::new(false)).unwrap();
    let first = frame_sample_boundary(8);
    let last = frame_sample_boundary(11);
    let mut expected = Vec::new();
    for tile in (first / 1024 * 1024..last).step_by(1024) {
        let mut left = vec![0.0; 5120];
        let mut right = vec![0.0; 5120];
        for index in 0..5120 {
            let at = tile - 2048 + index as i64;
            if first <= at && at < last {
                let frame = dry[(at - first) as usize];
                left[index] = f64::from(frame[0]);
                right[index] = f64::from(frame[1]);
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
        for at in tile.max(first)..(tile + 1024).min(last) {
            expected.push([
                out_left[(at - tile) as usize] as f32,
                out_right[(at - tile) as usize] as f32,
            ]);
        }
    }
    let treated = edit(
        &copied,
        "wrapper-eq",
        Command::SetAudioTreatments {
            node: id("wrapper-copy-node-0"),
            treatments: AudioTreatments::default().with_equalizer(Some(eq)).unwrap(),
        },
    );
    compare_copy(&expected, &render(&treated, 8, 11), true, "new wrapper EQ");
}

#[test]
fn copied_owner_preparation_keeps_the_terminal_sample_hidden_by_new_rounding() {
    let before = ntsc(AudioEdgePolicy::Hard);
    let slice = capture_range(&before, "root", 3, 6, "terminal-capture");
    let once = paste_repeat_slice(&before, "root", 3, &slice, "terminal-copy");
    let recopy = capture_range(&once, "terminal-copy-node-0", 9, 11, "terminal-recapture");
    let twice = paste_repeat_slice(&once, "root", 4, &recopy, "terminal-recopy");
    let owner = twice
        .beat_sounds()
        .keys()
        .find(|id| id.as_str().starts_with("terminal-recopy"))
        .unwrap();
    let expected = super::owner_input::read_input(&before, &id("owner"), 9608, 3);
    assert_ne!(expected[1], [0.0; 2]);
    assert_eq!(expected[2], [0.0; 2]);
    let plan = compile(&twice, false);
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: owner.clone(),
                repeats: vec![],
            },
            Default::default(),
        )
        .unwrap();
    assert_eq!(context.samples().end, AudioSample(20821));
    let mut reader = StageAudio::new(plan.clone());
    let mut provider = ClockProvider::new(&twice);
    let read = |reader: &mut StageAudio, provider: &mut ClockProvider| {
        reader.read_beat_sound_owner_input(
            provider,
            &context,
            AudioSample(20820),
            3,
            TIMEOUT,
            &AtomicBool::new(false),
        )
    };
    assert_eq!(read(&mut reader, &mut provider).unwrap().samples, expected);
    assert_eq!(read(&mut reader, &mut provider).unwrap().samples, expected);
    provider.fixture.unavailable = true;
    assert!(read(&mut reader, &mut provider).is_err());
}

#[test]
fn shared_copy_history_preserves_half_sample_ties_after_an_odd_move() {
    // Three samples per two frames deliberately puts both full-owner edges on
    // half-sample ties. The selected output moves by minus three sample labels.
    let rate = FrameRate::new(32_000, 1).unwrap();
    let base = document(
        rate,
        &["lead", "owner", "tail"],
        [
            ("lead", picture_only(rate, 1)),
            ("owner", picture_only(rate, 4)),
            ("tail", picture_only(rate, 1)),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let voice = voice_recipe(rate, 6);
    wire["beat_sounds"] = serde_json::json!({"owner": {"effect": BeatSound {
        label: "Tiny sound".into(), source: voice.source, mapping: voice.mapping,
        offset: voice.offset, treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard, end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    }}});
    let eq = EqualizerRecipe::new(
        [6000, -6000, 3000].map(|v| GainDb::new(v).unwrap()),
        None,
        None,
    )
    .unwrap();
    wire["nodes"]["owner"]["audio_treatments"] =
        serde_json::to_value(AudioTreatments::default().with_equalizer(Some(eq)).unwrap()).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let expected = render(&before, 2, 5);
    let captured = capture_range(&before, "root", 2, 5, "half-capture");
    let copied = paste_repeat_slice(&before, "root", 0, &captured, "half-copy");
    let actual = render(&copied, 0, 3);
    assert_eq!(expected.len(), 5);
    assert_eq!(actual.len(), 4);
    compare_copy(&expected, &actual, true, "half-sample copy");
}
