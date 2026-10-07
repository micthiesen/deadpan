//! Stable generation clocks for explicit authored Hold scopes.
//!
//! A worker keeps its original Hold identity. Only this operational address
//! follows proven structural isolation; relevance never travels backward.

use deadpan_core::{MAX_IDENTITY_BYTES, ProjectDocument, RevisionId, ScopedNodeTarget};
use deadpan_jobs::{RequestId, RequestVersion};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::StoreError;

mod transitions;
mod validation;
pub(crate) use transitions::{
    Event, command_transition, history_transition, insert_event, preview_command, preview_history,
    read_event, with_command_proof, with_history_proof,
};
pub(crate) use validation::{Replay, digest};

pub(crate) const MAX_TARGET_BYTES: usize = 128 * 1024;
const _: () = assert!(MAX_TARGET_BYTES <= crate::schema::MAX_DOCUMENT_BYTES);

/// The first request's identity names its stable authoring scope. Later
/// requests advance this scope's clock even after its Hold is isolated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GenerationScopeId(RequestId);

impl GenerationScopeId {
    pub fn from_first_request(request: RequestId) -> Self {
        Self(request)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug)]
pub(crate) struct ScopeRecord {
    pub id: GenerationScopeId,
    pub origin_revision: RevisionId,
    pub origin_target: ScopedNodeTarget,
    pub target: ScopedNodeTarget,
    pub high_water: i64,
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE generation_scopes (
            scope_id TEXT PRIMARY KEY,
            origin_revision TEXT NOT NULL REFERENCES revisions(id),
            origin_target TEXT NOT NULL CHECK (json_valid(origin_target)),
            current_target TEXT NOT NULL UNIQUE CHECK (json_valid(current_target)),
            high_water INTEGER NOT NULL CHECK (high_water BETWEEN 1 AND 9223372036854775807)
        ) STRICT;
        CREATE INDEX generation_scopes_origin ON generation_scopes(origin_revision);
        CREATE TABLE generation_scope_events (
            revision_id TEXT PRIMARY KEY REFERENCES revisions(id),
            history_id INTEGER NOT NULL REFERENCES history(id),
            direction TEXT NOT NULL CHECK(direction IN ('forward','inverse'))
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn target_json(target: &ScopedNodeTarget) -> Result<String, StoreError> {
    let json = serde_json::to_string(target)?;
    if json.len() > MAX_TARGET_BYTES {
        return Err(StoreError::GenerationTarget(
            "generation scope exceeds the persistence limit".into(),
        ));
    }
    Ok(json)
}

pub(crate) fn parse_target(json: &str) -> Result<ScopedNodeTarget, StoreError> {
    if json.len() > MAX_TARGET_BYTES {
        return Err(integrity("generation scope exceeds the persistence limit"));
    }
    let target: ScopedNodeTarget =
        serde_json::from_str(json).map_err(|_| integrity("invalid generation scope target"))?;
    if target_json(&target)? != json {
        return Err(integrity("generation scope target is not canonical"));
    }
    Ok(target)
}

pub(crate) fn parse_id(id: String) -> Result<GenerationScopeId, StoreError> {
    RequestId::new(id)
        .map(GenerationScopeId)
        .map_err(|_| integrity("invalid generation scope identity"))
}

pub(crate) fn allocate(
    connection: &Connection,
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    first_request: &RequestId,
) -> Result<(GenerationScopeId, RequestVersion), StoreError> {
    target.validate(document)?;
    let json = target_json(target)?;
    let existing: Option<(String, i64)> = connection
        .query_row(
            "SELECT scope_id,high_water FROM generation_scopes WHERE current_target=?1",
            [&json],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (scope, previous) = match existing {
        Some((id, value)) if value > 0 => (parse_id(id)?, Some(value)),
        Some(_) => return Err(integrity("invalid generation scope clock")),
        None => (
            GenerationScopeId::from_first_request(first_request.clone()),
            None,
        ),
    };
    let retired = crate::retired::floor(connection, "generation_scope_version", scope.as_str())?;
    let previous = previous.unwrap_or(0).max(retired);
    let next = previous
        .checked_add(1)
        .filter(|value| *value > 0)
        .ok_or_else(|| StoreError::GenerationVersionExhausted(scope.as_str().to_owned()))?;
    connection.execute(
        "INSERT INTO generation_scopes(scope_id,origin_revision,origin_target,current_target,high_water)
         VALUES (?1,?2,?3,?3,?4)
         ON CONFLICT(scope_id) DO UPDATE SET high_water=excluded.high_water",
        params![scope.as_str(), document.revision_id().as_str(), json, next],
    )?;
    Ok((
        scope,
        RequestVersion::new(next as u64).expect("positive SQLite version"),
    ))
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let invalid: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM generation_scopes WHERE
            typeof(scope_id)!='text' OR length(CAST(scope_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(origin_revision)!='text' OR length(CAST(origin_revision AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(origin_target)!='text' OR length(CAST(origin_target AS BLOB)) NOT BETWEEN 1 AND ?2 OR
            typeof(current_target)!='text' OR length(CAST(current_target AS BLOB)) NOT BETWEEN 1 AND ?2 OR
            typeof(high_water)!='integer' OR high_water<1)",
        params![MAX_IDENTITY_BYTES as i64, MAX_TARGET_BYTES as i64],
        |row| row.get(0),
    )?;
    if invalid {
        return Err(integrity("invalid or oversized generation scope metadata"));
    }
    transitions::check_stored_sizes(connection)?;
    validation::check_budget(connection)?;
    Ok(())
}

pub(crate) fn read(
    connection: &Connection,
    id: &GenerationScopeId,
) -> Result<ScopeRecord, StoreError> {
    type ScopeRow = (Option<String>, Option<String>, Option<String>, Option<i64>);
    let value: Option<ScopeRow> = connection
        .query_row(
            "SELECT
                CASE WHEN typeof(origin_revision)='text' AND length(CAST(origin_revision AS BLOB)) BETWEEN 1 AND ?2 THEN origin_revision END,
                CASE WHEN typeof(origin_target)='text' AND length(CAST(origin_target AS BLOB)) BETWEEN 1 AND ?3 THEN origin_target END,
                CASE WHEN typeof(current_target)='text' AND length(CAST(current_target AS BLOB)) BETWEEN 1 AND ?3 THEN current_target END,
                CASE WHEN typeof(high_water)='integer' AND high_water>=1 THEN high_water END
             FROM generation_scopes WHERE scope_id=?1",
            params![id.as_str(), MAX_IDENTITY_BYTES as i64, MAX_TARGET_BYTES as i64],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((Some(revision), Some(origin), Some(target), Some(high_water))) = value else {
        return Err(integrity("generation request lacks a valid scope clock"));
    };
    Ok(ScopeRecord {
        id: id.clone(),
        origin_revision: RevisionId::new(revision)
            .map_err(|_| integrity("invalid scope revision"))?,
        origin_target: parse_target(&origin)?,
        target: parse_target(&target)?,
        high_water,
    })
}

fn integrity(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}

impl crate::ProjectStore {
    /// Preview an authored command and the current requests' after addresses.
    /// No scope, relevance, document or history row is changed.
    pub fn preview_generation_contexts(
        &self,
        request: &deadpan_core::CommandRequest,
    ) -> Result<
        (
            deadpan_core::EditTransaction,
            Vec<crate::generation::StoredGenerationRequest>,
        ),
        StoreError,
    > {
        let transaction = self.connection.unchecked_transaction()?;
        let plan = crate::prepare_command(&transaction, &self.documents, request)?;
        crate::compound::require_authored(&plan)?;
        let contexts = preview_command(&transaction, &plan.current, request, &plan.next)?;
        Ok((plan.edit, contexts))
    }
}
