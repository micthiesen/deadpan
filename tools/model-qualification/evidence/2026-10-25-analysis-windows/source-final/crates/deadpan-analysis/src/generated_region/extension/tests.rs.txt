use super::*;
use crate::generated_extension::MAX_NATIVE_FRAMES;
use crate::generated_region::RegionObservationUnavailableReason;
use deadpan_core::ExtensionDirection;

const RASTER: [u32; 2] = [512, 320];
const PRESENTATION: [u32; 4] = [0, 0, 512, 320];
const DIRECTIONS: [ExtensionDirection; 2] =
    [ExtensionDirection::FromLeft, ExtensionDirection::FromRight];

fn rect(x: f64, y: f64, width: f64, height: f64) -> NormalizedRect {
    NormalizedRect::new(x, y, width, height).unwrap()
}

fn seed() -> NormalizedRect {
    rect(0.20, 0.30, 0.15, 0.20)
}

fn seen(region: NormalizedRect) -> RegionObservation {
    RegionObservation::Tracked {
        region,
        confidence: 0.95,
    }
}

fn stable() -> RegionObservation {
    seen(seed())
}

fn drifted() -> RegionObservation {
    seen(rect(0.55, 0.30, 0.15, 0.20))
}

fn missing() -> RegionObservation {
    RegionObservation::Unavailable {
        reason: RegionObservationUnavailableReason::Missing,
    }
}

/// Input observations follow the tracker outward; wire frames always retain
/// chronological native ordinals and PTS, including the context offset.
fn batch(
    direction: ExtensionDirection,
    context_count: usize,
    outward: &[RegionObservation],
) -> (RawExtensionRegionBatch, Vec<i64>) {
    let native_count = context_count + outward.len();
    let pts: Vec<_> = (0..native_count)
        .map(|index| -91 + i64::try_from(index).unwrap() * 42)
        .collect();
    let coverage = ExtensionCoverage {
        direction,
        start: match direction {
            ExtensionDirection::FromLeft => context_count as u32,
            ExtensionDirection::FromRight => 0,
        },
        end: match direction {
            ExtensionDirection::FromLeft => native_count as u32,
            ExtensionDirection::FromRight => outward.len() as u32,
        },
    };
    let mut frames: Vec<_> = coverage
        .tracking_ordinals()
        .into_iter()
        .zip(outward)
        .map(|(ordinal, &observation)| RawRegionFrame {
            ordinal,
            pts: pts[ordinal as usize],
            observation,
        })
        .collect();
    frames.sort_unstable_by_key(|frame| frame.ordinal);
    (
        RawExtensionRegionBatch {
            schema_version: RAW_EXTENSION_REGION_SCHEMA_VERSION,
            coverage,
            seed: seed(),
            anchor: stable(),
            frames,
        },
        pts,
    )
}

fn evaluate(raw: &RawExtensionRegionBatch, pts: &[i64]) -> ExtensionRegionReport {
    analyze(raw, pts, RASTER, PRESENTATION).unwrap()
}

#[test]
fn both_directions_measure_generated_pictures_only_on_canonical_clock() {
    for direction in DIRECTIONS {
        let (mut raw, mut pts) = batch(direction, 3, &[stable(); 4]);
        // Neither uneven PTS nor a nonzero origin changes the static seed.
        pts = pts.iter().map(|value| value * value * value).collect();
        for frame in &mut raw.frames {
            frame.pts = pts[frame.ordinal as usize];
        }
        let report = evaluate(&raw, &pts);
        assert_eq!(report.profile, PROFILE);
        assert_eq!(report.native_frames, 7);
        assert_eq!(report.generated_frames, 4);
        assert_eq!(report.measured_frames, 4);
        assert_eq!(report.unavailable_frames, 0);
        assert_eq!(report.coverage, raw.coverage);
        assert_eq!(report.status, RegionCheckStatus::Measured);
        assert_eq!(report.first_unavailable_ordinal, None);
        assert!(report.anchor.measured);
        assert!(report.maximum_center_residual.unwrap() < 1e-14);
        assert_eq!(report.maximum_log_size_residual, Some(0.0));
        assert!(report.rejection.is_none());
        assert!(report.unavailable_reasons.is_empty());
        assert_eq!(report.thresholds, RegionThresholds::policy());
        report
            .validate_recomputed(&raw, &pts, RASTER, PRESENTATION)
            .unwrap();
    }
}

