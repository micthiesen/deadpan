//! Durable generation attempt snapshots and host-validation receipts.
//!
//! Attempt state is operational metadata, separate from authored revisions and
//! request relevance. Progress remains live in memory. A legacy Ready receipt
//! records what the host says it validated and still requires the candidate
//! cache artifact to be reopened and verified. A V2 Ready receipt is recorded
//! only after this store has verified all three generated objects. Either kind
//! still requires a current-relevance check and an authored acceptance
//! transaction; Ready alone never changes the document.

use deadpan_core::{FrameDuration, GeneratedObjectRef, NodeId, ProjectId};
use deadpan_jobs::{
    AttemptId, BridgeGenerationPlan, CancellationAcknowledgement, CancellationToken,
    CandidateDeclaration, CandidateManifest, Diagnostic, FailureCode, HoldConstraints, HostFailure,
    HostFailureCode, JobFailure, JobLifecycle, JobState, LifecycleCheckpoint, MAX_DIAGNOSTIC_BYTES,
    MAX_PROTOCOL_ID_BYTES, MessageIdentity, NativeCandidateManifest, ProtocolVersion,
    ProviderSelection, Relevance, RequestId, RequestVersion, Sha256, TargetBinding, VideoSpec,
    WorkerEventOutcome, WorkerFailure, WorkerMessage, WorkerStage,
};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use thiserror::Error;

use crate::{ProjectStore, StoreError};

const MAX_ATTEMPT_JSON_BYTES: usize = 16 * 1024;
const MAX_MANAGED_REF_BYTES: usize = 1_024;
const MAX_ATTEMPT_PAGE: usize = 256;
const MAX_SQL_COUNTER: i64 = i64::MAX;

const NONTERMINAL_STATES: &str =
    "'queued','preflight','loading','running','validating','cancelling'";

