//! SQLite is authoritative. JSON dumps are read-only inspection artifacts.
//!
//! Each committed revision is immutable. Edits and the durable undo/redo cursor
//! change atomically. Undo restores content under a fresh revision, so an old
//! optimistic request never becomes valid again after undo.

mod error;
pub mod generation;
mod history;
mod migration;
mod schema;
mod validation;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use deadpan_core::{CommandRequest, EditTransaction, ProjectDocument, RevisionId};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;

pub use error::StoreError;
pub use migration::MigrationOutcome;

/// SQLite package format, versioned separately from authored document JSON.
pub const DATABASE_SCHEMA_VERSION: u32 = schema::VERSION;

pub fn sqlite_version() -> &'static str {
    rusqlite::version()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

pub struct ProjectStore {
    connection: Connection,
    package: PathBuf,
    mode: AccessMode,
    // Explicitly unlocked on drop so a briefly inherited descriptor in a spawned
    // child cannot extend this writer's ownership beyond the store lifetime.
    _writer_lock: Option<File>,
}

impl Drop for ProjectStore {
    fn drop(&mut self) {
        if let Some(lock) = &self._writer_lock {
            // File::drop still closes the handle if explicit unlock fails.
            let _ = lock.unlock();
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CommitOutcome {
    pub revision_id: RevisionId,
    pub edit: EditTransaction,
}

impl ProjectStore {
    /// Creates a new package exclusively. An existing path is never overwritten.
    /// If setup fails, an incomplete new package may remain for inspection.
    pub fn create(path: &Path, document: &ProjectDocument) -> Result<Self, StoreError> {
        validate_extension(path)?;
        document.validate()?;
        let json = document.to_json()?;
        check_document_size(&json)?;
        fs::create_dir(path)?;
        let package = fs::canonicalize(path)?;
        let lock = acquire_lock(&package)?;
        for directory in [
            "Media/Originals",
            "Media/Generated",
            "Media/UserAssets",
            "Analysis/Manual",
            "Snapshots",
            "Reports",
        ] {
            fs::create_dir_all(package.join(directory))?;
        }
        let mut connection = Connection::open(package.join("project.sqlite"))?;
        schema::configure(&connection)?;
        schema::create(&mut connection)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO revisions(id,parent_id,kind,document) VALUES (?1,NULL,'initial',?2)",
            params![document.revision_id().as_str(), json],
        )?;
        transaction.execute(
            "INSERT INTO state(singleton,head_revision,cursor) VALUES (1,?1,NULL)",
            [document.revision_id().as_str()],
        )?;
        transaction.commit()?;
        #[derive(Serialize)]
        struct Manifest<'a> {
            format: &'static str,
            project_id: &'a str,
        }
        let manifest = Manifest {
            format: "deadpan",
            project_id: document.project_id().as_str(),
        };
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(package.join("manifest.json"))?;
        serde_json::to_writer_pretty(&mut file, &manifest)?;
        writeln!(file)?;
        file.sync_all()?;
        File::open(&package)?.sync_all()?;
        Ok(Self {
            connection,
            package,
            mode: AccessMode::ReadWrite,
            _writer_lock: Some(lock),
        })
    }

    pub fn open(path: &Path, mode: AccessMode) -> Result<Self, StoreError> {
        validate_extension(path)?;
        let package = fs::canonicalize(path)?;
        let database = package.join("project.sqlite");
        require_regular_file(&database)?;
        // Probe the format read-only before acquiring writable state or enabling WAL.
        let probe = Connection::open_with_flags(&database, read_flags())?;
        schema::configure(&probe)?;
        schema::check_version(&probe)?;
        drop(probe);
        let lock = if mode == AccessMode::ReadWrite {
            Some(acquire_lock(&package)?)
        } else {
            None
        };
        let flags = if mode == AccessMode::ReadWrite {
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW
        } else {
            read_flags()
        };
        let connection = Connection::open_with_flags(&database, flags)?;
        schema::configure(&connection)?;
        schema::check_version(&connection)?;
        if mode == AccessMode::ReadWrite {
            connection.pragma_update(None, "journal_mode", "WAL")?;
            connection.pragma_update(None, "synchronous", "FULL")?;
        } else {
            connection.pragma_update(None, "query_only", true)?;
        }
        let store = Self {
            connection,
            package,
            mode,
            _writer_lock: lock,
        };
        store.validate()?;
        Ok(store)
    }

    pub fn snapshot(&self) -> Result<ProjectDocument, StoreError> {
        read_snapshot(&self.connection)
    }

    pub fn validate(&self) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        validation::check_stored_sizes(&transaction, schema::MAX_DOCUMENT_BYTES)?;
        generation::check_stored_sizes(&transaction)?;
        let integrity: String =
            transaction.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(StoreError::Integrity(integrity));
        }
        let foreign_keys: i64 =
            transaction.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if foreign_keys != 0 {
            return Err(StoreError::Integrity("foreign-key violation".into()));
        }
        validation::validate_history(&transaction)?;
        generation::validate_store(&transaction)
    }

    pub fn preview(&self, request: &CommandRequest) -> Result<EditTransaction, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        Ok(prepare_command(&transaction, request)?.edit)
    }

    pub fn commit(&mut self, request: &CommandRequest) -> Result<CommitOutcome, StoreError> {
        self.commit_inner(request, None)
    }

    pub fn commit_reconciled(
        &mut self,
        request: &CommandRequest,
        relevance: &generation::RelevancePlan,
    ) -> Result<CommitOutcome, StoreError> {
        self.commit_inner(request, Some(relevance))
    }

    fn commit_inner(
        &mut self,
        request: &CommandRequest,
        relevance: Option<&generation::RelevancePlan>,
    ) -> Result<CommitOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let plan = prepare_command(&transaction, request)?;
        match relevance {
            Some(relevance) => generation::apply_relevance_plan(
                &transaction,
                &plan.current,
                &plan.next,
                relevance,
            )?,
            None => generation::ensure_no_current(&transaction)?,
        }
        insert_revision(&transaction, &plan.current, &plan.next, "edit")?;
        let cursor: Option<i64> =
            transaction.query_row("SELECT cursor FROM state WHERE singleton=1", [], |row| {
                row.get(0)
            })?;
        transaction.execute(
            "INSERT INTO history(parent_id,revision_id,request,edit) VALUES (?1,?2,?3,?4)",
            params![
                cursor,
                plan.next.revision_id().as_str(),
                plan.request_json,
                plan.edit_json
            ],
        )?;
        let history_id = transaction.last_insert_rowid();
        transaction.execute(
            "UPDATE state SET head_revision=?1,cursor=?2 WHERE singleton=1",
            params![plan.next.revision_id().as_str(), history_id],
        )?;
        transaction.execute("DELETE FROM redo", [])?;
        transaction.commit()?;
        Ok(CommitOutcome {
            revision_id: plan.next.revision_id().clone(),
            edit: plan.edit,
        })
    }

    /// Produces a consistent SQLite snapshot including all committed WAL pages.
    /// Original/generated media stays in the project package; this is a database
    /// recovery checkpoint, not a portable copy of the whole project.
    pub fn checkpoint(&self) -> Result<PathBuf, StoreError> {
        self.require_writer()?;
        let directory = self.package.join("Snapshots");
        if !fs::symlink_metadata(&directory)?.file_type().is_dir() {
            return Err(StoreError::UnsafePath(directory));
        }
        let temporary = tempfile::NamedTempFile::new_in(&directory)?;
        self.connection
            .backup(rusqlite::MAIN_DB, temporary.path(), None)?;
        temporary.as_file().sync_all()?;
        let (_, path) = temporary.keep().map_err(|error| error.error)?;
        File::open(&directory)?.sync_all()?;
        Ok(path)
    }

    fn require_writer(&self) -> Result<(), StoreError> {
        if self.mode == AccessMode::ReadOnly {
            return Err(StoreError::ReadOnly);
        }
        Ok(())
    }
}

