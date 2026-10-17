//! Opt-in inference-level scheduling qualification against the installed
//! Extension pack. This is intentionally ignored in routine test runs.

use super::*;
use crate::generation::conditioning::prepare_extension_scoped_with_provider;
use crate::generation::runtime::{
    BridgeRuntime, Lookup, plan_for_manifest, selected_provider_for_manifest,
};
use deadpan_core::NodeKind;
use deadpan_store::{AccessMode, ProjectStore};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

const INFERENCE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const WORKER_DRAIN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const PROGRESS_QUIET_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug)]
enum ScheduleSignal {
    Progress(AttemptProgress),
    PauseAcknowledged(bool),
}

/// Real MLX pause/resume/cancel qualification. Required environment:
///
/// - `DEADPAN_EXTENSION_REAL_PROJECT`: retained real Extension project fixture.
/// - `DEADPAN_EXTENSION_REAL_RUNTIME_SOURCE`: verified LTX checkout.
/// - `DEADPAN_EXTENSION_REAL_MODELS_ROOT`: installed model-pack root.
/// - `DEADPAN_EXTENSION_REAL_HELPERS`: directory containing both native workers.
/// - `DEADPAN_EXTENSION_REAL_REPORT`: output JSON path outside the repository.
///
/// Python, worker adapter and FFmpeg paths are derived from the runtime source
/// and repository, except FFmpeg which uses the existing Homebrew location.
#[test]
#[ignore = "requires explicit real-model paths and runs MLX inference on the GPU"]
fn real_extension_worker_pauses_resumes_and_cancels_while_paused() {
    let report_path = required_path("DEADPAN_EXTENSION_REAL_REPORT");
    let mut events = Vec::new();
    let result = exercise_real_extension_schedule(&mut events);
    let report = serde_json::json!({
        "schema": 1,
        "test": "real_extension_worker_pauses_resumes_and_cancels_while_paused",
        "events": events,
        "result": match &result {
            Ok(result) => serde_json::json!({"ok": true, "details": result}),
            Err(error) => serde_json::json!({"ok": false, "error": error}),
        },
    });
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent).expect("create qualification report directory");
    }
    std::fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap())
        .expect("write real Extension scheduling report");
    assert!(
        result.is_ok(),
        "real Extension scheduling failed; report: {}",
        report_path.display()
    );
}

