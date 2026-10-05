//! The Models panel: every approved model pack with its size, state, memory
//! estimate and licenses, and the explicit actions that install, resume,
//! import, cancel, remove or discard it.
//!
//! `:models` and the Deadpan menu's Models… open it; the AI PICTURES and
//! TRANSCRIPT offers open it focused on their pack. It is a modal: Tab moves
//! between its controls, Space or Enter activates the focused one, and Escape
//! closes it without changing anything. An install keeps running after the
//! panel closes and across project changes.

use std::collections::{BTreeMap, BTreeSet};

use deadpan_models::packs::{Operation, PackManifest, PackState};

use super::*;
use crate::model_packs::{
    Backend, Ending, Manager, Work, format_bytes, install_blocker, install_label, remaining,
};

/// How often an open panel or a visible offer re-reads the pack directory,
/// which the command line can change too.
const REFRESH: Duration = Duration::from_secs(2);

pub(super) struct Models {
    pub(super) manager: Manager,
    pub(super) open: bool,
    /// The pack whose first control takes focus when the panel opens.
    pub(super) focus: Option<String>,
    focus_pending: bool,
    /// Show the list from its top on the next frame.
    scroll_top: bool,
    /// License identifiers accepted per pack in this app session.
    pub(super) accepted: BTreeMap<String, BTreeSet<String>>,
    /// Licenses whose full text is expanded, as (pack, license).
    reading: BTreeSet<(String, String)>,
    /// A newly expanded license text to scroll into view, for this many
    /// more frames: the list's content size grows only after the first.
    reveal_text: Option<((String, String), u8)>,
    /// The pack a folder or archive picker was opened for.
    dialog_pack: Option<String>,
    pub(super) error: Option<String>,
    return_focus: Option<(u64, Pane)>,
}

impl Default for Models {
    fn default() -> Self {
        Self {
            manager: Manager::new(Backend::Real),
            open: false,
            focus: None,
            focus_pending: false,
            scroll_top: false,
            accepted: BTreeMap::new(),
            reading: BTreeSet::new(),
            reveal_text: None,
            dialog_pack: None,
            error: None,
            return_focus: None,
        }
    }
}

/// What a pack is used for, in app terms.
fn purpose(pack: &PackManifest) -> &'static str {
    if pack.supports(Operation::BridgeHold) {
        "AI pause pictures (AI PICTURES in the inspector)"
    } else if pack.supports(Operation::Transcribe) {
        "Transcription and pause detection (TRANSCRIPT in the Original rail)"
    } else {
        "Analysis on this Mac"
    }
}

/// The status line under a pack's title.
pub(super) fn state_line(
    pack: &PackManifest,
    state: Option<&Result<PackState, String>>,
) -> (String, egui::Color32) {
    let size = format_bytes(pack.total_bytes());
    match state {
        Some(Ok(PackState::Installed(_))) => (format!("Installed · {size}"), style::SAVED),
        Some(Ok(PackState::Partial { bytes })) => (
            format!(
                "Partly downloaded · {} of {size} · {} left",
                format_bytes(*bytes),
                format_bytes(pack.total_bytes().saturating_sub(*bytes))
            ),
            style::WARNING,
        ),
        Some(Ok(PackState::Absent)) | None => {
            (format!("Not installed · {size} download"), style::MUTED)
        }
        Some(Err(error)) => (format!("Pack state unavailable: {error}"), style::ERROR),
    }
}

impl DeadpanApp {
    /// Open the panel, optionally focused on one pack.
    pub(super) fn open_models(&mut self, focus: Option<&str>, context: &egui::Context) {
        self.pause_playback();
        self.bindings.clear();
        self.models.manager.refresh();
        self.models.open = true;
        self.models.focus = focus.map(str::to_owned);
        self.models.focus_pending = focus.is_some();
        self.models.scroll_top = focus.is_none();
        self.models.error = None;
        context.request_repaint();
    }

    fn close_models(&mut self, context: &egui::Context) {
        self.models.open = false;
        self.models.focus = None;
        self.models.focus_pending = false;
        self.models.return_focus = Some((context.cumulative_frame_nr(), self.pane));
        context.request_discard("models closed");
        context.request_repaint();
    }

    /// Apply job events once per outer frame.
    pub(super) fn reconcile_models(&mut self, context: &egui::Context) {
        if self.models.manager.poll() {
            context.request_repaint();
        }
        if self.models.manager.job().is_some() {
            // Progress arrives with a repaint; this keeps a stalled transfer
            // and its elapsed state visible.
            context.request_repaint_after(Duration::from_millis(250));
        }
        if self.models.open {
            self.models.manager.refresh_if_stale(REFRESH);
            context.request_repaint_after(REFRESH);
        }
    }

