use super::*;

fn rect(x: f64, y: f64, width: f64, height: f64) -> NormalizedRect {
    NormalizedRect::new(x, y, width, height).unwrap()
}

fn seen(region: NormalizedRect) -> RegionObservation {
    RegionObservation::Tracked {
        region,
        confidence: 0.95,
    }
}

fn missing() -> RegionObservation {
    RegionObservation::Unavailable {
        reason: RegionObservationUnavailableReason::Missing,
    }
}

fn batch(pts: &[i64], seeds: RegionSeeds, regions: &[RegionObservation]) -> RawRegionBatch {
    RawRegionBatch {
        schema_version: RAW_REGION_SCHEMA_VERSION,
        seeds,
        left: seen(seeds.left),
        frames: pts
            .iter()
            .zip(regions)
            .enumerate()
            .map(|(index, (&pts, &observation))| RawRegionFrame {
                ordinal: u32::try_from(index).unwrap(),
                pts,
                observation,
            })
            .collect(),
        right: seen(seeds.right),
    }
}

fn static_batch(regions: &[RegionObservation]) -> RawRegionBatch {
    let subject = rect(0.20, 0.30, 0.15, 0.20);
    let pts: Vec<_> = (0..regions.len())
        .map(|index| i64::try_from(index).unwrap() * 42)
        .collect();
    batch(
        &pts,
        RegionSeeds {
            left: subject,
            right: subject,
        },
        regions,
    )
}

fn evaluate(batch: &RawRegionBatch) -> GeneratedRegionReport {
    let pts: Vec<_> = batch.frames.iter().map(|frame| frame.pts).collect();
    analyze(batch, &pts, [512, 320], [0, 0, 512, 320]).unwrap()
}

fn stable() -> RegionObservation {
    seen(rect(0.20, 0.30, 0.15, 0.20))
}

fn drifted() -> RegionObservation {
    seen(rect(0.55, 0.30, 0.15, 0.20))
}

#[test]
fn stable_native_path_uses_the_exact_pts_fraction() {
    let left = rect(0.10, 0.20, 0.10, 0.20);
    let right = rect(0.65, 0.25, 0.20, 0.30);
    let pts = [10, 11, 99, 110];
    let regions = [
        seen(left),
        seen(rect(0.1055, 0.2005, 0.101, 0.201)),
        seen(rect(0.5895, 0.2445, 0.189, 0.289)),
        seen(right),
    ];
    let raw = batch(&pts, RegionSeeds { left, right }, &regions);
    let report = evaluate(&raw);
    assert_eq!(report.status, RegionCheckStatus::Measured);
    assert_eq!(report.measured_frames, 4);
    assert_eq!(report.unavailable_frames, 0);
    assert!(report.left_boundary.measured && report.right_boundary.measured);
    assert!(report.maximum_center_residual.unwrap() < 1e-14);
    assert!(report.maximum_log_size_residual.unwrap() < 1e-14);
    assert!(report.unavailable_reasons.is_empty());
    assert_eq!(report.thresholds, RegionThresholds::policy());
    report
        .validate_recomputed(&raw, &pts, [512, 320], [0, 0, 512, 320])
        .unwrap();
}

#[test]
fn sustained_drift_reports_the_earliest_run() {
    let raw = static_batch(&[
        stable(),
        drifted(),
        drifted(),
        stable(),
        drifted(),
        drifted(),
    ]);
    let report = evaluate(&raw);
    assert_eq!(report.status, RegionCheckStatus::Rejected);
    let rejection = report.rejection.unwrap();
    assert_eq!(rejection.first_ordinal, 1);
    assert_eq!(rejection.consecutive_frames, 2);
    assert!(rejection.center_residual > report.thresholds.center_residual);
    assert_eq!(report.measured_frames, 6);
}

#[test]
fn isolated_spikes_do_not_form_a_sustained_rejection() {
    let report = evaluate(&static_batch(&[drifted(), stable(), drifted(), stable()]));
    assert_eq!(report.status, RegionCheckStatus::Measured);
    assert!(report.rejection.is_none());
    assert!(report.maximum_center_residual.unwrap() > report.thresholds.center_residual);
}

#[test]
fn two_native_frames_are_enough_for_a_sustained_rejection() {
    let report = evaluate(&static_batch(&[drifted(), drifted()]));
    assert_eq!(report.rejection.unwrap().first_ordinal, 0);
    assert_eq!(report.measured_frames, 2);
}

