//! Replacement intent is a durable operation, separate from authored timing
//! and from a worker request whose conditioning has already been measured.
use super::*;
use deadpan_core::{LeafEdit, ResolvedStep, ResolvedTransaction};
use deadpan_store::generation::GenerationContextResolver;
use deadpan_store::generation_preparations::{
    PreparationControls, PreparationFailure, PreparationOrigin, PreparationState,
    StoredGenerationPreparation,
};

#[path = "preparations/current_controls.rs"]
mod current_controls;
#[path = "preparations/insertion.rs"]
mod insertion;

struct Relevant;
impl GenerationContextResolver for Relevant {
    fn observe(
        &self,
        _: &ProjectDocument,
        _: &ProjectDocument,
        request: &StoredGenerationRequest,
    ) -> ContextObservation {
        ContextObservation::Resolved(request.binding.context_sha256.clone())
    }
    fn preparation_is_relevant(
        &self,
        _: &ProjectDocument,
        _: &ProjectDocument,
        _: &StoredGenerationPreparation,
    ) -> bool {
        true
    }
}

fn accepted(package: &Path) -> Result<ProjectStore> {
    let mut store = ProjectStore::create(package, &document()?)?;
    let input = ready_for_acceptance(&mut store)?;
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    Ok(store)
}

fn command(store: &ProjectStore, revision: &str, command: Command) -> Result<CommandRequest> {
    let before = store.snapshot()?;
    Ok(CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command,
    })
}

fn resize(
    store: &mut ProjectStore,
    revision: &str,
    frames: i64,
) -> Result<deadpan_store::CommitOutcome> {
    Ok(store.commit(&command(
        store,
        revision,
        Command::SetHoldDuration {
            node: NodeId::new("hold")?,
            duration: FrameDuration::new(frames)?,
        },
    )?)?)
}

fn recipe(store: &ProjectStore) -> Result<HoldRecipe> {
    let snapshot = store.snapshot()?;
    let NodeKind::Hold { recipe } = &snapshot.nodes()[&NodeId::new("hold")?].kind else {
        panic!()
    };
    Ok(recipe.clone())
}

fn replacement_input(
    store: &ProjectStore,
    request: &str,
    frames: i64,
) -> Result<(
    GenerationRequestInput,
    BridgeGenerationPlan,
    BeginGenerationAttempt,
)> {
    let mut constraints = constraints();
    constraints.video = VideoSpec::new(FrameDuration::new(frames)?, rate(), 512, 320)?;
    let plan = BridgeGenerationPlan::new(
        FrameDuration::new(frames)?,
        rate(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1)?,
            FrameCountFormula::new(1, 0, 2, 97)?,
            DimensionLimits::new(AxisLimits::new(512, 512, 1)?, AxisLimits::new(320, 320, 1)?),
        ),
        NativeDimensions::new(512, 320)?,
    )?;
    let id = RequestId::new(request)?;
    Ok((
        GenerationRequestInput {
            request_id: id.clone(),
            expected_revision: store.head_revision()?,
            hold_id: NodeId::new("hold")?,
            context_sha256: sha('b'),
            constraints,
            provider: provider(9),
        },
        plan,
        BeginGenerationAttempt {
            identity: MessageIdentity::new(id, AttemptId::new("replacement-attempt")?),
            cancellation_token: CancellationToken::new("replacement-cancel")?,
        },
    ))
}

