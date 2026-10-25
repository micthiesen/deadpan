use super::*;

const RASTER: [u32; 2] = [128, 96];
const PRESENTATION: [u32; 4] = [0, 0, 128, 96];

fn point(x: f64, y: f64) -> [f64; 2] {
    [x / f64::from(RASTER[0]), y / f64::from(RASTER[1])]
}

fn face(center_x: f64, aperture: f64, angle: f64) -> FaceLandmarks {
    face_scaled(center_x, 1.0, 1.0, aperture, angle)
}

fn face_scaled(
    center_x: f64,
    scale_x: f64,
    scale_y: f64,
    aperture: f64,
    angle: f64,
) -> FaceLandmarks {
    let width = f64::from(RASTER[0]);
    let height = f64::from(RASTER[1]);
    let cx = center_x * width;
    let region = NormalizedRect::new(
        center_x - 0.18 * scale_x,
        0.22 - 0.28 * (scale_y - 1.0),
        0.36 * scale_x,
        0.56 * scale_y,
    )
    .unwrap();
    let axis = [angle.cos(), angle.sin()];
    let normal = [-axis[1], axis[0]];
    let eye_distance = f64::from(RASTER[1]) * 0.18;
    let eye_mid = [cx, 0.41 * height];
    let offset = |center: [f64; 2], along: f64, across: f64| {
        [
            center[0] + axis[0] * along + normal[0] * across,
            center[1] + axis[1] * along + normal[1] * across,
        ]
    };
    let left_eye_center = offset(eye_mid, -eye_distance * 0.5, 0.0);
    let right_eye_center = offset(eye_mid, eye_distance * 0.5, 0.0);
    let nose_center = offset(eye_mid, 0.0, eye_distance * 0.50);
    let mouth_center = offset(eye_mid, 0.0, eye_distance * 1.40);
    let lip_half_width = eye_distance * 0.20;
    let lip_half_height = eye_distance * aperture * 0.5;
    let point_pair = |center: [f64; 2]| {
        vec![
            point(center[0] - normal[0], center[1] - normal[1]),
            point(center[0] + normal[0], center[1] + normal[1]),
        ]
    };
    let lips = vec![
        point(
            offset(mouth_center, -lip_half_width, -lip_half_height)[0],
            offset(mouth_center, -lip_half_width, -lip_half_height)[1],
        ),
        point(
            offset(mouth_center, lip_half_width, -lip_half_height)[0],
            offset(mouth_center, lip_half_width, -lip_half_height)[1],
        ),
        point(
            offset(mouth_center, lip_half_width, lip_half_height)[0],
            offset(mouth_center, lip_half_width, lip_half_height)[1],
        ),
        point(
            offset(mouth_center, -lip_half_width, lip_half_height)[0],
            offset(mouth_center, -lip_half_width, lip_half_height)[1],
        ),
    ];
    FaceLandmarks {
        region,
        confidence: 0.95,
        landmarks: LandmarkAvailability::Available {
            confidence: 0.95,
            left_eye: LandmarkRegion::Detected {
                points: point_pair(left_eye_center),
            },
            right_eye: LandmarkRegion::Detected {
                points: point_pair(right_eye_center),
            },
            nose: LandmarkRegion::Detected {
                points: vec![point(nose_center[0], nose_center[1])],
            },
            outer_lips: LandmarkRegion::Detected {
                points: lips.clone(),
            },
            inner_lips: LandmarkRegion::Detected { points: lips },
        },
    }
}

fn face_without_landmarks(center_x: f64) -> FaceLandmarks {
    let mut result = face(center_x, 0.03, 0.0);
    result.landmarks = LandmarkAvailability::Unavailable {
        reason: LandmarkUnavailableReason::Missing,
    };
    result
}

fn set(mut faces: Vec<FaceLandmarks>) -> FaceObservationSet {
    faces.sort_by(FaceLandmarks::order);
    FaceObservationSet::Detected { faces }
}

fn batch(
    left: FaceObservationSet,
    right: FaceObservationSet,
    frames: Vec<FaceObservationSet>,
) -> RawLandmarkBatch {
    RawLandmarkBatch {
        schema_version: RAW_LANDMARK_SCHEMA_VERSION,
        boundaries: Some(BoundaryObservations { left, right }),
        frames: frames
            .into_iter()
            .enumerate()
            .map(|(ordinal, observation)| FrameObservation {
                ordinal: ordinal as u32,
                pts: ordinal as i64 * 40,
                observation,
            })
            .collect(),
    }
}

