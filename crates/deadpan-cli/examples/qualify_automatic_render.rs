//! Real durable automatic SDR workflow, checkpoint retry and reconciliation.
//! Usage: qualify_automatic_render PACKAGE WORKER NEW_OUTPUT_DIRECTORY
//! The package and all newly published evidence must be in private /tmp scratch.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        EncodedWorkerLimits,
        verification::VerificationLimits,
        workflow::{
            PublicationRequest, ReconcileRender, RenderWorkflow, RetryRender, StartRender,
            WorkflowConfig, WorkflowIdentity, WorkflowOutcome, WorkflowStage, WorkflowStatus,
        },
    },
    render_worker::RenderWorkerRuntime,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        RenderAutomaticAlgorithm, RenderAutomaticPolicy, RenderAutomaticSelection, RenderPolicy,
    },
};
use deadpan_store::{AccessMode, ProjectStore, render_media::RenderMediaLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "qualify_render_workflow/reference.rs"]
mod reference;

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

fn save(file: &mut File, report: &Value) -> Result {
    let bytes = serde_json::to_vec_pretty(report)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("qualification report bound".into());
    }
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn publication_evidence(
    status: &WorkflowStatus,
    decision: &deadpan_jobs::render::admission::RenderEncodingDecision,
    intent: &deadpan_jobs::render::RenderIntent,
) -> Result<Value> {
    let receipt = status
        .receipt
        .as_ref()
        .ok_or("missing publication receipt")?;
    let mut bytes = Vec::new();
    File::open(&receipt.report)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if bytes.len() > 16 * 1024 * 1024
        || u64::try_from(bytes.len())? != receipt.report_bytes
        || digest != receipt.report_sha256.as_str()
    {
        return Err("published provenance differs from its exact receipt".into());
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    let reported_intent: deadpan_jobs::render::RenderIntent =
        serde_json::from_value(value["provenance"]["render_intent"].clone())?;
    let reported_decision: deadpan_jobs::render::admission::RenderEncodingDecision =
        serde_json::from_value(value["provenance"]["encoding_decision"].clone())?;
    if value["provenance"]["schema_version"] != 2
        || &reported_decision != decision
        || &reported_intent != intent
    {
        return Err("published provenance lost the original automatic decision".into());
    }
    Ok(value)
}

fn pump(
    workflow: &mut RenderWorkflow,
    store: &mut ProjectStore,
    deadline: Instant,
) -> Result<Vec<WorkflowStage>> {
    let result = (|| {
        let mut stages = Vec::new();
        while workflow.is_active() {
            if Instant::now() >= deadline {
                return Err("workflow deadline".into());
            }
            workflow.poll(store)?;
            let stage = workflow.status().stage;
            if stages.last() != Some(&stage) {
                if stages.len() >= 128 {
                    return Err("workflow stage bound".into());
                }
                stages.push(stage);
            }
            std::thread::park_timeout(Duration::from_millis(2));
        }
        let status = workflow.status();
        if status.outcome != Some(WorkflowOutcome::Published)
            || !status.cleanup_confirmed
            || !status.observed_movie_commit
            || status.journal_diagnostic.is_some()
        {
            return Err(format!("workflow failed: {}", serde_json::to_string(status)?).into());
        }
        Ok(stages)
    })();
    if result.is_err() {
        workflow.drain(store)?;
    }
    result
}

fn run(
    package: &Path,
    config: WorkflowConfig,
    directory: &Path,
    report: &mut Value,
    file: &mut File,
) -> Result {
    let deadline = Instant::now() + Duration::from_secs(15 * 60);
    ProjectStore::migrate(package)?;
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let document = store.snapshot()?;
    let initial = identity("automatic-qualification", "automatic-first")?;
    let destination = publication(directory, "automatic-first")?;
    let mut workflow = RenderWorkflow::new(&store, config.clone())?;
    workflow.start(
        &mut store,
        StartRender {
            revision: document.revision_id().clone(),
            range: None,
            identity: initial.clone(),
            policy: RenderPolicy::Automatic(RenderAutomaticPolicy {
                schema_version: 1,
                selection: RenderAutomaticSelection::Automatic,
                algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
            }),
            publication: destination.clone(),
            deadline,
        },
    )?;
    report["initial_stages"] = json!(pump(&mut workflow, &mut store, deadline)?);
    report["initial"] = serde_json::to_value(workflow.status())?;
    let intent = store.render_job(&initial.job_id)?;
    let decision = store
        .render_encoding_decision(&initial.job_id, &initial.attempt_id)?
        .ok_or("automatic encode has no durable decision")?;
    let checkpoint = store.render_checkpoint(&initial.job_id, &initial.attempt_id)?;
    decision.validate_for(&intent, &initial.attempt_id)?;
    if !decision.is_selected() || !intent.policy.is_automatic() || store.snapshot()? != document {
        return Err("automatic render changed authoring or lost its decision".into());
    }
    report["decision"] = serde_json::to_value(&decision)?;
    report["initial_report"] = publication_evidence(workflow.status(), &decision, &intent)?;
    report["checkpoint"] = serde_json::to_value(&checkpoint)?;
    report["direct_inputs"] = reference::capture(
        package,
        document.revision_id(),
        intent.range,
        "automatic",
        directory,
        deadline,
    )?;
    save(file, report)?;
    drop(workflow);
    drop(store);

    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let mut workflow = RenderWorkflow::new(&store, config)?;
    let retry = identity(initial.job_id.as_str(), "automatic-checkpoint-retry")?;
    workflow.retry(
        &mut store,
        RetryRender {
            identity: retry.clone(),
            checkpoint_attempt_id: Some(initial.attempt_id.clone()),
            publication: publication(directory, "automatic-checkpoint-retry")?,
            deadline,
        },
    )?;
    let stages = pump(&mut workflow, &mut store, deadline)?;
    if stages.contains(&WorkflowStage::Qualifying)
        || stages.contains(&WorkflowStage::Encoding)
        || store.render_encoding_decision(&initial.job_id, &initial.attempt_id)?
            != Some(decision.clone())
        || store
            .render_encoding_decision(&retry.job_id, &retry.attempt_id)?
            .is_some()
        || workflow
            .status()
            .receipt
            .as_ref()
            .ok_or("missing retry receipt")?
            .movie_sha256
            != *checkpoint.media.movie_sha256()
    {
        return Err("checkpoint retry changed its original encoding evidence or re-encoded".into());
    }
    report["retry_stages"] = json!(stages);
    report["retry"] = serde_json::to_value(workflow.status())?;
    report["retry_report"] = publication_evidence(workflow.status(), &decision, &intent)?;
    save(file, report)?;

    workflow.reconcile(
        &mut store,
        ReconcileRender {
            publication_id: destination.publication_id,
            identity: identity(initial.job_id.as_str(), "automatic-reconcile")?,
            operation_id: AttemptId::new("automatic-reconcile-operation")?,
            cancellation_token: CancellationToken::new("automatic-reconcile-cancel")?,
            deadline,
        },
    )?;
    let stages = pump(&mut workflow, &mut store, deadline)?;
    if stages.contains(&WorkflowStage::Qualifying)
        || stages.contains(&WorkflowStage::Encoding)
        || store.render_encoding_decision(&initial.job_id, &initial.attempt_id)?
            != Some(decision.clone())
    {
        return Err("reconciliation changed the original encoder decision".into());
    }
    report["reconcile_stages"] = json!(stages);
    report["reconcile"] = serde_json::to_value(workflow.status())?;
    report["reconcile_report"] = publication_evidence(workflow.status(), &decision, &intent)?;
    if report["reconcile_report"] != report["initial_report"] {
        return Err("reconciliation replaced the already committed provenance report".into());
    }
    save(file, report)?;

    let cold = identity(initial.job_id.as_str(), "automatic-cold-retry")?;
    workflow.retry(
        &mut store,
        RetryRender {
            identity: cold.clone(),
            checkpoint_attempt_id: None,
            publication: publication(directory, "automatic-cold-retry")?,
            deadline,
        },
    )?;
    let stages = pump(&mut workflow, &mut store, deadline)?;
    let fresh = store
        .render_encoding_decision(&cold.job_id, &cold.attempt_id)?
        .ok_or("cold encode did not retain fresh qualification")?;
    fresh.validate_for(&intent, &cold.attempt_id)?;
    if !stages.contains(&WorkflowStage::Qualifying)
        || !stages.contains(&WorkflowStage::Encoding)
        || fresh.encoding_attempt_id == decision.encoding_attempt_id
        || fresh.probes.iter().any(|probe| {
            decision
                .probes
                .iter()
                .any(|old| old.identity == probe.identity)
        })
        || store.render_encoding_decision(&initial.job_id, &initial.attempt_id)? != Some(decision)
        || store.snapshot()? != document
    {
        return Err(
            "cold retry reused admission or changed existing authored/encoder history".into(),
        );
    }
    report["cold_stages"] = json!(stages);
    report["cold"] = serde_json::to_value(workflow.status())?;
    report["cold_report"] = publication_evidence(workflow.status(), &fresh, &intent)?;
    report["cold_decision"] = serde_json::to_value(fresh)?;
    report["authoring_preserved"] = json!(true);
    workflow.drain(&mut store)?;
    Ok(())
}

fn main() -> Result {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [package, worker, directory] = args.as_slice() else {
        return Err("expected PACKAGE WORKER NEW_OUTPUT_DIRECTORY".into());
    };
    let scratch = fs::canonicalize("/tmp")?;
    let package = fs::canonicalize(package)?;
    if !package.starts_with(&scratch) {
        return Err("package must be a /tmp fixture copy".into());
    }
    let directory = PathBuf::from(directory);
    let parent = fs::canonicalize(directory.parent().ok_or("output parent missing")?)?;
    if !parent.starts_with(&scratch) || parent.starts_with(&package) {
        return Err("output must be in /tmp outside the package".into());
    }
    fs::create_dir(&directory)?;
    let directory = fs::canonicalize(directory)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join("report.json"))?;
    let mut report = json!({"schema_version": 1, "status": "running", "package": package,
        "scope": "real automatic durable encoding, verified publication, checkpoint reopen/retry, reconciliation and cold retry",
        "limitations": ["bounded SDR synthetic project on one host", "no public Render UI, HDR, full effects or release qualification", "independent decoder comparison runs separately"]});
    save(&mut file, &report)?;
    let config = WorkflowConfig {
        package: package.clone(),
        runtime: RenderWorkerRuntime {
            executable: fs::canonicalize(worker)?,
            arguments: Vec::new(),
            environment: BTreeMap::new(),
        },
        encode_limits: EncodedWorkerLimits::default(),
        verification_limits: VerificationLimits::default(),
        media_limits: RenderMediaLimits::new(
            512 * 1024 * 1024,
            256 * 1024,
            512 * 1024 * 1024 + 256 * 1024,
            1024 * 1024 * 1024,
            256,
        )?,
    };
    let result = run(&package, config, &directory, &mut report, &mut file);
    report["status"] = json!(if result.is_ok() { "passed" } else { "failed" });
    if let Err(error) = &result {
        report["error"] = json!(error.to_string());
    }
    save(&mut file, &report)?;
    result
}
