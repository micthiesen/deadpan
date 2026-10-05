use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt},
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        jobs::{self, RenderStageRequest},
        publication::{
            PublicationOutcome,
            journal::{self, RecoveryOutcome},
        },
        verification::{VerificationLimits, VerifiedCandidate},
    },
    render_worker::RenderWorkerRuntime,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        RenderDiagnostic,
        publication::{
            PublicationCompletion, PublicationIntent, PublicationOutcome as StoredOutcome,
            PublicationPhase, PublicationReconciliation, StoredPublication,
        },
    },
};
use deadpan_store::{
    AccessMode, ProjectStore,
    render_jobs::{BeginRenderAttempt, RenderAttemptTransition},
    render_media::RenderMediaLimits,
};
use serde_json::{Value, json};

#[path = "evidence.rs"]
mod evidence;
use evidence::{artifacts, authored, check, save, save_new};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const MAX_PROGRESS: usize = 4096;
const CHILD_WAIT: Duration = Duration::from_secs(240);
const STAGE_DEADLINE: Duration = Duration::from_secs(120);

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    point: &'static str,
    variant: Option<&'static str>,
    expected: &'static str,
}
const CASES: [Case; 10] = [
    Case {
        name: "intent",
        point: "intent",
        variant: None,
        expected: "not_published",
    },
    Case {
        name: "prepared",
        point: "prepared",
        variant: None,
        expected: "not_published",
    },
    Case {
        name: "report-authorized",
        point: "report-authorized",
        variant: None,
        expected: "not_published",
    },
    Case {
        name: "report-renamed",
        point: "report-renamed-before-record",
        variant: None,
        expected: "not_published",
    },
    Case {
        name: "report-recorded",
        point: "report-recorded",
        variant: None,
        expected: "not_published",
    },
    Case {
        name: "movie-authorized",
        point: "movie-authorized",
        variant: None,
        expected: "unresolved",
    },
    Case {
        name: "movie-renamed",
        point: "movie-renamed-before-terminal",
        variant: None,
        expected: "published",
    },
    Case {
        name: "terminal",
        point: "terminal",
        variant: None,
        expected: "published",
    },
    Case {
        name: "missing-report",
        point: "movie-renamed-before-terminal",
        variant: Some("missing-report"),
        expected: "published_unconfirmed",
    },
    Case {
        name: "identical-replacement",
        point: "movie-renamed-before-terminal",
        variant: Some("identical-replacement"),
        expected: "unresolved",
    },
];

struct Inputs {
    package: PathBuf,
    job: RequestId,
    checkpoint: AttemptId,
    worker: PathBuf,
    output: PathBuf,
}
impl Inputs {
    fn parse(arguments: &[OsString]) -> Result<Self> {
        if arguments.len() != 5 {
            return Err(
                "expected PACKAGE JOB_ID ENCODING_ATTEMPT WORKER NEW_OUTPUT_DIRECTORY".into(),
            );
        }
        let package = fs::canonicalize(&arguments[0])?;
        let scratch = fs::canonicalize("/tmp")?;
        if !package.starts_with(&scratch) {
            return Err(
                "qualification requires a caller-prepared scratch package under /tmp".into(),
            );
        }
        let output = std::path::absolute(&arguments[4])?;
        let parent = output
            .parent()
            .ok_or("output has no parent")?
            .canonicalize()?;
        let output = parent.join(output.file_name().ok_or("output has no basename")?);
        if !output.starts_with(&scratch)
            || output.starts_with(&package)
            || package.starts_with(&output)
        {
            return Err("output must be separate scratch space under /tmp".into());
        }
        Ok(Self {
            package,
            job: RequestId::new(arguments[1].to_str().ok_or("non-UTF8 job id")?)?,
            checkpoint: AttemptId::new(arguments[2].to_str().ok_or("non-UTF8 encoding attempt")?)?,
            worker: fs::canonicalize(&arguments[3])?,
            output,
        })
    }
    fn runtime(&self) -> RenderWorkerRuntime {
        RenderWorkerRuntime {
            executable: self.worker.clone(),
            arguments: Vec::new(),
            environment: BTreeMap::new(),
        }
    }
}

