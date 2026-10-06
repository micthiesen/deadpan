//! Revisions are stored as keyframe snapshots plus forward patches, and pause
//! history records only the retained timing state it changes.

use std::collections::BTreeMap;
use std::error::Error;

use deadpan_core::{
    AudioTimingId, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate,
    HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectFrame,
    ProjectId, RevisionId, SplitIdentities, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn hold(frames: i64) -> Result<HoldRecipe> {
    Ok(HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(frames)?,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    })
}

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("revision-storage")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn request(document: &ProjectDocument, revision: &str, command: Command) -> Result<CommandRequest> {
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command,
    })
}

fn beats(count: usize) -> Result<Command> {
    let group = NodeId::new("beats")?;
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    for index in 0..count {
        let id = NodeId::new(format!("b{index:04}"))?;
        nodes.insert(id.clone(), BeatNode::hold("Beat", hold(3)?));
        children.push(id);
    }
    nodes.insert(group.clone(), BeatNode::sequence("Beats", children));
    Ok(Command::Insert {
        parent: NodeId::new("root")?,
        index: 0,
        subtree: Subtree {
            root: group,
            nodes,
            overrides: Default::default(),
            gap_overrides: Default::default(),
        },
    })
}

fn pause(document: &ProjectDocument, revision: &str, at: i64) -> Result<CommandRequest> {
    let target = document.insert_time_target(ProjectFrame(at))?;
    let needed = target.split.map_or(0, |split| split.required_ids);
    request(
        document,
        revision,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: hold(2)?,
            id: NodeId::new(format!("{revision}-pause"))?,
            identities: SplitIdentities {
                nodes: (0..needed)
                    .map(|index| NodeId::new(format!("{revision}-split-{index}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: RevisionId::new(revision)?,
                ordinal: 0,
            },
        },
    )
}

/// One revision's stored keyframe metadata.
fn metadata(path: &std::path::Path) -> Result<Vec<(String, String, i64, i64)>> {
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = database
        .prepare("SELECT id, document, depth, json_bound FROM revisions ORDER BY rowid")?
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

#[test]
fn keyframes_bound_every_chain_and_rebuild_every_revision() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("chain.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let mut expected = vec![store.snapshot()?];
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(8)?)?)?;
    expected.push(store.snapshot()?);
    // Pauses, undo and redo produce every revision kind and enough revisions
    // to cross more than one keyframe interval.
    for index in 2..140 {
        let current = store.snapshot()?;
        let revision = format!("r{index}");
        match index % 5 {
            3 => {
                store.undo(current.revision_id(), RevisionId::new(&revision)?)?;
            }
            4 if index % 10 == 4 => {
                store.redo(current.revision_id(), RevisionId::new(&revision)?)?;
            }
            _ => {
                let total = current.duration()?.frames();
                store.commit(&pause(&current, &revision, (index as i64 * 7) % total)?)?;
            }
        }
        expected.push(store.snapshot()?);
    }
    let rows = metadata(&path)?;
    assert_eq!(rows.len(), expected.len());
    // Only the initial revision and every 64th in the linear chronology store
    // a document, and each bound covers the actual compact document.
    for (index, ((id, document, depth, bound), snapshot)) in rows.iter().zip(&expected).enumerate()
    {
        assert_eq!(*depth, (index % 64) as i64, "{id}");
        assert_eq!(document != "null", index % 64 == 0, "{id}");
        assert!(
            snapshot.to_compact_json()?.len() as i64 <= *bound,
            "{id}: {bound}"
        );
    }
    store.validate_full()?;
    for snapshot in &expected {
        assert_eq!(&store.snapshot_at(snapshot.revision_id())?, snapshot);
    }
    drop(store);
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        let store = ProjectStore::open(&path, mode)?;
        assert_eq!(store.open_validation().replayed, 0);
        assert_eq!(&store.snapshot()?, expected.last().unwrap());
        for snapshot in &expected {
            assert_eq!(&store.snapshot_at(snapshot.revision_id())?, snapshot);
        }
    }
    // A non-initial keyframe changed without changing its length, so its
    // size bound and every later chain still hold: the receipt's chain and
    // the replay it forces must reject it.
    let keyframe = &rows[64].0;
    let database = Connection::open(path.join("project.sqlite"))?;
    let changed = database.execute(
        "UPDATE revisions SET document=replace(document,'\"label\":\"Beats\"','\"label\":\"Bxats\"') WHERE id=?1",
        [keyframe],
    )?;
    assert_eq!(changed, 1);
    let replaced: bool = database.query_row(
        "SELECT instr(document,'\"label\":\"Bxats\"')>0 FROM revisions WHERE id=?1",
        [keyframe],
        |row| row.get(0),
    )?;
    assert!(replaced, "the keyframe holds the Beats group");
    drop(database);
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(ProjectStore::open(&path, mode).is_err(), "{mode:?}");
    }
    Ok(())
}

