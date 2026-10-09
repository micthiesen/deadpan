use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::PermissionsExt,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        EncodedWorkerLimits,
        verification::{VerificationLimits, VerificationStage},
        workflow::{
            PublicationRequest, RenderWorkflow, RetryRender, StartRender, WorkflowConfig,
            WorkflowIdentity, WorkflowOutcome, WorkflowProgress, WorkflowStage,
        },
    },
    render_worker::RenderWorkerRuntime,
};
use deadpan_core::{FrameRange, ProjectFrame};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        RenderAttemptState, RenderAutomaticAlgorithm, RenderAutomaticPolicy,
        RenderAutomaticSelection,
    },
};
use deadpan_store::{AccessMode, ProjectStore, render_media::RenderMediaLimits};
use serde_json::{Value, json};

#[path = "evidence.rs"]
mod evidence;
#[path = "process.rs"]
mod process;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const HOST_DEADLINE: Duration = Duration::from_secs(180);
const GROUP_DEADLINE: Duration = Duration::from_secs(20);
const CASE_EXECUTABLE: &str = "qualify_render_host_crash";
const GATE_CONFIG: &str = "render-host-crash-gate.json";

pub(super) fn run() -> Result<ExitCode> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|argument| {
        [
            deadpan_cli::encoded_render::PRIVATE_WORKER_ARGUMENT,
            deadpan_cli::encoded_render::verification::PRIVATE_WORKER_ARGUMENT,
            deadpan_cli::encoded_render::admission::PRIVATE_WORKER_ARGUMENT,
        ]
        .contains(&argument.as_str())
    }) {
        let (directory, _) = current_case()?;
        let role = match arguments[0].as_str() {
            deadpan_cli::encoded_render::PRIVATE_WORKER_ARGUMENT => "encoding",
            deadpan_cli::encoded_render::verification::PRIVATE_WORKER_ARGUMENT => "verification",
            _ => "admission",
        };
        let receipt = directory.join(format!("worker-{}.json", std::process::id()));
        if receipt.exists() {
            return Err("worker receipt identity reused".into());
        }
        let group = u32::try_from(rustix::process::getpgrp().as_raw_pid())?;
        require(
            group == std::process::id(),
            "worker did not start as its group leader",
        )?;
        evidence::save(
            &receipt,
            &json!({"pid": std::process::id(), "group": group, "role": role,
            "executable": std::env::current_exe()?, "arguments": arguments}),
        )?;
        return Ok(deadpan_cli::entry(arguments));
    }
    if let [mode, package, directory, stage] = arguments.as_slice()
        && mode == "--host"
    {
        host(Path::new(package), Path::new(directory), stage)?;
        return Ok(ExitCode::SUCCESS);
    }
    let [package, output] = arguments.as_slice() else {
        return Err("expected SCRATCH_PACKAGE NEW_EVIDENCE_DIRECTORY".into());
    };
    let package = fs::canonicalize(package)?;
    let scratch = fs::canonicalize("/tmp")?;
    let output = std::path::absolute(output)?;
    let output = output
        .parent()
        .ok_or("output has no parent")?
        .canonicalize()?
        .join(output.file_name().ok_or("output has no name")?);
    if !package.starts_with(&scratch)
        || !output.starts_with(&scratch)
        || output.starts_with(&package)
        || package.starts_with(&output)
    {
        return Err("package and new evidence directory must be separate paths under /tmp".into());
    }
    fs::create_dir(&output)?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let binary = evidence::hash(&executable)?;
    let started = Instant::now();
    let mut report = json!({"schema_version": 1, "status": "running", "package": package,
        "executable": binary, "cases": [], "scope":
        "production automatic SDR RenderWorkflow; real encode and full-file verifier processes; SIGKILL of host leader only after flushed intermediate progress and matching public status",
        "limitations": ["direct coordinator harness, not native UI or authenticated IPC",
            "first at most 120 frames of a caller-prepared fixture",
            "process groups are observed, not containment of escaped processes",
            "no physical power-loss or crash inside a filesystem or SQLite call",
            "qualification pause is absent from ordinary builds"]});
    evidence::save(&output.join("report.json"), &report)?;
    let result = (|| -> Result {
        let database = rusqlite::Connection::open_with_flags(
            package.join("project.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let schema: u32 = database.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema != deadpan_store::DATABASE_SCHEMA_VERSION {
            return Err("qualification requires a current-format package".into());
        }
        drop(database);
        let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        let document = initial.snapshot()?;
        if document.duration()?.frames() < 8 {
            return Err("qualification requires at least eight authored frames".into());
        }
        report["revision"] = json!(document.revision_id());
        report["captured_frames"] = json!(document.duration()?.frames().min(120));
        drop(initial);
        let before = evidence::authored(&package)?;
        evidence::save(&output.join("authored-before.json"), &before)?;
        for stage in ["encoding", "verification"] {
            let directory = output.join(stage);
            fs::create_dir(&directory)?;
            let mut case = json!({"stage": stage, "status": "running"});
            let outcome = crash_case(&package, &directory, stage, &before, &mut case);
            case["status"] = json!(if outcome.is_ok() { "passed" } else { "failed" });
            if let Err(error) = &outcome {
                case["error"] = json!(error.to_string());
            }
            evidence::save(&directory.join("case.json"), &case)?;
            report["cases"]
                .as_array_mut()
                .ok_or("no report cases")?
                .push(case);
            evidence::save(&output.join("report.json"), &report)?;
            outcome?;
        }
        let after = evidence::authored(&package)?;
        evidence::save(&output.join("authored-after.json"), &after)?;
        require(before == after, "authored/history cells changed")?;
        require(
            evidence::hash(&executable)? == binary,
            "executed binary changed",
        )?;
        report["authored_history_preserved"] = json!(true);
        Ok(())
    })();
    report["status"] = json!(if result.is_ok() { "passed" } else { "failed" });
    report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    if let Err(error) = &result {
        report["error"] = json!(error.to_string());
    }
    evidence::save(&output.join("report.json"), &report)?;
    result?;
    println!("{}", output.join("report.json").display());
    Ok(ExitCode::SUCCESS)
}

