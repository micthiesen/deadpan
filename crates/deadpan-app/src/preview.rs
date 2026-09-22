use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use deadpan_core::{AssetId, NodeId, NodeKind, ProjectFrame, SourceFrameId};
use deadpan_render::{FitMode, PictureRenderer, RenderTarget};
use deadpan_store::original_media::OriginalOwnership;
use eframe::{egui, egui_wgpu};

use crate::dialogs::{DialogKind, Dialogs};
use crate::navigation::{self, Action, Bindings, Pane, TextAction};
use crate::presentation::Presentation;
use crate::project::{
    ImportMedia, ImportStage, ImportStatus, ProjectRequest, ProjectService, Workspace,
};
use crate::worker::{PreviewWorker, ProjectView, SourceSummary, Ticket, Work};

const SEARCH_ID: &str = "source-search";
const COMMAND_ID: &str = "command-input";
const MAX_TARGET_PIXELS: f64 = 1920.0 * 1080.0;
const BEAT_WIDTH: f32 = 172.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Source,
    Sequence,
}

struct RegisteredTarget {
    target: RenderTarget,
    texture: egui::TextureId,
}
struct DialogIntent {
    session: Option<u64>,
    media: ImportMedia,
    linked: bool,
    preview_only: bool,
}

pub struct DeadpanApp {
    smoke_frames: Option<u8>,
    close_pending: bool,
    exited: Rc<Cell<bool>>,
    worker: PreviewWorker,
    service: ProjectService,
    dialogs: Dialogs,
    dialog_intent: Option<DialogIntent>,
    render_state: egui_wgpu::RenderState,
    renderer: PictureRenderer,
    target: Option<RegisteredTarget>,
    workspace: Option<Arc<Workspace>>,
    import: Option<ImportStatus>,
    selected_source: Option<AssetId>,
    selected_beat: Option<NodeId>,
    view: View,
    pane: Pane,
    source_cursor: u64,
    sequence_cursor: u64,
    source_search: String,
    command: String,
    command_open: bool,
    bindings: Bindings,
    ime_composing: bool,
    help_open: bool,
    linked_import: bool,
    audio_import: bool,
    last_inserted: Option<deadpan_core::RevisionId>,
    beat_rows: Arc<Vec<BeatRow>>,
    source_rows: Arc<Vec<SourceRow>>,
    reveal_beat: bool,
    reveal_source: bool,
    raw_source: Option<PathBuf>,
    summary: Option<SourceSummary>,
    serial: u64,
    preview_source: u64,
    presentation: Presentation,
    error: Option<String>,
    project_error: Option<String>,
    message: Option<String>,
}

impl DeadpanApp {
    pub fn new(
        context: &eframe::CreationContext<'_>,
        render_state: egui_wgpu::RenderState,
        smoke_test: bool,
        exited: Rc<Cell<bool>>,
        initial_path: Option<String>,
        initial_project: Option<String>,
    ) -> Result<Self, std::io::Error> {
        context.egui_ctx.all_styles_mut(|style| {
            style.visuals.weak_text_color = Some(if style.visuals.dark_mode {
                egui::Color32::from_gray(170)
            } else {
                egui::Color32::from_gray(85)
            });
            for text in [egui::TextStyle::Body, egui::TextStyle::Button] {
                style
                    .text_styles
                    .insert(text, egui::FontId::proportional(14.0));
            }
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
        });
        let repaint = context.egui_ctx.clone();
        let service = ProjectService::new(Arc::new(move || repaint.request_repaint()))?;
        let worker = PreviewWorker::new(context.egui_ctx.clone())?;
        let renderer = PictureRenderer::new(&render_state.device, &render_state.queue);
        let mut app = Self {
            smoke_frames: smoke_test.then_some(0),
            close_pending: false,
            exited,
            worker,
            service,
            dialogs: Dialogs::default(),
            dialog_intent: None,
            render_state,
            renderer,
            target: None,
            workspace: None,
            import: None,
            selected_source: None,
            selected_beat: None,
            view: View::Source,
            pane: Pane::Viewer,
            source_cursor: 0,
            sequence_cursor: 0,
            source_search: String::new(),
            command: String::new(),
            command_open: false,
            bindings: Bindings::default(),
            ime_composing: false,
            help_open: false,
            linked_import: false,
            audio_import: false,
            last_inserted: None,
            beat_rows: Arc::new(Vec::new()),
            source_rows: Arc::new(Vec::new()),
            reveal_beat: false,
            reveal_source: false,
            raw_source: None,
            summary: None,
            serial: 0,
            preview_source: 0,
            presentation: Presentation::default(),
            error: None,
            project_error: None,
            message: None,
        };
        if let Some(path) = initial_project {
            app.submit(ProjectRequest::Open(PathBuf::from(path)));
        } else if let Some(path) = initial_path {
            app.open_raw(PathBuf::from(path));
        }
        context
            .egui_ctx
            .memory_mut(|m| m.request_focus(pane_id(Pane::Viewer)));
        Ok(app)
    }

