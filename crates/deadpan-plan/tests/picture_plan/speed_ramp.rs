//! Continuous speed ramps: every output frame center reads the exact
//! quadratic input clock, at the 30000/1001 basis these fixtures use.
use super::*;
use deadpan_plan::PictureClockSlope;

fn ramp_node(child: &str, frames: i64, start: i64, end: i64, ramp: SpeedRamp) -> BeatNode {
    node(NodeKind::Retime {
        purpose: deadpan_core::RetimePurpose::Edit,
        child: id(child),
        duration: duration(frames),
        mapping: range(start, end),
        pitch: PitchPolicy::TapeRamp { ramp },
    })
}

fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

#[test]
fn ramp_pictures_sample_the_exact_integral_at_every_frame_center() {
    // 90 Original frames (1001 ticks each) ramp from 1/2x to 2x: the mean
    // speed is 5/4, so the ramp lasts 72 frames, after a 3-frame lead-in.
    let ramp = SpeedRamp::new(q(1, 2), q(2, 1)).unwrap();
    let document = document(
        &["lead", "ramp"],
        vec![
            ("lead", hold(3)),
            ("src", source(90, 0, 90 * 1001)),
            ("ramp", ramp_node("src", 72, 0, 90, ramp)),
        ],
    );
    let reopened = ProjectDocument::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(reopened, document);
    let plan = RenderPlan::compile(&reopened).unwrap();
    let mut previous = None;
    for frame in 0..72_i64 {
        let sample = plan.picture(ProjectFrame(3 + frame)).unwrap();
        let center = q(2 * i128::from(frame) + 1, 2);
        let input = ramp
            .input_offset(center, ExactRatio::integer(72), ExactRatio::integer(90))
            .unwrap();
        assert_eq!(sample.local_position, input, "frame {frame}");
        let point = ticks(&sample.picture);
        assert_eq!(point, input.checked_mul(ExactRatio::integer(1001)).unwrap());
        if let Some(previous) = previous {
            assert!(point.compare(previous).is_gt(), "frame {frame}");
        }
        previous = Some(point);
    }
    // Slow at the start: the first frame centers land just over half an input
    // frame apart; at the end they are almost two input frames apart.
    let first = plan.picture(ProjectFrame(3)).unwrap().local_position;
    let second = plan.picture(ProjectFrame(4)).unwrap().local_position;
    let step = second.checked_sub(first).unwrap();
    assert!(step.compare(q(1, 2)).is_gt() && step.compare(q(3, 4)).is_lt());
    let last = plan.picture(ProjectFrame(3 + 71)).unwrap().local_position;
    let before = plan.picture(ProjectFrame(3 + 70)).unwrap().local_position;
    let step = last.checked_sub(before).unwrap();
    assert!(step.compare(q(3, 2)).is_gt() && step.compare(q(2, 1)).is_lt());
    // Shown pictures are monotonic Original ordinals ending at the last frame.
    let frames = (0..=90).map(|frame| frame * 1001).collect::<Vec<_>>();
    let index = index("video", clock(), &frames[..90], 90 * 1001);
    let ordinals = (0..72)
        .map(|frame| {
            plan.picture(ProjectFrame(3 + frame))
                .unwrap()
                .picture
                .select_source_frame(&index)
                .unwrap()
                .identity
                .0
        })
        .collect::<Vec<_>>();
    assert!(ordinals.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(ordinals[0], 0);
    assert_eq!(ordinals[71], 89);
    // Coverage reports the nonlinear clock instead of a constant slope.
    let coverage = plan
        .definition_picture_coverage(
            &id("ramp"),
            ExactRatio::ZERO,
            ExactRatio::integer(71),
            BoundaryQueryLimits {
                max_scopes: 1_000,
                max_comparisons: 1_000,
            },
        )
        .unwrap();
    assert_eq!(coverage.spans.len(), 1);
    assert_eq!(coverage.spans[0].clock, PictureClockSlope::Varying);
}

#[test]
fn a_ramp_holds_exactly_one_original_beat() {
    let ramp = SpeedRamp::new(q(1, 1), q(2, 1)).unwrap();
    let wire = |inner: Vec<(&str, BeatNode)>| {
        let mut nodes = inner;
        nodes.push(("ramp", ramp_node("inner", 4, 0, 6, ramp)));
        serde_json::to_string(&document(&["ramp"], {
            // Build with an ordinary Retime first so the fixture validates,
            // then swap in the ramp on the wire.
            nodes
                .into_iter()
                .map(|(name, node)| {
                    if name == "ramp" {
                        (name, retime("inner", 4, 0, 6))
                    } else {
                        (name, node)
                    }
                })
                .collect()
        }))
        .unwrap()
        .replace(
            r#""pitch":"preserve""#,
            &format!(
                r#""pitch":{}"#,
                serde_json::to_string(&PitchPolicy::TapeRamp { ramp }).unwrap()
            ),
        )
    };
    let group = node(NodeKind::Sequence {
        children: vec![id("src")],
    });
    assert!(
        ProjectDocument::from_json(&wire(vec![("inner", group), ("src", source(6, 0, 6006))]))
            .is_ok()
    );
    for (name, inner) in [
        ("hold", vec![("inner", hold(6))]),
        (
            "seam",
            vec![
                (
                    "inner",
                    node(NodeKind::Sequence {
                        children: vec![id("a"), id("b")],
                    }),
                ),
                ("a", source(3, 0, 3003)),
                ("b", source(3, 3003, 6006)),
            ],
        ),
    ] {
        let error = ProjectDocument::from_json(&wire(inner)).unwrap_err();
        assert!(
            error.message.contains("exactly one Original beat"),
            "{name}: {error:?}"
        );
    }
}
