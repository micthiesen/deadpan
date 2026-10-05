//! Identify the exact history validator compiled into this store.
//!
//! A history receipt records which validator proved a stored chronology. It
//! is accepted only by a build whose validating code is byte-identical: every
//! workspace path crate the store depends on, directly or transitively
//! (derived from `Cargo.lock`, never hand-listed), the locked registry graph
//! and the pinned toolchain. A change anywhere in that code invalidates every
//! receipt, so changed command or validation semantics never inherit an older
//! proof. Unreadable inputs fail the build rather than hashing as empty.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const ROOT_PACKAGE: &str = "deadpan-store";

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| {
        panic!(
            "history validator input {} is unreadable: {error}",
            path.display()
        )
    })
}

fn text(path: &Path) -> String {
    String::from_utf8(read(path))
        .unwrap_or_else(|_| panic!("history validator input {} is not UTF-8", path.display()))
}

/// Path (workspace) packages and every package's dependency names.
fn lock_graph(lock: &str) -> (BTreeSet<String>, BTreeMap<String, Vec<String>>) {
    let mut local = BTreeSet::new();
    let mut graph = BTreeMap::new();
    for block in lock.split("[[package]]").skip(1) {
        let mut name = None;
        let mut source = false;
        let mut dependencies = Vec::new();
        let mut in_dependencies = false;
        for line in block.lines().map(str::trim) {
            if in_dependencies {
                if line == "]" {
                    in_dependencies = false;
                } else if let Some(entry) = line.trim_end_matches(',').strip_prefix('"') {
                    let entry = entry.trim_end_matches('"');
                    let dependency = entry.split(' ').next().unwrap_or(entry);
                    dependencies.push(dependency.to_owned());
                }
            } else if let Some(value) = line.strip_prefix("name = ") {
                name = Some(value.trim_matches('"').to_owned());
            } else if line.starts_with("source = ") {
                source = true;
            } else if line == "dependencies = [" {
                in_dependencies = true;
            }
        }
        let name = name.expect("Cargo.lock package without a name");
        if !source {
            local.insert(name.clone());
        }
        graph.insert(name, dependencies);
    }
    (local, graph)
}

/// Workspace member directories by package name.
fn members(workspace: &Path) -> BTreeMap<String, PathBuf> {
    let manifest = text(&workspace.join("Cargo.toml"));
    let line = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("members = ["))
        .expect("workspace members list");
    let list = &line
        [line.find('[').expect("members list") + 1..line.rfind(']').expect("members list end")];
    let mut directories = BTreeMap::new();
    for member in list.split(',').map(|entry| entry.trim().trim_matches('"')) {
        if member.is_empty() {
            continue;
        }
        let directory = workspace.join(member);
        let name = text(&directory.join("Cargo.toml"))
            .lines()
            .find_map(|line| line.trim().strip_prefix("name = ").map(str::to_owned))
            .expect("member package name")
            .trim_matches('"')
            .to_owned();
        directories.insert(name, directory);
    }
    directories
}

/// Every file that can affect a crate's compiled code. Tests, benches,
/// examples and hidden or build-output directories are excluded.
fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory).unwrap_or_else(|error| {
        panic!(
            "history validator input {} is unreadable: {error}",
            directory.display()
        )
    });
    for entry in entries {
        let entry = entry.expect("history validator directory entry");
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let kind = entry.file_type().expect("history validator file type");
        if kind.is_dir() {
            if !matches!(&*name, "target" | "tests" | "benches" | "examples") {
                collect(&path, files);
            }
        } else if kind.is_file() {
            files.push(path);
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace = manifest
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let lock_path = workspace.join("Cargo.lock");
    let (local, graph) = lock_graph(&text(&lock_path));
    let directories = members(&workspace);

    // Every path package reachable from the store, including dev-dependencies
    // listed in the lockfile: a superset is conservative.
    let mut reached = BTreeSet::new();
    let mut pending = vec![ROOT_PACKAGE.to_owned()];
    while let Some(package) = pending.pop() {
        if !local.contains(&package) || !reached.insert(package.clone()) {
            continue;
        }
        pending.extend(graph.get(&package).cloned().unwrap_or_default());
    }

    let mut inputs = vec![
        lock_path,
        workspace.join("rust-toolchain.toml"),
        workspace.join("Cargo.toml"),
    ];
    for package in &reached {
        let directory = directories
            .get(package)
            .unwrap_or_else(|| panic!("path package {package} is not a workspace member"));
        println!("cargo:rerun-if-changed={}", directory.display());
        collect(directory, &mut inputs);
    }
    inputs.sort();
    inputs.dedup();
    let mut hasher = Sha256::new();
    hasher.update(b"deadpan-history-validator-v2");
    for file in &inputs {
        println!("cargo:rerun-if-changed={}", file.display());
        let relative = file.strip_prefix(&workspace).unwrap_or(file);
        let name = relative.to_string_lossy();
        let contents = read(file);
        hasher.update((name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update((contents.len() as u64).to_le_bytes());
        hasher.update(&contents);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("cargo:rustc-env=DEADPAN_HISTORY_VALIDATOR={digest}");
    println!(
        "cargo:rustc-env=DEADPAN_HISTORY_VALIDATOR_CRATES={}",
        reached.into_iter().collect::<Vec<_>>().join(",")
    );
}
