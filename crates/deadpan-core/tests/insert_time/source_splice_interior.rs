use super::*;

fn fixture(lead: BeatNode) -> ProjectDocument {
    tree(
        &["prefix", "group", "tail"],
        vec![
            ("prefix", source(1)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("lead"), id("next")]),
            ),
            ("lead", lead),
            ("next", source(2)),
            ("tail", source(1)),
        ],
    )
}

fn sequence<'a>(document: &'a ProjectDocument, key: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(key)].kind else {
        panic!("Sequence")
    };
    children
}

fn splice_at(document: &ProjectDocument, target: NodeId, at: i64, name: &str) -> CommandRequest {
    let NodeKind::Source { source } = source(1).kind else {
        unreachable!()
    };
    request(
        document,
        name,
        Command::SpliceSourceAt {
            parent: id("group"),
            target,
            at: duration(at),
            source,
            id: id(name),
            label: "Inserted Original slice".into(),
            identities: pool(name, 3),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn interior_splice_preserves_live_framing_and_transports_boundary_and_local_marks_once() {
    let pose = |scale| {
        FramingPose::new(
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::integer(scale),
        )
        .unwrap()
    };
    let framing = Framing::creep(pose(1), pose(2), FramingCurve::Linear).unwrap();
    let mut lead = source(4);
    lead.framing = Some(framing.clone());
    let mut wire = serde_json::to_value(fixture(lead)).unwrap();
    wire["nodes"]["group"]["framing"] = json!(framing);
    let mut local = mark(0, InsertionBias::Right, false);
    local.boundary.coordinate = Anchor::Local {
        node: id("lead"),
        position: ExactRatio::new(5, 2).unwrap(),
    };
    wire["marks"] = json!(BTreeMap::from([
        ("left", mark(2, InsertionBias::Left, false)),
        ("right", mark(2, InsertionBias::Right, false)),
        ("pin", mark(5, InsertionBias::Right, true)),
        ("local", local),
    ]));
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, splice_at(&before, id("lead"), 1, "framed"));
    assert_eq!(
        after.nodes()[&id("group")].framing,
        before.nodes()[&id("group")].framing
    );
    assert_eq!(after.nodes()[&id("lead")], before.nodes()[&id("lead")]);
    let right = owner(&after, &sequence(&after, "group")[2]);
    assert_eq!(after.nodes()[&right], before.nodes()[&id("lead")]);
    assert_eq!(mark_position(&after, "left"), ExactRatio::integer(2));
    assert_eq!(mark_position(&after, "right"), ExactRatio::integer(3));
    assert_eq!(mark_position(&after, "pin"), ExactRatio::integer(5));
    assert_eq!(
        mark_position(&after, "local"),
        ExactRatio::new(9, 2).unwrap()
    );
}

#[test]
fn interior_splice_rejects_bad_targets_identity_pools_and_stale_requests_atomically() {
    let before = fixture(source(4));
    let saved = before.to_json().unwrap();
    for mode in [
        "parent",
        "indirect",
        "group",
        "start",
        "end",
        "missing",
        "pool",
        "duplicate",
        "new_collision",
        "old_collision",
        "timing",
        "ordinal",
        "zero",
        "stale",
        "pool_limit",
        "overflow",
        "asset",
    ] {
        let mut request = splice_at(&before, id("lead"), 1, "bad");
        let Command::SpliceSourceAt {
            parent,
            target,
            at,
            source,
            id: inserted,
            identities,
            timing,
            ..
        } = &mut request.command
        else {
            panic!()
        };
        match mode {
            "parent" => *parent = id("absent"),
            "indirect" => *parent = id("root"),
            "group" => {
                *parent = id("root");
                *target = id("group");
            }
            "start" => *at = duration(0),
            "end" => *at = duration(4),
            "missing" => *target = id("absent"),
            "pool" => identities.nodes.truncate(2),
            "duplicate" => identities.nodes[1] = identities.nodes[0].clone(),
            "new_collision" => identities.nodes[0] = inserted.clone(),
            "old_collision" => *inserted = id("lead"),
            "timing" => timing.allocation = revision("other"),
            "ordinal" => timing.ordinal = u32::MAX,
            "zero" => source.duration = FrameDuration::ZERO,
            "stale" => request.expected_revision = revision("stale"),
            "pool_limit" => identities.nodes = vec![id("unused"); MAX_DOCUMENT_NODES + 1],
            "overflow" => source.duration = duration(i64::MAX),
            "asset" => {
                source.video = SourceVideo::Stream {
                    asset: AssetId::new("absent").unwrap(),
                    span: source_span(),
                }
            }
            _ => unreachable!(),
        }
        assert!(apply(&before, &request).is_err(), "{mode}");
        assert_eq!(before.to_json().unwrap(), saved, "{mode}");
    }
    let mut wire = json!(splice_at(&before, id("lead"), 1, "extra"));
    wire["command"]["unexpected"] = json!(true);
    assert!(serde_json::from_value::<CommandRequest>(wire).is_err());
}

#[test]
fn interior_splice_never_descends_a_repeat_or_authored_retime() {
    for kind in [
        NodeKind::Repeat {
            child: id("group"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
            escalation: None,
        },
        NodeKind::Retime {
            child: id("group"),
            duration: duration(4),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        },
    ] {
        let before = tree(
            &["container"],
            vec![
                (
                    "container",
                    BeatNode {
                        label: "Container".into(),
                        framing: None,
                        audio_treatments: Default::default(),
                        audio_editorial_edges: Default::default(),
                        audio_edges: Default::default(),
                        kind,
                        cutaways: Vec::new(),
                        captions: Vec::new(),
                    },
                ),
                ("group", BeatNode::sequence("Group", vec![id("lead")])),
                ("lead", source(4)),
            ],
        );
        assert!(
            before
                .source_splice_interior(&id("group"), &id("lead"), duration(1))
                .is_err()
        );
        assert!(apply(&before, &splice_at(&before, id("lead"), 1, "refused")).is_err());
        let error = before
            .source_splice_interior(&id("root"), &id("container"), duration(1))
            .unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains("Slice placement"));
        assert!(!error.message.contains("pause insertion"));
    }
}

#[test]
fn interior_splice_reserves_the_inserted_source_in_addition_to_split_nodes() {
    let original = tree(&["lead"], vec![("lead", source(4))]);
    let mut wire = json!(original);
    for i in 2..MAX_DOCUMENT_NODES - 3 {
        let name = format!("empty-{i}");
        wire["nodes"][&name] = json!(BeatNode::sequence("", vec![]));
        wire["nodes"]["root"]["kind"]["children"]
            .as_array_mut()
            .unwrap()
            .push(json!(name));
    }
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(before.nodes().len(), MAX_DOCUMENT_NODES - 3);
    // A Source Split alone consumes three IDs; this command also inserts one.
    assert_eq!(
        before
            .source_splice_interior(&id("root"), &id("lead"), duration(1))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    let mut request = splice_at(&before, id("lead"), 1, "over-budget");
    let Command::SpliceSourceAt { parent, .. } = &mut request.command else {
        panic!()
    };
    *parent = id("root");
    assert_eq!(
        apply(&before, &request).unwrap_err().code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(before.revision_id(), &revision("initial"));
    assert!(!before.nodes().contains_key(&id("over-budget")));
}
