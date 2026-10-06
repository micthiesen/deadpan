//! Project-owned copies and macros, independent of the timeline's undo cursor.
//!
//! Slots refer to canonical, content-addressed values. A named copy also writes
//! the unnamed slot; macros replace only their named slot. A cut and both copy
//! writes share the timeline transaction.

use std::{collections::BTreeMap, io::Write, ops::Range, sync::Arc};

use deadpan_core::{
    AssetId, CapturedEditSlice, Command, CommandRequest, ProjectDocument, ProjectId, RevisionId,
    SemanticProgram, SliceCaptureSelection, SourceQualificationId,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CommitOutcome, ProjectStore, StoreError, generation::RelevancePlan};

/// Total canonical bytes retained by distinct register values, including metadata.
pub const MAX_REGISTER_BYTES: usize = 64 * 1024 * 1024;
const MAX_SLOTS: i64 = 27;
// The fixed RegisterValue envelope adds fewer than 64 canonical bytes.
const MAX_MACRO_REGISTER_BYTES: usize = deadpan_core::MAX_SEMANTIC_PROGRAM_BYTES + 64;

pub use deadpan_core::{RegisterName, RegisterValue};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterBank {
    pub version: u64,
    pub entries: BTreeMap<RegisterName, Arc<RegisterValue>>,
}

impl ProjectStore {
    /// Cheap cache identity. A changed version requires a fresh `registers` read.
    pub fn register_version(&self) -> Result<u64, StoreError> {
        let version: Option<i64> = self.connection.query_row(
            "SELECT CASE WHEN typeof(version)='integer' AND version>=0 THEN version END FROM register_state WHERE singleton=1",
            [], |row| row.get(0),
        )?;
        version
            .map(|value| value as u64)
            .ok_or_else(|| invalid("invalid register version"))
    }

