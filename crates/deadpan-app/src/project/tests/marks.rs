//! Native marks save authored boundaries without changing editorial state.

use super::*;
use crate::project::marks::{self, Id, Location, Operation, Outcome, Request, ResolvedLocation};
use deadpan_core::{
    Anchor, ExactRatio, InsertionBias, Mark, MarkFragment, MarkState, SourceMoment, SourceStream,
};

fn request(workspace: &Workspace, ticket: u64, operation: Operation) -> Request {
    Request {
        id: Id {
            ticket,
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            revision: workspace.document.revision_id().clone(),
        },
        operation,
    }
}

fn set(workspace: &Workspace, ticket: u64, letter: char, at: i64) -> Request {
    request(
        workspace,
        ticket,
        Operation::Set {
            letter,
            location: Location::Edit {
                scope: SequenceScope::default(),
                at: ProjectFrame(at),
                selected: None,
            },
        },
    )
}

fn send(service: &ProjectService, request: Request) -> ProjectUpdate {
    let id = request.id.clone();
    let update = command(service, ProjectRequest::Marks(request));
    assert_eq!(update.marks.reply.as_ref().unwrap().id, id);
    update
}

fn saved(update: &ProjectUpdate) -> &marks::Saved {
    let Outcome::Saved(saved) = update
        .marks
        .reply
        .as_ref()
        .unwrap()
        .result
        .as_ref()
        .unwrap()
    else {
        panic!("expected mark save")
    };
    assert!(update.committed.is_none());
    assert_eq!(update.marks.saved.as_ref(), Some(saved));
    saved
}

fn jumped(
    service: &ProjectService,
    workspace: &Workspace,
    ticket: u64,
    letter: char,
) -> ResolvedLocation {
    let update = send(
        service,
        request(workspace, ticket, Operation::Jump { letter }),
    );
    let Outcome::Jumped(location) = update.marks.reply.unwrap().result.unwrap() else {
        panic!("expected mark jump")
    };
    location
}

fn counts(path: &Path) -> (i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn setup(path: &Path) -> (Harness, Arc<Workspace>) {
    drop(seed_holds(path, &["a", "b", "c"]));
    let harness = Harness::new();
    let workspace = command(&harness.service, ProjectRequest::Open(path.into()))
        .workspace
        .unwrap();
    (harness, workspace)
}

fn unchanged_edit(before: &Workspace, after: &Workspace) {
    assert_eq!(before.document.nodes(), after.document.nodes());
    assert_eq!(before.document.assets(), after.document.assets());
    assert_eq!(before.document.sounds(), after.document.sounds());
    assert_eq!(before.plan.duration(), after.plan.duration());
    for frame in 0..before.plan.duration().frames() {
        assert_eq!(
            before.plan.picture(ProjectFrame(frame)).unwrap().picture,
            after.plan.picture(ProjectFrame(frame)).unwrap().picture
        );
    }
}

#[test]
fn native_letter_ids_are_exact_case_sensitive_and_do_not_depend_on_labels() {
    let ids: std::collections::BTreeSet<_> = ('a'..='z')
        .chain('A'..='Z')
        .map(|letter| marks::mark_id(letter).unwrap())
        .collect();
    assert_eq!(ids.len(), 52);
    assert_eq!(marks::mark_id('a').unwrap().as_str(), "native-mark-a");
    for invalid in ['0', 'é', ' ', '\'', '\n'] {
        assert!(marks::mark_id(invalid).is_err());
    }
}

#[test]
fn saves_are_atomic_deduplicated_persistent_and_do_not_change_picture_or_time() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("marks.deadpan");
    let (harness, before) = setup(&path);
    let initial_counts = counts(&path);
    let capture = set(&before, 1, 'a', 13);
    let result = send(&harness.service, capture.clone());
    let receipt = saved(&result).clone();
    let after = result.workspace.unwrap();
    unchanged_edit(&before, &after);
    assert_eq!(counts(&path), (initial_counts.0 + 1, initial_counts.1 + 1));
    let mark = &after.document.marks()[&marks::mark_id('a').unwrap()];
    assert_eq!(mark.owner, node("b"));
    assert_eq!(
        mark.loss_policy,
        deadpan_core::AnchorLossPolicy::KeepUnresolved
    );
    assert!(
        matches!(&mark.boundary.coordinate, Anchor::Occurrence { instance, position } if instance.node == node("b") && instance.repeats.is_empty() && *position == ExactRatio::integer(3))
    );
    assert_eq!(mark.boundary.bias, InsertionBias::Right);
    let duplicate = send(&harness.service, capture.clone());
    assert_eq!(saved(&duplicate), &receipt);
    let mut collision = capture.clone();
    collision.operation = Operation::Delete { letter: 'a' };
    assert!(
        send(&harness.service, collision)
            .marks
            .reply
            .unwrap()
            .result
            .unwrap_err()
            .contains("different operation")
    );
    assert_eq!(counts(&path), (initial_counts.0 + 1, initial_counts.1 + 1));
    assert_eq!(
        jumped(&harness.service, &after, 2, 'a'),
        ResolvedLocation::Edit {
            scope: SequenceScope::default(),
            selected: Some(node("b")),
            frame: ProjectFrame(13),
            exact_frame: ExactRatio::integer(13)
        }
    );
    assert_eq!(counts(&path), (initial_counts.0 + 1, initial_counts.1 + 1));

    assert_eq!(saved(&send(&harness.service, capture)), &receipt);

    // Uppercase and lowercase keyboard marks remain independently addressable.
    let uppercase = send(&harness.service, set(&after, 3, 'A', 2));
    let uppercase = uppercase.workspace.unwrap();
    assert_eq!(uppercase.document.marks().len(), 2);
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    assert_eq!(reopened.document.marks(), uppercase.document.marks());
    assert!(matches!(
        jumped(&harness.service, &reopened, 1, 'A'),
        ResolvedLocation::Edit {
            frame: ProjectFrame(2),
            ..
        }
    ));
    let deleted = send(
        &harness.service,
        request(&reopened, 2, Operation::Delete { letter: 'a' }),
    );
    saved(&deleted);
    let deleted = deleted.workspace.unwrap();
    assert!(
        !deleted
            .document
            .marks()
            .contains_key(&marks::mark_id('a').unwrap())
    );
    unchanged_edit(&reopened, &deleted);
    let undo = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: deleted.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undo.document.marks(), reopened.document.marks());
    let redo = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: undo.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redo.document.marks(), deleted.document.marks());
    assert_ne!(undo.document.revision_id(), reopened.document.revision_id());
}

