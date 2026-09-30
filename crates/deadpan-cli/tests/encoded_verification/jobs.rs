//! Transport replay exercises durable ownership and real fresh verification.
//! These background documents do not qualify editorial content fidelity.
use super::*;
use deadpan_cli::encoded_render::jobs::{self, CaptureRenderIntent, RenderStageRequest};
use deadpan_encode::{BFramePolicy, EncoderMode};
use deadpan_jobs::render::{
    RenderAttemptState, RenderBFrames, RenderEncoder, RenderEngineeringPolicy, RenderSelection,
};
use deadpan_store::{
    render_jobs::{BeginRenderAttempt, RenderAttemptTransition, StoredRenderCheckpoint},
    render_media::{PreparedRenderRetention, RenderMediaLimits},
};

pub(super) fn limits() -> RenderMediaLimits {
    RenderMediaLimits::new(
        16 * 1024 * 1024,
        256 * 1024,
        17 * 1024 * 1024,
        64 * 1024 * 1024,
        128,
    )
    .unwrap()
}

pub(super) fn begin(fixture: &Fixture) -> (ProjectStore, RenderStageRequest) {
    let choice = fixture.manifest.contract.choice;
    let intent = jobs::capture_intent(
        CaptureRenderIntent {
            package: fixture.package.clone(),
            revision: fixture.document.revision_id().clone(),
            range: Some(fixture.manifest.contract.picture.range),
            job_id: RequestId::new("durable-render").unwrap(),
            policy: RenderEngineeringPolicy {
                schema_version: 1,
                selection: RenderSelection::ExplicitEngineering,
                encoder: match choice.mode {
                    EncoderMode::Hardware => RenderEncoder::Hardware,
                    EncoderMode::Software => RenderEncoder::Software,
                },
                b_frames: match choice.b_frames {
                    BFramePolicy::None => RenderBFrames::None,
                    BFramePolicy::TargetTwo => RenderBFrames::TargetTwo,
                },
            }
            .into(),
        },
        &NOT_CANCELLED,
        deadline(),
    )
    .unwrap();
    let mut store = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
    store
        .create_render_job(intent.clone(), &NOT_CANCELLED, deadline())
        .unwrap();
    let attempt = store
        .begin_render_attempt(BeginRenderAttempt {
            job_id: intent.job_id.clone(),
            attempt_id: AttemptId::new("encoding-1").unwrap(),
            cancellation_token: CancellationToken::new("encoding-token-1").unwrap(),
            checkpoint_attempt_id: None,
        })
        .unwrap();
    let attempt = store
        .transition_render_attempt(&attempt.identity(), RenderAttemptTransition::Encoding)
        .unwrap();
    (
        store,
        RenderStageRequest {
            package: fixture.package.clone(),
            intent,
            attempt,
        },
    )
}

pub(super) fn retain(
    fixture: &Fixture,
    store: &ProjectStore,
    request: &RenderStageRequest,
) -> PreparedRenderRetention {
    jobs::encode_and_retain(
        &python([
            "encode".into(),
            fixture.manifest_path.clone().into_os_string(),
            fixture.movie_path.clone().into_os_string(),
        ]),
        request,
        &store.render_write_handle().unwrap(),
        (EncodedWorkerLimits::default(), limits()),
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap()
}

pub(super) fn commit_retention(
    store: &mut ProjectStore,
    request: &mut RenderStageRequest,
    prepared: &PreparedRenderRetention,
) -> StoredRenderCheckpoint {
    request.attempt = store
        .retain_render_checkpoint(
            &request.attempt.identity(),
            prepared,
            &NOT_CANCELLED,
            deadline(),
        )
        .unwrap();
    store
        .render_checkpoint(&request.intent.job_id, &request.attempt.attempt_id)
        .unwrap()
}

pub(super) fn verifying(store: &mut ProjectStore, request: &mut RenderStageRequest) {
    request.attempt = store
        .transition_render_attempt(
            &request.attempt.identity(),
            RenderAttemptTransition::Verifying,
        )
        .unwrap();
}

#[test]
fn restart_preserves_checkpoint_and_fresh_attempt_reverifies_exact_bytes() {
    let fixture = Fixture::new("nonzero");
    let (mut store, mut stage) = begin(&fixture);
    let prepared = retain(&fixture, &store, &stage);
    let stale = stage.attempt.identity();
    let checkpoint = commit_retention(&mut store, &mut stage, &prepared);
    assert!(
        store
            .retain_render_checkpoint(&stale, &prepared, &NOT_CANCELLED, deadline())
            .is_err()
    );
    drop(store);
    let readonly = ProjectStore::open(&fixture.package, AccessMode::ReadOnly).unwrap();
    assert_eq!(
        readonly
            .render_attempt(&stage.intent.job_id, &stage.attempt.attempt_id)
            .unwrap()
            .state,
        RenderAttemptState::EncodedRetained
    );
    drop(readonly);
    let mut store = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        store
            .render_attempt(&stage.intent.job_id, &stage.attempt.attempt_id)
            .unwrap()
            .state,
        RenderAttemptState::Interrupted
    );
    assert_eq!(
        store
            .render_checkpoint(&stage.intent.job_id, &stage.attempt.attempt_id)
            .unwrap(),
        checkpoint
    );
    stage.attempt = store
        .begin_render_attempt(BeginRenderAttempt {
            job_id: stage.intent.job_id.clone(),
            attempt_id: AttemptId::new("verification-2").unwrap(),
            cancellation_token: CancellationToken::new("verification-token-2").unwrap(),
            checkpoint_attempt_id: Some(checkpoint.encoding_attempt_id.clone()),
        })
        .unwrap();
    verifying(&mut store, &mut stage);
    let mut verified = jobs::verify_checkpoint(
        &native(),
        &stage,
        (&checkpoint, &store.render_read_handle()),
        (VerificationLimits::default(), limits()),
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap();
    let mut bytes = Vec::new();
    verified
        .copy_to(&mut bytes, &NOT_CANCELLED, deadline())
        .unwrap();
    assert_eq!(bytes, fs::read(&fixture.movie_path).unwrap());
    let observation =
        jobs::verification_observation(&verified, &NOT_CANCELLED, deadline()).unwrap();
    let done = store
        .record_render_verification(&stage.attempt.identity(), observation.clone())
        .unwrap();
    assert_eq!(done.state, RenderAttemptState::Verified);
    assert!(
        store
            .record_render_verification(&stage.attempt.identity(), observation)
            .is_err()
    );
    drop(store);
    assert!(
        verified
            .copy_to(&mut Vec::new(), &NOT_CANCELLED, deadline())
            .is_err()
    );
    let reopened = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        reopened
            .render_attempt(&stage.intent.job_id, &done.attempt_id)
            .unwrap(),
        done
    );
    fixture.assert_project_unchanged();
}

