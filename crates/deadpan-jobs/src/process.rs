//! One private worker attempt, with bounded pipes and process-group cleanup.
//!
//! Call `poll` regularly from the host's job service to enforce deadlines. It
//! performs no pipe reads/writes or blocking thread joins. Explicitly finish
//! owned work before recording a terminal operation. Drop attempts cleanup but
//! cannot report its success. Shutdown belongs on the job service, never the
//! audio callback. This is not an operating-system sandbox.

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

/// A host-selected protocol layered over the shared process transport.
///
/// Implementations validate the initial operation, response identity, version,
/// and message semantics. Their codecs must bound allocation and input/output
/// sizes; the transport bounds queued messages, not an individual message's
/// representation. No adapter may treat a completed response as trusted media.
pub trait WorkerProtocol: Sized {
    type Request: Clone + Send + 'static;
    type Response: Send + 'static;

    /// Validate the initial request and capture its immutable response identity.
    fn from_request(request: &Self::Request) -> Result<Self, SupervisorError>;
    fn cancellation(&self) -> Self::Request;
    fn write_request(writer: &mut impl Write, request: &Self::Request) -> Result<(), String>;
    fn read_response(reader: &mut impl Read) -> Result<Option<Self::Response>, String>;
    fn classify(&self, response: &Self::Response) -> Result<ResponseKind, String>;
    /// Which process diagnostics bucket receives this worker's footprint.
    const WORKER_CLASS: deadpan_diagnostics::WorkerClass = deadpan_diagnostics::WorkerClass::Other;
}

/// Whether a validated response ends the protocol and needs clean teardown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    Progress,
    /// Failure or cancellation acknowledgement, delivered immediately.
    Terminal,
    /// Validated failure, delivered immediately. Exit code 1 is expected after
    /// this terminal; signals, other failing codes and protocol faults are not.
    Failed,
    /// Successful result, withheld until clean process and pipe teardown.
    Completed,
}

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
    #[error("{primary}; worker cleanup remains unconfirmed: {cleanup}")]
    CleanupUnconfirmed {
        #[source]
        primary: Box<SupervisorError>,
        cleanup: CleanupFailure,
    },
}

