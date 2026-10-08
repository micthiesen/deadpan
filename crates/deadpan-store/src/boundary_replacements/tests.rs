use super::*;
use deadpan_core::*;
use deadpan_jobs::{
    AttemptId, AxisLimits, BridgeCapability, BridgeGenerationPlan, ConditioningMode,
    DimensionLimits, FrameCountFormula, GenerationOptions, HoldConstraints, MessageIdentity,
    MotionAmount, NativeDimensions, ProviderPackId, ProviderPackVersion, ProviderSelection,
    RequestId, RequestVersion, RuntimeId, RuntimeVersion, Sha256 as ContentSha256, VideoSpec,
};
use rusqlite::params;
use serde_json::json;
use sha2::{Digest, Sha256};

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
fn target(value: &str) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: id(value),
        repeats: vec![],
    }
}
fn black() -> Option<GenerationPictureIdentity> {
    Some(GenerationPictureIdentity::AuthoredBlack)
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 12,
            time_base,
        },
    )
    .unwrap()
}
fn hold() -> HoldRecipe {
    HoldRecipe {
        duration: frames(12),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
}
fn artifact(name: &str, ordinal: usize) -> GeneratedArtifact {
    let object = |offset| {
        GeneratedObjectRef::new(
            GeneratedContentId::new(format!("{:064x}", ordinal * 3 + offset)).unwrap(),
            100,
        )
        .unwrap()
    };
    GeneratedArtifact {
        sampled_asset: AssetId::new(format!("sampled-{name}")).unwrap(),
        sampled_object: object(1),
        native_asset: AssetId::new(format!("native-{name}")).unwrap(),
        native_object: object(2),
        provenance: object(3),
        sampling: BridgeSamplingMap::new(
            rate(),
            rate(),
            frames(12),
            frames(12),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap(),
        content_aspect: Some([640, 480]),
    }
}
fn fixture(names: &[&str], generated: &[&str]) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("boundary-host").unwrap(),
        revision("base"),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes = BTreeMap::from([(
        id("root"),
        BeatNode::sequence("Root", names.iter().map(|name| id(name)).collect()),
    )]);
    let mut assets = BTreeMap::new();
    for (ordinal, &name) in names.iter().enumerate() {
        let mut recipe = hold();
        if generated.contains(&name) {
            let artifact = artifact(name, ordinal);
            for (asset, object) in [
                (&artifact.sampled_asset, &artifact.sampled_object),
                (&artifact.native_asset, &artifact.native_object),
            ] {
                assets.insert(
                    asset.clone(),
                    AssetRecord {
                        label: name.into(),
                        content_hash: object.content().to_string(),
                        video: Some(span()),
                        audio: None,
                        still_image: false,
                        frame_count: Some(frames(12)),
                        source_qualification: None,
                    },
                );
            }
            recipe.video = HoldVideo::Generated {
                accepted: Box::new(AcceptedGeneration {
                    artifact,
                    fallback: HoldFallback::Background,
                }),
            };
        }
        nodes.insert(id(name), BeatNode::hold(name, recipe));
    }
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(assets).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn accepted<'a>(document: &'a ProjectDocument, name: &str) -> &'a AcceptedGeneration {
    let NodeKind::Hold { recipe } = &document.nodes()[&id(name)].kind else {
        panic!()
    };
    let HoldVideo::Generated { accepted } = &recipe.video else {
        panic!()
    };
    accepted
}
fn binding(
    left: Option<GenerationPictureIdentity>,
    right: Option<GenerationPictureIdentity>,
) -> GenerationInputBinding {
    GenerationInputBinding {
        duration: frames(12),
        frame_rate: rate(),
        canvas: [640, 480],
        left,
        right,
    }
}
fn generated(
    document: &ProjectDocument,
    name: &str,
    ordinal: u64,
) -> Option<GenerationPictureIdentity> {
    Some(GenerationPictureIdentity::Generated {
        sampled_object: accepted(document, name).artifact.sampled_object.clone(),
        frame: SourceFrameId(ordinal),
        content_aspect: Some([4, 3]),
    })
}
fn connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE revisions(id TEXT PRIMARY KEY);
        INSERT INTO revisions VALUES('origin'),('accepted');
        CREATE TABLE generation_bundle_receipts(request_id TEXT NOT NULL,attempt_id TEXT NOT NULL,
            PRIMARY KEY(request_id,attempt_id));",
        )
        .unwrap();
    crate::generation_origins::create_tables(&connection).unwrap();
    connection
}
fn hash(domain: &[u8], bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// Canonical retained metadata exercises the production bounded receipt reader.
// Admission-proof validation and media decoding are separately tested by the
// origin module; these fixtures do not claim a worker or decoded-media receipt.
fn save_origin(
    connection: &Connection,
    document: &ProjectDocument,
    name: &str,
    input: GenerationInputBinding,
) -> AcceptedOriginReceipt {
    let artifact = &accepted(document, name).artifact;
    let request = RequestId::new(format!("request-{name}")).unwrap();
    let identity = MessageIdentity::new(
        request.clone(),
        AttemptId::new(format!("attempt-{name}")).unwrap(),
    );
    let constraints = HoldConstraints {
        video: VideoSpec::new(frames(12), rate(), 640, 480).unwrap(),
        conditioning: ConditioningMode::Bridge,
        motion: MotionAmount::Still,
        instructions: None,
        region_target: None,
    };
    let bridge = BridgeGenerationPlan::new(
        frames(12),
        rate(),
        &BridgeCapability::new(
            true,
            rate(),
            FrameCountFormula::new(1, 0, 2, 97).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(640, 640, 1).unwrap(),
                AxisLimits::new(480, 480, 1).unwrap(),
            ),
        ),
        NativeDimensions::new(640, 480).unwrap(),
    )
    .unwrap();
    let provider = ProviderSelection {
        pack_id: ProviderPackId::new("fixture").unwrap(),
        pack_version: ProviderPackVersion::new("1").unwrap(),
        runtime_id: RuntimeId::new("fixture").unwrap(),
        runtime_version: RuntimeVersion::new("1").unwrap(),
        seed: 1,
    };
    let receipt: AcceptedOriginReceipt = serde_json::from_value(json!({
        "version": 1, "artifact": artifact, "identity": identity,
        "origin": {"request_id": request, "project_id": document.project_id(),
            "scope_id": crate::generation::GenerationScopeId::from_first_request(request.clone()),
            "origin_revision": revision("origin"), "origin_target": target(name),
            "request_version": RequestVersion::new(1).unwrap(), "context_sha256": ContentSha256::new("a".repeat(64)).unwrap(),
            "constraints": constraints, "provider": provider, "bridge_plan": bridge},
        "options": GenerationOptions::from_constraints(&constraints), "input_binding": input,
        "accepted_revision": revision("accepted"),
    })).unwrap();
    let json = serde_json::to_string(&receipt).unwrap();
    let key = hash(
        b"deadpan-accepted-artifact-1",
        &serde_json::to_vec(artifact).unwrap(),
    );
    connection
        .execute(
            "INSERT INTO generation_bundle_receipts VALUES(?1,?2)",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO generation_accepted_origins VALUES(?1,?2,?3,'origin','accepted',?4,?5)",
            params![
                key,
                identity.request_id.as_str(),
                identity.attempt_id.as_str(),
                json,
                hash(b"deadpan-accepted-origin-1", json.as_bytes())
            ],
        )
        .unwrap();
    receipt
}
fn derive(
    connection: &Connection,
    document: ProjectDocument,
    exclusions: BTreeSet<ScopedNodeTarget>,
    intents: BTreeSet<NodeId>,
) -> DerivedReplacements {
    derive_with_bindings(
        connection,
        &ValidatedDocument::new(Arc::new(document)).unwrap(),
        &exclusions,
        &intents,
        None,
    )
    .unwrap()
}
fn apply(document: &ProjectDocument, command: Command, next: &str) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    };
    deadpan_core::apply(document, &request)
        .unwrap()
        .forward
        .apply(document)
        .unwrap()
}

