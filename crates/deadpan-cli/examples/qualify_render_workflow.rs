//! Real coordinator qualification. Caller supplies distinct scratch packages.
//! Usage: qualify_render_workflow SOURCE_PACKAGE GENERATED_PACKAGE WORKER REPORT NEW_OUTPUT_DIRECTORY
//! Runs fresh encodes, a live edit/undo/redo, cancellation, reopened checkpoint
//! retry and publication reconciliation. Independent decoding is a separate check.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        EncodedProgress, EncodedWorkerLimits,
        verification::VerificationLimits,
        workflow::{
            PublicationRequest, ReconcileRender, RenderWorkflow, RetryRender, StartRender,
            WorkflowConfig, WorkflowIdentity, WorkflowOutcome, WorkflowProgress, WorkflowStatus,
        },
    },
    render_worker::RenderWorkerRuntime,
};
use deadpan_core::{FrameRange, ProjectFrame};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        RenderAttemptState, RenderBFrames, RenderEncoder, RenderEngineeringPolicy, RenderSelection,
    },
};
use deadpan_store::{AccessMode, ProjectStore, render_media::RenderMediaLimits};
use serde_json::{Value, json};

#[path = "qualify_render_jobs/evidence.rs"]
mod evidence;
#[path = "qualify_render_workflow/reference.rs"]
mod reference;
use evidence::{authored, check, coverage, mutate, same_content, save};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn identity(job: &str, attempt: &str) -> Result<WorkflowIdentity> {
    Ok(WorkflowIdentity {
        job_id: RequestId::new(job)?,
        attempt_id: AttemptId::new(attempt)?,
        cancellation_token: CancellationToken::new(format!("{attempt}-cancel"))?,
    })
}

fn publication(directory: &Path, name: &str) -> Result<PublicationRequest> {
    Ok(PublicationRequest {
        destination: directory.join(format!("{name}.mp4")),
        publication_id: RequestId::new(name)?,
        operation_id: AttemptId::new(format!("{name}-operation"))?,
        cancellation_token: CancellationToken::new(format!("{name}-publication-cancel"))?,
    })
}

fn policy() -> RenderEngineeringPolicy {
    RenderEngineeringPolicy {
        schema_version: 1,
        selection: RenderSelection::ExplicitEngineering,
        encoder: RenderEncoder::Hardware,
        b_frames: RenderBFrames::None,
    }
}

fn main() -> Result {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 5 {
        return Err(
            "expected SOURCE_PACKAGE GENERATED_PACKAGE WORKER REPORT NEW_OUTPUT_DIRECTORY".into(),
        );
    }
    let packages = [
        fs::canonicalize(&arguments[0])?,
        fs::canonicalize(&arguments[1])?,
    ];
    let scratch = fs::canonicalize("/tmp")?;
    if packages[0] == packages[1] || packages.iter().any(|path| !path.starts_with(&scratch)) {
        return Err("qualification requires two distinct /tmp packages".into());
    }
    let runtime = RenderWorkerRuntime {
        executable: fs::canonicalize(&arguments[2])?,
        arguments: Vec::new(),
        environment: BTreeMap::new(),
    };
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&arguments[3])?;
    let directory = PathBuf::from(&arguments[4]);
    fs::create_dir(&directory)?;
    let directory = directory.canonicalize()?;
    if !directory.starts_with(&scratch)
        || packages
            .iter()
            .any(|package| directory.starts_with(package))
    {
        return Err("qualification output must be in /tmp and outside both packages".into());
    }
    let deadline = Instant::now() + Duration::from_secs(15 * 60);
    let mut report = json!({
        "schema_version": 1, "status": "running", "cases": [],
        "scope": "real shared render workflow with immutable capture, live editing, cancellation, checkpoint retry and explicit publication reconciliation",
        "limitations": ["explicit engineering hardware/no-B-frame policy", "orderly restart, not injected process or power loss", "independent final-file decoding is a separate check", "no public automatic Render, full mastering/effects, HDR or release qualification"]
    });
    save(&mut output, &report)?;
    for (name, package, range) in [
        (
            "structural",
            &packages[0],
            FrameRange::new(ProjectFrame(20), ProjectFrame(128))?,
        ),
        (
            "generated",
            &packages[1],
            FrameRange::new(ProjectFrame(0), ProjectFrame(30))?,
        ),
    ] {
        let mut case = json!({"name": name, "package": package, "status": "running", "checks": []});
        let result = run_case(
            name, package, range, &runtime, &directory, deadline, &mut case,
        );
        case["status"] = json!(if result.is_ok() { "passed" } else { "failed" });
        if let Err(error) = &result {
            case["error"] = json!(error.to_string());
        }
        report["cases"]
            .as_array_mut()
            .ok_or("missing cases")?
            .push(case);
        report["status"] = json!(if result.is_ok() { "running" } else { "failed" });
        save(&mut output, &report)?;
        result?;
    }
    report["status"] = json!("passed");
    save(&mut output, &report)
}

