//! `cargo xtask sanitizers`: AddressSanitizer/UndefinedBehaviorSanitizer and
//! ThreadSanitizer runs of the native adapters that the media qualification
//! (`tools/media-qualification/host/build_sanitized.py`) and `chaos
//! --sanitize` do not cover (docs/ADVERSARIAL.md#native-adapter-sanitizers).
//!
//! Unlike those runs, the Rust code itself is instrumented: the nightly
//! toolchain builds the adapters, their dependencies and the standard library
//! (`-Zbuild-std`) with `-Zsanitizer=address` or `-Zsanitizer=thread`. Native
//! C/C++/Objective-C dependencies compiled by build scripts for the target
//! (bundled SQLite, BLAKE3, whisper.cpp/ggml including its Metal host code, the
//! deadpan-source adapters) are instrumented by Apple clang through a compiler
//! wrapper: address runs add `-fsanitize=address,undefined`, thread runs
//! `-fsanitize=thread`. Host build scripts and proc-macros stay uninstrumented.
//! Every instrumented object links the nightly Rust compiler-rt runtime, so the
//! Apple clang ASan version guard is disabled; positive controls compiled with
//! the same flags prove that this mixed runtime reports real defects before any
//! adapter result is trusted. System frameworks (Vision, CoreVideo, CoreAudio,
//! AppKit, Metal) and the separately built FFmpeg libraries are not
//! instrumented.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const USAGE: &str = "usage: cargo xtask sanitizers [--only name,..] [--sanitizer address|thread] [--runs N] [--target-root DIR] [--output NEW_DIR] [--whisper-model PATH] [--no-device] [--list]";

const TARGET: &str = "aarch64-apple-darwin";
/// One test binary, device probe or harness step; generous for whisper.cpp
/// under ASan, then the process group is killed and the step fails.
const STEP_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Sanitizer {
    Address,
    Thread,
}

impl Sanitizer {
    fn name(self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Thread => "thread",
        }
    }

    /// Instrumentation for target C, C++ and Objective-C objects.
    fn c_flags(self) -> &'static [&'static str] {
        match self {
            // The Rust runtime, not Apple's, provides the ASan/UBSan symbols.
            Self::Address => &[
                "-fsanitize=address,undefined",
                "-fno-omit-frame-pointer",
                "-fno-sanitize-recover=all",
                "-mllvm",
                "-asan-guard-against-version-mismatch=0",
            ],
            Self::Thread => &["-fsanitize=thread", "-fno-omit-frame-pointer"],
        }
    }

    fn runtime_environment(self) -> Vec<(&'static str, String)> {
        let environment: Vec<(&'static str, &'static str)> = match self {
            // LeakSanitizer is not used: Darwin's runtime does not support it
            // reliably on arm64, and leaks are not memory-safety defects here.
            Self::Address => vec![
                (
                    "ASAN_OPTIONS",
                    "detect_leaks=0:abort_on_error=1:detect_stack_use_after_return=1:strict_string_checks=1:check_initialization_order=1:strict_init_order=1",
                ),
                ("UBSAN_OPTIONS", "print_stacktrace=1:halt_on_error=1"),
            ],
            Self::Thread => vec![(
                "TSAN_OPTIONS",
                "halt_on_error=1:second_deadlock_stack=1:report_signal_unsafe=1",
            )],
        };
        environment
            .into_iter()
            .map(|(key, value)| {
                let mut value = value.to_owned();
                if self == Self::Thread {
                    // Reviewed Darwin false positives; the path is absolute
                    // because tests run in their package directories.
                    let suppressions = std::env::current_dir()
                        .unwrap_or_default()
                        .join(TSAN_SUPPRESSIONS);
                    value.push_str(&format!(":suppressions={}", suppressions.display()));
                }
                (key, value)
            })
            .collect()
    }

    /// Checks disabled for one vendored third-party source tree each, matched
    /// against compiler arguments: (shell pattern, flag, recorded reason).
    /// Each is a reported upstream finding, never an adapter defect.
    fn third_party_exclusions(self) -> &'static [(&'static str, &'static str, &'static str)] {
        match self {
            Self::Address => &[(
                "*/whisper.cpp/ggml/*",
                "-fno-sanitize=pointer-overflow",
                "ggml sizes graph layouts by offsetting a null pointer (ggml_graph_nbytes/incr_ptr_aligned); reported as applying a non-zero offset to a null pointer, no memory is accessed",
            )],
            Self::Thread => &[],
        }
    }

    /// Imported runtime entry point proving a binary is instrumented.
    fn init_symbol(self) -> &'static str {
        match self {
            Self::Address => "___asan_init",
            Self::Thread => "___tsan_init",
        }
    }
}

