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
            &record.origin,
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
            &record.origin,
            record.request_id.as_ref(),
        )?;
    }
    Ok(hash.finalize().into())
}

fn digest_dependencies(
    connection: &Connection,
    hash: &mut Sha256,
    origin: &PreparationOrigin,
    fulfilled: Option<&RequestId>,
) -> Result<(), StoreError> {
    let source = origin.source_request();
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
    let mut budget = transitions::AddressBudget::default();
    let canonical = rows
        .iter()
        .any(|(value, _)| value.state.is_active())
        .then(|| transitions::CanonicalTargets::new(&head, &mut budget))
        .transpose()?;
    let mut active = std::collections::BTreeSet::new();
    rows.sort_unstable_by(|(a, _), (b, _)| a.origin_revision.cmp(&b.origin_revision));
    let mut origin: Option<ProjectDocument> = None;
    for (value, _) in rows {
        if value.project_id != *head.project_id()
            || value.current_revision != *head.revision_id()
            || value.duration == FrameDuration::ZERO
        {
            return Err(invalid(
                "preparation project, revision or replacement duration differs",
            ));
        }
        validate_origin(connection, &value.origin)?;
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
            if !transitions::same_hold(
                origin.as_ref().expect("origin loaded"),
                &head,
                &value,
                canonical.as_ref().expect("active ownership index"),
                &mut budget,
            )? || !has_current_intent(connection, &value)?
                || !active.insert(value.target.clone())
            {
                return Err(invalid(
                    "active preparation has no exact fallback target or is duplicated",
                ));
            }
        }
        validate_fulfilment(
            connection,
            value.request_id.as_ref(),
            value.duration,
            &value.origin,
        )?;
    }
    Ok(())
}

fn validate_origin(connection: &Connection, origin: &PreparationOrigin) -> Result<(), StoreError> {
    if let PreparationOrigin::AcceptedBoundary { accepted, controls } = origin {
        let receipt = crate::generation_origins::read(connection, accepted)?
            .ok_or_else(|| invalid("boundary origin has no retained accepted receipt"))?;
        if !matches!(controls, PreparationControls::Request { request_id, options }
            if request_id == receipt.request_id() && options == receipt.options())
        {
            return Err(invalid(
                "boundary controls differ from their accepted artifact origin",
            ));
        }
    }
    if let PreparationOrigin::InsertedPause { .. } = origin
        && origin != &PreparationOrigin::inserted_pause()
    {
        return Err(invalid(
            "inserted AI pause controls differ from its command",
        ));
    }
    if let PreparationOrigin::AcceptedExtension {
        controls:
            PreparationControls::Request {
                request_id,
                options,
            },
        ..
    }
    | PreparationOrigin::AcceptedBoundary {
        controls:
            PreparationControls::Request {
                request_id,
                options,
            },
        ..
    } = origin
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
    origin: &PreparationOrigin,
) -> Result<(), StoreError> {
    if let Some(request) = request {
        let constraints: HoldConstraints =
            serde_json::from_str(&request_constraints(connection, request)?)?;
        if constraints.video.frames() != duration {
            return Err(invalid(
                "fulfilled preparation duration differs from its request",
            ));
        }
        if origin
            .options()
            .is_some_and(|options| options != &GenerationOptions::from_constraints(&constraints))
        {
            return Err(invalid(
                "fulfilled preparation controls differ from its origin",
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
        let bindings = crate::generation_intents::capture_inputs(
            connection,
            document,
            &births
                .iter()
                .map(|birth| birth.target.clone())
                .collect::<Vec<_>>(),
        )?;
        for (birth, binding) in births.into_iter().zip(bindings) {
            let expected_fallback = match &birth.fallback {
                deadpan_core::HoldVideo::Background => deadpan_core::HoldFallback::Background,
                deadpan_core::HoldVideo::Freeze { asset, timestamp } => {
                    deadpan_core::HoldFallback::Freeze {
                        asset: asset.clone(),
                        timestamp: *timestamp,
                    }
                }
                _ => return Err(invalid("birth fallback is not deterministic")),
            };
            let id = transitions::id_for(document.revision_id(), &birth.target)?;
            let immutable = read_intent_birth(connection, &id)?
                .ok_or_else(|| invalid("immutable intent birth is absent"))?;
            if immutable.project_id != *document.project_id()
                || immutable.activation_revision != *document.revision_id()
                || immutable.origin_target != birth.target
                || immutable.duration != birth.duration
                || immutable.receipt.history_id != history
                || immutable.receipt.cause != birth.cause
                || immutable.receipt.authorization != birth.authorization
                || immutable.receipt.fallback != expected_fallback
                || !immutable.receipt.input_binding.same_authority(&binding)
            {
                return Err(invalid(
                    "immutable intent receipt differs from its command and measured inputs",
                ));
            }
            let origin = if let Some(value) = read(connection, &id)? {
                let recorded_history: i64 = connection.query_row(
                    "SELECT history_id FROM generation_preparations WHERE id=?1",
                    [id.as_str()],
                    |row| row.get(0),
                )?;
                if recorded_history != history
                    || value.project_id != *document.project_id()
                    || value.origin_target != birth.target
                    || !value.origin.same_birth(&birth.origin)
                    || value.duration != birth.duration
                {
                    return Err(invalid("preparation differs from its validated command"));
                }
                if self.targets.insert(id, birth.target.clone()).is_some() {
                    return Err(invalid("preparation birth identity is reused"));
                }
                value.origin
            } else if let Some(value) = retention::read(connection, &id)? {
                if value.history != history
                    || value.origin_revision != *document.revision_id()
                    || !value.origin.same_birth(&birth.origin)
                    || value.project_id != *document.project_id()
                    || value.origin_target != birth.target
                    || value.duration != birth.duration
                {
                    return Err(invalid("retired preparation differs from its command"));
                }
                validate_origin(connection, &value.origin)?;
                validate_fulfilment(
                    connection,
                    value.request_id.as_ref(),
                    birth.duration,
                    &value.origin,
                )?;
                self.retired = self
                    .retired
                    .checked_add(1)
                    .ok_or_else(|| invalid("retirement count overflow"))?;
                value.origin
            } else {
                return Err(invalid(
                    "replacement preparation identity differs from the command",
                ));
            };
            validate_origin(connection, &origin)?;
            if let PreparationOrigin::AcceptedExtension {
                controls: PreparationControls::Request { request_id, .. },
                ..
            } = &origin
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
