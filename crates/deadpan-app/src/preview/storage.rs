//! The Storage panel: how much space this project and Deadpan's per-user
//! caches use, what nothing references any more, explicit cleanup, and
//! portable copies (specification Sections 19.2 and 20.1, DP-19).
//!
//! `:storage` or Deadpan › Storage… opens it; File › Save Portable Copy… and
//! `:portable-copy` start a copy. The report is computed off the UI thread
//! from a read-only open of the package, so it never waits for the project
//! writer. P previews project cleanup on a read-only open off the writer;
//! R asks the project service, which owns the writer, to remove exactly the
//! previewed files that a fresh scan still finds unreferenced. Any project
//! change discards the preview. Per-user caches (C) are rebuildable.
//! Escape or Close closes it. Nothing here edits the project or its history,
//! except restoring a backup (BACKUPS section, `preview/backups.rs`), which
//! replaces it after backing up the current state.

use std::sync::mpsc;

use deadpan_cli::storage::{UserCleanupOutcome, UserStorage, UserStorageReport};
use deadpan_store::generation_retention::{ClockAnomaly, ExpiryPlan, VariantExpiry};
use deadpan_store::portable::PortableCopyReport;
use deadpan_store::storage::{CleanupOutcome, DEFAULT_GRACE, StorageReport};

use super::*;

/// One computed report.
pub(super) struct Snapshot {
    pub(super) project: Option<Result<StorageReport, String>>,
    pub(super) user: Option<UserStorageReport>,
}

/// The background preview's reply channel.
type PreviewReply = mpsc::Receiver<Result<CleanupOutcome, String>>;

#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    focus_pending: bool,
    return_focus: Option<(u64, Pane)>,
    loading: Option<mpsc::Receiver<Snapshot>>,
    pub(super) snapshot: Option<Snapshot>,
    ticket: u64,
    /// The service cleanup request awaiting its reply, and whether it was a
    /// dry run.
    pending: Option<u64>,
    /// A preview running on a read-only open, for its session and revision.
    previewing: Option<(u64, String, PreviewReply)>,
    /// The latest previewed cleanup and the session and revision it saw. R
    /// removes exactly these entries; any project change discards it.
    preview: Option<(u64, String, CleanupOutcome)>,
    caches: Option<mpsc::Receiver<Result<UserCleanupOutcome, String>>>,
    copy: Option<mpsc::Receiver<Result<PortableCopyReport, String>>>,
    pub(super) status: Option<String>,
    /// The per-user directories to account for; replay uses a private root
    /// so it never reports or cleans the person's real caches.
    pub(super) user: Option<UserStorage>,
    /// The BACKUPS section.
    pub(super) backups: super::backups::View,
    /// This session's automatic AI variant retention pass.
    pub(super) retention: Option<crate::project::RetentionPassStatus>,
    /// The two-press clock confirmation (E) after a long gap.
    pub(super) clock: Option<ClockStep>,
}

/// Where the E clock confirmation is.
pub(super) enum ClockStep {
    /// Planning an explicit expiry on a read-only open, off the writer.
    Planning(u64, String, mpsc::Receiver<Result<ExpiryPlan, String>>),
    /// Reviewed: a second E confirms exactly this plan.
    Ready(u64, String, Box<ExpiryPlan>),
    /// Sent to the project service with this ticket.
    Pending(u64),
}

impl State {
    fn user(&self) -> Option<UserStorage> {
        self.user.clone().or_else(UserStorage::current)
    }

    pub(super) fn copying(&self) -> bool {
        self.copy.is_some()
    }
}

pub(super) fn bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Rows of the project section: label, value.
pub(super) fn project_rows(report: &StorageReport) -> Vec<(String, String)> {
    let mut rows = vec![("Database".to_owned(), bytes(report.database_bytes))];
    for namespace in &report.namespaces {
        let label = match namespace.namespace {
            "originals" => "Originals",
            "generated" => "AI pause media",
            "render_candidates" => "Render candidates",
            other => other,
        };
        let mut value = format!("{} referenced", bytes(namespace.referenced_bytes));
        if namespace.unreferenced_bytes > 0 {
            value.push_str(&format!(
                " · {} unreferenced",
                bytes(namespace.unreferenced_bytes)
            ));
        }
        if namespace.pending_bytes > 0 {
            value.push_str(&format!(
                " · {} unfinished writes",
                bytes(namespace.pending_bytes)
            ));
        }
        if namespace.other_bytes > 0 {
            value.push_str(&format!(" · {} kept aside", bytes(namespace.other_bytes)));
        }
        rows.push((label.to_owned(), value));
    }
    rows.push((
        "History".to_owned(),
        format!(
            "{} revisions · {} ({} in {} keyframes); all kept",
            report.history.revisions,
            bytes(
                report
                    .history
                    .keyframe_bytes
                    .saturating_add(report.history.patch_bytes)
            ),
            bytes(report.history.keyframe_bytes),
            report.history.keyframes
        ),
    ));
    rows.push((
        "Checkpoints, backups and reports".to_owned(),
        bytes(report.auxiliary_bytes),
    ));
    rows.push(("Total".to_owned(), bytes(report.total_bytes)));
    rows.push((
        "Removable now".to_owned(),
        format!(
            "{} (unreferenced and unchanged for {} hours)",
            bytes(report.removable_bytes),
            report.grace_seconds / 3600
        ),
    ));
    rows
}

