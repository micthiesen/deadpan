use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::json;

#[path = "insert_time/delete.rs"]
mod delete;
#[path = "insert_time/delete_children.rs"]
mod delete_children;
#[path = "insert_time/delete_range.rs"]
mod delete_range;
#[path = "insert_time/interior.rs"]
mod interior;
#[path = "insert_time/nested_sequence.rs"]
mod nested_sequence;
#[path = "insert_time/occurrence_structure.rs"]
mod occurrence_structure;
#[path = "insert_time/retimed.rs"]
mod retimed;
#[path = "insert_time/source_replace.rs"]
mod source_replace;
#[path = "insert_time/source_splice.rs"]
mod source_splice;
#[path = "insert_time/source_splice_interior.rs"]
mod source_splice_interior;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn recipe(frames: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: duration(frames),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn source(frames: i64) -> BeatNode {
    BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "Original".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: duration(frames),
                video: SourceVideo::Stream {
                    asset: AssetId::new("media").unwrap(),
                    span: source_span(),
                },
                audio: None,
                link: LinkRelation::Independent,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        },
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn source_span() -> SourceSpan {
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
        SourceTimestamp {
            ticks: 100,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
    )
    .unwrap()
}
fn tree(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Project", children.iter().map(|name| id(name)).collect()),
    );
    ProjectDocument::from_json(&json!({
        "schema_version": DOCUMENT_SCHEMA_VERSION, "project_id":"pause", "revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root", "assets":{"media": AssetRecord {
            label: "Media".into(), content_hash: "a".repeat(64), video: Some(source_span()),
            audio: None, still_image: false, frame_count: None, source_qualification: None,
        }}, "marks":{}, "overrides":{}, "nodes":nodes,
    }).to_string()).unwrap()
}
fn pool(prefix: &str, count: usize) -> SplitIdentities {
    SplitIdentities {
        nodes: (0..count)
            .map(|index| id(&format!("{prefix}-{index}")))
            .collect(),
    }
}
fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    }
}
fn insertion(document: &ProjectDocument, name: &str, at: i64, frames: i64) -> CommandRequest {
    request(
        document,
        name,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: recipe(frames),
            id: id(&format!("pause-{name}")),
            identities: pool(name, 3),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn ai_insertion_matches_silent_freeze_inside_source_hold_and_nested_sequence() {
    let frozen = || {
        BeatNode::hold(
            "Held",
            HoldRecipe {
                video: HoldVideo::Freeze {
                    asset: AssetId::new("media").unwrap(),
                    timestamp: source_span().start(),
                },
                ..recipe(10)
            },
        )
    };
    let documents = [
        tree(&["source"], vec![("source", source(10))]),
        tree(&["held"], vec![("held", frozen())]),
        tree(
            &["group"],
            vec![
                ("group", BeatNode::sequence("Group", vec![id("source")])),
                ("source", source(10)),
            ],
        ),
    ];
    for document in documents {
        for at in [0, 4, 10] {
            let mut ordinary = insertion(&document, "insert", at, 15);
            let Command::InsertTime { hold, .. } = &mut ordinary.command else {
                panic!()
            };
            hold.video = HoldVideo::Freeze {
                asset: AssetId::new("media").unwrap(),
                timestamp: source_span().start(),
            };
            let Command::InsertTime {
                at,
                hold,
                id,
                identities,
                timing,
            } = ordinary.command.clone()
            else {
                panic!()
            };
            let ai = CommandRequest {
                command: Command::InsertAiTime {
                    at,
                    hold,
                    id,
                    identities,
                    timing,
                },
                ..ordinary.clone()
            };
            let expected = apply_with_result(&document, &ordinary).unwrap().1;
            let (edit, actual) = apply_with_result(&document, &ai).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(actual.duration().unwrap().frames(), 25);
            assert_eq!(edit.inverse.apply(&actual).unwrap(), document);
        }
    }
}
fn edit(document: &ProjectDocument, request: CommandRequest) -> ProjectDocument {
    let request_wire = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&request_wire).unwrap(),
        request
    );
    let transaction = apply(document, &request).unwrap();
    let wire = serde_json::to_string(&transaction).unwrap();
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&wire).unwrap(),
        transaction
    );
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    result
}
fn children(document: &ProjectDocument) -> &[NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        panic!("Sequence")
    };
    children
}
fn owner(document: &ProjectDocument, root_child: &NodeId) -> NodeId {
    match &document.nodes()[root_child].kind {
        NodeKind::Retime {
            child,
            purpose: RetimePurpose::Partition,
            ..
        } => child.clone(),
        _ => root_child.clone(),
    }
}
fn split(document: &ProjectDocument, name: &str, target: NodeId, at: i64) -> ProjectDocument {
    edit(
        document,
        request(
            document,
            name,
            Command::Split {
                node: target,
                at: duration(at),
                identities: pool(name, 3),
            },
        ),
    )
}

