//! Canonical PCM fixtures exercise sample clocks, not decoded real-media evidence.
use super::*;

fn voice(start: i64, frames: i64) -> BeatNode {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut node = source(rate, frames, 0..1);
    let NodeKind::Source { source } = &mut node.kind else {
        panic!()
    };
    let audio = audio_at_rate(start, start + 20_000, 44_100);
    source.audio_mapping = SourceAudioMapping::natural_rate(audio.span, rate).unwrap();
    source.audio = Some(audio);
    node
}

fn resize(document: &ProjectDocument, target: &str, frames: i64) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("ordinary-duration").unwrap(),
            command: Command::SetHoldDuration {
                node: id(target),
                duration: duration(frames),
            },
        },
    )
    .unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

#[test]
fn ordinary_hold_shrink_retains_44100_source_phase_3100_at_new_sample_6406() {
    let before = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["lead", "pause", "speech"],
        [
            ("lead", hold(3)),
            ("pause", hold(2)),
            ("speech", voice(3100, 2)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let (_, mut provider) = mono_original(2);
    // B(5)=8008 exactly. The old Source starts at source sample 3100.
    // B(4)=6406.4 rounds to 6406; sampling anew there would instead begin
    // at 3100-147/400. Resolve the literal old lattice directly from the
    // canonical provider rather than deriving an expected plan descriptor.
    // Only the first 256 samples use an independent recipe. They are far
    // from the finite Source endpoint and prove the literal entry phase.
    // The complete old StageAudio allocation supplies its own endpoint DSP
    // support for the exact retained old/new comparison below.
    let expected = provider
        .source
        .prepare(
            ResampleRecipe::new(
                3100..23_100,
                ratio(3100, 1),
                AudioSample(8008),
                ratio(147, 160),
                AudioSample(8008)..AudioSample(11211),
            )
            .unwrap(),
            AudioSample(8008),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    let old = record(&before, &mut provider, false, &[256]);
    assert_eq!(old.samples[8008], expected[0]);
    for (offset, sample) in expected.iter().enumerate() {
        assert_eq!(
            old.samples[8008 + offset],
            *sample,
            "independent old sample {}",
            8008 + offset
        );
    }
    let after = resize(&before, "pause", 1);
    let actual = record(&after, &mut provider, false, &[127, 3, 251]);
    // B(6)=roundEven(9609.6)=9610. The new four-to-six-frame Source
    // allocation is one sample longer than its old five-to-seven allocation.
    assert_eq!(actual.samples.len(), 9610);
    assert_eq!(actual.samples[6406], expected[0]);
    for offset in 0..3203 {
        assert_eq!(
            actual.samples[6406 + offset],
            old.samples[8008 + offset],
            "new sample {} must retain old source phase",
            6406 + offset
        );
    }
    assert_eq!(actual.samples[9609], [0.; 2]);
    assert!(actual.suppressed[9609]);
    assert_pause(&actual, 0..6406);
    assert_eq!(record(&after, &mut provider, false, &[1, 239]), actual);
}

fn partial_bindings(document: &ProjectDocument) -> ProjectDocument {
    let bindings = capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: RevisionId::new("ordinary-old-clock").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    for owner in ["speech", "tail"] {
        assert!(
            wire["audio_bindings"]["bindings"]
                .as_object_mut()
                .unwrap()
                .remove(owner)
                .is_some()
        );
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn assert_allocation(
    actual: &Recorded,
    old: &Recorded,
    range: Range<i64>,
    shift: i64,
    label: &str,
) {
    let before = ntsc_boundary(range.start)..ntsc_boundary(range.end);
    let after = ntsc_boundary(range.start + shift)..ntsc_boundary(range.end + shift);
    for (offset, sample) in after.enumerate() {
        let previous = before.start + offset;
        let (expected, suppressed) = if previous < before.end {
            (old.samples[previous], old.suppressed[previous])
        } else {
            ([0.; 2], true)
        };
        assert_eq!(
            actual.samples[sample], expected,
            "{label} allocation {range:?}, old sample {previous}, new sample {sample}"
        );
        assert_eq!(
            actual.suppressed[sample], suppressed,
            "{label} suppression at {sample}"
        );
    }
}

fn chosen_hold(room: bool, frames: i64) -> BeatNode {
    if room {
        room_tone(frames, audio_at_rate(321, 987, 44_100))
    } else {
        hold(frames)
    }
}

fn assert_current_hold(
    actual: &Recorded,
    provider: &mut FixtureProvider,
    room: bool,
    old_start: i64,
    new_start: i64,
    frames: i64,
    label: &str,
) {
    if !room {
        assert_pause(
            actual,
            ntsc_boundary(new_start)..ntsc_boundary(new_start + frames),
        );
        return;
    }
    // A changed room-tone endpoint legitimately changes filter support. Its
    // current-duration recipe still samples on the old absolute owner clock.
    let reference = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["lead", "room"],
        [
            ("lead", hold(old_start)),
            ("room", chosen_hold(true, frames)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let expected = record(&reference, provider, false, &[199, 7]);
    let old_samples = ntsc_boundary(old_start)..ntsc_boundary(old_start + frames);
    for (offset, sample) in
        (ntsc_boundary(new_start)..ntsc_boundary(new_start + frames)).enumerate()
    {
        let old = old_samples.start + offset;
        assert_eq!(
            actual.samples[sample],
            expected.samples.get(old).copied().unwrap_or([0.; 2]),
            "{label} current room tone, old sample {old}, new sample {sample}"
        );
        assert_eq!(
            actual.suppressed[sample],
            expected.suppressed.get(old).copied().unwrap_or(true)
        );
    }
}

fn assert_policy_only_duration(
    before: &ProjectDocument,
    after: &ProjectDocument,
    target: &str,
    frames: i64,
) {
    let NodeKind::Hold { recipe: old } = &before.nodes()[&id(target)].kind else {
        panic!()
    };
    let NodeKind::Hold { recipe: current } = &after.nodes()[&id(target)].kind else {
        panic!()
    };
    let mut expected = old.clone();
    expected.duration = duration(frames);
    assert_eq!(*current, expected);
}

#[test]
fn ordinary_hold_duration_preserves_unbound_and_partial_source_room_tone_and_tail_allocations() {
    for room in [false, true] {
        let initial = document_with_asset(
            FrameRate::new(30_000, 1001).unwrap(),
            &["lead", "pause", "speech", "room", "tail"],
            [
                ("lead", hold(3)),
                ("pause", chosen_hold(room, 2)),
                ("speech", voice(3100, 2)),
                ("room", chosen_hold(true, 2)),
                ("tail", voice(15_000, 2)),
            ],
            BTreeMap::new(),
            audio_at_rate(0, 44_117, 44_100).span,
        );
        for partial in [false, true] {
            let before = if partial {
                partial_bindings(&initial)
            } else {
                initial.clone()
            };
            let (_, mut provider) = mono_original(2);
            let old = record(&before, &mut provider, false, &[256]);
            for frames in [1, 3] {
                let after = resize(&before, "pause", frames);
                assert_policy_only_duration(&before, &after, "pause", frames);
                assert_eq!(after.duration().unwrap().frames(), 9 + frames);
                let actual = record(&after, &mut provider, false, &[127, 3, 251]);
                let label = format!("ordinary room={room} partial={partial} frames={frames}");
                assert_allocation(&actual, &old, 0..3, 0, &label);
                assert_current_hold(&actual, &mut provider, room, 3, 3, frames, &label);
                for range in [5..7, 7..9, 9..11] {
                    assert_allocation(&actual, &old, range, frames - 2, &label);
                }
                assert_eq!(
                    record(&after, &mut provider, false, &[1, 239]),
                    actual,
                    "{label}"
                );
            }
        }
    }
}

fn shared_fixture(room: bool) -> ProjectDocument {
    let NodeKind::Hold { recipe: gap } = chosen_hold(true, 2).kind else {
        panic!()
    };
    let mut repeat = BeatNode::sequence("Three plays", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("body"),
        iterations: IterationOrder::new(RevisionId::new("ordinary-plays").unwrap(), 3).unwrap(),
        gap: Some(gap),
        escalation: None,
    };
    document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["lead", "repeat", "tail"],
        [
            ("lead", hold(3)),
            ("repeat", repeat),
            (
                "body",
                BeatNode::sequence("Default body", vec![id("pause"), id("speech")]),
            ),
            ("pause", chosen_hold(room, 2)),
            ("speech", voice(3100, 2)),
            (
                "owned-body",
                BeatNode::sequence("Owned body", vec![id("owned-pause"), id("owned-speech")]),
            ),
            ("owned-pause", chosen_hold(room, 2)),
            ("owned-speech", voice(9000, 2)),
            ("tail", voice(15_000, 2)),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: RevisionId::new("ordinary-plays").unwrap(),
                    ordinal: 1,
                },
                root: id("owned-body"),
            }])
            .unwrap(),
        )]),
        audio_at_rate(0, 44_117, 44_100).span,
    )
}

