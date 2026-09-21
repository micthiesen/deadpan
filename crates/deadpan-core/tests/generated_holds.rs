use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::{Value, json};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}

fn duration(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn rate() -> FrameRate {
    FrameRate::new(30_000, 1001).unwrap()
}

fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 1000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 10_000,
            time_base,
        },
    )
    .unwrap()
}

fn object(digit: char, bytes: u64) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
        bytes,
    )
    .unwrap()
}

fn video_asset(label: &str, object: &GeneratedObjectRef, frames: i64) -> AssetRecord {
    AssetRecord {
        source_qualification: None,
        label: label.into(),
        content_hash: object.content().to_string(),
        video: Some(span()),
        audio: None,
        still_image: false,
        frame_count: Some(duration(frames)),
    }
}

fn generated_fixture() -> (GeneratedArtifact, BTreeMap<AssetId, AssetRecord>) {
    let sampled_object = object('a', 100);
    let native_object = object('b', 200);
    let sampled_asset = asset("sampled");
    let native_asset = asset("native");
    let artifact = GeneratedArtifact {
        sampled_asset: sampled_asset.clone(),
        sampled_object: sampled_object.clone(),
        native_asset: native_asset.clone(),
        native_object: native_object.clone(),
        provenance: object('c', 300),
        sampling: BridgeSamplingMap::new(
            rate(),
            FrameRate::new(25, 1).unwrap(),
            duration(25),
            duration(30),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap(),
    };
    let assets = BTreeMap::from([
        (sampled_asset, video_asset("Sampled", &sampled_object, 30)),
        (native_asset, video_asset("Native", &native_object, 25)),
    ]);
    (artifact, assets)
}

fn empty() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap()
}

fn request(document: &ProjectDocument, command: Command, revision: &str) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}

fn edit(document: &ProjectDocument, command: Command, revision: &str) -> ProjectDocument {
    let transaction = apply(document, &request(document, command, revision)).unwrap();
    let next = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&next).unwrap(), *document);
    next
}

