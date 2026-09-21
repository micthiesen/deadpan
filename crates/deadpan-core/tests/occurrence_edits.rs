use std::collections::BTreeMap;

use deadpan_core::*;
use proptest::prelude::*;

fn node(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn mark(s: &str) -> MarkId {
    MarkId::new(s).unwrap()
}
fn duration(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn hold(n: i64) -> HoldRecipe {
    HoldRecipe {
        duration: duration(n),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
        command,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    let request = request(document, command);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let transaction = apply(document, &request).unwrap();
    let next = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&next).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&next.to_json().unwrap()).unwrap(),
        next
    );
    next
}
fn fixture(outer: u32, inner: u32, frames: i64, gap: i64) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        RevisionId::new("r").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let inserted = edit(
        &empty,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("group"),
                overrides: BTreeMap::new(),
                nodes: BTreeMap::from([
                    (
                        node("group"),
                        BeatNode::sequence(
                            "group",
                            vec![node("prefix"), node("inside"), node("tail")],
                        ),
                    ),
                    (node("prefix"), BeatNode::hold("prefix", hold(3))),
                    (
                        node("inside"),
                        BeatNode::sequence("inside", vec![node("a"), node("b")]),
                    ),
                    (node("a"), BeatNode::hold("a", hold(frames))),
                    (node("b"), BeatNode::hold("b", hold(4))),
                    (node("tail"), BeatNode::hold("tail", hold(2))),
                ]),
            },
        },
    );
    let wrapped = edit(
        &inserted,
        Command::WrapRepeat {
            node: node("inside"),
            id: node("inner"),
            plays: inner,
            gap: (gap > 0).then(|| hold(gap)),
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    edit(
        &wrapped,
        Command::WrapRepeat {
            node: node("group"),
            id: node("outer"),
            plays: outer,
            gap: Some(hold(2)),
            anchor_policy: WrapAnchorPolicy::First,
        },
    )
}
fn iteration(document: &ProjectDocument, name: &str, index: u32) -> IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(name)].kind else {
        panic!()
    };
    iterations.at(index).unwrap()
}
fn path(document: &ProjectDocument, name: &str, outer: u32, inner: u32) -> InstancePath {
    InstancePath {
        node: node(name),
        repeats: vec![
            RepeatInstance {
                node: node("outer"),
                iteration: iteration(document, "outer", outer),
            },
            RepeatInstance {
                node: node("inner"),
                iteration: iteration(document, "inner", inner),
            },
        ],
    }
}
fn identities(prefix: &str, nodes: usize, marks: usize) -> OccurrenceIdentities {
    OccurrenceIdentities {
        nodes: (0..nodes)
            .map(|n| node(&format!("{prefix}-n{n}")))
            .collect(),
        marks: (0..marks)
            .map(|n| mark(&format!("{prefix}-m{n}")))
            .collect(),
    }
}
fn occurrence(instance: InstancePath, edit: OccurrenceEdit, ids: OccurrenceIdentities) -> Command {
    Command::EditOccurrence {
        instance,
        edit,
        identities: ids,
    }
}
fn local(name: &str, position: i64) -> Anchor {
    Anchor::Local {
        node: node(name),
        position: ExactRatio::integer(position),
    }
}
fn put(
    document: &ProjectDocument,
    name: &str,
    owner: &str,
    coordinate: Anchor,
    bias: InsertionBias,
) -> ProjectDocument {
    edit(
        document,
        Command::SetMark {
            id: mark(name),
            owner: node(owner),
            label: name.into(),
            boundary: BoundaryAnchor { coordinate, bias },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
}
fn at(document: &ProjectDocument, name: &str) -> ExactRatio {
    let stored = &document.marks()[&mark(name)];
    assert_eq!(stored.state, MarkState::Bound);
    let Anchor::Local { position, .. } = stored.boundary.coordinate else {
        panic!()
    };
    position
}

#[test]
fn nested_edit_isolates_one_play_and_preserves_ancestor_boundaries_and_gaps() {
    let mut document = fixture(3, 2, 2, 1);
    assert_eq!(document.duration().unwrap(), duration(58));
    for (name, pos) in [
        ("selected", 31),
        ("next", 33),
        ("inner-gap", 29),
        ("outer-gap", 39),
        ("later", 45),
    ] {
        document = put(
            &document,
            name,
            "root",
            local("root", pos),
            InsertionBias::Right,
        );
    }
    document = put(
        &document,
        "before",
        "root",
        local("root", 32),
        InsertionBias::Left,
    );
    document = put(
        &document,
        "after",
        "root",
        local("root", 32),
        InsertionBias::Right,
    );
    let selected = path(&document, "a", 1, 1);
    document = put(
        &document,
        "concrete",
        "a",
        Anchor::Occurrence {
            instance: selected.clone(),
            position: ExactRatio::integer(1),
        },
        InsertionBias::Right,
    );
    let other = path(&document, "a", 0, 1);
    document = put(
        &document,
        "other",
        "a",
        Anchor::Occurrence {
            instance: other.clone(),
            position: ExactRatio::integer(1),
        },
        InsertionBias::Right,
    );
    let next = edit(
        &document,
        occurrence(
            selected.clone(),
            OccurrenceEdit::SetHoldDuration {
                duration: duration(5),
            },
            identities("one", 10, 0),
        ),
    );
    assert_eq!(next.duration().unwrap(), duration(61));
    assert_eq!(next.nodes().len(), document.nodes().len() + 10);
    for (name, expected) in [
        ("selected", 31),
        ("next", 36),
        ("inner-gap", 29),
        ("outer-gap", 42),
        ("later", 48),
        ("before", 32),
        ("after", 35),
    ] {
        assert_eq!(at(&next, name), ExactRatio::integer(expected), "{name}");
    }
    assert_eq!(next.node_duration(&node("a")).unwrap(), duration(2));
    assert_eq!(next.node_duration(&node("one-n4")).unwrap(), duration(2));
    assert_eq!(next.node_duration(&node("one-n8")).unwrap(), duration(5));
    assert!(selected.validate(&next).is_err());
    other.validate(&next).unwrap();
    let concrete = &next.marks()[&mark("concrete")];
    assert_eq!(concrete.owner, node("one-n8"));
    let Anchor::Occurrence { instance, position } = &concrete.boundary.coordinate else {
        panic!()
    };
    assert_eq!(instance.node, node("one-n8"));
    assert_eq!(instance.repeats[1].node, node("one-n2"));
    assert_eq!(*position, ExactRatio::integer(1));
    instance.validate(&next).unwrap();
    assert_eq!(
        next.marks()[&mark("other")],
        document.marks()[&mark("other")]
    );
    let reused = edit(
        &next,
        occurrence(
            instance.clone(),
            OccurrenceEdit::Rename {
                label: "selected only".into(),
            },
            OccurrenceIdentities::default(),
        ),
    );
    assert_eq!(reused.nodes().len(), next.nodes().len());
    assert_eq!(reused.overrides(), next.overrides());
    assert_eq!(reused.nodes()[&node("one-n8")].label, "selected only");
}

#[test]
fn owned_marks_copy_but_external_hosts_and_pinned_or_unresolved_coordinates_keep_scope() {
    let mut document = fixture(2, 2, 2, 1);
    document = put(&document, "lost", "a", local("a", 1), InsertionBias::Right);
    document = edit(
        &document,
        Command::SetHoldDuration {
            node: node("a"),
            duration: duration(1),
        },
    );
    document = edit(
        &document,
        Command::SetHoldDuration {
            node: node("a"),
            duration: duration(2),
        },
    );
    document = put(&document, "own", "a", local("a", 1), InsertionBias::Right);
    document = put(
        &document,
        "external-host",
        "a",
        local("root", 33),
        InsertionBias::Right,
    );
    document = put(
        &document,
        "external-owner",
        "root",
        local("a", 1),
        InsertionBias::Right,
    );
    document = put(
        &document,
        "pinned",
        "a",
        Anchor::Sequence {
            frame: ProjectFrame(33),
        },
        InsertionBias::Right,
    );
    let original_lost = document.marks()[&mark("lost")].clone();
    assert!(matches!(original_lost.state, MarkState::Unresolved { .. }));
    let next = edit(
        &document,
        occurrence(
            path(&document, "a", 1, 1),
            OccurrenceEdit::SetHoldDuration {
                duration: duration(3),
            },
            identities("copy", 10, 6),
        ),
    );
    assert_eq!(next.marks().len(), document.marks().len() + 6);
    assert_eq!(
        next.marks()[&mark("pinned")],
        document.marks()[&mark("pinned")]
    );
    assert_eq!(
        next.marks()[&mark("external-owner")],
        document.marks()[&mark("external-owner")]
    );
    assert_eq!(next.marks()[&mark("lost")], original_lost);
    for name in ["own", "external-host", "lost"] {
        let copies: Vec<_> = next
            .marks()
            .values()
            .filter(|mark| mark.label == name)
            .collect();
        assert_eq!(copies.len(), 3, "{name}");
        assert!(copies.iter().any(|mark| mark.owner == node("a")));
        assert!(copies.iter().any(|mark| mark.owner == node("copy-n4")));
        assert!(copies.iter().any(|mark| mark.owner == node("copy-n8")));
        for copy in copies {
            match name {
                "own" => assert!(
                    matches!(&copy.boundary.coordinate,Anchor::Local {node,position} if *node==copy.owner && *position==ExactRatio::integer(1))
                ),
                "external-host" => assert!(
                    matches!(&copy.boundary.coordinate,Anchor::Local {node:host,..} if *host==node("root"))
                ),
                "lost" => {
                    assert_eq!(copy.state, original_lost.state);
                    assert_eq!(copy.boundary, original_lost.boundary);
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn failures_after_partial_isolation_leave_original_and_history_patch_unmodified() {
    let document = put(
        &fixture(3, 2, 2, 1),
        "own",
        "a",
        local("a", 1),
        InsertionBias::Right,
    );
    let original = document.to_json().unwrap();
    let selected = path(&document, "a", 1, 1);
    for ids in [
        identities("short", 9, 2),
        identities("short-mark", 10, 1),
        OccurrenceIdentities {
            nodes: vec![node("duplicate"); 10],
            marks: vec![],
        },
        OccurrenceIdentities {
            nodes: vec![node("a")],
            marks: vec![],
        },
    ] {
        let command = occurrence(
            selected.clone(),
            OccurrenceEdit::Rename {
                label: "bad".into(),
            },
            ids,
        );
        assert!(apply(&document, &request(&document, command)).is_err());
        assert_eq!(document.to_json().unwrap(), original);
    }
    let wrong_kind = occurrence(
        selected.clone(),
        OccurrenceEdit::SetRepeat {
            plays: 3,
            gap: None,
        },
        identities("wrong", 10, 2),
    );
    assert_eq!(
        apply(&document, &request(&document, wrong_kind))
            .unwrap_err()
            .code,
        EditErrorCode::WrongNodeKind
    );
    let mut incomplete = selected.clone();
    incomplete.repeats.pop();
    assert!(
        apply(
            &document,
            &request(
                &document,
                occurrence(
                    incomplete,
                    OccurrenceEdit::Rename {
                        label: "bad".into()
                    },
                    identities("bad", 10, 2)
                )
            )
        )
        .is_err()
    );
    let command = occurrence(
        selected,
        OccurrenceEdit::Rename {
            label: "bad".into(),
        },
        identities("stale", 10, 2),
    );
    let mut stale = request(&document, command);
    stale.expected_revision = RevisionId::new("older").unwrap();
    assert_eq!(
        apply(&document, &stale).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    assert_eq!(document.to_json().unwrap(), original);
}

#[test]
fn copied_repeat_keeps_compact_reordered_allocations_and_normalizes_new_insertions() {
    let document = fixture(2, 3, 2, 1);
    let document = edit(
        &document,
        Command::InsertPlays {
            node: node("inner"),
            index: 1,
            count: 2,
        },
    );
    let document = edit(
        &document,
        Command::MovePlays {
            node: node("inner"),
            start: 3,
            end: 5,
            destination: 0,
        },
    );
    let original = document.nodes()[&node("inner")].clone();
    let selected = InstancePath {
        node: node("inner"),
        repeats: vec![RepeatInstance {
            node: node("outer"),
            iteration: iteration(&document, "outer", 1),
        }],
    };
    let next = edit(
        &document,
        occurrence(
            selected,
            OccurrenceEdit::InsertPlays { index: 2, count: 1 },
            identities("repeat", 7, 0),
        ),
    );
    let NodeKind::Repeat {
        iterations: old, ..
    } = original.kind
    else {
        panic!()
    };
    let NodeKind::Repeat {
        iterations: new, ..
    } = &next.nodes()[&node("repeat-n2")].kind
    else {
        panic!()
    };
    assert_eq!(new.at(2).unwrap().allocation, *next.revision_id());
    for i in 0..old.len() {
        assert_eq!(old.at(i), new.at(if i < 2 { i } else { i + 1 }));
    }
    assert_eq!(
        document.nodes()[&node("inner")],
        next.nodes()[&node("inner")]
    );
}

#[test]
fn source_owned_marks_copy_without_changing_source_time_and_concrete_ancestor_marks_follow_once() {
    let document = fixture(2, 2, 2, 1);
    let base = SourceTimeBase::new(1, 30).unwrap();
    let asset = AssetId::new("source").unwrap();
    let mut document = edit(
        &document,
        Command::AddAsset {
            id: asset.clone(),
            asset: AssetRecord {
                label: "original".into(),
                content_hash: "a".repeat(64),
                video: Some(
                    SourceSpan::new(
                        SourceTimestamp {
                            ticks: 0,
                            time_base: base,
                        },
                        SourceTimestamp {
                            ticks: 100,
                            time_base: base,
                        },
                    )
                    .unwrap(),
                ),
                audio: None,
                still_image: false,
                frame_count: None,
            },
        },
    );
    let source = Anchor::Source {
        asset,
        moment: SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks: 17,
                time_base: base,
            },
        },
    };
    document = put(
        &document,
        "source-mark",
        "a",
        source.clone(),
        InsertionBias::Right,
    );
    document = put(
        &document,
        "ancestor",
        "a",
        Anchor::Occurrence {
            instance: InstancePath {
                node: node("root"),
                repeats: vec![],
            },
            position: ExactRatio::integer(33),
        },
        InsertionBias::Right,
    );
    let next = edit(
        &document,
        occurrence(
            path(&document, "a", 1, 1),
            OccurrenceEdit::SetHoldDuration {
                duration: duration(5),
            },
            identities("source-copy", 10, 2),
        ),
    );
    let copies: Vec<_> = next
        .marks()
        .values()
        .filter(|mark| mark.label == "source-mark")
        .collect();
    assert_eq!(copies.len(), 3);
    for copy in copies {
        assert_eq!(copy.boundary.coordinate, source);
        assert_eq!(copy.state, MarkState::Bound);
    }
    let concrete = &next.marks()[&mark("ancestor")];
    assert_eq!(concrete.owner, node("source-copy-n8"));
    assert_eq!(concrete.state, MarkState::Bound);
    assert!(
        matches!(&concrete.boundary.coordinate,Anchor::Occurrence {instance,position} if instance.node==node("root") && instance.repeats.is_empty() && *position==ExactRatio::integer(36))
    );
    assert_eq!(
        next.marks()
            .values()
            .filter(|mark| mark.label == "ancestor")
            .count(),
        1
    );
}

proptest! {
    #[test]
    fn variable_counts_change_exactly_one_nested_duration(
        outer in 1u32..8, inner in 1u32..8, frames in 1i64..10, replacement in 1i64..15,
        gap in 0i64..4, selected_outer in any::<u32>(), selected_inner in any::<u32>(),
    ) {
        let document=fixture(outer,inner,frames,gap);
        let oi=selected_outer%outer; let ii=selected_inner%inner;
        let next=edit(&document,occurrence(path(&document,"a",oi,ii),OccurrenceEdit::SetHoldDuration {duration:duration(replacement)},identities("prop",10,0)));
        prop_assert_eq!(next.duration().unwrap().frames(),document.duration().unwrap().frames()+replacement-frames);
        prop_assert_eq!(next.nodes().len(),document.nodes().len()+10);
        prop_assert_eq!(next.overrides().len(),2);
        prop_assert_eq!(next.node_duration(&node("a")).unwrap(),duration(frames));
        prop_assert_eq!(next.node_duration(&node("prop-n4")).unwrap(),duration(frames));
        prop_assert_eq!(next.node_duration(&node("prop-n8")).unwrap(),duration(replacement));
        for o in 0..outer {
            for i in 0..inner {
                let mut instance=path(&document,"a",o,i);
                if o==oi {
                    instance.repeats[1].node=node("prop-n2");
                    instance.node=node(if i==ii {"prop-n8"} else {"prop-n4"});
                }
                prop_assert!(instance.validate(&next).is_ok());
            }
        }
    }
}
