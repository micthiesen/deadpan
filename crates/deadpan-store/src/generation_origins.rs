//! Small, immutable origin evidence for an accepted generated master.
//!
//! Capture reads retained SQLite evidence only. Ordinary replacement planning
//! can then compare raw input identities without reopening provenance sidecars,
//! decoding media, or reconstructing a request from today's UI defaults.
//! A checksum is corruption detection, not admission: full validation rebuilds
//! the binding from the request's original revision and checks its exact bundle
//! against both the retained worker evidence and an authored acceptance command.

use deadpan_core::{
    Command, GeneratedArtifact, ProjectDocument, ProjectId, RevisionId, ScopedNodeEdit,
    ScopedNodeTarget,
};
use deadpan_jobs::{
    CandidateDeclaration, GenerationOptions, GenerationPlan, HoldConstraints, MessageIdentity,
    NativeCandidateManifest, ProviderSelection, RequestId, RequestVersion, Sha256 as ContentSha256,
};
use deadpan_plan::RenderPlan;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::generation::{GenerationScopeId, StoredGenerationRequest};
use crate::generation_attempts::{BundleValidationReceipt, CandidateAvailability};
use crate::generation_inputs::{GenerationCaptureSpec, InputCaptureBudget};
use crate::generation_pictures::QualifiedGenerationPictures;
use crate::{ProjectStore, StoreError};

// A maximum-depth scope is allowed, but source indexes and sidecars never
// belong in this row. The request input descriptor has a separate 512 KiB cap;
// its other fields and attempt evidence retain their smaller bounds.
pub(crate) const MAX_ROW_BYTES: usize =
    crate::generation_inputs::MAX_INPUT_BINDING_BYTES + 256 * 1024;
const MAX_EVIDENCE_BYTES: usize = 16 * 1024;
const RECEIPT_VERSION: u32 = 1;

pub use crate::generation_inputs::GenerationInputBinding;

/// Immutable request fields, excluding its later relevance and mapped address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestOrigin {
    request_id: RequestId,
    project_id: ProjectId,
    scope_id: GenerationScopeId,
    origin_revision: RevisionId,
    origin_target: ScopedNodeTarget,
    request_version: RequestVersion,
    context_sha256: ContentSha256,
    constraints: HoldConstraints,
    provider: ProviderSelection,
    plan: GenerationPlan,
    input_binding: GenerationInputBinding,
}

