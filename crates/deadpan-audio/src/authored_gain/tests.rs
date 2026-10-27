use super::*;
use deadpan_core::{
    ClipGain, GainClock, GainCurve, GainDb, GainEnvelope, GainRange, GainSegment, Saturation,
};

fn work_control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(5),
        work,
    }
}

fn constant(trim: i32, muted: bool) -> ClipGain {
    ClipGain::new(GainDb::new(trim).unwrap(), muted, vec![], vec![]).unwrap()
}

fn dynamic_apply(gain: &mut GainSum, clip: &ClipGain, control: WorkControl<'_>) {
    gain.treatment(
        &AudioTreatments::from_clip_gain(clip.clone()),
        ExactRatio::new(17, 7).unwrap(),
        control,
        0..1,
    )
    .unwrap();
}

#[test]
fn prepared_clip_gain_keeps_exact_cancellation_and_nonlinear_stage_boundaries() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let positive = constant(24_000, false);
    let negative = constant(-24_000, false);
    let prepared_positive = PreparedClipGain::prepare(&positive, control)
        .unwrap()
        .unwrap();
    let prepared_negative = PreparedClipGain::prepare(&negative, control)
        .unwrap()
        .unwrap();
    let stage = ChannelStage::Saturation(Saturation::new(GainDb::new(6000).unwrap()).unwrap());
    for between in [false, true] {
        let mut prepared = GainSum::default();
        let mut original = GainSum::default();
        prepared_positive.apply(&mut prepared, control).unwrap();
        dynamic_apply(&mut original, &positive, control);
        if between {
            prepared.channel_stage(stage).unwrap();
            original.channel_stage(stage).unwrap();
        }
        prepared_negative.apply(&mut prepared, control).unwrap();
        dynamic_apply(&mut original, &negative, control);
        if !between {
            assert_eq!(prepared.millidecibels_q32, 0);
            prepared.channel_stage(stage).unwrap();
            original.channel_stage(stage).unwrap();
        }
        assert!(prepared.same_transfer(&original));
        assert_eq!(prepared.millidecibels_q32, original.millidecibels_q32);
        assert!(prepared.stages == original.stages);
        for frame in [[0.0, -0.0], [0.2, -0.7], [1.5, -2.0], [1e-18, -1e-18]] {
            assert_eq!(
                prepared.apply_wide(frame).unwrap(),
                original.apply_wide(frame).unwrap()
            );
        }
    }
}

#[test]
fn prepared_whole_owner_mute_preserves_the_exact_dynamic_transfer() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let clip = constant(12000, true);
    let compiled = PreparedClipGain::prepare(&clip, control).unwrap().unwrap();
    let mut prepared = GainSum::default();
    let mut original = GainSum::default();
    let stage = ChannelStage::Saturation(Saturation::new(GainDb::new(9000).unwrap()).unwrap());
    prepared.channel_stage(stage).unwrap();
    original.channel_stage(stage).unwrap();
    compiled.apply(&mut prepared, control).unwrap();
    dynamic_apply(&mut original, &clip, control);
    assert!(prepared.same_transfer(&original));
    for frame in [[0.1, 2.0], [-7.0, 0.0]] {
        assert_eq!(prepared.apply_wide(frame).unwrap(), [0.0; 2]);
        assert_eq!(
            prepared.apply_wide(frame).unwrap(),
            original.apply_wide(frame).unwrap()
        );
    }
}

#[test]
fn flat_envelopes_and_mute_ranges_retain_their_half_open_dynamic_clock() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let range = GainRange::new(ExactRatio::integer(1), ExactRatio::integer(3)).unwrap();
    let flat = GainEnvelope::new(
        GainClock::OwnerOutput,
        range,
        GainDb::new(6000).unwrap(),
        vec![GainSegment::new(range.end(), GainDb::new(6000).unwrap(), GainCurve::Step).unwrap()],
    )
    .unwrap();
    let automated = ClipGain::new(GainDb::UNITY, false, vec![flat], vec![]).unwrap();
    let ranged_mute = ClipGain::new(GainDb::UNITY, false, vec![], vec![range]).unwrap();
    assert!(
        PreparedClipGain::prepare(&automated, control)
            .unwrap()
            .is_none()
    );
    assert!(
        PreparedClipGain::prepare(&ranged_mute, control)
            .unwrap()
            .is_none()
    );
    for (local, active) in [(0, false), (1, true), (2, true), (3, false)] {
        let local = ExactRatio::integer(local);
        let gain = automated.evaluate(local).unwrap();
        assert_eq!(
            gain.millidecibels,
            ExactRatio::integer(if active { 6000 } else { 0 })
        );
        assert_eq!(ranged_mute.evaluate(local).unwrap().muted, active);
    }
}

