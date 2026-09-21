use std::collections::BTreeMap;
use std::error::Error;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_jobs::{
    ConditioningMode, HoldConstraints, MotionAmount, ProviderPackId, ProviderPackVersion,
    ProviderSelection, Relevance, RequestId, RuntimeId, RuntimeVersion, Sha256, VideoSpec,
};
use deadpan_store::generation::{
    ContextObservation, GenerationRequestInput, RelevanceObservation, RelevancePlan,
    StoredGenerationRequest,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::{Connection, params};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn rate() -> FrameRate {
    FrameRate::new(30_000, 1_001).unwrap()
}

fn document(holds: &[(&str, i64)]) -> Result<ProjectDocument> {
    let mut document = ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    for (index, (name, frames)) in holds.iter().enumerate() {
        let node = NodeId::new(*name)?;
        let revision = RevisionId::new(format!("setup-{index}"))?;
        let edit = deadpan_core::apply(
            &document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: revision,
                command: Command::Insert {
                    parent: document.root().clone(),
                    index,
                    subtree: Subtree {
                        root: node.clone(),
                        nodes: BTreeMap::from([(
                            node,
                            BeatNode::hold(
                                "Pause",
                                HoldRecipe {
                                    duration: FrameDuration::new(*frames)?,
                                    video: HoldVideo::Background,
                                    audio: HoldAudio::Silence,
                                },
                            ),
                        )]),
                        overrides: BTreeMap::new(),
                    },
                },
            },
        )?;
        document = edit.forward.apply(&document)?;
    }
    Ok(document)
}

fn provider(seed: u64) -> ProviderSelection {
    ProviderSelection {
        pack_id: ProviderPackId::new("pack").unwrap(),
        pack_version: ProviderPackVersion::new("v1").unwrap(),
        runtime_id: RuntimeId::new("runtime").unwrap(),
        runtime_version: RuntimeVersion::new("v1").unwrap(),
        seed,
    }
}

fn constraints(frames: i64) -> HoldConstraints {
    HoldConstraints {
        video: VideoSpec::new(FrameDuration::new(frames).unwrap(), rate(), 512, 320).unwrap(),
        conditioning: ConditioningMode::Bridge,
        motion: MotionAmount::Still,
    }
}

fn hash(byte: char) -> Sha256 {
    Sha256::new(byte.to_string().repeat(64)).unwrap()
}

fn allocate(
    store: &mut ProjectStore,
    request: &str,
    hold: &str,
    frames: i64,
    context: char,
) -> Result<StoredGenerationRequest> {
    Ok(store.allocate_generation_request(GenerationRequestInput {
        request_id: RequestId::new(request)?,
        expected_revision: store.snapshot()?.revision_id().clone(),
        hold_id: NodeId::new(hold)?,
        context_sha256: hash(context),
        constraints: constraints(frames),
        provider: provider(frames as u64),
    })?)
}

fn edit(store: &ProjectStore, revision: &str, command: Command) -> Result<CommandRequest> {
    let document = store.snapshot()?;
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command,
    })
}

fn observe(request: &StoredGenerationRequest, context: ContextObservation) -> RelevanceObservation {
    RelevanceObservation {
        request_id: request.request_id.clone(),
        binding: request.binding.clone(),
        after_context: context,
    }
}

fn plan(request: &CommandRequest, observations: Vec<RelevanceObservation>) -> RelevancePlan {
    RelevancePlan {
        from_revision: request.expected_revision.clone(),
        to_revision: request.new_revision.clone(),
        observations,
    }
}

