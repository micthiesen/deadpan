//! Operational publication journal. SQL stores bounded declarations only;
//! destination files and movie hashing belong to the publication host.
use crate::{ProjectStore, StoreError, render_jobs::StoredRenderAttempt};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{MAX_RENDER_COUNTER, RenderAttemptState, RenderDiagnostic, publication::*},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use std::{
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub const MAX_PUBLICATIONS: i64 = 4096;
pub const MAX_PUBLICATION_OPERATIONS: i64 = 65536;
const MAX_RECORD: usize = 160 * 1024;
const MAX_OPERATION: usize = 8192;
const ACTIVE_INDEX: &str = "CREATE UNIQUE INDEX one_active_publication_operation ON render_publication_operations(publication_id) WHERE active=1";

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Publication(message.into())
}
fn pure(error: deadpan_jobs::render::RenderError) -> StoreError {
    invalid(error.to_string())
}
fn sql_bound(value: usize) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| invalid("publication SQL bound exceeds range"))
}
fn number(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid("invalid publication counter"))
}

/// A live, exact-stage acknowledgement. Serialized journal rows cannot recreate
/// this capability. Any committed transition invalidates the previous epoch.
pub struct PublicationPermit {
    record: StoredPublication,
    encoding_decision: Option<deadpan_jobs::render::admission::RenderEncodingDecision>,
    closed: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
}
impl PublicationPermit {
    pub fn record(&self) -> &StoredPublication {
        &self.record
    }
    /// Original encoder provenance captured with this exact publication stage.
    /// This observation does not restore a live automatic encoding capability.
    pub fn encoding_decision(
        &self,
    ) -> Option<&deadpan_jobs::render::admission::RenderEncodingDecision> {
        self.encoding_decision.as_ref()
    }
    pub fn identity(&self) -> PublicationIdentity {
        self.record.identity()
    }
    pub fn check_live(&self) -> Result<(), StoreError> {
        if self.closed.load(Ordering::Acquire)
            || self.epoch.load(Ordering::Acquire) != self.record.sequence
            || !self.record.operation.active
            || self.record.cancellation_requested
        {
            return Err(invalid(
                "publication permit is closed, cancelled, or superseded",
            ));
        }
        Ok(())
    }
}
pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch("CREATE TABLE render_publications (
        publication_id TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES render_jobs(job_id),
        sequence INTEGER NOT NULL CHECK(sequence BETWEEN 1 AND 9223372036854775807),
        body TEXT NOT NULL CHECK(json_valid(body))
    ) STRICT;
    CREATE TABLE render_publication_operations (
        operation_id TEXT PRIMARY KEY, publication_id TEXT NOT NULL REFERENCES render_publications(publication_id),
        ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 1 AND 9223372036854775807),
        cancellation_token TEXT NOT NULL UNIQUE, active INTEGER NOT NULL CHECK(active IN (0,1)),
        body TEXT NOT NULL CHECK(json_valid(body)), UNIQUE(publication_id,ordinal)
    ) STRICT;
    ")?;
    connection.execute_batch(ACTIVE_INDEX)?;
    Ok(())
}
impl ProjectStore {
    fn publication_writer(&mut self) -> Result<(), StoreError> {
        self.require_writer()?;
        if self.publication_barrier_failed {
            return Err(invalid(
                "publication durability failed; close and reopen the project before retrying",
            ));
        }
        if let Err(error) = self
            .publication_durability
            .as_mut()
            .ok_or_else(|| invalid("publication writer has no pinned database"))?
            .capture_wal()
        {
            self.publication_barrier_failed = true;
            for epoch in self.publication_epochs.values() {
                epoch.store(0, Ordering::Release);
            }
            return Err(error);
        }
        Ok(())
    }
    /// Caller supplies the destination and fresh identities. All retained movie
    /// claims are derived from the store's exact Verified render attempt.
    pub fn begin_render_publication(
        &mut self,
        intent: PublicationIntent,
        operation_id: AttemptId,
        cancellation_token: CancellationToken,
    ) -> Result<PublicationPermit, StoreError> {
        self.publication_writer()?;
        intent.validate().map_err(pure)?;
        let render_intent = self.render_job(&intent.job_id)?;
        let verified = verified(self, &intent.job_id, &intent.verified_attempt_id)?;
        let encoding_attempt_id = verified
            .checkpoint_attempt_id
            .clone()
            .ok_or_else(|| invalid("verified render lacks checkpoint"))?;
        let checkpoint = self.render_checkpoint(&intent.job_id, &encoding_attempt_id)?;
        let record = StoredPublication {
            operation: PublicationOperation {
                publication_id: intent.publication_id.clone(),
                operation_id,
                cancellation_token,
                ordinal: 1,
                kind: PublicationOperationKind::Publish,
                verified_attempt_id: intent.verified_attempt_id.clone(),
                active: true,
                outcome: PublicationOutcome::InProgress,
                diagnostic: None,
            },
            intent,
            render_intent,
            encoding_attempt_id,
            movie_sha256: checkpoint.media.movie_sha256().clone(),
            movie_bytes: checkpoint.media.movie().byte_length(),
            prepared: None,
            phase: PublicationPhase::Intent,
            outcome: PublicationOutcome::InProgress,
            sequence: 1,
            observed_movie_commit: false,
            cancellation_requested: false,
        };
        record.validate().map_err(pure)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        capacity(&transaction)?;
        if count(&transaction, "render_publications")? >= MAX_PUBLICATIONS {
            return Err(invalid("publication capacity reached"));
        }
        ensure_new_operation(&transaction, &record.operation)?;
        transaction.execute("INSERT INTO render_publications(publication_id,job_id,sequence,body) VALUES(?1,?2,?3,?4)",params![record.intent.publication_id.as_str(),record.intent.job_id.as_str(),number(record.sequence)?,encode(&record,MAX_RECORD)?])?;
        insert_operation(&transaction, &record.operation)?;
        transaction.commit()?;
        self.publication_ack(record)
    }
    pub fn record_prepared_publication(
        &mut self,
        identity: &PublicationIdentity,
        evidence: PreparedPublicationEvidence,
    ) -> Result<PublicationPermit, StoreError> {
        evidence.validate().map_err(pure)?;
        let mut record = self.publication_bound(identity, PublicationOperationKind::Publish)?;
        no_cancel(&record)?;
        if record.phase != PublicationPhase::Intent
            || evidence.movie_sha256 != record.movie_sha256
            || evidence.movie_bytes != record.movie_bytes
        {
            return Err(invalid(
                "prepared publication does not match captured checkpoint or phase",
            ));
        }
        record.prepared = Some(evidence);
        record.phase = PublicationPhase::Prepared;
        self.publication_commit(record)
    }
    pub fn advance_publication(
        &mut self,
        identity: &PublicationIdentity,
        next: PublicationPhase,
    ) -> Result<PublicationPermit, StoreError> {
        let mut record = self.publication_bound(identity, PublicationOperationKind::Publish)?;
        no_cancel(&record)?;
        if !matches!(
            (record.phase, next),
            (
                PublicationPhase::Prepared,
                PublicationPhase::ReportCommitting
            ) | (
                PublicationPhase::ReportCommitting,
                PublicationPhase::ReportCommitted
            ) | (
                PublicationPhase::ReportCommitted,
                PublicationPhase::MovieCommitting
            )
        ) {
            return Err(invalid("invalid publication phase transition"));
        }
        record.phase = next;
        self.publication_commit(record)
    }
    pub fn request_publication_cancellation(
        &mut self,
        identity: &PublicationIdentity,
    ) -> Result<StoredPublication, StoreError> {
        self.publication_writer()?;
        let mut record = bound(&self.connection, identity)?;
        if record.cancellation_requested {
            return Err(invalid("publication cancellation already requested"));
        }
        // Revoke immediately; cancellation racing an already-started rename
        // is still recorded through the observed-commit completion path.
        if let Some(epoch) = self
            .publication_epochs
            .get(record.intent.publication_id.as_str())
        {
            epoch.store(0, Ordering::Release);
        }
        record.cancellation_requested = true;
        Ok(self.publication_commit(record)?.record)
    }
    /// Host declares completion only after owned external work has stopped.
    /// A rename racing cancellation must retain its observed commit outcome.
    pub fn finish_publication(
        &mut self,
        identity: &PublicationIdentity,
        completion: PublicationCompletion,
    ) -> Result<StoredPublication, StoreError> {
        let mut record = self.publication_bound(identity, PublicationOperationKind::Publish)?;
        let (outcome, diagnostic) = match completion {
            PublicationCompletion::Failed(diagnostic) => {
                no_commit(&record)?;
                (PublicationOutcome::Failed, Some(diagnostic))
            }
            PublicationCompletion::Cancelled => {
                no_commit(&record)?;
                if !record.cancellation_requested {
                    return Err(invalid("cancellation was not requested"));
                }
                (PublicationOutcome::Cancelled, None)
            }
            PublicationCompletion::Published => {
                commit_phase(&record)?;
                record.observed_movie_commit = true;
                (PublicationOutcome::Published, None)
            }
            PublicationCompletion::PublishedUnconfirmed(diagnostic) => {
                commit_phase(&record)?;
                record.observed_movie_commit = true;
                (PublicationOutcome::PublishedUnconfirmed, Some(diagnostic))
            }
            PublicationCompletion::Unresolved(diagnostic) => (
                if record.observed_movie_commit {
                    PublicationOutcome::PublishedUnconfirmed
                } else {
                    PublicationOutcome::Unresolved
                },
                Some(diagnostic),
            ),
        };
        end(&mut record, outcome, diagnostic);
        Ok(self.publication_commit(record)?.record)
    }
    pub fn begin_publication_reconciliation(
        &mut self,
        id: &RequestId,
        verified_attempt_id: AttemptId,
        operation_id: AttemptId,
        cancellation_token: CancellationToken,
    ) -> Result<PublicationPermit, StoreError> {
        self.publication_writer()?;
        let mut record = read_record(&self.connection, id)?;
        if record.operation.active
            || !matches!(
                record.outcome,
                PublicationOutcome::Interrupted
                    | PublicationOutcome::Unresolved
                    | PublicationOutcome::PublishedUnconfirmed
                    | PublicationOutcome::Published
            )
        {
            return Err(invalid(
                "publication has no unresolved or committed outcome to reconcile",
            ));
        }
        let attempt = verified(self, &record.intent.job_id, &verified_attempt_id)?;
        let previous = verified(
            self,
            &record.intent.job_id,
            &record.operation.verified_attempt_id,
        )?;
        if attempt.ordinal <= previous.ordinal
            || attempt.checkpoint_attempt_id.as_ref() != Some(&record.encoding_attempt_id)
        {
            return Err(invalid(
                "reconciliation requires a fresh verification of the same checkpoint",
            ));
        }
        let checkpoint =
            self.render_checkpoint(&record.intent.job_id, &record.encoding_attempt_id)?;
        if checkpoint.media.movie_sha256() != &record.movie_sha256
            || checkpoint.media.movie().byte_length() != record.movie_bytes
        {
            return Err(invalid("reconciliation checkpoint changed"));
        }
        let ordinal = record
            .operation
            .ordinal
            .checked_add(1)
            .filter(|v| *v < MAX_RENDER_COUNTER)
            .ok_or_else(|| invalid("publication operation counter exhausted"))?;
        record.operation = PublicationOperation {
            publication_id: id.clone(),
            operation_id,
            cancellation_token,
            ordinal,
            kind: PublicationOperationKind::Reconcile,
            verified_attempt_id,
            active: true,
            outcome: PublicationOutcome::InProgress,
            diagnostic: None,
        };
        record.outcome = PublicationOutcome::InProgress;
        record.cancellation_requested = false;
        bump(&mut record)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_new_operation(&transaction, &record.operation)?;
        write_record(&transaction, &record)?;
        insert_operation(&transaction, &record.operation)?;
        transaction.commit()?;
        self.publication_ack(record)
    }
    pub fn finish_publication_reconciliation(
        &mut self,
        identity: &PublicationIdentity,
        result: PublicationReconciliation,
    ) -> Result<StoredPublication, StoreError> {
        let mut record = self.publication_bound(identity, PublicationOperationKind::Reconcile)?;
        let (outcome, diagnostic) = match result {
            PublicationReconciliation::Confirmed => {
                no_cancel(&record)?;
                commit_phase(&record)?;
                record.observed_movie_commit = true;
                (PublicationOutcome::Published, None)
            }
            PublicationReconciliation::CommittedUnconfirmed(diagnostic) => {
                commit_phase(&record)?;
                record.observed_movie_commit = true;
                (PublicationOutcome::PublishedUnconfirmed, Some(diagnostic))
            }
            PublicationReconciliation::NotPublished(diagnostic) => {
                no_commit(&record)?;
                if record.phase == PublicationPhase::MovieCommitting {
                    return Err(invalid(
                        "possible movie commit cannot be declared unpublished",
                    ));
                }
                (PublicationOutcome::Failed, Some(diagnostic))
            }
            PublicationReconciliation::Unresolved(diagnostic) => (
                if record.observed_movie_commit {
                    PublicationOutcome::PublishedUnconfirmed
                } else {
                    PublicationOutcome::Unresolved
                },
                Some(diagnostic),
            ),
        };
        end(&mut record, outcome, diagnostic);
        Ok(self.publication_commit(record)?.record)
    }
    pub fn render_publication(&self, id: &RequestId) -> Result<StoredPublication, StoreError> {
        read_record(&self.connection, id)
    }
    pub fn render_publications(
        &self,
        after: Option<&RequestId>,
        limit: u32,
    ) -> Result<Vec<StoredPublication>, StoreError> {
        page(limit)?;
        capacity(&self.connection)?;
        let mut statement=self.connection.prepare("SELECT CASE WHEN length(CAST(publication_id AS BLOB)) BETWEEN 1 AND 128 THEN publication_id END FROM render_publications WHERE (?1 IS NULL OR publication_id>?1) ORDER BY publication_id LIMIT ?2")?;
        let ids = statement
            .query_map(params![after.map(RequestId::as_str), limit], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                read_record(
                    &self.connection,
                    &RequestId::new(id).map_err(|e| invalid(e.to_string()))?,
                )
            })
            .collect()
    }
    pub fn publication_operations(
        &self,
        id: &RequestId,
        after_ordinal: u64,
        limit: u32,
    ) -> Result<Vec<PublicationOperation>, StoreError> {
        page(limit)?;
        read_record(&self.connection, id)?;
        let after = i64::try_from(after_ordinal)
            .map_err(|_| invalid("publication page cursor exceeds bound"))?;
        let mut statement=self.connection.prepare("SELECT CASE WHEN length(CAST(operation_id AS BLOB)) BETWEEN 1 AND 128 THEN operation_id END FROM render_publication_operations WHERE publication_id=?1 AND ordinal>?2 ORDER BY ordinal LIMIT ?3")?;
        let ids = statement
            .query_map(params![id.as_str(), after, limit], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                read_operation(
                    &self.connection,
                    &AttemptId::new(id).map_err(|e| invalid(e.to_string()))?,
                )
            })
            .collect()
    }
    fn publication_bound(
        &mut self,
        identity: &PublicationIdentity,
        kind: PublicationOperationKind,
    ) -> Result<StoredPublication, StoreError> {
        self.publication_writer()?;
        let record = bound(&self.connection, identity)?;
        if record.operation.kind != kind {
            return Err(invalid("publication operation kind differs"));
        }
        Ok(record)
    }
    fn publication_commit(
        &mut self,
        mut record: StoredPublication,
    ) -> Result<PublicationPermit, StoreError> {
        bump(&mut record)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        write_record(&transaction, &record)?;
        let changed = transaction.execute(
            "UPDATE render_publication_operations SET active=?1,body=?2 WHERE operation_id=?3",
            params![
                record.operation.active,
                encode(&record.operation, MAX_OPERATION)?,
                record.operation.operation_id.as_str()
            ],
        )?;
        if changed != 1 {
            return Err(invalid("publication operation update count differs"));
        }
        transaction.commit()?;
        self.publication_ack(record)
    }
    fn publication_ack(
        &mut self,
        record: StoredPublication,
    ) -> Result<PublicationPermit, StoreError> {
        let epoch = self
            .publication_epochs
            .entry(record.intent.publication_id.as_str().to_owned())
            .or_insert_with(|| Arc::new(AtomicU64::new(0)))
            .clone();
        // COMMIT already happened. Never leave a previous stage live if the
        // stronger barrier fails; reopening recovers the durable journal row.
        epoch.store(record.sequence, Ordering::Release);
        if let Err(error) = self
            .publication_durability
            .as_mut()
            .ok_or_else(|| invalid("publication writer has no pinned database"))?
            .barrier()
        {
            self.publication_barrier_failed = true;
            for epoch in self.publication_epochs.values() {
                epoch.store(0, Ordering::Release);
            }
            return Err(error);
        }
        let encoding_decision = match self
            .render_encoding_decision(&record.intent.job_id, &record.encoding_attempt_id)
        {
            Ok(decision) => decision,
            Err(error) => {
                epoch.store(0, Ordering::Release);
                return Err(error);
            }
        };
        Ok(PublicationPermit {
            record,
            encoding_decision,
            closed: self.render_closed.clone(),
            epoch,
        })
    }
}
fn verified(
    store: &ProjectStore,
    job: &RequestId,
    id: &AttemptId,
) -> Result<StoredRenderAttempt, StoreError> {
    let attempt = store.render_attempt(job, id)?;
    if attempt.state != RenderAttemptState::Verified || attempt.cancellation_requested {
        return Err(invalid(
            "publication requires the exact completed Verified attempt",
        ));
    }
    Ok(attempt)
}
fn no_cancel(record: &StoredPublication) -> Result<(), StoreError> {
    if record.cancellation_requested {
        Err(invalid("publication cancellation prevents this action"))
    } else {
        Ok(())
    }
}
fn no_commit(record: &StoredPublication) -> Result<(), StoreError> {
    if record.observed_movie_commit {
        Err(invalid("observed movie commit cannot be downgraded"))
    } else {
        Ok(())
    }
}
fn commit_phase(record: &StoredPublication) -> Result<(), StoreError> {
    if record.phase != PublicationPhase::MovieCommitting || record.prepared.is_none() {
        Err(invalid("publication has no movie commit intent"))
    } else {
        Ok(())
    }
}
fn end(
    record: &mut StoredPublication,
    outcome: PublicationOutcome,
    diagnostic: Option<RenderDiagnostic>,
) {
    record.outcome = outcome;
    record.operation.outcome = outcome;
    record.operation.active = false;
    record.operation.diagnostic = diagnostic;
}
fn bump(record: &mut StoredPublication) -> Result<(), StoreError> {
    record.sequence = record
        .sequence
        .checked_add(1)
        .filter(|v| {
            *v <= MAX_RENDER_COUNTER && (!record.operation.active || *v < MAX_RENDER_COUNTER)
        })
        .ok_or_else(|| invalid("publication sequence exhausted"))?;
    record.validate().map_err(pure)
}
fn page(limit: u32) -> Result<(), StoreError> {
    if !(1..=256).contains(&limit) {
        Err(invalid("publication page limit must be 1 through 256"))
    } else {
        Ok(())
    }
}
fn count(connection: &Connection, table: &str) -> Result<i64, StoreError> {
    Ok(
        connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })?,
    )
}
fn capacity(connection: &Connection) -> Result<(), StoreError> {
    let index: Option<String> = connection.query_row("SELECT sql FROM sqlite_schema WHERE type='index' AND name='one_active_publication_operation'",[],|row|row.get(0)).optional()?;
    if index.as_deref() != Some(ACTIVE_INDEX) {
        return Err(invalid(
            "publication active-operation index missing or altered",
        ));
    }
    if count(connection, "render_publications")? > MAX_PUBLICATIONS
        || count(connection, "render_publication_operations")? > MAX_PUBLICATION_OPERATIONS
    {
        return Err(invalid("publication inventory exceeds bounds"));
    }
    Ok(())
}
fn ensure_new_operation(
    connection: &Connection,
    operation: &PublicationOperation,
) -> Result<(), StoreError> {
    capacity(connection)?;
    if count(connection, "render_publication_operations")? >= MAX_PUBLICATION_OPERATIONS {
        return Err(invalid("publication operation capacity reached"));
    }
    let exists:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM render_publication_operations WHERE operation_id=?1 OR cancellation_token=?2)",params![operation.operation_id.as_str(),operation.cancellation_token.as_str()],|row|row.get(0))?;
    if exists {
        return Err(invalid(
            "publication operation identity or token was already used",
        ));
    }
    Ok(())
}
fn insert_operation(
    connection: &Connection,
    operation: &PublicationOperation,
) -> Result<(), StoreError> {
    connection.execute("INSERT INTO render_publication_operations(operation_id,publication_id,ordinal,cancellation_token,active,body) VALUES(?1,?2,?3,?4,?5,?6)",params![operation.operation_id.as_str(),operation.publication_id.as_str(),number(operation.ordinal)?,operation.cancellation_token.as_str(),operation.active,encode(operation,MAX_OPERATION)?])?;
    Ok(())
}
fn write_record(connection: &Connection, record: &StoredPublication) -> Result<(), StoreError> {
    record.validate().map_err(pure)?;
    if connection.execute(
        "UPDATE render_publications SET sequence=?1,body=?2 WHERE publication_id=?3",
        params![
            number(record.sequence)?,
            encode(record, MAX_RECORD)?,
            record.intent.publication_id.as_str()
        ],
    )? != 1
    {
        return Err(invalid("publication update count differs"));
    }
    Ok(())
}
fn bound(
    connection: &Connection,
    identity: &PublicationIdentity,
) -> Result<StoredPublication, StoreError> {
    let record = read_record(connection, &identity.publication_id)?;
    if record.identity() != *identity || !record.operation.active {
        return Err(invalid("publication operation or sequence is stale"));
    }
    Ok(record)
}
fn read_record(connection: &Connection, id: &RequestId) -> Result<StoredPublication, StoreError> {
    capacity(connection)?;
    let row:Option<(String,i64,Option<String>)>=connection.query_row("SELECT CASE WHEN length(CAST(job_id AS BLOB)) BETWEEN 1 AND 128 THEN job_id END,sequence,CASE WHEN typeof(body)='text' AND length(CAST(body AS BLOB)) BETWEEN 1 AND ?2 THEN body END FROM render_publications WHERE publication_id=?1",params![id.as_str(),sql_bound(MAX_RECORD)?],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    let (job, sequence, json) = row.ok_or_else(|| invalid("publication not found"))?;
    let record: StoredPublication =
        serde_json::from_str(&json.ok_or_else(|| invalid("publication row exceeds bound"))?)?;
    record.validate().map_err(pure)?;
    if record.intent.publication_id != *id
        || record.intent.job_id.as_str() != job
        || number(record.sequence)? != sequence
        || read_operation(connection, &record.operation.operation_id)? != record.operation
    {
        return Err(invalid("publication row identity differs"));
    }
    validate_binding(connection, &record)?;
    Ok(record)
}
fn validate_binding(connection: &Connection, record: &StoredPublication) -> Result<(), StoreError> {
    let job = crate::render_jobs::read_job(connection, &record.intent.job_id)?;
    if job != record.render_intent {
        return Err(invalid("publication render intent changed"));
    }
    let checkpoint = crate::render_jobs::read_checkpoint(
        connection,
        &record.intent.job_id,
        &record.encoding_attempt_id,
    )?;
    if checkpoint.media.movie_sha256() != &record.movie_sha256
        || checkpoint.media.movie().byte_length() != record.movie_bytes
    {
        return Err(invalid("publication retained movie changed"));
    }
    for id in [
        &record.intent.verified_attempt_id,
        &record.operation.verified_attempt_id,
    ] {
        let attempt = crate::render_jobs::read_attempt(connection, &record.intent.job_id, id)?;
        if attempt.state != RenderAttemptState::Verified
            || attempt.checkpoint_attempt_id.as_ref() != Some(&record.encoding_attempt_id)
        {
            return Err(invalid("publication verification binding changed"));
        }
    }
    Ok(())
}
fn read_operation(
    connection: &Connection,
    id: &AttemptId,
) -> Result<PublicationOperation, StoreError> {
    type Row = (String, i64, String, bool, Option<String>);
    let row:Option<Row>=connection.query_row("SELECT CASE WHEN length(CAST(publication_id AS BLOB)) BETWEEN 1 AND 128 THEN publication_id END,ordinal,CASE WHEN length(CAST(cancellation_token AS BLOB)) BETWEEN 1 AND 128 THEN cancellation_token END,active,CASE WHEN typeof(body)='text' AND length(CAST(body AS BLOB)) BETWEEN 1 AND ?2 THEN body END FROM render_publication_operations WHERE operation_id=?1",params![id.as_str(),sql_bound(MAX_OPERATION)?],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
    let (publication, ordinal, token, active, json) =
        row.ok_or_else(|| invalid("publication operation not found"))?;
    let operation: PublicationOperation =
        serde_json::from_str(&json.ok_or_else(|| invalid("publication operation exceeds bound"))?)?;
    if operation.operation_id != *id
        || operation.publication_id.as_str() != publication
        || number(operation.ordinal)? != ordinal
        || operation.cancellation_token.as_str() != token
        || operation.active != active
        || (operation.outcome == PublicationOutcome::InProgress) != operation.active
    {
        return Err(invalid("publication operation identity differs"));
    }
    if let Some(diagnostic) = &operation.diagnostic {
        diagnostic.validate().map_err(pure)?;
    }
    if matches!(
        operation.outcome,
        PublicationOutcome::Failed
            | PublicationOutcome::Interrupted
            | PublicationOutcome::Unresolved
            | PublicationOutcome::PublishedUnconfirmed
    ) != operation.diagnostic.is_some()
    {
        return Err(invalid("publication operation diagnostic disagrees"));
    }
    Ok(operation)
}
struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "publication JSON bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(value: &impl Serialize, limit: usize) -> Result<String, StoreError> {
    let mut writer = BoundedBytes {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value)?;
    String::from_utf8(writer.bytes).map_err(|_| invalid("publication serialization is not UTF-8"))
}
pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    capacity(connection)?;
    for (table, bound) in [
        ("render_publications", MAX_RECORD),
        ("render_publication_operations", MAX_OPERATION),
    ] {
        let bad:bool=connection.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE typeof(body)!='text' OR length(CAST(body AS BLOB)) NOT BETWEEN 1 AND ?1)"),[sql_bound(bound)?],|row|row.get(0))?;
        if bad {
            return Err(invalid("stored publication metadata exceeds bound"));
        }
    }
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    check_stored_sizes(connection)?;
    for query in [
        "SELECT EXISTS(SELECT 1 FROM render_publications GROUP BY publication_id HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_publication_operations GROUP BY operation_id HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_publication_operations GROUP BY cancellation_token HAVING COUNT(*)!=1)",
        "SELECT EXISTS(SELECT 1 FROM render_publication_operations GROUP BY publication_id,ordinal HAVING COUNT(*)!=1)",
    ] {
        if connection.query_row(query, [], |row| row.get::<_, bool>(0))? {
            return Err(invalid("publication identity uniqueness is inconsistent"));
        }
    }
    let ids = connection
        .prepare("SELECT publication_id FROM render_publications ORDER BY publication_id")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        let record = read_record(
            connection,
            &RequestId::new(id).map_err(|e| invalid(e.to_string()))?,
        )?;
        let job_json: String = connection.query_row(
            "SELECT intent FROM render_jobs WHERE job_id=?1",
            [record.intent.job_id.as_str()],
            |row| row.get(0),
        )?;
        if serde_json::from_str::<deadpan_jobs::render::RenderIntent>(&job_json)?
            != record.render_intent
        {
            return Err(invalid("publication captured render intent changed"));
        }
        let operations=connection.prepare("SELECT operation_id FROM render_publication_operations WHERE publication_id=?1 ORDER BY ordinal")?.query_map([record.intent.publication_id.as_str()],|row|row.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
        let mut previous_verified = 0;
        let mut last = None;
        let mut prior_outcome = None;
        let mut observed = false;
        for (index, id) in operations.into_iter().enumerate() {
            let operation = read_operation(
                connection,
                &AttemptId::new(id).map_err(|e| invalid(e.to_string()))?,
            )?;
            if operation.ordinal
                != u64::try_from(index).map_err(|_| invalid("operation index overflow"))? + 1
                || operation.ordinal == 1
                    && (operation.kind != PublicationOperationKind::Publish
                        || operation.verified_attempt_id != record.intent.verified_attempt_id)
                || operation.ordinal > 1 && operation.kind != PublicationOperationKind::Reconcile
                || operation.active && operation != record.operation
            {
                return Err(invalid("publication operation history differs"));
            }
            let json: String = connection.query_row(
                "SELECT body FROM render_attempts WHERE job_id=?1 AND attempt_id=?2",
                params![
                    record.intent.job_id.as_str(),
                    operation.verified_attempt_id.as_str()
                ],
                |row| row.get(0),
            )?;
            let attempt: StoredRenderAttempt = serde_json::from_str(&json)?;
            if attempt.state != RenderAttemptState::Verified
                || attempt.checkpoint_attempt_id.as_ref() != Some(&record.encoding_attempt_id)
                || attempt.ordinal <= previous_verified
                || attempt.verification.as_ref().is_none_or(|v| {
                    v.movie_sha256 != record.movie_sha256
                        || v.movie_byte_length != record.movie_bytes
                })
            {
                return Err(invalid(
                    "publication operation lacks matching historical verification",
                ));
            }
            if prior_outcome.is_some_and(|prior| {
                matches!(
                    prior,
                    PublicationOutcome::Failed
                        | PublicationOutcome::Cancelled
                        | PublicationOutcome::InProgress
                )
            }) {
                return Err(invalid(
                    "publication reconciliation follows a definite unpublished or live operation",
                ));
            }
            observed |= matches!(
                operation.outcome,
                PublicationOutcome::Published | PublicationOutcome::PublishedUnconfirmed
            );
            if observed && !record.observed_movie_commit {
                return Err(invalid("publication lost observed commit knowledge"));
            }
            prior_outcome = Some(operation.outcome);
            previous_verified = attempt.ordinal;
            last = Some(operation);
        }
        if last.as_ref() != Some(&record.operation) {
            return Err(invalid("publication head does not match latest operation"));
        }
    }
    Ok(())
}
pub(crate) fn recover_nonterminal(
    connection: &mut Connection,
) -> Result<Vec<crate::recovery::InterruptedPublication>, StoreError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut interrupted = Vec::new();
    let ids = transaction
        .prepare("SELECT publication_id FROM render_publication_operations WHERE active=1")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        let mut record = read_record(
            &transaction,
            &RequestId::new(id.clone()).map_err(|e| invalid(e.to_string()))?,
        )?;
        let outcome = if record.observed_movie_commit {
            PublicationOutcome::PublishedUnconfirmed
        } else {
            PublicationOutcome::Interrupted
        };
        end(&mut record,outcome,Some(RenderDiagnostic { code:"PublicationInterrupted".into(),detail:"Writer reopened before publication work reached a recorded terminal outcome; external files were not inspected or changed.".into() }));
        bump(&mut record)?;
        write_record(&transaction, &record)?;
        transaction.execute(
            "UPDATE render_publication_operations SET active=0,body=?1 WHERE operation_id=?2",
            params![
                encode(&record.operation, MAX_OPERATION)?,
                record.operation.operation_id.as_str()
            ],
        )?;
        interrupted.push(crate::recovery::InterruptedPublication {
            publication_id: id,
            movie_committed: record.observed_movie_commit,
        });
    }
    transaction.commit()?;
    Ok(interrupted)
}

#[cfg(test)]
mod tests;