#[test]
fn extension_commits_exact_fallback_and_retains_controls_before_any_worker() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("replacement.deadpan");
    let mut store = accepted(&package)?;
    let original = recipe(&store)?;
    assert!(
        resize(&mut store, "shorten", 8)?
            .generation_preparations
            .is_empty()
    );
    assert!(
        resize(&mut store, "restore-available", 12)?
            .generation_preparations
            .is_empty()
    );
    assert!(matches!(recipe(&store)?.video, HoldVideo::Generated { .. }));
    let outcome = resize(&mut store, "extend", 18)?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    let id = &outcome.generation_preparations[0];
    let value = store.generation_preparation(id)?.unwrap();
    assert_eq!(value.state, PreparationState::Queued);
    assert_eq!(value.claim_sequence, 0);
    assert_eq!(value.origin_revision, outcome.revision_id);
    assert_eq!(value.origin_target, value.target);
    assert!(matches!(
        value.origin,
        PreparationOrigin::AcceptedExtension {
            controls: PreparationControls::AcceptedArtifact,
            ..
        }
    ));
    assert_eq!(
        store
            .generation_request(&RequestId::new("request")?)?
            .unwrap()
            .relevance,
        deadpan_jobs::Relevance::Stale,
        "shortening made the original request stale; recover immutable artifact controls"
    );
    let current = recipe(&store)?;
    assert_eq!(current.duration, FrameDuration::new(18)?);
    assert_eq!(current.video, HoldVideo::Background);
    assert_eq!(current.audio, original.audio);
    assert_eq!(current.picture_context, original.picture_context);
    assert!(store.current_generation_requests()?.is_empty());
    assert_eq!(store.generation_preparations(None, 1)?, vec![value]);
    store.validate()?;
    drop(store);
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(
        reader.generation_preparation(id)?.unwrap().state,
        PreparationState::Queued
    );
    Ok(())
}

#[test]
fn failed_attempt_insert_rolls_back_request_clock_and_preparation_fulfilment() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("atomic.deadpan");
    let mut store = accepted(&package)?;
    let id = resize(&mut store, "extend", 18)?
        .generation_preparations
        .remove(0);
    let claim = store.claim_generation_preparation(&id, &store.head_revision()?)?;
    let (input, plan, attempt) = replacement_input(&store, "replacement", 18)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let clock: i64 = database.query_row("SELECT high_water FROM generation_scopes", [], |row| {
        row.get(0)
    })?;
    database.execute_batch(
        "CREATE TRIGGER fail_prepared_attempt BEFORE INSERT ON generation_attempts
        BEGIN SELECT RAISE(ABORT,'injected attempt persistence failure'); END;",
    )?;
    assert!(
        store
            .fulfil_generation_preparation(&claim, input.clone(), plan.clone(), attempt.clone())
            .is_err()
    );
    assert!(store.generation_request(&input.request_id)?.is_none());
    assert!(store.generation_preparation_claim_is_current(&claim)?);
    assert_eq!(
        database.query_row("SELECT high_water FROM generation_scopes", [], |row| row
            .get::<_, i64>(0))?,
        clock
    );
    database.execute_batch("DROP TRIGGER fail_prepared_attempt")?;
    let before = store.snapshot()?;
    let (request, attempt) = store.fulfil_generation_preparation(&claim, input, plan, attempt)?;
    assert_eq!(request.target, claim.preparation.target);
    assert_eq!(attempt.checkpoint.state, JobState::Queued);
    assert_eq!(store.snapshot()?, before);
    let value = store.generation_preparation(&id)?.unwrap();
    assert_eq!(value.state, PreparationState::Fulfilled);
    assert_eq!(value.request_id.as_ref(), Some(&request.request_id));
    assert!(store.generation_preparations(None, 256)?.is_empty());
    store.validate()?;
    Ok(())
}

#[test]
fn unrelated_edit_requeues_claim_and_old_fulfilment_cannot_allocate() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = accepted(&scratch.path().join("requeue.deadpan"))?;
    let id = resize(&mut store, "extend", 18)?
        .generation_preparations
        .remove(0);
    let claim = store.claim_generation_preparation(&id, &store.head_revision()?)?;
    let (input, plan, attempt) = replacement_input(&store, "late", 18)?;
    let rename = command(
        &store,
        "rename",
        Command::Rename {
            node: NodeId::new("hold")?,
            label: "Long pause".into(),
        },
    )?;
    store.commit(&rename)?;
    let current = store.generation_preparation(&id)?.unwrap();
    assert_eq!(current.state, PreparationState::Queued);
    assert_eq!(current.current_revision, rename.new_revision);
    assert!(current.claim_sequence > claim.preparation.claim_sequence);
    assert!(!store.generation_preparation_claim_is_current(&claim)?);
    assert!(
        !store.finish_generation_preparation(
            &claim,
            PreparationFailure::Unavailable("late".into())
        )?
    );
    assert!(
        store
            .fulfil_generation_preparation(&claim, input, plan, attempt)
            .is_err()
    );
    assert!(
        store
            .generation_request(&RequestId::new("late")?)?
            .is_none()
    );
    let fresh = store.claim_generation_preparation(&id, &rename.new_revision)?;
    assert!(store.generation_preparation_claim_is_current(&fresh)?);
    store.validate()?;
    Ok(())
}

