#![cfg(any(target_os = "macos", target_os = "linux"))]
use deadpan_core::*;
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, render::*};
use deadpan_store::{AccessMode, ProjectStore, render_jobs::*};
use rusqlite::{Connection, params};
use std::{
    collections::BTreeMap,
    error::Error,
    os::unix::fs::PermissionsExt,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

#[test]
fn render_workflow_owner_requires_the_same_writable_open() -> Result {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("owner.deadpan");
    let store = ProjectStore::create(&path, &document()?)?;
    let owner = store.render_read_handle();
    store.check_render_owner(&owner.clone())?;

    let read_only = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(read_only.check_render_owner(&owner).is_err());
    assert!(
        read_only
            .check_render_owner(&read_only.render_read_handle())
            .is_err()
    );
    assert!(
        store
            .check_render_owner(&read_only.render_read_handle())
            .is_err()
    );
    let other = ProjectStore::create(&temporary.path().join("same-project.deadpan"), &document()?)?;
    assert!(other.check_render_owner(&owner).is_err());

    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(reopened.check_render_owner(&owner).is_err());
    reopened.check_render_owner(&reopened.render_read_handle())?;
    Ok(())
}

#[test]
fn render_workflow_admission_survives_unconfirmed_lease_destruction() -> Result {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("admission.deadpan");
    let store = ProjectStore::create(&path, &document()?)?;
    let read_only = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(read_only.acquire_render_workflow().is_err());

    let lease = store.acquire_render_workflow()?;
    assert!(store.acquire_render_workflow().is_err());
    lease.release();
    let lease = store.acquire_render_workflow()?;
    drop(lease);
    assert!(store.acquire_render_workflow().is_err());

    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    reopened.acquire_render_workflow()?.release();
    Ok(())
}

fn document() -> Result<ProjectDocument> {
    document_with_basis(
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        12,
    )
}
fn document_with_basis(basis: PresentationBasis, duration: i64) -> Result<ProjectDocument> {
    let document = ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("initial")?,
        basis,
        NodeId::new("root")?,
    )?;
    let edit = deadpan_core::apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("baseline")?,
            command: Command::Insert {
                parent: document.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: NodeId::new("hold")?,
                    nodes: BTreeMap::from([(
                        NodeId::new("hold")?,
                        BeatNode::hold(
                            "pause",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(duration)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        },
    )?;
    Ok(edit.forward.apply(&document)?)
}
fn intent(store: &ProjectStore, name: &str) -> Result<RenderIntent> {
    let document = store.snapshot()?;
    Ok(RenderIntent {
        schema_version: 1,
        job_id: RequestId::new(name)?,
        project_id: document.project_id().clone(),
        revision_id: document.revision_id().clone(),
        document_sha256: document_sha256(&document, &AtomicBool::new(false), deadline())?,
        range: FrameRange::new(ProjectFrame(0), ProjectFrame(12))?,
        policy: RenderPolicy::Engineering(RenderEngineeringPolicy {
            schema_version: 1,
            selection: RenderSelection::ExplicitEngineering,
            encoder: RenderEncoder::Software,
            b_frames: RenderBFrames::None,
        }),
    })
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn create(store: &mut ProjectStore, name: &str) -> Result<RenderIntent> {
    let intent = intent(store, name)?;
    Ok(store.create_render_job(intent, &AtomicBool::new(false), deadline())?)
}
fn begin(store: &mut ProjectStore, job: &RenderIntent, name: &str) -> Result<StoredRenderAttempt> {
    Ok(store.begin_render_attempt(BeginRenderAttempt {
        job_id: job.job_id.clone(),
        attempt_id: AttemptId::new(name)?,
        cancellation_token: CancellationToken::new(format!("cancel-{name}"))?,
        checkpoint_attempt_id: None,
    })?)
}
fn authored(database: &Connection) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        result.extend(
            database
                .prepare(query)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(result)
}
fn fail(store: &mut ProjectStore, attempt: &StoredRenderAttempt) -> Result<StoredRenderAttempt> {
    Ok(store.transition_render_attempt(
        &attempt.identity(),
        RenderAttemptTransition::Failed(RenderDiagnostic {
            code: "FixtureFailure".into(),
            detail: "Explicit test failure".into(),
        }),
    )?)
}

#[test]
fn captured_history_and_operational_writes_preserve_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let captured = create(&mut store, "job")?;
    let document = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("edit")?,
        command: Command::Delete {
            node: NodeId::new("hold")?,
        },
    })?;
    store.undo(&RevisionId::new("edit")?, RevisionId::new("undo")?)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = authored(&database)?;
    let queued = begin(&mut store, &captured, "first")?;
    let running =
        store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Encoding)?;
    fail(&mut store, &running)?;
    let retry = begin(&mut store, &captured, "second")?;
    assert_eq!(retry.ordinal, 2);
    fail(&mut store, &retry)?;
    assert_eq!(store.render_job(&captured.job_id)?, captured);
    assert_eq!(authored(&database)?, before);
    store.redo(&RevisionId::new("undo")?, RevisionId::new("redo")?)?;
    assert_eq!(store.render_job(&captured.job_id)?, captured);
    store.validate()?;
    Ok(())
}

