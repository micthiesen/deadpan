use std::collections::BTreeMap;

use deadpan_core::*;
use proptest::prelude::*;
use serde_json::{Value, json};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn duration(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn empty() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("project-1").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap()
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Pause",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn request(document: &ProjectDocument, command: Command, revision: &str) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}
fn edited(
    document: &ProjectDocument,
    command: Command,
    revision: &str,
) -> (ProjectDocument, EditTransaction) {
    let transaction = apply(document, &request(document, command, revision)).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    let encoded = serde_json::to_string(&transaction).unwrap();
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&encoded).unwrap(),
        transaction
    );
    (after, transaction)
}
fn insert_holds(durations: &[i64]) -> ProjectDocument {
    let nodes: BTreeMap<_, _> = durations
        .iter()
        .enumerate()
        .map(|(i, frames)| (id(&format!("hold-{i}")), hold(*frames)))
        .collect();
    let group = id("group");
    let mut subtree = Subtree {
        overrides: Default::default(),
        root: group.clone(),
        nodes,
    };
    let children = subtree.nodes.keys().cloned().collect();
    subtree
        .nodes
        .insert(group, BeatNode::sequence("Group", children));
    edited(
        &empty(),
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree,
        },
        "insert",
    )
    .0
}
fn changed_json(
    document: &ProjectDocument,
    change: impl FnOnce(&mut Value),
) -> Result<ProjectDocument, DocumentError> {
    let mut value = serde_json::to_value(document).unwrap();
    change(&mut value);
    ProjectDocument::from_json(&value.to_string())
}

#[test]
fn repeat_identity_survives_move_insert_group_and_exact_inverse() {
    let initial = insert_holds(&[7]);
    let (wrapped, _) = edited(
        &initial,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: id("hold-0"),
            id: id("repeat"),
            plays: 4,
            gap: None,
        },
        "allocate",
    );
    let NodeKind::Repeat { iterations, .. } = &wrapped.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let original = iterations.clone();
    let path = InstancePath {
        node: id("hold-0"),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: original.at(1).unwrap(),
        }],
    };
    path.validate(&wrapped).unwrap();
    let (moved, edit) = edited(
        &wrapped,
        Command::MovePlays {
            node: id("repeat"),
            start: 1,
            end: 2,
            destination: 3,
        },
        "move-play",
    );
    assert_eq!(edit.duration_delta, 0);
    path.validate(&moved).unwrap();
    let (inserted, edit) = edited(
        &moved,
        Command::InsertPlays {
            node: id("repeat"),
            index: 2,
            count: 3,
        },
        "insert-plays",
    );
    assert_eq!(edit.duration_delta, 21);
    path.validate(&inserted).unwrap();
    let NodeKind::Repeat { iterations, .. } = &inserted.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    assert_eq!(iterations.position(&path.repeats[0].iteration), Some(6));
    let (grouped, _) = edited(
        &inserted,
        Command::Group {
            parent: id("group"),
            start: 0,
            end: 1,
            id: id("new-group"),
            label: "Grouped".into(),
        },
        "group-repeat",
    );
    path.validate(&grouped).unwrap();
    let (shrunk, _) = edited(
        &grouped,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 6,
            gap: None,
        },
        "retire-play",
    );
    assert!(path.validate(&shrunk).is_err());
    let (grown, _) = edited(
        &shrunk,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 7,
            gap: None,
        },
        "grow-again",
    );
    assert!(
        path.validate(&grown).is_err(),
        "growth cannot retarget an old instance"
    );
    assert_eq!(
        ProjectDocument::from_json(&grown.to_json().unwrap()).unwrap(),
        grown
    );
}

