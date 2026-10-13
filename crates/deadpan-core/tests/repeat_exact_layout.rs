use std::collections::BTreeMap;

use deadpan_core::{
    DocumentErrorCode, ExactRatio, ExactRepeatLayout, FrameDuration, InsertionBias, IterationId,
    IterationOrder, NodeId, PlayOverride, PlayOverrides, RepeatLayout, RevisionId,
};
use proptest::prelude::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn overrides(entries: Vec<(IterationId, NodeId)>) -> PlayOverrides {
    PlayOverrides::try_from(
        entries
            .into_iter()
            .map(|(iteration, root)| PlayOverride { iteration, root })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

#[derive(Debug)]
struct Expected {
    index: u32,
    iteration: IterationId,
    child: NodeId,
    gap_child: Option<NodeId>,
    start: i64,
    duration: i64,
    gap: i64,
}

/// Independent expanded integer-tick oracle. The production layout receives
/// reduced rational extents with unrelated denominators, not these ticks.
fn check_expanded(
    choices: &[(u8, u8, bool, bool)],
    default_child: u8,
    default_gap: u8,
    denominator: i64,
) {
    let count = u32::try_from(choices.len()).unwrap();
    let original = IterationOrder::new(revision("initial"), count).unwrap();
    let order = if count >= 4 {
        original.moved(0, 2, count - 2).unwrap()
    } else {
        original
    };
    let mut durations = BTreeMap::from([(
        id("base"),
        ratio(i128::from(default_child), i128::from(denominator)),
    )]);
    let mut plays = Vec::new();
    let mut gaps = Vec::new();
    let mut expected = Vec::new();
    let mut cursor = 0_i64;
    for (index, &(child, gap, override_play, override_gap)) in choices.iter().enumerate() {
        let index = u32::try_from(index).unwrap();
        let iteration = order.at(index).unwrap();
        let child_id = if override_play {
            id(&format!("child-{index}"))
        } else {
            id("base")
        };
        let gap_child = override_gap.then(|| id(&format!("gap-{index}")));
        let child = if override_play { child } else { default_child };
        let gap = if override_gap { gap } else { default_gap };
        if override_play {
            plays.push((iteration.clone(), child_id.clone()));
            durations.insert(
                child_id.clone(),
                ratio(i128::from(child), i128::from(denominator)),
            );
        }
        if let Some(gap_child) = &gap_child {
            gaps.push((iteration.clone(), gap_child.clone()));
            durations.insert(
                gap_child.clone(),
                ratio(i128::from(gap), i128::from(denominator)),
            );
        }
        let gap = if index + 1 == count {
            0
        } else {
            i64::from(gap)
        };
        expected.push(Expected {
            index,
            iteration,
            child: child_id,
            gap_child,
            start: cursor,
            duration: i64::from(child),
            gap,
        });
        cursor += i64::from(child) + gap;
    }
    let plays = overrides(plays);
    let gaps = overrides(gaps);
    let layout = ExactRepeatLayout::compile_with_gap_overrides(
        &order,
        &id("base"),
        Some(&plays),
        ratio(i128::from(default_gap), i128::from(denominator)),
        Some(&gaps),
        &durations,
    )
    .unwrap();
    let exact = |value: i64| ratio(i128::from(value), i128::from(denominator));
    assert_eq!(layout.duration(), exact(cursor));
    assert!(layout.segment_count() <= choices.len());
    for expected in &expected {
        let play = layout.play(&expected.iteration).unwrap().unwrap();
        assert_eq!(play.index, expected.index);
        assert_eq!(play.start, exact(expected.start));
        assert_eq!(play.duration, exact(expected.duration));
        assert_eq!(play.gap_after, exact(expected.gap));
        assert_eq!(play.child, expected.child);
        assert_eq!(play.gap_child, expected.gap_child);
        assert_eq!(
            play.branch_offset(&play.child).unwrap(),
            Some(exact(expected.start))
        );
        if let Some(gap_child) = &expected.gap_child {
            assert_eq!(
                play.branch_offset(gap_child).unwrap(),
                (expected.gap != 0).then(|| exact(expected.start + expected.duration))
            );
        }
        assert_eq!(play.branch_offset(&id("foreign")).unwrap(), None);
    }
    // Every half tick tests interiors as well as all fractional play/gap seams.
    for tick in 0..=cursor * 2 {
        let position = ratio(i128::from(tick), i128::from(denominator) * 2);
        for bias in [InsertionBias::Left, InsertionBias::Right] {
            if tick == cursor * 2 && bias == InsertionBias::Right {
                assert!(layout.locate(position, bias).is_err());
                continue;
            }
            let selected = expected
                .iter()
                .find(|play| {
                    let end = (play.start + play.duration + play.gap) * 2;
                    if bias == InsertionBias::Left {
                        tick <= end
                    } else {
                        tick < end
                    }
                })
                .unwrap();
            let gap_start = (selected.start + selected.duration) * 2;
            let in_gap = tick > gap_start
                || (tick == gap_start && bias == InsertionBias::Right && selected.gap > 0);
            let found = layout.locate(position, bias).unwrap();
            assert_eq!(found.play.iteration, selected.iteration);
            assert_eq!(found.in_gap, in_gap);
            let origin = selected.start + if in_gap { selected.duration } else { 0 };
            assert_eq!(
                found.position,
                ratio(i128::from(tick - origin * 2), i128::from(denominator) * 2)
            );
            assert_eq!(
                layout
                    .locate_bounded(position, bias, found.comparisons)
                    .unwrap(),
                found
            );
            assert_eq!(
                layout
                    .locate_bounded(position, bias, found.comparisons - 1)
                    .unwrap_err()
                    .code,
                DocumentErrorCode::LimitExceeded
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn fractional_prefixes_match_expanded_tick_oracle(
        choices in prop::collection::vec((1_u8..=12, 0_u8..=8, any::<bool>(), any::<bool>()), 1..=18),
        default_child in 1_u8..=12,
        default_gap in 0_u8..=8,
        denominator in 2_i64..=12,
    ) {
        check_expanded(&choices, default_child, default_gap, denominator);
    }
}

#[test]
fn billion_fractional_plays_remain_compact_and_lookup_is_bounded() {
    let order = IterationOrder::new(revision("many"), 1_000_000_000).unwrap();
    let middle = order.at(500_000_000).unwrap();
    let plays = overrides(vec![(middle.clone(), id("long"))]);
    let durations = BTreeMap::from([(id("base"), ratio(3, 2)), (id("long"), ratio(9, 2))]);
    let layout =
        ExactRepeatLayout::compile(&order, &id("base"), Some(&plays), ratio(1, 3), &durations)
            .unwrap();
    assert_eq!(layout.segment_count(), 3);
    assert_eq!(layout.duration(), ratio(5_500_000_008, 3));
    let play = layout.play(&middle).unwrap().unwrap();
    assert_eq!(play.start, ratio(2_750_000_000, 3));
    for bias in [InsertionBias::Left, InsertionBias::Right] {
        let found = layout.locate(play.start, bias).unwrap();
        assert!(found.comparisons <= 2);
        assert_eq!(
            found.play.index,
            if bias == InsertionBias::Left {
                499_999_999
            } else {
                500_000_000
            }
        );
        assert_eq!(
            layout.locate_bounded(play.start, bias, 0).unwrap_err().code,
            DocumentErrorCode::LimitExceeded
        );
    }
    assert!(
        layout
            .play(&IterationId {
                allocation: revision("retired"),
                ordinal: 0
            })
            .unwrap()
            .is_none()
    );
}

#[test]
fn integer_and_exact_facades_keep_identical_stable_identities_and_edges() {
    let original = IterationOrder::new(revision("old"), 6).unwrap();
    let order = original
        .moved(0, 2, 4)
        .unwrap()
        .resized(8, revision("new"))
        .unwrap();
    let plays = overrides(vec![(original.at(1).unwrap(), id("alternate"))]);
    let gaps = overrides(vec![(original.at(4).unwrap(), id("gap"))]);
    let durations = BTreeMap::from([
        (id("base"), FrameDuration::new(3).unwrap()),
        (id("alternate"), FrameDuration::new(7).unwrap()),
        (id("gap"), FrameDuration::new(2).unwrap()),
    ]);
    let exact_durations = durations
        .iter()
        .map(|(id, d)| (id.clone(), ExactRatio::integer(d.frames())))
        .collect();
    let ordinary = RepeatLayout::compile_with_gap_overrides(
        &order,
        &id("base"),
        Some(&plays),
        FrameDuration::ZERO,
        Some(&gaps),
        &durations,
    )
    .unwrap();
    let exact = ExactRepeatLayout::compile_with_gap_overrides(
        &order,
        &id("base"),
        Some(&plays),
        ExactRatio::ZERO,
        Some(&gaps),
        &exact_durations,
    )
    .unwrap();
    assert_eq!(
        exact.duration(),
        ExactRatio::integer(ordinary.duration().frames())
    );
    assert_eq!(exact.segment_count(), ordinary.segment_count());
    for tick in 0..=ordinary.duration().frames() * 2 {
        for bias in [InsertionBias::Left, InsertionBias::Right] {
            let position = ratio(i128::from(tick), 2);
            let a = ordinary.locate(position, bias);
            let b = exact.locate(position, bias);
            if tick == ordinary.duration().frames() * 2 && bias == InsertionBias::Right {
                assert_eq!(a.unwrap_err(), b.unwrap_err());
                continue;
            }
            let a = a.unwrap();
            let b = b.unwrap();
            assert_eq!(a.play.iteration, b.play.iteration);
            assert_eq!(a.play.index, b.play.index);
            assert_eq!(a.play.child, b.play.child);
            assert_eq!(a.play.gap_child, b.play.gap_child);
            assert_eq!(ExactRatio::integer(a.play.start), b.play.start);
            assert_eq!(
                ExactRatio::integer(a.play.duration.frames()),
                b.play.duration
            );
            assert_eq!(
                ExactRatio::integer(a.play.gap_after.frames()),
                b.play.gap_after
            );
            assert_eq!(ExactRatio::integer(a.segment_end), b.segment_end);
            assert_eq!(
                (a.position, a.in_gap, a.comparisons),
                (b.position, b.in_gap, b.comparisons)
            );
        }
    }
}

#[test]
fn invalid_extents_and_retired_owners_fail_including_dormant_gaps() {
    let order = IterationOrder::new(revision("one"), 1).unwrap();
    let gaps = overrides(vec![(order.at(0).unwrap(), id("gap"))]);
    for (child, gap) in [
        (ExactRatio::ZERO, ExactRatio::ONE),
        (ratio(-1, 3), ExactRatio::ZERO),
        (ExactRatio::ONE, ratio(-1, 7)),
    ] {
        let durations = BTreeMap::from([(id("base"), child), (id("gap"), gap)]);
        assert!(
            ExactRepeatLayout::compile_with_gap_overrides(
                &order,
                &id("base"),
                None,
                ExactRatio::ZERO,
                Some(&gaps),
                &durations
            )
            .is_err()
        );
    }
    let durations = BTreeMap::from([(id("base"), ExactRatio::ONE)]);
    assert!(
        ExactRepeatLayout::compile(&order, &id("base"), None, ratio(-1, 2), &durations).is_err()
    );
    assert!(
        ExactRepeatLayout::compile_with_gap_overrides(
            &order,
            &id("base"),
            None,
            ExactRatio::ZERO,
            Some(&gaps),
            &durations
        )
        .is_err()
    );
    let retired = overrides(vec![(
        IterationId {
            allocation: revision("retired"),
            ordinal: 0,
        },
        id("base"),
    )]);
    for (plays, gaps) in [(Some(&retired), None), (None, Some(&retired))] {
        assert_eq!(
            ExactRepeatLayout::compile_with_gap_overrides(
                &order,
                &id("base"),
                plays,
                ExactRatio::ZERO,
                gaps,
                &durations
            )
            .unwrap_err()
            .code,
            DocumentErrorCode::InvalidIdentity
        );
    }
}

#[test]
fn huge_dormant_gap_does_not_enter_a_single_play_clock() {
    let order = IterationOrder::new(revision("one"), 1).unwrap();
    let duration = ratio(i128::MAX, 1);
    let durations = BTreeMap::from([(id("base"), duration), (id("gap"), duration)]);
    let gaps = overrides(vec![(order.at(0).unwrap(), id("gap"))]);
    let layout = ExactRepeatLayout::compile_with_gap_overrides(
        &order,
        &id("base"),
        None,
        duration,
        Some(&gaps),
        &durations,
    )
    .unwrap();
    assert_eq!(layout.duration(), duration);
    assert_eq!(
        layout
            .locate(duration, InsertionBias::Left)
            .unwrap()
            .position,
        duration
    );
    assert_eq!(
        layout
            .play(&order.at(0).unwrap())
            .unwrap()
            .unwrap()
            .branch_offset(&id("gap"))
            .unwrap(),
        None
    );
    let grown = order.resized(2, revision("grown")).unwrap();
    assert!(
        ExactRepeatLayout::compile_with_gap_overrides(
            &grown,
            &id("base"),
            None,
            duration,
            Some(&gaps),
            &durations
        )
        .is_err()
    );
}

#[test]
fn checked_fractional_prefix_overflow_returns_failure_instead_of_panicking() {
    let order = IterationOrder::new(revision("two"), 2).unwrap();
    let durations = BTreeMap::from([(id("base"), ratio(i128::MAX, 2))]);
    assert!(
        ExactRepeatLayout::compile(&order, &id("base"), None, ExactRatio::ONE, &durations).is_err()
    );
    // A total can reduce to a small ratio while an interior prefix cannot.
    // Lookup must retain its explicit arithmetic-failure channel.
    let three = IterationOrder::new(revision("three"), 3).unwrap();
    let durations = BTreeMap::from([(id("base"), ratio(i128::MAX - 2, 3))]);
    let layout =
        ExactRepeatLayout::compile(&three, &id("base"), None, ExactRatio::ZERO, &durations)
            .unwrap();
    assert!(layout.play(&three.at(2).unwrap()).is_err());
}