    /// Read a consistent, bounded bank, revalidating immutable capture provenance.
    pub fn registers(&self) -> Result<RegisterBank, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        read_bank(&transaction)
    }

    /// Read the committed document and register bank from one pinned SQLite
    /// snapshot. Available to read-only inspectors while another store writes.
    pub fn snapshot_with_registers(&self) -> Result<(ProjectDocument, RegisterBank), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let document = ProjectDocument::clone(&*self.documents.head(&transaction)?);
        let registers = read_bank(&transaction)?;
        Ok((document, registers))
    }

    /// Save a typed register without creating a timeline revision or changing Undo/Redo.
    /// Copies update the unnamed alias; macros require a named slot and preserve it.
    pub fn save_register(
        &mut self,
        expected_project: &ProjectId,
        expected_revision: &RevisionId,
        name: RegisterName,
        value: RegisterValue,
    ) -> Result<RegisterBank, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current = crate::read_head_project(&transaction)?;
        if current.project_id() != expected_project {
            return Err(invalid("copy belongs to another project"));
        }
        if current.revision_id() != expected_revision {
            return Err(StoreError::RevisionConflict {
                expected: expected_revision.as_str().into(),
                current: current.revision_id().as_str().into(),
            });
        }
        let bank = write_register(&transaction, name, value, current.project_id())?;
        transaction.commit()?;
        Ok(bank)
    }

    /// [`Self::save_register`] that also requires the register bank version
    /// the caller observed. Headless copies use it so a stale bank refuses.
    pub fn save_register_at(
        &mut self,
        expected_project: &ProjectId,
        expected_revision: &RevisionId,
        expected_bank_version: u64,
        name: RegisterName,
        value: RegisterValue,
    ) -> Result<RegisterBank, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let prepared = prepare_copy(
            &transaction,
            expected_project,
            expected_revision,
            expected_bank_version,
            name,
            value,
        )?;
        write_prepared_bank(&transaction, &prepared, expected_project)?;
        transaction.commit()?;
        Ok(prepared.bank)
    }

    /// The bank [`Self::save_register_at`] would write, with the same guards
    /// and content validation, without writing.
    pub fn preview_register(
        &self,
        expected_project: &ProjectId,
        expected_revision: &RevisionId,
        expected_bank_version: u64,
        name: RegisterName,
        value: RegisterValue,
    ) -> Result<RegisterBank, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let prepared = prepare_copy(
            &transaction,
            expected_project,
            expected_revision,
            expected_bank_version,
            name,
            value,
        )?;
        for content in prepared.contents.values() {
            if !content.validated {
                validate_value(&transaction, &content.value, expected_project)?;
            }
        }
        Ok(prepared.bank)
    }

    /// Replace only one named macro, binding the complete entry workspace and bank.
    /// Saving a macro leaves copied contents, timeline history and Redo intact.
    pub fn save_macro(
        &mut self,
        expected_project: &ProjectId,
        expected_revision: &RevisionId,
        expected_bank_version: u64,
        name: RegisterName,
        program: Arc<SemanticProgram>,
    ) -> Result<RegisterBank, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let prepared = prepare_macro(
            &transaction,
            expected_project,
            expected_revision,
            expected_bank_version,
            name,
            program,
        )?;
        write_prepared_bank(&transaction, &prepared, expected_project)?;
        transaction.commit()?;
        Ok(prepared.bank)
    }

    /// Prepare the exact prospective macro bank without writing or reserving a
    /// revision. Uses the save path's guards, validation and capacity checks.
    pub fn preview_macro(
        &self,
        expected_project: &ProjectId,
        expected_revision: &RevisionId,
        expected_bank_version: u64,
        name: RegisterName,
        program: Arc<SemanticProgram>,
    ) -> Result<RegisterBank, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        Ok(prepare_macro(
            &transaction,
            expected_project,
            expected_revision,
            expected_bank_version,
            name,
            program,
        )?
        .bank)
    }

    /// Commit exactly the captured range/child deletion and its durable copy.
    /// Any validation, storage or commit failure preserves both timeline and bank.
    pub fn cut_to_register(
        &mut self,
        request: &CommandRequest,
        name: RegisterName,
        slice: Arc<CapturedEditSlice>,
        relevance: Option<&RelevancePlan>,
    ) -> Result<(CommitOutcome, RegisterBank), StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let documents = &self.documents;
        let plan = crate::prepare_command(&transaction, documents, request)?;
        slice.validate_capture(&plan.current)?;
        let matches = match (&request.command, slice.selection()) {
            (
                Command::DeleteRange { parent, range, .. },
                SliceCaptureSelection::Range { range: captured },
            ) => parent == slice.parent() && range == captured,
            (
                Command::DeleteRipple { node, .. },
                SliceCaptureSelection::Child { node: captured },
            ) => node == captured,
            (
                Command::DeleteChildren {
                    parent,
                    first,
                    last,
                    ..
                },
                SliceCaptureSelection::Children {
                    first: captured_first,
                    last: captured_last,
                },
            ) => parent == slice.parent() && first == captured_first && last == captured_last,
            _ => false,
        };
        if !matches {
            return Err(invalid(
                "cut command does not delete the exact captured selection",
            ));
        }
        let bank = write_register(
            &transaction,
            name,
            RegisterValue::Edited { slice },
            plan.current.project_id(),
        )?;
        let (outcome, next) = crate::write_command_plan(
            &transaction,
            documents,
            plan,
            relevance,
            self.context_resolver.as_deref(),
        )?;
        transaction.commit()?;
        documents.insert(next);
        Ok((outcome, bank))
    }
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE register_state (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            version INTEGER NOT NULL CHECK(version>=0),
            bank_digest TEXT NOT NULL CHECK(length(CAST(bank_digest AS BLOB))=64)
        ) STRICT;
        CREATE TABLE register_contents (
            id TEXT PRIMARY KEY,
            capture_revision TEXT REFERENCES revisions(id),
            capture_step TEXT REFERENCES transaction_steps(step_revision),
            value TEXT NOT NULL CHECK(json_valid(value)),
            CHECK (coalesce(
                (json_extract(value,'$.type')='macro' AND capture_revision IS NULL AND capture_step IS NULL)
                OR (json_extract(value,'$.type') IN ('original','edited') AND ((capture_revision IS NULL) != (capture_step IS NULL))),
                0))
        ) STRICT;
        CREATE TABLE registers (
            name TEXT PRIMARY KEY CHECK(length(CAST(name AS BLOB))=1 AND (name='\"' OR name GLOB '[a-z]')),
            content_id TEXT NOT NULL REFERENCES register_contents(id)
        ) STRICT;",
    )?;
    connection.execute(
        "INSERT INTO register_state(singleton,version,bank_digest) VALUES(1,0,?1)",
        [bank_digest(0, std::iter::empty())],
    )?;
    Ok(())
}

