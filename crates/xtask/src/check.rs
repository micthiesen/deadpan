//! `cargo xtask check`: the fast tier. Format, lint and test only the
//! workspace packages whose files changed since a base revision (default
//! `origin/main`, including uncommitted and untracked files). Dependents,
//! replays and preview/export runs belong to the milestone gate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::target_hygiene::Result;

struct Options {
    base: String,
    tests: bool,
    filter: Option<String>,
}

fn options(arguments: &[String]) -> Result<Options> {
    let mut options = Options {
        base: "origin/main".into(),
        tests: true,
        filter: None,
    };
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--since" => {
                options.base = arguments.next().ok_or("--since needs a revision")?.clone();
            }
            "--no-test" => options.tests = false,
            "-E" | "--filter" => {
                options.filter = Some(
                    arguments
                        .next()
                        .ok_or("--filter needs a nextest filter expression")?
                        .clone(),
                );
            }
            other => {
                return Err(format!(
                    "unknown check argument {other}; usage: cargo xtask check [--since REV] [--no-test] [--filter EXPR]"
                ));
            }
        }
    }
    Ok(options)
}

fn git_lines(arguments: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(arguments)
        .output()
        .map_err(|error| format!("failed to start git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Workspace packages as (name, manifest directory relative to the root).
fn packages(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .output()
        .map_err(|error| format!("failed to start cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err("cargo metadata failed".into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("cargo metadata output: {error}"))?;
    let mut packages = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let (Some(name), Some(manifest)) =
            (package["name"].as_str(), package["manifest_path"].as_str())
        else {
            continue;
        };
        let directory = Path::new(manifest)
            .parent()
            .and_then(|directory| directory.strip_prefix(root).ok())
            .map(Path::to_path_buf);
        if let Some(directory) = directory {
            packages.push((name.to_owned(), directory));
        }
    }
    // Longest directory first, so a nested package wins over its parent.
    packages.sort_by_key(|(_, directory)| std::cmp::Reverse(directory.as_os_str().len()));
    Ok(packages)
}

pub fn run(arguments: &[String]) -> Result<()> {
    let options = options(arguments)?;
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let mut changed: BTreeSet<String> = git_lines(&["diff", "--name-only", &options.base])?
        .into_iter()
        .collect();
    changed.extend(git_lines(&["ls-files", "--others", "--exclude-standard"])?);
    let packages = packages(&root)?;
    let mut selected = BTreeSet::new();
    let mut rust_files = Vec::new();
    for file in &changed {
        let path = Path::new(file);
        if !root.join(path).exists() {
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "rs") {
            rust_files.push(file.clone());
        }
        if let Some((name, _)) = packages
            .iter()
            .find(|(_, directory)| !directory.as_os_str().is_empty() && path.starts_with(directory))
        {
            selected.insert(name.clone());
        }
    }
    if selected.is_empty() {
        println!("check: no workspace package changed since {}", options.base);
        return Ok(());
    }
    println!(
        "check: {} changed since {}: {}",
        if selected.len() == 1 {
            "package"
        } else {
            "packages"
        },
        options.base,
        selected.iter().cloned().collect::<Vec<_>>().join(", ")
    );
    if !rust_files.is_empty() {
        let status = Command::new("rustfmt")
            .args(["--edition", "2024", "--check"])
            .args(&rust_files)
            .status()
            .map_err(|error| format!("failed to start rustfmt: {error}"))?;
        if !status.success() {
            return Err("rustfmt --check failed on changed files".into());
        }
    }
    let mut package_arguments = Vec::new();
    for name in &selected {
        package_arguments.push("-p");
        package_arguments.push(name.as_str());
    }
    let cli = selected.contains("deadpan-cli");
    let mut clippy = vec!["clippy"];
    clippy.extend(&package_arguments);
    clippy.extend(["--all-targets", "--locked"]);
    if cli {
        clippy.extend(["--features", super::SYNTHETIC_WORKER]);
    }
    clippy.extend(["--", "-D", "warnings"]);
    super::run(&clippy)?;
    if selected.contains("deadpan-app") {
        super::run(&[
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
    }
    if !options.tests {
        return Ok(());
    }
    if !super::nextest_available() {
        return Err(
            "cargo xtask check runs tests through cargo-nextest; install it or use --no-test"
                .into(),
        );
    }
    let mut tests = vec!["nextest", "run"];
    tests.extend(&package_arguments);
    tests.extend(["--locked", "--no-fail-fast"]);
    if cli {
        tests.extend(["--features", super::SYNTHETIC_WORKER]);
    }
    if let Some(filter) = &options.filter {
        tests.extend(["-E", filter.as_str()]);
    }
    super::run(&tests)
}
