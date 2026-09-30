//! Reinspect retained actual encoder files with the production verification child.
//! Usage: qualify_encoded_verification ENCODE_REPORT WORKER REPORT NEW_DIRECTORY
//! This harness reads the recorded manifests; it does not grant publication authority.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        protocol::EncodedManifest,
        verification::{
            PRIVATE_WORKER_ARGUMENT, VerificationLimits,
            protocol::{HostMessage, VerificationProtocol, WorkerMessage},
        },
    },
    render_worker::protocol::RenderIdentity,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess},
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 4 {
        return Err("expected ENCODE_REPORT WORKER REPORT NEW_DIRECTORY".into());
    }
    let retained: Value = serde_json::from_slice(&fs::read(&arguments[0])?)?;
    let worker = fs::canonicalize(&arguments[1])?;
    let report_path = PathBuf::from(&arguments[2]);
    let directory = PathBuf::from(&arguments[3]);
    fs::create_dir(&directory)?;
    let mut report = json!({"scope": "production verifier over retained actual encoded files; no encoding or publication", "cases": [], "status": "running"});
    let mut environment = BTreeMap::new();
    // Libraries and sanitizer runtimes use the captured executable's link paths.
    // Do not introduce an ambient loader override into the inspected runtime.
    for key in ["ASAN_OPTIONS", "UBSAN_OPTIONS"] {
        if let Some(value) = std::env::var_os(key) {
            environment.insert(key.into(), value);
        }
    }
    let cases = retained["encoded"]["cases"]
        .as_array()
        .filter(|cases| !cases.is_empty())
        .ok_or("missing or empty encoded cases")?;
    for case in cases {
        let name = case["name"].as_str().ok_or("missing case name")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("unsafe case name".into());
        }
        let manifest: EncodedManifest = serde_json::from_value(case["manifest"].clone())?;
        let workspace = directory.join(name);
        fs::create_dir(&workspace)?;
        fs::create_dir(workspace.join("input"))?;
        fs::copy(
            case["path"].as_str().ok_or("missing retained file")?,
            workspace.join("input/movie.mp4"),
        )?;
        let identity = RenderIdentity {
            request_id: RequestId::new(format!("verify-{name}"))?,
            attempt_id: AttemptId::new("actual-1")?,
        };
        let request = HostMessage::Inspect {
            protocol: 1,
            identity,
            cancellation_token: CancellationToken::new(format!("cancel-{name}"))?,
            manifest: Box::new(manifest),
            limits: VerificationLimits::default(),
            timeout_millis: 120_000,
        };
        let started = Instant::now();
        let mut process = SupervisedProcess::<VerificationProtocol>::spawn(
            ProcessSpec {
                executable: worker.clone(),
                arguments: vec![PRIVATE_WORKER_ARGUMENT.into()],
                environment: environment.clone(),
                workspace,
                limits: ProcessLimits {
                    maximum_duration: Duration::from_secs(120),
                    cancellation_grace: Duration::from_millis(500),
                    exit_grace: Duration::from_secs(2),
                },
            },
            request,
        )?;
        let mut progress = Vec::new();
        let mut result = None;
        let mut failure = None;
        let mut clean_exit = false;
        while !process.is_finished() {
            match process.poll(Instant::now()) {
                Ok(events) => {
                    for event in events {
                        match event {
                            ProcessEvent::Message(message) => match *message {
                                WorkerMessage::Progress {
                                    progress: value, ..
                                } => progress.push(value),
                                WorkerMessage::Completed { report, .. } => result = Some(report),
                                WorkerMessage::Failed { diagnostic, .. } => {
                                    failure = Some(format!("{diagnostic:?}"))
                                }
                                WorkerMessage::Cancelled { .. } => {
                                    failure = Some("cancelled".into())
                                }
                            },
                            ProcessEvent::Fault(reason) => {
                                failure.get_or_insert(reason);
                            }
                            ProcessEvent::Exited {
                                status,
                                cancellation_escalated,
                            } => {
                                clean_exit = status.success() && !cancellation_escalated;
                                if !clean_exit {
                                    failure.get_or_insert_with(|| format!("worker exit {status}, escalated={cancellation_escalated}"));
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    failure.get_or_insert_with(|| error.to_string());
                    break;
                }
            }
            if !process.is_finished() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let logs = process.log_tail();
        if !clean_exit {
            failure.get_or_insert_with(|| "missing clean process exit".into());
        }
        report["cases"].as_array_mut().unwrap().push(json!({"name": name, "report": result, "failure": failure,
            "progress": progress, "seconds": started.elapsed().as_secs_f64(), "clean_exit": clean_exit,
            "stderr": String::from_utf8_lossy(&logs.bytes), "discarded_stderr_bytes": logs.discarded_bytes}));
        if failure.is_some() || result.is_none() {
            report["status"] = json!("failed");
        }
        fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
        if failure.is_some() || result.is_none() {
            return Err(format!("verification failed for {name}: {failure:?}").into());
        }
    }
    report["status"] = json!("passed");
    fs::write(report_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
