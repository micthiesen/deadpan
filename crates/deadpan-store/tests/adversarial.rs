//! Gate G tampered-package regression. A real package with undo/redo history
//! is corrupted either at the SQLite file level (arbitrary byte damage) or at
//! the row level (one cell rewritten, nulled, retyped or deleted in any table).
//! Opening read-only and running full validation must then fail with a typed
//! `StoreError`, or succeed with exactly the original head document: a change
//! that validates as a different head is undetected tampering. Read-write
//! opening must not panic either. See docs/ADVERSARIAL.md.

use deadpan_chaos::{Outcome, Target, Verdict, fuzz, reject};
use deadpan_core::{
    ColorPolicy, CommandRequest, FrameRate, NodeId, PresentationBasis, ProjectDocument, ProjectId,
    RevisionId,
};
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::{Connection, types::Value as Sql};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[global_allocator]
static ALLOCATOR: deadpan_chaos::CountingAllocator = deadpan_chaos::CountingAllocator;

fn request(
    document: &ProjectDocument,
    revision: &str,
    command: serde_json::Value,
) -> CommandRequest {
    serde_json::from_value(json!({
        "project_id": document.project_id(),
        "expected_revision": document.revision_id(),
        "new_revision": revision,
        "command": command,
    }))
    .unwrap()
}

/// Everything a validated package exposes: every revision and the registers.
#[derive(PartialEq)]
struct Expected {
    revisions: Vec<(RevisionId, ProjectDocument)>,
    head: ProjectDocument,
    /// Register contents. The bank `version` is a cache identity, not
    /// authored state, and is deliberately outside the comparison.
    registers: std::collections::BTreeMap<
        deadpan_core::RegisterName,
        std::sync::Arc<deadpan_core::RegisterValue>,
    >,
}

