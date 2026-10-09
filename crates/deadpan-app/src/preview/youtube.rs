//! Start a project from one YouTube URL.
//!
//! The start surface shows the URL field inline, as in the product board's
//! "Choose one Original" panel; over an open project the same steps appear in
//! a sheet (`⌘⇧N`, `:youtube`). Typing, paste and IME stay native. Enter runs
//! the current step's primary action and Escape cancels or leaves it. Details
//! are shown before any transfer, the download starts only on confirmation,
//! and the finished package opens through the ordinary Open path.

use std::path::PathBuf;

use deadpan_cli::youtube::url::normalize;

use super::*;
use crate::navigation::youtube::{UrlKey, route_key};
use crate::youtube::{self as job, Failure, Jobs, Request, Stage, Status};

pub(super) const URL_ID: &str = "youtube-url";

pub(super) struct Flow {
    pub(super) jobs: Jobs,
    pub(super) url: String,
    pub(super) cookies: Option<PathBuf>,
    /// The sheet over an open project. The start surface needs none.
    pub(super) modal: bool,
    focus_field: bool,
    key: Option<UrlKey>,
    /// A created package waiting to open, with its canonical path once the
    /// Open request was admitted, and when opening began.
    opening: Option<(PathBuf, Option<PathBuf>, std::time::Instant)>,
    /// An explicit service outcome, independent of its admission bit and UI mailbox.
    open_reply: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    /// The project session the visible step belongs to.
    session: Option<u64>,
    /// Title of the video being imported, for the opening message.
    title: Option<String>,
}

impl Flow {
    pub(super) fn new(jobs: Jobs) -> Self {
        Self {
            jobs,
            url: String::new(),
            cookies: None,
            modal: false,
            focus_field: false,
            key: None,
            opening: None,
            open_reply: None,
            session: None,
            title: None,
        }
    }

    /// Work exists that the user must see through or cancel.
    pub(super) fn active(&self) -> bool {
        self.jobs.status().busy() || self.opening.is_some()
    }

    /// Replay-visible step name.
    #[cfg(feature = "ui-harness")]
    pub(super) fn step_name(&self) -> &'static str {
        if self.opening.is_some() {
            return "opening";
        }
        match self.jobs.status() {
            Status::Idle => "idle",
            Status::Working(Stage::CheckingDownloader | Stage::FetchingDetails) => "details",
            Status::Working(Stage::Downloading { .. }) => "downloading",
            Status::Working(Stage::Assembling) => "assembling",
            Status::Working(Stage::Qualifying) => "qualifying",
            Status::Confirm { .. } => "confirm",
            Status::NeedsDownloader => "needs-downloader",
            Status::Installing(_) => "installing",
            Status::Cancelling => "cancelling",
            Status::Cancelled => "cancelled",
            Status::Failed(_) => "failed",
            Status::Created(_) => "created",
        }
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn failure_code(&self) -> Option<&str> {
        match self.jobs.status() {
            Status::Failed(failure) => Some(&failure.code),
            _ => None,
        }
    }
}

/// The URL check shown while typing: the normalized video or the exact refusal.
fn validation(url: &str) -> Option<Result<String, String>> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    Some(match normalize(url) {
        Ok(id) => Ok(id.as_str().to_owned()),
        Err(error) => Err(sentence(&error.to_string())),
    })
}

fn sentence(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => {
            let mut sentence: String = first.to_uppercase().chain(characters).collect();
            if !sentence.ends_with('.') {
                sentence.push('.');
            }
            sentence
        }
        None => String::new(),
    }
}

fn failure_title(failure: &Failure) -> &'static str {
    match failure.code.as_str() {
        "YouTubeUrlInvalid" => "Not a supported YouTube video link",
        "YouTubePlaylistNeedsVideo" | "YouTubePlaylistRefused" => "This link is a playlist",
        "DownloaderHelperInvalid" => "The downloader failed verification",
        "DownloaderInstallFailed" => "The downloader did not install",
        "DownloaderNetworkFailed" => "Cannot download the helper",
        "DownloaderUnsupportedPlatform" => "The downloader is not available on this Mac",
        "YouTubeVideoUnavailable" => "Video unavailable",
        "YouTubeVideoPrivate" => "This video is private",
        "YouTubeRegionRestricted" => "Not available in this region",
        "YouTubeAgeRestricted" => "This video is age-restricted",
        "YouTubeSignInRequired" => "YouTube asks you to sign in",
        "YouTubeRateLimited" => "YouTube is limiting requests",
        "YouTubeLiveUnsupported" => "Live streams cannot be imported",
        "YouTubeFormatUnavailable" => "No compatible streams",
        "YouTubeTooLong" | "YouTubeTooLarge" => "This video is too large",
        "YouTubeInsufficientSpace" => "Not enough disk space",
        "ProjectOpenFailed" => "The project was created but did not open",
        _ => "The import did not finish",
    }
}