#[test]
fn allocation_is_typed_monotonic_persistent_and_preview_is_pure() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("generation.deadpan");
    let mut store = ProjectStore::create(&path, &document(&[("hold", 12)])?)?;
    let first = allocate(&mut store, "request-1", "hold", 12, 'a')?;
    assert_eq!(first.binding.request_version.get(), 1);
    assert_eq!(first.relevance, Relevance::Current);
    assert_eq!(first.constraints, constraints(12));
    assert_eq!(first.provider, provider(12));

    let rename = edit(
        &store,
        "rename",
        Command::Rename {
            node: NodeId::new("hold")?,
            label: "Renamed pause".into(),
        },
    )?;
    store.preview(&rename)?;
    assert_eq!(store.current_generation_requests()?, vec![first.clone()]);
    assert!(matches!(
        store.commit(&rename),
        Err(StoreError::GenerationRelevanceRequired)
    ));
    store.commit_reconciled(
        &rename,
        &plan(
            &rename,
            vec![observe(&first, ContextObservation::Resolved(hash('a')))],
        ),
    )?;
    assert_eq!(
        store.generation_request(&first.request_id)?,
        Some(first.clone())
    );

    let second = allocate(&mut store, "request-2", "hold", 12, 'a')?;
    assert_eq!(second.binding.request_version.get(), 2);
    assert_eq!(
        store
            .generation_request(&first.request_id)?
            .unwrap()
            .relevance,
        Relevance::Stale
    );
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let third = allocate(&mut store, "request-3", "hold", 12, 'a')?;
    assert_eq!(third.binding.request_version.get(), 3);
    assert_eq!(
        store
            .generation_request(&second.request_id)?
            .unwrap()
            .relevance,
        Relevance::Stale
    );
    store.validate()?;
    Ok(())
}

#[test]
fn allocation_rejects_read_only_stale_invalid_and_exhausted_inputs() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("reject.deadpan");
    let mut store = ProjectStore::create(&path, &document(&[("hold", 12)])?)?;
    let stale = RevisionId::new("stale")?;
    let input = |expected_revision, frames| GenerationRequestInput {
        request_id: RequestId::new(format!("request-{frames}")).unwrap(),
        expected_revision,
        hold_id: NodeId::new("hold").unwrap(),
        context_sha256: hash('a'),
        constraints: constraints(frames),
        provider: provider(1),
    };
    assert!(matches!(
        store.allocate_generation_request(input(stale, 12)),
        Err(StoreError::RevisionConflict { .. })
    ));
    assert!(matches!(
        store.allocate_generation_request(input(store.snapshot()?.revision_id().clone(), 13)),
        Err(StoreError::GenerationTarget(_))
    ));
    assert!(matches!(
        store.allocate_generation_request(GenerationRequestInput {
            request_id: RequestId::new("wrong-node")?,
            expected_revision: store.snapshot()?.revision_id().clone(),
            hold_id: NodeId::new("root")?,
            context_sha256: hash('a'),
            constraints: constraints(12),
            provider: provider(1),
        }),
        Err(StoreError::GenerationTarget(_))
    ));
    let wrong_rate = FrameRate::new(24, 1)?;
    assert!(matches!(
        store.allocate_generation_request(GenerationRequestInput {
            request_id: RequestId::new("wrong-rate")?,
            expected_revision: store.snapshot()?.revision_id().clone(),
            hold_id: NodeId::new("hold")?,
            context_sha256: hash('a'),
            constraints: HoldConstraints {
                video: VideoSpec::new(FrameDuration::new(12)?, wrong_rate, 512, 320)?,
                conditioning: ConditioningMode::Bridge,
                motion: MotionAmount::Still,
            },
            provider: provider(1),
        }),
        Err(StoreError::GenerationTarget(_))
    ));
    let current = allocate(&mut store, "current", "hold", 12, 'a')?;
    assert!(matches!(
        store.allocate_generation_request(GenerationRequestInput {
            request_id: current.request_id.clone(),
            expected_revision: store.snapshot()?.revision_id().clone(),
            hold_id: NodeId::new("hold")?,
            context_sha256: hash('b'),
            constraints: constraints(12),
            provider: provider(2),
        }),
        Err(StoreError::GenerationRequestReused(_))
    ));
    drop(store);

    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute(
        "UPDATE hold_request_clocks SET high_water=?1 WHERE hold_id='hold'",
        [i64::MAX],
    )?;
    connection.execute(
        "UPDATE generation_requests SET relevance='stale' WHERE hold_id='hold'",
        [],
    )?;
    connection.execute(
        "INSERT INTO generation_requests
         SELECT 'allocated-maximum',project_id,hold_id,?1,origin_revision,context_sha256,
                constraints,provider,'stale'
         FROM generation_requests WHERE request_id='current'",
        [i64::MAX],
    )?;
    drop(connection);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(matches!(
        allocate(&mut store, "overflow", "hold", 12, 'c'),
        Err(error) if matches!(error.downcast_ref::<StoreError>(), Some(StoreError::GenerationVersionExhausted(_)))
    ));
    drop(store);

    let mut read_only = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let revision = read_only.snapshot()?.revision_id().clone();
    assert!(matches!(
        read_only.allocate_generation_request(input(revision, 12)),
        Err(StoreError::ReadOnly)
    ));
    Ok(())
}

