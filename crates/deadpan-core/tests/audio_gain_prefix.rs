use deadpan_core::*;

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}
fn gain(value: i32) -> GainDb {
    GainDb::new(value).unwrap()
}
fn range(start: ExactRatio, end: ExactRatio) -> GainRange {
    GainRange::new(start, end).unwrap()
}
fn prefix(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn recipe(curve: GainCurve, muted: bool) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(
            gain(3000),
            muted,
            vec![
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    range(ratio(1, 4), ratio(9, 4)),
                    gain(-12_000),
                    vec![
                        GainSegment::new(ratio(5, 4), gain(12_000), curve).unwrap(),
                        GainSegment::new(ratio(9, 4), gain(6000), GainCurve::Step).unwrap(),
                    ],
                )
                .unwrap(),
            ],
            vec![range(ratio(1, 2), ratio(3, 4))],
        )
        .unwrap(),
    )
}

#[test]
fn physical_prefix_translates_every_key_and_preserves_independent_gain_oracles() {
    for (curve, quarter_values) in [
        (GainCurve::Step, [-12_000, -12_000, -12_000]),
        (GainCurve::Linear, [-6000, 0, 6000]),
        (GainCurve::Smoothstep, [-8250, 0, 8250]),
        (
            GainCurve::Cubic {
                control1: gain(24_000),
                control2: gain(-24_000),
            },
            [1875, 0, -1875],
        ),
    ] {
        let original = recipe(curve, false);
        let encoded = original.to_json().unwrap();
        let shifted = original.with_owner_prefix(prefix(3)).unwrap();
        assert_eq!(original.to_json().unwrap(), encoded);
        assert_eq!(shifted.record_count(), original.record_count());
        assert_eq!(shifted.order(), original.order());
        let clip = shifted.clip_gain().unwrap();
        assert_eq!(clip.trim(), gain(3000));
        assert!(!clip.muted());
        let envelope = &clip.envelopes()[0];
        assert_eq!(envelope.clock(), GainClock::OwnerOutput);
        assert_eq!(envelope.initial(), gain(-12_000));
        assert_eq!(envelope.range(), range(ratio(13, 4), ratio(21, 4)));
        assert_eq!(envelope.segments()[0].end(), ratio(17, 4));
        assert_eq!(envelope.segments()[0].curve(), curve);
        assert_eq!(envelope.segments()[1].end(), ratio(21, 4));
        assert_eq!(envelope.segments()[1].value(), gain(6000));
        assert_eq!(clip.mute_ranges(), &[range(ratio(7, 2), ratio(15, 4))]);
        for (quarter, expected) in (1..=3).zip(quarter_values) {
            assert_eq!(
                shifted
                    .evaluate(ratio(13 + quarter, 4))
                    .unwrap()
                    .millidecibels,
                ExactRatio::integer(3000 + expected)
            );
        }
        for (at, muted) in [
            (ratio(349, 100), false),
            (ratio(7, 2), true),
            (ratio(374, 100), true),
            (ratio(15, 4), false),
        ] {
            assert_eq!(shifted.evaluate(at).unwrap().muted, muted);
        }
        for (at, expected) in [
            (ratio(3, 1), 3000),
            (ratio(13, 4), -9000),
            (ratio(17, 4), 15_000),
            (ratio(21, 4), 3000),
        ] {
            assert_eq!(
                shifted.evaluate(at).unwrap().millidecibels,
                ExactRatio::integer(expected)
            );
        }
        assert_eq!(
            AudioTreatments::from_json(&shifted.to_json().unwrap()).unwrap(),
            shifted
        );
        assert_eq!(original.with_owner_prefix(prefix(0)).unwrap(), original);
        assert_eq!(
            original
                .with_owner_prefix(prefix(1))
                .unwrap()
                .with_owner_prefix(prefix(2))
                .unwrap(),
            shifted
        );
        assert!(
            recipe(curve, true)
                .with_owner_prefix(prefix(3))
                .unwrap()
                .evaluate(ExactRatio::ZERO)
                .unwrap()
                .muted
        );
    }
}

#[test]
fn new_prefix_factor_uses_current_keys_without_retranslating_existing_factors() {
    let shifted = recipe(GainCurve::Linear, false)
        .with_owner_prefix(prefix(3))
        .unwrap();
    let clip = shifted.clip_gain().unwrap();
    let mut envelopes = clip.envelopes().to_vec();
    envelopes.push(
        GainEnvelope::new(
            GainClock::OwnerOutput,
            range(ExactRatio::ZERO, ExactRatio::integer(3)),
            gain(-6000),
            vec![GainSegment::new(ExactRatio::integer(3), gain(-6000), GainCurve::Step).unwrap()],
        )
        .unwrap(),
    );
    let current = AudioTreatments::from_clip_gain(
        ClipGain::new(
            clip.trim(),
            clip.muted(),
            envelopes,
            clip.mute_ranges().to_vec(),
        )
        .unwrap(),
    );
    assert_eq!(
        current.clip_gain().unwrap().envelopes()[0],
        clip.envelopes()[0]
    );
    for (at, expected) in [
        (ratio(2, 1), -3000),
        (ratio(3, 1), 3000),
        (ratio(13, 4), -9000),
        (ratio(4, 1), 9000),
    ] {
        assert_eq!(
            current.evaluate(at).unwrap().millidecibels,
            ExactRatio::integer(expected)
        );
    }
    assert_eq!(
        AudioTreatments::default()
            .with_owner_prefix(prefix(3))
            .unwrap(),
        AudioTreatments::default()
    );
    let unity = AudioTreatments::from_clip_gain(ClipGain::default());
    assert_eq!(unity.with_owner_prefix(prefix(3)).unwrap(), unity);
}

#[test]
fn unrepresentable_shift_rejects_without_mutating_any_existing_factor() {
    let huge = range(ratio(i128::MAX - 1, 1), ratio(i128::MAX, 1));
    let base = recipe(GainCurve::Linear, false);
    let clip = base.clip_gain().unwrap();
    for mute in [false, true] {
        let mut envelopes = clip.envelopes().to_vec();
        let mut ranges = clip.mute_ranges().to_vec();
        if mute {
            ranges.push(huge);
        } else {
            envelopes.push(
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    huge,
                    gain(0),
                    vec![GainSegment::new(huge.end(), gain(6000), GainCurve::Linear).unwrap()],
                )
                .unwrap(),
            );
        }
        let original = AudioTreatments::from_clip_gain(
            ClipGain::new(clip.trim(), false, envelopes, ranges).unwrap(),
        );
        let encoded = original.to_json().unwrap();
        assert_eq!(
            original.with_owner_prefix(prefix(1)),
            Err(GainError::Overflow)
        );
        assert_eq!(original.to_json().unwrap(), encoded);
    }
    assert!(FrameDuration::new(-1).is_err());
}