impl RequestOrigin {
    fn from_request(request: &StoredGenerationRequest) -> Result<Self, StoreError> {
        if request.binding.hold_id != request.origin_target.node {
            return Err(invalid(
                "accepted request Hold differs from its origin target",
            ));
        }
        let Some(plan @ GenerationPlan::Bridge(_)) = &request.plan else {
            return Err(invalid(
                "accepted origin requires a retained Bridge plan; extension output admission is unavailable",
            ));
        };
        Ok(Self {
            request_id: request.request_id.clone(),
            project_id: request.binding.project_id.clone(),
            scope_id: request.scope_id.clone(),
            origin_revision: request.origin_revision.clone(),
            origin_target: request.origin_target.clone(),
            request_version: request.binding.request_version,
            context_sha256: request.binding.context_sha256.clone(),
            constraints: request.constraints.clone(),
            provider: request.provider.clone(),
            plan: plan.clone(),
            input_binding: request
                .input_binding
                .clone()
                .ok_or_else(|| invalid("accepted origin needs retained request inputs"))?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedOriginReceipt {
    version: u32,
    artifact: GeneratedArtifact,
    identity: MessageIdentity,
    origin: RequestOrigin,
    options: GenerationOptions,
    accepted_revision: RevisionId,
}

impl AcceptedOriginReceipt {
    pub fn artifact(&self) -> &GeneratedArtifact {
        &self.artifact
    }
    pub fn identity(&self) -> &MessageIdentity {
        &self.identity
    }
    pub fn request_id(&self) -> &RequestId {
        &self.origin.request_id
    }
    pub fn origin_revision(&self) -> &RevisionId {
        &self.origin.origin_revision
    }
    pub fn origin_target(&self) -> &ScopedNodeTarget {
        &self.origin.origin_target
    }
    pub fn options(&self) -> &GenerationOptions {
        &self.options
    }
    pub fn input_binding(&self) -> &GenerationInputBinding {
        &self.origin.input_binding
    }
    pub fn accepted_revision(&self) -> &RevisionId {
        &self.accepted_revision
    }
}

impl ProjectStore {
    /// Cheap metadata lookup on an already validated store. Exact copies of an
    /// artifact share its receipt; a remapped asset alias needs fresh admission.
    pub fn accepted_generation_origin(
        &self,
        artifact: &GeneratedArtifact,
    ) -> Result<Option<AcceptedOriginReceipt>, StoreError> {
        read(&self.connection, artifact)
    }
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE generation_accepted_origins (
        artifact_key TEXT PRIMARY KEY,
        request_id TEXT NOT NULL,
        attempt_id TEXT NOT NULL,
        origin_revision TEXT NOT NULL REFERENCES revisions(id),
        accepted_revision TEXT NOT NULL REFERENCES revisions(id),
        receipt TEXT NOT NULL CHECK(json_valid(receipt)),
        receipt_sha256 TEXT NOT NULL CHECK(length(receipt_sha256)=64),
        FOREIGN KEY(request_id,attempt_id)
            REFERENCES generation_bundle_receipts(request_id,attempt_id)
    ) STRICT;
    CREATE INDEX generation_accepted_origins_request
        ON generation_accepted_origins(request_id,attempt_id);
    CREATE INDEX generation_accepted_origins_acceptance
        ON generation_accepted_origins(accepted_revision);",
    )?;
    Ok(())
}

/// Call after the authored acceptance and retention flag are written, inside
/// the same transaction. The supplied request may have its pre-isolation address;
/// only its immutable origin fields are compared with retained request evidence.
pub(crate) fn capture(
    connection: &Connection,
    request: &StoredGenerationRequest,
    identity: &MessageIdentity,
    artifact: &GeneratedArtifact,
    accepted_revision: &RevisionId,
) -> Result<AcceptedOriginReceipt, StoreError> {
    let value = capture_origin_inputs(connection, request, identity, artifact, accepted_revision)?;
    validate_admission(connection, &value, BundleUse::Admission)?;
    Ok(value)
}

/// Internal precommit input for the final boundary decision. The acceptance
/// caller has already checked its exact selected candidate and media objects.
/// This checks the immutable request, measured inputs and present Ready bundle;
/// only authored acceptance and accepted-retention evidence await the commit.
/// It grants no write capability. `insert` still verifies complete admission.
pub(crate) fn prepare_acceptance_origin(
    connection: &Connection,
    request: &StoredGenerationRequest,
    identity: &MessageIdentity,
    artifact: &GeneratedArtifact,
    accepted_revision: &RevisionId,
) -> Result<AcceptedOriginReceipt, StoreError> {
    let value = capture_origin_inputs(connection, request, identity, artifact, accepted_revision)?;
    validate_bundle(connection, &value, BundleUse::Candidate)?;
    Ok(value)
}

fn capture_origin_inputs(
    connection: &Connection,
    request: &StoredGenerationRequest,
    identity: &MessageIdentity,
    artifact: &GeneratedArtifact,
    accepted_revision: &RevisionId,
) -> Result<AcceptedOriginReceipt, StoreError> {
    let origin = RequestOrigin::from_request(request)?;
    if origin != read_request_origin(connection, &request.request_id)?
        || identity.request_id != request.request_id
    {
        return Err(invalid("accepted origin differs from its admitted request"));
    }
    let document =
        crate::validation::read_revision(connection, origin.origin_revision.as_str())?.document;
    let input_binding = recapture_inputs(connection, &document, &origin)?;
    let value = AcceptedOriginReceipt {
        version: RECEIPT_VERSION,
        artifact: artifact.clone(),
        identity: identity.clone(),
        options: GenerationOptions::from_constraints(&origin.constraints),
        origin,
        accepted_revision: accepted_revision.clone(),
    };
    canonical(&value)?;
    validate_input_contract(&value, &document, &input_binding)?;
    Ok(value)
}

/// An exact duplicate is harmless. A second claimed origin for the same complete
/// artifact is an integrity error, including a different acceptance receipt.
pub(crate) fn insert(
    connection: &Connection,
    value: &AcceptedOriginReceipt,
) -> Result<(), StoreError> {
    let json = canonical(value)?;
    if let Some(existing) = read(connection, &value.artifact)? {
        if existing != *value {
            return Err(invalid(
                "accepted artifact already has different origin evidence",
            ));
        }
        // Candidate offering may have ended since this immutable admission.
        return validate_proof(connection, value, BundleUse::Retained);
    }
    validate_proof(connection, value, BundleUse::Admission)?;
    connection.execute(
        "INSERT INTO generation_accepted_origins(
        artifact_key,request_id,attempt_id,origin_revision,accepted_revision,receipt,receipt_sha256)
        VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            artifact_key(&value.artifact)?,
            value.identity.request_id.as_str(),
            value.identity.attempt_id.as_str(),
            value.origin.origin_revision.as_str(),
            value.accepted_revision.as_str(),
            json,
            receipt_digest(json.as_bytes()),
        ],
    )?;
    Ok(())
}

/// Includes aliases, all three exact object references, the sampling map and
/// content aspect. No digest-only or native-master-only equivalence is inferred.
fn artifact_key(artifact: &GeneratedArtifact) -> Result<String, StoreError> {
    let bytes = serde_json::to_vec(artifact)?;
    if bytes.len() > MAX_EVIDENCE_BYTES {
        return Err(invalid("accepted artifact exceeds its metadata byte limit"));
    }
    Ok(hash(b"deadpan-accepted-artifact-1", &bytes))
}

fn receipt_digest(bytes: &[u8]) -> String {
    hash(b"deadpan-accepted-origin-1", bytes)
}

fn hash(domain: &[u8], bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn canonical(value: &AcceptedOriginReceipt) -> Result<String, StoreError> {
    let json = serde_json::to_string(value)?;
    if json.len() > MAX_ROW_BYTES {
        return Err(invalid("accepted origin exceeds its metadata byte limit"));
    }
    Ok(json)
}

// Every field is bounded before SQLite returns its bytes, even for a lookup
// made independently of the store-wide stored-size pass.
const SELECT: &str = "SELECT
    CASE WHEN typeof(receipt)='text' AND length(CAST(receipt AS BLOB)) BETWEEN 1 AND ?1 THEN receipt END,
    CASE WHEN typeof(artifact_key)='text' AND length(CAST(artifact_key AS BLOB))=64 THEN artifact_key END,
    CASE WHEN typeof(request_id)='text' AND length(CAST(request_id AS BLOB)) BETWEEN 1 AND ?2 THEN request_id END,
    CASE WHEN typeof(attempt_id)='text' AND length(CAST(attempt_id AS BLOB)) BETWEEN 1 AND ?2 THEN attempt_id END,
    CASE WHEN typeof(origin_revision)='text' AND length(CAST(origin_revision AS BLOB)) BETWEEN 1 AND ?2 THEN origin_revision END,
    CASE WHEN typeof(accepted_revision)='text' AND length(CAST(accepted_revision AS BLOB)) BETWEEN 1 AND ?2 THEN accepted_revision END,
    CASE WHEN typeof(receipt_sha256)='text' AND length(CAST(receipt_sha256 AS BLOB))=64 THEN receipt_sha256 END
    FROM generation_accepted_origins";

fn parse(row: &Row<'_>) -> Result<AcceptedOriginReceipt, StoreError> {
    let field = |index: usize| -> Result<String, StoreError> {
        row.get::<_, Option<String>>(index)?
            .ok_or_else(|| invalid("accepted origin column is missing, mistyped or oversized"))
    };
    let json = field(0)?;
    let value: AcceptedOriginReceipt = serde_json::from_str(&json)?;
    if value.version != RECEIPT_VERSION
        || value.identity.request_id != value.origin.request_id
        || value.accepted_revision == value.origin.origin_revision
        || canonical(&value)? != json
        || artifact_key(&value.artifact)? != field(1)?
        || value.identity.request_id.as_str() != field(2)?
        || value.identity.attempt_id.as_str() != field(3)?
        || value.origin.origin_revision.as_str() != field(4)?
        || value.accepted_revision.as_str() != field(5)?
        || receipt_digest(json.as_bytes()) != field(6)?
    {
        return Err(invalid(
            "accepted origin columns, canonical bytes or digest differ",
        ));
    }
    Ok(value)
}

pub(crate) fn read(
    connection: &Connection,
    artifact: &GeneratedArtifact,
) -> Result<Option<AcceptedOriginReceipt>, StoreError> {
    let mut statement = connection.prepare(&format!("{SELECT} WHERE artifact_key=?3"))?;
    let mut rows = statement.query(params![
        MAX_ROW_BYTES as i64,
        deadpan_core::MAX_IDENTITY_BYTES as i64,
        artifact_key(artifact)?,
    ])?;
    let value = rows.next()?.map(parse).transpose()?;
    if rows.next()?.is_some()
        || value
            .as_ref()
            .is_some_and(|value| value.artifact != *artifact)
    {
        return Err(invalid(
            "accepted artifact content key is duplicated or conflicts",
        ));
    }
    Ok(value)
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let bad: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM generation_accepted_origins WHERE
        typeof(artifact_key)!='text' OR length(CAST(artifact_key AS BLOB))!=64 OR
        typeof(request_id)!='text' OR length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?2 OR
        typeof(attempt_id)!='text' OR length(CAST(attempt_id AS BLOB)) NOT BETWEEN 1 AND ?2 OR
        typeof(origin_revision)!='text' OR length(CAST(origin_revision AS BLOB)) NOT BETWEEN 1 AND ?2 OR
        typeof(accepted_revision)!='text' OR length(CAST(accepted_revision AS BLOB)) NOT BETWEEN 1 AND ?2 OR
        typeof(receipt)!='text' OR length(CAST(receipt AS BLOB)) NOT BETWEEN 1 AND ?1 OR
        typeof(receipt_sha256)!='text' OR length(CAST(receipt_sha256 AS BLOB))!=64)",
        params![MAX_ROW_BYTES as i64, deadpan_core::MAX_IDENTITY_BYTES as i64], |row| row.get(0))?;
    if bad {
        return Err(invalid(
            "accepted origin metadata exceeds its stored bounds",
        ));
    }
    Ok(())
}

