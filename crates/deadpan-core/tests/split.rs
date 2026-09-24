use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use deadpan_core::*;
use serde_json::json;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Beat",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn repeat(child: &str, plays: u32) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Repeat".into(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), plays).unwrap(),
            gap: None,
        },
        audio_edges: Default::default(),
    }
}
fn retime(child: &str, start: i64, end: i64, output: i64, purpose: RetimePurpose) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Retime".into(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(output),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose,
        },
        audio_edges: Default::default(),
    }
}
fn tree(children: &[&str], entries: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut nodes: BTreeMap<_, _> = entries
        .into_iter()
        .map(|(key, node)| (id(key), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Project", children.iter().map(|name| id(name)).collect()),
    );
    ProjectDocument::from_json(&json!({
        "schema_version": DOCUMENT_SCHEMA_VERSION,
        "project_id":"split", "revision_id":"r",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root", "assets":{}, "marks":{}, "overrides":{}, "nodes":nodes
    }).to_string()).unwrap()
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(format!(
            "edit-{}",
            NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
        ))
        .unwrap(),
        command,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    let transaction = apply(document, &request(document, command)).unwrap();
    let wire = serde_json::to_string(&transaction).unwrap();
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&wire).unwrap(),
        transaction
    );
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
fn pool(prefix: &str, count: usize) -> SplitIdentities {
    SplitIdentities {
        nodes: (0..count).map(|i| id(&format!("{prefix}-{i}"))).collect(),
    }
}
fn split(document: &ProjectDocument, target: &str, at: i64, prefix: &str) -> ProjectDocument {
    edit(
        document,
        Command::Split {
            node: id(target),
            at: duration(at),
            identities: pool(prefix, document.nodes().len() + 3),
        },
    )
}
fn children<'a>(document: &'a ProjectDocument, node: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(node)].kind else {
        panic!("Sequence")
    };
    children
}
fn partition<'a>(document: &'a ProjectDocument, node: &NodeId) -> (&'a NodeId, FrameRange) {
    let NodeKind::Retime {
        child,
        duration,
        mapping,
        purpose,
        ..
    } = &document.nodes()[node].kind
    else {
        panic!("partition")
    };
    assert_eq!(*purpose, RetimePurpose::Partition);
    assert_eq!(*duration, mapping.duration());
    assert_eq!(
        document.nodes()[node].audio_edges,
        AudioEdgePolicies::default()
    );
    (child, *mapping)
}
fn local(host: &str, position: i64) -> Anchor {
    Anchor::Local {
        node: id(host),
        position: ExactRatio::integer(position),
    }
}
fn path(host: &str) -> InstancePath {
    InstancePath {
        node: id(host),
        repeats: vec![],
    }
}
fn mark(owner: &str, coordinate: Anchor, bias: InsertionBias) -> Mark {
    Mark {
        owner: id(owner),
        label: "Cue".into(),
        boundary: BoundaryAnchor { coordinate, bias },
        loss_policy: AnchorLossPolicy::DeleteOwned,
        state: MarkState::Bound,
        fragments: vec![],
    }
}
fn with_marks(document: &ProjectDocument, marks: Vec<(&str, Mark)>) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["marks"] = serde_json::to_value(
        marks
            .into_iter()
            .map(|(name, mark)| (MarkId::new(name).unwrap(), mark))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn stored<'a>(document: &'a ProjectDocument, name: &str) -> &'a Mark {
    &document.marks()[&MarkId::new(name).unwrap()]
}
fn query(
    document: &ProjectDocument,
    name: &str,
    occurrence: Option<InstancePath>,
) -> Result<ResolvedBoundary, AnchorError> {
    let resolved = AnchorIndex::new(document)
        .unwrap()
        .resolve(&SelectionRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            role: MediaRole::Linked,
            selector: BoundarySelector::Mark {
                target: NamedMarkTarget {
                    id: MarkId::new(name).unwrap(),
                    occurrence,
                },
            },
        })?;
    let ResolvedSelectionKind::Point { point } = resolved.selection else {
        panic!("point")
    };
    Ok(point)
}

