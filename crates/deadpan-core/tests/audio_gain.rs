use deadpan_core::*;
use serde_json::json;

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn gain(value: i32) -> GainDb {
    GainDb::new(value).unwrap()
}
fn range(start: ExactRatio, end: ExactRatio) -> GainRange {
    GainRange::new(start, end).unwrap()
}
fn envelope(from: i32, to: i32, curve: GainCurve) -> GainEnvelope {
    GainEnvelope::new(
        GainClock::OwnerOutput,
        range(ExactRatio::ZERO, ExactRatio::ONE),
        gain(from),
        vec![GainSegment::new(ExactRatio::ONE, gain(to), curve).unwrap()],
    )
    .unwrap()
}
fn treatment(envelopes: Vec<GainEnvelope>) -> AudioTreatments {
    AudioTreatments::from_clip_gain(ClipGain::new(gain(0), false, envelopes, vec![]).unwrap())
}

#[test]
fn gain_is_db_interpolation_with_independent_quarter_point_oracles() {
    // At t=1/4 and 3/4 these Bernstein and smoothstep fractions are dyadic,
    // so the declared Q32 grid represents the independently computed values.
    for (curve, expected) in [
        (GainCurve::Step, [-12_000, -12_000, -12_000]),
        (GainCurve::Linear, [-6_000, 0, 6_000]),
        (GainCurve::Smoothstep, [-8_250, 0, 8_250]),
        (
            GainCurve::Cubic {
                control1: gain(24_000),
                control2: gain(-24_000),
            },
            [1_875, 0, -1_875],
        ),
    ] {
        let envelope = envelope(-12_000, 12_000, curve);
        for (quarter, expected) in (1..=3).zip(expected) {
            assert_eq!(
                envelope.evaluate(ratio(quarter, 4)).unwrap(),
                ExactRatio::integer(expected)
            );
        }
        assert_eq!(
            envelope.evaluate(ExactRatio::ZERO).unwrap(),
            ExactRatio::integer(-12_000)
        );
        assert_eq!(
            envelope.evaluate(ExactRatio::ONE).unwrap(),
            ExactRatio::ZERO
        );
    }
    assert_eq!(
        envelope(-12_000, 0, GainCurve::Linear)
            .evaluate(ratio(1, 2))
            .unwrap(),
        ExactRatio::integer(-6_000)
    );
}

#[test]
fn discontinuities_are_right_continuous_and_range_exits_are_unity() {
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        range(ratio(1, 4), ratio(5, 4)),
        gain(-6_000),
        vec![
            GainSegment::new(ratio(3, 4), gain(9_000), GainCurve::Step).unwrap(),
            GainSegment::new(ratio(5, 4), gain(-3_000), GainCurve::Linear).unwrap(),
        ],
    )
    .unwrap();
    for (at, expected) in [
        (ratio(-1, 1), 0),
        (ratio(1, 4), -6_000),
        (ratio(74, 100), -6_000),
        (ratio(3, 4), 9_000),
        (ratio(1, 1), 3_000),
        (ratio(5, 4), 0),
        (ratio(2, 1), 0),
    ] {
        assert_eq!(
            envelope.evaluate(at).unwrap(),
            ExactRatio::integer(expected)
        );
    }
}

#[test]
fn sample_derived_keys_and_extent_clipping_never_normalize_the_curve() {
    let one_sample = ratio(30_000, 48_000 * 1001);
    let at = |sample| one_sample.checked_mul(ExactRatio::integer(sample)).unwrap();
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        range(at(113), at(117)),
        gain(0),
        vec![GainSegment::new(at(117), gain(12_000), GainCurve::Linear).unwrap()],
    )
    .unwrap();
    for (sample, expected) in [
        (112, 0),
        (113, 0),
        (114, 3_000),
        (115, 6_000),
        (116, 9_000),
        (117, 0),
    ] {
        assert_eq!(
            envelope.evaluate(at(sample)).unwrap(),
            ExactRatio::integer(expected)
        );
    }
    // Owners supply an allocation. Reading only a short prefix cannot rewrite
    // the curve, and returning later exposes the same suffix at the same time.
    let before = serde_json::to_string(&envelope).unwrap();
    envelope.evaluate(at(114)).unwrap();
    assert_eq!(
        envelope.evaluate(at(116)).unwrap(),
        ExactRatio::integer(9_000)
    );
    assert_eq!(serde_json::to_string(&envelope).unwrap(), before);
}