#[test]
fn start_interior_end_and_empty_project_insert_one_reversible_pause() {
    for at in [0, 1, 3] {
        let before = tree(&["original"], vec![("original", source(3))]);
        let request = insertion(&before, "pause", at, 2);
        let transaction = apply(&before, &request).unwrap();
        assert_eq!(transaction.duration_delta, 2);
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(5));
        assert_eq!(
            after.nodes()[&id("original")],
            before.nodes()[&id("original")]
        );
        assert_eq!(
            after.nodes()[&id("pause-pause")],
            BeatNode::hold("Pause", recipe(2))
        );
        assert_eq!(children(&after).len(), if at == 1 { 3 } else { 2 });
        let index = if at == 0 { 0 } else { 1 };
        assert_eq!(children(&after)[index], id("pause-pause"));
    }
    let before = tree(&[], vec![]);
    let after = edit(&before, insertion(&before, "first", 0, 2));
    assert_eq!(children(&after), &[id("pause-first")]);
    assert!(after.audio_bindings().is_empty());
}

#[test]
fn first_pause_locks_an_automatic_empty_project_before_final_validation() {
    let before = ProjectDocument::new_automatic(
        ProjectId::new("automatic-pause").unwrap(),
        revision("initial"),
        id("root"),
    )
    .unwrap();
    let after = edit(&before, insertion(&before, "first", 0, 2));
    assert_eq!(after.duration().unwrap(), duration(2));
    assert_eq!(children(&after), &[id("pause-first")]);
    assert_eq!(after.presentation_basis(), before.presentation_basis());
    assert_eq!(after.basis_state().rate_origin, FrameRateOrigin::TimedEdit);
    after.validate().unwrap();
}

fn mark(position: i64, bias: InsertionBias, pinned: bool) -> Mark {
    Mark {
        owner: id("root"),
        label: "Cue".into(),
        boundary: BoundaryAnchor {
            coordinate: if pinned {
                Anchor::Sequence {
                    frame: ProjectFrame(position),
                }
            } else {
                Anchor::Local {
                    node: id("root"),
                    position: ExactRatio::integer(position),
                }
            },
            bias,
        },
        loss_policy: AnchorLossPolicy::DeleteOwned,
        state: MarkState::Bound,
        fragments: vec![],
    }
}
fn mark_position(document: &ProjectDocument, name: &str) -> ExactRatio {
    let selection = AnchorIndex::new(document)
        .unwrap()
        .resolve(&SelectionRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            role: MediaRole::Linked,
            selector: BoundarySelector::Mark {
                target: NamedMarkTarget {
                    id: MarkId::new(name).unwrap(),
                    occurrence: None,
                },
            },
        })
        .unwrap();
    let ResolvedSelectionKind::Point { point } = selection.selection else {
        panic!("point")
    };
    point.exact_frame
}

#[test]
fn split_and_insertion_compose_mark_bias_and_keep_sequence_pins() {
    let before = tree(&["original"], vec![("original", source(4))]);
    let mut wire = serde_json::to_value(before).unwrap();
    wire["marks"] = serde_json::to_value(BTreeMap::from([
        ("left", mark(1, InsertionBias::Left, false)),
        ("right", mark(1, InsertionBias::Right, false)),
        ("content", mark(3, InsertionBias::Right, false)),
        ("pinned", mark(3, InsertionBias::Right, true)),
    ]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, insertion(&before, "insert", 1, 2));
    assert_eq!(mark_position(&after, "left"), ExactRatio::ONE);
    assert_eq!(mark_position(&after, "right"), ExactRatio::integer(3));
    assert_eq!(mark_position(&after, "content"), ExactRatio::integer(5));
    assert_eq!(mark_position(&after, "pinned"), ExactRatio::integer(3));
}

#[test]
fn zero_bounds_stale_revision_and_identity_errors_leave_input_unchanged() {
    let before = tree(&["original"], vec![("original", source(4))]);
    let saved = before.to_json().unwrap();
    for (at, frames, code) in [
        (0, 0, EditErrorCode::InvalidDuration),
        (-1, 1, EditErrorCode::InvalidCommand),
        (5, 1, EditErrorCode::InvalidCommand),
    ] {
        assert_eq!(
            apply(&before, &insertion(&before, "bad", at, frames))
                .unwrap_err()
                .code,
            code
        );
    }
    let mut bad = insertion(&before, "bad", 1, 1);
    bad.expected_revision = revision("stale");
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    let mut bad = insertion(&before, "bad", 1, 1);
    let Command::InsertTime { timing, .. } = &mut bad.command else {
        unreachable!()
    };
    timing.allocation = revision("foreign");
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
    let Command::InsertTime {
        id: hold_id,
        identities,
        timing,
        ..
    } = &mut bad.command
    else {
        unreachable!()
    };
    timing.allocation = revision("bad");
    identities.nodes[0] = hold_id.clone();
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::IdentityConflict
    );
    let mut bad = insertion(&before, "bad", 1, 1);
    let Command::InsertTime { identities, .. } = &mut bad.command else {
        unreachable!()
    };
    identities.nodes.clear();
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
    assert_eq!(before.to_json().unwrap(), saved);
    edit(&before, insertion(&before, "valid", 1, 1));
}