fn crash_case(
    package: &Path,
    directory: &Path,
    stage: &str,
    authored: &Value,
    record: &mut Value,
) -> Result {
    let executable = prepare_case(directory, stage, record)?;
    let mut command = Command::new(&executable);
    command
        .arg("--host")
        .arg(package)
        .arg(directory)
        .arg(stage)
        .stdin(Stdio::null())
        .stdout(File::create(directory.join("host.stdout"))?)
        .stderr(File::create(directory.join("host.stderr"))?);
    let mut host = process::OwnedChild::spawn(&mut command)?;
    record["host_pid"] = json!(host.id());
    let result = crash_with_host(&mut host, package, directory, stage, authored, record);
    if result.is_err() {
        failure_cleanup(&mut host, directory, record);
    }
    let after = evidence::hash(&executable)?;
    record["executable_after"] = after.clone();
    require(after == record["executable"], "case executable changed")?;
    let configuration_after = evidence::hash(&directory.join(GATE_CONFIG))?;
    record["gate_configuration_file_after"] = configuration_after.clone();
    require(
        configuration_after == record["gate_configuration_file"],
        "case gate configuration changed",
    )?;
    result
}

fn gate_config(stage: &str) -> Result<Value> {
    require(
        ["encoding", "verification"].contains(&stage),
        "unknown crash stage",
    )?;
    let identity = identity(stage, false)?;
    Ok(json!({"schema_version":1,"stage":stage,"identity":{
        "request_id":identity.job_id,"attempt_id":identity.attempt_id}}))
}