#[test]
fn full_width_ratio_times_use_bounded_wide_products_instead_of_overflowing_subtraction() {
    let n = i128::MAX - 4;
    let start = ratio(n, n + 1);
    let middle = ratio(n + 1, n + 2);
    let end = ratio(n + 2, n + 3);
    assert!(middle.checked_sub(start).is_err());
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        range(start, end),
        gain(-12_000),
        vec![GainSegment::new(end, gain(12_000), GainCurve::Linear).unwrap()],
    )
    .unwrap();
    // Exact progress is (n+3)/(2(n+2)), less than half a Q32 step above 1/2.
    assert_eq!(envelope.evaluate(middle).unwrap(), ExactRatio::ZERO);
    assert_eq!(
        envelope.evaluate(start).unwrap(),
        ExactRatio::integer(-12_000)
    );
    assert_eq!(envelope.evaluate(end).unwrap(), ExactRatio::ZERO);
    assert_eq!(
        envelope.evaluate(ratio(i128::MIN, 1)).unwrap(),
        ExactRatio::ZERO
    );
    let p = 1_i128 << 124;
    let start = ratio(p, 4 * p + 3);
    let middle = ratio(2 * p + 1, 4 * p + 7);
    let end = ratio(3 * p + 1, 4 * p + 11);
    let curve = GainEnvelope::new(
        GainClock::OwnerOutput,
        range(start, end),
        gain(-12_000),
        vec![GainSegment::new(end, gain(12_000), GainCurve::Linear).unwrap()],
    )
    .unwrap();
    assert_eq!(curve.evaluate(middle).unwrap(), ExactRatio::ZERO);
}

#[test]
fn q32_progress_uses_ties_to_even_without_changing_segment_selection() {
    let curve = envelope(0, 1, GainCurve::Linear);
    let q = i128::from(GAIN_NUMERIC_SCALE);
    for (numerator, rounded) in [(1, 0), (3, 2), (5, 2), (7, 4)] {
        assert_eq!(
            curve.evaluate(ratio(numerator, 2 * q)).unwrap(),
            ratio(rounded, q)
        );
    }
    let step = envelope(-3_000, 12_000, GainCurve::Step);
    // An interior point whose numeric progress would round to one stays in
    // the Step's old segment; time comparisons never use that numeric grid.
    assert_eq!(
        step.evaluate(ratio(2 * q - 1, 2 * q)).unwrap(),
        ExactRatio::integer(-3_000)
    );
}

