use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use deadpan_core::{AssetId, NodeId, NodeKind, ProjectFrame, RevisionId, SourceFrameId};
use deadpan_render::{FitMode, PictureRenderer, RenderTarget};
use deadpan_store::original_media::OriginalOwnership;
use deadpan_store::single_source::SingleSourceState;
use eframe::{egui, egui_wgpu};

use crate::dialogs::{DialogKind, Dialogs};
use crate::navigation::{self, Action, BeatEdit, Bindings, Pane, TextAction};
use crate::presentation::Presentation;
use crate::project::{
    ImportMedia, ImportStage, ImportStatus, ProjectEdit, ProjectRequest, ProjectService, Workspace,
};
use crate::worker::{PreviewWorker, ProjectView, SourceSummary, Ticket, Work};

mod cards;
mod inspector;
mod selection;
mod style;

const SEARCH_ID: &str = "source-search";
const COMMAND_ID: &str = "command-input";
const MAX_TARGET_PIXELS: f64 = 1920.0 * 1080.0;
const SOURCE_INSERT_HINT: &str = "The Original stays intact. Switch to Your edit (:sequence) to reshape it, or reuse the full Original with ⌘Return (:insert).";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Source,
    Sequence,
}

impl View {
    fn on_open(profile: Option<&SingleSourceState>) -> Self {
        if matches!(profile, Some(SingleSourceState::Ready { .. })) {
            Self::Sequence
        } else {
            Self::Source
        }
    }
    fn after_completion(self, completion: &selection::Completion) -> Self {
        if *completion == selection::Completion::Edit {
            Self::Sequence
        } else {
            self
        }
    }

    fn set(&mut self, next: Self, message: &mut Option<String>) {
        if *self != next && message.as_deref() == Some(SOURCE_INSERT_HINT) {
            *message = None;
        }
        *self = next;
    }
}

