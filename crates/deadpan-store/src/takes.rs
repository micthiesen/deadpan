//! Named, revision-bound compositions. Catalog writes are independent of Undo;
//! restoring a take is one ordinary reversible authored edit.

use std::{collections::BTreeSet, fmt, sync::Arc};

use deadpan_core::{Command, CommandRequest, ProjectDocument, ProjectId, RevisionId};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CommitOutcome, ProjectStore, StoreError};

pub const MAX_TAKES: usize = 256;
pub const MAX_TAKE_NAME_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TakeId(String);
impl TakeId {
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        RevisionId::new(value.clone()).map_err(|_| {
            invalid("take IDs require 1–128 ASCII letters, digits, hyphens or underscores")
        })?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for TakeId {
    type Error = StoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TakeId> for String {
    fn from(value: TakeId) -> Self {
        value.0
    }
}
impl fmt::Display for TakeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TakeName(String);
impl TakeName {
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_TAKE_NAME_BYTES
            || value.trim() != value
            || value
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            return Err(invalid(
                "take names require 1–128 UTF-8 bytes, no surrounding whitespace, line breaks or control characters",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for TakeName {
    type Error = StoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TakeName> for String {
    fn from(value: TakeName) -> Self {
        value.0
    }
}
impl fmt::Display for TakeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Take {
    pub id: TakeId,
    pub name: TakeName,
    pub revision_id: RevisionId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakeCatalog {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub version: u64,
    /// Stable ID order, independent of names and updates.
    pub entries: Vec<Take>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub project_id: ProjectId,
    pub expected_revision: RevisionId,
    pub expected_version: u64,
    pub action: Action,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Create {
        id: TakeId,
        name: TakeName,
    },
    Update {
        id: TakeId,
        expected_snapshot: RevisionId,
    },
    Rename {
        id: TakeId,
        expected_snapshot: RevisionId,
        name: TakeName,
    },
    Delete {
        id: TakeId,
        expected_snapshot: RevisionId,
    },
    Restore {
        id: TakeId,
        expected_snapshot: RevisionId,
        new_revision: RevisionId,
    },
}
#[derive(Debug, Serialize)]
pub struct Outcome {
    pub catalog: TakeCatalog,
    pub commit: Option<CommitOutcome>,
    pub changed: bool,
}

struct Prepared {
    catalog: TakeCatalog,
    changed: bool,
    created: Option<TakeId>,
    restore: Option<crate::CommandPlan>,
}

impl ProjectStore {
    pub fn take_catalog(&self) -> Result<TakeCatalog, StoreError> {
        if let Some(found) = self.newer_schema() {
            return Err(StoreError::NewerSchema {
                found,
                supported: crate::DATABASE_SCHEMA_VERSION,
            });
        }
        let transaction = self.connection.unchecked_transaction()?;
        read_catalog(&transaction)
    }

    pub fn preview_take(&self, request: &Request) -> Result<Outcome, StoreError> {
        if let Some(found) = self.newer_schema() {
            return Err(StoreError::NewerSchema {
                found,
                supported: crate::DATABASE_SCHEMA_VERSION,
            });
        }
        let transaction = self.connection.unchecked_transaction()?;
        let prepared = prepare(&transaction, &self.documents, request)?;
        let commit = prepared.restore.map(|plan| CommitOutcome {
            revision_id: plan.next.revision_id().clone(),
            edit: plan.edit,
            register_bank: None,
            generation_preparations: Vec::new(),
            generation_preparation_notices: Vec::new(),
        });
        Ok(Outcome {
            catalog: prepared.catalog,
            changed: prepared.changed,
            commit,
        })
    }

    pub fn apply_take(&mut self, request: &Request) -> Result<Outcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let prepared = prepare(&transaction, &self.documents, request)?;
        let committed = if let Some(plan) = prepared.restore {
            let relevance = crate::generation::RelevancePlan {
                from_revision: plan.current.revision_id().clone(),
                to_revision: plan.next.revision_id().clone(),
                observations: crate::generation::read_current_requests(&transaction)?
                    .into_iter()
                    .map(|request| crate::generation::RelevanceObservation {
                        request_id: request.request_id,
                        binding: request.binding,
                        target: request.target,
                        after_context: crate::generation::ContextObservation::Unresolved,
                    })
                    .collect(),
            };
            let command: CommandRequest = serde_json::from_str(&plan.request_json)?;
            let Command::RestoreSnapshot { snapshot } = command.command else {
                return Err(invalid("restore plan has another command"));
            };
            transaction.execute(
                "INSERT INTO take_restores(revision_id,snapshot_revision) VALUES(?1,?2)",
                params![
                    plan.next.revision_id().as_str(),
                    snapshot.revision_id().as_str()
                ],
            )?;
            Some(crate::write_command_plan(
                &transaction,
                &self.documents,
                plan,
                Some(&relevance),
                self.context_resolver.as_deref(),
            )?)
        } else {
            if prepared.changed {
                if let Some(id) = &prepared.created {
                    transaction.execute("INSERT INTO take_ids(id) VALUES(?1)", [id.as_str()])?;
                }
                write_catalog(&transaction, &prepared.catalog)?;
            }
            None
        };
        transaction.commit()?;
        let commit = committed.map(|(outcome, next)| {
            self.documents.insert(next);
            outcome
        });
        Ok(Outcome {
            catalog: prepared.catalog,
            changed: prepared.changed,
            commit,
        })
    }
}

fn prepare(
    connection: &Connection,
    documents: &crate::document_cache::DocumentCache,
    request: &Request,
) -> Result<Prepared, StoreError> {
    let mut catalog = read_catalog(connection)?;
    if catalog.project_id != request.project_id {
        return Err(invalid("take request belongs to another project"));
    }
    if catalog.revision_id != request.expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: request.expected_revision.to_string(),
            current: catalog.revision_id.to_string(),
        });
    }
    if catalog.version != request.expected_version {
        return Err(invalid(
            "take catalog version changed; refresh before applying this request",
        ));
    }
    let before = catalog.clone();
    let mut created = None;
    let mut restore = None;
    match &request.action {
        Action::Create { id, name } => {
            check_snapshot_reference(connection, &catalog.revision_id)?;
            require_available_name(&catalog, name, None)?;
            if catalog.entries.len() >= MAX_TAKES {
                return Err(invalid("the project already contains 256 takes"));
            }
            let used: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM take_ids WHERE id=?1)",
                [id.as_str()],
                |row| row.get(0),
            )?;
            if used {
                return Err(invalid("this take ID has already been used"));
            }
            catalog.entries.push(Take {
                id: id.clone(),
                name: name.clone(),
                revision_id: catalog.revision_id.clone(),
            });
            catalog.entries.sort_by(|a, b| a.id.cmp(&b.id));
            created = Some(id.clone());
        }
        Action::Update {
            id,
            expected_snapshot,
        }
        | Action::Rename {
            id,
            expected_snapshot,
            ..
        }
        | Action::Delete {
            id,
            expected_snapshot,
        }
        | Action::Restore {
            id,
            expected_snapshot,
            ..
        } => {
            let index = catalog
                .entries
                .iter()
                .position(|entry| &entry.id == id)
                .ok_or_else(|| invalid("the selected take no longer exists"))?;
            if &catalog.entries[index].revision_id != expected_snapshot {
                return Err(invalid("the selected take snapshot changed"));
            }
            match &request.action {
                Action::Update { .. } => {
                    check_snapshot_reference(connection, &catalog.revision_id)?;
                    catalog.entries[index].revision_id = catalog.revision_id.clone();
                }
                Action::Rename { name, .. } => {
                    require_available_name(&catalog, name, Some(id))?;
                    catalog.entries[index].name = name.clone();
                }
                Action::Delete { .. } => {
                    catalog.entries.remove(index);
                }
                Action::Restore { new_revision, .. } => {
                    let snapshot = ready_snapshot(connection, expected_snapshot)?;
                    let command = CommandRequest {
                        project_id: request.project_id.clone(),
                        expected_revision: request.expected_revision.clone(),
                        new_revision: new_revision.clone(),
                        command: Command::RestoreSnapshot {
                            snapshot: Box::new(snapshot),
                        },
                    };
                    let current = crate::read_command_snapshot(connection, documents, &command)?;
                    let (edit, next) = deadpan_core::apply_validated(&current, &command)?;
                    crate::ensure_unused_revision(connection, documents, new_revision)?;
                    let Command::RestoreSnapshot { snapshot } = &command.command else {
                        unreachable!()
                    };
                    crate::validate_transition(
                        connection,
                        &current,
                        &next,
                        &command,
                        crate::Admission::default(),
                        Some(snapshot),
                    )?;
                    restore = Some(crate::command_plan(
                        Arc::clone(current.document()),
                        next,
                        edit,
                        &command,
                        None,
                    )?);
                    catalog.revision_id = new_revision.clone();
                }
                Action::Create { .. } => unreachable!(),
            }
        }
    }
    let changed = before != catalog;
    if changed && restore.is_none() {
        catalog.version = catalog
            .version
            .checked_add(1)
            .filter(|version| *version <= i64::MAX as u64)
            .ok_or_else(|| invalid("take catalog version exhausted"))?;
    }
    Ok(Prepared {
        catalog,
        changed,
        created,
        restore,
    })
}

fn require_available_name(
    catalog: &TakeCatalog,
    name: &TakeName,
    selected: Option<&TakeId>,
) -> Result<(), StoreError> {
    if catalog
        .entries
        .iter()
        .any(|entry| &entry.name == name && Some(&entry.id) != selected)
    {
        return Err(invalid("another take already uses this name"));
    }
    Ok(())
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch("CREATE TABLE take_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1), version INTEGER NOT NULL CHECK(version>=0), digest BLOB NOT NULL CHECK(length(digest)=32)) STRICT;
        CREATE TABLE take_ids(id TEXT PRIMARY KEY CHECK(length(CAST(id AS BLOB)) BETWEEN 1 AND 128)) STRICT;
        CREATE TABLE takes(id TEXT PRIMARY KEY REFERENCES take_ids(id), name TEXT NOT NULL UNIQUE CHECK(length(CAST(name AS BLOB)) BETWEEN 1 AND 128), snapshot_revision TEXT NOT NULL REFERENCES revisions(id)) STRICT;
        CREATE TABLE take_restores(revision_id TEXT PRIMARY KEY REFERENCES revisions(id) DEFERRABLE INITIALLY DEFERRED, snapshot_revision TEXT NOT NULL REFERENCES revisions(id)) STRICT;")?;
    let digest = digest(connection, 0, &[])?;
    connection.execute(
        "INSERT INTO take_state(singleton,version,digest) VALUES(1,0,?1)",
        [&digest[..]],
    )?;
    Ok(())
}

fn digest(connection: &Connection, version: u64, entries: &[Take]) -> Result<[u8; 32], StoreError> {
    let mut hash = Sha256::new();
    hash.update(b"deadpan-takes-1");
    hash.update(serde_json::to_vec(&(version, entries))?);
    // Stream permanently reserved IDs; deleted labels never free their identity.
    let mut statement = connection.prepare("SELECT id FROM take_ids ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let id = TakeId::new(id)?;
        hash.update((id.as_str().len() as u64).to_le_bytes());
        hash.update(id.as_str().as_bytes());
    }
    Ok(hash.finalize().into())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let bad: bool = connection.query_row("SELECT
        (SELECT count(*) FROM take_state)!=1 OR EXISTS(SELECT 1 FROM take_state WHERE singleton!=1 OR typeof(version)!='integer' OR version<0 OR typeof(digest)!='blob' OR length(digest)!=32)
        OR (SELECT count(*) FROM takes)>256
        OR EXISTS(SELECT 1 FROM takes WHERE typeof(id)!='text' OR length(CAST(id AS BLOB)) NOT BETWEEN 1 AND 128 OR typeof(name)!='text' OR length(CAST(name AS BLOB)) NOT BETWEEN 1 AND 128 OR typeof(snapshot_revision)!='text' OR length(CAST(snapshot_revision AS BLOB)) NOT BETWEEN 1 AND 128)
        OR EXISTS(SELECT 1 FROM take_ids WHERE typeof(id)!='text' OR length(CAST(id AS BLOB)) NOT BETWEEN 1 AND 128)
        OR EXISTS(SELECT 1 FROM take_restores WHERE typeof(revision_id)!='text' OR length(CAST(revision_id AS BLOB)) NOT BETWEEN 1 AND 128 OR typeof(snapshot_revision)!='text' OR length(CAST(snapshot_revision AS BLOB)) NOT BETWEEN 1 AND 128)", [], |row| row.get(0))?;
    if bad {
        return Err(invalid("invalid or oversized named take metadata"));
    }
    Ok(())
}

fn read_catalog(connection: &Connection) -> Result<TakeCatalog, StoreError> {
    check_stored_sizes(connection)?;
    let head = crate::read_head_project(connection)?;
    let (version, stored): (u64, Vec<u8>) = connection.query_row(
        "SELECT version,digest FROM take_state WHERE singleton=1",
        [],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get(1)?)),
    )?;
    let mut entries = Vec::new();
    let mut names = BTreeSet::new();
    let mut statement =
        connection.prepare("SELECT id,name,snapshot_revision FROM takes ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let entry = Take {
            id: TakeId::new(row.get::<_, String>(0)?)?,
            name: TakeName::new(row.get::<_, String>(1)?)?,
            revision_id: RevisionId::new(row.get::<_, String>(2)?)?,
        };
        if !names.insert(entry.name.clone()) {
            return Err(invalid("named take catalog contains duplicate names"));
        }
        let reserved: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM take_ids WHERE id=?1)",
            [entry.id.as_str()],
            |row| row.get(0),
        )?;
        if !reserved {
            return Err(invalid("take has no permanent identity reservation"));
        }
        // Whole-store opening proves every timeline revision belongs to this
        // project. Listing never reconstructs the potentially large snapshots.
        check_snapshot_reference(connection, &entry.revision_id)?;
        entries.push(entry);
    }
    if stored != digest(connection, version, &entries)? {
        return Err(invalid(
            "take catalog digest differs from its stored values",
        ));
    }
    Ok(TakeCatalog {
        project_id: head.project_id().clone(),
        revision_id: head.revision_id().clone(),
        version,
        entries,
    })
}

