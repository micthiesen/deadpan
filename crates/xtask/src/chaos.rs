//! `cargo xtask chaos`: the Gate G adversarial campaign (docs/ADVERSARIAL.md).
//!
//! Every adversarial target is an ordinary test (deterministic regression
//! mode under `cargo test`). This task builds those test binaries once,
//! records their SHA-256 digests, and reruns only the adversarial tests in
//! campaign mode: `DEADPAN_CHAOS_SECONDS` per target from a fresh (or given)
//! seed, with crash artifacts and one JSON report line per target written
//! below `--output`. The wall time is about `--minutes` (default 10).
//!
//! `--sanitize` instead builds `deadpan-source` with the same ASan/UBSan
//! setup as `tools/media-qualification/host/build_sanitized.py` (instrumented
//! C adapters, explicit target so host proc-macros stay uninstrumented) in
//! its own target directory and runs the native decoder targets there.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const USAGE: &str = "usage: cargo xtask chaos [--minutes N] [--seed HEX] [--only name,..] [--output NEW_DIR] [--sanitize] [--stress] [--list]";

/// One test binary selection and the filter naming its adversarial tests.
/// `targets` counts the fuzz targets that run one after another inside a
/// single test function, so each receives its share of the time budget.
struct Entry {
    name: &'static str,
    package: &'static str,
    /// `None` for the library unit tests, else an integration test target.
    test: Option<&'static str>,
    filter: &'static str,
    targets: u32,
}

const ENTRIES: &[Entry] = &[
    Entry {
        name: "source-containers",
        package: "deadpan-source",
        test: None,
        filter: "input::adversarial",
        targets: 1,
    },
    Entry {
        name: "source-native-decode",
        package: "deadpan-source",
        test: Some("adversarial_decode"),
        filter: "",
        targets: 1,
    },
    Entry {
        name: "core-json",
        package: "deadpan-core",
        test: Some("adversarial"),
        filter: "",
        targets: 2,
    },
    Entry {
        name: "store-tamper",
        package: "deadpan-store",
        test: Some("adversarial"),
        filter: "",
        targets: 1,
    },
    Entry {
        name: "jobs-protocols",
        package: "deadpan-jobs",
        test: None,
        filter: "adversarial",
        targets: 1,
    },
    Entry {
        name: "jobs-generation",
        package: "deadpan-jobs",
        test: Some("protocol"),
        filter: "adversarial",
        targets: 1,
    },
    Entry {
        name: "jobs-extension-plans",
        package: "deadpan-jobs",
        test: Some("extension_adversarial"),
        filter: "adversarial",
        targets: 1,
    },
    Entry {
        name: "cli-boundaries",
        package: "deadpan-cli",
        test: None,
        filter: "adversarial",
        targets: 1,
    },
    Entry {
        name: "media-json",
        package: "deadpan-media",
        test: None,
        filter: "adversarial",
        targets: 1,
    },
    Entry {
        name: "media-worker",
        package: "deadpan-media-worker",
        test: Some("canonicalize_real_media"),
        filter: "adversarial",
        targets: 1,
    },
    Entry {
        name: "model-packs",
        package: "deadpan-models",
        test: None,
        filter: "adversarial",
        // Pack archive/manifest and Bridge/Extension conditioning manifests.
        targets: 4,
    },
    Entry {
        name: "analysis-json",
        package: "deadpan-analysis",
        test: None,
        filter: "adversarial",
        targets: 1,
    },
];

