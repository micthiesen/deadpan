//! Reusable requests follow proven Repeat isolation without changing worker origins.
use super::*;
use deadpan_core::{
    IterationOrder, NodeKind, OccurrenceIdentities, RepeatEditBranch, RepeatEditStep,
    ScopedNodeEdit, ScopedNodeTarget,
};
use deadpan_jobs::{
    AxisLimits, BridgeCapability, BridgeGenerationPlan, DimensionLimits, FrameCountFormula,
    NativeDimensions,
};

fn fixture() -> Result<ProjectDocument> {
    let before = document(&[])?;
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: NodeId::new("body")?,
        iterations: IterationOrder::new(RevisionId::new("plays")?, 3)?,
        gap: None,
        escalation: None,
    };
    let hold = |label| {
        BeatNode::hold(
            label,
            HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(12).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        )
    };
    let command = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("fixture")?,
        command: Command::Insert {
            parent: before.root().clone(),
            index: 0,
            subtree: Subtree {
                root: NodeId::new("repeat")?,
                nodes: BTreeMap::from([
                    (NodeId::new("repeat")?, repeat),
                    (
                        NodeId::new("body")?,
                        BeatNode::sequence(
                            "Body",
                            vec![NodeId::new("hold")?, NodeId::new("sibling")?],
                        ),
                    ),
                    (NodeId::new("hold")?, hold("Pause")),
                    (NodeId::new("sibling")?, hold("Neighbour")),
                ]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    };
    Ok(deadpan_core::apply(&before, &command)?
        .forward
        .apply(&before)?)
}

fn target(play: Option<u32>) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: NodeId::new("hold").unwrap(),
        repeats: vec![RepeatEditStep {
            repeat: NodeId::new("repeat").unwrap(),
            branch: play.map_or(RepeatEditBranch::Default, |ordinal| {
                RepeatEditBranch::Play {
                    iteration: deadpan_core::IterationId {
                        allocation: RevisionId::new("fixture").unwrap(),
                        ordinal,
                    },
                }
            }),
        }],
    }
}

fn scoped_request(
    store: &mut ProjectStore,
    id: &str,
    target: ScopedNodeTarget,
) -> Result<StoredGenerationRequest> {
    let plan = BridgeGenerationPlan::new(
        FrameDuration::new(12)?,
        rate(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1)?,
            FrameCountFormula::new(1, 0, 2, 97)?,
            DimensionLimits::new(AxisLimits::new(512, 512, 1)?, AxisLimits::new(320, 320, 1)?),
        ),
        NativeDimensions::new(512, 320)?,
    )?;
    Ok(store.record_scoped_bridge_generation_request(
        GenerationRequestInput {
            request_id: RequestId::new(id)?,
            expected_revision: store.head_revision()?,
            hold_id: target.node.clone(),
            context_sha256: hash('a'),
            constraints: constraints(12),
            provider: provider(1),
        },
        target,
        plan,
    )?)
}