#[test]
fn missing_suffix_keeps_the_earlier_rejection_and_stops_coverage() {
    let raw = static_batch(&[
        stable(),
        drifted(),
        drifted(),
        missing(),
        stable(),
        drifted(),
    ]);
    let report = evaluate(&raw);
    assert_eq!(report.status, RegionCheckStatus::Rejected);
    assert_eq!(report.rejection.unwrap().first_ordinal, 1);
    assert_eq!(report.measured_frames, 3);
    assert_eq!(report.unavailable_frames, 3);
    assert_eq!(report.first_unavailable_ordinal, Some(3));
    assert_eq!(
        report.right_boundary.unavailable_reason,
        Some(RegionUnavailableReason::LostTrack)
    );
    assert_eq!(
        report.unavailable_reasons,
        vec![
            RegionUnavailableReason::Missing,
            RegionUnavailableReason::LostTrack
        ]
    );
}

#[test]
fn weak_suffix_keeps_earlier_rejection_without_accepting_later_confidence() {
    let weak = RegionObservation::Tracked {
        region: rect(0.2, 0.3, 0.15, 0.2),
        confidence: 0.69,
    };
    let report = evaluate(&static_batch(&[
        drifted(),
        drifted(),
        weak,
        stable(),
        stable(),
    ]));
    assert_eq!(report.status, RegionCheckStatus::Rejected);
    assert_eq!(report.measured_frames, 2);
    assert_eq!(report.unavailable_frames, 3);
    assert_eq!(report.first_unavailable_ordinal, Some(2));
    assert!(
        report
            .unavailable_reasons
            .contains(&RegionUnavailableReason::LowConfidence)
    );
}

#[test]
fn confidence_lower_bound_is_inclusive_in_the_native_f32_domain() {
    let subject = rect(0.20, 0.30, 0.15, 0.20);
    let at_limit = RegionObservation::Tracked {
        region: subject,
        confidence: 0.70_f32,
    };
    let below_limit = RegionObservation::Tracked {
        region: subject,
        confidence: 0.70_f32.next_down(),
    };
    let mut raw = static_batch(&[at_limit, at_limit, at_limit]);
    raw.left = at_limit;
    raw.right = at_limit;
    let report = evaluate(&raw);
    assert_eq!(report.measured_frames, 3);
    assert_eq!(report.unavailable_frames, 0);
    assert!(report.left_boundary.measured && report.right_boundary.measured);

    let mut weak_left = raw.clone();
    weak_left.left = below_limit;
    let report = evaluate(&weak_left);
    assert_eq!(report.measured_frames, 0);
    assert_eq!(
        report.left_boundary.unavailable_reason,
        Some(RegionUnavailableReason::LowConfidence)
    );

    let mut weak_frame = raw.clone();
    weak_frame.frames[1].observation = below_limit;
    let report = evaluate(&weak_frame);
    assert_eq!(report.measured_frames, 1);
    assert_eq!(report.unavailable_frames, 2);
    assert_eq!(report.first_unavailable_ordinal, Some(1));

    raw.right = below_limit;
    let report = evaluate(&raw);
    assert_eq!(report.measured_frames, 3);
    assert_eq!(
        report.right_boundary.unavailable_reason,
        Some(RegionUnavailableReason::LowConfidence)
    );
}

#[test]
fn missing_or_weak_prefix_never_restarts_on_later_boxes() {
    for first in [
        missing(),
        RegionObservation::Tracked {
            region: rect(0.2, 0.3, 0.15, 0.2),
            confidence: 0.0,
        },
    ] {
        let report = evaluate(&static_batch(&[first, drifted(), drifted(), stable()]));
        assert_eq!(report.status, RegionCheckStatus::Unavailable);
        assert_eq!(report.measured_frames, 0);
        assert_eq!(report.unavailable_frames, 4);
        assert_eq!(report.first_unavailable_ordinal, Some(0));
        assert_eq!(report.maximum_center_residual, None);
        assert_eq!(report.maximum_log_size_residual, None);
        assert!(report.rejection.is_none());
    }
}

#[test]
fn an_interrupted_spike_cannot_join_a_later_spike() {
    let report = evaluate(&static_batch(&[drifted(), missing(), drifted(), drifted()]));
    assert_eq!(report.status, RegionCheckStatus::Measured);
    assert_eq!(report.measured_frames, 1);
    assert_eq!(report.unavailable_frames, 3);
    assert!(report.rejection.is_none());
}