struct RegisteredTarget {
    target: RenderTarget,
    texture: egui::TextureId,
}
struct DialogIntent {
    session: Option<u64>,
    revision: Option<RevisionId>,
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
    sound_stream: Option<u32>,
    last_committed: Option<deadpan_core::RevisionId>,
    beat_rows: Arc<Vec<BeatRow>>,
    source_rows: Arc<Vec<SourceRow>>,
    sound_rows: Arc<Vec<SourceRow>>,
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
        style::apply(&context.egui_ctx);
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
            sound_stream: None,
            last_committed: None,
            beat_rows: Arc::new(Vec::new()),
            source_rows: Arc::new(Vec::new()),
            sound_rows: Arc::new(Vec::new()),
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
        self.bindings.clear();
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
        self.bindings.clear();
        self.clear_picture();
        self.summary = None;
        self.raw_source = Some(path.clone());
        self.view.set(View::Source, &mut self.message);
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
                self.bindings.clear();
                self.clear_picture();
                self.raw_source = None;
                self.selected_source = None;
                self.selected_beat = None;
                self.source_cursor = 0;
                self.sequence_cursor = 0;
                self.last_committed = None;
                self.source_search.clear();
                self.view.set(
                    View::on_open(
                        self.workspace
                            .as_ref()
                            .and_then(|workspace| workspace.single_source.as_ref()),
                    ),
                    &mut self.message,
                );
                self.summary = None;
                self.error = None;
            }
            if let Some(workspace) = &self.workspace {
                if self
                    .selected_source
                    .as_ref()
                    .is_none_or(|id| !workspace.sources.contains_key(id))
                {
                    self.selected_source = original_asset(workspace)
                        .cloned()
                        .or_else(|| workspace.sources.keys().next().cloned());
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
                self.bindings.clear();
                self.rebuild_rows();
            }
            let completion = selection::completion(
                update.committed.as_ref().map(|commit| &commit.revision),
                self.last_committed.as_ref(),
                completed,
            );
            let committed_selection = completion == selection::Completion::Edit;
            self.view
                .set(self.view.after_completion(&completion), &mut self.message);
            if let Some(commit) = update.committed.filter(|_| committed_selection) {
                self.last_committed = Some(commit.revision);
                self.bindings.clear();
                self.pane = Pane::Sequence;
                self.selected_beat = commit.selected_node;
                if let Some(beat) = self
                    .beat_rows
                    .iter()
                    .find(|b| Some(&b.id) == self.selected_beat.as_ref())
                {
                    self.sequence_cursor = beat.start;
                    self.reveal_beat = true;
                } else {
                    self.selected_beat = None;
                }
            } else if completion == selection::Completion::Registration
                && let Some(asset) = self.import.as_ref().and_then(|i| i.asset.clone())
                && let Some(asset) = registration_selection(
                    self.workspace
                        .as_ref()
                        .and_then(|workspace| workspace.single_source.as_ref()),
                    asset,
                )
            {
                self.bindings.clear();
                self.selected_source = Some(asset);
                self.source_cursor = 0;
                // Registration makes this source available without stealing
                // Sequence context after edits, history or navigation.
                self.reveal_source = true;
            }
            self.sequence_cursor = self.sequence_cursor.min(self.sequence_length());
            self.source_cursor = self.source_cursor.min(self.source_length());
            if self.view == View::Sequence && !committed_selection {
                self.reconcile_beat_selection();
            }
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
        if matches!(
            kind,
            DialogKind::CreateProject
                | DialogKind::InitializeSource
                | DialogKind::ImportSound
                | DialogKind::ImportMedia
        ) && !preview_only
            && self.importing()
        {
            self.message =
                Some("Finish or cancel the current import before choosing another source.".into());
            return;
        }
        if matches!(
            kind,
            DialogKind::InitializeSource | DialogKind::ImportSound | DialogKind::ImportMedia
        ) && !preview_only
            && self.workspace.is_none()
        {
            self.message = Some("Create or open a project before importing media.".into());
            return;
        }
        match self.dialogs.start(kind, context) {
            Ok(()) => {
                self.dialog_intent = Some(DialogIntent {
                    session: self.workspace.as_ref().map(|w| w.session),
                    revision: self
                        .workspace
                        .as_ref()
                        .map(|w| w.document.revision_id().clone()),
                    media: if kind == DialogKind::ImportSound || self.audio_import {
                        self.sound_stream.map_or(ImportMedia::FirstAudio, |stream| {
                            ImportMedia::Audio { stream }
                        })
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
                self.submit(ProjectRequest::CreateFromSource { path });
            }
            DialogKind::OpenProject => {
                self.submit(ProjectRequest::Open(path));
            }
            DialogKind::InitializeSource | DialogKind::ImportSound => {
                let Some(intent) = intent else {
                    return;
                };
                let (Some(expected_session), Some(expected_revision)) =
                    (intent.session, intent.revision)
                else {
                    return;
                };
                self.submit(if result.kind == DialogKind::InitializeSource {
                    ProjectRequest::InitializeSource {
                        expected_session,
                        expected_revision,
                        path,
                    }
                } else {
                    ProjectRequest::ImportSound {
                        expected_session,
                        expected_revision,
                        path,
                        stream: match intent.media {
                            ImportMedia::Audio { stream } => Some(stream),
                            ImportMedia::FirstAudio => None,
                            ImportMedia::Video => None,
                        },
                        ownership: if intent.linked {
                            OriginalOwnership::Linked { bookmark: None }
                        } else {
                            OriginalOwnership::Managed
                        },
                    }
                });
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
        let Some(workspace) = &self.workspace else {
            self.message = Some("Choose a source to insert.".into());
            return;
        };
        let Some(asset) = original_asset(workspace).or(self.selected_source.as_ref()) else {
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

    fn focused_workflow(&self) -> bool {
        self.workspace
            .as_ref()
            .is_none_or(|workspace| workspace.single_source.is_some())
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

    fn offer_insert(&mut self) {
        self.bindings.clear();
        self.error = None;
        self.message = Some(SOURCE_INSERT_HINT.into());
    }

    fn edit(&mut self, edit: BeatEdit) {
        self.bindings.clear();
        if self.view == View::Source {
            self.offer_insert();
            return;
        }
        let (Some(workspace), Some(node)) = (&self.workspace, &self.selected_beat) else {
            self.error = Some("Select a root beat in the sequence before editing.".into());
            return;
        };
        let node = node.clone();
        let edit = match edit {
            BeatEdit::Repeat(plays) => ProjectEdit::Repeat { node, plays },
            BeatEdit::WrapRepeat(plays) => ProjectEdit::WrapRepeat { node, plays },
            BeatEdit::Delete => ProjectEdit::Delete { node },
            BeatEdit::HoldDuration(duration) => ProjectEdit::HoldDuration { node, duration },
        };
        self.submit(ProjectRequest::Edit {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            edit,
        });
    }

    fn select_at_cursor(&mut self) {
        let selected = selection::at_boundary(&self.beat_rows, self.sequence_cursor)
            .map(|index| self.beat_rows[index].id.clone());
        if selected != self.selected_beat {
            self.selected_beat = selected;
            self.reveal_beat = true;
        }
    }

    fn reconcile_beat_selection(&mut self) {
        let selected = selection::after_refresh(
            &self.beat_rows,
            self.selected_beat.as_ref(),
            self.sequence_cursor,
        );
        let node = selected.map(|index| self.beat_rows[index].id.clone());
        if node != self.selected_beat {
            self.selected_beat = node;
            self.reveal_beat = true;
        }
        if let Some(index) = selected {
            let beat = &self.beat_rows[index];
            if self.sequence_cursor < beat.start || self.sequence_cursor > beat.start + beat.frames
            {
                self.sequence_cursor = beat.start;
            }
        }
    }

    fn open_command(&mut self, command: String, context: &egui::Context) {
        self.bindings.clear();
        self.command_open = true;
        self.command = command;
        context.memory_mut(|m| m.request_focus(egui::Id::new(COMMAND_ID)));
    }

    fn select_source(&mut self, id: AssetId) {
        if self
            .workspace
            .as_ref()
            .and_then(|workspace| original_asset(workspace))
            .is_some_and(|original| original != &id)
        {
            return;
        }
        self.bindings.clear();
        self.selected_source = Some(id);
        self.raw_source = None;
        self.source_cursor = 0;
        self.view.set(View::Source, &mut self.message);
        self.pane = Pane::Sources;
        self.reveal_source = true;
        self.request_picture(true);
    }

    fn rebuild_rows(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.beat_rows = Arc::new(Vec::new());
            self.source_rows = Arc::new(Vec::new());
            self.sound_rows = Arc::new(Vec::new());
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
                    let kind = if original_asset(workspace).is_some()
                        && matches!(node.kind, NodeKind::Source { .. })
                    {
                        "Original".into()
                    } else {
                        beat_kind(&node.kind)
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
        self.sound_rows = Arc::new(self.workspace.as_ref().map_or_else(Vec::new, |workspace| {
            workspace
                .sources
                .values()
                .filter(|source| {
                    source.receipt.snapshot().video().is_none()
                        && source.receipt.snapshot().audio().is_some()
                        && source.label.to_lowercase().contains(&query)
                })
                .map(|source| (source.asset.clone(), source.label.clone(), false))
                .collect()
        }));
        self.source_rows = Arc::new(self.workspace.as_ref().map_or_else(Vec::new, |w| {
            w.sources
                .values()
                .filter(|source| original_asset(w).is_none_or(|original| original == &source.asset))
                .filter(|s| original_asset(w).is_some() || s.label.to_lowercase().contains(&query))
                .map(|s| (s.asset.clone(), s.label.clone(), s.video_index.is_some()))
                .collect()
        }));
    }

    fn action(&mut self, action: Action, context: &egui::Context) {
        match action {
            Action::New => self.begin_dialog(DialogKind::CreateProject, context, false),
            Action::Open => self.begin_dialog(DialogKind::OpenProject, context, false),
            Action::Import => self.begin_dialog(self.import_dialog_kind(), context, false),
            Action::Insert => self.insert(),
            Action::Undo => self.history(false),
            Action::Redo => self.history(true),
            Action::Edit(edit) => self.edit(edit),
            Action::Invalid(error) => self.error = Some(error.into()),
            Action::Pane { reverse } => {
                self.pane = self.pane.cycle_visible(reverse, self.inspector_visible());
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
                if self.view == View::Sequence {
                    self.select_at_cursor();
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
                if self.view == View::Sequence {
                    self.select_at_cursor();
                }
                self.request_picture(false);
            }
            Action::Beat { forward, count } => {
                if self.pane == Pane::Sources && !self.focused_workflow() {
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
                    if let Some(next) = selection::step(
                        &beats,
                        self.selected_beat.as_ref(),
                        self.sequence_cursor,
                        forward,
                        count,
                    ) {
                        self.selected_beat = Some(beats[next].id.clone());
                        self.sequence_cursor = beats[next].start;
                        self.view.set(View::Sequence, &mut self.message);
                        self.reveal_beat = true;
                        self.request_picture(true);
                    }
                }
            }
            Action::Search => {
                if self.workspace.as_ref().is_some_and(|workspace| {
                    matches!(
                        workspace.single_source,
                        Some(SingleSourceState::AwaitingSource { .. })
                    )
                }) {
                    return;
                }
                self.pane = Pane::Sources;
                context.memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_ID)));
                context.input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, egui::Event::Text(text) if text == "/"))
                });
            }
            Action::Command => {
                self.open_command(String::new(), context);
                context.input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, egui::Event::Text(text) if text == ":"))
                });
            }
            Action::Help => {
                self.help_open = true;
                self.bindings.clear();
            }
            Action::Escape => {
                self.command_open = false;
                self.help_open = false;
                self.bindings.clear();
            }
            Action::OfferInsert => {
                if self.view == View::Source {
                    self.offer_insert();
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
        // Widgets resolve pointer-driven focus later in this frame. Do not
        // dispatch a destructive shortcut or submit the formerly focused
        // command against that old focus. Text events still reach the widgets.
        if pointer_focus_transition(&events) {
            self.bindings.clear();
            return None;
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
                let focused = text_input_active(context, false);
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
                if repeat && !navigation::allows_key_repeat(key, modifiers) {
                    continue;
                }
                if navigation::inspector_parameter_key(
                    key,
                    modifiers,
                    self.pane,
                    text_input_active(context, self.command_open),
                    ime,
                ) && self.open_inspector_parameter(context)
                {
                    context.input_mut(|input| {
                        input.consume_key(modifiers, key);
                    });
                    continue;
                }
                let before = self.bindings.pending();
                // Command mode remains text-only until the end-of-frame blur
                // handling closes it, even if a click has already moved focus.
                if let Some(action) = self.bindings.key(
                    key,
                    modifiers,
                    text_input_active(context, self.command_open),
                    ime,
                ) {
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
        let command = navigation::command::parse(&self.command);
        self.bindings.clear();
        self.command_open = false;
        match command {
            Ok(navigation::command::Entry::Action(action)) => self.action(action, context),
            Ok(navigation::command::Entry::Source) => {
                if self.view != View::Source {
                    self.view.set(View::Source, &mut self.message);
                    self.request_picture(true);
                }
            }
            Ok(navigation::command::Entry::Sequence) => {
                if self.workspace.is_none() {
                    self.error = Some("Create or open a project to inspect its sequence.".into());
                } else if self.view != View::Sequence {
                    self.view.set(View::Sequence, &mut self.message);
                    self.reconcile_beat_selection();
                    self.request_picture(true);
                }
            }
            Ok(navigation::command::Entry::Help) => self.help_open = true,
            Ok(navigation::command::Entry::Empty) => {}
            Err(error) => self.error = Some(error),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("workspace-header")
            .resizable(false)
            .frame(style::panel())
            .show(ui, |ui| {
                let title = self
                    .workspace
                    .as_ref()
                    .and_then(|w| w.path.file_stem())
                    .map_or_else(
                        || "No project".into(),
                        |name| name.to_string_lossy().into_owned(),
                    );
                ui.columns(3, |columns| {
                    columns[0].horizontal(|ui| {
                        ui.spacing_mut().button_padding.x = 6.0;
                        ui.label(egui::RichText::new("DEADPAN").size(14.0).strong());
                        let ready = !self.service.is_busy() && !self.dialogs.is_open();
                        ui.add_enabled_ui(ready, |ui| {
                            ui.menu_button("File", |ui| {
                                for (label, action) in [
                                    ("New project…  ⌘N", Action::New),
                                    ("Open project…  ⌘O", Action::Open),
                                    (
                                        match self.import_dialog_kind() {
                                            DialogKind::InitializeSource => "Choose Original…  ⌘I",
                                            DialogKind::ImportSound => "Add sound…  ⌘I",
                                            _ => "Import media…  ⌘I",
                                        },
                                        Action::Import,
                                    ),
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
                        });
                    });
                    columns[1].with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(egui::RichText::new(&title).color(style::MUTED))
                                .truncate(),
                        )
                        .on_hover_text(&title);
                    });
                    columns[2].with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .button("Keys  ?")
                                .on_hover_text("Keyboard reference · ? or :help")
                                .clicked()
                            {
                                self.help_open = !self.help_open;
                            }
                            let ready = !self.service.is_busy() && !self.dialogs.is_open();
                            if ui
                                .add_enabled(
                                    ready && self.workspace.as_ref().is_some_and(|w| w.can_redo),
                                    egui::Button::new("Redo  Ctrl R"),
                                )
                                .on_hover_text("Redo · ⌘Shift Z or Ctrl R")
                                .clicked()
                            {
                                self.history(true);
                            }
                            if ui
                                .add_enabled(
                                    ready && self.workspace.as_ref().is_some_and(|w| w.can_undo),
                                    egui::Button::new("Undo  u"),
                                )
                                .on_hover_text("Undo · ⌘Z or u")
                                .clicked()
                            {
                                self.history(false);
                            }
                            if self.service.is_busy() {
                                ui.spinner();
                                ui.weak("Working");
                            } else if self.workspace.is_some() {
                                ui.colored_label(style::SAVED, "Saved")
                                    .on_hover_text("Current committed revision is saved locally");
                            }
                        },
                    );
                });
            });
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("workspace-status").resizable(false).frame(style::panel()).show(ui, |ui| {
            let pending = self.bindings.pending();
            let mode = if self.command_open { "COMMAND" } else if text_input_active(ui.ctx(), false) { "TEXT" } else if pending.is_empty() { "NORMAL" } else { "PENDING" };
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(mode).monospace().strong());
                ui.separator();
                ui.label(egui::RichText::new(match (self.focused_workflow(), self.view) { (true, View::Source) => "ORIGINAL", (true, View::Sequence) => "YOUR EDIT", (false, View::Source) => "SOURCE", (false, View::Sequence) => "SEQUENCE" }).monospace());
                ui.separator();
                if self.view == View::Sequence {
                    if let Some(beat) = self.beat_rows.iter().find(|beat| Some(&beat.id) == self.selected_beat.as_ref()) {
                        ui.add(egui::Label::new(format!("{} · Root beat", beat.label)).truncate()).on_hover_text(format!("{} · {} · {} frames · Root beat", beat.label, beat.kind, beat.frames));
                    } else { ui.weak("No beat selected"); }
                } else { ui.weak("Unchanged source"); }
                ui.colored_label(style::LAVENDER, format!("Focus: {}", if self.pane == Pane::Sources && self.focused_workflow() { "Original / sounds" } else { pane_name(self.pane) }));
                if !pending.is_empty() { style::keycap(ui, &pending); }
                if let Some(hint) = self.bindings.pending_hint() { ui.label(egui::RichText::new(hint).size(11.0).color(style::LAVENDER)); }
            });
            if self.command_open {
                egui::Frame::new().fill(style::PANEL).stroke(egui::Stroke::new(1.0, style::LAVENDER)).corner_radius(4).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(":").monospace().color(style::LAVENDER));
                        ui.add(egui::TextEdit::singleline(&mut self.command).id(egui::Id::new(COMMAND_ID)).font(egui::TextStyle::Monospace).frame(egui::Frame::NONE).desired_width(f32::INFINITY).hint_text("repeat 3 · wrap-repeat 2 · hold-duration 11f · delete · help"));
                        retain_text_escape(ui, COMMAND_ID);
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    style::key_hint(ui, "Enter", "apply command");
                    style::key_hint(ui, "Esc", "cancel entry");
                    ui.weak("Whole project frames: 11f · repeat count: total plays");
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    let (cursor, length) = if self.view == View::Source { (self.source_cursor, self.source_length()) } else { (self.sequence_cursor, self.sequence_length()) };
                    ui.label(egui::RichText::new(format!("{} {cursor} / {length}", if self.view == View::Source { "Original video boundary" } else { "Edit boundary" })).monospace().color(style::CURSOR));
                    if self.presentation.loading() || self.presentation.needs_render() { ui.spinner(); ui.weak("Updating picture"); }
                    style::key_hint(ui, "h l", "frame");
                    if self.view == View::Sequence {
                        style::key_hint(ui, "j k", "beat");
                        style::key_hint(ui, "rr", "repeat");
                        style::key_hint(ui, "dd", "cut beat");
                        style::key_hint(ui, "u", "undo");
                    } else {
                        if self.focused_workflow() && !self.beat_rows.is_empty() { style::key_hint(ui, "j k", "edit beat"); }
                        style::key_hint(ui, ":sequence", if self.focused_workflow() { "Your edit" } else { "Sequence" });
                        if self.workspace.is_some() && self.selected_source.is_some() { style::key_hint(ui, "⌘↩", if self.focused_workflow() { "reuse Original" } else { "insert source" }); }
                    }
                    style::key_hint(ui, "Tab", "pane");
                    style::key_hint(ui, ":", "command");
                    style::key_hint(ui, "?", "keys");
                });
                if let Some(error) = self.error.as_deref().or(self.project_error.as_deref()).or(self.presentation.error()) { ui.colored_label(ui.visuals().error_fg_color, format!("Could not complete action: {error}")); }
                else if let Some(message) = &self.message { ui.label(egui::RichText::new(message).color(style::MUTED).size(12.0)); }
            }
        });
    }

    fn import_dialog_kind(&self) -> DialogKind {
        match self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.single_source.as_ref())
        {
            Some(SingleSourceState::AwaitingSource { .. }) => DialogKind::InitializeSource,
            Some(SingleSourceState::Ready { .. }) => DialogKind::ImportSound,
            None => DialogKind::ImportMedia,
        }
    }

    fn sources(&mut self, ui: &mut egui::Ui) {
        let layout = self.workspace_layout(ui);
        let profile = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.single_source.clone());
        let focused = self.focused_workflow();
        egui::Panel::left("workspace-sources")
            .resizable(false)
            .default_size(layout.sources)
            .min_size(layout.sources)
            .max_size(layout.sources)
            .frame(style::panel())
            .show(ui, |ui| {
                let heading = pane_heading(ui, if focused { "ORIGINAL" } else { "SOURCES" }, self.pane == Pane::Sources);
                if pane_focus(ui, Pane::Sources, heading.rect, if focused { "Original and sounds pane" } else { "Legacy sources pane" }).has_focus() {
                    self.pane = Pane::Sources;
                }
                ui.separator();
                egui::ScrollArea::vertical().id_salt("original-rail").show(ui, |ui| {
                    if matches!(profile, Some(SingleSourceState::AwaitingSource { .. })) {
                        ui.label("Project created. Original not ready.");
                        ui.weak("Choose a video to start with its full picture and sound.");
                        if ui.add_enabled(!self.importing() && !self.service.is_busy(), egui::Button::new("Choose Original…  ⌘I")).clicked() {
                            self.begin_dialog(DialogKind::InitializeSource, ui.ctx(), false);
                        }
                    } else if matches!(profile, Some(SingleSourceState::Ready { .. })) {
                        let sources = Arc::clone(&self.source_rows);
                        if let Some((asset, label, _)) = sources.first() {
                            let detail = format!("{} decoded video frames", self.source_length());
                            let response = cards::original(ui, label, &detail, self.selected_source.as_ref() == Some(asset));
                            if response.clicked() { self.select_source(asset.clone()); }
                        }
                        ui.label(egui::RichText::new("Your starting point stays intact.").size(12.0).color(style::MUTED));
                        ui.add_space(8.0);
                        ui.separator();
                        ui.label("Reuse from original");
                        if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("Browse  :source")).clicked()
                            && let Some(asset) = self.workspace.as_ref().and_then(|workspace| original_asset(workspace)).cloned() { self.select_source(asset); }
                        if ui.add_enabled(!self.service.is_busy(), egui::Button::new("Reuse full Original  ⌘↩")).on_hover_text("Append the entire Original after the selected root beat. Range reuse is not available yet.").clicked() { self.insert(); }
                    } else {
                        let search = ui.add(egui::TextEdit::singleline(&mut self.source_search).id(egui::Id::new(SEARCH_ID)).hint_text("Find source  /").desired_width(f32::INFINITY));
                        if search.has_focus() { self.pane = Pane::Sources; }
                        retain_text_escape(ui, SEARCH_ID);
                        if search.changed() { self.filter_sources(); }
                        let sources = Arc::clone(&self.source_rows);
                        let mut scroll = egui::ScrollArea::vertical().id_salt("legacy-source-list").max_height(190.0);
                        if std::mem::take(&mut self.reveal_source) && let Some(index) = sources.iter().position(|source| Some(&source.0) == self.selected_source.as_ref()) { scroll = scroll.vertical_scroll_offset(index as f32 * (34.0 + ui.spacing().item_spacing.y)); }
                        scroll.show_rows(ui, 34.0, sources.len(), |ui, range| {
                            for index in range {
                                let (asset, label, video) = &sources[index];
                                if ui.selectable_label(self.selected_source.as_ref() == Some(asset), format!("{}  {label}", if *video { "Video" } else { "Audio" })).clicked() { self.select_source(asset.clone()); }
                            }
                        });
                        if self.workspace.is_none() {
                            ui.weak("One video. Start intact, then make it strange.");
                            ui.add_space(8.0);
                            ui.small("Projects live in Documents/Deadpan.");
                        } else {
                            ui.weak("Legacy project. Existing sources remain available.");
                        }
                    }
                    if matches!(profile, Some(SingleSourceState::Ready { .. })) {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.label(egui::RichText::new("SOUND EFFECTS").size(12.0).strong());
                        let search = ui.add(egui::TextEdit::singleline(&mut self.source_search).id(egui::Id::new(SEARCH_ID)).hint_text("Find sound  /").desired_width(f32::INFINITY));
                        if search.has_focus() { self.pane = Pane::Sources; }
                        retain_text_escape(ui, SEARCH_ID);
                        if search.changed() { self.filter_sources(); }
                        let sounds = Arc::clone(&self.sound_rows);
                        if sounds.is_empty() { ui.weak(if self.source_search.is_empty() { "No sounds added." } else { "No matching sounds." }); }
                        egui::ScrollArea::vertical().id_salt("sound-catalog").max_height(120.0).show_rows(ui, 32.0, sounds.len(), |ui, range| {
                            for index in range {
                                let (_, label, _) = &sounds[index];
                                egui::Frame::new().fill(style::PANEL).corner_radius(4).inner_margin(6).show(ui, |ui| {
                                    ui.add(egui::Label::new(label).truncate()).on_hover_text("Retained audio-only source. Sound placement and audition are not available yet.");
                                });
                            }
                        });
                        ui.label(egui::RichText::new("Retained sounds. Placement is not available yet.").size(11.0).color(style::MUTED));
                    }
                    let available = self.workspace.is_some() && !matches!(profile, Some(SingleSourceState::AwaitingSource { .. })) && !self.service.is_busy() && !self.dialogs.is_open() && !self.importing();
                    if self.workspace.is_some() && !matches!(profile, Some(SingleSourceState::AwaitingSource { .. })) {
                        if ui.add_enabled(available, egui::Button::new(if focused { "Add sound…  ⌘I" } else { "Import media…  ⌘I" })).clicked() {
                            self.begin_dialog(self.import_dialog_kind(), ui.ctx(), false);
                        }
                        ui.collapsing(if focused { "Sound import options" } else { "Import options" }, |ui| {
                            if !focused { ui.checkbox(&mut self.audio_import, "Import audio only"); }
                            if focused || self.audio_import {
                                ui.label("Audio stream");
                                egui::ComboBox::from_id_salt("sound-stream").selected_text(self.sound_stream.map_or_else(|| "Automatic".into(), |stream| format!("Index {stream}"))).show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.sound_stream, None, "Automatic: first audio");
                                    for stream in 0..=32 { ui.selectable_value(&mut self.sound_stream, Some(stream), format!("Index {stream}")); }
                                });
                                ui.small("Automatic selects the first actual audio stream. Advanced indices refer to the container, not the audio-track order.");
                                ui.small("Admitted: PCM16 WAV or qualified MP4 audio.");
                            }
                            ui.checkbox(&mut self.linked_import, "Link to original location");
                        });
                    }
                    if let Some(import) = &self.import {
                        let completed_sound = matches!(profile, Some(SingleSourceState::Ready { .. }))
                            && import.asset.as_ref().and_then(|asset| self.workspace.as_ref()?.sources.get(asset)).is_some_and(|source| {
                                source.receipt.snapshot().video().is_none() && source.receipt.snapshot().audio().is_some()
                            });
                        ui.separator();
                        ui.label(egui::RichText::new(match import.stage {
                            ImportStage::Retaining => "Retaining original…", ImportStage::Decoding => "Qualifying media…", ImportStage::PreparingInsertion => "Preparing reuse…", ImportStage::Registering => "Saving source…", ImportStage::Complete if completed_sound => "Sound added", ImportStage::Complete => "Source ready", ImportStage::Cancelled => "Import cancelled", ImportStage::Failed => "Import failed",
                        }).size(12.0));
                        if let Some(error) = &import.error { ui.colored_label(ui.visuals().error_fg_color, error); }
                        if self.importing() && ui.button("Cancel import").clicked() { self.submit(ProjectRequest::CancelImport); }
                    }
                });
            });
    }

    fn timeline(&mut self, ui: &mut egui::Ui) {
        let beats = Arc::clone(&self.beat_rows);
        let layout = self.workspace_layout(ui);
        style::beat_panel(layout).show(ui, |ui| {
            let heading = cards::heading(
                ui,
                if self.focused_workflow() {
                    "YOUR EDIT"
                } else {
                    "BEATS"
                },
                self.pane == Pane::Sequence,
                beats.len(),
                self.sequence_length(),
                self.workspace
                    .as_ref()
                    .map(|workspace| workspace.document.presentation_basis().frame_rate),
            );
            if pane_focus(
                ui,
                Pane::Sequence,
                heading.rect,
                "Root sequence beat outline pane",
            )
            .has_focus()
            {
                self.pane = Pane::Sequence;
            }
            if beats.is_empty() {
                ui.add_space(16.0);
                ui.weak(
                    match self
                        .workspace
                        .as_ref()
                        .and_then(|workspace| workspace.single_source.as_ref())
                    {
                        Some(SingleSourceState::AwaitingSource { .. }) => {
                            "Your full video will appear here when its Original is ready."
                        }
                        Some(SingleSourceState::Ready { .. }) => {
                            "Your edit is empty. Undo the cut or reuse the full Original."
                        }
                        None if self.workspace.is_none() => {
                            "Choose one video to begin. Its full length becomes your starting edit."
                        }
                        None => "Choose a source, then insert it into this legacy sequence.",
                    },
                );
                return;
            }
            let selected = self.selected_beat.clone();
            let marker = if self.view == View::Sequence {
                selection::cursor_marker(&beats, self.sequence_cursor)
            } else {
                None
            };
            if let Some(index) = cards::strip(
                ui,
                layout,
                &beats,
                selected.as_ref(),
                marker,
                self.sequence_cursor,
                std::mem::take(&mut self.reveal_beat),
            ) {
                let beat = &beats[index];
                self.bindings.clear();
                self.selected_beat = Some(beat.id.clone());
                self.sequence_cursor = beat.start;
                self.view.set(View::Sequence, &mut self.message);
                self.pane = Pane::Sequence;
                self.request_picture(true);
                ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sequence)));
            }
        });
    }

    fn workspace_layout(&self, ui: &egui::Ui) -> style::Layout {
        let size = ui.ctx().input(|input| input.content_rect().size());
        style::Layout::for_size(size.x, size.y)
    }

    fn inspector_visible(&self) -> bool {
        self.view == View::Sequence
            && self.workspace.as_ref().is_some_and(|workspace| {
                self.selected_beat
                    .as_ref()
                    .is_some_and(|node| root_children(workspace).contains(node))
            })
    }

    fn ensure_visible_pane(&mut self, context: &egui::Context) {
        let pane = self.pane.visible(self.inspector_visible());
        if pane != self.pane {
            self.pane = pane;
            self.bindings.clear();
            context.memory_mut(|memory| memory.request_focus(pane_id(pane)));
        }
    }

    fn inspector_description(&self) -> Option<inspector::Inspector> {
        if !self.inspector_visible() {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        let row = self
            .beat_rows
            .iter()
            .find(|row| Some(&row.id) == self.selected_beat.as_ref())?;
        let node = workspace.document.nodes().get(&row.id)?;
        Some(inspector::Inspector::describe(node, row.start, row.frames))
    }

    fn open_inspector_parameter(&mut self, context: &egui::Context) -> bool {
        let Some((_, command)) = self.inspector_description().and_then(|data| data.parameter)
        else {
            return false;
        };
        self.open_command(command, context);
        true
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.inspector_description() else {
            return;
        };
        let Some(workspace) = &self.workspace else {
            return;
        };
        let frame_rate = frame_rate_label(workspace.document.presentation_basis().frame_rate);
        let layout = self.workspace_layout(ui);
        egui::Panel::right("workspace-inspector")
            .resizable(false)
            .default_size(layout.inspector)
            .min_size(layout.inspector)
            .max_size(layout.inspector)
            .frame(style::panel())
            .show(ui, |ui| {
                let heading = pane_heading(ui, "INSPECTOR", self.pane == Pane::Inspector);
                if pane_focus(
                    ui,
                    Pane::Inspector,
                    heading.rect,
                    "Selected root beat inspector pane",
                )
                .has_focus()
                {
                    self.pane = Pane::Inspector;
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("inspector-details")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            egui::Frame::new()
                                .fill(style::SELECTED)
                                .corner_radius(6)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(data.glyph)
                                            .size(24.0)
                                            .color(style::LAVENDER),
                                    );
                                });
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&data.label).size(17.0))
                                        .truncate(),
                                )
                                .on_hover_text(&data.label);
                                ui.label(egui::RichText::new(data.kind).color(style::MUTED));
                            });
                        });
                        ui.add_space(12.0);
                        inspector_value(ui, "Duration", &data.duration);
                        for (label, value) in &data.fields {
                            inspector_value(ui, label, value);
                        }
                        ui.add_space(8.0);
                        if data.parameter.is_some() {
                            style::key_hint(ui, "Enter", "change parameter");
                        }
                        ui.separator();
                        inspector_value(ui, "Scope", "Root beat");
                        inspector_value(ui, "Boundaries", &data.range);
                        ui.label(
                            egui::RichText::new(format!("{} at {frame_rate}", data.duration))
                                .size(12.0)
                                .color(style::MUTED),
                        );
                        ui.label(
                            egui::RichText::new(data.note)
                                .size(12.0)
                                .color(style::MUTED),
                        );
                        ui.add_space(8.0);
                        let ready = !self.service.is_busy() && !self.dialogs.is_open();
                        ui.add_enabled_ui(ready, |ui| {
                            if let Some((label, command)) = data.parameter
                                && ui
                                    .add_sized(
                                        [ui.available_width(), 30.0],
                                        egui::Button::new(label).fill(style::SELECTED),
                                    )
                                    .clicked()
                            {
                                self.pane = Pane::Inspector;
                                self.open_command(command, ui.ctx());
                            }
                            if ui
                                .add_sized(
                                    [ui.available_width(), 28.0],
                                    egui::Button::new("Wrap repeat  ·  rr"),
                                )
                                .on_hover_text("Two total plays. An existing Repeat is nested.")
                                .clicked()
                            {
                                self.edit(BeatEdit::WrapRepeat(2));
                            }
                            if ui
                                .add_sized(
                                    [ui.available_width(), 28.0],
                                    egui::Button::new("Delete beat  ·  dd"),
                                )
                                .on_hover_text("Ripple-delete this root beat. Undo is available.")
                                .clicked()
                            {
                                self.edit(BeatEdit::Delete);
                            }
                        });
                    });
            });
    }

    fn viewer(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(style::panel()).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (label, view) in [(if self.focused_workflow() { "Original" } else { "Source" }, View::Source), (if self.focused_workflow() { "Your edit" } else { "Sequence" }, View::Sequence)] {
                    if ui.add_enabled(view == View::Source || self.workspace.is_some(), egui::Button::new(label).min_size(egui::vec2(88.0, 28.0)).selected(self.view == view)).clicked() {
                        self.bindings.clear();
                        if self.view != view { self.view.set(view, &mut self.message); if view == View::Sequence { self.reconcile_beat_selection(); } self.request_picture(true); }
                        self.pane = Pane::Viewer;
                        ui.memory_mut(|m| m.request_focus(pane_id(Pane::Viewer)));
                    }
                }
                if self.view == View::Sequence { ui.add(egui::Label::new(egui::RichText::new(if self.focused_workflow() { "Same original. Your changes." } else { "Current sequence" }).color(style::MUTED).size(12.0)).truncate()); }
                else if let Some(source) = self.workspace.as_ref().and_then(|w| self.selected_source.as_ref().and_then(|id| w.sources.get(id))) { ui.add(egui::Label::new(egui::RichText::new(&source.label).color(style::MUTED).size(12.0)).truncate()).on_hover_text(&source.label); }
            });
            let available = egui::vec2(ui.available_width().max(1.0), (ui.available_height() - 98.0).max(50.0));
            let (_, rect) = ui.allocate_space(available);
            let response = pane_focus(ui, Pane::Viewer, rect, "Picture viewer pane");
            if response.has_focus() { self.pane = Pane::Viewer; }
            ui.painter().rect_filled(rect, 2.0, egui::Color32::BLACK);
            let aspect = self.presentation.picture().and_then(|p| p.canvas).map(|(w, h)| w as f32 / h as f32);
            let canvas = aspect.map_or(rect, |aspect| fit_rect(rect, aspect));
            self.render_picture(ui.ctx(), canvas.size());
            let displayed_label = self.presentation.displayed_label();
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, displayed_label.as_deref().unwrap_or("No picture displayed")));
            if self.presentation.has_displayed() && !(self.view == View::Sequence && self.sequence_length() == 0) {
                if let Some(target) = &self.target { ui.painter().image(target.texture, canvas, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE); }
                else { ui.painter().rect_filled(canvas, 0.0, egui::Color32::BLACK); }
            } else {
                let message = if self.presentation.loading() { "Preparing picture…" } else if self.presentation.error().is_some() { "Picture unavailable" } else if self.workspace.is_none() && self.raw_source.is_none() { "Start with one video. Make it strange." } else if self.view == View::Sequence { "Your edit is empty" } else if self.selected_source.is_some() && self.source_length() == 0 { "Audio source · no picture" } else { "Choose the Original to begin" };
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, message, egui::FontId::proportional(18.0), style::MUTED);
            }
            ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, if self.pane == Pane::Viewer { style::LAVENDER } else { style::BORDER }), egui::StrokeKind::Inside);
            if let Some(label) = displayed_label {
                let mut response = ui.add(egui::Label::new(egui::RichText::new(&label).size(11.5).color(style::MUTED)).truncate()).on_hover_text(&label);
                if let Some(summary) = &self.summary { response = response.on_hover_text(format!("Measured source: {} × {} pixels; original PTS [{}, {}), clock {}/{} seconds per tick.", summary.info.width, summary.info.height, summary.first_pts, summary.terminal_pts, summary.info.time_base_num, summary.info.time_base_den)); }
                if let Some(source_frame) = self.presentation.displayed_source_frame() { response.on_hover_text(format!("Original source frame {}", u128::from(source_frame.0) + 1)); }
            } else { ui.label(egui::RichText::new("Stopped-frame inspection").size(11.5).color(style::MUTED)); }
            if let Some(workspace) = &self.workspace && let Some(original) = workspace.original_duration {
                ui.horizontal_wrapped(|ui| {
                    let edit = workspace.plan.duration();
                    ui.label(egui::RichText::new(format!("Original {} f", original.frames())).monospace()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("Full Original duration: {} project frames", original.frames())));
                    ui.separator();
                    ui.label(egui::RichText::new(format!("Your edit {} f", edit.frames())).monospace()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("Your edit duration: {} project frames", edit.frames())));
                    ui.colored_label(style::LAVENDER, format!("{:+} f", edit.frames() - original.frames()));
                    ui.label(egui::RichText::new(format!("project clock · {}", frame_rate_label(workspace.document.presentation_basis().frame_rate))).size(10.0).color(style::MUTED));
                }).response.on_hover_text("Both durations use the same project-frame clock. Original includes the measured picture/audio stream union; source browsing counts decoded picture frames separately.");
            }
            ui.horizontal_wrapped(|ui| {
                if self.workspace.is_none() && self.raw_source.is_none() {
                    if ui.button("New project  ⌘N").clicked() { self.begin_dialog(DialogKind::CreateProject, ui.ctx(), false); }
                    if ui.button("Open project  ⌘O").clicked() { self.begin_dialog(DialogKind::OpenProject, ui.ctx(), false); }
                    ui.weak("Saved in Documents/Deadpan");
                } else {
                    for (label, action) in [("Start  gg", Action::First), ("Previous  h", Action::Step { forward: false, count: 1 }), ("Next  l", Action::Step { forward: true, count: 1 }), ("End  G", Action::Last)] {
                        if ui.small_button(label).clicked() { self.action(action, ui.ctx()); }
                    }
                    if self.view == View::Source && ui.add_enabled(self.workspace.is_some() && self.selected_source.is_some() && !self.service.is_busy(), egui::Button::new(if self.focused_workflow() { "Reuse full Original  ⌘↩" } else { "Insert source  ⌘↩" }).fill(style::SELECTED)).on_hover_text("Insert the whole source after the selected root beat. This creates an undoable edit.").clicked() { self.insert(); }
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
        egui::Window::new("Keys · reshape one Original")
            .open(&mut self.help_open)
            .collapsible(false)
            .resizable(true)
            .default_width(680.0)
            .show(context, |ui| {
                egui::ScrollArea::vertical().max_height((context.input(|input| input.content_rect().height()) - 140.0).max(240.0)).show(ui, |ui| {
                    ui.label("New starts with your full video. Its Original stays intact while Your edit changes.");
                    ui.label(egui::RichText::new("START & MOVE").strong().color(style::LAVENDER));
                    for (key, description) in [
                        ("⌘N / ⌘O", "Choose one Original / open a project. New projects live in Documents/Deadpan."),
                        ("h l · Left Right", "Move one frame in the current clock. Prefix a count: 12l."),
                        ("j k", "V1: select the next / previous root beat and return to Your edit. In a legacy Sources pane, choose a source."),
                        ("gg / G", "First / final boundary."),
                        (":source / :sequence", "Browse unchanged Original / work on Your edit."),
                        ("Tab / Shift Tab", "Cycle Original, Viewer, visible Inspector, and Beats focus."),
                        ("/", "Find a sound in V1, or a source in a legacy project."),
                    ] { help_binding(ui, key, description); }
                    ui.separator();
                    ui.label(egui::RichText::new("RESHAPE THE SELECTED BEAT").strong().color(style::LAVENDER));
                    for (key, description) in [
                        ("rr / 3rr", "Wrap the selected root beat in two / three total plays."),
                        ("dd / :delete", "Cut one whole root beat and close its time. Undo restores it."),
                        (":repeat 3", "Set total plays on a Repeat; wrap a different root beat."),
                        (":wrap-repeat 3", "Always add an enclosing Repeat, including nesting."),
                        ("Enter in Inspector", "Edit the selected Repeat count or existing Hold duration."),
                        (":hold-duration 11f", "Set a selected root Hold to exactly 11 project frames."),
                        ("⌘Return / :insert", "Reuse the full Original after the selected root beat. Legacy projects insert their selected source."),
                        ("u / Ctrl R", "Undo / redo. Native ⌘Z / ⌘Shift Z also work."),
                    ] { help_binding(ui, key, description); }
                    ui.separator();
                    for (key, description) in [
                        ("⌘I", "Add an audio-only sound, or retry an incomplete Original. Audio selection is automatic; advanced stream choices are in import options."),
                        (": / Enter / Esc", "Enter a command / apply / cancel. Text fields keep native editing and IME."),
                        ("? / :help / Esc", "Open this reference / close it."),
                    ] { help_binding(ui, key, description); }
                    ui.separator();
                    ui.weak("Original browsing never changes it. Your edit commands affect the selected root beat and its linked picture and sound. Counts precede operators, such as 3rr; the visible PENDING badge waits without a timer.");
                    ui.weak("Current limits: range cuts/reuse, nested navigation, new Holds, sound placement/audition, playback, effects, AI generation in the app, and export are not available yet. Registered sounds are retained catalog entries only.");
                });
            });
    }
}

impl eframe::App for DeadpanApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.close_pending |= context.input(|i| i.viewport().close_requested());
        let previous_pane = self.pane;
        self.receive();
        self.ensure_visible_pane(&context);
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
        let input_scope = (
            self.pane,
            self.view,
            self.selected_beat.clone(),
            self.selected_source.clone(),
        );
        self.header(ui);
        self.footer(ui);
        self.sources(ui);
        self.timeline(ui);
        self.inspector(ui);
        self.viewer(ui);
        self.help(&context);
        if input_scope
            != (
                self.pane,
                self.view,
                self.selected_beat.clone(),
                self.selected_source.clone(),
            )
            || context.memory(|m| {
                m.has_focus(egui::Id::new(SEARCH_ID)) || m.has_focus(egui::Id::new(COMMAND_ID))
            })
        {
            self.bindings.clear();
        }
        if let Some((action, command)) = text_result {
            if command && action == TextAction::Open {
                self.run_command(&context);
            }
            self.command_open = false;
            context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
        }
        self.ensure_visible_pane(&context);
        if close_command_on_blur(&context, &mut self.command_open) {
            self.bindings.clear();
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
    kind: String,
    start: u64,
    frames: u64,
}

fn beat_kind(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Source { .. } => "Source".into(),
        NodeKind::Hold { .. } => "Hold".into(),
        NodeKind::Repeat { iterations, .. } => format!(
            "Repeat · {} total {}",
            iterations.len(),
            if iterations.len() == 1 {
                "play"
            } else {
                "plays"
            }
        ),
        NodeKind::Retime { .. } => "Retime".into(),
        NodeKind::Sequence { .. } => "Sequence".into(),
    }
}

