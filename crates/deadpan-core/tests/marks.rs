use std::collections::BTreeMap;

use deadpan_core::*;
use proptest::prelude::*;
use serde_json::json;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn mark_id(value: &str) -> MarkId {
    MarkId::new(value).unwrap()
}
fn duration(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn recipe(frames: i64) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold("Pause", recipe(frames))
}
fn empty() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        RevisionId::new("r-0").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap()
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    let revision: u32 = document
        .revision_id()
        .as_str()
        .strip_prefix("r-")
        .unwrap()
        .parse()
        .unwrap();
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(format!("r-{}", revision + 1)).unwrap(),
        command,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let transaction = apply(document, &request(document, command)).unwrap();
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&serde_json::to_string(&transaction).unwrap())
            .unwrap(),
        transaction
    );
    (result, transaction)
}
fn tree(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(id, beat)| (node(id), beat))
        .collect();
    nodes.insert(
        node("group"),
        BeatNode::sequence("Group", children.iter().map(|id| node(id)).collect()),
    );
    edit(
        &empty(),
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("group"),
                nodes,
            },
        },
    )
    .0
}
fn local(host: &str, n: i64) -> Anchor {
    Anchor::Local {
        node: node(host),
        position: ExactRatio::integer(n),
    }
}
fn put(
    document: &ProjectDocument,
    id: &str,
    owner: &str,
    coordinate: Anchor,
    bias: InsertionBias,
    loss_policy: AnchorLossPolicy,
) -> ProjectDocument {
    edit(
        document,
        Command::SetMark {
            id: mark_id(id),
            owner: node(owner),
            label: id.into(),
            boundary: BoundaryAnchor { coordinate, bias },
            loss_policy,
        },
    )
    .0
}
fn keep(
    document: &ProjectDocument,
    id: &str,
    coordinate: Anchor,
    bias: InsertionBias,
) -> ProjectDocument {
    put(
        document,
        id,
        "root",
        coordinate,
        bias,
        AnchorLossPolicy::KeepUnresolved,
    )
}
fn position(document: &ProjectDocument, id: &str) -> ExactRatio {
    let mark = &document.marks()[&mark_id(id)];
    assert_eq!(mark.state, MarkState::Bound);
    match mark.boundary.coordinate {
        Anchor::Local { position, .. } | Anchor::Occurrence { position, .. } => position,
        _ => panic!("local position expected"),
    }
}
fn reason(document: &ProjectDocument, id: &str) -> MarkLossReason {
    let MarkState::Unresolved { reason } = document.marks()[&mark_id(id)].state else {
        panic!("unresolved mark expected")
    };
    reason
}
fn wrap(
    document: &ProjectDocument,
    target: &str,
    id: &str,
    plays: u32,
    gap: Option<HoldRecipe>,
    policy: WrapAnchorPolicy,
) -> ProjectDocument {
    edit(
        document,
        Command::WrapRepeat {
            node: node(target),
            id: node(id),
            plays,
            gap,
            anchor_policy: policy,
        },
    )
    .0
}
fn iterations(document: &ProjectDocument, id: &str) -> IterationOrder {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(id)].kind else {
        panic!("repeat expected")
    };
    iterations.clone()
}
fn select(
    document: &ProjectDocument,
    selector: BoundarySelector,
) -> Result<ResolvedSelection, AnchorError> {
    AnchorIndex::new(document)
        .unwrap()
        .resolve(&SelectionRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            role: MediaRole::Linked,
            selector,
        })
}
fn named(id: &str) -> NamedMarkTarget {
    NamedMarkTarget {
        id: mark_id(id),
        occurrence: None,
    }
}

#[test]
fn biased_insertions_and_outside_edges_follow_original_content() {
    let mut document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    for (id, frame, bias) in [
        ("left", 3, InsertionBias::Left),
        ("right", 3, InsertionBias::Right),
        ("start-left", 0, InsertionBias::Left),
        ("start-right", 0, InsertionBias::Right),
        ("end-left", 7, InsertionBias::Left),
        ("end-right", 7, InsertionBias::Right),
    ] {
        document = keep(&document, id, local("group", frame), bias);
    }
    let insert = |id: &str, index| Command::Insert {
        parent: node("group"),
        index,
        subtree: Subtree {
            root: node(id),
            nodes: BTreeMap::from([(node(id), hold(2))]),
        },
    };
    let (middle, patch) = edit(&document, insert("middle", 1));
    assert_eq!(position(&middle, "left"), ExactRatio::integer(3));
    assert_eq!(position(&middle, "right"), ExactRatio::integer(5));
    assert!(patch.forward.marks.contains_key(&mark_id("right")));
    assert!(!patch.forward.marks.contains_key(&mark_id("left")));
    let leading = edit(&document, insert("first", 0)).0;
    assert_eq!(position(&leading, "start-left"), ExactRatio::ZERO);
    assert_eq!(position(&leading, "start-right"), ExactRatio::integer(2));
    let trailing = edit(&document, insert("last", 2)).0;
    assert_eq!(position(&trailing, "end-left"), ExactRatio::integer(7));
    assert_eq!(position(&trailing, "end-right"), ExactRatio::integer(9));
}

