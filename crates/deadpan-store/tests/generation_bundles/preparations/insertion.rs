use super::*;
use deadpan_core::{
    AudioTimingId, ProjectFrame, RepeatEditBranch, RepeatEditStep, ScopedNodeTarget,
    SplitIdentities,
};

fn create(package: &Path) -> Result<ProjectStore> {
    let mut store = ProjectStore::create(package, &document()?)?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    Ok(store)
}

fn insert(
    document: &ProjectDocument,
    name: &str,
    at: i64,
    frames: i64,
    allocation: &str,
) -> Result<Command> {
    let target = document.insert_time_target(ProjectFrame(at))?;
    Ok(Command::InsertAiTime {
        at: ProjectFrame(at),
        hold: HoldRecipe {
            duration: FrameDuration::new(frames)?,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
        id: NodeId::new(name)?,
        identities: SplitIdentities {
            nodes: (0..target.split.map_or(0, |split| split.required_ids))
                .map(|index| NodeId::new(format!("{name}-split-{index}")))
                .collect::<std::result::Result<_, _>>()?,
        },
        timing: AudioTimingId {
            allocation: RevisionId::new(allocation)?,
            ordinal: 0,
        },
    })
}

fn commit_insert(store: &mut ProjectStore) -> Result<StoredGenerationPreparation> {
    let request = command(
        store,
        "insert-ai",
        insert(&store.snapshot()?, "ai-hold", 12, 18, "insert-ai")?,
    )?;
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    Ok(store
        .generation_preparation(&outcome.generation_preparations[0])?
        .unwrap())
}

#[test]
fn inserted_ai_pause_is_durable_and_usable_without_a_model() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("insert.deadpan");
    let mut store = create(&package)?;
    let before = store.snapshot()?;
    let prepared = commit_insert(&mut store)?;
    assert_eq!(prepared.origin, PreparationOrigin::inserted_pause());
    assert!(prepared.origin.accepted_artifact().is_none());
    assert_eq!(prepared.duration.frames(), 18);
    assert_eq!(prepared.target.node, NodeId::new("ai-hold")?);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), before.duration()?.frames() + 18);
    assert_eq!(
        after.nodes()[&NodeId::new("hold")?],
        before.nodes()[&NodeId::new("hold")?]
    );
    assert!(matches!(&after.nodes()[&prepared.target.node].kind,
        NodeKind::Hold { recipe } if recipe.audio == HoldAudio::Silence && recipe.video == HoldVideo::Background));
    let claim = store.claim_generation_preparation(&prepared.id, &store.head_revision()?)?;
    assert!(store.finish_generation_preparation(
        &claim,
        PreparationFailure::Unavailable("No model installed.".into())
    )?);
    assert_eq!(store.snapshot()?, after);
    store.validate_full()?;
    drop(store);
    let readonly = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    readonly.validate_full()?;
    let saved = readonly.generation_preparation(&prepared.id)?.unwrap();
    assert_eq!(saved.state, PreparationState::Unavailable);
    assert_eq!(saved.origin, prepared.origin);
    assert_eq!(readonly.snapshot()?, after);
    drop(readonly);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let retry = store.retry_generation_preparation(&prepared.id, &store.head_revision()?)?;
    assert_eq!(retry.origin, prepared.origin);
    assert_eq!(retry.state, PreparationState::Queued);
    assert_eq!(store.snapshot()?, after);
    Ok(())
}

