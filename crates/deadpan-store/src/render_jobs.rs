//! Operational render intent and attempt evidence, independent of authored
//! history and generation relevance. No stored state grants publication trust.

#[path = "render_jobs/admission.rs"]
mod admission;
pub(crate) use admission::create_decision_table;
#[cfg(test)]
#[path = "render_jobs/audit_tests.rs"]
mod audit_tests;
#[cfg(test)]
#[path = "render_jobs/test_fixture.rs"]
pub(crate) mod test_fixture;

use crate::{
    ProjectStore, StoreError,
    render_media::{PreparedRenderRetention, RenderCandidateMedia},
    validation,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        self, MAX_RENDER_COUNTER, RenderAttemptIdentity, RenderAttemptState, RenderDiagnostic,
        RenderIntent, RenderVerificationObservation,
    },
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::atomic::AtomicBool, time::Instant};

pub const MAX_RENDER_JOBS: i64 = 4096;
pub const MAX_RENDER_ATTEMPTS: i64 = 65536;
pub const MAX_RENDER_PAGE: u32 = 256;
const MAX_JOB_BYTES: i64 = 8192;
const MAX_ATTEMPT_BYTES: i64 = 272 * 1024;
const MAX_CHECKPOINT_BYTES: i64 = 8192;
const ACTIVE: &str = "'queued','encoding','encoded_retained','verifying','cancelling'";
const ACTIVE_INDEX: &str = "CREATE UNIQUE INDEX one_active_render_attempt ON render_attempts((1)) WHERE state IN ('queued','encoding','encoded_retained','verifying','cancelling')";
const TOKEN_INDEX: &str =
    "CREATE UNIQUE INDEX unique_render_cancellation_token ON render_attempts(cancellation_token)";

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch("CREATE TABLE render_jobs (
        job_id TEXT PRIMARY KEY, revision_id TEXT NOT NULL REFERENCES revisions(id),
        intent TEXT NOT NULL CHECK(json_valid(intent))
    ) STRICT;
    CREATE TABLE render_attempts (
        job_id TEXT NOT NULL REFERENCES render_jobs(job_id), attempt_id TEXT PRIMARY KEY,
        ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 1 AND 9223372036854775807),
        cancellation_token TEXT NOT NULL, state TEXT NOT NULL,
        transition_sequence INTEGER NOT NULL CHECK(transition_sequence BETWEEN 1 AND 9223372036854775807),
        body TEXT NOT NULL CHECK(json_valid(body)), UNIQUE(job_id,ordinal), UNIQUE(job_id,attempt_id)
    ) STRICT;
    CREATE TABLE render_job_heads (
        job_id TEXT PRIMARY KEY REFERENCES render_jobs(job_id),
        high_water INTEGER NOT NULL CHECK(high_water BETWEEN 1 AND 9223372036854775807),
        latest_attempt_id TEXT NOT NULL,
        FOREIGN KEY(job_id,latest_attempt_id) REFERENCES render_attempts(job_id,attempt_id)
    ) STRICT;
    CREATE TABLE render_candidate_checkpoints (
        job_id TEXT NOT NULL, attempt_id TEXT PRIMARY KEY,
        media TEXT NOT NULL CHECK(json_valid(media)),
        FOREIGN KEY(job_id,attempt_id) REFERENCES render_attempts(job_id,attempt_id)
    ) STRICT;")?;
    connection.execute_batch(ACTIVE_INDEX)?;
    connection.execute_batch(TOKEN_INDEX)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredRenderAttempt {
    pub job_id: RequestId,
    pub attempt_id: AttemptId,
    pub cancellation_token: CancellationToken,
    pub ordinal: u64,
    pub transition_sequence: u64,
    pub state: RenderAttemptState,
    /// Original encoding attempt, preserved through every verification retry.
    pub checkpoint_attempt_id: Option<AttemptId>,
    pub cancellation_requested: bool,
    pub diagnostic: Option<RenderDiagnostic>,
    pub verification: Option<RenderVerificationObservation>,
}
impl StoredRenderAttempt {
    pub fn identity(&self) -> RenderAttemptIdentity {
        RenderAttemptIdentity {
            job_id: self.job_id.clone(),
            attempt_id: self.attempt_id.clone(),
            cancellation_token: self.cancellation_token.clone(),
            expected_sequence: self.transition_sequence,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginRenderAttempt {
    pub job_id: RequestId,
    pub attempt_id: AttemptId,
    pub cancellation_token: CancellationToken,
    pub checkpoint_attempt_id: Option<AttemptId>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredRenderCheckpoint {
    pub job_id: RequestId,
    pub encoding_attempt_id: AttemptId,
    pub media: RenderCandidateMedia,
}
#[derive(Debug, Clone)]
pub enum RenderAttemptTransition {
    Encoding,
    Verifying,
    RequestCancellation,
    /// Host confirms all owned work has stopped and been reaped/drained.
    FinishCancelled,
    /// Terminal host failure after owned work has stopped. Unresolved cleanup
    /// remains Cancelling; a diagnostic is not evidence of worker teardown.
    Failed(RenderDiagnostic),
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::RenderJob(message.into())
}
fn pure(error: render::RenderError) -> StoreError {
    invalid(error.to_string())
}
fn counter(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid("render counter exhausted or zero"))
}
fn encode<T: Serialize>(value: &T, limit: i64) -> Result<String, StoreError> {
    let json = serde_json::to_string(value)?;
    if json.len() > usize::try_from(limit).map_err(|_| invalid("invalid JSON bound"))? {
        return Err(invalid("render metadata exceeds JSON bound"));
    }
    Ok(json)
}
fn page(limit: u32) -> Result<(), StoreError> {
    if limit == 0 || limit > MAX_RENDER_PAGE {
        return Err(invalid("render page limit must be 1 through 256"));
    }
    Ok(())
}

impl ProjectStore {
    /// Capture one exact retained revision. Edits and undo never retarget it.
    pub fn create_render_job(
        &mut self,
        intent: RenderIntent,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<RenderIntent, StoreError> {
        self.require_writer()?;
        intent.validate().map_err(pure)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_runtime(&transaction)?;
        validate_intent_document(&transaction, &intent, cancelled, deadline)?;
        let count: i64 =
            transaction.query_row("SELECT COUNT(*) FROM render_jobs", [], |row| row.get(0))?;
        if count >= MAX_RENDER_JOBS {
            return Err(invalid("render job capacity reached"));
        }
        if transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM render_jobs WHERE job_id=?1)",
            [intent.job_id.as_str()],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(invalid("render job identity was already used"));
        }
        transaction.execute(
            "INSERT INTO render_jobs(job_id,revision_id,intent) VALUES(?1,?2,?3)",
            params![
                intent.job_id.as_str(),
                intent.revision_id.as_str(),
                encode(&intent, MAX_JOB_BYTES)?
            ],
        )?;
        transaction.commit()?;
        Ok(intent)
    }

    pub fn render_job(&self, id: &RequestId) -> Result<RenderIntent, StoreError> {
        validate_runtime(&self.connection)?;
        read_job(&self.connection, id)
    }
    pub fn render_jobs(
        &self,
        after: Option<&RequestId>,
        limit: u32,
    ) -> Result<Vec<RenderIntent>, StoreError> {
        page(limit)?;
        validate_runtime(&self.connection)?;
        let mut statement=self.connection.prepare("SELECT CASE WHEN typeof(job_id)='text' AND length(CAST(job_id AS BLOB)) BETWEEN 1 AND 128 THEN job_id END FROM render_jobs WHERE (?1 IS NULL OR job_id>?1) ORDER BY job_id LIMIT ?2")?;
        let mut rows = statement.query(params![after.map(RequestId::as_str), limit])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let id = required_text(row.get(0)?)?;
            result.push(read_job(
                &self.connection,
                &RequestId::new(id).map_err(|e| invalid(e.to_string()))?,
            )?);
        }
        Ok(result)
    }
    pub fn render_attempt(
        &self,
        job: &RequestId,
        attempt: &AttemptId,
    ) -> Result<StoredRenderAttempt, StoreError> {
        validate_runtime(&self.connection)?;
        read_job(&self.connection, job)?;
        read_attempt(&self.connection, job, attempt)
    }
    pub fn render_attempts(
        &self,
        job: &RequestId,
        after_ordinal: u64,
        limit: u32,
    ) -> Result<Vec<StoredRenderAttempt>, StoreError> {
        page(limit)?;
        validate_runtime(&self.connection)?;
        read_job(&self.connection, job)?;
        let after =
            i64::try_from(after_ordinal).map_err(|_| invalid("invalid render page cursor"))?;
        let mut statement=self.connection.prepare("SELECT CASE WHEN typeof(attempt_id)='text' AND length(CAST(attempt_id AS BLOB)) BETWEEN 1 AND 128 THEN attempt_id END FROM render_attempts WHERE job_id=?1 AND ordinal>?2 ORDER BY ordinal LIMIT ?3")?;
        let mut rows = statement.query(params![job.as_str(), after, limit])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let id = required_text(row.get(0)?)?;
            result.push(read_attempt(
                &self.connection,
                job,
                &AttemptId::new(id).map_err(|e| invalid(e.to_string()))?,
            )?);
        }
        Ok(result)
    }
    pub fn render_checkpoint(
        &self,
        job: &RequestId,
        attempt: &AttemptId,
    ) -> Result<StoredRenderCheckpoint, StoreError> {
        validate_runtime(&self.connection)?;
        read_job(&self.connection, job)?;
        read_checkpoint(&self.connection, job, attempt)
    }

