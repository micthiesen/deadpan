//! Independent fractional-clock move oracles using the decoded 44.1 kHz fixture.
//! Raw timing, live owner gain and the independent root sound bus are separate.
use super::placement::{bounded_source, prior_edits};
use super::*;

fn interior(parent: &str, target: &str, at: i64) -> MoveRangeDestination {
    MoveRangeDestination::Interior {
        parent: id(parent),
        target: id(target),
        at: frames(at),
    }
}

fn moved(
    before: &ProjectDocument,
    parent: &str,
    selected: Range<i64>,
    destination: MoveRangeDestination,
    name: &str,
) -> ProjectDocument {
    let range = FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end)).unwrap();
    let query = before.range_move(&id(parent), range, &destination).unwrap();
    let after = edit(
        before,
        name,
        Command::MoveRange {
            source_revision: before.revision_id().clone(),
            source_parent: id(parent),
            range,
            destination,
            identities: SplitIdentities {
                nodes: (0..query.required_ids)
                    .map(|n| id(&format!("{name}-{n}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    after
}

fn count(start: i64, end: i64) -> usize {
    usize::try_from(boundary(end) - boundary(start)).unwrap()
}

// (old start, new start, frame length, old physical owner start, owner duration).
// These fixture-specific intervals are specified by hand, never read from a
// binding, preflight map or compiled plan. Sample entries come from B(old start)
// and exact owner origins; retained discrete support is independent of geometry.
fn source_islands(provider: &Provider, islands: &[(i64, i64, i64, i64, i64)]) -> Vec<[f32; 2]> {
    let mut result = Vec::new();
    for &(old, new, length, owner, extent) in islands {
        assert_eq!(i64::try_from(result.len()).unwrap(), boundary(new));
        let phase = ExactRatio::integer(boundary(old))
            .checked_sub(ratio(i128::from(owner) * 8008, 5))
            .unwrap();
        let mut samples = bounded_source(provider, extent, phase, count(new, new + length));
        for (offset, sample) in samples.iter_mut().enumerate() {
            let label = boundary(old) + i64::try_from(offset).unwrap();
            if label >= boundary(owner + extent) {
                *sample = [0.; 2];
            }
        }
        result.extend(samples);
    }
    result
}

fn observed(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    oracle: &[[f32; 2]],
) -> Vec<[f32; 2]> {
    let actual = pcm(document, provider, start, oracle.len());
    assert_close(&actual, oracle);
    cold_reverse(document, provider, start, &actual);
    actual
}

#[test]
fn three_cuts_in_one_source_move_left_and_right_with_a_real_extra_continuing_sample() {
    let before = document(ntsc(), &["voice"], vec![("voice", source(ntsc(), 9))]);
    let mut provider = Provider::new();
    let oracle = source_islands(
        &provider,
        &[
            (0, 0, 2, 0, 9),
            (4, 2, 2, 0, 9),
            (2, 4, 2, 0, 9),
            (6, 6, 3, 0, 9),
        ],
    );
    assert_eq!(count(4, 6), count(2, 4) + 1);
    let extra = bounded_source(&provider, 9, ExactRatio::integer(6406), 1)[0];
    assert!(extra.iter().any(|value| value.abs() > 0.001));
    assert_eq!(oracle[usize::try_from(boundary(6) - 1).unwrap()], extra);
    for (range, at, name) in [(2..4, 6, "same-right"), (4..6, 2, "same-left")] {
        let destination = interior("root", "voice", at);
        let query = before
            .range_move(
                &id("root"),
                FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap(),
                &destination,
            )
            .unwrap();
        assert_eq!(query.required_ids, 7);
        let after = moved(&before, "root", range, destination, name);
        observed(&after, &mut provider, 0, &oracle);
    }
}

fn cross_fixture() -> ProjectDocument {
    document(
        ntsc(),
        &["lead", "a", "b", "tail"],
        vec![
            ("lead", source(ntsc(), 1)),
            ("a", BeatNode::sequence("A", vec![id("voice")])),
            ("voice", source(ntsc(), 7)),
            ("b", BeatNode::sequence("B", vec![id("target")])),
            ("target", source(ntsc(), 4)),
            ("tail", source(ntsc(), 2)),
        ],
    )
}

#[test]
fn cross_parent_moves_preserve_every_source_sample_entry_in_both_directions() {
    let before = cross_fixture();
    let mut provider = Provider::new();
    let right = moved(
        &before,
        "a",
        2..4,
        interior("b", "target", 3),
        "cross-right",
    );
    let oracle = source_islands(
        &provider,
        &[
            (0, 0, 1, 0, 1),
            (1, 1, 1, 1, 7),
            (4, 2, 4, 1, 7),
            (8, 6, 3, 8, 4),
            (2, 9, 2, 1, 7),
            (11, 11, 1, 8, 4),
            (12, 12, 2, 12, 2),
        ],
    );
    assert_eq!(count(9, 11), count(2, 4) + 1);
    observed(&right, &mut provider, 0, &oracle);
    let left = moved(&before, "b", 9..11, interior("a", "voice", 3), "cross-left");
    let oracle = source_islands(
        &provider,
        &[
            (0, 0, 1, 0, 1),
            (1, 1, 3, 1, 7),
            (9, 4, 2, 8, 4),
            (4, 6, 4, 1, 7),
            (8, 10, 1, 8, 4),
            (11, 11, 1, 8, 4),
            (12, 12, 2, 12, 2),
        ],
    );
    observed(&left, &mut provider, 0, &oracle);
}

#[test]
fn whole_owner_move_allocates_the_new_terminal_sample_but_reports_exhausted_old_support() {
    let before = document(
        ntsc(),
        &["lead", "voice", "tail"],
        vec![
            ("lead", source(ntsc(), 1)),
            ("voice", source(ntsc(), 2)),
            ("tail", source(ntsc(), 5)),
        ],
    );
    let after = moved(
        &before,
        "root",
        1..3,
        interior("root", "tail", 3),
        "whole-owner-extra",
    );
    let mut provider = Provider::new();
    let oracle = source_islands(
        &provider,
        &[
            (0, 0, 1, 0, 1),
            (3, 1, 3, 3, 5),
            (1, 4, 2, 1, 2),
            (6, 6, 2, 3, 5),
        ],
    );
    assert_eq!(count(4, 6), count(1, 3) + 1);
    let actual = observed(&after, &mut provider, 0, &oracle);
    let last = AudioSample(boundary(6) - 1);
    assert_eq!(actual[usize::try_from(last.0).unwrap()], [0.; 2]);
    let mut cold = renderer(&after, &mut provider);
    let terminal = cold
        .read(&mut provider, last, 1, TIMEOUT, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(terminal.samples, vec![[0.; 2]]);
    assert_eq!(terminal.suppressed, vec![last..AudioSample(last.0 + 1)]);
    assert_eq!(after.nodes()[&id("voice")], before.nodes()[&id("voice")]);
}

#[test]
fn prior_pause_delete_bindings_survive_another_move_without_restarting_the_source() {
    let before = prior_edits();
    let after = moved(
        &before,
        "root",
        4..6,
        MoveRangeDestination::Seam {
            parent: id("root"),
            index: 0,
        },
        "historical-move",
    );
    let mut provider = Provider::new();
    let oracle = bounded_source(&provider, 7, ratio(32032, 5), count(0, 2));
    assert_close(
        &pcm(&before, &mut provider, boundary(4), oracle.len()),
        &oracle,
    );
    observed(&after, &mut provider, 0, &oracle);
    let suffix = pcm(&before, &mut provider, boundary(6), count(6, 12));
    cold_reverse(&after, &mut provider, boundary(6), &suffix);
}

#[test]
fn full_repeat_families_and_preserve_contexts_move_without_changing_their_prepared_pcm() {
    let before = repeated_history();
    let after = moved(
        &before,
        "root",
        5..19,
        MoveRangeDestination::Seam {
            parent: id("root"),
            index: 0,
        },
        "repeat-move",
    );
    assert_eq!(after.nodes()[&id("repeat")], before.nodes()[&id("repeat")]);
    assert_eq!(after.overrides(), before.overrides());
    assert_eq!(after.gap_overrides(), before.gap_overrides());
    let mut provider = Provider::new();
    let saved = pcm(&before, &mut provider, boundary(5), count(5, 19));
    assert_close(&saved[..128], &expected(&provider, ExactRatio::ZERO, 128));
    let room = room_reference(&provider, 100..321, 1602);
    assert_close(
        &saved[3203..3331],
        &sample_reference(&room, ratio(-1, 5), ExactRatio::ONE, 128),
    );
    observed(&after, &mut provider, 0, &saved);
    // The displaced prefix's internal boundaries change allocation widths.
    // Check each pre-existing owner entry rather than concatenate rounded spans.
    for old in [0, 1, 4] {
        let entry = pcm(&before, &mut provider, boundary(old), 128);
        check_reads(&after, &mut provider, boundary(old + 14), &entry);
    }
    let suffix = pcm(&before, &mut provider, boundary(19), count(19, 25));
    cold_reverse(&after, &mut provider, boundary(19), &suffix);

    let before = document(
        ntsc(),
        &["lead", "stage", "tail"],
        vec![
            ("lead", source(ntsc(), 5)),
            ("stage", preserve("voice", 4, 12)),
            ("voice", source(ntsc(), 4)),
            ("tail", source(ntsc(), 5)),
        ],
    );
    let after = moved(
        &before,
        "root",
        5..17,
        MoveRangeDestination::Seam {
            parent: id("root"),
            index: 0,
        },
        "preserve-move",
    );
    assert_eq!(after.nodes()[&id("stage")], before.nodes()[&id("stage")]);
    let full = preserve_reference(&provider);
    let oracle = &full[..count(0, 12)];
    assert_close(
        &pcm(&before, &mut provider, boundary(5), oracle.len()),
        oracle,
    );
    observed(&after, &mut provider, 0, oracle);
    let prefix = pcm(&before, &mut provider, 0, count(0, 5));
    cold_reverse(&after, &mut provider, boundary(12), &prefix);
    let suffix = pcm(&before, &mut provider, boundary(17), count(17, 22));
    cold_reverse(&after, &mut provider, boundary(17), &suffix);
}

#[test]
fn moved_voice_keeps_its_gain_clock_and_uses_live_destination_gain_once() {
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ExactRatio::ZERO, ExactRatio::integer(7)).unwrap(),
        GainDb::new(-6000).unwrap(),
        vec![
            GainSegment::new(
                ExactRatio::integer(7),
                GainDb::new(6000).unwrap(),
                GainCurve::Linear,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let before = cross_fixture();
    let mut wire = serde_json::to_value(&before).unwrap();
    for (node, treatment) in [
        ("a", gain(6000, vec![])),
        ("b", gain(-6000, vec![])),
        ("voice", gain(-3000, vec![envelope])),
    ] {
        wire["nodes"][node]["audio_treatments"] = serde_json::to_value(treatment).unwrap();
    }
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = moved(&before, "a", 2..4, interior("b", "target", 3), "gain-move");
    let mut provider = Provider::new();
    // Interior windows avoid newly authored automatic edge fades. The moved
    // physical owner keeps its original gain clock while A/B remain live.
    for (old_frame, new_frame, ancestor_millidb) in [(2, 9, -6000.), (4, 2, 6000.)] {
        let offset = 512;
        let at = boundary(new_frame) + offset;
        let old_at = boundary(old_frame) + offset;
        let raw = bounded_source(&provider, 7, ratio(i128::from(old_at) * 5 - 8008, 5), 128);
        let oracle: Vec<_> = raw
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                let local = ratio(
                    i128::from(old_at + i64::try_from(index).unwrap()) * 5 - 8008,
                    5,
                );
                let progress = local
                    .checked_div(ratio(56056, 5))
                    .unwrap()
                    .checked_mul(ExactRatio::integer(
                        i64::try_from(GAIN_NUMERIC_SCALE).unwrap(),
                    ))
                    .unwrap()
                    .round_even()
                    .unwrap();
                let millidb =
                    ancestor_millidb - 9000. + 12000. * progress as f64 / GAIN_NUMERIC_SCALE as f64;
                sample.map(|value| (f64::from(value) * 10_f64.powf(millidb / 20000.)) as f32)
            })
            .collect();
        assert_close(&authored(&after, &mut provider, at, 128), &oracle);
    }
}

fn silent_picture(duration: i64) -> BeatNode {
    let mut node = source(ntsc(), duration);
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    // A picture-only Source supplies time without introducing a silent Hold's
    // root-sound gate. The shared synthetic asset already admits still images.
    source.video = SourceVideo::Still {
        asset: AssetId::new("media").unwrap(),
    };
    source.audio = None;
    source.audio_mapping = SourceAudioMapping::FitBeat;
    node
}

fn sounded(document: &ProjectDocument, start: i64, length: i64) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    let qualified = ProjectDocument::from_json(&wire.to_string()).unwrap();
    edit(
        &qualified,
        "placed-sound",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event: SoundEvent {
                owner: id("root"),
                label: "Root clock".into(),
                source: audio(100..20_100),
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::integer(start),
                    frames: ratio(2_000_000, 147_147),
                    selection: ExactFrameRange::new(
                        ExactRatio::integer(start),
                        ExactRatio::integer(start + length),
                    )
                    .unwrap(),
                },
                offset: AudioSample(0),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    )
}

fn authored_all(document: &ProjectDocument, provider: &mut Provider) -> Vec<[f32; 2]> {
    let mut result = Vec::new();
    let mut reader = renderer(document, provider);
    let end = boundary(document.duration().unwrap().frames());
    for start in (0..end).step_by(256) {
        result.extend(
            reader
                .prepare_authored_bus(
                    provider,
                    AudioSample(start),
                    u32::try_from((end - start).min(256)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    result
}

#[test]
fn root_sound_stays_at_frames_one_to_three_when_a_moves_to_the_end() {
    let before = sounded(
        &document(
            ntsc(),
            &["a", "b", "c"],
            vec![
                ("a", silent_picture(10)),
                ("b", silent_picture(10)),
                ("c", silent_picture(10)),
            ],
        ),
        1,
        2,
    );
    let after = moved(
        &before,
        "root",
        0..10,
        MoveRangeDestination::Seam {
            parent: id("root"),
            index: 3,
        },
        "sound-stays",
    );
    let mut provider = Provider::new();
    let old = authored_all(&before, &mut provider);
    let new = authored_all(&after, &mut provider);
    assert_eq!(new, old);
    let a = usize::try_from(boundary(1)).unwrap();
    let b = usize::try_from(boundary(3)).unwrap();
    assert!(new[..a].iter().all(|sample| *sample == [0.; 2]));
    assert!(new[a..b].iter().flatten().any(|value| value.abs() > 0.001));
    assert!(new[b..].iter().all(|sample| *sample == [0.; 2]));
    // The sound retains natural rate with an independently selected two-frame
    // interval. It must not follow A to frame21.
    let phase = ratio(100, 1)
        .checked_add(
            ratio(i128::from(boundary(1)) * 5 - 8008, 5)
                .checked_mul(ratio(147, 160))
                .unwrap(),
        )
        .unwrap();
    let reference = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                100..20_100,
                phase,
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(i64::try_from(b - a).unwrap()),
            )
            .unwrap(),
            AudioSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    assert_close(&new[a..a + 128], &reference);
}

#[test]
fn existing_root_sound_routes_and_sample_labels_are_unchanged_by_left_and_right_moves() {
    let original = sounded(
        &document(
            ntsc(),
            &["a", "b", "c"],
            vec![
                ("a", silent_picture(5)),
                ("b", silent_picture(5)),
                ("c", silent_picture(5)),
            ],
        ),
        0,
        12,
    );
    let before = cut(&original, 2..3, "routed-sound-cut");
    assert!(!before.sound_routes().is_empty());
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ExactRatio::ZERO, ExactRatio::integer(14)).unwrap(),
        GainDb::new(-6000).unwrap(),
        vec![
            GainSegment::new(
                ExactRatio::integer(14),
                GainDb::new(6000).unwrap(),
                GainCurve::Linear,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["root"]["audio_treatments"] =
        serde_json::to_value(gain(0, vec![envelope])).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
        unreachable!()
    };
    let mut provider = Provider::new();
    let oracle = authored_all(&before, &mut provider);
    assert!(oracle.iter().flatten().any(|value| value.abs() > 0.001));
    for (selected, destination, name) in [
        (
            0..4,
            MoveRangeDestination::Seam {
                parent: id("root"),
                index: children.len(),
            },
            "routed-right",
        ),
        (
            9..14,
            MoveRangeDestination::Seam {
                parent: id("root"),
                index: 0,
            },
            "routed-left",
        ),
    ] {
        let after = moved(&before, "root", selected, destination, name);
        assert_eq!(authored_all(&after, &mut provider), oracle);
        for at in [boundary(10) + 17, boundary(1) + 19, boundary(6) + 23] {
            assert_eq!(
                authored(&after, &mut provider, at, 128),
                oracle[usize::try_from(at).unwrap()..usize::try_from(at).unwrap() + 128]
            );
        }
    }
}

#[test]
fn root_sound_recipe_stays_fixed_while_live_hold_gates_and_grants_follow_the_hold() {
    let base = sounded(
        &document(
            ntsc(),
            &["a", "pause", "b"],
            vec![
                ("a", silent_picture(5)),
                ("pause", silence(2)),
                ("b", silent_picture(5)),
            ],
        ),
        0,
        12,
    );
    let issuer = SoundHoldIssuer::Node {
        instance: InstancePath {
            node: id("pause"),
            repeats: vec![],
        },
    };
    let allowed = edit(
        &base,
        "allow-pause",
        Command::SetSoundAllowance {
            sound: SoundId::new("effect").unwrap(),
            issuer: issuer.clone(),
            allowed: true,
        },
    );
    let mut provider = Provider::new();
    for (before, name, permitted) in [(&base, "gate-move", false), (&allowed, "grant-move", true)] {
        let after = moved(
            before,
            "root",
            5..7,
            MoveRangeDestination::Seam {
                parent: id("root"),
                index: 0,
            },
            name,
        );
        assert_eq!(after.sound_allowances(), before.sound_allowances());
        let old = authored_all(before, &mut provider);
        let new = authored_all(&after, &mut provider);
        if permitted {
            assert_eq!(new, old);
            assert!(
                new[..count(0, 2)]
                    .iter()
                    .flatten()
                    .any(|value| value.abs() > 0.001)
            );
            assert!(after.sound_allowances()[&SoundId::new("effect").unwrap()].contains(&issuer));
        } else {
            assert!(new[..count(0, 2)].iter().all(|sample| *sample == [0.; 2]));
            assert!(
                old[usize::try_from(boundary(5)).unwrap()..usize::try_from(boundary(7)).unwrap()]
                    .iter()
                    .all(|sample| *sample == [0.; 2])
            );
            assert!(
                new[usize::try_from(boundary(5)).unwrap()..usize::try_from(boundary(7)).unwrap()]
                    .iter()
                    .flatten()
                    .any(|value| value.abs() > 0.001)
            );
            // Away from either old/new live gate, integral root labels are exact.
            for (start, end) in [(2, 5), (7, 12)] {
                let range = usize::try_from(boundary(start) + 192).unwrap()
                    ..usize::try_from(boundary(end) - 192).unwrap();
                assert_eq!(new[range.clone()], old[range]);
            }
        }
    }
}