/// Integrity digest of the bank's slot table and version: SHA-256 over a
/// domain tag, the version and every `name:content-id` slot in name order.
/// Content rows are already addressed by their own digests, so this binds
/// which content each name holds and which version the bank is at. A lost,
/// added, renamed or retargeted slot row, or a changed version, no longer
/// matches. Like the history receipt chain it is unkeyed: it detects
/// corruption and partial tampering, not a consistent rewrite by a writer.
fn bank_digest<'a>(version: u64, slots: impl Iterator<Item = (char, &'a str)>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"deadpan-register-bank-v1\n");
    hasher.update(version.to_string().as_bytes());
    hasher.update(b"\n");
    for (name, id) in slots {
        let mut buffer = [0_u8; 4];
        hasher.update(name.encode_utf8(&mut buffer).as_bytes());
        hasher.update(b":");
        hasher.update(id.as_bytes());
        hasher.update(b"\n");
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Test support: recompute the stored digest after a deliberate direct edit,
/// so a test can reach the checks behind the digest.
#[cfg(test)]
pub(crate) fn reseal_for_test(connection: &Connection) -> Result<(), StoreError> {
    let version: i64 =
        connection.query_row("SELECT version FROM register_state", [], |r| r.get(0))?;
    let slots: Vec<(String, String)> = connection
        .prepare("SELECT name,content_id FROM registers ORDER BY name")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let digest = bank_digest(
        u64::try_from(version).unwrap_or(0),
        slots
            .iter()
            .map(|(name, id)| (name.chars().next().unwrap_or('?'), id.as_str())),
    );
    connection.execute("UPDATE register_state SET bank_digest=?1", [digest])?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let states: i64 =
        connection.query_row("SELECT count(*) FROM register_state", [], |r| r.get(0))?;
    let slots: i64 = connection.query_row("SELECT count(*) FROM registers", [], |r| r.get(0))?;
    let contents: i64 =
        connection.query_row("SELECT count(*) FROM register_contents", [], |r| r.get(0))?;
    if states != 1 || slots > MAX_SLOTS || contents > MAX_SLOTS {
        return Err(invalid("invalid register row count"));
    }
    let malformed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM register_state WHERE singleton!=1 OR typeof(version)!='integer' OR version<0
            OR typeof(bank_digest)!='text' OR length(CAST(bank_digest AS BLOB))!=64 OR bank_digest GLOB '*[^0-9a-f]*')
        OR EXISTS(SELECT 1 FROM registers WHERE typeof(name)!='text' OR length(CAST(name AS BLOB))!=1
            OR NOT (name='\"' OR name GLOB '[a-z]') OR typeof(content_id)!='text'
            OR length(CAST(content_id AS BLOB))!=64 OR content_id GLOB '*[^0-9a-f]*')
        OR EXISTS(SELECT 1 FROM register_contents WHERE typeof(id)!='text' OR length(CAST(id AS BLOB))!=64
            OR id GLOB '*[^0-9a-f]*'
            OR (capture_revision IS NOT NULL AND capture_step IS NOT NULL)
            OR (capture_revision IS NOT NULL AND (typeof(capture_revision)!='text'
                OR length(CAST(capture_revision AS BLOB)) NOT BETWEEN 1 AND ?2))
            OR (capture_step IS NOT NULL AND (typeof(capture_step)!='text'
                OR length(CAST(capture_step AS BLOB)) NOT BETWEEN 1 AND ?2))
            OR typeof(value)!='text' OR length(CAST(value AS BLOB)) NOT BETWEEN 1 AND ?1)",
        params![MAX_REGISTER_BYTES as i64, deadpan_core::MAX_IDENTITY_BYTES as i64],
        |r| r.get(0),
    )?;
    if malformed {
        return Err(invalid("invalid register field type or size"));
    }
    // Bound identities before SQLite may materialize them for grouping.
    let duplicate: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM registers GROUP BY name HAVING count(*)>1)
        OR EXISTS(SELECT 1 FROM register_contents GROUP BY id HAVING count(*)>1)",
        [],
        |r| r.get(0),
    )?;
    if duplicate {
        return Err(invalid("duplicate register name or content identity"));
    }
    let bytes: i64 = connection.query_row(
        "SELECT coalesce(sum(length(CAST(value AS BLOB))),0) FROM register_contents",
        [],
        |r| r.get(0),
    )?;
    if bytes > MAX_REGISTER_BYTES as i64 {
        return Err(invalid(
            "register contents exceed the aggregate 64 MiB limit",
        ));
    }
    // Raw type and aggregate bounds precede JSON inspection, including hostile
    // packages that replace STRICT tables or disable their constraints.
    let malformed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM register_contents WHERE NOT CASE WHEN json_valid(value) THEN
            coalesce((json_extract(value,'$.type')='macro' AND capture_revision IS NULL AND capture_step IS NULL AND length(CAST(value AS BLOB))<=?1)
            OR (json_extract(value,'$.type') IN ('original','edited') AND ((capture_revision IS NULL) != (capture_step IS NULL))),0)
            ELSE 0 END)", [MAX_MACRO_REGISTER_BYTES as i64], |r| r.get(0),
    )?;
    if malformed {
        return Err(invalid("register type and capture provenance disagree"));
    }
    let inconsistent: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM register_contents c WHERE NOT EXISTS(SELECT 1 FROM registers r WHERE r.content_id=c.id))
        OR EXISTS(SELECT 1 FROM registers r WHERE NOT EXISTS(SELECT 1 FROM register_contents c WHERE c.id=r.content_id))
        OR EXISTS(SELECT 1 FROM register_contents c WHERE c.capture_revision IS NOT NULL AND NOT EXISTS(SELECT 1 FROM revisions v WHERE v.id=c.capture_revision))
        OR EXISTS(SELECT 1 FROM register_contents c WHERE c.capture_step IS NOT NULL AND NOT EXISTS(SELECT 1 FROM transaction_steps s WHERE s.step_revision=c.capture_step AND s.document IS NOT NULL))
        OR EXISTS(SELECT 1 FROM registers r JOIN register_contents c ON c.id=r.content_id WHERE r.name='\"' AND json_extract(c.value,'$.type')='macro')
        OR (EXISTS(SELECT 1 FROM register_contents WHERE json_extract(value,'$.type')!='macro') AND NOT EXISTS(SELECT 1 FROM registers WHERE name='\"'))
        OR EXISTS(SELECT 1 FROM register_state WHERE (version=0)!=(NOT EXISTS(SELECT 1 FROM registers)))",
        [], |r| r.get(0),
    )?;
    if inconsistent {
        return Err(invalid(
            "register references, default slot or version are inconsistent",
        ));
    }
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    read_bank(connection).map(|_| ())
}

