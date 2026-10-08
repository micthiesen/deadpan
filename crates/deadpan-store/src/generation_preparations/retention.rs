//! Terminal preparations are operational records. Their immutable birth and
//! controls proof survives compaction; historical documents retain the media.

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Retirement {
    pub id: PreparationId,
    pub origin_revision: RevisionId,
    pub history: i64,
    pub origin: PreparationOrigin,
    pub project_id: ProjectId,
    pub origin_target: ScopedNodeTarget,
    pub duration: FrameDuration,
    pub intent: crate::generation_intents::IntentBirthReceipt,
    pub state: PreparationState,
    pub request_id: Option<RequestId>,
}

impl Retirement {
    pub(super) fn birth(&self) -> crate::generation_intents::IntentBirth {
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

pub(super) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch("CREATE TABLE generation_preparation_retirements (
        id TEXT PRIMARY KEY,
        origin_revision TEXT NOT NULL REFERENCES revisions(id),
        history_id INTEGER NOT NULL REFERENCES history(id),
        record TEXT NOT NULL CHECK(json_valid(record))
    ) STRICT;
    CREATE INDEX generation_preparation_retirements_origin ON generation_preparation_retirements(origin_revision);")?;
    Ok(())
}

pub(super) fn check_sizes(connection: &Connection) -> Result<(), StoreError> {
    let bad: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM generation_preparation_retirements
        WHERE typeof(record)!='text' OR length(CAST(record AS BLOB))>?1
        OR typeof(id)!='text' OR length(CAST(id AS BLOB)) NOT BETWEEN 1 AND ?2
        OR typeof(origin_revision)!='text' OR length(CAST(origin_revision AS BLOB)) NOT BETWEEN 1 AND ?2
        OR typeof(history_id)!='integer' OR history_id<1)",
        params![MAX_ROW_BYTES as i64, deadpan_core::MAX_IDENTITY_BYTES as i64], |row| row.get(0))?;
    if bad {
        return Err(invalid("retired preparation exceeds its row bounds"));
    }
    Ok(())
}

pub(super) fn parse(row: &rusqlite::Row<'_>) -> Result<Retirement, StoreError> {
    let text: String = row.get(0)?;
    if text.len() > MAX_ROW_BYTES {
        return Err(invalid("retired preparation exceeds its byte bound"));
    }
    let value: Retirement = serde_json::from_str(&text)?;
    if value.id.as_str() != row.get::<_, String>(1)?
        || value.origin_revision.as_str() != row.get::<_, String>(2)?
        || value.history != row.get::<_, i64>(3)?
        || value.history < 1
        || value.state.is_active()
        || (value.state == PreparationState::Fulfilled) != value.request_id.is_some()
        || serde_json::to_string(&value)? != text
    {
        return Err(invalid("retirement columns or terminal state differ"));
    }
    value.birth().validate()?;
    if value.intent.history_id != value.history {
        return Err(invalid("retired birth history differs"));
    }
    Ok(value)
}

pub(super) fn read(
    connection: &Connection,
    id: &PreparationId,
) -> Result<Option<Retirement>, StoreError> {
    let mut statement = connection.prepare("SELECT record,id,origin_revision,history_id FROM generation_preparation_retirements WHERE id=?1")?;
    let mut rows = statement.query([id.as_str()])?;
    rows.next()?.map(parse).transpose()
}

fn retire(
    connection: &Connection,
    value: StoredGenerationPreparation,
    history: i64,
) -> Result<(), StoreError> {
    if value.state.is_active() {
        return Err(invalid("cannot retire pending preparation"));
    }
    let record = Retirement {
        id: value.id,
        origin_revision: value.origin_revision,
        history,
        origin: value.origin,
        project_id: value.project_id,
        origin_target: value.origin_target,
        duration: value.duration,
        intent: value.intent,
        state: value.state,
        request_id: value.request_id,
    };
    connection.execute("INSERT INTO generation_preparation_retirements(id,origin_revision,history_id,record) VALUES (?1,?2,?3,?4)",
        params![record.id.as_str(), record.origin_revision.as_str(), record.history, serde_json::to_string(&record)?])?;
    connection.execute(
        "DELETE FROM generation_preparations WHERE id=?1",
        [record.id.as_str()],
    )?;
    Ok(())
}

