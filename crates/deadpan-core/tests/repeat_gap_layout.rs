use std::collections::BTreeMap;

use deadpan_core::{
    DocumentErrorCode, ExactRatio, FrameDuration, InsertionBias, IterationId, IterationOrder,
    NodeId, PlayOverride, PlayOverrides, RepeatLayout, RepeatPlay, RevisionId,
};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn override_for(iteration: IterationId, root: &str) -> PlayOverrides {
    PlayOverrides::try_from(vec![PlayOverride {
        iteration,
        root: id(root),
    }])
    .unwrap()
}

#[test]
fn sparse_play_and_gap_branches_are_independent_across_reorder() {
    let initial = IterationOrder::new(revision("original"), 4).unwrap();
    let selected = initial.at(0).unwrap();
    let other = initial.at(3).unwrap();
    let order = initial.moved(0, 2, 2).unwrap();
    let play = override_for(selected.clone(), "long-play");
    let gaps = PlayOverrides::try_from(vec![
        PlayOverride {
            iteration: selected.clone(),
            root: id("long-gap"),
        },
        PlayOverride {
            iteration: other,
            root: id("empty-gap"),
        },
    ])
    .unwrap();
    let durations = BTreeMap::from([
        (id("base"), frames(2)),
        (id("long-play"), frames(5)),
        (id("long-gap"), frames(3)),
        (id("empty-gap"), frames(0)),
    ]);
    let layout = RepeatLayout::compile_with_gap_overrides(
        &order,
        &id("base"),
        Some(&play),
        frames(1),
        Some(&gaps),
        &durations,
    )
    .unwrap();
    assert_eq!(layout.duration(), frames(15));
    assert_eq!(layout.segment_count(), 4);
    assert_eq!(layout.play(&selected).unwrap().index, 2);
    assert_eq!(layout.play(&selected).unwrap().gap_after, frames(3));
    assert_eq!(
        layout.play(&selected).unwrap().gap_child,
        Some(id("long-gap"))
    );
    assert_eq!(
        layout
            .play(&selected)
            .unwrap()
            .branch_offset(&id("long-play")),
        Some(5)
    );
    assert_eq!(
        layout
            .play(&selected)
            .unwrap()
            .branch_offset(&id("long-gap")),
        Some(10)
    );
    assert_eq!(
        layout.play(&initial.at(3).unwrap()).unwrap().gap_after,
        frames(0)
    );
    assert_eq!(
        layout.play(&initial.at(3).unwrap()).unwrap().gap_child,
        Some(id("empty-gap"))
    );
    assert!(
        layout
            .locate(ExactRatio::integer(10), InsertionBias::Right)
            .unwrap()
            .in_gap
    );
}

#[test]
fn final_override_is_validated_but_dormant_and_revives_after_growth() {
    let order = IterationOrder::new(revision("first"), 1).unwrap();
    let last = order.at(0).unwrap();
    let gaps = override_for(last.clone(), "custom-gap");
    let durations = BTreeMap::from([(id("base"), frames(2)), (id("custom-gap"), frames(7))]);
    let compile = |order: &IterationOrder| {
        RepeatLayout::compile_with_gap_overrides(
            order,
            &id("base"),
            None,
            frames(0),
            Some(&gaps),
            &durations,
        )
        .unwrap()
    };
    let first = compile(&order);
    assert_eq!(first.duration(), frames(2));
    assert_eq!(first.play(&last).unwrap().gap_after, frames(0));
    assert_eq!(first.play(&last).unwrap().gap_child, Some(id("custom-gap")));
    assert_eq!(
        first.play(&last).unwrap().branch_offset(&id("custom-gap")),
        None
    );
    let grown = compile(&order.resized(2, revision("growth")).unwrap());
    assert_eq!(grown.duration(), frames(11));
    assert_eq!(grown.play(&last).unwrap().gap_after, frames(7));
    assert_eq!(
        grown.play(&last).unwrap().branch_offset(&id("custom-gap")),
        Some(2)
    );
    assert!(
        grown
            .locate(ExactRatio::integer(2), InsertionBias::Right)
            .unwrap()
            .in_gap
    );
}

