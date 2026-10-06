//! `cargo xtask perf`: the reproducible Section 25 benchmark suite.
//!
//! Builds the release CLI, media worker and `perf` example once, copies them
//! into `OUTPUT/bin` (a concurrent rebuild cannot swap a running binary), and
//! runs every stage under `/usr/bin/time -l` on private copies of the named
//! fixture packages; the given packages are never opened writable. Optional
//! `--generate` makes long synthetic H.264/AAC fixtures with the `ffmpeg` on
//! PATH and imports each as a one-Original project. `--ui` also builds the
//! ui-harness app in release and replays its gated performance scenarios.
//!
//! Writes `OUTPUT/summary.json` (environment, per-stage results, peak RSS and
//! footprint, and each Section 25 target with its measured value) plus every
//! stage's raw JSON and log. See docs/PERFORMANCE.md.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const USAGE: &str = "usage: cargo xtask perf [--output NEW_DIR] [--fixture NAME=PACKAGE]... [--generate] [--ui] [--stages doctor,scale,seek,edit,playback,export,stress,ui] [--audition-seconds N] [--max-load L] [--quick]";
const ALL_STAGES: [&str; 6] = ["doctor", "scale", "seek", "edit", "playback", "export"];
/// Stages run only when named in `--stages` (or `--ui`).
const OPT_IN_STAGES: [&str; 2] = ["stress", "ui"];
/// Fragment length and plays of the jumpy `-cuts` playback fixture.
const CUT_EVERY: &str = "24";
const CUT_PLAYS: &str = "3";
/// Minimum samples before a distribution can PASS or FAIL a target.
const MIN_SEEKS: u64 = 100;
const MIN_EDITS: u64 = 20;
const MIN_UI_SAMPLES: u64 = 40;
/// Longest wait for the 1-minute load average to fall below `--max-load`.
const QUIET_WAIT_SECONDS: u64 = 600;
const UI_SCENARIOS: [&str; 3] = ["rapid-input", "edit-latency", "large-project"];
const LARGE_BEATS: u64 = 10_000;

struct Options {
    output: Option<PathBuf>,
    fixtures: Vec<(String, PathBuf)>,
    generate: bool,
    ui: bool,
    stages: Vec<String>,
    audition_seconds: u64,
    max_load: f64,
    quick: bool,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        output: None,
        fixtures: Vec::new(),
        generate: false,
        ui: false,
        stages: ALL_STAGES.map(str::to_owned).to_vec(),
        audition_seconds: 30,
        max_load: 3.0,
        quick: false,
    };
    let mut arguments = arguments.iter();
    while let Some(flag) = arguments.next() {
        match flag.as_str() {
            "--generate" => options.generate = true,
            "--ui" => options.ui = true,
            "--quick" => options.quick = true,
            _ => {
                let value = arguments
                    .next()
                    .ok_or_else(|| format!("missing value for {flag}; {USAGE}"))?;
                match flag.as_str() {
                    "--output" => options.output = Some(PathBuf::from(value)),
                    "--fixture" => {
                        let (name, path) = value
                            .split_once('=')
                            .ok_or_else(|| format!("--fixture needs NAME=PACKAGE; {USAGE}"))?;
                        if name.is_empty()
                            || !name
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                        {
                            return Err(format!("fixture name {name:?} must be [A-Za-z0-9_-]"));
                        }
                        options
                            .fixtures
                            .push((name.to_owned(), PathBuf::from(path)));
                    }
                    "--stages" => {
                        options.stages = value.split(',').map(str::to_owned).collect();
                        if let Some(unknown) = options.stages.iter().find(|stage| {
                            !ALL_STAGES.contains(&stage.as_str())
                                && !OPT_IN_STAGES.contains(&stage.as_str())
                        }) {
                            return Err(format!("unknown stage {unknown}; {USAGE}"));
                        }
                    }
                    "--max-load" => {
                        options.max_load = value
                            .parse()
                            .ok()
                            .filter(|load: &f64| load.is_finite() && *load > 0.0)
                            .ok_or("--max-load must be a positive number")?;
                    }
                    "--audition-seconds" => {
                        options.audition_seconds = value
                            .parse()
                            .ok()
                            .filter(|seconds| (1..=600).contains(seconds))
                            .ok_or("--audition-seconds must be 1 to 600")?;
                    }
                    _ => return Err(format!("unknown option {flag}; {USAGE}")),
                }
            }
        }
    }
    if options.ui && !options.stages.iter().any(|stage| stage == "ui") {
        options.stages.push("ui".into());
    }
    Ok(options)
}