#[test]
fn final_fallback_can_preserve_neighbor_that_an_independent_first_pass_would_replace() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    save_origin(&connection, &document, "a", binding(None, black()));
    save_origin(&connection, &document, "b", binding(black(), black()));
    let changes = connection.total_changes();
    let result = derive(
        &connection,
        document.clone(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    assert_eq!(
        result
            .entries
            .iter()
            .map(|entry| &entry.target)
            .collect::<Vec<_>>(),
        vec![&target("a")]
    );
    assert_eq!(
        result.entries[0].accepted.as_ref(),
        accepted(&document, "a")
    );
    assert_eq!(result.bindings[&target("b")].left, black());
    assert_eq!(
        result.bindings[&target("a")].right,
        generated(&document, "b", 0)
    );
    assert_eq!(
        result.births[0].input_binding,
        result.bindings[&target("a")]
    );
    assert_eq!(result.births[0].target, target("a"));
    assert_eq!(
        result.births[0].origin.artifact(),
        &accepted(&document, "a").artifact
    );
    assert_eq!(result.births[0].cause, IntentCause::SourceBoundaryChanged);
    assert_eq!(connection.total_changes(), changes);
}

#[test]
fn final_provider_changes_propagate_and_births_bind_the_complete_final_assignment() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    save_origin(
        &connection,
        &document,
        "a",
        binding(None, generated(&document, "b", 0)),
    );
    save_origin(
        &connection,
        &document,
        "b",
        binding(generated(&document, "a", 11), black()),
    );
    let result = derive(&connection, document, BTreeSet::new(), BTreeSet::new());
    assert_eq!(result.entries.len(), 2);
    assert!(
        result.births.iter().all(
            |birth| birth.input_binding.left == black() && birth.input_binding.right == black()
        )
    );
    assert!(
        result
            .births
            .iter()
            .all(|birth| birth.cause == IntentCause::SourceBoundaryChanged)
    );
}