#[test]
fn loss_stops_outward_tracking_permanently_in_both_directions() {
    for direction in DIRECTIONS {
        let (raw, pts) = batch(
            direction,
            2,
            &[stable(), missing(), stable(), drifted(), drifted()],
        );
        let report = evaluate(&raw, &pts);
        assert_eq!(report.status, RegionCheckStatus::Measured);
        assert_eq!(report.measured_frames, 1);
        assert_eq!(report.unavailable_frames, 4);
        assert_eq!(
            report.first_unavailable_ordinal,
            Some(raw.coverage.tracking_ordinals()[1])
        );
        assert!(report.rejection.is_none());
        assert_eq!(
            report.unavailable_reasons,
            [
                RegionUnavailableReason::Missing,
                RegionUnavailableReason::LostTrack
            ]
        );
        // The later drift cannot inflate a measured maximum after loss.
        assert!(report.maximum_center_residual.unwrap() < 1e-14);
    }
}

#[test]
fn sustained_rejection_before_loss_survives_with_chronological_start_ordinal() {
    for direction in DIRECTIONS {
        let (raw, pts) = batch(
            direction,
            3,
            &[stable(), drifted(), drifted(), missing(), stable()],
        );
        let report = evaluate(&raw, &pts);
        assert_eq!(report.status, RegionCheckStatus::Rejected);
        assert_eq!(report.measured_frames, 3);
        assert_eq!(report.unavailable_frames, 2);
        let rejection = report.rejection.unwrap();
        assert_eq!(
            rejection.first_ordinal,
            raw.coverage.tracking_ordinals()[1].min(raw.coverage.tracking_ordinals()[2])
        );
        assert_eq!(rejection.consecutive_frames, 2);
        assert!(rejection.center_residual > report.thresholds.center_residual);
        assert_eq!(
            report.first_unavailable_ordinal,
            Some(raw.coverage.tracking_ordinals()[3])
        );
    }
}

#[test]
fn isolated_and_interrupted_excess_cannot_form_a_sustained_rejection() {
    for direction in DIRECTIONS {
        for observations in [
            [drifted(), stable(), drifted(), stable()],
            [drifted(), missing(), drifted(), drifted()],
        ] {
            let (raw, pts) = batch(direction, 1, &observations);
            let report = evaluate(&raw, &pts);
            assert_eq!(report.status, RegionCheckStatus::Measured);
            assert!(report.rejection.is_none());
        }
    }
}

#[test]
fn one_generated_picture_is_measured_without_a_fabricated_second_endpoint() {
    for direction in DIRECTIONS {
        let (raw, pts) = batch(direction, 1, &[drifted()]);
        let report = evaluate(&raw, &pts);
        assert_eq!(report.native_frames, 2);
        assert_eq!(report.generated_frames, 1);
        assert_eq!(report.measured_frames, 1);
        assert_eq!(report.status, RegionCheckStatus::Measured);
        assert!(report.rejection.is_none());
        assert!(report.maximum_center_residual.unwrap() > report.thresholds.center_residual);
    }
}

#[test]
fn one_static_seed_rejects_drift_without_interpolating_toward_an_opposite_box() {
    for direction in DIRECTIONS {
        let (raw, pts) = batch(direction, 2, &[stable(), drifted(), drifted()]);
        let report = evaluate(&raw, &pts);
        assert_eq!(report.status, RegionCheckStatus::Rejected);
        assert_eq!(
            report.rejection.unwrap().first_ordinal,
            raw.coverage.tracking_ordinals()[1].min(raw.coverage.tracking_ordinals()[2])
        );
    }
}

#[test]
fn reverse_rejection_reports_the_lowest_measured_sustained_interval() {
    let (raw, pts) = batch(
        ExtensionDirection::FromRight,
        1,
        &[
            drifted(),
            drifted(),
            stable(),
            drifted(),
            drifted(),
            drifted(),
        ],
    );
    let rejection = evaluate(&raw, &pts).rejection.unwrap();
    assert_eq!(rejection.first_ordinal, 0);
    assert_eq!(rejection.consecutive_frames, 3);
    assert_eq!(rejection.first_ordinal + rejection.consecutive_frames, 3);
}

