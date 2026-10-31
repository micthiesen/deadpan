//! Child of ops::tests; all admission uses its real renderer and read budget.
use super::super::super::{MaskSample, SuppressionMask, payload_frames_for, shell_frames};
use super::*;
use deadpan_core::TimeError;
use std::ops::Range;

fn signal(start: i64, end: i64) -> Range<SignalSample> {
    SignalSample(start)..SignalSample(end)
}

fn make_owned<S: MaskSample>(
    ctx: &mut Context<'_, '_>,
    input: &[Range<S>],
    capacity: usize,
) -> SuppressionMask<S> {
    assert!(capacity >= input.len());
    let mut builder = begin_ranges::<S, _>(ctx, capacity).unwrap();
    for range in input {
        builder.push(range.start..range.end, ctx).unwrap();
    }
    builder.finish(ctx).unwrap()
}

fn plain_input() -> Vec<Range<SignalSample>> {
    let mut ranges = Vec::with_capacity(11);
    ranges.extend([signal(-3, 1), signal(1, 2), signal(4, 6), signal(6, 10)]);
    ranges
}

fn clipped_literal<S: Copy + Ord>(
    output: &mut Vec<Range<S>>,
    input: &[Range<S>],
    requested: &Range<S>,
) {
    for range in input {
        let clipped = range.start.max(requested.start)..range.end.min(requested.end);
        if clipped.start >= clipped.end {
            continue;
        }
        if let Some(last) = output.last_mut()
            && last.end >= clipped.start
        {
            last.end = last.end.max(clipped.end);
        } else {
            output.push(clipped);
        }
    }
}

fn intersect_literal<S: Copy + Ord>(left: &[Range<S>], right: &[Range<S>]) -> Vec<Range<S>> {
    let mut result = Vec::new();
    let (mut a, mut b) = (0, 0);
    while a < left.len() && b < right.len() {
        let range = left[a].start.max(right[b].start)..left[a].end.min(right[b].end);
        if range.start < range.end {
            result.push(range);
        }
        if left[a].end < right[b].end {
            a += 1;
        } else {
            b += 1;
        }
    }
    result
}

fn assert_plain_vec<S: MaskSample>(actual: &SuppressionMask<S>, expected: &Vec<Range<S>>) {
    assert_eq!(actual.as_slice(), expected.as_slice());
    assert_eq!(actual.capacity(), expected.capacity());
    assert_eq!(actual.unaccounted_capacity(), expected.capacity());
    assert!(!actual.is_owned());
}

