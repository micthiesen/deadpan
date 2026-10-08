//! Synthetic byte bundles exercise store admission, not media qualification.
//! Requests capture actual authored Background Holds; no decoded image claim is made.
use super::*;
use deadpan_core::{ExtensionDirection, GeneratedSamplingMap, ScopedNodeTarget};
use deadpan_jobs::{ExtensionCapability, ExtensionGenerationPlan, GenerationPlan};
use deadpan_store::generation_retention::{DEFAULT_VARIANT_RETENTION, ExpiryMode};
use deadpan_store::storage::{CleanupPolicy, EntryState};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

fn extension_plan(direction: ExtensionDirection) -> ExtensionGenerationPlan {
    ExtensionGenerationPlan::new(
        direction,
        FrameDuration::new(12).unwrap(),
        rate(),
        &ExtensionCapability::new(
            FrameRate::new(24, 1).unwrap(),
            9,
            FrameCountFormula::new(8, 0, 8, 96).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(512, 512, 1).unwrap(),
                AxisLimits::new(320, 320, 1).unwrap(),
            ),
            FrameDuration::new(120).unwrap(),
        )
        .unwrap(),
        NativeDimensions::new(512, 320).unwrap(),
    )
    .unwrap()
}

fn authored(direction: ExtensionDirection, opposite: bool) -> Result<ProjectDocument> {
    let mut document = document()?;
    let left = direction == ExtensionDirection::FromLeft || opposite;
    let right = direction == ExtensionDirection::FromRight || opposite;
    for (name, index, present) in [
        ("left", 0, left),
        ("right", if left { 2 } else { 1 }, right),
    ] {
        if !present {
            continue;
        }
        let id = NodeId::new(name)?;
        document = deadpan_core::apply(
            &document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(format!("add-{name}"))?,
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
                                    duration: FrameDuration::new(60)?,
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
            },
        )?
        .forward
        .apply(&document)?;
    }
    Ok(document)
}

fn request(
    store: &mut ProjectStore,
    id: &str,
    direction: ExtensionDirection,
) -> Result<StoredGenerationRequest> {
    let plan = extension_plan(direction);
    let mut constraints = constraints();
    constraints.conditioning = GenerationPlan::Extension(plan.clone()).conditioning();
    Ok(store.record_scoped_generation_request(
        GenerationRequestInput {
            request_id: RequestId::new(id)?,
            expected_revision: store.head_revision()?,
            hold_id: NodeId::new("hold")?,
            context_sha256: sha('a'),
            constraints,
            provider: provider(13),
        },
        ScopedNodeTarget {
            node: NodeId::new("hold")?,
            repeats: Vec::new(),
        },
        plan.into(),
    )?)
}

fn span(video: &VideoSpec) -> SourceSpan {
    let rate = video.frame_rate();
    let base = SourceTimeBase::new(rate.denominator(), rate.numerator()).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: base,
        },
        SourceTimestamp {
            ticks: video.frames().frames(),
            time_base: base,
        },
    )
    .unwrap()
}

struct Variant {
    identity: MessageIdentity,
    candidate: NativeCandidateManifest,
    receipt: BundleValidationReceipt,
    bytes: Vec<Vec<u8>>,
}

