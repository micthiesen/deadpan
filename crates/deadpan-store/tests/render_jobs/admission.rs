//! Store state tests use retained measured probe declarations and opaque media
//! objects. They never claim that rebinding metadata encoded or verified media.
use super::*;
use deadpan_jobs::render::{admission::*, publication::PublicationIntent};

fn measured() -> Result<RenderEncodingDecision> {
    Ok(RenderEncodingDecision::from_json(include_bytes!(
        "../../../deadpan-jobs/src/render/admission/tests/measured-decision-v1.json"
    ))?)
}

fn auto_document() -> Result<ProjectDocument> {
    let output = measured()?.output;
    document_with_basis(
        PresentationBasis {
            width: output.canvas[0],
            height: output.canvas[1],
            frame_rate: output.frame_rate,
            color_policy: output.color_policy,
        },
        i64::try_from(output.frame_count)?,
    )
}

fn automatic(store: &mut ProjectStore, name: &str) -> Result<RenderIntent> {
    let mut job = intent(store, name)?;
    job.schema_version = 2;
    job.range = FrameRange::new(
        ProjectFrame(0),
        ProjectFrame(store.snapshot()?.duration()?.frames()),
    )?;
    job.policy = RenderPolicy::Automatic(RenderAutomaticPolicy {
        schema_version: 1,
        selection: RenderAutomaticSelection::Automatic,
        algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
    });
    Ok(store.create_render_job(job, &AtomicBool::new(false), deadline())?)
}

fn selected(job: &RenderIntent, attempt: &StoredRenderAttempt) -> Result<RenderEncodingDecision> {
    let mut value = measured()?;
    value.job_id = job.job_id.clone();
    value.encoding_attempt_id = attempt.attempt_id.clone();
    value.document_sha256 = job.document_sha256.clone();
    value.output.project_id = job.project_id.clone();
    value.output.revision_id = job.revision_id.clone();
    value.output.range = job.range;
    for probe in &mut value.probes {
        probe.identity.request_id = job.job_id.clone();
    }
    value.validate_for(job, &attempt.attempt_id)?;
    Ok(value)
}

fn aborted(
    job: &RenderIntent,
    attempt: &StoredRenderAttempt,
    kind: RenderAdmissionFailureKind,
) -> Result<RenderEncodingDecision> {
    let mut value = selected(job, attempt)?;
    value.runtime = None;
    value.probes.clear();
    value.outcome = RenderDecisionOutcome::Aborted {
        failure: RenderAdmissionFailure {
            kind,
            diagnostic: deadpan_jobs::Diagnostic::new("Controlled qualification interruption")?,
        },
    };
    value.validate()?;
    Ok(value)
}

fn decision_count(database: &Connection) -> Result<i64> {
    Ok(database.query_row(
        "SELECT COUNT(*) FROM render_encoding_decisions",
        [],
        |row| row.get(0),
    )?)
}

#[test]
fn selected_decision_and_encoding_commit_atomically_and_preserve_authored_state() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("automatic.deadpan");
    let mut store = ProjectStore::create(&package, &auto_document()?)?;
    let job = automatic(&mut store, "automatic")?;
    let queued = begin(&mut store, &job, "encode")?;
    let decision = selected(&job, &queued)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = authored(&database)?;
    assert!(
        store
            .transition_render_attempt(&queued.identity(), RenderAttemptTransition::Encoding)
            .is_err()
    );
    assert_eq!(decision_count(&database)?, 0);
    database.execute_batch("CREATE TRIGGER reject_encoding BEFORE UPDATE ON render_attempts BEGIN SELECT RAISE(ABORT,'controlled journal failure'); END")?;
    assert!(
        store
            .begin_render_encoding(&queued.identity(), &decision)
            .is_err()
    );
    assert_eq!(decision_count(&database)?, 0);
    assert_eq!(
        store.render_attempt(&job.job_id, &queued.attempt_id)?,
        queued
    );
    database.execute_batch("DROP TRIGGER reject_encoding")?;
    let encoding = store.begin_render_encoding(&queued.identity(), &decision)?;
    assert_eq!(
        (encoding.state, encoding.transition_sequence),
        (RenderAttemptState::Encoding, 2)
    );
    assert_eq!(
        store.render_encoding_decision(&job.job_id, &queued.attempt_id)?,
        Some(decision.clone())
    );
    let bytes: String =
        database.query_row("SELECT body FROM render_encoding_decisions", [], |row| {
            row.get(0)
        })?;
    assert!(
        store
            .begin_render_encoding(&queued.identity(), &decision)
            .is_err()
    );
    assert!(
        store
            .begin_render_encoding(&encoding.identity(), &decision)
            .is_err()
    );
    assert_eq!(
        database.query_row("SELECT body FROM render_encoding_decisions", [], |row| {
            row.get::<_, String>(0)
        })?,
        bytes
    );
    assert_eq!(authored(&database)?, before);
    store.validate()?;
    Ok(())
}

