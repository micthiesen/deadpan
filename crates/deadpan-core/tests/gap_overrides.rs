use std::collections::BTreeMap;

use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(n: u32) -> RevisionId {
    RevisionId::new(format!("r{n}")).unwrap()
}
fn duration(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn recipe(n: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: duration(n),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn subtree(root: &str, n: i64) -> Subtree {
    Subtree {
        root: id(root),
        nodes: BTreeMap::from([(id(root), BeatNode::hold(root, recipe(n)))]),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
    }
}
fn gap_sequence(root: &str, prefix: i64, inserted: i64, suffix: i64) -> Subtree {
    let children = ["prefix", "inserted", "suffix"].map(|part| id(&format!("{root}-{part}")));
    Subtree {
        root: id(root),
        nodes: BTreeMap::from([
            (id(root), BeatNode::sequence(root, children.to_vec())),
            (
                children[0].clone(),
                BeatNode::hold("prefix", recipe(prefix)),
            ),
            (
                children[1].clone(),
                BeatNode::hold("inserted", recipe(inserted)),
            ),
            (
                children[2].clone(),
                BeatNode::hold("suffix", recipe(suffix)),
            ),
        ]),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
    }
}
fn empty_sequence(root: &str) -> Subtree {
    Subtree {
        root: id(root),
        nodes: BTreeMap::from([(id(root), BeatNode::sequence(root, vec![]))]),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
    }
}
fn apply_edit(document: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let next = document.revision_id().as_str()[1..].parse::<u32>().unwrap() + 1;
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    };
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    (after, transaction)
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    apply_edit(document, command).0
}
fn fixture(plays: u32, gap: i64) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("gap-project").unwrap(),
        revision(0),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let inserted = edit(
        &empty,
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: subtree("base", 2),
        },
    );
    edit(
        &inserted,
        Command::WrapRepeat {
            node: id("base"),
            id: id("repeat"),
            plays,
            gap: (gap > 0).then(|| recipe(gap)),
            anchor_policy: WrapAnchorPolicy::First,
        },
    )
}
fn with_source_base(document: &ProjectDocument) -> ProjectDocument {
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
        SourceTimestamp {
            ticks: 10,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
    )
    .unwrap();
    let source = BeatNode {
        label: "Original".into(),
        framing: None,
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(2),
                video: SourceVideo::Stream {
                    asset: AssetId::new("media").unwrap(),
                    span,
                },
                audio: None,
                link: LinkRelation::Independent,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        },
    };
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["base"] = serde_json::to_value(source).unwrap();
    wire["assets"]["media"] = serde_json::to_value(AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(span),
        audio: None,
        still_image: false,
        frame_count: None,
        source_qualification: None,
    })
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn iteration(document: &ProjectDocument, repeat: &str, index: u32) -> IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id(repeat)].kind else {
        panic!("Repeat expected")
    };
    iterations.at(index).unwrap()
}
fn path(document: &ProjectDocument, node: &str, repeat: &str, index: u32) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: vec![RepeatInstance {
            node: id(repeat),
            iteration: iteration(document, repeat, index),
        }],
    }
}
fn point(instance: InstancePath, position: i64) -> AnchorTarget {
    AnchorTarget {
        boundary: BoundaryAnchor {
            coordinate: Anchor::Occurrence {
                instance,
                position: ExactRatio::integer(position),
            },
            bias: InsertionBias::Right,
        },
        occurrence: None,
    }
}
fn project(document: &ProjectDocument, instance: InstancePath, position: i64) -> ProjectFrame {
    AnchorIndex::new(document)
        .unwrap()
        .resolve_target(&point(instance, position))
        .unwrap()
        .frame
}

