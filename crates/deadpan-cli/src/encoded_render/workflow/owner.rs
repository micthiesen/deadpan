use super::*;
use worker::Work;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

impl RenderWorkflow {
    pub fn new(store: &ProjectStore, config: WorkflowConfig) -> Result<Self, WorkflowError> {
        let owner = store.render_read_handle();
        store.check_render_owner(&owner)?;
        let project_id = store.snapshot()?.project_id().clone();
        config
            .verification_limits
            .validate()
            .map_err(WorkflowError::Configuration)?;
        let (commands, receiver) = mpsc::sync_channel(1);
        let (sender, replies) = mpsc::sync_channel(1);
        let progress = Arc::new(Mutex::new(None));
        let worker_progress = progress.clone();
        let worker_config = config.clone();
        let worker = thread::Builder::new()
            .name("deadpan-render-stages".into())
            .spawn(move || {
                Worker::run(worker_config, receiver, sender, worker_progress);
            })?;
        Ok(Self {
            config,
            owner,
            project_id,
            commands: Some(commands),
            replies,
            progress,
            worker: Some(worker),
            active: None,
            status: WorkflowStatus::default(),
        })
    }

    pub fn status(&self) -> &WorkflowStatus {
        &self.status
    }

    /// True includes unresolved cleanup. Such a run still owns the slot and
    /// must not be replaced merely because it has stopped emitting progress.
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// The native service can close/switch the writer once all stage-owned
    /// values have been released. Journal faults remain visible in status and
    /// require reopen, even when execution cleanup has completed safely.
    pub fn can_release_writer(&self) -> bool {
        self.active.as_ref().is_none_or(|active| {
            active.released && !active.pending && self.status.cleanup_confirmed
        })
    }

    pub fn start(
        &mut self,
        store: &mut ProjectStore,
        request: StartRender,
    ) -> Result<(), WorkflowError> {
        self.admit(store, request.deadline)?;
        let snapshot = store.snapshot()?;
        if snapshot.project_id() != &self.project_id || snapshot.revision_id() != &request.revision
        {
            return Err(WorkflowError::StaleRevision);
        }
        if match &request.policy {
            RenderPolicy::Engineering(value) => value.schema_version != 1,
            RenderPolicy::Automatic(value) => value.schema_version != 1,
        } {
            return Err(WorkflowError::Configuration(
                "unsupported render policy version".into(),
            ));
        }
        validate_destination(&request.identity, &request.publication)?;
        let capture = CaptureRenderIntent {
            package: self.config.package.clone(),
            revision: request.revision,
            range: request.range,
            job_id: request.identity.job_id.clone(),
            policy: request.policy,
        };
        self.begin(
            store,
            request.identity,
            Destination::Publish(request.publication),
            request.deadline,
        )?;
        self.status.stage = WorkflowStage::Capturing;
        self.send(Work::Capture(capture))
    }

    pub fn retry(
        &mut self,
        store: &mut ProjectStore,
        request: RetryRender,
    ) -> Result<(), WorkflowError> {
        self.admit(store, request.deadline)?;
        validate_destination(&request.identity, &request.publication)?;
        let intent = store.render_job(&request.identity.job_id)?;
        if let Some(checkpoint) = &request.checkpoint_attempt_id {
            store.render_checkpoint(&request.identity.job_id, checkpoint)?;
        }
        self.begin(
            store,
            request.identity,
            Destination::Publish(request.publication),
            request.deadline,
        )?;
        self.status.intent = Some(intent);
        let result = self.begin_attempt(store, request.checkpoint_attempt_id);
        self.handle_owner_result(store, result)
    }

