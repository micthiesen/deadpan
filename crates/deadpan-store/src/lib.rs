//! SQLite is authoritative. JSON dumps are read-only inspection artifacts.
//!
//! Each committed revision is immutable. Edits and the durable undo/redo cursor
//! change atomically. Undo restores content under a fresh revision, so an old
//! optimistic request never becomes valid again after undo.

mod audit;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod backups;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod checkpoint;
mod compound;
mod document_cache;
mod error;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod generated_media;
pub mod generation;
pub mod generation_acceptance;
pub mod generation_attempts;
pub mod generation_preparations;
pub mod generation_retention;
mod generation_scope;
mod history;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod host_owner;
pub mod migration;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod object_storage;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod original_media;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod original_provenance;
mod output_color;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod portable;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod publication;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod publication_durability;
pub mod recovery;
pub mod registers;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod render_jobs;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod render_media;
mod retired;
mod revision_storage;
mod schema;
mod shot_analysis;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod single_source;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod slice_preview;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod source_registration;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod storage;
pub use shot_analysis::{MAX_SHOT_ANALYSES, ShotAnalysisKey};
pub mod analysis_corrections;
pub use analysis_corrections::{
    CorrectionChange, CorrectionsKey, MAX_CORRECTION_UNDO, MAX_CORRECTIONS_JSON_BYTES,
    StoredCorrections, UnreadableCorrection,
};
mod speech_activity;
pub use speech_activity::{MAX_SPEECH_ACTIVITY, SpeechActivityKey};
mod transcripts;
pub use transcripts::{MAX_TRANSCRIPT_JSON_BYTES, MAX_TRANSCRIPTS, TranscriptKey};
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
use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;

pub use compound::{CompoundCommitOutcome, CompoundPreview};
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
    /// Validated documents of committed revisions, including the head.
    documents: document_cache::DocumentCache,
    /// How opening validated the history.
    opened: HistoryValidation,
    /// Host resolver for generation request relevance on ordinary writes.
    context_resolver: Option<Arc<dyn generation::GenerationContextResolver>>,
    package: PathBuf,
    mode: AccessMode,
    /// The newer database schema of a package opened read-only for viewing.
    newer_schema: Option<u32>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    generated_storage: Arc<generated_media::GeneratedStorage>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    generated_read_closed: Arc<AtomicBool>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    original_storage: Arc<object_storage::ObjectStorage>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    import_closed: Arc<AtomicBool>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    render_storage: Arc<object_storage::ObjectStorage>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    render_closed: Arc<AtomicBool>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    render_workflow_claimed: Arc<AtomicBool>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    publication_durability: Option<publication_durability::PublicationDurability>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    publication_epochs: std::collections::BTreeMap<String, Arc<std::sync::atomic::AtomicU64>>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    publication_barrier_failed: bool,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    writer_package: Option<host_owner::PackageAnchor>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    writer_owner: Option<host_owner::OwnerState>,
    /// What this writable open recovered; empty for read-only and new stores.
    recovery: recovery::OpenRecovery,
    /// This writer recorded `.writer.session` and removes it on close.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    session_marker: bool,
    // Explicitly unlocked on drop so a briefly inherited descriptor in a spawned
    // child cannot extend this writer's ownership beyond the store lifetime.
    _writer_lock: Option<File>,
}

impl Drop for ProjectStore {
    fn drop(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.import_closed.store(true, Ordering::Release);
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.generated_read_closed.store(true, Ordering::Release);
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.render_closed.store(true, Ordering::Release);
        // A clean close removes the marker before releasing the lock, so the
        // next writer can tell this session ended normally.
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if self.session_marker
            && let Some(anchor) = &self.writer_package
        {
            let _ = anchor.end_session();
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if let Some(owner) = &mut self.writer_owner {
            owner.close(
                self.writer_package.as_ref(),
                &self.package,
                self._writer_lock.as_ref(),
            );
        }
        if let Some(lock) = &self._writer_lock {
            // File::drop still closes the handle if explicit unlock fails.
            let _ = lock.unlock();
        }
    }
}

/// What a history validation proved and how.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct HistoryValidation {
    /// Revisions in the chronology, including the initial one.
    pub revisions: usize,
    /// Leading revisions proved by a receipt from this validator build, after
    /// hashing every stored row.
    pub verified_by_receipt: usize,
    /// Revisions whose command or navigation was recomputed.
    pub replayed: usize,
}

