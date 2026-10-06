use std::cell::Cell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_core::{AssetId, NodeId, NodeKind, ProjectFrame, RevisionId, SourceFrameId};
use deadpan_render::{FitMode, PictureRenderer, RenderTarget};
use deadpan_store::original_media::OriginalOwnership;
use deadpan_store::single_source::SingleSourceState;
use eframe::{egui, egui_wgpu};

use crate::dialogs::{DialogKind, Dialogs};
use crate::navigation::{
    self, Action, BeatEdit, BindingId as EditorKey, Bindings, Pane, TextAction, registry,
};
use crate::presentation::Presentation;
use crate::project::{
    ImportMedia, ImportStage, ImportStatus, ProjectEdit, ProjectRequest, ProjectService,
    SequenceScope, Workspace,
};
use crate::worker::{PreviewWorker, ProjectView, SourceSummary, Ticket, Work};

mod accessibility;
mod ai_pause;
mod backups;
mod camera;
#[cfg(test)]
pub(crate) use camera::dispatch_debug as camera_dispatch_debug;
mod camera_fields;
mod captions;
mod cards;
mod copied;
mod corrections;
mod cutaways;
mod delete;
mod diagnostics;
mod edit_range;
mod editor_input;
#[cfg(target_os = "macos")]
mod fonts;
mod gain;
mod groups;
#[cfg(feature = "ui-harness")]
pub(crate) mod harness;
mod help_scroll;
mod hold_effects;
mod inspector;
mod jobs;
mod key_labels;
mod macros;
mod marks;
mod model_packs;
mod moment;
mod operators;
mod playback;
mod proxies;
mod recovery;
mod render;
mod repeat_queue;
mod repeats;
mod room_tone;
mod scope;
mod scoped;
mod selection;
mod semantic;
mod shots;
mod slip;
mod sound_events;
mod splice;
mod storage;
mod style;
mod targets;
mod thumbnails;
mod transcript;
mod trim;
mod youtube;

const SEARCH_ID: &str = "source-search";
const COMMAND_ID: &str = "command-input";
const TRANSCRIPT_SEARCH_ID: &str = "transcript-search";
/// Focused text fields that own keyboard input instead of editor bindings.
const TEXT_INPUT_IDS: [&str; 4] = [SEARCH_ID, COMMAND_ID, TRANSCRIPT_SEARCH_ID, youtube::URL_ID];
const MAX_TARGET_PIXELS: f64 = 1920.0 * 1080.0;
const SOURCE_INSERT_HINT: &str = "The Original stays intact. Switch to Your edit (:sequence) to reshape it, or reuse the full Original with :insert.";

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
    #[cfg(feature = "ui-harness")]
    feedback: harness::Feedback,
    smoke_frames: Option<u8>,
    /// Reduce motion / Increase contrast; fixed outside the native app.
    display: accessibility::Display,
    close_pending: bool,
    exited: Rc<Cell<bool>>,
    worker: PreviewWorker,
    endpoint_worker: crate::worker::EndpointWorker,
    playback: deadpan_playback::Engine,
    #[cfg(target_os = "macos")]
    _lifecycle: deadpan_output::LifecycleObserver,
    playback_interrupted: Arc<AtomicBool>,
    transport: Option<crate::transport::Run>,
    monitor_gain: f32,
    audition_context: playback::AuditionContext,
    monitor_control: Option<egui::Id>,
    resume: Option<crate::transport::Resume>,
    service: ProjectService,
    repeat_queue: repeat_queue::Queue,
    dialogs: Dialogs,
    dialog_intent: Option<DialogIntent>,
    render_state: egui_wgpu::RenderState,
    renderer: PictureRenderer,
    // Logical drafts borrow this owner; closing one never drops in-flight work.
    junction_pictures: splice::JunctionDisplay,
    thumbnails: thumbnails::Thumbnails,
    transcription: transcript::Transcription,
    shots: shots::ShotJob,
    proxies: proxies::ProxyJob,
    /// New-from-URL: one YouTube import job and its URL step.
    youtube: youtube::Flow,
    /// The native menu bar, present only for native launches.
    #[cfg(target_os = "macos")]
    menu: Option<crate::menu::MenuBar>,
    target: Option<RegisteredTarget>,
    workspace: Option<Arc<Workspace>>,
    import: Option<ImportStatus>,
    render_job: Option<crate::project::ProjectRenderUpdate>,
    render: render::State,
    selected_source: Option<AssetId>,
    selected_sound: Option<AssetId>,
    selected_event: Option<deadpan_core::SoundId>,
    sound_inspection: Option<sound_events::Inspection>,
    reveal_event: bool,
    sound_command_target: Option<sound_events::CommandTarget>,
    hold_command_target: Option<room_tone::CommandTarget>,
    room_tone: Option<room_tone::Draft>,
    /// `:correct`: the transcript and pause correction sheet.
    correction: Option<corrections::Draft>,
    gain_command_target: Option<Result<crate::project::gain::Target, String>>,
    gain: Option<gain::Draft>,
    slip_command_target: Option<Result<slip::Capture, String>>,
    slip: Option<slip::Draft>,
    slip_abandon: Option<crate::project::slip::ProposalId>,
    slip_picture_pending: bool,
    trim: Option<trim::Draft>,
    trim_command_target: Option<Result<trim::Capture, String>>,
    trim_prefix_target: Option<Result<trim::Capture, String>>,
    trim_abandon: VecDeque<crate::project::trim::ProposalId>,
    trim_picture_pending: bool,
    ai: ai_pause::State,
    targets: targets::State,
    splice: Option<splice::Draft>,
    splice_abandon: Option<crate::project::splice::ProposalId>,
    sound_cursor: u64,
    selected_beat: Option<NodeId>,
    scoped: Option<scoped::model::State>,
    scoped_command_target: Option<Result<Option<crate::project::scoped::Target>, String>>,
    sequence_scope: SequenceScope,
    scope_start: u64,
    scope_end: u64,
    scope_labels: Vec<String>,
    view: View,
    pane: Pane,
    source_cursor: u64,
    moment: moment::Selection,
    copied: copied::Register,
    edit_range: edit_range::Selection,
    placement_command_target: Option<Result<moment::PlacementTarget, String>>,
    copy_command_register: Option<(u64, char)>,
    register_command_cancelled: bool,
    delete_command_target: Option<Result<delete::CommandTarget, String>>,
    frame_delete_command_target: Option<Result<delete::FrameTarget, String>>,
    macros: macros::State,
    macro_prefix_target: Option<Result<macros::Capture, String>>,
    operator_target: Option<operators::Capture>,
    repeat_prefix_target: Option<repeats::Capture>,
    macro_command_target: Option<Result<macros::Capture, String>>,
    marks: marks::State,
    sequence_cursor: u64,
    source_search: String,
    command: String,
    command_open: bool,
    command_focus_pending: bool,
    bindings: Bindings,
    keymap_status: String,
    keymap_error: bool,
    text_entry_gate: editor_input::TextEntryGate,
    deferred_text_input: Vec<egui::Event>,
    camera: Option<camera::CameraSession>,
    camera_pending: Option<camera::CameraPending>,
    /// A fallback explanation appended to the next framing save message.
    /// An explanation shown with the commit of one submitted framing.
    zoom_note: Option<camera::ZoomNote>,
    /// `:zoom`/`:creep` context captured when command entry opened.
    zoom_command_target: Option<camera::ZoomContext>,
    ime_composing: bool,
    help_open: bool,
    help_scroll: help_scroll::HelpScroll,
    /// `:registers` opens Help with the register inventory first; `?` opens
    /// it with the basics first.
    help_registers_first: bool,
    /// `:gag-inspect`: a recipe's exact expansion, listed first in Help
    /// until Help next opens without it. Nothing is applied.
    help_expansion: Option<(String, Vec<String>)>,
    /// `:select role=`: the role a Visual `d` removes in Your edit. Linked
    /// removes time; audio or video delete one role without moving content.
    edit_role: deadpan_core::MediaRole,
    /// The project session and group a non-linked role was chosen in; the
    /// role returns to linked when either changes or the Original is shown.
    edit_role_context: Option<(u64, SequenceScope)>,
    linked_import: bool,
    audio_import: bool,
    sound_stream: Option<u32>,
    /// Explicit speaker reading for sounds whose channels declare no layout.
    /// None refuses such a sound with guidance; it is never guessed.
    sound_interpretation: Option<crate::project::AudioLayoutInterpretation>,
    /// The import error last scrolled into view in the rail.
    revealed_import_error: Option<String>,
    last_committed: Option<deadpan_core::RevisionId>,
    semantic: semantic::Mirror,
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
    recovery: recovery::RecoveryUi,
    models: model_packs::Models,
    /// `:diagnostics`: live process counters.
    diagnostics: diagnostics::State,
    /// `:storage`: project and cache usage, cleanup and portable copies.
    storage: storage::State,
    /// `:jobs`: background jobs, queued state, cancel and interrupted AI.
    jobs: jobs::State,
    /// The user's gag presets, shared by every project.
    gag_presets: crate::gag_presets::GagPresets,
    /// Where bundled synthesized sounds are written before import.
    bundled_sounds: Option<PathBuf>,
}

