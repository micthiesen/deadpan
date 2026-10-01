use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::json;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn duration(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}
fn play(allocation: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new(allocation).unwrap(),
        ordinal,
    }
}
fn instance(node: &str, repeats: &[(&str, &str, u32)]) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: repeats
            .iter()
            .map(|(node, allocation, ordinal)| RepeatInstance {
                node: id(node),
                iteration: play(allocation, *ordinal),
            })
            .collect(),
    }
}
fn node(frames: i64, kind: FrozenAudioKind) -> FrozenAudioNode {
    FrozenAudioNode {
        duration: duration(frames),
        editorial_edges: Default::default(),
        edges: Default::default(),
        kind,
    }
}
fn hold(frames: i64) -> FrozenAudioNode {
    node(
        frames,
        FrozenAudioKind::Hold {
            audio: ReferenceAudibility::Silence,
        },
    )
}
fn source(frames: i64) -> FrozenAudioNode {
    node(
        frames,
        FrozenAudioKind::Source {
            placement: Some(ExactFrameRange::new(ratio(-1, 3), ratio(20, 3)).unwrap()),
        },
    )
}
fn sequence(frames: i64, children: &[&str]) -> FrozenAudioNode {
    node(
        frames,
        FrozenAudioKind::Sequence {
            children: children.iter().map(|name| id(name)).collect(),
        },
    )
}
fn repeat(frames: i64, child: &str, allocation: &str, plays: u32, gap: i64) -> FrozenAudioNode {
    node(
        frames,
        FrozenAudioKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new(allocation).unwrap(), plays).unwrap(),
            gap_duration: duration(gap),
            gap_audio: ReferenceAudibility::Silence,
        },
    )
}
fn retime(
    frames: i64,
    child: &str,
    start: i64,
    end: i64,
    pitch: PitchPolicy,
    purpose: RetimePurpose,
) -> FrozenAudioNode {
    node(
        frames,
        FrozenAudioKind::Retime {
            child: id(child),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose,
        },
    )
}
fn layout(
    nodes: impl IntoIterator<Item = (&'static str, FrozenAudioNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> FrozenAudioLayout {
    let nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    FrozenAudioLayout::from_json(
        &json!({
            "root": id("root"),
            "rate": FrameRate::new(30_000, 1001).unwrap(),
            "nodes": nodes,
            "overrides": overrides,
        })
        .to_string(),
    )
    .unwrap()
}

#[test]
fn repeated_sources_and_holds_have_clipped_full_and_hidden_allocations() {
    for leaf in [source(5), hold(5)] {
        for purpose in [RetimePurpose::Partition, RetimePurpose::Edit] {
            let layout = layout(
                [
                    ("root", sequence(9, &["crop"])),
                    (
                        "crop",
                        retime(9, "repeat", 3, 12, PitchPolicy::Preserve, purpose),
                    ),
                    ("repeat", repeat(20, "leaf", "plays", 4, 0)),
                    ("leaf", leaf.clone()),
                ],
                BTreeMap::new(),
            );
            for (ordinal, expected) in [
                (0, Some(ratio(3, 1)..ratio(5, 1))),
                (1, Some(ExactRatio::ZERO..ratio(5, 1))),
                (2, Some(ExactRatio::ZERO..ratio(2, 1))),
                (3, None),
            ] {
                let scope = instance("leaf", &[("repeat", "plays", ordinal)]);
                let (projection, allocation) = layout
                    .project_scoped_with_allocation(layout.root(), &scope, None, 20)
                    .unwrap();
                assert_eq!(allocation, expected);
                assert_eq!(projection.origin, ratio(i128::from(ordinal) * 5 - 3, 1));
                assert_eq!(projection.frames_per_local_frame, ExactRatio::ONE);
                assert_eq!(projection.local_duration, duration(5));
                assert_eq!(
                    layout
                        .project_scoped(layout.root(), &scope, ExactRatio::ZERO, None, 20)
                        .unwrap(),
                    projection
                );
                let (_, support) = layout
                    .project_scoped_with_support(layout.root(), &scope, None, 20)
                    .unwrap();
                if purpose == RetimePurpose::Partition {
                    assert_eq!(support, ExactRatio::ZERO..ratio(5, 1));
                } else {
                    assert_eq!(
                        Some(support.clone()).filter(|range| range.start != range.end),
                        expected
                    );
                }
            }
        }
    }
}

#[test]
fn nested_intrinsic_partitions_clip_only_inside_the_explicit_definition_root() {
    let layout = layout(
        [
            ("root", sequence(9, &["prefix", "outer"])),
            ("prefix", hold(5)),
            (
                "outer",
                retime(
                    4,
                    "group",
                    4,
                    8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Partition,
                ),
            ),
            ("group", sequence(10, &["lead", "inner"])),
            ("lead", hold(3)),
            (
                "inner",
                retime(
                    7,
                    "leaf",
                    2,
                    9,
                    PitchPolicy::Preserve,
                    RetimePurpose::Partition,
                ),
            ),
            ("leaf", source(10)),
        ],
        BTreeMap::new(),
    );
    for (root, expected_origin, expected_allocation) in [
        ("root", ratio(2, 1), ratio(3, 1)..ratio(7, 1)),
        ("outer", ratio(-3, 1), ratio(3, 1)..ratio(7, 1)),
        ("group", ratio(1, 1), ratio(2, 1)..ratio(9, 1)),
        ("inner", ratio(-2, 1), ratio(2, 1)..ratio(9, 1)),
        ("leaf", ExactRatio::ZERO, ExactRatio::ZERO..ratio(10, 1)),
    ] {
        let (projection, allocation) = layout
            .project_scoped_with_allocation(&id(root), &instance("leaf", &[]), None, 20)
            .unwrap();
        assert_eq!(projection.origin, expected_origin);
        assert_eq!(allocation, Some(expected_allocation));
    }
    let (output, allocation) = layout
        .project_scoped_with_allocation(&id("inner"), &instance("inner", &[]), None, 1)
        .unwrap();
    assert_eq!(output.origin, ExactRatio::ZERO);
    assert_eq!(allocation, Some(ExactRatio::ZERO..ratio(7, 1)));
    assert_eq!(
        layout
            .project_scoped_with_support(layout.root(), &instance("leaf", &[]), None, 20)
            .unwrap()
            .1,
        ExactRatio::ZERO..ratio(10, 1)
    );
}

#[test]
fn follow_speed_keeps_signed_fractional_origin_and_physical_local_allocation() {
    let layout = layout(
        [
            ("root", sequence(1, &["crop"])),
            (
                "crop",
                retime(
                    1,
                    "tape",
                    4,
                    5,
                    PitchPolicy::Preserve,
                    RetimePurpose::Partition,
                ),
            ),
            (
                "tape",
                retime(
                    10,
                    "leaf",
                    1,
                    14,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
            ("leaf", source(17)),
        ],
        BTreeMap::new(),
    );
    let scope = instance("leaf", &[]);
    let (projection, allocation) = layout
        .project_scoped_with_allocation(layout.root(), &scope, None, 20)
        .unwrap();
    assert_eq!(projection.origin, ratio(-62, 13));
    assert_eq!(projection.point, projection.origin);
    assert_eq!(projection.frames_per_local_frame, ratio(10, 13));
    assert_eq!(allocation, Some(ratio(31, 5)..ratio(15, 2)));
    assert_eq!(
        layout
            .project_scoped(layout.root(), &scope, ratio(-3, 2), None, 20)
            .unwrap()
            .point,
        ratio(-77, 13)
    );
    assert_eq!(
        layout
            .project_scoped_with_support(layout.root(), &scope, None, 20)
            .unwrap()
            .1,
        ExactRatio::ONE..ratio(14, 1)
    );
}

#[test]
fn nested_real_gaps_keep_local_duration_and_hidden_gaps_are_not_endpoints() {
    for (start, end, expected) in [
        (
            6,
            14,
            [
                Some(ExactRatio::ONE..ratio(3, 1)),
                Some(ExactRatio::ZERO..ExactRatio::ONE),
                None,
            ],
        ),
        (8, 13, [None, None, None]),
    ] {
        let cropped_duration = end - start;
        let layout = layout(
            [
                ("root", sequence(cropped_duration * 2 + 2, &["outer"])),
                (
                    "outer",
                    repeat(cropped_duration * 2 + 2, "crop", "outer-plays", 2, 2),
                ),
                (
                    "crop",
                    retime(
                        cropped_duration,
                        "inner",
                        start,
                        end,
                        PitchPolicy::Preserve,
                        RetimePurpose::Partition,
                    ),
                ),
                ("inner", repeat(29, "leaf", "inner-plays", 4, 3)),
                ("leaf", hold(5)),
            ],
            BTreeMap::new(),
        );
        let scope = instance("inner", &[("outer", "outer-plays", 1)]);
        for (ordinal, allocation) in (0..3).zip(expected) {
            let after = play("inner-plays", ordinal);
            let (projection, actual) = layout
                .project_scoped_with_allocation(layout.root(), &scope, Some(&after), 20)
                .unwrap();
            assert_eq!(actual, allocation);
            assert_eq!(projection.gap_after, Some(after));
            assert_eq!(projection.instance, scope);
            assert_eq!(projection.local_duration, duration(3));
            assert_eq!(
                projection.origin,
                ratio(
                    i128::from(cropped_duration + 2 + 8 * i64::from(ordinal) + 5 - start),
                    1
                )
            );
        }
        assert!(
            layout
                .project_scoped_with_allocation(
                    layout.root(),
                    &scope,
                    Some(&play("inner-plays", 3)),
                    20
                )
                .is_err()
        );
    }
}

#[test]
fn fractional_gap_allocation_and_stable_gap_identity_survive_reordering() {
    let make_layout = |moved| {
        let mut repeated = repeat(21, "leaf", "plays", 3, 3);
        if moved {
            let FrozenAudioKind::Repeat { iterations, .. } = &mut repeated.kind else {
                unreachable!()
            };
            *iterations = iterations.moved(0, 1, 2).unwrap();
        }
        layout(
            [
                ("root", sequence(1, &["crop"])),
                (
                    "crop",
                    retime(
                        1,
                        "tape",
                        4,
                        5,
                        PitchPolicy::Preserve,
                        RetimePurpose::Partition,
                    ),
                ),
                (
                    "tape",
                    retime(
                        10,
                        "repeat",
                        0,
                        13,
                        PitchPolicy::FollowSpeed,
                        RetimePurpose::Edit,
                    ),
                ),
                ("repeat", repeated),
                ("leaf", hold(5)),
            ],
            BTreeMap::new(),
        )
    };
    let frozen = make_layout(false);
    let scope = instance("repeat", &[]);
    let after = play("plays", 0);
    let (projection, allocation) = frozen
        .project_scoped_with_allocation(frozen.root(), &scope, Some(&after), 20)
        .unwrap();
    assert_eq!(projection.origin, ratio(-2, 13));
    assert_eq!(projection.frames_per_local_frame, ratio(10, 13));
    assert_eq!(allocation, Some(ratio(1, 5)..ratio(3, 2)));
    assert_eq!(projection.gap_after, Some(after.clone()));
    let current = make_layout(true);
    assert!(
        current
            .project_scoped_with_allocation(current.root(), &scope, Some(&after), 20)
            .is_err()
    );
    let former_final = play("plays", 2);
    let (reordered, allocation) = current
        .project_scoped_with_allocation(&id("repeat"), &scope, Some(&former_final), 20)
        .unwrap();
    assert_eq!(reordered.origin, ratio(13, 1));
    assert_eq!(reordered.gap_after, Some(former_final));
    assert_eq!(allocation, Some(ExactRatio::ZERO..ratio(3, 1)));
    assert_eq!(
        frozen
            .project_scoped_with_allocation(frozen.root(), &scope, Some(&after), 20)
            .unwrap(),
        (projection, Some(ratio(1, 5)..ratio(3, 2)))
    );
}

#[test]
fn opaque_preserve_requires_its_input_clock_or_its_output_occurrence() {
    let layout = layout(
        [
            ("root", sequence(26, &["preserve"])),
            (
                "preserve",
                retime(
                    26,
                    "repeat",
                    0,
                    13,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("repeat", repeat(13, "leaf", "plays", 2, 3)),
            ("leaf", hold(5)),
        ],
        BTreeMap::new(),
    );
    for (scope, gap_after) in [
        (instance("leaf", &[("repeat", "plays", 0)]), None),
        (instance("repeat", &[]), Some(play("plays", 0))),
    ] {
        for root in [layout.root(), &id("preserve")] {
            assert!(
                layout
                    .project_scoped_with_allocation(root, &scope, gap_after.as_ref(), 20)
                    .unwrap_err()
                    .message
                    .contains("opaque Preserve")
            );
        }
        assert!(
            layout
                .project_scoped_with_allocation(&id("repeat"), &scope, gap_after.as_ref(), 20)
                .unwrap()
                .1
                .is_some()
        );
        assert_eq!(
            layout
                .project_scoped(
                    layout.root(),
                    &scope,
                    ExactRatio::ZERO,
                    gap_after.as_ref(),
                    20
                )
                .unwrap()
                .frames_per_local_frame,
            ratio(2, 1)
        );
    }
    let (output, allocation) = layout
        .project_scoped_with_allocation(layout.root(), &instance("preserve", &[]), None, 20)
        .unwrap();
    assert_eq!(output.frames_per_local_frame, ExactRatio::ONE);
    assert_eq!(allocation, Some(ExactRatio::ZERO..ratio(26, 1)));
}

#[test]
fn billion_play_sparse_projection_charges_exact_compact_work() {
    let make_layout = |plays| {
        let frames = i64::from(plays) * 8 + 3;
        layout(
            [
                ("root", sequence(frames, &["repeat"])),
                ("repeat", repeat(frames, "leaf", "plays", plays, 3)),
                ("leaf", source(5)),
                ("override", hold(11)),
            ],
            BTreeMap::from([(
                id("repeat"),
                PlayOverrides::try_from(vec![PlayOverride {
                    iteration: play("plays", 4),
                    root: id("override"),
                }])
                .unwrap(),
            )]),
        )
    };
    let small = make_layout(9);
    let large = make_layout(1_000_000_000);
    for scope in [
        instance("leaf", &[("repeat", "plays", 5)]),
        instance("override", &[("repeat", "plays", 4)]),
    ] {
        let expected = small
            .project_scoped_with_allocation(small.root(), &scope, None, 20)
            .unwrap();
        assert_eq!(expected.0.work, 6);
        assert_eq!(
            large
                .project_scoped_with_allocation(large.root(), &scope, None, expected.0.work)
                .unwrap(),
            expected
        );
        for budget in [0, expected.0.work - 1, MAX_DOCUMENT_NODES + 1] {
            assert_eq!(
                large
                    .project_scoped_with_allocation(large.root(), &scope, None, budget)
                    .unwrap_err()
                    .code,
                DocumentErrorCode::LimitExceeded
            );
        }
    }
    let scope = instance("leaf", &[("repeat", "plays", 999_999_999)]);
    let (last, allocation) = large
        .project_scoped_with_allocation(large.root(), &scope, None, 6)
        .unwrap();
    assert_eq!(last.origin, ratio(7_999_999_998, 1));
    assert_eq!(allocation, Some(ExactRatio::ZERO..ratio(5, 1)));
    for wrong in [
        instance("leaf", &[("repeat", "plays", 4)]),
        instance("override", &[("repeat", "plays", 5)]),
        instance("leaf", &[]),
        instance("leaf", &[("repeat", "unknown", 5)]),
    ] {
        assert!(
            large
                .project_scoped_with_allocation(large.root(), &wrong, None, 20)
                .is_err()
        );
    }
}

#[test]
fn empty_root_allocation_is_none() {
    let layout = layout([("root", sequence(0, &[]))], BTreeMap::new());
    let (projection, allocation) = layout
        .project_scoped_with_allocation(layout.root(), &instance("root", &[]), None, 1)
        .unwrap();
    assert_eq!(projection.work, 1);
    assert_eq!(allocation, None);
}
