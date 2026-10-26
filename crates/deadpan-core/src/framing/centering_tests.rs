use super::*;
use crate::framing_zoom::{self, Center, Shot, WholeFollowOwner, ZoomCurve, ZoomKind};

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn follow(centering: FollowCentering) -> Framing {
    Framing {
        clock: FramingClock::RetainedOutput {
            offset: ratio(3, 2),
            duration: ExactRatio::integer(40),
        },
        value: FramingValue::Follow {
            target: crate::TargetId::new("moving").unwrap(),
            scale: ExactRatio::integer(2),
            fallback: FramingPose::new(ratio(1, 10), ratio(9, 10), ExactRatio::integer(2)).unwrap(),
            centering,
            scale_transition: None,
        },
    }
}

#[test]
fn centering_clamps_only_zoomed_live_centers_on_the_declared_grid() {
    let center = [ratio(1, 17), ratio(16, 17)];
    for scale in [
        ratio(1, 2),
        ExactRatio::ONE,
        ExactRatio::integer(2),
        ExactRatio::integer(3),
    ] {
        let pose = FramingPose::new(center[0], center[1], scale).unwrap();
        assert_eq!(FollowCentering::Unconstrained.apply(pose).unwrap(), pose);
        let clamped = FollowCentering::CanvasClamped.apply(pose).unwrap();
        if scale.compare_integer(1).is_gt() {
            let margin = ExactRatio::ONE
                .checked_div(scale)
                .unwrap()
                .checked_div(ExactRatio::integer(2))
                .unwrap();
            let expected =
                FramingPose::new(margin, ExactRatio::ONE.checked_sub(margin).unwrap(), scale)
                    .unwrap()
                    .quantized()
                    .unwrap();
            assert_eq!(clamped, expected);
        } else {
            assert_eq!(clamped, pose, "Zooming out retains the live target center");
        }
    }
}

#[test]
fn centering_evaluation_retains_the_authored_fallback_and_policy() {
    let recipe = follow(FollowCentering::CanvasClamped);
    let evaluated = recipe
        .evaluate_with_follow_exact(ExactRatio::integer(7), ExactRatio::integer(20))
        .unwrap();
    let live = evaluated.follow.unwrap();
    assert_eq!(live.target.as_str(), "moving");
    assert_eq!(live.scale, ExactRatio::integer(2));
    assert_eq!(live.centering, FollowCentering::CanvasClamped);
    assert_eq!(evaluated.pose.center_x, ratio(1, 10));
    assert_eq!(evaluated.pose.center_y, ratio(9, 10));
    assert_ne!(
        live.centering.apply(evaluated.pose).unwrap(),
        evaluated.pose,
        "The resolver must apply the policy only to a successfully projected live target"
    );
}

#[test]
fn centering_wire_defaults_are_omitted_and_unknown_policies_refuse() {
    let ordinary = follow(FollowCentering::Unconstrained);
    let old = serde_json::to_value(&ordinary).unwrap();
    assert!(old["value"].get("centering").is_none());
    assert_eq!(
        serde_json::from_value::<Framing>(old.clone()).unwrap(),
        ordinary
    );
    let clamped = follow(FollowCentering::CanvasClamped);
    let raw = serde_json::to_value(&clamped).unwrap();
    assert_eq!(raw["value"]["centering"], "canvas_clamped");
    assert_eq!(
        serde_json::from_value::<Framing>(raw.clone()).unwrap(),
        clamped
    );
    for bad in [
        serde_json::json!("clamp"),
        serde_json::json!({"type":"canvas_clamped"}),
        serde_json::json!(null),
    ] {
        let mut value = raw.clone();
        value["value"]["centering"] = bad;
        assert!(serde_json::from_value::<Framing>(value).is_err());
    }
    assert_eq!(ordinary.record_count(), clamped.record_count());
}

#[test]
fn centering_survives_semantic_keep_scale_creep_range_and_retune() {
    let entry = follow(FollowCentering::CanvasClamped);
    let current = entry
        .evaluate_exact(ExactRatio::integer(7), ExactRatio::integer(20))
        .unwrap();
    let shot = |range| Shot {
        whole_owner: Some(WholeFollowOwner::Sequence),
        entry: Some(&entry),
        current,
        frames: 20,
        range,
        explicit: false,
    };
    for range in [None, Some((4, 12))] {
        let kind = ZoomKind::Creep {
            from: None,
            to: ExactRatio::integer(3),
            curve: ZoomCurve::Smoothstep,
        };
        let built = framing_zoom::build(kind, &Center::Keep, &shot(range))
            .unwrap()
            .unwrap();
        let FramingValue::Follow { centering, .. } = built.value else {
            panic!("Keep must retain Follow")
        };
        assert_eq!(centering, FollowCentering::CanvasClamped);
        let retuned =
            framing_zoom::retune_follow(&built, None, ExactRatio::integer(4), ZoomCurve::Linear)
                .unwrap();
        let FramingValue::Follow { centering, .. } = retuned.value else {
            panic!("Retune must retain Follow")
        };
        assert_eq!(centering, FollowCentering::CanvasClamped);
    }
    for center in [
        Center::Keep,
        Center::Target {
            id: crate::TargetId::new("replacement").unwrap(),
            fixed: None,
        },
    ] {
        let built = framing_zoom::build(
            ZoomKind::Smash {
                scale: ExactRatio::integer(3),
            },
            &center,
            &shot(None),
        )
        .unwrap()
        .unwrap();
        let FramingValue::Follow {
            centering, target, ..
        } = built.value
        else {
            panic!("Scale must retain Follow")
        };
        assert_eq!(centering, FollowCentering::CanvasClamped);
        assert_eq!(
            target.as_str(),
            if center == Center::Keep {
                "moving"
            } else {
                "replacement"
            }
        );
    }
}