pub(super) fn run() -> Result {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|value| value == "--child") {
        if arguments.len() != 7 {
            return Err("invalid child qualification arguments".into());
        }
        let inputs = Inputs::parse(&arguments[1..6])?;
        let case = CASES
            .iter()
            .copied()
            .find(|case| arguments[6] == case.name)
            .ok_or("unknown crash case")?;
        return child(&inputs, case);
    }
    let inputs = Inputs::parse(&arguments)?;
    fs::create_dir(&inputs.output)?;
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(inputs.output.join("report.json"))?;
    let started = Instant::now();
    let mut report = json!({"schema_version": 1, "status": "running", "package": inputs.package,
        "job_id": inputs.job, "encoding_attempt_id": inputs.checkpoint, "worker": inputs.worker,
        "scope": "real SIGKILL at eight host stage boundaries; retained checkpoint freshly verified for every publication and reconciliation; no new encoding",
        "limitations": ["SIGKILL occurs after completed host calls, never inside rename/fsync/SQLite or power loss", "APFS and explicit engineering encoder policy only", "child helper processes finish before the pause signal; process groups do not contain escaped processes", "independent emitted-media content checks are a separate parent-owned qualification", "no automatic retry, native Render UI, full effects/audio, HDR or release qualification"],
        "cases": []});
    save(&mut file, &report)?;
    let result = (|| -> Result {
        let before = authored(&inputs.package)?;
        save_new(&inputs.output.join("authored-before.json"), &before)?;
        let migration = ProjectStore::migrate(&inputs.package)?;
        if migration.from_schema != deadpan_store::DATABASE_SCHEMA_VERSION
            || migration.to_schema != deadpan_store::DATABASE_SCHEMA_VERSION
            || migration.backup.is_some()
        {
            return Err(
                "publication recovery qualification requires a current-format project".into(),
            );
        }
        report["migration"] = json!(migration);
        if authored(&inputs.package)? != before {
            return Err("current-format validation changed authored or history cells".into());
        }
        let initial = ProjectStore::open(&inputs.package, AccessMode::ReadOnly)?;
        let intent = initial.render_job(&inputs.job)?;
        let checkpoint = initial.render_checkpoint(&inputs.job, &inputs.checkpoint)?;
        report["render_intent"] = json!(intent);
        report["retained_checkpoint"] = json!(checkpoint);
        report["captured_document"] =
            serde_json::to_value(initial.snapshot_at(&intent.revision_id)?)?;
        report["initial_authoring"] = before.summary();
        drop(initial);
        for case in CASES {
            let mut record = json!({"name": case.name, "crash_point": case.point,
                "variant": case.variant, "expected": case.expected, "status": "running", "checks": []});
            let outcome = run_case(&inputs, case, &mut record);
            record["status"] = json!(if outcome.is_ok() { "passed" } else { "failed" });
            if let Err(error) = &outcome {
                record["error"] = json!(error.to_string());
            }
            save_new(
                &inputs.output.join(format!("case-{}.json", case.name)),
                &record,
            )?;
            report["cases"]
                .as_array_mut()
                .ok_or("missing cases")?
                .push(record);
            report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
            save(&mut file, &report)?;
            outcome?;
        }
        let after = authored(&inputs.package)?;
        save_new(&inputs.output.join("authored-after.json"), &after)?;
        if before != after {
            return Err("publication qualification changed exact authored/history cells".into());
        }
        report["final_authoring"] = after.summary();
        report["exact_authoring_preserved"] = json!(true);
        report["process_crashes"] = json!(CASES.len());
        report["fresh_verifications"] = json!(CASES.len() * 2);
        report["new_encodes"] = json!(0);
        report["observed_movie_commits_before_crash"] = json!(4);
        report["classifications"] = json!({"not_published": 5, "unresolved": 2, "published": 2, "published_unconfirmed": 1});
        Ok(())
    })();
    report["status"] = json!(if result.is_ok() { "passed" } else { "failed" });
    report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    if let Err(error) = &result {
        report["error"] = json!(error.to_string());
    }
    save(&mut file, &report)?;
    result
}

