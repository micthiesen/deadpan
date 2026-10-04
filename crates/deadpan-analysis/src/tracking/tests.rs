use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use super::*;

fn rect(x: f64, y: f64, w: f64, h: f64) -> NormalizedRect {
    NormalizedRect::new(x, y, w, h).unwrap()
}

fn seen(pts: i64, region: NormalizedRect, confidence: f32) -> RawObservation {
    RawObservation {
        pts,
        region: Some(region),
        confidence,
    }
}

fn failed(pts: i64) -> RawObservation {
    RawObservation {
        pts,
        region: None,
        confidence: 0.0,
    }
}

fn pictures(count: i64) -> Vec<i64> {
    (0..count).map(|ordinal| ordinal * 10).collect()
}

fn states(path: &TrackedPath) -> Vec<TrackState> {
    path.samples().iter().map(|sample| sample.state).collect()
}

/// A square moving 0.01 per picture to the right.
fn moving(ordinal: i64) -> NormalizedRect {
    rect(0.1 + 0.01 * ordinal as f64, 0.4, 0.1, 0.1)
}

fn track(observations: &[RawObservation], count: i64) -> TrackedPath {
    TrackedPath::track(
        TrackPolicy::default(),
        1.0,
        &pictures(count),
        count * 10,
        TrackStop::RangeEnd,
        Keyframe {
            pts: 0,
            region: moving(0),
        },
        observations,
    )
    .unwrap()
}

#[test]
fn regions_are_bounded_and_rotations_round_trip() {
    assert!(NormalizedRect::new(0.5, 0.5, 0.6, 0.1).is_err());
    assert!(NormalizedRect::new(f64::NAN, 0.0, 0.1, 0.1).is_err());
    assert!(NormalizedRect::new(0.0, 0.0, 0.0, 0.1).is_err());
    let clipped = NormalizedRect::clipped(-0.1, 0.9, 0.3, 0.3).unwrap();
    assert!((clipped.width() - 0.2).abs() < 1e-12 && (clipped.height() - 0.1).abs() < 1e-12);
    assert_eq!((clipped.x(), clipped.y()), (0.0, 0.9));
    assert_eq!(NormalizedRect::clipped(1.1, 0.0, 0.3, 0.3), None);
    // A box at the top-left of a picture displayed a quarter turn clockwise
    // is at the bottom-left of the coded picture.
    let displayed = rect(0.0, 0.0, 0.2, 0.1);
    let coded = displayed.coded_from_displayed(1);
    assert!((coded.x() - 0.0).abs() < 1e-12 && (coded.y() - 0.8).abs() < 1e-12);
    assert!((coded.width() - 0.1).abs() < 1e-12 && (coded.height() - 0.2).abs() < 1e-12);
    assert_eq!(displayed.coded_from_displayed(4), displayed);
}

#[test]
fn confident_observations_track_and_short_gaps_interpolate() {
    let mut observations: Vec<_> = (0..12).map(|o| seen(o * 10, moving(o), 0.9)).collect();
    observations[4] = seen(40, moving(4), 0.1);
    observations[5] = failed(50);
    let path = track(&observations, 12);
    use TrackState::*;
    assert_eq!(
        states(&path),
        [
            Manual,
            Tracked,
            Tracked,
            Tracked,
            Interpolated,
            Interpolated,
            Tracked,
            Tracked,
            Tracked,
            Tracked,
            Tracked,
            Tracked
        ]
    );
    let gap = path.samples()[5].region;
    assert!((gap.x() - moving(5).x()).abs() < 1e-12);
    // Interpolated samples carry their neighbours' lower confidence, never the
    // rejected observation's.
    assert_eq!(path.samples()[5].confidence, Some(0.9));
    assert_eq!(path.samples()[4].confidence, Some(0.9));
}