#[test]
fn unavailable_retry_restart_and_manual_supersession_preserve_timing() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("recover.deadpan");
    let mut store = accepted(&package)?;
    let id = resize(&mut store, "extend", 18)?
        .generation_preparations
        .remove(0);
    let before = store.snapshot()?;
    let claim = store.claim_generation_preparation(&id, before.revision_id())?;
    assert!(store.finish_generation_preparation(
        &claim,
        PreparationFailure::Unavailable("The model pack is not installed.".into())
    )?);
    assert_eq!(
        store.generation_preparation(&id)?.unwrap().state,
        PreparationState::Unavailable
    );
    assert!(
        store
            .claim_generation_preparation(&id, before.revision_id())
            .is_err()
    );
    store.retry_generation_preparation(&id, before.revision_id())?;
    let claim = store.claim_generation_preparation(&id, before.revision_id())?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(
        store.generation_preparation(&id)?.unwrap().state,
        PreparationState::Interrupted
    );
    assert!(!store.generation_preparation_claim_is_current(&claim)?);
    store.retry_generation_preparation(&id, before.revision_id())?;
    let claim = store.claim_generation_preparation(&id, before.revision_id())?;
    let (input, plan, _) = replacement_input(&store, "manual", 18)?;
    store.record_bridge_generation_request(input, plan)?;
    assert_eq!(
        store.generation_preparation(&id)?.unwrap().state,
        PreparationState::Cancelled
    );
    assert!(
        !store.finish_generation_preparation(
            &claim,
            PreparationFailure::Interrupted("late".into())
        )?
    );
    assert_eq!(store.snapshot()?, before);
    store.validate()?;
    Ok(())
}

#[test]
fn undo_cancels_claim_and_redo_creates_new_intent_with_original_controls() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = accepted(&scratch.path().join("history.deadpan"))?;
    let id = resize(&mut store, "extend", 18)?
        .generation_preparations
        .remove(0);
    let original = store.generation_preparation(&id)?.unwrap();
    let claim = store.claim_generation_preparation(&id, &store.head_revision()?)?;
    store.undo(&store.head_revision()?, RevisionId::new("undo")?)?;
    assert!(matches!(recipe(&store)?.video, HoldVideo::Generated { .. }));
    assert_eq!(
        store.generation_preparation(&id)?.unwrap().state,
        PreparationState::Cancelled
    );
    assert!(!store.generation_preparation_claim_is_current(&claim)?);
    let redo = store.redo(&store.head_revision()?, RevisionId::new("redo")?)?;
    let new_id = &redo.generation_preparations[0];
    assert_ne!(new_id, &id);
    let fresh = store.generation_preparation(new_id)?.unwrap();
    assert_eq!(fresh.state, PreparationState::Queued);
    assert_eq!(fresh.origin, original.origin);
    assert_eq!(fresh.duration, original.duration);
    assert_eq!(fresh.origin_revision, redo.revision_id);
    assert_eq!(
        store
            .generation_request(&RequestId::new("request")?)?
            .unwrap()
            .relevance,
        deadpan_jobs::Relevance::Stale
    );
    store.validate()?;
    Ok(())
}

