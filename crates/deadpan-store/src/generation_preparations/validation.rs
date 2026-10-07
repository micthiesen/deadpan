use super::*;
use deadpan_core::{ProjectDocument, ValidatedScopedIsolation};
use deadpan_jobs::HoldConstraints;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(crate) fn digest(connection: &Connection) -> Result<crate::audit::Chain, StoreError> {
    let mut hash = Sha256::new();
    hash.update(b"deadpan-generation-preparations-1");
    for (record, history) in all(connection)? {
        let bytes = serde_json::to_vec(&record)?;
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
        hash.update(history.to_le_bytes());
        digest_dependencies(
            connection,
            &mut hash,
            &record.controls,
            record.request_id.as_ref(),
        )?;
    }
    hash.update([0]);
    let mut statement = connection.prepare("SELECT record,id,origin_revision,history_id FROM generation_preparation_retirements ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let record = retention::parse(row)?;
        let bytes = serde_json::to_vec(&record)?;
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
        digest_dependencies(
            connection,
            &mut hash,
            &record.controls,
            record.request_id.as_ref(),
        )?;
    }
    Ok(hash.finalize().into())
}

fn digest_dependencies(
    connection: &Connection,
    hash: &mut Sha256,
    controls: &PreparationControls,
    fulfilled: Option<&RequestId>,
) -> Result<(), StoreError> {
    let source = match controls {
        PreparationControls::Request { request_id, .. } => Some(request_id),
        PreparationControls::AcceptedArtifact => None,
    };
    for request in [source, fulfilled] {
        if let Some(request) = request {
            hash.update([1]);
            let bytes = request_constraints(connection, request)?;
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes.as_bytes());
            let attempts: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM generation_attempts WHERE request_id=?1)",
                [request.as_str()],
                |row| row.get(0),
            )?;
            hash.update([u8::from(attempts)]);
        } else {
            hash.update([0]);
        }
    }
    Ok(())
}

fn request_constraints(connection: &Connection, request: &RequestId) -> Result<String, StoreError> {
    let source: Option<Option<String>> = connection.query_row("SELECT CASE WHEN typeof(constraints)='text' AND length(CAST(constraints AS BLOB))<=?2 THEN constraints END FROM generation_requests WHERE request_id=?1",
        params![request.as_str(), MAX_ROW_BYTES as i64], |row| row.get(0)).optional()?;
    source.flatten().ok_or_else(|| {
        invalid("preparation request is missing or its constraints exceed their bounds")
    })
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    let mut rows = all(connection)?;
    if rows.is_empty() {
        return Ok(());
    }
    let head = crate::read_snapshot(connection)?;
    let mut active = std::collections::BTreeSet::new();
    rows.sort_unstable_by(|(a, _), (b, _)| a.origin_revision.cmp(&b.origin_revision));
    let mut origin: Option<ProjectDocument> = None;
    for (value, _) in rows {
        if value.project_id != *head.project_id()
            || value.current_revision != *head.revision_id()
            || value.duration <= value.accepted.sampling.output_frame_count()
        {
            return Err(invalid(
                "preparation project, revision or replacement duration differs",
            ));
        }
        validate_controls(connection, &value.controls)?;
        if value.state.is_active() {
            if origin
                .as_ref()
                .is_none_or(|document| document.revision_id() != &value.origin_revision)
            {
                origin = Some(
                    crate::validation::read_revision(connection, value.origin_revision.as_str())?
                        .document,
                );
            }
            if !transitions::same_hold(origin.as_ref().expect("origin loaded"), &head, &value)
                || !active.insert(value.target.clone())
            {
                return Err(invalid(
                    "active preparation has no exact fallback target or is duplicated",
                ));
            }
        }
        validate_fulfilment(connection, value.request_id.as_ref(), value.duration)?;
    }
    Ok(())
}

fn validate_controls(
    connection: &Connection,
    controls: &PreparationControls,
) -> Result<(), StoreError> {
    if let PreparationControls::Request {
        request_id,
        options,
    } = controls
    {
        let constraints: HoldConstraints =
            serde_json::from_str(&request_constraints(connection, request_id)?)?;
        if &GenerationOptions::from_constraints(&constraints) != options {
            return Err(invalid("captured controls differ from their request"));
        }
    }
    Ok(())
}