#[test]
fn incomplete_duplicate_and_wrong_binding_plans_are_atomic() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("plans.deadpan");
    let mut store = ProjectStore::create(&path, &document(&[("hold", 12)])?)?;
    let current = allocate(&mut store, "request", "hold", 12, 'a')?;
    let change = edit(
        &store,
        "changed",
        Command::SetHoldDuration {
            node: NodeId::new("hold")?,
            duration: FrameDuration::new(13)?,
        },
    )?;
    for invalid in [
        plan(&change, vec![]),
        plan(
            &change,
            vec![
                observe(&current, ContextObservation::Unresolved),
                observe(&current, ContextObservation::Unresolved),
            ],
        ),
        RelevancePlan {
            from_revision: RevisionId::new("wrong")?,
            to_revision: change.new_revision.clone(),
            observations: vec![observe(&current, ContextObservation::Unresolved)],
        },
        plan(
            &change,
            vec![RelevanceObservation {
                request_id: current.request_id.clone(),
                binding: deadpan_jobs::TargetBinding {
                    context_sha256: hash('b'),
                    ..current.binding.clone()
                },
                after_context: ContextObservation::Unresolved,
            }],
        ),
    ] {
        assert!(matches!(
            store.commit_reconciled(&change, &invalid),
            Err(StoreError::GenerationPlan(_))
        ));
        assert_eq!(store.snapshot()?.revision_id(), &change.expected_revision);
        assert_eq!(
            store
                .generation_request(&current.request_id)?
                .unwrap()
                .relevance,
            Relevance::Current
        );
    }

    let stale_plan = plan(
        &change,
        vec![observe(&current, ContextObservation::Unresolved)],
    );
    let newer = allocate(&mut store, "newer-request", "hold", 12, 'a')?;
    assert!(matches!(
        store.commit_reconciled(&change, &stale_plan),
        Err(StoreError::GenerationPlan(_))
    ));
    assert_eq!(store.current_generation_requests()?, vec![newer.clone()]);
    assert_eq!(store.snapshot()?.revision_id(), &change.expected_revision);
    Ok(())
}

#[test]
fn relevant_edits_isolate_holds_and_undo_redo_never_revive_requests() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("history.deadpan");
    let mut store = ProjectStore::create(&path, &document(&[("a", 12), ("b", 8)])?)?;
    let a1 = allocate(&mut store, "a-1", "a", 12, 'a')?;
    let b1 = allocate(&mut store, "b-1", "b", 8, 'b')?;
    let grow = edit(
        &store,
        "grow-a",
        Command::SetHoldDuration {
            node: NodeId::new("a")?,
            duration: FrameDuration::new(13)?,
        },
    )?;
    store.commit_reconciled(
        &grow,
        &plan(
            &grow,
            vec![
                observe(&a1, ContextObservation::Resolved(hash('a'))),
                observe(&b1, ContextObservation::Resolved(hash('b'))),
            ],
        ),
    )?;
    assert_eq!(
        store.generation_request(&a1.request_id)?.unwrap().relevance,
        Relevance::Stale
    );
    assert_eq!(
        store.generation_request(&b1.request_id)?.unwrap().relevance,
        Relevance::Current
    );

    let a2 = allocate(&mut store, "a-2", "a", 13, 'c')?;
    let undo_revision = RevisionId::new("undo-grow")?;
    let undo = RelevancePlan {
        from_revision: grow.new_revision.clone(),
        to_revision: undo_revision.clone(),
        observations: vec![
            observe(&a2, ContextObservation::Resolved(hash('a'))),
            observe(&b1, ContextObservation::Resolved(hash('b'))),
        ],
    };
    store.undo_reconciled(&grow.new_revision, undo_revision.clone(), &undo)?;
    assert_eq!(
        store.generation_request(&a2.request_id)?.unwrap().relevance,
        Relevance::Stale
    );
    assert_eq!(
        store.generation_request(&a1.request_id)?.unwrap().relevance,
        Relevance::Stale
    );
    assert_eq!(
        store.generation_request(&b1.request_id)?.unwrap().relevance,
        Relevance::Current
    );
    assert!(matches!(
        store.redo(&undo_revision, RevisionId::new("redo-without-plan")?),
        Err(StoreError::GenerationRelevanceRequired)
    ));
    let redo_revision = RevisionId::new("redo-grow")?;
    store.redo_reconciled(
        &undo_revision,
        redo_revision.clone(),
        &RelevancePlan {
            from_revision: undo_revision.clone(),
            to_revision: redo_revision,
            observations: vec![observe(&b1, ContextObservation::Resolved(hash('b')))],
        },
    )?;
    assert_eq!(
        store.generation_request(&b1.request_id)?.unwrap().relevance,
        Relevance::Current
    );
    let a3 = allocate(&mut store, "a-3", "a", 13, 'c')?;
    assert_eq!(a3.binding.request_version.get(), 3);
    Ok(())
}