    /// Explicit retry always allocates fresh identities. A checkpoint selects
    /// verification of its original bytes; it cannot select a different job.
    pub fn begin_render_attempt(
        &mut self,
        input: BeginRenderAttempt,
    ) -> Result<StoredRenderAttempt, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_runtime(&transaction)?;
        read_job(&transaction, &input.job_id)?;
        let active: i64 = transaction.query_row(
            &format!("SELECT COUNT(*) FROM render_attempts WHERE state IN ({ACTIVE})"),
            [],
            |row| row.get(0),
        )?;
        if active != 0 {
            return Err(invalid("project already has an active render attempt"));
        }
        let count: i64 =
            transaction.query_row("SELECT COUNT(*) FROM render_attempts", [], |row| row.get(0))?;
        if count >= MAX_RENDER_ATTEMPTS {
            return Err(invalid("render attempt capacity reached"));
        }
        let reused:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM render_attempts WHERE attempt_id=?1 OR cancellation_token=?2)",params![input.attempt_id.as_str(),input.cancellation_token.as_str()],|row|row.get(0))?;
        if reused {
            return Err(invalid(
                "render attempt identity or cancellation token was already used",
            ));
        }
        if let Some(checkpoint) = &input.checkpoint_attempt_id {
            read_checkpoint(&transaction, &input.job_id, checkpoint)?;
        }
        let high_water: Option<i64> = transaction
            .query_row(
                "SELECT high_water FROM render_job_heads WHERE job_id=?1",
                [input.job_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let ordinal = high_water
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("render attempt ordinal exhausted"))?;
        let attempt = StoredRenderAttempt {
            job_id: input.job_id,
            attempt_id: input.attempt_id,
            cancellation_token: input.cancellation_token,
            ordinal: u64::try_from(ordinal).map_err(|_| invalid("invalid ordinal"))?,
            transition_sequence: 1,
            state: RenderAttemptState::Queued,
            checkpoint_attempt_id: input.checkpoint_attempt_id,
            cancellation_requested: false,
            diagnostic: None,
            verification: None,
        };
        transaction.execute("INSERT INTO render_attempts(job_id,attempt_id,ordinal,cancellation_token,state,transition_sequence,body) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![attempt.job_id.as_str(),attempt.attempt_id.as_str(),ordinal,attempt.cancellation_token.as_str(),attempt.state.as_str(),1,encode(&attempt,MAX_ATTEMPT_BYTES)?])?;
        transaction.execute("INSERT INTO render_job_heads(job_id,high_water,latest_attempt_id) VALUES(?1,?2,?3) ON CONFLICT(job_id) DO UPDATE SET high_water=excluded.high_water,latest_attempt_id=excluded.latest_attempt_id",params![attempt.job_id.as_str(),ordinal,attempt.attempt_id.as_str()])?;
        transaction.commit()?;
        Ok(attempt)
    }

    pub fn transition_render_attempt(
        &mut self,
        identity: &RenderAttemptIdentity,
        transition: RenderAttemptTransition,
    ) -> Result<StoredRenderAttempt, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_runtime(&transaction)?;
        let mut attempt = bound_attempt(&transaction, identity)?;
        let next = match transition {
            RenderAttemptTransition::Encoding => {
                if read_job(&transaction, &identity.job_id)?
                    .policy
                    .is_automatic()
                {
                    return Err(invalid(
                        "automatic encoding requires an atomic admission decision",
                    ));
                }
                RenderAttemptState::Encoding
            }
            RenderAttemptTransition::Verifying => RenderAttemptState::Verifying,
            RenderAttemptTransition::RequestCancellation => RenderAttemptState::Cancelling,
            RenderAttemptTransition::FinishCancelled => RenderAttemptState::Cancelled,
            RenderAttemptTransition::Failed(diagnostic) => {
                diagnostic.validate().map_err(pure)?;
                attempt.diagnostic = Some(diagnostic);
                RenderAttemptState::Failed
            }
        };
        advance(&mut attempt, next)?;
        if next == RenderAttemptState::Cancelling {
            attempt.cancellation_requested = true;
        }
        write_attempt(&transaction, &attempt)?;
        transaction.commit()?;
        Ok(attempt)
    }

    /// Admission only checks prepared session/descriptor guards here. Full
    /// hashing and durable object retention already ran on the I/O worker.
    pub fn retain_render_checkpoint(
        &mut self,
        identity: &RenderAttemptIdentity,
        prepared: &PreparedRenderRetention,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<StoredRenderAttempt, StoreError> {
        self.require_writer()?;
        if prepared.identity() != identity {
            return Err(invalid(
                "render retention belongs to another attempt transition",
            ));
        }
        let storage = self.render_storage.clone();
        let closed = self.render_closed.clone();
        prepared.validate_for(&storage, &closed, cancelled, deadline)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_runtime(&transaction)?;
        let mut attempt = bound_attempt(&transaction, identity)?;
        if attempt.state != RenderAttemptState::Encoding || attempt.checkpoint_attempt_id.is_some()
        {
            return Err(invalid(
                "only a fresh encoding attempt can retain a checkpoint",
            ));
        }
        attempt.checkpoint_attempt_id = Some(attempt.attempt_id.clone());
        advance(&mut attempt, RenderAttemptState::EncodedRetained)?;
        transaction.execute(
            "INSERT INTO render_candidate_checkpoints(job_id,attempt_id,media) VALUES(?1,?2,?3)",
            params![
                attempt.job_id.as_str(),
                attempt.attempt_id.as_str(),
                encode(prepared.media(), MAX_CHECKPOINT_BYTES)?
            ],
        )?;
        write_attempt(&transaction, &attempt)?;
        prepared.validate_for(&storage, &closed, cancelled, deadline)?;
        transaction.commit()?;
        Ok(attempt)
    }

    /// Persist a fresh verifier's observation. This returns metadata only.
    pub fn record_render_verification(
        &mut self,
        identity: &RenderAttemptIdentity,
        observation: RenderVerificationObservation,
    ) -> Result<StoredRenderAttempt, StoreError> {
        self.require_writer()?;
        observation.validate().map_err(pure)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_runtime(&transaction)?;
        let mut attempt = bound_attempt(&transaction, identity)?;
        let checkpoint = read_checkpoint(
            &transaction,
            &attempt.job_id,
            attempt
                .checkpoint_attempt_id
                .as_ref()
                .ok_or_else(|| invalid("verification has no checkpoint"))?,
        )?;
        check_observation(&observation, &checkpoint.media)?;
        advance(&mut attempt, RenderAttemptState::Verified)?;
        attempt.verification = Some(observation);
        write_attempt(&transaction, &attempt)?;
        transaction.commit()?;
        Ok(attempt)
    }
}