#[test]
fn inserted_ai_pause_undo_redo_has_fresh_intent_and_saved_controls() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = create(&scratch.path().join("history.deadpan"))?;
    let original = commit_insert(&mut store)?;
    let claim = store.claim_generation_preparation(&original.id, &store.head_revision()?)?;
    store.undo(&store.head_revision()?, RevisionId::new("undo-ai")?)?;
    assert!(
        !store
            .snapshot()?
            .nodes()
            .contains_key(&original.target.node)
    );
    assert!(!store.generation_preparation_claim_is_current(&claim)?);
    assert_eq!(
        store.generation_preparation(&original.id)?.unwrap().state,
        PreparationState::Cancelled
    );
    let redo = store.redo(&store.head_revision()?, RevisionId::new("redo-ai")?)?;
    assert_eq!(redo.generation_preparations.len(), 1);
    let fresh = store
        .generation_preparation(&redo.generation_preparations[0])?
        .unwrap();
    assert_ne!(fresh.id, original.id);
    assert_eq!(fresh.origin, original.origin);
    assert_eq!(fresh.target, original.target);
    assert_eq!(fresh.duration, original.duration);
    assert_eq!(fresh.state, PreparationState::Queued);
    store.validate_full()?;
    Ok(())
}

#[test]
fn inserted_ai_pause_fulfilment_binds_controls_and_rejects_stale_claims() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = create(&scratch.path().join("claim.deadpan"))?;
    let original = commit_insert(&mut store)?;
    let claim = store.claim_generation_preparation(&original.id, &store.head_revision()?)?;
    let (mut input, plan, attempt) = replacement_input(&store, "insert-candidate", 18)?;
    input.hold_id = original.target.node.clone();
    let mut wrong = input.clone();
    wrong.constraints.motion = MotionAmount::Subtle;
    assert!(
        store
            .fulfil_generation_preparation(&claim, wrong, plan.clone(), attempt.clone())
            .is_err()
    );
    assert!(store.generation_request(&input.request_id)?.is_none());
    assert!(store.generation_preparation_claim_is_current(&claim)?);
    store.commit(&command(
        &store,
        "rename",
        Command::Rename {
            node: original.target.node.clone(),
            label: "My pause".into(),
        },
    )?)?;
    assert!(
        store
            .fulfil_generation_preparation(&claim, input.clone(), plan.clone(), attempt.clone())
            .is_err()
    );
    assert!(store.generation_request(&input.request_id)?.is_none());
    let fresh = store.claim_generation_preparation(&original.id, &store.head_revision()?)?;
    input.expected_revision = store.head_revision()?;
    let before = store.snapshot()?;
    let (request, attempt) = store.fulfil_generation_preparation(&fresh, input, plan, attempt)?;
    assert_eq!(request.target, original.target);
    assert_eq!(attempt.checkpoint.state, JobState::Queued);
    assert_eq!(store.snapshot()?, before);
    assert_eq!(
        store.generation_preparation(&original.id)?.unwrap().state,
        PreparationState::Fulfilled
    );
    store.validate_full()?;
    Ok(())
}

#[test]
fn compound_ai_pause_keeps_exact_identity_final_duration_and_repeat_ancestry() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = create(&scratch.path().join("compound.deadpan"))?;
    let snapshot = store.snapshot()?;
    let request = compound(
        &store,
        vec![
            insert(&snapshot, "ai-hold", 12, 18, "leaf-0")?,
            Command::SetHoldDuration {
                node: NodeId::new("ai-hold")?,
                duration: FrameDuration::new(27)?,
            },
            Command::WrapRepeat {
                node: NodeId::new("ai-hold")?,
                id: NodeId::new("repeat")?,
                plays: 3,
                gap: None,
                anchor_policy: Default::default(),
            },
            Command::Rename {
                node: NodeId::new("hold")?,
                label: "Selection can end elsewhere".into(),
            },
        ],
    )?;
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    let preparation = store
        .generation_preparation(&outcome.generation_preparations[0])?
        .unwrap();
    assert_eq!(preparation.duration.frames(), 27);
    assert_eq!(
        preparation.target,
        ScopedNodeTarget {
            node: NodeId::new("ai-hold")?,
            repeats: vec![RepeatEditStep {
                repeat: NodeId::new("repeat")?,
                branch: RepeatEditBranch::Default,
            }],
        }
    );
    assert_eq!(preparation.origin_target, preparation.target);
    assert_eq!(preparation.origin_revision, outcome.revision_id);
    store.validate_full()?;
    store.undo(&store.head_revision()?, RevisionId::new("undo")?)?;
    let redo = store.redo(&store.head_revision()?, RevisionId::new("redo")?)?;
    let fresh = store
        .generation_preparation(&redo.generation_preparations[0])?
        .unwrap();
    assert_eq!(fresh.target, preparation.target);
    assert_eq!(fresh.origin, preparation.origin);
    store.validate_full()?;
    Ok(())
}