/// A small history with every revision kind.
fn history(path: &std::path::Path) -> Result<ProjectStore> {
    let mut store = ProjectStore::create(path, &document()?)?;
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(6)?)?)?;
    for (index, at) in [(2, 4), (3, 10), (4, 1)] {
        let current = store.snapshot()?;
        store.commit(&pause(&current, &format!("r{index}"), at)?)?;
    }
    store.undo(&RevisionId::new("r4")?, RevisionId::new("r5")?)?;
    store.undo(&RevisionId::new("r5")?, RevisionId::new("r6")?)?;
    store.redo(&RevisionId::new("r6")?, RevisionId::new("r7")?)?;
    let current = store.snapshot()?;
    store.commit(&request(
        &current,
        "r8",
        Command::Rename {
            node: NodeId::new("beats")?,
            label: "Renamed".into(),
        },
    )?)?;
    Ok(store)
}

#[test]
fn receipts_replace_replay_and_commits_extend_them() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("receipt.deadpan");
    let store = history(&path)?;
    // Every commit extended the receipt, so nothing needs recomputing.
    let report = store.validate_report(false)?;
    assert_eq!(
        (
            report.revisions,
            report.verified_by_receipt,
            report.replayed
        ),
        (9, 9, 0)
    );
    let full = store.validate_report(true)?;
    assert_eq!((full.verified_by_receipt, full.replayed), (0, 8));
    let head = store.snapshot()?;
    drop(store);
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        let store = ProjectStore::open(&path, mode)?;
        assert_eq!(store.open_validation().replayed, 0);
        assert_eq!(store.snapshot()?, head);
    }
    // Without a receipt, reading replays everything; a writer then proves it.
    Connection::open(path.join("project.sqlite"))?.execute("DELETE FROM history_receipt", [])?;
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?
            .open_validation()
            .replayed,
        8
    );
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadWrite)?
            .open_validation()
            .replayed,
        8
    );
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?
            .open_validation()
            .replayed,
        0
    );
    // A receipt from any other validator build proves nothing.
    Connection::open(path.join("project.sqlite"))?.execute(
        "UPDATE history_receipt SET validator='deadpan-history-v1/other'",
        [],
    )?;
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?
            .open_validation()
            .replayed,
        8
    );
    Ok(())
}