/// Each helper discovers only the fixed configuration beside its own immutable
/// executable. The normal runtime remains free of argument/environment overrides.
fn prepare_case(directory: &Path, stage: &str, record: &mut Value) -> Result<PathBuf> {
    let source = std::env::current_exe()?.canonicalize()?;
    let source_hash = evidence::hash(&source)?;
    let executable = directory.join(CASE_EXECUTABLE);
    let source_file = File::open(&source)?;
    let mut destination = File::create_new(&executable)?;
    let copy_bound = source_hash["bytes"]
        .as_u64()
        .and_then(|bytes| bytes.checked_add(1))
        .ok_or("case executable copy bound is unavailable")?;
    let copied = std::io::copy(&mut source_file.take(copy_bound), &mut destination)?;
    destination.set_permissions(fs::Permissions::from_mode(0o555))?;
    destination.sync_all()?;
    let copied_hash = evidence::hash(&executable)?;
    require(
        copied_hash["sha256"] == source_hash["sha256"]
            && copied_hash["bytes"] == source_hash["bytes"]
            && copied_hash["bytes"] == copied,
        "case executable differs from qualification executable",
    )?;
    let configuration = gate_config(stage)?;
    let configuration_path = directory.join(GATE_CONFIG);
    evidence::save(&configuration_path, &configuration)?;
    fs::set_permissions(&configuration_path, fs::Permissions::from_mode(0o444))?;
    File::open(&configuration_path)?.sync_all()?;
    File::open(directory)?.sync_all()?;
    record["source_executable"] = source_hash;
    record["executable"] = copied_hash;
    record["gate_configuration"] = configuration;
    record["gate_configuration_file"] = evidence::hash(&configuration_path)?;
    Ok(executable)
}

fn current_case() -> Result<(PathBuf, String)> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let directory = executable.parent().ok_or("case executable has no parent")?;
    require(
        executable
            .file_name()
            .is_some_and(|name| name == CASE_EXECUTABLE)
            && directory.starts_with(fs::canonicalize("/tmp")?),
        "qualification worker requires its own scratch case executable",
    )?;
    let configuration_path = directory.join(GATE_CONFIG);
    require(
        fs::symlink_metadata(&configuration_path)?
            .file_type()
            .is_file()
            && fs::metadata(&configuration_path)?.len() <= 4096,
        "invalid adjacent qualification configuration extent",
    )?;
    let configuration = evidence::read_bounded(&configuration_path, 4096)?;
    let stage = configuration["stage"]
        .as_str()
        .ok_or("qualification configuration has no stage")?;
    require(
        configuration == gate_config(stage)?,
        "qualification configuration does not bind the exact initial attempt",
    )?;
    Ok((directory.to_owned(), stage.to_owned()))
}

