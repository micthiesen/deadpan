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

/// Estimated reclaimable physical memory, not a promise of allocation success.
/// Pressure remains an independent admission condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryHeadroom {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl MemoryHeadroom {
    /// Admission policy: leave at least 8 GiB, or one eighth of physical RAM,
    /// for the editor, OS and other applications. This is a reserve policy,
    /// not a measured bound on those applications' future allocations.
    pub fn reserve_bytes(self) -> u64 {
        (8 * 1024 * 1024 * 1024).max(self.total_bytes / 8)
    }

    pub fn permits(self, model_bytes: u64) -> bool {
        self.total_bytes > 0
            && self.available_bytes <= self.total_bytes
            && model_bytes > 0
            && model_bytes
                .checked_add(self.reserve_bytes())
                .is_some_and(|needed| needed <= self.available_bytes)
    }
}

/// Bounded macOS observation for preparation threads. Uses free, speculative
/// and inactive pages as estimated headroom; it does not count compressed or
/// purgeable pages again. `vm_stat` subtracts speculative pages from its printed
/// free count, unlike the underlying Mach `free_count`.
pub fn memory_headroom() -> Option<MemoryHeadroom> {
    let total = system_tool("/usr/sbin/sysctl", &["-n", "hw.memsize"])?;
    let pages = system_tool("/usr/bin/vm_stat", &[])?;
    parse_memory_headroom(total.trim().parse().ok()?, &pages)
}

fn parse_memory_headroom(total_bytes: u64, text: &str) -> Option<MemoryHeadroom> {
    let mut lines = text.lines();
    let page_size: u64 = lines
        .next()?
        .strip_prefix("Mach Virtual Memory Statistics: (page size of ")?
        .strip_suffix(" bytes)")?
        .parse()
        .ok()?;
    if !page_size.is_power_of_two() || page_size < 4096 || total_bytes == 0 {
        return None;
    }
    let mut counts = [None; 3];
    for line in lines {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let index = match key {
            "Pages free" => 0,
            "Pages speculative" => 1,
            "Pages inactive" => 2,
            _ => continue,
        };
        if counts[index].is_some() {
            return None;
        }
        counts[index] = Some(value.trim().strip_suffix('.')?.parse::<u64>().ok()?);
    }
    let available_bytes = counts[0]?
        .checked_add(counts[1]?)?
        .checked_add(counts[2]?)?
        .checked_mul(page_size)?;
    (available_bytes <= total_bytes).then_some(MemoryHeadroom {
        total_bytes,
        available_bytes,
    })
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
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
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
        .take(64 * 1024 + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= 64 * 1024).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const SNAPSHOT: &str = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 10.\nPages inactive: 20.\nPages speculative: 3.\nPages purgeable: 7.\nPages occupied by compressor: 50.\n";

    #[test]
    fn headroom_counts_printed_speculative_once_and_excludes_other_reclaimable_fields() {
        assert_eq!(
            parse_memory_headroom(128 * GIB, SNAPSHOT),
            Some(MemoryHeadroom {
                total_bytes: 128 * GIB,
                available_bytes: 33 * 16384,
            })
        );
    }

    #[test]
    fn headroom_rejects_missing_duplicate_invalid_and_overflowed_observations() {
        for text in [
            SNAPSHOT.replace("Pages inactive: 20.\n", ""),
            format!("{SNAPSHOT}Pages free: 1.\n"),
            SNAPSHOT.replace("16384", "0"),
            SNAPSHOT.replace("16384", "16385"),
            SNAPSHOT.replace("20.", "-1."),
            SNAPSHOT.replace("20.", "18446744073709551615."),
            SNAPSHOT.replace("20.", "1125899906842624."),
        ] {
            assert_eq!(parse_memory_headroom(128 * GIB, &text), None);
        }
        assert_eq!(parse_memory_headroom(16384, SNAPSHOT), None);
        assert_eq!(parse_memory_headroom(0, SNAPSHOT), None);
    }

    #[test]
    fn admission_preserves_reserve_and_checks_arithmetic() {
        let memory = MemoryHeadroom {
            total_bytes: 128 * GIB,
            available_bytes: 37 * GIB,
        };
        assert_eq!(memory.reserve_bytes(), 16 * GIB);
        assert!(memory.permits(21 * GIB));
        assert!(!memory.permits(21 * GIB + 1));
        assert!(!memory.permits(u64::MAX));
        assert!(!memory.permits(0));
        assert_eq!(
            MemoryHeadroom {
                total_bytes: 16 * GIB,
                available_bytes: 0
            }
            .reserve_bytes(),
            8 * GIB
        );
        assert!(
            !MemoryHeadroom {
                total_bytes: GIB,
                available_bytes: 128 * GIB
            }
            .permits(GIB)
        );
    }

    #[test]
    fn system_observations_reject_unsuccessful_commands() {
        assert!(system_tool("/usr/bin/false", &[]).is_none());
    }

    #[test]
    #[ignore = "live macOS observation; run explicitly on the qualification host"]
    fn live_memory_headroom_observation() {
        let snapshot = memory_headroom().expect("read a valid macOS memory snapshot");
        println!(
            "{snapshot:?}; reserve_bytes={}; pressure={:?}",
            snapshot.reserve_bytes(),
            memory_pressure_level()
        );
        assert!(snapshot.total_bytes > 0);
        assert!(snapshot.available_bytes <= snapshot.total_bytes);
    }
}