#[test]
fn composite_seam_moves_root_marks_once_and_retains_occurrence_and_source_clocks() {
    let before = tree(
        &["lead", "repeat"],
        vec![
            ("lead", source(1)),
            (
                "repeat",
                BeatNode {
                    audio_treatments: Default::default(),
                    label: "Repeated Original".into(),
                    framing: None,
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("inner"),
                        iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
                        gap: Some(recipe(1)),
                        escalation: None,
                    },
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                },
            ),
            ("inner", source(2)),
        ],
    );
    let mut occurrence = mark(0, InsertionBias::Right, false);
    occurrence.owner = id("inner");
    occurrence.boundary.coordinate = Anchor::Occurrence {
        instance: InstancePath {
            node: id("inner"),
            repeats: vec![RepeatInstance {
                node: id("repeat"),
                iteration: IterationId {
                    allocation: revision("plays"),
                    ordinal: 1,
                },
            }],
        },
        position: ExactRatio::ONE,
    };
    let mut source_mark = occurrence.clone();
    source_mark.boundary.coordinate = Anchor::Source {
        asset: AssetId::new("media").unwrap(),
        moment: SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks: 5,
                time_base: SourceTimeBase::new(1, 30).unwrap(),
            },
        },
    };
    let mut unresolved = mark(5, InsertionBias::Right, false);
    unresolved.state = MarkState::Unresolved {
        reason: MarkLossReason::WrapAmbiguous,
    };
    unresolved.loss_policy = AnchorLossPolicy::KeepUnresolved;
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["marks"] = serde_json::to_value(BTreeMap::from([
        ("left", mark(1, InsertionBias::Left, false)),
        ("right", mark(1, InsertionBias::Right, false)),
        ("content", mark(5, InsertionBias::Right, false)),
        ("pinned", mark(5, InsertionBias::Right, true)),
        ("occurrence", occurrence),
        ("source", source_mark),
        ("unresolved", unresolved),
    ]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(mark_position(&before, "occurrence"), ExactRatio::integer(5));
    let after = edit(&before, insertion(&before, "composite", 1, 2));
    for (name, position) in [
        ("left", 1),
        ("right", 3),
        ("content", 7),
        ("pinned", 5),
        ("occurrence", 7),
    ] {
        assert_eq!(
            mark_position(&after, name),
            ExactRatio::integer(position),
            "{name}"
        );
    }
    for name in ["occurrence", "source", "unresolved"] {
        let key = MarkId::new(name).unwrap();
        assert_eq!(after.marks()[&key], before.marks()[&key], "{name}");
    }
}

#[test]
fn duration_overflow_and_nonfallback_provider_are_rejected_before_capture() {
    let before = tree(&["original"], vec![("original", source(i64::MAX))]);
    assert_eq!(
        apply(&before, &insertion(&before, "overflow", 0, 1))
            .unwrap_err()
            .code,
        EditErrorCode::TimingOverflow
    );
    let before = tree(&["original"], vec![("original", source(4))]);
    let mut request = insertion(&before, "provider", 1, 1);
    let Command::InsertTime { hold, .. } = &mut request.command else {
        unreachable!()
    };
    hold.video = HoldVideo::Accepted {
        asset: AssetId::new("media").unwrap(),
        frames: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
    };
    let error = apply(&before, &request).unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(error.message.contains("Background, Freeze or Reverse"));
}
