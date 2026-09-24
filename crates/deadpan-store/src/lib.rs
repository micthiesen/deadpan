//! SQLite is authoritative. JSON dumps are read-only inspection artifacts.
//!
//! Each committed revision is immutable. Edits and the durable undo/redo cursor
//! change atomically. Undo restores content under a fresh revision, so an old
//! optimistic request never becomes valid again after undo.

mod error;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod generated_media;
pub mod generation;
pub mod generation_acceptance;
pub mod generation_attempts;
mod history;
mod migration;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod object_storage;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod original_media;
mod schema;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod single_source;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod source_registration;
mod validation;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    generated_storage: generated_media::GeneratedStorage,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    original_storage: Arc<object_storage::ObjectStorage>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    import_closed: Arc<AtomicBool>,
    // Explicitly unlocked on drop so a briefly inherited descriptor in a spawned
    // child cannot extend this writer's ownership beyond the store lifetime.
    _writer_lock: Option<File>,
}

impl Drop for ProjectStore {
    fn drop(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.import_closed.store(true, Ordering::Release);
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
        Self::create_inner(path, document, false)
    }

    fn create_inner(
        path: &Path,
        document: &ProjectDocument,
        single_source: bool,
    ) -> Result<Self, StoreError> {
        validate_extension(path)?;
        document.validate()?;
        let json = document.to_json()?;
        check_document_size(&json)?;
        ensure_generated_admission(None, document)?;
        ensure_source_admission(None, document, None)?;
        fs::create_dir(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                StoreError::PackageAlreadyExists(path.into())
            } else {
                StoreError::Io(error)
            }
        })?;
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if single_source {
            single_source::create_profile(&transaction, document)?;
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let _ = single_source;
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let generated_storage = generated_media::GeneratedStorage::open(&package)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let original_storage = object_storage::ObjectStorage::open(
            &package,
            object_storage::StorageNamespace::Originals,
        )
        .map_err(original_media::OriginalMediaError::from)?;
        Ok(Self {
            connection,
            package,
            mode: AccessMode::ReadWrite,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            generated_storage,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            original_storage: Arc::new(original_storage),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            import_closed: Arc::new(AtomicBool::new(false)),
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let generated_storage = generated_media::GeneratedStorage::open(&package)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let original_storage = object_storage::ObjectStorage::open(
            &package,
            object_storage::StorageNamespace::Originals,
        )
        .map_err(original_media::OriginalMediaError::from)?;
        let mut store = Self {
            connection,
            package,
            mode,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            generated_storage,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            original_storage: Arc::new(original_storage),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            import_closed: Arc::new(AtomicBool::new(false)),
            _writer_lock: lock,
        };
        store.validate()?;
        if mode == AccessMode::ReadWrite {
            generation_attempts::recover_nonterminal(&mut store.connection)?;
        }
        Ok(store)
    }

    pub fn snapshot(&self) -> Result<ProjectDocument, StoreError> {
        read_snapshot(&self.connection)
    }

    /// Read one immutable committed revision, including an abandoned branch.
    /// This does not move the history cursor or perform writer recovery.
    pub fn snapshot_at(&self, revision: &RevisionId) -> Result<ProjectDocument, StoreError> {
        Ok(validation::read_revision(&self.connection, revision.as_str())?.document)
    }

    pub fn validate(&self) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        validation::check_stored_sizes(&transaction, schema::MAX_DOCUMENT_BYTES)?;
        generation::check_stored_sizes(&transaction)?;
        generation_attempts::check_stored_sizes(&transaction)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        original_media::check_stored_sizes(&transaction)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        source_registration::check_stored_sizes(&transaction)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        single_source::check_stored_sizes(&transaction)?;
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
        generation::validate_store(&transaction)?;
        generation_attempts::validate_store(&transaction)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        original_media::validate_store(&transaction)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        source_registration::validate_store(&transaction)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        single_source::validate_store(&transaction)?;
        Ok(())
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
        let outcome = write_command_plan(&transaction, plan, relevance)?;
        transaction.commit()?;
        Ok(outcome)
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

    /// Publishes verified bytes in the project's non-cache generated-media area.
    /// This does not validate a codec, register an asset, or accept a candidate.
    /// Call on a host I/O worker, before committing any authored reference.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub fn promote_generated_object(
        &mut self,
        reader: &mut impl std::io::Read,
        expected: &generated_media::GeneratedObjectRef,
        limits: generated_media::GeneratedMediaLimits,
    ) -> Result<generated_media::GeneratedObjectRef, StoreError> {
        self.require_writer()?;
        Ok(self.generated_storage.promote(reader, expected, limits)?)
    }

    /// Verifies stored bytes into an immutable read/seek snapshot. Available to
    /// read-only consumers; the worker workspace and generation runtime are not
    /// consulted. This is a bounded I/O operation, not a realtime callback API.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub fn snapshot_generated_object(
        &self,
        expected: &generated_media::GeneratedObjectRef,
        limits: generated_media::GeneratedMediaLimits,
    ) -> Result<generated_media::VerifiedGeneratedObject, StoreError> {
        Ok(self.generated_storage.snapshot(expected, limits)?)
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
    prepare_admitted_command(connection, request, None)
}

fn prepare_admitted_command(
    connection: &Connection,
    request: &CommandRequest,
    admitted: Option<&deadpan_core::GeneratedArtifact>,
) -> Result<CommandPlan, StoreError> {
    prepare_command_with_admission(connection, request, admitted, None, None)
}

