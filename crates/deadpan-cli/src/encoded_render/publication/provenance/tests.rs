use std::time::Duration;

use deadpan_core::{
    AcceptedGeneration, BeatNode, BridgeInterpolation, BridgeSamplingMap, ColorPolicy, Command,
    CommandRequest, FrameDuration, FrameRate, GeneratedContentId, GeneratedObjectRef, HoldAudio,
    HoldFallback, HoldRecipe, HoldVideo, IterationOrder, NodeId, NodeKind, PitchPolicy,
    PlayOverride, PresentationBasis, RetimePurpose, SourceSpan, SourceTimeBase, SourceTimestamp,
};

use super::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn background(frames: i64) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
}

fn generated(artifact: &GeneratedArtifact, frames: i64) -> HoldRecipe {
    HoldRecipe {
        video: HoldVideo::Generated {
            accepted: Box::new(AcceptedGeneration {
                artifact: artifact.clone(),
                fallback: HoldFallback::Background,
            }),
        },
        ..background(frames)
    }
}

fn node(kind: NodeKind) -> BeatNode {
    BeatNode {
        label: "Private fixture label https://private.invalid/secret".into(),
        kind,
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        audio_treatments: Default::default(),
        framing: None,
        cutaways: Vec::new(),
    }
}

fn artifact(
    prefix: &str,
    digits: [char; 3],
) -> (GeneratedArtifact, BTreeMap<AssetId, AssetRecord>) {
    let object = |digit: char| {
        GeneratedObjectRef::new(
            GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
            1024,
        )
        .unwrap()
    };
    let artifact = GeneratedArtifact {
        sampled_asset: AssetId::new(format!("{prefix}-sampled")).unwrap(),
        sampled_object: object(digits[0]),
        native_asset: AssetId::new(format!("{prefix}-native")).unwrap(),
        native_object: object(digits[1]),
        provenance: object(digits[2]),
        sampling: BridgeSamplingMap::new(
            FrameRate::new(30_000, 1001).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            duration(25),
            duration(30),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap(),
        content_aspect: None,
    };
    let record = |reference: &GeneratedObjectRef, frames: i64| AssetRecord {
        label: "Private generated label".into(),
        content_hash: reference.content().to_string(),
        video: Some(
            SourceSpan::new(
                SourceTimestamp {
                    ticks: 0,
                    time_base: SourceTimeBase::new(1, 30_000).unwrap(),
                },
                SourceTimestamp {
                    ticks: frames * 1001,
                    time_base: SourceTimeBase::new(1, 30_000).unwrap(),
                },
            )
            .unwrap(),
        ),
        audio: None,
        still_image: false,
        frame_count: Some(duration(frames)),
        source_qualification: None,
    };
    let assets = BTreeMap::from([
        (
            artifact.sampled_asset.clone(),
            record(&artifact.sampled_object, 30),
        ),
        (
            artifact.native_asset.clone(),
            record(&artifact.native_object, 25),
        ),
    ]);
    (artifact, assets)
}

fn document(
    roots: &[&str],
    nodes: Vec<(&str, BeatNode)>,
    assets: BTreeMap<AssetId, AssetRecord>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("provenance-project").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 4,
            height: 2,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", roots.iter().map(|name| id(name)).collect()),
    );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(assets).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn generated_intervals_follow_retime_sparse_plays_and_effective_gap_branches() {
    let (a, mut assets) = artifact("a", ['1', '2', '3']);
    let (b, b_assets) = artifact("b", ['4', '5', '6']);
    let (dormant, dormant_assets) = artifact("dormant", ['7', '8', '9']);
    assets.extend(b_assets);
    assets.extend(dormant_assets);
    let plays = IterationOrder::new(revision("plays"), 3).unwrap();
    // Build the entire override graph together so validation sees no detached
    // node before the sparse play/gap ownership is present.
    let base = document(
        &["repeat"],
        vec![
            ("child", BeatNode::hold("Black", background(30))),
            (
                "repeat",
                node(NodeKind::Repeat {
                    child: id("child"),
                    iterations: plays.clone(),
                    gap: Some(generated(&a, 30)),
                    escalation: None,
                }),
            ),
        ],
        assets,
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["play"] =
        serde_json::to_value(BeatNode::hold("Play", generated(&b, 30))).unwrap();
    wire["nodes"]["gap"] = serde_json::to_value(BeatNode::hold("Gap", generated(&b, 15))).unwrap();
    wire["nodes"]["dormant"] =
        serde_json::to_value(BeatNode::hold("Dormant", generated(&dormant, 20))).unwrap();
    wire["overrides"]["repeat"] = serde_json::to_value(vec![PlayOverride {
        iteration: plays.at(1).unwrap(),
        root: id("play"),
    }])
    .unwrap();
    wire["gap_overrides"]["repeat"] = serde_json::to_value(vec![
        PlayOverride {
            iteration: plays.at(0).unwrap(),
            root: id("gap"),
        },
        PlayOverride {
            iteration: plays.at(2).unwrap(),
            root: id("dormant"),
        },
    ])
    .unwrap();
    wire["nodes"]["retime"] = serde_json::to_value(node(NodeKind::Retime {
        child: id("repeat"),
        duration: duration(70),
        mapping: range(15, 120),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    }))
    .unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["retime"]);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&document).unwrap();
    let (artifacts, intervals) = generated_intervals(
        &plan,
        range(12, 65),
        10,
        10,
        &AtomicBool::new(false),
        deadline(),
    )
    .unwrap();
    assert_eq!(artifacts, vec![b, a]);
    assert_eq!(intervals.len(), 2);
    assert_eq!(intervals[0].artifact_index, 0);
    assert_eq!(intervals[0].project_range, range(12, 40));
    assert_eq!(intervals[0].output_range, [0, 28]);
    assert_eq!(intervals[1].project_range, range(40, 60));
    assert_eq!(intervals[1].output_range, [28, 48]);
    assert!(matches!(
        generated_intervals(
            &plan,
            range(12, 65),
            10,
            1,
            &AtomicBool::new(false),
            deadline(),
        ),
        Err(ProvenanceError::Capacity("generated intervals"))
    ));
    assert!(matches!(
        generated_intervals(
            &plan,
            range(12, 65),
            1,
            10,
            &AtomicBool::new(false),
            deadline(),
        ),
        Err(ProvenanceError::Capacity("generated artifact table"))
    ));
}

