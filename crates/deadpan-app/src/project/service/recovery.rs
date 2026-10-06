//! Open-time recovery reporting, the persistent storage alert and locating a
//! missing Original. A relink never edits authored history: it restores the
//! bytes a registered original already names, after verifying them.

use deadpan_store::original_media::{
    LinkedOriginal, OriginalAvailability, OriginalContentId, OriginalRetentionMethod,
};

use super::*;
use crate::project::{OpenReport, OriginalStatus, RelinkState, RelinkStatus};
use crate::recovery::{StorageAlert, storage_code};

/// What the storage alert has already seen, so a standing failure message
/// raises it once and a save without a new revision can clear it.
#[derive(Default)]
pub(super) struct StorageWatch {
    seen: Vec<String>,
    saves: Option<SaveMarks>,
}

type SaveMarks = (Option<usize>, [Option<u64>; 4], Option<Option<u64>>);

pub(super) struct Relinking {
    id: u64,
    ticket: u64,
    session: u64,
    label: String,
    cancelled: Arc<AtomicBool>,
}

/// Recovery facts from the writer open and a metadata-only presence check of
/// each registered original. Opening never fails because media is missing.
pub(super) fn open_report(store: &ProjectStore, workspace: &Workspace) -> OpenReport {
    OpenReport {
        session: workspace.session,
        path: workspace.path.clone(),
        recovery: store.open_recovery().clone(),
        originals: original_statuses(store, workspace),
    }
}

fn original_statuses(store: &ProjectStore, workspace: &Workspace) -> Vec<OriginalStatus> {
    let primary = match &workspace.single_source {
        Some(SingleSourceState::Ready { asset, .. }) => Some(asset),
        _ => None,
    };
    workspace
        .sources
        .values()
        .map(|source| OriginalStatus {
            label: source.label.clone(),
            primary: primary == Some(&source.asset),
            record: source.original.clone(),
            availability: store
                .original_availability(&source.original)
                .unwrap_or_else(|error| OriginalAvailability::Unreadable {
                    reason: error.to_string(),
                }),
        })
        .collect()
}

impl Service {
    pub(super) fn current_storage_alert(&self) -> Option<StorageAlert> {
        let alert = self.storage.as_ref()?;
        let workspace = self.workspace.as_ref()?;
        // A later successful save moved the head: the alert is resolved.
        (workspace.session == alert.session
            && Some(workspace.document.revision_id()) == alert.revision.as_ref())
        .then(|| alert.clone())
    }