#[test]
fn every_mark_intent_clears_an_unconsumed_edit_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("mark-receipt.deadpan");
    let (harness, before) = setup(&path);
    let marked = send(&harness.service, set(&before, 1, 'a', 4))
        .workspace
        .unwrap();

    let first_edit = edited(
        &harness.service,
        &marked,
        ProjectEdit::HoldDuration {
            node: node("a"),
            duration: FrameDuration::new(11).unwrap(),
        },
    );
    assert!(first_edit.committed.is_some());
    let first_edit = first_edit.workspace.unwrap();
    let jump = send(
        &harness.service,
        request(&first_edit, 2, Operation::Jump { letter: 'a' }),
    );
    assert!(jump.committed.is_none());
    assert!(matches!(
        jump.marks.reply.unwrap().result.unwrap(),
        Outcome::Jumped(ResolvedLocation::Edit {
            frame: ProjectFrame(4),
            ..
        })
    ));

    let second_edit = edited(
        &harness.service,
        &first_edit,
        ProjectEdit::HoldDuration {
            node: node("a"),
            duration: FrameDuration::new(12).unwrap(),
        },
    );
    assert!(second_edit.committed.is_some());
    let second_edit = second_edit.workspace.unwrap();
    let rejected = send(
        &harness.service,
        request(
            &second_edit,
            3,
            Operation::Set {
                letter: 'b',
                location: Location::Edit {
                    scope: SequenceScope::default(),
                    at: ProjectFrame(-1),
                    selected: None,
                },
            },
        ),
    );
    assert!(rejected.committed.is_none());
    assert!(rejected.marks.reply.unwrap().result.is_err());
}