#[test]
fn full_artifact_identity_controls_coalescing_and_blank_seams_remain_separate() {
    let (a, assets) = artifact("shared", ['1', '2', '3']);
    let mut different = a.clone();
    different.provenance =
        GeneratedObjectRef::new(GeneratedContentId::new("f".repeat(64)).unwrap(), 1024).unwrap();
    let doc = document(
        &["first", "next", "blank", "last"],
        vec![
            ("first", BeatNode::hold("First", generated(&a, 3))),
            ("next", BeatNode::hold("Next", generated(&different, 2))),
            ("blank", BeatNode::hold("Black", background(1))),
            ("last", BeatNode::hold("Last", generated(&a, 2))),
        ],
        assets,
    );
    let (artifacts, intervals) = generated_intervals(
        &RenderPlan::compile(&doc).unwrap(),
        range(0, 8),
        10,
        10,
        &AtomicBool::new(false),
        deadline(),
    )
    .unwrap();
    assert_eq!(artifacts, vec![a, different]);
    assert_eq!(
        intervals
            .iter()
            .map(|entry| entry.project_range)
            .collect::<Vec<_>>(),
        vec![range(0, 3), range(3, 5), range(6, 8)]
    );
}

#[test]
fn huge_compact_repeat_rejects_full_scan_but_admits_a_small_selected_suffix() {
    let (a, assets) = artifact("a", ['1', '2', '3']);
    let doc = document(
        &["repeat"],
        vec![
            ("child", BeatNode::hold("Generated", generated(&a, 30))),
            (
                "repeat",
                node(NodeKind::Repeat {
                    child: id("child"),
                    iterations: IterationOrder::new(revision("many"), 40_000).unwrap(),
                    gap: None,
                    escalation: None,
                }),
            ),
        ],
        assets,
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    assert_eq!(plan.metadata().storage.iteration_run_entries, 1);
    assert!(matches!(
        generated_intervals(
            &plan,
            range(0, 1_200_000),
            10,
            10,
            &AtomicBool::new(false),
            deadline(),
        ),
        Err(ProvenanceError::Capacity("output-frame scan"))
    ));
    let (artifacts, intervals) = generated_intervals(
        &plan,
        range(1_199_965, 1_200_000),
        10,
        10,
        &AtomicBool::new(false),
        deadline(),
    )
    .unwrap();
    assert_eq!(artifacts, vec![a]);
    assert_eq!(
        intervals.len(),
        1,
        "adjacent plays with identical artifacts coalesce"
    );
    assert_eq!(intervals[0].output_range, [0, 35]);
}

#[test]
fn historical_capture_ignores_live_edits_and_omits_private_catalog_labels() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("project.deadpan");
    let document = document(
        &["black"],
        vec![("black", BeatNode::hold("Private beat", background(30)))],
        BTreeMap::from([(
            AssetId::new("unused-legacy").unwrap(),
            AssetRecord {
                label: "https://private.invalid/source?secret=token".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: None,
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        )]),
    );
    let mut writer = ProjectStore::create(&package, &document).unwrap();
    let cancelled = AtomicBool::new(false);
    let pictures = crate::picture::ProjectPictureSession::open_revision(
        &package,
        document.revision_id(),
        Some(range(3, 17)),
        &cancelled,
    )
    .unwrap();
    let contract = ExportPictureContract::capture(&pictures).unwrap();
    let hash = document_hash(&document, &cancelled, deadline()).unwrap();
    writer
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision("renamed"),
            command: Command::Rename {
                node: id("black"),
                label: "New live label".into(),
            },
        })
        .unwrap();
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let result = capture_revision(&reader, &contract, &hash, &cancelled, deadline()).unwrap();
    assert_eq!(result.revision_id, revision("initial"));
    assert_eq!(result.range, range(3, 17));
    assert!(result.generated_intervals.is_empty());
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(serialized.contains("committed_catalog_superset"));
    assert!(serialized.contains("unqualified"));
    for private in [
        "https:",
        "secret",
        "Private",
        "New live label",
        &"a".repeat(64),
    ] {
        assert!(!serialized.contains(private));
    }
    let other_hash = Sha256::new("f".repeat(64)).unwrap();
    assert!(matches!(
        capture_revision(&reader, &contract, &other_hash, &cancelled, deadline()),
        Err(ProvenanceError::Binding("complete document hash"))
    ));
    assert!(matches!(
        capture_revision(
            &reader,
            &contract,
            &hash,
            &AtomicBool::new(true),
            deadline()
        ),
        Err(ProvenanceError::Cancelled)
    ));
    assert!(matches!(
        capture_revision(&reader, &contract, &hash, &cancelled, Instant::now()),
        Err(ProvenanceError::Deadline)
    ));
}

