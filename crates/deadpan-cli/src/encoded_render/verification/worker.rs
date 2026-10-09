use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
    process::ExitCode,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use deadpan_jobs::{CancellationToken, Sha256};
use sha2::{Digest, Sha256 as Hasher};

use super::{
    VerificationReport,
    host::open_input,
    inspect,
    protocol::{self, HostMessage, WorkerMessage},
};
use crate::render_worker::{
    protocol::RenderIdentity,
    worker::{
        check_control,
        control::{ControlEnd, ControlPump, ControlReader},
        diagnostic,
    },
};

#[cfg(test)]
mod tests;

pub(crate) fn entry() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("render verification: {}", diagnostic(&error).as_str());
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<bool, String> {
    let handshake = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("handshake clock overflow")?;
    let stdin = File::from(rustix::io::dup(io::stdin()).map_err(|e| e.to_string())?);
    let mut reader = ControlReader::new(stdin, Arc::new(AtomicBool::new(false)), handshake)
        .map_err(|e| e.to_string())?;
    let Some(HostMessage::Inspect {
        identity,
        cancellation_token,
        manifest,
        limits,
        timeout_millis,
        ..
    }) = protocol::read_host(&mut reader)?
    else {
        return Err("verification worker expected one Inspect".into());
    };
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(timeout_millis))
        .ok_or("verification clock overflow")?;
    let expected = identity.clone();
    let mut control = ControlPump::start_with(reader, deadline, move |reader| {
        receive_control(reader, &expected, &cancellation_token)
    })
    .map_err(|e| e.to_string())?;
    let mut stdout = io::stdout().lock();
    let result = (|| {
        check_control(control.cancelled(), deadline)?;
        let file = open_input(Path::new("."), false).map_err(|e| e.to_string())?;
        let before = file.metadata().map_err(|e| e.to_string())?;
        if before.len() != manifest.movie.byte_length() || before.len() > limits.maximum_bytes {
            return Err("verification input extent differs from captured candidate".into());
        }
        if hash(
            &file,
            manifest.movie.byte_length(),
            control.cancelled(),
            deadline,
        )? != *manifest.movie.sha256()
        {
            return Err("verification input hash differs from captured candidate".into());
        }
        let report = inspect::inspect(
            &file,
            &manifest,
            limits,
            control.cancelled(),
            deadline,
            |progress| {
                protocol::write_worker(
                    &mut stdout,
                    &WorkerMessage::Progress {
                        protocol: protocol::VERSION,
                        identity: identity.clone(),
                        progress,
                    },
                )?;
                #[cfg(feature = "qualification-render-host-crash")]
                if progress.stage == super::VerificationStage::Pictures {
                    stdout.flush().map_err(|error| error.to_string())?;
                    super::super::host_crash_gate::after_progress(
                        "verification",
                        &identity,
                        progress.completed,
                        progress.total,
                        control.cancelled(),
                        deadline,
                    )?;
                }
                Ok(())
            },
        )?;
        use std::os::unix::fs::MetadataExt;
        let after = file.metadata().map_err(|e| e.to_string())?;
        if before.len() != after.len()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
            || hash(
                &file,
                manifest.movie.byte_length(),
                control.cancelled(),
                deadline,
            )? != *manifest.movie.sha256()
        {
            return Err("verification input changed during decode".into());
        }
        report.validate(limits)?;
        protocol::bind(&report, &manifest)?;
        Ok::<VerificationReport, String>(report)
    })();
    let terminal = match control.finish() {
        Ok(ControlEnd::Cancelled) => WorkerMessage::Cancelled {
            protocol: protocol::VERSION,
            identity,
        },
        Err(error) => WorkerMessage::Failed {
            protocol: protocol::VERSION,
            identity,
            diagnostic: diagnostic(&error),
        },
        Ok(ControlEnd::Stopped) => match result {
            Ok(report) => WorkerMessage::Completed {
                protocol: protocol::VERSION,
                identity,
                report: Box::new(report),
            },
            Err(error) => WorkerMessage::Failed {
                protocol: protocol::VERSION,
                identity,
                diagnostic: diagnostic(&error),
            },
        },
    };
    let success = !matches!(terminal, WorkerMessage::Failed { .. });
    protocol::write_worker(&mut stdout, &terminal)?;
    stdout.flush().map_err(|e| e.to_string())?;
    Ok(success)
}

fn receive_control(
    reader: &mut impl Read,
    identity: &RenderIdentity,
    token: &CancellationToken,
) -> Result<ControlEnd, String> {
    match protocol::read_host(reader)? {
        Some(HostMessage::Cancel {
            identity: actual,
            cancellation_token,
            ..
        }) if &actual == identity && &cancellation_token == token => Ok(ControlEnd::Cancelled),
        Some(HostMessage::Cancel { .. }) => {
            Err("verification cancellation identity or token differs".into())
        }
        Some(HostMessage::Inspect { .. }) => {
            Err("verification worker received a second Inspect".into())
        }
        None => Err("verification host closed its control stream".into()),
    }
}

fn hash(
    file: &File,
    length: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Sha256, String> {
    use std::os::unix::fs::FileExt;
    if file.metadata().map_err(|e| e.to_string())?.len() != length {
        return Err("verification input extent changed before hashing".into());
    }
    let mut offset = 0;
    let mut buffer = [0_u8; 64 * 1024];
    let mut hash = Hasher::new();
    while offset < length {
        check_control(cancelled, deadline)?;
        let count =
            usize::try_from((length - offset).min(buffer.len() as u64)).expect("bounded chunk");
        file.read_exact_at(&mut buffer[..count], offset)
            .map_err(|e| e.to_string())?;
        hash.update(&buffer[..count]);
        offset += count as u64;
    }
    check_control(cancelled, deadline)?;
    Sha256::new(
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .map_err(|e| e.to_string())
}
