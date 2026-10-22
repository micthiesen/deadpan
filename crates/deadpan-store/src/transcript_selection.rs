//! Host-only authority for a selection derived from one exact stored transcript.
//! Never serialized into authored commands, captures or history.

use std::io::Write;

use deadpan_analysis::Transcript;
use deadpan_core::{AssetId, ProjectId, RevisionId, SourceQualificationId};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CorrectionsKey, ProjectStore, StoreError, StoredCorrections, TranscriptKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptSelectionFormat {
    #[serde(rename = "deadpan-transcript-selection-1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptSelectionCorrectionRule {
    #[serde(rename = "deadpan-corrections-1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum TranscriptSelectionCorrections {
    Absent,
    Present { version: u64, sha256: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "GuardRecord")]
pub struct TranscriptSelectionGuard {
    pub format: TranscriptSelectionFormat,
    pub asset: AssetId,
    pub qualification: SourceQualificationId,
    pub key: TranscriptKey,
    pub proposal_sha256: String,
    pub correction_rule: TranscriptSelectionCorrectionRule,
    pub corrections: TranscriptSelectionCorrections,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuardRecord {
    format: TranscriptSelectionFormat,
    asset: AssetId,
    qualification: SourceQualificationId,
    key: TranscriptKey,
    proposal_sha256: String,
    correction_rule: TranscriptSelectionCorrectionRule,
    corrections: TranscriptSelectionCorrections,
}

impl TryFrom<GuardRecord> for TranscriptSelectionGuard {
    type Error = StoreError;

