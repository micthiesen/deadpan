//! Real executable protocol, ownership revocation and artifact refusals.
//! Valid model cases are offline opt-ins; default refusal cases need no model.
#![cfg(target_os = "macos")]

use deadpan_jobs::{
    process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess, WorkerProtocol},
    protocol::{AttemptId, CancellationToken, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef},
    refinement::{self, HostMessage, ModelInput, RefinementProtocol, WorkerMessage},
};
use rustix::{
    fs::OFlags,
    process::{Pid, WaitId, WaitIdOptions, waitid},
};
use sha2::{Digest, Sha256 as Hasher};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Write},
    os::unix::{fs::symlink, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const WALL_LIMIT: Duration = Duration::from_secs(30);

fn digest(bytes: &[u8]) -> Sha256 {
    Sha256::new(
        Hasher::digest(bytes)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>(),
    )
    .unwrap()
}

struct Fixture {
    directory: tempfile::TempDir,
    checkpoint: PathBuf,
    statistics: PathBuf,
    audio: WorkspaceArtifact,
}

impl Fixture {
    fn new(samples: usize, models: Option<&(PathBuf, PathBuf)>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("input")).unwrap();
        fs::create_dir(directory.path().join("output")).unwrap();
        let bytes: Vec<u8> = (0..samples)
            .flat_map(|i| ((i as f32 * 0.07).sin() * 0.1).to_le_bytes())
            .collect();
        let audio = WorkspaceArtifact::new(
            WorkspaceRef::new("input/audio.f32le").unwrap(),
            digest(&bytes),
            bytes.len() as u64,
        )
        .unwrap();
        fs::write(directory.path().join("input/audio.f32le"), bytes).unwrap();
        let (checkpoint, statistics) = if let Some((checkpoint, statistics)) = models {
            (
                fs::canonicalize(checkpoint).unwrap(),
                fs::canonicalize(statistics).unwrap(),
            )
        } else {
            let checkpoint = directory.path().join("model.ckpt");
            File::create(&checkpoint)
                .unwrap()
                .set_len(refinement::CHECKPOINT_BYTES)
                .unwrap();
            let statistics = directory.path().join("global_stats.json");
            fs::write(
                &statistics,
                vec![0_u8; refinement::STATISTICS_BYTES as usize],
            )
            .unwrap();
            (checkpoint, statistics)
        };
        Self {
            directory,
            checkpoint,
            statistics,
            audio,
        }
    }
    fn path(&self) -> &Path {
        self.directory.path()
    }
    fn infer(&self) -> HostMessage {
        HostMessage::Infer {
            protocol: refinement::VERSION,
            request: RequestId::new("worker-request").unwrap(),
            attempt: AttemptId::new("worker-attempt").unwrap(),
            cancellation_token: CancellationToken::new("worker-cancel").unwrap(),
            checkpoint: ModelInput {
                path: self.checkpoint.clone(),
                sha256: Sha256::new(refinement::CHECKPOINT_SHA256).unwrap(),
                byte_length: refinement::CHECKPOINT_BYTES,
            }
            .into(),
            statistics: ModelInput {
                path: self.statistics.clone(),
                sha256: Sha256::new(refinement::STATISTICS_SHA256).unwrap(),
                byte_length: refinement::STATISTICS_BYTES,
            }
            .into(),
            audio: self.audio.clone(),
            output_scope: WorkspaceRef::new("output").unwrap(),
            max_output_bytes: refinement::MAX_OUTPUT_BYTES,
            timeout_ms: 20_000,
        }
    }
    fn assert_no_output(&self, messages: &[WorkerMessage]) {
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, WorkerMessage::Completed { .. })),
            "{messages:?}"
        );
        assert!(
            !self.path().join("output/emissions.f32le").exists(),
            "unexpected emission artifact"
        );
    }
}

