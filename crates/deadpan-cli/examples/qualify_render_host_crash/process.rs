use std::{
    fs::File,
    io::Read,
    os::unix::process::CommandExt,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use serde::Serialize;

use super::Result;

/// Sole reaping ownership. Failure cleanup may stop this owned child group;
/// the measured crash path explicitly kills only its host leader.
pub(super) struct OwnedChild {
    child: Child,
    reap_attempted: bool,
}

impl OwnedChild {
    pub(super) fn spawn(command: &mut Command) -> Result<Self> {
        command.process_group(0);
        Ok(Self {
            child: deadpan_native_process::spawn(command)?,
            reap_attempted: false,
        })
    }

    pub(super) fn id(&self) -> u32 {
        self.child.id()
    }

    pub(super) fn exited(&self) -> Result<bool> {
        if self.reap_attempted {
            return Err("owned child already had its sole reap attempt".into());
        }
        Ok(deadpan_native_process::exited_leader_has_no_other_members(
            &self.child,
        )?)
    }

    pub(super) fn reap(&mut self) -> Result<ExitStatus> {
        if !self.exited()? {
            return Err("cannot reap before child exit and group cleanup".into());
        }
        self.reap_attempted = true;
        Ok(self.child.wait()?)
    }

    pub(super) fn kill_host_only(&mut self) -> Result<ExitStatus> {
        if self.exited()? {
            return Err("host exited before the measured SIGKILL".into());
        }
        // This is deliberately Child::kill, NOT owned-group teardown. Helpers
        // have their own groups and must independently observe the lost host.
        self.child.kill()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.exited()? {
            if Instant::now() >= deadline {
                return Err("SIGKILL host did not exit before deadline".into());
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
        self.reap()
    }

    /// Explicit error cleanup lets the harness observe the helpers' separate
    /// groups after the lost host closes their control streams. Preserve group
    /// failures even when the checked leader fallback can finish reaping.
    pub(super) fn stop_after_failure(&mut self) -> Result {
        if self.reap_attempted {
            return Ok(());
        }
        let group_error = deadpan_native_process::terminate_owned_group(
            &self.child,
            Instant::now() + Duration::from_secs(2),
        )
        .err();
        if let Some(group_error) = &group_error {
            deadpan_native_process::terminate_owned_leader(
                &self.child,
                Instant::now() + Duration::from_secs(2),
            )
            .map_err(|leader_error| {
                format!("owned-group cleanup: {group_error}; owned-leader cleanup: {leader_error}")
            })?;
        }
        self.reap_attempted = true;
        let reaped = self.child.wait();
        match (group_error, reaped) {
            (None, Ok(_)) => Ok(()),
            (Some(error), Ok(_)) => Err(format!(
                "owned-group cleanup failed despite checked leader cleanup: {error}"
            )
            .into()),
            (None, Err(error)) => Err(format!("owned-child reap failed: {error}").into()),
            (Some(group), Err(reap)) => {
                Err(format!("owned-group cleanup: {group}; owned-child reap: {reap}").into())
            }
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Err(error) = self.stop_after_failure() {
            eprintln!("qualification cleanup failed: {error}");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct ProcessIdentity {
    pub pid: u32,
    pub parent: u32,
    pub group: u32,
    /// ps lstart (five fields) followed by the executable's comm value.
    pub start_and_executable: String,
}

/// Read-only observation, including zombies. Never signals the recorded PIDs.
/// A process-table error or malformed/truncated result fails closed.
pub(super) fn snapshot() -> Result<Vec<ProcessIdentity>> {
    const LIMIT: u64 = 8 * 1024 * 1024;
    let output = tempfile::tempfile()?;
    let error = tempfile::tempfile()?;
    let mut command = Command::new("ps");
    command
        .args(["-axo", "pid=,ppid=,pgid=,lstart=,comm="])
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(error.try_clone()?);
    let mut child = OwnedChild::spawn(&mut command)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !child.exited()? {
        if output.metadata()?.len() > LIMIT || error.metadata()?.len() > LIMIT {
            return Err("ps output exceeded qualification bound".into());
        }
        if Instant::now() >= deadline {
            return Err("ps observation timed out".into());
        }
        std::thread::park_timeout(Duration::from_millis(2));
    }
    let status = child.reap()?;
    let output = read_bounded(output, LIMIT)?;
    let error = read_bounded(error, LIMIT)?;
    if !status.success() || !error.is_empty() {
        return Err(format!("ps observation failed: {status}: {error}").into());
    }
    let mut processes = Vec::new();
    for line in output.lines() {
        let mut columns = line.split_whitespace();
        let pid = columns.next().ok_or("ps missing pid")?.parse()?;
        let parent = columns.next().ok_or("ps missing parent")?.parse()?;
        let group = columns.next().ok_or("ps missing group")?.parse()?;
        let identity: Vec<_> = columns.collect();
        if identity.len() < 6 || processes.len() >= 65_536 {
            return Err("ps identity fields missing or process count exceeded".into());
        }
        processes.push(ProcessIdentity {
            pid,
            parent,
            group,
            start_and_executable: identity.join(" "),
        });
    }
    if processes.is_empty() {
        return Err("ps returned no process identities".into());
    }
    Ok(processes)
}

fn read_bounded(mut file: File, limit: u64) -> Result<String> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len())? > limit {
        return Err("process output grew past bound".into());
    }
    Ok(String::from_utf8(bytes)?)
}