fn pts(batch: &RawLandmarkBatch) -> Vec<i64> {
    batch.frames.iter().map(|frame| frame.pts).collect()
}

fn analyze_fixture(batch: &RawLandmarkBatch) -> GeneratedGeometryReport {
    analyze(batch, &pts(batch), RASTER, PRESENTATION, [true, true]).unwrap()
}

#[test]
fn landmark_unavailability_keeps_its_actual_reason_in_both_assessments() {
    for reason in [
        LandmarkUnavailableReason::Missing,
        LandmarkUnavailableReason::InvalidGeometry,
        LandmarkUnavailableReason::LowConfidence,
        LandmarkUnavailableReason::Unsupported,
    ] {
        for field in ["whole", "eye", "nose", "lips"] {
            let mut observation = if field == "whole" {
                face_without_landmarks(0.5)
            } else {
                face(0.5, 0.03, 0.0)
            };
            if field == "whole" {
                observation.landmarks = LandmarkAvailability::Unavailable { reason };
            } else if let LandmarkAvailability::Available {
                left_eye,
                nose,
                inner_lips,
                ..
            } = &mut observation.landmarks
            {
                *match field {
                    "eye" => left_eye,
                    "nose" => nose,
                    "lips" => inner_lips,
                    _ => unreachable!(),
                } = LandmarkRegion::Unavailable { reason };
            }
            let faces = set(vec![observation]);
            let raw = batch(faces.clone(), faces.clone(), vec![faces; 4]);
            let report = analyze_fixture(&raw);
            let expected = FeatureUnavailable::from_raw(reason);
            if field != "lips" {
                assert!(
                    report
                        .geometry
                        .unavailable_reasons
                        .contains(&expected.geometry()),
                    "geometry {reason:?} {field}"
                );
                assert_eq!(report.geometry.feature_unavailable_tracks, 1);
            }
            if field != "nose" {
                assert!(
                    report
                        .mouth
                        .unavailable_reasons
                        .contains(&expected.mouth(field == "lips")),
                    "mouth {reason:?} {field}"
                );
                assert_eq!(report.mouth.measured_tracks, 0);
            }
        }
    }
}

#[test]
fn raw_batch_requires_exact_ordered_native_picture_coverage() {
    let raw = batch(
        set(vec![face(0.35, 0.03, 0.0)]),
        set(vec![face(0.65, 0.03, 0.0)]),
        [0.35, 0.45, 0.55, 0.65]
            .into_iter()
            .map(|x| set(vec![face(x, 0.03, 0.0)]))
            .collect(),
    );
    assert!(raw.validate(&[0, 40, 80, 120]).is_ok());

    let mut missing = raw.clone();
    missing.frames.pop();
    assert_eq!(
        missing.validate(&[0, 40, 80, 120]),
        Err(RawObservationError::FrameCount)
    );

    let mut wrong_ordinal = raw.clone();
    wrong_ordinal.frames[1].ordinal = 3;
    assert_eq!(
        wrong_ordinal.validate(&[0, 40, 80, 120]),
        Err(RawObservationError::FrameOrdinal)
    );

    let mut wrong_pts = raw.clone();
    wrong_pts.frames[1].pts = 41;
    assert_eq!(
        wrong_pts.validate(&[0, 40, 80, 120]),
        Err(RawObservationError::FramePts)
    );
    assert_eq!(
        raw.validate(&[0, 40, 80, 120, 160]),
        Err(RawObservationError::FrameCount)
    );
}

