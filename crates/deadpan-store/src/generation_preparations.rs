//! Durable AI intent before conditioning has a real content hash.
//! These records never authorize a worker or acceptance. Fulfilment binds a
//! current claim to a normal request and its first attempt in one transaction.

use deadpan_core::{FrameDuration, GeneratedArtifact, ProjectId, RevisionId, ScopedNodeTarget};
use deadpan_jobs::{BridgeGenerationPlan, GenerationOptions, RequestId};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::generation::{GenerationRequestInput, StoredGenerationRequest};
use crate::generation_attempts::{BeginGenerationAttempt, StoredGenerationAttempt};
use crate::{ProjectStore, StoreError};

mod retention;
mod transitions;
mod validation;
pub(crate) use transitions::{
    Birth, apply_command_terminals, apply_history_terminals, command_births, derive,
    history_births, id_for, insert_births, map, provider_choices, reconcile,
};
pub(crate) use validation::{Replay, digest, validate_store};

pub const MAX_PREPARATIONS: usize = 4096;
pub const MAX_PREPARATION_PAGE: usize = 256;
const MAX_TERMINAL_PREPARATIONS: usize = 256;
const MAX_ROW_BYTES: usize = 512 * 1024;
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const MAX_REASON_BYTES: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PreparationId(String);

