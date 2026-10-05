//! Deadpan developer tasks.
//!
//! `cargo xtask gate` runs the complete milestone gate from
//! [docs/DEVELOPMENT.md](../../../docs/DEVELOPMENT.md), starting with
//! build-directory hygiene. `cargo xtask hygiene` runs only that check.
//! `cargo xtask bundle` assembles a relocatable `Deadpan.app`;
//! `bundle-audit` and `bundle-verify` check one (docs/PACKAGING.md).

use std::path::Path;
use std::process::{Command, ExitCode};

mod bundle;
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
        _ => Err("usage: cargo xtask gate | hygiene | bundle --output <dir> | bundle-audit <Deadpan.app> | bundle-verify <Deadpan.app>".into()),
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
