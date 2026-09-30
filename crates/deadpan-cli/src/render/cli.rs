use super::*;
use crate::encoded_render::workflow::{WorkflowOutcome, WorkflowStage};
use deadpan_core::RevisionId;
use deadpan_store::AccessMode;
use serde::Serialize;
use std::{
    fs::File,
    io::{self, Read, Write},
    os::fd::AsFd,
    path::{Component, Path},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

const OUTPUT_WAIT: Duration = Duration::from_secs(1);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
enum Invocation {
    Start {
        directory: PathBuf,
        name: Option<String>,
        expected: Option<RevisionId>,
    },
    Retry {
        job: RequestId,
        checkpoint: Option<AttemptId>,
        directory: PathBuf,
        name: Option<String>,
    },
    Reconcile(RequestId),
    Json {
        path: PathBuf,
        cancel_only: bool,
    },
    Status {
        job: Option<RequestId>,
        publication: Option<RequestId>,
        publications: bool,
        after: Option<RequestId>,
        after_attempt: u64,
    },
}

fn parse(arguments: &[&str]) -> Result<(PathBuf, Invocation), PublicRenderError> {
    if arguments.is_empty() || arguments.len() > 14 {
        return Err(PublicRenderError::invalid(
            "Use render PROJECT --output DIRECTORY, or render status/retry/reencode/reconcile PROJECT",
        ));
    }
    let (verb, package, rest) = match arguments {
        [
            verb @ ("status" | "retry" | "reencode" | "reconcile" | "cancel"),
            package,
            rest @ ..,
        ] => (*verb, *package, rest),
        [package, rest @ ..] => ("start", *package, rest),
        _ => return Err(PublicRenderError::invalid("A project path is required")),
    };
    let mut flags = BTreeMap::new();
    let mut items = rest.iter();
    while let Some(flag) = items.next() {
        let value = if *flag == "--publications" {
            ""
        } else {
            items
                .next()
                .copied()
                .ok_or_else(|| PublicRenderError::invalid(format!("Missing value for {flag}")))?
        };
        if !flag.starts_with("--") || flags.insert(*flag, value).is_some() {
            return Err(PublicRenderError::invalid(
                "Unexpected argument or duplicate render option",
            ));
        }
    }
    let invocation = if let Some(path) = flags.remove("--json") {
        if !matches!(verb, "start" | "cancel") {
            return Err(PublicRenderError::invalid(
                "--json is accepted by render PROJECT and render cancel PROJECT",
            ));
        }
        Invocation::Json {
            path: path.into(),
            cancel_only: verb == "cancel",
        }
    } else {
        match verb {
            "start" => Invocation::Start {
                directory: required(&mut flags, "--output")?.into(),
                name: flags.remove("--name").map(str::to_owned),
                expected: flags
                    .remove("--expected")
                    .map(RevisionId::new)
                    .transpose()
                    .map_err(PublicRenderError::invalid)?,
            },
            "retry" | "reencode" => Invocation::Retry {
                job: RequestId::new(required(&mut flags, "--job")?)
                    .map_err(PublicRenderError::invalid)?,
                checkpoint: if verb == "retry" {
                    Some(
                        AttemptId::new(required(&mut flags, "--checkpoint")?)
                            .map_err(PublicRenderError::invalid)?,
                    )
                } else {
                    None
                },
                directory: required(&mut flags, "--output")?.into(),
                name: flags.remove("--name").map(str::to_owned),
            },
            "reconcile" => Invocation::Reconcile(
                RequestId::new(required(&mut flags, "--publication")?)
                    .map_err(PublicRenderError::invalid)?,
            ),
            "status" => {
                let job = flags
                    .remove("--job")
                    .map(RequestId::new)
                    .transpose()
                    .map_err(PublicRenderError::invalid)?;
                let publication = flags
                    .remove("--publication")
                    .map(RequestId::new)
                    .transpose()
                    .map_err(PublicRenderError::invalid)?;
                let publications = flags.remove("--publications").is_some();
                let after = flags
                    .remove("--after")
                    .map(RequestId::new)
                    .transpose()
                    .map_err(PublicRenderError::invalid)?;
                let after_attempt = flags
                    .remove("--after-attempt")
                    .map(str::parse::<u64>)
                    .transpose()
                    .map_err(PublicRenderError::invalid)?;
                if u8::from(job.is_some())
                    + u8::from(publication.is_some())
                    + u8::from(publications)
                    > 1
                    || after.is_some() && (job.is_some() || publication.is_some())
                    || after_attempt.is_some() && job.is_none()
                {
                    return Err(PublicRenderError::invalid(
                        "Use one status collection and its matching cursor",
                    ));
                }
                Invocation::Status {
                    job,
                    publication,
                    publications,
                    after,
                    after_attempt: after_attempt.unwrap_or(0),
                }
            }
            _ => {
                return Err(PublicRenderError::invalid(
                    "Use render cancel PROJECT --json REQUEST; remote cancellation requires the future host transport",
                ));
            }
        }
    };
    if !flags.is_empty() {
        return Err(PublicRenderError::invalid(
            "Unknown or incompatible render option",
        ));
    }
    Ok((package.into(), invocation))
}
fn required<'a>(
    flags: &mut BTreeMap<&str, &'a str>,
    name: &str,
) -> Result<&'a str, PublicRenderError> {
    flags
        .remove(name)
        .ok_or_else(|| PublicRenderError::invalid(format!("Required render option: {name}")))
}

