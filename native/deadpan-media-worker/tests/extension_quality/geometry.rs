//! Real canonicalization, supervised Vision inspection and retained admission.
use super::*;
use deadpan_models::{ExtensionGeometryChecks, inspect_extension_geometry};
use std::path::{Path, PathBuf};

#[path = "geometry/selected.rs"]
mod selected;

fn tracker() -> PathBuf {
    let path =
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")).with_file_name("deadpan-track");
    assert!(
        path.is_file(),
        "build deadpan-track beside the media worker: {}",
        path.display()
    );
    path
}

#[test]
fn extension_geometry_uses_retained_anchor_and_only_generated_native_observations() {
    for direction in DIRECTIONS {
        let Fixture {
            directory,
            mut conditioning,
            request,
            media,
            ..
        } = Fixture::new(
            direction,
            1,
            [0; GENERATED as usize],
            [255; CONTEXT as usize],
            true,
        );
        fs::write(
            directory.path().join("inputs/black.png"),
            b"worker changed input",
        )
        .unwrap();
        let (mut native, _, _) = media.into_parts();
        let report = inspect_extension_geometry(
            &tracker(),
            &mut native,
            &mut conditioning,
            &request,
            Instant::now() + Duration::from_secs(30),
            &AtomicBool::new(false),
        )
        .unwrap();
        report
            .validate_for(&native, &conditioning, &request)
            .unwrap();
        assert!(report.region().is_none());
        assert_eq!(
            report.region_unavailable_reason(),
            Some("no selected region target")
        );
        let wire = serde_json::to_value(&report).unwrap();
        assert_eq!(
            wire["observations"]["landmarks"]["frames"]
                .as_array()
                .unwrap()
                .len(),
            GENERATED as usize
        );
        let start = if direction == ExtensionDirection::FromLeft {
            CONTEXT
        } else {
            0
        };
        for (offset, frame) in wire["observations"]["landmarks"]["frames"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            assert_eq!(frame["ordinal"], start + offset as u32);
        }
        assert!(
            wire["observations"]["landmarks"]
                .get("boundaries")
                .is_none()
        );
        assert!(wire["observations"]["landmarks"].get("anchor").is_some());
        let retained: ExtensionGeometryChecks = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(retained, report);
        for field in ["region", "region_runtime", "region_unavailable_reason"] {
            let mut omitted = wire.clone();
            omitted.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ExtensionGeometryChecks>(omitted).is_err(),
                "{field}"
            );
        }
        for (pointer, value) in [
            ("/native/width", serde_json::json!(384)),
            ("/runtime/request_revision", serde_json::json!(99)),
            (
                "/observations/landmarks/frames/0/pts",
                serde_json::json!(-100),
            ),
            (
                "/geometry/thresholds/center_residual",
                serde_json::json!(1.0),
            ),
            (
                "/region_unavailable_reason",
                serde_json::json!("inference succeeded"),
            ),
        ] {
            let mut changed = wire.clone();
            *changed.pointer_mut(pointer).expect(pointer) = value;
            match serde_json::from_value::<ExtensionGeometryChecks>(changed) {
                Err(_) => {}
                Ok(changed) => assert!(
                    changed
                        .validate_for(&native, &conditioning, &request)
                        .is_err(),
                    "{pointer}"
                ),
            }
        }
        let other = Fixture::new(
            direction,
            1,
            [16; GENERATED as usize],
            [255; CONTEXT as usize],
            true,
        );
        assert!(
            report
                .validate_for(other.media.native(), &conditioning, &request)
                .is_err()
        );
    }
}