#[test]
fn a_receipt_never_hides_a_modified_row() -> Result {
    // Changes that keep every structural check satisfied: only the broken
    // hash chain, and the replay it forces, can find them. Widening every
    // elided row's size bound by 100,000 bytes per patch keeps the bound
    // checks satisfied whatever the tampered lengths are.
    let widen = "UPDATE revisions SET json_bound=json_bound+100000*depth;";
    let semantic = [
        "UPDATE history SET edit=json_set(edit,'$.description','forged') WHERE revision_id='r3'",
        "UPDATE history SET request=json_set(request,'$.command.InsertTime.at',5) WHERE revision_id='r3'",
        "UPDATE history SET edit=json_set(edit,'$.duration_delta',7) WHERE revision_id='r2'",
        "UPDATE revision_patches SET patch=(SELECT patch FROM revision_patches WHERE revision_id='r5') WHERE revision_id='r6'",
        "UPDATE revision_patches SET patch=json_set(patch,'$.nodes',json('{}')) WHERE revision_id='r7'",
    ];
    for change in semantic {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("tampered.deadpan");
        drop(history(&path)?);
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(widen)?;
        database.execute_batch(change)?;
        drop(database);
        for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
            let message = match ProjectStore::open(&path, mode) {
                Ok(_) => panic!("accepted {change} with {mode:?}"),
                Err(error) => error.to_string(),
            };
            for structural in [
                "keyframe metadata",
                "parent or revision",
                "has no target",
                "is not a child",
                "bound",
            ] {
                assert!(
                    !message.contains(structural),
                    "{change}: rejected before replay: {message}"
                );
            }
        }
    }
    // Inconsistent row structure is rejected by the row checks themselves.
    let structural = [
        "UPDATE revisions SET kind='redo' WHERE id='r5'",
        "UPDATE revisions SET json_bound=json_bound-5000 WHERE id='r4'",
        "DELETE FROM revision_patches WHERE revision_id='r7'",
        "UPDATE state SET cursor=NULL",
    ];
    for change in structural {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("tampered.deadpan");
        drop(history(&path)?);
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch("PRAGMA foreign_keys=OFF;")?;
        database.execute_batch(change)?;
        drop(database);
        for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
            assert!(
                ProjectStore::open(&path, mode).is_err(),
                "accepted {change} with {mode:?}"
            );
        }
    }
    // Reformatting a stored command without changing its meaning breaks the
    // chain but not the history. The receipt names only the chain value at
    // its last revision, so the whole chronology is recomputed and accepted.
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("reformatted.deadpan");
    drop(history(&path)?);
    Connection::open(path.join("project.sqlite"))?.execute(
        "UPDATE history SET request=json(request) || ' ' WHERE revision_id='r3'",
        [],
    )?;
    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        (
            store.open_validation().verified_by_receipt,
            store.open_validation().replayed
        ),
        (0, 8)
    );
    Ok(())
}

#[test]
fn a_tampered_navigation_patch_is_rejected() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("tampered.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(3)?)?)?;
    let current = store.snapshot()?;
    store.commit(&pause(&current, "r2", 4)?)?;
    store.undo(&RevisionId::new("r2")?, RevisionId::new("r3")?)?;
    store.redo(&RevisionId::new("r3")?, RevisionId::new("r4")?)?;
    store.undo(&RevisionId::new("r4")?, RevisionId::new("r5")?)?;
    store.validate()?;
    drop(store);
    let database = Connection::open(path.join("project.sqlite"))?;
    // r4 is elided; replace its redo patch with r3's undo patch.
    let r4: String =
        database.query_row("SELECT document FROM revisions WHERE id='r4'", [], |row| {
            row.get(0)
        })?;
    assert_eq!(r4, "null");
    database.execute(
        "UPDATE revision_patches SET patch=(SELECT patch FROM revision_patches WHERE revision_id='r3') WHERE revision_id='r4'",
        [],
    )?;
    drop(database);
    assert!(ProjectStore::open(&path, AccessMode::ReadOnly).is_err());
    Ok(())
}

