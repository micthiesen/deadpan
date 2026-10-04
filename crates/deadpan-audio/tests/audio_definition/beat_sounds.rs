//! Saved attachments enter the same canonical bus used by audition and export.
use super::*;
use deadpan_audio::{LimitedAudio, LimitedTile, LimiterContext};
use std::time::Instant;

#[path = "sound_clocks.rs"]
mod sound_clocks;

fn saved(document: &ProjectDocument, owner: &str, length: i64) -> ProjectDocument {
    let recipe = voice_recipe(document.presentation_basis().frame_rate, length);
    let event = BeatSound {
        label: "Owned effect".into(),
        source: recipe.source,
        mapping: recipe.mapping,
        offset: recipe.offset,
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let mut wire = serde_json::to_value(document).unwrap();
    wire["beat_sounds"] = serde_json::json!({owner: {"effect": event}});
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn bus(
    renderer: &mut StageAudio,
    provider: &mut impl AudioSourceProvider,
    start: i64,
    count: u32,
) -> deadpan_audio::EdgeFadedBlock {
    renderer
        .prepare_authored_bus(
            provider,
            AudioSample(start),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
}

fn trim(db: i32, mute: Vec<GainRange>) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(db).unwrap(), false, vec![], mute).unwrap(),
    )
}

fn picture_only(rate: FrameRate, duration: i64) -> BeatNode {
    let mut node = source(rate, duration, 0..128);
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    source.video = SourceVideo::Stream {
        asset: AssetId::new("picture").unwrap(),
        span: audio(0..128).span,
    };
    source.audio = None;
    source.audio_mapping = SourceAudioMapping::FitBeat;
    node
}

#[test]
fn saved_beat_sound_renders_nested_preserve_and_restarts_owner_gain_each_play() {
    let base = saved(&occurrence_document(), "owner", 3840);
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(trim(
        6000,
        vec![GainRange::new(ExactRatio::ZERO, ExactRatio::integer(1920)).unwrap()],
    ))
    .unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(trim(-3000, vec![])).unwrap();
    wire["beat_sounds"]["owner"]["effect"]["gain_millidecibels"] = serde_json::json!(-3000);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&document, false);
    let mut passage = stretch(&stretch(&raw_voice(3840), 2560, 3, 2), 1280, 2, 1);
    // Nominal owner output 0..1920 maps to the first 640 root samples of
    // each independently processed play. Opposing trims cancel exactly.
    passage[..640].fill([0.0; 2]);
    let expected: Vec<_> = vec![[0.0; 2]; 17]
        .into_iter()
        .chain(passage.iter().copied())
        .chain(passage.iter().copied())
        .collect();
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(plan.clone());
    let full = bus(&mut renderer, &mut provider, 0, 2577);
    assert_eq!(full.samples, expected);
    assert_eq!(full.stage, "authored_bus_pcm_before_mastering");
    assert_eq!(full.suppressed, vec![AudioSample(0)..AudioSample(17)]);
    for (start, count) in [(1200, 256), (650, 71), (1297, 37), (1930, 100), (2500, 77)] {
        let mut cold = StageAudio::new(plan.clone());
        assert_eq!(
            bus(&mut cold, &mut provider, start, count).samples,
            expected[start as usize..start as usize + count as usize]
        );
    }
}

#[test]
fn saved_beat_sound_takes_its_repeat_escalation_on_later_plays() {
    let base = saved(&occurrence_document(), "owner", 3840);
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["nodes"]["repeat"]["kind"]["escalation"] = serde_json::json!({"gain_step": 6000});
    let escalated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(compile(&base, false));
    let baseline = bus(&mut renderer, &mut provider, 0, 2577).samples;
    // 17 lead samples, then two plays of 1280 root samples each.
    let gain = 10.0_f64.powf(6000.0 / 20_000.0);
    let expected: Vec<_> = baseline
        .iter()
        .enumerate()
        .map(|(at, sample)| {
            if at < 17 + 1280 {
                *sample
            } else {
                sample.map(|value| (f64::from(value) * gain) as f32)
            }
        })
        .collect();
    let plan = compile(&escalated, false);
    let mut renderer = StageAudio::new(plan.clone());
    assert_eq!(bus(&mut renderer, &mut provider, 0, 2577).samples, expected);
    let mut cold = StageAudio::new(plan);
    assert_eq!(
        bus(&mut cold, &mut provider, 1290, 123).samples,
        expected[1290..1413]
    );
}

