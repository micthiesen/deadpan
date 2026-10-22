//! Repeat owners retain their separate PointCeil phases inside processing input.
use super::*;
#[path = "processing_hold_duration_repeat/nested_fixed_retime.rs"]
mod nested_fixed_retime;
#[path = "processing_hold_duration_repeat/shared_fixed.rs"]
mod shared_fixed;
#[path = "processing_hold_duration_repeat/this_play_fixed_retime.rs"]
mod this_play_fixed_retime;
use deadpan_audio::{RoomTone, RoomToneRecipe};

fn repeated_fixture(pitch: PitchPolicy, partial: bool) -> ProjectDocument {
    let NodeKind::Hold { recipe: gap } = room_tone(2, audio_at_rate(512, 833, 44_100)).kind else {
        unreachable!()
    };
    let mut repeated = BeatNode::sequence("Three plays", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: id("default"),
        iterations: IterationOrder::new(RevisionId::new("input-plays").unwrap(), 3).unwrap(),
        gap: Some(gap),
        escalation: None,
    };
    let spoken = |start, frames| {
        let mut node = voice();
        let NodeKind::Source { source } = &mut node.kind else {
            unreachable!()
        };
        source.duration = duration(frames);
        let audio = audio_at_rate(start, start + 20_000, 44_100);
        source.audio_mapping =
            SourceAudioMapping::natural_rate(audio.span, FrameRate::new(30_000, 1001).unwrap())
                .unwrap();
        source.audio = Some(audio);
        node
    };
    let document = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["processor"],
        [
            (
                "processor",
                retime(
                    "body",
                    if pitch == PitchPolicy::Preserve {
                        9
                    } else {
                        18
                    },
                    0..18,
                    pitch,
                ),
            ),
            (
                "body",
                BeatNode::sequence("Fixed operand", vec![id("lead"), id("repeat"), id("tail")]),
            ),
            ("lead", hold(1)),
            ("repeat", repeated),
            (
                "default",
                BeatNode::sequence("Default", vec![id("pause"), id("speech")]),
            ),
            ("pause", hold(2)),
            ("speech", spoken(3100, 2)),
            (
                "owned",
                BeatNode::sequence("Owned", vec![id("owned-pause"), id("owned-speech")]),
            ),
            ("owned-pause", hold(2)),
            ("owned-speech", spoken(9000, 2)),
            ("tail", spoken(15_000, 4)),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: RevisionId::new("input-plays").unwrap(),
                    ordinal: 1,
                },
                root: id("owned"),
            }])
            .unwrap(),
        )]),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    document.validate().unwrap();
    if !partial {
        assert!(document.audio_bindings().is_empty());
        return document;
    }
    let bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("old-repeated-input-clocks").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    // Historical gap and owned Source clocks coexist with unbound default
    // Source/tail owners. The default is used by two separate physical plays.
    for owner in ["speech", "tail"] {
        assert!(
            wire["audio_bindings"]["bindings"]
                .as_object_mut()
                .unwrap()
                .remove(owner)
                .is_some()
        );
    }
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(
        document
            .audio_bindings()
            .gap_bindings()
            .contains_key(&id("repeat"))
    );
    assert!(
        document
            .audio_bindings()
            .bindings()
            .contains_key(&id("owned-speech"))
    );
    document
}

fn point(frame: i64) -> i64 {
    // The processor input's own origin is0. This is PointCeil, never root
    // RoundEven: point9=14415, while RoundEven(frame9)=14414.
    ratio(i128::from(frame) * 8008, 5).ceil().unwrap() as i64
}

