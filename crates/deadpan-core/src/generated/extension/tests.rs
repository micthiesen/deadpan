use super::*;

fn frames(count: i64) -> FrameDuration {
    FrameDuration::new(count).unwrap()
}

fn map(
    direction: ExtensionDirection,
    context: i64,
    generated: i64,
    output: i64,
) -> ExtensionSamplingMap {
    ExtensionSamplingMap::new(
        direction,
        FrameRate::new(24, 1).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        frames(context),
        frames(generated),
        frames(output),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap()
}

/// Independently find the neighboring native picture centers on an integer
/// grid. This deliberately does not use the production clamped formula.
fn center_oracle(generated: i64, output: i64, j: i64) -> ExactRatio {
    let destination_center = (2 * j + 1) * generated;
    let centers: Vec<i64> = (0..generated).map(|q| (2 * q + 1) * output).collect();
    let Some(upper) = centers
        .iter()
        .position(|center| *center >= destination_center)
    else {
        return ExactRatio::integer(generated - 1);
    };
    if upper == 0 {
        return ExactRatio::ZERO;
    }
    let lower = upper - 1;
    ExactRatio::integer(i64::try_from(lower).unwrap())
        .checked_add(
            ExactRatio::new(
                i128::from(destination_center - centers[lower]),
                i128::from(centers[upper] - centers[lower]),
            )
            .unwrap(),
        )
        .unwrap()
}

#[test]
fn extension_sampling_matches_independent_center_oracle_and_never_reads_context() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for context in 1..=4 {
            for generated in 1..=12 {
                for output in 1..=18 {
                    let sampling = map(direction, context, generated, output);
                    let interval = sampling.generated_interval();
                    for j in 0..output {
                        let position = sampling.native_position(j).unwrap();
                        let expected = center_oracle(generated, output, j)
                            .checked_add(ExactRatio::integer(interval.start))
                            .unwrap();
                        assert_eq!(
                            position, expected,
                            "{direction:?} K={context} E={generated} N={output} j={j}"
                        );
                        assert!(interval.contains(&i64::try_from(position.floor()).unwrap()));
                        assert!(
                            interval.contains(&i64::try_from(position.ceil().unwrap()).unwrap())
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn extension_nine_plus_eight_to_twelve_clamps_generated_edges() {
    let left = map(ExtensionDirection::FromLeft, 9, 8, 12);
    let right = map(ExtensionDirection::FromRight, 9, 8, 12);
    assert_eq!(left.native_position(0).unwrap(), ExactRatio::integer(9));
    assert_eq!(left.native_position(11).unwrap(), ExactRatio::integer(16));
    assert_eq!(right.native_position(0).unwrap(), ExactRatio::ZERO);
    assert_eq!(right.native_position(11).unwrap(), ExactRatio::integer(7));
    assert_eq!(
        left.native_position(1).unwrap(),
        ExactRatio::new(19, 2).unwrap()
    );
    assert_eq!(
        right.native_position(10).unwrap(),
        ExactRatio::new(13, 2).unwrap()
    );
    assert_eq!(left.native_frame_count(), frames(17));
    assert_eq!(left.generated_interval(), 9..17);
    assert_eq!(left.context_interval(), 0..9);
    assert_eq!(right.generated_interval(), 0..8);
    assert_eq!(right.context_interval(), 8..17);
    assert_eq!(left.output_interval(), 0..12);
    assert_eq!(left.context_anchor_index(), 8);
    assert_eq!(right.context_anchor_index(), 8);
}

#[test]
fn extension_one_output_uses_center_and_equal_count_is_identity() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let single = map(direction, 9, 8, 1);
        assert_eq!(
            single.native_position(0).unwrap(),
            ExactRatio::new(7, 2)
                .unwrap()
                .checked_add(ExactRatio::integer(single.generated_interval().start))
                .unwrap()
        );
        let equal = map(direction, 9, 8, 8);
        for j in 0..8 {
            assert_eq!(
                equal.native_position(j).unwrap(),
                ExactRatio::integer(equal.generated_interval().start + j)
            );
        }
    }
}

#[test]
fn extension_core_accepts_general_counts_and_rates_without_provider_rules() {
    let sampling = ExtensionSamplingMap::new(
        ExtensionDirection::FromRight,
        FrameRate::new(30000, 1001).unwrap(),
        FrameRate::new(24000, 1001).unwrap(),
        frames(2),
        frames(3),
        frames(5),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap();
    assert_eq!(sampling.native_frame_count(), frames(5));
    assert_eq!(sampling.native_position(2).unwrap(), ExactRatio::ONE);
    assert_eq!(
        sampling.project_rate(),
        FrameRate::new(30000, 1001).unwrap()
    );
}

#[test]
fn extension_sampling_checks_counts_indices_and_maximum_arithmetic() {
    let rate = FrameRate::new(24, 1).unwrap();
    for counts in [(0, 8, 1), (1, 0, 1), (1, 8, 0)] {
        assert_eq!(
            ExtensionSamplingMap::new(
                ExtensionDirection::FromLeft,
                rate,
                rate,
                frames(counts.0),
                frames(counts.1),
                frames(counts.2),
                BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
            )
            .unwrap_err()
            .code,
            GeneratedErrorCode::InvalidSamplingMap
        );
    }
    assert_eq!(
        ExtensionSamplingMap::new(
            ExtensionDirection::FromLeft,
            rate,
            rate,
            frames(1),
            frames(i64::MAX),
            frames(1),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap_err()
        .code,
        GeneratedErrorCode::SamplingOverflow
    );
    for (context, generated) in [(1, i64::MAX - 1), (i64::MAX - 1, 1)] {
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            let sampling = map(direction, context, generated, i64::MAX);
            for j in [0, 1, i64::MAX / 2, i64::MAX - 1] {
                let position = sampling.native_position(j).unwrap();
                assert!(
                    sampling
                        .generated_interval()
                        .contains(&i64::try_from(position.floor()).unwrap())
                );
                assert!(
                    sampling
                        .generated_interval()
                        .contains(&i64::try_from(position.ceil().unwrap()).unwrap())
                );
            }
            for index in [-1, i64::MAX] {
                assert_eq!(
                    sampling.native_position(index).unwrap_err().code,
                    GeneratedErrorCode::SamplingIndexOutOfRange
                );
            }
        }
    }
}

#[test]
fn extension_sampling_wire_is_versioned_strict_and_revalidated() {
    let sampling = map(ExtensionDirection::FromLeft, 9, 8, 12);
    let wire = serde_json::to_value(&sampling).unwrap();
    assert_eq!(wire["schema_version"], 1);
    assert_eq!(wire["direction"], "from_left");
    assert_eq!(wire["policy"], "frame_centers_clamped");
    assert_eq!(
        serde_json::from_value::<ExtensionSamplingMap>(wire.clone()).unwrap(),
        sampling
    );
    for (key, value) in [
        ("schema_version", serde_json::json!(2)),
        ("direction", serde_json::json!("after")),
        ("policy", serde_json::json!("interior_only")),
        ("interpolation", serde_json::json!("nearest")),
        ("context_frame_count", serde_json::json!(0)),
        ("generated_frame_count", serde_json::json!(0)),
        ("output_frame_count", serde_json::json!(-1)),
        ("native_frame_count", serde_json::json!(17)),
    ] {
        let mut bad = wire.clone();
        bad[key] = value;
        assert!(
            serde_json::from_value::<ExtensionSamplingMap>(bad).is_err(),
            "{key}"
        );
    }
    let mut missing = wire;
    missing.as_object_mut().unwrap().remove("policy");
    assert!(serde_json::from_value::<ExtensionSamplingMap>(missing).is_err());
}