#[test]
fn raw_faces_are_bounded_finite_and_canonically_ordered() {
    let mut faces = vec![face(0.7, 0.03, 0.0), face(0.3, 0.03, 0.0)];
    let unsorted = FaceObservationSet::Detected {
        faces: faces.clone(),
    };
    assert_eq!(unsorted.validate(), Err(RawObservationError::FaceOrder));
    faces.sort_by(FaceLandmarks::order);
    assert!(FaceObservationSet::Detected { faces }.validate().is_ok());

    let too_many = FaceObservationSet::Detected {
        faces: (0..=MAX_FACES_PER_PICTURE)
            .map(|_| face(0.5, 0.03, 0.0))
            .collect(),
    };
    assert_eq!(too_many.validate(), Err(RawObservationError::FaceCount));

    let mut bad_confidence = face(0.5, 0.03, 0.0);
    bad_confidence.confidence = f32::NAN;
    assert_eq!(
        set(vec![bad_confidence]).validate(),
        Err(RawObservationError::FaceConfidence)
    );

    let mut bad_point = face(0.5, 0.03, 0.0);
    if let LandmarkAvailability::Available { nose, .. } = &mut bad_point.landmarks {
        *nose = LandmarkRegion::Detected {
            points: vec![[f64::NAN, 0.5]],
        };
    }
    assert_eq!(
        set(vec![bad_point]).validate(),
        Err(RawObservationError::LandmarkPointBounds)
    );

    let mut too_many_one_region = face(0.5, 0.03, 0.0);
    if let LandmarkAvailability::Available { left_eye, .. } = &mut too_many_one_region.landmarks {
        *left_eye = LandmarkRegion::Detected {
            points: vec![[0.5, 0.5]; MAX_LANDMARK_POINTS_PER_REGION + 1],
        };
    }
    assert_eq!(
        set(vec![too_many_one_region]).validate(),
        Err(RawObservationError::LandmarkPointCount)
    );

    let mut too_many_combined = face(0.5, 0.03, 0.0);
    if let LandmarkAvailability::Available {
        left_eye,
        right_eye,
        nose,
        outer_lips,
        inner_lips,
        ..
    } = &mut too_many_combined.landmarks
    {
        *left_eye = LandmarkRegion::Detected {
            points: vec![[0.5, 0.5]; 18],
        };
        *right_eye = LandmarkRegion::Detected {
            points: vec![[0.5, 0.5]; 18],
        };
        *nose = LandmarkRegion::Detected {
            points: vec![[0.5, 0.5]; 18],
        };
        *outer_lips = LandmarkRegion::Detected {
            points: vec![[0.5, 0.5]; 12],
        };
        *inner_lips = LandmarkRegion::Detected {
            points: vec![[0.5, 0.5]; 11],
        };
    }
    assert_eq!(
        set(vec![too_many_combined]).validate(),
        Err(RawObservationError::LandmarkPointTotal)
    );
}

#[test]
fn detector_unavailability_and_low_confidence_remain_explicit() {
    let too_many = batch(
        FaceObservationSet::Unavailable {
            reason: FaceSetUnavailableReason::TooManyFaces,
        },
        set(vec![face(0.5, 0.03, 0.0)]),
        (0..4).map(|_| set(vec![face(0.5, 0.03, 0.0)])).collect(),
    );
    let report = analyze_fixture(&too_many);
    assert_eq!(report.geometry.status, CheckStatus::Unavailable);
    assert!(
        report
            .geometry
            .unavailable_reasons
            .contains(&GeometryUnavailableReason::TooManyFaces)
    );

    let mut weak = face(0.5, 0.03, 0.0);
    weak.confidence = 0.4;
    let weak = batch(
        set(vec![weak.clone()]),
        set(vec![weak.clone()]),
        (0..4).map(|_| set(vec![weak.clone()])).collect(),
    );
    let report = analyze_fixture(&weak);
    assert_eq!(report.geometry.status, CheckStatus::Unavailable);
    assert!(
        report
            .geometry
            .unavailable_reasons
            .contains(&GeometryUnavailableReason::LowConfidence)
    );
    assert_eq!(report.mouth.status, CheckStatus::Unavailable);
    assert!(
        report
            .mouth
            .unavailable_reasons
            .contains(&MouthUnavailableReason::LowConfidence)
    );
}