fn crash_with_host(
    host: &mut process::OwnedChild,
    package: &Path,
    directory: &Path,
    stage: &str,
    authored: &Value,
    record: &mut Value,
) -> Result {
    let deadline = Instant::now() + HOST_DEADLINE;
    while !directory.join("host-ready.json").exists() {
        for name in ["host.stdout", "host.stderr"] {
            require(
                fs::metadata(directory.join(name))?.len() <= 8 * 1024 * 1024,
                "host log exceeded qualification bound",
            )?;
        }
        if host.exited()? {
            let status = host.reap()?;
            return Err(format!("host exited before matching progress witness: {status}").into());
        }
        if Instant::now() >= deadline {
            return Err("host progress witness timed out".into());
        }
        std::thread::park_timeout(Duration::from_millis(10));
    }
    let ready = evidence::read(&directory.join("host-ready.json"))?;
    let gate = evidence::read(&directory.join("gate-ready.json"))?;
    require(
        ready["host_pid"] == host.id() && gate["stage"] == stage && ready["gate"] == gate,
        "host/gate witness identity differs",
    )?;
    record["public_status"] = ready["status"].clone();
    record["gate"] = gate.clone();
    let worker_pid = u32::try_from(gate["pid"].as_u64().ok_or("gate has no worker pid")?)?;
    let processes = process::snapshot()?;
    let worker = processes
        .iter()
        .find(|entry| entry.pid == worker_pid)
        .ok_or("gated worker is absent before host kill")?;
    require(
        worker.group == worker_pid && worker.parent == host.id(),
        "gated worker process-group/parent identity differs",
    )?;
    record["worker_before_kill"] = json!(worker);
    let identity = identity(stage, false)?;
    let reader = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let attempt = reader.render_attempt(&identity.job_id, &identity.attempt_id)?;
    let expected = if stage == "encoding" {
        RenderAttemptState::Encoding
    } else {
        RenderAttemptState::Verifying
    };
    require(
        attempt.state == expected,
        "durable pre-crash attempt state differs",
    )?;
    require(
        evidence::authored(package)? == *authored,
        "authoring changed before crash",
    )?;
    let checkpoint_before = if stage == "verification" {
        Some(evidence::checkpoint(
            &reader,
            &reader.render_checkpoint(&identity.job_id, &identity.attempt_id)?,
        )?)
    } else {
        require(
            attempt.checkpoint_attempt_id.is_none(),
            "encoding unexpectedly retained a checkpoint",
        )?;
        None
    };
    record["checkpoint_before"] = json!(checkpoint_before);
    record["attempt_before"] = json!(attempt);
    drop(reader);
    evidence::save(&directory.join("before-kill.json"), record)?;
    // Reconfirm the same live worker after potentially expensive checkpoint
    // hashing. The gate's finite deadline cannot turn a stale witness into a pass.
    let live = process::snapshot()?;
    require(
        live.iter().any(|entry| entry == worker),
        "gated worker changed before SIGKILL",
    )?;
    let killed_at = Instant::now();
    let status = host.kill_host_only()?;
    require(status.signal() == Some(9), "host did not exit from SIGKILL")?;
    record["host_exit_signal"] = json!(status.signal());
    let workers = evidence::worker_receipts(directory)?;
    require(
        workers.iter().any(|entry| entry["pid"] == worker_pid),
        "gated worker lacks launch receipt",
    )?;
    let groups = worker_groups(&workers)?;
    observe_groups(&groups, &directory.join("group-observations.json"), record)?;
    record["helper_groups_absent_after_seconds"] = json!(killed_at.elapsed().as_secs_f64());
    let reader = ProjectStore::open(package, AccessMode::ReadOnly)?;
    require(
        reader
            .render_attempt(&identity.job_id, &identity.attempt_id)?
            .state
            == expected,
        "read-only open changed abandoned attempt",
    )?;
    drop(reader);
    require(
        evidence::authored(package)? == *authored,
        "host crash changed authored/history cells",
    )?;
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let recovered = store.render_attempt(&identity.job_id, &identity.attempt_id)?;
    require(
        recovered.state == RenderAttemptState::Interrupted
            && recovered
                .diagnostic
                .as_ref()
                .is_some_and(|value| value.code == "InterruptedOnOpen"),
        "writable recovery did not report InterruptedOnOpen",
    )?;
    let recovery = store.open_recovery();
    require(
        recovery.unclean_previous_writer.is_some()
            && recovery.record_error.is_none()
            && recovery.interrupted_renders.iter().any(|value| {
                value.job_id == identity.job_id.as_str()
                    && value.attempt_id == identity.attempt_id.as_str()
            }),
        "recovery report lost the abandoned render identity",
    )?;
    record["recovery"] = json!(recovery);
    record["recovered_attempt"] = json!(recovered);
    require(
        evidence::authored(package)? == *authored,
        "recovery changed authored/history cells",
    )?;
    if let Some(before) = checkpoint_before {
        let retained = store.render_checkpoint(&identity.job_id, &identity.attempt_id)?;
        require(
            evidence::checkpoint(&store, &retained)? == before,
            "recovery changed checkpoint bytes or identity",
        )?;
        retry(package, directory, stage, &mut store, record)?;
        require(
            evidence::checkpoint(&store, &retained)? == before,
            "retry changed retained checkpoint",
        )?;
    } else {
        require(
            recovered.checkpoint_attempt_id.is_none(),
            "recovery invented a checkpoint",
        )?;
    }
    require(
        !directory.join("crashed.mp4").exists(),
        "crashed operation published a movie",
    )?;
    require(
        evidence::authored(package)? == *authored,
        "retry changed authored/history cells",
    )?;
    record["worker_launches"] = json!(evidence::worker_receipts(directory)?);
    record["authored_history_preserved"] = json!(true);
    store.acknowledge_recovery()?;
    Ok(())
}

