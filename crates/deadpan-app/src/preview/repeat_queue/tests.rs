use super::*;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, PresentationBasis, ProjectId, Subtree, apply,
};

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}

fn hold(frames: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(frames).unwrap(),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}

fn edited(document: &ProjectDocument, command: Command, name: &str) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap();
    transaction.forward.apply(document).unwrap()
}

fn fixture() -> (ProjectDocument, Target) {
    let empty = ProjectDocument::new(
        ProjectId::new("repeat-queue").unwrap(),
        revision("empty"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let document = edited(
        &empty,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("group"),
                nodes: [
                    (
                        node("group"),
                        BeatNode::sequence("Group", vec![node("beat"), node("other")]),
                    ),
                    (node("beat"), BeatNode::hold("Beat", hold(3))),
                    (node("other"), BeatNode::hold("Other", hold(5))),
                ]
                .into(),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "seed",
    );
    let target = Target {
        session: 17,
        revision: document.revision_id().clone(),
        scope: SequenceScope::test_path(vec![node("group")]),
        node: node("beat"),
        cursor: ProjectFrame(1),
        pane: Pane::Sequence,
    };
    (document, target)
}

fn wrapped(
    document: &ProjectDocument,
    target: &Target,
    plays: u32,
    name: &str,
) -> (ProjectDocument, CommittedEdit, Target) {
    let document = edited(
        document,
        Command::WrapRepeat {
            node: target.node.clone(),
            id: node(name),
            plays,
            gap: None,
            anchor_policy: Default::default(),
        },
        name,
    );
    let committed = completion(&document, target, Some(node(name)));
    let next = Target {
        revision: document.revision_id().clone(),
        node: node(name),
        cursor: ProjectFrame(0),
        ..target.clone()
    };
    (document, committed, next)
}

fn completion(
    document: &ProjectDocument,
    target: &Target,
    selected_node: Option<NodeId>,
) -> CommittedEdit {
    CommittedEdit {
        scoped: None,
        range_selection: None,
        revision: document.revision_id().clone(),
        selected_node,
        preserve_cursor: false,
        cursor: None,
        scope: target.scope.clone(),
        sound: None,
    }
}

fn saving(target: &Target, plays: u32, waiting: &[u32]) -> Queue {
    let mut queue = Queue::default();
    assert!(!queue.offer(target, plays).unwrap());
    assert!(
        !queue.active(),
        "only service admission can start the first save"
    );
    queue.started(target.clone(), plays);
    for plays in waiting {
        assert!(queue.offer(target, *plays).unwrap());
    }
    queue
}

fn assert_request(target: &Target, plays: u32) {
    let ProjectRequest::Edit {
        expected_session,
        expected_revision,
        scope,
        cursor,
        edit: ProjectEdit::WrapRepeat {
            node,
            plays: requested,
        },
    } = target.request(plays)
    else {
        panic!("a repeat continuation must remain an explicit wrap");
    };
    assert_eq!(expected_session, target.session);
    assert_eq!(expected_revision, target.revision);
    assert_eq!(scope, target.scope);
    assert_eq!(cursor, target.cursor);
    assert_eq!(node, target.node);
    assert_eq!(requested, plays);
}

#[test]
fn counted_wraps_continue_in_fifo_order_from_each_exact_committed_wrapper() {
    let (mut document, mut target) = fixture();
    let counts = [3, 2, 5, 4];
    let mut queue = saving(&target, counts[0], &counts[1..]);
    assert_eq!(queue.waiting(), 3);
    assert!(
        queue.next().is_none(),
        "an in-flight wrap owns the service slot"
    );
    for (index, plays) in counts.into_iter().enumerate() {
        assert_request(&target, plays);
        let name = format!("wrapper-{index}");
        let (after, committed, next_target) = wrapped(&document, &target, plays, &name);
        assert_eq!(
            queue.matching_completion(Some(target.session), Some(&after), Some(&committed), false),
            Ok(true)
        );
        queue.completed(next_target.clone());
        document = after;
        if let Some(expected) = counts.get(index + 1) {
            let (next, count) = queue.next().expect("the next admitted intent is ready");
            assert_eq!(next, next_target);
            assert_eq!(count, *expected);
            assert_eq!(next.revision, *document.revision_id());
            assert_eq!(next.node, node(&name));
            assert_eq!(next.cursor, ProjectFrame(0));
            assert!(
                queue.next().is_none(),
                "dispatch must reserve the only saving slot"
            );
            // The service acknowledges admission after next() reserves the slot.
            queue.started(next.clone(), count);
            target = next;
        } else {
            assert!(!queue.active());
            assert_eq!(queue.waiting(), 0);
            assert!(queue.next().is_none());
            assert!(queue.status().is_none());
        }
    }
    let mut selected = node("wrapper-3");
    for plays in counts.into_iter().rev() {
        let NodeKind::Repeat {
            child,
            iterations,
            gap,
            ..
        } = &document.nodes()[&selected].kind
        else {
            panic!("each queued wrap must add one actual Repeat ancestor");
        };
        assert_eq!(iterations.len(), plays);
        assert!(gap.is_none());
        selected = child.clone();
    }
    assert_eq!(selected, node("beat"));
    assert_eq!(document.duration().unwrap().frames(), 365);
}

#[test]
fn same_revision_background_progress_neither_finishes_nor_consumes_waiting_intents() {
    let (document, target) = fixture();
    let mut queue = saving(&target, 3, &[2, 4]);
    let prior_commit = completion(&document, &target, Some(target.node.clone()));
    for committed in [None, Some(&prior_commit)] {
        for _ in 0..3 {
            assert_eq!(
                queue.matching_completion(Some(target.session), Some(&document), committed, false),
                Ok(false)
            );
            assert_eq!(queue.waiting(), 2);
            assert!(queue.active());
            assert!(queue.next().is_none());
        }
    }
    let (after, committed, next_target) = wrapped(&document, &target, 3, "actual-wrap");
    assert_eq!(
        queue.matching_completion(Some(target.session), Some(&after), Some(&committed), false),
        Ok(true)
    );
    queue.completed(next_target.clone());
    assert_eq!(queue.next(), Some((next_target, 2)));
    assert_eq!(queue.waiting(), 1);
}

#[test]
fn completion_requires_the_current_session_document_and_exact_commit_metadata() {
    let (document, target) = fixture();
    let (after, committed, _) = wrapped(&document, &target, 3, "wrapper");
    let queue = saving(&target, 3, &[2]);
    for (session, current) in [
        (None, Some(&after)),
        (Some(target.session + 1), Some(&after)),
        (Some(target.session), None),
    ] {
        assert!(
            queue
                .matching_completion(session, current, Some(&committed), false)
                .is_err()
        );
    }
    assert!(
        queue
            .matching_completion(Some(target.session), Some(&after), None, false)
            .is_err()
    );
    let mut cases = Vec::new();
    let mut stale = committed.clone();
    stale.revision = target.revision.clone();
    cases.push(stale);
    let mut wrong_revision = committed.clone();
    wrong_revision.revision = revision("another-commit");
    cases.push(wrong_revision);
    let mut wrong_scope = committed.clone();
    wrong_scope.scope = SequenceScope::default();
    cases.push(wrong_scope);
    let mut retained_cursor = committed.clone();
    retained_cursor.preserve_cursor = true;
    cases.push(retained_cursor);
    let mut explicit_cursor = committed.clone();
    explicit_cursor.cursor = Some(ProjectFrame(0));
    cases.push(explicit_cursor);
    for selected in [
        None,
        Some(target.node.clone()),
        Some(node("missing")),
        Some(node("other")),
    ] {
        let mut changed = committed.clone();
        changed.selected_node = selected;
        cases.push(changed);
    }
    for changed in cases {
        assert!(
            queue
                .matching_completion(Some(target.session), Some(&after), Some(&changed), false)
                .is_err(),
            "unexpectedly admitted {changed:?}"
        );
        assert_eq!(queue.waiting(), 1);
    }
    let renamed = edited(
        &document,
        Command::Rename {
            node: target.node.clone(),
            label: "Unrelated change".into(),
        },
        "renamed",
    );
    let unrelated = completion(&renamed, &target, Some(target.node.clone()));
    assert!(
        queue
            .matching_completion(
                Some(target.session),
                Some(&renamed),
                Some(&unrelated),
                false
            )
            .is_err()
    );
    let changed_again = edited(
        &after,
        Command::Rename {
            node: node("other"),
            label: "Later change".into(),
        },
        "later",
    );
    assert!(
        queue
            .matching_completion(
                Some(target.session),
                Some(&changed_again),
                Some(&committed),
                false
            )
            .is_err()
    );
}

#[test]
fn a_real_repeat_with_the_wrong_child_count_or_gap_cannot_authorize_continuation() {
    let (document, target) = fixture();
    for (name, child, plays, gap) in [
        ("wrong-child", node("other"), 3, None),
        ("wrong-count", target.node.clone(), 4, None),
        ("with-gap", target.node.clone(), 3, Some(hold(1))),
    ] {
        let after = edited(
            &document,
            Command::WrapRepeat {
                node: child,
                id: node(name),
                plays,
                gap,
                anchor_policy: Default::default(),
            },
            name,
        );
        let committed = completion(&after, &target, Some(node(name)));
        let mut queue = saving(&target, 3, &[2]);
        assert!(
            queue
                .matching_completion(Some(target.session), Some(&after), Some(&committed), false)
                .is_err(),
            "{name}"
        );
        assert!(queue.next().is_none());
        assert_eq!(queue.cancel("completion changed"), 1);
        assert!(!queue.active());
        assert!(queue.next().is_none());
    }
}

#[test]
fn service_failure_cancels_pending_work_even_if_the_revision_or_commit_looks_valid() {
    let (document, target) = fixture();
    let (after, committed, _) = wrapped(&document, &target, 3, "wrapper");
    for (current, committed) in [(&document, None), (&after, Some(&committed))] {
        let mut queue = saving(&target, 3, &[2, 4]);
        assert!(
            queue
                .matching_completion(Some(target.session), Some(current), committed, true)
                .is_err()
        );
        assert_eq!(queue.cancel("save failed"), 2);
        assert_eq!(queue.waiting(), 0);
        assert!(!queue.active());
        assert!(queue.next().is_none());
        assert_eq!(
            queue.matching_completion(Some(target.session), Some(current), committed, false),
            Ok(false)
        );
    }
}

#[test]
fn overflow_preserves_the_admitted_fifo_prefix_and_counts_every_rejected_intent() {
    let (mut document, mut target) = fixture();
    let admitted: Vec<_> = (0..MAX_WAITING)
        .map(|index| [2, 1, 3, 1][index % 4])
        .collect();
    let mut queue = saving(&target, 3, &admitted);
    assert_eq!(queue.waiting(), MAX_WAITING);
    for (count, plays) in [99, 100].into_iter().enumerate() {
        let error = queue.offer(&target, plays).unwrap_err();
        assert!(
            error.contains(&format!("{} Repeat intents not queued", count + 1)),
            "{error}"
        );
        assert!(
            error.contains(&format!("{MAX_WAITING} already waiting")),
            "{error}"
        );
        assert_eq!(queue.waiting(), MAX_WAITING);
    }
    let mut dispatched = Vec::new();
    let mut saving_count = 3;
    for index in 0..=MAX_WAITING {
        let (after, committed, next_target) =
            wrapped(&document, &target, saving_count, &format!("wrap-{index}"));
        assert_eq!(
            queue.matching_completion(Some(target.session), Some(&after), Some(&committed), false),
            Ok(true)
        );
        queue.completed(next_target);
        document = after;
        if let Some((next, count)) = queue.next() {
            dispatched.push(count);
            assert!(queue.next().is_none());
            queue.started(next.clone(), count);
            target = next;
            saving_count = count;
        }
    }
    assert_eq!(dispatched, admitted);
    assert_eq!(queue.waiting(), 0);
    assert!(!queue.active());
    assert!(queue.next().is_none());
    assert!(
        queue
            .status()
            .unwrap()
            .contains("2 Repeat intents not queued")
    );
    assert!(
        !queue.offer(&target, 2).unwrap(),
        "a new first intent is submitted normally"
    );
    assert!(
        queue.status().is_none(),
        "a new chain clears the old overflow notice"
    );
}

#[test]
fn changed_context_cannot_append_intents_or_retarget_ready_work() {
    let (document, target) = fixture();
    let (after, committed, ready_target) = wrapped(&document, &target, 3, "wrapper");
    for ready in [false, true] {
        let mut queue = saving(&target, 3, &[2]);
        let current = if ready {
            assert_eq!(
                queue.matching_completion(
                    Some(target.session),
                    Some(&after),
                    Some(&committed),
                    false
                ),
                Ok(true)
            );
            queue.completed(ready_target.clone());
            ready_target.clone()
        } else {
            target.clone()
        };
        let changed = [
            Target {
                session: current.session + 1,
                ..current.clone()
            },
            Target {
                revision: revision("different"),
                ..current.clone()
            },
            Target {
                scope: SequenceScope::default(),
                ..current.clone()
            },
            Target {
                node: node("other"),
                ..current.clone()
            },
            Target {
                cursor: ProjectFrame(current.cursor.0 + 1),
                ..current.clone()
            },
            Target {
                pane: Pane::Sources,
                ..current.clone()
            },
        ];
        assert!(queue.context_matches(Some(&current)));
        assert!(!queue.context_matches(None));
        for context in changed {
            assert!(!queue.context_matches(Some(&context)), "{context:?}");
            assert!(queue.offer(&context, 99).is_err());
            assert_eq!(queue.waiting(), 1);
        }
        assert_eq!(queue.cancel("context changed"), 1);
        assert!(queue.next().is_none());
        assert!(queue.context_matches(None));
    }
}

#[test]
fn cancellation_before_dispatch_or_during_save_never_resurrects_waiting_work() {
    let (document, target) = fixture();
    let (after, committed, next_target) = wrapped(&document, &target, 3, "wrapper");
    for ready in [false, true] {
        let mut queue = saving(&target, 3, &[2, 4]);
        if ready {
            assert_eq!(
                queue.matching_completion(
                    Some(target.session),
                    Some(&after),
                    Some(&committed),
                    false
                ),
                Ok(true)
            );
            queue.completed(next_target.clone());
        }
        assert_eq!(queue.cancel("Escape"), 2);
        assert_eq!(queue.cancel("Escape again"), 0);
        assert_eq!(queue.waiting(), 0);
        assert!(!queue.active());
        assert!(
            queue
                .status()
                .unwrap()
                .contains("Cancelled 2 queued Repeats")
        );
        assert!(queue.next().is_none());
        assert_eq!(
            queue.matching_completion(Some(target.session), Some(&after), Some(&committed), false),
            Ok(false)
        );
        // Even an already queued UI adoption callback cannot recreate an intent.
        queue.completed(next_target.clone());
        assert!(queue.next().is_none());
        assert!(!queue.active());
    }
    let mut only_submitted = saving(&target, 3, &[]);
    assert_eq!(only_submitted.cancel("Escape"), 0);
    assert!(!only_submitted.active());
    assert!(only_submitted.next().is_none());
}