impl DeadpanApp {
    pub fn new(
        context: &eframe::CreationContext<'_>,
        render_state: egui_wgpu::RenderState,
        smoke_test: bool,
        exited: Rc<Cell<bool>>,
        initial_path: Option<String>,
        initial_project: Option<String>,
        keymap: crate::keymap::Startup,
    ) -> Result<Self, std::io::Error> {
        #[cfg(target_os = "macos")]
        fonts::install(&context.egui_ctx);
        style::apply(&context.egui_ctx);
        let repaint = context.egui_ctx.clone();
        let service = ProjectService::new(Arc::new(move || repaint.request_repaint()))?;
        let worker = PreviewWorker::new(context.egui_ctx.clone())?;
        let endpoint_worker = crate::worker::EndpointWorker::new(context.egui_ctx.clone())?;
        let repaint = context.egui_ctx.clone();
        let playback = deadpan_playback::Engine::new(Arc::new(move || repaint.request_repaint()))?;
        let playback_interrupted = Arc::new(AtomicBool::new(false));
        #[cfg(target_os = "macos")]
        let lifecycle = {
            let stop = playback.stop_handle();
            let interrupted = Arc::clone(&playback_interrupted);
            let repaint = context.egui_ctx.clone();
            deadpan_output::LifecycleObserver::new(Arc::new(move || {
                stop.stop();
                interrupted.store(true, Ordering::Release);
                repaint.request_repaint();
            }))
            .map_err(std::io::Error::other)?
        };
        let renderer = PictureRenderer::new(&render_state.device, &render_state.queue);
        let junction_pictures = splice::JunctionDisplay::new(render_state.clone());
        let thumbnails =
            thumbnails::Thumbnails::new(context.egui_ctx.clone(), render_state.clone())?;
        let mut app = Self {
            #[cfg(feature = "ui-harness")]
            feedback: harness::Feedback::default(),
            smoke_frames: smoke_test.then_some(0),
            display: accessibility::Display::fixed(),
            close_pending: false,
            exited,
            worker,
            endpoint_worker,
            playback,
            #[cfg(target_os = "macos")]
            _lifecycle: lifecycle,
            playback_interrupted,
            transport: None,
            monitor_gain: 0.125,
            audition_context: playback::AuditionContext::default(),
            monitor_control: None,
            resume: None,
            service,
            repeat_queue: repeat_queue::Queue::default(),
            dialogs: Dialogs::default(),
            dialog_intent: None,
            render_state,
            renderer,
            junction_pictures,
            thumbnails,
            transcription: transcript::Transcription::default(),
            shots: shots::ShotJob::default(),
            proxies: proxies::ProxyJob::default(),
            youtube: youtube::Flow::new(crate::youtube::Jobs::new(
                Arc::new(crate::youtube::Pinned::new(None)),
                None,
                {
                    let repaint = context.egui_ctx.clone();
                    Arc::new(move || repaint.request_repaint())
                },
            )),
            #[cfg(target_os = "macos")]
            menu: None,
            target: None,
            workspace: None,
            import: None,
            render_job: None,
            render: render::State::default(),
            selected_source: None,
            selected_sound: None,
            selected_event: None,
            sound_inspection: None,
            reveal_event: false,
            sound_command_target: None,
            hold_command_target: None,
            room_tone: None,
            correction: None,
            gain_command_target: None,
            gain: None,
            slip_command_target: None,
            slip: None,
            slip_abandon: None,
            slip_picture_pending: false,
            trim: None,
            trim_command_target: None,
            trim_prefix_target: None,
            trim_abandon: VecDeque::new(),
            ai: ai_pause::State::default(),
            targets: targets::State::default(),
            trim_picture_pending: false,
            splice: None,
            splice_abandon: None,
            sound_cursor: 0,
            selected_beat: None,
            scoped: None,
            scoped_command_target: None,
            sequence_scope: SequenceScope::default(),
            scope_start: 0,
            scope_end: 0,
            scope_labels: Vec::new(),
            view: View::Source,
            pane: Pane::Viewer,
            source_cursor: 0,
            moment: moment::Selection::default(),
            copied: copied::Register::default(),
            edit_range: edit_range::Selection::default(),
            placement_command_target: None,
            copy_command_register: None,
            register_command_cancelled: false,
            delete_command_target: None,
            frame_delete_command_target: None,
            macros: macros::State::default(),
            macro_prefix_target: None,
            operator_target: None,
            repeat_prefix_target: None,
            macro_command_target: None,
            marks: marks::State::default(),
            sequence_cursor: 0,
            source_search: String::new(),
            command: String::new(),
            command_open: false,
            command_focus_pending: false,
            bindings: keymap.bindings,
            keymap_status: keymap.status.clone(),
            keymap_error: keymap.failed,
            text_entry_gate: editor_input::TextEntryGate::default(),
            deferred_text_input: Vec::new(),
            camera: None,
            camera_pending: None,
            zoom_note: None,
            zoom_command_target: None,
            ime_composing: false,
            help_open: false,
            help_scroll: help_scroll::HelpScroll::default(),
            help_registers_first: false,
            help_expansion: None,
            edit_role: deadpan_core::MediaRole::Linked,
            edit_role_context: None,
            linked_import: false,
            audio_import: false,
            sound_stream: None,
            sound_interpretation: None,
            revealed_import_error: None,
            last_committed: None,
            semantic: semantic::Mirror::default(),
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
            message: keymap.failed.then_some(keymap.status),
            recovery: recovery::RecoveryUi::default(),
            models: model_packs::Models::default(),
            diagnostics: diagnostics::State::default(),
            storage: storage::State::default(),
            jobs: jobs::State::default(),
            gag_presets: crate::gag_presets::GagPresets::native(),
            bundled_sounds: crate::keymap_file::application_support_directory()
                .ok()
                .map(|directory| directory.join("Deadpan").join("Sounds")),
        };
        app.worker.set_proxy_cache(app.proxies.cache.clone());
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

    fn keymap_status(&self) -> &str {
        &self.keymap_status
    }

    fn submit(&mut self, request: ProjectRequest) -> bool {
        self.cancel_repeats("another project action");
        self.submit_now(request)
    }

    fn submit_now(&mut self, request: ProjectRequest) -> bool {
        if !self.macro_request_allowed(&request) {
            return false;
        }
        self.cancel_camera();
        self.stop_playback();
        self.bindings.clear();
        match self.service.submit(request) {
            Ok(()) => {
                #[cfg(feature = "ui-harness")]
                self.feedback.record("command_admitted");
                self.error = None;
                true
            }
            Err(error) => {
                #[cfg(feature = "ui-harness")]
                self.feedback.record("command_rejected");
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
        self.stop_playback();
        self.reset_picture();
        self.worker.clear();
    }

    /// Clear presentation while a self-contained request replaces the picture.
    /// The worker keeps its verified decoder until the session/asset key changes.
    fn reset_picture(&mut self) {
        self.cancel_camera();
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
            transport: None,
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
        let mark_position = self.capture_mark().ok();
        self.marks.navigated(mark_position.as_ref());
        self.reconcile_moment();
        self.reconcile_edit_range();
        // Pointer navigation also reaches this boundary. Revoke the draft in
        // this frame, before a later inspector widget could apply its old scope.
        self.cancel_camera();
        self.stop_playback();
        self.request_picture_for_transport(clear, None);
    }

    fn request_picture_for_transport(
        &mut self,
        clear: bool,
        transport: Option<deadpan_output::Generation>,
    ) {
        self.request_picture_for_transport_at(clear, transport, None);
    }

    fn request_picture_for_transport_at(
        &mut self,
        clear: bool,
        transport: Option<deadpan_output::Generation>,
        picture: Option<u64>,
    ) {
        // Trim's exact boundary pair stays fixed throughout audition, pause,
        // and stop. Its endpoint worker owns both inspection pictures.
        if self.trim.is_some() {
            return;
        }
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
        let work = if self.slip.is_some() {
            let Some(work) = self.slip_picture_work() else {
                self.worker.cancel();
                self.presentation.invalidate_pending();
                return;
            };
            work
        } else if self.splice.is_some() {
            let Some(work) = self.splice_picture_work(picture) else {
                // A refined copied source may still be awaiting admission.
                // Keep the displayed picture, but reject the preceding draft's
                // unfinished decode while no replacement can be submitted.
                self.worker.cancel();
                self.presentation.invalidate_pending();
                return;
            };
            work
        } else if let Some(work) = self.ai_picture_work(picture) {
            work
        } else if let Some(workspace) = &self.workspace {
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
                            picture
                                .unwrap_or(self.source_cursor)
                                .min(self.source_length().saturating_sub(1)),
                        ),
                    }
                }
                View::Sequence => ProjectView::Sequence {
                    frame: ProjectFrame(
                        picture
                            .unwrap_or(self.sequence_cursor)
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
            transport,
            source: self.preview_source,
            request: serial,
        };
        self.presentation.request(ticket, &work);
        #[cfg(feature = "ui-harness")]
        self.feedback.picture_requested(ticket);
        self.worker.submit(ticket, work);
    }

    fn receive(&mut self, context: &egui::Context) {
        #[cfg(feature = "ui-harness")]
        let update = self.feedback.take_project_update(&self.service);
        #[cfg(not(feature = "ui-harness"))]
        let update = self.service.take_update();
        if let Some(mut update) = update {
            let scoped_before = self.scoped_target().ok().flatten();
            let scoped_commit_selects = update
                .committed
                .as_ref()
                .and_then(|commit| commit.scoped.as_ref())
                .is_none_or(|receipt| {
                    self.view == View::Sequence
                        && !matches!(self.pane, Pane::Sources | Pane::Sounds)
                        && !self.event_focused()
                        && !self.sound_focused()
                        && scoped_before.as_ref() == Some(&receipt.before)
                });
            self.reconcile_repeat_prefix();
            self.prepare_macro_update(&mut update);
            if let Some(saved) = &update.marks.saved
                && saved.needs_refresh(update.workspace.as_deref())
                && let Some(warning) = &saved.refresh_error
            {
                update.message = Some(warning.clone());
            }
            let unrefreshed_cut = update
                .saved_cut
                .as_ref()
                .is_some_and(|receipt| receipt.needs_refresh(update.workspace.as_deref()));
            if unrefreshed_cut
                && let Some(message) = update
                    .saved_cut
                    .as_ref()
                    .and_then(|receipt| receipt.refresh_error.as_ref())
            {
                update.message = Some(message.clone());
            }
            if update.committed.is_none()
                && let Some(receipt) = update.saved_cut.as_ref()
                && update.workspace.as_ref().is_some_and(|workspace| {
                    workspace.session == receipt.copied.id().session
                        && workspace.document.project_id() == &receipt.copied.id().project
                        && workspace.document.revision_id() == &receipt.committed.revision
                })
            {
                update.committed = Some(receipt.committed.clone());
            }
            self.finish_trim_update(&mut update);
            self.finish_slip_update(&mut update);
            self.finish_gain_commit(&update);
            self.receive_room_tone(update.room_tone, update.room_tone_error);
            self.receive_gain(update.gain);
            let repeat_completion = match self.repeat_queue.matching_completion(
                update.workspace.as_ref().map(|workspace| workspace.session),
                update
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.document.as_ref()),
                update.committed.as_ref(),
                update.error.is_some(),
            ) {
                Ok(matched) => matched,
                Err(reason) => {
                    self.cancel_repeats(reason);
                    false
                }
            };
            // Preserve pending input without ever swapping the admitted map for
            // shipped defaults while the completion is reconciled.
            let repeat_bindings = repeat_completion.then(|| self.bindings.clone());
            let repeat_continuation = repeat_completion && self.repeat_prefix_can_continue();
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
            // A zoom note belongs to the commit of its own base revision in
            // its session; any other revision change or an error drops it.
            let zoom_note = self.zoom_note.take().and_then(|note| {
                let landed = Some(note.session) == new_session
                    && old_revision.as_ref() == Some(&note.base)
                    && new_revision
                        .as_ref()
                        .is_some_and(|revision| revision != &note.base)
                    && update
                        .committed
                        .as_ref()
                        .is_some_and(|commit| Some(&commit.revision) == new_revision.as_ref());
                if landed {
                    Some(note.text)
                } else {
                    // Updates without a workspace or with an unchanged head
                    // (progress, queries) keep it for the pending commit.
                    if update.error.is_none()
                        && (new_session.is_none()
                            || (new_revision == old_revision && new_session == old_session))
                    {
                        self.zoom_note = Some(note);
                    }
                    None
                }
            });
            self.workspace = update.workspace;
            self.receive_targets(update.targets.take());
            // A target save changes only the revision. Camera continues on
            // the revision its own save created; the picture is not cleared.
            let target_save = self.targets.saved().is_some_and(|saved| {
                Some(saved.session) == new_session
                    && Some(&saved.base) == old_revision.as_ref()
                    && Some(&saved.revision) == new_revision.as_ref()
            });
            let saved_target = self.targets.saved().filter(|_| target_save).cloned();
            // Rebase scoped inspection first: a scoped Camera compares against it.
            self.reconcile_scoped(
                scoped_before.as_ref(),
                update
                    .committed
                    .as_ref()
                    .and_then(|commit| commit.scoped.as_ref()),
                update.marks.saved.as_ref(),
                saved_target.as_ref(),
            );
            if target_save
                && old_session == new_session
                && let Some(revision) = new_revision.clone()
                && self.camera_follows_target_save(Some(&revision))
            {
                self.rebase_camera(revision);
            }
            self.semantic.receive(
                update.semantic,
                self.workspace
                    .as_ref()
                    .map(|workspace| (workspace.session, workspace.document.project_id())),
            );
            self.copied.reconcile(self.workspace.as_deref());
            if let Some(bank) = update.registers {
                self.copied.install_bank(&bank);
            }
            self.receive_splice(update.splice, update.splice_commit);
            self.receive_slip(update.slip);
            self.receive_trim(update.trim);
            self.import = update.import;
            if old_session != new_session {
                self.render_session_changed();
                self.macros.session_changed();
                self.bindings.set_macro_recording(false);
                self.macro_prefix_target = None;
                self.operator_target = None;
                self.repeat_prefix_target = None;
                self.macro_command_target = None;
                self.marks = marks::State::default();
                self.trim_prefix_target = None;
                self.trim_command_target = None;
            }
            let incoming_identity = update
                .render
                .as_ref()
                .and_then(|update| update.workflow.as_ref())
                .filter(|workflow| {
                    self.workspace.as_ref().is_some_and(|workspace| {
                        workspace.session == workflow.context.session
                            && workspace.document.project_id() == &workflow.context.project
                    })
                })
                .and_then(|workflow| workflow.status.identity.as_ref());
            let previous_identity = self
                .render_job
                .as_ref()
                .and_then(|update| update.workflow.as_ref())
                .and_then(|workflow| workflow.status.identity.as_ref());
            if incoming_identity.is_some() && incoming_identity != previous_identity {
                self.render.open = true;
            }
            self.render_job = update.render;
            self.receive_render_history(update.render_history);
            self.receive_recovery(
                update.opened.take(),
                update.storage.take(),
                update.relink.take(),
            );
            self.receive_storage_cleanup(update.storage_cleanup.take());
            self.receive_storage_retention(update.storage_retention.take());
            self.receive_backups(std::mem::take(&mut update.backups));
            self.project_error = update.error;
            self.message = update.message;
            if let Some(note) = zoom_note {
                self.message = Some(match self.message.take() {
                    Some(message) => format!("{message}. {note}"),
                    None => note,
                });
            }
            if old_session != new_session {
                self.bindings.clear();
                self.clear_picture();
                self.raw_source = None;
                self.selected_source = None;
                self.selected_sound = None;
                self.selected_event = None;
                self.sound_inspection = None;
                self.reveal_event = false;
                self.sound_command_target = None;
                self.sound_cursor = 0;
                self.selected_beat = None;
                self.sequence_scope = SequenceScope::default();
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
            let commit_matches_visible = update.committed.as_ref().is_some_and(|commit| {
                self.workspace.as_ref().is_some_and(|workspace| {
                    selection::commit_matches_visible(
                        commit,
                        workspace.session,
                        workspace.document.project_id(),
                        workspace.document.revision_id(),
                    )
                })
            });
            let mut restored_scope = false;
            if let Some(commit) = update.committed.as_ref()
                && commit_matches_visible
                && scoped_commit_selects
                && self.last_committed.as_ref() != Some(&commit.revision)
                && commit.sound.is_none()
                && self.sequence_scope != commit.scope
            {
                self.sequence_scope = commit.scope.clone();
                restored_scope = true;
            }
            if old_revision != new_revision || old_session != new_session || restored_scope {
                self.bindings.clear();
                self.rebuild_rows();
            }
            let mut completion = selection::completion(
                update.committed.as_ref().map(|commit| &commit.revision),
                self.last_committed.as_ref(),
                completed,
            );
            // A durable receipt can outlive a failed workspace refresh. Keep
            // the visible cursor/selection bound to the snapshot we actually
            // have; the saved warning explains that reopening is required.
            let unrefreshed_commit =
                unrefreshed_cut || (update.committed.is_some() && !commit_matches_visible);
            if completion == selection::Completion::Edit && unrefreshed_commit {
                completion = selection::Completion::None;
            }
            if !scoped_commit_selects {
                if commit_matches_visible {
                    self.last_committed = update
                        .committed
                        .as_ref()
                        .map(|commit| commit.revision.clone());
                }
                completion = selection::Completion::None;
            }
            let committed_selection = completion == selection::Completion::Edit;
            let preserve_picture = committed_selection
                && old_session == new_session
                && update
                    .committed
                    .as_ref()
                    .is_some_and(|commit| commit.preserve_cursor);
            self.view
                .set(self.view.after_completion(&completion), &mut self.message);
            let mut committed_range = None;
            if let Some(commit) = update.committed.filter(|_| committed_selection) {
                #[cfg(feature = "ui-harness")]
                self.feedback.record("command_committed");
                self.last_committed = Some(commit.revision);
                committed_range = commit.range_selection;
                self.bindings.clear();
                if let Some(sound) = commit.sound {
                    self.selected_event = sound.selected;
                    self.reveal_event = true;
                    self.pane = Pane::Sounds;
                } else {
                    self.selected_event = None;
                    self.pane = Pane::Sequence;
                    if let Some(cursor) = commit.cursor.and_then(|at| u64::try_from(at.0).ok()) {
                        self.sequence_cursor = cursor;
                    }
                    let selected = selection::after_commit(
                        &self.beat_rows,
                        commit.selected_node.as_ref(),
                        commit.cursor,
                    );
                    self.selected_beat = selected.map(|index| self.beat_rows[index].id.clone());
                    if let Some(index) = selected {
                        let beat = &self.beat_rows[index];
                        if commit.cursor.is_none() && !commit.preserve_cursor {
                            self.sequence_cursor = beat.start;
                        }
                        self.reveal_beat = true;
                    }
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
            self.reconcile_moment();
            self.rebase_mark_metadata(&update.marks);
            self.marks.reconcile(self.workspace.as_deref());
            self.reconcile_edit_range();
            if let Some(range) = committed_range {
                self.select_committed_range(&range);
            }
            self.reconcile_events();
            if self.view == View::Sequence && !committed_selection {
                self.reconcile_beat_selection();
            }
            let ai_picture = self.receive_ai(update.generation);
            if old_revision != new_revision || old_session != new_session || completed {
                if self.trim.is_some() || self.trim_picture_pending {
                    // Restore the ordinary stopped picture only after Trim closes.
                    self.trim_picture_pending = true;
                } else if self.slip.is_some() || self.slip_picture_pending {
                    // Slip's final layout pass owns its next stopped picture.
                    self.slip_picture_pending = true;
                } else {
                    // A rebasing Camera keeps its draft across this request.
                    let camera = self.camera.take_if(|camera| camera.rebasing());
                    self.request_picture(!preserve_picture && !target_save);
                    if camera.is_some() {
                        self.camera = camera;
                    }
                }
            } else if ai_picture && self.trim.is_none() && self.slip.is_none() {
                self.request_picture(false);
            }
            if repeat_completion {
                if let Some(target) = self.repeat_target() {
                    self.repeat_queue.completed(target);
                } else {
                    self.cancel_repeats("the committed selection is unavailable");
                }
            }
            if let Some(bindings) = repeat_bindings {
                self.bindings = bindings;
            }
            if repeat_continuation {
                self.advance_repeat_prefix();
            }
            self.reconcile_repeat_prefix();
            self.receive_original_copy(update.captured_original, unrefreshed_commit);
            self.receive_copied(update.captured_slice, unrefreshed_commit);
            self.receive_macro_cut(update.cut_slice.as_ref(), update.saved_cut.as_ref());
            self.receive_cut(update.cut_slice);
            self.receive_macro(update.macros, update.saved_macro);
            self.receive_marks(update.marks, context);
            self.transcription.receive_save(update.transcript_save);
            self.transcription
                .receive_activity_save(update.activity_save);
            self.shots.receive_save(update.shot_save);
            self.receive_correction_save(update.correction_save);
            // A service publication can replace the captured head before a
            // stopped decode is polled below. Revoke that proposal now, not
            // after receive returns or when final layout requests its successor.
            self.reconcile_trim(context);
            self.reconcile_slip(context);
        }
        #[cfg(feature = "ui-harness")]
        let reply = self.feedback.take_reply(&self.worker);
        #[cfg(not(feature = "ui-harness"))]
        let reply = self.worker.take_reply();
        let retain_draft_display = self.slip.is_some() || self.trim.is_some();
        let result = reply.and_then(|reply| {
            #[cfg(feature = "ui-harness")]
            let (ticket, timing) = (reply.ticket, reply.timing);
            let result = if retain_draft_display {
                self.presentation.receive_retaining_display(reply)
            } else {
                self.presentation.receive(reply)
            };
            #[cfg(feature = "ui-harness")]
            self.feedback
                .picture_received(ticket, result.as_ref().map(Result::is_ok), timing);
            result
        });
        if let Some(result) = result {
            match result {
                Ok(summary) => {
                    if summary.is_some() {
                        self.summary = summary;
                    }
                }
                Err(_) => {
                    // A failed refinement keeps its proxy picture displayed.
                    if !retain_draft_display && !self.presentation.has_displayed() {
                        self.forget_target();
                    }
                }
            }
        }
    }

    /// Read and build seek proxies in `cache` (and keep the setting in
    /// `settings`) instead of the per-user locations.
    #[cfg(feature = "ui-harness")]
    pub(crate) fn use_proxy_locations(
        &mut self,
        cache: Option<deadpan_cli::proxy::cache::ProxyCache>,
        settings: Option<PathBuf>,
    ) {
        self.proxies.use_settings(settings);
        self.proxies.use_cache(cache.clone());
        self.worker.set_proxy_cache(cache);
    }

    /// A user action that needs the project writer while another project
    /// command (an edit, open/create, or a remote command) holds it. Say so
    /// visibly and keep any earlier error instead of replacing it.
    pub(super) fn refuse_while_busy(&mut self, action: &str) {
        let refusal = format!(
            "{action} did not start because another project command is still in progress. Try again when it finishes."
        );
        self.error = Some(match self.error.take() {
            Some(previous) if previous.contains(&refusal) => previous,
            Some(previous) => format!("{}. {refusal}", previous.trim_end_matches('.')),
            None => refusal,
        });
    }

    fn begin_dialog(&mut self, kind: DialogKind, context: &egui::Context, preview_only: bool) {
        self.cancel_repeats("a file action was requested");
        // An open dialog is itself visible; a busy writer must say why the
        // request did nothing rather than drop it silently.
        if self.dialogs.is_open() {
            return;
        }
        if self.service.is_busy() {
            let action = match kind {
                DialogKind::CreateProject => "New project",
                DialogKind::InitializeSource => "Choose Original",
                DialogKind::OpenProject => "Open project",
                DialogKind::ImportSound => "Import sound",
                DialogKind::ImportMedia => "Import media",
                DialogKind::Render => "Render",
                DialogKind::Cookies => "Choose cookies",
                DialogKind::RelinkOriginal => "Locate Original",
                DialogKind::ModelPackFolder | DialogKind::ModelPackArchive => "Choose model pack",
                DialogKind::SignedUpdate => "Choose signed update",
                DialogKind::PortableCopy => "Save portable copy",
            };
            self.refuse_while_busy(action);
            return;
        }
        self.cancel_camera();
        self.pause_playback();
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
                // Choosing or opening another project leaves an idle URL sheet.
                if matches!(kind, DialogKind::CreateProject | DialogKind::OpenProject)
                    && !self.youtube.active()
                {
                    self.youtube.modal = false;
                }
                self.dialog_intent = Some(DialogIntent {
                    session: self.workspace.as_ref().map(|w| w.session),
                    revision: self
                        .workspace
                        .as_ref()
                        .map(|w| w.document.revision_id().clone()),
                    media: if kind == DialogKind::ImportSound || self.audio_import {
                        ImportMedia::sound(self.sound_stream, self.sound_interpretation)
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

    fn receive_dialog(&mut self, context: &egui::Context) {
        let Some(result) = self.dialogs.take_result() else {
            return;
        };
        if matches!(
            result.kind,
            DialogKind::ModelPackFolder | DialogKind::ModelPackArchive
        ) {
            self.receive_model_source(result.path, result.error, context);
            return;
        }
        if result.kind == DialogKind::SignedUpdate {
            self.receive_signed_update(result.path, result.error, context);
            return;
        }
        if result.kind == DialogKind::PortableCopy {
            if let Some(error) = result.error {
                self.message = Some(error);
            } else if let Some(path) = result.path {
                self.receive_portable_copy_dialog(path);
            }
            return;
        }
        if let Some(error) = result.error {
            if result.kind == DialogKind::Render {
                self.render_dialog_failed(error);
            } else {
                self.error = Some(error);
                self.dialog_intent = None;
            }
            return;
        }
        if result.kind == DialogKind::Render {
            self.receive_render_dialog(result.path);
            return;
        }
        let intent = self.dialog_intent.take();
        let Some(path) = result.path else {
            return;
        };
        match result.kind {
            DialogKind::Render => unreachable!("Render uses its captured destination intent"),
            DialogKind::Cookies => {
                // A running import already copied its cookies (or none).
                if self.youtube.active() {
                    self.message = Some(
                        "The cookies file was not applied because an import is running.".into(),
                    );
                } else {
                    self.youtube.cookies = Some(path);
                }
            }
            DialogKind::CreateProject => {
                self.submit(ProjectRequest::CreateFromSource { path });
            }
            DialogKind::OpenProject => {
                self.submit(ProjectRequest::Open(path));
            }
            DialogKind::RelinkOriginal => self.receive_relink_dialog(path),
            DialogKind::ModelPackFolder
            | DialogKind::ModelPackArchive
            | DialogKind::SignedUpdate => {
                unreachable!("model pack sources and updates return to the Models panel")
            }
            DialogKind::PortableCopy => unreachable!("portable copies return to Storage"),
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
                            ImportMedia::Audio { stream, .. } => Some(stream),
                            ImportMedia::FirstAudio { .. } | ImportMedia::Video => None,
                        },
                        interpretation: intent.media.interpretation(),
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
        if self.refuse_scoped_structure() {
            return;
        }
        let Some(workspace) = &self.workspace else {
            self.message = Some("Choose a source to insert.".into());
            return;
        };
        let Some(asset) = original_asset(workspace).or(self.selected_source.as_ref()) else {
            return;
        };
        let scope = match self.sequence_scope.resolve(workspace) {
            Ok(scope) => scope,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let children = scope.children;
        let index = self
            .selected_beat
            .as_ref()
            .and_then(|id| children.iter().position(|child| child == id))
            .map_or(children.len(), |n| n + 1);
        let request = ProjectRequest::Insert {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            scope: self.sequence_scope.clone(),
            asset: asset.clone(),
            parent: scope.owner.clone(),
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
        self.cancel_repeats("history navigation was requested");
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
        if self.refuse_scoped_structure() {
            return;
        }
        self.bindings.clear();
        if edit == BeatEdit::Delete {
            if !self.record_macro_delete(self.copied.selected(), None) {
                self.delete_captured(self.capture_delete_target());
            }
            return;
        }
        if !matches!(edit, BeatEdit::WrapRepeat(_)) {
            self.cancel_repeats("another edit was requested");
        }
        if self.view == View::Source {
            self.offer_insert();
            return;
        }
        if let BeatEdit::WrapRepeat(plays) | BeatEdit::Repeat(plays) = edit {
            let target = self.capture_macro_target();
            self.repeat_command(Some(target), plays, matches!(edit, BeatEdit::Repeat(_)));
            return;
        }
        if let BeatEdit::Escalate(input) = edit {
            self.repeat_change(input);
            return;
        }
        if let BeatEdit::InsertHold(input) | BeatEdit::InsertBlack(input) = edit {
            let black = matches!(edit, BeatEdit::InsertBlack(_));
            let Some(workspace) = &self.workspace else {
                self.error = Some("Open a project before inserting a pause.".into());
                return;
            };
            let duration = match input.resolve(workspace.document.presentation_basis().frame_rate) {
                Ok(duration) => duration,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            };
            if duration == deadpan_core::FrameDuration::ZERO {
                self.error = None;
                self.message = Some("Pause resolves to 0 frames; no edit was made.".into());
                return;
            }
            if let Err(error) = self
                .sequence_scope
                .check_pause(workspace, ProjectFrame(self.sequence_cursor as i64))
            {
                self.error = Some(error);
                return;
            }
            // One semantic instruction, recorded by a macro and repeated by `.`.
            let length = input.pause_length(workspace.document.presentation_basis().frame_rate);
            let target = self.capture_macro_target();
            self.apply_recorded_instruction(
                target,
                length
                    .map(|length| deadpan_core::SemanticInstruction::InsertPause { length, black }),
            );
            return;
        }
        if let Some(instruction) = self.semantic_beat_edit(&edit) {
            // An offset gives a trimmed beat's sound its own clock, so the
            // common picture/sound window that Slip and Trim need ends.
            let ends_window = matches!(edit, BeatEdit::AudioLag { .. })
                && self
                    .workspace
                    .as_ref()
                    .zip(self.selected_beat.as_ref())
                    .and_then(|(workspace, node)| {
                        let (host, _) = deadpan_core::cutaway_host(&workspace.document, node)?;
                        match &workspace.document.nodes()[&host].kind {
                            NodeKind::Source { source } => Some(source.edit_window.is_some()),
                            _ => None,
                        }
                    })
                    .unwrap_or(false);
            let target = self.capture_macro_target();
            self.apply_recorded_instruction(target, instruction);
            if ends_window && self.macros.is_pending() {
                self.macros.set_summary("Sound offset saved. Slip and Trim no longer apply to this beat because its sound no longer shares the picture's clock; Undo restores them".into());
            }
            return;
        }
        let (Some(workspace), Some(node)) = (&self.workspace, &self.selected_beat) else {
            self.error = Some("Select a beat in the current group before editing.".into());
            return;
        };
        let node = node.clone();
        let edit = match edit {
            BeatEdit::InsertHold(_) | BeatEdit::InsertBlack(_) => {
                unreachable!("pause handled above")
            }
            BeatEdit::Split => {
                let Some(at) =
                    selection::split_boundary(&self.beat_rows, &node, self.sequence_cursor)
                else {
                    self.error = Some("Move inside the selected beat, then split at the cursor. Existing boundaries need no split.".into());
                    return;
                };
                ProjectEdit::Split { node, at }
            }
            BeatEdit::Repeat(plays) => ProjectEdit::Repeat { node, plays },
            BeatEdit::Escalate(_) => unreachable!("Repeat changes handled above"),
            BeatEdit::Cutaway(input) => match self.cutaway_edit(&node, input) {
                Ok(edit) => edit,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            },
            BeatEdit::WrapRepeat(_) => unreachable!("Repeat continuation handled above"),
            BeatEdit::Delete => unreachable!("deletion captured above"),
            BeatEdit::HoldDuration(_)
            | BeatEdit::AudioLag { .. }
            | BeatEdit::Retime(_)
            | BeatEdit::Pitch(_) => unreachable!("committed as semantic instructions above"),
        };
        self.submit(ProjectRequest::Edit {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            scope: self.sequence_scope.clone(),
            cursor: ProjectFrame(self.sequence_cursor as i64),
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
        if self.scoped.is_some() {
            return;
        }
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
        // History and background refresh preserve the absolute cursor. A valid
        // selected identity may have moved; do not turn that into a seek.
    }

    fn open_command(&mut self, command: String, context: &egui::Context) {
        self.scoped_command_target = Some(self.scoped_target());
        self.marks.command = Some(self.capture_mark());
        self.placement_command_target = Some(self.capture_placement_target());
        self.copy_command_register = self.copied.selected().and_then(|name| {
            self.workspace
                .as_ref()
                .map(|workspace| (workspace.session, name))
        });
        self.register_command_cancelled = false;
        self.delete_command_target = Some(self.capture_delete_target());
        self.frame_delete_command_target = Some(self.capture_frame_delete_target());
        self.macro_command_target = Some(self.capture_macro_target());
        self.macros.capture_command();
        self.macro_prefix_target = None;
        self.gain_command_target = Some(self.capture_gain_target());
        self.slip_command_target = Some(self.capture_slip_target());
        // Capture success or absence before pausing delivery or changing focus.
        self.trim_command_target = Some(self.capture_trim_target());
        self.trim_prefix_target = None;
        self.hold_command_target = Some(self.capture_hold_command());
        self.ai.command = Some(self.ai_capture());
        self.targets.command = Some(self.capture_track());
        self.zoom_command_target = Some(self.capture_zoom());
        self.sound_command_target = self.capture_sound_command(&command);
        self.cancel_repeats("command entry was opened");
        self.pause_playback();
        self.bindings.clear();
        self.command_open = true;
        self.command = command;
        // A new command is a fresh native text session. Retaining the previous
        // field's selection puts same-batch typing inside a longer prefill,
        // and retaining its undo history can restore an unrelated command.
        let mut text_state = egui::text_edit::TextEditState::default();
        text_state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(self.command.chars().count()),
            )));
        text_state.store(context, egui::Id::new(COMMAND_ID));
        // Inspector actions run after the footer. Focusing its absent field
        // would publish an invalid accessibility tree for this frame.
        // Keep the real origin pane focused until the field is drawn, so later
        // pane observers do not restore the previously focused pane.
        context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
        self.command_focus_pending = true;
        context.request_repaint();
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
        self.stop_playback();
        let same_source = self.raw_source.is_none() && self.selected_source.as_ref() == Some(&id);
        self.selected_sound = None;
        self.selected_source = Some(id);
        self.selected_event = None;
        self.raw_source = None;
        if !same_source {
            self.source_cursor = 0;
        }
        self.view.set(View::Source, &mut self.message);
        self.pane = Pane::Sources;
        self.reveal_source = true;
        self.request_picture(!same_source);
    }

    fn select_sound(&mut self, id: AssetId) {
        if self
            .workspace
            .as_ref()
            .and_then(|w| w.sources.get(&id))
            .and_then(|source| source.sound_audition.as_ref())
            .is_none()
        {
            return;
        }
        self.stop_playback();
        self.bindings.clear();
        self.selected_sound = Some(id);
        self.selected_event = None;
        self.reveal_source = true;
        self.sound_cursor = 0;
        self.pane = Pane::Sources;
        self.error = None;
    }

    fn rebuild_rows(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.beat_rows = Arc::new(Vec::new());
            self.scope_start = 0;
            self.scope_end = 0;
            self.scope_labels.clear();
            self.source_rows = Arc::new(Vec::new());
            self.sound_rows = Arc::new(Vec::new());
            return;
        };
        self.sequence_scope.reconcile(workspace);
        let scope = match self.sequence_scope.resolve(workspace) {
            Ok(scope) => scope,
            Err(error) => {
                self.error = Some(error);
                self.beat_rows = Arc::new(Vec::new());
                return;
            }
        };
        self.scope_start = scope.start;
        self.scope_end = scope.end;
        self.scope_labels = self
            .sequence_scope
            .groups()
            .iter()
            .map(|id| workspace.document.nodes()[id].label.clone())
            .collect();
        let mut start = scope.start;
        self.beat_rows = Arc::new(
            scope
                .children
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
        if self.scoped_action(action, context) {
            return;
        }
        if !self.macro_action_allowed(action) {
            if action == Action::RepeatLast && self.last_edit_uses_register() {
                self.copied.begin_write();
            }
            return;
        }
        if matches!(
            action,
            Action::MacroRecord(_)
                | Action::MacroExecute { .. }
                | Action::MacroStop
                | Action::MacroCancel
        ) {
            let target = self.macro_prefix_target.take();
            self.macro_action(action, target);
            return;
        }
        if self.trim.is_some() {
            self.error = Some("Finish or cancel Trim before other editor commands.".into());
            return;
        }
        if self.slip.is_some() {
            self.error = Some("Finish or cancel Slip preview before other editor commands.".into());
            return;
        }
        if !matches!(
            action,
            Action::SetMark(_) | Action::JumpMark(_) | Action::DeleteMark(_)
        ) {
            self.marks.cancel_jump();
        }
        if matches!(
            action,
            Action::CopyMoment
                | Action::Operator { .. }
                | Action::DeleteSelection
                | Action::DeleteFrames(_)
        ) || (action == Action::RepeatLast && self.last_edit_uses_register())
            || (action == Action::Edit(BeatEdit::Delete)
                && self.pane != Pane::Sounds
                && !self.event_focused())
        {
            // A newer yank or picture cut owns register intent even when its
            // focused pane rejects it. Sound deletion is an independent action.
            self.copied.supersede();
        }
        if action == Action::Render {
            // Fields below the header must consume this frame's native text
            // before the exact preview proposal is captured.
            self.render.requested = true;
            return;
        }
        // OfferInsert also represents an unfinished operator (including the
        // first r of rr). In Sequence it is only a hint, not another action.
        let repeat_input = matches!(action, Action::Edit(BeatEdit::WrapRepeat(_)))
            || matches!(
                action,
                Action::Repeat {
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                    ..
                }
            )
            || (matches!(action, Action::OfferInsert) && self.view == View::Sequence);
        if !repeat_input {
            self.cancel_repeats("another action was requested");
        }
        if action == Action::Trim {
            let target = self.trim_prefix_target.take();
            self.open_trim(target, navigation::trim::TrimInput::default(), context);
            return;
        }
        if self.pane == Pane::Sounds || self.event_focused() {
            match action {
                Action::Step { forward, count } => {
                    self.move_event(forward, count, context);
                    return;
                }
                Action::Beat { forward, count } => {
                    self.step_event(forward, count, context);
                    return;
                }
                Action::First | Action::Last => {
                    self.selected_event = None;
                    self.step_event(action == Action::First, 1, context);
                    return;
                }
                Action::EnterGroup => {
                    self.open_sound_position(context);
                    return;
                }
                Action::Edit(BeatEdit::Delete)
                | Action::Operator {
                    cut: true,
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                } => {
                    self.sound_action(navigation::SoundAction::Delete, context);
                    return;
                }
                Action::Edit(_)
                | Action::Framing(_)
                | Action::LeaveGroup
                | Action::VisualMoment
                | Action::SelectObject(_)
                | Action::DeleteSelection
                | Action::DeleteFrames(_)
                | Action::Operator { .. }
                | Action::Repeat { .. }
                | Action::Group
                | Action::Ungroup
                | Action::Explode
                | Action::Duplicate
                | Action::RepeatLast
                | Action::CopyMoment
                | Action::PasteMoment { .. } => {
                    if matches!(
                        action,
                        Action::CopyMoment
                            | Action::Operator { .. }
                            | Action::DeleteSelection
                            | Action::DeleteFrames(_)
                            | Action::PasteMoment { .. }
                    ) || (action == Action::RepeatLast && self.last_edit_uses_register())
                    {
                        self.copied.clear_selection();
                    }
                    self.bindings.clear();
                    self.error = Some("Focus Beats to edit the video structure. Placed sounds support frame nudges, exact position, gain and removal.".into());
                    return;
                }
                _ => {}
            }
        }
        if self.sound_focused() && !sound_action_allowed(action) {
            if matches!(
                action,
                Action::CopyMoment
                    | Action::Operator { .. }
                    | Action::DeleteSelection
                    | Action::DeleteFrames(_)
                    | Action::PasteMoment { .. }
                    | Action::Edit(BeatEdit::Delete)
            ) || (action == Action::RepeatLast && self.last_edit_uses_register())
            {
                self.copied.clear_selection();
            }
            self.bindings.clear();
            self.message = Some(
                "Choose Original or Your edit for editing commands. Sounds can be auditioned here."
                    .into(),
            );
            return;
        }
        if self.camera.is_some()
            && !self.sound_focused()
            && !matches!(action, Action::Framing(_) | Action::RepeatLast)
        {
            self.cancel_camera();
        }
        match action {
            Action::MacroRecord(_)
            | Action::MacroExecute { .. }
            | Action::MacroStop
            | Action::MacroCancel => unreachable!("macros handle their captured targets first"),
            Action::Render => unreachable!("Render handles previews before ordinary actions"),
            Action::Trim => {
                unreachable!("Trim handles its captured target before ordinary actions")
            }
            Action::SetMark(_) | Action::JumpMark(_) | Action::DeleteMark(_) => {
                let captured = self.marks.prefix.take();
                self.mark_action(action, captured);
            }
            Action::JumpHistory { forward } => self.jump_history(forward, context),
            Action::Marks => self.open_marks(context),
            Action::Sound(action) => self.sound_action(action, context),
            Action::GainStep(delta) => self.gain_step(delta, context),
            Action::Mute => self.mute_key(),
            Action::CutawayPicker => self.pick_cutaway(context),
            Action::TailPicker => self.pick_tail(context),
            Action::Lift => self.lift_selection(),
            Action::Bleep {
                frequency_hz,
                level_millidecibels,
            } => self.bleep_selection(frequency_hz, level_millidecibels),
            Action::Reverse { length, bounce } => self.apply_reverse(length, bounce),
            Action::SplitEdit { kind, length } => self.apply_split_edit(kind, length),
            Action::RoleRepeat { role, plays, trim } => {
                self.role_repeat(role, plays, trim, self.capture_macro_target())
            }
            Action::Tail { length, effect } => self.apply_tail(length, effect),
            Action::SaveFraming(name) => self.save_framing_preset(name),
            Action::Framing(action) => self.framing_action(action, context),
            Action::New => self.begin_dialog(DialogKind::CreateProject, context, false),
            Action::NewFromUrl => self.open_youtube(context),
            Action::Open => self.begin_dialog(DialogKind::OpenProject, context, false),
            Action::Import => self.begin_dialog(self.import_dialog_kind(), context, false),
            Action::Insert => self.insert(),
            Action::Group => self.open_command("group name=".into(), context),
            Action::Ungroup => self.group_command(Some(self.capture_macro_target()), None),
            Action::Explode => self.structure_command(Some(self.capture_macro_target()), true),
            Action::Duplicate => self.structure_command(Some(self.capture_macro_target()), false),
            Action::Undo => self.history(false),
            Action::Redo => self.history(true),
            Action::Playback => {
                self.toggle_playback();
                context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
            }
            Action::Audition => {
                self.audition_selection();
                context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
            }
            Action::EnterGroup => self.enter_group(context),
            Action::LeaveGroup => self.leave_group(context),
            Action::VisualMoment => {
                if self.view == View::Sequence {
                    self.visual_edit_range();
                } else {
                    self.visual_moment();
                }
            }
            Action::CopyMoment => self.copy_slice(),
            Action::SelectObject(object) => self.select_group_object(object),
            Action::Operator { cut, selector } => self.operator_action(cut, selector),
            Action::Repeat { selector, plays } => self.repeat_action(selector, plays),
            Action::SelectRegister(name) => self.select_register(name),
            Action::DeleteSelection if self.edit_role != deadpan_core::MediaRole::Linked => {
                self.delete_role(self.edit_role, self.capture_macro_target());
            }
            Action::DeleteSelection => {
                if !self.record_macro_delete(self.copied.selected(), None) {
                    self.delete_captured(self.capture_delete_target());
                }
            }
            Action::DeleteFrames(count) => {
                self.delete_frames_captured(self.capture_frame_delete_target(), count)
            }
            Action::RepeatLast => self.repeat_last_edit(),
            Action::PasteMoment { before } => self.paste_moment(before),
            Action::Edit(edit) => self.edit(edit),
            Action::Invalid(error) => self.error = Some(error.into()),
            Action::Pane { reverse } => {
                if self.sound_focused() {
                    self.stop_playback();
                }
                // Skip panes the current layout does not draw (the start
                // screen has no Placed sounds), so focus never names a
                // missing control.
                let inspector = self.inspector_visible();
                self.pane = self.pane.cycle_available(reverse, inspector, |pane| {
                    accessibility::pane_drawn(context, pane)
                });
                if self.pane == Pane::Sounds {
                    self.focus_events(context);
                }
                if !matches!(self.pane, Pane::Sounds | Pane::Inspector) {
                    self.selected_event = None;
                }
                self.reconcile_events();
                self.bindings.clear();
                context.memory_mut(|m| {
                    // egui computed Tab traversal before our router consumed
                    // the event. Do not leave a deferred native focus move that
                    // steals the next Enter from the explicitly chosen pane.
                    m.move_focus(egui::FocusDirection::None);
                    m.request_focus(pane_id(self.pane));
                });
            }
            Action::Step { forward, count } => {
                match self.view {
                    View::Source => {
                        self.source_cursor = navigation::boundary_step(
                            self.source_cursor,
                            self.source_length(),
                            forward,
                            count,
                        );
                        self.moment.move_to(self.source_cursor);
                    }
                    View::Sequence => {
                        self.sequence_cursor = navigation::boundary_step(
                            self.sequence_cursor.clamp(self.scope_start, self.scope_end)
                                - self.scope_start,
                            self.scope_end - self.scope_start,
                            forward,
                            count,
                        ) + self.scope_start
                    }
                }
                if self.view == View::Sequence {
                    self.edit_range.move_to(self.sequence_cursor);
                    self.select_at_cursor();
                }
                self.request_picture(false);
                self.record_macro_motion(forward, count);
            }
            Action::Word {
                forward,
                end,
                count,
            } => self.speech_motion(deadpan_core::SpeechMotion {
                forward,
                count,
                sentence: false,
                end,
            }),
            Action::Sentence { forward, count } => self.speech_motion(deadpan_core::SpeechMotion {
                forward,
                count,
                sentence: true,
                end: false,
            }),
            Action::EscalatingRepeat => self.escalating_repeat(),
            Action::Ai(action) => {
                // Key paths carry the `,a` ancestor capture; commands are
                // dispatched with their entry capture in run_command.
                let target = self.ai.prefix.take();
                self.ai_action(action, target);
            }
            Action::Gag(input) => self.apply_gag(input),
            Action::Pause { forward, count } => {
                self.analysis_motion(deadpan_core::SpeechUnit::Pause, forward, count)
            }
            Action::Shot { forward, count } => {
                self.analysis_motion(deadpan_core::SpeechUnit::Shot, forward, count)
            }
            Action::Play { forward, count } => self.scoped_play(forward, count, context),
            Action::SelectSpeech(object) => self.select_speech(object),
            Action::First | Action::Last => {
                let end = action == Action::Last;
                match self.view {
                    View::Source => {
                        self.source_cursor = if end { self.source_length() } else { 0 };
                        self.moment.move_to(self.source_cursor);
                    }
                    View::Sequence => {
                        self.sequence_cursor = if end {
                            self.scope_end
                        } else {
                            self.scope_start
                        }
                    }
                }
                if self.view == View::Sequence {
                    self.edit_range.move_to(self.sequence_cursor);
                    self.select_at_cursor();
                }
                self.request_picture(false);
                self.record_macro_local(deadpan_core::SemanticInstruction::MoveScope { end });
            }
            Action::Beat { forward, count } => {
                if self.pane == Pane::Sources && self.focused_workflow() {
                    let sounds = Arc::clone(&self.sound_rows);
                    if !sounds.is_empty() {
                        let current = self
                            .selected_sound
                            .as_ref()
                            .and_then(|id| sounds.iter().position(|sound| &sound.0 == id));
                        let next =
                            current.map_or(if forward { 0 } else { sounds.len() - 1 }, |index| {
                                navigation::boundary_step(
                                    index as u64,
                                    sounds.len().saturating_sub(1) as u64,
                                    forward,
                                    count,
                                ) as usize
                            });
                        self.select_sound(sounds[next].0.clone());
                    }
                } else if self.pane == Pane::Sources && !self.focused_workflow() {
                    let sources = Arc::clone(&self.source_rows);
                    if !sources.is_empty() {
                        let index = self
                            .selected_sound
                            .as_ref()
                            .or(self.selected_source.as_ref())
                            .and_then(|id| sources.iter().position(|source| &source.0 == id))
                            .unwrap_or(0);
                        let next = navigation::boundary_step(
                            index as u64,
                            sources.len().saturating_sub(1) as u64,
                            forward,
                            count,
                        ) as usize;
                        if sources[next].2 {
                            self.select_source(sources[next].0.clone());
                        } else {
                            self.select_sound(sources[next].0.clone());
                        }
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
                        self.selected_event = None;
                        self.selected_beat = Some(beats[next].id.clone());
                        self.sequence_cursor = beats[next].start;
                        self.view.set(View::Sequence, &mut self.message);
                        self.edit_range.move_to(self.sequence_cursor);
                        self.reveal_beat = true;
                        self.request_picture(true);
                    }
                    if let Some(count) = std::num::NonZeroU32::new(count) {
                        self.record_macro_local(deadpan_core::SemanticInstruction::MoveBeats {
                            forward,
                            count,
                        });
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
                // With a transcript, / searches its words; n and N then step
                // through matches in the current context.
                if self.transcription_ready() && !self.sound_focused() {
                    context.memory_mut(|m| m.request_focus(egui::Id::new(TRANSCRIPT_SEARCH_ID)));
                    return;
                }
                self.pane = Pane::Sources;
                context.memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_ID)));
            }
            Action::SearchStep { forward } => self.search_step(forward),
            Action::Command => {
                self.open_command(String::new(), context);
            }
            Action::Help => {
                self.pause_playback();
                if !self.help_open {
                    self.help_registers_first = false;
                    self.help_expansion = None;
                }
                self.help_open = true;
                self.bindings.clear();
            }
            Action::Escape => {
                // Other Escape owners clear first; only an otherwise idle
                // Escape leaves the AI preview. It never cancels generation.
                let owned = self.escape_owned_elsewhere();
                self.record_macro_escape();
                self.cancel_register_choice();
                if !self.sound_focused() {
                    self.moment.cancel();
                    self.edit_range.clear();
                }
                self.pause_playback();
                self.command_open = false;
                self.command_focus_pending = false;
                self.help_open = false;
                self.bindings.clear();
                if !owned && self.ai_stop_preview() {
                    self.message = Some("Showing your edit again.".into());
                }
                context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
            }
            Action::OfferInsert => {
                if self.sound_focused() {
                    self.message =
                        Some("Complete the displayed key sequence, or press Esc to cancel.".into());
                } else if self.view == View::Source {
                    self.message = Some(SOURCE_INSERT_HINT.into());
                }
            }
        }
    }

    fn keyboard(&mut self, context: &egui::Context) -> Option<(TextAction, bool)> {
        self.reconcile_edit_role();
        self.reconcile_macro_recording();
        self.reconcile_operator();
        self.reconcile_repeat_prefix();
        self.bindings.set_routing_domain(self.routed_domain());
        self.bindings.set_macro_recording(self.macros.recording());
        // The field can close before its opener is released. Keep ownership
        // through that transition and filter before any modal early return.
        let mut events = self
            .text_entry_gate
            .filter(context.input(|input| input.events.clone()));
        // Register choice is one-shot editor intent. Observe cancellation even
        // when a native field, help or preview owns delivery of the same key.
        let composing = self.ime_composing
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)));
        if events.iter().any(|event| {
            matches!(event, egui::Event::WindowFocused(false))
                || (!composing
                    && matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            modifiers: egui::Modifiers::NONE,
                            pressed: true,
                            repeat: false,
                            ..
                        }
                    ))
        }) {
            if self.command_open && self.copy_command_register.is_some() {
                self.register_command_cancelled = true;
            }
            self.cancel_register_choice();
        }
        context.input_mut(|input| input.events.clone_from(&events));
        if !self.bindings.trim_pending() {
            self.trim_prefix_target = None;
        }
        if !self.bindings.macro_pending() {
            self.macro_prefix_target = None;
        }
        // A recovery or close question owns input above every draft, so its
        // Escape can never cancel the draft it is asking about.
        if self.recovery_keyboard(context) {
            return None;
        }
        if self.trim.is_some() {
            self.trim_keyboard(context);
            return None;
        }
        if self.marks_keyboard(context) {
            return None;
        }
        if self.models_keyboard(context) {
            return None;
        }
        if self.diagnostics_keyboard(context) {
            return None;
        }
        if self.storage_keyboard(context) {
            return None;
        }
        if self.jobs_keyboard(context) {
            return None;
        }
        if self.render_keyboard(context) {
            return None;
        }
        if self.slip.is_some() {
            self.slip_keyboard(context);
            return None;
        }
        if self.splice.is_some() {
            self.splice_keyboard(context);
            return None;
        }
        if self.gain.is_some() {
            self.gain_keyboard(context);
            return None;
        }
        if self.room_tone.is_some() {
            self.room_tone_keyboard(context);
            return None;
        }
        if self.correction.is_some() {
            self.corrections_keyboard(context);
            return None;
        }
        if self.youtube_keyboard(context) {
            return None;
        }
        if help_scroll::defer_popup_input(
            context,
            self.dialogs.is_open(),
            &mut self.ime_composing,
            &mut self.bindings,
        ) {
            return None;
        }
        let help_search = context.memory(|m| m.has_focus(egui::Id::new(help_scroll::SEARCH_ID)));
        if self.help_open {
            let closed = context.input_mut(|input| {
                self.help_scroll
                    .route_events(&mut input.events, help_search)
            });
            self.help_open = !closed;
            self.ime_composing = false;
            self.bindings.clear();
            if self.help_open {
                return None;
            }
            // Escape may precede a command in the same native input batch.
            // Only the suffix survives; help's consumed keys and IME events
            // never reach the editor or its composition gate.
            events = context.input(|input| input.events.clone());
        }
        let mut ime_event = events.iter().any(|e| matches!(e, egui::Event::Ime(_)));
        if help_scroll::observe_composition(&events, &mut self.ime_composing) {
            self.bindings.clear();
        }
        // Widgets resolve pointer-driven focus later in this frame. Do not
        // dispatch a destructive shortcut or submit the formerly focused
        // command against that old focus. Text events still reach the widgets.
        if pointer_focus_transition(&events) {
            self.bindings.clear();
            return None;
        }
        let mut text_result = None;
        let mut events = events.into_iter();
        loop {
            let event =
                match self
                    .help_scroll
                    .next_event(&mut self.help_open, &mut events, help_search)
                {
                    help_scroll::RoutedInput::Editor(event) => event,
                    help_scroll::RoutedInput::Done => break,
                    help_scroll::RoutedInput::Help(remaining) => {
                        // Help may have opened earlier in this very batch. Give it the
                        // ordered suffix immediately, including a possible next Escape.
                        self.bindings.clear();
                        self.ime_composing = false;
                        context.input_mut(|input| input.events.clone_from(&remaining));
                        if self.help_open {
                            return text_result;
                        }
                        ime_event = remaining
                            .iter()
                            .any(|event| matches!(event, egui::Event::Ime(_)));
                        help_scroll::observe_composition(&remaining, &mut self.ime_composing);
                        continue;
                    }
                };
            if matches!(event, egui::Event::WindowFocused(false)) {
                self.cancel_register_choice();
                self.bindings.clear();
                self.trim_prefix_target = None;
                self.macro_prefix_target = None;
                self.marks.prefix = None;
                continue;
            }
            if let egui::Event::Key {
                key,
                physical_key,
                modifiers,
                pressed,
                repeat,
            } = event
            {
                self.bindings.set_routing_domain(self.routed_domain());
                // Release events revoke the held semantic binding even when a
                // native control now owns focus. They never dispatch an action.
                if !pressed {
                    self.bindings.route_event(
                        key,
                        physical_key,
                        modifiers,
                        false,
                        false,
                        repeat,
                        false,
                        self.routed_edit_selection(),
                    );
                    continue;
                }
                let focused = text_input_active(context, self.command_open);
                let ime = self.ime_composing || ime_event;
                // egui has no Key::At, and egui-winit reports a physical
                // fallback for other characters it cannot name (AZERTY `&`,
                // QWERTZ `"`). Only a printable key's immediate native text
                // companion proves the logical character on the current layout.
                let logical_text = match events.as_slice().first() {
                    Some(egui::Event::Text(text)) if editor_input::printable(key) => {
                        Some(text.clone())
                    }
                    _ => None,
                };
                let logical_text = logical_text.as_deref();
                if self.camera.is_some() && !self.sound_focused() {
                    // The typed character decides Camera digits and letters:
                    // AZERTY Shift+3 is 3, unshifted `&` names no Camera key.
                    let camera_press = navigation::mode_key(key, modifiers, logical_text);
                    let (camera_key, camera_modifiers) = camera_press.unwrap_or((key, modifiers));
                    match camera::dispatch_key(
                        camera_key,
                        camera_modifiers,
                        focused,
                        ime,
                        repeat,
                        self.camera_field_focused(context),
                        control_owns_activation(context, key),
                    ) {
                        camera::KeyDispatch::Native => {
                            self.camera_input(navigation::camera::CameraKey::ClearCount, context);
                            continue;
                        }
                        camera::KeyDispatch::Deferred(camera_key) => {
                            self.defer_camera_field_key(camera_key);
                            context.input_mut(|input| {
                                input.consume_key(modifiers, key);
                            });
                            continue;
                        }
                        camera::KeyDispatch::Draft(camera_key) => {
                            let camera_key = if camera_press.is_some() {
                                camera_key
                            } else {
                                navigation::camera::CameraKey::Other
                            };
                            if matches!(camera_key, navigation::camera::CameraKey::Tab { .. }) {
                                camera::retain_field_input_suffix(context, events.as_slice());
                            }
                            self.camera_input(camera_key, context);
                            context.input_mut(|input| {
                                input.consume_key(modifiers, key);
                            });
                            continue;
                        }
                        camera::KeyDispatch::Global => {
                            // Recognized global actions cancel the draft through
                            // the ordinary action path. Native text shortcuts
                            // continue to use their original focus context.
                        }
                    }
                }
                if self
                    .monitor_control
                    .is_some_and(|id| context.memory(|m| m.has_focus(id)))
                    && !focused
                    && !ime
                    && matches!(key, egui::Key::ArrowLeft | egui::Key::ArrowRight)
                {
                    self.bindings.clear();
                    continue; // The focused volume slider owns its arrows.
                }
                // Find words behaves as a find bar: Enter and Shift+Enter step
                // through matches and keep the field focused.
                if key == egui::Key::Enter
                    && !ime
                    && (modifiers == egui::Modifiers::NONE || modifiers == egui::Modifiers::SHIFT)
                    && context.memory(|m| m.has_focus(egui::Id::new(TRANSCRIPT_SEARCH_ID)))
                {
                    self.transcription.request_step(!modifiers.shift);
                    context.input_mut(|input| {
                        input.consume_key(modifiers, key);
                    });
                    continue;
                }
                if let Some(text_action) = navigation::text_action(key, modifiers, focused, ime) {
                    text_result = Some((text_action, self.command_open));
                    self.deferred_text_input = context.input_mut(|input| {
                        editor_input::take_field_tail(
                            &mut input.events,
                            &egui::Event::Key {
                                key,
                                physical_key,
                                modifiers,
                                pressed,
                                repeat,
                            },
                        )
                    });
                    if !self.deferred_text_input.is_empty() {
                        context.request_repaint();
                    }
                    break;
                }
                if control_owns_activation(context, key) {
                    self.bindings.clear();
                    continue; // Preserve egui/AccessKit activation of a focused control.
                }
                if native_control_focused(context)
                    && self
                        .bindings
                        .native_control_owns_cut_event_with_logical_text(
                            key,
                            physical_key,
                            modifiers,
                            self.routed_edit_selection(),
                            logical_text,
                        )
                {
                    self.bindings.clear();
                    continue;
                }
                if !repeat
                    && navigation::inspector_parameter_key(
                        key,
                        modifiers,
                        self.pane,
                        text_input_active(context, self.command_open),
                        ime,
                    )
                    && self.open_inspector_parameter(context)
                {
                    context.input_mut(|input| {
                        input.consume_key(modifiers, key);
                    });
                    continue;
                }
                let before = self.bindings.pending();
                let trim_pending = self.bindings.trim_pending();
                let ai_pending = self.bindings.ai_pending();
                let macro_pending = self.bindings.macro_pending();
                let operator_pending = self.bindings.operator_pending();
                let repeat_pending = self.bindings.repeat_pending();
                let mark_prefix = self.bindings.mark_prefix();
                // Command mode remains text-only until the end-of-frame blur
                // handling closes it, even if a click has already moved focus.
                let selection = self.routed_edit_selection();
                let action = self.bindings.route_event_with_logical_text(
                    key,
                    physical_key,
                    modifiers,
                    text_input_active(context, self.command_open),
                    ime,
                    repeat,
                    true,
                    selection,
                    logical_text,
                );
                if action.is_none()
                    && !ime
                    && !text_input_active(context, self.command_open)
                    && let Some(notice) = self.bindings.reserved_layout_notice(
                        key,
                        physical_key,
                        modifiers,
                        logical_text,
                    )
                {
                    // Kestrel keeps this chord; say so instead of a silent no-op.
                    self.message = Some(notice);
                }
                if !operator_pending
                    && (self.bindings.operator_pending()
                        || matches!(action, Some(Action::Operator { .. })))
                {
                    self.begin_operator();
                }
                if !repeat_pending
                    && (self.bindings.repeat_pending()
                        || matches!(action, Some(Action::Repeat { .. })))
                {
                    self.begin_repeat();
                }
                if operator_pending
                    && !self.bindings.operator_pending()
                    && matches!(action, Some(Action::Invalid(_)))
                {
                    self.copied.begin_write();
                }
                if !macro_pending
                    && (self.bindings.macro_pending()
                        || matches!(
                            action,
                            Some(Action::MacroRecord(_) | Action::MacroExecute { .. })
                        ))
                {
                    self.macro_prefix_target = Some(self.capture_macro_target());
                }
                if !trim_pending && (self.bindings.trim_pending() || action == Some(Action::Trim)) {
                    // Capture at the first Trim ancestor, or immediately before
                    // a direct binding. An absent target stays absent throughout
                    // the pending path, even if a service reply arrives later.
                    self.trim_prefix_target = Some(self.capture_trim_target());
                }
                if !ai_pending
                    && (self.bindings.ai_pending() || matches!(action, Some(Action::Ai(_))))
                {
                    // Capture at the first `,a`/`,x`/`,n` ancestor, including
                    // absence.
                    self.ai.prefix = Some(self.ai_capture());
                }
                if let Some(action) = action {
                    if matches!(action, Action::SetMark(_) | Action::JumpMark(_))
                        && self.marks.prefix.is_none()
                    {
                        self.marks.prefix = Some(Err(
                            "The captured mark target expired. Enter the mark binding again."
                                .into(),
                        ));
                    }
                    // Catalog navigation calls the same focus helpers as a
                    // pointer selection. Preserve only this completed motion's
                    // latch; actual pointer, text and context changes still clear it.
                    let motion = matches!(action, Action::Step { .. } | Action::Beat { .. })
                        .then(|| self.bindings.clone());
                    self.action(action, context);
                    if let Some(bindings) = motion {
                        self.bindings = bindings;
                        self.bindings.set_macro_recording(self.macros.recording());
                    }
                    let entered_text = (matches!(action, Action::Command | Action::Group)
                        && self.command_open)
                        || (action == Action::Search
                            && context.memory(|m| {
                                m.has_focus(egui::Id::new(SEARCH_ID))
                                    || m.has_focus(egui::Id::new(TRANSCRIPT_SEARCH_ID))
                            }));
                    if entered_text {
                        // Discard this path's earlier keys and companion text,
                        // but preserve every later native input event in order.
                        // Bulk consume_key here could erase a later same-key
                        // press that now belongs to the newly focused field.
                        let suffix = self.text_entry_gate.begin(
                            key,
                            physical_key,
                            modifiers,
                            events.as_slice(),
                        );
                        context.input_mut(|input| input.events.clone_from(&suffix));
                        events = suffix.into_iter();
                    } else {
                        context.input_mut(|i| {
                            i.consume_key(modifiers, key);
                        });
                    }
                    if self.trim.is_some() {
                        // Route only the suffix after Trim entry through the new mode.
                        // Earlier keys must not be replayed as Trim adjustments.
                        context.input_mut(|input| input.events = events.as_slice().to_vec());
                        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
                        self.trim_keyboard(context);
                        return text_result;
                    }
                    if action == Action::Render {
                        break;
                    }
                } else if before != self.bindings.pending() {
                    context.input_mut(|i| {
                        i.consume_key(modifiers, key);
                    });
                }
                if !self.bindings.trim_pending() {
                    self.trim_prefix_target = None;
                }
                if !self.bindings.ai_pending() {
                    self.ai.prefix = None;
                }
                if !self.bindings.macro_pending() {
                    self.macro_prefix_target = None;
                }
                if !self.bindings.operator_pending() {
                    self.operator_target = None;
                }
                if !self.bindings.repeat_pending() {
                    self.repeat_prefix_target = None;
                }
                if mark_prefix.is_none() && self.bindings.mark_prefix().is_some() {
                    self.begin_mark_prefix();
                } else if self.bindings.mark_prefix().is_none() {
                    self.marks.prefix = None;
                }
            }
        }
        text_result
    }

    fn run_command(&mut self, context: &egui::Context) {
        let command = navigation::command::parse(&self.command);
        let scoped_target = self.scoped_command_target.take();
        let hold_target = self.hold_command_target.take();
        let sound_target = self.sound_command_target.take();
        let gain_target = self.gain_command_target.take();
        let slip_target = self.slip_command_target.take();
        let trim_target = self.trim_command_target.take();
        let ai_target = self.ai.command.take();
        let track_target = self.targets.command.take();
        let zoom_target = self.zoom_command_target.take();
        self.trim_prefix_target = None;
        let placement_target = self.placement_command_target.take();
        let copy_register = self.copy_command_register.take();
        let register_cancelled = std::mem::take(&mut self.register_command_cancelled)
            || copy_register.is_some_and(|(session, _)| {
                self.workspace
                    .as_ref()
                    .is_none_or(|workspace| workspace.session != session)
            });
        let delete_target = self.delete_command_target.take();
        let frame_delete_target = self.frame_delete_command_target.take();
        let macro_target = self.macro_command_target.take();
        let mark_target = self.marks.command.take();
        self.bindings.clear();
        self.command_open = false;
        self.command_focus_pending = false;
        if self.scoped_command_blocked(&command) {
            return;
        }
        if !self.macro_command_allowed(&command) {
            return;
        }
        if register_cancelled
            && matches!(
                command,
                Ok(navigation::command::Entry::Splice
                    | navigation::command::Entry::Action(
                        Action::CopyMoment
                            | Action::DeleteFrames(_)
                            | Action::Edit(BeatEdit::Delete)
                            | Action::PasteMoment { .. }
                    ))
            )
        {
            if matches!(
                command,
                Ok(navigation::command::Entry::Action(
                    Action::CopyMoment | Action::DeleteFrames(_) | Action::Edit(BeatEdit::Delete)
                ))
            ) {
                self.copied.supersede();
            }
            self.error = Some(
                "Register choice was cancelled. Start the command again; no edit was made.".into(),
            );
            return;
        }
        if matches!(
            command,
            Ok(navigation::command::Entry::Action(
                Action::Edit(BeatEdit::Delete) | Action::DeleteFrames(_)
            ))
        ) {
            self.copied.supersede();
            self.copied.clear_selection();
        }
        if matches!(
            command,
            Ok(navigation::command::Entry::Action(Action::Edit(
                BeatEdit::Delete
            )))
        ) && (sound_target.is_some() || self.pane == Pane::Sounds || self.event_focused())
        {
            self.error = Some("Use :sound-delete to remove a placed sound. :delete cuts the selected picture beat when Beats is focused; no edit was made.".into());
            return;
        }
        if let Ok(navigation::command::Entry::Action(Action::Sound(action))) = &command
            && !matches!(
                action,
                navigation::SoundAction::Place | navigation::SoundAction::Focus
            )
        {
            match sound_target {
                Some(target) => {
                    if let Err(error) = self.check_sound_command(&target) {
                        self.error = Some(error);
                        return;
                    }
                    if let navigation::SoundAction::Allowance(allowed) = action {
                        self.captured_sound_allowance(target, *allowed);
                        return;
                    }
                    // The displayed first audible sample may enclose an exact
                    // fractional-frame onset. Merely accepting the prefilled
                    // field must not snap that retained phase or add history.
                    if matches!(action, navigation::SoundAction::Move(_))
                        && self.unchanged_sound_position(&target)
                    {
                        self.message = Some("Sound position unchanged.".into());
                        return;
                    }
                }
                None => {
                    self.error = Some("Select a placed sound before opening its command. Use :sounds, then enter the command again; no edit was made.".into());
                    return;
                }
            }
        }
        match command {
            Ok(navigation::command::Entry::Group { label }) => {
                self.group_command(macro_target, Some(label));
            }
            Ok(navigation::command::Entry::Action(Action::Ungroup)) => {
                self.group_command(macro_target, None);
            }
            Ok(navigation::command::Entry::Action(Action::Explode)) => {
                self.structure_command(macro_target, true);
            }
            Ok(navigation::command::Entry::Action(Action::Duplicate)) => {
                self.structure_command(macro_target, false);
            }
            Ok(navigation::command::Entry::Scope(choice)) => {
                self.scoped_command(choice, scoped_target, context);
            }
            Ok(navigation::command::Entry::Action(Action::Edit(BeatEdit::WrapRepeat(plays)))) => {
                self.repeat_command(macro_target, plays, false);
            }
            Ok(navigation::command::Entry::Action(Action::Edit(BeatEdit::Repeat(plays)))) => {
                self.repeat_command(macro_target, plays, true);
            }
            Ok(navigation::command::Entry::Action(
                action @ (Action::MacroRecord(_)
                | Action::MacroExecute { .. }
                | Action::MacroStop
                | Action::MacroCancel),
            )) => self.macro_action(action, macro_target),
            Ok(navigation::command::Entry::Action(Action::CopyMoment)) => {
                if self.record_macro_yank(
                    copy_register.map(|(_, name)| name),
                    Some(macro_target.unwrap_or_else(|| {
                        Err("Open the copy command again to capture its selected beat.".into())
                    })),
                ) {
                    return;
                }
                self.copied
                    .select(copy_register.map_or('"', |(_, name)| name))
                    .expect("captured register is validated");
                self.action(Action::CopyMoment, context);
            }
            Ok(navigation::command::Entry::Action(
                action @ (Action::SetMark(_) | Action::JumpMark(_) | Action::DeleteMark(_)),
            )) => {
                self.mark_action(
                    action,
                    Some(mark_target.unwrap_or_else(|| {
                        Err("Open the mark command again to capture its context.".into())
                    })),
                );
            }
            Ok(navigation::command::Entry::Action(Action::Edit(BeatEdit::Delete))) => {
                if self.record_macro_delete(
                    copy_register.map(|(_, name)| name),
                    Some(macro_target.unwrap_or_else(|| {
                        Err("Open :delete again to capture its selection.".into())
                    })),
                ) {
                    return;
                }
                self.delete_captured(delete_target.unwrap_or_else(|| {
                    Err("Open :delete again to capture its target; no edit was made.".into())
                }));
            }
            Ok(navigation::command::Entry::Action(Action::DeleteFrames(count))) => {
                self.delete_frames_captured(
                    frame_delete_target.unwrap_or_else(|| {
                        Err(
                            "Open :delete-frames again to capture its cursor; no edit was made."
                                .into(),
                        )
                    }),
                    count,
                );
            }
            Ok(navigation::command::Entry::Gain(Some(gain)))
                if self.macros.recording()
                    || (gain::whole_beat(&gain_target) && self.selected_edit_range().is_none()) =>
            {
                self.record_audio_change(
                    gain_target,
                    macro_target,
                    deadpan_core::AudioChange::Trim { gain },
                )
            }
            Ok(navigation::command::Entry::Gain(value)) => {
                self.gain_command(gain_target, value, context)
            }
            Ok(navigation::command::Entry::GainMute) => self.gain_mute(gain_target, macro_target),
            Ok(navigation::command::Entry::GainStep(millidecibels))
                if self.macros.recording()
                    || (gain::whole_beat(&gain_target) && self.selected_edit_range().is_none()) =>
            {
                self.record_audio_change(
                    gain_target,
                    macro_target,
                    deadpan_core::AudioChange::Step { millidecibels },
                )
            }
            Ok(navigation::command::Entry::GainStep(delta)) => {
                self.gain_step_captured(gain_target, delta)
            }
            Ok(navigation::command::Entry::GainRange {
                millidecibels,
                range,
            }) => self.gain_range_captured(gain_target, millidecibels, range),
            Ok(navigation::command::Entry::Saturate(stage)) => {
                self.saturate_command(gain_target, macro_target, stage)
            }
            Ok(navigation::command::Entry::Slip(amount)) => {
                self.open_slip(slip_target, amount, context)
            }
            Ok(navigation::command::Entry::Trim(preset)) => {
                self.open_trim(trim_target, preset, context)
            }
            Ok(navigation::command::Entry::RoomTone) => self.open_room_tone(hold_target, context),
            Ok(navigation::command::Entry::Correct) => self.open_corrections(context),
            Ok(navigation::command::Entry::HoldSilence) => self.silence_hold(hold_target),
            Ok(navigation::command::Entry::Action(Action::PasteMoment { before })) => {
                self.paste_captured_moment(
                    before,
                    placement_target.unwrap_or_else(|| {
                        Err("Open the paste command again to capture its destination.".into())
                    }),
                );
            }
            Ok(navigation::command::Entry::Action(Action::Ai(action))) => {
                self.ai_action(action, ai_target);
            }
            Ok(navigation::command::Entry::Track {
                target,
                through_shots,
            }) => self.track_command(track_target, target, through_shots),
            Ok(navigation::command::Entry::TrackCancel) => self.track_cancel(),
            Ok(navigation::command::Entry::Zoom(input)) => {
                self.zoom_command(zoom_target, input, context)
            }
            Ok(navigation::command::Entry::Caption(input)) => self.caption_command(input),
            Ok(navigation::command::Entry::GagInspect(input)) => self.inspect_gag(input),
            Ok(navigation::command::Entry::Edge { side, policy }) => {
                self.cancel_repeats("a sound edge change was requested");
                let target = macro_target.unwrap_or_else(|| {
                    Err("Open :edge again to capture the selected beat.".into())
                });
                self.apply_recorded_instruction(
                    target,
                    Ok(deadpan_core::SemanticInstruction::SetAudioEdges { side, policy }),
                );
            }
            Ok(navigation::command::Entry::GagPreset { name, parameters }) => {
                self.apply_gag_preset(macro_target, &name, &parameters)
            }
            Ok(navigation::command::Entry::GagSave(name)) => {
                self.save_gag_preset(macro_target, &name)
            }
            Ok(navigation::command::Entry::GagPresets) => self.list_gag_presets(),
            Ok(navigation::command::Entry::Sting) => self.import_sting(),
            Ok(navigation::command::Entry::GagSet(parameters)) => {
                self.set_gag(macro_target, &parameters)
            }
            Ok(navigation::command::Entry::SelectRole(role)) => self.select_role(role),
            Ok(navigation::command::Entry::DeleteRole(role)) => self.delete_role(
                role,
                macro_target.unwrap_or_else(|| {
                    Err("Open :delete again to capture its range; no edit was made.".into())
                }),
            ),
            Ok(navigation::command::Entry::RecipeSave(name)) => {
                self.save_recipe(name, macro_target)
            }
            Ok(navigation::command::Entry::Recipe(name)) => self.insert_recipe(name, macro_target),
            Ok(navigation::command::Entry::RecipeInspect(name)) => self.inspect_recipe(name),
            Ok(navigation::command::Entry::Action(action)) => self.action(action, context),
            Ok(navigation::command::Entry::Source) => self.show_original(context),
            Ok(navigation::command::Entry::Sequence) => self.show_edit(context),
            Ok(navigation::command::Entry::Help) => {
                if self
                    .command
                    .trim()
                    .trim_start_matches(':')
                    .eq_ignore_ascii_case("registers")
                {
                    self.help_scroll = Default::default();
                    self.help_registers_first = true;
                } else if self.help_registers_first {
                    self.help_scroll = Default::default();
                    self.help_registers_first = false;
                }
                self.help_open = true;
            }
            Ok(navigation::command::Entry::Renders) => self.render.history.requested = true,
            Ok(navigation::command::Entry::Models) => self.open_models(None, context),
            Ok(navigation::command::Entry::Diagnostics) => self.open_diagnostics(context),
            Ok(navigation::command::Entry::Storage) => self.open_storage(context),
            Ok(navigation::command::Entry::Backups) => self.open_backups(context),
            Ok(navigation::command::Entry::Jobs) => self.open_jobs(context),
            Ok(navigation::command::Entry::PortableCopy) => self.start_portable_copy(context),
            Ok(navigation::command::Entry::Relink) => self.locate_original(context),
            Ok(navigation::command::Entry::Recovery) => self.show_recovery_report(),
            Ok(navigation::command::Entry::Close) => self.close_command(),
            Ok(navigation::command::Entry::SoundChannels(choice)) => {
                self.sound_interpretation = choice;
                self.message = Some(match choice {
                    Some(choice) => format!(
                        "Sounds without a speaker layout will be heard as {}. Add one with ⌘I.",
                        choice.label()
                    ),
                    None => "Sounds without a speaker layout will be refused.".into(),
                });
            }
            Ok(navigation::command::Entry::Splice) => self.open_captured_splice(
                context,
                placement_target.unwrap_or_else(|| {
                    Err("Open Place slice again to capture its destination.".into())
                }),
            ),
            Ok(navigation::command::Entry::Proxies(command)) => self.proxy_command(command),
            Ok(navigation::command::Entry::Monitor(tenths)) => {
                self.pause_playback();
                self.monitor_gain = f32::from(tenths) / 1000.0;
                self.message = Some(format!(
                    "Monitor {}.{}% · project and export gain unchanged",
                    tenths / 10,
                    tenths % 10
                ));
            }
            Ok(navigation::command::Entry::AuditionContext { lead, follow }) => {
                let rate = self
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.document.presentation_basis().frame_rate);
                match rate
                    .ok_or_else(|| "Open a project before changing audition context.".to_owned())
                    .and_then(|rate| {
                        Ok(playback::AuditionContext {
                            lead: lead.samples(rate)?,
                            follow: follow.samples(rate)?,
                        })
                    }) {
                    Ok(value) => {
                        self.stop_playback();
                        self.audition_context = value;
                        self.message = Some(format!(
                            "Loop context: {} lead-in · {} follow-through",
                            value.lead_label(),
                            value.follow_label()
                        ));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Ok(navigation::command::Entry::Empty) => {}
            Err(error) => self.error = Some(error),
        }
    }

    /// Follow the macOS Reduce motion and Increase contrast settings. Only the
    /// native app calls this; replays keep the fixed workspace style.
    pub fn follow_system_display(&mut self) {
        self.display.follow_system();
    }

    /// Replace the application menu bar. Native launches only; replay and
    /// tests keep the in-window File menu.
    #[cfg(target_os = "macos")]
    pub fn install_menu(&mut self, context: &egui::Context) -> Result<(), String> {
        let repaint = context.clone();
        self.menu = Some(crate::menu::MenuBar::install(move || {
            repaint.request_repaint();
        })?);
        Ok(())
    }

    /// Dispatch chosen menu commands that are currently allowed, then refresh
    /// menu enablement. Each command rechecks the state left by the previous
    /// one, so a queued item can never act after its precondition ended.
    #[cfg(target_os = "macos")]
    fn menu_commands(&mut self, context: &egui::Context) {
        use crate::menu::MenuCommand;
        let Some(commands) = self.menu.as_ref().map(crate::menu::MenuBar::take_commands) else {
            return;
        };
        for command in commands {
            if !self.menu_state(context).enabled(command) {
                continue;
            }
            match command {
                MenuCommand::New => self.action(Action::New, context),
                MenuCommand::NewFromUrl => self.action(Action::NewFromUrl, context),
                MenuCommand::Open => self.action(Action::Open, context),
                MenuCommand::Import => self.action(Action::Import, context),
                MenuCommand::Close => {
                    self.submit(ProjectRequest::Close);
                }
                MenuCommand::Render => self.action(Action::Render, context),
                MenuCommand::Renders => self.render.history.requested = true,
                MenuCommand::Undo => self.history(false),
                MenuCommand::Redo => self.history(true),
                MenuCommand::ViewOriginal => self.show_original(context),
                MenuCommand::ViewEdit => self.show_edit(context),
                MenuCommand::Keys => self.action(Action::Help, context),
                MenuCommand::Models => self.open_models(None, context),
                MenuCommand::Storage => self.open_storage(context),
                MenuCommand::Jobs => self.open_jobs(context),
                MenuCommand::PortableCopy => self.start_portable_copy(context),
                MenuCommand::Quit => context.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
        let state = self.menu_state(context);
        if let Some(menu) = &mut self.menu {
            menu.update(state);
        }
    }

    #[cfg(target_os = "macos")]
    fn menu_state(&self, context: &egui::Context) -> crate::menu::MenuState {
        let profile = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.single_source.as_ref());
        crate::menu::MenuState {
            ready: !self.service.is_busy()
                && !self.dialogs.is_open()
                && self.gain.is_none()
                && self.camera.is_none()
                && self.splice.is_none()
                && self.slip.is_none()
                && self.trim.is_none()
                && !self.render.blocking()
                && !self.marks.open
                && !self.models.open
                && !self.diagnostics.open
                && !self.storage.open
                && !self.jobs.open
                && !self.help_open
                && !self.macros.recording()
                && !self.macros.is_pending()
                && self.bindings.pending().is_empty()
                // The YouTube URL field leaves ⌘N/⌘O/⌘I/⌘⇧N available.
                && !self.command_open
                && !context.memory(|m| {
                    TEXT_INPUT_IDS
                        .iter()
                        .filter(|id| **id != youtube::URL_ID)
                        .any(|id| m.has_focus(egui::Id::new(id)))
                })
                && !self.youtube_blocks_menu(),
            project: self.workspace.is_some(),
            importing: self.importing(),
            can_undo: self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.can_undo),
            can_redo: self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.can_redo),
            single_original_ready: matches!(profile, Some(SingleSourceState::Ready { .. })),
            awaiting_original: matches!(profile, Some(SingleSourceState::AwaitingSource { .. })),
            help_allowed: self.gain.is_none() && !self.render.blocking(),
        }
    }

    /// Browse the unchanged Original, as `:source` and the View menu do.
    fn show_original(&mut self, context: &egui::Context) {
        self.scoped = None;
        self.stop_playback();
        let leaving_event = self.pane == Pane::Sounds || self.event_focused();
        self.selected_sound = None;
        self.selected_event = None;
        self.sound_inspection = None;
        if leaving_event {
            self.pane = Pane::Sources;
            context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
        }
        if self.view != View::Source {
            self.view.set(View::Source, &mut self.message);
            self.request_picture(true);
        }
    }

    /// Return to Your edit, as `:sequence` and the View menu do.
    fn show_edit(&mut self, context: &egui::Context) {
        self.stop_playback();
        let leaving_event = self.pane == Pane::Sounds || self.event_focused();
        self.selected_sound = None;
        self.selected_event = None;
        self.sound_inspection = None;
        if leaving_event {
            self.pane = Pane::Sequence;
            context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
        }
        if self.workspace.is_none() {
            self.error = Some("Create or open a project to inspect its sequence.".into());
        } else if self.view != View::Sequence {
            self.view.set(View::Sequence, &mut self.message);
            self.reconcile_beat_selection();
            self.request_picture(true);
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, enabled: bool) {
        egui::Panel::top("workspace-header")
            .resizable(false)
            .frame(style::panel())
            .show(ui, |ui| {
                if !enabled {
                    ui.disable();
                }
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
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(
                            style::semibold("DEADPAN")
                                .size(13.0)
                                .extra_letter_spacing(2.4),
                        );
                        ui.add_space(8.0);
                        let ready = !self.service.is_busy()
                            && !self.dialogs.is_open()
                            && self.gain.is_none();
                        #[cfg(target_os = "macos")]
                        let native_menu = self.menu.is_some();
                        #[cfg(not(target_os = "macos"))]
                        let native_menu = false;
                        ui.add_enabled_ui(ready && !native_menu, |ui| {
                            if native_menu {
                                return;
                            }
                            ui.menu_button("File", |ui| {
                                for (label, action) in [
                                    ("New project…  ⌘N", Action::New),
                                    ("New from YouTube URL…  ⌘⇧N", Action::NewFromUrl),
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
                        if ui
                            .add_enabled(
                                self.workspace.is_some() && !self.dialogs.is_open(),
                                style::action("Render", "⌘E").frame_when_inactive(false),
                            )
                            .clicked()
                        {
                            self.action(Action::Render, ui.ctx());
                        }
                        if ui
                            .add_enabled(
                                self.workspace.is_some() && !self.dialogs.is_open(),
                                egui::Button::new("Renders").frame_when_inactive(false),
                            )
                            .on_hover_text("Saved renders and recovery · :renders")
                            .clicked()
                        {
                            self.render.history.requested = true;
                        }
                    });
                    columns[1].with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        accessibility::full_text(ui.add(egui::Label::new(style::semibold(&title)).truncate()), &title)
                            .on_hover_text(&title);
                    });
                    // Start from one natural row, not the cached panel height.
                    // Centered wrapping over inherited height feeds its own
                    // extra space back into the next panel measurement.
                    let controls_size = egui::vec2(
                        columns[2].available_width(),
                        columns[2].spacing().interact_size.y,
                    );
                    let controls = columns[2].allocate_ui_with_layout(
                        controls_size,
                        egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true),
                        |ui| {
                            if ui
                                .add_enabled(
                                    self.gain.is_none(),
                                    style::action("Keys", self.editor_key(EditorKey::Help)),
                                )
                                .on_hover_text(format!(
                                    "Keyboard reference · {} or :help",
                                    self.editor_keys(EditorKey::Help)
                                ))
                                .clicked()
                            {
                                if self.help_open {
                                    self.help_open = false;
                                } else {
                                    self.action(Action::Help, ui.ctx());
                                }
                            }
                            if self.keymap_error
                                && ui
                                    .small_button("Keymap error")
                                    .on_hover_text(self.keymap_status())
                                    .clicked()
                            {
                                self.action(Action::Help, ui.ctx());
                            }
                            let ready = !self.service.is_busy()
                                && !self.dialogs.is_open()
                                && self.gain.is_none();
                            if ui
                                .add_enabled(
                                    ready && self.workspace.as_ref().is_some_and(|w| w.can_redo),
                                    style::action("Redo", "Ctrl R"),
                                )
                                .on_hover_text("Redo · ⌘Shift Z or Ctrl R")
                                .clicked()
                            {
                                self.history(true);
                            }
                            if ui
                                .add_enabled(
                                    ready && self.workspace.as_ref().is_some_and(|w| w.can_undo),
                                    style::action("Undo", self.editor_key(EditorKey::Undo)),
                                )
                                .on_hover_text(format!(
                                    "Undo · ⌘Z or {}",
                                    self.editor_keys(EditorKey::Undo)
                                ))
                                .clicked()
                            {
                                self.history(false);
                            }
                            ui.add_space(6.0);
                            if self.service.is_busy() || self.repeat_queue.active() {
                                ui.weak("Working");
                                crate::preview::accessibility::busy(ui);
                            } else if self.camera.is_some()
                                || self.slip.is_some()
                                || self.trim.is_some()
                            {
                                ui.colored_label(style::LAVENDER, "Draft preview");
                            } else if let Some(reason) = self
                                .workspace
                                .as_ref()
                                .and_then(|workspace| workspace.read_only.clone())
                            {
                                // A newer package viewed read-only: never "Saved".
                                let response = ui
                                    .push_id("header-save-state", |ui| {
                                        ui.colored_label(style::WARNING, "Read-only")
                                    })
                                    .inner
                                    .on_hover_text(reason.to_string());
                                accessibility::full_text(response, &format!("Read-only: {reason}"));
                            } else if self.recovery.storage.is_some() {
                                // Never "Saved" after a refused transaction.
                                // A fixed ID, so a busy spinner before it cannot
                                // turn it into a new, re-announced node.
                                let response = ui.push_id("header-save-state", |ui| ui.colored_label(style::ERROR, "Not saved"))
                                    .inner
                                    .on_hover_text("The last action was refused by storage; the last saved edit is intact");
                                accessibility::live(&response, true);
                            } else if self.workspace.is_some() {
                                ui.colored_label(style::SAVED, "Saved")
                                    .on_hover_text("Current committed revision is saved locally");
                            }
                        },
                    );
                    if controls.response.rect.bottom() > columns[2].clip_rect().bottom() + 0.5 {
                        // A resize or longer status may add a row beyond the
                        // panel's cached clip. Re-measure before first paint.
                        columns[2]
                            .ctx()
                            .request_discard("workspace header controls changed height");
                    }
                });
            });
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        self.notice(ui);
        let available = ui.available_rect_before_wrap();
        let available_bottom = available.bottom();
        // Command entry adds a field and help beneath the status rows. Reserve
        // that space on entry rather than inheriting Normal mode's short panel.
        // The completion line is one truncated row; measure it before the
        // first paint so the reserve already includes it.
        let completion = self
            .command_open
            .then(|| command_completion_text(&self.command))
            .flatten();
        let completion_height = completion.as_ref().map_or(0.0, |_| {
            ui.fonts_mut(|fonts| fonts.row_height(&egui::FontId::monospace(11.0))) + 4.0
        });
        let minimum = if self.command_open {
            144.0 + completion_height
        } else {
            0.0
        };
        let panel = egui::Panel::bottom("workspace-status").resizable(false).min_size(minimum).frame(style::compact_panel()).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            if self.camera.is_some() && !self.sound_focused() {
                self.camera_footer(ui);
                return;
            }
            if !self.command_open && self.youtube_footer_shown(ui.ctx()) {
                self.youtube_footer(ui);
                return;
            }
            if self.gain.is_some() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(style::LAVENDER, "GAIN DRAFT · UNSAVED");
                    ui.weak(self.beat_scope_label());
                    style::key_hint(ui, "Tab", "reveal controls");
                    style::key_hint(ui, registry::mode_label("gain.draft.audition"), "audition on heading");
                    style::key_hint(ui, registry::mode_label("gain.draft.apply"), "apply on heading");
                    style::key_hint(ui, registry::mode_label("gain.draft.cancel"), "cancel");
                });
                return;
            }
            self.ai_footer(ui);
            self.tracking_footer(ui);
            let pending = self.bindings.pending();
            let mode = if self.command_open { "COMMAND" } else if text_input_active(ui.ctx(), false) { "TEXT" } else if (self.moment.active && self.view == View::Source) || (self.edit_range.active && self.view == View::Sequence) { "VISUAL" } else if pending.is_empty() { "NORMAL" } else { "PENDING" };
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(mode).monospace().strong());
                ui.separator();
                ui.label(egui::RichText::new(if self.sound_focused() { "SOUND" } else { match (self.focused_workflow(), self.view) { (true, View::Source) => "ORIGINAL", (true, View::Sequence) => "YOUR EDIT", (false, View::Source) => "SOURCE", (false, View::Sequence) => "SEQUENCE" } }).monospace());
                ui.separator();
                if self.view == View::Sequence && self.edit_role != deadpan_core::MediaRole::Linked {
                    ui.colored_label(style::LAVENDER, egui::RichText::new(match self.edit_role {
                        deadpan_core::MediaRole::Audio => "AUDIO ROLE",
                        _ => "VIDEO ROLE",
                    }).monospace());
                    ui.separator();
                }
                if self.presentation.loading() || self.presentation.needs_render() { crate::preview::accessibility::busy(ui); ui.weak("Updating picture"); }
                if self.sound_focused() {
                    ui.weak("Catalog audition");
                } else if self.event_focused() {
                    if let Some(event) = self.workspace.as_ref().and_then(|w| w.document.sounds().get(self.selected_event.as_ref()?)) {
                        ui.label(format!("Sound: {}", event.label));
                    }
                } else if self.view == View::Sequence {
                    if let Some(label) = self.edit_range_label() {
                        ui.colored_label(style::LAVENDER, label);
                    } else if self.scoped.is_some() {
                        let label = self.beat_scope_label();
                        accessibility::full_text(ui.add(egui::Label::new(&label).truncate()), &label).on_hover_text(label);
                    } else if let Some(beat) = self.beat_rows.iter().find(|beat| Some(&beat.id) == self.selected_beat.as_ref()) {
                        { let text = format!("{} · {}", beat.label, self.beat_scope_label()); accessibility::full_text(ui.add(egui::Label::new(&text).truncate()), &text) }.on_hover_text(format!("{} · {} · {} frames · {}", beat.label, beat.kind, beat.frames, self.beat_scope_label()));
                    } else { ui.weak("No beat selected"); }
                } else { ui.weak("Unchanged source"); }
                ui.colored_label(style::LAVENDER, format!("Focus: {}", if self.pane == Pane::Sources && self.focused_workflow() { "Original / sounds" } else { pane_name(self.pane) }));
                if let Some(label) = self.register_status() {
                    ui.colored_label(style::LAVENDER, label);
                }
                if let Some(label) = self.macros.label() {
                    ui.colored_label(style::CURSOR, label);
                    style::key_hint(ui, &self.editor_key(EditorKey::MacroRecord), "save macro");
                    style::key_hint(ui, "Esc", if self.edit_selection() == navigation::EditSelection::None || self.macros.is_pending() { "cancel recording" } else { "clear selection" });
                }
                if !self.command_open && !self.sound_focused() && self.pane != Pane::Sounds && !self.event_focused()
                    && ui.add(egui::Button::new(format!("Marks  {}", self.editor_pair(EditorKey::MarkSet, EditorKey::MarkJump, " / "))).small().wrap()).on_hover_text(format!("{} + letter saves this position; {} + letter returns. Browse with :marks.", self.editor_key(EditorKey::MarkSet), self.editor_key(EditorKey::MarkJump))).clicked()
                {
                    self.open_marks(ui.ctx());
                }
                if !pending.is_empty() { key_labels::keycap(ui, &pending); }
                if let Some(hint) = self.bindings.pending_hint() {
                    let next = self.bindings.pending_next_keys().unwrap_or_else(|| hint.clone());
                    key_labels::pending_guidance(ui, &hint, &next);
                }
            });
            if self.command_open {
                focus_command_for_frame(ui.ctx(), &mut self.command_focus_pending);
                egui::Frame::new().fill(style::PANEL).stroke(egui::Stroke::new(1.0, style::LAVENDER)).corner_radius(4).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(":").monospace().color(style::LAVENDER));
                        ui.add(command_text_edit(&mut self.command).font(egui::TextStyle::Monospace).frame(egui::Frame::NONE).desired_width(f32::INFINITY).hint_text("hold 0.5s · repeat 3 · retime 0.75 pitch=preserve · help"));
                        retain_text_escape(ui, COMMAND_ID);
                    });
                });
                if let Some((shown, full)) = &completion {
                    // Completion teaching: the first few matching commands; the
                    // accessible name and hover carry every match.
                    let response = ui.add(egui::Label::new(egui::RichText::new(shown).monospace().size(11.0).color(style::LAVENDER)).truncate());
                    accessibility::full_text(response, full).on_hover_text(full);
                }
                ui.horizontal_wrapped(|ui| {
                    style::key_hint(ui, "Enter", "apply command");
                    style::key_hint(ui, "Esc", "cancel entry");
                    let group_entry = self.command.trim_start().trim_start_matches(':').split_whitespace().next().is_some_and(|verb| verb.eq_ignore_ascii_case("group"));
                    ui.weak(if group_entry { "Name the selection · use quotes, for example name=\"the answer\"" } else { sound_events::command_hint(&self.command).unwrap_or("Whole project frames: 11f · repeat count: total plays") });
                    if let (Ok(navigation::command::Entry::Action(Action::Edit(BeatEdit::InsertHold(input)))), Some(workspace)) = (navigation::command::parse(&self.command), &self.workspace)
                        && let Ok(duration) = input.resolve(workspace.document.presentation_basis().frame_rate)
                    {
                        ui.colored_label(style::LAVENDER, format!("{} frames · freeze + silence · at boundary {}", duration.frames(), self.sequence_cursor));
                    }
                });
                if let Some(hint) = self.retime_hint() {
                    match hint {
                        Ok(text) => { ui.colored_label(style::LAVENDER, text); }
                        Err(error) => { ui.colored_label(ui.visuals().error_fg_color, error); }
                    }
                }
                if let Some(hint) = self.delete_hint() { ui.colored_label(style::LAVENDER, hint); }
                if let Some(hint) = self.cutaway_hint() { ui.colored_label(style::LAVENDER, hint); }
                if let Some(hint) = self.frame_delete_hint() { ui.colored_label(style::LAVENDER, hint); }
            } else {
                ui.horizontal_wrapped(|ui| {
                    let clock = if self.sound_focused() { format!("Sound {}", playback::sound_time(self.sound_cursor)) } else if self.view == View::Source { format!("Original boundary {}/{}", self.source_cursor, self.source_length()) } else { self.scope_clock_label() };
                    ui.label(egui::RichText::new(clock).monospace().color(style::CURSOR));
                    let mut hints = key_labels::Hints::new();
                    if self.macros.recording() {
                        // The way out of recording comes first; the footer keeps
                        // as many of the remaining keys as fit.
                        let visual = self.edit_selection() != navigation::EditSelection::None;
                        hints.push(("Esc".into(), if visual && !self.macros.is_pending() { "clear selection".into() } else { "cancel recording".into() }));
                        self.add_editor_hint(&mut hints, EditorKey::MacroRecord, "save macro");
                        self.add_editor_pair_hint(&mut hints, EditorKey::FramePrevious, EditorKey::FrameNext, " / ", "frame");
                        self.add_editor_pair_hint(&mut hints, EditorKey::BeatNext, EditorKey::BeatPrevious, " / ", "beat");
                        self.add_editor_pair_hint(&mut hints, EditorKey::First, EditorKey::Last, " / ", "group bounds");
                        self.add_editor_hint(&mut hints, EditorKey::Visual, if self.edit_range.active { "finish range" } else { "start range" });
                        self.add_editor_hint(&mut hints, if visual { EditorKey::CutRange } else { EditorKey::CutFrames }, if visual { "cut range" } else { "cut frames" });
                        self.add_editor_hint(&mut hints, EditorKey::Copy, if visual { "copy range" } else { "copy beat" });
                        self.add_editor_hint(&mut hints, EditorKey::Repeat, if visual { "repeat range" } else { "repeat beat" });
                        self.add_editor_hint(&mut hints, EditorKey::Group, "name group");
                        self.add_editor_pair_hint(&mut hints, EditorKey::PasteAfter, EditorKey::PasteBefore, " / ", if visual { "replace range" } else { "paste" });
                        self.add_editor_hint(&mut hints, EditorKey::MacroExecute, "+ letter: record call");
                    } else if self.sound_focused() {
                        self.add_editor_pair_hint(&mut hints, EditorKey::BeatNext, EditorKey::BeatPrevious, " ", "sound");
                        self.add_editor_hint(&mut hints, EditorKey::Playback, "play / pause");
                        self.add_editor_hint(&mut hints, EditorKey::Audition, "loop sound");
                        self.add_editor_hint(&mut hints, EditorKey::PlaceSound, "place at edit cursor");
                    } else if self.pane == Pane::Sounds || self.event_focused() {
                        self.add_editor_pair_hint(&mut hints, EditorKey::BeatNext, EditorKey::BeatPrevious, " ", "sound");
                        self.add_editor_pair_hint(&mut hints, EditorKey::FramePrevious, EditorKey::FrameNext, " ", "nudge frame");
                        hints.push(("Enter".into(), "position".into()));
                        self.add_editor_pair_hint(&mut hints, EditorKey::GainUp, EditorKey::GainDown, " / ", "gain 3 dB");
                        self.add_editor_hint(&mut hints, EditorKey::CutBeat, "remove");
                        self.add_editor_hint(&mut hints, EditorKey::Undo, "undo");
                    } else if self.view == View::Sequence && self.scoped.is_some() {
                        self.add_editor_pair_hint(&mut hints, EditorKey::FramePrevious, EditorKey::FrameNext, " / ", "picture frame");
                        self.add_editor_pair_hint(&mut hints, EditorKey::BeatPrevious, EditorKey::BeatNext, " / ", "child");
                        self.add_editor_hint(&mut hints, EditorKey::EnterGroup, "enter contents");
                        self.add_editor_hint(&mut hints, EditorKey::LeaveGroup, "parent");
                        self.add_editor_pair_hint(&mut hints, EditorKey::PlayNext, EditorKey::PlayPrevious, " ", "Repeat play");
                        self.add_editor_pair_hint(&mut hints, EditorKey::GainUp, EditorKey::GainDown, " / ", "gain 3 dB");
                        self.add_editor_hint(&mut hints, EditorKey::Camera, "camera");
                        hints.push((":scope".into(), "all / play N / plays 2-3".into()));
                        self.add_editor_hint(&mut hints, EditorKey::Undo, "undo");
                    } else if self.view == View::Sequence {
                        // Tiers decide what survives the two rows (see
                        // key_labels::Tier); this order is the painted order.
                        let selection = self.edit_selection();
                        self.add_editor_pair_hint(&mut hints, EditorKey::FramePrevious, EditorKey::FrameNext, " ", "frame");
                        self.add_editor_pair_hint(&mut hints, EditorKey::BeatNext, EditorKey::BeatPrevious, " ", "beat");
                        let words = self.workspace.as_ref().is_some_and(|workspace| workspace.transcript.is_some());
                        let pauses = self.workspace.as_ref().is_some_and(|workspace| workspace.speech_activity.is_some());
                        hints.tier(key_labels::Tier::Context);
                        if words { self.add_editor_pair_hint(&mut hints, EditorKey::WordNext, EditorKey::WordPrevious, " ", "word"); }
                        if pauses { self.add_editor_pair_hint(&mut hints, EditorKey::PauseNext, EditorKey::PausePrevious, " ", "pauses"); }
                        let shots = self.workspace.as_ref().is_some_and(|workspace| workspace.shot_analysis.is_some());
                        if shots { self.add_editor_pair_hint(&mut hints, EditorKey::ShotNext, EditorKey::ShotPrevious, " ", "shots"); }
                        if let Some(hold) = self.ai_hold() {
                            match self.ai_offered_variants(&hold) {
                                0 => self.add_editor_hint(&mut hints, EditorKey::GenerateAi, "AI pictures"),
                                count => {
                                    self.add_editor_hint(&mut hints, EditorKey::CompareAi, "AI before / after");
                                    if count > 1 { self.add_editor_hint(&mut hints, EditorKey::NextAi, "next AI variant"); }
                                }
                            }
                        }
                        hints.tier(key_labels::Tier::Core);
                        self.add_editor_hint(&mut hints, EditorKey::Visual, if selection == navigation::EditSelection::Object { if self.edit_range.active { "retain object" } else { "select time" } } else if self.edit_range.active { "finish range" } else { "select range" });
                        self.add_editor_hint(&mut hints, EditorKey::Repeat, if selection == navigation::EditSelection::None { "repeat beat" } else if selection == navigation::EditSelection::Object { "repeat object" } else { "repeat range" });
                        match selection {
                            navigation::EditSelection::Object => self.add_editor_hint(&mut hints, EditorKey::CutRange, "cut object"),
                            navigation::EditSelection::Range => self.add_editor_hint(&mut hints, EditorKey::CutRange, "cut range"),
                            navigation::EditSelection::Empty => self.add_editor_hint(&mut hints, EditorKey::CutRange, "empty range"),
                            navigation::EditSelection::None => self.add_editor_hint(&mut hints, EditorKey::CutBeat, "cut beat"),
                        };
                        // Without a selection the cursor's frame is a cut target too.
                        if selection == navigation::EditSelection::None && self.pane != Pane::Sources {
                            self.add_editor_hint(&mut hints, EditorKey::CutFrames, "cut frame");
                        }
                        self.add_editor_hint(&mut hints, EditorKey::Undo, "undo");
                        // Parameter entry: :hold, :repeat, :retime and the rest.
                        self.add_editor_hint(&mut hints, EditorKey::Command, "command");
                        if self.pane != Pane::Sources && let Some(label) = self.repeat_hint() { self.add_editor_hint(&mut hints, EditorKey::RepeatLast, &label); }
                        self.add_editor_hint(&mut hints, EditorKey::Copy, if self.copied.is_pending() { "copy pending…" } else if selection == navigation::EditSelection::None { "copy beat" } else if selection == navigation::EditSelection::Object { "copy object" } else { "copy range" });
                        if self.copied.selected_content().is_some() { self.add_editor_pair_hint(&mut hints, EditorKey::PasteAfter, EditorKey::PasteBefore, " / ", if selection == navigation::EditSelection::Object { "replace object" } else if self.selected_edit_range().is_some() { "replace range" } else { "paste after / before" }); }
                        hints.tier(key_labels::Tier::Context);
                        if self.selected_group() { self.add_editor_hint(&mut hints, EditorKey::EnterGroup, "open group"); }
                        if self.selected_repeat() { self.add_editor_pair_hint(&mut hints, EditorKey::PlayNext, EditorKey::PlayPrevious, " ", "open a play"); }
                        if !self.sequence_scope.groups().is_empty() { self.add_editor_hint(&mut hints, EditorKey::LeaveGroup, "parent"); }
                        if selection != navigation::EditSelection::None {
                            if words {
                                self.add_editor_hint(&mut hints, EditorKey::InnerWord, "word");
                                self.add_editor_hint(&mut hints, EditorKey::InnerSentence, "sentence");
                            }
                            if pauses { self.add_editor_hint(&mut hints, EditorKey::InnerPause, "pause"); }
                            if shots { self.add_editor_hint(&mut hints, EditorKey::InnerShot, "shot"); }
                            self.add_editor_hint(&mut hints, EditorKey::InnerGroup, "group contents");
                            self.add_editor_hint(&mut hints, EditorKey::AroundGroup, "whole group");
                        }
                        hints.tier(key_labels::Tier::More);
                        self.add_editor_hint(&mut hints, EditorKey::Split, "split");
                        self.add_editor_hint(&mut hints, EditorKey::Hold, "pause");
                        // Within a tier earlier hints win the row budget.
                        if selection == navigation::EditSelection::None && self.pane != Pane::Sources {
                            // Operators take any motion or object: d3l, y]s.
                            self.add_editor_pair_hint(&mut hints, EditorKey::CutOperator, EditorKey::YankOperator, " / ", "+ motion: cut / copy");
                            self.add_editor_pair_hint(&mut hints, EditorKey::MacroRecord, EditorKey::MacroExecute, " / ", "+ letter: record / run macro");
                        }
                        self.add_editor_hint(&mut hints, EditorKey::Camera, "camera");
                        self.add_editor_pair_hint(&mut hints, EditorKey::PunchIn, EditorKey::Creep, " / ", "punch in / creep");
                        self.add_editor_hint(&mut hints, EditorKey::Trim, "Trim beat");
                        self.add_editor_pair_hint(&mut hints, EditorKey::GainUp, EditorKey::GainDown, " / ", if selection == navigation::EditSelection::None { "gain 3 dB" } else { "range gain 3 dB" });
                        self.add_editor_hint(&mut hints, EditorKey::RegisterSelect, "register");
                        self.add_editor_hint(&mut hints, EditorKey::Group, "name group");
                    } else {
                        self.add_editor_pair_hint(&mut hints, EditorKey::FramePrevious, EditorKey::FrameNext, " ", "frame");
                        hints.tier(key_labels::Tier::Context);
                        if self.workspace.as_ref().is_some_and(|workspace| workspace.transcript.is_some()) { self.add_editor_pair_hint(&mut hints, EditorKey::WordNext, EditorKey::WordPrevious, " ", "word"); }
                        if self.workspace.as_ref().is_some_and(|workspace| workspace.speech_activity.is_some()) { self.add_editor_pair_hint(&mut hints, EditorKey::PauseNext, EditorKey::PausePrevious, " ", "pauses"); }
                        if self.workspace.as_ref().is_some_and(|workspace| workspace.shot_analysis.is_some()) { self.add_editor_pair_hint(&mut hints, EditorKey::ShotNext, EditorKey::ShotPrevious, " ", "shots"); }
                        hints.tier(key_labels::Tier::Core);
                        if self.focused_workflow() && !self.beat_rows.is_empty() { self.add_editor_pair_hint(&mut hints, EditorKey::BeatNext, EditorKey::BeatPrevious, " ", "edit beat"); }
                        self.add_editor_hint(&mut hints, EditorKey::Visual, if self.moment.active { "finish selection" } else { "select moment" });
                        self.add_editor_hint(&mut hints, EditorKey::Copy, "copy moment");
                        self.add_editor_hint(&mut hints, EditorKey::RegisterSelect, "register");
                        hints.push((":sequence".into(), if self.focused_workflow() { "Your edit" } else { "Sequence" }.into()));
                        if self.workspace.is_some() && self.selected_source.is_some() { self.add_editor_hint(&mut hints, EditorKey::Insert, if self.focused_workflow() { "reuse all" } else { "insert source" }); }
                    }
                    hints.tier(key_labels::Tier::Context);
                    self.add_editor_hint(&mut hints, EditorKey::PaneNext, "pane");
                    hints.tier(key_labels::Tier::Core);
                    if !hints.iter().any(|(_, description)| description == "command") {
                        self.add_editor_hint(&mut hints, EditorKey::Command, "command");
                    }
                    self.add_editor_hint(&mut hints, EditorKey::Help, "keys");
                    // At most two hint rows; the picture keeps the remaining
                    // height and Help lists every key.
                    let budget = 2.0 * ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
                    key_labels::footer_hints(ui, &hints, &self.editor_key(EditorKey::Help), budget);
                });
            }
        });
        // Bottom panels initially anchor using their previous height. If this
        // content changed height, resolve the cached measurement in the same
        // frame instead of painting a gap or clipped hints for one frame.
        if panel.response.rect.height() <= available.height()
            && (panel.response.rect.bottom() - available_bottom).abs() > 0.5
        {
            ui.ctx().request_discard("workspace footer height changed");
        }
        #[cfg(feature = "ui-harness")]
        {
            self.feedback.footer_bottom = Some((panel.response.rect.bottom(), available_bottom));
            self.feedback.footer_command_open = self.command_open;
        }
    }

    fn notice(&self, ui: &mut egui::Ui) {
        let repeat_status = self.repeat_queue.status();
        let mut urgent = false;
        let text = if self.command_open || (self.camera.is_some() && !self.sound_focused()) {
            None
        } else if let Some(error) = self
            .error
            .as_deref()
            .or(self.project_error.as_deref())
            .or(self.missing_original_notice())
            .or(self.presentation.error())
        {
            let detail = match repeat_status.as_deref() {
                Some(status) if status.contains(error) => status.to_owned(),
                Some(status) => format!("{}; {status}", error.trim_end_matches('.')),
                None => error.to_owned(),
            };
            urgent = true;
            Some(
                egui::RichText::new(format!("Could not complete action: {detail}"))
                    .color(ui.visuals().error_fg_color),
            )
        } else {
            repeat_status
                .as_ref()
                .or(self.message.as_ref())
                .map(|message| egui::RichText::new(message).weak().size(12.0))
        };
        // A bottom panel normally starts with last frame's height. Measure this
        // notice before anchoring it so a new or newly wrapped error is visible
        // on its first paint, including after a viewport resize.
        let frame = if text.is_some() {
            style::compact_panel()
        } else {
            egui::Frame::NONE
        };
        let margins = frame.total_margin().sum();
        let width = (ui.available_width() - margins.x).max(1.0);
        let galley = text.map(|text| {
            egui::WidgetText::from(text).into_galley(
                ui,
                Some(egui::TextWrapMode::Wrap),
                width,
                egui::TextStyle::Body,
            )
        });
        // Even an empty notice owns one panel in the UI tree. Omitting it
        // would change subsequent automatic widget IDs and swallow the first
        // pointer press after command entry or a notice clears.
        egui::Panel::bottom("workspace-notice")
            .resizable(false)
            .show_separator_line(false)
            .exact_size(
                galley
                    .as_ref()
                    .map_or(0.0, |galley| galley.size().y + margins.y),
            )
            .frame(frame)
            .show(ui, |ui| {
                if let Some(galley) = galley {
                    // Saved, refused and finished-work notices are spoken
                    // as they appear; refusals interrupt.
                    // A fixed ID: the notice stays one live region, so only
                    // a changed message is announced.
                    let response = ui
                        .push_id("workspace-notice-text", |ui| {
                            ui.add(egui::Label::new(galley))
                        })
                        .inner;
                    accessibility::live(&response, urgent);
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
                self.sound_preview_controls(ui);
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
                            let detail = self.original_detail();
                            let thumbnail = self.workspace.as_ref().filter(|_| self.source_length() > 0).and_then(|workspace| {
                                self.thumbnails.show(thumbnails::Key {
                                    session: workspace.session,
                                    revision: workspace.document.revision_id().clone(),
                                    slot: thumbnails::Slot::Original,
                                    view: ProjectView::Source { asset: asset.clone(), frame: SourceFrameId(0) },
                                })
                            });
                            let mut response = cards::original(ui, label, &detail, self.selected_sound.is_none() && self.selected_source.as_ref() == Some(asset), thumbnail);
                            if let Some(explanation) = self.proxies.explanation() { response = response.on_hover_text(explanation); }
                            if response.clicked() { self.select_source(asset.clone()); }
                        }
                        ui.label(egui::RichText::new("Your starting point stays intact.").size(12.0).weak());
                        ui.add_space(8.0);
                        self.reuse_heading(ui);
                        if ui.add(style::row_action(ui, "Browse", ":source")).clicked()
                            && let Some(asset) = self.workspace.as_ref().and_then(|workspace| original_asset(workspace)).cloned() { self.select_source(asset); }
                        if ui.add_enabled(!self.service.is_busy(), style::row_action(ui, "Reuse all", self.editor_key(EditorKey::Insert))).on_hover_text(format!("Append the entire Original after the selected beat in the current group. Select a range in Original with {}, move with {}, then copy with {}.", self.editor_key(EditorKey::Visual), self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/"), self.bindings.key_label(EditorKey::Copy))).clicked() { self.insert(); }
                    } else if self.workspace.is_none() {
                        ui.weak("One video. Start intact, then make it strange.");
                        ui.add_space(8.0);
                        ui.small("Projects live in Documents/Deadpan.");
                    } else {
                        let search_hint = format!("Find source  {}", self.editor_key(EditorKey::Search));
                        let search = ui.add(egui::TextEdit::singleline(&mut self.source_search).id(egui::Id::new(SEARCH_ID)).hint_text(search_hint).desired_width(f32::INFINITY));
                        accessibility::name(&search, "Find source");
                        if search.has_focus() { self.pane = Pane::Sources; }
                        retain_text_escape(ui, SEARCH_ID);
                        if search.changed() { self.filter_sources(); }
                        let sources = Arc::clone(&self.source_rows);
                        let mut scroll = egui::ScrollArea::vertical().id_salt("legacy-source-list").max_height(190.0);
                        if std::mem::take(&mut self.reveal_source) && let Some(index) = sources.iter().position(|source| Some(&source.0) == self.selected_sound.as_ref().or(self.selected_source.as_ref())) { scroll = scroll.vertical_scroll_offset(index as f32 * (34.0 + ui.spacing().item_spacing.y)); }
                        scroll.show_rows(ui, 34.0, sources.len(), |ui, range| {
                            for index in range {
                                let (asset, label, video) = &sources[index];
                                if ui.selectable_label(self.selected_sound.as_ref().or(self.selected_source.as_ref()) == Some(asset), format!("{}  {label}", if *video { "Video" } else { "Audio" })).clicked() {
                                    if *video { self.select_source(asset.clone()); } else { self.select_sound(asset.clone()); }
                                    ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sources)));
                                }
                            }
                        });
                        ui.weak("Legacy project. Existing sources remain available.");
                    }
                    if matches!(profile, Some(SingleSourceState::Ready { .. })) {
                        ui.add_space(12.0);
                        ui.label(style::section_title("SOUND EFFECTS", false));
                        let search_hint = format!("Find sound  {}", self.editor_key(EditorKey::Search));
                        let search = ui.add(egui::TextEdit::singleline(&mut self.source_search).id(egui::Id::new(SEARCH_ID)).hint_text(search_hint).desired_width(f32::INFINITY));
                        accessibility::name(&search, "Find sound");
                        if search.has_focus() { self.pane = Pane::Sources; }
                        retain_text_escape(ui, SEARCH_ID);
                        if search.changed() { self.filter_sources(); }
                        let sounds = Arc::clone(&self.sound_rows);
                        if sounds.is_empty() { ui.weak(if self.source_search.is_empty() { "No sounds added." } else { "No matching sounds." }); }
                        let reveal = self.selected_sound.is_some() && std::mem::take(&mut self.reveal_source);
                        let mut catalog = egui::ScrollArea::vertical().id_salt("sound-catalog").max_height(120.0);
                        if reveal && let Some(index) = sounds.iter().position(|sound| Some(&sound.0) == self.selected_sound.as_ref()) {
                            catalog = catalog.vertical_scroll_offset(index as f32 * (32.0 + ui.spacing().item_spacing.y));
                        }
                        let catalog = catalog.show_rows(ui, 32.0, sounds.len(), |ui, range| {
                            for index in range {
                                let (asset, label, _) = &sounds[index];
                                if ui.selectable_label(self.selected_sound.as_ref() == Some(asset), label)
                                    .on_hover_text(format!("Select to audition this sound. {} plays or pauses; {} loops the whole sound.", self.editor_key(EditorKey::Playback), self.editor_key(EditorKey::Audition))).clicked() {
                                    self.select_sound(asset.clone());
                                    ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sources)));
                                }
                            }
                        });
                        if reveal { ui.scroll_to_rect(catalog.inner_rect, Some(egui::Align::Max)); }
                    }
                    let available = self.workspace.is_some() && !matches!(profile, Some(SingleSourceState::AwaitingSource { .. })) && !self.service.is_busy() && !self.dialogs.is_open() && !self.importing();
                    if self.workspace.is_some() && !matches!(profile, Some(SingleSourceState::AwaitingSource { .. })) {
                        if ui.add_enabled(available, style::row_action(ui, if focused { "Add sound…" } else { "Import media…" }, "⌘I")).clicked() {
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
                                ui.label("Unlabelled channels");
                                let choice = |value: Option<crate::project::AudioLayoutInterpretation>| match value {
                                    None => "Not chosen".to_owned(),
                                    Some(choice) => format!("{}, {} ch", choice.label(), choice.channels()),
                                };
                                egui::ComboBox::from_id_salt("sound-interpretation").selected_text(choice(self.sound_interpretation)).show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.sound_interpretation, None, choice(None));
                                    for option in crate::project::AudioLayoutInterpretation::ALL { ui.selectable_value(&mut self.sound_interpretation, Some(option), choice(Some(option))); }
                                });
                                ui.small("Automatic selects the first actual audio stream. Advanced indices refer to the container, not the audio-track order.");
                                ui.small("Unlabelled channels: how to hear a file that declares no speaker layout, such as a plain WAV. Not chosen refuses such a file; Deadpan never guesses speakers from the channel count. A declared layout is used as is. Keyboard: :sound-channels mono|stereo|none.");
                                ui.small("Admitted: PCM16 WAV or qualified MP4 audio.");
                            }
                            ui.checkbox(&mut self.linked_import, "Link to original location");
                        });
                    }
                    // A completed import is already visible in the rail and
                    // the status line; keep only in-progress or failed states.
                    if let Some(import) = self.import.as_ref().filter(|import| import.stage != ImportStage::Complete || import.error.is_some()) {
                        let completed_sound = matches!(profile, Some(SingleSourceState::Ready { .. }))
                            && import.asset.as_ref().and_then(|asset| self.workspace.as_ref()?.sources.get(asset)).is_some_and(|source| {
                                source.receipt.snapshot().video().is_none() && source.receipt.snapshot().audio().is_some()
                            });
                        ui.separator();
                        ui.label(egui::RichText::new(match import.stage {
                            ImportStage::Retaining => "Retaining original…", ImportStage::Decoding => "Qualifying media…", ImportStage::PreparingInsertion => "Preparing reuse…", ImportStage::Registering => "Saving source…", ImportStage::Complete if completed_sound => "Sound added", ImportStage::Complete => "Source ready", ImportStage::Cancelled => "Import cancelled", ImportStage::Failed => "Import failed",
                        }).size(12.0));
                        if let Some(error) = &import.error {
                            let response = ui.colored_label(ui.visuals().error_fg_color, error);
                            // Reveal a new refusal below the fold until it has
                            // been fully shown once; a discarded layout pass
                            // cannot consume the reveal.
                            if self.revealed_import_error.as_ref() != Some(error) {
                                if ui.clip_rect().contains_rect(response.rect) {
                                    self.revealed_import_error = Some(error.clone());
                                } else {
                                    response.scroll_to_me(None);
                                }
                            }
                        }
                        if self.importing() && ui.button("Cancel import").clicked() { self.submit(ProjectRequest::CancelImport); }
                    }
                    if matches!(profile, Some(SingleSourceState::Ready { .. })) {
                        self.transcript_section(ui);
                    }
                });
            });
    }

    fn timeline(&mut self, ui: &mut egui::Ui, compact_sounds_heading: bool) {
        if self.scoped.is_some() && self.view == View::Sequence {
            self.scoped_timeline(ui);
            return;
        }
        let mut layout = self.workspace_layout(ui);
        if self.gain.is_some() {
            layout.beats = 42.0;
        }
        style::beat_panel(layout).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            if self.gain.is_some() {
                if let Some(beat) = self.beat_rows.iter().find(|beat| self.selected_beat.as_ref() == Some(&beat.id)) {
                    ui.horizontal(|ui| {
                        ui.weak("GAIN OWNER");
                        { let text = format!("{} · {} f · {}", beat.label, beat.frames, self.beat_scope_label()); accessibility::full_text(ui.add(egui::Label::new(&text).truncate()), &text) }
                            .on_hover_text(&beat.label);
                    });
                }
                return;
            }
            let heading = self.sequence_heading(ui, compact_sounds_heading);
            let beats = Arc::clone(&self.beat_rows);
            if pane_focus(
                ui,
                Pane::Sequence,
                heading.rect,
                "Current group beat outline pane",
            )
            .has_focus()
            {
                self.pane = Pane::Sequence;
            }
            if beats.is_empty() {
                ui.add_space(16.0);
                if !self.sequence_scope.groups().is_empty() {
                    ui.weak(format!("This group is empty. {} returns to its parent; {} reuses the Original here.", self.editor_key(EditorKey::LeaveGroup), self.editor_key(EditorKey::Insert)));
                    return;
                }
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
            let analysis = self.edit_analysis_marks();
            let markers = cards::Markers { cursor: marker, frame: self.sequence_cursor, range: self.selected_edit_range().map(|range| range.start().0 as u64..range.end().0 as u64), analysis };
            let reveal = std::mem::take(&mut self.reveal_beat);
            let identity = self.workspace.as_ref().map(|workspace| (workspace.session, workspace.document.revision_id().clone()));
            let thumbnails = &mut self.thumbnails;
            let mut thumbnail = |beat: &BeatRow| {
                let (session, revision) = identity.clone()?;
                thumbnails.show(thumbnails::Key {
                    session,
                    revision,
                    slot: thumbnails::Slot::Beat(beat.id.clone()),
                    view: ProjectView::Sequence { frame: ProjectFrame(i64::try_from(beat.start).ok()?) },
                })
            };
            if let Some(index) = cards::strip(
                ui,
                layout,
                &beats,
                selected.as_ref(),
                markers,
                reveal,
                &mut thumbnail,
            ) {
                let beat = &beats[index];
                self.bindings.clear();
                self.selected_beat = Some(beat.id.clone());
                self.selected_event = None;
                self.sequence_cursor = beat.start;
                self.view.set(View::Sequence, &mut self.message);
                self.edit_range.move_to(self.sequence_cursor);
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
        self.event_focused()
            || (self.view == View::Source && self.moment_identity().is_some())
            || self.view == View::Sequence
                && self
                    .selected_beat
                    .as_ref()
                    .is_some_and(|node| self.beat_rows.iter().any(|row| &row.id == node))
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
        if self.view != View::Sequence || !self.inspector_visible() {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        let row = self
            .beat_rows
            .iter()
            .find(|row| Some(&row.id) == self.selected_beat.as_ref())?;
        let node = workspace.document.nodes().get(&row.id)?;
        let mut description = inspector::Inspector::describe(node, row.start, row.frames);
        // A split fragment shows its Source's sound offset (`:audio-lag`).
        if !matches!(node.kind, deadpan_core::NodeKind::Source { .. })
            && let Some((host, _)) = deadpan_core::cutaway_host(&workspace.document, &row.id)
            && let Some(deadpan_core::NodeKind::Source { source }) =
                workspace.document.nodes().get(&host).map(|host| &host.kind)
            && source.audio_offset.0 != 0
        {
            let samples = source.audio_offset.0;
            description.fields.push((
                "Sound offset",
                format!(
                    "{:.1} ms {}",
                    samples.unsigned_abs() as f64 / 48.0,
                    if samples > 0 { "late" } else { "early" }
                ),
            ));
        }
        // A split fragment lists its Source's captions over its own frames.
        if !matches!(
            node.kind,
            deadpan_core::NodeKind::Source { .. } | deadpan_core::NodeKind::Hold { .. }
        ) && let Some((host, offset)) = deadpan_core::cutaway_host(&workspace.document, &row.id)
            && let Some(host) = workspace.document.nodes().get(&host)
        {
            let end = offset + row.frames as i64;
            let visible: Vec<String> = host
                .captions
                .iter()
                .filter(|caption| caption.range.start().0 < end && caption.range.end().0 > offset)
                .map(|caption| {
                    format!(
                        "“{}” {}–{}",
                        caption.text,
                        (caption.range.start().0 - offset).max(0),
                        (caption.range.end().0 - offset).min(row.frames as i64)
                    )
                })
                .collect();
            if !visible.is_empty() {
                description.fields.push(("Captions", visible.join(", ")));
            }
        }
        // Independent gap Holds replace the default gap after their play.
        if let deadpan_core::NodeKind::Repeat {
            iterations, gap, ..
        } = &node.kind
            && let Some(branches) = workspace.document.gap_overrides().get(&row.id)
            && let Some(field) = description
                .fields
                .iter_mut()
                .find(|(label, _)| *label == "Between plays")
        {
            let shown = iterations.len().saturating_sub(1).min(8);
            let gaps: Vec<String> = (0..shown)
                .filter_map(|position| iterations.at(position))
                .map(|after| match branches.get(&after) {
                    Some(branch) => match workspace.document.nodes().get(branch).map(|n| &n.kind) {
                        Some(deadpan_core::NodeKind::Hold { recipe }) => {
                            format!("{} f", recipe.duration.frames())
                        }
                        _ => "edited".into(),
                    },
                    None => gap.as_ref().map_or_else(
                        || "none".into(),
                        |gap| format!("{} f", gap.duration.frames()),
                    ),
                })
                .collect();
            let more = if iterations.len().saturating_sub(1) > shown {
                " …"
            } else {
                ""
            };
            field.1 = format!("{}{more}", gaps.join(" · "));
        }
        // A follow names its target by label, never by its internal id.
        if let Some(deadpan_core::Framing {
            value: deadpan_core::FramingValue::Follow { target, scale, .. },
            ..
        }) = &node.framing
            && let Some(field) = description
                .fields
                .iter_mut()
                .find(|(label, _)| *label == "Framing")
        {
            let scale = scale.numerator() as f64 / scale.denominator() as f64;
            *field = (
                "Follows",
                format!("{} · {scale:.2}×", self.target_label(target)),
            );
        }
        Some(description)
    }

    fn retime_hint(&self) -> Option<Result<String, String>> {
        let Ok(navigation::command::Entry::Action(Action::Edit(BeatEdit::Retime(input)))) =
            navigation::command::parse(&self.command)
        else {
            return None;
        };
        Some((|| {
            if self.view != View::Sequence {
                return Err(
                    "The Original is unchanged. Select a beat in Your edit to change its speed."
                        .into(),
                );
            }
            let workspace = self
                .workspace
                .as_ref()
                .ok_or("Open a project before changing speed.")?;
            let target = self
                .selected_beat
                .as_ref()
                .ok_or("Select a beat before changing speed.")?;
            if !self
                .sequence_scope
                .resolve(workspace)?
                .children
                .contains(target)
            {
                return Err("Select a direct child of the active Sequence before editing.".into());
            }
            crate::project::retime::resolve(workspace, target, input.speed, input.wrap)
                .map(|change| change.describe(input.pitch))
        })())
    }

    fn open_inspector_parameter(&mut self, context: &egui::Context) -> bool {
        if self.open_sound_position(context) {
            return true;
        }
        if self.scoped.is_some() && self.view == View::Sequence && !self.event_focused() {
            return self.enter_scoped(context);
        }
        let Some((_, command)) = self.inspector_description().and_then(|data| data.parameter)
        else {
            return false;
        };
        self.open_command(command, context);
        true
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        if self.event_focused() {
            self.event_inspector(ui);
            return;
        }
        if self.camera.is_some() {
            self.camera_inspector(ui);
            return;
        }
        if self.scoped.is_some() && self.view == View::Sequence {
            self.scoped_inspector(ui);
            return;
        }
        if self.view == View::Source && self.inspector_visible() {
            self.moment_inspector(ui);
            return;
        }
        let Some(data) = self.inspector_description() else {
            return;
        };
        let Some(workspace) = &self.workspace else {
            return;
        };
        let frame_rate = frame_rate_label(workspace.document.presentation_basis().frame_rate);
        let can_retime = self
            .selected_beat
            .as_ref()
            .and_then(|node| workspace.plan.node_duration(node))
            .is_some_and(|duration| duration.frames() > 0);
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
                    "Selected beat inspector pane",
                )
                .has_focus()
                {
                    self.pane = Pane::Inspector;
                }
                ui.separator();
                // Focus reveals set the offset at once: an animated reveal is
                // queued for the next frame, where a still-decaying wheel
                // scroll over the inspector cancels it and leaves the newly
                // focused control hidden below the fold.
                egui::ScrollArea::vertical()
                    .id_salt("inspector-details")
                    .animated(false)
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
                                style::ink_padded_label(
                                    ui,
                                    style::semibold(&data.label).size(16.0),
                                    Some(egui::TextWrapMode::Truncate),
                                )
                                .on_hover_text(&data.label);
                                ui.label(egui::RichText::new(data.kind).weak());
                            });
                        });
                        ui.add_space(8.0);
                        let ready = !self.service.is_busy() && !self.dialogs.is_open();
                        if self.selected_group()
                            && ui.add(style::row_action(ui, "Enter group", self.editor_key(EditorKey::EnterGroup)).fill(style::SELECTED)).clicked()
                        {
                            self.enter_group(ui.ctx());
                            return;
                        }
                        if let Some((label, command)) = &data.parameter
                            && ui.add_enabled(ready, style::row_action(ui, *label, "Enter").fill(style::SELECTED)).clicked()
                        {
                            self.pane = Pane::Inspector;
                            self.open_command(command.clone(), ui.ctx());
                        }
                        ui.add_space(4.0);
                        let scope = self.beat_scope_label();
                        style::value_grid(
                            ui,
                            "inspector-values",
                            [
                                ("Duration", data.duration.as_str()),
                                ("Boundaries", data.range.as_str()),
                                ("Scope", scope.as_str()),
                            ]
                            .into_iter()
                            .chain(data.fields.iter().map(|(label, value)| (*label, value.as_str()))),
                        );
                        ui.label(
                            egui::RichText::new(format!("Project clock {frame_rate}. {}", data.note))
                                .size(12.0)
                                .weak(),
                        );
                        if data.kind == "Hold" {
                            self.ai_inspector(ui, ready);
                            self.hold_audio_controls(ui, ready);
                        }
                        self.gain_inspector(ui, ready);
                        ui.add_space(8.0);
                        ui.label(style::section_title("EDIT", false));
                        ui.add_enabled_ui(ready, |ui| {
                            if data.kind != "Retime"
                                && ui.add_enabled(can_retime, style::row_action(ui, "Change speed…", ":retime"))
                                    .on_hover_text("Choose an exact playback speed and preserve or tape pitch. Enter applies one undoable edit; Escape cancels entry.")
                                    .clicked()
                            {
                                self.pane = Pane::Inspector;
                                self.open_command("retime 0.75 pitch=preserve".into(), ui.ctx());
                            }
                            if ui.add(style::row_action(ui, "Camera…", self.editor_key(EditorKey::Camera))).clicked() {
                                self.framing_action(navigation::FramingAction::EnterCamera, ui.ctx());
                            }
                            if ui.add(style::row_action(ui, "Insert pause", self.editor_key(EditorKey::Hold)))
                                .on_hover_text(format!("Insert 0.5 s of frozen picture and silence at the cursor. A count scales the duration: {} adds 1.5 s.", self.editor_counted(EditorKey::Hold, 3)))
                                .clicked()
                            {
                                self.edit(BeatEdit::InsertHold(navigation::duration::DurationInput::half_seconds(1)));
                            }
                            if ui.add(style::row_action(ui, "Insert pause of…", ":hold"))
                                .on_hover_text("Choose the pause duration before inserting it.")
                                .clicked()
                            {
                                self.pane = Pane::Inspector;
                                self.open_command("hold 0.5s".into(), ui.ctx());
                            }
                            let can_split = self.selected_beat.as_ref().is_some_and(|node| {
                                selection::split_boundary(&self.beat_rows, node, self.sequence_cursor).is_some()
                            });
                            if ui.add_enabled(can_split, style::row_action(ui, "Split at cursor", self.editor_key(EditorKey::Split)))
                                .on_hover_text(format!("Move inside this beat with {}. Split keeps its picture, sound and total duration unchanged.", self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/")))
                                .clicked()
                            {
                                self.edit(BeatEdit::Split);
                            }
                            if ui.add(style::row_action(ui, "Wrap repeat", self.editor_key(EditorKey::Repeat)))
                                .on_hover_text("Two total plays. An existing Repeat is nested.")
                                .clicked()
                            {
                                self.edit(BeatEdit::WrapRepeat(2));
                            }
                            let selection = self.edit_selection();
                            let copy = if self.copied.is_pending() {
                                egui::Button::new("Copy pending…").min_size(egui::vec2(ui.available_width(), 28.0))
                            } else {
                                style::row_action(ui, if selection == navigation::EditSelection::None { "Copy beat" } else { "Copy range" }, self.editor_key(EditorKey::Copy))
                            };
                            if ui.add_enabled(selection != navigation::EditSelection::Empty && !self.service.is_busy(), copy)
                                .on_hover_text("Retain this linked Edit slice for paste or visible placement. Copying does not change the edit.")
                                .clicked()
                            {
                                self.copy_slice();
                            }
                            let cut = if selection == navigation::EditSelection::None {
                                style::row_action(ui, "Cut beat", self.editor_key(EditorKey::CutBeat))
                            } else {
                                style::row_action(ui, "Cut selection", self.editor_key(EditorKey::CutRange))
                            };
                            if ui.add_enabled(selection != navigation::EditSelection::Empty, cut)
                                .on_hover_text("Save one linked cut and copy its structure for paste. Failure keeps the previous copy; Undo restores the removed content.")
                                .clicked()
                            {
                                self.edit(BeatEdit::Delete);
                            }
                            let presets = format!("Framing presets  {}", self.editor_pair(EditorKey::PunchIn, EditorKey::Creep, " / "));
                            ui.collapsing(presets, |ui| {
                                for (label, key, action) in [
                                    ("Punch in 1.35× on target", self.editor_key(EditorKey::PunchIn), navigation::FramingAction::PunchIn),
                                    ("Creep to 1.35×", self.editor_key(EditorKey::Creep), navigation::FramingAction::Creep),
                                ] {
                                    if ui.add(style::row_action(ui, label, key)).clicked() {
                                        self.framing_action(action, ui.ctx());
                                    }
                                }
                            });
                        });
                        self.targets_inspector(ui);
                    });
            });
    }

    fn viewer(&mut self, ui: &mut egui::Ui, compact_sounds_heading: bool, sounds_in_heading: bool) {
        // At compact heights give the picture the eight points otherwise spent
        // on extra outer padding. Keep control reserves and hit sizes intact.
        let frame = if ui
            .ctx()
            .input(|input| input.content_rect().height() < 700.0)
        {
            style::panel().inner_margin(egui::Margin::symmetric(12, 8))
        } else {
            style::panel()
        };
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal_wrapped(|ui| {
                // A segmented pair: adjacent, with only the outer corners round.
                ui.spacing_mut().item_spacing.x = 0.0;
                for (index, (label, view)) in [(if self.focused_workflow() { "Original" } else { "Source" }, View::Source), (if self.focused_workflow() { "Your edit" } else { "Sequence" }, View::Sequence)].into_iter().enumerate() {
                    let corners = if index == 0 { egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 } } else { egui::CornerRadius { nw: 0, sw: 0, ne: 5, se: 5 } };
                    let selected = self.view == view;
                    let text = if selected { style::semibold(label).color(style::LAVENDER) } else { egui::RichText::new(label).weak() };
                    if ui.add_enabled(view == View::Source || self.workspace.is_some(), egui::Button::new(text).min_size(egui::vec2(92.0, 28.0)).corner_radius(corners).selected(selected)).clicked() {
                        self.stop_playback();
                        self.selected_sound = None;
                        self.selected_event = None;
                        self.sound_inspection = None;
                        self.bindings.clear();
                        if self.view != view { self.view.set(view, &mut self.message); if view == View::Sequence { self.reconcile_beat_selection(); } self.request_picture(true); }
                        self.pane = Pane::Viewer;
                        ui.memory_mut(|m| m.request_focus(pane_id(Pane::Viewer)));
                    }
                }
                ui.add_space(12.0);
                ui.spacing_mut().item_spacing.x = 2.0;
                if self.camera.is_none() && (self.workspace.is_some() || self.raw_source.is_some()) {
                    for (label, key, action) in [("Start", self.editor_key(EditorKey::First), Action::First), ("Previous", self.editor_key(EditorKey::FramePrevious), Action::Step { forward: false, count: 1 }), ("Next", self.editor_key(EditorKey::FrameNext), Action::Step { forward: true, count: 1 }), ("End", self.editor_key(EditorKey::Last), Action::Last)] {
                        if ui.add(style::action(label, key).frame_when_inactive(false).min_size(egui::vec2(0.0, 28.0))).clicked() {
                            self.selected_event = None;
                            self.sound_inspection = None;
                            self.pane = Pane::Viewer;
                            ui.memory_mut(|m| m.request_focus(pane_id(Pane::Viewer)));
                            self.action(action, ui.ctx());
                        }
                    }
                }
            });
            if self.camera.is_some() {
                ui.label(egui::RichText::new("CAMERA · Draft preview").color(style::LAVENDER));
            }
            // The empty start surface has no transport or moment controls; its
            // card uses the whole viewer.
            let start_surface = self.workspace.is_none() && self.raw_source.is_none();
            let controls_height = if start_surface {
                0.0
            } else if self.gain.is_some() {
                54.0
            } else if self.camera.is_some() {
                76.0
            } else {
                let other_controls = if self.view == View::Sequence {
                    if self.copied.selected_content().is_some() {
                        if compact_sounds_heading { 68.0 } else { 92.0 }
                    } else {
                        54.0
                    }
                } else if self.compact_original_controls(ui.ctx()) {
                    100.0
                } else {
                    110.0
                };
                other_controls + self.playback_controls_height(ui)
            };
            let available = egui::vec2(ui.available_width().max(1.0), (ui.available_height() - controls_height).max(50.0));
            let (_, rect) = ui.allocate_space(available);
            let response = pane_focus(ui, Pane::Viewer, rect, "Picture viewer pane");
            if response.has_focus() { self.pane = Pane::Viewer; }
            ui.painter().rect_filled(rect, 2.0, egui::Color32::BLACK);
            let aspect = self.presentation.canvas().map(|(w, h)| w as f32 / h as f32);
            let canvas = aspect.map_or(rect, |aspect| fit_rect(rect, aspect));
            // Earlier panes and this viewer's tabs can change view while
            // painting. Reject their old panel allocation before GPU work.
            if compact_sounds_heading != self.compact_sounds_heading(ui.ctx())
                || self.sounds_in_heading(ui.ctx()) != sounds_in_heading
            {
                ui.ctx().request_discard("Sounds heading changed placement");
            }
            // Controls below the picture can change its reserved height later
            // in this pass. Let their input run before any resized GPU target
            // is submitted, including native button and accessibility actions.
            if ui.ctx().current_pass_index() == 0 {
                let controls = egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.bottom()), ui.max_rect().right_bottom(),
                );
                let native_focus = native_control_focused(ui.ctx());
                let pending = ui.input(|input| {
                    let dragging = input.pointer.is_decidedly_dragging();
                    input.events.iter().any(|event| match event {
                        // A press or release can change the controls; moving
                        // within an ongoing drag (a slider) cannot change
                        // their reserved height, so it needs no retry.
                        egui::Event::PointerButton { pos, .. } => controls.contains(*pos) || dragging,
                        egui::Event::Key { key: egui::Key::Enter | egui::Key::Space, pressed: true, .. } => native_focus,
                        egui::Event::AccessKitActionRequest(request) => request.action == egui::accesskit::Action::Click,
                        _ => false,
                    })
                });
                if pending { ui.ctx().request_discard("viewer controls will process input"); }
            }
            self.render_picture(ui.ctx(), canvas.size());
            let displayed_label = self.presentation.displayed_label();
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, displayed_label.as_deref().unwrap_or(if start_surface { "Picture viewer: no project open" } else { "No picture displayed" })));
            if self.presentation.has_displayed() && !(self.view == View::Sequence && self.sequence_length() == 0) {
                if let Some(target) = &self.target {
                    // The retained texture includes its own composition and
                    // letterboxing. Preserve those pixels' aspect while a new
                    // layout or decoded canvas waits for GPU submission.
                    let painted = fit_rect(rect, target.target.width() as f32 / target.target.height() as f32);
                    ui.painter().image(target.texture, painted, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    if self.presentation.displayed_tier() == Some(crate::worker::PictureTier::Proxy) {
                        style::proxy_badge(ui.painter(), painted);
                    }
                    if let Some(label) = self.workspace.as_ref().and_then(|workspace| workspace.color_label()) {
                        style::color_badge(ui.painter(), painted, label);
                    }
                }
                else { ui.painter().rect_filled(canvas, 0.0, egui::Color32::BLACK); }
            } else if self.workspace.is_none() && self.raw_source.is_none() && !self.presentation.loading() {
                // The empty start surface: one card to choose, paste or open.
                ui.painter().rect_filled(rect, 2.0, style::CANVAS);
                self.start_card(ui, rect);
            } else {
                let message = if self.presentation.loading() { "Preparing picture…" } else if self.presentation.error().is_some() { "Picture unavailable" } else if self.view == View::Sequence { "Your edit is empty" } else if self.selected_source.is_some() && self.source_length() == 0 { "Audio source · no picture" } else { "Choose the Original to begin" };
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, message, egui::FontId::proportional(18.0), style::muted(ui));
            }
            ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, if self.pane == Pane::Viewer { style::LAVENDER } else { accessibility::border(ui.ctx()) }), egui::StrokeKind::Inside);
            self.camera_overlay(ui, canvas);
            if let Some(label) = displayed_label {
                let mut response = accessibility::full_text(ui.add(egui::Label::new(egui::RichText::new(&label).size(11.5).weak()).truncate()), &label).on_hover_text(&label);
                if let Some(summary) = &self.summary { response = response.on_hover_text(format!("Measured source: {} × {} pixels; original PTS [{}, {}), clock {}/{} seconds per tick.", summary.info.width, summary.info.height, summary.first_pts, summary.terminal_pts, summary.info.time_base_num, summary.info.time_base_den)); }
                if let Some(source_frame) = self.presentation.displayed_source_frame() { response.on_hover_text(format!("Original source frame {}", u128::from(source_frame.0) + 1)); }
            } else if !start_surface { ui.label(egui::RichText::new("Stopped-frame inspection").size(11.5).weak()); }
            if let Some(workspace) = &self.workspace && let Some(original) = workspace.original_duration {
                // These clocks are read-only. At minimum height, reserve text
                // height instead of the ordinary 28-point button row.
                let clock_height = if compact_sounds_heading { 14.0 } else { ui.spacing().interact_size.y };
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), clock_height), egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true), |ui| {
                    let edit = workspace.plan.duration();
                    ui.label(egui::RichText::new(format!("Original {} f", original.frames())).monospace()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("Full Original duration: {} project frames", original.frames())));
                    ui.separator();
                    ui.label(egui::RichText::new(format!("Your edit {} f", edit.frames())).monospace()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("Your edit duration: {} project frames", edit.frames())));
                    egui::Frame::new().fill(style::SELECTED).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| {
                        ui.label(egui::RichText::new(format!("{:+} f", edit.frames() - original.frames())).monospace().color(style::LAVENDER));
                    });
                    ui.label(egui::RichText::new(format!("project clock · {}", frame_rate_label(workspace.document.presentation_basis().frame_rate))).size(10.0).weak());
                }).response.on_hover_text("Both durations use the same project-frame clock. Original includes the measured picture/audio stream union; source browsing counts decoded picture frames separately.");
            }
            if self.camera.is_some() {
                ui.weak("Apply or cancel Camera to resume navigation and playback.");
            } else if !start_surface {
                if self.gain.is_none() { self.moment_controls(ui); }
                self.playback_controls(ui);
                ui.horizontal_wrapped(|ui| {
                if self.view == View::Source && !self.focused_workflow()
                    && ui.add_enabled(self.workspace.is_some() && self.selected_source.is_some() && !self.service.is_busy(), egui::Button::new(format!("Insert source  {}", self.editor_key(EditorKey::Insert))).wrap().fill(style::SELECTED)).on_hover_text("Insert the whole source after the selected beat in the current group. This creates an undoable edit.").clicked() { self.insert();
                }
                });
            }
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
        // The final layout pass supplies the actual viewer dimensions. Keep
        // the accepted target until then instead of submitting an unused size.
        if context.will_discard() || !self.presentation.can_render() {
            return;
        }
        let Some(picture) = self.presentation.picture() else {
            return;
        };
        // Captions over an authored black picture render through the shared
        // pass like any other picture; an uncaptioned one needs no target.
        if picture.frame.is_none() && (picture.captions.is_empty() || picture.canvas.is_none()) {
            if self.presentation.needs_render() {
                self.forget_target();
                #[cfg(feature = "ui-harness")]
                self.feedback
                    .picture_submitted(self.presentation.decoded_ticket());
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
                #[cfg(feature = "ui-harness")]
                self.feedback
                    .picture_failed(self.presentation.decoded_ticket());
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
                    #[cfg(feature = "ui-harness")]
                    self.feedback
                        .picture_failed(self.presentation.decoded_ticket());
                    self.presentation
                        .render_failed(format!("Preview renderer: {error}"));
                    return;
                }
            })
        } else {
            None
        };
        let picture = self.presentation.picture().expect("picture checked");
        let target = replacement
            .as_ref()
            .unwrap_or_else(|| &self.target.as_ref().expect("target exists").target);
        let captions = match picture.canvas.map(|(width, height)| {
            deadpan_cli::picture::caption_overlay(
                &picture.captions,
                [width, height],
                [target.width(), target.height()],
            )
        }) {
            Some(Ok(captions)) => captions,
            None => None,
            Some(Err(error)) => {
                #[cfg(feature = "ui-harness")]
                self.feedback
                    .picture_failed(self.presentation.decoded_ticket());
                self.presentation
                    .render_failed(format!("Preview captions: {error}"));
                return;
            }
        };
        // Preview applies the committed revision's automatic SDR/HDR branch,
        // the same decision export derives; raw sources preview as SDR with
        // any HDR picture tone-mapped.
        self.renderer.set_color_pipeline(
            self.workspace
                .as_ref()
                .map(|workspace| {
                    use deadpan_cli::picture::ColorDecisionPipeline;
                    workspace.color_decision().pipeline()
                })
                .unwrap_or_default(),
        );
        let result = if let Some(frame) = picture.frame.as_ref()
            && let Some((width, height)) = picture.canvas
        {
            match camera::render_layers(picture) {
                Ok(layers) => self.renderer.render_composed_captioned(
                    frame,
                    target,
                    picture.picture_context.as_deref(),
                    [width, height],
                    FitMode::Fit,
                    &layers,
                    captions.as_ref(),
                ),
                Err(error) => {
                    #[cfg(feature = "ui-harness")]
                    self.feedback
                        .picture_failed(self.presentation.decoded_ticket());
                    self.presentation.render_failed(error);
                    return;
                }
            }
        } else if let Some(frame) = picture.frame.as_ref() {
            self.renderer.render(frame, target, FitMode::Fit)
        } else {
            self.renderer
                .render_background_captioned(target, captions.as_ref())
        };
        match result {
            Ok(_) => {
                #[cfg(feature = "ui-harness")]
                self.feedback
                    .picture_submitted(self.presentation.decoded_ticket());
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
                #[cfg(feature = "ui-harness")]
                self.feedback
                    .picture_failed(self.presentation.decoded_ticket());
                self.presentation
                    .render_failed(format!("Preview renderer: {error}"));
            }
        }
    }

    /// The editor context Help marks as "here".
    fn help_context(&self) -> registry::Contexts {
        use registry::Contexts as C;
        if self.sound_focused() {
            C::CATALOG
        } else if self.pane == Pane::Sounds || self.event_focused() {
            C::PLACED
        } else if self.view == View::Source {
            C::ORIGINAL
        } else if self.scoped.is_some() {
            C::CONTENTS
        } else if self.edit_selection() != navigation::EditSelection::None {
            C::VISUAL
        } else {
            C::EDIT
        }
    }

    /// The Keys sheet: every registered action once, by section, with its
    /// live keys, verbs and where it works; or the search results.
    fn help(&mut self, context: &egui::Context) {
        let bindings = self.bindings.clone();
        let key = |id| bindings.key_labels(id);
        let keymap_status = self.keymap_status().to_owned();
        let registers: Vec<_> = self
            .copied
            .entries()
            .map(|(name, content)| (name, content.available_label(self.workspace.as_deref())))
            .collect();
        let registers_first = self.help_registers_first;
        let expansion = self.help_expansion.clone();
        let here = self.help_context();
        self.help_scroll.show(context, &mut self.help_open, |ui, query| {
            ui.label(&keymap_status);
            if let Some((label, rows)) = &expansion {
                ui.label(style::section_title("RECIPE EXPANSION", true));
                ui.label(format!("{label} expands to these ordinary steps, as one Undo. Nothing has been applied; enter :gag with the same parameters to apply it."));
                for (index, row) in rows.iter().enumerate() {
                    help_binding(ui, &format!("{}", index + 1), row);
                }
                ui.separator();
            }
            if !query.trim().is_empty() {
                help_search_results(ui, &bindings, query, here);
                return;
            }
            ui.label(
                egui::RichText::new(format!(
                    "Each action once. Lavender notes mark what works here in {}.",
                    here.label()
                ))
                .size(11.0)
                .color(style::LAVENDER),
            );
            ui.label("New starts with your full video. Its Original stays intact while Your edit changes.");
            let mut sections = registry::Section::ALL.to_vec();
            if registers_first {
                sections.retain(|section| {
                    !matches!(section, registry::Section::Registers | registry::Section::Macros)
                });
                sections.splice(0..0, [registry::Section::Registers, registry::Section::Macros]);
            }
            for section in sections {
                ui.label(style::section_title(section.title(), true));
                for spec in registry::SPECS.iter().filter(|spec| spec.section == section) {
                    help_spec_row(ui, &bindings, spec, here);
                }
                if section == registry::Section::Registers {
                    if registers.is_empty() {
                        ui.weak("All registers are empty.");
                    }
                    for (name, content) in &registers {
                        help_binding(ui, &format!("{} + {name}", key(EditorKey::RegisterSelect)), content);
                    }
                }
                ui.separator();
            }
            ui.weak(format!("Original browsing never changes it. Your edit commands affect the selected beat in the displayed group and its linked picture and sound. Counts precede operators, such as {}; the visible PENDING badge waits without a timer.", bindings.counted_label(EditorKey::Repeat, 3)));
            ui.weak(format!("{} auditions the focused catalog sound, Original, or full edit. In the catalog, {} selects a sound and {} loops its complete measured audio. Catalog audition keeps the picture and both editor cursors in place. Leaving the catalog or choosing another sound stops it. Elsewhere {} loops the selected Original moment, Edit range or edited beat with context. Playback has edge fades and a safety limiter; pause before changing Monitor volume. Picture-only or sound-only range cuts and placement, temporal edits inside Repeat/Retime contents, moving routed sounds, voice effects, the full mix, and AI generation in the app remain unavailable. Render supports the current SDR picture and audio path; unsupported content fails explicitly. Headless commands can use this open project. Use :renders for saved renders and recovery. HDR output and full mastering remain unavailable.", key(EditorKey::Playback), key_labels::aliases_pair(&bindings, EditorKey::BeatNext, EditorKey::BeatPrevious, "/"), key(EditorKey::Audition), key(EditorKey::Audition)));
        });
    }
}