    fn try_from(value: GuardRecord) -> Result<Self, Self::Error> {
        let guard = Self {
            format: value.format,
            asset: value.asset,
            qualification: value.qualification,
            key: value.key,
            proposal_sha256: value.proposal_sha256,
            correction_rule: value.correction_rule,
            corrections: value.corrections,
        };
        guard.validate()?;
        Ok(guard)
    }
}

impl TranscriptSelectionGuard {
    fn validate(&self) -> Result<(), StoreError> {
        self.key.validate()?;
        let hash_ok = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        };
        if !hash_ok(&self.proposal_sha256)
            || matches!(&self.corrections, TranscriptSelectionCorrections::Present { sha256, .. } if !hash_ok(sha256))
        {
            return Err(invalid(
                "transcript selection hashes must be lowercase SHA-256",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptSelectionSnapshot {
    pub key: TranscriptKey,
    pub proposal: Transcript,
    pub corrections: Option<StoredCorrections>,
    pub guard: TranscriptSelectionGuard,
}

impl ProjectStore {
    /// Qualified Original, exact proposal and strict corrections from one read snapshot.
    pub fn transcript_selection(
        &self,
        key: &TranscriptKey,
    ) -> Result<TranscriptSelectionSnapshot, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        read_selection(&transaction, &self.documents, key)
    }

    /// A selection-only receipt has authority only in this read transaction.
    /// This works for read-only inspectors and performs no database writes.
    pub fn validate_selection_at(
        &self,
        project: &ProjectId,
        revision: &RevisionId,
        bank_version: u64,
        guard: &TranscriptSelectionGuard,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let current = crate::read_head_project(&transaction)?;
        if current.project_id() != project || current.revision_id() != revision {
            return Err(invalid("transcript selection project or revision changed"));
        }
        if crate::registers::read_bank(&transaction)?.version != bank_version {
            return Err(invalid("transcript selection register bank changed"));
        }
        check_guard(&transaction, &self.documents, guard)
    }
}

pub(crate) fn check_guard(
    connection: &Connection,
    documents: &crate::document_cache::DocumentCache,
    expected: &TranscriptSelectionGuard,
) -> Result<(), StoreError> {
    expected.validate()?;
    let current = read_selection(connection, documents, &expected.key)?;
    if current.guard != *expected {
        return Err(invalid("transcript selection analysis changed"));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn read_selection(
    connection: &Connection,
    documents: &crate::document_cache::DocumentCache,
    key: &TranscriptKey,
) -> Result<TranscriptSelectionSnapshot, StoreError> {
    key.validate()?;
    let Some((
        crate::single_source::SingleSourceState::Ready {
            asset,
            qualification,
            baseline_revision,
            ..
        },
        _,
    )) = crate::single_source::read(connection)?
    else {
        return Err(invalid(
            "transcript selection requires a ready single Original",
        ));
    };
    let baseline = documents.revision(connection, baseline_revision.as_str())?;
    let record = baseline
        .assets()
        .get(&asset)
        .ok_or_else(|| invalid("transcript selection baseline Original is missing"))?;
    let receipt = crate::source_registration::read_receipt(connection, &qualification)?
        .ok_or_else(|| invalid("transcript selection Original qualification is missing"))?;
    if record.source_qualification.as_ref() != Some(&qualification)
        || receipt.asset_record(record.label.clone())? != *record
        || receipt.snapshot().video().is_none()
        || receipt.original().content().to_string() != key.content
        || receipt
            .snapshot()
            .audio()
            .map(|audio| audio.stream().stream_index)
            != Some(key.audio_stream)
    {
        return Err(invalid(
            "transcript selection key does not match the qualified Original",
        ));
    }
    let proposal = crate::transcripts::read(connection, key)?
        .ok_or_else(|| invalid("transcript selection proposal is missing"))?;
    let correction_key = CorrectionsKey {
        content: key.content.clone(),
        audio_stream: key.audio_stream,
    };
    crate::analysis_corrections::audit_key(connection, &correction_key)?;
    let corrections = crate::analysis_corrections::read(connection, &correction_key)?;
    let state = match &corrections {
        None => TranscriptSelectionCorrections::Absent,
        Some(stored) => {
            let corrected = stored
                .corrections
                .apply_to_transcript(&proposal)
                .map_err(|error| invalid(&format!("transcript selection corrections: {error}")))?;
            if corrected.skipped != 0 {
                return Err(invalid(
                    "transcript selection corrections contain skipped word regions",
                ));
            }
            TranscriptSelectionCorrections::Present {
                version: stored.version,
                sha256: canonical_digest(
                    b"deadpan-transcript-selection-corrections-v1\n",
                    &stored.corrections,
                    crate::MAX_CORRECTIONS_JSON_BYTES,
                )?,
            }
        }
    };
    let guard = TranscriptSelectionGuard {
        format: TranscriptSelectionFormat::V1,
        asset,
        qualification,
        key: key.clone(),
        proposal_sha256: canonical_digest(
            b"deadpan-transcript-selection-proposal-v1\n",
            &proposal,
            crate::MAX_TRANSCRIPT_JSON_BYTES,
        )?,
        correction_rule: TranscriptSelectionCorrectionRule::V1,
        corrections: state,
    };
    Ok(TranscriptSelectionSnapshot {
        key: key.clone(),
        proposal,
        corrections,
        guard,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read_selection(
    _connection: &Connection,
    _documents: &crate::document_cache::DocumentCache,
    _key: &TranscriptKey,
) -> Result<TranscriptSelectionSnapshot, StoreError> {
    Err(invalid(
        "qualified transcript selection is unavailable on this platform",
    ))
}

fn canonical_digest(
    domain: &[u8],
    value: &impl Serialize,
    limit: usize,
) -> Result<String, StoreError> {
    struct BoundedDigest {
        digest: Sha256,
        bytes: usize,
        limit: usize,
    }
    impl Write for BoundedDigest {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let length = self
                .bytes
                .checked_add(bytes.len())
                .filter(|length| *length <= self.limit)
                .ok_or_else(|| {
                    std::io::Error::other("transcript selection canonical value exceeds its bound")
                })?;
            self.digest.update(bytes);
            self.bytes = length;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = BoundedDigest {
        digest: Sha256::new(),
        bytes: 0,
        limit,
    };
    writer.digest.update(domain);
    serde_json::to_writer(&mut writer, value)?;
    Ok(writer
        .digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
