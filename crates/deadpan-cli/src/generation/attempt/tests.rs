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
    conditioning::assemble(plan, constraints, b"left".to_vec(), b"right".to_vec()).unwrap()
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