#[test]
fn native_key_addresses_reject_other_named_marks_without_overwriting_or_deleting() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("mark-name-collision.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    let id = marks::mark_id('a').unwrap();
    seed_command(
        &mut store,
        Command::SetMark {
            id: id.clone(),
            owner: node("b"),
            label: "Named bookmark".into(),
            boundary: deadpan_core::BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: deadpan_core::InstancePath {
                        node: node("b"),
                        repeats: Vec::new(),
                    },
                    position: ExactRatio::integer(4),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: deadpan_core::AnchorLossPolicy::KeepUnresolved,
        },
        "named-mark-collision",
    );
    drop(store);

    let harness = Harness::new();
    let workspace = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let original = workspace.document.marks()[&id].clone();
    let initial_counts = counts(&path);
    let operations = [
        Operation::Set {
            letter: 'a',
            location: Location::Edit {
                scope: SequenceScope::default(),
                at: ProjectFrame(3),
                selected: Some(node("a")),
            },
        },
        Operation::Jump { letter: 'a' },
        Operation::Delete { letter: 'a' },
    ];
    for (index, operation) in operations.into_iter().enumerate() {
        let result = send(
            &harness.service,
            request(&workspace, index as u64 + 1, operation),
        );
        assert!(
            result
                .marks
                .reply
                .unwrap()
                .result
                .unwrap_err()
                .contains("conflicts with a named mark")
        );
        assert_eq!(result.workspace.unwrap().document.marks()[&id], original);
        assert_eq!(counts(&path), initial_counts);
    }
}

#[test]
fn cli_mark_with_the_exact_native_address_and_label_remains_accessible() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("mark-native-parity.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    let id = marks::mark_id('b').unwrap();
    seed_command(
        &mut store,
        Command::SetMark {
            id: id.clone(),
            owner: node("b"),
            label: "b".into(),
            boundary: deadpan_core::BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: deadpan_core::InstancePath {
                        node: node("b"),
                        repeats: Vec::new(),
                    },
                    position: ExactRatio::integer(4),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: deadpan_core::AnchorLossPolicy::KeepUnresolved,
        },
        "cli-native-mark",
    );
    drop(store);

    let harness = Harness::new();
    let workspace = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(
        jumped(&harness.service, &workspace, 1, 'b'),
        ResolvedLocation::Edit {
            scope: SequenceScope::default(),
            selected: Some(node("b")),
            frame: ProjectFrame(14),
            exact_frame: ExactRatio::integer(14),
        }
    );
    let set_result = send(
        &harness.service,
        request(
            &workspace,
            2,
            Operation::Set {
                letter: 'b',
                location: Location::Edit {
                    scope: SequenceScope::default(),
                    at: ProjectFrame(16),
                    selected: Some(node("b")),
                },
            },
        ),
    );
    saved(&set_result);
    let updated = set_result.workspace.unwrap();
    assert_eq!(updated.document.marks()[&id].label, "b");
    let delete_result = send(
        &harness.service,
        request(&updated, 3, Operation::Delete { letter: 'b' }),
    );
    saved(&delete_result);
    assert!(
        !delete_result
            .workspace
            .unwrap()
            .document
            .marks()
            .contains_key(&id)
    );
}

#[test]
fn jump_rejects_equal_time_mark_bindings_with_distinct_accessible_targets() {
    let scratch = tempfile::tempdir().unwrap();
    let seed_path = scratch.path().join("mark-binding-seed.deadpan");
    let seed = seed_holds(&seed_path, &["a", "b"]);
    let document = seed.snapshot().unwrap();
    let id = marks::mark_id('a').unwrap();
    let mark = Mark {
        owner: node("a"),
        label: "a".into(),
        boundary: deadpan_core::BoundaryAnchor {
            coordinate: Anchor::Occurrence {
                instance: deadpan_core::InstancePath {
                    node: node("a"),
                    repeats: Vec::new(),
                },
                position: ExactRatio::integer(10),
            },
            bias: InsertionBias::Left,
        },
        loss_policy: deadpan_core::AnchorLossPolicy::KeepUnresolved,
        state: MarkState::Bound,
        fragments: vec![MarkFragment {
            owner: node("b"),
            coordinate: Anchor::Occurrence {
                instance: deadpan_core::InstancePath {
                    node: node("b"),
                    repeats: Vec::new(),
                },
                position: ExactRatio::ZERO,
            },
            state: MarkState::Bound,
        }],
    };
    let mut wire: serde_json::Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
    wire["marks"][id.as_str()] = serde_json::to_value(mark).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    drop(seed);
    let path = scratch.path().join("mark-binding-context.deadpan");
    drop(ProjectStore::create(&path, &document).unwrap());

    let harness = Harness::new();
    let workspace = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let initial_counts = counts(&path);
    let result = send(
        &harness.service,
        request(&workspace, 1, Operation::Jump { letter: 'a' }),
    );
    assert!(
        result
            .marks
            .reply
            .unwrap()
            .result
            .unwrap_err()
            .contains("distinct accessible edit targets")
    );
    assert_eq!(counts(&path), initial_counts);
    assert_eq!(*result.workspace.unwrap().document, *workspace.document);
}