#[test]
fn instance_paths_require_exact_ordered_repeat_ancestry() {
    let (inner, _) = edited(
        &insert_holds(&[3]),
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: id("hold-0"),
            id: id("inner"),
            plays: 2,
            gap: None,
        },
        "inner-allocation",
    );
    let (outer, _) = edited(
        &inner,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: id("inner"),
            id: id("outer"),
            plays: 3,
            gap: None,
        },
        "outer-allocation",
    );
    let step = |name: &str, position| {
        let NodeKind::Repeat { iterations, .. } = &outer.nodes()[&id(name)].kind else {
            unreachable!()
        };
        RepeatInstance {
            node: id(name),
            iteration: iterations.at(position).unwrap(),
        }
    };
    let path = InstancePath {
        node: id("hold-0"),
        repeats: vec![step("outer", 2), step("inner", 1)],
    };
    path.validate(&outer).unwrap();
    for repeats in [
        vec![],
        vec![step("inner", 1)],
        vec![step("inner", 1), step("outer", 2)],
        vec![step("outer", 2), step("inner", 1), step("inner", 0)],
    ] {
        assert!(
            InstancePath {
                repeats,
                ..path.clone()
            }
            .validate(&outer)
            .is_err()
        );
    }
    // A Repeat node itself names only its outer occurrence, not one of its plays.
    InstancePath {
        node: id("inner"),
        repeats: vec![step("outer", 2)],
    }
    .validate(&outer)
    .unwrap();
}

#[test]
fn inserted_repeat_cannot_reserve_a_future_revision_and_revive_retired_plays() {
    let imported = IterationOrder::new(RevisionId::new("foreign").unwrap(), 3)
        .unwrap()
        .inserted(3, 1, RevisionId::new("future-r").unwrap())
        .unwrap();
    let (inserted, _) = edited(
        &empty(),
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                root: id("repeat"),
                nodes: BTreeMap::from([
                    (id("child"), hold(1)),
                    (
                        id("repeat"),
                        BeatNode {
                            label: "Imported".into(),
                            kind: NodeKind::Repeat {
                                child: id("child"),
                                iterations: imported,
                                gap: None,
                            },
                        },
                    ),
                ]),
            },
        },
        "actual-insertion",
    );
    let NodeKind::Repeat { iterations, .. } = &inserted.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let retired = iterations.at(3).unwrap();
    assert_eq!(retired.allocation.as_str(), "actual-insertion");
    let (shrunk, _) = edited(
        &inserted,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 3,
            gap: None,
        },
        "shrink-import",
    );
    let (grown, _) = edited(
        &shrunk,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 4,
            gap: None,
        },
        "future-r",
    );
    let NodeKind::Repeat { iterations, .. } = &grown.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    assert_ne!(iterations.at(3), Some(retired.clone()));
    assert!(iterations.position(&retired).is_none());
}