pub fn run(arguments: &[String]) -> Result<(), String> {
    let options = parse(arguments)?;
    let output = match &options.output {
        Some(path) => path.clone(),
        None => std::env::temp_dir().join(format!("deadpan-perf-{}", unix_seconds()?)),
    };
    std::fs::create_dir(&output)
        .map_err(|error| format!("create new output directory {}: {error}", output.display()))?;
    let output = output
        .canonicalize()
        .map_err(|error| format!("canonicalize {}: {error}", output.display()))?;
    let mut summary = json!({
        "schema_version": 1,
        "started_unix": unix_seconds()?,
        "command": std::env::args().collect::<Vec<_>>(),
        "environment": environment(),
    });
    let bin = output.join("bin");
    let build_started = Instant::now();
    let binaries = build(&bin)?;
    summary["build"] =
        json!({"seconds": build_started.elapsed().as_secs_f64(), "binaries": binaries});
    let cli = bin.join("deadpan-cli");
    let perf = bin.join("perf");
    eprintln!(
        "perf: built and copied release binaries to {}",
        bin.display()
    );

    // Private package copies; inputs are only read.
    let fixtures_dir = output.join("fixtures");
    std::fs::create_dir(&fixtures_dir).map_err(|error| error.to_string())?;
    let mut media = Vec::new();
    for (name, path) in &options.fixtures {
        let copy = fixtures_dir.join(format!("{name}.deadpan"));
        copy_package(path, &copy)?;
        media.push((name.clone(), copy));
    }
    let mut fixture_report = serde_json::Map::new();
    if options.generate {
        for (name, recipe) in generated_recipes(options.quick) {
            let movie = fixtures_dir.join(format!("{name}.mp4"));
            let started = Instant::now();
            ffmpeg(&recipe, &movie)?;
            let generated = started.elapsed().as_secs_f64();
            let package = fixtures_dir.join(format!("{name}.deadpan"));
            let import = measured(
                &cli,
                &[
                    "project",
                    "create-original",
                    path_str(&package)?,
                    path_str(&movie)?,
                ],
                &output.join(format!("import-{name}.log")),
            )?;
            if !import.success {
                return Err(format!("importing {name} failed; see import-{name}.log"));
            }
            fixture_report.insert(
                name.to_owned(),
                json!({"ffmpeg": recipe, "generation_seconds": generated,
                    "movie_bytes": file_len(&movie), "import": import.resources()}),
            );
            media.push((name.to_owned(), package));
        }
    }
    let large = fixtures_dir.join(format!("large-{LARGE_BEATS}.deadpan"));
    let made = stage_json(
        &perf,
        &[
            "make-large",
            path_str(&large)?,
            "--beats",
            &LARGE_BEATS.to_string(),
        ],
        &output,
        "make-large",
    )?;
    fixture_report.insert(format!("large-{LARGE_BEATS}"), made);
    summary["fixtures"] = Value::Object(fixture_report);
    if media.is_empty() {
        eprintln!("perf: no media fixtures; pass --fixture NAME=PACKAGE or --generate");
    }

    let work = output.join("work");
    std::fs::create_dir(&work).map_err(|error| error.to_string())?;
    let mut results: BTreeMap<String, Value> = BTreeMap::new();
    let wants = |stage: &str| options.stages.iter().any(|selected| selected == stage);
    let all: Vec<(String, PathBuf)> = media
        .iter()
        .cloned()
        .chain(std::iter::once((
            format!("large-{LARGE_BEATS}"),
            large.clone(),
        )))
        .collect();

    if wants("doctor") {
        for (name, package) in &all {
            let report = stage_json(
                &cli,
                &["doctor", "--project", path_str(package)?],
                &output,
                &format!("doctor-{name}"),
            )?;
            results.insert(format!("doctor/{name}"), report);
        }
    }
    if wants("scale") {
        let sizes = if options.quick {
            "100,1000,10000"
        } else {
            "100,1000,10000,50000"
        };
        let mut arguments = vec!["scale", "--sizes", sizes];
        if let Some((_, package)) = media.first() {
            arguments.extend(["--source", path_str(package)?, "--fragments", "10000"]);
        }
        results.insert(
            "scale".into(),
            stage_json(&perf, &arguments, &output, "scale")?,
        );
    }
    if wants("seek") {
        let (cold, warm, step) = if options.quick {
            ("2", "40", "60")
        } else {
            ("10", "200", "240")
        };
        let worker = bin.join("deadpan-media-worker");
        for (name, package) in &media {
            // Proxy seeks build into a private cache root, never the user's.
            let proxies = work.join(format!("proxies-{name}"));
            let load = quiet(options.max_load);
            let mut report = stage_json(
                &perf,
                &[
                    "seek",
                    path_str(package)?,
                    "--cold",
                    cold,
                    "--warm",
                    warm,
                    "--step",
                    step,
                    "--proxy-cache",
                    path_str(&proxies)?,
                    "--worker",
                    path_str(&worker)?,
                ],
                &output,
                &format!("seek-{name}"),
            )?;
            report["load"] = load;
            results.insert(format!("seek/{name}"), report);
        }
    }
    if wants("edit") {
        let cycles = if options.quick { "8" } else { "30" };
        for (name, package) in &all {
            let copy = work.join(format!("edit-{name}.deadpan"));
            copy_package(package, &copy)?;
            let load = quiet(options.max_load);
            let mut report = stage_json(
                &perf,
                &["edit", path_str(&copy)?, "--cycles", cycles],
                &output,
                &format!("edit-{name}"),
            )?;
            report["load"] = load;
            results.insert(format!("edit/{name}"), report);
        }
    }
    if wants("playback") {
        let seconds = options.audition_seconds.to_string();
        let worker = bin.join("deadpan-media-worker");
        for (name, package) in &media {
            // The seek stage's private proxy cache, or a new one.
            let proxies = work.join(format!("proxies-{name}"));
            let copy = work.join(format!("playback-{name}.deadpan"));
            copy_package(package, &copy)?;
            // A YTP-like edit of the same Original: a Repeat restart (a
            // backward jump needing a keyframe seek) every few seconds.
            let cuts = work.join(format!("playback-{name}-cuts.deadpan"));
            copy_package(package, &cuts)?;
            let made = stage_json(
                &perf,
                &[
                    "make-cuts",
                    path_str(&cuts)?,
                    "--every",
                    CUT_EVERY,
                    "--plays",
                    CUT_PLAYS,
                    "--seconds",
                    &(options.audition_seconds + 10).to_string(),
                ],
                &output,
                &format!("make-cuts-{name}"),
            )?;
            results.insert(format!("fixture/{name}-cuts"), made);
            // The native look-ahead decoder is on; one cuts run without it
            // measures what it changes.
            for (label, template, policy, lookahead) in [
                (name.clone(), &copy, "original", "on"),
                (format!("{name}+proxy"), &copy, "adaptive", "on"),
                (format!("{name}-cuts"), &cuts, "original", "on"),
                (format!("{name}-cuts+proxy"), &cuts, "adaptive", "on"),
                (format!("{name}-cuts-nolookahead"), &cuts, "original", "off"),
            ] {
                // Each run opens its own copy writable.
                let package = &work.join(format!("playback-run-{label}.deadpan"));
                copy_package(template, package)?;
                let load = quiet(options.max_load);
                let mut arguments = vec![
                    "playback",
                    path_str(package)?,
                    "--seconds",
                    &seconds,
                    "--pictures",
                    policy,
                    "--lookahead",
                    lookahead,
                ];
                if policy == "adaptive" {
                    arguments.extend([
                        "--proxy-cache",
                        path_str(&proxies)?,
                        "--worker",
                        path_str(&worker)?,
                    ]);
                }
                let mut report =
                    stage_json(&perf, &arguments, &output, &format!("playback-{label}"))?;
                report["load"] = load;
                results.insert(format!("playback/{label}"), report);
            }
        }
    }
    if wants("export") {
        for (name, package) in &media {
            let copy = work.join(format!("export-{name}.deadpan"));
            copy_package(package, &copy)?;
            let destination = work.join(format!("export-{name}"));
            std::fs::create_dir(&destination).map_err(|error| error.to_string())?;
            let load = quiet(options.max_load);
            let mut report = export(&cli, &copy, &destination, &output, name)?;
            report["load"] = load;
            results.insert(format!("export/{name}"), report);
        }
    }
    if wants("stress") {
        stress(
            &options,
            &bin,
            &cli,
            &perf,
            &output,
            &work,
            &media,
            &large,
            &mut results,
        )?;
    }
    if wants("ui") {
        results.insert("ui".into(), ui(&output, options.max_load)?);
    }
    summary["max_load"] = json!(options.max_load);
    summary["quick"] = json!(options.quick);
    summary["results"] = json!(results);
    summary["targets"] = targets(&results, options.quick);
    summary["finished_unix"] = json!(unix_seconds()?);
    summary["environment_after"] = json!({"load": command_text("uptime", &[]),
        "thermal": command_text("pmset", &["-g", "therm"])});
    let path = output.join("summary.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("write {}: {error}", path.display()))?;
    for target in summary["targets"].as_array().into_iter().flatten() {
        eprintln!(
            "perf: {:<4} {:<44} {}",
            target["status"].as_str().unwrap_or("?"),
            target["target"].as_str().unwrap_or("?"),
            target["measured"]
        );
    }
    eprintln!("perf: wrote {}", path.display());
    Ok(())
}

fn unix_seconds() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs())
}

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("{} is not UTF-8", path.display()))
}

