//! The automatic AI variant retention pass of a writable session.

use super::*;
use crate::project::{RetentionPassState, RetentionPassStatus};

fn retention(update: &ProjectUpdate) -> Option<&RetentionPassStatus> {
    update.storage_retention.as_ref()
}

/// The latest retention status, including quiet changes the service does
/// not publish, once `ready` holds.
pub(super) fn quiet_status(
    service: &ProjectService,
    ready: impl Fn(&RetentionPassStatus) -> bool,
) -> RetentionPassStatus {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(status) = service
            .shared
            .retention_status
            .lock()
            .unwrap()
            .clone()
            .filter(|status| ready(status))
        {
            return status;
        }
        assert!(Instant::now() < deadline, "retention status timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn done(status: &RetentionPassStatus, session: u64) -> bool {
    status.session == session && matches!(status.state, RetentionPassState::Done { .. })
}

/// The pass waits while the session's import prepares, then runs once the
/// writer is idle, without publishing an update when it does nothing;
/// reopening runs it again for the new session.
#[test]
fn the_automatic_pass_defers_while_an_import_runs_and_runs_when_idle() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let created = command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        },
    );
    let session = created.workspace.as_ref().unwrap().session;
    // The import's first preparation is held by the test: the pass waits.
    let job = harness.job();
    quiet_status(&harness.service, |status| {
        status.session == session
            && matches!(&status.state, RetentionPassState::Deferred(reason) if reason.contains("import"))
    });
    harness.finish(job);
    harness.finish(harness.job());
    let completed = complete(&harness.service);
    let status = quiet_status(&harness.service, |status| done(status, session));
    let RetentionPassState::Done {
        expired,
        removed_files,
        ..
    } = status.state
    else {
        unreachable!()
    };
    // A new project has no variants and nothing a day old.
    assert_eq!((expired, removed_files), (0, 0));
    // Doing nothing publishes nothing.
    std::thread::sleep(Duration::from_millis(50));
    assert!(harness.service.take_update().is_none());
    let path = completed.path.clone();

    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path.clone()));
    let session = reopened.workspace.as_ref().unwrap().session;
    quiet_status(&harness.service, |status| done(status, session));

    // A clock earlier than the last recorded check expires and removes
    // nothing, and says so in a published update.
    command(&harness.service, ProjectRequest::Close);
    let future = std::time::SystemTime::now() + Duration::from_secs(10 * 24 * 60 * 60);
    let future_ms = future
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let database = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
    database
        .execute(
            "UPDATE generation_retention_state SET last_pass_ms=?1",
            [future_ms],
        )
        .unwrap();
    drop(database);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    let session = reopened.workspace.as_ref().unwrap().session;
    wait(&harness.service, |update| {
        retention(update).is_some_and(|status| {
            status.session == session
                && matches!(&status.state, RetentionPassState::ClockAnomaly(text) if text.contains("clock"))
        })
    });
}

/// Off, a session never starts the automatic check.
#[test]
fn the_automatic_check_can_be_turned_off() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    harness.service.set_automatic_retention(false);
    let workspace = create(&harness.service, &scratch.path().join("off.deadpan"));
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        harness
            .service
            .shared
            .retention_status
            .lock()
            .unwrap()
            .is_none()
    );
    let watermark: Option<i64> = rusqlite::Connection::open(workspace.path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT max(last_pass_ms) FROM generation_retention_state",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(watermark, None);
}

/// The Storage panel's clock confirmation applies an explicit plan made off
/// the writer: refused for another revision or a clock behind the records,
/// accepted after a long gap, which advances the watermark.
#[test]
fn confirming_the_clock_applies_a_reviewed_plan_and_refuses_a_clock_behind() {
    use deadpan_store::generation_retention::{DEFAULT_VARIANT_RETENTION, ExpiryMode};
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("clock.deadpan");
    let harness = Harness::new();
    let workspace = create(&harness.service, &path);
    quiet_status(&harness.service, |status| done(status, workspace.session));
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let set_watermark = |ms: i64| {
        rusqlite::Connection::open(workspace.path.join("project.sqlite"))
            .unwrap()
            .execute(
                "UPDATE generation_retention_state SET last_pass_ms=?1",
                [ms],
            )
            .unwrap();
    };
    let watermark = || -> i64 {
        rusqlite::Connection::open(workspace.path.join("project.sqlite"))
            .unwrap()
            .query_row(
                "SELECT last_pass_ms FROM generation_retention_state",
                [],
                |row| row.get(0),
            )
            .unwrap()
    };
    let plan = || {
        Box::new(
            ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
                .unwrap()
                .plan_generation_expiry(
                    std::time::SystemTime::now(),
                    DEFAULT_VARIANT_RETENTION,
                    ExpiryMode::Explicit,
                )
                .unwrap(),
        )
    };
    let confirm = |ticket, revision: RevisionId| {
        let update = command(
            &harness.service,
            ProjectRequest::ConfirmVariantClock {
                ticket,
                expected_session: workspace.session,
                expected_revision: revision,
                plan: plan(),
            },
        );
        let confirmation = retention(&update).unwrap().confirmation.clone().unwrap();
        assert_eq!(confirmation.ticket, ticket);
        confirmation.result
    };
    let long_ago = now_ms - 30 * 24 * 60 * 60 * 1000;
    set_watermark(long_ago);
    let stale = confirm(1, RevisionId::new("another-revision").unwrap());
    assert!(stale.unwrap_err().contains("changed"));
    assert_eq!(watermark(), long_ago);
    let confirmed = confirm(2, workspace.document.revision_id().clone());
    assert!(confirmed.unwrap().expired.is_empty());
    assert!(watermark() > long_ago + 29 * 24 * 60 * 60 * 1000);

    let ahead = now_ms + 10 * 24 * 60 * 60 * 1000;
    set_watermark(ahead);
    let behind = confirm(3, workspace.document.revision_id().clone());
    assert!(behind.unwrap_err().contains("earlier"));
    assert_eq!(watermark(), ahead);
}