#[test]
fn nested_scopes_preserve_explicit_empty_hosts_and_choose_the_right_boundary_content() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("empty-marks.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    for (index, name) in [(1, "empty-left"), (2, "empty"), (3, "empty-right")] {
        seed_command(
            &mut store,
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(node(name), BeatNode::sequence(name, vec![]))]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            &format!("seed-{name}"),
        );
    }
    seed_command(
        &mut store,
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 5,
            id: node("group"),
            label: "Nested".into(),
        },
        "seed-group",
    );
    drop(store);
    let harness = Harness::new();
    let mut workspace = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&workspace, &node("group"))
        .unwrap();
    for (ticket, letter, selected, expected, at, bias) in [
        (
            1,
            'a',
            Some(node("empty")),
            node("empty"),
            10,
            InsertionBias::Left,
        ),
        (3, 'b', None, node("b"), 10, InsertionBias::Right),
        (5, 'c', None, node("b"), 20, InsertionBias::Left),
    ] {
        let result = send(
            &harness.service,
            request(
                &workspace,
                ticket,
                Operation::Set {
                    letter,
                    location: Location::Edit {
                        scope: scope.clone(),
                        at: ProjectFrame(at),
                        selected,
                    },
                },
            ),
        );
        saved(&result);
        workspace = result.workspace.unwrap();
        let mark = &workspace.document.marks()[&marks::mark_id(letter).unwrap()];
        assert_eq!(mark.owner, expected);
        assert_eq!(mark.boundary.bias, bias);
        assert_eq!(
            jumped(&harness.service, &workspace, ticket + 1, letter),
            ResolvedLocation::Edit {
                scope: scope.clone(),
                selected: Some(expected),
                frame: ProjectFrame(at),
                exact_frame: ExactRatio::integer(at)
            }
        );
    }
    let empty_scope = scope.descend(&workspace, &node("empty-right")).unwrap();
    let result = send(
        &harness.service,
        request(
            &workspace,
            7,
            Operation::Set {
                letter: 'z',
                location: Location::Edit {
                    scope: empty_scope,
                    at: ProjectFrame(10),
                    selected: None,
                },
            },
        ),
    );
    workspace = result.workspace.unwrap();
    assert_eq!(
        workspace.document.marks()[&marks::mark_id('z').unwrap()].owner,
        node("empty-right")
    );
    assert!(
        matches!(jumped(&harness.service, &workspace, 8, 'z'), ResolvedLocation::Edit { selected: Some(selected), .. } if selected == node("empty-right"))
    );
    let before = counts(&workspace.path);
    for (ticket, at, selected) in [(9, 21, None), (10, 10, Some(node("root")))] {
        let result = send(
            &harness.service,
            request(
                &workspace,
                ticket,
                Operation::Set {
                    letter: 'q',
                    location: Location::Edit {
                        scope: scope.clone(),
                        at: ProjectFrame(at),
                        selected,
                    },
                },
            ),
        );
        assert!(result.marks.reply.unwrap().result.is_err());
    }
    assert_eq!(counts(&workspace.path), before);
}

#[test]
fn marks_follow_split_move_and_first_repeat_then_remain_unresolved_after_deletion() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("structural-marks.deadpan");
    let (harness, before) = setup(&path);
    let saved_mark = send(&harness.service, set(&before, 1, 'm', 6))
        .workspace
        .unwrap();
    let split = edited(
        &harness.service,
        &saved_mark,
        ProjectEdit::Split {
            node: node("a"),
            at: FrameDuration::new(4).unwrap(),
        },
    );
    let right = split.committed.unwrap().selected_node.unwrap();
    let split = split.workspace.unwrap();
    assert!(
        matches!(jumped(&harness.service, &split, 2, 'm'), ResolvedLocation::Edit { selected: Some(selected), frame: ProjectFrame(6), .. } if selected == right)
    );
    let wrapped = edited(
        &harness.service,
        &split,
        ProjectEdit::WrapRepeat {
            node: right,
            plays: 3,
        },
    );
    let repeat = wrapped.committed.unwrap().selected_node.unwrap();
    let wrapped = wrapped.workspace.unwrap();
    assert!(
        matches!(&wrapped.document.marks()[&marks::mark_id('m').unwrap()].boundary.coordinate, Anchor::Occurrence { instance, .. } if instance.repeats.len() == 1)
    );
    assert!(
        matches!(jumped(&harness.service, &wrapped, 3, 'm'), ResolvedLocation::Edit { selected: Some(selected), frame: ProjectFrame(6), .. } if selected == repeat)
    );
    command(&harness.service, ProjectRequest::Close);
    {
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
        seed_command(
            &mut store,
            Command::Move {
                node: repeat.clone(),
                parent: node("root"),
                index: 3,
            },
            "move-repeat",
        );
    }
    let moved = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert!(
        matches!(jumped(&harness.service, &moved, 1, 'm'), ResolvedLocation::Edit { selected: Some(selected), frame: ProjectFrame(26), .. } if selected == repeat)
    );
    let deleted = edited(
        &harness.service,
        &moved,
        ProjectEdit::Delete { node: repeat },
    )
    .workspace
    .unwrap();
    let mark = &deleted.document.marks()[&marks::mark_id('m').unwrap()];
    assert!(matches!(mark.state, MarkState::Unresolved { .. }));
    let rejected = send(
        &harness.service,
        request(&deleted, 2, Operation::Jump { letter: 'm' }),
    );
    assert!(
        rejected
            .marks
            .reply
            .unwrap()
            .result
            .unwrap_err()
            .contains("unresolved")
    );
    let undo = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: deleted.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert!(matches!(
        jumped(&harness.service, &undo, 3, 'm'),
        ResolvedLocation::Edit {
            frame: ProjectFrame(26),
            ..
        }
    ));
}