pub(crate) fn read_bank(connection: &Connection) -> Result<RegisterBank, StoreError> {
    read_bank_contents(connection).map(|(bank, _)| bank)
}

fn read_bank_contents(
    connection: &Connection,
) -> Result<(RegisterBank, BTreeMap<String, CanonicalContent>), StoreError> {
    check_stored_sizes(connection)?;
    let project = crate::read_head_project(connection)?.project_id().clone();
    let (version, stored_digest): (i64, String) = connection.query_row(
        "SELECT version,bank_digest FROM register_state WHERE singleton=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let version = u64::try_from(version).map_err(|_| invalid("invalid register version"))?;
    let mut values = BTreeMap::new();
    let mut statement = connection.prepare(
        "SELECT id,capture_revision,capture_step,value FROM register_contents ORDER BY id",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let revision: Option<String> = row.get(1)?;
        let step: Option<String> = row.get(2)?;
        let json: String = row.get(3)?;
        if digest(json.as_bytes()) != id {
            return Err(invalid("register content hash disagrees with its bytes"));
        }
        let value: RegisterValue = serde_json::from_str(&json)?;
        let columns = capture_columns(connection, &value)?;
        if columns != (revision, step) || canonical(&value)? != json.as_bytes() {
            return Err(invalid(
                "register content is not canonical or names another capture revision",
            ));
        }
        validate_value(connection, &value, &project)?;
        values.insert(
            id,
            CanonicalContent {
                value: Arc::new(value),
                json: json.into_bytes(),
                validated: true,
            },
        );
    }
    let mut entries = BTreeMap::new();
    let mut slots = Vec::new();
    let mut statement =
        connection.prepare("SELECT name,content_id FROM registers ORDER BY name")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        let id: String = row.get(1)?;
        let name = RegisterName::new(
            name.chars()
                .next()
                .ok_or_else(|| invalid("empty register name"))?,
        )?;
        let value = values
            .get(&id)
            .ok_or_else(|| invalid("register content is missing"))?;
        entries.insert(name, Arc::clone(&value.value));
        slots.push((name.as_char(), id));
    }
    slots.sort();
    if bank_digest(version, slots.iter().map(|(name, id)| (*name, id.as_str()))) != stored_digest {
        return Err(invalid(
            "register bank digest disagrees with its slots and version",
        ));
    }
    Ok((RegisterBank { version, entries }, values))
}

