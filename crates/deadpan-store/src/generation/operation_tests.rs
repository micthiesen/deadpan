use super::*;
use crate::generation_attempts::{AttemptMutationOutcome, BeginGenerationAttempt};
use crate::generation_inputs::GenerationInputs;
use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, PresentationBasis, ProjectId, Subtree,
};
use deadpan_jobs::{
    AttemptId, AxisLimits, BridgeCapability, CancellationToken, Diagnostic, DimensionLimits,
    FailureCode, FrameCountFormula, JobFailure, JobState, MessageIdentity, NativeDimensions,
    ProtocolVersion, StageProgress, WorkerFailure, WorkerMessage, WorkerStage,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn document() -> Result<ProjectDocument> {
    let mut document = ProjectDocument::new(
        ProjectId::new("operation-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 512,
            height: 320,
            frame_rate: FrameRate::new(24, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    for (index, (name, frames)) in [("left", 24), ("hold", 12), ("right", 24)]
        .into_iter()
        .enumerate()
    {
        let id = NodeId::new(name)?;
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(format!("setup-{index}"))?,
            command: Command::Insert {
                parent: document.root().clone(),
                index,
                subtree: Subtree {
                    root: id.clone(),
                    nodes: BTreeMap::from([(
                        id,
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                duration: FrameDuration::new(frames)?,
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
        document = deadpan_core::apply(&document, &request)?
            .forward
            .apply(&document)?;
    }
    Ok(document)
}

fn extension(right: bool) -> GenerationPlan {
    let mut wire: serde_json::Value = serde_json::from_str(include_str!(
        "../../../deadpan-jobs/tests/fixtures/generate_extension_v3.json"
    ))
    .unwrap();
    if right {
        wire["plan"]["sampling"]["direction"] = "from_right".into();
    }
    GenerationPlan::Extension(serde_json::from_value(wire["plan"].clone()).unwrap())
}

fn bridge() -> BridgeGenerationPlan {
    BridgeGenerationPlan::new(
        FrameDuration::new(12).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1).unwrap(),
            FrameCountFormula::new(1, 0, 2, 97).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(512, 512, 1).unwrap(),
                AxisLimits::new(320, 320, 1).unwrap(),
            ),
        ),
        NativeDimensions::new(512, 320).unwrap(),
    )
    .unwrap()
}

fn input(store: &ProjectStore, name: &str, plan: &GenerationPlan) -> GenerationRequestInput {
    let wire: serde_json::Value = serde_json::from_str(include_str!(
        "../../../deadpan-jobs/tests/fixtures/generate_extension_v3.json"
    ))
    .unwrap();
    let mut constraints: HoldConstraints =
        serde_json::from_value(wire["constraints"].clone()).unwrap();
    constraints.conditioning = plan.conditioning();
    GenerationRequestInput {
        request_id: RequestId::new(name).unwrap(),
        expected_revision: store.snapshot().unwrap().revision_id().clone(),
        hold_id: NodeId::new("hold").unwrap(),
        context_sha256: Sha256::new("a".repeat(64)).unwrap(),
        constraints,
        provider: serde_json::from_value(wire["provider"].clone()).unwrap(),
    }
}

fn target() -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: NodeId::new("hold").unwrap(),
        repeats: Vec::new(),
    }
}

#[test]
fn bridge_and_both_extensions_capture_and_reopen_real_authored_black_metadata() -> Result {
    for plan in [
        GenerationPlan::Bridge(bridge()),
        extension(false),
        extension(true),
    ] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("operations.deadpan");
        let mut store = ProjectStore::create(&path, &document()?)?;
        let before = store.snapshot()?;
        let request = store.record_scoped_generation_request(
            input(&store, "request", &plan),
            target(),
            plan.clone(),
        )?;
        assert_eq!(request.plan, Some(plan.clone()));
        assert_eq!(
            request.bridge_plan().is_some(),
            matches!(plan, GenerationPlan::Bridge(_))
        );
        let binding = request.input_binding.as_ref().unwrap();
        assert_eq!(binding.duration.frames(), 12);
        match (&plan, &binding.inputs) {
            (GenerationPlan::Bridge(_), GenerationInputs::Bridge { left, right }) => {
                assert_eq!(
                    left,
                    &Some(crate::generation_pictures::GenerationPictureIdentity::AuthoredBlack)
                );
                assert_eq!(left, right);
            }
            (
                GenerationPlan::Extension(_),
                GenerationInputs::Extension {
                    samples,
                    opposite,
                    support,
                    ..
                },
            ) => {
                assert_eq!(samples.len(), 9);
                assert!(samples.iter().all(|sample| sample.picture
                    == crate::generation_pictures::GenerationPictureIdentity::AuthoredBlack));
                assert!(opposite.is_some());
                assert!(!support.is_empty());
            }
            other => panic!("operation mismatch {other:?}"),
        }
        assert_eq!(store.snapshot()?, before);
        validate_store(&store.connection)?;
        drop(store);
        let store = ProjectStore::open(&path, crate::AccessMode::ReadOnly)?;
        assert_eq!(
            store.generation_request(&request.request_id)?,
            Some(request)
        );
    }
    Ok(())
}

#[test]
fn legacy_and_bridge_wrappers_preserve_their_explicit_protocol_contracts() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("wrappers.deadpan"), &document()?)?;
    let plan = GenerationPlan::Bridge(bridge());
    let v1 = store.allocate_generation_request(input(&store, "legacy", &plan))?;
    assert!(v1.plan.is_none() && v1.input_binding.is_none() && v1.bridge_plan().is_none());
    let v2 = store.record_bridge_generation_request(input(&store, "bridge", &plan), bridge())?;
    assert_eq!(v2.bridge_plan(), Some(&bridge()));
    assert!(v2.input_binding.is_some());
    let scoped = store.record_scoped_bridge_generation_request(
        input(&store, "scoped", &plan),
        target(),
        bridge(),
    )?;
    assert_eq!(scoped.plan, v2.plan);
    assert_eq!(scoped.input_binding, v2.input_binding);
    validate_store(&store.connection)?;
    Ok(())
}

#[test]
fn wrong_operation_and_unavailable_context_refuse_before_allocating_request_or_scope() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("refusal.deadpan"), &document()?)?;
    let plan = extension(false);
    let before = store.snapshot()?;
    let mut wrong = input(&store, "wrong", &plan);
    wrong.constraints.conditioning = deadpan_jobs::ConditioningMode::Bridge;
    assert!(
        store
            .record_scoped_generation_request(wrong, target(), plan.clone())
            .is_err()
    );
    // At the actual definition beginning there is no lead-in context. A black
    // recipe cannot manufacture those preceding pictures.
    let mut missing = input(&store, "missing", &plan);
    missing.hold_id = NodeId::new("left")?;
    missing.constraints.video =
        deadpan_jobs::VideoSpec::new(FrameDuration::new(24)?, FrameRate::new(24, 1)?, 512, 320)?;
    let mut wire = serde_json::to_value(plan)?;
    wire["plan"]["sampling"]["output_frame_count"] = 24.into();
    let plan: GenerationPlan = serde_json::from_value(wire)?;
    assert!(
        store
            .record_scoped_generation_request(
                missing,
                ScopedNodeTarget {
                    node: NodeId::new("left")?,
                    repeats: Vec::new()
                },
                plan
            )
            .is_err()
    );
    assert!(store.current_generation_requests()?.is_empty());
    assert_eq!(
        store
            .connection
            .query_row("SELECT COUNT(*) FROM generation_scopes", [], |row| row
                .get::<_, i64>(0))?,
        0
    );
    assert_eq!(store.snapshot()?, before);
    Ok(())
}

