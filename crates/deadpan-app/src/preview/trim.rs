//! One entry-bound Trim draft, independent inspection and a retained save receipt.

use std::collections::VecDeque;

use deadpan_core::{
    AudioSample, ProjectId, RevisionId, SourceTrimControl, SourceTrimEdge, SourceTrimIntent,
};

use super::*;
use crate::navigation::trim::{TrimInput, TrimKey};
use crate::project::trim::{Event, ProposalId, ProposalUpdate, Target};
use crate::worker::JunctionSide;

mod controls;
#[cfg(feature = "ui-harness")]
mod harness;
mod input;
mod inspection;
mod waveform;

#[cfg(test)]
mod tests;

const FOCUS: &str = "trim-preview-focus";
const AMOUNT: &str = "trim-preview-amount";

#[derive(Clone)]
pub(super) struct Capture {
    target: Target,
    source_cursor: u64,
    selected_source: Option<AssetId>,
    pane: Pane,
    view: View,
    selection: edit_range::Selection,
}

impl Capture {
    fn matches_revision(&self, session: u64, project: &ProjectId, revision: &RevisionId) -> bool {
        // Initial capture validated the geometry. Authoritative native revisions
        // are immutable and never reused, so UI continuity needs no document
        // walk each frame. Service and media admission still validate fully.
        self.target.session != 0
            && self.target.session == session
            && &self.target.project == project
            && &self.target.base_revision == revision
    }
}

pub(super) struct Draft {
    capture: Capture,
    input: input::Input,
    control: SourceTrimControl,
    slip_edge: SourceTrimEdge,
    side: JunctionSide,
    inspection_serial: u64,
    inspection: Option<inspection::Inspection>,
    applying: Option<ProposalId>,
    apply_requested: Option<ProposalId>,
    waveform: waveform::Display,
    pub(super) position: Option<AudioSample>,
    looping: bool,
    amount: String,
    amount_control: SourceTrimControl,
    amount_dirty: bool,
    accept_amount: bool,
    amount_events: Option<Vec<egui::Event>>,
    focus_pending: bool,
    keys: Vec<TrimKey>,
    label: String,
    scope_label: String,
    error: Option<String>,
}

fn amount(intent: SourceTrimIntent, control: SourceTrimControl) -> i64 {
    match control {
        SourceTrimControl::In => intent.in_frames,
        SourceTrimControl::Out => intent.out_frames,
        SourceTrimControl::Slip => intent.slip_frames,
        SourceTrimControl::Roll => intent.roll_frames,
    }
}

impl Draft {
    fn invalidate_inspection(&mut self) {
        self.inspection = None;
        self.apply_requested = None;
        self.position = None;
        self.looping = false;
        self.error = None;
        if let Some(serial) = self.inspection_serial.checked_add(1) {
            self.inspection_serial = serial;
        } else {
            self.input
                .invalidate("Trim inspection counter exhausted. Cancel and reopen Trim.");
        }
    }

    fn sync_amount(&mut self) {
        if !self.amount_dirty {
            self.amount_control = self.control;
            self.amount = format!("{:+}f", amount(self.input.accepted, self.control));
        }
    }

    fn finish_amount_entry(&mut self) -> bool {
        if self.input.text_error.is_some() || navigation::trim::parse_frames(&self.amount).is_err()
        {
            return false;
        }
        // The input queue already owns the typed intent. Closing the field
        // shows the current control's accepted value without changing that
        // pending prefix or retaining an earlier control's editable buffer.
        self.amount_dirty = false;
        self.sync_amount();
        true
    }

    fn can_apply(&self, pictures: &splice::JunctionDisplay) -> bool {
        self.applying.is_none()
            && self.error.is_none()
            && self.side == JunctionSide::Proposed
            && self
                .input
                .ready()
                .is_some_and(|prepared| prepared.snapshot.is_some())
            && self
                .inspection
                .as_ref()
                .is_some_and(|inspection| pictures.ready_for_apply(&inspection.identity))
    }
}

