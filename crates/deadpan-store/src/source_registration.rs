//! Atomic authored source registration from actual decoded measurements.
//!
//! Receipts retain their measured indexes independently of undo/redo. Each
//! historical asset binds an immutable receipt identity, never a mutable alias.
//! Deserializing a receipt cannot grant fresh admission; only a decoded token
//! and reverified original bytes can enter through this host boundary.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_core::{
    AssetId, AssetRecord, Command, CommandRequest, EditTransaction, FrameDuration, FrameRate,
    FrameRateOrigin, GeometryOrigin, NodeId, PrimarySourceImport, ProjectDocument, RevisionId,
    SourceFrameIndex, SourceInsertion, SourceQualificationId,
};
use deadpan_media::source_qualification::{
    DecodedSourceQualification, MAX_SOURCE_QUALIFICATION_JSON_BYTES, SourceQualificationSnapshot,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::generation::RelevancePlan;
use crate::original_media::{
    OriginalContentId, OriginalMediaLimits, OriginalMediaRecord, OriginalObjectRef,
    PreparedOriginalSnapshot,
};
use crate::{CommandPlan, CommitOutcome, ProjectStore, StoreError};

const MAX_QUALIFICATIONS: i64 = 100_000;
const MAX_ORIGINAL_REF_BYTES: usize = 256;
const RECEIPT_DOMAIN: &[u8] = b"deadpan-source-qualification-v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInsertionRequest {
    pub parent: NodeId,
    pub index: usize,
    pub node: NodeId,
    pub label: String,
    /// Ordinary sequence insertion is primary; reactions/supporting material
    /// must opt into Secondary and can never choose the project basis.
    #[serde(default)]
    pub purpose: SourceInsertionPurpose,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceInsertionPurpose {
    #[default]
    Primary,
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryGeometryAdoption {
    pub expected_revision: RevisionId,
    pub new_revision: RevisionId,
}

/// Allocation IDs are supplied by the host. Existing equal qualification is
/// reused, so `new_asset_id` is used only when registration creates an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRegistration {
    pub expected_revision: RevisionId,
    pub new_revision: RevisionId,
    pub original: OriginalContentId,
    pub new_asset_id: AssetId,
    pub label: String,
    pub insertion: Option<SourceInsertionRequest>,
}

#[derive(Debug, Serialize)]
pub struct SourceRegistrationPreview {
    pub asset_id: AssetId,
    pub qualification: SourceQualificationId,
    /// None means an identical source is registered and no insertion was asked for.
    pub edit: Option<EditTransaction>,
}

#[derive(Debug, Serialize)]
pub struct SourceRegistrationOutcome {
    pub asset_id: AssetId,
    pub qualification: SourceQualificationId,
    pub commit: Option<CommitOutcome>,
}

/// Worker-prepared admission from verified retained bytes and actual decoders.
/// It is bound to the issuing project session and keeps its private byte snapshot
/// alive. It contains no authored target or frozen project timing decisions.
///
/// ```compile_fail
/// use deadpan_store::source_registration::PreparedSourceRegistration;
/// let forged = serde_json::from_str::<PreparedSourceRegistration>("{}");
/// ```
pub struct PreparedSourceRegistration {
    original: PreparedOriginalSnapshot,
    receipt: SourceQualificationReceipt,
    bytes: Vec<u8>,
}

impl PreparedSourceRegistration {
    /// Canonicalize measured indexes and hash their receipt off the writer thread.
    pub fn from_decoded(
        original: PreparedOriginalSnapshot,
        decoded: &DecodedSourceQualification,
        cancelled: &AtomicBool,
    ) -> Result<Self, StoreError> {
        original.recheck(cancelled)?;
        let (receipt, bytes) = prepare_receipt(original.record(), decoded)?;
        original.recheck(cancelled)?;
        Ok(Self {
            original,
            receipt,
            bytes,
        })
    }

    pub fn receipt(&self) -> &SourceQualificationReceipt {
        &self.receipt
    }

    fn validate_for(
        &self,
        store: &ProjectStore,
        input: &SourceRegistration,
        cancelled: &AtomicBool,
    ) -> Result<(), StoreError> {
        if &input.original != self.receipt.original.content() {
            return Err(invalid(
                "prepared qualification belongs to another original",
            ));
        }
        self.original.validate_for(store, cancelled)
    }
}

/// Validated stored evidence, not a live decode/admission token. Availability
/// still requires a fresh original snapshot, including on linked-source use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceQualificationReceipt {
    id: SourceQualificationId,
    original: OriginalObjectRef,
    snapshot: SourceQualificationSnapshot,
}

