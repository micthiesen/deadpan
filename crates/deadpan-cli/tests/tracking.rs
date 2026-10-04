#![cfg(any(target_os = "macos", target_os = "linux"))]
//! The `track` command's argument and availability checks, and saving
//! tracked paths as attention targets: success, stale heads, replacement,
//! undo and corrections. Paths here come from the pure policy over the
//! fixture's indexed pictures; the worker itself is exercised end to end by
//! native/deadpan-track/tests/worker.rs, which this package cannot build.
//! Saving while the app holds the writer (the live endpoint route) is not
//! exercised here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deadpan_analysis::{
    Keyframe, NormalizedRect, RawObservation, TrackPolicy, TrackStop, TrackedPath,
};
use deadpan_cli::tracking::{TrackOutcome, save_correction, save_target, save_tracked};
use deadpan_core::{AssetId, TargetId};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

fn cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

fn failure(arguments: &[&str]) -> String {
    let output = cli(arguments);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn track_refuses_bad_arguments_and_unavailable_analysis_without_writing() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("track.deadpan");
    let path = package.to_str().unwrap();
    assert!(cli(&["project", "create", path]).status.success());

    assert!(failure(&["track", path]).contains("InvalidInput"));
    assert!(failure(&["track", path, "--from", "0", "--to", "10"]).contains("InvalidInput"));
    let outside = failure(&[
        "track",
        path,
        "--from",
        "0",
        "--to",
        "10",
        "--region",
        "0.5,0.5,0.6,0.1",
    ]);
    assert!(outside.contains("InvalidInput") && outside.contains("outside the picture"));
    let stride = failure(&[
        "track",
        path,
        "--from",
        "0",
        "--to",
        "10",
        "--region",
        "0.1,0.1,0.2,0.2",
        "--stride",
        "0",
    ]);
    assert!(stride.contains("InvalidInput"), "{stride}");
    // Repeated options, a label without --save and unknown options refuse.
    let repeated = failure(&[
        "track",
        path,
        "--from",
        "0",
        "--from",
        "1",
        "--to",
        "10",
        "--region",
        "0.1,0.1,0.2,0.2",
    ]);
    assert!(repeated.contains("InvalidInput"), "{repeated}");
    assert!(
        failure(&[
            "track",
            path,
            "--from",
            "0",
            "--to",
            "10",
            "--region",
            "0.1,0.1,0.2,0.2",
            "--label",
            "Speaker",
        ])
        .contains("InvalidInput")
    );
    assert!(failure(&["track-correct", path]).contains("InvalidInput"));
    assert!(
        failure(&[
            "track-correct",
            path,
            "--target",
            "speaker",
            "--at",
            "0",
            "--region",
            "0.1,0.1,0.2,0.2",
            "--at",
            "1",
        ])
        .contains("InvalidInput")
    );
    let missing_target = failure(&[
        "track-correct",
        path,
        "--target",
        "speaker",
        "--at",
        "0",
        "--region",
        "0.1,0.1,0.2,0.2",
    ]);
    assert!(
        missing_target.contains("TrackingUnavailable"),
        "{missing_target}"
    );
    // A generic project has no ready Original to default to.
    let missing = failure(&[
        "track",
        path,
        "--from",
        "0",
        "--to",
        "10",
        "--region",
        "0.1,0.1,0.2,0.2",
    ]);
    assert!(missing.contains("TrackingUnavailable"), "{missing}");
}