#[test]
fn stale_tokens_sequences_and_fresh_id_rules_fail_atomically() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let job = create(&mut store, "job")?;
    let other = create(&mut store, "other")?;
    let queued = begin(&mut store, &job, "first")?;
    assert!(begin(&mut store, &other, "another").is_err());
    let running =
        store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Encoding)?;
    assert!(
        store
            .transition_render_attempt(
                &queued.identity(),
                RenderAttemptTransition::RequestCancellation
            )
            .is_err()
    );
    let mut wrong = running.identity();
    wrong.cancellation_token = CancellationToken::new("foreign")?;
    assert!(
        store
            .transition_render_attempt(&wrong, RenderAttemptTransition::RequestCancellation)
            .is_err()
    );
    wrong = running.identity();
    wrong.job_id = other.job_id.clone();
    assert!(
        store
            .transition_render_attempt(&wrong, RenderAttemptTransition::RequestCancellation)
            .is_err()
    );
    assert_eq!(
        store.render_attempt(&job.job_id, &running.attempt_id)?,
        running
    );
    let cancelling = store.transition_render_attempt(
        &running.identity(),
        RenderAttemptTransition::RequestCancellation,
    )?;
    assert!(!cancelling.state.is_terminal());
    assert!(begin(&mut store, &job, "second").is_err());
    store.transition_render_attempt(
        &cancelling.identity(),
        RenderAttemptTransition::FinishCancelled,
    )?;
    assert!(begin(&mut store, &job, "first").is_err());
    assert!(
        store
            .begin_render_attempt(BeginRenderAttempt {
                job_id: job.job_id.clone(),
                attempt_id: AttemptId::new("second")?,
                cancellation_token: queued.cancellation_token,
                checkpoint_attempt_id: None
            })
            .is_err()
    );
    let second = begin(&mut store, &job, "second")?;
    assert_eq!(second.ordinal, 2);
    assert_eq!(second.transition_sequence, 1);
    Ok(())
}

#[test]
fn readonly_open_never_recovers_and_writer_preserves_cancel_intent() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let job = create(&mut store, "job")?;
    let queued = begin(&mut store, &job, "first")?;
    let cancelling = store.transition_render_attempt(
        &queued.identity(),
        RenderAttemptTransition::RequestCancellation,
    )?;
    drop(store);
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(
        reader.render_attempt(&job.job_id, &queued.attempt_id)?,
        cancelling
    );
    drop(reader);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let interrupted = writer.render_attempt(&job.job_id, &queued.attempt_id)?;
    assert_eq!(interrupted.state, RenderAttemptState::Interrupted);
    assert!(interrupted.cancellation_requested);
    assert_eq!(
        interrupted.transition_sequence,
        cancelling.transition_sequence + 1
    );
    let retry = begin(&mut writer, &job, "retry")?;
    assert_eq!(retry.ordinal, 2);
    fail(&mut writer, &retry)?;
    drop(writer);
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(
        writer.render_attempt(&job.job_id, &queued.attempt_id)?,
        interrupted
    );
    Ok(())
}

