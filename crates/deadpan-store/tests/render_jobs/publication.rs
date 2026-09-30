//! Opaque byte fixtures test journal semantics, not decoded-media validity.
use super::*;
use deadpan_jobs::render::publication::*;
use deadpan_store::publication::PublicationPermit;
fn verified_fixture(store: &mut ProjectStore) -> Result<StoredRenderAttempt> {
    let job = create(store, "publication-job")?;
    let running = encoding(store, &job, "publication-encode")?;
    let token = retention(store, &running)?;
    let retained = store.retain_render_checkpoint(
        &running.identity(),
        &token,
        &AtomicBool::new(false),
        deadline(),
    )?;
    let checkpoint = store.render_checkpoint(&job.job_id, &running.attempt_id)?;
    let verifying = store
        .transition_render_attempt(&retained.identity(), RenderAttemptTransition::Verifying)?;
    Ok(store.record_render_verification(&verifying.identity(), observation(&checkpoint))?)
}
fn verify_again(
    store: &mut ProjectStore,
    previous: &StoredRenderAttempt,
    name: &str,
) -> Result<StoredRenderAttempt> {
    let queued = store.begin_render_attempt(BeginRenderAttempt {
        job_id: previous.job_id.clone(),
        attempt_id: AttemptId::new(name)?,
        cancellation_token: CancellationToken::new(format!("cancel-{name}"))?,
        checkpoint_attempt_id: previous.checkpoint_attempt_id.clone(),
    })?;
    let verifying =
        store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Verifying)?;
    let checkpoint = store.render_checkpoint(
        &previous.job_id,
        previous.checkpoint_attempt_id.as_ref().unwrap(),
    )?;
    Ok(store.record_render_verification(&verifying.identity(), observation(&checkpoint))?)
}
fn begin_publication(
    store: &mut ProjectStore,
    attempt: &StoredRenderAttempt,
    id: &str,
) -> Result<PublicationPermit> {
    Ok(store.begin_render_publication(
        PublicationIntent {
            schema_version: 1,
            publication_id: RequestId::new(id)?,
            job_id: attempt.job_id.clone(),
            verified_attempt_id: attempt.attempt_id.clone(),
            destination: std::env::temp_dir().join(format!("publication-{id}.mp4")),
        },
        AttemptId::new(format!("op-{id}"))?,
        CancellationToken::new(format!("pub-cancel-{id}"))?,
    )?)
}
fn prepared(permit: &PublicationPermit) -> PreparedPublicationEvidence {
    PreparedPublicationEvidence {
        schema_version: 1,
        movie_sha256: permit.record().movie_sha256.clone(),
        movie_bytes: permit.record().movie_bytes,
        report_sha256: deadpan_jobs::Sha256::new("a".repeat(64)).unwrap(),
        report_bytes: 512,
        contains_generated_pictures: false,
        filesystem: serde_json::json!({"opaque_fixture":true}),
    }
}
fn through_movie(store: &mut ProjectStore, permit: PublicationPermit) -> Result<PublicationPermit> {
    let mut current = store.record_prepared_publication(&permit.identity(), prepared(&permit))?;
    assert!(permit.check_live().is_err());
    for phase in [
        PublicationPhase::ReportCommitting,
        PublicationPhase::ReportCommitted,
        PublicationPhase::MovieCommitting,
    ] {
        let next = store.advance_publication(&current.identity(), phase)?;
        assert!(current.check_live().is_err());
        current = next;
    }
    Ok(current)
}
fn diagnostic() -> RenderDiagnostic {
    RenderDiagnostic {
        code: "Fixture".into(),
        detail: "Controlled journal fixture outcome".into(),
    }
}
#[test]
fn stages_require_exact_live_sequence_and_matching_movie() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let verified = verified_fixture(&mut store)?;
    let first = begin_publication(&mut store, &verified, "pub")?;
    first.check_live()?;
    assert!(
        store
            .advance_publication(&first.identity(), PublicationPhase::MovieCommitting)
            .is_err()
    );
    let mut wrong = prepared(&first);
    wrong.movie_bytes += 1;
    assert!(
        store
            .record_prepared_publication(&first.identity(), wrong)
            .is_err()
    );
    let mut wrong = first.identity();
    wrong.cancellation_token = CancellationToken::new("foreign")?;
    assert!(store.request_publication_cancellation(&wrong).is_err());
    let current = through_movie(&mut store, first)?;
    let done = store.finish_publication(&current.identity(), PublicationCompletion::Published)?;
    assert!(current.check_live().is_err());
    assert!(done.observed_movie_commit);
    assert_eq!(done.outcome, PublicationOutcome::Published);
    assert!(
        store
            .finish_publication(
                &done.identity(),
                PublicationCompletion::Failed(diagnostic())
            )
            .is_err()
    );
    store.validate()?;
    Ok(())
}
#[test]
fn cancellation_revokes_permit_but_racing_movie_commit_remains_recordable() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let verified = verified_fixture(&mut store)?;
    let first = begin_publication(&mut store, &verified, "pub")?;
    let current = through_movie(&mut store, first)?;
    let cancelled = store.request_publication_cancellation(&current.identity())?;
    assert!(current.check_live().is_err());
    assert!(cancelled.operation.active);
    let done = store.finish_publication(
        &cancelled.identity(),
        PublicationCompletion::PublishedUnconfirmed(diagnostic()),
    )?;
    assert!(done.cancellation_requested);
    assert!(done.observed_movie_commit);
    assert_eq!(done.outcome, PublicationOutcome::PublishedUnconfirmed);
    store.validate()?;
    Ok(())
}
#[test]
fn definite_cancel_before_commit_cannot_be_adopted_as_published() -> Result {
    let root = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&root.path().join("project.deadpan"), &document()?)?;
    let verified = verified_fixture(&mut store)?;
    let first = begin_publication(&mut store, &verified, "pub")?;
    let cancelled = store.request_publication_cancellation(&first.identity())?;
    let done = store.finish_publication(&cancelled.identity(), PublicationCompletion::Cancelled)?;
    let fresh = verify_again(&mut store, &verified, "new-verify")?;
    assert!(
        store
            .begin_publication_reconciliation(
                &done.intent.publication_id,
                fresh.attempt_id,
                AttemptId::new("reconcile")?,
                CancellationToken::new("reconcile-cancel")?
            )
            .is_err()
    );
    assert!(!done.observed_movie_commit);
    store.validate()?;
    Ok(())
}
#[test]
fn readonly_preserves_active_phase_reopen_interrupts_without_touching_destination() -> Result {
    for phase in [
        PublicationPhase::Intent,
        PublicationPhase::Prepared,
        PublicationPhase::ReportCommitting,
        PublicationPhase::ReportCommitted,
        PublicationPhase::MovieCommitting,
    ] {
        let root = tempfile::tempdir()?;
        let package = root.path().join("project.deadpan");
        let mut store = ProjectStore::create(&package, &document()?)?;
        let verified = verified_fixture(&mut store)?;
        let mut permit = begin_publication(&mut store, &verified, "pub")?;
        if phase != PublicationPhase::Intent {
            permit = store.record_prepared_publication(&permit.identity(), prepared(&permit))?;
        }
        for next in [
            PublicationPhase::ReportCommitting,
            PublicationPhase::ReportCommitted,
            PublicationPhase::MovieCommitting,
        ] {
            if permit.record().phase == phase {
                break;
            }
            permit = store.advance_publication(&permit.identity(), next)?;
        }
        let before = permit.record().clone();
        drop(store);
        assert!(permit.check_live().is_err());
        let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        assert_eq!(
            reader.render_publication(&before.intent.publication_id)?,
            before
        );
        drop(reader);
        let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        let interrupted = store.render_publication(&before.intent.publication_id)?;
        assert_eq!(interrupted.phase, phase);
        assert_eq!(interrupted.prepared, before.prepared);
        assert_eq!(interrupted.outcome, PublicationOutcome::Interrupted);
        assert_eq!(interrupted.sequence, before.sequence + 1);
        assert!(
            store
                .begin_publication_reconciliation(
                    &before.intent.publication_id,
                    verified.attempt_id.clone(),
                    AttemptId::new("stale")?,
                    CancellationToken::new("stale-token")?
                )
                .is_err()
        );
        let fresh = verify_again(&mut store, &verified, "fresh-verify")?;
        let reconcile = store.begin_publication_reconciliation(
            &before.intent.publication_id,
            fresh.attempt_id,
            AttemptId::new("reconcile")?,
            CancellationToken::new("reconcile-token")?,
        )?;
        let result = if phase == PublicationPhase::MovieCommitting {
            PublicationReconciliation::Unresolved(diagnostic())
        } else {
            PublicationReconciliation::NotPublished(diagnostic())
        };
        let done = store.finish_publication_reconciliation(&reconcile.identity(), result)?;
        assert_eq!(
            done.outcome,
            if phase == PublicationPhase::MovieCommitting {
                PublicationOutcome::Unresolved
            } else {
                PublicationOutcome::Failed
            }
        );
        assert_eq!(done.render_intent, before.render_intent);
        assert!(!done.observed_movie_commit);
        store.validate()?;
    }
    Ok(())
}
#[test]
fn reconciliation_preserves_positive_commit_through_unavailable_report_and_reopen() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let verified = verified_fixture(&mut store)?;
    let first = begin_publication(&mut store, &verified, "pub")?;
    let current = through_movie(&mut store, first)?;
    let uncertain = store.finish_publication(
        &current.identity(),
        PublicationCompletion::Unresolved(diagnostic()),
    )?;
    let fresh = verify_again(&mut store, &verified, "fresh-one")?;
    let reconcile = store.begin_publication_reconciliation(
        &uncertain.intent.publication_id,
        fresh.attempt_id.clone(),
        AttemptId::new("reconcile-one")?,
        CancellationToken::new("token-one")?,
    )?;
    let observed = store.finish_publication_reconciliation(
        &reconcile.identity(),
        PublicationReconciliation::CommittedUnconfirmed(diagnostic()),
    )?;
    assert!(observed.observed_movie_commit);
    assert_eq!(observed.outcome, PublicationOutcome::PublishedUnconfirmed);
    let fresh2 = verify_again(&mut store, &fresh, "fresh-two")?;
    let reconcile = store.begin_publication_reconciliation(
        &observed.intent.publication_id,
        fresh2.attempt_id.clone(),
        AttemptId::new("reconcile-two")?,
        CancellationToken::new("token-two")?,
    )?;
    assert!(reconcile.record().observed_movie_commit);
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let recovered = store.render_publication(&observed.intent.publication_id)?;
    assert_eq!(recovered.outcome, PublicationOutcome::PublishedUnconfirmed);
    assert!(recovered.observed_movie_commit);
    let fresh3 = verify_again(&mut store, &fresh2, "fresh-three")?;
    let reconcile = store.begin_publication_reconciliation(
        &observed.intent.publication_id,
        fresh3.attempt_id,
        AttemptId::new("reconcile-three")?,
        CancellationToken::new("token-three")?,
    )?;
    let done = store.finish_publication_reconciliation(
        &reconcile.identity(),
        PublicationReconciliation::Confirmed,
    )?;
    assert_eq!(done.outcome, PublicationOutcome::Published);
    assert!(done.observed_movie_commit);
    assert_eq!(
        store
            .publication_operations(&observed.intent.publication_id, 0, 256)?
            .len(),
        4
    );
    store.validate()?;
    Ok(())
}
#[test]
fn bounds_reused_operation_and_tokens_fail_without_mutating_record() -> Result {
    let root = tempfile::tempdir()?;
    let mut store = ProjectStore::create(&root.path().join("project.deadpan"), &document()?)?;
    let verified = verified_fixture(&mut store)?;
    let first = begin_publication(&mut store, &verified, "pub")?;
    let mut huge = prepared(&first);
    huge.filesystem = serde_json::json!({"oversize":"x".repeat(MAX_PUBLICATION_FILESYSTEM_BYTES)});
    assert!(
        store
            .record_prepared_publication(&first.identity(), huge)
            .is_err()
    );
    first.check_live()?;
    let mut other = first.record().intent.clone();
    other.publication_id = RequestId::new("other")?;
    assert!(
        store
            .begin_render_publication(
                other.clone(),
                first.record().operation.operation_id.clone(),
                CancellationToken::new("new-token")?
            )
            .is_err()
    );
    assert!(
        store
            .begin_render_publication(
                other,
                AttemptId::new("new-operation")?,
                first.record().operation.cancellation_token.clone()
            )
            .is_err()
    );
    assert_eq!(
        store.render_publication(&first.record().intent.publication_id)?,
        *first.record()
    );
    assert!(store.render_publications(None, 0).is_err());
    assert!(store.render_publications(None, 257).is_err());
    assert_eq!(store.render_publications(None, 1)?.len(), 1);
    store.validate()?;
    Ok(())
}