impl eframe::App for DeadpanApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        style::CANVAS.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        let first_pass = context.current_pass_index() == 0;
        if first_pass {
            self.display.poll(&context);
        }
        // Cards record the thumbnails they show during this layout pass.
        self.thumbnails
            .begin(self.workspace.as_ref().map(|workspace| workspace.session));
        if first_pass && !self.deferred_text_input.is_empty() {
            // A field closes only after its native text is processed. Preserve
            // the later events for the resulting context, ahead of new input.
            // A discarded layout pass must never replay that suffix early.
            context.input_mut(|input| {
                let mut deferred = std::mem::take(&mut self.deferred_text_input);
                deferred.append(&mut input.events);
                input.events = deferred;
            });
        }
        if self.playback_interrupted.swap(false, Ordering::AcqRel) && self.transport.is_some() {
            self.pause_playback();
            self.message = Some(format!(
                "Audition stopped for sleep or wake. Press {} to start again.",
                self.editor_key(EditorKey::Playback)
            ));
        }
        if context.input(|i| i.viewport().close_requested())
            && (self.close_pending || !self.hold_close_for_previews(&context))
        {
            self.close_pending = true;
        }
        self.reconcile_repeats(&context);
        let previous_pane = self.pane;
        // A layout retry reuses this frame's external state. In particular it
        // must not consume a second Repeat completion between input and paint.
        if first_pass {
            self.receive(&context);
            self.reconcile_room_tone(&context);
            self.reconcile_gain(&context);
            self.reconcile_splice(&context);
            self.reconcile_slip(&context);
            self.reconcile_trim(&context);
            self.reconcile_render(&context);
            self.receive_gain_waveform();
            self.receive_trim_media();
            self.reconcile_models(&context);
            self.reconcile_diagnostics(&context);
            self.reconcile_storage(&context);
            self.reconcile_transcription(&context);
            self.reconcile_shots(&context);
            self.reconcile_proxies(&context);
            self.reconcile_youtube(&context);
            self.reconcile_jobs(&context);
            if self.close_pending {
                self.junction_pictures.clear();
            }
            // This consumes no replies and starts no allocation/submission.
            // Drain hidden drafts too, once per outer frame, never a retry.
            self.junction_pictures.drain(&mut self.renderer, &context);
        }
        self.reconcile_sound_playback();
        if first_pass {
            self.receive_playback();
            self.reconcile_camera();
        }
        self.ensure_visible_pane(&context);
        if self.close_pending {
            self.invalidate_trim_media(true);
            self.stop_playback();
            self.cancel_gain_waveform();
            // Release queued and paused background jobs so they drain.
            self.service.jobs().shutdown();
            self.service.shutdown();
            let youtube_drained = self.youtube_drained();
            if !self.service.is_shutdown_complete() || !youtube_drained {
                context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                context.request_repaint_after(Duration::from_millis(16));
                self.message = Some(
                    self.render_job
                        .as_ref()
                        .and_then(|render| render.workflow.as_ref())
                        .filter(|workflow| {
                            workflow.status.stage
                                == deadpan_cli::encoded_render::workflow::WorkflowStage::Unresolved
                        })
                        .and_then(|workflow| {
                            workflow
                                .status
                                .diagnostic
                                .as_ref()
                                .or(workflow.status.journal_diagnostic.as_ref())
                        })
                        .map_or_else(
                            || "Finishing project work before closing…".into(),
                            |diagnostic| {
                                format!("Render shutdown requires recovery: {}", diagnostic.detail)
                            },
                        ),
                );
                ui.disable();
            } else {
                context.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        } else if first_pass {
            self.receive_dialog(&context);
        }
        // A writer completion may select its edited content while the user is
        // entering the next command. Preserve text focus so submission can
        // reject its captured stale target instead of silently closing entry.
        if self.pane != previous_pane && !text_input_active(&context, self.command_open) {
            context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
        }
        // A command opened after last frame's footer receives this batch's text
        // and Enter/Escape events. Its widget will be emitted below this frame.
        if self.command_open {
            focus_command_for_frame(&context, &mut self.command_focus_pending);
        }
        #[cfg(feature = "ui-harness")]
        if first_pass {
            self.feedback.record("input_dispatch");
        }
        #[cfg(target_os = "macos")]
        if first_pass && !self.close_pending {
            self.menu_commands(&context);
        }
        let text_result = if self.close_pending {
            None
        } else {
            self.keyboard(&context)
        };
        if self.command_open && text_result.as_ref().is_some_and(|(_, command)| *command) {
            // Native text is still processed and the command executed below.
            // Its guaranteed footer close must precede any resized submission.
            context.request_discard("native command footer will close");
        }
        let input_scope = (
            self.pane,
            self.view,
            self.selected_beat.clone(),
            self.selected_source.clone(),
            self.selected_sound.clone(),
        );
        if self.render.blocking() || self.marks.open {
            ui.disable();
            ui.set_opacity(1.0);
        }
        // Paint the canvas behind every pane so spacing between panels never
        // exposes the window clear color.
        ui.painter().rect_filled(ui.max_rect(), 0.0, style::CANVAS);
        // The header disables itself inside its panel. Wrapping the panel in a
        // child UI would add item spacing below it as an unpainted band.
        self.header(
            ui,
            self.splice.is_none() && self.slip.is_none() && self.trim.is_none(),
        );
        // A pointer activation can change views while the panes are painted.
        // Use one placement decision for Sounds throughout this pass.
        let compact_sounds_heading = self.compact_sounds_heading(&context);
        let sounds_in_heading = self.sounds_in_heading(&context);
        let footer_mode = (
            self.command_open,
            self.camera.is_some(),
            self.gain.is_some(),
            self.splice.is_some(),
            self.slip.is_some(),
            self.trim.is_some(),
            self.youtube_footer_key(&context),
        );
        if self.trim.is_some() {
            self.trim_workspace(ui);
        } else if self.slip.is_some() {
            self.slip_workspace(ui);
        } else if self.splice.is_some() {
            self.splice_workspace(ui);
        } else {
            self.footer(ui);
            self.gain_panel(ui);
            if self.gain.is_some() {
                // Only draft controls participate in native Tab focus while the
                // comparison is open. Keep the retained picture at full opacity.
                ui.disable();
                ui.set_opacity(1.0);
            }
            self.sources(ui);
            self.inspector(ui);
            self.placed_sounds(ui, sounds_in_heading);
            self.timeline(ui, sounds_in_heading);
            self.viewer(ui, compact_sounds_heading, sounds_in_heading);
        }
        if first_pass {
            self.finish_camera_entry(&context);
        }
        self.reconcile_sound_playback();
        if self.trim.is_none() {
            self.help(&context);
            if !self.render.blocking() {
                self.room_tone_sheet(&context);
                self.corrections_sheet(&context);
            }
            self.render_windows(&context);
            self.marks_window(&context);
            self.models_window(&context);
            self.diagnostics_window(&context);
            self.storage_window(&context);
            self.jobs_window(&context);
            self.youtube_window(&context);
        }
        // Drawn even over Trim, whose close question it may be asking.
        self.recovery_windows(&context);
        if input_scope
            != (
                self.pane,
                self.view,
                self.selected_beat.clone(),
                self.selected_source.clone(),
                self.selected_sound.clone(),
            )
            || context.memory(|m| {
                TEXT_INPUT_IDS
                    .iter()
                    .any(|id| m.has_focus(egui::Id::new(id)))
            })
        {
            self.bindings.clear();
        }
        if let Some((action, command)) = text_result {
            if command && action == TextAction::Open {
                self.run_command(&context);
            }
            self.command_open = false;
            self.command_focus_pending = false;
            context.memory_mut(|m| m.request_focus(pane_id(self.pane)));
        }
        self.ensure_visible_pane(&context);
        if !self.command_focus_pending && close_command_on_blur(&context, &mut self.command_open) {
            self.bindings.clear();
        }
        self.reconcile_edit_role();
        self.reconcile_macro_recording();
        self.reconcile_operator();
        self.reconcile_repeat_prefix();
        if !self.bindings.operator_pending() {
            self.operator_target = None;
        }
        if !self.bindings.repeat_pending() {
            self.repeat_prefix_target = None;
        }
        if !self.bindings.trim_pending() {
            self.trim_prefix_target = None;
        }
        if !self.command_open {
            self.trim_command_target = None;
        }
        if footer_mode
            != (
                self.command_open,
                self.camera.is_some(),
                self.gain.is_some(),
                self.splice.is_some(),
                self.slip.is_some(),
                self.trim.is_some(),
                self.youtube_footer_key(&context),
            )
        {
            context.request_discard("workspace footer mode changed after input");
        }
        if compact_sounds_heading != self.compact_sounds_heading(&context)
            || sounds_in_heading != self.sounds_in_heading(&context)
        {
            context.request_discard("Sounds heading changed placement");
        }
        if !context.will_discard() {
            if self.trim.is_none() {
                if self.render.requested {
                    self.begin_render(&context);
                }
                self.dispatch_render_history(&context);
            }
            self.dispatch_ai_audition();
            self.schedule_playback_picture();
            let main_busy = self.presentation.loading() || self.presentation.needs_render();
            self.thumbnails.pump(
                &context,
                self.workspace.as_ref(),
                &mut self.renderer,
                main_busy,
            );
            self.dispatch_waiting_repeat(&context);
            self.dispatch_gain_proposal(&context);
            self.dispatch_splice(&context);
            self.dispatch_slip(&context);
            if !self.close_pending {
                self.dispatch_trim(&context);
            }
        }
        if first_pass && let Some(frames) = self.smoke_frames.as_mut() {
            *frames += 1;
            if *frames >= 3 {
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                context.request_repaint();
            }
        }
        self.repair_focus(&context);
        self.describe_panes(&context);
        // Last: every focus change of this pass is final, and AccessKit
        // rejects a tree whose focus names a control that was not drawn.
        accessibility::guard_focus(&context);
        self.service.set_preview_active(
            self.camera.is_some()
                || self.camera_pending.is_some()
                || self.gain.is_some()
                || self.room_tone.is_some()
                || self.splice.is_some()
                || self.slip.is_some()
                || self.trim.is_some(),
        );
    }
    fn on_exit(&mut self) {
        self.invalidate_trim_media(true);
        self.trim = None;
        self.trim_command_target = None;
        self.trim_prefix_target = None;
        self.trim_abandon.clear();
        self.playback.shutdown();
        // Release every queued or paused job first, so each job's own
        // shutdown below joins a thread that is already draining.
        self.service.jobs().shutdown();
        self.service.shutdown();
        self.worker.shutdown();
        self.endpoint_worker.shutdown();
        self.thumbnails.shutdown();
        self.transcription.shutdown();
        // Cancel a model install; partial bytes stay for Resume.
        self.models.manager.shutdown(Duration::from_secs(3));
        self.shots.shutdown();
        self.proxies.shutdown();
        // Cancel and drain the import so no helper or private files outlive it.
        self.youtube.jobs.shutdown(Duration::from_secs(10));
        // No GPU wait on the UI. Submitted targets keep their queue callback
        // owner if shutdown ends the display before its final frame can drain.
        self.junction_pictures.clear();
        self.splice = None;
        self.slip = None;
        self.forget_target();
        // Only a drained writer is a clean exit. A system Quit can reach here
        // without the window's close path; leave the journal open otherwise.
        self.record_clean_exit_after_drain(Duration::from_secs(3));
        self.exited.set(true);
    }
}
impl Drop for DeadpanApp {
    fn drop(&mut self) {
        self.invalidate_trim_media(true);
        self.trim = None;
        self.trim_command_target = None;
        self.trim_prefix_target = None;
        self.trim_abandon.clear();
        self.playback.shutdown();
        self.service.shutdown();
        // A no-op after on_exit; otherwise drain the import before its files go.
        self.youtube.jobs.shutdown(Duration::from_secs(10));
        self.worker.shutdown();
        self.endpoint_worker.shutdown();
        self.junction_pictures.clear();
        self.splice = None;
        self.slip = None;
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
        NodeKind::Retime {
            purpose: deadpan_core::RetimePurpose::Partition,
            ..
        } => "Fragment".into(),
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

fn sound_action_allowed(action: Action) -> bool {
    matches!(
        action,
        Action::New
            | Action::NewFromUrl
            | Action::Open
            | Action::Import
            | Action::Render
            | Action::Undo
            | Action::Redo
            | Action::Playback
            | Action::Audition
            | Action::Beat { .. }
            | Action::Pane { .. }
            | Action::Search
            | Action::Command
            | Action::Help
            | Action::Escape
            | Action::Invalid(_)
            | Action::OfferInsert
            | Action::Sound(navigation::SoundAction::Place | navigation::SoundAction::Focus)
    )
}

fn pane_name(pane: Pane) -> &'static str {
    match pane {
        Pane::Sources => "Sources",
        Pane::Viewer => "Viewer",
        Pane::Sequence => "Beats",
        Pane::Inspector => "Inspector",
        Pane::Sounds => "Placed sounds",
    }
}

fn pane_heading(ui: &mut egui::Ui, title: &str, focused: bool) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(style::section_title(title, focused));
        if focused {
            style::focus_pill(ui);
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

/// One read-only inspector value with a fixed label column, matching
/// `style::value_grid` for inspectors that interleave values with controls.
fn inspector_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(104.0, 18.0), egui::Sense::hover());
        ui.put(
            rect,
            egui::Label::new(egui::RichText::new(label).size(12.0).weak())
                .truncate()
                .halign(egui::Align::Min),
        );
        accessibility::full_text(
            ui.add(egui::Label::new(egui::RichText::new(value).monospace().size(11.5)).truncate()),
            value,
        )
        .on_hover_text(format!("{label}: {value}"));
    });
}