#[test]
fn full_validation_reconstructs_origin_instead_of_trusting_stored_input_json() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("origin.deadpan"), &document()?)?;
    let plan = extension(false);
    let request =
        store.record_scoped_generation_request(input(&store, "request", &plan), target(), plan)?;
    let mut binding = request.input_binding.unwrap();
    let GenerationInputs::Extension { samples, .. } = &mut binding.inputs else {
        unreachable!()
    };
    samples[0].position = samples[0]
        .position
        .checked_add(deadpan_core::ExactRatio::integer(1))?;
    store.connection.execute(
        "UPDATE generation_requests SET input_binding=?1",
        [serde_json::to_string(&binding)?],
    )?;
    // The JSON remains well typed and bounded; origin reconstruction rejects
    // the false chronological input, independently of an audit digest.
    assert!(store.generation_request(&request.request_id).is_ok());
    assert!(validate_store(&store.connection).is_err());
    Ok(())
}

#[test]
fn descriptor_size_and_nullability_are_guarded_before_host_reading() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&scratch.path().join("bounds.deadpan"), &document()?)?;
    let plan = extension(false);
    let request =
        store.record_scoped_generation_request(input(&store, "request", &plan), target(), plan)?;
    let oversized = serde_json::to_string(&"x".repeat(MAX_INPUT_BINDING_BYTES))?;
    store.connection.execute(
        "UPDATE generation_requests SET input_binding=?1",
        [oversized],
    )?;
    // Observe the SQL guard itself: the oversized value never reaches row.get.
    let guarded: String =
        store
            .connection
            .query_row(BOUNDED_REQUEST_SELECT, [MAX_IDENTITY_BYTES as i64], |row| {
                row.get(12)
            })?;
    assert_eq!(guarded, "__invalid__");
    assert!(check_stored_sizes(&store.connection).is_err());
    assert!(store.generation_request(&request.request_id).is_err());
    assert!(
        crate::generation_attempts::read_request(&store.connection, &request.request_id).is_err()
    );
    store
        .connection
        .pragma_update(None, "ignore_check_constraints", true)?;
    store
        .connection
        .execute("UPDATE generation_requests SET input_binding=NULL", [])?;
    assert!(store.generation_request(&request.request_id).is_err());
    assert!(check_stored_sizes(&store.connection).is_err());
    Ok(())
}