/// Text that marks a sanitizer report in any captured output, including a
/// worker's stderr relayed by its host.
const REPORT_MARKERS: &[&str] = &[
    "ERROR: AddressSanitizer",
    "WARNING: ThreadSanitizer",
    "runtime error:",
    "SUMMARY: UndefinedBehaviorSanitizer",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Extra {
    None,
    /// `qualify_output` on the default device with digital silence.
    DeviceProbe,
    /// The sanitized CLI transcribes a speech fixture with the sanitized
    /// whisper.cpp worker beside it.
    Transcription,
}

struct Entry {
    name: &'static str,
    package: &'static str,
    /// Directory below the repository root.
    directory: &'static str,
    sanitizers: &'static [Sanitizer],
    extra: Extra,
}

const ENTRIES: &[Entry] = &[
    Entry {
        name: "fileclone",
        package: "deadpan-fileclone",
        directory: "native/deadpan-fileclone",
        sanitizers: &[Sanitizer::Address],
        extra: Extra::None,
    },
    Entry {
        name: "filesystem",
        package: "deadpan-filesystem",
        directory: "native/deadpan-filesystem",
        sanitizers: &[Sanitizer::Address],
        extra: Extra::None,
    },
    Entry {
        name: "sqlite-status",
        package: "deadpan-sqlite-status",
        directory: "native/deadpan-sqlite-status",
        sanitizers: &[Sanitizer::Address],
        extra: Extra::None,
    },
    Entry {
        name: "process",
        package: "deadpan-native-process",
        directory: "native/deadpan-process",
        sanitizers: &[Sanitizer::Address, Sanitizer::Thread],
        extra: Extra::None,
    },
    Entry {
        name: "output",
        package: "deadpan-output",
        directory: "native/deadpan-output",
        sanitizers: &[Sanitizer::Address, Sanitizer::Thread],
        extra: Extra::DeviceProbe,
    },
    Entry {
        name: "track",
        package: "deadpan-track",
        directory: "native/deadpan-track",
        sanitizers: &[Sanitizer::Address],
        extra: Extra::None,
    },
    Entry {
        name: "transcribe",
        package: "deadpan-transcribe",
        directory: "native/deadpan-transcribe",
        sanitizers: &[Sanitizer::Address, Sanitizer::Thread],
        extra: Extra::Transcription,
    },
];

const TSAN_SUPPRESSIONS: &str = "tools/sanitizer-qualification/tsan.supp";
const SPEECH_FIXTURE: &str = "tools/sanitizer-qualification/fixtures/speech.mp4";

struct Options {
    only: Option<Vec<String>>,
    sanitizer: Option<Sanitizer>,
    runs: u32,
    target_root: PathBuf,
    output: Option<PathBuf>,
    whisper_model: Option<PathBuf>,
    device: bool,
    list: bool,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        only: None,
        sanitizer: None,
        runs: 3,
        target_root: PathBuf::from("target/sanitizers"),
        output: None,
        whisper_model: None,
        device: true,
        list: false,
    };
    let mut arguments = arguments.iter();
    while let Some(flag) = arguments.next() {
        match flag.as_str() {
            "--list" => options.list = true,
            "--no-device" => options.device = false,
            "--only" | "--sanitizer" | "--runs" | "--target-root" | "--output"
            | "--whisper-model" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| format!("missing value for {flag}; {USAGE}"))?;
                match flag.as_str() {
                    "--only" => {
                        let names: Vec<String> =
                            value.split(',').map(str::trim).map(str::to_owned).collect();
                        if let Some(unknown) = names
                            .iter()
                            .find(|name| ENTRIES.iter().all(|entry| entry.name != *name))
                        {
                            return Err(format!("unknown entry {unknown}; see --list"));
                        }
                        options.only = Some(names);
                    }
                    "--sanitizer" => {
                        options.sanitizer = Some(match value.as_str() {
                            "address" => Sanitizer::Address,
                            "thread" => Sanitizer::Thread,
                            _ => return Err(format!("--sanitizer is address or thread; {USAGE}")),
                        });
                    }
                    "--runs" => {
                        options.runs = value
                            .parse()
                            .ok()
                            .filter(|runs| (1..=100).contains(runs))
                            .ok_or("--runs must be between 1 and 100")?;
                    }
                    "--target-root" => options.target_root = PathBuf::from(value),
                    "--output" => options.output = Some(PathBuf::from(value)),
                    _ => options.whisper_model = Some(PathBuf::from(value)),
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
            let sanitizers: Vec<&str> = entry.sanitizers.iter().map(|s| s.name()).collect();
            println!(
                "{:<14} {:<24} {}",
                entry.name,
                entry.package,
                sanitizers.join(",")
            );
        }
        return Ok(());
    }
    if std::env::consts::OS != "macos" || std::env::consts::ARCH != "aarch64" {
        return Err("the sanitizer runs target Apple Silicon macOS".into());
    }
    let repository = std::env::current_dir().map_err(|error| error.to_string())?;
    let selected: Vec<&Entry> = ENTRIES
        .iter()
        .filter(|entry| {
            options
                .only
                .as_ref()
                .is_none_or(|only| only.iter().any(|name| name == entry.name))
        })
        .collect();
    let mut sanitizers: Vec<Sanitizer> = selected
        .iter()
        .flat_map(|entry| entry.sanitizers.iter().copied())
        .filter(|sanitizer| options.sanitizer.is_none_or(|only| only == *sanitizer))
        .collect();
    sanitizers.sort_unstable();
    sanitizers.dedup();
    if sanitizers.is_empty() {
        return Err(format!(
            "no entry/sanitizer pair matches; see --list. {USAGE}"
        ));
    }
    let needs_ffmpeg = selected
        .iter()
        .any(|entry| matches!(entry.name, "track" | "transcribe"));
    if needs_ffmpeg
        && std::env::var_os("DEADPAN_FFMPEG_PREFIX").is_none_or(|prefix| prefix.is_empty())
    {
        return Err("export DEADPAN_FFMPEG_PREFIX first (docs/DEVELOPMENT.md)".into());
    }
    let whisper = if selected
        .iter()
        .any(|entry| entry.extra == Extra::Transcription)
    {
        Some(whisper_model(options.whisper_model.as_deref())?)
    } else {
        None
    };

    let output = options.output.clone().unwrap_or_else(|| {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_secs());
        options.target_root.join("runs").join(stamp.to_string())
    });
    if output.exists() {
        return Err(format!(
            "{} already exists; choose a new --output",
            output.display()
        ));
    }
    std::fs::create_dir_all(&output)
        .map_err(|error| format!("create {}: {error}", output.display()))?;
    let output = output.canonicalize().map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&options.target_root).map_err(|error| error.to_string())?;
    let target_root = options
        .target_root
        .canonicalize()
        .map_err(|error| error.to_string())?;

    let started = Instant::now();
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for sanitizer in sanitizers {
        let entries: Vec<&Entry> = selected
            .iter()
            .copied()
            .filter(|entry| entry.sanitizers.contains(&sanitizer))
            .collect();
        let directory = output.join(sanitizer.name());
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let (target_dir, wrapper) = compiler_wrappers(&target_root, sanitizer)?;
        eprintln!("sanitizers: {} positive controls", sanitizer.name());
        let controls = positive_controls(sanitizer, &target_dir, &wrapper, &directory)?;
        if let Some(control) = controls
            .iter()
            .find(|control| control["passed"] != json!(true))
        {
            failures.push(format!(
                "{} control {} did not behave as expected",
                sanitizer.name(),
                control["case"]
            ));
            results.push(json!({"sanitizer": sanitizer.name(), "controls": controls}));
            continue;
        }
        eprintln!(
            "sanitizers: building {} test binaries in {}",
            sanitizer.name(),
            target_dir.display()
        );
        let mut cargo = cargo_command(sanitizer, &wrapper);
        cargo.args(["test", "--no-run", "--tests"]);
        for entry in &entries {
            cargo.args(["-p", entry.package]);
        }
        finish_cargo(&mut cargo, &target_dir);
        let artifacts = cargo_artifacts(cargo, &directory.join("build-tests.log"))?;
        let mut entry_results = Vec::new();
        for entry in &entries {
            let package_directory = repository.join(entry.directory);
            let binaries: Vec<&PathBuf> = artifacts
                .iter()
                .filter(|(manifest, kind, _)| {
                    manifest == &package_directory.join("Cargo.toml") && kind == "test"
                })
                .map(|(_, _, executable)| executable)
                .collect();
            if binaries.is_empty() {
                failures.push(format!("{}: no test binaries were built", entry.name));
                continue;
            }
            let mut tests = Vec::new();
            let mut totals = Counts::default();
            for binary in binaries {
                let instrumented = imports_symbol(binary, sanitizer.init_symbol())?;
                let stem = binary
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("test");
                let mut runs = Vec::new();
                for run in 1..=options.runs {
                    let log = directory.join(format!("{}-{stem}-{run}.log", entry.name));
                    let mut command = Command::new(binary);
                    command
                        .current_dir(&package_directory)
                        .envs(sanitizer.runtime_environment());
                    let step = run_step(command, &log)?;
                    let counts = Counts::parse(&step.text);
                    totals.add(counts);
                    if !step.passed() || counts.failed > 0 {
                        failures.push(format!(
                            "{} {} {stem} run {run}: {}",
                            sanitizer.name(),
                            entry.name,
                            step.describe()
                        ));
                    }
                    runs.push(step.json(counts));
                }
                if !instrumented {
                    failures.push(format!(
                        "{} {stem} does not import {}",
                        entry.name,
                        sanitizer.init_symbol()
                    ));
                }
                tests.push(json!({
                    "binary": binary.display().to_string(),
                    "sha256": crate::replays::sha256(binary)?,
                    "instrumented": instrumented,
                    "ubsan_handler_imports": ubsan_handlers(binary)?,
                    "runs": runs,
                }));
            }
            let extra = match entry.extra {
                Extra::None => Value::Null,
                Extra::DeviceProbe if !options.device => json!({"skipped": "--no-device"}),
                Extra::DeviceProbe => device_probe(
                    sanitizer,
                    &target_dir,
                    &wrapper,
                    &directory,
                    &package_directory,
                    options.runs,
                    &mut failures,
                )?,
                Extra::Transcription => transcription(
                    sanitizer,
                    &target_dir,
                    &wrapper,
                    &directory,
                    &repository,
                    whisper.as_ref().ok_or("no whisper model")?,
                    options.runs,
                    &mut failures,
                )?,
            };
            eprintln!(
                "sanitizers: {:<8} {:<14} {:>5} passed {:>3} failed {:>3} ignored over {} run(s)",
                sanitizer.name(),
                entry.name,
                totals.passed,
                totals.failed,
                totals.ignored,
                options.runs
            );
            // Worker executables the integration tests launch
            // (CARGO_BIN_EXE_*) come from the same instrumented build.
            let mut workers = Vec::new();
            for (manifest, kind, executable) in &artifacts {
                if manifest == &package_directory.join("Cargo.toml") && kind == "bin" {
                    let instrumented = imports_symbol(executable, sanitizer.init_symbol())?;
                    if !instrumented {
                        failures.push(format!(
                            "{} worker {} is not instrumented",
                            entry.name,
                            executable.display()
                        ));
                    }
                    workers.push(json!({
                        "binary": executable.display().to_string(),
                        "sha256": crate::replays::sha256(executable)?,
                        "instrumented": instrumented,
                        "ubsan_handler_imports": ubsan_handlers(executable)?,
                    }));
                }
            }
            entry_results.push(json!({
                "entry": entry.name,
                "package": entry.package,
                "totals": totals.json(),
                "tests": tests,
                "workers": workers,
                "extra": extra,
            }));
        }
        results.push(json!({
            "sanitizer": sanitizer.name(),
            "target_dir": target_dir.display().to_string(),
            "rustflags": rust_flags(sanitizer),
            "c_flags": sanitizer.c_flags(),
            "third_party_exclusions": sanitizer.third_party_exclusions().iter().map(|(pattern, flag, reason)| json!({"pattern": pattern, "flag": flag, "reason": reason})).collect::<Vec<_>>(),
            "runtime_environment": sanitizer.runtime_environment().into_iter().collect::<BTreeMap<_, _>>(),
            "controls": controls,
            "entries": entry_results,
        }));
    }

    let summary = json!({
        "revision": capture("git", &["rev-parse", "HEAD"]),
        "dirty_worktree": capture("git", &["status", "--porcelain"]).is_some_and(|status| !status.is_empty()),
        "toolchain": {
            "rustc": capture("rustc", &["+nightly", "-vV"]),
            "cargo": capture("cargo", &["+nightly", "-V"]),
            "clang": capture("clang", &["--version"]),
            "sdk": capture("xcrun", &["--show-sdk-version"]),
            "macos": capture("sw_vers", &["-productVersion"]),
        },
        "runs_per_binary": options.runs,
        "elapsed_s": started.elapsed().as_secs_f64(),
        "failures": failures,
        "results": results,
    });
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    eprintln!(
        "sanitizers: summary in {}",
        output.join("summary.json").display()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} failure(s):\n  {}",
            failures.len(),
            failures.join("\n  ")
        ))
    }
}

