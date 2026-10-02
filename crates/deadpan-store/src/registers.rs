//! Project-owned copies, independent of the timeline's undo cursor.
//!
//! Slots refer to canonical, content-addressed values. A named write also writes
//! the unnamed slot. A cut and both slot writes share the timeline transaction.

use std::{collections::BTreeMap, io::Write, ops::Range, sync::Arc};

use deadpan_core::{
    AssetId, CapturedEditSlice, Command, CommandRequest, ProjectDocument, ProjectId, RevisionId,
    SliceCaptureSelection, SourceQualificationId,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CommitOutcome, ProjectStore, StoreError, generation::RelevancePlan};

/// Total canonical bytes retained by distinct register values, including metadata.
pub const MAX_REGISTER_BYTES: usize = 64 * 1024 * 1024;
const MAX_SLOTS: i64 = 27;

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

    /// Copy without creating a timeline revision or changing Undo/Redo.
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
        let current = crate::read_snapshot(&transaction)?;
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
        let plan = crate::prepare_command(&transaction, request)?;
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
        let outcome = crate::write_command_plan(&transaction, plan, relevance)?;
        transaction.commit()?;
        Ok((outcome, bank))
    }
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE register_state (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            version INTEGER NOT NULL CHECK(version>=0)
        ) STRICT;
        INSERT INTO register_state VALUES(1,0);
        CREATE TABLE register_contents (
            id TEXT PRIMARY KEY,
            capture_revision TEXT REFERENCES revisions(id),
            capture_step TEXT REFERENCES transaction_steps(step_revision),
            value TEXT NOT NULL CHECK(json_valid(value)),
            CHECK ((capture_revision IS NULL) != (capture_step IS NULL))
        ) STRICT;
        CREATE TABLE registers (
            name TEXT PRIMARY KEY CHECK(length(CAST(name AS BLOB))=1 AND (name='\"' OR name GLOB '[a-z]')),
            content_id TEXT NOT NULL REFERENCES register_contents(id)
        ) STRICT;",
    )?;
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
        "SELECT EXISTS(SELECT 1 FROM register_state WHERE singleton!=1 OR typeof(version)!='integer' OR version<0)
        OR EXISTS(SELECT 1 FROM registers WHERE typeof(name)!='text' OR length(CAST(name AS BLOB))!=1
            OR NOT (name='\"' OR name GLOB '[a-z]') OR typeof(content_id)!='text'
            OR length(CAST(content_id AS BLOB))!=64 OR content_id GLOB '*[^0-9a-f]*')
        OR EXISTS(SELECT 1 FROM register_contents WHERE typeof(id)!='text' OR length(CAST(id AS BLOB))!=64
            OR id GLOB '*[^0-9a-f]*'
            OR ((capture_revision IS NULL) = (capture_step IS NULL))
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
    let inconsistent: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM register_contents c WHERE NOT EXISTS(SELECT 1 FROM registers r WHERE r.content_id=c.id))
        OR EXISTS(SELECT 1 FROM registers r WHERE NOT EXISTS(SELECT 1 FROM register_contents c WHERE c.id=r.content_id))
        OR EXISTS(SELECT 1 FROM register_contents c WHERE c.capture_revision IS NOT NULL AND NOT EXISTS(SELECT 1 FROM revisions v WHERE v.id=c.capture_revision))
        OR EXISTS(SELECT 1 FROM register_contents c WHERE c.capture_step IS NOT NULL AND NOT EXISTS(SELECT 1 FROM transaction_steps s WHERE s.step_revision=c.capture_step AND s.document IS NOT NULL))
        OR (EXISTS(SELECT 1 FROM registers) AND NOT EXISTS(SELECT 1 FROM registers WHERE name='\"'))
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
    let project = crate::read_snapshot(connection)?.project_id().clone();
    let version: i64 = connection.query_row(
        "SELECT version FROM register_state WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    let version = u64::try_from(version).map_err(|_| invalid("invalid register version"))?;
    let mut values = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT id,coalesce(capture_revision,capture_step),value FROM register_contents ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let revision: String = row.get(1)?;
        let json: String = row.get(2)?;
        if digest(json.as_bytes()) != id {
            return Err(invalid("register content hash disagrees with its bytes"));
        }
        let value: RegisterValue = serde_json::from_str(&json)?;
        if value.revision().as_str() != revision || canonical(&value)? != json.as_bytes() {
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
    }
    Ok((RegisterBank { version, entries }, values))
}

pub(crate) fn validate_value(
    connection: &Connection,
    value: &RegisterValue,
    project: &ProjectId,
) -> Result<(), StoreError> {
    let captured = crate::compound::read_capture(connection, value.revision())?;
    validate_value_at(connection, value, project, &captured)
}

pub(crate) fn validate_value_at(
    connection: &Connection,
    value: &RegisterValue,
    project: &ProjectId,
    captured: &ProjectDocument,
) -> Result<(), StoreError> {
    if value.revision() != captured.revision_id() {
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

fn write_register(
    connection: &Connection,
    name: RegisterName,
    value: RegisterValue,
    project: &ProjectId,
) -> Result<RegisterBank, StoreError> {
    let value = Arc::new(value);
    let writes = BTreeMap::from([(RegisterName::unnamed(), Arc::clone(&value)), (name, value)]);
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
        let (revision, step) =
            crate::compound::capture_columns(connection, content.value.revision())?;
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
    connection.execute(
        "UPDATE register_state SET version=?1 WHERE singleton=1",
        [i64::try_from(prepared.bank.version)
            .map_err(|_| invalid("register versions are exhausted"))?],
    )?;
    check_stored_sizes(connection)
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