fn write_catalog(connection: &Connection, catalog: &TakeCatalog) -> Result<(), StoreError> {
    connection.execute("DELETE FROM takes", [])?;
    for entry in &catalog.entries {
        connection.execute(
            "INSERT INTO takes(id,name,snapshot_revision) VALUES(?1,?2,?3)",
            params![
                entry.id.as_str(),
                entry.name.as_str(),
                entry.revision_id.as_str()
            ],
        )?;
    }
    let digest = digest(connection, catalog.version, &catalog.entries)?;
    connection.execute(
        "UPDATE take_state SET version=?1,digest=?2 WHERE singleton=1",
        params![
            i64::try_from(catalog.version)
                .map_err(|_| invalid("take catalog version exhausted"))?,
            &digest[..]
        ],
    )?;
    Ok(())
}

fn ready_snapshot(
    connection: &Connection,
    revision: &RevisionId,
) -> Result<ProjectDocument, StoreError> {
    check_snapshot_reference(connection, revision)?;
    Ok(crate::validation::read_revision(connection, revision.as_str())?.document)
}

fn check_snapshot_reference(
    connection: &Connection,
    revision: &RevisionId,
) -> Result<(), StoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM revisions WHERE id=?1)",
        [revision.as_str()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(invalid("take snapshot is not a retained timeline revision"));
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    match crate::single_source::read(connection)? {
        Some((crate::single_source::SingleSourceState::AwaitingSource { .. }, _)) => {
            return Err(invalid(
                "choose the Original before saving or restoring takes",
            ));
        }
        Some((
            crate::single_source::SingleSourceState::Ready {
                baseline_revision, ..
            },
            _,
        )) => {
            let later: bool = connection.query_row("SELECT target.rowid>=baseline.rowid FROM revisions target,revisions baseline WHERE target.id=?1 AND baseline.id=?2", params![revision.as_str(),baseline_revision.as_str()], |row| row.get(0))?;
            if !later {
                return Err(invalid(
                    "take snapshot precedes the protected Original baseline",
                ));
            }
        }
        None => {}
    }
    Ok(())
}