fn fresh_verification(
    inputs: &Inputs,
    store: &mut ProjectStore,
    name: &str,
    report: &mut Value,
) -> Result<(VerifiedCandidate, AttemptId)> {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + STAGE_DEADLINE;
    let intent = store.render_job(&inputs.job)?;
    let checkpoint = store.render_checkpoint(&inputs.job, &inputs.checkpoint)?;
    let attempt_id = AttemptId::new(format!("publication-{name}"))?;
    let queued = store.begin_render_attempt(BeginRenderAttempt {
        job_id: inputs.job.clone(),
        attempt_id: attempt_id.clone(),
        cancellation_token: CancellationToken::new(format!("publication-cancel-{name}"))?,
        checkpoint_attempt_id: Some(inputs.checkpoint.clone()),
    })?;
    let verifying =
        store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Verifying)?;
    let reader = store.render_read_handle();
    let limits = RenderMediaLimits::new(
        512 * 1024 * 1024,
        256 * 1024,
        512 * 1024 * 1024 + 256 * 1024,
        1024 * 1024 * 1024,
        128,
    )?;
    let mut progress = Vec::new();
    let verified = jobs::verify_checkpoint(
        &inputs.runtime(),
        &RenderStageRequest {
            package: inputs.package.clone(),
            intent,
            attempt: verifying.clone(),
        },
        (&checkpoint, &reader),
        (VerificationLimits::default(), limits),
        &cancelled,
        deadline,
        |update| {
            if progress.len() == MAX_PROGRESS {
                cancelled.store(true, Ordering::Release);
            } else {
                progress.push(update);
            }
        },
    )?;
    let observation = jobs::verification_observation(&verified, &cancelled, deadline)?;
    let terminal = store.record_render_verification(&verifying.identity(), observation)?;
    report["verification_progress"] = json!(progress);
    report["verification"] = json!(verified.report());
    report["verification_attempt"] = json!(terminal);
    report["manifest"] = json!(verified.candidate().manifest());
    Ok((verified, attempt_id))
}

fn publication_id(case: Case) -> Result<RequestId> {
    Ok(RequestId::new(format!("crash-{}", case.name))?)
}

fn child(inputs: &Inputs, case: Case) -> Result {
    let before = authored(&inputs.package)?;
    let mut report = json!({"case": case.name, "point": case.point, "pid": std::process::id(), "authoring_before": before.summary()});
    let mut store = ProjectStore::open(&inputs.package, AccessMode::ReadWrite)?;
    let (verified, verified_attempt_id) = fresh_verification(
        inputs,
        &mut store,
        &format!("{}-child", case.name),
        &mut report,
    )?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + STAGE_DEADLINE;
    let mut permit = store.begin_render_publication(
        PublicationIntent {
            schema_version: 1,
            publication_id: publication_id(case)?,
            job_id: inputs.job.clone(),
            verified_attempt_id,
            destination: inputs.output.join("movie.mp4"),
        },
        AttemptId::new(format!("publish-{}", case.name))?,
        CancellationToken::new(format!("publish-cancel-{}", case.name))?,
    )?;
    pause_at(
        inputs,
        case,
        "intent",
        permit.record(),
        &before,
        &mut report,
    )?;
    let mut prepared = journal::prepare(
        verified,
        &inputs.package,
        &permit,
        &cancelled,
        deadline,
        |_| {},
    )?;
    permit = store.record_prepared_publication(&permit.identity(), prepared.evidence().clone())?;
    pause_at(
        inputs,
        case,
        "prepared",
        permit.record(),
        &before,
        &mut report,
    )?;
    permit = store.advance_publication(&permit.identity(), PublicationPhase::ReportCommitting)?;
    pause_at(
        inputs,
        case,
        "report-authorized",
        permit.record(),
        &before,
        &mut report,
    )?;
    prepared.commit_report(&permit, &cancelled, deadline)?;
    pause_at(
        inputs,
        case,
        "report-renamed-before-record",
        permit.record(),
        &before,
        &mut report,
    )?;
    permit = store.advance_publication(&permit.identity(), PublicationPhase::ReportCommitted)?;
    pause_at(
        inputs,
        case,
        "report-recorded",
        permit.record(),
        &before,
        &mut report,
    )?;
    permit = store.advance_publication(&permit.identity(), PublicationPhase::MovieCommitting)?;
    pause_at(
        inputs,
        case,
        "movie-authorized",
        permit.record(),
        &before,
        &mut report,
    )?;
    let published = prepared.commit_movie(&permit, &cancelled, deadline)?;
    match published {
        PublicationOutcome::Published(receipt) => report["publication_receipt"] = json!(receipt),
        PublicationOutcome::PublishedUnconfirmed {
            receipt,
            diagnostic,
        } => {
            report["publication_receipt"] = json!(receipt);
            report["publication_error"] = json!(diagnostic);
            save_new(&inputs.output.join("child-failed.json"), &report)?;
            return Err(
                "movie commit did not complete confirmation before crash checkpoint".into(),
            );
        }
    }
    pause_at(
        inputs,
        case,
        "movie-renamed-before-terminal",
        permit.record(),
        &before,
        &mut report,
    )?;
    let terminal =
        store.finish_publication(&permit.identity(), PublicationCompletion::Published)?;
    pause_at(inputs, case, "terminal", &terminal, &before, &mut report)?;
    Err("child reached end without the selected crash checkpoint".into())
}

