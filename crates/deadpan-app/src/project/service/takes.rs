//! Named take admission and durable feedback, separate from selection edits.

use super::*;
use crate::project::takes::{Operation, Receipt, Request, Update};

impl Service {
    pub(super) fn takes_command(&mut self, request: Request) {
        let result = self.apply_takes(&request);
        if matches!(request.operation, Operation::Apply(_))
            && let Err(error) = &result
        {
            self.error = Some(error.clone());
            self.message = None;
        }
        self.takes = Some(Update {
            ticket: request.ticket,
            session: request.session,
            result,
        });
    }

    fn apply_takes(&mut self, request: &Request) -> Result<Receipt> {
        if request.ticket == 0 {
            return Err("Take request ticket must be nonzero".into());
        }
        self.check_context(request.session, &request.revision)?;
        if self.pending_session_change.is_some() {
            return Err("Project is closing or changing before the take request".into());
        }
        let store = self.store.as_ref().ok_or("Open a project first")?;
        if store.newer_schema().is_some() {
            return Err("This project uses a newer format. Open it in that Deadpan version to browse its takes.".into());
        }
        match &request.operation {
            Operation::List => Ok(Receipt {
                catalog: store.take_catalog().map_err(display)?,
                committed_revision: None,
                changed: false,
                refresh_error: None,
            }),
            Operation::Apply(operation) => {
                if operation.expected_revision != request.revision {
                    return Err("Take and editor revision identities differ".into());
                }
                if self.shared.preview_active.load(Ordering::Acquire) {
                    return Err(
                        "Finish or cancel the unsaved preview before changing takes.".into(),
                    );
                }
                let outcome = self.writer()?.apply_take(operation).map_err(display)?;
                if outcome.changed && outcome.commit.is_none() {
                    self.storage_watch.record_take_save();
                }
                let committed_revision = outcome.commit.map(|commit| commit.revision_id);
                let refresh_error = if committed_revision.is_some() {
                    self.committed = None;
                    self.room_tone = None;
                    self.room_tone_error = None;
                    self.gain = None;
                    self.refresh_saved("Take opened and saved").err()
                } else {
                    None
                };
                self.error = refresh_error.clone();
                self.message = Some(if committed_revision.is_some() {
                    "Take opened. Undo returns to the previous edit.".into()
                } else if outcome.changed {
                    "Saved takes updated. The current edit is unchanged.".into()
                } else {
                    "Take already matches the saved edit.".into()
                });
                self.take_catalog = Some(outcome.catalog.clone());
                Ok(Receipt {
                    catalog: outcome.catalog,
                    committed_revision,
                    changed: outcome.changed,
                    refresh_error,
                })
            }
        }
    }
}