#[test]
fn conservative_cycle_is_named_once_by_the_same_canonical_group_identity() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    for name in ["a", "b"] {
        save_origin(&connection, &document, name, binding(black(), black()));
    }
    let result = derive(&connection, document, BTreeSet::new(), BTreeSet::new());
    assert_eq!(result.entries.len(), 2);
    let cause = IntentCause::cyclic_group(&[target("a"), target("b")]).unwrap();
    assert!(result.births.iter().all(|birth| birth.cause == cause));
}

#[test]
fn exact_explicit_exclusion_is_a_constant_and_needs_no_origin_receipt() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    save_origin(
        &connection,
        &document,
        "a",
        binding(black(), generated(&document, "b", 0)),
    );
    let result = derive(
        &connection,
        document,
        BTreeSet::from([target("b")]),
        BTreeSet::new(),
    );
    assert!(result.entries.is_empty());
    assert_eq!(result.bindings.len(), 1);
    assert!(result.bindings.contains_key(&target("a")));
}

#[test]
fn missing_or_corrupt_origin_refuses_without_any_store_write() {
    let document = fixture(&["lead", "a", "tail"], &["a"]);
    let connection = connection();
    let validated = ValidatedDocument::new(Arc::new(document.clone())).unwrap();
    let before = connection.total_changes();
    let error = derive_with_bindings(
        &connection,
        &validated,
        &BTreeSet::new(),
        &BTreeSet::new(),
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("no immutable accepted-origin"));
    assert_eq!(connection.total_changes(), before);
    save_origin(&connection, &document, "a", binding(black(), black()));
    connection
        .execute(
            "UPDATE generation_accepted_origins SET receipt_sha256=?1",
            ["0".repeat(64)],
        )
        .unwrap();
    let before = connection.total_changes();
    assert!(
        derive_with_bindings(
            &connection,
            &validated,
            &BTreeSet::new(),
            &BTreeSet::new(),
            None
        )
        .is_err()
    );
    assert_eq!(connection.total_changes(), before);
}

#[test]
fn shortening_zoom_caption_and_whole_group_moves_preserve_raw_conditioning() {
    let document = fixture(&["lead", "a", "tail", "outside"], &["a"]);
    let connection = connection();
    save_origin(&connection, &document, "a", binding(black(), black()));
    let mut changed = apply(
        &document,
        Command::Group {
            parent: id("root"),
            start: 0,
            end: 3,
            id: id("group"),
            label: "Group".into(),
        },
        "grouped",
    );
    changed = apply(
        &changed,
        Command::Move {
            node: id("group"),
            parent: id("root"),
            index: 1,
        },
        "moved",
    );
    changed = apply(
        &changed,
        Command::SetHoldDuration {
            node: id("a"),
            duration: frames(3),
        },
        "shortened",
    );
    changed = apply(
        &changed,
        Command::SetFraming {
            node: id("group"),
            framing: Some(
                Framing::static_pose(
                    FramingPose::new(
                        ExactRatio::new(1, 2).unwrap(),
                        ExactRatio::new(1, 2).unwrap(),
                        ExactRatio::integer(2),
                    )
                    .unwrap(),
                )
                .unwrap(),
            ),
        },
        "zoomed",
    );
    changed = apply(
        &changed,
        Command::SetCaptions {
            node: id("a"),
            captions: vec![Caption {
                range: range(0, 3),
                text: "A caption".into(),
                placement: CaptionPlacement::Bottom,
                reveal: None,
            }],
        },
        "captioned",
    );
    let result = derive(&connection, changed, BTreeSet::new(), BTreeSet::new());
    assert!(result.entries.is_empty());
    assert_eq!(result.bindings[&target("a")].duration, frames(3));
    assert_eq!(result.bindings[&target("a")].left, black());
    assert_eq!(result.bindings[&target("a")].right, black());
}