#[test]
fn bad_anchor_cannot_be_replaced_by_a_later_confident_box() {
    for direction in DIRECTIONS {
        for (anchor, reason) in [
            (missing(), RegionUnavailableReason::Missing),
            (drifted(), RegionUnavailableReason::BoundarySeedMismatch),
            (
                RegionObservation::Tracked {
                    region: seed(),
                    confidence: 0.70_f32.next_down(),
                },
                RegionUnavailableReason::LowConfidence,
            ),
        ] {
            let (mut raw, pts) = batch(direction, 1, &[stable(), drifted(), drifted()]);
            raw.anchor = anchor;
            let report = evaluate(&raw, &pts);
            assert_eq!(report.status, RegionCheckStatus::Unavailable);
            assert!(!report.anchor.measured);
            assert_eq!(report.anchor.unavailable_reason, Some(reason));
            assert_eq!(report.measured_frames, 0);
            assert_eq!(report.unavailable_frames, 3);
            assert_eq!(
                report.first_unavailable_ordinal,
                Some(raw.coverage.tracking_ordinals()[0])
            );
            assert_eq!(report.maximum_center_residual, None);
            assert_eq!(report.maximum_log_size_residual, None);
            assert!(report.rejection.is_none());
        }
    }
}

#[test]
fn inclusive_native_confidence_boundary_applies_to_anchor_and_generated_pictures() {
    let at_limit = RegionObservation::Tracked {
        region: seed(),
        confidence: 0.70_f32,
    };
    for direction in DIRECTIONS {
        let (mut raw, pts) = batch(direction, 1, &[at_limit; 3]);
        raw.anchor = at_limit;
        assert_eq!(evaluate(&raw, &pts).measured_frames, 3);
        let ordinal = raw.coverage.tracking_ordinals()[1];
        raw.frames[(ordinal - raw.coverage.start) as usize].observation =
            RegionObservation::Tracked {
                region: seed(),
                confidence: 0.70_f32.next_down(),
            };
        let report = evaluate(&raw, &pts);
        assert_eq!(report.measured_frames, 1);
        assert_eq!(report.unavailable_frames, 2);
        assert_eq!(report.first_unavailable_ordinal, Some(ordinal));
        assert!(
            report
                .unavailable_reasons
                .contains(&RegionUnavailableReason::LowConfidence)
        );
    }
}

#[test]
fn aspect_drift_uses_the_shared_log_size_threshold() {
    // Both width and height change by less than exp(0.4), but their ratio does not.
    let changed = seen(rect(0.18, 0.32, 0.19, 0.16));
    for direction in DIRECTIONS {
        let (raw, pts) = batch(direction, 1, &[changed; 2]);
        let report = evaluate(&raw, &pts);
        assert_eq!(report.status, RegionCheckStatus::Rejected);
        let rejection = report.rejection.unwrap();
        assert!(rejection.center_residual < report.thresholds.center_residual);
        assert!(rejection.log_size_residual > report.thresholds.log_size_residual);
    }
}

#[test]
fn observations_in_padding_end_tracking_and_invalid_seed_geometry_rejects() {
    let crop = [64, 32, 384, 256];
    let padding = seen(rect(0.01, 0.30, 0.08, 0.20));
    for direction in DIRECTIONS {
        let (raw, pts) = batch(direction, 1, &[stable(), padding, stable()]);
        let report = analyze(&raw, &pts, RASTER, crop).unwrap();
        assert_eq!(report.measured_frames, 1);
        assert_eq!(report.unavailable_frames, 2);
        assert!(
            report
                .unavailable_reasons
                .contains(&RegionUnavailableReason::OutsidePresentation)
        );
        let mut invalid = raw.clone();
        invalid.seed = rect(0.01, 0.30, 0.08, 0.20);
        assert_eq!(
            analyze(&invalid, &pts, RASTER, crop),
            Err(RegionError::SeedsOutsidePresentation.into())
        );
        for raster in [[0, 320], [4097, 320]] {
            assert_eq!(
                analyze(&raw, &pts, raster, crop),
                Err(RegionError::Raster.into())
            );
        }
        for crop in [[0, 0, 0, 320], [1, 0, 512, 320], [u32::MAX, 0, 2, 320]] {
            assert_eq!(
                analyze(&raw, &pts, RASTER, crop),
                Err(RegionError::Presentation.into())
            );
        }
    }
}

