//! Shared support for hostile-worker tests. The hostile Python fixtures record
//! the identities of every process they create in a test-owned directory; this
//! module checks that the host left none of the group behind and kills
//! anything a fixture deliberately escaped with. Only tests use Python;
//! production selects packaged native workers.
#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process, test_kill_process, test_kill_process_group};

/// How long an exited group's orphans may take to be reaped by launchd.
const REAP_GRACE: Duration = Duration::from_secs(3);

pub fn python() -> PathBuf {
    let executable = std::env::split_paths(&std::env::var_os("PATH").expect("test PATH"))
        .map(|directory| directory.join("python3"))
        .find(|candidate| candidate.is_file())
        .expect("Python 3 is required for hostile worker fixtures");
    fs::canonicalize(executable).unwrap()
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/hostile_workers")
        .join(name)
}

/// One hostile run's test-owned record directory.
pub struct Record {
    directory: tempfile::TempDir,
}

impl Record {
    pub fn new() -> Self {
        Self {
            directory: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    pub fn mode(&self, name: &str) -> String {
        format!("hostile:{name}:{}", self.path().display())
    }

    /// An executable wrapper for hosts whose seam is one executable without
    /// test-chosen arguments. It runs `fixture` in `mode`, passing the host's
    /// own arguments through.
    pub fn wrapper(&self, fixture: &Path, name: &str) -> PathBuf {
        let script = self.path().join(format!("worker-{name}"));
        fs::write(
            &script,
            format!(
                "#!{} -I\nimport runpy, sys\nsys.argv = [{fixture:?}, {mode:?}, *sys.argv[1:]]\nrunpy.run_path({fixture:?}, run_name=\"__main__\")\n",
                python().display(),
                fixture = fixture.display().to_string(),
                mode = self.mode(name),
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        script
    }

    fn pid(&self, name: &str) -> Option<Pid> {
        fs::read_to_string(self.path().join(name))
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .and_then(Pid::from_raw)
    }

    fn pids(&self, name: &str) -> Vec<Pid> {
        fs::read_to_string(self.path().join(name))
            .map(|text| {
                text.split_whitespace()
                    .filter_map(|value| value.parse().ok())
                    .filter_map(Pid::from_raw)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn leader(&self) -> Pid {
        self.pid("leader").expect("fixture recorded its leader")
    }

    pub fn children(&self) -> Vec<Pid> {
        self.pids("children")
    }

    /// Every recorded group member is gone, the leader is reaped and the group
    /// is empty. Orphans of an exited leader are reaped by launchd, so allow a
    /// short grace for that, never for a live process.
    pub fn assert_group_gone(&self) {
        let group = self.pid("group").expect("fixture recorded its group");
        let mut members = self.children();
        members.push(self.leader());
        let deadline = Instant::now() + REAP_GRACE;
        loop {
            let alive: Vec<_> = members
                .iter()
                .filter(|pid| test_kill_process(**pid).is_ok())
                .collect();
            let group_alive = test_kill_process_group(group).is_ok();
            if alive.is_empty() && !group_alive {
                return;
            }
            if Instant::now() >= deadline {
                for pid in &members {
                    let _ = kill_process(*pid, Signal::KILL);
                }
                panic!("worker group members survived the host: {alive:?} (group {group:?})");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The deliberately escaped descendant, which group cleanup cannot reach.
    pub fn escaped(&self, within: Duration) -> Pid {
        let deadline = Instant::now() + within;
        loop {
            if let Some(pid) = self.pid("escaped") {
                return pid;
            }
            assert!(Instant::now() < deadline, "fixture never escaped");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn wait_for(&self, name: &str, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if self.path().join(name).exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }
}

impl Drop for Record {
    fn drop(&mut self) {
        // Escaped descendants are outside every supervisor's authority; the
        // test that created them stops them itself.
        if let Some(pid) = self.pid("escaped") {
            let _ = kill_process(pid, Signal::KILL);
        }
    }
}

/// An escaped process stays alive after its host returned: the documented
/// limit of process-group ownership, not a sandbox.
pub fn assert_alive(pid: Pid) {
    assert!(
        test_kill_process(pid).is_ok(),
        "escaped descendant {pid:?} was expected to outlive group cleanup"
    );
}

pub fn assert_bounded(started: Instant, bound: Duration, what: &str) {
    let elapsed = started.elapsed();
    assert!(elapsed < bound, "{what} took {elapsed:?}, above {bound:?}");
}

/// The host refused for the hostile behaviour itself, not because the fixture
/// crashed first, and the typed error carries no flood of worker output.
pub fn assert_generic_cause(name: &str, message: &str) {
    assert!(message.len() < 4_096, "{name}: unbounded error: {message}");
    let expected = match name {
        "oversized" | "just_over" => "payload bytes",
        "malformed" | "invalid_utf8" | "zero_length" => "malformed",
        "truncated" => "ended after",
        "fork_spam_exit" => "without a terminal",
        "stderr_flood" => "status: 3",
        _ => return,
    };
    assert!(message.contains(expected), "{name}: {message}");
}

impl Record {
    /// How many hostile worker processes this record saw start.
    pub fn launches(&self) -> usize {
        fs::read_to_string(self.path().join("launches"))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }
}