fn rust_flags(sanitizer: Sanitizer) -> String {
    format!("-Zsanitizer={}", sanitizer.name())
}

/// `cargo +nightly` with instrumented target Rust, std and native objects.
fn cargo_command(sanitizer: Sanitizer, wrappers: &Path) -> Command {
    let mut command = Command::new("cargo");
    command
        .args(["+nightly"])
        .env_remove("RUSTFLAGS")
        .env_remove("CFLAGS")
        .env_remove("CXXFLAGS")
        .env("CARGO_ENCODED_RUSTFLAGS", rust_flags(sanitizer))
        .env("CC_aarch64_apple_darwin", wrappers.join("clang"))
        .env("CXX_aarch64_apple_darwin", wrappers.join("clang++"))
        // whisper-rs-sys compiles through ccache when present; compile fresh.
        .env("CCACHE_DISABLE", "1");
    command
}

/// Appends the shared build arguments after the subcommand and options.
fn finish_cargo(command: &mut Command, target_dir: &Path) {
    command
        .args([
            "-Zbuild-std",
            "--locked",
            "--message-format=json-render-diagnostics",
            "--target",
            TARGET,
            "--target-dir",
        ])
        .arg(target_dir);
}

/// Compiler wrappers that instrument only target objects. Build scripts run
/// with `OUT_DIR` below `<target-dir>/<triple>/` for target units and below
/// `<target-dir>/debug/` for host units; host objects (for example BLAKE3 in
/// a build script's dependency) must stay uninstrumented because host Rust is
/// not linked with a sanitizer runtime. The wrappers live in a directory named
/// by their content hash inside a target directory of the same name, so
/// changing flags rebuilds every native dependency.
fn compiler_wrappers(
    target_root: &Path,
    sanitizer: Sanitizer,
) -> Result<(PathBuf, PathBuf), String> {
    use sha2::{Digest, Sha256};
    let flags = sanitizer.c_flags().join(" ");
    let exclusions: String = sanitizer
        .third_party_exclusions()
        .iter()
        .map(|(pattern, flag, _)| format!("    {pattern}) extra=\"$extra {flag}\" ;;\n"))
        .collect();
    let scripts: Vec<(&str, String)> = ["clang", "clang++"]
        .into_iter()
        .map(|compiler| {
            (
                compiler,
                format!(
                    "#!/bin/sh\n# Generated by cargo xtask sanitizers.\ncase \"${{OUT_DIR:-}}\" in\n  */{TARGET}/*) ;;\n  *) exec /usr/bin/{compiler} \"$@\" ;;\nesac\nextra=\nfor argument in \"$@\"; do\n  case \"$argument\" in\n{exclusions}  esac\ndone\nexec /usr/bin/{compiler} {flags} $extra \"$@\"\n"
                ),
            )
        })
        .collect();
    let mut hasher = Sha256::new();
    for (_, script) in &scripts {
        hasher.update(script.as_bytes());
    }
    let digest: String = hasher.finalize()[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    // Not every native build script reruns when `CC` changes (whisper-rs-sys
    // does not), so each wrapper version gets its own target directory.
    let target_dir = target_root.join(format!("{}-{digest}", sanitizer.name()));
    let directory = target_dir.join("compiler-wrappers");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    for (compiler, script) in scripts {
        let path = directory.join(compiler);
        std::fs::write(&path, script).map_err(|error| error.to_string())?;
        let status = Command::new("chmod")
            .arg("755")
            .arg(&path)
            .status()
            .map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("chmod {}", path.display()));
        }
    }
    Ok((target_dir, directory))
}