#[test]
fn long_gaps_are_lost_then_held_until_corrected() {
    let mut observations: Vec<_> = (0..20).map(|o| seen(o * 10, moving(o), 0.9)).collect();
    for observation in &mut observations[3..11] {
        observation.confidence = 0.05;
    }
    let mut path = track(&observations, 20);
    use TrackState::*;
    let observed = states(&path);
    assert_eq!(&observed[..3], [Manual, Tracked, Tracked]);
    assert!(observed[3..11].iter().all(|state| *state == Lost));
    // The tracker reports the square again, but tracking does not resume.
    assert!(observed[11..].iter().all(|state| *state == Held));
    let last_confident = moving(2);
    assert!(
        path.samples()[3..]
            .iter()
            .all(|s| s.region == last_confident)
    );

    let range = path
        .correct(Keyframe {
            pts: 120,
            region: moving(12),
        })
        .unwrap();
    assert_eq!(
        range,
        TrackRange {
            start_pts: 120,
            end_pts: 200
        }
    );
    assert_eq!(path.samples()[12].state, Manual);
    assert!(path.samples()[13..].iter().all(|s| s.region == moving(12)));
    let retracked: Vec<_> = (12..20).map(|o| seen(o * 10, moving(o), 0.8)).collect();
    path.retrack(120, &pictures(20)[12..], &retracked).unwrap();
    assert!(path.samples()[13..].iter().all(|s| s.state == Tracked));
    assert_eq!(&states(&path)[..12], &observed[..12]);
}

#[test]
fn a_confident_jump_to_another_subject_is_a_loss() {
    let mut observations: Vec<_> = (0..12).map(|o| seen(o * 10, moving(o), 0.95)).collect();
    // Another subject on the far side of the picture, reported confidently.
    for observation in &mut observations[5..] {
        observation.region = Some(rect(0.8, 0.8, 0.1, 0.1));
    }
    let path = track(&observations, 12);
    assert!(path.samples().iter().all(|sample| sample.region.x() < 0.5));
    assert!(
        path.samples()[5..]
            .iter()
            .all(|sample| matches!(sample.state, TrackState::Lost | TrackState::Held))
    );
    // An area explosion is also a different subject.
    let mut grown: Vec<_> = (0..4).map(|o| seen(o * 10, moving(o), 0.95)).collect();
    grown[2].region = Some(rect(0.1, 0.4, 0.3, 0.3));
    let path = track(&grown, 4);
    assert_eq!(path.samples()[2].state, TrackState::Interpolated);
}

#[test]
fn a_trailing_gap_is_lost_and_strides_scale_motion() {
    let mut observations: Vec<_> = (0..6).map(|o| seen(o * 10, moving(o), 0.9)).collect();
    observations[5].confidence = 0.0;
    let path = track(&observations, 6);
    assert_eq!(path.samples()[5].state, TrackState::Lost);
    // With a stride of five, each step moves five pictures' worth.
    let strided: Vec<_> = (0..4)
        .map(|step| {
            let ordinal = step * 5;
            seen(
                ordinal * 10,
                rect(0.1 + 0.08 * step as f64, 0.4, 0.1, 0.1),
                0.9,
            )
        })
        .collect();
    let path = TrackedPath::track(
        TrackPolicy::default(),
        1.0,
        &pictures(20),
        200,
        TrackStop::RangeEnd,
        Keyframe {
            pts: 0,
            region: rect(0.1, 0.4, 0.1, 0.1),
        },
        &strided,
    )
    .unwrap();
    assert!(
        path.samples()[1..]
            .iter()
            .all(|s| s.state == TrackState::Tracked)
    );
}

#[test]
fn invalid_inputs_are_refused() {
    let seed = Keyframe {
        pts: 0,
        region: moving(0),
    };
    let policy = TrackPolicy::default();
    let stop = TrackStop::RangeEnd;
    // Observation at a PTS that is not a range picture.
    assert!(TrackedPath::track(policy, 1.0, &pictures(3), 30, stop, seed, &[failed(5)]).is_err());
    // Unordered observations.
    assert!(
        TrackedPath::track(
            policy,
            1.0,
            &pictures(3),
            30,
            stop,
            seed,
            &[failed(20), failed(10)]
        )
        .is_err()
    );
    // Pictures that do not begin at the keyframe or end inside the range.
    assert!(TrackedPath::track(policy, 1.0, &[10, 20], 30, stop, seed, &[]).is_err());
    assert!(TrackedPath::track(policy, 1.0, &pictures(3), 20, stop, seed, &[]).is_err());
    let invalid = TrackPolicy {
        min_confidence: 2.0,
        ..policy
    };
    assert!(TrackedPath::track(invalid, 1.0, &pictures(3), 30, stop, seed, &[]).is_err());
    let mut path = track(&[], 3);
    assert!(
        path.correct(Keyframe {
            pts: 30,
            region: moving(0)
        })
        .is_err()
    );
    assert!(path.retrack(10, &[10, 20], &[]).is_err());
}

