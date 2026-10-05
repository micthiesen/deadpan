//! One-case-per-process isolation for native code.
//!
//! The parent test re-executes its own test binary filtered to the same test
//! with `DEADPAN_CHAOS_CHILD_INPUT` naming the case file. The child runs one
//! case, prints a verdict line and exits. Signals, aborts, sanitizer reports,
//! hangs and excessive resident memory are failures observed by the parent.
//! `/usr/bin/time -l` reports the child's peak RSS without unsafe code.

use crate::{Outcome, Verdict, fnv1a};
use std::io::Read as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_INPUT: &str = "DEADPAN_CHAOS_CHILD_INPUT";
const VERDICT_PREFIX: &str = "deadpan-chaos-child-verdict:";

/// In a child, the case input; `None` in the parent.
pub fn child_input() -> Option<Vec<u8>> {
    let path = std::env::var_os(CHILD_INPUT)?;
    Some(std::fs::read(path).unwrap_or_default())
}

/// Prints the child's verdict and exits the process immediately, skipping the
/// remaining libtest machinery.
pub fn finish_child(outcome: Outcome) -> ! {
    use std::io::Write as _;
    let line = match outcome {
        Ok(Verdict::Accepted) => "accepted".to_owned(),
        Ok(Verdict::Rejected(class)) => format!("rejected:{}", class.replace('\n', " ")),
        Err(violation) => format!("invariant:{}", violation.replace('\n', " ")),
    };
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "\n{VERDICT_PREFIX}{line}");
    let _ = stdout.flush();
    std::process::exit(0)
}

/// Runs cases in a fresh process of the current test binary.
#[derive(Clone, Debug)]
pub struct ChildRunner {
    /// Full libtest path of the calling test, e.g. `decoder_fuzz`.
    pub test_name: &'static str,
    pub timeout: Duration,
    pub max_rss_bytes: u64,
    pub env: Vec<(String, String)>,
}

impl ChildRunner {
    pub fn new(test_name: &'static str) -> Self {
        Self {
            test_name,
            timeout: Duration::from_secs(30),
            max_rss_bytes: 2 << 30,
            env: Vec::new(),
        }
    }

    /// Runs one case. Crashes, timeouts and resource excess are `Err`.
    pub fn run(&self, input: &[u8]) -> Outcome {
        let directory = crate::output_directory().join("child");
        std::fs::create_dir_all(&directory).map_err(|error| format!("harness: {error}"))?;
        let path: PathBuf = directory.join(format!(
            "{}-{}-{:016x}.case",
            self.test_name,
            std::process::id(),
            fnv1a(input)
        ));
        std::fs::write(&path, input).map_err(|error| format!("harness: {error}"))?;
        let executable = std::env::current_exe().map_err(|error| format!("harness: {error}"))?;
        let time = std::path::Path::new("/usr/bin/time");
        let mut command = if time.exists() {
            let mut command = Command::new(time);
            command.arg("-l").arg(executable);
            command
        } else {
            Command::new(executable)
        };
        command
            .args([self.test_name, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD_INPUT, &path)
            .env_remove("DEADPAN_CHAOS_SECONDS")
            .env_remove("DEADPAN_CHAOS_REPLAY")
            .envs(
                self.env
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("harness: {error}"))?;
        // Drain both pipes on helper threads so a chatty child cannot block.
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let out = std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(pipe) = stdout.as_mut() {
                let _ = pipe.take(1 << 20).read_to_string(&mut text);
            }
            text
        });
        let err = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(pipe) = stderr.as_mut() {
                let _ = pipe.read_to_end(&mut bytes);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        });
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if started.elapsed() > self.timeout => {
                    // `/usr/bin/time` does not forward SIGKILL; stop the test
                    // process it wraps first so the pipes reach EOF.
                    let _ = Command::new("/usr/bin/pkill")
                        .args(["-KILL", "-P", &child.id().to_string()])
                        .status();
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(2)),
                Err(error) => return Err(format!("harness: {error}")),
            }
        };
        let stdout = out.join().unwrap_or_default();
        let stderr = err.join().unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        let tail: String = {
            let lines: Vec<&str> = stderr
                .lines()
                .filter(|line| !line.trim_start().starts_with(char::is_numeric))
                .collect();
            lines[lines.len().saturating_sub(12)..].join(" | ")
        };
        let Some(status) = status else {
            return Err(format!("child timeout after {:?}", self.timeout));
        };
        if stderr.contains("AddressSanitizer") || stderr.contains("UndefinedBehaviorSanitizer") {
            return Err(format!("sanitizer report: {tail}"));
        }
        let rss = stderr
            .lines()
            .find(|line| line.contains("maximum resident set size"))
            .and_then(|line| line.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok());
        if let Some(rss) = rss.filter(|rss| *rss > self.max_rss_bytes) {
            return Err(format!(
                "child peak RSS {rss} bytes > {} bytes",
                self.max_rss_bytes
            ));
        }
        let verdict = stdout.lines().find_map(|line| {
            line.find(VERDICT_PREFIX)
                .map(|at| &line[at + VERDICT_PREFIX.len()..])
        });
        // `/usr/bin/time` reports a signalled child as "command terminated
        // abnormally" and exits 128 + signal; libtest reports a panic as 101.
        let signalled = status.code().is_none_or(|code| code > 128);
        if stderr.contains("terminated abnormally") || signalled {
            return Err(format!("child crashed ({status}): {tail}"));
        }
        match verdict {
            Some("accepted") => Ok(Verdict::Accepted),
            Some(line) if line.starts_with("rejected:") => {
                Ok(Verdict::Rejected(line["rejected:".len()..].to_owned()))
            }
            Some(line) => Err(line.to_owned()),
            None => Err(format!("child exited {status} without a verdict: {tail}")),
        }
    }
}