#[test]
fn compound_ai_pause_failure_rolls_back_document_history_and_queue() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("atomic.deadpan");
    let mut store = create(&package)?;
    let before = store.snapshot()?;
    let request = compound(
        &store,
        vec![
            insert(&before, "ai-hold", 12, 18, "leaf-0")?,
            Command::Rename {
                node: NodeId::new("missing")?,
                label: "Fails after insertion".into(),
            },
        ],
    )?;
    assert!(store.commit(&request).is_err());
    assert_eq!(store.snapshot()?, before);
    assert!(store.generation_preparations(None, 256)?.is_empty());
    // Failure while inserting the durable preparation must also roll back
    // the already staged document/history writes in the same transaction.
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_ai_preparation BEFORE INSERT ON generation_preparations BEGIN SELECT RAISE(ABORT,'injected preparation failure'); END;")?;
    assert!(commit_insert(&mut store).is_err());
    assert_eq!(store.snapshot()?, before);
    assert_eq!(
        database.query_row("SELECT count(*) FROM history", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    assert!(store.generation_preparations(None, 256)?.is_empty());
    database.execute_batch("DROP TRIGGER fail_ai_preparation")?;
    commit_insert(&mut store)?;
    store.validate_full()?;
    Ok(())
}

#[test]
fn explicit_same_fallback_supersedes_inserted_ai_intent_in_and_after_compounds() -> Result {
    for same_transaction in [false, true] {
        let scratch = tempfile::tempdir()?;
        let mut store = create(&scratch.path().join("cancel.deadpan"))?;
        let fallback = Command::SetHoldProvider {
            node: NodeId::new("ai-hold")?,
            video: HoldVideo::Background,
        };
        if same_transaction {
            let request = compound(
                &store,
                vec![
                    insert(&store.snapshot()?, "ai-hold", 12, 18, "leaf-0")?,
                    fallback,
                ],
            )?;
            assert!(store.commit(&request)?.generation_preparations.is_empty());
        } else {
            let original = commit_insert(&mut store)?;
            let claim =
                store.claim_generation_preparation(&original.id, &store.head_revision()?)?;
            store.commit(&command(&store, "fallback", fallback)?)?;
            assert!(!store.generation_preparation_claim_is_current(&claim)?);
            assert_eq!(
                store.generation_preparation(&original.id)?.unwrap().state,
                PreparationState::Cancelled
            );
        }
        assert!(store.generation_preparations(None, 256)?.is_empty());
        assert_eq!(store.snapshot()?.duration()?.frames(), 30);
        store.validate_full()?;
    }
    Ok(())
}

#[test]
fn inserted_ai_origin_controls_and_presence_cannot_be_laundered_by_an_edit() -> Result {
    for missing in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("tampered.deadpan");
        let mut store = create(&package)?;
        let mut value = commit_insert(&mut store)?;
        let before = store.snapshot()?;
        let database = Connection::open(package.join("project.sqlite"))?;
        if missing {
            database.execute("DELETE FROM generation_preparations", [])?;
        } else {
            let PreparationOrigin::InsertedPause { options } = &mut value.origin else {
                panic!()
            };
            options.motion = MotionAmount::Moderate;
            database.execute(
                "UPDATE generation_preparations SET record=?1",
                [serde_json::to_string(&value)?],
            )?;
        }
        assert!(store.validate_full().is_err());
        assert!(
            store
                .commit(&command(
                    &store,
                    "rename",
                    Command::Rename {
                        node: value.target.node,
                        label: "Must not hide tampering".into(),
                    }
                )?)
                .is_err()
        );
        assert_eq!(store.snapshot()?, before);
    }
    Ok(())
}