fn offline_models() -> Option<(PathBuf, PathBuf)> {
    let checkpoint = std::env::var_os("DEADPAN_ALIGN_CHECKPOINT").map(PathBuf::from);
    let statistics = std::env::var_os("DEADPAN_ALIGN_STATISTICS")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("DEADPAN_ALIGN_REFERENCE_ROOT")
                .map(PathBuf::from)
                .and_then(|p| p.parent().map(|p| p.join("upstream/global_stats.json")))
        });
    match (checkpoint, statistics) {
        (Some(checkpoint), Some(statistics)) => Some((checkpoint, statistics)),
        _ => {
            eprintln!(
                "UNEXERCISED: real model control tests need DEADPAN_ALIGN_CHECKPOINT and DEADPAN_ALIGN_STATISTICS (or DEADPAN_ALIGN_REFERENCE_ROOT)"
            );
            None
        }
    }
}

/// Own the unreaped process-group leader through cleanup. No try_wait call can
/// reap it before the membership proof. Drop covers assertions and I/O errors;
/// a reaping attempt is marked before wait and can never be retried.
struct OwnedWorker {
    child: Child,
    input: Option<ChildStdin>,
    output: Option<ChildStdout>,
    errors: Option<ChildStderr>,
    reaping_attempted: bool,
}