#[test]
fn pause_history_records_only_changed_timing_state() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("pauses.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(400)?)?)?;
    let mut sizes = Vec::new();
    for (index, at) in [(2, 7), (3, 600), (4, 31), (5, 902), (6, 13)] {
        let current = store.snapshot()?;
        let revision = format!("r{index}");
        store.commit(&pause(&current, &revision, at)?)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        sizes.push(database.query_row(
            "SELECT length(edit) FROM history WHERE revision_id=?1",
            [revision.as_str()],
            |row| row.get::<_, i64>(0),
        )?);
    }
    let document = store.snapshot()?;
    // The first pause binds every Hold; later pauses add one small timing
    // table and a few bindings, never another copy of the project. Their
    // remaining bytes are the parent Sequence's child list in the node patch,
    // stored once (its new list is a splice and the inverse is implied).
    assert!(sizes[0] > 150_000, "{sizes:?}");
    for size in &sizes[1..] {
        assert!(*size < 30_000, "{sizes:?}");
    }
    let timings = document.audio_bindings().timings();
    // An interior pause names two tables: lattices before its Split and
    // placements after it.
    assert!(timings.len() <= 10, "{}", timings.len());
    let largest = timings
        .values()
        .map(|layout| layout.nodes().len())
        .max()
        .unwrap();
    let smaller: Vec<_> = timings
        .values()
        .map(|layout| layout.nodes().len())
        .filter(|nodes| *nodes != largest)
        .collect();
    assert!(smaller.iter().all(|nodes| *nodes < 16), "{smaller:?}");
    Ok(())
}

#[test]
fn an_older_package_with_whole_state_binding_history_is_refused_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("older.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(3)?)?)?;
    let before = store.snapshot()?;
    store.commit(&pause(&before, "r2", 4)?)?;
    let after = store.snapshot()?;
    drop(store);
    // Rewrite the pause entry in the schema-62 form: complete binding states
    // on both sides of each patch, with no revision patch table.
    let database = Connection::open(path.join("project.sqlite"))?;
    let edit: String = database.query_row(
        "SELECT edit FROM history WHERE revision_id='r2'",
        [],
        |row| row.get(0),
    )?;
    let mut edit: serde_json::Value = serde_json::from_str(&edit)?;
    let states = [
        serde_json::to_value(before.audio_bindings())?,
        serde_json::to_value(after.audio_bindings())?,
    ];
    edit["forward"]["audio_bindings"] =
        serde_json::json!({"before": states[0], "after": states[1]});
    edit["inverse"]["audio_bindings"] =
        serde_json::json!({"before": states[1], "after": states[0]});
    database.execute(
        "UPDATE history SET edit=?1 WHERE revision_id='r2'",
        [edit.to_string()],
    )?;
    database.execute_batch("DROP TABLE revision_patches; PRAGMA user_version=62;")?;
    database.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(database);
    let bytes = std::fs::read(path.join("project.sqlite"))?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(deadpan_store::StoreError::UnsupportedSchema(62))
        ));
    }
    assert_eq!(std::fs::read(path.join("project.sqlite"))?, bytes);
    Ok(())
}

struct Lcg(u64);

impl Lcg {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound.max(1)
    }
}

fn children(document: &ProjectDocument, parent: &NodeId) -> Vec<NodeId> {
    match &document.nodes()[parent].kind {
        deadpan_core::NodeKind::Sequence { children } => children.clone(),
        _ => Vec::new(),
    }
}