impl From<&validation::HistoryAudit> for HistoryValidation {
    fn from(audit: &validation::HistoryAudit) -> Self {
        let revisions = audit.order.len();
        Self {
            revisions,
            verified_by_receipt: audit.verified,
            replayed: revisions - audit.verified.max(1),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CommitOutcome {
    pub revision_id: RevisionId,
    pub edit: EditTransaction,
    /// Prepared inside the commit transaction; no fallible read follows saving.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub register_bank: Option<registers::RegisterBank>,
    /// Replacement inputs queued atomically with this exact duration edit.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub generation_preparations: Vec<generation_preparations::PreparationId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub generation_preparation_notices: Vec<generation_preparations::PreparationNotice>,
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
        let json = document.to_compact_json()?;
        check_document_size(&json)?;
        ensure_generated_admission(None, document)?;
        ensure_source_admission(None, document, None, None)?;
        fs::create_dir(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                StoreError::PackageAlreadyExists(path.into())
            } else {
                StoreError::Io(error)
            }
        })?;
        let package = fs::canonicalize(path)?;
        let lock = acquire_lock(&package)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let writer_package = Some(host_owner::PackageAnchor::open(&package, &lock)?);
        for directory in [
            "Media/Originals",
            "Media/Generated",
            "Media/RenderCandidates",
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
        revision_storage::insert(
            &transaction,
            None,
            document,
            "initial",
            revision_storage::StoredPatch::Initial,
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
        let rows = audit::read_rows(&transaction, document.revision_id().as_str())?;
        audit::certify(
            &transaction,
            1,
            document.revision_id().as_str(),
            audit::link(&audit::genesis(), &rows),
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let generated_storage = generated_media::GeneratedStorage::open(&package)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let original_storage = object_storage::ObjectStorage::open(
            &package,
            object_storage::StorageNamespace::Originals,
        )
        .map_err(original_media::OriginalMediaError::from)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let render_storage = object_storage::ObjectStorage::open(
            &package,
            object_storage::StorageNamespace::RenderCandidates,
        )
        .map_err(render_media::RenderMediaError::from)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let publication_durability = Some(publication_durability::PublicationDurability::open(
            &package,
        )?);
        let documents = document_cache::DocumentCache::default();
        documents.insert(deadpan_core::ValidatedDocument::new(Arc::new(
            document.clone(),
        ))?);
        Ok(Self {
            connection,
            documents,
            opened: HistoryValidation {
                revisions: 1,
                verified_by_receipt: 1,
                replayed: 0,
            },
            context_resolver: None,
            package,
            mode: AccessMode::ReadWrite,
            newer_schema: None,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            generated_storage: Arc::new(generated_storage),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            generated_read_closed: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            original_storage: Arc::new(original_storage),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            import_closed: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            render_storage: Arc::new(render_storage),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            render_closed: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            render_workflow_claimed: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            publication_durability,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            publication_epochs: std::collections::BTreeMap::new(),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            publication_barrier_failed: false,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            writer_package,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            writer_owner: None,
            recovery: recovery::OpenRecovery::default(),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            session_marker: false,
            _writer_lock: Some(lock),
        })
        .map(|mut store| {
            store.begin_writer_session();
            store
        })
    }

    pub fn open(path: &Path, mode: AccessMode) -> Result<Self, StoreError> {
        validate_extension(path)?;
        let package = fs::canonicalize(path)?;
        // A WAL database needs writable shared memory even to read, so a
        // read-only volume refuses both modes. Say so instead of reporting
        // SQLite's generic open failure.
        if read_only_filesystem(&package) {
            if mode == AccessMode::ReadWrite {
                return Err(StoreError::ReadOnlyLocation(package));
            }
            return Self::open_package(package.clone(), mode).map_err(|error| match error {
                StoreError::Database(rusqlite::Error::SqliteFailure(failure, _))
                    if failure.code == rusqlite::ErrorCode::CannotOpen =>
                {
                    StoreError::ReadOnlyLocation(package)
                }
                error => error,
            });
        }
        Self::open_package(package, mode)
    }

    fn open_package(package: PathBuf, mode: AccessMode) -> Result<Self, StoreError> {
        let database = package.join("project.sqlite");
        require_regular_file(&database)?;
        // Probe the format read-only before acquiring writable state or enabling WAL.
        let probe = Connection::open_with_flags(&database, read_flags())?;
        schema::configure(&probe)?;
        let mut recovered_lock = None;
        let newer_schema = match readable_version(&probe, mode) {
            // A write in rollback-journal mode was interrupted (for example a
            // release migration leaving WAL mode). Only a writer may roll the
            // hot journal back; do it under the package lock, then probe again.
            Err(error) if interrupted_rollback(&error) => {
                drop(probe);
                if mode == AccessMode::ReadOnly {
                    return Err(StoreError::Storage(
                        "an interrupted write left a recovery journal; open the project for editing once to recover it (nothing has been changed)".into(),
                    ));
                }
                let lock = acquire_lock(&package)?;
                let recovery = Connection::open_with_flags(
                    &database,
                    OpenFlags::SQLITE_OPEN_READ_WRITE
                        | OpenFlags::SQLITE_OPEN_NO_MUTEX
                        | OpenFlags::SQLITE_OPEN_NOFOLLOW,
                )?;
                schema::configure(&recovery)?;
                // The first read rolls the hot journal back.
                recovery.query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0))?;
                recovery
                    .close()
                    .map_err(|(_, error)| StoreError::Database(error))?;
                recovered_lock = Some(lock);
                let probe = Connection::open_with_flags(&database, read_flags())?;
                schema::configure(&probe)?;
                readable_version(&probe, mode)?
            }
            result => {
                drop(probe);
                result?
            }
        };
        let lock = if mode == AccessMode::ReadWrite {
            Some(match recovered_lock {
                Some(lock) => lock,
                None => acquire_lock(&package)?,
            })
        } else {
            None
        };
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let writer_package = lock
            .as_ref()
            .map(|lock| host_owner::PackageAnchor::open(&package, lock))
            .transpose()?;
        let flags = if mode == AccessMode::ReadWrite {
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW
        } else {
            read_flags()
        };
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let publication_durability = if mode == AccessMode::ReadWrite {
            Some(publication_durability::PublicationDurability::open(
                &package,
            )?)
        } else {
            None
        };
        let connection = Connection::open_with_flags(&database, flags)?;
        schema::configure(&connection)?;
        if readable_version(&connection, mode)? != newer_schema {
            return Err(StoreError::Integrity(
                "database schema changed while opening".into(),
            ));
        }
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let render_storage = object_storage::ObjectStorage::open(
            &package,
            object_storage::StorageNamespace::RenderCandidates,
        )
        .map_err(render_media::RenderMediaError::from)?;
        let mut store = Self {
            connection,
            documents: document_cache::DocumentCache::default(),
            opened: HistoryValidation::default(),
            context_resolver: None,
            package,
            mode,
            newer_schema,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            generated_storage: Arc::new(generated_storage),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            generated_read_closed: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            original_storage: Arc::new(original_storage),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            import_closed: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            render_storage: Arc::new(render_storage),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            render_closed: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            render_workflow_claimed: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            publication_durability,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            publication_epochs: std::collections::BTreeMap::new(),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            publication_barrier_failed: false,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            writer_package,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            writer_owner: None,
            recovery: recovery::OpenRecovery::default(),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            session_marker: false,
            _writer_lock: lock,
        };
        if newer_schema.is_some() {
            // A later build's history and operational tables cannot be
            // validated here. Show only what reads as a current document.
            store.check_newer_readable()?;
            return Ok(store);
        }
        let audit = store.validate_with(validation::HistoryMode::Receipt)?;
        store.opened = HistoryValidation::from(&audit);
        if mode == AccessMode::ReadWrite {
            store.recover_writable(&audit, false)?;
            store.begin_writer_session();
        }
        Ok(store)
    }

    /// The writable-open steps after validation: certify the chronology and
    /// interrupt every attempt a previous writer left running. Shared by
    /// opening and by restoring a backup into the open writer.
    fn recover_writable(
        &mut self,
        audit: &validation::HistoryAudit,
        restored: bool,
    ) -> Result<(), StoreError> {
        let store = self;
        {
            // Record this validator's proof of the complete chronology, so the
            // next open hashes the stored rows instead of replaying them.
            if audit.verified < audit.order.len() {
                let transaction = rusqlite::Transaction::new_unchecked(
                    &store.connection,
                    rusqlite::TransactionBehavior::Immediate,
                )?;
                audit::certify(
                    &transaction,
                    i64::try_from(audit.order.len()).unwrap_or(i64::MAX),
                    audit.order.last().expect("nonempty chronology"),
                    audit.chain,
                )?;
                transaction.commit()?;
            }
            // Read the previous writer's evidence first, but replace its
            // marker only after recovery succeeds: a failed or crashed
            // recovery leaves the earlier evidence for the next writer.
            // After a restore the marker is this session's own.
            let mut found = recovery::OpenRecovery {
                unclean_previous_writer: if restored {
                    None
                } else {
                    store.previous_writer()
                },
                ..Default::default()
            };
            let generations = generation_attempts::recover_nonterminal(&mut store.connection)?;
            generation_preparations::recover(&mut store.connection)?;
            found.interrupted_generation_count = generations.len();
            found.interrupted_generations = generations
                .into_iter()
                .take(recovery::MAX_REPORTED_ATTEMPTS)
                .collect();
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                let renders = render_jobs::recover_nonterminal(&mut store.connection)?;
                found.interrupted_render_count = renders.len();
                found.interrupted_renders = renders
                    .into_iter()
                    .take(recovery::MAX_REPORTED_ATTEMPTS)
                    .collect();
                let publications = publication::recover_nonterminal(&mut store.connection)?;
                found.interrupted_publication_count = publications.len();
                found.interrupted_publications = publications
                    .into_iter()
                    .take(recovery::MAX_REPORTED_ATTEMPTS)
                    .collect();
            }
            found.record_error = store.recovery.record_error.take();
            // Keep unacknowledged findings across writers, so a headless
            // open in between cannot swallow what the app must report.
            store.recovery = recovery::retain_pending(&store.package, found);
        }
        Ok(())
    }