fn variant(
    store: &mut ProjectStore,
    request: &StoredGenerationRequest,
    ordinal: u64,
    repeated: bool,
) -> Result<Variant> {
    let Some(GenerationPlan::Extension(plan)) = &request.plan else {
        panic!("extension request")
    };
    let identity = begin(store, request, &format!("attempt-{ordinal}"))?;
    let candidate = NativeCandidateManifest {
        native: WorkspaceArtifact::new(WorkspaceRef::new("outputs/native.mp4")?, sha('c'), 101)?,
        provenance: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/provenance.json")?,
            sha('d'),
            202,
        )?,
        video: VideoSpec::new(
            FrameDuration::new(i64::from(plan.native_frame_count()))?,
            plan.native_frame_rate(),
            512,
            320,
        )?,
        provider: request.provider.for_attempt(ordinal),
    };
    stage(
        store,
        &identity,
        ProtocolVersion::V3,
        WorkerStage::Preflight,
    )?;
    stage(
        store,
        &identity,
        ProtocolVersion::V3,
        WorkerStage::Inference,
    )?;
    store.record_generation_worker_message(&WorkerMessage::CompletedExtension {
        protocol: ProtocolVersion::V3,
        identity: identity.clone(),
        candidate: candidate.clone(),
    })?;
    let mut bytes: Vec<_> = ["native", "sampled", "provenance"]
        .into_iter()
        .map(|kind| format!("extension-{kind}-{ordinal}").into_bytes())
        .collect();
    bytes.push(b"retained extension manifest".to_vec());
    let context: Vec<_> = (0..plan.context_frame_count())
        .map(|index| {
            format!(
                "retained context picture {}",
                if repeated { 0 } else { index }
            )
            .into_bytes()
        })
        .collect();
    let context_objects = context.iter().map(|bytes| object(bytes)).collect();
    bytes.extend(context);
    let has_opposite = matches!(
        &request.input_binding.as_ref().unwrap().inputs,
        deadpan_store::generation_inputs::GenerationInputs::Extension {
            opposite: Some(_),
            ..
        }
    );
    let opposite = has_opposite.then(|| b"retained opposite seam".to_vec());
    let opposite_object = opposite.as_ref().map(|bytes| object(bytes));
    bytes.extend(opposite);
    bytes.push(b"retained context signatures".to_vec());
    let inputs = BundleInputObjects::new_extension(
        request.binding.context_sha256.clone(),
        object(&bytes[3]),
        context_objects,
        opposite_object,
        object(bytes.last().unwrap()),
    )?;
    let receipt = BundleValidationReceipt::new_extension(
        &candidate,
        object(&bytes[0]),
        object(&bytes[1]),
        object(&bytes[2]),
        request.constraints.video.clone(),
        plan.clone(),
        ValidatorIdentity::new("synthetic-store-byte-check", "extension-1")?,
        BundleAdmissionEvidence::new(
            span(&candidate.video),
            span(&request.constraints.video),
            inputs,
        )?,
    )?;
    Ok(Variant {
        identity,
        candidate,
        receipt,
        bytes,
    })
}

fn publish(store: &mut ProjectStore, variant: &Variant, omit: Option<usize>) -> Result {
    for (index, bytes) in variant.bytes.iter().enumerate() {
        if omit == Some(index) {
            continue;
        }
        store.promote_generated_object(&mut Cursor::new(bytes), &object(bytes), media_limits())?;
    }
    Ok(())
}

fn ready(store: &mut ProjectStore, variant: &Variant) -> Result {
    store.record_generation_bundle_ready(
        &variant.identity,
        &variant.candidate,
        variant.receipt.clone(),
        media_limits(),
    )?;
    Ok(())
}

fn acceptance(store: &ProjectStore, variant: &Variant) -> Result<GenerationAcceptance> {
    Ok(GenerationAcceptance {
        expected_revision: store.head_revision()?,
        new_revision: RevisionId::new("accepted-extension")?,
        identity: variant.identity.clone(),
        expected_receipt: variant.receipt.clone(),
        native_asset: AssetId::new("native-extension")?,
        sampled_asset: AssetId::new("sampled-extension")?,
    })
}