fn configuration(package: &Path, runtime: &RenderWorkerRuntime) -> Result<WorkflowConfig> {
    let mut encode_limits = EncodedWorkerLimits::default();
    encode_limits.encode.maximum_output_bytes = 512 * 1024 * 1024;
    Ok(WorkflowConfig {
        package: package.to_owned(),
        runtime: runtime.clone(),
        encode_limits,
        verification_limits: VerificationLimits::default(),
        media_limits: RenderMediaLimits::new(
            512 * 1024 * 1024,
            256 * 1024,
            512 * 1024 * 1024 + 256 * 1024,
            1024 * 1024 * 1024,
            256,
        )?,
    })
}

fn pump(
    workflow: &mut RenderWorkflow,
    store: &mut ProjectStore,
    deadline: Instant,
    mut observe: impl FnMut(&mut RenderWorkflow, &mut ProjectStore) -> Result,
) -> Result<Vec<Value>> {
    match pump_until_stopped(workflow, store, deadline, &mut observe) {
        Ok(events) => Ok(events),
        Err(primary) => match workflow.drain(store) {
            Ok(()) => Err(primary),
            Err(cleanup) => Err(format!("{primary}; cleanup: {cleanup}").into()),
        },
    }
}

fn pump_until_stopped(
    workflow: &mut RenderWorkflow,
    store: &mut ProjectStore,
    deadline: Instant,
    observe: &mut impl FnMut(&mut RenderWorkflow, &mut ProjectStore) -> Result,
) -> Result<Vec<Value>> {
    let mut events = Vec::new();
    while workflow.is_active() {
        if Instant::now() >= deadline {
            return Err("qualification pump exceeded its deadline".into());
        }
        if workflow.poll(store)? {
            if events.len() >= 4096 {
                return Err("workflow event evidence bound".into());
            }
            events.push(serde_json::to_value(workflow.status())?);
        }
        observe(workflow, store)?;
        std::thread::park_timeout(Duration::from_millis(2));
    }
    Ok(events)
}

fn successful(report: &mut Value, label: &str, status: &WorkflowStatus) -> Result {
    check(
        report,
        label,
        status.outcome == Some(WorkflowOutcome::Published)
            && status.cleanup_confirmed
            && status.observed_movie_commit
            && status.receipt.is_some()
            && status.journal_diagnostic.is_none(),
        serde_json::to_value(status)?,
    )
}