fn days(seconds: u64) -> String {
    let days = seconds / (24 * 60 * 60);
    if days == 1 {
        "1 day".into()
    } else if days > 0 {
        format!("{days} days")
    } else {
        format!("{} hours", seconds / 3600)
    }
}

/// Rows of the AI VARIANTS section: the retention policy for offered,
/// unaccepted AI variants, its current state and the last automatic pass.
pub(super) fn retention_rows(
    report: &deadpan_store::generation_retention::VariantRetentionReport,
    grace_seconds: u64,
    pass: Option<&crate::project::RetentionPassState>,
    now: std::time::SystemTime,
) -> Vec<(String, String)> {
    let mut rows = vec![(
        "Retention".to_owned(),
        format!(
            "variants you have not kept, chosen or accepted stop being offered {} after they were generated; their files go at least {} later",
            days(report.retention_seconds),
            days(grace_seconds)
        ),
    )];
    let mut offered = format!("{} offered", report.offered);
    if report.kept > 0 {
        offered.push_str(&format!(" · {} kept", report.kept));
    }
    if report.picked > 0 {
        offered.push_str(&format!(" · {} picked", report.picked));
    }
    if report.selected > 0 {
        offered.push_str(&format!(" · {} chosen", report.selected));
    }
    if report.accepted > 0 {
        offered.push_str(&format!(" · {} accepted", report.accepted));
    }
    rows.push(("Variants".to_owned(), offered));
    let expiring = match report.soonest_expiry_unix_seconds {
        _ if report.expiring == 0 => "none".to_owned(),
        _ if report.due > 0 => format!(
            "{} of {} due now ({}), at the next automatic check",
            report.due,
            report.expiring,
            bytes(report.due_bytes)
        ),
        Some(at) => {
            let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(at);
            let left = at.duration_since(now).unwrap_or_default().as_secs();
            let when = if left >= 24 * 60 * 60 {
                format!("in {}", days(left))
            } else {
                format!("in {} hours", left / 3600)
            };
            format!("{} · the first {when}", report.expiring)
        }
        None => report.expiring.to_string(),
    };
    rows.push(("Expiring".to_owned(), expiring));
    rows.push((
        "No longer offered".to_owned(),
        format!(
            "{} discarded · {} expired · {} awaiting cleanup ({})",
            report.discarded,
            report.expired,
            report.evicted_awaiting_cleanup,
            bytes(report.evicted_awaiting_cleanup_bytes)
        ),
    ));
    if let Some(anomaly) = &report.clock_anomaly {
        rows.push(("Clock".to_owned(), clock_row(anomaly)));
    }
    use crate::project::RetentionPassState;
    let pass = match pass {
        None => "not run yet in this session; runs when idle after opening, then every 6 hours"
            .to_owned(),
        Some(RetentionPassState::ClockAnomaly(text)) => format!("skipped. {text}"),
        Some(RetentionPassState::Deferred(reason)) => format!("deferred. {reason}"),
        Some(RetentionPassState::Running) => "running…".to_owned(),
        Some(RetentionPassState::Done {
            expired,
            removed_files,
            removed_bytes,
            kept_files,
            ..
        }) => {
            let mut text = format!(
                "expired {expired} variants, removed {removed_files} files ({})",
                bytes(*removed_bytes)
            );
            if *kept_files > 0 {
                text.push_str(&format!("; {kept_files} in use or changed were kept"));
            }
            text
        }
        Some(RetentionPassState::Failed(error)) => format!("failed: {error}"),
    };
    rows.push(("Automatic check".to_owned(), pass));
    rows
}

/// The Clock row: a long gap can be confirmed with E; a clock behind the
/// project's records cannot.
pub(super) fn clock_row(anomaly: &ClockAnomaly) -> String {
    match anomaly {
        ClockAnomaly::Behind { .. } => "earlier than times this project recorded; nothing expires until the date and time are right".into(),
        ClockAnomaly::Ahead {
            now_unix_ms,
            last_seen_unix_ms,
        } => format!(
            "long gap since the last check ({}); automatic expiry waits. E reviews and confirms the clock",
            days(u64::try_from(now_unix_ms.saturating_sub(*last_seen_unix_ms) / 1000).unwrap_or(0))
        ),
    }
}

