//! Keep Cargo's build directory from silently growing until builds slow down.
//!
//! Cargo never deletes superseded artifacts. Every feature set, profile and
//! source change leaves dependency objects and incremental sessions behind. On
//! 2026-10-04, 2.87 million stale objects made an incremental `deadpan-core`
//! test build take 376 s instead of 16 s, and one day later `target/debug`
//! had regrown to 78 GiB. `cargo xtask gate`
//! measures the Cargo profile directories (those holding `.fingerprint`) and,
//! above the limit, removes the oldest artifacts with `cargo sweep --maxsize`
//! until half the limit remains, so recent builds stay warm. `cargo sweep`
//! leaves incremental state alone, which grew to 39 GiB in 1,555 per-crate
//! directories in one day, so the oldest of those are pruned the same way;
//! rustc rebuilds missing incremental state.
//! Evidence written under `target/` by other tools is never measured or touched.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Hygiene errors name the directory that could not be read.
pub type Result<T> = std::result::Result<T, String>;

/// Above this many GiB of Cargo artifacts, the gate sweeps stale ones.
const DEFAULT_LIMIT_GIB: u64 = 40;
const LIMIT_VARIABLE: &str = "DEADPAN_TARGET_LIMIT_GIB";
const GIB: u64 = 1 << 30;

/// Measure Cargo artifacts and sweep stale ones when they exceed the limit.
/// Reports what it did; never fails the gate for disk usage alone.
pub fn maintain(workspace: &Path) -> Result<()> {
    let limit = std::env::var(LIMIT_VARIABLE)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_LIMIT_GIB);
    let target = workspace.join("target");
    let before = artifact_bytes(&target)?;
    if before <= limit * GIB {
        return Ok(());
    }
    println!(
        "target hygiene: {} of Cargo artifacts exceeds {limit} GiB; removing the oldest down to {} GiB",
        gib(before),
        limit / 2
    );
    let swept = Command::new("cargo")
        .args(["sweep", "--maxsize", &format!("{}GB", limit / 2)])
        .current_dir(workspace)
        .status();
    let goal = limit / 2 * GIB;
    if !matches!(swept, Ok(status) if status.success()) {
        println!(
            "target hygiene: `cargo sweep` is unavailable (install cargo-sweep, listed in the dotfiles Brewfile) or failed; pruning incremental state only"
        );
    }
    prune_incremental(&target, goal)?;
    let after = artifact_bytes(&target)?;
    println!("target hygiene: {} -> {}", gib(before), gib(after));
    if after > limit * GIB {
        println!("target hygiene: still above {limit} GiB; run `cargo clean` when convenient");
    }
    Ok(())
}

/// Remove the least recently changed per-crate incremental directories until
/// the artifacts fit `goal` bytes.
fn prune_incremental(target: &Path, goal: u64) -> Result<()> {
    let mut total = artifact_bytes(target)?;
    let mut sessions = Vec::new();
    for profile in profile_directories(target)? {
        let Ok(entries) = std::fs::read_dir(profile.join("incremental")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            sessions.push((modified, path));
        }
    }
    sessions.sort();
    for (_, path) in sessions {
        if total <= goal {
            break;
        }
        let bytes = tree_bytes(&path)?;
        std::fs::remove_dir_all(&path)
            .map_err(|error| format!("failed to remove {}: {error}", path.display()))?;
        total = total.saturating_sub(bytes);
    }
    Ok(())
}

/// Bytes in Cargo profile directories directly under `target`, including
/// cross-compilation triples one level deeper.
fn artifact_bytes(target: &Path) -> Result<u64> {
    let mut total = 0;
    for directory in profile_directories(target)? {
        total += tree_bytes(&directory)?;
    }
    Ok(total)
}

fn profile_directories(target: &Path) -> Result<Vec<PathBuf>> {
    let mut profiles = Vec::new();
    let Ok(entries) = std::fs::read_dir(target) else {
        return Ok(profiles);
    };
    for entry in entries {
        let path = entry
            .map_err(|error| format!("failed to read {}: {error}", target.display()))?
            .path();
        if !path.is_dir() {
            continue;
        }
        if path.join(".fingerprint").is_dir() {
            profiles.push(path);
            continue;
        }
        // Target triples nest profiles: target/<triple>/<profile>.
        if let Ok(nested) = std::fs::read_dir(&path) {
            for entry in nested.flatten() {
                let nested = entry.path();
                if nested.join(".fingerprint").is_dir() {
                    profiles.push(nested);
                }
            }
        }
    }
    Ok(profiles)
}

fn tree_bytes(root: &Path) -> Result<u64> {
    let mut total = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let read =
            |error: std::io::Error| format!("failed to read {}: {error}", directory.display());
        for entry in std::fs::read_dir(&directory).map_err(read)? {
            let entry = entry.map_err(read)?;
            let kind = entry.file_type().map_err(read)?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                total += entry.metadata().map_err(read)?.len();
            }
        }
    }
    Ok(total)
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / GIB as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_cargo_profile_directories_are_measured() -> std::io::Result<()> {
        let root = std::env::temp_dir().join(format!("deadpan-hygiene-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let target = Scratch(root);
        let debug = target.path().join("debug");
        std::fs::create_dir_all(debug.join(".fingerprint"))?;
        std::fs::create_dir_all(debug.join("deps"))?;
        std::fs::write(debug.join("deps/libcore.rlib"), vec![0; 1000])?;
        let triple = target.path().join("aarch64-apple-darwin/release");
        std::fs::create_dir_all(triple.join(".fingerprint"))?;
        std::fs::write(triple.join("app"), vec![0; 500])?;
        // Evidence from other tools lives beside the profiles.
        std::fs::create_dir_all(target.path().join("feel/run"))?;
        std::fs::write(target.path().join("feel/run/capture.png"), vec![0; 4000])?;
        assert_eq!(artifact_bytes(target.path()), Ok(1500));
        assert_eq!(artifact_bytes(&target.path().join("missing")), Ok(0));
        Ok(())
    }

    #[test]
    fn the_oldest_incremental_state_is_pruned_first() -> std::io::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "{}-prune-{}",
            env!("CARGO_PKG_NAME"),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let target = Scratch(root);
        let incremental = target.path().join("debug/incremental");
        std::fs::create_dir_all(target.path().join("debug/.fingerprint"))?;
        let now = std::time::SystemTime::now();
        for (name, age) in [("old-1", 60), ("new-2", 0)] {
            let session = incremental.join(name);
            std::fs::create_dir_all(&session)?;
            std::fs::write(session.join("query-cache.bin"), vec![0; 1000])?;
            std::fs::File::open(&session)?
                .set_modified(now - std::time::Duration::from_secs(age))?;
        }
        assert_eq!(prune_incremental(target.path(), 1500), Ok(()));
        assert!(!incremental.join("old-1").exists());
        assert!(incremental.join("new-2").exists());
        assert_eq!(artifact_bytes(target.path()), Ok(1000));
        Ok(())
    }

    /// Removes its directory when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
