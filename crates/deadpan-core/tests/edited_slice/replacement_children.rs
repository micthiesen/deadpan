use super::*;

fn copied(document: &ProjectDocument, parent: &str, first: &str, last: &str) -> CapturedEditSlice {
    CapturedEditSlice::capture_selection(
        document,
        &id(parent),
        &SliceCaptureSelection::Children {
            first: id(first),
            last: id(last),
        },
        timing("replacement-capture"),
    )
    .unwrap()
}

fn replacement(
    document: &ProjectDocument,
    parent: &str,
    first: &str,
    last: &str,
    slice: &CapturedEditSlice,
    revision: &str,
) -> CommandRequest {
    let required = slice.identity_requirements().unwrap();
    request(
        document,
        revision,
        Command::ReplaceSliceChildren {
            parent: id(parent),
            first: id(first),
            last: id(last),
            slice: slice.clone(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|n| id(&format!("{revision}-node-{n}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|n| MarkId::new(format!("{revision}-mark-{n}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|n| id(&format!("{revision}-alias-{n}")))
                    .collect(),
            },
            timing: timing(revision),
        },
    )
}

#[test]
fn positive_span_to_empty_slice_removes_exact_empty_endpoints_and_owned_marks() {
    let before = marked(
        &tree(
            &["left", "body", "right", "tail"],
            vec![
                ("left", BeatNode::sequence("Left empty", vec![])),
                ("body", hold(4)),
                ("right", BeatNode::sequence("Right empty", vec![])),
                ("tail", hold(3)),
            ],
        ),
        vec![("body-mark", {
            let mut value = mark("body", 2, InsertionBias::Right);
            value.loss_policy = AnchorLossPolicy::DeleteOwned;
            value
        })],
    );
    let empty = copied(&before, "root", "left", "left");
    let target = before
        .slice_children_replacement(&id("root"), &id("left"), &id("right"), &empty)
        .unwrap();
    assert_eq!(target.range, range(0, 4));
    assert_eq!((target.first, target.end), (0, 3));
    let command = replacement(&before, "root", "left", "right", &empty, "empty-result");
    let tx = apply(&before, &command).unwrap();
    assert_eq!(tx.duration_delta, -4);
    let after = edit(&before, &command);
    let NodeKind::Sequence { children } = &after.nodes()[&id("root")].kind else {
        panic!()
    };
    assert_eq!(children, &[id("empty-result-node-0"), id("tail")]);
    for removed in ["left", "body", "right"] {
        assert!(!after.nodes().contains_key(&id(removed)));
    }
    assert!(
        !after
            .marks()
            .contains_key(&MarkId::new("body-mark").unwrap())
    );
    assert_eq!(after.duration().unwrap(), duration(3));
}

#[test]
fn empty_nested_forest_to_positive_slice_moves_ancestor_suffix_once() {
    let before = tree(
        &["group", "tail"],
        vec![
            (
                "group",
                BeatNode::sequence("Group", ["a", "b", "c"].map(id).to_vec()),
            ),
            ("a", BeatNode::sequence("A", vec![])),
            ("b", BeatNode::sequence("B", vec![])),
            ("c", BeatNode::sequence("C", vec![])),
            ("tail", hold(2)),
        ],
    );
    let positive = CapturedEditSlice::capture_selection(
        &before,
        &id("root"),
        &SliceCaptureSelection::Child { node: id("tail") },
        timing("replacement-capture"),
    )
    .unwrap();
    let target = before
        .slice_children_replacement(&id("group"), &id("a"), &id("c"), &positive)
        .unwrap();
    assert_eq!(target.range, range(0, 0));
    let command = replacement(&before, "group", "a", "c", &positive, "positive-result");
    let tx = apply(&before, &command).unwrap();
    assert_eq!(tx.duration_delta, 2);
    let after = edit(&before, &command);
    let NodeKind::Sequence { children } = &after.nodes()[&id("group")].kind else {
        panic!()
    };
    assert_eq!(children, &[id("positive-result-node-0")]);
    assert_eq!(after.duration().unwrap(), duration(4));
    assert_eq!(after.node_duration(&id("tail")).unwrap(), duration(2));
    assert_eq!(
        after.source_splice_boundary(&id("root"), 1).unwrap(),
        ProjectFrame(2)
    );
}

#[test]
fn empty_to_empty_replacement_still_retires_selected_owner_and_mark() {
    let before = marked(
        &tree(
            &["a", "b", "tail"],
            vec![
                ("a", BeatNode::sequence("A", vec![])),
                ("b", BeatNode::sequence("B", vec![])),
                ("tail", hold(2)),
            ],
        ),
        vec![("b-mark", {
            let mut value = mark("b", 0, InsertionBias::Right);
            value.loss_policy = AnchorLossPolicy::DeleteOwned;
            value
        })],
    );
    let empty = copied(&before, "root", "a", "a");
    let command = replacement(&before, "root", "a", "b", &empty, "zero-result");
    let tx = apply(&before, &command).unwrap();
    assert_eq!(tx.duration_delta, 0);
    let after = edit(&before, &command);
    assert!(!after.nodes().contains_key(&id("a")));
    assert!(!after.nodes().contains_key(&id("b")));
    assert!(!after.marks().contains_key(&MarkId::new("b-mark").unwrap()));
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
}

#[test]
fn positive_to_positive_replacement_preserves_exact_duration_and_fresh_identity() {
    let before = tree(
        &["left", "body", "right", "tail"],
        vec![
            ("left", BeatNode::sequence("Left empty", vec![])),
            ("body", hold(4)),
            ("right", BeatNode::sequence("Right empty", vec![])),
            ("tail", hold(2)),
        ],
    );
    let body = CapturedEditSlice::capture_selection(
        &before,
        &id("root"),
        &SliceCaptureSelection::Child { node: id("body") },
        timing("replacement-capture"),
    )
    .unwrap();
    let command = replacement(&before, "root", "left", "right", &body, "same-size");
    let after = edit(&before, &command);
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    for removed in ["left", "body", "right"] {
        assert!(!after.nodes().contains_key(&id(removed)));
    }
    let NodeKind::Sequence { children } = &after.nodes()[&id("root")].kind else {
        panic!()
    };
    assert_eq!(children, &[id("same-size-node-0"), id("tail")]);
    let mut conflict = command.clone();
    let Command::ReplaceSliceChildren { identities, .. } = &mut conflict.command else {
        unreachable!()
    };
    identities.authored.nodes[0] = id("tail");
    assert_eq!(
        apply(&before, &conflict).unwrap_err().code,
        EditErrorCode::IdentityConflict
    );
}

#[test]
fn source_replaces_empty_siblings_with_one_exact_new_owner() {
    let asset = AssetId::new("source-for-replacement").unwrap();
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 2,
            time_base,
        },
    )
    .unwrap();
    let source = SourceNode {
        edit_window: None,
        duration: duration(2),
        video: SourceVideo::Stream {
            asset: asset.clone(),
            span,
        },
        audio: None,
        video_mapping: SourceVideoMapping::FitBeat,
        audio_mapping: SourceAudioMapping::FitBeat,
        link: LinkRelation::Independent,
        audio_offset: AudioSample(0),
    };
    let mut before_wire = serde_json::to_value(tree(
        &["a", "b", "donor"],
        vec![
            ("a", BeatNode::sequence("A", vec![])),
            ("b", BeatNode::sequence("B", vec![])),
            ("donor", hold(2)),
        ],
    ))
    .unwrap();
    let mut donor = hold(2);
    donor.kind = NodeKind::Source {
        source: source.clone(),
    };
    before_wire["nodes"]["donor"] = serde_json::to_value(donor).unwrap();
    before_wire["assets"] = json!({ asset.as_str(): AssetRecord {
        label: "Source".into(), content_hash: "a".repeat(64), video: Some(span),
        audio: None, frame_count: None, still_image: false, source_qualification: None,
    }});
    let before = ProjectDocument::from_json(&before_wire.to_string()).unwrap();
    let target = before
        .source_children_replacement(&id("root"), &id("a"), &id("b"))
        .unwrap();
    assert_eq!(target.range, range(0, 0));
    let command = request(
        &before,
        "source-result",
        Command::ReplaceSourceChildren {
            parent: id("root"),
            first: id("a"),
            last: id("b"),
            source,
            id: id("source-result-new"),
            label: "Copied Original".into(),
            timing: timing("source-result"),
        },
    );
    let after = edit(&before, &command);
    let NodeKind::Sequence { children } = &after.nodes()[&id("root")].kind else {
        panic!()
    };
    assert_eq!(children, &[id("source-result-new"), id("donor")]);
    assert_eq!(after.duration().unwrap(), duration(4));
    assert!(!after.nodes().contains_key(&id("a")));
    assert!(!after.nodes().contains_key(&id("b")));
}