fn run_case(
    name: &str,
    package: &Path,
    range: FrameRange,
    runtime: &RenderWorkerRuntime,
    directory: &Path,
    deadline: Instant,
    report: &mut Value,
) -> Result {
    let cancelled = AtomicBool::new(false);
    let config = configuration(package, runtime)?;
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let before = store.snapshot()?;
    let before_rows = authored(package)?;
    report["coverage"] = coverage(&before, range)?;
    report["direct_inputs"] = reference::capture(
        package,
        before.revision_id(),
        range,
        name,
        directory,
        deadline,
    )?;
    let job = format!("workflow-{name}");
    let first = identity(&job, &format!("{job}-encode"))?;
    let destination = publication(directory, &format!("{job}-published"))?;
    let mut workflow = RenderWorkflow::new(&store, config.clone())?;
    let request = StartRender {
        revision: before.revision_id().clone(),
        range: Some(range),
        identity: first.clone(),
        policy: policy(),
        publication: destination.clone(),
        deadline,
    };
    workflow.start(&mut store, request.clone())?;
    check(
        report,
        "a second start cannot overlap the complete workflow",
        workflow.start(&mut store, request).is_err(),
        json!({}),
    )?;
    let wrong = identity(&job, "wrong-cancel-target")?;
    check(
        report,
        "stale cancellation cannot affect the captured run",
        workflow.cancel(&mut store, &wrong).is_err() && !workflow.status().cancellation_requested,
        json!({}),
    )?;

    let mut mutation = None;
    let mut edit_latency = None;
    let events = pump(&mut workflow, &mut store, deadline, |workflow, store| {
        if name == "structural"
            && mutation.is_none()
            && let Some(WorkflowProgress::Encoding {
                completed_frames,
                total_frames,
                completed_audio_samples,
                total_audio_samples,
            }) = workflow.status().progress.clone()
            && completed_frames > 0
            && completed_frames < total_frames
        {
            let start = Instant::now();
            mutation = Some(mutate(
                store,
                EncodedProgress {
                    completed_frames,
                    total_frames,
                    completed_audio_samples,
                    total_audio_samples,
                },
                "workflow",
            )?);
            edit_latency = Some(start.elapsed().as_secs_f64());
        }
        Ok(())
    })?;
    report["events"] = json!(events);
    successful(
        report,
        "complete workflow publishes and drains",
        workflow.status(),
    )?;
    report["published"] = serde_json::to_value(workflow.status())?;
    if name == "structural" {
        check(
            report,
            "edit, undo, redo and undo run during interior encoding progress",
            mutation.is_some() && same_content(&before, &store.snapshot()?)?,
            json!({"mutation": mutation, "seconds_for_four_transactions": edit_latency}),
        )?;
    } else {
        check(
            report,
            "generated render preserves exact authored history",
            authored(package)? == before_rows,
            json!({}),
        )?;
    }
    let stable_rows = authored(package)?;
    let checkpoint = store.render_checkpoint(&first.job_id, &first.attempt_id)?;
    let snapshot = store.render_read_handle().snapshot(
        &checkpoint.media,
        config.media_limits,
        &cancelled,
        deadline,
    )?;
    let retained: Value = serde_json::from_slice(snapshot.manifest_bytes())?;
    report["manifest"] = retained
        .get("encoded")
        .cloned()
        .ok_or("missing encoded manifest")?;
    report["contract"] = report["manifest"]["contract"]["picture"].clone();
    report["path"] = json!(
        workflow
            .status()
            .receipt
            .as_ref()
            .ok_or("missing receipt")?
            .movie
    );
    check(
        report,
        "captured revision and direct inputs match retained encoding",
        report["contract"] == report["direct_inputs"]["contract"]
            && store.snapshot_at(before.revision_id())? == before,
        json!({"captured_revision": before.revision_id(), "current_revision": store.snapshot()?.revision_id()}),
    )?;
    drop(snapshot);

    if name == "structural" {
        let cancel_id = identity("workflow-cancel", "workflow-cancel-encode")?;
        let cancel_destination = publication(directory, "workflow-cancel-published")?;
        let cancel_revision = store.snapshot()?.revision_id().clone();
        workflow.start(
            &mut store,
            StartRender {
                revision: cancel_revision,
                range: Some(range),
                identity: cancel_id.clone(),
                policy: policy(),
                publication: cancel_destination.clone(),
                deadline,
            },
        )?;
        let mut requested = false;
        report["cancel_events"] = json!(pump(
            &mut workflow,
            &mut store,
            deadline,
            |workflow, store| {
                if !requested
                    && let Some(WorkflowProgress::Encoding {
                        completed_frames,
                        total_frames,
                        ..
                    }) = &workflow.status().progress
                    && *completed_frames > 0
                    && *completed_frames < *total_frames
                {
                    workflow.cancel(store, &cancel_id)?;
                    requested = true;
                }
                Ok(())
            }
        )?);
        check(
            report,
            "real encoder cancellation drains before terminal journal state",
            requested
                && workflow.status().outcome == Some(WorkflowOutcome::Cancelled)
                && workflow.status().cleanup_confirmed
                && !cancel_destination.destination.exists()
                && store
                    .render_attempt(&cancel_id.job_id, &cancel_id.attempt_id)?
                    .state
                    == RenderAttemptState::Cancelled,
            serde_json::to_value(workflow.status())?,
        )?;
    }
    drop(workflow);
    drop(store);

    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let mut workflow = RenderWorkflow::new(&store, config)?;
    let retry_id = identity(&job, &format!("{job}-verify-after-reopen"))?;
    workflow.retry(
        &mut store,
        RetryRender {
            identity: retry_id.clone(),
            checkpoint_attempt_id: Some(first.attempt_id.clone()),
            publication: publication(directory, &format!("{job}-retry-published"))?,
            deadline,
        },
    )?;
    report["retry_events"] = json!(pump(&mut workflow, &mut store, deadline, |_, _| Ok(()))?);
    successful(
        report,
        "reopened checkpoint retry publishes without another encode",
        workflow.status(),
    )?;
    check(
        report,
        "retry uses the original retained movie bytes and checkpoint",
        workflow
            .status()
            .receipt
            .as_ref()
            .is_some_and(|receipt| receipt.movie_sha256 == *checkpoint.media.movie_sha256())
            && store
                .render_attempt(&retry_id.job_id, &retry_id.attempt_id)?
                .checkpoint_attempt_id
                .as_ref()
                == Some(&first.attempt_id),
        serde_json::to_value(workflow.status())?,
    )?;

    workflow.reconcile(
        &mut store,
        ReconcileRender {
            publication_id: destination.publication_id.clone(),
            identity: identity(&job, &format!("{job}-verify-reconcile"))?,
            operation_id: AttemptId::new(format!("{job}-reconcile-operation"))?,
            cancellation_token: CancellationToken::new(format!("{job}-reconcile-cancel"))?,
            deadline,
        },
    )?;
    report["reconcile_events"] = json!(pump(&mut workflow, &mut store, deadline, |_, _| Ok(()))?);
    successful(
        report,
        "reconciliation freshly verifies and confirms the original destination",
        workflow.status(),
    )?;
    check(
        report,
        "all recovery work preserves current and captured authoring",
        authored(package)? == stable_rows && store.snapshot_at(before.revision_id())? == before,
        json!({"authoring": stable_rows, "publication": store.render_publication(&destination.publication_id)?}),
    )?;
    let attempts = store.render_attempts(&first.job_id, 0, 10)?;
    check(
        report,
        "exactly one encoding attempt and two fresh verification retries",
        attempts.len() == 3
            && attempts.iter().all(|attempt| {
                attempt.state == RenderAttemptState::Verified
                    && attempt.checkpoint_attempt_id.as_ref() == Some(&first.attempt_id)
            }),
        json!(attempts),
    )?;
    store.validate()?;
    report["final_checkpoint"] = json!(checkpoint);
    report["final_status"] = serde_json::to_value(workflow.status())?;
    Ok(())
}