fn compound(store: &ProjectStore, commands: Vec<Command>) -> Result<CommandRequest> {
    let steps = commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            Ok(ResolvedStep::Edit {
                edit: LeafEdit::new(RevisionId::new(format!("leaf-{index}"))?, command)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    command(
        store,
        "compound",
        Command::Compound {
            transaction: ResolvedTransaction::new(0, BTreeMap::new(), steps)?,
        },
    )
}

#[test]
fn compound_explicit_fallback_supersedes_birth_even_if_its_pixels_are_equal() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = accepted(&scratch.path().join("compound.deadpan"))?;
    let request = compound(
        &store,
        vec![
            Command::SetHoldDuration {
                node: NodeId::new("hold")?,
                duration: FrameDuration::new(18)?,
            },
            Command::SetHoldProvider {
                node: NodeId::new("hold")?,
                video: HoldVideo::Background,
            },
        ],
    )?;
    let outcome = store.commit(&request)?;
    assert!(outcome.generation_preparations.is_empty());
    assert_eq!(recipe(&store)?.duration, FrameDuration::new(18)?);
    assert_eq!(recipe(&store)?.video, HoldVideo::Background);
    store.validate()?;
    Ok(())
}

#[test]
fn explicit_fallback_cancels_pending_preparation_in_plain_compound_and_scoped_commands() -> Result {
    for (kind, compound_command) in ["set", "revert", "scoped", "scoped_many"]
        .into_iter()
        .flat_map(|kind| [false, true].map(|compound| (kind, compound)))
    {
        let scratch = tempfile::tempdir()?;
        let mut store = accepted(&scratch.path().join("provider-choice.deadpan"))?;
        let id = resize(&mut store, "extend", 18)?
            .generation_preparations
            .remove(0);
        let claim = store.claim_generation_preparation(&id, &store.head_revision()?)?;
        let target = store.generation_preparation(&id)?.unwrap().target;
        let choice = match kind {
            "set" => Command::SetHoldProvider {
                node: target.node,
                video: HoldVideo::Background,
            },
            "revert" => Command::RevertGeneratedHold { node: target.node },
            "scoped" => Command::EditScoped {
                target,
                edit: deadpan_core::ScopedNodeEdit::RevertGeneratedHold,
                identities: deadpan_core::OccurrenceIdentities {
                    nodes: vec![],
                    marks: vec![],
                },
            },
            "scoped_many" => Command::EditScopedMany {
                edits: vec![deadpan_core::ScopedTargetEdit {
                    target,
                    edit: deadpan_core::ScopedNodeEdit::RevertGeneratedHold,
                }],
                identities: vec![deadpan_core::OccurrenceIdentities {
                    nodes: vec![],
                    marks: vec![],
                }],
            },
            _ => unreachable!(),
        };
        let request = if compound_command {
            compound(&store, vec![choice])?
        } else {
            command(&store, "choose-fallback", choice)?
        };
        store.commit(&request)?;
        assert_eq!(
            store.generation_preparation(&id)?.unwrap().state,
            PreparationState::Cancelled
        );
        assert!(!store.generation_preparation_claim_is_current(&claim)?);
        assert_eq!(recipe(&store)?.duration.frames(), 18);
        assert_eq!(recipe(&store)?.video, HoldVideo::Background);
        store.validate_full()?;
        store.undo(&store.head_revision()?, RevisionId::new("undo-choice")?)?;
        store.undo(&store.head_revision()?, RevisionId::new("undo-extension")?)?;
        let redo = store.redo(&store.head_revision()?, RevisionId::new("redo-extension")?)?;
        let fresh = &redo.generation_preparations[0];
        assert_eq!(
            store.generation_preparation(fresh)?.unwrap().state,
            PreparationState::Queued
        );
        store.redo(&store.head_revision()?, RevisionId::new("redo-choice")?)?;
        assert_eq!(
            store.generation_preparation(fresh)?.unwrap().state,
            PreparationState::Cancelled
        );
        assert!(store.generation_preparations(None, 256)?.is_empty());
        store.validate_full()?;
        store.undo(
            &store.head_revision()?,
            RevisionId::new("undo-choice-again")?,
        )?;
        store.undo(
            &store.head_revision()?,
            RevisionId::new("undo-extension-again")?,
        )?;
        let redo = store.redo(
            &store.head_revision()?,
            RevisionId::new("redo-extension-again")?,
        )?;
        let pending_id = &redo.generation_preparations[0];
        let pending = store.generation_preparation(pending_id)?.unwrap();
        assert_eq!(pending.state, PreparationState::Queued);
        // A past provider Redo must not mutate a newer pending replacement
        // while validating historical commands, on either access mode.
        store.validate_full()?;
        assert_eq!(store.generation_preparation(pending_id)?.unwrap(), pending);
        let readonly = ProjectStore::open(
            &scratch.path().join("provider-choice.deadpan"),
            AccessMode::ReadOnly,
        )?;
        readonly.validate_full()?;
        assert_eq!(
            readonly.generation_preparation(pending_id)?.unwrap(),
            pending
        );
    }
    Ok(())
}

#[test]
fn compound_multiple_resizes_prepare_only_the_final_duration() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = accepted(&scratch.path().join("compound-resize.deadpan"))?;
    let request = compound(
        &store,
        [18, 20, 16]
            .into_iter()
            .map(|frames| Command::SetHoldDuration {
                node: NodeId::new("hold").unwrap(),
                duration: FrameDuration::new(frames).unwrap(),
            })
            .collect(),
    )?;
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    let preparation = store
        .generation_preparation(&outcome.generation_preparations[0])?
        .unwrap();
    assert_eq!(preparation.duration, FrameDuration::new(16)?);
    assert_eq!(preparation.origin_revision, request.new_revision);
    store.validate()?;
    Ok(())
}

