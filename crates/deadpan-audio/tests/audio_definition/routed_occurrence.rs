//! Retained occurrence routes preserve complete processing and old sample labels.
use super::*;
use deadpan_plan::{AudioBoundaryRule, AudioRoutedRoot, AudioSampleGrid, AudioSoundRoute};

fn routed<'plan>(
    voice: deadpan_plan::AudioSourceOccurrence<'plan>,
    route: SoundRoute,
    origins: &[ExactRatio],
) -> AudioRoutedRoot<'plan> {
    let rate = voice.plan().metadata().presentation_basis.frame_rate;
    let step = ratio(
        i128::from(rate.numerator()),
        48_000 * i128::from(rate.denominator()),
    );
    AudioRoutedRoot::occurrence(
        voice,
        AudioSoundRoute::<AudioSample>::new(
            route,
            origins
                .iter()
                .map(|origin| {
                    AudioSampleGrid::new(*origin, step, AudioBoundaryRule::RoundEven).unwrap()
                })
                .collect(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn read(
    renderer: &mut StageAudio,
    provider: &mut FixtureProvider,
    route: &AudioRoutedRoot<'_>,
    start: i64,
    frames: u32,
) -> deadpan_audio::RoutedRootBlock {
    renderer
        .read_routed_root(
            provider,
            route,
            AudioSample(start),
            frames,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
}

fn keep(extent: i64) -> SoundRippleMap {
    SoundRippleMap::new(
        ExactRatio::integer(extent),
        0,
        vec![SoundRippleNode::Keep {
            range: ExactFrameRange {
                start: ExactRatio::ZERO,
                end: ExactRatio::integer(extent),
            },
        }],
    )
    .unwrap()
}

fn insert(extent: i64, at: i64, duration: i64) -> SoundRippleMap {
    SoundRippleMap::new(
        ExactRatio::integer(extent),
        3,
        vec![
            SoundRippleNode::Keep {
                range: ExactFrameRange {
                    start: ExactRatio::ZERO,
                    end: ExactRatio::integer(at),
                },
            },
            SoundRippleNode::Gap {
                duration: ExactRatio::integer(duration),
            },
            SoundRippleNode::Keep {
                range: ExactFrameRange {
                    start: ExactRatio::integer(at),
                    end: ExactRatio::integer(extent),
                },
            },
            SoundRippleNode::Sequence {
                parts: vec![0, 1, 2],
            },
        ],
    )
    .unwrap()
}

#[test]
fn routed_occurrence_keeps_nested_preserve_history_across_edits_and_shuffled_reads() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let passage = stretch(&stretch(&raw_voice(3840), 2560, 3, 2), 1280, 2, 1);
    for play in [0, 1] {
        let voice = plan
            .source_voice_occurrence(
                occurrence("owner", play),
                voice_recipe(document.presentation_basis().frame_rate, 3840),
                Default::default(),
            )
            .unwrap();
        let start = voice.samples().start.0;
        let origin = ExactRatio::integer(-start);
        let route = routed(
            voice,
            SoundRoute::identity(ExactRatio::integer(1280))
                .unwrap()
                .ripple(insert(1280, 31, 7))
                .unwrap()
                .ripple(insert(1287, 113, 5))
                .unwrap(),
            &[origin; 3],
        );
        let mut expected = passage.clone();
        expected.splice(31..31, [[0.0; 2]; 7]);
        expected.splice(113..113, [[0.0; 2]; 5]);
        let mut provider = FixtureProvider::new();
        let mut renderer = StageAudio::new(plan.clone());
        let mut suppressed = Vec::new();
        for offset in (0..expected.len()).step_by(256) {
            let count = (expected.len() - offset).min(256);
            let block = read(
                &mut renderer,
                &mut provider,
                &route,
                start + offset as i64,
                count as u32,
            );
            assert_eq!(block.samples, expected[offset..offset + count]);
            suppressed.extend(block.suppressed);
        }
        assert_eq!(
            suppressed,
            vec![
                AudioSample(start + 31)..AudioSample(start + 38),
                AudioSample(start + 113)..AudioSample(start + 118)
            ]
        );
        for (offset, count) in [(1200, 92), (20, 128), (100, 61), (0, 11)] {
            let mut cold = StageAudio::new(plan.clone());
            assert_eq!(
                read(&mut cold, &mut provider, &route, start + offset, count).samples,
                expected[offset as usize..offset as usize + count as usize]
            );
        }
    }
}

#[test]
fn routed_occurrence_ntsc_transport_keeps_phase_and_cannot_revive_clipped_terminal_samples() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let base = document(
        rate,
        &["lead", "owner"],
        [
            ("lead", hold(2, HoldAudio::Silence)),
            ("owner", source(rate, 4, 0..6406)),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&document, false);
    // Retain natural-rate source support through the last root sample while
    // selecting exactly the owner's four-frame extent.
    let mut recipe = voice_recipe(rate, 6408);
    recipe.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: recipe.mapping.duration_frames(FrameDuration::ZERO).unwrap(),
        selection: ExactFrameRange {
            start: ExactRatio::ZERO,
            end: ExactRatio::integer(4),
        },
    };
    let voice = plan
        .source_voice_occurrence(
            InstancePath {
                node: id("owner"),
                repeats: vec![],
            },
            recipe,
            Default::default(),
        )
        .unwrap();
    assert_eq!(voice.samples(), AudioSample(3203)..AudioSample(9610));
    let mut provider = FixtureProvider::new();
    let mut original_reader = StageAudio::new(plan.clone());
    let mut original = Vec::new();
    for start in (3203..9610).step_by(256) {
        original.extend(
            original_reader
                .read_source_voice_occurrence(
                    &mut provider,
                    &voice,
                    AudioSample(start),
                    (9610 - start).min(256) as u32,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    let route = SoundRoute::identity(ExactRatio::integer(4))
        .unwrap()
        .ripple(keep(4))
        .unwrap()
        .ripple(keep(4))
        .unwrap();
    // Move the entire owner one frame right and back. The intermediate
    // allocation has 6406 samples, so its lost last sample stays lost.
    let route = routed(
        voice,
        route,
        &[
            ExactRatio::integer(-2),
            ExactRatio::integer(-3),
            ExactRatio::integer(-2),
        ],
    );
    assert_eq!(route.samples(), AudioSample(3203)..AudioSample(9610));
    let mut expected = original;
    assert_ne!(expected[6406], [0.0; 2]);
    expected[6406] = [0.0; 2];
    let mut renderer = StageAudio::new(plan.clone());
    let mut suppressed = Vec::new();
    for offset in (0..expected.len()).step_by(256) {
        let count = (expected.len() - offset).min(256);
        let actual = read(
            &mut renderer,
            &mut provider,
            &route,
            3203 + offset as i64,
            count as u32,
        );
        assert_eq!(actual.samples, expected[offset..offset + count]);
        suppressed.extend(actual.suppressed);
    }
    assert_eq!(suppressed, vec![AudioSample(9609)..AudioSample(9610)]);
    for (offset, count) in [(6300, 107), (0, 256), (1590, 37), (3200, 201)] {
        let mut cold = StageAudio::new(plan.clone());
        assert_eq!(
            read(&mut cold, &mut provider, &route, 3203 + offset, count).samples,
            expected[offset as usize..offset as usize + count as usize]
        );
    }
}

#[test]
fn routed_occurrence_retains_raw_sound_before_consuming_hold_gates_and_gain() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let base = document(
        rate,
        &["owner"],
        [("owner", hold(256, HoldAudio::Silence))],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] =
        serde_json::to_value(AudioTreatments::from_clip_gain(
            ClipGain::new(GainDb::new(6000).unwrap(), true, vec![], vec![]).unwrap(),
        ))
        .unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&document, false);
    let voice = plan
        .source_voice_occurrence(
            InstancePath {
                node: id("owner"),
                repeats: vec![],
            },
            voice_recipe(rate, 256),
            Default::default(),
        )
        .unwrap();
    let mut provider = FixtureProvider::new();
    let gated = StageAudio::new(plan.clone())
        .read_source_voice_occurrence(
            &mut provider,
            &voice,
            AudioSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(gated.samples, vec![[0.0; 2]; 256]);
    let route = routed(
        voice,
        SoundRoute::identity(ExactRatio::integer(256)).unwrap(),
        &[ExactRatio::ZERO],
    );
    let actual = read(
        &mut StageAudio::new(plan.clone()),
        &mut provider,
        &route,
        0,
        256,
    );
    assert_eq!(actual.samples, raw_voice(256));
    assert!(actual.suppressed.is_empty());
}

#[test]
fn routed_occurrence_gap_admits_complete_processing_and_live_source() {
    let document = occurrence_document();
    let plan = compile(&document, false);
    let voice = plan
        .source_voice_occurrence(
            occurrence("owner", 0),
            voice_recipe(document.presentation_basis().frame_rate, 3840),
            Default::default(),
        )
        .unwrap();
    let gap = SoundRoute::identity(ExactRatio::integer(1280))
        .unwrap()
        .ripple(
            SoundRippleMap::new(
                ExactRatio::integer(1280),
                0,
                vec![SoundRippleNode::Gap {
                    duration: ExactRatio::integer(1280),
                }],
            )
            .unwrap(),
        )
        .unwrap();
    let route = routed(voice, gap, &[ExactRatio::integer(-17); 2]);
    for limits in [
        StageLimits {
            maximum_depth: 1,
            ..Default::default()
        },
        StageLimits {
            maximum_prepared_stages: 1,
            ..Default::default()
        },
        StageLimits {
            maximum_input_frames: 100,
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
            renderer.read_routed_root(
                &mut provider,
                &route,
                AudioSample(20),
                32,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(provider.calls, 0);
        assert_eq!(renderer.cached_stage_count(), 0);
    }
    let mut renderer = StageAudio::new(plan.clone());
    let mut provider = FixtureProvider::new();
    for unavailable in [true, false, true, false] {
        provider.unavailable = unavailable;
        let result = renderer.read_routed_root(
            &mut provider,
            &route,
            AudioSample(20),
            32,
            TIMEOUT,
            &AtomicBool::new(false),
        );
        if unavailable {
            assert!(result.is_err());
        } else {
            let result = result.unwrap();
            assert_eq!(result.samples, vec![[0.0; 2]; 32]);
            assert_eq!(result.suppressed, vec![AudioSample(20)..AudioSample(52)]);
        }
    }
}