impl SourceQualificationReceipt {
    pub fn id(&self) -> &SourceQualificationId {
        &self.id
    }
    pub fn original(&self) -> &OriginalObjectRef {
        &self.original
    }
    pub fn snapshot(&self) -> &SourceQualificationSnapshot {
        &self.snapshot
    }

    fn asset_record(&self, label: String) -> Result<AssetRecord, StoreError> {
        // Asset extents stay in original clocks. One fps avoids attaching the
        // metadata to any project's presentation basis.
        let timing = self
            .snapshot
            .derive_timing(FrameRate::new(1, 1).expect("constant positive rate"))?;
        Ok(AssetRecord {
            label,
            content_hash: self.original.content().to_string(),
            video: timing.video.map(|placement| placement.span),
            audio: timing.audio.map(|placement| placement.span),
            still_image: false,
            frame_count: self
                .snapshot
                .video()
                .map(|video| {
                    let count = i64::try_from(video.index().index().frames().len())
                        .map_err(|_| invalid("source frame count is not representable"))?;
                    FrameDuration::new(count).map_err(|error| invalid(&error.to_string()))
                })
                .transpose()?,
            source_qualification: Some(self.id.clone()),
        })
    }
}

struct PreparedRegistration {
    asset_id: AssetId,
    plan: Option<CommandPlan>,
}

