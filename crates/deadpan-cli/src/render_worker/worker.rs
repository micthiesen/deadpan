//! Private child process for committed SDR picture preparation.
//!
//! Output is a bounded, unpromoted raw I420 artifact. This entry does not encode,
//! publish an export, write authored state, or substitute a software GPU backend.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use deadpan_jobs::{Diagnostic, Sha256, WorkspaceArtifact, WorkspaceRef};
use deadpan_render::{PictureRenderer, Yuv420Policy};
use rustix::fs::{CWD, FileType, Mode, OFlags, fstat, openat};
use sha2::{Digest, Sha256 as Sha256Hasher};

use crate::export_picture::{ExportPictureContract, ExportPictureSession, OutputFrameOrdinal};
use crate::picture::ProjectPictureSession;

use super::protocol::{
    PICTURE_REF, PROTOCOL_VERSION, RenderContract, RenderHostMessage, RenderIdentity,
    RenderManifest, RenderPixelPolicy, RenderWorkerMessage, read_host_message,
    write_worker_message,
};

pub(crate) mod control;
use control::{ControlEnd, ControlPump, ControlReader};

type Result<T> = std::result::Result<T, String>;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_CHUNK_BYTES: usize = 64 * 1024;
const MAX_DIAGNOSTIC_BYTES: usize = 4_096;

struct Request {
    identity: RenderIdentity,
    contract: RenderContract,
    document_sha256: Sha256,
    maximum_output_bytes: u64,
}

/// Called only by the private `--render-picture-worker` dispatch. Before a
/// valid Prepare exists there is no request identity with which to frame an
/// error; such handshake failures use bounded stderr and a failed process exit.
pub(crate) fn entry(package: &Path) -> ExitCode {
    match run_entry(package) {
        Ok(success) if success => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("render picture worker: {}", diagnostic(&error).as_str());
            ExitCode::FAILURE
        }
    }
}

fn run_entry(package: &Path) -> Result<bool> {
    if !package.is_absolute() {
        return Err("render worker requires an absolute project package path".to_owned());
    }
    let handshake_deadline = Instant::now()
        .checked_add(HANDSHAKE_TIMEOUT)
        .ok_or("render handshake deadline overflow")?;
    let stdin = File::from(rustix::io::dup(io::stdin()).map_err(|error| error.to_string())?);
    let mut reader =
        ControlReader::new(stdin, Arc::new(AtomicBool::new(false)), handshake_deadline)
            .map_err(|error| error.to_string())?;
    let Some(RenderHostMessage::Prepare {
        identity,
        cancellation_token,
        contract,
        document_sha256,
        maximum_output_bytes,
        timeout_millis,
        ..
    }) = read_host_message(&mut reader)?
    else {
        return Err("render worker expected one Prepare message".to_owned());
    };
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(timeout_millis))
        .ok_or("render request deadline overflow")?;
    let request = Request {
        identity,
        contract: *contract,
        document_sha256,
        maximum_output_bytes,
    };
    let mut control = ControlPump::start(
        reader,
        request.identity.clone(),
        cancellation_token,
        deadline,
    )
    .map_err(|error| error.to_string())?;
    let mut stdout = io::stdout().lock();
    let prepared = prepare(
        package,
        &request,
        control.cancelled(),
        deadline,
        &mut stdout,
    );
    // Join before the terminal message. No child control thread or unreported
    // input error survives a claimed success.
    let terminal = match control.finish() {
        Ok(ControlEnd::Cancelled) => RenderWorkerMessage::Cancelled {
            protocol: PROTOCOL_VERSION,
            identity: request.identity,
        },
        Err(error) => RenderWorkerMessage::Failed {
            protocol: PROTOCOL_VERSION,
            identity: request.identity,
            diagnostic: diagnostic(&error),
        },
        Ok(ControlEnd::Stopped) => match prepared {
            Ok(manifest) => RenderWorkerMessage::Completed {
                protocol: PROTOCOL_VERSION,
                identity: request.identity,
                manifest: Box::new(manifest),
            },
            Err(error) => RenderWorkerMessage::Failed {
                protocol: PROTOCOL_VERSION,
                identity: request.identity,
                diagnostic: diagnostic(&error),
            },
        },
    };
    let success = !matches!(terminal, RenderWorkerMessage::Failed { .. });
    write_worker_message(&mut stdout, &terminal)?;
    stdout.flush().map_err(|error| error.to_string())?;
    Ok(success)
}

