use super::*;

#[test]
fn activity_words_stop_early_and_ignore_non_sample_padding() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    let mut flags = SamplePermissions::new(65);
    flags.allocate(2);
    flags.words[1] = 2; // Only bit zero of this last word is a real sample.
    assert!(!flags.any_words(0, control).unwrap());
    assert_eq!(work.borrow().plan_work, 2);
    flags.mark(0, 64).unwrap();
    assert!(flags.any_words(0, control).unwrap());
    assert_eq!(work.borrow().plan_work, 4);
    flags.mark(1, 0).unwrap();
    assert!(flags.any_words(1, control).unwrap());
    assert_eq!(work.borrow().plan_work, 5);
    assert!(flags.any_words(2, control).is_err());
    assert_eq!(work.borrow().plan_work, 5);
}

#[test]
fn activity_word_refusal_retains_the_exact_inspected_prefix() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork {
        plan_work: MAX_PLAN_WORK_PER_READ - 1,
        ..Default::default()
    });
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    let mut flags = SamplePermissions::new(65);
    flags.allocate(1);
    flags.mark(0, 64).unwrap();
    assert!(matches!(
        flags.any_words(0, control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    work.borrow_mut().plan_work = 0;
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(flags.any_words(0, control).is_err());
    assert_eq!(work.borrow().plan_work, 0);
}

#[test]
fn permission_payload_is_deferred_exact_and_preserved_across_growth() {
    let mut flags = SamplePermissions::new(256);
    let header = size_of::<SamplePermissions>();
    assert_eq!(flags.words.capacity(), 0);
    assert_eq!(flags.resident_bytes(0).unwrap(), header);
    assert_eq!(
        flags.resident_bytes(2).unwrap(),
        header + size_of::<[u64; 8]>()
    );
    assert!(flags.mark(0, 0).is_err());
    flags.allocate(2);
    assert_eq!(flags.words.capacity(), 8);
    flags.mark(0, 255).unwrap();
    flags.mark(1, 0).unwrap();
    assert!(flags.contains(0, 255) && flags.contains(1, 0));
    assert!(!flags.contains(0, 0) && !flags.contains(1, 255));
    assert!(!flags.contains(0, 256) && !flags.contains(2, 0));
    let pointer = flags.words.as_ptr();
    flags.allocate(2);
    assert_eq!(flags.words.as_ptr(), pointer);
    // Growth must admit both old and replacement allocations while copying.
    assert_eq!(
        flags.resident_bytes(3).unwrap(),
        header + (8 + 12) * size_of::<u64>()
    );
    flags.allocate(3);
    assert_eq!(flags.words.capacity(), 12);
    assert_eq!(
        flags.resident_bytes(3).unwrap(),
        header + 12 * size_of::<u64>()
    );
    assert!(flags.contains(0, 255) && flags.contains(1, 0));
    assert!(!flags.contains(2, 0));
}

#[test]
fn permission_payload_supports_existing_one_and_1024_frame_windows() {
    let mut one = SamplePermissions::new(1);
    assert_eq!(
        one.resident_bytes(1).unwrap(),
        size_of::<SamplePermissions>() + 8
    );
    one.allocate(1);
    assert_eq!(one.words.capacity(), 1);
    one.mark(0, 0).unwrap();
    assert!(one.contains(0, 0));
    assert!(!one.contains(0, 1));
    assert!(matches!(one.mark(0, 1), Err(StageAudioError::Range)));

    let mut large = SamplePermissions::new(1024);
    large.allocate(2);
    assert_eq!(large.words.capacity(), 32);
    for at in [0, 63, 64, 255, 256, 1023] {
        large.mark(1, at).unwrap();
        assert!(large.contains(1, at));
        assert!(!large.contains(0, at));
    }
    assert!(!large.contains(1, 1024));
    assert!(large.mark(1, 1024).is_err());
}

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

#[test]
fn metadata_permissions_can_precede_pcm_and_allocation_preserves_them() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut components = Components::new(AudioSample(0), 1024, None, &[]);
    for voice in 0..2 {
        components
            .route(
                (voice, Contribution::Original),
                vec![],
                0..1024,
                None,
                control,
            )
            .unwrap();
    }
    let prospective = components.resident_frames().unwrap();
    assert_eq!(components.permitted.words.capacity(), 0);
    components.allocate_policy();
    for (route, at) in [(0, 0), (0, 1023), (1, 64)] {
        components.permitted.mark(route, at).unwrap();
    }
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.event_gain.is_none())
    );
    assert_eq!(components.resident_frames().unwrap(), prospective);
    let pointer = components.permitted.words.as_ptr();
    components.allocate();
    assert_eq!(components.permitted.words.as_ptr(), pointer);
    for (route, at) in [(0, 0), (0, 1023), (1, 64)] {
        assert!(components.permitted.contains(route, at));
    }
    assert!(!components.permitted.contains(1, 1023));
    assert_eq!(size_of::<Sample>(), 4 * size_of::<f64>());
}

#[test]
fn add_marks_only_after_both_finite_checks_and_preserves_event_gain_errors() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    for dry in [[f64::NAN, 0.0], [0.25, f64::INFINITY]] {
        let mut components = Components::new(AudioSample(0), 2, None, &[]);
        let route = components
            .route((0, Contribution::Original), vec![], 0..2, None, control)
            .unwrap();
        components.allocate();
        assert!(matches!(
            components.add(route, 0, (dry, [0.0; 2]), 123, control),
            Err(StageAudioError::Preparation(
                PreparationError::InvalidSamples
            ))
        ));
        assert_eq!(components.routes[route].event_gain, Some(123));
        assert!(!components.permitted.contains(route, 0));
        if dry[0].is_finite() {
            assert_eq!(components.routes[route].pcm[0].value[0], 0.25);
        }
    }
    let mut components = Components::new(AudioSample(0), 2, None, &[]);
    let route = components
        .route((0, Contribution::Original), vec![], 0..2, None, control)
        .unwrap();
    components.allocate();
    components
        .add(route, 0, ([0.25; 2], [0.0; 2]), 123, control)
        .unwrap();
    assert!(components.permitted.contains(route, 0));
    assert!(matches!(
        components.add(route, 1, ([0.25; 2], [0.0; 2]), 456, control),
        Err(StageAudioError::Plan(PlanError::InvalidPlan(
            "inconsistent contribution event gain"
        )))
    ));
    assert!(!components.permitted.contains(route, 1));
    assert_eq!(components.routes[route].event_gain, Some(123));
    assert_eq!(
        work.borrow().plan_work,
        0,
        "existing add never billed a policy walk"
    );
}