fn mark(
    document: &ProjectDocument,
    name: &str,
    owner: &str,
    coordinate: Anchor,
) -> ProjectDocument {
    edit(
        document,
        Command::SetMark {
            id: MarkId::new(name).unwrap(),
            owner: id(owner),
            label: name.into(),
            boundary: BoundaryAnchor {
                coordinate,
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
}

#[test]
fn set_and_clear_gap_branch_preserve_default_and_play_child() {
    let before = fixture(3, 2);
    let selected = iteration(&before, "repeat", 0);
    let changed = edit(
        &before,
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: selected.clone(),
            subtree: gap_sequence("custom-gap", 1, 2, 1),
        },
    );
    assert_eq!(before.duration().unwrap(), duration(10));
    assert_eq!(changed.duration().unwrap(), duration(12));
    assert_eq!(
        changed.gap_overrides()[&id("repeat")].get(&selected),
        Some(&id("custom-gap"))
    );
    assert_eq!(
        project(
            &changed,
            path(&changed, "custom-gap-inserted", "repeat", 0),
            1
        ),
        ProjectFrame(4)
    );
    assert_eq!(
        project(&changed, path(&changed, "base", "repeat", 1), 0),
        ProjectFrame(6)
    );
    assert_eq!(
        project(&changed, path(&changed, "base", "repeat", 2), 0),
        ProjectFrame(10)
    );
    let NodeKind::Repeat { gap, .. } = &changed.nodes()[&id("repeat")].kind else {
        panic!()
    };
    assert_eq!(gap.as_ref().unwrap().duration, duration(2));
    let cleared = edit(
        &changed,
        Command::ClearGapOverride {
            node: id("repeat"),
            iteration: selected,
        },
    );
    assert_eq!(cleared.duration().unwrap(), before.duration().unwrap());
    assert!(cleared.gap_overrides().is_empty());
    assert!(!cleared.nodes().contains_key(&id("custom-gap")));
}

#[test]
fn empty_override_suppresses_one_gap_and_survives_default_removal() {
    let before = fixture(3, 2);
    let selected = iteration(&before, "repeat", 0);
    let changed = edit(
        &before,
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: selected.clone(),
            subtree: empty_sequence("no-gap"),
        },
    );
    assert_eq!(changed.duration().unwrap(), duration(8));
    assert!(
        path(&changed, "no-gap", "repeat", 0)
            .validate(&changed)
            .is_ok()
    );
    assert_eq!(
        AnchorIndex::new(&changed)
            .unwrap()
            .resolve_target(&point(path(&changed, "no-gap", "repeat", 0), 0))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
    let no_default = edit(
        &changed,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 3,
            gap: None,
        },
    );
    assert_eq!(no_default.duration().unwrap(), duration(6));
    assert_eq!(
        no_default.gap_overrides()[&id("repeat")].get(&selected),
        Some(&id("no-gap"))
    );
    let restored_default = edit(
        &no_default,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 3,
            gap: Some(recipe(3)),
        },
    );
    assert_eq!(restored_default.duration().unwrap(), duration(9));
}

#[test]
fn final_branch_is_dormant_then_reorder_and_growth_reactivate_it() {
    let before = fixture(3, 1);
    let selected = iteration(&before, "repeat", 2);
    let changed = edit(
        &before,
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: selected.clone(),
            subtree: subtree("late-gap", 4),
        },
    );
    assert_eq!(changed.duration().unwrap(), before.duration().unwrap());
    assert!(
        path(&changed, "late-gap", "repeat", 2)
            .validate(&changed)
            .is_ok()
    );
    assert_eq!(
        AnchorIndex::new(&changed)
            .unwrap()
            .resolve_target(&point(path(&changed, "late-gap", "repeat", 2), 1))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
    let moved = edit(
        &changed,
        Command::MovePlays {
            node: id("repeat"),
            start: 2,
            end: 3,
            destination: 0,
        },
    );
    assert_eq!(iteration(&moved, "repeat", 0), selected);
    assert_eq!(
        project(&moved, path(&moved, "late-gap", "repeat", 0), 1),
        ProjectFrame(3)
    );
    assert_eq!(moved.duration().unwrap(), duration(11));
    let back = edit(
        &moved,
        Command::MovePlays {
            node: id("repeat"),
            start: 0,
            end: 1,
            destination: 2,
        },
    );
    assert_eq!(back.duration().unwrap(), before.duration().unwrap());
    let grown = edit(
        &back,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 4,
            gap: Some(recipe(1)),
        },
    );
    assert_eq!(grown.duration().unwrap(), duration(14));
    assert_eq!(
        project(&grown, path(&grown, "late-gap", "repeat", 2), 1),
        ProjectFrame(9)
    );
}