pub(crate) fn validate_value(
    connection: &Connection,
    value: &RegisterValue,
    project: &ProjectId,
) -> Result<(), StoreError> {
    match value.capture_revision() {
        Some(revision) => {
            let captured = crate::compound::read_capture(connection, revision)?;
            validate_value_at(connection, value, project, &captured)
        }
        None => match value {
            RegisterValue::Macro { program } => Ok(program.validate()?),
            _ => Err(invalid("copied register has no capture revision")),
        },
    }
}

pub(crate) fn validate_value_at(
    connection: &Connection,
    value: &RegisterValue,
    project: &ProjectId,
    captured: &ProjectDocument,
) -> Result<(), StoreError> {
    if value.capture_revision() != Some(captured.revision_id()) {
        return Err(invalid("register capture revision differs from its source"));
    }
    if captured.project_id() != project {
        return Err(invalid("register capture belongs to another project"));
    }
    match value {
        RegisterValue::Edited { slice } => slice.validate_capture(captured)?,
        RegisterValue::Original {
            asset,
            qualification,
            ordinals,
            ..
        } => {
            validate_original(connection, captured, asset, qualification, ordinals)?;
        }
        RegisterValue::Macro { .. } => return Err(invalid("macros have no media capture")),
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn validate_original(
    connection: &Connection,
    captured: &ProjectDocument,
    asset: &AssetId,
    qualification: &SourceQualificationId,
    ordinals: &Range<u64>,
) -> Result<(), StoreError> {
    let record = captured
        .assets()
        .get(asset)
        .ok_or_else(|| invalid("Original asset is absent from its capture revision"))?;
    if record.source_qualification.as_ref() != Some(qualification) {
        return Err(invalid(
            "Original qualification differs from its historical asset",
        ));
    }
    let receipt = crate::source_registration::read_receipt(connection, qualification)?
        .ok_or_else(|| invalid("Original qualification is missing"))?;
    if receipt.asset_record(record.label.clone())? != *record {
        return Err(invalid(
            "Original asset differs from its retained qualification",
        ));
    }
    let video = receipt
        .snapshot()
        .video()
        .ok_or_else(|| invalid("Original qualification has no picture"))?;
    let count = u64::try_from(video.index().index().frames().len())
        .map_err(|_| invalid("Original frame count is not representable"))?;
    if ordinals.start >= ordinals.end || ordinals.end > count {
        return Err(invalid(
            "Original range must be nonempty and inside its qualified picture index",
        ));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn validate_original(
    _: &Connection,
    _: &ProjectDocument,
    _: &AssetId,
    _: &SourceQualificationId,
    _: &Range<u64>,
) -> Result<(), StoreError> {
    Err(StoreError::SourceAdmissionUnavailable)
}

struct CanonicalContent {
    value: Arc<RegisterValue>,
    json: Vec<u8>,
    // Existing content was semantically validated while reading this same
    // transaction. New checkpoint-backed contents are validated before writing.
    validated: bool,
}

pub(crate) struct PreparedBank {
    pub bank: RegisterBank,
    contents: BTreeMap<String, CanonicalContent>,
    slots: BTreeMap<RegisterName, String>,
}

fn prepare_copy(
    connection: &Connection,
    expected_project: &ProjectId,
    expected_revision: &RevisionId,
    expected_bank_version: u64,
    name: RegisterName,
    value: RegisterValue,
) -> Result<PreparedBank, StoreError> {
    if matches!(value, RegisterValue::Macro { .. }) {
        return Err(invalid("macros are saved through save_macro"));
    }
    let current = crate::read_head_project(connection)?;
    if current.project_id() != expected_project {
        return Err(invalid("copy belongs to another project"));
    }
    if current.revision_id() != expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: expected_revision.as_str().into(),
            current: current.revision_id().as_str().into(),
        });
    }
    let prepared = prepare_bank(connection)?;
    if prepared.bank.version != expected_bank_version {
        return Err(invalid("register bank version changed"));
    }
    let value = Arc::new(value);
    let writes = BTreeMap::from([(RegisterName::unnamed(), Arc::clone(&value)), (name, value)]);
    prepare_writes_from(prepared, &writes)
}

fn prepare_macro(
    connection: &Connection,
    expected_project: &ProjectId,
    expected_revision: &RevisionId,
    expected_bank_version: u64,
    name: RegisterName,
    program: Arc<SemanticProgram>,
) -> Result<PreparedBank, StoreError> {
    let current = crate::read_head_project(connection)?;
    if current.project_id() != expected_project {
        return Err(invalid("macro belongs to another project"));
    }
    if current.revision_id() != expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: expected_revision.as_str().into(),
            current: current.revision_id().as_str().into(),
        });
    }
    let prepared = prepare_bank(connection)?;
    if prepared.bank.version != expected_bank_version {
        return Err(invalid("register bank version changed"));
    }
    prepare_writes_from(
        prepared,
        &BTreeMap::from([(name, Arc::new(RegisterValue::Macro { program }))]),
    )
}

