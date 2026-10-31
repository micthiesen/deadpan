//! Child of ops::tests: these use its actual StageAudio and ReadWork context.
use super::super::super::{MaskSample, SuppressionMask, payload_frames_for, shell_frames};
use super::*;
use deadpan_core::TimeError;
use deadpan_plan::PlanError;
use std::ops::Range;

fn signal(start: i64, end: i64) -> Range<SignalSample> {
    SignalSample(start)..SignalSample(end)
}

fn plain_input() -> Vec<Range<SignalSample>> {
    let mut ranges = Vec::with_capacity(19);
    ranges.extend([signal(8, 11), signal(-2, 1), signal(1, 4), signal(8, 9)]);
    ranges
}

fn make_owned<S: MaskSample>(
    ctx: &mut Context<'_, '_>,
    ranges: &[Range<S>],
    capacity: usize,
) -> SuppressionMask<S> {
    assert!(capacity >= ranges.len());
    let mut builder = begin_ranges::<S, _>(ctx, capacity).unwrap();
    for range in ranges {
        builder.push(range.start..range.end, ctx).unwrap();
    }
    builder.finish(ctx).unwrap()
}

fn assert_vec<S: MaskSample>(mask: &SuppressionMask<S>, expected: &Vec<Range<S>>) {
    assert_eq!(mask.as_slice(), expected.as_slice());
    assert_eq!(mask.capacity(), expected.capacity());
}

fn literal_merge<S: Copy + Ord>(mut ranges: Vec<Range<S>>) -> Vec<Range<S>> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<S>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut().filter(|last| range.start <= last.end) {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

fn map_audio(range: Range<SignalSample>) -> Range<AudioSample> {
    AudioSample(range.start.0 + 7)..AudioSample(range.end.0 + 7)
}

#[test]
fn plain_mutations_keep_literal_vec_values_capacity_and_error_specialization() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(true);
    work.borrow_mut().plan_work = crate::stages::MAX_PLAN_WORK_PER_READ;
    let mut ctx = context(&mut renderer, &work, &cancelled);

    let expected: Vec<_> = plain_input().into_iter().map(map_audio).collect();
    let mapped = SuppressionMask::plain(plain_input())
        .map(map_audio, &mut ctx)
        .unwrap();
    assert_vec(&mapped, &expected);
    assert!(!mapped.is_owned());
    assert_eq!(mapped.unaccounted_capacity(), expected.capacity());

    let expected = plain_input()
        .into_iter()
        .map(|range| Ok::<_, PlanError>(map_audio(range)))
        .collect::<Result<Vec<_>, PlanError>>()
        .unwrap();
    let mapped = SuppressionMask::plain(plain_input())
        .try_map(|range| Ok::<_, PlanError>(map_audio(range)), &mut ctx)
        .unwrap();
    assert_vec(&mapped, &expected);
    let expected = plain_input()
        .into_iter()
        .map(|range| Ok::<_, TimeError>(map_audio(range)))
        .collect::<Result<Vec<_>, TimeError>>()
        .unwrap();
    let mapped = SuppressionMask::plain(plain_input())
        .try_map(|range| Ok::<_, TimeError>(map_audio(range)), &mut ctx)
        .unwrap();
    assert_vec(&mapped, &expected);
    let calls = Cell::new(0);
    let failed = SuppressionMask::plain(plain_input()).try_map(
        |range| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err(PlanError::AudioOnlyContext)
            } else {
                Ok(map_audio(range))
            }
        },
        &mut ctx,
    );
    assert!(matches!(
        failed,
        Err(StageAudioError::Plan(PlanError::AudioOnlyContext))
    ));
    assert_eq!(calls.get(), 2);
    let failed = SuppressionMask::plain(plain_input()).try_map(
        |_| Err::<Range<AudioSample>, _>(TimeError::Overflow),
        &mut ctx,
    );
    assert!(matches!(
        failed,
        Err(StageAudioError::Time(TimeError::Overflow))
    ));

    let filter = |range: Range<SignalSample>| (range.start.0 > 0).then(|| map_audio(range));
    let mut expected = Vec::new();
    expected.extend(plain_input().into_iter().filter_map(filter));
    let mut filtered = SuppressionMask::default();
    filtered
        .extend_filter_map(SuppressionMask::plain(plain_input()), filter, &mut ctx)
        .unwrap();
    assert_vec(&filtered, &expected);

    let mut expected = plain_input();
    expected.extend(plain_input());
    let mut extended = SuppressionMask::plain(plain_input());
    extended
        .extend(SuppressionMask::plain(plain_input()), &mut ctx)
        .unwrap();
    assert_vec(&extended, &expected);

    let mut expected = plain_input();
    let mut expected_source = plain_input();
    expected.append(&mut expected_source);
    let mut appended = SuppressionMask::plain(plain_input());
    let mut source = plain_input();
    appended.append_exhausted(&mut source, &mut ctx).unwrap();
    assert_vec(&appended, &expected);
    assert!(source.is_empty());
    assert_eq!(source.capacity(), expected_source.capacity());

    let identity_filter = |range: Range<SignalSample>| (range.start.0 > 0).then_some(range);
    let mut expected = plain_input();
    expected.extend(plain_input().into_iter().filter_map(identity_filter));
    let mut extended = SuppressionMask::plain(plain_input());
    extended
        .extend_filter_map(
            SuppressionMask::plain(plain_input()),
            identity_filter,
            &mut ctx,
        )
        .unwrap();
    assert_vec(&extended, &expected);
    let mut expected = plain_input();
    expected.extend(plain_input().into_iter().filter(|range| range.start.0 < 3));
    let mut extended = SuppressionMask::plain(plain_input());
    extended
        .extend_iter(
            plain_input().into_iter().filter(|range| range.start.0 < 3),
            &mut ctx,
        )
        .unwrap();
    assert_vec(&extended, &expected);

    let expected = literal_merge(plain_input());
    let merged = SuppressionMask::plain(plain_input()).merge(&ctx).unwrap();
    assert_vec(&merged, &expected);
    assert_eq!(ctx.accepted.get(), [0; PHASES]);
    assert_eq!(
        work.borrow().plan_work,
        crate::stages::MAX_PLAN_WORK_PER_READ
    );
    assert_eq!(ctx.renderer.owned_suppression.live_frames(), 0);
}

