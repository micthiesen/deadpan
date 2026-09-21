//! Persistent generation intent and monotonic relevance outside document history.
//!
//! Media/context resolution remains host-owned. The store persists the resolved
//! context hash and independently checks the target Hold, duration, project rate,
//! request identity, and complete relevance coverage.

use std::collections::BTreeMap;

use deadpan_core::{
    MAX_DOCUMENT_NODES, MAX_IDENTITY_BYTES, NodeId, NodeKind, ProjectDocument, RevisionId,
};
use deadpan_jobs::{
    HoldConstraints, ProviderSelection, Relevance, RequestId, RequestVersion, Sha256, TargetBinding,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{ProjectStore, StoreError, read_snapshot, validation};

const MAX_REQUEST_JSON_BYTES: usize = 16 * 1024;
const MAX_SQL_REQUEST_VERSION: i64 = i64::MAX;

const CREATE_TABLES: &str = "
CREATE TABLE hold_request_clocks (
    hold_id TEXT PRIMARY KEY,
    high_water INTEGER NOT NULL
        CHECK (high_water BETWEEN 1 AND 9223372036854775807)
) STRICT;
CREATE TABLE generation_requests (
    request_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    hold_id TEXT NOT NULL,
    request_version INTEGER NOT NULL
        CHECK (request_version BETWEEN 1 AND 9223372036854775807),
    origin_revision TEXT NOT NULL REFERENCES revisions(id),
    context_sha256 TEXT NOT NULL,
    constraints TEXT NOT NULL CHECK (json_valid(constraints)),
    provider TEXT NOT NULL CHECK (json_valid(provider)),
    relevance TEXT NOT NULL CHECK (relevance IN ('current','stale','detached')),
    UNIQUE (hold_id, request_version)
) STRICT;
CREATE UNIQUE INDEX one_current_generation_per_hold
    ON generation_requests(hold_id) WHERE relevance='current';";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationRequestInput {
    pub request_id: RequestId,
    pub expected_revision: RevisionId,
    pub hold_id: NodeId,
    pub context_sha256: Sha256,
    pub constraints: HoldConstraints,
    pub provider: ProviderSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredGenerationRequest {
    pub request_id: RequestId,
    pub origin_revision: RevisionId,
    pub binding: TargetBinding,
    pub constraints: HoldConstraints,
    pub provider: ProviderSelection,
    pub relevance: Relevance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextObservation {
    Resolved(Sha256),
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelevanceObservation {
    pub request_id: RequestId,
    pub binding: TargetBinding,
    pub after_context: ContextObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelevancePlan {
    pub from_revision: RevisionId,
    pub to_revision: RevisionId,
    pub observations: Vec<RelevanceObservation>,
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(CREATE_TABLES)?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let invalid_requests: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_requests WHERE
            typeof(request_id)!='text' OR length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(project_id)!='text' OR length(CAST(project_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(hold_id)!='text' OR length(CAST(hold_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(request_version)!='integer' OR request_version<1 OR
            typeof(origin_revision)!='text' OR length(CAST(origin_revision AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(context_sha256)!='text' OR length(CAST(context_sha256 AS BLOB))!=64 OR
            typeof(constraints)!='text' OR length(CAST(constraints AS BLOB))>?2 OR
            typeof(provider)!='text' OR length(CAST(provider AS BLOB))>?2 OR
            typeof(relevance)!='text' OR length(CAST(relevance AS BLOB)) NOT BETWEEN 5 AND 8",
        params![MAX_IDENTITY_BYTES as i64, MAX_REQUEST_JSON_BYTES as i64],
        |row| row.get(0),
    )?;
    let invalid_clocks: i64 = connection.query_row(
        "SELECT COUNT(*) FROM hold_request_clocks WHERE
            typeof(hold_id)!='text' OR length(CAST(hold_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(high_water)!='integer' OR high_water<1",
        [MAX_IDENTITY_BYTES as i64],
        |row| row.get(0),
    )?;
    if invalid_requests != 0 || invalid_clocks != 0 {
        return Err(integrity(
            "stored generation metadata exceeds its bounds or has the wrong type",
        ));
    }
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    let document = read_snapshot(connection)?;
    let mut identifiers = connection.prepare(
        "SELECT CASE WHEN typeof(request_id)='text'
                     AND length(CAST(request_id AS BLOB)) BETWEEN 1 AND ?1
                     THEN request_id END
         FROM generation_requests ORDER BY request_id",
    )?;
    let mut identifier_rows = identifiers.query([MAX_IDENTITY_BYTES as i64])?;
    let mut prior_request: Option<RequestId> = None;
    while let Some(row) = identifier_rows.next()? {
        let request: Option<String> = row.get(0)?;
        let request =
            RequestId::new(request.ok_or_else(|| integrity("invalid generation request ID"))?)
                .map_err(|_| integrity("invalid generation request ID"))?;
        if prior_request.as_ref() == Some(&request) {
            return Err(integrity("duplicate generation request ID"));
        }
        prior_request = Some(request);
    }

    let mut statement = connection.prepare(&format!(
        "{BOUNDED_REQUEST_SELECT} ORDER BY hold_id,request_version,request_id"
    ))?;
    let mut rows = statement.query([MAX_IDENTITY_BYTES as i64])?;
    let mut prior_pair: Option<(NodeId, RequestVersion)> = None;
    let mut prior_current_hold: Option<NodeId> = None;
    while let Some(row) = rows.next()? {
        let request = parse_request_row(row)?;
        let pair = (
            request.binding.hold_id.clone(),
            request.binding.request_version,
        );
        if prior_pair.as_ref() == Some(&pair) {
            return Err(integrity("duplicate Hold request version"));
        }
        prior_pair = Some(pair);
        if request.relevance == Relevance::Current {
            if prior_current_hold.as_ref() == Some(&request.binding.hold_id) {
                return Err(integrity("multiple current requests target one Hold"));
            }
            prior_current_hold = Some(request.binding.hold_id.clone());
        }
        let origin =
            validation::read_revision(connection, request.origin_revision.as_str())?.document;
        if request.binding.project_id != *document.project_id()
            || request.binding.project_id != *origin.project_id()
            || validate_new_target(&origin, &request.binding.hold_id, &request.constraints).is_err()
        {
            return Err(integrity(
                "generation request is incompatible with its origin revision",
            ));
        }
        let high_water: Option<i64> = connection
            .query_row(
                "SELECT CASE WHEN typeof(high_water)='integer' THEN high_water END
                 FROM hold_request_clocks WHERE hold_id=?1",
                [request.binding.hold_id.as_str()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let Some(high_water) = high_water else {
            return Err(integrity("generation request has no valid Hold clock"));
        };
        let request_version = request_version_i64(request.binding.request_version)?;
        if high_water < request_version {
            return Err(integrity(
                "Hold request clock is below an allocated version",
            ));
        }
        if request.relevance == Relevance::Current {
            if high_water != request_version {
                return Err(integrity(
                    "current generation request is not the latest allocated Hold version",
                ));
            }
            validate_current_target(&document, &request)?;
        }
    }

    let mut clocks = connection.prepare(
        "SELECT
            CASE WHEN typeof(hold_id)='text'
                 AND length(CAST(hold_id AS BLOB)) BETWEEN 1 AND ?1 THEN hold_id END,
            CASE WHEN typeof(high_water)='integer' AND high_water>=1 THEN high_water END
         FROM hold_request_clocks ORDER BY hold_id",
    )?;
    let mut rows = clocks.query([MAX_IDENTITY_BYTES as i64])?;
    let mut prior_hold: Option<NodeId> = None;
    while let Some(row) = rows.next()? {
        let hold: Option<String> = row.get(0)?;
        let high_water: Option<i64> = row.get(1)?;
        let hold =
            NodeId::new(hold.ok_or_else(|| integrity("invalid Hold request clock identity"))?)
                .map_err(|_| integrity("invalid Hold request clock identity"))?;
        if prior_hold.as_ref() == Some(&hold) {
            return Err(integrity("duplicate Hold request clock"));
        }
        prior_hold = Some(hold.clone());
        let high_water =
            high_water.ok_or_else(|| integrity("invalid Hold request clock high-water value"))?;
        let maximum: Option<i64> = connection.query_row(
            "SELECT MAX(request_version) FROM generation_requests WHERE hold_id=?1",
            [hold.as_str()],
            |row| row.get(0),
        )?;
        if maximum != Some(high_water) {
            return Err(integrity(
                "Hold request clock does not equal its latest allocated version",
            ));
        }
    }
    Ok(())
}

impl ProjectStore {
    pub fn allocate_generation_request(
        &mut self,
        input: GenerationRequestInput,
    ) -> Result<StoredGenerationRequest, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let document = read_snapshot(&transaction)?;
        require_revision(&document, &input.expected_revision)?;
        validate_new_target(&document, &input.hold_id, &input.constraints)?;

        let exists: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM generation_requests WHERE request_id=?1",
                [input.request_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(StoreError::GenerationRequestReused(
                input.request_id.as_str().to_owned(),
            ));
        }

        let prior: Option<i64> = transaction
            .query_row(
                "SELECT high_water FROM hold_request_clocks WHERE hold_id=?1",
                [input.hold_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let next = match prior {
            Some(MAX_SQL_REQUEST_VERSION) => {
                return Err(StoreError::GenerationVersionExhausted(
                    input.hold_id.as_str().to_owned(),
                ));
            }
            Some(value) if value > 0 => value + 1,
            Some(_) => return Err(integrity("invalid Hold request clock")),
            None => 1,
        };
        transaction.execute(
            "INSERT INTO hold_request_clocks(hold_id,high_water) VALUES (?1,?2)
             ON CONFLICT(hold_id) DO UPDATE SET high_water=excluded.high_water",
            params![input.hold_id.as_str(), next],
        )?;
        transaction.execute(
            "UPDATE generation_requests SET relevance='stale'
             WHERE hold_id=?1 AND relevance='current'",
            [input.hold_id.as_str()],
        )?;

        let constraints = bounded_json(&input.constraints, "generation constraints")?;
        let provider = bounded_json(&input.provider, "generation provider")?;
        transaction.execute(
            "INSERT INTO generation_requests(
                request_id,project_id,hold_id,request_version,origin_revision,
                context_sha256,constraints,provider,relevance
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'current')",
            params![
                input.request_id.as_str(),
                document.project_id().as_str(),
                input.hold_id.as_str(),
                next,
                document.revision_id().as_str(),
                input.context_sha256.as_str(),
                constraints,
                provider,
            ],
        )?;
        transaction.commit()?;
        Ok(StoredGenerationRequest {
            request_id: input.request_id,
            origin_revision: document.revision_id().clone(),
            binding: TargetBinding {
                project_id: document.project_id().clone(),
                hold_id: input.hold_id,
                request_version: RequestVersion::new(next as u64)
                    .expect("positive SQLite request version fits u64"),
                context_sha256: input.context_sha256,
            },
            constraints: input.constraints,
            provider: input.provider,
            relevance: Relevance::Current,
        })
    }

    pub fn generation_request(
        &self,
        request_id: &RequestId,
    ) -> Result<Option<StoredGenerationRequest>, StoreError> {
        let mut statement = self
            .connection
            .prepare(&format!("{BOUNDED_REQUEST_SELECT} WHERE request_id=?2"))?;
        let mut rows = statement.query(params![MAX_IDENTITY_BYTES as i64, request_id.as_str()])?;
        let result = rows.next()?.map(parse_request_row).transpose()?;
        if rows.next()?.is_some() {
            return Err(integrity("duplicate generation request identity"));
        }
        Ok(result)
    }

    pub fn current_generation_requests(&self) -> Result<Vec<StoredGenerationRequest>, StoreError> {
        read_current_requests(&self.connection)
    }
}

pub(crate) fn ensure_no_current(connection: &Connection) -> Result<(), StoreError> {
    let current: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM generation_requests WHERE relevance='current' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some() {
        return Err(StoreError::GenerationRelevanceRequired);
    }
    Ok(())
}

pub(crate) fn apply_relevance_plan(
    connection: &Connection,
    before: &ProjectDocument,
    after: &ProjectDocument,
    plan: &RelevancePlan,
) -> Result<(), StoreError> {
    if &plan.from_revision != before.revision_id() || &plan.to_revision != after.revision_id() {
        return Err(plan_error(
            "plan revisions do not match the document transition",
        ));
    }
    let current = read_current_requests(connection)?;
    if plan.observations.len() > MAX_DOCUMENT_NODES {
        return Err(plan_error("plan exceeds the current Hold limit"));
    }
    let mut observations = BTreeMap::new();
    for observation in &plan.observations {
        if observations
            .insert(observation.request_id.as_str(), observation)
            .is_some()
        {
            return Err(plan_error("plan contains a duplicate request observation"));
        }
    }
    if observations.len() != current.len() {
        return Err(plan_error(
            "plan does not cover exactly every current generation request",
        ));
    }

    for request in current {
        let observation = observations
            .remove(request.request_id.as_str())
            .ok_or_else(|| plan_error("plan is missing a current generation request"))?;
        if observation.binding != request.binding {
            return Err(plan_error(
                "plan binding does not match the persisted generation request",
            ));
        }
        let relevance = relevance_after(after, &request, &observation.after_context);
        if relevance != Relevance::Current {
            let changed = connection.execute(
                "UPDATE generation_requests SET relevance=?1
                 WHERE request_id=?2 AND relevance='current'",
                params![relevance_text(relevance), request.request_id.as_str()],
            )?;
            if changed != 1 {
                return Err(plan_error(
                    "current generation request changed during reconciliation",
                ));
            }
        }
    }
    Ok(())
}

fn relevance_after(
    document: &ProjectDocument,
    request: &StoredGenerationRequest,
    context: &ContextObservation,
) -> Relevance {
    let Some(node) = document.nodes().get(&request.binding.hold_id) else {
        return Relevance::Detached;
    };
    let NodeKind::Hold { recipe } = &node.kind else {
        return Relevance::Stale;
    };
    if request.constraints.video.frames() != recipe.duration
        || request.constraints.video.frame_rate() != document.presentation_basis().frame_rate
    {
        return Relevance::Stale;
    }
    match context {
        ContextObservation::Resolved(hash) if hash == &request.binding.context_sha256 => {
            Relevance::Current
        }
        ContextObservation::Resolved(_) | ContextObservation::Unresolved => Relevance::Stale,
    }
}

fn validate_new_target(
    document: &ProjectDocument,
    hold_id: &NodeId,
    constraints: &HoldConstraints,
) -> Result<(), StoreError> {
    let node = document
        .nodes()
        .get(hold_id)
        .ok_or_else(|| StoreError::GenerationTarget(format!("Hold {hold_id} does not exist")))?;
    let NodeKind::Hold { recipe } = &node.kind else {
        return Err(StoreError::GenerationTarget(format!(
            "node {hold_id} is not a Hold"
        )));
    };
    if constraints.video.frames() != recipe.duration {
        return Err(StoreError::GenerationTarget(
            "requested frame count differs from the Hold duration".into(),
        ));
    }
    if constraints.video.frame_rate() != document.presentation_basis().frame_rate {
        return Err(StoreError::GenerationTarget(
            "requested frame rate differs from the project rate".into(),
        ));
    }
    Ok(())
}

fn validate_current_target(
    document: &ProjectDocument,
    request: &StoredGenerationRequest,
) -> Result<(), StoreError> {
    validate_new_target(document, &request.binding.hold_id, &request.constraints)
        .map_err(|_| integrity("current generation request is incompatible with the project head"))
}

fn require_revision(document: &ProjectDocument, expected: &RevisionId) -> Result<(), StoreError> {
    if document.revision_id() != expected {
        return Err(StoreError::RevisionConflict {
            expected: expected.as_str().to_owned(),
            current: document.revision_id().as_str().to_owned(),
        });
    }
    Ok(())
}

fn read_current_requests(
    connection: &Connection,
) -> Result<Vec<StoredGenerationRequest>, StoreError> {
    let mut statement = connection.prepare(&format!(
        "{BOUNDED_REQUEST_SELECT} WHERE relevance='current' ORDER BY hold_id"
    ))?;
    let mut rows = statement.query([MAX_IDENTITY_BYTES as i64])?;
    let mut requests = Vec::new();
    while let Some(row) = rows.next()? {
        if requests.len() == MAX_DOCUMENT_NODES {
            return Err(integrity(
                "current generation requests exceed the document node limit",
            ));
        }
        requests.push(parse_request_row(row)?);
    }
    Ok(requests)
}

const BOUNDED_REQUEST_SELECT: &str = "SELECT
    CASE WHEN typeof(request_id)='text'
         AND length(CAST(request_id AS BLOB)) BETWEEN 1 AND ?1 THEN request_id END,
    CASE WHEN typeof(project_id)='text'
         AND length(CAST(project_id AS BLOB)) BETWEEN 1 AND ?1 THEN project_id END,
    CASE WHEN typeof(hold_id)='text'
         AND length(CAST(hold_id AS BLOB)) BETWEEN 1 AND ?1 THEN hold_id END,
    CASE WHEN typeof(request_version)='integer' AND request_version>=1 THEN request_version END,
    CASE WHEN typeof(origin_revision)='text'
         AND length(CAST(origin_revision AS BLOB)) BETWEEN 1 AND ?1 THEN origin_revision END,
    CASE WHEN typeof(context_sha256)='text'
         AND length(CAST(context_sha256 AS BLOB))=64 THEN context_sha256 END,
    CASE WHEN typeof(constraints)='text'
         AND length(CAST(constraints AS BLOB))<=16384 THEN constraints END,
    CASE WHEN typeof(provider)='text'
         AND length(CAST(provider AS BLOB))<=16384 THEN provider END,
    CASE WHEN relevance IN ('current','stale','detached') THEN relevance END
 FROM generation_requests";

fn parse_request_row(row: &Row<'_>) -> Result<StoredGenerationRequest, StoreError> {
    let request_id: Option<String> = row.get(0)?;
    let project_id: Option<String> = row.get(1)?;
    let hold_id: Option<String> = row.get(2)?;
    let version: Option<i64> = row.get(3)?;
    let origin_revision: Option<String> = row.get(4)?;
    let context_sha256: Option<String> = row.get(5)?;
    let constraints: Option<String> = row.get(6)?;
    let provider: Option<String> = row.get(7)?;
    let relevance: Option<String> = row.get(8)?;
    (|| {
        let request_id = RequestId::new(required(request_id, "request ID")?)
            .map_err(|_| integrity("invalid generation request ID"))?;
        let project_id = deadpan_core::ProjectId::new(required(project_id, "project ID")?)
            .map_err(|_| integrity("invalid generation project ID"))?;
        let hold_id = NodeId::new(required(hold_id, "Hold ID")?)
            .map_err(|_| integrity("invalid generation Hold ID"))?;
        let version = version.ok_or_else(|| integrity("invalid generation request version"))?;
        let request_version = RequestVersion::new(version as u64)
            .map_err(|_| integrity("invalid generation request version"))?;
        let origin_revision = RevisionId::new(required(origin_revision, "origin revision")?)
            .map_err(|_| integrity("invalid generation origin revision"))?;
        let context_sha256 = Sha256::new(required(context_sha256, "context SHA-256")?)
            .map_err(|_| integrity("invalid generation context SHA-256"))?;
        let constraints = strict_json(
            &required(constraints, "constraints")?,
            "generation constraints",
        )?;
        let provider = strict_json(&required(provider, "provider")?, "generation provider")?;
        let relevance = match required(relevance, "relevance")?.as_str() {
            "current" => Relevance::Current,
            "stale" => Relevance::Stale,
            "detached" => Relevance::Detached,
            _ => return Err(integrity("invalid generation relevance")),
        };
        Ok(StoredGenerationRequest {
            request_id,
            origin_revision,
            binding: TargetBinding {
                project_id,
                hold_id,
                request_version,
                context_sha256,
            },
            constraints,
            provider,
            relevance,
        })
    })()
}

fn bounded_json(value: &impl serde::Serialize, label: &str) -> Result<String, StoreError> {
    let json = serde_json::to_string(value)?;
    if json.len() > MAX_REQUEST_JSON_BYTES {
        return Err(StoreError::GenerationTarget(format!(
            "{label} exceeds the persistence limit"
        )));
    }
    Ok(json)
}

fn strict_json<T: serde::de::DeserializeOwned>(json: &str, label: &str) -> Result<T, StoreError> {
    serde_json::from_str(json).map_err(|_| integrity(&format!("invalid stored {label}")))
}

fn required(value: Option<String>, label: &str) -> Result<String, StoreError> {
    value.ok_or_else(|| integrity(&format!("invalid or oversized generation {label}")))
}

fn request_version_i64(version: RequestVersion) -> Result<i64, StoreError> {
    i64::try_from(version.get()).map_err(|_| integrity("generation request version exceeds SQLite"))
}

fn relevance_text(relevance: Relevance) -> &'static str {
    match relevance {
        Relevance::Current => "current",
        Relevance::Stale => "stale",
        Relevance::Detached => "detached",
    }
}

fn integrity(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}

fn plan_error(message: &str) -> StoreError {
    StoreError::GenerationPlan(message.into())
}