/// A random edit on the beats group, or None when the draw does not apply.
fn random_command(
    document: &ProjectDocument,
    random: &mut Lcg,
    revision: &str,
) -> Result<Option<Command>> {
    let group = NodeId::new("beats")?;
    let beats = children(document, &group);
    let pick = |random: &mut Lcg| beats[random.below(beats.len() as u64) as usize].clone();
    let timing = || -> Result<AudioTimingId> {
        Ok(AudioTimingId {
            allocation: RevisionId::new(revision)?,
            ordinal: 0,
        })
    };
    Ok(Some(match random.below(6) {
        0 => return Ok(None),
        1 if !beats.is_empty() => {
            let node = pick(random);
            let duration = document.node_duration(&node)?.frames();
            if duration < 2
                || !matches!(
                    document.nodes()[&node].kind,
                    deadpan_core::NodeKind::Hold { .. }
                )
            {
                return Ok(None);
            }
            Command::Split {
                node,
                at: FrameDuration::new(1 + random.below(duration as u64 - 1) as i64)?,
                identities: SplitIdentities {
                    nodes: (0..8)
                        .map(|index| NodeId::new(format!("{revision}-split-{index}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
            }
        }
        2 if !beats.is_empty() => Command::WrapRepeat {
            node: pick(random),
            id: NodeId::new(format!("{revision}-repeat"))?,
            plays: 2 + random.below(2) as u32,
            gap: (random.below(2) == 0).then(|| hold(2)).transpose()?,
            anchor_policy: Default::default(),
        },
        3 if beats.len() > 2 => Command::DeleteRipple {
            node: pick(random),
            timing: timing()?,
        },
        4 if !beats.is_empty() => {
            let node = pick(random);
            if !matches!(
                document.nodes()[&node].kind,
                deadpan_core::NodeKind::Hold { .. }
            ) {
                return Ok(None);
            }
            Command::SetHoldDuration {
                node,
                duration: FrameDuration::new(1 + random.below(6) as i64)?,
            }
        }
        _ => {
            let total = document.duration()?.frames();
            let at = random.below(total as u64 + 1) as i64;
            let Ok(target) = document.insert_time_target(ProjectFrame(at)) else {
                return Ok(None);
            };
            let needed = target.split.map_or(0, |split| split.required_ids);
            Command::InsertTime {
                at: ProjectFrame(at),
                hold: hold(1 + random.below(3) as i64)?,
                id: NodeId::new(format!("{revision}-pause"))?,
                identities: SplitIdentities {
                    nodes: (0..needed)
                        .map(|index| NodeId::new(format!("{revision}-pause-split-{index}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
                timing: timing()?,
            }
        }
    }))
}

/// The commit path adopts the core's validated result instead of applying and
/// validating the stored patch again, stores patches instead of documents and
/// extends the receipt instead of replaying. Every step must equal what the
/// complete path computes: the validated patch application, the document
/// rebuilt from storage by an independent reader, full document validation,
/// the stored size bound, and finally full history replay.
#[test]
fn random_commits_equal_full_validation_and_storage_rebuilds() -> Result {
    for seed in 1..=4_u64 {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join(format!("random-{seed}.deadpan"));
        let mut store = ProjectStore::create(&path, &document()?)?;
        let current = store.snapshot()?;
        store.commit(&request(&current, "r1", beats(10)?)?)?;
        let mut random = Lcg(seed);
        let mut committed = 0;
        for step in 2..70 {
            let previous = store.snapshot()?;
            let revision = format!("s{seed}-{step}");
            let outcome = match random.below(10) {
                0 | 1 => store.undo(previous.revision_id(), RevisionId::new(&revision)?),
                2 => store.redo(previous.revision_id(), RevisionId::new(&revision)?),
                _ => match random_command(&previous, &mut random, &revision)? {
                    Some(command) => store.commit(&request(&previous, &revision, command)?),
                    None => continue,
                },
            };
            let Ok(outcome) = outcome else {
                // A refused draw leaves the committed head unchanged.
                assert_eq!(store.snapshot()?, previous);
                continue;
            };
            committed += 1;
            let head = store.snapshot()?;
            assert_eq!(head.revision_id(), &outcome.revision_id);
            assert_eq!(outcome.edit.forward.apply(&previous)?, head);
            assert_eq!(outcome.edit.inverse.apply(&head)?.nodes(), previous.nodes());
            head.validate()?;
            let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
            assert_eq!(reader.snapshot()?, head);
            assert_eq!(reader.open_validation().replayed, 0);
            let (_, _, _, bound) = metadata(&path)?.pop().unwrap();
            assert!(head.to_compact_json()?.len() as i64 <= bound);
        }
        assert!(committed > 20, "seed {seed}: {committed}");
        let report = store.validate_report(true)?;
        assert_eq!(report.replayed + 1, report.revisions);
    }
    Ok(())
}
