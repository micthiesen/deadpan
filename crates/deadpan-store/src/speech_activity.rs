//! Durable speech activity of a project's Original audio.
//!
//! Voice activity probabilities and measured energies are analysis
//! annotations keyed by what produced them: the Original's content identity,
//! its qualified audio stream, the detector model hash and the engine. Like
//! transcripts they live outside document history, so saving never creates a
//! revision or an Undo step, and every read revalidates the stored values.

use deadpan_analysis::{ActivityAudio, ENERGY_HOP, MAX_ACTIVITY_SAMPLES, SpeechActivity, VAD_HOP};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{ProjectStore, StoreError};

/// Retained speech activity analyses per project (models and engines tried).
pub const MAX_SPEECH_ACTIVITY: i64 = 32;
const MAX_KEY_TEXT_BYTES: usize = 128;
const MAX_SPEECH_BYTES: u64 = MAX_ACTIVITY_SAMPLES.div_ceil(VAD_HOP);
const MAX_ENERGY_BYTES: u64 = MAX_ACTIVITY_SAMPLES.div_ceil(ENERGY_HOP);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeechActivityKey {
    /// The Original's content identity.
    pub content: String,
    /// The qualified audio stream within the Original container.
    pub audio_stream: u32,
    /// The voice activity detector's model hash.
    pub model_sha256: String,
    /// Detector engine and version.
    pub engine: String,
}

impl SpeechActivityKey {
    fn validate(&self) -> Result<(), StoreError> {
        let text_ok = |value: &str| {
            !value.is_empty()
                && value.len() <= MAX_KEY_TEXT_BYTES
                && !value.chars().any(char::is_control)
        };
        if !text_ok(&self.content) || !text_ok(&self.engine) {
            return Err(invalid(
                "speech activity key text is empty, oversized or has controls",
            ));
        }
        if self.model_sha256.len() != 64
            || !self
                .model_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(invalid(
                "speech activity model hash must be lowercase SHA-256",
            ));
        }
        Ok(())
    }
}

impl ProjectStore {
    /// Save or replace the speech activity for its key.
    pub fn save_speech_activity(
        &self,
        key: &SpeechActivityKey,
        activity: &SpeechActivity,
    ) -> Result<(), StoreError> {
        self.require_writer()?;
        key.validate()?;
        let audio = activity.audio();
        let transaction = self.connection.unchecked_transaction()?;
        let existing: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM speech_activity WHERE content=?1 AND audio_stream=?2
                AND model_sha256=?3 AND engine=?4)",
            params![key.content, key.audio_stream, key.model_sha256, key.engine],
            |row| row.get(0),
        )?;
        let count: i64 =
            transaction.query_row("SELECT count(*) FROM speech_activity", [], |row| row.get(0))?;
        if !existing && count >= MAX_SPEECH_ACTIVITY {
            return Err(invalid(
                "project already retains its maximum speech activity analyses",
            ));
        }
        transaction.execute(
            "INSERT INTO speech_activity(content,audio_stream,model_sha256,engine,
                origin,sample_rate,samples,speech,energy)
                VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                ON CONFLICT(content,audio_stream,model_sha256,engine)
                DO UPDATE SET origin=excluded.origin, sample_rate=excluded.sample_rate,
                    samples=excluded.samples, speech=excluded.speech, energy=excluded.energy",
            params![
                key.content,
                key.audio_stream,
                key.model_sha256,
                key.engine,
                audio.origin,
                audio.sample_rate,
                i64::try_from(audio.samples).map_err(|_| invalid("speech activity length"))?,
                activity.speech(),
                activity.energy()
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// The speech activity stored for exactly this key, revalidated.
    pub fn speech_activity(
        &self,
        key: &SpeechActivityKey,
    ) -> Result<Option<SpeechActivity>, StoreError> {
        key.validate()?;
        if !has_table(&self.connection)? {
            return Ok(None);
        }
        let row = self
            .connection
            .query_row(
                "SELECT origin,sample_rate,samples,speech,energy FROM speech_activity
                    WHERE content=?1 AND audio_stream=?2 AND model_sha256=?3 AND engine=?4",
                params![key.content, key.audio_stream, key.model_sha256, key.engine],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(origin, sample_rate, samples, speech, energy)| {
            let samples = u64::try_from(samples).map_err(|_| invalid("speech activity length"))?;
            SpeechActivity::new(
                ActivityAudio {
                    origin,
                    sample_rate,
                    samples,
                },
                speech,
                energy,
            )
            .map_err(|error| invalid(&format!("stored speech activity: {error}")))
        })
        .transpose()
    }

    /// The keys stored for one Original, ordered, without reading the values.
    pub fn speech_activity_keys_for_content(
        &self,
        content: &str,
    ) -> Result<Vec<SpeechActivityKey>, StoreError> {
        if !has_table(&self.connection)? {
            return Ok(Vec::new());
        }
        let mut statement = self.connection.prepare(
            "SELECT content,audio_stream,model_sha256,engine FROM speech_activity
                WHERE content=?1 ORDER BY audio_stream,model_sha256,engine",
        )?;
        let rows = statement.query_map(params![content], |row| {
            Ok(SpeechActivityKey {
                content: row.get(0)?,
                audio_stream: row.get(1)?,
                model_sha256: row.get(2)?,
                engine: row.get(3)?,
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

/// A read-only open of a schema-59 package has no table until a writer
/// upgrades it; it simply has no stored activity.
fn has_table(connection: &Connection) -> Result<bool, StoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='speech_activity')",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE speech_activity (
            content TEXT NOT NULL,
            audio_stream INTEGER NOT NULL CHECK(audio_stream BETWEEN 0 AND 4294967295),
            model_sha256 TEXT NOT NULL CHECK(length(model_sha256)=64),
            engine TEXT NOT NULL,
            origin INTEGER NOT NULL,
            sample_rate INTEGER NOT NULL CHECK(sample_rate BETWEEN 1 AND 4294967295),
            samples INTEGER NOT NULL CHECK(samples >= 0),
            speech BLOB NOT NULL,
            energy BLOB NOT NULL,
            PRIMARY KEY(content,audio_stream,model_sha256,engine)
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    if !has_table(connection)? {
        return Ok(());
    }
    let count: i64 =
        connection.query_row("SELECT count(*) FROM speech_activity", [], |row| row.get(0))?;
    if count > MAX_SPEECH_ACTIVITY {
        return Err(invalid("too many stored speech activity analyses"));
    }
    let malformed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM speech_activity WHERE
            length(CAST(content AS BLOB)) NOT BETWEEN 1 AND ?1
            OR length(CAST(engine AS BLOB)) NOT BETWEEN 1 AND ?1
            OR model_sha256 GLOB '*[^0-9a-f]*'
            OR samples > ?2
            OR length(speech) > ?3
            OR length(energy) > ?4)",
        params![
            MAX_KEY_TEXT_BYTES as i64,
            MAX_ACTIVITY_SAMPLES as i64,
            MAX_SPEECH_BYTES as i64,
            MAX_ENERGY_BYTES as i64
        ],
        |row| row.get(0),
    )?;
    if malformed {
        return Err(invalid("invalid stored speech activity field"));
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