fn gap_reference(provider: &FixtureProvider, old_start: i64) -> Vec<[f32; 2]> {
    // Exact room-tone selected extent321 source samples ->17120/49 mix
    // samples, stored in350 cells. The overlap recipe keeps that fraction.
    let source = finite_pcm(provider, 512..833, ratio(512, 1), 0, ratio(147, 160), 350);
    let recipe = RoomToneRecipe::new(ratio(17120, 49), 3204).unwrap();
    let room = RoomTone::new(recipe, &source, &AtomicBool::new(false)).unwrap();
    let mut prepared = Vec::new();
    while prepared.len() < 3204 {
        let count = (3204 - prepared.len()).min(251) as u32;
        prepared.extend(
            room.render(
                AudioSample(prepared.len() as i64),
                count,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples,
        );
    }
    let (phase, physical_length) = match old_start {
        5 => (ExactRatio::ZERO, 3204),
        11 => (ratio(2, 5), 3203),
        _ => unreachable!(),
    };
    sample_reference(0..3204, phase, ExactRatio::ONE, physical_length, |at| {
        prepared[at as usize]
    })
}

fn expected_input(provider: &FixtureProvider, target: &str, frames: i64) -> Recorded {
    // Explicit frame layouts. Tuples are (first Source, first gap, owned
    // Source, second gap, last default Source, suffix). The fixed window0..18
    // clips only the last Source/suffix, including newly exposed suffix PCM.
    let (first, gap0, owned, gap1, last, tail) = match (target, frames) {
        (_, 2) => (3, 5, 9, 11, 15, 17),
        ("pause", 1) => (2, 4, 8, 10, 13, 15),
        ("pause", 3) => (4, 6, 10, 12, 17, 19),
        ("owned-pause", 1) => (3, 5, 8, 10, 14, 16),
        ("owned-pause", 3) => (3, 5, 10, 12, 16, 18),
        _ => unreachable!(),
    };
    assert_eq!(point(9), 14415);
    assert_eq!(point(18), 28829);
    let mut expected = Recorded {
        samples: vec![[0.; 2]; 28829],
        suppressed: vec![true; 28829],
    };
    // Old per-owner phases follow ceil(frame*8008/5) minus exact entry,
    // times147/160. Different default plays deliberately have different phase.
    for (old, current, duration, native, phase) in [
        (3, first, 2, 3100, ratio(2_480_147, 800)),
        (9, owned, 2, 9000, ratio(7_200_441, 800)),
        (15, last, 2, 3100, ratio(3100, 1)),
        (17, tail, 4, 15_000, ratio(3_000_147, 200)),
    ] {
        let visible = duration.min(18 - current).max(0);
        if visible == 0 {
            continue;
        }
        let start = point(current);
        let end = point(current + visible);
        let historical_length = point(old + visible) - point(old);
        let admitted = (end - start).min(historical_length);
        let native_end = (ratio(native.into(), 1)
            .checked_add(
                ratio(147147, 100)
                    .checked_mul(ratio(visible.into(), 1))
                    .unwrap(),
            )
            .unwrap())
        .ceil()
        .unwrap() as i64;
        let source = finite_pcm(
            provider,
            native..native_end,
            phase,
            start,
            ratio(147, 160),
            start + admitted,
        );
        expected.samples[start as usize..(start + admitted) as usize].copy_from_slice(&source);
        expected.suppressed[start as usize..(start + admitted) as usize].fill(false);
        if admitted < end - start {
            assert_eq!(
                end - start - admitted,
                1,
                "literal retained Source exhaustion"
            );
            assert_eq!(expected.samples[(end - 1) as usize], [0.; 2]);
            assert!(expected.suppressed[(end - 1) as usize]);
        }
    }
    for (old, current) in [(5, gap0), (11, gap1)] {
        let raw = gap_reference(provider, old);
        let start = point(current);
        let end = point(current + 2);
        assert_eq!(raw.len() as i64, point(old + 2) - point(old));
        let admitted = (end - start).min(raw.len() as i64);
        expected.samples[start as usize..(start + admitted) as usize]
            .copy_from_slice(&raw[..admitted as usize]);
        expected.suppressed[start as usize..(start + admitted) as usize].fill(false);
        if admitted < end - start {
            assert_eq!((old, current, end - start - admitted), (11, 10, 1));
            assert_eq!(expected.samples[(end - 1) as usize], [0.; 2]);
            assert!(expected.suppressed[(end - 1) as usize]);
        }
    }
    expected
}

fn repeated_inputs(pitch: PitchPolicy) {
    for partial in [false, true] {
        let before = repeated_fixture(pitch, partial);
        let (_, mut provider) = mono_original(2);
        let plan = Arc::new(RenderPlan::compile(&before).unwrap());
        let support = ratio(0, 1)..ratio(18, 1);
        let old = input_record(&plan, &mut provider, support.clone(), 0..28829, &[251, 3]);
        same_recorded(
            &old,
            &expected_input(&provider, "pause", 2),
            0,
            "old repeated PointCeil input",
        );
        for target in ["pause", "owned-pause"] {
            for frames in [1, 3] {
                let transaction = apply(
                    &before,
                    &CommandRequest {
                        project_id: before.project_id().clone(),
                        expected_revision: before.revision_id().clone(),
                        new_revision: RevisionId::new("repeat-processing-duration").unwrap(),
                        command: Command::SetHoldDuration {
                            node: id(target),
                            duration: duration(frames),
                        },
                    },
                )
                .unwrap();
                assert_eq!(transaction.duration_delta, 0);
                let after = transaction.forward.apply(&before).unwrap();
                assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
                assert_eq!(
                    after.nodes()[&id("processor")],
                    before.nodes()[&id("processor")]
                );
                assert_eq!(after.nodes()[&id("repeat")], before.nodes()[&id("repeat")]);
                assert_eq!(after.overrides(), before.overrides());
                let after = ProjectDocument::from_json(&after.to_json().unwrap()).unwrap();
                let plan = Arc::new(RenderPlan::compile(&after).unwrap());
                let label =
                    format!("pitch={pitch:?} partial={partial} target={target} frames={frames}");
                let expected = expected_input(&provider, target, frames);
                let actual = input_record(
                    &plan,
                    &mut provider,
                    support.clone(),
                    0..28829,
                    &[127, 3, 251],
                );
                same_recorded(&actual, &expected, 0, &label);
                same_recorded(
                    &input_record(&plan, &mut provider, support.clone(), 0..28829, &[1, 239]),
                    &expected,
                    0,
                    &format!("chunk parity {label}"),
                );
            }
        }
    }
}

#[test]
fn preserve_repeat_default_owned_room_tone_gaps_keep_old_point_phases_through_grow_and_shrink() {
    repeated_inputs(PitchPolicy::Preserve);
}

#[test]
fn shift_repeat_default_owned_room_tone_gaps_keep_old_point_phases_through_grow_and_shrink() {
    repeated_inputs(PitchPolicy::Shift { semitones: 3 });
}
