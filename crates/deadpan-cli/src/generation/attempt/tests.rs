use std::collections::BTreeMap;
use std::path::PathBuf;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe,
    HoldVideo, PresentationBasis, ProjectDocument, ProjectId, Subtree,
};
use deadpan_jobs::{
    BridgeGenerationPlan, ConditioningMode, FailureCode, HoldConstraints, MotionAmount, Relevance,
    VideoSpec, WorkerFailure,
};

use super::*;
use crate::generation::conditioning;
use crate::generation::runtime::BridgeRuntime;

const FRAMES: i64 = 24;

fn hold_id() -> NodeId {
    NodeId::new("pause").unwrap()
}

/// A project whose root Sequence holds one 24-frame silent Hold at 24 fps.
fn project(directory: &Path) -> ProjectStore {
    let root = NodeId::new("root").unwrap();
    let initial = ProjectDocument::new(
        ProjectId::new("ai-hold-test").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: deadpan_core::FrameRate::new(24, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )
    .unwrap();
    let edit = deadpan_core::apply(
        &initial,
        &CommandRequest {
            project_id: initial.project_id().clone(),
            expected_revision: initial.revision_id().clone(),
            new_revision: RevisionId::new("with-pause").unwrap(),
            command: Command::Insert {
                parent: root,
                index: 0,
                subtree: Subtree {
                    root: hold_id(),
                    nodes: BTreeMap::from([(
                        hold_id(),
                        BeatNode::hold(
                            "Pause",
                            HoldRecipe {
                                duration: FrameDuration::new(FRAMES).unwrap(),
                                picture_context: None,
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
    )
    .unwrap();
    let document = edit.forward.apply(&initial).unwrap();
    ProjectStore::create(&directory.join("test.deadpan"), &document).unwrap()
}

fn inputs() -> BridgeInputs {
    let rate = deadpan_core::FrameRate::new(24, 1).unwrap();
    let duration = FrameDuration::new(FRAMES).unwrap();
    let plan = BridgeGenerationPlan::for_conditioning(
        ConditioningMode::Bridge,
        duration,
        rate,
        &crate::generation::development_capability(),
        crate::generation::native_dimensions(),
    )
    .unwrap();
    let constraints = HoldConstraints {
        video: VideoSpec::new(duration, rate, 768, 320).unwrap(),
        conditioning: ConditioningMode::Bridge,
        motion: MotionAmount::Still,
    };
    // Prepared pictures are opaque retained bytes to capture and the store.
    let (left, right) = conditioning::opaque_boundaries(&plan, b"left".to_vec(), b"right".to_vec());
    conditioning::assemble(plan, constraints, left, right).unwrap()
}

fn allocated(store: &mut ProjectStore) -> Allocated {
    let expected_revision = store.head_revision().unwrap();
    allocate(
        store,
        AllocateInput {
            hold: hold_id(),
            expected_revision,
            seed: 7,
            inputs: inputs(),
        },
    )
    .unwrap()
}

fn state(store: &ProjectStore, allocated: &Allocated) -> JobState {
    store
        .generation_attempt(&allocated.identity)
        .unwrap()
        .unwrap()
        .checkpoint
        .state
}

/// Every executable is a stand-in; `python` is what actually runs.
fn runtime(directory: &Path, python: &str) -> BridgeRuntime {
    let stand_in = PathBuf::from("/usr/bin/true");
    BridgeRuntime {
        python: PathBuf::from(python),
        runtime_source: directory.to_path_buf(),
        model_cache: directory.to_path_buf(),
        ffmpeg: stand_in.clone(),
        ffprobe: stand_in.clone(),
        worker_script: stand_in.clone(),
        media_worker: stand_in,
    }
}

#[test]
fn allocation_records_a_current_bridge_request_and_a_queued_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let first = allocated(&mut store);
    assert_eq!(first.request.relevance, Relevance::Current);
    assert_eq!(
        first.request.binding.context_sha256,
        inputs().manifest_sha256
    );
    assert_eq!(state(&store, &first), JobState::Queued);
    first.host_message.validate().unwrap();
    // A new request for the same Hold supersedes the first.
    let second = allocated(&mut store);
    assert_eq!(second.request.binding.request_version.get(), 2);
    let current = store.current_generation_requests().unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].request_id, second.identity.request_id);
}

#[test]
fn cancellation_before_launch_is_recorded_as_cancelled() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let allocated = allocated(&mut store);
    let mut records = Vec::new();
    let run = run_worker(
        &allocated,
        &runtime(directory.path(), "/usr/bin/false"),
        |_| {},
        |record| {
            records.push(record);
            Ok(())
        },
        &AtomicBool::new(true),
    );
    assert!(matches!(run.result, RunResult::Cancelled));
    assert_eq!(records, [AttemptRecord::CancelRequested]);
    for record in &records {
        super::record(&mut store, &allocated, record).unwrap();
    }
    assert_eq!(state(&store, &allocated), JobState::Cancelling);
    let finished = finish(&mut store, &allocated, run).unwrap();
    assert_eq!(finished.state, JobState::Cancelled);
    assert_eq!(state(&store, &allocated), JobState::Cancelled);
}

#[test]
fn a_cancelled_run_whose_cancel_record_was_lost_still_finishes_cancelled() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let allocated = allocated(&mut store);
    let run = run_worker(
        &allocated,
        &runtime(directory.path(), "/usr/bin/false"),
        |_| {},
        |_| Err("writer unavailable".into()),
        &AtomicBool::new(true),
    );
    assert_eq!(
        finish(&mut store, &allocated, run).unwrap().state,
        JobState::Cancelled
    );
}

#[test]
fn a_worker_that_exits_without_a_candidate_fails_truthfully() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let allocated = allocated(&mut store);
    let mut stages = Vec::new();
    let mut records = Vec::new();
    let run = run_worker(
        &allocated,
        &runtime(directory.path(), "/usr/bin/false"),
        |progress| stages.push(progress),
        |record| {
            records.push(record);
            Ok(())
        },
        &AtomicBool::new(false),
    );
    assert_eq!(stages, [AttemptProgress::Preparing]);
    assert!(records.is_empty(), "no worker messages: {records:?}");
    let RunResult::Failed(JobFailure::Host(failure)) = &run.result else {
        panic!("expected a host failure");
    };
    assert_eq!(failure.code, HostFailureCode::WorkerExited);
    let finished = finish(&mut store, &allocated, run).unwrap();
    assert_eq!(finished.state, JobState::Failed);
    assert_eq!(state(&store, &allocated), JobState::Failed);
    // The request stays current for a retry with a new attempt.
    assert_eq!(store.current_generation_requests().unwrap().len(), 1);
}

#[cfg(target_os = "macos")]
#[test]
fn ai_network_inference_host_preserves_files_protocol_and_owned_group() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let allocated = allocated(&mut store);
    let probe = crate::generation::runtime::network_tests::Probe::new();
    let mut stages = Vec::new();
    let run = run_worker(
        &allocated,
        &probe.runtime(),
        |progress| stages.push(progress),
        |entry| record(&mut store, &allocated, &entry).map_err(|error| error.to_string()),
        &AtomicBool::new(false),
    );
    assert!(
        matches!(&run.result, RunResult::Failed(JobFailure::Worker(failure))
        if failure.code == FailureCode::BackendFailure
            && failure.detail.as_str() == "network isolation probe finished"),
        "worker did not exchange its valid protocol: {}",
        run.worker_log
    );
    assert!(stages.contains(&AttemptProgress::Stage(WorkerStage::Preflight)));
    probe.assert_denied();
    assert_eq!(
        finish(&mut store, &allocated, run).unwrap().state,
        JobState::Failed
    );
}

#[test]
fn a_worker_failure_the_store_missed_is_recorded_as_a_host_failure() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let allocated = allocated(&mut store);
    let run = WorkerRun::early(
        RunResult::Failed(JobFailure::Worker(WorkerFailure {
            code: FailureCode::BackendFailure,
            detail: diagnostic("out of memory"),
        })),
        RunTimings::default(),
    );
    let finished = finish(&mut store, &allocated, run).unwrap();
    assert_eq!(finished.state, JobState::Failed);
    let Some(JobFailure::Host(failure)) = finished.failure else {
        panic!("expected the recorded host failure");
    };
    assert!(failure.detail.as_str().contains("out of memory"));
}

#[test]
fn reaping_finishes_cancel_and_keeps_the_first_failure() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let allocated = allocated(&mut store);
    let lifecycle = || {
        JobLifecycle::new_with_protocol(
            allocated.identity.clone(),
            allocated.cancellation_token.clone(),
            allocated.request.binding.clone(),
            ProtocolVersion::V2,
        )
    };
    for clean in [true, false] {
        let mut job = lifecycle();
        job.request_cancel(&allocated.identity, &allocated.cancellation_token)
            .unwrap();
        reaped(&mut job, clean);
        assert!(matches!(concluded(&job), RunResult::Cancelled));
    }
    let mut job = lifecycle();
    fail(
        &mut job,
        HostFailureCode::ProtocolViolation,
        "malformed payload",
    );
    fail(&mut job, HostFailureCode::WorkerExited, "later exit");
    reaped(&mut job, false);
    let RunResult::Failed(JobFailure::Host(failure)) = concluded(&job) else {
        panic!("expected failure");
    };
    assert_eq!(failure.code, HostFailureCode::ProtocolViolation);
    // A clean exit without a candidate is not success.
    let mut job = lifecycle();
    reaped(&mut job, true);
    assert!(matches!(concluded(&job), RunResult::Failed(_)));
}