#[test]
fn matching_is_order_independent_and_crossings_become_unavailable() {
    let raw = batch(
        set(vec![face(0.3, 0.03, 0.0), face(0.7, 0.03, 0.0)]),
        set(vec![face(0.3, 0.03, 0.0), face(0.7, 0.03, 0.0)]),
        vec![
            set(vec![face(0.34, 0.03, 0.0), face(0.66, 0.03, 0.0)]),
            set(vec![face(0.38, 0.03, 0.0), face(0.62, 0.03, 0.0)]),
            set(vec![face(0.42, 0.03, 0.0), face(0.58, 0.03, 0.0)]),
            set(vec![face(0.46, 0.03, 0.0), face(0.54, 0.03, 0.0)]),
        ],
    );
    let canonical = analyze_fixture(&raw);
    let mut reversed = raw.clone();
    let boundaries = reversed.boundaries.as_mut().unwrap();
    for observation in [&mut boundaries.left, &mut boundaries.right] {
        if let FaceObservationSet::Detected { faces } = observation {
            faces.reverse();
            faces.sort_by(FaceLandmarks::order);
        }
    }
    for frame in &mut reversed.frames {
        if let FaceObservationSet::Detected { faces } = &mut frame.observation {
            faces.reverse();
            faces.sort_by(FaceLandmarks::order);
        }
    }
    assert_eq!(analyze_fixture(&reversed), canonical);

    let crossing = batch(
        set(vec![face(0.3, 0.03, 0.0), face(0.7, 0.03, 0.0)]),
        set(vec![face(0.3, 0.03, 0.0), face(0.7, 0.03, 0.0)]),
        vec![
            set(vec![face(0.38, 0.03, 0.0), face(0.62, 0.03, 0.0)]),
            set(vec![face(0.46, 0.03, 0.0), face(0.54, 0.03, 0.0)]),
            set(vec![face(0.38, 0.03, 0.0), face(0.62, 0.03, 0.0)]),
            set(vec![face(0.3, 0.03, 0.0), face(0.7, 0.03, 0.0)]),
        ],
    );
    let report = analyze_fixture(&crossing);
    assert_eq!(report.geometry.status, CheckStatus::Unavailable);
    assert!(
        report
            .geometry
            .unavailable_reasons
            .contains(&GeometryUnavailableReason::AmbiguousAssociation)
    );
}

#[test]
fn geometry_rejects_sustained_drift_but_ignores_one_picture_spike() {
    let make = |centers: [f64; 4]| {
        batch(
            set(vec![face(0.35, 0.03, 0.0)]),
            set(vec![face(0.65, 0.03, 0.0)]),
            centers
                .into_iter()
                .map(|x| set(vec![face(x, 0.03, 0.0)]))
                .collect(),
        )
    };
    let spike = analyze_fixture(&make([0.35, 0.70, 0.55, 0.65]));
    assert_eq!(spike.geometry.status, CheckStatus::Measured);
    assert!(spike.geometry.rejection.is_none());

    let sustained = analyze_fixture(&make([0.35, 0.70, 0.75, 0.65]));
    assert_eq!(sustained.geometry.status, CheckStatus::Rejected);
    let rejection = sustained.geometry.rejection.unwrap();
    assert_eq!(rejection.first_ordinal, 1);
    assert_eq!(rejection.consecutive_frames, 2);
    assert!(rejection.center_residual > sustained.thresholds.center_residual);
}

#[test]
fn geometry_rejects_sustained_scale_and_aspect_change() {
    let raw = batch(
        set(vec![face_scaled(0.5, 1.0, 1.0, 0.03, 0.0)]),
        set(vec![face_scaled(0.5, 1.0, 1.0, 0.03, 0.0)]),
        [1.0, 1.7, 1.7, 1.0]
            .into_iter()
            .map(|scale| set(vec![face_scaled(0.5, scale, 1.0, 0.03, 0.0)]))
            .collect(),
    );
    let report = analyze_fixture(&raw);
    assert_eq!(report.geometry.status, CheckStatus::Rejected);
    assert!(
        report.geometry.maximum_log_size_residual.unwrap() > report.thresholds.log_size_residual
    );
}

#[test]
fn eye_nose_geometry_is_roll_normalized_on_odd_non_square_raster() {
    let raster = [127, 73];
    let presentation = [0, 0, raster[0], raster[1]];
    let tilted = |angle| face_for_raster(0.5, 0.03, angle, raster);
    let raw = RawLandmarkBatch {
        schema_version: RAW_LANDMARK_SCHEMA_VERSION,
        boundaries: Some(BoundaryObservations {
            left: FaceObservationSet::Detected {
                faces: vec![tilted(-0.6)],
            },
            right: FaceObservationSet::Detected {
                faces: vec![tilted(0.7)],
            },
        }),
        frames: [-0.3, 0.0, 0.3, 0.55]
            .into_iter()
            .enumerate()
            .map(|(ordinal, angle)| FrameObservation {
                ordinal: ordinal as u32,
                pts: ordinal as i64 * 33,
                observation: FaceObservationSet::Detected {
                    faces: vec![tilted(angle)],
                },
            })
            .collect(),
    };
    let report = analyze(&raw, &[0, 33, 66, 99], raster, presentation, [true, true]).unwrap();
    assert_eq!(report.geometry.status, CheckStatus::Measured);
    assert!(report.geometry.maximum_feature_residual.unwrap() < 1e-10);
    assert_eq!(report.mouth.status, CheckStatus::Measured);
    assert!(report.mouth.maximum_aperture_step.unwrap() < 1e-10);
}