#[test]
fn each_raw_unavailability_reason_remains_visible() {
    let reasons = [
        (
            RegionObservationUnavailableReason::Missing,
            RegionUnavailableReason::Missing,
        ),
        (
            RegionObservationUnavailableReason::InvalidGeometry,
            RegionUnavailableReason::InvalidGeometry,
        ),
        (
            RegionObservationUnavailableReason::LowConfidence,
            RegionUnavailableReason::LowConfidence,
        ),
        (
            RegionObservationUnavailableReason::LostTrack,
            RegionUnavailableReason::LostTrack,
        ),
        (
            RegionObservationUnavailableReason::Unsupported,
            RegionUnavailableReason::Unsupported,
        ),
    ];
    for (raw, expected) in reasons {
        let report = evaluate(&static_batch(&[
            stable(),
            RegionObservation::Unavailable { reason: raw },
        ]));
        assert_eq!(report.measured_frames, 1);
        assert!(report.unavailable_reasons.contains(&expected));
    }
}

#[test]
fn left_boundary_loss_or_seed_mismatch_prevents_native_measurement() {
    for observation in [
        missing(),
        drifted(),
        RegionObservation::Tracked {
            region: rect(0.2, 0.3, 0.15, 0.2),
            confidence: 0.2,
        },
    ] {
        let mut raw = static_batch(&[stable(), drifted(), drifted()]);
        raw.left = observation;
        let report = evaluate(&raw);
        assert_eq!(report.status, RegionCheckStatus::Unavailable);
        assert_eq!(report.measured_frames, 0);
        assert_eq!(report.unavailable_frames, 3);
        assert!(!report.left_boundary.measured);
        assert!(report.left_boundary.unavailable_reason.is_some());
    }
    let mut raw = static_batch(&[stable(), stable()]);
    raw.left = drifted();
    let report = evaluate(&raw);
    assert_eq!(
        report.left_boundary.unavailable_reason,
        Some(RegionUnavailableReason::BoundarySeedMismatch)
    );
    assert!(report.left_boundary.center_residual.unwrap() > report.thresholds.center_residual);
}

#[test]
fn right_boundary_loss_or_mismatch_does_not_erase_native_rejection() {
    for observation in [
        missing(),
        drifted(),
        RegionObservation::Tracked {
            region: rect(0.2, 0.3, 0.15, 0.2),
            confidence: 0.2,
        },
    ] {
        let mut raw = static_batch(&[stable(), drifted(), drifted()]);
        raw.right = observation;
        let report = evaluate(&raw);
        assert_eq!(report.status, RegionCheckStatus::Rejected);
        assert_eq!(report.measured_frames, 3);
        assert_eq!(report.unavailable_frames, 0);
        assert_eq!(report.first_unavailable_ordinal, None);
        assert!(!report.right_boundary.measured);
        assert!(report.right_boundary.unavailable_reason.is_some());
    }
}

#[test]
fn log_size_policy_catches_aspect_drift_even_when_area_is_unchanged() {
    let seed = rect(0.30, 0.30, 0.10, 0.20);
    let changed = rect(0.25, 0.35, 0.20, 0.10);
    let raw = batch(
        &[0, 42],
        RegionSeeds {
            left: seed,
            right: seed,
        },
        &[seen(changed), seen(changed)],
    );
    let report = evaluate(&raw);
    assert_eq!(report.status, RegionCheckStatus::Rejected);
    assert!(report.maximum_center_residual.unwrap() < 1e-14);
    assert!((report.maximum_log_size_residual.unwrap() - 4.0_f64.ln()).abs() < 1e-14);
}

fn pixel_rect(x: f64, y: f64, width: f64, height: f64, raster: [u32; 2]) -> NormalizedRect {
    rect(
        x / f64::from(raster[0]),
        y / f64::from(raster[1]),
        width / f64::from(raster[0]),
        height / f64::from(raster[1]),
    )
}

