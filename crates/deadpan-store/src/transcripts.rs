//! Durable local transcripts of a project's Original audio.
//!
//! Transcripts are analysis annotations keyed by what produced them: the
//! Original's content identity, its qualified audio stream, the model hash,
//! the language and the engine. They live outside document history, so saving
//! one never creates a revision or an Undo step, and every read revalidates the
//! stored transcript before use.

use deadpan_analysis::Transcript;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{ProjectStore, StoreError};

/// Large enough for the analysis word limit; checked before parsing.
pub const MAX_TRANSCRIPT_JSON_BYTES: usize = 48 * 1024 * 1024;
/// Retained transcripts per project (models and languages tried).
pub const MAX_TRANSCRIPTS: i64 = 32;
const MAX_KEY_TEXT_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptKey {
    /// The Original's content identity.
    pub content: String,
    /// The qualified audio stream within the Original container.
    pub audio_stream: u32,
    pub model_sha256: String,
    /// `auto` or an ISO 639-1 code.
    pub language: String,
    /// Recognizer engine and version.
    pub engine: String,
}

impl TranscriptKey {
    fn validate(&self) -> Result<(), StoreError> {
        let text_ok = |value: &str| {
            !value.is_empty()
                && value.len() <= MAX_KEY_TEXT_BYTES
                && !value.chars().any(char::is_control)
        };
        if !text_ok(&self.content) || !text_ok(&self.language) || !text_ok(&self.engine) {
            return Err(invalid(
                "transcript key text is empty, oversized or has controls",
            ));
        }
        if self.model_sha256.len() != 64
            || !self
                .model_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(invalid("transcript model hash must be lowercase SHA-256"));
        }
        Ok(())
    }
}

impl ProjectStore {
    /// Save or replace the transcript for its key.
    pub fn save_transcript(
        &self,
        key: &TranscriptKey,
        transcript: &Transcript,
    ) -> Result<(), StoreError> {
        self.require_writer()?;
        key.validate()?;
        let value = serde_json::to_string(transcript)?;
        if value.len() > MAX_TRANSCRIPT_JSON_BYTES {
            return Err(invalid("transcript exceeds its stored size"));
        }
        let transaction = self.connection.unchecked_transaction()?;
        let existing: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM transcripts WHERE content=?1 AND audio_stream=?2
                AND model_sha256=?3 AND language=?4 AND engine=?5)",
            params![
                key.content,
                key.audio_stream,
                key.model_sha256,
                key.language,
                key.engine
            ],
            |row| row.get(0),
        )?;
        let count: i64 =
            transaction.query_row("SELECT count(*) FROM transcripts", [], |row| row.get(0))?;
        if !existing && count >= MAX_TRANSCRIPTS {
            return Err(invalid("project already retains its maximum transcripts"));
        }
        transaction.execute(
            "INSERT INTO transcripts(content,audio_stream,model_sha256,language,engine,value)
                VALUES(?1,?2,?3,?4,?5,?6)
                ON CONFLICT(content,audio_stream,model_sha256,language,engine)
                DO UPDATE SET value=excluded.value",
            params![
                key.content,
                key.audio_stream,
                key.model_sha256,
                key.language,
                key.engine,
                value
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// The transcript stored for exactly this key, revalidated.
    pub fn transcript(&self, key: &TranscriptKey) -> Result<Option<Transcript>, StoreError> {
        key.validate()?;
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM transcripts WHERE content=?1 AND audio_stream=?2
                    AND model_sha256=?3 AND language=?4 AND engine=?5",
                params![
                    key.content,
                    key.audio_stream,
                    key.model_sha256,
                    key.language,
                    key.engine
                ],
                |row| row.get(0),
            )
            .optional()?;
        value.map(|value| parse(&value)).transpose()
    }

    /// The keys stored for one Original, ordered, without parsing transcripts.
    pub fn transcript_keys_for_content(
        &self,
        content: &str,
    ) -> Result<Vec<TranscriptKey>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT content,audio_stream,model_sha256,language,engine FROM transcripts
                WHERE content=?1 ORDER BY audio_stream,model_sha256,language,engine",
        )?;
        let rows = statement.query_map(params![content], |row| {
            Ok(TranscriptKey {
                content: row.get(0)?,
                audio_stream: row.get(1)?,
                model_sha256: row.get(2)?,
                language: row.get(3)?,
                engine: row.get(4)?,
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

    /// Every stored transcript of one Original, ordered by key.
    pub fn transcripts_for_content(
        &self,
        content: &str,
    ) -> Result<Vec<(TranscriptKey, Transcript)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT content,audio_stream,model_sha256,language,engine,value FROM transcripts
                WHERE content=?1 ORDER BY audio_stream,model_sha256,language,engine",
        )?;
        let rows = statement.query_map(params![content], |row| {
            Ok((
                TranscriptKey {
                    content: row.get(0)?,
                    audio_stream: row.get(1)?,
                    model_sha256: row.get(2)?,
                    language: row.get(3)?,
                    engine: row.get(4)?,
                },
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut transcripts = Vec::new();
        for row in rows {
            let (key, value) = row?;
            key.validate()?;
            transcripts.push((key, parse(&value)?));
        }
        Ok(transcripts)
    }
}

fn parse(value: &str) -> Result<Transcript, StoreError> {
    if value.len() > MAX_TRANSCRIPT_JSON_BYTES {
        return Err(invalid("stored transcript exceeds its size"));
    }
    serde_json::from_str(value).map_err(|error| invalid(&format!("stored transcript: {error}")))
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE transcripts (
            content TEXT NOT NULL,
            audio_stream INTEGER NOT NULL CHECK(audio_stream BETWEEN 0 AND 4294967295),
            model_sha256 TEXT NOT NULL CHECK(length(model_sha256)=64),
            language TEXT NOT NULL,
            engine TEXT NOT NULL,
            value TEXT NOT NULL CHECK(json_valid(value)),
            PRIMARY KEY(content,audio_stream,model_sha256,language,engine)
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let count: i64 =
        connection.query_row("SELECT count(*) FROM transcripts", [], |row| row.get(0))?;
    if count > MAX_TRANSCRIPTS {
        return Err(invalid("too many stored transcripts"));
    }
    let malformed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM transcripts WHERE
            length(CAST(content AS BLOB)) NOT BETWEEN 1 AND ?1
            OR length(CAST(language AS BLOB)) NOT BETWEEN 1 AND ?1
            OR length(CAST(engine AS BLOB)) NOT BETWEEN 1 AND ?1
            OR model_sha256 GLOB '*[^0-9a-f]*'
            OR length(CAST(value AS BLOB)) NOT BETWEEN 1 AND ?2)",
        params![MAX_KEY_TEXT_BYTES as i64, MAX_TRANSCRIPT_JSON_BYTES as i64],
        |row| row.get(0),
    )?;
    if malformed {
        return Err(invalid("invalid stored transcript field"));
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