fn exercise_real_extension_schedule(
    events: &mut Vec<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let source_project = required_path("DEADPAN_EXTENSION_REAL_PROJECT");
    let runtime_source = required_path("DEADPAN_EXTENSION_REAL_RUNTIME_SOURCE");
    let models_root = required_path("DEADPAN_EXTENSION_REAL_MODELS_ROOT");
    let helpers = required_path("DEADPAN_EXTENSION_REAL_HELPERS");
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not locate repository root from CARGO_MANIFEST_DIR")?
        .to_path_buf();
    let python = runtime_source.join(".venv/bin/python3");
    let worker = repository.join("tools/model-qualification/worker.py");
    let ffmpeg = PathBuf::from("/opt/homebrew/bin/ffmpeg");
    let ffprobe = PathBuf::from("/opt/homebrew/bin/ffprobe");
    for path in [
        &source_project,
        &runtime_source,
        &models_root,
        &helpers,
        &python,
        &worker,
        &ffmpeg,
        &ffprobe,
    ] {
        if !path.exists() {
            return Err(format!(
                "required real-runtime path does not exist: {}",
                path.display()
            ));
        }
    }

    let variables = |name: &str| -> Option<OsString> {
        let path = match name {
            crate::generation::runtime::RUNTIME_SOURCE => &runtime_source,
            crate::generation::runtime::PYTHON => &python,
            crate::generation::runtime::WORKER => &worker,
            crate::generation::runtime::FFMPEG => &ffmpeg,
            crate::generation::runtime::FFPROBE => &ffprobe,
            _ => return None,
        };
        Some(path.as_os_str().to_owned())
    };
    let mode = ConditioningMode::ExtendFromLeft;
    let runtime = BridgeRuntime::resolve_with_for(
        variables,
        None,
        Some(&helpers),
        &Lookup::Development,
        Some(&models_root),
        mode,
    )
    .map_err(|error| error.to_string())?;
    if runtime.model_manifest.pack_id != "ltx-2.3-q4-extension" {
        return Err(format!(
            "resolved unexpected model pack {}",
            runtime.model_manifest.pack_id
        ));
    }

    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let copy_path = scratch.path().join("real-extension-scheduling.deadpan");
    deadpan_store::portable::copy_portable(&source_project, &copy_path, &AtomicBool::new(false))
        .map_err(|error| format!("copy fixture to scratch: {error}"))?;
    let mut store =
        ProjectStore::open(&copy_path, AccessMode::ReadWrite).map_err(|error| error.to_string())?;
    let document = store.snapshot().map_err(|error| error.to_string())?;
    let target = ScopedNodeTarget {
        node: NodeId::new("extension").map_err(|error| error.to_string())?,
        repeats: Vec::new(),
    };
    let hold = document
        .nodes()
        .get(&target.node)
        .ok_or("fixture has no `extension` Hold")?;
    let duration = match &hold.kind {
        NodeKind::Hold { recipe } => recipe.duration,
        _ => return Err("fixture `extension` node is not a Hold".into()),
    };
    let basis = document.presentation_basis().clone();
    let revision = document.revision_id().clone();
    let plan = plan_for_manifest(&runtime.model_manifest, mode, duration, basis.frame_rate)
        .map_err(|error| format!("fixture plan does not fit installed Extension pack: {error}"))?;
    let selected = selected_provider_for_manifest(&runtime.model_manifest, &plan, 42107)
        .map_err(|error| error.to_string())?;
    let before = revision.clone();
    let inputs = prepare_extension_scoped_with_provider(
        &copy_path,
        &revision,
        &target,
        match &plan {
            GenerationPlan::Extension(plan) => plan,
            _ => return Err("Extension pack produced a non-Extension plan".into()),
        },
        match &selected {
            SelectedGenerationProvider::Extension(selected) => selected,
            _ => return Err("Extension manifest selected a non-Extension provider".into()),
        },
        &deadpan_jobs::GenerationOptions::default(),
        &AtomicBool::new(false),
    )
    .map_err(|error| format!("capture fixture context: {error}"))?;
    let allocated = crate::generation::attempt::allocate_scoped_with_provider(
        &mut store,
        AllocateInput {
            hold: target.node.clone(),
            expected_revision: revision,
            seed: 42107,
            inputs: inputs.into(),
        },
        target,
        selected.selection().clone(),
    )
    .map_err(|error| format!("allocate real Extension attempt: {error}"))?;
    let request = allocated.request.request_id.clone();
    let attempt = allocated.identity.attempt_id.clone();
    let request_for_worker = request.clone();
    let before_for_worker = before.clone();
    let selected_bundle_before = store
        .selected_generation_bundle(&request)
        .map_err(|error| error.to_string())?;
    let selected_bundle_before_for_worker = selected_bundle_before.clone();
    let source_hashes = source_hashes(&[
        &worker,
        &repository.join("tools/model-qualification/worker_protocol.py"),
        &repository.join("tools/model-qualification/mlx_backend.py"),
    ])?;
    let runtime_details = serde_json::json!({
        "provider": {
            "pack_id": runtime.model_manifest.pack_id,
            "pack_version": runtime.model_manifest.pack_version,
            "runtime_id": runtime.model_manifest.runtime_id,
            "runtime_version": runtime.model_manifest.runtime_versions.first().ok_or("runtime manifest has no version")?,
        },
        "model_manifest_sha256": runtime.model_manifest_sha256(),
        "worker_source_sha256": source_hashes,
        "python": runtime.python.display().to_string(),
        "runtime_source": runtime.runtime_source.display().to_string(),
        "model_cache": runtime.model_cache.display().to_string(),
        "worker_script": runtime.worker_script.display().to_string(),
        "helper_directory": helpers.display().to_string(),
    });
    let pause_requested = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (signals_tx, signals_rx) = mpsc::sync_channel(128);
    let (done_tx, done_rx) = mpsc::sync_channel::<Result<serde_json::Value, String>>(1);
    let worker_progress = signals_tx.clone();
    let worker_pause = Arc::clone(&pause_requested);
    let worker_cancel = Arc::clone(&cancelled);
    let worker_thread = std::thread::Builder::new()
        .name("real-extension-scheduling-qualification".into())
        .spawn(move || {
            let run = crate::generation::attempt::run_worker_scheduled(
                &allocated,
                &runtime,
                |progress| {
                    let _ = worker_progress.try_send(ScheduleSignal::Progress(progress));
                },
                |record| {
                    crate::generation::attempt::record(&mut store, &allocated, &record)
                        .map_err(|error| error.to_string())
                },
                &worker_cancel,
                || worker_pause.load(Ordering::Acquire),
                |paused| {
                    let _ = signals_tx.try_send(ScheduleSignal::PauseAcknowledged(paused));
                },
            );
            let worker_outcome = match &run.result {
                RunResult::Cancelled => "cancelled",
                RunResult::Failed(_) => "failed",
                RunResult::Qualified(_) => "qualified",
            };
            let worker_log = run.worker_log.clone();
            let worker_log_discarded_bytes = run.worker_log_discarded_bytes;
            let finished = crate::generation::attempt::finish(&mut store, &allocated, run)
                .map_err(|error| error.to_string());
            let state = store.generation_attempt(&allocated.identity)
                .map_err(|error| error.to_string())?;
            let selected_bundle = store.selected_generation_bundle(&request_for_worker)
                .map_err(|error| error.to_string())?;
            let head = store.head_revision().map_err(|error| error.to_string())?;
            store.validate_full().map_err(|error| error.to_string())?;
            let finished = finished?;
            done_tx.send(Ok(serde_json::json!({
                "worker_outcome": worker_outcome,
                "worker_log": worker_log,
                "worker_log_discarded_bytes": worker_log_discarded_bytes,
                "finished_state": format!("{:?}", finished.state),
                "failure": finished.failure.map(|failure| format!("{failure:?}")),
                "stored_state": state.as_ref().map(|attempt| format!("{:?}", attempt.checkpoint.state)),
                "bundle_receipt_present": state.as_ref().is_some_and(|attempt| attempt.bundle_receipt.is_some()),
                "selected_bundle_present": selected_bundle.is_some(),
                "selected_bundle_unchanged": selected_bundle == selected_bundle_before_for_worker,
                "head_revision": head.as_str(),
                "head_unchanged": head == before_for_worker,
            }))).ok();
            Ok::<(), String>(())
        })
        .map_err(|error| format!("spawn worker thread: {error}"))?;

    // All failures after launch use the same cleanup path: request resume,
    // request cancellation, and reap/join before returning the failed result.
    let exercise = drive_schedule(&signals_rx, &pause_requested, &cancelled, events);
    if exercise.is_err() {
        pause_requested.store(false, Ordering::Release);
        cancelled.store(true, Ordering::Release);
    }
    let done_deadline = Instant::now() + WORKER_DRAIN_TIMEOUT;
    let mut reports = Vec::new();
    let _completion_channel_closed = loop {
        match done_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(report) => reports.push(report),
            Err(RecvTimeoutError::Disconnected) => break true,
            Err(RecvTimeoutError::Timeout) if Instant::now() >= done_deadline => break false,
            Err(RecvTimeoutError::Timeout) => {}
        }
    };
    worker_thread
        .join()
        .map_err(|_| format!("worker thread panicked; schedule result: {exercise:?}"))?
        .map_err(|error| format!("worker thread failed; schedule result: {exercise:?}; {error}"))?;
    if !_completion_channel_closed {
        return Err(format!(
            "worker did not finish within {WORKER_DRAIN_TIMEOUT:?}; schedule result: {exercise:?}"
        ));
    }
    let worker_report = reports.pop().ok_or_else(|| {
        format!(
            "worker did not finish within {WORKER_DRAIN_TIMEOUT:?}; schedule result: {exercise:?}"
        )
    })??;
    events.push(serde_json::json!({
        "event":"worker_report",
        "worker_outcome":worker_report["worker_outcome"],
        "worker_log":worker_report["worker_log"],
        "worker_log_discarded_bytes":worker_report["worker_log_discarded_bytes"],
        "finished_state":worker_report["finished_state"],
        "stored_state":worker_report["stored_state"],
        "failure":worker_report["failure"],
    }));
    if let Err(error) = exercise {
        return Err(format!("{error}; worker report: {worker_report}"));
    }
    if worker_report["worker_outcome"] != "cancelled"
        || worker_report["finished_state"] != "Cancelled"
        || worker_report["stored_state"] != "Cancelled"
        || worker_report["bundle_receipt_present"] != false
        || worker_report["selected_bundle_unchanged"] != true
        || worker_report["head_unchanged"] != true
    {
        return Err(format!(
            "worker cancellation did not preserve the expected durable state: {worker_report}"
        ));
    }
    events.push(serde_json::json!({"event":"attempt_finished", "request":request.as_str(), "attempt":attempt.as_str(), "result":worker_report}));
    Ok(serde_json::json!({
        "request_id": request.as_str(),
        "attempt_id": attempt.as_str(),
        "project_revision": before.as_str(),
        "project_rate": format!("{}/{}", basis.frame_rate.numerator(), basis.frame_rate.denominator()),
        "project_frames": duration.frames(),
        "runtime": runtime_details,
        "worker": worker_report,
    }))
}

