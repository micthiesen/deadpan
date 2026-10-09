//! Read-only admission and each native command's normal failure channel.
//!
//! Enqueueing a command may already put its UI into a pending state. A global
//! diagnostic cannot finish that state: preserve the submitted identity in the
//! same reply the writable handler would publish. Never recapture a target.

use super::*;
use crate::project::{self, generation::GenerationOperation, targets::Operation as Target};

impl Service {
    fn read_only_refusal(&self, request: &ProjectRequest) -> Option<String> {
        let reason = self.workspace.as_ref()?.read_only.as_ref()?;
        if matches!(
            request,
            ProjectRequest::Open(_)
                | ProjectRequest::Close
                | ProjectRequest::CreateFromSource { .. }
                | ProjectRequest::CancelImport
                | ProjectRequest::Takes(project::takes::Request {
                    operation: project::takes::Operation::List,
                    ..
                })
                | ProjectRequest::Backup(
                    project::backups::Request::LoadSettings { .. }
                        | project::backups::Request::SaveSettings { .. }
                )
                | ProjectRequest::Marks(project::marks::Request {
                    operation: project::marks::Operation::Jump { .. },
                    ..
                })
                | ProjectRequest::Target(Target::DetectFaces { .. } | Target::Cancel { .. })
                | ProjectRequest::PrepareRoomTone { .. }
                | ProjectRequest::PrepareGain(_)
                | ProjectRequest::PrepareSlip(_)
                | ProjectRequest::PrepareTrim(_)
                | ProjectRequest::AbandonSplice(_)
                | ProjectRequest::AbandonSlip(_)
                | ProjectRequest::AbandonTrim(_)
        ) {
            // These read snapshots or prepare private previews. Target Cancel
            // only signals its owned tracking job. Backup settings live outside
            // the package. Place Slice still requires store writer admission;
            // Generation Preview persists selection and is also refused.
            return None;
        }
        Some(format!("Not saved: {reason}"))
    }