fn with_hold(video: HoldVideo) -> ProjectDocument {
    edit(
        &empty(),
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("hold"),
                nodes: BTreeMap::from([(
                    node("hold"),
                    BeatNode::hold(
                        "Hold",
                        HoldRecipe {
                            duration: duration(10),
                            video,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
            },
        },
        "hold",
    )
}

fn hold_recipe<'a>(document: &'a ProjectDocument, id: &str) -> &'a HoldRecipe {
    let NodeKind::Hold { recipe } = &document.nodes()[&node(id)].kind else {
        panic!("expected Hold")
    };
    recipe
}

#[test]
fn sampling_map_is_exact_strict_and_never_samples_an_endpoint() {
    assert!(GeneratedContentId::new("A".repeat(64)).is_err());
    assert!(GeneratedContentId::new("a".repeat(63)).is_err());
    assert!(GeneratedObjectRef::new(GeneratedContentId::new("a".repeat(64)).unwrap(), 0,).is_err());
    assert!(
        serde_json::from_value::<GeneratedContentId>(json!({
            "algorithm": "sha256",
            "digest": "a".repeat(64)
        }))
        .is_err()
    );
    let map = BridgeSamplingMap::new(
        rate(),
        FrameRate::new(25, 1).unwrap(),
        duration(25),
        duration(30),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap();
    assert_eq!(
        map.native_position(0).unwrap(),
        ExactRatio::new(24, 31).unwrap()
    );
    assert_eq!(
        map.native_position(29).unwrap(),
        ExactRatio::new(720, 31).unwrap()
    );
    assert!(map.native_position(-1).is_err());
    assert!(map.native_position(30).is_err());

    let one = BridgeSamplingMap::new(
        rate(),
        rate(),
        duration(2),
        duration(1),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap();
    assert_eq!(
        one.native_position(0).unwrap(),
        ExactRatio::new(1, 2).unwrap()
    );
    assert!(
        BridgeSamplingMap::new(
            rate(),
            rate(),
            duration(1),
            duration(1),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .is_err()
    );

    let mut wire = serde_json::to_value(&map).unwrap();
    assert_eq!(wire["schema_version"], 1);
    wire["schema_version"] = json!(2);
    assert!(serde_json::from_value::<BridgeSamplingMap>(wire).is_err());
    let mut wire = serde_json::to_value(&map).unwrap();
    wire["unknown"] = json!(true);
    assert!(serde_json::from_value::<BridgeSamplingMap>(wire).is_err());
}

#[test]
fn acceptance_is_atomic_reversible_and_resize_preserves_or_restores_fallback() {
    let document = with_hold(HoldVideo::Background);
    let before_audio = hold_recipe(&document, "hold").audio.clone();
    let (artifact, assets) = generated_fixture();
    let accepted = edit(
        &document,
        Command::AcceptGeneratedHold {
            node: node("hold"),
            artifact: artifact.clone(),
            assets,
        },
        "accepted",
    );
    assert_eq!(hold_recipe(&accepted, "hold").audio, before_audio);
    let HoldVideo::Generated { accepted: provider } = &hold_recipe(&accepted, "hold").video else {
        panic!("expected generated provider")
    };
    assert_eq!(provider.artifact, artifact);
    assert_eq!(provider.fallback, HoldFallback::Background);

    let shortened = edit(
        &accepted,
        Command::SetHoldDuration {
            node: node("hold"),
            duration: duration(6),
        },
        "shortened",
    );
    let HoldVideo::Generated { accepted: provider } = &hold_recipe(&shortened, "hold").video else {
        panic!("expected generated provider")
    };
    assert_eq!(provider.artifact, artifact);

    let lengthened = edit(
        &shortened,
        Command::SetHoldDuration {
            node: node("hold"),
            duration: duration(31),
        },
        "lengthened",
    );
    assert_eq!(
        hold_recipe(&lengthened, "hold").video,
        HoldVideo::Background
    );

    let reverted = edit(
        &accepted,
        Command::RevertGeneratedHold { node: node("hold") },
        "reverted",
    );
    assert_eq!(hold_recipe(&reverted, "hold").video, HoldVideo::Background);
}

#[test]
fn acceptance_rejects_missing_unrelated_or_incompatible_assets_and_legacy_provider() {
    let document = with_hold(HoldVideo::Background);
    let (artifact, assets) = generated_fixture();
    let mut missing = assets.clone();
    missing.remove(&artifact.native_asset);
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::AcceptGeneratedHold {
                    node: node("hold"),
                    artifact: artifact.clone(),
                    assets: missing,
                },
                "missing"
            )
        )
        .is_err()
    );

    let mut unrelated = assets.clone();
    unrelated.insert(asset("other"), unrelated[&artifact.sampled_asset].clone());
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::AcceptGeneratedHold {
                    node: node("hold"),
                    artifact: artifact.clone(),
                    assets: unrelated,
                },
                "unrelated"
            )
        )
        .is_err()
    );

    let mut wrong = assets.clone();
    wrong.get_mut(&artifact.native_asset).unwrap().frame_count = Some(duration(24));
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::AcceptGeneratedHold {
                    node: node("hold"),
                    artifact: artifact.clone(),
                    assets: wrong,
                },
                "wrong"
            )
        )
        .is_err()
    );

    let mut wrong_hash = assets.clone();
    wrong_hash
        .get_mut(&artifact.sampled_asset)
        .unwrap()
        .content_hash = "e".repeat(64);
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::AcceptGeneratedHold {
                    node: node("hold"),
                    artifact: artifact.clone(),
                    assets: wrong_hash,
                },
                "wrong-hash"
            )
        )
        .is_err()
    );

    let mut wrong_rate = artifact.clone();
    wrong_rate.sampling = BridgeSamplingMap::new(
        FrameRate::new(24, 1).unwrap(),
        FrameRate::new(25, 1).unwrap(),
        duration(25),
        duration(30),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap();
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::AcceptGeneratedHold {
                    node: node("hold"),
                    artifact: wrong_rate,
                    assets: assets.clone(),
                },
                "wrong-rate"
            )
        )
        .is_err()
    );

    let legacy_id = asset("legacy");
    let legacy = edit(
        &document,
        Command::AddAsset {
            id: legacy_id.clone(),
            asset: AssetRecord {
                source_qualification: None,
                label: "Legacy".into(),
                content_hash: "d".repeat(64),
                video: Some(span()),
                audio: None,
                still_image: false,
                frame_count: Some(duration(10)),
            },
        },
        "legacy-asset",
    );
    let legacy = edit(
        &legacy,
        Command::SetHoldProvider {
            node: node("hold"),
            video: HoldVideo::Accepted {
                asset: legacy_id,
                frames: FrameRange::new(ProjectFrame(0), ProjectFrame(10)).unwrap(),
            },
        },
        "legacy-provider",
    );
    assert_eq!(
        apply(
            &legacy,
            &request(
                &legacy,
                Command::AcceptGeneratedHold {
                    node: node("hold"),
                    artifact: artifact.clone(),
                    assets: assets.clone(),
                },
                "legacy-rejected",
            ),
        )
        .unwrap_err()
        .code,
        EditErrorCode::InvalidCommand
    );

    let accepted = edit(
        &document,
        Command::AcceptGeneratedHold {
            node: node("hold"),
            artifact: artifact.clone(),
            assets,
        },
        "accepted",
    );
    assert_eq!(
        apply(
            &accepted,
            &request(
                &accepted,
                Command::SetHoldProvider {
                    node: node("hold"),
                    video: hold_recipe(&accepted, "hold").video.clone(),
                },
                "bypass"
            )
        )
        .unwrap_err()
        .code,
        EditErrorCode::InvalidCommand
    );
}

