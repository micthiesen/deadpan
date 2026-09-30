//! APFS publication mechanics with a real tiny finished-file verifier. The
//! encode transport replays retained fixtures and does not qualify edit fidelity.
use super::*;
use std::os::unix::fs::MetadataExt;

use deadpan_cli::encoded_render::{
    jobs::{self as render_jobs, RenderStageRequest},
    publication::{
        PublicationOutcome, PublicationReceipt,
        journal::{self as host, PreparedPublication, RecoveryOutcome},
    },
    verification::VerifiedCandidate,
};
use deadpan_jobs::render::{
    RenderAttemptState, RenderDiagnostic,
    publication::{
        PublicationCompletion, PublicationIntent, PublicationOutcome as StoredOutcome,
        PublicationPhase, PublicationReconciliation,
    },
};
use deadpan_store::{
    publication::PublicationPermit,
    render_jobs::{BeginRenderAttempt, StoredRenderCheckpoint},
};

fn verify_retained(
    store: &mut ProjectStore,
    stage: &mut RenderStageRequest,
    checkpoint: &StoredRenderCheckpoint,
) -> VerifiedCandidate {
    jobs::verifying(store, stage);
    let candidate = render_jobs::verify_checkpoint(
        &native(),
        stage,
        (checkpoint, &store.render_read_handle()),
        (VerificationLimits::default(), jobs::limits()),
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        candidate.verification_identity().request_id,
        stage.intent.job_id
    );
    assert_eq!(
        candidate.verification_identity().attempt_id,
        stage.attempt.attempt_id
    );
    let observation =
        render_jobs::verification_observation(&candidate, &NOT_CANCELLED, deadline()).unwrap();
    stage.attempt = store
        .record_render_verification(&stage.attempt.identity(), observation)
        .unwrap();
    assert_eq!(stage.attempt.state, RenderAttemptState::Verified);
    candidate
}

fn setup(
    fixture: &Fixture,
) -> (
    ProjectStore,
    RenderStageRequest,
    StoredRenderCheckpoint,
    VerifiedCandidate,
) {
    let (mut store, mut stage) = jobs::begin(fixture);
    let retained = jobs::retain(fixture, &store, &stage);
    let checkpoint = jobs::commit_retention(&mut store, &mut stage, &retained);
    let candidate = verify_retained(&mut store, &mut stage, &checkpoint);
    (store, stage, checkpoint, candidate)
}

fn verify_again(
    store: &mut ProjectStore,
    stage: &mut RenderStageRequest,
    checkpoint: &StoredRenderCheckpoint,
) -> VerifiedCandidate {
    stage.attempt = store
        .begin_render_attempt(BeginRenderAttempt {
            job_id: stage.intent.job_id.clone(),
            attempt_id: AttemptId::new("fresh-verification").unwrap(),
            cancellation_token: CancellationToken::new("fresh-verification-token").unwrap(),
            checkpoint_attempt_id: Some(checkpoint.encoding_attempt_id.clone()),
        })
        .unwrap();
    verify_retained(store, stage, checkpoint)
}

fn begin_publication(
    fixture: &Fixture,
    store: &mut ProjectStore,
    stage: &RenderStageRequest,
    id: &str,
) -> PublicationPermit {
    let directory = fixture.scratch.path().join("exports");
    fs::create_dir_all(&directory).unwrap();
    store
        .begin_render_publication(
            PublicationIntent {
                schema_version: 1,
                publication_id: RequestId::new(id).unwrap(),
                job_id: stage.intent.job_id.clone(),
                verified_attempt_id: stage.attempt.attempt_id.clone(),
                destination: directory.join(format!("{id}.mp4")),
            },
            AttemptId::new(format!("publish-{id}")).unwrap(),
            CancellationToken::new(format!("publish-token-{id}")).unwrap(),
        )
        .unwrap()
}

