use super::*;
use deadpan_core::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("interior-insert-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v30-interior-insert-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema30_history_retains_composite_seams_gap_clocks_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (21, 10));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(30))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (30, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    for ((old_id, old), (new_id, new)) in old_documents.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        let legacy = legacy_v24::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v24::upgrade_request(old_request)?, request);
        assert!(legacy_v24::matches_edit(old_edit, &edit)?);
        // This semantic boundary adds no wire fields and must not recapture or
        // normalize any admitted old binding, command or patch.
        assert_eq!(old_request, &new_request);
        assert_eq!(old_edit, &new_edit);
        assert_eq!(
            serde_json::from_str::<Value>(old_request)?,
            serde_json::to_value(&request)?
        );
        assert_eq!(
            serde_json::from_str::<Value>(old_edit)?,
            serde_json::to_value(&edit)?
        );
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    let gap = NodeId::new("core23-detached-gap")?;
    let repeat = NodeId::new("old-gap-owner")?;
    assert_eq!(baseline.gap_overrides()[&repeat].len(), 3);
    assert_eq!(
        baseline.audio_bindings().bindings()[&gap]
            .lattice
            .reference
            .recipe,
        AudioRecipeKind::RepeatGap
    );
    assert!(baseline.nodes().contains_key(&NodeId::new("core23-pause")?));
    let pause = NodeId::new("core24-pause")?;
    let second_pause = NodeId::new("core24-second-pause")?;
    assert!(baseline.nodes().contains_key(&pause));
    assert!(!baseline.nodes().contains_key(&second_pause));
    store.redo(baseline.revision_id(), RevisionId::new("v30-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert!(redone.nodes().contains_key(&second_pause));
    assert_eq!(
        redone.duration()?.frames(),
        baseline.duration()?.frames() + 2
    );
    store.undo(redone.revision_id(), RevisionId::new("v30-undo-redo")?)?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    assert_eq!(
        store.snapshot()?.audio_bindings(),
        baseline.audio_bindings()
    );
    assert_eq!(store.snapshot()?.gap_overrides(), baseline.gap_overrides());
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}

fn silent_hold(frames: i64) -> Result<HoldRecipe> {
    Ok(HoldRecipe {
        duration: FrameDuration::new(frames)?,
        picture_context: None,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    })
}

fn interior_initial(source: bool) -> Result<ProjectDocument> {
    let root = NodeId::new("root")?;
    let first = NodeId::new("first")?;
    let child = NodeId::new("repeated-hold")?;
    let repeat = NodeId::new("repeat")?;
    let initial = ProjectDocument::new(
        ProjectId::new("interior-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )?;
    let mut wire = serde_json::to_value(initial)?;
    let first_node = if source {
        let asset = AssetId::new("original")?;
        let time_base = SourceTimeBase::new(1, 30)?;
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 4,
                time_base,
            },
        )?;
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            asset.clone(),
            AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: Some(span),
                audio: None,
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        )]))?;
        BeatNode {
            framing: None,
            label: "Original".into(),
            audio_edges: AudioEdgePolicies::default(),
            kind: NodeKind::Source {
                source: SourceNode {
                    duration: FrameDuration::new(4)?,
                    video: SourceVideo::Stream { asset, span },
                    audio: None,
                    link: LinkRelation::Independent,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio_mapping: SourceAudioMapping::FitBeat,
                    audio_offset: AudioSample(0),
                },
            },
        }
    } else {
        BeatNode::hold("First", silent_hold(4)?)
    };
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            root,
            BeatNode::sequence("Root", vec![first.clone(), repeat.clone()]),
        ),
        (first, first_node),
        (
            repeat,
            BeatNode {
                framing: None,
                label: "Repeat".into(),
                audio_edges: AudioEdgePolicies::default(),
                kind: NodeKind::Repeat {
                    child: child.clone(),
                    iterations: IterationOrder::new(RevisionId::new("initial-plays")?, 2)?,
                    gap: None,
                },
            },
        ),
        (child, BeatNode::hold("Repeated", silent_hold(3)?)),
    ]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn pause(initial: &ProjectDocument, at: i64) -> Result<CommandRequest> {
    let revision = RevisionId::new("interior-insert")?;
    Ok(CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision.clone(),
        command: Command::InsertTime {
            at: ProjectFrame(at),
            hold: silent_hold(2)?,
            id: NodeId::new("pause")?,
            identities: SplitIdentities {
                nodes: ["left", "right", "split-owner"]
                    .into_iter()
                    .map(NodeId::new)
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: revision,
                ordinal: 0,
            },
        },
    })
}