fn begin(
    store: &mut ProjectStore,
    request: &StoredGenerationRequest,
    name: &str,
) -> Result<(MessageIdentity, CancellationToken)> {
    let identity = MessageIdentity::new(request.request_id.clone(), AttemptId::new(name)?);
    let cancellation = CancellationToken::new(format!("cancel-{name}"))?;
    let attempt = store.begin_generation_attempt(BeginGenerationAttempt {
        identity: identity.clone(),
        cancellation_token: cancellation.clone(),
    })?;
    assert_eq!(attempt.checkpoint.protocol, ProtocolVersion::V3);
    assert_eq!(attempt.checkpoint.state, JobState::Queued);
    Ok((identity, cancellation))
}

#[test]
fn extension_attempts_persist_stages_failure_cancellation_and_restart_without_ready() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("lifecycle.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let plan = extension(false);
    let request =
        store.record_scoped_generation_request(input(&store, "request", &plan), target(), plan)?;
    let (identity, cancellation) = begin(&mut store, &request, "first")?;
    for stage in [
        WorkerStage::Preflight,
        WorkerStage::ModelLoading,
        WorkerStage::Inference,
    ] {
        store.record_generation_worker_message(&WorkerMessage::Stage {
            protocol: ProtocolVersion::V3,
            identity: identity.clone(),
            stage,
        })?;
    }
    let running = store.generation_attempt(&identity)?.unwrap();
    assert_eq!(running.checkpoint.state, JobState::Running);
    assert!(matches!(
        store.record_generation_worker_message(&WorkerMessage::Progress {
            protocol: ProtocolVersion::V3,
            identity: identity.clone(),
            stage: WorkerStage::Inference,
            progress: StageProgress::new(1, 30)?,
        }),
        Err(StoreError::GenerationProgressNotPersistent)
    ));
    // This is an unqualified declaration used solely to prove refusal. No
    // candidate files, validation receipt or Ready state are manufactured.
    let mut declaration: serde_json::Value = serde_json::from_str(include_str!(
        "../../../deadpan-jobs/tests/fixtures/completed_extension_v3.json"
    ))?;
    declaration["identity"] = serde_json::to_value(&identity)?;
    let completed: WorkerMessage = serde_json::from_value(declaration)?;
    assert!(store.record_generation_worker_message(&completed).is_err());
    assert_eq!(store.generation_attempt(&identity)?.unwrap(), running);
    assert!(
        store
            .selected_generation_candidate(&request.request_id)?
            .is_none()
    );
    assert!(
        store
            .selected_generation_bundle(&request.request_id)?
            .is_none()
    );
    store.request_generation_attempt_cancel(&identity, &cancellation)?;
    assert!(store.record_generation_worker_message(&completed).is_err());
    store.record_generation_worker_message(&WorkerMessage::Cancelled {
        protocol: ProtocolVersion::V3,
        identity: identity.clone(),
    })?;
    store.finish_generation_attempt_cancelled(&identity, &cancellation)?;
    assert_eq!(
        store
            .generation_attempt(&identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Cancelled
    );

    let (failed, _) = begin(&mut store, &request, "second")?;
    let failure = WorkerMessage::Failed {
        protocol: ProtocolVersion::V3,
        identity: failed.clone(),
        failure: WorkerFailure {
            code: FailureCode::BackendFailure,
            detail: Diagnostic::new("real worker reported failure")?,
        },
    };
    assert_eq!(
        store.record_generation_worker_message(&failure)?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(
        store.record_generation_worker_message(&failure)?,
        AttemptMutationOutcome::Duplicate
    );
    let WorkerMessage::Failed {
        identity, failure, ..
    } = failure
    else {
        unreachable!()
    };
    assert!(
        store
            .record_generation_worker_message(&WorkerMessage::Failed {
                protocol: ProtocolVersion::V2,
                identity,
                failure
            })
            .is_err()
    );
    let (interrupted, _) = begin(&mut store, &request, "third")?;
    drop(store);
    let store = ProjectStore::open(&path, crate::AccessMode::ReadWrite)?;
    let recovered = store.generation_attempt(&interrupted)?.unwrap();
    assert_eq!(recovered.checkpoint.protocol, ProtocolVersion::V3);
    assert_eq!(recovered.checkpoint.state, JobState::Failed);
    assert!(
        matches!(recovered.checkpoint.failure, Some(JobFailure::Host(ref error)) if error.code == deadpan_jobs::HostFailureCode::Interrupted)
    );
    assert!(
        recovered.receipt.is_none() && recovered.bundle_receipt.is_none() && !recovered.selected
    );
    Ok(())
}