#[test]
fn owned_maps_overlap_real_reservations_and_outlive_the_controller() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = context(&mut renderer, &work, &cancelled);
    let input = [signal(-4, -2), signal(0, 3), signal(9, 13)];
    let copied = SuppressionMask::plain(input.to_vec())
        .into_admitted(&mut ctx)
        .unwrap();
    assert!(copied.is_owned());
    assert_eq!(copied.as_slice(), input);
    assert_eq!(copied.capacity(), input.len());
    let prior_work = work.borrow().plan_work;
    let prior_live = ctx.renderer.owned_suppression.live_frames();
    let copied = copied.into_admitted(&mut ctx).unwrap();
    assert_eq!(work.borrow().plan_work, prior_work);
    assert_eq!(ctx.renderer.owned_suppression.live_frames(), prior_live);
    drop(copied);
    let source = make_owned(&mut ctx, &input, 7);
    let observer = ctx.renderer.owned_suppression.observer().unwrap();
    let shell = shell_frames().unwrap();
    let expected_overlap = shell
        + payload_frames_for::<SignalSample>(7).unwrap()
        + payload_frames_for::<AudioSample>(input.len()).unwrap();
    let calls = Cell::new(0);
    let mapped = source
        .map(
            |range| {
                calls.set(calls.get() + 1);
                assert_eq!(observer.live_frames(), expected_overlap);
                map_audio(range)
            },
            &mut ctx,
        )
        .unwrap();
    assert_eq!(calls.get(), input.len());
    assert_eq!(
        mapped.as_slice(),
        input.iter().cloned().map(map_audio).collect::<Vec<_>>()
    );
    assert_eq!(mapped.capacity(), input.len());
    assert_eq!(mapped.unaccounted_capacity(), 0);
    assert_eq!(
        observer.live_frames(),
        shell + payload_frames_for::<AudioSample>(3).unwrap()
    );

    let failed_source = make_owned(&mut ctx, &input, 4);
    let failed_calls = Cell::new(0);
    let failed = failed_source.try_map(
        |range| {
            failed_calls.set(failed_calls.get() + 1);
            assert_eq!(
                observer.live_frames(),
                shell
                    + payload_frames_for::<AudioSample>(3).unwrap()
                    + payload_frames_for::<SignalSample>(4).unwrap()
                    + payload_frames_for::<AudioSample>(3).unwrap()
            );
            if failed_calls.get() == 2 {
                Err(TimeError::Overflow)
            } else {
                Ok(map_audio(range))
            }
        },
        &mut ctx,
    );
    assert!(matches!(
        failed,
        Err(StageAudioError::Time(TimeError::Overflow))
    ));
    assert_eq!(failed_calls.get(), 2);
    assert_eq!(
        observer.live_frames(),
        shell + payload_frames_for::<AudioSample>(3).unwrap()
    );

    let source = make_owned(&mut ctx, &input, 5);
    let calls = Cell::new(0);
    let mut filtered = SuppressionMask::<AudioSample>::default();
    filtered
        .extend_filter_map(
            source,
            |_| {
                calls.set(calls.get() + 1);
                assert_eq!(
                    observer.live_frames(),
                    shell
                        + payload_frames_for::<AudioSample>(3).unwrap()
                        + payload_frames_for::<SignalSample>(5).unwrap()
                        + payload_frames_for::<AudioSample>(3).unwrap()
                );
                None
            },
            &mut ctx,
        )
        .unwrap();
    assert_eq!(calls.get(), input.len());
    assert!(filtered.is_empty());
    assert!(filtered.is_owned());
    assert_eq!(filtered.capacity(), input.len());
    let live = shell + 2 * payload_frames_for::<AudioSample>(3).unwrap();
    assert_eq!(observer.live_frames(), live);
    drop(renderer);
    assert_eq!(mapped[0], AudioSample(3)..AudioSample(5));
    assert_eq!(observer.live_frames(), live);
    drop(mapped);
    drop(filtered);
    assert_eq!(observer.live_frames(), shell);
}