fn destination(directory: &Path, name: Option<&str>) -> Result<PathBuf, PublicRenderError> {
    let name = name.map_or_else(
        || format!("deadpan-{}.mp4", uuid::Uuid::new_v4()),
        str::to_owned,
    );
    if Path::new(&name).components().count() != 1
        || !matches!(
            Path::new(&name).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(PublicRenderError::invalid(
            "--name must be one MP4 filename",
        ));
    }
    let directory = std::fs::canonicalize(directory).map_err(PublicRenderError::io)?;
    if !directory.is_dir() {
        return Err(PublicRenderError::invalid(
            "--output must name an existing directory",
        ));
    }
    Ok(directory.join(name))
}

fn read_json(path: &Path) -> Result<RenderRequest, PublicRenderError> {
    let file = File::from(
        rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(PublicRenderError::io)?,
    );
    let metadata = file.metadata().map_err(PublicRenderError::io)?;
    if !metadata.is_file() || metadata.len() > MAX_REQUEST_BYTES as u64 {
        return Err(PublicRenderError::invalid(
            "Render JSON must be a regular file of at most 16 KiB",
        ));
    }
    let mut bytes = Vec::new();
    file.take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(PublicRenderError::io)?;
    RenderRequest::from_json(&bytes)
}

fn open_writer(package: &Path) -> Result<ProjectStore, PublicRenderError> {
    match ProjectStore::open(package, AccessMode::ReadWrite) {
        Err(StoreError::MigrationRequired(_)) => {
            ProjectStore::migrate(package).map_err(PublicRenderError::store)?;
            ProjectStore::open(package, AccessMode::ReadWrite).map_err(PublicRenderError::store)
        }
        result => result.map_err(PublicRenderError::store),
    }
}

