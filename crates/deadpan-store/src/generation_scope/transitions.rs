//! The index names the actual history command, never a caller-provided map.

use deadpan_core::{
    Command, CommandRequest, ProjectDocument, RevisionId, ValidatedScopedIsolation,
    derive_scoped_isolation,
};
use rusqlite::{Connection, OptionalExtension, params};

use super::{integrity, parse_id, read, target_json};
use crate::{StoreError, generation::StoredGenerationRequest, validation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Event {
    pub history_id: i64,
    pub forward: bool,
}

fn may_isolate(command: &Command) -> bool {
    match command.base_command() {
        Command::EditScoped { .. }
        | Command::EditScopedMany { .. }
        | Command::EditOccurrence { .. }
        | Command::KeepFirstPlayAttachments { .. } => true,
        Command::Compound { transaction } => transaction
            .steps()
            .iter()
            .filter_map(|step| step.edit())
            .any(|edit| may_isolate(edit.command.as_command())),
        _ => false,
    }
}

pub(crate) fn with_command_proof(
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
    apply: impl FnOnce(&ValidatedScopedIsolation<'_>) -> Result<(), StoreError>,
) -> Result<bool, StoreError> {
    if !may_isolate(&request.command) {
        return Ok(false);
    }
    let proof = derive_scoped_isolation(before, request, after)?;
    if proof.record().is_empty() {
        return Ok(false);
    }
    apply(&proof)?;
    Ok(true)
}

pub(crate) fn with_history_proof(
    connection: &Connection,
    entry: i64,
    apply: impl FnOnce(&ValidatedScopedIsolation<'_>) -> Result<(), StoreError>,
) -> Result<bool, StoreError> {
    let history = validation::read_history(connection, entry)?;
    if !may_isolate(&history.request.command) {
        return Ok(false);
    }
    // Undo has a new revision identity. The proof always binds the original
    // command's exact snapshots; only the indexed event uses the new identity.
    let before =
        validation::read_revision(connection, history.request.expected_revision.as_str())?.document;
    let after = history.edit.forward.apply_stored(&before)?;
    with_command_proof(&before, &history.request, &after, apply)
}

fn map_scopes(
    connection: &Connection,
    proof: &ValidatedScopedIsolation<'_>,
    forward: bool,
) -> Result<(), StoreError> {
    let mut statement =
        connection.prepare("SELECT scope_id FROM generation_scopes ORDER BY scope_id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id = parse_id(row.get(0)?)?;
        let scope = read(connection, &id)?;
        let mapped = if forward {
            proof.map_retained_forward(&scope.target)
        } else {
            proof.map_retained_backward(&scope.target)
        };
        if mapped != scope.target {
            connection.execute(
                "UPDATE generation_scopes SET current_target=?1 WHERE scope_id=?2",
                params![target_json(&mapped)?, id.as_str()],
            )?;
        }
    }
    super::validation::check_budget(connection)?;
    crate::generation_preparations::map(connection, proof, forward)?;
    Ok(())
}

pub(crate) fn command_transition(
    connection: &Connection,
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
) -> Result<bool, StoreError> {
    with_command_proof(before, request, after, |proof| {
        map_scopes(connection, proof, true)
    })
}

pub(crate) fn history_transition(
    connection: &Connection,
    entry: i64,
    forward: bool,
) -> Result<bool, StoreError> {
    with_history_proof(connection, entry, |proof| {
        map_scopes(connection, proof, forward)
    })
}

pub(crate) fn preview_command(
    connection: &Connection,
    before: &ProjectDocument,
    request: &CommandRequest,
    after: &ProjectDocument,
) -> Result<Vec<StoredGenerationRequest>, StoreError> {
    let mut requests = crate::generation::read_current_requests(connection)?;
    with_command_proof(before, request, after, |proof| {
        for request in &mut requests {
            request.target = proof.map_retained_forward(&request.target);
        }
        Ok(())
    })?;
    Ok(requests)
}

pub(crate) fn preview_history(
    connection: &Connection,
    entry: i64,
    forward: bool,
) -> Result<Vec<StoredGenerationRequest>, StoreError> {
    let mut requests = crate::generation::read_current_requests(connection)?;
    with_history_proof(connection, entry, |proof| {
        for request in &mut requests {
            request.target = if forward {
                proof.map_retained_forward(&request.target)
            } else {
                proof.map_retained_backward(&request.target)
            };
        }
        Ok(())
    })?;
    Ok(requests)
}

pub(crate) fn insert_event(
    connection: &Connection,
    revision: &RevisionId,
    history_id: i64,
    forward: bool,
) -> Result<(), StoreError> {
    connection.execute(
        "INSERT INTO generation_scope_events(revision_id,history_id,direction) VALUES (?1,?2,?3)",
        params![
            revision.as_str(),
            history_id,
            if forward { "forward" } else { "inverse" }
        ],
    )?;
    Ok(())
}

pub(crate) fn read_event(
    connection: &Connection,
    revision: &str,
) -> Result<Option<Event>, StoreError> {
    let value: Option<(Option<i64>, Option<String>)> = connection.query_row(
        "SELECT CASE WHEN typeof(history_id)='integer' AND history_id>0 THEN history_id END,
                CASE WHEN typeof(direction)='text' AND direction IN ('forward','inverse') THEN direction END
         FROM generation_scope_events WHERE revision_id=?1",
        [revision], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    match value {
        None => Ok(None),
        Some((Some(history_id), Some(direction))) => Ok(Some(Event {
            history_id,
            forward: direction == "forward",
        })),
        Some(_) => Err(integrity("invalid generation scope transition")),
    }
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let invalid: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM generation_scope_events WHERE
            typeof(revision_id)!='text' OR length(CAST(revision_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(history_id)!='integer' OR history_id<1 OR
            typeof(direction)!='text' OR direction NOT IN ('forward','inverse'))",
        [deadpan_core::MAX_IDENTITY_BYTES as i64],
        |row| row.get(0),
    )?;
    if invalid {
        return Err(integrity("invalid generation scope event metadata"));
    }
    Ok(())
}