    /// The panel owns the keyboard while open. Its widgets keep native Tab,
    /// Space and Enter; editor bindings never see these keys.
    pub(super) fn models_keyboard(&mut self, context: &egui::Context) -> bool {
        if let Some((frame, pane)) = self.models.return_focus
            && context.cumulative_frame_nr() > frame
        {
            context.memory_mut(|memory| memory.request_focus(pane_id(pane)));
            self.models.return_focus = None;
        }
        if !self.models.open {
            return false;
        }
        let events = context.input(|input| input.events.clone());
        help_scroll::observe_composition(&events, &mut self.ime_composing);
        self.bindings.clear();
        // A held key must not activate one control after another.
        context.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Enter | egui::Key::Space,
                        repeat: true,
                        ..
                    }
                )
            })
        });
        true
    }

    fn start_model_job(&mut self, pack_id: &str, work: Work, context: &egui::Context) {
        let accepted = self
            .models
            .accepted
            .get(pack_id)
            .cloned()
            .unwrap_or_default();
        let repaint = context.clone();
        self.models.error = self
            .models
            .manager
            .start(pack_id, work, &accepted, move || repaint.request_repaint())
            .err();
    }

    fn choose_model_source(&mut self, pack_id: &str, archive: bool, context: &egui::Context) {
        let kind = if archive {
            DialogKind::ModelPackArchive
        } else {
            DialogKind::ModelPackFolder
        };
        match self.dialogs.start(kind, context) {
            Ok(()) => self.models.dialog_pack = Some(pack_id.to_owned()),
            Err(error) => self.models.error = Some(error),
        }
    }

    /// A chosen folder or archive starts that pack's offline install.
    pub(super) fn receive_model_source(
        &mut self,
        path: Option<std::path::PathBuf>,
        error: Option<String>,
        context: &egui::Context,
    ) {
        let pack = self.models.dialog_pack.take();
        if let Some(error) = error {
            self.models.error = Some(error);
            return;
        }
        let (Some(pack), Some(path)) = (pack, path) else {
            return;
        };
        self.start_model_job(&pack, Work::Install { source: Some(path) }, context);
    }

    pub(super) fn models_window(&mut self, context: &egui::Context) {
        if !self.models.open {
            return;
        }
        let width = (context.content_rect().width() - 64.0).clamp(300.0, 640.0);
        let height = (context.content_rect().height() - 200.0).max(160.0);
        let picker_open = self.dialogs.is_open();
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("models-window")).show(context, |ui| {
            ui.set_width(width);
            ui.label(style::section_title("MODELS", true));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Models run on this Mac and are shared by every project. Nothing downloads until you choose Install; every file is verified before use.",
                )
                .color(style::MUTED),
            );
            let storage = match (self.models.manager.root(), self.models.manager.free_space()) {
                (Ok(root), Some(Ok(free))) => {
                    format!("Stored in {} · {} free", root.display(), format_bytes(*free))
                }
                (Ok(root), Some(Err(error))) => {
                    format!("Stored in {} · free space unknown: {error}", root.display())
                }
                (Ok(root), None) => format!("Stored in {}", root.display()),
                (Err(error), _) => format!("Model storage is unavailable: {error}"),
            };
            ui.label(egui::RichText::new(storage).size(11.5).color(style::MUTED));
            ui.add_space(6.0);
            let mut list = egui::ScrollArea::vertical()
                .id_salt("model-packs")
                .max_height(height)
                .auto_shrink([false, true]);
            if std::mem::take(&mut self.models.scroll_top) {
                list = list.vertical_scroll_offset(0.0);
            }
            list.show(ui, |ui| {
                    let packs = self.models.manager.packs().to_vec();
                    for pack in &packs {
                        self.model_pack_card(ui, pack, picker_open);
                        ui.add_space(8.0);
                    }
                });
            if let Some(error) = &self.models.error {
                ui.colored_label(style::ERROR, error);
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let button = ui.add(style::action("Close", "Esc"));
                reveal(&button);
                if button.clicked() {
                    close = true;
                }
                ui.label(
                    egui::RichText::new("Tab moves between controls · Space or Enter activates")
                        .size(11.0)
                        .color(style::MUTED),
                );
            });
        });
        close |= modal.should_close() && !self.ime_composing && !picker_open;
        if close {
            self.close_models(context);
        }
    }

    fn model_pack_card(&mut self, ui: &mut egui::Ui, pack: &PackManifest, picker_open: bool) {
        let id = pack.pack_id.clone();
        let state = self.models.manager.state(&id).cloned();
        let job = self.models.manager.job().cloned();
        let busy = job.is_some() || picker_open;
        let outcome = self
            .models
            .manager
            .outcome()
            .filter(|outcome| outcome.pack_id == id)
            .cloned();
        let focus_here = self.models.focus_pending && self.models.focus.as_deref() == Some(&id);
        let mut claimed = false;
        // The first control of the focused pack takes focus once.
        let mut claim = |response: &egui::Response, primary: bool| {
            if primary && focus_here && !claimed {
                claimed = true;
                response.request_focus();
                response.scroll_to_me_animation(
                    Some(egui::Align::Center),
                    egui::style::ScrollAnimation::none(),
                );
            }
            reveal(response);
        };
        egui::Frame::new()
            .fill(style::PANEL)
            .stroke(egui::Stroke::new(
                1.0,
                if self.models.focus.as_deref() == Some(&id) {
                    style::LAVENDER
                } else {
                    style::BORDER
                },
            ))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(style::semibold(&pack.title).size(14.0));
                let (line, color) = state_line(pack, state.as_ref());
                ui.colored_label(color, line);
                ui.label(
                    egui::RichText::new(format!(
                        "For {} · uses about {} of memory while running",
                        purpose(pack),
                        format_bytes(pack.memory_bytes)
                    ))
                    .size(12.0)
                    .color(style::MUTED),
                );
                let accepted = self.models.accepted.entry(id.clone()).or_default();
                for license in &pack.licenses {
                    ui.add_space(6.0);
                    ui.label(style::semibold(format!(
                        "{} · {}",
                        license.title,
                        format_bytes(pack.license_bytes(license))
                    )));
                    ui.label(egui::RichText::new(&license.terms).size(12.5));
                    ui.label(
                        egui::RichText::new(&license.attribution)
                            .size(11.5)
                            .color(style::MUTED),
                    );
                    ui.label(
                        egui::RichText::new(&license.access)
                            .size(11.5)
                            .color(style::MUTED),
                    );
                    let link = ui.hyperlink_to(format!("License source: {}", license.url), &license.url);
                    claim(&link, false);
                    if let Some(text) = license
                        .text
                        .as_deref()
                        .and_then(deadpan_models::packs::license_text)
                    {
                        let key = (id.clone(), license.id.clone());
                        let expanded = self.models.reading.contains(&key);
                        let toggle = ui.button(format!(
                            "{} the full {}",
                            if expanded { "Hide" } else { "Read" },
                            license.title
                        ));
                        claim(&toggle, false);
                        if toggle.clicked() {
                            if expanded {
                                self.models.reading.remove(&key);
                            } else {
                                self.models.reveal_text = Some((key.clone(), 2));
                                self.models.reading.insert(key.clone());
                            }
                        }
                        if expanded {
                            let shown = egui::Frame::new()
                                .fill(style::CANVAS)
                                .corner_radius(4)
                                .inner_margin(egui::Margin::same(8))
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .id_salt(("license-text", &id, &license.id))
                                        .max_height(220.0)
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            ui.label(egui::RichText::new(text).size(11.5));
                                        });
                                });
                            if let Some((revealed, frames)) = &mut self.models.reveal_text
                                && *revealed == key
                            {
                                *frames -= 1;
                                if *frames == 0 {
                                    self.models.reveal_text = None;
                                }
                                ui.ctx().request_repaint();
                                shown.response.scroll_to_me_animation(
                                    None,
                                    egui::style::ScrollAnimation::none(),
                                );
                            }
                        }
                    }
                    if license.acceptance_required {
                        let mut checked = accepted.contains(&license.id);
                        let checkbox = ui.add_enabled(
                            job.as_ref().is_none_or(|job| job.pack_id != id),
                            egui::Checkbox::new(&mut checked, format!("I accept the {}", license.title)),
                        );
                        claim(&checkbox, true);
                        if checkbox.changed() {
                            if checked {
                                accepted.insert(license.id.clone());
                            } else {
                                accepted.remove(&license.id);
                            }
                        }
                    }
                }
                let accepted = accepted.clone();
                ui.add_space(8.0);
                if let Some(job) = job.as_ref().filter(|job| job.pack_id == id) {
                    let text = if job.installing() {
                        format!(
                            "{} · {} / {}",
                            job.label(),
                            format_bytes(job.progress.completed_bytes),
                            format_bytes(job.progress.total_bytes)
                        )
                    } else {
                        format!("{}…", job.label())
                    };
                    ui.label(text);
                    ui.add(
                        egui::ProgressBar::new(job.fraction())
                            .desired_height(4.0)
                            .fill(style::LAVENDER),
                    );
                    if job.cancelling {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.weak("Cancelling; downloaded bytes are kept for Resume…");
                        });
                    } else if job.installing() {
                        let cancel = ui.button("Cancel install");
                        claim(&cancel, true);
                        if cancel.clicked() {
                            self.models.manager.cancel();
                        }
                    }
                    return;
                }
                let current = state.as_ref().and_then(|state| state.as_ref().ok());
                let installed = matches!(current, Some(PackState::Installed(_)));
                let free = self
                    .models
                    .manager
                    .free_space()
                    .and_then(|free| free.as_ref().ok().copied());
                let blocker = install_blocker(pack, &accepted, current, free, busy);
                let licensed = pack.check_acceptance(&accepted.iter().cloned().collect::<Vec<_>>()).is_ok();
                if let Some(blocker) = blocker.as_ref().filter(|_| !installed && !busy) {
                    ui.colored_label(
                        if blocker.starts_with("Not enough") {
                            style::WARNING
                        } else {
                            style::MUTED
                        },
                        blocker,
                    );
                }
                let mut action = None;
                let mut pick = None;
                ui.horizontal_wrapped(|ui| {
                    if installed {
                        let remove = ui.add_enabled(!busy, egui::Button::new(format!("Remove · frees {}", format_bytes(pack.total_bytes()))));
                        claim(&remove, true);
                        if remove.clicked() {
                            action = Some(Work::Remove);
                        }
                        return;
                    }
                    let mut label = install_label(pack, current);
                    if remaining(pack, current) == 0 {
                        label = "Resume · test and activate".into();
                    }
                    let install = ui.add_enabled(
                        blocker.is_none(),
                        egui::Button::new(label).fill(style::SELECTED),
                    );
                    claim(&install, true);
                    if install.clicked() {
                        action = Some(Work::Install { source: None });
                    }
                    for (archive, label) in [(false, "Install from folder…"), (true, "Install from archive…")] {
                        let button = ui.add_enabled(licensed && !busy, egui::Button::new(label))
                            .on_hover_text(if archive {
                                "An uncompressed .tar holding the pack's files, for example one `deadpan-cli models export` wrote. Every file is verified."
                            } else {
                                "A folder holding the pack's files, for example a copy from another Mac. Every file is verified; on the same volume nothing is duplicated."
                            });
                        claim(&button, false);
                        if button.clicked() {
                            pick = Some(archive);
                        }
                    }
                    if matches!(current, Some(PackState::Partial { .. })) {
                        let discard = ui.add_enabled(!busy, egui::Button::new("Discard partial download"));
                        claim(&discard, false);
                        if discard.clicked() {
                            action = Some(Work::Discard);
                        }
                    }
                });
                if let Some(outcome) = &outcome {
                    let (text, color) = match &outcome.ending {
                        Ending::Installed => ("Installed and tested.".to_owned(), style::SAVED),
                        Ending::Removed => ("Removed. Projects are unchanged.".to_owned(), style::MUTED),
                        Ending::Discarded => ("Partial download discarded.".to_owned(), style::MUTED),
                        Ending::Cancelled => (
                            match current {
                                Some(PackState::Partial { bytes }) => format!(
                                    "Cancelled. {} kept for Resume.",
                                    format_bytes(*bytes)
                                ),
                                _ => "Cancelled.".to_owned(),
                            },
                            style::MUTED,
                        ),
                        Ending::Failed(error) => (format!("Install failed: {error}"), style::ERROR),
                    };
                    ui.colored_label(color, text);
                }
                if let Some(archive) = pick {
                    self.choose_model_source(&id, archive, ui.ctx());
                }
                if let Some(work) = action {
                    self.start_model_job(&id, work, ui.ctx());
                }
            });
        if focus_here && claimed {
            self.models.focus_pending = false;
        }
    }

    /// A short offer for a missing pack, used by the inspector and rail.
    /// Shows a running install's progress instead. Returns true when the
    /// user asked to open the panel.
    pub(super) fn model_pack_offer(
        &mut self,
        ui: &mut egui::Ui,
        pack_id: &str,
        label: &str,
        key: &str,
    ) -> bool {
        if let Some(job) = self.models.manager.installing(pack_id).cloned() {
            ui.label(
                egui::RichText::new(format!(
                    "Installing model · {} / {}",
                    format_bytes(job.progress.completed_bytes),
                    format_bytes(job.progress.total_bytes)
                ))
                .size(12.0),
            );
            ui.add(
                egui::ProgressBar::new(job.fraction())
                    .desired_height(4.0)
                    .fill(style::LAVENDER),
            );
            return ui
                .add(style::row_action(ui, "Show in Models…", key))
                .clicked();
        }
        ui.add(style::row_action(ui, label, key)).clicked()
    }
}

/// Keep a newly focused control inside its scroll area at once; an animated
/// reveal can be cancelled by a decaying wheel scroll.
fn reveal(response: &egui::Response) {
    if response.gained_focus() {
        // Centered, so the reason or action below a control shows too.
        response.scroll_to_me_animation(
            Some(egui::Align::Center),
            egui::style::ScrollAnimation::none(),
        );
    }
}