#[test]
fn odd_non_square_crop_preserves_equal_pixel_distances_on_both_axes() {
    let raster = [901, 301];
    let crop = [7, 13, 880, 270];
    let seed = pixel_rect(110.0, 90.0, 60.0, 50.0, raster);
    let pts = [0, 42];
    let make = |actual| {
        batch(
            &pts,
            RegionSeeds {
                left: seed,
                right: seed,
            },
            &[seen(actual), seen(actual)],
        )
    };
    let horizontal = analyze(
        &make(pixel_rect(180.0, 90.0, 60.0, 50.0, raster)),
        &pts,
        raster,
        crop,
    )
    .unwrap();
    let vertical = analyze(
        &make(pixel_rect(110.0, 160.0, 60.0, 50.0, raster)),
        &pts,
        raster,
        crop,
    )
    .unwrap();
    let expected = 70.0 / 880.0_f64.hypot(270.0);
    assert!((horizontal.maximum_center_residual.unwrap() - expected).abs() < 1e-14);
    assert!((vertical.maximum_center_residual.unwrap() - expected).abs() < 1e-14);
    assert_eq!(horizontal.status, RegionCheckStatus::Measured);
    assert_eq!(vertical.status, RegionCheckStatus::Measured);
}

#[test]
fn padding_is_unavailable_and_cannot_be_a_seed() {
    let raster = [512, 320];
    let crop = [0, 40, 512, 240];
    let mut raw = static_batch(&[stable(), seen(rect(0.2, 0.01, 0.15, 0.2)), drifted()]);
    let pts = [0, 42, 84];
    let report = analyze(&raw, &pts, raster, crop).unwrap();
    assert_eq!(report.measured_frames, 1);
    assert_eq!(report.unavailable_frames, 2);
    assert!(
        report
            .unavailable_reasons
            .contains(&RegionUnavailableReason::OutsidePresentation)
    );
    raw.seeds.right = rect(0.2, 0.01, 0.15, 0.2);
    assert_eq!(
        analyze(&raw, &pts, raster, crop),
        Err(RegionError::SeedsOutsidePresentation)
    );
}

#[test]
fn signed_pts_extremes_do_not_overflow() {
    let raw = batch(
        &[i64::MIN, -1, i64::MAX],
        RegionSeeds {
            left: rect(0.1, 0.2, 0.1, 0.2),
            right: rect(0.7, 0.2, 0.1, 0.2),
        },
        &[
            seen(rect(0.1, 0.2, 0.1, 0.2)),
            seen(rect(0.4, 0.2, 0.1, 0.2)),
            seen(rect(0.7, 0.2, 0.1, 0.2)),
        ],
    );
    let report = evaluate(&raw);
    assert_eq!(report.measured_frames, 3);
    assert!(report.maximum_center_residual.unwrap() < 1e-14);
}

#[test]
fn raw_validation_rejects_schema_count_order_ordinal_pts_and_confidence_changes() {
    let original = static_batch(&[stable(), stable()]);
    let pts = [0, 42];
    let mut raw = original.clone();
    raw.schema_version += 1;
    assert_eq!(raw.validate(&pts), Err(RawRegionError::SchemaVersion));
    assert_eq!(
        original.validate(&[0]),
        Err(RawRegionError::NativeFrameLimit)
    );
    assert_eq!(
        original.validate(&vec![0; MAX_NATIVE_FRAMES + 1]),
        Err(RawRegionError::NativeFrameLimit)
    );
    assert_eq!(
        original.validate(&[42, 42]),
        Err(RawRegionError::ExpectedPtsOrder)
    );
    assert_eq!(
        original.validate(&[42, 0]),
        Err(RawRegionError::ExpectedPtsOrder)
    );
    raw = original.clone();
    raw.frames.pop();
    assert_eq!(raw.validate(&pts), Err(RawRegionError::FrameCount));
    raw = original.clone();
    raw.frames[1].ordinal = 0;
    assert_eq!(raw.validate(&pts), Err(RawRegionError::FrameOrdinal));
    raw = original.clone();
    raw.frames[1].pts = 43;
    assert_eq!(raw.validate(&pts), Err(RawRegionError::FramePts));
    for confidence in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        raw = original.clone();
        raw.frames[1].observation = RegionObservation::Tracked {
            region: original.seeds.left,
            confidence,
        };
        assert_eq!(raw.validate(&pts), Err(RawRegionError::Confidence));
        raw.frames[1].observation = stable();
        raw.right = RegionObservation::Tracked {
            region: original.seeds.left,
            confidence,
        };
        assert_eq!(raw.validate(&pts), Err(RawRegionError::Confidence));
    }
}

