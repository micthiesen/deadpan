//! Manual corrections of a project's Original audio analyses.
//!
//! Transcripts and speech activity are rebuildable proposals keyed by the
//! model that produced them. A person's corrections are keyed only by the
//! Original's content identity and audio stream, so they survive
//! transcribing or detecting again, and they live outside document history:
//! correcting a word never creates a revision. Corrections keep their own
//! bounded Undo and Redo stacks of complete earlier values. Every write names
//! the version it expects, so a stale view cannot overwrite a newer
//! correction, and every read revalidates the stored value.

use deadpan_analysis::Corrections;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::{ProjectStore, StoreError};

/// Large enough for the analysis correction limits; checked before parsing.
pub const MAX_CORRECTIONS_JSON_BYTES: usize = 8 * 1024 * 1024;
/// Undo steps retained per Original audio stream.
pub const MAX_CORRECTION_UNDO: i64 = 64;
/// Corrected Original audio streams per project.
pub const MAX_CORRECTED_STREAMS: i64 = 16;
const MAX_KEY_TEXT_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionsKey {
    /// The Original's content identity.
    pub content: String,
    /// The qualified audio stream within the Original container.
    pub audio_stream: u32,
}

impl CorrectionsKey {
    fn validate(&self) -> Result<(), StoreError> {
        if self.content.is_empty()
            || self.content.len() > MAX_KEY_TEXT_BYTES
            || self.content.chars().any(char::is_control)
        {
            return Err(invalid(
                "corrections key text is empty, oversized or has controls",
            ));
        }
        Ok(())
    }
}

/// The current corrections and what Undo and Redo would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCorrections {
    pub corrections: Corrections,
    /// Increases with every change, Undo and Redo; 0 means none stored.
    pub version: u64,
    /// The label of the change Undo reverts.
    pub undo: Option<String>,
    /// The label of the change Redo repeats.
    pub redo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorrectionChange {
    /// Replace the corrections; the earlier value becomes the Undo step.
    Apply {
        corrections: Corrections,
        label: String,
    },
    Undo,
    Redo,
    /// Remove every stored value that no longer parses: the current value
    /// (replaced by no corrections on `clock`) and any Undo or Redo step.
    /// Readable steps stay, so Undo still reaches the last readable state.
    DiscardUnreadable {
        clock: deadpan_analysis::CorrectionClock,
    },
}

/// One stored corrections value that does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableCorrection {
    pub key: CorrectionsKey,
    /// `current`, `undo` or `redo`.
    pub place: &'static str,
    pub error: String,
}

impl ProjectStore {
    /// The stored corrections of one Original audio stream, revalidated.
    pub fn analysis_corrections(
        &self,
        key: &CorrectionsKey,
    ) -> Result<Option<StoredCorrections>, StoreError> {
        key.validate()?;
        read(&self.connection, key)
    }