/// Runs a cargo build/test `--no-run` and returns (manifest, kind, executable)
/// for every produced executable.
fn cargo_artifacts(
    mut command: Command,
    log: &Path,
) -> Result<Vec<(PathBuf, String, PathBuf)>, String> {
    let log_file = std::fs::File::create(log).map_err(|error| error.to_string())?;
    let output = command
        .stderr(Stdio::from(log_file))
        .output()
        .map_err(|error| format!("start cargo: {error}"))?;
    if !output.status.success() {
        let tail = std::fs::read_to_string(log).unwrap_or_default();
        let tail: String = tail
            .chars()
            .rev()
            .take(4000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(format!("cargo failed; see {}\n{tail}", log.display()));
    }
    let mut artifacts = Vec::new();
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
        let kind = if message["profile"]["test"] == json!(true) {
            "test"
        } else {
            message["target"]["kind"][0].as_str().unwrap_or_default()
        };
        artifacts.push((
            PathBuf::from(manifest),
            kind.to_owned(),
            PathBuf::from(executable),
        ));
    }
    Ok(artifacts)
}

fn imports_symbol(binary: &Path, symbol: &str) -> Result<bool, String> {
    Ok(imports(binary)?.iter().any(|line| line == symbol))
}

/// Undefined (imported) symbols of a binary.
fn imports(binary: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("nm")
        .args(["-u"])
        .arg(binary)
        .output()
        .map_err(|error| format!("nm {}: {error}", binary.display()))?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_owned())
        .collect())
}