#[test]
fn extension_selected_original_region_is_measured_and_real_drift_is_rejected() {
    for direction in DIRECTIONS {
        let mut fixture = selected::SelectedFixture::new(direction, true);
        let drifted = fixture.drifted_media();
        // Preserve source authenticity: these were production-captured PNGs
        // from a measured Original before their worker-writable copies changed.
        fs::remove_dir_all(fixture.directory.path().join("inputs")).unwrap();
        let (mut native, _, _) = fixture.media.into_parts();
        let report = inspect_extension_geometry(
            &tracker(),
            &mut native,
            &mut fixture.conditioning,
            &fixture.request,
            Instant::now() + Duration::from_secs(60),
            &AtomicBool::new(false),
        )
        .unwrap();
        report
            .validate_for(&native, &fixture.conditioning, &fixture.request)
            .unwrap();
        assert!(report.geometry().anchor_usable);
        assert!(report.region_unavailable_reason().is_none());
        let region = report
            .region()
            .expect("available selected target is actually tracked");
        assert_eq!(region.measured_frames, GENERATED);
        assert_eq!(region.unavailable_frames, 0);
        assert!(region.anchor.measured);
        assert!(region.rejection.is_none());
        let wire = serde_json::to_value(&report).unwrap();
        assert_eq!(wire["region"]["status"], "measured");
        assert!(wire["region_runtime"].is_object());
        assert_eq!(
            wire["observations"]["region"]["frames"]
                .as_array()
                .unwrap()
                .len(),
            GENERATED as usize
        );
        let restored: ExtensionGeometryChecks = serde_json::from_value(wire.clone()).unwrap();
        restored
            .validate_for(&native, &fixture.conditioning, &fixture.request)
            .unwrap();
        for (pointer, value) in [
            ("/region/measured_frames", serde_json::json!(0)),
            ("/region/thresholds/center_residual", serde_json::json!(1.0)),
            ("/region_runtime", serde_json::Value::Null),
            ("/observations/region/seed/width", serde_json::json!(0.01)),
        ] {
            let mut altered = wire.clone();
            *altered.pointer_mut(pointer).expect(pointer) = value;
            match serde_json::from_value::<ExtensionGeometryChecks>(altered) {
                Err(_) => {}
                Ok(altered) => assert!(
                    altered
                        .validate_for(&native, &fixture.conditioning, &fixture.request)
                        .is_err(),
                    "{pointer}"
                ),
            }
        }
        let (mut drifted, _, _) = drifted.into_parts();
        let error = inspect_extension_geometry(
            &tracker(),
            &mut drifted,
            &mut fixture.conditioning,
            &fixture.request,
            Instant::now() + Duration::from_secs(60),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        let QualificationError::Quality(reason) = error else {
            panic!("expected measured rejection, got {error}");
        };
        assert!(reason.contains("gross selected-region drift"), "{reason}");
    }
}

#[test]
fn extension_selected_target_outside_anchor_span_is_explicitly_unavailable() {
    for direction in DIRECTIONS {
        let mut fixture = selected::SelectedFixture::new(direction, false);
        assert!(
            fixture
                .conditioning
                .context()
                .region()
                .target_id()
                .is_some()
        );
        let expected = fixture
            .conditioning
            .context()
            .region()
            .unavailable_reason()
            .unwrap();
        assert_ne!(expected, "no selected region target");
        let (mut native, _, _) = fixture.media.into_parts();
        let report = inspect_extension_geometry(
            &tracker(),
            &mut native,
            &mut fixture.conditioning,
            &fixture.request,
            Instant::now() + Duration::from_secs(60),
            &AtomicBool::new(false),
        )
        .unwrap();
        report
            .validate_for(&native, &fixture.conditioning, &fixture.request)
            .unwrap();
        assert!(report.geometry().anchor_usable);
        assert!(report.region().is_none());
        assert_eq!(report.region_unavailable_reason(), Some(expected.as_str()));
        let wire = serde_json::to_value(report).unwrap();
        assert!(wire["region_runtime"].is_null());
        assert!(wire["observations"]["region"].is_null());
        let restored: ExtensionGeometryChecks = serde_json::from_value(wire).unwrap();
        restored
            .validate_for(&native, &fixture.conditioning, &fixture.request)
            .unwrap();
    }
}

#[test]
fn extension_geometry_cancellation_deadline_and_native_mismatch_precede_launch() {
    let Fixture {
        mut conditioning,
        request,
        media,
        ..
    } = Fixture::new(
        ExtensionDirection::FromLeft,
        8,
        [0; GENERATED as usize],
        [0; CONTEXT as usize],
        false,
    );
    let (mut native, _, _) = media.into_parts();
    let missing = Path::new("/missing/deadpan-track");
    assert!(matches!(
        inspect_extension_geometry(
            missing,
            &mut native,
            &mut conditioning,
            &request,
            Instant::now() + Duration::from_secs(30),
            &AtomicBool::new(true)
        ),
        Err(QualificationError::Cancelled)
    ));
    assert!(matches!(
        inspect_extension_geometry(
            missing,
            &mut native,
            &mut conditioning,
            &request,
            Instant::now(),
            &AtomicBool::new(false)
        ),
        Err(QualificationError::Deadline)
    ));
    let directory = tempfile::tempdir().unwrap();
    let other_plan = plan_with_dimensions(ExtensionDirection::FromLeft, 8, WIDTH * 2, HEIGHT * 2);
    let (other_request, mut other_conditioning) =
        retained_inputs(directory.path(), &other_plan, false);
    let error = inspect_extension_geometry(
        missing,
        &mut native,
        &mut other_conditioning,
        &other_request,
        Instant::now() + Duration::from_secs(30),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("private native movie differs"),
        "{error}"
    );
}
