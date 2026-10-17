//! Bounded system observations for background-work admission.
//! Call from preparation threads, never from the UI or project writer.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Why background work should wait now: Low Power Mode, battery power
/// or thermal pressure, as `pmset` reports them. None when it may run, or
/// when the state cannot be read.
pub fn power_guard() -> Option<String> {
    let batt = pmset(&["-g", "batt"])?;
    if batt.contains("'Battery Power'") {
        return Some("running on battery".into());
    }
    let settings = pmset(&["-g"]).unwrap_or_default();
    if settings.lines().any(|line| {
        let mut words = line.split_whitespace();
        matches!(
            (words.next(), words.next()),
            (Some("lowpowermode" | "powermode"), Some("1"))
        )
    }) {
        return Some("Low Power Mode is on".into());
    }
    let thermal = pmset(&["-g", "therm"]).unwrap_or_default();
    let limited = thermal.lines().any(|line| {
        line.split_once('=').is_some_and(|(name, value)| {
            name.trim().ends_with("Speed_Limit")
                && value.trim().parse::<u32>().is_ok_and(|limit| limit < 100)
        })
    });
    let warned = thermal
        .lines()
        .any(|line| line.contains("warning level") && !line.contains("No "));
    (limited || warned).then(|| "the Mac is under thermal pressure".into())
}

/// The kernel's memory pressure level (`kern.memorystatus_vm_pressure_level`:
/// 1 normal, 2 warning, 4 critical), read through `/usr/sbin/sysctl` with
/// the same bounded runner as `pmset`. None when it cannot be read.
pub fn memory_pressure_level() -> Option<u32> {
    system_tool(
        "/usr/sbin/sysctl",
        &["-n", "kern.memorystatus_vm_pressure_level"],
    )?
    .trim()
    .parse()
    .ok()
}

fn pmset(arguments: &[&str]) -> Option<String> {
    system_tool("/usr/bin/pmset", arguments)
}

fn system_tool(tool: &str, arguments: &[&str]) -> Option<String> {
    let mut child = deadpan_native_process::spawn(
        Command::new(tool)
            .args(arguments)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .ok()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut text = String::new();
    child
        .stdout
        .take()?
        .take(64 * 1024)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}