type SourceRow = (AssetId, String, bool);
fn command_text_edit(command: &mut String) -> egui::TextEdit<'_> {
    // The application routes non-IME Enter after final text has been processed.
    // TextEdit's default return key instead surrenders focus unconditionally,
    // including during Preedit, closing command mode before composition ends.
    egui::TextEdit::singleline(command)
        .id(egui::Id::new(COMMAND_ID))
        .return_key(None)
}

fn retain_text_escape(ui: &egui::Ui, id: &str) {
    // egui installs this filter only on an already-focused widget. Initialize
    // it in the same outer frame as focus acquisition; otherwise an immediate
    // Escape surrenders focus before TextEdit can consume that batch's text.
    let id = egui::Id::new(id);
    if ui.memory(|m| m.has_focus(id) && !m.had_focus_last_frame(id)) {
        ui.ctx()
            .request_discard("initialize text input focus filter");
    }
    // Keep focus through the input pass so the application can leave text mode
    // after the TextEdit has processed this frame's final text/IME events.
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            egui::EventFilter {
                escape: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                tab: false,
            },
        )
    });
}
#[cfg(feature = "ui-harness")]
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
        key_labels::keycap(ui, key);
        ui.label(description);
    });
}

/// One registered action: its keys and verbs, name, help, and where it works.
fn help_spec_row(
    ui: &mut egui::Ui,
    bindings: &Bindings,
    spec: &registry::Spec,
    here: registry::Contexts,
) {
    let keys = registry::key_text(spec, bindings);
    let help = registry::render(spec.help, bindings);
    help_spec_row_with(ui, &keys, &help, spec, here);
}