impl ProjectStore {
    /// Derives the recorded first primary source's geometry at the already
    /// fixed project rate. Metadata can be inspected even when media is offline.
    pub fn preview_primary_geometry(
        &self,
        input: &PrimaryGeometryAdoption,
    ) -> Result<EditTransaction, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        Ok(prepare_primary_geometry(&transaction, input)?.edit)
    }

    pub fn adopt_primary_geometry(
        &mut self,
        input: &PrimaryGeometryAdoption,
        relevance: Option<&RelevancePlan>,
    ) -> Result<CommitOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let plan = prepare_primary_geometry(&transaction, input)?;
        let outcome = crate::write_command_plan(&transaction, plan, relevance)?;
        transaction.commit()?;
        Ok(outcome)
    }

    /// Resolve media through the revision being rendered, never the latest
    /// meaning of a potentially reused asset alias. This does not verify bytes.
    pub fn registered_source(
        &self,
        revision: &RevisionId,
        asset: &AssetId,
    ) -> Result<SourceQualificationReceipt, StoreError> {
        let document =
            crate::validation::read_revision(&self.connection, revision.as_str())?.document;
        let record = document
            .assets()
            .get(asset)
            .ok_or_else(|| invalid("asset is absent from the selected revision"))?;
        let id = record
            .source_qualification
            .as_ref()
            .ok_or_else(|| invalid("asset has no measured source qualification"))?;
        let receipt = self.source_qualification(id)?;
        if receipt.asset_record(record.label.clone())? != *record {
            return Err(invalid(
                "asset metadata disagrees with selected source qualification",
            ));
        }
        Ok(receipt)
    }

    /// Rebind the canonical cached index to the authored alias of one immutable
    /// revision. Original presentation identities, clocks and hints stay exact.
    pub fn source_video_index(
        &self,
        revision: &RevisionId,
        asset: &AssetId,
    ) -> Result<SourceFrameIndex, StoreError> {
        let receipt = self.registered_source(revision, asset)?;
        let video = receipt
            .snapshot
            .video()
            .ok_or_else(|| invalid("registered source has no selected picture"))?;
        let index = video.index().index();
        Ok(SourceFrameIndex::new(
            asset.clone(),
            index.time_base(),
            index.frames().to_vec(),
            index.terminal_end(),
            index.terminal_provenance(),
        )?)
    }

    pub fn source_qualification(
        &self,
        id: &SourceQualificationId,
    ) -> Result<SourceQualificationReceipt, StoreError> {
        read_receipt(&self.connection, id)?
            .ok_or_else(|| invalid("source qualification is missing"))
    }

    /// Computes precisely the edit used for revision-bound relevance resolution.
    /// It verifies media but writes neither an asset nor a qualification receipt.
    pub fn preview_source_registration(
        &self,
        input: &SourceRegistration,
        decoded: &DecodedSourceQualification,
        limits: OriginalMediaLimits,
        cancelled: &AtomicBool,
    ) -> Result<SourceRegistrationPreview, StoreError> {
        check_revision(&self.snapshot()?, input)?;
        let original = self.snapshot_original(&input.original, limits, cancelled)?;
        let (receipt, _) = prepare_receipt(original.record(), decoded)?;
        check_cancelled(cancelled)?;
        let transaction = self.connection.unchecked_transaction()?;
        let prepared = prepare_registration(&transaction, input, &receipt)?;
        Ok(SourceRegistrationPreview {
            asset_id: prepared.asset_id,
            qualification: receipt.id,
            edit: prepared.plan.map(|plan| plan.edit),
        })
    }

    /// Retains measured evidence and commits registration plus optional full-source
    /// insertion in one SQLite transaction. Bytes must already be retained by the
    /// original-media service. Their publication survives a later command failure.
    pub fn register_source(
        &mut self,
        input: &SourceRegistration,
        decoded: &DecodedSourceQualification,
        relevance: Option<&RelevancePlan>,
        limits: OriginalMediaLimits,
        cancelled: &AtomicBool,
    ) -> Result<SourceRegistrationOutcome, StoreError> {
        self.require_writer()?;
        check_revision(&self.snapshot()?, input)?;
        let record = self
            .original_record(&input.original)?
            .ok_or(crate::original_media::OriginalMediaError::MissingRecord)?;
        let original = self
            .original_import_handle()?
            .snapshot_original(&record, limits, cancelled)?;
        let prepared = PreparedSourceRegistration::from_decoded(original, decoded, cancelled)?;
        self.register_prepared_source(input, &prepared, relevance, cancelled)
    }

    /// Preview current-revision intent without repeating file verification or
    /// decoding. This session-bound path is intended for the native writer service;
    /// the ordinary synchronous preview also supports independent read-only hosts.
    pub fn preview_prepared_source_registration(
        &self,
        input: &SourceRegistration,
        source: &PreparedSourceRegistration,
        cancelled: &AtomicBool,
    ) -> Result<SourceRegistrationPreview, StoreError> {
        check_revision(&self.snapshot()?, input)?;
        source.validate_for(self, input, cancelled)?;
        let transaction = self.connection.unchecked_transaction()?;
        check_original_binding(&transaction, &source.receipt)?;
        let prepared = prepare_registration(&transaction, input, &source.receipt)?;
        source.original.recheck(cancelled)?;
        Ok(SourceRegistrationPreview {
            asset_id: prepared.asset_id,
            qualification: source.receipt.id.clone(),
            edit: prepared.plan.map(|plan| plan.edit),
        })
    }

    /// Commit an already prepared source using the current document clock and
    /// explicit revision/target intent. Final availability checks perform metadata
    /// inspection only; they reject changed or missing originals instead of rehashing.
    pub fn register_prepared_source(
        &mut self,
        input: &SourceRegistration,
        source: &PreparedSourceRegistration,
        relevance: Option<&RelevancePlan>,
        cancelled: &AtomicBool,
    ) -> Result<SourceRegistrationOutcome, StoreError> {
        self.require_writer()?;
        check_revision(&self.snapshot()?, input)?;
        source.validate_for(self, input, cancelled)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_original_binding(&transaction, &source.receipt)?;
        let prepared = prepare_registration(&transaction, input, &source.receipt)?;
        write_receipt(&transaction, &source.receipt, &source.bytes)?;
        let commit = prepared
            .plan
            .map(|plan| crate::write_command_plan(&transaction, plan, relevance))
            .transpose()?;
        source.original.recheck(cancelled)?;
        transaction.commit()?;
        Ok(SourceRegistrationOutcome {
            asset_id: prepared.asset_id,
            qualification: source.receipt.id.clone(),
            commit,
        })
    }
}

fn prepare_receipt(
    original: &OriginalMediaRecord,
    decoded: &DecodedSourceQualification,
) -> Result<(SourceQualificationReceipt, Vec<u8>), StoreError> {
    let snapshot = decoded.snapshot();
    if snapshot.content().sha256() != original.sha256()
        || snapshot.content().byte_length() != original.object().byte_length()
    {
        return Err(invalid(
            "decoded source and verified original identities differ",
        ));
    }
    let bytes = snapshot.to_json()?;
    let id = receipt_id(original.object(), &bytes)?;
    Ok((
        SourceQualificationReceipt {
            id,
            original: original.object().clone(),
            snapshot: snapshot.clone(),
        },
        bytes,
    ))
}

