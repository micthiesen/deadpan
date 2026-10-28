use super::*;

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

fn components(frames: u32, control: WorkControl<'_>) -> Components<'static> {
    let mut mix = Components::new(AudioSample(-3), frames, None, &[]);
    for voice in 0..2 {
        mix.route(
            (voice, Contribution::Original),
            Vec::new(),
            0..frames as usize,
            None,
            control,
        )
        .unwrap();
    }
    mix
}

#[test]
fn independent_prediction_precedes_pcm_and_releases_exact_payload_after_observation() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut mix = components(4, control);
    let ordinary = mix.resident_frames().unwrap();
    mix.begin_prediction().unwrap();
    let admitted = mix.resident_frames().unwrap();
    assert_eq!(admitted - ordinary, 2);
    mix.allocate_policy();
    mix.predict_range(0, 0..1, true, control).unwrap();
    mix.predict_range(1, 2..3, true, control).unwrap();
    assert!(mix.routes.iter().all(|route| route.pcm.capacity() == 0));
    assert!(
        !mix.permitted.contains(0, 0),
        "a prediction never ORs into observations"
    );
    mix.allocate();
    // Numerical zero and a real admitted contribution remain independent.
    mix.add(0, 0, ([0.0; 2], [0.0; 2]), 0, control).unwrap();
    mix.add(1, 2, ([0.0; 2], [0.0; 2]), 0, control).unwrap();
    assert_eq!(mix.verify_prediction(control).unwrap(), 2);
    assert!(mix.predicted.is_none());
    assert_eq!(mix.resident_frames().unwrap(), ordinary);
    let result = mix
        .finish(None, Permissions::default(), control, |_, _, _, _, _| {
            panic!("no temporal owner")
        })
        .unwrap();
    assert_eq!(result.samples, vec![[0.0; 2]; 4]);
    assert_eq!(
        result.suppressed,
        vec![
            AudioSample(-2)..AudioSample(-1),
            AudioSample(0)..AudioSample(1)
        ]
    );
}

#[test]
fn false_positive_negative_and_omitted_producer_samples_cannot_validate_themselves() {
    for (predicted, observed) in [(true, false), (false, true)] {
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let mut mix = components(4, control);
        mix.begin_prediction().unwrap();
        mix.allocate_policy();
        if predicted {
            mix.predict_range(0, 1..2, true, control).unwrap();
        }
        mix.allocate();
        if observed {
            mix.add(0, 1, ([0.25; 2], [0.0; 2]), 0, control).unwrap();
        }
        assert_eq!(mix.predicted.as_ref().unwrap().contains(0, 1), predicted);
        assert_eq!(mix.permitted.contains(0, 1), observed);
        assert!(matches!(
            mix.verify_prediction(control),
            Err(StageAudioError::Plan(PlanError::InvalidPlan(
                "contribution permission prediction differs"
            )))
        ));
        assert!(
            mix.predicted.is_some(),
            "a mismatch cannot drop the comparison state as verified"
        );
        assert!(
            mix.finish(
                None,
                Permissions::default(),
                control,
                |_, _, _, _, _| panic!("unverified")
            )
            .is_err()
        );
    }
}

#[test]
fn packed_prediction_ranges_keep_half_open_boundaries_and_other_routes() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut mix = components(129, control);
    mix.begin_prediction().unwrap();
    mix.allocate_policy();
    mix.predict_range(1, 0..129, true, control).unwrap();
    mix.predict_range(1, 63..65, false, control).unwrap();
    mix.predict_range(1, 128..129, false, control).unwrap();
    for at in 0..129 {
        assert!(!mix.predicted.as_ref().unwrap().contains(0, at));
        assert_eq!(
            mix.predicted.as_ref().unwrap().contains(1, at),
            at != 63 && at != 64 && at != 128
        );
        assert!(!mix.permitted.contains(1, at));
    }
    assert!(mix.predict_range(1, 128..130, true, control).is_err());
    assert!(mix.predict_range(1, 64..64, false, control).is_err());
}

#[test]
fn prediction_word_mutations_and_comparisons_keep_partial_work_on_refusal() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut mix = components(129, control);
    mix.begin_prediction().unwrap();
    mix.allocate_policy();
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 2;
    assert!(matches!(
        mix.predict_range(0, 0..129, true, control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    assert!(mix.predicted.as_ref().unwrap().contains(0, 63));
    assert!(!mix.predicted.as_ref().unwrap().contains(0, 64));
    assert!(mix.routes.iter().all(|route| route.pcm.is_empty()));

    work.borrow_mut().plan_work = 0;
    let mut equal = components(1, control);
    equal.begin_prediction().unwrap();
    equal.allocate();
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 3;
    assert_eq!(equal.verify_prediction(control).unwrap(), 2);
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    work.borrow_mut().plan_work = 0;
    let mut one_short = components(1, control);
    one_short.begin_prediction().unwrap();
    one_short.allocate();
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 2;
    assert!(matches!(
        one_short.verify_prediction(control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    assert!(one_short.predicted.is_some());
}

#[test]
fn prediction_cannot_start_after_sample_allocation_or_survive_bad_pcm_as_observation() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut late = components(1, control);
    late.allocate();
    assert!(late.begin_prediction().is_err());
    let mut mix = components(1, control);
    mix.begin_prediction().unwrap();
    mix.allocate_policy();
    mix.predict_range(0, 0..1, true, control).unwrap();
    mix.allocate();
    assert!(matches!(
        mix.add(0, 0, ([0.25, f64::NAN], [0.0; 2]), 0, control),
        Err(StageAudioError::Preparation(
            PreparationError::InvalidSamples
        ))
    ));
    assert!(!mix.permitted.contains(0, 0));
    assert!(mix.predicted.as_ref().unwrap().contains(0, 0));
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(
        mix.verify_prediction(control),
        Err(StageAudioError::Preparation(PreparationError::Cancelled))
    ));
    assert!(mix.predicted.is_some());
}