fn help_spec_row_with(
    ui: &mut egui::Ui,
    keys: &str,
    help: &str,
    spec: &registry::Spec,
    here: registry::Contexts,
) {
    ui.horizontal_wrapped(|ui| {
        if !keys.is_empty() {
            key_labels::keycap(ui, keys);
        }
        // Name the contexts unless "works here" already covers every editor
        // context, and name macro/dot behaviour only for what can be recorded.
        let here_now = spec.contexts.intersects(here);
        let mut notes = Vec::new();
        if here_now {
            notes.push("Works here".to_owned());
        }
        if !(here_now && spec.contexts.contains(registry::Contexts::EDITOR)) {
            notes.push(spec.contexts.label());
        }
        if spec.replay != registry::Replay::Ignored {
            notes.push(spec.replay.label().to_owned());
        }
        notes.push(format!("headless {}", spec.headless.parity.label()));
        // One label per row, so a screen reader hears name, help and notes
        // together after the keycap.
        let body = egui::TextStyle::Body.resolve(ui.style());
        let small = egui::FontId::new(11.0, body.family.clone());
        let mut job = egui::text::LayoutJob::default();
        job.append(
            spec.name,
            0.0,
            egui::TextFormat {
                font_id: body.clone(),
                color: ui.visuals().strong_text_color(),
                ..Default::default()
            },
        );
        job.append(
            help,
            8.0,
            egui::TextFormat {
                font_id: body,
                color: ui.visuals().text_color(),
                ..Default::default()
            },
        );
        job.append(
            &notes.join(" · "),
            8.0,
            egui::TextFormat {
                font_id: small,
                color: if here_now {
                    style::LAVENDER
                } else {
                    ui.visuals().weak_text_color()
                },
                ..Default::default()
            },
        );
        ui.label(job).on_hover_text(format!(
            "Headless: {}, {} (docs/PARITY.md#{})",
            spec.headless.parity.label(),
            spec.headless.form,
            spec.headless.anchor
        ));
    });
}

