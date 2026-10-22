//! Native service receipts distinguish a cropped visible start from an anchor.

use super::*;
use deadpan_core::{FrameRange, IterationOrder, PauseLength, PitchPolicy, RetimePurpose, Subtree};

fn length(frames: u32) -> SemanticInstruction {
    SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn exercise(output: i64, crop: (i64, i64), cursor: i64, continuation: i64) {
    let scratch = tempfile::tempdir().unwrap();
    let (service, mut opened, _) = setup_source(&scratch.path().join("Documents"));
    for _ in 0..2 {
        opened = command(
            &service,
            ProjectRequest::Undo {
                expected_revision: opened
                    .workspace
                    .as_ref()
                    .unwrap()
                    .document
                    .revision_id()
                    .clone(),
            },
        );
    }
    let initial = opened.workspace.as_ref().unwrap();
    assert_eq!(initial.plan.duration().frames(), 120);
    let path = initial.path.clone();
    let parent = initial.document.root().clone();
    command(&service, ProjectRequest::Close);
    let repeat = node("one-fixed-repeat");
    let stage = node("one-fixed-stage");
    let body = node("one-fixed-body");
    let held = node("one-fixed-held");
    let lead = node("one-fixed-before");
    let tail = node("one-fixed-after");
    let iterations = IterationOrder::new(RevisionId::new("one-fixed-plays").unwrap(), 3).unwrap();
    let mut wrapper = BeatNode::sequence("Fixed window", vec![]);
    wrapper.kind = NodeKind::Retime {
        child: body.clone(),
        duration: FrameDuration::new(output).unwrap(),
        mapping: FrameRange::new(ProjectFrame(crop.0), ProjectFrame(crop.1)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let mut outer = BeatNode::sequence("Three fixed windows", vec![]);
    outer.kind = NodeKind::Repeat {
        child: stage.clone(),
        iterations,
        gap: None,
        escalation: None,
    };
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::Insert {
            parent,
            index: 0,
            subtree: Subtree {
                root: repeat.clone(),
                nodes: BTreeMap::from([
                    (repeat.clone(), outer),
                    (stage.clone(), wrapper.clone()),
                    (
                        body,
                        BeatNode::sequence(
                            "Interior",
                            vec![lead.clone(), held.clone(), tail.clone()],
                        ),
                    ),
                    (lead, BeatNode::hold("Before", hold(2))),
                    (held.clone(), BeatNode::hold("Selected pause", hold(8))),
                    (tail, BeatNode::hold("Suffix", hold(10))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "one-fixed-fixture",
    );
    drop(store);
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    let before = opened.workspace.as_ref().unwrap();
    let NodeKind::Repeat { iterations, .. } = &before.document.nodes()[&repeat].kind else {
        panic!("committed Repeat");
    };
    let second = iterations.at(1).unwrap();
    let picture = before.plan.picture(ProjectFrame(cursor)).unwrap();
    let presentation = picture
        .framing
        .iter()
        .find(|layer| layer.instance.node == held)
        .unwrap()
        .instance
        .clone();
    assert_eq!(presentation.repeats.len(), 1);
    assert_eq!(presentation.repeats[0].iteration, second);
    let target = Target {
        session: before.session,
        navigation: 0,
        project: before.document.project_id().clone(),
        revision: before.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: repeat.clone(),
        target: ScopedNodeTarget {
            node: held.clone(),
            repeats: vec![RepeatEditStep {
                repeat: repeat.clone(),
                branch: RepeatEditBranch::Play {
                    iteration: second.clone(),
                },
            }],
        },
        presentation: Some(presentation),
        cursor: ProjectFrame(cursor),
        also: vec![],
    };
    let context = SemanticContext {
        parent: before.document.root().clone(),
        cursor: target.cursor,
        selected_child: Some(repeat.clone()),
        scoped: Some(target.semantic_framing(before).unwrap()),
        visual_selection: None,
        original: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
    };
    let visible = crate::project::scoped::fixed_duration_visibility(
        &before.document,
        &context.parent,
        context.scoped.as_ref().unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(visible.frames.start(), ProjectFrame(continuation));
    crate::project::scoped::verify_duration_pictures(
        &before.plan,
        &context.scoped.as_ref().unwrap().presentation,
        visible,
    )
    .unwrap();
    let unchanged_bank = stored_bank(&path);
    let before_rows = rows(&path);
    let changed = send(
        &service,
        Operation::Apply {
            id: id(&opened, 1),
            instruction: Box::new(length(10)),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        },
    );
    let after = changed.workspace.as_ref().unwrap();
    let committed = receipt(&changed).committed().unwrap();
    assert_eq!(committed.cursor, Some(ProjectFrame(continuation)));
    assert_eq!(committed.selected_node, Some(repeat.clone()));
    let mapped = committed.scoped.as_ref().unwrap();
    assert_eq!(mapped.before, target);
    assert_ne!(mapped.target.node, held);
    assert_eq!(
        mapped.target.repeats,
        context.scoped.as_ref().unwrap().target.repeats
    );
    assert!(matches!(&after.document.nodes()[&mapped.target.node].kind,
        NodeKind::Hold { recipe } if recipe.duration.frames() == 10));
    assert_eq!(after.document.nodes()[&stage], wrapper);
    assert_eq!(after.plan.duration(), before.plan.duration());
    assert_eq!(after.document.overrides()[&repeat].len(), 1);
    assert!(after.document.overrides()[&repeat].get(&second).is_some());
    assert_eq!(stored_bank(&path), unchanged_bank);
    assert_eq!(
        rows(&path),
        (before_rows.0 + 1, before_rows.1 + 1, before_rows.2 + 1)
    );
    let stable_rows = rows(&path);
    let stale = command(
        &service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(&opened, 2),
            instruction: Box::new(length(11)),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    assert!(stale.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), stable_rows);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    assert_same_content(
        &undone.workspace.as_ref().unwrap().document,
        &before.document,
    );
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: undone
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert_same_content(
        &redone.workspace.as_ref().unwrap().document,
        &after.document,
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path.clone()));
    assert_same_content(
        &reopened.workspace.as_ref().unwrap().document,
        &after.document,
    );

    let mut current = context;
    current.cursor = ProjectFrame(continuation);
    let scoped = current.scoped.as_mut().unwrap();
    scoped.target = mapped.target.clone();
    scoped.presentation = mapped.presentation.clone().unwrap();
    let saved = send(
        &service,
        save(&reopened, 3, 'x', program(vec![length(11), length(12)])),
    );
    let prior_rows = rows(&path);
    let prior_bank = stored_bank(&path);
    let run = send(
        &service,
        Operation::Run {
            id: id(&saved, 4),
            register: 'x',
            count: 2,
            scope: target.scope,
            context: current,
        },
    );
    assert_eq!(
        rows(&path),
        (prior_rows.0 + 1, prior_rows.1 + 1, prior_rows.2 + 4)
    );
    assert_eq!(stored_bank(&path), prior_bank);
    assert_eq!(
        run.workspace.as_ref().unwrap().document.nodes().len(),
        after.document.nodes().len()
    );
    assert!(
        matches!(&run.workspace.as_ref().unwrap().document.nodes()[&mapped.target.node].kind,
        NodeKind::Hold { recipe } if recipe.duration.frames() == 12)
    );
    assert_eq!(
        receipt(&run).committed().unwrap().cursor,
        Some(ProjectFrame(continuation))
    );
    command(&service, ProjectRequest::Close);
}

#[test]
fn native_this_play_fixed_retime_duration_service_retains_visible_continuation() {
    exercise(20, (0, 20), 25, 22);
}

#[test]
fn native_this_play_fixed_retime_duration_service_retains_cropped_fractional_continuation() {
    // Intrinsic start is 28/3, outside the middle stage's crop. Frame 10 is
    // its first visible picture, while frame 12 is strictly inside the Hold.
    exercise(10, (3, 18), 12, 10);
}
