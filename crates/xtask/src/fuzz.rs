//! `cargo xtask fuzz`: coverage-guided libFuzzer campaigns over the untrusted
//! input boundaries (docs/ADVERSARIAL.md).
//!
//! The targets live in the separate `fuzz/` workspace and build only with
//! `cargo +nightly fuzz` (sanitizer coverage); the product toolchain stays the
//! pinned stable one. Each target has the input format of the `deadpan-chaos`
//! regression target of the same name, so:
//!
//! - `seeds` exports the chaos seeds (`DEADPAN_CHAOS_EXPORT_SEEDS`) into
//!   `fuzz/corpus/<target>`, plus request-then-response streams for the
//!   protocol targets' classified mode (selector 2).
//! - `run` builds every target once and runs them in parallel for a bounded
//!   time each, then merges (minimizes) new coverage back into the committed
//!   corpus. Crash, timeout and out-of-memory artifacts are copied to the
//!   report directory and fail the task.
//! - `replay` executes every committed corpus input once per target.
//!
//! The stable chaos regression suite also replays `fuzz/corpus/<name>` in
//! ordinary `cargo test`, so fixed reproducers stay in the gate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const USAGE: &str = "usage: cargo xtask fuzz list | seeds | replay [--only a,b] | run [--minutes N] [--jobs N] [--only a,b] [--output NEW_DIR] [--no-merge]";
const FUZZ_DIR: &str = "fuzz";
const TRIPLE: &str = "aarch64-apple-darwin";
/// libFuzzer limits: per-input wall time (s) and resident memory (MiB).
const TIMEOUT_SECONDS: u32 = 10;
const RSS_LIMIT_MB: u32 = 2048;
/// Inputs larger than this are not explored; seeds are far smaller.
const MAX_LEN: u32 = 256 * 1024;
/// Larger inputs (the chaos JSON depth/size bombs) are explored but not
/// committed; the chaos harness regenerates them.
const COMMIT_MAX: usize = 128 * 1024;

/// Chaos packages whose `adversarial` tests own the seed corpora.
const SEED_PACKAGES: &[&str] = &[
    "deadpan-analysis",
    "deadpan-app",
    "deadpan-cli",
    "deadpan-core",
    "deadpan-jobs",
    "deadpan-media",
    "deadpan-models",
    "deadpan-source",
];

/// Chaos targets whose seeds also feed a differently named fuzz target.
const SEED_ALIASES: &[(&str, &str)] = &[("core-macro-program", "core-register-value")];

struct Options {
    minutes: f64,
    jobs: usize,
    only: Option<Vec<String>>,
    output: Option<PathBuf>,
    merge: bool,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        minutes: 10.0,
        jobs: 8,
        only: None,
        output: None,
        merge: true,
    };
    let mut iter = arguments.iter();
    while let Some(argument) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{argument} needs a value. {USAGE}"))
        };
        match argument.as_str() {
            "--minutes" => {
                options.minutes = value()?
                    .parse()
                    .ok()
                    .filter(|minutes: &f64| {
                        minutes.is_finite() && *minutes > 0.0 && *minutes <= 1440.0
                    })
                    .ok_or_else(|| format!("--minutes must be in (0, 1440]. {USAGE}"))?;
            }
            "--jobs" => {
                options.jobs = value()?
                    .parse()
                    .ok()
                    .filter(|jobs| *jobs > 0 && *jobs <= 64)
                    .ok_or_else(|| format!("--jobs must be between 1 and 64. {USAGE}"))?;
            }
            "--only" => {
                options.only = Some(value()?.split(',').map(str::to_owned).collect());
            }
            "--output" => options.output = Some(PathBuf::from(value()?)),
            "--no-merge" => options.merge = false,
            other => return Err(format!("unknown argument {other}. {USAGE}")),
        }
    }
    Ok(options)
}

pub fn run(arguments: &[String]) -> Result<(), String> {
    let (command, rest) = arguments
        .split_first()
        .map_or(("", &[][..]), |(command, rest)| (command.as_str(), rest));
    match command {
        "list" => {
            for target in targets()? {
                println!("{target}");
            }
            Ok(())
        }
        "seeds" => seeds(),
        "replay" => replay(&parse(rest)?),
        "run" => campaign(&parse(rest)?),
        _ => Err(USAGE.into()),
    }
}

