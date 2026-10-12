use super::*;
use deadpan_plan::AudioOccurrenceWindow;
use std::ops::Range;

fn identity_chain(length: ExactRatio, entries: usize) -> SoundRoute {
    let mut route = SoundRoute::identity(length).unwrap();
    for _ in 1..entries {
        route = route
            .ripple(
                SoundRippleMap::new(
                    length,
                    0,
                    vec![SoundRippleNode::Keep {
                        range: ExactFrameRange::new(ExactRatio::ZERO, length).unwrap(),
                    }],
                )
                .unwrap(),
            )
            .unwrap();
    }
    route
}

fn window(start: ExactRatio, end: ExactRatio, anchor: ExactRatio) -> AudioOccurrenceWindow {
    AudioOccurrenceWindow {
        selection: start..end,
        anchor,
    }
}

// Independent dense physical labels, rounded directly from exact arithmetic.
// Every step copies only samples that survived the previous step. There is no
// route, frame lookup, final-offset shortcut or DSP implementation in this oracle.
fn oracle(
    origins: &[ExactRatio],
    step: ExactRatio,
    windows: &[AudioOccurrenceWindow],
) -> (Range<i64>, Vec<Option<ExactRatio>>) {
    let boundary = |origin: ExactRatio, frame: ExactRatio| {
        i64::try_from(
            frame
                .checked_sub(origin)
                .unwrap()
                .checked_div(step)
                .unwrap()
                .round_even()
                .unwrap(),
        )
        .unwrap()
    };
    let range = |index: usize| {
        boundary(origins[index], windows[index].selection.start)
            ..boundary(origins[index], windows[index].selection.end)
    };
    let mut allocation = range(0);
    let mut labels: Vec<_> = allocation.clone().map(Some).collect();
    for index in 1..origins.len() {
        let next = range(index);
        let delta = boundary(origins[index - 1], windows[index].anchor)
            - boundary(origins[index], windows[index].anchor);
        labels = next
            .clone()
            .map(|sample| {
                let old = sample + delta;
                if allocation.contains(&old) {
                    labels[(old - allocation.start) as usize]
                } else {
                    None
                }
            })
            .collect();
        allocation = next;
    }
    (
        allocation,
        labels
            .into_iter()
            .map(|label| {
                label.map(|sample| {
                    origins[0]
                        .checked_add(ExactRatio::integer(sample).checked_mul(step).unwrap())
                        .unwrap()
                })
            })
            .collect(),
    )
}

#[test]
fn cropped_occurrence_moves_keep_each_rounded_window_and_complete_recipe() {
    let length = q(6, 1);
    let windows = [
        window(q(0, 1), length, q(1, 1)),
        window(q(0, 1), length, q(1, 1)),
        window(q(4, 3), q(16, 3), q(4, 3)),
        window(q(4, 3), q(16, 3), q(4, 3)),
        window(q(5, 2), q(9, 2), q(5, 2)),
        // A later expansion must not recover previously clipped PCM.
        window(q(0, 1), length, q(0, 1)),
    ];
    for step in [q(5, 8008), q(2, 3), q(1, 2)] {
        for phase in [-3, -1, 0, 1, 3] {
            let phase = q(phase, 7);
            let origins: Vec<_> = [q(1, 1), q(-1, 1), q(-7, 3), q(2, 1), q(-9, 2), q(1, 1)]
                .map(|origin| origin.checked_add(phase).unwrap())
                .to_vec();
            let grids: Vec<_> = origins
                .iter()
                .map(|origin| root_grid(*origin, step))
                .collect();
            let sampled = AudioSoundRoute::<AudioSample>::new(
                identity_chain(length, windows.len()),
                grids.clone(),
            )
            .unwrap()
            .with_occurrence_windows(windows.to_vec())
            .unwrap();
            let (allocation, expected) = oracle(&origins, step, &windows);
            assert_eq!(
                sampled.samples(),
                AudioSample(allocation.start)..AudioSample(allocation.end)
            );
            assert_eq!(sampled.recipe_grid(), grids[0]);
            assert_eq!(
                sampled.recipe_samples(),
                grids[0].boundary(q(0, 1)).unwrap()..grids[0].boundary(length).unwrap()
            );
            assert_eq!(
                values(
                    sampled
                        .query(sampled.samples(), Default::default())
                        .unwrap()
                ),
                expected
            );
            assert!(expected.iter().any(Option::is_some));
            assert!(expected.iter().any(Option::is_none));
            // Fresh shuffled short queries must have the same retained labels.
            for offset in [expected.len() / 2, 0, expected.len().saturating_sub(97), 1] {
                let end = (offset + 97).min(expected.len());
                let samples = AudioSample(allocation.start + offset as i64)
                    ..AudioSample(allocation.start + end as i64);
                assert_eq!(
                    values(sampled.query(samples, Default::default()).unwrap()),
                    expected[offset..end]
                );
            }
        }
    }
}