#[test]
fn empty_hosts_retain_biased_edges_and_zero_children_do_not_capture_time() {
    let mut document = tree(&[], vec![]);
    document = keep(&document, "left", local("group", 0), InsertionBias::Left);
    document = keep(&document, "right", local("group", 0), InsertionBias::Right);
    let after = edit(
        &document,
        Command::Insert {
            parent: node("group"),
            index: 0,
            subtree: Subtree {
                root: node("a"),
                nodes: BTreeMap::from([(node("a"), hold(5))]),
            },
        },
    )
    .0;
    assert_eq!(position(&after, "left"), ExactRatio::ZERO);
    assert_eq!(position(&after, "right"), ExactRatio::integer(5));
    let with_empty = edit(
        &after,
        Command::Insert {
            parent: node("group"),
            index: 0,
            subtree: Subtree {
                root: node("zero"),
                nodes: BTreeMap::from([(node("zero"), BeatNode::sequence("Empty", vec![]))]),
            },
        },
    )
    .0;
    let marked = keep(
        &with_empty,
        "content",
        local("group", 0),
        InsertionBias::Right,
    );
    let removed = edit(&marked, Command::Delete { node: node("zero") }).0;
    assert_eq!(position(&removed, "content"), ExactRatio::ZERO);
}

#[test]
fn parent_local_follows_moves_within_host_and_child_local_follows_global_moves() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let document = keep(&document, "parent", local("group", 1), InsertionBias::Right);
    let document = keep(&document, "child", local("a", 1), InsertionBias::Right);
    let moved = edit(
        &document,
        Command::Move {
            node: node("a"),
            parent: node("group"),
            index: 1,
        },
    )
    .0;
    assert_eq!(position(&moved, "parent"), ExactRatio::integer(5));
    assert_eq!(position(&moved, "child"), ExactRatio::ONE);
    let departed = edit(
        &moved,
        Command::Move {
            node: node("a"),
            parent: node("root"),
            index: 1,
        },
    )
    .0;
    assert_eq!(reason(&departed, "parent"), MarkLossReason::OutsideHost);
    assert_eq!(position(&departed, "child"), ExactRatio::ONE);
    assert_eq!(
        departed.marks()[&mark_id("parent")].boundary,
        moved.marks()[&mark_id("parent")].boundary
    );
}