fn read_request_origin(
    connection: &Connection,
    id: &RequestId,
) -> Result<RequestOrigin, StoreError> {
    // Reuse the bounded, operation-tagged reader. It checks SQL types/lengths,
    // paired plan/input presence and the capture operation before allocating
    // immutable origin metadata. Mutable relevance/target are excluded below.
    let request = crate::generation::read_stored_request(connection, id)?
        .ok_or_else(|| invalid("accepted origin request is missing"))?;
    RequestOrigin::from_request(&request)
}

#[derive(Clone, Copy)]
enum BundleUse {
    /// Host-qualified candidate, before authored acceptance and retention write.
    Candidate,
    /// First durable origin receipt, after the authored acceptance.
    Admission,
    /// An immutable receipt may survive the end of candidate offering.
    Retained,
}

/// Only the small host-qualified receipt and declaration are loaded. Mutable
/// selection/relevance may have changed since acceptance and are not proof of
/// origin. First admission needs a present Ready candidate. Historical proof
/// retains that immutable Ready bundle even after explicit candidate Discard;
/// offering availability cannot revoke an authored acceptance or its Undo.
fn read_bundle_evidence(
    connection: &Connection,
    identity: &MessageIdentity,
    usage: BundleUse,
) -> Result<(u64, NativeCandidateManifest, BundleValidationReceipt), StoreError> {
    type EvidenceRow = (
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<bool>,
        bool,
        Option<bool>,
    );
    let row: Option<EvidenceRow> = connection.query_row(
        "SELECT CASE WHEN typeof(a.ordinal)='integer' AND a.ordinal>0 THEN a.ordinal END,
            CASE WHEN typeof(a.worker_candidate)='text' AND length(CAST(a.worker_candidate AS BLOB)) BETWEEN 1 AND ?3 THEN a.worker_candidate END,
            CASE WHEN typeof(b.bundle)='text' AND length(CAST(b.bundle AS BLOB)) BETWEEN 1 AND ?3 THEN b.bundle END,
            CASE WHEN b.availability='present' AND r.eviction IS NULL THEN 1
                 WHEN b.availability='evicted' AND r.eviction IN ('discarded','expired') THEN 0 END,
            a.state='ready',
            CASE WHEN typeof(r.accepted)='integer' AND r.accepted IN (0,1) THEN r.accepted END
        FROM generation_attempts a
        JOIN generation_bundle_receipts b USING(request_id,attempt_id)
        JOIN generation_variant_retention r USING(request_id,attempt_id)
        WHERE a.request_id=?1 AND a.attempt_id=?2",
        params![identity.request_id.as_str(), identity.attempt_id.as_str(), MAX_EVIDENCE_BYTES as i64],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).optional()?;
    let Some((Some(ordinal), Some(candidate), Some(bundle), Some(present), true, Some(accepted))) =
        row
    else {
        return Err(invalid(
            "accepted origin lacks retained Ready bundle and acceptance evidence",
        ));
    };
    if !matches!(usage, BundleUse::Candidate) && !accepted {
        return Err(invalid(
            "accepted origin bundle lacks accepted retention evidence",
        ));
    }
    let bundle: BundleValidationReceipt = serde_json::from_str(&bundle)?;
    if (bundle.availability() == CandidateAvailability::Present) != present
        || bundle.admission().is_none()
    {
        return Err(invalid(
            "accepted origin bundle lacks host admission or has inconsistent availability",
        ));
    }
    if !matches!(usage, BundleUse::Retained) && !present {
        return Err(invalid(
            "first accepted origin admission requires a present Ready bundle",
        ));
    }
    Ok((
        u64::try_from(ordinal).map_err(|_| invalid("invalid accepted attempt ordinal"))?,
        serde_json::from_str(&candidate)?,
        bundle,
    ))
}