fn pause_at(
    inputs: &Inputs,
    case: Case,
    point: &str,
    record: &StoredPublication,
    before: &evidence::Authoring,
    report: &mut Value,
) -> Result {
    if case.point != point {
        return Ok(());
    }
    let after = authored(&inputs.package)?;
    if before != &after {
        return Err("child changed authored/history cells".into());
    }
    report["paused_after"] = json!(point);
    report["record"] = json!(record);
    report["authoring_at_pause"] = after.summary();
    report["artifacts"] = artifacts(&inputs.output)?;
    save_new(&inputs.output.join("child-state.json"), report)?;
    let mut ready = File::options()
        .write(true)
        .create_new(true)
        .open(inputs.output.join("pause-ready"))?;
    ready.write_all(b"ready\n")?;
    ready.sync_all()?;
    // The parent retains stdin only to keep this syscall blocked. Its checked
    // SIGKILL kills this real writer; no destructor or journal finish can run.
    let mut input = [0; 1];
    std::io::stdin().read_exact(&mut input)?;
    Err("qualification pause unexpectedly resumed instead of receiving SIGKILL".into())
}

fn run_case(inputs: &Inputs, case: Case, report: &mut Value) -> Result {
    let output = inputs.output.join(case.name);
    fs::create_dir(&output)?;
    let before = authored(&inputs.package)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--child")
        .arg(&inputs.package)
        .arg(inputs.job.as_str())
        .arg(inputs.checkpoint.as_str())
        .arg(&inputs.worker)
        .arg(&output)
        .arg(case.name)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(
            File::options()
                .write(true)
                .create_new(true)
                .open(output.join("child.stdout"))?,
        )
        .stderr(
            File::options()
                .write(true)
                .create_new(true)
                .open(output.join("child.stderr"))?,
        );
    let mut child = OwnedChild {
        child: deadpan_native_process::spawn(&mut command)?,
        cleanup_attempted: false,
        reap_attempted: false,
    };
    let pid = child.child.id();
    let ready = wait_for_pause(&child.child, &output);
    let exit = child.kill_and_reap();
    report["child_pid"] = json!(pid);
    if let Ok(status) = &exit {
        report["child_exit"] = json!({"code": status.code(), "signal": status.signal()});
    }
    if let Err(error) = &exit {
        report["cleanup_error"] = json!(error.to_string());
    }
    if let Err(error) = &ready {
        report["pause_error"] = json!(error.to_string());
    }
    // Record both failures even if the pause observation failed first.
    ready?;
    let status = exit?;
    check(
        report,
        "the selected publication process was killed by SIGKILL and reaped",
        status.signal() == Some(9),
        json!({"signal": status.signal()}),
    )?;
    let child_report = evidence::read_json(&output.join("child-state.json"))?;
    let prior: StoredPublication = serde_json::from_value(child_report["record"].clone())?;
    report["child"] = child_report;
    let readonly = ProjectStore::open(&inputs.package, AccessMode::ReadOnly)?;
    let readonly_record = readonly.render_publication(&publication_id(case)?)?;
    check(
        report,
        "read-only reopen preserves the exact pre-crash operation",
        readonly_record == prior,
        json!(readonly_record),
    )?;
    drop(readonly);
    let mut store = ProjectStore::open(&inputs.package, AccessMode::ReadWrite)?;
    let interrupted = store.render_publication(&publication_id(case)?)?;
    let expected_outcome = if case.point == "terminal" {
        StoredOutcome::Published
    } else {
        StoredOutcome::Interrupted
    };
    check(
        report,
        "writer reopen interrupts active publication and preserves a terminal commit",
        interrupted.outcome == expected_outcome
            && !interrupted.operation.active
            && interrupted.phase == prior.phase
            && interrupted.prepared == prior.prepared
            && interrupted.intent == prior.intent,
        json!(interrupted),
    )?;
    check(
        report,
        "SIGKILL and reopen preserve exact authored/history cells",
        authored(&inputs.package)? == before,
        before.summary(),
    )?;
    apply_variant(&output, case, &interrupted, report)?;
    let before_artifacts = artifacts(&output)?;
    report["artifacts_before_reconcile"] = before_artifacts.clone();
    let (verified, verify_id) = fresh_verification(
        inputs,
        &mut store,
        &format!("{}-recovery", case.name),
        report,
    )?;
    let permit = store.begin_publication_reconciliation(
        &publication_id(case)?,
        verify_id,
        AttemptId::new(format!("reconcile-{}", case.name))?,
        CancellationToken::new(format!("reconcile-cancel-{}", case.name))?,
    )?;
    let cancelled = AtomicBool::new(false);
    let (classification, terminal) = match journal::reconcile(
        &verified,
        &permit,
        &cancelled,
        Instant::now() + STAGE_DEADLINE,
    ) {
        Ok(inspection) => {
            let classification = match inspection.outcome() {
                RecoveryOutcome::NotPublished => "not_published",
                RecoveryOutcome::Committed(PublicationOutcome::Published(receipt)) => {
                    report["recovered_receipt"] = json!(receipt);
                    "published"
                }
                RecoveryOutcome::Committed(PublicationOutcome::PublishedUnconfirmed {
                    receipt,
                    diagnostic,
                }) => {
                    report["recovered_receipt"] = json!(receipt);
                    report["recovery_diagnostic"] = json!(diagnostic);
                    "published_unconfirmed"
                }
            };
            let terminal = store.finish_publication_reconciliation(
                inspection.identity(),
                inspection.completion(),
            )?;
            drop(inspection); // Keep recovered file locks through the durable record.
            (classification, terminal)
        }
        Err(error) => {
            report["recovery_diagnostic"] = json!(error);
            let terminal = store.finish_publication_reconciliation(
                &permit.identity(),
                PublicationReconciliation::Unresolved(RenderDiagnostic {
                    code: error.code,
                    detail: error.message,
                }),
            )?;
            ("unresolved", terminal)
        }
    };
    report["classification"] = json!(classification);
    report["terminal"] = json!(terminal);
    check(
        report,
        "reconciliation reports the expected commit knowledge",
        classification == case.expected,
        json!({"expected": case.expected, "observed": classification}),
    )?;
    let after_artifacts = artifacts(&output)?;
    check(
        report,
        "reconciliation preserves every retained file, identity and byte",
        before_artifacts == after_artifacts,
        after_artifacts.clone(),
    )?;
    report["artifacts_after_reconcile"] = after_artifacts;
    let operations = store.publication_operations(&publication_id(case)?, 0, 3)?;
    check(
        report,
        "one crashed publication and one explicit reconciliation are retained",
        operations.len() == 2,
        json!(operations),
    )?;
    check(
        report,
        "fresh verification and reconciliation preserve exact authoring",
        authored(&inputs.package)? == before,
        before.summary(),
    )?;
    store.validate_full()?;
    drop(verified);
    drop(store);
    let reopened = ProjectStore::open(&inputs.package, AccessMode::ReadWrite)?;
    check(
        report,
        "terminal publication survives another writer restart",
        reopened.render_publication(&publication_id(case)?)? == terminal,
        json!(terminal),
    )?;
    Ok(())
}