#[test]
fn saved_beat_sound_sums_overlapping_preserve_histories_before_one_limiter() {
    let mut wire = serde_json::to_value(occurrence_document()).unwrap();
    wire["nodes"].as_object_mut().unwrap().remove("inner");
    wire["nodes"]["repeat"]["kind"]["child"] = serde_json::json!("owner");
    wire["nodes"]["outer"] =
        serde_json::to_value(retime("repeat", 2560, 0..7680, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer"]);
    let document = saved(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        "owner",
        3840,
    );
    let mut left = raw_voice(3840);
    left.extend(vec![[0.0; 2]; 3840]);
    let mut right = vec![[0.0; 2]; 3840];
    right.extend(raw_voice(3840));
    let left = stretch(&left, 2560, 3, 1);
    let right = stretch(&right, 2560, 3, 1);
    let expected: Vec<_> = vec![[0.0; 2]; 17]
        .into_iter()
        .chain(
            left.iter()
                .zip(&right)
                .map(|(a, b)| std::array::from_fn(|c| (f64::from(a[c]) + f64::from(b[c])) as f32)),
        )
        .collect();
    let plan = compile(&document, false);
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(plan.clone());
    assert_eq!(bus(&mut renderer, &mut provider, 0, 2577).samples, expected);
    let mastered = LimitedTile::prepare(
        LimiterContext {
            project_samples: AudioSample(0)..AudioSample(2577),
            start: AudioSample(0),
            samples: expected,
        },
        AudioSample(0)..AudioSample(2577),
        Instant::now() + TIMEOUT,
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut renderer = LimitedAudio::new(plan);
    for (start, count) in [(0, 2577), (1290, 123), (1, 256)] {
        let actual = renderer
            .read(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            actual.samples,
            mastered.samples[start as usize..start as usize + count as usize]
        );
        assert_eq!(
            actual.gain,
            mastered.gain[start as usize..start as usize + count as usize]
        );
    }
    provider.unavailable = true;
    assert!(
        renderer
            .read(
                &mut provider,
                AudioSample(1290),
                123,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
}

#[test]
fn saved_beat_sound_preflights_all_occurrences_before_media_or_dsp() {
    let document = saved(&occurrence_document(), "owner", 3840);
    let plan = compile(&document, false);
    for limits in [
        StageLimits {
            maximum_prepared_stages: 3,
            ..Default::default()
        },
        StageLimits {
            maximum_resident_frames: 100,
            ..Default::default()
        },
    ] {
        let mut renderer = StageAudio::with_limits(plan.clone(), limits).unwrap();
        let mut provider = FixtureProvider::new();
        assert!(matches!(
            renderer.prepare_authored_bus(
                &mut provider,
                AudioSample(1200),
                256,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(provider.calls, 0);
        assert_eq!(renderer.cached_stage_count(), 0);
    }
}

#[test]
fn saved_beat_sound_with_no_current_occurrence_still_admits_source() {
    let document = saved(&occurrence_document(), "owner", 3840);
    let mut renderer = StageAudio::new(compile(&document, false));
    let mut provider = FixtureProvider::new();
    let empty = bus(&mut renderer, &mut provider, 0, 17);
    assert_eq!(empty.samples, vec![[0.0; 2]; 17]);
    assert_eq!(empty.suppressed, vec![AudioSample(0)..AudioSample(17)]);
    assert!(provider.calls > 0);
    provider.unavailable = true;
    assert!(
        renderer
            .prepare_authored_bus(
                &mut provider,
                AudioSample(0),
                17,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
}

#[test]
fn saved_beat_sound_uses_exact_ntsc_phase_and_independent_sample_offset() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let base = document(
        rate,
        &["lead", "owner"],
        [
            ("lead", hold(1, HoldAudio::Silence)),
            ("owner", picture_only(rate, 4)),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let base = saved(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        "owner",
        4800,
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["beat_sounds"]["owner"]["effect"]["offset"] = serde_json::json!(37);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut expected = sample_reference(
        1024..5824,
        ratio(5122, 5).checked_sub(ratio(37, 1)).unwrap(),
        ExactRatio::ONE,
        6406,
        fixture_sample,
    );
    expected[..37].fill([0.0; 2]);
    expected[4837..].fill([0.0; 2]);
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(compile(&document, false));
    for start in [0, 33, 4790, 6144] {
        assert_eq!(
            bus(&mut renderer, &mut provider, 1602 + start, 256).samples,
            expected[start as usize..start as usize + 256]
        );
    }
}

#[test]
fn saved_beat_sound_edges_use_whole_audible_islands_and_exact_hard_boundaries() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let base = document(
        rate,
        &["owner"],
        [
            (
                "owner",
                BeatNode::sequence("Effect owner", vec![id("left"), id("pause"), id("right")]),
            ),
            ("left", picture_only(rate, 128)),
            ("pause", hold(64, HoldAudio::Silence)),
            ("right", picture_only(rate, 128)),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let base = saved(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        "owner",
        320,
    );
    for hard_start in [false, true] {
        let mut wire = serde_json::to_value(&base).unwrap();
        wire["beat_sounds"]["owner"]["effect"]["start_edge"] =
            serde_json::to_value(AudioEdgePolicy::Automatic).unwrap();
        wire["beat_sounds"]["owner"]["effect"]["end_edge"] =
            serde_json::to_value(AudioEdgePolicy::Automatic).unwrap();
        if hard_start {
            wire["nodes"]["owner"]["audio_edges"] = serde_json::to_value(AudioEdgePolicies {
                node_start: AudioEdgePolicy::Hard,
                ..Default::default()
            })
            .unwrap();
        }
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let expected: Vec<_> = (0..320)
            .map(|at| {
                if (128..192).contains(&at) {
                    return [0.0; 2];
                }
                let local = if at < 128 { at } else { at - 192 };
                let fade_in = if hard_start && at < 128 {
                    1.0
                } else {
                    ((2 * local + 1) as f32 / 128.0).min(1.0)
                };
                let fade_out = ((2 * (127 - local) + 1) as f32 / 128.0).min(1.0);
                fixture_sample(1024 + at).map(|sample| sample * fade_in.min(fade_out))
            })
            .collect();
        let plan = compile(&document, false);
        let mut provider = FixtureProvider::new();
        let mut renderer = StageAudio::new(plan.clone());
        let whole = bus(&mut renderer, &mut provider, 0, 320);
        assert_eq!(whole.samples, expected);
        assert_eq!(whole.suppressed, vec![AudioSample(128)..AudioSample(192)]);
        for (start, count) in [(120, 81), (0, 7), (193, 19), (270, 50)] {
            let mut cold = StageAudio::new(plan.clone());
            assert_eq!(
                bus(&mut cold, &mut provider, start, count).samples,
                expected[start as usize..start as usize + count as usize]
            );
        }
    }
}

#[test]
fn saved_beat_sound_on_gap_override_inherits_current_repeat_gap_edges() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plays = IterationOrder::new(RevisionId::new("gap-plays").unwrap(), 2).unwrap();
    let base = document(
        rate,
        &["repeat"],
        [
            ("owner", picture_only(rate, 128)),
            (
                "repeat",
                BeatNode {
                    kind: NodeKind::Repeat {
                        child: id("owner"),
                        iterations: plays.clone(),
                        gap: None,
                        escalation: None,
                    },
                    ..BeatNode::sequence("Repeat", vec![])
                },
            ),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["gap"] = wire["nodes"]["owner"].clone();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: plays.at(0).unwrap(),
            root: id("gap"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let base = saved(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        "gap",
        128,
    );
    for hard_start in [false, true] {
        let mut wire = serde_json::to_value(&base).unwrap();
        wire["beat_sounds"]["gap"]["effect"]["start_edge"] =
            serde_json::to_value(AudioEdgePolicy::Automatic).unwrap();
        wire["beat_sounds"]["gap"]["effect"]["end_edge"] =
            serde_json::to_value(AudioEdgePolicy::Automatic).unwrap();
        wire["nodes"]["repeat"]["audio_edges"] = serde_json::to_value(AudioEdgePolicies {
            repeat_gap_start: if hard_start {
                AudioEdgePolicy::Hard
            } else {
                AudioEdgePolicy::Automatic
            },
            repeat_gap_end: if hard_start {
                AudioEdgePolicy::Automatic
            } else {
                AudioEdgePolicy::Hard
            },
            ..Default::default()
        })
        .unwrap();
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let expected: Vec<_> = (0..128)
            .map(|at| {
                let distance = if hard_start { 127 - at } else { at };
                let gain = ((2 * distance + 1) as f32 / 128.0).min(1.0);
                fixture_sample(1024 + at).map(|sample| sample * gain)
            })
            .collect();
        let mut provider = FixtureProvider::new();
        let mut renderer = StageAudio::new(compile(&document, false));
        assert_eq!(
            bus(&mut renderer, &mut provider, 128, 128).samples,
            expected
        );
        assert_eq!(
            bus(&mut renderer, &mut provider, 129, 17).samples,
            expected[1..18]
        );
        assert_eq!(
            bus(&mut renderer, &mut provider, 230, 26).samples,
            expected[102..]
        );
    }
}