fn failure_cleanup(host: &mut process::OwnedChild, directory: &Path, record: &mut Value) {
    let mut cleanup = json!({"status": "observing", "helpers_signalled_by_harness": false});
    let host_result = host.stop_after_failure();
    cleanup["host_cleanup"] = match &host_result {
        Ok(()) => json!({"status": "stopped_or_already_reaped"}),
        Err(error) => json!({"status": "failed", "error": error.to_string()}),
    };
    let helpers = (|| -> Result {
        let workers = evidence::worker_receipts(directory)?;
        cleanup["worker_launches"] = json!(workers);
        let groups = worker_groups(&workers)?;
        observe_groups(
            &groups,
            &directory.join("failure-group-observations.json"),
            &mut cleanup,
        )
    })();
    cleanup["status"] = json!(if host_result.is_ok() && helpers.is_ok() {
        "confirmed"
    } else {
        "unconfirmed"
    });
    if let Err(error) = helpers {
        cleanup["helper_observation_error"] = json!(error.to_string());
    }
    record["failure_cleanup"] = cleanup;
    // The original error is returned by crash_case regardless of cleanup.
}

fn worker_groups(workers: &[Value]) -> Result<Vec<u32>> {
    workers
        .iter()
        .map(|entry| {
            let pid = entry["pid"].as_u64().ok_or("worker receipt has no pid")?;
            let group = entry["group"]
                .as_u64()
                .ok_or("worker receipt has no group")?;
            require(
                group == pid && group > 1,
                "worker receipt has no valid leader group",
            )?;
            u32::try_from(group).map_err(Into::into)
        })
        .collect()
}

fn observe_groups(groups: &[u32], output: &Path, record: &mut Value) -> Result {
    let started = Instant::now();
    let deadline = started + GROUP_DEADLINE;
    let mut observations = Vec::new();
    loop {
        let members: Vec<_> = process::snapshot()?
            .into_iter()
            .filter(|entry| groups.contains(&entry.group))
            .collect();
        let empty = members.is_empty();
        observations
            .push(json!({"elapsed_seconds": started.elapsed().as_secs_f64(), "members": members}));
        if empty || Instant::now() >= deadline || observations.len() >= 256 {
            evidence::save(output, &observations)?;
            record["observed_worker_groups"] = json!(groups);
            record["helpers_signalled_by_harness"] = json!(false);
            require(empty, "helper process groups survived host SIGKILL")?;
            return Ok(());
        }
        std::thread::park_timeout(Duration::from_millis(100));
    }
}

