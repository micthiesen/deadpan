use std::sync::Arc;

use deadpan_core::{
    DocumentPatch, EditTransaction, ProjectDocument, RevisionId, ValidatedDocument,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::document_cache::DocumentCache;
use crate::{
    CommitOutcome, ProjectStore, StoreError, check_document_size, ensure_unused_revision,
    generation, insert_revision, validation,
};

pub(crate) struct NavigationPlan {
    current: Arc<ProjectDocument>,
    pub next: ValidatedDocument,
    pub(crate) edit: EditTransaction,
    entry: i64,
    pub next_cursor: Option<i64>,
}

impl ProjectStore {
    /// Availability at the current durable cursor without building inverse edits.
    pub fn history_availability(&self) -> Result<(bool, bool), StoreError> {
        let (undo, redo) = self.connection.query_row(
            "SELECT cursor IS NOT NULL, EXISTS(SELECT 1 FROM redo) FROM state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let undo = undo && !crate::single_source::at_baseline(&self.connection)?;
        Ok((undo, redo))
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
        let plan =
            prepare_navigation(&transaction, &self.documents, expected, next_revision, redo)?;
        Ok(CommitOutcome {
            revision_id: plan.next.revision_id().clone(),
            edit: plan.edit,
            register_bank: None,
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
        let resolver = self.context_resolver.clone();
        let documents = &self.documents;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let plan = prepare_navigation(&transaction, documents, expected, next_revision, redo)?;
        generation::reconcile(
            &transaction,
            &plan.current,
            &plan.next,
            relevance,
            resolver.as_deref(),
        )?;
        // Every replay check for a navigation revision: the plan was built from
        // the stored entry, and its stored patch must decode to the same value.
        let json = serde_json::to_string(&plan.edit.forward)?;
        check_document_size(&json)?;
        if serde_json::from_str::<DocumentPatch>(&json)? != plan.edit.forward {
            return Err(StoreError::Integrity(
                "navigation patch changes meaning when decoded for history replay".into(),
            ));
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::single_source::check_navigation(&transaction, &plan.current, &plan.next)?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::source_registration::check_revision_assets(&transaction, &plan.current, &plan.next)?;
        insert_revision(
            &transaction,
            documents,
            &plan.current,
            &plan.next,
            if redo { "redo" } else { "undo" },
            crate::revision_storage::StoredPatch::Navigation { json: &json },
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
        crate::audit::extend(
            &transaction,
            plan.current.revision_id().as_str(),
            plan.next.revision_id().as_str(),
        )?;
        transaction.commit()?;
        documents.insert(plan.next.clone());
        Ok(CommitOutcome {
            revision_id: plan.next.revision_id().clone(),
            edit: plan.edit,
            register_bank: None,
        })
    }
}

fn prepare_navigation(
    connection: &Connection,
    documents: &DocumentCache,
    expected: &RevisionId,
    next_revision: RevisionId,
    redo: bool,
) -> Result<NavigationPlan, StoreError> {
    let current = documents.head_validated(connection)?;
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if crate::single_source::at_baseline(connection)? {
            return Err(StoreError::NothingToUndo);
        }
        cursor.ok_or(StoreError::NothingToUndo)?
    };
    ensure_unused_revision(connection, documents, &next_revision)?;
    let plan = current
        .scope(|| build_navigation(connection, &current, next_revision, redo, cursor, entry))?;
    check_document_size(&serde_json::to_string(&plan.edit)?)?;
    Ok(plan)
}

/// Rebuild an undo or redo from its stored history entry; the restored
/// document is validated once.
pub(crate) fn build_navigation(
    connection: &Connection,
    current: &ValidatedDocument,
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
    let next = current.apply_patch(&forward)?;
    #[cfg(debug_assertions)]
    next.check_against_complete_validation()
        .expect("retained validation must equal complete validation");
    let inverse = forward.inverse();
    let edit = EditTransaction {
        forward,
        inverse,
        changed_ids: original.changed_ids,
        duration_delta: next.durations()[next.root()].frames()
            - current.structural_duration()?.frames(),
        description: format!(
            "{} {}",
            if redo { "Redo" } else { "Undo" },
            original.description
        ),
    };
    Ok(NavigationPlan {
        current: Arc::clone(current.document()),
        next,
        edit,
        entry,
        next_cursor: if redo { Some(entry) } else { record.parent },
    })
}
