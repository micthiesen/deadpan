//! Explicit storage cleanup on the project writer.
//!
//! The Storage panel previews cleanup on a read-only open in its own
//! thread. Only the confirmed removal runs here, because it needs the
//! writable store this service owns: it rescans references (the safety
//! check, which blocks other project commands for its duration, typically
//! well under a second and longer for very long histories), waits at most
//! two seconds for the render namespace lock, and removes only previewed
//! entries that are still unreferenced. It is refused while any job of this
//! session could be publishing media whose row is not yet committed; the
//! default grace period additionally keeps every object changed in the last
//! day.

use deadpan_store::storage::{DEFAULT_GRACE, RemovedEntry};

use super::Service;

impl Service {
    pub(super) fn clean_storage(
        &mut self,
        ticket: u64,
        expected_session: u64,
        previewed: &[RemovedEntry],
    ) {
        let session = self.session;
        let result = if self
            .workspace
            .as_ref()
            .is_none_or(|workspace| workspace.session != expected_session)
        {
            Err("The project changed; open Storage again.".to_owned())
        } else if self.active.is_some()
            || self.relinking.is_some()
            || self.host_preparation_active()
            || self.render.is_some()
            || self.generation.active()
            || self.targets.active()
        {
            Err("Wait for the current import, render, AI pause or tracking to finish, then clean up again.".to_owned())
        } else {
            match self.store.as_mut() {
                None => Err("Open a project first.".to_owned()),
                Some(store) => store
                    .clean_previewed_storage(DEFAULT_GRACE, previewed)
                    .map_err(|error| error.to_string()),
            }
        };
        self.storage_cleanup = Some(super::super::StorageCleanupStatus {
            ticket,
            session,
            result,
        });
    }
}