#[test]
fn empty_gap_override_suppresses_default_without_affecting_other_gaps() {
    let order = IterationOrder::new(revision("plays"), 3).unwrap();
    let gaps = override_for(order.at(0).unwrap(), "empty");
    let durations = BTreeMap::from([(id("base"), frames(2)), (id("empty"), frames(0))]);
    let layout = RepeatLayout::compile_with_gap_overrides(
        &order,
        &id("base"),
        None,
        frames(4),
        Some(&gaps),
        &durations,
    )
    .unwrap();
    assert_eq!(layout.duration(), frames(10));
    assert_eq!(
        layout.play(&order.at(0).unwrap()).unwrap().gap_after,
        frames(0)
    );
    assert_eq!(
        layout
            .play(&order.at(0).unwrap())
            .unwrap()
            .branch_offset(&id("empty")),
        None
    );
    assert_eq!(
        layout.play(&order.at(1).unwrap()).unwrap().gap_after,
        frames(4)
    );
    assert_eq!(
        layout
            .locate(ExactRatio::integer(2), InsertionBias::Right)
            .unwrap()
            .play
            .index,
        1
    );
    assert!(
        !layout
            .locate(ExactRatio::integer(2), InsertionBias::Right)
            .unwrap()
            .in_gap
    );
    assert!(
        !layout
            .locate(ExactRatio::integer(2), InsertionBias::Left)
            .unwrap()
            .in_gap
    );
    assert!(
        layout
            .locate(ExactRatio::integer(4), InsertionBias::Right)
            .unwrap()
            .in_gap
    );
    assert!(
        !layout
            .locate(ExactRatio::integer(4), InsertionBias::Left)
            .unwrap()
            .in_gap
    );
    assert_eq!(
        layout
            .locate(ExactRatio::integer(8), InsertionBias::Right)
            .unwrap()
            .play
            .index,
        2
    );
    assert!(
        layout
            .locate(ExactRatio::integer(8), InsertionBias::Left)
            .unwrap()
            .in_gap
    );
}

#[test]
fn retired_or_missing_gap_roots_and_overflow_are_rejected() {
    let original = IterationOrder::new(revision("original"), 2).unwrap();
    let durations = BTreeMap::from([(id("base"), frames(1)), (id("gap"), frames(2))]);
    let retired = override_for(original.at(1).unwrap(), "gap");
    let reduced = original.resized(1, revision("shrink")).unwrap();
    assert_eq!(
        RepeatLayout::compile_with_gap_overrides(
            &reduced,
            &id("base"),
            None,
            frames(0),
            Some(&retired),
            &durations,
        )
        .unwrap_err()
        .code,
        DocumentErrorCode::InvalidIdentity
    );
    let missing = override_for(original.at(0).unwrap(), "missing");
    assert_eq!(
        RepeatLayout::compile_with_gap_overrides(
            &original,
            &id("base"),
            None,
            frames(0),
            Some(&missing),
            &durations,
        )
        .unwrap_err()
        .code,
        DocumentErrorCode::InvalidIdentity
    );
    let huge = BTreeMap::from([(id("base"), frames(i64::MAX))]);
    assert_eq!(
        RepeatLayout::compile_with_gap_overrides(
            &original,
            &id("base"),
            None,
            frames(0),
            None,
            &huge,
        )
        .unwrap_err()
        .code,
        DocumentErrorCode::TimingOverflow
    );
}