    pub fn access_mode(&self) -> AccessMode {
        self.mode
    }

    /// The database schema of a package a newer Deadpan saved, opened
    /// read-only for viewing; `None` for current packages.
    pub fn newer_schema(&self) -> Option<u32> {
        self.newer_schema
    }

    /// A newer package is viewable only when SQLite integrity holds and its
    /// head document reads and validates under this build's document model.
    fn check_newer_readable(&self) -> Result<(), StoreError> {
        let found = self.newer_schema.unwrap_or(schema::VERSION);
        let unreadable = |detail: String| {
            StoreError::Integrity(format!(
                "this project was saved by a newer Deadpan (database schema {found}) and this build cannot read it ({detail}); nothing was changed"
            ))
        };
        let integrity: String = self
            .connection
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|error| unreadable(error.to_string()))?;
        if integrity != "ok" {
            return Err(StoreError::Integrity(integrity));
        }
        self.documents
            .head_validated(&self.connection)
            .map_err(|error| unreadable(error.to_string()))?;
        Ok(())
    }

    /// What writable opens found and recovered and nobody has acknowledged,
    /// including earlier opens. Read-only opens and new packages report
    /// nothing.
    pub fn open_recovery(&self) -> &recovery::OpenRecovery {
        &self.recovery
    }

    /// The person has seen the recovery report: forget the retained findings.
    pub fn acknowledge_recovery(&mut self) -> Result<(), StoreError> {
        self.require_writer()?;
        recovery::clear_pending(&self.package)?;
        self.recovery = recovery::OpenRecovery::default();
        Ok(())
    }

    /// A marker left by a writer of this package that never closed.
    fn previous_writer(&mut self) -> Option<recovery::PreviousWriter> {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if let Some(anchor) = &self.writer_package {
            match anchor.read_session() {
                Ok(host_owner::SessionEvidence::Unclean(marker)) => {
                    return Some(recovery::PreviousWriter { marker });
                }
                Ok(_) => {}
                Err(error) => {
                    self.recovery.record_error = Some(error.to_string());
                    return Some(recovery::PreviousWriter {
                        marker: format!("unreadable writer marker: {error}"),
                    });
                }
            }
        }
        None
    }

    /// Records this writer's marker. A marker that cannot be written (for
    /// example on a full disk) never prevents opening.
    fn begin_writer_session(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if let Some(anchor) = &self.writer_package {
            match anchor.write_session(&anchor.session_marker(&recovery::writer_marker())) {
                Ok(()) => self.session_marker = true,
                Err(error) => {
                    let error = format!("this session's crash marker was not recorded: {error}");
                    self.recovery.record_error = Some(match self.recovery.record_error.take() {
                        Some(earlier) => format!("{earlier}; {error}"),
                        None => error,
                    });
                }
            }
        }
    }

    /// Install the host's resolver so ordinary writes keep current
    /// generation requests reconciled instead of refusing.
    pub fn set_generation_context_resolver(
        &mut self,
        resolver: Arc<dyn generation::GenerationContextResolver>,
    ) {
        self.context_resolver = Some(resolver);
    }

    pub fn snapshot(&self) -> Result<ProjectDocument, StoreError> {
        Ok(ProjectDocument::clone(&*self.snapshot_shared()?))
    }

    /// The current revision's validated document without copying it. The
    /// store keeps the head in memory, so this does not parse or validate.
    pub fn snapshot_shared(&self) -> Result<Arc<ProjectDocument>, StoreError> {
        self.documents.head(&self.connection)
    }

    /// The current revision with its retained validation proof, so callers
    /// can compile plans in [`deadpan_core::ValidatedDocument::scope`]
    /// without validating the document again.
    pub fn snapshot_validated(&self) -> Result<deadpan_core::ValidatedDocument, StoreError> {
        self.documents.head_validated(&self.connection)
    }

    /// Read the authoritative current revision without decoding its document.
    pub fn head_revision(&self) -> Result<RevisionId, StoreError> {
        Ok(RevisionId::new(validation::read_head(&self.connection)?)?)
    }

    /// Read one immutable committed revision, including an abandoned branch.
    /// This does not move the history cursor or perform writer recovery.
    pub fn snapshot_at(&self, revision: &RevisionId) -> Result<ProjectDocument, StoreError> {
        Ok(ProjectDocument::clone(
            &*self
                .documents
                .revision(&self.connection, revision.as_str())?,
        ))
    }

    /// Immutable copy provenance, including captured intermediate transaction
    /// states. These identities never authorize a live command or export.
    pub fn capture_snapshot_at(
        &self,
        revision: &RevisionId,
    ) -> Result<ProjectDocument, StoreError> {
        compound::read_capture(&self.connection, revision)
    }

    /// Validate the package. History before this validator build's receipt is
    /// verified by its hash chain; later revisions are recomputed.
    pub fn validate(&self) -> Result<(), StoreError> {
        self.validate_report(false).map(|_| ())
    }

    /// Validate the package, recomputing every command and revision of the
    /// history from its initial revision regardless of any receipt.
    pub fn validate_full(&self) -> Result<(), StoreError> {
        self.validate_report(true).map(|_| ())
    }

    /// Validate the package and report how much history was recomputed.
    pub fn validate_report(&self, full: bool) -> Result<HistoryValidation, StoreError> {
        if let Some(found) = self.newer_schema {
            return Err(StoreError::NewerSchema {
                found,
                supported: schema::VERSION,
            });
        }
        let mode = if full {
            validation::HistoryMode::Full
        } else {
            validation::HistoryMode::Receipt
        };
        let audit = self.validate_with(mode)?;
        // Opening tolerates unreadable corrections so the app can report and
        // discard them; explicit validation does not.
        analysis_corrections::validate_store(&self.connection)?;
        Ok(HistoryValidation::from(&audit))
    }

    /// How opening this store validated its history.
    pub fn open_validation(&self) -> HistoryValidation {
        self.opened
    }

    fn validate_with(
        &self,
        mode: validation::HistoryMode,
    ) -> Result<validation::HistoryAudit, StoreError> {
        validate_database(&self.connection, mode)
    }

    pub fn preview(&self, request: &CommandRequest) -> Result<EditTransaction, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let plan = prepare_command(&transaction, &self.documents, request)?;
        compound::require_authored(&plan)?;
        Ok(plan.edit)
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
        let documents = &self.documents;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let plan = prepare_command(&transaction, documents, request)?;
        let (outcome, next) = write_command_plan(
            &transaction,
            documents,
            plan,
            relevance,
            self.context_resolver.as_deref(),
        )?;
        transaction.commit()?;
        documents.insert(next);
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
        let checkpoint = Connection::open_with_flags(temporary.path(), read_flags())?;
        schema::configure(&checkpoint)?;
        // Recompute the complete history of the copy; a checkpoint never
        // inherits the live database's receipt as proof.
        validation::check_stored_sizes(&checkpoint, schema::MAX_DOCUMENT_BYTES)?;
        compound::check_stored_sizes(&checkpoint)?;
        validation::validate_history(&checkpoint, validation::HistoryMode::Full)?;
        registers::validate_store(&checkpoint)?;
        drop(checkpoint);
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

    /// Gives an I/O worker revocable, connection-free generated-object access.
    /// Available from both writable and read-only stores. The capability pins
    /// the package namespace and is revoked before this store releases its lock.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub fn generated_read_handle(&self) -> generated_media::GeneratedReadHandle {
        generated_media::GeneratedReadHandle::new(
            Arc::clone(&self.generated_storage),
            Arc::clone(&self.generated_read_closed),
        )
    }

    fn require_writer(&self) -> Result<(), StoreError> {
        if self.mode == AccessMode::ReadOnly {
            return Err(StoreError::ReadOnly);
        }
        Ok(())
    }
}