#[test]
fn ownership_loss_is_independent_and_unresolved_requires_explicit_reattachment() {
    let mut document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    document = put(
        &document,
        "delete",
        "a",
        Anchor::Sequence {
            frame: ProjectFrame(1),
        },
        InsertionBias::Right,
        AnchorLossPolicy::DeleteOwned,
    );
    document = put(
        &document,
        "keep",
        "a",
        local("b", 1),
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    document = keep(
        &document,
        "content",
        local("group", 1),
        InsertionBias::Right,
    );
    let (deleted, transaction) = edit(&document, Command::Delete { node: node("a") });
    assert!(!deleted.marks().contains_key(&mark_id("delete")));
    assert_eq!(reason(&deleted, "keep"), MarkLossReason::OwnerMissing);
    assert_eq!(reason(&deleted, "content"), MarkLossReason::ContentMissing);
    assert_eq!(transaction.forward.marks.len(), 3);
    let reused = edit(
        &deleted,
        Command::Insert {
            parent: node("group"),
            index: 0,
            subtree: Subtree {
                root: node("a"),
                nodes: BTreeMap::from([(node("a"), hold(3))]),
            },
        },
    )
    .0;
    assert_eq!(reason(&reused, "keep"), MarkLossReason::OwnerMissing);
    assert_eq!(reason(&reused, "content"), MarkLossReason::ContentMissing);
    let rebound = keep(&reused, "keep", local("a", 2), InsertionBias::Left);
    assert_eq!(position(&rebound, "keep"), ExactRatio::integer(2));
    let removed = edit(
        &rebound,
        Command::DeleteMark {
            id: mark_id("keep"),
        },
    )
    .0;
    assert!(!removed.marks().contains_key(&mark_id("keep")));
}

#[test]
fn repeat_gap_belongs_to_preceding_stable_play_and_disappears_when_last() {
    let document = wrap(
        &tree(&["a"], vec![("a", hold(2))]),
        "a",
        "repeat",
        3,
        Some(recipe(1)),
        WrapAnchorPolicy::First,
    );
    let mut document = keep(&document, "gap", local("repeat", 2), InsertionBias::Right);
    document = keep(
        &document,
        "play-end",
        local("repeat", 2),
        InsertionBias::Left,
    );
    document = keep(
        &document,
        "gap-end",
        local("repeat", 3),
        InsertionBias::Left,
    );
    document = keep(
        &document,
        "next-play",
        local("repeat", 3),
        InsertionBias::Right,
    );
    let moved = edit(
        &document,
        Command::MovePlays {
            node: node("repeat"),
            start: 0,
            end: 1,
            destination: 1,
        },
    )
    .0;
    assert_eq!(position(&moved, "gap"), ExactRatio::integer(5));
    assert_eq!(position(&moved, "gap-end"), ExactRatio::integer(6));
    assert_eq!(position(&moved, "play-end"), ExactRatio::integer(5));
    assert_eq!(position(&moved, "next-play"), ExactRatio::ZERO);
    let last = edit(
        &document,
        Command::MovePlays {
            node: node("repeat"),
            start: 0,
            end: 1,
            destination: 2,
        },
    )
    .0;
    assert_eq!(reason(&last, "gap"), MarkLossReason::GapMissing);
    assert_eq!(reason(&last, "gap-end"), MarkLossReason::GapMissing);
    assert_eq!(position(&last, "play-end"), ExactRatio::integer(8));
    let no_gap = edit(
        &document,
        Command::SetRepeat {
            node: node("repeat"),
            plays: 3,
            gap: None,
        },
    )
    .0;
    assert_eq!(reason(&no_gap, "gap"), MarkLossReason::GapMissing);
    assert_eq!(position(&no_gap, "next-play"), ExactRatio::integer(2));
}

#[test]
fn occurrence_marks_preserve_ids_without_cloning_on_growth_or_reviving_after_shrink() {
    let document = wrap(
        &tree(&["a"], vec![("a", hold(2))]),
        "a",
        "repeat",
        3,
        None,
        WrapAnchorPolicy::First,
    );
    let identity = iterations(&document, "repeat").at(2).unwrap();
    let occurrence = Anchor::Occurrence {
        instance: InstancePath {
            node: node("a"),
            repeats: vec![RepeatInstance {
                node: node("repeat"),
                iteration: identity.clone(),
            }],
        },
        position: ExactRatio::ONE,
    };
    let document = keep(&document, "specific", occurrence, InsertionBias::Right);
    let document = keep(&document, "all", local("a", 1), InsertionBias::Right);
    let grown = edit(
        &document,
        Command::InsertPlays {
            node: node("repeat"),
            index: 0,
            count: 5,
        },
    )
    .0;
    assert_eq!(grown.marks().len(), 2);
    assert_eq!(grown.marks(), document.marks());
    let shrunk = edit(
        &document,
        Command::SetRepeat {
            node: node("repeat"),
            plays: 2,
            gap: None,
        },
    )
    .0;
    assert_eq!(
        reason(&shrunk, "specific"),
        MarkLossReason::OccurrenceMissing
    );
    assert_eq!(position(&shrunk, "all"), ExactRatio::ONE);
    let regrown = edit(
        &shrunk,
        Command::SetRepeat {
            node: node("repeat"),
            plays: 3,
            gap: None,
        },
    )
    .0;
    assert_eq!(
        reason(&regrown, "specific"),
        MarkLossReason::OccurrenceMissing
    );
    assert_ne!(iterations(&regrown, "repeat").at(2).unwrap(), identity);
}

#[test]
fn wrap_policy_inserts_occurrence_step_in_ancestry_and_keeps_authored_local_scope() {
    let original = wrap(
        &tree(&["a"], vec![("a", hold(3))]),
        "a",
        "inner",
        2,
        None,
        WrapAnchorPolicy::First,
    );
    let path = InstancePath {
        node: node("a"),
        repeats: vec![RepeatInstance {
            node: node("inner"),
            iteration: iterations(&original, "inner").at(1).unwrap(),
        }],
    };
    let original = keep(
        &original,
        "specific",
        Anchor::Occurrence {
            instance: path.clone(),
            position: ExactRatio::ONE,
        },
        InsertionBias::Right,
    );
    let original = keep(&original, "all", local("a", 1), InsertionBias::Right);
    let original = keep(&original, "parent", local("group", 4), InsertionBias::Right);
    let first = wrap(
        &original,
        "inner",
        "outer",
        2,
        None,
        WrapAnchorPolicy::First,
    );
    let Anchor::Occurrence { instance, .. } =
        &first.marks()[&mark_id("specific")].boundary.coordinate
    else {
        panic!()
    };
    assert_eq!(instance.repeats.len(), 2);
    assert_eq!(instance.repeats[0].node, node("outer"));
    assert_eq!(
        instance.repeats[0].iteration,
        iterations(&first, "outer").at(0).unwrap()
    );
    assert_eq!(instance.repeats[1], path.repeats[0]);
    assert_eq!(position(&first, "parent"), ExactRatio::integer(4));
    let unresolved = wrap(
        &original,
        "inner",
        "outer",
        2,
        None,
        WrapAnchorPolicy::Unresolved,
    );
    assert_eq!(
        reason(&unresolved, "specific"),
        MarkLossReason::WrapAmbiguous
    );
    assert_eq!(reason(&unresolved, "parent"), MarkLossReason::WrapAmbiguous);
    assert_eq!(position(&unresolved, "all"), ExactRatio::ONE);
    let command: Command = serde_json::from_value(
        json!({"command":"wrap_repeat","node":"a","id":"new","plays":2,"gap":null}),
    )
    .unwrap();
    assert!(matches!(
        command,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            ..
        }
    ));
}