#[test]
fn mouth_opening_and_closing_is_rejected_but_constant_opening_is_not() {
    let make = |apertures: [f64; 4]| {
        batch(
            set(vec![face(0.5, 0.35, 0.0)]),
            set(vec![face(0.5, 0.35, 0.0)]),
            apertures
                .into_iter()
                .map(|value| set(vec![face(0.5, value, 0.0)]))
                .collect(),
        )
    };
    let moving = analyze_fixture(&make([0.02, 0.42, 0.02, 0.42]));
    assert_eq!(moving.mouth.status, CheckStatus::Rejected);
    assert_eq!(moving.mouth.rejection.unwrap().consecutive_changes, 2);
    assert_eq!(moving.mouth.basis, MouthBasis::NativeFramesOnly);

    let open = analyze_fixture(&make([0.42, 0.42, 0.42, 0.42]));
    assert_eq!(open.mouth.status, CheckStatus::Measured);
    assert!(open.mouth.rejection.is_none());
}

#[test]
fn mouth_rejection_survives_later_occlusion_or_missing_landmarks() {
    for suffix in [set(vec![]), set(vec![face_without_landmarks(0.5)])] {
        let mut frames: Vec<_> = [0.0, 0.3, 0.0, 0.3]
            .into_iter()
            .map(|aperture| set(vec![face(0.5, aperture, 0.0)]))
            .collect();
        frames.push(suffix);
        let report = analyze_fixture(&batch(set(vec![]), set(vec![]), frames));
        assert_eq!(report.mouth.status, CheckStatus::Rejected);
        assert_eq!(report.mouth.rejection.unwrap().first_after_ordinal, 1);
        assert_eq!(report.mouth.measured_tracks, 1);
        assert_eq!(report.mouth.unavailable_tracks, 1);
        assert!(!report.mouth.unavailable_reasons.is_empty());
    }
}

#[test]
fn mouth_requires_enough_observed_frames_for_each_late_face() {
    for visible_frames in 1..=2 {
        let frames = (0..4)
            .map(|ordinal| {
                if ordinal >= 4 - visible_frames {
                    set(vec![face(0.5, 0.03, 0.0)])
                } else {
                    set(vec![])
                }
            })
            .collect();
        let report = analyze_fixture(&batch(set(vec![]), set(vec![]), frames));
        assert_eq!(report.mouth.status, CheckStatus::Unavailable);
        assert_eq!(report.mouth.measured_tracks, 0);
        assert_eq!(report.mouth.maximum_aperture_step, None);
        assert_eq!(report.mouth.leading_unobserved_frames, 4 - visible_frames);
        assert!(
            report
                .mouth
                .unavailable_reasons
                .contains(&MouthUnavailableReason::InsufficientFrames)
        );
    }
}

#[test]
fn mouth_checks_a_later_face_beside_an_existing_face() {
    let frames = (0..4)
        .map(|ordinal| {
            let mut faces = vec![face(0.25, 0.03, 0.0)];
            if ordinal > 0 {
                faces.push(face(0.75, if ordinal == 2 { 0.3 } else { 0.0 }, 0.0));
            }
            set(faces)
        })
        .collect();
    let report = analyze_fixture(&batch(set(vec![]), set(vec![]), frames));
    assert_eq!(report.mouth.status, CheckStatus::Rejected);
    assert_eq!(report.mouth.rejection.unwrap().first_after_ordinal, 2);
    assert_eq!(report.mouth.measured_tracks, 2);
    assert_eq!(report.mouth.unavailable_tracks, 0);
}

#[test]
fn mouth_does_not_join_changes_across_a_gap_or_ambiguous_crossing() {
    for interruption in [
        set(vec![]),
        set(vec![face(0.48, 0.0, 0.0), face(0.52, 0.3, 0.0)]),
    ] {
        let raw = batch(
            set(vec![]),
            set(vec![]),
            vec![
                set(vec![face(0.5, 0.0, 0.0)]),
                set(vec![face(0.5, 0.0, 0.0)]),
                set(vec![face(0.5, 0.3, 0.0)]),
                interruption,
                set(vec![face(0.5, 0.0, 0.0)]),
                set(vec![face(0.5, 0.3, 0.0)]),
                set(vec![face(0.5, 0.3, 0.0)]),
            ],
        );
        let report = analyze_fixture(&raw);
        assert_eq!(report.mouth.status, CheckStatus::Measured);
        assert_eq!(report.mouth.measured_tracks, 2);
        assert!(report.mouth.rejection.is_none());
        assert!(report.mouth.unavailable_tracks > 0);
    }
}