#[test]
fn schema24_admits_composite_seams_but_rejects_interior_physical_splits() -> Result {
    for source in [true, false] {
        let initial = interior_initial(source)?;
        for seam in [0, 4, 10] {
            legacy_v24::validate_request_context(&initial, &pause(&initial, seam)?)?;
        }
        for interior in [1, 2, 3, 5, 7] {
            assert!(
                legacy_v24::validate_request_context(&initial, &pause(&initial, interior)?)
                    .is_err()
            );
        }
    }
    Ok(())
}

#[test]
fn valid_modern_interior_history_cannot_claim_schema30() -> Result {
    for source in [true, false] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("forged-interior.deadpan");
        let initial = interior_initial(source)?;
        let request = pause(&initial, 2)?;
        let mut store = ProjectStore::create(&package, &initial)?;
        store.commit(&request)?;
        store.validate()?;
        let modern = store.snapshot()?;
        assert_eq!(
            modern.duration()?.frames(),
            initial.duration()?.frames() + 2
        );
        drop(store);
        // Only malicious test construction relabels modern history. The
        // positive fixture was authored by the retained real core24 binary.
        let database = Connection::open(package.join("project.sqlite"))?;
        database.execute(
            "UPDATE revisions SET document=json_set(document,'$.schema_version',24)",
            [],
        )?;
        database.pragma_update(None, "user_version", 30)?;
        let edit = apply(&initial, &request)?;
        let edit_json = history_json(&database)?[0].1.clone();
        for (_, wire) in docs(&database)? {
            legacy_v24::Document::from_json(&wire)?;
        }
        assert!(legacy_v24::matches_edit(&edit_json, &edit)?);
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, source } =
            ProjectStore::migrate(&package).unwrap_err()
        else {
            panic!("schema 30 accepted an interior split before a composite suffix");
        };
        let expected = legacy_v24::validate_request_context(&initial, &request).unwrap_err();
        match source.as_ref() {
            StoreError::Edit(error) => {
                assert_eq!(error.code, expected.code);
                assert_eq!(error.message, expected.message);
            }
            _ => panic!("schema 30: {source}"),
        }
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            30
        );
    }
    Ok(())
}

#[test]
fn schema24_snapshot_request_and_patch_fields_remain_closed_without_promotion() -> Result {
    for position in ["initial", "later", "command", "forward", "inverse"] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path())?;
        let database = Connection::open(package.join("project.sqlite"))?;
        if matches!(position, "initial" | "later") {
            let selector = if position == "initial" {
                "parent_id IS NULL"
            } else {
                "parent_id IS NOT NULL"
            };
            database.execute(&format!("UPDATE revisions SET document=json_set(document,'$.future',null) WHERE id=(SELECT id FROM revisions WHERE {selector} LIMIT 1)"), [])?;
        } else if position == "command" {
            database.execute("UPDATE history SET request=json_set(request,'$.command.future',null) WHERE id=(SELECT MIN(id) FROM history)", [])?;
        } else {
            database.execute("UPDATE history SET edit=json_set(edit,?1,null) WHERE id=(SELECT MIN(id) FROM history)", [format!("$.{position}.future")])?;
        }
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, .. } =
            ProjectStore::migrate(&package).unwrap_err()
        else {
            panic!("schema 30 accepted unknown {position} field");
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            30
        );
    }
    Ok(())
}