impl DeadpanApp {
    pub(super) fn capture_trim_target(&self) -> Result<Capture, String> {
        if self.view != View::Sequence
            || matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.event_focused()
            || self.sound_focused()
        {
            return Err("Select a picture beat in Your edit before opening Trim.".into());
        }
        if self.edit_range.active || self.edit_selection() != navigation::EditSelection::None {
            return Err(
                "Trim edits the selected beat. Clear the Visual Edit selection with Esc first."
                    .into(),
            );
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project before Trim.")?;
        let target = Target::capture(
            workspace,
            self.sequence_scope.clone(),
            self.selected_beat.as_ref(),
            ProjectFrame(i64::try_from(self.sequence_cursor).map_err(|error| error.to_string())?),
        )?;
        Ok(Capture {
            target,
            source_cursor: self.source_cursor,
            selected_source: self.selected_source.clone(),
            pane: self.pane,
            view: self.view,
            selection: self.edit_range.clone(),
        })
    }

    fn trim_context_matches(&self, capture: &Capture, entering: bool) -> bool {
        self.workspace.as_ref().is_some_and(|workspace| {
            capture.matches_revision(
                workspace.session,
                workspace.document.project_id(),
                workspace.document.revision_id(),
            )
        }) && self.sequence_scope == capture.target.scope
            && self.selected_beat.as_ref() == Some(&capture.target.node)
            && i64::try_from(self.sequence_cursor).ok() == Some(capture.target.cursor.0)
            && self.source_cursor == capture.source_cursor
            && self.selected_source == capture.selected_source
            && self.view == capture.view
            && self.edit_range == capture.selection
            && !self.sound_focused()
            && !self.event_focused()
            && (!entering || self.pane == capture.pane)
    }

    pub(super) fn open_trim(
        &mut self,
        captured: Option<Result<Capture, String>>,
        preset: TrimInput,
        context: &egui::Context,
    ) {
        // Earlier input in this same native batch may have opened a picker or
        // another modal after the outer keyboard guard already ran.
        if self.close_pending || self.dialogs.is_open() || self.render.blocking() {
            self.error = Some("Finish the current dialog before opening Trim.".into());
            return;
        }
        if self.trim.is_some()
            || self.slip.is_some()
            || self.splice.is_some()
            || self.gain.is_some()
            || self.room_tone.is_some()
        {
            self.error = Some("Finish or cancel the current preview before opening Trim.".into());
            return;
        }
        if !self.trim_abandon.is_empty() {
            self.error =
                Some("The previous Trim is closing. Open Trim again when it finishes.".into());
            return;
        }
        let capture = match captured
            .unwrap_or_else(|| Err("Open Trim again to capture its target.".into()))
        {
            Ok(capture) if self.trim_context_matches(&capture, true) => capture,
            Ok(_) => {
                self.error = Some(
                    "The selected beat, scope or cursor changed. Reopen Trim; no edit was made."
                        .into(),
                );
                return;
            }
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(draft_id) = self.next_serial() else {
            return;
        };
        let label = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.document.nodes().get(&capture.target.node))
            .map_or_else(|| "Selected beat".into(), |node| node.label.clone());
        let mut input = input::Input::new(capture.target.clone(), draft_id);
        // The preset's policy must govern the numerical clamp and preflight.
        input.queue(Event::SetPolicy(preset.intent.policy));
        input.queue(Event::SetAmount {
            control: preset.control,
            frames: amount(preset.intent, preset.control),
        });
        self.stop_playback();
        self.cancel_camera();
        self.cancel_repeats("Trim preview opened");
        self.worker.cancel();
        self.presentation.invalidate_pending();
        self.endpoint_worker.clear();
        self.junction_pictures.clear();
        self.help_open = false;
        self.bindings.clear();
        self.error = None;
        self.message = None;
        self.trim = Some(Draft {
            capture,
            input,
            control: preset.control,
            slip_edge: SourceTrimEdge::In,
            side: JunctionSide::Proposed,
            inspection_serial: 1,
            inspection: None,
            applying: None,
            apply_requested: None,
            waveform: waveform::Display::default(),
            position: None,
            looping: false,
            amount: format!("{:+}f", amount(preset.intent, preset.control)),
            amount_control: preset.control,
            amount_dirty: false,
            accept_amount: false,
            amount_events: None,
            focus_pending: true,
            keys: vec![],
            label,
            error: None,
            scope_label: if self.scope_labels.is_empty() {
                "Your edit".into()
            } else {
                format!("Your edit / {}", self.scope_labels.join(" / "))
            },
        });
        self.pane = Pane::Viewer;
        self.service.set_preview_active(true);
        context.request_discard("Trim preview opened");
        context.request_repaint();
    }

    pub(super) fn invalidate_trim_media(&mut self, clear: bool) {
        self.stop_playback();
        self.worker.cancel();
        self.presentation.invalidate_pending();
        self.endpoint_worker.clear();
        if clear {
            self.junction_pictures.clear();
        } else {
            self.junction_pictures.invalidate();
        }
        if let Some(draft) = &mut self.trim {
            draft.waveform.clear(&self.playback);
            draft.invalidate_inspection();
        }
    }

    pub(super) fn finish_trim_update(&mut self, update: &mut crate::project::ProjectUpdate) {
        if let Some(receipt) = &update.saved_trim
            && self
                .trim
                .as_ref()
                .is_some_and(|draft| draft.applying.as_ref() == Some(&receipt.id))
        {
            let same_project = update.workspace.as_ref().is_some_and(|workspace| {
                workspace.session == receipt.id.session
                    && workspace.document.project_id() == &receipt.id.project
            });
            if same_project
                && update.workspace.as_ref().is_some_and(|workspace| {
                    workspace.document.revision_id() == &receipt.committed.revision
                })
                && update.committed.is_none()
                && self.last_committed.as_ref() != Some(&receipt.committed.revision)
            {
                update.committed = Some(receipt.committed.clone());
            }
            if same_project
                && update.workspace.as_ref().is_some_and(|workspace| {
                    workspace.document.revision_id() == &receipt.id.base_revision
                })
                && let Some(error) = &receipt.refresh_error
            {
                update.message = Some(format!(
                    "Trim saved, but the preview could not refresh: {error}. Reopen the project to view the saved edit."
                ));
            }
            self.invalidate_trim_media(true);
            self.trim = None;
            self.bindings.clear();
            self.trim_picture_pending = true;
        }
        if let Some(commit) = &update.trim_commit
            && let Some(draft) = &mut self.trim
            && draft.applying.as_ref() == Some(&commit.id)
        {
            match &commit.result {
                Ok(_) => {
                    self.invalidate_trim_media(true);
                    self.trim = None;
                    self.bindings.clear();
                    self.trim_picture_pending = true;
                }
                Err(error) => {
                    draft.applying = None;
                    draft.input.error = Some(error.clone());
                    draft.inspection = None;
                    self.junction_pictures.invalidate();
                }
            }
        }
    }

    pub(super) fn receive_trim(&mut self, update: Option<ProposalUpdate>) {
        if let Some(update) = update
            && let Some(draft) = &mut self.trim
            && draft.input.receive(update)
        {
            draft.sync_amount();
            self.invalidate_trim_media(false);
        }
    }

    pub(super) fn reconcile_trim(&mut self, context: &egui::Context) {
        let stale = self.trim.as_ref().is_some_and(|draft| {
            !draft.input.invalidated && !self.trim_context_matches(&draft.capture, false)
        });
        if stale {
            self.invalidate_trim_media(true);
            if let Some(draft) = &mut self.trim {
                self.trim_abandon.extend(draft.input.abandon_ids());
                draft.input.invalidate(
                    "The saved edit or captured selection changed. Cancel and reopen Trim.",
                );
            }
            context.request_repaint();
        }
    }

    pub(super) fn dispatch_trim(&mut self, context: &egui::Context) {
        // Called only after the final layout pass has computed the required
        // picture raster. A keypress never becomes an armed future save.
        if !context.will_discard() {
            self.commit_requested_trim();
        }
        if !self.service.is_busy() {
            if let Some(id) = self.trim_abandon.front().cloned() {
                if self.service.submit(ProjectRequest::AbandonTrim(id)).is_ok() {
                    self.trim_abandon.pop_front();
                }
            } else if let Some(draft) = &mut self.trim
                && draft.applying.is_none()
                && let Some(request) = draft.input.request()
            {
                let id = request.id();
                if let Err(error) = self.service.submit(ProjectRequest::PrepareTrim(request)) {
                    draft.input.submission_failed(&id, error);
                }
                context.request_repaint();
            }
        }
        if let Some(draft) = &mut self.trim
            && draft.applying.is_none()
            && draft.inspection.is_none()
            && let (Some(id), Some(prepared)) = (draft.input.ready_id(), draft.input.ready())
        {
            match inspection::build(
                id,
                prepared,
                draft.side,
                draft.control,
                draft.slip_edge,
                draft.inspection_serial,
                self.audition_context,
            ) {
                Ok(inspection) => {
                    self.junction_pictures.expect(inspection.identity.clone());
                    self.endpoint_worker.submit_junction(
                        inspection.identity.clone(),
                        crate::worker::EditJunctionInput {
                            base: inspection.input.base.clone(),
                            snapshot: inspection.input.snapshot.clone(),
                        },
                    );
                    if self.transport.is_none() {
                        draft.waveform.request(
                            &self.playback,
                            inspection.identity.clone(),
                            &inspection.input,
                            inspection.samples.clone(),
                        );
                    }
                    draft.inspection = Some(inspection);
                    draft.error = None;
                }
                Err(error) => draft.error = Some(error),
            }
        }
        if std::mem::take(&mut self.trim_picture_pending) && self.trim.is_none() {
            self.request_picture(false);
        }
    }

    pub(super) fn receive_trim_media(&mut self) {
        #[cfg(feature = "ui-harness")]
        let reply = self.feedback.take_junction_reply(&self.endpoint_worker);
        #[cfg(not(feature = "ui-harness"))]
        let reply = self.endpoint_worker.take_junction_reply();
        if let Some(reply) = reply
            && self
                .trim
                .as_ref()
                .and_then(|draft| draft.inspection.as_ref())
                .is_some_and(|inspection| inspection.identity == reply.identity)
        {
            self.junction_pictures.receive(reply);
        }
        if let Some(update) = self.playback.poll_edit_waveform()
            && let Some(draft) = &mut self.trim
        {
            draft.waveform.receive(update);
        }
    }

    fn close_trim(&mut self, context: &egui::Context) {
        if self
            .trim
            .as_ref()
            .is_some_and(|draft| draft.applying.is_some())
        {
            return;
        }
        self.invalidate_trim_media(true);
        if let Some(draft) = self.trim.take() {
            // At most two IDs: last dispatched request and last acknowledged prefix.
            self.trim_abandon = VecDeque::from(draft.input.abandon_ids());
            if self
                .workspace
                .as_ref()
                .is_some_and(|workspace| draft.capture.target.validate(workspace).is_ok())
            {
                self.sequence_cursor = u64::try_from(draft.capture.target.cursor.0)
                    .expect("captured nonnegative cursor");
                self.source_cursor = draft.capture.source_cursor;
                self.selected_source = draft.capture.selected_source;
                self.sequence_scope = draft.capture.target.scope;
                self.selected_beat = Some(draft.capture.target.node);
                self.edit_range = draft.capture.selection;
                self.pane = draft.capture.pane;
                self.view = draft.capture.view;
            }
        }
        self.bindings.clear();
        self.trim_picture_pending = true;
        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
        context.request_discard("Trim preview closed");
        context.request_repaint();
    }

    fn trim_action(&mut self, action: TrimKey, context: &egui::Context) {
        if action == TrimKey::Cancel {
            self.close_trim(context);
            return;
        }
        if self
            .trim
            .as_ref()
            .is_none_or(|draft| draft.applying.is_some() || draft.input.invalidated)
        {
            return;
        }
        if matches!(action, TrimKey::Play | TrimKey::Loop) {
            self.trim_audition(action);
            // Pointer activation follows painting the transport caption. A
            // stopped worker need not publish another update to repaint it.
            context.request_repaint();
            return;
        }
        if action == TrimKey::Apply {
            let Some(draft) = &mut self.trim else {
                return;
            };
            if !draft.can_apply(&self.junction_pictures) || self.service.is_busy() {
                return;
            }
            draft.apply_requested = draft.input.ready_id().cloned();
            context.request_repaint();
            return;
        }
        if action == TrimKey::FocusAmount {
            context.memory_mut(|memory| memory.request_focus(egui::Id::new(AMOUNT)));
            return;
        }
        let draft = self.trim.as_mut().expect("retained Trim draft");
        match action {
            TrimKey::Cycle { reverse } => {
                draft.control = navigation::trim::cycle_control(draft.control, reverse);
                draft.slip_edge = SourceTrimEdge::In;
            }
            TrimKey::Nudge(frames) => {
                draft.input.queue(Event::Nudge {
                    control: draft.control,
                    frames,
                });
            }
            TrimKey::TogglePolicy => {
                draft.input.queue(Event::TogglePolicy);
            }
            TrimKey::Compare => {
                draft.side = if draft.side == JunctionSide::Before {
                    JunctionSide::Proposed
                } else {
                    JunctionSide::Before
                };
            }
            TrimKey::In => {
                draft.slip_edge = SourceTrimEdge::In;
                if draft.control != SourceTrimControl::Slip {
                    draft.control = SourceTrimControl::In;
                }
            }
            TrimKey::Out => {
                draft.slip_edge = SourceTrimEdge::Out;
                if draft.control != SourceTrimControl::Slip {
                    draft.control = SourceTrimControl::Out;
                }
            }
            TrimKey::Play
            | TrimKey::Loop
            | TrimKey::Apply
            | TrimKey::Cancel
            | TrimKey::FocusAmount => unreachable!(),
        }
        draft.sync_amount();
        self.invalidate_trim_media(false);
        context.request_discard("Trim input changed");
        context.request_repaint();
    }

    fn trim_audition(&mut self, action: TrimKey) {
        if self.transport.is_some() && action == TrimKey::Play {
            // Preserve the private transport receipt so a late device fault
            // remains visible. Trim owns the exact heard resume position.
            self.pause_playback();
            if let Some(draft) = &mut self.trim {
                draft.waveform.cancel(&self.playback);
            }
            return;
        }
        let heard = self
            .transport
            .as_ref()
            .and_then(|run| run.content_sample().ok());
        self.stop_playback();
        let Some(draft) = &mut self.trim else {
            return;
        };
        if let Some(heard) = heard {
            draft.position = Some(heard);
        }
        draft.waveform.cancel(&self.playback);
        let result = (|| {
            let inspection = draft
                .inspection
                .as_ref()
                .ok_or("Wait for the current Trim proposal.")?;
            let prepared = draft
                .input
                .ready()
                .ok_or("Wait for the accepted Trim values.")?;
            let snapshot = match (draft.side, &prepared.snapshot) {
                (JunctionSide::Proposed, Some(snapshot)) => Arc::clone(snapshot),
                _ => Arc::new(prepared.base.playback_snapshot()),
            };
            if action == TrimKey::Loop {
                draft.looping = true;
            }
            let window = inspection.playback_window(draft.looping)?;
            let start = draft
                .position
                .filter(|at| *at >= window.start() && *at < window.end())
                .filter(|_| action != TrimKey::Loop)
                .unwrap_or(window.start());
            let domain = crate::transport::Domain::Sequence {
                rate: snapshot.document.presentation_basis().frame_rate,
                frames: inspection.duration.frames(),
            };
            Ok::<_, String>((snapshot, domain, window, start))
        })();
        match result {
            Ok((snapshot, domain, window, start)) => {
                self.start_snapshot_playback(snapshot, domain, window, start)
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn commit_requested_trim(&mut self) {
        let Some(draft) = &mut self.trim else {
            return;
        };
        let Some(id) = draft.apply_requested.take() else {
            return;
        };
        if draft.input.ready_id() != Some(&id)
            || !draft.can_apply(&self.junction_pictures)
            || self.service.is_busy()
        {
            return;
        }
        self.stop_playback();
        let draft = self.trim.as_mut().expect("retained Trim draft");
        draft.waveform.cancel(&self.playback);
        match self.service.submit(ProjectRequest::CommitTrim(id.clone())) {
            Ok(()) => {
                draft.applying = Some(id);
                draft.keys.clear();
            }
            Err(error) => draft.error = Some(error),
        }
    }
}