#[test]
fn mouth_segment_counts_do_not_saturate_at_a_byte() {
    let frames = (0..MAX_NATIVE_FRAMES)
        .map(|_| set(vec![face_without_landmarks(0.5)]))
        .collect();
    let report = analyze_fixture(&batch(set(vec![]), set(vec![]), frames));
    assert_eq!(report.mouth.unavailable_tracks, MAX_NATIVE_FRAMES as u32);
    assert_eq!(report.mouth.measured_tracks, 0);
}

#[test]
fn missing_landmarks_and_authored_black_boundaries_are_explicit() {
    let missing_mouth = batch(
        set(vec![face(0.5, 0.03, 0.0)]),
        set(vec![face(0.5, 0.03, 0.0)]),
        (0..4)
            .map(|_| {
                let mut detected = face(0.5, 0.03, 0.0);
                if let LandmarkAvailability::Available { inner_lips, .. } = &mut detected.landmarks
                {
                    *inner_lips = LandmarkRegion::Unavailable {
                        reason: LandmarkUnavailableReason::Missing,
                    };
                }
                set(vec![detected])
            })
            .collect(),
    );
    let report = analyze_fixture(&missing_mouth);
    assert_eq!(report.mouth.status, CheckStatus::Unavailable);
    assert!(
        report
            .mouth
            .unavailable_reasons
            .contains(&MouthUnavailableReason::MissingInnerLipLandmarks)
    );

    let black = analyze(
        &missing_mouth,
        &pts(&missing_mouth),
        RASTER,
        PRESENTATION,
        [false, true],
    )
    .unwrap();
    assert_eq!(black.geometry.status, CheckStatus::Unavailable);
    assert!(
        black
            .geometry
            .unavailable_reasons
            .contains(&GeometryUnavailableReason::AuthoredBlackBoundary)
    );
    assert_eq!(black.mouth.status, CheckStatus::Unavailable);

    let available = batch(
        set(vec![face(0.5, 0.03, 0.0)]),
        set(vec![face(0.5, 0.03, 0.0)]),
        (0..4).map(|_| set(vec![face(0.5, 0.03, 0.0)])).collect(),
    );
    let black = analyze(
        &available,
        &pts(&available),
        RASTER,
        PRESENTATION,
        [false, true],
    )
    .unwrap();
    assert_eq!(black.geometry.status, CheckStatus::Unavailable);
    assert_eq!(black.mouth.status, CheckStatus::Measured);
}

#[test]
fn boxes_and_landmarks_outside_presentation_do_not_contribute() {
    let raw = batch(
        set(vec![face(0.25, 0.03, 0.0)]),
        set(vec![face(0.25, 0.03, 0.0)]),
        (0..4).map(|_| set(vec![face(0.25, 0.03, 0.0)])).collect(),
    );
    let crop = [32, 8, 64, 80];
    let report = analyze(&raw, &pts(&raw), RASTER, crop, [true, true]).unwrap();
    assert_eq!(report.geometry.status, CheckStatus::Unavailable);
    assert!(
        report
            .geometry
            .unavailable_reasons
            .contains(&GeometryUnavailableReason::OutsidePresentation)
    );
    assert_eq!(report.mouth.status, CheckStatus::Unavailable);
    assert!(
        report
            .mouth
            .unavailable_reasons
            .contains(&MouthUnavailableReason::OutsidePresentation)
    );

    let outside_crop = analyze(&raw, &pts(&raw), RASTER, [32, 8, 0, 80], [true, true]);
    assert_eq!(outside_crop, Err(GeometryError::Presentation));
}