fn validate_extension(path: &Path) -> Result<(), StoreError> {
    if path
        .extension()
        .is_none_or(|extension| extension != "deadpan")
    {
        return Err(StoreError::PackageExtension);
    }
    Ok(())
}

fn require_regular_file(path: &Path) -> Result<(), StoreError> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(StoreError::UnsafePath(path.into()));
    }
    Ok(())
}

fn read_flags() -> OpenFlags {
    OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_NOFOLLOW
}

fn acquire_lock(package: &Path) -> Result<File, StoreError> {
    let path = package.join(".writer.lock");
    if path.symlink_metadata().is_ok() {
        require_regular_file(&path)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err(StoreError::AlreadyOpen),
        Err(std::fs::TryLockError::Error(error)) => Err(StoreError::Io(error)),
    }
}

fn read_snapshot(connection: &Connection) -> Result<ProjectDocument, StoreError> {
    let head = validation::read_head(connection)?;
    Ok(validation::read_revision(connection, &head)?.document)
}

fn check_document_size(json: &str) -> Result<(), StoreError> {
    if json.len() > schema::MAX_DOCUMENT_BYTES {
        return Err(StoreError::Integrity(
            "document exceeds the core document size limit".into(),
        ));
    }
    Ok(())
}

struct CommandPlan {
    current: ProjectDocument,
    next: ProjectDocument,
    edit: EditTransaction,
    request_json: String,
    edit_json: String,
}

