//! Named Original moments: tagged half-open picture ranges on the project's
//! one Original clock. Operational metadata outside authored history; no
//! catalog write edits the timeline or touches Undo.

use std::{collections::BTreeSet, fmt};

use deadpan_core::{ProjectId, RevisionId};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ProjectStore, StoreError};

pub const MAX_MOMENTS: usize = 1024;
pub const MAX_MOMENT_NAME_BYTES: usize = 128;
pub const MAX_MOMENT_TAGS: usize = 16;
pub const MAX_MOMENT_TAG_BYTES: usize = 32;
/// Bound on the stored JSON tag list: 16 quoted 32-byte tags and separators.
const MAX_TAGS_JSON_BYTES: usize = 2 + MAX_MOMENT_TAGS * (MAX_MOMENT_TAG_BYTES * 6 + 3);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MomentId(String);
impl MomentId {
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        RevisionId::new(value.clone()).map_err(|_| {
            invalid("moment IDs require 1–128 ASCII letters, digits, hyphens or underscores")
        })?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for MomentId {
    type Error = StoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<MomentId> for String {
    fn from(value: MomentId) -> Self {
        value.0
    }
}
impl fmt::Display for MomentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MomentName(String);
impl MomentName {
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_MOMENT_NAME_BYTES
            || value.trim() != value
            || value
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            return Err(invalid(
                "moment names require 1–128 UTF-8 bytes, no surrounding whitespace, line breaks or control characters",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for MomentName {
    type Error = StoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<MomentName> for String {
    fn from(value: MomentName) -> Self {
        value.0
    }
}
impl fmt::Display for MomentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One editorial label, already lowercase. Commas separate tags in commands
/// and whitespace separates filter terms, so neither can occur inside one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MomentTag(String);
impl MomentTag {
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_MOMENT_TAG_BYTES
            || value.to_lowercase() != value
            || value
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == ',')
        {
            return Err(invalid(
                "moment tags require 1–32 lowercase UTF-8 bytes without spaces, commas or control characters",
            ));
        }
        Ok(Self(value))
    }
    /// Lowercase a typed tag before validating it.
    pub fn normalize(value: &str) -> Result<Self, StoreError> {
        Self::new(value.trim().to_lowercase())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for MomentTag {
    type Error = StoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<MomentTag> for String {
    fn from(value: MomentTag) -> Self {
        value.0
    }
}
impl fmt::Display for MomentTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// At most 16 distinct tags in sorted order, so equal sets compare equal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<MomentTag>", into = "Vec<MomentTag>")]
pub struct MomentTags(Vec<MomentTag>);
impl MomentTags {
    pub fn new(tags: Vec<MomentTag>) -> Result<Self, StoreError> {
        if tags.len() > MAX_MOMENT_TAGS || !tags.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(invalid(
                "moment tags must be at most 16 distinct tags in sorted order",
            ));
        }
        Ok(Self(tags))
    }
    /// Normalize typed tags: lowercase, deduplicate and sort.
    pub fn from_input<'a>(tags: impl IntoIterator<Item = &'a str>) -> Result<Self, StoreError> {
        let tags = tags
            .into_iter()
            .map(MomentTag::normalize)
            .collect::<Result<BTreeSet<_>, _>>()?;
        Self::new(tags.into_iter().collect())
    }
    pub fn as_slice(&self) -> &[MomentTag] {
        &self.0
    }
    pub fn contains(&self, tag: &str) -> bool {
        self.0.iter().any(|known| known.as_str() == tag)
    }
}
impl TryFrom<Vec<MomentTag>> for MomentTags {
    type Error = StoreError;
    fn try_from(value: Vec<MomentTag>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<MomentTags> for Vec<MomentTag> {
    fn from(value: MomentTags) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Moment {
    pub id: MomentId,
    pub name: MomentName,
    pub tags: MomentTags,
    /// Half-open Original picture ordinals `[start, end)`.
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MomentCatalog {
    pub project_id: ProjectId,
    pub version: u64,
    /// Stable ID order, independent of names and updates.
    pub entries: Vec<Moment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub project_id: ProjectId,
    pub expected_version: u64,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Create {
        id: MomentId,
        name: MomentName,
        #[serde(default)]
        tags: MomentTags,
        start: u64,
        end: u64,
    },
    Rename {
        id: MomentId,
        name: MomentName,
    },
    Retag {
        id: MomentId,
        tags: MomentTags,
    },
    Delete {
        id: MomentId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub catalog: MomentCatalog,
    pub changed: bool,
}

impl ProjectStore {
    pub fn moment_catalog(&self) -> Result<MomentCatalog, StoreError> {
        self.refuse_newer_schema()?;
        let transaction = self.connection.unchecked_transaction()?;
        read_catalog(&transaction)
    }

    /// Validate a request exactly as `apply_moment` does, writing nothing.
    pub fn preview_moment(&self, request: &Request) -> Result<Outcome, StoreError> {
        self.refuse_newer_schema()?;
        let transaction = self.connection.unchecked_transaction()?;
        let (catalog, changed, _) = prepare(&transaction, request)?;
        Ok(Outcome { catalog, changed })
    }

    pub fn apply_moment(&mut self, request: &Request) -> Result<Outcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (catalog, changed, created) = prepare(&transaction, request)?;
        if changed {
            if let Some(id) = &created {
                transaction.execute("INSERT INTO moment_ids(id) VALUES(?1)", [id.as_str()])?;
            }
            write_catalog(&transaction, &catalog)?;
        }
        transaction.commit()?;
        Ok(Outcome { catalog, changed })
    }

    fn refuse_newer_schema(&self) -> Result<(), StoreError> {
        match self.newer_schema() {
            Some(found) => Err(StoreError::NewerSchema {
                found,
                supported: crate::DATABASE_SCHEMA_VERSION,
            }),
            None => Ok(()),
        }
    }
}

fn prepare(
    connection: &Connection,
    request: &Request,
) -> Result<(MomentCatalog, bool, Option<MomentId>), StoreError> {
    let mut catalog = read_catalog(connection)?;
    if catalog.project_id != request.project_id {
        return Err(invalid("moment request belongs to another project"));
    }
    if catalog.version != request.expected_version {
        return Err(invalid(
            "moment catalog version changed; refresh before applying this request",
        ));
    }
    let before = catalog.clone();
    let mut created = None;
    match &request.action {
        Action::Create {
            id,
            name,
            tags,
            start,
            end,
        } => {
            require_available_name(&catalog, name, None)?;
            if catalog.entries.len() >= MAX_MOMENTS {
                return Err(invalid("the project already contains 1024 named moments"));
            }
            let used: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM moment_ids WHERE id=?1)",
                [id.as_str()],
                |row| row.get(0),
            )?;
            if used {
                return Err(invalid("this moment ID has already been used"));
            }
            check_range(*start, *end)?;
            let pictures = original_pictures(connection)?;
            if *end > pictures {
                return Err(invalid(&format!(
                    "the moment ends after the Original's last picture ({pictures} pictures)"
                )));
            }
            catalog.entries.push(Moment {
                id: id.clone(),
                name: name.clone(),
                tags: tags.clone(),
                start: *start,
                end: *end,
            });
            catalog.entries.sort_by(|a, b| a.id.cmp(&b.id));
            created = Some(id.clone());
        }
        Action::Rename { id, .. } | Action::Retag { id, .. } | Action::Delete { id } => {
            let index = catalog
                .entries
                .iter()
                .position(|entry| &entry.id == id)
                .ok_or_else(|| invalid("the selected moment no longer exists"))?;
            match &request.action {
                Action::Rename { name, .. } => {
                    require_available_name(&catalog, name, Some(id))?;
                    catalog.entries[index].name = name.clone();
                }
                Action::Retag { tags, .. } => catalog.entries[index].tags = tags.clone(),
                Action::Delete { .. } => {
                    catalog.entries.remove(index);
                }
                Action::Create { .. } => unreachable!(),
            }
        }
    }
    let changed = before != catalog;
    if changed {
        catalog.version = catalog
            .version
            .checked_add(1)
            .filter(|version| *version <= i64::MAX as u64)
            .ok_or_else(|| invalid("moment catalog version exhausted"))?;
    }
    Ok((catalog, changed, created))
}

fn check_range(start: u64, end: u64) -> Result<(), StoreError> {
    if start >= end || end > i64::MAX as u64 {
        return Err(invalid(
            "a moment needs a nonempty half-open picture range [start, end)",
        ));
    }
    Ok(())
}

/// Picture count of the ready single Original, from its stored measured
/// receipt. Generic projects have no one Original clock to name.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn original_pictures(connection: &Connection) -> Result<u64, StoreError> {
    use crate::single_source::SingleSourceState;
    let Some((SingleSourceState::Ready { qualification, .. }, _)) =
        crate::single_source::read(connection)?
    else {
        return Err(invalid(
            "named moments need a project whose Original is ready",
        ));
    };
    let receipt = crate::source_registration::read_receipt(connection, &qualification)?
        .ok_or_else(|| invalid("the Original's measured receipt is missing"))?;
    let video = receipt
        .snapshot()
        .video()
        .ok_or_else(|| invalid("the Original has no measured pictures"))?;
    u64::try_from(video.index().index().frames().len())
        .map_err(|_| invalid("the Original's picture count is not representable"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn original_pictures(_: &Connection) -> Result<u64, StoreError> {
    Err(invalid("named moments need a measured Original"))
}

fn require_available_name(
    catalog: &MomentCatalog,
    name: &MomentName,
    selected: Option<&MomentId>,
) -> Result<(), StoreError> {
    if catalog
        .entries
        .iter()
        .any(|entry| &entry.name == name && Some(&entry.id) != selected)
    {
        return Err(invalid("another moment already uses this name"));
    }
    Ok(())
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE moment_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1), version INTEGER NOT NULL CHECK(version>=0), digest BLOB NOT NULL CHECK(length(digest)=32)) STRICT;
        CREATE TABLE moment_ids(id TEXT PRIMARY KEY CHECK(length(CAST(id AS BLOB)) BETWEEN 1 AND 128)) STRICT;
        CREATE TABLE moments(id TEXT PRIMARY KEY REFERENCES moment_ids(id), name TEXT NOT NULL UNIQUE CHECK(length(CAST(name AS BLOB)) BETWEEN 1 AND 128), tags TEXT NOT NULL CHECK(json_valid(tags)), start_ordinal INTEGER NOT NULL CHECK(start_ordinal>=0), end_ordinal INTEGER NOT NULL CHECK(end_ordinal>start_ordinal)) STRICT;",
    )?;
    let digest = digest(connection, 0, &[])?;
    connection.execute(
        "INSERT INTO moment_state(singleton,version,digest) VALUES(1,0,?1)",
        [&digest[..]],
    )?;
    Ok(())
}

fn digest(
    connection: &Connection,
    version: u64,
    entries: &[Moment],
) -> Result<[u8; 32], StoreError> {
    let mut hash = Sha256::new();
    hash.update(b"deadpan-moments-1");
    hash.update(serde_json::to_vec(&(version, entries))?);
    // Deleted moments never free their identity.
    let mut statement = connection.prepare("SELECT id FROM moment_ids ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id = MomentId::new(row.get::<_, String>(0)?)?;
        hash.update((id.as_str().len() as u64).to_le_bytes());
        hash.update(id.as_str().as_bytes());
    }
    Ok(hash.finalize().into())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let bad: bool = connection.query_row(
        "SELECT
        (SELECT count(*) FROM moment_state)!=1 OR EXISTS(SELECT 1 FROM moment_state WHERE singleton!=1 OR typeof(version)!='integer' OR version<0 OR typeof(digest)!='blob' OR length(digest)!=32)
        OR (SELECT count(*) FROM moments)>?1
        OR EXISTS(SELECT 1 FROM moments WHERE typeof(id)!='text' OR length(CAST(id AS BLOB)) NOT BETWEEN 1 AND 128 OR typeof(name)!='text' OR length(CAST(name AS BLOB)) NOT BETWEEN 1 AND 128 OR typeof(tags)!='text' OR length(CAST(tags AS BLOB)) NOT BETWEEN 2 AND ?2 OR typeof(start_ordinal)!='integer' OR typeof(end_ordinal)!='integer' OR start_ordinal<0 OR end_ordinal<=start_ordinal)
        OR EXISTS(SELECT 1 FROM moment_ids WHERE typeof(id)!='text' OR length(CAST(id AS BLOB)) NOT BETWEEN 1 AND 128)",
        params![
            MAX_MOMENTS as i64,
            MAX_TAGS_JSON_BYTES as i64,
        ],
        |row| row.get(0),
    )?;
    if bad {
        return Err(invalid("invalid or oversized named moment metadata"));
    }
    Ok(())
}

fn read_catalog(connection: &Connection) -> Result<MomentCatalog, StoreError> {
    check_stored_sizes(connection)?;
    let head = crate::read_head_project(connection)?;
    let (version, stored): (u64, Vec<u8>) = connection.query_row(
        "SELECT version,digest FROM moment_state WHERE singleton=1",
        [],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get(1)?)),
    )?;
    let mut entries = Vec::new();
    let mut names = BTreeSet::new();
    let mut statement = connection.prepare(
        "SELECT m.id,m.name,m.tags,m.start_ordinal,m.end_ordinal,r.id IS NOT NULL FROM moments m LEFT JOIN moment_ids r ON r.id=m.id ORDER BY m.id",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let entry = Moment {
            id: MomentId::new(row.get::<_, String>(0)?)?,
            name: MomentName::new(row.get::<_, String>(1)?)?,
            tags: serde_json::from_str(&row.get::<_, String>(2)?)?,
            start: row.get::<_, i64>(3)? as u64,
            end: row.get::<_, i64>(4)? as u64,
        };
        if !row.get::<_, bool>(5)? {
            return Err(invalid("moment has no permanent identity reservation"));
        }
        if !names.insert(entry.name.clone()) {
            return Err(invalid("named moment catalog contains duplicate names"));
        }
        check_range(entry.start, entry.end)?;
        entries.push(entry);
    }
    if stored != digest(connection, version, &entries)? {
        return Err(invalid(
            "moment catalog digest differs from its stored values",
        ));
    }
    Ok(MomentCatalog {
        project_id: head.project_id().clone(),
        version,
        entries,
    })
}

fn write_catalog(connection: &Connection, catalog: &MomentCatalog) -> Result<(), StoreError> {
    connection.execute("DELETE FROM moments", [])?;
    for entry in &catalog.entries {
        connection.execute(
            "INSERT INTO moments(id,name,tags,start_ordinal,end_ordinal) VALUES(?1,?2,?3,?4,?5)",
            params![
                entry.id.as_str(),
                entry.name.as_str(),
                serde_json::to_string(&entry.tags)?,
                i64::try_from(entry.start).map_err(|_| invalid("moment start is too large"))?,
                i64::try_from(entry.end).map_err(|_| invalid("moment end is too large"))?,
            ],
        )?;
    }
    let digest = digest(connection, catalog.version, &catalog.entries)?;
    connection.execute(
        "UPDATE moment_state SET version=?1,digest=?2 WHERE singleton=1",
        params![
            i64::try_from(catalog.version)
                .map_err(|_| invalid("moment catalog version exhausted"))?,
            &digest[..]
        ],
    )?;
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    read_catalog(connection).map(drop)
}

/// Backup replacement must not free identities or make an old catalog version
/// current again. This runs inside the backup restore transaction.
pub(crate) fn carry_forward(live: &Connection, copy: &Connection) -> Result<(), StoreError> {
    let old = read_catalog(live)?;
    let mut restored = read_catalog(copy)?;
    let mut statement = live.prepare("SELECT id FROM moment_ids ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        copy.execute(
            "INSERT OR IGNORE INTO moment_ids(id) VALUES(?1)",
            [row.get::<_, String>(0)?],
        )?;
    }
    restored.version = old
        .version
        .max(restored.version)
        .checked_add(1)
        .filter(|version| *version <= i64::MAX as u64)
        .ok_or_else(|| invalid("moment catalog version exhausted"))?;
    write_catalog(copy, &restored)
}

fn invalid(message: &str) -> StoreError {
    StoreError::Moments(message.into())
}

#[cfg(test)]
mod tests;
