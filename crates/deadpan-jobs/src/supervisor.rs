//! One private worker attempt, with bounded pipes and process-group cleanup.
//!
//! Call `poll` regularly from the host's job service to enforce deadlines. It
//! performs no pipe reads/writes or blocking thread joins. Dropping a live
//! supervisor kills and reaps its group; perform that shutdown on the job
//! service, never on the audio callback. This is not an operating-system sandbox.

use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
use thiserror::Error;

use crate::protocol::{
    HostMessage, MessageIdentity, ProtocolVersion, WorkerMessage, read_worker_message,
    write_host_message,
};

const EVENT_CAPACITY: usize = 8;
const POLL_EVENT_LIMIT: usize = 16;
const LOG_BYTES: usize = 64 * 1024;
const MAX_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

/// Executable and environment are selected by the host's vetted runtime, not
/// by a project, model pack, or worker message. Nothing invokes a shell.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: BTreeMap<OsString, OsString>,
    pub workspace: PathBuf,
    pub limits: ProcessLimits,
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessLimits {
    pub maximum_duration: Duration,
    pub cancellation_grace: Duration,
    pub exit_grace: Duration,
}

impl ProcessLimits {
    fn validate(self) -> Result<(), SupervisorError> {
        if self.maximum_duration.is_zero()
            || self.maximum_duration > MAX_TIMEOUT
            || self.cancellation_grace > self.maximum_duration
            || self.exit_grace > self.maximum_duration
        {
            return Err(SupervisorError::Configuration("invalid worker deadlines"));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("invalid worker configuration: {0}")]
    Configuration(&'static str),
    #[error("invalid initial worker request: {0}")]
    Request(String),
    #[error("worker process I/O: {0}")]
    Io(#[from] io::Error),
    #[error("inspect worker exit: {0}")]
    ObserveExit(#[source] io::Error),
    #[error("stop worker process group: {0}")]
    SignalGroup(#[source] io::Error),
}

#[derive(Debug)]
pub enum ProcessEvent {
    /// Completed candidates are emitted only with a clean `Exited` event in the
    /// same batch. They still require independent host artifact validation.
    Message(Box<WorkerMessage>),
    Fault(String),
    /// Emitted only after process-group cleanup and all pipe readers finish.
    /// Exit success alone does not validate an artifact or authorize acceptance.
    Exited {
        status: ExitStatus,
        cancellation_escalated: bool,
    },
}

#[derive(Debug, Clone)]
pub struct LogTail {
    pub bytes: Vec<u8>,
    pub discarded_bytes: u64,
}

#[derive(Default)]
struct LogBuffer {
    bytes: VecDeque<u8>,
    discarded: u64,
}

impl LogBuffer {
    fn append(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.bytes.len() == LOG_BYTES {
                self.bytes.pop_front();
                self.discarded = self.discarded.saturating_add(1);
            }
            self.bytes.push_back(byte);
        }
    }
}

enum PipeEvent {
    Message(Box<WorkerMessage>),
    ReadError(String),
    WriteError(String),
}

/// Pipe pumping may wait on its own thread, but shutdown can interrupt it even
/// when a descendant has moved to another process group and retained a handle.
struct CancellablePipe<T> {
    pipe: T,
    stopped: Arc<AtomicBool>,
}

impl<T: AsFd> CancellablePipe<T> {
    fn new(pipe: T, stopped: Arc<AtomicBool>) -> io::Result<Self> {
        let flags = fcntl_getfl(&pipe)?;
        fcntl_setfl(&pipe, flags | OFlags::NONBLOCK)?;
        Ok(Self { pipe, stopped })
    }

    fn check_stop(&self) -> io::Result<()> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "worker pipe shutdown",
            ));
        }
        Ok(())
    }
}

impl<T: Read + AsFd> Read for CancellablePipe<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            self.check_stop()?;
            match self.pipe.read(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::park_timeout(Duration::from_millis(2))
                }
                result => return result,
            }
        }
    }
}

impl<T: Write + AsFd> Write for CancellablePipe<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.check_stop()?;
            match self.pipe.write(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::park_timeout(Duration::from_millis(2))
                }
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.pipe.flush()
    }
}

pub struct WorkerProcess {
    child: Child,
    pid: Pid,
    identity: MessageIdentity,
    protocol: ProtocolVersion,
    cancel_message: HostMessage,
    control: Option<mpsc::SyncSender<HostMessage>>,
    events: Option<mpsc::Receiver<PipeEvent>>,
    readers: Vec<JoinHandle<()>>,
    logs: Arc<Mutex<LogBuffer>>,
    stop_io: Arc<AtomicBool>,
    limits: ProcessLimits,
    started: Instant,
    cancel_started: Option<Instant>,
    terminal_received: Option<Instant>,
    completed: Option<Box<WorkerMessage>>,
    group_stopped: bool,
    reap_attempted: bool,
    faulted: bool,
    exit: Option<ExitStatus>,
    exited_at: Option<Instant>,
    exit_delivered: bool,
    cancellation_escalated: bool,
}

