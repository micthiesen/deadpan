//! Bounded capture and sole reaping ownership for system codesign commands.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use super::super::{Control, invalid};

const OUTPUT_LIMIT: u64 = 64 * 1024;
const CLEANUP_DURATION: Duration = Duration::from_secs(2);

fn owned_exit(child: &Child) -> io::Result<bool> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    Ok(waitid(
        WaitId::Pid(Pid::from_child(child)),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )?
    .is_some_and(|status| status.exited() || status.killed() || status.dumped()))
}

/// Consumes the only child handle, so no exit path can retry a reaped PID.
/// The fallback may reap the leader but never conceals failed group cleanup.
fn stop_and_reap(mut child: Child) -> io::Result<ExitStatus> {
    let deadline = Instant::now() + CLEANUP_DURATION;
    #[cfg(target_os = "macos")]
    let group = deadpan_native_process::terminate_owned_group(&child, deadline);
    #[cfg(target_os = "linux")]
    let group = deadpan_native_process::signal_owned_group(&child)
        .and_then(|()| deadpan_native_process::terminate_owned_leader(&child, deadline));
    let leader = if group.is_err() {
        deadpan_native_process::terminate_owned_leader(&child, Instant::now() + CLEANUP_DURATION)
    } else {
        Ok(())
    };
    if let Err(leader) = leader {
        // Reap at most once, nonblocking, when exit could not be confirmed.
        // Never signal again after this ownership/reap attempt.
        let reaping = child.try_wait();
        return Err(io::Error::other(format!(
            "signature command cleanup unconfirmed: group={group:?}; leader={leader}; final reap={reaping:?}"
        )));
    }
    let reaped = child.wait();
    match (group, reaped) {
        (Ok(()), Ok(status)) => Ok(status),
        (group, reaped) => Err(io::Error::other(format!(
            "signature command cleanup/reap failed: group={group:?}; reap={reaped:?}"
        ))),
    }
}

fn observe(
    child: &Child,
    output: &File,
    control: Control<'_>,
    exited: impl Fn(&Child) -> io::Result<bool>,
) -> io::Result<()> {
    loop {
        let terminal = exited(child)?;
        control.check()?;
        if output.metadata()?.len() > OUTPUT_LIMIT {
            return Err(invalid("signature command output exceeded its bound"));
        }
        if terminal {
            return Ok(());
        }
        std::thread::park_timeout(Duration::from_millis(5));
    }
}

fn capture_with(
    command: &mut Command,
    control: Control<'_>,
    exited: impl Fn(&Child) -> io::Result<bool>,
) -> io::Result<String> {
    control.check()?;
    let mut output = tempfile::tempfile()?;
    command
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output.try_clone()?))
        .process_group(0);
    control.check()?;
    let child = deadpan_native_process::spawn(command)?;
    // All fallible work after spawn is contained by this result. Teardown
    // always runs, including metadata/ownership failures and interruption.
    let observation = observe(&child, &output, control, exited);
    let cleanup = stop_and_reap(child);
    let status = match (observation, cleanup) {
        (Ok(()), Ok(status)) => status,
        (Err(error), Ok(_)) => return Err(error),
        (observation, Err(cleanup)) => {
            return Err(io::Error::other(format!(
                "signature command failed: observation={observation:?}; {cleanup}"
            )));
        }
    };
    control.check()?;
    if output.metadata()?.len() > OUTPUT_LIMIT {
        return Err(invalid("signature command output exceeded its bound"));
    }
    output.seek(SeekFrom::Start(0))?;
    let mut text = String::new();
    output.take(OUTPUT_LIMIT + 1).read_to_string(&mut text)?;
    control.check()?;
    if !status.success() {
        return Err(invalid(format!(
            "signature command failed: {}",
            text.trim()
        )));
    }
    Ok(text)
}

pub(super) fn capture(command: &mut Command, control: Control<'_>) -> io::Result<String> {
    capture_with(command, control, owned_exit)
}

#[cfg(test)]
mod tests;