#[test]
fn shot_boundaries_and_limits_end_the_range() {
    let boundaries = [10, 40, 90];
    assert_eq!(
        tracking_end(100, 12, 80, &boundaries, true).unwrap(),
        (40, TrackStop::ShotBoundary { picture: 40 })
    );
    assert_eq!(
        tracking_end(100, 12, 80, &boundaries, false).unwrap(),
        (80, TrackStop::RangeEnd)
    );
    // A boundary at the start picture does not stop it.
    assert_eq!(
        tracking_end(100, 40, 60, &boundaries, true).unwrap(),
        (60, TrackStop::RangeEnd)
    );
    assert_eq!(
        tracking_end(
            MAX_TRACK_PICTURES + 10,
            0,
            MAX_TRACK_PICTURES + 10,
            &[],
            true
        )
        .unwrap(),
        (MAX_TRACK_PICTURES, TrackStop::PictureLimit)
    );
    assert!(tracking_end(100, 50, 50, &[], true).is_err());
    assert!(tracking_end(100, 0, 101, &[], true).is_err());
}

#[test]
fn paths_round_trip_through_validated_json() {
    let observations: Vec<_> = (0..5).map(|o| seen(o * 10, moving(o), 0.9)).collect();
    let path = track(&observations, 5);
    let json = serde_json::to_value(&path).unwrap();
    assert_eq!(json["rule"], TRACK_RULE);
    assert_eq!(json["samples"][1]["state"], "tracked");
    let back: TrackedPath = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(back, path);
    let mut forged = json.clone();
    forged["samples"][2]["state"] = "manual".into();
    assert!(serde_json::from_value::<TrackedPath>(forged).is_err());
    let mut outside = json;
    outside["samples"][1]["region"]["x"] = 0.95.into();
    assert!(serde_json::from_value::<TrackedPath>(outside).is_err());
}

fn region_strategy() -> impl Strategy<Value = NormalizedRect> {
    (0.0..0.7_f64, 0.0..0.7_f64, 0.05..0.3_f64, 0.05..0.3_f64)
        .prop_map(|(x, y, w, h)| rect(x, y, w, h))
}

fn observation_strategy() -> impl Strategy<Value = (bool, NormalizedRect, f32, bool)> {
    (
        any::<bool>(),
        region_strategy(),
        0.0..1.0_f32,
        prop::bool::weighted(0.7),
    )
}

/// Observations near a slow random walk, with random failures, low
/// confidence and confident jumps elsewhere.
fn observations(
    seed: NormalizedRect,
    raw: &[(bool, NormalizedRect, f32, bool)],
    stride: usize,
) -> Vec<RawObservation> {
    let mut walk = seed;
    raw.iter()
        .enumerate()
        .map(|(step, (present, jump, confidence, near))| {
            let target = NormalizedRect::clipped(
                walk.x() + 0.01 * (step % 3) as f64 - 0.01,
                walk.y() + 0.005,
                walk.width(),
                walk.height(),
            )
            .unwrap_or(walk);
            walk = target;
            RawObservation {
                pts: ((step + 1) * stride) as i64 * 10,
                region: present.then_some(if *near { target } else { *jump }),
                confidence: *confidence,
            }
        })
        .collect()
}