#[test]
fn prepared_program_has_bounded_compilation_and_application_work_and_checked_addition() {
    const APPLICATIONS: usize = 256;
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork {
        plan_work: MAX_PLAN_WORK_PER_READ - APPLICATIONS - 2,
        ..Default::default()
    });
    let control = work_control(&cancelled, &work);
    let prepared = PreparedClipGain::prepare(&constant(3000, false), control)
        .unwrap()
        .unwrap();
    for _ in 0..APPLICATIONS {
        let mut gain = GainSum::default();
        prepared.apply(&mut gain, control).unwrap();
        assert_eq!(
            gain.millidecibels_q32,
            3000 * i128::from(GAIN_NUMERIC_SCALE)
        );
    }
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    assert!(matches!(
        prepared.apply(&mut GainSum::default(), control),
        Err(StageAudioError::Limit("plan work per read"))
    ));

    let fresh_work = RefCell::new(ReadWork::default());
    let mut gain = GainSum {
        millidecibels_q32: i128::MAX,
        ..Default::default()
    };
    assert!(
        prepared
            .apply(&mut gain, work_control(&cancelled, &fresh_work))
            .is_err()
    );
    assert_eq!(gain.millidecibels_q32, i128::MAX);
}

#[test]
fn constant_transfer_reuses_exact_gain_boundaries_and_send_cancellation() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let mut live = GainSum::default();
    // These factors cannot be converted separately: each overflows amplitude,
    // while the exact sum cancels before the first real channel boundary.
    live.add_q32(i128::MAX / 4, false).unwrap();
    live.add_q32(-(i128::MAX / 4), false).unwrap();
    live.add_q32(3000 * i128::from(GAIN_NUMERIC_SCALE), false)
        .unwrap();
    live.channel_stage(ChannelStage::Balance(
        deadpan_core::StereoBalance::new(-37).unwrap(),
    ))
    .unwrap();
    live.add_q32(-6000 * i128::from(GAIN_NUMERIC_SCALE), false)
        .unwrap();
    live.channel_stage(ChannelStage::Saturation(
        Saturation::new(GainDb::new(9000).unwrap()).unwrap(),
    ))
    .unwrap();
    live.add_q32(6000 * i128::from(GAIN_NUMERIC_SCALE), false)
        .unwrap();
    let prepared = live.prepare(control).unwrap();
    let sent = live
        .prepare_send(GainDb::new(-6000).unwrap(), control)
        .unwrap();
    for input in [[0.0, -0.0], [0.17, -0.33], [2.0, -1e-30]] {
        assert_eq!(
            prepared
                .apply_wide(input, control)
                .unwrap()
                .map(f64::to_bits),
            live.apply_wide(input).unwrap().map(f64::to_bits)
        );
        assert_eq!(
            sent.apply_wide(input, control).unwrap().map(f64::to_bits),
            live.send_sample(input, GainDb::new(-6000).unwrap())
                .unwrap()
                .map(f64::to_bits)
        );
    }
}

#[test]
fn prepared_transfer_preserves_muted_validation_and_unconsumed_factor_errors() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let mut live = GainSum {
        millidecibels_q32: i128::MAX,
        ..Default::default()
    };
    // Compiling an unconsumed checkpoint must not produce an early finite
    // error. The original evaluator only checks amplitude at consumption.
    let prepared = live.prepare(control).unwrap();
    assert!(prepared.apply_wide([0.0; 2], control).is_err());
    assert!(live.apply_wide([0.0; 2]).is_err());
    assert!(live.prepare_send(GainDb::new(1).unwrap(), control).is_err());
    assert_eq!(live.millidecibels_q32, i128::MAX);
    live.muted = true;
    let prepared = live.prepare(control).unwrap();
    assert_eq!(
        prepared.apply_wide([f64::MAX, -f64::MAX], control).unwrap(),
        [0.0; 2]
    );
    assert!(prepared.apply_wide([f64::NAN, 0.0], control).is_err());
    assert!(live.apply_wide([f64::NAN, 0.0]).is_err());
}

#[test]
fn prepared_transfer_charges_compilation_and_each_consumed_channel_operation() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let mut gain = GainSum::default();
    gain.channel_stage(ChannelStage::Balance(
        deadpan_core::StereoBalance::new(50).unwrap(),
    ))
    .unwrap();
    let prepared = gain.prepare(control).unwrap();
    assert_eq!(work.borrow().plan_work, 2);
    for _ in 0..32 {
        prepared.apply_wide([0.25, -0.5], control).unwrap();
    }
    assert_eq!(work.borrow().plan_work, 66);
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 1;
    assert!(matches!(
        prepared.apply_wide([0.25, -0.5], control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
}