#[test]
fn saved_refresh_failure_survives_queries_failures_and_fresh_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = setup(&scratch.path().join("refresh-marks.deadpan"));
    harness
        .service
        .shared
        .render_commit_refresh_failure
        .store(true, Ordering::Release);
    let capture = set(&before, 1, 'a', 4);
    let result = send(&harness.service, capture.clone());
    let receipt = saved(&result).clone();
    assert!(receipt.refresh_error.as_ref().unwrap().contains("Reopen"));
    assert!(receipt.needs_refresh(result.workspace.as_deref()));
    let initial_counts = counts(&before.path);
    assert_eq!(saved(&send(&harness.service, capture)), &receipt);
    let failed = send(
        &harness.service,
        request(&before, 2, Operation::Jump { letter: 'a' }),
    );
    assert!(
        failed
            .marks
            .reply
            .unwrap()
            .result
            .unwrap_err()
            .contains("reopen")
    );
    assert_eq!(failed.marks.saved.as_ref(), Some(&receipt));
    let query = command(&harness.service, ProjectRequest::CancelImport);
    assert_eq!(query.marks.saved.as_ref(), Some(&receipt));
    assert_eq!(counts(&before.path), initial_counts);
    let undo = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: receipt.revision.clone(),
        },
    )
    .workspace
    .unwrap();
    assert!(undo.document.marks().is_empty());
    assert!(!receipt.needs_refresh(Some(&undo)));
    let stale = send(&harness.service, set(&before, 3, 'b', 8));
    assert!(
        stale
            .marks
            .reply
            .unwrap()
            .result
            .unwrap_err()
            .contains("changed")
    );
    let mut wrong = set(&undo, 4, 'b', 8);
    wrong.id.project = ProjectId::new("other-project").unwrap();
    assert!(
        send(&harness.service, wrong)
            .marks
            .reply
            .unwrap()
            .result
            .is_err()
    );
    let mut zero_ticket = set(&undo, 0, 'b', 8);
    assert!(
        send(&harness.service, zero_ticket.clone())
            .marks
            .reply
            .unwrap()
            .result
            .is_err()
    );
    zero_ticket.id.ticket = 5;
    zero_ticket.id.session += 1;
    assert!(
        send(&harness.service, zero_ticket)
            .marks
            .reply
            .unwrap()
            .result
            .is_err()
    );
}

