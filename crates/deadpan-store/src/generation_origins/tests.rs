use super::*;
use std::collections::BTreeMap;

use crate::generation_attempts::{BundleAdmissionEvidence, BundleInputObjects, ValidatorIdentity};
use crate::generation_inputs::GenerationInputs;
use crate::generation_pictures::{GenerationPictureIdentity, GenerationPictures};
use deadpan_core::{
    AssetId, AssetRecord, BeatNode, BoundaryQueryLimits, ColorPolicy, CommandRequest,
    FrameDuration, FrameRate, GeneratedContentId, GeneratedObjectRef, HoldAudio, HoldRecipe,
    HoldVideo, NodeId, NodeKind, OccurrenceIdentities, PresentationBasis, SourceSpan,
    SourceTimeBase, SourceTimestamp, Subtree,
};
use deadpan_jobs::{
    AttemptId, AxisLimits, BridgeCapability, BridgeGenerationPlan, ConditioningMode,
    DimensionLimits, FrameCountFormula, MotionAmount, NativeDimensions, ProviderPackId,
    ProviderPackVersion, Relevance, RuntimeId, RuntimeVersion, TargetBinding, VideoSpec,
    WorkspaceArtifact, WorkspaceRef,
};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn rate() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn sha(value: char) -> ContentSha256 {
    ContentSha256::new(value.to_string().repeat(64)).unwrap()
}
fn object(value: char) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(value.to_string().repeat(64)).unwrap(),
        100,
    )
    .unwrap()
}
fn span(count: FrameDuration, rate: FrameRate) -> SourceSpan {
    let clock = SourceTimeBase::new(rate.denominator(), rate.numerator()).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: clock,
        },
        SourceTimestamp {
            ticks: count.frames(),
            time_base: clock,
        },
    )
    .unwrap()
}
fn request(document: &ProjectDocument, command: Command, next: &str) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}
fn apply(document: &ProjectDocument, command: Command, next: &str) -> ProjectDocument {
    deadpan_core::apply(document, &request(document, command, next))
        .unwrap()
        .forward
        .apply(document)
        .unwrap()
}
fn document(neighbors: bool) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("origins").unwrap(),
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
    let names = if neighbors {
        vec!["left", "hold", "right"]
    } else {
        vec!["hold"]
    };
    for (index, name) in names.into_iter().enumerate() {
        document = apply(
            &document,
            Command::Insert {
                parent: id("root"),
                index,
                subtree: Subtree {
                    root: id(name),
                    nodes: BTreeMap::from([(
                        id(name),
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                picture_context: None,
                                duration: frames(12),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            name,
        );
    }
    document
}
fn target() -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: id("hold"),
        repeats: vec![],
    }
}

struct Fixture {
    connection: Connection,
    request: StoredGenerationRequest,
    identity: MessageIdentity,
    artifact: GeneratedArtifact,
    acceptance: CommandRequest,
}

impl Fixture {
    // Valid typed request, bundle and core acceptance, without claiming media
    // decoding. These SQLite metadata fixtures exercise proof validation only.
    fn new(scoped: bool) -> Self {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON;
            CREATE TABLE revisions(id TEXT PRIMARY KEY,parent_id TEXT,kind TEXT,document TEXT);
            CREATE TABLE history(id INTEGER PRIMARY KEY,parent_id INTEGER,revision_id TEXT,request TEXT,edit TEXT);").unwrap();
        crate::generation::create_tables(&connection).unwrap();
        crate::generation_attempts::create_tables(&connection).unwrap();
        crate::generation_retention::create_tables(&connection).unwrap();
        create_tables(&connection).unwrap();
        let original = document(true);
        let origin_revision = original.revision_id().clone();
        connection
            .execute(
                "INSERT INTO revisions VALUES(?1,NULL,'initial',?2)",
                params![origin_revision.as_str(), original.to_json().unwrap()],
            )
            .unwrap();
        let bridge = BridgeGenerationPlan::new(
            frames(12),
            rate(),
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
        .unwrap();
        let constraints = HoldConstraints {
            video: VideoSpec::new(frames(12), rate(), 512, 320).unwrap(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
            instructions: Some(deadpan_jobs::HoldInstructions::new("Keep the position.").unwrap()),
            region_target: None,
        };
        let provider = ProviderSelection {
            pack_id: ProviderPackId::new("pack").unwrap(),
            pack_version: ProviderPackVersion::new("1").unwrap(),
            runtime_id: RuntimeId::new("runtime").unwrap(),
            runtime_version: RuntimeVersion::new("1").unwrap(),
            seed: 11,
        };
        let request_id = RequestId::new("request").unwrap();
        let stored = StoredGenerationRequest {
            request_id: request_id.clone(),
            origin_revision: origin_revision.clone(),
            binding: TargetBinding {
                project_id: original.project_id().clone(),
                hold_id: id("hold"),
                request_version: RequestVersion::new(1).unwrap(),
                context_sha256: sha('a'),
            },
            scope_id: GenerationScopeId::from_first_request(request_id.clone()),
            origin_target: target(),
            target: target(),
            constraints: constraints.clone(),
            provider: provider.clone(),
            plan: Some(GenerationPlan::Bridge(bridge.clone())),
            input_binding: Some(
                crate::generation_inputs::GenerationInputCapture::capture(
                    &original,
                    &target(),
                    &QualifiedGenerationPictures::new(&connection),
                )
                .unwrap(),
            ),
            relevance: Relevance::Current,
        };
        let target_json = serde_json::to_string(&target()).unwrap();
        connection
            .execute(
                "INSERT INTO generation_scopes VALUES(?1,?2,?3,?3,1)",
                params![request_id.as_str(), origin_revision.as_str(), target_json],
            )
            .unwrap();
        connection.execute("INSERT INTO generation_requests(request_id,project_id,hold_id,scope_id,origin_target,
            request_version,origin_revision,context_sha256,constraints,provider,plan,input_binding,relevance)
            VALUES(?1,'origins','hold',?1,?2,1,?3,?4,?5,?6,?7,?8,'current')",
            params![request_id.as_str(), target_json, origin_revision.as_str(), sha('a').as_str(),
                serde_json::to_string(&constraints).unwrap(), serde_json::to_string(&provider).unwrap(),
                serde_json::to_string(stored.plan.as_ref().unwrap()).unwrap(),
                serde_json::to_string(stored.input_binding.as_ref().unwrap()).unwrap()]).unwrap();
        let identity = MessageIdentity::new(request_id, AttemptId::new("attempt").unwrap());
        let native_video = VideoSpec::new(
            frames(i64::from(bridge.native_frame_count())),
            bridge.native_frame_rate(),
            512,
            320,
        )
        .unwrap();
        let candidate = NativeCandidateManifest {
            native: WorkspaceArtifact::new(
                WorkspaceRef::new("outputs/native.mp4").unwrap(),
                sha('b'),
                100,
            )
            .unwrap(),
            provenance: WorkspaceArtifact::new(
                WorkspaceRef::new("outputs/provenance.json").unwrap(),
                sha('c'),
                100,
            )
            .unwrap(),
            video: native_video.clone(),
            provider,
        };
        let bundle = BundleValidationReceipt::new(
            &candidate,
            object('1'),
            object('2'),
            object('3'),
            constraints.video.clone(),
            bridge.clone(),
            ValidatorIdentity::new("fixture", "1").unwrap(),
        )
        .unwrap()
        .with_admission(
            BundleAdmissionEvidence::new(
                span(native_video.frames(), native_video.frame_rate()),
                span(frames(12), rate()),
                BundleInputObjects::new(sha('a'), object('4'), object('5'), object('6')).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        connection.execute("INSERT INTO generation_attempts(request_id,attempt_id,ordinal,cancellation_token,state,
            transition_sequence,worker_candidate) VALUES('request','attempt',1,'cancel','ready',4,?1)",
            [serde_json::to_string(&candidate).unwrap()]).unwrap();
        connection
            .execute(
                "INSERT INTO generation_bundle_receipts VALUES('request','attempt',?1,'present')",
                [serde_json::to_string(&bundle).unwrap()],
            )
            .unwrap();
        connection.execute("INSERT INTO generation_variant_retention(request_id,attempt_id,ready_at_ms,accepted)
            VALUES('request','attempt',0,1)", []).unwrap();
        let artifact = GeneratedArtifact {
            sampled_asset: AssetId::new("sampled").unwrap(),
            sampled_object: object('2'),
            native_asset: AssetId::new("native").unwrap(),
            native_object: object('1'),
            provenance: object('3'),
            sampling: bridge.sampling_map().unwrap(),
            content_aspect: Some([512, 320]),
        };
        let asset = |object: GeneratedObjectRef, video: &VideoSpec| AssetRecord {
            label: format!("Generated {}", object.content().digest()),
            content_hash: object.content().to_string(),
            video: Some(span(video.frames(), video.frame_rate())),
            audio: None,
            still_image: false,
            frame_count: Some(video.frames()),
            source_qualification: None,
        };
        let assets = BTreeMap::from([
            (
                artifact.sampled_asset.clone(),
                asset(object('2'), &constraints.video),
            ),
            (
                artifact.native_asset.clone(),
                asset(object('1'), &native_video),
            ),
        ]);
        let command = if scoped {
            Command::EditScoped {
                target: target(),
                edit: ScopedNodeEdit::AcceptGeneratedHold {
                    artifact: artifact.clone(),
                    assets,
                },
                identities: OccurrenceIdentities {
                    nodes: vec![],
                    marks: vec![],
                },
            }
        } else {
            Command::AcceptGeneratedHold {
                node: id("hold"),
                artifact: artifact.clone(),
                assets,
            }
        };
        let acceptance = request(&original, command, "accepted");
        let edit = deadpan_core::apply(&original, &acceptance).unwrap();
        let accepted = edit.forward.apply(&original).unwrap();
        connection
            .execute(
                "INSERT INTO revisions VALUES('accepted',?1,'edit',?2)",
                params![origin_revision.as_str(), accepted.to_json().unwrap()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO history VALUES(1,NULL,'accepted',?1,?2)",
                params![
                    serde_json::to_string(&acceptance).unwrap(),
                    serde_json::to_string(&edit).unwrap()
                ],
            )
            .unwrap();
        Self {
            connection,
            request: stored,
            identity,
            artifact,
            acceptance,
        }
    }

    fn capture(&self) -> AcceptedOriginReceipt {
        capture(
            &self.connection,
            &self.request,
            &self.identity,
            &self.artifact,
            &revision("accepted"),
        )
        .unwrap()
    }

    fn save(&self) -> AcceptedOriginReceipt {
        let receipt = self.capture();
        insert(&self.connection, &receipt).unwrap();
        receipt
    }

    fn rewrite_with_checksum(&self, receipt: &AcceptedOriginReceipt) {
        let json = canonical(receipt).unwrap();
        self.connection
            .execute(
                "UPDATE generation_accepted_origins SET receipt=?1,receipt_sha256=?2",
                params![json, receipt_digest(json.as_bytes())],
            )
            .unwrap();
    }

    fn copy_acceptance(&self) -> CommandRequest {
        let before = crate::validation::read_revision(&self.connection, "accepted")
            .unwrap()
            .document;
        let copy = request(
            &before,
            Command::AcceptGeneratedHold {
                node: id("left"),
                artifact: self.artifact.clone(),
                assets: BTreeMap::from([
                    (
                        self.artifact.sampled_asset.clone(),
                        before.assets()[&self.artifact.sampled_asset].clone(),
                    ),
                    (
                        self.artifact.native_asset.clone(),
                        before.assets()[&self.artifact.native_asset].clone(),
                    ),
                ]),
            },
            "copied-acceptance",
        );
        let edit = deadpan_core::apply(&before, &copy).unwrap();
        let after = edit.forward.apply(&before).unwrap();
        self.connection
            .execute(
                "INSERT INTO revisions VALUES('copied-acceptance','accepted','edit',?1)",
                [after.to_json().unwrap()],
            )
            .unwrap();
        self.connection
            .execute(
                "INSERT INTO history VALUES(2,1,'copied-acceptance',?1,?2)",
                params![
                    serde_json::to_string(&copy).unwrap(),
                    serde_json::to_string(&edit).unwrap()
                ],
            )
            .unwrap();
        copy
    }
}

#[test]
fn input_binding_distinguishes_definition_edges_from_explicit_black_and_brands_revision() {
    let connection = Connection::open_in_memory().unwrap();
    let pictures = QualifiedGenerationPictures::new(&connection);
    let absent = crate::generation_inputs::GenerationInputCapture::capture(
        &document(false),
        &target(),
        &pictures,
    )
    .unwrap();
    let black = crate::generation_inputs::GenerationInputCapture::capture(
        &document(true),
        &target(),
        &pictures,
    )
    .unwrap();
    assert_eq!(
        absent.inputs,
        GenerationInputs::Bridge {
            left: None,
            right: None
        }
    );
    assert_eq!(
        black.inputs,
        GenerationInputs::Bridge {
            left: Some(GenerationPictureIdentity::AuthoredBlack),
            right: Some(GenerationPictureIdentity::AuthoredBlack)
        }
    );
    let original = document(true);
    let plan = RenderPlan::compile(&original).unwrap();
    let boundaries = plan
        .scoped_hold_boundaries(&target(), BoundaryQueryLimits::default())
        .unwrap();
    let changed = apply(
        &original,
        Command::Rename {
            node: id("hold"),
            label: "renamed".into(),
        },
        "later",
    );
    assert!(
        crate::generation_inputs::GenerationInputCapture::from_boundaries(
            &changed,
            &boundaries,
            &pictures
        )
        .is_err()
    );
}

#[test]
fn missing_measured_input_evidence_is_not_replaced_by_a_guessed_frame() {
    struct Unavailable;
    impl GenerationPictures for Unavailable {
        fn identity(
            &self,
            _: &ProjectDocument,
            _: &deadpan_plan::Picture,
        ) -> Result<GenerationPictureIdentity, StoreError> {
            Err(StoreError::GenerationPlan(
                "qualification unavailable".into(),
            ))
        }
    }
    let error = crate::generation_inputs::GenerationInputCapture::capture(
        &document(true),
        &target(),
        &Unavailable,
    )
    .unwrap_err();
    assert!(error.to_string().contains("qualification unavailable"));
    assert!(
        crate::generation_inputs::GenerationInputCapture::capture(
            &document(false),
            &target(),
            &Unavailable
        )
        .is_ok()
    );

    let asset_id = AssetId::new("unqualified-source").unwrap();
    let source_span = span(frames(12), rate());
    let registered = apply(
        &document(true),
        Command::AddAsset {
            id: asset_id.clone(),
            asset: AssetRecord {
                label: "Unqualified source fixture".into(),
                content_hash: object('8').content().to_string(),
                video: Some(source_span),
                audio: None,
                still_image: false,
                frame_count: Some(frames(12)),
                source_qualification: None,
            },
        },
        "source-registered",
    );
    let original = apply(
        &registered,
        Command::SetHoldProvider {
            node: id("left"),
            video: HoldVideo::Freeze {
                asset: asset_id,
                timestamp: source_span.start(),
            },
        },
        "source-neighbor",
    );
    let connection = Connection::open_in_memory().unwrap();
    let error = crate::generation_inputs::GenerationInputCapture::capture(
        &original,
        &target(),
        &QualifiedGenerationPictures::new(&connection),
    )
    .unwrap_err();
    assert!(error.to_string().contains("no measured qualification"));
}

#[test]
fn exact_origin_round_trips_for_direct_and_scoped_acceptance_and_survives_staleness() {
    for scoped in [false, true] {
        let fixture = Fixture::new(scoped);
        let receipt = fixture.save();
        assert_eq!(
            receipt.options(),
            &GenerationOptions::from_constraints(&fixture.request.constraints)
        );
        assert_eq!(
            read(&fixture.connection, &fixture.artifact).unwrap(),
            Some(receipt.clone())
        );
        insert(&fixture.connection, &receipt).unwrap();
        let before = digest(&fixture.connection).unwrap();
        fixture
            .connection
            .execute("UPDATE generation_requests SET relevance='stale'", [])
            .unwrap();
        assert_eq!(before, digest(&fixture.connection).unwrap());
        validate_store(&fixture.connection, 0).unwrap();
        assert!(
            fixture
                .connection
                .execute("DELETE FROM generation_bundle_receipts", [])
                .is_err()
        );
    }
}

#[test]
fn matching_audit_prefix_skips_origin_document_and_index_reconstruction() {
    let fixture = Fixture::new(false);
    fixture.save();
    crate::validation::REVISION_READS.with(|reads| reads.set(0));
    validate_store(&fixture.connection, 2).unwrap();
    crate::validation::REVISION_READS.with(|reads| assert_eq!(reads.get(), 0));
    validate_store(&fixture.connection, 1).unwrap();
    crate::validation::REVISION_READS.with(|reads| assert!(reads.get() > 0));
}

#[test]
fn discarding_an_accepted_offer_preserves_history_but_cannot_grant_first_admission() {
    let fixture = Fixture::new(false);
    let receipt = fixture.save();
    let before = digest(&fixture.connection).unwrap();
    let json: String = fixture
        .connection
        .query_row("SELECT bundle FROM generation_bundle_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    let mut bundle: serde_json::Value = serde_json::from_str(&json).unwrap();
    bundle["availability"] = serde_json::json!("evicted");
    fixture
        .connection
        .execute(
            "UPDATE generation_bundle_receipts SET bundle=?1,availability='evicted'",
            [serde_json::to_string(&bundle).unwrap()],
        )
        .unwrap();
    fixture
        .connection
        .execute(
            "UPDATE generation_variant_retention SET eviction='discarded',evicted_at_ms=1,picked=0",
            [],
        )
        .unwrap();

    // The offer change invalidates the cached audit dependency, then complete
    // metadata proof still succeeds without reading or reopening model files.
    assert_ne!(before, digest(&fixture.connection).unwrap());
    validate_store(&fixture.connection, 0).unwrap();
    assert_eq!(
        read(&fixture.connection, &fixture.artifact).unwrap(),
        Some(receipt.clone())
    );
    insert(&fixture.connection, &receipt).unwrap();

    assert!(
        capture(
            &fixture.connection,
            &fixture.request,
            &fixture.identity,
            &fixture.artifact,
            &revision("accepted")
        )
        .unwrap_err()
        .to_string()
        .contains("present Ready")
    );
    fixture
        .connection
        .execute("DELETE FROM generation_accepted_origins", [])
        .unwrap();
    assert!(
        insert(&fixture.connection, &receipt)
            .unwrap_err()
            .to_string()
            .contains("present Ready")
    );
    assert!(validate_store(&fixture.connection, 0).is_err());
}

#[test]
fn precommit_origin_proves_inputs_and_ready_bundle_without_granting_authored_acceptance() {
    let fixture = Fixture::new(false);
    fixture
        .connection
        .execute("DELETE FROM history", [])
        .unwrap();
    fixture
        .connection
        .execute("DELETE FROM revisions WHERE id='accepted'", [])
        .unwrap();
    fixture
        .connection
        .execute("UPDATE generation_variant_retention SET accepted=0", [])
        .unwrap();
    let prepared = prepare_acceptance_origin(
        &fixture.connection,
        &fixture.request,
        &fixture.identity,
        &fixture.artifact,
        &revision("accepted"),
    )
    .unwrap();
    assert_eq!(
        prepared.input_binding().inputs,
        GenerationInputs::Bridge {
            left: Some(GenerationPictureIdentity::AuthoredBlack),
            right: Some(GenerationPictureIdentity::AuthoredBlack)
        }
    );
    assert!(
        insert(&fixture.connection, &prepared)
            .unwrap_err()
            .to_string()
            .contains("accepted retention")
    );
    assert!(
        capture(
            &fixture.connection,
            &fixture.request,
            &fixture.identity,
            &fixture.artifact,
            &revision("accepted")
        )
        .is_err()
    );

    let mut changed_request = fixture.request.clone();
    changed_request.constraints.instructions = None;
    assert!(
        prepare_acceptance_origin(
            &fixture.connection,
            &changed_request,
            &fixture.identity,
            &fixture.artifact,
            &revision("accepted")
        )
        .is_err()
    );
    let mut wrong_artifact = fixture.artifact.clone();
    wrong_artifact.native_object = object('9');
    assert!(
        prepare_acceptance_origin(
            &fixture.connection,
            &fixture.request,
            &fixture.identity,
            &wrong_artifact,
            &revision("accepted")
        )
        .is_err()
    );
    wrong_artifact = fixture.artifact.clone();
    wrong_artifact.content_aspect = Some([16, 9]);
    assert!(
        prepare_acceptance_origin(
            &fixture.connection,
            &fixture.request,
            &fixture.identity,
            &wrong_artifact,
            &revision("accepted")
        )
        .is_err()
    );
    fixture
        .connection
        .execute("UPDATE generation_attempts SET state='validating'", [])
        .unwrap();
    assert!(
        prepare_acceptance_origin(
            &fixture.connection,
            &fixture.request,
            &fixture.identity,
            &fixture.artifact,
            &revision("accepted")
        )
        .is_err()
    );
}

#[test]
fn receipt_proves_base_acceptance_when_its_final_boundary_tail_restores_fallback() {
    let mut fixture = Fixture::new(false);
    let before = crate::validation::read_revision(
        &fixture.connection,
        fixture.request.origin_revision.as_str(),
    )
    .unwrap()
    .document;
    let accepted_document = crate::validation::read_revision(&fixture.connection, "accepted")
        .unwrap()
        .document;
    let NodeKind::Hold { recipe } = &accepted_document.nodes()[&id("hold")].kind else {
        panic!("Hold")
    };
    let HoldVideo::Generated { accepted } = &recipe.video else {
        panic!("accepted artifact")
    };
    fixture.acceptance.command = Command::WithBoundaryReplacements {
        edit: deadpan_core::BoundaryReplacementEdit::new(
            fixture.acceptance.command.clone(),
            vec![deadpan_core::BoundaryReplacement {
                target: target(),
                accepted: accepted.clone(),
            }],
        )
        .unwrap(),
    };
    let edit = deadpan_core::apply(&before, &fixture.acceptance).unwrap();
    let final_document = edit.forward.apply(&before).unwrap();
    let NodeKind::Hold { recipe } = &final_document.nodes()[&id("hold")].kind else {
        panic!("Hold")
    };
    assert_eq!(recipe.video, HoldVideo::Background);
    fixture
        .connection
        .execute(
            "UPDATE history SET request=?1,edit=?2 WHERE id=1",
            params![
                serde_json::to_string(&fixture.acceptance).unwrap(),
                serde_json::to_string(&edit).unwrap()
            ],
        )
        .unwrap();
    fixture
        .connection
        .execute(
            "UPDATE revisions SET document=?1 WHERE id='accepted'",
            [final_document.to_json().unwrap()],
        )
        .unwrap();
    fixture
        .connection
        .execute("UPDATE generation_requests SET relevance='stale'", [])
        .unwrap();
    fixture.save();
    validate_store(&fixture.connection, 0).unwrap();
}

#[test]
fn first_admission_binds_both_asset_spans_to_measured_bundle_evidence() {
    for scoped in [false, true] {
        for native in [false, true] {
            let fixture = Fixture::new(scoped);
            let receipt = fixture.save();
            let mut acceptance = fixture.acceptance.clone();
            let assets = match &mut acceptance.command {
                Command::AcceptGeneratedHold { assets, .. }
                | Command::EditScoped {
                    edit: ScopedNodeEdit::AcceptGeneratedHold { assets, .. },
                    ..
                } => assets,
                _ => unreachable!(),
            };
            let asset = assets
                .get_mut(if native {
                    &fixture.artifact.native_asset
                } else {
                    &fixture.artifact.sampled_asset
                })
                .unwrap();
            let measured = asset.video.unwrap();
            let mut start = measured.start();
            let mut end = measured.end();
            start.ticks += 1;
            end.ticks += 1;
            asset.video = Some(SourceSpan::new(start, end).unwrap());
            let before = crate::validation::read_revision(
                &fixture.connection,
                fixture.request.origin_revision.as_str(),
            )
            .unwrap()
            .document;
            // Core correctly treats media measurement as host evidence. This
            // forged metadata remains a valid structural acceptance and patch.
            let edit = deadpan_core::apply(&before, &acceptance).unwrap();
            let after = edit.forward.apply(&before).unwrap();
            fixture
                .connection
                .execute(
                    "UPDATE history SET request=?1,edit=?2 WHERE id=1",
                    params![
                        serde_json::to_string(&acceptance).unwrap(),
                        serde_json::to_string(&edit).unwrap()
                    ],
                )
                .unwrap();
            fixture
                .connection
                .execute(
                    "UPDATE revisions SET document=?1 WHERE id='accepted'",
                    [after.to_json().unwrap()],
                )
                .unwrap();
            assert!(
                validate_store(&fixture.connection, 0)
                    .unwrap_err()
                    .to_string()
                    .contains("exact canonical acceptance")
            );
            assert!(insert(&fixture.connection, &receipt).is_err());
            assert!(
                capture(
                    &fixture.connection,
                    &fixture.request,
                    &fixture.identity,
                    &fixture.artifact,
                    &revision("accepted")
                )
                .is_err()
            );
        }
    }
}

#[test]
fn copied_acceptance_retains_initial_receipt_and_cannot_become_its_admission_witness() {
    let fixture = Fixture::new(false);
    let original = fixture.save();
    let copy = fixture.copy_acceptance();
    validate_store(&fixture.connection, 0).unwrap();
    assert_eq!(
        read(&fixture.connection, &fixture.artifact).unwrap(),
        Some(original.clone())
    );

    let mut forged = original;
    forged.accepted_revision = copy.new_revision.clone();
    assert!(
        validate_admission(&fixture.connection, &forged, BundleUse::Retained)
            .unwrap_err()
            .to_string()
            .contains("fresh asset aliases")
    );
    fixture.rewrite_with_checksum(&forged);
    fixture
        .connection
        .execute(
            "UPDATE generation_accepted_origins SET accepted_revision=?1",
            [copy.new_revision.as_str()],
        )
        .unwrap();
    let order = BTreeMap::from([
        (fixture.request.origin_revision.as_str().to_owned(), 0),
        ("accepted".to_owned(), 1),
        ("copied-acceptance".to_owned(), 2),
    ]);
    assert!(
        require_command_origins(&fixture.connection, &fixture.acceptance.command, 1, &order)
            .unwrap_err()
            .to_string()
            .contains("predates")
    );
    assert!(validate_store(&fixture.connection, 0).is_err());
}

#[test]
fn full_validation_rejects_rehashed_input_or_option_forgery() {
    let fixture = Fixture::new(false);
    let original = fixture.save();
    let mut forged = original.clone();
    let GenerationInputs::Bridge { left, .. } = &mut forged.origin.input_binding.inputs else {
        unreachable!()
    };
    *left = None;
    fixture.rewrite_with_checksum(&forged);
    assert!(
        read(&fixture.connection, &fixture.artifact)
            .unwrap()
            .is_some()
    );
    assert!(
        validate_store(&fixture.connection, 0)
            .unwrap_err()
            .to_string()
            .contains("retained request")
    );
    forged = original;
    forged.options.instructions = None;
    fixture.rewrite_with_checksum(&forged);
    assert!(
        validate_store(&fixture.connection, 0)
            .unwrap_err()
            .to_string()
            .contains("controls")
    );
}

#[test]
fn insertion_rejects_wrong_artifact_alias_object_canvas_and_acceptance_command() {
    let fixture = Fixture::new(false);
    let original = fixture.capture();
    let mut forged = original.clone();
    forged.artifact.native_object = object('7');
    assert!(
        insert(&fixture.connection, &forged)
            .unwrap_err()
            .to_string()
            .contains("bundle")
    );
    forged = original.clone();
    forged.artifact.sampled_asset = AssetId::new("another-alias").unwrap();
    assert!(
        insert(&fixture.connection, &forged)
            .unwrap_err()
            .to_string()
            .contains("acceptance command")
    );
    forged = original.clone();
    forged.artifact.content_aspect = Some([16, 9]);
    assert!(
        insert(&fixture.connection, &forged)
            .unwrap_err()
            .to_string()
            .contains("canvas")
    );
    let mut acceptance = fixture.acceptance.clone();
    acceptance.command = Command::Rename {
        node: id("hold"),
        label: "not acceptance".into(),
    };
    fixture
        .connection
        .execute(
            "UPDATE history SET request=?1",
            [serde_json::to_string(&acceptance).unwrap()],
        )
        .unwrap();
    assert!(
        insert(&fixture.connection, &original)
            .unwrap_err()
            .to_string()
            .contains("acceptance command")
    );
}

#[test]
fn complete_artifact_key_and_insert_conflict_do_not_merge_different_evidence() {
    let fixture = Fixture::new(false);
    let original = fixture.save();
    let mut remapped = fixture.artifact.clone();
    remapped.native_asset = AssetId::new("remapped-native").unwrap();
    assert_ne!(
        artifact_key(&fixture.artifact).unwrap(),
        artifact_key(&remapped).unwrap()
    );
    assert!(read(&fixture.connection, &remapped).unwrap().is_none());
    // A receipt has one immutable witness, not a mutable last-accepted field.
    let acceptance = fixture.copy_acceptance();
    let mut duplicate = original;
    duplicate.accepted_revision = acceptance.new_revision;
    assert!(
        insert(&fixture.connection, &duplicate)
            .unwrap_err()
            .to_string()
            .contains("different origin")
    );
}

#[test]
fn deleting_origin_or_accepted_retention_and_mutating_dependencies_are_detected() {
    let fixture = Fixture::new(false);
    fixture.save();
    let before = digest(&fixture.connection).unwrap();
    let mut constraints = fixture.request.constraints.clone();
    constraints.instructions = None;
    fixture
        .connection
        .execute(
            "UPDATE generation_requests SET constraints=?1",
            [serde_json::to_string(&constraints).unwrap()],
        )
        .unwrap();
    assert_ne!(before, digest(&fixture.connection).unwrap());
    assert!(validate_store(&fixture.connection, 0).is_err());

    let fixture = Fixture::new(false);
    fixture.save();
    fixture
        .connection
        .execute("UPDATE generation_variant_retention SET accepted=0", [])
        .unwrap();
    assert!(validate_store(&fixture.connection, 0).is_err());

    let fixture = Fixture::new(false);
    fixture.save();
    fixture
        .connection
        .execute("DELETE FROM generation_accepted_origins", [])
        .unwrap();
    // A legal escaped JSON tag must not evade the missing-receipt scan.
    let escaped = serde_json::to_string(&fixture.acceptance)
        .unwrap()
        .replace("accept_generated_hold", r"\u0061ccept_generated_hold");
    fixture
        .connection
        .execute("UPDATE history SET request=?1", [escaped])
        .unwrap();
    assert!(
        validate_store(&fixture.connection, 0)
            .unwrap_err()
            .to_string()
            .contains("missing its retained origin")
    );
}

#[test]
fn bounded_canonical_reader_rejects_tampering_before_accepting_a_digest() {
    let fixture = Fixture::new(false);
    fixture.save();
    fixture
        .connection
        .execute(
            "UPDATE generation_accepted_origins SET receipt=receipt || ' '",
            [],
        )
        .unwrap();
    assert!(read(&fixture.connection, &fixture.artifact).is_err());
    fixture
        .connection
        .execute(
            "UPDATE generation_accepted_origins SET receipt=?1",
            [" ".repeat(MAX_ROW_BYTES + 1)],
        )
        .unwrap_err();
    // A valid oversized JSON value reaches the SQL size guard, never serde.
    fixture
        .connection
        .execute(
            "UPDATE generation_accepted_origins SET receipt=?1",
            [format!("\"{}\"", "x".repeat(MAX_ROW_BYTES))],
        )
        .unwrap();
    assert!(
        read(&fixture.connection, &fixture.artifact)
            .unwrap_err()
            .to_string()
            .contains("oversized")
    );
    assert!(check_stored_sizes(&fixture.connection).is_err());
}

#[test]
fn origin_recaptures_inputs_even_when_request_and_rehashed_receipt_agree_on_a_forgery() {
    let fixture = Fixture::new(false);
    let mut forged = fixture.save();
    let original_digest = digest(&fixture.connection).unwrap();
    let GenerationInputs::Bridge { left, .. } = &mut forged.origin.input_binding.inputs else {
        unreachable!()
    };
    *left = None;
    fixture
        .connection
        .execute(
            "UPDATE generation_requests SET input_binding=?1",
            [serde_json::to_string(&forged.origin.input_binding).unwrap()],
        )
        .unwrap();
    fixture.rewrite_with_checksum(&forged);
    assert_ne!(digest(&fixture.connection).unwrap(), original_digest);
    assert!(
        validate_store(&fixture.connection, 0)
            .unwrap_err()
            .to_string()
            .contains("inputs")
    );
    let mut claimed = fixture.request.clone();
    claimed.input_binding = Some(forged.origin.input_binding);
    assert!(
        prepare_acceptance_origin(
            &fixture.connection,
            &claimed,
            &fixture.identity,
            &fixture.artifact,
            &revision("accepted")
        )
        .unwrap_err()
        .to_string()
        .contains("inputs")
    );
}

#[test]
fn removed_selected_region_is_retained_structurally_but_cannot_authorize_an_origin() {
    let mut fixture = Fixture::new(false);
    let region = deadpan_core::TargetId::new("removed-region").unwrap();
    fixture.request.constraints.region_target = Some(region.clone());
    let document = crate::validation::read_revision(
        &fixture.connection,
        fixture.request.origin_revision.as_str(),
    )
    .unwrap()
    .document;
    let binding = crate::generation_inputs::GenerationInputCapture::with_region(
        fixture.request.input_binding.take().unwrap(),
        &document,
        Some(&region),
    )
    .unwrap();
    assert_eq!(binding.region.as_ref().unwrap().record, None);
    fixture
        .connection
        .execute(
            "UPDATE generation_requests SET constraints=?1,input_binding=?2",
            params![
                serde_json::to_string(&fixture.request.constraints).unwrap(),
                serde_json::to_string(&binding).unwrap()
            ],
        )
        .unwrap();
    fixture.request.input_binding = Some(binding);
    assert!(
        prepare_acceptance_origin(
            &fixture.connection,
            &fixture.request,
            &fixture.identity,
            &fixture.artifact,
            &revision("accepted")
        )
        .unwrap_err()
        .to_string()
        .contains("inputs")
    );
}

#[test]
fn origin_requires_a_tagged_bridge_plan_paired_with_bounded_request_inputs() {
    let fixture = Fixture::new(false);
    assert!(
        fixture
            .connection
            .execute("UPDATE generation_requests SET input_binding=NULL", [])
            .is_err()
    );
    let bridge = fixture.request.bridge_plan().unwrap();
    fixture
        .connection
        .execute(
            "UPDATE generation_requests SET plan=?1",
            [serde_json::to_string(bridge).unwrap()],
        )
        .unwrap();
    assert!(
        read_request_origin(&fixture.connection, &fixture.request.request_id).is_err(),
        "an untagged bridge cannot be reinterpreted as the operation wrapper"
    );
    fixture
        .connection
        .execute(
            "UPDATE generation_requests SET plan=?1,input_binding=?2",
            params![
                serde_json::to_string(fixture.request.plan.as_ref().unwrap()).unwrap(),
                format!(
                    "\"{}\"",
                    "x".repeat(crate::generation_inputs::MAX_INPUT_BINDING_BYTES)
                )
            ],
        )
        .unwrap();
    assert!(read_request_origin(&fixture.connection, &fixture.request.request_id).is_err());
    fixture
        .connection
        .execute(
            "UPDATE generation_requests SET plan=NULL,input_binding=NULL",
            [],
        )
        .unwrap();
    assert!(
        read_request_origin(&fixture.connection, &fixture.request.request_id)
            .unwrap_err()
            .to_string()
            .contains("retained Bridge")
    );
}

#[test]
fn extension_request_never_enters_bridge_accepted_origin_admission() {
    use deadpan_core::ExtensionDirection;
    use deadpan_jobs::{ExtensionCapability, ExtensionGenerationPlan};
    let fixture = Fixture::new(false);
    let mut request = fixture.request.clone();
    let dimensions = DimensionLimits::new(
        AxisLimits::new(512, 512, 1).unwrap(),
        AxisLimits::new(320, 320, 1).unwrap(),
    );
    let capability = ExtensionCapability::new(
        FrameRate::new(24, 1).unwrap(),
        9,
        FrameCountFormula::new(8, 0, 8, 16).unwrap(),
        dimensions,
        frames(100),
    )
    .unwrap();
    request.plan = Some(GenerationPlan::Extension(
        ExtensionGenerationPlan::new(
            ExtensionDirection::FromLeft,
            frames(12),
            rate(),
            &capability,
            NativeDimensions::new(512, 320).unwrap(),
        )
        .unwrap(),
    ));
    let error = RequestOrigin::from_request(&request)
        .unwrap_err()
        .to_string();
    assert!(error.contains("extension output admission is unavailable"));
}