    /// Apply, undo or redo a correction. `expected_version` is the version
    /// the caller saw (0 when none was stored); a different stored version
    /// refuses the change without writing.
    pub fn change_analysis_corrections(
        &self,
        key: &CorrectionsKey,
        expected_version: u64,
        change: CorrectionChange,
    ) -> Result<StoredCorrections, StoreError> {
        self.require_writer()?;
        key.validate()?;
        let transaction = self.connection.unchecked_transaction()?;
        if let CorrectionChange::DiscardUnreadable { clock } = change {
            let stored = discard_unreadable(&transaction, key, expected_version, clock)?;
            transaction.commit()?;
            return Ok(stored);
        }
        let current = read(&transaction, key)?;
        let version = current.as_ref().map_or(0, |current| current.version);
        if version != expected_version {
            return Err(StoreError::AnalysisCorrections(format!(
                "the corrections changed (version {version}, expected {expected_version})"
            )));
        }
        let next_version =
            i64::try_from(version + 1).map_err(|_| invalid("corrections version overflow"))?;
        match change {
            CorrectionChange::Apply { corrections, label } => {
                check_label(&label)?;
                let value = encode(&corrections)?;
                match &current {
                    Some(current) => {
                        push(
                            &transaction,
                            key,
                            "undo",
                            &label,
                            &encode(&current.corrections)?,
                        )?;
                    }
                    None => {
                        let count: i64 = transaction.query_row(
                            "SELECT count(*) FROM analysis_corrections",
                            [],
                            |row| row.get(0),
                        )?;
                        if count >= MAX_CORRECTED_STREAMS {
                            return Err(invalid(
                                "project already retains its maximum corrected audio streams",
                            ));
                        }
                        transaction.execute(
                            "INSERT INTO analysis_corrections(content,audio_stream,version,value)
                                VALUES(?1,?2,0,?3)",
                            params![key.content, key.audio_stream, value],
                        )?;
                        // The first change's Undo restores no corrections.
                        let none = encode(&Corrections::empty(corrections.clock()))?;
                        push(&transaction, key, "undo", &label, &none)?;
                    }
                }
                transaction.execute(
                    "DELETE FROM analysis_correction_steps
                        WHERE content=?1 AND audio_stream=?2 AND stack='redo'",
                    params![key.content, key.audio_stream],
                )?;
                set(&transaction, key, next_version, &value)?;
            }
            CorrectionChange::DiscardUnreadable { .. } => unreachable!("handled above"),
            CorrectionChange::Undo | CorrectionChange::Redo => {
                let current = current.ok_or_else(|| {
                    StoreError::AnalysisCorrections(
                        "there are no corrections to undo or redo".into(),
                    )
                })?;
                let (from, to) = if change == CorrectionChange::Undo {
                    ("undo", "redo")
                } else {
                    ("redo", "undo")
                };
                let (label, value) = pop(&transaction, key, from)?.ok_or_else(|| {
                    StoreError::AnalysisCorrections(format!("there is nothing to {from}"))
                })?;
                // The restored value must still be valid before it replaces
                // the current one; the transaction is dropped on failure.
                parse(&value).map_err(|error| {
                    StoreError::AnalysisCorrections(format!(
                        "the stored {from} step is unreadable ({error}); discard unreadable corrections to continue"
                    ))
                })?;
                push(
                    &transaction,
                    key,
                    to,
                    &label,
                    &encode(&current.corrections)?,
                )?;
                set(&transaction, key, next_version, &value)?;
            }
        }
        let stored = read(&transaction, key)?
            .ok_or_else(|| invalid("corrections vanished while changing"))?;
        transaction.commit()?;
        Ok(stored)
    }
}

impl ProjectStore {
    /// Every stored corrections value (current, Undo and Redo) that does not
    /// parse. Explicit validation fails on any; opening a project does not,
    /// so the app can offer to discard them.
    pub fn unreadable_analysis_corrections(&self) -> Result<Vec<UnreadableCorrection>, StoreError> {
        audit(&self.connection)
    }
}

pub(crate) fn audit(connection: &Connection) -> Result<Vec<UnreadableCorrection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT content,audio_stream,'current',value FROM analysis_corrections
         UNION ALL
         SELECT content,audio_stream,stack,value FROM analysis_correction_steps",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut unreadable = Vec::new();
    for row in rows {
        let (content, audio_stream, place, value) = row?;
        if let Err(error) = parse(&value) {
            unreadable.push(UnreadableCorrection {
                key: CorrectionsKey {
                    content,
                    audio_stream,
                },
                place: match place.as_str() {
                    "undo" => "undo",
                    "redo" => "redo",
                    _ => "current",
                },
                error: error.to_string(),
            });
        }
    }
    Ok(unreadable)
}

/// Explicit validation: every stored value must parse.
pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    match audit(connection)?.first() {
        Some(bad) => Err(invalid(&format!(
            "unreadable stored corrections ({} value of audio stream {}): {}",
            bad.place, bad.key.audio_stream, bad.error
        ))),
        None => Ok(()),
    }
}

