//! A `Follow` resolved by the shared plan centers its target exactly where
//! the renderer places that source point when the canvas aspect differs from
//! the picture's: the plan's exact letterbox mapping agrees with
//! `PictureGeometry::source_to_input` for the same metadata and canvas.

use deadpan_core::{
    AssetId, AttentionTarget, ExactRatio, FramingPose, SourcePoint, SourceSpan, SourceTimeBase,
    SourceTimestamp, TargetRegion,
};
use deadpan_render::{
    FitMode, FrameMetadata, PictureGeometry, Primaries, Rotation, SampleAspectRatio, SourceColor,
    Transfer,
};

fn value(ratio: ExactRatio) -> f64 {
    ratio.numerator() as f64 / ratio.denominator() as f64
}

fn metadata(width: u32, height: u32, sar: [u32; 2], rotation: Rotation) -> FrameMetadata {
    FrameMetadata {
        width,
        height,
        row_stride_bytes: width * 4,
        clean_aperture: None,
        sample_aspect_ratio: SampleAspectRatio::new(sar[0], sar[1]).unwrap(),
        rotation,
        color: SourceColor {
            transfer: Transfer::Srgb,
            primaries: Primaries::Rec709,
        },
        pts: SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 1000).unwrap(),
        },
    }
}

fn target(center: [u32; 2], aspect: Option<ExactRatio>) -> AttentionTarget {
    let time_base = SourceTimeBase::new(1, 1000).unwrap();
    let stamp = |ticks| SourceTimestamp { ticks, time_base };
    AttentionTarget {
        label: "Speaker".into(),
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(stamp(0), stamp(1000)).unwrap(),
        region: TargetRegion {
            center,
            size: [100_000, 100_000],
        }
        .into(),
        samples: Vec::new(),
        corrections: Vec::new(),
        provenance: None,
        display_aspect: aspect,
    }
}

#[test]
fn plan_follow_centers_agree_with_the_renderers_fit_on_other_canvases() {
    let point = SourcePoint {
        ticks: ExactRatio::integer(10),
        time_base: SourceTimeBase::new(1, 1000).unwrap(),
    };
    let asset = AssetId::new("original").unwrap();
    for meta in [
        // A landscape Original on a Shorts canvas, and the reverse.
        metadata(1920, 1080, [1, 1], Rotation::None),
        metadata(1080, 1920, [1, 1], Rotation::None),
        // Anamorphic DV with a quarter turn: upright 9:16.
        metadata(720, 480, [32, 27], Rotation::Clockwise90),
        metadata(720, 480, [32, 27], Rotation::Clockwise270),
    ] {
        let aspect = meta.display_aspect().unwrap();
        for canvas in [[1080_u32, 1920_u32], [1920, 1080], [1080, 1080]] {
            let geometry = PictureGeometry::new(&meta, canvas[0], canvas[1], FitMode::Fit).unwrap();
            for center in [[200_000, 250_000], [500_000, 500_000], [900_000, 50_000]] {
                let target = target(center, Some(aspect));
                let pose = deadpan_plan::follow_pose(
                    &target,
                    &asset,
                    point,
                    canvas,
                    std::iter::empty::<Option<FramingPose>>(),
                    ExactRatio::ONE,
                )
                .unwrap();
                let rendered = geometry
                    .source_to_input(0, center.map(|v| f64::from(v) / 1_000_000.0))
                    .unwrap()
                    .unwrap();
                // Poses are quantized to the Q32 framing grid.
                for (axis, resolved) in [pose.center_x, pose.center_y].into_iter().enumerate() {
                    assert!(
                        (value(resolved) - rendered[axis]).abs() < 1e-9,
                        "{meta:?} {canvas:?} {center:?}: {resolved:?} vs {rendered:?}"
                    );
                }
            }
        }
    }
    // Without a recorded aspect the follow keeps source coordinates, which the
    // renderer places elsewhere on a vertical canvas.
    let landscape = metadata(1920, 1080, [1, 1], Rotation::None);
    let geometry = PictureGeometry::new(&landscape, 1080, 1920, FitMode::Fit).unwrap();
    let legacy = deadpan_plan::follow_pose(
        &target([200_000, 250_000], None),
        &asset,
        point,
        [1080, 1920],
        std::iter::empty::<Option<FramingPose>>(),
        ExactRatio::ONE,
    )
    .unwrap();
    let rendered = geometry.source_to_input(0, [0.2, 0.25]).unwrap().unwrap();
    assert_eq!(value(legacy.center_y), 0.25);
    assert!((rendered[1] - (0.5 - 81.0 / 1024.0)).abs() < 1e-12);
}
