//! One bounded inference attempt per process, supervised by the host.
use deadpan_align::{AlignError, Control, Normalization, Session, extract_features};
use deadpan_jobs::{
    protocol::{Diagnostic, Sha256, WorkspaceArtifact, WorkspaceRef},
    refinement::{
        self, Backend, HostMessage, ModelInput, RuntimeReport, WorkerMessage, read_host,
        write_worker,
    },
};
use rustix::fs::{Mode, OFlags};
use sha2::{Digest, Sha256 as Hasher};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::{fd::OwnedFd, unix::fs::MetadataExt},
    path::Path,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const CHUNK: usize = 64 * 1024;

fn main() -> ExitCode {
    let started = Instant::now();
    let mut input = io::stdin();
    let Ok(Some(HostMessage::Infer {
        request,
        attempt,
        cancellation_token,
        checkpoint,
        statistics,
        audio,
        output_scope,
        max_output_bytes,
        timeout_ms,
        ..
    })) = read_host(&mut input)
    else {
        return ExitCode::from(2);
    };
    let Some(deadline) = started.checked_add(Duration::from_millis(timeout_ms)) else {
        return ExitCode::from(2);
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let flag = cancelled.clone();
        let request = request.clone();
        let attempt = attempt.clone();
        std::thread::spawn(move || {
            // A second work request, malformed stream or EOF revokes
            // ownership. A well-formed unrelated cancel is ignored.
            while let Ok(Some(message @ HostMessage::Cancel { .. })) = read_host(&mut input) {
                if message.cancels(&request, &attempt, &cancellation_token) {
                    break;
                }
            }
            flag.store(true, Ordering::Release);
        });
    }
    let control = Control::new(cancelled, deadline);
    let result = run(
        &checkpoint,
        &statistics,
        &audio,
        &output_scope,
        max_output_bytes,
        &control,
    )
    .and_then(|value| {
        control.check()?;
        Ok(value)
    });
    let message = match result {
        Ok((emissions, input_samples, frames)) => WorkerMessage::Completed {
            protocol: refinement::VERSION,
            request,
            attempt,
            emissions,
            input_samples,
            frames,
            runtime: runtime(),
            elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
        Err(AlignError::Cancelled) => WorkerMessage::Cancelled {
            protocol: refinement::VERSION,
            request,
            attempt,
        },
        Err(error) => WorkerMessage::Failed {
            protocol: refinement::VERSION,
            request,
            attempt,
            diagnostic: Diagnostic::new(error.to_string()).unwrap_or_else(|_| {
                Diagnostic::new("phoneme inference failed").expect("static diagnostic")
            }),
        },
    };
    let failed = matches!(message, WorkerMessage::Failed { .. });
    let mut output = io::stdout().lock();
    if write_worker(&mut output, &message).is_err() || output.flush().is_err() {
        return ExitCode::from(2);
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn runtime() -> RuntimeReport {
    RuntimeReport {
        engine: deadpan_align::ENGINE_ID.into(),
        ort_version: deadpan_align::ORT_VERSION.into(),
        backend: Backend::Cpu,
        checkpoint_sha256: Sha256::new(deadpan_align::assembly::CHECKPOINT_SHA256)
            .expect("compiled SHA256"),
        statistics_sha256: Sha256::new(deadpan_align::GLOBAL_STATS_SHA256)
            .expect("compiled SHA256"),
        graph_sha256: Sha256::new(deadpan_align::assembly::GRAPH_SHA256).expect("compiled SHA256"),
    }
}

fn native_error(message: &'static str) -> AlignError {
    AlignError::Input(message)
}

fn run(
    checkpoint: &ModelInput,
    statistics: &ModelInput,
    audio: &WorkspaceArtifact,
    output_scope: &WorkspaceRef,
    maximum_bytes: u64,
    control: &Control,
) -> Result<(WorkspaceArtifact, u32, u32), AlignError> {
    control.check()?;
    let workspace = rustix::fs::open(
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| native_error("open attempt workspace"))?;
    validate_directory(&workspace, &workspace)?;
    let pcm_file = open_input(&workspace, audio.reference(), control)?;
    let pcm_bytes = read_verified(
        pcm_file,
        audio.byte_length(),
        audio.sha256().as_str(),
        control,
    )?;
    let pcm: Vec<f32> = pcm_bytes
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        .collect();
    drop(pcm_bytes);
    if !(refinement::MIN_SAMPLES..=refinement::MAX_SAMPLES).contains(&(pcm.len() as u64))
        || pcm.iter().any(|v| !v.is_finite())
    {
        return Err(native_error("PCM format, length or finite values"));
    }
    let statistics_bytes = read_model(statistics, control)?;
    let normalization = Normalization::from_json(&statistics_bytes)?;
    drop(statistics_bytes);
    let features = extract_features(&pcm, &normalization, control)?;
    let input_samples = u32::try_from(pcm.len()).map_err(|_| native_error("PCM sample count"))?;
    drop(pcm);
    let model = read_model(checkpoint, control)?;
    let mut session = Session::from_checkpoint(model, control)?;
    let emissions = session.infer(&features, control)?;
    let frames =
        u32::try_from(emissions.frames).map_err(|_| native_error("emission frame count"))?;
    if frames
        != refinement::output_frames(u64::from(input_samples))
            .map_err(|_| native_error("emission shape"))?
    {
        return Err(native_error("emission shape differs from captured PCM"));
    }
    let mut bytes = Vec::with_capacity(emissions.log_probs.len() * 4);
    for value in emissions.log_probs {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let artifact = write_output(&workspace, output_scope, &bytes, maximum_bytes, control)?;
    control.check()?;
    Ok((artifact, input_samples, frames))
}

fn validate_directory(root: &OwnedFd, directory: &OwnedFd) -> Result<(), AlignError> {
    let root = rustix::fs::fstat(root).map_err(|_| native_error("inspect workspace root"))?;
    let target =
        rustix::fs::fstat(directory).map_err(|_| native_error("inspect workspace directory"))?;
    if root.st_uid != rustix::process::geteuid().as_raw()
        || target.st_uid != root.st_uid
        || target.st_dev != root.st_dev
    {
        return Err(native_error("workspace directory ownership or volume"));
    }
    Ok(())
}

/// Every workspace component is opened relative to the retained root, without
/// symlinks. No worker-selected relative path is resolved through cwd again.
fn open_directory(
    root: &OwnedFd,
    reference: &str,
    control: &Control,
) -> Result<OwnedFd, AlignError> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory = rustix::fs::openat(root, ".", flags, Mode::empty())
        .map_err(|_| native_error("open retained workspace"))?;
    for component in reference.split('/') {
        control.check()?;
        directory = rustix::fs::openat(&directory, component, flags, Mode::empty())
            .map_err(|_| native_error("open contained workspace directory"))?;
        validate_directory(root, &directory)?;
    }
    Ok(directory)
}

fn open_input(
    root: &OwnedFd,
    reference: &WorkspaceRef,
    control: &Control,
) -> Result<File, AlignError> {
    let path = Path::new(reference.as_str());
    if path.parent() != Some(Path::new("input")) {
        return Err(native_error("PCM must be directly below input"));
    }
    let name = path
        .file_name()
        .ok_or_else(|| native_error("PCM filename"))?;
    let parent = open_directory(root, "input", control)?;
    let descriptor = rustix::fs::openat(
        &parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| native_error("open contained PCM"))?;
    let metadata = rustix::fs::fstat(&descriptor).map_err(|_| native_error("inspect PCM"))?;
    let root_metadata =
        rustix::fs::fstat(root).map_err(|_| native_error("inspect workspace root"))?;
    if metadata.st_uid != root_metadata.st_uid
        || metadata.st_dev != root_metadata.st_dev
        || metadata.st_nlink != 1
    {
        return Err(native_error("PCM ownership, volume or links"));
    }
    Ok(File::from(descriptor))
}

fn read_model(model: &ModelInput, control: &Control) -> Result<Vec<u8>, AlignError> {
    control.check()?;
    let descriptor = rustix::fs::open(
        &model.path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| native_error("open pinned model input"))?;
    read_verified(
        File::from(descriptor),
        model.byte_length,
        model.sha256.as_str(),
        control,
    )
}

fn same_state(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.mode() == after.mode()
        && before.uid() == after.uid()
        && before.nlink() == after.nlink()
        && before.len() == after.len()
        && before.mtime() == after.mtime()
        && before.mtime_nsec() == after.mtime_nsec()
        && before.ctime() == after.ctime()
        && before.ctime_nsec() == after.ctime_nsec()
}

fn read_verified(
    mut file: File,
    expected: u64,
    hash: &str,
    control: &Control,
) -> Result<Vec<u8>, AlignError> {
    control.check()?;
    if expected == 0 || expected > refinement::CHECKPOINT_BYTES {
        return Err(native_error("file admission size"));
    }
    let before = file
        .metadata()
        .map_err(|_| native_error("inspect pinned input"))?;
    if !before.is_file() || before.len() != expected {
        return Err(native_error(
            "input must be regular with exact declared size",
        ));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(
            usize::try_from(expected).map_err(|_| native_error("input allocation bound"))?,
        )
        .map_err(|_| native_error("input allocation failed"))?;
    let mut hasher = Hasher::new();
    let mut buffer = [0_u8; CHUNK];
    loop {
        control.check()?;
        let read = file
            .read(&mut buffer)
            .map_err(|_| native_error("read pinned input"))?;
        control.check()?;
        if read == 0 {
            break;
        }
        if bytes
            .len()
            .checked_add(read)
            .is_none_or(|v| v as u64 > expected)
        {
            return Err(native_error("input grew while reading"));
        }
        hasher.update(&buffer[..read]);
        control.check()?;
        bytes.extend_from_slice(&buffer[..read]);
    }
    let after = file
        .metadata()
        .map_err(|_| native_error("reinspect pinned input"))?;
    if bytes.len() as u64 != expected || !same_state(&before, &after) {
        return Err(native_error("input changed while reading"));
    }
    let observed = hex(&hasher.finalize());
    control.check()?;
    if observed != hash {
        return Err(AlignError::Integrity(
            "input SHA256 differs from its declaration",
        ));
    }
    Ok(bytes)
}

fn write_output(
    root: &OwnedFd,
    scope: &WorkspaceRef,
    bytes: &[u8],
    maximum: u64,
    control: &Control,
) -> Result<WorkspaceArtifact, AlignError> {
    control.check()?;
    if bytes.is_empty() || bytes.len() as u64 > maximum || maximum > refinement::MAX_OUTPUT_BYTES {
        return Err(native_error("emission output byte budget"));
    }
    let directory = open_directory(root, scope.as_str(), control)?;
    let descriptor = rustix::fs::openat(
        &directory,
        refinement::OUTPUT_FILE,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|_| native_error("create contained emission output"))?;
    let mut output = File::from(descriptor);
    let mut hasher = Hasher::new();
    for chunk in bytes.chunks(CHUNK) {
        control.check()?;
        output
            .write_all(chunk)
            .map_err(|_| native_error("write emission output"))?;
        hasher.update(chunk);
        control.check()?;
    }
    output
        .flush()
        .map_err(|_| native_error("flush emission output"))?;
    output
        .sync_all()
        .map_err(|_| native_error("sync emission output"))?;
    control.check()?;
    let reference = WorkspaceRef::new(format!("{}/{}", scope.as_str(), refinement::OUTPUT_FILE))
        .map_err(|_| native_error("output reference"))?;
    let hash = Sha256::new(hex(&hasher.finalize())).map_err(|_| native_error("output SHA256"))?;
    WorkspaceArtifact::new(reference, hash, bytes.len() as u64)
        .map_err(|_| native_error("output declaration"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control() -> Control {
        Control::new(
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_secs(10),
        )
    }
    fn workspace(path: &Path) -> OwnedFd {
        rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap()
    }

    #[test]
    fn compiled_library_and_protocol_pins_match() {
        assert_eq!(deadpan_align::ENGINE_ID, refinement::ENGINE_ID);
        assert_eq!(deadpan_align::ORT_VERSION, refinement::ORT_VERSION);
        assert_eq!(
            deadpan_align::GLOBAL_STATS_SHA256,
            refinement::STATISTICS_SHA256
        );
        assert_eq!(
            deadpan_align::assembly::CHECKPOINT_SHA256,
            refinement::CHECKPOINT_SHA256
        );
        assert_eq!(
            deadpan_align::assembly::GRAPH_SHA256,
            refinement::GRAPH_SHA256
        );
        runtime().validate().unwrap();
    }

    #[test]
    fn bounded_reads_verify_exact_owned_bytes_and_observe_cancellation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, b"four").unwrap();
        let digest = hex(&Hasher::digest(b"four"));
        assert_eq!(
            read_verified(File::open(&path).unwrap(), 4, &digest, &control()).unwrap(),
            b"four"
        );
        assert!(read_verified(File::open(&path).unwrap(), 3, &digest, &control()).is_err());
        assert!(
            read_verified(
                File::open(&path).unwrap(),
                4,
                refinement::GRAPH_SHA256,
                &control()
            )
            .is_err()
        );
        assert!(
            read_verified(
                File::open(&path).unwrap(),
                refinement::CHECKPOINT_BYTES + 1,
                &digest,
                &control()
            )
            .is_err()
        );
        let cancelled = Control::new(
            Arc::new(AtomicBool::new(true)),
            Instant::now() + Duration::from_secs(10),
        );
        assert!(matches!(
            read_verified(File::open(&path).unwrap(), 4, &digest, &cancelled),
            Err(AlignError::Cancelled)
        ));
    }

    #[test]
    fn workspace_inputs_and_output_reject_symlinks_and_existing_files() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("input")).unwrap();
        std::fs::create_dir(directory.path().join("output")).unwrap();
        let root = workspace(directory.path());
        let outside = tempfile::NamedTempFile::new().unwrap();
        symlink(outside.path(), directory.path().join("input/audio")).unwrap();
        let reference = WorkspaceRef::new("input/audio").unwrap();
        assert!(open_input(&root, &reference, &control()).is_err());
        symlink(
            directory.path().join("output"),
            directory.path().join("redirect"),
        )
        .unwrap();
        assert!(open_directory(&root, "redirect", &control()).is_err());
        let scope = WorkspaceRef::new("output").unwrap();
        let first = write_output(&root, &scope, &[1, 2, 3, 4], 4, &control()).unwrap();
        assert_eq!(first.byte_length(), 4);
        assert!(write_output(&root, &scope, &[1, 2, 3, 4], 4, &control()).is_err());
        assert_eq!(
            std::fs::read(directory.path().join("output/emissions.f32le")).unwrap(),
            [1, 2, 3, 4]
        );
    }
}