#[test]
fn deserialization_preserves_timing_invariants() {
    for json in ["-1", "-9223372036854775808", "1.5"] {
        assert!(serde_json::from_str::<FrameDuration>(json).is_err());
    }
    for json in [
        r#"{"numerator":0,"denominator":1}"#,
        r#"{"numerator":30,"denominator":0}"#,
        r#"{"numerator":30,"denominator":1,"typo":true}"#,
    ] {
        assert!(serde_json::from_str::<FrameRate>(json).is_err());
        assert!(serde_json::from_str::<SourceTimeBase>(json).is_err());
    }
    for json in [
        r#"{"start":2,"end":1}"#,
        r#"{"start":-9223372036854775808,"end":0}"#,
    ] {
        assert!(serde_json::from_str::<FrameRange>(json).is_err());
    }
    assert!(NodeId::new("").is_err());
    assert!(NodeId::new("../../bad").is_err());
    assert!(NodeId::new("a".repeat(129)).is_err());
    assert!(serde_json::from_str::<NodeId>(r#""bad id""#).is_err());
    let rate: FrameRate =
        serde_json::from_str(r#"{"numerator":60000,"denominator":2002}"#).unwrap();
    assert_eq!(rate, FrameRate::new(30_000, 1001).unwrap());
}

#[test]
fn dump_is_deterministic_and_unknown_schemas_are_rejected() {
    let document = insert_holds(&[10, 20, 30]);
    let json = document.to_json().unwrap();
    let decoded = ProjectDocument::from_json(&json).unwrap();
    assert_eq!(decoded, document);
    assert_eq!(decoded.to_json().unwrap(), json);
    assert_eq!(
        changed_json(&document, |v| v["schema_version"] =
            json!(DOCUMENT_SCHEMA_VERSION + 1))
        .unwrap_err()
        .code,
        DocumentErrorCode::UnsupportedSchema
    );
    assert!(changed_json(&document, |v| v["unrecognized"] = json!(true)).is_err());
    assert!(changed_json(&document, |v| v["presentation_basis"]["width"] = json!(0)).is_err());
    assert!(
        changed_json(
            &document,
            |v| v["nodes"]["hold-0"]["kind"]["recipe"]["duration"] = json!(0)
        )
        .is_err()
    );
    assert!(
        changed_json(
            &document,
            |v| v["nodes"]["hold-0"]["kind"]["recipe"]["duration"] = json!(-1)
        )
        .is_err()
    );
    let duplicate = empty().to_json().unwrap().replace("\"nodes\": {", "\"nodes\": {\"root\": {\"label\":\"Other\",\"kind\":{\"type\":\"sequence\",\"children\":[]}},");
    assert!(
        ProjectDocument::from_json(&duplicate)
            .unwrap_err()
            .message
            .contains("duplicate identity key")
    );
}

#[test]
fn tree_rejects_missing_orphans_cycles_and_multiple_parents() {
    let document = insert_holds(&[10, 20]);
    for children in [
        json!(["missing"]),
        json!(["hold-0"]),
        json!(["hold-0", "hold-0", "hold-1"]),
        json!(["root", "hold-0", "hold-1"]),
    ] {
        assert!(
            changed_json(&document, |v| v["nodes"]["group"]["kind"]["children"] =
                children)
            .is_err()
        );
    }
    assert!(changed_json(&document, |v| v["root"] = json!("hold-0")).is_err());
    assert!(
        changed_json(&document, |v| v["nodes"]["orphan"] =
            json!({"label":"orphan","kind":{"type":"sequence","children":[]}}))
        .is_err()
    );
}

#[test]
fn structural_depth_is_bounded_without_recursive_documents() {
    for (depth, valid) in [(MAX_DOCUMENT_DEPTH, true), (MAX_DOCUMENT_DEPTH + 1, false)] {
        let mut nodes = BTreeMap::new();
        for index in 1..depth {
            nodes.insert(
                id(&format!("level-{index}")),
                BeatNode::sequence("Layer", vec![id(&format!("level-{}", index + 1))]),
            );
        }
        nodes.insert(id(&format!("level-{depth}")), hold(1));
        let command = Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                root: id("level-1"),
                nodes,
            },
        };
        let result = apply(&empty(), &request(&empty(), command, "deep"));
        assert_eq!(result.is_ok(), valid);
    }
}

#[test]
fn repeat_setter_is_not_a_wrapper_and_gaps_are_between_plays() {
    let document = insert_holds(&[5]);
    let gap = HoldRecipe {
        duration: duration(2),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    };
    let (wrapped, first) = edited(
        &document,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: id("hold-0"),
            id: id("repeat"),
            plays: 3,
            gap: Some(gap.clone()),
        },
        "repeat",
    );
    assert_eq!(wrapped.duration().unwrap().frames(), 19);
    assert_eq!(first.duration_delta, 14);
    let (updated, _) = edited(
        &wrapped,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 4,
            gap: Some(gap),
        },
        "set-repeat",
    );
    assert_eq!(updated.duration().unwrap().frames(), 26);
    assert_eq!(updated.nodes().len(), wrapped.nodes().len());
    assert!(
        matches!(&updated.nodes()[&id("repeat")].kind, NodeKind::Repeat { child, iterations, .. } if child == &id("hold-0") && iterations.len() == 4)
    );
    let (nested, _) = edited(
        &updated,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: id("repeat"),
            id: id("outer-repeat"),
            plays: 2,
            gap: None,
        },
        "nested",
    );
    assert_eq!(nested.duration().unwrap().frames(), 52);
    assert_eq!(nested.nodes().len(), updated.nodes().len() + 1);
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::SetRepeat {
                    node: id("hold-0"),
                    plays: 3,
                    gap: None
                },
                "bad"
            )
        )
        .is_err()
    );
}