#[test]
fn invalid_immutable_binding_and_paging_are_rejected() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let valid = intent(&store, "job")?;
    for variation in 0..5 {
        let mut value = valid.clone();
        match variation {
            0 => value.project_id = ProjectId::new("foreign")?,
            1 => value.document_sha256 = deadpan_jobs::Sha256::new("0".repeat(64))?,
            2 => value.range = FrameRange::new(ProjectFrame(0), ProjectFrame(13))?,
            3 => {
                let RenderPolicy::Engineering(policy) = &mut value.policy else {
                    unreachable!("fixture uses engineering policy");
                };
                policy.schema_version = 2;
            }
            _ => value.revision_id = RevisionId::new("missing")?,
        }
        assert!(
            store
                .create_render_job(value, &AtomicBool::new(false), deadline())
                .is_err()
        );
        assert!(store.render_jobs(None, 256)?.is_empty());
    }
    store.create_render_job(valid.clone(), &AtomicBool::new(false), deadline())?;
    assert!(
        store
            .create_render_job(valid, &AtomicBool::new(false), deadline())
            .is_err()
    );
    assert!(store.render_jobs(None, 0).is_err());
    assert!(store.render_jobs(None, 257).is_err());
    Ok(())
}

#[test]
fn missing_indexes_malformed_states_and_reports_block_recovery() -> Result {
    for corruption in 0..5 {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("project.deadpan");
        let mut store = ProjectStore::create(&package, &document()?)?;
        let job = create(&mut store, "job")?;
        begin(&mut store, &job, "first")?;
        drop(store);
        let database = Connection::open(package.join("project.sqlite"))?;
        match corruption {
            0=>database.execute_batch("DROP INDEX one_active_render_attempt")?,
            1=>database.execute_batch("DROP INDEX unique_render_cancellation_token")?,
            2=>database.execute_batch("UPDATE render_attempts SET state='verified',body=json_set(body,'$.state','verified')")?,
            3=>database.execute_batch("UPDATE render_attempts SET body=json_set(body,'$.verification',json('{\"schema_version\":99}'))")?,
            _=>database.execute_batch("UPDATE render_job_heads SET high_water=2")?,
        }
        let before: Vec<String> = database
            .prepare("SELECT body FROM render_attempts")?
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        assert!(ProjectStore::open(&package, AccessMode::ReadWrite).is_err());
        assert_eq!(
            database
                .prepare("SELECT body FROM render_attempts")?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
            before
        );
    }
    Ok(())
}

fn retention(
    store: &ProjectStore,
    attempt: &StoredRenderAttempt,
) -> Result<deadpan_store::render_media::PreparedRenderRetention> {
    use sha2::{Digest, Sha256};
    let movie = b"opaque movie byte fixture";
    let hash = deadpan_jobs::Sha256::new(
        Sha256::digest(movie)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )?;
    Ok(store.render_write_handle()?.prepare_retention(
        &attempt.identity(),
        &mut &movie[..],
        movie.len() as u64,
        &hash,
        b"{\"opaque_manifest\":true}",
        media_limits(),
        &AtomicBool::new(false),
        deadline(),
    )?)
}
fn media_limits() -> deadpan_store::render_media::RenderMediaLimits {
    deadpan_store::render_media::RenderMediaLimits::new(1024, 1024, 2048, 8192, 16).unwrap()
}
fn encoding(
    store: &mut ProjectStore,
    job: &RenderIntent,
    name: &str,
) -> Result<StoredRenderAttempt> {
    let queued = begin(store, job, name)?;
    Ok(store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Encoding)?)
}
fn observation(checkpoint: &StoredRenderCheckpoint) -> RenderVerificationObservation {
    RenderVerificationObservation {
        schema_version: 1,
        validator_id: "fixture-validator".into(),
        validator_version: "v1".into(),
        movie_sha256: checkpoint.media.movie_sha256().clone(),
        movie_byte_length: checkpoint.media.movie().byte_length(),
        report: serde_json::json!({"historical_observation":true}),
    }
}

