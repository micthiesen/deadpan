//! `cargo xtask install [--dest DIR] [bundle options]`: build the release
//! bundle and put a launchable `Deadpan.app` in `/Applications` (or `DIR`).
//!
//! The bundle is built by this binary's own `bundle` task into a private
//! staging directory, copied beside the destination, and swapped in by two
//! renames so a failed copy never leaves the previous app half replaced. The
//! installed app must then pass `--smoke-test` and report this checkout's
//! HEAD commit from `--version`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, String>;

const USAGE: &str = "usage: cargo xtask install [--dest DIR] [--no-build] [--identity NAME] [--allow-dirty] [--without-ai-runtime] [--helpers-from ROOT]";

pub fn run(arguments: &[String]) -> Result<()> {
    let (dest, bundle_arguments) = parse(arguments)?;
    if !dest.is_dir() {
        return Err(format!("destination {} is not a directory", dest.display()));
    }
    let head = git_head()?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    let developer = home.join("Library/Developer/Deadpan");
    let ffmpeg = developer.join("ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix");
    let ort = developer.join("onnxruntime-1.31.0-qualified-1/prefix");

    let staging = std::env::temp_dir().join(format!("deadpan-install-{}", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    println!(
        "install: building the bundle for {head} in {}",
        staging.display()
    );
    let mut build = Command::new(&me);
    build
        .arg("bundle")
        .arg("--output")
        .arg(&staging)
        .args(&bundle_arguments);
    if std::env::var_os("DEADPAN_FFMPEG_PREFIX").is_none() {
        build.env("DEADPAN_FFMPEG_PREFIX", &ffmpeg);
    }
    if std::env::var_os("DEADPAN_ORT_PREFIX").is_none() {
        build.env("DEADPAN_ORT_PREFIX", &ort);
    }
    let status = build.status().map_err(|e| format!("start bundle: {e}"))?;
    if !status.success() {
        return Err(format!("bundle failed ({status})"));
    }

    let built = staging.join("Deadpan.app");
    let target = dest.join("Deadpan.app");
    swap_in(&built, &target)?;
    fs::remove_dir_all(&staging).map_err(|e| format!("remove staging: {e}"))?;

    let executable = target.join("Contents/MacOS/deadpan-app");
    let smoke = Command::new(&executable)
        .arg("--smoke-test")
        .output()
        .map_err(|e| format!("start {}: {e}", executable.display()))?;
    if !smoke.status.success() {
        return Err(format!(
            "installed app --smoke-test failed ({}): {}{}",
            smoke.status,
            String::from_utf8_lossy(&smoke.stdout).trim(),
            String::from_utf8_lossy(&smoke.stderr).trim()
        ));
    }
    println!("install: smoke test passed");
    let version = Command::new(&executable)
        .arg("--version")
        .output()
        .map_err(|e| format!("start {}: {e}", executable.display()))?;
    let line = String::from_utf8_lossy(&version.stdout).trim().to_owned();
    if !version.status.success() || !line.contains(&head) {
        return Err(format!(
            "installed app reports {line:?}, expected a version line naming {head}"
        ));
    }
    println!("install: {line}");
    println!("install: {}", target.display());
    Ok(())
}

fn parse(arguments: &[String]) -> Result<(PathBuf, Vec<OsString>)> {
    let mut dest = PathBuf::from("/Applications");
    let mut pass = Vec::new();
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--dest" => {
                dest = arguments
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| format!("missing value for --dest; {USAGE}"))?;
            }
            "--no-build" | "--allow-dirty" | "--without-ai-runtime" => pass.push(argument.into()),
            "--identity" | "--helpers-from" => {
                pass.push(argument.into());
                pass.push(
                    arguments
                        .next()
                        .ok_or_else(|| format!("missing value for {argument}; {USAGE}"))?
                        .into(),
                );
            }
            _ => return Err(format!("unknown option {argument}; {USAGE}")),
        }
    }
    if !dest.is_absolute() {
        return Err("--dest must be an absolute directory".into());
    }
    Ok((dest, pass))
}

fn git_head() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err("git rev-parse HEAD failed".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Replace `target` with `built` through renames beside it, restoring the old
/// app if the final rename fails.
fn swap_in(built: &Path, target: &Path) -> Result<()> {
    let parent = target.parent().ok_or("destination has no parent")?;
    let incoming = parent.join(".Deadpan.app.new");
    let outgoing = parent.join(".Deadpan.app.old");
    for leftover in [&incoming, &outgoing] {
        if leftover.exists() {
            fs::remove_dir_all(leftover).map_err(|e| format!("{}: {e}", leftover.display()))?;
        }
    }
    let copy = Command::new("ditto")
        .arg(built)
        .arg(&incoming)
        .output()
        .map_err(|e| format!("ditto: {e}"))?;
    if !copy.status.success() {
        let _ = fs::remove_dir_all(&incoming);
        return Err(format!(
            "ditto into {} failed: {}",
            incoming.display(),
            String::from_utf8_lossy(&copy.stderr).trim()
        ));
    }
    let replaced = target.exists();
    if replaced {
        fs::rename(target, &outgoing).map_err(|e| format!("move the old app aside: {e}"))?;
    }
    if let Err(error) = fs::rename(&incoming, target) {
        if replaced {
            let _ = fs::rename(&outgoing, target);
        }
        return Err(format!("install the new app: {error}"));
    }
    if replaced {
        fs::remove_dir_all(&outgoing).map_err(|e| format!("remove the old app: {e}"))?;
    }
    Ok(())
}