impl DeadpanApp {
    /// `⌘⇧N` / `:youtube`: focus the URL field, in a sheet over open work.
    pub(super) fn open_youtube(&mut self, context: &egui::Context) {
        self.cancel_repeats("a file action was requested");
        if self.dialogs.is_open() {
            return;
        }
        // The finished project replaces this session through Open, which a
        // recording macro would refuse; the same guards as a local New apply.
        let refusal = if self.macros.recording() || self.macros.is_pending() {
            Some("Save or cancel macro recording before starting a project from YouTube.")
        } else if self.importing() {
            Some("Finish or cancel the current import before choosing another source.")
        } else if self.render.blocking() || self.render_workflow_active() {
            Some("Finish or cancel the render before starting a project from YouTube.")
        } else if self.ai.generation_running() {
            Some("Cancel the AI pause generation before starting a project from YouTube.")
        } else {
            None
        };
        if let Some(refusal) = refusal {
            self.error = Some(refusal.into());
            self.bindings.clear();
            return;
        }
        self.cancel_camera();
        self.pause_playback();
        if self.workspace.is_some() || self.raw_source.is_some() {
            self.youtube.modal = true;
        }
        if !self.youtube.active() {
            self.youtube.focus_field = true;
        }
        self.bindings.clear();
        context.request_repaint();
    }

    fn youtube_surface_inline(&self) -> bool {
        self.workspace.is_none() && self.raw_source.is_none()
    }

    /// The URL step owns the keyboard: its field is focused, its sheet is
    /// open or its work is running.
    pub(super) fn youtube_owns_keyboard(&self, context: &egui::Context) -> bool {
        self.youtube.modal
            || self.youtube.active()
            || context.memory(|memory| memory.has_focus(egui::Id::new(URL_ID)))
    }

