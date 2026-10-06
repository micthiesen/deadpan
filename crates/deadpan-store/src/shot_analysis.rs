//! Durable shot analysis of a project's Original pictures.
//!
//! Per-picture visual change measurements are analysis annotations keyed by
//! what produced them: the Original's content identity, its qualified video
//! stream and the picture signature version. Like transcripts and speech
//! activity they live outside document history, so saving never creates a
//! revision or an Undo step, and every read revalidates the stored values
//! against the picture count the caller expects.
//!
//! A scan in progress may save [`ShotProgress`] checkpoints in the separate
//! operational `shot_scan_progress` table under the same key, so a long scan
//! interrupted by quitting continues where it stopped. A checkpoint appends
//! only the measures changed since the previous one ([`ShotProgressTail`]).
//! Saving the final analysis removes the key's progress in the same
//! transaction.
//!
//! Progress is rebuildable and retained for one scan at a time: saving
//! progress removes every other key's progress, and saving any analysis or
//! progress removes progress of content other than a single-Original
//! project's ready Original.

use deadpan_analysis::{
    MAX_SHOT_PICTURES, MEASURE_BYTES, PictureMeasure, ShotAnalysis, ShotProgress, ShotProgressTail,
    decode_measures, encode_measures,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{ProjectStore, StoreError};

/// Retained shot analyses per project (signature versions and streams).
pub const MAX_SHOT_ANALYSES: i64 = 16;
const MAX_KEY_TEXT_BYTES: usize = 128;
/// One measure per picture.
const MAX_CHANGE_BYTES: usize = MAX_SHOT_PICTURES * MEASURE_BYTES;

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
        let changes = encode_measures(analysis.measures());
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
        // The finished analysis supersedes any scan progress of its pictures.
        transaction.execute(
            "DELETE FROM shot_scan_progress WHERE content=?1 AND video_stream=?2",
            params![key.content, key.video_stream],
        )?;
        self.prune_foreign_progress(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    /// Remove scan progress of content other than the ready Original of a
    /// single-Original project, returning that Original's content. Generic
    /// projects keep it.
    fn prune_foreign_progress(
        &self,
        connection: &Connection,
    ) -> Result<Option<String>, StoreError> {
        use crate::single_source::SingleSourceState;
        let Some(SingleSourceState::Ready { qualification, .. }) = self.single_source_state()?
        else {
            return Ok(None);
        };
        let original: String = connection.query_row(
            "SELECT original_content_id FROM source_qualifications WHERE id=?1",
            params![qualification.as_str()],
            |row| row.get(0),
        )?;
        connection.execute(
            "DELETE FROM shot_scan_progress WHERE content<>?1",
            params![original],
        )?;
        Ok(Some(original))
    }

    /// Save or replace a scan checkpoint for its key, outside history. Every
    /// other key's progress is removed.
    pub fn save_shot_scan_progress(
        &self,
        key: &ShotAnalysisKey,
        progress: &ShotProgress,
    ) -> Result<(), StoreError> {
        self.write_progress(key, progress.pictures(), 0, progress.measures())
            .map(|_| ())
    }

    /// Append a checkpoint's changed measures to the key's saved progress,
    /// outside history, returning the first unmeasured picture now saved.
    /// A tail from picture 0 replaces the progress; any other tail needs
    /// saved progress of the same pictures that reaches its start, or it is
    /// refused (the caller then offers the measures again from earlier).
    /// Every other key's progress is removed.
    pub fn append_shot_scan_progress(
        &self,
        key: &ShotAnalysisKey,
        tail: &ShotProgressTail,
    ) -> Result<usize, StoreError> {
        self.write_progress(key, tail.pictures(), tail.start(), tail.measures())
    }

    fn write_progress(
        &self,
        key: &ShotAnalysisKey,
        pictures: usize,
        start: usize,
        measures: &[PictureMeasure],
    ) -> Result<usize, StoreError> {
        self.require_writer()?;
        key.validate()?;
        let count = |value: usize| i64::try_from(value).map_err(|_| invalid("shot picture count"));
        let next = start
            .checked_add(measures.len())
            .filter(|next| *next <= pictures && pictures <= MAX_SHOT_PICTURES)
            .ok_or_else(|| invalid("shot scan progress exceeds its pictures"))?;
        let transaction = self.connection.unchecked_transaction()?;
        if self
            .prune_foreign_progress(&transaction)?
            .is_some_and(|original| original != key.content)
        {
            // Still remove the stale rows.
            transaction.commit()?;
            return Err(invalid(
                "shot scan progress is kept only for the project's Original",
            ));
        }
        // One scan's progress at a time.
        transaction.execute(
            "DELETE FROM shot_scan_progress WHERE NOT (content=?1 AND video_stream=?2
                AND signature_version=?3)",
            params![key.content, key.video_stream, key.signature_version],
        )?;
        let bytes = encode_measures(measures);
        if start == 0 {
            transaction.execute(
                "INSERT INTO shot_scan_progress(content,video_stream,signature_version,pictures,
                    next_picture,measures) VALUES(?1,?2,?3,?4,?5,?6)
                    ON CONFLICT(content,video_stream,signature_version)
                    DO UPDATE SET pictures=excluded.pictures, next_picture=excluded.next_picture,
                        measures=excluded.measures",
                params![
                    key.content,
                    key.video_stream,
                    key.signature_version,
                    count(pictures)?,
                    count(next)?,
                    bytes
                ],
            )?;
        } else {
            // Keep the saved measures before `start`; SQLite joins them with
            // the tail without the caller copying them. `||` yields text, whose
            // bytes the cast keeps unchanged in the UTF-8 database (the store
            // never changes its default encoding).
            let changed = transaction.execute(
                "UPDATE shot_scan_progress SET next_picture=?5,
                    measures=CAST(substr(measures,1,?6) || ?7 AS BLOB)
                    WHERE content=?1 AND video_stream=?2 AND signature_version=?3
                    AND pictures=?4 AND next_picture>=?8 AND length(measures)>=?6",
                params![
                    key.content,
                    key.video_stream,
                    key.signature_version,
                    count(pictures)?,
                    count(next)?,
                    count(start * MEASURE_BYTES)?,
                    bytes,
                    count(start)?
                ],
            )?;
            if changed != 1 {
                return Err(invalid(
                    "shot scan progress tail does not join the saved progress",
                ));
            }
        }
        transaction.commit()?;
        Ok(next)
    }

    /// The scan checkpoint stored for exactly this key, revalidated against
    /// `expected_pictures` (the Original's qualified index length).
    pub fn shot_scan_progress(
        &self,
        key: &ShotAnalysisKey,
        expected_pictures: usize,
    ) -> Result<Option<ShotProgress>, StoreError> {
        key.validate()?;
        let row = self
            .connection
            .query_row(
                "SELECT pictures,next_picture,measures FROM shot_scan_progress
                    WHERE content=?1 AND video_stream=?2 AND signature_version=?3",
                params![key.content, key.video_stream, key.signature_version],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(pictures, next, measures)| {
            let pictures = usize::try_from(pictures).map_err(|_| invalid("shot picture count"))?;
            let next = usize::try_from(next).map_err(|_| invalid("shot progress picture"))?;
            if pictures != expected_pictures {
                return Err(invalid(&format!(
                    "stored shot scan progress has {pictures} pictures, the Original has {expected_pictures}"
                )));
            }
            let measures = decode_measures(&measures)
                .map_err(|error| invalid(&format!("stored shot scan progress: {error}")))?;
            if measures.len() != next {
                return Err(invalid("stored shot progress disagrees with its next picture"));
            }
            ShotProgress::new(pictures, measures)
                .map_err(|error| invalid(&format!("stored shot scan progress: {error}")))
        })
        .transpose()
    }

    /// Remove a key's scan checkpoint, for a scan that starts over.
    pub fn delete_shot_scan_progress(&self, key: &ShotAnalysisKey) -> Result<(), StoreError> {
        self.require_writer()?;
        key.validate()?;
        self.connection.execute(
            "DELETE FROM shot_scan_progress WHERE content=?1 AND video_stream=?2
                AND signature_version=?3",
            params![key.content, key.video_stream, key.signature_version],
        )?;
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
            if changes.len() != pictures * MEASURE_BYTES {
                return Err(invalid("stored shot changes disagree with their picture count"));
            }
            let measures = decode_measures(&changes)
                .map_err(|error| invalid(&format!("stored shot analysis: {error}")))?;
            ShotAnalysis::new(measures)
                .map_err(|error| invalid(&format!("stored shot analysis: {error}")))
        })
        .transpose()
    }

    /// The keys stored for one Original, ordered, without reading the values.
    pub fn shot_analysis_keys_for_content(
        &self,
        content: &str,
    ) -> Result<Vec<ShotAnalysisKey>, StoreError> {
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

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE shot_analysis (
            content TEXT NOT NULL,
            video_stream INTEGER NOT NULL CHECK(video_stream BETWEEN 0 AND 4294967295),
            signature_version TEXT NOT NULL,
            pictures INTEGER NOT NULL CHECK(pictures >= 0),
            changes BLOB NOT NULL,
            PRIMARY KEY(content,video_stream,signature_version)
        ) STRICT;
        CREATE TABLE shot_scan_progress (
            content TEXT NOT NULL,
            video_stream INTEGER NOT NULL CHECK(video_stream BETWEEN 0 AND 4294967295),
            signature_version TEXT NOT NULL,
            pictures INTEGER NOT NULL CHECK(pictures >= 0),
            next_picture INTEGER NOT NULL CHECK(next_picture BETWEEN 0 AND pictures),
            measures BLOB NOT NULL,
            PRIMARY KEY(content,video_stream,signature_version)
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
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
    let count: i64 =
        connection.query_row("SELECT count(*) FROM shot_scan_progress", [], |row| {
            row.get(0)
        })?;
    if count > MAX_SHOT_ANALYSES {
        return Err(invalid("too many stored shot scan progress rows"));
    }
    let malformed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM shot_scan_progress WHERE
            length(CAST(content AS BLOB)) NOT BETWEEN 1 AND ?1
            OR length(CAST(signature_version AS BLOB)) NOT BETWEEN 1 AND ?1
            OR pictures > ?2
            OR length(measures) > ?3)",
        params![
            MAX_KEY_TEXT_BYTES as i64,
            MAX_SHOT_PICTURES as i64,
            MAX_CHANGE_BYTES as i64
        ],
        |row| row.get(0),
    )?;
    if malformed {
        return Err(invalid("invalid stored shot scan progress field"));
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
