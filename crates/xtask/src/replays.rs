//! `cargo xtask replays`: build the ui-harness app once and replay every
//! scenario in its own process and report directory.
//!
//! Each scenario runs as `deadpan-app --ui-check --scenario NAME --output
//! DIR/NAME`, so one failure cannot hide another and every report stays
//! separately inspectable. The scenario list comes from the built harness
//! (`--ui-check --list-scenarios`), never from a copy here. Scenarios that
//! need an explicit `--project` fixture are reported as skipped.
//!
//! The built app and the helper executables it finds beside itself are
//! copied into `DIR/bin` right after the build, and every scenario runs that
//! private copy. A concurrent rebuild of `target/debug` therefore cannot
//! replace the binary under a running replay (the render worker, for one,
//! verifies that its mapped executable image is unchanged). The copies'
//! SHA-256 digests are recorded in `summary.json`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The harness binary and the workers it locates beside its own executable
/// (`deadpan-cli` media download, transcription and tracking runtimes).
const APP: &str = "deadpan-app";
const HELPERS: [&str; 3] = [
    "deadpan-media-worker",
    "deadpan-transcribe",
    "deadpan-track",
];

const USAGE: &str = "usage: cargo xtask replays [--scenario a,b] [--output NEW_DIR] [--jobs N]";

struct Options {
    scenarios: Option<Vec<String>>,
    output: Option<PathBuf>,
    jobs: usize,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        scenarios: None,
        output: None,
        jobs: 1,
    };
    let mut arguments = arguments.iter();
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {flag}; {USAGE}"))?;
        match flag.as_str() {
            "--scenario" | "--scenarios" if options.scenarios.is_none() => {
                let names = value
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if names.is_empty() {
                    return Err(format!("--scenario needs at least one name; {USAGE}"));
                }
                options.scenarios = Some(names);
            }
            "--output" if options.output.is_none() => options.output = Some(PathBuf::from(value)),
            "--jobs" => {
                options.jobs = value
                    .parse()
                    .ok()
                    .filter(|jobs| (1..=8).contains(jobs))
                    .ok_or("--jobs must be between 1 and 8")?;
            }
            _ => return Err(format!("unknown or repeated option {flag}; {USAGE}")),
        }
    }
    Ok(options)
}

/// One scenario's outcome, read from its own report rather than inferred
/// from the exit status alone.
struct Outcome {
    name: String,
    status: &'static str,
    seconds: f64,
    checks: usize,
    failed_checks: Vec<String>,
    failures: Vec<String>,
    layout_warnings: Vec<String>,
    other_warnings: usize,
    note: Option<String>,
}

pub fn run(arguments: &[String]) -> Result<(), String> {
    let options = parse(arguments)?;
    let output = match options.output {
        Some(path) => path,
        None => std::env::temp_dir().join(format!(
            "deadpan-replays-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_secs()
        )),
    };
    // Exclusive, like the harness's own report directories: stale reports
    // must never be read as this run's evidence.
    std::fs::create_dir(&output)
        .map_err(|error| format!("create new output directory {}: {error}", output.display()))?;
    let build_started = Instant::now();
    let built = build()?;
    let (executable, binaries) = snapshot(&built, &output.join("bin"))?;
    let build_seconds = build_started.elapsed().as_secs_f64();
    eprintln!(
        "replays: built and copied {} in {build_seconds:.1} s (sha256 {})",
        executable.display(),
        binaries[APP]["sha256"].as_str().unwrap_or("?")
    );
    let listed = list(&executable)?;
    let selected = match &options.scenarios {
        None => listed.clone(),
        Some(names) => {
            let unknown = names
                .iter()
                .filter(|name| !listed.iter().any(|(listed, _)| listed == *name))
                .cloned()
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                return Err(format!(
                    "unknown scenario(s): {}; known: {}",
                    unknown.join(", "),
                    listed
                        .iter()
                        .map(|(name, _)| name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            listed
                .iter()
                .filter(|(name, _)| names.contains(name))
                .cloned()
                .collect()
        }
    };
    let queue = Mutex::new(selected.iter().enumerate());
    let results = Mutex::new(Vec::with_capacity(selected.len()));
    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..options.jobs.min(selected.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().map(|mut queue| queue.next()).ok().flatten();
                    let Some((index, (name, needs_project))) = next else {
                        break;
                    };
                    let outcome = if *needs_project {
                        Outcome::skipped(
                            name,
                            "needs an explicit --project fixture; run it directly with deadpan-app --ui-check",
                        )
                    } else {
                        replay(&executable, &output, name)
                    };
                    eprintln!(
                        "replays: {:<22} {:<4} {:>7.1} s",
                        outcome.name, outcome.status, outcome.seconds
                    );
                    if let Ok(mut results) = results.lock() {
                        results.push((index, outcome));
                    }
                }
            });
        }
    });
    let wall = started.elapsed();
    let mut results = results.into_inner().map_err(|error| error.to_string())?;
    results.sort_by_key(|(index, _)| *index);
    let outcomes = results
        .into_iter()
        .map(|(_, outcome)| outcome)
        .collect::<Vec<_>>();
    summarize(
        &outcomes,
        &output,
        build_seconds,
        wall,
        options.jobs,
        binaries,
    )
}