#[test]
fn original_marks_keep_measured_vfr_pts_and_terminal_end_without_an_edit_occurrence() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    create(
        &harness.service,
        &scratch.path().join("original-marks.deadpan"),
    );
    let before = harness.imported("vfr.mp4");
    let source = before.sources.values().next().unwrap();
    let asset = source.asset.clone();
    let qualification = source.receipt.id().clone();
    let index = source.video_index.as_ref().unwrap().clone();
    let count = u64::try_from(index.frames().len()).unwrap();
    assert!(count > 2);
    assert_eq!(
        before.plan.duration().frames(),
        0,
        "No timeline occurrence exists"
    );
    let mut workspace = before;
    for (ticket, letter, ordinal) in [(1, 'v', 2), (3, 'e', count)] {
        let result = send(
            &harness.service,
            request(
                &workspace,
                ticket,
                Operation::Set {
                    letter,
                    location: Location::Original {
                        asset: asset.clone(),
                        qualification: qualification.clone(),
                        ordinal,
                    },
                },
            ),
        );
        saved(&result);
        let after = result.workspace.unwrap();
        unchanged_edit(&workspace, &after);
        workspace = after;
        let mark = &workspace.document.marks()[&marks::mark_id(letter).unwrap()];
        assert_eq!(&mark.owner, workspace.document.root());
        let Anchor::Source {
            moment:
                SourceMoment::Timestamp {
                    stream: SourceStream::Video,
                    timestamp,
                },
            ..
        } = mark.boundary.coordinate
        else {
            panic!("Original boundary")
        };
        assert_eq!(timestamp.time_base, index.time_base());
        assert_eq!(
            timestamp.ticks,
            if ordinal == count {
                index.terminal_end()
            } else {
                index.frames()[usize::try_from(ordinal).unwrap()].pts
            }
        );
        assert_eq!(
            jumped(&harness.service, &workspace, ticket + 1, letter),
            ResolvedLocation::Original {
                asset: asset.clone(),
                qualification: qualification.clone(),
                ordinal
            }
        );
    }
    let initial_counts = counts(&workspace.path);
    for (ticket, ordinal, qualification) in [
        (5, count + 1, qualification.clone()),
        (
            6,
            1,
            deadpan_core::SourceQualificationId::new("b".repeat(64)).unwrap(),
        ),
    ] {
        let rejected = send(
            &harness.service,
            request(
                &workspace,
                ticket,
                Operation::Set {
                    letter: 'z',
                    location: Location::Original {
                        asset: asset.clone(),
                        qualification,
                        ordinal,
                    },
                },
            ),
        );
        assert!(rejected.marks.reply.unwrap().result.is_err());
    }
    assert_eq!(counts(&workspace.path), initial_counts);
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(
        &harness.service,
        ProjectRequest::Open(workspace.path.clone()),
    )
    .workspace
    .unwrap();
    assert_eq!(
        jumped(&harness.service, &reopened, 1, 'e'),
        ResolvedLocation::Original {
            asset,
            qualification,
            ordinal: count
        }
    );
}

#[test]
fn retimed_marks_keep_exact_fraction_and_unscoped_repeat_marks_are_not_guessed() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("fractional-marks.deadpan");
    let (harness, before) = setup(&path);
    let marked = send(&harness.service, set(&before, 1, 'r', 3))
        .workspace
        .unwrap();
    let retimed = edited(
        &harness.service,
        &marked,
        ProjectEdit::Retime {
            node: node("a"),
            speed: ExactRatio::integer(2),
            pitch: deadpan_core::PitchPolicy::FollowSpeed,
            wrap: true,
        },
    );
    let target = retimed.committed.unwrap().selected_node.unwrap();
    let retimed = retimed.workspace.unwrap();
    assert_eq!(
        jumped(&harness.service, &retimed, 2, 'r'),
        ResolvedLocation::Edit {
            scope: SequenceScope::default(),
            selected: Some(target),
            frame: ProjectFrame(2),
            exact_frame: ExactRatio::new(3, 2).unwrap(),
        }
    );
    command(&harness.service, ProjectRequest::Close);
    {
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
        seed_command(
            &mut store,
            Command::WrapRepeat {
                node: node("b"),
                id: node("repeat-b"),
                plays: 3,
                gap: None,
                anchor_policy: Default::default(),
            },
            "repeat-b",
        );
        seed_command(
            &mut store,
            Command::SetMark {
                id: marks::mark_id('q').unwrap(),
                owner: node("b"),
                label: "q".into(),
                boundary: deadpan_core::BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: node("b"),
                        position: ExactRatio::integer(2),
                    },
                    bias: InsertionBias::Right,
                },
                loss_policy: deadpan_core::AnchorLossPolicy::KeepUnresolved,
            },
            "unscoped-mark",
        );
    }
    let workspace = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let before_counts = counts(&workspace.path);
    let rejected = send(
        &harness.service,
        request(&workspace, 1, Operation::Jump { letter: 'q' }),
    );
    assert!(
        rejected
            .marks
            .reply
            .unwrap()
            .result
            .unwrap_err()
            .contains("occurrence")
    );
    assert_eq!(counts(&workspace.path), before_counts);
    assert_eq!(*rejected.workspace.unwrap().document, *workspace.document);
}