#[test]
fn durable_checkpoint_survives_interruption_and_explicit_verification_retry() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let job = create(&mut store, "job")?;
    let encoding = encoding(&mut store, &job, "encoder")?;
    let token = retention(&store, &encoding)?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = authored(&database)?;
    let retained = store.retain_render_checkpoint(
        &encoding.identity(),
        &token,
        &AtomicBool::new(false),
        deadline(),
    )?;
    let checkpoint = store.render_checkpoint(&job.job_id, &encoding.attempt_id)?;
    assert_eq!(retained.state, RenderAttemptState::EncodedRetained);
    assert!(
        store
            .retain_render_checkpoint(
                &encoding.identity(),
                &token,
                &AtomicBool::new(false),
                deadline()
            )
            .is_err()
    );
    let verifying = store
        .transition_render_attempt(&retained.identity(), RenderAttemptTransition::Verifying)?;
    drop(store);
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(
        reader.render_attempt(&job.job_id, &encoding.attempt_id)?,
        verifying
    );
    drop(reader);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(
        store
            .render_attempt(&job.job_id, &encoding.attempt_id)?
            .state,
        RenderAttemptState::Interrupted
    );
    assert_eq!(
        store.render_checkpoint(&job.job_id, &encoding.attempt_id)?,
        checkpoint
    );
    let retry = store.begin_render_attempt(BeginRenderAttempt {
        job_id: job.job_id.clone(),
        attempt_id: AttemptId::new("verifier")?,
        cancellation_token: CancellationToken::new("cancel-verifier")?,
        checkpoint_attempt_id: Some(encoding.attempt_id.clone()),
    })?;
    assert_eq!(retry.ordinal, 2);
    assert!(
        store
            .transition_render_attempt(&retry.identity(), RenderAttemptTransition::Encoding)
            .is_err()
    );
    let verifying =
        store.transition_render_attempt(&retry.identity(), RenderAttemptTransition::Verifying)?;
    let mut wrong = observation(&checkpoint);
    wrong.movie_byte_length += 1;
    assert!(
        store
            .record_render_verification(&verifying.identity(), wrong)
            .is_err()
    );
    let verified =
        store.record_render_verification(&verifying.identity(), observation(&checkpoint))?;
    assert_eq!(verified.state, RenderAttemptState::Verified);
    assert_eq!(
        verified.checkpoint_attempt_id,
        Some(encoding.attempt_id.clone())
    );
    assert_eq!(authored(&database)?, before);
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(
        store.render_attempt(&job.job_id, &retry.attempt_id)?,
        verified
    );
    // Persisted success remains metadata. A separate read handle hashes bytes
    // again, and still creates no media-verifier/publication capability.
    let snapshot = store.render_read_handle().snapshot(
        &checkpoint.media,
        media_limits(),
        &AtomicBool::new(false),
        deadline(),
    )?;
    assert_eq!(snapshot.manifest_bytes(), b"{\"opaque_manifest\":true}");
    assert_eq!(authored(&database)?, before);
    Ok(())
}