/// The synthetic worker's two external tools, when this machine has them.
fn synthetic_tools() -> Option<synthetic::SyntheticWorker> {
    let ffmpeg = std::env::var_os("DEADPAN_BRIDGE_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    let media_worker = std::env::var_os("DEADPAN_MEDIA_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/deadpan-media-worker")
        });
    (ffmpeg.is_file() && media_worker.is_file()).then_some(synthetic::SyntheticWorker {
        ffmpeg,
        media_worker,
    })
}

/// Real PNG conditioning pictures at the native raster.
fn picture_inputs() -> BridgeInputs {
    let png = |rgb: [u8; 3]| {
        let image = image::RgbImage::from_pixel(768, 320, image::Rgb(rgb));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    };
    let template = inputs();
    let (left, right) =
        conditioning::opaque_boundaries(&template.plan, png([200, 40, 40]), png([40, 40, 200]));
    conditioning::assemble(template.plan, template.constraints, left, right).unwrap()
}

fn run_synthetic(
    store: &mut ProjectStore,
    allocated: &Allocated,
    worker: &synthetic::SyntheticWorker,
) -> Finished {
    let run = synthetic::run(
        allocated,
        worker,
        |_| {},
        |record| super::record(store, allocated, &record).map_err(|error| error.to_string()),
        &AtomicBool::new(false),
    );
    finish(store, allocated, run).unwrap()
}