proptest! {
    #[test]
    fn rotations_are_inverse(region in region_strategy(), turns in 0_u8..8) {
        let back = region.coded_from_displayed(turns).displayed_from_coded(turns);
        prop_assert!((back.x() - region.x()).abs() < 1e-12);
        prop_assert!((back.y() - region.y()).abs() < 1e-12);
        prop_assert!((back.width() - region.width()).abs() < 1e-12);
        prop_assert!((back.height() - region.height()).abs() < 1e-12);
        let coded = region.coded_from_displayed(turns);
        prop_assert!(NormalizedRect::new(coded.x(), coded.y(), coded.width(), coded.height()).is_ok());
    }

    #[test]
    fn the_policy_never_follows_an_implausible_or_unbounded_path(
        seed in region_strategy(),
        raw in prop::collection::vec(observation_strategy(), 0..60),
        stride in 1_usize..9,
        aspect in 0.4_f64..2.5,
    ) {
        let policy = TrackPolicy::default();
        let observed = observations(seed, &raw, stride);
        let count = (raw.len() + 1) * stride;
        let all = pictures(count as i64);
        let path = TrackedPath::track(
            policy,
            aspect,
            &all,
            count as i64 * 10,
            TrackStop::RangeEnd,
            Keyframe { pts: 0, region: seed },
            &observed,
        ).unwrap();
        let samples = path.samples();
        prop_assert_eq!(samples.len(), observed.len() + 1);
        prop_assert_eq!(samples[0].state, TrackState::Manual);
        let ordinal = |pts: i64| (pts / 10) as usize;
        // Last confident region, ordinal and confidence (manual counts as 1).
        let mut anchor = (samples[0].region, 0_usize, 1.0_f32);
        let mut lost = false;
        for (index, sample) in samples.iter().enumerate().skip(1) {
            let observation = &observed[index - 1];
            prop_assert_eq!(sample.pts, observation.pts);
            match sample.state {
                TrackState::Tracked => {
                    prop_assert!(!lost);
                    prop_assert_eq!(Some(sample.region), observation.region);
                    prop_assert!(observation.confidence >= policy.min_confidence);
                    prop_assert_eq!(sample.confidence, Some(observation.confidence));
                    let elapsed = ordinal(sample.pts) - anchor.1;
                    prop_assert!(policy.plausible(&anchor.0, &sample.region, elapsed, aspect));
                    anchor = (sample.region, ordinal(sample.pts), observation.confidence);
                }
                TrackState::Interpolated => {
                    prop_assert!(!lost);
                    // A confident neighbour closes every interpolated run,
                    // within the gap bound in pictures, whatever the stride.
                    let closing = samples[index..]
                        .iter()
                        .find(|s| s.state != TrackState::Interpolated)
                        .copied();
                    prop_assert_eq!(closing.map(|s| s.state), Some(TrackState::Tracked));
                    let closing = closing.unwrap();
                    prop_assert!(
                        ordinal(closing.pts) - anchor.1 - 1 <= policy.max_interpolated_gap as usize
                    );
                    let next = closing.region;
                    let within = |a: f64, b: f64, value: f64| {
                        (a.min(b) - 1e-12..=a.max(b) + 1e-12).contains(&value)
                    };
                    prop_assert!(within(anchor.0.x(), next.x(), sample.region.x()));
                    prop_assert!(within(anchor.0.y(), next.y(), sample.region.y()));
                    prop_assert!(within(anchor.0.width(), next.width(), sample.region.width()));
                    prop_assert!(within(anchor.0.height(), next.height(), sample.region.height()));
                    prop_assert_eq!(
                        sample.confidence,
                        Some(anchor.2.min(closing.confidence.unwrap()))
                    );
                }
                TrackState::Lost | TrackState::Held => {
                    prop_assert_eq!(sample.region, anchor.0);
                    prop_assert_eq!(sample.confidence, None);
                    if sample.state == TrackState::Held {
                        prop_assert!(lost);
                    }
                    lost = true;
                    // Tracking never resumes by itself after a loss.
                    prop_assert!(!samples[index..].iter().any(|s| matches!(
                        s.state,
                        TrackState::Tracked | TrackState::Interpolated
                    )));
                }
                TrackState::Manual => prop_assert!(false, "unexpected manual sample"),
            }
        }
        // serde_json's default float parsing may differ by an ulp, so compare
        // what validation must preserve exactly.
        let json = serde_json::to_string(&path).unwrap();
        let back = serde_json::from_str::<TrackedPath>(&json).unwrap();
        let shape = |path: &TrackedPath| -> Vec<_> {
            path.samples().iter().map(|s| (s.pts, s.state, s.confidence)).collect()
        };
        prop_assert_eq!(shape(&back), shape(&path));
    }

    #[test]
    fn corrections_invalidate_only_their_range(
        seed in region_strategy(),
        raw in prop::collection::vec(observation_strategy(), 4..40),
        corrections in prop::collection::vec((any::<prop::sample::Index>(), region_strategy()), 1..4),
    ) {
        let observed = observations(seed, &raw, 1);
        let count = raw.len() as i64 + 1;
        let all = pictures(count);
        let mut path = TrackedPath::track(
            TrackPolicy::default(),
            1.0,
            &all,
            count * 10,
            TrackStop::RangeEnd,
            Keyframe { pts: 0, region: seed },
            &observed,
        ).unwrap();
        for (index, region) in corrections {
            let pts = all[index.index(all.len())];
            let before = path.clone();
            let range = path.correct(Keyframe { pts, region }).unwrap();
            prop_assert_eq!(range.start_pts, pts);
            for (old, new) in before.samples().iter().zip(path.samples()) {
                if old.pts < range.start_pts || old.pts >= range.end_pts {
                    prop_assert_eq!(old, new);
                }
            }
            prop_assert_eq!(before.samples().len(), path.samples().len());
            // Re-tracking the same range from fresh observations changes
            // nothing outside it either.
            let first = all.binary_search(&range.start_pts).unwrap();
            let last = all.partition_point(|&p| p < range.end_pts);
            let fresh: Vec<_> = all[first..last]
                .iter()
                .map(|&p| seen(p, region, 0.9))
                .collect();
            let corrected = path.clone();
            path.retrack(pts, &all[first..last], &fresh).unwrap();
            for (old, new) in corrected.samples().iter().zip(path.samples()) {
                if old.pts < range.start_pts || old.pts >= range.end_pts {
                    prop_assert_eq!(old, new);
                } else if old.pts > range.start_pts {
                    prop_assert_eq!(new.state, TrackState::Tracked);
                }
            }
        }
    }
}

