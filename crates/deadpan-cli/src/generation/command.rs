//! `generate-hold` and `accept-hold`: the headless AI pause commands.
//!
//! Both need the project closed in the app: the writer is held for the whole
//! attempt so its durable transitions stay ordered. Routing through the open
//! app's writer is not implemented.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{NodeId, RevisionId};
use deadpan_jobs::{JobFailure, JobState, RequestId};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

use super::attempt::{
    self, AllocateInput, AttemptProgress, Finished, GenerationError, RunResult, WorkerRun,
};
use super::runtime::BridgeRuntime;
use crate::CliError;
use crate::generation_context::BoundaryContextResolver;

fn writer(path: &Path) -> Result<ProjectStore, CliError> {
    let mut store = match ProjectStore::open(path, AccessMode::ReadWrite) {
        Ok(store) => store,
        Err(StoreError::AlreadyOpen) => {
            return Err(GenerationError::Invalid(
                "This project is open in Deadpan; close it there to generate or accept AI pauses from the command line."
                    .into(),
            )
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    store.set_generation_context_resolver(Arc::new(BoundaryContextResolver::default()));
    Ok(store)
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// `generate-hold <project> --hold <id> [--seed N]`
pub fn run_generate(arguments: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage("usage: generate-hold <project.deadpan> --hold <node-id> [--seed N]".into())
    };
    let [path, rest @ ..] = arguments else {
        return Err(usage());
    };
    let mut hold = None;
    let mut seed = 1_u64;
    let mut options = rest.iter();
    while let Some(option) = options.next() {
        let value = options.next().ok_or_else(usage)?;
        match *option {
            "--hold" => hold = Some(NodeId::new(*value)?),
            "--seed" => {
                seed = value
                    .parse()
                    .ok()
                    .filter(|seed| *seed < 1 << 32)
                    .ok_or_else(|| CliError::Usage("--seed must be below 2^32".into()))?;
            }
            _ => return Err(usage()),
        }
    }
    let hold = hold.ok_or_else(usage)?;
    let path = Path::new(path);
    let runtime = BridgeRuntime::from_environment().map_err(GenerationError::from)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        // A second signal exits at once, even during uncancellable work.
        signal_hook::flag::register_conditional_shutdown(signal, 130, Arc::clone(&cancelled))?;
        signal_hook::flag::register(signal, Arc::clone(&cancelled))?;
    }
    let mut store = writer(path)?;
    let revision = store.head_revision()?;
    let started = Instant::now();
    let inputs =
        super::conditioning::prepare(path, &revision, &hold, &cancelled).map_err(|error| {
            if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                GenerationError::Cancelled
            } else {
                GenerationError::Inputs(error)
            }
        })?;
    if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(GenerationError::Cancelled.into());
    }
    let conditioning = started.elapsed();
    let allocated = attempt::allocate(
        &mut store,
        AllocateInput {
            hold,
            expected_revision: revision,
            seed,
            inputs,
        },
    )?;
    let mut record_error = None;
    let run: WorkerRun = attempt::run_worker(
        &allocated,
        &runtime,
        |progress| {
            let line = match progress {
                AttemptProgress::Preparing => serde_json::json!({"progress": "preparing"}),
                AttemptProgress::Stage(stage) => serde_json::json!({"progress": stage}),
                AttemptProgress::Step {
                    stage,
                    completed,
                    total,
                } => serde_json::json!({"progress": stage, "completed": completed, "total": total}),
                AttemptProgress::Qualifying => serde_json::json!({"progress": "qualifying"}),
            };
            eprintln!("{line}");
        },
        |record| {
            attempt::record(&mut store, &allocated, &record).map_err(|error| {
                let text = error.to_string();
                record_error.get_or_insert(text.clone());
                text
            })
        },
        &cancelled,
    );
    let timings = run.timings;
    let worker_log_tail: String = {
        let lines: Vec<&str> = run.worker_log.lines().collect();
        lines[lines.len().saturating_sub(20)..].join("\n")
    };
    let failed_run = matches!(run.result, RunResult::Failed(_));
    let finished: Finished = attempt::finish(&mut store, &allocated, run)?;
    let receipt = finished.receipt.as_ref().map(|receipt| {
        serde_json::json!({
            "native": receipt.native_object(),
            "sampled": receipt.sampled_object(),
            "provenance": receipt.provenance_object(),
            "admission": receipt.admission().map(|evidence| serde_json::json!({
                "native_span": evidence.native_span(),
                "sampled_span": evidence.sampled_span(),
            })),
        })
    });
    let report = serde_json::json!({
        "protocol": 1,
        "request_id": allocated.identity.request_id,
        "attempt_id": allocated.identity.attempt_id,
        "hold_id": allocated.request.binding.hold_id,
        "request_version": allocated.request.binding.request_version,
        "context_sha256": allocated.request.binding.context_sha256,
        "plan": allocated.request.bridge_plan,
        "state": format!("{:?}", finished.state),
        "failure": finished.failure.as_ref().map(failure_text),
        "record_error": record_error,
        "timings_ms": {
            "conditioning": millis(conditioning),
            "workspace": millis(timings.preparation),
            "worker": millis(timings.worker),
            "qualification": millis(timings.qualification),
            "publication": millis(finished.publication),
        },
        "ready": receipt,
        "worker_log_tail": failed_run.then_some(worker_log_tail),
    });
    crate::write_json(&report)?;
    match finished.state {
        JobState::Ready => Ok(()),
        JobState::Cancelled => Err(GenerationError::Cancelled.into()),
        _ => Err(GenerationError::Failed(
            finished
                .failure
                .as_ref()
                .map_or_else(|| "the attempt did not finish".into(), failure_text),
        )
        .into()),
    }
}

fn failure_text(failure: &JobFailure) -> String {
    match failure {
        JobFailure::Host(host) => format!("{:?}: {}", host.code, host.detail.as_str()),
        JobFailure::Worker(worker) => format!("{:?}: {}", worker.code, worker.detail.as_str()),
    }
}

/// `accept-hold <project> --request <id>`
pub fn run_accept(arguments: &[&str]) -> Result<(), CliError> {
    let [path, "--request", request] = arguments else {
        return Err(CliError::Usage(
            "usage: accept-hold <project.deadpan> --request <request-id>".into(),
        ));
    };
    let request = RequestId::new(*request).map_err(|error| CliError::Usage(error.to_string()))?;
    let mut store = writer(Path::new(path))?;
    let new_revision = RevisionId::new(uuid::Uuid::new_v4().to_string())?;
    let outcome = super::acceptance::accept(&mut store, &request, new_revision)?;
    crate::write_json(&serde_json::json!({ "protocol": 1, "committed": true, "outcome": outcome }))
}
