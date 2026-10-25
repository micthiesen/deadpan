use deadpan_core::ExtensionDirection;
use serde_json::json;

use super::*;

const RASTER: [u32; 2] = [128, 96];
const PRESENTATION: [u32; 4] = [0, 0, 128, 96];
const DIRECTIONS: [ExtensionDirection; 2] =
    [ExtensionDirection::FromLeft, ExtensionDirection::FromRight];

fn face(center: f64, aperture: f64) -> FaceLandmarks {
    let region = |points| LandmarkRegion::Detected { points };
    let lips = vec![
        [center - 0.025, 0.62 - aperture / 12.0],
        [center + 0.025, 0.62 + aperture / 12.0],
    ];
    FaceLandmarks {
        region: NormalizedRect::new(center - 0.18, 0.22, 0.36, 0.56).unwrap(),
        confidence: 0.95,
        landmarks: LandmarkAvailability::Available {
            confidence: 0.95,
            left_eye: region(vec![[center - 0.0625, 0.4]]),
            right_eye: region(vec![[center + 0.0625, 0.4]]),
            nose: region(vec![[center, 0.5]]),
            outer_lips: region(lips.clone()),
            inner_lips: region(lips),
        },
    }
}

fn set(mut faces: Vec<FaceLandmarks>) -> FaceObservationSet {
    faces.sort_by(FaceLandmarks::order);
    FaceObservationSet::Detected { faces }
}

fn one(center: f64, aperture: f64) -> FaceObservationSet {
    set(vec![face(center, aperture)])
}

fn fixture(
    direction: ExtensionDirection,
    mut outward: Vec<FaceObservationSet>,
) -> (RawExtensionLandmarkBatch, Vec<i64>) {
    let count = outward.len() as u32;
    let (start, end) = match direction {
        ExtensionDirection::FromLeft => (3, 3 + count),
        ExtensionDirection::FromRight => {
            outward.reverse();
            (0, count)
        }
    };
    let native = count + 3;
    let pts: Vec<_> = (0..native).map(|ordinal| i64::from(ordinal) * 40).collect();
    (
        RawExtensionLandmarkBatch {
            schema_version: RAW_EXTENSION_LANDMARK_SCHEMA_VERSION,
            coverage: ExtensionCoverage {
                direction,
                start,
                end,
            },
            anchor: one(0.35, 0.03),
            frames: outward
                .into_iter()
                .enumerate()
                .map(|(index, observation)| {
                    let ordinal = start + index as u32;
                    FrameObservation {
                        ordinal,
                        pts: pts[ordinal as usize],
                        observation,
                    }
                })
                .collect(),
        },
        pts,
    )
}

fn inspect(raw: &RawExtensionLandmarkBatch, pts: &[i64]) -> ExtensionGeometryReport {
    analyze(raw, pts, RASTER, PRESENTATION, true).unwrap()
}

#[test]
fn static_anchor_and_generated_only_coverage_are_explicit_for_both_directions() {
    for direction in DIRECTIONS {
        let (raw, pts) = fixture(direction, vec![one(0.35, 0.03); 4]);
        raw.validate(&pts).unwrap();
        let report = inspect(&raw, &pts);
        assert_eq!(report.profile, PROFILE);
        assert_eq!(report.coverage, raw.coverage);
        assert_eq!(report.measured_face_frames, 4);
        assert_eq!(report.geometry.status, CheckStatus::Measured);
        assert_eq!(report.geometry.maximum_center_residual, Some(0.0));
        assert_eq!(report.geometry.maximum_feature_residual, Some(0.0));
        assert_eq!(report.mouth.basis, MouthBasis::GeneratedIntervalOnly);
        assert_eq!(report.mouth.maximum_aperture_step, Some(0.0));
        report
            .validate_recomputed(&raw, &pts, RASTER, PRESENTATION, true)
            .unwrap();
        let bytes = serde_json::to_vec(&raw).unwrap();
        assert_eq!(
            serde_json::from_slice::<RawExtensionLandmarkBatch>(&bytes).unwrap(),
            raw
        );
    }
}