    /// Every failure message the service is about to publish, from every
    /// path that writes the store: commands, copies and cuts, registers,
    /// Render and its history, annotation saves, imports and relinks.
    fn failure_messages(&self) -> Vec<String> {
        fn failed<T>(result: &std::result::Result<T, String>) -> Option<&String> {
            result.as_ref().err()
        }
        let mut messages: Vec<String> = [
            self.error.as_ref(),
            self.import
                .as_ref()
                .and_then(|status| status.error.as_ref()),
            self.captured_original
                .as_ref()
                .and_then(|update| failed(&update.result)),
            self.captured_slice
                .as_ref()
                .and_then(|update| failed(&update.result)),
            self.cut_slice
                .as_ref()
                .and_then(|update| failed(&update.result)),
            self.slip_commit
                .as_ref()
                .and_then(|update| failed(&update.result)),
            self.trim_commit
                .as_ref()
                .and_then(|update| failed(&update.result)),
            self.splice_commit
                .as_ref()
                .and_then(|update| failed(&update.result)),
            self.macros
                .as_ref()
                .and_then(|update| failed(&update.result)),
            self.room_tone_error.as_ref().map(|failure| &failure.error),
            self.transcript_save
                .as_ref()
                .and_then(|save| save.error.as_ref()),
            self.activity_save
                .as_ref()
                .and_then(|save| save.error.as_ref()),
            self.shot_save.as_ref().and_then(|save| save.error.as_ref()),
            self.correction_save
                .as_ref()
                .and_then(|save| save.error.as_ref()),
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
        if let Some(render) = &self.render_update {
            messages.extend(
                render
                    .command
                    .as_ref()
                    .and_then(|command| command.result.as_ref().err())
                    .into_iter()
                    .chain(render.service_error.as_ref())
                    .map(|error| error.message.clone()),
            );
        }
        if let Some(Err(error)) = self.render_history.as_ref().map(|update| &update.result) {
            messages.push(error.message.clone());
        }
        if let Some(RelinkState::Failed(error)) = self.relink.as_ref().map(|status| &status.state) {
            messages.push(error.clone());
        }
        messages
    }

    /// The one choke point: before publishing, any newly appearing storage
    /// failure raises the alert, and a successful save that creates no
    /// revision (an annotation, register or relink save) clears it.
    pub(super) fn observe_storage(&mut self) {
        let failures: Vec<String> = self
            .failure_messages()
            .into_iter()
            .filter(|message| storage_code(message).is_some())
            .collect();
        let saves = (
            self.registers
                .as_ref()
                .map(|bank| Arc::as_ptr(bank) as usize),
            [
                &self.transcript_save,
                &self.activity_save,
                &self.shot_save,
                &self.correction_save,
            ]
            .map(|save| {
                save.as_ref()
                    .filter(|save| save.error.is_none())
                    .map(|save| save.attempt)
            }),
            matches!(
                self.relink.as_ref().map(|status| &status.state),
                Some(RelinkState::Restored)
            )
            .then(|| self.relink.as_ref().map(|status| status.ticket)),
        );
        let saved_without_revision = self.storage_watch.saves.as_ref().is_some_and(|before| {
            before.0 != saves.0
                || before
                    .1
                    .iter()
                    .zip(saves.1.iter())
                    .any(|(before, now)| now.is_some() && before != now)
                || (saves.2.is_some() && before.2 != saves.2)
        });
        if saved_without_revision && failures.iter().all(|f| self.storage_watch.seen.contains(f)) {
            self.storage = None;
        }
        if let Some(workspace) = &self.workspace
            && let Some(message) = failures
                .iter()
                .find(|message| !self.storage_watch.seen.contains(message))
        {
            self.storage = Some(StorageAlert {
                session: workspace.session,
                code: storage_code(message).expect("filtered storage failure"),
                revision: Some(workspace.document.revision_id().clone()),
                message: message.clone(),
            });
        }
        self.storage_watch.seen = failures;
        self.storage_watch.saves = Some(saves);
    }

    pub(super) fn relink_original(
        &mut self,
        ticket: u64,
        expected_session: u64,
        content: &OriginalContentId,
        expected_version: u64,
        path: PathBuf,
    ) -> Result<()> {
        let workspace = self.workspace.as_ref().ok_or("No project is open")?;
        if workspace.session != expected_session {
            return Err("The project changed while choosing the file. Locate the Original again in this project.".into());
        }
        if self.active.is_some() || self.relinking.is_some() || self.host_preparation_active() {
            return Err(
                "Wait for the current import or relink to finish, then locate the Original again."
                    .into(),
            );
        }
        if !path.is_absolute() {
            return Err("Choose the original file by its full location.".into());
        }
        let session = workspace.session;
        let handle = workspace.originals.clone();
        let store = self.store.as_ref().ok_or("No project is open")?;
        let record = store
            .original_record(content)
            .map_err(display)?
            .ok_or("This original is no longer registered in the project.")?;
        if record.version() != expected_version {
            return Err(
                "The original's location changed since you chose it. Locate it again.".into(),
            );
        }
        let label = workspace
            .sources
            .values()
            .find(|source| source.original.object().content() == content)
            .map_or_else(|| record.label().to_owned(), |source| source.label.clone());
        let work = if record.managed() {
            Work::Restore { record, path }
        } else {
            Work::Relink {
                location: LinkedOriginal::new(path, None).map_err(|error| error.to_string())?,
                record,
            }
        };
        let id = self
            .serial
            .checked_add(1)
            .ok_or("Import identities exhausted")?;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.jobs
            .try_send(Job {
                id,
                handle,
                cancelled: cancelled.clone(),
                work,
            })
            .map_err(|_| "The import worker is busy; try again in a moment.")?;
        self.serial = id;
        self.relinking = Some(Relinking {
            id,
            ticket,
            session,
            label: label.clone(),
            cancelled,
        });
        self.relink = Some(RelinkStatus {
            ticket,
            session,
            label: label.clone(),
            state: RelinkState::Verifying,
        });
        self.message = Some(format!("Verifying the chosen file for {label}…"));
        Ok(())
    }

    /// Consumes this relink's worker reply; other replies pass through.
    pub(super) fn relink_result(&mut self, reply: Reply) -> Option<Reply> {
        if self
            .relinking
            .as_ref()
            .is_none_or(|relinking| relinking.id != reply.id)
        {
            return Some(reply);
        }
        let relinking = self.relinking.take()?;
        let current = self.session == relinking.session
            && self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.session == relinking.session)
            && !relinking.cancelled.load(Ordering::Acquire);
        if !current {
            // A later session never receives an earlier project's original.
            return None;
        }
        let committed = match reply.result {
            Ok(Prepared::Restored(prepared)) => self.writer().and_then(|store| {
                let outcome = store
                    .retain_prepared_original(&prepared.retention, &relinking.cancelled)
                    .map_err(display)?;
                Ok(if prepared.quarantined.is_some() {
                    "The damaged project copy was set aside and replaced by the verified file. Its pictures and sound are available again."
                } else if outcome.method == OriginalRetentionMethod::Existing {
                    "The project copy is intact; nothing needed to change."
                } else {
                    "found and verified. Its pictures and sound are available again."
                })
            }),
            Ok(Prepared::Relinked(prepared)) => self.writer().and_then(|store| {
                store
                    .relink_prepared_original(&prepared, &relinking.cancelled)
                    .map(|_| "found and verified. Its pictures and sound are available again.")
                    .map_err(display)
            }),
            Ok(_) => Err("The import worker returned an unrelated result.".into()),
            Err(error) => Err(error),
        };
        let state =
            match committed.and_then(|outcome| self.refresh_after_relink().map(|()| outcome)) {
                Ok(outcome) => {
                    self.message = Some(format!("{}: {outcome}", relinking.label));
                    RelinkState::Restored
                }
                Err(error) => {
                    self.error = Some(error.clone());
                    self.message = None;
                    RelinkState::Failed(error)
                }
            };
        self.relink = Some(RelinkStatus {
            ticket: relinking.ticket,
            session: relinking.session,
            label: relinking.label,
            state,
        });
        None
    }

    fn refresh_after_relink(&mut self) -> Result<()> {
        self.cached = None;
        self.refresh()?;
        let (Some(store), Some(workspace), Some(previous)) =
            (&self.store, &self.workspace, &self.opened)
        else {
            return Ok(());
        };
        self.opened = Some(Arc::new(OpenReport {
            session: previous.session,
            path: previous.path.clone(),
            recovery: previous.recovery.clone(),
            originals: original_statuses(store, workspace),
        }));
        Ok(())
    }
}