#[test]
fn source_replacement_at_node_capacity_removes_an_owner_before_inserting() {
    let names: Vec<_> = (0..MAX_DOCUMENT_NODES - 1)
        .map(|n| format!("empty-{n}"))
        .collect();
    let last = names.last().unwrap();
    let mut wire = serde_json::to_value(tree(
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
        names
            .iter()
            .map(|name| (name.as_str(), BeatNode::sequence("Empty", vec![])))
            .collect(),
    ))
    .unwrap();
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 1,
            time_base,
        },
    )
    .unwrap();
    let asset = AssetId::new("original").unwrap();
    wire["assets"] = json!({asset.as_str(): AssetRecord {
        label: "Original".into(), content_hash: "a".repeat(64), video: Some(span),
        audio: None, frame_count: None, still_image: false, source_qualification: None,
    }});
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(before.nodes().len(), MAX_DOCUMENT_NODES);
    let source = SourceNode {
        edit_window: None,
        duration: duration(1),
        video: SourceVideo::Stream { asset, span },
        audio: None,
        video_mapping: SourceVideoMapping::FitBeat,
        audio_mapping: SourceAudioMapping::FitBeat,
        link: LinkRelation::Independent,
        audio_offset: AudioSample(0),
    };
    let command = request(
        &before,
        "at-capacity",
        Command::ReplaceSourceChildren {
            parent: id("root"),
            first: id(last),
            last: id(last),
            source,
            id: id("replacement"),
            label: "Replacement".into(),
            timing: timing("at-capacity"),
        },
    );
    let after = edit(&before, &command);
    assert_eq!(after.nodes().len(), MAX_DOCUMENT_NODES);
    assert!(!after.nodes().contains_key(&id(last)));
    assert!(after.nodes().contains_key(&id("replacement")));
    assert_eq!(after.duration().unwrap(), duration(1));
}