#[test]
fn raw_coverage_rejects_context_frames_wrong_clocks_and_reordered_observations() {
    for direction in DIRECTIONS {
        let (raw, pts) = batch(direction, 2, &[stable(); 3]);
        let mut extra = raw.clone();
        extra.frames.push(RawRegionFrame {
            ordinal: 0,
            pts: pts[0],
            observation: stable(),
        });
        assert_eq!(extra.validate(&pts), Err(RawRegionError::FrameCount.into()));
        let mut missing = raw.clone();
        missing.frames.pop();
        assert_eq!(
            missing.validate(&pts),
            Err(RawRegionError::FrameCount.into())
        );
        let mut duplicate = raw.clone();
        duplicate.frames[1] = duplicate.frames[0];
        assert_eq!(
            duplicate.validate(&pts),
            Err(RawRegionError::FrameOrdinal.into())
        );
        let mut reversed = raw.clone();
        reversed.frames.reverse();
        assert_eq!(
            reversed.validate(&pts),
            Err(RawRegionError::FrameOrdinal.into())
        );
        let mut wrong_pts = raw.clone();
        wrong_pts.frames[1].pts += 1;
        assert_eq!(
            wrong_pts.validate(&pts),
            Err(RawRegionError::FramePts.into())
        );
        let mut wrong_schema = raw.clone();
        wrong_schema.schema_version += 1;
        assert_eq!(
            wrong_schema.validate(&pts),
            Err(RawRegionError::SchemaVersion.into())
        );
        let mut contextless = raw.clone();
        contextless.coverage.start = 0;
        contextless.coverage.end = pts.len() as u32;
        assert_eq!(
            contextless.validate(&pts),
            Err(CoverageError::GeneratedInterval.into())
        );
        let mut wrong_direction = raw.clone();
        wrong_direction.coverage.direction = match direction {
            ExtensionDirection::FromLeft => ExtensionDirection::FromRight,
            ExtensionDirection::FromRight => ExtensionDirection::FromLeft,
        };
        assert_eq!(
            wrong_direction.validate(&pts),
            Err(CoverageError::GeneratedInterval.into())
        );
        let mut bad_clock = pts.clone();
        bad_clock[1] = bad_clock[0];
        assert_eq!(
            raw.validate(&bad_clock),
            Err(CoverageError::ExpectedPtsOrder.into())
        );
    }
}

#[test]
fn nonfinite_and_out_of_range_confidence_reject_even_after_track_loss() {
    let (raw, pts) = batch(ExtensionDirection::FromRight, 1, &[missing(), stable()]);
    for confidence in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        let mut invalid = raw.clone();
        invalid.frames[0].observation = RegionObservation::Tracked {
            region: seed(),
            confidence,
        };
        assert_eq!(
            invalid.validate(&pts),
            Err(RawRegionError::Confidence.into())
        );
        invalid = raw.clone();
        invalid.anchor = RegionObservation::Tracked {
            region: seed(),
            confidence,
        };
        assert_eq!(
            invalid.validate(&pts),
            Err(RawRegionError::Confidence.into())
        );
    }
}

#[test]
fn maximum_generated_and_context_counts_are_bounded_independently() {
    for direction in DIRECTIONS {
        let (raw, pts) = batch(
            direction,
            MAX_NATIVE_FRAMES - MAX_GENERATED_FRAMES,
            &vec![stable(); MAX_GENERATED_FRAMES],
        );
        let report = evaluate(&raw, &pts);
        assert_eq!(report.native_frames, MAX_NATIVE_FRAMES as u32);
        assert_eq!(report.measured_frames, MAX_GENERATED_FRAMES as u32);
        let (too_many, pts) = batch(direction, 1, &vec![stable(); MAX_GENERATED_FRAMES + 1]);
        assert_eq!(
            too_many.validate(&pts),
            Err(CoverageError::GeneratedInterval.into())
        );
        let (too_long, pts) = batch(direction, MAX_NATIVE_FRAMES, &[stable()]);
        assert_eq!(
            too_long.validate(&pts),
            Err(CoverageError::NativeFrameLimit.into())
        );
    }
}

