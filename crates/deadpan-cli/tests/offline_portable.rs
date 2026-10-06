#![cfg(target_os = "macos")]

//! Accepted AI pauses render without any model, and a portable copy renders
//! from another path after its source package is gone (DP-19).
//!
//! The fixture is the `black_pause` recipe with its 12-frame pause filled by
//! two synthetic Ready variants (the real conditioning, qualification,
//! publication and acceptance code; only the model is replaced). The first
//! variant is accepted, the second discarded. Every render runs with `HOME`
//! pointing at an empty directory, so no model pack, qualification weight,
//! runtime, proxy or helper is reachable, with every `DEADPAN_BRIDGE_*`
//! variable pointing nowhere and outbound IP networking denied by the
//! sandbox.

#[allow(dead_code)]
#[path = "preview_export/recipes.rs"]
mod recipes;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::AtomicBool;

use deadpan_cli::generation::acceptance;
use deadpan_cli::generation::attempt::{self, AllocateInput, synthetic};
use deadpan_cli::generation::conditioning;
use deadpan_core::{NodeId, RevisionId};
use deadpan_jobs::JobState;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::Value;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn synthetic_tools() -> Option<synthetic::SyntheticWorker> {
    let ffmpeg = std::env::var_os("DEADPAN_BRIDGE_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    let media_worker = std::env::var_os("DEADPAN_MEDIA_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/deadpan-media-worker")
        });
    (ffmpeg.is_file() && media_worker.is_file()).then_some(synthetic::SyntheticWorker {
        ffmpeg,
        media_worker,
    })
}

/// Run one variant through the synthetic worker and record its outcome.
fn run_variant(
    store: &mut ProjectStore,
    allocated: &attempt::Allocated,
    worker: &synthetic::SyntheticWorker,
) -> Result<attempt::Finished> {
    let run = synthetic::run(
        allocated,
        worker,
        |_| {},
        |record| attempt::record(store, allocated, &record).map_err(|error| error.to_string()),
        &AtomicBool::new(false),
    );
    Ok(attempt::finish(store, allocated, run)?)
}

/// `deadpan-cli` with no reachable model, runtime or network.
fn offline(arguments: &[&str], home: &Path) -> Result<Output> {
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command
        .args([
            "-p",
            "(version 1)(allow default)(deny network-outbound (remote ip))",
            recipes::cli_path(),
        ])
        .args(arguments)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", std::env::temp_dir())
        .env("DEADPAN_BRIDGE_RUNTIME_SOURCE", "/nonexistent/runtime")
        .env("DEADPAN_BRIDGE_PYTHON", "/nonexistent/python")
        .env("DEADPAN_BRIDGE_WORKER", "/nonexistent/worker.py")
        .env("DEADPAN_BRIDGE_MODEL_ROOT", "/nonexistent/models")
        .env("DEADPAN_BRIDGE_FFMPEG", "/nonexistent/ffmpeg")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(prefix) = std::env::var_os("DEADPAN_FFMPEG_PREFIX") {
        command.env("DEADPAN_FFMPEG_PREFIX", prefix);
    }
    Ok(deadpan_native_process::spawn(&mut command)?.wait_with_output()?)
}

fn json(output: &Output, what: &str) -> Result<Value> {
    let stdout = String::from_utf8(output.stdout.clone())?;
    if !output.status.success() {
        return Err(format!(
            "{what} failed: {}\n{}",
            String::from_utf8_lossy(&output.stderr),
            stdout.lines().last().unwrap_or_default()
        )
        .into());
    }
    Ok(serde_json::from_str(
        stdout
            .lines()
            .last()
            .filter(|line| line.starts_with('{'))
            .map_or(stdout.as_str(), |line| line),
    )?)
}

fn render(package: &Path, revision: &str, output: &Path, home: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output)?;
    let result = offline(
        &[
            "render",
            package.to_str().ok_or("UTF-8")?,
            "--output",
            output.to_str().ok_or("UTF-8")?,
            "--name",
            "pause.mp4",
            "--expected",
            revision,
        ],
        home,
    )?;
    let finished = json(&result, "render")?;
    assert_eq!(finished["event"], "finished");
    assert_eq!(finished["status"]["outcome"], "published");
    Ok(PathBuf::from(
        finished["status"]["receipt"]["movie"]
            .as_str()
            .ok_or("published movie")?,
    ))
}