#[test]
fn retiring_preceding_identity_removes_only_its_gap_branch_and_undo_restores_it() {
    let before = fixture(3, 1);
    let selected = iteration(&before, "repeat", 2);
    let changed = edit(
        &before,
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: selected.clone(),
            subtree: subtree("retired-gap", 3),
        },
    );
    let (shrunk, transaction) = apply_edit(
        &changed,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 2,
            gap: Some(recipe(1)),
        },
    );
    assert!(shrunk.gap_overrides().is_empty());
    assert!(!shrunk.nodes().contains_key(&id("retired-gap")));
    assert_eq!(transaction.inverse.apply(&shrunk).unwrap(), changed);
    let regrown = edit(
        &shrunk,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 3,
            gap: Some(recipe(1)),
        },
    );
    assert_ne!(iteration(&regrown, "repeat", 2), selected);
    assert!(regrown.gap_overrides().is_empty());
}

#[test]
fn nested_occurrence_isolation_copies_owned_gap_branch() {
    let original = fixture(2, 1);
    let inner = edit(
        &original,
        Command::WrapRepeat {
            node: id("repeat"),
            id: id("outer"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    let inner_first = iteration(&inner, "repeat", 0);
    let with_gap = edit(
        &inner,
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: inner_first.clone(),
            subtree: subtree("inner-gap", 3),
        },
    );
    let second_outer = iteration(&with_gap, "outer", 1);
    let isolated = edit(
        &with_gap,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("repeat"),
                repeats: vec![RepeatInstance {
                    node: id("outer"),
                    iteration: second_outer.clone(),
                }],
            },
            edit: OccurrenceEdit::Rename {
                label: "Selected inner".into(),
            },
            identities: OccurrenceIdentities {
                nodes: (0..16).map(|n| id(&format!("isolated-{n}"))).collect(),
                marks: vec![],
            },
        },
    );
    let copied_inner = isolated.overrides()[&id("outer")]
        .get(&second_outer)
        .unwrap();
    assert_ne!(copied_inner, &id("repeat"));
    let copied_gap = isolated.gap_overrides()[copied_inner]
        .get(&inner_first)
        .unwrap();
    assert_ne!(copied_gap, &id("inner-gap"));
    assert_eq!(isolated.nodes()[copied_inner].label, "Selected inner");
    assert_eq!(
        isolated.nodes()[&id("repeat")].label,
        with_gap.nodes()[&id("repeat")].label
    );
    assert_eq!(
        project(
            &isolated,
            InstancePath {
                node: copied_gap.clone(),
                repeats: vec![
                    RepeatInstance {
                        node: id("outer"),
                        iteration: second_outer
                    },
                    RepeatInstance {
                        node: copied_inner.clone(),
                        iteration: inner_first
                    },
                ],
            },
            1
        ),
        ProjectFrame(10)
    );
}