#[test]
fn grouping_preserves_content_but_removed_host_has_explicit_loss() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let document = keep(&document, "outer", local("group", 4), InsertionBias::Right);
    let grouped = edit(
        &document,
        Command::Group {
            parent: node("group"),
            start: 0,
            end: 2,
            id: node("nested"),
            label: "Nested".into(),
        },
    )
    .0;
    assert_eq!(position(&grouped, "outer"), ExactRatio::integer(4));
    let grouped = keep(&grouped, "host", local("nested", 4), InsertionBias::Right);
    let ungrouped = edit(
        &grouped,
        Command::Ungroup {
            node: node("nested"),
        },
    )
    .0;
    assert_eq!(position(&ungrouped, "outer"), ExactRatio::integer(4));
    assert_eq!(reason(&ungrouped, "host"), MarkLossReason::HostMissing);
}

#[test]
fn nested_retimes_preserve_exact_fractions_and_report_crop_loss() {
    let retime = |child: &str, frames, start, end| BeatNode {
        label: "Retime".into(),
        kind: NodeKind::Retime {
            child: node(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
    };
    let document = tree(
        &["outer"],
        vec![
            ("outer", retime("inner", 17, 1, 8)),
            ("inner", retime("sequence", 11, 0, 7)),
            (
                "sequence",
                BeatNode::sequence("Sequence", vec![node("a"), node("b")]),
            ),
            ("a", hold(3)),
            ("b", hold(4)),
        ],
    );
    let document = keep(
        &document,
        "b",
        Anchor::Local {
            node: node("outer"),
            position: ratio(629, 49),
        },
        InsertionBias::Right,
    );
    let document = keep(
        &document,
        "a",
        Anchor::Local {
            node: node("outer"),
            position: ratio(323, 98),
        },
        InsertionBias::Right,
    );
    let moved = edit(
        &document,
        Command::Move {
            node: node("b"),
            parent: node("sequence"),
            index: 0,
        },
    )
    .0;
    assert_eq!(position(&moved, "b"), ratio(68, 49));
    assert_eq!(reason(&moved, "a"), MarkLossReason::OutsideMapping);
}

#[test]
fn pinned_sequence_time_stays_fixed_and_trimmed_hold_points_do_not_retarget() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let document = keep(
        &document,
        "pinned",
        Anchor::Sequence {
            frame: ProjectFrame(5),
        },
        InsertionBias::Right,
    );
    let document = keep(&document, "tail", local("b", 3), InsertionBias::Left);
    let shortened = edit(
        &document,
        Command::SetHoldDuration {
            node: node("b"),
            duration: duration(2),
        },
    )
    .0;
    assert_eq!(
        shortened.marks()[&mark_id("pinned")].boundary,
        document.marks()[&mark_id("pinned")].boundary
    );
    assert_eq!(reason(&shortened, "tail"), MarkLossReason::OutOfRange);
    let too_short = edit(&shortened, Command::Delete { node: node("a") }).0;
    assert_eq!(reason(&too_short, "pinned"), MarkLossReason::OutOfRange);
}

#[test]
fn trimming_at_a_mark_preserves_only_the_surviving_content_side() {
    let document = tree(&["a"], vec![("a", hold(7))]);
    let document = keep(&document, "left", local("a", 3), InsertionBias::Left);
    let document = keep(&document, "right", local("a", 3), InsertionBias::Right);
    let document = keep(&document, "edge", local("a", 7), InsertionBias::Right);
    let trimmed = edit(
        &document,
        Command::SetHoldDuration {
            node: node("a"),
            duration: duration(3),
        },
    )
    .0;
    assert_eq!(position(&trimmed, "left"), ExactRatio::integer(3));
    assert_eq!(reason(&trimmed, "right"), MarkLossReason::OutOfRange);
    assert_eq!(position(&trimmed, "edge"), ExactRatio::integer(3));
}

#[test]
fn source_marks_use_original_clock_independent_of_owner_and_timeline_usage() {
    let original = SourceSpan::new(
        SourceTimestamp {
            ticks: -4800,
            time_base: SourceTimeBase::new(1, 48000).unwrap(),
        },
        SourceTimestamp {
            ticks: 48000,
            time_base: SourceTimeBase::new(1, 48000).unwrap(),
        },
    )
    .unwrap();
    let asset = AssetId::new("audio").unwrap();
    let document = edit(
        &tree(&["a"], vec![("a", hold(3))]),
        Command::AddAsset {
            id: asset.clone(),
            asset: AssetRecord {
                label: "Audio".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(original),
                still_image: false,
                frame_count: None,
            },
        },
    )
    .0;
    let coordinate = Anchor::Source {
        asset: asset.clone(),
        moment: SourceMoment::AudioSample {
            sample: -2400,
            sample_rate: 48000,
        },
    };
    let document = keep(
        &document,
        "source",
        coordinate.clone(),
        InsertionBias::Right,
    );
    let deleted = edit(&document, Command::Delete { node: node("a") }).0;
    assert_eq!(
        deleted.marks()[&mark_id("source")].boundary.coordinate,
        coordinate
    );
    assert_eq!(deleted.marks()[&mark_id("source")].state, MarkState::Bound);
    assert_eq!(
        select(
            &deleted,
            BoundarySelector::Mark {
                target: named("source")
            }
        )
        .unwrap_err()
        .code,
        AnchorErrorCode::OccurrenceRequired
    );
    let invalid = Command::SetMark {
        id: mark_id("bad"),
        owner: node("root"),
        label: "Bad".into(),
        boundary: BoundaryAnchor {
            coordinate: Anchor::Source {
                asset,
                moment: SourceMoment::AudioSample {
                    sample: -2400,
                    sample_rate: 24000,
                },
            },
            bias: InsertionBias::Right,
        },
        loss_policy: AnchorLossPolicy::KeepUnresolved,
    };
    // -0.1 seconds is the original leading boundary and is valid, while
    // -0.2 seconds at 12 kHz is outside the original stream.
    assert!(apply(&deleted, &request(&deleted, invalid.clone())).is_ok());
    let Command::SetMark {
        id,
        owner,
        label,
        mut boundary,
        loss_policy,
    } = invalid
    else {
        panic!()
    };
    let Anchor::Source { moment, .. } = &mut boundary.coordinate else {
        panic!()
    };
    *moment = SourceMoment::AudioSample {
        sample: -2400,
        sample_rate: 12000,
    };
    assert!(
        apply(
            &deleted,
            &request(
                &deleted,
                Command::SetMark {
                    id,
                    owner,
                    label,
                    boundary,
                    loss_policy
                }
            )
        )
        .is_err()
    );
}

#[test]
fn mark_queries_share_revision_and_exact_range_validation_and_refuse_unresolved() {
    let document = tree(&["a"], vec![("a", hold(10))]);
    let document = keep(
        &document,
        "start",
        Anchor::Local {
            node: node("a"),
            position: ratio(3, 2),
        },
        InsertionBias::Right,
    );
    let document = keep(&document, "end", local("a", 7), InsertionBias::Left);
    let range = select(
        &document,
        BoundarySelector::MarkRange {
            start: named("start"),
            end: named("end"),
        },
    )
    .unwrap();
    let ResolvedSelectionKind::Range { start, end, frames } = range.selection else {
        panic!()
    };
    assert_eq!(start.exact_frame, ratio(3, 2));
    assert_eq!(
        frames,
        FrameRange::new(ProjectFrame(2), ProjectFrame(7)).unwrap()
    );
    assert_eq!(end.frame, ProjectFrame(7));
    assert_eq!(
        select(
            &document,
            BoundarySelector::MarkRange {
                start: named("end"),
                end: named("start")
            }
        )
        .unwrap_err()
        .code,
        AnchorErrorCode::InvalidRange
    );
    assert_eq!(
        select(
            &document,
            BoundarySelector::Mark {
                target: named("missing")
            }
        )
        .unwrap_err()
        .code,
        AnchorErrorCode::MarkMissing
    );
    let deleted = edit(&document, Command::Delete { node: node("a") }).0;
    assert_eq!(
        select(
            &deleted,
            BoundarySelector::Mark {
                target: named("start")
            }
        )
        .unwrap_err()
        .code,
        AnchorErrorCode::MarkUnresolved
    );
    let stale = SelectionRequest {
        project_id: deleted.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        role: MediaRole::Linked,
        selector: BoundarySelector::Mark {
            target: named("missing"),
        },
    };
    assert_eq!(
        AnchorIndex::new(&deleted)
            .unwrap()
            .resolve(&stale)
            .unwrap_err()
            .code,
        AnchorErrorCode::RevisionConflict
    );
}

#[test]
fn named_source_and_authored_local_marks_require_explicit_repeat_scope() {
    let clock = SourceTimeBase::new(1, 30).unwrap();
    let span = |start, end| {
        SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: end,
                time_base: clock,
            },
        )
        .unwrap()
    };
    let asset = AssetId::new("video").unwrap();
    let document = edit(
        &empty(),
        Command::AddAsset {
            id: asset.clone(),
            asset: AssetRecord {
                label: "Video".into(),
                content_hash: "b".repeat(64),
                video: Some(span(0, 90)),
                audio: None,
                still_image: false,
                frame_count: None,
            },
        },
    )
    .0;
    let document = edit(
        &document,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("source"),
                nodes: BTreeMap::from([(
                    node("source"),
                    BeatNode {
                        label: "Source".into(),
                        kind: NodeKind::Source {
                            source: SourceNode {
                                duration: duration(10),
                                video: SourceVideo::Stream {
                                    asset: asset.clone(),
                                    span: span(30, 60),
                                },
                                audio: None,
                                link: LinkRelation::Independent,
                                audio_offset: AudioSample(0),
                            },
                        },
                    },
                )]),
            },
        },
    )
    .0;
    let document = wrap(
        &document,
        "source",
        "repeat",
        2,
        None,
        WrapAnchorPolicy::First,
    );
    let document = keep(
        &document,
        "original",
        Anchor::Source {
            asset,
            moment: SourceMoment::Timestamp {
                stream: SourceStream::Video,
                timestamp: SourceTimestamp {
                    ticks: 45,
                    time_base: clock,
                },
            },
        },
        InsertionBias::Right,
    );
    let document = keep(
        &document,
        "authored",
        local("source", 5),
        InsertionBias::Right,
    );
    let path = InstancePath {
        node: node("source"),
        repeats: vec![RepeatInstance {
            node: node("repeat"),
            iteration: iterations(&document, "repeat").at(1).unwrap(),
        }],
    };
    for id in ["original", "authored"] {
        assert_eq!(
            select(&document, BoundarySelector::Mark { target: named(id) })
                .unwrap_err()
                .code,
            AnchorErrorCode::OccurrenceRequired
        );
        let result = select(
            &document,
            BoundarySelector::Mark {
                target: NamedMarkTarget {
                    id: mark_id(id),
                    occurrence: Some(path.clone()),
                },
            },
        )
        .unwrap();
        let ResolvedSelectionKind::Point { point } = result.selection else {
            panic!()
        };
        assert_eq!(point.exact_frame, ExactRatio::integer(15));
    }
}