    pub(super) fn refuse_read_only(&mut self, request: &ProjectRequest) -> bool {
        let Some(reason) = self.read_only_refusal(request) else {
            return false;
        };
        match request {
            ProjectRequest::Backup(
                project::backups::Request::Now { ticket, .. }
                | project::backups::Request::Restore { ticket, .. },
            ) => self.answer_backup(*ticket, Err(reason.clone())),
            ProjectRequest::Render(request) => {
                self.render_update
                    .get_or_insert_with(Default::default)
                    .command = Some(project::ProjectRenderCommandOutcome {
                    ticket: request.ticket,
                    context: request.context.clone(),
                    committed_revision: None,
                    result: Err(project::ProjectRenderError {
                        code: "RenderReadOnly",
                        message: reason.clone(),
                    }),
                });
            }
            ProjectRequest::Takes(request) => {
                self.takes = Some(project::takes::Update {
                    ticket: request.ticket,
                    session: request.session,
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::RenderHistory(request) => {
                self.render_history = Some(project::render_history::Update {
                    ticket: request.ticket,
                    context: request.context.clone(),
                    query: request.query.clone(),
                    result: Err(project::ProjectRenderError {
                        code: "RenderHistoryReadOnly",
                        message: reason.clone(),
                    }),
                });
            }
            ProjectRequest::Marks(request) => {
                self.marks.reply = Some(project::marks::Reply {
                    id: request.id.clone(),
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::Macro(operation) => {
                self.macros = Some(project::macros::Update {
                    id: operation.id().clone(),
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::ChangeCorrections(request) => {
                self.correction_save = Some(project::TranscriptSave {
                    session: request.expected_session,
                    attempt: request.attempt,
                    error: Some(reason.clone()),
                });
            }
            ProjectRequest::Target(
                Target::Save { ticket, .. }
                | Target::Track { ticket, .. }
                | Target::SaveFramed { ticket, .. },
            ) => self.refuse_target(*ticket, reason.clone()),
            ProjectRequest::Generation(
                GenerationOperation::Start { ticket, .. }
                | GenerationOperation::Cancel { ticket, .. }
                | GenerationOperation::Select { ticket, .. }
                | GenerationOperation::Preview { ticket, .. }
                | GenerationOperation::Discard { ticket, .. }
                | GenerationOperation::Keep { ticket, .. }
                | GenerationOperation::RetryPreparation { ticket, .. }
                | GenerationOperation::DiscardPreparation { ticket, .. }
                | GenerationOperation::DismissInterrupted { ticket, .. },
            ) => self.refuse_generation(*ticket, reason.clone()),
            ProjectRequest::CaptureOriginal(request) => {
                self.captured_original = Some(project::registers::OriginalUpdate {
                    id: request.id.clone(),
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::CaptureEditSlice(request) => {
                self.captured_slice = Some(project::slice::CaptureUpdate {
                    id: request.id.clone(),
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::CutEditSlice(request)
            | ProjectRequest::CutFrames {
                capture: request, ..
            } => {
                self.cut_slice = Some(project::slice::CutUpdate {
                    request: request.clone(),
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::PrepareSplice(proposal) => {
                self.splice = Some(project::splice::ProposalUpdate {
                    id: proposal.id.clone(),
                    source_view: None,
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::CommitSplice(id) => {
                self.splice_commit = Some(project::splice::SpliceCommitUpdate {
                    id: id.clone(),
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::CommitSlip(id) => {
                self.slip_commit = Some(project::slip::CommitUpdate {
                    id: id.clone(),
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::CommitTrim(id) => {
                self.trim_commit = Some(project::trim::CommitUpdate {
                    id: id.clone(),
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::CleanStorage {
                ticket,
                expected_session,
                ..
            } => {
                self.storage_cleanup = Some(project::StorageCleanupStatus {
                    ticket: *ticket,
                    session: *expected_session,
                    result: Err(reason.clone()),
                });
            }
            ProjectRequest::RelinkOriginal {
                ticket,
                expected_session,
                content,
                ..
            } => {
                let label = self
                    .workspace
                    .as_ref()
                    .filter(|workspace| workspace.session == *expected_session)
                    .and_then(|workspace| {
                        workspace
                            .sources
                            .values()
                            .find(|source| source.original.object().content() == content)
                    })
                    .map_or_else(|| "Original".into(), |source| source.label.clone());
                self.relink = Some(project::RelinkStatus {
                    ticket: *ticket,
                    session: *expected_session,
                    label,
                    state: project::RelinkState::Failed(reason.clone()),
                });
            }
            // These commands either have no independent pending reducer or
            // use the separate annotation-save lane. Keep this exhaustive so
            // a new request must make an explicit completion-policy choice.
            ProjectRequest::AcknowledgeRecovery { .. }
            | ProjectRequest::InitializeSource { .. }
            | ProjectRequest::ImportSound { .. }
            | ProjectRequest::Import { .. }
            | ProjectRequest::Insert { .. }
            | ProjectRequest::PasteMoment(_)
            | ProjectRequest::PasteEditedSlice(_)
            | ProjectRequest::ConfirmVariantClock { .. }
            | ProjectRequest::Generation(GenerationOperation::Accept { .. })
            | ProjectRequest::Edit { .. }
            | ProjectRequest::SoundEdit { .. }
            | ProjectRequest::Undo { .. }
            | ProjectRequest::Redo { .. }
            | ProjectRequest::SaveTranscript { .. }
            | ProjectRequest::SaveSpeechActivity { .. }
            | ProjectRequest::SaveShotAnalysis { .. }
            | ProjectRequest::SaveShotProgress { .. } => {}
            // Admitted above. Listing them also makes allowlist changes
            // reviewable alongside the terminal-response policy.
            ProjectRequest::Open(_)
            | ProjectRequest::Close
            | ProjectRequest::CreateFromSource { .. }
            | ProjectRequest::CancelImport
            | ProjectRequest::Backup(
                project::backups::Request::LoadSettings { .. }
                | project::backups::Request::SaveSettings { .. },
            )
            | ProjectRequest::Target(Target::DetectFaces { .. } | Target::Cancel { .. })
            | ProjectRequest::PrepareRoomTone { .. }
            | ProjectRequest::PrepareGain(_)
            | ProjectRequest::PrepareSlip(_)
            | ProjectRequest::PrepareTrim(_)
            | ProjectRequest::AbandonSplice(_)
            | ProjectRequest::AbandonSlip(_)
            | ProjectRequest::AbandonTrim(_) => unreachable!("read-only operation was admitted"),
            #[cfg(test)]
            ProjectRequest::Create(_) => {}
        }
        self.error = Some(reason);
        self.message = None;
        true
    }
}
