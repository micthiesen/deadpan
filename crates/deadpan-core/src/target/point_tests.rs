use super::*;
use proptest::prelude::*;

pub(super) fn point_target() -> AttentionTarget {
    let mut value = target();
    value.region = TargetPoint {
        center: value.region.center(),
    }
    .into();
    for sample in &mut value.samples {
        sample.region = TargetPoint {
            center: sample.region.center(),
        }
        .into();
    }
    for correction in &mut value.corrections {
        correction.region = TargetPoint {
            center: correction.region.center(),
        }
        .into();
    }
    value
}

#[test]
fn point_geometry_codec_preserves_rectangle_bytes_and_rejects_open_or_oversized_shapes() {
    let rectangle = TargetRegion {
        center: [0, TARGET_UNITS],
        size: [1, TARGET_UNITS],
    };
    let geometry: TargetGeometry = rectangle.into();
    assert_eq!(
        serde_json::to_string(&geometry).unwrap(),
        serde_json::to_string(&rectangle).unwrap()
    );
    let point: TargetGeometry = TargetPoint {
        center: [0, TARGET_UNITS],
    }
    .into();
    assert_eq!(
        serde_json::to_string(&point).unwrap(),
        r#"{"center":[0,1000000]}"#
    );
    assert_eq!(
        serde_json::from_str::<TargetGeometry>(r#"{"center":[0,1000000]}"#).unwrap(),
        point
    );
    for raw in [
        r#"{"center":[0,0],"size":null}"#,
        r#"{"center":[0,0],"size":[0,1]}"#,
        r#"{"center":[1000001,0]}"#,
        r#"{"center":[-1,0]}"#,
        r#"{"center":[0]}"#,
        r#"{"center":[0,0,1]}"#,
        r#"{"center":[0,0],"center":[1,1]}"#,
        r#"{"center":[0,0],"size":[1,1],"size":[2,2]}"#,
        r#"{"center":[0,0],"point":true}"#,
    ] {
        assert!(
            serde_json::from_str::<TargetGeometry>(raw).is_err(),
            "{raw}"
        );
    }
    for raw in [r#"{"center":[0,0,{"#, r#"{"center":[0,0],"size":[1,1,{"#] {
        let error = serde_json::from_str::<TargetGeometry>(raw).unwrap_err();
        assert!(
            !error.is_eof(),
            "an oversized array must refuse before reading its extra body: {error}"
        );
    }
    let error =
        serde_json::from_str::<TargetGeometry>(r#"{"center":[0,0],"unexpected":{"#).unwrap_err();
    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn point_targets_require_one_geometry_kind_and_keep_existing_time_confidence_and_lost_rules() {
    let value = point_target();
    value.validate(&assets()).unwrap();
    assert!(value.region.as_point().is_some());
    assert!(value.region.as_region().is_none());
    let mut bad = value.clone();
    bad.samples[0].region = region(200_000);
    assert!(bad.validate_shape().is_err());
    let mut bad = value.clone();
    bad.corrections[0].region = region(300_000);
    assert!(bad.validate_shape().is_err());
    let mut moving = value;
    moving.corrections.clear();
    moving.samples = vec![
        TargetSample {
            at: 20,
            region: TargetPoint { center: [0, 0] }.into(),
            confidence: 900,
            state: TrackState::Tracked,
        },
        TargetSample {
            at: 22,
            region: TargetPoint { center: [1, 3] }.into(),
            confidence: 600,
            state: TrackState::Tracked,
        },
        TargetSample {
            at: 24,
            region: TargetPoint {
                center: [400_000, 500_000],
            }
            .into(),
            confidence: 0,
            state: TrackState::Lost,
        },
    ];
    assert_eq!(
        moving
            .evaluated_region_at(point(21))
            .unwrap()
            .region
            .center(),
        [0, 2]
    ); // exact ties to even
    assert_eq!(
        moving.evaluated_region_at(point(21)).unwrap().confidence,
        Some(600)
    );
    assert_eq!(moving.region_at(point(23)).unwrap().0.center(), [1, 3]); // no interpolation into Lost
    assert_eq!(
        moving.region_at(point(99)).unwrap().0.center(),
        [400_000, 500_000]
    );
    assert_eq!(moving.region_at(point(100)), None);
    let wrong = SourcePoint {
        time_base: SourceTimeBase::new(1, 1000).unwrap(),
        ..point(20)
    };
    assert_eq!(moving.region_at(wrong), None);
}

#[test]
fn point_manual_corrections_invalidate_only_their_half_open_range_atomically() {
    let mut value = point_target();
    value.samples = [10, 20, 29, 30, 39, 40, 50, 70, 90]
        .into_iter()
        .map(|at| TargetSample {
            at,
            region: TargetPoint {
                center: [at as u32 * 1000, 500_000],
            }
            .into(),
            confidence: 900,
            state: TrackState::Tracked,
        })
        .collect();
    value.corrections = [10, 30, 70]
        .into_iter()
        .map(|at| TargetCorrection {
            at,
            region: value.region,
        })
        .collect();
    let before = value.clone();
    let changed = value
        .with_point_correction(
            40,
            TargetPoint {
                center: [700_000, 300_000],
            },
        )
        .unwrap();
    assert_eq!(
        changed.samples.iter().map(|s| s.at).collect::<Vec<_>>(),
        [10, 20, 29, 30, 39, 70, 90]
    );
    assert_eq!(
        changed.corrections.iter().map(|c| c.at).collect::<Vec<_>>(),
        [10, 30, 40, 70]
    );
    assert_eq!(
        changed.region_at(point(40)).unwrap().0.center(),
        [700_000, 300_000]
    );
    assert_eq!(
        changed
            .samples
            .iter()
            .filter(|s| s.at >= 70)
            .collect::<Vec<_>>(),
        value
            .samples
            .iter()
            .filter(|s| s.at >= 70)
            .collect::<Vec<_>>()
    );
    let start = value
        .with_point_correction(
            10,
            TargetPoint {
                center: [900_000, 200_000],
            },
        )
        .unwrap();
    assert_eq!(start.region.center(), [900_000, 200_000]);
    assert_eq!(
        start.corrections.iter().map(|c| c.at).collect::<Vec<_>>(),
        [30, 70]
    );
    assert_eq!(
        start.samples.iter().map(|s| s.at).collect::<Vec<_>>(),
        [30, 39, 40, 50, 70, 90]
    );
    for at in [i64::MIN, 9, 100, i64::MAX] {
        assert!(
            value
                .with_point_correction(at, TargetPoint { center: [0, 0] })
                .is_err()
        );
    }
    assert!(
        value
            .with_point_correction(
                40,
                TargetPoint {
                    center: [TARGET_UNITS + 1, 0]
                }
            )
            .is_err()
    );
    assert!(
        target()
            .with_point_correction(40, TargetPoint { center: [0, 0] })
            .is_err()
    );
    assert_eq!(value, before);
    assert_eq!(changed.span, value.span);
    assert_eq!(changed.provenance, value.provenance);
}

#[test]
fn point_correction_real_limit_allows_update_and_start_without_a_new_slot() {
    let mut value = point_target();
    value.span = SourceSpan::new(stamp(10), stamp(1000)).unwrap();
    value.samples.clear();
    value.corrections = (11..=266)
        .map(|at| TargetCorrection {
            at,
            region: value.region,
        })
        .collect();
    assert_eq!(value.corrections.len(), MAX_TARGET_CORRECTIONS);
    let before = value.clone();
    assert_eq!(
        value
            .with_point_correction(267, TargetPoint { center: [0, 0] })
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    assert_eq!(
        value
            .with_point_correction(120, TargetPoint { center: [0, 0] })
            .unwrap()
            .corrections
            .len(),
        MAX_TARGET_CORRECTIONS
    );
    assert_eq!(
        value
            .with_point_correction(10, TargetPoint { center: [0, 0] })
            .unwrap()
            .corrections
            .len(),
        MAX_TARGET_CORRECTIONS
    );
    assert_eq!(value, before);
}

proptest! {
    #[test]
    fn point_samples_interpolate_exactly_like_rectangle_centers(a in 0..=TARGET_UNITS,b in 0..=TARGET_UNITS,t in 0i64..=20) {
        let mut rectangle=target();
        rectangle.corrections.clear();
        rectangle.samples=vec![
            TargetSample{at:20,region:region(a),confidence:950,state:TrackState::Tracked},
            TargetSample{at:40,region:region(b),confidence:750,state:TrackState::Tracked}
        ];
        let mut point=rectangle.clone();
        point.region=TargetPoint{center:point.region.center()}.into();
        for sample in &mut point.samples {sample.region=TargetPoint{center:sample.region.center()}.into();}
        let at=SourcePoint{ticks:ExactRatio::integer(20+t),time_base:base()};
        prop_assert_eq!(point.evaluated_region_at(at).unwrap().region.center(),rectangle.evaluated_region_at(at).unwrap().region.center());
        prop_assert_eq!(point.evaluated_region_at(at).unwrap().confidence,rectangle.evaluated_region_at(at).unwrap().confidence);
    }
}

#[test]
fn point_command_and_stored_patch_roundtrip_reject_mixed_geometry_atomically() {
    let empty = crate::ProjectDocument::new(
        crate::ProjectId::new("points").unwrap(),
        crate::RevisionId::new("initial").unwrap(),
        crate::PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: crate::FrameRate::new(30, 1).unwrap(),
            color_policy: crate::ColorPolicy::SdrRec709,
        },
        crate::NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut raw = serde_json::to_value(empty).unwrap();
    raw["assets"] = serde_json::to_value(assets()).unwrap();
    let before = crate::ProjectDocument::from_json(&raw.to_string()).unwrap();
    let id = TargetId::new("point").unwrap();
    let request = crate::CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: crate::RevisionId::new("saved").unwrap(),
        command: crate::Command::SetTarget {
            id: id.clone(),
            target: point_target(),
        },
    };
    let request: crate::CommandRequest =
        serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    let edit = crate::apply(&before, &request).unwrap();
    let patch: crate::DocumentPatch =
        serde_json::from_str(&serde_json::to_string(&edit.forward).unwrap()).unwrap();
    let after = patch.apply(&before).unwrap();
    assert_eq!(after.targets()[&id], point_target());
    assert_eq!(edit.inverse.apply(&after).unwrap(), before);
    let before_json = before.to_json().unwrap();
    let mut invalid = patch;
    invalid
        .targets
        .get_mut(&id)
        .unwrap()
        .after
        .as_mut()
        .unwrap()
        .samples[0]
        .region = region(200_000);
    let raw = serde_json::to_string(&invalid).unwrap();
    let invalid: crate::DocumentPatch = serde_json::from_str(&raw).unwrap();
    assert!(invalid.apply(&before).is_err());
    assert_eq!(before.to_json().unwrap(), before_json);
}

#[test]
fn point_corrections_preserve_extreme_source_tick_endpoints_without_subtraction_overflow() {
    for start in [i64::MIN, i64::MAX - 1000] {
        let mut value = point_target();
        value.span = SourceSpan::new(stamp(start), stamp(start + 1000)).unwrap();
        value.corrections = vec![
            TargetCorrection {
                at: start,
                region: value.region,
            },
            TargetCorrection {
                at: start + 20,
                region: value.region,
            },
        ];
        value.samples = [start, start + 1, start + 500]
            .into_iter()
            .map(|at| TargetSample {
                at,
                region: value.region,
                confidence: 900,
                state: TrackState::Tracked,
            })
            .collect();
        let corrected = value
            .with_point_correction(
                start,
                TargetPoint {
                    center: [0, TARGET_UNITS],
                },
            )
            .unwrap();
        assert_eq!(
            corrected.samples.iter().map(|s| s.at).collect::<Vec<_>>(),
            [start + 500]
        );
        assert_eq!(
            corrected
                .corrections
                .iter()
                .map(|c| c.at)
                .collect::<Vec<_>>(),
            [start + 20]
        );
        assert_eq!(corrected.region.center(), [0, TARGET_UNITS]);
        let last = value
            .with_point_correction(
                start + 999,
                TargetPoint {
                    center: [TARGET_UNITS, 0],
                },
            )
            .unwrap();
        assert_eq!(last.corrections.last().unwrap().at, start + 999);
        assert!(
            value
                .with_point_correction(start + 1000, TargetPoint { center: [0, 0] })
                .is_err()
        );
    }
}
