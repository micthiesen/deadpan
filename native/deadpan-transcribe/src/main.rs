//! Private transcription worker.
//!
//! The host launches this executable with an empty environment in a fresh
//! attempt workspace and sends one framed `Transcribe` message on stdin. The
//! worker verifies its model and analysis PCM again, runs whisper.cpp with token
//! timestamps, and writes raw recognizer segments to `output/transcript.json`.
//! Stdout carries only framed protocol messages; whisper.cpp logs go to stderr,
//! which the host keeps as a bounded diagnostic tail.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use deadpan_analysis::{RawSegment, RawToken};
use deadpan_jobs::protocol::{
    AttemptId, Diagnostic, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_jobs::transcription::{
    Backend, HostMessage, Language, ModelInput, RuntimeReport, VERSION, WorkerMessage, read_host,
    write_worker,
};
use rustix::fs::{Mode, OFlags};
use sha2::Digest;

const ENGINE: &str = "whisper.cpp 1.8.3";
const OUTPUT_FILE: &str = "transcript.json";

type Output = Arc<Mutex<io::Stdout>>;

fn main() -> ExitCode {
    let output: Output = Arc::new(Mutex::new(io::stdout()));
    let mut input = io::stdin();
    let first = match read_host(&mut input) {
        Ok(Some(message)) => message,
        Ok(None) | Err(_) => return ExitCode::from(2),
    };
    let HostMessage::Transcribe {
        request,
        attempt,
        cancellation_token,
        model,
        audio,
        language,
        output_scope,
        maximum_output_bytes,
        ..
    } = first
    else {
        return ExitCode::from(2);
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        // Later host messages can only cancel this attempt.
        let cancelled = Arc::clone(&cancelled);
        let token = cancellation_token.clone();
        std::thread::spawn(move || {
            while let Ok(Some(message)) = read_host(&mut input) {
                if let HostMessage::Cancel {
                    cancellation_token, ..
                } = message
                    && cancellation_token == token
                {
                    cancelled.store(true, Ordering::Release);
                }
            }
        });
    }
    let job = Job {
        request,
        attempt,
        output: Arc::clone(&output),
    };
    let result = run(
        &job,
        &model,
        &audio,
        &language,
        &output_scope,
        maximum_output_bytes,
        &cancelled,
    );
    match result {
        Ok(Some(completed)) => {
            if job.send(completed).is_err() {
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Ok(None) => {
            let cancelled = WorkerMessage::Cancelled {
                protocol: VERSION,
                request: job.request.clone(),
                attempt: job.attempt.clone(),
            };
            if job.send(cancelled).is_err() {
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            let diagnostic = Diagnostic::new(truncate(&error, 4_000))
                .unwrap_or_else(|_| Diagnostic::new("transcription failed").expect("static"));
            let failed = WorkerMessage::Failed {
                protocol: VERSION,
                request: job.request.clone(),
                attempt: job.attempt.clone(),
                diagnostic,
            };
            let _ = job.send(failed);
            ExitCode::from(1)
        }
    }
}

struct Job {
    request: RequestId,
    attempt: AttemptId,
    output: Output,
}

impl Job {
    fn send(&self, message: WorkerMessage) -> Result<(), String> {
        let mut output = self.output.lock().map_err(|_| "stdout lock poisoned")?;
        write_worker(&mut *output, &message)?;
        output.flush().map_err(|error| error.to_string())
    }

    fn progress(&self, percent: u8) -> WorkerMessage {
        WorkerMessage::Progress {
            protocol: VERSION,
            request: self.request.clone(),
            attempt: self.attempt.clone(),
            percent: percent.min(100),
        }
    }
}

fn run(
    job: &Job,
    model: &ModelInput,
    audio: &WorkspaceArtifact,
    language: &Language,
    output_scope: &WorkspaceRef,
    maximum_output_bytes: u64,
    cancelled: &Arc<AtomicBool>,
) -> Result<Option<WorkerMessage>, String> {
    let started = Instant::now();
    verify_model(model)?;
    let pcm = read_pcm(audio)?;
    if cancelled.load(Ordering::Acquire) {
        return Ok(None);
    }
    let segments = recognize(job, model, &pcm, language, cancelled)?;
    let Some(segments) = segments else {
        return Ok(None);
    };
    let artifact = write_transcript(output_scope, &segments, maximum_output_bytes)?;
    Ok(Some(WorkerMessage::Completed {
        protocol: VERSION,
        request: job.request.clone(),
        attempt: job.attempt.clone(),
        transcript: artifact,
        runtime: RuntimeReport {
            engine: ENGINE.into(),
            backend: if cfg!(target_os = "macos") {
                Backend::Metal
            } else {
                Backend::Cpu
            },
            model_sha256: model.sha256.clone(),
        },
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    }))
}

/// Open a regular file without following a final symbolic link.
fn open_regular(directory: Option<&Path>, name: &Path) -> Result<File, String> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let descriptor = match directory {
        Some(directory) => {
            let parent = rustix::fs::open(
                directory,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| format!("open {}: {error}", directory.display()))?;
            rustix::fs::openat(&parent, name, flags, Mode::empty())
        }
        None => rustix::fs::open(name, flags, Mode::empty()),
    }
    .map_err(|error| format!("open {}: {error}", name.display()))?;
    let file = File::from(descriptor);
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", name.display()));
    }
    Ok(file)
}

fn hash_file(mut file: &File, expected_bytes: u64) -> Result<(Vec<u8>, String), String> {
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    if length != expected_bytes {
        return Err(format!(
            "file has {length} bytes, expected {expected_bytes}"
        ));
    }
    let mut hasher = sha2::Sha256::new();
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(usize::try_from(length).map_err(|_| "file too large")?)
        .map_err(|_| "cannot allocate file buffer")?;
    Read::by_ref(&mut file)
        .take(length + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 != length {
        return Err("file changed while reading".into());
    }
    hasher.update(&bytes);
    Ok((bytes, hex(&hasher.finalize())))
}

fn verify_model(model: &ModelInput) -> Result<(), String> {
    let file = open_regular(None, &model.path)?;
    // The model is hashed in a streaming pass; whisper.cpp then loads it by path.
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    if length != model.byte_length {
        return Err("model size differs from its verified manifest".into());
    }
    let mut hasher = sha2::Sha256::new();
    let mut reader = io::BufReader::with_capacity(1 << 20, file);
    io::copy(&mut reader, &mut HashWriter(&mut hasher)).map_err(|error| error.to_string())?;
    if hex(&hasher.finalize()) != model.sha256.as_str() {
        return Err("model hash differs from its verified manifest".into());
    }
    Ok(())
}

struct HashWriter<'a>(&'a mut sha2::Sha256);

impl Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn read_pcm(audio: &WorkspaceArtifact) -> Result<Vec<f32>, String> {
    let reference = Path::new(audio.reference().as_str());
    let (directory, name) = match (reference.parent(), reference.file_name()) {
        (Some(directory), Some(name)) if directory == Path::new("input") => (directory, name),
        _ => return Err("analysis audio must be directly below input/".into()),
    };
    let file = open_regular(Some(directory), Path::new(name))?;
    let (bytes, digest) = hash_file(&file, audio.byte_length())?;
    if digest != audio.sha256().as_str() {
        return Err("analysis audio hash differs from its declaration".into());
    }
    let samples: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err("analysis audio contains non-finite samples".into());
    }
    Ok(samples)
}

#[cfg(target_os = "macos")]
fn recognize(
    job: &Job,
    model: &ModelInput,
    pcm: &[f32],
    language: &Language,
    cancelled: &Arc<AtomicBool>,
) -> Result<Option<Vec<RawSegment>>, String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let path = model.path.to_str().ok_or("model path must be UTF-8")?;
    let context = WhisperContext::new_with_params(path, WhisperContextParameters::default())
        .map_err(|error| format!("load model: {error}"))?;
    let mut state = context
        .create_state()
        .map_err(|error| format!("create recognizer state: {error}"))?;
    let mut parameters = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    parameters.set_token_timestamps(true);
    parameters.set_language(Some(language.code().unwrap_or("auto")));
    parameters.set_print_progress(false);
    parameters.set_print_realtime(false);
    parameters.set_print_special(false);
    parameters.set_print_timestamps(false);
    {
        let output = Arc::clone(&job.output);
        let request = job.request.clone();
        let attempt = job.attempt.clone();
        let mut last = None;
        parameters.set_progress_callback_safe(move |percent: i32| {
            let percent = u8::try_from(percent.clamp(0, 100)).unwrap_or(100);
            if last == Some(percent) {
                return;
            }
            last = Some(percent);
            let job = Job {
                request: request.clone(),
                attempt: attempt.clone(),
                output: Arc::clone(&output),
            };
            let _ = job.send(job.progress(percent));
        });
    }
    abort::install(&mut parameters, cancelled);
    let result = state.full(parameters, pcm);
    if cancelled.load(Ordering::Acquire) {
        return Ok(None);
    }
    result.map_err(|error| format!("recognize: {error}"))?;
    let mut segments = Vec::new();
    for segment in state.as_iter() {
        let mut tokens = Vec::new();
        for index in 0..segment.n_tokens() {
            let Some(token) = segment.get_token(index) else {
                continue;
            };
            let data = token.token_data();
            tokens.push(RawToken {
                text: token
                    .to_str_lossy()
                    .map_err(|error| format!("token text: {error}"))?
                    .into_owned(),
                t0: data.t0,
                t1: data.t1,
                p: data.p,
            });
        }
        segments.push(RawSegment {
            t0: segment.start_timestamp(),
            t1: segment.end_timestamp(),
            tokens,
        });
    }
    Ok(Some(segments))
}