#[test]
fn selected_decision_rejects_stale_identity_and_foreign_immutable_binding() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("binding.deadpan");
    let mut store = ProjectStore::create(&package, &auto_document()?)?;
    let job = automatic(&mut store, "automatic")?;
    let queued = begin(&mut store, &job, "encode")?;
    let good = selected(&job, &queued)?;
    for change in [
        (|value: &mut RenderAttemptIdentity| value.expected_sequence += 1)
            as fn(&mut RenderAttemptIdentity),
        |value| value.cancellation_token = CancellationToken::new("wrong-token").unwrap(),
        |value| value.job_id = RequestId::new("wrong-job").unwrap(),
        |value| value.attempt_id = AttemptId::new("wrong-attempt").unwrap(),
    ] {
        let mut identity = queued.identity();
        change(&mut identity);
        assert!(store.begin_render_encoding(&identity, &good).is_err());
    }
    for change in [
        (|value: &mut RenderEncodingDecision| {
            value.encoding_attempt_id = AttemptId::new("wrong-owner").unwrap()
        }) as fn(&mut RenderEncodingDecision),
        |value| value.document_sha256 = deadpan_jobs::Sha256::new("a".repeat(64)).unwrap(),
        |value| value.output.revision_id = RevisionId::new("wrong-revision").unwrap(),
        |value| value.job_id = RequestId::new("wrong-job").unwrap(),
    ] {
        let mut changed = good.clone();
        change(&mut changed);
        assert!(
            store
                .begin_render_encoding(&queued.identity(), &changed)
                .is_err()
        );
    }
    let database = Connection::open(package.join("project.sqlite"))?;
    assert_eq!(decision_count(&database)?, 0);
    assert_eq!(
        store.render_attempt(&job.job_id, &queued.attempt_id)?,
        queued
    );
    Ok(())
}

#[test]
fn current_contract_cannot_substitute_another_document_canvas() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut doc = measured()?.output;
    doc.canvas = [640, 360];
    let document = document_with_basis(
        PresentationBasis {
            width: doc.canvas[0],
            height: doc.canvas[1],
            frame_rate: doc.frame_rate,
            color_policy: doc.color_policy,
        },
        i64::try_from(doc.frame_count)?,
    )?;
    let mut store = ProjectStore::create(&scratch.path().join("canvas.deadpan"), &document)?;
    let job = automatic(&mut store, "automatic")?;
    let queued = begin(&mut store, &job, "encode")?;
    let decision = selected(&job, &queued)?;
    // Internally valid probe output, but not this captured document's canvas.
    decision.validate_for(&job, &queued.attempt_id)?;
    assert!(
        store
            .begin_render_encoding(&queued.identity(), &decision)
            .is_err()
    );
    assert!(
        store
            .render_encoding_decision(&job.job_id, &queued.attempt_id)?
            .is_none()
    );
    Ok(())
}