#[test]
fn deletion_detaches_permanently_even_when_undo_restores_the_hold() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("delete.deadpan");
    let mut store = ProjectStore::create(&path, &document(&[("hold", 12)])?)?;
    let first = allocate(&mut store, "request-1", "hold", 12, 'a')?;
    let delete = edit(
        &store,
        "delete",
        Command::Delete {
            node: NodeId::new("hold")?,
        },
    )?;
    store.commit_reconciled(
        &delete,
        &plan(
            &delete,
            vec![observe(&first, ContextObservation::Resolved(hash('a')))],
        ),
    )?;
    assert_eq!(
        store
            .generation_request(&first.request_id)?
            .unwrap()
            .relevance,
        Relevance::Detached
    );
    store.undo(&delete.new_revision, RevisionId::new("restore")?)?;
    assert_eq!(
        store
            .generation_request(&first.request_id)?
            .unwrap()
            .relevance,
        Relevance::Detached
    );
    let second = allocate(&mut store, "request-2", "hold", 12, 'a')?;
    assert_eq!(second.binding.request_version.get(), 2);
    Ok(())
}

#[test]
fn relevance_and_revision_changes_roll_back_together() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("rollback.deadpan");
    let mut store = ProjectStore::create(&path, &document(&[("hold", 12)])?)?;
    let current = allocate(&mut store, "request", "hold", 12, 'a')?;
    drop(store);
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute_batch(
        "CREATE TRIGGER reject_injected_revision BEFORE INSERT ON revisions
         WHEN NEW.id='injected-failure'
         BEGIN SELECT RAISE(ABORT,'injected revision failure'); END;",
    )?;
    drop(connection);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let change = edit(
        &store,
        "injected-failure",
        Command::SetHoldDuration {
            node: NodeId::new("hold")?,
            duration: FrameDuration::new(13)?,
        },
    )?;
    assert!(matches!(
        store.commit_reconciled(
            &change,
            &plan(
                &change,
                vec![observe(&current, ContextObservation::Unresolved)]
            )
        ),
        Err(StoreError::Database(_))
    ));
    assert_eq!(store.snapshot()?.revision_id(), &change.expected_revision);
    assert_eq!(
        store
            .generation_request(&current.request_id)?
            .unwrap()
            .relevance,
        Relevance::Current
    );
    Ok(())
}

#[test]
fn stored_generation_json_is_bounded_and_strictly_revalidated() -> Result {
    for (name, replacement) in [
        ("oversized", serde_json::to_string(&"x".repeat(16 * 1024))?),
        ("unknown", "{\"unknown\":true}".into()),
    ] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join(format!("{name}.deadpan"));
        let mut store = ProjectStore::create(&path, &document(&[("hold", 12)])?)?;
        allocate(&mut store, "request", "hold", 12, 'a')?;
        drop(store);
        let connection = Connection::open(path.join("project.sqlite"))?;
        connection.execute(
            "UPDATE generation_requests SET constraints=?1",
            params![replacement],
        )?;
        drop(connection);
        assert!(matches!(
            ProjectStore::open(&path, AccessMode::ReadOnly),
            Err(StoreError::Integrity(_))
        ));
    }
    Ok(())
}

