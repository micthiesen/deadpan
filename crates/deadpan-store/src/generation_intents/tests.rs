use super::*;
use crate::generation::{
    ContextObservation, GenerationContextResolver, GenerationRequestInput, StoredGenerationRequest,
};
use crate::generation_attempts::BeginGenerationAttempt;
use crate::generation_preparations::{
    PreparationFailure, PreparationState, StoredGenerationPreparation,
};
use deadpan_core::{
    AudioTimingId, BeatNode, ColorPolicy, Command, CommandRequest, FrameRate, HoldAudio,
    HoldRecipe, NodeId, PresentationBasis, ProjectFrame, SplitIdentities, Subtree,
};
use deadpan_jobs::{
    AttemptId, AxisLimits, BridgeCapability, BridgeGenerationPlan, CancellationToken,
    ConditioningMode, DimensionLimits, FrameCountFormula, HoldConstraints, MessageIdentity,
    MotionAmount, NativeDimensions, ProviderPackId, ProviderPackVersion, ProviderSelection,
    Relevance, RuntimeId, RuntimeVersion, Sha256 as ContentHash, VideoSpec,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn rate() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

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

fn document() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("intent-fixture").unwrap(),
        revision("empty"),
        PresentationBasis {
            width: 512,
            height: 320,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let command = CommandRequest {
        project_id: empty.project_id().clone(),
        expected_revision: empty.revision_id().clone(),
        new_revision: revision("initial"),
        command: Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("base"),
                nodes: BTreeMap::from([(
                    id("base"),
                    BeatNode::hold(
                        "Black fixture",
                        HoldRecipe {
                            duration: frames(60),
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                            picture_context: None,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    };
    deadpan_core::apply(&empty, &command)
        .unwrap()
        .forward
        .apply(&empty)
        .unwrap()
}
fn command(store: &ProjectStore, next: &str, command: Command) -> CommandRequest {
    let current = store.snapshot().unwrap();
    CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}
fn insert(store: &mut ProjectStore, name: &str) -> Result<StoredGenerationPreparation> {
    let before = store.snapshot()?;
    let target = before.insert_time_target(ProjectFrame(12))?;
    let next = format!("insert-{name}");
    let outcome = store.commit(&command(
        store,
        &next,
        Command::InsertAiTime {
            at: ProjectFrame(12),
            id: id(name),
            hold: HoldRecipe {
                duration: frames(18),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
            identities: SplitIdentities {
                nodes: (0..target.split.map_or(0, |split| split.required_ids))
                    .map(|index| id(&format!("{name}-split-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(&next),
                ordinal: 0,
            },
        },
    ))?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    Ok(store
        .generation_preparation(&outcome.generation_preparations[0])?
        .unwrap())
}
fn fulfil(
    store: &mut ProjectStore,
    value: &StoredGenerationPreparation,
    name: &str,
) -> Result<RequestId> {
    let request_id = RequestId::new(name)?;
    let plan = BridgeGenerationPlan::new(
        value.duration,
        rate(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1)?,
            FrameCountFormula::new(1, 0, 2, 97)?,
            DimensionLimits::new(AxisLimits::new(512, 512, 1)?, AxisLimits::new(320, 320, 1)?),
        ),
        NativeDimensions::new(512, 320)?,
    )?;
    let claim = store.claim_generation_preparation(&value.id, &store.head_revision()?)?;
    store.fulfil_generation_preparation(
        &claim,
        GenerationRequestInput {
            request_id: request_id.clone(),
            expected_revision: store.head_revision()?,
            hold_id: value.target.node.clone(),
            context_sha256: ContentHash::new("a".repeat(64))?,
            constraints: HoldConstraints {
                video: VideoSpec::new(value.duration, rate(), 512, 320)?,
                conditioning: ConditioningMode::Bridge,
                motion: MotionAmount::Still,
                instructions: None,
                region_target: None,
            },
            provider: ProviderSelection {
                pack_id: ProviderPackId::new("fixture")?,
                pack_version: ProviderPackVersion::new("1")?,
                runtime_id: RuntimeId::new("fixture")?,
                runtime_version: RuntimeVersion::new("1")?,
                seed: 1,
            },
        },
        plan,
        BeginGenerationAttempt {
            identity: MessageIdentity::new(request_id.clone(), AttemptId::new("attempt")?),
            cancellation_token: CancellationToken::new("cancel")?,
        },
    )?;
    Ok(request_id)
}

#[test]
fn fresh_intent_without_optional_resolver_stays_queued_and_reopens() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("fresh.deadpan");
    let mut store = ProjectStore::create(&package, &document())?;
    let value = insert(&mut store, "ai")?;
    assert_eq!(value.state, PreparationState::Queued);
    assert_eq!(value.claim_sequence, 0);
    assert!(
        matches!(&value.intent.input_binding, IntentInputBinding::Measured { binding }
        if binding.left == Some(crate::generation_pictures::GenerationPictureIdentity::AuthoredBlack)
        && binding.right == Some(crate::generation_pictures::GenerationPictureIdentity::AuthoredBlack))
    );
    assert_eq!(
        store.generation_intents(None, 1)?[0].birth,
        value.intent_birth()
    );
    store.validate_full()?;
    drop(store);
    let store = ProjectStore::open(&package, crate::AccessMode::ReadOnly)?;
    store.validate_full()?;
    assert_eq!(
        store.generation_intents(None, 1)?[0].head.activation_id,
        value.id
    );
    Ok(())
}

#[test]
fn operational_failure_keeps_intent_but_explicit_cancel_is_one_shot_and_cas_bound() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("cancel.deadpan"), &document())?;
    let value = insert(&mut store, "ai")?;
    let claim = store.claim_generation_preparation(&value.id, &store.head_revision()?)?;
    assert!(store.finish_generation_preparation(
        &claim,
        PreparationFailure::Cancelled("Worker stopped without a candidate.".into())
    )?);
    assert_eq!(store.generation_intents(None, 1)?.len(), 1);
    assert!(!store.cancel_generation_intent(&value.id, &revision("initial"))?);
    assert!(store.cancel_generation_intent(&value.id, &store.head_revision()?)?);
    let first = read_terminal(&store.connection, &value.id)?.unwrap();
    assert_eq!(first.reason, IntentTerminalReason::UserCancelled);
    assert!(!store.cancel_generation_intent(&value.id, &store.head_revision()?)?);
    assert_eq!(read_terminal(&store.connection, &value.id)?, Some(first));
    assert!(store.generation_intents(None, 1)?.is_empty());
    assert!(!store.generation_preparation_claim_is_current(&claim)?);
    store.validate_full()?;
    Ok(())
}

#[test]
fn fulfilled_head_survives_unrelated_edit_and_provider_choice_revokes_it() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("fulfilled.deadpan"), &document())?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    let value = insert(&mut store, "ai")?;
    let request = fulfil(&mut store, &value, "candidate")?;
    store.commit(&command(
        &store,
        "rename",
        Command::Rename {
            node: id("base"),
            label: "Unrelated label".into(),
        },
    ))?;
    assert_eq!(
        store.generation_intents(None, 1)?[0]
            .head
            .request_id
            .as_ref(),
        Some(&request)
    );
    store.commit(&command(
        &store,
        "choose-fallback",
        Command::RevertGeneratedHold {
            node: value.target.node.clone(),
        },
    ))?;
    assert!(store.generation_intents(None, 1)?.is_empty());
    assert_eq!(
        read_terminal(&store.connection, &value.id)?.unwrap().reason,
        IntentTerminalReason::ProviderChoice
    );
    assert_eq!(
        store.generation_request(&request)?.unwrap().relevance,
        Relevance::Stale
    );
    let fulfilled = store.generation_preparation(&value.id)?.unwrap();
    assert_eq!(fulfilled.state, PreparationState::Fulfilled);
    assert_eq!(fulfilled.request_id.as_ref(), Some(&request));
    store.validate_full()?;
    Ok(())
}

#[test]
fn undo_and_repeated_redo_allocate_fresh_authorizations_without_resurrection() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("redo.deadpan"), &document())?;
    let first = insert(&mut store, "ai")?;
    assert!(store.cancel_generation_intent(&first.id, &store.head_revision()?)?);
    store.undo(&store.head_revision()?, revision("undo-one"))?;
    let second = store
        .redo(&store.head_revision()?, revision("redo-one"))?
        .generation_preparations
        .remove(0);
    let second_birth = read_birth(&store.connection, &second)?;
    assert_eq!(
        second_birth.receipt.authorization,
        IntentAuthorization::Redo {
            original_activation: first.id.clone()
        }
    );
    store.undo(&store.head_revision()?, revision("undo-two"))?;
    let third = store
        .redo(&store.head_revision()?, revision("redo-two"))?
        .generation_preparations
        .remove(0);
    assert_ne!(third, second);
    assert_ne!(third, first.id);
    assert_eq!(
        read_terminal(&store.connection, &first.id)?.unwrap().reason,
        IntentTerminalReason::UserCancelled
    );
    assert!(read_terminal(&store.connection, &second)?.is_some());
    assert_eq!(
        store.generation_intents(None, 1)?[0].head.activation_id,
        third
    );
    store.validate_full()?;
    Ok(())
}

#[test]
fn full_replay_rejects_fabricated_capacity_instead_of_user_cancellation() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store =
        ProjectStore::create(&scratch.path().join("capacity-proof.deadpan"), &document())?;
    let value = insert(&mut store, "ai")?;
    store.cancel_generation_intent(&value.id, &store.head_revision()?)?;
    let mut terminal = read_terminal(&store.connection, &value.id)?.unwrap();
    terminal.phase = TerminalPhase::Transition;
    terminal.reason = IntentTerminalReason::Capacity;
    store.connection.execute("UPDATE generation_intent_terminals SET phase='transition',record=?1 WHERE activation_id=?2",
        params![serde_json::to_string(&terminal)?,value.id.as_str()])?;
    assert!(store.validate_full().is_err());
    Ok(())
}

#[test]
fn unavailable_diagnostics_are_not_identity_but_typed_causes_are() {
    let unavailable = |cause, detail: &str| IntentInputBinding::Unavailable {
        cause,
        detail: detail.into(),
    };
    let first = unavailable(InputUnavailableCause::MissingQualification, "Old wording");
    assert!(first.same_authority(&unavailable(
        InputUnavailableCause::MissingQualification,
        "New wording"
    )));
    assert!(!first.same_authority(&unavailable(
        InputUnavailableCause::InvalidRetainedEvidence,
        "Old wording"
    )));
}

#[test]
fn fulfilled_retirement_keeps_full_birth_and_current_head_then_cancels_explicitly() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("retired.deadpan");
    let mut store = ProjectStore::create(&package, &document())?;
    let value = insert(&mut store, "ai")?;
    let request = fulfil(&mut store, &value, "candidate")?;
    // Use the real compaction path with zero retained terminals to isolate the
    // live-to-retired proof from the separate production queue-capacity test.
    crate::generation_preparations::compact_completed_for_test(&store.connection)?;
    crate::audit::refresh_generation_scopes(&store.connection)?;
    assert!(store.generation_preparation(&value.id)?.is_none());
    let current = store.generation_intents(None, 1)?.remove(0);
    assert_eq!(current.birth, value.intent_birth());
    assert_eq!(current.head.request_id, Some(request.clone()));
    store.validate_full()?;
    drop(store);
    let mut store = ProjectStore::open(&package, crate::AccessMode::ReadWrite)?;
    assert_eq!(
        store.generation_intents(None, 1)?[0].birth,
        value.intent_birth()
    );
    assert!(store.cancel_generation_intent(&value.id, &store.head_revision()?)?);
    assert_eq!(
        store.generation_request(&request)?.unwrap().relevance,
        Relevance::Stale
    );
    assert_eq!(
        read_birth(&store.connection, &value.id)?,
        value.intent_birth()
    );
    store.validate_full()?;
    Ok(())
}

