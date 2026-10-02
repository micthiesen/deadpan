//! Window peaks are reduced from full-context authored PCM, never cropped plans.
use super::*;
use deadpan_audio::{
    EditWaveform, EditWaveformStage, WaveformCompletion, WaveformControl, WaveformLimits,
    WaveformMemory, WaveformStopReason,
};

fn measured(
    document: &ProjectDocument,
    start: i64,
    end: i64,
    provider: &mut FixtureProvider,
) -> Arc<EditWaveform> {
    let result = StageAudio::new(compile(document, false))
        .measure_edit_window(
            provider,
            AudioSample(start)..AudioSample(end),
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &AtomicBool::new(false),
                memory: &WaveformMemory::default(),
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(result.completion, WaveformCompletion::Complete);
    assert_eq!(result.waveform.measured_end(), AudioSample(end));
    if end > start {
        let leaves = result.waveform.level(0).unwrap();
        assert!(!leaves.is_empty());
        assert_eq!(
            result.waveform.bin_samples(0, 0).unwrap().start,
            AudioSample(start)
        );
        assert_eq!(
            result
                .waveform
                .bin_samples(0, leaves.len() - 1)
                .unwrap()
                .end,
            AudioSample(end)
        );
    }
    assert_eq!(result.examined_samples, u64::try_from(end - start).unwrap());
    assert_eq!(
        result.waveform.descriptor().samples,
        AudioSample(start)..AudioSample(end)
    );
    assert_eq!(
        result.waveform.descriptor().stage,
        EditWaveformStage::AuthoredBusBeforeLimiter
    );
    result.waveform
}

fn extrema(waveform: &EditWaveform, origin: i64, expected: &[[f32; 2]]) {
    for level in 0..waveform.level_count() {
        for (index, peak) in waveform.level(level).unwrap().iter().enumerate() {
            let range = waveform.bin_samples(level, index).unwrap();
            let values = &expected[usize::try_from(range.start.0 - origin).unwrap()
                ..usize::try_from(range.end.0 - origin).unwrap()];
            for channel in 0..2 {
                assert_eq!(
                    peak.minimum()[channel].to_bits(),
                    values
                        .iter()
                        .map(|v| v[channel])
                        .min_by(f32::total_cmp)
                        .unwrap()
                        .to_bits()
                );
                assert_eq!(
                    peak.maximum()[channel].to_bits(),
                    values
                        .iter()
                        .map(|v| v[channel])
                        .max_by(f32::total_cmp)
                        .unwrap()
                        .to_bits()
                );
            }
        }
    }
}

fn gain(db: i32) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(db).unwrap(), false, vec![], vec![]).unwrap(),
    )
}

#[test]
fn exact_ntsc_window_retains_fractional_phase_offset_ancestor_gain_and_filter_context() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut src = source(rate, 5, 0..8197);
    src.audio_edges.node_start = AudioEdgePolicy::Hard;
    src.audio_edges.node_end = AudioEdgePolicy::Hard;
    let NodeKind::Source { source } = &mut src.kind else {
        unreachable!()
    };
    source.audio_offset = AudioSample(7);
    let mut group = BeatNode::sequence("Group", vec![id("a")]);
    group.audio_treatments = gain(3000);
    let doc = document(
        rate,
        &["prefix", "group"],
        [
            ("prefix", hold(1, HoldAudio::Silence)),
            ("group", group),
            ("a", src),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(&doc).unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(gain(-6000)).unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    // Absolute 2115 minus one exact NTSC frame (1601.6) and offset 7:
    // source 506.4. Full 0..8197 support remains behind the display window.
    let raw = sample_reference(
        0..8197,
        ratio(2532, 5),
        ExactRatio::ONE,
        513,
        fixture_sample,
    );
    let expected: Vec<_> = raw
        .iter()
        .map(|sample| sample.map(|v| (f64::from(v) * 10_f64.powf(-3.0 / 20.0)) as f32))
        .collect();
    let wrong = sample_reference(
        0..8197,
        ratio(2537, 5),
        ExactRatio::ONE,
        513,
        fixture_sample,
    );
    assert_ne!(raw, wrong);
    let mut provider = FixtureProvider::new();
    extrema(&measured(&doc, 2115, 2628, &mut provider), 2115, &expected);
    // A shorter request must not acquire fresh filter bounds or edge fades.
    extrema(
        &measured(&doc, 2215, 2471, &mut provider),
        2215,
        &expected[100..356],
    );
    assert!(provider.calls > 0);
}

#[test]
fn root_routes_and_silent_hold_permissions_change_actual_window_peaks() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut pause = hold(1020, HoldAudio::Silence);
    pause.audio_edges.node_start = AudioEdgePolicy::Hard;
    pause.audio_edges.node_end = AudioEdgePolicy::Hard;
    let doc = document(rate, &["pause"], [("pause", pause)], BTreeMap::new());
    let event = SoundEvent {
        owner: id("root"),
        label: "Routed sound".into(),
        source: audio(0..8197),
        mapping: SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: ratio(8197, 1),
            selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(1000, 1)).unwrap(),
        },
        offset: AudioSample(0),
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let mut wire = serde_json::to_value(&doc).unwrap();
    // Root sounds require the qualified audio asset shape. This synthetic
    // provider supplies the PCM; no store/media admission is claimed here.
    wire["assets"]["media"]["still_image"] = serde_json::json!(false);
    wire["assets"]["media"]["source_qualification"] =
        serde_json::json!(SourceQualificationId::new("b".repeat(64)).unwrap());
    wire["sounds"] = serde_json::json!({"sound":event});
    wire["sound_routes"] = serde_json::json!({"sound":RootSoundRoute {recipe_extent:frames(1000),recipe_grid:RootSoundGrid::root(rate),edits:vec![RootSoundEdit {grid:RootSoundGrid::root(rate),operation:RootSoundOperation::Insert {at:ProjectFrame(300),duration:frames(20)},cuts:RootSoundCutEdges {before:AudioEdgePolicy::Hard,after:AudioEdgePolicy::Hard}}]}});
    let silent = ProjectDocument::from_json(&wire.to_string()).unwrap();
    extrema(
        &measured(&silent, 280, 793, &mut FixtureProvider::new()),
        280,
        &[[0.0; 2]; 513],
    );
    wire["sound_allowances"] = serde_json::json!({"sound":SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {instance:InstancePath {node:id("pause"),repeats:vec![]}}]).unwrap()});
    let allowed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut expected =
        sample_reference(0..1000, ratio(280, 1), ExactRatio::ONE, 20, fixture_sample);
    expected.extend([[0.0; 2]; 20]);
    expected.extend(sample_reference(
        0..1000,
        ratio(300, 1),
        ExactRatio::ONE,
        473,
        fixture_sample,
    ));
    assert!(expected.iter().flatten().any(|v| *v != 0.0));
    extrema(
        &measured(&allowed, 280, 793, &mut FixtureProvider::new()),
        280,
        &expected,
    );
}