#[test]
fn plain_projection_helpers_keep_literal_vec_behavior_without_control_or_account_work() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    work.borrow_mut().plan_work = crate::stages::MAX_PLAN_WORK_PER_READ;
    let cancelled = AtomicBool::new(true);
    let mut ctx = context(&mut renderer, &work, &cancelled);

    let mut expected = plain_input();
    expected.push(signal(15, 17));
    let mut actual = SuppressionMask::plain(plain_input());
    actual.push(signal(15, 17), &mut ctx).unwrap();
    assert_plain_vec(&actual, &expected);

    let checked = |range: Range<SignalSample>| {
        Ok::<_, TimeError>(
            AudioSample(range.start.0.checked_add(2).ok_or(TimeError::Overflow)?)
                ..AudioSample(range.end.0.checked_add(2).ok_or(TimeError::Overflow)?),
        )
    };
    let mut expected = Vec::with_capacity(3);
    expected.push(AudioSample(-9)..AudioSample(-8));
    for range in plain_input() {
        expected.push(checked(range).unwrap());
    }
    let mut initial = Vec::with_capacity(3);
    initial.push(AudioSample(-9)..AudioSample(-8));
    let mut actual = SuppressionMask::plain(initial);
    actual
        .extend_try_map(SuppressionMask::plain(plain_input()), checked, &mut ctx)
        .unwrap();
    assert_plain_vec(&actual, &expected);

    let mut expected = plain_input();
    let mut actual = SuppressionMask::plain(plain_input());
    let incoming = [signal(20, 21), signal(i64::MAX, i64::MAX)];
    let mut expected_error = None;
    for range in incoming.iter().cloned() {
        match range.start.0.checked_add(1).zip(range.end.0.checked_add(1)) {
            Some((start, end)) => expected.push(signal(start, end)),
            None => {
                expected_error = Some(TimeError::Overflow);
                break;
            }
        }
    }
    let calls = Cell::new(0);
    let result = actual.extend_try_map(
        SuppressionMask::plain(incoming.to_vec()),
        |range| {
            calls.set(calls.get() + 1);
            Ok::<_, TimeError>(signal(
                range.start.0.checked_add(1).ok_or(TimeError::Overflow)?,
                range.end.0.checked_add(1).ok_or(TimeError::Overflow)?,
            ))
        },
        &mut ctx,
    );
    assert!(matches!(expected_error, Some(TimeError::Overflow)));
    assert!(matches!(
        result,
        Err(StageAudioError::Time(TimeError::Overflow))
    ));
    assert_eq!(calls.get(), 2);
    assert_plain_vec(&actual, &expected);

    let source = SuppressionMask::plain(plain_input());
    let filter = |range: &Range<SignalSample>| {
        (range.start.0 >= 1).then_some(signal(range.start.0 + 30, range.end.0 + 30))
    };
    let mut expected = plain_input();
    expected.extend(source.iter().filter_map(filter));
    let mut actual = SuppressionMask::plain(plain_input());
    actual
        .extend_borrowed_filter_map(&source, filter, &mut ctx)
        .unwrap();
    assert_plain_vec(&actual, &expected);
    assert_eq!(source.as_slice(), plain_input().as_slice());

    let requested = signal(0, 8);
    let mut expected = Vec::with_capacity(11);
    expected.push(signal(-2, 0));
    clipped_literal(&mut expected, source.as_slice(), &requested);
    let mut initial = Vec::with_capacity(11);
    initial.push(signal(-2, 0));
    let mut actual = SuppressionMask::plain(initial);
    actual
        .append_clipped(&source, &requested, &mut ctx)
        .unwrap();
    assert_plain_vec(&actual, &expected);

    let right = SuppressionMask::plain(vec![signal(-1, 1), signal(2, 5), signal(7, 12)]);
    let expected = intersect_literal(source.as_slice(), right.as_slice());
    let actual = SuppressionMask::intersect(&source, &right, &mut ctx).unwrap();
    assert_plain_vec(&actual, &expected);
    #[expect(
        clippy::vec_init_then_push,
        reason = "Pin literal Vec push capacity growth"
    )]
    {
        let mut expected = Vec::new();
        expected.push(signal(20, 21));
        let mut actual = SuppressionMask::default();
        actual.push(signal(20, 21), &mut ctx).unwrap();
        assert_plain_vec(&actual, &expected);
    }
    assert_eq!(ctx.accepted.get(), [0; PHASES]);
    assert_eq!(
        work.borrow().plan_work,
        crate::stages::MAX_PLAN_WORK_PER_READ
    );
    assert_eq!(ctx.renderer.owned_suppression.live_frames(), 0);
}

#[test]
fn borrowed_arc_mask_and_new_projection_stay_charged_after_controller_drop() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = context(&mut renderer, &work, &cancelled);
    let source_values = [signal(-4, -2), signal(2, 5), signal(8, 11)];
    let source = Arc::new(make_owned(&mut ctx, &source_values, 7));
    let cache = Arc::clone(&source);
    let mut output = make_owned(&mut ctx, &[AudioSample(-10)..AudioSample(-9)], 1);
    let observer = ctx.renderer.owned_suppression.observer().unwrap();
    let shell = shell_frames().unwrap();
    let overlap = shell
        + payload_frames_for::<SignalSample>(7).unwrap()
        + payload_frames_for::<AudioSample>(1).unwrap()
        + payload_frames_for::<AudioSample>(4).unwrap();
    let calls = Cell::new(0);
    output
        .extend_borrowed_filter_map(
            cache.as_ref(),
            |range| {
                calls.set(calls.get() + 1);
                assert_eq!(observer.live_frames(), overlap);
                (range.start.0 >= 0)
                    .then_some(AudioSample(range.start.0 + 1)..AudioSample(range.end.0 + 1))
            },
            &mut ctx,
        )
        .unwrap();
    assert_eq!(calls.get(), source_values.len());
    assert_eq!(
        output.as_slice(),
        [
            AudioSample(-10)..AudioSample(-9),
            AudioSample(3)..AudioSample(6),
            AudioSample(9)..AudioSample(12),
        ]
    );
    assert_eq!(output.capacity(), 4);
    assert_eq!(source.as_slice(), source_values);
    assert_eq!(Arc::strong_count(&source), 2);
    let live = shell
        + payload_frames_for::<SignalSample>(7).unwrap()
        + payload_frames_for::<AudioSample>(4).unwrap();
    assert_eq!(observer.live_frames(), live);
    drop(renderer);
    drop(source);
    assert_eq!(cache.as_slice(), source_values);
    assert_eq!(observer.live_frames(), live);
    drop(cache);
    assert_eq!(
        observer.live_frames(),
        shell + payload_frames_for::<AudioSample>(4).unwrap()
    );
    drop(output);
    assert_eq!(observer.live_frames(), shell);
}