fn file_len(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|metadata| metadata.len())
}

fn command_text(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Hardware, OS, power, load and source identity for the report header.
fn environment() -> Value {
    let sysctl = |name: &str| command_text("sysctl", &["-n", name]);
    let status = command_text("git", &["status", "--porcelain"]);
    // Content identity of uncommitted tracked changes; untracked paths are
    // listed in `git_dirty`. Binary SHA-256 digests remain the run identity.
    let diff = Command::new("git")
        .args(["diff", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success());
    let diff_sha256 = diff.map(|output| {
        use sha2::{Digest, Sha256};
        Sha256::digest(&output.stdout)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    });
    json!({
        "cpu": sysctl("machdep.cpu.brand_string"),
        "model": sysctl("hw.model"),
        "cores": sysctl("hw.ncpu"),
        "memory_bytes": sysctl("hw.memsize").and_then(|value| value.parse::<u64>().ok()),
        "os": command_text("sw_vers", &["-productVersion"]),
        "os_build": command_text("sw_vers", &["-buildVersion"]),
        "power_source": command_text("pmset", &["-g", "ps"]).and_then(|text| text.lines().next().map(str::to_owned)),
        "low_power_mode": command_text("pmset", &["-g"]).and_then(|text| {
            text.lines()
                .find(|line| line.contains("lowpowermode") || line.contains("powermode"))
                .map(|line| line.trim().to_owned())
        }),
        "thermal": command_text("pmset", &["-g", "therm"]),
        "load": command_text("uptime", &[]),
        "rustc": command_text("rustc", &["-V"]),
        "git_revision": command_text("git", &["rev-parse", "HEAD"]),
        "git_diff_sha256": diff_sha256,
        "source_note": if status.is_none() { Some("not a Git checkout; identify the source by the binary digests") } else { None },
        "load_1m": load_average(),
        "git_dirty_paths": status.as_ref().map(|text| text.lines().count()),
        "git_dirty": status.as_ref().map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>()),
        "ffmpeg_prefix": std::env::var("DEADPAN_FFMPEG_PREFIX").ok(),
        "build_profile": "release",
    })
}

/// Release-build the CLI, media worker and perf example; copy them together
/// so the render path finds its worker beside the CLI.
fn build(directory: &Path) -> Result<Value, String> {
    let output = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "deadpan-cli",
            "-p",
            "deadpan-media-worker",
            "--bins",
            "--example",
            "perf",
            "--message-format=json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("failed to start cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!("release build exited with {}", output.status));
    }
    let built: Vec<(String, PathBuf)> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter_map(|message| {
            Some((
                message["target"]["name"].as_str()?.to_owned(),
                PathBuf::from(message["executable"].as_str()?),
            ))
        })
        .collect();
    std::fs::create_dir(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let mut binaries = serde_json::Map::new();
    for name in ["deadpan-cli", "deadpan-media-worker", "perf"] {
        let source = built
            .iter()
            .find(|(built, _)| built == name)
            .map(|(_, path)| path)
            .ok_or_else(|| format!("cargo did not report a {name} executable"))?;
        let copy = directory.join(name);
        std::fs::copy(source, &copy)
            .map_err(|error| format!("copy {} to {}: {error}", source.display(), copy.display()))?;
        binaries.insert(
            name.to_owned(),
            json!({"source": source, "sha256": crate::replays::sha256(&copy)?}),
        );
    }
    Ok(Value::Object(binaries))
}

/// Copy a package only while no writer holds it. A shared lock on
/// `.writer.lock` is held for the whole copy, so no writer can start and the
/// database, WAL and shared-memory files are copied quiescent and together.
/// A package held open by the app or CLI is refused rather than copied
/// mid-transaction.
fn copy_package(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.join("project.sqlite").is_file() {
        return Err(format!("{} is not a Deadpan package", source.display()));
    }
    let lock_path = source.join(".writer.lock");
    let _lock = match std::fs::File::open(&lock_path) {
        Ok(lock) => match lock.try_lock_shared() {
            Ok(()) => Some(lock),
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(format!(
                    "{} is open for writing; close it before copying",
                    source.display()
                ));
            }
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(format!("lock {}: {error}", lock_path.display()));
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("open {}: {error}", lock_path.display())),
    };
    let status = Command::new("cp")
        .arg("-Rp")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("copying {} failed", source.display()));
    }
    Ok(())
}

