use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use deadpan_core::{
    AssetId, Command, CommandRequest, NodeId, NodeKind, ProjectDocument, ProjectId, RevisionId,
};
use deadpan_plan::RenderPlan;
use deadpan_store::original_media::{OriginalMediaRecord, OriginalOwnership};
use deadpan_store::single_source::{SingleSourceInitialization, SingleSourceState};
use deadpan_store::source_registration::{
    PreparedSourceRegistration, SourceInsertionPurpose, SourceInsertionRequest, SourceRegistration,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

use crate::library::ProjectLibrary;

use super::worker::{Job, Prepared, Reply, Streams, Work};
use super::{
    CommittedEdit, ImportMedia, ImportStage, ImportStatus, PreparedRoomTone, ProjectEdit,
    ProjectRequest, ProjectSoundEdit, ProjectUpdate, RegisteredSource, RoomToneFailure,
    SequenceScope, Shared, SoundCommit, Workspace,
};

type Result<T> = std::result::Result<T, String>;

mod gain;
mod headless;
mod moment;
mod render;
mod render_history;
mod room_tone;
mod splice;

struct Pending {
    id: u64,
    session: u64,
    cancelled: Arc<AtomicBool>,
    streams: Streams,
    insertion: Option<SourceRegistration>,
    initialization: Option<SingleSourceInitialization>,
    moment: Option<moment::PendingMoment>,
    splice: Option<super::splice::ProposalId>,
    scope: SequenceScope,
}

struct Service {
    shared: Arc<Shared>,
    store: Option<ProjectStore>,
    workspace: Option<Arc<Workspace>>,
    import: Option<ImportStatus>,
    error: Option<String>,
    message: Option<String>,
    committed: Option<CommittedEdit>,
    room_tone: Option<PreparedRoomTone>,
    room_tone_error: Option<RoomToneFailure>,
    gain: Option<super::gain::ProposalUpdate>,
    splice: Option<super::splice::ProposalUpdate>,
    splice_commit: Option<super::splice::SpliceCommitUpdate>,
    splice_draft: Option<splice::Draft>,
    splice_seen: Option<super::splice::ProposalId>,
    active: Option<Pending>,
    // One reusable catalog token. A live slice draft can additionally retain
    // its exact prepared Original, never a token for every catalog asset.
    cached: Option<(AssetId, PreparedSourceRegistration)>,
    session: u64,
    serial: u64,
    jobs: SyncSender<Job>,
    library: Option<ProjectLibrary>,
    render: Option<render::NativeRender>,
    render_update: Option<super::ProjectRenderUpdate>,
    render_history: Option<super::render_history::Update>,
    pending_session_change: Option<render::PendingSessionChange>,
    host: Option<headless::Host>,
    // Kept across session replacement until the shared worker drains its reply.
    host_preparing: Option<(u64, Arc<AtomicBool>)>,
    #[cfg(test)]
    render_preview_refresh_failure: bool,
}

pub(super) fn run(
    shared: Arc<Shared>,
    requests: Receiver<ProjectRequest>,
    jobs: SyncSender<Job>,
    results: Receiver<Reply>,
    worker: JoinHandle<()>,
    library: Option<ProjectLibrary>,
) {
    let mut service = Service {
        shared,
        store: None,
        workspace: None,
        import: None,
        error: None,
        message: None,
        committed: None,
        room_tone: None,
        room_tone_error: None,
        gain: None,
        splice: None,
        splice_commit: None,
        splice_draft: None,
        splice_seen: None,
        active: None,
        cached: None,
        session: 0,
        serial: 0,
        jobs,
        library,
        render: None,
        render_update: None,
        render_history: None,
        pending_session_change: None,
        host: None,
        host_preparing: None,
        #[cfg(test)]
        render_preview_refresh_failure: false,
    };
    let mut requests_connected = true;
    loop {
        let shutdown_changed = if service.shared.stopping.load(Ordering::Acquire)
            && (!service.shared.busy.load(Ordering::Acquire)
                || service.pending_session_change.is_some())
        {
            service.begin_render_shutdown()
        } else {
            false
        };
        let changed = shutdown_changed | service.pump_render() | service.finish_session_change();
        if changed {
            service.publish();
        }
        service.pump_host();
        service.pump_host_preparation();
        if service.shared.stopping.load(Ordering::Acquire)
            && !service.shared.busy.load(Ordering::Acquire)
            && service.pending_session_change.is_none()
            && service.render.is_none()
        {
            break;
        }
        let request = if requests_connected {
            requests.recv_timeout(Duration::from_millis(10))
        } else {
            std::thread::park_timeout(Duration::from_millis(10));
            Err(RecvTimeoutError::Timeout)
        };
        match request {
            Ok(request) => {
                if service.dispatch_request(request) {
                    service.shared.busy.store(false, Ordering::Release);
                }
                service.invalidate_changed_splice();
                service.publish();
            }
            Err(RecvTimeoutError::Disconnected) => {
                requests_connected = false;
                service.shared.stopping.store(true, Ordering::Release);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if service.shared.stopping.load(Ordering::Acquire) {
            continue;
        }
        match results.try_recv() {
            Ok(reply) => {
                service.result(reply);
                service.invalidate_changed_splice();
                service.publish();
            }
            Err(TryRecvError::Disconnected)
                if service.active.is_some() || service.host_preparing.is_some() =>
            {
                service.host_preparation_disconnected();
                if let Some(active) = service.active.take() {
                    service.splice_worker_disconnected(&active);
                    active.cancelled.store(true, Ordering::Release);
                    if service
                        .workspace
                        .as_ref()
                        .is_some_and(|workspace| workspace.session == active.session)
                        && active.splice.is_none()
                        && let Some(status) = &mut service.import
                        && status.stage != ImportStage::Cancelled
                    {
                        status.stage = ImportStage::Failed;
                        status.error =
                            Some("Import worker stopped before completing preparation".into());
                    }
                }
                service.publish();
            }
            Err(_) => {}
        }
    }
    service.cancel();
    // Revoke handles and release the lock before waiting for cooperative decoding.
    service.host = None;
    service.store = None;
    service.workspace = None;
    service.cached = None;
    service.shared.busy.store(false, Ordering::Release);
    let shared = service.shared.clone();
    drop(service);
    drop(results);
    let _ = worker.join();
    shared.shutdown_complete.store(true, Ordering::Release);
    (shared.wake)();
}

impl Service {
    fn publish(&self) {
        let update = ProjectUpdate {
            workspace: self.workspace.clone(),
            import: self.import.clone(),
            error: self.error.clone(),
            message: self.message.clone(),
            committed: self.committed.clone(),
            room_tone: self
                .room_tone
                .as_ref()
                .filter(|prepared| {
                    self.workspace.as_ref().is_some_and(|workspace| {
                        workspace.session == prepared.session
                            && workspace.document.revision_id() == &prepared.revision
                    })
                })
                .cloned(),
            room_tone_error: self.room_tone_error.clone(),
            gain: self.gain.clone(),
            splice: self.splice.clone(),
            splice_commit: self.splice_commit.clone(),
            render: self.render_update.clone(),
            render_history: self.render_history.clone(),
        };
        *self
            .shared
            .update
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(update);
        (self.shared.wake)();
    }

    fn command(&mut self, request: ProjectRequest) -> Result<()> {
        let request = match request {
            ProjectRequest::PrepareSplice(proposal) => {
                self.prepare_splice_command(proposal);
                return Ok(());
            }
            ProjectRequest::CommitSplice(id) => return self.commit_splice_command(id),
            ProjectRequest::AbandonSplice(id) => {
                self.abandon_splice(&id);
                return Ok(());
            }
            request => request,
        };
        if let ProjectRequest::RenderHistory(request) = request {
            self.render_history_command(request);
            return Ok(());
        }
        if let ProjectRequest::Render(request) = request {
            self.render_command(request);
            return Ok(());
        }
        self.committed = None;
        self.room_tone = None;
        self.room_tone_error = None;
        self.gain = None;
        match request {
            ProjectRequest::Render(_) => unreachable!("render commands use their own feedback"),
            ProjectRequest::RenderHistory(_) => {
                unreachable!("render history queries use their own feedback")
            }
            ProjectRequest::CreateFromSource { path } => self.create_from_source(path),
            ProjectRequest::InitializeSource {
                expected_session,
                expected_revision,
                path,
            } => self.initialize_source(expected_session, expected_revision, path),
            ProjectRequest::ImportSound {
                expected_session,
                expected_revision,
                path,
                stream,
                ownership,
            } => {
                self.check_context(expected_session, &expected_revision)?;
                self.import(
                    path,
                    stream.map_or(ImportMedia::FirstAudio, |stream| ImportMedia::Audio {
                        stream,
                    }),
                    ownership,
                )
            }
            #[cfg(test)]
            ProjectRequest::Create(path) => self.open(path, true),
            ProjectRequest::Open(path) => self.open(path, false),
            ProjectRequest::Close => {
                self.cancel();
                self.host = None;
                self.store = None;
                self.workspace = None;
                self.cached = None;
                self.import = None;
                self.message = Some("Project closed".into());
                Ok(())
            }
            ProjectRequest::Import {
                path,
                media,
                ownership,
            } => self.import(path, media, ownership),
            ProjectRequest::CancelImport => {
                self.cancel();
                self.message = Some("Import cancellation requested".into());
                Ok(())
            }
            ProjectRequest::Insert {
                expected_session,
                expected_revision,
                asset,
                scope,
                parent,
                index,
            } => self.insert(
                expected_session,
                expected_revision,
                asset,
                scope,
                parent,
                index,
            ),
            ProjectRequest::PasteMoment(request) => self.paste_moment(request),
            ProjectRequest::PrepareSplice(_)
            | ProjectRequest::CommitSplice(_)
            | ProjectRequest::AbandonSplice(_) => unreachable!("splice uses independent feedback"),
            ProjectRequest::PrepareGain(proposal) => {
                let result = self.prepare_gain(&proposal);
                self.gain = Some(super::gain::ProposalUpdate {
                    id: proposal.id(),
                    result,
                });
                // Proposal failures are exclusively identity-tagged. A stale
                // failure must not escape through the generic workspace error.
                Ok(())
            }
            ProjectRequest::PrepareRoomTone {
                expected_session,
                expected_revision,
                ticket,
                selection,
            } => {
                let result = self.prepare_room_tone(
                    expected_session,
                    expected_revision.clone(),
                    ticket,
                    selection,
                );
                if let Err(error) = &result {
                    self.room_tone_error = Some(RoomToneFailure {
                        ticket,
                        session: expected_session,
                        revision: expected_revision,
                        error: error.clone(),
                    });
                }
                result
            }
            ProjectRequest::Edit {
                expected_session,
                expected_revision,
                cursor,
                scope,
                edit,
            } => self.edit(expected_session, expected_revision, cursor, scope, edit),
            ProjectRequest::SoundEdit {
                expected_session,
                expected_revision,
                edit,
            } => self.sound_edit(expected_session, expected_revision, edit),
            ProjectRequest::Undo { expected_revision } => {
                self.writer()?
                    .undo(&expected_revision, revision())
                    .map_err(display)?;
                self.refresh()?;
                self.message = Some("Undo saved".into());
                Ok(())
            }
            ProjectRequest::Redo { expected_revision } => {
                self.writer()?
                    .redo(&expected_revision, revision())
                    .map_err(display)?;
                self.refresh()?;
                self.message = Some("Redo saved".into());
                Ok(())
            }
        }
    }

    fn writer(&mut self) -> Result<&mut ProjectStore> {
        self.store
            .as_mut()
            .ok_or_else(|| "Open or create a project first".into())
    }

    fn check_context(&self, expected_session: u64, expected_revision: &RevisionId) -> Result<()> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open or create a project first")?;
        if workspace.session != expected_session {
            return Err("Project session changed before the request".into());
        }
        if workspace.document.revision_id() != expected_revision {
            return Err("Project changed before the request".into());
        }
        Ok(())
    }

    fn sound_edit(
        &mut self,
        expected_session: u64,
        expected_revision: RevisionId,
        edit: ProjectSoundEdit,
    ) -> Result<()> {
        self.check_context(expected_session, &expected_revision)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let (command, selected, message) = match edit {
            ProjectSoundEdit::Place { asset, at } => {
                let event = super::sound::placement(workspace, &asset, at)?;
                let id = deadpan_core::SoundId::new(uuid::Uuid::new_v4().to_string())
                    .map_err(display)?;
                (
                    Command::SetSound {
                        id: id.clone(),
                        event,
                    },
                    Some(id),
                    "Sound placed and saved",
                )
            }
            ProjectSoundEdit::Update {
                id,
                gain_millidecibels,
                start_edge,
                end_edge,
            } => {
                let mut event = workspace
                    .document
                    .sounds()
                    .get(&id)
                    .cloned()
                    .ok_or("The selected sound no longer exists.")?;
                event.gain_millidecibels = gain_millidecibels;
                event.start_edge = start_edge;
                event.end_edge = end_edge;
                (
                    Command::SetSound {
                        id: id.clone(),
                        event,
                    },
                    Some(id),
                    "Sound parameters saved",
                )
            }
            ProjectSoundEdit::Move { id, at } => {
                let event = super::sound::moved(workspace, &id, at)?;
                (
                    Command::SetSound {
                        id: id.clone(),
                        event,
                    },
                    Some(id),
                    "Sound moved and saved",
                )
            }
            ProjectSoundEdit::Nudge { id, frames } => {
                let event = super::sound::nudge(workspace, &id, frames)?;
                (
                    Command::SetSound {
                        id: id.clone(),
                        event,
                    },
                    Some(id),
                    "Sound nudged and saved",
                )
            }
            ProjectSoundEdit::Allowance {
                id,
                issuer,
                at,
                allowed,
            } => {
                let target = super::sound::pause_target(workspace, &id, at)?;
                if target.issuer != issuer {
                    return Err("The identified pause changed; no allowance was saved.".into());
                }
                target.validate_change(allowed)?;
                (
                    Command::SetSoundAllowance {
                        sound: id.clone(),
                        issuer,
                        allowed,
                    },
                    Some(id),
                    if allowed {
                        "Sound allowed in this pause and saved"
                    } else {
                        "Sound silenced in this pause and saved"
                    },
                )
            }
            ProjectSoundEdit::Delete { id } => {
                if !workspace.document.sounds().contains_key(&id) {
                    return Err("The selected sound no longer exists.".into());
                }
                (Command::DeleteSound { id }, None, "Sound removed and saved")
            }
        };
        let request = CommandRequest {
            project_id: workspace.document.project_id().clone(),
            expected_revision,
            new_revision: revision(),
            command,
        };
        // Recheck revision-bound source evidence and the existing generation
        // relevance guard in the writer transaction. Do not invent observations.
        let outcome = self.writer()?.commit(&request).map_err(display)?;
        self.refresh()?;
        self.committed = Some(CommittedEdit {
            revision: outcome.revision_id,
            selected_node: None,
            preserve_cursor: true,
            cursor: None,
            scope: SequenceScope::default(),
            sound: Some(SoundCommit { selected }),
        });
        self.message = Some(message.into());
        Ok(())
    }

    fn create_from_source(&mut self, path: PathBuf) -> Result<()> {
        // A cancelled preparation retains its single worker slot until its reply.
        // Never allocate a package that cannot immediately start initialization.
        if self.active.is_some() || self.host_preparation_active() {
            return Err("Wait for the current import to stop before creating a project".into());
        }
        let library = match &self.library {
            Some(library) => library.clone(),
            None => ProjectLibrary::documents()?,
        };
        let document = new_document()?;
        let (package, mut store) = library.create(&path, &document)?;
        let next = self
            .session
            .checked_add(1)
            .ok_or("Project session identities exhausted")?;
        let workspace = snapshot(&store, next, package.canonicalize().map_err(display)?, None)?;
        let host = headless::Host::bind(&mut store)?;
        self.cancel();
        self.host = Some(host);
        self.store = Some(store);
        self.workspace = Some(Arc::new(workspace));
        self.session = next;
        self.cached = None;
        self.import = None;
        self.initialize_source(next, document.revision_id().clone(), path)
    }

    fn initialize_source(
        &mut self,
        expected_session: u64,
        expected_revision: RevisionId,
        path: PathBuf,
    ) -> Result<()> {
        self.check_context(expected_session, &expected_revision)?;
        if !matches!(
            self.workspace
                .as_ref()
                .and_then(|workspace| workspace.single_source.as_ref()),
            Some(SingleSourceState::AwaitingSource { .. })
        ) {
            return Err("This project already has an Original or is a legacy project".into());
        }
        let initialization = SingleSourceInitialization {
            expected_revision,
            new_revision: revision(),
            new_asset_id: AssetId::new(uuid::Uuid::new_v4().to_string()).map_err(display)?,
            node: node(),
            label: source_label(&path),
        };
        self.begin(
            path.clone(),
            Streams::Import(ImportMedia::Video),
            None,
            Some(initialization),
            SequenceScope::default(),
            Work::Retain {
                path,
                ownership: OriginalOwnership::Managed,
            },
        )
    }

    fn edit(
        &mut self,
        expected_session: u64,
        expected_revision: RevisionId,
        cursor: deadpan_core::ProjectFrame,
        scope: SequenceScope,
        edit: ProjectEdit,
    ) -> Result<()> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open or create a project first")?;
        if workspace.session != expected_session {
            return Err("Project session changed before the edit".into());
        }
        let document = &workspace.document;
        if document.revision_id() != &expected_revision {
            return Err("Project changed before the edit".into());
        }
        scope.resolve(workspace)?;
        if cursor.0 < 0 || cursor.0 > workspace.plan.duration().frames() {
            return Err("Edit cursor is outside the project".into());
        }
        if let ProjectEdit::InsertTime { at, duration } = edit {
            if duration == deadpan_core::FrameDuration::ZERO {
                self.message = Some("Pause resolves to 0 frames; no edit was made.".into());
                return Ok(());
            }
            scope.check_pause(workspace, at)?;
            let id = node();
            let request =
                super::pause::prepare(workspace, at, duration, revision(), id.clone(), node)?;
            // As for every native edit, current generation requests require the
            // real host relevance resolver. Never invent observations here.
            let outcome = self.writer()?.commit(&request).map_err(display)?;
            self.committed = Some(CommittedEdit {
                revision: outcome.revision_id,
                selected_node: Some(id),
                preserve_cursor: false,
                cursor: Some(at),
                scope,
                sound: None,
            });
            self.refresh()?;
            self.message = Some(format!(
                "Inserted a {} frame silent pause at boundary {} and saved",
                duration.frames(),
                at.0
            ));
            return Ok(());
        }
        let target = match &edit {
            ProjectEdit::InsertTime { .. } => unreachable!("pause handled above"),
            ProjectEdit::Split { node, .. }
            | ProjectEdit::Repeat { node, .. }
            | ProjectEdit::WrapRepeat { node, .. }
            | ProjectEdit::Retime { node, .. }
            | ProjectEdit::SetFraming { node, .. }
            | ProjectEdit::SetAudioTreatments { node, .. }
            | ProjectEdit::Delete { node }
            | ProjectEdit::HoldDuration { node, .. }
            | ProjectEdit::HoldAudio { node, .. } => node,
        };
        let view = scope.resolve(workspace)?;
        let children = view.children;
        let position = children
            .iter()
            .position(|child| child == target)
            .ok_or("Select a direct child of the active Sequence before editing")?;
        let selected = Some(target.clone());
        let split_position = matches!(edit, ProjectEdit::Split { .. }).then_some(position);
        let preserve_cursor = matches!(
            edit,
            ProjectEdit::SetFraming { .. }
                | ProjectEdit::HoldAudio { .. }
                | ProjectEdit::SetAudioTreatments { .. }
        );
        let mut retime_message = None;
        let (command, selected_node, message) = match edit {
            ProjectEdit::SetAudioTreatments { node, treatments } => {
                if document.nodes()[&node].audio_treatments == treatments {
                    self.message = Some("Gain is unchanged. No edit was made.".into());
                    return Ok(());
                }
                (
                    Command::SetAudioTreatments { node, treatments },
                    selected,
                    "Gain updated and saved",
                )
            }
            ProjectEdit::SetFraming { node, framing } => (
                Command::SetFraming { node, framing },
                selected,
                "Framing updated and saved",
            ),
            ProjectEdit::InsertTime { .. } => unreachable!("pause handled above"),
            ProjectEdit::Split { node: target, at } => {
                let mut pending = vec![target.clone()];
                let mut count = 3_usize;
                while let Some(id) = pending.pop() {
                    count = count.checked_add(1).ok_or("Split node budget exhausted")?;
                    if count > deadpan_core::MAX_DOCUMENT_NODES {
                        return Err("Split exceeds the document node limit".into());
                    }
                    pending.extend(document.children(&id).cloned());
                }
                (
                    Command::Split {
                        node: target,
                        at,
                        identities: deadpan_core::SplitIdentities {
                            nodes: (0..count).map(|_| node()).collect(),
                        },
                    },
                    None,
                    "Beat split at the cursor and saved",
                )
            }
            ProjectEdit::Repeat {
                node: target,
                plays,
            } => {
                if let NodeKind::Repeat { gap, .. } = &document.nodes()[&target].kind {
                    (
                        Command::SetRepeat {
                            node: target,
                            plays,
                            gap: gap.clone(),
                        },
                        selected,
                        "Repeat updated and saved",
                    )
                } else {
                    let id = node();
                    (
                        Command::WrapRepeat {
                            node: target,
                            id: id.clone(),
                            plays,
                            gap: None,
                            anchor_policy: Default::default(),
                        },
                        Some(id),
                        "Repeat created and saved",
                    )
                }
            }
            ProjectEdit::WrapRepeat {
                node: target,
                plays,
            } => {
                let id = node();
                (
                    Command::WrapRepeat {
                        node: target,
                        id: id.clone(),
                        plays,
                        gap: None,
                        anchor_policy: Default::default(),
                    },
                    Some(id),
                    "Repeat created and saved",
                )
            }
            ProjectEdit::Retime {
                node: target,
                speed,
                pitch,
                wrap,
            } => {
                let change = super::retime::resolve(workspace, &target, speed, wrap)?;
                if change.update {
                    if matches!(&document.nodes()[&target].kind, NodeKind::Retime { duration, pitch: current, .. }
                        if *duration == change.after && *current == pitch)
                    {
                        self.message = Some(format!(
                            "Already set: {}. No edit was made.",
                            change.describe(pitch)
                        ));
                        return Ok(());
                    }
                    retime_message = Some(format!("Saved: {}", change.describe(pitch)));
                    (
                        Command::SetRetime {
                            node: target,
                            duration: change.after,
                            pitch,
                        },
                        selected,
                        "Retime updated and saved",
                    )
                } else {
                    let id = node();
                    retime_message = Some(format!("Saved: {}", change.describe(pitch)));
                    (
                        Command::WrapRetime {
                            node: target,
                            id: id.clone(),
                            duration: change.after,
                            pitch,
                        },
                        Some(id),
                        "Retime created and saved",
                    )
                }
            }
            ProjectEdit::Delete { node } => {
                let selected = children
                    .get(position + 1)
                    .or_else(|| {
                        position
                            .checked_sub(1)
                            .and_then(|index| children.get(index))
                    })
                    .cloned();
                (Command::Delete { node }, selected, "Beat deleted and saved")
            }
            ProjectEdit::HoldDuration { node, duration } => (
                Command::SetHoldDuration { node, duration },
                selected,
                "Hold duration updated and saved",
            ),
            ProjectEdit::HoldAudio { node, audio } => {
                if !matches!(document.nodes()[&node].kind, NodeKind::Hold { .. }) {
                    return Err("Select an ordinary Hold to change its sound policy".into());
                }
                (
                    Command::SetHoldAudio { node, audio },
                    selected,
                    "Hold sound updated and saved",
                )
            }
        };
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision,
            new_revision: revision(),
            command,
        };
        // Generic commit deliberately preserves the store's relevance guard.
        // An unresolved active generation request must fail rather than receive
        // invented observations from a widget or this service.
        let outcome = self.writer()?.commit(&request).map_err(display)?;
        // Retain the actual durable receipt even if rebuilding the workspace
        // fails. Render continuation must report this commit independently of
        // its later admission result.
        self.committed = Some(CommittedEdit {
            revision: outcome.revision_id,
            selected_node: selected_node.clone(),
            preserve_cursor,
            cursor: preserve_cursor.then_some(cursor),
            scope: scope.clone(),
            sound: None,
        });
        self.refresh()?;
        // Resolve the right fragment from the committed structure, never from
        // progress text or an identity-pool ordering. Its start is the cut.
        let selected_node = if let Some(position) = split_position {
            self.workspace.as_ref().and_then(|workspace| {
                scope
                    .resolve(workspace)
                    .ok()?
                    .children
                    .get(position + 1)
                    .cloned()
            })
        } else {
            selected_node
        };
        self.committed
            .as_mut()
            .expect("successful store commit retained its receipt")
            .selected_node = selected_node;
        self.message = Some(retime_message.unwrap_or_else(|| message.into()));
        Ok(())
    }

    fn open(&mut self, path: PathBuf, create: bool) -> Result<()> {
        if let Some(prepared) = self.prepare_open(path, create)? {
            self.install_open(prepared)?;
        } else {
            self.message = Some("Project is already open".into());
        }
        Ok(())
    }

    fn prepare_open(&self, path: PathBuf, create: bool) -> Result<Option<render::PreparedOpen>> {
        if !create
            && let Some(current) = &self.workspace
            && path.canonicalize().ok().as_ref() == Some(&current.path)
        {
            return Ok(None);
        }
        // Keep the previous session and its work alive until the candidate is valid.
        // Native writable Open owns this backed-up migration. Read-only/headless
        // inspection keeps its explicit, non-writing migration contract.
        let mut migration = None;
        let store = if create {
            let document = new_document()?;
            ProjectStore::create(&path, &document)
        } else {
            match ProjectStore::open(&path, AccessMode::ReadWrite) {
                Err(StoreError::MigrationRequired(_)) => {
                    migration = Some(ProjectStore::migrate(&path).map_err(display)?);
                    ProjectStore::open(&path, AccessMode::ReadWrite)
                }
                result => result,
            }
        }
        .map_err(display)?;
        let next = self
            .session
            .checked_add(1)
            .ok_or("Project session identities exhausted")?;
        let workspace = snapshot(&store, next, path.canonicalize().map_err(display)?, None)?;
        let message = match migration {
            Some(migration) if migration.backup.is_some() => format!(
                "Project opened. Upgraded schema {} to {}; original database backup: {}",
                migration.from_schema,
                migration.to_schema,
                migration.backup.as_ref().expect("backup checked").display(),
            ),
            _ if create => "Project created".into(),
            _ => "Project opened".into(),
        };
        Ok(Some(render::PreparedOpen {
            store,
            workspace,
            message,
        }))
    }

    fn install_open(&mut self, mut prepared: render::PreparedOpen) -> Result<()> {
        // A pending Open is unadvertised until installation. Bind before
        // replacing the old session so a failed endpoint keeps that session.
        let host = headless::Host::bind(&mut prepared.store)?;
        self.cancel();
        self.host = Some(host);
        self.store = Some(prepared.store);
        self.session = prepared.workspace.session;
        self.workspace = Some(Arc::new(prepared.workspace));
        self.cached = None;
        self.import = None;
        self.message = Some(prepared.message);
        Ok(())
    }

    fn refresh(&mut self) -> Result<()> {
        #[cfg(test)]
        if std::mem::take(&mut self.render_preview_refresh_failure) {
            return Err("Injected failure refreshing the committed preview".into());
        }
        let current = self.workspace.as_ref().ok_or("No project is open")?;
        let store = self.store.as_ref().ok_or("No project is open")?;
        self.workspace = Some(Arc::new(snapshot(
            store,
            current.session,
            current.path.clone(),
            Some(current),
        )?));
        self.invalidate_changed_splice();
        Ok(())
    }

    fn cancel(&mut self) {
        self.invalidate_splice("Slice proposal was cancelled");
        self.cancel_host_preparation();
        if let Some(active) = &self.active {
            active.cancelled.store(true, Ordering::Release);
            if active.session == self.session
                && active.splice.is_none()
                && let Some(status) = &mut self.import
            {
                status.stage = ImportStage::Cancelled;
                status.error = None;
            }
        }
    }

    fn begin(
        &mut self,
        path: PathBuf,
        streams: Streams,
        insertion: Option<SourceRegistration>,
        initialization: Option<SingleSourceInitialization>,
        scope: SequenceScope,
        work: Work,
    ) -> Result<()> {
        if self.active.is_some() || self.host_preparation_active() {
            return Err(
                "An import is still active; cancel it and wait for preparation to stop".into(),
            );
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open or create a project first")?;
        let id = self
            .serial
            .checked_add(1)
            .ok_or("Import identities exhausted")?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let stage = if insertion.is_some() {
            ImportStage::PreparingInsertion
        } else {
            ImportStage::Retaining
        };
        self.jobs
            .try_send(Job {
                id,
                handle: workspace.originals.clone(),
                cancelled: cancelled.clone(),
                work,
            })
            .map_err(|error| format!("Import worker is unavailable: {error}"))?;
        self.active = Some(Pending {
            id,
            session: workspace.session,
            cancelled,
            streams,
            insertion,
            initialization,
            moment: None,
            splice: None,
            scope,
        });
        self.serial = id;
        self.import = Some(ImportStatus {
            path,
            stage,
            error: None,
            asset: None,
        });
        self.message = None;
        Ok(())
    }

    fn import(
        &mut self,
        path: PathBuf,
        media: ImportMedia,
        ownership: OriginalOwnership,
    ) -> Result<()> {
        if let Some(state) = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.single_source.as_ref())
        {
            if matches!(state, SingleSourceState::AwaitingSource { .. }) {
                return Err("Choose the Original to finish creating this project first".into());
            }
            if matches!(media, ImportMedia::Video) {
                return Err(
                    "V1 projects use one Original video; add a sound or reuse the Original".into(),
                );
            }
        }
        self.begin(
            path.clone(),
            Streams::Import(media),
            None,
            None,
            SequenceScope::default(),
            Work::Retain { path, ownership },
        )
    }

    fn insert(
        &mut self,
        expected_session: u64,
        expected_revision: RevisionId,
        asset: AssetId,
        scope: SequenceScope,
        parent: NodeId,
        index: usize,
    ) -> Result<()> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open or create a project first")?;
        if workspace.session != expected_session {
            return Err("Project session changed before the insertion".into());
        }
        if workspace.document.revision_id() != &expected_revision {
            return Err(display(StoreError::RevisionConflict {
                expected: expected_revision.to_string(),
                current: workspace.document.revision_id().to_string(),
            }));
        }
        let view = scope.resolve(workspace)?;
        if view.owner != &parent {
            return Err("Insertion parent must be the active Sequence scope".into());
        }
        if index > view.children.len() {
            return Err("Insertion position is outside the active Sequence".into());
        }
        let source = workspace
            .sources
            .get(&asset)
            .cloned()
            .ok_or("The selected asset has no measured source receipt")?;
        let registration = SourceRegistration {
            expected_revision,
            new_revision: revision(),
            original: source.original.object().content().clone(),
            new_asset_id: asset.clone(),
            label: source.label.clone(),
            insertion: Some(SourceInsertionRequest {
                parent,
                index,
                node: node(),
                label: source.label.clone(),
                purpose: SourceInsertionPurpose::Primary,
            }),
        };
        if let Some((cached_asset, token)) = self.cached.take() {
            if cached_asset == asset {
                let cancel = AtomicBool::new(false);
                match self
                    .writer()?
                    .register_prepared_source(&registration, &token, None, &cancel)
                {
                    Ok(outcome) => {
                        self.cached = Some((cached_asset, token));
                        self.refresh()?;
                        self.committed = outcome.commit.and_then(|commit| {
                            registration
                                .insertion
                                .as_ref()
                                .map(|insertion| CommittedEdit {
                                    revision: commit.revision_id,
                                    selected_node: Some(insertion.node.clone()),
                                    preserve_cursor: false,
                                    cursor: None,
                                    scope: scope.clone(),
                                    sound: None,
                                })
                        });
                        if self.active.is_none() {
                            self.import = Some(ImportStatus {
                                path: PathBuf::from(&source.label),
                                stage: ImportStage::Complete,
                                error: None,
                                asset: Some(asset),
                            });
                        }
                        self.message = Some("Source inserted and saved".into());
                        return Ok(());
                    }
                    // Availability changed: prepare again using the same captured intent.
                    Err(StoreError::OriginalMedia(_) | StoreError::Io(_)) => {}
                    Err(error) => {
                        self.cached = Some((cached_asset, token));
                        return Err(display(error));
                    }
                }
            } else {
                self.cached = Some((cached_asset, token));
            }
        }
        if self.active.is_some() {
            return Err("Insertion needs source preparation; wait for the active import".into());
        }
        let streams = Streams::Exact {
            video: source.receipt.snapshot().video().is_some(),
            audio: source
                .receipt
                .snapshot()
                .audio()
                .map(|audio| audio.stream().stream_index),
        };
        let record = self
            .writer()?
            .original_record(source.original.object().content())
            .map_err(display)?
            .ok_or("Original ownership record is missing")?;
        self.begin(
            PathBuf::from(&source.label),
            streams,
            Some(registration),
            None,
            scope,
            Work::Qualify { record, streams },
        )
    }

    fn result(&mut self, reply: Reply) {
        let Some(reply) = self.host_preparation_result(reply) else {
            return;
        };
        let Some(active) = self.active.take() else {
            return;
        };
        if reply.id != active.id {
            self.active = Some(active);
            return;
        }
        if active.splice.is_some() {
            self.splice_result(active, reply);
            return;
        }
        if active.cancelled.load(Ordering::Acquire)
            || self
                .workspace
                .as_ref()
                .is_none_or(|workspace| workspace.session != active.session)
        {
            return;
        }
        let outcome = match reply.result {
            Ok(Prepared::Host(_)) => {
                Err("Import worker returned an unrelated host operation".into())
            }
            Err(error) => Err(error),
            Ok(Prepared::Retained(prepared)) => {
                let retained = self.writer().and_then(|store| {
                    store
                        .retain_prepared_original(&prepared, &active.cancelled)
                        .map_err(display)
                });
                match retained {
                    Err(error) => Err(error),
                    Ok(retained) => self.start_qualification(active, retained.record),
                }
            }
            Ok(Prepared::Qualified(prepared)) => self.register(active, *prepared),
        };
        if let Err(error) = outcome {
            if let Some(status) = &mut self.import {
                status.stage = ImportStage::Failed;
                status.error = Some(error);
            }
            self.message = None;
        }
    }

    fn start_qualification(&mut self, active: Pending, record: OriginalMediaRecord) -> Result<()> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Project closed during import")?;
        self.jobs
            .try_send(Job {
                id: active.id,
                handle: workspace.originals.clone(),
                cancelled: active.cancelled.clone(),
                work: Work::Qualify {
                    record,
                    streams: active.streams,
                },
            })
            .map_err(|error| format!("Import worker is unavailable: {error}"))?;
        self.active = Some(active);
        if let Some(status) = &mut self.import {
            status.stage = ImportStage::Decoding;
        }
        Ok(())
    }

    fn register(&mut self, active: Pending, prepared: PreparedSourceRegistration) -> Result<()> {
        if active.moment.is_some() {
            return self.register_moment(active, prepared);
        }
        if let Some(initialization) = active.initialization {
            if let Some(status) = &mut self.import {
                status.stage = ImportStage::Registering;
            }
            self.publish();
            let outcome = self
                .writer()?
                .initialize_prepared_source(&initialization, &prepared, &active.cancelled)
                .map_err(display)?;
            self.cached = Some((outcome.asset_id.clone(), prepared));
            self.refresh()?;
            self.committed = outcome.commit.map(|commit| CommittedEdit {
                revision: commit.revision_id,
                selected_node: Some(initialization.node),
                preserve_cursor: false,
                cursor: None,
                scope: active.scope.clone(),
                sound: None,
            });
            if let Some(status) = &mut self.import {
                status.stage = ImportStage::Complete;
                status.asset = Some(outcome.asset_id);
            }
            self.message = Some("Original ready. The full video is on Your edit.".into());
            return Ok(());
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Project closed during import")?;
        let is_insertion = active.insertion.is_some();
        let is_sound_catalog = !is_insertion
            && matches!(
                workspace.single_source,
                Some(SingleSourceState::Ready { .. })
            )
            && prepared.receipt().snapshot().video().is_none()
            && prepared.receipt().snapshot().audio().is_some();
        let registration = if let Some(registration) = active.insertion {
            let source = workspace
                .sources
                .get(&registration.new_asset_id)
                .ok_or("Insertion asset is no longer registered")?;
            if source.receipt.id() != prepared.receipt().id() {
                return Err("Fresh source qualification differs from the selected asset".into());
            }
            registration
        } else {
            let label = self
                .import
                .as_ref()
                .map(|status| source_label(&status.path))
                .unwrap_or_else(|| "Source".into());
            SourceRegistration {
                expected_revision: workspace.document.revision_id().clone(),
                new_revision: revision(),
                original: prepared.receipt().original().content().clone(),
                new_asset_id: AssetId::new(uuid::Uuid::new_v4().to_string()).map_err(display)?,
                label,
                insertion: None,
            }
        };
        if let Some(status) = &mut self.import {
            status.stage = ImportStage::Registering;
        }
        self.publish();
        // No synthetic relevance observations: active-generation projects fail explicitly.
        let outcome = self
            .writer()?
            .register_prepared_source(&registration, &prepared, None, &active.cancelled)
            .map_err(display)?;
        self.cached = Some((outcome.asset_id.clone(), prepared));
        self.refresh()?;
        if let (Some(insertion), Some(commit)) = (&registration.insertion, &outcome.commit) {
            self.committed = Some(CommittedEdit {
                revision: commit.revision_id.clone(),
                selected_node: Some(insertion.node.clone()),
                preserve_cursor: false,
                cursor: None,
                scope: active.scope.clone(),
                sound: None,
            });
        }
        if let Some(status) = &mut self.import {
            status.stage = ImportStage::Complete;
            status.asset = Some(outcome.asset_id);
        }
        self.message = Some(
            if is_insertion {
                "Source inserted and saved"
            } else if is_sound_catalog {
                "Sound added to the catalog"
            } else {
                "Source registered; ready for explicit insertion"
            }
            .into(),
        );
        Ok(())
    }
}

