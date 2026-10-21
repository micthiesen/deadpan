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

/// Literal current source support and historical entry phase, independent of
/// the document, RenderPlan, and retained binding representation.
fn cropped_source_pcm(
    provider: &FixtureProvider,
    selection: Range<i64>,
    phase: ExactRatio,
    origin: i64,
    step: ExactRatio,
    end: i64,
) -> Vec<[f32; 2]> {
    let recipe = ResampleRecipe::new(
        selection,
        phase,
        AudioSample(origin),
        step,
        AudioSample(origin)..AudioSample(end),
    )
    .unwrap();
    let mut result = Vec::new();
    while result.len() < (end - origin) as usize {
        let count = ((end - origin) as usize - result.len()).min(251) as u32;
        result.extend(
            provider
                .source
                .prepare(
                    recipe.clone(),
                    AudioSample(origin + result.len() as i64),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    result
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

fn assert_fixed_pcm(actual: &[[f32; 2]], expected: &[[f32; 2]], start: usize, label: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label} PCM length at {start}"
    );
    for (offset, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..2 {
            assert_eq!(
                actual[channel],
                expected[channel],
                "{label} output sample {} channel {channel}",
                start + offset
            );
        }
    }
}

fn assert_fixed_mask(actual: &[bool], expected: &[bool], start: usize, label: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label} mask length at {start}"
    );
    for (offset, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            actual,
            expected,
            "{label} suppression at output sample {}",
            start + offset
        );
    }
}

fn assert_fixed_recorded(actual: &Recorded, expected: &Recorded, label: &str) {
    assert_fixed_pcm(&actual.samples, &expected.samples, 0, label);
    assert_fixed_mask(&actual.suppressed, &expected.suppressed, 0, label);
}

#[test]
fn ordinary_hold_growth_inside_fixed_retime_retains_44100_source_phase_with_unchanged_root_duration()
 {
    let before = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["lead", "rate"],
        [
            ("lead", hold(3)),
            ("rate", retime("body", 4, 0..4, PitchPolicy::FollowSpeed)),
            (
                "body",
                BeatNode::sequence("Hold then Source", vec![id("pause"), id("speech")]),
            ),
            ("pause", hold(2)),
            ("speech", voice(3100, 2)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    // This is an admitted ordinary Edit Retime, with a fixed four-frame input
    // window and four-frame output. No prior physical timing capture exists.
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap().frames(), 7);
    assert_eq!(before.audio_bindings(), &AudioBindingState::default());
    let (_, mut provider) = mono_original(2);
    // B(5)=8008 exactly, and the Source's literal first phase is 3100.
    // Resolve only the first 256 samples independently, far from the finite
    // Source endpoint. The old StageAudio output supplies the complete finite
    // source context for the surviving prefix comparison below.
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
    assert_eq!(old.samples.len(), 11211);
    assert_pause(&old, 0..8008);
    assert_eq!(old.samples[8008], expected[0]);
    assert_fixed_pcm(
        &old.samples[8008..8264],
        &expected,
        8008,
        "old literal phase",
    );
    assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.01));

    let transaction = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new("ordinary-fixed-retime-duration").unwrap(),
            command: Command::SetHoldDuration {
                node: id("pause"),
                duration: duration(3),
            },
        },
    )
    .unwrap();
    assert_eq!(transaction.duration_delta, 0);
    let changed = transaction.forward.apply(&before).unwrap();
    assert_eq!(transaction.inverse.apply(&changed).unwrap(), before);
    let after = ProjectDocument::from_json(&changed.to_json().unwrap()).unwrap();
    assert_eq!(after.duration().unwrap().frames(), 7);
    assert_policy_only_duration(&before, &after, "pause", 3);
    assert_eq!(after.nodes()[&id("rate")], before.nodes()[&id("rate")]);
    assert_eq!(after.nodes()[&id("speech")], before.nodes()[&id("speech")]);
    let actual = record(&after, &mut provider, false, &[127, 3, 251]);
    // The child is now five frames, but the Retime still selects [0,4).
    // The visible Source moves from root [5,7) to [6,7), retaining its
    // old phase. B(6)=roundEven(9609.6)=9610. A new sampling
    // origin here would reset phase by +147/400 of a 44.1 kHz source sample.
    assert_eq!(actual.samples.len(), 11211);
    assert_pause(&actual, 0..9610);
    assert_eq!(actual.samples[9610], expected[0]);
    assert_fixed_pcm(
        &actual.samples[9610..9866],
        &expected,
        9610,
        "new literal phase",
    );
    // Its current one-frame crop ends at source ceil(3100+1471.47)=4572.
    // The 128-tap radius first touches that changed endpoint at offset1463.
    // Every earlier sample remains identical to the old audible prefix.
    for offset in 0..1463 {
        assert_eq!(
            actual.samples[9610 + offset],
            old.samples[8008 + offset],
            "fixed Retime new sample {} must retain old Source prefix at {}",
            9610 + offset,
            8008 + offset,
        );
        assert_eq!(
            actual.suppressed[9610 + offset],
            old.suppressed[8008 + offset]
        );
    }
    let current = cropped_source_pcm(
        &provider,
        3100..4572,
        ratio(3100, 1),
        9610,
        ratio(147, 160),
        11211,
    );
    assert_fixed_pcm(
        &actual.samples[9610..11211],
        &current,
        9610,
        "minimal current crop",
    );
    assert!(actual.suppressed[9610..11211].iter().all(|value| !value));
    assert_fixed_recorded(
        &record(&after, &mut provider, false, &[1, 239]),
        &actual,
        "alternate chunk schedule",
    );
}