fn prepare(
    fixture: &Fixture,
    candidate: VerifiedCandidate,
    permit: &PublicationPermit,
) -> PreparedPublication {
    host::prepare(
        candidate,
        &fixture.package,
        permit,
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap()
}

fn authorize_report(
    store: &mut ProjectStore,
    prepared: &PreparedPublication,
    permit: &PublicationPermit,
) -> PublicationPermit {
    let permit = store
        .record_prepared_publication(&permit.identity(), prepared.evidence().clone())
        .unwrap();
    store
        .advance_publication(&permit.identity(), PublicationPhase::ReportCommitting)
        .unwrap()
}

fn commit_report(
    store: &mut ProjectStore,
    prepared: &mut PreparedPublication,
    permit: &PublicationPermit,
) -> PublicationPermit {
    prepared
        .commit_report(permit, &NOT_CANCELLED, deadline())
        .unwrap();
    store
        .advance_publication(&permit.identity(), PublicationPhase::ReportCommitted)
        .unwrap()
}

fn commit_movie(
    store: &mut ProjectStore,
    prepared: PreparedPublication,
    permit: &PublicationPermit,
) -> (PublicationPermit, PublicationReceipt) {
    let permit = store
        .advance_publication(&permit.identity(), PublicationPhase::MovieCommitting)
        .unwrap();
    let outcome = prepared
        .commit_movie(&permit, &NOT_CANCELLED, deadline())
        .unwrap();
    let PublicationOutcome::Published(receipt) = outcome else {
        panic!("local APFS publication must confirm its final files")
    };
    (permit, receipt)
}

fn reconciliation(
    store: &mut ProjectStore,
    stage: &RenderStageRequest,
    publication_id: &RequestId,
) -> PublicationPermit {
    store
        .begin_publication_reconciliation(
            publication_id,
            stage.attempt.attempt_id.clone(),
            AttemptId::new("reconcile").unwrap(),
            CancellationToken::new("reconcile-token").unwrap(),
        )
        .unwrap()
}

#[test]
fn each_rename_requires_its_exact_current_durable_permit() {
    let fixture = Fixture::new("nonzero");
    let (mut store, stage, _, candidate) = setup(&fixture);
    let initial = begin_publication(&fixture, &mut store, &stage, "stages");
    let destination = initial.record().intent.destination.clone();
    let report = destination
        .parent()
        .unwrap()
        .join(initial.record().intent.report_name());
    let mut prepared = prepare(&fixture, candidate, &initial);
    let retained = prepared.retained();
    assert_eq!(
        prepared
            .commit_report(&initial, &NOT_CANCELLED, deadline())
            .unwrap_err()
            .code,
        "invalid_publication"
    );
    let saved = store
        .record_prepared_publication(&initial.identity(), prepared.evidence().clone())
        .unwrap();
    assert_eq!(
        prepared
            .commit_report(&initial, &NOT_CANCELLED, deadline())
            .unwrap_err()
            .code,
        "publication_revoked"
    );
    assert_eq!(
        prepared
            .commit_report(&saved, &NOT_CANCELLED, deadline())
            .unwrap_err()
            .code,
        "invalid_publication"
    );

    // Another live permit can carry the same byte evidence. Its publication
    // identity and destination still cannot authorize these prepared files.
    let foreign = begin_publication(&fixture, &mut store, &stage, "foreign");
    let foreign = authorize_report(&mut store, &prepared, &foreign);
    assert_eq!(
        prepared
            .commit_report(&foreign, &NOT_CANCELLED, deadline())
            .unwrap_err()
            .code,
        "invalid_publication"
    );
    assert!(!report.exists());
    assert!(!destination.exists());
    assert!(retained.partial_movie.as_ref().unwrap().exists());
    assert!(retained.partial_report.as_ref().unwrap().exists());

    let authorized = store
        .advance_publication(&saved.identity(), PublicationPhase::ReportCommitting)
        .unwrap();
    let committed = commit_report(&mut store, &mut prepared, &authorized);
    assert_eq!(
        prepared
            .commit_report(&authorized, &NOT_CANCELLED, deadline())
            .unwrap_err()
            .code,
        "publication_revoked"
    );
    assert!(report.exists());
    let failure = prepared
        .commit_movie(&committed, &NOT_CANCELLED, deadline())
        .unwrap_err();
    assert_eq!(failure.error.code, "invalid_publication");
    assert!(!destination.exists());
    assert!(report.exists());
    assert_eq!(
        failure.retained.published_report.as_ref(),
        Some(&report.canonicalize().unwrap())
    );
    assert!(failure.retained.partial_movie.as_ref().unwrap().exists());
    fixture.assert_project_unchanged();
}

#[test]
fn closing_the_writer_revokes_both_report_and_movie_rename_authority() {
    for before_movie in [false, true] {
        let fixture = Fixture::new("nonzero");
        let (mut store, stage, _, candidate) = setup(&fixture);
        let initial = begin_publication(&fixture, &mut store, &stage, "closed");
        let intent = initial.record().intent.clone();
        let mut prepared = prepare(&fixture, candidate, &initial);
        let mut permit = authorize_report(&mut store, &prepared, &initial);
        if before_movie {
            permit = commit_report(&mut store, &mut prepared, &permit);
            permit = store
                .advance_publication(&permit.identity(), PublicationPhase::MovieCommitting)
                .unwrap();
        }
        let retained = prepared.retained();
        drop(store);
        let code = if before_movie {
            prepared
                .commit_movie(&permit, &NOT_CANCELLED, deadline())
                .unwrap_err()
                .error
                .code
        } else {
            prepared
                .commit_report(&permit, &NOT_CANCELLED, deadline())
                .unwrap_err()
                .code
        };
        assert_eq!(code, "publication_revoked");
        assert!(!intent.destination.exists());
        assert!(retained.partial_movie.unwrap().exists());
        let report = intent
            .destination
            .parent()
            .unwrap()
            .join(intent.report_name());
        assert_eq!(report.exists(), before_movie);
        fixture.assert_project_unchanged();
    }
}

#[test]
fn durable_report_precedes_movie_and_completed_publication_survives_reopen() {
    let fixture = Fixture::new("nonzero");
    let (mut store, stage, _, candidate) = setup(&fixture);
    let expected = candidate.report().clone();
    let initial = begin_publication(&fixture, &mut store, &stage, "roundtrip");
    let intent = initial.record().intent.clone();
    let report = intent
        .destination
        .parent()
        .unwrap()
        .join(intent.report_name());
    let mut prepared = prepare(&fixture, candidate, &initial);
    let evidence = prepared.evidence().clone();
    let retained = prepared.retained();
    assert!(!report.exists());
    assert!(!intent.destination.exists());
    let permit = authorize_report(&mut store, &prepared, &initial);
    let permit = commit_report(&mut store, &mut prepared, &permit);
    assert!(report.exists());
    assert!(!intent.destination.exists());
    let wire: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(wire["publication_id"], intent.publication_id.as_str());
    assert_eq!(
        wire["destination_readback"]["sha256"],
        expected.movie_sha256.as_str()
    );
    let (permit, receipt) = commit_movie(&mut store, prepared, &permit);
    assert_eq!(
        fs::read(&receipt.movie).unwrap(),
        fs::read(&fixture.movie_path).unwrap()
    );
    assert_eq!(receipt.movie_sha256, expected.movie_sha256);
    assert_eq!(receipt.report_sha256, evidence.report_sha256);
    assert_eq!(
        fs::metadata(&receipt.report).unwrap().len(),
        evidence.report_bytes
    );
    assert!(!retained.partial_movie.unwrap().exists());
    assert!(!retained.partial_report.unwrap().exists());
    let done = store
        .finish_publication(&permit.identity(), PublicationCompletion::Published)
        .unwrap();
    assert_eq!(done.outcome, StoredOutcome::Published);
    assert!(done.observed_movie_commit);
    assert!(permit.check_live().is_err());
    drop(store);
    let reopened = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        reopened.render_publication(&intent.publication_id).unwrap(),
        done
    );
    fixture.assert_project_unchanged();
}