#[test]
fn distance_is_measured_in_the_display_aspect() {
    let policy = TrackPolicy::default();
    let from = rect(0.1, 0.4, 0.1, 0.1);
    let to = rect(0.25, 0.4, 0.1, 0.1);
    // 0.15 of a landscape picture's width is 0.15 of its longer side.
    assert!(!policy.plausible(&from, &to, 1, 16.0 / 9.0));
    // In a portrait picture the width is the shorter side: 0.15 × 9/16.
    assert!(policy.plausible(&from, &to, 1, 9.0 / 16.0));
    // Vertical movement is scaled the other way round.
    let down = rect(0.1, 0.55, 0.1, 0.1);
    assert!(policy.plausible(&from, &down, 1, 16.0 / 9.0));
    assert!(!policy.plausible(&from, &down, 1, 9.0 / 16.0));
}

#[test]
fn interpolation_is_bounded_in_pictures_not_observations() {
    // Stride 30: one rejected observation spans 59 pictures.
    let all = pictures(91);
    let observations = [
        seen(300, moving(0), 0.9),
        failed(600),
        seen(900, moving(0), 0.9),
    ];
    let path = TrackedPath::track(
        TrackPolicy::default(),
        1.0,
        &all,
        910,
        TrackStop::RangeEnd,
        Keyframe {
            pts: 0,
            region: moving(0),
        },
        &observations,
    )
    .unwrap();
    use TrackState::*;
    assert_eq!(states(&path), [Manual, Tracked, Lost, Held]);
    // Stride 3: the same single gap spans five pictures and interpolates.
    let short = [
        seen(30, moving(0), 0.9),
        failed(60),
        seen(90, moving(0), 0.9),
    ];
    let path = TrackedPath::track(
        TrackPolicy::default(),
        1.0,
        &all,
        910,
        TrackStop::RangeEnd,
        Keyframe {
            pts: 0,
            region: moving(0),
        },
        &short,
    )
    .unwrap();
    assert_eq!(states(&path), [Manual, Tracked, Interpolated, Tracked]);
}

