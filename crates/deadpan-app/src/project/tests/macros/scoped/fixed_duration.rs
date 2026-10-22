//! Positive native admission controls for an ordinary Hold inside a fixed window.

use super::*;
use deadpan_core::{FrameRange, PauseLength, PitchPolicy, RetimePurpose, Subtree};

fn length(frames: u32) -> SemanticInstruction {
    SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn exercise(output: i64, mapping_start: i64, mapping_end: i64, cursor: i64) {
    let scratch = tempfile::tempdir().unwrap();
    let (service, mut opened, _) = setup_source(&scratch.path().join("Documents"));
    // Reuse the qualified Original preparation, undoing both fixture Repeats.
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
    let workspace = opened.workspace.as_ref().unwrap();
    assert!(matches!(
        workspace.single_source,
        Some(SingleSourceState::Ready { .. })
    ));
    assert_eq!(workspace.plan.duration().frames(), 120);
    let path = workspace.path.clone();
    let parent = workspace.document.root().clone();
    command(&service, ProjectRequest::Close);
    let rate = node("fixed-duration-rate");
    let body = node("fixed-duration-body");
    let selected = node("fixed-duration-hold");
    let lead = node("fixed-duration-lead");
    let tail = node("fixed-duration-tail");
    let mut wrapper = BeatNode::sequence("Fixed window", vec![]);
    wrapper.kind = NodeKind::Retime {
        child: body.clone(),
        duration: FrameDuration::new(output).unwrap(),
        mapping: FrameRange::new(ProjectFrame(mapping_start), ProjectFrame(mapping_end)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::Insert {
            parent,
            index: 0,
            subtree: Subtree {
                root: rate.clone(),
                nodes: BTreeMap::from([
                    (rate.clone(), wrapper.clone()),
                    (
                        body,
                        BeatNode::sequence(
                            "Interior",
                            vec![lead.clone(), selected.clone(), tail.clone()],
                        ),
                    ),
                    (lead, BeatNode::hold("Lead", hold(2))),
                    (
                        selected.clone(),
                        BeatNode::hold("Resize this pause", hold(8)),
                    ),
                    (tail, BeatNode::hold("Suffix", hold(10))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "fixed-duration-fixture",
    );
    drop(store);
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let before = opened.workspace.as_ref().unwrap();
    before.document.validate().unwrap();
    assert!(
        before
            .document
            .nodes()
            .values()
            .all(|beat| !matches!(beat.kind, NodeKind::Repeat { .. }))
    );
    let picture = before.plan.picture(ProjectFrame(cursor)).unwrap();
    let presentation = picture
        .framing
        .iter()
        .find(|layer| layer.instance.node == selected)
        .expect("the interior cursor must actually display the selected Hold")
        .instance
        .clone();
    assert!(presentation.repeats.is_empty());
    let target = Target {
        session: before.session,
        navigation: 0,
        project: before.document.project_id().clone(),
        revision: before.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: rate.clone(),
        target: ScopedNodeTarget {
            node: selected.clone(),
            repeats: vec![],
        },
        presentation: Some(presentation),
        cursor: ProjectFrame(cursor),
        also: vec![],
    };
    target.validate(before).unwrap();
    let bank = stored_bank(&path);
    // Duration authority stays separate from generic framing authority.
    assert!(target.semantic_framing(before).is_err());
    let context = SemanticContext {
        parent: before.document.root().clone(),
        cursor: target.cursor,
        selected_child: Some(rate.clone()),
        original: None,
        scoped: None,
        visual_selection: None,
        hold_duration: Some(target.semantic_hold_duration(before).unwrap()),
        shared_hold_duration: None,
        mixed_hold_duration: None,
    };
    let stable_rows = rows(&path);
    let mut conflicting_context = context.clone();
    conflicting_context.visual_selection = Some(deadpan_core::SemanticVisualSelection::Time {
        anchor: ProjectFrame(0),
        head: ProjectFrame(1),
        extending: false,
    });
    let conflicting = command(
        &service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(&opened, 89),
            instruction: length(10),
            repeat_version: None,
            scope: target.scope.clone(),
            context: conflicting_context,
        }),
    );
    assert!(conflicting.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), stable_rows);
    let unchanged = command(
        &service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(&opened, 90),
            instruction: length(8),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    assert!(unchanged.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), stable_rows);
    assert_eq!(stored_bank(&path), bank);
    let changed = send(
        &service,
        Operation::Apply {
            id: id(&opened, 1),
            instruction: SemanticInstruction::SetHoldDuration {
                length: PauseLength::Frames {
                    frames: NonZeroU32::new(10).unwrap(),
                },
            },
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        },
    );
    let after = changed.workspace.as_ref().unwrap();
    let committed = receipt(&changed).committed().unwrap();
    let visibility = after
        .document
        .ordinary_hold_duration_visibility(&context.parent, context.hold_duration.as_ref().unwrap())
        .unwrap();
    assert_eq!(committed.cursor, Some(visibility.frames.start()));
    assert_eq!(committed.selected_node.as_ref(), Some(&rate));
    assert_eq!(committed.scoped.as_ref().unwrap().before, target);
    assert_eq!(
        after.document.nodes()[&rate],
        wrapper,
        "fixed mapping and output remain authored unchanged"
    );
    assert_eq!(after.plan.duration(), before.plan.duration());
    assert!(
        matches!(&after.document.nodes()[&selected].kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 10)
    );
    assert_eq!(stored_bank(&path), bank);
    let stale_rows = rows(&path);
    let stale = command(
        &service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(&opened, 91),
            instruction: length(11),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    assert!(stale.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), stale_rows);
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
    let mut current_context = context.clone();
    current_context.cursor = visibility.frames.start();
    let saved_inner = send(&service, save(&reopened, 3, 'x', program(vec![length(11)])));
    let saved_outer = send(
        &service,
        save(
            &saved_inner,
            4,
            'y',
            program(vec![
                SemanticInstruction::Call {
                    register: RegisterName::new('x').unwrap(),
                    count: NonZeroU32::new(1).unwrap(),
                },
                length(12),
            ]),
        ),
    );
    let before_calls = rows(&path);
    let call_bank = stored_bank(&path);
    let called = send(
        &service,
        Operation::Run {
            id: id(&saved_outer, 5),
            register: 'y',
            count: 2,
            scope: target.scope.clone(),
            context: current_context.clone(),
        },
    );
    // Two calls each change duration to 11 then 12. They share one revision
    // and one history entry, while retaining all four transaction steps.
    assert_eq!(
        rows(&path),
        (before_calls.0 + 1, before_calls.1 + 1, before_calls.2 + 4)
    );
    assert_eq!(stored_bank(&path), call_bank);
    assert!(
        matches!(&called.workspace.as_ref().unwrap().document.nodes()[&selected].kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 12)
    );
    let failed_program = send(
        &service,
        save(
            &called,
            6,
            'z',
            program(vec![length(13), SemanticInstruction::Split]),
        ),
    );
    let before_failure = rows(&path);
    let failure_bank = stored_bank(&path);
    let failed = command(
        &service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&failed_program, 7),
            register: 'z',
            count: 1,
            scope: target.scope.clone(),
            context: current_context,
        }),
    );
    assert!(failed.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), before_failure);
    assert_eq!(stored_bank(&path), failure_bank);
    assert_same_content(
        &failed_program.workspace.as_ref().unwrap().document,
        &called.workspace.as_ref().unwrap().document,
    );
    command(&service, ProjectRequest::Close);
}

#[test]
fn native_ordinary_fixed_retime_hold_duration_admits_interior_capture() {
    exercise(20, 0, 20, 5);
}

#[test]
fn native_ordinary_fixed_retime_hold_duration_admits_cropped_fractional_visibility() {
    // The Hold's intrinsic start maps to -2/3; visible start is the crop at 0.
    // Its visible end maps to 14/3. Frame 2 is strictly inside that interval.
    exercise(10, 3, 18, 2);
}