#[test]
fn accepted_close_preserves_request_relevance_and_fulfilled_receipt() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store =
        ProjectStore::create(&scratch.path().join("accept-policy.deadpan"), &document())?;
    let value = insert(&mut store, "ai")?;
    let request_id = fulfil(&mut store, &value, "candidate")?;
    let before = store.generation_preparation(&value.id)?.unwrap();
    let head = read_head(&store.connection, &value.id)?.unwrap();
    // This transaction-local policy test does not fabricate a Ready movie or
    // an authored acceptance. Full replay must reject that missing proof.
    assert!(close(
        &store.connection,
        &head,
        IntentTerminal {
            activation_id: value.id.clone(),
            at_revision: store.head_revision()?,
            phase: TerminalPhase::Transition,
            target: head.target.clone(),
            reason: IntentTerminalReason::Accepted {
                request_id: request_id.clone(),
                attempt_id: AttemptId::new("attempt")?
            }
        }
    )?);
    assert_eq!(
        store.generation_request(&request_id)?.unwrap().relevance,
        Relevance::Current
    );
    // Bypass public proof verification to inspect the unchanged work row.
    let record: String = store.connection.query_row(
        "SELECT record FROM generation_preparations WHERE id=?1",
        [value.id.as_str()],
        |row| row.get(0),
    )?;
    assert_eq!(
        serde_json::from_str::<StoredGenerationPreparation>(&record)?,
        before
    );
    assert!(store.validate_full().is_err());
    Ok(())
}