    fn submit(&mut self, request: ProjectRequest) -> bool {
        match self.service.submit(request) {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    fn next_serial(&mut self) -> Option<u64> {
        match self.serial.checked_add(1) {
            Some(value) => {
                self.serial = value;
                Some(value)
            }
            None => {
                self.error = Some("Preview identities are exhausted. Reopen Deadpan.".into());
                None
            }
        }
    }

    fn clear_picture(&mut self) {
        self.reset_picture();
        self.worker.clear();
    }

    /// Clear presentation while a self-contained request replaces the picture.
    /// The worker keeps its verified decoder until the session/asset key changes.
    fn reset_picture(&mut self) {
        self.presentation.clear();
        if self.raw_source.is_none() {
            self.summary = None;
        }
        self.forget_target();
    }

    fn open_raw(&mut self, path: PathBuf) {
        self.clear_picture();
        self.summary = None;
        self.raw_source = Some(path.clone());
        self.view = View::Source;
        self.source_cursor = 0;
        let Some(serial) = self.next_serial() else {
            return;
        };
        self.preview_source = serial;
        let ticket = Ticket {
            source: serial,
            request: serial,
        };
        let work = Work::Open(path);
        self.presentation.request(ticket, &work);
        self.error = None;
        self.worker.submit(ticket, work);
    }

    fn source_length(&self) -> u64 {
        if self.raw_source.is_some() {
            return self.summary.as_ref().map_or(0, |s| s.frame_count);
        }
        self.workspace
            .as_ref()
            .and_then(|w| {
                self.selected_source
                    .as_ref()
                    .and_then(|id| w.sources.get(id))
            })
            .and_then(|source| source.video_index.as_ref())
            .map_or(0, |index| index.frames().len() as u64)
    }

    fn sequence_length(&self) -> u64 {
        self.workspace
            .as_ref()
            .map_or(0, |w| w.plan.duration().frames() as u64)
    }

    fn request_picture(&mut self, clear: bool) {
        self.error = None;
        if clear {
            self.reset_picture();
        }
        let Some(serial) = self.next_serial() else {
            return;
        };
        if clear {
            self.preview_source = serial;
        }
        let work = if let Some(workspace) = &self.workspace {
            let view = match self.view {
                View::Source => {
                    let Some(asset) = &self.selected_source else {
                        self.clear_picture();
                        return;
                    };
                    if self.source_length() == 0 {
                        self.clear_picture();
                        return;
                    }
                    ProjectView::Source {
                        asset: asset.clone(),
                        frame: SourceFrameId(
                            self.source_cursor
                                .min(self.source_length().saturating_sub(1)),
                        ),
                    }
                }
                View::Sequence => ProjectView::Sequence {
                    frame: ProjectFrame(
                        self.sequence_cursor
                            .min(self.sequence_length().saturating_sub(1))
                            as i64,
                    ),
                },
            };
            Work::Project {
                workspace: Arc::clone(workspace),
                view,
            }
        } else if clear && let Some(path) = &self.raw_source {
            self.source_cursor = 0;
            Work::Open(path.clone())
        } else if self.raw_source.is_some() && self.summary.is_some() {
            Work::Frame(SourceFrameId(
                self.source_cursor
                    .min(self.source_length().saturating_sub(1)),
            ))
        } else {
            return;
        };
        let ticket = Ticket {
            source: self.preview_source,
            request: serial,
        };
        self.presentation.request(ticket, &work);
        self.worker.submit(ticket, work);
    }

    fn receive(&mut self) {
        if let Some(update) = self.service.take_update() {
            let old_session = self.workspace.as_ref().map(|w| w.session);
            let old_revision = self
                .workspace
                .as_ref()
                .map(|w| w.document.revision_id().clone());
            let new_session = update.workspace.as_ref().map(|w| w.session);
            let new_revision = update
                .workspace
                .as_ref()
                .map(|w| w.document.revision_id().clone());
            let completed = update
                .import
                .as_ref()
                .is_some_and(|i| i.stage == ImportStage::Complete)
                && self.import.as_ref().is_none_or(|i| {
                    i.stage != ImportStage::Complete
                        || update
                            .import
                            .as_ref()
                            .is_some_and(|next| next.asset != i.asset)
                });
            self.workspace = update.workspace;
            self.import = update.import;
            self.project_error = update.error;
            self.message = update.message;
            if old_session != new_session {
                self.clear_picture();
                self.raw_source = None;
                self.selected_source = None;
                self.selected_beat = None;
                self.source_cursor = 0;
                self.sequence_cursor = 0;
                self.last_inserted = None;
                self.source_search.clear();
                self.view = View::Source;
                self.summary = None;
                self.error = None;
            }
            if let Some(workspace) = &self.workspace {
                if self
                    .selected_source
                    .as_ref()
                    .is_none_or(|id| !workspace.sources.contains_key(id))
                {
                    self.selected_source = workspace.sources.keys().next().cloned();
                    self.source_cursor = 0;
                }
                if self
                    .selected_beat
                    .as_ref()
                    .is_some_and(|id| !workspace.document.nodes().contains_key(id))
                {
                    self.selected_beat = None;
                }
            }
            if old_revision != new_revision || old_session != new_session {
                self.rebuild_rows();
            }
            let inserted = update
                .inserted
                .filter(|(revision, _)| self.last_inserted.as_ref() != Some(revision));
            if let Some((revision, node)) = inserted {
                self.last_inserted = Some(revision);
                self.view = View::Sequence;
                self.pane = Pane::Sequence;
                if let Some(beat) = self.beat_rows.iter().find(|b| b.id == node) {
                    self.selected_beat = Some(node);
                    self.sequence_cursor = beat.start;
                    self.reveal_beat = true;
                }
            } else if completed
                && let Some(asset) = self.import.as_ref().and_then(|i| i.asset.clone())
            {
                self.selected_source = Some(asset);
                self.source_cursor = 0;
                self.view = View::Source;
                self.reveal_source = true;
            }
            self.sequence_cursor = self.sequence_cursor.min(self.sequence_length());
            self.source_cursor = self.source_cursor.min(self.source_length());
            if old_revision != new_revision || old_session != new_session || completed {
                self.request_picture(true);
            }
        }
        if let Some(result) = self
            .worker
            .take_reply()
            .and_then(|reply| self.presentation.receive(reply))
        {
            match result {
                Ok(summary) => {
                    if summary.is_some() {
                        self.summary = summary;
                    }
                }
                Err(_) => {
                    self.forget_target();
                }
            }
        }
    }

    fn begin_dialog(&mut self, kind: DialogKind, context: &egui::Context, preview_only: bool) {
        if self.dialogs.is_open() || self.service.is_busy() {
            return;
        }
        if kind == DialogKind::ImportMedia && !preview_only && self.importing() {
            self.message =
                Some("Finish or cancel the current import before choosing another source.".into());
            return;
        }
        if kind == DialogKind::ImportMedia && !preview_only && self.workspace.is_none() {
            self.message = Some("Create or open a project before importing media.".into());
            return;
        }
        match self.dialogs.start(kind, context) {
            Ok(()) => {
                self.dialog_intent = Some(DialogIntent {
                    session: self.workspace.as_ref().map(|w| w.session),
                    media: if self.audio_import {
                        ImportMedia::Audio { stream: 0 }
                    } else {
                        ImportMedia::Video
                    },
                    linked: self.linked_import,
                    preview_only,
                });
                self.bindings.clear();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn importing(&self) -> bool {
        self.import.as_ref().is_some_and(|import| {
            matches!(
                import.stage,
                ImportStage::Retaining
                    | ImportStage::Decoding
                    | ImportStage::PreparingInsertion
                    | ImportStage::Registering
            )
        })
    }

    fn receive_dialog(&mut self) {
        let Some(result) = self.dialogs.take_result() else {
            return;
        };
        let intent = self.dialog_intent.take();
        let Some(path) = result.path else {
            return;
        };
        match result.kind {
            DialogKind::CreateProject => {
                self.submit(ProjectRequest::Create(path));
            }
            DialogKind::OpenProject => {
                self.submit(ProjectRequest::Open(path));
            }
            DialogKind::ImportMedia => {
                let Some(intent) = intent else {
                    return;
                };
                if intent.preview_only {
                    self.open_raw(path);
                    return;
                }
                if intent.session != self.workspace.as_ref().map(|w| w.session) {
                    self.error = Some("The project changed while choosing media. Import it again in the current project.".into());
                    return;
                }
                self.submit(ProjectRequest::Import {
                    path,
                    media: intent.media,
                    ownership: if intent.linked {
                        OriginalOwnership::Linked { bookmark: None }
                    } else {
                        OriginalOwnership::Managed
                    },
                });
            }
        }
    }

    fn insert(&mut self) {
        let (Some(workspace), Some(asset)) = (&self.workspace, &self.selected_source) else {
            self.message = Some("Choose a source to insert.".into());
            return;
        };
        let children = root_children(workspace);
        let index = self
            .selected_beat
            .as_ref()
            .and_then(|id| children.iter().position(|child| child == id))
            .map_or(children.len(), |n| n + 1);
        let request = ProjectRequest::Insert {
            expected_revision: workspace.document.revision_id().clone(),
            asset: asset.clone(),
            parent: workspace.document.root().clone(),
            index,
        };
        self.submit(request);
    }

    fn history(&mut self, redo: bool) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if (redo && !workspace.can_redo) || (!redo && !workspace.can_undo) {
            return;
        }
        let expected_revision = workspace.document.revision_id().clone();
        self.submit(if redo {
            ProjectRequest::Redo { expected_revision }
        } else {
            ProjectRequest::Undo { expected_revision }
        });
    }

    fn select_source(&mut self, id: AssetId) {
        self.selected_source = Some(id);
        self.raw_source = None;
        self.source_cursor = 0;
        self.view = View::Source;
        self.pane = Pane::Sources;
        self.reveal_source = true;
        self.request_picture(true);
    }

    fn rebuild_rows(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.beat_rows = Arc::new(Vec::new());
            self.source_rows = Arc::new(Vec::new());
            return;
        };
        let mut start = 0_u64;
        self.beat_rows = Arc::new(
            root_children(workspace)
                .iter()
                .map(|id| {
                    let node = &workspace.document.nodes()[id];
                    let frames = workspace
                        .plan
                        .node_duration(id)
                        .map_or(0, |d| d.frames() as u64);
                    let kind = match &node.kind {
                        NodeKind::Source { .. } => "Source",
                        NodeKind::Hold { .. } => "Hold",
                        NodeKind::Repeat { .. } => "Repeat",
                        NodeKind::Retime { .. } => "Retime",
                        NodeKind::Sequence { .. } => "Sequence",
                    };
                    let result = BeatRow {
                        id: id.clone(),
                        label: node.label.clone(),
                        kind,
                        start,
                        frames,
                    };
                    start = start.saturating_add(frames);
                    result
                })
                .collect(),
        );
        self.filter_sources();
    }

    fn filter_sources(&mut self) {
        let query = self.source_search.to_lowercase();
        self.source_rows = Arc::new(self.workspace.as_ref().map_or_else(Vec::new, |w| {
            w.sources
                .values()
                .filter(|s| s.label.to_lowercase().contains(&query))
                .map(|s| (s.asset.clone(), s.label.clone(), s.video_index.is_some()))
                .collect()
        }));
    }

    fn action(&mut self, action: Action, context: &egui::Context) {
        match action {
            Action::New => self.begin_dialog(DialogKind::CreateProject, context, false),
            Action::Open => self.begin_dialog(DialogKind::OpenProject, context, false),
            Action::Import => self.begin_dialog(DialogKind::ImportMedia, context, false),
            Action::Insert => self.insert(),
            Action::Undo => self.history(false),
            Action::Redo => self.history(true),
            Action::Pane { reverse } => {
                self.pane = self.pane.cycle(reverse);
                self.bindings.clear();
                context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
            }
            Action::Step { forward, count } => {
                match self.view {
                    View::Source => {
                        self.source_cursor = navigation::boundary_step(
                            self.source_cursor,
                            self.source_length(),
                            forward,
                            count,
                        )
                    }
                    View::Sequence => {
                        self.sequence_cursor = navigation::boundary_step(
                            self.sequence_cursor,
                            self.sequence_length(),
                            forward,
                            count,
                        )
                    }
                }
                self.request_picture(false);
            }
            Action::First | Action::Last => {
                let end = action == Action::Last;
                match self.view {
                    View::Source => self.source_cursor = if end { self.source_length() } else { 0 },
                    View::Sequence => {
                        self.sequence_cursor = if end { self.sequence_length() } else { 0 }
                    }
                }
                self.request_picture(false);
            }
            Action::Beat { forward, count } => {
                if self.pane == Pane::Sources {
                    let sources = Arc::clone(&self.source_rows);
                    if !sources.is_empty() {
                        let index = self
                            .selected_source
                            .as_ref()
                            .and_then(|id| sources.iter().position(|source| &source.0 == id))
                            .unwrap_or(0);
                        let next = navigation::boundary_step(
                            index as u64,
                            sources.len().saturating_sub(1) as u64,
                            forward,
                            count,
                        ) as usize;
                        self.select_source(sources[next].0.clone());
                    }
                } else {
                    let beats = Arc::clone(&self.beat_rows);
                    if !beats.is_empty() {
                        let index = self
                            .selected_beat
                            .as_ref()
                            .and_then(|id| beats.iter().position(|beat| &beat.id == id))
                            .unwrap_or(0);
                        let next = navigation::boundary_step(
                            index as u64,
                            beats.len().saturating_sub(1) as u64,
                            forward,
                            count,
                        ) as usize;
                        self.selected_beat = Some(beats[next].id.clone());
                        self.sequence_cursor = beats[next].start;
                        self.view = View::Sequence;
                        self.reveal_beat = true;
                        self.request_picture(true);
                    }
                }
            }
            Action::Search => {
                self.pane = Pane::Sources;
                context.memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_ID)));
                context.input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, egui::Event::Text(text) if text == "/"))
                });
            }
            Action::Command => {
                self.command_open = true;
                self.command.clear();
                context.memory_mut(|m| m.request_focus(egui::Id::new(COMMAND_ID)));
                context.input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, egui::Event::Text(text) if text == ":"))
                });
            }
            Action::Escape => {
                self.command_open = false;
                self.help_open = false;
                self.bindings.clear();
            }
            Action::OfferInsert => {
                if self.view == View::Source {
                    self.message = Some("Source browsing preserves the original. Use Insert source (⌘Return) to begin editing it in the sequence.".into());
                }
            }
        }
    }

    fn keyboard(&mut self, context: &egui::Context) -> Option<(TextAction, bool)> {
        let events = context.input(|input| input.events.clone());
        let ime_event = events.iter().any(|e| matches!(e, egui::Event::Ime(_)));
        for event in &events {
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                    self.ime_composing = !text.is_empty()
                }
                egui::Event::Ime(egui::ImeEvent::Commit(_)) => self.ime_composing = false,
                egui::Event::WindowFocused(false) => {
                    self.ime_composing = false;
                    self.bindings.clear();
                }
                _ => {}
            }
        }
        if self.dialogs.is_open() || context.any_popup_open() {
            self.bindings.clear();
            return None;
        }
        if self.help_open {
            if context.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.help_open = false;
            }
            self.bindings.clear();
            return None;
        }
        let mut text_result = None;
        for event in events {
            if let egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                repeat,
                ..
            } = event
            {
                let focused = context.memory(|m| {
                    m.has_focus(egui::Id::new(SEARCH_ID)) || m.has_focus(egui::Id::new(COMMAND_ID))
                });
                let ime = self.ime_composing || ime_event;
                if let Some(text_action) = navigation::text_action(key, modifiers, focused, ime) {
                    text_result = Some((
                        text_action,
                        context.memory(|m| m.has_focus(egui::Id::new(COMMAND_ID))),
                    ));
                    context.input_mut(|i| {
                        i.consume_key(modifiers, key);
                    });
                    continue;
                }
                if text_result.is_some() {
                    continue;
                }
                if repeat && (modifiers.command || key == egui::Key::U) {
                    continue;
                }
                let before = self.bindings.pending();
                if let Some(action) = self.bindings.key(key, modifiers, focused, ime) {
                    self.action(action, context);
                    context.input_mut(|i| {
                        i.consume_key(modifiers, key);
                    });
                } else if before != self.bindings.pending() {
                    context.input_mut(|i| {
                        i.consume_key(modifiers, key);
                    });
                }
            }
        }
        text_result
    }

    fn run_command(&mut self, context: &egui::Context) {
        let command = self.command.trim().trim_start_matches(':').to_lowercase();
        self.command_open = false;
        match command.as_str() {
            "insert" => self.insert(),
            "undo" => self.history(false),
            "redo" => self.history(true),
            "new" => self.begin_dialog(DialogKind::CreateProject, context, false),
            "open" => self.begin_dialog(DialogKind::OpenProject, context, false),
            "import" => self.begin_dialog(DialogKind::ImportMedia, context, false),
            "source" => {
                if self.view != View::Source {
                    self.view = View::Source;
                    self.request_picture(true);
                }
            }
            "sequence" => {
                if self.workspace.is_none() {
                    self.error = Some("Create or open a project to inspect its sequence.".into());
                } else if self.view != View::Sequence {
                    self.view = View::Sequence;
                    self.request_picture(true);
                }
            }
            "help" => self.help_open = true,
            "" => {}
            _ => {
                self.error = Some(format!(
                    "Unknown command: {command}. Available: insert, undo, redo, new, open, import, source, sequence, help."
                ))
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("workspace-header")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("DEADPAN");
                    ui.separator();
                    let ready = !self.service.is_busy() && !self.dialogs.is_open();
                    ui.add_enabled_ui(ready, |ui| {
                        ui.menu_button("File", |ui| {
                            for (label, action) in [
                                ("New project…  ⌘N", Action::New),
                                ("Open project…  ⌘O", Action::Open),
                                ("Import media…  ⌘I", Action::Import),
                            ] {
                                let enabled = action != Action::Import
                                    || (self.workspace.is_some() && !self.importing());
                                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                    self.action(action, ui.ctx());
                                    ui.close();
                                }
                            }
                            if ui
                                .add_enabled(
                                    self.workspace.is_some(),
                                    egui::Button::new("Close project"),
                                )
                                .clicked()
                            {
                                self.submit(ProjectRequest::Close);
                                ui.close();
                            }
                        });
                        let undo = self.workspace.as_ref().is_some_and(|w| w.can_undo);
                        let redo = self.workspace.as_ref().is_some_and(|w| w.can_redo);
                        if ui
                            .add_enabled(undo, egui::Button::new("Undo"))
                            .on_hover_text("Undo · ⌘Z or u")
                            .clicked()
                        {
                            self.history(false);
                        }
                        if ui
                            .add_enabled(redo, egui::Button::new("Redo"))
                            .on_hover_text("Redo · ⌘Shift Z or Ctrl R")
                            .clicked()
                        {
                            self.history(true);
                        }
                    });
                    if self.service.is_busy() {
                        ui.spinner();
                    }
                    ui.separator();
                    ui.label(
                        self.workspace
                            .as_ref()
                            .and_then(|w| w.path.file_stem())
                            .map_or_else(
                                || "No project".into(),
                                |name| name.to_string_lossy().into_owned(),
                            ),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Keys").clicked() {
                            self.help_open = !self.help_open;
                        }
                        if self.workspace.is_some() {
                            ui.weak("Saved locally");
                        }
                    });
                });
            });
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("workspace-status").resizable(false).show(ui, |ui| {
            if self.command_open {
                ui.horizontal(|ui| { ui.strong(":"); ui.add(egui::TextEdit::singleline(&mut self.command).id(egui::Id::new(COMMAND_ID)).desired_width(f32::INFINITY).hint_text("insert · undo · redo · source · sequence · help")); retain_text_escape(ui, COMMAND_ID); });
            } else {
                ui.horizontal_wrapped(|ui| {
                    let text = ui.memory(|m| m.has_focus(egui::Id::new(SEARCH_ID)));
                    ui.strong(format!("{} · {}", if text { "TEXT" } else { "NORMAL" }, if self.view == View::Source { "Source" } else { "Sequence" }));
                    ui.separator();
                    let (cursor, length) = if self.view == View::Source { (self.source_cursor, self.source_length()) } else { (self.sequence_cursor, self.sequence_length()) };
                    ui.monospace(format!("Boundary {cursor} / {length}"));
                    if self.presentation.loading() || self.presentation.needs_render() { ui.spinner(); ui.weak("Updating picture…"); }
                    let pending = self.bindings.pending(); if !pending.is_empty() { ui.monospace(format!("Pending: {pending}")); }
                });
                if let Some(error) = self.error.as_deref().or(self.project_error.as_deref()).or(self.presentation.error()) { ui.colored_label(ui.visuals().error_fg_color, format!("Could not complete action: {error}")); }
                else if let Some(message) = &self.message { ui.weak(message); }
                else { ui.weak("h / l: frame · j / k: source or beat · gg / G: start / end · Tab: pane · :help"); }
            }
        });
    }

    fn sources(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("workspace-sources")
            .default_size(224.0)
            .min_size(170.0)
            .max_size(340.0)
            .show(ui, |ui| {
                let heading = ui.strong(if self.pane == Pane::Sources {
                    "Sources · focused"
                } else {
                    "Sources"
                });
                if pane_focus(ui, Pane::Sources, heading.rect, "Sources pane").has_focus() {
                    self.pane = Pane::Sources;
                }
                ui.add_space(6.0);
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.source_search)
                        .id(egui::Id::new(SEARCH_ID))
                        .hint_text("Find a source  /")
                        .desired_width(f32::INFINITY),
                );
                if search.has_focus() {
                    self.pane = Pane::Sources;
                }
                retain_text_escape(ui, SEARCH_ID);
                if search.changed() {
                    self.filter_sources();
                }
                ui.add_space(8.0);
                let sources = Arc::clone(&self.source_rows);
                let height = (ui.available_height() - 180.0).max(44.0);
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("source-list")
                    .max_height(height);
                if std::mem::take(&mut self.reveal_source)
                    && let Some(index) = sources
                        .iter()
                        .position(|s| Some(&s.0) == self.selected_source.as_ref())
                {
                    scroll = scroll.vertical_scroll_offset(
                        index as f32 * (48.0 + ui.spacing().item_spacing.y),
                    );
                }
                scroll.show_rows(ui, 48.0, sources.len(), |ui, range| {
                    for index in range {
                        let (asset, label, video) = &sources[index];
                        let response = ui.add_sized(
                            [ui.available_width(), 44.0],
                            egui::Button::new(format!(
                                "{}\n{}",
                                label,
                                if *video {
                                    "Video source"
                                } else {
                                    "Audio source"
                                }
                            ))
                            .selected(self.selected_source.as_ref() == Some(asset)),
                        );
                        if response.clicked() {
                            self.select_source(asset.clone());
                            ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sources)));
                        }
                    }
                });
                if sources.is_empty() {
                    ui.weak(if self.source_search.is_empty() {
                        "Imported sources appear here."
                    } else {
                        "No matching sources."
                    });
                }
                ui.add_space(12.0);
                ui.separator();
                let available = self.workspace.is_some()
                    && !self.service.is_busy()
                    && !self.dialogs.is_open()
                    && !self.importing();
                if ui
                    .add_enabled(available, egui::Button::new("Import media…  ⌘I"))
                    .clicked()
                {
                    self.begin_dialog(DialogKind::ImportMedia, ui.ctx(), false);
                }
                ui.collapsing("Import options", |ui| {
                    ui.checkbox(&mut self.audio_import, "Audio file (first track)");
                    ui.checkbox(&mut self.linked_import, "Link to original location");
                    ui.weak(if self.linked_import {
                        "Keep the original at its current location."
                    } else {
                        "Retain a copy inside the project."
                    });
                });
                if let Some(import) = &self.import {
                    ui.add_space(8.0);
                    let label = match import.stage {
                        ImportStage::Retaining => "Retaining original…",
                        ImportStage::Decoding => "Reading picture and audio…",
                        ImportStage::PreparingInsertion => "Preparing insertion…",
                        ImportStage::Registering => "Adding source…",
                        ImportStage::Complete => "Source ready",
                        ImportStage::Cancelled => "Import cancelled",
                        ImportStage::Failed => "Import failed",
                    };
                    ui.label(label);
                    if let Some(error) = &import.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if matches!(
                        import.stage,
                        ImportStage::Retaining
                            | ImportStage::Decoding
                            | ImportStage::PreparingInsertion
                            | ImportStage::Registering
                    ) && ui.button("Cancel import").clicked()
                    {
                        self.submit(ProjectRequest::CancelImport);
                    }
                }
            });
    }

    fn timeline(&mut self, ui: &mut egui::Ui) {
        let beats = Arc::clone(&self.beat_rows);
        egui::Panel::bottom("workspace-sequence").default_size(140.0).min_size(118.0).max_size(230.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                let heading = ui.strong(if self.pane == Pane::Sequence { "Sequence · focused" } else { "Sequence" });
                if pane_focus(ui, Pane::Sequence, heading.rect, "Sequence pane").has_focus() { self.pane = Pane::Sequence; }
                ui.weak(format!("{} beats · {} frames", beats.len(), self.sequence_length()));
                if let Some(w) = &self.workspace { let basis = w.document.presentation_basis(); ui.weak(format!("{} × {} · {}/{} fps", basis.width, basis.height, basis.frame_rate.numerator(), basis.frame_rate.denominator())); }
            });
            ui.add_space(6.0);
            if beats.is_empty() { ui.weak("Choose a source, then insert it into this sequence. Browsing never changes the edit."); return; }
            let selected = self.selected_beat.clone();
            let mut scroll = egui::ScrollArea::horizontal().id_salt("beat-strip");
            if std::mem::take(&mut self.reveal_beat) && let Some(index) = beats.iter().position(|b| Some(&b.id) == selected.as_ref()) { scroll = scroll.horizontal_scroll_offset(index as f32 * BEAT_WIDTH); }
            scroll.show_viewport(ui, |ui, viewport| {
                ui.set_min_width(beats.len() as f32 * BEAT_WIDTH);
                let start = (viewport.min.x / BEAT_WIDTH).floor().max(0.0) as usize;
                let end = ((viewport.max.x / BEAT_WIDTH).ceil() as usize + 1).min(beats.len());
                let origin = ui.min_rect().min;
                for (index, beat) in beats.iter().enumerate().take(end).skip(start) {
                    let rect = egui::Rect::from_min_size(origin + egui::vec2(index as f32 * BEAT_WIDTH, 0.0), egui::vec2(BEAT_WIDTH - 8.0, 66.0));
                    let response = ui.put(rect, egui::Button::new(format!("{}\n{} · {} frames", beat.label, beat.kind, beat.frames)).selected(selected.as_ref() == Some(&beat.id)));
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Beat {}: {}, {}, {} frames, starts at frame {}", index + 1, beat.label, beat.kind, beat.frames, beat.start)));
                    if response.clicked() { self.selected_beat = Some(beat.id.clone()); self.sequence_cursor = beat.start; self.view = View::Sequence; self.pane = Pane::Sequence; self.request_picture(true); ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sequence))); }
                }
                ui.allocate_space(egui::vec2(1.0, 68.0));
            });
        });
    }

    fn viewer(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (label, view) in [("Source", View::Source), ("Sequence", View::Sequence)] {
                    if ui.add_enabled(view == View::Source || self.workspace.is_some(), egui::Button::new(label).selected(self.view == view)).clicked() { if self.view != view { self.view = view; self.request_picture(true); } self.pane = Pane::Viewer; ui.memory_mut(|m| m.request_focus(pane_id(Pane::Viewer))); }
                }
                if self.view == View::Source {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(self.workspace.is_some() && self.selected_source.is_some() && !self.service.is_busy(), egui::Button::new("Insert source  ⌘↩")).on_hover_text("Insert the whole source after the selected beat, or at sequence end. This creates an undoable edit.").clicked() { self.insert(); }
                    });
                }
            });
            ui.add_space(6.0);
            let title = if self.view == View::Sequence { "Current sequence".to_owned() } else {
                self.workspace.as_ref().and_then(|w| self.selected_source.as_ref().and_then(|id| w.sources.get(id))).map(|s| s.label.clone())
                    .or_else(|| self.raw_source.as_ref().and_then(|p| p.file_name()).map(|p| p.to_string_lossy().into_owned())).unwrap_or_else(|| "Source viewer".into())
            };
            let title = ui.strong(title);
            if let Some(summary) = &self.summary {
                title.on_hover_text(format!(
                    "{} × {} · {}\n{} measured frames\nOriginal timestamp interval: {}..{} in {}/{} s units",
                    summary.info.width, summary.info.height, summary.info.codec,
                    summary.frame_count, summary.first_pts, summary.terminal_pts,
                    summary.info.time_base_num, summary.info.time_base_den
                ));
            }
            let available = egui::vec2(ui.available_width().max(1.0), (ui.available_height() - 76.0).max(80.0));
            let (_, rect) = ui.allocate_space(available);
            let response = pane_focus(ui, Pane::Viewer, rect, "Picture viewer pane");
            if response.has_focus() { self.pane = Pane::Viewer; }
            ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(13, 15, 18));
            let aspect = self.presentation.picture().and_then(|p| p.canvas).map(|(w,h)| w as f32 / h as f32);
            let canvas = aspect.map_or(rect, |aspect| fit_rect(rect, aspect));
            self.render_picture(ui.ctx(), canvas.size());
            let displayed_label = self.presentation.displayed_label();
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, displayed_label.as_deref().unwrap_or("No picture displayed")));
            if self.presentation.has_displayed() && !(self.view == View::Sequence && self.sequence_length() == 0) {
                if let Some(target) = &self.target { ui.painter().image(target.texture, canvas, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE); }
                else { ui.painter().rect_filled(canvas, 0.0, egui::Color32::BLACK); }
            } else {
                let message = if self.presentation.loading() { "Preparing picture…" } else if self.presentation.error().is_some() { "Picture unavailable" } else if self.workspace.is_none() && self.raw_source.is_none() { "Create a project to begin" } else if self.view == View::Sequence { "Your sequence is empty" } else if self.selected_source.is_some() && self.source_length() == 0 { "Audio source · no picture" } else { "Choose a source to preview" };
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, message, egui::FontId::proportional(20.0), egui::Color32::from_gray(180));
            }
            if self.pane == Pane::Viewer { ui.painter().rect_stroke(rect, 4.0, ui.visuals().selection.stroke, egui::StrokeKind::Inside); }
            ui.add_space(8.0);
            if let Some(label) = displayed_label {
                let label = ui.weak(label);
                if let Some(source_frame) = self.presentation.displayed_source_frame() {
                    label.on_hover_text(format!("Original source frame {}", u128::from(source_frame.0) + 1));
                }
            }
            ui.horizontal_wrapped(|ui| {
                if self.workspace.is_none() && self.raw_source.is_none() {
                    if ui.button("New project…").clicked() { self.begin_dialog(DialogKind::CreateProject, ui.ctx(), false); }
                    if ui.button("Open project…").clicked() { self.begin_dialog(DialogKind::OpenProject, ui.ctx(), false); }
                    if ui.button("Preview a source…").clicked() { self.begin_dialog(DialogKind::ImportMedia, ui.ctx(), true); }
                } else {
                    for (label, action) in [("Start", Action::First), ("Previous", Action::Step { forward: false, count: 1 }), ("Next", Action::Step { forward: true, count: 1 }), ("End", Action::Last)] {
                        if ui.button(label).clicked() { self.action(action, ui.ctx()); }
                    }
                    ui.weak("Frame inspection · playback is not yet available");
                }
            });
        });
    }

    fn forget_target(&mut self) {
        if let Some(registered) = self.target.take() {
            self.render_state
                .renderer
                .write()
                .free_texture(&registered.texture);
        }
    }

    fn render_picture(&mut self, context: &egui::Context, size: egui::Vec2) {
        if !self.presentation.can_render() {
            return;
        }
        let Some(picture) = self.presentation.picture() else {
            return;
        };
        if picture.frame.is_none() {
            if self.presentation.needs_render() {
                self.forget_target();
                self.presentation.presented();
                context.request_repaint();
            }
            return;
        }
        match self.renderer.is_idle() {
            Ok(false) => {
                context.request_repaint_after(Duration::from_millis(16));
                return;
            }
            Err(error) => {
                self.presentation
                    .render_failed(format!("Preview renderer: {error}"));
                return;
            }
            Ok(true) => {}
        }
        let (width, height) = target_size(size, context.pixels_per_point());
        let resize = self
            .target
            .as_ref()
            .is_none_or(|t| t.target.width() != width || t.target.height() != height);
        if !self.presentation.needs_render() && !resize {
            return;
        }
        // Keep the visible texture and its identity until its replacement is
        // successfully rendered. A failed resize must not label a blank target
        // as the previous picture.
        let replacement = if resize {
            Some(match self.renderer.create_target(width, height) {
                Ok(target) => target,
                Err(error) => {
                    self.presentation
                        .render_failed(format!("Preview renderer: {error}"));
                    return;
                }
            })
        } else {
            None
        };
        let picture = self.presentation.picture().expect("picture checked");
        let frame = picture.frame.as_ref().expect("frame checked");
        match self.renderer.render(
            frame,
            replacement
                .as_ref()
                .unwrap_or_else(|| &self.target.as_ref().expect("target exists").target),
            FitMode::Fit,
        ) {
            Ok(_) => {
                if let Some(target) = replacement {
                    let texture = self.render_state.renderer.write().register_native_texture(
                        &self.render_state.device,
                        target.display_view(),
                        eframe::wgpu::FilterMode::Linear,
                    );
                    self.forget_target();
                    self.target = Some(RegisteredTarget { target, texture });
                }
                self.presentation.presented();
                context.request_repaint_after(Duration::from_millis(16));
            }
            Err(error) => {
                self.presentation
                    .render_failed(format!("Preview renderer: {error}"));
            }
        }
    }

    fn help(&mut self, context: &egui::Context) {
        egui::Window::new("Keyboard and context").open(&mut self.help_open).collapsible(false).resizable(false).show(context, |ui| {
            for (key, description) in [("⌘N / ⌘O / ⌘I", "New project / open project / import"), ("h / l · Left / Right", "Previous / next frame; a count such as 12l works"), ("j / k", "Next / previous source or sequence beat"), ("gg / G · Home / End", "Start / end boundary"), ("Tab / Shift Tab", "Cycle Sources, Viewer and Sequence panes"), ("/ · Escape", "Find a source / leave text entry"), ("⌘Return · :insert", "Insert the whole source after the selected beat"), ("⌘Z / ⌘Shift Z · u / Ctrl R", "Undo / redo"), (":source / :sequence", "Change the viewer context")] {
                ui.horizontal(|ui| { ui.monospace(key); ui.label(description); });
            }
            ui.separator();
            ui.label("Source browsing preserves original media. Every insertion is a reversible sequence edit. The cursor is a boundary; at the end, the viewer shows the preceding final frame.");
            ui.weak("Playback, range trimming and the remaining editing operators are still in development.");
        });
    }
}