#[test]
fn disjoint_occurrence_windows_do_not_revive_when_expanded() {
    let length = q(4, 1);
    let grids = vec![ntsc(), root_grid(q(-1, 1), q(5, 8008)), ntsc(), ntsc()];
    let windows = vec![
        window(q(0, 1), length, q(0, 1)),
        window(q(0, 1), q(1, 1), q(0, 1)),
        window(q(3, 1), length, q(3, 1)),
        window(q(0, 1), length, q(0, 1)),
    ];
    let route = AudioSoundRoute::<AudioSample>::new(identity_chain(length, 4), grids)
        .unwrap()
        .with_occurrence_windows(windows)
        .unwrap();
    assert!(
        values(route.query(route.samples(), Default::default()).unwrap())
            .iter()
            .all(Option::is_none)
    );
}

#[test]
fn sampleless_occurrence_window_and_history_budgets_stay_explicit() {
    let length = q(4, 1);
    let windows = vec![
        window(q(0, 1), length, q(0, 1)),
        // Positive exact support can contain no physical sample.
        window(q(1, 10), q(1, 5), q(1, 10)),
        window(q(0, 1), length, q(0, 1)),
    ];
    let grid = root_grid(ExactRatio::ZERO, ExactRatio::ONE);
    let route = AudioSoundRoute::<AudioSample>::new(identity_chain(length, 3), vec![grid; 3])
        .unwrap()
        .with_occurrence_windows(windows)
        .unwrap();
    let query = route.query(route.samples(), Default::default()).unwrap();
    let required = query.stats.work;
    assert_eq!(values(query), vec![None; 4]);
    assert!(matches!(
        route.query(
            route.samples(),
            AudioQueryLimits {
                maximum_work: required - 1,
                maximum_spans: 16,
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(
        route
            .query(AudioSample(-1)..AudioSample(1), Default::default())
            .is_err()
    );
    let empty = route
        .query(AudioSample(2)..AudioSample(2), Default::default())
        .unwrap();
    assert!(empty.spans.is_empty());
    assert_eq!(empty.stats.work, 0);
}

#[test]
fn occurrence_windows_refuse_incomplete_history_and_nonidentity_routes() {
    let full = window(q(0, 1), q(4, 1), q(0, 1));
    let route =
        AudioSoundRoute::<AudioSample>::new(identity_chain(q(4, 1), 2), vec![ntsc(); 2]).unwrap();
    for windows in [
        vec![],
        vec![full.clone()],
        vec![window(q(1, 1), q(4, 1), q(1, 1)), full.clone()],
        vec![full.clone(), window(q(-1, 1), q(2, 1), q(0, 1))],
        vec![full.clone(), window(q(1, 1), q(5, 1), q(1, 1))],
        vec![full.clone(), window(q(2, 1), q(2, 1), q(2, 1))],
        vec![full.clone(), window(q(3, 1), q(2, 1), q(2, 1))],
    ] {
        assert!(route.clone().with_occurrence_windows(windows).is_err());
    }
    let changed = SoundRoute::identity(q(3, 1))
        .unwrap()
        .ripple(insert(3, 1, 1))
        .unwrap();
    let changed = AudioSoundRoute::<AudioSample>::new(changed, vec![ntsc(); 2]).unwrap();
    assert!(
        changed
            .with_occurrence_windows(vec![window(q(0, 1), q(3, 1), q(0, 1)), full])
            .is_err()
    );
    assert!(
        route
            .with_occurrence_windows(vec![
                window(q(0, 1), q(4, 1), q(i128::MAX, 1)),
                window(q(0, 1), q(4, 1), q(0, 1)),
            ])
            .is_err()
    );
}