#[test]
fn nested_retime_uses_the_sampled_master_ordinal_and_saved_content_aspect() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    // The two-frame wrapper's last center samples A at local frame nine.
    let changed = apply(
        &document,
        Command::WrapRetime {
            node: id("a"),
            id: id("speed"),
            duration: frames(2),
            pitch: PitchPolicy::Preserve,
        },
        "retimed",
    );
    save_origin(
        &connection,
        &changed,
        "b",
        binding(generated(&changed, "a", 9), black()),
    );
    let result = derive(
        &connection,
        changed,
        BTreeSet::from([target("a")]),
        BTreeSet::new(),
    );
    assert!(result.entries.is_empty());
    assert_eq!(
        result.bindings[&target("b")].left,
        generated(&document, "a", 9)
    );
}

#[test]
fn removed_cutaway_over_generated_provider_does_not_create_a_false_dependency() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    let changed = apply(
        &document,
        Command::SetCutaways {
            node: id("a"),
            cutaways: vec![Cutaway {
                range: range(0, 12),
                asset: accepted(&document, "a").artifact.sampled_asset.clone(),
                selection: ExactSourceSpan::from(span()),
                fit: CutawayFit::Hold,
                removed: true,
            }],
        },
        "removed",
    );
    save_origin(&connection, &changed, "a", binding(None, black()));
    save_origin(&connection, &changed, "b", binding(black(), black()));
    let result = derive(&connection, changed, BTreeSet::new(), BTreeSet::new());
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].target, target("a"));
    assert_eq!(result.bindings[&target("b")].left, black());
}

#[test]
fn only_requested_live_intent_nodes_are_measured_and_their_final_inputs_are_returned() {
    let document = fixture(&["lead", "a", "pending", "tail", "unrelated"], &["a"]);
    let connection = connection();
    save_origin(&connection, &document, "a", binding(None, black()));
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"]["unrelated"]["kind"] = serde_json::to_value(NodeKind::Hold {
        recipe: HoldRecipe {
            video: HoldVideo::Freeze {
                asset: accepted(&document, "a").artifact.sampled_asset.clone(),
                timestamp: span().start(),
            },
            ..hold()
        },
    })
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let result = derive(
        &connection,
        document,
        BTreeSet::new(),
        BTreeSet::from([id("pending"), id("deleted")]),
    );
    assert_eq!(result.bindings.len(), 2);
    assert_eq!(result.bindings[&target("pending")].left, black());
    assert!(!result.bindings.contains_key(&target("unrelated")));
}

#[test]
fn repeated_defaults_and_owned_branches_get_one_canonical_binding_without_occurrence_expansion() {
    let document = fixture(&["a"], &["a"]);
    let connection = connection();
    save_origin(&connection, &document, "a", binding(None, None));
    let repeated = apply(
        &document,
        Command::WrapRepeat {
            node: id("a"),
            id: id("repeat"),
            plays: 100_000,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
        "wrapped",
    );
    let shared = ScopedNodeTarget {
        node: id("a"),
        repeats: vec![RepeatEditStep {
            repeat: id("repeat"),
            branch: RepeatEditBranch::Play {
                iteration: IterationId {
                    allocation: revision("wrapped"),
                    ordinal: 1,
                },
            },
        }],
    };
    let owned = apply(
        &repeated,
        Command::EditScoped {
            target: shared.clone(),
            edit: ScopedNodeEdit::Rename {
                label: "Owned".into(),
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("owned")],
                marks: vec![],
            },
        },
        "isolated",
    );
    let result = derive(
        &connection,
        owned,
        BTreeSet::from([shared]),
        BTreeSet::new(),
    );
    assert!(result.entries.is_empty());
    assert_eq!(result.bindings.len(), 2);
    assert!(result.bindings.contains_key(&ScopedNodeTarget {
        node: id("a"),
        repeats: vec![RepeatEditStep {
            repeat: id("repeat"),
            branch: RepeatEditBranch::Default,
        }]
    }));
    assert!(result.bindings.contains_key(&ScopedNodeTarget {
        node: id("owned"),
        repeats: vec![RepeatEditStep {
            repeat: id("repeat"),
            branch: RepeatEditBranch::Play {
                iteration: IterationId {
                    allocation: revision("wrapped"),
                    ordinal: 1
                }
            },
        }]
    }));
}