/// Matching actions, best first: an exact key, then a verb, a name, any text.
fn help_search_results(
    ui: &mut egui::Ui,
    bindings: &Bindings,
    query: &str,
    here: registry::Contexts,
) {
    let mut matches: Vec<_> = registry::SPECS
        .iter()
        .enumerate()
        .filter_map(|(index, spec)| {
            let keys = registry::key_text(spec, bindings);
            let help = registry::render(spec.help, bindings);
            registry::search_rank(spec, &keys, &help, query)
                .map(|rank| (rank, spec.section, index, spec, keys, help))
        })
        .collect();
    matches.sort_by_key(|(rank, section, index, ..)| (*rank, *section, *index));
    let query = query.trim();
    // One live region with a fixed id: each changed count is announced.
    let count = ui
        .push_id("keys-search-count", |ui| {
            if matches.is_empty() {
                ui.label(format!(
                    "No action matches “{query}”. Try a key such as dd, a command such as :hold, or a word such as pause."
                ))
            } else {
                ui.label(style::section_title(
                    &format!(
                        "{} {} FOR “{query}”",
                        matches.len(),
                        if matches.len() == 1 { "MATCH" } else { "MATCHES" }
                    ),
                    true,
                ))
            }
        })
        .inner;
    accessibility::live(&count, false);
    for (_, section, _, spec, keys, help) in &matches {
        help_spec_row_with(
            ui,
            keys,
            &format!("{help} ({})", section.title().to_lowercase()),
            spec,
            here,
        );
    }
}
/// The completion row for a partially typed verb: up to four usages, and
/// the complete list for accessibility and hover.
fn command_completion_text(command: &str) -> Option<(String, String)> {
    let matches = navigation::command::completions(command);
    if matches.is_empty() {
        return None;
    }
    let mut shown = matches
        .iter()
        .take(4)
        .copied()
        .collect::<Vec<_>>()
        .join("  ·  ");
    if matches.len() > 4 {
        shown.push_str(&format!("  ·  +{} more", matches.len() - 4));
    }
    Some((shown, format!("Matching commands: {}", matches.join(" · "))))
}

