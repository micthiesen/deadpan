//! Developer-only replay of the production app, with private disposable projects.

mod comparison;
pub(crate) mod gpu;
pub(crate) mod report;

use std::path::{Path, PathBuf};

use report::{Check, Report, RunMode, ScenarioReport};
use serde_json::json;

pub(crate) const SCENARIOS: &[&str] = &[
    "workspace",
    "editing",
    "camera",
    "menus",
    "delayed-preview",
    "rapid-input",
    "playback-feedback",
    "large-project",
    "edit-latency",
    "nested-pause",
    "original-moment",
    "original-playback",
    "sound-playback",
    "retime",
];

pub(crate) struct Options {
    pub output: PathBuf,
    pub mode: RunMode,
    pub scenario: Option<String>,
    pub kestrel_source: Option<PathBuf>,
    pub hz: u32,
    pub baseline: Option<PathBuf>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut output = None;
        let mut mode = RunMode::Visual;
        let mut scenario = None;
        let mut kestrel_source = None;
        let mut hz = 60;
        let mut baseline = None;
        let mut args = arguments.iter();
        while let Some(flag) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("Missing value for {flag}"))?;
            match flag.as_str() {
                "--output" if output.is_none() => output = Some(PathBuf::from(value)),
                "--mode" => {
                    mode = match value.as_str() {
                        "visual" => RunMode::Visual,
                        "performance" => RunMode::Performance,
                        _ => return Err("Mode must be visual or performance".into()),
                    }
                }
                "--scenario" if SCENARIOS.contains(&value.as_str()) => {
                    scenario = Some(value.clone())
                }
                "--kestrel-source" => kestrel_source = Some(PathBuf::from(value)),
                "--baseline" => baseline = Some(PathBuf::from(value)),
                "--hz" => {
                    hz = value
                        .parse()
                        .ok()
                        .filter(|hz| [60, 120].contains(hz))
                        .ok_or("Replay rate must be 60 or 120")?
                }
                _ => return Err(format!("Unknown option or scenario: {flag} {value}")),
            }
        }
        if baseline.is_some() && mode != RunMode::Visual {
            return Err("Baseline comparison requires visual mode".into());
        }
        Ok(Self {
            output: output.ok_or("Specify a new --output directory")?,
            mode,
            scenario,
            kestrel_source,
            hz,
            baseline,
        })
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn binary_sha256() -> Option<String> {
    // Hashing a debug binary in unoptimized Rust dominates a short replay.
    // macOS's developer-side digest tool keeps this outside measured work.
    let path = std::env::current_exe().ok()?;
    let digest = command_output("shasum", &["-a", "256", path.to_str()?])?;
    let digest = digest.split_whitespace().next()?;
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| digest.to_owned())
}