#[test]
fn both_directions_and_optional_opposite_accept_undo_reopen_and_copy_every_input() -> Result {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for opposite in [false, true] {
            let scratch = tempfile::tempdir()?;
            let package = scratch.path().join("extension.deadpan");
            let mut store = ProjectStore::create(&package, &authored(direction, opposite)?)?;
            let original = store.snapshot()?;
            let request = request(&mut store, "request", direction)?;
            let variant = variant(&mut store, &request, 1, true)?;
            publish(&mut store, &variant, None)?;
            ready(&mut store, &variant)?;
            assert_eq!(store.snapshot()?, original, "Ready cannot author an edit");
            assert_eq!(
                store.record_generation_bundle_ready(
                    &variant.identity,
                    &variant.candidate,
                    variant.receipt.clone(),
                    media_limits()
                )?,
                AttemptMutationOutcome::Duplicate
            );
            let input = acceptance(&store, &variant)?;
            store.accept_generation_bundle(
                &input,
                &unchanged_relevance(&store, &input.new_revision)?,
                media_limits(),
            )?;
            let accepted = store.snapshot()?;
            let NodeKind::Hold { recipe } = &accepted.nodes()[&NodeId::new("hold")?].kind else {
                panic!("Hold")
            };
            let HoldVideo::Generated { accepted } = &recipe.video else {
                panic!("accepted generated video")
            };
            assert!(matches!(
                accepted.artifact.sampling,
                GeneratedSamplingMap::Extension(_)
            ));
            assert_eq!(accepted.artifact.sampling, variant.receipt.sampling_map()?);
            let artifact = accepted.artifact.clone();
            assert!(store.accepted_generation_origin(&artifact)?.is_some());
            let undo = RevisionId::new("undo-extension")?;
            store.undo_reconciled(
                &input.new_revision,
                undo.clone(),
                &unchanged_relevance(&store, &undo)?,
            )?;
            assert!(store.snapshot()?.assets().is_empty());
            store.discard_generation_bundle_variant(&variant.identity)?;
            store.validate_full()?;
            assert!(
                store
                    .clean_storage(CleanupPolicy::everything(Duration::ZERO, true))?
                    .removed
                    .is_empty()
            );
            drop(store);
            let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
            assert!(store.accepted_generation_origin(&artifact)?.is_some());
            for bytes in &variant.bytes {
                store.snapshot_generated_object(&object(bytes), media_limits())?;
            }
            drop(store);
            let copy = scratch.path().join("copy.deadpan");
            deadpan_store::portable::copy_portable(&package, &copy, &AtomicBool::new(false))?;
            let mut store = ProjectStore::open(&copy, AccessMode::ReadWrite)?;
            for bytes in &variant.bytes {
                store.snapshot_generated_object(&object(bytes), media_limits())?;
            }
            let redo = RevisionId::new("redo-extension")?;
            store.redo_reconciled(&undo, redo.clone(), &unchanged_relevance(&store, &redo)?)?;
            assert_eq!(store.snapshot()?.assets().len(), 2);
            store.validate_full()?;
        }
    }
    Ok(())
}

#[test]
fn every_missing_input_prevents_ready_without_selection_or_receipt() -> Result {
    // Three outputs, manifest, nine context pictures, opposite and signatures.
    for missing in 3..15 {
        let scratch = tempfile::tempdir()?;
        let mut store = ProjectStore::create(
            &scratch.path().join("missing.deadpan"),
            &authored(ExtensionDirection::FromRight, true)?,
        )?;
        let request = request(&mut store, "request", ExtensionDirection::FromRight)?;
        let variant = variant(&mut store, &request, 1, false)?;
        assert_eq!(variant.bytes.len(), 15);
        publish(&mut store, &variant, Some(missing))?;
        assert!(
            ready(&mut store, &variant).is_err(),
            "missing input {missing}"
        );
        let attempt = store.generation_attempt(&variant.identity)?.unwrap();
        assert_eq!(attempt.checkpoint.state, JobState::Validating);
        assert!(attempt.bundle_receipt.is_none() && !attempt.selected);
        publish(&mut store, &variant, None)?;
        ready(&mut store, &variant)?;
    }
    Ok(())
}