    /// Route plain Enter/Escape to the import step; leave everything else,
    /// including text, paste, Tab and composition, to native widgets.
    pub(super) fn youtube_keyboard(&mut self, context: &egui::Context) -> bool {
        // An install offer or a refusal teaches Enter/Esc even when the field
        // is not focused. Then only those keys belong to the step; any other
        // batch keeps its ordinary editor routing, such as ⌘N or ⌘O.
        let soft = !self.youtube_owns_keyboard(context)
            && self.youtube_surface_visible()
            && matches!(
                self.youtube.jobs.status(),
                Status::NeedsDownloader | Status::Failed(_)
            );
        if !soft && !self.youtube_owns_keyboard(context) {
            return false;
        }
        let events = context.input(|input| input.events.clone());
        help_scroll::observe_composition(&events, &mut self.ime_composing);
        let ime = self.ime_composing
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)));
        if pointer_focus_transition(&events) {
            return true;
        }
        let field = context.memory(|memory| memory.has_focus(egui::Id::new(URL_ID)));
        let control = native_control_focused(context) && !field;
        for (index, event) in events.iter().enumerate() {
            let &egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                repeat,
                ..
            } = event
            else {
                continue;
            };
            let companion = match events.get(index + 1) {
                Some(egui::Event::Text(text)) => Some(text.as_str()),
                _ => None,
            };
            if let Some(routed) = navigation::mode_key(key, modifiers, companion)
                .and_then(|(key, modifiers)| route_key(key, modifiers, control, ime, repeat))
            {
                self.youtube.key = Some(routed);
                self.bindings.clear();
                context.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
                return true;
            }
        }
        if soft {
            return false;
        }
        // Command chords such as ⌘N, ⌘O, ⌘I and ⌘⇧N keep their ordinary
        // routing beside the field; the router leaves text chords (⌘A, ⌘V,
        // ⌘Z) to the focused field. Running work keeps the whole keyboard.
        let chord = events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key { pressed: true, modifiers, .. }
                    if modifiers.command || modifiers.mac_cmd
            )
        });
        if chord && !self.youtube.active() {
            return false;
        }
        self.bindings.clear();
        true
    }

    /// The footer teaches only the start or URL-step keys: on the empty start
    /// surface, and while the URL step owns the keyboard.
    pub(super) fn youtube_footer_shown(&self, context: &egui::Context) -> bool {
        self.youtube_surface_inline() || self.youtube_owns_keyboard(context)
    }

    /// Footer content identity, so a step change after input re-lays it out.
    pub(super) fn youtube_footer_key(&self, context: &egui::Context) -> Option<(u8, bool)> {
        if !self.youtube_footer_shown(context) {
            return None;
        }
        let field = context.memory(|memory| memory.has_focus(egui::Id::new(URL_ID)));
        let step = if self.youtube.opening.is_some() {
            9
        } else {
            match self.youtube.jobs.status() {
                Status::Idle => 0,
                Status::Working(_) => 1,
                Status::Confirm { .. } => 2,
                Status::NeedsDownloader => 3,
                Status::Installing(_) => 4,
                Status::Cancelling => 5,
                Status::Cancelled => 6,
                Status::Failed(_) => 7,
                Status::Created(_) => 9,
            }
        };
        Some((step, field))
    }

    pub(super) fn youtube_footer(&mut self, ui: &mut egui::Ui) {
        let field = ui
            .ctx()
            .memory(|memory| memory.has_focus(egui::Id::new(URL_ID)));
        let status = self.youtube.jobs.status().clone();
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(if field { "TEXT" } else { "START" })
                    .monospace()
                    .strong(),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(if self.youtube_surface_inline() {
                    "NO PROJECT"
                } else {
                    "NEW FROM YOUTUBE"
                })
                .monospace(),
            );
            ui.separator();
            let keys: Vec<(&str, &str)> =
                if self.youtube.opening.is_some() || matches!(status, Status::Created(_)) {
                    ui.weak("Opening the new project");
                    Vec::new()
                } else {
                    match status {
                        Status::Confirm { .. } => {
                            vec![("Enter", "download and create"), ("Esc", "cancel")]
                        }
                        Status::Working(_) | Status::Installing(_) => vec![("Esc", "cancel")],
                        Status::Created(_) => Vec::new(),
                        Status::Cancelling => {
                            ui.weak("Stopping");
                            Vec::new()
                        }
                        Status::NeedsDownloader => {
                            vec![("Enter", "install downloader"), ("Esc", "not now")]
                        }
                        Status::Failed(_) if field => vec![
                            ("Enter", "try again"),
                            ("Esc", "dismiss"),
                            ("⌘N", "choose video"),
                            ("⌘O", "open project"),
                        ],
                        Status::Failed(_) => vec![
                            ("Esc", "dismiss"),
                            ("⌘N", "choose video"),
                            ("⌘⇧N", "YouTube URL"),
                            ("⌘O", "open project"),
                        ],
                        Status::Idle | Status::Cancelled if field => vec![
                            ("Enter", "fetch details"),
                            (
                                "Esc",
                                if self.youtube.modal {
                                    "close"
                                } else {
                                    "leave field"
                                },
                            ),
                            ("⌘N", "choose video"),
                            ("⌘O", "open project"),
                        ],
                        Status::Idle | Status::Cancelled if self.youtube.modal => {
                            vec![("⌘⇧N", "YouTube URL"), ("Esc", "close")]
                        }
                        Status::Idle | Status::Cancelled => vec![
                            ("⌘N", "choose video"),
                            ("⌘⇧N", "YouTube URL"),
                            ("⌘O", "open project"),
                            (":", "command"),
                            ("?", "keys"),
                        ],
                    }
                };
            for (key, label) in keys {
                style::key_hint(ui, key, label);
            }
        });
    }

    /// Native menu key equivalents stay available beside an idle URL step.
    pub(super) fn youtube_blocks_menu(&self) -> bool {
        self.youtube.active() || self.youtube.jobs.status().busy()
    }

    /// A render workflow that has not finished and cleaned up.
    fn render_workflow_active(&self) -> bool {
        self.current_render().is_some_and(|workflow| {
            !workflow.status.cleanup_confirmed
                || !matches!(
                    workflow.status.stage,
                    deadpan_cli::encoded_render::workflow::WorkflowStage::Idle
                        | deadpan_cli::encoded_render::workflow::WorkflowStage::Finished
                )
        })
    }

    /// The URL step is on screen: the start card or the sheet.
    fn youtube_surface_visible(&self) -> bool {
        self.youtube_surface_inline() || self.youtube.modal
    }

    fn youtube_primary(&mut self, context: &egui::Context) {
        // A native picker (the cookies file) decides first.
        if self.dialogs.is_open() {
            return;
        }
        match self.youtube.jobs.status() {
            Status::Failed(failure) if failure.code == "ProjectOpenFailed" => {
                // The package exists; never download it again.
                self.begin_dialog(DialogKind::OpenProject, context, false);
            }
            Status::Idle | Status::Failed(_) | Status::Cancelled => {
                // The live check already shows a refusal; Enter never turns
                // an empty or unsupported URL into a job.
                if !matches!(validation(&self.youtube.url), Some(Ok(_))) {
                    self.youtube.focus_field = true;
                    return;
                }
                let url = self.youtube.url.trim().to_owned();
                let request = Request {
                    url,
                    cookies: self.youtube.cookies.clone(),
                };
                self.youtube.title = None;
                // Back at the field after a failure or cancellation.
                self.youtube.focus_field = true;
                if let Err(error) = self.youtube.jobs.start(request) {
                    self.error = Some(error);
                }
            }
            Status::Confirm { preview, .. } => {
                self.youtube.title = Some(preview.metadata.title.clone());
                self.youtube.jobs.confirm();
            }
            Status::NeedsDownloader => {
                if let Err(error) = self.youtube.jobs.install() {
                    self.error = Some(error);
                }
            }
            Status::Working(_)
            | Status::Installing(_)
            | Status::Cancelling
            | Status::Created(_) => {}
        }
    }

    fn youtube_cancel(&mut self, context: &egui::Context) {
        match self.youtube.jobs.status() {
            Status::Working(_) | Status::Confirm { .. } | Status::Installing(_) => {
                self.youtube.jobs.cancel();
            }
            Status::Cancelling | Status::Created(_) => {}
            Status::NeedsDownloader | Status::Failed(_) | Status::Cancelled => {
                self.youtube.jobs.dismiss();
                self.youtube.focus_field = true;
            }
            Status::Idle if self.youtube.opening.is_some() => {}
            Status::Idle => {
                self.youtube.modal = false;
                context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
            }
        }
    }

    /// Advance the job and open a created package once per outer frame.
    pub(super) fn reconcile_youtube(&mut self, context: &egui::Context) {
        /// The service normally admits Open within a frame or two; a refusal
        /// or a service that stays busy this long reports the package instead.
        const OPEN_WAIT: Duration = Duration::from_secs(60);
        if self.youtube.jobs.poll() {
            context.request_repaint();
        }
        // A finished failure or cancellation belongs to the session it was shown in.
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        if self.youtube.session != session {
            self.youtube.session = session;
            if self.youtube.opening.is_none()
                && matches!(
                    self.youtube.jobs.status(),
                    Status::Failed(_) | Status::Cancelled | Status::NeedsDownloader
                )
            {
                self.youtube.jobs.dismiss();
            }
        }
        // Closing never opens a package that finishes meanwhile; it stays
        // complete in the library under its own name.
        if self.close_pending {
            return;
        }
        if let Some(path) = self.youtube.jobs.take_created() {
            self.youtube.opening = Some((path, None, std::time::Instant::now()));
        }
        let Some((path, admitted, mut started)) = self.youtube.opening.clone() else {
            return;
        };
        let failed = |app: &mut Self, reason: String| {
            app.youtube.opening = None;
            app.youtube.open_reply = None;
            // Enter must not download it again.
            app.youtube.url.clear();
            app.youtube.jobs.fail(Failure::new(
                "ProjectOpenFailed",
                format!(
                    "{} is complete in your library but did not open: {reason}",
                    job::destination_label(&path)
                ),
            ));
        };
        match admitted {
            None => {
                if self.service.is_busy() || self.dialogs.is_open() {
                    if started.elapsed() >= OPEN_WAIT {
                        failed(self, "the project service stayed busy".into());
                    } else {
                        context.request_repaint_after(Duration::from_millis(16));
                    }
                    return;
                }
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                let (reply, result) = std::sync::mpsc::sync_channel(1);
                if self.submit(ProjectRequest::OpenReported {
                    path: path.clone(),
                    reply,
                }) {
                    self.youtube.opening = Some((path, Some(canonical), started));
                    self.youtube.open_reply = Some(result);
                } else {
                    let reason = self
                        .error
                        .clone()
                        .unwrap_or_else(|| "Open was refused".into());
                    failed(self, reason);
                }
            }
            Some(canonical) => {
                if let Some(reply) = &self.youtube.open_reply {
                    match reply.try_recv() {
                        Ok(Ok(())) => {
                            self.youtube.open_reply = None;
                            started = std::time::Instant::now();
                            self.youtube.opening =
                                Some((path.clone(), Some(canonical.clone()), started));
                        }
                        Ok(Err(reason)) => {
                            failed(self, reason);
                            return;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            failed(
                                self,
                                "the project service stopped before reporting Open".into(),
                            );
                            return;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // An admitted Open may still be verifying a large
                            // project or draining owned workers. Only its reply
                            // completes it; the admission wait is not its limit.
                            context.request_repaint_after(Duration::from_millis(16));
                            return;
                        }
                    }
                }
                if self
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.path == canonical)
                {
                    self.youtube.opening = None;
                    self.youtube.session =
                        self.workspace.as_ref().map(|workspace| workspace.session);
                    self.youtube.modal = false;
                    self.youtube.focus_field = false;
                    self.youtube.url.clear();
                    self.youtube.cookies = None;
                    let name = path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.message = Some(match self.youtube.title.take() {
                        Some(title) => format!("Created “{title}” from YouTube as {name}"),
                        None => format!("Created {name} from YouTube"),
                    });
                    context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
                } else if started.elapsed() >= OPEN_WAIT {
                    failed(
                        self,
                        "Open completed but its workspace was not received".into(),
                    );
                } else {
                    // The command can finish after this frame consumed its UI
                    // mailbox. Admit its actual workspace on a following frame.
                    context.request_repaint_after(Duration::from_millis(16));
                }
            }
        }
    }

    /// Cancel the job for window close; true once no job thread remains.
    pub(super) fn youtube_drained(&mut self) -> bool {
        if self.youtube.jobs.running() {
            self.youtube.jobs.cancel();
            self.youtube.jobs.poll();
        }
        !self.youtube.jobs.running()
    }

    /// The sheet over an open project.
    pub(super) fn youtube_window(&mut self, context: &egui::Context) {
        if self.youtube_surface_inline() {
            self.youtube.modal = false;
            return;
        }
        if !(self.youtube.modal || self.youtube.active()) {
            return;
        }
        self.youtube.modal = true;
        self.youtube_apply_key(context);
        if !(self.youtube.modal || self.youtube.active()) {
            return;
        }
        let width = (context.content_rect().width() - 64.0).clamp(280.0, 520.0);
        egui::Modal::new(egui::Id::new("youtube-sheet")).show(context, |ui| {
            super::accessibility::dialog(ui, "New project from YouTube");
            ui.set_width(width);
            ui.label(style::section_title("NEW PROJECT FROM YOUTUBE", true));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "The whole video becomes the Original of a new project. This project stays as it is.",
                )
                .weak(),
            );
            ui.add_space(8.0);
            self.youtube_steps(ui);
            if !self.youtube.active()
                && matches!(self.youtube.jobs.status(), Status::Idle | Status::Cancelled)
            {
                ui.add_space(4.0);
                if ui.add(style::action("Close", "Esc")).clicked() {
                    self.youtube.jobs.dismiss();
                    self.youtube.modal = false;
                    context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
                }
            }
        });
    }

    /// The board's "Choose one Original" panel on the empty start surface.
    pub(super) fn start_card(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        self.youtube_apply_key(ui.ctx());
        // Once an import is under way the card narrows to that import; the
        // hero, Choose video and Open project return when it ends.
        let importing = self.youtube.opening.is_some()
            || !matches!(
                self.youtube.jobs.status(),
                Status::Idle | Status::Failed(_) | Status::Cancelled
            );
        let width = (rect.width() - 32.0).clamp(240.0, 560.0);
        // Below the default height the hero shares one line, so the whole
        // card, including Open project, fits the minimum window unscrolled.
        let compact = rect.height() < 470.0
            || matches!(
                self.youtube.jobs.status(),
                Status::Failed(_) | Status::NeedsDownloader
            );
        let margin = if compact { 16 } else { 24 };
        // Center the card on its measured height from the previous frame.
        let measured = egui::Id::new("start-card-height");
        let height = ui.ctx().data(|data| data.get_temp::<f32>(measured));
        let top = height.map_or(12.0, |height| ((rect.height() - height) / 2.0).max(12.0));
        let card = egui::Rect::from_min_max(
            egui::pos2(rect.center().x - width / 2.0, rect.top() + top),
            egui::pos2(rect.center().x + width / 2.0, rect.bottom() - 12.0),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(card)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let output = egui::ScrollArea::vertical()
            .id_salt("start-card")
            .auto_shrink([false, true])
            .show(&mut child, |ui| {
                egui::Frame::new()
                    .fill(style::PANEL)
                    .stroke(egui::Stroke::new(1.0, accessibility::border(ui.ctx())))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(margin))
                    .show(ui, |ui| {
                        // The frame's margins and stroke stay inside the card.
                        ui.set_width(
                            width
                                - 2.0 * f32::from(margin)
                                - 2.0
                                - ui.spacing().scroll.allocated_width(),
                        );
                        ui.spacing_mut().item_spacing.y = if compact { 4.0 } else { 6.0 };
                        if importing {
                            ui.label(style::section_title("NEW PROJECT FROM YOUTUBE", true));
                            ui.add_space(2.0);
                            self.youtube_steps(ui);
                            return;
                        }
                        if compact {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    style::semibold("Start with one video.")
                                        .size(20.0)
                                        .color(style::TEXT),
                                );
                                ui.label(
                                    style::semibold("Make it weird.")
                                        .size(20.0)
                                        .color(style::LAVENDER),
                                );
                            });
                        } else {
                            ui.label(
                                style::semibold("Start with one video.")
                                    .size(26.0)
                                    .color(style::TEXT),
                            );
                            ui.label(
                                style::semibold("Make it weird.")
                                    .size(26.0)
                                    .color(style::LAVENDER),
                            );
                        }
                        ui.label(
                            egui::RichText::new("The full video becomes your starting edit.")
                                .size(14.0)
                                .weak(),
                        );
                        ui.add_space(if compact { 4.0 } else { 12.0 });
                        let choose = ui.add(
                            egui::Button::new(style::action_text("Choose video…", "⌘N", 15.0))
                                .fill(style::SELECTED)
                                .stroke(egui::Stroke::new(1.0, style::LAVENDER))
                                .min_size(egui::vec2(ui.available_width(), 40.0)),
                        );
                        if choose.clicked() {
                            self.begin_dialog(DialogKind::CreateProject, ui.ctx(), false);
                        }
                        if ui.button(style::action_text("Link video in place…", ":new-linked", 13.0)).clicked() {
                            self.action(Action::NewLinked, ui.ctx());
                        }
                        ui.small("Choose video keeps a project copy. Linking keeps the video at its existing location.");
                        ui.add_space(if compact { 0.0 } else { 6.0 });
                        divider(ui, "or");
                        ui.add_space(if compact { 0.0 } else { 6.0 });
                        self.youtube_steps(ui);
                        ui.add_space(if compact { 2.0 } else { 10.0 });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Library location: Documents / Deadpan")
                                    .size(12.0)
                                    .weak(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.add(style::action("Open project…", "⌘O")).clicked() {
                                        self.begin_dialog(DialogKind::OpenProject, ui.ctx(), false);
                                    }
                                },
                            );
                        });
                    });
            });
        let content = output.content_size.y;
        if height != Some(content) {
            ui.ctx()
                .data_mut(|data| data.insert_temp(measured, content));
            ui.ctx().request_repaint();
        }
    }

    /// Apply the key routed this frame before any step is laid out.
    fn youtube_apply_key(&mut self, context: &egui::Context) {
        match self.youtube.key.take() {
            Some(UrlKey::Primary) => self.youtube_primary(context),
            Some(UrlKey::Cancel) => self.youtube_cancel(context),
            None => {}
        }
    }

    /// The URL field and every later step, shared by the card and the sheet.
    fn youtube_steps(&mut self, ui: &mut egui::Ui) {
        self.youtube_apply_key(ui.ctx());
        let status = self.youtube.jobs.status().clone();
        if self.youtube.opening.is_some() {
            self.youtube_progress(ui, None, true);
            return;
        }
        match status {
            Status::Idle | Status::Failed(_) | Status::Cancelled => {
                self.youtube_entry(ui);
                match &status {
                    Status::Failed(failure) => self.youtube_failure(ui, failure),
                    Status::Cancelled => {
                        ui.label(
                            egui::RichText::new("Import cancelled. No project was created.")
                                .size(12.0)
                                .weak(),
                        );
                    }
                    _ => {}
                }
            }
            Status::Working(Stage::CheckingDownloader | Stage::FetchingDetails) => {
                self.youtube_url_line(ui);
                ui.horizontal(|ui| {
                    crate::preview::accessibility::busy(ui);
                    ui.label(match status {
                        Status::Working(Stage::CheckingDownloader) => "Checking the downloader…",
                        _ => "Fetching title, length and streams… Nothing downloads yet.",
                    });
                });
                if ui.add(style::action("Cancel", "Esc")).clicked() {
                    self.youtube.jobs.cancel();
                }
            }
            Status::Confirm {
                preview,
                destination,
            } => self.youtube_confirm(ui, &preview, &destination),
            Status::Working(stage) => self.youtube_progress(ui, Some(&stage), false),
            Status::NeedsDownloader => self.youtube_install_offer(ui),
            Status::Installing(progress) => {
                ui.label(style::semibold(format!(
                    "Installing {}…",
                    job::helper_label(progress.helper)
                )));
                ui.add(
                    egui::ProgressBar::new(
                        progress.completed as f32 / progress.total.max(1) as f32,
                    )
                    .desired_height(6.0),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} of {} · each file is checked against its pinned SHA-256",
                        job::megabytes(progress.completed),
                        job::megabytes(progress.total)
                    ))
                    .size(12.0)
                    .weak(),
                );
                if ui.add(style::action("Cancel install", "Esc")).clicked() {
                    self.youtube.jobs.cancel();
                }
            }
            Status::Cancelling => {
                ui.horizontal(|ui| {
                    crate::preview::accessibility::busy(ui);
                    ui.label("Stopping and removing partial files…");
                });
            }
            Status::Created(_) => self.youtube_progress(ui, None, true),
        }
    }

    fn youtube_url_line(&self, ui: &mut egui::Ui) {
        if let Some(Ok(id)) = validation(&self.youtube.url) {
            ui.label(
                egui::RichText::new(format!("youtube.com/watch?v={id}"))
                    .monospace()
                    .size(12.0)
                    .weak(),
            );
        }
    }

    fn youtube_entry(&mut self, ui: &mut egui::Ui) {
        let button = style::action("Start project", "Enter");
        let caption = ui.label(style::section_title("YOUTUBE URL", false));
        ui.horizontal(|ui| {
            let button_width = 150.0;
            let field = ui
                .add(
                    egui::TextEdit::singleline(&mut self.youtube.url)
                        .id(egui::Id::new(URL_ID))
                        .event_filter(super::editor_input::field_filter())
                        .hint_text("https://youtu.be/… or https://www.youtube.com/watch?v=…")
                        .return_key(None)
                        .desired_width((ui.available_width() - button_width).max(120.0))
                        .min_size(egui::vec2(0.0, 30.0))
                        .margin(egui::Margin::symmetric(8, 6)),
                )
                .labelled_by(caption.id);
            if std::mem::take(&mut self.youtube.focus_field) {
                field.request_focus();
            }
            super::retain_text_escape(ui, URL_ID);
            // Validate after the field applied this frame's text and paste,
            // so the button and message never lag the visible URL.
            let valid = matches!(validation(&self.youtube.url), Some(Ok(_)));
            if ui
                .add_enabled(valid, button.min_size(egui::vec2(0.0, 30.0)))
                .clicked()
            {
                self.youtube_primary(ui.ctx());
            }
        });
        match validation(&self.youtube.url) {
            None => {
                ui.label(
                    egui::RichText::new(
                        "Paste a link to one video. Its details appear before anything downloads.",
                    )
                    .size(12.0)
                    .weak(),
                );
            }
            Some(Ok(id)) => {
                ui.label(
                    egui::RichText::new(format!("YouTube video {id} · Enter fetches its details"))
                        .size(12.0)
                        .color(style::SAVED),
                );
            }
            Some(Err(reason)) => {
                let response = ui.label(egui::RichText::new(reason).size(12.0).color(style::ERROR));
                // Spoken when typing makes the URL invalid, without interrupting.
                accessibility::live(&response, false);
            }
        }
        ui.horizontal(|ui| {
            let name = self
                .youtube
                .cookies
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned());
            // A refusal that needs a signed-in session points at this row.
            let wanted = matches!(
                self.youtube.jobs.status(),
                Status::Failed(failure) if failure.needs_cookies()
            ) && name.is_none();
            ui.label(
                egui::RichText::new(match &name {
                    Some(name) => format!("Cookies: {name}"),
                    None => "Cookies (optional, for signed-in videos): none".into(),
                })
                .size(12.0)
                .color(if wanted {
                    style::WARNING
                } else {
                    style::muted(ui)
                }),
            );
            if name.is_some() {
                if ui.small_button("Remove cookies").clicked() {
                    self.youtube.cookies = None;
                }
            } else if ui.small_button("Choose cookies file…").clicked() {
                self.choose_cookies(ui.ctx());
            }
        });
    }

    fn choose_cookies(&mut self, context: &egui::Context) {
        if let Err(error) = self.dialogs.start(DialogKind::Cookies, context) {
            self.error = Some(error);
        }
    }

    fn youtube_failure(&mut self, ui: &mut egui::Ui, failure: &Failure) {
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, style::ERROR))
            .corner_radius(4)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let title = ui.label(style::semibold(failure_title(failure)).color(style::ERROR));
                accessibility::live(&title, true);
                ui.label(egui::RichText::new(sentence(&failure.message)).size(12.0));
                ui.label(egui::RichText::new(failure.guidance()).size(12.0).weak());
                ui.label(
                    egui::RichText::new(&failure.code)
                        .monospace()
                        .size(10.5)
                        .weak(),
                );
                ui.horizontal_wrapped(|ui| {
                    if failure.code == "ProjectOpenFailed"
                        && ui
                            .add(style::action("Open project…", "Enter").fill(style::SELECTED))
                            .clicked()
                    {
                        self.begin_dialog(DialogKind::OpenProject, ui.ctx(), false);
                    }
                    if ui.add(style::action("Dismiss", "Esc")).clicked() {
                        self.youtube.jobs.dismiss();
                        self.youtube.focus_field = true;
                    }
                });
            });
    }

    fn youtube_install_offer(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, style::WARNING))
            .corner_radius(4)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(style::semibold("Install the YouTube downloader?").color(style::WARNING));
                ui.label(format!(
                    "Deadpan downloads videos with {} and {}, which are not installed yet.",
                    job::helper_label(deadpan_cli::youtube::helpers::YT_DLP.name),
                    job::helper_label(deadpan_cli::youtube::helpers::DENO.name)
                ));
                ui.label(
                    egui::RichText::new(format!(
                        "Installing downloads {} from GitHub, checks every file against its pinned SHA-256 and uses {} in Application Support. Nothing is installed until you choose to.",
                        job::megabytes(job::INSTALL_DOWNLOAD_BYTES),
                        job::megabytes(job::INSTALLED_BYTES)
                    ))
                    .size(12.0)
                    .weak(),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add(
                            style::action(
                                format!(
                                    "Install downloader ({})",
                                    job::megabytes(job::INSTALL_DOWNLOAD_BYTES)
                                ),
                                "Enter",
                            )
                            .fill(style::SELECTED),
                        )
                        .clicked()
                    {
                        self.youtube_primary(ui.ctx());
                    }
                    if ui.add(style::action("Not now", "Esc")).clicked() {
                        self.youtube.jobs.dismiss();
                        self.youtube.focus_field = true;
                    }
                });
            });
    }

    fn youtube_confirm(
        &mut self,
        ui: &mut egui::Ui,
        preview: &job::Preview,
        destination: &std::path::Path,
    ) {
        let metadata = &preview.metadata;
        ui.label(
            style::semibold(&metadata.title)
                .size(17.0)
                .color(style::TEXT),
        );
        let mut byline = Vec::new();
        if let Some(author) = &metadata.author {
            byline.push(author.clone());
        }
        byline.push(job::duration_label(metadata.duration_seconds));
        if let Some(date) = &metadata.upload_date
            && date.len() == 8
        {
            byline.push(format!(
                "uploaded {}-{}-{}",
                &date[..4],
                &date[4..6],
                &date[6..]
            ));
        }
        ui.label(egui::RichText::new(byline.join(" · ")).weak());
        ui.add_space(4.0);
        let picture = job::picture_summary(preview);
        let sound = job::sound_summary(preview);
        let download = preview.estimated_bytes.map_or_else(
            || "size not declared".into(),
            |bytes| format!("about {}", job::megabytes(bytes)),
        );
        let saved = job::destination_label(destination);
        let mut rows = vec![
            ("Picture", picture.as_str()),
            ("Sound", sound.as_str()),
            ("Download", download.as_str()),
            ("Saved as", saved.as_str()),
        ];
        if let Some(license) = &metadata.license {
            rows.push(("License", license.as_str()));
        }
        style::value_grid(ui, "youtube-confirm", rows);
        if let Some(note) = &preview.downloader_note {
            // The import uses the baseline helpers instead of an installed
            // signed update; never silently.
            ui.colored_label(
                style::WARNING,
                format!("Downloader: {note}. Check Models (:models) to roll back or update."),
            );
        }
        ui.label(
            egui::RichText::new("You are responsible for having the rights to use this video.")
                .size(12.0)
                .color(style::WARNING),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(style::action("Download and create", "Enter").fill(style::SELECTED))
                .clicked()
            {
                self.youtube_primary(ui.ctx());
            }
            if ui.add(style::action("Cancel", "Esc")).clicked() {
                self.youtube.jobs.cancel();
            }
        });
    }

    /// Stage rows from details to opening. `stage` None with `opening` true
    /// means every acquisition stage is complete.
    fn youtube_progress(&mut self, ui: &mut egui::Ui, stage: Option<&Stage>, opening: bool) {
        if let Some(title) = &self.youtube.title {
            ui.label(style::semibold(title).size(17.0).color(style::TEXT));
        }
        let current = match stage {
            Some(Stage::Downloading { .. }) => 1,
            Some(Stage::Assembling) => 2,
            Some(Stage::Qualifying) => 3,
            Some(Stage::CheckingDownloader | Stage::FetchingDetails) => 0,
            None if opening => 4,
            None => 0,
        };
        let download_detail = match stage {
            Some(Stage::Downloading {
                downloaded,
                estimated: Some(total),
            }) if *total > 0 => Some((
                (*downloaded as f32 / *total as f32).min(1.0),
                format!(
                    "{:.0}% · {} of {}",
                    (*downloaded as f64 * 100.0 / *total as f64).min(100.0),
                    job::megabytes(*downloaded),
                    job::megabytes(*total)
                ),
            )),
            Some(Stage::Downloading { downloaded, .. }) => {
                Some((0.0, format!("{} so far", job::megabytes(*downloaded))))
            }
            _ => None,
        };
        let steps = [
            "Video details",
            "Downloading picture and sound",
            "Assembling one Original",
            "Checking and retaining the Original",
            "Opening the project",
        ];
        for (index, label) in steps.into_iter().enumerate() {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 20.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 16.0), egui::Sense::hover());
                    let center = rect.center();
                    match index.cmp(&current) {
                        std::cmp::Ordering::Less => {
                            ui.painter().circle_filled(center, 5.0, style::SAVED);
                        }
                        std::cmp::Ordering::Equal => {
                            ui.painter().circle_filled(center, 5.0, style::LAVENDER);
                        }
                        std::cmp::Ordering::Greater => {
                            ui.painter().circle_stroke(
                                center,
                                4.5,
                                egui::Stroke::new(1.0, accessibility::border(ui.ctx())),
                            );
                        }
                    }
                    let state = match index.cmp(&current) {
                        std::cmp::Ordering::Less => "done",
                        std::cmp::Ordering::Equal => "now",
                        std::cmp::Ordering::Greater => "next",
                    };
                    let text = egui::RichText::new(label).color(if index <= current {
                        style::TEXT
                    } else {
                        style::muted(ui)
                    });
                    ui.label(if index == current {
                        text.strong()
                    } else {
                        text
                    })
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Label,
                            true,
                            format!("{label}: {state}"),
                        )
                    });
                },
            );
            if index == 1
                && current == 1
                && let Some((fraction, detail)) = &download_detail
            {
                ui.add(egui::ProgressBar::new(*fraction).desired_height(6.0));
                ui.label(egui::RichText::new(detail).monospace().size(11.5).weak());
            }
        }
        if opening {
            // Publication is atomic; a complete package is never removed.
            if self.youtube.jobs.completed_after_cancel() {
                ui.label(
                    egui::RichText::new(
                        "The project was already complete when you pressed Esc, so it opens.",
                    )
                    .size(12.0)
                    .color(style::WARNING),
                );
            }
            ui.label(
                egui::RichText::new("The project is complete. Esc can no longer cancel; opening…")
                    .size(12.0)
                    .weak(),
            );
        } else if ui.add(style::action("Cancel", "Esc")).clicked() {
            self.youtube.jobs.cancel();
        }
    }
}