#[test]
fn partial_limit_and_cancellation_publish_only_complete_window_bins() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["a"],
        [("a", source(rate, 2000, 0..8197))],
        BTreeMap::new(),
    );
    let mut audio = StageAudio::new(compile(&doc, false));
    let memory = WaveformMemory::default();
    let active = AtomicBool::new(false);
    let result = audio
        .measure_edit_window(
            &mut FixtureProvider::new(),
            AudioSample(100)..AudioSample(869),
            WaveformControl {
                limits: WaveformLimits::new(513, 4096, TIMEOUT).unwrap(),
                cancelled: &active,
                memory: &memory,
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(
        result.completion,
        WaveformCompletion::Partial(WaveformStopReason::OutputLimit)
    );
    assert_eq!(result.examined_samples, 513);
    assert_eq!(result.waveform.measured_end(), AudioSample(612));
    assert_eq!(result.waveform.level(0).unwrap().len(), 2);
    let stopped = AtomicBool::new(false);
    let result = audio
        .measure_edit_window(
            &mut FixtureProvider::new(),
            AudioSample(100)..AudioSample(869),
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &stopped,
                memory: &memory,
            },
            |_| stopped.store(true, Ordering::Release),
        )
        .unwrap();
    assert_eq!(
        result.completion,
        WaveformCompletion::Partial(WaveformStopReason::Cancelled)
    );
    assert_eq!(result.examined_samples, 256);
    assert_eq!(result.waveform.measured_end(), AudioSample(356));
    let mut unavailable = FixtureProvider::new();
    unavailable.unavailable = true;
    let failed = audio
        .measure_edit_window(
            &mut unavailable,
            AudioSample(100)..AudioSample(869),
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &active,
                memory: &memory,
            },
            |_| {},
        )
        .unwrap();
    assert!(matches!(
        failed.completion,
        WaveformCompletion::Partial(WaveformStopReason::Preparation(_))
    ));
    assert_eq!(failed.waveform.measured_end(), AudioSample(100));
    assert!(failed.waveform.level(0).unwrap().is_empty());
}

#[test]
fn late_window_keeps_complete_nested_preserve_histories_and_room_tone() {
    let (document, independent_outer, _) = nested_fixture();
    // Hard authored edges make the existing independent two-stage DSP oracle
    // visible unchanged. They do not alter either stage's full input history.
    let mut wire = serde_json::to_value(&document).unwrap();
    for value in wire["nodes"].as_object_mut().unwrap().values_mut() {
        let mut beat: BeatNode = serde_json::from_value(value.clone()).unwrap();
        beat.audio_edges.node_start = AudioEdgePolicy::Hard;
        beat.audio_edges.node_end = AudioEdgePolicy::Hard;
        if matches!(beat.kind, NodeKind::Source { .. }) {
            beat.audio_edges.source_placement_start = AudioEdgePolicy::Hard;
            beat.audio_edges.source_placement_end = AudioEdgePolicy::Hard;
        }
        *value = serde_json::to_value(beat).unwrap();
    }
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    // The root prefix occupies 64 samples, then crop [1000,1256) of the outer
    // 1280-sample stage. Window [113,310) therefore selects [1049,1246), with
    // both Preserve stages and RoomTone prepared from their original starts.
    let expected = &independent_outer[1049..1246];
    assert!(expected.iter().flatten().any(|sample| sample.abs() > 1e-6));
    let mut provider = FixtureProvider::new();
    extrema(&measured(&document, 113, 310, &mut provider), 113, expected);
    assert!(provider.calls > 0);
    // A fresh renderer's overlapping request must keep that same DSP history.
    extrema(
        &measured(&document, 151, 256, &mut provider),
        151,
        &independent_outer[1087..1192],
    );
}