fn expiry_summary(expiry: &VariantExpiry) -> String {
    format!(
        "{} AI variant{} ({})",
        expiry.expired.len(),
        if expiry.expired.len() == 1 { "" } else { "s" },
        bytes(expiry.expired_bytes)
    )
}

fn summary(outcome: &CleanupOutcome) -> String {
    let mut text = format!(
        "{} {} files ({})",
        if outcome.dry_run {
            "Cleanup would remove"
        } else {
            "Removed"
        },
        outcome.removed.len(),
        bytes(outcome.removed_bytes)
    );
    if !outcome.in_use.is_empty() {
        text.push_str(&format!("; {} in use were kept", outcome.in_use.len()));
    }
    if !outcome.changed.is_empty() {
        text.push_str(&format!(
            "; {} changed meanwhile were kept",
            outcome.changed.len()
        ));
    }
    text
}

impl DeadpanApp {
    pub(super) fn open_storage(&mut self, context: &egui::Context) {
        self.storage.backups.only = false;
        self.bindings.clear();
        self.storage.open = true;
        self.storage.focus_pending = true;
        self.storage.preview = None;
        self.refresh_storage();
        context.request_repaint();
    }

    fn close_storage(&mut self, context: &egui::Context) {
        self.storage.open = false;
        self.storage.focus_pending = false;
        self.storage.return_focus = Some((context.cumulative_frame_nr(), self.pane));
        context.request_discard("storage closed");
        context.request_repaint();
    }

