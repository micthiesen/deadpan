//! Developer-only replay of the production app, with private project storage.

mod comparison;
pub(crate) mod gpu;
pub(crate) mod report;

use std::io::Read;
use std::path::{Path, PathBuf};

use report::{Check, Report, RunMode, ScenarioReport};
use serde_json::json;

pub(crate) const SCENARIOS: &[&str] = &[
    "workspace",
    "editing",
    "camera",
    "targets",
    "points",
    "faces",
    "menus",
    "palette",
    "delayed-preview",
    "rapid-input",
    "keymap",
    "keymap-error",
    "playback-feedback",
    "terminal-playback",
    "large-project",
    "edit-latency",
    "large-first-pause",
    "nested-pause",
    "original-moment",
    "transcript",
    "corrections",
    "shots",
    "proxy-seek",
    "source-interpretation",
    "cutaway",
    "gags",
    "recipes",
    "hold-effects",
    "hold-follow-creep",
    "tail-sends",
    "wet-returns",
    "audio-treatments",
    "equalizer",
    "split-edits",
    "recipe-library",
    "captions",
    "zoom",
    "original-layout",
    "original-layout-long",
    "place-slice",
    "delete-range",
    "marks",
    "named-registers",
    "dot-repeat",
    "named-mark-selectors",
    "explicit-node-selectors",
    "node-set-selectors",
    "range-set-selectors",
    "transcript-search-selectors",
    "creative-dot",
    "repeat-operator",
    "repeat-setters",
    "groups",
    "sound-group-endpoints",
    "scoped-plays",
    "owned-gap-duration",
    "fixed-retime-duration",
    "this-play-fixed-duration",
    "nested-fixed-duration",
    "mixed-fixed-duration",
    "selected-fixed-duration",
    "structure-copies",
    "macros",
    "original-playback",
    "audio-warming",
    "sound-playback",
    "sound-placement",
    "room-tone",
    "gain",
    "ai-pause",
    "ai-variants",
    "ai-extension",
    "ai-compare",
    "ai-scoped",
    "ai-replacements",
    "ai-boundaries",
    "ai-insertion",
    "ai-pause-ready",
    "model-packs",
    "retime",
    "slip",
    "trim",
    "render",
    "recovery",
    "relink",
    "sound-relink",
    "storage-failure",
    "generated-picture",
    "generated-playback",
    "generated-loop",
    "ai-playback-interference",
    "youtube",
    "linked-original",
    "accessibility",
    "diagnostics",
    "storage",
    "backups",
    "takes",
    "damaged-recovery",
    "jobs",
    "full-session",
    "layouts",
];

pub(crate) struct Options {
    pub output: PathBuf,
    pub mode: RunMode,
    pub scenario: Option<String>,
    pub kestrel_source: Option<PathBuf>,
    pub hz: u32,
    pub baseline: Option<PathBuf>,
    pub retain_projects: bool,
    pub project: Option<PathBuf>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut output = None;
        let mut mode = RunMode::Visual;
        let mut scenario = None;
        let mut kestrel_source = None;
        let mut hz = 60;
        let mut baseline = None;
        let mut retain_projects = false;
        let mut project = None;
        let mut args = arguments.iter();
        while let Some(flag) = args.next() {
            if flag == "--retain-projects" {
                if retain_projects {
                    return Err("Specify --retain-projects only once".into());
                }
                retain_projects = true;
                continue;
            }
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
                "--project" if project.is_none() => project = Some(PathBuf::from(value)),
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
        let fixture_scenario = scenario
            .as_deref()
            .is_some_and(|name| PROJECT_FIXTURE_SCENARIOS.contains(&name));
        if project.is_some() && !fixture_scenario {
            return Err(
                "--project requires generated-picture, generated-playback, generated-loop, ai-playback-interference, ai-pause-ready or large-first-pause".into(),
            );
        }
        if fixture_scenario && project.is_none() {
            return Err(format!(
                "{} replay requires an explicit --project fixture",
                scenario.as_deref().unwrap_or_default()
            ));
        }
        if scenario.as_deref().is_some_and(real_playback) && mode != RunMode::Performance {
            return Err("Real playback opens device audio and requires --mode performance".into());
        }
        if project.as_ref().is_some_and(|path| {
            !path.is_absolute()
                || path
                    .extension()
                    .is_none_or(|extension| extension != "deadpan")
        }) {
            return Err("--project must name an absolute .deadpan fixture package".into());
        }
        if project.is_some()
            && retain_projects
            && scenario.as_deref() != Some("ai-playback-interference")
        {
            return Err(
                "The explicit --project fixture is already retained; omit --retain-projects".into(),
            );
        }
        Ok(Self {
            output: output.ok_or("Specify a new --output directory")?,
            mode,
            scenario,
            kestrel_source,
            hz,
            baseline,
            retain_projects,
            project,
        })
    }