#[test]
fn hold_sequence_repeat_and_authored_retime_retain_full_context_and_exact_inverse() {
    let fixtures = [
        tree(&["t"], vec![("t", hold(10))]),
        tree(
            &["t"],
            vec![
                ("t", BeatNode::sequence("Group", vec![id("a"), id("b")])),
                ("a", hold(4)),
                ("b", hold(6)),
            ],
        ),
        tree(
            &["t"],
            vec![("t", repeat("a", 1_000_000_000)), ("a", hold(10))],
        ),
        tree(
            &["t"],
            vec![
                ("t", retime("a", 2, 8, 12, RetimePurpose::Edit)),
                ("a", hold(10)),
            ],
        ),
    ];
    for document in fixtures {
        let original = document.nodes()[&id("t")].clone();
        let before_duration = document.durations().unwrap()[document.root()];
        let after = split(&document, "t", 3, "cut");
        assert_eq!(after.nodes()[&id("t")], original);
        assert_eq!(after.durations().unwrap()[after.root()], before_duration);
        let sides = children(&after, "root");
        assert_eq!(sides, &[id("cut-0"), id("cut-1")]);
        assert_eq!(partition(&after, &sides[0]).0, &id("t"));
        assert_eq!(partition(&after, &sides[0]).1.end(), ProjectFrame(3));
        let (right, mapping) = partition(&after, &sides[1]);
        assert_eq!(mapping.start(), ProjectFrame(3));
        assert_eq!(after.durations().unwrap()[right], before_duration);
        if let NodeKind::Repeat { iterations, .. } = &after.nodes()[right].kind {
            assert_eq!(iterations.len(), 1_000_000_000);
            assert_eq!(iterations.segment_count(), 1);
            assert_eq!(
                iterations.at(999_999_999).unwrap().allocation,
                RevisionId::new("plays").unwrap()
            );
        }
    }
}

#[test]
fn source_identity_and_original_clock_scopes_survive_the_cut() {
    let document = tree(&["t"], vec![("t", hold(10))]);
    let mut wire = serde_json::to_value(document).unwrap();
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
    let asset = AssetId::new("original").unwrap();
    wire["assets"]["original"] = serde_json::to_value(AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(span),
        audio: None,
        still_image: false,
        frame_count: None,
        source_qualification: None,
    })
    .unwrap();
    wire["nodes"]["t"]["kind"] = serde_json::to_value(NodeKind::Source {
        source: SourceNode {
            duration: duration(10),
            video: SourceVideo::Stream {
                asset: asset.clone(),
                span,
            },
            audio: None,
            link: LinkRelation::Independent,
            video_mapping: SourceVideoMapping::FitBeat,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
        },
    })
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let document = with_marks(
        &document,
        vec![(
            "source",
            mark(
                "t",
                Anchor::Source {
                    asset,
                    moment: SourceMoment::Timestamp {
                        stream: SourceStream::Video,
                        timestamp: SourceTimestamp {
                            ticks: 7,
                            time_base: SourceTimeBase::new(1, 30).unwrap(),
                        },
                    },
                },
                InsertionBias::Right,
            ),
        )],
    );
    let after = split(&document, "t", 4, "cut");
    assert!(matches!(
        after.nodes()[&id("t")].kind,
        NodeKind::Source { .. }
    ));
    assert_eq!(stored(&after, "source").binding_count(), 2);
    assert_eq!(
        query(&after, "source", None).unwrap_err().code,
        AnchorErrorCode::OccurrenceRequired
    );
    assert_eq!(
        query(&after, "source", Some(path("t"))).unwrap_err().code,
        AnchorErrorCode::OutsideMapping
    );
    assert_eq!(
        query(&after, "source", Some(path("cut-2")))
            .unwrap()
            .exact_frame,
        ExactRatio::integer(7)
    );
}

#[test]
fn root_cut_keeps_root_scopes_and_ownership_while_copying_descendant_hosts() {
    let document = with_marks(
        &tree(&["a", "b"], vec![("a", hold(4)), ("b", hold(6))]),
        vec![
            ("root", mark("root", local("root", 7), InsertionBias::Right)),
            ("child", mark("root", local("b", 2), InsertionBias::Right)),
            ("owned", mark("a", local("root", 7), InsertionBias::Right)),
        ],
    );
    let after = split(&document, "root", 3, "cut");
    assert_eq!(after.root(), document.root());
    assert_eq!(children(&after, "root"), &[id("cut-0"), id("cut-1")]);
    assert_eq!(partition(&after, &id("cut-0")).0, &id("cut-2"));
    assert_eq!(stored(&after, "root"), stored(&document, "root"));
    assert_eq!(stored(&after, "child").binding_count(), 2);
    assert!(
        stored(&after, "child")
            .bindings()
            .all(|binding| binding.owner == id("root"))
    );
    assert_eq!(stored(&after, "owned").binding_count(), 2);
    assert!(
        stored(&after, "owned")
            .bindings()
            .all(|binding| binding.coordinate == local("root", 7))
    );
    assert_eq!(
        query(&after, "child", None).unwrap().exact_frame,
        ExactRatio::integer(6)
    );
}