fn receipt_id(
    original: &OriginalObjectRef,
    bytes: &[u8],
) -> Result<SourceQualificationId, StoreError> {
    let mut hash = blake3::Hasher::new();
    hash.update(RECEIPT_DOMAIN);
    hash.update(original.content().digest().as_bytes());
    hash.update(&original.byte_length().to_be_bytes());
    let length = u64::try_from(bytes.len()).map_err(|_| invalid("receipt is too large"))?;
    hash.update(&length.to_be_bytes());
    hash.update(bytes);
    Ok(SourceQualificationId::new(
        hash.finalize().to_hex().to_string(),
    )?)
}

fn prepare_registration(
    connection: &Connection,
    input: &SourceRegistration,
    receipt: &SourceQualificationReceipt,
) -> Result<PreparedRegistration, StoreError> {
    let current = crate::read_snapshot(connection)?;
    check_revision(&current, input)?;
    let existing = current
        .assets()
        .iter()
        .find(|(_, record)| record.source_qualification.as_ref() == Some(receipt.id()));
    let (asset_id, record) = if let Some((id, record)) = existing {
        if *record != receipt.asset_record(record.label.clone())? {
            return Err(invalid(
                "registered asset metadata disagrees with its qualification",
            ));
        }
        (id.clone(), record.clone())
    } else {
        (
            input.new_asset_id.clone(),
            receipt.asset_record(input.label.clone())?,
        )
    };
    if existing.is_some() && input.insertion.is_none() {
        return Ok(PreparedRegistration {
            asset_id,
            plan: None,
        });
    }
    let primary = if input
        .insertion
        .as_ref()
        .is_some_and(|target| target.purpose == SourceInsertionPurpose::Primary)
        && receipt.snapshot.video().is_some()
        && current.basis_state().primary.is_none()
    {
        Some(
            if current.basis_state().rate_origin == FrameRateOrigin::Provisional {
                PrimarySourceImport::Adopt {
                    basis: receipt
                        .snapshot
                        .basis_candidate()?
                        .ok_or_else(|| invalid("primary source has no picture basis"))?
                        .basis,
                }
            } else {
                PrimarySourceImport::KeepBasis
            },
        )
    } else {
        None
    };
    let frame_rate = match &primary {
        Some(PrimarySourceImport::Adopt { basis }) => basis.frame_rate,
        _ => current.presentation_basis().frame_rate,
    };
    let insertion = input
        .insertion
        .as_ref()
        .map(|target| {
            Ok::<_, StoreError>(SourceInsertion {
                parent: target.parent.clone(),
                index: target.index,
                node: target.node.clone(),
                label: target.label.clone(),
                source: receipt
                    .snapshot
                    .derive_timing(frame_rate)?
                    .source_node(asset_id.clone()),
            })
        })
        .transpose()?;
    let request = CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: input.expected_revision.clone(),
        new_revision: input.new_revision.clone(),
        command: Command::ImportSource {
            id: asset_id.clone(),
            asset: record.clone(),
            insertion: insertion.map(Box::new),
            primary,
        },
    };
    let plan = crate::prepare_command_with_admission(
        connection,
        &request,
        None,
        Some((&asset_id, &record)),
        None,
    )?;
    Ok(PreparedRegistration {
        asset_id,
        plan: Some(plan),
    })
}

fn prepare_primary_geometry(
    connection: &Connection,
    input: &PrimaryGeometryAdoption,
) -> Result<CommandPlan, StoreError> {
    let current = crate::read_snapshot(connection)?;
    if current.revision_id() != &input.expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: input.expected_revision.as_str().into(),
            current: current.revision_id().as_str().into(),
        });
    }
    let primary = current
        .basis_state()
        .primary
        .as_ref()
        .ok_or_else(|| invalid("project has no recorded primary picture source"))?;
    let receipt = read_receipt(connection, &primary.qualification)?
        .ok_or_else(|| invalid("primary source qualification is missing"))?;
    let record = current
        .assets()
        .get(&primary.asset)
        .ok_or_else(|| invalid("primary asset is absent"))?;
    if receipt.asset_record(record.label.clone())? != *record {
        return Err(invalid(
            "primary asset disagrees with measured source evidence",
        ));
    }
    let geometry = receipt
        .snapshot
        .geometry_candidate()?
        .ok_or_else(|| invalid("primary source has no picture geometry"))?;
    let request = CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: input.expected_revision.clone(),
        new_revision: input.new_revision.clone(),
        command: Command::AdoptPrimaryGeometry {
            width: geometry.width,
            height: geometry.height,
        },
    };
    crate::prepare_command_with_admission(
        connection,
        &request,
        None,
        None,
        Some((geometry.width, geometry.height)),
    )
}

