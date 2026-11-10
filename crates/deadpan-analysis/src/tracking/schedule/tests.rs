use proptest::prelude::*;

use super::*;
use crate::tracking::{TrackSample, TrackState};

fn rect(x: f64) -> NormalizedRect {
    NormalizedRect::new(x, 0.4, 0.1, 0.1).unwrap()
}

fn pictures(count: usize) -> Vec<i64> {
    (0..count as i64).map(|ordinal| ordinal * 10).collect()
}

/// Run a plan to completion from `cursor`, recording the order.
fn order(plan: &RangePlan, cursor: usize) -> Vec<usize> {
    let mut completed = vec![false; plan.ranges().len()];
    let mut order = Vec::new();
    while let Some(index) = plan.next(&completed, cursor) {
        assert!(plan.ready(index, &completed));
        completed[index] = true;
        order.push(index);
    }
    order
}

#[test]
fn ranges_cover_each_segment_in_whole_strides() {
    let plan = RangePlan::new(25, &[0, 13], 2, 5).unwrap();
    let spans: Vec<_> = plan
        .ranges()
        .iter()
        .map(|range| (range.segment, range.first, range.end, range.seed))
        .collect();
    // Width 5 rounds up to 6 pictures (three strides of 2).
    assert_eq!(
        spans,
        [
            (0, 0, 6, 0),
            (0, 6, 12, 4),
            (0, 12, 13, 10),
            (1, 13, 19, 13),
            (1, 19, 25, 17)
        ]
    );
    // Analysed pictures stay on the keyframe's stride grid across ranges.
    let analysed: Vec<Vec<usize>> = (0..plan.ranges().len())
        .map(|index| plan.analysed(index).collect())
        .collect();
    assert_eq!(analysed[1], [6, 8, 10]);
    assert_eq!(analysed[2], [12]);
    assert_eq!(analysed[3], [13, 15, 17]);
    assert!(RangePlan::new(10, &[1], 1, 4).is_err());
    assert!(RangePlan::new(10, &[0, 0], 1, 4).is_err());
    assert!(RangePlan::new(10, &[0, 10], 1, 4).is_err());
    assert!(RangePlan::new(10, &[0], 0, 4).is_err());
}

#[test]
fn one_segment_runs_forward_from_its_keyframe_whatever_the_cursor() {
    let plan = RangePlan::new(40, &[0], 1, 10).unwrap();
    // Tracking is causal: the cursor's range needs every earlier range's seed.
    for cursor in [0, 25, 39, 500] {
        assert_eq!(order(&plan, cursor), [0, 1, 2, 3]);
    }
}

#[test]
fn the_cursor_neighbourhood_is_finished_first_then_outward() {
    // Keyframes (the selection and two corrections) at 0, 30 and 60.
    let plan = RangePlan::new(90, &[0, 30, 60], 1, 10).unwrap();
    // Ranges 0..=2 own [0,30), 3..=5 [30,60), 6..=8 [60,90).
    // The cursor at 35 is inside range 3, which its keyframe seeds directly.
    let ordered = order(&plan, 35);
    // The cursor's range, its forward continuation, then the next keyframe's
    // first range (a tie at 26 pictures prefers the side after the cursor),
    // then the reverse neighbourhood, reached forward from the earlier
    // keyframe, nearest-first as it becomes ready.
    assert_eq!(ordered, [3, 4, 5, 6, 0, 1, 2, 7, 8]);
    // Each segment still runs forward from its keyframe.
    for segment in 0..3 {
        let in_segment: Vec<_> = ordered
            .iter()
            .copied()
            .filter(|&index| plan.ranges()[index].segment == segment)
            .collect();
        assert!(in_segment.windows(2).all(|pair| pair[0] < pair[1]));
    }
    // Ties at equal distance prefer the range after the cursor: at 29 the
    // range [20,30) contains it, but it is not ready; [30,40) at distance 1
    // and the segment start [0,10) at distance 20 are.
    let at_seam = order(&plan, 29);
    assert_eq!(at_seam[0], 3);
}

#[test]
fn completed_ranges_resume_without_rerunning() {
    let plan = RangePlan::new(40, &[0], 1, 10).unwrap();
    // A cancelled run kept ranges 0 and 1 in the cache.
    let mut completed = vec![true, true, false, false];
    assert_eq!(plan.covered(&completed), 20);
    assert_eq!(plan.next(&completed, 0), Some(2));
    completed[2] = true;
    completed[3] = true;
    assert_eq!(plan.next(&completed, 0), None);
    assert_eq!(plan.covered(&completed), 40);
    // A later completed range alone covers nothing contiguous.
    assert_eq!(plan.covered(&[false, true, true, true]), 0);
}

/// Confident observations of a square moving 0.01 per picture, with a short
/// gap and a long loss.
fn observed(count: usize) -> Vec<RawObservation> {
    (0..count)
        .map(|ordinal| RawObservation {
            pts: ordinal as i64 * 10,
            region: match ordinal {
                7 | 8 => None,
                30..=40 => Some(rect(0.9)),
                _ => Some(rect(0.1 + 0.01 * ordinal as f64)),
            },
            confidence: if ordinal == 9 { 0.1 } else { 0.9 },
        })
        .collect()
}

/// Split one run's observations by owned range, dropping continuation seeds.
fn split(plan: &RangePlan, all: &[RawObservation], stride: usize) -> Vec<Vec<RawObservation>> {
    (0..plan.ranges().len())
        .map(|index| {
            plan.analysed(index)
                .map(|ordinal| all[ordinal / stride])
                .collect()
        })
        .collect()
}