fn write_register(
    connection: &Connection,
    name: RegisterName,
    value: RegisterValue,
    project: &ProjectId,
) -> Result<RegisterBank, StoreError> {
    let value = Arc::new(value);
    let writes = if matches!(value.as_ref(), RegisterValue::Macro { .. }) {
        BTreeMap::from([(name, value)])
    } else {
        BTreeMap::from([(RegisterName::unnamed(), Arc::clone(&value)), (name, value)])
    };
    let prepared = prepare_writes(connection, &writes)?;
    write_prepared_bank(connection, &prepared, project)?;
    Ok(prepared.bank)
}

pub(crate) fn prepare_bank(connection: &Connection) -> Result<PreparedBank, StoreError> {
    let (bank, contents) = read_bank_contents(connection)?;
    canonical_contents(bank, contents)
}

pub(crate) fn prepare_writes(
    connection: &Connection,
    writes: &BTreeMap<RegisterName, Arc<RegisterValue>>,
) -> Result<PreparedBank, StoreError> {
    prepare_writes_from(prepare_bank(connection)?, writes)
}

pub(crate) fn prepare_writes_from(
    prepared: PreparedBank,
    writes: &BTreeMap<RegisterName, Arc<RegisterValue>>,
) -> Result<PreparedBank, StoreError> {
    if writes.is_empty() {
        return Ok(prepared);
    }
    let PreparedBank {
        mut bank, contents, ..
    } = prepared;
    bank.version = bank
        .version
        .checked_add(1)
        .filter(|version| *version <= i64::MAX as u64)
        .ok_or_else(|| invalid("register versions are exhausted"))?;
    bank.entries.extend(
        writes
            .iter()
            .map(|(name, value)| (*name, Arc::clone(value))),
    );
    canonical_contents(bank, contents)
}