#[test]
fn tampered_preparation_birth_controls_or_address_cannot_be_laundered_by_an_edit() -> Result {
    for field in ["missing", "target", "controls", "duration"] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("tampered.deadpan");
        let mut store = accepted(&package)?;
        let id = resize(&mut store, "extend", 18)?
            .generation_preparations
            .remove(0);
        let mut value = store.generation_preparation(&id)?.unwrap();
        let before = store.snapshot()?;
        let database = Connection::open(package.join("project.sqlite"))?;
        if field == "missing" {
            database.execute("DELETE FROM generation_preparations", [])?;
        } else {
            match field {
                "target" => value.target.node = NodeId::new("root")?,
                "controls" => {
                    if let PreparationOrigin::AcceptedExtension {
                        controls: PreparationControls::Request { options, .. },
                        ..
                    } = &mut value.origin
                    {
                        options.motion = MotionAmount::Subtle;
                    }
                }
                "duration" => value.duration = FrameDuration::new(19)?,
                _ => unreachable!(),
            }
            database.execute(
                "UPDATE generation_preparations SET record=?1",
                [serde_json::to_string(&value)?],
            )?;
        }
        assert!(store.generation_preparation(&id).is_err(), "{field}");
        let edit = command(
            &store,
            "rename",
            Command::Rename {
                node: NodeId::new("hold")?,
                label: "Late".into(),
            },
        )?;
        assert!(store.commit(&edit).is_err(), "{field}");
        assert_eq!(store.snapshot()?, before);
        assert!(store.validate().is_err(), "{field}");
    }
    Ok(())
}