#[test]
fn checkpoint_requires_live_session_exact_transition_and_unchanged_objects() -> Result {
    for mode in 0..3 {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("project.deadpan");
        let other_package = scratch.path().join("other.deadpan");
        let mut store = ProjectStore::create(&package, &document()?)?;
        let job = create(&mut store, "job")?;
        let encoding = encoding(&mut store, &job, "encoder")?;
        let token = if mode == 0 {
            let other = ProjectStore::create(&other_package, &document()?)?;
            retention(&other, &encoding)?
        } else {
            retention(&store, &encoding)?
        };
        match mode {
            0 => {}
            1 => {
                store.transition_render_attempt(
                    &encoding.identity(),
                    RenderAttemptTransition::RequestCancellation,
                )?;
            }
            _ => {
                let path = package.join("Media/RenderCandidates").join(format!(
                    "blake3-{}",
                    token.media().movie().content().digest()
                ));
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
                std::fs::write(&path, b"opaque movie byte changed")?;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444))?;
            }
        }
        assert!(
            store
                .retain_render_checkpoint(
                    &encoding.identity(),
                    &token,
                    &AtomicBool::new(false),
                    deadline()
                )
                .is_err()
        );
        assert!(
            store
                .render_checkpoint(&job.job_id, &encoding.attempt_id)
                .is_err()
        );
        let db = Connection::open(package.join("project.sqlite"))?;
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM render_candidate_checkpoints",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            0
        );
    }
    Ok(())
}

#[test]
fn malformed_checkpoint_or_oversized_report_cannot_become_verified() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let job = create(&mut store, "job")?;
    let encoding = encoding(&mut store, &job, "encoder")?;
    let token = retention(&store, &encoding)?;
    let retained = store.retain_render_checkpoint(
        &encoding.identity(),
        &token,
        &AtomicBool::new(false),
        deadline(),
    )?;
    let verifying = store
        .transition_render_attempt(&retained.identity(), RenderAttemptTransition::Verifying)?;
    let checkpoint = store.render_checkpoint(&job.job_id, &encoding.attempt_id)?;
    let mut report = observation(&checkpoint);
    report.report = serde_json::json!({"oversized":"x".repeat(MAX_RENDER_REPORT_BYTES)});
    assert!(
        store
            .record_render_verification(&verifying.identity(), report)
            .is_err()
    );
    assert_eq!(
        store.render_attempt(&job.job_id, &encoding.attempt_id)?,
        verifying
    );
    drop(store);
    let db = Connection::open(package.join("project.sqlite"))?;
    db.execute(
        "UPDATE render_candidate_checkpoints SET media=json_set(media,'$.movie_sha256',?1)",
        params!["not-sha256"],
    )?;
    assert!(ProjectStore::open(&package, AccessMode::ReadOnly).is_err());
    assert!(ProjectStore::open(&package, AccessMode::ReadWrite).is_err());
    Ok(())
}

#[test]
fn transition_counter_exhaustion_rolls_back_and_reserves_terminal_recovery() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let job = create(&mut store, "job")?;
    let encoding = encoding(&mut store, &job, "encoder")?;
    let db = Connection::open(package.join("project.sqlite"))?;
    let maximum = i64::MAX - 1;
    db.execute("UPDATE render_attempts SET transition_sequence=?1,body=json_set(body,'$.transition_sequence',?1)",[maximum])?;
    let bound = store.render_attempt(&job.job_id, &encoding.attempt_id)?;
    assert_eq!(bound.transition_sequence, MAX_RENDER_COUNTER - 1);
    assert!(
        store
            .transition_render_attempt(
                &bound.identity(),
                RenderAttemptTransition::RequestCancellation
            )
            .is_err()
    );
    assert_eq!(
        store.render_attempt(&job.job_id, &encoding.attempt_id)?,
        bound
    );
    let terminal = fail(&mut store, &bound)?;
    assert_eq!(terminal.transition_sequence, MAX_RENDER_COUNTER);
    assert!(
        store
            .transition_render_attempt(
                &terminal.identity(),
                RenderAttemptTransition::RequestCancellation
            )
            .is_err()
    );
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(
        store.render_attempt(&job.job_id, &encoding.attempt_id)?,
        terminal
    );
    Ok(())
}