#[test]
fn malformed_geometry_and_unknown_raw_fields_are_rejected() {
    let raw = static_batch(&[stable(), stable()]);
    let value = serde_json::to_value(&raw).unwrap();
    for path in [
        vec!["unexpected"],
        vec!["seeds", "unexpected"],
        vec!["left", "unexpected"],
    ] {
        let mut altered = value.clone();
        let mut target = &mut altered;
        for key in &path[..path.len() - 1] {
            target = &mut target[*key];
        }
        target[path[path.len() - 1]] = serde_json::json!(true);
        assert!(serde_json::from_value::<RawRegionBatch>(altered).is_err());
    }
    let mut altered = value.clone();
    altered["frames"][0]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<RawRegionBatch>(altered).is_err());
    let mut altered = value.clone();
    altered["seeds"]["left"]["width"] = serde_json::json!(0.0);
    assert!(serde_json::from_value::<RawRegionBatch>(altered).is_err());
    let mut altered = value;
    // The shared rectangle's rotation tolerance cannot admit raster padding.
    altered["seeds"]["left"]["x"] = serde_json::json!(-1e-10);
    let altered: RawRegionBatch = serde_json::from_value(altered).unwrap();
    assert_eq!(
        altered.validate(&[0, 42]),
        Err(RawRegionError::RegionBounds)
    );
}

#[test]
fn geometry_bounds_fail_before_policy_evaluation() {
    let raw = static_batch(&[stable(), stable()]);
    let pts = [0, 42];
    for raster in [
        [0, 320],
        [512, 0],
        [4097, 1],
        [1, 4097],
        [u32::MAX, u32::MAX],
    ] {
        assert_eq!(
            analyze(&raw, &pts, raster, [0, 0, 512, 320]),
            Err(RegionError::Raster)
        );
    }
    for crop in [
        [0, 0, 0, 320],
        [0, 0, 512, 0],
        [1, 0, 512, 320],
        [0, 1, 512, 320],
        [u32::MAX, 0, 2, 1],
    ] {
        assert_eq!(
            analyze(&raw, &pts, [512, 320], crop),
            Err(RegionError::Presentation)
        );
    }
}

#[test]
fn maximum_frame_count_has_complete_bounded_coverage() {
    let raw = static_batch(&vec![stable(); MAX_NATIVE_FRAMES]);
    let report = evaluate(&raw);
    assert_eq!(
        report.measured_frames,
        u32::try_from(MAX_NATIVE_FRAMES).unwrap()
    );
    assert_eq!(report.unavailable_frames, 0);
}

#[test]
fn recomputation_rejects_altered_raw_evidence_and_retained_policy() {
    let raw = static_batch(&[stable(), drifted(), drifted()]);
    let pts = [0, 42, 84];
    let report = evaluate(&raw);
    let verify = |report: &GeneratedRegionReport, batch: &RawRegionBatch| {
        report.validate_recomputed(batch, &pts, [512, 320], [0, 0, 512, 320])
    };
    assert!(verify(&report, &raw).is_ok());
    let mut altered = raw.clone();
    altered.frames[1].observation = stable();
    assert_eq!(verify(&report, &altered), Err(RegionError::PolicyMismatch));
    let mut altered = report.clone();
    altered.thresholds.center_residual = 1.0;
    assert_eq!(verify(&altered, &raw), Err(RegionError::PolicyMismatch));
    altered = report.clone();
    altered.measured_frames = 0;
    assert_eq!(verify(&altered, &raw), Err(RegionError::PolicyMismatch));
    altered = report.clone();
    altered.rejection = None;
    assert_eq!(verify(&altered, &raw), Err(RegionError::PolicyMismatch));
    altered = report.clone();
    altered.profile.push_str("-altered");
    assert_eq!(verify(&altered, &raw), Err(RegionError::PolicyMismatch));
    let serialized = serde_json::to_vec(&report).unwrap();
    let restored: GeneratedRegionReport = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(
        restored, report,
        "stored floating-point metrics must round-trip exactly"
    );
    let raw_bytes = serde_json::to_vec(&raw).unwrap();
    let restored_raw: RawRegionBatch = serde_json::from_slice(&raw_bytes).unwrap();
    assert_eq!(restored_raw, raw, "native boxes must round-trip exactly");
    assert!(verify(&restored, &restored_raw).is_ok());
}