#[test]
fn billion_plays_keep_compact_gap_lookup() {
    let order = IterationOrder::new(revision("billion"), 1_000_000_000).unwrap();
    let target = order.at(500_000_000).unwrap();
    let gaps = override_for(target.clone(), "special");
    let durations = BTreeMap::from([(id("base"), frames(1)), (id("special"), frames(3))]);
    let layout = RepeatLayout::compile_with_gap_overrides(
        &order,
        &id("base"),
        None,
        frames(1),
        Some(&gaps),
        &durations,
    )
    .unwrap();
    assert_eq!(layout.segment_count(), 3);
    assert_eq!(layout.duration(), frames(2_000_000_001));
    let gap = layout
        .locate_bounded(ExactRatio::integer(1_000_000_002), InsertionBias::Right, 2)
        .unwrap();
    assert_eq!(gap.play.iteration, target);
    assert!(gap.in_gap);
    assert!(gap.comparisons <= 2);
}

#[test]
fn expanded_small_reference_matches_every_half_frame_and_bias() {
    for plays in 1..=5 {
        let original = IterationOrder::new(revision("base-allocation"), plays).unwrap();
        for moved in 0..plays {
            let order = original.moved(moved, moved + 1, 0).unwrap();
            for play_override_at in 0..plays {
                for gap_override_at in 0..plays {
                    for custom_gap in 0..=3 {
                        let play_id = original.at(play_override_at).unwrap();
                        let gap_id = original.at(gap_override_at).unwrap();
                        let play = override_for(play_id.clone(), "custom-play");
                        let gaps = override_for(gap_id.clone(), "custom-gap");
                        let durations = BTreeMap::from([
                            (id("base"), frames(2)),
                            (id("custom-play"), frames(3)),
                            (id("custom-gap"), frames(custom_gap)),
                        ]);
                        let layout = RepeatLayout::compile_with_gap_overrides(
                            &order,
                            &id("base"),
                            Some(&play),
                            frames(1),
                            Some(&gaps),
                            &durations,
                        )
                        .unwrap();
                        let mut expected = Vec::new();
                        let mut cursor = 0;
                        for index in 0..plays {
                            let iteration = order.at(index).unwrap();
                            let is_last = index + 1 == plays;
                            let child_duration = if iteration == play_id { 3 } else { 2 };
                            let gap_duration = if is_last {
                                0
                            } else if iteration == gap_id {
                                custom_gap
                            } else {
                                1
                            };
                            expected.push(RepeatPlay {
                                index,
                                iteration: iteration.clone(),
                                child: if iteration == play_id {
                                    id("custom-play")
                                } else {
                                    id("base")
                                },
                                start: cursor,
                                duration: frames(child_duration),
                                gap_after: frames(gap_duration),
                                gap_child: (iteration == gap_id).then(|| id("custom-gap")),
                            });
                            cursor += child_duration + gap_duration;
                        }
                        assert_eq!(layout.duration(), frames(cursor));
                        for play in &expected {
                            assert_eq!(layout.play(&play.iteration), Some(play.clone()));
                        }
                        for doubled in 0..=cursor * 2 {
                            let position = ExactRatio::new(doubled.into(), 2).unwrap();
                            for bias in [InsertionBias::Left, InsertionBias::Right] {
                                if doubled == cursor * 2 && bias == InsertionBias::Right {
                                    assert!(layout.locate(position, bias).is_err());
                                    continue;
                                }
                                let actual = layout.locate(position, bias).unwrap();
                                let (reference, in_gap) = expected
                                    .iter()
                                    .find_map(|play| {
                                        let child_end = play.start + play.duration.frames();
                                        let gap_end = child_end + play.gap_after.frames();
                                        let before_child_end =
                                            position.compare_integer(child_end).is_lt()
                                                || (position.compare_integer(child_end).is_eq()
                                                    && bias == InsertionBias::Left);
                                        if before_child_end {
                                            return Some((play, false));
                                        }
                                        let before_gap_end =
                                            position.compare_integer(gap_end).is_lt()
                                                || (position.compare_integer(gap_end).is_eq()
                                                    && bias == InsertionBias::Left
                                                    && play.gap_after != frames(0));
                                        before_gap_end.then_some((play, true))
                                    })
                                    .expect("reference position is within Repeat");
                                assert_eq!(&actual.play, reference);
                                assert_eq!(actual.in_gap, in_gap);
                            }
                        }
                    }
                }
            }
        }
    }
}