fn isolate_sibling(store: &ProjectStore) -> Result<CommandRequest> {
    let target = ScopedNodeTarget {
        node: NodeId::new("sibling")?,
        ..target(Some(1))
    };
    let operation = ScopedNodeEdit::Rename {
        label: "Only the second play".into(),
    };
    let required = store
        .snapshot()?
        .scoped_edit_requirements(&target, &operation)?;
    edit(
        store,
        "isolate",
        Command::EditScoped {
            target,
            edit: operation,
            identities: OccurrenceIdentities {
                nodes: (0..required.nodes)
                    .map(|i| NodeId::new(format!("clone-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
                marks: (0..required.marks)
                    .map(|i| deadpan_core::MarkId::new(format!("clone-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
        },
    )
}
fn unchanged(requests: Vec<StoredGenerationRequest>) -> Vec<RelevanceObservation> {
    requests
        .iter()
        .map(|r| {
            observe(
                r,
                ContextObservation::Resolved(r.binding.context_sha256.clone()),
            )
        })
        .collect()
}

#[test]
fn sibling_isolation_maps_only_its_play_and_new_requests_follow_undo_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("scope.deadpan");
    let mut store = ProjectStore::create(&path, &fixture()?)?;
    let default = scoped_request(&mut store, "default", target(None))?;
    let first = scoped_request(&mut store, "first", target(Some(0)))?;
    let second = scoped_request(&mut store, "second", target(Some(1)))?;
    let command = isolate_sibling(&store)?;
    let (_, mapped) = store.preview_generation_contexts(&command)?;
    assert_eq!(
        store.generation_request(&second.request_id)?.unwrap(),
        second
    );
    store.commit_reconciled(&command, &plan(&command, unchanged(mapped)))?;
    let isolated = store.generation_request(&second.request_id)?.unwrap();
    assert_ne!(isolated.target.node, second.target.node);
    assert_eq!(isolated.binding, second.binding);
    assert_eq!(isolated.origin_target, second.origin_target);
    for original in [&default, &first] {
        assert_eq!(
            store.generation_request(&original.request_id)?.unwrap(),
            *original
        );
    }
    let newer = scoped_request(&mut store, "second-new", isolated.target.clone())?;
    assert_eq!(newer.scope_id, second.scope_id);
    assert_eq!(newer.binding.request_version.get(), 2);
    let undo = RevisionId::new("undo")?;
    let (_, mapped) =
        store.preview_undo_generation_contexts(&command.new_revision, undo.clone())?;
    store.undo_reconciled(
        &command.new_revision,
        undo.clone(),
        &RelevancePlan {
            from_revision: command.new_revision.clone(),
            to_revision: undo.clone(),
            observations: unchanged(mapped),
        },
    )?;
    assert_eq!(
        store.generation_request(&newer.request_id)?.unwrap().target,
        second.target
    );
    assert_eq!(
        store
            .generation_request(&second.request_id)?
            .unwrap()
            .relevance,
        Relevance::Stale
    );
    let redo = RevisionId::new("redo")?;
    let (_, mapped) = store.preview_redo_generation_contexts(&undo, redo.clone())?;
    store.redo_reconciled(
        &undo,
        redo.clone(),
        &RelevancePlan {
            from_revision: undo.clone(),
            to_revision: redo,
            observations: unchanged(mapped),
        },
    )?;
    assert_eq!(
        store.generation_request(&newer.request_id)?.unwrap().target,
        isolated.target
    );
    assert_eq!(
        store
            .generation_request(&second.request_id)?
            .unwrap()
            .relevance,
        Relevance::Stale
    );
    store.validate_full()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate_full()?;
    Ok(())
}

#[test]
fn wrong_target_observation_rolls_back_isolation_and_all_scope_addresses() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("atomic.deadpan"), &fixture()?)?;
    let request = scoped_request(&mut store, "second", target(Some(1)))?;
    let before = store.snapshot()?;
    let command = isolate_sibling(&store)?;
    let stale_address = plan(&command, unchanged(vec![request.clone()]));
    assert!(store.commit_reconciled(&command, &stale_address).is_err());
    assert_eq!(store.snapshot()?, before);
    assert_eq!(
        store.generation_request(&request.request_id)?.unwrap(),
        request
    );
    store.validate_full()?;
    Ok(())
}

#[test]
fn valid_sibling_target_forgery_fails_receipt_and_full_validation() -> Result {
    for full in [false, true] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("tampered.deadpan");
        let mut store = ProjectStore::create(&path, &fixture()?)?;
        scoped_request(&mut store, "second", target(Some(1)))?;
        store.validate()?;
        let connection = Connection::open(path.join("project.sqlite"))?;
        connection.execute(
            "UPDATE generation_scopes SET current_target=?1 WHERE scope_id='second'",
            [serde_json::to_string(&target(Some(2)))?],
        )?;
        drop(connection);
        if full {
            assert!(matches!(
                store.validate_full(),
                Err(StoreError::Integrity(_))
            ));
        } else {
            drop(store);
            assert!(matches!(
                ProjectStore::open(&path, AccessMode::ReadOnly),
                Err(StoreError::Integrity(_))
            ));
        }
    }
    Ok(())
}

#[test]
fn forged_isolation_events_never_supply_address_authority() -> Result {
    for sql in [
        "DELETE FROM generation_scope_events WHERE revision_id='isolate'",
        "UPDATE generation_scope_events SET direction='inverse' WHERE revision_id='isolate'",
        "UPDATE generation_requests SET origin_revision='isolate' WHERE request_id='second'",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("forged-history.deadpan");
        let mut store = ProjectStore::create(&path, &fixture()?)?;
        scoped_request(&mut store, "second", target(Some(1)))?;
        let command = isolate_sibling(&store)?;
        let (_, mapped) = store.preview_generation_contexts(&command)?;
        store.commit_reconciled(&command, &plan(&command, unchanged(mapped)))?;
        store.validate_full()?;
        drop(store);
        let connection = Connection::open(path.join("project.sqlite"))?;
        connection.execute(sql, [])?;
        drop(connection);
        assert!(
            ProjectStore::open(&path, AccessMode::ReadOnly).is_err(),
            "{sql}"
        );
    }
    Ok(())
}