#[test]
fn renewed_inserted_intent_keeps_reciprocal_proof_after_successor_cancel() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("renewed.deadpan");
    let mut store = ProjectStore::create(&package, &document())?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    let original = insert(&mut store, "ai")?;
    let snapshot = store.snapshot()?;
    let NodeKind::Sequence { children } = &snapshot.nodes()[snapshot.root()].kind else {
        panic!()
    };
    let position = children
        .iter()
        .position(|node| node == &original.target.node)
        .unwrap();
    let right = children[position + 1].clone();
    let outcome = store.commit(&command(
        &store,
        "remove-right",
        Command::Delete { node: right },
    ))?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    let successor = outcome.generation_preparations[0].clone();
    let new_birth = read_birth(&store.connection, &successor)?;
    assert_eq!(new_birth.origin, original.origin);
    assert_eq!(
        new_birth.receipt.authorization,
        IntentAuthorization::Renewal {
            predecessor: original.id.clone()
        }
    );
    assert_eq!(
        read_terminal(&store.connection, &original.id)?
            .unwrap()
            .reason,
        IntentTerminalReason::Renewed {
            successor: successor.clone()
        }
    );
    assert!(
        matches!(&new_birth.receipt.input_binding, IntentInputBinding::Measured { binding } if binding.right.is_none())
    );
    assert!(store.cancel_generation_intent(&successor, &store.head_revision()?)?);
    store.validate_full()?;
    drop(store);
    let store = ProjectStore::open(&package, crate::AccessMode::ReadOnly)?;
    store.validate_full()?;
    assert_eq!(
        read_terminal(&store.connection, &original.id)?
            .unwrap()
            .reason,
        IntentTerminalReason::Renewed { successor }
    );
    assert!(store.generation_intents(None, 1)?.is_empty());
    Ok(())
}

