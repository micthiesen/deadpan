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
    CommittedEdit, CommittedRangeSelection, ImportMedia, ImportStage, ImportStatus,
    PreparedRoomTone, ProjectEdit, ProjectRequest, ProjectSoundEdit, ProjectUpdate,
    RegisteredSource, RoomToneFailure, SequenceScope, Shared, SoundCommit, Workspace,
};

type Result<T> = std::result::Result<T, String>;

mod backups;
mod cut_slice;
mod delete_range;
pub(super) mod edit_slice;
mod gain;
mod generation;
mod headless;
mod macros;
mod marks;
mod moment;
mod recovery;
mod registers;
mod remote_storage;
mod render;
mod render_history;
mod room_tone;
mod scoped;
mod semantic;
mod shots;
mod slip;
mod splice;
mod storage;
mod targets;
mod transcripts;
mod trim;

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
    semantic: semantic::State,
    macros: Option<super::macros::Update>,
    saved_macro: Option<(super::macros::Operation, super::macros::Receipt)>,
    room_tone: Option<PreparedRoomTone>,
    room_tone_error: Option<RoomToneFailure>,
    gain: Option<super::gain::ProposalUpdate>,
    transcript_save: Option<super::TranscriptSave>,
    activity_save: Option<super::TranscriptSave>,
    shot_save: Option<super::TranscriptSave>,
    correction_save: Option<super::TranscriptSave>,
    splice: Option<super::splice::ProposalUpdate>,
    splice_commit: Option<super::splice::SpliceCommitUpdate>,
    splice_draft: Option<splice::Draft>,
    splice_seen: Option<super::splice::ProposalId>,
    slip: Option<super::slip::ProposalUpdate>,
    slip_commit: Option<super::slip::CommitUpdate>,
    saved_slip: Option<super::slip::CommitReceipt>,
    slip_draft: Option<slip::Draft>,
    slip_seen: Option<super::slip::ProposalId>,
    slip_session: Option<(u64, ProjectId)>,
    trim: Option<super::trim::ProposalUpdate>,
    trim_commit: Option<super::trim::CommitUpdate>,
    saved_trim: Option<super::trim::CommitReceipt>,
    trim_draft: Option<trim::Draft>,
    trim_seen: Option<super::trim::ProposalId>,
    trim_session: Option<(u64, ProjectId)>,
    captured_slice: Option<super::slice::CaptureUpdate>,
    registers: Option<Arc<super::registers::Bank>>,
    captured_original: Option<super::registers::OriginalUpdate>,
    cut_slice: Option<super::slice::CutUpdate>,
    last_cut: Option<(
        super::slice::CaptureRequest,
        Option<super::semantic::CutAttempt>,
        super::slice::CutReceipt,
    )>,
    marks: super::marks::Update,
    marks_state: marks::State,
    copied_view: Option<edit_slice::PreparedCopy>,
    splice_source_view: Option<super::slice::SourceViewUpdate>,
    active: Option<Pending>,
    // One reusable catalog token. A live slice draft can additionally retain
    // its exact prepared Original, never a token for every catalog asset.
    cached: Option<(AssetId, PreparedSourceRegistration)>,
    session: u64,
    serial: u64,
    jobs: SyncSender<Job>,
    library: Option<ProjectLibrary>,
    render: Option<render::NativeRender>,
    /// The render's Jobs-panel registration while it owns the render slot.
    render_registration: Option<crate::jobs::JobHandle>,
    render_update: Option<super::ProjectRenderUpdate>,
    render_history: Option<super::render_history::Update>,
    pending_session_change: Option<render::PendingSessionChange>,
    host: Option<headless::Host>,
    /// Owners a restore replaced, kept only to write their admitted replies.
    retired_hosts: Vec<(headless::Host, std::time::Instant)>,
    /// Command-line cleanup or clock confirmation running off this thread.
    remote_storage: Option<remote_storage::Job>,
    // Kept across session replacement until the shared worker drains its reply.
    host_preparing: Option<(u64, Arc<AtomicBool>)>,
    generation: generation::State,
    targets: targets::State,
    /// Recovery report and original presence for the current session.
    opened: Option<Arc<super::OpenReport>>,
    /// Persistent "Not saved" alert for the current session.
    storage: Option<crate::recovery::StorageAlert>,
    storage_watch: recovery::StorageWatch,
    /// One relink on the import worker, kept until its reply drains.
    relinking: Option<recovery::Relinking>,
    relink: Option<super::RelinkStatus>,
    storage_cleanup: Option<super::StorageCleanupStatus>,
    /// The automatic AI variant retention pass of this session.
    retention: storage::Retention,
    /// Missing linked originals whose bookmark candidate was tried this
    /// session.
    auto_relinked: std::collections::BTreeSet<deadpan_store::original_media::OriginalContentId>,
    backups: backups::State,
    #[cfg(test)]
    render_preview_refresh_failure: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    shared: Arc<Shared>,
    requests: Receiver<ProjectRequest>,
    jobs: SyncSender<Job>,
    results: Receiver<Reply>,
    worker: JoinHandle<()>,
    library: Option<ProjectLibrary>,
    backend: super::generation::Backend,
    tracking: super::targets::Backend,
) {
    let mut service = Service {
        shared,
        store: None,
        workspace: None,
        import: None,
        error: None,
        message: None,
        committed: None,
        semantic: semantic::State::default(),
        macros: None,
        saved_macro: None,
        room_tone: None,
        room_tone_error: None,
        gain: None,
        transcript_save: None,
        activity_save: None,
        shot_save: None,
        correction_save: None,
        splice: None,
        splice_commit: None,
        splice_draft: None,
        splice_seen: None,
        slip: None,
        slip_commit: None,
        saved_slip: None,
        slip_draft: None,
        slip_seen: None,
        slip_session: None,
        trim: None,
        trim_commit: None,
        saved_trim: None,
        trim_draft: None,
        trim_seen: None,
        trim_session: None,
        captured_slice: None,
        registers: None,
        captured_original: None,
        cut_slice: None,
        last_cut: None,
        marks: super::marks::Update::default(),
        marks_state: marks::State::default(),
        copied_view: None,
        splice_source_view: None,
        active: None,
        cached: None,
        session: 0,
        serial: 0,
        jobs,
        library,
        render: None,
        render_registration: None,
        render_update: None,
        render_history: None,
        pending_session_change: None,
        host: None,
        retired_hosts: Vec::new(),
        remote_storage: None,
        host_preparing: None,
        generation: generation::State::new(backend),
        targets: targets::State::new(tracking),
        opened: None,
        storage: None,
        storage_watch: Default::default(),
        relinking: None,
        relink: None,
        storage_cleanup: None,
        retention: storage::Retention::default(),
        auto_relinked: Default::default(),
        backups: backups::State::default(),
        #[cfg(test)]
        render_preview_refresh_failure: false,
    };
    let mut requests_connected = true;
    #[cfg(any(test, feature = "ui-harness"))]
    let mut held = std::collections::VecDeque::<ProjectRequest>::new();
    loop {
        let shutdown_changed = if service.shared.stopping.load(Ordering::Acquire)
            && (!service.shared.busy.load(Ordering::Acquire)
                || service.pending_session_change.is_some())
        {
            service.begin_render_shutdown()
        } else {
            false
        };
        let changed = shutdown_changed
            | service.pump_backups()
            | service.pump_render()
            | service.pump_generation()
            | service.pump_tracking()
            | service.finish_session_change()
            | service.pump_retention()
            | service.pump_remote_storage();
        if changed {
            service.reconcile_slip();
            service.reconcile_trim();
            service.publish();
        }
        service.reconcile_render_registration();
        service.pump_host();
        service.pump_host_preparation();
        if service.shared.stopping.load(Ordering::Acquire)
            && !service.shared.busy.load(Ordering::Acquire)
            && !service.shared.annotation.load(Ordering::Acquire)
            && service.pending_session_change.is_none()
            && service.render.is_none()
            && !service.generation.active()
            && !service.targets.active()
        {
            break;
        }
        let request = if requests_connected {
            requests.recv_timeout(Duration::from_millis(10))
        } else {
            std::thread::park_timeout(Duration::from_millis(10));
            Err(RecvTimeoutError::Timeout)
        };
        // Replay/tests can hold admitted requests, including one received by
        // a wait that began before the hold, until they release it. Order is
        // preserved; production never buffers. Shutdown and a disconnected
        // mailbox override the hold so a failed check cannot strand an
        // admitted command (and its busy flag) forever: buffered requests are
        // drained in order before the disconnect is observed.
        #[cfg(any(test, feature = "ui-harness"))]
        let request = {
            let request = match request {
                Ok(request) => {
                    held.push_back(request);
                    Err(RecvTimeoutError::Timeout)
                }
                error => error,
            };
            let releasing = service.shared.stopping.load(Ordering::Acquire)
                || matches!(request, Err(RecvTimeoutError::Disconnected));
            if service.shared.requests_held.load(Ordering::Acquire) && !releasing {
                Err(RecvTimeoutError::Timeout)
            } else {
                // A disconnect is observed again on the next empty receive.
                held.pop_front().map_or(request, Ok)
            }
        };
        match request {
            Ok(request) if request.is_annotation_save() => {
                // Independent feedback: each save reports only through its
                // own receipt, never a user command's error or completion.
                service.annotation_command(request);
                service.shared.annotation.store(false, Ordering::Release);
                service.invalidate_changed_splice();
                service.reconcile_slip();
                service.reconcile_trim();
                service.publish();
            }
            Ok(request) => {
                if service.dispatch_request(request) {
                    service.shared.busy.store(false, Ordering::Release);
                }
                service.invalidate_changed_splice();
                service.reconcile_slip();
                service.reconcile_trim();
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
                service.reconcile_slip();
                service.reconcile_trim();
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
    service.backup_before_session_change();
    // Revoke handles and release the lock before waiting for cooperative
    // decoding; replies already produced get a bounded chance to be written.
    service.retire_host();
    service.store = None;
    service.drain_hosts_before_exit();
    service.backups.finish_on_exit();
    service.workspace = None;
    service.cached = None;
    service.shared.busy.store(false, Ordering::Release);
    service.shared.annotation.store(false, Ordering::Release);
    let shared = service.shared.clone();
    drop(service);
    drop(results);
    let _ = worker.join();
    shared.shutdown_complete.store(true, Ordering::Release);
    (shared.wake)();
}

impl Service {
    fn publish(&mut self) {
        self.observe_semantic();
        self.observe_storage();
        let update = ProjectUpdate {
            workspace: self.workspace.clone(),
            import: self.import.clone(),
            error: self.error.clone(),
            message: self.message.clone(),
            committed: self.committed.clone(),
            semantic: self.semantic.snapshot(),
            macros: self.macros.clone(),
            saved_macro: self
                .saved_macro
                .as_ref()
                .map(|(_, receipt)| receipt.clone()),
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
            slip: self.slip.clone(),
            slip_commit: self.slip_commit.clone(),
            saved_slip: self.saved_slip.clone(),
            trim: self.trim.clone(),
            trim_commit: self.trim_commit.clone(),
            saved_trim: self.saved_trim.clone(),
            captured_slice: self.captured_slice.clone(),
            registers: self.registers.clone(),
            captured_original: self.captured_original.clone(),
            cut_slice: self.cut_slice.clone(),
            saved_cut: self
                .last_cut
                .as_ref()
                .map(|(_, _, receipt)| receipt.clone()),
            marks: self.marks.clone(),
            render: self.render_update.clone(),
            render_history: self.render_history.clone(),
            transcript_save: self.transcript_save.clone(),
            activity_save: self.activity_save.clone(),
            shot_save: self.shot_save.clone(),
            correction_save: self.correction_save.clone(),
            generation: self.generation_update(),
            targets: self.targets_update(),
            opened: self.opened.clone().filter(|report| {
                self.workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.session == report.session)
            }),
            storage: self.current_storage_alert(),
            relink: self.relink.clone(),
            storage_cleanup: self.storage_cleanup.clone(),
            storage_retention: self.retention.status(self.session),
            backups: self.backups.update(),
        };
        *self
            .shared
            .update
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(update);
        (self.shared.wake)();
    }

    /// Analysis saves record success or failure in their own receipt.
    fn annotation_command(&mut self, request: ProjectRequest) {
        match request {
            ProjectRequest::SaveTranscript {
                expected_session,
                attempt,
                key,
                transcript,
            } => self.save_transcript_command(expected_session, attempt, key, transcript),
            ProjectRequest::SaveSpeechActivity {
                expected_session,
                attempt,
                key,
                activity,
            } => self.save_speech_activity_command(expected_session, attempt, key, activity),
            ProjectRequest::SaveShotAnalysis {
                expected_session,
                attempt,
                key,
                analysis,
            } => self.save_shot_analysis_command(expected_session, attempt, key, analysis),
            ProjectRequest::SaveShotProgress {
                expected_session,
                key,
                progress,
            } => self.save_shot_progress_command(expected_session, key, progress),
            _ => unreachable!("only analysis saves use the annotation lane"),
        }
    }

    fn command(&mut self, request: ProjectRequest) -> Result<()> {
        let request = match request {
            ProjectRequest::Marks(request) => {
                self.marks_command(request);
                return Ok(());
            }
            ProjectRequest::SaveTranscript {
                expected_session,
                attempt,
                key,
                transcript,
            } => {
                self.save_transcript_command(expected_session, attempt, key, transcript);
                return Ok(());
            }
            ProjectRequest::SaveSpeechActivity {
                expected_session,
                attempt,
                key,
                activity,
            } => {
                self.save_speech_activity_command(expected_session, attempt, key, activity);
                return Ok(());
            }
            ProjectRequest::SaveShotAnalysis {
                expected_session,
                attempt,
                key,
                analysis,
            } => {
                self.save_shot_analysis_command(expected_session, attempt, key, analysis);
                return Ok(());
            }
            ProjectRequest::SaveShotProgress {
                expected_session,
                key,
                progress,
            } => {
                self.save_shot_progress_command(expected_session, key, progress);
                return Ok(());
            }
            ProjectRequest::ChangeCorrections(request) => {
                self.change_corrections_command(request);
                return Ok(());
            }
            ProjectRequest::CaptureEditSlice(request) => {
                self.capture_edit_slice_command(request);
                return Ok(());
            }
            ProjectRequest::CaptureOriginal(request) => {
                self.capture_original_command(request);
                return Ok(());
            }
            ProjectRequest::CutEditSlice(request) => {
                self.cut_edit_slice_command(request, None);
                return Ok(());
            }
            ProjectRequest::CutFrames { capture, attempt } => {
                self.cut_edit_slice_command(capture, Some(attempt));
                return Ok(());
            }
            ProjectRequest::Macro(operation) => {
                self.macro_command(operation);
                return Ok(());
            }
            ProjectRequest::PrepareTrim(proposal) => {
                self.prepare_trim_command(proposal);
                return Ok(());
            }
            ProjectRequest::CommitTrim(id) => {
                self.commit_trim_command(id);
                return Ok(());
            }
            ProjectRequest::AbandonTrim(id) => {
                self.abandon_trim(&id);
                return Ok(());
            }
            ProjectRequest::PrepareSlip(proposal) => {
                self.prepare_slip_command(proposal);
                return Ok(());
            }
            ProjectRequest::CommitSlip(id) => {
                self.commit_slip_command(id);
                return Ok(());
            }
            ProjectRequest::AbandonSlip(id) => {
                self.abandon_slip(&id);
                return Ok(());
            }
            ProjectRequest::PrepareSplice(proposal) => {
                self.prepare_splice_command(proposal);
                return Ok(());
            }
            ProjectRequest::CommitSplice(id) => return self.commit_splice_command(id),
            ProjectRequest::AbandonSplice(id) => {
                self.abandon_splice(&id);
                return Ok(());
            }
            ProjectRequest::Generation(operation)
                if !matches!(
                    operation,
                    super::generation::GenerationOperation::Accept { .. }
                ) =>
            {
                self.generation_command(operation);
                return Ok(());
            }
            ProjectRequest::Target(operation) => {
                self.target_command(operation);
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
            ProjectRequest::Marks(_) => unreachable!("marks use independent feedback"),
            ProjectRequest::Backup(request) => {
                // Normally answered before dispatch; kept for direct callers.
                self.backup_command(request);
                Ok(())
            }
            ProjectRequest::SaveTranscript { .. }
            | ProjectRequest::SaveSpeechActivity { .. }
            | ProjectRequest::SaveShotAnalysis { .. }
            | ProjectRequest::SaveShotProgress { .. }
            | ProjectRequest::ChangeCorrections(_) => {
                unreachable!("analysis annotations use independent feedback")
            }
            ProjectRequest::Render(_) => unreachable!("render commands use their own feedback"),
            ProjectRequest::Target(_) => unreachable!("targets use independent feedback"),
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
                interpretation,
                ownership,
            } => {
                self.check_context(expected_session, &expected_revision)?;
                self.import(path, ImportMedia::sound(stream, interpretation), ownership)
            }
            #[cfg(test)]
            ProjectRequest::Create(path) => self.open(path, true),
            ProjectRequest::Open(path) => self.open(path, false),
            ProjectRequest::AcknowledgeRecovery { expected_session } => {
                if self.session == expected_session && self.workspace.is_some() {
                    self.writer()?.acknowledge_recovery().map_err(display)?;
                }
                Ok(())
            }
            ProjectRequest::RelinkOriginal {
                ticket,
                expected_session,
                content,
                expected_version,
                path,
            } => self.relink_original(ticket, expected_session, &content, expected_version, path),
            ProjectRequest::CleanStorage {
                ticket,
                expected_session,
                previewed,
            } => {
                self.clean_storage(ticket, expected_session, &previewed);
                Ok(())
            }
            ProjectRequest::ConfirmVariantClock {
                ticket,
                expected_session,
                expected_revision,
                plan,
            } => {
                self.confirm_variant_clock(ticket, expected_session, &expected_revision, &plan);
                Ok(())
            }
            ProjectRequest::Close => {
                self.backup_before_session_change();
                self.cancel();
                self.host = None;
                self.store = None;
                self.workspace = None;
                self.cached = None;
                self.clear_copied_slice();
                self.clear_marks();
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
            ProjectRequest::Generation(operation) => self.accept_generation(operation),
            ProjectRequest::PasteEditedSlice(request) => self.paste_edited_slice(request),
            ProjectRequest::CaptureEditSlice(_) => unreachable!("copy uses independent feedback"),
            ProjectRequest::CaptureOriginal(_) => unreachable!("copy uses independent feedback"),
            ProjectRequest::CutEditSlice(_) => unreachable!("cut uses independent feedback"),
            ProjectRequest::CutFrames { .. } => unreachable!("frame cut uses independent feedback"),
            ProjectRequest::Macro(_) => unreachable!("macro uses independent feedback"),
            ProjectRequest::PrepareSplice(_)
            | ProjectRequest::CommitSplice(_)
            | ProjectRequest::AbandonSplice(_) => unreachable!("splice uses independent feedback"),
            ProjectRequest::PrepareTrim(_)
            | ProjectRequest::CommitTrim(_)
            | ProjectRequest::AbandonTrim(_) => unreachable!("Trim uses independent feedback"),
            ProjectRequest::PrepareSlip(_)
            | ProjectRequest::CommitSlip(_)
            | ProjectRequest::AbandonSlip(_) => unreachable!("Slip uses independent feedback"),
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
                let outcome = self
                    .writer()?
                    .undo(&expected_revision, revision())
                    .map_err(display)?;
                self.preserve_semantic(&expected_revision, &outcome.revision_id);
                self.refresh_saved("Undo saved")?;
                self.message = Some("Undo saved".into());
                Ok(())
            }
            ProjectRequest::Redo { expected_revision } => {
                let outcome = self
                    .writer()?
                    .redo(&expected_revision, revision())
                    .map_err(display)?;
                self.preserve_semantic(&expected_revision, &outcome.revision_id);
                self.refresh_saved("Redo saved")?;
                self.message = Some("Redo saved".into());
                Ok(())
            }
        }
    }

    /// Why a request cannot run in a read-only session (a newer package
    /// opened for viewing). Session changes and cancellations always can.
    fn read_only_refusal(&self, request: &ProjectRequest) -> Option<String> {
        let reason = self.workspace.as_ref()?.read_only.as_ref()?;
        if matches!(
            request,
            ProjectRequest::Open(_)
                | ProjectRequest::Close
                | ProjectRequest::CreateFromSource { .. }
                | ProjectRequest::CancelImport
                | ProjectRequest::AbandonSplice(_)
                | ProjectRequest::AbandonSlip(_)
                | ProjectRequest::AbandonTrim(_)
        ) {
            return None;
        }
        Some(format!("Not saved: {reason}"))
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
            ProjectSoundEdit::Cut { id, at } => {
                let event = super::sound::cut(workspace, &id, at)?;
                (
                    Command::SetSound {
                        id: id.clone(),
                        event,
                    },
                    Some(id),
                    "Sound cut at the Edit cursor and saved",
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
        self.committed = Some(CommittedEdit {
            scoped: None,
            revision: outcome.revision_id,
            selected_node: None,
            preserve_cursor: true,
            cursor: None,
            scope: SequenceScope::default(),
            sound: Some(SoundCommit { selected }),
            range_selection: None,
        });
        self.refresh_saved(message)?;
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
        // Edits keep pending AI requests reconciled instead of refusing, as
        // for an opened project.
        store.set_generation_context_resolver(std::sync::Arc::new(
            deadpan_cli::generation_context::BoundaryContextResolver::default(),
        ));
        let next = self
            .session
            .checked_add(1)
            .ok_or("Project session identities exhausted")?;
        let workspace = snapshot(&store, next, package.canonicalize().map_err(display)?, None)?;
        let registers = registers::restore(&store, next)?;
        let report = Arc::new(recovery::open_report(&store, &workspace));
        let host = headless::Host::bind(&mut store)?;
        self.backup_before_session_change();
        self.cancel();
        self.host = Some(host);
        self.store = Some(store);
        self.workspace = Some(Arc::new(workspace));
        self.session = next;
        self.opened = Some(report);
        self.storage = None;
        self.storage_watch = Default::default();
        self.cached = None;
        self.clear_copied_slice();
        self.registers = Some(registers);
        self.clear_marks();
        self.import = None;
        self.begin_backup_session();
        self.retention.begin_session(next);
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
        if let ProjectEdit::Scoped { target, edit } = edit {
            target.validate_request(
                workspace,
                expected_session,
                &expected_revision,
                &scope,
                cursor,
            )?;
            return self.edit_scoped(*target, edit);
        }
        if let ProjectEdit::DeleteRange { parent, range } = edit {
            return self.delete_range(expected_revision, scope, parent, range);
        }
        let black = matches!(edit, ProjectEdit::InsertBlack { .. });
        if let ProjectEdit::InsertTime { at, duration }
        | ProjectEdit::InsertBlack { at, duration } = edit
        {
            if duration == deadpan_core::FrameDuration::ZERO {
                self.message = Some("Pause resolves to 0 frames; no edit was made.".into());
                return Ok(());
            }
            scope.check_pause(workspace, at)?;
            let id = node();
            let request = super::pause::prepare(
                workspace,
                at,
                duration,
                revision(),
                id.clone(),
                black,
                node,
            )?;
            // As for every native edit, current generation requests require the
            // real host relevance resolver. Never invent observations here.
            let outcome = self.writer()?.commit(&request).map_err(display)?;
            self.committed = Some(CommittedEdit {
                scoped: None,
                revision: outcome.revision_id,
                selected_node: Some(id),
                preserve_cursor: false,
                cursor: Some(at),
                scope,
                sound: None,
                range_selection: None,
            });
            self.refresh()?;
            self.message = Some(format!(
                "Inserted a {} frame silent {} at boundary {} and saved",
                duration.frames(),
                if black { "black pause" } else { "pause" },
                at.0
            ));
            return Ok(());
        }
        let target = match &edit {
            ProjectEdit::Scoped { .. }
            | ProjectEdit::InsertTime { .. }
            | ProjectEdit::InsertBlack { .. }
            | ProjectEdit::DeleteRange { .. } => {
                unreachable!("range operation handled above")
            }
            ProjectEdit::Split { node, .. }
            | ProjectEdit::Repeat { node, .. }
            | ProjectEdit::WrapRepeat { node, .. }
            | ProjectEdit::SetCutaways { node, .. }
            | ProjectEdit::SetCaptions { node, .. }
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
                | ProjectEdit::SetCutaways { .. }
                | ProjectEdit::SetCaptions { .. }
        );
        let mut retime_message = None;
        let new_revision = revision();
        let repeat_intent = match &edit {
            ProjectEdit::WrapRepeat { plays, .. } => Some(super::semantic::LastEdit {
                operation: super::semantic::RepeatableEdit::Repeat {
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                    plays: std::num::NonZeroU32::new(*plays)
                        .ok_or("A Repeat needs at least one total play")?,
                    escalation: None,
                },
                register: None,
            }),
            _ => None,
        };
        let wrap_repeat = |target: NodeId, plays| {
            let selection = deadpan_core::SliceCaptureSelection::Child { node: target };
            let plan = document
                .repeat_selection(view.owner, &selection, plays)
                .map_err(display)?;
            let id = node();
            Ok::<_, String>((
                Command::RepeatSelection {
                    parent: view.owner.clone(),
                    selection,
                    plays,
                    identities: deadpan_core::RepeatSelectionIdentities {
                        repeat: id.clone(),
                        group: plan.needs_group.then(node),
                        split: deadpan_core::SplitIdentities {
                            nodes: (0..plan.required_split_ids).map(|_| node()).collect(),
                        },
                    },
                    timing: deadpan_core::AudioTimingId {
                        allocation: new_revision.clone(),
                        ordinal: 0,
                    },
                },
                Some(id),
                "Repeat created and saved",
            ))
        };
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
            ProjectEdit::SetFraming { node, framing } => {
                retime_message = Some(format!(
                    "Framing saved: {}",
                    crate::navigation::zoom::describe(framing.as_ref(), &|id| {
                        document
                            .targets()
                            .get(id)
                            .map_or_else(|| id.as_str().to_owned(), |target| target.label.clone())
                    })
                ));
                (
                    Command::SetFraming { node, framing },
                    selected,
                    "Framing updated and saved",
                )
            }
            ProjectEdit::SetCutaways {
                node,
                host,
                cutaways,
            } => {
                if deadpan_core::cutaway_host(document, &node).map(|(host, _)| host)
                    != Some(host.clone())
                {
                    return Err("The cutaway host is no longer under the selected beat.".into());
                }
                (
                    Command::SetCutaways {
                        node: host,
                        cutaways,
                    },
                    selected,
                    "Cutaways updated and saved",
                )
            }
            ProjectEdit::SetCaptions {
                node,
                host,
                captions,
            } => {
                if deadpan_core::cutaway_host(document, &node).map(|(host, _)| host)
                    != Some(host.clone())
                {
                    return Err("The caption host is no longer under the selected beat.".into());
                }
                (
                    Command::SetCaptions {
                        node: host,
                        captions,
                    },
                    selected,
                    "Captions updated and saved",
                )
            }
            ProjectEdit::Scoped { .. }
            | ProjectEdit::InsertTime { .. }
            | ProjectEdit::InsertBlack { .. }
            | ProjectEdit::DeleteRange { .. } => {
                unreachable!("range operation handled above")
            }
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
                if let NodeKind::Repeat { .. } = &document.nodes()[&target].kind {
                    (
                        Command::SetRepeatPlays {
                            node: target,
                            plays,
                            timing: deadpan_core::AudioTimingId {
                                allocation: new_revision.clone(),
                                ordinal: 0,
                            },
                        },
                        selected,
                        "Repeat updated and saved",
                    )
                } else {
                    wrap_repeat(target, plays)?
                }
            }
            ProjectEdit::WrapRepeat {
                node: target,
                plays,
            } => wrap_repeat(target, plays)?,
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
                (
                    Command::DeleteRipple {
                        node,
                        timing: deadpan_core::AudioTimingId {
                            allocation: new_revision.clone(),
                            ordinal: 0,
                        },
                    },
                    selected,
                    "Beat deleted and saved",
                )
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
            new_revision,
            command,
        };
        // Generic commit deliberately preserves the store's relevance guard.
        // An unresolved active generation request must fail rather than receive
        // invented observations from a widget or this service.
        let outcome = self.writer()?.commit(&request).map_err(display)?;
        if let Some(intent) = repeat_intent {
            self.semantic.prove(
                expected_session,
                &request.project_id,
                &request.expected_revision,
                &outcome.revision_id,
                semantic::Change::Replace(intent),
            );
        }
        // Retain the actual durable receipt even if rebuilding the workspace
        // fails. Render continuation must report this commit independently of
        // its later admission result.
        self.committed = Some(CommittedEdit {
            scoped: None,
            revision: outcome.revision_id,
            selected_node: selected_node.clone(),
            preserve_cursor,
            cursor: preserve_cursor.then_some(cursor),
            scope: scope.clone(),
            sound: None,
            range_selection: None,
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
                // A newer Deadpan saved it: view it read-only, never rewrite.
                Err(StoreError::NewerSchema { .. }) => {
                    ProjectStore::open(&path, AccessMode::ReadOnly)
                }
                result => result,
            }
        }
        .map_err(display)?;
        let mut store = store;
        // Edits keep pending AI requests reconciled instead of refusing.
        store.set_generation_context_resolver(std::sync::Arc::new(
            deadpan_cli::generation_context::BoundaryContextResolver::default(),
        ));
        let next = self
            .session
            .checked_add(1)
            .ok_or("Project session identities exhausted")?;
        let workspace = snapshot(&store, next, path.canonicalize().map_err(display)?, None)?;
        let registers = registers::restore(&store, next)?;
        let report = Arc::new(recovery::open_report(&store, &workspace));
        let message = match migration {
            Some(migration) if migration.backup.is_some() => format!(
                "Project opened. Upgraded schema {} to {}; original database backup: {}",
                migration.from_schema,
                migration.to_schema,
                migration.backup.as_ref().expect("backup checked").display(),
            ),
            _ if create => "Project created".into(),
            _ if workspace.read_only.is_some() => {
                "Opened read-only: a newer Deadpan saved this project. You can look at it; nothing can be saved here.".into()
            }
            _ => "Project opened".into(),
        };
        Ok(Some(render::PreparedOpen {
            store,
            workspace,
            registers,
            message,
            report,
        }))
    }

    fn install_open(&mut self, mut prepared: render::PreparedOpen) -> Result<()> {
        // A pending Open is unadvertised until installation. Bind before
        // replacing the old session so a failed endpoint keeps that session.
        // A read-only view has no writer, so no command endpoint either.
        let host = if prepared.store.access_mode() == AccessMode::ReadWrite {
            Some(headless::Host::bind(&mut prepared.store)?)
        } else {
            None
        };
        self.backup_before_session_change();
        self.cancel();
        self.host = host;
        self.store = Some(prepared.store);
        self.session = prepared.workspace.session;
        self.workspace = Some(Arc::new(prepared.workspace));
        self.cached = None;
        self.clear_copied_slice();
        self.registers = Some(prepared.registers);
        self.import = None;
        self.message = Some(prepared.message);
        self.opened = Some(prepared.report);
        self.storage = None;
        self.storage_watch = Default::default();
        self.clear_marks();
        self.begin_backup_session();
        self.retention.begin_session(self.session);
        self.auto_relinked.clear();
        self.relink_moved_originals();
        Ok(())
    }

    /// The writer has already committed. Refreshing cannot retract that result
    /// or report the authored operation as unsaved.
    fn refresh_saved(&mut self, saved: &str) -> Result<()> {
        self.refresh().map_err(|error| {
            format!(
                "{saved}, but the workspace could not refresh: {error}. Reopen this project before editing or undoing."
            )
        })
    }

    fn refresh(&mut self) -> Result<()> {
        #[cfg(test)]
        if self
            .shared
            .workspace_refresh_failure
            .swap(false, Ordering::AcqRel)
        {
            return Err("Injected failure refreshing the saved workspace".into());
        }
        // A history-neutral host write can update copies without a new revision.
        self.refresh_registers()?;
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
        self.reconcile_slip();
        self.reconcile_trim();
        Ok(())
    }

    fn cancel(&mut self) {
        self.invalidate_splice("Slice proposal was cancelled");
        self.invalidate_slip("Slip proposal was cancelled.");
        self.invalidate_trim("Trim proposal was cancelled.");
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
                        self.committed = outcome.commit.and_then(|commit| {
                            registration
                                .insertion
                                .as_ref()
                                .map(|insertion| CommittedEdit {
                                    scoped: None,
                                    revision: commit.revision_id,
                                    selected_node: Some(insertion.node.clone()),
                                    preserve_cursor: false,
                                    cursor: None,
                                    scope: scope.clone(),
                                    sound: None,
                                    range_selection: None,
                                })
                        });
                        self.refresh_saved("Source inserted and saved")?;
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
            interpretation: source.receipt.snapshot().audio_interpretation(),
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
        let Some(reply) = self.relink_result(reply) else {
            return;
        };
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
            Ok(Prepared::Host(_) | Prepared::Restored(_) | Prepared::Relinked(_)) => {
                Err("Import worker returned an unrelated operation".into())
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
            self.committed = outcome.commit.map(|commit| CommittedEdit {
                scoped: None,
                revision: commit.revision_id,
                selected_node: Some(initialization.node),
                preserve_cursor: false,
                cursor: None,
                scope: active.scope.clone(),
                sound: None,
                range_selection: None,
            });
            if let Some(status) = &mut self.import {
                status.asset = Some(outcome.asset_id.clone());
            }
            self.refresh_saved("Original saved with the full video on Your edit")?;
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
        if let (Some(insertion), Some(commit)) = (&registration.insertion, &outcome.commit) {
            self.committed = Some(CommittedEdit {
                scoped: None,
                revision: commit.revision_id.clone(),
                selected_node: Some(insertion.node.clone()),
                preserve_cursor: false,
                cursor: None,
                scope: active.scope.clone(),
                sound: None,
                range_selection: None,
            });
        }
        let saved = if is_insertion {
            "Source inserted and saved"
        } else if is_sound_catalog {
            "Sound added to the catalog"
        } else {
            "Source registered and saved"
        };
        if let Some(status) = &mut self.import {
            status.asset = Some(outcome.asset_id.clone());
        }
        self.refresh_saved(saved)?;
        if let Some(status) = &mut self.import {
            status.stage = ImportStage::Complete;
            status.asset = Some(outcome.asset_id);
        }
        self.message = Some(
            if !is_insertion && !is_sound_catalog {
                "Source registered; ready for explicit insertion"
            } else {
                saved
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
    // The store retains the head's validation, so compiling inside its scope
    // does not validate the whole document again after every commit.
    let validated = store.snapshot_validated().map_err(display)?;
    let plan = validated
        .scope(|| RenderPlan::compile(&validated))
        .map_err(display)?;
    let document = ProjectDocument::clone(&validated);
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
        sources.insert(
            asset.clone(),
            registered_source(asset, metadata, rate, receipt, original)?,
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
    let (transcript, speech_activity, shot_analysis, corrections) = match previous {
        Some(previous) => (
            previous.transcript.clone(),
            previous.speech_activity.clone(),
            previous.shot_analysis.clone(),
            // The Original can become ready after the first workspace.
            previous
                .corrections
                .clone()
                .or_else(|| original_corrections(store, single_source.as_ref(), &sources)),
        ),
        None => (
            original_transcript(store, single_source.as_ref(), &sources),
            original_activity(store, single_source.as_ref(), &sources),
            original_shots(store, single_source.as_ref(), &sources),
            original_corrections(store, single_source.as_ref(), &sources),
        ),
    };
    let color = store.output_color(&document).map_err(display)?;
    let read_only = store.newer_schema().map(|found| {
        Arc::<str>::from(
            StoreError::NewerSchema {
                found,
                supported: deadpan_store::DATABASE_SCHEMA_VERSION,
            }
            .to_string(),
        )
    });
    Ok(Workspace {
        read_only,
        color,
        session,
        path,
        document: Arc::new(document),
        plan: Arc::new(plan),
        sources,
        originals: if store.access_mode() == AccessMode::ReadWrite {
            store.original_import_handle().map_err(display)?
        } else {
            store.original_view_handle()
        },
        generated: store.generated_read_handle(),
        can_undo,
        can_redo,
        single_source,
        original_duration,
        transcript,
        speech_activity,
        shot_analysis,
        corrections,
    })
}

/// The Original's stored transcript, preferring the installed transcription
/// pack's model. A damaged stored transcript is reported, not hidden.
/// The preferred stored transcript of the ready Original. An unreadable one
/// is skipped; without any, the app transcribes again and replaces it.
fn original_transcript(
    store: &ProjectStore,
    single_source: Option<&SingleSourceState>,
    sources: &BTreeMap<AssetId, Arc<RegisteredSource>>,
) -> Option<Arc<super::OriginalTranscript>> {
    let Some(SingleSourceState::Ready { asset, .. }) = single_source else {
        return None;
    };
    let source = sources.get(asset)?;
    let content = source.receipt.original().content().to_string();
    let words = deadpan_cli::speech::stored_words(store, &content)?;
    Some(Arc::new(super::OriginalTranscript::new(words)))
}

/// The ready Original's corrections, keyed by its qualified audio stream.
/// Unreadable corrections are reported with the workspace, never dropped.
fn original_corrections(
    store: &ProjectStore,
    single_source: Option<&SingleSourceState>,
    sources: &BTreeMap<AssetId, Arc<RegisteredSource>>,
) -> Option<Arc<super::OriginalCorrections>> {
    let Some(SingleSourceState::Ready { asset, .. }) = single_source else {
        return None;
    };
    let receipt = &sources.get(asset)?.receipt;
    let stream = receipt.snapshot().audio()?.stream().stream_index;
    let key =
        deadpan_cli::speech::corrections_key(&receipt.original().content().to_string(), stream);
    let (stored, mut error) = match store.analysis_corrections(&key) {
        Ok(stored) => (stored, None),
        Err(error) => (None, Some(error.to_string())),
    };
    // Unreadable Undo or Redo steps are reported too, so they are discarded
    // explicitly rather than found by a failing Undo.
    if error.is_none() {
        error = match store.unreadable_analysis_corrections() {
            Ok(unreadable) => unreadable
                .iter()
                .find(|bad| bad.key == key)
                .map(|bad| format!("a stored {} step is unreadable: {}", bad.place, bad.error)),
            Err(failure) => Some(failure.to_string()),
        };
    }
    Some(Arc::new(super::OriginalCorrections { key, stored, error }))
}

/// The preferred stored speech activity of the ready Original. An unreadable
/// row is skipped; without any, the app detects speech again.
fn original_activity(
    store: &ProjectStore,
    single_source: Option<&SingleSourceState>,
    sources: &BTreeMap<AssetId, Arc<RegisteredSource>>,
) -> Option<Arc<super::OriginalActivity>> {
    let Some(SingleSourceState::Ready { asset, .. }) = single_source else {
        return None;
    };
    let source = sources.get(asset)?;
    let content = source.receipt.original().content().to_string();
    let (key, activity) = deadpan_cli::activity::stored_activity(store, &content)?;
    let (pauses, corrections_error) =
        deadpan_cli::speech::corrected_pauses(store, &content, key.audio_stream, &activity);
    Some(Arc::new(super::OriginalActivity {
        key,
        activity,
        pauses,
        corrections_error,
    }))
}

/// The stored shot analysis of the ready Original under the current
/// signature, checked against its qualified picture count. An unreadable or
/// mismatched row is skipped; without one, the app scans again.
fn original_shots(
    store: &ProjectStore,
    single_source: Option<&SingleSourceState>,
    sources: &BTreeMap<AssetId, Arc<RegisteredSource>>,
) -> Option<Arc<super::OriginalShots>> {
    let Some(SingleSourceState::Ready { asset, .. }) = single_source else {
        return None;
    };
    let receipt = &sources.get(asset)?.receipt;
    let video = receipt.snapshot().video()?;
    let pictures = video.index().index().frames().len();
    let content = receipt.original().content().to_string();
    let (key, analysis) =
        deadpan_cli::shots::stored_shots(store, &content, video.index().stream_index(), pictures)?;
    Some(Arc::new(super::OriginalShots { key, analysis }))
}

fn registered_source(
    asset: &AssetId,
    metadata: &deadpan_core::AssetRecord,
    rate: deadpan_core::FrameRate,
    receipt: Arc<deadpan_store::source_registration::SourceQualificationReceipt>,
    original: OriginalMediaRecord,
) -> Result<Arc<RegisteredSource>> {
    if receipt
        .asset_record(metadata.label.clone())
        .map_err(display)?
        != *metadata
        || original.object() != receipt.original()
        || original.sha256() != receipt.snapshot().content().sha256()
    {
        return Err("Source catalog differs from its admitted media".into());
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
    let sound_audition = if receipt.snapshot().video().is_none()
        && receipt.snapshot().audio().is_some()
    {
        Some(Arc::new(
            deadpan_playback::Sound::new(rate, asset.clone(), receipt.clone()).map_err(display)?,
        ))
    } else {
        None
    };
    Ok(Arc::new(RegisteredSource {
        asset: asset.clone(),
        label: metadata.label.clone(),
        receipt,
        original,
        video_index,
        original_audition,
        sound_audition,
    }))
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

/// Store failures get their person-facing explanation and suggested action;
/// disk-full and permission failures also raise the persistent alert.
fn display<E: std::fmt::Display + 'static>(error: E) -> String {
    if let Some(store) = (&error as &dyn std::any::Any).downcast_ref::<StoreError>() {
        return crate::recovery::describe_store_error(store);
    }
    error.to_string()
}
