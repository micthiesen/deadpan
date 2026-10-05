//! Validated documents of committed revisions, kept in memory per store.
//!
//! Revision identities are never reused once committed and revisions are
//! immutable, so a document cached under its identity stays correct for the
//! lifetime of the package. Entries are added only for committed data: after
//! a write transaction commits, or by a read outside any transaction. A read
//! inside a transaction may observe that transaction's own uncommitted
//! revision, which a rollback could later let another commit reuse, so it
//! only consults the cache. The head is looked up through the stored head
//! identity on every call, so a read-only store also sees another writer's
//! newer revision.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use deadpan_core::{ProjectDocument, RevisionId, ValidatedDocument};
use rusqlite::Connection;

use crate::StoreError;

/// Historical revisions read through `snapshot_at` and similar readers. The
/// head has its own slot, so these reads never evict it.
const HISTORICAL: usize = 2;

#[derive(Default)]
struct Entries {
    /// The most recently committed or read head.
    head: Option<ValidatedDocument>,
    /// Least recently used first.
    historical: Vec<ValidatedDocument>,
}

impl Entries {
    fn remember(&mut self, document: ValidatedDocument) {
        self.historical
            .retain(|entry| entry.revision_id() != document.revision_id());
        self.historical.push(document);
        if self.historical.len() > HISTORICAL {
            self.historical.remove(0);
        }
    }
}

#[derive(Default)]
pub(crate) struct DocumentCache {
    entries: Mutex<Entries>,
    initial: Mutex<Option<Arc<InitialAllocations>>>,
}

/// Allocation names an imported initial snapshot reserves forever.
pub(crate) struct InitialAllocations {
    pub revision: String,
    pub names: BTreeSet<RevisionId>,
}

impl DocumentCache {
    fn entries(&self) -> std::sync::MutexGuard<'_, Entries> {
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn lookup(&self, id: &str) -> Option<ValidatedDocument> {
        let mut entries = self.entries();
        if let Some(head) = entries
            .head
            .as_ref()
            .filter(|head| head.revision_id().as_str() == id)
        {
            return Some(head.clone());
        }
        let position = entries
            .historical
            .iter()
            .position(|document| document.revision_id().as_str() == id)?;
        let document = entries.historical.remove(position);
        entries.historical.push(document.clone());
        Some(document)
    }

    /// Record the validated document of a newly committed head. The head it
    /// replaced stays available as the most recent historical revision.
    pub(crate) fn insert(&self, document: ValidatedDocument) {
        let mut entries = self.entries();
        if let Some(previous) = entries.head.replace(document) {
            entries.remember(previous);
        }
        let head = entries.head.as_ref().map(|head| head.revision_id().clone());
        entries
            .historical
            .retain(|entry| Some(entry.revision_id()) != head.as_ref());
    }

    /// One committed revision's validated document.
    pub(crate) fn revision_validated(
        &self,
        connection: &Connection,
        id: &str,
    ) -> Result<ValidatedDocument, StoreError> {
        if let Some(document) = self.lookup(id) {
            return Ok(document);
        }
        let document = crate::validation::read_validated_revision(connection, id)?;
        if connection.is_autocommit() {
            self.entries().remember(document.clone());
        }
        Ok(document)
    }

    pub(crate) fn revision(
        &self,
        connection: &Connection,
        id: &str,
    ) -> Result<Arc<ProjectDocument>, StoreError> {
        Ok(Arc::clone(
            self.revision_validated(connection, id)?.document(),
        ))
    }

    /// The current head's validated document, kept in the head slot.
    pub(crate) fn head_validated(
        &self,
        connection: &Connection,
    ) -> Result<ValidatedDocument, StoreError> {
        let head = crate::validation::read_head(connection)?;
        if let Some(document) = self.lookup(&head) {
            return Ok(document);
        }
        let document = crate::validation::read_validated_revision(connection, &head)?;
        if connection.is_autocommit() {
            self.insert(document.clone());
        }
        Ok(document)
    }

    pub(crate) fn head(&self, connection: &Connection) -> Result<Arc<ProjectDocument>, StoreError> {
        Ok(Arc::clone(self.head_validated(connection)?.document()))
    }

    /// Allocation names of the immutable initial revision.
    pub(crate) fn initial_allocations(
        &self,
        connection: &Connection,
    ) -> Result<Arc<InitialAllocations>, StoreError> {
        let id = crate::validation::read_initial_id(connection)?;
        let mut cached = self
            .initial
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(initial) = cached.as_ref().filter(|initial| initial.revision == id) {
            return Ok(Arc::clone(initial));
        }
        let initial = crate::validation::read_revision(connection, &id)?.document;
        let names = initial
            .nodes()
            .values()
            .filter_map(|node| match &node.kind {
                deadpan_core::NodeKind::Repeat { iterations, .. } => Some(iterations),
                _ => None,
            })
            .flat_map(|iterations| iterations.segments().map(|(id, _, _)| id.clone()))
            .chain(
                initial
                    .audio_lineage()
                    .values()
                    .map(|lineage| lineage.allocation.clone()),
            )
            .chain(
                initial
                    .audio_bindings()
                    .allocation_ids()
                    .into_iter()
                    .cloned(),
            )
            .collect();
        let allocations = Arc::new(InitialAllocations {
            revision: id,
            names,
        });
        // The initial revision is immutable and always stored, so its
        // allocations are cached regardless of the surrounding transaction.
        *cached = Some(Arc::clone(&allocations));
        Ok(allocations)
    }
}