#[test]
fn strict_mark_ingress_limits_and_patch_preconditions_are_atomic() {
    let document = tree(&["a"], vec![("a", hold(2))]);
    let mut wire = serde_json::to_value(&document).unwrap();
    wire.as_object_mut().unwrap().remove("marks");
    assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
    let document = keep(&document, "a", local("a", 1), InsertionBias::Right);
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["marks"]["a"]["owner"] = json!("missing");
    assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["marks"]["a"]["label"] = json!("x".repeat(1025));
    assert_eq!(
        ProjectDocument::from_json(&wire.to_string())
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    let transaction = apply(
        &document,
        &request(&document, Command::DeleteMark { id: mark_id("a") }),
    )
    .unwrap();
    let mut forged = transaction.forward.clone();
    forged
        .marks
        .get_mut(&mark_id("a"))
        .unwrap()
        .before
        .as_mut()
        .unwrap()
        .label = "wrong".into();
    assert_eq!(
        forged.apply(&document).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
    let payload = serde_json::to_string(&document.marks()[&mark_id("a")]).unwrap();
    let duplicated = document
        .to_json()
        .unwrap()
        .replace("\"marks\": {", &format!("\"marks\": {{\"a\": {payload},"));
    assert!(
        ProjectDocument::from_json(&duplicated)
            .unwrap_err()
            .message
            .contains("duplicate identity key")
    );
    let mut wire = serde_json::to_value(&transaction).unwrap();
    wire["forward"].as_object_mut().unwrap().remove("marks");
    assert!(serde_json::from_value::<EditTransaction>(wire).is_err());
    assert_eq!(document.marks().len(), 1);
}

#[test]
fn unrepresentable_exact_mark_transform_rejects_the_whole_edit() {
    let document = tree(&["a", "b"], vec![("a", hold(1)), ("b", hold(1))]);
    let document = keep(
        &document,
        "wide",
        Anchor::Local {
            node: node("group"),
            position: ratio(1, i128::MAX),
        },
        InsertionBias::Right,
    );
    let snapshot = document.to_json().unwrap();
    let command = Command::Move {
        node: node("a"),
        parent: node("group"),
        index: 1,
    };
    assert_eq!(
        apply(&document, &request(&document, command))
            .unwrap_err()
            .code,
        EditErrorCode::TimingOverflow
    );
    assert_eq!(document.to_json().unwrap(), snapshot);
    assert_eq!(document.marks()[&mark_id("wide")].state, MarkState::Bound);
}

#[test]
fn billions_of_plays_keep_marks_compact_and_seek_the_last_stable_iteration() {
    let document = wrap(
        &tree(&["a"], vec![("a", hold(1))]),
        "a",
        "repeat",
        u32::MAX,
        Some(recipe(1)),
        WrapAnchorPolicy::First,
    );
    let last = i64::from(u32::MAX - 1) * 2;
    let document = keep(
        &document,
        "last",
        local("repeat", last),
        InsertionBias::Right,
    );
    let moved = edit(
        &document,
        Command::MovePlays {
            node: node("repeat"),
            start: u32::MAX - 1,
            end: u32::MAX,
            destination: 0,
        },
    )
    .0;
    assert_eq!(position(&moved, "last"), ExactRatio::ZERO);
    assert_eq!(moved.marks().len(), 1);
    assert_eq!(iterations(&moved, "repeat").segment_count(), 2);
    assert!(moved.to_json().unwrap().len() < 4000);
}

proptest! {
    #[test]
    fn repeat_reorders_preserve_marked_play_offset_without_expansion(plays in 2u32..100, selected in 0u32..100, destination in 0u32..100, offset in 0i64..7) {
        let selected=selected%plays;
        let destination=destination%plays;
        let document=wrap(&tree(&["a"],vec![("a",hold(7))]),"a","repeat",plays,Some(recipe(2)),WrapAnchorPolicy::First);
        let document=keep(&document,"point",local("repeat",i64::from(selected)*9+offset),InsertionBias::Right);
        let moved=edit(&document,Command::MovePlays {node:node("repeat"),start:selected,end:selected+1,destination}).0;
        prop_assert_eq!(position(&moved,"point"),ExactRatio::integer(i64::from(destination)*9+offset));
        prop_assert_eq!(moved.marks().len(),1);
    }
}
