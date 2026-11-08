//! Named Original moments: catalog writes outside authored history.

use super::*;
use crate::project::moments::{Operation, Receipt, Request, Update};

impl Service {
    pub(super) fn moments_command(&mut self, request: Request) {
        // Failures reach the rail's own feedback, never the editor's
        // command error, which would outlive the next unrelated command.
        let result = self.apply_moments(&request);
        if let Ok(receipt) = &result {
            self.moment_catalog = Some(receipt.catalog.clone());
        }
        self.moments = Some(Update {
            ticket: request.ticket,
            session: request.session,
            result,
        });
    }

    fn apply_moments(&mut self, request: &Request) -> Result<Receipt> {
        if request.ticket == 0 {
            return Err("Moment request ticket must be nonzero".into());
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if workspace.session != request.session {
            return Err("Project session changed before the moment request".into());
        }
        if self.pending_session_change.is_some() {
            return Err("Project is closing or changing before the moment request".into());
        }
        let store = self.store.as_ref().ok_or("Open a project first")?;
        if store.newer_schema().is_some() {
            return Err("This project uses a newer format. Open it in that Deadpan version to browse its moments.".into());
        }
        match &request.operation {
            Operation::List => Ok(Receipt {
                catalog: store.moment_catalog().map_err(display)?,
                changed: false,
            }),
            Operation::Apply(operation) => {
                let outcome = self.writer()?.apply_moment(operation).map_err(display)?;
                if outcome.changed {
                    self.storage_watch.record_take_save();
                }
                Ok(Receipt {
                    catalog: outcome.catalog,
                    changed: outcome.changed,
                })
            }
        }
    }
}