impl SupervisorError {
    /// For a failed `spawn`, whether no child started or explicit cleanup passed.
    /// A raw `poll` error still requires `finish_owned_work` before this question
    /// can be answered for the owning operation.
    pub fn cleanup_confirmed(&self) -> bool {
        !matches!(self, Self::CleanupUnconfirmed { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupCleanupScope {
    /// Darwin confirmed an exited, owned leader and no other group members.
    MembershipConfirmed,
    /// Linux confirmed a group signal and leader exit, not descendant exit.
    SignalAndLeaderOnly,
}

/// Live cleanup evidence. Only the owning supervisor can construct it.
#[derive(Debug)]
pub struct StoppedProcess {
    group_scope: GroupCleanupScope,
    status: ExitStatus,
    pump_panicked: bool,
}

impl StoppedProcess {
    pub fn group_scope(&self) -> GroupCleanupScope {
        self.group_scope
    }

    pub fn status(&self) -> ExitStatus {
        self.status
    }

    /// A panicked pump has stopped, but cannot qualify successful media work.
    pub fn pump_panicked(&self) -> bool {
        self.pump_panicked
    }

    /// Require the membership evidence needed by durable render completion.
    pub fn require_membership(self) -> Result<Self, CleanupFailure> {
        if self.group_scope == GroupCleanupScope::MembershipConfirmed {
            Ok(self)
        } else {
            Err(CleanupFailure {
                issues: vec![CleanupIssue {
                    stage: CleanupStage::Group,
                    diagnostic: "this platform confirmed only a group signal and leader exit"
                        .into(),
                }],
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupStage {
    Group,
    LeaderFallback,
    Reap,
    Pipes,
}

#[derive(Debug, Clone)]
pub struct CleanupIssue {
    pub stage: CleanupStage,
    pub diagnostic: String,
}

/// Structured failure to establish stopped work. A successful leader fallback
/// never erases a failed group observation.
#[derive(Debug, Clone)]
pub struct CleanupFailure {
    issues: Vec<CleanupIssue>,
}

impl CleanupFailure {
    pub fn issues(&self) -> &[CleanupIssue] {
        &self.issues
    }
}

impl std::fmt::Display for CleanupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            write!(formatter, "{:?}: {}", issue.stage, issue.diagnostic)?;
        }
        Ok(())
    }
}

impl std::error::Error for CleanupFailure {}

#[derive(Debug)]
pub enum ProcessEvent<R> {
    /// Completed responses are emitted only with a clean `Exited` event in the
    /// same batch. They still require independent host artifact validation.
    Message(Box<R>),
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

enum PipeEvent<R> {
    Message(Box<R>),
    ReadError(String),
    /// The framed output reader reached EOF after the host started killing a
    /// worker that ignored cancellation. This is distinct from a parsed
    /// protocol error, stderr I/O failure or EOF observed before escalation.
    CancellationEof,
    WriteError(String),
}

struct ResponseReader<'a, R> {
    reader: &'a mut R,
    cancellation_kill: &'a AtomicBool,
    cancelled_eof: bool,
}

impl<R: Read> Read for ResponseReader<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.reader.read(bytes)?;
        if count == 0 && !bytes.is_empty() {
            self.cancelled_eof = self.cancellation_kill.load(Ordering::Acquire);
        }
        Ok(count)
    }
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

pub struct SupervisedProcess<P: WorkerProtocol> {
    child: Child,
    pid: Pid,
    protocol: P,
    cancel_message: P::Request,
    control: Option<mpsc::SyncSender<P::Request>>,
    events: Option<mpsc::Receiver<PipeEvent<P::Response>>>,
    readers: Vec<JoinHandle<()>>,
    logs: Arc<Mutex<LogBuffer>>,
    stop_io: Arc<AtomicBool>,
    cancellation_kill: Arc<AtomicBool>,
    limits: ProcessLimits,
    started: Instant,
    cancel_started: Option<Instant>,
    terminal_received: Option<Instant>,
    failure_exit_expected: bool,
    completed: Option<Box<P::Response>>,
    group_stopped: bool,
    reap_attempted: bool,
    faulted: bool,
    exit: Option<ExitStatus>,
    exited_at: Option<Instant>,
    exit_delivered: bool,
    cancellation_escalated: bool,
    cleanup_failure: Option<CleanupFailure>,
    pump_panicked: bool,
    /// Process diagnostics: this worker's liveness and sampled footprint.
    live: deadpan_diagnostics::Share,
    footprint: deadpan_diagnostics::Share,
    footprint_sampled: Option<Instant>,
}

impl<P: WorkerProtocol> SupervisedProcess<P> {
    pub fn spawn(spec: ProcessSpec, request: P::Request) -> Result<Self, SupervisorError> {
        Self::spawn_with_controls(spec, request, |_, _| Ok(()), &mut NativeCleanup)
    }

    fn spawn_with_controls(
        spec: ProcessSpec,
        request: P::Request,
        mut check_setup: impl FnMut(SetupStage, &Child) -> io::Result<()>,
        cleanup_operations: &mut impl CleanupOperations,
    ) -> Result<Self, SupervisorError> {
        spec.limits.validate()?;
        P::write_request(&mut io::sink(), &request).map_err(SupervisorError::Request)?;
        let protocol = P::from_request(&request)?;
        let cancel_message = protocol.cancellation();
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
        let child = deadpan_native_process::spawn(
            Command::new(&spec.executable)
                .args(&spec.arguments)
                .env_clear()
                .envs(&spec.environment)
                .current_dir(workspace)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0),
        )?;
        let pid = Pid::from_child(&child);
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_CAPACITY);
        let (control_tx, control_rx) = mpsc::sync_channel(2);
        let mut process = Self {
            child,
            pid,
            protocol,
            cancel_message,
            control: Some(control_tx),
            events: Some(event_rx),
            readers: Vec::new(),
            logs: Arc::new(Mutex::new(LogBuffer::default())),
            stop_io: Arc::new(AtomicBool::new(false)),
            cancellation_kill: Arc::new(AtomicBool::new(false)),
            limits: spec.limits,
            started: Instant::now(),
            cancel_started: None,
            terminal_received: None,
            failure_exit_expected: false,
            completed: None,
            group_stopped: false,
            reap_attempted: false,
            faulted: false,
            exit: None,
            exited_at: None,
            exit_delivered: false,
            cancellation_escalated: false,
            cleanup_failure: None,
            pump_panicked: false,
            live: deadpan_diagnostics::Share::new(&P::WORKER_CLASS.memory().live),
            footprint: deadpan_diagnostics::Share::new(&P::WORKER_CLASS.memory().footprint_bytes),
            footprint_sampled: None,
        };
        process.live.set(1);
        let setup = (|| -> Result<(), SupervisorError> {
            check_setup(SetupStage::Pipes, &process.child)?;
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
            check_setup(SetupStage::InputPump, &process.child)?;
            process.readers.push(
                thread::Builder::new()
                    .name("deadpan-worker-input".into())
                    .spawn(move || {
                        for message in control_rx {
                            if let Err(error) = P::write_request(&mut stdin, &message) {
                                let _ = writer_events.send(PipeEvent::WriteError(error));
                                break;
                            }
                        }
                    })?,
            );
            let reader_events = event_tx.clone();
            let cancellation_kill = Arc::clone(&process.cancellation_kill);
            check_setup(SetupStage::OutputPump, &process.child)?;
            process.readers.push(
                thread::Builder::new()
                    .name("deadpan-worker-output".into())
                    .spawn(move || {
                        loop {
                            let mut reader = ResponseReader {
                                reader: &mut stdout,
                                cancellation_kill: &cancellation_kill,
                                cancelled_eof: false,
                            };
                            let event = match P::read_response(&mut reader) {
                                Ok(Some(message)) => PipeEvent::Message(Box::new(message)),
                                Ok(None) => break,
                                Err(error) => {
                                    let event = if reader.cancelled_eof {
                                        PipeEvent::CancellationEof
                                    } else {
                                        PipeEvent::ReadError(error)
                                    };
                                    let _ = reader_events.send(event);
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
            check_setup(SetupStage::ErrorPump, &process.child)?;
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
                                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                                    continue;
                                }
                                Err(error) => {
                                    let _ = event_tx
                                        .send(PipeEvent::ReadError(format!("stderr: {error}")));
                                    break;
                                }
                            }
                        }
                    })?,
            );
            check_setup(SetupStage::InitialRequest, &process.child)?;
            process
                .control
                .as_ref()
                .ok_or_else(|| io::Error::other("missing worker control"))?
                .try_send(request)
                .map_err(|error| io::Error::other(error.to_string()))?;
            Ok(())
        })();
        if let Err(primary) = setup {
            return match process
                .finish_with(Instant::now() + Duration::from_secs(2), cleanup_operations)
                .and_then(StoppedProcess::require_membership)
            {
                Ok(_) => Err(primary),
                Err(cleanup) => Err(SupervisorError::CleanupUnconfirmed {
                    primary: Box::new(primary),
                    cleanup,
                }),
            };
        }
        Ok(process)
    }

    /// Sample a live, unreaped worker's physical footprint at low frequency
    /// for process diagnostics. Never signals, waits or reaps.
    fn sample_footprint(&mut self, now: Instant) {
        if self.exit.is_some() || self.reap_attempted {
            self.live.set(0);
            self.footprint.set(0);
            return;
        }
        if self.footprint_sampled.is_some_and(|last| {
            now.saturating_duration_since(last) < deadpan_diagnostics::WORKER_SAMPLE_INTERVAL
        }) {
            return;
        }
        self.footprint_sampled = Some(now);
        let memory = P::WORKER_CLASS.memory();
        #[cfg(target_os = "macos")]
        match deadpan_native_process::owned_child_memory(&self.child) {
            Ok(Some(sample)) => {
                memory.samples.increment();
                self.footprint.set(sample.phys_footprint);
            }
            Ok(None) => self.footprint.set(0),
            Err(_) => memory.failures.increment(),
        }
        #[cfg(not(target_os = "macos"))]
        memory.failures.increment();
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

    /// Stop and join all owned work and return live cleanup evidence. This may
    /// block until the supplied real monotonic cleanup deadline; call it only
    /// on a worker. The render deadline may already have expired. A failed group
    /// observation remains unresolved even if the checked leader fallback exits.
    pub fn finish_owned_work(
        &mut self,
        deadline: Instant,
    ) -> Result<StoppedProcess, CleanupFailure> {
        self.finish_with(deadline, &mut NativeCleanup)
    }

    fn finish_with(
        &mut self,
        deadline: Instant,
        operations: &mut impl CleanupOperations,
    ) -> Result<StoppedProcess, CleanupFailure> {
        self.stop_io.store(true, Ordering::Release);
        self.completed = None;
        self.control = None;
        // Drop the receiver before joining pumps blocked on bounded sends.
        self.events = None;
        self.child.stdin = None;
        self.child.stdout = None;
        self.child.stderr = None;
        let mut issues = self
            .cleanup_failure
            .take()
            .map_or_else(Vec::new, |failure| failure.issues);
        if self.reap_attempted && self.exit.is_none() {
            push_cleanup_issue(
                &mut issues,
                CleanupStage::Reap,
                "worker wait failed; reaping ownership is unavailable",
            );
        } else if !self.reap_attempted {
            if !self.group_stopped {
                let group_deadline = deadline.min(Instant::now() + Duration::from_millis(250));
                match operations.stop_group(&self.child, group_deadline) {
                    Ok(()) => self.group_stopped = true,
                    Err(error) => {
                        push_cleanup_issue(&mut issues, CleanupStage::Group, error.to_string());
                    }
                }
            }
            let can_reap = self.group_stopped
                || match operations.stop_leader(&self.child, deadline) {
                    Ok(()) => true,
                    Err(error) => {
                        push_cleanup_issue(
                            &mut issues,
                            CleanupStage::LeaderFallback,
                            error.to_string(),
                        );
                        false
                    }
                };
            if can_reap {
                self.reap_attempted = true;
                match operations.reap(&mut self.child) {
                    Ok(status) => {
                        self.exit = Some(status);
                        self.live.set(0);
                        self.footprint.set(0);
                    }
                    Err(error) => {
                        push_cleanup_issue(&mut issues, CleanupStage::Reap, error.to_string());
                    }
                }
            }
        }
        while !self.readers.iter().all(JoinHandle::is_finished) && Instant::now() < deadline {
            thread::park_timeout(Duration::from_millis(2));
        }
        let mut unfinished = Vec::new();
        for reader in self.readers.drain(..) {
            if reader.is_finished() {
                self.pump_panicked |= reader.join().is_err();
            } else {
                unfinished.push(reader);
            }
        }
        self.readers = unfinished;
        if !self.readers.is_empty() {
            push_cleanup_issue(
                &mut issues,
                CleanupStage::Pipes,
                "worker I/O pumps did not stop before the cleanup deadline",
            );
        }
        if !issues.is_empty() {
            let failure = CleanupFailure { issues };
            self.cleanup_failure = Some(failure.clone());
            return Err(failure);
        }
        if let Some(status) = self
            .exit
            .filter(|_| self.group_stopped && self.readers.is_empty())
        {
            Ok(StoppedProcess {
                group_scope: native_group_scope(),
                status,
                pump_panicked: self.pump_panicked,
            })
        } else {
            let failure = CleanupFailure {
                issues: vec![CleanupIssue {
                    stage: CleanupStage::Reap,
                    diagnostic: "worker cleanup lacks a confirmed exit".into(),
                }],
            };
            self.cleanup_failure = Some(failure.clone());
            Err(failure)
        }
    }

    /// A bounded batch of events. The host must also apply its job lifecycle and
    /// validate completed media after `Exited`, before any durable promotion.
    pub fn poll(
        &mut self,
        now: Instant,
    ) -> Result<Vec<ProcessEvent<P::Response>>, SupervisorError> {
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
        self.sample_footprint(now);
        if self.exit.is_none() && !self.group_stopped {
            if self.cancel_started.is_some_and(|start| {
                now.saturating_duration_since(start) >= self.limits.cancellation_grace
            }) {
                self.cancellation_escalated = true;
                self.cancellation_kill.store(true, Ordering::Release);
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
                            if !status.success()
                                && !(self.failure_exit_expected && status.code() == Some(1))
                            {
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
        event: PipeEvent<P::Response>,
        now: Instant,
        output: &mut Vec<ProcessEvent<P::Response>>,
    ) -> Result<(), SupervisorError> {
        match event {
            PipeEvent::Message(message) if !self.faulted => {
                let kind = match self.protocol.classify(&message) {
                    Ok(kind) => kind,
                    Err(reason) => {
                        self.fail(reason, output)?;
                        return Ok(());
                    }
                };
                if self.terminal_received.is_some() {
                    self.fail(
                        "worker emitted a message after its terminal response".into(),
                        output,
                    )?;
                } else {
                    if kind != ResponseKind::Progress {
                        self.terminal_received = Some(now);
                        self.failure_exit_expected = kind == ResponseKind::Failed;
                        self.control = None;
                    }
                    if kind == ResponseKind::Completed {
                        if self.cancel_started.is_none() {
                            self.completed = Some(message);
                        }
                    } else {
                        output.push(ProcessEvent::Message(message));
                    }
                }
            }
            // Only output EOF observed after our cancellation kill is ignored.
            // Never suppress an already parsed protocol error, a prior EOF or
            // a stderr failure merely because it arrived after escalation.
            PipeEvent::CancellationEof => {}
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
        output: &mut Vec<ProcessEvent<P::Response>>,
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
            NativeCleanup
                .stop_group(&self.child, Instant::now() + Duration::from_millis(250))
                .map_err(SupervisorError::SignalGroup)?;
            self.group_stopped = true;
            self.control = None;
        }
        Ok(())
    }
}

impl<P: WorkerProtocol> Drop for SupervisedProcess<P> {
    fn drop(&mut self) {
        // Fallback only: no caller can infer evidence from this discarded result.
        let _ = self.finish_owned_work(Instant::now() + Duration::from_millis(500));
        // A pump that ignored cancellable I/O can outlive this owner. Dropping
        // its JoinHandle detaches it; it does not stop or join the thread. Keep
        // the explicit Pipes failure returned by finalization truthful rather
        // than blocking its delivery forever. Pumps own their pipe/protocol
        // values and shared buffers, never a project writer or borrowed state.
        // No stopped receipt was issued, so the host must retain its unresolved
        // operation and execution slot even if a detached pump later exits.
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupStage {
    Pipes,
    InputPump,
    OutputPump,
    ErrorPump,
    InitialRequest,
}

trait CleanupOperations {
    fn stop_group(&mut self, child: &Child, deadline: Instant) -> io::Result<()>;
    fn stop_leader(&mut self, child: &Child, deadline: Instant) -> io::Result<()>;
    fn reap(&mut self, child: &mut Child) -> io::Result<ExitStatus>;
}

struct NativeCleanup;

impl CleanupOperations for NativeCleanup {
    fn stop_group(&mut self, child: &Child, deadline: Instant) -> io::Result<()> {
        if Pid::from_child(child) == Pid::INIT {
            return Err(io::Error::other("invalid worker group identity"));
        }
        #[cfg(target_os = "macos")]
        {
            deadpan_native_process::terminate_owned_group(child, deadline)
        }
        #[cfg(target_os = "linux")]
        {
            deadpan_native_process::signal_owned_group(child)?;
            deadpan_native_process::terminate_owned_leader(child, deadline)
        }
    }

    fn stop_leader(&mut self, child: &Child, deadline: Instant) -> io::Result<()> {
        deadpan_native_process::terminate_owned_leader(child, deadline)
    }

    fn reap(&mut self, child: &mut Child) -> io::Result<ExitStatus> {
        child.wait()
    }
}

fn native_group_scope() -> GroupCleanupScope {
    #[cfg(target_os = "macos")]
    {
        GroupCleanupScope::MembershipConfirmed
    }
    #[cfg(target_os = "linux")]
    {
        GroupCleanupScope::SignalAndLeaderOnly
    }
}

fn push_cleanup_issue(
    issues: &mut Vec<CleanupIssue>,
    stage: CleanupStage,
    diagnostic: impl Into<String>,
) {
    // Retrying cleanup cannot erase earlier uncertainty or grow its report.
    if !issues.iter().any(|issue| issue.stage == stage) {
        issues.push(CleanupIssue {
            stage,
            diagnostic: diagnostic.into(),
        });
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

    struct QuietProtocol;

    impl WorkerProtocol for QuietProtocol {
        type Request = ();
        type Response = ();

        fn from_request(_: &()) -> Result<Self, SupervisorError> {
            Ok(Self)
        }

        fn cancellation(&self) {}

        fn write_request(_: &mut impl Write, _: &()) -> Result<(), String> {
            Ok(())
        }

        fn read_response(reader: &mut impl Read) -> Result<Option<()>, String> {
            let mut byte = [0];
            let count = reader.read(&mut byte).map_err(|error| error.to_string())?;
            Ok((count != 0).then_some(()))
        }

        fn classify(&self, _: &()) -> Result<ResponseKind, String> {
            Ok(ResponseKind::Progress)
        }
    }

    fn quiet_spec(workspace: &std::path::Path) -> ProcessSpec {
        ProcessSpec {
            executable: "/bin/sleep".into(),
            arguments: vec!["60".into()],
            environment: BTreeMap::new(),
            workspace: workspace.into(),
            limits: ProcessLimits {
                maximum_duration: Duration::from_secs(60),
                cancellation_grace: Duration::from_millis(50),
                exit_grace: Duration::from_millis(50),
            },
        }
    }

    #[test]
    fn cancellation_eof_is_recorded_only_when_the_reader_observes_it() {
        let cancellation_kill = AtomicBool::new(false);
        let mut source = io::Cursor::new(b"x");
        let mut reader = ResponseReader {
            reader: &mut source,
            cancellation_kill: &cancellation_kill,
            cancelled_eof: false,
        };
        let mut byte = [0];
        assert_eq!(reader.read(&mut byte).unwrap(), 1);
        assert_eq!(reader.read(&mut byte).unwrap(), 0);
        assert!(!reader.cancelled_eof);
        cancellation_kill.store(true, Ordering::Release);
        // A delayed error event retains the earlier EOF's evidence.
        assert!(!reader.cancelled_eof);

        let mut source = io::Cursor::new(b"malformed JSON");
        let mut reader = ResponseReader {
            reader: &mut source,
            cancellation_kill: &cancellation_kill,
            cancelled_eof: false,
        };
        let mut bytes = [0; 14];
        reader.read_exact(&mut bytes).unwrap();
        // Parsing an already complete malformed frame must still fail.
        assert!(!reader.cancelled_eof);
        assert_eq!(reader.read(&mut byte).unwrap(), 0);
        assert!(reader.cancelled_eof);
    }

    #[test]
    fn cancellation_escalation_preserves_independent_read_faults() {
        for diagnostic in [
            "malformed frame",
            "stderr: independent I/O failure",
            "prior EOF",
        ] {
            let workspace = tempfile::tempdir().unwrap();
            let mut process =
                SupervisedProcess::<QuietProtocol>::spawn(quiet_spec(workspace.path()), ())
                    .unwrap();
            process.cancellation_escalated = true;
            let mut events = Vec::new();
            process
                .handle_pipe_event(
                    PipeEvent::ReadError(diagnostic.into()),
                    Instant::now(),
                    &mut events,
                )
                .unwrap();
            assert!(
                matches!(events.as_slice(), [ProcessEvent::Fault(message)] if message == diagnostic)
            );
            process
                .finish_owned_work(Instant::now() + Duration::from_secs(2))
                .unwrap();
        }
    }

    #[derive(Default)]
    struct FaultCleanup {
        fail_group: bool,
        fail_wait: bool,
        calls: Vec<&'static str>,
    }

    impl CleanupOperations for FaultCleanup {
        fn stop_group(&mut self, child: &Child, deadline: Instant) -> io::Result<()> {
            self.calls.push("group");
            if self.fail_group {
                Err(io::Error::other("injected membership query failure"))
            } else {
                NativeCleanup.stop_group(child, deadline)
            }
        }

        fn stop_leader(&mut self, child: &Child, deadline: Instant) -> io::Result<()> {
            self.calls.push("leader");
            NativeCleanup.stop_leader(child, deadline)
        }

        fn reap(&mut self, child: &mut Child) -> io::Result<ExitStatus> {
            self.calls.push("wait");
            let status = NativeCleanup.reap(child)?;
            if self.fail_wait {
                // Simulate an ambiguous wait result after actual reaping. Any
                // subsequent PID operation would violate ownership.
                Err(io::Error::other("injected wait observation failure"))
            } else {
                Ok(status)
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn poll_samples_a_live_worker_footprint_and_releases_it_after_exit() {
        let workspace = tempfile::tempdir().unwrap();
        let memory = &deadpan_diagnostics::WORKERS.other;
        let samples = memory.samples.get();
        let mut process =
            SupervisedProcess::<QuietProtocol>::spawn(quiet_spec(workspace.path()), ()).unwrap();
        assert_eq!(process.live.value(), 1);
        process.poll(Instant::now()).unwrap();
        assert!(process.footprint.value() > 0);
        assert!(memory.samples.get() > samples);
        assert!(memory.footprint_bytes.level().high >= process.footprint.value());
        // A second poll inside the interval does not sample again.
        let sampled = process.footprint_sampled;
        process.poll(Instant::now()).unwrap();
        assert_eq!(process.footprint_sampled, sampled);
        process
            .finish_owned_work(Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(process.live.value(), 0);
        assert_eq!(process.footprint.value(), 0);
    }

    #[test]
    fn explicit_cleanup_reaps_and_joins_before_issuing_receipt() {
        let workspace = tempfile::tempdir().unwrap();
        let mut process =
            SupervisedProcess::<QuietProtocol>::spawn(quiet_spec(workspace.path()), ()).unwrap();
        let stopped = process
            .finish_owned_work(Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(stopped.group_scope(), native_group_scope());
        assert!(!stopped.status().success());
        assert!(!stopped.pump_panicked());
        assert!(process.reap_attempted);
        assert!(process.readers.is_empty());
        assert!(process.control.is_none() && process.events.is_none());
        assert!(process.finish_owned_work(Instant::now()).is_ok());
    }

    #[test]
    fn failed_membership_remains_unconfirmed_after_successful_leader_fallback() {
        let workspace = tempfile::tempdir().unwrap();
        let mut process =
            SupervisedProcess::<QuietProtocol>::spawn(quiet_spec(workspace.path()), ()).unwrap();
        let mut operations = FaultCleanup {
            fail_group: true,
            ..Default::default()
        };
        let failure = process
            .finish_with(Instant::now() + Duration::from_secs(2), &mut operations)
            .unwrap_err();
        assert_eq!(failure.issues()[0].stage, CleanupStage::Group);
        assert_eq!(operations.calls, ["group", "leader", "wait"]);
        assert!(process.exit.is_some());
        assert!(process.readers.is_empty());
        assert!(
            process
                .finish_with(Instant::now(), &mut operations)
                .is_err()
        );
        assert_eq!(operations.calls, ["group", "leader", "wait"]);
    }

    #[test]
    fn failed_explicit_wait_never_retries_pid_operations() {
        let workspace = tempfile::tempdir().unwrap();
        let mut process =
            SupervisedProcess::<QuietProtocol>::spawn(quiet_spec(workspace.path()), ()).unwrap();
        let mut operations = FaultCleanup {
            fail_wait: true,
            ..Default::default()
        };
        let failure = process
            .finish_with(Instant::now() + Duration::from_secs(2), &mut operations)
            .unwrap_err();
        assert_eq!(failure.issues()[0].stage, CleanupStage::Reap);
        assert_eq!(operations.calls, ["group", "wait"]);
        assert!(process.reap_attempted && process.exit.is_none());
        assert!(
            process
                .finish_with(Instant::now(), &mut operations)
                .is_err()
        );
        assert_eq!(operations.calls, ["group", "wait"]);
    }

    #[test]
    fn setup_failures_report_cleanup_for_every_partial_initialization() {
        for selected in [
            SetupStage::Pipes,
            SetupStage::InputPump,
            SetupStage::OutputPump,
            SetupStage::ErrorPump,
            SetupStage::InitialRequest,
        ] {
            let workspace = tempfile::tempdir().unwrap();
            let mut pid = None;
            let result = SupervisedProcess::<QuietProtocol>::spawn_with_controls(
                quiet_spec(workspace.path()),
                (),
                |stage, child| {
                    pid = Some(Pid::from_child(child));
                    if stage == selected {
                        Err(io::Error::other("injected setup failure"))
                    } else {
                        Ok(())
                    }
                },
                &mut NativeCleanup,
            );
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("setup did not fail"),
            };
            assert_eq!(error.cleanup_confirmed(), cfg!(target_os = "macos"));
            assert!(error.to_string().contains("injected setup failure"));
            assert_eq!(
                waitid(
                    WaitId::Pid(pid.unwrap()),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT
                )
                .unwrap_err(),
                rustix::io::Errno::CHILD
            );
        }
    }

    #[test]
    fn setup_failure_keeps_primary_and_unconfirmed_group_cleanup() {
        let workspace = tempfile::tempdir().unwrap();
        let mut operations = FaultCleanup {
            fail_group: true,
            ..Default::default()
        };
        let result = SupervisedProcess::<QuietProtocol>::spawn_with_controls(
            quiet_spec(workspace.path()),
            (),
            |_, _| Err(io::Error::other("injected initial setup failure")),
            &mut operations,
        );
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("setup did not fail"),
        };
        assert!(!error.cleanup_confirmed());
        let SupervisorError::CleanupUnconfirmed { primary, cleanup } = error else {
            panic!("setup discarded cleanup evidence")
        };
        assert!(
            primary
                .to_string()
                .contains("injected initial setup failure")
        );
        assert_eq!(cleanup.issues()[0].stage, CleanupStage::Group);
        assert_eq!(operations.calls, ["group", "leader", "wait"]);
    }

    #[test]
    fn joined_pump_panic_is_distinct_from_successful_pump_completion() {
        let workspace = tempfile::tempdir().unwrap();
        let mut process =
            SupervisedProcess::<QuietProtocol>::spawn(quiet_spec(workspace.path()), ()).unwrap();
        process
            .readers
            .push(thread::spawn(|| panic!("injected I/O pump panic")));
        let stopped = process
            .finish_owned_work(Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert!(stopped.pump_panicked());
        assert!(process.readers.is_empty());
    }

    #[test]
    fn unresolved_held_pump_does_not_block_failure_delivery_during_drop() {
        let workspace = tempfile::tempdir().unwrap();
        let mut process =
            SupervisedProcess::<QuietProtocol>::spawn(quiet_spec(workspace.path()), ()).unwrap();
        process
            .finish_owned_work(Instant::now() + Duration::from_secs(2))
            .unwrap();
        let (release, held) = mpsc::channel();
        let (exited, stopped) = mpsc::channel();
        process.readers.push(thread::spawn(move || {
            let _ = held.recv();
            let _ = exited.send(());
        }));
        let failure = process.finish_owned_work(Instant::now()).unwrap_err();
        assert!(
            failure
                .issues()
                .iter()
                .any(|issue| issue.stage == CleanupStage::Pipes)
        );
        let (returned, result) = mpsc::channel();
        let dropping = thread::spawn(move || {
            drop(process);
            let _ = returned.send(());
        });
        // Always release the held thread before asserting, so the regression
        // fails without leaving either fixture thread blocked on old code.
        let delivered = result.recv_timeout(Duration::from_secs(2));
        release.send(()).unwrap();
        stopped.recv_timeout(Duration::from_secs(2)).unwrap();
        dropping.join().unwrap();
        assert!(
            delivered.is_ok(),
            "Drop blocked delivery of unresolved cleanup"
        );
        assert!(
            failure
                .issues()
                .iter()
                .any(|issue| issue.stage == CleanupStage::Pipes)
        );
    }

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