impl PreparationId {
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        RequestId::new(value.clone()).map_err(|_| invalid("invalid preparation identity"))?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for PreparationId {
    type Error = StoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<PreparationId> for String {
    fn from(value: PreparationId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationState {
    Queued,
    Claimed,
    Interrupted,
    Unavailable,
    Fulfilled,
    Cancelled,
}
impl PreparationState {
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Fulfilled | Self::Cancelled)
    }
    fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Claimed => "claimed",
            Self::Interrupted => "interrupted",
            Self::Unavailable => "unavailable",
            Self::Fulfilled => "fulfilled",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationControls {
    Request {
        request_id: RequestId,
        options: GenerationOptions,
    },
    /// Read the retained accepted artifact's validated provenance off the writer.
    AcceptedArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationOrigin {
    AcceptedBoundary {
        accepted: Box<GeneratedArtifact>,
        controls: PreparationControls,
    },
    AcceptedExtension {
        accepted: Box<GeneratedArtifact>,
        controls: PreparationControls,
    },
    InsertedPause {
        options: GenerationOptions,
    },
}

impl PreparationOrigin {
    /// The InsertAiTime contract fixes these controls at birth. Do not use
    /// mutable UI preferences or future GenerationOptions defaults on replay.
    pub fn inserted_pause() -> Self {
        Self::InsertedPause {
            options: GenerationOptions {
                motion: deadpan_jobs::MotionAmount::Still,
                instructions: None,
                region_target: deadpan_jobs::GenerationTarget::None,
            },
        }
    }

    /// None means verified accepted provenance must be read off the writer.
    pub fn options(&self) -> Option<&GenerationOptions> {
        match self {
            Self::InsertedPause { options }
            | Self::AcceptedExtension {
                controls: PreparationControls::Request { options, .. },
                ..
            }
            | Self::AcceptedBoundary {
                controls: PreparationControls::Request { options, .. },
                ..
            } => Some(options),
            Self::AcceptedExtension {
                controls: PreparationControls::AcceptedArtifact,
                ..
            }
            | Self::AcceptedBoundary {
                controls: PreparationControls::AcceptedArtifact,
                ..
            } => None,
        }
    }

    pub fn accepted_artifact(&self) -> Option<&GeneratedArtifact> {
        match self {
            Self::AcceptedExtension { accepted, .. } | Self::AcceptedBoundary { accepted, .. } => {
                Some(accepted.as_ref())
            }
            Self::InsertedPause { .. } => None,
        }
    }

    fn source_request(&self) -> Option<&RequestId> {
        match self {
            Self::AcceptedExtension {
                controls: PreparationControls::Request { request_id, .. },
                ..
            }
            | Self::AcceptedBoundary {
                controls: PreparationControls::Request { request_id, .. },
                ..
            } => Some(request_id),
            _ => None,
        }
    }

    fn supports_duration(&self, duration: FrameDuration) -> bool {
        duration != FrameDuration::ZERO
            && match self {
                Self::AcceptedExtension { accepted, .. } => {
                    duration > accepted.sampling.output_frame_count()
                }
                _ => true,
            }
    }

    /// Request controls enrich an accepted birth after pure command replay;
    /// validation separately proves that request's exact scope and controls.
    fn same_birth(&self, derived: &Self) -> bool {
        match (self, derived) {
            (
                Self::AcceptedExtension { accepted, .. },
                Self::AcceptedExtension {
                    accepted: expected, ..
                },
            ) => accepted == expected,
            (
                Self::AcceptedBoundary { accepted, .. },
                Self::AcceptedBoundary {
                    accepted: expected, ..
                },
            ) => accepted == expected,
            (Self::InsertedPause { options }, Self::InsertedPause { options: expected }) => {
                options == expected
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredGenerationPreparation {
    pub id: PreparationId,
    pub project_id: ProjectId,
    pub origin_revision: RevisionId,
    pub origin_target: ScopedNodeTarget,
    pub current_revision: RevisionId,
    pub target: ScopedNodeTarget,
    pub duration: FrameDuration,
    pub origin: PreparationOrigin,
    pub intent: crate::generation_intents::IntentBirthReceipt,
    pub state: PreparationState,
    pub claim_sequence: u64,
    pub reason: Option<String>,
    pub request_id: Option<RequestId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationClaim {
    pub preparation: StoredGenerationPreparation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparationFailure {
    Unavailable(String),
    Interrupted(String),
    Cancelled(String),
}

/// The edit still commits if the bounded pending queue is full. The first
/// displaced IDs and the complete count explain the cancelled background work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationNotice {
    IntentCapacity {
        displaced: Vec<PreparationId>,
        total: u64,
    },
    QueueCapacity {
        displaced: Vec<PreparationId>,
        total: u64,
    },
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch("CREATE TABLE generation_preparations (
        id TEXT PRIMARY KEY,
        origin_revision TEXT NOT NULL REFERENCES revisions(id),
        current_revision TEXT NOT NULL REFERENCES revisions(id),
        history_id INTEGER NOT NULL REFERENCES history(id),
        state TEXT NOT NULL CHECK(state IN ('queued','claimed','interrupted','unavailable','fulfilled','cancelled')),
        record TEXT NOT NULL CHECK(json_valid(record)),
        charged_bytes INTEGER NOT NULL CHECK(charged_bytes>0)
    ) STRICT;
    CREATE INDEX generation_preparations_origin ON generation_preparations(origin_revision);
    CREATE INDEX generation_preparations_state ON generation_preparations(state,id);")?;
    retention::create_tables(connection)?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let (count, bytes, bad, active): (i64, i64, i64, i64) = connection.query_row(
        "SELECT count(*),coalesce(sum(charged_bytes),0),
         coalesce(sum(CASE WHEN typeof(record)!='text' OR length(CAST(record AS BLOB))>?1
          OR typeof(id)!='text' OR length(CAST(id AS BLOB)) NOT BETWEEN 1 AND ?2
          OR typeof(origin_revision)!='text' OR length(CAST(origin_revision AS BLOB)) NOT BETWEEN 1 AND ?2
          OR typeof(current_revision)!='text' OR length(CAST(current_revision AS BLOB)) NOT BETWEEN 1 AND ?2
          OR typeof(state)!='text' OR length(CAST(state AS BLOB)) NOT BETWEEN 1 AND 11
          OR typeof(charged_bytes)!='integer' OR charged_bytes<length(CAST(record AS BLOB)) OR charged_bytes>?1
          OR typeof(history_id)!='integer' OR history_id<1 THEN 1 ELSE 0 END),0),
         coalesce(sum(state NOT IN ('fulfilled','cancelled')),0)
         FROM generation_preparations", params![MAX_ROW_BYTES as i64, deadpan_core::MAX_IDENTITY_BYTES as i64],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if count > (MAX_PREPARATIONS + MAX_TERMINAL_PREPARATIONS) as i64
        || active > MAX_PREPARATIONS as i64
        || bytes > MAX_TOTAL_BYTES as i64
        || bad != 0
    {
        return Err(invalid("preparation records exceed their bounds"));
    }
    retention::check_sizes(connection)?;
    Ok(())
}

fn parse_row(row: &rusqlite::Row<'_>) -> Result<(StoredGenerationPreparation, i64), StoreError> {
    let text: String = row.get(0)?;
    if text.len() > MAX_ROW_BYTES {
        return Err(invalid("preparation row exceeds its byte limit"));
    }
    let value: StoredGenerationPreparation = serde_json::from_str(&text)?;
    let id: String = row.get(1)?;
    let origin: String = row.get(2)?;
    let current: String = row.get(3)?;
    let state: String = row.get(4)?;
    let history: i64 = row.get(5)?;
    let charged: i64 = row.get(6)?;
    if value.id.as_str() != id
        || value.origin_revision.as_str() != origin
        || value.current_revision.as_str() != current
        || value.state.name() != state
        || serde_json::to_string(&value)? != text
        || history < 1
        || value.intent.history_id != history
        || value.duration.frames() <= 0
        || value.claim_sequence > i64::MAX as u64
        || value
            .reason
            .as_ref()
            .is_some_and(|reason| reason.is_empty() || reason.len() > MAX_REASON_BYTES)
        || (value.state == PreparationState::Fulfilled) != value.request_id.is_some()
        || (matches!(
            value.state,
            PreparationState::Queued | PreparationState::Claimed | PreparationState::Fulfilled
        )) != value.reason.is_none()
        || (value.state == PreparationState::Claimed && value.claim_sequence == 0)
        || charged != charged_bytes(&value, text.len())? as i64
    {
        return Err(invalid("preparation columns or state are inconsistent"));
    }
    value.intent_birth().validate()?;
    Ok((value, history))
}

/// Reserve all mutable growth before admitting background work. Isolation
/// changes node identities only; repeat choices and depth stay fixed. Core
/// and protocol identities are bounded ASCII and require no JSON escaping.
/// Reasons may contain control characters, whose JSON encoding uses six bytes.
/// The charge stays constant across every operational transition and mapping.
fn charged_bytes(
    value: &StoredGenerationPreparation,
    serialized: usize,
) -> Result<usize, StoreError> {
    let identity = deadpan_core::MAX_IDENTITY_BYTES;
    let mut bytes = serialized + identity - value.current_revision.as_str().len() + identity
        - value.target.node.as_str().len()
        + 11
        - value.state.name().len()
        + 19
        - value.claim_sequence.to_string().len();
    for step in &value.target.repeats {
        bytes += identity - step.repeat.as_str().len();
    }
    bytes += 2 + 6 * MAX_REASON_BYTES - serde_json::to_vec(&value.reason)?.len();
    bytes += 2 + deadpan_jobs::MAX_PROTOCOL_ID_BYTES - serde_json::to_vec(&value.request_id)?.len();
    if bytes > MAX_ROW_BYTES {
        return Err(invalid("preparation exceeds its reserved byte limit"));
    }
    Ok(bytes)
}

fn read(
    connection: &Connection,
    id: &PreparationId,
) -> Result<Option<StoredGenerationPreparation>, StoreError> {
    let mut statement = connection.prepare("SELECT record,id,origin_revision,current_revision,state,history_id,charged_bytes FROM generation_preparations WHERE id=?1")?;
    let mut rows = statement.query([id.as_str()])?;
    rows.next()?
        .map(parse_row)
        .transpose()
        .map(|value| value.map(|(record, _)| record))
}

pub(crate) fn all(
    connection: &Connection,
) -> Result<Vec<(StoredGenerationPreparation, i64)>, StoreError> {
    check_stored_sizes(connection)?;
    let mut statement = connection.prepare("SELECT record,id,origin_revision,current_revision,state,history_id,charged_bytes FROM generation_preparations ORDER BY id")?;
    let mut rows = statement.query([])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(parse_row(row)?);
    }
    Ok(result)
}

fn save(connection: &Connection, value: &StoredGenerationPreparation) -> Result<(), StoreError> {
    let json = serde_json::to_string(value)?;
    if json.len() > MAX_ROW_BYTES {
        return Err(invalid("preparation exceeds its byte limit"));
    }
    connection.execute(
        "UPDATE generation_preparations SET current_revision=?1,state=?2,record=?3 WHERE id=?4",
        params![
            value.current_revision.as_str(),
            value.state.name(),
            json,
            value.id.as_str()
        ],
    )?;
    Ok(())
}

fn advance(value: &mut StoredGenerationPreparation) -> Result<(), StoreError> {
    value.claim_sequence = value
        .claim_sequence
        .checked_add(1)
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or_else(|| invalid("preparation claim sequence exhausted"))?;
    Ok(())
}

fn reason(text: String) -> Result<String, StoreError> {
    if text.is_empty() || text.len() > MAX_REASON_BYTES {
        return Err(invalid("preparation reason must be 1–2048 bytes"));
    }
    Ok(text)
}

pub(crate) fn verify(connection: &Connection) -> Result<(), StoreError> {
    crate::validation::validate_history(connection, crate::validation::HistoryMode::Receipt)?;
    validate_store(connection)
}

fn claim_matches(connection: &Connection, claim: &PreparationClaim) -> Result<bool, StoreError> {
    Ok(claim.preparation.state == PreparationState::Claimed
        && crate::validation::read_head(connection)? == claim.preparation.current_revision.as_str()
        && read(connection, &claim.preparation.id)?.as_ref() == Some(&claim.preparation)
        && has_current_intent(connection, &claim.preparation)?)
}

impl ProjectStore {
    /// Page only unfinished, displayable preparations. Terminal history never
    /// hides queued work behind a full first page. Direct reads include recent
    /// terminals; old terminal records are compacted into immutable history proofs.
    pub fn generation_preparations(
        &self,
        after: Option<&PreparationId>,
        limit: usize,
    ) -> Result<Vec<StoredGenerationPreparation>, StoreError> {
        if !(1..=MAX_PREPARATION_PAGE).contains(&limit) {
            return Err(invalid("preparation page must contain 1–256 rows"));
        }
        verify(&self.connection)?;
        let mut statement = self.connection.prepare("SELECT record,id,origin_revision,current_revision,state,history_id,charged_bytes FROM generation_preparations
            WHERE state IN ('queued','claimed','interrupted','unavailable') AND (?1 IS NULL OR id>?1) ORDER BY id LIMIT ?2")?;
        let mut rows = statement.query(params![after.map(PreparationId::as_str), limit as i64])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(parse_row(row)?.0);
        }
        Ok(result)
    }

    pub fn generation_preparation(
        &self,
        id: &PreparationId,
    ) -> Result<Option<StoredGenerationPreparation>, StoreError> {
        verify(&self.connection)?;
        read(&self.connection, id)
    }

    pub fn claim_generation_preparation(
        &mut self,
        id: &PreparationId,
        expected: &RevisionId,
    ) -> Result<PreparationClaim, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        verify(&transaction)?;
        let mut value =
            read(&transaction, id)?.ok_or_else(|| invalid("preparation does not exist"))?;
        if value.state != PreparationState::Queued
            || &value.current_revision != expected
            || crate::validation::read_head(&transaction)? != expected.as_str()
            || !has_current_intent(&transaction, &value)?
        {
            return Err(invalid(
                "preparation is not queued at the captured revision",
            ));
        }
        advance(&mut value)?;
        value.state = PreparationState::Claimed;
        save(&transaction, &value)?;
        retention::compact(&transaction)?;
        crate::audit::refresh_generation_scopes(&transaction)?;
        transaction.commit()?;
        Ok(PreparationClaim { preparation: value })
    }

    pub fn retry_generation_preparation(
        &mut self,
        id: &PreparationId,
        expected: &RevisionId,
    ) -> Result<StoredGenerationPreparation, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        verify(&transaction)?;
        let mut value =
            read(&transaction, id)?.ok_or_else(|| invalid("preparation does not exist"))?;
        if !matches!(
            value.state,
            PreparationState::Interrupted | PreparationState::Unavailable
        ) || &value.current_revision != expected
            || crate::validation::read_head(&transaction)? != expected.as_str()
            || !has_current_intent(&transaction, &value)?
        {
            return Err(invalid("preparation cannot retry at the captured revision"));
        }
        advance(&mut value)?;
        value.state = PreparationState::Queued;
        value.reason = None;
        save(&transaction, &value)?;
        retention::compact(&transaction)?;
        crate::audit::refresh_generation_scopes(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }

    pub fn cancel_generation_preparation(
        &mut self,
        id: &PreparationId,
        expected_sequence: u64,
    ) -> Result<StoredGenerationPreparation, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        verify(&transaction)?;
        let mut value =
            read(&transaction, id)?.ok_or_else(|| invalid("preparation does not exist"))?;
        if !value.state.is_active() || value.claim_sequence != expected_sequence {
            return Err(invalid("preparation changed before cancellation"));
        }
        let head = crate::generation_intents::read_head(&transaction, id)?
            .ok_or_else(|| invalid("preparation no longer owns current intent"))?;
        crate::generation_intents::close(
            &transaction,
            &head,
            crate::generation_intents::IntentTerminal {
                activation_id: id.clone(),
                at_revision: value.current_revision.clone(),
                phase: crate::generation_intents::TerminalPhase::AfterRevision,
                target: head.target.clone(),
                reason: crate::generation_intents::IntentTerminalReason::UserCancelled,
            },
        )?;
        value =
            read(&transaction, id)?.ok_or_else(|| invalid("cancelled preparation disappeared"))?;
        retention::compact(&transaction)?;
        crate::audit::refresh_generation_scopes(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }

    pub fn generation_preparation_claim_is_current(
        &self,
        claim: &PreparationClaim,
    ) -> Result<bool, StoreError> {
        verify(&self.connection)?;
        claim_matches(&self.connection, claim)
    }

    /// A late or superseded result writes nothing and returns false.
    pub fn finish_generation_preparation(
        &mut self,
        claim: &PreparationClaim,
        failure: PreparationFailure,
    ) -> Result<bool, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        verify(&transaction)?;
        if !claim_matches(&transaction, claim)? {
            return Ok(false);
        }
        let mut value = claim.preparation.clone();
        let (state, text) = match failure {
            PreparationFailure::Unavailable(text) => (PreparationState::Unavailable, text),
            PreparationFailure::Interrupted(text) => (PreparationState::Interrupted, text),
            PreparationFailure::Cancelled(text) => (PreparationState::Cancelled, text),
        };
        advance(&mut value)?;
        value.state = state;
        value.reason = Some(reason(text)?);
        save(&transaction, &value)?;
        retention::compact(&transaction)?;
        crate::audit::refresh_generation_scopes(&transaction)?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn fulfil_generation_preparation(
        &mut self,
        claim: &PreparationClaim,
        input: GenerationRequestInput,
        plan: BridgeGenerationPlan,
        attempt: BeginGenerationAttempt,
    ) -> Result<(StoredGenerationRequest, StoredGenerationAttempt), StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        verify(&transaction)?;
        if !claim_matches(&transaction, claim)? {
            return Err(invalid("preparation claim is no longer current"));
        }
        let mut value = claim.preparation.clone();
        if input.expected_revision != value.current_revision
            || input.hold_id != value.target.node
            || input.constraints.video.frames() != value.duration
            || attempt.identity.request_id != input.request_id
        {
            return Err(invalid("prepared request differs from its captured intent"));
        }
        if let Some(options) = value.origin.options()
            && &GenerationOptions::from_constraints(&input.constraints) != options
        {
            return Err(invalid(
                "prepared request changed its captured generation controls",
            ));
        }
        let request = crate::generation::allocate_request(
            &transaction,
            input,
            Some(plan),
            Some(value.target.clone()),
            false,
        )?;
        let attempt = crate::generation_attempts::begin_attempt(&transaction, attempt)?;
        crate::generation_intents::link_request(
            &transaction,
            &value.id,
            &value.target,
            &request.request_id,
        )?;
        value.state = PreparationState::Fulfilled;
        value.request_id = Some(request.request_id.clone());
        advance(&mut value)?;
        save(&transaction, &value)?;
        retention::compact(&transaction)?;
        crate::audit::refresh_generation_scopes(&transaction)?;
        transaction.commit()?;
        Ok((request, attempt))
    }
}

pub(crate) fn supersede(
    connection: &Connection,
    target: &ScopedNodeTarget,
    request_id: &RequestId,
) -> Result<(), StoreError> {
    if let Some(head) = crate::generation_intents::load_current(connection, target)? {
        crate::generation_intents::close(
            connection,
            &head,
            crate::generation_intents::IntentTerminal {
                activation_id: head.activation_id.clone(),
                at_revision: RevisionId::new(crate::validation::read_head(connection)?)?,
                phase: crate::generation_intents::TerminalPhase::AfterRevision,
                target: head.target.clone(),
                reason:
                    crate::generation_intents::IntentTerminalReason::ExplicitGenerationRequest {
                        request_id: request_id.clone(),
                    },
            },
        )?;
    }
    retention::compact(connection)
}

pub(crate) fn has_current_intent(
    connection: &Connection,
    value: &StoredGenerationPreparation,
) -> Result<bool, StoreError> {
    Ok(crate::generation_intents::read_head(connection, &value.id)?
        .is_some_and(|head| head.target == value.target && head.request_id == value.request_id))
}

pub(crate) fn close_intent_work(
    connection: &Connection,
    id: &PreparationId,
    detail: &str,
) -> Result<(), StoreError> {
    if let Some(mut value) = read(connection, id)?
        && value.state.is_active()
    {
        advance(&mut value)?;
        value.state = PreparationState::Cancelled;
        value.reason = Some(reason(detail.into())?);
        save(connection, &value)?;
    }
    Ok(())
}

impl StoredGenerationPreparation {
    pub fn intent_birth(&self) -> crate::generation_intents::IntentBirth {
        crate::generation_intents::IntentBirth {
            activation_id: self.id.clone(),
            project_id: self.project_id.clone(),
            activation_revision: self.origin_revision.clone(),
            origin_target: self.origin_target.clone(),
            duration: self.duration,
            origin: self.origin.clone(),
            receipt: self.intent.clone(),
        }
    }
}

pub(crate) fn read_intent_birth(
    connection: &Connection,
    id: &PreparationId,
) -> Result<Option<crate::generation_intents::IntentBirth>, StoreError> {
    if let Some(value) = read(connection, id)? {
        return Ok(Some(value.intent_birth()));
    }
    Ok(retention::read(connection, id)?.map(|value| value.birth()))
}

pub(crate) fn read_intent_births_at(
    connection: &Connection,
    revision: &RevisionId,
) -> Result<Vec<crate::generation_intents::IntentBirth>, StoreError> {
    let mut statement = connection.prepare("SELECT id FROM generation_preparations WHERE origin_revision=?1 UNION ALL SELECT id FROM generation_preparation_retirements WHERE origin_revision=?1 ORDER BY id")?;
    let mut rows = statement.query([revision.as_str()])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        if result.len() >= deadpan_core::MAX_DOCUMENT_NODES {
            return Err(invalid("birth batch exceeds its node bound"));
        }
        let id = PreparationId::new(row.get::<_, String>(0)?)?;
        result.push(
            read_intent_birth(connection, &id)?
                .ok_or_else(|| invalid("indexed birth is missing"))?,
        );
    }
    Ok(result)
}

pub(crate) fn intent_request(
    connection: &Connection,
    id: &PreparationId,
) -> Result<Option<RequestId>, StoreError> {
    if let Some(value) = read(connection, id)? {
        return Ok(value.request_id);
    }
    retention::read(connection, id)?
        .map(|value| value.request_id)
        .ok_or_else(|| invalid("intent has no preparation birth"))
}

#[cfg(test)]
pub(crate) fn compact_completed_for_test(connection: &Connection) -> Result<(), StoreError> {
    retention::compact_to(connection, 0)
}

pub(crate) fn recover(connection: &mut Connection) -> Result<(), StoreError> {
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for (mut value, _) in all(&transaction)? {
        if value.state == PreparationState::Claimed {
            advance(&mut value)?;
            value.state = PreparationState::Interrupted;
            value.reason = Some("The previous writer ended before conditioning completed. Retry prepares fresh inputs.".into());
            save(&transaction, &value)?;
        }
    }
    retention::compact(&transaction)?;
    crate::audit::refresh_generation_scopes(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Integrity(format!("generation preparation: {}", message.into()))
}