    pub fn reconcile(
        &mut self,
        store: &mut ProjectStore,
        request: ReconcileRender,
    ) -> Result<(), WorkflowError> {
        self.admit(store, request.deadline)?;
        let publication = store.render_publication(&request.publication_id)?;
        if publication.intent.job_id != request.identity.job_id || publication.operation.active {
            return Err(WorkflowError::Configuration(
                "reconciliation job differs or publication remains active".into(),
            ));
        }
        let checkpoint = publication.encoding_attempt_id.clone();
        self.begin(
            store,
            request.identity,
            Destination::Reconcile {
                publication_id: request.publication_id,
                operation_id: request.operation_id,
                cancellation_token: request.cancellation_token,
            },
            request.deadline,
        )?;
        self.status.intent = Some(publication.render_intent.clone());
        // The old inactive record is recovery context, not an active permit.
        self.status.observed_movie_commit = publication.observed_movie_commit;
        self.status.publication = Some(publication);
        let result = self.begin_attempt(store, Some(checkpoint));
        self.handle_owner_result(store, result)
    }

    /// Persist cancellation and revoke a publication permit before signaling
    /// the stage. A successful racing movie commit still wins over cancellation.
    pub fn cancel(
        &mut self,
        store: &mut ProjectStore,
        identity: &WorkflowIdentity,
    ) -> Result<(), WorkflowError> {
        store.check_render_owner(&self.owner)?;
        let active = self.active.as_ref().ok_or(WorkflowError::Identity)?;
        if &active.identity != identity {
            return Err(WorkflowError::Identity);
        }
        if self.status.cancellation_requested {
            return Ok(());
        }
        let result = self.persist_cancellation(store);
        // Failure is still a stop request. Never allow a failed journal write
        // to leave a stage running intentionally; its result is unresolved.
        self.status.cancellation_requested = true;
        if let Some(active) = &self.active {
            active.cancelled.store(true, Ordering::Release);
        }
        if let Err(error) = result {
            self.journal_fault(store, &error);
            return Err(error);
        }
        if !self.active.as_ref().is_some_and(|active| active.blocked) {
            self.status.stage = WorkflowStage::Cancelling;
        }
        Ok(())
    }

    /// Nonblocking: handles at most one reliable stage completion and samples
    /// optional progress. No recv/join/media/hash/copy is performed here.
    pub fn poll(&mut self, store: &mut ProjectStore) -> Result<bool, WorkflowError> {
        store.check_render_owner(&self.owner)?;
        let mut changed = false;
        if let Ok(mut progress) = self.progress.try_lock()
            && let Some((identity, value)) = progress.take()
            && self
                .active
                .as_ref()
                .is_some_and(|active| active.identity == identity && active.pending)
            && matches!(
                (&value, self.status.stage),
                (
                    WorkflowProgress::Qualification { .. },
                    WorkflowStage::Qualifying
                ) | (WorkflowProgress::Encoding { .. }, WorkflowStage::Encoding)
                    | (WorkflowProgress::Verification(_), WorkflowStage::Verifying)
                    | (
                        WorkflowProgress::Publication(_),
                        WorkflowStage::PreparingPublication
                    )
            )
        {
            self.status.progress = Some(value);
            changed = true;
        }
        match self.replies.try_recv() {
            Ok(reply) => {
                self.receive(store, reply)?;
                Ok(true)
            }
            Err(TryRecvError::Empty) => Ok(changed),
            Err(TryRecvError::Disconnected) if self.active.is_none() => Ok(changed),
            Err(TryRecvError::Disconnected) => Err(self.worker_lost(store)),
        }
    }

    /// Off-UI shutdown only. Cancellation is persisted while the writer is
    /// alive, and reliable replies keep advancing the journal until release.
    /// A panic or unknown teardown returns an error and keeps the slot fenced.
    pub fn drain(&mut self, store: &mut ProjectStore) -> Result<(), WorkflowError> {
        store.check_render_owner(&self.owner)?;
        if let Some(identity) = self.active.as_ref().map(|active| active.identity.clone()) {
            // Even if persistence fails, receive outstanding work to retain a
            // racing movie commit and to finish safe off-owner cleanup.
            let _ = self.cancel(store, &identity);
        }
        loop {
            if self.can_release_writer() {
                return Ok(());
            }
            let Some(active) = &self.active else {
                return Ok(());
            };
            if !active.pending {
                return Err(WorkflowError::Unresolved(self.unresolved_detail()));
            }
            match self.replies.recv() {
                Ok(reply) => {
                    let _ = self.receive(store, reply);
                }
                Err(_) => return Err(self.worker_lost(store)),
            }
        }
    }

