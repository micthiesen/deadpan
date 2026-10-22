//! Positive native mixed-scope admission baseline using only preexisting public APIs.

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

fn exercise(outer_play: bool) {
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
        if ordinal == 2 {
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
    if outer_play {
        state.switch_play(workspace, 2).unwrap();
    }
    assert!(state.enter(workspace).unwrap());
    if !outer_play {
        state.switch_play(workspace, 1).unwrap();
    }
    assert!(state.enter(workspace).unwrap());
    state.select(workspace, 1).unwrap();
    let target = state.target(workspace, cursor).unwrap();
    target.validate(workspace).unwrap();
    assert_eq!(target.target.node, node("shared-pause"));
    assert_eq!(target.target.repeats.len(), 2);
    assert_eq!(
        target
            .target
            .repeats
            .iter()
            .map(|step| matches!(step.branch, RepeatEditBranch::Play { .. }))
            .collect::<Vec<_>>(),
        vec![outer_play, !outer_play],
        "retain the explicit mixed authoring path",
    );
    assert!(target.also.is_empty());
    assert!(
        target.semantic_framing(workspace).is_err(),
        "mixed duration must never grant generic one-play framing authority"
    );
    let representative = target
        .presentation
        .as_ref()
        .expect("actual live representative");
    assert_eq!(
        state.presentation(workspace).unwrap().unwrap().frames,
        19..21
    );
    assert!(
        workspace
            .plan
            .picture(cursor)
            .unwrap()
            .framing
            .iter()
            .any(|layer| &layer.instance == representative)
    );
    assert_eq!(workspace.plan.duration().frames(), 45);

    // Existing public value editing executes the required branch isolation.
    // Restore the temporary label before invoking the raw duration reducer.
    // This is an independent positive control, not a substitute for a new
    // atomic temporal wrapper.
    let edit = ScopedNodeEdit::Rename {
        label: "Mixed isolation control".into(),
    };
    let required = workspace
        .document
        .scoped_edit_requirements(&target.target, &edit)
        .unwrap();
    assert_eq!(required.nodes, if outer_play { 9 } else { 4 });
    assert_eq!(required.marks, 0);
    let prepared = prepare_scoped_edit(
        &workspace.document,
        &CommandRequest {
            project_id: workspace.document.project_id().clone(),
            expected_revision: workspace.document.revision_id().clone(),
            new_revision: revision("mixed-isolation-control"),
            command: Command::EditScoped {
                target: target.target.clone(),
                edit,
                identities: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|index| node(&format!("mixed-copy-{index}")))
                        .collect(),
                    marks: vec![],
                },
            },
        },
    )
    .unwrap();
    let mapped = prepared
        .map_instance(&workspace.document, representative)
        .unwrap();
    prepared.target.validate(&prepared.document).unwrap();
    assert!(
        prepared
            .target
            .matches_instance(&prepared.document, &mapped)
            .unwrap()
    );
    assert_eq!(
        prepared
            .target
            .repeats
            .iter()
            .map(|step| matches!(step.branch, RepeatEditBranch::Play { .. }))
            .collect::<Vec<_>>(),
        vec![outer_play, !outer_play],
    );
    assert_ne!(prepared.target.node, target.target.node);
    assert_eq!(
        RenderPlan::compile(&prepared.document).unwrap().duration(),
        workspace.plan.duration()
    );
    let isolated = edited(
        &prepared.document,
        Command::Rename {
            node: prepared.target.node.clone(),
            label: workspace.document.nodes()[&target.target.node]
                .label
                .clone(),
        },
        "mixed-restore-label",
    );
    let changed = edited(
        &isolated,
        Command::SetHoldDuration {
            node: prepared.target.node.clone(),
            duration: frames(3),
        },
        "mixed-raw-duration",
    );
    changed.validate().unwrap();
    let changed_plan = RenderPlan::compile(&changed).unwrap();
    assert_eq!(
        changed_plan.duration().frames(),
        if outer_play { 46 } else { 47 }
    );
    let mut expected_nodes = isolated.nodes().clone();
    let NodeKind::Hold { recipe } =
        &mut expected_nodes.get_mut(&prepared.target.node).unwrap().kind
    else {
        panic!("mapped Hold");
    };
    recipe.duration = frames(3);
    assert_eq!(
        changed.nodes(),
        &expected_nodes,
        "raw duration changes only the isolated Hold recipe"
    );
    assert_eq!(
        changed.overrides(),
        isolated.overrides(),
        "duration retains all explicit branches"
    );
    for name in ["shared-pause", "inner-pause", "outer-owned-two-pause"] {
        assert_eq!(
            changed.nodes()[&node(name)],
            workspace.document.nodes()[&node(name)],
            "original shared and owned Hold recipes remain untouched"
        );
    }
    let new_start = if outer_play { 19 } else { 20 };
    assert!(
        changed_plan
            .picture(ProjectFrame(new_start))
            .unwrap()
            .framing
            .iter()
            .any(|layer| layer.instance == mapped)
    );
    let mapped_frames = (0..changed_plan.duration().frames())
        .filter(|frame| {
            changed_plan
                .picture(ProjectFrame(*frame))
                .unwrap()
                .framing
                .iter()
                .any(|layer| layer.instance.node == prepared.target.node)
        })
        .count();
    assert_eq!(
        mapped_frames,
        if outer_play { 3 } else { 6 },
        "one resized use inside an owned outer play, or two through the shared outer default"
    );
    // Today's unchanged native entrypoint returns None. All controls above must
    // pass before this intended positive requirement counts as a red baseline.
    assert_eq!(
        state.hold_parameter(workspace).as_deref(),
        Some("hold-duration 2f"),
        "the explicit mixed Hold target must offer its authored length",
    );
    service_checks(&service, &opened, &target, &mut state, outer_play);
    command(&service, ProjectRequest::Close);
}