/// Only clang-instrumented C/C++/Objective-C imports UBSan handlers (Rust has
/// no UBSan), so their presence shows that native objects are instrumented.
fn ubsan_handlers(binary: &Path) -> Result<usize, String> {
    Ok(imports(binary)?
        .iter()
        .filter(|symbol| symbol.starts_with("___ubsan_handle_"))
        .count())
}

#[derive(Clone, Copy, Default)]
struct Counts {
    passed: u64,
    failed: u64,
    ignored: u64,
}

impl Counts {
    /// Sums libtest `test result:` lines.
    fn parse(text: &str) -> Self {
        let mut counts = Self::default();
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("test result: ") else {
                continue;
            };
            for part in rest.split(['.', ';']) {
                let mut words = part.split_whitespace();
                let (Some(number), Some(label)) = (words.next(), words.next()) else {
                    continue;
                };
                let Ok(number) = number.parse::<u64>() else {
                    continue;
                };
                match label {
                    "passed" => counts.passed += number,
                    "failed" => counts.failed += number,
                    "ignored" => counts.ignored += number,
                    _ => {}
                }
            }
        }
        counts
    }

    fn add(&mut self, other: Self) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.ignored += other.ignored;
    }

    fn json(self) -> Value {
        json!({"passed": self.passed, "failed": self.failed, "ignored": self.ignored})
    }
}

struct Step {
    exit: Option<i32>,
    timed_out: bool,
    seconds: f64,
    text: String,
    log: PathBuf,
}

impl Step {
    fn report(&self) -> Option<&'static str> {
        REPORT_MARKERS
            .iter()
            .copied()
            .find(|marker| self.text.contains(marker))
    }

    fn passed(&self) -> bool {
        self.exit == Some(0) && !self.timed_out && self.report().is_none()
    }

    fn describe(&self) -> String {
        if self.timed_out {
            format!("timed out; see {}", self.log.display())
        } else if let Some(marker) = self.report() {
            format!("sanitizer report ({marker}); see {}", self.log.display())
        } else {
            format!("exit {:?}; see {}", self.exit, self.log.display())
        }
    }

    fn json(&self, counts: Counts) -> Value {
        json!({
            "exit": self.exit,
            "timed_out": self.timed_out,
            "sanitizer_report": self.report(),
            // TSan prints this at exit when a reviewed suppression matched.
            "suppressions_matched": self.text.contains("ThreadSanitizer: Matched"),
            "seconds": self.seconds,
            "counts": counts.json(),
            "log": self.log.display().to_string(),
        })
    }
}

