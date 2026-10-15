#![cfg(target_os = "macos")]
//! The real worker on rotated and anamorphic Originals, and saving a tracked
//! target while another owner holds the writer behind the authenticated live
//! endpoint. Fixtures are synthetic (tests/generate_geometry_fixtures.py): one
//! moving square drawn in a square-pixel 320x180 scene, stored with a
//! clockwise quarter turn, with a 4:3 sample aspect ratio, or both. The truth
//! is FFmpeg's own upright, square-pixel presentation of each file, measured
//! by the generator, not Deadpan's mapping.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use deadpan_analysis::{NormalizedRect, TrackState, TrackStop};
use deadpan_cli::host::Endpoint;
use deadpan_cli::live_project::{Operation, Reply, Request};
use deadpan_cli::tracking::{
    TrackOutcome, TrackRequest, TrackingRuntime, prepare_tracking, save_tracked, track_project,
};
use deadpan_core::{AssetId, AttentionTarget, ExactRatio, FramingPose, SourcePoint, TargetId};
use deadpan_render::{FitMode, PictureGeometry};
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::Value;

const PICTURES: usize = 24;
/// Upright pixels of the square-pixel scene; tracking error is measured in them.
const TOLERANCE_PX: f64 = 3.0;

struct Fixture {
    name: &'static str,
    bytes: &'static [u8],
    /// Clockwise quarter turns the stream declares.
    turns: u8,
    /// Upright scene size in square pixels (the generator's 320x180, turned).
    upright: [u32; 2],
    /// Exact upright display aspect: visible width x SAR / height, turned.
    aspect: (i128, i128),
}

const FIXTURES: [Fixture; 3] = [
    Fixture {
        name: "square-rotated-90.mp4",
        bytes: include_bytes!("fixtures/square-rotated-90.mp4"),
        turns: 1,
        upright: [180, 320],
        aspect: (9, 16),
    },
    Fixture {
        name: "square-anamorphic.mkv",
        bytes: include_bytes!("fixtures/square-anamorphic.mkv"),
        turns: 0,
        upright: [320, 180],
        aspect: (16, 9),
    },
    Fixture {
        name: "square-rotated-270-anamorphic.mp4",
        bytes: include_bytes!("fixtures/square-rotated-270-anamorphic.mp4"),
        turns: 3,
        upright: [180, 320],
        aspect: (9, 16),
    },
];

