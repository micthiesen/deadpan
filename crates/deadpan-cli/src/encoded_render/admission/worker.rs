//! Private synthetic encoder probe. It owns no project or publication authority.

use std::{
    fs::File,
    io::{self, Read, Write},
    os::unix::fs::MetadataExt,
    path::Path,
    process::ExitCode,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use deadpan_encode::{EncodeError, EncodeLimits, EncoderSession, NextInput};
use deadpan_jobs::{CancellationToken, WorkspaceArtifact, WorkspaceRef};
use rustix::fs::{CWD, Mode, OFlags, openat};

use super::{
    ProbeReport, ProbeSpec, content,
    protocol::{self, HostMessage, PROTOCOL_VERSION, WorkerMessage},
};
use crate::{
    encoded_render::{
        protocol::{EncodedFailure, EncodedFailureKind, EncodedManifest, MOVIE_REF},
        verification::{self, VerificationLimits},
        worker::hash_movie,
    },
    render_worker::{
        protocol::RenderIdentity,
        worker::{
            WorkerOutput, check_control as check_worker_control,
            control::{ControlEnd, ControlPump, ControlReader},
            diagnostic, open_output, progress_due,
        },
    },
};

type Result<T> = std::result::Result<T, EncodedFailure>;

pub(crate) fn entry() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("render encoder probe: {}", diagnostic(&error).as_str());
            ExitCode::FAILURE
        }
    }
}

fn run() -> std::result::Result<bool, String> {
    let handshake = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("probe handshake clock overflow")?;
    let stdin = File::from(rustix::io::dup(io::stdin()).map_err(|error| error.to_string())?);
    let mut reader = ControlReader::new(stdin, Arc::new(AtomicBool::new(false)), handshake)
        .map_err(|error| error.to_string())?;
    let Some(HostMessage::Probe {
        identity,
        cancellation_token,
        spec,
        limits,
        timeout_millis,
        ..
    }) = protocol::read_host_message(&mut reader)?
    else {
        return Err("encoder probe worker expected one Probe message".into());
    };
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(timeout_millis))
        .ok_or("encoder probe clock overflow")?;
    let control_identity = identity.clone();
    let mut control = ControlPump::start_with(reader, deadline, move |reader| {
        receive_control(reader, &control_identity, &cancellation_token)
    })
    .map_err(|error| error.to_string())?;
    let mut stdout = io::stdout().lock();
    let prepared = prepare(
        &identity,
        &spec,
        limits,
        control.cancelled(),
        deadline,
        &mut stdout,
    );
    // Drain queued cancellation and malformed controls before trusting any
    // terminal, including a typed native rejection.
    let terminal = match control.finish() {
        Ok(ControlEnd::Cancelled) => WorkerMessage::Cancelled {
            protocol: PROTOCOL_VERSION,
            identity,
        },
        Err(error) => WorkerMessage::Failed {
            protocol: PROTOCOL_VERSION,
            identity,
            failure: failure(EncodedFailureKind::Control, error),
        },
        Ok(ControlEnd::Stopped) => match prepared {
            Ok(report) => WorkerMessage::Completed {
                protocol: PROTOCOL_VERSION,
                identity,
                report: Box::new(report),
            },
            Err(failure) => WorkerMessage::Failed {
                protocol: PROTOCOL_VERSION,
                identity,
                failure,
            },
        },
    };
    let success = !matches!(terminal, WorkerMessage::Failed { .. });
    protocol::write_worker_message(&mut stdout, &terminal)?;
    stdout.flush().map_err(|error| error.to_string())?;
    Ok(success)
}

fn receive_control(
    reader: &mut impl Read,
    identity: &RenderIdentity,
    token: &CancellationToken,
) -> std::result::Result<ControlEnd, String> {
    match protocol::read_host_message(reader)? {
        Some(HostMessage::Cancel {
            identity: received,
            cancellation_token,
            ..
        }) if &received == identity && &cancellation_token == token => Ok(ControlEnd::Cancelled),
        Some(HostMessage::Cancel { .. }) => {
            Err("probe cancellation identity or token differs".into())
        }
        Some(HostMessage::Probe { .. }) => {
            Err("encoder probe received a second Probe message".into())
        }
        None => Err("encoder probe host closed its control stream".into()),
    }
}

