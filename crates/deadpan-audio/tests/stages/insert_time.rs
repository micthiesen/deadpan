//! Command-level PCM oracles share decoding helpers with the stage suite, but
//! derive cut/resume coordinates directly from the project frame clock.
use super::*;

fn insert_pause(document: &ProjectDocument, at: i64, frames: i64, name: &str) -> ProjectDocument {
    let revision = RevisionId::new(name).unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision.clone(),
        command: Command::InsertTime {
            at: ProjectFrame(at),
            hold: HoldRecipe {
                duration: duration(frames),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: id(&format!("{name}-pause")),
            identities: SplitIdentities {
                nodes: (0..document.nodes().len() + 4)
                    .map(|index| id(&format!("{name}-split-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision,
                ordinal: 0,
            },
        },
    };
    let before = document.clone();
    let transaction = apply(document, &request).unwrap();
    assert_eq!(transaction.duration_delta, frames);
    assert_eq!(*document, before);
    let changed = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&changed).unwrap(), before);
    changed
}

fn mono_original(frames: i64) -> (ProjectDocument, FixtureProvider) {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let selected = audio_at_rate(100, 20_100, 44_100);
    let mut leaf = source(rate, frames, 0..1);
    let NodeKind::Source { source } = &mut leaf.kind else {
        unreachable!()
    };
    source.audio_mapping = SourceAudioMapping::natural_rate(selected.span, rate).unwrap();
    source.audio = Some(selected);
    let document = document_with_asset(
        rate,
        &["source"],
        [("source", leaf)],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let provider = FixtureProvider::from_fixture(
        "pcm-mono-44100.wav",
        AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
    );
    (document, provider)
}

fn ntsc_boundary(frame: i64) -> usize {
    let samples = ratio(i128::from(frame) * 8008, 5).round_even().unwrap();
    usize::try_from(samples).unwrap()
}

#[derive(Debug, PartialEq)]
struct Recorded {
    samples: Vec<[f32; 2]>,
    suppressed: Vec<bool>,
}

fn record(
    document: &ProjectDocument,
    provider: &mut FixtureProvider,
    faded: bool,
    pieces: &[u32],
) -> Recorded {
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let total = renderer.plan().audio_duration().unwrap().0 as usize;
    let mut result = Recorded {
        samples: Vec::with_capacity(total),
        suppressed: vec![false; total],
    };
    for requested in pieces.iter().copied().cycle() {
        if result.samples.len() == total {
            break;
        }
        let start = result.samples.len();
        let count = requested.min((total - start) as u32);
        let (samples, suppressed) = if faded {
            let block = renderer
                .read_edge_faded(
                    provider,
                    AudioSample(start as i64),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            (block.samples, block.suppressed)
        } else {
            let block = renderer
                .read(
                    provider,
                    AudioSample(start as i64),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            (block.samples, block.suppressed)
        };
        result.samples.extend(samples);
        for range in suppressed {
            result.suppressed[range.start.0 as usize..range.end.0 as usize].fill(true);
        }
    }
    result
}

fn assert_pause(recorded: &Recorded, samples: Range<usize>) {
    assert!(
        recorded.samples[samples.clone()]
            .iter()
            .all(|v| *v == [0.; 2])
    );
    assert!(recorded.suppressed[samples].iter().all(|v| *v));
}

fn assert_resumed(actual: &Recorded, old: &Recorded, new_samples: Range<usize>, old_start: usize) {
    for (offset, new_sample) in new_samples.enumerate() {
        let old_sample = old_start + offset;
        assert_eq!(
            actual.samples[new_sample],
            old.samples.get(old_sample).copied().unwrap_or([0.; 2]),
            "new sample {new_sample} must resume old sample {old_sample}"
        );
        assert_eq!(
            actual.suppressed[new_sample],
            old.suppressed.get(old_sample).copied().unwrap_or(true),
            "suppression at new {new_sample}, old {old_sample}"
        );
    }
}

#[test]
fn ntsc_44100_pause_at_an_existing_split_resumes_1602_and_exhausts_4804() {
    let (original, mut provider) = mono_original(2);
    let divided = split_command(&original, &id("source"), 1, "existing-cut");
    let inserted = insert_pause(&divided, 1, 1, "existing-seam-pause");
    let direct = insert_pause(&original, 1, 1, "interior-pause");
    // Original sample 1602 maps to source 100 + 1602 * 44100/48000.
    // This oracle uses the decoded WAV and canonical resampler, independently
    // of either the current plan or its retained placement/resume descriptors.
    let expected_resume = provider
        .source
        .prepare(
            ResampleRecipe::new(
                100..20_100,
                ratio(125_747, 80),
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(128),
            )
            .unwrap(),
            AudioSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    for faded in [false, true] {
        let old = record(&original, &mut provider, faded, &[256]);
        let before = record(&divided, &mut provider, faded, &[199, 7]);
        assert_eq!(before, old, "pure Split must not change the oracle");
        let actual = record(&inserted, &mut provider, faded, &[127, 3, 251]);
        assert_eq!(actual.samples.len(), 4805);
        assert_eq!(actual.samples[..1602], old.samples[..1602]);
        assert_eq!(actual.suppressed[..1602], old.suppressed[..1602]);
        assert_pause(&actual, 1602..3203);
        assert_resumed(&actual, &old, 3203..4805, 1602);
        if !faded {
            assert_eq!(actual.samples[3203..3331], expected_resume);
        }
        assert_eq!(actual.samples[4804], [0.; 2]);
        assert!(actual.suppressed[4804]);
        assert_eq!(record(&direct, &mut provider, faded, &[13, 256]), actual);
        assert_eq!(record(&inserted, &mut provider, faded, &[1, 239]), actual);
    }
}

#[test]
fn inserting_before_several_split_fragments_reanchors_every_surviving_entry() {
    let (original, mut provider) = mono_original(5);
    let mut divided = split_command(&original, &id("source"), 1, "split-first");
    for (name, local) in [("split-second", 1), ("split-fourth", 2)] {
        let NodeKind::Sequence { children } = &divided.nodes()[divided.root()].kind else {
            unreachable!()
        };
        let target = children.last().unwrap().clone();
        divided = split_command(&divided, &target, local, name);
    }
    let inserted = insert_pause(&divided, 1, 1, "before-fragments");
    for faded in [false, true] {
        let old = record(&original, &mut provider, faded, &[256]);
        assert_eq!(record(&divided, &mut provider, faded, &[127]), old);
        let actual = record(&inserted, &mut provider, faded, &[251, 17]);
        assert_resumed(&actual, &old, 0..ntsc_boundary(1), 0);
        assert_pause(&actual, ntsc_boundary(1)..ntsc_boundary(2));
        for (start, end) in [(1, 2), (2, 4), (4, 5)] {
            assert_resumed(
                &actual,
                &old,
                ntsc_boundary(start + 1)..ntsc_boundary(end + 1),
                ntsc_boundary(start),
            );
        }
        assert_eq!(record(&inserted, &mut provider, faded, &[5, 256]), actual);
    }
}

#[test]
fn a_second_interior_pause_composes_the_current_resume_without_changing_its_prefix() {
    let (original, mut provider) = mono_original(6);
    let first = insert_pause(&original, 2, 1, "first-pause");
    let second = insert_pause(&first, 4, 2, "second-pause");
    for faded in [false, true] {
        let before = record(&first, &mut provider, faded, &[197, 256]);
        let actual = record(&second, &mut provider, faded, &[101, 13]);
        assert_resumed(&actual, &before, 0..ntsc_boundary(4), 0);
        assert_pause(&actual, ntsc_boundary(4)..ntsc_boundary(6));
        assert_resumed(
            &actual,
            &before,
            ntsc_boundary(6)..ntsc_boundary(9),
            ntsc_boundary(4),
        );
        assert_eq!(record(&second, &mut provider, faded, &[256, 1]), actual);
    }
}

#[test]
fn start_and_end_pauses_retain_hard_source_edges_room_tone_phase_and_silence() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut first = source(rate, 128, 0..128);
    first.audio_edges.node_start = AudioEdgePolicy::Hard;
    first.audio_edges.node_end = AudioEdgePolicy::Hard;
    let original = document_with_asset(
        rate,
        &["a", "room", "quiet", "b"],
        [
            ("a", first),
            ("room", room_tone(128, audio(512, 641))),
            ("quiet", hold(32)),
            ("b", source(rate, 128, 1024..1152)),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let start = insert_pause(&original, 0, 17, "start-pause");
    let end = insert_pause(&start, 433, 13, "end-pause");
    let mut provider = FixtureProvider::new();
    for faded in [false, true] {
        let old = record(&original, &mut provider, faded, &[256]);
        let shifted = record(&start, &mut provider, faded, &[29, 199]);
        assert_pause(&shifted, 0..17);
        assert_resumed(&shifted, &old, 17..433, 0);
        let appended = record(&end, &mut provider, faded, &[37]);
        assert_resumed(&appended, &shifted, 0..433, 0);
        assert_pause(&appended, 433..446);
    }
}

#[test]
fn ntsc_pause_reanchors_room_tone_from_its_old_visible_entry() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let original = document_with_asset(
        rate,
        &["quiet", "room"],
        [("quiet", hold(1)), ("room", room_tone(2, audio(512, 641)))],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let inserted = insert_pause(&original, 1, 1, "room-pause");
    let mut provider = FixtureProvider::new();
    for faded in [false, true] {
        let old = record(&original, &mut provider, faded, &[256]);
        let actual = record(&inserted, &mut provider, faded, &[31, 223]);
        assert_resumed(&actual, &old, 0..1602, 0);
        assert_pause(&actual, 1602..3203);
        assert_resumed(&actual, &old, 3203..6406, 1602);
        assert_eq!(record(&inserted, &mut provider, faded, &[251, 7]), actual);
    }
}

#[test]
fn extending_moved_room_tone_retains_its_binding_and_uses_the_current_duration() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let original = document_with_asset(
        rate,
        &["quiet", "room"],
        [("quiet", hold(1)), ("room", room_tone(2, audio(512, 641)))],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let inserted = insert_pause(&original, 1, 1, "room-pause");
    let extend = |document: &ProjectDocument, name: &str| {
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(name).unwrap(),
            command: Command::SetHoldDuration {
                node: id("room"),
                duration: duration(3),
            },
        };
        let edit = apply(document, &request).unwrap();
        let result = edit.forward.apply(document).unwrap();
        assert_eq!(edit.inverse.apply(&result).unwrap(), *document);
        result
    };
    let extended = extend(&inserted, "extend-moved-room");
    let reference = extend(&original, "extend-original-room");
    // The binding is a sampling clock, not a frozen copy of the old recipe.
    // Extending a Hold must not drop, recapture or mutate its historical clock.
    assert_eq!(extended.audio_bindings(), inserted.audio_bindings());
    let mut provider = FixtureProvider::new();
    for faded in [false, true] {
        let before = record(&inserted, &mut provider, faded, &[256]);
        let old_clock = record(&reference, &mut provider, faded, &[127]);
        let actual = record(&extended, &mut provider, faded, &[73, 199]);
        assert_resumed(&actual, &old_clock, 3203..8008, 1602);
        // New samples replace the old endpoint's zero extension in the
        // canonical 128-sample filter halo. Earlier raw PCM stays identical;
        // the shorter 96-sample creative end fade also relocates intentionally.
        let unchanged_end = 6406 - 128;
        for sample in 0..unchanged_end {
            assert_eq!(
                actual.samples[sample], before.samples[sample],
                "prefix sample {sample}, faded={faded}"
            );
        }
        assert!(
            actual.samples[6406..8007]
                .iter()
                .any(|sample| *sample != [0.; 2])
        );
        assert_eq!(actual.samples[8007], [0.; 2]);
        assert!(actual.suppressed[8007]);
    }
}

#[test]
fn extending_a_captured_repeat_hold_keeps_each_surviving_play_clock() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let initial = document_with_asset(
        rate,
        &["quiet", "repeat"],
        [
            ("quiet", hold(1)),
            ("room", room_tone(2, audio(512, 641))),
            (
                "repeat",
                BeatNode {
                    label: "Twice".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("room"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2)
                            .unwrap(),
                        gap: None,
                    },
                },
            ),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let bindings = capture_unbound_audio_bindings(
        &initial,
        AudioTimingId {
            allocation: RevisionId::new("repeat-capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&initial).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let captured = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let request = CommandRequest {
        project_id: captured.project_id().clone(),
        expected_revision: captured.revision_id().clone(),
        new_revision: RevisionId::new("extend-repeat-room").unwrap(),
        command: Command::SetHoldDuration {
            node: id("room"),
            duration: duration(3),
        },
    };
    let edit = apply(&captured, &request).unwrap();
    let extended = edit.forward.apply(&captured).unwrap();
    assert_eq!(edit.inverse.apply(&extended).unwrap(), captured);
    assert_eq!(extended.audio_bindings(), captured.audio_bindings());
    let mut provider = FixtureProvider::new();
    for faded in [false, true] {
        let actual = record(&extended, &mut provider, faded, &[181, 23]);
        for (old_origin, current_start, current_end) in [(1, 1, 4), (3, 4, 7)] {
            let reference = document_with_asset(
                rate,
                &["quiet", "room"],
                [
                    ("quiet", hold(old_origin)),
                    ("room", room_tone(3, audio(512, 641))),
                ],
                BTreeMap::new(),
                audio(0, 8197).span,
            );
            let expected = record(&reference, &mut provider, faded, &[256]);
            assert_resumed(
                &actual,
                &expected,
                ntsc_boundary(current_start)..ntsc_boundary(current_end),
                ntsc_boundary(old_origin),
            );
        }
    }
}

#[test]
fn insertion_preserves_fractional_source_placement_offset_and_first_speech_sample() {
    for sign in [-1, 1] {
        let (original, mut provider) = mono_original(4);
        let mut wire = serde_json::to_value(&original).unwrap();
        let mut leaf = original.nodes()[&id("source")].clone();
        let NodeKind::Source { source } = &mut leaf.kind else {
            unreachable!()
        };
        source.audio_mapping = SourceAudioMapping::Placement {
            start: ratio(sign, 7),
            frames: source
                .audio_mapping
                .duration_frames(source.duration)
                .unwrap(),
        };
        source.audio_offset = AudioSample(3);
        wire["nodes"]["source"] = serde_json::to_value(leaf).unwrap();
        let original = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let inserted = insert_pause(&original, 0, 1, "before-placed-source");
        for faded in [false, true] {
            let old = record(&original, &mut provider, faded, &[256]);
            let actual = record(&inserted, &mut provider, faded, &[37, 197]);
            assert_pause(&actual, 0..1602);
            assert_resumed(&actual, &old, 1602..8008, 0);
        }
    }
}

#[test]
fn a_pause_preserves_the_two_sample_creative_envelope_on_both_sides() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document_with_asset(
        rate,
        &["source"],
        [("source", source(rate, 2, 0..2))],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let inserted = insert_pause(&original, 1, 1, "tiny-pause");
    let mut provider = FixtureProvider::new();
    let old = record(&original, &mut provider, true, &[2]);
    let actual = record(&inserted, &mut provider, true, &[1]);
    assert_eq!(old.samples[0], fixture_sample(0).map(|v| v * 0.5));
    assert_eq!(old.samples[1], fixture_sample(1).map(|v| v * 0.5));
    assert_eq!(
        actual.samples,
        vec![old.samples[0], [0.; 2], old.samples[1]]
    );
    assert_eq!(actual.suppressed, vec![false, true, false]);
}
