use std::{collections::BTreeMap, error::Error};

use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn node() -> NodeId {
    NodeId::new("hold").unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn request(document: &ProjectDocument, revision: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}

fn fixture() -> Result<(
    ProjectDocument,
    GeneratedArtifact,
    BTreeMap<AssetId, AssetRecord>,
)> {
    let rate = FrameRate::new(30_000, 1001)?;
    let document = ProjectDocument::new(
        ProjectId::new("generated")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 512,
            height: 320,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let inserted = apply(
        &document,
        &request(
            &document,
            "hold-inserted",
            Command::Insert {
                parent: document.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: node(),
                    nodes: BTreeMap::from([(
                        node(),
                        BeatNode::hold(
                            "Pause",
                            HoldRecipe {
                                duration: duration(30),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                },
            },
        ),
    )?
    .forward
    .apply(&document)?;
    let object = |digit: char| {
        GeneratedObjectRef::new(
            GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
            1024,
        )
        .unwrap()
    };
    let artifact = GeneratedArtifact {
        sampled_asset: AssetId::new("sampled")?,
        sampled_object: object('a'),
        native_asset: AssetId::new("native")?,
        native_object: object('b'),
        provenance: object('c'),
        sampling: BridgeSamplingMap::new(
            rate,
            FrameRate::new(24, 1)?,
            duration(25),
            duration(30),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )?,
    };
    let clock = SourceTimeBase::new(1, 1000)?;
    let record = |object: &GeneratedObjectRef, frames| AssetRecord {
        source_qualification: None,
        label: "Fixture master".into(),
        content_hash: object.content().to_string(),
        video: Some(
            SourceSpan::new(
                SourceTimestamp {
                    ticks: 0,
                    time_base: clock,
                },
                SourceTimestamp {
                    ticks: 1043,
                    time_base: clock,
                },
            )
            .unwrap(),
        ),
        audio: None,
        still_image: false,
        frame_count: Some(duration(frames)),
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
    Ok((inserted, artifact, assets))
}

#[test]
fn generic_commands_cannot_bypass_dedicated_candidate_admission() -> Result {
    let scratch = tempfile::tempdir()?;
    let (initial, artifact, assets) = fixture()?;
    let acceptance = request(
        &initial,
        "accepted",
        Command::AcceptGeneratedHold {
            node: node(),
            artifact: artifact.clone(),
            assets: assets.clone(),
        },
    );
    let authored = apply(&initial, &acceptance)?.forward.apply(&initial)?;
    let import = scratch.path().join("import.deadpan");
    assert_eq!(
        ProjectStore::create(&import, &authored)
            .err()
            .unwrap()
            .code(),
        "GeneratedAcceptanceUnavailable"
    );
    assert!(
        !import.exists(),
        "rejected ingress must not create a partial package"
    );
    let path = scratch.path().join("commands.deadpan");
    let mut store = ProjectStore::create(&path, &initial)?;
    for operation in [
        acceptance.command,
        Command::EditOccurrence {
            instance: InstancePath {
                node: node(),
                repeats: Vec::new(),
            },
            edit: OccurrenceEdit::AcceptGeneratedHold {
                artifact: artifact.clone(),
                assets: assets.clone(),
            },
            identities: OccurrenceIdentities::default(),
        },
    ] {
        let operation = request(&initial, "rejected", operation);
        apply(&initial, &operation)?;
        assert_eq!(
            store.preview(&operation).unwrap_err().code(),
            "GeneratedAcceptanceUnavailable"
        );
        assert_eq!(
            store.commit(&operation).unwrap_err().code(),
            "GeneratedAcceptanceUnavailable"
        );
        assert_eq!(store.snapshot()?, initial);
    }
    for (id, asset) in assets {
        let before = store.snapshot()?;
        store.commit(&request(
            &before,
            &format!("asset-{id}"),
            Command::AddAsset { id, asset },
        ))?;
    }
    let before = store.snapshot()?;
    let video = HoldVideo::Generated {
        accepted: Box::new(AcceptedGeneration {
            artifact,
            fallback: HoldFallback::Background,
        }),
    };
    let recipe = HoldRecipe {
        duration: duration(30),
        video: video.clone(),
        audio: HoldAudio::Silence,
    };
    let fresh = NodeId::new("inserted")?;
    let provider = request(
        &before,
        "direct-provider",
        Command::SetHoldProvider {
            node: node(),
            video,
        },
    );
    assert_eq!(
        store.preview(&provider).unwrap_err().code(),
        "InvalidCommand"
    );
    assert_eq!(
        store.commit(&provider).unwrap_err().code(),
        "InvalidCommand"
    );
    assert_eq!(store.snapshot()?, before);
    for command in [
        Command::Insert {
            parent: before.root().clone(),
            index: 1,
            subtree: Subtree {
                root: fresh.clone(),
                nodes: BTreeMap::from([(fresh, BeatNode::hold("Inserted", recipe.clone()))]),
                overrides: BTreeMap::new(),
            },
        },
        Command::WrapRepeat {
            node: node(),
            id: NodeId::new("repeat")?,
            plays: 2,
            gap: Some(recipe),
            anchor_policy: WrapAnchorPolicy::default(),
        },
    ] {
        let operation = request(&before, "rejected-provider", command);
        apply(&before, &operation)?;
        assert_eq!(
            store.preview(&operation).unwrap_err().code(),
            "GeneratedAcceptanceUnavailable"
        );
        assert_eq!(
            store.commit(&operation).unwrap_err().code(),
            "GeneratedAcceptanceUnavailable"
        );
        assert_eq!(store.snapshot()?, before);
    }
    store.validate()?;
    Ok(())
}

#[test]
fn existing_authored_artifact_resizes_reverts_and_navigates_durable_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("retained.deadpan");
    let (initial, artifact, assets) = fixture()?;
    let accepted = apply(
        &initial,
        &request(
            &initial,
            "accepted",
            Command::AcceptGeneratedHold {
                node: node(),
                artifact,
                assets,
            },
        ),
    )?
    .forward
    .apply(&initial)?;
    drop(ProjectStore::create(&path, &initial)?);
    // A trusted preexisting snapshot fixture, not a media acceptance shortcut.
    // No test file is claimed to be validated or playable generated media.
    let mut json = serde_json::to_value(&accepted)?;
    json["revision_id"] = serde_json::to_value(initial.revision_id())?;
    let retained = ProjectDocument::from_json(&json.to_string())?;
    let database = rusqlite::Connection::open(path.join("project.sqlite"))?;
    database.execute(
        "UPDATE revisions SET document=?1 WHERE kind='initial'",
        [retained.to_json()?],
    )?;
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.commit(&request(
        &retained,
        "shorter",
        Command::SetHoldDuration {
            node: node(),
            duration: duration(12),
        },
    ))?;
    let shorter = store.snapshot()?;
    store.undo(shorter.revision_id(), RevisionId::new("undo-shorter")?)?;
    assert_eq!(store.snapshot()?.nodes(), retained.nodes());
    let current = store.snapshot()?;
    store.redo(current.revision_id(), RevisionId::new("redo-shorter")?)?;
    assert_eq!(store.snapshot()?.nodes(), shorter.nodes());
    let current = store.snapshot()?;
    store.commit(&request(
        &current,
        "revert",
        Command::RevertGeneratedHold { node: node() },
    ))?;
    assert!(matches!(&store.snapshot()?.nodes()[&node()].kind,
        NodeKind::Hold { recipe } if recipe.video == HoldVideo::Background && recipe.duration == duration(12)));
    let current = store.snapshot()?;
    store.undo(current.revision_id(), RevisionId::new("undo-revert")?)?;
    assert_eq!(store.snapshot()?.nodes(), shorter.nodes());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), shorter.nodes());
    assert_eq!(reopened.snapshot()?.assets(), retained.assets());
    reopened.validate()?;
    Ok(())
}