impl WorkerProcess {
    pub fn spawn(spec: ProcessSpec, request: HostMessage) -> Result<Self, SupervisorError> {
        spec.limits.validate()?;
        write_host_message(&mut io::sink(), &request)
            .map_err(|error| SupervisorError::Request(error.to_string()))?;
        let (identity, cancel_message) = match &request {
            HostMessage::GenerateHold {
                protocol,
                identity,
                cancellation_token,
                ..
            }
            | HostMessage::GenerateBridge {
                protocol,
                identity,
                cancellation_token,
                ..
            } => (
                identity.clone(),
                HostMessage::Cancel {
                    protocol: *protocol,
                    identity: identity.clone(),
                    cancellation_token: cancellation_token.clone(),
                },
            ),
            HostMessage::Cancel { .. } => {
                return Err(SupervisorError::Configuration(
                    "initial request must generate a hold",
                ));
            }
        };
        if !spec.executable.is_absolute() || !spec.executable.is_file() {
            return Err(SupervisorError::Configuration(
                "runtime executable must be an absolute file",
            ));
        }
        if !std::fs::symlink_metadata(&spec.workspace)?
            .file_type()
            .is_dir()
        {
            return Err(SupervisorError::Configuration(
                "workspace must be a real directory",
            ));
        }
        let workspace = std::fs::canonicalize(&spec.workspace)?;
        let child = Command::new(&spec.executable)
            .args(&spec.arguments)
            .env_clear()
            .envs(&spec.environment)
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let pid = Pid::from_child(&child);
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_CAPACITY);
        let (control_tx, control_rx) = mpsc::sync_channel(2);
        let mut process = Self {
            child,
            pid,
            identity,
            protocol: request.protocol(),
            cancel_message,
            control: Some(control_tx),
            events: Some(event_rx),
            readers: Vec::new(),
            logs: Arc::new(Mutex::new(LogBuffer::default())),
            stop_io: Arc::new(AtomicBool::new(false)),
            limits: spec.limits,
            started: Instant::now(),
            cancel_started: None,
            terminal_received: None,
            completed: None,
            group_stopped: false,
            reap_attempted: false,
            faulted: false,
            exit: None,
            exited_at: None,
            exit_delivered: false,
            cancellation_escalated: false,
        };
        let stdin = process
            .child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing worker stdin"))?;
        let stdout = process
            .child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing worker stdout"))?;
        let stderr = process
            .child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("missing worker stderr"))?;
        let mut stdin = CancellablePipe::new(stdin, Arc::clone(&process.stop_io))?;
        let mut stdout = CancellablePipe::new(stdout, Arc::clone(&process.stop_io))?;
        let mut stderr = CancellablePipe::new(stderr, Arc::clone(&process.stop_io))?;
        let writer_events = event_tx.clone();
        process.readers.push(
            thread::Builder::new()
                .name("deadpan-worker-input".into())
                .spawn(move || {
                    for message in control_rx {
                        if let Err(error) = write_host_message(&mut stdin, &message) {
                            let _ = writer_events.send(PipeEvent::WriteError(error.to_string()));
                            break;
                        }
                    }
                })?,
        );
        let reader_events = event_tx.clone();
        process.readers.push(
            thread::Builder::new()
                .name("deadpan-worker-output".into())
                .spawn(move || {
                    loop {
                        let event = match read_worker_message(&mut stdout) {
                            Ok(Some(message)) => PipeEvent::Message(Box::new(message)),
                            Ok(None) => break,
                            Err(error) => {
                                let _ = reader_events.send(PipeEvent::ReadError(error.to_string()));
                                break;
                            }
                        };
                        if reader_events.send(event).is_err() {
                            break;
                        }
                    }
                })?,
        );
        let logs = Arc::clone(&process.logs);
        process.readers.push(
            thread::Builder::new()
                .name("deadpan-worker-stderr".into())
                .spawn(move || {
                    let mut buffer = [0; 4096];
                    loop {
                        match stderr.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(count) => logs
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .append(&buffer[..count]),
                            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                            Err(error) => {
                                let _ =
                                    event_tx.send(PipeEvent::ReadError(format!("stderr: {error}")));
                                break;
                            }
                        }
                    }
                })?,
        );
        process
            .control
            .as_ref()
            .ok_or_else(|| io::Error::other("missing worker control"))?
            .try_send(request)
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(process)
    }

    /// Queues one cooperative cancellation without touching a pipe on this thread.
    pub fn request_cancel(&mut self, now: Instant) -> Result<bool, SupervisorError> {
        if self.cancel_started.is_some() || self.reap_attempted {
            return Ok(false);
        }
        self.cancel_started = Some(now);
        self.completed = None;
        if let Some(control) = &self.control {
            match control.try_send(self.cancel_message.clone()) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(_)) => {
                    return Err(SupervisorError::Configuration(
                        "worker control queue is full",
                    ));
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    self.control = None;
                }
            }
        }
        Ok(true)
    }

    pub fn log_tail(&self) -> LogTail {
        let logs = self.logs.lock().unwrap_or_else(|error| error.into_inner());
        LogTail {
            bytes: logs.bytes.iter().copied().collect(),
            discarded_bytes: logs.discarded,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.exit_delivered
    }

    /// A bounded batch of events. The host must also apply its job lifecycle and
    /// validate completed media after `Exited`, before any durable promotion.
    pub fn poll(&mut self, now: Instant) -> Result<Vec<ProcessEvent>, SupervisorError> {
        if self.reap_attempted && self.exit.is_none() {
            return Err(SupervisorError::ObserveExit(io::Error::other(
                "worker reaping failed; ownership is no longer available",
            )));
        }
        let mut output = Vec::new();
        for _ in 0..POLL_EVENT_LIMIT {
            let event = self
                .events
                .as_ref()
                .and_then(|events| events.try_recv().ok());
            let Some(event) = event else {
                break;
            };
            self.handle_pipe_event(event, now, &mut output)?;
        }
        if self.exit.is_none()
            && waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .map_err(|error| SupervisorError::ObserveExit(error.into()))?
            .is_some()
        {
            // Observe exit before applying deadlines: a delayed host poll must
            // not report a finished worker as timed out or force-cancelled.
            // Keep the leader unreaped until group cleanup completes, preventing
            // PID reuse during every cleanup signal.
            self.stop_group()?;
            // Any wait error may mean lost ownership. Drop must not retry that
            // PID using the cached successful group cleanup alone.
            self.exit = Some(reap_child_once(&mut self.child, &mut self.reap_attempted)?);
            self.exited_at = Some(now);
            self.control = None;
        }
        if self.exit.is_none() && !self.group_stopped {
            if self.cancel_started.is_some_and(|start| {
                now.saturating_duration_since(start) >= self.limits.cancellation_grace
            }) {
                self.cancellation_escalated = true;
                self.stop_group()?;
            } else if now.saturating_duration_since(self.started) >= self.limits.maximum_duration {
                self.fail("worker exceeded its maximum duration".into(), &mut output)?;
            } else if self
                .terminal_received
                .is_some_and(|start| now.saturating_duration_since(start) >= self.limits.exit_grace)
            {
                self.fail(
                    "worker did not exit after its terminal response".into(),
                    &mut output,
                )?;
            }
        }
        if !self.readers.iter().all(JoinHandle::is_finished)
            && self
                .exited_at
                .is_some_and(|exit| now.saturating_duration_since(exit) >= self.limits.exit_grace)
        {
            if !self.faulted {
                self.fail(
                    format!(
                        "worker pipes stayed open after process exit: {:?}",
                        self.readers
                            .iter()
                            .filter(|pump| !pump.is_finished())
                            .filter_map(|pump| pump.thread().name())
                            .collect::<Vec<_>>()
                    ),
                    &mut output,
                )?;
            }
            self.stop_io.store(true, Ordering::Release);
        }
        if self.exit.is_some()
            && self.readers.iter().all(JoinHandle::is_finished)
            && !self.exit_delivered
        {
            // Observe an empty queue only AFTER the senders have finished, so a
            // last result cannot arrive behind the process-exit event.
            match self.events.as_ref().map(mpsc::Receiver::try_recv) {
                Some(Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)) | None => {
                    if let Some(status) = self.exit {
                        if !self.faulted && self.cancel_started.is_none() {
                            if !status.success() {
                                self.fail(
                                    format!("worker exited unsuccessfully: {status}"),
                                    &mut output,
                                )?;
                            } else if self.terminal_received.is_none() {
                                self.fail(
                                    "worker exited without a terminal response".into(),
                                    &mut output,
                                )?;
                            }
                        }
                        if !self.faulted
                            && self.cancel_started.is_none()
                            && status.success()
                            && let Some(completed) = self.completed.take()
                        {
                            // Never expose a candidate while its worker or
                            // inherited pipes could still fail cleanup. The
                            // host must still validate its untrusted artifact.
                            output.push(ProcessEvent::Message(completed));
                        }
                        output.push(ProcessEvent::Exited {
                            status,
                            cancellation_escalated: self.cancellation_escalated,
                        });
                        self.exit_delivered = true;
                    }
                }
                Some(Ok(event)) => {
                    // A final queued item must retain normal validation/order.
                    // Defer it explicitly rather than consuming it invisibly.
                    self.handle_pipe_event(event, now, &mut output)?;
                }
            }
        }
        Ok(output)
    }

    fn handle_pipe_event(
        &mut self,
        event: PipeEvent,
        now: Instant,
        output: &mut Vec<ProcessEvent>,
    ) -> Result<(), SupervisorError> {
        match event {
            PipeEvent::Message(message) if !self.faulted => {
                if message.protocol() != self.protocol {
                    self.fail(
                        "worker response protocol differs from this attempt".into(),
                        output,
                    )?;
                } else if message.identity() != &self.identity {
                    self.fail(
                        "worker response identity differs from this attempt".into(),
                        output,
                    )?;
                } else if self.terminal_received.is_some() {
                    self.fail(
                        "worker emitted a message after its terminal response".into(),
                        output,
                    )?;
                } else {
                    if matches!(
                        *message,
                        WorkerMessage::Completed { .. }
                            | WorkerMessage::CompletedBridge { .. }
                            | WorkerMessage::Failed { .. }
                            | WorkerMessage::Cancelled { .. }
                    ) {
                        self.terminal_received = Some(now);
                        self.control = None;
                    }
                    if matches!(
                        *message,
                        WorkerMessage::Completed { .. } | WorkerMessage::CompletedBridge { .. }
                    ) {
                        if self.cancel_started.is_none() {
                            self.completed = Some(message);
                        }
                    } else {
                        output.push(ProcessEvent::Message(message));
                    }
                }
            }
            PipeEvent::ReadError(error) if !self.faulted => self.fail(error, output)?,
            PipeEvent::WriteError(error)
                if !self.faulted
                    && self.cancel_started.is_none()
                    && self.terminal_received.is_none() =>
            {
                self.fail(error, output)?
            }
            _ => {}
        }
        Ok(())
    }

    fn fail(
        &mut self,
        reason: String,
        output: &mut Vec<ProcessEvent>,
    ) -> Result<(), SupervisorError> {
        self.faulted = true;
        self.completed = None;
        output.push(ProcessEvent::Fault(reason));
        self.stop_group()
    }

    fn stop_group(&mut self) -> Result<(), SupervisorError> {
        if !self.group_stopped {
            // Never allow kill(-1), including in case of an unexpected platform
            // API change. A spawned child cannot legitimately be init.
            if self.pid == Pid::INIT {
                return Err(SupervisorError::Configuration(
                    "invalid worker group identity",
                ));
            }
            #[cfg(target_os = "macos")]
            deadpan_native_process::terminate_owned_group(
                &self.child,
                Instant::now() + Duration::from_millis(250),
            )
            .map_err(SupervisorError::SignalGroup)?;
            #[cfg(target_os = "linux")]
            {
                deadpan_native_process::signal_owned_group(&self.child)
                    .map_err(SupervisorError::SignalGroup)?;
                deadpan_native_process::terminate_owned_leader(
                    &self.child,
                    Instant::now() + Duration::from_millis(250),
                )
                .map_err(SupervisorError::SignalGroup)?;
            }
            self.group_stopped = true;
            self.control = None;
        }
        Ok(())
    }
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        self.stop_io.store(true, Ordering::Release);
        let can_reap = !self.reap_attempted && self.stop_group().is_ok();
        // A membership-query failure still permits a separately ownership-
        // checked attempt to stop the leader, but never a retry after wait.
        let can_reap = can_reap
            || (!self.reap_attempted
                && deadpan_native_process::terminate_owned_leader(
                    &self.child,
                    Instant::now() + Duration::from_millis(250),
                )
                .is_ok());
        self.control = None;
        // Release backpressure before joining any pipe reader.
        self.events = None;
        if can_reap {
            let _ = reap_child_once(&mut self.child, &mut self.reap_attempted);
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

fn reap_child_once(child: &mut Child, attempted: &mut bool) -> io::Result<ExitStatus> {
    if *attempted {
        return Err(io::Error::other("worker reaping was already attempted"));
    }
    *attempted = true;
    child.wait()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_reap_prevents_a_second_pid_wait() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()
            .unwrap();
        // Reap outside Child so its own wait encounters a real ownership error.
        waitid(WaitId::Pid(Pid::from_child(&child)), WaitIdOptions::EXITED).unwrap();
        let mut attempted = false;
        assert_eq!(
            reap_child_once(&mut child, &mut attempted)
                .unwrap_err()
                .raw_os_error(),
            Some(rustix::io::Errno::CHILD.raw_os_error())
        );
        assert!(attempted);
        assert_eq!(
            reap_child_once(&mut child, &mut attempted)
                .unwrap_err()
                .to_string(),
            "worker reaping was already attempted"
        );
    }
}