fn snapshot(
    store: &ProjectStore,
    session: u64,
    path: PathBuf,
    previous: Option<&Workspace>,
) -> Result<Workspace> {
    let document = store.snapshot().map_err(display)?;
    let plan = RenderPlan::compile(&document).map_err(display)?;
    let rate = document.presentation_basis().frame_rate;
    let mut sources = BTreeMap::new();
    for (asset, metadata) in document.assets() {
        let Some(qualification) = &metadata.source_qualification else {
            continue;
        };
        let cached = previous.and_then(|workspace| workspace.sources.get(asset));
        let receipt =
            if let Some(cached) = cached.filter(|source| source.receipt.id() == qualification) {
                cached.receipt.clone()
            } else {
                Arc::new(
                    store
                        .registered_source(document.revision_id(), asset)
                        .map_err(display)?,
                )
            };
        let original = store
            .original_record(receipt.original().content())
            .map_err(display)?
            .ok_or("Registered source has no original inventory record")?;
        if let Some(cached) = cached.filter(|source| {
            source.receipt.id() == qualification
                && source.label == metadata.label
                && source.original == original
                && source
                    .original_audition
                    .as_ref()
                    .is_none_or(|view| view.rate() == rate)
                && source
                    .sound_audition
                    .as_ref()
                    .is_none_or(|view| view.rate() == rate)
        }) {
            sources.insert(asset.clone(), cached.clone());
            continue;
        }
        let original_audition = receipt
            .snapshot()
            .video()
            .map(|_| {
                deadpan_playback::Original::new(rate, asset.clone(), receipt.clone()).map(Arc::new)
            })
            .transpose()
            .map_err(display)?;
        let video_index = original_audition.as_ref().map(|view| view.index().clone());
        let sound_audition =
            if receipt.snapshot().video().is_none() && receipt.snapshot().audio().is_some() {
                Some(Arc::new(
                    deadpan_playback::Sound::new(rate, asset.clone(), receipt.clone())
                        .map_err(display)?,
                ))
            } else {
                None
            };
        sources.insert(
            asset.clone(),
            Arc::new(RegisteredSource {
                asset: asset.clone(),
                label: metadata.label.clone(),
                receipt,
                original,
                video_index,
                original_audition,
                sound_audition,
            }),
        );
    }
    let (can_undo, can_redo) = store.history_availability().map_err(display)?;
    let single_source = store.single_source_state().map_err(display)?;
    let original_duration = match &single_source {
        Some(SingleSourceState::Ready { asset, .. }) => Some(
            sources
                .get(asset)
                .ok_or("Original qualification is missing")?
                .original_audition
                .as_ref()
                .ok_or("Original audition view is missing")?
                .duration(),
        ),
        _ => None,
    };
    Ok(Workspace {
        session,
        path,
        document: Arc::new(document),
        plan: Arc::new(plan),
        sources,
        originals: store.original_import_handle().map_err(display)?,
        generated: store.generated_read_handle(),
        can_undo,
        can_redo,
        single_source,
        original_duration,
    })
}

fn new_document() -> Result<ProjectDocument> {
    ProjectDocument::new_automatic(
        ProjectId::new(uuid::Uuid::new_v4().to_string()).map_err(display)?,
        revision(),
        node(),
    )
    .map_err(display)
}

fn source_label(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Original".into())
}

fn revision() -> RevisionId {
    RevisionId::new(uuid::Uuid::new_v4().to_string()).expect("UUID is a valid revision identity")
}

fn node() -> NodeId {
    NodeId::new(uuid::Uuid::new_v4().to_string()).expect("UUID is a valid node identity")
}

fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}
