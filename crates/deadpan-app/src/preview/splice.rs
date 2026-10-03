//! A local placement draft never changes the copied register or committed view.

use deadpan_core::{AudioSample, FrameDuration, FrameRange};
use deadpan_playback::{Snapshot, Window};

use super::*;
use crate::navigation::splice::SpliceKey;
use crate::project::splice::{
    Destination, Operation, Prepared, PreparedMedia, Proposal, ProposalId, ProposalUpdate, Source,
    SpliceCommitUpdate,
};
use crate::transport::Domain;
use crate::worker::{EndpointIdentity, EndpointInput, EndpointReply, EndpointSourceId};

mod comparison;
mod controls;
mod empty;
mod pictures;
pub(super) use pictures::JunctionDisplay;

use comparison::{Comparison, Site, map_comparison};

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
    replacement: Option<FrameRange>,
    replacing: bool,
    site: Site,
    comparison_note: Option<String>,
    pub(super) cursor: u64,
    focus: Focus,
    before: bool,
    looping: bool,
    pub(super) position: Option<AudioSample>,
    count: Option<u32>,
    source_bounds: std::ops::Range<u64>,
    source_view: Option<Arc<crate::project::slice::CopiedView>>,
    endpoint_change: u64,
    endpoints_pending: bool,
    endpoints: pictures::Display,
    entry_cursor: u64,
    entry_source: u64,
    entry_pane: Pane,
    entry_selection: edit_range::Selection,
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
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn error_for_check(&self) -> Option<&str> {
        self.error.as_deref()
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn empty_endpoints_for_check(&self) -> bool {
        self.empty_structure() && !self.endpoints_pending && self.endpoints.empty_for_check()
    }
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn site_for_check(&self) -> &'static str {
        match self.site {
            Site::Removal => "removal",
            Site::Insertion => "insertion",
        }
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

    fn comparison(&self) -> Option<Comparison> {
        let prepared = self.prepared.as_ref()?;
        Some(Comparison::new(
            (prepared.range.start().0, prepared.range.end().0),
            prepared
                .removed
                .map(|range| (range.start().0, range.end().0)),
            prepared.movement.as_ref(),
            self.site,
            self.base.plan.duration().frames(),
            prepared.plan.duration().frames(),
        ))
    }

    fn inspect_site(&mut self, site: Site) {
        self.site = site;
        self.focus = Focus::Picture;
        self.position = None;
        self.comparison_note = None;
        self.cursor = self.comparison().map_or(self.destination, |comparison| {
            comparison.affected.side(self.before).0 as u64
        });
    }

    fn compare(&mut self, context: playback::AuditionContext) -> Result<(), String> {
        let comparison = self
            .comparison()
            .ok_or("Wait for the current slice proposal.")?;
        let rate = self.base.document.presentation_basis().frame_rate;
        let (windows, _) = comparison.windows(rate, context.lead.0, context.follow.0)?;
        let samples = comparison.affected.samples(rate)?;
        let (from, to) = (
            comparison.affected.side(self.before),
            comparison.affected.side(!self.before),
        );
        let current_sample = self.position.map_or_else(
            || {
                rate.audio_boundary(ProjectFrame(self.cursor as i64))
                    .map(|at| at.0)
                    .map_err(|e| e.to_string())
            },
            |at| Ok(at.0),
        )?;
        let current_window = windows.side(self.before);
        let target_window = windows.side(!self.before);
        let outside = current_sample < current_window.0 || current_sample > current_window.1;
        let mapped = if outside {
            to.0
        } else {
            map_comparison(self.cursor as i64, from, to)?
        };
        self.cursor = u64::try_from(mapped).map_err(|_| "Negative comparison frame")?;
        self.position = if let Some(position) = self.position {
            let mapped = if outside {
                samples.side(!self.before).0
            } else {
                map_comparison(
                    position.0,
                    samples.side(self.before),
                    samples.side(!self.before),
                )?
            };
            Some(AudioSample(mapped.clamp(target_window.0, target_window.1)))
        } else {
            None
        };
        self.comparison_note = outside.then(|| {
            "Outside this join's context; comparison returned to its counterpart join.".into()
        });
        self.before = !self.before;
        // A terminal join is a valid cursor boundary. Picture requests clamp
        // separately to the final frame, without changing this boundary.
        self.cursor = self.cursor.min(self.frames());
        Ok(())
    }

    fn endpoint_identity(&self) -> EndpointIdentity {
        let source = match &self.proposal.source {
            Source::Original {
                asset,
                qualification,
                ordinals,
            } => EndpointSourceId::Original {
                asset: asset.clone(),
                qualification: qualification.clone(),
                in_frame: SourceFrameId(ordinals.start),
                out_frame: SourceFrameId(ordinals.end),
            },
            Source::Edited { .. } => EndpointSourceId::Copied(
                self.proposal
                    .source
                    .copied_view_id()
                    .expect("edited slice identity"),
            ),
        };
        EndpointIdentity {
            session: self.proposal.id.session,
            project: self.proposal.id.project.clone(),
            revision: self.proposal.id.base_revision.clone(),
            draft: self.proposal.id.draft,
            change: self.endpoint_change,
            source,
        }
    }

    fn source_range(&self) -> std::ops::Range<u64> {
        self.proposal
            .source
            .boundaries()
            .expect("checked slice boundaries")
    }

    fn edited_source(&self) -> bool {
        matches!(self.proposal.source, Source::Edited { .. })
    }

    fn empty_structure(&self) -> bool {
        empty::is_structural(&self.proposal.source)
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
            self.endpoints_pending = !self.empty_structure();
            self.source_view = None;
        }
        self.prepared = None;
        self.error = None;
        self.position = None;
        self.comparison_note = None;
        self.dirty = true;
        self.proposal.destination = if self.replacing {
            Destination::Replace {
                range: self.replacement.ok_or("No captured Edit selection")?,
            }
        } else {
            placement_at(&self.seams, &self.children, self.slot, self.destination)?
        };
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
        let target = self.capture_placement_target();
        self.open_captured_splice(context, target);
    }

    pub(super) fn open_captured_splice(
        &mut self,
        context: &egui::Context,
        target: Result<moment::PlacementTarget, String>,
    ) {
        self.copied.clear_selection();
        let captured = (|| {
            let target = target?;
            self.check_placement_target(&target)?;
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
            let base = target.base.clone();
            let copied = target.copied.as_ref().ok_or_else(|| {
                if let Some(name) = target.register {
                    return format!(
                        "Register {name} is empty. Copy or cut into it before placing a slice."
                    );
                }
                format!(
                    "Copy a range from Original or Your edit first: {}.",
                    self.editor_copy_recipe()
                )
            })?;
            let view = target.scope.resolve(&base)?;
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
            let source = copied.source()?;
            let range = source.boundaries()?;
            let source_bounds = match copied {
                copied::Content::Original(copied) => {
                    0..base
                        .sources
                        .get(&copied.identity.asset)
                        .and_then(|source| source.video_index.as_ref())
                        .ok_or("The copied Original has no qualified pictures.")?
                        .frames()
                        .len() as u64
                }
                copied::Content::Edited(copied) => {
                    u64::try_from(copied.bounds().start().0).map_err(|_| "Negative source In")?
                        ..u64::try_from(copied.bounds().end().0)
                            .map_err(|_| "Negative source Out")?
                }
                copied::Content::Macro(_) => return Err(copied::MACRO_PASTE_ERROR.into()),
            };
            if (range.start >= range.end && !empty::is_structural(&source))
                || range.start < source_bounds.start
                || range.end > source_bounds.end
            {
                return Err("The copied range is outside its source scope.".into());
            }
            Ok((base, source, parent, seams, children, source_bounds, target))
        })();
        let (base, source, parent, seams, children, source_bounds, target) = match captured {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(token) = self.next_serial() else {
            return;
        };
        let empty_structure = empty::is_structural(&source);
        let slot = empty::initial_slot(
            &seams,
            &children,
            target.selected_beat.as_ref().filter(|_| empty_structure),
            target.cursor,
        );
        let destination = match placement_at(&seams, &children, slot, target.cursor) {
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
            source,
            operation: Operation::Copy,
            scope: target.scope,
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
            destination: target.cursor,
            replacement: target.range,
            replacing: false,
            site: Site::Insertion,
            comparison_note: None,
            cursor: target.cursor,
            focus: Focus::Destination,
            before: false,
            looping: false,
            position: None,
            count: None,
            source_bounds,
            source_view: None,
            endpoint_change: 1,
            endpoints_pending: !empty_structure,
            endpoints: pictures::Display::new(self.render_state.clone()),
            entry_cursor: target.cursor,
            entry_source: target.source_cursor,
            entry_pane: target.pane,
            entry_selection: target.selection,
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
                if let Some(source) = update.source_view
                    && draft.proposal.source.copied_view_id().as_ref() == Some(&source.id)
                {
                    let result = source.result.and_then(|view| {
                        if view.id() != &source.id
                            || view.media().session() != update.id.session
                            || view.media().admitted().document().project_id() != &update.id.project
                            || view.media().admitted().capture_revision()
                                != &source.id.copy.source_revision
                        {
                            return Err("Slice endpoints returned a different copied range.".into());
                        }
                        Ok(view)
                    });
                    match result {
                        Ok(view) => {
                            if draft
                                .source_view
                                .as_ref()
                                .is_none_or(|old| !Arc::ptr_eq(old, &view))
                            {
                                draft.endpoints_pending = !draft.empty_structure();
                            }
                            draft.source_view = Some(view);
                        }
                        Err(error) => {
                            draft.source_view = None;
                            draft.endpoints_pending = false;
                            draft.endpoints.receive(EndpointReply {
                                identity: draft.endpoint_identity(),
                                pictures: Err(error),
                            });
                        }
                    }
                }
                let result = update.result.and_then(|prepared| {
                    prepared.validate_result()?;
                    prepared
                        .snapshot
                        .validate_proposed_base(prepared.base.session, &prepared.base.document)
                        .map_err(|error| error.to_string())?;
                    match (&draft.proposal.source, &prepared.media) {
                        (Source::Original { .. }, PreparedMedia::Original) => {
                            prepared
                                .snapshot
                                .validate_original_proposal()
                                .map_err(|error| error.to_string())?;
                        }
                        (Source::Edited { .. }, PreparedMedia::Edited(media)) => {
                            if media.session() != update.id.session {
                                return Err(
                                    "Slice media belongs to another project session.".into()
                                );
                            }
                            prepared
                                .snapshot
                                .validate_edit_slice_view(media.admitted())
                                .map_err(|error| error.to_string())?;
                        }
                        _ => {
                            return Err(
                                "Slice preparation returned a different source kind.".into()
                            );
                        }
                    }
                    if prepared.base.session != update.id.session
                        || prepared.base.document.project_id() != &update.id.project
                        || prepared.base.document.revision_id() != &update.id.base_revision
                        || prepared.parent != draft.proposal.parent
                        || prepared.empty_slot
                            != if draft.empty_structure() {
                                match draft.proposal.destination {
                                    Destination::Slot(slot) => Some(slot),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        || prepared.movement.is_some()
                            != (draft.proposal.operation == Operation::Move)
                        || prepared.removed
                            != match draft.proposal.destination {
                                Destination::Replace { range } => Some(range),
                                _ => None,
                            }
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
                        if draft.proposal.operation == Operation::Move
                            && draft.focus == Focus::Picture
                        {
                            draft.inspect_site(draft.site);
                        }
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
            && !draft.empty_structure()
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
        if draft.endpoints_pending && !draft.empty_structure() {
            let input = match &draft.proposal.source {
                Source::Original { .. } => Some(EndpointInput::Original(draft.base.clone())),
                Source::Edited { .. } => draft
                    .source_view
                    .as_ref()
                    .filter(|view| {
                        Some(view.id()) == draft.proposal.source.copied_view_id().as_ref()
                    })
                    .map(|view| EndpointInput::Copied(view.clone())),
            };
            if let Some(input) = input {
                self.endpoint_worker
                    .submit(draft.endpoint_identity(), input);
                draft.endpoints_pending = false;
            }
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
            if draft.empty_structure() {
                return None;
            }
            return Some(match &draft.proposal.source {
                Source::Original {
                    asset, ordinals, ..
                } => Work::Project {
                    workspace: draft.base.clone(),
                    view: ProjectView::Source {
                        asset: asset.clone(),
                        frame: SourceFrameId(if draft.focus == Focus::In {
                            ordinals.start
                        } else {
                            ordinals.end - 1
                        }),
                    },
                },
                Source::Edited { range, .. } => Work::Copied {
                    view: draft.source_view.as_ref()?.clone(),
                    frame: ProjectFrame(if draft.focus == Focus::In {
                        0
                    } else {
                        range.duration().frames() - 1
                    }),
                },
            });
        }
        if draft.frames() == 0 {
            return None;
        }
        let at = frame
            .unwrap_or(draft.cursor)
            .min(draft.frames().saturating_sub(1));
        let view = ProjectView::Sequence {
            frame: ProjectFrame(i64::try_from(at).ok()?),
        };
        if draft.before || draft.prepared.is_none() || draft.empty_structure() {
            Some(Work::Project {
                workspace: draft.base.clone(),
                view,
            })
        } else {
            let prepared = draft.prepared.as_ref()?;
            Some(match &prepared.media {
                PreparedMedia::Original => Work::Proposed {
                    base: draft.base.clone(),
                    snapshot: prepared.snapshot.clone(),
                    view,
                },
                PreparedMedia::Edited(media) => Work::EditedProposed {
                    base: draft.base.clone(),
                    snapshot: prepared.snapshot.clone(),
                    media: media.clone(),
                    frame: ProjectFrame(i64::try_from(at).ok()?),
                },
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
                self.edit_range = draft.entry_selection;
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
            if let Some(draft) = &mut self.splice
                && draft.empty_structure()
                && (draft.frames() == 0 || matches!(draft.focus, Focus::In | Focus::Out))
            {
                draft.error = Some(if draft.frames() == 0 {
                    "The destination edit is empty; there are no pictures or audio to audition."
                } else {
                    "This empty group has no pictures or audio. Use d or f to audition the destination."
                }.into());
                return;
            }
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
            SpliceKey::Move => {
                let entering_move = draft.proposal.operation != Operation::Move;
                if entering_move && draft.empty_structure() {
                    draft.error = Some(empty::PLACEMENT_REASON.into());
                    return;
                }
                if entering_move && !draft.edited_source() {
                    draft.error = Some(
                        "Original stays immutable. Copy a range from Your edit to move it.".into(),
                    );
                    return;
                }
                draft.proposal.operation = if draft.proposal.operation == Operation::Move {
                    Operation::Copy
                } else {
                    Operation::Move
                };
                draft.replacing = false;
                draft.site = Site::Insertion;
                draft.focus = Focus::Picture;
                draft.cursor = draft.destination;
                changed = true;
            }
            SpliceKey::Replace => {
                if draft.empty_structure() {
                    draft.error = Some(empty::PLACEMENT_REASON.into());
                    return;
                }
                if draft.replacement.is_none() {
                    draft.error = Some(
                        "Select an Edit range before opening Place slice to replace it.".into(),
                    );
                    return;
                }
                draft.proposal.operation = Operation::Copy;
                draft.site = Site::Insertion;
                draft.replacing = !draft.replacing;
                draft.focus = if draft.replacing {
                    Focus::Picture
                } else {
                    Focus::Destination
                };
                draft.cursor = if draft.replacing {
                    draft.replacement.expect("checked range").start().0 as u64
                } else {
                    draft.destination
                };
                changed = true;
            }
            SpliceKey::In => {
                draft.focus = Focus::In;
                if draft.empty_structure() {
                    draft.error = Some(empty::ENDPOINT_REASON.into());
                }
            }
            SpliceKey::Out => {
                draft.focus = Focus::Out;
                if draft.empty_structure() {
                    draft.error = Some(empty::ENDPOINT_REASON.into());
                }
            }
            SpliceKey::Destination => {
                if draft.replacing {
                    draft.error = Some("Replace keeps the captured Edit range fixed. f inspects; r returns to Insert.".into());
                    return;
                }
                draft.focus = Focus::Destination;
                draft.site = Site::Insertion;
                draft.cursor = draft.destination;
            }
            SpliceKey::Picture => {
                if draft.proposal.operation == Operation::Move {
                    draft.inspect_site(Site::Insertion);
                } else {
                    draft.focus = Focus::Picture;
                }
            }
            SpliceKey::Removal => {
                if draft.proposal.operation != Operation::Move {
                    draft.error = Some("Choose Move with m to inspect the removal join.".into());
                    return;
                }
                draft.inspect_site(Site::Removal);
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
                        if draft.empty_structure() {
                            draft.error = Some(empty::ENDPOINT_REASON.into());
                            return;
                        }
                        let range = draft.source_range();
                        let start = advance(range.start, draft.source_bounds.start, range.end - 1);
                        if let Err(error) = draft.proposal.source.set_boundaries(start..range.end) {
                            draft.error = Some(error);
                            return;
                        }
                        changed = true;
                        endpoints = true;
                    }
                    Focus::Out => {
                        if draft.empty_structure() {
                            draft.error = Some(empty::ENDPOINT_REASON.into());
                            return;
                        }
                        let range = draft.source_range();
                        let end = advance(range.end, range.start + 1, draft.source_bounds.end);
                        if let Err(error) = draft.proposal.source.set_boundaries(range.start..end) {
                            draft.error = Some(error);
                            return;
                        }
                        changed = true;
                        endpoints = true;
                    }
                    Focus::Destination => {
                        if draft.empty_structure() {
                            draft.error = Some(empty::PLACEMENT_REASON.into());
                            return;
                        }
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
                        draft.cursor = advance(draft.cursor, 0, draft.frames());
                        draft.position = None;
                        draft.comparison_note = None;
                    }
                }
            }
            SpliceKey::Boundary(forward) => {
                if draft.replacing {
                    draft.error = Some("Replace keeps the captured Edit range fixed. f and h/l inspect either join; r returns to Insert.".into());
                    return;
                }
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
                draft.site = Site::Insertion;
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
        if draft.frames() == 0 {
            draft.error = Some(
                "The destination edit is empty; there are no pictures or audio to audition.".into(),
            );
            return;
        }
        if let Some(heard) = heard {
            draft.position = Some(heard);
        }
        if action == SpliceKey::Compare
            && let Err(error) = draft.compare(context)
        {
            draft.error = Some(error);
            return;
        }
        if action == SpliceKey::Loop {
            draft.looping = true;
        }
        draft.focus = Focus::Picture;
        let ready = (|| {
            let snapshot = draft
                .snapshot()
                .ok_or("Wait for the current slice proposal.")?;
            let comparison = draft
                .comparison()
                .ok_or("Wait for the current slice proposal.")?;
            let domain = Domain::Sequence {
                rate: snapshot.document.presentation_basis().frame_rate,
                frames: i64::try_from(draft.frames())
                    .map_err(|_| "Slice preview duration overflow")?,
            };
            let (windows, _) = comparison.windows(
                snapshot.document.presentation_basis().frame_rate,
                context.lead.0,
                context.follow.0,
            )?;
            let (start, finish) = windows.side(draft.before);
            let looping = draft.looping;
            let window = Window::new(AudioSample(start), AudioSample(finish), looping)
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::FrameRate;

    #[test]
    fn comparison_retains_prefix_suffix_and_clamps_only_replaced_interiors() {
        assert_eq!(map_comparison(29, (30, 44), (30, 60)), Ok(29));
        assert_eq!(map_comparison(30, (30, 44), (30, 60)), Ok(30));
        assert_eq!(map_comparison(43, (30, 44), (30, 60)), Ok(43));
        assert_eq!(map_comparison(44, (30, 44), (30, 60)), Ok(60));
        assert_eq!(map_comparison(49, (30, 44), (30, 60)), Ok(65));
        assert_eq!(map_comparison(59, (30, 60), (30, 44)), Ok(43));
        assert_eq!(map_comparison(44, (30, 44), (30, 30)), Ok(30));
        assert_eq!(map_comparison(31, (30, 30), (30, 44)), Ok(45));
        assert!(map_comparison(i64::MAX, (0, 1), (0, 3)).is_err());
    }

    #[test]
    fn ntsc_comparison_uses_absolute_sample_boundaries_and_round_trips_suffix() {
        let rate = FrameRate::new(30_000, 1_001).unwrap();
        let boundary = |frame| rate.audio_boundary(ProjectFrame(frame)).unwrap().0;
        let mut non_additive = false;
        for start in 1..20 {
            for removed in 1..20 {
                let from = (boundary(start), boundary(start + 7));
                let to = (boundary(start), boundary(start + removed));
                let heard = from.1 + 137;
                let compared = map_comparison(heard, from, to).unwrap();
                assert_eq!(compared, to.1 + 137);
                assert_eq!(map_comparison(compared, to, from).unwrap(), heard);
                non_additive |= to.1 - from.1 != boundary(removed) - boundary(7);
            }
        }
        assert!(non_additive, "fixture must expose rounded-duration drift");
    }
}