fn validate_intent_document(
    connection: &Connection,
    intent: &RenderIntent,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), StoreError> {
    intent.validate().map_err(pure)?;
    let document = validation::read_revision(connection, intent.revision_id.as_str())?.document;
    if document.project_id() != &intent.project_id
        || intent.range.end().0 > document.duration()?.frames()
        || render::document_sha256(&document, cancelled, deadline).map_err(pure)?
            != intent.document_sha256
    {
        return Err(invalid(
            "render intent does not match its immutable document",
        ));
    }
    Ok(())
}
pub(crate) fn read_job(
    connection: &Connection,
    id: &RequestId,
) -> Result<RenderIntent, StoreError> {
    let row: Option<(Option<String>, Option<String>)> = connection
        .query_row(
            "SELECT CASE WHEN typeof(revision_id)='text' AND length(CAST(revision_id AS BLOB)) BETWEEN 1 AND 128 THEN revision_id END,
                CASE WHEN typeof(intent)='text' AND length(CAST(intent AS BLOB)) BETWEEN 1 AND ?2 THEN intent END
             FROM render_jobs WHERE job_id=?1",
            params![id.as_str(), MAX_JOB_BYTES],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (revision, json) = row.ok_or_else(|| invalid("render job not found"))?;
    let revision = required_text(revision)?;
    let json = required_text(json)?;
    let intent: RenderIntent = serde_json::from_str(&json)?;
    intent.validate().map_err(pure)?;
    if &intent.job_id != id || intent.revision_id.as_str() != revision {
        return Err(invalid("render intent row identity mismatch"));
    }
    let retained: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM revisions WHERE id=?1)",
        [intent.revision_id.as_str()],
        |row| row.get(0),
    )?;
    if !retained {
        return Err(invalid("render intent revision is missing"));
    }
    validate_job_head(connection, id)?;
    Ok(intent)
}