#[test]
fn corrections_at_unanalysed_pictures_are_all_or_nothing() {
    // Stride 2: pictures 0, 2, 4 … are analysed.
    let all = pictures(10);
    let observations: Vec<_> = (0..5)
        .map(|step| seen(step * 20, moving(step * 2), 0.9))
        .collect();
    let mut path = TrackedPath::track(
        TrackPolicy::default(),
        1.0,
        &all,
        100,
        TrackStop::RangeEnd,
        Keyframe {
            pts: 0,
            region: moving(0),
        },
        &observations,
    )
    .unwrap();
    let before = path.clone();
    // Re-tracking with observations that are not range pictures fails and
    // leaves the path exactly as it was.
    assert!(
        path.correct_and_retrack(
            Keyframe {
                pts: 30,
                region: moving(3)
            },
            &all[3..],
            &[failed(35)],
        )
        .is_err()
    );
    assert_eq!(path, before);
    assert!(
        path.correct(Keyframe {
            pts: 100,
            region: moving(3)
        })
        .is_err()
    );
    assert_eq!(path, before);
    // Picture 3 was never analysed; its correction still applies there.
    let fresh: Vec<_> = [30, 50, 70, 90]
        .iter()
        .map(|&pts| seen(pts, moving(pts / 10), 0.8))
        .collect();
    let range = path
        .correct_and_retrack(
            Keyframe {
                pts: 30,
                region: moving(3),
            },
            &all[3..],
            &fresh,
        )
        .unwrap();
    assert_eq!((range.start_pts, range.end_pts), (30, 100));
    let pts: Vec<_> = path.samples().iter().map(|s| (s.pts, s.state)).collect();
    use TrackState::*;
    assert_eq!(
        pts,
        [
            (0, Manual),
            (20, Tracked),
            (30, Manual),
            (50, Tracked),
            (70, Tracked),
            (90, Tracked)
        ]
    );
}

fn time_base() -> deadpan_core::SourceTimeBase {
    deadpan_core::SourceTimeBase::new(1, 1_000).unwrap()
}

fn core_region_at(
    target: &deadpan_core::AttentionTarget,
    pts: i64,
) -> (deadpan_core::TargetRegion, deadpan_core::TargetSource) {
    target
        .region_at(deadpan_core::SourcePoint {
            ticks: deadpan_core::ExactRatio::integer(pts),
            time_base: time_base(),
        })
        .unwrap()
}

/// Every stored or dropped sample is reproduced by core `region_at` within the
/// tolerance plus one millionth of rounding.
fn assert_reconstructed(
    path: &TrackedPath,
    target: &deadpan_core::AttentionTarget,
    tolerance: u32,
) -> Result<(), TestCaseError> {
    for sample in path.samples() {
        let (region, source) = core_region_at(target, sample.pts);
        let expected = target_region(&sample.region);
        if sample.state == TrackState::Manual {
            prop_assert_eq!(region, expected);
            continue;
        }
        prop_assert!(matches!(source, deadpan_core::TargetSource::Tracked(_)));
        for axis in 0..2 {
            prop_assert!(region.center[axis].abs_diff(expected.center[axis]) <= tolerance + 1);
            prop_assert!(region.size[axis].abs_diff(expected.size[axis]) <= tolerance + 1);
        }
    }
    Ok(())
}

#[test]
fn dense_paths_compact_within_core_bounds_or_refuse() {
    // A smooth 10,000-picture path compacts far below 4,096 samples.
    let count = 10_000_i64;
    let region = |ordinal: i64| {
        let t = ordinal as f64 / count as f64;
        rect(0.1 + 0.6 * t, 0.2 + 0.3 * (t * 6.0).sin().abs(), 0.1, 0.1)
    };
    let observations: Vec<_> = (0..count).map(|o| seen(o, region(o), 0.9)).collect();
    let all: Vec<i64> = (0..count).collect();
    let path = TrackedPath::track(
        TrackPolicy::default(),
        1.0,
        &all,
        count,
        TrackStop::RangeEnd,
        Keyframe {
            pts: 0,
            region: region(0),
        },
        &observations,
    )
    .unwrap();
    let asset = deadpan_core::AssetId::new("clip").unwrap();
    let (target, tolerance) = path
        .to_target("Square".into(), asset.clone(), time_base(), "test", 4_096)
        .unwrap();
    assert!(target.samples.len() < 4_096, "{}", target.samples.len());
    assert_eq!(tolerance, COMPACTION_TOLERANCES[0]);
    let provenance = target.provenance.as_ref().unwrap();
    assert_eq!(provenance.rule, TARGET_RULE);
    // The core rule's wire name is this policy's identity.
    assert_eq!(serde_json::to_value(TARGET_RULE).unwrap(), TRACK_RULE);
    assert_eq!(provenance.stop, deadpan_core::TargetStop::RangeEnd);
    assert_reconstructed(&path, &target, tolerance).unwrap();
    // A budget too small for any tolerance refuses rather than coarsen more.
    assert_eq!(
        path.to_target("Square".into(), asset, time_base(), "test", 2),
        Err(TrackError::Limit("saved target sample"))
    );
}