/// The 1-minute load average from `vm.loadavg` (`{ 1.43 1.91 3.13 }`).
fn load_average() -> Option<f64> {
    command_text("sysctl", &["-n", "vm.loadavg"])?
        .trim_matches(|c: char| c == '{' || c == '}' || c.is_whitespace())
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Wait (bounded) for a quiet machine before a load-sensitive stage. The
/// stage still runs when the wait expires, but is flagged, and its targets
/// are reported INFO rather than PASS or FAIL.
fn quiet(max_load: f64) -> Value {
    let started = Instant::now();
    let mut load = load_average();
    while load.is_none_or(|load| load > max_load)
        && started.elapsed().as_secs() < QUIET_WAIT_SECONDS
    {
        std::thread::sleep(std::time::Duration::from_secs(5));
        load = load_average();
    }
    let flagged = load.is_none_or(|load| load > max_load);
    if flagged {
        eprintln!(
            "perf: load {load:?} still above {max_load} after {QUIET_WAIT_SECONDS} s; flagging stage"
        );
    }
    json!({"load_1m_at_start": load, "max_load": max_load,
        "waited_seconds": started.elapsed().as_secs(), "flagged": flagged})
}

/// Deterministic long fixtures: SMPTE-like test pattern and a sine tone, H.264
/// High with x264's default long GOP, explicit BT.709 limited-range tags.
fn generated_recipes(quick: bool) -> Vec<(&'static str, Vec<String>)> {
    let recipe = |size: &str, rate: u32, seconds: u32, tone: u32| {
        [
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size={size}:rate={rate}:duration={seconds}"),
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={tone}:sample_rate=48000:duration={seconds}"),
            "-ac",
            "2",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-bsf:v",
            "h264_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1",
            "-color_range",
            "tv",
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-movflags",
            "+faststart",
        ]
        .map(str::to_owned)
        .to_vec()
    };
    let scale = if quick { 4 } else { 1 };
    vec![
        ("gen-1080p60", recipe("1920x1080", 60, 120 / scale, 440)),
        ("gen-4k30", recipe("3840x2160", 30, 60 / scale, 330)),
    ]
}

fn ffmpeg(recipe: &[String], movie: &Path) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args(recipe)
        .arg(movie)
        .status()
        .map_err(|error| format!("ffmpeg is needed for --generate: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg exited with {status}"))
    }
}

/// One process measured by `/usr/bin/time -l`.
struct Measured {
    success: bool,
    stdout: String,
    wall_seconds: f64,
    user_seconds: Option<f64>,
    system_seconds: Option<f64>,
    max_rss_bytes: Option<u64>,
    peak_footprint_bytes: Option<u64>,
}

impl Measured {
    fn resources(&self) -> Value {
        json!({
            "wall_seconds": self.wall_seconds,
            "user_seconds": self.user_seconds,
            "system_seconds": self.system_seconds,
            "max_rss_bytes": self.max_rss_bytes,
            "peak_footprint_bytes": self.peak_footprint_bytes,
            "note": "max RSS covers the process and its reaped children; peak footprint is the measured process itself",
        })
    }
}

fn parse_time(stderr: &str, measured: &mut Measured) {
    for line in stderr.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.as_slice() {
            [real, "real", user, "user", system, "sys"] => {
                measured.wall_seconds = real.parse().unwrap_or(measured.wall_seconds);
                measured.user_seconds = user.parse().ok();
                measured.system_seconds = system.parse().ok();
            }
            [value, "maximum", "resident", "set", "size"] => {
                measured.max_rss_bytes = value.parse().ok();
            }
            [value, "peak", "memory", "footprint"] => {
                measured.peak_footprint_bytes = value.parse().ok();
            }
            _ => {}
        }
    }
}

fn measured(program: &Path, arguments: &[&str], log: &Path) -> Result<Measured, String> {
    let started = Instant::now();
    let output = Command::new("/usr/bin/time")
        .arg("-l")
        .arg(program)
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("run {}: {error}", program.display()))?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    std::fs::write(log, &stderr).map_err(|error| error.to_string())?;
    let mut result = Measured {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        wall_seconds: started.elapsed().as_secs_f64(),
        user_seconds: None,
        system_seconds: None,
        max_rss_bytes: None,
        peak_footprint_bytes: None,
    };
    parse_time(&stderr, &mut result);
    Ok(result)
}

/// Run one JSON-reporting stage; failures are retained as evidence.
fn stage_json(
    program: &Path,
    arguments: &[&str],
    output: &Path,
    name: &str,
) -> Result<Value, String> {
    eprintln!("perf: {name}");
    let result = measured(program, arguments, &output.join(format!("{name}.log")))?;
    std::fs::write(output.join(format!("{name}.json")), &result.stdout)
        .map_err(|error| error.to_string())?;
    let mut report = serde_json::from_str::<Value>(&result.stdout).unwrap_or_else(|_| {
        json!({"error": "stage did not produce JSON; see its log", "log": format!("{name}.log")})
    });
    report["success"] = json!(result.success);
    report["resources"] = result.resources();
    Ok(report)
}