fn prepare(
    identity: &RenderIdentity,
    spec: &ProbeSpec,
    limits: EncodeLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    stdout: &mut impl Write,
) -> Result<ProbeReport> {
    check_control(cancelled, deadline)?;
    let generator = spec
        .generator()
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    let contract = spec
        .contract()
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    let native = contract
        .native_contract()
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    limits
        .validate_for(&native)
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    let document_sha256 = spec
        .document_sha256()
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    let config = generator.config();
    let length = usize::try_from(config.picture_bytes)
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    let mut picture = Vec::new();
    picture
        .try_reserve_exact(length)
        .map_err(|error| failure(EncodedFailureKind::Picture, error))?;
    picture.resize(length, 0);
    let mut left = [0_f32; 1024];
    let mut right = [0_f32; 1024];
    let output = open_output(Path::new("."), WorkerOutput::Movie)
        .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    let mut encoder = EncoderSession::open(output, native, limits, cancelled, deadline)
        .map_err(encoder_failure)?;
    loop {
        check_control(cancelled, deadline)?;
        match encoder.next_input().map_err(encoder_failure)? {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => {
                generator
                    .fill_picture(ordinal, &mut picture)
                    .map_err(|error| failure(EncodedFailureKind::Picture, error))?;
                encoder
                    .push_picture(ordinal, pts, duration, &picture)
                    .map_err(encoder_failure)?;
                let completed = encoder.accepted_pictures();
                if progress_due(completed, config.video_frames) {
                    protocol::write_worker_message(
                        stdout,
                        &WorkerMessage::Progress {
                            protocol: PROTOCOL_VERSION,
                            identity: identity.clone(),
                            completed_frames: completed,
                            total_frames: config.video_frames,
                        },
                    )
                    .map_err(|error| failure(EncodedFailureKind::Control, error))?;
                    stdout
                        .flush()
                        .map_err(|error| failure(EncodedFailureKind::Control, error))?;
                }
            }
            NextInput::Audio {
                first_sample,
                samples,
            } => {
                let count = usize::try_from(samples)
                    .ok()
                    .filter(|&count| (1..=1024).contains(&count))
                    .ok_or_else(|| {
                        failure(
                            EncodedFailureKind::Contract,
                            "probe audio block exceeds bound",
                        )
                    })?;
                generator
                    .fill_audio(first_sample, &mut left[..count], &mut right[..count])
                    .map_err(|error| failure(EncodedFailureKind::Audio, error))?;
                encoder
                    .push_audio(first_sample, &left[..count], &right[..count])
                    .map_err(encoder_failure)?;
            }
            NextInput::Finish => break,
        }
    }
    // finish consumes and drops the native encoder before any decoder exists.
    let (mut output, report) = encoder.finish().map_err(encoder_failure)?.into_parts();
    drop(picture);
    check_control(cancelled, deadline)?;
    let hash = hash_movie(
        &mut output,
        report.output_bytes,
        limits.maximum_output_bytes,
        cancelled,
        deadline,
    )?;
    let movie = WorkspaceArtifact::new(
        WorkspaceRef::new(MOVIE_REF).map_err(|error| failure(EncodedFailureKind::Output, error))?,
        hash,
        report.output_bytes,
    )
    .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    let manifest = EncodedManifest {
        contract,
        document_sha256,
        movie,
        report,
    };
    manifest
        .validate_for(limits)
        .map_err(|error| failure(EncodedFailureKind::Contract, error))?;
    let mut input = read_only_output(Path::new("."), &output)
        .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    drop(output);
    let before = input
        .metadata()
        .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    let verification_limits = VerificationLimits {
        maximum_bytes: limits.maximum_output_bytes,
        maximum_packets: limits.maximum_packets.min(1_000_000),
    };
    let verification = verification::inspect_file(
        &input,
        &manifest,
        verification_limits,
        cancelled,
        deadline,
        |_| Ok(()),
    )
    .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    let content = content::inspect(
        &input,
        spec,
        verification_limits.maximum_bytes,
        verification_limits.maximum_packets,
        cancelled,
        deadline,
    )
    .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    let after = input
        .metadata()
        .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    if !same_file(&before, &after)
        || hash_movie(
            &mut input,
            manifest.movie.byte_length(),
            limits.maximum_output_bytes,
            cancelled,
            deadline,
        )? != *manifest.movie.sha256()
    {
        return Err(failure(
            EncodedFailureKind::Output,
            "probe bytes changed during verification",
        ));
    }
    let result = ProbeReport {
        schema_version: 1,
        spec: spec.clone(),
        manifest,
        verification,
        content,
    };
    result
        .validate(limits)
        .map_err(|error| failure(EncodedFailureKind::Output, error))?;
    check_control(cancelled, deadline)?;
    Ok(result)
}

fn read_only_output(workspace: &Path, encoded: &File) -> std::result::Result<File, String> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let root = File::from(
        openat(CWD, workspace, flags, Mode::empty()).map_err(|error| error.to_string())?,
    );
    let directory = File::from(
        openat(&root, "output", flags, Mode::empty()).map_err(|error| error.to_string())?,
    );
    let root_metadata = root.metadata().map_err(|error| error.to_string())?;
    let directory_metadata = directory.metadata().map_err(|error| error.to_string())?;
    if root_metadata.uid() != rustix::process::geteuid().as_raw()
        || directory_metadata.uid() != root_metadata.uid()
        || directory_metadata.dev() != root_metadata.dev()
    {
        return Err("probe output directory changed owner or filesystem".into());
    }
    let input = File::from(
        openat(
            &directory,
            "movie.mp4",
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| error.to_string())?,
    );
    let expected = encoded.metadata().map_err(|error| error.to_string())?;
    let actual = input.metadata().map_err(|error| error.to_string())?;
    if !same_file(&expected, &actual)
        || actual.uid() != root_metadata.uid()
        || actual.dev() != root_metadata.dev()
    {
        return Err("probe read-only descriptor differs from the encoded output".into());
    }
    Ok(input)
}