impl eframe::App for DeadpanApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.close_pending |= context.input(|i| i.viewport().close_requested());
        let previous_pane = self.pane;
        self.receive();
        if self.close_pending {
            if self.service.is_busy() {
                context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                context.request_repaint_after(Duration::from_millis(16));
                self.message = Some("Finishing the project command before closing…".into());
                ui.disable();
            } else {
                context.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        } else {
            self.receive_dialog();
        }
        if self.pane != previous_pane {
            context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
        }
        let text_result = if self.close_pending {
            None
        } else {
            self.keyboard(&context)
        };
        self.header(ui);
        self.footer(ui);
        self.sources(ui);
        self.timeline(ui);
        self.viewer(ui);
        self.help(&context);
        if let Some((action, command)) = text_result {
            if command && action == TextAction::Open {
                self.run_command(&context);
            }
            self.command_open = false;
            context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
        }
        if let Some(frames) = self.smoke_frames.as_mut() {
            *frames += 1;
            if *frames >= 3 {
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                context.request_repaint();
            }
        }
    }
    fn on_exit(&mut self) {
        self.service.shutdown();
        self.worker.shutdown();
        self.forget_target();
        self.exited.set(true);
    }
}
impl Drop for DeadpanApp {
    fn drop(&mut self) {
        self.service.shutdown();
        self.worker.shutdown();
        self.forget_target();
    }
}