/// Runs one step in its own process group with combined output in `log`.
fn run_step(command: Command, log: &Path) -> Result<Step, String> {
    run_step_with_stdout(command, log, None)
}

/// [`run_step`], optionally keeping stdout (a JSON report) in its own file;
/// `text` then holds stderr followed by stdout.
fn run_step_with_stdout(
    mut command: Command,
    log: &Path,
    stdout: Option<&Path>,
) -> Result<Step, String> {
    use std::os::unix::process::CommandExt;
    let file = std::fs::File::create(log).map_err(|error| error.to_string())?;
    let out = match stdout {
        Some(path) => std::fs::File::create(path).map_err(|error| error.to_string())?,
        None => file.try_clone().map_err(|error| error.to_string())?,
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(file))
        .process_group(0);
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| format!("start {command:?}: {error}"))?;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > STEP_TIMEOUT {
            timed_out = true;
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{}", child.id())])
                .status();
            break child.wait().map_err(|error| error.to_string())?;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut bytes = std::fs::read(log).map_err(|error| error.to_string())?;
    if let Some(path) = stdout {
        bytes.extend(std::fs::read(path).map_err(|error| error.to_string())?);
    }
    Ok(Step {
        exit: status.code(),
        timed_out,
        seconds: started.elapsed().as_secs_f64(),
        text: String::from_utf8_lossy(&bytes).into_owned(),
        log: log.to_path_buf(),
    })
}

const CONTROL_RUST: &str = r#"// Positive controls for cargo xtask sanitizers (generated).
use std::hint::black_box;

unsafe extern "C" {
    fn deadpan_control_c_heap_overflow(count: i32) -> i32;
    fn deadpan_control_c_signed_overflow(value: i32) -> i32;
    fn deadpan_control_c_race() -> i32;
}

fn main() {
    let case = std::env::args().nth(1).unwrap_or_default();
    let value: i64 = match case.as_str() {
        "clean" => 0,
        "rust-heap-overflow" => {
            let bytes = vec![1u8; 8];
            let pointer = black_box(bytes.as_ptr());
            // SAFETY: deliberately out of bounds; the sanitizer must report it.
            i64::from(unsafe { *pointer.add(black_box(8)) })
        }
        "rust-use-after-free" => {
            let pointer = black_box(Box::into_raw(Box::new(7u64)));
            // SAFETY: deliberately used after free; the sanitizer must report it.
            unsafe {
                drop(Box::from_raw(pointer));
                *pointer as i64
            }
        }
        "rust-race" => {
            let mut shared = Box::new(0u64);
            let address = &raw mut *shared as usize;
            let writer = std::thread::spawn(move || {
                // SAFETY: deliberately racy; the sanitizer must report it.
                unsafe { *(address as *mut u64) += 1 }
            });
            // SAFETY: deliberately racy; the sanitizer must report it.
            unsafe { *(address as *mut u64) += 1 };
            writer.join().unwrap();
            *shared as i64
        }
        // SAFETY (all three): deliberate C defects the sanitizer must report.
        "c-heap-overflow" => i64::from(unsafe { deadpan_control_c_heap_overflow(black_box(8)) }),
        "c-signed-overflow" => {
            i64::from(unsafe { deadpan_control_c_signed_overflow(black_box(i32::MAX)) })
        }
        "c-race" => i64::from(unsafe { deadpan_control_c_race() }),
        _ => panic!("unknown control {case}"),
    };
    println!("{value}");
}
"#;

const CONTROL_C: &str = r#"/* Positive controls for cargo xtask sanitizers (generated). */
#include <pthread.h>
#include <stdlib.h>

int deadpan_control_c_heap_overflow(int count) {
    volatile int *values = malloc((size_t)count * sizeof(int));
    for (int index = 0; index < count; index++) values[index] = index;
    int value = values[count];
    free((void *)values);
    return value;
}

int deadpan_control_c_signed_overflow(int value) { return value + 1; }

static int shared;
static void *writer(void *argument) { (void)argument; shared++; return 0; }

int deadpan_control_c_race(void) {
    pthread_t thread;
    if (pthread_create(&thread, 0, writer, 0) != 0) return -1;
    shared++;
    pthread_join(thread, 0);
    return shared;
}
"#;