struct Options {
    minutes: f64,
    seed: Option<String>,
    only: Option<Vec<String>>,
    output: Option<PathBuf>,
    sanitize: bool,
    stress: bool,
    list: bool,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        minutes: 10.0,
        seed: None,
        only: None,
        output: None,
        sanitize: false,
        stress: false,
        list: false,
    };
    let mut arguments = arguments.iter();
    while let Some(flag) = arguments.next() {
        match flag.as_str() {
            "--sanitize" => options.sanitize = true,
            "--stress" => options.stress = true,
            "--list" => options.list = true,
            "--minutes" | "--seed" | "--only" | "--output" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| format!("missing value for {flag}; {USAGE}"))?;
                match flag.as_str() {
                    "--minutes" => {
                        options.minutes = value
                            .parse()
                            .ok()
                            .filter(|minutes: &f64| {
                                minutes.is_finite() && *minutes > 0.0 && *minutes <= 24.0 * 60.0
                            })
                            .ok_or("--minutes must be positive and at most one day")?;
                    }
                    "--seed" => {
                        u64::from_str_radix(value.trim_start_matches("0x"), 16)
                            .map_err(|_| "--seed must be hexadecimal")?;
                        options.seed = Some(value.clone());
                    }
                    "--only" => {
                        options.only =
                            Some(value.split(',').map(str::trim).map(str::to_owned).collect());
                    }
                    _ => options.output = Some(PathBuf::from(value)),
                }
            }
            _ => return Err(format!("unknown option {flag}; {USAGE}")),
        }
    }
    Ok(options)
}