#[test]
fn ordinary_shared_and_owned_hold_duration_keeps_each_default_play_gap_and_tail_on_its_old_clock() {
    for room in [false, true] {
        let initial = shared_fixture(room);
        for partial in [false, true] {
            let before = if partial {
                partial_bindings(&initial)
            } else {
                initial.clone()
            };
            let (_, mut provider) = mono_original(2);
            let old = record(&before, &mut provider, false, &[256]);
            // An ordinary default definition changes first and last plays;
            // the middle owned branch stays independent. Editing that owned
            // definition changes only its one concrete occurrence instead.
            for target in ["pause", "owned-pause"] {
                for frames in [1, 3] {
                    let after = resize(&before, target, frames);
                    assert_policy_only_duration(&before, &after, target, frames);
                    let count = if target == "pause" { 2 } else { 1 };
                    assert_eq!(
                        after.duration().unwrap().frames(),
                        21 + count * (frames - 2)
                    );
                    let actual = record(&after, &mut provider, false, &[127, 3, 251]);
                    let label = format!(
                        "shared target={target} room={room} partial={partial} frames={frames}"
                    );
                    let mut shift = 0;
                    for range in [
                        0..3,
                        3..5,
                        5..7,
                        7..9,
                        9..11,
                        11..13,
                        13..15,
                        15..17,
                        17..19,
                        19..21,
                    ] {
                        let selected = if target == "pause" {
                            range == (3..5) || range == (15..17)
                        } else {
                            range == (9..11)
                        };
                        if selected {
                            assert_current_hold(
                                &actual,
                                &mut provider,
                                room,
                                range.start,
                                range.start + shift,
                                frames,
                                &label,
                            );
                            shift += frames - 2;
                        } else {
                            assert_allocation(&actual, &old, range, shift, &label);
                        }
                    }
                    assert_eq!(shift, count * (frames - 2));
                    assert_eq!(
                        record(&after, &mut provider, false, &[1, 239]),
                        actual,
                        "{label}"
                    );
                }
            }
        }
    }
}