/// The movie matches its committed preview, including generated pictures.
fn verify(package: &Path, movie: &Path, revision: &str, home: &Path) -> Result {
    let result = offline(
        &[
            "verify-export",
            package.to_str().ok_or("UTF-8")?,
            "--movie",
            movie.to_str().ok_or("UTF-8")?,
            "--revision",
            revision,
            "--frames",
            "14,15,20,26,27",
            "--no-audio",
        ],
        home,
    )?;
    let stdout = String::from_utf8(result.stdout.clone())?;
    let report: Value = serde_json::from_str(&stdout)?;
    assert!(
        result.status.success(),
        "verify-export: {}\n{}",
        report["failures"],
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

#[test]
fn accepted_ai_pause_renders_offline_and_from_a_portable_copy() -> Result {
    let Some(worker) = synthetic_tools() else {
        eprintln!("skipped: needs ffmpeg with libx264rgb and a built deadpan-media-worker");
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let root = root.path().canonicalize()?;
    let empty_home = root.join("empty-home");
    std::fs::create_dir(&empty_home)?;
    let fixture = recipes::black_pause(&root.join("fixture"))?;
    let package = fixture.package.clone();
    let hold = NodeId::new("black")?;

    // Two variants of one request through real conditioning and
    // qualification; accept the first, discard the second.
    let cancelled = AtomicBool::new(false);
    let revision = RevisionId::new(fixture.revision.clone())?;
    let inputs = conditioning::prepare(&package, &revision, &hold, &cancelled)?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let first = attempt::allocate(
        &mut store,
        AllocateInput {
            hold: hold.clone(),
            expected_revision: revision.clone(),
            seed: 11,
            inputs,
        },
    )?;
    let finished = run_variant(&mut store, &first, &worker)?;
    assert_eq!(finished.state, JobState::Ready, "{:?}", finished.failure);
    let conditioned = finished
        .receipt
        .ok_or("first receipt")?
        .admission()
        .ok_or("admission evidence")?
        .inputs()
        .clone();
    let second =
        attempt::allocate_variant(&mut store, first.request.clone(), first.inputs().clone())?;
    let finished = run_variant(&mut store, &second, &worker)?;
    assert_eq!(finished.state, JobState::Ready, "{:?}", finished.failure);
    let discarded = finished.receipt.ok_or("second receipt")?;
    store.select_generation_bundle_variant(&first.identity)?;
    let accepted = RevisionId::new("accepted-ai-pause")?;
    acceptance::accept(&mut store, &first.request.request_id, accepted.clone())?;
    store.discard_generation_bundle_variant(&second.identity)?;
    drop(store);

    // The discarded variant's three masters are the only unreferenced media.
    let path = package.to_str().ok_or("UTF-8")?;
    let report = json(
        &offline(
            &["project", "storage", path, "--grace-hours", "0"],
            &empty_home,
        )?,
        "storage",
    )?;
    let generated = report["project"]["namespaces"]
        .as_array()
        .ok_or("namespaces")?
        .iter()
        .find(|namespace| namespace["namespace"] == "generated")
        .ok_or("generated namespace")?;
    let unreferenced: Vec<&str> = generated["entries"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .filter(|entry| entry["state"] == "unreferenced")
        .filter_map(|entry| entry["digest"].as_str())
        .collect();
    let mut expected = vec![
        discarded.native_object().content().digest(),
        discarded.sampled_object().content().digest(),
        discarded.provenance_object().content().digest(),
    ];
    expected.sort_unstable();
    let mut found = unreferenced.clone();
    found.sort_unstable();
    assert_eq!(found, expected);
    let cleanup = json(
        &offline(
            &["project", "storage", path, "--clean", "--grace-hours", "0"],
            &empty_home,
        )?,
        "cleanup",
    )?;
    assert_eq!(
        cleanup["cleanup"]["removed"].as_array().map(Vec::len),
        Some(3)
    );

    // Render with no model anywhere, after cleanup.
    let movie = render(
        &package,
        accepted.as_str(),
        &root.join("exports"),
        &empty_home,
    )?;
    verify(&package, &movie, accepted.as_str(), &empty_home)?;

    // A portable copy elsewhere renders after the source package is gone.
    let elsewhere = root.join("elsewhere");
    std::fs::create_dir(&elsewhere)?;
    let copy = elsewhere.join("Moved.deadpan");
    let copied = json(
        &offline(
            &[
                "project",
                "copy-portable",
                path,
                copy.to_str().ok_or("UTF-8")?,
            ],
            &empty_home,
        )?,
        "copy-portable",
    )?;
    assert_eq!(
        copied["portable_copy"]["revision_id"],
        accepted.as_str(),
        "{copied}"
    );
    assert_eq!(copied["portable_copy"]["originals"][0]["from"], "managed");
    std::fs::remove_dir_all(root.join("fixture"))?;
    let copy_path = copy.to_str().ok_or("UTF-8")?;
    json(
        &offline(&["project", "validate", copy_path], &empty_home)?,
        "validate copy",
    )?;
    let movie = render(
        &copy,
        accepted.as_str(),
        &root.join("exports-copy"),
        &empty_home,
    )?;
    verify(&copy, &movie, accepted.as_str(), &empty_home)?;

    // Source-clock evidence: the copy alone re-derives the exact retained
    // conditioning. Its boundary pictures, decoded from the request's origin
    // revision through the shared picture path, reproduce the manifest whose
    // SHA-256 the request binds, and the retained manifest object is byte
    // identical to it.
    let rederived = conditioning::prepare(&copy, &revision, &hold, &cancelled)?;
    assert_eq!(
        rederived.manifest_sha256,
        first.request.binding.context_sha256
    );
    let store = ProjectStore::open(&copy, AccessMode::ReadOnly)?;
    let mut retained = Vec::new();
    std::io::Read::read_to_end(
        &mut store.snapshot_generated_object(
            conditioned.manifest(),
            deadpan_cli::generation::attempt::object_limits(),
        )?,
        &mut retained,
    )?;
    assert_eq!(retained, rederived.manifest);
    drop(store);
    // Nothing reached the empty home: no model, cache or helper was created.
    assert_eq!(std::fs::read_dir(&empty_home)?.count(), 0);
    Ok(())
}