/// Every stored-size bound, SQLite integrity, the history and each
/// operational table of one database, in one read transaction. Shared by
/// opening, explicit validation and backup verification.
fn validate_database(
    connection: &Connection,
    mode: validation::HistoryMode,
) -> Result<validation::HistoryAudit, StoreError> {
    let transaction = connection.unchecked_transaction()?;
    validation::check_stored_sizes(&transaction, schema::MAX_DOCUMENT_BYTES)?;
    compound::check_stored_sizes(&transaction)?;
    registers::check_stored_sizes(&transaction)?;
    generation::check_stored_sizes(&transaction)?;
    generation_preparations::check_stored_sizes(&transaction)?;
    generation_attempts::check_stored_sizes(&transaction)?;
    transcripts::check_stored_sizes(&transaction)?;
    speech_activity::check_stored_sizes(&transaction)?;
    shot_analysis::check_stored_sizes(&transaction)?;
    analysis_corrections::check_stored_sizes(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    render_jobs::check_stored_sizes(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    publication::check_stored_sizes(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    original_media::check_stored_sizes(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::check_stored_sizes(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    single_source::check_stored_sizes(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    original_provenance::validate_store(&transaction)?;
    let integrity: String = transaction.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
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
    let audit = validation::validate_history(&transaction, mode)?;
    generation::validate_store(&transaction)?;
    generation_attempts::validate_store(&transaction)?;
    generation_preparations::validate_store(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    render_jobs::validate_store(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    publication::validate_store(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    original_media::validate_store(&transaction)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::validate_store(&transaction, audit.verified)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    single_source::validate_store(&transaction, audit.verified)?;
    registers::validate_store(&transaction)?;
    Ok(audit)
}

/// Debug builds only: abort the process (as a kill would) at a named crash
/// window when `DEADPAN_STORE_FAILPOINT` names it, first appending `hit
/// <name>` to `DEADPAN_STORE_FAILPOINT_LOG`. Release builds compile it away.
pub(crate) fn failpoint(name: &str) {
    #[cfg(debug_assertions)]
    if std::env::var("DEADPAN_STORE_FAILPOINT").as_deref() == Ok(name) {
        if let Some(log) = std::env::var_os("DEADPAN_STORE_FAILPOINT_LOG")
            && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log)
        {
            let _ = writeln!(file, "hit {name}");
            let _ = file.sync_all();
        }
        std::process::abort();
    }
    #[cfg(not(debug_assertions))]
    let _ = name;
}

/// SQLite refused a read because a hot rollback journal needs a writer
/// (`SQLITE_READONLY_ROLLBACK`).
fn interrupted_rollback(error: &StoreError) -> bool {
    matches!(
        error,
        StoreError::Database(rusqlite::Error::SqliteFailure(failure, _))
            if failure.extended_code == rusqlite::ffi::SQLITE_READONLY_ROLLBACK
    )
}

/// The schema check of `open`: a newer schema is readable only for a
/// read-only open, and returns its version.
fn readable_version(connection: &Connection, mode: AccessMode) -> Result<Option<u32>, StoreError> {
    match schema::check_version(connection) {
        Ok(()) => Ok(None),
        Err(StoreError::NewerSchema { found, .. }) if mode == AccessMode::ReadOnly => {
            Ok(Some(found))
        }
        Err(error) => Err(error),
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn acquire_lock(package: &Path) -> Result<File, StoreError> {
    host_owner::acquire_lock(package)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn read_only_filesystem(path: &Path) -> bool {
    rustix::fs::statvfs(path).is_ok_and(|volume| {
        volume
            .f_flag
            .contains(rustix::fs::StatVfsMountFlags::RDONLY)
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read_only_filesystem(_path: &Path) -> bool {
    false
}

fn read_snapshot(connection: &Connection) -> Result<ProjectDocument, StoreError> {
    let head = validation::read_head(connection)?;
    Ok(validation::read_revision(connection, &head)?.document)
}

/// The project and current revision identities, without any document.
pub(crate) struct HeadIdentity {
    project: deadpan_core::ProjectId,
    revision: RevisionId,
}

impl HeadIdentity {
    pub(crate) fn project_id(&self) -> &deadpan_core::ProjectId {
        &self.project
    }
    pub(crate) fn revision_id(&self) -> &RevisionId {
        &self.revision
    }
}

/// Every revision shares the initial revision's project identity: commands
/// and stored patches are checked against it before they apply.
pub(crate) fn read_head_project(connection: &Connection) -> Result<HeadIdentity, StoreError> {
    let revision = RevisionId::new(validation::read_head(connection)?)?;
    let project: Option<String> = connection.query_row(
        "SELECT CASE WHEN typeof(document)='text' AND json_valid(document)
            THEN json_extract(document,'$.project_id') END
         FROM revisions WHERE parent_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    let project = project
        .ok_or_else(|| StoreError::Integrity("initial revision has no project identity".into()))?;
    Ok(HeadIdentity {
        project: deadpan_core::ProjectId::new(project)?,
        revision,
    })
}

/// The same optimistic guard covers command preparation and descriptive
/// previews which may legitimately produce no EditTransaction.
fn read_command_snapshot(
    connection: &Connection,
    documents: &document_cache::DocumentCache,
    request: &CommandRequest,
) -> Result<deadpan_core::ValidatedDocument, StoreError> {
    use deadpan_core::{EditError, EditErrorCode};

    let current = documents.head_validated(connection)?;
    if request.project_id != *current.project_id() {
        return Err(EditError {
            code: EditErrorCode::ProjectConflict,
            message: "command targets a different project".into(),
            current_revision: None,
        }
        .into());
    }
    if request.expected_revision != *current.revision_id() {
        return Err(EditError {
            code: EditErrorCode::RevisionConflict,
            message: format!(
                "expected revision {}; current revision is {}",
                request.expected_revision,
                current.revision_id()
            ),
            current_revision: Some(current.revision_id().clone()),
        }
        .into());
    }
    if request.expected_revision == request.new_revision {
        return Err(EditError {
            code: EditErrorCode::InvalidCommand,
            message: "new revision must differ from the current revision".into(),
            current_revision: None,
        }
        .into());
    }
    Ok(current)
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
    current: Arc<ProjectDocument>,
    next: deadpan_core::ValidatedDocument,
    edit: EditTransaction,
    request_json: String,
    edit_json: String,
    compound: Option<compound::Prepared>,
}

fn prepare_command(
    connection: &Connection,
    documents: &document_cache::DocumentCache,
    request: &CommandRequest,
) -> Result<CommandPlan, StoreError> {
    prepare_admitted_command(connection, documents, request, None)
}

fn prepare_admitted_command(
    connection: &Connection,
    documents: &document_cache::DocumentCache,
    request: &CommandRequest,
    admitted: Option<&deadpan_core::GeneratedArtifact>,
) -> Result<CommandPlan, StoreError> {
    prepare_command_with_admission(connection, documents, request, admitted, None, None)
}

fn prepare_command_with_admission(
    connection: &Connection,
    documents: &document_cache::DocumentCache,
    request: &CommandRequest,
    generated: Option<&deadpan_core::GeneratedArtifact>,
    source: Option<(&deadpan_core::AssetId, &deadpan_core::AssetRecord)>,
    geometry: Option<(u32, u32)>,
) -> Result<CommandPlan, StoreError> {
    let current = read_command_snapshot(connection, documents, request)?;
    prepare_current_command_with_admission(
        connection, documents, current, request, generated, source, geometry,
    )
}

fn prepare_current_command_with_admission(
    connection: &Connection,
    documents: &document_cache::DocumentCache,
    current: deadpan_core::ValidatedDocument,
    request: &CommandRequest,
    generated: Option<&deadpan_core::GeneratedArtifact>,
    source: Option<(&deadpan_core::AssetId, &deadpan_core::AssetRecord)>,
    geometry: Option<(u32, u32)>,
) -> Result<CommandPlan, StoreError> {
    if matches!(&request.command, deadpan_core::Command::Compound { .. }) {
        // Serialized commands cannot carry qualified import/acceptance capabilities.
        if generated.is_some() || source.is_some() || geometry.is_some() {
            return Err(StoreError::Integrity(
                "compound commands require per-leaf admission".into(),
            ));
        }
        return compound::prepare(connection, documents, current, request);
    }
    // The core validates the result once; it equals the forward patch applied
    // to `current`, so the store neither reapplies nor revalidates it, and the
    // document size is bounded by its stored patch (see `revision_storage`).
    let (edit, next) = deadpan_core::apply_validated(&current, request)?;
    ensure_unused_revision(connection, documents, &request.new_revision)?;
    let captured = slice_capture_revision(connection, request)?;
    validate_transition(
        connection,
        &current,
        &next,
        request,
        Admission {
            generated,
            source,
            geometry,
        },
        captured.as_ref(),
    )?;
    command_plan(Arc::clone(current.document()), next, edit, request, None)
}

fn command_plan(
    current: Arc<ProjectDocument>,
    next: deadpan_core::ValidatedDocument,
    edit: EditTransaction,
    request: &CommandRequest,
    compound: Option<compound::Prepared>,
) -> Result<CommandPlan, StoreError> {
    let request_json = serde_json::to_string(request)?;
    check_document_size(&request_json)?;
    // Typed native callers bypass JSON ingress. Admit their exact stored wire
    // through the replay reader before any writes, including its value/depth
    // bounds, so a successful save cannot create unreadable command history.
    if serde_json::from_str::<CommandRequest>(&request_json)? != *request {
        return Err(StoreError::Integrity(
            "command changes meaning when decoded for history replay".into(),
        ));
    }
    let edit_json = serde_json::to_string(&edit)?;
    check_document_size(&edit_json)?;
    // The remaining replay checks for this revision, so that a commit can
    // extend the history receipt: the stored edit decodes to the computed one
    // and its inverse restores the preceding revision exactly. The preceding
    // revision is valid, so the restored document needs no validation.
    if serde_json::from_str::<EditTransaction>(&edit_json)? != edit {
        return Err(StoreError::Integrity(
            "edit changes meaning when decoded for history replay".into(),
        ));
    }
    if !edit.inverse.restores(&next, &current)? {
        return Err(StoreError::History(
            "inverse does not restore the preceding revision".into(),
        ));
    }
    // The adopted result must be exactly what history replay reconstructs
    // from the stored patch, and its retained validation must equal complete
    // validation. Debug builds (every test) check both on every commit.
    #[cfg(debug_assertions)]
    {
        let replayed = edit.forward.apply_stored(&current)?;
        assert!(
            replayed == **next.document(),
            "commit result differs from its stored forward patch applied to the head"
        );
        next.check_against_complete_validation()
            .expect("retained validation must equal complete validation");
    }
    Ok(CommandPlan {
        current,
        next,
        edit,
        request_json,
        edit_json,
        compound,
    })
}

#[derive(Default)]
struct Admission<'a> {
    generated: Option<&'a deadpan_core::GeneratedArtifact>,
    source: Option<(&'a deadpan_core::AssetId, &'a deadpan_core::AssetRecord)>,
    geometry: Option<(u32, u32)>,
}

fn validate_transition(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    request: &CommandRequest,
    admission: Admission<'_>,
    captured: Option<&ProjectDocument>,
) -> Result<(), StoreError> {
    let Admission {
        generated,
        source,
        geometry,
    } = admission;
    ensure_generated_admission_with(Some(current), next, generated, captured)?;
    ensure_source_admission(Some(current), next, source, captured)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::validate_sound_sources(connection, current, next, captured)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::validate_hold_audio_source(connection, current, next, request)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::validate_source_slip(connection, current, next, request)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::validate_source_trim(connection, current, next, request)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::validate_source_roll(connection, current, next, request)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::validate_source_trim_edit(connection, current, next, request)?;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    if matches!(
        &request.command,
        deadpan_core::Command::SlipSource { .. }
            | deadpan_core::Command::TrimSource { .. }
            | deadpan_core::Command::RollSources { .. }
            | deadpan_core::Command::ApplySourceTrim { .. }
    ) {
        return Err(StoreError::SourceAdmissionUnavailable);
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    single_source::check_transition(connection, current, next, source.is_some())?;
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
    Ok(())
}

/// A slice may reuse media admitted in its immutable capture revision after the
/// original beat was deleted or its source registration undone. The payload's
/// provenance names a stored revision; it never supplies admission evidence.
fn slice_capture_revision(
    connection: &Connection,
    request: &CommandRequest,
) -> Result<Option<ProjectDocument>, StoreError> {
    let slice = match &request.command {
        deadpan_core::Command::SpliceSlice { slice, .. }
        | deadpan_core::Command::SpliceSliceAt { slice, .. }
        | deadpan_core::Command::ReplaceSlice { slice, .. } => slice,
        deadpan_core::Command::ReplaceSliceChildren { slice, .. } => slice,
        _ => return Ok(None),
    };
    let captured = compound::read_capture(connection, slice.revision_id())?;
    slice.validate_capture(&captured)?;
    Ok(Some(captured))
}

/// A qualified source binding may enter history only through the host's
/// decoded-source registration. Retaining existing immutable records is safe;
/// undo/redo and edited-slice paste can restore records from durable history.
fn ensure_source_admission(
    current: Option<&ProjectDocument>,
    next: &ProjectDocument,
    admitted: Option<(&deadpan_core::AssetId, &deadpan_core::AssetRecord)>,
    captured: Option<&ProjectDocument>,
) -> Result<(), StoreError> {
    for (id, asset) in next.assets() {
        if asset.source_qualification.is_none()
            || current.is_some_and(|document| document.assets().get(id) == Some(asset))
            || captured.is_some_and(|document| document.assets().get(id) == Some(asset))
            || admitted.is_some_and(|(allowed_id, allowed)| allowed_id == id && allowed == asset)
        {
            continue;
        }
        return Err(StoreError::SourceAdmissionUnavailable);
    }
    Ok(())
}

/// Write a prepared edit. The caller commits the transaction and then records
/// the returned document in the store's cache.
fn write_command_plan(
    connection: &Connection,
    documents: &document_cache::DocumentCache,
    plan: CommandPlan,
    relevance: Option<&generation::RelevancePlan>,
    resolver: Option<&dyn generation::GenerationContextResolver>,
) -> Result<(CommitOutcome, deadpan_core::ValidatedDocument), StoreError> {
    compound::require_authored(&plan)?;
    generation_preparations::verify(connection)?;
    let request: CommandRequest = serde_json::from_str(&plan.request_json)?;
    let preparations =
        generation_preparations::command_births(connection, &plan.current, &request, &plan.next)?;
    let isolated =
        generation_scope::command_transition(connection, &plan.current, &request, &plan.next)?;
    generation::reconcile(connection, &plan.current, &plan.next, relevance, resolver)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    source_registration::check_revision_assets(connection, &plan.current, &plan.next)?;
    insert_revision(
        connection,
        documents,
        &plan.current,
        &plan.next,
        "edit",
        revision_storage::StoredPatch::Edit {
            bytes: plan.edit_json.len(),
        },
    )?;
    let register_bank = match &plan.compound {
        Some(prepared) => {
            compound::write_steps(connection, plan.next.revision_id(), &prepared.steps)?;
            if prepared.writes {
                registers::write_prepared_bank(
                    connection,
                    &prepared.registers,
                    plan.current.project_id(),
                )?;
            }
            Some(prepared.registers.bank.clone())
        }
        None => None,
    };
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
    deadpan_diagnostics::IO
        .store_revisions
        .write((plan.request_json.len() + plan.edit_json.len()) as u64);
    let history_id = connection.last_insert_rowid();
    if isolated {
        generation_scope::insert_event(connection, plan.next.revision_id(), history_id, true)?;
    }
    connection.execute(
        "UPDATE state SET head_revision=?1,cursor=?2 WHERE singleton=1",
        params![plan.next.revision_id().as_str(), history_id],
    )?;
    connection.execute("DELETE FROM redo", [])?;
    generation_preparations::reconcile(connection, &plan.next, resolver)?;
    let (generation_preparations, generation_preparation_notices) =
        generation_preparations::insert_births(connection, &plan.next, history_id, preparations)?;
    audit::extend(
        connection,
        plan.current.revision_id().as_str(),
        plan.next.revision_id().as_str(),
    )?;
    Ok((
        CommitOutcome {
            revision_id: plan.next.revision_id().clone(),
            edit: plan.edit,
            register_bank,
            generation_preparations,
            generation_preparation_notices,
        },
        plan.next,
    ))
}

/// Core edits describe authored intent. They cannot prove that a candidate was
/// selected, revalidated, canonicalized and durably promoted. Generic store
/// commands may retain/copy an existing artifact,
/// but cannot introduce a new one, including through subtrees or Repeat gaps.
fn ensure_generated_admission(
    current: Option<&ProjectDocument>,
    next: &ProjectDocument,
) -> Result<(), StoreError> {
    ensure_generated_admission_with(current, next, None, None)
}

fn ensure_generated_admission_with(
    current: Option<&ProjectDocument>,
    next: &ProjectDocument,
    admitted: Option<&deadpan_core::GeneratedArtifact>,
    captured: Option<&ProjectDocument>,
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
    if let Some(document) = captured {
        retained.extend(artifacts(document)?);
    }
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
    documents: &document_cache::DocumentCache,
    revision: &RevisionId,
) -> Result<(), StoreError> {
    ensure_unused_revisions(connection, documents, &[revision])
}

fn ensure_unused_revisions(
    connection: &Connection,
    documents: &document_cache::DocumentCache,
    revisions: &[&RevisionId],
) -> Result<(), StoreError> {
    // A package may start from a nonempty imported snapshot. Its occurrence
    // and audio-lineage/timing allocations predate this database's revision
    // rows. Retained timing layouts also own old play namespaces. Reserve all
    // of those names forever, even after every live owner is removed.
    // Subsequent command allocations use committed revision IDs.
    let initial = documents.initial_allocations(connection)?;
    let allocations = &initial.names;
    let mut seen = std::collections::BTreeSet::new();
    for revision in revisions {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM revisions WHERE id=?1 UNION ALL SELECT 1 FROM transaction_steps WHERE step_revision=?1 UNION ALL SELECT 1 FROM retired_identities WHERE kind='revision' AND key=?1)",
            [revision.as_str()], |row| row.get(0),
        )?;
        if exists || allocations.contains(*revision) || !seen.insert(*revision) {
            return Err(StoreError::RevisionReused(revision.as_str().to_owned()));
        }
    }
    Ok(())
}

/// Insert a revision row whose stored forward patch is described by `patch`:
/// an edit's history entry, or an undo/redo revision's own patch row.
fn insert_revision(
    connection: &Connection,
    documents: &document_cache::DocumentCache,
    before: &ProjectDocument,
    after: &ProjectDocument,
    kind: &str,
    patch: revision_storage::StoredPatch<'_>,
) -> Result<(), StoreError> {
    ensure_unused_revision(connection, documents, after.revision_id())?;
    revision_storage::insert(connection, Some(before.revision_id()), after, kind, patch)
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