fn apply_variant(
    directory: &Path,
    case: Case,
    record: &StoredPublication,
    report: &mut Value,
) -> Result {
    match case.variant {
        None => (),
        Some("missing-report") => {
            let original = directory.join(record.intent.report_name());
            let displaced = directory.join("displaced-report.json");
            fs::rename(&original, &displaced)?;
            report["fault_injection"] = json!({"kind": "report moved aside after SIGKILL", "original": original, "preserved": displaced});
        }
        Some("identical-replacement") => {
            let original = directory.join("movie.mp4");
            let displaced = directory.join("original-movie.mp4");
            fs::rename(&original, &displaced)?;
            let mut source = File::open(&displaced)?;
            let mut replacement = File::options()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&original)?;
            let copied = std::io::copy(
                &mut Read::by_ref(&mut source).take(
                    record
                        .movie_bytes
                        .checked_add(1)
                        .ok_or("replacement byte bound overflow")?,
                ),
                &mut replacement,
            )?;
            if copied != record.movie_bytes {
                return Err("replacement source extent differs from the retained movie".into());
            }
            replacement.set_modified(source.metadata()?.modified()?)?;
            replacement.sync_all()?;
            if source.metadata()?.ino() == replacement.metadata()?.ino() {
                return Err("replacement did not get a distinct inode".into());
            }
            let observed = artifacts(directory)?;
            if observed["movie.mp4"]["sha256"] != observed["original-movie.mp4"]["sha256"] {
                return Err("replacement bytes differ".into());
            }
            report["fault_injection"] = json!({"kind": "byte-identical movie replacement with restored mtime after SIGKILL", "original": original, "preserved": displaced, "artifacts": observed});
        }
        Some(_) => return Err("unknown qualification fault".into()),
    }
    Ok(())
}