fn prepare(
    package: &Path,
    request: &Request,
    cancelled: &AtomicBool,
    deadline: Instant,
    stdout: &mut impl Write,
) -> Result<RenderManifest> {
    check_control(cancelled, deadline)?;
    request.contract.validate()?;
    let expected_bytes = request.contract.total_bytes()?;
    if expected_bytes > request.maximum_output_bytes {
        return Err("render pictures exceed the host's output byte budget".to_owned());
    }
    let pictures = ProjectPictureSession::open_revision(
        package,
        &request.contract.revision_id,
        Some(request.contract.range),
        cancelled,
    )
    .map_err(|error| error.to_string())?;
    check_control(cancelled, deadline)?;
    let captured = ExportPictureContract::capture(&pictures).map_err(|error| error.to_string())?;
    if !request.contract.matches(&captured) {
        return Err("committed project does not match the requested render contract".to_owned());
    }
    if super::document_sha256(&pictures, cancelled, deadline).map_err(|error| error.to_string())?
        != request.document_sha256
    {
        return Err("committed document does not match the requested SHA-256".to_owned());
    }
    check_control(cancelled, deadline)?;
    // Both immutable identity checks precede GPU allocation and any output file.
    let renderer = metal_renderer(cancelled, deadline)?;
    let mut session = ExportPictureSession::new(pictures, renderer, cancelled, deadline)
        .map_err(|error| error.to_string())?;
    check_control(cancelled, deadline)?;
    let mut output = open_output(Path::new("."), WorkerOutput::Pictures)?;
    let frame_bytes = request.contract.frame_bytes()?;
    let mut byte_length = 0_u64;
    let mut hasher = Sha256Hasher::new();
    for ordinal in 0..captured.frame_count() {
        check_control(cancelled, deadline)?;
        let ordinal = OutputFrameOrdinal(ordinal);
        let frame = session
            .prepare(ordinal, cancelled, deadline)
            .map_err(|error| error.to_string())?;
        let pixels = frame.pixels();
        if frame.contract() != &captured
            || frame.timing()
                != captured
                    .timing(ordinal)
                    .map_err(|error| error.to_string())?
            || [pixels.width(), pixels.height()] != captured.raster()
            || pixels
                .sdr()
                .is_none_or(|pixels| pixels.policy() != Yuv420Policy::Rec709LimitedLeft)
            || u64::try_from(pixels.bytes().len()).ok() != Some(frame_bytes)
        {
            return Err("prepared encoder picture changed its captured contract".to_owned());
        }
        byte_length = byte_length
            .checked_add(frame_bytes)
            .filter(|length| *length <= expected_bytes)
            .ok_or("render pictures exceed the captured byte count")?;
        write_chunks(
            &mut output,
            pixels.bytes(),
            &mut hasher,
            cancelled,
            deadline,
        )?;
        // Release the completed-frame permit before requesting the next one.
        drop(frame);
        let completed = ordinal.0.checked_add(1).ok_or("frame count overflow")?;
        if progress_due(completed, captured.frame_count()) {
            write_worker_message(
                stdout,
                &RenderWorkerMessage::Progress {
                    protocol: PROTOCOL_VERSION,
                    identity: request.identity.clone(),
                    completed_frames: completed,
                    total_frames: captured.frame_count(),
                },
            )?;
            stdout.flush().map_err(|error| error.to_string())?;
        }
    }
    check_control(cancelled, deadline)?;
    if byte_length != expected_bytes {
        return Err("render pictures ended before the captured byte count".to_owned());
    }
    output.sync_all().map_err(|error| error.to_string())?;
    check_control(cancelled, deadline)?;
    // Hash exactly the bytes written, in frame/Y/Cb/Cr order. The host separately
    // freezes and hashes this file after clean process and descendant teardown.
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").map_err(|error| error.to_string())?;
    }
    let planes = WorkspaceArtifact::new(
        WorkspaceRef::new(PICTURE_REF).map_err(|error| error.to_string())?,
        Sha256::new(hex).map_err(|error| error.to_string())?,
        byte_length,
    )
    .map_err(|error| error.to_string())?;
    Ok(RenderManifest {
        contract: request.contract.clone(),
        document_sha256: request.document_sha256.clone(),
        planes,
        pixel_policy: RenderPixelPolicy::I420Rec709LimitedLeft,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn metal_renderer(cancelled: &AtomicBool, deadline: Instant) -> Result<PictureRenderer> {
    check_control(cancelled, deadline)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .map_err(|error| error.to_string())?;
    check_control(cancelled, deadline)?;
    if adapter.get_info().backend != wgpu::Backend::Metal {
        return Err("render worker requires the qualified Metal backend".to_owned());
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Deadpan isolated committed render pictures"),
        ..Default::default()
    }))
    .map_err(|error| error.to_string())?;
    check_control(cancelled, deadline)?;
    Ok(PictureRenderer::new(&device, &queue))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn metal_renderer(
    _cancelled: &AtomicBool,
    _deadline: Instant,
) -> Result<PictureRenderer> {
    Err("isolated render pictures require the qualified macOS Metal backend".to_owned())
}

pub(crate) enum WorkerOutput {
    Pictures,
    Movie,
}

pub(crate) fn open_output(workspace: &Path, kind: WorkerOutput) -> Result<File> {
    let root = openat(
        CWD,
        workspace,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| error.to_string())?;
    let directory = openat(
        &root,
        "output",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| error.to_string())?;
    let root_metadata = fstat(&root).map_err(|error| error.to_string())?;
    let directory_metadata = fstat(&directory).map_err(|error| error.to_string())?;
    if root_metadata.st_uid != rustix::process::geteuid().as_raw()
        || directory_metadata.st_uid != root_metadata.st_uid
        || directory_metadata.st_dev != root_metadata.st_dev
    {
        return Err("render output directory changed filesystem or owner".to_owned());
    }
    let (name, access) = match kind {
        WorkerOutput::Pictures => ("pictures.i420", OFlags::WRONLY),
        WorkerOutput::Movie => ("movie.mp4", OFlags::RDWR),
    };
    let file = openat(
        &directory,
        name,
        access
            | OFlags::CREATE
            | OFlags::EXCL
            | OFlags::NOFOLLOW
            | OFlags::NONBLOCK
            | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|error| error.to_string())?;
    let metadata = fstat(&file).map_err(|error| error.to_string())?;
    if !FileType::from_raw_mode(metadata.st_mode).is_file()
        || metadata.st_nlink != 1
        || metadata.st_uid != root_metadata.st_uid
        || metadata.st_dev != root_metadata.st_dev
    {
        return Err("render output is not an owned private regular file".to_owned());
    }
    Ok(File::from(file))
}

fn write_chunks(
    output: &mut impl Write,
    bytes: &[u8],
    hasher: &mut Sha256Hasher,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<()> {
    for chunk in bytes.chunks(WRITE_CHUNK_BYTES) {
        check_control(cancelled, deadline)?;
        output.write_all(chunk).map_err(|error| error.to_string())?;
        hasher.update(chunk);
        check_control(cancelled, deadline)?;
    }
    Ok(())
}

pub(crate) fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        return Err("render preparation was cancelled".to_owned());
    }
    if Instant::now() >= deadline {
        return Err("render preparation exceeded its monotonic deadline".to_owned());
    }
    Ok(())
}