fn prepare_command_with_admission(
    connection: &Connection,
    request: &CommandRequest,
    generated: Option<&deadpan_core::GeneratedArtifact>,
    source: Option<(&deadpan_core::AssetId, &deadpan_core::AssetRecord)>,
    geometry: Option<(u32, u32)>,
) -> Result<CommandPlan, StoreError> {
    let current = read_snapshot(connection)?;
    let edit = deadpan_core::apply(&current, request)?;
    ensure_unused_revision(connection, &request.new_revision)?;
    let next = edit.forward.apply(&current)?;
    check_document_size(&next.to_json()?)?;
    ensure_generated_admission_with(Some(&current), &next, generated)?;
    ensure_source_admission(Some(&current), &next, source)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    single_source::check_transition(connection, &current, &next, source.is_some())?;
    match &request.command {
        deadpan_core::Command::ImportSource {
            primary: Some(_), ..
        } if source.is_none() => {
            return Err(StoreError::SourceBasisAdmissionUnavailable);
        }
        deadpan_core::Command::AdoptPrimaryGeometry { width, height }
            if geometry != Some((*width, *height)) =>
        {
            return Err(StoreError::SourceBasisAdmissionUnavailable);
        }
        _ => {}
    }
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

/// A qualified source binding may enter history only through the host's
/// decoded-source registration. Retaining existing immutable records is safe;
/// undo/redo restores already-admitted records from durable history.
fn ensure_source_admission(
    current: Option<&ProjectDocument>,
    next: &ProjectDocument,
    admitted: Option<(&deadpan_core::AssetId, &deadpan_core::AssetRecord)>,
) -> Result<(), StoreError> {
    for (id, asset) in next.assets() {
        if asset.source_qualification.is_none()
            || current.is_some_and(|document| document.assets().get(id) == Some(asset))
            || admitted.is_some_and(|(allowed_id, allowed)| allowed_id == id && allowed == asset)
        {
            continue;
        }
        return Err(StoreError::SourceAdmissionUnavailable);
    }
    Ok(())
}

fn write_command_plan(
    connection: &Connection,
    plan: CommandPlan,
    relevance: Option<&generation::RelevancePlan>,
) -> Result<CommitOutcome, StoreError> {
    match relevance {
        Some(relevance) => {
            generation::apply_relevance_plan(connection, &plan.current, &plan.next, relevance)?
        }
        None => generation::ensure_no_current(connection)?,
    }
    insert_revision(connection, &plan.current, &plan.next, "edit")?;
    let cursor: Option<i64> =
        connection.query_row("SELECT cursor FROM state WHERE singleton=1", [], |row| {
            row.get(0)
        })?;
    connection.execute(
        "INSERT INTO history(parent_id,revision_id,request,edit) VALUES (?1,?2,?3,?4)",
        params![
            cursor,
            plan.next.revision_id().as_str(),
            plan.request_json,
            plan.edit_json
        ],
    )?;
    let history_id = connection.last_insert_rowid();
    connection.execute(
        "UPDATE state SET head_revision=?1,cursor=?2 WHERE singleton=1",
        params![plan.next.revision_id().as_str(), history_id],
    )?;
    connection.execute("DELETE FROM redo", [])?;
    Ok(CommitOutcome {
        revision_id: plan.next.revision_id().clone(),
        edit: plan.edit,
    })
}

/// Core edits describe authored intent. They cannot prove that a candidate was
/// selected, revalidated, canonicalized and durably promoted. Generic store
/// commands may retain/copy an existing artifact,
/// but cannot introduce a new one, including through subtrees or Repeat gaps.
fn ensure_generated_admission(
    current: Option<&ProjectDocument>,
    next: &ProjectDocument,
) -> Result<(), StoreError> {
    ensure_generated_admission_with(current, next, None)
}

fn ensure_generated_admission_with(
    current: Option<&ProjectDocument>,
    next: &ProjectDocument,
    admitted: Option<&deadpan_core::GeneratedArtifact>,
) -> Result<(), StoreError> {
    use deadpan_core::{HoldVideo, NodeKind};
    use std::collections::BTreeSet;

    fn artifacts(document: &ProjectDocument) -> Result<BTreeSet<Vec<u8>>, StoreError> {
        document
            .nodes()
            .values()
            .filter_map(|node| match &node.kind {
                NodeKind::Hold { recipe } => Some(&recipe.video),
                NodeKind::Repeat {
                    gap: Some(recipe), ..
                } => Some(&recipe.video),
                _ => None,
            })
            .filter_map(|video| match video {
                HoldVideo::Generated { accepted } => Some(&accepted.artifact),
                _ => None,
            })
            .map(|artifact| serde_json::to_vec(artifact).map_err(StoreError::from))
            .collect()
    }
    let mut retained = match current {
        Some(document) => artifacts(document)?,
        None => BTreeSet::new(),
    };
    if let Some(artifact) = admitted {
        retained.insert(serde_json::to_vec(artifact)?);
    }
    if artifacts(next)?.is_subset(&retained) {
        Ok(())
    } else {
        Err(StoreError::GeneratedAcceptanceUnavailable)
    }
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
    // and audio-lineage/timing allocations predate this database's revision
    // rows. Retained timing layouts also own old play namespaces. Reserve all
    // of those names forever, even after every live owner is removed.
    // Subsequent command allocations use committed revision IDs.
    let initial =
        validation::read_revision(connection, &validation::read_initial_id(connection)?)?.document;
    if initial.nodes().values().any(|node| {
        matches!(&node.kind,
        deadpan_core::NodeKind::Repeat { iterations, .. }
        if iterations.segments().any(|(allocation,_,_)| allocation == revision))
    }) || initial
        .audio_lineage()
        .values()
        .any(|lineage| &lineage.allocation == revision)
        || initial.audio_bindings().allocation_ids().contains(revision)
    {
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
