use super::*;

pub(super) struct Command {
    pub identity: WorkflowIdentity,
    pub cancelled: Arc<AtomicBool>,
    pub deadline: Instant,
    pub kind: Work,
}

pub(super) enum Work {
    Capture(CaptureRenderIntent),
    Encode(RenderStageRequest, RenderWriteHandle),
    Verify(RenderStageRequest, StoredRenderCheckpoint, RenderReadHandle),
    Prepare(PublicationPermit),
    Report(PublicationPermit),
    Movie(PublicationPermit),
    Reconcile(PublicationPermit),
    Release,
}

#[derive(Clone, Copy)]
pub(super) enum ExpectedReply {
    Captured,
    Retained,
    Verified,
    Prepared,
    ReportCommitted,
    Published,
    Reconciled,
    Released,
}

impl Work {
    pub fn expected_reply(&self) -> ExpectedReply {
        match self {
            Self::Capture(_) => ExpectedReply::Captured,
            Self::Encode(..) => ExpectedReply::Retained,
            Self::Verify(..) => ExpectedReply::Verified,
            Self::Prepare(_) => ExpectedReply::Prepared,
            Self::Report(_) => ExpectedReply::ReportCommitted,
            Self::Movie(_) => ExpectedReply::Published,
            Self::Reconcile(_) => ExpectedReply::Reconciled,
            Self::Release => ExpectedReply::Released,
        }
    }
}

impl ExpectedReply {
    pub fn admits(self, reply: &StageReply) -> bool {
        matches!(
            (self, reply),
            (_, StageReply::Failed { .. })
                | (Self::Captured, StageReply::Captured(_))
                | (Self::Retained, StageReply::Retained(_))
                | (Self::Verified, StageReply::Verified(_))
                | (Self::Prepared, StageReply::Prepared(..))
                | (Self::ReportCommitted, StageReply::ReportCommitted(_))
                | (Self::Published, StageReply::Published(_))
                | (Self::Reconciled, StageReply::Reconciled { .. })
                | (Self::Released, StageReply::Released)
        )
    }
}

pub(super) struct Reply {
    pub identity: WorkflowIdentity,
    pub stage: StageReply,
}

pub(super) enum StageReply {
    Captured(RenderIntent),
    Retained(Arc<PreparedRenderRetention>),
    Verified(RenderVerificationObservation),
    Prepared(PreparedPublicationEvidence, RetainedPublicationArtifacts),
    ReportCommitted(RetainedPublicationArtifacts),
    Published(PublicationOutcome),
    Reconciled {
        completion: PublicationReconciliation,
        receipt: Option<PublicationReceipt>,
        observed_movie_commit: bool,
    },
    Failed {
        diagnostic: RenderDiagnostic,
        cleanup_confirmed: bool,
        retained: RetainedPublicationArtifacts,
    },
    Released,
}

pub(super) struct Worker {
    config: WorkflowConfig,
    progress: Arc<Mutex<Option<(WorkflowIdentity, WorkflowProgress)>>>,
    retained: Option<Arc<PreparedRenderRetention>>,
    candidate: Option<VerifiedCandidate>,
    publication: Option<PreparedPublication>,
    inspection: Option<RecoveryInspection>,
}

impl Worker {
    pub fn run(
        config: WorkflowConfig,
        commands: Receiver<Command>,
        replies: SyncSender<Reply>,
        progress: Arc<Mutex<Option<(WorkflowIdentity, WorkflowProgress)>>>,
    ) {
        let mut worker = Self {
            config,
            progress,
            retained: None,
            candidate: None,
            publication: None,
            inspection: None,
        };
        while let Ok(command) = commands.recv() {
            let identity = command.identity.clone();
            let stage = worker.execute(command);
            // Reliable capacity-one replies cannot be replaced by progress.
            // The owner sends the next command only after consuming this reply.
            if replies.send(Reply { identity, stage }).is_err() {
                break;
            }
        }
        // Candidates and locks are destroyed here, on the preparation worker.
    }