#[test]
fn raw_observations_cannot_include_context_or_omit_reorder_reclock_generated_frames() {
    for direction in DIRECTIONS {
        let (raw, pts) = fixture(direction, vec![one(0.35, 0.03); 4]);
        let mut variants = Vec::new();
        let mut missing = raw.clone();
        missing.frames.pop();
        variants.push(missing);
        let mut reordered = raw.clone();
        reordered.frames.swap(0, 1);
        variants.push(reordered);
        let mut pts_changed = raw.clone();
        pts_changed.frames[0].pts += 1;
        variants.push(pts_changed);
        let mut context = raw.clone();
        context.frames[0].ordinal = match direction {
            ExtensionDirection::FromLeft => 0,
            ExtensionDirection::FromRight => raw.coverage.end,
        };
        variants.push(context);
        let mut all_native = raw.clone();
        all_native.coverage.start = 0;
        all_native.coverage.end = pts.len() as u32;
        variants.push(all_native);
        for changed in variants {
            assert!(changed.validate(&pts).is_err());
        }
        let mut reversed_pts = pts.clone();
        reversed_pts.reverse();
        assert!(raw.validate(&reversed_pts).is_err());
        let invalid_length = match direction {
            ExtensionDirection::FromLeft => pts.len() - 1,
            ExtensionDirection::FromRight => raw.coverage.end as usize,
        };
        assert!(raw.validate(&pts[..invalid_length]).is_err());
    }
}

#[test]
fn measured_drift_prefix_remains_rejected_after_loss_in_each_outward_direction() {
    for direction in DIRECTIONS {
        let (raw, pts) = fixture(
            direction,
            vec![one(0.55, 0.03), one(0.55, 0.03), set(vec![])],
        );
        let report = inspect(&raw, &pts);
        assert_eq!(report.geometry.status, CheckStatus::Rejected);
        assert_eq!(report.measured_face_frames, 2);
        assert_eq!(report.geometry.measured_tracks, 1);
        assert_eq!(report.geometry.unavailable_tracks, 1);
        let rejection = report.geometry.rejection.unwrap();
        let first = match direction {
            ExtensionDirection::FromLeft => raw.coverage.start,
            ExtensionDirection::FromRight => raw.coverage.end - 2,
        };
        assert_eq!(rejection.first_ordinal, first);
        assert_eq!(rejection.consecutive_frames, 2);
        assert!(rejection.center_residual > report.thresholds.center_residual);
    }
}

#[test]
fn association_never_jumps_across_a_gap_or_ambiguous_anchor() {
    for direction in DIRECTIONS {
        let (raw, pts) = fixture(
            direction,
            vec![set(vec![]), one(0.55, 0.03), one(0.55, 0.03)],
        );
        let report = inspect(&raw, &pts);
        assert_eq!(report.measured_face_frames, 0);
        assert_eq!(report.geometry.status, CheckStatus::Unavailable);
        assert!(report.geometry.rejection.is_none());
        let (mut ambiguous, pts) = fixture(direction, vec![one(0.35, 0.03); 4]);
        ambiguous.anchor = set(vec![face(0.35, 0.03), face(0.38, 0.03)]);
        let report = inspect(&ambiguous, &pts);
        assert_eq!(report.measured_face_frames, 0);
        assert_eq!(report.geometry.unavailable_tracks, 2);
        assert!(
            report
                .geometry
                .unavailable_reasons
                .contains(&GeometryUnavailableReason::AmbiguousAssociation)
        );
    }
}

#[test]
fn single_generated_picture_measures_geometry_but_cannot_invent_sustained_checks() {
    for direction in DIRECTIONS {
        let (raw, pts) = fixture(direction, vec![one(0.55, 0.03)]);
        let report = inspect(&raw, &pts);
        assert_eq!(report.measured_face_frames, 1);
        assert!(report.geometry.rejection.is_none());
        assert_eq!(report.mouth.status, CheckStatus::Unavailable);
        assert!(
            report
                .mouth
                .unavailable_reasons
                .contains(&MouthUnavailableReason::InsufficientFrames)
        );
    }
}

#[test]
fn geometry_expects_one_static_anchor_and_preserves_unavailable_features() {
    for direction in DIRECTIONS {
        let (mut raw, pts) = fixture(
            direction,
            vec![one(0.45, 0.03), one(0.55, 0.03), one(0.65, 0.03)],
        );
        let report = inspect(&raw, &pts);
        assert_eq!(report.geometry.status, CheckStatus::Rejected);
        let FaceObservationSet::Detected { faces } = &mut raw.anchor else {
            unreachable!()
        };
        faces[0].landmarks = LandmarkAvailability::Unavailable {
            reason: LandmarkUnavailableReason::Unsupported,
        };
        let report = inspect(&raw, &pts);
        assert_eq!(report.geometry.status, CheckStatus::Rejected);
        assert_eq!(report.geometry.maximum_feature_residual, None);
        assert_eq!(report.geometry.feature_unavailable_tracks, 1);
        assert!(
            report
                .geometry
                .unavailable_reasons
                .contains(&GeometryUnavailableReason::UnsupportedLandmarks)
        );
    }
}

