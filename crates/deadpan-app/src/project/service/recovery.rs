//! Open-time recovery reporting, the persistent storage alert and locating a
//! missing Original. A relink never edits authored history: it restores the
//! bytes a registered original already names, after verifying them.

use deadpan_store::original_media::{
    LinkedOriginal, OriginalAvailability, OriginalContentId, OriginalRetentionMethod,
    moved_location,
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
    take_saved: bool,
}

type SaveMarks = (Option<usize>, [Option<u64>; 4], Option<Option<u64>>);

impl StorageWatch {
    /// A changed take catalog is a durable save without a document revision.
    /// Reads, dry runs and no-ops must never acknowledge a storage failure.
    pub(super) fn record_take_save(&mut self) {
        self.take_saved = true;
    }
}

pub(super) struct Relinking {
    id: u64,
    ticket: u64,
    session: u64,
    label: String,
    cancelled: Arc<AtomicBool>,
    /// Found by a bookmark rather than chosen by the person: where, and
    /// which original.
    moved_to: Option<(PathBuf, OriginalContentId)>,
}

/// The ticket of relinks the service starts itself for moved linked files.
pub(super) const AUTOMATIC_RELINK: u64 = 0;

/// Recovery facts from the writer open and a metadata-only presence check of
/// each registered original. Opening never fails because media is missing.
pub(super) fn open_report(store: &ProjectStore, workspace: &Workspace) -> OpenReport {
    OpenReport {
        session: workspace.session,
        path: workspace.path.clone(),
        recovery: store.open_recovery().clone(),
        originals: original_statuses(store, workspace, &Default::default()),
    }
}

/// `attempted`: originals whose bookmark candidate was already tried this
/// session and refused; they are reported missing again.
fn original_statuses(
    store: &ProjectStore,
    workspace: &Workspace,
    attempted: &std::collections::BTreeSet<OriginalContentId>,
) -> Vec<OriginalStatus> {
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
            // A writable session relinks a moved linked file once its bytes
            // verify; a read-only view only reports it missing.
            moved_to: (store.access_mode() == AccessMode::ReadWrite
                && !attempted.contains(source.original.object().content()))
            .then(|| moved_location(&source.original))
            .flatten()
            .map(|location| location.path().to_owned()),
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
    /// revision (an annotation, register, take or relink save) clears it.
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
        let saved_take = std::mem::take(&mut self.storage_watch.take_saved);
        let saved_without_revision = saved_take
            || self.storage_watch.saves.as_ref().is_some_and(|before| {
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
                // A fresh bookmark, so a later move is found again.
                location: LinkedOriginal::bookmarked(path).map_err(|error| error.to_string())?,
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
            moved_to: None,
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
        if let Some((_, content)) = &relinking.moved_to {
            // Tried once per session, whatever the outcome.
            self.auto_relinked.insert(content.clone());
        }
        let committed = match (relinking.moved_to.as_ref().map(|(path, _)| path), committed) {
            (Some(moved), Ok(_)) => Ok(format!(
                "found by its bookmark at {} and verified; relinked there.",
                moved.display()
            )),
            (Some(moved), Err(error)) => Err(format!(
                "{} was found by its bookmark at {}, but that file is not the same content, so it was not relinked ({error}). Use :relink to locate it.",
                relinking.label,
                moved.display()
            )),
            (None, result) => result.map(str::to_owned),
        };
        let state =
            match committed.and_then(|outcome| self.refresh_after_relink().map(|()| outcome)) {
                Ok(outcome) => {
                    self.message = Some(format!("{}: {outcome}", relinking.label));
                    RelinkState::Restored
                }
                Err(error) => {
                    // An automatic failure still updates the report so the
                    // file shows as missing again.
                    if relinking.moved_to.is_some() {
                        // Not the person's command: report it through the
                        // relink status and message, never editor feedback.
                        let _ = self.refresh_after_relink();
                        self.message = Some(error.clone());
                    } else {
                        self.error = Some(error.clone());
                        self.message = None;
                    }
                    RelinkState::Failed(error)
                }
            };
        self.relink = Some(RelinkStatus {
            ticket: relinking.ticket,
            session: relinking.session,
            label: relinking.label,
            state,
        });
        self.relink_moved_originals();
        None
    }

    /// Start verifying the next missing linked file a bookmark found, one at
    /// a time on the import worker. Each candidate is tried once per
    /// session; only identical bytes are relinked.
    pub(super) fn relink_moved_originals(&mut self) {
        if self.active.is_some() || self.relinking.is_some() || self.host_preparation_active() {
            return;
        }
        let Some(report) = self.opened.clone() else {
            return;
        };
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        if report.session != workspace.session {
            return;
        }
        let Some(status) = report.originals.iter().find(|status| {
            status.moved_to.is_some()
                && !self
                    .auto_relinked
                    .contains(status.record.object().content())
        }) else {
            return;
        };
        let moved = status.moved_to.clone().expect("filtered");
        let Ok(location) = LinkedOriginal::bookmarked(moved.clone()) else {
            self.auto_relinked
                .insert(status.record.object().content().clone());
            return;
        };
        let Some(id) = self.serial.checked_add(1) else {
            return;
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        if self
            .jobs
            .try_send(Job {
                id,
                handle: workspace.originals.clone(),
                cancelled: cancelled.clone(),
                work: Work::Relink {
                    record: status.record.clone(),
                    location,
                },
            })
            .is_err()
        {
            return;
        }
        self.serial = id;
        self.relinking = Some(Relinking {
            id,
            ticket: AUTOMATIC_RELINK,
            session: workspace.session,
            label: status.label.clone(),
            cancelled,
            moved_to: Some((moved.clone(), status.record.object().content().clone())),
        });
        self.relink = Some(RelinkStatus {
            ticket: AUTOMATIC_RELINK,
            session: workspace.session,
            label: status.label.clone(),
            state: RelinkState::Verifying,
        });
        self.message = Some(format!(
            "{} moved; found it by its bookmark at {}. Verifying its contents…",
            status.label,
            moved.display()
        ));
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
            originals: original_statuses(store, workspace, &self.auto_relinked),
        }));
        Ok(())
    }
}