#[test]
fn mapping_callback_stops_before_each_refused_map_and_cleans_pending_storage() {
    let input = [signal(-3, -1), signal(1, 4), signal(8, 12)];
    let callback_offsets = {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let source = make_owned(&mut ctx, &input, 5);
        let before = work.borrow().plan_work;
        let offsets = RefCell::new(Vec::new());
        let mapped = source
            .map(
                |range| {
                    offsets.borrow_mut().push(work.borrow().plan_work - before);
                    map_audio(range)
                },
                &mut ctx,
            )
            .unwrap();
        drop(mapped);
        offsets.into_inner()
    };
    assert_eq!(callback_offsets.len(), input.len());
    for (expected_calls, offset) in callback_offsets.into_iter().enumerate() {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let source = make_owned(&mut ctx, &input, 5);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let allowance = offset - 1;
        let prior = crate::stages::MAX_PLAN_WORK_PER_READ - allowance;
        assert!(prior >= work.borrow().plan_work);
        work.borrow_mut().plan_work = prior;
        let calls = Cell::new(0);
        let result = source.map(
            |range| {
                calls.set(calls.get() + 1);
                map_audio(range)
            },
            &mut ctx,
        );
        assert!(matches!(result, Err(StageAudioError::Limit(_))));
        assert_eq!(calls.get(), expected_calls);
        assert_eq!(work.borrow().plan_work - prior, allowance);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }
}