#[test]
fn mouth_is_chronological_generated_only_and_a_later_loss_cannot_erase_rejection() {
    for direction in DIRECTIONS {
        let (mut raw, pts) = fixture(direction, vec![one(0.35, 0.03); 5]);
        let chronological = [
            one(0.35, 0.03),
            one(0.35, 0.4),
            one(0.35, 0.03),
            one(0.35, 0.03),
            set(vec![]),
        ];
        for (frame, observation) in raw.frames.iter_mut().zip(chronological) {
            frame.observation = observation;
        }
        let report = inspect(&raw, &pts);
        assert_eq!(report.mouth.status, CheckStatus::Rejected);
        assert_eq!(
            report.mouth.rejection.unwrap().first_after_ordinal,
            raw.coverage.start + 1
        );
        assert_eq!(report.mouth.rejection.unwrap().consecutive_changes, 2);
        assert_eq!(report.mouth.unavailable_tracks, 1);
        let black = analyze(&raw, &pts, RASTER, PRESENTATION, false).unwrap();
        assert_eq!(black.geometry.status, CheckStatus::Unavailable);
        assert_eq!(black.mouth, report.mouth);
    }
}

#[test]
fn mouth_does_not_compare_anchor_or_reverse_chronological_leading_coverage() {
    for direction in DIRECTIONS {
        let (mut raw, pts) = fixture(direction, vec![one(0.35, 0.03); 4]);
        raw.anchor = one(0.35, 0.8);
        raw.frames[0].observation = set(vec![]);
        let report = inspect(&raw, &pts);
        assert_eq!(report.mouth.leading_unobserved_frames, 1);
        assert_eq!(report.mouth.maximum_aperture_step, Some(0.0));
        assert_eq!(report.mouth.status, CheckStatus::Measured);
        assert!(report.mouth.rejection.is_none());
    }
}

#[test]
fn extension_raw_wire_bounds_every_collection_and_rejects_hidden_fields() {
    let (raw, _) = fixture(ExtensionDirection::FromLeft, vec![one(0.35, 0.03); 4]);
    let wire = serde_json::to_value(raw).unwrap();
    let mut variants = Vec::new();
    let mut unknown = wire.clone();
    unknown["opposite"] = json!(null);
    variants.push(unknown);
    let mut missing = wire.clone();
    missing.as_object_mut().unwrap().remove("anchor");
    variants.push(missing);
    let mut frames = wire.clone();
    frames["frames"] = json!(vec![wire["frames"][0].clone(); MAX_GENERATED_FRAMES + 1]);
    variants.push(frames);
    let mut faces = wire.clone();
    faces["anchor"]["faces"] = json!(vec![
        wire["anchor"]["faces"][0].clone();
        MAX_FACES_PER_PICTURE + 1
    ]);
    variants.push(faces);
    let mut points = wire.clone();
    points["anchor"]["faces"][0]["landmarks"]["nose"]["points"] =
        json!(vec![[0.4, 0.5]; MAX_LANDMARK_POINTS_PER_REGION + 1]);
    variants.push(points);
    let mut hidden = wire.clone();
    hidden["frames"][0]["observation"] =
        json!({"status":"unavailable","reason":"too_many_faces","faces":[]});
    variants.push(hidden);
    let mut schema = wire;
    schema["schema_version"] = json!(2);
    variants.push(schema);
    for changed in variants {
        assert!(serde_json::from_value::<RawExtensionLandmarkBatch>(changed).is_err());
    }
}

#[test]
fn recomputation_rejects_every_changed_policy_identity_and_assessment() {
    let (raw, pts) = fixture(ExtensionDirection::FromRight, vec![one(0.35, 0.03); 4]);
    let report = inspect(&raw, &pts);
    let wire = serde_json::to_value(&report).unwrap();
    for (pointer, value) in [
        ("/schema_version", json!(2)),
        ("/profile", json!("bridge")),
        ("/coverage/end", json!(3)),
        ("/raster/0", json!(127)),
        ("/anchor_usable", json!(false)),
        ("/measured_face_frames", json!(0)),
        ("/thresholds/center_residual", json!(1.0)),
        ("/mouth_thresholds/aperture_step", json!(0.5)),
        ("/geometry/maximum_center_residual", json!(0.1)),
        ("/mouth/basis", json!("native_frames_only")),
        ("/mouth/leading_unobserved_frames", json!(1)),
    ] {
        let mut changed = wire.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        let changed: ExtensionGeometryReport = serde_json::from_value(changed).unwrap();
        assert!(
            changed
                .validate_recomputed(&raw, &pts, RASTER, PRESENTATION, true)
                .is_err(),
            "{pointer}"
        );
    }
    assert!(analyze(&raw, &pts, [0, 96], PRESENTATION, true).is_err());
    assert!(analyze(&raw, &pts, RASTER, [127, 0, 2, 96], true).is_err());
}
