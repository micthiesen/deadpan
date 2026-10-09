//! Stable inspection without keeping another writer's WAL snapshot alive.

use std::time::{Duration, Instant};

use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};

use crate::{StoreError, schema};

pub(super) fn capture(source: Connection) -> Result<(Connection, tempfile::TempDir), StoreError> {
    copy(
        source,
        crate::backups::BackupLimits::default().max_database_bytes,
        Instant::now() + Duration::from_secs(60),
    )
}

fn copy(
    source: Connection,
    max_bytes: u64,
    deadline: Instant,
) -> Result<(Connection, tempfile::TempDir), StoreError> {
    source.pragma_update(None, "query_only", true)?;
    source.execute_batch("BEGIN DEFERRED")?;
    // The first read pins one complete snapshot, including committed WAL data.
    let page_size: i64 = source.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let pages: i64 = source.pragma_query_value(None, "page_count", |row| row.get(0))?;
    if !u64::try_from(page_size)
        .ok()
        .zip(u64::try_from(pages).ok())
        .and_then(|(size, count)| size.checked_mul(count))
        .is_some_and(|bytes| bytes > 0 && bytes <= max_bytes)
    {
        return Err(StoreError::Storage(
            "The project database exceeds the read-only inspection copy limit".into(),
        ));
    }
    let files = tempfile::Builder::new()
        .prefix("deadpan-inspection-")
        .tempdir()?;
    // macOS's default temporary root may contain the /var alias. Resolve
    // that trusted root once before SQLite's no-follow admission.
    let database = files.path().canonicalize()?.join("inspection.sqlite");
    let mut target = Connection::open_with_flags(
        &database,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    schema::configure(&target)?;
    target.busy_timeout(Duration::ZERO)?;
    {
        let backup = Backup::new(&source, &mut target)?;
        loop {
            if Instant::now() >= deadline {
                return Err(StoreError::Storage(
                    "The read-only inspection copy timed out; reopen the project to try again"
                        .into(),
                ));
            }
            match backup.step(256)? {
                StepResult::Done => break,
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                _ => {
                    return Err(StoreError::Storage(
                        "SQLite reported an unknown inspection copy result".into(),
                    ));
                }
            }
        }
    }
    // Backup copies the source header's WAL mode; make this private copy
    // standalone before reopening it with a genuinely read-only connection.
    target.pragma_update(None, "journal_mode", "DELETE")?;
    target
        .close()
        .map_err(|(_, error)| StoreError::Database(error))?;
    source.execute_batch("ROLLBACK")?;
    drop(source);
    let connection = Connection::open_with_flags(database, crate::read_flags())?;
    schema::configure(&connection)?;
    Ok((connection, files))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccessMode, ProjectStore};
    use deadpan_core::*;

    fn database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE test(value TEXT); INSERT INTO test VALUES ('kept');")
            .unwrap();
        connection
    }

    #[test]
    fn inspection_copy_enforces_size_and_deadline() {
        assert!(copy(database(), 1, Instant::now() + Duration::from_secs(1)).is_err());
        assert!(copy(database(), 1 << 20, Instant::now()).is_err());
    }

    #[test]
    fn inspection_keeps_one_revision_without_holding_the_writer_wal()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("inspection.deadpan");
        let document = ProjectDocument::new_automatic(
            ProjectId::new("project")?,
            RevisionId::new("initial")?,
            NodeId::new("root")?,
        )?;
        let mut writer = ProjectStore::create(&path, &document)?;
        let first = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("insert")?,
            command: Command::Insert {
                parent: document.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: NodeId::new("hold")?,
                    nodes: std::collections::BTreeMap::from([(
                        NodeId::new("hold")?,
                        BeatNode::hold(
                            "Pause",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(10)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        };
        writer.commit(&first)?;
        let retained = writer.snapshot()?;
        let session = std::fs::read(path.join(".writer.session"))?;
        let mut view = ProjectStore::open_inspection(&path)?;
        let private = view
            ._inspection
            .as_ref()
            .expect("private copy")
            .path()
            .to_owned();
        assert_eq!(view.access_mode(), AccessMode::ReadOnly);
        assert_eq!(view.snapshot()?, retained);
        let next = CommandRequest {
            expected_revision: first.new_revision.clone(),
            new_revision: RevisionId::new("longer")?,
            command: Command::SetHoldDuration {
                node: NodeId::new("hold")?,
                duration: FrameDuration::new(20)?,
            },
            ..first
        };
        assert!(matches!(view.commit(&next), Err(StoreError::ReadOnly)));
        writer.commit(&next)?;
        // A long-lived reader transaction would make this checkpoint busy.
        let checkpoint: (u32, u32, u32) =
            writer
                .connection
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
        assert_eq!(checkpoint, (0, 0, 0));
        assert_eq!(std::fs::read(path.join(".writer.session"))?, session);
        assert_eq!(view.snapshot()?, retained);
        view.validate_full()?;
        assert!(matches!(
            ProjectStore::open(&path, AccessMode::ReadWrite),
            Err(StoreError::AlreadyOpen)
        ));
        drop(writer);
        let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(reopened.snapshot()?.revision_id(), &next.new_revision);
        assert_eq!(view.snapshot()?, retained);
        drop(view);
        assert!(!private.exists());
        Ok(())
    }
}