fn validate_proof(
    connection: &Connection,
    value: &AcceptedOriginReceipt,
    usage: BundleUse,
) -> Result<(), StoreError> {
    if value.version != RECEIPT_VERSION
        || value.identity.request_id != value.origin.request_id
        || value.accepted_revision == value.origin.origin_revision
        || value.origin != read_request_origin(connection, &value.identity.request_id)?
        || value.options != GenerationOptions::from_constraints(&value.origin.constraints)
    {
        return Err(invalid(
            "accepted origin differs from its retained request or exact controls",
        ));
    }
    let document =
        crate::validation::read_revision(connection, value.origin.origin_revision.as_str())?
            .document;
    let binding = recapture_inputs(connection, &document, &value.origin)?;
    validate_input_contract(value, &document, &binding)?;
    validate_admission(connection, value, usage)
}

fn recapture_inputs(
    connection: &Connection,
    document: &ProjectDocument,
    origin: &RequestOrigin,
) -> Result<GenerationInputBinding, StoreError> {
    if !matches!(origin.plan, GenerationPlan::Bridge(_)) {
        return Err(invalid(
            "extension accepted origins require qualified output admission",
        ));
    }
    let plan = RenderPlan::compile(document).map_err(plan_error)?;
    crate::generation_inputs::GenerationInputCapture::capture_with_plan(
        document,
        &plan,
        &origin.origin_target,
        GenerationCaptureSpec::from_plan(&origin.plan),
        origin.constraints.region_target.as_ref(),
        &QualifiedGenerationPictures::new(connection),
        &mut InputCaptureBudget::default(),
    )
}

