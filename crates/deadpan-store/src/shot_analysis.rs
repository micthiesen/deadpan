//! Durable shot analysis of a project's Original pictures.
//!
//! Per-picture visual change measurements are analysis annotations keyed by
//! what produced them: the Original's content identity, its qualified video
//! stream and the picture signature version. Like transcripts and speech
//! activity they live outside document history, so saving never creates a
//! revision or an Undo step, and every read revalidates the stored values
//! against the picture count the caller expects.

use deadpan_analysis::{MAX_SHOT_PICTURES, ShotAnalysis};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{ProjectStore, StoreError};

/// Retained shot analyses per project (signature versions and streams).
pub const MAX_SHOT_ANALYSES: i64 = 16;
const MAX_KEY_TEXT_BYTES: usize = 128;
/// Three change bytes per picture.
const MAX_CHANGE_BYTES: usize = MAX_SHOT_PICTURES * 3;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShotAnalysisKey {
    /// The Original's content identity.
    pub content: String,
    /// The qualified video stream within the Original container.
    pub video_stream: u32,
    /// The picture measurement the changes come from.
    pub signature_version: String,
}

impl ShotAnalysisKey {
    fn validate(&self) -> Result<(), StoreError> {
        let text_ok = |value: &str| {
            !value.is_empty()
                && value.len() <= MAX_KEY_TEXT_BYTES
                && !value.chars().any(char::is_control)
        };
        if !text_ok(&self.content) || !text_ok(&self.signature_version) {
            return Err(invalid(
                "shot analysis key text is empty, oversized or has controls",
            ));
        }
        Ok(())
    }
}

impl ProjectStore {
    /// Save or replace the shot analysis for its key.
    pub fn save_shot_analysis(
        &self,
        key: &ShotAnalysisKey,
        analysis: &ShotAnalysis,
    ) -> Result<(), StoreError> {
        self.require_writer()?;
        key.validate()?;
        let transaction = self.connection.unchecked_transaction()?;
        let existing: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM shot_analysis WHERE content=?1 AND video_stream=?2
                AND signature_version=?3)",
            params![key.content, key.video_stream, key.signature_version],
            |row| row.get(0),
        )?;
        // A newer measurement supersedes older ones of the same pictures.
        transaction.execute(
            "DELETE FROM shot_analysis WHERE content=?1 AND video_stream=?2
                AND signature_version<>?3",
            params![key.content, key.video_stream, key.signature_version],
        )?;
        let count: i64 =
            transaction.query_row("SELECT count(*) FROM shot_analysis", [], |row| row.get(0))?;
        if !existing && count >= MAX_SHOT_ANALYSES {
            return Err(invalid("project already retains its maximum shot analyses"));
        }
        let changes: Vec<u8> = analysis.changes().iter().flatten().copied().collect();
        transaction.execute(
            "INSERT INTO shot_analysis(content,video_stream,signature_version,pictures,changes)
                VALUES(?1,?2,?3,?4,?5)
                ON CONFLICT(content,video_stream,signature_version)
                DO UPDATE SET pictures=excluded.pictures, changes=excluded.changes",
            params![
                key.content,
                key.video_stream,
                key.signature_version,
                i64::try_from(analysis.pictures()).map_err(|_| invalid("shot picture count"))?,
                changes
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// The shot analysis stored for exactly this key, revalidated. A stored
    /// picture count other than `expected_pictures` (the Original's qualified
    /// index length) is an integrity failure, never silently reinterpreted.
    pub fn shot_analysis(
        &self,
        key: &ShotAnalysisKey,
        expected_pictures: usize,
    ) -> Result<Option<ShotAnalysis>, StoreError> {
        key.validate()?;
        if !has_table(&self.connection)? {
            return Ok(None);
        }
        let row = self
            .connection
            .query_row(
                "SELECT pictures,changes FROM shot_analysis
                    WHERE content=?1 AND video_stream=?2 AND signature_version=?3",
                params![key.content, key.video_stream, key.signature_version],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        row.map(|(pictures, changes)| {
            let pictures = usize::try_from(pictures).map_err(|_| invalid("shot picture count"))?;
            if pictures != expected_pictures {
                return Err(invalid(&format!(
                    "stored shot analysis has {pictures} pictures, the Original has {expected_pictures}"
                )));
            }
            if changes.len() != pictures * 3 {
                return Err(invalid("stored shot changes disagree with their picture count"));
            }
            let changes = changes
                .chunks_exact(3)
                .map(|change| [change[0], change[1], change[2]])
                .collect();
            ShotAnalysis::new(changes)
                .map_err(|error| invalid(&format!("stored shot analysis: {error}")))
        })
        .transpose()
    }

    /// The keys stored for one Original, ordered, without reading the values.
    pub fn shot_analysis_keys_for_content(
        &self,
        content: &str,
    ) -> Result<Vec<ShotAnalysisKey>, StoreError> {
        if !has_table(&self.connection)? {
            return Ok(Vec::new());
        }
        let mut statement = self.connection.prepare(
            "SELECT content,video_stream,signature_version FROM shot_analysis
                WHERE content=?1 ORDER BY video_stream,signature_version",
        )?;
        let rows = statement.query_map(params![content], |row| {
            Ok(ShotAnalysisKey {
                content: row.get(0)?,
                video_stream: row.get(1)?,
                signature_version: row.get(2)?,
            })
        })?;
        let mut keys = Vec::new();
        for row in rows {
            let key = row?;
            key.validate()?;
            keys.push(key);
        }
        Ok(keys)
    }
}

/// A read-only open of an older package has no table until a writer
/// upgrades it; it simply has no stored shot analysis.
fn has_table(connection: &Connection) -> Result<bool, StoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='shot_analysis')",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE shot_analysis (
            content TEXT NOT NULL,
            video_stream INTEGER NOT NULL CHECK(video_stream BETWEEN 0 AND 4294967295),
            signature_version TEXT NOT NULL,
            pictures INTEGER NOT NULL CHECK(pictures >= 0),
            changes BLOB NOT NULL,
            PRIMARY KEY(content,video_stream,signature_version)
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    if !has_table(connection)? {
        return Ok(());
    }
    let count: i64 =
        connection.query_row("SELECT count(*) FROM shot_analysis", [], |row| row.get(0))?;
    if count > MAX_SHOT_ANALYSES {
        return Err(invalid("too many stored shot analyses"));
    }
    let malformed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM shot_analysis WHERE
            length(CAST(content AS BLOB)) NOT BETWEEN 1 AND ?1
            OR length(CAST(signature_version AS BLOB)) NOT BETWEEN 1 AND ?1
            OR pictures > ?2
            OR length(changes) > ?3)",
        params![
            MAX_KEY_TEXT_BYTES as i64,
            MAX_SHOT_PICTURES as i64,
            MAX_CHANGE_BYTES as i64
        ],
        |row| row.get(0),
    )?;
    if malformed {
        return Err(invalid("invalid stored shot analysis field"));
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
