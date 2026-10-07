//! `cargo xtask bundle-verify`: exercise a copied bundle in a scrubbed
//! environment.
//!
//! The bundle is copied with `ditto` to a fresh temporary directory and every
//! command runs there with only `HOME` and `PATH=/usr/bin:/bin`, from an empty
//! working directory. `HOME` is a fresh directory holding only a managed
//! helper copy (when the build user has one), so a fallback would be visible.
//! Neither `DEADPAN_FFMPEG_PREFIX` nor any Cargo state is visible. Negative
//! cases tamper with or delete the bundled helpers in separate copies. This proves relocation on the build Mac; it is not the
//! clean-machine release test (specification §26.6).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::{APP_NAME, Result, build_markers, macho, print_audit, run_tool, workspace_root};

const USAGE: &str = "usage: cargo xtask bundle-verify <Deadpan.app> [--fixture <absolute video>] [--ai-models-from <folder or archive>] [--keep]";

struct Scrubbed {
    home: String,
    work: PathBuf,
}

impl Scrubbed {
    fn run(&self, program: &Path, arguments: &[&str]) -> Result<(bool, String, String)> {
        let result = Command::new(program)
            .args(arguments)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .current_dir(&self.work)
            .output()
            .map_err(|e| format!("failed to start {}: {e}", program.display()))?;
        Ok((
            result.status.success(),
            String::from_utf8_lossy(&result.stdout).into_owned(),
            String::from_utf8_lossy(&result.stderr).into_owned(),
        ))
    }

    fn json(&self, program: &Path, arguments: &[&str]) -> Result<Value> {
        let (success, stdout, stderr) = self.run(program, arguments)?;
        if !success {
            return Err(format!(
                "{} {} failed: {}{}",
                program.display(),
                arguments.join(" "),
                stdout.trim(),
                stderr.trim()
            ));
        }
        let last = stdout
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        serde_json::from_str(&stdout)
            .or_else(|_| serde_json::from_str(last))
            .map_err(|e| format!("{} output is not JSON ({e}): {stdout}", arguments.join(" ")))
    }
}

fn check(condition: bool, message: impl Into<String>, failures: &mut Vec<String>) {
    let message = message.into();
    if condition {
        println!("verify: ok   {message}");
    } else {
        println!("verify: FAIL {message}");
        failures.push(message);
    }
}