/// Build the harness app and its helpers once; return each executable Cargo
/// reported, by target name.
fn build() -> Result<Vec<(String, PathBuf)>, String> {
    let mut arguments = vec![
        "build",
        "--locked",
        "-p",
        APP,
        "--features",
        "deadpan-app/ui-harness",
        "--message-format=json-render-diagnostics",
    ];
    for helper in HELPERS {
        arguments.extend(["-p", helper]);
    }
    let output = Command::new("cargo")
        .args(arguments)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("failed to start cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!("ui-harness build exited with {}", output.status));
    }
    let built = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact"
                && message["target"]["kind"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
        })
        .filter_map(|message| {
            Some((
                message["target"]["name"].as_str()?.to_owned(),
                PathBuf::from(message["executable"].as_str()?),
            ))
        })
        .collect::<Vec<_>>();
    for name in std::iter::once(APP).chain(HELPERS) {
        if !built.iter().any(|(built, _)| built == name) {
            return Err(format!("cargo did not report a {name} executable"));
        }
    }
    Ok(built)
}

/// Copy the app and its helpers side by side into `directory`, so the app
/// still finds them beside itself, and hash the copies that will run.
fn snapshot(built: &[(String, PathBuf)], directory: &Path) -> Result<(PathBuf, Value), String> {
    std::fs::create_dir(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let mut binaries = serde_json::Map::new();
    for name in std::iter::once(APP).chain(HELPERS) {
        let source = built
            .iter()
            .find(|(built, _)| built == name)
            .map(|(_, path)| path)
            .ok_or_else(|| format!("missing built {name}"))?;
        let copy = directory.join(name);
        // fs::copy keeps the executable mode and the ad hoc code signature.
        std::fs::copy(source, &copy)
            .map_err(|error| format!("copy {} to {}: {error}", source.display(), copy.display()))?;
        binaries.insert(
            name.to_owned(),
            json!({"source": source, "path": copy, "sha256": sha256(&copy)?}),
        );
    }
    Ok((directory.join(APP), Value::Object(binaries)))
}

fn sha256(path: &Path) -> Result<String, String> {
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|error| format!("hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn list(executable: &Path) -> Result<Vec<(String, bool)>, String> {
    let output = Command::new(executable)
        .args(["--ui-check", "--list-scenarios"])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("failed to list scenarios: {error}"))?;
    if !output.status.success() {
        return Err(format!("listing scenarios exited with {}", output.status));
    }
    let scenarios = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| match line.split_once('\t') {
            Some((name, tag)) => (name.to_owned(), tag == "project"),
            None => (line.to_owned(), false),
        })
        .collect::<Vec<_>>();
    if scenarios.is_empty() {
        return Err("the harness listed no scenarios".into());
    }
    Ok(scenarios)
}

fn replay(executable: &Path, output: &Path, name: &str) -> Outcome {
    let directory = output.join(name);
    let started = Instant::now();
    let log = std::fs::File::create(output.join(format!("{name}.log")));
    let status = log.and_then(|log| {
        let stderr = log.try_clone()?;
        Command::new(executable)
            .args(["--ui-check", "--scenario", name, "--output"])
            .arg(&directory)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(stderr)
            .status()
    });
    let seconds = started.elapsed().as_secs_f64();
    let report = std::fs::read(directory.join("report.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let mut outcome = Outcome::from_report(name, seconds, report.as_ref());
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            outcome.status = "FAIL";
            if outcome.failed_checks.is_empty() && outcome.failures.is_empty() {
                outcome
                    .failures
                    .push(format!("process exited with {status}; see {name}.log"));
            }
        }
        Err(error) => {
            outcome.status = "FAIL";
            outcome
                .failures
                .push(format!("failed to start replay: {error}"));
        }
    }
    outcome
}

