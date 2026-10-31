//! Child of point_suppression::ops::tests: observer allocations are test-only.
use super::super::super::{Charge, Observer, frames_for, payload_frames_for, shell_frames};
use super::*;
use crate::signal_transfer::{
    ControlledRootBlock, RootSignalBlock, RootSignalTransfer, SignalTransferError,
};
use crate::{MAX_INPUT_MAGNITUDE, MAX_OUTPUT_FRAMES, PreparationError};
use deadpan_core::ExactRatio;
use std::alloc::Layout;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    Before,
    Inside,
    After,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Check,
    Spend(MaskWorkPhase, usize),
    Reserve(u64),
    Read(AudioSample, u32),
}
#[derive(Clone, Copy, Debug)]
struct Event {
    kind: Kind,
    tag: Tag,
    before_work: usize,
    after_work: usize,
    before_live: u64,
    after_live: u64,
    accepted: bool,
}
#[derive(Clone, Copy)]
enum Interrupt {
    Cancel,
    Deadline,
}

/// Delegates every gate to the existing real Context; only observations are added.
struct Watching<'r, 'w> {
    inner: RefCell<Context<'r, 'w>>,
    events: RefCell<Vec<Event>>,
    tag: Cell<Tag>,
    accepted_spends: Cell<usize>,
    interrupt: Option<(usize, Interrupt)>,
}
impl<'r, 'w> Watching<'r, 'w> {
    fn new(inner: Context<'r, 'w>) -> Self {
        Self {
            inner: RefCell::new(inner),
            events: RefCell::new(Vec::new()),
            tag: Cell::new(Tag::Before),
            accepted_spends: Cell::new(0),
            interrupt: None,
        }
    }
    fn state(&self) -> (usize, u64) {
        let inner = self.inner.borrow();
        let work = inner.control.work.borrow().plan_work;
        (work, inner.renderer.owned_suppression.live_frames())
    }
    fn observer(&self) -> Option<Observer> {
        self.inner.borrow().renderer.owned_suppression.observer()
    }
    fn record(&self, kind: Kind, before: (usize, u64), accepted: bool) {
        let after = self.state();
        self.events.borrow_mut().push(Event {
            kind,
            tag: self.tag.get(),
            before_work: before.0,
            after_work: after.0,
            before_live: before.1,
            after_live: after.1,
            accepted,
        });
    }
    fn enter_read(&self, start: AudioSample, frames: u32) {
        self.record(Kind::Read(start, frames), self.state(), true);
        self.tag.set(Tag::Inside);
    }
    fn ready_read(&self) {
        self.tag.set(Tag::After);
    }
    fn interrupt_now(&self, interruption: Interrupt) {
        match interruption {
            Interrupt::Cancel => self
                .inner
                .borrow()
                .control
                .cancelled
                .store(true, Ordering::Relaxed),
            Interrupt::Deadline => {
                self.inner.borrow_mut().control.deadline = Instant::now() - Duration::from_secs(1);
            }
        }
    }
}
impl MaskContext for Watching<'_, '_> {
    fn check(&self) -> Result<(), StageAudioError> {
        let before = self.state();
        let result = self.inner.borrow().check();
        self.record(Kind::Check, before, result.is_ok());
        result
    }
    fn spend(&self, phase: MaskWorkPhase, units: usize) -> Result<(), StageAudioError> {
        let before = self.state();
        let result = self.inner.borrow().spend(phase, units);
        self.record(Kind::Spend(phase, units), before, result.is_ok());
        if result.is_ok() {
            let number = self.accepted_spends.get() + 1;
            self.accepted_spends.set(number);
            if let Some((at, interruption)) = self.interrupt
                && number == at
            {
                self.interrupt_now(interruption);
            }
        }
        result
    }
    fn reserve(&mut self, frames: u64) -> Result<Charge, StageAudioError> {
        let before = self.state();
        let result = self.inner.get_mut().reserve(frames);
        self.record(Kind::Reserve(frames), before, result.is_ok());
        result
    }
}