#[test]
fn non_sequence_default_child_and_sparse_override_gain_one_local_container() {
    let document = tree(&["r"], vec![("r", repeat("t", 3)), ("t", hold(10))]);
    let after = split(&document, "t", 4, "cut");
    let NodeKind::Repeat {
        child, iterations, ..
    } = &after.nodes()[&id("r")].kind
    else {
        panic!("Repeat")
    };
    assert_eq!(child, &id("cut-3"));
    assert_eq!(iterations.len(), 3);
    assert_eq!(children(&after, "cut-3"), &[id("cut-0"), id("cut-1")]);
    assert_eq!(after.durations().unwrap()[after.root()], duration(30));

    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"]["override"] = serde_json::to_value(hold(7)).unwrap();
    wire["overrides"]["r"] =
        json!([{ "iteration":{"allocation":"plays","ordinal":1},"root":"override" }]);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = split(&document, "override", 2, "over");
    let play = IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal: 1,
    };
    assert_eq!(after.overrides()[&id("r")].get(&play), Some(&id("over-3")));
    assert_eq!(after.durations().unwrap()[after.root()], duration(27));
}

#[test]
fn refinement_after_hundreds_of_cuts_keeps_context_depth_and_absolute_mappings_constant() {
    let mut document = tree(&["t"], vec![("t", hold(300))]);
    document = split(&document, "t", 1, "first");
    let mut target = "first-1".to_owned();
    for cut in 2..=270 {
        let prefix = format!("cut{cut}");
        document = split(&document, &target, 1, &prefix);
        target = format!("{prefix}-0");
    }
    let mut expected = 0;
    for side in children(&document, "root") {
        let (context, mapping) = partition(&document, side);
        assert_eq!(mapping.start(), ProjectFrame(expected));
        expected = mapping.end().0;
        assert!(matches!(
            document.nodes()[context].kind,
            NodeKind::Hold { .. }
        ));
        assert_eq!(document.durations().unwrap()[context], duration(300));
    }
    assert_eq!(expected, 300);
    assert_eq!(document.nodes().len(), 1 + 271 * 2);
}

#[test]
fn paired_local_bindings_remain_bound_when_owner_and_host_have_different_visibility() {
    let document = tree(
        &["r", "outside"],
        vec![
            ("r", repeat("group", 3)),
            ("group", BeatNode::sequence("Group", vec![id("a"), id("b")])),
            ("a", hold(4)),
            ("b", hold(6)),
            ("outside", hold(5)),
        ],
    );
    let mut unresolved = mark("a", local("missing", 1), InsertionBias::Right);
    unresolved.state = MarkState::Unresolved {
        reason: MarkLossReason::HostMissing,
    };
    unresolved.loss_policy = AnchorLossPolicy::KeepUnresolved;
    let document = with_marks(
        &document,
        vec![
            ("cross", mark("a", local("b", 1), InsertionBias::Right)),
            (
                "outside",
                mark("a", local("outside", 2), InsertionBias::Right),
            ),
            ("unresolved", unresolved),
        ],
    );
    let after = split(&document, "r", 2, "cut");
    assert_eq!(after.marks().len(), 3);
    for name in ["cross", "outside", "unresolved"] {
        assert_eq!(stored(&after, name).binding_count(), 2);
    }
    assert!(
        stored(&after, "cross")
            .bindings()
            .all(|binding| binding.state == MarkState::Bound)
    );
    assert!(
        stored(&after, "outside")
            .bindings()
            .all(|binding| binding.coordinate == local("outside", 2))
    );
    assert!(
        stored(&after, "unresolved")
            .bindings()
            .all(|binding| binding.coordinate == local("missing", 1))
    );
    assert_eq!(
        query(&after, "cross", None).unwrap_err().code,
        AnchorErrorCode::OccurrenceRequired
    );
    let scope = InstancePath {
        node: id("cut-5"),
        repeats: vec![RepeatInstance {
            node: id("cut-2"),
            iteration: IterationId {
                allocation: RevisionId::new("plays").unwrap(),
                ordinal: 0,
            },
        }],
    };
    assert_eq!(
        query(&after, "cross", Some(scope)).unwrap().exact_frame,
        ExactRatio::integer(5)
    );
    let after = edit(&after, Command::Delete { node: id("cut-0") });
    assert_eq!(stored(&after, "cross").binding_count(), 1);
    assert_eq!(stored(&after, "cross").owner, id("cut-4"));
}