impl Outcome {
    fn skipped(name: &str, note: &str) -> Self {
        Self {
            name: name.to_owned(),
            status: "SKIP",
            seconds: 0.0,
            checks: 0,
            failed_checks: Vec::new(),
            failures: Vec::new(),
            layout_warnings: Vec::new(),
            other_warnings: 0,
            note: Some(note.to_owned()),
        }
    }

    fn from_report(name: &str, seconds: f64, report: Option<&Value>) -> Self {
        let mut outcome = Self {
            name: name.to_owned(),
            status: "PASS",
            seconds,
            checks: 0,
            failed_checks: Vec::new(),
            failures: Vec::new(),
            layout_warnings: Vec::new(),
            other_warnings: 0,
            note: None,
        };
        let Some(report) = report else {
            outcome.status = "FAIL";
            outcome
                .failures
                .push("no readable report.json; see the scenario log".into());
            return outcome;
        };
        // The harness always adds the Kestrel audit; count it with the
        // scenario so a routing conflict fails the run too.
        for scenario in report["scenarios"].as_array().into_iter().flatten() {
            for check in scenario["checks"].as_array().into_iter().flatten() {
                outcome.checks += 1;
                if check["passed"] != true {
                    outcome
                        .failed_checks
                        .push(check["name"].as_str().unwrap_or("unnamed check").to_owned());
                }
            }
            for finding in scenario["findings"].as_array().into_iter().flatten() {
                let message = finding["message"].as_str().unwrap_or_default();
                match finding["severity"].as_str() {
                    Some("failure") => outcome.failures.push(message.to_owned()),
                    _ if is_layout_retry(message) => {
                        outcome.layout_warnings.push(layout_summary(message));
                    }
                    _ => outcome.other_warnings += 1,
                }
            }
            for metric in scenario["timings"].as_array().into_iter().flatten() {
                let failed = metric["samples"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|sample| sample["outcome"] != "completed")
                    .count();
                if failed > 0 {
                    outcome.failures.push(format!(
                        "{failed} timing sample(s) of {} did not complete",
                        metric["name"].as_str().unwrap_or("a metric")
                    ));
                }
            }
        }
        if report["failed"] == true
            || !outcome.failed_checks.is_empty()
            || !outcome.failures.is_empty()
        {
            outcome.status = "FAIL";
        }
        outcome
    }
}

/// The harness's egui layout-retry findings (see docs/UI_FEEDBACK.md).
fn is_layout_retry(message: &str) -> bool {
    message.starts_with("egui discard warning")
        || message.contains("settled their layout only on a second retry")
}

/// Keep the leading sentence; the full frame/reason trace stays in the report.
fn layout_summary(message: &str) -> String {
    message
        .split_once(": [")
        .map_or(message, |(head, _)| head)
        .to_owned()
}