pub fn run(arguments: &[String]) -> Result<(), String> {
    let options = parse(arguments)?;
    if options.list {
        for entry in ENTRIES {
            println!(
                "{:<22} {} {}",
                entry.name,
                entry.package,
                entry.test.unwrap_or("--lib")
            );
        }
        return Ok(());
    }
    if options.stress {
        return stress(options.output.as_deref());
    }
    let selected: Vec<&Entry> = ENTRIES
        .iter()
        .filter(|entry| {
            options
                .only
                .as_ref()
                .is_none_or(|only| only.iter().any(|name| name == entry.name))
                && (!options.sanitize || entry.package == "deadpan-source")
        })
        .collect();
    if selected.is_empty() {
        return Err(format!("no adversarial entry matches; see --list. {USAGE}"));
    }
    let output = options.output.clone().unwrap_or_else(|| {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_secs());
        PathBuf::from("target/chaos").join(stamp.to_string())
    });
    if output.exists() {
        return Err(format!(
            "{} already exists; choose a new --output",
            output.display()
        ));
    }
    std::fs::create_dir_all(&output)
        .map_err(|error| format!("create {}: {error}", output.display()))?;
    let output = output
        .canonicalize()
        .map_err(|error| format!("resolve {}: {error}", output.display()))?;

    let sanitizer = if options.sanitize {
        Some(sanitizer_environment(&output)?)
    } else {
        None
    };
    let binaries = build(&selected, sanitizer.as_ref())?;
    let share = options.minutes * 60.0 / selected.len() as f64;
    let started = Instant::now();
    let mut runs = Vec::new();
    for entry in &selected {
        let (binary, directory) = binaries
            .get(entry.name)
            .ok_or_else(|| format!("no test binary was built for {}", entry.name))?;
        let seconds = share / f64::from(entry.targets);
        eprintln!("chaos: {} for {:.0} s per target", entry.name, seconds);
        let log = output.join(format!("{}.log", entry.name));
        let log_file = std::fs::File::create(&log).map_err(|error| error.to_string())?;
        let mut command = Command::new(binary);
        if !entry.filter.is_empty() {
            command.arg(entry.filter);
        }
        command
            .arg("--nocapture")
            .current_dir(directory)
            .env("DEADPAN_CHAOS_SECONDS", format!("{seconds:.1}"))
            .env("DEADPAN_CHAOS_OUT", &output)
            .stdout(Stdio::from(
                log_file.try_clone().map_err(|error| error.to_string())?,
            ))
            .stderr(Stdio::from(log_file));
        if let Some(seed) = &options.seed {
            command.env("DEADPAN_CHAOS_SEED", seed);
        }
        if let Some(environment) = &sanitizer {
            command.envs(environment.runtime.iter().map(|(key, value)| (key, value)));
        }
        let entry_started = Instant::now();
        let status = command
            .status()
            .map_err(|error| format!("run {}: {error}", binary.display()))?;
        runs.push(json!({
            "entry": entry.name,
            "binary": binary.display().to_string(),
            "binary_sha256": crate::replays::sha256(binary)?,
            "exit": status.code(),
            "seconds": entry_started.elapsed().as_secs_f64(),
            "log": log.display().to_string(),
        }));
    }
    let reports: Vec<Value> = std::fs::read_to_string(output.join("report.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let failures: Vec<&Value> = reports
        .iter()
        .filter(|report| {
            report["failures"]
                .as_array()
                .is_some_and(|failures| !failures.is_empty())
        })
        .collect();
    let failed_runs = runs.iter().filter(|run| run["exit"] != json!(0)).count();
    let executions: u64 = reports
        .iter()
        .filter_map(|report| report["executions"].as_u64())
        .sum();
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .is_some_and(|output| !output.stdout.is_empty());
    let summary = json!({
        "revision": revision,
        "dirty_worktree": dirty,
        "minutes": options.minutes,
        "sanitize": options.sanitize,
        "elapsed_s": started.elapsed().as_secs_f64(),
        "executions": executions,
        "targets": reports.len(),
        "failing_targets": failures.len(),
        "failed_runs": failed_runs,
        "runs": runs,
        "reports": reports,
    });
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    for report in &reports {
        eprintln!(
            "chaos: {:<30} {:>9} executions {:>5} classes  max {:>7.1} ms  {} failure(s)",
            report["target"].as_str().unwrap_or("?"),
            report["executions"],
            report["distinct_classes"],
            report["max_case_ms"].as_f64().unwrap_or(0.0),
            report["failures"].as_array().map_or(0, Vec::len),
        );
    }
    eprintln!(
        "chaos: {executions} executions; summary in {}",
        output.join("summary.json").display()
    );
    if failures.is_empty() && failed_runs == 0 {
        Ok(())
    } else {
        Err(format!(
            "{} target(s) found failures and {failed_runs} run(s) exited unsuccessfully; see {}",
            failures.len(),
            output.display()
        ))
    }
}

/// The full-scale long-project stress in release: 10,000 beats, hundreds of
/// revisions and a synthetic two-hour Original, with stage budgets.
fn stress(output: Option<&Path>) -> Result<(), String> {
    let output = output.map_or_else(
        || {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |time| time.as_secs());
            PathBuf::from("target/chaos").join(format!("stress-{stamp}"))
        },
        Path::to_path_buf,
    );
    std::fs::create_dir_all(&output)
        .map_err(|error| format!("create {}: {error}", output.display()))?;
    let output = output.canonicalize().map_err(|error| error.to_string())?;
    let status = Command::new("cargo")
        .args([
            "test",
            "--release",
            "--locked",
            "-p",
            "deadpan-store",
            "--test",
            "long_project_stress",
            "--",
            "--nocapture",
        ])
        .env("DEADPAN_STRESS", "full")
        .env("DEADPAN_CHAOS_OUT", &output)
        .status()
        .map_err(|error| format!("start cargo: {error}"))?;
    if status.success() {
        eprintln!(
            "chaos: stress report in {}",
            output.join("stress.json").display()
        );
        Ok(())
    } else {
        Err(format!(
            "long-project stress failed ({status}); see {}",
            output.display()
        ))
    }
}

struct Sanitizer {
    build: Vec<(String, String)>,
    runtime: Vec<(String, String)>,
    target_dir: PathBuf,
}