#[test]
fn owner_revocation_during_native_verification_cannot_admit_a_candidate() {
    let fixture = Fixture::new("software-two");
    let (mut store, mut stage) = begin(&fixture);
    let prepared = retain(&fixture, &store, &stage);
    let checkpoint = commit_retention(&mut store, &mut stage, &prepared);
    verifying(&mut store, &mut stage);
    let reader = store.render_read_handle();
    let mut owner = Some(store);
    let result = jobs::verify_checkpoint(
        &native(),
        &stage,
        (&checkpoint, &reader),
        (VerificationLimits::default(), limits()),
        &NOT_CANCELLED,
        deadline(),
        |_| {
            drop(owner.take());
        },
    );
    assert!(
        owner.is_none(),
        "native verifier must reach a reported stage"
    );
    assert!(matches!(result, Err(EncodedRenderError::RetainedMedia(_))));
    let reopened = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        reopened
            .render_attempt(&stage.intent.job_id, &stage.attempt.attempt_id)
            .unwrap()
            .state,
        RenderAttemptState::Interrupted
    );
    fixture.assert_project_unchanged();
}

#[test]
fn closing_owner_cancels_and_reaps_a_running_encode_without_user_cancellation() {
    let fixture = Fixture::new("nonzero");
    let (store, stage) = begin(&fixture);
    let writer = store.render_write_handle().unwrap();
    let witness = fixture.scratch.path().join("encode-cancelled");
    let runtime = python(["owned-wait".into(), witness.clone().into_os_string()]);
    let mut owner = Some(store);
    let result = jobs::encode_and_retain(
        &runtime,
        &stage,
        &writer,
        (EncodedWorkerLimits::default(), limits()),
        &NOT_CANCELLED,
        deadline(),
        |_| {
            drop(owner.take());
        },
    );
    assert!(matches!(result, Err(EncodedRenderError::RetainedMedia(_))));
    assert!(owner.is_none());
    assert_eq!(
        fs::read_to_string(witness).unwrap(),
        "host cancelled revoked owner\n"
    );
    assert!(!NOT_CANCELLED.load(Ordering::Acquire));
    let reopened = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        reopened
            .render_attempt(&stage.intent.job_id, &stage.attempt.attempt_id)
            .unwrap()
            .state,
        RenderAttemptState::Interrupted
    );
    assert!(
        reopened
            .render_checkpoint(&stage.intent.job_id, &stage.attempt.attempt_id)
            .is_err()
    );
}