#[test]
fn native_outer_play_inner_default_hold_duration_prefills_mixed_length() {
    exercise(true);
}

#[test]
fn native_outer_default_inner_play_hold_duration_prefills_mixed_length() {
    exercise(false);
}

fn service_checks(
    service: &ProjectService,
    opened: &ProjectUpdate,
    target: &crate::project::scoped::Target,
    state: &mut State,
    outer_play: bool,
) {
    use crate::project::macros::{Id, Operation};
    use deadpan_core::{PauseLength, SemanticContext, SemanticInstruction, SemanticProgram};
    use std::num::NonZeroU32;
    let before = opened.workspace.as_ref().unwrap();
    let path = before.path.clone();
    let id = |update: &ProjectUpdate, request| {
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
        shared_hold_duration: None,
        selected_hold_duration: None,
        mixed_hold_duration: Some(Box::new(
            target.semantic_mixed_hold_duration(before).unwrap(),
        )),
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
    let initial_rows = rows();
    let initial_bank = bank();
    let no_op = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(opened, 180),
            instruction: length(2),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    assert!(no_op.macros.as_ref().unwrap().result.is_err());
    assert_eq!(
        rows(),
        initial_rows,
        "no-op cannot publish clones or history"
    );
    assert_eq!(bank(), initial_bank);
    assert_eq!(state.target(before, target.cursor).unwrap(), *target);
    let mut absent_picture = context.clone();
    absent_picture
        .mixed_hold_duration
        .as_mut()
        .unwrap()
        .presentation = None;
    let forged = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(opened, 181),
            instruction: length(3),
            repeat_version: None,
            scope: target.scope.clone(),
            context: absent_picture,
        }),
    );
    assert!(forged.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(), initial_rows);

    let changed = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(opened, 182),
            instruction: length(3),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    let receipt = changed.macros.as_ref().unwrap().result.as_ref().unwrap();
    let commit = receipt.committed().unwrap();
    let scoped = commit.scoped.as_ref().unwrap();
    let after = changed.workspace.as_ref().unwrap();
    assert_eq!(
        rows(),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 1)
    );
    assert_eq!(bank(), initial_bank);
    assert_eq!(
        after.plan.duration().frames(),
        if outer_play { 46 } else { 47 }
    );
    assert_eq!(
        after.document.nodes().len(),
        before.document.nodes().len() + if outer_play { 9 } else { 4 }
    );
    assert_eq!(scoped.before, *target);
    assert_ne!(scoped.target.node, target.target.node);
    assert_eq!(
        scoped
            .target
            .repeats
            .iter()
            .map(|step| matches!(step.branch, RepeatEditBranch::Play { .. }))
            .collect::<Vec<_>>(),
        vec![outer_play, !outer_play]
    );
    let continuation = ProjectFrame(if outer_play { 19 } else { 20 });
    assert_eq!(commit.cursor, Some(continuation));
    assert_eq!(commit.selected_node.as_ref(), Some(&target.root));
    assert!(state.reconcile(after, scoped, commit.cursor).unwrap());
    assert_eq!(
        state.hold_parameter(after).as_deref(),
        Some("hold-duration 3f")
    );
    let mapped_target = state.target(after, continuation).unwrap();
    assert_eq!(mapped_target.target, scoped.target);
    assert_eq!(mapped_target.presentation, scoped.presentation);
    mapped_target.semantic_mixed_hold_duration(after).unwrap();
    assert!(
        changed.semantic.as_ref().unwrap().edit.is_some(),
        "direct Apply seeds dot"
    );

    let current_rows = rows();
    let stale = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(opened, 183),
            instruction: length(4),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    assert!(stale.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(), current_rows);
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
    assert_eq!(
        redone.workspace.as_ref().unwrap().document.overrides(),
        after.document.overrides()
    );
    command(service, ProjectRequest::Close);
    let reopened = command(service, ProjectRequest::Open(path.clone()));
    assert_eq!(
        *reopened.workspace.as_ref().unwrap().document,
        *redone.workspace.as_ref().unwrap().document
    );

    let mut current_context = context;
    current_context.cursor = continuation;
    let authority = current_context.mixed_hold_duration.as_mut().unwrap();
    authority.target = scoped.target.clone();
    authority.presentation = scoped.presentation.clone();
    let saved = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: id(&reopened, 184),
            register: 'z',
            program: Arc::new(
                SemanticProgram::new(vec![length(4), SemanticInstruction::Split]).unwrap(),
            ),
        }),
    );
    saved.macros.as_ref().unwrap().result.as_ref().unwrap();
    let failure_rows = rows();
    let failure_bank = bank();
    let failed = command(
        service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&saved, 185),
            register: 'z',
            count: 1,
            scope: target.scope.clone(),
            context: current_context.clone(),
        }),
    );
    assert!(failed.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(), failure_rows);
    assert_eq!(bank(), failure_bank);
    assert_eq!(
        *failed.workspace.as_ref().unwrap().document,
        *reopened.workspace.as_ref().unwrap().document
    );

    let inner = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: id(&failed, 186),
            register: 'x',
            program: Arc::new(SemanticProgram::new(vec![length(4)]).unwrap()),
        }),
    );
    inner.macros.as_ref().unwrap().result.as_ref().unwrap();
    let outer = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: id(&inner, 187),
            register: 'y',
            program: Arc::new(
                SemanticProgram::new(vec![
                    SemanticInstruction::Call {
                        register: deadpan_core::RegisterName::new('x').unwrap(),
                        count: NonZeroU32::new(1).unwrap(),
                    },
                    length(5),
                ])
                .unwrap(),
            ),
        }),
    );
    outer.macros.as_ref().unwrap().result.as_ref().unwrap();
    let call_rows = rows();
    let call_bank = bank();
    let called = command(
        service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&outer, 188),
            register: 'y',
            count: 2,
            scope: target.scope.clone(),
            context: current_context.clone(),
        }),
    );
    let call_receipt = called.macros.as_ref().unwrap().result.as_ref().unwrap();
    assert_eq!(rows(), (call_rows.0 + 1, call_rows.1 + 1, call_rows.2 + 4));
    assert_eq!(bank(), call_bank);
    let called_workspace = called.workspace.as_ref().unwrap();
    assert_eq!(
        called_workspace.plan.duration().frames(),
        if outer_play { 48 } else { 51 }
    );
    assert_eq!(
        called_workspace.document.nodes().len(),
        after.document.nodes().len(),
        "owned mixed target never reclones"
    );
    assert_eq!(
        call_receipt.committed().unwrap().cursor,
        Some(ProjectFrame(if outer_play { 19 } else { 22 }))
    );
    assert!(
        called.semantic.as_ref().unwrap().edit.is_none(),
        "named Run does not seed dot"
    );

    // A real qualified independent sound keeps the native temporal sound gate.
    let NodeKind::Hold { recipe } = &called_workspace.document.nodes()[&target.target.node].kind
    else {
        panic!("original Hold");
    };
    let HoldAudio::RoomTone { source } = &recipe.audio else {
        panic!("qualified room tone");
    };
    let sound = deadpan_core::SoundEvent {
        owner: called_workspace.document.root().clone(),
        label: "Independent sound guard".into(),
        source: source.clone(),
        mapping: deadpan_core::SourceAudioMapping::natural_rate(
            source.span,
            called_workspace.document.presentation_basis().frame_rate,
        )
        .unwrap(),
        offset: deadpan_core::AudioSample(0),
        treatments: Default::default(),
        start_edge: deadpan_core::AudioEdgePolicy::Automatic,
        end_edge: deadpan_core::AudioEdgePolicy::Automatic,
        overflow: deadpan_core::SoundOverflowPolicy::Reject,
    };
    command(service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::SetSound {
            id: deadpan_core::SoundId::new("mixed-sound").unwrap(),
            event: sound,
        },
        "mixed-sound-guard",
    );
    drop(store);
    let sounding = command(service, ProjectRequest::Open(path.clone()));
    assert!(sounding.error.is_none(), "{:?}", sounding.error);
    let sounding_workspace = sounding.workspace.as_ref().unwrap();
    let mut sounding_target = mapped_target;
    sounding_target.session = sounding_workspace.session;
    sounding_target.revision = sounding_workspace.document.revision_id().clone();
    sounding_target.cursor = ProjectFrame(if outer_play { 19 } else { 22 });
    sounding_target.validate(sounding_workspace).unwrap();
    assert!(
        sounding_target
            .semantic_mixed_hold_duration(sounding_workspace)
            .is_err()
    );
    current_context.cursor = sounding_target.cursor;
    let sound_rows = rows();
    let sound_bank = bank();
    let refused = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(&sounding, 189),
            instruction: length(6),
            repeat_version: None,
            scope: target.scope.clone(),
            context: current_context,
        }),
    );
    assert!(refused.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(), sound_rows);
    assert_eq!(bank(), sound_bank);
}