#[test]
fn assembled_ranges_equal_one_run_over_the_same_observations() {
    for stride in [1_usize, 2, 3] {
        for width in [1, 4, 9, 50] {
            let count = 47;
            let pictures = pictures(count);
            let all: Vec<_> = observed(count).into_iter().step_by(stride).collect();
            let seed = Keyframe {
                pts: 0,
                region: rect(0.1),
            };
            let stop = TrackStop::ShotBoundary { picture: 47 };
            let whole = TrackedPath::track(
                TrackPolicy::default(),
                16.0 / 9.0,
                &pictures,
                470,
                stop,
                seed,
                &all,
            )
            .unwrap();
            let plan = RangePlan::new(count, &[0], stride as u32, width).unwrap();
            let owned = split(&plan, &all, stride);
            let ranges: Vec<_> = owned.into_iter().map(Some).collect();
            let assembled = plan
                .assemble(
                    TrackPolicy::default(),
                    16.0 / 9.0,
                    &pictures,
                    470,
                    stop,
                    &[seed],
                    &ranges,
                )
                .unwrap()
                .unwrap();
            assert_eq!(assembled, whole, "stride {stride} width {width}");
        }
    }
}

#[test]
fn a_partial_prefix_ends_at_its_first_missing_range() {
    let count = 40;
    let pictures = pictures(count);
    let all = observed(count);
    let seed = Keyframe {
        pts: 0,
        region: rect(0.1),
    };
    let plan = RangePlan::new(count, &[0], 1, 10).unwrap();
    let mut ranges: Vec<_> = split(&plan, &all, 1).into_iter().map(Some).collect();
    ranges[2] = None;
    let partial = plan
        .assemble(
            TrackPolicy::default(),
            1.0,
            &pictures,
            400,
            TrackStop::PictureLimit,
            &[seed],
            &ranges,
        )
        .unwrap()
        .unwrap();
    assert_eq!(partial.end_pts(), 200);
    assert_eq!(partial.stop(), TrackStop::RangeEnd);
    assert_eq!(partial.samples().len(), 20);
    // The prefix is the complete path's prefix up to the short gap's end.
    let whole = TrackedPath::track(
        TrackPolicy::default(),
        1.0,
        &pictures,
        400,
        TrackStop::PictureLimit,
        seed,
        &all,
    )
    .unwrap();
    assert_eq!(partial.samples(), &whole.samples()[..20]);
    ranges[0] = None;
    assert_eq!(
        plan.assemble(
            TrackPolicy::default(),
            1.0,
            &pictures,
            400,
            TrackStop::RangeEnd,
            &[seed],
            &ranges
        )
        .unwrap(),
        None
    );
}

#[test]
fn later_keyframes_assemble_like_corrections() {
    let count = 40;
    let pictures = pictures(count);
    let all = observed(count);
    let first = Keyframe {
        pts: 0,
        region: rect(0.1),
    };
    let correction = Keyframe {
        pts: 250,
        region: rect(0.6),
    };
    // Reference: one run, then a correction re-tracking its range.
    let mut reference = TrackedPath::track(
        TrackPolicy::default(),
        1.0,
        &pictures,
        400,
        TrackStop::RangeEnd,
        first,
        &all,
    )
    .unwrap();
    reference
        .correct_and_retrack(correction, &pictures[25..], &all[25..])
        .unwrap();
    let plan = RangePlan::new(count, &[0, 25], 1, 7).unwrap();
    let ranges: Vec<_> = split(&plan, &all, 1).into_iter().map(Some).collect();
    let assembled = plan
        .assemble(
            TrackPolicy::default(),
            1.0,
            &pictures,
            400,
            TrackStop::RangeEnd,
            &[first, correction],
            &ranges,
        )
        .unwrap()
        .unwrap();
    assert_eq!(assembled, reference);
    assert_eq!(assembled.keyframes(), [first, correction]);
    assert!(matches!(
        assembled.samples()[25],
        TrackSample {
            state: TrackState::Manual,
            ..
        }
    ));
}

#[test]
fn continuations_use_the_last_reported_region() {
    let seed = rect(0.1);
    let reports = [
        RawObservation {
            pts: 0,
            region: Some(rect(0.2)),
            confidence: 0.9,
        },
        RawObservation {
            pts: 10,
            region: None,
            confidence: 0.0,
        },
    ];
    assert_eq!(continuation_region(seed, &reports), rect(0.2));
    assert_eq!(continuation_region(seed, &reports[1..]), seed);
}

proptest! {
    #[test]
    fn every_order_visits_each_range_once_after_its_seed(
        count in 1_usize..200,
        stride in 1_u32..5,
        width in 1_usize..40,
        cursor in 0_usize..250,
        cuts in proptest::collection::btree_set(1_usize..200, 0..4),
    ) {
        let mut keyframes = vec![0];
        keyframes.extend(cuts.into_iter().filter(|&cut| cut < count));
        let plan = RangePlan::new(count, &keyframes, stride, width).unwrap();
        let ordered = order(&plan, cursor);
        let mut sorted = ordered.clone();
        sorted.sort_unstable();
        prop_assert_eq!(sorted, (0..plan.ranges().len()).collect::<Vec<_>>());
        // The first range is the nearest one any keyframe can seed.
        let clamped = cursor.min(count - 1);
        let first = plan.ranges()[ordered[0]];
        let nearest = plan
            .ranges()
            .iter()
            .filter(|range| !range.continues())
            .map(|range| range.distance(clamped))
            .min()
            .unwrap();
        prop_assert_eq!(first.distance(clamped), nearest);
        // Owned pictures tile the tracked pictures exactly.
        let mut owned: Vec<_> = plan.ranges().iter().flat_map(|r| r.first..r.end).collect();
        owned.sort_unstable();
        prop_assert_eq!(owned, (0..count).collect::<Vec<_>>());
    }
}