#[test]
fn closing_owner_cancels_and_reaps_a_running_verifier_without_user_cancellation() {
    let fixture = Fixture::new("nonzero");
    let (mut store, mut stage) = begin(&fixture);
    let prepared = retain(&fixture, &store, &stage);
    let checkpoint = commit_retention(&mut store, &mut stage, &prepared);
    verifying(&mut store, &mut stage);
    let reader = store.render_read_handle();
    let witness = fixture.scratch.path().join("verify-cancelled");
    let runtime = python(["owned-wait".into(), witness.clone().into_os_string()]);
    let mut owner = Some(store);
    let result = jobs::verify_checkpoint(
        &runtime,
        &stage,
        (&checkpoint, &reader),
        (VerificationLimits::default(), limits()),
        &NOT_CANCELLED,
        deadline(),
        |_| {
            drop(owner.take());
        },
    );
    assert!(matches!(result, Err(EncodedRenderError::RetainedMedia(_))));
    assert!(owner.is_none());
    assert_eq!(
        fs::read_to_string(witness).unwrap(),
        "host cancelled revoked owner\n"
    );
    assert!(!NOT_CANCELLED.load(Ordering::Acquire));
    let reopened = ProjectStore::open(&fixture.package, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        reopened
            .render_attempt(&stage.intent.job_id, &stage.attempt.attempt_id)
            .unwrap()
            .state,
        RenderAttemptState::Interrupted
    );
    assert_eq!(
        reopened
            .render_checkpoint(&stage.intent.job_id, &checkpoint.encoding_attempt_id)
            .unwrap(),
        checkpoint
    );
}

#[test]
fn malformed_or_retargeted_retained_manifests_fail_before_verification() {
    for mutation in ["json", "unknown", "version", "intent", "attempt", "encoder"] {
        let fixture = Fixture::new("nonzero");
        let (mut store, mut stage) = begin(&fixture);
        let original = retain(&fixture, &store, &stage);
        let snapshot = store
            .render_read_handle()
            .snapshot(original.media(), limits(), &NOT_CANCELLED, deadline())
            .unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(snapshot.manifest_bytes()).unwrap();
        match mutation {
            "json" => {}
            "unknown" => manifest["unexpected"] = true.into(),
            "version" => manifest["schema_version"] = 99.into(),
            "intent" => manifest["intent"]["document_sha256"] = "0".repeat(64).into(),
            "attempt" => manifest["encoding_attempt_id"] = "wrong-attempt".into(),
            "encoder" => {
                let previous = manifest["encoded"]["contract"]["choice"]["mode"]
                    .as_str()
                    .unwrap();
                manifest["encoded"]["contract"]["choice"]["mode"] = if previous == "hardware" {
                    "software"
                } else {
                    "hardware"
                }
                .into();
            }
            _ => unreachable!(),
        }
        let bytes = if mutation == "json" {
            b"{broken".to_vec()
        } else {
            serde_json::to_vec(&manifest).unwrap()
        };
        let prepared = store
            .render_write_handle()
            .unwrap()
            .prepare_retention(
                &stage.attempt.identity(),
                &mut File::open(&fixture.movie_path).unwrap(),
                original.media().movie().byte_length(),
                original.media().movie_sha256(),
                &bytes,
                limits(),
                &NOT_CANCELLED,
                deadline(),
            )
            .unwrap();
        let checkpoint = commit_retention(&mut store, &mut stage, &prepared);
        verifying(&mut store, &mut stage);
        let mut no_process = native();
        no_process.executable = fixture.scratch.path().join("must-not-launch");
        let result = jobs::verify_checkpoint(
            &no_process,
            &stage,
            (&checkpoint, &store.render_read_handle()),
            (VerificationLimits::default(), limits()),
            &NOT_CANCELLED,
            deadline(),
            |_| panic!("invalid manifest reached verifier"),
        );
        assert!(
            matches!(
                result,
                Err(EncodedRenderError::Json(_) | EncodedRenderError::Protocol(_))
            ),
            "{mutation}"
        );
        fixture.assert_project_unchanged();
    }
}

#[test]
fn corrupted_retained_movie_cannot_reuse_saved_manifest_or_verification() {
    use std::io::{Seek, SeekFrom, Write};
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new("nonzero");
    let (mut store, mut stage) = begin(&fixture);
    let prepared = retain(&fixture, &store, &stage);
    let checkpoint = commit_retention(&mut store, &mut stage, &prepared);
    verifying(&mut store, &mut stage);
    let path = fixture.package.join("Media/RenderCandidates").join(format!(
        "blake3-{}",
        checkpoint.media.movie().content().digest()
    ));
    let original_permissions = fs::metadata(&path).unwrap().permissions();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(17)).unwrap();
    file.write_all(b"invalid").unwrap();
    file.sync_all().unwrap();
    fs::set_permissions(&path, original_permissions).unwrap();
    let result = jobs::verify_checkpoint(
        &native(),
        &stage,
        (&checkpoint, &store.render_read_handle()),
        (VerificationLimits::default(), limits()),
        &NOT_CANCELLED,
        deadline(),
        |_| panic!("corrupt movie reached verifier"),
    );
    assert!(matches!(result, Err(EncodedRenderError::RetainedMedia(_))));
    fixture.assert_project_unchanged();
}