fn roots(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}
fn signals(start: i64, end: i64) -> Range<SignalSample> {
    SignalSample(start)..SignalSample(end)
}
fn recipe(root_at: ExactRatio, step: ExactRatio, frames: u32) -> RootSignalTransfer {
    RootSignalTransfer::new(
        roots(0, 2048),
        root_at,
        SignalSample(1000),
        step,
        signals(1000, 1000 + i64::from(frames)),
    )
    .unwrap()
}
fn small_transfer() -> RootSignalTransfer {
    recipe(ExactRatio::integer(20), ExactRatio::ONE, 3)
}
fn block(start: AudioSample, frames: u32, full_mask: bool) -> RootSignalBlock {
    let end = start.0 + i64::from(frames);
    let samples = (0..frames)
        .map(|offset| {
            let value = ((start.0 + i64::from(offset)) % 31) as f32 / 64.0;
            [value, -value]
        })
        .collect();
    let suppressed = if full_mask {
        vec![start..AudioSample(end)]
    } else {
        [roots(100, 108), roots(257, 260), roots(380, 383)]
            .into_iter()
            .filter_map(|range| {
                let left = range.start.max(start);
                let right = range.end.min(AudioSample(end));
                (left < right).then_some(left..right)
            })
            .collect()
    };
    RootSignalBlock {
        start,
        samples,
        suppressed,
    }
}
fn owned_block(
    ctx: &mut Watching<'_, '_>,
    input: RootSignalBlock,
) -> Result<ControlledRootBlock, StageAudioError> {
    let mut destination = begin_ranges::<AudioSample, _>(ctx, input.suppressed.len())?;
    for range in &input.suppressed {
        destination.push(range.start..range.end, ctx)?;
    }
    let suppressed = destination.finish(ctx)?;
    ctx.ready_read();
    Ok(ControlledRootBlock {
        start: input.start,
        samples: input.samples,
        suppressed,
    })
}
fn read_owned(
    ctx: &mut Watching<'_, '_>,
    start: AudioSample,
    frames: u32,
) -> Result<ControlledRootBlock, StageAudioError> {
    ctx.enter_read(start, frames);
    owned_block(ctx, block(start, frames, true))
}
fn bits(samples: &[[f32; 2]]) -> Vec<[u32; 2]> {
    samples
        .iter()
        .map(|frame| frame.map(f32::to_bits))
        .collect()
}
fn assert_no_payload(ctx: &Watching<'_, '_>) {
    let live = ctx.state().1;
    if let Some(observer) = ctx.observer() {
        assert_eq!(live, shell_frames().unwrap());
        assert_eq!(observer.live_frames(), live);
    } else {
        assert_eq!(live, 0);
    }
}