fn summarize(
    outcomes: &[Outcome],
    output: &Path,
    build_seconds: f64,
    wall: Duration,
    jobs: usize,
    binaries: Value,
) -> Result<(), String> {
    let failed = outcomes.iter().filter(|outcome| outcome.status == "FAIL");
    let failed_count = failed.clone().count();
    let passed = outcomes
        .iter()
        .filter(|outcome| outcome.status == "PASS")
        .count();
    let skipped = outcomes
        .iter()
        .filter(|outcome| outcome.status == "SKIP")
        .count();
    let checks = outcomes.iter().map(|outcome| outcome.checks).sum::<usize>();
    let replay_seconds = outcomes.iter().map(|outcome| outcome.seconds).sum::<f64>();
    println!();
    println!(
        "{:<22} {:<4} {:>8} {:>7}  layout-retry warnings",
        "scenario", "", "seconds", "checks"
    );
    for outcome in outcomes {
        println!(
            "{:<22} {:<4} {:>8.1} {:>7}  {}",
            outcome.name,
            outcome.status,
            outcome.seconds,
            outcome.checks,
            if outcome.layout_warnings.is_empty() {
                outcome.note.clone().unwrap_or_else(|| "-".into())
            } else {
                outcome.layout_warnings.join("; ")
            }
        );
    }
    for outcome in failed {
        println!(
            "\n{} failed (report: {}):",
            outcome.name,
            output.join(&outcome.name).join("report.html").display()
        );
        for check in &outcome.failed_checks {
            println!("  check: {check}");
        }
        for failure in &outcome.failures {
            // Full messages (with control inventories) stay in summary.json.
            let short = failure.chars().take(240).collect::<String>();
            let ellipsis = if short.len() < failure.len() {
                "…"
            } else {
                ""
            };
            println!("  {short}{ellipsis}");
        }
    }
    println!(
        "\n{passed} passed, {failed_count} failed, {skipped} skipped; {checks} checks; build {build_seconds:.1} s, replays {:.1} s wall ({replay_seconds:.1} s summed, {jobs} job(s))",
        wall.as_secs_f64()
    );
    println!("Reports: {}", output.display());
    let summary = json!({
        "build_seconds": build_seconds,
        "wall_seconds": wall.as_secs_f64(),
        "jobs": jobs,
        "binaries": binaries,
        "scenarios": outcomes.iter().map(|outcome| json!({
            "name": outcome.name,
            "status": outcome.status,
            "seconds": outcome.seconds,
            "checks": outcome.checks,
            "failed_checks": outcome.failed_checks,
            "failures": outcome.failures,
            "layout_retry_warnings": outcome.layout_warnings,
            "other_warnings": outcome.other_warnings,
            "note": outcome.note,
        })).collect::<Vec<_>>(),
    });
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("write summary: {error}"))?;
    if failed_count > 0 {
        Err(format!("{failed_count} replay scenario(s) failed"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_split_scenarios_and_reject_unknown_flags() {
        let options = parse(&["--scenario".into(), "room-tone, delete-range".into()]).unwrap();
        assert_eq!(
            options.scenarios.unwrap(),
            vec!["room-tone".to_owned(), "delete-range".to_owned()]
        );
        assert!(parse(&["--jobs".into(), "0".into()]).is_err());
        assert!(parse(&["--frobnicate".into(), "1".into()]).is_err());
        assert!(parse(&["--output".into()]).is_err());
    }

    #[test]
    fn the_app_and_its_helpers_run_from_hashed_private_copies() {
        let root = std::env::temp_dir().join(format!(
            "deadpan-xtask-snapshot-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("target")).unwrap();
        let built = std::iter::once(APP)
            .chain(HELPERS)
            .map(|name| {
                let path = root.join("target").join(name);
                std::fs::write(&path, name).unwrap();
                (name.to_owned(), path)
            })
            .collect::<Vec<_>>();
        let (executable, binaries) = snapshot(&built, &root.join("bin")).unwrap();
        assert_eq!(executable, root.join("bin").join(APP));
        // Rewriting the build output after the snapshot cannot reach the copy.
        std::fs::write(&built[0].1, "rebuilt").unwrap();
        assert_eq!(std::fs::read_to_string(&executable).unwrap(), APP);
        for name in HELPERS {
            assert!(root.join("bin").join(name).is_file());
        }
        // SHA-256("deadpan-app").
        assert_eq!(
            binaries[APP]["sha256"],
            "200bbd925503912d39460fed17e4ad65224468e90312a78f75eb7e58b8355534"
        );
        assert!(snapshot(&built[..1], &root.join("partial")).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reports_count_failed_checks_failures_and_layout_retries() {
        let report = json!({
            "failed": true,
            "scenarios": [
                {"name": "kestrel-shortcuts", "checks": [{"name": "audit", "passed": true}], "findings": [], "timings": []},
                {"name": "room-tone", "checks": [{"name": "a", "passed": true}, {"name": "b", "passed": false}],
                 "findings": [
                    {"severity": "warning", "message": "3 frame(s) settled their layout only on a second retry: [{\"frame\":1}]"},
                    {"severity": "warning", "message": "Screenshot budget reached"},
                    {"severity": "failure", "message": "Check failed: b"}
                 ],
                 "timings": [{"name": "nav", "samples": [{"outcome": "completed"}, {"outcome": "timed_out"}]}]}
            ]
        });
        let outcome = Outcome::from_report("room-tone", 1.0, Some(&report));
        assert_eq!(outcome.status, "FAIL");
        assert_eq!(outcome.checks, 3);
        assert_eq!(outcome.failed_checks, vec!["b".to_owned()]);
        assert_eq!(outcome.failures.len(), 2);
        assert_eq!(
            outcome.layout_warnings,
            vec!["3 frame(s) settled their layout only on a second retry".to_owned()]
        );
        assert_eq!(outcome.other_warnings, 1);
        let clean = json!({"failed": false, "scenarios": [{"checks": [{"passed": true}], "findings": [], "timings": []}]});
        assert_eq!(Outcome::from_report("x", 0.0, Some(&clean)).status, "PASS");
        assert_eq!(Outcome::from_report("x", 0.0, None).status, "FAIL");
    }
}