#[test]
fn occurrence_acceptance_isolates_only_the_selected_play() {
    let document = with_hold(HoldVideo::Background);
    let repeated = edit(
        &document,
        Command::WrapRepeat {
            node: node("hold"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
        "repeat",
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&node("repeat")].kind else {
        panic!()
    };
    let selected = iterations.at(1).unwrap();
    let (artifact, assets) = generated_fixture();
    let isolated = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: node("hold"),
                repeats: vec![RepeatInstance {
                    node: node("repeat"),
                    iteration: selected.clone(),
                }],
            },
            edit: OccurrenceEdit::AcceptGeneratedHold { artifact, assets },
            identities: OccurrenceIdentities {
                nodes: vec![node("isolated-hold")],
                marks: vec![],
            },
        },
        "isolated",
    );
    assert_eq!(hold_recipe(&isolated, "hold").video, HoldVideo::Background);
    assert!(matches!(
        hold_recipe(&isolated, "isolated-hold").video,
        HoldVideo::Generated { .. }
    ));
    assert_eq!(
        isolated.overrides()[&node("repeat")].get(&selected),
        Some(&node("isolated-hold"))
    );
}

#[test]
fn every_legacy_adapter_rejects_generated_documents_and_commands() {
    let document = with_hold(HoldVideo::Background);
    let (artifact, assets) = generated_fixture();
    let generated = edit(
        &document,
        Command::AcceptGeneratedHold {
            node: node("hold"),
            artifact: artifact.clone(),
            assets: assets.clone(),
        },
        "generated",
    );
    let mut wire = serde_json::to_value(&generated).unwrap();
    wire.as_object_mut().unwrap().remove("basis_state");
    wire["schema_version"] = json!(4);
    assert!(legacy_v4::Document::from_json(&wire.to_string()).is_err());
    wire.as_object_mut().unwrap().remove("overrides");
    wire["schema_version"] = json!(3);
    assert!(legacy_v3::Document::from_json(&wire.to_string()).is_err());
    wire.as_object_mut().unwrap().remove("marks");
    wire["schema_version"] = json!(2);
    assert!(legacy_v2::Document::from_json(&wire.to_string()).is_err());
    wire["schema_version"] = json!(1);
    assert!(legacy_v1::Document::from_json(&wire.to_string()).is_err());

    let current_request = request(
        &document,
        Command::AcceptGeneratedHold {
            node: node("hold"),
            artifact,
            assets,
        },
        "new-command",
    );
    let json = serde_json::to_string(&current_request).unwrap();
    assert!(legacy_v1::upgrade_request(&json).is_err());
    assert!(legacy_v2::upgrade_request(&json).is_err());
    assert!(legacy_v3::upgrade_request(&json).is_err());
    assert!(legacy_v4::upgrade_request(&json).is_err());

    let Value::Object(mut request_wire) = serde_json::to_value(request(
        &document,
        Command::SetHoldProvider {
            node: node("hold"),
            video: HoldVideo::Background,
        },
        "new-provider",
    ))
    .unwrap() else {
        unreachable!()
    };
    request_wire.get_mut("command").unwrap()["video"] =
        serde_json::to_value(hold_recipe(&generated, "hold").video.clone()).unwrap();
    let json = Value::Object(request_wire).to_string();
    assert!(legacy_v1::upgrade_request(&json).is_err());
    assert!(legacy_v2::upgrade_request(&json).is_err());
    assert!(legacy_v3::upgrade_request(&json).is_err());
    assert!(legacy_v4::upgrade_request(&json).is_err());
}

