#![cfg(target_os = "macos")]

//! Local workflows need no network (specification Section 27.4,
//! docs/PRIVACY.md).
//!
//! Every `deadpan-cli` process here runs under `sandbox-exec` with all
//! network operations denied (IP and Unix-domain alike), a cleared
//! environment and an empty `HOME`. Creating a one-Original project from a
//! committed fixture, editing, Undo/Redo, validation, inspection, shot
//! detection, transcript and pause reads, backup, storage, portable copy,
//! Render and export verification must all succeed. The explicit download
//! commands must fail promptly with their typed errors instead of hanging,
//! and nothing may be written to `HOME`.

use std::fs::{self, File};
use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SANDBOX: &str = "/usr/bin/sandbox-exec";
const DENY_NETWORK: &str = "(version 1)(allow default)(deny network*)";
/// Generous for a debug Render of the 4 s fixture; a hang fails the test.
const DEADLINE: Duration = Duration::from_secs(120);
/// A refused download must report promptly rather than wait out a timeout.
const DOWNLOAD_DEADLINE: Duration = Duration::from_secs(30);

struct Run {
    status: ExitStatus,
    stdout: String,
    stderr: String,
    elapsed: Duration,
}

struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    /// A private copy of `deadpan-cli`: Render binds the worker's mapped
    /// image to its path, so a concurrent rebuild of the shared target
    /// binary must not replace it mid-test.
    cli: String,
    count: usize,
}

impl Sandbox {
    fn new(root: &Path) -> Result<Self> {
        let home = root.join("empty-home");
        fs::create_dir(&home)?;
        let bin = root.join("bin");
        fs::create_dir(&bin)?;
        let cli = bin.join("deadpan-cli");
        fs::copy(env!("CARGO_BIN_EXE_deadpan-cli"), &cli)?;
        Ok(Self {
            root: root.to_path_buf(),
            home,
            cli: text(&cli).to_owned(),
            count: 0,
        })
    }