fn canonical_contents(
    mut bank: RegisterBank,
    mut contents: BTreeMap<String, CanonicalContent>,
) -> Result<PreparedBank, StoreError> {
    // At most 27 slots. Arc identity avoids repeatedly serializing aliases;
    // canonical bytes also deduplicate independently allocated equal values.
    let mut known: Vec<_> = contents
        .iter()
        .map(|(id, content)| (Arc::clone(&content.value), id.clone()))
        .collect();
    let mut slots = BTreeMap::new();
    for (name, value) in &mut bank.entries {
        if let RegisterValue::Macro { program } = value.as_ref() {
            if *name == RegisterName::unnamed() {
                return Err(invalid("macros require a named register a-z"));
            }
            program.validate()?;
        }
        let id = if let Some((_, id)) = known.iter().find(|(seen, _)| Arc::ptr_eq(seen, value)) {
            id.clone()
        } else {
            let json = canonical(value)?;
            let id = digest(&json);
            if let Some(existing) = contents.get(&id) {
                if existing.json != json || existing.value.as_ref() != value.as_ref() {
                    return Err(invalid("register content hash collision"));
                }
            } else {
                contents.insert(
                    id.clone(),
                    CanonicalContent {
                        value: Arc::clone(value),
                        json,
                        validated: false,
                    },
                );
            }
            known.push((Arc::clone(value), id.clone()));
            id
        };
        *value = Arc::clone(&contents[&id].value);
        slots.insert(*name, id);
    }
    contents.retain(|id, _| slots.values().any(|used| used == id));
    let mut bytes = 0_usize;
    for content in contents.values() {
        bytes = bytes
            .checked_add(content.json.len())
            .filter(|bytes| *bytes <= MAX_REGISTER_BYTES)
            .ok_or_else(|| invalid("register contents exceed the aggregate 64 MiB limit"))?;
    }
    Ok(PreparedBank {
        bank,
        contents,
        slots,
    })
}

pub(crate) fn write_prepared_bank(
    connection: &Connection,
    prepared: &PreparedBank,
    project: &ProjectId,
) -> Result<(), StoreError> {
    // Checkpoints are inserted first, inside the same transaction. Each unique
    // new payload is validated once; aliases reuse its prepared ID and bytes.
    for (id, content) in &prepared.contents {
        if !content.validated {
            validate_value(connection, &content.value, project)?;
        }
        let (revision, step) = capture_columns(connection, &content.value)?;
        connection.execute(
            "INSERT INTO register_contents(id,capture_revision,capture_step,value) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO NOTHING",
            params![id, revision, step, std::str::from_utf8(&content.json).map_err(|_| invalid("register JSON is not UTF-8"))?],
        )?;
        let same: bool = connection.query_row(
            "SELECT capture_revision IS ?2 AND capture_step IS ?3 AND CAST(value AS BLOB)=?4 FROM register_contents WHERE id=?1",
            params![id, revision, step, content.json], |r| r.get(0),
        )?;
        if !same {
            return Err(invalid("register content hash collision"));
        }
    }
    connection.execute("DELETE FROM registers", [])?;
    for (name, id) in &prepared.slots {
        connection.execute(
            "INSERT INTO registers(name,content_id) VALUES(?1,?2)",
            params![name.as_char().to_string(), id],
        )?;
    }
    connection.execute(
        "DELETE FROM register_contents WHERE id NOT IN (SELECT content_id FROM registers)",
        [],
    )?;
    let digest = bank_digest(
        prepared.bank.version,
        prepared
            .slots
            .iter()
            .map(|(name, id)| (name.as_char(), id.as_str())),
    );
    connection.execute(
        "UPDATE register_state SET version=?1,bank_digest=?2 WHERE singleton=1",
        params![
            i64::try_from(prepared.bank.version)
                .map_err(|_| invalid("register versions are exhausted"))?,
            digest
        ],
    )?;
    check_stored_sizes(connection)
}

fn capture_columns(
    connection: &Connection,
    value: &RegisterValue,
) -> Result<(Option<String>, Option<String>), StoreError> {
    match value.capture_revision() {
        Some(revision) => crate::compound::capture_columns(connection, revision),
        None => Ok((None, None)),
    }
}

fn canonical(value: &RegisterValue) -> Result<Vec<u8>, StoreError> {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > MAX_REGISTER_BYTES {
                return Err(std::io::Error::other("register content exceeds 64 MiB"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded(Vec::new());
    serde_json::to_writer(&mut output, value)?;
    Ok(output.0)
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn invalid(message: &str) -> StoreError {
    StoreError::Registers(message.into())
}

#[cfg(test)]
mod tests;