fn pane_id(pane: Pane) -> egui::Id {
    egui::Id::new(match pane {
        Pane::Sources => "sources-pane",
        Pane::Viewer => "viewer-pane",
        Pane::Sequence => "sequence-pane",
        Pane::Inspector => "inspector-pane",
        Pane::Sounds => "placed-sounds-pane",
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
fn focus_command_for_frame(context: &egui::Context, pending: &mut bool) {
    if std::mem::take(pending) {
        context.memory_mut(|m| m.request_focus(egui::Id::new(COMMAND_ID)));
    }
}
fn control_owns_activation(context: &egui::Context, key: egui::Key) -> bool {
    matches!(key, egui::Key::Space | egui::Key::Enter) && native_control_focused(context)
}

fn native_control_focused(context: &egui::Context) -> bool {
    context
        .memory(|memory| memory.focused())
        .is_some_and(|focused| {
            [
                Pane::Sources,
                Pane::Viewer,
                Pane::Sequence,
                Pane::Inspector,
                Pane::Sounds,
            ]
            .into_iter()
            .all(|pane| focused != pane_id(pane))
        })
}

fn text_input_active(context: &egui::Context, command_open: bool) -> bool {
    command_open
        || context.memory(|m| {
            TEXT_INPUT_IDS
                .iter()
                .any(|id| m.has_focus(egui::Id::new(id)))
        })
}
fn pointer_focus_transition(events: &[egui::Event]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, egui::Event::PointerButton { .. }))
}
fn pane_focus(ui: &egui::Ui, pane: Pane, rect: egui::Rect, label: &str) -> egui::Response {
    let response = ui.interact(rect, pane_id(pane), egui::Sense::click());
    accessibility::record_drawn_pane(ui.ctx(), pane);
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
    #[test]
    fn completion_row_truncates_to_four_and_names_every_match_accessibly() {
        assert_eq!(super::command_completion_text("caption hi"), None);
        let (shown, full) = super::command_completion_text(":cap").unwrap();
        assert_eq!(shown, ":caption TEXT [at=top|center] [delay=4f]");
        assert!(full.ends_with(":caption TEXT [at=top|center] [delay=4f]"));
        let (shown, full) = super::command_completion_text("s").unwrap();
        let total = crate::navigation::command::completions("s").len();
        assert!(total > 4);
        assert!(shown.ends_with(&format!("+{} more", total - 4)));
        assert_eq!(full.matches(" · ").count(), total - 1);
    }

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
                escalation: None,
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
        context.enable_accesskit();
        let mut output = context.run_ui(input, draw);
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        assert!(
            tree.nodes.iter().any(|(id, _)| *id == tree.focus),
            "Accessibility focus must name a node emitted in this frame"
        );
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
    fn focused_button_keeps_space_and_return_while_pane_space_controls_playback() {
        for (pane, label) in [
            (Pane::Viewer, "Change duration"),
            (Pane::Sources, "Play sound  Space"),
        ] {
            for key in [egui::Key::Space, egui::Key::Enter] {
                let context = egui::Context::default();
                let mut bindings = Bindings::default();
                let draw = |ui: &mut egui::Ui, focus_button: bool| {
                    let (_, rect) = ui.allocate_space(egui::vec2(100.0, 40.0));
                    pane_focus(ui, pane, rect, "Playback pane");
                    let response = ui.button(label);
                    if focus_button {
                        response.request_focus();
                    }
                    response.clicked()
                };
                run_ui(&context, egui::RawInput::default(), |ui| {
                    draw(ui, true);
                });
                let mut clicked = false;
                let mut action = None;
                run_ui(
                    &context,
                    egui::RawInput {
                        events: vec![key_event(key)],
                        ..Default::default()
                    },
                    |ui| {
                        assert!(control_owns_activation(ui.ctx(), key));
                        if !control_owns_activation(ui.ctx(), key) {
                            action = bindings.key(key, egui::Modifiers::NONE, false, false);
                            ui.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, key);
                            });
                        }
                        clicked |= draw(ui, false);
                    },
                );
                assert!(clicked, "focused button did not receive {key:?}");
                assert!(action.is_none());
                run_ui(&context, egui::RawInput::default(), |ui| {
                    ui.memory_mut(|m| m.request_focus(pane_id(pane)));
                    draw(ui, false);
                });
                run_ui(
                    &context,
                    egui::RawInput {
                        events: vec![key_event(egui::Key::Space)],
                        ..Default::default()
                    },
                    |ui| {
                        assert!(!control_owns_activation(ui.ctx(), egui::Key::Space));
                        assert_eq!(
                            bindings.key(egui::Key::Space, egui::Modifiers::NONE, false, false),
                            Some(Action::Playback)
                        );
                        draw(ui, false);
                    },
                );
            }
        }
    }

    #[test]
    fn sound_catalog_shortcuts_cannot_target_the_retained_timeline_selection() {
        for action in [
            Action::Insert,
            Action::DeleteFrames(1),
            Action::Edit(BeatEdit::Delete),
            Action::Edit(BeatEdit::Split),
            Action::Edit(BeatEdit::Repeat(3)),
            Action::PasteMoment { before: false },
            Action::VisualMoment,
            Action::CopyMoment,
            Action::EnterGroup,
            Action::LeaveGroup,
            Action::First,
            Action::Last,
            Action::Step {
                forward: true,
                count: 1,
            },
            Action::Framing(navigation::FramingAction::EnterCamera),
        ] {
            assert!(
                !sound_action_allowed(action),
                "{action:?} must not target the retained editor selection"
            );
        }
        for action in [
            Action::Playback,
            Action::Audition,
            Action::Beat {
                forward: true,
                count: 1,
            },
            Action::Pane { reverse: false },
            Action::Search,
            Action::Command,
            Action::Undo,
            Action::Redo,
            Action::Escape,
        ] {
            assert!(
                sound_action_allowed(action),
                "{action:?} remains available in the sound catalog"
            );
        }
    }

    #[test]
    fn sound_catalog_keys_use_production_bindings_and_honor_native_text() {
        use egui::{Key, Modifiers};
        for keys in [
            vec![Key::D, Key::D],
            vec![Key::S],
            vec![Key::R, Key::R],
            vec![Key::Comma, Key::F],
            vec![Key::P],
        ] {
            let mut bindings = Bindings::default();
            let action = keys
                .into_iter()
                .filter_map(|key| bindings.key(key, Modifiers::NONE, false, false))
                .last()
                .unwrap();
            assert!(
                !sound_action_allowed(action),
                "Sound focus must suppress {action:?}"
            );
        }
        for (key, forward) in [(Key::J, true), (Key::K, false)] {
            let mut bindings = Bindings::default();
            assert_eq!(bindings.key(Key::Num2, Modifiers::NONE, false, false), None);
            let action = bindings.key(key, Modifiers::NONE, false, false).unwrap();
            assert_eq!(action, Action::Beat { forward, count: 2 });
            assert!(sound_action_allowed(action));
        }
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            let mut bindings = Bindings::default();
            for key in [Key::J, Key::K, Key::Space, Key::D, Key::D] {
                assert_eq!(bindings.key(key, Modifiers::NONE, text, ime), None);
            }
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn focused_control_keeps_shift_space_and_pane_shift_space_loops_selection() {
        let context = egui::Context::default();
        let mut bindings = Bindings::default();
        for focus_control in [true, false] {
            let draw = |ui: &mut egui::Ui| {
                let (_, rect) = ui.allocate_space(egui::vec2(100.0, 40.0));
                pane_focus(ui, Pane::Viewer, rect, "Viewer");
                let button = ui.button("Change duration");
                if focus_control {
                    button.request_focus();
                } else {
                    ui.memory_mut(|m| m.request_focus(pane_id(Pane::Viewer)));
                }
            };
            run_ui(&context, egui::RawInput::default(), draw);
            let event = egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            };
            run_ui(
                &context,
                egui::RawInput {
                    events: vec![event],
                    ..Default::default()
                },
                |ui| {
                    let owned = control_owns_activation(ui.ctx(), egui::Key::Space);
                    assert_eq!(owned, focus_control);
                    let action = (!owned)
                        .then(|| {
                            bindings.key(egui::Key::Space, egui::Modifiers::SHIFT, false, false)
                        })
                        .flatten();
                    assert_eq!(action, (!focus_control).then_some(Action::Audition));
                    draw(ui);
                },
            );
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
    fn command_composition_keeps_focus_through_enter_and_layout_retries() {
        let context = egui::Context::default();
        context.options_mut(|options| options.max_passes = 3.try_into().unwrap());
        let mut command = String::new();
        let mut open = true;
        let mut composing = false;
        let mut submissions = 0;
        run_ui(&context, egui::RawInput::default(), |ui| {
            ui.memory_mut(|memory| memory.request_focus(egui::Id::new(COMMAND_ID)));
            ui.add(command_text_edit(&mut command));
            retain_text_escape(ui, COMMAND_ID);
        });
        for (events, should_submit) in [
            (
                vec![
                    egui::Event::Ime(egui::ImeEvent::Preedit {
                        text: "sound-silence".into(),
                        active_range_chars: Some(0..13),
                    }),
                    key_event(egui::Key::Enter),
                ],
                false,
            ),
            // Composition remains active without another Preedit in this batch.
            (vec![key_event(egui::Key::Enter)], false),
            (
                vec![
                    egui::Event::Ime(egui::ImeEvent::Commit("sound-silence".into())),
                    key_event(egui::Key::Enter),
                ],
                false,
            ),
            (vec![key_event(egui::Key::Enter)], true),
        ] {
            let mut passes = 0;
            run_ui(
                &context,
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    passes += 1;
                    let events = ui.input(|input| input.events.clone());
                    help_scroll::observe_composition(&events, &mut composing);
                    let ime = composing
                        || events
                            .iter()
                            .any(|event| matches!(event, egui::Event::Ime(_)));
                    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                    let action = enter
                        .then(|| {
                            navigation::text_action(
                                egui::Key::Enter,
                                egui::Modifiers::NONE,
                                text_input_active(ui.ctx(), open),
                                ime,
                            )
                        })
                        .flatten();
                    if action.is_some() {
                        ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                        });
                    }
                    if open {
                        ui.add(command_text_edit(&mut command));
                        retain_text_escape(ui, COMMAND_ID);
                    }
                    let heading = ui.label("Sounds");
                    pane_focus(ui, Pane::Sounds, heading.rect, "Placed sounds pane");
                    if action == Some(TextAction::Open) {
                        submissions += 1;
                        open = false;
                        ui.memory_mut(|memory| memory.request_focus(pane_id(Pane::Sounds)));
                    }
                    assert!(
                        !close_command_on_blur(ui.ctx(), &mut open),
                        "IME must not cause a native blur"
                    );
                    if ui.ctx().current_pass_index() < 2 {
                        ui.ctx()
                            .request_discard("exercise command footer layout retry");
                    }
                },
            );
            assert_eq!(passes, 3);
            assert_eq!(
                command, "sound-silence",
                "preedit and commit must reach the native widget exactly once"
            );
            assert_eq!(open, !should_submit);
            assert_eq!(submissions, usize::from(should_submit));
            assert_eq!(
                context.memory(|memory| memory.has_focus(egui::Id::new(COMMAND_ID))),
                !should_submit
            );
        }
    }

    #[test]
    fn parameter_entry_opened_after_footer_draw_receives_next_frame_text() {
        let context = egui::Context::default();
        let mut command = "repeat ".to_owned();
        let mut pending = false;
        run_ui(&context, egui::RawInput::default(), |ui| {
            // The toolbar opens the command after this frame's footer was drawn.
            let heading = ui.label("Sequence");
            pane_focus(ui, Pane::Sequence, heading.rect, "Sequence pane").request_focus();
            pending = true;
            assert!(!ui.memory(|m| m.has_focus(egui::Id::new(COMMAND_ID))));
        });
        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Text("4".into())],
                ..Default::default()
            },
            |ui| {
                focus_command_for_frame(ui.ctx(), &mut pending);
                assert!(ui.memory(|m| m.has_focus(egui::Id::new(COMMAND_ID))));
                ui.add(egui::TextEdit::singleline(&mut command).id(egui::Id::new(COMMAND_ID)));
                retain_text_escape(ui, COMMAND_ID);
            },
        );
        assert_eq!(command, "repeat 4");
        assert!(!pending);
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
