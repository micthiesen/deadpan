#![cfg(target_os = "macos")]
//! The real worker executable under the shared supervisor, end to end on a
//! deterministic synthetic fixture (not person footage; see
//! tests/generate_fixture.py): a bright square moves over a dark textured
//! background, passes behind a pillar, and a hard cut at picture 30 shows a
//! second square elsewhere on a bright background.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_analysis::{Keyframe, NormalizedRect, TrackPolicy, TrackState, TrackStop};
use deadpan_cli::shots::{prepare_shot_input, scan_shots};
use deadpan_cli::tracking::{
    TrackRequest, TrackingError, TrackingRuntime, correct, prepare_tracking, track, track_project,
};
use deadpan_core::AssetId;
use deadpan_store::{AccessMode, ProjectStore};

const WIDTH: f64 = 320.0;
const HEIGHT: f64 = 180.0;
const SQUARE: f64 = 36.0;
const CUT: usize = 30;

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

/// Shot A's square, in pixels, at a picture ordinal (as generated).
fn truth(picture: usize) -> (f64, f64) {
    let (x, y) = (40.0 + 5.0 * picture as f64, 40.0 + 1.5 * picture as f64);
    (x.round() + SQUARE / 2.0, y.round() + SQUARE / 2.0)
}

fn center_pixels(region: &NormalizedRect) -> (f64, f64) {
    let (x, y) = region.center();
    (x * WIDTH, y * HEIGHT)
}

/// A project with the fixture registered as asset `clip` and its shot
/// analysis stored.
fn project(scratch: &Path, with_shots: bool) -> PathBuf {
    let package = scratch.join("track.deadpan");
    let path = package.to_str().unwrap();
    cli(&[
        "project", "create", path, "--fps", "24/1", "--size", "320x180",
    ]);
    let source = scratch.join("source.mkv");
    std::fs::write(&source, include_bytes!("fixtures/moving-square-cut.mkv")).unwrap();
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
    if with_shots {
        let cancelled = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(60);
        let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
        let input = prepare_shot_input(&store, Some(&clip()), &cancelled, deadline).unwrap();
        drop(store);
        let scan = scan_shots(
            &input,
            &cancelled,
            deadline,
            deadpan_cli::shots::ShotScanOptions::default(),
            |_, _| {},
            |_| true,
        )
        .unwrap();
        assert_eq!(scan.analysis.boundaries(), [CUT]);
        let writer = ProjectStore::open(&package, AccessMode::ReadWrite).unwrap();
        writer
            .save_shot_analysis(&scan.key, &scan.analysis)
            .unwrap();
    }
    package
}

fn clip() -> AssetId {
    AssetId::new("clip").unwrap()
}

fn request(stop_at_shots: bool, stride: u32) -> TrackRequest {
    let (x, y) = (40.0, 40.0);
    TrackRequest {
        asset: Some(clip()),
        from_pts: 0,
        to_pts: i64::MAX,
        region: NormalizedRect::new(x / WIDTH, y / HEIGHT, SQUARE / WIDTH, SQUARE / HEIGHT)
            .unwrap(),
        stride,
        stop_at_shots,
    }
}

#[test]
fn the_tracker_follows_the_square_and_stops_at_the_cut() {
    let scratch = tempfile::tempdir().unwrap();
    let package = project(scratch.path(), true);
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let report = track_project(
        &runtime(),
        &package,
        &request(true, 1),
        &cancelled,
        deadline,
    )
    .unwrap()
    .report;
    let path: deadpan_analysis::TrackedPath =
        serde_json::from_value(report["path"].clone()).unwrap();
    assert_eq!(path.stop(), TrackStop::ShotBoundary { picture: CUT });
    assert_eq!(report["pictures"], CUT);
    assert_eq!(path.samples().len(), CUT);
    // Matroska's millisecond clock: picture 30 is at 1250 ms.
    assert_eq!(path.end_pts(), 1_250);
    assert!(path.samples().iter().all(|sample| sample.pts < 1_250));
    eprintln!(
        "worker {} ms, Vision {:.2} ms per picture, states {:?}",
        report["worker_elapsed_ms"],
        report["vision_ms_per_picture"].as_f64().unwrap(),
        path.samples().iter().map(|s| s.state).collect::<Vec<_>>()
    );
    for (picture, sample) in path.samples().iter().enumerate() {
        let (x, y) = center_pixels(&sample.region);
        let (tx, ty) = truth(picture);
        eprintln!(
            "{picture:2} {:?} conf {:?} center ({x:6.1}, {y:6.1}) truth ({tx:6.1}, {ty:6.1})",
            sample.state, sample.confidence
        );
        // Whatever the state, the path never leaves the square for another
        // subject, and confident pictures stay on it.
        let error = (x - tx).hypot(y - ty);
        match sample.state {
            TrackState::Manual | TrackState::Tracked | TrackState::Interpolated => {
                assert!(error < 8.0, "picture {picture} is {error:.1} px off")
            }
            TrackState::Lost | TrackState::Held => {
                assert!(error < 5.0 * (picture as f64) + 8.0)
            }
        }
    }
    // The unoccluded approach is tracked, not merely held.
    assert!(
        path.samples()[1..12]
            .iter()
            .all(|sample| sample.state == TrackState::Tracked)
    );
}

