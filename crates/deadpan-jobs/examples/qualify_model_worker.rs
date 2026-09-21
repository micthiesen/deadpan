//! Developer-only execution harness. No runtime download, project edit, or
//! candidate acceptance occurs here. Media validation follows the hashed copy.

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod supported {
    use std::collections::BTreeMap;
    use std::error::Error;
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::{Duration, Instant};

    use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace};
    use deadpan_jobs::supervisor::{ProcessEvent, ProcessLimits, ProcessSpec, WorkerProcess};
    use deadpan_jobs::{
        Diagnostic, HostFailure, HostFailureCode, HostMessage, JobLifecycle, JobState,
        TargetBinding, WorkerMessage, WorkerStage,
    };
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Configuration {
        executable: PathBuf,
        worker_script: PathBuf,
        runtime_config: PathBuf,
        workspace: PathBuf,
        report_directory: PathBuf,
        request: HostMessage,
        cancel_after_millis: Option<u64>,
        cancel_at_stage: Option<WorkerStage>,
    }

    fn create(path: &Path) -> std::io::Result<File> {
        OpenOptions::new().write(true).create_new(true).open(path)
    }

    fn fail(lifecycle: &mut JobLifecycle, code: HostFailureCode, reason: &str) {
        if lifecycle.state().is_terminal() {
            return;
        }
        let detail: String = reason
            .chars()
            .filter(|value| *value != '\0')
            .take(1_000)
            .collect();
        let detail = Diagnostic::new(if detail.is_empty() {
            "worker failed".into()
        } else {
            detail
        })
        .expect("at most 1000 Unicode characters fit the diagnostic budget");
        lifecycle
            .host_failed(&lifecycle.identity().clone(), HostFailure { code, detail })
            .expect("same attempt, nonterminal lifecycle");
    }

    fn reaped(lifecycle: &mut JobLifecycle, clean: bool) {
        if lifecycle.state() == JobState::Cancelling {
            lifecycle
                .host_cancelled(
                    &lifecycle.identity().clone(),
                    &lifecycle.cancellation_token().clone(),
                )
                .expect("same cancelling attempt, worker has been reaped");
        } else if !lifecycle.state().is_terminal()
            && (!clean || lifecycle.state() != JobState::Validating)
        {
            fail(
                lifecycle,
                HostFailureCode::WorkerExited,
                "worker exited without a clean candidate",
            );
        }
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let mut arguments = std::env::args_os().skip(1);
        let path = arguments.next().ok_or("expected host configuration path")?;
        if arguments.next().is_some() {
            return Err("expected exactly one host configuration path".into());
        }
        let mut bytes = Vec::new();
        File::open(path)?.take(1_048_577).read_to_end(&mut bytes)?;
        if bytes.len() > 1_048_576 {
            return Err("host configuration exceeds 1 MiB".into());
        }
        let config: Configuration = serde_json::from_slice(&bytes)?;
        for path in [
            &config.executable,
            &config.worker_script,
            &config.runtime_config,
            &config.workspace,
            &config.report_directory,
        ] {
            if !path.is_absolute() {
                return Err("host paths must be absolute".into());
            }
        }
        let HostMessage::GenerateHold {
            identity,
            cancellation_token,
            project_id,
            target,
            input,
            output_workspace,
            constraints,
            provider,
            ..
        } = &config.request
        else {
            return Err("initial request must generate a Hold".into());
        };
        let mut lifecycle = JobLifecycle::new(
            identity.clone(),
            cancellation_token.clone(),
            TargetBinding {
                project_id: project_id.clone(),
                hold_id: target.hold_id.clone(),
                request_version: target.request_version,
                context_sha256: input.sha256.clone(),
            },
        );
        let workspace = ArtifactWorkspace::open(&config.workspace)?;
        fs::create_dir(&config.report_directory)?;
        let mut events = create(&config.report_directory.join("events.jsonl"))?;
        let environment = [
            ("HF_HUB_OFFLINE", "1"),
            ("TRANSFORMERS_OFFLINE", "1"),
            ("HF_HUB_DISABLE_TELEMETRY", "1"),
            ("DO_NOT_TRACK", "1"),
            ("PYTHONNOUSERSITE", "1"),
            ("PYTHONUNBUFFERED", "1"),
            ("TOKENIZERS_PARALLELISM", "false"),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect::<BTreeMap<_, _>>();
        let mut process = WorkerProcess::spawn(
            ProcessSpec {
                executable: config.executable,
                arguments: vec![
                    "-I".into(),
                    config.worker_script.into_os_string(),
                    "--runtime-config".into(),
                    config.runtime_config.into_os_string(),
                ],
                environment,
                workspace: config.workspace,
                limits: ProcessLimits {
                    maximum_duration: Duration::from_secs(30 * 60),
                    cancellation_grace: Duration::from_secs(5),
                    exit_grace: Duration::from_secs(10),
                },
            },
            config.request.clone(),
        )?;
        let started = Instant::now();
        let mut report_bytes = 0_usize;
        let mut cancellation_sent = false;
        let mut clean_exit = false;
        let mut faults = Vec::new();
        while !process.is_finished() {
            if !cancellation_sent
                && !lifecycle.state().is_terminal()
                && config
                    .cancel_after_millis
                    .is_some_and(|limit| started.elapsed() >= Duration::from_millis(limit))
            {
                lifecycle.request_cancel(identity, cancellation_token)?;
                process.request_cancel(Instant::now())?;
                cancellation_sent = true;
            }
            for event in process.poll(Instant::now())? {
                let recorded = match event {
                    ProcessEvent::Message(message) => {
                        if !lifecycle.state().is_terminal()
                            && let Err(error) = lifecycle.apply_worker_message(&message)
                        {
                            fail(
                                &mut lifecycle,
                                HostFailureCode::ProtocolViolation,
                                &error.to_string(),
                            );
                            process.request_cancel(Instant::now())?;
                        }
                        if let WorkerMessage::Completed { candidate, .. } = message.as_ref()
                            && (candidate.video != constraints.video
                                || &candidate.provider != provider.as_ref())
                        {
                            fail(
                                &mut lifecycle,
                                HostFailureCode::OutputValidationFailed,
                                "candidate does not match the requested video/provider",
                            );
                        }
                        if !cancellation_sent
                            && !lifecycle.state().is_terminal()
                            && let WorkerMessage::Stage { stage, .. } = message.as_ref()
                            && Some(*stage) == config.cancel_at_stage
                        {
                            lifecycle.request_cancel(identity, cancellation_token)?;
                            process.request_cancel(Instant::now())?;
                            cancellation_sent = true;
                        }
                        json!({"seconds":started.elapsed().as_secs_f64(),"message":message})
                    }
                    ProcessEvent::Fault(reason) => {
                        fail(&mut lifecycle, HostFailureCode::WorkerExited, &reason);
                        if faults.len() < 16 {
                            faults.push(reason.clone());
                        }
                        json!({"seconds":started.elapsed().as_secs_f64(),"fault":reason})
                    }
                    ProcessEvent::Exited {
                        status,
                        cancellation_escalated,
                    } => {
                        clean_exit = status.success() && !cancellation_escalated;
                        reaped(&mut lifecycle, clean_exit);
                        json!({"seconds":started.elapsed().as_secs_f64(),"exit":status.to_string(),"cancellation_escalated":cancellation_escalated})
                    }
                };
                let mut line = serde_json::to_vec(&recorded)?;
                line.push(b'\n');
                report_bytes = report_bytes
                    .checked_add(line.len())
                    .ok_or("event log overflow")?;
                if report_bytes > 4 * 1024 * 1024 {
                    return Err("qualification event log exceeded 4 MiB".into());
                }
                events.write_all(&line)?;
            }
            thread::park_timeout(Duration::from_millis(10));
        }
        let logs = process.log_tail();
        create(&config.report_directory.join("worker.stderr.log"))?.write_all(&logs.bytes)?;
        let mut result = json!({
            "elapsed_seconds":started.elapsed().as_secs_f64(),
            "clean_exit":clean_exit,
            "cancellation_sent":cancellation_sent,
            "state":format!("{:?}",lifecycle.state()),
            "failure":lifecycle.failure().map(|value| format!("{value:?}")),
            "faults":faults,
            "discarded_log_bytes":logs.discarded_bytes,
            "media_validated":false,
            "accepted":false,
        });
        if clean_exit && faults.is_empty() && lifecycle.state() == JobState::Validating {
            let captured: Result<serde_json::Value, Box<dyn Error>> = (|| {
                let candidate = lifecycle.candidate().ok_or("missing candidate")?;
                let mut snapshot = workspace.snapshot(
                    output_workspace,
                    &candidate.media,
                    ArtifactLimits::new(512 * 1024 * 1024)?,
                )?;
                let mut output = create(&config.report_directory.join("candidate.snapshot.mp4"))?;
                std::io::copy(&mut snapshot, &mut output)?;
                output.sync_all()?;
                Ok(serde_json::to_value(candidate)?)
            })();
            match captured {
                Ok(candidate) => {
                    result["candidate"] = candidate;
                    result["hash_verified_snapshot"] = json!(true);
                }
                Err(error) => fail(
                    &mut lifecycle,
                    HostFailureCode::OutputValidationFailed,
                    &error.to_string(),
                ),
            }
        }
        result["state"] = json!(format!("{:?}", lifecycle.state()));
        result["failure"] = json!(lifecycle.failure().map(|value| format!("{value:?}")));
        let successful = result["hash_verified_snapshot"] == true
            || (cancellation_sent
                && clean_exit
                && faults.is_empty()
                && lifecycle.state() == JobState::Cancelled);
        serde_json::to_writer_pretty(
            create(&config.report_directory.join("host-report.json"))?,
            &result,
        )?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        if !successful {
            return Err(
                "worker did not produce a clean hashed candidate or cooperative cancellation"
                    .into(),
            );
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use deadpan_core::{NodeId, ProjectId};
        use deadpan_jobs::{
            AttemptId, CancellationToken, MessageIdentity, RequestId, RequestVersion, Sha256,
        };

        fn job() -> JobLifecycle {
            JobLifecycle::new(
                MessageIdentity::new(
                    RequestId::new("request").unwrap(),
                    AttemptId::new("attempt").unwrap(),
                ),
                CancellationToken::new("token").unwrap(),
                TargetBinding {
                    project_id: ProjectId::new("project").unwrap(),
                    hold_id: NodeId::new("hold").unwrap(),
                    request_version: RequestVersion::new(1).unwrap(),
                    context_sha256: Sha256::new("a".repeat(64)).unwrap(),
                },
            )
        }

        #[test]
        fn reaping_finishes_cancel_when_completion_was_suppressed() {
            for clean in [true, false] {
                let mut job = job();
                job.request_cancel(&job.identity().clone(), &job.cancellation_token().clone())
                    .unwrap();
                reaped(&mut job, clean);
                assert_eq!(job.state(), JobState::Cancelled);
                assert!(job.candidate().is_none());
            }
        }

        #[test]
        fn supervisor_faults_fail_and_keep_the_first_terminal_reason() {
            let mut job = job();
            fail(
                &mut job,
                HostFailureCode::ProtocolViolation,
                "malformed worker payload",
            );
            let failure = job.failure().cloned();
            fail(
                &mut job,
                HostFailureCode::WorkerExited,
                "later process exit",
            );
            reaped(&mut job, false);
            assert_eq!(job.state(), JobState::Failed);
            assert_eq!(job.failure(), failure.as_ref());
        }

        #[test]
        fn clean_exit_without_candidate_is_not_success() {
            let mut job = job();
            reaped(&mut job, true);
            assert_eq!(job.state(), JobState::Failed);
        }
    }
}

fn main() {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Err(error) = supported::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    compile_error!("model qualification requires macOS or Linux");
}
