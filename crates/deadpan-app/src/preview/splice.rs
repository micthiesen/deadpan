//! A local placement draft never changes the copied register or committed view.

use deadpan_core::{AudioSample, FrameDuration};
use deadpan_playback::{Snapshot, Window};

use super::*;
use crate::navigation::splice::SpliceKey;
use crate::project::splice::{
    Destination, Prepared, Proposal, ProposalId, ProposalUpdate, SpliceCommitUpdate,
};
use crate::transport::Domain;
use crate::worker::EndpointIdentity;

mod controls;
mod pictures;

const FOCUS: &str = "place-slice-focus";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    In,
    Out,
    Destination,
    Picture,
}

pub(super) struct Draft {
    base: Arc<Workspace>,
    base_audio: Arc<Snapshot>,
    proposal: Proposal,
    prepared: Option<Arc<Prepared>>,
    pending: Option<ProposalId>,
    issued: Option<ProposalId>,
    dirty: bool,
    applying: bool,
    invalidated: bool,
    error: Option<String>,
    seams: Vec<u64>,
    children: Vec<NodeId>,
    slot: Option<usize>,
    destination: u64,
    pub(super) cursor: u64,
    focus: Focus,
    before: bool,
    looping: bool,
    pub(super) position: Option<AudioSample>,
    count: Option<u32>,
    source_frames: u64,
    endpoint_change: u64,
    endpoints_pending: bool,
    endpoints: pictures::Display,
    entry_cursor: u64,
    entry_source: u64,
    entry_pane: Pane,
    focus_pending: bool,
    keys: Vec<SpliceKey>,
    scope_label: String,
}

