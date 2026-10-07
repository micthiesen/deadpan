//! The Jobs panel: every background job of this project and of the app,
//! with its state (queued, running, paused, cancelling), progress, elapsed
//! time and Cancel, plus AI pause attempts an earlier session left
//! unfinished, with Retry and Discard (specification §18.3 to §18.5, DP-18).
//!
//! `:jobs` or Deadpan › Jobs… opens it. While it is open it owns the
//! keyboard: Up/Down (or J/K) choose a row, X cancels the chosen job, R
//! retries and D discards the chosen interrupted attempt, Escape closes.
//! Rows come from the shared coordinator (`crate::jobs`), so what the panel
//! shows is exactly what admission, queueing and yielding act on.

use crate::jobs::{CancelOutcome, JobHandle, JobId, JobKind, JobRow, JobSpec, RowState};
use crate::project::generation::{Interrupted, Preparation};

use super::*;

#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    focus_pending: bool,
    return_focus: Option<(u64, Pane)>,
    /// The chosen row by identity, so a reordered list (a job admitted,
    /// finished or added) never moves an action to another row.
    selected: Option<RowKey>,
    /// Registrations the UI holds for app-wide work whose threads report
    /// through their own state rather than through a handle.
    youtube: Option<JobHandle>,
    models: Option<JobHandle>,
    copy: Option<JobHandle>,
    /// The newest project session the UI has shown.
    last_session: Option<u64>,
    pub(super) status: Option<String>,
}

/// A row's stable identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RowKey {
    Job(JobId),
    /// Request and attempt of an interrupted AI attempt.
    Interrupted(String, String),
    Preparation(deadpan_store::generation_preparations::PreparationId),
}

/// One panel row.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Row {
    Job(JobRow),
    Interrupted(Interrupted),
    Preparation(Preparation),
}

impl Row {
    pub(super) fn key(&self) -> RowKey {
        match self {
            Self::Job(job) => RowKey::Job(job.id),
            Self::Preparation(item) => RowKey::Preparation(item.id.clone()),
            Self::Interrupted(item) => {
                RowKey::Interrupted(item.request.clone(), item.attempt.clone())
            }
        }
    }

    /// What the row's action buttons name, so each is distinguishable.
    fn subject(&self) -> String {
        match self {
            Self::Preparation(item) => format!("AI pictures for “{}”", item.label),
            Self::Job(job) if job.detail.is_empty() => job.kind.label().to_owned(),
            Self::Job(job) => format!("{} · {}", job.kind.label(), job.detail),
            Self::Interrupted(item) => match &item.pause {
                Some(label) => format!("interrupted AI pause pictures for “{label}”"),
                None => format!("interrupted AI pause pictures for pause {}", item.hold),
            },
        }
    }
}

/// The selected row's current position: the first row while nothing was
/// chosen, `None` once the chosen row has gone (an action then refuses
/// rather than act on whatever row moved into its place).
pub(super) fn position(rows: &[Row], selected: Option<&RowKey>) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    match selected {
        None => Some(0),
        Some(key) => rows.iter().position(|row| &row.key() == key),
    }
}

/// The key `delta` rows from the selection, clamped to the list; from the
/// first row when the chosen one has gone.
pub(super) fn step(rows: &[Row], selected: Option<&RowKey>, delta: isize) -> Option<RowKey> {
    if rows.is_empty() {
        return None;
    }
    let at = position(rows, selected).unwrap_or(0);
    let target = at.saturating_add_signed(delta).min(rows.len() - 1);
    Some(rows[target].key())
}