fn discard_unreadable(
    transaction: &Transaction<'_>,
    key: &CorrectionsKey,
    expected_version: u64,
    clock: deadpan_analysis::CorrectionClock,
) -> Result<StoredCorrections, StoreError> {
    let row: Option<(i64, String)> = transaction
        .query_row(
            "SELECT version,value FROM analysis_corrections WHERE content=?1 AND audio_stream=?2",
            params![key.content, key.audio_stream],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((version, value)) = row else {
        return Err(StoreError::AnalysisCorrections(
            "there are no stored corrections to discard".into(),
        ));
    };
    if u64::try_from(version).ok() != Some(expected_version) {
        return Err(StoreError::AnalysisCorrections(format!(
            "the corrections changed (version {version}, expected {expected_version})"
        )));
    }
    let mut steps = transaction.prepare(
        "SELECT stack,position,value FROM analysis_correction_steps
            WHERE content=?1 AND audio_stream=?2",
    )?;
    let bad: Vec<(String, i64)> = steps
        .query_map(params![key.content, key.audio_stream], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .filter_map(|row| match row {
            Ok((stack, position, value)) => parse(&value).is_err().then_some(Ok((stack, position))),
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<_, _>>()?;
    drop(steps);
    let current_bad = parse(&value).is_err();
    if bad.is_empty() && !current_bad {
        return Err(StoreError::AnalysisCorrections(
            "every stored correction is readable; nothing was discarded".into(),
        ));
    }
    for (stack, position) in bad {
        transaction.execute(
            "DELETE FROM analysis_correction_steps
                WHERE content=?1 AND audio_stream=?2 AND stack=?3 AND position=?4",
            params![key.content, key.audio_stream, stack, position],
        )?;
    }
    let next = version
        .checked_add(1)
        .ok_or_else(|| invalid("corrections version overflow"))?;
    let value = if current_bad {
        encode(&Corrections::empty(clock))?
    } else {
        value
    };
    set(transaction, key, next, &value)?;
    read(transaction, key)?.ok_or_else(|| invalid("corrections vanished while discarding"))
}

fn read(
    connection: &Connection,
    key: &CorrectionsKey,
) -> Result<Option<StoredCorrections>, StoreError> {
    let row: Option<(i64, String)> = connection
        .query_row(
            "SELECT version,value FROM analysis_corrections WHERE content=?1 AND audio_stream=?2",
            params![key.content, key.audio_stream],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((version, value)) = row else {
        return Ok(None);
    };
    let label = |stack: &str| -> Result<Option<String>, StoreError> {
        Ok(connection
            .query_row(
                "SELECT label FROM analysis_correction_steps
                    WHERE content=?1 AND audio_stream=?2 AND stack=?3
                    ORDER BY position DESC LIMIT 1",
                params![key.content, key.audio_stream, stack],
                |row| row.get(0),
            )
            .optional()?)
    };
    Ok(Some(StoredCorrections {
        corrections: parse(&value)?,
        version: u64::try_from(version).map_err(|_| invalid("corrections version"))?,
        undo: label("undo")?,
        redo: label("redo")?,
    }))
}

fn set(
    transaction: &Transaction<'_>,
    key: &CorrectionsKey,
    version: i64,
    value: &str,
) -> Result<(), StoreError> {
    transaction.execute(
        "UPDATE analysis_corrections SET version=?3, value=?4 WHERE content=?1 AND audio_stream=?2",
        params![key.content, key.audio_stream, version, value],
    )?;
    Ok(())
}

/// Push a step, dropping the oldest beyond the bound.
fn push(
    transaction: &Transaction<'_>,
    key: &CorrectionsKey,
    stack: &str,
    label: &str,
    value: &str,
) -> Result<(), StoreError> {
    let top: Option<i64> = transaction.query_row(
        "SELECT max(position) FROM analysis_correction_steps
            WHERE content=?1 AND audio_stream=?2 AND stack=?3",
        params![key.content, key.audio_stream, stack],
        |row| row.get(0),
    )?;
    let position = top.map_or(0, |top| top + 1);
    transaction.execute(
        "INSERT INTO analysis_correction_steps(content,audio_stream,stack,position,label,value)
            VALUES(?1,?2,?3,?4,?5,?6)",
        params![key.content, key.audio_stream, stack, position, label, value],
    )?;
    transaction.execute(
        "DELETE FROM analysis_correction_steps WHERE content=?1 AND audio_stream=?2 AND stack=?3
            AND position <= ?4",
        params![
            key.content,
            key.audio_stream,
            stack,
            position - MAX_CORRECTION_UNDO
        ],
    )?;
    Ok(())
}

fn pop(
    transaction: &Transaction<'_>,
    key: &CorrectionsKey,
    stack: &str,
) -> Result<Option<(String, String)>, StoreError> {
    let top: Option<(i64, String, String)> = transaction
        .query_row(
            "SELECT position,label,value FROM analysis_correction_steps
                WHERE content=?1 AND audio_stream=?2 AND stack=?3
                ORDER BY position DESC LIMIT 1",
            params![key.content, key.audio_stream, stack],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((position, label, value)) = top else {
        return Ok(None);
    };
    transaction.execute(
        "DELETE FROM analysis_correction_steps
            WHERE content=?1 AND audio_stream=?2 AND stack=?3 AND position=?4",
        params![key.content, key.audio_stream, stack, position],
    )?;
    Ok(Some((label, value)))
}

fn encode(corrections: &Corrections) -> Result<String, StoreError> {
    let value = serde_json::to_string(corrections)?;
    if value.len() > MAX_CORRECTIONS_JSON_BYTES {
        return Err(invalid("corrections exceed their stored size"));
    }
    Ok(value)
}

fn parse(value: &str) -> Result<Corrections, StoreError> {
    if value.len() > MAX_CORRECTIONS_JSON_BYTES {
        return Err(invalid("stored corrections exceed their size"));
    }
    serde_json::from_str(value).map_err(|error| invalid(&format!("stored corrections: {error}")))
}

fn check_label(label: &str) -> Result<(), StoreError> {
    if label.is_empty() || label.len() > MAX_LABEL_BYTES || label.chars().any(char::is_control) {
        return Err(invalid(
            "correction label is empty, oversized or has controls",
        ));
    }
    Ok(())
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE analysis_corrections (
            content TEXT NOT NULL,
            audio_stream INTEGER NOT NULL CHECK(audio_stream BETWEEN 0 AND 4294967295),
            version INTEGER NOT NULL CHECK(version>=0),
            value TEXT NOT NULL CHECK(json_valid(value)),
            PRIMARY KEY(content,audio_stream)
        ) STRICT;
        CREATE TABLE analysis_correction_steps (
            content TEXT NOT NULL,
            audio_stream INTEGER NOT NULL,
            stack TEXT NOT NULL CHECK(stack IN ('undo','redo')),
            position INTEGER NOT NULL CHECK(position>=0),
            label TEXT NOT NULL,
            value TEXT NOT NULL CHECK(json_valid(value)),
            PRIMARY KEY(content,audio_stream,stack,position),
            FOREIGN KEY(content,audio_stream)
                REFERENCES analysis_corrections(content,audio_stream) ON DELETE CASCADE
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let count: i64 =
        connection.query_row("SELECT count(*) FROM analysis_corrections", [], |row| {
            row.get(0)
        })?;
    if count > MAX_CORRECTED_STREAMS {
        return Err(invalid("too many corrected audio streams"));
    }
    let malformed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM analysis_corrections WHERE
                length(CAST(content AS BLOB)) NOT BETWEEN 1 AND ?1
                OR length(CAST(value AS BLOB)) NOT BETWEEN 1 AND ?2)
            OR EXISTS(SELECT 1 FROM analysis_correction_steps WHERE
                length(CAST(label AS BLOB)) NOT BETWEEN 1 AND ?3
                OR length(CAST(value AS BLOB)) NOT BETWEEN 1 AND ?2)
            OR EXISTS(SELECT 1 FROM analysis_correction_steps
                GROUP BY content,audio_stream,stack HAVING count(*) > ?4)",
        params![
            MAX_KEY_TEXT_BYTES as i64,
            MAX_CORRECTIONS_JSON_BYTES as i64,
            MAX_LABEL_BYTES as i64,
            MAX_CORRECTION_UNDO
        ],
        |row| row.get(0),
    )?;
    if malformed {
        return Err(invalid("invalid stored corrections field"));
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