fn validate_input_contract(
    value: &AcceptedOriginReceipt,
    document: &ProjectDocument,
    binding: &GenerationInputBinding,
) -> Result<(), StoreError> {
    if value.origin.project_id != *document.project_id()
        || &value.origin.input_binding != binding
        || binding
            .region
            .as_ref()
            .is_some_and(|region| region.record.is_none())
        || value.accepted_revision == value.origin.origin_revision
        || binding.duration != value.origin.constraints.video.frames()
        || binding.frame_rate != value.origin.constraints.video.frame_rate()
        || value.artifact.content_aspect != Some(binding.canvas)
    {
        return Err(invalid(
            "accepted origin inputs, duration, rate or canvas differ from its revision",
        ));
    }
    Ok(())
}

fn validate_bundle(
    connection: &Connection,
    value: &AcceptedOriginReceipt,
    usage: BundleUse,
) -> Result<BundleValidationReceipt, StoreError> {
    let (ordinal, candidate, bundle) = read_bundle_evidence(connection, &value.identity, usage)?;
    let request = crate::generation_attempts::read_request(connection, &value.identity.request_id)?;
    crate::generation_attempts::validate_bundle_receipt(
        &request,
        ordinal,
        Some(&CandidateDeclaration::NativeBridgeV2(candidate)),
        &bundle,
    )?;
    if &value.artifact.sampled_object != bundle.sampled_object()
        || &value.artifact.native_object != bundle.native_object()
        || &value.artifact.provenance != bundle.provenance_object()
        || value.artifact.sampling != bundle.plan().sampling_map().map_err(plan_error)?
        || value.origin.plan != GenerationPlan::Bridge(bundle.plan().clone())
        || bundle.admission().is_none_or(|admission| {
            admission.inputs().context_sha256() != &value.origin.context_sha256
        })
    {
        return Err(invalid(
            "accepted artifact differs from its exact admitted bundle",
        ));
    }
    Ok(bundle)
}

