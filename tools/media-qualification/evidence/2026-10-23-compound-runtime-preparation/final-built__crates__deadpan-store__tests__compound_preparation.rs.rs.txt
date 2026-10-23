//! Host preparation shares the authoritative Compound transaction. SQL triggers
//! below inject write failures; the fixture has only structural Hold content.

use deadpan_core::*;
use deadpan_store::{
    AccessMode, CompoundPreparationError, ProjectStore, TranscriptKey,
    TranscriptSelectionCorrectionRule, TranscriptSelectionCorrections, TranscriptSelectionFormat,
    TranscriptSelectionGuard,
};
use rusqlite::{Connection, OpenFlags};
use std::{
    cell::Cell,
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
type DatabaseCells = Vec<(String, Vec<Vec<String>>)>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn ordinary(document: &ProjectDocument, id: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(id),
        command,
    }
}

fn create(path: &Path) -> Result<ProjectStore> {
    let document = ProjectDocument::new(
        ProjectId::new("prepared-compound")?,
        revision("empty"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(24, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let request = ordinary(
        &document,
        "initial",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("hold"),
                nodes: BTreeMap::from([(
                    node("hold"),
                    BeatNode::hold(
                        "Initial Hold",
                        HoldRecipe {
                            duration: FrameDuration::new(12)?,
                            picture_context: None,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let transaction = apply(&document, &request)?;
    let document = transaction.forward.apply(&document)?;
    Ok(ProjectStore::create(path, &document)?)
}

fn compound(store: &ProjectStore, id: &str, authored: bool) -> Result<CommandRequest> {
    let document = store.snapshot()?;
    let set = CapturedEditSet::capture_ranges(
        &document,
        document.root(),
        &[FrameRange::new(ProjectFrame(1), ProjectFrame(3))?],
        AudioTimingId {
            allocation: revision(&format!("{id}-capture")),
            ordinal: 0,
        },
    )?;
    let mut steps = vec![ResolvedStep::Yank {
        name: RegisterName::new('a')?,
        value: Arc::new(RegisterValue::EditedSet { set: Arc::new(set) }),
    }];
    if authored {
        steps.push(ResolvedStep::Edit {
            edit: LeafEdit::new(
                revision(&format!("{id}-leaf")),
                Command::Rename {
                    node: node("hold"),
                    label: "Prepared edit".into(),
                },
            )?,
        });
    }
    Ok(ordinary(
        &document,
        id,
        Command::Compound {
            transaction: ResolvedTransaction::new(
                store.register_version()?,
                BTreeMap::new(),
                steps,
            )?,
        },
    ))
}

fn cells(path: &Path) -> Result<DatabaseCells> {
    let database = Connection::open_with_flags(
        path.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let tables = database
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut result = Vec::new();
    for table in tables {
        let mut statement =
            database.prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))?;
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| Ok(format!("{:?}", row.get_ref(column)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.sort();
        result.push((table, rows));
    }
    Ok(result)
}

#[test]
fn host_failure_rolls_back_authored_and_register_only_preparation_and_allows_retry() -> Result {
    for authored in [false, true] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("failure.deadpan");
        let mut store = create(&path)?;
        let before = store.snapshot()?;
        store.commit(&ordinary(
            &before,
            "later",
            Command::Rename {
                node: node("hold"),
                label: "Later edit".into(),
            },
        ))?;
        store.undo(&revision("later"), revision("undone"))?;
        assert_eq!(store.history_availability()?, (false, true));
        let request = compound(&store, "retry", authored)?;
        let document = store.snapshot()?;
        let bank = store.registers()?;
        let before = cells(&path)?;
        let called = Cell::new(0);
        let failed = store.commit_compound_prepared(&request, None, |prepared| {
            called.set(called.get() + 1);
            assert_eq!(prepared.version, bank.version + 1);
            assert!(
                prepared
                    .entries
                    .contains_key(&RegisterName::new('a').unwrap())
            );
            Err::<(), _>("host rejected prepared state")
        });
        assert!(matches!(
            failed,
            Err(CompoundPreparationError::Preparation(
                "host rejected prepared state"
            ))
        ));
        assert_eq!(called.get(), 1);
        assert_eq!(cells(&path)?, before);
        assert_eq!(store.snapshot()?, document);
        assert_eq!(store.registers()?, bank);
        assert_eq!(store.history_availability()?, (false, true));

        let (outcome, prepared) = store.commit_compound_prepared(&request, None, |bank| {
            Ok::<_, std::convert::Infallible>(bank.clone())
        })?;
        assert_eq!(outcome.register_bank, prepared);
        assert_eq!(outcome.committed.is_some(), authored);
        assert_eq!(store.registers()?, prepared);
        assert_eq!(
            store.history_availability()?,
            if authored {
                (true, false)
            } else {
                (false, true)
            }
        );
        if !authored {
            assert_eq!(store.snapshot()?, document);
            store.redo(document.revision_id(), revision("redo-after-register"))?;
            assert_eq!(store.snapshot()?.nodes()[&node("hold")].label, "Later edit");
        }
        store.validate_report(true)?;
    }
    Ok(())
}

#[test]
fn authoritative_refusals_happen_before_host_preparation() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("refusals.deadpan");
    let mut store = create(&path)?;
    let valid = compound(&store, "candidate", true)?;
    let mut stale_head = valid.clone();
    stale_head.expected_revision = revision("stale");
    let mut stale_bank = valid.clone();
    let Command::Compound { transaction } = &valid.command else {
        unreachable!()
    };
    stale_bank.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version() + 1,
            transaction.inputs().clone(),
            transaction.steps().to_vec(),
        )?,
    };
    let mut reused_root = valid.clone();
    reused_root.new_revision = revision("initial");
    let mut reused_leaf = valid.clone();
    let mut steps = transaction.steps().to_vec();
    let ResolvedStep::Edit { edit } = &mut steps[1] else {
        unreachable!()
    };
    edit.new_revision = revision("initial");
    reused_leaf.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version(),
            transaction.inputs().clone(),
            steps,
        )?,
    };
    let before = cells(&path)?;
    for request in [stale_head, stale_bank, reused_root, reused_leaf] {
        let called = Cell::new(false);
        let result = store.commit_compound_prepared(&request, None, |_| {
            called.set(true);
            Ok::<_, std::convert::Infallible>(())
        });
        assert!(matches!(result, Err(CompoundPreparationError::Store(_))));
        assert!(!called.get());
        assert_eq!(cells(&path)?, before);
    }
    let mut reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let called = Cell::new(false);
    assert!(matches!(
        reader.commit_compound_prepared(&valid, None, |_| {
            called.set(true);
            Ok::<_, std::convert::Infallible>(())
        }),
        Err(CompoundPreparationError::Store(
            deadpan_store::StoreError::ReadOnly
        ))
    ));
    assert!(!called.get());
    Ok(())
}

#[test]
fn prepared_commit_matches_preview_commit_history_and_reopening() -> Result {
    let scratch = tempfile::tempdir()?;
    let old_path = scratch.path().join("old.deadpan");
    let new_path = scratch.path().join("new.deadpan");
    let mut old = create(&old_path)?;
    let mut new = create(&new_path)?;
    let request = compound(&old, "accepted", true)?;
    let preview = old.preview_compound(&request)?;
    let expected = old.commit_compound(&request, None)?;
    let called = Cell::new(0);
    let (outcome, prepared) = new.commit_compound_prepared(&request, None, |bank| {
        called.set(called.get() + 1);
        assert_eq!(*bank, preview.register_bank);
        Ok::<_, std::convert::Infallible>(bank.clone())
    })?;
    assert_eq!(called.get(), 1);
    assert_eq!(
        serde_json::to_value(outcome)?,
        serde_json::to_value(expected)?
    );
    assert_eq!(new.registers()?, prepared);
    assert_eq!(new.snapshot()?, old.snapshot()?);
    assert_eq!(cells(&new_path)?, cells(&old_path)?);
    for store in [&mut old, &mut new] {
        store.undo(&revision("accepted"), revision("undo"))?;
        assert_eq!(
            store.snapshot()?.nodes()[&node("hold")].label,
            "Initial Hold"
        );
        assert_eq!(store.registers()?, prepared);
        store.redo(&revision("undo"), revision("redo"))?;
        assert_eq!(
            store.snapshot()?.nodes()[&node("hold")].label,
            "Prepared edit"
        );
        store.validate_report(true)?;
    }
    assert_eq!(cells(&new_path)?, cells(&old_path)?);
    drop(old);
    drop(new);
    let old = ProjectStore::open(&old_path, AccessMode::ReadOnly)?;
    let new = ProjectStore::open(&new_path, AccessMode::ReadOnly)?;
    assert_eq!(
        new.snapshot_with_registers()?,
        old.snapshot_with_registers()?
    );
    new.validate_report(true)?;
    Ok(())
}

struct PreparedHost(Arc<AtomicUsize>);

impl Drop for PreparedHost {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn write_failure_discards_host_state_and_rolls_back_before_retry() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("write-failure.deadpan");
    let mut store = create(&path)?;
    let request = compound(&store, "accepted", true)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER reject_prepared_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'injected prepared history failure'); END;")?;
    let before = cells(&path)?;
    let dropped = Arc::new(AtomicUsize::new(0));
    let called = Cell::new(0);
    let result = store.commit_compound_prepared(&request, None, |_| {
        called.set(called.get() + 1);
        Ok::<_, std::convert::Infallible>(PreparedHost(Arc::clone(&dropped)))
    });
    assert!(matches!(result, Err(CompoundPreparationError::Store(_))));
    assert_eq!(called.get(), 1);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(cells(&path)?, before);
    assert_eq!(store.snapshot()?.revision_id(), &revision("initial"));
    assert_eq!(store.register_version()?, 0);
    database.execute_batch("DROP TRIGGER reject_prepared_history;")?;
    let (outcome, host) = store.commit_compound_prepared(&request, None, |_| {
        Ok::<_, std::convert::Infallible>(PreparedHost(Arc::clone(&dropped)))
    })?;
    assert!(outcome.committed.is_some());
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    drop(host);
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
    store.validate_report(true)?;
    Ok(())
}

#[test]
fn unavailable_transcript_authority_refuses_before_host_preparation() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("guard.deadpan");
    let mut store = create(&path)?;
    let request = compound(&store, "guarded", true)?;
    let guard = TranscriptSelectionGuard {
        format: TranscriptSelectionFormat::V1,
        asset: AssetId::new("missing-original")?,
        qualification: SourceQualificationId::new("a".repeat(64))?,
        key: TranscriptKey {
            content: "b".repeat(64),
            audio_stream: 0,
            model_sha256: "c".repeat(64),
            language: "en".into(),
            engine: "guard-test".into(),
        },
        proposal_sha256: "d".repeat(64),
        correction_rule: TranscriptSelectionCorrectionRule::V1,
        corrections: TranscriptSelectionCorrections::Absent,
    };
    let before = cells(&path)?;
    let called = Cell::new(false);
    let result =
        store.commit_compound_with_transcript_guard_prepared(&request, None, &guard, |_| {
            called.set(true);
            Ok::<_, std::convert::Infallible>(())
        });
    assert!(matches!(result, Err(CompoundPreparationError::Store(_))));
    assert!(!called.get());
    assert_eq!(cells(&path)?, before);
    Ok(())
}