#[test]
fn landmarks_outside_presentation_are_excluded_from_feature_and_mouth_checks() {
    let mut outside = face(0.5, 0.03, 0.0);
    if let LandmarkAvailability::Available {
        left_eye,
        right_eye,
        nose,
        ..
    } = &mut outside.landmarks
    {
        *left_eye = LandmarkRegion::Detected {
            points: vec![[0.1, 0.4], [0.1, 0.41]],
        };
        *right_eye = LandmarkRegion::Detected {
            points: vec![[0.9, 0.4], [0.9, 0.41]],
        };
        *nose = LandmarkRegion::Detected {
            points: vec![[0.9, 0.8]],
        };
    }
    let raw = batch(
        set(vec![outside.clone()]),
        set(vec![outside.clone()]),
        (0..4).map(|_| set(vec![outside.clone()])).collect(),
    );
    let crop = [32, 0, 64, 96];
    let report = analyze(&raw, &pts(&raw), RASTER, crop, [true, true]).unwrap();
    assert_eq!(report.geometry.status, CheckStatus::Measured);
    assert_eq!(report.geometry.maximum_feature_residual, None);
    assert_eq!(report.geometry.feature_unavailable_tracks, 1);
    assert!(
        report
            .geometry
            .unavailable_reasons
            .contains(&GeometryUnavailableReason::OutsidePresentation)
    );
    assert_eq!(report.mouth.status, CheckStatus::Unavailable);
    assert!(
        report
            .mouth
            .unavailable_reasons
            .contains(&MouthUnavailableReason::OutsidePresentation)
    );
}

#[test]
fn policy_thresholds_are_retained_and_recomputed_exactly() {
    let raw = batch(
        set(vec![face(0.35, 0.03, 0.0)]),
        set(vec![face(0.65, 0.03, 0.0)]),
        [0.35, 0.45, 0.55, 0.65]
            .into_iter()
            .map(|x| set(vec![face(x, 0.03, 0.0)]))
            .collect(),
    );
    let mut report = analyze_fixture(&raw);
    assert_eq!(report.profile, PROFILE);
    assert_eq!(report.thresholds, GeometryThresholds::policy());
    assert_eq!(report.mouth_thresholds, MouthThresholds::policy());
    assert!(
        report
            .validate_recomputed(&raw, &[0, 40, 80, 120], RASTER, PRESENTATION, [true, true])
            .is_ok()
    );
    report.mouth_thresholds.aperture_step += 0.01;
    assert_eq!(
        report.validate_recomputed(&raw, &[0, 40, 80, 120], RASTER, PRESENTATION, [true, true]),
        Err(GeometryError::PolicyMismatch)
    );
}

fn face_for_raster(center_x: f64, aperture: f64, angle: f64, raster: [u32; 2]) -> FaceLandmarks {
    let width = f64::from(raster[0]);
    let height = f64::from(raster[1]);
    let cx = center_x * width;
    let axis = [angle.cos(), angle.sin()];
    let normal = [-axis[1], axis[0]];
    let distance = f64::from(raster[1]) * 0.18;
    let eye_mid = [cx, height * 0.41];
    let offset = |along: f64, across: f64| {
        [
            eye_mid[0] + axis[0] * along + normal[0] * across,
            eye_mid[1] + axis[1] * along + normal[1] * across,
        ]
    };
    let norm = |p: [f64; 2]| [p[0] / width, p[1] / height];
    let eye_points = |along: f64| vec![norm(offset(along, -1.0)), norm(offset(along, 1.0))];
    let lip_center = offset(0.0, distance * 1.4);
    let half_width = distance * 0.2;
    let half_height = distance * aperture * 0.5;
    let lips = [
        [-half_width, -half_height],
        [half_width, -half_height],
        [half_width, half_height],
        [-half_width, half_height],
    ]
    .map(|[along, across]| {
        norm([
            lip_center[0] + axis[0] * along + normal[0] * across,
            lip_center[1] + axis[1] * along + normal[1] * across,
        ])
    })
    .to_vec();
    FaceLandmarks {
        region: NormalizedRect::new(center_x - 0.18, 0.22, 0.36, 0.56).unwrap(),
        confidence: 0.95,
        landmarks: LandmarkAvailability::Available {
            confidence: 0.95,
            left_eye: LandmarkRegion::Detected {
                points: eye_points(-distance * 0.5),
            },
            right_eye: LandmarkRegion::Detected {
                points: eye_points(distance * 0.5),
            },
            nose: LandmarkRegion::Detected {
                points: vec![norm(offset(0.0, distance * 0.5))],
            },
            outer_lips: LandmarkRegion::Detected {
                points: lips.clone(),
            },
            inner_lips: LandmarkRegion::Detected { points: lips },
        },
    }
}
