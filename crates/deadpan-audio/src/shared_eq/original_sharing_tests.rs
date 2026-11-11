use super::*;

#[test]
fn description_retains_both_charges_through_boundary_failure_cancel_and_absent_sides() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        let description = describe_admitted(
            &plan,
            request(
                &incoming,
                &owner,
                AudioSample(128),
                Temporal::Wet(TailEffect::Delay),
                false,
            ),
            control,
            &mut admission,
        )
        .unwrap();
        assert!(description.original.is_some() && description.sound.is_some());
        let [plan_lease, core, buffer] = description.original.as_ref().unwrap().leases();
        let total = [
            plan_lease,
            core,
            buffer,
            description.sound.as_ref().unwrap().lease(),
            description.stop.lease(),
            description.boundary.lease(),
        ]
        .into_iter()
        .flatten()
        .map(sound_context_admission::Lease::frames)
        .sum::<u64>();
        assert_eq!(work.borrow().sound_context_frames, total);
        drop(description);
        assert_eq!(work.borrow().sound_context_frames, 0);
        let fallback = describe_admitted(
            &plan,
            request(
                &incoming,
                &owner,
                AudioSample(600),
                Temporal::Wet(TailEffect::Delay),
                false,
            ),
            control,
            &mut admission,
        )
        .unwrap();
        assert!(fallback.original.is_none() && fallback.sound.is_some());
        drop(fallback);
        assert_eq!(work.borrow().sound_context_frames, 0);
        let mut invalid = request(
            &incoming,
            &owner,
            AudioSample(128),
            Temporal::Wet(TailEffect::Delay),
            false,
        );
        invalid.stage = usize::MAX;
        assert!(describe_admitted(&plan, invalid, control, &mut admission).is_err());
        assert_eq!(work.borrow().sound_context_frames, 0);
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(
            describe_admitted(
                &plan,
                request(
                    &incoming,
                    &owner,
                    AudioSample(128),
                    Temporal::Wet(TailEffect::Delay),
                    false
                ),
                control,
                &mut admission
            )
            .is_err()
        );
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
    assert_eq!(renderer.active_frames, 0);
}
