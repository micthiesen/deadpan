//! Independent event returns enter the shared group bus on retained sound clocks.
use super::*;

fn provider() -> FixtureProvider {
    FixtureProvider::from_fixture(
        "pcm-mono-44100.wav",
        AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
    )
}

pub(super) fn fixture(beat: bool, effect: Option<TailEffect>, saturation: bool) -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut picture = source(rate, 48_000, 0..8197);
    picture.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::UNITY, true, vec![], vec![]).unwrap(),
    );
    let NodeKind::Source { source } = &mut picture.kind else {
        unreachable!()
    };
    let audio = audio_at_rate(0, 8197, 44_100);
    source.audio_mapping = SourceAudioMapping::natural_rate(audio.span, rate).unwrap();
    source.audio = Some(audio);
    let base = document_with_asset(
        rate,
        &["owner"],
        [
            (
                "owner",
                BeatNode::sequence("Sound owner", vec![id("picture")]),
            ),
            ("picture", picture),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let source = audio_at_rate(0, 40_000, 44_100);
    let event = SoundEvent {
        owner: id("root"),
        label: "Independent return".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
        source,
        offset: AudioSample(37),
        treatments: deadpan_core::AudioTreatments::default()
            .with_trim(deadpan_core::GainDb::new(-6000).unwrap())
            .unwrap()
            .with_wet_sends(effect.map_or_else(TailSends::default, |effect| {
                TailSends::default().with_level(effect, Some(GainDb::new(-3000).unwrap()))
            })),

        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let mut wire = serde_json::to_value(base).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    if beat {
        let mut event = serde_json::to_value(event).unwrap();
        event.as_object_mut().unwrap().remove("owner");
        wire["beat_sounds"] = serde_json::json!({"owner": {"effect": event}});
    } else {
        wire["sounds"] = serde_json::json!({"effect": event});
    }
    if saturation {
        wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(
            AudioTreatments::default()
                .with_saturation(Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()))
                .unwrap(),
        )
        .unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

pub(super) fn read(document: &ProjectDocument, start: i64, frames: u32) -> Vec<[f32; 2]> {
    let plan = Arc::new(RenderPlan::compile(document).unwrap());
    StageAudio::new(plan)
        .prepare_authored_bus(
            &mut provider(),
            AudioSample(start),
            frames,
            Duration::from_secs(30),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

#[test]
fn root_and_beat_returns_match_finite_history_before_parent_saturation() {
    for beat in [false, true] {
        let dry = read(&fixture(beat, None, false), 0, 48_000);
        assert!(dry[37..43_574].iter().any(|sample| sample[0].abs() > 0.001));
        assert!(dry[43_574..].iter().all(|sample| *sample == [0.0; 2]));
        for effect in [TailEffect::Reverb, TailEffect::Delay] {
            let returned = super::wet_owners::wet(effect, &dry);
            let expected: Vec<_> = dry
                .iter()
                .zip(&returned)
                .enumerate()
                .map(|(at, (dry, wet))| {
                    if !(37..43_574).contains(&at) {
                        return [0.0; 2];
                    }
                    std::array::from_fn(|ch| {
                        (f64::from(dry[ch]) + wet[ch] * 10f64.powf(-3.0 / 20.0)) as f32
                    })
                })
                .collect();
            let document = fixture(beat, Some(effect), false);
            assert!(
                RenderPlan::compile(&document)
                    .unwrap()
                    .has_audio_wet_sends()
            );
            let actual = read(&document, 0, 48_000);
            assert_pcm_close(&actual, &expected);
            assert!(
                actual
                    .iter()
                    .zip(&dry)
                    .any(|(a, b)| (a[0] - b[0]).abs() > 0.001)
            );
            for (at, frames) in [(14_437, 193), (32_767, 257), (43_520, 256), (0, 51)] {
                assert_pcm_close(
                    &read(&document, at, frames),
                    &actual[at as usize..at as usize + frames as usize],
                );
            }
            let saturation = Saturation::new(GainDb::new(9000).unwrap()).unwrap();
            let expected: Vec<_> = expected
                .iter()
                .map(|sample| sample.map(|value| saturation.shape(f64::from(value)) as f32))
                .collect();
            assert_pcm_close(
                &read(&fixture(beat, Some(effect), true), 0, 48_000),
                &expected,
            );
        }
    }
}

#[test]
fn root_route_transports_processed_history_without_advancing_through_inserted_time() {
    let before = fixture(false, Some(TailEffect::Delay), false);
    let original = read(&before, 0, 48_000);
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["picture"]["kind"]["source"]["duration"] = serde_json::json!(49_000);
    wire["sound_routes"] = serde_json::json!({"effect": RootSoundRoute {
        recipe_extent: duration(48_000),
        recipe_grid: RootSoundGrid::root(before.presentation_basis().frame_rate),
        edits: vec![RootSoundEdit {
            grid: RootSoundGrid::root(before.presentation_basis().frame_rate),
            operation: RootSoundOperation::Insert { at: ProjectFrame(20_000), duration: duration(1000) },
            cuts: RootSoundCutEdges { before: AudioEdgePolicy::Hard, after: AudioEdgePolicy::Hard },
        }],
    }});
    let moved = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut expected = original.clone();
    expected.splice(20_000..20_000, vec![[0.0; 2]; 1000]);
    assert_pcm_close(&read(&moved, 0, 49_000), &expected);
    for (at, frames) in [(19_950, 1150), (21_000, 256), (35_390, 257)] {
        assert_pcm_close(
            &read(&moved, at, frames),
            &expected[at as usize..at as usize + frames as usize],
        );
    }
}

#[test]
fn routed_processed_root_preserves_automatic_authored_edge_gains() {
    let mut wire = serde_json::to_value(fixture(false, Some(TailEffect::Delay), false)).unwrap();
    wire["sounds"]["effect"]["start_edge"] = serde_json::json!("automatic");
    wire["sounds"]["effect"]["end_edge"] = serde_json::json!("automatic");
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let original = read(&before, 0, 48_000);

    // This insertion exercises the routed-root gate path while leaving both
    // authored event boundaries in place. Processing must keep each existing
    // edge gain exactly once in the delivered dry signal.
    wire["nodes"]["picture"]["kind"]["source"]["duration"] = serde_json::json!(49_000);
    wire["sound_routes"] = serde_json::json!({"effect": RootSoundRoute {
        recipe_extent: duration(48_000),
        recipe_grid: RootSoundGrid::root(before.presentation_basis().frame_rate),
        edits: vec![RootSoundEdit {
            grid: RootSoundGrid::root(before.presentation_basis().frame_rate),
            operation: RootSoundOperation::Insert { at: ProjectFrame(20_000), duration: duration(1000) },
            cuts: RootSoundCutEdges { before: AudioEdgePolicy::Hard, after: AudioEdgePolicy::Hard },
        }],
    }});
    let moved = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut expected = original.clone();
    expected.splice(20_000..20_000, vec![[0.0; 2]; 1000]);

    assert_pcm_close(&read(&moved, 0, 49_000), &expected);
    // Check both authored fade windows as well as the routed edit boundary.
    for (at, frames) in [(0, 160), (19_950, 1150), (43_450, 256), (44_500, 256)] {
        assert_pcm_close(
            &read(&moved, at, frames),
            &expected[at as usize..at as usize + frames as usize],
        );
    }
}

#[test]
fn processed_root_automatic_edge_is_unchanged_by_a_silent_hold_ending_at_onset() {
    let mut wire = serde_json::to_value(fixture(false, Some(TailEffect::Delay), false)).unwrap();
    wire["sounds"]["effect"]["start_edge"] = serde_json::json!("automatic");
    wire["sounds"]["effect"]["end_edge"] = serde_json::json!("automatic");
    // Retain a valid picture-only Source while removing all Original audio.
    // The fixture's catalog already admits this asset as a still image.
    wire["nodes"]["picture"]["kind"]["source"]["video"] =
        serde_json::to_value(SourceVideo::Still {
            asset: AssetId::new("media").unwrap(),
        })
        .unwrap();
    wire["nodes"]["picture"]["kind"]["source"]["audio"] = serde_json::Value::Null;
    wire["nodes"]["picture"]["kind"]["source"]["audio_mapping"] =
        serde_json::to_value(SourceAudioMapping::FitBeat).unwrap();
    wire["nodes"]["picture"]["kind"]["source"]["audio_offset"] = serde_json::json!(0);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        before.sounds()[&SoundId::new("effect").unwrap()].offset,
        AudioSample(37)
    );
    let expected = read(&before, 0, 48_000);
    assert!(expected[..37].iter().all(|sample| *sample == [0.0; 2]));
    assert!(expected[37..133].iter().any(|sample| sample[0] != 0.0));

    // This current Hold is wholly outside the event's audible support. Its
    // automatic end coincides exactly with the already faded authored onset.
    wire["nodes"]["lead"] = serde_json::to_value(hold(37)).unwrap();
    wire["nodes"]["owner"]["kind"]["children"] = serde_json::json!(["lead", "picture"]);
    wire["nodes"]["picture"]["kind"]["source"]["duration"] = serde_json::json!(47_963);
    let after = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        after.nodes()[&id("lead")].audio_edges.node_end,
        AudioEdgePolicy::Automatic
    );
    assert_eq!(before.sounds(), after.sounds());
    for document in [&before, &after] {
        assert_eq!(
            RenderPlan::compile(document)
                .unwrap()
                .audio_duration()
                .unwrap(),
            AudioSample(48_000)
        );
    }
    assert_pcm_close(&read(&after, 0, 48_000), &expected);
}

#[test]
fn automatic_event_edges_feed_the_effect_and_fade_the_bounded_return() {
    let mut wire = serde_json::to_value(fixture(false, None, false)).unwrap();
    wire["sounds"]["effect"]["start_edge"] = serde_json::json!("automatic");
    wire["sounds"]["effect"]["end_edge"] = serde_json::json!("automatic");
    let dry = read(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        0,
        48_000,
    );
    let wet = super::wet_owners::wet(TailEffect::Reverb, &dry);
    wire["sounds"]["effect"]["treatments"]["wet_sends"] = serde_json::json!({"reverb": -3000});
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let expected: Vec<_> = dry
        .iter()
        .zip(wet)
        .enumerate()
        .map(|(at, (dry, wet))| {
            if !(37..43_574).contains(&at) {
                return [0.0; 2];
            }
            let distance = (at - 37).min(43_573 - at);
            let edge = ((2 * distance + 1) as f64 / 192.0).min(1.0) as f32;
            std::array::from_fn(|ch| {
                (f64::from(dry[ch]) + wet[ch] * 10f64.powf(-3.0 / 20.0) * f64::from(edge)) as f32
            })
        })
        .collect();
    assert_pcm_close(&read(&document, 0, 48_000), &expected);
}

#[test]
fn current_hold_blocks_event_return_until_that_sound_is_explicitly_allowed() {
    let base = fixture(false, Some(TailEffect::Reverb), false);
    let expected = read(&base, 0, 48_000);
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["nodes"]["after"] = wire["nodes"]["picture"].clone();
    wire["nodes"]["picture"]["kind"]["source"]["duration"] = serde_json::json!(20_000);
    wire["nodes"]["after"]["kind"]["source"]["duration"] = serde_json::json!(27_500);
    wire["nodes"]["pause"] = serde_json::to_value(hold(500)).unwrap();
    wire["nodes"]["owner"]["kind"]["children"] = serde_json::json!(["picture", "pause", "after"]);
    let blocked = read(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        0,
        48_000,
    );
    assert!(
        blocked[20_000..20_500]
            .iter()
            .all(|frame| *frame == [0.0; 2])
    );
    assert_pcm_close(&blocked[..19_900], &expected[..19_900]);
    assert_pcm_close(&blocked[20_600..], &expected[20_600..]);
    wire["sound_allowances"] = serde_json::json!({"effect": SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {
        instance: InstancePath {node:id("pause"), repeats:vec![]},
    }]).unwrap()});
    let allowed = read(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        0,
        48_000,
    );
    assert_pcm_close(&allowed, &expected);
    assert!(
        allowed[20_000..20_500]
            .iter()
            .any(|frame| frame[0].abs() > 0.001)
    );
}

#[test]
fn event_return_limits_and_cancellation_preserve_reusability() {
    let mut wire = serde_json::to_value(fixture(false, Some(TailEffect::Delay), false)).unwrap();
    wire["sounds"]["effect"]["treatments"]["wet_sends"]["reverb"] = serde_json::json!(-6000);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    for limits in [
        StageLimits {
            maximum_prepared_stages: 1,
            ..Default::default()
        },
        StageLimits {
            maximum_resident_frames: 1024 * 1024,
            ..Default::default()
        },
        StageLimits {
            maximum_prepared_frames: 1000,
            ..Default::default()
        },
    ] {
        let mut reader = StageAudio::with_limits(Arc::clone(&plan), limits).unwrap();
        for _ in 0..2 {
            assert!(matches!(
                reader.prepare_authored_bus(
                    &mut provider(),
                    AudioSample(20_000),
                    128,
                    TIMEOUT,
                    &AtomicBool::new(false)
                ),
                Err(StageAudioError::Limit(_))
            ));
        }
    }
    let mut reader = StageAudio::new(plan);
    assert!(
        reader
            .prepare_authored_bus(
                &mut provider(),
                AudioSample(20_000),
                128,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    let actual = reader
        .prepare_authored_bus(
            &mut provider(),
            AudioSample(20_000),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_pcm_close(&actual.samples, &read(&document, 20_000, 128));
}

#[test]
fn event_return_enters_the_parent_return_without_self_feedback() {
    let base = fixture(true, Some(TailEffect::Reverb), false);
    let dry = read(&base, 0, 48_000);
    let returned = super::wet_owners::wet(TailEffect::Delay, &dry);
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["root"]["audio_treatments"] =
        serde_json::to_value(AudioTreatments::default().with_wet_sends(
            TailSends::default().with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap())),
        ))
        .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let expected: Vec<_> = dry[32_000..32_256]
        .iter()
        .zip(&returned[32_000..32_256])
        .map(|(dry, wet)| {
            std::array::from_fn(|ch| {
                (f64::from(dry[ch]) + wet[ch] * 10f64.powf(-6.0 / 20.0)) as f32
            })
        })
        .collect();
    assert_pcm_close(&read(&document, 32_000, 256), &expected);
}