pub(crate) fn run(arguments: &[&str]) -> Result<(), PublicRenderError> {
    let (package, invocation) = parse(arguments)?;
    let mut output = JsonOutput::stdout()?;
    if let Invocation::Status {
        job,
        publication,
        publications,
        after,
        after_attempt,
    } = invocation
    {
        let store =
            ProjectStore::open(&package, AccessMode::ReadOnly).map_err(PublicRenderError::store)?;
        return status(
            &store,
            &mut output,
            job,
            publication,
            publications,
            after,
            after_attempt,
        );
    }
    // Parse bounded external input before migration, recovery, or worker spawn.
    let parsed = match &invocation {
        Invocation::Json { path, cancel_only } => {
            let request = read_json(path)?;
            if *cancel_only && !matches!(request.operation, RenderOperation::Cancel { .. }) {
                return Err(PublicRenderError::invalid(
                    "render cancel requires a cancel operation",
                ));
            }
            Some(request)
        }
        _ => None,
    };
    if parsed
        .as_ref()
        .is_some_and(|request| matches!(request.operation, RenderOperation::Cancel { .. }))
    {
        let request = parsed.as_ref().expect("cancel request checked");
        let store =
            ProjectStore::open(&package, AccessMode::ReadOnly).map_err(PublicRenderError::store)?;
        check_context(&store, &request.context, false)?;
        return Err(PublicRenderError::new(
            "RenderOwnerUnavailable",
            "Send SIGINT/SIGTERM to the running headless render. Cross-process cancellation requires local host routing, which is not implemented yet.",
        ));
    }
    if !cfg!(target_os = "macos") {
        return Err(PublicRenderError::new(
            "RenderPlatformUnsupported",
            "Automatic rendering requires the qualified macOS runtime and recoverable APFS publication path",
        ));
    }
    let signals = Signals::register()?;
    let mut store = open_writer(&package)?;
    let document = store.snapshot().map_err(PublicRenderError::store)?;
    let context = RenderContext::from_document(&document);
    let request = if let Some(request) = parsed {
        request
    } else {
        let (context, operation) = match invocation {
            Invocation::Start {
                directory,
                name,
                expected,
            } => {
                let mut context = context;
                if let Some(expected) = expected {
                    context.revision_id = expected;
                }
                (
                    context,
                    RenderOperation::Start {
                        destination: destination(&directory, name.as_deref())?,
                    },
                )
            }
            Invocation::Retry {
                job,
                checkpoint,
                directory,
                name,
            } => {
                let destination = destination(&directory, name.as_deref())?;
                (
                    context,
                    match checkpoint {
                        Some(encoding_attempt_id) => RenderOperation::RetryCheckpoint {
                            job_id: job,
                            encoding_attempt_id,
                            destination,
                        },
                        None => RenderOperation::Reencode {
                            job_id: job,
                            destination,
                        },
                    },
                )
            }
            Invocation::Reconcile(publication_id) => {
                (context, RenderOperation::Reconcile { publication_id })
            }
            _ => {
                return Err(PublicRenderError::invalid(
                    "invalid closed-project render invocation",
                ));
            }
        };
        RenderRequest {
            schema_version: SCHEMA_VERSION,
            request_id: fresh_job()?,
            context,
            operation,
        }
    };
    check_context(
        &store,
        &request.context,
        matches!(request.operation, RenderOperation::Start { .. }),
    )?;
    if matches!(request.operation, RenderOperation::Start { .. }) {
        output_summary(&document)?;
    }
    if signals.cancelled.load(Ordering::Acquire) {
        return Err(PublicRenderError::new(
            "Cancelled",
            "Cancelled before render admission",
        ));
    }
    let config = current_runtime_config(package)?;
    let mut workflow = RenderWorkflow::new(&store, config).map_err(PublicRenderError::workflow)?;
    let admission = match request.operation {
        RenderOperation::Start { destination } => workflow.start(
            &mut store,
            start_request(&request.context, destination, Instant::now())?,
        ),
        RenderOperation::RetryCheckpoint {
            job_id,
            encoding_attempt_id,
            destination,
        } => {
            let retry = retry_request(
                &store,
                &request.context,
                &job_id,
                Some(&encoding_attempt_id),
                destination,
                Instant::now(),
            )?;
            workflow.retry(&mut store, retry)
        }
        RenderOperation::Reencode {
            job_id,
            destination,
        } => {
            let retry = retry_request(
                &store,
                &request.context,
                &job_id,
                None,
                destination,
                Instant::now(),
            )?;
            workflow.retry(&mut store, retry)
        }
        RenderOperation::Reconcile { publication_id } => {
            let reconcile =
                reconcile_request(&store, &request.context, &publication_id, Instant::now())?;
            workflow.reconcile(&mut store, reconcile)
        }
        RenderOperation::Cancel { .. } => {
            return Err(PublicRenderError::invalid(
                "cancel requires an existing live owner",
            ));
        }
    };
    let mut failure = admission.err().map(PublicRenderError::workflow);
    if failure.is_none() {
        failure = output
            .send(&RenderEvent::Admitted {
                schema_version: SCHEMA_VERSION,
                request_id: request.request_id.clone(),
                status: Box::new(RenderStatus::from_workflow(
                    &request.context,
                    workflow.status(),
                )),
            })
            .err();
    }
    let mut error_output = JsonOutput::stderr().ok();
    pump(
        &mut workflow,
        &mut store,
        (&request.context, &request.request_id),
        &signals.cancelled,
        &mut output,
        &mut error_output,
        failure,
    )
}