pub(crate) fn entry(arguments: &[String]) -> Result<(), String> {
    if arguments == ["--help"] {
        println!(
            "Usage: deadpan-app --ui-check --output NEW_DIRECTORY [--mode visual|performance] [--scenario NAME] [--hz 60|120] [--kestrel-source Shortcuts.swift] [--baseline PRIOR_DIRECTORY]\n\nScenarios: {}\nVisual mode writes report.json, report.html and actual offscreen PNG frames.\nPerformance mode submits full UI + picture GPU work without screenshot readback.\nUse --release for performance. All projects live in private temporary storage.\nNative pickers are scripted; audio output and the desktop are not opened.",
            SCENARIOS.join(", ")
        );
        return Ok(());
    }
    let options = Options::parse(arguments)?;
    if options.mode == RunMode::Performance && cfg!(debug_assertions) {
        return Err(
            "Performance checks require --release; debug-build timings are not comparable".into(),
        );
    }
    std::fs::create_dir(&options.output).map_err(|error| {
        format!(
            "Create exclusive report directory {}: {error}",
            options.output.display()
        )
    })?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()
        .map_err(|error| format!("Resolve replay fixture: {error}"))?;
    let fixture_hash = std::fs::read(&fixture)
        .map(|bytes| sha256(&bytes))
        .map_err(|error| error.to_string())?;
    let mut report = Report::new(
        options.mode,
        json!({
            "started_unix_ms": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|elapsed| elapsed.as_millis()),
            "binary_sha256": binary_sha256(),
            "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "git_revision": command_output("git", &["rev-parse", "HEAD"]),
            "git_status": command_output("git", &["-c", "core.fsmonitor=false", "status", "--short"]),
            "tracked_diff_sha256_at_run": command_output("git", &["-c", "core.fsmonitor=false", "diff", "--binary", "HEAD"]).map(|diff| sha256(diff.as_bytes())),
            "cargo_lock_sha256_at_run": std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock")).ok().map(|bytes| sha256(&bytes)),
            "source_identity_note": "Git metadata describes the checkout at replay start; binary_sha256 identifies the executed build even during concurrent edits.",
            "hardware": command_output("sysctl", &["-n", "machdep.cpu.brand_string"]),
            "os": command_output("sw_vers", &["-productVersion"]),
            "rust": command_output("rustc", &["--version"]),
            "fixture": "cfr-bframes.mp4", "fixture_sha256": fixture_hash,
            "replay_hz": options.hz, "viewport_points": [1280,820],
            "physical_display_measured": false, "gpu_readback_in_timing_run": false,
            "wait_strategy": "egui_repaint_callback_v1",
            "picture_worker_timing": "request_start_finish_publication_receipt_v1",
            "cache_state": "first import/index cold, subsequent operations warm; OS file cache uncontrolled",
            "power_thermal_state": "uncontrolled; compare on the same idle machine and power mode",
        }),
    );
    let audit = (|| match &options.kestrel_source {
        Some(path) => crate::navigation::shortcut_audit::audit_with_kestrel_source(
            &std::fs::read_to_string(path).map_err(|e| e.to_string())?,
        ),
        None => crate::navigation::shortcut_audit::audit(),
    })();
    let mut shortcuts = ScenarioReport::new("kestrel-shortcuts");
    match audit {
        Ok(audit) => shortcuts.checks.push(Check {
            name: "Production key routing reserves all Kestrel globals".into(), passed: audit.passed(),
            expected: json!({"conflicts":0,"source_drift":false}),
            actual: json!({"reserved_bindings":audit.reserved_bindings,"routing_cases":audit.routing_cases,"source_sha256":audit.source_sha256,"live_source_sha256":audit.live_source_sha256,"conflicts":audit.conflicts.iter().map(|c| json!({"chord":c.chord,"context":c.context,"response":c.response})).collect::<Vec<_>>() }),
        }),
        Err(error) => shortcuts.checks.push(Check { name: "Kestrel audit".into(), passed:false, expected:json!("valid reserved registry"), actual:json!(error) }),
    }
    report.scenarios.push(shortcuts);
    report::write_report(&report, &options.output).map_err(|error| error.to_string())?;
    let replay_started = std::time::Instant::now();
    for name in SCENARIOS.iter().filter(|name| {
        options
            .scenario
            .as_deref()
            .is_none_or(|selected| selected == **name)
    }) {
        let scenario = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::preview::harness::run(name, &options, &fixture)
        }))
        .unwrap_or_else(|panic| {
            let mut failed = ScenarioReport::new(*name);
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "Unknown panic".into());
            failed.findings.push(report::Finding {
                severity: report::Severity::Failure,
                message: format!("Scenario panicked: {message}"),
            });
            failed
        });
        println!(
            "{}: {}",
            name,
            if scenario.is_failure() {
                "FAIL"
            } else {
                "PASS"
            }
        );
        report.scenarios.push(scenario);
        report::write_report(&report, &options.output).map_err(|error| error.to_string())?;
    }
    report.metadata["replay_wall_ms"] = json!(replay_started.elapsed().as_secs_f64() * 1000.0);
    report::write_report(&report, &options.output).map_err(|error| error.to_string())?;
    if let Some(baseline) = &options.baseline {
        if let Err(error) = comparison::compare(&mut report, &options.output, baseline) {
            let mut comparison = ScenarioReport::new("baseline-comparison");
            comparison.findings.push(report::Finding {
                severity: report::Severity::Failure,
                message: error,
            });
            report.scenarios.push(comparison);
        }
        report::write_report(&report, &options.output).map_err(|error| error.to_string())?;
    }
    println!(
        "UI feedback: {}",
        options.output.join("report.html").display()
    );
    if report.is_failure() {
        Err("UI feedback checks failed; inspect report.json and report.html".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_runs_fail_before_creating_artifacts() {
        for args in [
            vec!["--output"],
            vec!["--output", "/tmp/example", "--scenario", "typo"],
            vec!["--output", "/tmp/example", "--hz", "4"],
        ] {
            assert!(
                Options::parse(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err()
            );
        }
    }
}