fn frame_rate_label(rate: deadpan_core::FrameRate) -> String {
    if rate.denominator() == 1 {
        format!("{} fps", rate.numerator())
    } else {
        format!("{}/{} fps", rate.numerator(), rate.denominator())
    }
}

fn pane_name(pane: Pane) -> &'static str {
    match pane {
        Pane::Sources => "Sources",
        Pane::Viewer => "Viewer",
        Pane::Sequence => "Beats",
        Pane::Inspector => "Inspector",
    }
}

fn pane_heading(ui: &mut egui::Ui, title: &str, focused: bool) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(13.0).strong());
        if focused {
            ui.label(
                egui::RichText::new("FOCUS")
                    .size(9.0)
                    .color(style::LAVENDER),
            );
        }
    })
    .response
}

fn paint_cursor(ui: &egui::Ui, rect: egui::Rect, fraction: f32, cursor: u64) {
    let x = egui::lerp(rect.left()..=rect.right(), fraction);
    ui.painter().line_segment(
        [
            egui::pos2(x, rect.top() - 3.0),
            egui::pos2(x, rect.bottom()),
        ],
        egui::Stroke::new(1.0, style::CURSOR),
    );
    let text = ui.painter().layout_no_wrap(
        cursor.to_string(),
        egui::FontId::monospace(10.0),
        style::CANVAS,
    );
    let size = text.size() + egui::vec2(8.0, 4.0);
    let center_x = x.clamp(rect.left() + size.x / 2.0, rect.right() - size.x / 2.0);
    let badge = egui::Rect::from_center_size(egui::pos2(center_x, rect.top() - 10.0), size);
    ui.painter().rect_filled(badge, 3.0, style::CURSOR);
    ui.painter()
        .galley(badge.min + egui::vec2(4.0, 2.0), text, style::CANVAS);
}