fn pump(
    workflow: &mut RenderWorkflow,
    store: &mut ProjectStore,
    request: (&RenderContext, &RequestId),
    cancelled: &AtomicBool,
    output: &mut JsonOutput,
    error_output: &mut Option<JsonOutput>,
    mut failure: Option<PublicRenderError>,
) -> Result<(), PublicRenderError> {
    let (context, request_id) = request;
    let mut last_stage = workflow.status().stage;
    let mut last_progress = Instant::now();
    let mut recovery_emitted = false;
    while !workflow.can_release_writer() {
        if (cancelled.load(Ordering::Acquire) || failure.is_some())
            && !workflow.status().cancellation_requested
            && let Some(identity) = workflow.status().identity.clone()
            && let Err(error) = workflow.cancel(store, &identity)
        {
            failure.get_or_insert_with(|| PublicRenderError::workflow(error));
        }
        if let Err(error) = workflow.poll(store) {
            failure.get_or_insert_with(|| PublicRenderError::workflow(error));
        }
        let status = workflow.status();
        if status.stage == WorkflowStage::Unresolved && !status.cleanup_confirmed {
            if !recovery_emitted {
                let error = failure.clone().unwrap_or_else(|| PublicRenderError::new("RenderCleanupUnconfirmed", "Worker cleanup is unconfirmed. This process retains the writer; inspect the diagnostic before any external recovery."));
                if let Err(error) = send_important(
                    output,
                    error_output,
                    &RenderEvent::RecoveryRequired {
                        schema_version: SCHEMA_VERSION,
                        request_id: request_id.clone(),
                        status: Box::new(RenderStatus::from_workflow(context, status)),
                        error,
                    },
                ) {
                    failure.get_or_insert(error);
                }
                recovery_emitted = true;
            }
            // Never turn a lost worker or unproven teardown into writer release.
            // Signals remain cancellation requests; a forced process termination
            // is outside this cooperative recovery contract.
        } else if status.stage != last_stage || last_progress.elapsed() >= PROGRESS_INTERVAL {
            if let Err(error) = output.send(&RenderEvent::Progress {
                schema_version: SCHEMA_VERSION,
                request_id: request_id.clone(),
                status: Box::new(RenderStatus::from_workflow(context, status)),
            }) {
                failure.get_or_insert(error);
            }
            last_stage = status.stage;
            last_progress = Instant::now();
        }
        thread::park_timeout(POLL_INTERVAL);
    }
    if let Err(error) = workflow.drain(store) {
        failure.get_or_insert_with(|| PublicRenderError::workflow(error));
    }
    let status = RenderStatus::from_workflow(context, workflow.status());
    send_important(
        output,
        error_output,
        &RenderEvent::Finished {
            schema_version: SCHEMA_VERSION,
            request_id: request_id.clone(),
            status: Box::new(status.clone()),
        },
    )?;
    if let Some(failure) = failure {
        return Err(failure);
    }
    match status.outcome {
        Some(WorkflowOutcome::Published) => Ok(()),
        Some(WorkflowOutcome::Cancelled) => Err(PublicRenderError::new(
            "Cancelled",
            "Render cancelled after owned work stopped",
        )),
        Some(WorkflowOutcome::PublishedUnconfirmed) => Err(PublicRenderError::new(
            "PublishedUnconfirmed",
            "The movie was committed; publication still requires reconciliation",
        )),
        _ => Err(PublicRenderError::new(
            "RenderFailed",
            status
                .diagnostic
                .as_ref()
                .map_or("Render did not publish a final movie", |diagnostic| {
                    diagnostic.detail.as_str()
                }),
        )),
    }
}