struct BeatRow {
    id: NodeId,
    label: String,
    kind: &'static str,
    start: u64,
    frames: u64,
}
type SourceRow = (AssetId, String, bool);
fn retain_text_escape(ui: &egui::Ui, id: &str) {
    // Keep focus through the input pass so the application can leave text mode
    // after the TextEdit has processed this frame's final text/IME events.
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            egui::Id::new(id),
            egui::EventFilter {
                escape: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                tab: false,
            },
        )
    });
}
fn root_children(workspace: &Workspace) -> &[NodeId] {
    match &workspace.document.nodes()[workspace.document.root()].kind {
        NodeKind::Sequence { children } => children,
        _ => &[],
    }
}
fn pane_id(pane: Pane) -> egui::Id {
    egui::Id::new(match pane {
        Pane::Sources => "sources-pane",
        Pane::Viewer => "viewer-pane",
        Pane::Sequence => "sequence-pane",
    })
}
fn pane_focus(ui: &egui::Ui, pane: Pane, rect: egui::Rect, label: &str) -> egui::Response {
    let response = ui.interact(rect, pane_id(pane), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.clicked() {
        response.request_focus();
    }
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            pane_id(pane),
            egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        )
    });
    response
}
fn fit_rect(rect: egui::Rect, aspect: f32) -> egui::Rect {
    let width = rect.width().min(rect.height() * aspect);
    egui::Rect::from_center_size(rect.center(), egui::vec2(width, width / aspect))
}
fn target_size(size: egui::Vec2, pixels_per_point: f32) -> (u32, u32) {
    let width = f64::from(size.x.max(1.0)) * f64::from(pixels_per_point);
    let height = f64::from(size.y.max(1.0)) * f64::from(pixels_per_point);
    let scale = (MAX_TARGET_PIXELS / (width * height))
        .sqrt()
        .min(1.0)
        .min(2048.0 / width)
        .min(2048.0 / height);
    (
        (width * scale).floor().max(1.0) as u32,
        (height * scale).floor().max(1.0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ui(context: &egui::Context, input: egui::RawInput, draw: impl FnMut(&mut egui::Ui)) {
        let mut output = context.run_ui(input, draw);
        output.textures_delta.clear();
    }

    fn key_event(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn text_exit_keeps_final_text_and_returns_to_a_real_pane() {
        for key in [egui::Key::Enter, egui::Key::Escape] {
            let context = egui::Context::default();
            let mut command = String::new();
            let mut draw = |ui: &mut egui::Ui| {
                ui.add(egui::TextEdit::singleline(&mut command).id(egui::Id::new(COMMAND_ID)));
                retain_text_escape(ui, COMMAND_ID);
                let heading = ui.label("Sources");
                pane_focus(ui, Pane::Sources, heading.rect, "Sources pane");
            };
            run_ui(&context, egui::RawInput::default(), |ui| {
                ui.memory_mut(|m| m.request_focus(egui::Id::new(COMMAND_ID)));
                draw(ui);
            });
            run_ui(&context, egui::RawInput::default(), &mut draw);
            let mut captured = None;
            run_ui(
                &context,
                egui::RawInput {
                    events: vec![egui::Event::Text("source".into()), key_event(key)],
                    ..Default::default()
                },
                |ui| {
                    captured = navigation::text_action(
                        key,
                        egui::Modifiers::NONE,
                        ui.memory(|m| m.has_focus(egui::Id::new(COMMAND_ID))),
                        false,
                    );
                    assert!(
                        captured.is_some(),
                        "Escape must not surrender text focus before routing"
                    );
                    ui.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, key);
                    });
                    draw(ui);
                    ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sources)));
                },
            );
            run_ui(&context, egui::RawInput::default(), &mut draw);
            assert_eq!(
                command, "source",
                "final text is applied before leaving the field"
            );
            assert_eq!(
                captured,
                Some(if key == egui::Key::Enter {
                    TextAction::Open
                } else {
                    TextAction::Leave
                })
            );
            assert!(context.memory(|m| m.has_focus(pane_id(Pane::Sources))));
        }
    }

    #[test]
    fn pane_focus_survives_egui_arrow_routing_and_cycles_explicitly() {
        let context = egui::Context::default();
        let draw = |ui: &mut egui::Ui| {
            for pane in [Pane::Sources, Pane::Viewer, Pane::Sequence] {
                let label = ui.label(format!("{pane:?}"));
                pane_focus(ui, pane, label.rect, "Pane");
            }
        };
        run_ui(&context, egui::RawInput::default(), |ui| {
            ui.memory_mut(|m| m.request_focus(pane_id(Pane::Viewer)));
            draw(ui);
        });
        run_ui(&context, egui::RawInput::default(), draw);
        run_ui(
            &context,
            egui::RawInput {
                events: vec![key_event(egui::Key::ArrowDown)],
                ..Default::default()
            },
            |ui| {
                assert!(ui.memory(|m| m.has_focus(pane_id(Pane::Viewer))));
                draw(ui);
            },
        );
        run_ui(
            &context,
            egui::RawInput {
                events: vec![key_event(egui::Key::Tab)],
                ..Default::default()
            },
            |ui| {
                assert!(ui.memory(|m| m.has_focus(pane_id(Pane::Viewer))));
                ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                });
                ui.memory_mut(|m| m.request_focus(pane_id(Pane::Viewer.cycle(false))));
                draw(ui);
            },
        );
        run_ui(&context, egui::RawInput::default(), draw);
        assert!(context.memory(|m| m.has_focus(pane_id(Pane::Sequence))));
    }
    #[test]
    fn preview_target_bounds_retina_and_preserves_canvas_geometry() {
        let (width, height) = target_size(egui::vec2(6000.0, 4000.0), 2.0);
        assert!(u64::from(width) * u64::from(height) <= MAX_TARGET_PIXELS as u64);
        assert!(width <= 2048 && height <= 2048);
        assert!((f64::from(width) / f64::from(height) - 1.5).abs() < 0.002);
        assert_eq!(target_size(egui::Vec2::ZERO, 1.0), (1, 1));
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        assert_eq!(fit_rect(rect, 16.0 / 9.0).size(), egui::vec2(800.0, 450.0));
        assert_eq!(fit_rect(rect, 0.5).size(), egui::vec2(300.0, 600.0));
    }
}