pub(super) fn duration(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

/// The complete accessible text of a job row.
pub(super) fn job_text(row: &JobRow, current: Option<u64>) -> String {
    let mut text = row.kind.label().to_owned();
    if !row.detail.is_empty() {
        text.push_str(&format!(" · {}", row.detail));
    }
    if row.session.is_some() && row.session != current {
        text.push_str(" (closing project)");
    }
    text.push_str(&format!(": {}", row.state.label()));
    let running = matches!(row.state, RowState::Running | RowState::Paused(_));
    if running && !row.progress.stage.is_empty() {
        text.push_str(&format!(" · {}", row.progress.stage));
        if let Some(fraction) = row.progress.fraction {
            text.push_str(&format!(" {:.0}%", fraction * 100.0));
        }
    }
    text.push_str(&format!(
        " · {} {}",
        duration(row.elapsed),
        if matches!(row.state, RowState::Queued { .. }) {
            "waiting"
        } else {
            "elapsed"
        }
    ));
    text
}

pub(super) fn interrupted_text(item: &Interrupted) -> String {
    match &item.pause {
        Some(label) => format!(
            "AI pause pictures for “{label}” stopped when Deadpan last closed. Retry generates a new variant; Discard removes this entry."
        ),
        None => format!(
            "AI pause pictures for pause {} stopped when Deadpan last closed; the pause is no longer in your edit. Discard removes this entry.",
            item.hold
        ),
    }
}

pub(super) fn preparation_text(item: &Preparation) -> String {
    use deadpan_store::generation_preparations::PreparationState;
    let state = match item.state {
        PreparationState::Queued => "Queued",
        PreparationState::Claimed => "Reading boundary pictures",
        PreparationState::Interrupted => "Interrupted; R retries",
        PreparationState::Unavailable => "Unavailable; R retries",
        PreparationState::Fulfilled => "Request recorded",
        PreparationState::Cancelled => "Discarded",
    };
    let mut text = format!(
        "AI pictures for “{}” · {} frames · {state}. D discards; timing stays saved.",
        item.label, item.frames
    );
    if let Some(reason) = &item.reason {
        text.push(' ');
        text.push_str(reason);
    }
    text
}

impl DeadpanApp {
    pub(super) fn open_jobs(&mut self, context: &egui::Context) {
        self.bindings.clear();
        self.jobs.open = true;
        self.jobs.focus_pending = true;
        self.jobs.status = None;
        // A fresh panel starts on its first row.
        self.jobs.selected = None;
        context.request_repaint();
    }

    fn close_jobs(&mut self, context: &egui::Context) {
        self.jobs.open = false;
        self.jobs.focus_pending = false;
        self.jobs.return_focus = Some((context.cumulative_frame_nr(), self.pane));
        context.request_discard("jobs closed");
        context.request_repaint();
    }

    /// Once per outer frame: tell the coordinator what the foreground is
    /// doing, keep UI-held registrations in step with their work, and
    /// release waiters of earlier project sessions.
    pub(super) fn reconcile_jobs(&mut self, context: &egui::Context) {
        let board = self.service.jobs().clone();
        board.set_foreground(crate::jobs::Foreground {
            playback: self.transport.is_some(),
        });
        // Jobs of sessions the UI has left; never one it has not seen yet.
        if let Some(session) = self.workspace.as_ref().map(|workspace| workspace.session) {
            self.jobs.last_session = Some(session);
            board.cancel_sessions_before(session);
        } else if let Some(left) = self.jobs.last_session {
            board.cancel_sessions_before(left.saturating_add(1));
        }
        // YouTube: only while it transfers, installs or cancels; a pending
        // confirmation is a question, not work.
        let youtube = match self.youtube.jobs.status() {
            crate::youtube::Status::Working(stage) => Some(match stage {
                crate::youtube::Stage::CheckingDownloader => {
                    ("Checking the downloader".to_owned(), None)
                }
                crate::youtube::Stage::FetchingDetails => ("Fetching details".to_owned(), None),
                crate::youtube::Stage::Downloading {
                    downloaded,
                    estimated,
                } => (
                    "Downloading".to_owned(),
                    estimated
                        .filter(|total| *total > 0)
                        .map(|total| *downloaded as f32 / total as f32),
                ),
                crate::youtube::Stage::Assembling => ("Assembling".to_owned(), None),
                crate::youtube::Stage::Qualifying => ("Checking the video".to_owned(), None),
            }),
            crate::youtube::Status::Installing(progress) => Some((
                format!("Installing {}", progress.helper),
                (progress.total > 0).then(|| progress.completed as f32 / progress.total as f32),
            )),
            crate::youtube::Status::Cancelling => Some(("Cancelling".to_owned(), None)),
            _ => None,
        };
        sync_registration(
            &board,
            &mut self.jobs.youtube,
            JobKind::YoutubeImport,
            youtube,
        );
        let models = self.models.manager.job().map(|job| {
            (
                format!("{} · {}", job.pack_id, job.label()),
                Some(job.fraction()),
            )
        });
        sync_registration(&board, &mut self.jobs.models, JobKind::ModelPack, models);
        let copy = self
            .storage
            .copying()
            .then(|| ("Copying and verifying".to_owned(), None));
        sync_registration(&board, &mut self.jobs.copy, JobKind::PortableCopy, copy);
        if self.jobs.open {
            // Elapsed times advance while the panel is visible.
            context.request_repaint_after(Duration::from_millis(500));
        }
    }

    /// Jobs of this project and app-wide ones, then interrupted attempts.
    pub(super) fn job_rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = self
            .service
            .jobs()
            .snapshot()
            .into_iter()
            .map(Row::Job)
            .collect();
        rows.extend(self.ai_preparations().into_iter().map(Row::Preparation));
        rows.extend(self.ai_interrupted().into_iter().map(Row::Interrupted));
        rows
    }

    fn cancel_job(&mut self, row: &JobRow) {
        let board = self.service.jobs().clone();
        let label = row.kind.label();
        self.jobs.status = Some(match board.cancel(row.id) {
            CancelOutcome::Gone => "That job has already finished.".into(),
            CancelOutcome::NotCancellable(kind) => {
                format!(
                    "{} cannot be cancelled; it finishes on its own.",
                    kind.label()
                )
            }
            // The job's own cooperative flag is set. Kinds whose cancel is
            // also a request get it, so their own phase and receipts stay
            // exact; the flag alone already stops them.
            CancelOutcome::Signalled(kind, _) => {
                match kind {
                    JobKind::AiPause => {
                        let _ = self.ai_cancel_running();
                    }
                    JobKind::Tracking => self.track_cancel(),
                    _ => {}
                }
                format!("Cancelling {label}…")
            }
            // Owned by a request: shown as cancelling only once accepted.
            CancelOutcome::NeedsRequest(kind, _) => {
                let accepted = match kind {
                    JobKind::Render => self.cancel_running_render(),
                    JobKind::YoutubeImport => {
                        let busy = self.youtube.jobs.status().busy();
                        self.youtube.jobs.cancel();
                        busy
                    }
                    JobKind::ModelPack => {
                        let running = self.models.manager.job().is_some_and(|job| !job.cancelling);
                        self.models.manager.cancel();
                        running
                    }
                    _ => false,
                };
                if accepted {
                    board.mark_cancelling(row.id);
                    format!("Cancelling {label}…")
                } else {
                    format!(
                        "{label} could not be cancelled now: {}",
                        self.render
                            .error()
                            .map(str::to_owned)
                            .filter(|_| kind == JobKind::Render)
                            .unwrap_or_else(|| "it is already finishing or cancelling.".into())
                    )
                }
            }
        });
    }

    fn retry_interrupted(&mut self, item: &Interrupted) {
        let result = match &item.pause {
            Some(_) => self.ai_retry_interrupted(item),
            None => Err("That pause is no longer in your edit; Discard removes the entry.".into()),
        };
        self.jobs.status = Some(match result {
            Ok(()) => "Generating a new variant for that pause…".into(),
            Err(error) => error,
        });
    }

    fn discard_interrupted(&mut self, item: &Interrupted) {
        if let Err(error) = self.ai_dismiss_interrupted(item) {
            self.jobs.status = Some(error);
        } else {
            self.jobs.status = Some("Discarding the interrupted attempt…".into());
        }
    }

    /// Act on the selected row, resolved by identity in the current list.
    fn act_on_row(&mut self, action: char) {
        let rows = self.job_rows();
        let Some(row) = position(&rows, self.jobs.selected.as_ref()).map(|at| rows[at].clone())
        else {
            self.jobs.status =
                Some("The chosen row has already finished. Choose another with J or K.".into());
            return;
        };
        self.jobs.selected = Some(row.key());
        match (action, row) {
            ('r' | 'd', Row::Preparation(item)) => {
                self.jobs.status = Some(match self.ai_preparation_action(&item, action == 'r') {
                    Ok(()) if action == 'r' => "Queued the AI retry.".into(),
                    Ok(()) => "Discard requested; saved timing is unchanged.".into(),
                    Err(error) => error,
                });
            }
            ('x', Row::Preparation(_)) => {
                self.jobs.status = Some("D discards an AI preparation. R retries an unavailable or interrupted preparation.".into());
            }
            ('x', Row::Job(job)) => self.cancel_job(&job),
            ('r', Row::Interrupted(item)) => self.retry_interrupted(&item),
            ('d', Row::Interrupted(item)) => self.discard_interrupted(&item),
            ('x', Row::Interrupted(_)) => {
                self.jobs.status =
                    Some("This attempt is not running. R retries it, D discards it.".into());
            }
            (_, Row::Job(_)) => {
                self.jobs.status = Some(
                    "X cancels a job; R and D act on interrupted attempts and AI preparations."
                        .into(),
                );
            }
            _ => {}
        }
    }

    /// The panel owns the keyboard while open.
    pub(super) fn jobs_keyboard(&mut self, context: &egui::Context) -> bool {
        if let Some((frame, pane)) = self.jobs.return_focus
            && context.cumulative_frame_nr() > frame
        {
            context.memory_mut(|memory| memory.request_focus(pane_id(pane)));
            self.jobs.return_focus = None;
        }
        if !self.jobs.open {
            return false;
        }
        let composing = &mut self.ime_composing;
        context.input(|input| help_scroll::observe_composition(&input.events, composing));
        self.bindings.clear();
        if self.ime_composing {
            return true;
        }
        // Keys are read by their typed character, like the other mode
        // routers (`navigation::mode_key`), so non-Latin layouts reach J, K,
        // X, R and D at their physical positions. One action per press: a
        // held key never repeats a cancel.
        let mut actions = Vec::new();
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
                    && let Some(action) =
                        crate::navigation::panels::jobs_key(key, modifiers).map(|action| {
                            use crate::navigation::panels::JobsKey;
                            match action {
                                JobsKey::Next => 'j',
                                JobsKey::Previous => 'k',
                                JobsKey::Cancel => 'x',
                                JobsKey::Retry => 'r',
                                JobsKey::Discard => 'd',
                            }
                        })
                {
                    if !repeat || matches!(action, 'j' | 'k') {
                        actions.push(action);
                    }
                    // Its companion text is the typed letter, not input.
                    if matches!(events.peek(), Some(egui::Event::Text(_))) {
                        events.next();
                    }
                    continue;
                }
                input.events.push(event);
            }
        });
        for action in actions {
            match action {
                'j' | 'k' => {
                    let rows = self.job_rows();
                    self.jobs.selected = step(
                        &rows,
                        self.jobs.selected.as_ref(),
                        if action == 'j' { 1 } else { -1 },
                    );
                }
                action => self.act_on_row(action),
            }
        }
        true
    }

    pub(super) fn jobs_window(&mut self, context: &egui::Context) {
        if !self.jobs.open {
            return;
        }
        let rows = self.job_rows();
        let selected_at = position(&rows, self.jobs.selected.as_ref());
        let current = self.workspace.as_ref().map(|workspace| workspace.session);
        let content = context.content_rect();
        let width = (content.width() - 32.0).clamp(300.0, 480.0);
        let height = (content.height() - 270.0).max(160.0);
        let mut close = false;
        let mut chosen: Option<(RowKey, char)> = None;
        let modal = egui::Modal::new(egui::Id::new("jobs-window"))
            .backdrop_color(egui::Color32::TRANSPARENT)
            .area(
                egui::Modal::default_area(egui::Id::new("jobs-window"))
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 56.0)),
            )
            .show(context, |ui| {
                accessibility::dialog(ui, "Jobs");
                ui.set_width(width);
                ui.label(style::section_title("JOBS", true));
                ui.label(
                    egui::RichText::new(
                        "Background work never delays editing or playback. One AI model runs at a time; others wait their turn. Shot detection and seek proxies pause while the edit plays, a render runs or tracking runs.",
                    )
                    .size(11.5)
                    .weak(),
                );
                ui.add_space(4.0);
                let mut first = None;
                egui::ScrollArea::vertical()
                    .id_salt("jobs-rows")
                    .max_height(height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let jobs = rows.iter().filter(|row| matches!(row, Row::Job(_))).count();
                        if jobs == 0 {
                            let response = ui.label(egui::RichText::new("Nothing is running in the background.").weak());
                            accessibility::full_text(response, "Nothing is running in the background.");
                        }
                        let mut heading = false;
                        let mut preparation_heading = false;
                        for (index, row) in rows.iter().enumerate() {
                            let selected = Some(index) == selected_at;
                            let text = match row {
                                Row::Job(job) => job_text(job, current),
                                Row::Preparation(item) => {
                                    if !preparation_heading {
                                        preparation_heading = true;
                                        ui.add_space(6.0);
                                        ui.label(style::section_title("AI PREPARATIONS", false));
                                    }
                                    preparation_text(item)
                                }
                                Row::Interrupted(item) => {
                                    if !heading {
                                        heading = true;
                                        ui.add_space(6.0);
                                        ui.label(style::section_title("INTERRUPTED", false));
                                    }
                                    interrupted_text(item)
                                }
                            };
                            ui.horizontal_wrapped(|ui| {
                                let response = ui.add(
                                    egui::Button::selectable(
                                        selected,
                                        egui::RichText::new(&text).size(12.0),
                                    )
                                    .wrap(),
                                );
                                let response = accessibility::full_text(response, &text);
                                if response.clicked() {
                                    chosen = Some((row.key(), ' '));
                                }
                                if selected {
                                    first.get_or_insert(response);
                                }
                                // Each action names its row for VoiceOver.
                                let subject = row.subject();
                                let mut action = |ui: &mut egui::Ui, enabled: bool, verb: &str, key: &str, code: char| {
                                    let button = ui.add_enabled(enabled, style::action(verb, key));
                                    accessibility::name(&button, &format!("{verb} {subject} ({key})"));
                                    if button.clicked() {
                                        chosen = Some((row.key(), code));
                                    }
                                };
                                match row {
                                    Row::Job(job) => action(
                                        ui,
                                        job.cancellable && job.state != RowState::Cancelling,
                                        "Cancel",
                                        "X",
                                        'x',
                                    ),
                                    Row::Preparation(item) => {
                                        action(ui, item.retryable(), "Retry", "R", 'r');
                                        action(ui, true, "Discard", "D", 'd');
                                    }
                                    Row::Interrupted(item) => {
                                        action(ui, item.pause.is_some(), "Retry", "R", 'r');
                                        action(ui, true, "Discard", "D", 'd');
                                    }
                                }
                            });
                        }
                    });
                if let Some(warning) = self.ai_interrupted_warning() {
                    let response = ui.add(egui::Label::new(egui::RichText::new(&warning).size(11.5).weak()).wrap());
                    accessibility::full_text(response, &warning);
                }
                if let Some(status) = &self.jobs.status {
                    let response = ui.add(egui::Label::new(egui::RichText::new(status).size(12.0)).wrap());
                    accessibility::full_text(response, status);
                }
                ui.add_space(6.0);
                let close_button = ui.add(style::action("Close", "Esc"));
                if close_button.clicked() {
                    close = true;
                }
                if std::mem::take(&mut self.jobs.focus_pending) {
                    first.unwrap_or(close_button).request_focus();
                }
            });
        if let Some((key, action)) = chosen {
            self.jobs.selected = Some(key);
            if action != ' ' {
                self.act_on_row(action);
            }
        }
        close |= modal.should_close() && !self.ime_composing;
        if close {
            self.close_jobs(context);
        }
    }
}