fn wait_for_pause(child: &Child, directory: &Path) -> Result {
    let end = Instant::now() + CHILD_WAIT;
    loop {
        if directory.join("pause-ready").try_exists()? {
            return Ok(());
        }
        for name in ["child.stdout", "child.stderr"] {
            if fs::metadata(directory.join(name))?.len() > 8 * 1024 * 1024 {
                return Err("child output bound exceeded".into());
            }
        }
        use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
        if waitid(
            WaitId::Pid(Pid::from_child(child)),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )?
        .is_some()
        {
            return Err(
                "qualification child exited before selected pause; inspect child.stderr".into(),
            );
        }
        if Instant::now() >= end {
            return Err("qualification child pause deadline elapsed".into());
        }
        std::thread::park_timeout(Duration::from_millis(20));
    }
}

struct OwnedChild {
    child: Child,
    cleanup_attempted: bool,
    reap_attempted: bool,
}
impl OwnedChild {
    fn kill_and_reap(&mut self) -> Result<ExitStatus> {
        if self.cleanup_attempted || self.reap_attempted {
            return Err("child cleanup or reaping was already attempted".into());
        }
        self.cleanup_attempted = true;
        let group = deadpan_native_process::terminate_owned_group(
            &self.child,
            Instant::now() + Duration::from_secs(5),
        );
        if let Err(group_error) = &group {
            // Never wait indefinitely for a leader whose exit was not proved.
            // A failed checked fallback remains a qualification failure; Drop
            // cannot retry a numeric PID or claim descendant cleanup succeeded.
            if let Err(leader_error) = deadpan_native_process::terminate_owned_leader(
                &self.child,
                Instant::now() + Duration::from_secs(5),
            ) {
                return Err(format!(
                    "owned child cleanup failed: group={group_error}; leader={leader_error}"
                )
                .into());
            }
        }
        // A successful group cleanup or checked leader fallback proved exit.
        // Mark the sole reap attempt before wait, including its error path.
        self.reap_attempted = true;
        let status = self.child.wait();
        group?;
        Ok(status?)
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.cleanup_attempted && !self.reap_attempted {
            let _ = self.kill_and_reap();
        }
    }
}