#[test]
fn replacement_extensions_preserve_target_on_actual_quota_and_work_errors() {
    let replacement_units = {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let mut target = make_owned(&mut ctx, &[signal(0, 1), signal(3, 4)], 2);
        let source = make_owned(&mut ctx, &[signal(6, 7), signal(9, 10)], 2);
        let prior = work.borrow().plan_work;
        target.extend(source, &mut ctx).unwrap();
        work.borrow().plan_work - prior
    };
    assert!(replacement_units > 1);
    for refusal in 0..3 {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let mut target = make_owned(&mut ctx, &[signal(0, 1), signal(3, 4)], 2);
        let source = make_owned(&mut ctx, &[signal(6, 7), signal(9, 10)], 2);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let before_values = target.as_slice().to_vec();
        let before_capacity = target.capacity();
        if refusal == 1 {
            ctx.renderer.limits.maximum_resident_frames =
                u32::try_from(observer.live_frames()).unwrap();
        } else {
            let allowance = if refusal == 2 {
                replacement_units - 1
            } else {
                0
            };
            let prior = crate::stages::MAX_PLAN_WORK_PER_READ - allowance;
            assert!(prior >= work.borrow().plan_work);
            work.borrow_mut().plan_work = prior;
        }
        let before_units = ctx.accepted.get()[index(MaskWorkPhase::AllocateVector)];
        assert!(matches!(
            target.extend(source, &mut ctx),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(target.as_slice(), before_values);
        assert_eq!(target.capacity(), before_capacity);
        assert_eq!(
            ctx.accepted.get()[index(MaskWorkPhase::AllocateVector)],
            before_units + usize::from(refusal == 2)
        );
        assert_eq!(
            observer.live_frames(),
            shell_frames().unwrap() + payload_frames_for::<SignalSample>(before_capacity).unwrap()
        );
        drop(target);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }

    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = context(&mut renderer, &work, &cancelled);
    let mut target = make_owned(&mut ctx, &[signal(0, 1)], 1);
    let source = make_owned(&mut ctx, &[signal(4, 5), signal(8, 9)], 4);
    let observer = ctx.renderer.owned_suppression.observer().unwrap();
    let calls = Cell::new(0);
    let overlap = shell_frames().unwrap()
        + payload_frames_for::<SignalSample>(1).unwrap()
        + payload_frames_for::<SignalSample>(4).unwrap()
        + payload_frames_for::<SignalSample>(3).unwrap();
    target
        .extend_filter_map(
            source,
            |range| {
                calls.set(calls.get() + 1);
                assert_eq!(observer.live_frames(), overlap);
                Some(range)
            },
            &mut ctx,
        )
        .unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(
        target.as_slice(),
        [signal(0, 1), signal(4, 5), signal(8, 9)]
    );
    assert_eq!(target.capacity(), 3);
    assert_eq!(
        observer.live_frames(),
        shell_frames().unwrap() + payload_frames_for::<SignalSample>(3).unwrap()
    );
    drop(target);
    assert_eq!(observer.live_frames(), shell_frames().unwrap());
}

#[test]
fn in_capacity_failure_keeps_accepted_prefix_and_exhausted_clears_only_on_success() {
    for append in [false, true] {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let mut target = make_owned(&mut ctx, &[signal(0, 1)], 8);
        let mut exhausted = vec![signal(3, 4), signal(6, 7), signal(9, 10)];
        let original = exhausted.clone();
        let source_capacity = exhausted.capacity();
        let source = if append {
            None
        } else {
            Some(make_owned(&mut ctx, &original, 3))
        };
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let before_vector = ctx.accepted.get()[index(MaskWorkPhase::AllocateVector)];
        let prior = crate::stages::MAX_PLAN_WORK_PER_READ - 1;
        work.borrow_mut().plan_work = prior;
        let result = if append {
            target.append_exhausted(&mut exhausted, &mut ctx)
        } else {
            target.extend(source.unwrap(), &mut ctx)
        };
        assert!(matches!(result, Err(StageAudioError::Limit(_))));
        assert_eq!(target.as_slice(), [signal(0, 1), signal(3, 4)]);
        assert_eq!(target.capacity(), 8);
        assert_eq!(work.borrow().plan_work - prior, 1);
        assert_eq!(
            ctx.accepted.get()[index(MaskWorkPhase::AllocateVector)],
            before_vector
        );
        assert_eq!(exhausted, original);
        assert_eq!(exhausted.capacity(), source_capacity);
        drop(target);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }

    // A fresh actual read budget verifies the succeeding operation independently.
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = context(&mut renderer, &work, &cancelled);
    let mut target = make_owned(&mut ctx, &[signal(0, 1)], 4);
    let mut exhausted = Vec::with_capacity(17);
    exhausted.extend([signal(3, 4), signal(6, 7)]);
    let capacity = exhausted.capacity();
    target.append_exhausted(&mut exhausted, &mut ctx).unwrap();
    assert!(exhausted.is_empty());
    assert_eq!(exhausted.capacity(), capacity);
    assert_eq!(
        target.as_slice(),
        [signal(0, 1), signal(3, 4), signal(6, 7)]
    );
}

fn merge_small() -> Vec<Range<SignalSample>> {
    vec![
        signal(9, 12),
        signal(0, 3),
        signal(3, 5),
        signal(9, 10),
        signal(7, 7),
        signal(6, 4),
    ]
}

#[test]
fn owned_merge_matches_literal_values_for_real_257_ranges_and_keeps_capacity_charge() {
    let wide: Vec<_> = (0..257)
        .map(|index| {
            let start = i64::from((index * 73) % 257) - 128;
            match index % 5 {
                0 => signal(start, start),
                1 => signal(start, start - 1),
                2 => signal(start / 4, start / 4 + 3),
                _ => signal(start, start + 1),
            }
        })
        .collect();
    assert_eq!(wide.len(), 257);
    let cases = [
        Vec::new(),
        merge_small(),
        vec![signal(2, 8), signal(2, 2), signal(2, 4)],
        wide,
    ];
    for input in cases {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let capacity = input.len() + 17;
        let source = make_owned(&mut ctx, &input, capacity);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let before_live = observer.live_frames();
        let result = source.merge(&ctx).unwrap();
        assert_eq!(result.as_slice(), literal_merge(input));
        assert_eq!(result.capacity(), capacity);
        assert!(result.is_owned());
        assert_eq!(result.unaccounted_capacity(), 0);
        assert_eq!(observer.live_frames(), before_live);
        drop(result);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }
}

#[test]
fn every_observed_small_heap_merge_work_prefix_stops_and_drops_to_shell() {
    let units = {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let source = make_owned(&mut ctx, &merge_small(), 9);
        let prior = work.borrow().plan_work;
        let result = source.merge(&ctx).unwrap();
        drop(result);
        work.borrow().plan_work - prior
    };
    assert!(units > 0);
    for allowance in 0..units {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let source = make_owned(&mut ctx, &merge_small(), 9);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let accepted = ctx.accepted.get();
        let prior = crate::stages::MAX_PLAN_WORK_PER_READ - allowance;
        assert!(prior >= work.borrow().plan_work);
        work.borrow_mut().plan_work = prior;
        assert!(matches!(source.merge(&ctx), Err(StageAudioError::Limit(_))));
        assert_eq!(work.borrow().plan_work - prior, allowance);
        let after = ctx.accepted.get();
        let additional: usize = after
            .iter()
            .zip(accepted)
            .map(|(after, before)| after - before)
            .sum();
        assert_eq!(additional, allowance);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }
}

struct Counted<'a, I> {
    inner: I,
    calls: &'a Cell<usize>,
}
impl<I: Iterator> Iterator for Counted<'_, I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<Self::Item> {
        self.calls.set(self.calls.get() + 1);
        self.inner.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<I: ExactSizeIterator> ExactSizeIterator for Counted<'_, I> {}

struct NoUpper<I>(I);
impl<I: Iterator> Iterator for NoUpper<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

#[test]
fn real_iterator_bounds_and_unknown_growth_remain_funded_and_stop_before_next() {
    for mode in 0..3 {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let mut target = make_owned(&mut ctx, &[signal(-2, -1)], 1);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let calls = Cell::new(0);
        let input: Vec<_> = (0..6).map(|i| signal(i * 3, i * 3 + 1)).collect();
        let (expected, expected_capacity) = match mode {
            0 => {
                let iterator = Counted {
                    inner: input.clone().into_iter(),
                    calls: &calls,
                };
                assert_eq!(iterator.len(), 6);
                target.extend_iter(iterator, &mut ctx).unwrap();
                (input, 7)
            }
            1 => {
                let filter = input
                    .clone()
                    .into_iter()
                    .filter(|range| range.start.0 % 2 == 0);
                assert_eq!(filter.size_hint(), (0, Some(6)));
                target
                    .extend_iter(
                        Counted {
                            inner: filter,
                            calls: &calls,
                        },
                        &mut ctx,
                    )
                    .unwrap();
                (
                    input
                        .into_iter()
                        .filter(|range| range.start.0 % 2 == 0)
                        .collect(),
                    7,
                )
            }
            _ => {
                target
                    .extend_iter(
                        Counted {
                            inner: NoUpper(input.clone().into_iter()),
                            calls: &calls,
                        },
                        &mut ctx,
                    )
                    .unwrap();
                (input, 8)
            }
        };
        assert_eq!(&target.as_slice()[1..], expected);
        assert_eq!(calls.get(), expected.len() + 1);
        assert_eq!(target.capacity(), expected_capacity);
        assert_eq!(
            observer.live_frames(),
            shell_frames().unwrap()
                + payload_frames_for::<SignalSample>(expected_capacity).unwrap()
        );
        drop(target);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }

    for unknown in [false, true] {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let mut target = make_owned(&mut ctx, &[signal(-2, -1)], 1);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let calls = Cell::new(0);
        let prior = crate::stages::MAX_PLAN_WORK_PER_READ - usize::from(unknown);
        work.borrow_mut().plan_work = prior;
        let input = vec![signal(0, 1), signal(3, 4)];
        let result = if unknown {
            target.extend_iter(
                Counted {
                    inner: NoUpper(input.into_iter()),
                    calls: &calls,
                },
                &mut ctx,
            )
        } else {
            target.extend_iter(
                Counted {
                    inner: input.into_iter(),
                    calls: &calls,
                },
                &mut ctx,
            )
        };
        assert!(matches!(result, Err(StageAudioError::Limit(_))));
        assert_eq!(calls.get(), usize::from(unknown));
        assert_eq!(work.borrow().plan_work - prior, usize::from(unknown));
        assert_eq!(target.as_slice(), [signal(-2, -1)]);
        assert_eq!(target.capacity(), 1);
        drop(target);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }
}