fn validate_admission(
    connection: &Connection,
    value: &AcceptedOriginReceipt,
    usage: BundleUse,
) -> Result<(), StoreError> {
    let bundle = validate_bundle(connection, value, usage)?;
    let mut statement =
        connection.prepare("SELECT id FROM history WHERE revision_id=?1 LIMIT 2")?;
    let mut rows = statement.query([value.accepted_revision.as_str()])?;
    let history_id: i64 = rows
        .next()?
        .ok_or_else(|| invalid("accepted origin has no authored acceptance history"))?
        .get(0)?;
    if rows.next()?.is_some() {
        return Err(invalid("accepted origin history revision is duplicated"));
    }
    let history = crate::validation::read_history(connection, history_id)?;
    if history.request.new_revision != value.accepted_revision
        || history.request.project_id != value.origin.project_id
        || !canonical_acceptance(&history.request.command, &value.artifact, &bundle)
    {
        return Err(invalid(
            "accepted origin does not name its exact canonical acceptance command",
        ));
    }
    let before =
        crate::validation::read_revision(connection, history.request.expected_revision.as_str())?
            .document;
    if before.assets().contains_key(&value.artifact.sampled_asset)
        || before.assets().contains_key(&value.artifact.native_asset)
    {
        return Err(invalid(
            "accepted origin admission did not introduce fresh asset aliases",
        ));
    }
    Ok(())
}

fn canonical_acceptance(
    command: &Command,
    artifact: &GeneratedArtifact,
    bundle: &BundleValidationReceipt,
) -> bool {
    let (accepted, assets) = match command.base_command() {
        Command::AcceptGeneratedHold {
            artifact, assets, ..
        }
        | Command::EditScoped {
            edit: ScopedNodeEdit::AcceptGeneratedHold { artifact, assets },
            ..
        } => (artifact, assets),
        _ => return false,
    };
    let Some(evidence) = bundle.admission() else {
        return false;
    };
    let sampled = crate::generation_acceptance::asset_record(
        bundle.sampled_object(),
        bundle.sampled_video(),
        evidence.sampled_span(),
    );
    let native = crate::generation_acceptance::asset_record(
        bundle.native_object(),
        bundle.native_video(),
        evidence.native_span(),
    );
    let expected_assets = if artifact.sampled_asset == artifact.native_asset {
        1
    } else {
        2
    };
    accepted == artifact
        && assets.len() == expected_assets
        && assets.get(&artifact.sampled_asset) == Some(&sampled)
        && assets.get(&artifact.native_asset) == Some(&native)
}

/// Hash the receipt and the small immutable rows it relies on. The enclosing
/// history audit separately hashes revision/command bytes and source receipts;
/// this digest is not a substitute for those dependencies.
pub(crate) fn digest(connection: &Connection) -> Result<crate::audit::Chain, StoreError> {
    check_stored_sizes(connection)?;
    let mut hash = Sha256::new();
    hash.update(b"deadpan-accepted-origins-table-1");
    let mut statement = connection.prepare(&format!("{SELECT} ORDER BY artifact_key"))?;
    let mut rows = statement.query(params![
        MAX_ROW_BYTES as i64,
        deadpan_core::MAX_IDENTITY_BYTES as i64
    ])?;
    while let Some(row) = rows.next()? {
        let value = parse(row)?;
        hash_field(&mut hash, canonical(&value)?.as_bytes());
        hash_field(
            &mut hash,
            &serde_json::to_vec(&read_request_origin(
                connection,
                &value.identity.request_id,
            )?)?,
        );
        let (ordinal, candidate, bundle) =
            read_bundle_evidence(connection, &value.identity, BundleUse::Retained)?;
        hash.update(ordinal.to_le_bytes());
        hash_field(&mut hash, &serde_json::to_vec(&candidate)?);
        hash_field(&mut hash, &serde_json::to_vec(&bundle)?);
    }
    Ok(hash.finalize().into())
}

fn hash_field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