#[test]
fn validation_rejects_an_obsolete_version_forged_back_to_current() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("obsolete-current.deadpan");
    let mut store = ProjectStore::create(&path, &document(&[("hold", 12)])?)?;
    let first = allocate(&mut store, "request-1", "hold", 12, 'a')?;
    let second = allocate(&mut store, "request-2", "hold", 12, 'a')?;
    assert_eq!(second.binding.request_version.get(), 2);
    drop(store);

    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute(
        "UPDATE generation_requests SET relevance='stale' WHERE request_id=?1",
        [second.request_id.as_str()],
    )?;
    connection.execute(
        "UPDATE generation_requests SET relevance='current' WHERE request_id=?1",
        [first.request_id.as_str()],
    )?;
    drop(connection);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(_))
    ));
    Ok(())
}

#[test]
fn validation_does_not_depend_on_generation_uniqueness_indexes() -> Result {
    for (name, duplicate_version, second_relevance) in [
        ("current-hold", 2_i64, "current"),
        ("version", 1_i64, "stale"),
    ] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join(format!("duplicate-{name}.deadpan"));
        let mut store = ProjectStore::create(&path, &document(&[("hold", 12)])?)?;
        allocate(&mut store, "request-1", "hold", 12, 'a')?;
        drop(store);

        let connection = Connection::open(path.join("project.sqlite"))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=OFF;
             ALTER TABLE generation_requests RENAME TO prior_generation_requests;
             DROP INDEX one_current_generation_per_hold;
             CREATE TABLE generation_requests (
                 request_id TEXT,
                 project_id TEXT,
                 hold_id TEXT,
                 request_version INTEGER,
                 origin_revision TEXT,
                 context_sha256 TEXT,
                 constraints TEXT,
                 provider TEXT,
                 relevance TEXT
             ) STRICT;
             INSERT INTO generation_requests SELECT * FROM prior_generation_requests;",
        )?;
        connection.execute(
            "INSERT INTO generation_requests
             SELECT 'request-2',project_id,hold_id,?1,origin_revision,context_sha256,
                    constraints,provider,?2
             FROM prior_generation_requests WHERE request_id='request-1'",
            params![duplicate_version, second_relevance],
        )?;
        if duplicate_version == 2 {
            connection.execute(
                "UPDATE hold_request_clocks SET high_water=2 WHERE hold_id='hold'",
                [],
            )?;
        }
        connection.execute_batch("DROP TABLE prior_generation_requests")?;
        drop(connection);
        assert!(matches!(
            ProjectStore::open(&path, AccessMode::ReadOnly),
            Err(StoreError::Integrity(_))
        ));
    }
    Ok(())
}

#[test]
fn validation_streams_duplicate_request_ids_without_a_primary_key() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("duplicate-request-id.deadpan");
    let mut store = ProjectStore::create(&path, &document(&[("a", 12), ("b", 8)])?)?;
    allocate(&mut store, "request-a", "a", 12, 'a')?;
    allocate(&mut store, "request-b", "b", 8, 'b')?;
    drop(store);

    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=OFF;
         ALTER TABLE generation_requests RENAME TO prior_generation_requests;
         DROP INDEX one_current_generation_per_hold;
         CREATE TABLE generation_requests (
             request_id TEXT,
             project_id TEXT,
             hold_id TEXT,
             request_version INTEGER,
             origin_revision TEXT,
             context_sha256 TEXT,
             constraints TEXT,
             provider TEXT,
             relevance TEXT
         ) STRICT;
         INSERT INTO generation_requests SELECT * FROM prior_generation_requests;
         UPDATE generation_requests SET request_id='request-a' WHERE hold_id='b';
         DROP TABLE prior_generation_requests;",
    )?;
    drop(connection);

    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(_))
    ));
    Ok(())
}

#[test]
fn validation_rechecks_the_request_against_its_origin_revision() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("forged-origin.deadpan");
    let initial = document(&[])?;
    let mut store = ProjectStore::create(&path, &initial)?;
    let insert = edit(
        &store,
        "insert-hold",
        Command::Insert {
            parent: NodeId::new("root")?,
            index: 0,
            subtree: Subtree {
                root: NodeId::new("hold")?,
                nodes: BTreeMap::from([(
                    NodeId::new("hold")?,
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            duration: FrameDuration::new(12)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
            },
        },
    )?;
    store.commit(&insert)?;
    allocate(&mut store, "request", "hold", 12, 'a')?;
    drop(store);

    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute(
        "UPDATE generation_requests SET origin_revision=?1",
        [initial.revision_id().as_str()],
    )?;
    drop(connection);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(_))
    ));
    Ok(())
}