#[test]
fn structural_operations_preserve_identity_and_atomic_inverse() {
    let original = insert_holds(&[5, 10, 15]);
    let (grouped, _) = edited(
        &original,
        Command::Group {
            parent: id("group"),
            start: 0,
            end: 2,
            id: id("inner"),
            label: "Answer".into(),
        },
        "grouped",
    );
    assert_eq!(grouped.duration().unwrap(), original.duration().unwrap());
    let (moved, _) = edited(
        &grouped,
        Command::Move {
            node: id("inner"),
            parent: id("group"),
            index: 1,
        },
        "moved",
    );
    assert_eq!(moved.nodes()[&id("inner")], grouped.nodes()[&id("inner")]);
    assert_eq!(moved.duration().unwrap(), original.duration().unwrap());
    let (ungrouped, _) = edited(&moved, Command::Ungroup { node: id("inner") }, "ungrouped");
    assert_eq!(ungrouped.duration().unwrap(), original.duration().unwrap());
    assert!(
        matches!(&ungrouped.nodes()[&id("group")].kind, NodeKind::Sequence { children } if children == &vec![id("hold-2"),id("hold-0"),id("hold-1")])
    );
    let (deleted, transaction) = edited(&grouped, Command::Delete { node: id("inner") }, "deleted");
    assert_eq!(transaction.duration_delta, -15);
    assert_eq!(deleted.duration().unwrap().frames(), 15);
    assert!(!deleted.nodes().contains_key(&id("hold-0")));
    assert!(!deleted.nodes().contains_key(&id("hold-1")));
}

#[test]
fn failed_edits_do_not_mutate_input_and_conflicts_identify_revision() {
    let document = insert_holds(&[5]);
    let before = document.to_json().unwrap();
    let commands = [
        Command::Delete { node: id("root") },
        Command::Move {
            node: id("group"),
            parent: id("group"),
            index: 0,
        },
        Command::Group {
            parent: id("group"),
            start: 0,
            end: 2,
            id: id("new"),
            label: "Invalid".into(),
        },
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: id("hold-0"),
            id: id("repeat"),
            plays: 0,
            gap: None,
        },
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: id("hold-0"),
            id: id("hold-0"),
            plays: 2,
            gap: None,
        },
        Command::SetHoldDuration {
            node: id("hold-0"),
            duration: FrameDuration::ZERO,
        },
    ];
    for command in commands {
        assert!(apply(&document, &request(&document, command, "invalid")).is_err());
        assert_eq!(document.to_json().unwrap(), before);
    }
    let command = Command::Rename {
        node: id("hold-0"),
        label: "New label".into(),
    };
    let mut stale = request(&document, command, "new");
    stale.expected_revision = RevisionId::new("old").unwrap();
    let error = apply(&document, &stale).unwrap_err();
    assert_eq!(error.code, EditErrorCode::RevisionConflict);
    assert_eq!(
        error.current_revision.as_ref(),
        Some(document.revision_id())
    );
}