fn host(package: &Path, directory: &Path, stage: &str) -> Result {
    let (adjacent, configured_stage) = current_case()?;
    require(
        adjacent == directory && configured_stage == stage,
        "host arguments differ from its adjacent case configuration",
    )?;
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let document = store.snapshot()?;
    let mut workflow = RenderWorkflow::new(&store, config(package, directory)?)?;
    let identity = identity(stage, false)?;
    let deadline = Instant::now() + HOST_DEADLINE;
    workflow.start(
        &mut store,
        StartRender {
            revision: document.revision_id().clone(),
            range: Some(FrameRange::new(
                ProjectFrame(0),
                ProjectFrame(document.duration()?.frames().min(120)),
            )?),
            identity: identity.clone(),
            policy: RenderAutomaticPolicy {
                schema_version: 1,
                selection: RenderAutomaticSelection::Automatic,
                algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
            }
            .into(),
            publication: publication(directory, stage, false)?,
            deadline,
        },
    )?;
    let result = (|| -> Result {
        let mut witnessed = false;
        loop {
            workflow.poll(&mut store)?;
            let status = workflow.status();
            let progress_counts = match (stage, status.stage, status.progress.as_ref()) {
                (
                    "encoding",
                    WorkflowStage::Encoding,
                    Some(WorkflowProgress::Encoding {
                        completed_frames,
                        total_frames,
                        ..
                    }),
                ) if *completed_frames > 0 && completed_frames < total_frames => {
                    Some((*completed_frames, *total_frames))
                }
                (
                    "verification",
                    WorkflowStage::Verifying,
                    Some(WorkflowProgress::Verification(value)),
                ) if value.stage == VerificationStage::Pictures
                    && value.completed > 0
                    && value.completed < value.total =>
                {
                    Some((value.completed, value.total))
                }
                _ => None,
            };
            if !witnessed
                && let Some((completed, total)) = progress_counts
                && directory.join("gate-ready.json").exists()
            {
                let gate = evidence::read(&directory.join("gate-ready.json"))?;
                require(
                    gate["identity"]["request_id"] == identity.job_id.as_str()
                        && gate["identity"]["attempt_id"] == identity.attempt_id.as_str()
                        && gate["completed"] == completed
                        && gate["total"] == total
                        && status.identity.as_ref() == Some(&identity),
                    "gate identity or counts differ from current public workflow",
                )?;
                evidence::save(
                    &directory.join("host-ready.json"),
                    &json!({"host_pid": std::process::id(), "status": status, "gate": gate}),
                )?;
                witnessed = true;
            }
            if !workflow.is_active() {
                evidence::save(&directory.join("host-terminal.json"), workflow.status())?;
                return Err("workflow finished before host SIGKILL".into());
            }
            if Instant::now() >= deadline {
                return Err("host qualification deadline exceeded".into());
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
    })();
    // Only setup/error paths arrive here. The measured SIGKILL never executes
    // this cancellation or the coordinator's destructor.
    if workflow.is_active() {
        let cancellation = workflow.cancel(&mut store, &identity);
        let drained = workflow.drain(&mut store);
        cancellation?;
        drained?;
    }
    result
}

fn retry(
    package: &Path,
    directory: &Path,
    stage: &str,
    store: &mut ProjectStore,
    record: &mut Value,
) -> Result {
    let before = evidence::worker_receipts(directory)?;
    let encoder_count = |values: &[Value]| {
        values
            .iter()
            .filter(|value| value["role"] == "encoding")
            .count()
    };
    require(
        encoder_count(&before) == 1,
        "verifier case did not begin with exactly one real encoder",
    )?;
    let identity = identity(stage, true)?;
    let original = self::identity(stage, false)?;
    let mut workflow = RenderWorkflow::new(store, config(package, directory)?)?;
    let deadline = Instant::now() + HOST_DEADLINE;
    workflow.retry(
        store,
        RetryRender {
            identity: identity.clone(),
            checkpoint_attempt_id: Some(original.attempt_id.clone()),
            publication: publication(directory, stage, true)?,
            deadline,
        },
    )?;
    let result = (|| -> Result {
        let mut stages = Vec::new();
        while workflow.is_active() {
            workflow.poll(store)?;
            let stage = workflow.status().stage;
            if stages.last() != Some(&stage) {
                stages.push(stage);
            }
            require(
                stage != WorkflowStage::Encoding && stage != WorkflowStage::Qualifying,
                "checkpoint retry reentered encoding or encoder admission",
            )?;
            if Instant::now() >= deadline {
                return Err("checkpoint retry timed out".into());
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
        evidence::save(&directory.join("retry-status.json"), workflow.status())?;
        require(
            workflow.status().outcome == Some(WorkflowOutcome::Published)
                && workflow.status().cleanup_confirmed
                && workflow.can_release_writer(),
            "checkpoint retry did not publish with confirmed cleanup",
        )?;
        let attempts = store.render_attempts(&identity.job_id, 0, 3)?;
        require(
            attempts.len() == 2
                && attempts[0].state == RenderAttemptState::Interrupted
                && attempts[1].state == RenderAttemptState::Verified
                && attempts[1].checkpoint_attempt_id.as_ref() == Some(&original.attempt_id),
            "retry attempt lifecycle or retained encoding identity differs",
        )?;
        let after = evidence::worker_receipts(directory)?;
        require(
            encoder_count(&after) == 1
                && after.len() == before.len() + 1
                && after
                    .iter()
                    .filter(|value| value["role"] == "verification")
                    .count()
                    == 2,
            "checkpoint retry did not run exactly one fresh verifier without encoding",
        )?;
        let groups = worker_groups(&after)?;
        let remaining: Vec<_> = process::snapshot()?
            .into_iter()
            .filter(|entry| groups.contains(&entry.group))
            .collect();
        record["retry_remaining_processes"] = json!(remaining);
        require(
            remaining.is_empty(),
            "retry claimed cleanup while a helper group remained",
        )?;
        let movie = evidence::hash(&directory.join("retried.mp4"))?;
        let checkpoint = store.render_checkpoint(&identity.job_id, &original.attempt_id)?;
        require(
            movie["sha256"] == checkpoint.media.movie_sha256().as_str(),
            "published retry bytes differ from retained checkpoint",
        )?;
        record["retry"] = json!({"stages": stages, "status": workflow.status(),
            "attempts": attempts, "movie": movie, "new_encoder_processes": 0, "new_verifier_processes": 1});
        Ok(())
    })();
    if workflow.is_active() {
        let cancellation = workflow.cancel(store, &identity);
        let drained = workflow.drain(store);
        cancellation?;
        drained?;
    }
    result
}

fn identity(stage: &str, retry: bool) -> Result<WorkflowIdentity> {
    let suffix = if retry { "retry" } else { "initial" };
    Ok(WorkflowIdentity {
        job_id: RequestId::new(format!("host-crash-{stage}"))?,
        attempt_id: AttemptId::new(format!("host-crash-{stage}-{suffix}"))?,
        cancellation_token: CancellationToken::new(format!("host-crash-{stage}-{suffix}-cancel"))?,
    })
}

fn publication(directory: &Path, stage: &str, retry: bool) -> Result<PublicationRequest> {
    let suffix = if retry { "retry" } else { "initial" };
    Ok(PublicationRequest {
        destination: directory.join(if retry { "retried.mp4" } else { "crashed.mp4" }),
        publication_id: RequestId::new(format!("host-crash-publish-{stage}-{suffix}"))?,
        operation_id: AttemptId::new(format!("host-crash-publication-{stage}-{suffix}"))?,
        cancellation_token: CancellationToken::new(format!(
            "host-crash-publish-{stage}-{suffix}-cancel"
        ))?,
    })
}

fn config(package: &Path, directory: &Path) -> Result<WorkflowConfig> {
    Ok(WorkflowConfig {
        package: package.to_owned(),
        runtime: RenderWorkerRuntime {
            executable: directory.join(CASE_EXECUTABLE),
            arguments: Vec::new(),
            environment: Default::default(),
        },
        encode_limits: EncodedWorkerLimits {
            encode: deadpan_encode::EncodeLimits {
                maximum_output_bytes: 256 * 1024 * 1024,
                maximum_packets: 100_000,
                ..deadpan_encode::EncodeLimits::default()
            },
            ..EncodedWorkerLimits::default()
        },
        verification_limits: VerificationLimits {
            maximum_bytes: 256 * 1024 * 1024,
            maximum_packets: 100_000,
        },
        media_limits: media_limits()?,
    })
}

fn media_limits() -> Result<RenderMediaLimits> {
    Ok(RenderMediaLimits::new(
        256 * 1024 * 1024,
        256 * 1024,
        257 * 1024 * 1024,
        1024 * 1024 * 1024,
        128,
    )?)
}

fn require(condition: bool, message: &str) -> Result {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_workflow_uses_case_helper_without_runtime_overrides() {
        let directory = Path::new("/private/tmp/deadpan-crash-unlaunched-case");
        let configuration =
            config(Path::new("/private/tmp/unopened.deadpan"), directory).expect("workflow config");
        assert_eq!(
            configuration.runtime.executable,
            directory.join(CASE_EXECUTABLE)
        );
        assert!(configuration.runtime.arguments.is_empty());
        assert!(configuration.runtime.environment.is_empty());
    }
}