#[test]
fn every_legacy_adapter_rejects_only_the_new_asset_hash_vocabulary() {
    let base = empty();
    let generated_asset = AssetRecord {
        source_qualification: None,
        label: "Unreferenced generated bytes".into(),
        content_hash: object('a', 1).content().to_string(),
        video: None,
        audio: None,
        still_image: true,
        frame_count: None,
    };
    let add = request(
        &base,
        Command::AddAsset {
            id: asset("generated-bytes"),
            asset: generated_asset,
        },
        "asset-only",
    );
    let transaction = apply(&base, &add).unwrap();
    let current = transaction.forward.apply(&base).unwrap();

    let mut base_wire = serde_json::to_value(&base).unwrap();
    base_wire.as_object_mut().unwrap().remove("basis_state");
    base_wire["schema_version"] = json!(4);
    let old4 = legacy_v4::Document::from_json(&base_wire.to_string()).unwrap();
    base_wire.as_object_mut().unwrap().remove("overrides");
    base_wire["schema_version"] = json!(3);
    let old3 = legacy_v3::Document::from_json(&base_wire.to_string()).unwrap();
    base_wire.as_object_mut().unwrap().remove("marks");
    base_wire["schema_version"] = json!(2);
    let old2 = legacy_v2::Document::from_json(&base_wire.to_string()).unwrap();
    base_wire["schema_version"] = json!(1);
    let old1 = legacy_v1::Document::from_json(&base_wire.to_string()).unwrap();
    assert!(!old1.matches(&current));
    assert!(!old2.matches(&current));
    assert!(!old3.matches(&current));
    assert!(!old4.matches(&current));

    let mut current_wire = serde_json::to_value(&current).unwrap();
    current_wire.as_object_mut().unwrap().remove("basis_state");
    current_wire["schema_version"] = json!(4);
    assert!(legacy_v4::Document::from_json(&current_wire.to_string()).is_err());
    current_wire.as_object_mut().unwrap().remove("overrides");
    current_wire["schema_version"] = json!(3);
    assert!(legacy_v3::Document::from_json(&current_wire.to_string()).is_err());
    current_wire.as_object_mut().unwrap().remove("marks");
    current_wire["schema_version"] = json!(2);
    assert!(legacy_v2::Document::from_json(&current_wire.to_string()).is_err());
    current_wire["schema_version"] = json!(1);
    assert!(legacy_v1::Document::from_json(&current_wire.to_string()).is_err());

    let request_json = serde_json::to_string(&add).unwrap();
    assert!(legacy_v1::upgrade_request(&request_json).is_err());
    assert!(legacy_v2::upgrade_request(&request_json).is_err());
    assert!(legacy_v3::upgrade_request(&request_json).is_err());
    assert!(legacy_v4::upgrade_request(&request_json).is_err());

    let edit4 = serde_json::to_value(&transaction).unwrap();
    assert!(legacy_v4::matches_edit(&edit4.to_string(), &transaction).is_err());
    let mut edit3 = edit4.clone();
    for direction in ["forward", "inverse"] {
        edit3[direction]
            .as_object_mut()
            .unwrap()
            .remove("overrides");
    }
    assert!(legacy_v3::matches_edit(&edit3.to_string(), &transaction).is_err());
    let mut edit2 = edit3;
    for direction in ["forward", "inverse"] {
        edit2[direction].as_object_mut().unwrap().remove("marks");
    }
    assert!(legacy_v2::matches_edit(&edit2.to_string(), &transaction).is_err());
    assert!(legacy_v1::matches_edit(&edit2.to_string(), &transaction).is_err());
}