#[test]
fn mapping_rounds_and_maps_states_and_confidence() {
    let region = target_region(&rect(0.1, 0.2, 0.3333333, 0.25));
    assert_eq!(region.center, [266_667, 325_000]);
    assert_eq!(region.size, [333_333, 250_000]);
    assert_eq!(confidence_thousandths(Some(0.4995)), 500);
    assert_eq!(confidence_thousandths(Some(0.4994)), 499);
    assert_eq!(confidence_thousandths(None), 0);
    let back = rect_from_target(&region).unwrap();
    assert!((back.x() - 0.1).abs() < 1e-6 && (back.width() - 0.333333).abs() < 1e-6);
    let mut observations: Vec<_> = (0..20).map(|o| seen(o * 10, moving(o), 0.9)).collect();
    for observation in &mut observations[3..11] {
        observation.confidence = 0.05;
    }
    let path = track(&observations, 20);
    let (target, _) = path
        .to_target(
            "Square".into(),
            deadpan_core::AssetId::new("clip").unwrap(),
            time_base(),
            "test",
            4_096,
        )
        .unwrap();
    // Lost and held both become core lost without confidence, and the held
    // run compacts to its first sample.
    let lost: Vec<_> = target
        .samples
        .iter()
        .filter(|s| s.state == deadpan_core::TrackState::Lost)
        .collect();
    assert_eq!(lost.len(), 1);
    assert_eq!(lost[0].confidence, 0);
    assert_eq!(lost[0].at, 30);
    assert_eq!(core_region_at(&target, 190).0, target_region(&moving(2)));
}

proptest! {
    #[test]
    fn saved_targets_reproduce_the_path_and_corrections_replace_only_their_range(
        seed in region_strategy(),
        raw in prop::collection::vec(observation_strategy(), 1..80),
        stride in 1_usize..4,
        correction in any::<prop::sample::Index>(),
        corrected in region_strategy(),
    ) {
        let observed = observations(seed, &raw, stride);
        let count = (raw.len() + 1) * stride;
        let all = pictures(count as i64);
        let path = TrackedPath::track(
            TrackPolicy::default(),
            1.0,
            &all,
            count as i64 * 10,
            TrackStop::RangeEnd,
            Keyframe { pts: 0, region: seed },
            &observed,
        ).unwrap();
        let asset = deadpan_core::AssetId::new("clip").unwrap();
        let (target, tolerance) = path
            .to_target("Subject".into(), asset, time_base(), "test", 4_096)
            .unwrap();
        prop_assert!(target.samples.len() <= path.samples().len());
        assert_reconstructed(&path, &target, tolerance)?;

        // Re-track from a correction at any picture: only its range changes.
        let at = all[correction.index(all.len())];
        let first = all.binary_search(&at).unwrap();
        let fresh: Vec<_> = all[first..].iter().map(|&pts| seen(pts, corrected, 0.9)).collect();
        let segment = TrackedPath::track(
            TrackPolicy::default(),
            1.0,
            &all[first..],
            count as i64 * 10,
            TrackStop::RangeEnd,
            Keyframe { pts: at, region: corrected },
            &fresh,
        ).unwrap();
        let (retracked, _) = retrack_target(&target, &segment, "later engine", 4_096).unwrap();
        for sample in &target.samples {
            if sample.at < at {
                prop_assert!(retracked.samples.contains(sample));
            }
        }
        prop_assert_eq!(core_region_at(&retracked, at).0, target_region(&corrected));
        let provenance = retracked.provenance.as_ref().unwrap();
        prop_assert_eq!(provenance.engine.as_str(), "later engine");
        prop_assert_eq!(provenance.stop, target.provenance.as_ref().unwrap().stop);
        for &pts in &all[first..] {
            prop_assert_eq!(core_region_at(&retracked, pts).0, target_region(&corrected));
        }
        // Before the correction nothing moves by more than the one millionth
        // of rounding where an interpolated line now ends a tick early.
        for &pts in &all[..first] {
            let (now, now_source) = core_region_at(&retracked, pts);
            let (was, was_source) = core_region_at(&target, pts);
            prop_assert_eq!(now_source, was_source);
            for axis in 0..2 {
                prop_assert!(now.center[axis].abs_diff(was.center[axis]) <= 1);
                prop_assert!(now.size[axis].abs_diff(was.size[axis]) <= 1);
            }
        }
    }
}