#[test]
fn identical_reports_do_not_substitute_a_different_live_verification_attempt() {
    let fixture = Fixture::new("nonzero");
    let (mut store, mut stage, checkpoint, previous) = setup(&fixture);
    let fresh = verify_again(&mut store, &mut stage, &checkpoint);
    assert_eq!(previous.report(), fresh.report());
    assert_ne!(
        previous.verification_identity(),
        fresh.verification_identity()
    );
    let permit = begin_publication(&fixture, &mut store, &stage, "exact-verifier");
    let failure = host::prepare(
        previous,
        &fixture.package,
        &permit,
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .err()
    .expect("matching reports must not replace the exact verifier capability");
    assert_eq!(failure.error.code, "invalid_publication");
    assert!(failure.retained.partial_movie.is_none());
    assert!(failure.retained.partial_report.is_none());
    assert!(!permit.record().intent.destination.exists());
    let prepared = prepare(&fixture, fresh, &permit);
    assert_eq!(
        prepared.evidence().movie_sha256,
        failure.candidate.report().movie_sha256
    );
}

#[test]
fn restart_reconciliation_requires_the_original_final_objects_and_fresh_verification() {
    for change in [
        "unchanged",
        "replace-movie",
        "missing-report",
        "replace-report",
    ] {
        let fixture = Fixture::new("nonzero");
        let (mut store, mut stage, checkpoint, candidate) = setup(&fixture);
        let initial = begin_publication(&fixture, &mut store, &stage, "recovery");
        let intent = initial.record().intent.clone();
        let mut prepared = prepare(&fixture, candidate, &initial);
        let permit = authorize_report(&mut store, &prepared, &initial);
        let permit = commit_report(&mut store, &mut prepared, &permit);
        let (permit, receipt) = commit_movie(&mut store, prepared, &permit);
        // Simulate losing the completion acknowledgement after the movie rename.
        drop(store);
        assert!(permit.check_live().is_err());
        if change == "missing-report" {
            fs::remove_file(&receipt.report).unwrap();
        } else if change.starts_with("replace-") {
            let path = if change == "replace-movie" {
                &receipt.movie
            } else {
                &receipt.report
            };
            let replacement = path.with_extension("replacement");
            let original = fs::metadata(path).unwrap();
            fs::copy(path, &replacement).unwrap();
            File::options()
                .write(true)
                .open(&replacement)
                .unwrap()
                .set_modified(original.modified().unwrap())
                .unwrap();
            assert_ne!(fs::metadata(&replacement).unwrap().ino(), original.ino());
            fs::rename(&replacement, path).unwrap();
        }
        let movie_before = fs::read(&receipt.movie).unwrap();
        let report_before = fs::read(&receipt.report).ok();
        let mut store = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
        let interrupted = store.render_publication(&intent.publication_id).unwrap();
        assert_eq!(interrupted.phase, PublicationPhase::MovieCommitting);
        assert_eq!(interrupted.outcome, StoredOutcome::Interrupted);
        assert!(!interrupted.observed_movie_commit);
        assert!(
            store
                .begin_publication_reconciliation(
                    &intent.publication_id,
                    stage.attempt.attempt_id.clone(),
                    AttemptId::new("old-verification").unwrap(),
                    CancellationToken::new("old-verification-token").unwrap(),
                )
                .is_err()
        );
        let fresh = verify_again(&mut store, &mut stage, &checkpoint);
        let permit = reconciliation(&mut store, &stage, &intent.publication_id);
        let inspected = host::reconcile(&fresh, &permit, &NOT_CANCELLED, deadline());
        let done = if change == "replace-movie" {
            let error = inspected
                .err()
                .expect("identical bytes in a different file must not be adopted");
            assert_eq!(error.code, "destination_changed");
            store
                .finish_publication_reconciliation(
                    &permit.identity(),
                    PublicationReconciliation::Unresolved(RenderDiagnostic {
                        code: error.code,
                        detail: error.message,
                    }),
                )
                .unwrap()
        } else {
            let inspection = inspected.unwrap();
            assert_eq!(inspection.identity(), &permit.identity());
            if change == "unchanged" {
                assert!(matches!(inspection.outcome(),
                    RecoveryOutcome::Committed(PublicationOutcome::Published(actual)) if actual == &receipt));
            } else {
                let RecoveryOutcome::Committed(PublicationOutcome::PublishedUnconfirmed {
                    diagnostic,
                    ..
                }) = inspection.outcome()
                else {
                    panic!(
                        "an admitted movie remains committed when its report cannot be confirmed"
                    )
                };
                assert_eq!(
                    diagnostic.code,
                    if change == "missing-report" {
                        "report_missing"
                    } else {
                        "destination_changed"
                    }
                );
            }
            store
                .finish_publication_reconciliation(inspection.identity(), inspection.completion())
                .unwrap()
        };
        let expected = match change {
            "unchanged" => StoredOutcome::Published,
            "replace-movie" => StoredOutcome::Unresolved,
            _ => StoredOutcome::PublishedUnconfirmed,
        };
        assert_eq!(done.outcome, expected);
        assert_eq!(done.observed_movie_commit, change != "replace-movie");
        assert_eq!(fs::read(&receipt.movie).unwrap(), movie_before);
        assert_eq!(fs::read(&receipt.report).ok(), report_before);
        drop(store);
        let reopened = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
        assert_eq!(
            reopened.render_publication(&intent.publication_id).unwrap(),
            done
        );
        fixture.assert_project_unchanged();
    }
}

#[test]
fn interruption_before_movie_authorization_never_adopts_or_touches_foreign_files() {
    for after_report in [false, true] {
        let fixture = Fixture::new("nonzero");
        let (mut store, mut stage, checkpoint, candidate) = setup(&fixture);
        let initial = begin_publication(&fixture, &mut store, &stage, "not-published");
        let intent = initial.record().intent.clone();
        if after_report {
            let mut prepared = prepare(&fixture, candidate, &initial);
            let permit = authorize_report(&mut store, &prepared, &initial);
            commit_report(&mut store, &mut prepared, &permit);
        } else {
            drop(candidate);
        }
        drop(store);
        let directory = intent.destination.parent().unwrap();
        let paths = [
            intent.destination.clone(),
            directory.join(intent.report_name()),
            directory.join(intent.movie_partial_name()),
            directory.join(intent.report_partial_name()),
        ];
        for path in &paths {
            fs::write(path, b"foreign user-owned bytes").unwrap();
        }
        let mut store = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
        let interrupted = store.render_publication(&intent.publication_id).unwrap();
        assert_eq!(interrupted.outcome, StoredOutcome::Interrupted);
        assert_eq!(
            interrupted.phase,
            if after_report {
                PublicationPhase::ReportCommitted
            } else {
                PublicationPhase::Intent
            }
        );
        let fresh = verify_again(&mut store, &mut stage, &checkpoint);
        let permit = reconciliation(&mut store, &stage, &intent.publication_id);
        let inspection = host::reconcile(&fresh, &permit, &NOT_CANCELLED, deadline()).unwrap();
        assert!(matches!(
            inspection.outcome(),
            RecoveryOutcome::NotPublished
        ));
        let done = store
            .finish_publication_reconciliation(inspection.identity(), inspection.completion())
            .unwrap();
        assert_eq!(done.outcome, StoredOutcome::Failed);
        assert!(!done.observed_movie_commit);
        for path in paths {
            assert_eq!(fs::read(path).unwrap(), b"foreign user-owned bytes");
        }
        fixture.assert_project_unchanged();
    }
}
