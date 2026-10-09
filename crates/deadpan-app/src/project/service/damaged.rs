//! Recovery of a failed Open runs off the UI and retains its original target.

use super::*;
use crate::project::damaged::{Action, Offer, Outcome, Reply, Request, Update};
use deadpan_store::backups::{
    DamagedRecovery, inspect_damaged_database, list_backups, restore_damaged_database,
};

#[derive(Default)]
pub(super) struct State {
    serial: u64,
    origin: Option<(u64, Option<RevisionId>)>,
    pub(super) update: Option<Update>,
    inspected: Option<DamagedRecovery>,
}

impl State {
    pub(super) fn clear(&mut self) {
        self.origin = None;
        self.update = None;
        self.inspected = None;
    }
}

impl Service {
    fn damaged_context(&self) -> Result<(u64, Option<RevisionId>)> {
        Ok((
            self.session,
            self.store
                .as_ref()
                .map(ProjectStore::head_revision)
                .transpose()
                .map_err(display)?,
        ))
    }

    pub(super) fn offer_damaged_open(&mut self, path: &std::path::Path, error: &str) {
        let Ok(backups) = list_backups(path) else {
            return;
        };
        if backups.is_empty() {
            return;
        }
        let Ok(path) = path.canonicalize() else {
            return;
        };
        let Ok(origin) = self.damaged_context() else {
            return;
        };
        let Some(id) = self.damaged.serial.checked_add(1) else {
            return;
        };
        self.damaged.serial = id;
        self.damaged.origin = Some(origin);
        self.damaged.update = Some(Update {
            offer: Arc::new(Offer {
                id,
                path,
                error: error.into(),
                backups,
            }),
            reply: None,
        });
    }

    pub(super) fn damaged_command(&mut self, request: Request) {
        let Some(update) = &self.damaged.update else {
            self.error =
                Some("The failed Open is no longer current. Open that project again.".into());
            return;
        };
        let offer = Arc::clone(&update.offer);
        if request.offer != offer.id || request.ticket == 0 {
            self.error = Some("The failed Open changed before this recovery request.".into());
            return;
        }
        if matches!(request.action, Action::Dismiss) {
            self.damaged.clear();
            return;
        }
        let result = self.apply_damaged(&offer, request.action);
        self.damaged.update = Some(Update {
            offer,
            reply: Some(Reply {
                ticket: request.ticket,
                result,
            }),
        });
    }

    fn apply_damaged(&mut self, offer: &Offer, action: Action) -> Result<Outcome> {
        if self.damaged.origin.as_ref() != Some(&self.damaged_context()?)
            || self.pending_session_change.is_some()
        {
            return Err(
                "The open project changed. Open the damaged project again before recovering it."
                    .into(),
            );
        }
        match action {
            Action::Inspect { backup } => {
                self.damaged.inspected = None;
                if !offer.backups.iter().any(|info| info.id == backup) {
                    return Err("This backup was not listed for the failed Open.".into());
                }
                let inspected = inspect_damaged_database(&offer.path, &backup).map_err(display)?;
                let outcome = Outcome::Inspected {
                    preview: inspected.preview.clone(),
                    requires_project_confirmation: inspected.requires_project_confirmation,
                };
                self.damaged.inspected = Some(inspected);
                Ok(outcome)
            }
            Action::Restore {
                backup,
                confirmed_project,
            } => {
                if self.active.is_some()
                    || self.relinking.is_some()
                    || self.render.is_some()
                    || self.generation.active()
                    || self.targets.active()
                    || self.host_preparation_active()
                    || self.remote_storage.is_some()
                    || self.shared.preview_active.load(Ordering::Acquire)
                    || self.splice_draft.is_some()
                    || self.slip_draft.is_some()
                    || self.trim_draft.is_some()
                {
                    return Err("Finish or cancel current previews, imports and background work before recovering another project.".into());
                }
                let inspected = self
                    .damaged
                    .inspected
                    .as_ref()
                    .filter(|capture| capture.preview.info.id == backup)
                    .ok_or("Check the selected backup before restoring it.")?;
                let replaced = restore_damaged_database(inspected, confirmed_project.as_ref())
                    .map_err(display)?;
                self.damaged.inspected = None;
                // The disk replacement already happened. A later Open failure is
                // reported separately so the person never retries it blindly.
                let opened =
                    self.prepare_open_inner(offer.path.clone(), false)
                        .and_then(|prepared| match prepared {
                            Some(prepared) => self.install_open(prepared),
                            None => Err("The recovered project unexpectedly remained open.".into()),
                        });
                let open_error = opened.err();
                self.error = open_error.clone();
                self.message = Some(format!(
                    "Backup restored. Previous database files were kept at {}.{}",
                    replaced.quarantine.display(),
                    if open_error.is_some() {
                        " Opening failed; reopen the recovered project."
                    } else {
                        ""
                    }
                ));
                Ok(Outcome::Restored {
                    quarantine: replaced.quarantine,
                    open_error,
                    warnings: replaced.warnings,
                })
            }
            Action::Dismiss => unreachable!("dismiss handled before context admission"),
        }
    }
}