fn inspector_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(72.0, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new(label).size(12.0));
            },
        );
        egui::Frame::new()
            .fill(style::PANEL)
            .stroke(egui::Stroke::new(1.0, style::BORDER))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(7, 5))
            .show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 14.0).max(24.0));
                ui.add(
                    egui::Label::new(egui::RichText::new(value).monospace().size(11.5)).truncate(),
                )
                .on_hover_text(format!("{label}: {value}"));
            });
    });
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

fn original_asset(workspace: &Workspace) -> Option<&AssetId> {
    match &workspace.single_source {
        Some(SingleSourceState::Ready { asset, .. }) => Some(asset),
        _ => None,
    }
}

/// A sound arriving in the catalog cannot move the Original cursor or selection.
fn registration_selection(
    profile: Option<&SingleSourceState>,
    imported: AssetId,
) -> Option<AssetId> {
    match profile {
        Some(SingleSourceState::Ready { asset, .. }) if asset == &imported => Some(imported),
        Some(_) => None,
        None => Some(imported),
    }
}

fn help_binding(ui: &mut egui::Ui, key: &str, description: &str) {
    ui.horizontal_wrapped(|ui| {
        style::keycap(ui, key);
        ui.label(description);
    });
}
fn pane_id(pane: Pane) -> egui::Id {
    egui::Id::new(match pane {
        Pane::Sources => "sources-pane",
        Pane::Viewer => "viewer-pane",
        Pane::Sequence => "sequence-pane",
        Pane::Inspector => "inspector-pane",
    })
}
fn close_command_on_blur(context: &egui::Context, open: &mut bool) -> bool {
    if *open && !context.memory(|m| m.has_focus(egui::Id::new(COMMAND_ID))) {
        *open = false;
        context.request_repaint();
        true
    } else {
        false
    }
}
fn text_input_active(context: &egui::Context, command_open: bool) -> bool {
    command_open
        || context.memory(|m| {
            m.has_focus(egui::Id::new(SEARCH_ID)) || m.has_focus(egui::Id::new(COMMAND_ID))
        })
}
fn pointer_focus_transition(events: &[egui::Event]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, egui::Event::PointerButton { .. }))
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

    #[test]
    fn v1_open_and_sound_completion_preserve_original_identity_and_cursor() {
        let original = AssetId::new("original").unwrap();
        let sound = AssetId::new("sound").unwrap();
        let ready = SingleSourceState::Ready {
            initial_revision: RevisionId::new("initial").unwrap(),
            asset: original.clone(),
            qualification: deadpan_core::SourceQualificationId::new("a".repeat(64)).unwrap(),
            node: NodeId::new("original-beat").unwrap(),
            baseline_revision: RevisionId::new("baseline").unwrap(),
        };
        let awaiting = SingleSourceState::AwaitingSource {
            initial_revision: RevisionId::new("initial").unwrap(),
        };
        assert_eq!(View::on_open(Some(&ready)), View::Sequence);
        assert_eq!(View::on_open(Some(&awaiting)), View::Source);
        assert_eq!(View::on_open(None), View::Source);
        assert_eq!(registration_selection(Some(&ready), sound.clone()), None);
        assert_eq!(
            registration_selection(Some(&ready), original.clone()),
            Some(original)
        );
        assert_eq!(registration_selection(Some(&awaiting), sound.clone()), None);
        assert_eq!(registration_selection(None, sound.clone()), Some(sound));
    }

    #[test]
    fn pointer_focus_batches_defer_shortcuts_and_old_command_submission() {
        for pressed in [true, false] {
            let pointer = egui::Event::PointerButton {
                pos: egui::pos2(30.0, 50.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            for events in [
                vec![
                    pointer.clone(),
                    key_event(egui::Key::D),
                    key_event(egui::Key::D),
                ],
                vec![key_event(egui::Key::Enter), pointer],
            ] {
                assert!(pointer_focus_transition(&events));
            }
        }
        assert!(!pointer_focus_transition(&[
            key_event(egui::Key::D),
            key_event(egui::Key::D)
        ]));
        assert!(!pointer_focus_transition(&[egui::Event::PointerMoved(
            egui::pos2(30.0, 50.0)
        )]));
    }

    #[test]
    fn clicking_away_closes_command_mode_before_normal_edit_keys_resume() {
        let context = egui::Context::default();
        let mut command = "hold-duration 45f".to_owned();
        let mut open = true;
        let mut target = egui::Pos2::ZERO;
        let mut draw = |ui: &mut egui::Ui| {
            ui.add(egui::TextEdit::singleline(&mut command).id(egui::Id::new(COMMAND_ID)));
            retain_text_escape(ui, COMMAND_ID);
            let button = ui.button("Another root beat");
            target = button.rect.center();
            if button.clicked() {
                ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sequence)));
            }
            let heading = ui.label("Sequence");
            pane_focus(ui, Pane::Sequence, heading.rect, "Sequence pane");
        };
        run_ui(&context, egui::RawInput::default(), |ui| {
            ui.memory_mut(|m| m.request_focus(egui::Id::new(COMMAND_ID)));
            draw(ui);
        });
        run_ui(&context, egui::RawInput::default(), &mut draw);
        let click = target;
        run_ui(
            &context,
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                ui.add(egui::TextEdit::singleline(&mut command).id(egui::Id::new(COMMAND_ID)));
                retain_text_escape(ui, COMMAND_ID);
                let button = ui.button("Another root beat");
                assert!(button.clicked());
                ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sequence)));
                let heading = ui.label("Sequence");
                pane_focus(ui, Pane::Sequence, heading.rect, "Sequence pane");
                let focused = ui.memory(|m| m.has_focus(egui::Id::new(COMMAND_ID)));
                assert!(!focused);
                let mut bindings = Bindings::default();
                for _ in 0..2 {
                    assert!(
                        bindings
                            .key(
                                egui::Key::D,
                                egui::Modifiers::NONE,
                                text_input_active(ui.ctx(), open),
                                false
                            )
                            .is_none()
                    );
                }
                assert!(close_command_on_blur(ui.ctx(), &mut open));
            },
        );
        assert!(!open, "the old command is no longer displayed as active");
        assert_eq!(
            command, "hold-duration 45f",
            "blur never submits or rewrites it"
        );
        assert!(!context.memory(|m| m.has_focus(egui::Id::new(COMMAND_ID))));
    }

    #[test]
    fn import_completion_after_history_preserves_the_current_view() {
        let revision = deadpan_core::RevisionId::new("edited").unwrap();
        let edit = selection::completion(Some(&revision), None, false);
        let mut view = View::Source.after_completion(&edit);
        assert_eq!(view, View::Sequence);
        // An Undo/Redo command clears the service marker. Registration still
        // finishes later, after the user has resumed Sequence work.
        let registration = selection::completion(None, Some(&revision), true);
        assert_eq!(registration, selection::Completion::Registration);
        view = view.after_completion(&registration);
        assert_eq!(view, View::Sequence);
        assert_eq!(View::Source.after_completion(&registration), View::Source);
        // Coalesced commit+registration and a separately delivered registration
        // agree about context even after a marker-clearing command.
        let coalesced = selection::completion(Some(&revision), None, true);
        assert_eq!(View::Source.after_completion(&coalesced), view);
    }

    #[test]
    fn repeat_description_exposes_current_total_plays_without_expanding_them() {
        for (plays, expected) in [
            (1, "Repeat · 1 total play"),
            (3, "Repeat · 3 total plays"),
            (u32::MAX, "Repeat · 4294967295 total plays"),
        ] {
            let kind = NodeKind::Repeat {
                child: NodeId::new("child").unwrap(),
                iterations: deadpan_core::IterationOrder::new(
                    deadpan_core::RevisionId::new("allocation").unwrap(),
                    plays,
                )
                .unwrap(),
                gap: None,
            };
            assert_eq!(beat_kind(&kind), expected);
        }
        assert_eq!(
            beat_kind(&NodeKind::Sequence {
                children: Vec::new()
            }),
            "Sequence"
        );
    }

    #[test]
    fn source_hint_clears_on_context_change_without_clearing_project_feedback() {
        let mut view = View::Source;
        let mut message = Some(SOURCE_INSERT_HINT.to_owned());
        view.set(View::Source, &mut message);
        assert_eq!(message.as_deref(), Some(SOURCE_INSERT_HINT));
        view.set(View::Sequence, &mut message);
        assert!(message.is_none());
        message = Some("Source inserted and saved".into());
        view.set(View::Source, &mut message);
        view.set(View::Sequence, &mut message);
        assert_eq!(message.as_deref(), Some("Source inserted and saved"));
    }

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
    fn parameter_entry_opened_after_footer_draw_receives_next_frame_text() {
        let context = egui::Context::default();
        let mut command = "repeat ".to_owned();
        run_ui(&context, egui::RawInput::default(), |ui| {
            // The toolbar opens the command after this frame's footer was drawn.
            let heading = ui.label("Sequence");
            pane_focus(ui, Pane::Sequence, heading.rect, "Sequence pane");
            ui.memory_mut(|m| m.request_focus(egui::Id::new(COMMAND_ID)));
        });
        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Text("4".into())],
                ..Default::default()
            },
            |ui| {
                assert!(ui.memory(|m| m.has_focus(egui::Id::new(COMMAND_ID))));
                ui.add(egui::TextEdit::singleline(&mut command).id(egui::Id::new(COMMAND_ID)));
                retain_text_escape(ui, COMMAND_ID);
            },
        );
        assert_eq!(command, "repeat 4");
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
