use deadpan_core::{EditTransaction, ProjectDocument, RevisionId};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    CommitOutcome, ProjectStore, StoreError, check_document_size, ensure_unused_revision,
    generation, insert_revision, read_snapshot, validation,
};

pub(crate) struct NavigationPlan {
    current: ProjectDocument,
    pub next: ProjectDocument,
    edit: EditTransaction,
    entry: i64,
    pub next_cursor: Option<i64>,
}

impl ProjectStore {
    /// Availability at the current durable cursor without building inverse edits.
    pub fn history_availability(&self) -> Result<(bool, bool), StoreError> {
        Ok(self.connection.query_row(
            "SELECT cursor IS NOT NULL, EXISTS(SELECT 1 FROM redo) FROM state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    pub fn undo(
        &mut self,
        expected_revision: &RevisionId,
        new_revision: RevisionId,
    ) -> Result<CommitOutcome, StoreError> {
        self.navigate_history(expected_revision, new_revision, false, None)
    }

    pub fn redo(
        &mut self,
        expected_revision: &RevisionId,
        new_revision: RevisionId,
    ) -> Result<CommitOutcome, StoreError> {
        self.navigate_history(expected_revision, new_revision, true, None)
    }

    pub fn undo_reconciled(
        &mut self,
        expected_revision: &RevisionId,
        new_revision: RevisionId,
        relevance: &generation::RelevancePlan,
    ) -> Result<CommitOutcome, StoreError> {
        self.navigate_history(expected_revision, new_revision, false, Some(relevance))
    }

    pub fn redo_reconciled(
        &mut self,
        expected_revision: &RevisionId,
        new_revision: RevisionId,
        relevance: &generation::RelevancePlan,
    ) -> Result<CommitOutcome, StoreError> {
        self.navigate_history(expected_revision, new_revision, true, Some(relevance))
    }

    pub fn preview_undo(
        &self,
        expected_revision: &RevisionId,
        new_revision: RevisionId,
    ) -> Result<CommitOutcome, StoreError> {
        self.preview_history(expected_revision, new_revision, false)
    }

    pub fn preview_redo(
        &self,
        expected_revision: &RevisionId,
        new_revision: RevisionId,
    ) -> Result<CommitOutcome, StoreError> {
        self.preview_history(expected_revision, new_revision, true)
    }

    fn preview_history(
        &self,
        expected: &RevisionId,
        next_revision: RevisionId,
        redo: bool,
    ) -> Result<CommitOutcome, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let plan = prepare_navigation(&transaction, expected, next_revision, redo)?;
        Ok(CommitOutcome {
            revision_id: plan.next.revision_id().clone(),
            edit: plan.edit,
        })
    }

    fn navigate_history(
        &mut self,
        expected: &RevisionId,
        next_revision: RevisionId,
        redo: bool,
        relevance: Option<&generation::RelevancePlan>,
    ) -> Result<CommitOutcome, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let plan = prepare_navigation(&transaction, expected, next_revision, redo)?;
        match relevance {
            Some(relevance) => generation::apply_relevance_plan(
                &transaction,
                &plan.current,
                &plan.next,
                relevance,
            )?,
            None => generation::ensure_no_current(&transaction)?,
        }
        insert_revision(
            &transaction,
            &plan.current,
            &plan.next,
            if redo { "redo" } else { "undo" },
        )?;
        if redo {
            transaction.execute(
                "DELETE FROM redo WHERE position=(SELECT MAX(position) FROM redo)",
                [],
            )?;
        } else {
            transaction.execute("INSERT INTO redo(position,history_id) VALUES ((SELECT COALESCE(MAX(position),0)+1 FROM redo),?1)", [plan.entry])?;
        }
        transaction.execute(
            "UPDATE state SET head_revision=?1,cursor=?2 WHERE singleton=1",
            params![plan.next.revision_id().as_str(), plan.next_cursor],
        )?;
        transaction.commit()?;
        Ok(CommitOutcome {
            revision_id: plan.next.revision_id().clone(),
            edit: plan.edit,
        })
    }
}

fn prepare_navigation(
    connection: &Connection,
    expected: &RevisionId,
    next_revision: RevisionId,
    redo: bool,
) -> Result<NavigationPlan, StoreError> {
    let current = read_snapshot(connection)?;
    if current.revision_id() != expected {
        return Err(StoreError::RevisionConflict {
            expected: expected.as_str().to_owned(),
            current: current.revision_id().as_str().to_owned(),
        });
    }
    let cursor: Option<i64> =
        connection.query_row("SELECT cursor FROM state WHERE singleton=1", [], |row| {
            row.get(0)
        })?;
    let entry = if redo {
        connection
            .query_row(
                "SELECT history_id FROM redo ORDER BY position DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StoreError::NothingToRedo)?
    } else {
        cursor.ok_or(StoreError::NothingToUndo)?
    };
    ensure_unused_revision(connection, &next_revision)?;
    let plan = build_navigation(connection, current, next_revision, redo, cursor, entry)?;
    check_document_size(&plan.next.to_json()?)?;
    check_document_size(&serde_json::to_string(&plan.edit)?)?;
    Ok(plan)
}

pub(crate) fn build_navigation(
    connection: &Connection,
    current: ProjectDocument,
    next_revision: RevisionId,
    redo: bool,
    cursor: Option<i64>,
    entry: i64,
) -> Result<NavigationPlan, StoreError> {
    let record = validation::read_history(connection, entry)?;
    if redo && record.parent != cursor {
        return Err(StoreError::History(
            "redo entry is not a child of the current edit".into(),
        ));
    }
    let original = record.edit;
    let patch = if redo {
        &original.forward
    } else {
        &original.inverse
    };
    let forward = patch.rebased(current.revision_id().clone(), next_revision.clone());
    let next = forward.apply(&current)?;
    let inverse = forward.inverse();
    let edit = EditTransaction {
        forward,
        inverse,
        changed_ids: original.changed_ids,
        duration_delta: next.duration()?.frames() - current.duration()?.frames(),
        description: format!(
            "{} {}",
            if redo { "Redo" } else { "Undo" },
            original.description
        ),
    };
    Ok(NavigationPlan {
        current,
        next,
        edit,
        entry,
        next_cursor: if redo { Some(entry) } else { record.parent },
    })
}