#[test]
fn extension_wire_rejects_missing_evidence_wrong_operation_aliases_and_oversized_context() -> Result
{
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(
        &scratch.path().join("wire.deadpan"),
        &authored(ExtensionDirection::FromLeft, true)?,
    )?;
    let request = request(&mut store, "request", ExtensionDirection::FromLeft)?;
    let variant = variant(&mut store, &request, 1, false)?;
    let value = serde_json::to_value(&variant.receipt)?;
    assert_eq!(
        serde_json::from_value::<BundleValidationReceipt>(value.clone())?,
        variant.receipt
    );
    for field in ["opposite", "signatures", "context", "operation"] {
        let mut bad = value.clone();
        bad["admission"]["inputs"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<BundleValidationReceipt>(bad).is_err(),
            "missing {field}"
        );
    }
    for field in ["opposite", "signatures"] {
        let mut bad = value.clone();
        bad["admission"]["inputs"][field] = value["admission"]["inputs"]["manifest"].clone();
        assert!(serde_json::from_value::<BundleValidationReceipt>(bad).is_err());
    }
    let mut bad = value.clone();
    bad.as_object_mut().unwrap().remove("admission");
    assert!(serde_json::from_value::<BundleValidationReceipt>(bad).is_err());
    let mut bad = value.clone();
    bad["admission"]["inputs"] = serde_json::to_value(admission(sha('a')).inputs())?;
    assert!(serde_json::from_value::<BundleValidationReceipt>(bad).is_err());
    let mut bad = value.clone();
    bad["admission"]["inputs"]["context"] =
        serde_json::json!(vec![value["admission"]["inputs"]["context"][0].clone(); 65]);
    assert!(serde_json::from_value::<BundleValidationReceipt>(bad).is_err());
    let mut bad = value;
    bad["admission"]["inputs"]["context"][1] = bad["admission"]["inputs"]["context"][0].clone();
    bad["admission"]["inputs"]["context"][1]["byte_length"] = 999.into();
    assert!(serde_json::from_value::<BundleValidationReceipt>(bad).is_err());
    Ok(())
}

#[test]
fn captured_opposite_provider_direction_and_declaration_must_match_before_ready() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(
        &scratch.path().join("binding.deadpan"),
        &authored(ExtensionDirection::FromLeft, true)?,
    )?;
    let request = request(&mut store, "request", ExtensionDirection::FromLeft)?;
    let variant = variant(&mut store, &request, 1, false)?;
    publish(&mut store, &variant, None)?;
    let original = serde_json::to_value(&variant.receipt)?;
    for change in ["opposite", "direction", "seed", "context"] {
        let mut bad = original.clone();
        match change {
            "opposite" => bad["admission"]["inputs"]["opposite"] = serde_json::Value::Null,
            "direction" => {
                bad["plan"] = serde_json::to_value(GenerationPlan::Extension(extension_plan(
                    ExtensionDirection::FromRight,
                )))?
            }
            "seed" => bad["provider"]["seed"] = 999.into(),
            "context" => {
                bad["admission"]["inputs"]["context_sha256"] = serde_json::to_value(sha('e'))?
            }
            _ => unreachable!(),
        }
        let receipt: BundleValidationReceipt = serde_json::from_value(bad)?;
        assert!(
            store
                .record_generation_bundle_ready(
                    &variant.identity,
                    &variant.candidate,
                    receipt,
                    media_limits()
                )
                .is_err(),
            "{change}"
        );
    }
    let mut changed = variant.candidate.clone();
    changed.provider.seed += 1;
    assert!(
        store
            .record_generation_bundle_ready(
                &variant.identity,
                &changed,
                variant.receipt.clone(),
                media_limits()
            )
            .is_err()
    );
    assert!(
        store
            .record_generation_worker_message(&WorkerMessage::CompletedBridge {
                protocol: ProtocolVersion::V2,
                identity: variant.identity.clone(),
                candidate: variant.candidate.clone()
            })
            .is_err()
    );
    assert_eq!(
        store
            .generation_attempt(&variant.identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Validating
    );
    ready(&mut store, &variant)?;
    Ok(())
}