/// A centered word between two rules.
fn divider(ui: &mut egui::Ui, word: &str) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 16.0), egui::Sense::hover());
    let galley = ui.painter().layout_no_wrap(
        word.to_owned(),
        egui::FontId::proportional(12.0),
        style::muted(ui),
    );
    let gap = galley.size().x / 2.0 + 8.0;
    let y = rect.center().y;
    let stroke = egui::Stroke::new(1.0, accessibility::border(ui.ctx()));
    ui.painter()
        .hline(rect.left()..=rect.center().x - gap, y, stroke);
    ui.painter()
        .hline(rect.center().x + gap..=rect.right(), y, stroke);
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        style::muted(ui),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_shows_the_normalized_video_or_the_exact_refusal() {
        assert_eq!(validation("  "), None);
        assert_eq!(
            validation(" https://youtu.be/Z4C82eyhwgU?si=x "),
            Some(Ok("Z4C82eyhwgU".into()))
        );
        assert_eq!(
            validation("https://www.youtube.com/playlist?list=PLx"),
            Some(Err(
                "This is a playlist; choose a specific video from it.".into()
            ))
        );
        assert_eq!(
            validation("http://youtu.be/Z4C82eyhwgU"),
            Some(Err("Only HTTPS YouTube URLs are supported.".into()))
        );
        assert_eq!(
            validation("https://vimeo.com/1"),
            Some(Err(
                "Only youtube.com, m.youtube.com, music.youtube.com, youtu.be and youtube-nocookie.com URLs are supported."
                    .into()
            ))
        );
    }
}