    fn admit(&self, store: &ProjectStore, deadline: Instant) -> Result<(), WorkflowError> {
        store.check_render_owner(&self.owner)?;
        if self.active.is_some() {
            return Err(WorkflowError::Busy);
        }
        if Instant::now() >= deadline {
            return Err(WorkflowError::Configuration(
                "render deadline already expired".into(),
            ));
        }
        if self.worker.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(WorkflowError::Unresolved("stage worker stopped".into()));
        }
        Ok(())
    }

    fn begin(
        &mut self,
        store: &ProjectStore,
        identity: WorkflowIdentity,
        destination: Destination,
        deadline: Instant,
    ) -> Result<(), WorkflowError> {
        let lease = store.acquire_render_workflow()?;
        self.status = WorkflowStatus {
            identity: Some(identity.clone()),
            ..WorkflowStatus::default()
        };
        self.active = Some(Active {
            identity,
            destination,
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
            pending: false,
            expected_reply: None,
            blocked: false,
            released: false,
            lease: Some(lease),
        });
        Ok(())
    }

    fn send(&mut self, kind: Work) -> Result<(), WorkflowError> {
        let starts_work = !matches!(kind, Work::Release);
        let expected_reply = kind.expected_reply();
        let active = self.active.as_mut().ok_or(WorkflowError::Identity)?;
        if active.pending {
            return Err(WorkflowError::Configuration(
                "stage command already pending".into(),
            ));
        }
        let command = Command {
            identity: active.identity.clone(),
            cancelled: active.cancelled.clone(),
            deadline: active.deadline,
            kind,
        };
        let result = self
            .commands
            .as_ref()
            .ok_or_else(|| WorkflowError::Unresolved("stage worker closed".into()))?
            .try_send(command);
        match result {
            Ok(()) => {
                active.pending = true;
                active.expected_reply = Some(expected_reply);
                if starts_work {
                    self.status.cleanup_confirmed = false;
                }
                Ok(())
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                active.blocked = true;
                active.cancelled.store(true, Ordering::Release);
                self.status.stage = WorkflowStage::Unresolved;
                self.status.outcome = Some(if self.status.observed_movie_commit {
                    WorkflowOutcome::PublishedUnconfirmed
                } else {
                    WorkflowOutcome::Unresolved
                });
                self.status.diagnostic = Some(diagnostic(
                    "stage_channel_failed",
                    "Stage command was not delivered; cleanup remains unproven",
                ));
                Err(WorkflowError::Unresolved(
                    "stage command was not delivered".into(),
                ))
            }
        }
    }

    fn begin_attempt(
        &mut self,
        store: &mut ProjectStore,
        checkpoint: Option<AttemptId>,
    ) -> Result<(), WorkflowError> {
        let identity = self
            .active
            .as_ref()
            .ok_or(WorkflowError::Identity)?
            .identity
            .clone();
        let attempt = store.begin_render_attempt(BeginRenderAttempt {
            job_id: identity.job_id,
            attempt_id: identity.attempt_id,
            cancellation_token: identity.cancellation_token,
            checkpoint_attempt_id: checkpoint,
        })?;
        self.status.attempt = Some(attempt);
        if self
            .status
            .attempt
            .as_ref()
            .is_some_and(|attempt| attempt.checkpoint_attempt_id.is_some())
        {
            self.verify(store)
        } else if self
            .status
            .intent
            .as_ref()
            .is_some_and(|intent| intent.policy.is_automatic())
        {
            self.status.stage = WorkflowStage::Qualifying;
            self.send(Work::Qualify(
                self.stage_request()?,
                store.render_write_handle()?,
            ))
        } else {
            self.transition(store, RenderAttemptTransition::Encoding)?;
            self.status.stage = WorkflowStage::Encoding;
            self.send(Work::Encode(
                self.stage_request()?,
                store.render_write_handle()?,
            ))
        }
    }

    fn verify(&mut self, store: &mut ProjectStore) -> Result<(), WorkflowError> {
        self.transition(store, RenderAttemptTransition::Verifying)?;
        let attempt = self
            .status
            .attempt
            .as_ref()
            .ok_or(WorkflowError::Identity)?;
        let checkpoint_id = attempt
            .checkpoint_attempt_id
            .as_ref()
            .ok_or_else(|| WorkflowError::Configuration("verification has no checkpoint".into()))?;
        let checkpoint = store.render_checkpoint(&attempt.job_id, checkpoint_id)?;
        self.status.stage = WorkflowStage::Verifying;
        self.send(Work::Verify(
            self.stage_request()?,
            checkpoint,
            self.owner.clone(),
        ))
    }

    fn stage_request(&self) -> Result<RenderStageRequest, WorkflowError> {
        Ok(RenderStageRequest {
            package: self.config.package.clone(),
            intent: self.status.intent.clone().ok_or(WorkflowError::Identity)?,
            attempt: self.status.attempt.clone().ok_or(WorkflowError::Identity)?,
        })
    }

    fn transition(
        &mut self,
        store: &mut ProjectStore,
        transition: RenderAttemptTransition,
    ) -> Result<(), WorkflowError> {
        let identity = self
            .status
            .attempt
            .as_ref()
            .ok_or(WorkflowError::Identity)?
            .identity();
        self.status.attempt = Some(store.transition_render_attempt(&identity, transition)?);
        Ok(())
    }

    fn persist_cancellation(&mut self, store: &mut ProjectStore) -> Result<(), WorkflowError> {
        if let Some(publication) = &self.status.publication
            && publication.operation.active
            && !publication.cancellation_requested
        {
            self.status.publication =
                Some(store.request_publication_cancellation(&publication.identity())?);
        }
        if let Some(attempt) = &self.status.attempt
            && !attempt.state.is_terminal()
            && attempt.state != RenderAttemptState::Cancelling
        {
            self.transition(store, RenderAttemptTransition::RequestCancellation)?;
        }
        Ok(())
    }

    fn receive(&mut self, store: &mut ProjectStore, reply: Reply) -> Result<(), WorkflowError> {
        let active = self.active.as_mut().ok_or(WorkflowError::Identity)?;
        if active.identity != reply.identity
            || !active.pending
            || !active
                .expected_reply
                .is_some_and(|expected| expected.admits(&reply.stage))
        {
            return Err(self.worker_lost(store));
        }
        active.pending = false;
        active.expected_reply = None;
        self.status.progress = None;
        // A release only confirms owned values were dropped on the worker.
        // It cannot turn an earlier unknown process teardown into proof.
        if matches!(reply.stage, StageReply::Released) {
            active.released = true;
            if self.status.cleanup_confirmed
                && let Some(lease) = active.lease.take()
            {
                lease.release();
            }
            if active.blocked {
                return Err(WorkflowError::Unresolved(self.unresolved_detail()));
            }
            self.status.cleanup_confirmed = true;
            self.status.stage = WorkflowStage::Finished;
            self.active = None;
            return Ok(());
        }
        let blocked = active.blocked;
        if blocked {
            self.retain_late_result(&reply.stage);
            if matches!(
                reply.stage,
                StageReply::Failed {
                    cleanup_confirmed: false,
                    ..
                } | StageReply::AdmissionFailed {
                    cleanup_confirmed: false,
                    ..
                }
            ) {
                self.status.cleanup_confirmed = false;
                return Err(WorkflowError::Unresolved(self.unresolved_detail()));
            }
            self.status.cleanup_confirmed = true;
            self.send(Work::Release)?;
            return Err(WorkflowError::Unresolved(self.unresolved_detail()));
        }
        self.status.cleanup_confirmed = !matches!(
            reply.stage,
            StageReply::Failed {
                cleanup_confirmed: false,
                ..
            } | StageReply::AdmissionFailed {
                cleanup_confirmed: false,
                ..
            }
        );
        let result = self.advance(store, reply.stage);
        self.handle_owner_result(store, result)
    }

    fn handle_owner_result(
        &mut self,
        store: &mut ProjectStore,
        result: Result<(), WorkflowError>,
    ) -> Result<(), WorkflowError> {
        if let Err(error) = &result {
            self.journal_fault(store, error);
        }
        result
    }

    fn advance(
        &mut self,
        store: &mut ProjectStore,
        stage: StageReply,
    ) -> Result<(), WorkflowError> {
        // Completed destination work may race an already-persisted cancel.
        // Preserve its recovery names even when no next phase is authorized.
        if let StageReply::Prepared(_, retained) | StageReply::ReportCommitted(retained) = &stage {
            self.status.retained = retained.clone();
        }
        match stage {
            StageReply::Published(outcome) => return self.published(store, outcome),
            StageReply::Reconciled {
                completion,
                receipt,
                observed_movie_commit,
            } => {
                return self.reconciled(store, completion, receipt, observed_movie_commit);
            }
            StageReply::Failed {
                diagnostic,
                cleanup_confirmed,
                retained,
            } => {
                self.status.retained = retained;
                return self.failed(store, diagnostic, cleanup_confirmed);
            }
            StageReply::AdmissionFailed {
                diagnostic,
                cleanup_confirmed,
                decision,
            } => {
                if !cleanup_confirmed || decision.is_none() {
                    return self.failed(store, diagnostic, cleanup_confirmed);
                }
                let decision = decision.as_deref().ok_or(WorkflowError::Identity)?;
                // A native rejection may race a persisted cancellation. Keep the
                // terminal state consistent with the captured typed observation.
                let cancelled = decision.cancelled();
                let attempt = self
                    .status
                    .attempt
                    .as_ref()
                    .ok_or(WorkflowError::Identity)?;
                let transition = if cancelled {
                    RenderAttemptTransition::FinishCancelled
                } else {
                    RenderAttemptTransition::Failed(diagnostic.clone())
                };
                self.status.attempt = Some(store.finish_render_admission(
                    &attempt.identity(),
                    decision,
                    transition,
                )?);
                self.status.diagnostic = Some(diagnostic);
                self.status.cleanup_confirmed = true;
                self.status.outcome = Some(if cancelled {
                    WorkflowOutcome::Cancelled
                } else {
                    WorkflowOutcome::Failed
                });
                return self.release();
            }
            _ => {}
        }
        if self.status.cancellation_requested {
            return self.finish_cancelled(store);
        }
        match stage {
            StageReply::Captured(intent) => {
                let active = self.active.as_ref().ok_or(WorkflowError::Identity)?;
                let intent = store.create_render_job(intent, &active.cancelled, active.deadline)?;
                self.status.intent = Some(intent);
                self.begin_attempt(store, None)
            }
            StageReply::Qualified(decision) => {
                let identity = self
                    .status
                    .attempt
                    .as_ref()
                    .ok_or(WorkflowError::Identity)?
                    .identity();
                self.status.attempt = Some(store.begin_render_encoding(&identity, &decision)?);
                self.status.stage = WorkflowStage::Encoding;
                self.send(Work::Encode(
                    self.stage_request()?,
                    store.render_write_handle()?,
                ))
            }
            StageReply::Retained(prepared) => {
                let active = self.active.as_ref().ok_or(WorkflowError::Identity)?;
                let identity = self
                    .status
                    .attempt
                    .as_ref()
                    .ok_or(WorkflowError::Identity)?
                    .identity();
                self.status.attempt = Some(store.retain_render_checkpoint(
                    &identity,
                    &prepared,
                    &active.cancelled,
                    active.deadline,
                )?);
                self.verify(store)
            }
            StageReply::Verified(observation) => {
                let identity = self
                    .status
                    .attempt
                    .as_ref()
                    .ok_or(WorkflowError::Identity)?
                    .identity();
                self.status.attempt =
                    Some(store.record_render_verification(&identity, observation)?);
                self.begin_publication(store)
            }
            StageReply::Prepared(evidence, retained) => {
                self.status.retained = retained;
                let identity = self.publication_identity()?;
                let prepared = store.record_prepared_publication(&identity, evidence)?;
                self.status.publication = Some(prepared.record().clone());
                let permit = store.advance_publication(
                    &prepared.identity(),
                    PublicationPhase::ReportCommitting,
                )?;
                self.status.publication = Some(permit.record().clone());
                self.status.stage = WorkflowStage::CommittingReport;
                self.send(Work::Report(permit))
            }
            StageReply::ReportCommitted(retained) => {
                self.status.retained = retained;
                let committed = store.advance_publication(
                    &self.publication_identity()?,
                    PublicationPhase::ReportCommitted,
                )?;
                self.status.publication = Some(committed.record().clone());
                let permit = store.advance_publication(
                    &committed.identity(),
                    PublicationPhase::MovieCommitting,
                )?;
                self.status.publication = Some(permit.record().clone());
                self.status.stage = WorkflowStage::CommittingMovie;
                self.send(Work::Movie(permit))
            }
            StageReply::Published(_)
            | StageReply::Reconciled { .. }
            | StageReply::Failed { .. }
            | StageReply::AdmissionFailed { .. }
            | StageReply::Released => Err(WorkflowError::Configuration(
                "unexpected stage completion".into(),
            )),
        }
    }

    fn begin_publication(&mut self, store: &mut ProjectStore) -> Result<(), WorkflowError> {
        let active = self.active.as_ref().ok_or(WorkflowError::Identity)?;
        let verified = active.identity.attempt_id.clone();
        let destination = active.destination.clone();
        let (permit, reconcile) = match destination {
            Destination::Publish(request) => (
                store.begin_render_publication(
                    PublicationIntent {
                        schema_version: 1,
                        publication_id: request.publication_id,
                        job_id: active.identity.job_id.clone(),
                        verified_attempt_id: verified,
                        destination: request.destination,
                    },
                    request.operation_id,
                    request.cancellation_token,
                )?,
                false,
            ),
            Destination::Reconcile {
                publication_id,
                operation_id,
                cancellation_token,
            } => (
                store.begin_publication_reconciliation(
                    &publication_id,
                    verified,
                    operation_id,
                    cancellation_token,
                )?,
                true,
            ),
        };
        self.status.publication = Some(permit.record().clone());
        if reconcile {
            self.status.stage = WorkflowStage::Reconciling;
            self.send(Work::Reconcile(permit))
        } else {
            self.status.stage = WorkflowStage::PreparingPublication;
            self.send(Work::Prepare(permit))
        }
    }

    fn publication_identity(
        &self,
    ) -> Result<deadpan_jobs::render::publication::PublicationIdentity, WorkflowError> {
        self.status
            .publication
            .as_ref()
            .map(StoredPublication::identity)
            .ok_or(WorkflowError::Identity)
    }

    fn finish_cancelled(&mut self, store: &mut ProjectStore) -> Result<(), WorkflowError> {
        self.status.cleanup_confirmed = true;
        if let Some(publication) = &self.status.publication
            && publication.operation.active
        {
            let identity = publication.identity();
            let result = if publication.operation.kind == PublicationOperationKind::Reconcile {
                store.finish_publication_reconciliation(
                    &identity,
                    PublicationReconciliation::Unresolved(diagnostic(
                        "cancelled",
                        "Reconciliation cancelled before complete inspection",
                    )),
                )?
            } else {
                store.finish_publication(&identity, PublicationCompletion::Cancelled)?
            };
            self.status.publication = Some(result);
        }
        if self
            .status
            .attempt
            .as_ref()
            .is_some_and(|attempt| !attempt.state.is_terminal())
        {
            self.transition(store, RenderAttemptTransition::FinishCancelled)?;
        }
        self.status.outcome = Some(self.unfinished_outcome(WorkflowOutcome::Cancelled));
        self.release()
    }

    fn failed(
        &mut self,
        store: &mut ProjectStore,
        failure: RenderDiagnostic,
        cleanup_confirmed: bool,
    ) -> Result<(), WorkflowError> {
        self.status.diagnostic = Some(failure.clone());
        self.status.cleanup_confirmed = cleanup_confirmed;
        if !cleanup_confirmed {
            self.persist_cancellation(store)?;
            let active = self.active.as_mut().ok_or(WorkflowError::Identity)?;
            active.blocked = true;
            active.cancelled.store(true, Ordering::Release);
            self.status.stage = WorkflowStage::Unresolved;
            self.status.outcome = Some(self.unfinished_outcome(WorkflowOutcome::Unresolved));
            return Err(WorkflowError::Unresolved(failure.detail));
        }
        if self.status.cancellation_requested {
            return self.finish_cancelled(store);
        }
        if let Some(publication) = &self.status.publication
            && publication.operation.active
        {
            let identity = publication.identity();
            self.status.publication = Some(
                if publication.operation.kind == PublicationOperationKind::Reconcile {
                    store.finish_publication_reconciliation(
                        &identity,
                        PublicationReconciliation::Unresolved(failure.clone()),
                    )?
                } else {
                    store.finish_publication(
                        &identity,
                        PublicationCompletion::Failed(failure.clone()),
                    )?
                },
            );
        }
        if self
            .status
            .attempt
            .as_ref()
            .is_some_and(|attempt| !attempt.state.is_terminal())
        {
            self.transition(store, RenderAttemptTransition::Failed(failure))?;
        }
        self.status.outcome = Some(self.unfinished_outcome(WorkflowOutcome::Failed));
        self.release()
    }

    fn unfinished_outcome(&self, ordinary: WorkflowOutcome) -> WorkflowOutcome {
        if self.status.observed_movie_commit {
            WorkflowOutcome::PublishedUnconfirmed
        } else if self
            .active
            .as_ref()
            .is_some_and(|active| matches!(active.destination, Destination::Reconcile { .. }))
        {
            // A stopped inspection cannot prove that an earlier authorized
            // movie rename did not commit. Preserve that uncertainty even if
            // fresh verification stopped before opening the new operation.
            WorkflowOutcome::Unresolved
        } else {
            ordinary
        }
    }

    fn published(
        &mut self,
        store: &mut ProjectStore,
        outcome: PublicationOutcome,
    ) -> Result<(), WorkflowError> {
        self.status.observed_movie_commit = true;
        self.status.cleanup_confirmed = true;
        let completion = match outcome {
            PublicationOutcome::Published(receipt) => {
                self.status.receipt = Some(receipt);
                self.status.outcome = Some(WorkflowOutcome::Published);
                PublicationCompletion::Published
            }
            PublicationOutcome::PublishedUnconfirmed {
                receipt,
                diagnostic: error,
            } => {
                let diagnostic = diagnostic(&error.code, error.message);
                self.status.receipt = Some(receipt);
                self.status.diagnostic = Some(diagnostic.clone());
                self.status.outcome = Some(WorkflowOutcome::PublishedUnconfirmed);
                PublicationCompletion::PublishedUnconfirmed(diagnostic)
            }
        };
        // Save observed outcome before attempting SQLite. A failed commit must
        // never replace it with Failed/Cancelled or lose the final movie path.
        self.status.publication =
            Some(store.finish_publication(&self.publication_identity()?, completion)?);
        self.release()
    }

    fn reconciled(
        &mut self,
        store: &mut ProjectStore,
        mut completion: PublicationReconciliation,
        receipt: Option<PublicationReceipt>,
        observed_movie_commit: bool,
    ) -> Result<(), WorkflowError> {
        self.status.observed_movie_commit |= observed_movie_commit;
        self.status.receipt = receipt;
        self.status.cleanup_confirmed = true;
        if self.status.cancellation_requested
            && matches!(completion, PublicationReconciliation::Confirmed)
        {
            completion = PublicationReconciliation::CommittedUnconfirmed(diagnostic(
                "cancelled",
                "Movie was observed committed during cancellation",
            ));
        }
        self.status.outcome = Some(match &completion {
            PublicationReconciliation::Confirmed => WorkflowOutcome::Published,
            PublicationReconciliation::CommittedUnconfirmed(error) => {
                self.status.diagnostic = Some(error.clone());
                WorkflowOutcome::PublishedUnconfirmed
            }
            PublicationReconciliation::NotPublished(error) => {
                self.status.diagnostic = Some(error.clone());
                WorkflowOutcome::NotPublished
            }
            PublicationReconciliation::Unresolved(error) => {
                self.status.diagnostic = Some(error.clone());
                if self.status.observed_movie_commit {
                    WorkflowOutcome::PublishedUnconfirmed
                } else {
                    WorkflowOutcome::Unresolved
                }
            }
        });
        self.status.publication = Some(
            store.finish_publication_reconciliation(&self.publication_identity()?, completion)?,
        );
        // Worker still owns RecoveryInspection and its locks until this send.
        self.release()
    }

    fn release(&mut self) -> Result<(), WorkflowError> {
        self.status.stage = WorkflowStage::Releasing;
        self.send(Work::Release)
    }

    fn journal_fault(&mut self, store: &mut ProjectStore, error: &WorkflowError) {
        let Some(active) = &self.active else {
            return;
        };
        let was_blocked = active.blocked;
        if !was_blocked {
            self.status.journal_diagnostic = Some(diagnostic("workflow_journal_failed", error));
        }
        let _ = self.persist_cancellation(store);
        if let Some(active) = &mut self.active {
            active.blocked = true;
            active.cancelled.store(true, Ordering::Release);
        }
        self.status.stage = WorkflowStage::Unresolved;
        if self.status.observed_movie_commit {
            self.status.outcome = Some(WorkflowOutcome::PublishedUnconfirmed);
        } else {
            self.status.outcome = Some(WorkflowOutcome::Unresolved);
        }
        // If an ordinary stage has returned, release its owned values on the
        // worker. Unknown process cleanup deliberately remains fenced.
        if !was_blocked
            && self.active.as_ref().is_some_and(|active| !active.pending)
            && self.status.cleanup_confirmed
        {
            let _ = self.send(Work::Release);
        }
    }

    fn retain_late_result(&mut self, stage: &StageReply) {
        match stage {
            StageReply::Published(
                PublicationOutcome::Published(receipt)
                | PublicationOutcome::PublishedUnconfirmed { receipt, .. },
            ) => {
                self.status.observed_movie_commit = true;
                self.status.receipt = Some(receipt.clone());
                self.status.outcome = Some(WorkflowOutcome::PublishedUnconfirmed);
            }
            StageReply::Reconciled {
                receipt,
                observed_movie_commit,
                ..
            } => {
                self.status.observed_movie_commit |= observed_movie_commit;
                self.status.receipt = receipt.clone();
                if self.status.observed_movie_commit {
                    self.status.outcome = Some(WorkflowOutcome::PublishedUnconfirmed);
                }
            }
            StageReply::Prepared(_, retained)
            | StageReply::ReportCommitted(retained)
            | StageReply::Failed { retained, .. } => self.status.retained = retained.clone(),
            _ => {}
        }
    }

    fn worker_lost(&mut self, store: &mut ProjectStore) -> WorkflowError {
        if let Err(error) = self.persist_cancellation(store) {
            self.status.journal_diagnostic = Some(diagnostic("cancellation_journal_failed", error));
        }
        if let Some(active) = &mut self.active {
            active.pending = false;
            active.blocked = true;
            active.cancelled.store(true, Ordering::Release);
        }
        self.status.cleanup_confirmed = false;
        self.status.stage = WorkflowStage::Unresolved;
        self.status.diagnostic = Some(diagnostic(
            "stage_worker_lost",
            "Stage worker disconnected or replied outside its exact pending operation; process cleanup is unproven",
        ));
        self.status.outcome = Some(if self.status.observed_movie_commit {
            WorkflowOutcome::PublishedUnconfirmed
        } else {
            WorkflowOutcome::Unresolved
        });
        WorkflowError::Unresolved(self.unresolved_detail())
    }

    fn unresolved_detail(&self) -> String {
        self.status
            .journal_diagnostic
            .as_ref()
            .or(self.status.diagnostic.as_ref())
            .map_or_else(
                || "render cleanup or journal remains unresolved".into(),
                |error| error.detail.clone(),
            )
    }
}

fn validate_destination(
    identity: &WorkflowIdentity,
    request: &PublicationRequest,
) -> Result<(), WorkflowError> {
    PublicationIntent {
        schema_version: 1,
        publication_id: request.publication_id.clone(),
        job_id: identity.job_id.clone(),
        verified_attempt_id: identity.attempt_id.clone(),
        destination: request.destination.clone(),
    }
    .validate()
    .map_err(|error| WorkflowError::Configuration(error.to_string()))
}
