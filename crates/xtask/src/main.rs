//! Deadpan developer tasks.
//!
//! `cargo xtask gate` runs the complete milestone gate from
//! [docs/DEVELOPMENT.md](../../../docs/DEVELOPMENT.md), starting with
//! build-directory hygiene. `cargo xtask hygiene` runs only that check.

use std::path::Path;
use std::process::{Command, ExitCode};

mod target_hygiene;

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        Some("gate") => gate(),
        Some("hygiene") => target_hygiene::maintain(Path::new(".")),
        _ => Err("usage: cargo xtask gate | cargo xtask hygiene".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Formatting, lints and tests for the workspace and the replay harness
/// feature set. Tests use nextest when installed, then doc tests.
fn gate() -> target_hygiene::Result<()> {
    target_hygiene::maintain(Path::new("."))?;
    run(&["fmt", "--all", "--", "--check"])?;
    run(&[
        "clippy",
        "--workspace",
        "--all-targets",
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
        run(&["test", "--workspace", "--locked", "--doc", "--no-fail-fast"])
    } else {
        run(&["test", "--workspace", "--locked", "--no-fail-fast"])?;
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