#[test]
fn checked_owned_label_map_failure_preserves_old_target_and_releases_sources() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = context(&mut renderer, &work, &cancelled);
    let old = [signal(-7, -6)];
    let mut target = make_owned(&mut ctx, &old, 1);
    let input = [
        AudioSample(2)..AudioSample(4),
        AudioSample(i64::MAX)..AudioSample(i64::MAX),
    ];
    let source = make_owned(&mut ctx, &input, 4);
    let observer = ctx.renderer.owned_suppression.observer().unwrap();
    let shell = shell_frames().unwrap();
    let overlap = shell
        + payload_frames_for::<SignalSample>(1).unwrap()
        + payload_frames_for::<AudioSample>(4).unwrap()
        + payload_frames_for::<SignalSample>(3).unwrap();
    let calls = Cell::new(0);
    let failed = target.extend_try_map(
        source,
        |range| {
            calls.set(calls.get() + 1);
            assert_eq!(observer.live_frames(), overlap);
            Ok::<_, TimeError>(signal(
                range.start.0.checked_add(1).ok_or(TimeError::Overflow)?,
                range.end.0.checked_add(1).ok_or(TimeError::Overflow)?,
            ))
        },
        &mut ctx,
    );
    assert!(matches!(
        failed,
        Err(StageAudioError::Time(TimeError::Overflow))
    ));
    assert_eq!(calls.get(), 2);
    assert_eq!(target.as_slice(), old);
    assert_eq!(target.capacity(), 1);
    assert_eq!(
        observer.live_frames(),
        shell + payload_frames_for::<SignalSample>(1).unwrap()
    );
    let before_live = observer.live_frames();
    target.push(signal(20, 21), &mut ctx).unwrap();
    assert_eq!(target.as_slice(), [signal(-7, -6), signal(20, 21)]);
    assert_eq!(target.capacity(), 2);
    assert_eq!(
        observer.live_frames(),
        shell + payload_frames_for::<SignalSample>(2).unwrap()
    );
    assert!(observer.live_frames() > before_live);
    let mut spare = make_owned(&mut ctx, &[signal(30, 31)], 5);
    let before_live = observer.live_frames();
    spare.push(signal(33, 34), &mut ctx).unwrap();
    assert_eq!(spare.as_slice(), [signal(30, 31), signal(33, 34)]);
    assert_eq!(spare.capacity(), 5);
    assert_eq!(observer.live_frames(), before_live);
    drop(spare);
    drop(target);
    assert_eq!(observer.live_frames(), shell);
}

#[test]
fn owned_clipped_append_matches_literal_clipping_and_adjacent_merge_without_shrinking() {
    for requested in [signal(0, 8), signal(12, 15), signal(-8, 20)] {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let input = plain_input();
        let source = make_owned(&mut ctx, &input, 7);
        let mut target = make_owned(&mut ctx, &[signal(-8, -5)], 1);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let mut expected = vec![signal(-8, -5)];
        clipped_literal(&mut expected, &input, &requested);
        target
            .append_clipped(&source, &requested, &mut ctx)
            .unwrap();
        assert_eq!(target.as_slice(), expected.as_slice());
        assert_eq!(target.capacity(), 1 + input.len());
        assert_eq!(source.as_slice(), input.as_slice());
        assert_eq!(
            observer.live_frames(),
            shell_frames().unwrap()
                + payload_frames_for::<SignalSample>(7).unwrap()
                + payload_frames_for::<SignalSample>(1 + input.len()).unwrap()
        );
        drop(source);
        drop(target);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }

    // The spare-capacity route keeps the full existing allocation after merge.
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = context(&mut renderer, &work, &cancelled);
    let source = make_owned(&mut ctx, &plain_input(), 6);
    let mut target = make_owned(&mut ctx, &[signal(-2, 0)], 19);
    let mut expected = vec![signal(-2, 0)];
    clipped_literal(&mut expected, source.as_slice(), &signal(0, 8));
    let observer = ctx.renderer.owned_suppression.observer().unwrap();
    let before = observer.live_frames();
    target
        .append_clipped(&source, &signal(0, 8), &mut ctx)
        .unwrap();
    assert_eq!(target.as_slice(), expected.as_slice());
    assert_eq!(target.capacity(), 19);
    assert_eq!(observer.live_frames(), before);
    drop(source);
    drop(target);
    assert_eq!(observer.live_frames(), shell_frames().unwrap());
}

