//! Qualified positive admission controls for shared and dormant Repeat defaults.

use super::*;
use crate::{
    library::ProjectLibrary,
    project::{ImportStage, ProjectRequest, ProjectService, ProjectUpdate, RoomToneSelection},
};
use deadpan_core::SourceVideo;
use deadpan_store::{
    AccessMode, ProjectStore, original_media::OriginalOwnership, single_source::SingleSourceState,
};
use std::time::{Duration, Instant};

fn command(service: &ProjectService, request: ProjectRequest) -> ProjectUpdate {
    let lane = crate::project::Lane::of(&request);
    let completed = service.completed_for_check(lane);
    service.submit(request).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(update) = service.take_completion(lane, completed) {
            return update;
        }
        assert!(Instant::now() < deadline, "project command timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn seed_command(store: &mut ProjectStore, command: Command, name: &str) {
    let document = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        })
        .unwrap();
}

fn exercise(dormant: bool) {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/fixtures/offset-bframes.mp4")
                .canonicalize()
                .unwrap(),
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let initialized = loop {
        if let Some(update) = service.take_update()
            && update.import.as_ref().is_some_and(|status| {
                matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
            })
        {
            assert!(update.error.is_none(), "{:?}", update.error);
            assert_eq!(
                update.import.as_ref().unwrap().stage,
                ImportStage::Complete,
                "Original preparation status: {:?}",
                update.import
            );
            break update;
        }
        assert!(Instant::now() < deadline, "Original preparation timed out");
        std::thread::sleep(Duration::from_millis(2));
    };
    let opened = command(
        &service,
        ProjectRequest::Open(initialized.workspace.as_ref().unwrap().path.clone()),
    );
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let initial = opened.workspace.as_ref().unwrap();
    assert!(matches!(
        initial.single_source,
        Some(SingleSourceState::Ready { .. })
    ));
    let qualified = initial
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let prepared = command(
        &service,
        ProjectRequest::PrepareRoomTone {
            expected_session: initial.session,
            expected_revision: initial.document.revision_id().clone(),
            ticket: 71,
            selection: RoomToneSelection::Original {
                asset: qualified.asset.clone(),
                qualification: qualified.receipt.id().clone(),
                ordinals: 1..4,
            },
        },
    );
    assert!(prepared.error.is_none(), "{:?}", prepared.error);
    assert!(
        prepared.room_tone_error.is_none(),
        "{:?}",
        prepared.room_tone_error
    );
    let audio = prepared.room_tone.unwrap().source;
    let NodeKind::Sequence { children } = &initial.document.nodes()[initial.document.root()].kind
    else {
        panic!("ordinary initial root");
    };
    assert_eq!(children.len(), 1);
    let original = children[0].clone();
    let NodeKind::Source { source } = &initial.document.nodes()[&original].kind else {
        panic!("full Original source");
    };
    let SourceVideo::Stream { asset, span } = &source.video else {
        panic!("qualified Original video");
    };
    assert!(
        source.audio.is_some(),
        "fixture must carry real qualified audio"
    );
    let audible_hold = |name: &str, length: i64| {
        (
            node(name),
            BeatNode::hold(
                name,
                HoldRecipe {
                    duration: frames(length),
                    video: HoldVideo::Freeze {
                        asset: asset.clone(),
                        timestamp: span.start(),
                    },
                    picture_context: None,
                    audio: HoldAudio::RoomTone {
                        source: audio.clone(),
                    },
                },
            ),
        )
    };
    let mut nodes = vec![
        repeat("shared-outer", "shared-inner", 3, None),
        repeat("shared-inner", "shared-body", 2, None),
        sequence(
            "shared-body",
            &["shared-before", "shared-pause", "shared-after"],
        ),
        audible_hold("shared-before", 2),
        audible_hold("shared-pause", 2),
        audible_hold("shared-after", 2),
        sequence(
            "inner-owned",
            &["inner-before", "inner-pause", "inner-after"],
        ),
        audible_hold("inner-before", 2),
        audible_hold("inner-pause", 4),
        audible_hold("inner-after", 2),
    ];
    let owned_names = ["outer-owned-zero", "outer-owned-one", "outer-owned-two"];
    let mut outer_overrides = vec![];
    for (ordinal, name) in owned_names.iter().enumerate() {
        if dormant || ordinal == 2 {
            let before = format!("{name}-before");
            let pause = format!("{name}-pause");
            let after = format!("{name}-after");
            nodes.push(sequence(name, &[&before, &pause, &after]));
            nodes.push(audible_hold(&before, 2));
            nodes.push(audible_hold(&pause, 7));
            nodes.push(audible_hold(&after, 2));
            outer_overrides.push((u32::try_from(ordinal).unwrap(), *name));
        }
    }
    let subtree = Subtree {
        root: node("shared-outer"),
        nodes: nodes.into_iter().collect(),
        overrides: [
            overrides("shared-inner", &[(1, "inner-owned")]),
            overrides("shared-outer", &outer_overrides),
        ]
        .into(),
        gap_overrides: BTreeMap::new(),
    };
    let lead = audible_hold("root-lead", 3);
    let tail = audible_hold("root-tail", 3);
    let path = initial.path.clone();
    let parent = initial.document.root().clone();
    command(&service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::Delete { node: original },
        "shared-delete-baseline",
    );
    for (index, subtree) in [
        Subtree {
            root: lead.0.clone(),
            nodes: [lead].into(),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
        subtree,
        Subtree {
            root: tail.0.clone(),
            nodes: [tail].into(),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        seed_command(
            &mut store,
            Command::Insert {
                parent: parent.clone(),
                index,
                subtree,
            },
            &format!("shared-insert-{index}"),
        );
    }
    drop(store);
    let opened = command(&service, ProjectRequest::Open(path));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let workspace = opened.workspace.as_ref().unwrap();
    assert!(matches!(
        workspace.single_source,
        Some(SingleSourceState::Ready { .. })
    ));
    workspace.document.validate().unwrap();
    assert!(workspace.document.sounds().is_empty());
    assert!(workspace.document.beat_sounds().is_empty());
    assert!(
        workspace
            .document
            .nodes()
            .values()
            .all(|beat| !matches!(beat.kind, NodeKind::Retime { .. }))
    );
    let cursor = ProjectFrame(19);
    let mut state = State::new(
        workspace,
        SequenceScope::default(),
        node("shared-outer"),
        cursor,
    )
    .unwrap();
    assert!(state.enter(workspace).unwrap());
    assert!(state.enter(workspace).unwrap());
    state.select(workspace, 1).unwrap();
    assert_eq!(state.selected_target().node, node("shared-pause"));
    let target = state.target(workspace, cursor).unwrap();
    target.validate(workspace).unwrap();
    assert_eq!(target.target.repeats.len(), 2);
    assert!(
        target
            .target
            .repeats
            .iter()
            .all(|step| step.branch == RepeatEditBranch::Default)
    );
    assert!(target.also.is_empty());
    assert!(
        target.semantic_framing(workspace).is_err(),
        "shared duration must not broaden one-play framing authority"
    );

    let changed = edited(
        &workspace.document,
        Command::SetHoldDuration {
            node: node("shared-pause"),
            duration: frames(4),
        },
        "shared-raw-duration-control",
    );
    changed.validate().unwrap();
    let changed_plan = RenderPlan::compile(&changed).unwrap();
    assert_eq!(changed.overrides(), workspace.document.overrides());
    let mut expected_nodes = workspace.document.nodes().clone();
    let NodeKind::Hold { recipe } =
        &mut expected_nodes.get_mut(&node("shared-pause")).unwrap().kind
    else {
        panic!("selected Hold");
    };
    recipe.duration = frames(4);
    assert_eq!(
        changed.nodes(),
        &expected_nodes,
        "owned branches and every other recipe stay byte-for-byte authored-equivalent"
    );
    if dormant {
        assert_eq!(workspace.plan.duration().frames(), 39);
        assert_eq!(changed_plan.duration(), workspace.plan.duration());
        assert!(target.presentation.is_none());
        assert!(state.presentation(workspace).unwrap().is_none());
        assert_eq!(
            state.no_picture_reason(workspace).unwrap().as_deref(),
            Some(DORMANT_DEFAULT)
        );
        for frame in 0..39 {
            let mut before = workspace.plan.picture(ProjectFrame(frame)).unwrap();
            before.revision_id = changed.revision_id().clone();
            assert_eq!(changed_plan.picture(ProjectFrame(frame)).unwrap(), before);
        }
    } else {
        assert_eq!(workspace.plan.duration().frames(), 45);
        assert_eq!(changed_plan.duration().frames(), 49);
        let presentation = target
            .presentation
            .as_ref()
            .expect("live default has a real representative");
        assert_eq!(
            state.presentation(workspace).unwrap().unwrap().frames,
            19..21
        );
        for (plan, frame) in [(&*workspace.plan, 19), (&changed_plan, 21)] {
            assert!(
                plan.picture(ProjectFrame(frame))
                    .unwrap()
                    .framing
                    .iter()
                    .any(|layer| &layer.instance == presentation)
            );
        }
        let uses = |plan: &RenderPlan| {
            (0..plan.duration().frames())
                .filter(|frame| {
                    plan.picture(ProjectFrame(*frame))
                        .unwrap()
                        .framing
                        .iter()
                        .any(|layer| layer.instance.node == node("shared-pause"))
                })
                .count()
        };
        assert_eq!(uses(&workspace.plan), 4, "two live uses of the 2f default");
        assert_eq!(
            uses(&changed_plan),
            8,
            "the same two uses grow; owned plays stay unchanged"
        );
    }
    // Existing native entrypoint: this must offer the authored shared length,
    // including a dormant default without manufacturing a presentation play.
    assert_eq!(
        state.hold_parameter(workspace).as_deref(),
        Some("hold-duration 2f"),
        "All plays must offer Change duration for this valid shared Hold"
    );
    service_checks(&service, &opened, &target, &mut state, dormant);
    command(&service, ProjectRequest::Close);
}

#[test]
fn native_all_default_hold_duration_prefills_shared_length() {
    exercise(false);
}

#[test]
fn native_dormant_all_default_hold_duration_prefills_without_inventing_a_play() {
    exercise(true);
}

fn service_checks(
    service: &ProjectService,
    opened: &ProjectUpdate,
    target: &crate::project::scoped::Target,
    state: &mut State,
    dormant: bool,
) {
    use crate::project::macros::{Id, Operation};
    use deadpan_core::{PauseLength, SemanticContext, SemanticInstruction, SemanticProgram};
    use std::num::NonZeroU32;

    let before = opened.workspace.as_ref().unwrap();
    let path = before.path.clone();
    let make_id = |update: &ProjectUpdate, request| {
        let workspace = update.workspace.as_ref().unwrap();
        Id {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            revision: workspace.document.revision_id().clone(),
            bank_version: update.registers.as_ref().unwrap().version,
            request,
        }
    };
    let length = |value| SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(value).unwrap(),
        },
    };
    let context = SemanticContext {
        parent: before.document.root().clone(),
        cursor: target.cursor,
        selected_child: Some(target.root.clone()),
        original: None,
        visual_selection: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: Some(target.semantic_shared_hold_duration(before).unwrap()),
        selected_hold_duration: None,
        mixed_hold_duration: None,
    };
    let rows = || {
        rusqlite::Connection::open(path.join("project.sqlite")).unwrap().query_row(
        "SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history),(SELECT count(*) FROM transaction_steps)",
        [], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
    ).unwrap()
    };
    let bank = || {
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap()
    };
    let initial_bank = bank();
    let initial_rows = rows();
    let no_op = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: make_id(opened, 80),
            instruction: Box::new(length(2)),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    assert!(no_op.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(), initial_rows);
    assert_eq!(bank(), initial_bank);
    assert_eq!(state.target(before, target.cursor).unwrap(), *target);
    let changed = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: make_id(opened, 81),
            instruction: Box::new(length(4)),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    let receipt = changed.macros.as_ref().unwrap().result.as_ref().unwrap();
    let commit = receipt.committed().unwrap();
    let after = changed.workspace.as_ref().unwrap();
    assert_eq!(
        rows(),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 1)
    );
    assert_eq!(bank(), initial_bank);
    let continuation = if dormant {
        target.cursor
    } else {
        ProjectFrame(21)
    };
    assert_eq!(commit.cursor, Some(continuation));
    assert_eq!(commit.selected_node.as_ref(), Some(&target.root));
    let scoped = commit.scoped.as_ref().unwrap();
    assert_eq!(scoped.before, *target);
    assert_eq!(scoped.target, target.target);
    assert_eq!(scoped.presentation, target.presentation);
    assert!(state.reconcile(after, scoped, commit.cursor).unwrap());
    assert_eq!(
        state.hold_parameter(after).as_deref(),
        Some("hold-duration 4f")
    );
    assert_eq!(
        state.target(after, continuation).unwrap().presentation,
        target.presentation
    );
    assert!(
        changed.semantic.as_ref().unwrap().edit.is_some(),
        "direct Apply seeds dot"
    );

    let stale_rows = rows();
    let stale = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: make_id(opened, 82),
            instruction: Box::new(length(6)),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    assert!(stale.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(), stale_rows);
    let undone = command(
        service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    assert_eq!(
        undone.workspace.as_ref().unwrap().document.nodes(),
        before.document.nodes()
    );
    assert_eq!(
        undone.workspace.as_ref().unwrap().document.overrides(),
        before.document.overrides()
    );
    let redone = command(
        service,
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
    assert_eq!(
        redone.workspace.as_ref().unwrap().document.nodes(),
        after.document.nodes()
    );
    command(service, ProjectRequest::Close);
    let reopened = command(service, ProjectRequest::Open(path.clone()));
    assert_eq!(
        *reopened.workspace.as_ref().unwrap().document,
        *redone.workspace.as_ref().unwrap().document
    );

    // A valid first setter followed by an unsupported instruction must roll back
    // the entire named program, including cursor, history and register state.
    let saved = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: make_id(&reopened, 83),
            register: 'z',
            program: Arc::new(
                SemanticProgram::new(vec![length(6), SemanticInstruction::Split]).unwrap(),
            ),
        }),
    );
    saved.macros.as_ref().unwrap().result.as_ref().unwrap();
    let failed_rows = rows();
    let failed_bank = bank();
    let mut current_context = context.clone();
    current_context.cursor = continuation;
    let failed = command(
        service,
        ProjectRequest::Macro(Operation::Run {
            id: make_id(&saved, 84),
            register: 'z',
            count: 1,
            scope: target.scope.clone(),
            context: current_context,
        }),
    );
    assert!(failed.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(), failed_rows);
    assert_eq!(bank(), failed_bank);
    assert_eq!(
        *failed.workspace.as_ref().unwrap().document,
        *reopened.workspace.as_ref().unwrap().document
    );

    let inner = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: make_id(&failed, 85),
            register: 'x',
            program: Arc::new(SemanticProgram::new(vec![length(5)]).unwrap()),
        }),
    );
    inner.macros.as_ref().unwrap().result.as_ref().unwrap();
    let outer = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: make_id(&inner, 86),
            register: 'y',
            program: Arc::new(
                SemanticProgram::new(vec![
                    SemanticInstruction::Call {
                        register: deadpan_core::RegisterName::new('x').unwrap(),
                        count: NonZeroU32::new(1).unwrap(),
                    },
                    length(6),
                ])
                .unwrap(),
            ),
        }),
    );
    outer.macros.as_ref().unwrap().result.as_ref().unwrap();
    let call_rows = rows();
    let call_bank = bank();
    let mut call_context = context;
    call_context.cursor = continuation;
    let called = command(
        service,
        ProjectRequest::Macro(Operation::Run {
            id: make_id(&outer, 87),
            register: 'y',
            count: 2,
            scope: target.scope.clone(),
            context: call_context,
        }),
    );
    let call_receipt = called.macros.as_ref().unwrap().result.as_ref().unwrap();
    assert_eq!(rows(), (call_rows.0 + 1, call_rows.1 + 1, call_rows.2 + 4));
    assert_eq!(bank(), call_bank);
    assert_eq!(
        call_receipt.committed().unwrap().cursor,
        Some(if dormant {
            target.cursor
        } else {
            ProjectFrame(23)
        })
    );
    assert_eq!(
        called.workspace.as_ref().unwrap().document.overrides(),
        before.document.overrides()
    );
    assert!(
        called.semantic.as_ref().unwrap().edit.is_none(),
        "named Run does not seed dot"
    );
}