fn success(arguments: &[&str]) -> Value {
    let output = cli(arguments);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// A project with the tracking fixture registered as asset `clip`.
fn registered(scratch: &Path) -> PathBuf {
    let package = scratch.join("saved.deadpan");
    let path = package.to_str().unwrap();
    success(&[
        "project", "create", path, "--fps", "24/1", "--size", "320x180",
    ]);
    let source = scratch.join("source.mkv");
    std::fs::write(
        &source,
        include_bytes!("../../../native/deadpan-track/tests/fixtures/moving-square-cut.mkv"),
    )
    .unwrap();
    let retained = success(&["project", "retain-original", path, source.to_str().unwrap()]);
    let original = retained["retained_original"]["record"]["object"]["content"].clone();
    let head = head(&package);
    let request = json!({
        "protocol": 1,
        "registration": {
            "expected_revision": head.revision_id(), "new_revision": "registered",
            "original": original, "new_asset_id": "clip", "label": "Clip",
            "insertion": {"parent": head.root(), "index": 0, "node": "clip-source", "label": "Clip"}
        },
        "streams": {"type": "video_only"}
    });
    let request_path = scratch.join("request.json");
    std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    success(&[
        "project",
        "register-source",
        path,
        "--request-json",
        request_path.to_str().unwrap(),
    ]);
    package
}

fn head(package: &Path) -> deadpan_core::ProjectDocument {
    ProjectStore::open(package, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap()
}

fn clip() -> AssetId {
    AssetId::new("clip").unwrap()
}

/// The fixture's first-shot picture PTS (Matroska milliseconds at 24 fps).
fn pictures() -> Vec<i64> {
    (0..30).map(|picture| (picture * 1000 + 12) / 24).collect()
}

fn square(picture: i64) -> NormalizedRect {
    NormalizedRect::new(
        (40.0 + 5.0 * picture as f64) / 320.0,
        (40.0 + 1.5 * picture as f64) / 180.0,
        36.0 / 320.0,
        36.0 / 180.0,
    )
    .unwrap()
}

/// A path over `pictures` tracked from `first`, seeing the square throughout.
fn path(pictures: &[i64], first: usize) -> TrackedPath {
    let observations: Vec<_> = pictures[first..]
        .iter()
        .enumerate()
        .map(|(offset, &pts)| RawObservation {
            pts,
            region: Some(square((first + offset) as i64)),
            confidence: 0.9,
        })
        .collect();
    TrackedPath::track(
        TrackPolicy::default(),
        16.0 / 9.0,
        &pictures[first..],
        1_250,
        TrackStop::ShotBoundary { picture: 30 },
        Keyframe {
            pts: pictures[first],
            region: square(first as i64),
        },
        &observations,
    )
    .unwrap()
}

fn outcome(head: deadpan_core::ProjectDocument) -> TrackOutcome {
    let time_base = head.assets()[&clip()].video.unwrap().start().time_base;
    TrackOutcome {
        report: json!({}),
        path: path(&pictures(), 0),
        asset: clip(),
        time_base,
        engine: "test engine".into(),
        head,
    }
}

#[test]
fn saving_respects_the_head_tracked_against_replacement_undo_and_corrections() {
    let scratch = tempfile::tempdir().unwrap();
    let package = registered(scratch.path());
    let id = TargetId::new("square").unwrap();

    // Tracking resolved against this head; an edit lands while it runs.
    let tracked_against = head(&package);
    let intervening = outcome(tracked_against.clone());
    let (other, _) = intervening
        .path
        .to_target("Other".into(), clip(), intervening.time_base, "test", 4_096)
        .unwrap();
    save_target(
        &package,
        &tracked_against,
        TargetId::new("other").unwrap(),
        other,
    )
    .unwrap();
    // The stale save is refused, and nothing is overwritten.
    let stale = save_tracked(&package, &intervening, &id, "Square", false).unwrap_err();
    assert!(stale.to_string().starts_with("RevisionConflict"), "{stale}");
    let current = head(&package);
    assert!(!current.targets().contains_key(&id));
    assert_eq!(current.targets().len(), 1);

    // Against the current head it saves as one reversible edit.
    let saved = save_tracked(&package, &outcome(current.clone()), &id, "Square", false).unwrap();
    assert_eq!(saved["replaced"], false);
    let after_save = head(&package);
    let target = after_save.targets()[&id].clone();
    assert_eq!(target.label, "Square");
    assert_eq!(target.span.end().ticks, 1_250);
    assert_eq!(saved["samples"], target.samples.len());

    // An existing target is refused without --replace, even from the CLI
    // before any tracking starts.
    assert!(save_tracked(&package, &outcome(after_save.clone()), &id, "Again", false).is_err());
    let path_text = package.to_str().unwrap();
    let refused = failure(&[
        "track",
        path_text,
        "--asset",
        "clip",
        "--from",
        "0",
        "--to",
        "1250",
        "--region",
        "0.125,0.2222,0.1125,0.2",
        "--save",
        "square",
    ]);
    assert!(refused.contains("already exists"), "{refused}");
    assert!(
        failure(&[
            "track",
            path_text,
            "--asset",
            "clip",
            "--from",
            "0",
            "--to",
            "1250",
            "--region",
            "0.125,0.2222,0.1125,0.2",
            "--replace",
        ])
        .contains("InvalidInput")
    );
    assert_eq!(head(&package).revision_id(), after_save.revision_id());

    // A correction at picture 24 replaces only its range and records the
    // latest engine, keeping why the span ends.
    let pictures = pictures();
    let segment = path(&pictures, 24);
    let (corrected, _) =
        save_correction(&package, &after_save, &id, &segment, "later engine").unwrap();
    let after_correction = head(&package);
    assert_eq!(after_correction.targets()[&id], corrected);
    assert_eq!(corrected.corrections.len(), 1);
    assert_eq!(corrected.corrections[0].at, pictures[24]);
    for sample in target.samples.iter().filter(|s| s.at < pictures[24]) {
        assert!(corrected.samples.contains(sample));
    }
    let provenance = corrected.provenance.as_ref().unwrap();
    assert_eq!(provenance.engine, "later engine");
    assert_eq!(provenance.stop, deadpan_core::TargetStop::ShotBoundary);
    // A correction against a stale head is refused too.
    let stale = save_correction(&package, &after_save, &id, &segment, "stale").unwrap_err();
    assert!(stale.to_string().starts_with("RevisionConflict"), "{stale}");

    // --replace overwrites the target and its corrections.
    let replaced = save_tracked(
        &package,
        &outcome(after_correction.clone()),
        &id,
        "Square",
        true,
    )
    .unwrap();
    assert_eq!(replaced["replaced"], true);
    let after_replace = head(&package);
    assert!(after_replace.targets()[&id].corrections.is_empty());

    // Undo restores the corrected target, then the original save, then none.
    for expected in [Some(&corrected), Some(&target), None] {
        let current = head(&package);
        success(&[
            "project",
            "undo",
            path_text,
            "--expected",
            current.revision_id().as_str(),
        ]);
        assert_eq!(head(&package).targets().get(&id), expected);
    }
}