fn required_text(value: Option<String>) -> Result<String, StoreError> {
    value.ok_or_else(|| invalid("targeted render metadata is oversized or has wrong type"))
}

/// Only the selected job's scalar allocation history is needed by ordinary
/// operations. No attempt bodies or historical reports are inspected here.
fn validate_job_head(connection: &Connection, job: &RequestId) -> Result<(), StoreError> {
    #[cfg(test)]
    audit_tests::HEAD_READS.with(|count| count.set(count.get() + 1));
    let invalid_head: bool = connection.query_row(
        "SELECT
          (SELECT COUNT(*) FROM render_job_heads WHERE job_id=?1)>1 OR
          (SELECT COUNT(*) FROM render_attempts WHERE job_id=?1)!=(SELECT COUNT(DISTINCT ordinal) FROM render_attempts WHERE job_id=?1) OR
          ((SELECT COUNT(*) FROM render_attempts WHERE job_id=?1)>0 AND NOT EXISTS(SELECT 1 FROM render_job_heads WHERE job_id=?1)) OR
          EXISTS(SELECT 1 FROM render_job_heads h WHERE h.job_id=?1 AND
            (typeof(h.high_water)!='integer' OR h.high_water<1 OR
             h.high_water!=(SELECT COUNT(*) FROM render_attempts a WHERE a.job_id=?1) OR
             h.high_water!=(SELECT MAX(a.ordinal) FROM render_attempts a WHERE a.job_id=?1) OR
             NOT EXISTS(SELECT 1 FROM render_attempts a WHERE a.job_id=?1 AND a.attempt_id=h.latest_attempt_id AND a.ordinal=h.high_water)))",
        [job.as_str()], |row| row.get(0))?;
    if invalid_head {
        return Err(invalid(
            "selected render job allocation head is inconsistent",
        ));
    }
    Ok(())
}

