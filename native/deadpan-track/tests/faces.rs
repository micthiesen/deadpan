#![cfg(target_os = "macos")]
//! The real worker's `detect-faces` mode under the shared supervisor, end to
//! end on a deterministic synthetic fixture (drawn, shaded cartoon heads, not
//! person footage; see tests/generate_faces_fixture.py): pictures 0-1 show a
//! larger head left of center and a smaller one right of center, pictures
//! 2-3 only the background.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::faces::{
    FaceError, FaceRequest, choose_face, detect_faces, detect_project, face_target, prepare_faces,
};
use deadpan_cli::tracking::TrackingRuntime;
use deadpan_core::AssetId;
use deadpan_store::{AccessMode, ProjectStore};

fn runtime() -> TrackingRuntime {
    TrackingRuntime {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_deadpan-track")),
        environment: Default::default(),
    }
}

fn cli(arguments: &[&str]) {
    let code = deadpan_cli::entry(arguments.iter().map(|argument| argument.to_string()));
    assert_eq!(code, ExitCode::SUCCESS, "{arguments:?}");
}

fn clip() -> AssetId {
    AssetId::new("clip").unwrap()
}

/// A project with the fixture registered as asset `clip`.
fn project(scratch: &Path) -> PathBuf {
    let package = scratch.join("faces.deadpan");
    let path = package.to_str().unwrap();
    cli(&[
        "project", "create", path, "--fps", "24/1", "--size", "480x270",
    ]);
    let source = scratch.join("source.mkv");
    std::fs::write(&source, include_bytes!("fixtures/two-drawn-faces.mkv")).unwrap();
    cli(&["project", "retain-original", path, source.to_str().unwrap()]);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let records = store.original_records(None, 8).unwrap();
    let original = serde_json::to_value(records[0].object().content()).unwrap();
    let snapshot = store.snapshot().unwrap();
    drop(store);
    let request = serde_json::json!({
        "protocol": 1,
        "registration": {
            "expected_revision": snapshot.revision_id(), "new_revision": "registered",
            "original": original, "new_asset_id": "clip", "label": "Clip",
            "insertion": {"parent": snapshot.root(), "index": 0, "node": "clip-source", "label": "Clip"}
        },
        "streams": {"type": "video_only"}
    });
    let request_path = scratch.join("request.json");
    std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    cli(&[
        "project",
        "register-source",
        path,
        "--request-json",
        request_path.to_str().unwrap(),
    ]);
    package
}

fn request(at_pts: i64) -> FaceRequest {
    FaceRequest {
        asset: Some(clip()),
        at_pts,
    }
}

#[test]
fn vision_finds_both_drawn_faces_left_to_right_and_none_in_the_empty_picture() {
    let scratch = tempfile::tempdir().unwrap();
    let package = project(scratch.path());
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    // 30 ms lies inside picture 0 (Matroska's millisecond clock: 0, 42, 83, 125).
    let prepared = prepare_faces(&store, &request(30), &cancelled, deadline).unwrap();
    assert_eq!((prepared.ordinal, prepared.pts), (0, 0));
    let detection = detect_faces(&runtime(), &prepared, "first", &cancelled, deadline).unwrap();
    eprintln!(
        "faces {:?}, runtime {:?}, worker {:?}, Vision {:?}",
        detection.faces, detection.runtime, detection.worker_elapsed, detection.vision_elapsed
    );
    assert_eq!(detection.pts, 0);
    assert_eq!(detection.faces.len(), 2);
    assert!(
        detection
            .runtime
            .engine
            .contains("VNDetectFaceRectanglesRequest")
    );
    // Drawn heads: centers (135, 128) with half-width 45 px, and (352, 120)
    // with 34 px, of 480x270. Vision's boxes cover the face, not the hair.
    let [left, right] = [&detection.faces[0], &detection.faces[1]];
    let (lx, ly) = left.region.center();
    let (rx, ry) = right.region.center();
    assert!((lx * 480.0 - 135.0).abs() < 20.0 && (ly * 270.0 - 128.0).abs() < 30.0);
    assert!((rx * 480.0 - 352.0).abs() < 20.0 && (ry * 270.0 - 120.0).abs() < 30.0);
    assert!(left.region.width() > right.region.width());
    assert!(detection.faces.iter().all(|face| face.confidence >= 0.3));
    // Face 2 becomes an ordinary untracked target over [picture, end).
    let chosen = choose_face(&detection.faces, 2).unwrap();
    let target = face_target(
        chosen,
        "Face 2".into(),
        prepared.asset.clone(),
        prepared.time_base,
        prepared.pts,
        prepared.video_end,
    )
    .unwrap();
    assert!(target.samples.is_empty() && target.provenance.is_none());
    assert_eq!(
        target.region,
        deadpan_analysis::target_region(&right.region)
    );
    assert!(choose_face(&detection.faces, 3).is_err());

    // Picture 2 shows only the background.
    let empty = prepare_faces(&store, &request(83), &cancelled, deadline).unwrap();
    assert_eq!(empty.ordinal, 2);
    let detection = detect_faces(&runtime(), &empty, "second", &cancelled, deadline).unwrap();
    assert!(detection.faces.is_empty());
    assert_eq!(
        choose_face(&detection.faces, 1).unwrap_err(),
        "No faces were found in this picture."
    );

    // Before the first picture there is nothing to analyse.
    assert!(matches!(
        prepare_faces(&store, &request(-1), &cancelled, deadline),
        Err(FaceError::Request(_))
    ));
    drop(store);

    // The read-only report numbers the same faces.
    let report = detect_project(&runtime(), &package, &request(0), &cancelled, deadline).unwrap();
    assert_eq!(report["faces"].as_array().unwrap().len(), 2);
    assert_eq!(report["faces"][1]["face"], 2);
    assert_eq!(report["picture"], 0);
}

#[test]
fn cancellation_and_an_expired_deadline_return_typed_errors() {
    let scratch = tempfile::tempdir().unwrap();
    let package = project(scratch.path());
    let running = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let prepared = prepare_faces(&store, &request(0), &running, deadline).unwrap();
    drop(store);
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        detect_faces(&runtime(), &prepared, "cancelled", &cancelled, deadline),
        Err(FaceError::Cancelled)
    ));
    assert!(matches!(
        detect_faces(&runtime(), &prepared, "late", &running, Instant::now()),
        Err(FaceError::Deadline)
    ));
}
