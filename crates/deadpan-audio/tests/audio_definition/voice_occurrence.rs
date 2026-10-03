use super::*;

use deadpan_plan::{AudioQueryLimits, AudioSourceVoiceRecipe};

fn occurrence_document() -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plays = IterationOrder::new(RevisionId::new("voice-plays").unwrap(), 2).unwrap();
    let document = document(
        rate,
        &["lead", "repeat"],
        [
            ("lead", hold(17, HoldAudio::Silence)),
            // Missing primary audio must not suppress an independently owned sound.
            (
                "owner",
                BeatNode {
                    kind: NodeKind::Source {
                        source: SourceNode {
                            duration: frames(3840),
                            video: SourceVideo::Stream {
                                asset: AssetId::new("picture").unwrap(),
                                span: audio(0..3840).span,
                            },
                            video_mapping: SourceVideoMapping::FitBeat,
                            audio: None,
                            audio_mapping: SourceAudioMapping::FitBeat,
                            audio_offset: AudioSample(0),
                            link: LinkRelation::Independent,
                            edit_window: None,
                        },
                    },
                    ..source(rate, 3840, 0..3840)
                },
            ),
            (
                "inner",
                retime("owner", 2560, 0..3840, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", 1280, 0..2560, PitchPolicy::Preserve),
            ),
            (
                "repeat",
                BeatNode {
                    kind: NodeKind::Repeat {
                        child: id("outer"),
                        iterations: plays,
                        gap: None,
                    },
                    ..BeatNode::sequence("Repeated passage", Vec::new())
                },
            ),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn occurrence(owner: &str, play: u32) -> InstancePath {
    InstancePath {
        node: id(owner),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: IterationOrder::new(RevisionId::new("voice-plays").unwrap(), 2)
                .unwrap()
                .at(play)
                .unwrap(),
        }],
    }
}

fn voice_recipe(rate: FrameRate, length: i64) -> AudioSourceVoiceRecipe {
    let source = audio(1024..1024 + length);
    AudioSourceVoiceRecipe {
        mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
        source,
        offset: AudioSample(0),
    }
}

fn raw_voice(length: u32) -> Vec<[f32; 2]> {
    sample_reference(
        1024..1024 + i64::from(length),
        ratio(1024, 1),
        ExactRatio::ONE,
        length,
        fixture_sample,
    )
}

#[test]
fn source_voice_occurrence_keeps_ntsc_root_phase_and_sample_offset_independent() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = document(
        rate,
        &["lead", "owner"],
        [
            ("lead", hold(1, HoldAudio::Silence)),
            ("owner", source(rate, 4, 0..100)),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&document, false);
    let mut recipe = voice_recipe(rate, 4800);
    recipe.offset = AudioSample(37);
    let voice = plan
        .source_voice_occurrence(
            InstancePath {
                node: id("owner"),
                repeats: Vec::new(),
            },
            recipe,
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(voice.samples(), AudioSample(1602)..AudioSample(8008));
    // At the owner's first allocated root sample the exact local phase is
    // 2/5 sample, independently of the sound's integral 37-sample offset.
    let mut expected = sample_reference(
        1024..5824,
        ratio(5122, 5).checked_sub(ratio(37, 1)).unwrap(),
        ExactRatio::ONE,
        6406,
        fixture_sample,
    );
    expected[..37].fill([0.0; 2]);
    expected[4837..].fill([0.0; 2]);
    let mut renderer = StageAudio::new(plan.clone());
    let mut provider = FixtureProvider::new();
    for start in [4790, 0, 4800, 6144, 33, 2049] {
        let count = (6406 - start).min(256);
        let block = renderer
            .read_source_voice_occurrence(
                &mut provider,
                &voice,
                AudioSample(1602 + start),
                count as u32,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            block.samples,
            expected[start as usize..(start + count) as usize],
            "NTSC offset at {start}"
        );
    }
}

#[test]
fn source_voice_occurrence_resamples_mono_before_its_independent_processing_chain() {
    let document = occurrence_document();
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    let span = |end| {
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
    };
    let source = SourceAudio {
        asset: AssetId::new("mono").unwrap(),
        span: span(3528),
    };
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["assets"]["mono"] = wire["assets"]["media"].clone();
    wire["assets"]["mono"]["audio"] = serde_json::to_value(span(44117)).unwrap();
    wire["assets"]["mono"]["content_hash"] = serde_json::json!("b".repeat(64));
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&document, false);
    let voice = plan
        .source_voice_occurrence(
            occurrence("owner", 1),
            AudioSourceVoiceRecipe {
                mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    document.presentation_basis().frame_rate,
                )
                .unwrap(),
                source,
                offset: AudioSample(0),
            },
            AudioQueryLimits::default(),
        )
        .unwrap();
    let mono = sample_reference(0..3528, ExactRatio::ZERO, ratio(147, 160), 3840, |index| {
        let value = ((index * 73) % 65_536 - 32_768) as f32 / 32_768.0;
        [value; 2]
    });
    let expected = stretch(&stretch(&mono, 2560, 3, 2), 1280, 2, 1);
    let mut provider = mix::MixProvider::new();
    let mut renderer = StageAudio::new(plan.clone());
    for start in [512, 0, 1024, 256, 768] {
        let block = renderer
            .read_source_voice_occurrence(
                &mut provider,
                &voice,
                AudioSample(1297 + start),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            block.samples,
            expected[start as usize..start as usize + 256]
        );
    }
}