/// Register, update or release a UI-held registration.
fn sync_registration(
    board: &crate::jobs::Jobs,
    slot: &mut Option<JobHandle>,
    kind: JobKind,
    progress: Option<(String, Option<f32>)>,
) {
    match progress {
        Some((stage, fraction)) => {
            let handle = slot.get_or_insert_with(|| {
                let spec = JobSpec::new(kind, None);
                board.register(if kind == JobKind::PortableCopy {
                    spec.not_cancellable()
                } else {
                    spec
                })
            });
            handle.set_progress(stage, fraction);
        }
        None => *slot = None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobId, Progress};

    fn row(state: RowState, progress: Progress) -> JobRow {
        JobRow {
            id: JobId::for_test(1),
            kind: JobKind::AiPause,
            session: Some(2),
            detail: "pause hold-1".into(),
            state,
            progress,
            elapsed: Duration::from_secs(75),
            cancellable: true,
        }
    }

    #[test]
    fn rows_read_as_complete_sentences_for_voiceover() {
        assert_eq!(
            job_text(
                &row(
                    RowState::Running,
                    Progress {
                        stage: "Generating pictures".into(),
                        fraction: Some(0.25),
                    }
                ),
                Some(2)
            ),
            "AI pause pictures · pause hold-1: Running · Generating pictures 25% · 1:15 elapsed"
        );
        assert_eq!(
            job_text(
                &row(
                    RowState::Queued {
                        position: 1,
                        waiting_for: "Transcription".into()
                    },
                    Progress::default()
                ),
                Some(3)
            ),
            "AI pause pictures · pause hold-1 (closing project): Queued #1, waiting for Transcription · 1:15 waiting"
        );
        assert_eq!(duration(Duration::from_secs(3_725)), "1:02:05");
        let gone = Interrupted {
            request: "r".into(),
            attempt: "a".into(),
            hold: "hold-9".into(),
            target: None,
            pause: None,
        };
        assert!(interrupted_text(&gone).contains("no longer in your edit"));
    }

    #[test]
    fn the_selection_follows_its_job_when_rows_reorder() {
        let job = |id, kind| {
            Row::Job(JobRow {
                id: JobId::for_test(id),
                kind,
                ..row(RowState::Running, Progress::default())
            })
        };
        let interrupted = Row::Interrupted(Interrupted {
            request: "r".into(),
            attempt: "a".into(),
            hold: "h".into(),
            target: None,
            pause: Some("Pause".into()),
        });
        let before = vec![
            job(1, JobKind::Render),
            job(2, JobKind::AiPause),
            interrupted.clone(),
        ];
        // J from the default (first) row chooses the AI job by identity.
        let selected = step(&before, None, 1);
        assert_eq!(selected, Some(RowKey::Job(JobId::for_test(2))));
        // The render finished and a queued transcription was added above:
        // the same job is still chosen, now at another position.
        let after = vec![
            job(3, JobKind::Transcription),
            job(2, JobKind::AiPause),
            interrupted,
        ];
        assert_eq!(position(&after, selected.as_ref()), Some(1));
        assert_eq!(
            step(&after, selected.as_ref(), 5),
            Some(RowKey::Interrupted("r".into(), "a".into()))
        );
        assert_eq!(
            step(&after, selected.as_ref(), -5),
            Some(RowKey::Job(JobId::for_test(3)))
        );
        assert_eq!(position(&[], selected.as_ref()), None);
        // Once the chosen job has gone, nothing else is acted on in its
        // place; navigation restarts from the first row.
        let gone = Some(RowKey::Job(JobId::for_test(9)));
        assert_eq!(position(&after, gone.as_ref()), None);
        assert_eq!(
            step(&after, gone.as_ref(), 1),
            Some(RowKey::Job(JobId::for_test(2)))
        );
        assert_eq!(position(&after, None), Some(0));
    }
}