/// Compiles Rust and C defects with the exact instrumentation used for the
/// adapters, linked by rustc against the nightly runtime, and requires each
/// to be reported (and a clean run not to be).
fn positive_controls(
    sanitizer: Sanitizer,
    target_dir: &Path,
    wrappers: &Path,
    output: &Path,
) -> Result<Vec<Value>, String> {
    // A dependency-free package built like the adapters (same flags, -Zbuild-std
    // and target directory), so std and the runtime match exactly.
    let directory = target_dir.join("controls");
    std::fs::create_dir_all(directory.join("src")).map_err(|error| error.to_string())?;
    let c = directory.join("control.c");
    let object = directory.join("control_c.o");
    std::fs::write(directory.join("src/main.rs"), CONTROL_RUST)
        .map_err(|error| error.to_string())?;
    std::fs::write(&c, CONTROL_C).map_err(|error| error.to_string())?;
    std::fs::write(
        directory.join("Cargo.toml"),
        "[package]\nname = \"deadpan-sanitizer-control\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        directory.join("build.rs"),
        format!(
            "fn main() {{\n    println!(\"cargo:rerun-if-changed={object}\");\n    println!(\"cargo:rustc-link-arg={object}\");\n}}\n",
            object = object.display()
        ),
    )
    .map_err(|error| error.to_string())?;
    let compiled = Command::new("/usr/bin/clang")
        .args(sanitizer.c_flags())
        .args(["-g", "-O1", "-c"])
        .arg(&c)
        .arg("-o")
        .arg(&object)
        .status()
        .map_err(|error| error.to_string())?;
    if !compiled.success() {
        return Err("compile C positive control".into());
    }
    let mut cargo = cargo_command(sanitizer, wrappers);
    cargo
        .arg("build")
        .arg("--manifest-path")
        .arg(directory.join("Cargo.toml"));
    cargo.args([
        "-Zbuild-std",
        "--message-format=json-render-diagnostics",
        "--target",
        TARGET,
        "--target-dir",
    ]);
    cargo.arg(target_dir);
    let binary = cargo_artifacts(cargo, &output.join("build-controls.log"))?
        .into_iter()
        .find(|(_, kind, _)| kind == "bin")
        .map(|(_, _, executable)| executable)
        .ok_or("the positive control was not built")?;
    let cases: &[(&str, Option<&str>)] = match sanitizer {
        Sanitizer::Address => &[
            ("clean", None),
            (
                "rust-heap-overflow",
                Some("ERROR: AddressSanitizer: heap-buffer-overflow"),
            ),
            (
                "rust-use-after-free",
                Some("ERROR: AddressSanitizer: heap-use-after-free"),
            ),
            (
                "c-heap-overflow",
                Some("ERROR: AddressSanitizer: heap-buffer-overflow"),
            ),
            (
                "c-signed-overflow",
                Some("runtime error: signed integer overflow"),
            ),
        ],
        Sanitizer::Thread => &[
            ("clean", None),
            ("rust-race", Some("WARNING: ThreadSanitizer: data race")),
            ("c-race", Some("WARNING: ThreadSanitizer: data race")),
        ],
    };
    let mut results = Vec::new();
    for (case, expected) in cases {
        let mut command = Command::new(&binary);
        command.arg(case).envs(sanitizer.runtime_environment());
        let step = run_step(command, &output.join(format!("control-{case}.log")))?;
        let passed = match expected {
            None => step.passed(),
            Some(marker) => step.exit != Some(0) && step.text.contains(marker),
        };
        results.push(json!({
            "case": case,
            "expected": expected,
            "exit": step.exit,
            "passed": passed,
            "log": step.log.display().to_string(),
        }));
    }
    Ok(results)
}

/// Runs the hardware probe on the default output device with digital
/// silence: CoreAudio's real callback thread against the producer.
fn device_probe(
    sanitizer: Sanitizer,
    target_dir: &Path,
    wrappers: &Path,
    output: &Path,
    package_directory: &Path,
    runs: u32,
    failures: &mut Vec<String>,
) -> Result<Value, String> {
    let mut cargo = cargo_command(sanitizer, wrappers);
    cargo.args([
        "build",
        "-p",
        "deadpan-output",
        "--example",
        "qualify_output",
    ]);
    finish_cargo(&mut cargo, target_dir);
    let artifacts = cargo_artifacts(cargo, &output.join("build-device-probe.log"))?;
    let binary = artifacts
        .iter()
        .find(|(_, kind, _)| kind == "example")
        .map(|(_, _, executable)| executable.clone())
        .ok_or("qualify_output was not built")?;
    let mut steps = Vec::new();
    for run in 1..=runs {
        let mut command = Command::new(&binary);
        command
            .current_dir(package_directory)
            .envs(sanitizer.runtime_environment());
        let step = run_step(
            command,
            &output.join(format!("output-device-probe-{run}.log")),
        )?;
        let checks_failed = step.text.contains("\"passed\": false");
        if !step.passed() || checks_failed {
            failures.push(format!(
                "{} device probe run {run}: {}",
                sanitizer.name(),
                if checks_failed {
                    format!("a check failed; see {}", step.log.display())
                } else {
                    step.describe()
                }
            ));
        }
        let callbacks = step
            .text
            .lines()
            .find_map(|line| line.trim().strip_prefix("\"callback_count\": "))
            .map(|count| count.trim_end_matches(',').to_owned());
        let mut record = step.json(Counts::default());
        record["callback_count"] = json!(callbacks);
        record["checks_failed"] = json!(checks_failed);
        steps.push(record);
    }
    Ok(json!({
        "binary": binary.display().to_string(),
        "sha256": crate::replays::sha256(&binary)?,
        "instrumented": imports_symbol(&binary, sanitizer.init_symbol())?,
        "runs": steps,
    }))
}