/// Every `fuzz/fuzz_targets/<name>.rs`, sorted.
fn targets() -> Result<Vec<String>, String> {
    let directory = Path::new(FUZZ_DIR).join("fuzz_targets");
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("read {}: {error}", directory.display()))?;
        if let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".rs"))
        {
            names.push(name.to_owned());
        }
    }
    if names.is_empty() {
        return Err(format!("no fuzz targets in {}", directory.display()));
    }
    names.sort();
    Ok(names)
}

fn selected(only: Option<&Vec<String>>) -> Result<Vec<String>, String> {
    let all = targets()?;
    if let Some(only) = only {
        for name in only {
            if !all.contains(name) {
                return Err(format!(
                    "unknown fuzz target {name}; see `cargo xtask fuzz list`"
                ));
            }
        }
    }
    Ok(all
        .into_iter()
        .filter(|name| only.is_none_or(|only| only.contains(name)))
        .collect())
}

fn corpus_dir(name: &str) -> PathBuf {
    Path::new(FUZZ_DIR).join("corpus").join(name)
}

fn binary(name: &str) -> PathBuf {
    Path::new(FUZZ_DIR)
        .join("target")
        .join(TRIPLE)
        .join("release")
        .join(name)
}

/// One sanitizer-coverage build of every target (`cargo +nightly fuzz build`).
fn build() -> Result<(), String> {
    // cargo-fuzz 0.13 has no --locked forwarding flag. Resolve the complete
    // graph with Cargo's lock enforcement first, build without networking,
    // and require the exact same lock afterward.
    let lock = Path::new(FUZZ_DIR).join("Cargo.lock");
    let locked_hash = file_sha256(&lock)?;
    let metadata = Command::new("cargo")
        .args([
            "+nightly",
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
        ])
        .current_dir(FUZZ_DIR)
        .stdout(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if !metadata.success() {
        return Err(
            "locked fuzz dependency preflight failed; fetch the locked dependencies first".into(),
        );
    }
    let status = Command::new("cargo")
        .args([
            "+nightly",
            "fuzz",
            "build",
            "--release",
            "--debug-assertions",
            "--target",
            TRIPLE,
        ])
        .env_remove("CARGO_TARGET_DIR")
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(FUZZ_DIR)
        .status()
        .map_err(|error| {
            format!("start cargo +nightly fuzz (is cargo-fuzz installed?): {error}")
        })?;
    if file_sha256(&lock)? != locked_hash {
        return Err("cargo-fuzz changed its dependency lock; refusing to run".into());
    }
    if !status.success() {
        return Err("cargo +nightly fuzz build failed".into());
    }
    Ok(())
}

fn seeds() -> Result<(), String> {
    let export = Path::new("target/fuzz-seeds");
    if export.exists() {
        std::fs::remove_dir_all(export).map_err(|error| error.to_string())?;
    }
    std::fs::create_dir_all(export).map_err(|error| error.to_string())?;
    let export = export.canonicalize().map_err(|error| error.to_string())?;
    let mut command = Command::new("cargo");
    command.args(["test", "--locked"]);
    for package in SEED_PACKAGES {
        command.args(["-p", package]);
    }
    let status = command
        .args(["adversarial", "--", "--test-threads", "4"])
        .env("DEADPAN_CHAOS_EXPORT_SEEDS", &export)
        .status()
        .map_err(|error| format!("start cargo test: {error}"))?;
    if !status.success() {
        return Err("seed export tests failed".into());
    }
    let names = targets()?;
    let mut summary = BTreeMap::new();
    for name in &names {
        let mut sources = vec![export.join(name)];
        for (from, to) in SEED_ALIASES {
            if to == name {
                sources.push(export.join(from));
            }
        }
        let mut seeds: Vec<Vec<u8>> = Vec::new();
        for source in &sources {
            seeds.extend(read_dir_files(source)?);
        }
        if name.ends_with("-protocol") {
            seeds.extend(classified_streams(&seeds));
        }
        if seeds.is_empty() {
            return Err(format!(
                "no chaos seeds were exported for fuzz target {name}"
            ));
        }
        let directory = corpus_dir(name);
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        seeds.retain(|seed| seed.len() <= COMMIT_MAX);
        for seed in &seeds {
            std::fs::write(directory.join(content_name(seed)), seed)
                .map_err(|error| error.to_string())?;
        }
        summary.insert(name.clone(), seeds.len());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).unwrap_or_default()
    );
    Ok(())
}

