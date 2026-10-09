use super::*;
use crate::{CANONICAL_BRIDGE_COLOR, DecodedBoundary, MeasuredStream, ModelInputConversion};
use deadpan_core::{
    SourceFrameId, SourceQualificationId, SourceSpan, SourceTimeBase, SourceTimestamp,
    TargetCorrection, TargetSample,
};

fn captured_left(capture: &RegionCapture) -> &CapturedRegionBoundary {
    let RegionCapture::Selected { left, .. } = capture else {
        panic!("selected capture")
    };
    left
}

fn stamp(ticks: i64) -> SourceTimestamp {
    SourceTimestamp {
        ticks,
        time_base: SourceTimeBase::new(1, 1000).unwrap(),
    }
}
fn target() -> AttentionTarget {
    AttentionTarget {
        label: "Hand".into(),
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(stamp(0), stamp(1000)).unwrap(),
        region: TargetRegion {
            center: [500_000, 500_000],
            size: [200_000, 300_000],
        },
        samples: vec![],
        corrections: vec![],
        provenance: None,
    }
}
fn boundary(pts: i64) -> BoundaryPicture {
    BoundaryPicture::Original {
        clock: crate::BoundaryClock::Project { frame: pts },
        asset: AssetId::new("original").unwrap(),
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        picture: DecodedBoundary {
            source_frame: SourceFrameId(7),
            pts: stamp(pts),
            stream: MeasuredStream {
                codec: "ffv1".into(),
                pixel_format: "rgb24".into(),
                width: 640,
                height: 480,
                clean_aperture: None,
                sample_aspect: [4, 3],
                rotation_quarter_turns: 0,
                decoded_sample_bits: 8,
                color: CANONICAL_BRIDGE_COLOR,
            },
            model_input: ModelInputConversion::SrgbCodesUnchanged,
        },
    }
}
fn geometry() -> ConditioningGeometry {
    ConditioningGeometry {
        presentation: RasterRect::centered(569, 320, [768, 320]).unwrap(),
        left_content: Some(RasterRect::centered(427, 320, [768, 320]).unwrap()),
        right_content: Some(RasterRect::centered(426, 320, [768, 320]).unwrap()),
    }
}
fn boundaries() -> BridgeBoundaries {
    BridgeBoundaries {
        left: boundary(100),
        right: boundary(200),
    }
}
fn capture(target: &AttentionTarget) -> RegionCapture {
    RegionCapture::new(
        TargetId::new("hand").unwrap(),
        target,
        &boundaries(),
        &geometry(),
        [768, 320],
    )
    .unwrap()
}

#[test]
fn region_uses_exact_decoded_pts_and_actual_odd_fitted_content() {
    let mut target = target();
    target.corrections.push(TargetCorrection {
        at: 150,
        region: TargetRegion {
            center: [600_000, 400_000],
            size: [200_000, 300_000],
        },
    });
    let captured = capture(&target);
    let seeds = captured
        .seeds(&boundaries(), &geometry(), [768, 320])
        .unwrap()
        .unwrap();
    assert!((seeds.left.x() - (170.0 + 0.4 * 427.0) / 768.0).abs() < 1e-12);
    assert!((seeds.right.x() - (171.0 + 0.5 * 426.0) / 768.0).abs() < 1e-12);
    let RegionCapture::Selected { left, right, .. } = &captured else {
        panic!("selected")
    };
    let (
        CapturedRegionBoundary::Available { point, source, .. },
        CapturedRegionBoundary::Available {
            source: right_source,
            ..
        },
    ) = (left.as_ref(), right.as_ref())
    else {
        panic!("available seeds")
    };
    assert_eq!(point.ticks, ExactRatio::integer(100));
    assert_eq!(*source, TargetSource::Initial);
    assert_eq!(*right_source, TargetSource::Manual);
    assert!(captured.unavailable_reason().is_none());
    let wire = serde_json::to_vec(&captured).unwrap();
    assert_eq!(
        serde_json::from_slice::<RegionCapture>(&wire).unwrap(),
        captured
    );
}