#[test]
fn patch_preconditions_and_fresh_undo_revisions_prevent_stale_edits() {
    let before = insert_holds(&[5]);
    let command = Command::SetHoldDuration {
        node: id("hold-0"),
        duration: duration(9),
    };
    let (after, transaction) = edited(&before, command, "changed");
    assert_eq!(transaction.forward.nodes.len(), 1);
    let undo = transaction.inverse.rebased(
        after.revision_id().clone(),
        RevisionId::new("undo").unwrap(),
    );
    let restored = undo.apply(&after).unwrap();
    assert_eq!(restored.nodes(), before.nodes());
    assert_ne!(restored.revision_id(), before.revision_id());
    assert_eq!(
        transaction.forward.apply(&restored).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    let mut forged = transaction.forward.clone();
    forged.nodes.get_mut(&id("hold-0")).unwrap().before = Some(hold(100));
    assert_eq!(
        forged.apply(&before).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
}

fn span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 1000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}
fn with_asset() -> (ProjectDocument, AssetId) {
    let asset = AssetId::new("asset").unwrap();
    let record = AssetRecord {
        label: "Fixture".into(),
        content_hash: "a".repeat(64),
        video: Some(span(-100, 1000)),
        audio: Some(span(-100, 1000)),
        still_image: false,
        frame_count: Some(duration(33)),
    };
    (
        edited(
            &empty(),
            Command::AddAsset {
                id: asset.clone(),
                asset: record,
            },
            "asset",
        )
        .0,
        asset,
    )
}

#[test]
fn source_streams_retain_timestamps_and_validate_bounds_independently() {
    let (document, asset) = with_asset();
    let source = SourceNode {
        duration: duration(12),
        video: SourceVideo::Stream {
            asset: asset.clone(),
            span: span(-100, 300),
        },
        audio: Some(SourceAudio {
            asset: asset.clone(),
            span: span(-50, 350),
        }),
        link: LinkRelation::Linked,
        audio_mapping: SourceAudioMapping::FitBeat,
        audio_offset: AudioSample(2400),
    };
    let command = Command::Insert {
        parent: id("root"),
        index: 0,
        subtree: Subtree {
            overrides: Default::default(),
            root: id("source"),
            nodes: BTreeMap::from([(
                id("source"),
                BeatNode {
                    label: "Source".into(),
                    kind: NodeKind::Source { source },
                },
            )]),
        },
    };
    let (after, _) = edited(&document, command, "source");
    let encoded = after.to_json().unwrap();
    assert_eq!(ProjectDocument::from_json(&encoded).unwrap(), after);
    assert!(
        changed_json(&after, |v| v["nodes"]["source"]["kind"]["source"]["audio"]
            ["span"]["end"]["ticks"] = json!(1001))
        .is_err()
    );
    assert!(
        changed_json(&after, |v| v["nodes"]["source"]["kind"]["source"]["video"]
            ["span"]["start"]["ticks"] = json!(-101))
        .is_err()
    );
    assert!(
        changed_json(&after, |v| v["nodes"]["source"]["kind"]["source"]["audio"]
            ["span"]["start"]["time_base"]["denominator"] =
            json!(2000))
        .is_err()
    );
    assert!(
        changed_json(&after, |v| v["assets"]["asset"]["content_hash"] =
            json!("not-a-hash"))
        .is_err()
    );
    assert!(
        changed_json(&after, |v| {
            v["assets"].as_object_mut().unwrap().remove("asset");
        })
        .is_err()
    );
}

#[test]
fn hold_provider_changes_are_explicit_and_duration_checked() {
    let (document, asset) = with_asset();
    let (document, _) = edited(
        &document,
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                root: id("hold"),
                nodes: BTreeMap::from([(id("hold"), hold(12))]),
            },
        },
        "hold",
    );
    let video = HoldVideo::Accepted {
        asset,
        frames: FrameRange::new(ProjectFrame(2), ProjectFrame(14)).unwrap(),
    };
    let (accepted, tx) = edited(
        &document,
        Command::SetHoldProvider {
            node: id("hold"),
            video,
        },
        "accepted",
    );
    assert_eq!(tx.duration_delta, 0);
    assert_eq!(accepted.duration().unwrap().frames(), 12);
    assert_eq!(
        apply(
            &accepted,
            &request(
                &accepted,
                Command::SetHoldDuration {
                    node: id("hold"),
                    duration: duration(13)
                },
                "mismatch"
            )
        )
        .unwrap_err()
        .code,
        EditErrorCode::SourceRangeInvalid
    );
    let (fallback, _) = edited(
        &accepted,
        Command::SetHoldProvider {
            node: id("hold"),
            video: HoldVideo::Background,
        },
        "fallback",
    );
    let (extended, _) = edited(
        &fallback,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: duration(13),
        },
        "extended",
    );
    assert_eq!(extended.duration().unwrap().frames(), 13);
}