#[test]
fn selected_play_extension_keeps_default_and_other_plays_generated() -> Result {
    use deadpan_core::{
        InstancePath, OccurrenceEdit, OccurrenceIdentities, RepeatEditBranch, RepeatEditStep,
        RepeatInstance, ScopedNodeTarget,
    };
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("scoped-preparation.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    store.commit(&command(
        &store,
        "repeat",
        Command::WrapRepeat {
            node: NodeId::new("hold")?,
            id: NodeId::new("repeat")?,
            plays: 3,
            gap: None,
            anchor_policy: Default::default(),
        },
    )?)?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    let repeated = store.snapshot()?;
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&NodeId::new("repeat")?].kind
    else {
        panic!()
    };
    let second = iterations.at(1).unwrap();
    let third = iterations.at(2).unwrap();
    let default = ScopedNodeTarget {
        node: NodeId::new("hold")?,
        repeats: vec![RepeatEditStep {
            repeat: NodeId::new("repeat")?,
            branch: RepeatEditBranch::Default,
        }],
    };
    let request = store.record_scoped_bridge_generation_request(
        GenerationRequestInput {
            request_id: RequestId::new("default")?,
            expected_revision: store.head_revision()?,
            hold_id: default.node.clone(),
            context_sha256: sha('a'),
            constraints: constraints(),
            provider: provider(1),
        },
        default.clone(),
        plan(),
    )?;
    publish_bundle(&mut store)?;
    publish_inputs(&mut store)?;
    let (identity, expected_receipt) = ready_variant(&mut store, &request, "default-attempt", 1)?;
    let input = GenerationAcceptance {
        expected_revision: store.head_revision()?,
        new_revision: RevisionId::new("accepted")?,
        identity,
        expected_receipt,
        native_asset: AssetId::new("native")?,
        sampled_asset: AssetId::new("sampled")?,
    };
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    let resize = command(
        &store,
        "second-longer",
        Command::EditOccurrence {
            instance: InstancePath {
                node: NodeId::new("hold")?,
                repeats: vec![RepeatInstance {
                    node: NodeId::new("repeat")?,
                    iteration: second.clone(),
                }],
            },
            edit: OccurrenceEdit::SetHoldDuration {
                duration: FrameDuration::new(18)?,
            },
            identities: OccurrenceIdentities {
                nodes: vec![NodeId::new("second-hold")?],
                marks: Vec::new(),
            },
        },
    )?;
    let outcome = store.commit(&resize)?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    let id = &outcome.generation_preparations[0];
    let preparation = store.generation_preparation(id)?.unwrap();
    assert_eq!(preparation.target.node, NodeId::new("second-hold")?);
    assert_eq!(
        preparation.target.repeats[0].branch,
        RepeatEditBranch::Play {
            iteration: second.clone()
        }
    );
    assert!(matches!(
        preparation.origin,
        PreparationOrigin::AcceptedExtension {
            controls: PreparationControls::AcceptedArtifact,
            ..
        }
    ));
    assert_eq!(
        store
            .generation_request(&request.request_id)?
            .unwrap()
            .target,
        default
    );
    assert_eq!(
        store
            .generation_request(&request.request_id)?
            .unwrap()
            .relevance,
        deadpan_jobs::Relevance::Current
    );
    let after = store.snapshot()?;
    let third = ScopedNodeTarget {
        node: NodeId::new("hold")?,
        repeats: vec![RepeatEditStep {
            repeat: NodeId::new("repeat")?,
            branch: RepeatEditBranch::Play { iteration: third },
        }],
    };
    third.validate(&after)?;
    assert!(matches!(recipe(&store)?.video, HoldVideo::Generated { .. }));
    let claim = store.claim_generation_preparation(id, &outcome.revision_id)?;
    store.undo(&outcome.revision_id, RevisionId::new("undo-scope")?)?;
    let cancelled = store.generation_preparation(id)?.unwrap();
    assert_eq!(cancelled.state, PreparationState::Cancelled);
    assert_eq!(cancelled.target.node, NodeId::new("hold")?);
    assert_eq!(
        cancelled.target.repeats[0].branch,
        RepeatEditBranch::Play { iteration: second }
    );
    assert!(!store.generation_preparation_claim_is_current(&claim)?);
    let redo = store.redo(&store.head_revision()?, RevisionId::new("redo-scope")?)?;
    assert_ne!(redo.generation_preparations[0], *id);
    assert_eq!(
        store
            .generation_preparation(&redo.generation_preparations[0])?
            .unwrap()
            .target,
        preparation.target
    );
    store.validate_full()?;
    Ok(())
}