#[test]
fn controlled_transfer_preserves_unity_fractional_and_multicallback_pcm_and_policy() {
    let cases = [
        (ExactRatio::integer(96), ExactRatio::ONE, 64),
        (
            ExactRatio::new(385, 2).unwrap(),
            ExactRatio::new(147, 160).unwrap(),
            64,
        ),
        (ExactRatio::integer(96), ExactRatio::integer(4), 96),
    ];
    for (case, (root_at, step, frames)) in cases.into_iter().enumerate() {
        let transfer = recipe(root_at, step, frames);
        let cancelled = AtomicBool::new(false);
        let expected_reads = RefCell::new(Vec::new());
        let expected = transfer
            .render(SignalSample(1000), frames, &cancelled, |start, count| {
                expected_reads.borrow_mut().push((start, count));
                Ok::<_, SignalTransferError>(block(start, count, false))
            })
            .unwrap();
        let mut renderer = super::renderer();
        let work = RefCell::new(ReadWork::default());
        let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
        let flag_frames = frames_for(Layout::array::<bool>(frames as usize).unwrap()).unwrap();
        let actual_reads = RefCell::new(Vec::new());
        let actual = transfer
            .render_controlled(
                SignalSample(1000),
                frames,
                &cancelled,
                &mut ctx,
                |ctx, start, count| {
                    // No previous callback owner survives to the next callback.
                    assert_eq!(ctx.state().1, shell_frames().unwrap() + flag_frames);
                    actual_reads.borrow_mut().push((start, count));
                    ctx.enter_read(start, count);
                    owned_block(ctx, block(start, count, false))
                },
            )
            .unwrap();
        assert_eq!(
            actual_reads.borrow().as_slice(),
            expected_reads.borrow().as_slice()
        );
        let observed_reads: Vec<_> = ctx
            .events
            .borrow()
            .iter()
            .filter_map(|event| {
                if let Kind::Read(start, count) = event.kind {
                    Some((start, count))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(observed_reads.as_slice(), actual_reads.borrow().as_slice());
        if case == 2 {
            assert!(actual_reads.borrow().len() > 1);
        }
        assert_eq!(actual.start, expected.start);
        assert_eq!(bits(&actual.samples), bits(&expected.samples));
        assert_eq!(actual.suppressed.as_slice(), expected.suppressed.as_slice());
        assert_eq!(actual.suppressed.capacity(), actual.suppressed.len());
        let observer = ctx.observer().unwrap();
        let payload = if actual.suppressed.is_owned() {
            payload_frames_for::<SignalSample>(actual.suppressed.capacity()).unwrap()
        } else {
            0
        };
        assert_eq!(observer.live_frames(), shell_frames().unwrap() + payload);
        drop(actual);
        assert_no_payload(&ctx);
    }
}

#[test]
fn controlled_transfer_rejects_invalid_headers_and_ranges_before_masked_nan_or_zeroing() {
    for bad in 0..6 {
        let transfer = small_transfer();
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
        let calls = Cell::new(0);
        let result = transfer.render_controlled(
            SignalSample(1000),
            3,
            &cancelled,
            &mut ctx,
            |ctx, start, count| {
                calls.set(calls.get() + 1);
                ctx.enter_read(start, count);
                let mut input = block(start, count, true);
                input.samples[0][0] = f32::NAN;
                match bad {
                    0 => input.start = AudioSample(start.0 + 1),
                    1 => {
                        input.samples.pop();
                    }
                    2 => input.suppressed[0] = roots(start.0 - 1, start.0 + 1),
                    3 => {
                        input.suppressed =
                            vec![roots(start.0, start.0 + 1); MAX_OUTPUT_FRAMES as usize + 1]
                    }
                    4 => {}
                    5 => input.samples[0][0] = MAX_INPUT_MAGNITUDE + 1.0,
                    _ => unreachable!(),
                }
                owned_block(ctx, input)
            },
        );
        assert_eq!(calls.get(), 1);
        if bad < 4 {
            assert!(matches!(
                result,
                Err(StageAudioError::Transfer(
                    SignalTransferError::InvalidRootBlock
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(StageAudioError::Transfer(SignalTransferError::Preparation(
                    PreparationError::InvalidSamples
                )))
            ));
        }
        assert!(!ctx.events.borrow().iter().any(|event| {
            event.tag == Tag::After
                && matches!(
                    event.kind,
                    Kind::Spend(MaskWorkPhase::Zero | MaskWorkPhase::Mark, _)
                )
        }));
        assert_no_payload(&ctx);
    }
}

#[test]
fn complete_request_and_scratch_refusals_precede_root_callbacks() {
    let transfer = small_transfer();
    for frames in [0, MAX_OUTPUT_FRAMES + 1] {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
        let calls = Cell::new(0);
        assert!(
            transfer
                .render_controlled(
                    SignalSample(1000),
                    frames,
                    &cancelled,
                    &mut ctx,
                    |ctx, start, count| {
                        calls.set(calls.get() + 1);
                        read_owned(ctx, start, count)
                    }
                )
                .is_err()
        );
        assert_eq!(calls.get(), 0);
        assert!(
            ctx.events
                .borrow()
                .iter()
                .all(|event| event.kind == Kind::Check)
        );
        assert_eq!(ctx.state(), (0, 0));
    }
    let mut renderer = renderer();
    renderer.limits.maximum_resident_frames = u32::try_from(shell_frames().unwrap()).unwrap();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
    let calls = Cell::new(0);
    assert!(matches!(
        transfer.render_controlled(
            SignalSample(1000),
            3,
            &cancelled,
            &mut ctx,
            |ctx, start, count| {
                calls.set(calls.get() + 1);
                read_owned(ctx, start, count)
            }
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(calls.get(), 0);
    assert!(
        !ctx.events
            .borrow()
            .iter()
            .any(|event| matches!(event.kind, Kind::Spend(MaskWorkPhase::AllocateVector, _)))
    );
    assert_no_payload(&ctx);
}

#[test]
fn flags_input_and_output_storage_overlap_then_output_outlives_controller() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
    let result = small_transfer()
        .render_controlled(SignalSample(1000), 3, &cancelled, &mut ctx, read_owned)
        .unwrap();
    let shell = shell_frames().unwrap();
    let flags = frames_for(Layout::array::<bool>(3).unwrap()).unwrap();
    let input = payload_frames_for::<AudioSample>(1).unwrap();
    let output = payload_frames_for::<SignalSample>(1).unwrap();
    let events = ctx.events.borrow();
    for phase in [
        MaskWorkPhase::Scan,
        MaskWorkPhase::Zero,
        MaskWorkPhase::Mark,
    ] {
        assert!(events.iter().any(|event| event.accepted
            && event.tag == Tag::After
            && matches!(event.kind, Kind::Spend(actual, _) if actual == phase)
            && event.before_live == shell + flags + input
            && event.after_live == shell + flags + input));
    }
    assert!(events.iter().any(|event| event.accepted
        && event.tag == Tag::After
        && event.kind == Kind::Spend(MaskWorkPhase::Scan, 1)
        && event.after_live == shell + flags));
    assert!(events.iter().any(|event| event.accepted
        && event.tag == Tag::After
        && event.kind == Kind::Reserve(output)
        && event.before_live == shell + flags
        && event.after_live == shell + flags + output));
    assert!(events.iter().any(|event| event.accepted
        && event.tag == Tag::After
        && event.kind == Kind::Spend(MaskWorkPhase::AllocateStorage, 1)
        && event.after_live == shell + flags + output));
    let last_storage = events
        .iter()
        .rposition(|event| {
            event.accepted
                && event.tag == Tag::After
                && event.kind == Kind::Spend(MaskWorkPhase::AllocateStorage, 1)
        })
        .unwrap();
    // The builder's post-Box check observes the same still-live flag scratch.
    assert!(events[last_storage + 1..].iter().any(|event| event.accepted
        && event.kind == Kind::Check
        && event.after_live == shell + flags + output));
    drop(events);
    assert_eq!(result.suppressed.as_slice(), [signals(1000, 1003)]);
    assert_eq!(result.suppressed.capacity(), 1);
    let observer = ctx.observer().unwrap();
    assert_eq!(observer.live_frames(), shell + output);
    let expected_pcm = bits(&result.samples);
    drop(ctx);
    drop(renderer);
    assert_eq!(bits(&result.samples), expected_pcm);
    assert_eq!(result.suppressed.as_slice(), [signals(1000, 1003)]);
    assert_eq!(observer.live_frames(), shell + output);
    drop(result);
    assert_eq!(observer.live_frames(), shell);
}

fn successful_small_trace() -> (usize, usize, Vec<Event>) {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
    let result = small_transfer()
        .render_controlled(SignalSample(1000), 3, &cancelled, &mut ctx, read_owned)
        .unwrap();
    drop(result);
    assert_no_payload(&ctx);
    let units = work.borrow().plan_work;
    let spends = ctx.accepted_spends.get();
    let events = ctx.events.into_inner();
    (units, spends, events)
}

#[test]
fn every_small_transfer_work_prefix_refuses_without_unfunded_output_or_extra_callback() {
    let (units, _, events) = successful_small_trace();
    assert!(units > 0);
    let reads: Vec<_> = events
        .iter()
        .filter_map(|event| match event.kind {
            Kind::Read(_, _) => Some(event.before_work),
            _ => None,
        })
        .collect();
    for allowance in 0..units {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let prior = crate::stages::MAX_PLAN_WORK_PER_READ - allowance;
        work.borrow_mut().plan_work = prior;
        let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
        let result = small_transfer().render_controlled(
            SignalSample(1000),
            3,
            &cancelled,
            &mut ctx,
            read_owned,
        );
        assert!(matches!(result, Err(StageAudioError::Limit(_))));
        // The adapter has a real atomic Map2 debit; a one-unit remainder is
        // retained rather than normalized into an accepted partial operation.
        let accepted = events
            .iter()
            .find_map(|event| match event.kind {
                Kind::Spend(_, count)
                    if event.before_work < allowance
                        && allowance < event.after_work
                        && count > 1 =>
                {
                    Some(event.before_work)
                }
                _ => None,
            })
            .unwrap_or(allowance);
        assert_eq!(work.borrow().plan_work - prior, accepted);
        let expected_calls = reads.iter().filter(|offset| **offset <= accepted).count();
        let actual_calls = ctx
            .events
            .borrow()
            .iter()
            .filter(|event| matches!(event.kind, Kind::Read(_, _)))
            .count();
        assert_eq!(actual_calls, expected_calls);
        assert_no_payload(&ctx);
    }
}

#[test]
fn real_cancellation_and_deadline_stop_at_each_small_transfer_spend_and_after_read() {
    let (_, spends, _) = successful_small_trace();
    for interruption in [Interrupt::Cancel, Interrupt::Deadline] {
        for at in 1..=spends {
            let mut renderer = renderer();
            let work = RefCell::new(ReadWork::default());
            let cancelled = AtomicBool::new(false);
            let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
            ctx.interrupt = Some((at, interruption));
            let result = small_transfer().render_controlled(
                SignalSample(1000),
                3,
                &cancelled,
                &mut ctx,
                read_owned,
            );
            match interruption {
                Interrupt::Cancel => assert!(result.unwrap_err().is_cancelled()),
                Interrupt::Deadline => assert!(matches!(result, Err(StageAudioError::Timeout))),
            }
            assert_eq!(ctx.accepted_spends.get(), at);
            assert_no_payload(&ctx);
        }

        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
        let calls = Cell::new(0);
        let result = small_transfer().render_controlled(
            SignalSample(1000),
            3,
            &cancelled,
            &mut ctx,
            |ctx, start, count| {
                calls.set(calls.get() + 1);
                let block = read_owned(ctx, start, count)?;
                ctx.interrupt_now(interruption);
                Ok(block)
            },
        );
        assert_eq!(calls.get(), 1);
        match interruption {
            Interrupt::Cancel => assert!(result.unwrap_err().is_cancelled()),
            Interrupt::Deadline => assert!(matches!(result, Err(StageAudioError::Timeout))),
        }
        assert_no_payload(&ctx);

        let mut renderer = super::renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut ctx = Watching::new(context(&mut renderer, &work, &cancelled));
        ctx.interrupt_now(interruption);
        let calls = Cell::new(0);
        let result = small_transfer().render_controlled(
            SignalSample(1000),
            3,
            &cancelled,
            &mut ctx,
            |ctx, start, count| {
                calls.set(calls.get() + 1);
                read_owned(ctx, start, count)
            },
        );
        assert_eq!(calls.get(), 0);
        assert!(
            ctx.events
                .borrow()
                .iter()
                .all(|event| event.kind == Kind::Check)
        );
        match interruption {
            Interrupt::Cancel => assert!(result.unwrap_err().is_cancelled()),
            Interrupt::Deadline => assert!(matches!(result, Err(StageAudioError::Timeout))),
        }
        assert_eq!(ctx.state(), (0, 0));
    }
}
