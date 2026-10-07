use super::*;
use crate::{CANONICAL_BRIDGE_COLOR, DecodedBoundary, MeasuredStream, ModelInputConversion};
use deadpan_core::{
    AssetId, SourceFrameId, SourceQualificationId, SourceTimeBase, SourceTimestamp,
};

fn boundary(decoded: bool, project_frame: i64) -> BoundaryPicture {
    if !decoded {
        return BoundaryPicture::AuthoredBlack {
            clock: crate::BoundaryClock::Project {
                frame: project_frame,
            },
        };
    }
    BoundaryPicture::Original {
        clock: crate::BoundaryClock::Project {
            frame: project_frame,
        },
        asset: AssetId::new("original").unwrap(),
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        picture: DecodedBoundary {
            source_frame: SourceFrameId(0),
            pts: SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 24).unwrap(),
            },
            stream: MeasuredStream {
                codec: "rawvideo".into(),
                pixel_format: "rgb24".into(),
                width: 640,
                height: 480,
                sample_aspect: [1, 1],
                rotation_quarter_turns: 0,
                decoded_sample_bits: 8,
                color: CANONICAL_BRIDGE_COLOR,
            },
            model_input: ModelInputConversion::SrgbCodesUnchanged,
        },
    }
}

#[test]
fn rectangles_reject_empty_overflow_and_out_of_bounds_before_arithmetic() {
    for values in [
        (0, 0, 0, 1),
        (0, 0, 1, 0),
        (u32::MAX, 0, 1, 1),
        (0, u32::MAX, 1, 1),
    ] {
        assert!(RasterRect::new(values.0, values.1, values.2, values.3).is_err());
    }
    assert!(
        RasterRect::new(1, 0, 4, 2)
            .unwrap()
            .validate_within([4, 2])
            .is_err()
    );
    // Public fields still pass through validation when interpreting coordinates.
    assert!(
        RasterRect {
            x: u32::MAX,
            y: 0,
            width: 1,
            height: 1
        }
        .validate_within([4, 2])
        .is_err()
    );
    assert!(RasterRect::centered(5, 2, [4, 2]).is_err());
    assert!(RasterRect::centered(1, 0, [4, 2]).is_err());
}

#[test]
fn odd_placement_uses_native_floor_centering_and_content_is_contained() {
    let boundaries = BridgeBoundaries {
        left: boundary(true, 0),
        right: boundary(false, 4),
    };
    let geometry = ConditioningGeometry {
        presentation: RasterRect::centered(569, 320, [768, 320]).unwrap(),
        left_content: Some(RasterRect::centered(426, 320, [768, 320]).unwrap()),
        right_content: None,
    };
    assert_eq!(geometry.presentation.x, 99);
    // Centering within the already rounded presentation would give 170;
    // conditioning centers both rectangles directly in the native raster.
    assert_eq!(geometry.left_content.unwrap().x, 171);
    geometry.validate([768, 320], &boundaries).unwrap();
    let mut shifted = geometry;
    shifted.presentation.x += 1;
    assert!(shifted.validate([768, 320], &boundaries).is_err());
    let mut shifted = geometry;
    shifted.left_content.as_mut().unwrap().x += 1;
    assert!(shifted.validate([768, 320], &boundaries).is_err());
    let mut oversized = geometry;
    oversized.left_content = Some(RasterRect::centered(570, 320, [768, 320]).unwrap());
    assert!(oversized.validate([768, 320], &boundaries).is_err());
}

#[test]
fn content_presence_distinguishes_decoded_black_pixels_from_authored_black() {
    let native = [4, 2];
    let full = RasterRect::centered(4, 2, native).unwrap();
    let boundaries = BridgeBoundaries {
        left: boundary(true, 0),
        right: boundary(false, 4),
    };
    let geometry = ConditioningGeometry {
        presentation: full,
        left_content: Some(full),
        right_content: None,
    };
    geometry.validate(native, &boundaries).unwrap();
    assert!(
        ConditioningGeometry {
            left_content: None,
            ..geometry
        }
        .validate(native, &boundaries)
        .is_err()
    );
    assert!(
        ConditioningGeometry {
            right_content: Some(full),
            ..geometry
        }
        .validate(native, &boundaries)
        .is_err()
    );
}

#[test]
fn geometry_requires_every_key_and_strict_integer_rectangles() {
    let geometry = ConditioningGeometry {
        presentation: RasterRect::new(0, 0, 4, 2).unwrap(),
        left_content: None,
        right_content: None,
    };
    let wire = serde_json::to_value(geometry).unwrap();
    assert_eq!(
        serde_json::from_value::<ConditioningGeometry>(wire.clone()).unwrap(),
        geometry
    );
    for field in ["presentation", "left_content", "right_content"] {
        let mut changed = wire.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ConditioningGeometry>(changed).is_err(),
            "{field}"
        );
    }
    for value in [
        serde_json::json!(true),
        serde_json::json!(-1),
        serde_json::json!(1.0),
        serde_json::json!(4294967296_u64),
    ] {
        let mut changed = wire.clone();
        changed["presentation"]["x"] = value;
        assert!(serde_json::from_value::<ConditioningGeometry>(changed).is_err());
    }
    for field in ["x", "y", "width", "height"] {
        let mut changed = wire.clone();
        changed["presentation"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<ConditioningGeometry>(changed).is_err());
    }
    let mut changed = wire.clone();
    changed["presentation"]["extra"] = serde_json::json!(0);
    assert!(serde_json::from_value::<ConditioningGeometry>(changed).is_err());
    let mut changed = wire;
    changed["extra"] = serde_json::json!(0);
    assert!(serde_json::from_value::<ConditioningGeometry>(changed).is_err());
}