#[test]
fn non_dyadic_boundary_neighbors_never_quantize_onto_the_wrong_side() {
    let n = (i128::MAX - 1) / 3;
    let key = ratio(1, 3);
    let end = ratio(2, 3);
    let before_key = ratio(n, 3 * n + 1);
    let after_key = ratio(n, 3 * n - 1);
    let before_end = ratio(2 * n, 3 * n + 1);
    let after_end = ratio(2 * n, 3 * n - 1);
    let curve = GainEnvelope::new(
        GainClock::OwnerOutput,
        range(ExactRatio::ZERO, ExactRatio::ONE),
        gain(0),
        vec![
            GainSegment::new(key, gain(9_000), GainCurve::Step).unwrap(),
            GainSegment::new(ExactRatio::ONE, gain(0), GainCurve::Step).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(curve.evaluate(before_key).unwrap(), ExactRatio::ZERO);
    assert_eq!(curve.evaluate(key).unwrap(), ExactRatio::integer(9_000));
    assert_eq!(
        curve.evaluate(after_key).unwrap(),
        ExactRatio::integer(9_000)
    );
    let clip = ClipGain::new(gain(0), false, vec![], vec![range(key, end)]).unwrap();
    for (at, muted) in [
        (before_key, false),
        (key, true),
        (after_key, true),
        (before_end, true),
        (end, false),
        (after_end, false),
    ] {
        assert_eq!(clip.evaluate(at).unwrap().muted, muted);
    }
}

#[test]
fn overlaps_add_db_and_true_mute_is_separate_from_finite_attenuation() {
    let quiet = ClipGain::new(gain(-96_000), false, vec![], vec![]).unwrap();
    let evaluated = quiet.evaluate(ExactRatio::ZERO).unwrap();
    assert!(!evaluated.muted);
    assert_eq!(evaluated.decibels().unwrap(), ExactRatio::integer(-96));
    let loud = ClipGain::new(
        gain(24_000),
        false,
        vec![envelope(24_000, 24_000, GainCurve::Step); 2],
        vec![range(ratio(1, 4), ratio(3, 4))],
    )
    .unwrap();
    for (at, muted) in [
        (ExactRatio::ZERO, false),
        (ratio(1, 4), true),
        (ratio(1, 2), true),
        (ratio(3, 4), false),
    ] {
        let evaluated = loud.evaluate(at).unwrap();
        assert_eq!(evaluated.millidecibels, ExactRatio::integer(72_000));
        assert_eq!(evaluated.muted, muted);
    }
    let whole = ClipGain::new(gain(0), true, vec![], vec![]).unwrap();
    assert!(whole.evaluate(ratio(50, 1)).unwrap().muted);
}

#[test]
fn independent_trim_adjustment_preserves_curves_mutes_and_rejects_without_mutation() {
    let original = ClipGain::new(
        gain(0),
        true,
        vec![envelope(
            -12_000,
            12_000,
            GainCurve::Cubic {
                control1: gain(24_000),
                control2: gain(-24_000),
            },
        )],
        vec![range(ratio(1, 4), ratio(3, 4))],
    )
    .unwrap();
    let adjusted = original.adjust_trim(3_000).unwrap();
    assert_eq!(adjusted.trim(), gain(3_000));
    assert_eq!(adjusted.envelopes(), original.envelopes());
    assert_eq!(adjusted.mute_ranges(), original.mute_ranges());
    assert_eq!(adjusted.muted(), original.muted());
    assert_eq!(original.trim(), gain(0));
    assert_eq!(adjusted.adjust_trim(24_000), Err(GainError::ValueRange));
    assert_eq!(gain(24_000).adjusted(i32::MAX), Err(GainError::Overflow));
    assert_eq!(adjusted.trim(), gain(3_000));
}

#[test]
fn closed_wire_and_typed_ingress_share_validation() {
    let treatment = treatment(vec![envelope(-3_000, 6_000, GainCurve::Smoothstep)]);
    let wire = serde_json::to_value(&treatment).unwrap();
    assert_eq!(wire["order"], json!(["clip_gain"]));
    assert_eq!(wire["clip_gain"]["envelopes"][0]["clock"], "owner_output");
    assert_eq!(
        serde_json::from_value::<AudioTreatments>(wire.clone()).unwrap(),
        treatment
    );
    for (pointer, replacement) in [
        ("/order", json!([])),
        ("/order", json!(["saturation"])),
        ("/clip_gain/trim", json!(24_001)),
        ("/clip_gain/trim", json!(-96_001)),
        ("/clip_gain/trim", json!(3.5)),
        ("/clip_gain/muted", json!("true")),
        ("/clip_gain/envelopes/0/clock", json!("normalized")),
        ("/clip_gain/envelopes/0/initial", json!("NaN")),
        ("/clip_gain/envelopes/0/segments", json!([])),
        (
            "/clip_gain/envelopes/0/segments/0/end",
            serde_json::to_value(ratio(1, 2)).unwrap(),
        ),
        (
            "/clip_gain/envelopes/0/segments/0/curve",
            json!({"type":"linear", "control1":0}),
        ),
        (
            "/clip_gain/envelopes/0/segments/0/curve",
            json!({"type":"step", "future":null}),
        ),
        (
            "/clip_gain/envelopes/0/segments/0/curve",
            json!({"type":"smoothstep", "future":{}}),
        ),
        (
            "/clip_gain/envelopes/0/segments/0/curve",
            json!({"type":"cubic", "control1":24_001,"control2":0}),
        ),
    ] {
        let mut bad = wire.clone();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            serde_json::from_value::<AudioTreatments>(bad).is_err(),
            "{pointer}"
        );
    }
    for raw in [
        r#"{"order":[],"clip_gain":null,"future":null}"#,
        r#"{"order":[],"clip_gain":null,"order":[]}"#,
        r#"{"order":[]}"#,
        r#"{"order":["clip_gain"],"clip_gain":null}"#,
    ] {
        assert!(
            serde_json::from_str::<AudioTreatments>(raw).is_err(),
            "{raw}"
        );
    }
    assert!(AudioTreatments::new(vec![], Some(ClipGain::default())).is_err());
    assert!(GainRange::new(ratio(-1, 1), ExactRatio::ONE).is_err());
    assert!(GainRange::new(ExactRatio::ONE, ExactRatio::ONE).is_err());
    assert!(GainRange::new(ExactRatio::ONE, ExactRatio::ZERO).is_err());
    assert!(GainSegment::new(ExactRatio::ZERO, gain(0), GainCurve::Step).is_err());
    assert!(
        GainEnvelope::new(
            GainClock::OwnerOutput,
            range(ExactRatio::ZERO, ExactRatio::ONE),
            gain(0),
            vec![
                GainSegment::new(ratio(1, 2), gain(0), GainCurve::Step).unwrap(),
                GainSegment::new(ratio(1, 2), gain(0), GainCurve::Step).unwrap(),
            ]
        )
        .is_err()
    );
}

#[test]
fn every_curve_has_a_closed_standalone_wire_without_changing_its_public_shape() {
    for curve in [
        GainCurve::Step,
        GainCurve::Linear,
        GainCurve::Smoothstep,
        GainCurve::Cubic {
            control1: gain(-96_000),
            control2: gain(24_000),
        },
    ] {
        let value = serde_json::to_value(curve).unwrap();
        assert_eq!(
            serde_json::from_value::<GainCurve>(value.clone()).unwrap(),
            curve
        );
        let mut unknown = value;
        unknown["future"] = json!(null);
        assert!(serde_json::from_value::<GainCurve>(unknown).is_err());
    }
    for raw in [
        r#"{"type":"step","type":"step"}"#,
        r#"{"type":"cubic","control1":0,"control1":0,"control2":0}"#,
        r#"{"type":"cubic","control1":0}"#,
    ] {
        assert!(serde_json::from_str::<GainCurve>(raw).is_err(), "{raw}");
    }
    assert_eq!(
        serde_json::from_str::<GainCurve>(r#"{"control2":24000,"control1":-96000,"type":"cubic"}"#)
            .unwrap(),
        GainCurve::Cubic {
            control1: gain(-96_000),
            control2: gain(24_000)
        }
    );
}

#[test]
fn malformed_curve_payloads_fail_before_depth_parsing_or_content_buffering() {
    // Deliberately unterminated, far past serde_json's ordinary depth cap.
    // A buffering parser reports EOF/depth instead of the first typed defect.
    let deep = "[".repeat(512);
    for (prefix, expected) in [
        (r#"{"type":"linear","future":"#, "unknown field `future`"),
        (r#"{"future":"#, "unknown field `future`"),
        (r#"{"type":"cubic","control1":"#, "expected i32"),
        (r#"{"control2":"#, "expected i32"),
        (r#"{"type":"#, "gain curve type string"),
        (
            r#"{"type":"step","control1":"#,
            "only a cubic gain curve accepts controls",
        ),
        (
            r#"{"type":"cubic","control1":0,"control1":"#,
            "duplicate field `control1`",
        ),
        (r#"{"type":"linear","type":"#, "duplicate field `type`"),
    ] {
        let error = serde_json::from_str::<GainCurve>(&format!("{prefix}{deep}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(
            !error.contains("recursion limit") && !error.contains("EOF"),
            "{error}"
        );
    }
}

#[test]
fn standalone_json_has_an_upfront_byte_cap_and_fits_the_maximum_recipe() {
    let large = i128::MAX / 128;
    let end = ratio(large * MAX_GAIN_SEGMENTS as i128 + 1, large * 65 + 7);
    let curve = GainEnvelope::new(
        GainClock::OwnerOutput,
        range(ExactRatio::ZERO, end),
        gain(24_000),
        (1..=MAX_GAIN_SEGMENTS)
            .map(|n| {
                GainSegment::new(
                    ratio(large * n as i128 + 1, large * 65 + 7),
                    gain(-96_000),
                    GainCurve::Cubic {
                        control1: gain(24_000),
                        control2: gain(-96_000),
                    },
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap();
    let recipe = AudioTreatments::from_clip_gain(
        ClipGain::new(
            gain(-96_000),
            true,
            vec![curve.clone(); MAX_GAIN_ENVELOPES],
            vec![curve.range(); MAX_GAIN_MUTE_RANGES],
        )
        .unwrap(),
    );
    let compact = recipe.to_json().unwrap();
    assert!(compact.len() < MAX_AUDIO_TREATMENTS_JSON_BYTES);
    assert_eq!(AudioTreatments::from_json(&compact).unwrap(), recipe);
    let mut exact_cap = " ".repeat(MAX_AUDIO_TREATMENTS_JSON_BYTES - compact.len());
    exact_cap.push_str(&compact);
    assert_eq!(AudioTreatments::from_json(&exact_cap).unwrap(), recipe);
    exact_cap.push(' ');
    assert_eq!(
        AudioTreatments::from_json(&exact_cap).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
    // The inherited ExactRatio decoder is not allowed to allocate this long
    // decimal before the standalone helper checks the enclosing byte bound.
    let mut wire = serde_json::to_value(treatment(vec![envelope(0, 0, GainCurve::Step)])).unwrap();
    wire["clip_gain"]["envelopes"][0]["segments"][0]["end"]["numerator"] =
        json!("9".repeat(MAX_AUDIO_TREATMENTS_JSON_BYTES));
    assert_eq!(
        AudioTreatments::from_json(&wire.to_string())
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    assert_eq!(
        AudioTreatments::from_json(&AudioTreatments::default().to_json().unwrap()).unwrap(),
        AudioTreatments::default()
    );
}

#[test]
fn collection_limits_reject_typed_and_wire_growth_before_parsing_extra_records() {
    let curve = envelope(0, 3_000, GainCurve::Linear);
    assert_eq!(
        ClipGain::new(
            gain(0),
            false,
            vec![curve.clone(); MAX_GAIN_ENVELOPES + 1],
            vec![]
        ),
        Err(GainError::Limit)
    );
    assert_eq!(
        ClipGain::new(
            gain(0),
            false,
            vec![],
            vec![curve.range(); MAX_GAIN_MUTE_RANGES + 1]
        ),
        Err(GainError::Limit)
    );
    let mut wire = serde_json::to_value(treatment(vec![])).unwrap();
    for (name, value, maximum) in [
        (
            "envelopes",
            serde_json::to_value(&curve).unwrap(),
            MAX_GAIN_ENVELOPES,
        ),
        (
            "mute_ranges",
            serde_json::to_value(curve.range()).unwrap(),
            MAX_GAIN_MUTE_RANGES,
        ),
    ] {
        let mut records = vec![value; maximum];
        records.push(json!({"invalid_payload": {"not_even_a_gain_record": true}}));
        wire["clip_gain"][name] = json!(records);
        let error = serde_json::from_str::<AudioTreatments>(&wire.to_string()).unwrap_err();
        assert!(error.to_string().contains("gain exceeds"), "{error}");
        wire["clip_gain"][name] = json!([]);
    }
    let segments: Vec<_> = (1..=MAX_GAIN_SEGMENTS)
        .map(|n| {
            GainSegment::new(
                ratio(n as i128, MAX_GAIN_SEGMENTS as i128),
                gain(0),
                GainCurve::Linear,
            )
            .unwrap()
        })
        .collect();
    let maximum = GainEnvelope::new(
        GainClock::OwnerOutput,
        curve.range(),
        gain(0),
        segments.clone(),
    )
    .unwrap();
    let mut oversized = segments;
    oversized.push(GainSegment::new(ratio(2, 1), gain(0), GainCurve::Step).unwrap());
    assert_eq!(
        GainEnvelope::new(GainClock::OwnerOutput, curve.range(), gain(0), oversized),
        Err(GainError::Limit)
    );
    let mut wire = serde_json::to_value(maximum).unwrap();
    wire["segments"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bad":true}));
    let error = serde_json::from_str::<GainEnvelope>(&wire.to_string()).unwrap_err();
    assert!(error.to_string().contains("gain exceeds"), "{error}");
    assert!(
        serde_json::from_str::<AudioTreatments>(r#"{"order":["clip_gain",{}],"clip_gain":null}"#)
            .unwrap_err()
            .to_string()
            .contains("gain exceeds")
    );
}

#[test]
fn record_and_owner_path_limits_are_separate_and_include_explicit_unity() {
    let curve = GainEnvelope::new(
        GainClock::OwnerOutput,
        range(ExactRatio::ZERO, ExactRatio::ONE),
        gain(0),
        (1..=MAX_GAIN_SEGMENTS)
            .map(|n| {
                GainSegment::new(
                    ratio(n as i128, MAX_GAIN_SEGMENTS as i128),
                    gain(0),
                    GainCurve::Cubic {
                        control1: gain(0),
                        control2: gain(0),
                    },
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap();
    let treatment = treatment(vec![curve; MAX_GAIN_ENVELOPES]);
    assert_eq!(
        treatment.record_count(),
        2 + MAX_GAIN_ENVELOPES * (1 + 3 * MAX_GAIN_SEGMENTS)
    );
    let count = MAX_GAIN_RECORDS / treatment.record_count();
    assert!(validate_audio_treatments(std::iter::repeat_n(&treatment, count)).is_ok());
    assert_eq!(
        validate_audio_treatments(std::iter::repeat_n(&treatment, count + 1)),
        Err(GainError::Limit)
    );
    let unity = AudioTreatments::from_clip_gain(ClipGain::default());
    assert_eq!(
        validate_audio_treatment_layers(std::iter::repeat_n(&unity, MAX_GAIN_LAYERS)),
        Ok(MAX_GAIN_LAYERS)
    );
    assert_eq!(
        validate_audio_treatment_layers(std::iter::repeat_n(&unity, MAX_GAIN_LAYERS + 1)),
        Err(GainError::Limit)
    );
    assert!(AudioTreatments::default().is_empty());
    assert_eq!(
        AudioTreatments::default().evaluate(ratio(1, 2)).unwrap(),
        EvaluatedGain::UNITY
    );
}
