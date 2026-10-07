//! `generate-hold` and `accept-hold`: the headless AI pause commands.
//!
//! With the project closed, the command holds the writer for the whole job so
//! its durable transitions stay ordered. When the app already owns the
//! writer, both route through its authenticated live endpoint
//! (`docs/LIVE_PROJECT.md`): generation runs as the app's own AI job, which
//! the CLI observes and can cancel; acceptance is one owner-side transaction.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::RevisionId;
use deadpan_jobs::{GenerationOptions, JobFailure, JobState, RequestId};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

use super::attempt::{
    self, AllocateInput, AttemptProgress, Finished, GenerationError, RunResult, WorkerRun,
};
use super::runtime::BridgeRuntime;
use crate::CliError;
use crate::generation_context::BoundaryContextResolver;

mod options;

/// The project's writer, or None when the app already owns it.
fn writer(path: &Path) -> Result<Option<ProjectStore>, CliError> {
    let mut store = match ProjectStore::open(path, AccessMode::ReadWrite) {
        Ok(store) => store,
        Err(StoreError::AlreadyOpen) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    store.set_generation_context_resolver(Arc::new(BoundaryContextResolver::default()));
    Ok(Some(store))
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// The most variants one command generates, one attempt after another.
pub const MAX_VARIANTS: u32 = 4;

/// `generate-hold <project> --hold <id> [--seed N] [--variants N] [--another]`
///
/// Each variant is one attempt of the Hold's bridge request with its own seed
/// (`ProviderSelection::for_attempt`). Without `--another` the first variant
/// records a new request, which makes earlier ones stale; with it, variants
/// are added to the Hold's current request, conditioned from its original
/// revision. Stops at the first attempt that does not reach Ready.
pub fn run_generate(arguments: &[&str]) -> Result<(), CliError> {
    let options::Arguments {
        path,
        hold,
        seed: chosen_seed,
        variants,
        another,
        options,
    } = options::parse(arguments)?;
    let seed_given = chosen_seed.is_some();
    let mut seed = chosen_seed.unwrap_or(0);
    if !seed_given {
        // As in the app: a fresh random seed below 2^32 for a new request.
        let random = uuid::Uuid::new_v4();
        let mut bytes = [0; 4];
        bytes.copy_from_slice(&random.as_bytes()[..4]);
        seed = u64::from(u32::from_le_bytes(bytes));
    }
    let path = Path::new(path);
    let cancelled = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        // A second signal exits at once, even during uncancellable work.
        signal_hook::flag::register_conditional_shutdown(signal, 130, Arc::clone(&cancelled))?;
        signal_hook::flag::register(signal, Arc::clone(&cancelled))?;
    }
    let Some(mut store) = writer(path)? else {
        // The app's own job decides between a new request and another
        // variant of the current one, exactly as :generate does.
        let variants = u8::try_from(variants).expect("validated variant count");
        return live::run(
            path,
            hold,
            variants,
            seed_given.then_some(seed),
            options,
            &cancelled,
        );
    };
    let runtime = BridgeRuntime::from_environment().map_err(GenerationError::from)?;
    let existing = if another {
        let request = attempt::current_bridge_request(&store, &hold)?.ok_or_else(|| {
                GenerationError::Invalid(
                    "This pause has no current AI pictures request to add a variant to; generate without --another."
                        .into(),
                )
            })?;
        if !crate::generation::same_provider_identity(
            &request.provider,
            &runtime.provider(request.provider.seed),
        ) {
            return Err(GenerationError::Invalid(format!(
                "This pause's current request uses {} {}, but the selected AI pack is {} {}. Generate without --another to start a request with the selected pack.",
                request.provider.pack_id,
                request.provider.pack_version,
                runtime.provider(0).pack_id,
                runtime.provider(0).pack_version,
            ))
            .into());
        }
        Some(request)
    } else {
        None
    };
    let revision = match &existing {
        Some(request) => request.origin_revision.clone(),
        None => store.head_revision()?,
    };
    let started = Instant::now();
    let mut inputs =
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
    let options = existing
        .as_ref()
        .map(|request| GenerationOptions::from_constraints(&request.constraints))
        .or(options)
        .unwrap_or_default();
    options.apply_to(&mut inputs.constraints);
    let conditioning = started.elapsed();
    let mut allocated = match existing {
        Some(request) => attempt::allocate_variant(&mut store, request, inputs)?,
        None => attempt::allocate_with_provider(
            &mut store,
            AllocateInput {
                hold,
                expected_revision: revision,
                seed,
                inputs,
            },
            runtime.provider(seed),
        )?,
    };
    let mut reports = Vec::new();
    let mut last = None;
    for variant in 1..=variants {
        let (report, finished) = run_one(&mut store, path, &allocated, &runtime, &cancelled)?;
        reports.push(report);
        let ready = finished.state == JobState::Ready;
        last = Some(finished);
        if !ready || variant == variants || cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        allocated = attempt::allocate_variant(
            &mut store,
            allocated.request.clone(),
            allocated.inputs().clone(),
        )?;
    }
    let finished = last.expect("at least one variant ran");
    let mut report = reports.last().cloned().unwrap_or_default();
    if let serde_json::Value::Object(fields) = &mut report {
        fields.insert("protocol".into(), serde_json::json!(1));
        fields.insert(
            "options".into(),
            serde_json::to_value(&options).expect("generation controls serialize"),
        );
        fields.insert(
            "conditioning_ms".into(),
            serde_json::json!(millis(conditioning)),
        );
        // Per request: every variant shares these conditioning inputs.
        if let Some(colour) =
            super::conditioning::ConditioningColour::from_manifest(&allocated.inputs().manifest)
        {
            fields.insert("colour".into(), colour.to_json());
        }
        fields.insert("variants".into(), serde_json::Value::Array(reports));
    }
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

/// Run one allocated attempt to its durable outcome and describe it.
fn run_one(
    store: &mut ProjectStore,
    package: &Path,
    allocated: &attempt::Allocated,
    runtime: &BridgeRuntime,
    cancelled: &AtomicBool,
) -> Result<(serde_json::Value, Finished), CliError> {
    let mut record_error = None;
    let run: WorkerRun = attempt::run_worker(
        allocated,
        runtime,
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
            attempt::record(store, allocated, &record).map_err(|error| {
                let text = error.to_string();
                record_error.get_or_insert(text.clone());
                text
            })
        },
        cancelled,
    );
    let timings = run.timings;
    let worker_log_tail: String = {
        let lines: Vec<&str> = run.worker_log.lines().collect();
        lines[lines.len().saturating_sub(20)..].join("\n")
    };
    let failed_run = matches!(run.result, RunResult::Failed(_));
    let finished: Finished = attempt::finish(store, allocated, run)?;
    let receipt = finished.receipt.as_ref().map(|receipt| {
        let joins = join_report(store, package, allocated, receipt, cancelled);
        serde_json::json!({
            "joins": joins,
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
        "request_id": allocated.identity.request_id,
        "attempt_id": allocated.identity.attempt_id,
        "attempt_ordinal": allocated.ordinal(),
        "seed": allocated.provider().seed,
        "hold_id": allocated.request.binding.hold_id,
        "request_version": allocated.request.binding.request_version,
        "context_sha256": allocated.request.binding.context_sha256,
        "plan": allocated.request.bridge_plan,
        "state": format!("{:?}", finished.state),
        "failure": finished.failure.as_ref().map(failure_text),
        "record_error": record_error,
        "timings_ms": {
            "workspace": millis(timings.preparation),
            "worker": millis(timings.worker),
            "qualification": millis(timings.qualification),
            "publication": millis(finished.publication),
        },
        "ready": receipt,
        "worker_log_tail": failed_run.then_some(worker_log_tail),
    });
    Ok((report, finished))
}

/// The advisory join measurement of a Ready variant (spec §12.5), or why it
/// could not be measured. It never changes the attempt's outcome.
fn join_report(
    store: &ProjectStore,
    package: &Path,
    allocated: &attempt::Allocated,
    receipt: &deadpan_store::generation_attempts::BundleValidationReceipt,
    cancelled: &AtomicBool,
) -> serde_json::Value {
    match super::joins::measure_request_joins(
        package,
        &store.generated_read_handle(),
        &allocated.request.origin_revision,
        &allocated.request.binding.hold_id,
        receipt,
        cancelled,
    ) {
        Ok(report) => serde_json::json!({
            "entry": report.entry,
            "exit": report.exit,
            "region": report.region,
            "advisory": true,
        }),
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    }
}

fn failure_text(failure: &JobFailure) -> String {
    match failure {
        JobFailure::Host(host) => format!("{:?}: {}", host.code, host.detail.as_str()),
        JobFailure::Worker(worker) => format!("{:?}: {}", worker.code, worker.detail.as_str()),
    }
}

/// `accept-hold <project> --request <id> [--attempt <id>]`
///
/// With `--attempt`, that Ready variant of the request is selected first;
/// otherwise the request's selected variant is accepted.
pub fn run_accept(arguments: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "usage: accept-hold <project.deadpan> --request <request-id> [--attempt <attempt-id>]"
                .into(),
        )
    };
    let (path, request, attempt) = match arguments {
        [path, "--request", request] => (path, request, None),
        [path, "--request", request, "--attempt", attempt] => (path, request, Some(attempt)),
        _ => return Err(usage()),
    };
    let request = RequestId::new(*request).map_err(|error| CliError::Usage(error.to_string()))?;
    let attempt = attempt
        .map(|attempt| deadpan_jobs::AttemptId::new(*attempt))
        .transpose()
        .map_err(|error| CliError::Usage(error.to_string()))?;
    let new_revision = RevisionId::new(uuid::Uuid::new_v4().to_string())?;
    // Accept against the head observed now: an edit made meanwhile (in the
    // app or another command) refuses instead of being accepted over.
    let expected_revision =
        ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?.head_revision()?;
    // The closed project's writer, or the open app's owner through its
    // authenticated endpoint; never a replay after an uncertain delivery.
    let output = crate::live_project::dispatch_short(
        Path::new(path),
        None,
        crate::live_project::ShortOperation::AcceptHold {
            request,
            attempt,
            expected_revision: Some(expected_revision),
            new_revision,
        },
    )?;
    crate::write_json(&output)
}

/// `generate-hold` against a project the app has open.
mod live {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use deadpan_core::NodeId;

    use crate::CliError;
    use crate::host::Client;
    use crate::live_project::generation::{GenerateRequest, GenerationOutcome, GenerationStatus};
    use crate::live_project::{self, LiveError, Operation, Reply};

    use super::{GenerationError, attempt};

    const POLL_INTERVAL: Duration = Duration::from_millis(250);
    const CANCELLATION_DRAIN_LIMIT: Duration = Duration::from_secs(5 * 60);

    fn status(reply: Reply, job: Option<u64>) -> Result<GenerationStatus, LiveError> {
        match reply {
            Reply::Generation { status } if job.is_none_or(|job| status.job == job) => Ok(*status),
            _ => Err(LiveError::new(
                "HostProtocolInvalid",
                "Host returned an unexpected generation reply",
            )),
        }
    }

    /// A lost observation never becomes a retry or an invented outcome.
    fn lost(job: u64, error: LiveError) -> LiveError {
        LiveError::new(
            "HostOutcomeUnknown",
            format!(
                "Lost contact with the app's AI job {job} ({error}); it may still be running. Cancel it in the app with :cancel-ai or inspect the project before generating again"
            ),
        )
    }

    fn progress(status: &GenerationStatus) -> serde_json::Value {
        serde_json::json!({
            "progress": status.stage,
            "variant": status.variant,
            "variants": status.variants,
            "ready": status.ready,
            "steps": status.steps,
            "elapsed_ms": status.elapsed_ms,
        })
    }

    pub(super) fn run(
        package: &Path,
        hold: NodeId,
        variants: u8,
        seed: Option<u64>,
        options: Option<deadpan_jobs::GenerationOptions>,
        cancelled: &AtomicBool,
    ) -> Result<(), CliError> {
        let mut client = Client::discover(package)
            .map_err(LiveError::from)?
            .ok_or_else(|| {
                LiveError::new(
                    "HostOwnerUnavailable",
                    "The project writer has no available authenticated endpoint; nothing was generated",
                )
            })?;
        let context = live_project::inspect(&mut client)?.0;
        if cancelled.load(Ordering::Acquire) {
            return Err(GenerationError::Cancelled.into());
        }
        let project_id = context.project_id;
        let mut current = status(
            live_project::request(
                &mut client,
                Operation::Generate {
                    project_id: project_id.clone(),
                    request: GenerateRequest {
                        hold,
                        expected_revision: context.revision_id,
                        variants,
                        seed,
                        options,
                    },
                },
            )?,
            None,
        )?;
        let job = current.job;
        // A job runs at most one worker deadline per variant.
        let deadline =
            Instant::now() + attempt::WORKER_DEADLINE * u32::from(variants) + POLL_INTERVAL;
        let mut cancellation = None;
        let mut last = None;
        while !current.finished() {
            let line = progress(&current);
            if last.as_ref() != Some(&line) {
                eprintln!("{line}");
                last = Some(line);
            }
            if (cancelled.load(Ordering::Acquire) || Instant::now() >= deadline)
                && cancellation.is_none()
            {
                cancellation = Some(Instant::now() + CANCELLATION_DRAIN_LIMIT);
                current = live_project::request(
                    &mut client,
                    Operation::CancelGeneration {
                        project_id: project_id.clone(),
                        job,
                    },
                )
                .and_then(|reply| status(reply, Some(job)))
                .map_err(|error| lost(job, error))?;
                continue;
            }
            if cancellation.is_some_and(|limit| Instant::now() >= limit) {
                return Err(LiveError::new(
                    "HostOutcomeUnknown",
                    format!(
                        "The app has not confirmed cancelling AI job {job} within five minutes; inspect the project before generating again"
                    ),
                )
                .into());
            }
            std::thread::park_timeout(POLL_INTERVAL);
            current = live_project::request(
                &mut client,
                Operation::GenerationStatus {
                    project_id: project_id.clone(),
                    job,
                },
            )
            .and_then(|reply| status(reply, Some(job)))
            .map_err(|error| lost(job, error))?;
        }
        crate::write_json(&serde_json::json!({
            "protocol": 1,
            "routed": "live_project",
            "job": job,
            "request_id": current.request_id,
            "hold_id": current.hold,
            "options": current.options,
            "variants_requested": current.variants,
            "ready": current.ready,
            "elapsed_ms": current.elapsed_ms,
            "outcome": current.outcome,
            "note": current.note,
        }))?;
        // The result is out; a lost release only lets the owner keep it until
        // it expires.
        let _ = live_project::request(
            &mut client,
            Operation::ReleaseGenerationStatus {
                project_id: project_id.clone(),
                job,
            },
        );
        match current.outcome {
            Some(GenerationOutcome::Ready {}) => Ok(()),
            Some(GenerationOutcome::Cancelled {}) => Err(GenerationError::Cancelled.into()),
            Some(GenerationOutcome::Unavailable { reason }) => {
                Err(LiveError::new("GenerationUnavailable", reason).into())
            }
            Some(GenerationOutcome::Failed { reason }) => {
                Err(GenerationError::Failed(reason).into())
            }
            None => unreachable!("the loop ends on a finished job"),
        }
    }
}