/// The documented ASan/UBSan configuration for the native C adapters.
fn sanitizer_environment(output: &Path) -> Result<Sanitizer, String> {
    let runtime = Command::new("clang")
        .arg("-print-file-name=libclang_rt.asan_osx_dynamic.dylib")
        .output()
        .map_err(|error| format!("clang is needed for --sanitize: {error}"))?;
    let runtime = PathBuf::from(String::from_utf8_lossy(&runtime.stdout).trim());
    if !runtime.is_file() {
        return Err(format!("ASan runtime not found at {}", runtime.display()));
    }
    let directory = runtime.parent().ok_or("ASan runtime has no directory")?;
    let flags = "-fsanitize=address,undefined -fno-omit-frame-pointer -fno-sanitize-recover=all";
    let rust = [
        "-C".to_owned(),
        format!("link-arg={}", runtime.display()),
        "-C".to_owned(),
        format!("link-arg=-Wl,-rpath,{}", directory.display()),
    ]
    .join("\u{1f}");
    Ok(Sanitizer {
        build: vec![
            ("CFLAGS_aarch64_apple_darwin".into(), flags.into()),
            ("CXXFLAGS_aarch64_apple_darwin".into(), flags.into()),
            ("CARGO_ENCODED_RUSTFLAGS".into(), rust),
        ],
        runtime: vec![
            (
                "ASAN_OPTIONS".into(),
                "detect_leaks=0:abort_on_error=1".into(),
            ),
            (
                "UBSAN_OPTIONS".into(),
                "print_stacktrace=1:halt_on_error=1".into(),
            ),
        ],
        target_dir: output.join("sanitized-target"),
    })
}

/// Builds every selected test binary once and maps entry name to
/// (executable, package directory).
fn build(
    selected: &[&Entry],
    sanitizer: Option<&Sanitizer>,
) -> Result<BTreeMap<&'static str, (PathBuf, PathBuf)>, String> {
    let mut command = Command::new("cargo");
    command.args([
        "test",
        "--locked",
        "--no-run",
        "--message-format=json-render-diagnostics",
    ]);
    let mut packages: Vec<&str> = selected.iter().map(|entry| entry.package).collect();
    packages.sort_unstable();
    packages.dedup();
    for package in &packages {
        command.args(["-p", package]);
    }
    let mut seen_lib = false;
    let mut tests: Vec<&str> = Vec::new();
    for entry in selected {
        match entry.test {
            None => seen_lib = true,
            Some(test) => tests.push(test),
        }
    }
    if seen_lib {
        command.arg("--lib");
    }
    tests.sort_unstable();
    tests.dedup();
    for test in tests {
        command.args(["--test", test]);
    }
    if let Some(sanitizer) = sanitizer {
        command
            .args(["--target", "aarch64-apple-darwin", "--target-dir"])
            .arg(&sanitizer.target_dir)
            .env_remove("RUSTFLAGS")
            .envs(sanitizer.build.iter().map(|(key, value)| (key, value)));
    }
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("start cargo: {error}"))?;
    if !output.status.success() {
        return Err("cargo test --no-run failed".into());
    }
    let mut binaries = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let (Some(executable), Some(manifest)) = (
            message["executable"].as_str(),
            message["manifest_path"].as_str(),
        ) else {
            continue;
        };
        let directory = Path::new(manifest)
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let package = directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let kind = message["target"]["kind"][0].as_str().unwrap_or_default();
        let target = message["target"]["name"].as_str().unwrap_or_default();
        for entry in selected {
            let matches_package = entry.package == package;
            let matches_target = match entry.test {
                None => kind == "lib",
                Some(test) => kind == "test" && target == test,
            };
            if matches_package && matches_target {
                binaries.insert(entry.name, (PathBuf::from(executable), directory.clone()));
            }
        }
    }
    Ok(binaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn options_parse_and_reject_invalid_values() {
        let options = parse(&strings(&[
            "--minutes",
            "2.5",
            "--seed",
            "0xabc",
            "--only",
            "core-json,store-tamper",
        ]))
        .unwrap();
        assert_eq!(options.minutes, 2.5);
        assert_eq!(options.only.unwrap(), ["core-json", "store-tamper"]);
        assert!(parse(&strings(&["--minutes", "0"])).is_err());
        assert!(parse(&strings(&["--seed", "xyz"])).is_err());
        assert!(parse(&strings(&["--bogus"])).is_err());
    }

    #[test]
    fn entry_names_are_unique() {
        let mut names: Vec<&str> = ENTRIES.iter().map(|entry| entry.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ENTRIES.len());
    }
}
