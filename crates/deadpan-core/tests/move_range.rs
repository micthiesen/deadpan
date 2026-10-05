use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::json;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn range(a: i64, b: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap()
}
fn hold(n: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(n),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    )
}
fn sequence(children: &[&str]) -> BeatNode {
    BeatNode::sequence("Group", children.iter().map(|id| node(id)).collect())
}
fn tree(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(id, value)| (node(id), value))
        .collect();
    nodes.insert(node("root"), sequence(children));
    ProjectDocument::from_json(&json!({
        "schema_version":DOCUMENT_SCHEMA_VERSION,"project_id":"move","revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root","nodes":nodes,"assets":{},"marks":{},"overrides":{},
    }).to_string()).unwrap()
}
fn seam(parent: &str, index: usize) -> MoveRangeDestination {
    MoveRangeDestination::Seam {
        parent: node(parent),
        index,
    }
}
fn interior(parent: &str, target: &str, at: i64) -> MoveRangeDestination {
    MoveRangeDestination::Interior {
        parent: node(parent),
        target: node(target),
        at: frames(at),
    }
}
fn request(
    document: &ProjectDocument,
    name: &str,
    parent: &str,
    selected: FrameRange,
    destination: MoveRangeDestination,
) -> CommandRequest {
    let required = document
        .range_move(&node(parent), selected, &destination)
        .unwrap();
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::MoveRange {
            source_revision: document.revision_id().clone(),
            source_parent: node(parent),
            range: selected,
            destination,
            identities: SplitIdentities {
                nodes: (0..required.required_ids)
                    .map(|i| node(&format!("{name}-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    }
}
fn checked(document: &ProjectDocument, request: &CommandRequest) -> ProjectDocument {
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(request).unwrap()).unwrap(),
        *request
    );
    let edit = apply(document, request).unwrap();
    assert_eq!(edit.duration_delta, 0);
    let after = edit.forward.apply(document).unwrap();
    assert_eq!(edit.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
fn children(document: &ProjectDocument, parent: &str) -> Vec<String> {
    let NodeKind::Sequence { children } = &document.nodes()[&node(parent)].kind else {
        panic!("Sequence")
    };
    children.iter().map(ToString::to_string).collect()
}
fn windows(document: &ProjectDocument) -> Vec<FrameRange> {
    children(document, "root")
        .iter()
        .map(|id| match &document.nodes()[&node(id)].kind {
            NodeKind::Retime {
                mapping,
                purpose: RetimePurpose::Partition,
                ..
            } => *mapping,
            _ => panic!("partition"),
        })
        .collect()
}
fn marked(
    document: &ProjectDocument,
    marks: Vec<(&str, &str, Anchor, InsertionBias)>,
) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["marks"] = serde_json::to_value(
        marks
            .into_iter()
            .map(|(id, owner, coordinate, bias)| {
                (
                    MarkId::new(id).unwrap(),
                    Mark {
                        owner: node(owner),
                        label: id.into(),
                        boundary: BoundaryAnchor { coordinate, bias },
                        loss_policy: AnchorLossPolicy::KeepUnresolved,
                        state: MarkState::Bound,
                        fragments: Vec::new(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn local(id: &str, at: i64) -> Anchor {
    Anchor::Local {
        node: node(id),
        position: ExactRatio::integer(at),
    }
}
fn position(document: &ProjectDocument, id: &str) -> ExactRatio {
    match document.marks()[&MarkId::new(id).unwrap()]
        .boundary
        .coordinate
    {
        Anchor::Local { position, .. } => position,
        _ => panic!("local mark"),
    }
}

#[test]
fn whole_units_move_in_both_directions_without_new_authored_identities() {
    // Asserts the authored reference representation (every reanchor step
    // and complete timing tables). tests/timing_representation.rs proves the
    // compact storage resolves and renders identically.
    deadpan_core::with_reference_timing_representation(|| {
        let before = tree(
            &["a", "b", "c"],
            vec![("a", hold(2)), ("b", hold(3)), ("c", hold(4))],
        );
        let right = checked(
            &before,
            &request(&before, "right", "root", range(0, 2), seam("root", 3)),
        );
        assert_eq!(children(&right, "root"), ["b", "c", "a"]);
        assert_eq!(
            right.nodes().keys().collect::<Vec<_>>(),
            before.nodes().keys().collect::<Vec<_>>()
        );
        for id in ["a", "b", "c"] {
            assert_eq!(right.nodes()[&node(id)], before.nodes()[&node(id)]);
        }
        let back = checked(
            &right,
            &request(&right, "back", "root", range(7, 9), seam("root", 0)),
        );
        assert_eq!(back.nodes(), before.nodes());
        assert_eq!(right.audio_bindings().timings().len(), 1);
        for id in ["a", "b", "c"] {
            assert_eq!(
                right.audio_bindings().bindings()[&node(id)].reanchors.len(),
                1
            );
        }
    })
}

#[test]
fn three_cuts_share_one_context_budget_and_reanchor_each_island_once() {
    let before = tree(&["a"], vec![("a", hold(10))]);
    let destination = interior("root", "a", 7);
    let plan = before
        .range_move(&node("root"), range(2, 4), &destination)
        .unwrap();
    assert_eq!(plan.required_ids, 7);
    assert_eq!(plan.timing_slots, 2);
    assert_eq!(plan.inserted, range(5, 7));
    assert_eq!(plan.removal_join, ProjectFrame(2));
    let after = checked(
        &before,
        &request(&before, "three", "root", range(2, 4), destination),
    );
    assert_eq!(after.nodes().len(), before.nodes().len() + 7);
    assert_eq!(
        windows(&after),
        [range(0, 2), range(4, 7), range(2, 4), range(7, 10)]
    );
    let steps: Vec<_> = children(&after, "root")
        .iter()
        .map(|id| {
            let NodeKind::Retime { child, .. } = &after.nodes()[&node(id)].kind else {
                panic!()
            };
            after.audio_bindings().bindings()[child].reanchors.len()
        })
        .collect();
    assert_eq!(steps, [0, 1, 1, 0]);
    assert_eq!(after.audio_bindings().timings().len(), 2);
}

#[test]
fn partial_boundary_noops_do_not_split_or_capture_even_at_maximum_ordinal() {
    let before = tree(&["a"], vec![("a", hold(10))]);
    for at in [2, 7] {
        let destination = interior("root", "a", at);
        let plan = before
            .range_move(&node("root"), range(2, 7), &destination)
            .unwrap();
        assert!(plan.is_noop);
        assert_eq!((plan.required_ids, plan.timing_slots), (0, 0));
        let mut command = request(
            &before,
            &format!("noop-{at}"),
            "root",
            range(2, 7),
            destination,
        );
        let Command::MoveRange { timing, .. } = &mut command.command else {
            panic!()
        };
        timing.ordinal = u32::MAX;
        let after = checked(&before, &command);
        let mut wire = serde_json::to_value(&after).unwrap();
        wire["revision_id"] = json!(before.revision_id());
        assert_eq!(
            ProjectDocument::from_json(&wire.to_string()).unwrap(),
            before
        );
    }
}

fn groups() -> ProjectDocument {
    tree(
        &["left", "middle", "right"],
        vec![
            ("left", sequence(&["a", "b"])),
            ("a", hold(2)),
            ("b", hold(3)),
            ("middle", hold(4)),
            ("right", sequence(&["c"])),
            ("c", hold(5)),
        ],
    )
}

#[test]
fn cross_parent_moves_allow_both_ancestor_relationships_and_retain_empty_sources() {
    let before = groups();
    let out = checked(
        &before,
        &request(&before, "out", "left", range(0, 2), seam("root", 2)),
    );
    assert_eq!(children(&out, "root"), ["left", "middle", "a", "right"]);
    assert_eq!(children(&out, "left"), ["b"]);
    let into = checked(
        &before,
        &request(&before, "into", "root", range(5, 9), seam("left", 1)),
    );
    assert_eq!(children(&into, "root"), ["left", "right"]);
    assert_eq!(children(&into, "left"), ["a", "middle", "b"]);
    let empty = checked(
        &before,
        &request(&before, "empty", "left", range(0, 5), seam("right", 1)),
    );
    assert!(children(&empty, "left").is_empty());
    assert_eq!(children(&empty, "right"), ["c", "a", "b"]);
    assert!(empty.nodes().contains_key(&node("left")));
}

#[test]
fn marks_keep_identity_bias_pins_and_cross_parent_loss_policy() {
    use InsertionBias::{Left, Right};
    let before = marked(
        &groups(),
        vec![
            ("root-content", "root", local("root", 1), Right),
            ("parent-content", "left", local("left", 1), Right),
            ("child", "a", local("a", 1), Right),
            (
                "pin",
                "root",
                Anchor::Sequence {
                    frame: ProjectFrame(1),
                },
                Right,
            ),
        ],
    );
    let after = checked(
        &before,
        &request(&before, "cross", "left", range(0, 2), seam("right", 1)),
    );
    assert_eq!(
        after.marks().keys().collect::<Vec<_>>(),
        before.marks().keys().collect::<Vec<_>>()
    );
    assert_eq!(position(&after, "root-content"), ExactRatio::integer(13));
    assert_eq!(position(&after, "child"), ExactRatio::ONE);
    let departed = &after.marks()[&MarkId::new("parent-content").unwrap()];
    assert_eq!(
        departed.state,
        MarkState::Unresolved {
            reason: MarkLossReason::OutsideHost
        }
    );
    assert_eq!(
        departed.boundary,
        before.marks()[&MarkId::new("parent-content").unwrap()].boundary
    );
    assert_eq!(
        after.marks()[&MarkId::new("pin").unwrap()],
        before.marks()[&MarkId::new("pin").unwrap()]
    );
    let before = marked(
        &tree(&["a"], vec![("a", hold(10))]),
        vec![
            ("in-left", "root", local("root", 2), Left),
            ("in-right", "root", local("root", 2), Right),
            ("out-left", "root", local("root", 4), Left),
            ("out-right", "root", local("root", 4), Right),
        ],
    );
    let after = checked(
        &before,
        &request(
            &before,
            "bias",
            "root",
            range(2, 4),
            interior("root", "a", 7),
        ),
    );
    for (id, expected) in [
        ("in-left", 2),
        ("in-right", 5),
        ("out-left", 7),
        ("out-right", 2),
    ] {
        assert_eq!(position(&after, id), ExactRatio::integer(expected), "{id}");
    }
}

#[test]
fn co_located_empty_slots_reorder_ownership_without_retiming_the_bus() {
    let before = tree(
        &["zero", "a", "edge", "b", "out", "c"],
        vec![
            ("zero", sequence(&[])),
            ("a", hold(2)),
            ("edge", sequence(&[])),
            ("b", hold(3)),
            ("out", sequence(&[])),
            ("c", hold(4)),
        ],
    );
    let plan = before
        .range_move(&node("root"), range(2, 5), &seam("root", 2))
        .unwrap();
    assert!(!plan.is_noop);
    assert_eq!((plan.required_ids, plan.timing_slots), (0, 0));
    let after = checked(
        &before,
        &request(&before, "empty-slot", "root", range(2, 5), seam("root", 2)),
    );
    assert_eq!(
        children(&after, "root"),
        ["zero", "a", "b", "edge", "out", "c"]
    );
    assert!(after.audio_bindings().is_empty());
    let moved = checked(
        &before,
        &request(&before, "end", "root", range(2, 5), seam("root", 6)),
    );
    assert_eq!(
        children(&moved, "root"),
        ["zero", "a", "edge", "out", "c", "b"]
    );
    let interior_empty = checked(
        &before,
        &request(&before, "inside", "root", range(0, 5), seam("root", 6)),
    );
    assert_eq!(
        children(&interior_empty, "root"),
        ["zero", "out", "c", "a", "edge", "b"]
    );
}

#[test]
fn bound_partial_move_needs_only_one_new_clock_and_nested_windows_remain_editable() {
    let before = tree(&["a"], vec![("a", hold(10))]);
    let first = checked(
        &before,
        &request(
            &before,
            "first",
            "root",
            range(2, 4),
            interior("root", "a", 7),
        ),
    );
    let target = children(&first, "root")[1].clone();
    let destination = interior("root", &target, 2);
    let plan = first
        .range_move(&node("root"), range(0, 1), &destination)
        .unwrap();
    assert_eq!(plan.timing_slots, 1);
    let mut command = request(&first, "bound", "root", range(0, 1), destination);
    let Command::MoveRange { timing, .. } = &mut command.command else {
        panic!()
    };
    timing.ordinal = u32::MAX;
    let after = checked(&first, &command);
    assert!(
        after
            .audio_bindings()
            .timings()
            .contains_key(&AudioTimingId {
                allocation: revision("bound"),
                ordinal: u32::MAX
            })
    );
}

#[test]
fn invalid_scope_source_and_identity_budgets_are_atomic() {
    let before = groups();
    for destination in [seam("left", 0), seam("left", 2)] {
        assert!(
            before
                .range_move(&node("root"), range(0, 5), &destination)
                .is_err()
        );
    }
    assert!(
        before
            .range_move(&node("left"), range(0, 4), &interior("left", "b", 1))
            .is_err()
    );
    assert!(
        before
            .range_move(&node("left"), range(1, 1), &seam("right", 0))
            .is_err()
    );
    let original = request(&before, "failure", "left", range(1, 4), seam("right", 1));
    for kind in 0..5 {
        let mut command = original.clone();
        let Command::MoveRange {
            source_revision,
            identities,
            timing,
            ..
        } = &mut command.command
        else {
            panic!()
        };
        let expected = match kind {
            0 => {
                *source_revision = revision("stale");
                EditErrorCode::RevisionConflict
            }
            1 => {
                identities.nodes.clear();
                EditErrorCode::InvalidCommand
            }
            2 => {
                identities.nodes.push(node("root"));
                EditErrorCode::IdentityConflict
            }
            3 => {
                identities.nodes.push(identities.nodes[0].clone());
                EditErrorCode::IdentityConflict
            }
            _ => {
                timing.ordinal = u32::MAX;
                EditErrorCode::LimitExceeded
            }
        };
        let error = apply(&before, &command).unwrap_err();
        assert_eq!(error.code, expected);
        if kind == 0 {
            assert_eq!(error.current_revision, Some(before.revision_id().clone()));
        }
        assert_eq!(before, groups());
    }
}