/// Every attempt of one request is a seeded variant: each reaches Ready with
/// its own seed and pictures, the newest is selected, earlier ones stay
/// available, and a new request for the Hold makes them all stale.
#[test]
fn synthetic_variants_publish_distinct_ready_bundles_for_one_request() {
    let Some(worker) = synthetic_tools() else {
        eprintln!("skipped: needs ffmpeg with libx264rgb and a built deadpan-media-worker");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let expected_revision = store.head_revision().unwrap();
    let first = allocate(
        &mut store,
        AllocateInput {
            hold: hold_id(),
            expected_revision,
            seed: 7,
            inputs: picture_inputs(),
        },
    )
    .unwrap();
    assert_eq!((first.ordinal(), first.provider().seed), (1, 7));
    let finished = run_synthetic(&mut store, &first, &worker);
    assert_eq!(finished.state, JobState::Ready, "{:?}", finished.failure);
    let first_receipt = finished.receipt.unwrap();

    let second =
        allocate_variant(&mut store, first.request.clone(), first.inputs().clone()).unwrap();
    assert_eq!(second.request.request_id, first.request.request_id);
    assert_eq!((second.ordinal(), second.provider().seed), (2, 8));
    let finished = run_synthetic(&mut store, &second, &worker);
    assert_eq!(finished.state, JobState::Ready, "{:?}", finished.failure);
    let second_receipt = finished.receipt.unwrap();
    assert_eq!(second_receipt.provider().seed, 8);
    assert_ne!(
        first_receipt.sampled_object(),
        second_receipt.sampled_object(),
        "each seed yields its own pictures"
    );
    let selected = store
        .selected_generation_bundle(&first.request.request_id)
        .unwrap()
        .unwrap();
    assert_eq!(selected.identity, second.identity);
    store
        .select_generation_bundle_variant(&first.identity)
        .unwrap();

    // Inputs from another context cannot extend this request.
    let mut changed = picture_inputs();
    changed.manifest_sha256 = deadpan_jobs::Sha256::new("f".repeat(64)).unwrap();
    assert!(allocate_variant(&mut store, first.request.clone(), changed).is_err());

    let expected_revision = store.head_revision().unwrap();
    let replacement = allocate(
        &mut store,
        AllocateInput {
            hold: hold_id(),
            expected_revision,
            seed: 30,
            inputs: picture_inputs(),
        },
    )
    .unwrap();
    assert_eq!(state(&store, &replacement), JobState::Queued);
    assert!(
        store
            .selected_generation_bundle(&first.request.request_id)
            .unwrap()
            .is_none()
    );
    assert!(allocate_variant(&mut store, first.request.clone(), first.inputs().clone()).is_err());
}

// Hostile AI pause workers through the real attempt host
// (docs/ADVERSARIAL.md#hostile-workers). The trusted `python` and
// `worker_script` seam runs a hostile fixture instead of the bridge worker.

#[path = "../../../tests/hostile_workers/support.rs"]
mod hostile;

/// What one hostile attempt concluded, captured before `finish` consumes it.
struct HostileOutcome {
    record: hostile::Record,
    failure: Option<HostFailure>,
    cancelled: bool,
    qualified: bool,
    log_bytes: usize,
    discarded_bytes: u64,
    state: JobState,
    elapsed: Duration,
}

/// Run one hostile attempt through `run_worker`, record it as the app does,
/// and finish it durably.
fn hostile_attempt(name: &str, cancel_after: Option<Duration>) -> HostileOutcome {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project(directory.path());
    let allocated = allocated(&mut store);
    let record = hostile::Record::new();
    let runtime = BridgeRuntime {
        python: hostile::python(),
        worker_script: record.wrapper(&hostile::fixture("analysis.py"), name),
        ..runtime(directory.path(), "/usr/bin/false")
    };
    let cancelled = AtomicBool::new(false);
    let mut records = Vec::new();
    let started = Instant::now();
    let run = thread::scope(|scope| {
        if let Some(delay) = cancel_after {
            let cancelled = &cancelled;
            scope.spawn(move || {
                thread::sleep(delay);
                cancelled.store(true, Ordering::Release);
            });
        }
        run_worker(
            &allocated,
            &runtime,
            |_| {},
            |record| {
                records.push(record);
                Ok(())
            },
            &cancelled,
        )
    });
    let elapsed = started.elapsed();
    for entry in &records {
        super::record(&mut store, &allocated, entry).unwrap();
    }
    let failure = match &run.result {
        RunResult::Failed(JobFailure::Host(failure)) => Some(failure.clone()),
        _ => None,
    };
    let cancelled = matches!(run.result, RunResult::Cancelled);
    let qualified = matches!(run.result, RunResult::Qualified(_));
    let (log_bytes, discarded_bytes) = (run.worker_log.len(), run.worker_log_discarded_bytes);
    let finished = finish(&mut store, &allocated, run).unwrap();
    assert_eq!(state(&store, &allocated), finished.state);
    HostileOutcome {
        record,
        failure,
        cancelled,
        qualified,
        log_bytes,
        discarded_bytes,
        state: finished.state,
        elapsed,
    }
}

#[test]
fn hostile_ai_provenance_claims_fail_before_media_qualification() {
    for name in [
        "symlink_outside",
        "hardlink_outside",
        "symlinked_scope",
        "fifo",
        "sparse",
        "directory",
    ] {
        let outcome = hostile_attempt(name, None);
        assert!(outcome.elapsed < Duration::from_secs(10), "{name}");
        let failure = outcome.failure.expect("artifact claim must fail");
        assert_eq!(
            failure.code,
            HostFailureCode::OutputValidationFailed,
            "{name}: {failure:?}"
        );
        assert!(
            failure.detail.as_str().contains("artifact"),
            "{name}: {failure:?}"
        );
        assert_eq!(outcome.state, JobState::Failed);
        assert!(!outcome.qualified);
        assert_eq!(
            std::fs::read(outcome.record.path().join("outside.bin")).unwrap(),
            b"{}"
        );
        outcome.record.assert_group_gone();
    }
    for name in ["absolute", "parent"] {
        let outcome = hostile_attempt(name, None);
        let failure = outcome.failure.expect("unsafe reference must fail");
        assert!(
            matches!(
                failure.code,
                HostFailureCode::WorkerExited | HostFailureCode::ProtocolViolation
            ),
            "{name}: {failure:?}"
        );
        assert!(
            !failure.detail.as_str().contains("exited"),
            "{name}: {failure:?}"
        );
        assert_eq!(outcome.state, JobState::Failed);
        assert!(!outcome.qualified);
        outcome.record.assert_group_gone();
    }
}

#[test]
fn hostile_ai_workers_fail_truthfully_and_leave_no_group_member() {
    for name in [
        "malformed",
        "invalid_utf8",
        "zero_length",
        "truncated",
        "oversized",
        "just_over",
        "wrong_attempt",
        "fork_spam_exit",
        "stderr_flood",
    ] {
        let outcome = hostile_attempt(name, None);
        assert!(
            outcome.elapsed < Duration::from_secs(20),
            "{name}: {:?}",
            outcome.elapsed
        );
        assert!(!outcome.qualified, "{name}");
        let failure = outcome
            .failure
            .unwrap_or_else(|| panic!("{name}: expected a host failure"));
        assert!(
            matches!(
                failure.code,
                HostFailureCode::WorkerExited | HostFailureCode::ProtocolViolation
            ),
            "{name}: {failure:?}"
        );
        hostile::assert_generic_cause(name, failure.detail.as_str());
        if name == "wrong_attempt" {
            assert!(!failure.detail.as_str().contains("exited"), "{failure:?}");
        }
        assert_eq!(outcome.state, JobState::Failed, "{name}");
        outcome.record.assert_group_gone();
        if name == "stderr_flood" {
            // The retained diagnostic is the bounded tail; the rest is counted.
            assert_eq!(outcome.log_bytes, 64 * 1024);
            assert_eq!(outcome.discarded_bytes, 16 * 1024 * 1024 - 64 * 1024);
        }
    }
}

#[test]
fn stalled_and_forking_ai_workers_are_stopped_by_cancellation() {
    // The attempt deadline is thirty minutes; cancellation is how a person
    // stops a dribbling or forking worker that ignores the cooperative cancel.
    for name in ["slow_loris", "fork_spam"] {
        let outcome = hostile_attempt(name, Some(Duration::from_millis(300)));
        assert!(
            outcome.elapsed < Duration::from_secs(20),
            "{name}: {:?}",
            outcome.elapsed
        );
        assert!(outcome.cancelled, "{name}: {:?}", outcome.failure);
        assert_eq!(outcome.state, JobState::Cancelled, "{name}");
        outcome.record.assert_group_gone();
    }
}

#[test]
fn an_escaped_ai_worker_descendant_fails_the_attempt_and_is_not_contained() {
    let outcome = hostile_attempt("escape", None);
    assert!(
        outcome.elapsed < Duration::from_secs(25),
        "{:?}",
        outcome.elapsed
    );
    let failure = outcome.failure.expect("expected a host failure");
    assert!(
        failure.detail.as_str().contains("pipes stayed open"),
        "{failure:?}"
    );
    assert_eq!(outcome.state, JobState::Failed);
    outcome.record.assert_group_gone();
    hostile::assert_alive(outcome.record.escaped(Duration::from_secs(5)));
}