/// `verified` is the prefix proved by a matching enclosing history audit. Its
/// digest includes this table and its request/bundle dependencies. A changed or
/// deleted row therefore requires `verified=0`, including the missing-row scan.
pub(crate) fn validate_store(connection: &Connection, verified: usize) -> Result<(), StoreError> {
    check_stored_sizes(connection)?;
    let chronology = crate::validation::chronology(connection)?;
    if verified > chronology.len() {
        return Err(invalid("accepted origin audit prefix exceeds history"));
    }
    if verified == chronology.len() {
        return Ok(());
    }
    let order = chronology
        .into_iter()
        .enumerate()
        .map(|(index, revision)| (revision, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    let position = |revision: &str| {
        order
            .get(revision)
            .copied()
            .ok_or_else(|| invalid("accepted origin revision is outside retained history"))
    };
    let mut statement = connection.prepare(&format!("{SELECT} ORDER BY artifact_key"))?;
    let mut rows = statement.query(params![
        MAX_ROW_BYTES as i64,
        deadpan_core::MAX_IDENTITY_BYTES as i64
    ])?;
    while let Some(row) = rows.next()? {
        let receipt = parse(row)?;
        let accepted = position(receipt.accepted_revision.as_str())?;
        if accepted >= verified {
            if position(receipt.origin.origin_revision.as_str())? >= accepted {
                return Err(invalid(
                    "accepted origin request does not precede its acceptance",
                ));
            }
            validate_proof(connection, &receipt, BundleUse::Retained)?;
        }
    }

    // Deleting a receipt must not turn full validation into a successful empty
    // scan. Canonical acceptance commands, including later copies of a retained
    // artifact, require its admitted origin. Decode the bounded command instead
    // of filtering its text: JSON escapes can spell a valid command tag without
    // containing its literal name. Verified-prefix commands are not read again.
    let mut statement = connection.prepare(
        "SELECT id,CASE WHEN typeof(revision_id)='text' AND length(CAST(revision_id AS BLOB)) BETWEEN 1 AND ?1 THEN revision_id END
         FROM history ORDER BY id")?;
    let mut rows = statement.query([deadpan_core::MAX_IDENTITY_BYTES as i64])?;
    while let Some(row) = rows.next()? {
        let revision = row
            .get::<_, Option<String>>(1)?
            .ok_or_else(|| invalid("acceptance history revision exceeds its bounds"))?;
        if position(&revision)? < verified {
            continue;
        }
        let history = crate::validation::read_history(connection, row.get(0)?)?;
        require_command_origins(
            connection,
            &history.request.command,
            position(&revision)?,
            &order,
        )?;
    }
    Ok(())
}

fn require_command_origins(
    connection: &Connection,
    command: &Command,
    at: usize,
    order: &std::collections::BTreeMap<String, usize>,
) -> Result<(), StoreError> {
    match command {
        Command::AcceptGeneratedHold { artifact, .. }
        | Command::EditScoped {
            edit: ScopedNodeEdit::AcceptGeneratedHold { artifact, .. },
            ..
        } => {
            require_artifact_origin(connection, artifact, at, order)?;
        }
        Command::EditScopedMany { edits, .. } => {
            for edit in edits {
                if let ScopedNodeEdit::AcceptGeneratedHold { artifact, .. } = &edit.edit {
                    require_artifact_origin(connection, artifact, at, order)?;
                }
            }
        }
        Command::Compound { transaction } => {
            for step in transaction.steps() {
                if let Some(edit) = step.edit() {
                    require_command_origins(connection, edit.command.as_command(), at, order)?;
                }
            }
        }
        Command::WithBoundaryReplacements { edit } => {
            require_command_origins(connection, edit.command(), at, order)?
        }
        _ => {}
    }
    Ok(())
}

fn require_artifact_origin(
    connection: &Connection,
    artifact: &GeneratedArtifact,
    at: usize,
    order: &std::collections::BTreeMap<String, usize>,
) -> Result<(), StoreError> {
    let receipt = read(connection, artifact)?
        .ok_or_else(|| invalid("authored acceptance is missing its retained origin receipt"))?;
    if order
        .get(receipt.accepted_revision.as_str())
        .is_none_or(|origin| *origin > at)
    {
        return Err(invalid(
            "authored acceptance predates its claimed initial admission",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}
fn plan_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::GenerationPlan(error.to_string())
}

#[cfg(test)]
mod tests;