#[test]
fn failed_qualification_and_cancellation_keep_typed_decisions_after_cleanup() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("failed.deadpan");
    let mut store = ProjectStore::create(&package, &auto_document()?)?;
    let job = automatic(&mut store, "automatic")?;
    let queued = begin(&mut store, &job, "deadline")?;
    let decision = aborted(&job, &queued, RenderAdmissionFailureKind::Deadline)?;
    let diagnostic = RenderDiagnostic {
        code: "Deadline".into(),
        detail: "Controlled deadline after stopped work".into(),
    };
    let failed = store.finish_render_admission(
        &queued.identity(),
        &decision,
        RenderAttemptTransition::Failed(diagnostic),
    )?;
    assert_eq!(failed.state, RenderAttemptState::Failed);
    assert_eq!(
        store.render_encoding_decision(&job.job_id, &queued.attempt_id)?,
        Some(decision)
    );
    let queued = begin(&mut store, &job, "cancelled")?;
    let decision = aborted(&job, &queued, RenderAdmissionFailureKind::Cancelled)?;
    // Cancellation must first be persisted; a terminal declaration cannot
    // skip the revocation stage or substitute for owned teardown.
    assert!(
        store
            .finish_render_admission(
                &queued.identity(),
                &decision,
                RenderAttemptTransition::FinishCancelled
            )
            .is_err()
    );
    assert!(
        store
            .render_encoding_decision(&job.job_id, &queued.attempt_id)?
            .is_none()
    );
    let cancelling = store.transition_render_attempt(
        &queued.identity(),
        RenderAttemptTransition::RequestCancellation,
    )?;
    let unresolved = aborted(&job, &queued, RenderAdmissionFailureKind::UnresolvedCleanup)?;
    assert!(
        store
            .finish_render_admission(
                &cancelling.identity(),
                &unresolved,
                RenderAttemptTransition::Failed(RenderDiagnostic {
                    code: "Unresolved".into(),
                    detail: "Cannot prove stopped work".into()
                })
            )
            .is_err()
    );
    assert_eq!(
        store.render_attempt(&job.job_id, &queued.attempt_id)?,
        cancelling
    );
    assert!(
        store
            .render_encoding_decision(&job.job_id, &queued.attempt_id)?
            .is_none()
    );
    let cancelled = store.finish_render_admission(
        &cancelling.identity(),
        &decision,
        RenderAttemptTransition::FinishCancelled,
    )?;
    assert_eq!(cancelled.state, RenderAttemptState::Cancelled);
    assert!(cancelled.cancellation_requested);
    assert_eq!(
        store.render_encoding_decision(&job.job_id, &queued.attempt_id)?,
        Some(decision)
    );
    store.validate()?;
    Ok(())
}

#[test]
fn checkpoint_retries_and_publication_keep_original_encoding_decision() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("retry.deadpan");
    let mut store = ProjectStore::create(&package, &auto_document()?)?;
    let job = automatic(&mut store, "automatic")?;
    let queued = begin(&mut store, &job, "original-encode")?;
    let decision = selected(&job, &queued)?;
    let encoding = store.begin_render_encoding(&queued.identity(), &decision)?;
    let prepared = retention(&store, &encoding)?;
    let retained = store.retain_render_checkpoint(
        &encoding.identity(),
        &prepared,
        &AtomicBool::new(false),
        deadline(),
    )?;
    let checkpoint = store.render_checkpoint(&job.job_id, &encoding.attempt_id)?;
    fail(&mut store, &retained)?;
    let retry = store.begin_render_attempt(BeginRenderAttempt {
        job_id: job.job_id.clone(),
        attempt_id: AttemptId::new("fresh-verifier")?,
        cancellation_token: CancellationToken::new("fresh-verifier-token")?,
        checkpoint_attempt_id: Some(encoding.attempt_id.clone()),
    })?;
    assert!(
        store
            .render_encoding_decision(&job.job_id, &retry.attempt_id)?
            .is_none()
    );
    assert!(
        store
            .begin_render_encoding(&retry.identity(), &selected(&job, &retry)?)
            .is_err()
    );
    let verifying =
        store.transition_render_attempt(&retry.identity(), RenderAttemptTransition::Verifying)?;
    let verified =
        store.record_render_verification(&verifying.identity(), observation(&checkpoint))?;
    let permit = store.begin_render_publication(
        PublicationIntent {
            schema_version: 1,
            publication_id: RequestId::new("automatic-publication")?,
            job_id: job.job_id.clone(),
            verified_attempt_id: verified.attempt_id,
            destination: scratch.path().join("movie.mp4"),
        },
        AttemptId::new("publication-operation")?,
        CancellationToken::new("publication-token")?,
    )?;
    assert_eq!(permit.encoding_decision(), Some(&decision));
    assert_eq!(permit.record().encoding_attempt_id, encoding.attempt_id);
    store.validate()?;
    drop(permit);
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute(
        "DELETE FROM render_encoding_decisions WHERE attempt_id=?1",
        [encoding.attempt_id.as_str()],
    )?;
    assert!(
        store
            .render_checkpoint(&job.job_id, &encoding.attempt_id)
            .is_err()
    );
    assert!(
        store
            .render_attempt(&job.job_id, &retry.attempt_id)
            .is_err()
    );
    assert!(
        store
            .render_publication(&RequestId::new("automatic-publication")?)
            .is_err()
    );
    Ok(())
}