#[test]
fn split_copies_gap_branch_with_independent_owned_nodes() {
    let before = fixture(3, 2);
    let first = iteration(&before, "repeat", 0);
    let with_gap = edit(
        &before,
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: first.clone(),
            subtree: subtree("gap-owner", 3),
        },
    );
    let after = edit(
        &with_gap,
        Command::Split {
            node: id("repeat"),
            at: duration(3),
            identities: SplitIdentities {
                nodes: (0..12).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    let NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        panic!()
    };
    let NodeKind::Retime { child: right, .. } = &after.nodes()[&children[1]].kind else {
        panic!()
    };
    assert_ne!(right, &id("repeat"));
    let copied = after.gap_overrides()[right].get(&first).unwrap();
    assert_ne!(copied, &id("gap-owner"));
    assert!(after.nodes().contains_key(copied));
    assert_eq!(after.duration().unwrap(), with_gap.duration().unwrap());
}

#[test]
fn arbitrary_gap_replacement_does_not_claim_old_default_gap_marks() {
    let before = fixture(3, 2);
    let before = mark(
        &before,
        "in-old-gap",
        "root",
        Anchor::Local {
            node: id("repeat"),
            position: ExactRatio::integer(3),
        },
    );
    let selected = iteration(&before, "repeat", 0);
    let after = edit(
        &before,
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: selected,
            subtree: subtree("unrelated", 4),
        },
    );
    assert!(matches!(
        after.marks()[&MarkId::new("in-old-gap").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
}

#[test]
fn isolate_gap_retains_existing_marks_and_exact_undo() {
    let before = with_source_base(&fixture(3, 2));
    let before = mark(
        &before,
        "local-gap",
        "root",
        Anchor::Local {
            node: id("repeat"),
            position: ExactRatio::integer(3),
        },
    );
    let before = mark(
        &before,
        "occurrence",
        "base",
        Anchor::Occurrence {
            instance: path(&before, "base", "repeat", 1),
            position: ExactRatio::integer(1),
        },
    );
    let before = mark(
        &before,
        "source",
        "base",
        Anchor::Source {
            asset: AssetId::new("media").unwrap(),
            moment: SourceMoment::Timestamp {
                stream: SourceStream::Video,
                timestamp: SourceTimestamp {
                    ticks: 5,
                    time_base: SourceTimeBase::new(1, 30).unwrap(),
                },
            },
        },
    );
    let selected = iteration(&before, "repeat", 0);
    let (after, transaction) = apply_edit(
        &before,
        Command::IsolateGap {
            node: id("repeat"),
            iteration: selected.clone(),
            id: id("isolated-gap"),
            timing: AudioTimingId {
                allocation: revision(6),
                ordinal: 0,
            },
        },
    );
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.marks(), before.marks());
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        after.gap_overrides()[&id("repeat")].get(&selected),
        Some(&id("isolated-gap"))
    );
    assert_eq!(
        project(&after, path(&after, "isolated-gap", "repeat", 0), 1),
        ProjectFrame(3)
    );
    assert!(
        after
            .audio_bindings()
            .bindings()
            .contains_key(&id("isolated-gap"))
    );
}

#[test]
fn isolate_gap_rejects_final_missing_and_already_isolated_gaps() {
    let before = fixture(2, 1);
    let final_play = iteration(&before, "repeat", 1);
    let first = iteration(&before, "repeat", 0);
    let isolate =
        |document: &ProjectDocument, iteration: IterationId, name: &str| Command::IsolateGap {
            node: id("repeat"),
            iteration,
            id: id(name),
            timing: AudioTimingId {
                allocation: revision(
                    document.revision_id().as_str()[1..].parse::<u32>().unwrap() + 1,
                ),
                ordinal: 0,
            },
        };
    let request = |document: &ProjectDocument, command| {
        let next = document.revision_id().as_str()[1..].parse::<u32>().unwrap() + 1;
        CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(next),
            command,
        }
    };
    assert!(
        apply(
            &before,
            &request(&before, isolate(&before, final_play, "final-gap"))
        )
        .is_err()
    );
    let first_isolated = edit(&before, isolate(&before, first.clone(), "first-gap"));
    assert!(
        apply(
            &first_isolated,
            &request(
                &first_isolated,
                isolate(&first_isolated, first.clone(), "again")
            )
        )
        .is_err()
    );
    let no_gap = edit(
        &before,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 2,
            gap: None,
        },
    );
    assert!(
        apply(
            &no_gap,
            &request(&no_gap, isolate(&no_gap, first, "no-gap"))
        )
        .is_err()
    );
}