#[test]
fn cancelling_failure_retains_cancellation_request_and_candidate() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let job = create(&mut store, "job")?;
    let encoding = encoding(&mut store, &job, "encoder")?;
    let token = retention(&store, &encoding)?;
    let retained = store.retain_render_checkpoint(
        &encoding.identity(),
        &token,
        &AtomicBool::new(false),
        deadline(),
    )?;
    let cancelling = store.transition_render_attempt(
        &retained.identity(),
        RenderAttemptTransition::RequestCancellation,
    )?;
    let failed = fail(&mut store, &cancelling)?;
    assert!(failed.cancellation_requested);
    assert_eq!(failed.state, RenderAttemptState::Failed);
    assert_eq!(
        failed.checkpoint_attempt_id,
        Some(encoding.attempt_id.clone())
    );
    store.render_checkpoint(&job.job_id, &encoding.attempt_id)?;
    store.validate()?;
    Ok(())
}

#[test]
fn targeted_operations_skip_unrelated_report_but_full_audit_rejects_it() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let job = create(&mut store, "job")?;
    let encoding = encoding(&mut store, &job, "historical")?;
    let token = retention(&store, &encoding)?;
    let retained = store.retain_render_checkpoint(
        &encoding.identity(),
        &token,
        &AtomicBool::new(false),
        deadline(),
    )?;
    let checkpoint = store.render_checkpoint(&job.job_id, &encoding.attempt_id)?;
    let verifying = store
        .transition_render_attempt(&retained.identity(), RenderAttemptTransition::Verifying)?;
    let mut report = observation(&checkpoint);
    report.report = serde_json::json!({"large_report":"x".repeat(200*1024)});
    let historical = store.record_render_verification(&verifying.identity(), report)?;
    let current = begin(&mut store, &job, "current")?;
    store.validate()?;

    // An invalid report type is a deterministic witness for deserialization:
    // parsing/validating this unrelated record must fail, regardless of speed.
    let db = Connection::open(package.join("project.sqlite"))?;
    db.execute("UPDATE render_attempts SET body=json_set(body,'$.verification.report',json('[\"invalid report object\"]')) WHERE attempt_id=?1",[historical.attempt_id.as_str()])?;
    assert!(
        store
            .render_attempt(&job.job_id, &historical.attempt_id)
            .is_err()
    );
    assert_eq!(store.render_job(&job.job_id)?, job);
    assert_eq!(store.render_jobs(None, 1)?, vec![job.clone()]);
    assert_eq!(
        store.render_attempt(&job.job_id, &current.attempt_id)?,
        current
    );
    assert_eq!(
        store.render_attempts(&job.job_id, historical.ordinal, 1)?,
        vec![current.clone()]
    );
    let running =
        store.transition_render_attempt(&current.identity(), RenderAttemptTransition::Encoding)?;
    fail(&mut store, &running)?;
    create(&mut store, "unrelated-job")?;
    assert!(store.validate().is_err());
    drop(store);
    assert!(ProjectStore::open(&package, AccessMode::ReadOnly).is_err());
    assert!(ProjectStore::open(&package, AccessMode::ReadWrite).is_err());
    Ok(())
}

#[test]
fn targeted_json_bounds_reject_before_deserializing_the_row() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let job = create(&mut store, "job")?;
    let current = begin(&mut store, &job, "current")?;
    let db = Connection::open(package.join("project.sqlite"))?;
    db.pragma_update(None, "ignore_check_constraints", true)?;
    // Deliberately invalid JSON as well as oversized: the stable metadata
    // error establishes that the bounded SQL projection rejected it first.
    db.execute(
        "UPDATE render_attempts SET body=?1 WHERE attempt_id=?2",
        params!["x".repeat(280 * 1024), current.attempt_id.as_str()],
    )?;
    let error = store
        .render_attempt(&job.job_id, &current.attempt_id)
        .unwrap_err();
    assert_eq!(error.code(), "RenderJobInvalid");
    assert!(error.to_string().contains("oversized or has wrong type"));
    assert_eq!(store.render_job(&job.job_id)?, job);
    assert!(store.validate().is_err());
    Ok(())
}

#[path = "render_jobs/admission.rs"]
mod admission;
#[path = "render_jobs/publication.rs"]
mod publication;