impl Draft {
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn proposal_for_check(&self) -> &Proposal {
        &self.proposal
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn prepared_for_check(&self) -> Option<&Arc<Prepared>> {
        self.prepared.as_ref()
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn ready_for_check(&self) -> bool {
        self.prepared.is_some()
            && !self.dirty
            && self.pending.is_none()
            && !self.applying
            && !self.invalidated
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn before_for_check(&self) -> bool {
        self.before
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn invalidated_for_check(&self) -> bool {
        self.invalidated
    }

    fn snapshot(&self) -> Option<Arc<Snapshot>> {
        if self.invalidated {
            return None;
        }
        if self.before {
            Some(self.base_audio.clone())
        } else {
            self.prepared
                .as_ref()
                .map(|prepared| prepared.snapshot.clone())
        }
    }

    fn frames(&self) -> u64 {
        if self.before {
            self.base.plan.duration().frames() as u64
        } else {
            self.prepared
                .as_ref()
                .map_or(self.base.plan.duration().frames() as u64, |prepared| {
                    prepared.plan.duration().frames() as u64
                })
        }
    }

    fn endpoint_identity(&self) -> EndpointIdentity {
        EndpointIdentity {
            session: self.proposal.id.session,
            project: self.proposal.id.project.clone(),
            revision: self.proposal.id.base_revision.clone(),
            draft: self.proposal.id.draft,
            change: self.endpoint_change,
            asset: self.proposal.asset.clone(),
            in_frame: SourceFrameId(self.proposal.ordinals.start),
            out_frame: SourceFrameId(self.proposal.ordinals.end),
        }
    }

    fn changed(&mut self, endpoints: bool) -> Result<(), String> {
        self.proposal.id.change = self
            .proposal
            .id
            .change
            .checked_add(1)
            .ok_or("Slice change identities exhausted")?;
        if endpoints {
            self.endpoint_change = self
                .endpoint_change
                .checked_add(1)
                .ok_or("Slice endpoint identities exhausted")?;
            self.endpoints_pending = true;
        }
        self.prepared = None;
        self.error = None;
        self.position = None;
        self.dirty = true;
        self.proposal.destination =
            placement_at(&self.seams, &self.children, self.slot, self.destination)?;
        Ok(())
    }
}

fn placement_at(
    seams: &[u64],
    children: &[NodeId],
    slot: Option<usize>,
    boundary: u64,
) -> Result<Destination, String> {
    if let Some(slot) = slot {
        return Ok(Destination::Slot(slot));
    }
    let index = seams
        .windows(2)
        .position(|pair| pair[0] < boundary && boundary < pair[1])
        .ok_or("Slice destination is outside its captured Sequence")?;
    let target = children
        .get(index)
        .ok_or("Slice destination child is unavailable")?
        .clone();
    let local =
        i64::try_from(boundary - seams[index]).map_err(|_| "Slice local destination overflow")?;
    Ok(Destination::Interior {
        target,
        at: FrameDuration::new(local).map_err(|error| error.to_string())?,
    })
}

impl DeadpanApp {
    pub(super) fn open_splice(&mut self, context: &egui::Context) {
        self.reconcile_moment();
        let captured = (|| {
            if self.view != View::Sequence {
                return Err("Return to Your edit (:sequence) before placing a slice.".to_owned());
            }
            if self.splice.is_some()
                || self.splice_abandon.is_some()
                || self.camera.is_some()
                || self.gain.is_some()
                || self.room_tone.is_some()
                || self.dialogs.is_open()
                || self.render.blocking()
                || self.service.is_busy()
                || self.importing()
            {
                return Err(
                    "Finish the current preview or preparation before placing a slice.".to_owned(),
                );
            }
            let base = self.workspace.clone().ok_or("Open a project first.")?;
            let copied = self
                .moment
                .copied
                .as_ref()
                .ok_or("Copy an Original range first: :source, v, h/l, y.")?;
            let view = self.sequence_scope.resolve(&base)?;
            let mut seams = Vec::with_capacity(view.children.len() + 1);
            let mut at = view.start;
            seams.push(at);
            for child in view.children {
                let duration = base
                    .plan
                    .node_duration(child)
                    .ok_or("Missing destination beat duration")?
                    .frames();
                at = at
                    .checked_add(u64::try_from(duration).map_err(|_| "Negative beat duration")?)
                    .ok_or("Slice destination overflow")?;
                seams.push(at);
            }
            let parent = view.owner.clone();
            let children = view.children.to_vec();
            let source_frames = base
                .sources
                .get(&copied.identity.asset)
                .and_then(|source| source.video_index.as_ref())
                .ok_or("The copied Original has no qualified pictures.")?
                .frames()
                .len() as u64;
            Ok((base, copied.clone(), parent, seams, children, source_frames))
        })();
        let (base, copied, parent, seams, children, source_frames) = match captured {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(token) = self.next_serial() else {
            return;
        };
        let slot = seams.iter().position(|at| *at == self.sequence_cursor);
        let destination = match placement_at(&seams, &children, slot, self.sequence_cursor) {
            Ok(destination) => destination,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let proposal = Proposal {
            id: ProposalId {
                session: base.session,
                project: base.document.project_id().clone(),
                base_revision: base.document.revision_id().clone(),
                draft: token,
                change: 1,
            },
            asset: copied.identity.asset,
            qualification: copied.identity.qualification,
            ordinals: copied.ordinals,
            scope: self.sequence_scope.clone(),
            parent,
            destination,
        };
        self.stop_playback();
        self.cancel_repeats("Place slice opened");
        self.bindings.clear();
        self.service.set_preview_active(true);
        self.splice = Some(Draft {
            base_audio: Arc::new(base.playback_snapshot()),
            base,
            proposal,
            prepared: None,
            pending: None,
            issued: None,
            dirty: true,
            applying: false,
            invalidated: false,
            error: None,
            seams,
            children,
            slot,
            destination: self.sequence_cursor,
            cursor: self.sequence_cursor,
            focus: Focus::Destination,
            before: false,
            looping: false,
            position: None,
            count: None,
            source_frames,
            endpoint_change: 1,
            endpoints_pending: true,
            endpoints: pictures::Display::new(self.render_state.clone()),
            entry_cursor: self.sequence_cursor,
            entry_source: self.source_cursor,
            entry_pane: self.pane,
            focus_pending: true,
            keys: Vec::new(),
            scope_label: if self.scope_labels.is_empty() {
                "Your edit".into()
            } else {
                format!("Your edit / {}", self.scope_labels.join(" / "))
            },
        });
        self.pane = Pane::Viewer;
        self.error = None;
        self.message = None;
        self.request_picture_for_transport_at(false, None, None);
        context.request_repaint();
        context.request_discard("Place slice opened");
    }

    pub(super) fn receive_splice(
        &mut self,
        update: Option<ProposalUpdate>,
        commit: Option<SpliceCommitUpdate>,
    ) {
        if let Some(commit) = commit
            && self
                .splice
                .as_ref()
                .is_some_and(|draft| draft.applying && draft.proposal.id == commit.id)
        {
            match commit.result {
                Ok(_) => {
                    self.stop_playback();
                    self.splice = None;
                    self.endpoint_worker.clear();
                    self.bindings.clear();
                }
                Err(error) => {
                    if let Some(draft) = &mut self.splice {
                        draft.applying = false;
                        draft.error = Some(error);
                    }
                }
            }
        }
        if let Some(update) = update
            && let Some(draft) = &mut self.splice
            && draft.pending.as_ref() == Some(&update.id)
        {
            draft.pending = None;
            if draft.proposal.id == update.id && !draft.invalidated {
                let result = update.result.and_then(|prepared| {
                    prepared
                        .snapshot
                        .validate_proposed_base(prepared.base.session, &prepared.base.document)
                        .map_err(|error| error.to_string())?;
                    if prepared.base.session != update.id.session
                        || prepared.base.document.project_id() != &update.id.project
                        || prepared.base.document.revision_id() != &update.id.base_revision
                        || prepared
                            .plan
                            .node_duration(&prepared.node)
                            .is_none_or(|duration| {
                                duration.frames()
                                    != prepared.range.end().0 - prepared.range.start().0
                            })
                    {
                        return Err(
                            "Slice preparation returned a different captured destination.".into(),
                        );
                    }
                    Ok(prepared)
                });
                match result {
                    Ok(prepared) => {
                        draft.base = prepared.base.clone();
                        draft.base_audio = Arc::new(prepared.base.playback_snapshot());
                        draft.prepared = Some(prepared);
                        draft.error = None;
                    }
                    Err(error) => {
                        draft.prepared = None;
                        draft.error = Some(error);
                    }
                }
                self.request_picture_for_transport_at(false, None, None);
            }
        }
    }

    pub(super) fn reconcile_splice(&mut self, context: &egui::Context) {
        let stale = self.splice.as_ref().is_some_and(|draft| {
            !draft.invalidated
                && self.workspace.as_ref().is_none_or(|workspace| {
                    workspace.session != draft.proposal.id.session
                        || workspace.document.revision_id() != &draft.proposal.id.base_revision
                })
        });
        if stale {
            self.stop_playback();
            self.worker.cancel();
            self.presentation.invalidate_pending();
            self.endpoint_worker.clear();
            if let Some(draft) = &mut self.splice {
                draft.invalidated = true;
                draft.prepared = None;
                draft.dirty = false;
                draft.error = Some("The saved edit changed. Cancel and open Place slice again to capture a new destination.".into());
                self.splice_abandon = draft.issued.clone();
            }
            context.request_repaint();
        }
        if let Some(reply) = self.endpoint_worker.take_reply()
            && let Some(draft) = &mut self.splice
            && !draft.invalidated
            && reply.identity == draft.endpoint_identity()
        {
            draft.endpoints.receive(reply);
        }
    }

    pub(super) fn dispatch_splice(&mut self, context: &egui::Context) {
        if self.service.is_busy() {
            return;
        }
        if let Some(id) = self.splice_abandon.clone() {
            if self
                .service
                .submit(ProjectRequest::AbandonSplice(id))
                .is_ok()
            {
                self.splice_abandon = None;
            }
            return;
        }
        let Some(draft) = &mut self.splice else {
            return;
        };
        if draft.invalidated || draft.applying {
            return;
        }
        if draft.endpoints_pending {
            self.endpoint_worker
                .submit(draft.endpoint_identity(), draft.base.clone());
            draft.endpoints_pending = false;
        }
        if draft.dirty && draft.pending.is_none() {
            match self
                .service
                .submit(ProjectRequest::PrepareSplice(draft.proposal.clone()))
            {
                Ok(()) => {
                    draft.pending = Some(draft.proposal.id.clone());
                    draft.issued = Some(draft.proposal.id.clone());
                    draft.dirty = false;
                }
                Err(error) => {
                    draft.error = Some(error);
                }
            }
            context.request_repaint();
        }
    }

    pub(super) fn splice_picture_work(&self, frame: Option<u64>) -> Option<Work> {
        let draft = self.splice.as_ref()?;
        if draft.invalidated
            || self.workspace.as_ref().is_none_or(|workspace| {
                workspace.session != draft.proposal.id.session
                    || workspace.document.revision_id() != &draft.proposal.id.base_revision
            })
        {
            return None;
        }
        if frame.is_none() && matches!(draft.focus, Focus::In | Focus::Out) {
            let ordinal = if draft.focus == Focus::In {
                draft.proposal.ordinals.start
            } else {
                draft.proposal.ordinals.end - 1
            };
            return Some(Work::Project {
                workspace: draft.base.clone(),
                view: ProjectView::Source {
                    asset: draft.proposal.asset.clone(),
                    frame: SourceFrameId(ordinal),
                },
            });
        }
        let at = frame
            .unwrap_or(draft.cursor)
            .min(draft.frames().saturating_sub(1));
        let view = ProjectView::Sequence {
            frame: ProjectFrame(i64::try_from(at).ok()?),
        };
        if draft.before || draft.prepared.is_none() {
            Some(Work::Project {
                workspace: draft.base.clone(),
                view,
            })
        } else {
            Some(Work::Proposed {
                base: draft.base.clone(),
                snapshot: draft.prepared.as_ref()?.snapshot.clone(),
                view,
            })
        }
    }

    fn close_splice(&mut self, context: &egui::Context) {
        if self.splice.as_ref().is_some_and(|draft| draft.applying) {
            return;
        }
        self.stop_playback();
        if let Some(draft) = self.splice.take() {
            self.splice_abandon = draft.issued;
            if !draft.invalidated {
                self.sequence_cursor = draft.entry_cursor;
                self.source_cursor = draft.entry_source;
                self.pane = draft.entry_pane;
            }
        }
        self.endpoint_worker.clear();
        self.bindings.clear();
        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
        self.request_picture(false);
        context.request_discard("Place slice closed");
        context.request_repaint();
    }

    fn splice_action(&mut self, action: SpliceKey, context: &egui::Context) {
        if action == SpliceKey::Cancel {
            self.close_splice(context);
            return;
        }
        if self
            .splice
            .as_ref()
            .is_none_or(|draft| draft.invalidated || draft.applying)
        {
            return;
        }
        if action == SpliceKey::Apply {
            let Some(draft) = &mut self.splice else {
                return;
            };
            if draft.prepared.is_none() || draft.dirty || draft.pending.is_some() {
                return;
            }
            match self
                .service
                .submit(ProjectRequest::CommitSplice(draft.proposal.id.clone()))
            {
                Ok(()) => {
                    draft.applying = true;
                    self.stop_playback();
                }
                Err(error) => {
                    draft.error = Some(error);
                }
            }
            return;
        }
        if matches!(
            action,
            SpliceKey::Play | SpliceKey::Loop | SpliceKey::Compare
        ) {
            let ready = self.splice.as_ref().is_some_and(|draft| {
                draft.prepared.is_some() && !draft.dirty && draft.pending.is_none()
            });
            if !ready && !(action == SpliceKey::Play && self.transport.is_some()) {
                return;
            }
            self.splice_audition(action);
            return;
        }
        let Some(draft) = &mut self.splice else {
            return;
        };
        if let SpliceKey::Count(digit) = action {
            draft.count = draft
                .count
                .unwrap_or(0)
                .checked_mul(10)
                .and_then(|count| count.checked_add(digit))
                .filter(|count| *count <= 1_000_000);
            if draft.count.is_none() {
                draft.error = Some("Count exceeds 1,000,000; no placement change made.".into());
            }
            return;
        }
        let count = u64::from(draft.count.take().unwrap_or(1).max(1));
        draft.looping = false;
        let mut changed = false;
        let mut endpoints = false;
        match action {
            SpliceKey::In => {
                draft.focus = Focus::In;
            }
            SpliceKey::Out => {
                draft.focus = Focus::Out;
            }
            SpliceKey::Destination => {
                draft.focus = Focus::Destination;
                draft.cursor = draft.destination;
            }
            SpliceKey::Picture => {
                draft.focus = Focus::Picture;
            }
            SpliceKey::Step(forward) => {
                let advance = |value: u64, min: u64, max: u64| {
                    if forward {
                        value.saturating_add(count).min(max)
                    } else {
                        value.saturating_sub(count).max(min)
                    }
                };
                match draft.focus {
                    Focus::In => {
                        draft.proposal.ordinals.start = advance(
                            draft.proposal.ordinals.start,
                            0,
                            draft.proposal.ordinals.end - 1,
                        );
                        changed = true;
                        endpoints = true;
                    }
                    Focus::Out => {
                        draft.proposal.ordinals.end = advance(
                            draft.proposal.ordinals.end,
                            draft.proposal.ordinals.start + 1,
                            draft.source_frames,
                        );
                        changed = true;
                        endpoints = true;
                    }
                    Focus::Destination => {
                        let destination = advance(
                            draft.destination,
                            draft.seams[0],
                            *draft.seams.last().expect("scope has a boundary"),
                        );
                        // Distinct empty children share a numeric boundary.
                        // A clamped motion must retain the explicitly chosen slot.
                        if destination != draft.destination {
                            draft.slot = draft.seams.iter().position(|at| *at == destination);
                        }
                        draft.destination = destination;
                        draft.cursor = draft.destination;
                        changed = true;
                    }
                    Focus::Picture => {
                        draft.cursor = advance(draft.cursor, 0, draft.frames().saturating_sub(1));
                        draft.position = None;
                    }
                }
            }
            SpliceKey::Boundary(forward) => {
                let slot = if let Some(slot) = draft.slot {
                    if forward {
                        slot.saturating_add(count as usize)
                            .min(draft.seams.len() - 1)
                    } else {
                        slot.saturating_sub(count as usize)
                    }
                } else if forward {
                    draft
                        .seams
                        .iter()
                        .position(|at| *at > draft.destination)
                        .unwrap_or(draft.seams.len() - 1)
                        .saturating_add((count - 1) as usize)
                        .min(draft.seams.len() - 1)
                } else {
                    draft
                        .seams
                        .iter()
                        .rposition(|at| *at < draft.destination)
                        .unwrap_or(0)
                        .saturating_sub((count - 1) as usize)
                };
                draft.destination = draft.seams[slot];
                draft.slot = Some(slot);
                draft.cursor = draft.destination;
                draft.focus = Focus::Destination;
                changed = true;
            }
            _ => {}
        }
        if changed && let Err(error) = draft.changed(endpoints) {
            draft.error = Some(error);
            draft.invalidated = true;
        }
        self.stop_playback();
        self.request_picture_for_transport_at(false, None, None);
        context.memory_mut(|memory| memory.request_focus(egui::Id::new(FOCUS)));
        context.request_repaint();
    }

    fn splice_audition(&mut self, action: SpliceKey) {
        let running = self.transport.is_some();
        let heard = self
            .transport
            .as_ref()
            .and_then(|run| run.content_sample().ok());
        self.stop_playback();
        let context = self.audition_context;
        let Some(draft) = &mut self.splice else {
            return;
        };
        if let Some(heard) = heard {
            draft.position = Some(heard);
        }
        if action == SpliceKey::Compare {
            draft.before = !draft.before;
        }
        if action == SpliceKey::Loop {
            draft.looping = true;
        }
        draft.focus = Focus::Picture;
        let ready = (|| {
            let snapshot = draft
                .snapshot()
                .ok_or("Wait for the current slice proposal.")?;
            let prepared = draft
                .prepared
                .as_ref()
                .ok_or("Wait for the current slice proposal.")?;
            let domain = Domain::Sequence {
                rate: snapshot.document.presentation_basis().frame_rate,
                frames: i64::try_from(draft.frames())
                    .map_err(|_| "Slice preview duration overflow")?,
            };
            let end = domain.end()?;
            let start = domain
                .sample_at_boundary(prepared.range.start().0 as u64)?
                .0
                .saturating_sub(context.lead.0)
                .max(0);
            let proposed_end = Domain::Sequence {
                rate: snapshot.document.presentation_basis().frame_rate,
                frames: prepared.plan.duration().frames(),
            }
            .sample_at_boundary(prepared.range.end().0 as u64)?;
            let finish = proposed_end.0.saturating_add(context.follow.0).min(end.0);
            let looping = draft.looping;
            let window = Window::new(AudioSample(start.min(end.0)), AudioSample(finish), looping)
                .map_err(|error| error.to_string())?;
            let at = if action == SpliceKey::Loop {
                window.start()
            } else {
                draft.position.unwrap_or(window.start())
            };
            let at = if at < window.start() || at >= window.end() {
                window.start()
            } else {
                at
            };
            Ok::<_, String>((snapshot, domain, window, at))
        })();
        let should_play = action == SpliceKey::Loop
            || action == SpliceKey::Play && !running
            || action == SpliceKey::Compare && running;
        match ready {
            Ok((snapshot, domain, window, at)) if should_play => {
                self.start_snapshot_playback(snapshot, domain, window, at);
                if let Some(draft) = &mut self.splice {
                    draft.error = self.error.take();
                }
            }
            Ok(_) => {
                if let Some(draft) = &mut self.splice {
                    draft.error = None;
                }
            }
            Err(error) => {
                if let Some(draft) = &mut self.splice {
                    draft.error = Some(error);
                }
            }
        }
        if self.transport.is_none() {
            self.request_picture_for_transport_at(false, None, None);
        }
    }
}