fn validate_fulfilment(
    connection: &Connection,
    request: Option<&RequestId>,
    duration: FrameDuration,
) -> Result<(), StoreError> {
    if let Some(request) = request {
        let constraints: HoldConstraints =
            serde_json::from_str(&request_constraints(connection, request)?)?;
        if constraints.video.frames() != duration {
            return Err(invalid(
                "fulfilled preparation duration differs from its request",
            ));
        }
        let attempts: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM generation_attempts WHERE request_id=?1)",
            [request.as_str()],
            |row| row.get(0),
        )?;
        if !attempts {
            return Err(invalid("fulfilled preparation has no attempt"));
        }
    }
    Ok(())
}

/// Walk every chronological transition once. Indexed retirements are visited
/// at their birth only, so memory depends on live rows, not lifetime edits.
#[derive(Default)]
pub(crate) struct Replay {
    targets: BTreeMap<PreparationId, ScopedNodeTarget>,
    retired: u64,
}
impl Replay {
    pub(crate) fn map(&mut self, proof: &ValidatedScopedIsolation<'_>, forward: bool) {
        for target in self.targets.values_mut() {
            *target = if forward {
                proof.map_retained_forward(target)
            } else {
                proof.map_retained_backward(target)
            };
        }
    }

    pub(crate) fn arrive(
        &mut self,
        connection: &Connection,
        document: &ProjectDocument,
        history: i64,
        births: Vec<transitions::Birth>,
        scopes: &crate::generation_scope::Replay,
    ) -> Result<(), StoreError> {
        let count: i64 = connection.query_row(
            "SELECT
            (SELECT count(*) FROM generation_preparations WHERE origin_revision=?1) +
            (SELECT count(*) FROM generation_preparation_retirements WHERE origin_revision=?1)",
            [document.revision_id().as_str()],
            |row| row.get(0),
        )?;
        if usize::try_from(count).ok() != Some(births.len()) {
            return Err(invalid(
                "replacement preparation birth is missing or unexpected",
            ));
        }
        for birth in births {
            let id = transitions::id_for(document.revision_id(), &birth.target)?;
            let controls = if let Some(value) = read(connection, &id)? {
                let recorded_history: i64 = connection.query_row(
                    "SELECT history_id FROM generation_preparations WHERE id=?1",
                    [id.as_str()],
                    |row| row.get(0),
                )?;
                if recorded_history != history
                    || value.project_id != *document.project_id()
                    || value.origin_target != birth.target
                    || value.accepted != birth.accepted
                    || value.duration != birth.duration
                {
                    return Err(invalid(
                        "preparation differs from its validated duration command",
                    ));
                }
                if self.targets.insert(id, birth.target.clone()).is_some() {
                    return Err(invalid("preparation birth identity is reused"));
                }
                value.controls
            } else if let Some(value) = retention::read(connection, &id)? {
                if value.history != history || value.origin_revision != *document.revision_id() {
                    return Err(invalid(
                        "retired preparation differs from its duration command",
                    ));
                }
                validate_controls(connection, &value.controls)?;
                validate_fulfilment(connection, value.request_id.as_ref(), birth.duration)?;
                self.retired = self
                    .retired
                    .checked_add(1)
                    .ok_or_else(|| invalid("retirement count overflow"))?;
                value.controls
            } else {
                return Err(invalid(
                    "replacement preparation identity differs from the command",
                ));
            };
            if let PreparationControls::Request { request_id, .. } = &controls
                && !scopes.request_has_target(connection, request_id, &birth.target)?
            {
                return Err(invalid(
                    "preparation controls came from another authoring scope",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn finish(self, connection: &Connection) -> Result<(), StoreError> {
        let actual = all(connection)?;
        let retired: i64 = connection.query_row(
            "SELECT count(*) FROM generation_preparation_retirements",
            [],
            |row| row.get(0),
        )?;
        if actual.len() != self.targets.len() || u64::try_from(retired).ok() != Some(self.retired) {
            return Err(invalid(
                "preparation is not bound to an authored duration change",
            ));
        }
        for (record, _) in actual {
            if self.targets.get(&record.id) != Some(&record.target) {
                return Err(invalid(
                    "preparation address differs from proven isolation history",
                ));
            }
        }
        Ok(())
    }
}