/// whisper-rs 0.16's `set_abort_callback_safe` instantiates its trampoline for
/// the closure type while storing a boxed trait object, so every call reads the
/// wrong layout (observed here as every encode aborting). The raw callback reads
/// one process-wide flag instead and never dereferences its user data.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod abort {
    use std::ffi::c_void;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    static CANCELLED: AtomicBool = AtomicBool::new(false);

    extern "C" fn callback(_user_data: *mut c_void) -> bool {
        CANCELLED.load(Ordering::Acquire)
    }

    pub fn install(parameters: &mut whisper_rs::FullParams<'_, '_>, cancelled: &Arc<AtomicBool>) {
        // One attempt runs per worker process; mirror its flag into the static.
        let cancelled = Arc::clone(cancelled);
        std::thread::spawn(move || {
            while !cancelled.load(Ordering::Acquire) {
                std::thread::park_timeout(std::time::Duration::from_millis(20));
            }
            CANCELLED.store(true, Ordering::Release);
        });
        // SAFETY: `callback` is a plain function valid for the whole process and
        // ignores its user-data pointer, which stays null.
        unsafe {
            parameters.set_abort_callback(Some(callback));
            parameters.set_abort_callback_user_data(std::ptr::null_mut());
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn recognize(
    _job: &Job,
    _model: &ModelInput,
    _pcm: &[f32],
    _language: &Language,
    _cancelled: &Arc<AtomicBool>,
) -> Result<Option<Vec<RawSegment>>, String> {
    Err("this platform has no qualified transcription runtime".into())
}

fn write_transcript(
    output_scope: &WorkspaceRef,
    segments: &[RawSegment],
    maximum_bytes: u64,
) -> Result<WorkspaceArtifact, String> {
    let bytes = serde_json::to_vec(segments).map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > maximum_bytes {
        return Err("transcript exceeds its byte budget".into());
    }
    // The host creates and owns the output scope before launch.
    let directory = Path::new(output_scope.as_str());
    let parent = rustix::fs::open(
        directory,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("open output scope: {error}"))?;
    let descriptor = rustix::fs::openat(
        &parent,
        OUTPUT_FILE,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|error| format!("create transcript: {error}"))?;
    let mut file = File::from(descriptor);
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    let reference = WorkspaceRef::new(format!("{}/{OUTPUT_FILE}", output_scope.as_str()))
        .map_err(|error| error.to_string())?;
    let digest = Sha256::new(hex(&sha2::Sha256::digest(&bytes))).map_err(|e| e.to_string())?;
    WorkspaceArtifact::new(reference, digest, bytes.len() as u64).map_err(|error| error.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn truncate(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].replace('\0', " ")
}