#[test]
fn ordinary_hold_shrink_inside_fixed_retime_exposes_old_source_continuation_and_keeps_outside_suffix()
 {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let before = document_with_asset(
        rate,
        &["lead", "rate", "tail"],
        [
            ("lead", hold(3)),
            ("rate", retime("body", 4, 0..4, PitchPolicy::FollowSpeed)),
            (
                "body",
                BeatNode::sequence("Cropped speech", vec![id("pause"), id("speech")]),
            ),
            ("pause", hold(2)),
            ("speech", voice(3100, 4)),
            ("tail", voice(15_000, 2)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let (_, mut provider) = mono_original(2);
    let old = record(&before, &mut provider, false, &[256]);
    // Old two-frame crop ends at ceil(3100+2*1471.47)=6043.
    let old_crop = cropped_source_pcm(
        &provider,
        3100..6043,
        ratio(3100, 1),
        8008,
        ratio(147, 160),
        11211,
    );
    assert_fixed_pcm(
        &old.samples[8008..11211],
        &old_crop,
        8008,
        "old shrink crop",
    );
    let after = resize(&before, "pause", 1);
    assert_eq!(before.duration().unwrap().frames(), 9);
    assert_eq!(after.duration().unwrap().frames(), 9);
    assert_policy_only_duration(&before, &after, "pause", 1);
    assert_eq!(after.nodes()[&id("rate")], before.nodes()[&id("rate")]);
    let actual = record(&after, &mut provider, false, &[127, 3, 251]);
    assert_eq!(actual.samples.len(), 14414);
    assert_pause(&actual, 0..6406);
    // [4,7) exposes three Source frames on the old phase, including1602
    // samples beyond its old Retime crop. Current support ends at
    // ceil(3100+3*1471.47)=7515, so its finite endpoint is checked separately.
    let current = cropped_source_pcm(
        &provider,
        3100..7515,
        ratio(3100, 1),
        6406,
        ratio(147, 160),
        11211,
    );
    assert_fixed_pcm(
        &actual.samples[6406..11211],
        &current,
        6406,
        "current shrink crop",
    );
    assert!(actual.suppressed[6406..11211].iter().all(|value| !value));
    // The old crop's 128-tap zero extension begins affecting offset3064;
    // all samples before it must survive bit for bit.
    assert_fixed_pcm(
        &actual.samples[6406..9470],
        &old.samples[8008..11072],
        6406,
        "shrink unchanged prefix",
    );
    assert!(
        actual.samples[9609..11211]
            .iter()
            .flatten()
            .any(|s| s.abs() > 0.01)
    );
    assert_fixed_pcm(
        &actual.samples[11211..],
        &old.samples[11211..],
        11211,
        "shrink outside suffix",
    );
    assert_fixed_mask(
        &actual.suppressed[11211..],
        &old.suppressed[11211..],
        11211,
        "shrink outside suffix",
    );
    assert_fixed_recorded(
        &record(&after, &mut provider, false, &[1, 239]),
        &actual,
        "alternate chunk schedule",
    );

    // A genuinely too-short child still refuses; phase retention cannot make
    // an out-of-range fixed input window valid.
    let short = document_with_asset(
        rate,
        &["lead", "rate"],
        [
            ("lead", hold(3)),
            ("rate", retime("body", 4, 0..4, PitchPolicy::FollowSpeed)),
            (
                "body",
                BeatNode::sequence("Short child", vec![id("pause"), id("speech")]),
            ),
            ("pause", hold(2)),
            ("speech", voice(3100, 2)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let saved = short.to_json().unwrap();
    assert!(
        apply(
            &short,
            &CommandRequest {
                project_id: short.project_id().clone(),
                expected_revision: short.revision_id().clone(),
                new_revision: RevisionId::new("invalid-fixed-window-shrink").unwrap(),
                command: Command::SetHoldDuration {
                    node: id("pause"),
                    duration: duration(1)
                },
            }
        )
        .is_err()
    );
    assert_eq!(short.to_json().unwrap(), saved);
}

#[test]
fn ordinary_hold_growth_inside_nested_nonzero_affine_windows_keeps_literal_44100_phase_and_suffix()
{
    let before = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["lead", "outer-rate", "tail"],
        [
            ("lead", hold(3)),
            (
                "outer-rate",
                retime("inner-rate", 3, 0..6, PitchPolicy::FollowSpeed),
            ),
            (
                "inner-rate",
                retime("body", 6, 1..5, PitchPolicy::FollowSpeed),
            ),
            (
                "body",
                BeatNode::sequence("Affine crop", vec![id("pause"), id("speech")]),
            ),
            ("pause", hold(2)),
            ("speech", voice(3100, 4)),
            ("tail", voice(15_000, 2)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    // Inner output/input slope 3/2, outer 1/2, product 3/4. Input frame 2
    // appears at root 3+(2-1)*3/4=15/4, exactly sample6006. Natural audio
    // advances (147/160)/(3/4)=49/40 source samples per output sample.
    let (_, mut provider) = mono_original(2);
    let expected = provider
        .source
        .prepare(
            ResampleRecipe::new(
                3100..23_100,
                ratio(3100, 1),
                AudioSample(6006),
                ratio(49, 40),
                AudioSample(6006)..AudioSample(9610),
            )
            .unwrap(),
            AudioSample(6006),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    let old = record(&before, &mut provider, false, &[256]);
    assert_eq!(before.duration().unwrap().frames(), 8);
    assert_fixed_pcm(
        &old.samples[6006..6262],
        &expected,
        6006,
        "old affine literal phase",
    );
    assert!(expected.iter().flatten().any(|s| s.abs() > 0.01));
    let after = resize(&before, "pause", 3);
    assert_eq!(after.duration().unwrap().frames(), 8);
    for owner in ["inner-rate", "outer-rate", "speech", "tail"] {
        assert_eq!(after.nodes()[&id(owner)], before.nodes()[&id(owner)]);
    }
    let actual = record(&after, &mut provider, false, &[127, 3, 251]);
    // New input frame 3 appears at root 9/2, whose 7207.2 sample boundary
    // rounds to 7207. Sampling anew would subtract 49/200 source samples.
    assert_eq!(actual.samples.len(), 12813);
    assert_pause(&actual, 0..7207);
    assert_fixed_pcm(
        &actual.samples[7207..7463],
        &expected,
        7207,
        "new affine literal phase",
    );
    // Current two-input-frame crop ends at6043. Step49/40 uses radius157,
    // so offset2275 is the first sample whose kernel sees the changed crop.
    assert_fixed_pcm(
        &actual.samples[7207..9482],
        &old.samples[6006..8281],
        7207,
        "affine unchanged prefix",
    );
    assert_fixed_mask(
        &actual.suppressed[7207..9482],
        &old.suppressed[6006..8281],
        7207,
        "affine unchanged prefix",
    );
    // Current local support[0,2) on the old origin15/4, scale3/4 lattice
    // projects to historical root[15/4,21/4): B=6006..8408, length2402.
    // Its live root[9/2,6) allocation7207..9610 has2403 slots. The final
    // reference point8408 is outside the retained half-open allocation.
    let historical: Range<usize> = 6006..8408;
    let live: Range<usize> = 7207..9610;
    assert_eq!(historical.len(), 2402);
    assert_eq!(live.len(), 2403);
    let mut current = cropped_source_pcm(
        &provider,
        3100..6043,
        ratio(3100, 1),
        7207,
        ratio(49, 40),
        9609,
    );
    assert_eq!(current.len(), historical.len());
    current.push([0.; 2]);
    assert_fixed_pcm(
        &actual.samples[7207..9610],
        &current,
        7207,
        "affine current crop",
    );
    let mut mask = vec![false; historical.len()];
    mask.push(true);
    assert_fixed_mask(
        &actual.suppressed[live.clone()],
        &mask,
        live.start,
        "affine retained support",
    );
    assert_eq!(actual.samples[9609], [0.; 2]);
    assert!(actual.suppressed[9609]);
    assert_fixed_pcm(
        &actual.samples[9610..],
        &old.samples[9610..],
        9610,
        "affine outside suffix",
    );
    assert_fixed_mask(
        &actual.suppressed[9610..],
        &old.suppressed[9610..],
        9610,
        "affine outside suffix",
    );
    assert_fixed_recorded(
        &record(&after, &mut provider, false, &[1, 239]),
        &actual,
        "alternate chunk schedule",
    );
}

fn fixed_retime_repeated_fixture() -> ProjectDocument {
    let NodeKind::Hold { recipe: gap } = chosen_hold(true, 2).kind else {
        panic!()
    };
    let repeat = |child: &str, name: &str| {
        let mut node = BeatNode::sequence(name, vec![]);
        node.kind = NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new(name).unwrap(), 2).unwrap(),
            gap: Some(gap.clone()),
            escalation: None,
        };
        node
    };
    document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["lead", "outer", "tail"],
        [
            ("lead", hold(1)),
            ("outer", repeat("rate", "outer-fixed")),
            ("rate", retime("inner", 12, 0..12, PitchPolicy::FollowSpeed)),
            ("inner", repeat("body", "inner-fixed")),
            (
                "body",
                BeatNode::sequence("Default", vec![id("pause"), id("speech")]),
            ),
            ("pause", hold(2)),
            ("speech", voice(3100, 4)),
            (
                "owned-body",
                BeatNode::sequence("Owned", vec![id("owned-pause"), id("owned-speech")]),
            ),
            ("owned-pause", hold(2)),
            ("owned-speech", voice(9000, 4)),
            ("tail", voice(15_000, 2)),
        ],
        BTreeMap::from([(
            id("inner"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: RevisionId::new("inner-fixed").unwrap(),
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
fn ordinary_shared_hold_inside_fixed_retime_keeps_inner_owned_gap_clocks_and_outer_repeat_suffix() {
    let initial = fixed_retime_repeated_fixture();
    for partial in [false, true] {
        let before = if partial {
            let captured = capture_unbound_audio_bindings(
                &initial,
                AudioTimingId {
                    allocation: RevisionId::new("fixed-retime-old-clock").unwrap(),
                    ordinal: 0,
                },
            )
            .unwrap();
            let mut wire = serde_json::to_value(&initial).unwrap();
            wire["audio_bindings"] = serde_json::to_value(captured).unwrap();
            // Keep historical room-tone gap clocks while both Source owners
            // and the outside suffix deliberately begin without bindings.
            for owner in ["speech", "owned-speech", "tail"] {
                assert!(
                    wire["audio_bindings"]["bindings"]
                        .as_object_mut()
                        .unwrap()
                        .remove(owner)
                        .is_some()
                );
            }
            ProjectDocument::from_json(&wire.to_string()).unwrap()
        } else {
            initial.clone()
        };
        let (_, mut provider) = mono_original(2);
        let old = record(&before, &mut provider, false, &[256]);
        let after = resize(&before, "pause", 3);
        assert_eq!(before.duration().unwrap().frames(), 29);
        assert_eq!(after.duration().unwrap().frames(), 29);
        assert_policy_only_duration(&before, &after, "pause", 3);
        for owner in ["rate", "owned-pause", "owned-speech", "tail"] {
            assert_eq!(after.nodes()[&id(owner)], before.nodes()[&id(owner)]);
        }
        let actual = record(&after, &mut provider, false, &[127, 3, 251]);
        for offset in [0, 14] {
            let label = format!("fixed-stage outer offset={offset} partial={partial}");
            // Fixed stage length 12 leaves the outer plays at 1 and 15.
            // Inside it, default Source and room-tone gap move by one frame.
            assert_allocation(&actual, &old, 3 + offset..7 + offset, 1, &label);
            assert_allocation(&actual, &old, 7 + offset..9 + offset, 1, &label);
            // The owned Source moves 11->12 inside the first occurrence and
            // loses one visible frame at the fixed crop, retaining its prefix.
            let old_start = ntsc_boundary(11 + offset);
            let new_range = ntsc_boundary(12 + offset)..ntsc_boundary(13 + offset);
            // Old rounded entries are9000+147/400 at root11, and9000 at
            // root25. Current one-frame crop ends at ceil(9000+1471.47)
            // =10472. Its radius128 first changes the old prefix at1463.
            let phase = if offset == 0 {
                ratio(3_600_147, 400)
            } else {
                ratio(9000, 1)
            };
            // CURRENT one-frame support is allocated on the OLD lattice.
            // Outer0: old root[11,12) ->[17618,19219), length1601;
            // live[12,13) ->[19219,20821), length1602, final slot exhausted.
            // Outer1: old root[25,26) ->[40040,41642), length1602;
            // live[26,27) ->[41642,43243), length1601, no exhausted slot.
            let historical: Range<usize> = if offset == 0 {
                17618..19219
            } else {
                40040..41642
            };
            let literal_live: Range<usize> = if offset == 0 {
                19219..20821
            } else {
                41642..43243
            };
            assert_eq!(historical.len(), if offset == 0 { 1601 } else { 1602 });
            assert_eq!(new_range, literal_live);
            assert_eq!(new_range.len(), if offset == 0 { 1602 } else { 1601 });
            let admitted = historical.len().min(new_range.len());
            let mut current = cropped_source_pcm(
                &provider,
                9000..10472,
                phase,
                new_range.start as i64,
                ratio(147, 160),
                (new_range.start + admitted) as i64,
            );
            assert_eq!(current.len(), admitted);
            current.resize(new_range.len(), [0.; 2]);
            assert_fixed_pcm(
                &actual.samples[new_range.start..new_range.start + 1463],
                &old.samples[old_start..old_start + 1463],
                new_range.start,
                &label,
            );
            assert_fixed_pcm(
                &actual.samples[new_range.clone()],
                &current,
                new_range.start,
                &label,
            );
            assert_fixed_mask(
                &actual.suppressed[new_range.start..new_range.start + 1463],
                &old.suppressed[old_start..old_start + 1463],
                new_range.start,
                &label,
            );
            let mask: Vec<_> = (0..new_range.len())
                .map(|index| index >= admitted)
                .collect();
            assert_fixed_mask(
                &actual.suppressed[new_range.clone()],
                &mask,
                new_range.start,
                &label,
            );
            if offset == 0 {
                assert_eq!(actual.samples[20820], [0.; 2]);
                assert!(actual.suppressed[20820]);
            } else {
                assert!(!actual.suppressed[43242]);
            }
            assert_pause(
                &actual,
                ntsc_boundary(1 + offset)..ntsc_boundary(4 + offset),
            );
            assert_pause(
                &actual,
                ntsc_boundary(10 + offset)..ntsc_boundary(12 + offset),
            );
        }
        // The outer Repeat gap and suffix are audible and stay at the same
        // absolute support and phase; the inner growth cannot ripple past rate.
        for range in [13..15, 27..29] {
            let samples = ntsc_boundary(range.start)..ntsc_boundary(range.end);
            assert!(
                old.samples[samples.clone()]
                    .iter()
                    .flatten()
                    .any(|s| s.abs() > 0.01)
            );
            assert_fixed_pcm(
                &actual.samples[samples.clone()],
                &old.samples[samples.clone()],
                samples.start,
                "Repeat outside suffix",
            );
            assert_fixed_mask(
                &actual.suppressed[samples.clone()],
                &old.suppressed[samples.clone()],
                samples.start,
                "Repeat outside suffix",
            );
        }
        assert_fixed_recorded(
            &record(&after, &mut provider, false, &[1, 239]),
            &actual,
            "alternate chunk schedule",
        );
    }
}