fn drive_schedule(
    signals: &Receiver<ScheduleSignal>,
    pause: &AtomicBool,
    cancelled: &AtomicBool,
    events: &mut Vec<serde_json::Value>,
) -> Result<(), String> {
    let inference_deadline = Instant::now() + INFERENCE_TIMEOUT;
    let mut inference_seen = false;
    while !inference_seen {
        match next_signal(signals, inference_deadline)? {
            ScheduleSignal::Progress(AttemptProgress::Stage(WorkerStage::Inference)) => {
                inference_seen = true;
                events.push(serde_json::json!({"event":"stage","stage":"inference"}));
            }
            signal => record_signal(events, signal),
        }
    }

    pause.store(true, Ordering::Release);
    events.push(serde_json::json!({"event":"pause_requested","paused":true}));
    wait_for_ack(signals, true, inference_deadline, events)?;
    let mut steps_at_pause = 0;
    let quiet_deadline = Instant::now() + PROGRESS_QUIET_WINDOW;
    loop {
        match receive_signal_until(signals, quiet_deadline) {
            Ok(signal) => match signal {
                ScheduleSignal::Progress(AttemptProgress::Step { .. }) => steps_at_pause += 1,
                signal => record_signal(events, signal),
            },
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => {
                return Err("worker event channel closed while paused".into());
            }
        }
    }
    if steps_at_pause != 0 {
        return Err(format!(
            "worker emitted {steps_at_pause} inference progress events after pause acknowledgement"
        ));
    }

    pause.store(false, Ordering::Release);
    events.push(serde_json::json!({"event":"pause_requested","paused":false}));
    wait_for_ack(signals, false, inference_deadline, events)?;
    let mut resumed_progress = false;
    let mut first_resumed_step = None;
    while !resumed_progress {
        match next_signal(signals, inference_deadline)? {
            ScheduleSignal::Progress(AttemptProgress::Step {
                stage: WorkerStage::Inference,
                completed,
                total,
            }) => {
                // The callback held at the pause can report its previous step
                // immediately after resume. Require a later completed step to
                // establish that actual denoising advanced after resuming.
                resumed_progress = first_resumed_step.is_some_and(|first| completed > first);
                first_resumed_step.get_or_insert(completed);
                events.push(serde_json::json!({"event":"inference_progress_after_resume","completed":completed,"total":total}));
            }
            signal => record_signal(events, signal),
        }
    }

    pause.store(true, Ordering::Release);
    events.push(serde_json::json!({"event":"pause_requested","paused":true,"purpose":"cancel_while_paused"}));
    wait_for_ack(signals, true, inference_deadline, events)?;
    cancelled.store(true, Ordering::Release);
    events.push(serde_json::json!({"event":"cancel_requested","while_paused":true}));
    let cancel_started = Instant::now();
    let cancel_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match signals.recv_timeout(Duration::from_millis(250)) {
            Ok(signal) => record_signal(events, signal),
            Err(RecvTimeoutError::Disconnected) => {
                let elapsed = cancel_started.elapsed();
                events.push(serde_json::json!({
                    "event":"worker_channel_closed_after_cancel",
                    "elapsed_ms":elapsed.as_millis(),
                }));
                return Ok(());
            }
            Err(RecvTimeoutError::Timeout) if Instant::now() >= cancel_deadline => {
                return Err(
                    "worker did not exit within 30 seconds after cancellation while paused".into(),
                );
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn wait_for_ack(
    signals: &Receiver<ScheduleSignal>,
    expected: bool,
    deadline: Instant,
    events: &mut Vec<serde_json::Value>,
) -> Result<(), String> {
    loop {
        match next_signal(signals, deadline)? {
            ScheduleSignal::PauseAcknowledged(paused) if paused == expected => {
                events.push(serde_json::json!({"event":"pause_acknowledged","paused":paused}));
                return Ok(());
            }
            ScheduleSignal::PauseAcknowledged(paused) => {
                events.push(serde_json::json!({"event":"unexpected_pause_ack","paused":paused}));
            }
            signal => record_signal(events, signal),
        }
    }
}

fn next_signal(
    signals: &Receiver<ScheduleSignal>,
    deadline: Instant,
) -> Result<ScheduleSignal, String> {
    if Instant::now() >= deadline {
        return Err("timed out waiting for inference scheduling event".into());
    }
    match signals.recv_timeout(
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(250)),
    ) {
        Ok(signal) => Ok(signal),
        Err(RecvTimeoutError::Disconnected) => {
            Err("worker event channel closed before expected schedule event".into())
        }
        Err(RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
            Err("timed out waiting for inference scheduling event".into())
        }
        Err(RecvTimeoutError::Timeout) => next_signal(signals, deadline),
    }
}

fn receive_signal_until(
    signals: &Receiver<ScheduleSignal>,
    deadline: Instant,
) -> Result<ScheduleSignal, RecvTimeoutError> {
    signals.recv_timeout(
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(250)),
    )
}

fn record_signal(events: &mut Vec<serde_json::Value>, signal: ScheduleSignal) {
    match signal {
        ScheduleSignal::Progress(AttemptProgress::Step {
            stage,
            completed,
            total,
        }) => {
            events.push(serde_json::json!({"event":"progress","stage":format!("{stage:?}"),"completed":completed,"total":total}));
        }
        ScheduleSignal::Progress(AttemptProgress::Stage(stage)) => {
            events.push(serde_json::json!({"event":"stage","stage":format!("{stage:?}")}));
        }
        ScheduleSignal::Progress(progress) => {
            events.push(serde_json::json!({"event":"progress","detail":format!("{progress:?}")}));
        }
        ScheduleSignal::PauseAcknowledged(paused) => {
            events.push(serde_json::json!({"event":"pause_acknowledged","paused":paused}));
        }
    }
}

fn required_path(name: &str) -> PathBuf {
    let value = std::env::var_os(name)
        .unwrap_or_else(|| panic!("set {name} to run ignored real-model qualification"));
    PathBuf::from(value)
}

fn source_hashes(paths: &[&Path]) -> Result<serde_json::Value, String> {
    let mut hashes = serde_json::Map::new();
    for path in paths {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("read adapter source {}: {error}", path.display()))?;
        hashes.insert(
            path.file_name()
                .and_then(|name| name.to_str())
                .ok_or("adapter filename is not UTF-8")?
                .into(),
            serde_json::Value::String(hex(&Sha256::digest(bytes))),
        );
    }
    Ok(serde_json::Value::Object(hashes))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}