/// Export through the public render command, timestamping each JSON event as
/// it arrives (events carry no timing) to attribute wall time to stages.
fn export(
    cli: &Path,
    package: &Path,
    destination: &Path,
    output: &Path,
    name: &str,
) -> Result<Value, String> {
    eprintln!("perf: export-{name}");
    let log = output.join(format!("export-{name}.log"));
    let stderr = std::fs::File::create(&log).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let mut child = Command::new("/usr/bin/time")
        .arg("-l")
        .arg(cli)
        .args([
            "render",
            path_str(package)?,
            "--output",
            path_str(destination)?,
            "--name",
            "export.mp4",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child.stdout.take().ok_or("render stdout")?;
    let mut stages: Vec<(String, f64)> = Vec::new();
    let mut encoding: Vec<(f64, u64)> = Vec::new();
    let mut total_frames = None;
    let mut outcome = Value::Null;
    let mut events = Vec::new();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|error| error.to_string())?;
        let at = started.elapsed().as_secs_f64();
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        events.push(json!({"at_seconds": at, "event": event}));
        let status = &event["status"];
        if let Some(stage) = status["stage"].as_str()
            && stages.last().is_none_or(|(last, _)| last != stage)
        {
            stages.push((stage.to_owned(), at));
        }
        let progress = &status["progress"];
        if progress["kind"] == "encoding" {
            if let Some(done) = progress["value"]["completed_frames"].as_u64() {
                encoding.push((at, done));
            }
            total_frames = progress["value"]["total_frames"].as_u64().or(total_frames);
        }
        if !status["outcome"].is_null() {
            outcome = status["outcome"].clone();
        }
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    let wall = started.elapsed().as_secs_f64();
    std::fs::write(
        output.join(format!("export-{name}.events.json")),
        serde_json::to_vec_pretty(&events).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut measured = Measured {
        success: status.success(),
        stdout: String::new(),
        wall_seconds: wall,
        user_seconds: None,
        system_seconds: None,
        max_rss_bytes: None,
        peak_footprint_bytes: None,
    };
    parse_time(
        &std::fs::read_to_string(&log).unwrap_or_default(),
        &mut measured,
    );
    let mut stage_seconds = serde_json::Map::new();
    for (index, (stage, at)) in stages.iter().enumerate() {
        let end = stages.get(index + 1).map_or(wall, |(_, next)| *next);
        let entry = stage_seconds.entry(stage.clone()).or_insert(json!(0.0));
        *entry = json!(entry.as_f64().unwrap_or(0.0) + end - at);
    }
    let encode_fps = match (encoding.first(), encoding.last()) {
        (Some((first_at, first)), Some((last_at, last))) if last_at > first_at => {
            Some((last - first) as f64 / (last_at - first_at))
        }
        _ => None,
    };
    let movie = destination.join("export.mp4");
    // Content duration from the committed frame rate (read-only doctor).
    let rate = Command::new(cli)
        .args(["doctor", "--project", path_str(package)?])
        .output()
        .ok()
        .and_then(|output| serde_json::from_slice::<Value>(&output.stdout).ok())
        .map(|report| report["project"]["preview"]["frame_rate"].clone());
    let content_seconds = rate.as_ref().zip(total_frames).and_then(|(rate, frames)| {
        let numerator = rate["numerator"].as_f64()?;
        let denominator = rate["denominator"].as_f64()?;
        Some(frames as f64 * denominator / numerator)
    });
    Ok(json!({
        "content_seconds": content_seconds,
        "real_time_factor": content_seconds.map(|seconds| seconds / wall),
        "success": status.success(),
        "outcome": outcome,
        "total_frames": total_frames,
        "wall_seconds": wall,
        "stage_seconds": stage_seconds,
        "encoding_frames_per_second": encode_fps,
        "movie_bytes": file_len(&movie),
        "resources": measured.resources(),
        "note": "wall includes capture, automatic encoder admission probes, encoding, independent full verification and publication",
    }))
}

/// Stress workloads: long-project playback (10,000 Original Source beats
/// and 10,000 Holds) and, per media fixture, a proxy build, jumpy adaptive
/// playback and an export running at the same time. The app itself pauses
/// proxy builds during playback and renders; this deliberately does not.
#[allow(clippy::too_many_arguments)]
fn stress(
    options: &Options,
    bin: &Path,
    cli: &Path,
    perf: &Path,
    output: &Path,
    work: &Path,
    media: &[(String, PathBuf)],
    large: &Path,
    results: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    let seconds = options.audition_seconds.to_string();
    let worker = bin.join("deadpan-media-worker");
    let (fragments, plays) = if options.quick {
        ("20", "10")
    } else {
        ("100", "100")
    };
    let holds = work.join(format!("stress-large-{LARGE_BEATS}.deadpan"));
    copy_package(large, &holds)?;
    let load = quiet(options.max_load);
    let mut report = stage_json(
        perf,
        &["playback", path_str(&holds)?, "--seconds", &seconds],
        output,
        "stress-playback-holds",
    )?;
    report["load"] = load;
    results.insert(format!("stress/playback-large-{LARGE_BEATS}"), report);
    for (name, package) in media {
        let proxies = work.join(format!("proxies-{name}"));
        let long = work.join(format!("stress-{name}-long.deadpan"));
        copy_package(package, &long)?;
        let made = stage_json(
            perf,
            &[
                "make-long",
                path_str(&long)?,
                "--fragments",
                fragments,
                "--every",
                "6",
                "--plays",
                plays,
            ],
            output,
            &format!("make-long-{name}"),
        )?;
        results.insert(format!("fixture/{name}-long"), made);
        let load = quiet(options.max_load);
        let mut report = stage_json(
            perf,
            &[
                "playback",
                path_str(&long)?,
                "--seconds",
                &seconds,
                "--pictures",
                "adaptive",
                "--proxy-cache",
                path_str(&proxies)?,
                "--worker",
                path_str(&worker)?,
            ],
            output,
            &format!("stress-playback-{name}-long"),
        )?;
        report["load"] = load;
        results.insert(format!("stress/playback-{name}-long+proxy"), report);

        // Simultaneous: a fresh proxy build, jumpy adaptive playback with the
        // existing proxy, and a public Render export.
        let building = work.join(format!("stress-{name}-build.deadpan"));
        let playing = work.join(format!("stress-{name}-play.deadpan"));
        let exporting = work.join(format!("stress-{name}-export.deadpan"));
        for copy in [&building, &playing, &exporting] {
            copy_package(package, copy)?;
        }
        stage_json(
            perf,
            &[
                "make-cuts",
                path_str(&playing)?,
                "--every",
                CUT_EVERY,
                "--plays",
                CUT_PLAYS,
                "--seconds",
                &(options.audition_seconds + 10).to_string(),
            ],
            output,
            &format!("stress-make-cuts-{name}"),
        )?;
        let fresh = work.join(format!("stress-proxies-{name}"));
        let destination = work.join(format!("stress-export-{name}"));
        std::fs::create_dir(&destination).map_err(|error| error.to_string())?;
        let load = quiet(options.max_load);
        let started = Instant::now();
        let (build, playback, export_report) = std::thread::scope(|scope| {
            let build = scope.spawn(|| {
                stage_json(
                    perf,
                    &[
                        "proxy-build",
                        path_str(&building)?,
                        "--proxy-cache",
                        path_str(&fresh)?,
                        "--worker",
                        path_str(&worker)?,
                    ],
                    output,
                    &format!("stress-concurrent-{name}-proxy-build"),
                )
            });
            let playback = scope.spawn(|| {
                stage_json(
                    perf,
                    &[
                        "playback",
                        path_str(&playing)?,
                        "--seconds",
                        &seconds,
                        "--pictures",
                        "adaptive",
                        "--proxy-cache",
                        path_str(&proxies)?,
                        "--worker",
                        path_str(&worker)?,
                    ],
                    output,
                    &format!("stress-concurrent-{name}-playback"),
                )
            });
            let exported = scope.spawn(|| {
                export(
                    cli,
                    &exporting,
                    &destination,
                    output,
                    &format!("stress-concurrent-{name}"),
                )
            });
            (
                build
                    .join()
                    .map_err(|_| "proxy build thread panicked".to_owned()),
                playback
                    .join()
                    .map_err(|_| "playback thread panicked".to_owned()),
                exported
                    .join()
                    .map_err(|_| "export thread panicked".to_owned()),
            )
        });
        let mut playback = playback??;
        playback["load"] = load;
        results.insert(
            format!("stress/concurrent-{name}"),
            json!({
                "success": playback["success"],
                "load": playback["load"],
                "wall_seconds": started.elapsed().as_secs_f64(),
                "proxy_build": build??,
                "playback": playback,
                "export": export_report??,
            }),
        );
    }
    Ok(())
}

/// Release ui-harness performance replays of the gated Section 25 scenarios.
fn ui(output: &Path, max_load: f64) -> Result<Value, String> {
    eprintln!("perf: building release ui-harness app");
    let built = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "deadpan-app",
            "--features",
            "deadpan-app/ui-harness",
            "-p",
            "deadpan-media-worker",
            "--message-format=json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("failed to start cargo: {error}"))?;
    if !built.status.success() {
        return Err(format!(
            "ui-harness release build exited with {}",
            built.status
        ));
    }
    let executables: Vec<(String, PathBuf)> = String::from_utf8_lossy(&built.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter_map(|message| {
            Some((
                message["target"]["name"].as_str()?.to_owned(),
                PathBuf::from(message["executable"].as_str()?),
            ))
        })
        .collect();
    let directory = output.join("ui-bin");
    std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let mut binaries = serde_json::Map::new();
    for name in ["deadpan-app", "deadpan-media-worker"] {
        let source = executables
            .iter()
            .find(|(built, _)| built == name)
            .map(|(_, path)| path)
            .ok_or_else(|| format!("cargo did not report {name}"))?;
        let copy = directory.join(name);
        std::fs::copy(source, &copy).map_err(|error| error.to_string())?;
        binaries.insert(
            name.to_owned(),
            json!({"source": source, "sha256": crate::replays::sha256(&copy)?}),
        );
    }
    let app = directory.join("deadpan-app");
    let mut scenarios = serde_json::Map::new();
    for scenario in UI_SCENARIOS {
        eprintln!("perf: ui {scenario}");
        let load = quiet(max_load);
        let report_dir = output.join(format!("ui-{scenario}"));
        let result = measured(
            &app,
            &[
                "--ui-check",
                "--mode",
                "performance",
                "--scenario",
                scenario,
                "--output",
                path_str(&report_dir)?,
            ],
            &output.join(format!("ui-{scenario}.log")),
        )?;
        let report = std::fs::read(report_dir.join("report.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let mut timings = serde_json::Map::new();
        let mut failed = Vec::new();
        for entry in report
            .as_ref()
            .and_then(|report| report["scenarios"].as_array())
            .into_iter()
            .flatten()
        {
            for timing in entry["timings"].as_array().into_iter().flatten() {
                let samples: Vec<f64> = timing["samples"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|sample| {
                        sample["outcome"].is_null() || sample["outcome"] == "completed"
                    })
                    .filter_map(|sample| sample["elapsed_ms"].as_f64())
                    .collect();
                if let Some(name) = timing["name"].as_str() {
                    timings.insert(name.to_owned(), distribution(&samples));
                }
            }
            for check in entry["checks"].as_array().into_iter().flatten() {
                if check["passed"] == false {
                    failed.push(check["name"].clone());
                }
            }
        }
        scenarios.insert(
            scenario.to_owned(),
            json!({"success": result.success, "failed_checks": failed,
                "timings": timings, "resources": result.resources(), "load": load}),
        );
    }
    Ok(json!({"binaries": binaries, "scenarios": scenarios}))
}

fn distribution(samples: &[f64]) -> Value {
    match crate::percentile::distribution(samples) {
        None => json!({"n": 0}),
        Some(d) => {
            json!({"n": d.n, "min": d.min, "p50": d.p50, "p95": d.p95, "max": d.max, "mean": d.mean})
        }
    }
}

/// Section 25.2 targets against this run. A row can PASS or FAIL only when
/// its stage succeeded, it has enough samples, its machine was quiet and the
/// run was not `--quick`; otherwise it is INFO with the reason. A missing
/// workload is never a pass.
fn targets(results: &BTreeMap<String, Value>, quick: bool) -> Value {
    let mut rows = Vec::new();
    let mut push = |target: &str,
                    workload: String,
                    measured: Value,
                    verdict: Option<bool>,
                    gate: Result<(), String>| {
        let gate = if quick {
            Err("--quick run".to_owned())
        } else {
            gate
        };
        let (status, reason) = match (&gate, verdict) {
            (Err(reason), _) => ("INFO", Some(reason.clone())),
            (Ok(()), None) => ("INFO", None),
            (Ok(()), Some(true)) => ("PASS", None),
            (Ok(()), Some(false)) => ("FAIL", None),
        };
        rows.push(json!({
            "target": target, "workload": workload, "measured": measured,
            "status": status, "reason": reason,
        }));
    };
    for (key, value) in results {
        let (stage, fixture) = key.split_once('/').unwrap_or((key, ""));
        let base = base_gate(value);
        match stage {
            "seek" => {
                let warm = &value["warm_seek"]["total_ms"];
                push(
                    "Warm seek p95 < 80 ms",
                    format!("{fixture}: random long-GOP seek, decode + Metal completion"),
                    warm.clone(),
                    warm["p95"].as_f64().map(|ms| ms < 80.0),
                    base.clone().and(enough(warm, MIN_SEEKS)),
                );
                let proxy = &value["proxy"];
                if proxy["status"] == "ready" {
                    let warm = &proxy["warm_seek"]["total_ms"];
                    push(
                        "Warm seek p95 < 80 ms (preview proxy)",
                        format!(
                            "{fixture}: random seek through the verified intra proxy ({}), decode + Metal completion",
                            proxy["raster"]
                        ),
                        warm.clone(),
                        warm["p95"].as_f64().map(|ms| ms < 80.0),
                        base.clone().and(enough(warm, MIN_SEEKS)),
                    );
                    push(
                        "Refined Original after rest (informational)",
                        format!(
                            "{fixture}: proxy seek, {} ms rest, then the exact Original picture from a warm session",
                            proxy["refinement"]["rest_ms"]
                        ),
                        proxy["refinement"].clone(),
                        None,
                        base.clone(),
                    );
                    push(
                        "Proxy opening (informational)",
                        format!(
                            "{fixture}: first opening of a new entry hashes it once; later openings check the recorded file state"
                        ),
                        json!({"first_open_ms": proxy["first_open_ms"], "open_ms": proxy["open_ms"]}),
                        None,
                        base.clone(),
                    );
                    push(
                        "Cold proxy seek (informational)",
                        format!("{fixture}: new proxy session to first picture, page-cache-warm"),
                        proxy["cold_seek"]["total_ms"].clone(),
                        None,
                        Err("session-cold but page-cache-warm; cache purge needs root".into()),
                    );
                }
                let progressive = &value["cold"]["progressive"];
                push(
                    "Cold seek completion < 300 ms (informational)",
                    format!(
                        "{fixture}: session-cold, page-cache-warm preview session (progressive admission) to first picture"
                    ),
                    progressive["total_ms"].clone(),
                    progressive["total_ms"]["max"].as_f64().map(|ms| ms < 300.0),
                    Err("session-cold but page-cache-warm; cache purge needs root".into()),
                );
                push(
                    "Cold complete index admission (informational)",
                    format!(
                        "{fixture}: complete fresh index measurement, before the first picture (export) and in the preview background (verified_ms)"
                    ),
                    json!({
                        "complete_total_ms": value["cold"]["complete"]["total_ms"],
                        "progressive_verified_ms": progressive["verified_ms"],
                    }),
                    None,
                    Err("session-cold but page-cache-warm; cache purge needs root".into()),
                );
                push(
                    "Frame stepping (informational)",
                    format!("{fixture}: consecutive frames"),
                    value["frame_step"].clone(),
                    None,
                    base.clone(),
                );
            }
            "edit" => {
                for (edit, limit, label) in [
                    (
                        "split",
                        50.0,
                        "Cached ordinary edit p95 < 50 ms (headless commit + refresh)",
                    ),
                    (
                        "repeat_wrap",
                        50.0,
                        "Cached ordinary edit p95 < 50 ms (headless commit + refresh)",
                    ),
                    (
                        "undo",
                        50.0,
                        "Cached ordinary edit p95 < 50 ms (headless commit + refresh)",
                    ),
                    (
                        "insert_pause",
                        100.0,
                        "Hold insertion committed < 100 ms (headless commit + refresh)",
                    ),
                ] {
                    let total = &value[edit]["total_ms"];
                    let refused = value[edit]["refused"].as_u64().unwrap_or(0);
                    push(
                        label,
                        format!("{fixture}: {edit} ({refused} refused)"),
                        total.clone(),
                        total["p95"].as_f64().map(|ms| ms < limit),
                        base.clone().and(enough(total, MIN_EDITS)),
                    );
                }
            }
            "playback" => {
                let audio = &value["audio"];
                let pictures = &value["pictures"];
                let clean = value["failure"].is_null()
                    && audio["underruns_starved"] == 0
                    && audio["device_faults"] == 0;
                let covered = if value["covered_requested_interval"] == true {
                    Ok(())
                } else {
                    Err(format!(
                        "audition did not cover the requested interval (heard {} of {} s, terminal {})",
                        value["heard_seconds"], value["coverable_seconds"], value["terminal_phase"]
                    ))
                };
                push(
                    "Audio: no callback underruns",
                    format!("{fixture}: real-device audition"),
                    json!({"failure": value["failure"], "audio": audio, "heard_seconds": value["heard_seconds"]}),
                    Some(clean),
                    base.clone().and(covered.clone()),
                );
                let complete = pictures["dropped_skipped"] == 0
                    && pictures["leading_missed"] == 0
                    && pictures["trailing_missed"]
                        .as_i64()
                        .is_some_and(|missed| missed <= 1);
                push(
                    "Playback sustained at source rate",
                    format!("{fixture}: pictures following heard clock"),
                    pictures.clone(),
                    Some(complete && clean),
                    base.clone().and(covered),
                );
                push(
                    "Playback look-ahead decoding (informational)",
                    format!("{fixture}: decoder pre-positioned at cuts"),
                    json!({
                        "lookahead": pictures["lookahead"],
                        "seek_pictures": pictures["seek_pictures"],
                        "peak_footprint_bytes": value["resources"]["peak_footprint_bytes"],
                    }),
                    None,
                    base.clone(),
                );
                if value["picture_policy"] == "adaptive" {
                    push(
                        "Playback picture tiers (informational)",
                        format!("{fixture}: exact or proxy per picture, repositions"),
                        json!({
                            "tiers": pictures["tiers"],
                            "repositions": pictures["repositions"],
                            "proxy": pictures["proxy"],
                            "exact_on_stop_ms": pictures["exact_on_stop_ms"],
                        }),
                        None,
                        base.clone(),
                    );
                }
                push(
                    "Playback start latency (informational)",
                    format!("{fixture}: play() to first device-reported content"),
                    value["playback_start_to_first_heard_ms"].clone(),
                    None,
                    base.clone(),
                );
            }
            "stress" => {
                let playback = if fixture.starts_with("concurrent-") {
                    &value["playback"]
                } else {
                    value
                };
                push(
                    "Stress playback (informational)",
                    format!("{fixture}: real-device audition, pictures following heard clock"),
                    json!({
                        "covered": playback["covered_requested_interval"],
                        "failure": playback["failure"],
                        "audio": playback["audio"],
                        "dropped": playback["pictures"]["dropped_skipped"],
                        "presented": playback["pictures"]["presented"],
                        "tiers": playback["pictures"]["tiers"],
                        "start_ms": playback["playback_start_to_first_heard_ms"],
                        "open_ms": playback["picture_session_open_ms"],
                        "peak_footprint_bytes": playback["resources"]["peak_footprint_bytes"],
                    }),
                    None,
                    base.clone(),
                );
                if fixture.starts_with("concurrent-") {
                    push(
                        "Concurrent proxy build and export (informational)",
                        format!("{fixture}: beside the playback above"),
                        json!({
                            "proxy_build": value["proxy_build"]["status"],
                            "proxy_build_ms": value["proxy_build"]["build_ms"],
                            "proxy_build_peak_footprint_bytes": value["proxy_build"]["resources"]["peak_footprint_bytes"],
                            "export_wall_seconds": value["export"]["wall_seconds"],
                            "export_success": value["export"]["success"],
                            "export_peak_footprint_bytes": value["export"]["resources"]["peak_footprint_bytes"],
                        }),
                        None,
                        base.clone(),
                    );
                }
            }
            "export" => {
                push(
                    "Export speed by workload (informational)",
                    format!("{fixture}: render+verify+publish"),
                    json!({"real_time_factor": value["real_time_factor"], "wall_seconds": value["wall_seconds"], "frames": value["total_frames"], "encoding_fps": value["encoding_frames_per_second"], "stage_seconds": value["stage_seconds"]}),
                    None,
                    base.clone(),
                );
            }
            "scale" => {
                for row in value["holds"].as_array().into_iter().flatten() {
                    push(
                        "Per-cursor-move plan lookup independent of size (informational)",
                        format!("{} Holds: picture lookup", row["beats"]),
                        row["picture_lookup_us"].clone(),
                        None,
                        base.clone(),
                    );
                }
            }
            "ui" => {
                for (label, scenario, name, limit) in [
                    (
                        "Key event to command-state p95 < 8 ms",
                        "rapid-input",
                        "warm_navigation_input_cpu_ms",
                        8.0,
                    ),
                    (
                        "Warm seek p95 < 80 ms (UI navigation to GPU completion)",
                        "rapid-input",
                        "warm_navigation_input_to_picture_complete_ms",
                        80.0,
                    ),
                    (
                        "Cached ordinary edit to visible preview p95 < 50 ms",
                        "edit-latency",
                        "cached_repeat_input_to_picture_complete_ms",
                        50.0,
                    ),
                    (
                        "Hold fallback visible < 100 ms",
                        "edit-latency",
                        "hold_fallback_input_to_picture_complete_ms",
                        100.0,
                    ),
                    (
                        "10,000-beat navigation CPU p95 < 8 ms",
                        "large-project",
                        "large_project_navigation_cpu_ms",
                        8.0,
                    ),
                ] {
                    let entry = &value["scenarios"][scenario];
                    let timing = &entry["timings"][name];
                    push(
                        label,
                        format!("ui {scenario}: {name}"),
                        timing.clone(),
                        timing["p95"].as_f64().map(|ms| ms < limit),
                        base_gate(entry).and(enough(timing, MIN_UI_SAMPLES)),
                    );
                }
            }
            _ => {}
        }
    }
    json!(rows)
}

/// The stage process must have succeeded on a quiet machine.
fn base_gate(value: &Value) -> Result<(), String> {
    if value["success"] != true {
        return Err("stage or scenario did not succeed".into());
    }
    if value["load"]["flagged"] == true {
        return Err(format!(
            "machine load {} exceeded {}",
            value["load"]["load_1m_at_start"], value["load"]["max_load"]
        ));
    }
    Ok(())
}

fn enough(distribution: &Value, minimum: u64) -> Result<(), String> {
    let n = distribution["n"].as_u64().unwrap_or(0);
    if n >= minimum {
        Ok(())
    } else {
        Err(format!("{n} samples, fewer than {minimum}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_report_fields_parse() {
        let mut measured = Measured {
            success: true,
            stdout: String::new(),
            wall_seconds: 0.0,
            user_seconds: None,
            system_seconds: None,
            max_rss_bytes: None,
            peak_footprint_bytes: None,
        };
        parse_time(
            "noise\n      286.67 real       277.34 user         8.49 sys\n           186220544  maximum resident set size\n            13468080  peak memory footprint\n",
            &mut measured,
        );
        assert_eq!(measured.wall_seconds, 286.67);
        assert_eq!(measured.user_seconds, Some(277.34));
        assert_eq!(measured.max_rss_bytes, Some(186_220_544));
        assert_eq!(measured.peak_footprint_bytes, Some(13_468_080));
    }

    #[test]
    fn options_reject_unknown_stages_and_bad_names() {
        let strings = |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>();
        assert!(parse(&strings(&["--stages", "seek,bogus"])).is_err());
        assert!(parse(&strings(&["--fixture", "a b=/x"])).is_err());
        let options = parse(&strings(&["--fixture", "cam=/p", "--generate", "--quick"])).unwrap();
        assert_eq!(
            options.fixtures,
            vec![("cam".to_owned(), PathBuf::from("/p"))]
        );
        assert!(options.generate && options.quick && !options.ui);
    }

    #[test]
    fn missing_workloads_are_never_passes() {
        let mut results = BTreeMap::new();
        results.insert("seek/x".to_owned(), json!({}));
        let rows = targets(&results, false);
        assert!(
            rows.as_array()
                .unwrap()
                .iter()
                .all(|row| row["status"] != "PASS")
        );
    }

    fn status(results: &BTreeMap<String, Value>, quick: bool, target: &str) -> Vec<String> {
        targets(results, quick)
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                row["target"]
                    .as_str()
                    .is_some_and(|t| t.starts_with(target))
            })
            .map(|row| row["status"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn verdicts_need_success_samples_quiet_and_full_runs() {
        let seek = |n: u64, success: bool, flagged: bool| {
            json!({"success": success, "load": {"flagged": flagged},
                "warm_seek": {"total_ms": {"n": n, "p95": 10.0}},
                "cold": {"progressive": {"total_ms": {"n": 10, "max": 10.0}}}})
        };
        let one = |value: Value| BTreeMap::from([("seek/x".to_owned(), value)]);
        assert_eq!(
            status(&one(seek(200, true, false)), false, "Warm seek"),
            ["PASS"]
        );
        assert_eq!(
            status(&one(seek(200, true, false)), true, "Warm seek"),
            ["INFO"]
        );
        assert_eq!(
            status(&one(seek(4, true, false)), false, "Warm seek"),
            ["INFO"]
        );
        assert_eq!(
            status(&one(seek(200, false, false)), false, "Warm seek"),
            ["INFO"]
        );
        assert_eq!(
            status(&one(seek(200, true, true)), false, "Warm seek"),
            ["INFO"]
        );
        // A page-cache-warm cold sample never passes.
        assert_eq!(
            status(&one(seek(200, true, false)), false, "Cold seek"),
            ["INFO"]
        );
    }

    #[test]
    fn playback_must_cover_its_interval_and_every_frame() {
        let playback = |covered: bool, leading: i64| {
            BTreeMap::from([(
                "playback/x".to_owned(),
                json!({
                    "success": true, "failure": null, "covered_requested_interval": covered,
                    "audio": {"underruns_starved": 0, "device_faults": 0},
                    "pictures": {"dropped_skipped": 0, "leading_missed": leading, "trailing_missed": 0},
                }),
            )])
        };
        assert_eq!(
            status(&playback(true, 0), false, "Playback sustained"),
            ["PASS"]
        );
        assert_eq!(
            status(&playback(true, 3), false, "Playback sustained"),
            ["FAIL"]
        );
        assert_eq!(
            status(&playback(false, 0), false, "Playback sustained"),
            ["INFO"]
        );
        assert_eq!(status(&playback(false, 0), false, "Audio"), ["INFO"]);
    }
}