#[test]
fn shared_duration_hint_reuses_compact_index_with_mixed_choices_but_not_several_plays() {
    let document = fixture(
        "hint-outer",
        vec![
            repeat("hint-outer", "hint-inner", 2, None),
            repeat("hint-inner", "hint-pause", 1_000_000, None),
            hold("hint-pause", 2),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut inspector = state(document, "hint-outer", 0);
    assert!(inspector.enter_in().unwrap());
    let retained_index = inspector.index.clone();
    for _ in 0..128 {
        assert_eq!(
            inspector.hold_parameter_in().as_deref(),
            Some("hold-duration 2f")
        );
        assert!(Arc::ptr_eq(&retained_index, &inspector.index));
    }
    inspector.switch_play_in(1).unwrap();
    assert_eq!(
        inspector.hold_parameter_in().as_deref(),
        Some("hold-duration 2f")
    );
    assert!(Arc::ptr_eq(&retained_index, &inspector.index));
    inspector.switch_in(RepeatEditBranch::Default).unwrap();
    inspector.levels.last_mut().unwrap().also.push(
        match &inspector.index.document.nodes()[&node("hint-inner")].kind {
            NodeKind::Repeat { iterations, .. } => iterations.at(1).unwrap(),
            _ => panic!("Repeat"),
        },
    );
    assert!(
        inspector.hold_parameter_in().is_none(),
        "several selected plays stay unavailable"
    );
}