#[test]
fn compound_ai_insertions_do_not_fan_out_to_isolated_or_copied_holds() -> Result {
    use deadpan_core::{OccurrenceIdentities, ScopedNodeEdit};
    fn stage(
        document: &mut ProjectDocument,
        commands: &mut Vec<Command>,
        command: Command,
    ) -> Result {
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(format!("leaf-{}", commands.len()))?,
            command: command.clone(),
        };
        *document = deadpan_core::apply_with_result(document, &request)?.1;
        commands.push(command);
        Ok(())
    }
    let scratch = tempfile::tempdir()?;
    let mut store = create(&scratch.path().join("isolation.deadpan"))?;
    let mut staged = store.snapshot()?;
    let mut commands = Vec::new();
    let first = insert(&staged, "first-ai", 12, 18, "leaf-0")?;
    stage(&mut staged, &mut commands, first)?;
    let second = insert(&staged, "second-ai", 30, 9, "leaf-1")?;
    stage(&mut staged, &mut commands, second)?;
    stage(
        &mut staged,
        &mut commands,
        Command::WrapRepeat {
            node: NodeId::new("first-ai")?,
            id: NodeId::new("repeat")?,
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
    )?;
    let NodeKind::Repeat { iterations, .. } = &staged.nodes()[&NodeId::new("repeat")?].kind else {
        panic!()
    };
    let target = ScopedNodeTarget {
        node: NodeId::new("first-ai")?,
        repeats: vec![RepeatEditStep {
            repeat: NodeId::new("repeat")?,
            branch: RepeatEditBranch::Play {
                iteration: iterations.at(1).unwrap(),
            },
        }],
    };
    let edit = ScopedNodeEdit::Rename {
        label: "Only play two".into(),
    };
    let required = staged.scoped_edit_requirements(&target, &edit)?;
    stage(
        &mut staged,
        &mut commands,
        Command::EditScoped {
            target,
            edit,
            identities: OccurrenceIdentities {
                nodes: (0..required.nodes)
                    .map(|n| NodeId::new(format!("isolated-{n}")))
                    .collect::<std::result::Result<_, _>>()?,
                marks: (0..required.marks)
                    .map(|n| deadpan_core::MarkId::new(format!("isolated-mark-{n}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
        },
    )?;
    let copy = NodeId::new("copied-hold")?;
    let copied = staged.nodes()[&NodeId::new("second-ai")?].clone();
    stage(
        &mut staged,
        &mut commands,
        Command::Insert {
            parent: NodeId::new("root")?,
            index: 0,
            subtree: Subtree {
                root: copy.clone(),
                nodes: BTreeMap::from([(copy, copied)]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?;
    let outcome = store.commit(&compound(&store, commands)?)?;
    assert_eq!(outcome.generation_preparations.len(), 2);
    let records = store.generation_preparations(None, 256)?;
    assert_eq!(records.len(), 2);
    let first = records
        .iter()
        .find(|record| record.target.node == NodeId::new("first-ai").unwrap())
        .unwrap();
    assert_eq!(
        first.target.repeats,
        vec![RepeatEditStep {
            repeat: NodeId::new("repeat")?,
            branch: RepeatEditBranch::Default,
        }]
    );
    assert_eq!(first.duration.frames(), 18);
    assert!(records.iter().any(
        |record| record.target.node == NodeId::new("second-ai").unwrap()
            && record.duration.frames() == 9
    ));
    assert_eq!(store.snapshot()?.nodes(), staged.nodes());
    assert_eq!(store.snapshot()?.duration()?, staged.duration()?);
    store.validate_full()?;
    Ok(())
}