fn truth(name: &str) -> Vec<([f64; 2], [f64; 2])> {
    let all: Value =
        serde_json::from_str(include_str!("fixtures/square-geometry-truth.json")).unwrap();
    let pair = |value: &Value| [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()];
    all[name]["pictures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|picture| (pair(&picture["center"]), pair(&picture["size"])))
        .collect()
}

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

/// A project on the fixture's upright canvas with it registered as `clip`.
fn project(scratch: &Path, fixture: &Fixture) -> PathBuf {
    let package = scratch.join("geometry.deadpan");
    let path = package.to_str().unwrap();
    let size = format!("{}x{}", fixture.upright[0], fixture.upright[1]);
    cli(&["project", "create", path, "--fps", "24/1", "--size", &size]);
    let source = scratch.join(fixture.name);
    std::fs::write(&source, fixture.bytes).unwrap();
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

/// Track from picture 0, selecting the truth's box in upright coordinates.
fn request(name: &str) -> TrackRequest {
    let (center, size) = truth(name)[0];
    TrackRequest {
        asset: Some(clip()),
        from_pts: 0,
        to_pts: i64::MAX,
        region: NormalizedRect::new(
            center[0] - size[0] / 2.0,
            center[1] - size[1] / 2.0,
            size[0],
            size[1],
        )
        .unwrap(),
        stride: 1,
        stop_at_shots: false,
    }
}

fn track(package: &Path, name: &str) -> TrackOutcome {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    track_project(&runtime(), package, &request(name), &cancelled, deadline).unwrap()
}

fn error_px(fixture: &Fixture, actual: (f64, f64), expected: [f64; 2]) -> f64 {
    let [width, height] = fixture.upright.map(f64::from);
    ((actual.0 - expected[0]) * width).hypot((actual.1 - expected[1]) * height)
}

fn head(package: &Path) -> deadpan_core::ProjectDocument {
    ProjectStore::open(package, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap()
}

fn value(ratio: ExactRatio) -> f64 {
    ratio.numerator() as f64 / ratio.denominator() as f64
}

/// The decoded Original's render metadata, as the preview and export adapters
/// build it from the qualified stream.
fn render_metadata(scratch: &Path, fixture: &Fixture) -> deadpan_render::FrameMetadata {
    let cancelled = AtomicBool::new(false);
    let control = DecodeControl {
        timeout: Duration::from_secs(30),
        cancelled: &cancelled,
    };
    let file = std::fs::File::open(scratch.join(fixture.name)).unwrap();
    let mut decoder = SourceDecoder::open(file, DecodeLimits::default(), control).unwrap();
    let info = decoder.info().clone();
    let decoded = decoder.next_rgba(control).unwrap().unwrap();
    *deadpan_cli::picture::source_to_render_frame(decoded, &info)
        .unwrap()
        .metadata()
}

/// Where a Fit canvas shows an upright normalized point, from the upright
/// size alone (independent of the renderer's rotation and SAR handling).
fn fit_on_canvas(upright: [u32; 2], canvas: [u32; 2], point: [f64; 2]) -> [f64; 2] {
    let [width, height] = upright.map(f64::from);
    let [cw, ch] = canvas.map(f64::from);
    let scale = (cw / width).min(ch / height);
    [
        ((cw - width * scale) / 2.0 + point[0] * width * scale) / cw,
        ((ch - height * scale) / 2.0 + point[1] * height * scale) / ch,
    ]
}

#[test]
fn rotated_and_anamorphic_originals_track_and_follow_in_upright_coordinates() {
    for fixture in &FIXTURES {
        let scratch = tempfile::tempdir().unwrap();
        let package = project(scratch.path(), fixture);
        let expected = truth(fixture.name);
        let aspect = ExactRatio::new(fixture.aspect.0, fixture.aspect.1).unwrap();

        // The host measures the upright aspect from the qualified stream.
        {
            let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
            let prepared = prepare_tracking(
                &store,
                &request(fixture.name),
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(60),
            )
            .unwrap();
            assert_eq!(
                prepared.exact_display_aspect,
                Some(aspect),
                "{}",
                fixture.name
            );
            assert!((prepared.display_aspect - value(aspect)).abs() < 1e-12);
        }

        let outcome = track(&package, fixture.name);
        assert_eq!(
            outcome.report["stream"]["rotation_quarter_turns"],
            fixture.turns
        );
        let path = &outcome.path;
        assert_eq!(path.stop(), TrackStop::RangeEnd);
        assert_eq!(path.samples().len(), PICTURES);
        let mut worst: f64 = 0.0;
        for (ordinal, sample) in path.samples().iter().enumerate() {
            let (center, size) = expected[ordinal];
            let error = error_px(fixture, sample.region.center(), center);
            worst = worst.max(error);
            assert!(
                error < TOLERANCE_PX,
                "{} picture {ordinal}: {error:.2} px",
                fixture.name
            );
            if ordinal > 0 {
                assert_eq!(sample.state, TrackState::Tracked, "{}", fixture.name);
            }
            // The box keeps the upright orientation: a quarter turn swaps
            // which normalized extent is the larger.
            for (axis, (actual, truth)) in [sample.region.width(), sample.region.height()]
                .into_iter()
                .zip(size)
                .enumerate()
            {
                assert!(
                    (actual / truth - 1.0).abs() < 0.25,
                    "{} picture {ordinal} axis {axis}: {actual} vs {truth}",
                    fixture.name
                );
            }
        }

        // Save, reload, and evaluate the target exactly where the tracker ran.
        let id = TargetId::new("square").unwrap();
        save_tracked(&package, &outcome, &id, "Square", false).unwrap();
        let document = head(&package);
        let target: &AttentionTarget = &document.targets()[&id];
        assert_eq!(target.display_aspect, Some(aspect));
        let metadata = render_metadata(scratch.path(), fixture);
        // The decoded picture's own upright aspect is the one recorded.
        assert_eq!(metadata.display_aspect(), Some(aspect), "{}", fixture.name);
        let time_base = outcome.time_base;
        let mut saved_worst: f64 = 0.0;
        for (ordinal, sample) in path.samples().iter().enumerate() {
            let point = SourcePoint {
                ticks: ExactRatio::integer(sample.pts),
                time_base,
            };
            let (region, _) = target.region_at(point).unwrap();
            let center = region.center.map(|v| f64::from(v) / 1_000_000.0);
            let error = error_px(fixture, (center[0], center[1]), expected[ordinal].0);
            saved_worst = saved_worst.max(error);
            assert!(
                error < TOLERANCE_PX,
                "{} saved picture {ordinal}",
                fixture.name
            );

            // Camera and follow: the shared renderer places the saved center
            // on the true square for every canvas shape, and the plan's
            // follow centers exactly that point.
            for canvas in [fixture.upright, [1080, 1920], [1920, 1080], [1080, 1080]] {
                let geometry =
                    PictureGeometry::new(&metadata, canvas[0], canvas[1], FitMode::Fit).unwrap();
                let shown = geometry.source_to_canvas(center).unwrap().unwrap();
                let independent = fit_on_canvas(fixture.upright, canvas, center);
                let on_truth = fit_on_canvas(fixture.upright, canvas, expected[ordinal].0);
                for axis in 0..2 {
                    assert!((shown[axis] - independent[axis]).abs() < 1e-9);
                    let canvas_px = (shown[axis] - on_truth[axis]).abs() * f64::from(canvas[axis]);
                    let scale = f64::from(canvas[axis]) / f64::from(fixture.upright[axis]);
                    assert!(canvas_px <= TOLERANCE_PX * scale.max(1.0) * 2.0);
                }
                let pose = deadpan_plan::follow_pose(
                    target,
                    &clip(),
                    point,
                    canvas,
                    std::iter::empty::<Option<FramingPose>>(),
                    ExactRatio::ONE,
                )
                .unwrap();
                let input = geometry.source_to_input(0, center).unwrap().unwrap();
                assert!((value(pose.center_x) - input[0]).abs() < 1e-9);
                assert!((value(pose.center_y) - input[1]).abs() < 1e-9);
            }
        }
        eprintln!(
            "{}: path worst {worst:.2} px, saved worst {saved_worst:.2} px, {} stored samples, {}",
            fixture.name,
            target.samples.len(),
            outcome.engine
        );
    }
}

/// A service-thread stand-in for the open app: the real authenticated
/// transport and shared short-command executor over the writer it holds.
fn serve<T: Send>(
    store: &mut ProjectStore,
    endpoint: &mut Endpoint,
    refresh_error: Option<&str>,
    client: impl FnOnce() -> T + Send,
) -> (T, usize) {
    thread::scope(|scope| {
        let client = scope.spawn(client);
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut executed = 0;
        while !client.is_finished() {
            assert!(Instant::now() < deadline, "live endpoint fixture timed out");
            for incoming in endpoint.poll() {
                store.check_writer_owner(endpoint.owner_handle()).unwrap();
                let reply = match Request::from_value(incoming.payload) {
                    Ok(Request {
                        operation:
                            Operation::Execute {
                                project_id,
                                command,
                            },
                        ..
                    }) => {
                        executed += 1;
                        match deadpan_cli::live_project::execute_short(store, &project_id, &command)
                        {
                            Ok(execution) => Reply::Completed {
                                output: execution.output,
                                committed_revision: execution.committed_revision,
                                committed_registers: execution.committed_registers,
                                refresh_error: refresh_error.map(str::to_owned),
                            },
                            Err(error) => Reply::Failed { error },
                        }
                    }
                    other => panic!("unexpected live request {:?}", other.map(|_| ())),
                };
                endpoint
                    .respond(incoming.ticket, serde_json::to_value(reply).unwrap())
                    .unwrap();
            }
            thread::park_timeout(Duration::from_millis(1));
        }
        (client.join().unwrap(), executed)
    })
}

#[test]
fn track_save_commits_through_the_open_projects_live_endpoint() {
    let fixture = &FIXTURES[0];
    let scratch = tempfile::tempdir().unwrap();
    let package = project(scratch.path(), fixture);
    // The "app" holds the writer and advertises its endpoint for the whole test.
    let mut owner = ProjectStore::open(&package, AccessMode::ReadWrite).unwrap();
    let mut endpoint = Endpoint::bind(&mut owner).unwrap();
    let registered = owner.snapshot().unwrap();

    // Tracking reads a read-only snapshot beside the held writer.
    let outcome = track(&package, fixture.name);
    assert_eq!(outcome.head.revision_id(), registered.revision_id());
    let id = TargetId::new("square").unwrap();

    // The save goes to the owner, which commits it; its workspace refresh
    // then fails, which must not hide or repeat the durable commit.
    let (saved, executed) = serve(
        &mut owner,
        &mut endpoint,
        Some("workspace refresh failed"),
        || save_tracked(&package, &outcome, &id, "Square", false),
    );
    let saved = saved.unwrap();
    assert_eq!(executed, 1);
    let committed = owner.snapshot().unwrap();
    assert_ne!(committed.revision_id(), registered.revision_id());
    let receipt = &saved["receipt"];
    assert_eq!(receipt["committed"], true);
    assert_eq!(
        receipt["committed_revision"],
        serde_json::to_value(committed.revision_id()).unwrap()
    );
    assert_eq!(
        receipt["outcome"]["revision_id"],
        receipt["committed_revision"]
    );
    assert_eq!(receipt["host_refresh_error"], "workspace refresh failed");
    assert_eq!(saved["replaced"], false);
    let target = &committed.targets()[&id];
    assert_eq!(saved["samples"], target.samples.len());
    assert_eq!(target.label, "Square");
    assert_eq!(target.display_aspect, Some(ExactRatio::new(9, 16).unwrap()));
    // The owner's own history holds exactly that edit; a reader sees it too.
    assert_eq!(head(&package), committed);

    // A second save of the same run expects the pre-save head: the owner
    // refuses it as stale and writes nothing.
    let (stale, executed) = serve(&mut owner, &mut endpoint, None, || {
        save_tracked(&package, &outcome, &id, "Square", true)
    });
    let stale = stale.unwrap_err().to_string();
    assert!(stale.starts_with("RevisionConflict"), "{stale}");
    assert_eq!(executed, 1);
    assert_eq!(owner.snapshot().unwrap(), committed);

    // Re-tracking against the current head and replacing succeeds remotely.
    let again = track(&package, fixture.name);
    let (replaced, _) = serve(&mut owner, &mut endpoint, None, || {
        save_tracked(&package, &again, &id, "Square again", true)
    });
    let replaced = replaced.unwrap();
    assert_eq!(replaced["replaced"], true);
    assert!(replaced["receipt"].get("host_refresh_error").is_none());
    assert_eq!(
        owner.snapshot().unwrap().targets()[&id].label,
        "Square again"
    );

    // Without an advertised endpoint the held writer is never bypassed.
    drop(endpoint);
    let latest = owner.snapshot().unwrap();
    let orphan = track(&package, fixture.name);
    let error = save_tracked(
        &package,
        &orphan,
        &TargetId::new("other").unwrap(),
        "X",
        false,
    )
    .unwrap_err()
    .to_string();
    // Dropping the endpoint revokes its discovery record: the client refuses
    // as a changed owner instead of opening a second writer or replaying.
    assert!(
        error.starts_with("HostOwnerChanged") || error.starts_with("HostOwnerUnavailable"),
        "{error}"
    );
    assert_eq!(owner.snapshot().unwrap(), latest);

    // Undo on the owner reverts the remote saves like any edit.
    drop(owner);
    for expected in [Some("Square"), None] {
        let current = head(&package);
        cli(&[
            "project",
            "undo",
            package.to_str().unwrap(),
            "--expected",
            current.revision_id().as_str(),
        ]);
        assert_eq!(
            head(&package).targets().get(&id).map(|t| t.label.as_str()),
            expected
        );
    }
}