#[test]
fn concrete_occurrences_follow_exact_old_output_and_choose_one_side_at_the_seam() {
    for bias in [InsertionBias::Left, InsertionBias::Right] {
        let document = tree(
            &["t"],
            vec![
                ("t", retime("r", 2, 12, 15, RetimePurpose::Edit)),
                ("r", repeat("h", 3)),
                ("h", hold(5)),
            ],
        );
        let occurrence = InstancePath {
            node: id("h"),
            repeats: vec![RepeatInstance {
                node: id("r"),
                iteration: IterationId {
                    allocation: RevisionId::new("plays").unwrap(),
                    ordinal: 1,
                },
            }],
        };
        // (play start 5 + local 1 - crop start 2) * 15/10 = 6.
        let document = with_marks(
            &document,
            vec![(
                "cue",
                mark(
                    "h",
                    Anchor::Occurrence {
                        instance: occurrence,
                        position: ExactRatio::integer(1),
                    },
                    bias,
                ),
            )],
        );
        let after = split(&document, "t", 6, "cut");
        let binding = stored(&after, "cue");
        assert_eq!(binding.binding_count(), 1);
        assert_eq!(
            binding.owner,
            if bias == InsertionBias::Left {
                id("h")
            } else {
                id("cut-4")
            }
        );
        assert_eq!(
            query(&after, "cue", None).unwrap().exact_frame,
            ExactRatio::integer(6)
        );
    }
}

#[test]
fn refined_partition_local_output_coordinates_translate_but_context_coordinates_do_not() {
    for bias in [InsertionBias::Left, InsertionBias::Right] {
        let document = tree(
            &["t"],
            vec![
                ("t", retime("h", 5, 15, 10, RetimePurpose::Partition)),
                ("h", hold(20)),
            ],
        );
        let document = with_marks(
            &document,
            vec![
                ("output", mark("t", local("t", 3), bias)),
                ("context", mark("h", local("h", 8), bias)),
                // This event is valid but hidden by the existing left seam bias.
                (
                    "hidden",
                    mark(
                        "h",
                        Anchor::Occurrence {
                            instance: path("h"),
                            position: ExactRatio::integer(5),
                        },
                        InsertionBias::Left,
                    ),
                ),
            ],
        );
        let after = split(&document, "t", 3, "cut");
        assert_eq!(children(&after, "root"), &[id("t"), id("cut-0")]);
        for side in children(&after, "root") {
            assert!(matches!(
                after.nodes()[side].kind,
                NodeKind::Retime {
                    pitch: PitchPolicy::FollowSpeed,
                    ..
                }
            ));
        }
        assert_eq!(
            partition(&after, &id("t")).1,
            FrameRange::new(ProjectFrame(5), ProjectFrame(8)).unwrap()
        );
        assert_eq!(
            partition(&after, &id("cut-0")).1,
            FrameRange::new(ProjectFrame(8), ProjectFrame(15)).unwrap()
        );
        assert_eq!(stored(&after, "output").binding_count(), 1);
        assert_eq!(
            stored(&after, "output").boundary.coordinate,
            if bias == InsertionBias::Left {
                local("t", 3)
            } else {
                local("cut-0", 0)
            }
        );
        assert_eq!(stored(&after, "context").binding_count(), 2);
        assert_eq!(
            query(&after, "output", None).unwrap().exact_frame,
            ExactRatio::integer(3)
        );
        assert_eq!(
            query(&after, "context", None).unwrap().exact_frame,
            ExactRatio::integer(3)
        );
        assert_eq!(stored(&after, "hidden"), stored(&document, "hidden"));
        assert_eq!(
            query(&after, "hidden", None).unwrap_err().code,
            AnchorErrorCode::OutsideMapping
        );
    }
}

