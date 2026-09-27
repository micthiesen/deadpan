use super::*;
use deadpan_core::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("composite-insert-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v29-composite-insert-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema29_history_retains_insert_time_gap_clocks_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (16, 8));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(29))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (29, DATABASE_SCHEMA_VERSION)
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
        let legacy = legacy_v23::Document::from_json(old)?;
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
        assert_eq!(legacy_v23::upgrade_request(old_request)?, request);
        assert!(legacy_v23::matches_edit(old_edit, &edit)?);
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
    store.redo(baseline.revision_id(), RevisionId::new("v29-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes()[&gap].label, "Renamed detached gap");
    store.undo(redone.revision_id(), RevisionId::new("v29-undo-redo")?)?;
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

fn composite_initial() -> Result<ProjectDocument> {
    let root = NodeId::new("root")?;
    let hold = NodeId::new("repeated-hold")?;
    let repeat = NodeId::new("repeat")?;
    let initial = ProjectDocument::new(
        ProjectId::new("composite-project")?,
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
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (root, BeatNode::sequence("Root", vec![repeat.clone()])),
        (
            repeat,
            BeatNode {
                framing: None,
                label: "Repeat".into(),
                audio_edges: AudioEdgePolicies::default(),
                kind: NodeKind::Repeat {
                    child: hold.clone(),
                    iterations: IterationOrder::new(RevisionId::new("initial-plays")?, 2)?,
                    gap: None,
                },
            },
        ),
        (hold, BeatNode::hold("Hold", silent_hold(3)?)),
    ]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn silent_hold(frames: i64) -> Result<HoldRecipe> {
    Ok(HoldRecipe {
        duration: FrameDuration::new(frames)?,
        picture_context: None,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    })
}

fn composite_pause(initial: &ProjectDocument) -> Result<CommandRequest> {
    let revision = RevisionId::new("composite-insert")?;
    Ok(CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision.clone(),
        command: Command::InsertTime {
            at: ProjectFrame(0),
            hold: silent_hold(2)?,
            id: NodeId::new("pause")?,
            identities: SplitIdentities { nodes: vec![] },
            timing: AudioTimingId {
                allocation: revision,
                ordinal: 0,
            },
        },
    })
}

#[test]
fn every_legacy_insert_request_requires_its_pre_edit_physical_suffix() -> Result {
    let initial = composite_initial()?;
    let request = composite_pause(&initial)?;
    let wire = serde_json::to_string(&request)?;
    for upgrade in [
        legacy_v17::upgrade_request,
        legacy_v18::upgrade_request,
        legacy_v19::upgrade_request,
        legacy_v20::upgrade_request,
        legacy_v21::upgrade_request,
        legacy_v22::upgrade_request,
        legacy_v23::upgrade_request,
    ] {
        let old_request = upgrade(&wire)?;
        assert_eq!(old_request, request);
        assert!(legacy_v23::validate_request_context(&initial, &old_request).is_err());
    }
    Ok(())
}

#[test]
fn valid_modern_composite_history_cannot_claim_schema27_through29() -> Result {
    for version in 27..=29 {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("forged-composite.deadpan");
        let initial = composite_initial()?;
        let request = composite_pause(&initial)?;
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
        // retained positive fixture above is produced by the real old binary.
        let database = Connection::open(package.join("project.sqlite"))?;
        database.execute(
            "UPDATE revisions SET document=json_set(document,'$.schema_version',?1)",
            [version - 6],
        )?;
        database.pragma_update(None, "user_version", version)?;
        let edit = apply(&initial, &request)?;
        let edit_json = history_json(&database)?[0].1.clone();
        // Its old wire grammar, snapshot projections and exact patches all
        // match, so a failure must come from the frozen contextual admission.
        for (_, wire) in docs(&database)? {
            match version {
                27 => {
                    legacy_v21::Document::from_json(&wire)?;
                }
                28 => {
                    legacy_v22::Document::from_json(&wire)?;
                }
                29 => {
                    legacy_v23::Document::from_json(&wire)?;
                }
                _ => unreachable!(),
            }
        }
        assert!(match version {
            27 => legacy_v21::matches_edit(&edit_json, &edit)?,
            28 => legacy_v22::matches_edit(&edit_json, &edit)?,
            29 => legacy_v23::matches_edit(&edit_json, &edit)?,
            _ => unreachable!(),
        });
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, source } =
            ProjectStore::migrate(&package).unwrap_err()
        else {
            panic!("schema {version} accepted a composite suffix");
        };
        let expected = legacy_v23::validate_request_context(&initial, &request).unwrap_err();
        match source.as_ref() {
            StoreError::Edit(error) => {
                assert_eq!(error.code, expected.code);
                assert_eq!(error.message, expected.message);
            }
            _ => panic!("schema {version}: {source}"),
        }
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            version
        );
    }
    Ok(())
}

#[test]
fn schema23_snapshot_request_and_patch_fields_remain_closed() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let old = docs(&database)?.into_iter().next().unwrap().1;
    let mut wire: Value = serde_json::from_str(&old)?;
    wire["future"] = Value::Null;
    assert!(legacy_v23::Document::from_json(&wire.to_string()).is_err());
    let (request, edit) = history_json(&database)?.into_iter().last().unwrap();
    let mut wire: Value = serde_json::from_str(&request)?;
    wire["command"]["future"] = Value::Null;
    assert!(legacy_v23::upgrade_request(&wire.to_string()).is_err());
    let modern_edit: EditTransaction = serde_json::from_str(&edit)?;
    for direction in ["forward", "inverse"] {
        let mut wire: Value = serde_json::from_str(&edit)?;
        wire[direction]["future"] = Value::Null;
        assert!(legacy_v23::matches_edit(&wire.to_string(), &modern_edit).is_err());
    }
    Ok(())
}