#[test]
fn mixed_duration_hints_offer_owned_gaps_without_changing_all_play_gap_support() {
    let document = fixture(
        "gap-outer",
        vec![
            repeat("gap-outer", "gap-inner", 2, None),
            repeat("gap-inner", "gap-body", 2, Some(1)),
            hold("gap-body", 2),
            hold("owned-gap", 3),
        ],
        BTreeMap::new(),
        [overrides("gap-inner", &[(0, "owned-gap")])].into(),
    );
    let mut mixed = state(document.clone(), "gap-outer", 2);
    assert!(mixed.enter_in().unwrap());
    mixed.switch_play_in(1).unwrap();
    mixed.select_in(1).unwrap();
    assert_eq!(mixed.selected.node, node("owned-gap"));
    assert_eq!(
        mixed.hold_parameter_in().as_deref(),
        Some("hold-duration 3f")
    );

    let mut all_play = state(document, "gap-outer", 2);
    all_play.switch_play_in(1).unwrap();
    assert!(all_play.enter_in().unwrap());
    all_play.switch_play_in(1).unwrap();
    all_play.select_in(1).unwrap();
    assert_eq!(
        all_play.hold_parameter_in().as_deref(),
        Some("hold-duration 3f")
    );
}

#[test]
fn mixed_duration_hint_refuses_retime_ancestry() {
    let document = fixture(
        "retime-outer",
        vec![
            repeat("retime-outer", "retime-window", 2, None),
            retime("retime-window", "retime-inner", 0, 4, 4),
            repeat("retime-inner", "retime-pause", 2, None),
            hold("retime-pause", 2),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut inspector = state(document, "retime-outer", 0);
    assert!(inspector.enter_in().unwrap());
    assert!(inspector.enter_in().unwrap());
    inspector.switch_play_in(1).unwrap();
    assert!(inspector.hold_parameter_in().is_none());
}