/// Preserve the complete terminal or recovery observation if stdout failed.
/// The stderr attempt has the same bounded serialization/backpressure contract;
/// its success does not turn the failed stdout stream into a successful command.
fn send_important(
    output: &mut JsonOutput,
    error_output: &mut Option<JsonOutput>,
    event: &RenderEvent,
) -> Result<(), PublicRenderError> {
    let result = output.send(event);
    if result.is_err()
        && let Some(error_output) = error_output
    {
        let _ = error_output.send(event);
    }
    result
}

struct Signals {
    cancelled: Arc<AtomicBool>,
    handlers: Vec<signal_hook::SigId>,
}
impl Signals {
    fn register() -> Result<Self, PublicRenderError> {
        let mut result = Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            handlers: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            result.handlers.push(
                signal_hook::flag::register(signal, result.cancelled.clone())
                    .map_err(PublicRenderError::io)?,
            );
        }
        Ok(result)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            signal_hook::low_level::unregister(handler);
        }
    }
}

/// Fixed serialization buffer and nonblocking stdout prevent an unread pipe
/// from indefinitely stopping the writer's stage/cancellation pump.
struct JsonOutput {
    file: File,
    flags: rustix::fs::OFlags,
    failed: bool,
}
impl JsonOutput {
    fn stdout() -> Result<Self, PublicRenderError> {
        Self::from_file(File::from(
            rustix::io::fcntl_dupfd_cloexec(io::stdout().as_fd(), 0)
                .map_err(PublicRenderError::io)?,
        ))
    }
    fn stderr() -> Result<Self, PublicRenderError> {
        Self::from_file(File::from(
            rustix::io::fcntl_dupfd_cloexec(io::stderr().as_fd(), 0)
                .map_err(PublicRenderError::io)?,
        ))
    }
    fn from_file(file: File) -> Result<Self, PublicRenderError> {
        let flags = rustix::fs::fcntl_getfl(&file).map_err(PublicRenderError::io)?;
        rustix::fs::fcntl_setfl(&file, flags | rustix::fs::OFlags::NONBLOCK)
            .map_err(PublicRenderError::io)?;
        Ok(Self {
            file,
            flags,
            failed: false,
        })
    }
    fn send(&mut self, value: &impl Serialize) -> Result<(), PublicRenderError> {
        if self.failed {
            return Err(PublicRenderError::new(
                "RenderOutputUnavailable",
                "Structured output is no longer writable",
            ));
        }
        let result = self.write(value);
        self.failed |= result.is_err();
        result
    }
    fn write(&mut self, value: &impl Serialize) -> Result<(), PublicRenderError> {
        let mut bytes = vec![0; MAX_REPLY_BYTES];
        let mut cursor = io::Cursor::new(&mut bytes[..MAX_REPLY_BYTES - 1]);
        serde_json::to_writer(&mut cursor, value)
            .map_err(|error| PublicRenderError::new("RenderOutputLimit", error))?;
        let length = usize::try_from(cursor.position()).map_err(PublicRenderError::invalid)?;
        bytes[length] = b'\n';
        let end = Instant::now() + OUTPUT_WAIT;
        let mut remaining = &bytes[..length + 1];
        while !remaining.is_empty() {
            if Instant::now() >= end {
                return Err(PublicRenderError::new(
                    "RenderOutputUnavailable",
                    "Structured output exceeded its write deadline",
                ));
            }
            match self.file.write(remaining) {
                Ok(0) => {
                    return Err(PublicRenderError::new(
                        "RenderOutputUnavailable",
                        "Structured output closed",
                    ));
                }
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < end => {
                    thread::park_timeout(POLL_INTERVAL)
                }
                Err(error) => return Err(PublicRenderError::new("RenderOutputUnavailable", error)),
            }
        }
        Ok(())
    }
}