pub(crate) fn read_attempt(
    connection: &Connection,
    job: &RequestId,
    id: &AttemptId,
) -> Result<StoredRenderAttempt, StoreError> {
    let attempt = read_attempt_body(connection, job, id)?;
    validate_attempt(connection, &attempt)?;
    Ok(attempt)
}

fn read_attempt_body(
    connection: &Connection,
    job: &RequestId,
    id: &AttemptId,
) -> Result<StoredRenderAttempt, StoreError> {
    type AttemptRow = (i64, Option<String>, Option<String>, i64, Option<String>);
    let row:Option<AttemptRow>=connection.query_row(
        "SELECT ordinal,
            CASE WHEN typeof(cancellation_token)='text' AND length(CAST(cancellation_token AS BLOB)) BETWEEN 1 AND 128 THEN cancellation_token END,
            CASE WHEN typeof(state)='text' AND length(CAST(state AS BLOB)) BETWEEN 1 AND 16 THEN state END,
            transition_sequence,
            CASE WHEN typeof(body)='text' AND length(CAST(body AS BLOB)) BETWEEN 1 AND ?3 THEN body END
         FROM render_attempts WHERE job_id=?1 AND attempt_id=?2",
        params![job.as_str(),id.as_str(),MAX_ATTEMPT_BYTES],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
    let (ordinal, token, state, sequence, json) =
        row.ok_or_else(|| invalid("render attempt not found"))?;
    let token = required_text(token)?;
    let state = required_text(state)?;
    let json = required_text(json)?;
    let attempt: StoredRenderAttempt = serde_json::from_str(&json)?;
    if &attempt.job_id != job
        || &attempt.attempt_id != id
        || counter(attempt.ordinal)? != ordinal
        || attempt.cancellation_token.as_str() != token
        || attempt.state.as_str() != state
        || counter(attempt.transition_sequence)? != sequence
    {
        return Err(invalid("render attempt row identity mismatch"));
    }
    Ok(attempt)
}
pub(crate) fn read_checkpoint(
    connection: &Connection,
    job: &RequestId,
    id: &AttemptId,
) -> Result<StoredRenderCheckpoint, StoreError> {
    let intent = read_job(connection, job)?;
    read_checkpoint_for_intent(connection, &intent, id)
}

fn read_checkpoint_for_intent(
    connection: &Connection,
    intent: &RenderIntent,
    id: &AttemptId,
) -> Result<StoredRenderCheckpoint, StoreError> {
    let job = &intent.job_id;
    let owner = read_attempt_body(connection, job, id)?;
    if owner.checkpoint_attempt_id.as_ref() != Some(id)
        || matches!(
            owner.state,
            RenderAttemptState::Queued | RenderAttemptState::Encoding
        )
    {
        return Err(invalid("render checkpoint has no encoding owner"));
    }
    validate_attempt_for_intent(connection, &owner, intent)?;
    read_checkpoint_media(connection, job, id)
}

fn read_checkpoint_media(
    connection: &Connection,
    job: &RequestId,
    id: &AttemptId,
) -> Result<StoredRenderCheckpoint, StoreError> {
    let json: Option<Option<String>> = connection
        .query_row(
            "SELECT CASE WHEN typeof(media)='text' AND length(CAST(media AS BLOB)) BETWEEN 1 AND ?3 THEN media END FROM render_candidate_checkpoints WHERE job_id=?1 AND attempt_id=?2",
            params![job.as_str(), id.as_str(), MAX_CHECKPOINT_BYTES],
            |row| row.get(0),
        )
        .optional()?;
    let json = required_text(json.ok_or_else(|| invalid("render checkpoint not found"))?)?;
    let media = serde_json::from_str(&json)?;
    Ok(StoredRenderCheckpoint {
        job_id: job.clone(),
        encoding_attempt_id: id.clone(),
        media,
    })
}
fn check_observation(
    observation: &RenderVerificationObservation,
    media: &RenderCandidateMedia,
) -> Result<(), StoreError> {
    observation.validate().map_err(pure)?;
    if &observation.movie_sha256 != media.movie_sha256()
        || observation.movie_byte_length != media.movie().byte_length()
    {
        return Err(invalid(
            "verification movie identity differs from checkpoint",
        ));
    }
    Ok(())
}
fn validate_attempt(
    connection: &Connection,
    attempt: &StoredRenderAttempt,
) -> Result<(), StoreError> {
    let intent = read_job(connection, &attempt.job_id)?;
    validate_attempt_for_intent(connection, attempt, &intent)
}

/// The full audit reuses a job whose row and allocation head it already
/// validated. Targeted reads obtain that same evidence through read_job first.
fn validate_attempt_for_intent(
    connection: &Connection,
    attempt: &StoredRenderAttempt,
    intent: &RenderIntent,
) -> Result<(), StoreError> {
    use RenderAttemptState::*;
    if attempt.job_id != intent.job_id {
        return Err(invalid("render attempt belongs to a different job"));
    }
    admission::validate_attempt_decision(connection, attempt, intent)?;
    if !attempt.state.is_terminal() && attempt.transition_sequence == MAX_RENDER_COUNTER {
        return Err(invalid(
            "active render attempt has exhausted its recovery sequence",
        ));
    }
    if (attempt.state == Queued) != (attempt.transition_sequence == 1)
        || (attempt.state == Verified) != attempt.verification.is_some()
        || matches!(attempt.state, Failed | Interrupted) != attempt.diagnostic.is_some()
        || matches!(attempt.state, Cancelling | Cancelled) && !attempt.cancellation_requested
        || attempt.cancellation_requested
            && !matches!(attempt.state, Cancelling | Cancelled | Interrupted | Failed)
    {
        return Err(invalid("render attempt state and evidence disagree"));
    }
    if let Some(diagnostic) = &attempt.diagnostic {
        diagnostic.validate().map_err(pure)?;
    }
    let checkpoint = attempt
        .checkpoint_attempt_id
        .as_ref()
        .map(|id| {
            if id == &attempt.attempt_id {
                read_checkpoint_media(connection, &attempt.job_id, id)
            } else {
                read_checkpoint_for_intent(connection, intent, id)
            }
        })
        .transpose()?;
    if matches!(attempt.state, EncodedRetained | Verifying | Verified) && checkpoint.is_none()
        || attempt.state == Encoding && checkpoint.is_some()
        || attempt.state == Queued
            && attempt.checkpoint_attempt_id.as_ref() == Some(&attempt.attempt_id)
    {
        return Err(invalid("render phase and checkpoint disagree"));
    }
    if let Some(id) = &attempt.checkpoint_attempt_id {
        if attempt.state == EncodedRetained && id != &attempt.attempt_id {
            return Err(invalid("encoded checkpoint belongs to another attempt"));
        }
        let source_ordinal: i64 = connection.query_row(
            "SELECT ordinal FROM render_attempts WHERE job_id=?1 AND attempt_id=?2",
            params![attempt.job_id.as_str(), id.as_str()],
            |row| row.get(0),
        )?;
        if source_ordinal > counter(attempt.ordinal)?
            || source_ordinal == counter(attempt.ordinal)? && id != &attempt.attempt_id
        {
            return Err(invalid("checkpoint does not precede its retry"));
        }
    }
    if let Some(observation) = &attempt.verification {
        check_observation(
            observation,
            &checkpoint
                .ok_or_else(|| invalid("verified attempt has no checkpoint"))?
                .media,
        )?;
    }
    Ok(())
}
fn bound_attempt(
    connection: &Connection,
    identity: &RenderAttemptIdentity,
) -> Result<StoredRenderAttempt, StoreError> {
    read_job(connection, &identity.job_id)?;
    let attempt = read_attempt(connection, &identity.job_id, &identity.attempt_id)?;
    if attempt.identity() != *identity {
        return Err(invalid(
            "stale render transition sequence or cancellation token",
        ));
    }
    Ok(attempt)
}
fn advance(attempt: &mut StoredRenderAttempt, next: RenderAttemptState) -> Result<(), StoreError> {
    render::validate_transition(attempt.state, next, attempt.checkpoint_attempt_id.is_some())
        .map_err(pure)?;
    if attempt.transition_sequence >= MAX_RENDER_COUNTER {
        return Err(invalid("render transition sequence exhausted"));
    }
    if !next.is_terminal() && attempt.transition_sequence == MAX_RENDER_COUNTER - 1 {
        return Err(invalid(
            "render transition must retain one sequence for terminal recovery",
        ));
    }
    attempt.transition_sequence += 1;
    attempt.state = next;
    Ok(())
}
fn write_attempt(connection: &Connection, attempt: &StoredRenderAttempt) -> Result<(), StoreError> {
    validate_attempt(connection, attempt)?;
    let changed=connection.execute("UPDATE render_attempts SET state=?1,transition_sequence=?2,body=?3 WHERE job_id=?4 AND attempt_id=?5",params![attempt.state.as_str(),counter(attempt.transition_sequence)?,encode(attempt,MAX_ATTEMPT_BYTES)?,attempt.job_id.as_str(),attempt.attempt_id.as_str()])?;
    if changed != 1 {
        return Err(invalid(
            "render attempt update affected an unexpected row count",
        ));
    }
    Ok(())
}

/// Bounds before JSON parsing or integrity CHECK evaluation.
pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    check_table_capacities(connection)?;
    for (table, column, bound) in [
        ("render_jobs", "intent", MAX_JOB_BYTES),
        ("render_attempts", "body", MAX_ATTEMPT_BYTES),
        (
            "render_candidate_checkpoints",
            "media",
            MAX_CHECKPOINT_BYTES,
        ),
        (
            "render_encoding_decisions",
            "body",
            admission::MAX_DECISION_BYTES,
        ),
    ] {
        let bad:bool=connection.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE typeof({column})!='text' OR length(CAST({column} AS BLOB)) NOT BETWEEN 1 AND ?1)"),[bound],|row|row.get(0))?;
        if bad {
            return Err(invalid("render JSON is oversized or has wrong type"));
        }
    }
    for (table, columns) in [
        ("render_jobs", &["job_id", "revision_id"][..]),
        (
            "render_attempts",
            &["job_id", "attempt_id", "cancellation_token"][..],
        ),
        ("render_job_heads", &["job_id", "latest_attempt_id"][..]),
        (
            "render_candidate_checkpoints",
            &["job_id", "attempt_id"][..],
        ),
        ("render_encoding_decisions", &["job_id", "attempt_id"][..]),
    ] {
        for column in columns {
            let bad:bool=connection.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE typeof({column})!='text' OR length(CAST({column} AS BLOB)) NOT BETWEEN 1 AND 128)"),[],|row|row.get(0))?;
            if bad {
                return Err(invalid("render identifier is oversized or has wrong type"));
            }
        }
    }
    let bad:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM render_attempts WHERE typeof(ordinal)!='integer' OR ordinal<1 OR typeof(transition_sequence)!='integer' OR transition_sequence<1 OR typeof(state)!='text' OR length(CAST(state AS BLOB)) NOT BETWEEN 1 AND 16) OR EXISTS(SELECT 1 FROM render_job_heads WHERE typeof(high_water)!='integer' OR high_water<1)",[],|row|row.get(0))?;
    if bad {
        return Err(invalid("render counter/state has invalid bounds or type"));
    }
    Ok(())
}