pub fn run(arguments: &[String]) -> Result<()> {
    let mut app = None;
    let mut fixture = None;
    let mut keep = false;
    let mut models_from = None;
    let mut rest = arguments;
    while let Some((flag, tail)) = rest.split_first() {
        match (flag.as_str(), tail) {
            ("--fixture", [value, tail @ ..]) => {
                fixture = Some(PathBuf::from(value));
                rest = tail;
            }
            ("--ai-models-from", [value, tail @ ..]) => {
                models_from = Some(PathBuf::from(value));
                rest = tail;
            }
            ("--keep", tail) => {
                keep = true;
                rest = tail;
            }
            (value, tail) if app.is_none() && !value.starts_with("--") => {
                app = Some(PathBuf::from(value));
                rest = tail;
            }
            _ => return Err(USAGE.into()),
        }
    }
    let source = fs::canonicalize(app.ok_or(USAGE)?).map_err(|e| e.to_string())?;
    let fixture = match fixture {
        Some(path) => path,
        None => workspace_root().join("native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    };
    let fixture = fs::canonicalize(&fixture).map_err(|e| format!("{}: {e}", fixture.display()))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let root = std::env::temp_dir()
        .canonicalize()
        .map_err(|e| e.to_string())?
        .join(format!(
            "deadpan-bundle-verify-{}-{nanos}",
            std::process::id()
        ));
    let work = root.join("work");
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let app = root.join(APP_NAME);
    run_tool("ditto", &[source.as_os_str(), app.as_os_str()])?;
    println!("verify: copied to {}", app.display());
    let home = root.join("home");
    fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    // A verified managed copy in the isolated home: the packaged app must
    // still use, and fail on, its own baseline rather than fall back to it.
    let managed = PathBuf::from(std::env::var("HOME").map_err(|_| "HOME is not set")?)
        .join("Library/Application Support/Deadpan/helpers");
    if managed.is_dir() {
        let destination = home.join("Library/Application Support/Deadpan/helpers");
        fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
        run_tool("ditto", &[managed.as_os_str(), destination.as_os_str()])?;
    }
    let scrubbed = Scrubbed {
        home: home.display().to_string(),
        work: work.clone(),
    };
    let result = exercise(&scrubbed, &app, &fixture, models_from.as_deref())
        .and(negative(&scrubbed, &root, &app));
    if keep || result.is_err() {
        println!("verify: kept {}", root.display());
    } else {
        let _ = fs::remove_dir_all(&root);
    }
    result
}

fn exercise(
    scrubbed: &Scrubbed,
    app: &Path,
    fixture: &Path,
    models_from: Option<&Path>,
) -> Result<()> {
    let mut failures = Vec::new();
    let macos = app.join("Contents/MacOS");
    let cli = macos.join("deadpan-cli");
    let gui = macos.join("deadpan-app");

    let verified = run_tool(
        "codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            app.as_os_str(),
        ],
    );
    check(
        verified.is_ok(),
        format!("codesign --verify --deep --strict {verified:?}"),
        &mut failures,
    );

    let markers = std::env::var_os("DEADPAN_FFMPEG_PREFIX")
        .map(PathBuf::from)
        .map(|prefix| build_markers(&prefix))
        .unwrap_or_default();
    let audit = macho::audit(app, &markers)?;
    print_audit(&audit);
    check(
        audit.problems.is_empty(),
        "otool audit: only system or in-bundle libraries",
        &mut failures,
    );
    let deno_notices = deno_notice_problems(app)?;
    check(
        deno_notices.is_empty(),
        format!("Deno/V8 notices match the vendored notice set {deno_notices:?}"),
        &mut failures,
    );

    let (success, _, stderr) = scrubbed.run(&gui, &["--smoke-test"])?;
    check(
        success,
        format!("deadpan-app --smoke-test {}", stderr.trim()),
        &mut failures,
    );

    let inside = |value: &Value| {
        value
            .as_str()
            .is_some_and(|path| Path::new(path).starts_with(app))
    };
    for (label, program, arguments) in [
        ("deadpan-cli doctor", &cli, &["doctor"][..]),
        (
            "deadpan-app --headless doctor",
            &gui,
            &["--headless", "doctor"][..],
        ),
    ] {
        let doctor = scrubbed.json(program, arguments)?;
        let runtime = &doctor["runtime"];
        check(
            inside(&runtime["executable"]),
            format!("{label}: executable inside the bundle"),
            &mut failures,
        );
        for worker in runtime["workers"].as_array().into_iter().flatten() {
            check(
                worker["present"] == true && worker["inside_bundle"] == true,
                format!("{label}: {} at {}", worker["name"], worker["path"]),
                &mut failures,
            );
        }
        for library in runtime["ffmpeg"].as_array().into_iter().flatten() {
            check(
                library["inside_bundle"] == true,
                format!(
                    "{label}: lib{} loaded from {}",
                    library["library"].as_str().unwrap_or("?"),
                    library["path"]
                ),
                &mut failures,
            );
        }
        let downloader = &doctor["downloader"];
        check(
            downloader["source"] == "bundled",
            format!("{label}: downloader source {}", downloader["source"]),
            &mut failures,
        );
        for helper in downloader["helpers"].as_array().into_iter().flatten() {
            check(
                helper["present"] == true && inside(&helper["path"]),
                format!(
                    "{label}: {} {} at {}",
                    helper["name"], helper["version"], helper["path"]
                ),
                &mut failures,
            );
        }
    }

    let status = scrubbed.json(&cli, &["downloader", "status", "--probe"])?;
    check(
        status["source"] == "bundled",
        "downloader status: bundled baseline",
        &mut failures,
    );
    for helper in status["helpers"].as_array().into_iter().flatten() {
        check(
            helper["verified"] == true,
            format!("downloader status: {} verified", helper["name"]),
            &mut failures,
        );
    }
    let probe = &status["probe"];
    check(
        probe["matches_pins"] == true,
        format!(
            "downloader probe under hardened runtime: yt-dlp {} ejs {} deno {}",
            probe["yt_dlp"], probe["ejs"], probe["deno"]
        ),
        &mut failures,
    );

    let package = scrubbed.work.join("verify.deadpan");
    let package_text = package.to_str().ok_or("non-UTF-8 temp path")?;
    let created = scrubbed.json(
        &cli,
        &[
            "project",
            "create-original",
            package_text,
            fixture.to_str().ok_or("non-UTF-8 fixture")?,
        ],
    );
    check(
        created
            .as_ref()
            .is_ok_and(|value| value["created"]["single_source"]["state"] == "ready"),
        format!("project create-original from {}", fixture.display()),
        &mut failures,
    );
    let exports = scrubbed.work.join("Exports");
    fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
    let (success, stdout, stderr) = scrubbed.run(
        &cli,
        &[
            "render",
            package_text,
            "--output",
            exports.to_str().unwrap(),
            "--name",
            "verify.mp4",
        ],
    )?;
    let movie = exports.join("verify.mp4");
    let bytes = fs::metadata(&movie)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let last = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    check(
        success && bytes > 0,
        format!(
            "render -> {} ({bytes} bytes) {} {}",
            movie.display(),
            last.chars().take(300).collect::<String>(),
            stderr.trim()
        ),
        &mut failures,
    );

    ai_runtime_checks(scrubbed, app, &cli, models_from, &mut failures)?;

    if failures.is_empty() {
        println!("verify: all positive checks passed");
        Ok(())
    } else {
        Err(format!("{} verification checks failed", failures.len()))
    }
}

/// The bundled AI runtime: doctor locates it inside the copy and ignores the
/// environment; Python, MLX, Metal and the GPL ffmpeg run under the hardened
/// runtime from the scrubbed environment. With `models_from`, the bridge pack
/// is imported offline into the isolated home through the bundled CLI, which
/// smoke-tests it with the bundled runtime, and doctor then reports it ready.
fn ai_runtime_checks(
    scrubbed: &Scrubbed,
    app: &Path,
    cli: &Path,
    models_from: Option<&Path>,
    failures: &mut Vec<String>,
) -> Result<()> {
    let runtime = app.join("Contents").join(super::ai_runtime::DIRECTORY);
    let doctor = scrubbed.json(cli, &["doctor"])?;
    let ai = &doctor["runtime"]["ai_runtime"];
    let inside = |value: &Value| {
        value
            .as_str()
            .is_some_and(|path| Path::new(path).starts_with(app))
    };
    check(
        doctor["runtime"]["packaged"] == true && inside(&ai["bundled"]),
        format!("doctor: bundled AI runtime at {}", ai["bundled"]),
        failures,
    );
    check(
        ai["identity"]["runtime_id"] == "ltx-mlx"
            && ai["identity"]["runtime_version"] == "0.15.8+deadpan2",
        format!("doctor: AI runtime identity {}", ai["identity"]),
        failures,
    );
    if models_from.is_none() {
        let missing = ai["missing"].as_array().cloned().unwrap_or_default();
        check(
            ai["ready"] == false
                && missing.len() == 1
                && missing[0]
                    .as_str()
                    .is_some_and(|line| line.starts_with("install the AI model pack")),
            format!("doctor: only the model pack is missing ({missing:?})"),
            failures,
        );
    }
    let staged = super::ai_runtime::check_staged(&runtime, &scrubbed.work);
    check(
        staged.is_ok(),
        format!("bundled Python imports MLX and LTX, runs Metal, ffmpeg has libx264: {staged:?}"),
        failures,
    );
    if let Some(source) = models_from {
        let source = source.to_str().ok_or("non-UTF-8 models path")?;
        let started = std::time::Instant::now();
        let (success, stdout, stderr) = scrubbed.run(
            cli,
            &[
                "models",
                "import",
                "ltx-2.3-q4-bridge",
                source,
                "--accept-license",
            ],
        )?;
        let passed = stdout
            .lines()
            .any(|line| line.contains("\"smoke_test_passed\""));
        check(
            success && passed,
            format!(
                "models import ltx-2.3-q4-bridge with the bundled smoke test in {:.1} s: {}{}",
                started.elapsed().as_secs_f64(),
                stdout
                    .lines()
                    .rev()
                    .find(|line| line.contains("smoke_test_passed"))
                    .unwrap_or_default(),
                stderr.trim()
            ),
            failures,
        );
        let doctor = scrubbed.json(cli, &["doctor"])?;
        let ai = &doctor["runtime"]["ai_runtime"];
        check(
            ai["ready"] == true && inside(&ai["python"]) && inside(&ai["ffmpeg"]),
            format!(
                "doctor: AI pauses ready with model data at {}",
                ai["model_data"]
            ),
            failures,
        );
    }
    Ok(())
}

fn deno_notice_problems(app: &Path) -> Result<Vec<String>> {
    let workspace = workspace_root();
    Ok(super::deno_notices::audit_bundle(
        app,
        &workspace.join("packaging/notices"),
        &super::notices::spdx_identifiers(&workspace)?,
    ))
}

/// Damages one copied bundle.
type Damage = fn(&Path) -> Result<()>;

/// Copies whose bundled helpers are tampered with or deleted must fail
/// verification and never fall back to the managed copy in `HOME`.
fn negative(scrubbed: &Scrubbed, root: &Path, app: &Path) -> Result<()> {
    let mut failures = Vec::new();
    let cases: [(&str, Damage); 2] = [
        ("tampered yt-dlp", |copy| {
            let path = copy.join("Contents/Resources/helpers/yt-dlp/2026.08.19/yt-dlp_macos");
            let mut bytes = fs::read(&path).map_err(|e| e.to_string())?;
            let at = bytes.len() / 2;
            bytes[at] ^= 0x5a;
            fs::write(&path, bytes).map_err(|e| e.to_string())
        }),
        ("deleted helpers directory", |copy| {
            fs::remove_dir_all(copy.join("Contents/Resources/helpers")).map_err(|e| e.to_string())
        }),
    ];
    for (index, (label, damage)) in cases.into_iter().enumerate() {
        let directory = root.join(format!("negative-{index}"));
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let copy = directory.join(APP_NAME);
        run_tool("ditto", &[app.as_os_str(), copy.as_os_str()])?;
        damage(&copy)?;
        let cli = copy.join("Contents/MacOS/deadpan-cli");
        let (_, stdout, _) = scrubbed.run(&cli, &["downloader", "status"])?;
        let status: Value = serde_json::from_str(&stdout).unwrap_or(Value::Null);
        let refused = status["source"] == "bundled"
            && status["helpers"].as_array().is_some_and(|helpers| {
                helpers.iter().any(|helper| {
                    helper["verified"] != true
                        && helper["path"]
                            .as_str()
                            .is_some_and(|path| Path::new(path).starts_with(&copy))
                })
            });
        check(
            refused,
            format!("{label}: downloader status refuses the bundled baseline"),
            &mut failures,
        );
        let (success, _, stderr) = scrubbed.run(&cli, &["downloader", "status", "--probe"])?;
        check(
            !success && stderr.contains("DownloaderHelperInvalid"),
            format!("{label}: probe fails with DownloaderHelperInvalid"),
            &mut failures,
        );
        let doctor = scrubbed.json(&cli, &["doctor"])?;
        let downloader = &doctor["downloader"];
        let reported = downloader["source"] == "bundled"
            && downloader["helpers"].as_array().is_some_and(|helpers| {
                helpers
                    .iter()
                    .any(|helper| helper["present"] == false && helper["problem"].is_string())
            });
        check(
            reported || label.starts_with("tampered"),
            format!("{label}: doctor reports the bundled problem"),
            &mut failures,
        );
    }
    // A changed byte in the AI runtime breaks the bundle's sealed resources.
    let directory = root.join("negative-ai");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let copy = directory.join(APP_NAME);
    run_tool("ditto", &[app.as_os_str(), copy.as_os_str()])?;
    let source = copy
        .join("Contents")
        .join(super::ai_runtime::DIRECTORY)
        .join("worker/worker.py");
    if source.is_file() {
        let mut bytes = fs::read(&source).map_err(|e| e.to_string())?;
        bytes.extend_from_slice(b"\n# tampered\n");
        fs::write(&source, bytes).map_err(|e| e.to_string())?;
        let verified = run_tool(
            "codesign",
            &[
                "--verify".as_ref(),
                "--deep".as_ref(),
                "--strict".as_ref(),
                copy.as_os_str(),
            ],
        );
        check(
            verified.is_err(),
            "tampered AI worker: codesign --verify --deep --strict refuses the bundle",
            &mut failures,
        );
    }
    // Missing aggregated Deno notices fail the notice audit.
    let directory = root.join("negative-deno-notices");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let copy = directory.join(APP_NAME);
    run_tool("ditto", &[app.as_os_str(), copy.as_os_str()])?;
    fs::remove_file(
        copy.join("Contents/Resources/Notices")
            .join(super::deno_notices::BUNDLED_NOTICES),
    )
    .map_err(|e| e.to_string())?;
    let problems = deno_notice_problems(&copy)?;
    check(
        !problems.is_empty(),
        format!("deleted Deno notices: the notice audit refuses the bundle ({problems:?})"),
        &mut failures,
    );
    if failures.is_empty() {
        println!("verify: all negative checks passed");
        Ok(())
    } else {
        Err(format!("{} negative checks failed", failures.len()))
    }
}
