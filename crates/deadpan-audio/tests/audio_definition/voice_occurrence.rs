use super::*;

use deadpan_plan::{AudioQueryLimits, AudioSourceVoiceRecipe};

#[path = "beat_sounds.rs"]
mod beat_sounds;

#[path = "routed_occurrence.rs"]
mod routed_occurrence;

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
                        escalation: None,
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
fn source_voice_occurrences_cross_repeat_seams_and_keep_query_cuts_exact() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let batch = plan
        .source_voice_occurrences(
            &id("owner"),
            voice_recipe(document.presentation_basis().frame_rate, 3840),
            AudioSample(0)..AudioSample(2577),
            Default::default(),
        )
        .unwrap();
    assert_eq!(batch.voices().len(), 2);
    let passage = stretch(&stretch(&raw_voice(3840), 2560, 3, 2), 1280, 2, 1);
    let expected: Vec<_> = vec![[0.0; 2]; 17]
        .into_iter()
        .chain(passage.iter().copied())
        .chain(passage.iter().copied())
        .collect();
    let mut renderer = StageAudio::new(plan.clone());
    let mut provider = FixtureProvider::new();
    for start in [1200, 0, 2321, 1297, 17, 1290] {
        let block = renderer
            .read_source_voice_occurrences(
                &mut provider,
                &batch,
                AudioSample(start),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            block.samples,
            expected[start as usize..start as usize + 256]
        );
        let cut = renderer
            .read_source_voice_occurrences(
                &mut provider,
                &batch,
                AudioSample(start + 51),
                103,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(cut.samples, block.samples[51..154]);
        if start == 0 {
            assert_eq!(block.suppressed, vec![AudioSample(0)..AudioSample(17)]);
        }
    }
}

#[test]
fn source_voice_occurrences_sum_complete_independent_preserve_histories() {
    let mut wire = serde_json::to_value(occurrence_document()).unwrap();
    wire["nodes"].as_object_mut().unwrap().remove("inner");
    wire["nodes"]["repeat"]["kind"]["child"] = serde_json::json!("owner");
    wire["nodes"]["outer"] =
        serde_json::to_value(retime("repeat", 2560, 0..7680, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer"]);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&document, false);
    let batch = plan
        .source_voice_occurrences(
            &id("owner"),
            voice_recipe(document.presentation_basis().frame_rate, 3840),
            AudioSample(17)..AudioSample(2577),
            Default::default(),
        )
        .unwrap();
    assert_eq!(batch.voices().len(), 2);
    assert!(
        batch
            .voices()
            .iter()
            .all(|voice| voice.samples() == (AudioSample(17)..AudioSample(2577)))
    );
    let mut left = raw_voice(3840);
    left.extend(vec![[0.0; 2]; 3840]);
    let mut right = vec![[0.0; 2]; 3840];
    right.extend(raw_voice(3840));
    let left = stretch(&left, 2560, 3, 1);
    let right = stretch(&right, 2560, 3, 1);
    let expected: Vec<[f32; 2]> = left
        .iter()
        .zip(&right)
        .map(|(a, b)| std::array::from_fn(|c| (f64::from(a[c]) + f64::from(b[c])) as f32))
        .collect();
    let mut renderer = StageAudio::new(plan.clone());
    let mut provider = FixtureProvider::new();
    for offset in [2000, 0, 1177, 1280, 256] {
        let block = renderer
            .read_source_voice_occurrences(
                &mut provider,
                &batch,
                AudioSample(17 + offset),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.occurrences.len(), 2);
        assert_eq!(
            block.samples,
            expected[offset as usize..offset as usize + 256]
        );
    }
    // An outer crop hides the first play geometrically. Its already-processed
    // output still belongs to the selected half of the Preserve stage.
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"]["crop"] =
        serde_json::to_value(retime("outer", 1280, 1280..2560, PitchPolicy::FollowSpeed)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "crop"]);
    let cropped = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&cropped, false);
    let batch = plan
        .source_voice_occurrences(
            &id("owner"),
            voice_recipe(cropped.presentation_basis().frame_rate, 3840),
            AudioSample(17)..AudioSample(1297),
            Default::default(),
        )
        .unwrap();
    assert_eq!(batch.voices().len(), 2);
    let mut renderer = StageAudio::new(plan.clone());
    let block = renderer
        .read_source_voice_occurrences(
            &mut provider,
            &batch,
            AudioSample(17),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, expected[1280..1536]);
}