fn prepare_command(
    connection: &Connection,
    request: &CommandRequest,
) -> Result<CommandPlan, StoreError> {
    let current = read_snapshot(connection)?;
    let edit = deadpan_core::apply(&current, request)?;
    ensure_unused_revision(connection, &request.new_revision)?;
    let next = edit.forward.apply(&current)?;
    check_document_size(&next.to_json()?)?;
    let request_json = serde_json::to_string(request)?;
    let edit_json = serde_json::to_string(&edit)?;
    check_document_size(&request_json)?;
    check_document_size(&edit_json)?;
    Ok(CommandPlan {
        current,
        next,
        edit,
        request_json,
        edit_json,
    })
}

fn ensure_unused_revision(
    connection: &Connection,
    revision: &RevisionId,
) -> Result<(), StoreError> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM revisions WHERE id=?1",
            [revision.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_some() {
        return Err(StoreError::RevisionReused(revision.as_str().to_owned()));
    }
    // A package may start from a nonempty imported snapshot. Its occurrence
    // allocations predate this database's revision rows, but still reserve those
    // names forever. Subsequent Insert/Wrap/Grow allocations use committed IDs.
    let initial =
        validation::read_revision(connection, &validation::read_initial_id(connection)?)?.document;
    if initial.nodes().values().any(|node| {
        matches!(&node.kind,
        deadpan_core::NodeKind::Repeat { iterations, .. }
        if iterations.segments().any(|(allocation,_,_)| allocation == revision))
    }) {
        return Err(StoreError::RevisionReused(revision.as_str().to_owned()));
    }
    Ok(())
}

fn insert_revision(
    connection: &Connection,
    before: &ProjectDocument,
    after: &ProjectDocument,
    kind: &str,
) -> Result<(), StoreError> {
    ensure_unused_revision(connection, after.revision_id())?;
    let json = after.to_json()?;
    check_document_size(&json)?;
    connection.execute(
        "INSERT INTO revisions(id,parent_id,kind,document) VALUES (?1,?2,?3,?4)",
        params![
            after.revision_id().as_str(),
            before.revision_id().as_str(),
            kind,
            json
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectId};

    #[test]
    fn duplicated_handle_does_not_extend_writer_ownership() -> Result<(), Box<dyn std::error::Error>>
    {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("lock.deadpan");
        let document = ProjectDocument::new(
            ProjectId::new("p")?,
            RevisionId::new("r")?,
            PresentationBasis {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(30, 1)?,
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root")?,
        )?;
        let store = ProjectStore::create(&path, &document)?;
        let inherited = store
            ._writer_lock
            .as_ref()
            .expect("writer owns lock")
            .try_clone()?;
        drop(store);
        let next = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(next.snapshot()?, document);
        drop(inherited);
        Ok(())
    }

    #[test]
    fn sqlite_full_error_keeps_last_committed_revision() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = tempfile::tempdir()?;
        let document = ProjectDocument::new(
            ProjectId::new("p")?,
            RevisionId::new("r0")?,
            PresentationBasis {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(30, 1)?,
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root")?,
        )?;
        let path = scratch.path().join("capacity.deadpan");
        let mut store = ProjectStore::create(&path, &document)?;
        let pages: i64 = store
            .connection
            .pragma_query_value(None, "page_count", |row| row.get(0))?;
        store
            .connection
            .pragma_update(None, "max_page_count", pages)?;
        let mut observed_full = false;
        for index in 1..100 {
            let before = store.snapshot()?;
            let request = CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: RevisionId::new(format!("r{index}"))?,
                command: deadpan_core::Command::Rename {
                    node: before.root().clone(),
                    label: format!("{index}{}", "x".repeat(1000)),
                },
            };
            if let Err(error) = store.commit(&request) {
                assert_eq!(error.code(), "DiskFull", "{error}");
                assert_eq!(store.snapshot()?, before);
                store.validate()?;
                observed_full = true;
                break;
            }
        }
        assert!(
            observed_full,
            "the real SQLite page limit must reject a write"
        );
        Ok(())
    }
}