    /// Run `program arguments` under the network-denying profile with a
    /// cleared environment, output captured to files and a hard deadline.
    fn program(&mut self, program: &str, arguments: &[&str], deadline: Duration) -> Result<Run> {
        self.count += 1;
        let stdout_path = self.root.join(format!("run-{}.stdout", self.count));
        let stderr_path = self.root.join(format!("run-{}.stderr", self.count));
        let mut command = Command::new(SANDBOX);
        command
            .args(["-p", DENY_NETWORK, program])
            .args(arguments)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .env("TMPDIR", std::env::temp_dir())
            .stdin(Stdio::null())
            .stdout(File::create(&stdout_path)?)
            .stderr(File::create(&stderr_path)?)
            .process_group(0);
        if let Some(prefix) = std::env::var_os("DEADPAN_FFMPEG_PREFIX") {
            command.env("DEADPAN_FFMPEG_PREFIX", prefix);
        }
        let started = Instant::now();
        let mut child: Child = deadpan_native_process::spawn(&mut command)?;
        let pid = Pid::from_child(&child);
        let status = loop {
            if waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )?
            .is_some()
            {
                // Keep the leader unreaped while stopping the remaining
                // members of its group, including on a failing command.
                deadpan_native_process::terminate_owned_group(
                    &child,
                    Instant::now() + Duration::from_secs(2),
                )?;
                break child.wait()?;
            }
            if started.elapsed() > deadline {
                deadpan_native_process::terminate_owned_group(
                    &child,
                    Instant::now() + Duration::from_secs(2),
                )?;
                child.wait()?;
                return Err(format!(
                    "{program} {arguments:?} did not finish within {deadline:?} without network"
                )
                .into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        Ok(Run {
            status,
            stdout: fs::read_to_string(&stdout_path)?,
            stderr: fs::read_to_string(&stderr_path)?,
            elapsed: started.elapsed(),
        })
    }

    fn cli(&mut self, arguments: &[&str]) -> Result<Run> {
        let cli = self.cli.clone();
        self.program(&cli, arguments, DEADLINE)
    }

    /// The final JSON document of a successful command.
    fn success(&mut self, arguments: &[&str]) -> Result<Value> {
        let run = self.cli(arguments)?;
        if !run.status.success() {
            return Err(format!(
                "deadpan-cli {arguments:?} failed offline: {}\n{}",
                run.stderr,
                run.stdout.lines().last().unwrap_or_default()
            )
            .into());
        }
        last_json(&run.stdout)
    }

    /// A refused command: nonzero exit, its typed error code, promptly.
    fn refusal(&mut self, arguments: &[&str]) -> Result<String> {
        let cli = self.cli.clone();
        let run = self.program(&cli, arguments, DOWNLOAD_DEADLINE)?;
        assert!(
            !run.status.success(),
            "{arguments:?} succeeded without network: {}",
            run.stdout
        );
        let error = last_json(&run.stderr)?;
        let code = error["error"]["code"]
            .as_str()
            .ok_or_else(|| format!("{arguments:?} gave no typed error: {}", run.stderr))?
            .to_owned();
        assert!(
            !error["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "{error}"
        );
        assert!(run.elapsed < DOWNLOAD_DEADLINE, "{arguments:?}");
        Ok(code)
    }
}

fn last_json(text: &str) -> Result<Value> {
    if let Ok(value) = serde_json::from_str(text) {
        return Ok(value);
    }
    let line = text
        .lines()
        .rev()
        .find(|line| line.starts_with('{'))
        .ok_or_else(|| format!("no JSON in {text:?}"))?;
    Ok(serde_json::from_str(line)?)
}

fn text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

fn head(sandbox: &mut Sandbox, package: &str) -> Result<Value> {
    sandbox.success(&["project", "dump", package, "--json"])
}

#[test]
fn local_workflows_succeed_and_downloads_fail_truthfully_without_network() -> Result {
    if !Path::new(SANDBOX).is_file() {
        eprintln!("skipped: {SANDBOX} is not available on this system");
        return Ok(());
    }
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    let mut sandbox = Sandbox::new(&root)?;

    // The profile is effective: a loopback listener outside the sandbox
    // accepts connections, yet the sandboxed client cannot reach it.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port().to_string();
    let probe = sandbox.program(
        "/usr/bin/nc",
        &["-z", "-G", "2", "127.0.0.1", &port],
        DOWNLOAD_DEADLINE,
    )?;
    assert!(
        !probe.status.success(),
        "the sandbox profile did not deny a loopback TCP connection"
    );
    drop(listener);

    // A one-Original project from a committed fixture.
    let video = root.join("Original clip.mp4");
    fs::write(
        &video,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let package_path = root.join("Offline.deadpan");
    let package = text(&package_path).to_owned();
    let created = sandbox.success(&["project", "create-original", &package, text(&video)])?;
    assert_eq!(created["created"]["single_source"]["state"], "ready");

    // Edit: dry run, then insert a 15-frame pause.
    let dump = head(&mut sandbox, &package)?;
    let request = root.join("insert.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "protocol": 1,
            "project_id": dump["project_id"],
            "expected_revision": dump["revision_id"],
            "new_revision": "offline-pause",
            "command": {
                "command": "insert", "parent": dump["root"], "index": 0,
                "subtree": {"root": "pause-1", "nodes": {"pause-1": {
                    "label": "Offline pause",
                    "kind": {"type": "hold", "recipe": {
                        "duration": 15,
                        "video": {"type": "background"},
                        "audio": {"type": "silence"}
                    }}
                }}}
            }
        }))?,
    )?;
    let preview = sandbox.success(&["command", &package, "--json", text(&request), "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    let committed = sandbox.success(&["command", &package, "--json", text(&request)])?;
    assert_eq!(committed["committed"], true, "{committed}");
    assert_eq!(
        head(&mut sandbox, &package)?["revision_id"],
        "offline-pause"
    );

    // Undo and Redo through durable history.
    sandbox.success(&["project", "undo", &package, "--expected", "offline-pause"])?;
    let undone = head(&mut sandbox, &package)?;
    assert_ne!(undone["revision_id"], "offline-pause");
    let undone = undone["revision_id"].as_str().ok_or("revision")?.to_owned();
    sandbox.success(&["project", "redo", &package, "--expected", &undone])?;
    let redone = head(&mut sandbox, &package)?;
    let revision = redone["revision_id"].as_str().ok_or("revision")?.to_owned();

    // Inspection, analysis reads and maintenance.
    let valid = sandbox.success(&["project", "validate", &package])?;
    assert_eq!(valid["valid"], true, "{valid}");
    sandbox.success(&["inspect-plan", &package])?;
    let shots = sandbox.success(&["detect-shots", &package])?;
    assert_eq!(shots["pictures"], 120, "{shots}");
    sandbox.success(&["shots", &package])?;
    sandbox.success(&["transcript", &package])?;
    sandbox.success(&["pauses", &package])?;
    sandbox.success(&["doctor", "--project", &package])?;
    let diagnostic_path = root.join("diagnostic.json");
    sandbox.success(&[
        "diagnostics",
        "export",
        text(&diagnostic_path),
        "--project",
        &package,
    ])?;
    let diagnostic = fs::read_to_string(&diagnostic_path)?;
    let parsed: Value = serde_json::from_str(&diagnostic)?;
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["project"]["status"], "available");
    for private in [
        text(&root),
        text(&video),
        "Original clip",
        "Offline.deadpan",
        "Offline pause",
    ] {
        assert!(
            !diagnostic.contains(private),
            "diagnostic report contains {private:?}"
        );
    }
    sandbox.success(&["project", "backup", &package])?;
    sandbox.success(&["project", "storage", &package])?;
    let copy = root.join("Portable.deadpan");
    sandbox.success(&["project", "copy-portable", &package, text(&copy)])?;
    sandbox.success(&["project", "validate", text(&copy)])?;

    // Render and verify the published movie.
    let exports = root.join("exports");
    fs::create_dir(&exports)?;
    let finished = sandbox.success(&[
        "render",
        &package,
        "--output",
        text(&exports),
        "--name",
        "offline.mp4",
        "--expected",
        &revision,
    ])?;
    assert_eq!(finished["event"], "finished");
    assert_eq!(finished["status"]["outcome"], "published", "{finished}");
    let movie = finished["status"]["receipt"]["movie"]
        .as_str()
        .ok_or("published movie")?
        .to_owned();
    let report = fs::read_to_string(
        finished["status"]["receipt"]["report"]
            .as_str()
            .ok_or("published report")?,
    )?;
    // RENDER_PUBLICATION.md: source paths, labels and URLs are not copied
    // into report metadata, nor is the package's location.
    for private in [
        text(&root),
        text(&video),
        "Original clip",
        "Offline.deadpan",
        "Offline pause",
    ] {
        assert!(
            !report.contains(private),
            "the render report contains {private:?}"
        );
    }
    let verified = sandbox.cli(&[
        "verify-export",
        &package,
        "--movie",
        &movie,
        "--revision",
        &revision,
        "--frames",
        "0,20,40",
        "--no-audio",
    ])?;
    assert!(
        verified.status.success(),
        "verify-export offline: {}\n{}",
        verified.stdout,
        verified.stderr
    );

    // Explicit downloads fail with typed errors, promptly, and install
    // nothing.
    let helpers = root.join("helpers");
    let models = root.join("models");
    assert_eq!(
        sandbox.refusal(&["downloader", "install", "--root", text(&helpers)])?,
        "DownloaderNetworkFailed"
    );
    assert_eq!(
        sandbox.refusal(&[
            "models",
            "install",
            "whisper-base-en",
            "--root",
            text(&models),
        ])?,
        "ModelPackFailed"
    );
    assert_eq!(
        sandbox.refusal(&[
            "models",
            "update",
            "https://github.com/deadpan-updates/manifest.json",
            "--root",
            text(&models),
        ])?,
        "UpdateFetchFailed"
    );
    assert_eq!(
        sandbox.refusal(&[
            "downloader",
            "update",
            "--manifest",
            "https://github.com/deadpan-updates/manifest.json",
            "--root",
            text(&helpers),
        ])?,
        "UpdateFetchFailed"
    );
    let from_url = root.join("FromUrl.deadpan");
    assert_eq!(
        sandbox.refusal(&[
            "project",
            "create-from-url",
            text(&from_url),
            "https://youtu.be/dQw4w9WgXcQ",
            "--helpers",
            text(&helpers),
        ])?,
        "DownloaderNotInstalled"
    );
    assert!(!from_url.exists());
    let listed = sandbox.success(&["models", "list", "--root", text(&models)])?;
    let packs = listed["packs"].as_array().ok_or("packs")?;
    assert!(
        packs.iter().all(|pack| pack["installed"].is_null()),
        "{listed}"
    );
    let status = sandbox.success(&["downloader", "status", "--root", text(&helpers)])?;
    let installed = status["helpers"].as_array().ok_or("helpers")?;
    assert!(
        installed.iter().all(|helper| helper["installed"] == false),
        "{status}"
    );

    // Nothing reached HOME: no cache, preference, model or helper.
    assert_eq!(fs::read_dir(&sandbox.home)?.count(), 0);
    Ok(())
}
