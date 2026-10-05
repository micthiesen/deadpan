//! Private provenance of an Original acquired from a remote service.
//!
//! Provenance records where retained original bytes came from: the service,
//! its normalized source identifier and URL, untrusted display metadata, the
//! retrieval time and the exact helper versions and selected formats. Like
//! analysis tables it is operational metadata outside document history, so
//! saving it never creates a revision or an Undo step. Rows are keyed by the
//! retained original's content identity, which must already be retained.
//! Credentials, cookies and request headers never belong here.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::original_media::OriginalContentId;
use crate::{ProjectStore, StoreError};

pub const PROVENANCE_SCHEMA: u32 = 1;
/// Encoded bound for one provenance record.
pub const MAX_PROVENANCE_BYTES: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_TOOLS: usize = 8;
const MAX_FORMATS: usize = 8;
/// Matches the original-media inventory bound.
const MAX_ROWS: i64 = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperVersion {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatRole {
    Video,
    Audio,
    Combined,
}

/// One selected remote stream, as the service described it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedFormat {
    pub role: FormatRole,
    pub format_id: String,
    pub container: Option<String>,
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Frames per second times 1000, when declared.
    pub fps_millis: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    /// Exact downloaded byte length.
    pub byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteOriginalProvenance {
    pub schema: u32,
    /// Service identifier, for example `youtube`.
    pub service: String,
    /// Normalized service identifier of the source.
    pub source_id: String,
    /// Canonical source URL derived from `source_id`, not the user's input.
    pub source_url: String,
    pub title: String,
    pub author: Option<String>,
    pub author_id: Option<String>,
    pub author_url: Option<String>,
    pub license: Option<String>,
    /// Service-reported upload date, as given (YYYYMMDD for YouTube).
    pub upload_date: Option<String>,
    pub duration_millis: Option<u64>,
    pub thumbnail_url: Option<String>,
    /// Seconds since the Unix epoch when the bytes were retrieved.
    pub retrieved_at_unix_seconds: u64,
    pub helpers: Vec<HelperVersion>,
    pub formats: Vec<SelectedFormat>,
    /// How selected formats became one container, for example a stream-copy remux.
    pub assembly: String,
}

impl RemoteOriginalProvenance {
    pub fn validate(&self) -> Result<(), StoreError> {
        let text = |value: &str| {
            !value.is_empty()
                && value.len() <= MAX_TEXT_BYTES
                && !value.chars().any(char::is_control)
        };
        let optional = |value: &Option<String>| value.as_deref().is_none_or(text);
        if self.schema != PROVENANCE_SCHEMA {
            return Err(invalid("unsupported provenance schema"));
        }
        if ![
            &self.service,
            &self.source_id,
            &self.source_url,
            &self.title,
            &self.assembly,
        ]
        .into_iter()
        .all(|value| text(value))
            || ![
                &self.author,
                &self.author_id,
                &self.author_url,
                &self.license,
                &self.upload_date,
                &self.thumbnail_url,
            ]
            .into_iter()
            .all(optional)
        {
            return Err(invalid(
                "provenance text is empty, oversized or has control characters",
            ));
        }
        if !self.source_url.starts_with("https://") {
            return Err(invalid("provenance source URL must use HTTPS"));
        }
        if self.helpers.is_empty()
            || self.helpers.len() > MAX_TOOLS
            || self.formats.is_empty()
            || self.formats.len() > MAX_FORMATS
        {
            return Err(invalid("provenance helper or format count is out of range"));
        }
        if !self
            .helpers
            .iter()
            .all(|helper| text(&helper.name) && text(&helper.version))
            || !self.formats.iter().all(|format| {
                text(&format.format_id)
                    && optional(&format.container)
                    && optional(&format.codec)
                    && format.byte_length > 0
            })
        {
            return Err(invalid("invalid provenance helper or format"));
        }
        if serde_json::to_vec(self)?.len() > MAX_PROVENANCE_BYTES {
            return Err(invalid("provenance exceeds its size bound"));
        }
        Ok(())
    }
}

impl ProjectStore {
    /// Save or replace provenance for a retained original.
    pub fn save_original_provenance(
        &self,
        content: &OriginalContentId,
        provenance: &RemoteOriginalProvenance,
    ) -> Result<(), StoreError> {
        self.require_writer()?;
        provenance.validate()?;
        let encoded = serde_json::to_string(provenance)?;
        let transaction = self.connection.unchecked_transaction()?;
        let retained: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM original_media WHERE content_id=?1)",
            [content.to_string()],
            |row| row.get(0),
        )?;
        if !retained {
            return Err(invalid("provenance names an original that is not retained"));
        }
        transaction.execute(
            "INSERT INTO original_provenance(content_id,record) VALUES(?1,?2)
             ON CONFLICT(content_id) DO UPDATE SET record=excluded.record",
            params![content.to_string(), encoded],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn original_provenance(
        &self,
        content: &OriginalContentId,
    ) -> Result<Option<RemoteOriginalProvenance>, StoreError> {
        if !has_table(&self.connection)? {
            return Ok(None);
        }
        let encoded: Option<String> = self
            .connection
            .query_row(
                "SELECT record FROM original_provenance WHERE content_id=?1",
                [content.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        encoded.map(|encoded| decode(&encoded)).transpose()
    }
}

fn decode(encoded: &str) -> Result<RemoteOriginalProvenance, StoreError> {
    if encoded.len() > MAX_PROVENANCE_BYTES {
        return Err(invalid("stored provenance exceeds its size bound"));
    }
    let provenance: RemoteOriginalProvenance = serde_json::from_str(encoded)?;
    provenance.validate()?;
    Ok(provenance)
}

/// A read-only open of an older package has no table until a writer upgrades it.
pub(crate) fn table_exists(connection: &Connection) -> Result<bool, StoreError> {
    has_table(connection)
}

fn has_table(connection: &Connection) -> Result<bool, StoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='original_provenance')",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE original_provenance (
            content_id TEXT PRIMARY KEY REFERENCES original_media(content_id),
            record TEXT NOT NULL CHECK(json_valid(record))
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    if !has_table(connection)? {
        return Ok(());
    }
    let count: i64 =
        connection.query_row("SELECT count(*) FROM original_provenance", [], |row| {
            row.get(0)
        })?;
    let oversized: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM original_provenance WHERE length(CAST(record AS BLOB)) > ?1
            OR length(CAST(content_id AS BLOB)) > 71)",
        [MAX_PROVENANCE_BYTES as i64],
        |row| row.get(0),
    )?;
    if count > MAX_ROWS || oversized {
        return Err(invalid("stored provenance exceeds its bounds"));
    }
    let mut statement = connection.prepare("SELECT record FROM original_provenance")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        decode(&row.get::<_, String>(0)?)?;
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