fn check_table_capacities(connection: &Connection) -> Result<(), StoreError> {
    for (table, limit) in [
        ("render_jobs", MAX_RENDER_JOBS),
        ("render_job_heads", MAX_RENDER_JOBS),
        ("render_attempts", MAX_RENDER_ATTEMPTS),
        ("render_candidate_checkpoints", MAX_RENDER_ATTEMPTS),
        ("render_encoding_decisions", MAX_RENDER_ATTEMPTS),
    ] {
        let count: i64 =
            connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })?;
        if count > limit {
            return Err(invalid("render metadata table capacity exceeded"));
        }
    }
    Ok(())
}

/// Ordinary operations inspect only indexes, scalar capacities and the active
/// set. Selected rows are separately bounded and validated before use.
fn validate_runtime(connection: &Connection) -> Result<(), StoreError> {
    check_table_capacities(connection)?;
    for (name, expected) in [
        ("one_active_render_attempt", ACTIVE_INDEX),
        ("unique_render_cancellation_token", TOKEN_INDEX),
    ] {
        let actual: Option<String> = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='index' AND name=?1",
                [name],
                |row| row.get(0),
            )
            .optional()?;
        if actual.as_deref() != Some(expected) {
            return Err(invalid("render uniqueness index missing or altered"));
        }
    }
    let active: i64 = connection.query_row(
        &format!("SELECT COUNT(*) FROM render_attempts WHERE state IN ({ACTIVE})"),
        [],
        |row| row.get(0),
    )?;
    if active > 1 {
        return Err(invalid("multiple active render attempts"));
    }
    Ok(())
}

