//! Live scopes clip targets just as retained canvases do.
use super::*;
use deadpan_core::TargetPoint;
use deadpan_render::FramingLayer;

#[test]
fn live_follow_visibility_and_centers_match_the_renderers_irreversible_clips() {
    let q = |n, d| ExactRatio::new(n, d).unwrap();
    let pose = |x, scale| FramingPose::new(x, q(1, 2), scale).unwrap();
    let chains = [
        vec![],
        vec![Some(pose(q(1, 2), q(2, 1)))],
        // A point lost at 4x cannot return when its parent zooms back out.
        vec![Some(pose(q(1, 2), q(4, 1))), Some(pose(q(1, 2), q(1, 4)))],
        vec![None, Some(pose(q(1, 4), q(2, 1))), None],
        // The scale-3 clamp margin 1/6 rounds upward on the pose grid.
        // Source x=0 consequently lands at -1/Q32, which is truly clipped.
        vec![Some(pose(q(715_827_883, 4_294_967_296), q(3, 1)))],
    ];
    let point = SourcePoint {
        ticks: ExactRatio::integer(10),
        time_base: SourceTimeBase::new(1, 1000).unwrap(),
    };
    let asset = AssetId::new("original").unwrap();
    let mut visible = 0;
    let mut clipped = 0;
    for meta in [
        metadata(1920, 1080, [1, 1], Rotation::None),
        metadata(1080, 1920, [1, 1], Rotation::None),
        metadata(720, 480, [32, 27], Rotation::Clockwise90),
        metadata(720, 480, [32, 27], Rotation::Clockwise270),
    ] {
        for canvas in [[640, 360], [360, 640], [400, 400]] {
            for inner in &chains {
                let layers: Vec<_> = inner
                    .iter()
                    .map(|pose| match pose {
                        Some(pose) => {
                            FramingLayer::new([pose.center_x, pose.center_y], pose.scale).unwrap()
                        }
                        None => FramingLayer::identity(),
                    })
                    .collect();
                let geometry =
                    PictureGeometry::composed(&meta, None, canvas, canvas, FitMode::Fit, &layers)
                        .unwrap();
                for center in [
                    [0, 500_000],
                    [200_000, 250_000],
                    [500_000, 500_000],
                    [900_000, 50_000],
                    [680_000, 500_000],
                ] {
                    let rendered = geometry
                        .source_to_canvas(center.map(|n| f64::from(n) / 1_000_000.0))
                        .unwrap();
                    for is_point in [false, true] {
                        let mut target = target(center, meta.display_aspect());
                        if is_point {
                            target.region = TargetPoint { center }.into();
                        }
                        let resolved = deadpan_plan::follow_pose(
                            &target,
                            &asset,
                            point,
                            canvas,
                            inner.iter().copied(),
                            ExactRatio::ONE,
                        );
                        assert_eq!(
                            resolved.is_some(),
                            rendered.is_some(),
                            "visibility: {meta:?} {canvas:?} {inner:?} {center:?}"
                        );
                        if let (Some(resolved), Some(rendered)) = (resolved, rendered) {
                            for (axis, value_ratio) in [resolved.center_x, resolved.center_y]
                                .into_iter()
                                .enumerate()
                            {
                                assert!((value(value_ratio) - rendered[axis]).abs() < 1e-9);
                            }
                            visible += 1;
                        } else {
                            clipped += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(visible + clipped, 600);
    assert!(visible > 0 && clipped > 0);
}