fn same_file(expected: &std::fs::Metadata, actual: &std::fs::Metadata) -> bool {
    expected.is_file()
        && actual.is_file()
        && expected.nlink() == 1
        && actual.nlink() == 1
        && expected.dev() == actual.dev()
        && expected.ino() == actual.ino()
        && expected.uid() == actual.uid()
        && expected.len() == actual.len()
        && expected.mtime() == actual.mtime()
        && expected.mtime_nsec() == actual.mtime_nsec()
        && expected.ctime() == actual.ctime()
        && expected.ctime_nsec() == actual.ctime_nsec()
}

fn failure(kind: EncodedFailureKind, error: impl std::fmt::Display) -> EncodedFailure {
    EncodedFailure {
        kind,
        diagnostic: diagnostic(&error.to_string()),
    }
}

fn encoder_failure(error: EncodeError) -> EncodedFailure {
    failure(EncodedFailureKind::Encoder(error.kind()), error)
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<()> {
    check_worker_control(cancelled, deadline)
        .map_err(|error| failure(EncodedFailureKind::Control, error))
}

#[cfg(test)]
mod tests {
    use std::{os::fd::OwnedFd, os::unix::net::UnixStream, sync::atomic::Ordering};

    use deadpan_jobs::{AttemptId, RequestId, write_frame};

    use super::*;

    fn identity() -> RenderIdentity {
        RenderIdentity {
            request_id: RequestId::new("probe-controls").unwrap(),
            attempt_id: AttemptId::new("probe-attempt").unwrap(),
        }
    }

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(2)
    }

    fn pair() -> (UnixStream, ControlReader) {
        let (host, child) = UnixStream::pair().unwrap();
        let reader = ControlReader::new(
            File::from(OwnedFd::from(child)),
            Arc::new(AtomicBool::new(false)),
            deadline(),
        )
        .unwrap();
        (host, reader)
    }

    fn pump(reader: ControlReader) -> ControlPump {
        ControlPump::start_with(reader, deadline(), |reader| {
            receive_control(
                reader,
                &identity(),
                &CancellationToken::new("token").unwrap(),
            )
        })
        .unwrap()
    }

    #[test]
    fn queued_probe_controls_are_drained_before_any_terminal_claim() {
        for (version, token, valid) in [
            (PROTOCOL_VERSION, "token", true),
            (99, "token", false),
            (PROTOCOL_VERSION, "stale", false),
        ] {
            let (mut host, reader) = pair();
            write_frame(
                &mut host,
                &HostMessage::Cancel {
                    protocol: version,
                    identity: identity(),
                    cancellation_token: CancellationToken::new(token).unwrap(),
                },
            )
            .unwrap();
            let mut control = pump(reader);
            if valid {
                assert_eq!(control.finish(), Ok(ControlEnd::Cancelled));
            } else {
                assert!(control.finish().is_err());
            }
            assert!(control.cancelled().load(Ordering::Acquire));
        }
        let (mut host, reader) = pair();
        host.write_all(&[0, 0]).unwrap();
        assert!(pump(reader).finish().unwrap_err().contains("incomplete"));
        let (host, reader) = pair();
        drop(host);
        assert!(pump(reader).finish().unwrap_err().contains("closed"));
    }

    #[test]
    fn readonly_probe_input_is_the_same_owned_file_and_rejects_replacements() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::create_dir(workspace.path().join("output")).unwrap();
        let mut output = open_output(workspace.path(), WorkerOutput::Movie).unwrap();
        output.write_all(b"owned bytes").unwrap();
        let mut input = read_only_output(workspace.path(), &output).unwrap();
        assert!(input.write_all(b"x").is_err());
        let mut bytes = String::new();
        input.read_to_string(&mut bytes).unwrap();
        assert_eq!(bytes, "owned bytes");
        std::fs::rename(
            workspace.path().join("output/movie.mp4"),
            workspace.path().join("output/retained.mp4"),
        )
        .unwrap();
        std::fs::write(workspace.path().join("output/movie.mp4"), b"other bytes").unwrap();
        assert!(read_only_output(workspace.path(), &output).is_err());
    }

    #[test]
    fn nonencoder_probe_errors_cannot_claim_capability_through_diagnostics() {
        for kind in [
            EncodedFailureKind::Control,
            EncodedFailureKind::Contract,
            EncodedFailureKind::Picture,
            EncodedFailureKind::Audio,
            EncodedFailureKind::Output,
        ] {
            assert_eq!(
                failure(kind, "video_encoder_unavailable video_timestamp_order").kind,
                kind
            );
        }
    }
}