#[test]
fn retired_birth_receipt_tampering_is_rejected_by_full_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store =
        ProjectStore::create(&scratch.path().join("retired-tamper.deadpan"), &document())?;
    let value = insert(&mut store, "ai")?;
    fulfil(&mut store, &value, "candidate")?;
    crate::generation_preparations::compact_completed_for_test(&store.connection)?;
    crate::audit::refresh_generation_scopes(&store.connection)?;
    store.validate_full()?;
    let json: String = store.connection.query_row(
        "SELECT record FROM generation_preparation_retirements WHERE id=?1",
        [value.id.as_str()],
        |row| row.get(0),
    )?;
    let needle = "\"cause\":{\"kind\":\"inserted_pause\"}";
    assert_eq!(json.matches(needle).count(), 1);
    let changed = json.replacen(needle, "\"cause\":{\"kind\":\"duration_extension\"}", 1);
    store.connection.execute(
        "UPDATE generation_preparation_retirements SET record=?1 WHERE id=?2",
        params![changed, value.id.as_str()],
    )?;
    assert!(store.validate_full().is_err());
    Ok(())
}

#[test]
fn head_charge_reserves_identity_and_fulfilment_growth() -> Result {
    let mut head = IntentHead {
        activation_id: PreparationId::new("activation")?,
        target: ScopedNodeTarget {
            node: id("hold"),
            repeats: vec![deadpan_core::RepeatEditStep {
                repeat: id("repeat"),
                branch: deadpan_core::RepeatEditBranch::Default,
            }],
        },
        request_id: None,
    };
    let initial = head_charge(&head)?;
    head.target.node = id(&"n".repeat(deadpan_core::MAX_IDENTITY_BYTES));
    head.target.repeats[0].repeat = id(&"r".repeat(deadpan_core::MAX_IDENTITY_BYTES));
    head.request_id = Some(RequestId::new(
        "q".repeat(deadpan_jobs::MAX_PROTOCOL_ID_BYTES),
    )?);
    assert_eq!(head_charge(&head)?, initial);
    Ok(())
}

#[test]
fn capacity_index_is_bounded_and_ordered_for_one_hundred_thousand_heads() -> Result {
    let mut capacity = ActivationCapacity::default();
    let count = deadpan_core::MAX_DOCUMENT_NODES;
    for index in (0..count).rev() {
        capacity.insert(
            PreparationId::new(format!("activation-{index:06}"))?,
            (index % 7) as i64,
            100,
        )?;
    }
    assert_eq!(capacity.charges.len(), count);
    assert_eq!(capacity.order.len(), count);
    assert_eq!(capacity.total, count * 100);
    let mut previous: Option<(i64, PreparationId)> = None;
    while let Some(id) = capacity.oldest().cloned() {
        let (order, _) = capacity.charges[&id];
        let key = (order, id.clone());
        if let Some(previous) = &previous {
            assert!(previous < &key);
        }
        previous = Some(key);
        capacity.remove(&id);
    }
    assert_eq!(capacity.total, 0);
    assert!(capacity.charges.is_empty());
    assert!(capacity.order.is_empty());
    Ok(())
}

