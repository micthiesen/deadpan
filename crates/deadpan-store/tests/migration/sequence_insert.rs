use super::*;
use deadpan_core::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("sequence-insert-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v31-sequence-insert-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema31_history_retains_root_interiors_gap_clocks_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (26, 12));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(31))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (31, DATABASE_SCHEMA_VERSION)
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
        let legacy = legacy_v25::Document::from_json(old)?;
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
        assert_eq!(legacy_v25::upgrade_request(old_request)?, request);
        assert!(legacy_v25::matches_edit(old_edit, &edit)?);
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
    let pause = NodeId::new("core25-pause")?;
    let second_pause = NodeId::new("core25-second-pause")?;
    assert!(baseline.nodes().contains_key(&pause));
    assert!(!baseline.nodes().contains_key(&second_pause));
    store.redo(baseline.revision_id(), RevisionId::new("v31-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert!(redone.nodes().contains_key(&second_pause));
    assert_eq!(
        redone.duration()?.frames(),
        baseline.duration()?.frames() + 3
    );
    store.undo(redone.revision_id(), RevisionId::new("v31-undo-redo")?)?;
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

pub(super) fn nested_initial(source: bool) -> Result<ProjectDocument> {
    let root = NodeId::new("root")?;
    let outer = NodeId::new("outer")?;
    let inner = NodeId::new("inner")?;
    let first = NodeId::new("first")?;
    let second = NodeId::new("second")?;
    let initial = ProjectDocument::new(
        ProjectId::new("nested-project")?,
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
            audio_treatments: Default::default(),
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
        (root, BeatNode::sequence("Root", vec![outer.clone()])),
        (outer, BeatNode::sequence("Outer", vec![inner.clone()])),
        (
            inner,
            BeatNode::sequence("Inner", vec![first.clone(), second.clone()]),
        ),
        (first, first_node),
        (second, BeatNode::hold("Second", silent_hold(3)?)),
    ]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn pause(initial: &ProjectDocument, at: i64) -> Result<CommandRequest> {
    let revision = RevisionId::new("nested-insert")?;
    Ok(CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision.clone(),
        command: Command::InsertTime {
            at: ProjectFrame(at),
            hold: silent_hold(2)?,
            id: NodeId::new("pause")?,
            identities: SplitIdentities {
                nodes: if at == 4 {
                    Vec::new()
                } else {
                    ["left", "right", "split-owner"]
                        .into_iter()
                        .map(NodeId::new)
                        .collect::<std::result::Result<_, _>>()?
                },
            },
            timing: AudioTimingId {
                allocation: revision,
                ordinal: 0,
            },
        },
    })
}

#[test]
fn schema25_context_admits_root_seams_but_not_nested_sequence_interiors() -> Result {
    for source in [false, true] {
        let initial = nested_initial(source)?;
        for seam in [0, 7] {
            legacy_v25::validate_request_context(&initial, &pause(&initial, seam)?)?;
        }
        for interior in [2, 4] {
            assert!(
                legacy_v25::validate_request_context(&initial, &pause(&initial, interior)?)
                    .is_err()
            );
        }
    }
    Ok(())
}

#[test]
fn matching_modern_nested_history_cannot_claim_schema31() -> Result {
    for (source, at) in [(false, 2), (true, 2), (false, 4)] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("forged-sequence.deadpan");
        let initial = nested_initial(source)?;
        let request = pause(&initial, at)?;
        let mut store = ProjectStore::create(&package, &initial)?;
        store.commit(&request)?;
        store.validate()?;
        let modern = store.snapshot()?;
        assert_eq!(
            modern.duration()?.frames(),
            initial.duration()?.frames() + 2
        );
        // New admission keeps both Sequence owners live, unlike an old root seam.
        assert_eq!(
            modern.nodes()[&NodeId::new("root")?],
            initial.nodes()[&NodeId::new("root")?]
        );
        assert_eq!(
            modern.nodes()[&NodeId::new("outer")?],
            initial.nodes()[&NodeId::new("outer")?]
        );
        drop(store);
        // Deliberately forge the version only in this negative test. The positive
        // fixture is produced by the preserved real core25/database31 executable.
        let database = Connection::open(package.join("project.sqlite"))?;
        database.execute(
            "UPDATE revisions SET document=json_set(document,'$.schema_version',25)",
            [],
        )?;
        database.pragma_update(None, "user_version", 31)?;
        let edit = apply(&initial, &request)?;
        let edit_json = history_json(&database)?[0].1.clone();
        for (_, wire) in docs(&database)? {
            legacy_v25::Document::from_json(&wire)?;
        }
        assert!(legacy_v25::matches_edit(&edit_json, &edit)?);
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, source } =
            ProjectStore::migrate(&package).unwrap_err()
        else {
            panic!("schema 31 accepted formerly unadmitted nested Sequence insertion");
        };
        let expected = legacy_v25::validate_request_context(&initial, &request).unwrap_err();
        match source.as_ref() {
            StoreError::Edit(error) => {
                assert_eq!(error.code, expected.code);
                assert_eq!(error.message, expected.message);
            }
            _ => panic!("schema 31: {source}"),
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            31
        );
    }
    Ok(())
}

#[test]
fn schema25_snapshot_request_and_patch_fields_remain_closed_without_promotion() -> Result {
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
            panic!("schema 31 accepted unknown {position} field");
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            31
        );
    }
    Ok(())
}