fn read_dir_files(directory: &Path) -> Result<Vec<Vec<u8>>, String> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in
        std::fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display())))
        .collect()
}

/// Content-addressed corpus file name (SHA-256, as libFuzzer uses SHA-1).
fn content_name(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .take(20)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The first length-prefixed frame of a framed stream, with its prefix.
fn first_frame(stream: &[u8]) -> Option<&[u8]> {
    let length = u32::from_be_bytes(stream.get(..4)?.try_into().ok()?) as usize;
    stream.get(..4 + length)
}

/// Selector-2 seeds: each host request seed's first frame followed by each
/// worker response seed's frames, so the classified mode starts productive.
fn classified_streams(seeds: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let requests: Vec<&[u8]> = seeds
        .iter()
        .filter(|seed| seed.first() == Some(&0))
        .filter_map(|seed| first_frame(&seed[1..]))
        .collect();
    let responses: Vec<&[u8]> = seeds
        .iter()
        .filter(|seed| seed.first() == Some(&1))
        .map(|seed| &seed[1..])
        .collect();
    let mut streams = Vec::new();
    for request in requests.iter().take(8) {
        for response in responses.iter().take(8) {
            let mut stream = vec![2];
            stream.extend_from_slice(request);
            stream.extend_from_slice(response);
            streams.push(stream);
        }
    }
    streams
}

fn replay(options: &Options) -> Result<(), String> {
    let names = selected(options.only.as_ref())?;
    build()?;
    let mut failed = Vec::new();
    for name in &names {
        if read_dir_files(&corpus_dir(name))?.is_empty() {
            return Err(format!(
                "{name} has no readable corpus; run `cargo xtask fuzz seeds`"
            ));
        }
        let output = Command::new(binary(name))
            .arg(corpus_dir(name))
            .args([
                "-runs=0".to_owned(),
                format!("-timeout={TIMEOUT_SECONDS}"),
                format!("-rss_limit_mb={RSS_LIMIT_MB}"),
            ])
            .stdout(Stdio::null())
            .output()
            .map_err(|error| format!("run {name}: {error}"))?;
        let inputs = read_dir_files(&corpus_dir(name))?.len();
        if output.status.success() {
            eprintln!("fuzz replay: {name}: {inputs} inputs ok");
        } else {
            eprintln!(
                "fuzz replay: {name} FAILED\n{}",
                tail(&String::from_utf8_lossy(&output.stderr), 40)
            );
            failed.push(name.clone());
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("corpus replay failed: {}", failed.join(", ")))
    }
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// The final `#N DONE cov: X ft: Y corp: A/Bb ...` statistics.
fn statistics(log: &str) -> Value {
    let line = log
        .lines()
        .rev()
        .find(|line| line.contains(" DONE ") || line.contains("cov: "))
        .unwrap_or_default();
    let field = |key: &str| {
        line.split_whitespace()
            .skip_while(|word| *word != key)
            .nth(1)
            .map(str::to_owned)
    };
    let executions = line
        .split_whitespace()
        .next()
        .and_then(|word| word.strip_prefix('#'))
        .and_then(|count| count.parse::<u64>().ok());
    json!({
        "executions": executions,
        "edges": field("cov:").and_then(|value| value.parse::<u64>().ok()),
        "features": field("ft:").and_then(|value| value.parse::<u64>().ok()),
        "corpus": field("corp:"),
        "rss": field("rss:"),
    })
}

fn campaign(options: &Options) -> Result<(), String> {
    let names = selected(options.only.as_ref())?;
    let output = options.output.clone().unwrap_or_else(|| {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_secs());
        PathBuf::from("target/fuzz").join(stamp.to_string())
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
    let sources = source_manifest()?;
    let source_bytes = serde_json::to_vec_pretty(&sources).map_err(|error| error.to_string())?;
    std::fs::write(output.join("source-files.json"), &source_bytes)
        .map_err(|error| error.to_string())?;
    build()?;
    if source_manifest()? != sources {
        return Err("source files changed while the fuzz executables were building; rerun from stable source".into());
    }
    let seconds = (options.minutes * 60.0).round().max(1.0) as u64;
    eprintln!(
        "fuzz: {} targets, {seconds} s each, {} at a time; report in {}",
        names.len(),
        options.jobs,
        output.display()
    );
    // Capture source/build identity before workers or corpus merging mutate
    // anything. A dirty flag alone cannot identify the code that was run.
    let provenance = json!({
        "revision": git(&["rev-parse", "HEAD"]),
        "dirty_worktree": git(&["status", "--porcelain"]).is_some_and(|text| !text.is_empty()),
        "tracked_diff_sha256": git(&["diff", "HEAD", "--binary"]).map(|diff| sha256(diff.as_bytes())),
        "source_manifest": "source-files.json",
        "source_manifest_sha256": sha256(&source_bytes),
        "rustc": tool_version("rustc", &["+nightly", "-vV"]),
        "cargo_fuzz": tool_version("cargo", &["+nightly", "fuzz", "--version"]),
        "fuzz_lock_sha256": file_sha256(Path::new(FUZZ_DIR).join("Cargo.lock").as_path())?,
        "platform": TRIPLE,
    });
    let queue = Arc::new(Mutex::new(names.clone()));
    let results = Arc::new(Mutex::new(BTreeMap::new()));
    let started = Instant::now();
    let workers: Vec<_> = (0..options.jobs.min(names.len()))
        .map(|_| {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let output = output.clone();
            std::thread::spawn(move || {
                while let Some(name) = queue.lock().ok().and_then(|mut queue| queue.pop()) {
                    let result = one_campaign(&name, seconds, &output);
                    if let Ok(mut results) = results.lock() {
                        results.insert(name, result);
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker
            .join()
            .map_err(|_| "a fuzz worker thread panicked".to_owned())?;
    }
    let results = results
        .lock()
        .map_err(|_| "fuzz results poisoned".to_owned())?
        .clone();
    let mut failures = Vec::new();
    for name in &names {
        if !results.contains_key(name) {
            failures.push(format!("{name} (missing result)"));
        }
    }
    for (name, result) in &results {
        match result {
            Ok(report) => {
                if !report_clean(report) {
                    failures.push(name.clone());
                }
            }
            Err(error) => failures.push(format!("{name} ({error})")),
        }
    }
    let mut summary = json!({
        "provenance": provenance,
        "seconds_per_target": seconds,
        "jobs": options.jobs,
        "wall_seconds": started.elapsed().as_secs(),
        "merged": false,
        "targets": results
            .iter()
            .map(|(name, result)| (
                name.clone(),
                result.clone().unwrap_or_else(|error| json!({"error": error})),
            ))
            .collect::<serde_json::Map<_, _>>(),
        "failures": failures,
    });
    write_summary(&output, &summary)?;
    // Keep the complete failing run and its report. Never feed a known
    // failure into corpus minimization, which can lose the reproducer.
    if options.merge && failures.is_empty() {
        let mut merged = Vec::new();
        for name in &names {
            if let Err(error) = merge(name, &output) {
                summary["merge_error"] = json!(error);
                summary["merged_targets"] = json!(merged);
                write_summary(&output, &summary)?;
                return Err(error);
            }
            merged.push(name);
        }
        summary["merged"] = json!(true);
        summary["merged_targets"] = json!(merged);
        write_summary(&output, &summary)?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).unwrap_or_default()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "fuzz targets found failures: {}; artifacts below {}",
            failures.join(", "),
            output.display()
        ))
    }
}

/// One target for `seconds`, exploring from the committed corpus into a
/// private working corpus. Returns its statistics and any artifacts.
fn one_campaign(name: &str, seconds: u64, output: &Path) -> Result<Value, String> {
    if read_dir_files(&corpus_dir(name))?.is_empty() {
        return Err(format!(
            "{name} has no readable corpus; run `cargo xtask fuzz seeds`"
        ));
    }
    let binary_sha256 = file_sha256(&binary(name))?;
    let work = output.join("corpus").join(name);
    let artifacts = output.join("artifacts").join(name);
    std::fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&artifacts).map_err(|error| error.to_string())?;
    let log_path = output.join(format!("{name}.log"));
    let log = std::fs::File::create(&log_path).map_err(|error| error.to_string())?;
    let mut prefix = artifacts.as_os_str().to_owned();
    prefix.push("/");
    let status = Command::new(binary(name))
        .arg(&work)
        .arg(corpus_dir(name))
        .args([
            format!("-max_total_time={seconds}"),
            format!("-timeout={TIMEOUT_SECONDS}"),
            format!("-rss_limit_mb={RSS_LIMIT_MB}"),
            format!("-max_len={MAX_LEN}"),
            "-print_final_stats=1".to_owned(),
        ])
        .arg({
            let mut argument = std::ffi::OsString::from("-artifact_prefix=");
            argument.push(&prefix);
            argument
        })
        .stdout(Stdio::null())
        .stderr(log)
        .status()
        .map_err(|error| format!("run {name}: {error}"))?;
    let text = std::fs::read_to_string(&log_path)
        .map_err(|error| format!("read {}: {error}", log_path.display()))?;
    let mut found: Vec<String> = std::fs::read_dir(&artifacts)
        .map_err(|error| error.to_string())?
        .map(|entry| {
            entry
                .map(|entry| entry.path().display().to_string())
                .map_err(|error| error.to_string())
        })
        .collect::<Result<_, _>>()?;
    found.sort();
    eprintln!(
        "fuzz: {name} exit {:?}, {} artifact(s)",
        status.code(),
        found.len()
    );
    let mut report = statistics(&text);
    report["binary_sha256"] = json!(binary_sha256);
    report["success"] = json!(status.success());
    report["exit"] = json!(status.code());
    report["artifacts"] = json!(found);
    report["log"] = json!(log_path.display().to_string());
    Ok(report)
}

/// Minimize the committed corpus plus this run's discoveries into a fresh
/// directory with libFuzzer's coverage merge, then replace the committed one.
fn merge(name: &str, output: &Path) -> Result<(), String> {
    let merged = output.join("merged").join(name);
    std::fs::create_dir_all(&merged).map_err(|error| error.to_string())?;
    let status = Command::new(binary(name))
        .args([
            "-merge=1".to_owned(),
            format!("-timeout={TIMEOUT_SECONDS}"),
            format!("-rss_limit_mb={RSS_LIMIT_MB}"),
            format!("-max_len={MAX_LEN}"),
            "-max_total_time=300".to_owned(),
        ])
        .arg(&merged)
        .arg(corpus_dir(name))
        .arg(output.join("corpus").join(name))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("merge {name}: {error}"))?;
    if !status.success() {
        return Err(format!("corpus merge for {name} failed"));
    }
    let mut kept = read_dir_files(&merged)?;
    kept.retain(|input| input.len() <= COMMIT_MAX);
    if kept.is_empty() {
        return Err(format!("corpus merge for {name} kept nothing"));
    }
    let committed = corpus_dir(name);
    // Preserve original fixtures and fixed reproducers. The merge minimizes
    // new discoveries, but never deletes previously retained inputs.
    std::fs::create_dir_all(&committed).map_err(|error| error.to_string())?;
    for input in &kept {
        std::fs::write(committed.join(content_name(input)), input)
            .map_err(|error| error.to_string())?;
    }
    eprintln!(
        "fuzz: {name} merged {} inputs, preserving the committed corpus",
        kept.len()
    );
    Ok(())
}

fn report_clean(report: &Value) -> bool {
    report["success"] == true
        && report["artifacts"].as_array().is_some_and(Vec::is_empty)
        && ["executions", "edges", "features"]
            .iter()
            .all(|key| report[key].as_u64().is_some_and(|value| value > 0))
}

fn write_summary(output: &Path, summary: &Value) -> Result<(), String> {
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(summary).map_err(|e| e.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn file_sha256(path: &Path) -> Result<String, String> {
    use sha2::Digest;
    use std::io::Read;
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut hash = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let length = file
            .read(&mut buffer)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if length == 0 {
            break;
        }
        hash.update(&buffer[..length]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Identify tracked and untracked source inputs without storing project files
/// or ignored build products. Deleted paths remain explicit in the manifest.
fn source_manifest() -> Result<BTreeMap<String, Value>, String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("could not enumerate source files".into());
    }
    let mut manifest = BTreeMap::new();
    for bytes in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
    {
        let name = std::str::from_utf8(bytes).map_err(|_| "source path is not UTF-8")?;
        let path = Path::new(name);
        let value = match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() => {
                json!({"sha256":file_sha256(path)?, "bytes":metadata.len()})
            }
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = std::fs::read_link(path).map_err(|error| error.to_string())?;
                json!({"symlink_sha256":sha256(target.as_os_str().as_encoded_bytes())})
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({"deleted":true}),
            Err(error) => return Err(format!("{name}: {error}")),
            Ok(_) => return Err(format!("unsupported source file type: {name}")),
        };
        manifest.insert(name.to_owned(), value);
    }
    Ok(manifest)
}

fn tool_version(tool: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(tool).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git(arguments: &[&str]) -> Option<String> {
    Command::new("git")
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
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
            "0.5",
            "--jobs",
            "3",
            "--only",
            "a,b",
        ]))
        .expect("valid options");
        assert_eq!(options.minutes, 0.5);
        assert_eq!(options.jobs, 3);
        assert_eq!(options.only, Some(strings(&["a", "b"])));
        assert!(options.merge);
        assert!(parse(&strings(&["--minutes", "0"])).is_err());
        assert!(parse(&strings(&["--jobs", "0"])).is_err());
        assert!(parse(&strings(&["--bogus"])).is_err());
        assert!(!parse(&strings(&["--no-merge"])).unwrap().merge);
    }

    #[test]
    fn classified_streams_pair_first_requests_with_responses() {
        let frame = |body: &[u8]| {
            let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
            bytes.extend_from_slice(body);
            bytes
        };
        let request = [vec![0], frame(b"{\"a\":1}"), frame(b"{\"b\":2}")].concat();
        let response = [vec![1], frame(b"{\"c\":3}")].concat();
        let streams = classified_streams(&[request, response, vec![1]]);
        assert_eq!(
            streams,
            vec![
                [vec![2], frame(b"{\"a\":1}"), frame(b"{\"c\":3}")].concat(),
                [vec![2], frame(b"{\"a\":1}")].concat(),
            ]
        );
        assert_eq!(first_frame(&[0, 0, 0, 9, 1]), None, "truncated frame");
    }

    #[test]
    fn statistics_read_the_final_libfuzzer_line() {
        let stats = statistics(
            "#1 INITED cov: 3 ft: 4\n#2000\tDONE   cov: 396 ft: 481 corp: 53/130b lim: 4 exec/s: 0 rss: 43Mb\n",
        );
        assert_eq!(stats["executions"], 2000);
        assert_eq!(stats["edges"], 396);
        assert_eq!(stats["features"], 481);
        assert_eq!(stats["corpus"], "53/130b");
    }

    #[test]
    fn failures_without_artifacts_never_pass() {
        assert!(report_clean(
            &json!({"success": true, "artifacts": [], "executions": 100, "edges": 5, "features": 8})
        ));
        assert!(!report_clean(&json!({"success": true, "artifacts": []})));
        assert!(!report_clean(
            &json!({"success": false, "exit": 1, "artifacts": []})
        ));
        assert!(!report_clean(
            &json!({"success": false, "exit": null, "artifacts": []})
        ));
        assert!(!report_clean(
            &json!({"success": true, "artifacts": ["crash"]})
        ));
        assert!(!report_clean(&json!({})));
    }

    #[test]
    fn every_fuzz_target_has_a_committed_corpus() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let names: Vec<String> = std::fs::read_dir(root.join("fuzz/fuzz_targets"))
            .expect("fuzz targets")
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.strip_suffix(".rs"))
                    .map(str::to_owned)
            })
            .collect();
        assert!(names.len() >= 20, "fuzz targets missing: {names:?}");
        let manifest = std::fs::read_to_string(root.join("fuzz/Cargo.toml")).expect("manifest");
        for name in &names {
            assert!(
                manifest.contains(&format!("name = \"{name}\"")),
                "{name} has no [[bin]] entry"
            );
            let corpus = root.join("fuzz/corpus").join(name);
            assert!(
                std::fs::read_dir(&corpus).is_ok_and(|mut entries| entries.next().is_some()),
                "{name} has no committed corpus; run `cargo xtask fuzz seeds`"
            );
        }
    }
}
