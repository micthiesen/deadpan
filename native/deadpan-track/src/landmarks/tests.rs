use super::*;

#[test]
fn observation_writer_checks_the_budget_before_adding_a_chunk() {
    let mut output = BoundedBytes {
        bytes: Vec::new(),
        maximum: 3,
    };
    output.write_all(b"ab").unwrap();
    assert!(output.write_all(b"cd").is_err());
    assert_eq!(output.bytes, b"ab");
    output.write_all(b"c").unwrap();
    assert_eq!(output.bytes, b"abc");
}

#[test]
fn raw_region_boxes_convert_coordinates_without_clamping_invalid_measurements() {
    let RegionObservation::Tracked { region, confidence } =
        measured_region(0.1, 0.2, 0.3, 0.4, 0.6)
    else {
        panic!()
    };
    assert_eq!(region, NormalizedRect::new(0.1, 0.4, 0.3, 0.4).unwrap());
    assert_eq!(confidence, 0.6);
    for (x, y, width, height, confidence) in [
        (-0.1, 0.2, 0.3, 0.4, 0.8),
        (0.1, 0.9, 0.3, 0.4, 0.8),
        (0.1, 0.2, 0.0, 0.4, 0.8),
        (0.1, 0.2, 0.3, 0.4, f32::NAN),
        (f64::NAN, 0.2, 0.3, 0.4, 0.8),
        (0.1, 0.2, 0.3, 0.4, 1.1),
    ] {
        assert_eq!(
            measured_region(x, y, width, height, confidence),
            RegionObservation::Unavailable {
                reason: RegionObservationUnavailableReason::InvalidGeometry
            }
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_lost_region_tracker_never_reuses_an_old_box_or_restarts_on_later_pixels() {
    let mut tracker = RegionTracker { inner: None };
    for last in [false, true] {
        assert_eq!(
            tracker
                .track_picture(1, 1, 4, &[255, 255, 255, 255], last)
                .unwrap(),
            RegionObservation::Unavailable {
                reason: RegionObservationUnavailableReason::LostTrack
            }
        );
    }
}