    pub(crate) fn retained_project_root(&self, scenario: &str) -> Result<Option<PathBuf>, String> {
        if !self.retain_projects {
            return Ok(None);
        }
        if !SCENARIOS.contains(&scenario) {
            return Err(format!("Unknown retained-project scenario: {scenario}"));
        }
        Ok(Some(self.output.join("projects").join(scenario)))
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = deadpan_native_process::spawn(
        std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    )
    .ok()?
    .wait_with_output()
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

/// Scenarios that replay an explicit `--project` fixture instead of the
/// built-in Original; ordinary runs report them as skipped.
pub(crate) const PROJECT_FIXTURE_SCENARIOS: &[&str] = &[
    "generated-picture",
    "generated-playback",
    "generated-loop",
    "ai-playback-interference",
    "ai-pause-ready",
    "large-first-pause",
];

pub(crate) fn real_playback(name: &str) -> bool {
    matches!(
        name,
        "generated-playback" | "generated-loop" | "ai-playback-interference"
    )
}

/// One scenario per line for tools such as `cargo xtask replays`. A
/// scenario that needs an explicit fixture carries a tab and `project`.
fn scenario_list() -> String {
    SCENARIOS
        .iter()
        .map(|name| {
            if PROJECT_FIXTURE_SCENARIOS.contains(name) {
                format!("{name}\tproject\n")
            } else {
                format!("{name}\n")
            }
        })
        .collect()
}

pub(crate) fn entry(arguments: &[String]) -> Result<(), String> {
    if arguments == ["--list-scenarios"] {
        print!("{}", scenario_list());
        return Ok(());
    }
    if arguments == ["--help"] {
        println!(
            "Usage: deadpan-app --ui-check --output NEW_DIRECTORY [--mode visual|performance] [--scenario NAME] [--hz 60|120] [--kestrel-source Shortcuts.swift] [--baseline PRIOR_DIRECTORY] [--retain-projects]\n       deadpan-app --ui-check --list-scenarios\n\nScenarios: {}\nVisual mode writes report.json, report.html and actual offscreen PNG frames.\nPerformance mode submits full UI + picture GPU work without screenshot readback.\nUse --release for performance. Projects use private temporary storage by default.\n--retain-projects keeps each scenario's Documents root under output/projects/NAME for native QA after replay exits.\nGenerated picture replay requires --scenario generated-picture --project /absolute/accepted.deadpan, exported by the real bundle qualification test with DEADPAN_GENERATED_PICTURE_FIXTURE_ROOT. It opens the generic compatibility fixture without editing it.\nlarge-first-pause requires --project /absolute/fresh.deadpan made by perf make-large --beats 10000; one hold 12f at frame 17933 on a private copy. Timings describe offscreen Background acceptance/composition, n=1 diagnostic only; no percentile or latency-budget PASS.\nai-pause-ready requires --project /absolute/project.deadpan whose last edit accepted real AI pictures; it replays a private copy.\ngenerated-playback requires --mode performance --project and runs real device audio at 5% monitor gain on a private copy of a <=32-second accepted project. generated-loop uses the same real-audio setup, selects from the first through last Generated picture (at most eight seconds), and loops for at least 30 seconds. ai-playback-interference requires --mode performance --project with a fresh full-source Original of at least 40 seconds and the installed Extension model; it uses real inference, production scheduling and device audio at 5% monitor gain on a private project copy. All other scenarios simulate audio delivery. Native pickers are scripted; no desktop window is opened.",
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
    let fixture = options
        .project
        .clone()
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        })
        .canonicalize()
        .map_err(|error| format!("Resolve replay fixture: {error}"))?;
    let large_first_pause = options.scenario.as_deref() == Some("large-first-pause");
    let ready_fixture = matches!(
        options.scenario.as_deref(),
        Some(
            "ai-pause-ready" | "generated-playback" | "generated-loop" | "ai-playback-interference"
        )
    );
    let fixture_identity = if ready_fixture || large_first_pause {
        fixture.join("project.sqlite")
    } else if options.project.is_some() {
        fixture
            .parent()
            .ok_or("Fixture package has no parent")?
            .join("generated-picture-fixture.json")
    } else {
        fixture.clone()
    };
    let fixture_hash = if ready_fixture || large_first_pause {
        let mut bytes = Vec::new();
        std::fs::File::open(&fixture_identity)
            .map_err(|error| error.to_string())?
            .take(256 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 256 * 1024 * 1024 {
            return Err("Replay fixture database exceeds 256 MiB".into());
        }
        sha256(&bytes)
    } else if options.project.is_some() {
        let mut bytes = Vec::new();
        std::fs::File::open(&fixture_identity)
            .map_err(|error| error.to_string())?
            .take(128 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 128 * 1024 {
            return Err("Generated fixture expectations exceed 128 KiB".into());
        }
        sha256(&bytes)
    } else {
        std::fs::read(&fixture_identity)
            .map(|bytes| sha256(&bytes))
            .map_err(|error| error.to_string())?
    };
    let interference = options.scenario.as_deref() == Some("ai-playback-interference");
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
            "fixture": if large_first_pause { "pristine perf make-large --beats 10000 Background/Silence Holds (private portable copy; no source media)" } else if interference { "full-source Original project (replayed from a private portable copy)" } else if ready_fixture { "project with real accepted AI pictures (replayed from a private copy)" } else if options.project.is_some() { "accepted Generated compatibility project" } else { "cfr-bframes.mp4" },
            "fixture_identity_file": fixture_identity,
            "fixture_sha256": fixture_hash,
            "replay_hz": options.hz, "viewport_points": [1280,820],
            "physical_display_measured": false, "gpu_readback_in_timing_run": false,
            "wait_strategy": "egui_repaint_callback_v1",
            "picture_worker_timing": "request_start_finish_publication_receipt_v1",
            "cache_state": if large_first_pause { "one first pause in a fresh process/private project copy; initial Background settled; OS file cache uncontrolled" } else if interference { "baseline precedes inference in the same process; decoder and model caches are not reset; OS file cache uncontrolled" } else if options.project.is_some() { "first six-object generated admission/index cold, subsequent navigation warm; OS file cache uncontrolled" } else { "first import/index cold, subsequent operations warm; OS file cache uncontrolled" },
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
        if real_playback(name) && options.project.is_none() {
            let mut skipped = ScenarioReport::new(*name);
            skipped.skipped.push("Requires an explicit scenario, --mode performance and --project; opens real device audio at 5% monitor gain on a private accepted-project copy.".into());
            report.scenarios.push(skipped);
            continue;
        }
        if *name == "generated-picture" && options.project.is_none() {
            let mut skipped = ScenarioReport::new(*name);
            skipped.skipped.push("Requires explicit --scenario generated-picture --project and the real bundle qualification fixture; ordinary replay does not fabricate accepted evidence.".into());
            report.scenarios.push(skipped);
            continue;
        }
        if *name == "ai-pause-ready" && options.project.is_none() {
            let mut skipped = ScenarioReport::new(*name);
            skipped.skipped.push("Requires explicit --scenario ai-pause-ready --project with a project whose last edit accepted real AI pictures; ordinary replay does not fabricate Ready bundles.".into());
            report.scenarios.push(skipped);
            continue;
        }
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
    fn scenario_list_names_every_scenario_once_and_marks_fixture_replays() {
        let list = scenario_list();
        let lines = list.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), SCENARIOS.len());
        for (line, name) in lines.iter().zip(SCENARIOS) {
            let (listed, tag) = line.split_once('\t').unwrap_or((line, ""));
            assert_eq!(listed, *name);
            assert_eq!(tag == "project", PROJECT_FIXTURE_SCENARIOS.contains(name));
        }
        assert!(
            PROJECT_FIXTURE_SCENARIOS
                .iter()
                .all(|name| SCENARIOS.contains(name))
        );
    }

    #[test]
    fn large_first_pause_requires_an_absolute_explicit_project() {
        let arguments = |tail: &[&str]| {
            [
                "--output",
                "/tmp/example",
                "--scenario",
                "large-first-pause",
            ]
            .into_iter()
            .chain(tail.iter().copied())
            .map(str::to_owned)
            .collect::<Vec<_>>()
        };
        assert!(
            Options::parse(&arguments(&[
                "--mode",
                "performance",
                "--project",
                "/tmp/fresh.deadpan",
            ]))
            .is_ok()
        );
        for tail in [
            vec![],
            vec!["--project", "fresh.deadpan"],
            vec!["--project", "/tmp/fresh"],
            vec!["--project", "/tmp/fresh.deadpan", "--retain-projects"],
        ] {
            assert!(Options::parse(&arguments(&tail)).is_err());
        }
    }

    #[test]
    fn generated_picture_requires_a_dedicated_explicit_project() {
        let arguments = |tail: &[&str]| {
            ["--output", "/tmp/example"]
                .into_iter()
                .chain(tail.iter().copied())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let valid = Options::parse(&arguments(&[
            "--scenario",
            "generated-picture",
            "--project",
            "/tmp/fixture/accepted.deadpan",
        ]))
        .unwrap();
        assert_eq!(
            valid.project,
            Some(PathBuf::from("/tmp/fixture/accepted.deadpan"))
        );
        for invalid in [
            vec!["--scenario", "generated-picture"],
            vec!["--project", "/tmp/fixture/accepted.deadpan"],
            vec![
                "--scenario",
                "workspace",
                "--project",
                "/tmp/fixture/accepted.deadpan",
            ],
            vec![
                "--scenario",
                "generated-picture",
                "--project",
                "relative.deadpan",
            ],
            vec![
                "--scenario",
                "generated-picture",
                "--project",
                "/tmp/fixture",
            ],
            vec![
                "--scenario",
                "generated-picture",
                "--project",
                "/tmp/fixture/accepted.deadpan",
                "--retain-projects",
            ],
            vec![
                "--scenario",
                "generated-picture",
                "--project",
                "/tmp/fixture/accepted.deadpan",
                "--project",
                "/tmp/another.deadpan",
            ],
        ] {
            assert!(Options::parse(&arguments(&invalid)).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn real_playback_requires_explicit_performance_mode_and_project() {
        for name in [
            "generated-playback",
            "generated-loop",
            "ai-playback-interference",
        ] {
            let arguments = |tail: &[&str]| {
                ["--output", "/tmp/example", "--scenario", name]
                    .into_iter()
                    .chain(tail.iter().copied())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            };
            assert!(Options::parse(&arguments(&["--project", "/tmp/accepted.deadpan"])).is_err());
            assert!(Options::parse(&arguments(&["--mode", "performance"])).is_err());
            assert!(
                Options::parse(&arguments(&[
                    "--project",
                    "/tmp/accepted.deadpan",
                    "--mode",
                    "performance"
                ]))
                .is_ok()
            );
            assert!(
                Options::parse(&arguments(&[
                    "--project",
                    "/tmp/accepted.deadpan",
                    "--mode",
                    "visual"
                ]))
                .is_err()
            );
        }
    }

    #[test]
    fn interference_retains_its_private_copy_for_attempt_evidence() {
        let options = Options::parse(
            &[
                "--output",
                "/tmp/replay",
                "--scenario",
                "ai-playback-interference",
                "--project",
                "/tmp/original.deadpan",
                "--mode",
                "performance",
                "--retain-projects",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(
            options
                .retained_project_root("ai-playback-interference")
                .unwrap(),
            Some(PathBuf::from(
                "/tmp/replay/projects/ai-playback-interference"
            ))
        );
    }

    #[test]
    fn invalid_runs_fail_before_creating_artifacts() {
        for args in [
            vec!["--output"],
            vec!["--output", "/tmp/example", "--scenario", "typo"],
            vec!["--output", "/tmp/example", "--hz", "4"],
            vec![
                "--retain-projects",
                "--output",
                "/tmp/example",
                "--scenario",
                "../gain",
            ],
            vec![
                "--output",
                "/tmp/example",
                "--retain-projects",
                "--retain-projects",
            ],
            vec!["--output", "/tmp/example", "--retain-projects", "false"],
        ] {
            assert!(
                Options::parse(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err()
            );
        }
    }

    #[test]
    fn project_retention_is_explicit_and_does_not_consume_the_next_option() {
        let temporary = Options::parse(&["--output".into(), "relative-output".into()]).unwrap();
        assert!(!temporary.retain_projects);
        assert_eq!(temporary.retained_project_root("gain").unwrap(), None);
        for args in [
            vec![
                "--retain-projects",
                "--output",
                "relative-output",
                "--scenario",
                "gain",
            ],
            vec![
                "--output",
                "relative-output",
                "--scenario",
                "gain",
                "--retain-projects",
            ],
        ] {
            let options =
                Options::parse(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).unwrap();
            assert!(options.retain_projects);
            assert_eq!(options.scenario.as_deref(), Some("gain"));
            assert_eq!(
                options.retained_project_root("gain").unwrap(),
                Some(PathBuf::from("relative-output/projects/gain"))
            );
        }
    }

    #[test]
    fn retained_project_paths_accept_only_known_scenario_names() {
        let options = Options::parse(&[
            "--output".into(),
            "/tmp/example".into(),
            "--retain-projects".into(),
        ])
        .unwrap();
        for scenario in SCENARIOS {
            assert_eq!(
                options.retained_project_root(scenario).unwrap(),
                Some(PathBuf::from("/tmp/example/projects").join(scenario))
            );
        }
        for scenario in [
            "",
            "..",
            "../gain",
            "gain/../../outside",
            "/tmp/outside",
            "unknown",
        ] {
            assert!(
                options.retained_project_root(scenario).is_err(),
                "{scenario}"
            );
        }
    }
}