#[test]
fn compound_other_hold_duration_does_not_turn_provider_change_into_replacement() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = accepted(&scratch.path().join("unrelated-duration.deadpan"))?;
    store.commit(&command(
        &store,
        "sibling",
        Command::Insert {
            parent: NodeId::new("root")?,
            index: 1,
            subtree: Subtree {
                root: NodeId::new("sibling")?,
                nodes: BTreeMap::from([(
                    NodeId::new("sibling")?,
                    BeatNode::hold(
                        "Other pause",
                        HoldRecipe {
                            picture_context: None,
                            duration: FrameDuration::new(12)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?)?;
    let request = compound(
        &store,
        vec![
            Command::SetHoldDuration {
                node: NodeId::new("sibling")?,
                duration: FrameDuration::new(18)?,
            },
            Command::RevertGeneratedHold {
                node: NodeId::new("hold")?,
            },
        ],
    )?;
    assert!(store.commit(&request)?.generation_preparations.is_empty());
    assert_eq!(recipe(&store)?.video, HoldVideo::Background);
    assert_eq!(recipe(&store)?.duration, FrameDuration::new(12)?);
    store.validate_full()?;
    Ok(())
}

#[test]
fn compacted_terminal_preparation_still_proves_birth_and_supplies_redo_controls() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("compacted.deadpan");
    let mut store = accepted(&package)?;
    let recipe = recipe(&store)?;
    // Preserve the accepted Original inside its own absent-boundary definition
    // while ordinary authored insertion admits the copied artifacts below.
    store.commit(&command(
        &store,
        "isolate-original",
        Command::WrapRepeat {
            node: NodeId::new("hold")?,
            id: NodeId::new("original-definition")?,
            plays: 1,
            gap: None,
            anchor_policy: Default::default(),
        },
    )?)?;
    let count = 257;
    let children = (0..count)
        .map(|index| NodeId::new(format!("copy-{index}")))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let group = NodeId::new("copies")?;
    let mut nodes: BTreeMap<_, _> = children
        .iter()
        .map(|id| {
            (
                id.clone(),
                BeatNode::hold("Copied accepted pause", recipe.clone()),
            )
        })
        .collect();
    // Each copy owns a one-play definition with the same absent endpoints as
    // the accepted origin. Sequential unwrapped copies would correctly become
    // boundary replacements before this duration/compaction fixture reaches
    // its extension commands.
    let mut wrappers = Vec::new();
    for (index, child) in children.iter().enumerate() {
        let wrapper = NodeId::new(format!("copy-definition-{index}"))?;
        let mut node = BeatNode::sequence("Copied definition", Vec::new());
        node.kind = NodeKind::Repeat {
            child: child.clone(),
            iterations: deadpan_core::IterationOrder::new(RevisionId::new("copies")?, 1)?,
            gap: None,
            escalation: None,
        };
        nodes.insert(wrapper.clone(), node);
        wrappers.push(wrapper);
    }
    nodes.insert(group.clone(), BeatNode::sequence("Copies", wrappers));
    store.commit(&command(
        &store,
        "copies",
        Command::Insert {
            parent: NodeId::new("root")?,
            index: 1,
            subtree: Subtree {
                root: group,
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?)?;
    // Compact a new insertion origin alongside accepted extensions, without
    // requiring a second lifetime-sized integration fixture.
    let inserted = store
        .commit(&command(
            &store,
            "new-ai-pause",
            Command::InsertAiTime {
                at: deadpan_core::ProjectFrame(0),
                hold: HoldRecipe {
                    duration: FrameDuration::new(18)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
                id: NodeId::new("new-ai-hold")?,
                identities: deadpan_core::SplitIdentities { nodes: Vec::new() },
                timing: deadpan_core::AudioTimingId {
                    allocation: RevisionId::new("new-ai-pause")?,
                    ordinal: 0,
                },
            },
        )?)?
        .generation_preparations
        .remove(0);
    store.cancel_generation_preparation(&inserted, 0)?;
    // Capture current controls for this explicit definition. Wrapping changed
    // the original request address; its old request must not become current.
    let source_request = RequestId::new("extension-controls")?;
    let (input, plan, _) = replacement_input(&store, source_request.as_str(), 12)?;
    store.record_scoped_bridge_generation_request(
        input,
        deadpan_core::ScopedNodeTarget {
            node: NodeId::new("hold")?,
            repeats: vec![deadpan_core::RepeatEditStep {
                repeat: NodeId::new("original-definition")?,
                branch: deadpan_core::RepeatEditBranch::Default,
            }],
        },
        plan,
    )?;
    let source_id = resize(&mut store, "first-extension", 18)?
        .generation_preparations
        .remove(0);
    assert!(matches!(
        store.generation_preparation(&source_id)?.unwrap().origin,
        PreparationOrigin::AcceptedExtension {
            controls: PreparationControls::Request { .. },
            ..
        }
    ));
    store.cancel_generation_preparation(&source_id, 0)?;
    let request = compound(
        &store,
        children
            .into_iter()
            .map(|node| Command::SetHoldDuration {
                node,
                duration: FrameDuration::new(18).unwrap(),
            })
            .collect(),
    )?;
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.generation_preparations.len(), count);
    for id in &outcome.generation_preparations {
        store.cancel_generation_preparation(id, 0)?;
    }
    let database = Connection::open(package.join("project.sqlite"))?;
    let retired: String = database.query_row(
        "SELECT id FROM generation_preparation_retirements",
        [],
        |row| row.get(0),
    )?;
    let retired = deadpan_store::generation_preparations::PreparationId::new(retired)?;
    assert!(store.generation_preparation(&retired)?.is_none());
    assert!(store.generation_preparation(&source_id)?.is_none());
    assert!(store.generation_preparation(&inserted)?.is_none());
    let insertion_origin: String = database.query_row(
        "SELECT record FROM generation_preparation_retirements WHERE id=?1",
        [inserted.as_str()],
        |row| row.get(0),
    )?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&insertion_origin)?["origin"]["kind"],
        "inserted_pause"
    );
    assert_eq!(
        database.query_row("SELECT count(*) FROM generation_preparations", [], |row| {
            row.get::<_, i64>(0)
        })?,
        256
    );
    store.validate_full()?;
    store.undo(&store.head_revision()?, RevisionId::new("undo-copies")?)?;
    let redo = store.redo(&store.head_revision()?, RevisionId::new("redo-copies")?)?;
    assert_eq!(redo.generation_preparations.len(), count);
    assert_eq!(store.generation_preparations(None, 256)?.len(), 256);
    let first_page = store.generation_preparations(None, 256)?;
    assert_eq!(
        store
            .generation_preparations(first_page.last().map(|value| &value.id), 256)?
            .len(),
        1
    );
    assert!(first_page.iter().all(|value| matches!(
        value.origin,
        PreparationOrigin::AcceptedExtension {
            controls: PreparationControls::AcceptedArtifact,
            ..
        }
    )));
    store.validate_full()?;
    // The source constraints remain a dependency after full-row compaction.
    // Changing them must invalidate the receipt, including the warm read path.
    let original_constraints: String = database.query_row(
        "SELECT constraints FROM generation_requests WHERE request_id=?1",
        [source_request.as_str()],
        |row| row.get(0),
    )?;
    let mut changed = constraints();
    changed.motion = MotionAmount::Subtle;
    database.execute(
        "UPDATE generation_requests SET constraints=?1 WHERE request_id=?2",
        rusqlite::params![serde_json::to_string(&changed)?, source_request.as_str()],
    )?;
    assert!(store.generation_preparations(None, 1).is_err());
    let before = store.snapshot()?;
    let late = command(
        &store,
        "launder-retired-controls",
        Command::Rename {
            node: NodeId::new("hold")?,
            label: "Late".into(),
        },
    )?;
    assert!(store.commit(&late).is_err());
    assert_eq!(store.snapshot()?, before);
    database.execute(
        "UPDATE generation_requests SET constraints=?1 WHERE request_id=?2",
        rusqlite::params![original_constraints, source_request.as_str()],
    )?;
    store.validate_full()?;
    database.execute(
        "DELETE FROM generation_preparation_retirements WHERE id=?1",
        [retired.as_str()],
    )?;
    assert!(store.validate_full().is_err());
    Ok(())
}