#[test]
fn invalid_boundaries_and_identity_pools_reject_atomically_and_wire_vocabulary_is_strict() {
    let document = tree(&["t"], vec![("t", hold(10))]);
    for (target, at, identities) in [
        ("t", 0, pool("cut", 4)),
        ("t", 10, pool("cut", 4)),
        ("t", 11, pool("cut", 4)),
        ("missing", 1, pool("cut", 4)),
        ("t", 1, pool("cut", 2)),
        (
            "t",
            1,
            SplitIdentities {
                nodes: vec![id("t"), id("a"), id("b")],
            },
        ),
        (
            "t",
            1,
            SplitIdentities {
                nodes: vec![id("a"), id("b"), id("a")],
            },
        ),
    ] {
        let before = document.clone();
        assert!(
            apply(
                &document,
                &request(
                    &document,
                    Command::Split {
                        node: id(target),
                        at: duration(at),
                        identities
                    }
                )
            )
            .is_err()
        );
        assert_eq!(document, before);
    }
    let command = Command::Split {
        node: id("t"),
        at: duration(2),
        identities: pool("cut", 3),
    };
    let mut wire = serde_json::to_value(&command).unwrap();
    assert_eq!(
        serde_json::from_value::<Command>(wire.clone()).unwrap(),
        command
    );
    wire["identities"]["marks"] = json!([]);
    assert!(serde_json::from_value::<Command>(wire).is_err());
}

#[test]
fn split_preflights_logical_mark_binding_growth_without_partial_changes() {
    let document = tree(&["t"], vec![("t", hold(10))]);
    let mut cue = mark("t", local("t", 0), InsertionBias::Right);
    cue.fragments = (1..MAX_MARK_BINDINGS)
        .map(|i| MarkFragment {
            owner: id("t"),
            coordinate: Anchor::Local {
                node: id("t"),
                position: ExactRatio::new(i as i128, MAX_MARK_BINDINGS as i128).unwrap(),
            },
            state: MarkState::Bound,
        })
        .collect();
    let document = with_marks(&document, vec![("cue", cue)]);
    let error = apply(
        &document,
        &request(
            &document,
            Command::Split {
                node: id("t"),
                at: duration(2),
                identities: pool("cut", 3),
            },
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert_eq!(document.nodes().len(), 2);
    assert_eq!(stored(&document, "cue").binding_count(), MAX_MARK_BINDINGS);
}

#[test]
fn sparse_override_and_nonuniform_play_layout_are_copied_with_stable_identities() {
    let document = tree(&["r"], vec![("r", repeat("h", 3)), ("h", hold(5))]);
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"]["override"] = serde_json::to_value(hold(8)).unwrap();
    wire["overrides"]["r"] =
        json!([{ "iteration":{"allocation":"plays","ordinal":1},"root":"override" }]);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let occurrence = InstancePath {
        node: id("override"),
        repeats: vec![RepeatInstance {
            node: id("r"),
            iteration: IterationId {
                allocation: RevisionId::new("plays").unwrap(),
                ordinal: 1,
            },
        }],
    };
    let document = with_marks(
        &document,
        vec![(
            "cue",
            mark(
                "override",
                Anchor::Occurrence {
                    instance: occurrence,
                    position: ExactRatio::integer(2),
                },
                InsertionBias::Right,
            ),
        )],
    );
    let after = split(&document, "r", 7, "cut");
    assert_eq!(after.durations().unwrap()[after.root()], duration(18));
    let play = IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal: 1,
    };
    assert_eq!(
        after.overrides()[&id("r")].get(&play),
        Some(&id("override"))
    );
    assert_eq!(
        after.overrides()[&id("cut-2")].get(&play),
        Some(&id("cut-4"))
    );
    assert_eq!(stored(&after, "cue").owner, id("cut-4"));
    assert_eq!(stored(&after, "cue").binding_count(), 1);
    assert_eq!(
        query(&after, "cue", None).unwrap().exact_frame,
        ExactRatio::integer(7)
    );
}

#[test]
fn one_occurrence_split_isolates_only_the_selected_play_and_copies_one_logical_mark() {
    let document = tree(
        &["r"],
        vec![
            ("r", repeat("group", 3)),
            ("group", BeatNode::sequence("Group", vec![id("a"), id("b")])),
            ("a", hold(4)),
            ("b", hold(6)),
        ],
    );
    let document = with_marks(
        &document,
        vec![("cue", mark("a", local("b", 1), InsertionBias::Right))],
    );
    let iteration = IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal: 1,
    };
    let after = edit(
        &document,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("group"),
                repeats: vec![RepeatInstance {
                    node: id("r"),
                    iteration: iteration.clone(),
                }],
            },
            edit: OccurrenceEdit::Split {
                at: duration(2),
                identities: pool("cut", 6),
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("isolated-group"), id("isolated-a"), id("isolated-b")],
                marks: vec![MarkId::new("isolated-cue").unwrap()],
            },
        },
    );
    assert_eq!(after.nodes()[&id("group")], document.nodes()[&id("group")]);
    assert_eq!(
        after.overrides()[&id("r")].get(&iteration),
        Some(&id("cut-5"))
    );
    assert_eq!(after.marks().len(), 2);
    assert_eq!(stored(&after, "cue"), stored(&document, "cue"));
    assert_eq!(stored(&after, "isolated-cue").binding_count(), 2);
    assert_eq!(after.durations().unwrap()[after.root()], duration(30));
    let scope = InstancePath {
        node: id("cut-4"),
        repeats: vec![RepeatInstance {
            node: id("r"),
            iteration,
        }],
    };
    assert_eq!(
        query(&after, "isolated-cue", Some(scope))
            .unwrap()
            .exact_frame,
        ExactRatio::integer(15)
    );
}