fn check_revision(current: &ProjectDocument, input: &SourceRegistration) -> Result<(), StoreError> {
    if current.revision_id() != &input.expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: input.expected_revision.as_str().into(),
            current: current.revision_id().as_str().into(),
        });
    }
    Ok(())
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), StoreError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(crate::original_media::OriginalMediaError::Cancelled.into());
    }
    Ok(())
}

fn write_receipt(
    connection: &Connection,
    receipt: &SourceQualificationReceipt,
    bytes: &[u8],
) -> Result<(), StoreError> {
    // The incoming opaque token already owns canonical, hashed evidence. Compare
    // existing bytes inside SQLite instead of deserializing and rehashing a large
    // index on the writer thread. Exact equality retains collision/corruption checks.
    let existing: Option<(String, String, bool)> = connection.query_row(
        "SELECT CASE WHEN typeof(original_content_id)='text' AND length(CAST(original_content_id AS BLOB))=71 THEN original_content_id END,
         CASE WHEN typeof(original_ref)='text' AND length(CAST(original_ref AS BLOB))<=?3 THEN original_ref END,
         CASE WHEN typeof(snapshot)='blob' AND length(snapshot)<=?4 THEN snapshot=?2 ELSE 0 END
         FROM source_qualifications WHERE id=?1",
        params![receipt.id.as_str(), bytes, MAX_ORIGINAL_REF_BYTES as i64, MAX_SOURCE_QUALIFICATION_JSON_BYTES as i64],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    if let Some((content, original, same_bytes)) = existing {
        let original: OriginalObjectRef = serde_json::from_str(&original)?;
        if !same_bytes
            || original != receipt.original
            || content != receipt.original.content().to_string()
        {
            return Err(invalid("immutable qualification identity collision"));
        }
        return Ok(());
    }
    let count: i64 =
        connection.query_row("SELECT count(*) FROM source_qualifications", [], |row| {
            row.get(0)
        })?;
    if count >= MAX_QUALIFICATIONS {
        return Err(invalid("source qualification count limit reached"));
    }
    connection.execute(
        "INSERT INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
        params![receipt.id.as_str(), receipt.original.content().to_string(), serde_json::to_string(&receipt.original)?, bytes],
    )?;
    Ok(())
}

fn read_receipt(
    connection: &Connection,
    id: &SourceQualificationId,
) -> Result<Option<SourceQualificationReceipt>, StoreError> {
    let row: Option<(String, String, Vec<u8>)> = connection.query_row(
        "SELECT original_content_id,
         CASE WHEN typeof(original_ref)='text' AND length(CAST(original_ref AS BLOB))<=?2 THEN original_ref END,
         CASE WHEN typeof(snapshot)='blob' AND length(snapshot)<=?3 THEN snapshot END
         FROM source_qualifications WHERE id=?1",
        params![id.as_str(), MAX_ORIGINAL_REF_BYTES as i64, MAX_SOURCE_QUALIFICATION_JSON_BYTES as i64],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    row.map(|(content, original, bytes)| {
        let original: OriginalObjectRef = serde_json::from_str(&original)?;
        if original.content().to_string() != content || receipt_id(&original, &bytes)? != *id {
            return Err(invalid(
                "qualification identity disagrees with stored evidence",
            ));
        }
        let snapshot = SourceQualificationSnapshot::from_json(&bytes)?;
        if snapshot.to_json()? != bytes {
            return Err(invalid(
                "qualification snapshot is not in its canonical representation",
            ));
        }
        let receipt = SourceQualificationReceipt {
            id: id.clone(),
            original,
            snapshot,
        };
        check_original_binding(connection, &receipt)?;
        Ok(receipt)
    })
    .transpose()
}

fn check_original_binding(
    connection: &Connection,
    receipt: &SourceQualificationReceipt,
) -> Result<(), StoreError> {
    let record = crate::original_media::read_record(connection, receipt.original.content())?
        .ok_or_else(|| invalid("qualified original has no ownership record"))?;
    if record.object() != &receipt.original
        || record.sha256() != receipt.snapshot.content().sha256()
        || record.object().byte_length() != receipt.snapshot.content().byte_length()
    {
        return Err(invalid(
            "qualification and original ownership identities differ",
        ));
    }
    Ok(())
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE source_qualifications (
            id TEXT PRIMARY KEY,
            original_content_id TEXT NOT NULL REFERENCES original_media(content_id),
            original_ref TEXT NOT NULL CHECK(json_valid(original_ref)),
            snapshot BLOB NOT NULL
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let invalid_row: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM source_qualifications
         WHERE typeof(id)!='text' OR length(CAST(id AS BLOB))!=64
         OR typeof(original_content_id)!='text' OR length(CAST(original_content_id AS BLOB))!=71
         OR typeof(original_ref)!='text' OR length(CAST(original_ref AS BLOB)) NOT BETWEEN 1 AND ?1
         OR typeof(snapshot)!='blob' OR length(snapshot) NOT BETWEEN 1 AND ?2)",
        params![
            MAX_ORIGINAL_REF_BYTES as i64,
            MAX_SOURCE_QUALIFICATION_JSON_BYTES as i64
        ],
        |row| row.get(0),
    )?;
    let count: i64 =
        connection.query_row("SELECT count(*) FROM source_qualifications", [], |row| {
            row.get(0)
        })?;
    if invalid_row || count > MAX_QUALIFICATIONS {
        return Err(invalid("invalid or oversized source qualification rows"));
    }
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    check_stored_sizes(connection)?;
    let mut metadata = BTreeMap::new();
    let mut statement = connection.prepare("SELECT id FROM source_qualifications ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id = SourceQualificationId::new(row.get(0)?)?;
        let receipt =
            read_receipt(connection, &id)?.ok_or_else(|| invalid("qualification disappeared"))?;
        // Candidate derivation may legitimately be unavailable for a source
        // used at an explicit project basis. Retain only compact results and
        // require them when history claims source-derived presentation.
        let basis = receipt
            .snapshot
            .basis_candidate()
            .ok()
            .flatten()
            .map(|candidate| candidate.basis);
        let geometry = receipt
            .snapshot
            .geometry_candidate()
            .ok()
            .flatten()
            .map(|candidate| (candidate.width, candidate.height));
        metadata.insert(id, (receipt.asset_record(String::new())?, basis, geometry));
    }
    // Include abandoned branches, not only current head or active redo. Each
    // asset carries its own receipt ID even if its alias is reused after undo.
    let mut statement = connection.prepare(
        "SELECT CASE WHEN typeof(document)='text' AND length(CAST(document AS BLOB))<=?1 THEN document END FROM revisions ORDER BY rowid",
    )?;
    let mut rows = statement.query([crate::schema::MAX_DOCUMENT_BYTES as i64])?;
    while let Some(row) = rows.next()? {
        let document = ProjectDocument::from_json(&row.get::<_, String>(0)?)?;
        for asset in document.assets().values() {
            let Some(id) = &asset.source_qualification else {
                continue;
            };
            let mut expected = metadata
                .get(id)
                .ok_or_else(|| invalid("historical asset has no qualification receipt"))?
                .0
                .clone();
            expected.label.clone_from(&asset.label);
            if expected != *asset {
                return Err(invalid(
                    "historical asset differs from immutable measured metadata",
                ));
            }
        }
        if let Some(primary) = &document.basis_state().primary {
            let (_, basis, geometry) = metadata
                .get(&primary.qualification)
                .ok_or_else(|| invalid("historical primary source has no qualification receipt"))?;
            if document.basis_state().rate_origin == FrameRateOrigin::PrimarySource
                && basis.as_ref().map(|basis| basis.frame_rate)
                    != Some(document.presentation_basis().frame_rate)
            {
                return Err(invalid(
                    "source-derived project rate differs from measured cadence",
                ));
            }
            if document.basis_state().geometry_origin == GeometryOrigin::PrimarySource
                && *geometry
                    != Some((
                        document.presentation_basis().width,
                        document.presentation_basis().height,
                    ))
            {
                return Err(invalid(
                    "source-derived canvas differs from measured geometry",
                ));
            }
        }
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::SourceRegistration(message.into())
}