fn validate_metadata(
    connection: &Connection,
) -> Result<BTreeMap<RequestId, RenderIntent>, StoreError> {
    check_stored_sizes(connection)?;
    validate_runtime(connection)?;
    for query in [
        "SELECT EXISTS(SELECT 1 FROM render_jobs GROUP BY job_id HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_attempts GROUP BY attempt_id HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_attempts GROUP BY cancellation_token HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_attempts GROUP BY job_id,ordinal HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_job_heads GROUP BY job_id HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_candidate_checkpoints GROUP BY attempt_id HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_encoding_decisions GROUP BY attempt_id HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_encoding_decisions d WHERE NOT EXISTS(SELECT 1 FROM render_attempts a WHERE a.job_id=d.job_id AND a.attempt_id=d.attempt_id))",
        "SELECT EXISTS(SELECT 1 FROM render_jobs j WHERE NOT EXISTS(SELECT 1 FROM revisions r WHERE r.id=j.revision_id))",
        "SELECT EXISTS(SELECT 1 FROM render_attempts a WHERE NOT EXISTS(SELECT 1 FROM render_jobs j WHERE j.job_id=a.job_id))",
        "SELECT EXISTS(SELECT 1 FROM render_attempts a WHERE NOT EXISTS(SELECT 1 FROM render_job_heads h WHERE h.job_id=a.job_id))",
        "SELECT EXISTS(SELECT 1 FROM render_job_heads h WHERE NOT EXISTS(SELECT 1 FROM render_jobs j WHERE j.job_id=h.job_id) OR h.high_water!=(SELECT MAX(a.ordinal) FROM render_attempts a WHERE a.job_id=h.job_id) OR h.high_water!=(SELECT COUNT(*) FROM render_attempts a WHERE a.job_id=h.job_id) OR NOT EXISTS(SELECT 1 FROM render_attempts a WHERE a.job_id=h.job_id AND a.attempt_id=h.latest_attempt_id AND a.ordinal=h.high_water))",
        "SELECT EXISTS(SELECT 1 FROM render_candidate_checkpoints c WHERE NOT EXISTS(SELECT 1 FROM render_attempts a WHERE a.job_id=c.job_id AND a.attempt_id=c.attempt_id AND json_extract(a.body,'$.checkpoint_attempt_id')=c.attempt_id AND a.state NOT IN ('queued','encoding')))",
    ] {
        if connection.query_row(query, [], |row| row.get::<_, bool>(0))? {
            return Err(invalid(
                "render metadata identity, head, or checkpoint inconsistency",
            ));
        }
    }
    let mut intents = BTreeMap::new();
    let mut jobs = connection.prepare("SELECT job_id FROM render_jobs")?;
    let mut rows = jobs.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let id = RequestId::new(id).map_err(|e| invalid(e.to_string()))?;
        intents.insert(id.clone(), read_job(connection, &id)?);
    }
    let mut attempts = connection.prepare("SELECT job_id,attempt_id FROM render_attempts")?;
    let mut rows = attempts.query([])?;
    while let Some(row) = rows.next()? {
        let job: String = row.get(0)?;
        let id: String = row.get(1)?;
        let job = RequestId::new(job).map_err(|e| invalid(e.to_string()))?;
        let intent = intents
            .get(&job)
            .ok_or_else(|| invalid("render attempt job is missing"))?;
        let attempt = read_attempt_body(
            connection,
            &job,
            &AttemptId::new(id).map_err(|e| invalid(e.to_string()))?,
        )?;
        validate_attempt_for_intent(connection, &attempt, intent)?;
    }
    Ok(intents)
}