#[test]
fn expiry_does_not_treat_shared_inputs_as_accepted_and_missing_input_refuses_portable_copy()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("retention.deadpan");
    let mut store = ProjectStore::create(&package, &authored(ExtensionDirection::FromLeft, true)?)?;
    let request = request(&mut store, "request", ExtensionDirection::FromLeft)?;
    let first = variant(&mut store, &request, 1, false)?;
    publish(&mut store, &first, None)?;
    ready(&mut store, &first)?;
    let second = variant(&mut store, &request, 2, false)?;
    publish(&mut store, &second, None)?;
    ready(&mut store, &second)?;
    let expiry = store.expire_generation_variants(
        SystemTime::now() + DEFAULT_VARIANT_RETENTION + Duration::from_secs(1),
        DEFAULT_VARIANT_RETENTION,
        ExpiryMode::Explicit,
        false,
    )?;
    assert_eq!(expiry.expired.len(), 1);
    assert_eq!(
        expiry.expired[0].attempt_id,
        first.identity.attempt_id.as_str()
    );
    let report = store.storage_report(Duration::ZERO)?;
    for bytes in &first.bytes[..3] {
        assert_eq!(storage::state(&report, bytes), EntryState::Unreferenced);
    }
    for bytes in &second.bytes {
        assert!(matches!(
            storage::state(&report, bytes),
            EntryState::Referenced { .. }
        ));
    }
    let signature = second
        .receipt
        .admission()
        .unwrap()
        .inputs()
        .signatures()
        .unwrap();
    let path = stored_path(&package, signature);
    drop(store);
    fs::remove_file(path)?;
    let destination = scratch.path().join("missing-copy.deadpan");
    assert!(
        deadpan_store::portable::copy_portable(&package, &destination, &AtomicBool::new(false))
            .is_err()
    );
    assert!(!destination.exists());
    Ok(())
}

#[test]
fn superseded_extension_can_be_retained_ready_but_cannot_select_or_accept() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = ProjectStore::create(
        &scratch.path().join("stale.deadpan"),
        &authored(ExtensionDirection::FromLeft, true)?,
    )?;
    let old = request(&mut store, "old", ExtensionDirection::FromLeft)?;
    let variant = variant(&mut store, &old, 1, false)?;
    publish(&mut store, &variant, None)?;
    request(&mut store, "new", ExtensionDirection::FromLeft)?;
    ready(&mut store, &variant)?;
    assert!(
        !store
            .generation_attempt(&variant.identity)?
            .unwrap()
            .selected
    );
    assert!(store.selected_generation_bundle(&old.request_id)?.is_none());
    assert!(
        store
            .select_generation_bundle_variant(&variant.identity)
            .is_err()
    );
    assert!(
        store
            .preview_generation_acceptance(&acceptance(&store, &variant)?, media_limits())
            .is_err()
    );
    Ok(())
}

#[test]
fn acceptance_rechecks_retained_input_bytes_and_receipt_reads_enforce_metadata_bound() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("recheck.deadpan");
    let mut store =
        ProjectStore::create(&package, &authored(ExtensionDirection::FromRight, false)?)?;
    let request = request(&mut store, "request", ExtensionDirection::FromRight)?;
    let variant = variant(&mut store, &request, 1, false)?;
    publish(&mut store, &variant, None)?;
    ready(&mut store, &variant)?;
    let before = store.snapshot()?;
    let input = acceptance(&store, &variant)?;
    let signatures = variant
        .receipt
        .admission()
        .unwrap()
        .inputs()
        .signatures()
        .unwrap();
    fs::remove_file(stored_path(&package, signatures))?;
    assert!(
        store
            .accept_generation_bundle(
                &input,
                &unchanged_relevance(&store, &input.new_revision)?,
                media_limits()
            )
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    publish(&mut store, &variant, None)?;
    let mut wire = serde_json::to_value(&variant.receipt)?;
    wire["padding"] = "x".repeat(32 * 1024).into();
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute(
        "UPDATE generation_bundle_receipts SET bundle=?1",
        [serde_json::to_string(&wire)?],
    )?;
    assert!(
        matches!(store.generation_attempt(&variant.identity), Err(StoreError::Integrity(message))
        if message.contains("stored bundle validation receipt exceeds its bounds"))
    );
    assert!(store.validate().is_err());
    Ok(())
}