    /// Recompute both reports on a background thread.
    fn refresh_storage(&mut self) {
        let package = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone());
        let user = self.storage.user();
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-storage-report".into())
            .spawn(move || {
                let project = package.map(|package| {
                    deadpan_store::ProjectStore::open(&package, deadpan_store::AccessMode::ReadOnly)
                        .and_then(|store| store.storage_report(DEFAULT_GRACE))
                        .map_err(|error| error.to_string())
                });
                let user = user.map(|user| user.report(DEFAULT_GRACE));
                let _ = sender.send(Snapshot { project, user });
            });
        match spawned {
            Ok(_) => self.storage.loading = Some(receiver),
            Err(error) => self.storage.status = Some(format!("Storage report failed: {error}")),
        }
        self.refresh_backups();
    }

    /// Poll background work. Call once per outer frame.
    pub(super) fn reconcile_storage(&mut self, context: &egui::Context) {
        self.reconcile_backups(context);
        // A preview describes one session and revision; any change, such as
        // an edit, Undo or another project, discards it.
        let current = self.current_context();
        self.reconcile_clock(current.as_ref(), context);
        if self
            .storage
            .preview
            .as_ref()
            .is_some_and(|(session, revision, _)| {
                current.as_ref() != Some(&(*session, revision.clone()))
            })
        {
            self.storage.preview = None;
            if self.storage.open {
                self.storage.status =
                    Some("The project changed; preview the cleanup again with P.".into());
            }
        }
        if let Some((session, revision, receiver)) = &self.storage.previewing {
            match receiver.try_recv() {
                Ok(result) => {
                    let (session, revision) = (*session, revision.clone());
                    self.storage.previewing = None;
                    match result {
                        Ok(outcome) if current.as_ref() == Some(&(session, revision.clone())) => {
                            self.storage.status = Some(if outcome.removed.is_empty() {
                                "Nothing is removable now.".to_owned()
                            } else {
                                format!(
                                    "{}. R removes exactly these if they are still unreferenced.",
                                    summary(&outcome)
                                )
                            });
                            self.storage.preview = Some((session, revision, outcome));
                        }
                        Ok(_) => {
                            self.storage.status = Some(
                                "The project changed during the preview; press P again.".into(),
                            );
                        }
                        Err(error) => self.storage.status = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.storage.previewing = None,
            }
        }
        if let Some(receiver) = &self.storage.loading {
            match receiver.try_recv() {
                Ok(snapshot) => {
                    self.storage.snapshot = Some(snapshot);
                    self.storage.loading = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.storage.loading = None,
            }
        }
        if let Some(receiver) = &self.storage.caches {
            match receiver.try_recv() {
                Ok(result) => {
                    self.storage.caches = None;
                    self.storage.status = Some(match result {
                        Ok(outcome) => format!(
                            "Removed {} cache entries ({}){}",
                            outcome.removed.len(),
                            bytes(outcome.removed_bytes),
                            if outcome.kept_in_use.is_empty() {
                                String::new()
                            } else {
                                format!("; {} in use were kept", outcome.kept_in_use.len())
                            }
                        ),
                        Err(error) => format!("Cache cleanup failed: {error}"),
                    });
                    self.refresh_storage();
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.storage.caches = None,
            }
        }
        if let Some(receiver) = &self.storage.copy {
            match receiver.try_recv() {
                Ok(result) => {
                    self.storage.copy = None;
                    let text = match result {
                        Ok(report) => format!(
                            "Saved a portable copy at {} ({}, verified)",
                            report.destination.display(),
                            bytes(report.total_bytes)
                        ),
                        Err(error) => format!("The portable copy failed: {error}"),
                    };
                    self.message = Some(text.clone());
                    self.storage.status = Some(text);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(200))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.storage.copy = None,
            }
        }
    }

    /// Admit this session's automatic retention pass status. A finished
    /// pass refreshes an open panel's report.
    pub(super) fn receive_storage_retention(
        &mut self,
        status: Option<crate::project::RetentionPassStatus>,
    ) {
        if status == self.storage.retention {
            return;
        }
        let finished = status.as_ref().is_some_and(|status| {
            matches!(
                status.state,
                crate::project::RetentionPassState::Done { .. }
            )
        });
        let confirmation = status
            .as_ref()
            .and_then(|status| status.confirmation.clone());
        self.storage.retention = status;
        if let Some(confirmation) = confirmation
            && matches!(self.storage.clock, Some(ClockStep::Pending(ticket)) if ticket == confirmation.ticket)
        {
            self.storage.clock = None;
            self.storage.status = Some(match confirmation.result {
                Ok(expiry) => format!(
                    "Confirmed the clock: {} stopped being offered; their files go after the grace period. Automatic checks resume.",
                    expiry_summary(&expiry)
                ),
                Err(error) => error,
            });
            self.refresh_storage();
        } else if finished && self.storage.open {
            self.refresh_storage();
        }
    }

    /// E: after a long gap since the last retention check, the first press
    /// plans an explicit expiry on a read-only open and shows what would
    /// stop being offered; the second confirms exactly that plan on the
    /// writer, which rechecks every row and records the watermark. A clock
    /// behind the project's records is shown, never confirmed.
    fn confirm_clock(&mut self) {
        let Some((session, revision)) = self.current_context() else {
            self.storage.status = Some("Open a project first.".into());
            return;
        };
        if let Some(reason) = self.workspace.as_ref().and_then(|w| w.read_only.clone()) {
            self.storage.status = Some(format!("Not confirmed: {reason}"));
            return;
        }
        match &self.storage.clock {
            Some(ClockStep::Pending(_) | ClockStep::Planning(..)) => return,
            Some(ClockStep::Ready(plan_session, plan_revision, _))
                if *plan_session == session && *plan_revision == revision =>
            {
                let Some(ClockStep::Ready(_, _, plan)) = self.storage.clock.take() else {
                    return;
                };
                self.storage.ticket += 1;
                let ticket = self.storage.ticket;
                match self
                    .service
                    .submit(crate::project::ProjectRequest::ConfirmVariantClock {
                        ticket,
                        expected_session: session,
                        expected_revision: deadpan_core::RevisionId::new(revision)
                            .expect("a workspace revision is valid"),
                        plan,
                    }) {
                    Ok(()) => {
                        self.storage.clock = Some(ClockStep::Pending(ticket));
                        self.storage.status = Some("Confirming the clock…".into());
                    }
                    Err(error) => self.storage.status = Some(error),
                }
                return;
            }
            _ => {}
        }
        let anomaly = match &self.storage.snapshot {
            Some(Snapshot {
                project: Some(Ok(report)),
                ..
            }) => report.variant_retention.clock_anomaly,
            _ => {
                self.storage.status = Some("Storage is still being measured.".into());
                return;
            }
        };
        match anomaly {
            None => {
                self.storage.status =
                    Some("The clock needs no confirmation: automatic checks trust it.".into());
                return;
            }
            Some(ClockAnomaly::Behind { .. }) => {
                self.storage.status = Some(
                    "This Mac's clock is earlier than times this project recorded. Correct the date and time; there is nothing to confirm.".into(),
                );
                return;
            }
            Some(ClockAnomaly::Ahead { .. }) => {}
        }
        let Some(package) = self.workspace.as_ref().map(|w| w.path.clone()) else {
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-clock-review".into())
            .spawn(move || {
                let _ = sender.send(
                    deadpan_store::ProjectStore::open(
                        &package,
                        deadpan_store::AccessMode::ReadOnly,
                    )
                    .and_then(|store| {
                        store.plan_generation_expiry(
                            std::time::SystemTime::now(),
                            deadpan_store::generation_retention::DEFAULT_VARIANT_RETENTION,
                            deadpan_store::generation_retention::ExpiryMode::Explicit,
                        )
                    })
                    .map_err(|error| error.to_string()),
                );
            });
        match spawned {
            Ok(_) => {
                self.storage.clock = Some(ClockStep::Planning(session, revision, receiver));
                self.storage.status =
                    Some("Finding which AI variants would stop being offered…".into());
            }
            Err(error) => self.storage.status = Some(error.to_string()),
        }
    }

    /// Poll the clock review and discard it when the project changes.
    fn reconcile_clock(&mut self, current: Option<&(u64, String)>, context: &egui::Context) {
        let stale =
            |session: &u64, revision: &String| current != Some(&(*session, revision.clone()));
        match &self.storage.clock {
            Some(ClockStep::Ready(session, revision, _)) if stale(session, revision) => {
                self.storage.clock = None;
                if self.storage.open {
                    self.storage.status =
                        Some("The project changed; review the clock again with E.".into());
                }
            }
            Some(ClockStep::Planning(session, revision, receiver)) => {
                let stale = stale(session, revision);
                match receiver.try_recv() {
                    Ok(result) => {
                        let (session, revision) = (*session, revision.clone());
                        self.storage.clock = None;
                        match result {
                            Ok(_) if stale => {
                                self.storage.status = Some(
                                    "The project changed during the review; press E again.".into(),
                                );
                            }
                            Ok(plan) => {
                                self.storage.status = Some(format!(
                                    "Confirm this Mac's clock? {} would stop being offered now; their files go after the normal grace period. Automatic checks then resume. Press E again to confirm.",
                                    expiry_summary(&plan.preview())
                                ));
                                self.storage.clock =
                                    Some(ClockStep::Ready(session, revision, Box::new(plan)));
                            }
                            Err(error) => self.storage.status = Some(error),
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        context.request_repaint_after(std::time::Duration::from_millis(100))
                    }
                    Err(mpsc::TryRecvError::Disconnected) => self.storage.clock = None,
                }
            }
            _ => {}
        }
    }

    /// Admit the project service's reply to this panel's removal request.
    pub(super) fn receive_storage_cleanup(
        &mut self,
        status: Option<crate::project::StorageCleanupStatus>,
    ) {
        let Some(status) = status else { return };
        if self.storage.pending != Some(status.ticket) {
            return;
        }
        self.storage.pending = None;
        self.storage.preview = None;
        match status.result {
            Ok(outcome) => {
                self.storage.status = Some(summary(&outcome));
                // A reply from an earlier project leaves the new one's report.
                if self.current_context().map(|(session, _)| session) == Some(status.session) {
                    self.refresh_storage();
                }
            }
            Err(error) => self.storage.status = Some(error),
        }
    }

    fn current_context(&self) -> Option<(u64, String)> {
        self.workspace.as_ref().map(|workspace| {
            (
                workspace.session,
                workspace.document.revision_id().as_str().to_owned(),
            )
        })
    }

    /// P: list what cleanup would remove, on a read-only open in a worker
    /// thread, never on the project writer.
    fn preview_storage_cleanup(&mut self) {
        let Some((session, revision)) = self.current_context() else {
            self.storage.status = Some("Open a project to clean its storage.".into());
            return;
        };
        if self.storage.previewing.is_some() || self.storage.pending.is_some() {
            return;
        }
        let Some(package) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone())
        else {
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-storage-preview".into())
            .spawn(move || {
                let _ = sender.send(
                    deadpan_store::ProjectStore::open(
                        &package,
                        deadpan_store::AccessMode::ReadOnly,
                    )
                    .and_then(|store| store.preview_storage_cleanup(DEFAULT_GRACE))
                    .map_err(|error| error.to_string()),
                );
            });
        match spawned {
            Ok(_) => {
                self.storage.preview = None;
                self.storage.previewing = Some((session, revision, receiver));
                self.storage.status = Some("Finding unreferenced files…".into());
            }
            Err(error) => self.storage.status = Some(error.to_string()),
        }
    }

    /// R: remove exactly the previewed entries, on the writer.
    fn confirm_storage_cleanup(&mut self) {
        let Some((session, revision)) = self.current_context() else {
            self.storage.status = Some("Open a project to clean its storage.".into());
            return;
        };
        if self.storage.pending.is_some() || self.storage.previewing.is_some() {
            return;
        }
        let previewed = match &self.storage.preview {
            Some((previewed_session, previewed_revision, outcome))
                if *previewed_session == session && *previewed_revision == revision =>
            {
                if outcome.removed.is_empty() {
                    self.storage.status = Some("Nothing is removable now.".into());
                    return;
                }
                outcome.removed.clone()
            }
            _ => {
                self.storage.status = Some("Preview the cleanup with P first.".into());
                return;
            }
        };
        self.storage.ticket += 1;
        let ticket = self.storage.ticket;
        match self
            .service
            .submit(crate::project::ProjectRequest::CleanStorage {
                ticket,
                expected_session: session,
                previewed,
            }) {
            Ok(()) => {
                self.storage.pending = Some(ticket);
                self.storage.status = Some("Removing the previewed files…".into());
            }
            Err(error) => self.storage.status = Some(error),
        }
    }

    fn clean_user_caches(&mut self) {
        if self.storage.caches.is_some() {
            return;
        }
        let Some(user) = self.storage.user() else {
            self.storage.status = Some("No home directory is available.".into());
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-cache-cleanup".into())
            .spawn(move || {
                let _ = sender.send(
                    user.clean(DEFAULT_GRACE, false, &[])
                        .map_err(|error| error.to_string()),
                );
            });
        match spawned {
            Ok(_) => {
                self.storage.caches = Some(receiver);
                self.storage.status = Some("Cleaning caches…".into());
            }
            Err(error) => self.storage.status = Some(error.to_string()),
        }
    }

    /// File › Save Portable Copy…: choose where the copy goes.
    pub(super) fn start_portable_copy(&mut self, context: &egui::Context) {
        if self.workspace.is_none() {
            self.message = Some("Open a project to save a portable copy.".into());
            return;
        }
        if self.storage.copy.is_some() {
            self.message = Some("A portable copy is already being saved.".into());
            return;
        }
        if let Err(error) = self.dialogs.start(DialogKind::PortableCopy, context) {
            self.message = Some(error);
        }
    }

    /// Copy the open project to the chosen location on a background thread.
    /// The source is read through a read-only open, so editing continues.
    pub(super) fn receive_portable_copy_dialog(&mut self, path: PathBuf) {
        let Some(source) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone())
        else {
            return;
        };
        let destination = if path
            .extension()
            .is_some_and(|extension| extension == "deadpan")
        {
            path
        } else {
            path.with_extension("deadpan")
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("deadpan-portable-copy".into())
            .spawn(move || {
                let _ = sender.send(
                    deadpan_store::portable::copy_portable(
                        &source,
                        &destination,
                        &std::sync::atomic::AtomicBool::new(false),
                    )
                    .map_err(|error| error.to_string()),
                );
            });
        match spawned {
            Ok(_) => {
                self.storage.copy = Some(receiver);
                self.message = Some("Saving a portable copy…".into());
                self.storage.status = Some("Saving a portable copy…".into());
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }

    /// The panel owns the keyboard while open: editor bindings never see
    /// keys, Tab and Space/Enter stay native, Escape closes it and P, R, C,
    /// S and U are its actions.
    pub(super) fn storage_keyboard(&mut self, context: &egui::Context) -> bool {
        if let Some((frame, pane)) = self.storage.return_focus
            && context.cumulative_frame_nr() > frame
        {
            context.memory_mut(|memory| memory.request_focus(pane_id(pane)));
            self.storage.return_focus = None;
        }
        if !self.storage.open {
            return false;
        }
        let composing = &mut self.ime_composing;
        context.input(|input| help_scroll::observe_composition(&input.events, composing));
        self.bindings.clear();
        if !self.ime_composing {
            use crate::navigation::panels::{StorageKey, storage_key};
            // Keys are read by their typed character, as in Jobs, so a
            // non-Latin layout reaches them at their positions. The first
            // fresh press in the batch acts; held repeats never act, and every
            // registered press is consumed with its companion text.
            let mut pressed = None;
            context.input_mut(|input| {
                let mut events = std::mem::take(&mut input.events).into_iter().peekable();
                while let Some(event) = events.next() {
                    if let egui::Event::Key {
                        key,
                        modifiers,
                        pressed: true,
                        repeat,
                        ..
                    } = &event
                        && let Some((key, modifiers)) = crate::navigation::mode_key(
                            *key,
                            *modifiers,
                            super::editor_input::companion_text(*key, events.peek()),
                        )
                        && let Some(action) = storage_key(key, modifiers)
                    {
                        if !repeat && pressed.is_none() {
                            pressed = Some(action);
                        }
                        if matches!(events.peek(), Some(egui::Event::Text(_))) {
                            events.next();
                        }
                        continue;
                    }
                    input.events.push(event);
                }
            });
            // `:backups` shows only the backups; cleanup keys wait for
            // `:storage`, where their controls are.
            if self.storage.backups.only
                && matches!(
                    pressed,
                    Some(
                        StorageKey::Preview
                            | StorageKey::Remove
                            | StorageKey::CleanCaches
                            | StorageKey::PortableCopy
                            | StorageKey::ConfirmClock
                    )
                )
            {
                pressed = None;
            }
            match pressed {
                Some(StorageKey::Preview) => self.preview_storage_cleanup(),
                Some(StorageKey::Remove) => self.confirm_storage_cleanup(),
                Some(StorageKey::CleanCaches) => self.clean_user_caches(),
                Some(StorageKey::PortableCopy) => self.start_portable_copy(context),
                Some(StorageKey::Refresh) => self.refresh_storage(),
                Some(StorageKey::BackUp) => self.back_up_now(),
                Some(StorageKey::NextBackup) => self.move_backup_selection(true),
                Some(StorageKey::PreviousBackup) => self.move_backup_selection(false),
                Some(StorageKey::Restore) => self.restore_selected_backup(),
                Some(StorageKey::ConfirmClock) => self.confirm_clock(),
                None => {}
            }
        }
        true
    }

    pub(super) fn storage_window(&mut self, context: &egui::Context) {
        if !self.storage.open {
            return;
        }
        let content = context.content_rect();
        let width = (content.width() - 32.0).clamp(300.0, 440.0);
        let height = (content.height() - 270.0).max(160.0);
        let mut close = false;
        let mut action = None;
        let project = self.workspace.is_some();
        let previewed = self
            .storage
            .preview
            .as_ref()
            .is_some_and(|(_, _, outcome)| !outcome.removed.is_empty());
        let focus = std::mem::take(&mut self.storage.focus_pending);
        let mut backup_action = None;
        let long_gap = matches!(
            &self.storage.snapshot,
            Some(Snapshot {
                project: Some(Ok(report)),
                ..
            }) if matches!(report.variant_retention.clock_anomaly, Some(ClockAnomaly::Ahead { .. }))
        );
        let modal = egui::Modal::new(egui::Id::new("storage-window"))
            .backdrop_color(egui::Color32::TRANSPARENT)
            .area(
                egui::Modal::default_area(egui::Id::new("storage-window"))
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 56.0)),
            )
            .show(context, |ui| {
                accessibility::dialog(ui, "Storage");
                ui.set_width(width);
                ui.label(style::section_title("STORAGE", true));
                ui.label(
                    egui::RichText::new(if self.storage.backups.only {
                        "Verified copies of this project's saved state, newest first. Media stays in the project and is kept while a backup names it."
                    } else {
                        "What this project and Deadpan's caches use. P lists files no retained revision, register, checkpoint or offered AI variant references and that have been unchanged for a day; R removes exactly those."
                    })
                    .size(11.5)
                    .weak(),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("storage-rows")
                    .max_height(height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        // `:backups` shows the backups alone.
                        if self.storage.backups.only {
                            backup_action = self.backups_section(ui, focus);
                            return;
                        }
                        let Some(snapshot) = &self.storage.snapshot else {
                            ui.label(egui::RichText::new("Measuring…").weak());
                            backup_action = self.backups_section(ui, false);
                            return;
                        };
                        let section = |ui: &mut egui::Ui, title: &str, rows: Vec<(String, String)>| {
                            ui.label(style::section_title(title, false));
                            egui::Grid::new(("storage-section", title.to_owned()))
                                .num_columns(2)
                                .min_col_width(120.0)
                                .spacing(egui::vec2(10.0, 3.0))
                                .min_row_height(16.0)
                                .show(ui, |ui| {
                                    for (label, value) in &rows {
                                        ui.label(egui::RichText::new(label).size(12.0).weak());
                                        let response = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(value).monospace().size(11.5),
                                            )
                                            .wrap(),
                                        );
                                        accessibility::full_text(response, &format!("{label}: {value}"));
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(6.0);
                        };
                        match &snapshot.project {
                            Some(Ok(report)) => {
                                section(ui, "THIS PROJECT", project_rows(report));
                                section(
                                    ui,
                                    "AI VARIANTS",
                                    retention_rows(
                                        &report.variant_retention,
                                        report.grace_seconds,
                                        self.storage
                                            .retention
                                            .as_ref()
                                            .map(|status| &status.state),
                                        std::time::SystemTime::now(),
                                    ),
                                );
                            }
                            Some(Err(error)) => section(
                                ui,
                                "THIS PROJECT",
                                vec![("Unavailable".into(), error.clone())],
                            ),
                            None => section(
                                ui,
                                "THIS PROJECT",
                                vec![("No project".into(), "Open one to see its storage".into())],
                            ),
                        }
                        if let Some(user) = &snapshot.user {
                            let mut rows: Vec<(String, String)> = user
                                .directories
                                .iter()
                                .map(|directory| {
                                    let mut value = format!("{} · {}", bytes(directory.bytes), directory.kind);
                                    if directory.removable_bytes > 0 {
                                        value.push_str(&format!(
                                            " · {} removable",
                                            bytes(directory.removable_bytes)
                                        ));
                                    }
                                    (directory.name.clone(), value)
                                })
                                .collect();
                            rows.push((
                                "Not on disk".into(),
                                "decoded audio, pictures and thumbnails stay in memory".into(),
                            ));
                            section(ui, "DEADPAN CACHES", rows);
                        }
                        backup_action = self.backups_section(ui, false);
                    });
                if let Some(status) = &self.storage.status {
                    let response = ui.add(egui::Label::new(egui::RichText::new(status).size(12.0)).wrap());
                    accessibility::full_text(response, status);
                }
                ui.add_space(6.0);
                let only = self.storage.backups.only;
                ui.horizontal_wrapped(|ui| {
                    if only {
                        // `:backups`: its own actions are in the section.
                        if ui.add(style::action("Close", "Esc")).clicked() {
                            close = true;
                        }
                        return;
                    }
                    let preview = ui.add_enabled(project, style::action("Preview cleanup", "P"));
                    if focus {
                        preview.request_focus();
                    }
                    if preview.clicked() {
                        action = Some('p');
                    }
                    if ui.add_enabled(previewed, style::action("Remove", "R")).clicked() {
                        action = Some('r');
                    }
                    if ui.add(style::action("Clean caches", "C")).clicked() {
                        action = Some('c');
                    }
                    if long_gap && ui.add(style::action("Confirm clock", "E")).clicked() {
                        action = Some('e');
                    }
                    if ui
                        .add_enabled(project && !self.storage.copying(), style::action("Save portable copy…", "S"))
                        .clicked()
                    {
                        action = Some('s');
                    }
                    if ui.add(style::action("Close", "Esc")).clicked() {
                        close = true;
                    }
                });
            });
        match action.or(backup_action) {
            Some('b') => self.back_up_now(),
            Some('o') => self.restore_selected_backup(),
            Some('p') => self.preview_storage_cleanup(),
            Some('r') => self.confirm_storage_cleanup(),
            Some('c') => self.clean_user_caches(),
            Some('e') => self.confirm_clock(),
            Some('s') => self.start_portable_copy(context),
            _ => {}
        }
        close |= modal.should_close() && !self.ime_composing;
        if close {
            self.close_storage(context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_rows_state_the_policy_and_the_last_pass() {
        let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let report = deadpan_store::generation_retention::VariantRetentionReport {
            retention_seconds: 7 * 24 * 3600,
            offered: 3,
            kept: 1,
            selected: 1,
            expiring: 1,
            soonest_expiry_unix_seconds: Some(1_000_000 + 2 * 24 * 3600),
            discarded: 2,
            expired: 1,
            evicted_awaiting_cleanup: 1,
            evicted_awaiting_cleanup_bytes: 2048,
            ..Default::default()
        };
        let pass = crate::project::RetentionPassState::Done {
            expired: 1,
            removed_files: 3,
            removed_bytes: 4096,
            kept_files: 0,
            finished: now,
        };
        let rows = retention_rows(&report, 24 * 3600, Some(&pass), now);
        let text: Vec<String> = rows
            .iter()
            .map(|(label, value)| format!("{label}: {value}"))
            .collect();
        assert_eq!(
            text,
            vec![
                "Retention: variants you have not kept, chosen or accepted stop being offered 7 days after they were generated; their files go at least 1 day later",
                "Variants: 3 offered · 1 kept · 1 chosen",
                "Expiring: 1 · the first in 2 days",
                "No longer offered: 2 discarded · 1 expired · 1 awaiting cleanup (2.0 KiB)",
                "Automatic check: expired 1 variants, removed 3 files (4.0 KiB)",
            ]
        );
        let anomaly = deadpan_store::generation_retention::VariantRetentionReport {
            expiring: 2,
            due: 1,
            due_bytes: 1024,
            clock_anomaly: Some(deadpan_store::generation_retention::ClockAnomaly::Ahead {
                now_unix_ms: 0,
                last_seen_unix_ms: 0,
            }),
            ..Default::default()
        };
        let rows = retention_rows(&anomaly, 3600, None, now);
        assert_eq!(
            rows[2].1,
            "1 of 2 due now (1.0 KiB), at the next automatic check"
        );
        assert_eq!(rows[4].0, "Clock");
        let deferred = crate::project::RetentionPassState::Deferred(
            "Waiting for the AI pause to finish.".into(),
        );
        let rows = retention_rows(&Default::default(), 3600, Some(&deferred), now);
        assert_eq!(rows[2].1, "none");
        assert_eq!(rows[4].1, "deferred. Waiting for the AI pause to finish.");
    }

    #[test]
    fn sizes_and_cleanup_summaries_read_plainly() {
        assert_eq!(bytes(900), "900 B");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 MiB");
        let mut outcome = CleanupOutcome {
            dry_run: true,
            ..CleanupOutcome::default()
        };
        assert_eq!(summary(&outcome), "Cleanup would remove 0 files (0 B)");
        outcome.dry_run = false;
        outcome.removed_bytes = 2048;
        outcome
            .removed
            .push(deadpan_store::storage::RemovedEntry::for_test(
                "generated",
                "blake3-x",
                2048,
            ));
        outcome
            .in_use
            .push(deadpan_store::storage::RemovedEntry::for_test(
                "generated",
                "blake3-y",
                1,
            ));
        assert_eq!(
            summary(&outcome),
            "Removed 1 files (2.0 KiB); 1 in use were kept"
        );
    }
}
