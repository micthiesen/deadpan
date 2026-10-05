use super::*;

fn splice(before: &ProjectDocument, parent: &str, index: usize, name: &str) -> CommandRequest {
    let NodeKind::Source { source } = source(1).kind else {
        unreachable!()
    };
    request(
        before,
        name,
        Command::SpliceSource {
            parent: id(parent),
            index,
            source,
            id: id(name),
            label: "Selected Original moment".into(),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

fn fixture() -> ProjectDocument {
    tree(
        &["prefix", "group", "tail"],
        vec![
            ("prefix", source(1)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("empty"), id("b")]),
            ),
            ("a", source(2)),
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("b", source(2)),
            ("tail", source(3)),
        ],
    )
}

fn sequence<'a>(document: &'a ProjectDocument, key: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(key)].kind else {
        panic!()
    };
    children
}

#[test]
fn explicit_slot_retains_nested_edges_and_zero_duration_child_order() {
    let before = fixture();
    for (parent, slot, boundary) in [
        ("root", 0, 0),
        ("root", 3, 8),
        ("group", 0, 1),
        ("group", 1, 3),
        ("group", 2, 3),
        ("group", 3, 5),
        ("empty", 0, 3),
    ] {
        assert_eq!(
            before.source_splice_boundary(&id(parent), slot).unwrap(),
            ProjectFrame(boundary)
        );
        let after = edit(&before, splice(&before, parent, slot, "moment"));
        let mut expected = sequence(&before, parent).to_vec();
        expected.insert(slot, id("moment"));
        assert_eq!(sequence(&after, parent), expected);
        if parent != "root" {
            assert_eq!(children(&after), children(&before));
        }
        assert_eq!(after.nodes().len(), before.nodes().len() + 1);
        assert_eq!(after.duration().unwrap(), duration(9));
        assert!(matches!(
            after.nodes()[&id("moment")].kind,
            NodeKind::Source { .. }
        ));
        assert!(
            !after
                .audio_bindings()
                .bindings()
                .contains_key(&id("moment"))
        );
        // The single retained clock describes only pre-splice owners.
        assert!(
            after
                .audio_bindings()
                .timings()
                .values()
                .all(|layout| !layout.nodes().contains_key(&id("moment")))
        );
    }
}

#[test]
fn nested_source_splice_transports_marks_and_each_later_owner_once() {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["marks"] = serde_json::to_value(BTreeMap::from([
        ("left", mark(3, InsertionBias::Left, false)),
        ("right", mark(3, InsertionBias::Right, false)),
        ("tail", mark(5, InsertionBias::Right, false)),
        ("pin", mark(5, InsertionBias::Right, true)),
    ]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, splice(&before, "group", 1, "moment"));
    for (key, expected) in [("left", 3), ("right", 4), ("tail", 6), ("pin", 5)] {
        assert_eq!(mark_position(&after, key), ExactRatio::integer(expected));
    }
    for key in ["b", "tail"] {
        assert_eq!(
            after.audio_bindings().bindings()[&id(key)].reanchors.len(),
            1
        );
    }
    for key in ["prefix", "a"] {
        assert!(
            after.audio_bindings().bindings()[&id(key)]
                .reanchors
                .is_empty()
        );
    }
}

#[test]
fn empty_project_splice_locks_time_and_has_no_placeholder_or_retained_clock() {
    let automatic = ProjectDocument::new_automatic(
        ProjectId::new("automatic-splice").unwrap(),
        revision("initial"),
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(automatic).unwrap();
    wire["assets"] = serde_json::to_value(tree(&[], vec![]).assets()).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, splice(&before, "root", 0, "first"));
    assert_eq!(children(&after), &[id("first")]);
    assert_eq!(after.nodes().len(), 2);
    assert!(after.audio_bindings().is_empty());
    assert_eq!(after.basis_state().rate_origin, FrameRateOrigin::TimedEdit);
    assert_eq!(
        after.nodes()[&id("first")].label,
        "Selected Original moment"
    );
}

#[test]
fn malformed_splice_is_atomic_for_typed_and_closed_json_callers() {
    let before = fixture();
    let bytes = before.to_json().unwrap();
    for mode in [
        "missing",
        "leaf",
        "slot",
        "collision",
        "timing",
        "zero",
        "asset",
        "audio",
    ] {
        let mut request = splice(&before, "group", 1, "moment");
        let Command::SpliceSource {
            parent,
            index,
            id: new_id,
            timing,
            source,
            ..
        } = &mut request.command
        else {
            panic!()
        };
        match mode {
            "missing" => *parent = id("missing"),
            "leaf" => *parent = id("a"),
            "slot" => *index = usize::MAX,
            "collision" => *new_id = id("a"),
            "timing" => timing.allocation = revision("wrong"),
            "zero" => source.duration = FrameDuration::ZERO,
            "asset" => {
                source.video = SourceVideo::Stream {
                    asset: AssetId::new("missing").unwrap(),
                    span: source_span(),
                }
            }
            "audio" => {
                source.audio = Some(SourceAudio {
                    asset: AssetId::new("media").unwrap(),
                    span: source_span(),
                })
            }
            _ => unreachable!(),
        }
        assert!(apply(&before, &request).is_err(), "{mode}");
        assert_eq!(before.to_json().unwrap(), bytes);
    }
    let mut json = serde_json::to_value(splice(&before, "root", 0, "moment")).unwrap();
    json["command"]["extra"] = json!(true);
    assert!(serde_json::from_value::<CommandRequest>(json).is_err());
}

#[test]
fn repeat_or_retime_ancestry_never_silently_changes_scope() {
    for container in [
        BeatNode {
            audio_treatments: Default::default(),
            label: "Repeat".into(),
            framing: None,
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Repeat {
                child: id("group"),
                iterations: IterationOrder::new(revision("plays"), 1_000_000_000).unwrap(),
                gap: None,
                escalation: None,
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        },
        BeatNode {
            audio_treatments: Default::default(),
            label: "Unity Retime".into(),
            framing: None,
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: id("group"),
                duration: duration(2),
                mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
                pitch: PitchPolicy::FollowSpeed,
                purpose: RetimePurpose::Edit,
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        },
    ] {
        let before = tree(
            &["container"],
            vec![
                ("container", container),
                ("group", BeatNode::sequence("Group", vec![id("a")])),
                ("a", source(2)),
            ],
        );
        for slot in [0, 1] {
            assert!(before.source_splice_boundary(&id("group"), slot).is_err());
            assert!(apply(&before, &splice(&before, "group", slot, "moment")).is_err());
        }
        // Inserting beside the composite remains valid and compact.
        let after = edit(&before, splice(&before, "root", 0, "moment"));
        assert_eq!(
            after.nodes()[&id("container")],
            before.nodes()[&id("container")]
        );
        assert_eq!(after.nodes().len(), before.nodes().len() + 1);
        assert!(
            after
                .audio_bindings()
                .timings()
                .values()
                .all(|layout| layout.nodes().len() == before.nodes().len())
        );
    }
}

#[test]
fn source_splice_rejects_duration_overflow_before_capturing_clocks() {
    let before = tree(&["a"], vec![("a", source(i64::MAX))]);
    let bytes = before.to_json().unwrap();
    assert_eq!(
        apply(&before, &splice(&before, "root", 1, "overflow"))
            .unwrap_err()
            .code,
        EditErrorCode::TimingOverflow
    );
    assert_eq!(before.to_json().unwrap(), bytes);
}