/// Both the immutable payload and the store-only proof must name an already
/// replayed timeline revision. Labels can subsequently change or disappear.
pub(crate) fn validate_restore(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    request: &CommandRequest,
    admitted: &BTreeSet<RevisionId>,
) -> Result<(), StoreError> {
    let Command::RestoreSnapshot { snapshot } = &request.command else {
        return Err(invalid("restore proof has another command"));
    };
    if !admitted.contains(snapshot.revision_id()) {
        return Err(invalid(
            "restored snapshot is not an earlier admitted timeline revision",
        ));
    }
    let proof = restore_proof(connection, request.new_revision.as_str())?;
    if proof.as_deref() != Some(snapshot.revision_id().as_str()) {
        return Err(invalid("restore has no matching immutable store proof"));
    }
    let retained = ready_snapshot(connection, snapshot.revision_id())?;
    if retained != **snapshot {
        return Err(invalid(
            "restore snapshot differs from the retained immutable revision",
        ));
    }
    crate::validate_transition(
        connection,
        current,
        next,
        request,
        crate::Admission::default(),
        Some(&retained),
    )
}

pub(crate) fn restore_proof(
    connection: &Connection,
    revision: &str,
) -> Result<Option<String>, StoreError> {
    let value: Option<Option<String>> = connection.query_row("SELECT CASE WHEN typeof(snapshot_revision)='text' AND length(CAST(snapshot_revision AS BLOB)) BETWEEN 1 AND 128 THEN snapshot_revision END FROM take_restores WHERE revision_id=?1", [revision], |row| row.get(0)).optional()?;
    value
        .map(|value| value.ok_or_else(|| invalid("invalid restore proof")))
        .transpose()
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    read_catalog(connection)?;
    let mut statement = connection.prepare("SELECT t.revision_id,h.request FROM take_restores t LEFT JOIN history h ON h.revision_id=t.revision_id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let request: Option<String> = row.get(1)?;
        let request: CommandRequest =
            serde_json::from_str(&request.ok_or_else(|| invalid("orphaned take restore proof"))?)?;
        if !matches!(request.command, Command::RestoreSnapshot { .. })
            || request.new_revision.as_str() != id
        {
            return Err(invalid("take restore proof belongs to another command"));
        }
    }
    Ok(())
}

/// Backup replacement must not free identities or make an old catalog version
/// current again. This runs inside the backup restore transaction.
pub(crate) fn carry_forward(live: &Connection, copy: &Connection) -> Result<(), StoreError> {
    let old = read_catalog(live)?;
    let mut restored = read_catalog(copy)?;
    let mut statement = live.prepare("SELECT id FROM take_ids ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        copy.execute(
            "INSERT OR IGNORE INTO take_ids(id) VALUES(?1)",
            [row.get::<_, String>(0)?],
        )?;
    }
    restored.version = old
        .version
        .max(restored.version)
        .checked_add(1)
        .filter(|version| *version <= i64::MAX as u64)
        .ok_or_else(|| invalid("take catalog version exhausted"))?;
    write_catalog(copy, &restored)
}

fn invalid(message: &str) -> StoreError {
    StoreError::Takes(message.into())
}

#[cfg(test)]
mod tests;