const CREATE_TABLES: &str = "
CREATE TABLE generation_attempts (
    request_id TEXT NOT NULL REFERENCES generation_requests(request_id),
    attempt_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 9223372036854775807),
    cancellation_token TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'queued','preflight','loading','running','validating','ready','failed','cancelling','cancelled'
    )),
    worker_stage TEXT CHECK (worker_stage IS NULL OR worker_stage IN (
        'preflight','runtime_loading','model_loading','conditioning','inference',
        'decoding','encoding','worker_validation'
    )),
    transition_sequence INTEGER NOT NULL
        CHECK (transition_sequence BETWEEN 1 AND 9223372036854775807),
    cancel_response TEXT CHECK (cancel_response IS NULL OR cancel_response IN ('cancelled','completed')),
    worker_candidate TEXT CHECK (worker_candidate IS NULL OR json_valid(worker_candidate)),
    failure_origin TEXT CHECK (failure_origin IS NULL OR failure_origin IN ('worker','host')),
    failure_code TEXT,
    failure_detail TEXT,
    PRIMARY KEY (request_id,attempt_id),
    UNIQUE (request_id,ordinal),
    CHECK ((failure_origin IS NULL) = (failure_code IS NULL)),
    CHECK ((failure_origin IS NULL) = (failure_detail IS NULL)),
    CHECK (cancel_response!='completed' OR worker_candidate IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX one_nonterminal_generation_attempt_per_request
    ON generation_attempts(request_id)
    WHERE state IN ('queued','preflight','loading','running','validating','cancelling');
CREATE TABLE generation_candidate_receipts (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    staged_ref TEXT NOT NULL UNIQUE,
    sha256 TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length BETWEEN 1 AND 9223372036854775807),
    video TEXT NOT NULL CHECK (json_valid(video)),
    provider TEXT NOT NULL CHECK (json_valid(provider)),
    validator_id TEXT NOT NULL,
    validator_version TEXT NOT NULL,
    availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
CREATE TABLE generation_bundle_receipts (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    bundle TEXT NOT NULL CHECK (json_valid(bundle)),
    availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;
CREATE TABLE generation_attempt_heads (
    request_id TEXT PRIMARY KEY REFERENCES generation_requests(request_id),
    high_water INTEGER NOT NULL CHECK (high_water BETWEEN 1 AND 9223372036854775807),
    latest_attempt_id TEXT NOT NULL,
    selected_ready_attempt_id TEXT,
    FOREIGN KEY (request_id,latest_attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id),
    FOREIGN KEY (request_id,selected_ready_attempt_id)
        REFERENCES generation_attempts(request_id,attempt_id)
) STRICT;";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AttemptValueError {
    #[error("managed candidate reference must be a bounded path below Candidates")]
    InvalidManagedCandidateRef,
    #[error("validator identity must be a bounded protocol identifier")]
    InvalidValidatorIdentity,
    #[error("candidate byte length must be positive and fit SQLite")]
    InvalidByteLength,
    #[error("bundle metadata does not match its native bridge declaration")]
    BundleMetadataMismatch,
}

/// A lexical reference below a host-bound candidate cache root.
///
/// This type grants no filesystem authority. The host must create, contain,
/// fsync, and independently verify the file before constructing a receipt.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManagedCandidateRef(String);

impl ManagedCandidateRef {
    pub fn new(value: impl Into<String>) -> Result<Self, AttemptValueError> {
        let value = value.into();
        let valid = value.len() <= MAX_MANAGED_REF_BYTES
            && value.starts_with("Candidates/")
            && !value.contains('\0')
            && !value.contains('\\')
            && value
                .split('/')
                .all(|component| !component.is_empty() && !matches!(component, "." | ".."));
        if !valid {
            return Err(AttemptValueError::InvalidManagedCandidateRef);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidatorIdentity {
    id: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidatorIdentityWire {
    id: String,
    version: String,
}

impl<'de> Deserialize<'de> for ValidatorIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ValidatorIdentityWire::deserialize(deserializer)?;
        Self::new(wire.id, wire.version).map_err(D::Error::custom)
    }
}

impl ValidatorIdentity {
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, AttemptValueError> {
        let id = id.into();
        let version = version.into();
        if !valid_identifier(&id) || !valid_identifier(&version) {
            return Err(AttemptValueError::InvalidValidatorIdentity);
        }
        Ok(Self { id, version })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAvailability {
    Present,
    Evicted,
}

/// Immutable metadata recorded at the trusted host-validation boundary.
///
/// Construction does not validate media bytes. Before recording this receipt,
/// the host must independently contain, hash, probe, and validate the staged
/// file. Before acceptance it must verify the managed artifact again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateValidationReceipt {
    staged_ref: ManagedCandidateRef,
    sha256: Sha256,
    byte_length: u64,
    video: VideoSpec,
    provider: ProviderSelection,
    validator: ValidatorIdentity,
    availability: CandidateAvailability,
}

/// Host-owned qualification evidence for a V2 bridge result. The worker's
/// native/provenance declaration is retained in the attempt separately; this
/// receipt records the immutable generated objects after canonicalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundleValidationReceipt {
    native_object: GeneratedObjectRef,
    sampled_object: GeneratedObjectRef,
    provenance_object: GeneratedObjectRef,
    native_video: VideoSpec,
    sampled_video: VideoSpec,
    plan: BridgeGenerationPlan,
    provider: ProviderSelection,
    native_sha256: Sha256,
    native_byte_length: u64,
    provenance_sha256: Sha256,
    provenance_byte_length: u64,
    validator: ValidatorIdentity,
    availability: CandidateAvailability,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleValidationReceiptWire {
    native_object: GeneratedObjectRef,
    sampled_object: GeneratedObjectRef,
    provenance_object: GeneratedObjectRef,
    native_video: VideoSpec,
    sampled_video: VideoSpec,
    plan: BridgeGenerationPlan,
    provider: ProviderSelection,
    native_sha256: Sha256,
    native_byte_length: u64,
    provenance_sha256: Sha256,
    provenance_byte_length: u64,
    validator: ValidatorIdentity,
    availability: CandidateAvailability,
}

impl<'de> Deserialize<'de> for BundleValidationReceipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = BundleValidationReceiptWire::deserialize(deserializer)?;
        let receipt = Self {
            native_object: wire.native_object,
            sampled_object: wire.sampled_object,
            provenance_object: wire.provenance_object,
            native_video: wire.native_video,
            sampled_video: wire.sampled_video,
            plan: wire.plan,
            provider: wire.provider,
            native_sha256: wire.native_sha256,
            native_byte_length: wire.native_byte_length,
            provenance_sha256: wire.provenance_sha256,
            provenance_byte_length: wire.provenance_byte_length,
            validator: wire.validator,
            availability: wire.availability,
        };
        receipt.validate_shape().map_err(D::Error::custom)?;
        Ok(receipt)
    }
}

impl BundleValidationReceipt {
    pub fn new(
        declaration: &NativeCandidateManifest,
        native_object: GeneratedObjectRef,
        sampled_object: GeneratedObjectRef,
        provenance_object: GeneratedObjectRef,
        sampled_video: VideoSpec,
        plan: BridgeGenerationPlan,
        validator: ValidatorIdentity,
    ) -> Result<Self, AttemptValueError> {
        let dimensions = plan.native_dimensions();
        let native_video = VideoSpec::new(
            FrameDuration::new(i64::from(plan.native_frame_count()))
                .map_err(|_| AttemptValueError::BundleMetadataMismatch)?,
            plan.native_frame_rate(),
            dimensions.width(),
            dimensions.height(),
        )
        .map_err(|_| AttemptValueError::BundleMetadataMismatch)?;
        let receipt = Self {
            native_object,
            sampled_object,
            provenance_object,
            native_video,
            sampled_video,
            plan,
            provider: declaration.provider.clone(),
            native_sha256: declaration.native.sha256().clone(),
            native_byte_length: declaration.native.byte_length(),
            provenance_sha256: declaration.provenance.sha256().clone(),
            provenance_byte_length: declaration.provenance.byte_length(),
            validator,
            availability: CandidateAvailability::Present,
        };
        if declaration.video != receipt.native_video {
            return Err(AttemptValueError::BundleMetadataMismatch);
        }
        receipt.validate_shape()?;
        Ok(receipt)
    }

    fn validate_shape(&self) -> Result<(), AttemptValueError> {
        let dimensions = self.plan.native_dimensions();
        let expected_native_frames = FrameDuration::new(i64::from(self.plan.native_frame_count()))
            .map_err(|_| AttemptValueError::BundleMetadataMismatch)?;
        if self.native_video.frames() != expected_native_frames
            || self.native_video.frame_rate() != self.plan.native_frame_rate()
            || self.native_video.width() != dimensions.width()
            || self.native_video.height() != dimensions.height()
            || self.sampled_video.frames() != self.plan.project_frames()
            || self.sampled_video.frame_rate() != self.plan.project_frame_rate()
            || self.sampled_video.width() != dimensions.width()
            || self.sampled_video.height() != dimensions.height()
            || (self.native_object == self.sampled_object
                && self.native_video != self.sampled_video)
            || self.provenance_object == self.native_object
            || self.provenance_object == self.sampled_object
            || self.native_byte_length == 0
            || self.provenance_byte_length == 0
        {
            return Err(AttemptValueError::BundleMetadataMismatch);
        }
        Ok(())
    }

    pub fn native_object(&self) -> &GeneratedObjectRef {
        &self.native_object
    }
    pub fn sampled_object(&self) -> &GeneratedObjectRef {
        &self.sampled_object
    }
    pub fn provenance_object(&self) -> &GeneratedObjectRef {
        &self.provenance_object
    }
    pub fn native_video(&self) -> &VideoSpec {
        &self.native_video
    }
    pub fn sampled_video(&self) -> &VideoSpec {
        &self.sampled_video
    }
    pub fn plan(&self) -> &BridgeGenerationPlan {
        &self.plan
    }
    pub fn provider(&self) -> &ProviderSelection {
        &self.provider
    }
    pub fn native_sha256(&self) -> &Sha256 {
        &self.native_sha256
    }
    pub const fn native_byte_length(&self) -> u64 {
        self.native_byte_length
    }
    pub fn provenance_sha256(&self) -> &Sha256 {
        &self.provenance_sha256
    }
    pub const fn provenance_byte_length(&self) -> u64 {
        self.provenance_byte_length
    }
    pub fn validator(&self) -> &ValidatorIdentity {
        &self.validator
    }
    pub const fn availability(&self) -> CandidateAvailability {
        self.availability
    }
}

impl CandidateValidationReceipt {
    pub fn new(
        staged_ref: ManagedCandidateRef,
        sha256: Sha256,
        byte_length: u64,
        video: VideoSpec,
        provider: ProviderSelection,
        validator: ValidatorIdentity,
    ) -> Result<Self, AttemptValueError> {
        if byte_length == 0 || i64::try_from(byte_length).is_err() {
            return Err(AttemptValueError::InvalidByteLength);
        }
        Ok(Self {
            staged_ref,
            sha256,
            byte_length,
            video,
            provider,
            validator,
            availability: CandidateAvailability::Present,
        })
    }

    pub fn staged_ref(&self) -> &ManagedCandidateRef {
        &self.staged_ref
    }

    pub fn sha256(&self) -> &Sha256 {
        &self.sha256
    }

    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }

    pub fn video(&self) -> &VideoSpec {
        &self.video
    }

    pub fn provider(&self) -> &ProviderSelection {
        &self.provider
    }

    pub fn validator(&self) -> &ValidatorIdentity {
        &self.validator
    }

    pub const fn availability(&self) -> CandidateAvailability {
        self.availability
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginGenerationAttempt {
    pub identity: MessageIdentity,
    pub cancellation_token: CancellationToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredGenerationAttempt {
    pub ordinal: u64,
    pub transition_sequence: u64,
    pub checkpoint: LifecycleCheckpoint,
    pub declared_candidate: Option<CandidateDeclaration>,
    pub receipt: Option<CandidateValidationReceipt>,
    pub bundle_receipt: Option<BundleValidationReceipt>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedGenerationCandidate {
    pub identity: MessageIdentity,
    pub receipt: CandidateValidationReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedGenerationBundle {
    pub identity: MessageIdentity,
    pub receipt: BundleValidationReceipt,
}

/// Store-level mutation result. `IgnoredDuringCancellation` records no write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptMutationOutcome {
    Applied,
    Duplicate,
    IgnoredDuringCancellation,
    CancellationAcknowledged,
    CompletionDiscardedDuringCancellation,
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(CREATE_TABLES)?;
    Ok(())
}

pub(crate) fn add_schema8_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE generation_bundle_receipts (
            request_id TEXT NOT NULL,
            attempt_id TEXT NOT NULL,
            bundle TEXT NOT NULL CHECK (json_valid(bundle)),
            availability TEXT NOT NULL CHECK (availability IN ('present','evicted')),
            PRIMARY KEY (request_id,attempt_id),
            FOREIGN KEY (request_id,attempt_id)
                REFERENCES generation_attempts(request_id,attempt_id)
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let invalid_attempts: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_attempts WHERE
            typeof(request_id)!='text' OR length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(attempt_id)!='text' OR length(CAST(attempt_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(ordinal)!='integer' OR ordinal<1 OR
            typeof(cancellation_token)!='text' OR length(CAST(cancellation_token AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(state)!='text' OR length(CAST(state AS BLOB)) NOT BETWEEN 5 AND 10 OR
            (worker_stage IS NOT NULL AND (typeof(worker_stage)!='text' OR length(CAST(worker_stage AS BLOB))>24 OR
             worker_stage NOT IN ('preflight','runtime_loading','model_loading','conditioning','inference','decoding','encoding','worker_validation'))) OR
            typeof(transition_sequence)!='integer' OR transition_sequence<1 OR
            (cancel_response IS NOT NULL AND (typeof(cancel_response)!='text' OR length(CAST(cancel_response AS BLOB))>9 OR
             cancel_response NOT IN ('cancelled','completed'))) OR
            (worker_candidate IS NOT NULL AND (typeof(worker_candidate)!='text' OR length(CAST(worker_candidate AS BLOB))>?2)) OR
            (failure_origin IS NOT NULL AND (typeof(failure_origin)!='text' OR length(CAST(failure_origin AS BLOB))>6 OR
             failure_origin NOT IN ('worker','host'))) OR
            (failure_code IS NOT NULL AND (typeof(failure_code)!='text' OR length(CAST(failure_code AS BLOB))>32)) OR
            (failure_detail IS NOT NULL AND (typeof(failure_detail)!='text' OR length(CAST(failure_detail AS BLOB))>?3))",
        params![
            MAX_PROTOCOL_ID_BYTES as i64,
            MAX_ATTEMPT_JSON_BYTES as i64,
            MAX_DIAGNOSTIC_BYTES as i64
        ],
        |row| row.get(0),
    )?;
    let invalid_receipts: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_candidate_receipts WHERE
            typeof(request_id)!='text' OR length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(attempt_id)!='text' OR length(CAST(attempt_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(staged_ref)!='text' OR length(CAST(staged_ref AS BLOB)) NOT BETWEEN 1 AND ?2 OR
            typeof(sha256)!='text' OR length(CAST(sha256 AS BLOB))!=64 OR
            typeof(byte_length)!='integer' OR byte_length<1 OR
            typeof(video)!='text' OR length(CAST(video AS BLOB))>?3 OR
            typeof(provider)!='text' OR length(CAST(provider AS BLOB))>?3 OR
            typeof(validator_id)!='text' OR length(CAST(validator_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(validator_version)!='text' OR length(CAST(validator_version AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(availability)!='text' OR length(CAST(availability AS BLOB)) NOT BETWEEN 7 AND 8",
        params![
            MAX_PROTOCOL_ID_BYTES as i64,
            MAX_MANAGED_REF_BYTES as i64,
            MAX_ATTEMPT_JSON_BYTES as i64
        ],
        |row| row.get(0),
    )?;
    let invalid_bundles: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_bundle_receipts WHERE
            typeof(request_id)!='text' OR length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(attempt_id)!='text' OR length(CAST(attempt_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(bundle)!='text' OR length(CAST(bundle AS BLOB))>?2 OR
            typeof(availability)!='text' OR availability NOT IN ('present','evicted')",
        params![MAX_PROTOCOL_ID_BYTES as i64, MAX_ATTEMPT_JSON_BYTES as i64],
        |row| row.get(0),
    )?;
    let invalid_heads: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_attempt_heads WHERE
            typeof(request_id)!='text' OR length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(high_water)!='integer' OR high_water<1 OR
            typeof(latest_attempt_id)!='text' OR length(CAST(latest_attempt_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            (selected_ready_attempt_id IS NOT NULL AND
             (typeof(selected_ready_attempt_id)!='text' OR length(CAST(selected_ready_attempt_id AS BLOB)) NOT BETWEEN 1 AND ?1))",
        [MAX_PROTOCOL_ID_BYTES as i64],
        |row| row.get(0),
    )?;
    if invalid_attempts != 0 || invalid_receipts != 0 || invalid_bundles != 0 || invalid_heads != 0
    {
        return Err(integrity(
            "stored generation attempt metadata exceeds its bounds or has the wrong type",
        ));
    }
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    for (query, message) in [
        (
            "SELECT EXISTS(SELECT 1 FROM generation_attempts GROUP BY request_id,attempt_id HAVING COUNT(*)>1)",
            "duplicate generation attempt identity",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_attempts GROUP BY request_id,ordinal HAVING COUNT(*)>1)",
            "duplicate generation attempt ordinal",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_attempts WHERE state IN ('queued','preflight','loading','running','validating','cancelling') GROUP BY request_id HAVING COUNT(*)>1)",
            "multiple nonterminal attempts for one request",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_attempt_heads GROUP BY request_id HAVING COUNT(*)>1)",
            "duplicate generation attempt head",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_candidate_receipts GROUP BY request_id,attempt_id HAVING COUNT(*)>1)",
            "duplicate generation candidate receipt",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_candidate_receipts GROUP BY staged_ref HAVING COUNT(*)>1)",
            "duplicate managed candidate reference",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_attempts a WHERE NOT EXISTS(
                SELECT 1 FROM generation_requests r WHERE r.request_id=a.request_id))",
            "generation attempt has no request",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_attempt_heads h WHERE NOT EXISTS(
                SELECT 1 FROM generation_requests r WHERE r.request_id=h.request_id))",
            "generation attempt head has no request",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_candidate_receipts r WHERE NOT EXISTS(
                SELECT 1 FROM generation_attempts a
                WHERE a.request_id=r.request_id AND a.attempt_id=r.attempt_id))",
            "generation candidate receipt has no attempt",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_bundle_receipts r WHERE NOT EXISTS(
                SELECT 1 FROM generation_attempts a
                WHERE a.request_id=r.request_id AND a.attempt_id=r.attempt_id))",
            "generation bundle receipt has no attempt",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_bundle_receipts GROUP BY request_id,attempt_id HAVING COUNT(*)>1)",
            "duplicate generation bundle receipt",
        ),
    ] {
        if connection.query_row(query, [], |row| row.get::<_, i64>(0))? != 0 {
            return Err(integrity(message));
        }
    }

    let mut statement = connection.prepare(&format!(
        "{BOUNDED_ATTEMPT_SELECT} ORDER BY request_id,ordinal"
    ))?;
    let mut rows = statement.query(params![
        MAX_PROTOCOL_ID_BYTES as i64,
        MAX_ATTEMPT_JSON_BYTES as i64,
        MAX_DIAGNOSTIC_BYTES as i64
    ])?;
    while let Some(row) = rows.next()? {
        let request_id = bounded_request_id(row.get(0)?)?;
        let request = read_request(connection, &request_id)?;
        let attempt = parse_attempt_row(connection, row, &request)?;
        let ready = attempt.checkpoint.state == JobState::Ready;
        if ready != (attempt.receipt.is_some() || attempt.bundle_receipt.is_some()) {
            return Err(integrity("Ready attempt and validation receipt disagree"));
        }
        if attempt.receipt.is_some() && attempt.bundle_receipt.is_some() {
            return Err(integrity("attempt has both legacy and bundle receipts"));
        }
        if let Some(receipt) = &attempt.receipt {
            validate_receipt(&request, attempt.declared_candidate.as_ref(), receipt).map_err(
                |_| integrity("candidate receipt does not match its request and declaration"),
            )?;
        }
        if let Some(receipt) = &attempt.bundle_receipt {
            validate_bundle_receipt(&request, attempt.declared_candidate.as_ref(), receipt)
                .map_err(|_| integrity("bundle receipt does not match its request"))?;
        }
    }

    let invalid_heads: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_attempt_heads h WHERE
            h.high_water != (SELECT MAX(a.ordinal) FROM generation_attempts a WHERE a.request_id=h.request_id) OR
            NOT EXISTS(SELECT 1 FROM generation_attempts a WHERE a.request_id=h.request_id AND a.attempt_id=h.latest_attempt_id AND a.ordinal=h.high_water) OR
            (h.selected_ready_attempt_id IS NOT NULL AND NOT EXISTS(
                SELECT 1 FROM generation_attempts a
                LEFT JOIN generation_candidate_receipts r USING(request_id,attempt_id)
                LEFT JOIN generation_bundle_receipts b USING(request_id,attempt_id)
                WHERE a.request_id=h.request_id AND a.attempt_id=h.selected_ready_attempt_id
                  AND a.state='ready'
                  AND ((r.availability='present' AND b.request_id IS NULL)
                    OR (b.availability='present' AND r.request_id IS NULL))))",
        [],
        |row| row.get(0),
    )?;
    let missing_heads: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_attempts a
         WHERE NOT EXISTS(SELECT 1 FROM generation_attempt_heads h WHERE h.request_id=a.request_id)",
        [],
        |row| row.get(0),
    )?;
    if invalid_heads != 0 || missing_heads != 0 {
        return Err(integrity("generation attempt head is inconsistent"));
    }
    Ok(())
}

impl ProjectStore {
    pub fn begin_generation_attempt(
        &mut self,
        input: BeginGenerationAttempt,
    ) -> Result<StoredGenerationAttempt, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let request = read_request(&transaction, &input.identity.request_id)?;
        if request.relevance != Relevance::Current {
            return Err(attempt_error(
                "only a current generation request may start an attempt",
            ));
        }
        if attempt_exists(&transaction, &input.identity)? {
            return Err(StoreError::GenerationAttemptReused {
                request: input.identity.request_id.as_str().into(),
                attempt: input.identity.attempt_id.as_str().into(),
            });
        }
        let has_nonterminal: bool = transaction.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM generation_attempts
                 WHERE request_id=?1 AND state IN ({NONTERMINAL_STATES}))"
            ),
            [input.identity.request_id.as_str()],
            |row| row.get::<_, i64>(0).map(|value| value != 0),
        )?;
        if has_nonterminal {
            return Err(attempt_error(
                "generation request already has a nonterminal attempt",
            ));
        }
        let head = read_head(&transaction, &input.identity.request_id)?;
        let ordinal = match head.as_ref().map(|head| head.high_water) {
            Some(MAX_SQL_COUNTER) => {
                return Err(StoreError::GenerationAttemptExhausted(
                    input.identity.request_id.as_str().into(),
                ));
            }
            Some(value) => value + 1,
            None => 1,
        };
        transaction.execute(
            "INSERT INTO generation_attempts(
                request_id,attempt_id,ordinal,cancellation_token,state,transition_sequence
             ) VALUES (?1,?2,?3,?4,'queued',1)",
            params![
                input.identity.request_id.as_str(),
                input.identity.attempt_id.as_str(),
                ordinal,
                input.cancellation_token.as_str(),
            ],
        )?;
        match head {
            Some(_) => {
                transaction.execute(
                    "UPDATE generation_attempt_heads SET high_water=?1,latest_attempt_id=?2
                     WHERE request_id=?3",
                    params![
                        ordinal,
                        input.identity.attempt_id.as_str(),
                        input.identity.request_id.as_str()
                    ],
                )?;
            }
            None => {
                transaction.execute(
                    "INSERT INTO generation_attempt_heads(
                        request_id,high_water,latest_attempt_id,selected_ready_attempt_id
                     ) VALUES (?1,?2,?3,NULL)",
                    params![
                        input.identity.request_id.as_str(),
                        ordinal,
                        input.identity.attempt_id.as_str()
                    ],
                )?;
            }
        }
        let result = read_attempt(&transaction, &input.identity)?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn record_generation_worker_message(
        &mut self,
        message: &WorkerMessage,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        if matches!(message, WorkerMessage::Progress { .. }) {
            return Err(StoreError::GenerationProgressNotPersistent);
        }
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stored = read_attempt(&transaction, message.identity())?;
        if duplicate_worker_terminal(&stored, message) {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        let mut lifecycle = JobLifecycle::from_checkpoint(
            stored.checkpoint.clone(),
            read_request(&transaction, &message.identity().request_id)?.relevance,
        )
        .map_err(lifecycle_error)?;
        let outcome = lifecycle
            .apply_worker_message(message)
            .map_err(lifecycle_error)?;
        let mutation = match outcome {
            WorkerEventOutcome::Applied => AttemptMutationOutcome::Applied,
            WorkerEventOutcome::Duplicate => AttemptMutationOutcome::Duplicate,
            WorkerEventOutcome::IgnoredDuringCancellation => {
                AttemptMutationOutcome::IgnoredDuringCancellation
            }
            WorkerEventOutcome::CancellationAcknowledged => {
                AttemptMutationOutcome::CancellationAcknowledged
            }
            WorkerEventOutcome::CompletionDiscardedDuringCancellation => {
                AttemptMutationOutcome::CompletionDiscardedDuringCancellation
            }
        };
        if !matches!(
            mutation,
            AttemptMutationOutcome::Duplicate | AttemptMutationOutcome::IgnoredDuringCancellation
        ) {
            stored.checkpoint = lifecycle.checkpoint();
            match message {
                WorkerMessage::Completed { candidate, .. } => {
                    stored.declared_candidate =
                        Some(CandidateDeclaration::SampledV1(candidate.clone()));
                }
                WorkerMessage::CompletedBridge { candidate, .. } => {
                    stored.declared_candidate =
                        Some(CandidateDeclaration::NativeBridgeV2(candidate.clone()));
                }
                _ => {}
            }
            write_attempt(&transaction, &stored)?;
        }
        transaction.commit()?;
        Ok(mutation)
    }

    pub fn request_generation_attempt_cancel(
        &mut self,
        identity: &MessageIdentity,
        token: &CancellationToken,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stored = read_attempt(&transaction, identity)?;
        let relevance = read_request(&transaction, &identity.request_id)?.relevance;
        let mut lifecycle = JobLifecycle::from_checkpoint(stored.checkpoint.clone(), relevance)
            .map_err(lifecycle_error)?;
        let outcome = lifecycle
            .request_cancel(identity, token)
            .map_err(lifecycle_error)?;
        if outcome == WorkerEventOutcome::Duplicate {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        stored.checkpoint = lifecycle.checkpoint();
        write_attempt(&transaction, &stored)?;
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    pub fn finish_generation_attempt_cancelled(
        &mut self,
        identity: &MessageIdentity,
        token: &CancellationToken,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stored = read_attempt(&transaction, identity)?;
        let duplicate = stored.checkpoint.state == JobState::Cancelled;
        let relevance = read_request(&transaction, &identity.request_id)?.relevance;
        let mut lifecycle = JobLifecycle::from_checkpoint(stored.checkpoint.clone(), relevance)
            .map_err(lifecycle_error)?;
        lifecycle
            .host_cancelled(identity, token)
            .map_err(lifecycle_error)?;
        if duplicate {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        stored.checkpoint = lifecycle.checkpoint();
        write_attempt(&transaction, &stored)?;
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    pub fn fail_generation_attempt(
        &mut self,
        identity: &MessageIdentity,
        failure: HostFailure,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stored = read_attempt(&transaction, identity)?;
        if stored.checkpoint.state == JobState::Failed
            && stored.checkpoint.failure == Some(JobFailure::Host(failure.clone()))
        {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        let relevance = read_request(&transaction, &identity.request_id)?.relevance;
        let mut lifecycle = JobLifecycle::from_checkpoint(stored.checkpoint.clone(), relevance)
            .map_err(lifecycle_error)?;
        lifecycle
            .host_failed(identity, failure)
            .map_err(lifecycle_error)?;
        stored.checkpoint = lifecycle.checkpoint();
        write_attempt(&transaction, &stored)?;
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    pub fn record_generation_candidate_ready(
        &mut self,
        identity: &MessageIdentity,
        expected_candidate: &CandidateManifest,
        receipt: CandidateValidationReceipt,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stored = read_attempt(&transaction, identity)?;
        let request = read_request(&transaction, &identity.request_id)?;
        if request.bridge_plan.is_some() {
            return Err(attempt_error(
                "legacy candidate readiness cannot be used for a bridge request",
            ));
        }
        if stored.checkpoint.state == JobState::Ready {
            if stored.receipt.as_ref() == Some(&receipt)
                && stored.declared_candidate.as_ref()
                    == Some(&CandidateDeclaration::SampledV1(expected_candidate.clone()))
            {
                return Ok(AttemptMutationOutcome::Duplicate);
            }
            return Err(attempt_error(
                "Ready attempt has a different validation receipt",
            ));
        }
        if stored.declared_candidate.as_ref()
            != Some(&CandidateDeclaration::SampledV1(expected_candidate.clone()))
        {
            return Err(attempt_error(
                "worker candidate does not match the validating attempt",
            ));
        }
        if receipt.availability != CandidateAvailability::Present {
            return Err(attempt_error("new candidate receipt is not present"));
        }
        let expected = CandidateDeclaration::SampledV1(expected_candidate.clone());
        validate_receipt(&request, Some(&expected), &receipt)?;
        let mut lifecycle =
            JobLifecycle::from_checkpoint(stored.checkpoint.clone(), request.relevance)
                .map_err(lifecycle_error)?;
        lifecycle
            .host_validation_succeeded(identity, expected_candidate)
            .map_err(lifecycle_error)?;
        insert_receipt(&transaction, identity, &receipt)?;
        stored.checkpoint = lifecycle.checkpoint();
        stored.receipt = Some(receipt);
        write_attempt(&transaction, &stored)?;
        let is_latest: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM generation_attempt_heads
             WHERE request_id=?1 AND latest_attempt_id=?2)",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
            |row| row.get::<_, i64>(0).map(|value| value != 0),
        )?;
        if !is_latest {
            return Err(attempt_error(
                "only the latest attempt may become the selected candidate",
            ));
        }
        if request.relevance == Relevance::Current {
            transaction.execute(
                "UPDATE generation_attempt_heads SET selected_ready_attempt_id=?1
                 WHERE request_id=?2",
                params![identity.attempt_id.as_str(), identity.request_id.as_str()],
            )?;
        }
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    /// Records a host-qualified V2 bridge bundle. The caller supplies independent
    /// media/provenance qualification. This method verifies all three published
    /// objects before its transaction, then rechecks request/declaration bindings.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub fn record_generation_bundle_ready(
        &mut self,
        identity: &MessageIdentity,
        declaration: &NativeCandidateManifest,
        receipt: BundleValidationReceipt,
        limits: crate::generated_media::GeneratedMediaLimits,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        // Verify the three immutable objects before opening the SQLite
        // transaction. A missing or corrupt object therefore cannot leave a
        // Ready receipt behind, and the bound is supplied by the host rather
        // than inferred from untrusted receipt metadata.
        drop(self.snapshot_generated_object(receipt.native_object(), limits)?);
        drop(self.snapshot_generated_object(receipt.sampled_object(), limits)?);
        drop(self.snapshot_generated_object(receipt.provenance_object(), limits)?);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stored = read_attempt(&transaction, identity)?;
        let request = read_request(&transaction, &identity.request_id)?;
        if request.bridge_plan.is_none() {
            return Err(attempt_error(
                "bridge bundle readiness requires a V2 generation request",
            ));
        }
        let expected = CandidateDeclaration::NativeBridgeV2(declaration.clone());
        if stored.declared_candidate.as_ref() != Some(&expected) {
            return Err(attempt_error(
                "bridge declaration does not match the validating attempt",
            ));
        }
        if receipt.availability != CandidateAvailability::Present {
            return Err(attempt_error("new bundle receipt is not present"));
        }
        validate_bundle_receipt(&request, Some(&expected), &receipt)?;
        if stored.checkpoint.state == JobState::Ready {
            if stored.bundle_receipt.as_ref() == Some(&receipt) {
                return Ok(AttemptMutationOutcome::Duplicate);
            }
            return Err(attempt_error(
                "Ready attempt has a different bundle validation receipt",
            ));
        }
        if stored.checkpoint.state != JobState::Validating {
            return Err(attempt_error(
                "bridge bundle readiness requires a validating attempt",
            ));
        }
        let mut lifecycle =
            JobLifecycle::from_checkpoint(stored.checkpoint.clone(), request.relevance)
                .map_err(lifecycle_error)?;
        lifecycle
            .host_bundle_validation_succeeded(identity, declaration)
            .map_err(lifecycle_error)?;
        insert_bundle_receipt(&transaction, identity, &receipt)?;
        stored.checkpoint = lifecycle.checkpoint();
        stored.bundle_receipt = Some(receipt);
        write_attempt(&transaction, &stored)?;
        let is_latest: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM generation_attempt_heads
             WHERE request_id=?1 AND latest_attempt_id=?2)",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
            |row| row.get::<_, i64>(0).map(|value| value != 0),
        )?;
        if !is_latest {
            return Err(attempt_error(
                "only the latest attempt may become the selected bridge bundle",
            ));
        }
        if request.relevance == Relevance::Current {
            transaction.execute(
                "UPDATE generation_attempt_heads SET selected_ready_attempt_id=?1
                 WHERE request_id=?2",
                params![identity.attempt_id.as_str(), identity.request_id.as_str()],
            )?;
        }
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    pub fn select_generation_variant(
        &mut self,
        identity: &MessageIdentity,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let request = read_request(&transaction, &identity.request_id)?;
        if request.bridge_plan.is_some() {
            return Err(attempt_error(
                "legacy selection cannot be used for a bridge request",
            ));
        }
        if request.relevance != Relevance::Current {
            return Err(attempt_error(
                "stale or detached requests cannot select a candidate",
            ));
        }
        let stored = read_attempt(&transaction, identity)?;
        if stored.bundle_receipt.is_some() {
            return Err(attempt_error(
                "legacy selection cannot be used for a bridge bundle",
            ));
        }
        if stored.checkpoint.state != JobState::Ready
            || stored
                .receipt
                .as_ref()
                .is_none_or(|receipt| receipt.availability != CandidateAvailability::Present)
        {
            return Err(attempt_error(
                "selected attempt is not a present Ready candidate",
            ));
        }
        if stored.selected {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        transaction.execute(
            "UPDATE generation_attempt_heads SET selected_ready_attempt_id=?1 WHERE request_id=?2",
            params![identity.attempt_id.as_str(), identity.request_id.as_str()],
        )?;
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    /// Selects a retained, host-validated V2 bundle for comparison.
    ///
    /// This changes operational metadata only. Callers must still verify the
    /// three generated objects and perform a separate authored acceptance
    /// transaction before the bundle can affect the document.
    pub fn select_generation_bundle_variant(
        &mut self,
        identity: &MessageIdentity,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let request = read_request(&transaction, &identity.request_id)?;
        if request.bridge_plan.is_none() {
            return Err(attempt_error(
                "bridge bundle selection requires a V2 generation request",
            ));
        }
        if request.relevance != Relevance::Current {
            return Err(attempt_error(
                "stale or detached requests cannot select a bridge bundle",
            ));
        }
        let stored = read_attempt(&transaction, identity)?;
        if stored.receipt.is_some() {
            return Err(attempt_error(
                "bridge bundle selection cannot use a legacy candidate",
            ));
        }
        if stored.checkpoint.state != JobState::Ready
            || stored
                .bundle_receipt
                .as_ref()
                .is_none_or(|receipt| receipt.availability != CandidateAvailability::Present)
        {
            return Err(attempt_error(
                "selected attempt is not a present Ready bridge bundle",
            ));
        }
        if stored.selected {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        transaction.execute(
            "UPDATE generation_attempt_heads SET selected_ready_attempt_id=?1 WHERE request_id=?2",
            params![identity.attempt_id.as_str(), identity.request_id.as_str()],
        )?;
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    pub fn mark_generation_bundle_evicted(
        &mut self,
        identity: &MessageIdentity,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = read_attempt(&transaction, identity)?;
        let Some(mut receipt) = stored.bundle_receipt else {
            return Err(attempt_error(
                "attempt has no bundle validation receipt to evict",
            ));
        };
        if receipt.availability == CandidateAvailability::Evicted {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        receipt.availability = CandidateAvailability::Evicted;
        let bundle = serde_json::to_string(&receipt)?;
        if bundle.len() > MAX_ATTEMPT_JSON_BYTES {
            return Err(attempt_error(
                "bundle validation receipt exceeds the persistence limit",
            ));
        }
        let updated = transaction.execute(
            "UPDATE generation_bundle_receipts SET bundle=?1, availability='evicted'
             WHERE request_id=?2 AND attempt_id=?3",
            params![
                bundle,
                identity.request_id.as_str(),
                identity.attempt_id.as_str()
            ],
        )?;
        if updated != 1 {
            return Err(attempt_error("bundle receipt disappeared during eviction"));
        }
        transaction.execute(
            "UPDATE generation_attempt_heads SET selected_ready_attempt_id=NULL
             WHERE request_id=?1 AND selected_ready_attempt_id=?2",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
        )?;
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    pub fn mark_generation_candidate_evicted(
        &mut self,
        identity: &MessageIdentity,
    ) -> Result<AttemptMutationOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = read_attempt(&transaction, identity)?;
        let Some(receipt) = stored.receipt else {
            return Err(attempt_error("attempt has no validation receipt to evict"));
        };
        if receipt.availability == CandidateAvailability::Evicted {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        transaction.execute(
            "UPDATE generation_candidate_receipts SET availability='evicted'
             WHERE request_id=?1 AND attempt_id=?2",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
        )?;
        transaction.execute(
            "UPDATE generation_attempt_heads SET selected_ready_attempt_id=NULL
             WHERE request_id=?1 AND selected_ready_attempt_id=?2",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
        )?;
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    pub fn generation_attempt(
        &self,
        identity: &MessageIdentity,
    ) -> Result<Option<StoredGenerationAttempt>, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let result = read_attempt_optional(&transaction, identity)?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn generation_attempts(
        &self,
        request_id: &RequestId,
        after_ordinal: u64,
        limit: usize,
    ) -> Result<Vec<StoredGenerationAttempt>, StoreError> {
        if limit == 0 || limit > MAX_ATTEMPT_PAGE {
            return Err(attempt_error("attempt page size must be between 1 and 256"));
        }
        let after = i64::try_from(after_ordinal)
            .map_err(|_| attempt_error("attempt page cursor exceeds SQLite"))?;
        let transaction = self.connection.unchecked_transaction()?;
        let request = read_request(&transaction, request_id)?;
        let mut statement = transaction.prepare(&format!(
            "{BOUNDED_ATTEMPT_SELECT} WHERE request_id=?4 AND ordinal>?5 ORDER BY ordinal LIMIT ?6"
        ))?;
        let mut rows = statement.query(params![
            MAX_PROTOCOL_ID_BYTES as i64,
            MAX_ATTEMPT_JSON_BYTES as i64,
            MAX_DIAGNOSTIC_BYTES as i64,
            request_id.as_str(),
            after,
            limit as i64
        ])?;
        let mut attempts = Vec::with_capacity(limit);
        while let Some(row) = rows.next()? {
            attempts.push(parse_attempt_row(&transaction, row, &request)?);
        }
        drop(rows);
        drop(statement);
        transaction.commit()?;
        Ok(attempts)
    }

    /// Returns persisted eligibility metadata only. The caller must still
    /// verify the managed file and use an authored acceptance transaction.
    pub fn selected_generation_candidate(
        &self,
        request_id: &RequestId,
    ) -> Result<Option<SelectedGenerationCandidate>, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let request = read_request(&transaction, request_id)?;
        if request.bridge_plan.is_some() {
            return Ok(None);
        }
        if request.relevance != Relevance::Current {
            return Ok(None);
        }
        let selected: Option<String> = transaction
            .query_row(
                "SELECT selected_ready_attempt_id FROM generation_attempt_heads WHERE request_id=?1",
                [request_id.as_str()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let identity = MessageIdentity::new(
            request_id.clone(),
            AttemptId::new(selected).map_err(|_| integrity("invalid selected attempt ID"))?,
        );
        let attempt = read_attempt(&transaction, &identity)?;
        let Some(receipt) = attempt.receipt else {
            return Err(integrity("selected attempt has no validation receipt"));
        };
        if attempt.checkpoint.state != JobState::Ready
            || receipt.availability != CandidateAvailability::Present
        {
            return Err(integrity(
                "selected attempt is not an available Ready candidate",
            ));
        }
        transaction.commit()?;
        Ok(Some(SelectedGenerationCandidate { identity, receipt }))
    }

    pub fn selected_generation_bundle(
        &self,
        request_id: &RequestId,
    ) -> Result<Option<SelectedGenerationBundle>, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let request = read_request(&transaction, request_id)?;
        if request.bridge_plan.is_none() || request.relevance != Relevance::Current {
            return Ok(None);
        }
        let selected: Option<String> = transaction
            .query_row(
                "SELECT selected_ready_attempt_id FROM generation_attempt_heads WHERE request_id=?1",
                [request_id.as_str()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let identity = MessageIdentity::new(
            request_id.clone(),
            AttemptId::new(selected).map_err(|_| integrity("invalid selected attempt ID"))?,
        );
        let attempt = read_attempt(&transaction, &identity)?;
        let Some(receipt) = attempt.bundle_receipt else {
            return Err(integrity("selected bridge attempt has no bundle receipt"));
        };
        if attempt.checkpoint.state != JobState::Ready
            || receipt.availability != CandidateAvailability::Present
        {
            return Err(integrity(
                "selected bridge attempt is not an available Ready bundle",
            ));
        }
        transaction.commit()?;
        Ok(Some(SelectedGenerationBundle { identity, receipt }))
    }
}

pub(crate) fn recover_nonterminal(connection: &mut Connection) -> Result<usize, StoreError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = transaction.execute(
        &format!(
            "UPDATE generation_attempts SET
                state='failed',transition_sequence=transition_sequence+1,
                cancel_response=NULL,failure_origin='host',failure_code='interrupted',
                failure_detail='worker ownership ended before a terminal state was recorded'
             WHERE state IN ({NONTERMINAL_STATES})"
        ),
        [],
    )?;
    transaction.commit()?;
    Ok(changed)
}

#[derive(Debug, Clone)]
struct RequestMetadata {
    binding: TargetBinding,
    constraints: HoldConstraints,
    provider: ProviderSelection,
    bridge_plan: Option<BridgeGenerationPlan>,
    relevance: Relevance,
}

#[derive(Debug)]
struct AttemptHead {
    high_water: i64,
}

fn read_request(
    connection: &Connection,
    request_id: &RequestId,
) -> Result<RequestMetadata, StoreError> {
    let row = connection
        .query_row(
            "SELECT project_id,hold_id,request_version,context_sha256,constraints,provider,bridge_plan,relevance
             FROM generation_requests WHERE request_id=?1",
            [request_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((project, hold, version, context, constraints, provider, bridge_plan, relevance)) =
        row
    else {
        return Err(attempt_error("generation request does not exist"));
    };
    if constraints.len() > MAX_ATTEMPT_JSON_BYTES
        || provider.len() > MAX_ATTEMPT_JSON_BYTES
        || bridge_plan
            .as_ref()
            .is_some_and(|value| value.len() > MAX_ATTEMPT_JSON_BYTES)
    {
        return Err(integrity(
            "stored generation request exceeds attempt parsing bounds",
        ));
    }
    let constraints: HoldConstraints = serde_json::from_str(&constraints)
        .map_err(|_| integrity("invalid generation constraints"))?;
    let provider: ProviderSelection =
        serde_json::from_str(&provider).map_err(|_| integrity("invalid generation provider"))?;
    let bridge_plan = bridge_plan
        .map(|value| {
            serde_json::from_str(&value).map_err(|_| integrity("invalid bridge generation plan"))
        })
        .transpose()?;
    Ok(RequestMetadata {
        binding: TargetBinding {
            project_id: ProjectId::new(project)
                .map_err(|_| integrity("invalid generation project ID"))?,
            hold_id: NodeId::new(hold).map_err(|_| integrity("invalid generation Hold ID"))?,
            request_version: RequestVersion::new(
                u64::try_from(version)
                    .map_err(|_| integrity("invalid generation request version"))?,
            )
            .map_err(|_| integrity("invalid generation request version"))?,
            context_sha256: Sha256::new(context)
                .map_err(|_| integrity("invalid generation context hash"))?,
        },
        constraints,
        provider,
        bridge_plan,
        relevance: parse_relevance(&relevance)?,
    })
}

fn read_head(
    connection: &Connection,
    request_id: &RequestId,
) -> Result<Option<AttemptHead>, StoreError> {
    connection
        .query_row(
            "SELECT high_water FROM generation_attempt_heads WHERE request_id=?1",
            [request_id.as_str()],
            |row| {
                Ok(AttemptHead {
                    high_water: row.get(0)?,
                })
            },
        )
        .optional()
        .map_err(StoreError::from)
}

fn attempt_exists(connection: &Connection, identity: &MessageIdentity) -> Result<bool, StoreError> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM generation_attempts WHERE request_id=?1 AND attempt_id=?2",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some())
}

const BOUNDED_ATTEMPT_SELECT: &str = "SELECT
    CASE WHEN typeof(request_id)='text' AND length(CAST(request_id AS BLOB)) BETWEEN 1 AND ?1 THEN request_id END,
    CASE WHEN typeof(attempt_id)='text' AND length(CAST(attempt_id AS BLOB)) BETWEEN 1 AND ?1 THEN attempt_id END,
    CASE WHEN typeof(ordinal)='integer' AND ordinal>=1 THEN ordinal END,
    CASE WHEN typeof(cancellation_token)='text' AND length(CAST(cancellation_token AS BLOB)) BETWEEN 1 AND ?1 THEN cancellation_token END,
    CASE WHEN state IN ('queued','preflight','loading','running','validating','ready','failed','cancelling','cancelled') THEN state END,
    CASE WHEN worker_stage IS NULL THEN NULL
         WHEN worker_stage IN ('preflight','runtime_loading','model_loading','conditioning','inference','decoding','encoding','worker_validation') THEN worker_stage
         ELSE '__invalid__' END,
    CASE WHEN typeof(transition_sequence)='integer' AND transition_sequence>=1 THEN transition_sequence END,
    CASE WHEN cancel_response IS NULL THEN NULL
         WHEN cancel_response IN ('cancelled','completed') THEN cancel_response
         ELSE '__invalid__' END,
    CASE WHEN worker_candidate IS NULL OR (typeof(worker_candidate)='text' AND length(CAST(worker_candidate AS BLOB))<=?2) THEN worker_candidate END,
    CASE WHEN failure_origin IS NULL THEN NULL
         WHEN failure_origin IN ('worker','host') THEN failure_origin
         ELSE '__invalid__' END,
    CASE WHEN failure_code IS NULL OR (typeof(failure_code)='text' AND length(CAST(failure_code AS BLOB))<=32) THEN failure_code END,
    CASE WHEN failure_detail IS NULL OR (typeof(failure_detail)='text' AND length(CAST(failure_detail AS BLOB))<=?3) THEN failure_detail END
 FROM generation_attempts";

fn read_attempt_optional(
    connection: &Connection,
    identity: &MessageIdentity,
) -> Result<Option<StoredGenerationAttempt>, StoreError> {
    let request = read_request(connection, &identity.request_id)?;
    let mut statement = connection.prepare(&format!(
        "{BOUNDED_ATTEMPT_SELECT} WHERE request_id=?4 AND attempt_id=?5"
    ))?;
    let mut rows = statement.query(params![
        MAX_PROTOCOL_ID_BYTES as i64,
        MAX_ATTEMPT_JSON_BYTES as i64,
        MAX_DIAGNOSTIC_BYTES as i64,
        identity.request_id.as_str(),
        identity.attempt_id.as_str()
    ])?;
    let result = rows
        .next()?
        .map(|row| parse_attempt_row(connection, row, &request))
        .transpose()?;
    if rows.next()?.is_some() {
        return Err(integrity("duplicate generation attempt identity"));
    }
    Ok(result)
}

fn read_attempt(
    connection: &Connection,
    identity: &MessageIdentity,
) -> Result<StoredGenerationAttempt, StoreError> {
    read_attempt_optional(connection, identity)?.ok_or_else(|| {
        StoreError::GenerationAttemptNotFound {
            request: identity.request_id.as_str().into(),
            attempt: identity.attempt_id.as_str().into(),
        }
    })
}

fn parse_attempt_row(
    connection: &Connection,
    row: &Row<'_>,
    request: &RequestMetadata,
) -> Result<StoredGenerationAttempt, StoreError> {
    let request_id = bounded_request_id(row.get(0)?)?;
    let attempt_id = AttemptId::new(required(row.get(1)?, "attempt ID")?)
        .map_err(|_| integrity("invalid generation attempt ID"))?;
    let ordinal = positive_u64(row.get(2)?, "attempt ordinal")?;
    let cancellation_token = CancellationToken::new(required(row.get(3)?, "cancellation token")?)
        .map_err(|_| integrity("invalid generation cancellation token"))?;
    let state = parse_state(&required(row.get(4)?, "attempt state")?)?;
    let worker_stage = row
        .get::<_, Option<String>>(5)?
        .map(|value| parse_stage(&value))
        .transpose()?;
    let transition_sequence = positive_u64(row.get(6)?, "transition sequence")?;
    let cancel_response: Option<String> = row.get(7)?;
    let worker_candidate_json: Option<String> = row.get(8)?;
    let declared_candidate = worker_candidate_json
        .map(|json| -> Result<CandidateDeclaration, StoreError> {
            if request.bridge_plan.is_some() {
                let candidate: NativeCandidateManifest = serde_json::from_str(&json)
                    .map_err(|_| integrity("invalid native bridge candidate manifest"))?;
                Ok(CandidateDeclaration::NativeBridgeV2(candidate))
            } else {
                let candidate: CandidateManifest = serde_json::from_str(&json)
                    .map_err(|_| integrity("invalid worker candidate manifest"))?;
                Ok(CandidateDeclaration::SampledV1(candidate))
            }
        })
        .transpose()?;
    if declared_candidate.is_some()
        && matches!(
            state,
            JobState::Queued | JobState::Preflight | JobState::Loading | JobState::Running
        )
    {
        return Err(integrity(
            "worker candidate appears before completion or cancellation",
        ));
    }
    let failure_origin: Option<String> = row.get(9)?;
    let failure_code: Option<String> = row.get(10)?;
    let failure_detail: Option<String> = row.get(11)?;
    let failure = parse_failure(failure_origin, failure_code, failure_detail)?;
    let cancellation_acknowledgement = match cancel_response.as_deref() {
        None => None,
        Some("cancelled") => Some(CancellationAcknowledgement::Cancelled),
        Some("completed") => Some(CancellationAcknowledgement::CompletionDiscarded(Box::new(
            declared_candidate
                .clone()
                .ok_or_else(|| integrity("discarded completion is missing its candidate"))?,
        ))),
        Some(_) => return Err(integrity("invalid cancellation response")),
    };
    let identity = MessageIdentity::new(request_id.clone(), attempt_id);
    let protocol = if request.bridge_plan.is_some() {
        ProtocolVersion::V2
    } else {
        ProtocolVersion::V1
    };
    let completion = matches!(state, JobState::Validating | JobState::Ready)
        .then(|| declared_candidate.clone())
        .flatten();
    let checkpoint = LifecycleCheckpoint {
        identity: identity.clone(),
        cancellation_token,
        target: request.binding.clone(),
        protocol,
        state,
        worker_stage,
        cancellation_acknowledgement,
        completion,
        failure,
    };
    JobLifecycle::from_checkpoint(checkpoint.clone(), request.relevance)
        .map_err(|error| integrity(&format!("invalid generation lifecycle checkpoint: {error}")))?;
    let receipt = read_receipt(connection, &identity)?;
    let selected: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM generation_attempt_heads
         WHERE request_id=?1 AND selected_ready_attempt_id=?2)",
        params![request_id.as_str(), identity.attempt_id.as_str()],
        |row| row.get::<_, i64>(0).map(|value| value != 0),
    )?;
    Ok(StoredGenerationAttempt {
        ordinal,
        transition_sequence,
        checkpoint,
        declared_candidate,
        receipt,
        bundle_receipt: read_bundle_receipt(connection, &identity)?,
        selected,
    })
}

fn write_attempt(
    connection: &Connection,
    attempt: &StoredGenerationAttempt,
) -> Result<(), StoreError> {
    if attempt.transition_sequence == u64::try_from(MAX_SQL_COUNTER).expect("i64 max fits u64") {
        return Err(attempt_error("generation transition sequence is exhausted"));
    }
    let next_sequence = attempt.transition_sequence + 1;
    let stage = attempt.checkpoint.worker_stage.map(stage_text);
    let cancel_response = match &attempt.checkpoint.cancellation_acknowledgement {
        None => None,
        Some(CancellationAcknowledgement::Cancelled) => Some("cancelled"),
        Some(CancellationAcknowledgement::CompletionDiscarded(_)) => Some("completed"),
    };
    let candidate = attempt
        .declared_candidate
        .as_ref()
        .map(serialize_candidate_declaration)
        .transpose()?;
    let (failure_origin, failure_code, failure_detail) =
        failure_columns(attempt.checkpoint.failure.as_ref());
    let changed = connection.execute(
        "UPDATE generation_attempts SET
            state=?1,worker_stage=?2,transition_sequence=?3,cancel_response=?4,
            worker_candidate=?5,failure_origin=?6,failure_code=?7,failure_detail=?8
         WHERE request_id=?9 AND attempt_id=?10 AND transition_sequence=?11",
        params![
            state_text(attempt.checkpoint.state),
            stage,
            i64::try_from(next_sequence)
                .map_err(|_| attempt_error("transition sequence exceeds SQLite"))?,
            cancel_response,
            candidate,
            failure_origin,
            failure_code,
            failure_detail,
            attempt.checkpoint.identity.request_id.as_str(),
            attempt.checkpoint.identity.attempt_id.as_str(),
            i64::try_from(attempt.transition_sequence)
                .map_err(|_| attempt_error("transition sequence exceeds SQLite"))?
        ],
    )?;
    if changed != 1 {
        return Err(attempt_error("generation attempt changed concurrently"));
    }
    Ok(())
}

fn serialize_candidate_declaration(
    declaration: &CandidateDeclaration,
) -> Result<String, StoreError> {
    match declaration {
        CandidateDeclaration::SampledV1(candidate) => bounded_json(candidate, "worker candidate"),
        CandidateDeclaration::NativeBridgeV2(candidate) => {
            bounded_json(candidate, "native bridge candidate")
        }
    }
}

fn insert_receipt(
    connection: &Connection,
    identity: &MessageIdentity,
    receipt: &CandidateValidationReceipt,
) -> Result<(), StoreError> {
    let staged_ref_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM generation_candidate_receipts WHERE staged_ref=?1)",
        [receipt.staged_ref.as_str()],
        |row| row.get::<_, i64>(0).map(|value| value != 0),
    )?;
    if staged_ref_exists {
        return Err(attempt_error(
            "managed candidate reference has already been used",
        ));
    }
    connection.execute(
        "INSERT INTO generation_candidate_receipts(
            request_id,attempt_id,staged_ref,sha256,byte_length,video,provider,
            validator_id,validator_version,availability
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'present')",
        params![
            identity.request_id.as_str(),
            identity.attempt_id.as_str(),
            receipt.staged_ref.as_str(),
            receipt.sha256.as_str(),
            i64::try_from(receipt.byte_length)
                .map_err(|_| attempt_error("candidate length exceeds SQLite"))?,
            bounded_json(&receipt.video, "validated video")?,
            bounded_json(&receipt.provider, "validated provider")?,
            receipt.validator.id(),
            receipt.validator.version(),
        ],
    )?;
    Ok(())
}

fn insert_bundle_receipt(
    connection: &Connection,
    identity: &MessageIdentity,
    receipt: &BundleValidationReceipt,
) -> Result<(), StoreError> {
    let bundle = serde_json::to_string(receipt)?;
    if bundle.len() > MAX_ATTEMPT_JSON_BYTES {
        return Err(attempt_error(
            "bundle validation receipt exceeds the persistence limit",
        ));
    }
    connection.execute(
        "INSERT INTO generation_bundle_receipts(
            request_id,attempt_id,bundle,availability
         ) VALUES (?1,?2,?3,'present')",
        params![
            identity.request_id.as_str(),
            identity.attempt_id.as_str(),
            bundle
        ],
    )?;
    Ok(())
}

fn read_receipt(
    connection: &Connection,
    identity: &MessageIdentity,
) -> Result<Option<CandidateValidationReceipt>, StoreError> {
    let row = connection
        .query_row(
            "SELECT staged_ref,sha256,byte_length,video,provider,validator_id,validator_version,availability
             FROM generation_candidate_receipts WHERE request_id=?1 AND attempt_id=?2",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((
        staged_ref,
        hash,
        length,
        video,
        provider,
        validator_id,
        validator_version,
        availability,
    )) = row
    else {
        return Ok(None);
    };
    if staged_ref.len() > MAX_MANAGED_REF_BYTES
        || video.len() > MAX_ATTEMPT_JSON_BYTES
        || provider.len() > MAX_ATTEMPT_JSON_BYTES
    {
        return Err(integrity("stored candidate receipt exceeds its bounds"));
    }
    let availability = match availability.as_str() {
        "present" => CandidateAvailability::Present,
        "evicted" => CandidateAvailability::Evicted,
        _ => return Err(integrity("invalid candidate availability")),
    };
    Ok(Some(CandidateValidationReceipt {
        staged_ref: ManagedCandidateRef::new(staged_ref)
            .map_err(|_| integrity("invalid managed candidate reference"))?,
        sha256: Sha256::new(hash).map_err(|_| integrity("invalid candidate SHA-256"))?,
        byte_length: u64::try_from(length)
            .map_err(|_| integrity("invalid candidate byte length"))?,
        video: serde_json::from_str(&video).map_err(|_| integrity("invalid validated video"))?,
        provider: serde_json::from_str(&provider)
            .map_err(|_| integrity("invalid validated provider"))?,
        validator: ValidatorIdentity::new(validator_id, validator_version)
            .map_err(|_| integrity("invalid validator identity"))?,
        availability,
    }))
}

fn read_bundle_receipt(
    connection: &Connection,
    identity: &MessageIdentity,
) -> Result<Option<BundleValidationReceipt>, StoreError> {
    let row = connection
        .query_row(
            "SELECT bundle,availability FROM generation_bundle_receipts
             WHERE request_id=?1 AND attempt_id=?2",
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((bundle, availability)) = row else {
        return Ok(None);
    };
    if bundle.len() > MAX_ATTEMPT_JSON_BYTES {
        return Err(integrity(
            "stored bundle validation receipt exceeds its bounds",
        ));
    }
    let mut receipt: BundleValidationReceipt = serde_json::from_str(&bundle)
        .map_err(|_| integrity("invalid bundle validation receipt"))?;
    let expected = parse_availability(&availability)?;
    if receipt.availability != expected {
        return Err(integrity(
            "bundle receipt availability disagrees with its row",
        ));
    }
    receipt.availability = expected;
    Ok(Some(receipt))
}

fn validate_receipt(
    request: &RequestMetadata,
    candidate: Option<&CandidateDeclaration>,
    receipt: &CandidateValidationReceipt,
) -> Result<(), StoreError> {
    let Some(CandidateDeclaration::SampledV1(candidate)) = candidate else {
        return Err(attempt_error(
            "legacy candidate receipt has no V1 worker declaration",
        ));
    };
    if receipt.sha256 != *candidate.media.sha256()
        || receipt.byte_length != candidate.media.byte_length()
        || receipt.video != candidate.video
        || receipt.provider != candidate.provider
        || receipt.video != request.constraints.video
        || receipt.provider != request.provider
    {
        return Err(attempt_error(
            "candidate receipt does not exactly match the request and worker declaration",
        ));
    }
    Ok(())
}

fn validate_bundle_receipt(
    request: &RequestMetadata,
    candidate: Option<&CandidateDeclaration>,
    receipt: &BundleValidationReceipt,
) -> Result<(), StoreError> {
    let Some(CandidateDeclaration::NativeBridgeV2(candidate)) = candidate else {
        return Err(attempt_error("bundle receipt has no V2 worker declaration"));
    };
    let Some(plan) = request.bridge_plan.as_ref() else {
        return Err(attempt_error("bundle receipt belongs to a legacy request"));
    };
    if receipt.plan() != plan
        || receipt.provider() != &request.provider
        || receipt.native_video() != &candidate.video
        || receipt.native_sha256() != candidate.native.sha256()
        || receipt.native_byte_length() != candidate.native.byte_length()
        || receipt.provenance_sha256() != candidate.provenance.sha256()
        || receipt.provenance_byte_length() != candidate.provenance.byte_length()
        || receipt.sampled_video() != &request.constraints.video
        || receipt.native_video().frames()
            != FrameDuration::new(i64::from(plan.native_frame_count()))
                .map_err(|_| attempt_error("invalid bridge native frame count"))?
        || receipt.native_video().frame_rate() != plan.native_frame_rate()
        || receipt.native_video().width() != plan.native_dimensions().width()
        || receipt.native_video().height() != plan.native_dimensions().height()
        || receipt.sampled_video().frames() != plan.project_frames()
        || receipt.sampled_video().frame_rate() != plan.project_frame_rate()
    {
        return Err(attempt_error(
            "bundle receipt does not exactly match the request, plan, or worker declaration",
        ));
    }
    Ok(())
}

fn parse_availability(value: &str) -> Result<CandidateAvailability, StoreError> {
    match value {
        "present" => Ok(CandidateAvailability::Present),
        "evicted" => Ok(CandidateAvailability::Evicted),
        _ => Err(integrity("invalid candidate availability")),
    }
}

fn duplicate_worker_terminal(attempt: &StoredGenerationAttempt, message: &WorkerMessage) -> bool {
    match message {
        WorkerMessage::Completed { candidate, .. } => {
            matches!(
                attempt.checkpoint.state,
                JobState::Validating | JobState::Ready
            ) && attempt.declared_candidate.as_ref()
                == Some(&CandidateDeclaration::SampledV1(candidate.clone()))
        }
        WorkerMessage::CompletedBridge { candidate, .. } => {
            matches!(
                attempt.checkpoint.state,
                JobState::Validating | JobState::Ready
            ) && attempt.declared_candidate.as_ref()
                == Some(&CandidateDeclaration::NativeBridgeV2(candidate.clone()))
        }
        WorkerMessage::Failed { failure, .. } => {
            attempt.checkpoint.state == JobState::Failed
                && attempt.checkpoint.failure == Some(JobFailure::Worker(failure.clone()))
        }
        WorkerMessage::Cancelled { .. } => {
            matches!(
                attempt.checkpoint.state,
                JobState::Cancelling | JobState::Cancelled
            ) && attempt.checkpoint.cancellation_acknowledgement
                == Some(CancellationAcknowledgement::Cancelled)
        }
        WorkerMessage::Stage { .. } | WorkerMessage::Progress { .. } => false,
    }
}

fn bounded_request_id(value: Option<String>) -> Result<RequestId, StoreError> {
    RequestId::new(required(value, "request ID")?)
        .map_err(|_| integrity("invalid generation request ID"))
}

fn required(value: Option<String>, label: &str) -> Result<String, StoreError> {
    value.ok_or_else(|| integrity(&format!("invalid or oversized generation {label}")))
}

fn positive_u64(value: Option<i64>, label: &str) -> Result<u64, StoreError> {
    let value = value.ok_or_else(|| integrity(&format!("invalid generation {label}")))?;
    u64::try_from(value).map_err(|_| integrity(&format!("invalid generation {label}")))
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROTOCOL_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+'))
}

fn bounded_json(value: &impl serde::Serialize, label: &str) -> Result<String, StoreError> {
    let json = serde_json::to_string(value)?;
    if json.len() > MAX_ATTEMPT_JSON_BYTES {
        return Err(attempt_error(&format!(
            "{label} exceeds the persistence limit"
        )));
    }
    Ok(json)
}

fn parse_relevance(value: &str) -> Result<Relevance, StoreError> {
    match value {
        "current" => Ok(Relevance::Current),
        "stale" => Ok(Relevance::Stale),
        "detached" => Ok(Relevance::Detached),
        _ => Err(integrity("invalid generation relevance")),
    }
}

fn state_text(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Preflight => "preflight",
        JobState::Loading => "loading",
        JobState::Running => "running",
        JobState::Validating => "validating",
        JobState::Ready => "ready",
        JobState::Failed => "failed",
        JobState::Cancelling => "cancelling",
        JobState::Cancelled => "cancelled",
    }
}

fn parse_state(value: &str) -> Result<JobState, StoreError> {
    match value {
        "queued" => Ok(JobState::Queued),
        "preflight" => Ok(JobState::Preflight),
        "loading" => Ok(JobState::Loading),
        "running" => Ok(JobState::Running),
        "validating" => Ok(JobState::Validating),
        "ready" => Ok(JobState::Ready),
        "failed" => Ok(JobState::Failed),
        "cancelling" => Ok(JobState::Cancelling),
        "cancelled" => Ok(JobState::Cancelled),
        _ => Err(integrity("invalid generation attempt state")),
    }
}

fn stage_text(stage: WorkerStage) -> &'static str {
    match stage {
        WorkerStage::Preflight => "preflight",
        WorkerStage::RuntimeLoading => "runtime_loading",
        WorkerStage::ModelLoading => "model_loading",
        WorkerStage::Conditioning => "conditioning",
        WorkerStage::Inference => "inference",
        WorkerStage::Decoding => "decoding",
        WorkerStage::Encoding => "encoding",
        WorkerStage::WorkerValidation => "worker_validation",
    }
}

fn parse_stage(value: &str) -> Result<WorkerStage, StoreError> {
    match value {
        "preflight" => Ok(WorkerStage::Preflight),
        "runtime_loading" => Ok(WorkerStage::RuntimeLoading),
        "model_loading" => Ok(WorkerStage::ModelLoading),
        "conditioning" => Ok(WorkerStage::Conditioning),
        "inference" => Ok(WorkerStage::Inference),
        "decoding" => Ok(WorkerStage::Decoding),
        "encoding" => Ok(WorkerStage::Encoding),
        "worker_validation" => Ok(WorkerStage::WorkerValidation),
        _ => Err(integrity("invalid generation worker stage")),
    }
}

fn failure_columns(
    failure: Option<&JobFailure>,
) -> (Option<&'static str>, Option<&'static str>, Option<&str>) {
    match failure {
        None => (None, None, None),
        Some(JobFailure::Worker(failure)) => (
            Some("worker"),
            Some(worker_failure_code_text(failure.code)),
            Some(failure.detail.as_str()),
        ),
        Some(JobFailure::Host(failure)) => (
            Some("host"),
            Some(host_failure_code_text(failure.code)),
            Some(failure.detail.as_str()),
        ),
    }
}

fn parse_failure(
    origin: Option<String>,
    code: Option<String>,
    detail: Option<String>,
) -> Result<Option<JobFailure>, StoreError> {
    match (origin.as_deref(), code.as_deref(), detail) {
        (None, None, None) => Ok(None),
        (Some("worker"), Some(code), Some(detail)) => Ok(Some(JobFailure::Worker(WorkerFailure {
            code: parse_worker_failure_code(code)?,
            detail: Diagnostic::new(detail)
                .map_err(|_| integrity("invalid worker failure detail"))?,
        }))),
        (Some("host"), Some(code), Some(detail)) => Ok(Some(JobFailure::Host(HostFailure {
            code: parse_host_failure_code(code)?,
            detail: Diagnostic::new(detail)
                .map_err(|_| integrity("invalid host failure detail"))?,
        }))),
        _ => Err(integrity("incomplete generation failure")),
    }
}

fn worker_failure_code_text(code: FailureCode) -> &'static str {
    match code {
        FailureCode::UnsupportedRequest => "unsupported_request",
        FailureCode::InvalidInput => "invalid_input",
        FailureCode::MissingArtifact => "missing_artifact",
        FailureCode::HashMismatch => "hash_mismatch",
        FailureCode::ResourceExhausted => "resource_exhausted",
        FailureCode::BackendFailure => "backend_failure",
        FailureCode::OutputValidationFailed => "output_validation_failed",
        FailureCode::Internal => "internal",
    }
}

fn parse_worker_failure_code(value: &str) -> Result<FailureCode, StoreError> {
    match value {
        "unsupported_request" => Ok(FailureCode::UnsupportedRequest),
        "invalid_input" => Ok(FailureCode::InvalidInput),
        "missing_artifact" => Ok(FailureCode::MissingArtifact),
        "hash_mismatch" => Ok(FailureCode::HashMismatch),
        "resource_exhausted" => Ok(FailureCode::ResourceExhausted),
        "backend_failure" => Ok(FailureCode::BackendFailure),
        "output_validation_failed" => Ok(FailureCode::OutputValidationFailed),
        "internal" => Ok(FailureCode::Internal),
        _ => Err(integrity("invalid worker failure code")),
    }
}

fn host_failure_code_text(code: HostFailureCode) -> &'static str {
    match code {
        HostFailureCode::SpawnFailed => "spawn_failed",
        HostFailureCode::WorkerExited => "worker_exited",
        HostFailureCode::ProtocolViolation => "protocol_violation",
        HostFailureCode::Io => "io",
        HostFailureCode::DeadlineExceeded => "deadline_exceeded",
        HostFailureCode::OutputValidationFailed => "output_validation_failed",
        HostFailureCode::Interrupted => "interrupted",
    }
}

fn parse_host_failure_code(value: &str) -> Result<HostFailureCode, StoreError> {
    match value {
        "spawn_failed" => Ok(HostFailureCode::SpawnFailed),
        "worker_exited" => Ok(HostFailureCode::WorkerExited),
        "protocol_violation" => Ok(HostFailureCode::ProtocolViolation),
        "io" => Ok(HostFailureCode::Io),
        "deadline_exceeded" => Ok(HostFailureCode::DeadlineExceeded),
        "output_validation_failed" => Ok(HostFailureCode::OutputValidationFailed),
        "interrupted" => Ok(HostFailureCode::Interrupted),
        _ => Err(integrity("invalid host failure code")),
    }
}

fn lifecycle_error(error: deadpan_jobs::LifecycleError) -> StoreError {
    attempt_error(&error.to_string())
}

fn attempt_error(message: &str) -> StoreError {
    StoreError::GenerationAttempt(message.into())
}

fn integrity(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
