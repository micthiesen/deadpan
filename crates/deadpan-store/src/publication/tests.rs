use super::*;
use crate::{AccessMode, publication_durability::Step};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
fn fixture() -> Result<(tempfile::TempDir, ProjectStore)> {
    let root = tempfile::tempdir()?;
    let package = root.path().join("fixture.deadpan");
    std::fs::create_dir(&package)?;
    for name in [
        "Snapshots",
        "Media/Originals",
        "Media/Generated",
        "Media/RenderCandidates",
    ] {
        std::fs::create_dir_all(package.join(name))?;
    }
    let db = Connection::open(package.join("project.sqlite"))?;
    // The authentic SQLite dump is in table-name order, not foreign-key order.
    db.pragma_update(None, "foreign_keys", false)?;
    db.execute_batch(include_str!(
        "../../tests/fixtures/v40-publication-render.sql"
    ))?;
    drop(db);
    ProjectStore::migrate(&package)?;
    let store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    Ok((root, store))
}
fn begin(store: &mut ProjectStore) -> Result<PublicationPermit> {
    Ok(store.begin_render_publication(
        PublicationIntent {
            schema_version: 1,
            publication_id: RequestId::new("publication")?,
            job_id: RequestId::new("durable-structural")?,
            verified_attempt_id: AttemptId::new("structural-verify-2")?,
            destination: "/tmp/journal-fixture.mp4".into(),
        },
        AttemptId::new("publication-op")?,
        CancellationToken::new("publication-token")?,
    )?)
}
#[test]
fn post_commit_barrier_failures_keep_rows_but_revoke_permits_and_require_reopen() -> Result {
    for step in [
        Step::BeforeIdentity,
        Step::DatabaseFlush,
        Step::WalFlush,
        Step::DirectoryFlush,
        Step::AfterIdentity,
    ] {
        let (_root, mut store) = fixture()?;
        let first = begin(&mut store)?;
        let evidence = PreparedPublicationEvidence {
            schema_version: 1,
            movie_sha256: first.record.movie_sha256.clone(),
            movie_bytes: first.record.movie_bytes,
            report_sha256: deadpan_jobs::Sha256::new("a".repeat(64))?,
            report_bytes: 512,
            contains_generated_pictures: false,
            filesystem: serde_json::json!({"fixture":true}),
        };
        store.publication_durability.as_mut().unwrap().fail_at(step);
        assert!(
            store
                .record_prepared_publication(&first.identity(), evidence.clone())
                .is_err()
        );
        assert!(first.check_live().is_err());
        let committed = store.render_publication(&first.record.intent.publication_id)?;
        assert_eq!(committed.sequence, first.record.sequence + 1);
        assert_eq!(committed.prepared, Some(evidence));
        assert_eq!(committed.phase, PublicationPhase::Prepared);
        assert!(
            store
                .advance_publication(&committed.identity(), PublicationPhase::ReportCommitting)
                .is_err()
        );
        let path = store.package.clone();
        drop(store);
        let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
        assert_eq!(
            reader.render_publication(&committed.intent.publication_id)?,
            committed
        );
        drop(reader);
        let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(
            reopened
                .render_publication(&committed.intent.publication_id)?
                .outcome,
            PublicationOutcome::Interrupted
        );
    }
    Ok(())
}
#[test]
fn writers_request_full_sqlite_durability() -> Result {
    let (_root, store) = fixture()?;
    assert_eq!(
        store
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))?,
        "wal"
    );
    assert_eq!(
        store
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        store
            .connection
            .pragma_query_value(None, "fullfsync", |row| row.get::<_, i64>(0))?,
        1
    );
    Ok(())
}
#[test]
fn altered_unrelated_history_is_found_by_full_audit_without_loading_it_for_selected_reads() -> Result
{
    let (_root, mut store) = fixture()?;
    let first = begin(&mut store)?;
    let completed = store.finish_publication(
        &first.identity(),
        PublicationCompletion::Failed(RenderDiagnostic {
            code: "Fixture".into(),
            detail: "Fixture no-commit failure".into(),
        }),
    )?;
    let mut second_intent = completed.intent.clone();
    second_intent.publication_id = RequestId::new("second")?;
    let second = store.begin_render_publication(
        second_intent,
        AttemptId::new("second-op")?,
        CancellationToken::new("second-token")?,
    )?;
    store.connection.execute("UPDATE render_publication_operations SET body=json_set(body,'$.diagnostic',NULL) WHERE operation_id=?1",[completed.operation.operation_id.as_str()])?;
    assert_eq!(
        store.render_publication(&second.record.intent.publication_id)?,
        second.record
    );
    assert!(store.validate().is_err());
    Ok(())
}
#[test]
fn last_sequence_is_reserved_for_a_terminal_result() -> Result {
    let (_root, mut store) = fixture()?;
    let permit = begin(&mut store)?;
    let mut record = permit.record.clone();
    record.sequence = MAX_RENDER_COUNTER - 1;
    write_record(&store.connection, &record)?;
    let evidence = PreparedPublicationEvidence {
        schema_version: 1,
        movie_sha256: record.movie_sha256.clone(),
        movie_bytes: record.movie_bytes,
        report_sha256: deadpan_jobs::Sha256::new("a".repeat(64))?,
        report_bytes: 512,
        contains_generated_pictures: false,
        filesystem: serde_json::json!({"fixture":true}),
    };
    assert!(
        store
            .record_prepared_publication(&record.identity(), evidence)
            .is_err()
    );
    assert!(
        store
            .request_publication_cancellation(&record.identity())
            .is_err()
    );
    let failed = store.finish_publication(
        &record.identity(),
        PublicationCompletion::Failed(RenderDiagnostic {
            code: "Fixture".into(),
            detail: "Definite no-commit failure".into(),
        }),
    )?;
    assert_eq!(failed.sequence, MAX_RENDER_COUNTER);
    assert_eq!(failed.outcome, PublicationOutcome::Failed);
    store.validate()?;
    Ok(())
}
#[test]
fn changed_package_database_revokes_all_previously_issued_permits() -> Result {
    let (_root, mut store) = fixture()?;
    let permit = begin(&mut store)?;
    let db = store.package.join("project.sqlite");
    std::fs::rename(&db, store.package.join("replaced.sqlite"))?;
    std::fs::write(&db, b"unrelated bytes")?;
    assert!(
        store
            .request_publication_cancellation(&permit.identity())
            .is_err()
    );
    assert!(permit.check_live().is_err());
    Ok(())
}