#[test]
fn no_target_is_distinct_from_selected_but_unavailable() {
    assert!(
        RegionCapture::None
            .seeds(&boundaries(), &geometry(), [768, 320])
            .unwrap()
            .is_none()
    );
    for (state, confidence, reason) in [
        (TrackState::Lost, 900, RegionCaptureUnavailable::LostTrack),
        (
            TrackState::Interpolated,
            900,
            RegionCaptureUnavailable::InterpolatedTrack,
        ),
        (
            TrackState::Tracked,
            699,
            RegionCaptureUnavailable::LowConfidence,
        ),
    ] {
        let mut target = target();
        target.samples.push(TargetSample {
            at: 0,
            region: target.region,
            confidence,
            state,
        });
        let captured = capture(&target);
        assert!(captured.target_id().is_some());
        assert!(
            captured
                .seeds(&boundaries(), &geometry(), [768, 320])
                .unwrap()
                .is_none()
        );
        assert!(
            matches!(captured_left(&captured),CapturedRegionBoundary::Unavailable {reason:actual} if *actual==reason)
        );
    }
}

#[test]
fn off_source_boxes_are_unavailable_without_clipping() {
    let mut target = target();
    target.region.center[0] = 0;
    assert!(matches!(
        captured_left(&capture(&target)),
        CapturedRegionBoundary::Unavailable {
            reason: RegionCaptureUnavailable::OutsideSource
        }
    ));
}

#[test]
fn tiny_authored_or_fitted_regions_remain_explicitly_unavailable() {
    for size in [[1, 1], [300, 300]] {
        let mut target = target();
        target.region.size = size;
        target.region.validate().unwrap();
        let captured = capture(&target);
        assert!(matches!(
            captured_left(&captured),
            CapturedRegionBoundary::Unavailable {
                reason: RegionCaptureUnavailable::TooSmall
            }
        ));
        assert!(
            captured
                .seeds(&boundaries(), &geometry(), [768, 320])
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn foreign_asset_span_and_non_original_have_no_subject_seed() {
    let mut other = target();
    other.asset = AssetId::new("other").unwrap();
    assert!(matches!(
        captured_left(&capture(&other)),
        CapturedRegionBoundary::Unavailable {
            reason: RegionCaptureUnavailable::DifferentAsset
        }
    ));
    other = target();
    other.span = SourceSpan::new(stamp(101), stamp(200)).unwrap();
    let captured = capture(&other);
    assert!(
        captured
            .seeds(&boundaries(), &geometry(), [768, 320])
            .unwrap()
            .is_none()
    );
    let mut bounds = boundaries();
    bounds.left = BoundaryPicture::AuthoredBlack {
        clock: crate::BoundaryClock::Project { frame: 100 },
    };
    let mut layout = geometry();
    layout.left_content = None;
    let captured = RegionCapture::new(
        TargetId::new("hand").unwrap(),
        &target(),
        &bounds,
        &layout,
        [768, 320],
    )
    .unwrap();
    assert!(matches!(
        captured_left(&captured),
        CapturedRegionBoundary::Unavailable {
            reason: RegionCaptureUnavailable::NotOriginal
        }
    ));
}

#[test]
fn captured_seed_rejects_changed_pts_confidence_and_region() {
    let original = capture(&target());
    for mutation in 0..3 {
        let mut changed = original.clone();
        let RegionCapture::Selected { left, .. } = &mut changed else {
            unreachable!()
        };
        let CapturedRegionBoundary::Available {
            point,
            confidence,
            region,
            ..
        } = left.as_mut()
        else {
            unreachable!()
        };
        match mutation {
            0 => point.ticks = ExactRatio::integer(99),
            1 => *confidence = Some(900),
            _ => region.center[0] = 0,
        }
        assert!(
            changed
                .validate(&boundaries(), &geometry(), [768, 320])
                .is_err()
        );
    }
    let mut json = serde_json::to_value(&original).unwrap();
    json["unknown"] = true.into();
    assert!(serde_json::from_value::<RegionCapture>(json).is_err());
}

#[test]
fn record_fingerprint_changes_even_if_only_later_tracking_changes() {
    let first = capture(&target());
    let mut target = target();
    target.corrections.push(TargetCorrection {
        at: 900,
        region: target.region,
    });
    let second = capture(&target);
    let RegionCapture::Selected {
        target_sha256: first_hash,
        ..
    } = first
    else {
        unreachable!()
    };
    let RegionCapture::Selected {
        target_sha256: second_hash,
        ..
    } = second
    else {
        unreachable!()
    };
    assert_ne!(first_hash, second_hash);
}