#[test]
fn report_roundtrip_recomputes_and_rejects_changed_measurements_policy_and_coverage() {
    for direction in DIRECTIONS {
        let (raw, pts) = batch(direction, 2, &[drifted(), drifted(), missing()]);
        let report = evaluate(&raw, &pts);
        let decoded: ExtensionRegionReport =
            serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
        decoded
            .validate_recomputed(&raw, &pts, RASTER, PRESENTATION)
            .unwrap();
        let mut variants = vec![report.clone(); 10];
        variants[0].profile.push_str("-tampered");
        variants[1].schema_version += 1;
        variants[2].status = RegionCheckStatus::Measured;
        variants[3].thresholds.center_residual += 0.01;
        variants[4].measured_frames += 1;
        variants[5].coverage.start += 1;
        variants[6].anchor.measured = false;
        variants[7].rejection = None;
        variants[8].first_unavailable_ordinal = None;
        variants[9].maximum_center_residual = Some(0.0);
        for altered in variants {
            assert_eq!(
                altered.validate_recomputed(&raw, &pts, RASTER, PRESENTATION),
                Err(RegionError::PolicyMismatch.into())
            );
        }
        let mut altered = raw.clone();
        for frame in &mut altered.frames {
            frame.observation = stable();
        }
        assert_eq!(
            report.validate_recomputed(&altered, &pts, RASTER, PRESENTATION),
            Err(RegionError::PolicyMismatch.into())
        );
    }
}

#[test]
fn raw_wire_is_strict_and_bounded() {
    let (raw, _) = batch(ExtensionDirection::FromLeft, 1, &[stable()]);
    let value = serde_json::to_value(&raw).unwrap();
    let roundtrip: RawExtensionRegionBatch = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(roundtrip, raw);
    for pointer in [
        "",
        "/coverage",
        "/anchor",
        "/frames/0",
        "/frames/0/observation",
    ] {
        let mut altered = value.clone();
        altered
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<RawExtensionRegionBatch>(altered).is_err());
    }
    let mut oversized = value.clone();
    oversized["frames"] = serde_json::json!(vec![&raw.frames[0]; MAX_GENERATED_FRAMES + 1]);
    assert!(serde_json::from_value::<RawExtensionRegionBatch>(oversized).is_err());
    let mut absent_anchor = value.clone();
    absent_anchor.as_object_mut().unwrap().remove("anchor");
    assert!(serde_json::from_value::<RawExtensionRegionBatch>(absent_anchor).is_err());
    let mut fabricated_opposite = value;
    fabricated_opposite["opposite"] = serde_json::to_value(stable()).unwrap();
    assert!(serde_json::from_value::<RawExtensionRegionBatch>(fabricated_opposite).is_err());
}

#[test]
fn report_wire_requires_nullable_fields_and_rejects_unknown_nested_fields() {
    let (raw, pts) = batch(ExtensionDirection::FromLeft, 1, &[stable()]);
    let report = evaluate(&raw, &pts);
    let value = serde_json::to_value(report).unwrap();
    for key in [
        "first_unavailable_ordinal",
        "maximum_center_residual",
        "maximum_log_size_residual",
        "rejection",
    ] {
        let mut altered = value.clone();
        altered.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<ExtensionRegionReport>(altered).is_err());
    }
    for key in ["center_residual", "log_size_residual", "unavailable_reason"] {
        let mut altered = value.clone();
        altered["anchor"].as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<ExtensionRegionReport>(altered).is_err());
    }
    for pointer in ["", "/coverage", "/thresholds", "/anchor"] {
        let mut altered = value.clone();
        altered
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<ExtensionRegionReport>(altered).is_err());
    }
    let mut oversized = value;
    oversized["unavailable_reasons"] =
        serde_json::json!(vec!["lost_track"; MAX_UNAVAILABLE_REASONS + 1]);
    assert!(serde_json::from_value::<ExtensionRegionReport>(oversized).is_err());
}