    fn execute(&mut self, command: Command) -> StageReply {
        let Command {
            identity,
            cancelled,
            deadline,
            kind,
        } = command;
        match kind {
            Work::Capture(request) => match jobs::capture_intent(request, &cancelled, deadline) {
                Ok(intent) => StageReply::Captured(intent),
                Err(error) => failure(error),
            },
            Work::Encode(request, writer) => {
                let progress = self.progress.clone();
                match jobs::encode_and_retain(
                    &self.config.runtime,
                    &request,
                    &writer,
                    (self.config.encode_limits, self.config.media_limits),
                    &cancelled,
                    deadline,
                    |value| {
                        report_progress(
                            &progress,
                            &identity,
                            WorkflowProgress::Encoding {
                                completed_frames: value.completed_frames,
                                total_frames: value.total_frames,
                                completed_audio_samples: value.completed_audio_samples,
                                total_audio_samples: value.total_audio_samples,
                            },
                        )
                    },
                ) {
                    Ok(retained) => {
                        let retained = Arc::new(retained);
                        self.retained = Some(retained.clone());
                        StageReply::Retained(retained)
                    }
                    Err(error) => failure(error),
                }
            }
            Work::Verify(request, checkpoint, reader) => {
                self.retained = None;
                let progress = self.progress.clone();
                match jobs::verify_checkpoint(
                    &self.config.runtime,
                    &request,
                    (&checkpoint, &reader),
                    (self.config.verification_limits, self.config.media_limits),
                    &cancelled,
                    deadline,
                    |value| {
                        report_progress(&progress, &identity, WorkflowProgress::Verification(value))
                    },
                ) {
                    Ok(candidate) => {
                        let observation =
                            jobs::verification_observation(&candidate, &cancelled, deadline);
                        self.candidate = Some(candidate);
                        match observation {
                            Ok(observation) => StageReply::Verified(observation),
                            Err(error) => failure(error),
                        }
                    }
                    Err(error) => failure(error),
                }
            }
            Work::Prepare(permit) => {
                let Some(candidate) = self.candidate.take() else {
                    return missing("publication candidate");
                };
                let progress = self.progress.clone();
                match journal::prepare(
                    candidate,
                    &self.config.package,
                    &permit,
                    &cancelled,
                    deadline,
                    |value| {
                        report_progress(&progress, &identity, WorkflowProgress::Publication(value))
                    },
                ) {
                    Ok(prepared) => {
                        let reply =
                            StageReply::Prepared(prepared.evidence().clone(), prepared.retained());
                        self.publication = Some(prepared);
                        reply
                    }
                    Err(failure) => {
                        self.candidate = Some(failure.candidate);
                        publication_failure(failure.error, failure.retained)
                    }
                }
            }
            Work::Report(permit) => {
                let Some(prepared) = self.publication.as_mut() else {
                    return missing("prepared publication");
                };
                let result = prepared.commit_report(&permit, &cancelled, deadline);
                let retained = prepared.retained();
                match result {
                    Ok(()) => StageReply::ReportCommitted(retained),
                    Err(error) => publication_failure(error, retained),
                }
            }
            Work::Movie(permit) => {
                let Some(prepared) = self.publication.take() else {
                    return missing("prepared publication");
                };
                match prepared.commit_movie(&permit, &cancelled, deadline) {
                    Ok(outcome) => StageReply::Published(outcome),
                    Err(failure) => {
                        self.candidate = Some(failure.candidate);
                        publication_failure(failure.error, failure.retained)
                    }
                }
            }
            Work::Reconcile(permit) => {
                let Some(candidate) = self.candidate.as_ref() else {
                    return missing("reconciliation candidate");
                };
                match journal::reconcile(candidate, &permit, &cancelled, deadline) {
                    Ok(inspection) => {
                        let (receipt, observed_movie_commit) = match inspection.outcome() {
                            RecoveryOutcome::NotPublished => (None, false),
                            RecoveryOutcome::Committed(
                                PublicationOutcome::Published(receipt)
                                | PublicationOutcome::PublishedUnconfirmed { receipt, .. },
                            ) => (Some(receipt.clone()), true),
                        };
                        let reply = StageReply::Reconciled {
                            completion: inspection.completion(),
                            receipt,
                            observed_movie_commit,
                        };
                        // Retain descriptor locks until owner records completion
                        // and explicitly sends Release.
                        self.inspection = Some(inspection);
                        reply
                    }
                    Err(error) => {
                        publication_failure(error, RetainedPublicationArtifacts::default())
                    }
                }
            }
            Work::Release => {
                self.inspection = None;
                self.publication = None;
                self.candidate = None;
                self.retained = None;
                StageReply::Released
            }
        }
    }
}

fn report_progress(
    slot: &Mutex<Option<(WorkflowIdentity, WorkflowProgress)>>,
    identity: &WorkflowIdentity,
    progress: WorkflowProgress,
) {
    // Losing a progress sample is harmless. Completion has its own reliable
    // channel and never depends on this optional slot or a poisoned mutex.
    if let Ok(mut slot) = slot.try_lock() {
        *slot = Some((identity.clone(), progress));
    }
}

fn missing(what: &str) -> StageReply {
    StageReply::Failed {
        diagnostic: diagnostic("workflow_state", format!("Missing {what}")),
        cleanup_confirmed: true,
        retained: RetainedPublicationArtifacts::default(),
    }
}

fn failure(error: EncodedRenderError) -> StageReply {
    let code = match &error {
        EncodedRenderError::Cancelled => "cancelled",
        EncodedRenderError::Deadline => "deadline_exceeded",
        _ if !error.cleanup_confirmed() => "cleanup_unconfirmed",
        _ => "render_stage_failed",
    };
    StageReply::Failed {
        diagnostic: diagnostic(code, &error),
        cleanup_confirmed: error.cleanup_confirmed(),
        retained: RetainedPublicationArtifacts::default(),
    }
}

fn publication_failure(
    error: PublicationDiagnostic,
    retained: RetainedPublicationArtifacts,
) -> StageReply {
    StageReply::Failed {
        diagnostic: diagnostic(&error.code, error.message),
        // Publication stages own no child. A failure result is pre-movie-commit;
        // the host returns every observed movie rename as PublicationOutcome.
        cleanup_confirmed: true,
        retained,
    }
}