struct Whisper {
    path: PathBuf,
    sha256: String,
}

/// The installed whisper-base-en pack's model unless one is given; read only.
fn whisper_model(path: Option<&Path>) -> Result<Whisper, String> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?)
            .join("Library/Application Support/Deadpan/Models/whisper-base-en/1/ggml-base.en.bin"),
    };
    if !path.is_file() {
        return Err(format!(
            "whisper model {} not found; install the whisper-base-en pack or pass --whisper-model, or omit transcribe with --only",
            path.display()
        ));
    }
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let sha256 = crate::replays::sha256(&path)?;
    Ok(Whisper { path, sha256 })
}

/// Builds the CLI and worker together, creates a fresh project from the
/// speech fixture and transcribes it: real whisper.cpp/ggml (Metal) inference
/// and the worker's abort callback under the sanitizer.
#[allow(clippy::too_many_arguments)]
fn transcription(
    sanitizer: Sanitizer,
    target_dir: &Path,
    wrappers: &Path,
    output: &Path,
    repository: &Path,
    whisper: &Whisper,
    runs: u32,
    failures: &mut Vec<String>,
) -> Result<Value, String> {
    let mut cargo = cargo_command(sanitizer, wrappers);
    cargo.args([
        "build",
        "-p",
        "deadpan-cli",
        "--bin",
        "deadpan-cli",
        "-p",
        "deadpan-transcribe",
        "--bin",
        "deadpan-transcribe",
    ]);
    finish_cargo(&mut cargo, target_dir);
    let artifacts = cargo_artifacts(cargo, &output.join("build-transcription.log"))?;
    let find = |name: &str| {
        artifacts
            .iter()
            .find(|(_, kind, executable)| {
                kind == "bin" && executable.file_name().is_some_and(|file| file == name)
            })
            .map(|(_, _, executable)| executable.clone())
            .ok_or_else(|| format!("{name} was not built"))
    };
    let cli = find("deadpan-cli")?;
    let worker = find("deadpan-transcribe")?;
    if cli.parent() != worker.parent() {
        return Err("the CLI and transcription worker are not beside each other".into());
    }
    let fixture = repository.join(SPEECH_FIXTURE);
    let mut steps = Vec::new();
    for run in 1..=runs {
        let scratch = output.join(format!("transcribe-scratch-{run}"));
        std::fs::create_dir_all(&scratch).map_err(|error| error.to_string())?;
        let package = scratch.join("speech.deadpan");
        let mut create = Command::new(&cli);
        create
            .args(["project", "create-original"])
            .arg(&package)
            .arg(&fixture)
            .envs(sanitizer.runtime_environment());
        let created = run_step(create, &output.join(format!("transcribe-create-{run}.log")))?;
        if !created.passed() {
            failures.push(format!(
                "{} transcription project creation run {run}: {}",
                sanitizer.name(),
                created.describe()
            ));
            steps.push(json!({"create": created.json(Counts::default())}));
            continue;
        }
        let mut transcribe = Command::new(&cli);
        transcribe
            .arg("transcribe")
            .arg(&package)
            .arg("--model")
            .arg(&whisper.path)
            .args(["--sha256", &whisper.sha256])
            .envs(sanitizer.runtime_environment());
        let report = output.join(format!("transcribe-report-{run}.json"));
        let step = run_step_with_stdout(
            transcribe,
            &output.join(format!("transcribe-run-{run}.log")),
            Some(&report),
        )?;
        // The CLI prints one JSON report; its words prove inference ran.
        let words = std::fs::read(&report)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .map(|report| count_words(&report));
        let recognized = words.is_some_and(|words| words > 0);
        if !step.passed() || !recognized {
            failures.push(format!(
                "{} transcription run {run}: {}",
                sanitizer.name(),
                if step.passed() {
                    format!("no words were recognized; see {}", step.log.display())
                } else {
                    step.describe()
                }
            ));
        }
        let mut record = step.json(Counts::default());
        record["words"] = json!(words);
        steps.push(json!({"create": created.json(Counts::default()), "transcribe": record}));
        let _ = std::fs::remove_dir_all(&scratch);
    }
    Ok(json!({
        "cli": cli.display().to_string(),
        "cli_sha256": crate::replays::sha256(&cli)?,
        "worker": worker.display().to_string(),
        "worker_sha256": crate::replays::sha256(&worker)?,
        "worker_instrumented": imports_symbol(&worker, sanitizer.init_symbol())?,
        "worker_ubsan_handler_imports": ubsan_handlers(&worker)?,
        "fixture": SPEECH_FIXTURE,
        "fixture_sha256": crate::replays::sha256(&fixture)?,
        "model": whisper.path.display().to_string(),
        "model_sha256": whisper.sha256,
        "runs": steps,
    }))
}

/// The transcribe report's recognized word count.
fn count_words(report: &Value) -> u64 {
    report["words"].as_u64().unwrap_or(0)
}

fn capture(program: &str, arguments: &[&str]) -> Option<String> {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