pub(crate) fn report_error(error: &PublicRenderError) {
    if let Ok(mut output) = JsonOutput::stderr() {
        let _ = output.send(&serde_json::json!({"schema_version": SCHEMA_VERSION, "error": error}));
    }
}
impl Drop for JsonOutput {
    fn drop(&mut self) {
        let _ = rustix::fs::fcntl_setfl(&self.file, self.flags);
    }
}

fn status(
    store: &ProjectStore,
    output: &mut JsonOutput,
    job: Option<RequestId>,
    publication: Option<RequestId>,
    publications: bool,
    after: Option<RequestId>,
    after_attempt: u64,
) -> Result<(), PublicRenderError> {
    let context =
        RenderContext::from_document(&store.snapshot().map_err(PublicRenderError::store)?);
    let page = if let Some(job) = job {
        let intent = store.render_job(&job).map_err(PublicRenderError::store)?;
        let attempts = store
            .render_attempts(&job, after_attempt, STATUS_PAGE_SIZE)
            .map_err(PublicRenderError::store)?;
        let next = (attempts.len() == STATUS_PAGE_SIZE as usize)
            .then(|| attempts.last().map(|a| a.ordinal))
            .flatten();
        let attempts: Vec<_> = attempts.into_iter().map(|attempt| serde_json::json!({
            "target": WorkflowTarget { job_id: attempt.job_id, attempt_id: attempt.attempt_id, cancellation_token: attempt.cancellation_token },
            "ordinal": attempt.ordinal, "state": attempt.state, "checkpoint_attempt_id": attempt.checkpoint_attempt_id,
            "cancellation_requested": attempt.cancellation_requested, "diagnostic": attempt.diagnostic,
            "verification_recorded": attempt.verification.is_some()
        })).collect();
        serde_json::json!({"collection":"attempts","intent":intent,"items":attempts,"next_after_attempt":next})
    } else if let Some(publication) = publication {
        serde_json::json!({"collection":"publication","item":publication_summary(store.render_publication(&publication).map_err(PublicRenderError::store)?)})
    } else if publications {
        let records = store
            .render_publications(after.as_ref(), STATUS_PAGE_SIZE)
            .map_err(PublicRenderError::store)?;
        let next = (records.len() == STATUS_PAGE_SIZE as usize)
            .then(|| records.last().map(|r| r.intent.publication_id.clone()))
            .flatten();
        serde_json::json!({"collection":"publications","items":records.into_iter().map(publication_summary).collect::<Vec<_>>(),"next_after":next})
    } else {
        let jobs = store
            .render_jobs(after.as_ref(), STATUS_PAGE_SIZE)
            .map_err(PublicRenderError::store)?;
        let next = (jobs.len() == STATUS_PAGE_SIZE as usize)
            .then(|| jobs.last().map(|job| job.job_id.clone()))
            .flatten();
        serde_json::json!({"collection":"jobs","items":jobs,"next_after":next})
    };
    output.send(&serde_json::json!({"schema_version":SCHEMA_VERSION,"event":"stored_status","context":context,"live_progress":false,"page":page}))
}
fn publication_summary(
    record: deadpan_jobs::render::publication::StoredPublication,
) -> serde_json::Value {
    serde_json::json!({"publication_id":record.intent.publication_id,"job_id":record.intent.job_id,
        "revision_id":record.render_intent.revision_id,"automatic":record.render_intent.policy.is_automatic(),
        "encoding_attempt_id":record.encoding_attempt_id,"destination":record.intent.destination,
        "phase":record.phase,"outcome":record.outcome,"operation_active":record.operation.active,
        "observed_movie_commit":record.observed_movie_commit,"diagnostic":record.operation.diagnostic})
}

#[cfg(test)]
mod tests;