#[test]
fn exact_fractional_occurrence_side_is_not_chosen_from_the_rounded_project_frame() {
    for (position, right) in [
        (ExactRatio::new(11, 4).unwrap(), false),
        (ExactRatio::new(13, 4).unwrap(), true),
    ] {
        let document = with_marks(
            &tree(&["t"], vec![("t", hold(10))]),
            vec![(
                "cue",
                mark(
                    "t",
                    Anchor::Occurrence {
                        instance: path("t"),
                        position,
                    },
                    InsertionBias::Right,
                ),
            )],
        );
        let after = split(&document, "t", 3, "cut");
        assert_eq!(
            stored(&after, "cue").owner,
            if right { id("cut-2") } else { id("t") }
        );
        assert_eq!(query(&after, "cue", None).unwrap().exact_frame, position);
        assert_eq!(stored(&after, "cue").binding_count(), 1);
    }
}

#[test]
fn root_authored_edges_move_into_both_full_contexts_without_new_partition_edges() {
    let document = edit(
        &tree(&["h"], vec![("h", hold(10))]),
        Command::SetAudioEdge {
            node: id("root"),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
    );
    let after = split(&document, "root", 4, "cut");
    assert_eq!(
        after.nodes()[after.root()].audio_edges,
        AudioEdgePolicies::default()
    );
    for side in children(&after, "root") {
        let (context, _) = partition(&after, side);
        assert_eq!(
            after.nodes()[context].audio_edges,
            document.nodes()[document.root()].audio_edges
        );
    }
}

#[test]
fn separate_mark_limits_also_bound_total_fragment_growth() {
    let document = tree(&["t"], vec![("t", hold(10))]);
    let mut cue = mark("t", local("t", 0), InsertionBias::Right);
    cue.fragments = (1..500)
        .map(|i| MarkFragment {
            owner: id("t"),
            coordinate: Anchor::Local {
                node: id("t"),
                position: ExactRatio::new(i, 500).unwrap(),
            },
            state: MarkState::Bound,
        })
        .collect();
    let mut wire = serde_json::to_value(&document).unwrap();
    let marks: BTreeMap<_, _> = (0..101)
        .map(|i| (MarkId::new(format!("cue-{i}")).unwrap(), cue.clone()))
        .collect();
    wire["marks"] = serde_json::to_value(marks).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let error = apply(
        &document,
        &request(
            &document,
            Command::Split {
                node: id("t"),
                at: duration(2),
                identities: pool("cut", 3),
            },
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("total document mark binding"));
    assert_eq!(document.nodes().len(), 2);
}

#[test]
fn split_under_an_authored_retime_keeps_its_clock_and_mark_moves_follow_the_chosen_piece() {
    let document = with_marks(
        &tree(
            &["outer", "tail"],
            vec![
                ("outer", retime("t", 2, 8, 12, RetimePurpose::Edit)),
                ("t", hold(10)),
                ("tail", hold(4)),
            ],
        ),
        vec![(
            "cue",
            mark(
                "t",
                Anchor::Occurrence {
                    instance: path("t"),
                    position: ExactRatio::integer(6),
                },
                InsertionBias::Right,
            ),
        )],
    );
    let after = split(&document, "t", 5, "cut");
    assert_eq!(after.durations().unwrap()[after.root()], duration(16));
    assert_eq!(
        query(&after, "cue", None).unwrap().exact_frame,
        ExactRatio::integer(8)
    );
    assert_eq!(stored(&after, "cue").owner, id("cut-2"));
    assert_eq!(children(&after, "cut-3"), &[id("cut-0"), id("cut-1")]);

    let document = with_marks(
        &tree(&["t", "tail"], vec![("t", hold(10)), ("tail", hold(4))]),
        vec![("cue", mark("t", local("t", 6), InsertionBias::Right))],
    );
    let after = split(&document, "t", 5, "cut");
    assert_eq!(
        query(&after, "cue", None).unwrap().exact_frame,
        ExactRatio::integer(6)
    );
    let after = edit(
        &after,
        Command::Move {
            node: id("cut-1"),
            parent: id("root"),
            index: 2,
        },
    );
    assert_eq!(
        query(&after, "cue", None).unwrap().exact_frame,
        ExactRatio::integer(10)
    );
    assert_eq!(stored(&after, "cue").binding_count(), 2);
    let after = edit(&after, Command::Delete { node: id("cut-1") });
    assert_eq!(stored(&after, "cue").binding_count(), 1);
    assert_eq!(stored(&after, "cue").state, MarkState::Bound);
    assert_eq!(
        query(&after, "cue", None).unwrap_err().code,
        AnchorErrorCode::OutsideMapping
    );
}

#[test]
fn node_capacity_and_excessive_identity_pools_fail_before_inserting_any_context() {
    let document = tree(&["t"], vec![("t", hold(10))]);
    let error = apply(
        &document,
        &request(
            &document,
            Command::Split {
                node: id("t"),
                at: duration(1),
                identities: pool("unused", MAX_DOCUMENT_NODES + 1),
            },
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    let mut wire = serde_json::to_value(&document).unwrap();
    for i in 2..MAX_DOCUMENT_NODES {
        let name = format!("empty-{i}");
        wire["nodes"][&name] = serde_json::to_value(BeatNode::sequence("", vec![])).unwrap();
        wire["nodes"]["root"]["kind"]["children"]
            .as_array_mut()
            .unwrap()
            .push(json!(name));
    }
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let error = apply(
        &document,
        &request(
            &document,
            Command::Split {
                node: id("t"),
                at: duration(1),
                identities: pool("cut", 3),
            },
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert_eq!(document.nodes().len(), MAX_DOCUMENT_NODES);
    assert!(
        document
            .nodes()
            .keys()
            .all(|node| !node.as_str().starts_with("cut-"))
    );
}

#[test]
fn external_sequence_and_root_occurrence_coordinates_keep_one_event_with_two_owner_claims() {
    for (coordinate, after_deletion) in [
        (
            Anchor::Sequence {
                frame: ProjectFrame(8),
            },
            8,
        ),
        (
            Anchor::Occurrence {
                instance: path("root"),
                position: ExactRatio::integer(8),
            },
            4,
        ),
    ] {
        let document = with_marks(
            &tree(&["t", "tail"], vec![("t", hold(10)), ("tail", hold(10))]),
            vec![("cue", mark("t", coordinate, InsertionBias::Right))],
        );
        let after = split(&document, "t", 4, "cut");
        assert_eq!(after.marks().len(), 1);
        assert_eq!(stored(&after, "cue").binding_count(), 2);
        let event = query(&after, "cue", None).unwrap();
        assert_eq!(event.exact_frame, ExactRatio::integer(8));
        assert_eq!(event.mark.unwrap().bindings, vec![0, 1]);
        let after = edit(&after, Command::Delete { node: id("cut-0") });
        assert_eq!(after.marks().len(), 1);
        assert_eq!(stored(&after, "cue").binding_count(), 1);
        assert_eq!(stored(&after, "cue").owner, id("cut-2"));
        let event = query(&after, "cue", None).unwrap();
        assert_eq!(event.exact_frame, ExactRatio::integer(after_deletion));
        assert_eq!(event.mark.unwrap().bindings, vec![0]);
    }
}
