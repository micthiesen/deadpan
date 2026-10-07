//! Deadpan developer tasks.
//!
//! `cargo xtask gate` runs the complete milestone gate from
//! [docs/DEVELOPMENT.md](../../../docs/DEVELOPMENT.md), starting with
//! build-directory hygiene. `cargo xtask hygiene` runs only that check.
//! `cargo xtask bundle` assembles a relocatable `Deadpan.app`;
//! `bundle-audit` and `bundle-verify` check one (docs/PACKAGING.md).
//! `cargo xtask offline-dist` builds a full offline distribution with model
//! packs from such a bundle; `offline-dist-verify` checks and imports it.
//! `cargo xtask replays` builds the ui-harness app once and replays every
//! UI scenario in its own report directory (docs/UI_FEEDBACK.md).
//! `cargo xtask perf` runs the release Section 25 benchmark suite on copied
//! fixtures (docs/PERFORMANCE.md).
//! `cargo xtask chaos` runs the time-bounded Gate G adversarial campaign
//! (docs/ADVERSARIAL.md). `cargo xtask fuzz` runs the coverage-guided
//! libFuzzer targets in `fuzz/` with the nightly toolchain.

use std::path::Path;
use std::process::{Command, ExitCode};

mod bundle;
mod chaos;
mod fuzz;
mod offline_dist;
mod percentile;
mod perf;
mod replays;
mod target_hygiene;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (task, rest) = arguments
        .split_first()
        .map_or((None, &[][..]), |(task, rest)| (Some(task.as_str()), rest));
    let result = match task {
        Some("gate") => gate(),
        Some("hygiene") => target_hygiene::maintain(Path::new(".")),
        Some("bundle") => bundle::run(rest),
        Some("bundle-audit") => bundle::audit_command(rest),
        Some("bundle-verify") => bundle::verify::run(rest),
        Some("offline-dist") => offline_dist::run(rest),
        Some("offline-dist-verify") => offline_dist::verify_command(rest),
        Some("replays") => replays::run(rest),
        Some("perf") => perf::run(rest),
        Some("chaos") => chaos::run(rest),
        Some("fuzz") => fuzz::run(rest),
        _ => Err("usage: cargo xtask gate | hygiene | replays [--scenario a,b] [--output <new dir>] [--jobs N] | perf [--fixture NAME=PACKAGE]... [--generate] [--ui] [--output <new dir>] | chaos [--minutes N] [--seed HEX] [--only a,b] [--output <new dir>] [--sanitize] [--list] | fuzz list | seeds | replay | run [--minutes N] [--jobs N] [--only a,b] | bundle --output <dir> | bundle-audit <Deadpan.app> | bundle-verify <Deadpan.app> | offline-dist --app <Deadpan.app> --output <new dir> --pack <id>[=<models root>]... | offline-dist-verify <dist dir>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Enables `deadpan-cli` test targets that declare `required-features`
/// (the synthetic AI worker). The `deadpan-app` dev-dependency already unifies
/// this feature into workspace builds, so naming it changes no build graph; it
/// only keeps those targets from being skipped if that unification goes away.
const SYNTHETIC_WORKER: &str = "deadpan-cli/synthetic-worker";

/// Formatting, lints and tests for the workspace and the replay harness
/// feature set. Tests use nextest when installed, then doc tests.
fn gate() -> target_hygiene::Result<()> {
    target_hygiene::maintain(Path::new("."))?;
    run(&["fmt", "--all", "--", "--check"])?;
    run(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--features",
        SYNTHETIC_WORKER,
        "--locked",
        "--",
        "-D",
        "warnings",
    ])?;
    run(&[
        "clippy",
        "-p",
        "deadpan-app",
        "--all-targets",
        "--features",
        "ui-harness",
        "--locked",
        "--",
        "-D",
        "warnings",
    ])?;
    if nextest_available() {
        run(&[
            "nextest",
            "run",
            "--workspace",
            "--features",
            SYNTHETIC_WORKER,
            "--locked",
            "--no-fail-fast",
        ])?;
        run(&[
            "nextest",
            "run",
            "-p",
            "deadpan-app",
            "--features",
            "ui-harness",
            "--locked",
            "--no-fail-fast",
        ])?;
        run(&[
            "test",
            "--workspace",
            "--features",
            SYNTHETIC_WORKER,
            "--locked",
            "--doc",
            "--no-fail-fast",
        ])
    } else {
        run(&[
            "test",
            "--workspace",
            "--features",
            SYNTHETIC_WORKER,
            "--locked",
            "--no-fail-fast",
        ])?;
        run(&[
            "test",
            "-p",
            "deadpan-app",
            "--features",
            "ui-harness",
            "--locked",
            "--no-fail-fast",
        ])
    }
}

fn nextest_available() -> bool {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn run(arguments: &[&str]) -> target_hygiene::Result<()> {
    let status = Command::new("cargo")
        .args(arguments)
        // Synthetic-worker tests fail instead of skipping when their ffmpeg
        // or deadpan-media-worker is missing (crates/deadpan-cli/tests/generated_joins.rs).
        .env("DEADPAN_REQUIRE_SYNTHETIC_WORKER", "1")
        .status()
        .map_err(|error| format!("failed to start cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "cargo {} exited with {status}",
            arguments.join(" ")
        ))
    }
}