fn intersection_inputs() -> (Vec<Range<SignalSample>>, Vec<Range<SignalSample>>) {
    (
        vec![signal(-4, 0), signal(2, 6), signal(9, 13)],
        vec![signal(-2, 3), signal(5, 10), signal(12, 15)],
    )
}

#[test]
fn owned_intersections_preserve_all_values_sources_and_every_small_work_prefix() {
    let cases = [
        intersection_inputs(),
        (Vec::new(), vec![signal(0, 2)]),
        (vec![signal(0, 1)], vec![signal(2, 3)]),
        (
            vec![signal(-8, 20)],
            vec![signal(-4, 0), signal(2, 6), signal(9, 13)],
        ),
        (
            vec![signal(2, 6), signal(9, 13)],
            vec![signal(2, 6), signal(9, 13)],
        ),
    ];
    for (left_values, right_values) in cases {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let left = make_owned(&mut ctx, &left_values, left_values.len() + 2);
        let right = make_owned(&mut ctx, &right_values, right_values.len() + 3);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let source_live = observer.live_frames();
        let result = SuppressionMask::intersect(&left, &right, &mut ctx).unwrap();
        let expected = intersect_literal(&left_values, &right_values);
        assert_eq!(result.as_slice(), expected.as_slice());
        let capacity = left_values.len() + right_values.len();
        assert_eq!(result.capacity(), capacity);
        assert_eq!(
            observer.live_frames(),
            source_live + payload_frames_for::<SignalSample>(capacity).unwrap()
        );
        assert_eq!(left.as_slice(), left_values.as_slice());
        assert_eq!(right.as_slice(), right_values.as_slice());
        drop(result);
        assert_eq!(observer.live_frames(), source_live);
        drop(left);
        drop(right);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }

    let complete_units = {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let (left_values, right_values) = intersection_inputs();
        let left = make_owned(&mut ctx, &left_values, 5);
        let right = make_owned(&mut ctx, &right_values, 6);
        let prior = work.borrow().plan_work;
        let result = SuppressionMask::intersect(&left, &right, &mut ctx).unwrap();
        drop(result);
        work.borrow().plan_work - prior
    };
    assert!(complete_units > 0);
    for allowance in 0..complete_units {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = context(&mut renderer, &work, &cancelled);
        let (left_values, right_values) = intersection_inputs();
        let left = make_owned(&mut ctx, &left_values, 5);
        let right = make_owned(&mut ctx, &right_values, 6);
        let observer = ctx.renderer.owned_suppression.observer().unwrap();
        let sources_live = observer.live_frames();
        let prior = crate::stages::MAX_PLAN_WORK_PER_READ - allowance;
        assert!(prior >= work.borrow().plan_work);
        work.borrow_mut().plan_work = prior;
        assert!(matches!(
            SuppressionMask::intersect(&left, &right, &mut ctx),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(work.borrow().plan_work - prior, allowance);
        assert_eq!(observer.live_frames(), sources_live);
        assert_eq!(left.as_slice(), left_values.as_slice());
        assert_eq!(right.as_slice(), right_values.as_slice());
        drop(left);
        drop(right);
        assert_eq!(observer.live_frames(), shell_frames().unwrap());
    }

    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = context(&mut renderer, &work, &cancelled);
    let (left_values, right_values) = intersection_inputs();
    let left = make_owned(&mut ctx, &left_values, 5);
    let right = make_owned(&mut ctx, &right_values, 6);
    let observer = ctx.renderer.owned_suppression.observer().unwrap();
    let sources_live = observer.live_frames();
    ctx.renderer.limits.maximum_resident_frames = u32::try_from(sources_live).unwrap();
    let before_vector = ctx.accepted.get()[index(MaskWorkPhase::AllocateVector)];
    assert!(matches!(
        SuppressionMask::intersect(&left, &right, &mut ctx),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(
        ctx.accepted.get()[index(MaskWorkPhase::AllocateVector)],
        before_vector
    );
    assert_eq!(observer.live_frames(), sources_live);
    drop(left);
    drop(right);
    assert_eq!(observer.live_frames(), shell_frames().unwrap());
}