#[test]
fn rejected_edits_preserve_specific_error_codes_and_input() {
    let document = insert_holds(&[5, 1]);
    let before = document.to_json().unwrap();
    for (command, expected) in [
        (
            Command::SetHoldDuration {
                node: id("hold-0"),
                duration: FrameDuration::ZERO,
            },
            EditErrorCode::InvalidDuration,
        ),
        (
            Command::SetHoldDuration {
                node: id("hold-0"),
                duration: duration(i64::MAX),
            },
            EditErrorCode::TimingOverflow,
        ),
        (
            Command::Rename {
                node: id("hold-0"),
                label: "x".repeat(1025),
            },
            EditErrorCode::LimitExceeded,
        ),
    ] {
        assert_eq!(
            apply(&document, &request(&document, command, "rejected"))
                .unwrap_err()
                .code,
            expected
        );
        assert_eq!(document.to_json().unwrap(), before);
    }
}

#[test]
fn retime_range_is_in_child_clock_and_overflow_is_rejected() {
    let mut nodes = BTreeMap::from([(id("hold"), hold(10))]);
    nodes.insert(
        id("retime"),
        BeatNode {
            label: "Slow".into(),
            kind: NodeKind::Retime {
                child: id("hold"),
                duration: duration(15),
                mapping: FrameRange::new(ProjectFrame(2), ProjectFrame(9)).unwrap(),
                pitch: PitchPolicy::Preserve,
            },
        },
    );
    let (document, _) = edited(
        &empty(),
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                root: id("retime"),
                nodes,
            },
        },
        "retime",
    );
    assert_eq!(document.duration().unwrap().frames(), 15);
    for range in [
        json!({"start":-1,"end":5}),
        json!({"start":0,"end":11}),
        json!({"start":2,"end":2}),
    ] {
        assert!(
            changed_json(&document, |v| v["nodes"]["retime"]["kind"]["mapping"] =
                range)
            .is_err()
        );
    }
    let huge = insert_holds(&[i64::MAX]);
    assert!(
        apply(
            &huge,
            &request(
                &huge,
                Command::WrapRepeat {
                    anchor_policy: WrapAnchorPolicy::First,
                    node: id("hold-0"),
                    id: id("repeat"),
                    plays: 2,
                    gap: None
                },
                "overflow"
            )
        )
        .is_err()
    );
}

proptest! {
    #[test]
    fn arbitrary_grouping_and_repeat_edits_round_trip(frames in prop::collection::vec(1i64..100_000,1..20), plays in 1u32..100, gap in 1i64..1000) {
        let document = insert_holds(&frames);
        let expected: i64 = frames.iter().sum();
        prop_assert_eq!(document.duration().unwrap().frames(), expected);
        let (grouped, _) = edited(&document, Command::Group { parent:id("group"),start:0,end:frames.len(),id:id("nested"),label:"Nested".into() },"grouped");
        prop_assert_eq!(grouped.duration().unwrap().frames(), expected);
        let (wrapped, _) = edited(&grouped, Command::WrapRepeat { node:id("nested"),id:id("repeat"),plays,gap:Some(HoldRecipe { duration:duration(gap),video:HoldVideo::Background,audio:HoldAudio::Silence }),anchor_policy:WrapAnchorPolicy::First },"wrapped");
        prop_assert_eq!(wrapped.duration().unwrap().frames(), i64::from(plays)*expected+i64::from(plays-1)*gap);
        let encoded = wrapped.to_json().unwrap();
        prop_assert_eq!(ProjectDocument::from_json(&encoded).unwrap(), wrapped);
    }

    #[test]
    fn malformed_random_json_never_panics(bytes in prop::collection::vec(any::<u8>(),0..2048)) {
        if let Ok(json) = std::str::from_utf8(&bytes) { let _ = ProjectDocument::from_json(json); }
    }
}