#[test]
fn reopen_preserves_decision_and_new_encoding_retry_requires_fresh_admission() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("reopen.deadpan");
    let mut store = ProjectStore::create(&package, &auto_document()?)?;
    let job = automatic(&mut store, "automatic")?;
    let queued = begin(&mut store, &job, "old-encode")?;
    let decision = selected(&job, &queued)?;
    let encoding = store.begin_render_encoding(&queued.identity(), &decision)?;
    drop(store);
    let read_only = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(
        read_only.render_attempt(&job.job_id, &queued.attempt_id)?,
        encoding
    );
    assert_eq!(
        read_only.render_encoding_decision(&job.job_id, &queued.attempt_id)?,
        Some(decision.clone())
    );
    drop(read_only);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(
        store.render_attempt(&job.job_id, &queued.attempt_id)?.state,
        RenderAttemptState::Interrupted
    );
    assert_eq!(
        store.render_encoding_decision(&job.job_id, &queued.attempt_id)?,
        Some(decision.clone())
    );
    let fresh = begin(&mut store, &job, "new-encode")?;
    assert!(
        store
            .transition_render_attempt(&fresh.identity(), RenderAttemptTransition::Encoding)
            .is_err()
    );
    assert!(
        store
            .begin_render_encoding(&fresh.identity(), &decision)
            .is_err()
    );
    assert!(
        store
            .render_encoding_decision(&job.job_id, &fresh.attempt_id)?
            .is_none()
    );
    store.begin_render_encoding(&fresh.identity(), &selected(&job, &fresh)?)?;
    store.validate()?;
    Ok(())
}

#[test]
fn targeted_reads_skip_unrelated_bad_decisions_but_full_audit_rejects_them() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("targeted.deadpan");
    let mut store = ProjectStore::create(&package, &auto_document()?)?;
    let job = automatic(&mut store, "automatic")?;
    let historical = begin(&mut store, &job, "historical")?;
    let encoding =
        store.begin_render_encoding(&historical.identity(), &selected(&job, &historical)?)?;
    fail(&mut store, &encoding)?;
    let current = begin(&mut store, &job, "current")?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute(
        "UPDATE render_encoding_decisions SET body='[null]' WHERE attempt_id=?1",
        [historical.attempt_id.as_str()],
    )?;
    assert_eq!(
        store.render_attempt(&job.job_id, &current.attempt_id)?,
        current
    );
    assert!(
        store
            .render_encoding_decision(&job.job_id, &current.attempt_id)?
            .is_none()
    );
    assert!(
        store
            .render_encoding_decision(&job.job_id, &historical.attempt_id)
            .is_err()
    );
    assert!(store.validate().is_err());
    store.begin_render_encoding(&current.identity(), &selected(&job, &current)?)?;
    assert!(store.validate().is_err());
    Ok(())
}

#[test]
fn oversized_decision_is_rejected_before_recovery_changes_an_active_attempt() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("oversized.deadpan");
    let mut store = ProjectStore::create(&package, &auto_document()?)?;
    let job = automatic(&mut store, "automatic")?;
    let queued = begin(&mut store, &job, "encode")?;
    store.begin_render_encoding(&queued.identity(), &selected(&job, &queued)?)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute(
        "UPDATE render_encoding_decisions SET body=?1",
        [serde_json::json!({"oversized": "x".repeat(MAX_RENDER_DECISION_BYTES)}).to_string()],
    )?;
    assert!(
        store
            .render_encoding_decision(&job.job_id, &queued.attempt_id)
            .is_err()
    );
    let before: String =
        database.query_row("SELECT body FROM render_attempts", [], |row| row.get(0))?;
    drop(store);
    assert!(ProjectStore::open(&package, AccessMode::ReadOnly).is_err());
    assert!(ProjectStore::open(&package, AccessMode::ReadWrite).is_err());
    assert_eq!(
        database.query_row("SELECT body FROM render_attempts", [], |row| row
            .get::<_, String>(0))?,
        before
    );
    Ok(())
}