fn oldest(
    connection: &Connection,
    terminals_only: bool,
) -> Result<Option<(StoredGenerationPreparation, i64)>, StoreError> {
    let mut statement = connection.prepare("SELECT p.record,p.id,p.origin_revision,p.current_revision,p.state,p.history_id,p.charged_bytes
        FROM generation_preparations p JOIN revisions r ON r.id=p.origin_revision
        WHERE (?1=0 OR p.state IN ('fulfilled','cancelled'))
        ORDER BY CASE p.state WHEN 'fulfilled' THEN 0 WHEN 'cancelled' THEN 0 WHEN 'claimed' THEN 2 ELSE 1 END,r.rowid,p.id LIMIT 1")?;
    let mut rows = statement.query([terminals_only])?;
    rows.next()?.map(parse_row).transpose()
}

pub(super) fn compact(connection: &Connection) -> Result<(), StoreError> {
    compact_to(connection, MAX_TERMINAL_PREPARATIONS)
}

pub(super) fn compact_to(connection: &Connection, retain: usize) -> Result<(), StoreError> {
    loop {
        let count: i64 = connection.query_row(
            "SELECT count(*) FROM generation_preparations WHERE state IN ('fulfilled','cancelled')",
            [],
            |row| row.get(0),
        )?;
        if count <= retain as i64 {
            break;
        }
        let (value, history) =
            oldest(connection, true)?.ok_or_else(|| invalid("terminal count differs"))?;
        retire(connection, value, history)?;
    }
    Ok(())
}

/// Prefer old terminals, then waiting/retryable work, then claimed work. This
/// function runs inside the edit transaction and never rejects its timing
/// merely because earlier background work filled the queue.
pub(super) fn make_room(
    connection: &Connection,
    incoming_bytes: usize,
    displaced: &mut Vec<PreparationId>,
    total: &mut u64,
) -> Result<(), StoreError> {
    if incoming_bytes > MAX_ROW_BYTES {
        return Err(invalid("preparation exceeds its byte limit"));
    }
    compact(connection)?;
    loop {
        let (count, active, bytes): (i64,i64,i64) = connection.query_row("SELECT count(*),
            coalesce(sum(state NOT IN ('fulfilled','cancelled')),0),coalesce(sum(charged_bytes),0) FROM generation_preparations",
            [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
        if count < (MAX_PREPARATIONS + MAX_TERMINAL_PREPARATIONS) as i64
            && active < MAX_PREPARATIONS as i64
            && bytes + incoming_bytes as i64 <= MAX_TOTAL_BYTES as i64
        {
            break;
        }
        let (mut value, history) = oldest(connection, false)?
            .ok_or_else(|| invalid("preparation cannot fit its byte budget"))?;
        if value.state.is_active() {
            value.state = PreparationState::Cancelled;
            value.reason = Some(
                "The bounded replacement queue filled; the committed fallback remains unchanged."
                    .into(),
            );
            advance(&mut value)?;
            if displaced.len() < MAX_PREPARATION_PAGE {
                displaced.push(value.id.clone());
            }
            *total = total
                .checked_add(1)
                .ok_or_else(|| invalid("capacity notice count overflow"))?;
        }
        retire(connection, value, history)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{
        AssetId, BridgeInterpolation, BridgeSamplingMap, FrameRate, GeneratedContentId,
        GeneratedObjectRef, NodeId,
    };

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn database() -> Result<Connection, StoreError> {
        let mut db = Connection::open_in_memory()?;
        crate::schema::configure(&db)?;
        crate::schema::create(&mut db)?;
        db.execute_batch("INSERT INTO revisions(id,kind,document,depth,json_bound) VALUES ('r','initial','{}',0,2);
            INSERT INTO history(id,revision_id,request,edit) VALUES (1,'r','{}','{}');")?;
        Ok(db)
    }

    fn record(index: usize, state: PreparationState) -> StoredGenerationPreparation {
        let object = |digit: char| {
            GeneratedObjectRef::new(
                GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
                1024,
            )
            .unwrap()
        };
        let rate = FrameRate::new(30, 1).unwrap();
        let mut value = StoredGenerationPreparation {
            id: PreparationId::new(format!("preparation-{index:05}")).unwrap(),
            project_id: ProjectId::new("project").unwrap(),
            origin_revision: RevisionId::new("r").unwrap(),
            origin_target: ScopedNodeTarget {
                node: NodeId::new(format!("hold-{index:05}")).unwrap(),
                repeats: Vec::new(),
            },
            current_revision: RevisionId::new("r").unwrap(),
            target: ScopedNodeTarget {
                node: NodeId::new(format!("hold-{index:05}")).unwrap(),
                repeats: Vec::new(),
            },
            duration: FrameDuration::new(18).unwrap(),
            origin: PreparationOrigin::AcceptedExtension {
                accepted: Box::new(GeneratedArtifact {
                    sampled_asset: AssetId::new("sampled").unwrap(),
                    sampled_object: object('a'),
                    native_asset: AssetId::new("native").unwrap(),
                    native_object: object('b'),
                    provenance: object('c'),
                    sampling: BridgeSamplingMap::new(
                        rate,
                        rate,
                        FrameDuration::new(12).unwrap(),
                        FrameDuration::new(12).unwrap(),
                        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
                    )
                    .unwrap(),
                    content_aspect: None,
                }),
                controls: PreparationControls::AcceptedArtifact,
            },
            intent: crate::generation_intents::IntentBirthReceipt {
                schema_version: 1,
                history_id: 1,
                cause: crate::generation_intents::IntentCause::InsertedPause,
                authorization: crate::generation_intents::IntentAuthorization::AuthoredOrigin,
                fallback: deadpan_core::HoldFallback::Background,
                input_binding: crate::generation_intents::IntentInputBinding::Unavailable {
                    cause: crate::generation_intents::InputUnavailableCause::MissingQualification,
                    detail: "Synthetic queue capacity fixture.".into(),
                },
            },
            state,
            claim_sequence: 1,
            reason: matches!(state, PreparationState::Cancelled)
                .then(|| "Cancelled fixture".into()),
            request_id: None,
        };
        if index.is_multiple_of(2) {
            value.origin = PreparationOrigin::inserted_pause();
        }
        value
    }

    fn insert(db: &Connection, value: &StoredGenerationPreparation) -> Result<(), StoreError> {
        db.execute("INSERT INTO generation_preparations(id,origin_revision,current_revision,history_id,state,record,charged_bytes) VALUES (?1,'r','r',1,?2,?3,?4)",
            params![value.id.as_str(), value.state.name(), serde_json::to_string(value)?, charged_bytes(value, serde_json::to_vec(value)?.len())? as i64])?;
        Ok(())
    }

    #[test]
    fn capacity_retires_waiting_work_before_claimed_and_emits_bounded_truthful_notice() -> TestResult
    {
        let db = database()?;
        let mut displaced = Vec::new();
        let mut total = 0;
        let mut index = 0;
        // Reach the byte budget through real admission. It includes future
        // mutable growth and may be exhausted before the row-count ceiling.
        while total == 0 {
            let value = record(
                index,
                if index == 0 {
                    PreparationState::Claimed
                } else {
                    PreparationState::Queued
                },
            );
            make_room(
                &db,
                charged_bytes(&value, serde_json::to_vec(&value)?.len())?,
                &mut displaced,
                &mut total,
            )?;
            insert(&db, &value)?;
            index += 1;
        }
        let capacity = index - 1;
        assert!(capacity < MAX_PREPARATIONS);
        for index in index..index + MAX_PREPARATION_PAGE + 1 {
            let value = record(index, PreparationState::Queued);
            make_room(
                &db,
                charged_bytes(&value, serde_json::to_vec(&value)?.len())?,
                &mut displaced,
                &mut total,
            )?;
            insert(&db, &value)?;
        }
        assert_eq!(total, (MAX_PREPARATION_PAGE + 2) as u64);
        assert_eq!(displaced.len(), MAX_PREPARATION_PAGE);
        assert_eq!(displaced[0], record(1, PreparationState::Queued).id);
        assert_eq!(
            super::super::read(&db, &record(0, PreparationState::Claimed).id)?
                .unwrap()
                .state,
            PreparationState::Claimed
        );
        let retired = read(&db, &displaced[0])?.unwrap();
        assert_eq!(retired.state, PreparationState::Cancelled);
        assert_eq!(retired.origin, record(1, PreparationState::Queued).origin);
        assert_eq!(
            db.query_row("SELECT count(*) FROM generation_preparations", [], |row| {
                row.get::<_, i64>(0)
            })?,
            capacity as i64
        );
        // A completely full queue can still record the longest escaped error,
        // longest revision and isolated node identity, largest claim sequence
        // and a fulfilment request. The reserved charge never grows.
        let revision = "r".repeat(deadpan_core::MAX_IDENTITY_BYTES);
        db.execute("INSERT INTO revisions(id,kind,document,depth,json_bound) VALUES (?1,'initial','{}',0,2)", [&revision])?;
        for (mut value, _) in all(&db)? {
            let charge = charged_bytes(&value, serde_json::to_vec(&value)?.len())?;
            value.state = PreparationState::Unavailable;
            value.reason = Some("\u{1}".repeat(MAX_REASON_BYTES));
            value.claim_sequence = i64::MAX as u64;
            value.current_revision = RevisionId::new(revision.clone())?;
            value.target.node = NodeId::new("n".repeat(deadpan_core::MAX_IDENTITY_BYTES))?;
            save(&db, &value)?;
            assert_eq!(
                charged_bytes(&value, serde_json::to_vec(&value)?.len())?,
                charge
            );
            let reloaded = super::super::read(&db, &value.id)?.unwrap();
            assert_eq!(reloaded, value);
            value.state = PreparationState::Fulfilled;
            value.reason = None;
            value.request_id = Some(RequestId::new(
                "q".repeat(deadpan_jobs::MAX_PROTOCOL_ID_BYTES),
            )?);
            save(&db, &value)?;
            assert_eq!(
                charged_bytes(&value, serde_json::to_vec(&value)?.len())?,
                charge
            );
            assert_eq!(super::super::read(&db, &value.id)?.unwrap(), value);
        }
        check_stored_sizes(&db)?;
        Ok(())
    }

    #[test]
    fn terminal_compaction_preserves_immutable_birth_controls_without_lifetime_queue_cost()
    -> TestResult {
        let db = database()?;
        let first = record(0, PreparationState::Cancelled);
        for index in 0..MAX_TERMINAL_PREPARATIONS + 2 {
            insert(&db, &record(index, PreparationState::Cancelled))?;
        }
        compact(&db)?;
        assert!(super::super::read(&db, &first.id)?.is_none());
        let proof = read(&db, &first.id)?.unwrap();
        assert_eq!(proof.origin_revision, first.origin_revision);
        assert_eq!(proof.origin, first.origin);
        assert_eq!(proof.state, PreparationState::Cancelled);
        assert_eq!(
            db.query_row("SELECT count(*) FROM generation_preparations", [], |row| {
                row.get::<_, i64>(0)
            })?,
            MAX_TERMINAL_PREPARATIONS as i64
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM generation_preparation_retirements",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            2
        );
        check_stored_sizes(&db)?;
        Ok(())
    }
}