#[test]
fn tracking_through_the_cut_never_jumps_and_a_correction_retracks_only_its_range() {
    let scratch = tempfile::tempdir().unwrap();
    let package = project(scratch.path(), false);
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    // Without a stored analysis, the default refuses rather than running on.
    assert!(matches!(
        prepare_tracking(&store, &request(true, 1), &cancelled, deadline),
        Err(TrackingError::Unavailable(_))
    ));
    let prepared = prepare_tracking(&store, &request(false, 2), &cancelled, deadline).unwrap();
    drop(store);
    assert_eq!(prepared.stop(), TrackStop::RangeEnd);
    assert_eq!(prepared.pictures().len(), 48);
    let result = track(
        &runtime(),
        &prepared,
        TrackPolicy::default(),
        "through-cut",
        &cancelled,
        deadline,
        |_| {},
    )
    .unwrap();
    assert_eq!(result.analysed, 24);
    let samples = result.path.samples();
    assert_eq!(samples.len(), 24);
    for (step, sample) in samples.iter().enumerate() {
        let picture = step * 2;
        eprintln!(
            "{picture:2} {:?} conf {:?} center {:?}",
            sample.state,
            sample.confidence,
            center_pixels(&sample.region)
        );
        if picture >= CUT {
            // The second square starts at (248, 128); the path stays left.
            let (x, _) = center_pixels(&sample.region);
            assert!(x < 200.0, "picture {picture} jumped to {x:.1}");
        }
    }

    // A manual correction on the second square, at picture 32, re-tracks only
    // from there to the end; everything before it is untouched.
    let before = result.path.clone();
    let mut path = result.path;
    let keyframe_pts = prepared.pictures()[32];
    let (x, y) = (230.0 - 8.0, 110.0 - 2.0);
    let keyframe = Keyframe {
        pts: keyframe_pts,
        region: NormalizedRect::new(x / WIDTH, y / HEIGHT, SQUARE / WIDTH, SQUARE / HEIGHT)
            .unwrap(),
    };
    let range = correct(
        &runtime(),
        &prepared,
        &mut path,
        keyframe,
        "correction",
        &cancelled,
        deadline,
        |_| {},
    )
    .unwrap();
    assert_eq!(
        (range.start_pts, range.end_pts),
        (keyframe_pts, prepared.end_pts())
    );
    let split = before.samples().partition_point(|s| s.pts < keyframe_pts);
    assert_eq!(&path.samples()[..split], &before.samples()[..split]);
    assert_eq!(path.samples()[split].state, TrackState::Manual);
    assert_eq!(path.keyframes().len(), 2);
    for (offset, sample) in path.samples()[split + 1..].iter().enumerate() {
        let picture = 32 + 2 * (offset + 1);
        let shift = (picture - 32) as f64;
        let (tx, ty) = (x - 4.0 * shift + SQUARE / 2.0, y - shift + SQUARE / 2.0);
        let (cx, cy) = center_pixels(&sample.region);
        eprintln!(
            "retracked {picture} {:?} {cx:.1},{cy:.1} truth {tx:.1},{ty:.1}",
            sample.state
        );
        assert_eq!(sample.state, TrackState::Tracked);
        assert!((cx - tx).hypot(cy - ty) < 8.0);
    }
}

#[test]
fn cancellation_and_an_expired_deadline_stop_without_a_path() {
    let scratch = tempfile::tempdir().unwrap();
    let package = project(scratch.path(), true);
    let running = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let prepared = prepare_tracking(&store, &request(true, 1), &running, deadline).unwrap();
    drop(store);
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        track(
            &runtime(),
            &prepared,
            TrackPolicy::default(),
            "cancelled",
            &cancelled,
            deadline,
            |_| {}
        ),
        Err(TrackingError::Cancelled)
    ));
    assert!(matches!(
        track(
            &runtime(),
            &prepared,
            TrackPolicy::default(),
            "expired",
            &running,
            Instant::now(),
            |_| {}
        ),
        Err(TrackingError::Deadline)
    ));
    // A range that starts before the first picture is refused.
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let early = TrackRequest {
        from_pts: -1,
        ..request(true, 1)
    };
    assert!(matches!(
        prepare_tracking(&store, &early, &running, deadline),
        Err(TrackingError::Request(_))
    ));
}