#[test]
fn bounded_serialization_preserves_control_failures_and_stable_codes() {
    let cancelled = AtomicBool::new(false);
    let oversized = "x".repeat(MAX_REPORT_BYTES);
    assert!(matches!(
        check_serialized_size(&oversized, &cancelled, deadline()),
        Err(ProvenanceError::Capacity("serialized report"))
    ));
    let cancelled_error =
        check_serialized_size(&"small", &AtomicBool::new(true), deadline()).unwrap_err();
    assert!(matches!(cancelled_error, ProvenanceError::Cancelled));
    assert_eq!(cancelled_error.code(), "cancelled");
    let deadline_error = check_serialized_size(&"small", &cancelled, Instant::now()).unwrap_err();
    assert!(matches!(deadline_error, ProvenanceError::Deadline));
    assert_eq!(deadline_error.code(), "deadline_exceeded");
    assert_eq!(
        ProvenanceError::Binding("fixture").code(),
        "provenance_failed"
    );
}

#[test]
fn report_versions_preserve_legacy_bytes_and_exact_automatic_decision() {
    use deadpan_jobs::render::{RenderAutomaticPolicy, RenderAutomaticSelection};
    // Exercise report serialization only. Retained observations do not become
    // a VerifiedCandidate or permission to write a destination.
    let decision = RenderEncodingDecision::from_json(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../deadpan-jobs/src/render/admission/tests/measured-decision-v1.json"
    )))
    .unwrap();
    let selected = decision.selected().unwrap();
    let encoded_manifest: EncodedManifest =
        serde_json::from_value(serde_json::to_value(&selected.manifest).unwrap()).unwrap();
    let verification: VerificationReport =
        serde_json::from_value(serde_json::to_value(&selected.verification).unwrap()).unwrap();
    let mut report = PublicationProvenance {
        schema_version: 1,
        document: DocumentProvenance {
            project_id: decision.output.project_id.clone(),
            revision_id: decision.output.revision_id.clone(),
            document_sha256: decision.document_sha256.clone(),
            range: decision.output.range,
            catalog_scope: "committed_catalog_superset",
            catalog: Vec::new(),
            generated_dependency_scope: "emitted_picture_intervals",
            generated_artifacts: Vec::new(),
            generated_intervals: Vec::new(),
        },
        encoder_selection: "explicit_engineering_choice",
        render_intent: None,
        encoding_decision: None,
        encoded_manifest,
        verification,
    };
    #[derive(Serialize)]
    struct FrozenLegacy<'a> {
        schema_version: u32,
        #[serde(flatten)]
        document: &'a DocumentProvenance,
        encoder_selection: &'static str,
        encoded_manifest: &'a EncodedManifest,
        verification: &'a VerificationReport,
    }
    let old = FrozenLegacy {
        schema_version: 1,
        document: &report.document,
        encoder_selection: "explicit_engineering_choice",
        encoded_manifest: &report.encoded_manifest,
        verification: &report.verification,
    };
    assert_eq!(
        serde_json::to_vec(&report).unwrap(),
        serde_json::to_vec(&old).unwrap()
    );
    let legacy = serde_json::to_value(&report).unwrap();
    assert!(legacy.get("render_intent").is_none());
    assert!(legacy.get("encoding_decision").is_none());
    report.schema_version = 2;
    report.encoder_selection = "automatic_sdr_v1";
    report.render_intent = Some(RenderIntent {
        schema_version: 2,
        job_id: decision.job_id.clone(),
        project_id: decision.output.project_id.clone(),
        revision_id: decision.output.revision_id.clone(),
        document_sha256: decision.document_sha256.clone(),
        range: decision.output.range,
        policy: RenderAutomaticPolicy {
            schema_version: 1,
            selection: RenderAutomaticSelection::Automatic,
            algorithm: decision.algorithm,
        }
        .into(),
    });
    report.encoding_decision = Some(decision.clone());
    let automatic = serde_json::to_value(&report).unwrap();
    assert_eq!(automatic["schema_version"], 2);
    assert_eq!(automatic["encoder_selection"], "automatic_sdr_v1");
    assert_eq!(
        automatic["encoding_decision"],
        serde_json::to_value(&decision).unwrap()
    );
    assert_eq!(
        automatic["render_intent"],
        serde_json::to_value(report.render_intent.as_ref().unwrap()).unwrap()
    );
}