#[test]
fn source_voice_occurrence_applies_silent_holds_after_complete_nested_processing() {
    let document = occurrence_document();
    let mut wire = serde_json::to_value(&document).unwrap();
    let source = wire["nodes"]["owner"].clone();
    wire["nodes"]["owner"] = serde_json::to_value(BeatNode::sequence(
        "Sound owner",
        vec![id("before-pause"), id("pause"), id("after-pause")],
    ))
    .unwrap();
    for (name, duration) in [("before-pause", 1), ("after-pause", 3838)] {
        wire["nodes"][name] = source.clone();
        wire["nodes"][name]["kind"]["source"]["duration"] = serde_json::json!(duration);
    }
    wire["nodes"]["pause"] = serde_json::to_value(hold(1, HoldAudio::Silence)).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&document, false);
    let voice = plan
        .source_voice_occurrence(
            occurrence("owner", 1),
            voice_recipe(document.presentation_basis().frame_rate, 3840),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let mut expected = stretch(&stretch(&raw_voice(3840), 2560, 3, 2), 1280, 2, 1);
    // The Hold occupies [1/3..2/3) on the final root clock. RoundEven allocates
    // one sample; the corresponding PointCeil interval allocates none.
    expected[0] = [0.0; 2];
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut provider = FixtureProvider::new();
    for start in [256, 0, 1024, 1] {
        let block = renderer
            .read_source_voice_occurrence(
                &mut provider,
                &voice,
                AudioSample(1297 + start),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            block.samples,
            expected[start as usize..start as usize + 256]
        );
        assert_eq!(
            block.suppressed,
            if start == 0 {
                vec![AudioSample(1297)..AudioSample(1298)]
            } else {
                Vec::new()
            },
        );
    }
}

#[test]
fn source_voice_occurrence_rejects_foreign_ranges_and_cancelled_reads_before_media() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let voice = plan
        .source_voice_occurrence(
            occurrence("owner", 1),
            voice_recipe(document.presentation_basis().frame_rate, 3840),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let mut provider = FixtureProvider::new();
    let mut foreign = StageAudio::new(Arc::new(plan.as_ref().clone()));
    assert!(matches!(
        foreign.read_source_voice_occurrence(
            &mut provider,
            &voice,
            AudioSample(1297),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::ForeignDomain),
    ));
    let mut renderer = StageAudio::new(plan.clone());
    for (start, count) in [(1296, 1), (2577, 1), (1297, 0), (1297, 257), (i64::MAX, 2)] {
        assert!(matches!(
            renderer.read_source_voice_occurrence(
                &mut provider,
                &voice,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Range),
        ));
    }
    assert!(
        renderer
            .read_source_voice_occurrence(
                &mut provider,
                &voice,
                AudioSample(1297),
                1,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(provider.calls, 0);
}

#[test]
fn source_voice_occurrence_admits_source_even_outside_the_audible_recipe() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let mut recipe = voice_recipe(document.presentation_basis().frame_rate, 512);
    recipe.offset = AudioSample(256);
    // The owner is the outer stage itself, so it has no enclosing processor.
    // At its first sample the independent recipe has not started yet.
    let voice = plan
        .source_voice_occurrence(occurrence("outer", 1), recipe, Default::default())
        .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut provider = FixtureProvider::new();
    let block = renderer
        .read_source_voice_occurrence(
            &mut provider,
            &voice,
            AudioSample(1297),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, vec![[0.0; 2]]);
    assert!(provider.calls > 0);
    let calls = provider.calls;
    provider.unavailable = true;
    assert!(
        renderer
            .read_source_voice_occurrence(
                &mut provider,
                &voice,
                AudioSample(1297),
                1,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .is_err()
    );
    assert!(provider.calls > calls);
}

#[test]
fn source_voice_occurrence_processes_only_its_enclosing_preserve_stages() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let rate = document.presentation_basis().frame_rate;
    // A sound on a Retime enters after that node's processor. A sound on its
    // child traverses that processor with its own complete preparation history.
    let cases = [
        (
            "owner",
            3840,
            stretch(&stretch(&raw_voice(3840), 2560, 3, 2), 1280, 2, 1),
        ),
        ("inner", 2560, stretch(&raw_voice(2560), 1280, 2, 1)),
        ("outer", 1280, raw_voice(1280)),
    ];
    for (owner, length, expected) in cases {
        let voice = plan
            .source_voice_occurrence(
                occurrence(owner, 1),
                voice_recipe(rate, length),
                AudioQueryLimits::default(),
            )
            .unwrap();
        assert_eq!(voice.samples(), AudioSample(1297)..AudioSample(2577));
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let mut provider = FixtureProvider::new();
        // Begin cold near the end, then move backward and cross chunk seams.
        for start in [1024, 257, 0, 768, 256, 512] {
            let count = (1280 - start).min(256);
            let block = renderer
                .read_source_voice_occurrence(
                    &mut provider,
                    &voice,
                    AudioSample(1297 + start),
                    count as u32,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                block.samples,
                expected[start as usize..(start + count) as usize],
                "{owner} at {start}"
            );
            assert!(block.suppressed.is_empty());
        }
        assert_eq!(
            renderer.cached_stage_count(),
            0,
            "independent projections must not occupy the Original descriptor cache"
        );
        provider.unavailable = true;
        assert!(
            renderer
                .read_source_voice_occurrence(
                    &mut provider,
                    &voice,
                    AudioSample(1297),
                    1,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .is_err(),
            "prepared history must re-admit its source"
        );
    }
}