#[test]
fn exact_artifact_receipts_are_cached_and_byte_admission_precedes_database_reads() {
    let document = fixture(&["a"], &["a"]);
    let connection = connection();
    save_origin(&connection, &document, "a", binding(None, None));
    let artifact = &accepted(&document, "a").artifact;
    let mut cache = OriginCache::new(MAX_METADATA_BYTES);
    let first = cache.read(&connection, artifact).unwrap();
    connection
        .execute("DELETE FROM generation_accepted_origins", [])
        .unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &cache.read(&connection, artifact).unwrap()
    ));
    let empty = Connection::open_in_memory().unwrap();
    let error = OriginCache::new(MAX_ORIGIN_ROW_BYTES - 1)
        .read(&empty, artifact)
        .unwrap_err();
    assert!(error.to_string().contains("cache cannot admit"));
    let value = binding(black(), black());
    let size = serde_json::to_vec(&value).unwrap().len();
    assert!(MetadataBudget::new(size - 1).charge(&value).is_err());
    MetadataBudget::new(size).charge(&value).unwrap();
}

#[test]
fn fresh_acceptance_can_itself_revert_when_a_neighbor_loses_its_accepted_provider() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    let ephemeral = save_origin(
        &connection,
        &document,
        "a",
        binding(black(), generated(&document, "b", 0)),
    );
    connection
        .execute("DELETE FROM generation_accepted_origins", [])
        .unwrap();
    save_origin(&connection, &document, "b", binding(black(), black()));
    let before = connection.total_changes();
    let result = derive_with_bindings(
        &connection,
        &ValidatedDocument::new(Arc::new(document)).unwrap(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        Some(&ephemeral),
    )
    .unwrap();
    assert_eq!(
        result
            .entries
            .iter()
            .map(|entry| entry.target.clone())
            .collect::<Vec<_>>(),
        vec![target("a"), target("b")]
    );
    assert_eq!(*result.births[0].origin, ephemeral);
    assert!(
        result
            .births
            .iter()
            .all(|birth| matches!(birth.cause, IntentCause::CyclicBoundaryDependencies { .. }))
    );
    assert!(
        result.births.iter().all(
            |birth| birth.input_binding.left == black() && birth.input_binding.right == black()
        )
    );
    assert_eq!(connection.total_changes(), before);
}

#[test]
fn equal_aspect_ratios_match_but_a_changed_generated_crop_replaces_the_consumer() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    save_origin(
        &connection,
        &document,
        "b",
        binding(generated(&document, "a", 11), black()),
    );
    for (aspect, replacements) in [([1280, 960], 0), ([16, 9], 1)] {
        let mut wire = serde_json::to_value(&document).unwrap();
        let mut recipe = hold();
        let mut accepted = accepted(&document, "a").clone();
        accepted.artifact.content_aspect = Some(aspect);
        recipe.video = HoldVideo::Generated {
            accepted: Box::new(accepted),
        };
        wire["nodes"]["a"]["kind"] = serde_json::to_value(NodeKind::Hold { recipe }).unwrap();
        let changed = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let result = derive(
            &connection,
            changed,
            BTreeSet::from([target("a")]),
            BTreeSet::new(),
        );
        assert_eq!(result.entries.len(), replacements);
        if replacements == 1 {
            assert_eq!(result.entries[0].target, target("b"));
        }
    }
}

#[test]
fn derived_entries_apply_in_one_reversible_core_envelope_without_touching_valid_neighbor() {
    let document = fixture(&["lead", "a", "b", "tail"], &["a", "b"]);
    let connection = connection();
    save_origin(&connection, &document, "a", binding(None, black()));
    save_origin(&connection, &document, "b", binding(black(), black()));
    let base = Command::Rename {
        node: id("lead"),
        label: "New label".into(),
    };
    let after_base = apply(&document, base.clone(), "changed");
    let result = derive(&connection, after_base, BTreeSet::new(), BTreeSet::new());
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("changed"),
        command: Command::WithBoundaryReplacements {
            edit: BoundaryReplacementEdit::new(base, result.entries).unwrap(),
        },
    };
    let edit = deadpan_core::apply(&document, &request).unwrap();
    let after = edit.forward.apply(&document).unwrap();
    assert_eq!(after.nodes()[&id("lead")].label, "New label");
    assert!(
        matches!(&after.nodes()[&id("a")].kind, NodeKind::Hold { recipe } if recipe.video == HoldVideo::Background)
    );
    assert_eq!(accepted(&after, "b"), accepted(&document, "b"));
    assert_eq!(edit.inverse.apply(&after).unwrap(), document);
}