#[test]
fn source_voice_occurrences_empty_window_still_checks_live_source() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let batch = plan
        .source_voice_occurrences(
            &id("owner"),
            voice_recipe(document.presentation_basis().frame_rate, 3840),
            AudioSample(0)..AudioSample(17),
            Default::default(),
        )
        .unwrap();
    assert!(batch.voices().is_empty());
    let mut renderer = StageAudio::new(plan.clone());
    let mut provider = FixtureProvider::new();
    let block = renderer
        .read_source_voice_occurrences(
            &mut provider,
            &batch,
            AudioSample(0),
            17,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, vec![[0.0; 2]; 17]);
    assert_eq!(block.suppressed, vec![AudioSample(0)..AudioSample(17)]);
    assert!(provider.calls > 0);
    provider.unavailable = true;
    assert!(
        renderer
            .read_source_voice_occurrences(
                &mut provider,
                &batch,
                AudioSample(0),
                17,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .is_err()
    );
}

#[test]
fn source_voice_occurrences_reject_invalid_foreign_cancelled_and_overflowing_reads_without_io() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let recipe = voice_recipe(document.presentation_basis().frame_rate, 3840);
    let batch = plan
        .source_voice_occurrences(
            &id("owner"),
            recipe,
            AudioSample(0)..AudioSample(2577),
            Default::default(),
        )
        .unwrap();
    let mut provider = FixtureProvider::new();
    let mut foreign = StageAudio::new(Arc::new(plan.as_ref().clone()));
    assert!(matches!(
        foreign.read_source_voice_occurrences(
            &mut provider,
            &batch,
            AudioSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    let mut renderer = StageAudio::new(plan.clone());
    for (start, frames) in [(-1, 1), (2577, 1), (0, 0), (0, 257), (i64::MAX, 2)] {
        assert!(matches!(
            renderer.read_source_voice_occurrences(
                &mut provider,
                &batch,
                AudioSample(start),
                frames,
                TIMEOUT,
                &AtomicBool::new(false),
            ),
            Err(StageAudioError::Range)
        ));
    }
    assert!(
        renderer
            .read_source_voice_occurrences(
                &mut provider,
                &batch,
                AudioSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(true),
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(provider.calls, 0);
}

#[test]
fn source_voice_occurrences_admit_all_histories_and_residency_before_media() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let batch = plan
        .source_voice_occurrences(
            &id("owner"),
            voice_recipe(document.presentation_basis().frame_rate, 3840),
            AudioSample(0)..AudioSample(2577),
            Default::default(),
        )
        .unwrap();
    for limits in [
        StageLimits {
            maximum_prepared_stages: 3,
            ..Default::default()
        },
        StageLimits {
            maximum_resident_frames: 100,
            ..Default::default()
        },
        StageLimits {
            maximum_resident_frames: 2000,
            ..Default::default()
        },
    ] {
        let mut renderer = StageAudio::with_limits(plan.clone(), limits).unwrap();
        let mut provider = FixtureProvider::new();
        // This crosses two independently prepared, two-stage occurrences. The
        // second history must fail admission before the first can open media.
        assert!(matches!(
            renderer.read_source_voice_occurrences(
                &mut provider,
                &batch,
                AudioSample(1200),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            ),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(provider.calls, 0);
        assert_eq!(renderer.cached_stage_count(), 0);
    }
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