fn observe(store: &ProjectStore, package: &Path) -> Result<Expected, deadpan_store::StoreError> {
    let ids: Vec<String> = {
        let connection = Connection::open_with_flags(
            package.join("project.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(deadpan_store::StoreError::from)?;
        let mut statement = connection
            .prepare("SELECT id FROM revisions ORDER BY id")
            .map_err(deadpan_store::StoreError::from)?;
        statement
            .query_map([], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(deadpan_store::StoreError::from)?
    };
    let mut revisions = Vec::new();
    for id in ids {
        let Ok(revision) = RevisionId::new(&id) else {
            continue;
        };
        let document = store.snapshot_at(&revision)?;
        revisions.push((revision, document));
    }
    Ok(Expected {
        revisions,
        head: store.snapshot()?,
        registers: store.registers()?.entries,
    })
}

/// A package with inserts, a Repeat, renames, a deletion and undo/redo.
fn template(directory: &Path) -> (PathBuf, Expected) {
    let initial = ProjectDocument::new(
        ProjectId::new("adversarial").unwrap(),
        RevisionId::new("r0").unwrap(),
        PresentationBasis {
            width: 64,
            height: 36,
            frame_rate: FrameRate::new(24, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let path = directory.join("template.deadpan");
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    let mut serial = 0;
    let mut next = |store: &mut ProjectStore, command: serde_json::Value| {
        serial += 1;
        let document = store.snapshot().unwrap();
        store
            .commit(&request(&document, &format!("r{serial}"), command))
            .unwrap();
    };
    for index in 0..6 {
        next(
            &mut store,
            json!({"command": "insert", "parent": "root", "index": 0, "subtree": {
                "root": format!("hold-{index}"),
                "nodes": {format!("hold-{index}"): {"label": "Pause",
                    "kind": {"type": "hold", "recipe": {"duration": 3 + index,
                        "video": {"type": "background"}, "audio": {"type": "silence"}}}}}}}),
        );
    }
    next(
        &mut store,
        json!({"command": "rename", "node": "hold-2", "label": "Renamed"}),
    );
    next(
        &mut store,
        json!({"command": "wrap_repeat", "node": "hold-3", "id": "repeat", "plays": 3, "gap": null}),
    );
    next(&mut store, json!({"command": "delete", "node": "hold-0"}));
    let head = store.head_revision().unwrap();
    store.undo(&head, RevisionId::new("u1").unwrap()).unwrap();
    store
        .redo(
            &RevisionId::new("u1").unwrap(),
            RevisionId::new("u2").unwrap(),
        )
        .unwrap();
    let head = store.snapshot().unwrap();
    // One edited register so register rows and contents are under test too.
    let slice = deadpan_core::CapturedEditSlice::capture_selection(
        &head,
        head.root(),
        &deadpan_core::SliceCaptureSelection::Child {
            node: NodeId::new("hold-1").unwrap(),
        },
        serde_json::from_value(json!({"allocation": "copy", "ordinal": 0})).unwrap(),
    )
    .unwrap();
    store
        .save_register(
            head.project_id(),
            head.revision_id(),
            serde_json::from_value(json!("a")).unwrap(),
            deadpan_core::RegisterValue::Edited {
                slice: std::sync::Arc::new(slice),
            },
        )
        .unwrap();
    drop(store);
    // Fold the WAL so the template is one self-contained database file.
    Connection::open(path.join("project.sqlite"))
        .unwrap()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
        .unwrap();
    let store = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    store.validate_full().unwrap();
    let expected = observe(&store, &path).unwrap();
    assert_eq!(expected.head, head);
    assert!(expected.revisions.len() >= 10);
    (path, expected)
}

fn copy_package(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in walk(from) {
        let relative = entry.strip_prefix(from).unwrap();
        let target = to.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else {
            std::fs::copy(&entry, &target).unwrap();
        }
    }
}

fn walk(directory: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            paths.push(path.clone());
            paths.extend(walk(&path));
        } else {
            paths.push(path);
        }
    }
    paths
}

/// Opens a damaged package and classifies the outcome.
fn judge(package: &Path, expected: &Expected) -> Outcome {
    let verdict = match ProjectStore::open(package, AccessMode::ReadOnly) {
        Ok(store) => match store.validate_full() {
            // Validation passed: every observable value must be unchanged.
            Ok(()) => match observe(&store, package) {
                Ok(observed) => {
                    if observed.head != expected.head {
                        return Err(
                            "tampered package validated with a different head document".into()
                        );
                    }
                    if observed.revisions != expected.revisions {
                        return Err(
                            "tampered package validated with a different historical revision"
                                .into(),
                        );
                    }
                    if observed.registers != expected.registers {
                        return Err(
                            "tampered package validated with a different register bank".into()
                        );
                    }
                    Verdict::Accepted
                }
                Err(error) => reject(error)?,
            },
            Err(error) => reject(error)?,
        },
        Err(error) => reject(error)?,
    };
    // Writer opening may recover; it must never panic. Its outcome is not
    // classified because recovery legitimately repairs operational state.
    if let Ok(store) = ProjectStore::open(package, AccessMode::ReadWrite) {
        let _ = store.validate();
    }
    Ok(verdict)
}

#[test]
fn adversarial_corrupted_database_files_fail_cleanly() {
    let scratch = tempfile::tempdir().unwrap();
    let (template, expected) = template(scratch.path());
    let database = std::fs::read(template.join("project.sqlite")).unwrap();
    let cases = tempfile::tempdir().unwrap();
    let counter = std::cell::Cell::new(0_u64);
    let report = fuzz(
        Target::bytes("store-database-bytes")
            .iterations(150)
            .max_input_bytes(database.len() + 64 * 1024)
            .max_case_time(Duration::from_secs(20))
            .max_case_alloc(512 << 20)
            .minimize_budget(200),
        vec![database],
        |input| {
            counter.set(counter.get() + 1);
            let package = cases.path().join(format!("case-{}.deadpan", counter.get()));
            copy_package(&template, &package);
            std::fs::write(package.join("project.sqlite"), input).unwrap();
            let outcome = judge(&package, &expected);
            let _ = std::fs::remove_dir_all(&package);
            outcome
        },
    );
    report.assert_clean();
}

/// Applies one cell-level tamper described by the input bytes:
/// `[table, column, row, operation, payload...]`.
fn tamper(connection: &Connection, input: &[u8]) -> Result<(), String> {
    let mut padded = input.to_vec();
    padded.resize(padded.len().max(4), 0);
    let [table, column, row, operation, payload @ ..] = padded.as_slice() else {
        unreachable!("padded to four bytes");
    };
    // Only tables with rows: tampering an empty table is a no-op.
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .and_then(|mut statement| statement.query_map([], |row| row.get(0))?.collect::<Result<Vec<String>, _>>())
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|table| {
            connection
                .query_row(&format!("SELECT EXISTS(SELECT 1 FROM \"{table}\")"), [], |row| row.get::<_, bool>(0))
                .unwrap_or(false)
        })
        .collect();
    let table = &tables[usize::from(*table) % tables.len()];
    let columns: Vec<String> = connection
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .and_then(|mut statement| statement.query_map([], |row| row.get(0))?.collect())
        .map_err(|error| error.to_string())?;
    let column = &columns[usize::from(*column) % columns.len()];
    let rowids: Vec<i64> = connection
        .prepare(&format!("SELECT rowid FROM \"{table}\" ORDER BY rowid"))
        .and_then(|mut statement| statement.query_map([], |row| row.get(0))?.collect())
        .unwrap_or_default();
    let Some(rowid) = rowids.get(usize::from(*row) % rowids.len().max(1)).copied() else {
        return Err(format!("empty table {table}"));
    };
    let current: Sql = connection
        .query_row(
            &format!("SELECT \"{column}\" FROM \"{table}\" WHERE rowid=?1"),
            [rowid],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let replacement = match operation % 8 {
        0 => Sql::Null,
        1 => Sql::Blob(payload.to_vec()),
        2 => Sql::Text(String::from_utf8_lossy(payload).into_owned()),
        3 => Sql::Integer(i64::from_le_bytes({
            let mut bytes = [0; 8];
            for (slot, byte) in bytes.iter_mut().zip(payload) {
                *slot = *byte;
            }
            bytes
        })),
        4 | 5 => match current {
            // Flip one byte of the existing value: the subtle case a digest
            // or hash chain must catch.
            Sql::Text(text) => {
                let mut bytes = text.into_bytes();
                if !bytes.is_empty() {
                    let at = usize::from(payload.first().copied().unwrap_or(0)) * 31 % bytes.len();
                    bytes[at] ^= payload.get(1).copied().unwrap_or(1).max(1);
                }
                Sql::Text(String::from_utf8_lossy(&bytes).into_owned())
            }
            Sql::Blob(mut bytes) => {
                if !bytes.is_empty() {
                    let at = usize::from(payload.first().copied().unwrap_or(0)) * 31 % bytes.len();
                    bytes[at] ^= payload.get(1).copied().unwrap_or(1).max(1);
                }
                Sql::Blob(bytes)
            }
            Sql::Integer(value) => {
                Sql::Integer(value ^ i64::from(payload.first().copied().unwrap_or(1).max(1)))
            }
            Sql::Real(value) => Sql::Real(-value),
            Sql::Null => Sql::Integer(0),
        },
        6 => {
            connection
                .execute(&format!("DELETE FROM \"{table}\" WHERE rowid=?1"), [rowid])
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        _ => Sql::Text(format!("{}{}", "x", String::from_utf8_lossy(payload))),
    };
    connection
        .execute(
            &format!("UPDATE \"{table}\" SET \"{column}\"=?1 WHERE rowid=?2"),
            rusqlite::params![replacement, rowid],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[test]
fn adversarial_tampered_rows_fail_validation() {
    let scratch = tempfile::tempdir().unwrap();
    let (template, expected) = template(scratch.path());
    let cases = tempfile::tempdir().unwrap();
    let counter = std::cell::Cell::new(0_u64);
    // Seeds cover each operation against several tables, columns and rows.
    let seeds: Vec<Vec<u8>> = (0..48_u8)
        .map(|index| {
            vec![
                index,
                index / 3,
                index / 2,
                index % 8,
                index,
                0x5a,
                b'{',
                b'}',
            ]
        })
        .collect();
    let report = fuzz(
        Target::bytes("store-row-tamper")
            .iterations(150)
            .max_input_bytes(4096)
            .max_case_time(Duration::from_secs(20))
            .minimize_budget(100),
        seeds,
        |input| {
            counter.set(counter.get() + 1);
            let package = cases.path().join(format!("row-{}.deadpan", counter.get()));
            copy_package(&template, &package);
            let tampered = {
                let connection = Connection::open(package.join("project.sqlite")).unwrap();
                // Foreign keys and triggers are part of the integrity under
                // test; disable only foreign-key enforcement for the tamper.
                let _ = connection.execute_batch("PRAGMA foreign_keys=OFF;");
                tamper(&connection, input)
            };
            let outcome = match tampered {
                // A refused tamper (constraint, trigger) is itself a defence.
                Err(error) => Ok(Verdict::Rejected(format!(
                    "tamper refused: {}",
                    error.chars().take(60).collect::<String>()
                ))),
                Ok(()) => judge(&package, &expected),
            };
            let _ = std::fs::remove_dir_all(&package);
            outcome
        },
    );
    report.assert_clean();
}

/// Minimized 2026-10-05 campaign findings: register slot rows and the bank
/// version were outside tamper detection. The bank digest now binds them, so
/// each tamper must be refused on open and by full validation.
#[test]
fn adversarial_register_slot_tampers_are_detected() {
    let scratch = tempfile::tempdir().unwrap();
    let (template, _) = template(scratch.path());
    for (index, script) in [
        [0x03_u8, 0x01, 0x03, 0x7b, 0x7d].as_slice(), // register_state.version := 125
        &[0xa4, 0x87, 0x5d, 0xfe],                    // delete registers row 'a'
        &[0x2c, 0x5a, 0xdf, 0xff],                    // rename registers 'a' to 'x'
    ]
    .into_iter()
    .enumerate()
    {
        let package = scratch.path().join(format!("pinned-{index}.deadpan"));
        copy_package(&template, &package);
        let connection = Connection::open(package.join("project.sqlite")).unwrap();
        let _ = connection.execute_batch("PRAGMA foreign_keys=OFF;");
        tamper(&connection, script).unwrap();
        drop(connection);
        let error = match ProjectStore::open(&package, AccessMode::ReadOnly) {
            Ok(store) => store.validate_full().expect_err("tamper validated"),
            Err(error) => error,
        };
        assert!(
            matches!(error, deadpan_store::StoreError::Registers(_)),
            "{index}: {error}"
        );
    }
}