#[test]
fn a_saved_target_reloads_follows_in_the_plan_and_corrects_only_its_range() {
    use deadpan_analysis::target_region;
    use deadpan_cli::tracking::{correct_target, save_target, target_rect_at};
    use deadpan_core::{ExactRatio, Framing, FramingClock, FramingPose, FramingValue, TargetId};

    let scratch = tempfile::tempdir().unwrap();
    let package = project(scratch.path(), true);
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    // Stride 3: the saved target interpolates between analysed pictures.
    let outcome = track_project(
        &runtime(),
        &package,
        &request(true, 3),
        &cancelled,
        deadline,
    )
    .unwrap();
    let id = TargetId::new("square").unwrap();
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    let (target, tolerance) = outcome
        .path
        .to_target(
            "Square".into(),
            outcome.asset.clone(),
            outcome.time_base,
            &outcome.engine,
            4_096,
        )
        .unwrap();
    save_target(&package, &before, id.clone(), target).unwrap();

    // Reload from SQLite: an ordinary reversible edit with the path inside it.
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let document = store.snapshot().unwrap();
    assert_ne!(document.revision_id(), before.revision_id());
    drop(store);
    let saved = document.targets()[&id].clone();
    assert_eq!(saved.span.end().ticks, 1_250);
    let provenance = saved.provenance.clone().unwrap();
    assert_eq!(provenance.rule, deadpan_analysis::TARGET_RULE);
    assert_eq!(provenance.stop, deadpan_core::TargetStop::ShotBoundary);
    for sample in outcome.path.samples() {
        let (region, _) = saved
            .region_at(deadpan_core::SourcePoint {
                ticks: ExactRatio::integer(sample.pts),
                time_base: saved.span.start().time_base,
            })
            .unwrap();
        let expected = target_region(&sample.region);
        for axis in 0..2 {
            assert!(region.center[axis].abs_diff(expected.center[axis]) <= tolerance + 1);
            assert!(region.size[axis].abs_diff(expected.size[axis]) <= tolerance + 1);
        }
    }
    // Between analysed pictures 0 and 3 the saved path moves smoothly with the
    // square rather than stepping: picture 4 (167 ms) lies between 3 and 6.
    let at_four = target_rect_at(&saved, 167).unwrap();
    let (x, _) = center_pixels(&at_four);
    assert!((x - truth(4).0).abs() < 3.0, "picture 4 center {x:.1}");

    // A Follow layer over the saved target resolves in the render plan.
    let mut value = serde_json::to_value(&document).unwrap();
    value["nodes"]["clip-source"]["framing"] = serde_json::to_value(Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: id.clone(),
            scale: ExactRatio::integer(2),
            fallback: FramingPose::identity(),
        },
    })
    .unwrap();
    let followed = deadpan_core::ProjectDocument::from_json(&value.to_string()).unwrap();
    let plan = deadpan_plan::RenderPlan::compile(&followed).unwrap();
    for frame in [0, 5, 10] {
        let sample = plan.picture(deadpan_core::ProjectFrame(frame)).unwrap();
        let deadpan_plan::Picture::Source { point, .. } = sample.picture else {
            panic!("frame {frame} is not a source picture");
        };
        let (region, _) = saved.region_at(point).unwrap();
        let [x, y] = region.center_ratio();
        let expected = FramingPose::new(x, y, ExactRatio::integer(2))
            .unwrap()
            .quantized()
            .unwrap();
        assert_eq!(sample.framing[0].pose, Some(expected), "frame {frame}");
    }
    // Beyond the target's span (after the cut) the fallback applies.
    let after = plan.picture(deadpan_core::ProjectFrame(40)).unwrap();
    assert_eq!(after.framing[0].pose, Some(FramingPose::identity()));

    // A correction at picture 24 (1000 ms) re-tracks only [1000, 1250).
    let (x, y) = (40.0 + 5.0 * 24.0, 40.0 + 1.5 * 24.0);
    let region =
        NormalizedRect::new(x / WIDTH, y / HEIGHT, SQUARE / WIDTH, SQUARE / HEIGHT).unwrap();
    let (corrected, report) = correct_target(
        &runtime(),
        &package,
        &id,
        1_000,
        region,
        1,
        &cancelled,
        deadline,
    )
    .unwrap();
    assert_eq!(report["corrected_range"], serde_json::json!([1_000, 1_250]));
    let reloaded = ProjectStore::open(&package, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(reloaded.targets()[&id], corrected);
    assert_eq!(corrected.corrections.len(), 1);
    for sample in saved.samples.iter().filter(|sample| sample.at < 1_000) {
        assert!(corrected.samples.contains(sample));
    }
    assert_eq!(corrected.corrections[0].region, target_region(&region));
    let (cx, cy) = center_pixels(&target_rect_at(&corrected, 1_000).unwrap());
    assert!((cx - (x + SQUARE / 2.0)).abs() < 0.01 && (cy - (y + SQUARE / 2.0)).abs() < 0.01);
}