/// Compact immutable revision facts; never retain the full historical documents.
struct AuditRevision {
    project_id: deadpan_core::ProjectId,
    duration_frames: i64,
    document_sha256: deadpan_jobs::Sha256,
    basis: deadpan_core::PresentationBasis,
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    let intents = validate_metadata(connection)?;
    let mut documents = BTreeMap::new();
    for intent in intents.values() {
        let binding = match documents.entry(intent.revision_id.clone()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let document =
                    validation::read_revision(connection, intent.revision_id.as_str())?.document;
                entry.insert(AuditRevision {
                    project_id: document.project_id().clone(),
                    duration_frames: document.duration()?.frames(),
                    document_sha256: render::document_sha256_for_validation(&document)
                        .map_err(pure)?,
                    basis: document.presentation_basis().clone(),
                })
            }
        };
        if intent.project_id != binding.project_id
            || intent.range.end().0 > binding.duration_frames
            || intent.document_sha256 != binding.document_sha256
        {
            return Err(invalid(
                "render intent does not match its immutable document",
            ));
        }
    }
    admission::validate_all_outputs(connection, &intents, &documents)
}

pub(crate) fn recover_nonterminal(
    connection: &mut Connection,
) -> Result<Vec<crate::recovery::InterruptedRender>, StoreError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut interrupted = Vec::new();
    // Open performs the full audit before entering recovery. Re-read only the
    // bounded active set here, avoiding another pass over historical reports.
    validate_runtime(&transaction)?;
    let identities = {
        let mut statement = transaction.prepare(&format!(
            "SELECT job_id,attempt_id FROM render_attempts WHERE state IN ({ACTIVE})"
        ))?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (job, id) in identities {
        let mut attempt = read_attempt(
            &transaction,
            &RequestId::new(job).map_err(|e| invalid(e.to_string()))?,
            &AttemptId::new(id).map_err(|e| invalid(e.to_string()))?,
        )?;
        advance(&mut attempt, RenderAttemptState::Interrupted)?;
        attempt.diagnostic=Some(RenderDiagnostic{code:"InterruptedOnOpen".into(),detail:"Writable reopen interrupted abandoned render work; retry explicitly. Retained checkpoints remain available.".into()});
        write_attempt(&transaction, &attempt)?;
        interrupted.push(crate::recovery::InterruptedRender {
            job_id: attempt.job_id.as_str().into(),
            attempt_id: attempt.attempt_id.as_str().into(),
            ordinal: attempt.ordinal,
            checkpoint_attempt_id: attempt
                .checkpoint_attempt_id
                .as_ref()
                .map(|id| id.as_str().into()),
        });
    }
    transaction.commit()?;
    Ok(interrupted)
}