impl OwnedWorker {
    fn start(workspace: &Path) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_deadpan-align"));
        command.current_dir(workspace);
        Self::start_command(&mut command)
    }
    fn start_command(command: &mut Command) -> Self {
        command
            .env_clear()
            .env(
                "PATH",
                std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let child = deadpan_native_process::spawn(command).unwrap();
        let mut owned = Self {
            child,
            input: None,
            output: None,
            errors: None,
            reaping_attempted: false,
        };
        // Install the cleanup guard before any fallible pipe setup.
        owned.input = owned.child.stdin.take();
        owned.output = owned.child.stdout.take();
        owned.errors = owned.child.stderr.take();
        nonblocking(owned.output.as_ref().unwrap());
        nonblocking(owned.errors.as_ref().unwrap());
        owned
    }
    fn send(&mut self, message: &HostMessage) {
        RefinementProtocol::write_request(self.input.as_mut().unwrap(), message).unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
    }
    fn close_input(&mut self) {
        self.input = None;
    }
    fn cleanup(&mut self) -> io::Result<ExitStatus> {
        if self.reaping_attempted {
            return Err(io::Error::other("worker already had its sole reap attempt"));
        }
        let group = deadpan_native_process::terminate_owned_group(
            &self.child,
            Instant::now() + Duration::from_secs(3),
        );
        if group.is_err() {
            deadpan_native_process::terminate_owned_leader(
                &self.child,
                Instant::now() + Duration::from_secs(3),
            )?;
        }
        self.reaping_attempted = true;
        let status = self.child.wait()?;
        group?;
        Ok(status)
    }
    fn finish(mut self) -> (ExitStatus, Vec<WorkerMessage>) {
        let deadline = Instant::now() + WALL_LIMIT;
        let mut output = Vec::new();
        let mut errors = Vec::new();
        loop {
            drain(self.output.as_mut().unwrap(), &mut output).unwrap();
            drain(self.errors.as_mut().unwrap(), &mut errors).unwrap();
            let exited = waitid(
                WaitId::Pid(Pid::from_child(&self.child)),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .unwrap()
            .is_some();
            if exited {
                break;
            }
            assert!(Instant::now() < deadline, "worker exceeded wall bound");
            thread::park_timeout(Duration::from_millis(2));
        }
        let status = self.cleanup().unwrap();
        drain(self.output.as_mut().unwrap(), &mut output).unwrap();
        drain(self.errors.as_mut().unwrap(), &mut errors).unwrap();
        assert!(
            errors.is_empty(),
            "worker stderr: {}",
            String::from_utf8_lossy(&errors)
        );
        let mut cursor = output.as_slice();
        let mut messages = Vec::new();
        while let Some(message) = RefinementProtocol::read_response(&mut cursor).unwrap() {
            messages.push(message);
        }
        (status, messages)
    }
}

fn nonblocking(pipe: &impl std::os::fd::AsFd) {
    let flags = rustix::fs::fcntl_getfl(pipe).unwrap();
    rustix::fs::fcntl_setfl(pipe, flags | OFlags::NONBLOCK).unwrap();
}

impl Drop for OwnedWorker {
    fn drop(&mut self) {
        if !self.reaping_attempted
            && let Err(error) = self.cleanup()
        {
            eprintln!("worker cleanup failed: {error}");
        }
    }
}

fn drain(pipe: &mut impl Read, output: &mut Vec<u8>) -> io::Result<()> {
    let mut bytes = [0_u8; 4096];
    loop {
        match pipe.read(&mut bytes) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                if output.len() + read > 64 * 1024 {
                    return Err(io::Error::other("worker pipe exceeded bound"));
                }
                output.extend_from_slice(&bytes[..read]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[test]
fn real_worker_refuses_pcm_hash_nonfinite_symlink_and_fifo_without_models() {
    for case in ["hash", "nonfinite", "symlink", "fifo"] {
        let mut fixture = Fixture::new(16_000, None);
        let path = fixture.path().join("input/audio.f32le");
        match case {
            "hash" => {
                fixture.audio = WorkspaceArtifact::new(
                    fixture.audio.reference().clone(),
                    Sha256::new("0".repeat(64)).unwrap(),
                    fixture.audio.byte_length(),
                )
                .unwrap()
            }
            "nonfinite" => {
                let bytes = f32::NAN.to_le_bytes().repeat(16_000);
                fs::write(&path, &bytes).unwrap();
                fixture.audio = WorkspaceArtifact::new(
                    fixture.audio.reference().clone(),
                    digest(&bytes),
                    bytes.len() as u64,
                )
                .unwrap();
            }
            "symlink" => {
                fs::remove_file(&path).unwrap();
                symlink(&fixture.statistics, &path).unwrap();
            }
            "fifo" => {
                fs::remove_file(&path).unwrap();
                let mut command = Command::new("mkfifo");
                command.arg("-m").arg("600").arg(&path);
                let (status, messages) = OwnedWorker::start_command(&mut command).finish();
                assert!(status.success());
                assert!(messages.is_empty());
            }
            _ => unreachable!(),
        }
        let mut worker = OwnedWorker::start(fixture.path());
        worker.send(&fixture.infer());
        let (status, messages) = worker.finish();
        assert!(!status.success(), "{case}");
        assert!(
            matches!(messages.as_slice(), [WorkerMessage::Failed { .. }]),
            "{case}: {messages:?}"
        );
        fixture.assert_no_output(&messages);
    }
}

#[test]
fn real_worker_rejects_wrong_statistics_and_invalid_control_limits_without_models() {
    let fixture = Fixture::new(16_000, None);
    let mut worker = OwnedWorker::start(fixture.path());
    worker.send(&fixture.infer());
    let (status, messages) = worker.finish();
    assert!(!status.success());
    assert!(matches!(
        messages.as_slice(),
        [WorkerMessage::Failed { .. }]
    ));
    fixture.assert_no_output(&messages);
    let mut message = fixture.infer();
    if let HostMessage::Infer { timeout_ms, .. } = &mut message {
        *timeout_ms = 0;
    }
    let mut worker = OwnedWorker::start(fixture.path());
    // Write a raw framed value to exercise the executable's own admission,
    // deliberately bypassing the host helper that rejects invalid requests.
    deadpan_jobs::protocol::write_frame(worker.input.as_mut().unwrap(), &message).unwrap();
    worker.close_input();
    let (status, messages) = worker.finish();
    assert_eq!(status.code(), Some(2));
    assert!(messages.is_empty());
    fixture.assert_no_output(&messages);
}

#[test]
fn real_worker_revokes_ownership_on_eof_malformed_or_second_infer_and_honors_deadline() {
    let Some(models) = offline_models() else {
        return;
    };
    for case in ["eof", "malformed", "second_infer", "deadline"] {
        let fixture = Fixture::new(480_000, Some(&models));
        let mut message = fixture.infer();
        if case == "deadline"
            && let HostMessage::Infer { timeout_ms, .. } = &mut message
        {
            *timeout_ms = 1;
        }
        let mut worker = OwnedWorker::start(fixture.path());
        worker.send(&message);
        match case {
            "eof" => worker.close_input(),
            "malformed" => {
                worker
                    .input
                    .as_mut()
                    .unwrap()
                    .write_all(&[0, 0, 0, 1, b'{'])
                    .unwrap();
                worker.input.as_mut().unwrap().flush().unwrap();
            }
            "second_infer" => worker.send(&message),
            "deadline" => {}
            _ => unreachable!(),
        }
        let (_status, messages) = worker.finish();
        if case == "deadline" {
            assert!(
                matches!(messages.as_slice(), [WorkerMessage::Failed { diagnostic, .. }] if diagnostic.as_str().contains("deadline")),
                "{messages:?}"
            );
        } else {
            assert!(
                matches!(messages.as_slice(), [WorkerMessage::Cancelled { .. }]),
                "{case}: {messages:?}"
            );
        }
        fixture.assert_no_output(&messages);
    }
}

#[test]
fn real_worker_ignores_unrelated_cancel_and_completes_actual_inference() {
    let Some(models) = offline_models() else {
        return;
    };
    let fixture = Fixture::new(16_000, Some(&models));
    let mut worker = OwnedWorker::start(fixture.path());
    let infer = fixture.infer();
    worker.send(&infer);
    let mut cancel = RefinementProtocol::from_request(&infer)
        .unwrap()
        .cancellation();
    if let HostMessage::Cancel { attempt, .. } = &mut cancel {
        *attempt = AttemptId::new("unrelated").unwrap();
    }
    worker.send(&cancel);
    let (status, messages) = worker.finish();
    assert!(status.success());
    let [
        WorkerMessage::Completed {
            emissions,
            frames,
            input_samples,
            runtime,
            ..
        },
    ] = messages.as_slice()
    else {
        panic!("{messages:?}");
    };
    assert_eq!((*input_samples, *frames), (16_000, 51));
    runtime.validate().unwrap();
    let bytes = fs::read(fixture.path().join(emissions.reference().as_str())).unwrap();
    assert_eq!(digest(&bytes), *emissions.sha256());
    assert!(
        bytes
            .chunks_exact(4)
            .all(|v| f32::from_le_bytes(v.try_into().unwrap()).is_finite())
    );
}

#[test]
fn shared_supervisor_sends_matching_cancel_and_confirms_worker_cleanup() {
    let Some(models) = offline_models() else {
        return;
    };
    let fixture = Fixture::new(480_000, Some(&models));
    let spec = ProcessSpec {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_deadpan-align")),
        arguments: Vec::new(),
        environment: BTreeMap::new(),
        workspace: fixture.path().to_owned(),
        limits: ProcessLimits {
            maximum_duration: WALL_LIMIT,
            cancellation_grace: Duration::from_secs(3),
            exit_grace: Duration::from_secs(3),
        },
    };
    let mut process =
        SupervisedProcess::<RefinementProtocol>::spawn(spec, fixture.infer()).unwrap();
    process.request_cancel(Instant::now()).unwrap();
    let deadline = Instant::now() + WALL_LIMIT;
    let mut cancelled = false;
    let mut exited = false;
    let mut failure = None;
    while !exited && Instant::now() < deadline {
        match process.poll(Instant::now()) {
            Ok(events) => {
                for event in events {
                    match event {
                        ProcessEvent::Message(message)
                            if matches!(*message, WorkerMessage::Cancelled { .. }) =>
                        {
                            cancelled = true
                        }
                        ProcessEvent::Message(message) => {
                            failure = Some(format!("unexpected worker message: {message:?}"))
                        }
                        ProcessEvent::Fault(error) => failure = Some(error),
                        ProcessEvent::Exited {
                            status,
                            cancellation_escalated,
                        } => {
                            exited = true;
                            if !status.success() || cancellation_escalated {
                                failure = Some(
                                    "matching cancellation required escalation or failed".into(),
                                );
                            }
                        }
                    }
                }
            }
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
        }
        thread::park_timeout(Duration::from_millis(2));
    }
    let stopped = process.finish_owned_work(Instant::now() + Duration::from_secs(3));
    assert!(stopped.is_ok(), "{stopped:?}");
    assert!(failure.is_none(), "{failure:?}");
    assert!(
        exited && cancelled,
        "worker did not acknowledge matching cancellation"
    );
    assert!(!fixture.path().join("output/emissions.f32le").exists());
}