pub(crate) fn progress_due(completed: u64, total: u64) -> bool {
    completed == 1 || completed == total || completed.is_multiple_of(total.div_ceil(32))
}

pub(crate) fn diagnostic(error: &str) -> Diagnostic {
    let mut end = error.len().min(MAX_DIAGNOSTIC_BYTES);
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    let message = if end == 0 {
        "render preparation failed".to_owned()
    } else {
        error[..end].replace('\0', "?")
    };
    Diagnostic::new(message).expect("bounded nonempty diagnostic without NUL")
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn output_creation_is_exclusive_and_rejects_symlink_components() {
        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::tempdir().expect("outside");
        symlink(outside.path(), workspace.path().join("output")).expect("directory symlink");
        assert!(open_output(workspace.path(), WorkerOutput::Pictures).is_err());
        assert!(!outside.path().join("pictures.i420").exists());
        std::fs::remove_file(workspace.path().join("output")).expect("remove symlink");
        std::fs::create_dir(workspace.path().join("output")).expect("output directory");
        let original = outside.path().join("original");
        std::fs::write(&original, b"preserve").expect("original");
        let path = workspace.path().join("output/pictures.i420");
        symlink(&original, &path).expect("file symlink");
        assert!(open_output(workspace.path(), WorkerOutput::Pictures).is_err());
        assert_eq!(
            std::fs::read(&original).expect("retained original"),
            b"preserve"
        );
        std::fs::remove_file(&path).expect("remove file symlink");
        let mut output =
            open_output(workspace.path(), WorkerOutput::Pictures).expect("new contained output");
        output.write_all(b"first").expect("first write");
        assert!(open_output(workspace.path(), WorkerOutput::Pictures).is_err());
        assert_eq!(
            std::fs::read(&path).expect("retained first output"),
            b"first"
        );
    }

    #[test]
    fn picture_writes_stop_between_bounded_chunks_and_keep_partial_data() {
        struct CancellingWriter<'a> {
            cancelled: &'a AtomicBool,
            bytes: Vec<u8>,
        }
        impl Write for CancellingWriter<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                assert!(bytes.len() <= WRITE_CHUNK_BYTES);
                self.bytes.extend_from_slice(bytes);
                self.cancelled.store(true, Ordering::Release);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let cancelled = AtomicBool::new(false);
        let mut output = CancellingWriter {
            cancelled: &cancelled,
            bytes: Vec::new(),
        };
        let bytes = vec![19; WRITE_CHUNK_BYTES + 1];
        let mut hasher = Sha256Hasher::new();
        let error = write_chunks(
            &mut output,
            &bytes,
            &mut hasher,
            &cancelled,
            Instant::now() + Duration::from_secs(1),
        )
        .expect_err("cancelled after first chunk");
        assert!(error.contains("cancelled"));
        assert_eq!(output.bytes, bytes[..WRITE_CHUNK_BYTES]);
        assert_eq!(hasher.finalize(), Sha256Hasher::digest(&output.bytes));
    }

    #[test]
    fn progress_is_bounded_and_diagnostics_end_on_utf8_boundaries() {
        for total in [1, 2, 31, 32, 33, 99_999, 100_000] {
            let emitted: Vec<_> = (1..=total)
                .filter(|completed| progress_due(*completed, total))
                .collect();
            assert!(emitted.len() <= 34);
            assert_eq!(emitted.first(), Some(&1));
            assert_eq!(emitted.last(), Some(&total));
        }
        let text = "é".repeat(MAX_DIAGNOSTIC_BYTES);
        let clipped = diagnostic(&text);
        assert!(clipped.as_str().len() <= MAX_DIAGNOSTIC_BYTES);
        assert!(text.starts_with(clipped.as_str()));
        assert_eq!(diagnostic("bad\0text").as_str(), "bad?text");
        assert!(!diagnostic("").as_str().is_empty());
    }
}