#[test]
fn wrapping_current_fallback_renews_only_its_exact_retained_hold() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("wrapped.deadpan"), &document())?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    let original = insert(&mut store, "ai")?;
    let outcome = store.commit(&command(
        &store,
        "wrap",
        Command::WrapRepeat {
            node: original.target.node.clone(),
            id: id("repeat"),
            plays: 1,
            gap: None,
            anchor_policy: Default::default(),
        },
    ))?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    let fresh = read_birth(&store.connection, &outcome.generation_preparations[0])?;
    assert_eq!(fresh.origin_target.node, original.target.node);
    assert_eq!(
        fresh.origin_target.repeats,
        vec![deadpan_core::RepeatEditStep {
            repeat: id("repeat"),
            branch: deadpan_core::RepeatEditBranch::Default
        }]
    );
    assert_eq!(
        fresh.receipt.authorization,
        IntentAuthorization::Renewal {
            predecessor: original.id.clone()
        }
    );
    assert_eq!(store.generation_intents(None, 10)?.len(), 1);
    store.validate_full()?;
    Ok(())
}

#[test]
fn compound_wrap_then_scoped_revert_closes_intent_even_when_fallback_is_identical() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store =
        ProjectStore::create(&scratch.path().join("wrapped-choice.deadpan"), &document())?;
    let original = insert(&mut store, "ai")?;
    let commands = vec![
        Command::WrapRepeat {
            node: original.target.node.clone(),
            id: id("repeat"),
            plays: 1,
            gap: None,
            anchor_policy: Default::default(),
        },
        Command::EditScoped {
            target: ScopedNodeTarget {
                node: original.target.node.clone(),
                repeats: vec![deadpan_core::RepeatEditStep {
                    repeat: id("repeat"),
                    branch: deadpan_core::RepeatEditBranch::Default,
                }],
            },
            edit: deadpan_core::ScopedNodeEdit::RevertGeneratedHold,
            identities: deadpan_core::OccurrenceIdentities {
                nodes: vec![],
                marks: vec![],
            },
        },
    ];
    let steps = commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            Ok(deadpan_core::ResolvedStep::Edit {
                edit: deadpan_core::LeafEdit::new(
                    revision(&format!("choice-leaf-{index}")),
                    command,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let outcome = store.commit(&command(
        &store,
        "wrap-choice",
        Command::Compound {
            transaction: deadpan_core::ResolvedTransaction::new(0, BTreeMap::new(), steps)?,
        },
    ))?;
    assert!(outcome.generation_preparations.is_empty());
    assert!(store.generation_intents(None, 10)?.is_empty());
    assert_eq!(
        read_terminal(&store.connection, &original.id)?
            .unwrap()
            .reason,
        IntentTerminalReason::ProviderChoice
    );
    store.validate_full()?;
    Ok(())
}

#[test]
fn failed_renewal_admission_rolls_back_document_old_head_and_terminal() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("rollback.deadpan"), &document())?;
    let original = insert(&mut store, "ai")?;
    let before = store.snapshot()?;
    let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
        panic!()
    };
    let position = children
        .iter()
        .position(|node| node == &original.target.node)
        .unwrap();
    let request = command(
        &store,
        "remove-right",
        Command::Delete {
            node: children[position + 1].clone(),
        },
    );
    // Fail after renewal has consumed its predecessor, at actual SQLite head
    // admission. The enclosing authored transaction must roll back everything.
    store.connection.execute_batch("CREATE TEMP TRIGGER reject_intent_head BEFORE INSERT ON generation_intent_heads BEGIN SELECT RAISE(ABORT,'fixture intent admission failure'); END;")?;
    assert!(store.commit(&request).is_err());
    assert_eq!(store.snapshot()?, before);
    assert_eq!(
        store.generation_intents(None, 1)?[0].head.activation_id,
        original.id
    );
    assert!(read_terminal(&store.connection, &original.id)?.is_none());
    assert_eq!(
        store.generation_preparation(&original.id)?.unwrap(),
        original
    );
    store
        .connection
        .execute_batch("DROP TRIGGER reject_intent_head;")?;
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    store.validate_full()?;
    Ok(())
}
