use std::collections::BTreeMap;

use deadpan_core::*;

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
fn recipe(frames: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: duration(frames),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold("hold", recipe(frames))
}
fn repeat(child: &str, allocation: &str, plays: u32, gap: i64) -> BeatNode {
    BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "repeat".into(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new(allocation).unwrap(), plays).unwrap(),
            gap: (gap > 0).then(|| recipe(gap)),
            escalation: None,
        },
    }
}
fn retime(
    child: &str,
    frames: i64,
    start: i64,
    end: i64,
    pitch: PitchPolicy,
    purpose: RetimePurpose,
) -> BeatNode {
    BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "retime".into(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose,
        },
    }
}
fn document(
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("gap-projection").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("root", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn sparse(plays: u32) -> ProjectDocument {
    document(
        &["prefix", "repeat"],
        [
            ("prefix", hold(1)),
            ("repeat", repeat("child", "plays", plays, 3)),
            ("child", hold(5)),
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
}
fn moved(document: &ProjectDocument, start: u32, end: u32, destination: u32) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
            command: Command::MovePlays {
                node: id("repeat"),
                start,
                end,
                destination,
            },
        },
    )
    .unwrap();
    transaction.forward.apply(document).unwrap()
}

#[test]
fn gap_support_uses_actual_duration_after_sparse_unequal_plays_at_ntsc() {
    let layout = FrozenAudioLayout::capture(&sparse(1_000_000_000)).unwrap();
    let scope = instance("repeat", &[]);
    let after = play("plays", 4);
    let (projection, support) = layout
        .project_scoped_with_support(layout.root(), &scope, Some(&after), 20)
        .unwrap();
    assert_eq!(projection.origin, ratio(44, 1));
    assert_eq!(projection.point, projection.origin);
    assert_eq!(projection.frames_per_local_frame, ExactRatio::ONE);
    assert_eq!(projection.local_duration, duration(3));
    assert_eq!(projection.instance, scope);
    assert_eq!(projection.gap_after.as_ref(), Some(&after));
    assert_eq!(support, ExactRatio::ZERO..ratio(3, 1));
    assert!(projection.work <= 8);
    assert_eq!(
        layout.rate().audio_boundary(ProjectFrame(44)).unwrap(),
        AudioSample(70_470)
    );
    assert_eq!(
        layout.rate().audio_boundary(ProjectFrame(47)).unwrap(),
        AudioSample(75_275)
    );
    assert_eq!(
        layout
            .project_scoped_with_support(layout.root(), &scope, Some(&after), projection.work)
            .unwrap(),
        (projection.clone(), support.clone())
    );
    assert_eq!(
        layout
            .project_scoped_with_support(layout.root(), &scope, Some(&after), projection.work - 1)
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    assert_eq!(
        FrozenAudioLayout::from_json(&layout.to_json().unwrap())
            .unwrap()
            .project_scoped_with_support(layout.root(), &scope, Some(&after), 20)
            .unwrap(),
        (projection, support)
    );
}

#[test]
fn stable_gap_identity_follows_reordering_and_only_current_nonfinal_plays_have_gaps() {
    let before = sparse(9);
    let frozen = FrozenAudioLayout::capture(&before).unwrap();
    let scope = instance("repeat", &[]);
    let after = play("plays", 4);
    let original = frozen
        .project_scoped_with_support(frozen.root(), &scope, Some(&after), 30)
        .unwrap();
    let current = FrozenAudioLayout::capture(&moved(&before, 4, 5, 0)).unwrap();
    let reordered = current
        .project_scoped_with_support(current.root(), &scope, Some(&after), 30)
        .unwrap();
    assert_eq!(original.0.origin, ratio(44, 1));
    assert_eq!(reordered.0.origin, ratio(12, 1));
    assert_eq!(reordered.0.gap_after, original.0.gap_after);
    assert_eq!(reordered.1, original.1);
    assert_eq!(
        frozen
            .project_scoped_with_support(frozen.root(), &scope, Some(&after), 30)
            .unwrap(),
        original
    );

    let former_final = play("plays", 8);
    assert!(
        frozen
            .project_scoped_with_support(frozen.root(), &scope, Some(&former_final), 30)
            .is_err()
    );
    let current = FrozenAudioLayout::capture(&moved(&before, 8, 9, 0)).unwrap();
    let newly_followed = current
        .project_scoped_with_support(current.root(), &scope, Some(&former_final), 30)
        .unwrap();
    assert_eq!(newly_followed.0.origin, ratio(6, 1));
    assert_eq!(newly_followed.0.gap_after, Some(former_final));
    assert!(
        current
            .project_scoped_with_support(current.root(), &scope, Some(&play("plays", 7)), 30)
            .is_err()
    );
}

#[test]
fn nested_repeat_and_tape_retime_keep_exact_gap_origin_scale_and_local_support() {
    let document = document(
        &["prefix", "outer"],
        [
            ("prefix", hold(2)),
            ("outer", repeat("tape", "outer-plays", 2, 4)),
            (
                "tape",
                retime(
                    "inner",
                    10,
                    2,
                    19,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
            ("inner", repeat("child", "inner-plays", 3, 3)),
            ("child", hold(5)),
        ],
        BTreeMap::new(),
    );
    let layout = FrozenAudioLayout::capture(&document).unwrap();
    let scope = instance("inner", &[("outer", "outer-plays", 1)]);
    let after = play("inner-plays", 1);
    let (projection, support) = layout
        .project_scoped_with_support(layout.root(), &scope, Some(&after), 30)
        .unwrap();
    assert_eq!(projection.origin, ratio(382, 17));
    assert_eq!(projection.point, projection.origin);
    assert_eq!(projection.frames_per_local_frame, ratio(10, 17));
    assert_eq!(support, ExactRatio::ZERO..ratio(3, 1));
    let (local, local_support) = layout
        .project_scoped_with_support(&id("inner"), &instance("inner", &[]), Some(&after), 30)
        .unwrap();
    assert_eq!(local.origin, ratio(13, 1));
    assert_eq!(local.frames_per_local_frame, ExactRatio::ONE);
    assert_eq!(local_support, support);
    for wrong in [
        instance("inner", &[]),
        instance("inner", &[("outer", "outer-plays", 2)]),
        instance("inner", &[("inner", "inner-plays", 0)]),
        instance(
            "inner",
            &[("outer", "outer-plays", 1), ("inner", "inner-plays", 0)],
        ),
    ] {
        assert!(
            layout
                .project_scoped_with_support(layout.root(), &wrong, Some(&after), 30)
                .is_err()
        );
    }
    assert!(
        layout
            .project_scoped_with_support(&id("inner"), &scope, Some(&after), 30)
            .is_err()
    );
}

#[test]
fn partitions_retain_hidden_gap_support_while_edit_crops_clip_in_local_coordinates() {
    for (purpose, start, expected) in [
        (RetimePurpose::Partition, 6, ExactRatio::ZERO..ratio(3, 1)),
        (RetimePurpose::Edit, 6, ExactRatio::ONE..ratio(2, 1)),
        (RetimePurpose::Partition, 0, ExactRatio::ZERO..ratio(3, 1)),
        (RetimePurpose::Edit, 0, ExactRatio::ZERO..ExactRatio::ZERO),
        (RetimePurpose::Partition, 8, ExactRatio::ZERO..ratio(3, 1)),
        (RetimePurpose::Edit, 8, ratio(3, 1)..ratio(3, 1)),
    ] {
        let document = document(
            &["crop"],
            [
                (
                    "crop",
                    retime(
                        "repeat",
                        1,
                        start,
                        start + 1,
                        PitchPolicy::Preserve,
                        purpose,
                    ),
                ),
                ("repeat", repeat("child", "plays", 2, 3)),
                ("child", hold(5)),
            ],
            BTreeMap::new(),
        );
        let layout = FrozenAudioLayout::capture(&document).unwrap();
        let (projection, support) = layout
            .project_scoped_with_support(
                layout.root(),
                &instance("repeat", &[]),
                Some(&play("plays", 0)),
                20,
            )
            .unwrap();
        assert_eq!(projection.origin, ExactRatio::integer(5 - start));
        assert_eq!(projection.local_duration, duration(3));
        assert_eq!(projection.frames_per_local_frame, ExactRatio::ONE);
        assert_eq!(support, expected);
    }
}

#[test]
fn ancestor_crop_composes_fractional_gap_support_through_a_tape_retime() {
    let document = document(
        &["crop"],
        [
            (
                "crop",
                retime(
                    "tape",
                    1,
                    4,
                    5,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "tape",
                retime(
                    "repeat",
                    10,
                    0,
                    13,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
            ("repeat", repeat("child", "plays", 2, 3)),
            ("child", hold(5)),
        ],
        BTreeMap::new(),
    );
    let layout = FrozenAudioLayout::capture(&document).unwrap();
    let (projection, support) = layout
        .project_scoped_with_support(
            layout.root(),
            &instance("repeat", &[]),
            Some(&play("plays", 0)),
            20,
        )
        .unwrap();
    assert_eq!(projection.origin, ratio(-2, 13));
    assert_eq!(projection.frames_per_local_frame, ratio(10, 13));
    assert_eq!(support, ratio(1, 5)..ratio(3, 2));
}

#[test]
fn support_scope_stops_at_nonunity_preserve_but_its_input_and_output_are_independent() {
    let document = document(
        &["preserve"],
        [
            (
                "preserve",
                retime(
                    "repeat",
                    26,
                    0,
                    13,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("repeat", repeat("child", "plays", 2, 3)),
            ("child", hold(5)),
        ],
        BTreeMap::new(),
    );
    let layout = FrozenAudioLayout::capture(&document).unwrap();
    let scope = instance("repeat", &[]);
    let after = play("plays", 0);
    for root in [layout.root(), &id("preserve")] {
        let error = layout
            .project_scoped_with_support(root, &scope, Some(&after), 20)
            .unwrap_err();
        assert!(error.message.contains("opaque Preserve"));
    }
    let (input, support) = layout
        .project_scoped_with_support(&id("repeat"), &scope, Some(&after), 20)
        .unwrap();
    assert_eq!(input.origin, ratio(5, 1));
    assert_eq!(input.frames_per_local_frame, ExactRatio::ONE);
    assert_eq!(support, ExactRatio::ZERO..ratio(3, 1));
    let (output, support) = layout
        .project_scoped_with_support(layout.root(), &instance("preserve", &[]), None, 20)
        .unwrap();
    assert_eq!(output.origin, ExactRatio::ZERO);
    assert_eq!(output.frames_per_local_frame, ExactRatio::ONE);
    assert_eq!(support, ExactRatio::ZERO..ratio(26, 1));
    let affine = layout
        .project_scoped(layout.root(), &scope, ExactRatio::ZERO, Some(&after), 20)
        .unwrap();
    assert_eq!(affine.origin, ratio(10, 1));
    assert_eq!(affine.frames_per_local_frame, ratio(2, 1));
}

#[test]
fn gap_projection_independently_rejects_bad_scope_path_identity_and_budget() {
    let layout = FrozenAudioLayout::capture(&sparse(9)).unwrap();
    let scope = instance("repeat", &[]);
    for budget in [0, MAX_DOCUMENT_NODES + 1] {
        assert_eq!(
            layout
                .project_scoped_with_support(layout.root(), &scope, Some(&play("plays", 0)), budget)
                .unwrap_err()
                .code,
            DocumentErrorCode::LimitExceeded
        );
    }
    for after in [play("plays", 9), play("unknown", 0), play("plays", 8)] {
        assert!(
            layout
                .project_scoped_with_support(layout.root(), &scope, Some(&after), 30)
                .is_err()
        );
    }
    for root in [id("missing"), id("prefix"), id("child")] {
        assert!(
            layout
                .project_scoped_with_support(&root, &scope, Some(&play("plays", 0)), 30)
                .is_err()
        );
    }
    for wrong in [
        instance("missing", &[]),
        instance("prefix", &[]),
        instance("repeat", &[("repeat", "plays", 0)]),
        InstancePath {
            node: id("repeat"),
            repeats: vec![
                RepeatInstance {
                    node: id("repeat"),
                    iteration: play("plays", 0)
                };
                MAX_DOCUMENT_DEPTH + 1
            ],
        },
    ] {
        assert!(
            layout
                .project_scoped_with_support(layout.root(), &wrong, Some(&play("plays", 0)), 30)
                .is_err()
        );
    }
    for wrong in [
        instance("child", &[("repeat", "plays", 4)]),
        instance("override", &[("repeat", "plays", 0)]),
    ] {
        assert!(
            layout
                .project_scoped_with_support(layout.root(), &wrong, None, 30)
                .is_err()
        );
    }
    let no_gap = document(
        &["repeat"],
        [
            ("repeat", repeat("child", "plays", 2, 0)),
            ("child", hold(5)),
        ],
        BTreeMap::new(),
    );
    let no_gap = FrozenAudioLayout::capture(&no_gap).unwrap();
    assert!(
        no_gap
            .project_scoped_with_support(no_gap.root(), &scope, Some(&play("plays", 0)), 30)
            .is_err()
    );
}
