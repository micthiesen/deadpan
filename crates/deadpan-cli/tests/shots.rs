#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Shot detection over every picture of a registered source, and its commands.

use std::{
    error::Error,
    fs,
    path::Path,
    process::{Command, Output},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_analysis::SIGNATURE_VERSION;
use deadpan_cli::shots::{ShotScanError, prepare_shot_input, scan_shots, stored_shots};
use deadpan_core::AssetId;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()?)
}

fn success(arguments: &[&str]) -> Result<Value> {
    let output = cli(arguments)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// A generic project with the 120-picture 320×180 fixture registered as
/// asset `clip`.
fn registered(scratch: &Path) -> Result<std::path::PathBuf> {
    let package = scratch.join("shots.deadpan");
    success(&[
        "project",
        "create",
        package.to_str().unwrap(),
        "--fps",
        "30000/1001",
        "--size",
        "320x180",
    ])?;
    let source = scratch.join("source.mp4");
    fs::write(
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let retained = success(&[
        "project",
        "retain-original",
        package.to_str().unwrap(),
        source.to_str().unwrap(),
    ])?;
    let original = retained["retained_original"]["record"]["object"]["content"].clone();
    let snapshot = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let request = json!({
        "protocol": 1,
        "registration": {
            "expected_revision": snapshot.revision_id(), "new_revision": "registered",
            "original": original, "new_asset_id": "clip", "label": "Clip",
            "insertion": {"parent": snapshot.root(), "index": 0, "node": "clip-source", "label": "Clip"}
        },
        "streams": {"type":"video_only"}
    });
    let path = scratch.join("request.json");
    fs::write(&path, serde_json::to_vec(&request)?)?;
    success(&[
        "project",
        "register-source",
        package.to_str().unwrap(),
        "--request-json",
        path.to_str().unwrap(),
    ])?;
    Ok(package)
}

#[test]
fn every_picture_is_measured_in_index_order_and_stored_outside_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = registered(scratch.path())?;
    let head = ProjectStore::open(&package, AccessMode::ReadOnly)?.head_revision()?;
    let path = package.to_str().unwrap();

    let detected = success(&["detect-shots", path, "--asset", "clip"])?;
    assert_eq!(detected["rule"], "deadpan-shots-1");
    assert_eq!(detected["pictures"], 120);
    assert_eq!(detected["key"]["signature_version"], SIGNATURE_VERSION);
    assert_eq!(detected["key"]["video_stream"], 0);
    assert!(detected["pictures_per_second"].as_f64().unwrap() > 0.0);
    let boundaries = detected["boundaries"].as_array().unwrap().clone();
    assert_eq!(
        detected["shots"].as_u64().unwrap(),
        boundaries.len() as u64 + 1
    );

    let shots = success(&["shots", path, "--asset", "clip"])?;
    assert_eq!(shots["analysis"]["pictures"], 120);
    assert_eq!(shots["analysis"]["key"], detected["key"]);
    let listed: Vec<_> = shots["boundaries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|boundary| boundary["picture"].clone())
        .collect();
    assert_eq!(listed, boundaries);

    // Detection is an annotation: no revision, and running again replaces it.
    success(&["detect-shots", path, "--asset", "clip"])?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(store.head_revision()?, head);
    let content = detected["key"]["content"].as_str().unwrap();
    assert_eq!(store.shot_analysis_keys_for_content(content)?.len(), 1);
    let (key, analysis) = stored_shots(&store, content, 0, 120).unwrap();
    assert_eq!(key.signature_version, SIGNATURE_VERSION);
    assert_eq!(analysis.pictures(), 120);
    assert_eq!(analysis.changes()[0], [0, 0, 0]);
    // A count other than the Original's index length is never accepted.
    assert!(stored_shots(&store, content, 0, 119).is_none());
    Ok(())
}

#[test]
fn shots_without_analysis_and_without_an_original_report_plainly() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = registered(scratch.path())?;
    let path = package.to_str().unwrap();
    let shots = success(&["shots", path, "--asset", "clip"])?;
    assert!(shots["analysis"].is_null());
    assert_eq!(shots["boundaries"], json!([]));
    // A generic project has no ready Original to default to.
    let output = cli(&["detect-shots", path])?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ShotDetectionUnavailable"));
    Ok(())
}

#[test]
fn a_cancelled_scan_stops_without_an_analysis() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = registered(scratch.path())?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let running = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(60);
    let input = prepare_shot_input(&store, Some(&AssetId::new("clip")?), &running, deadline)?;
    assert_eq!(input.pictures(), 120);
    let mut seen = Vec::new();
    let scan = scan_shots(&input, &running, deadline, |done, total| {
        seen.push((done, total))
    })?;
    assert_eq!(seen.len(), 120);
    assert_eq!(seen.last(), Some(&(120, 120)));
    assert_eq!(scan.analysis.pictures(), 120);

    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        scan_shots(&input, &cancelled, deadline, |_, _| {}),
        Err(ShotScanError::Cancelled)
    ));
    // Cancelling midway also stops.
    let midway = AtomicBool::new(false);
    let result = scan_shots(&input, &midway, deadline, |done, _| {
        if done == 10 {
            midway.store(true, std::sync::atomic::Ordering::Release);
        }
    });
    assert!(matches!(result, Err(ShotScanError::Cancelled)));
    assert!(matches!(
        scan_shots(&input, &running, Instant::now(), |_, _| {}),
        Err(ShotScanError::Deadline)
    ));
    Ok(())
}
