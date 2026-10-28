use super::*;
use crate::stages::shared_components::program::tests::plan;
use deadpan_core::{GainDb, TailEffect, TailSends};

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

fn metadata<'plan>(plan: &'plan RenderPlan, control: WorkControl<'_>) -> Components<'plan> {
    let query = plan
        .audio_gain_owners(AudioSample(12)..AudioSample(16), Default::default())
        .unwrap();
    let layers: Vec<_> = query.spans()[0]
        .owners()
        .iter()
        .cloned()
        .map(|clock| {
            let until = clock.treatments().map_or(0, |recipe| recipe.order().len());
            (clock, until)
        })
        .collect();
    let mut components = Components::new(AudioSample(12), 4, None, &[]);
    for (voice, contribution) in [
        (5, Contribution::Original),
        (2, Contribution::Sound(Default::default())),
    ] {
        components
            .route((voice, contribution), layers.clone(), 0..4, None, control)
            .unwrap();
    }
    let _prospective = components.resident_frames().unwrap();
    components.allocate_policy();
    for (route, at) in [(0, 0), (0, 2), (1, 1), (1, 2)] {
        components.permitted.mark(route, at).unwrap();
    }
    components
}

#[test]
fn equalizer_phase_emits_metadata_requests_without_pcm_and_borrows_one_clock() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut components = metadata(&plan, control(&cancelled, &work));
    let owner = components
        .owners
        .values()
        .find(|owner| owner.clock.instance().node.as_str() == "inner")
        .unwrap();
    let at = 2;
    let mask = policy::permissions(
        &components.routes,
        owner,
        |id| components.permitted.contains(id, at),
        false,
    )
    .unwrap();
    let clock = policy::first_clock(
        &components.routes,
        owner,
        |id| components.permitted.contains(id, at),
        false,
    )
    .unwrap();
    let mut voices = Vec::new();
    visit_equalizer(
        &mut components.routes,
        EqualizerPhase {
            owner,
            permitted: &components.permitted,
            at,
            sample: AudioSample(14),
            stage: 2,
            permissions: &mask,
            clock: Some(&clock),
        },
        || Ok(()),
        |id, route, request| {
            assert!(route.pcm.is_empty());
            assert!(std::ptr::eq(request.clock, &clock));
            assert!(request.kind == Temporal::Equalizer(2));
            assert_eq!(request.sample, AudioSample(14));
            assert!(request.permissions.contains(5) && request.permissions.contains(2));
            voices.push((id, request.voice));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(voices, vec![(0, Some(5)), (1, Some(2))]);
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
}

#[test]
fn wet_phase_visits_reverb_then_delay_with_exact_levels_and_borrowed_clock() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let components = metadata(&plan, control(&cancelled, &work));
    let owner = components
        .owners
        .values()
        .find(|owner| owner.clock.instance().node.as_str() == "inner")
        .unwrap();
    let context = policy::wet_context(&components.routes, owner, |id| {
        components.permitted.contains(id, 2)
    })
    .unwrap();
    let clock = context.clock.as_ref().unwrap();
    let sends = TailSends::default()
        .with_level(TailEffect::Reverb, Some(GainDb::new(-3000).unwrap()))
        .with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap()));
    let mut calls = Vec::new();
    visit_wet_effects(
        WetPhase {
            sample: AudioSample(14),
            permissions: &context.permissions,
            clock: Some(clock),
            voice: 2,
            sends,
        },
        || Ok(()),
        |level, request| {
            assert!(std::ptr::eq(request.clock, clock));
            assert_eq!(request.voice, Some(2));
            assert!(request.permissions.contains(5) && request.permissions.contains(2));
            calls.push((
                if request.kind == Temporal::Wet(TailEffect::Reverb) {
                    0
                } else {
                    1
                },
                level.millidecibels(),
            ));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls, vec![(0, -3000), (1, -6000)]);
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
}

#[test]
fn phase_visitation_propagates_first_callback_error_without_later_requests() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut components = metadata(&plan, control(&cancelled, &work));
    let owner = components
        .owners
        .values()
        .find(|owner| owner.clock.instance().node.as_str() == "inner")
        .unwrap();
    let mask = policy::permissions(
        &components.routes,
        owner,
        |id| components.permitted.contains(id, 2),
        false,
    )
    .unwrap();
    let clock = policy::first_clock(
        &components.routes,
        owner,
        |id| components.permitted.contains(id, 2),
        false,
    )
    .unwrap();
    let mut calls = 0;
    assert!(matches!(
        visit_equalizer(
            &mut components.routes,
            EqualizerPhase {
                owner,
                permitted: &components.permitted,
                at: 2,
                sample: AudioSample(14),
                stage: 2,
                permissions: &mask,
                clock: Some(&clock),
            },
            || Ok(()),
            |_, _, _| {
                calls += 1;
                Err(StageAudioError::Unsupported("stop visitor"))
            }
        ),
        Err(StageAudioError::Unsupported("stop visitor"))
    ));
    assert_eq!(calls, 1);
    calls = 0;
    let sends = TailSends::default()
        .with_level(TailEffect::Reverb, Some(GainDb::UNITY))
        .with_level(TailEffect::Delay, Some(GainDb::UNITY));
    assert!(matches!(
        visit_wet_effects(
            WetPhase {
                sample: AudioSample(14),
                permissions: &mask,
                clock: Some(&clock),
                voice: 2,
                sends,
            },
            || Ok(()),
            |_, _| {
                calls += 1;
                Err(StageAudioError::Unsupported("stop visitor"))
            }
        ),
        Err(StageAudioError::Unsupported("stop visitor"))
    ));
    assert_eq!(calls, 1);
}

#[test]
fn inactive_phase_does_not_require_a_clock_but_active_request_still_does() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut components = metadata(&plan, control(&cancelled, &work));
    let owner = components
        .owners
        .values()
        .find(|owner| owner.clock.instance().node.as_str() == "inner")
        .unwrap();
    let mask = Permissions::default();
    let unallocated = permissions::SamplePermissions::new(4);
    assert!(matches!(
        visit_equalizer(
            &mut components.routes,
            EqualizerPhase {
                owner,
                permitted: &unallocated,
                at: 3,
                sample: AudioSample(15),
                stage: 2,
                permissions: &mask,
                clock: None,
            },
            || Ok(()),
            |_, _, _| panic!("unprepared policy must not masquerade as inactive")
        ),
        Err(StageAudioError::Plan(PlanError::InvalidPlan(
            "contribution permissions not allocated"
        )))
    ));
    visit_equalizer(
        &mut components.routes,
        EqualizerPhase {
            owner,
            permitted: &components.permitted,
            at: 3,
            sample: AudioSample(15),
            stage: 2,
            permissions: &mask,
            clock: None,
        },
        || Ok(()),
        |_, _, _| panic!("inactive request"),
    )
    .unwrap();
    assert!(matches!(
        visit_equalizer(
            &mut components.routes,
            EqualizerPhase {
                owner,
                permitted: &components.permitted,
                at: 0,
                sample: AudioSample(12),
                stage: 2,
                permissions: &mask,
                clock: None,
            },
            || Ok(()),
            |_, _, _| panic!("missing clock must reject first")
        ),
        Err(StageAudioError::Plan(PlanError::InvalidPlan(
            "shared EQ has no admitted clock"
        )))
    ));
}

#[test]
fn metadata_inspection_hooks_bill_inactive_probes_and_stop_at_the_actual_error() {
    let plan = plan(false, false, true);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut components = metadata(&plan, control);
    let owner = components
        .owners
        .values()
        .find(|owner| owner.clock.instance().node.as_str() == "inner")
        .unwrap();
    let mask = policy::permissions(
        &components.routes,
        owner,
        |id| components.permitted.contains(id, 0),
        false,
    )
    .unwrap();
    let clock = policy::first_clock(
        &components.routes,
        owner,
        |id| components.permitted.contains(id, 0),
        false,
    )
    .unwrap();
    let before = work.borrow().plan_work;
    let mut callbacks = 0;
    visit_equalizer(
        &mut components.routes,
        EqualizerPhase {
            owner,
            permitted: &components.permitted,
            at: 0,
            sample: AudioSample(12),
            stage: 2,
            permissions: &mask,
            clock: Some(&clock),
        },
        || control.spend_plan_work(1),
        |_, _, _| {
            callbacks += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(work.borrow().plan_work - before, 2);
    assert_eq!(
        callbacks, 1,
        "inactive member still consumed one inspection"
    );

    let before = work.borrow().plan_work;
    visit_wet_effects(
        WetPhase {
            sample: AudioSample(12),
            permissions: &mask,
            clock: Some(&clock),
            voice: 5,
            sends: TailSends::default().with_level(TailEffect::Delay, Some(GainDb::UNITY)),
        },
        || control.spend_plan_work(1),
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(
        work.borrow().plan_work - before,
        2,
        "absent Reverb is still probed"
    );

    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 1;
    callbacks = 0;
    assert!(matches!(
        visit_equalizer(
            &mut components.routes,
            EqualizerPhase {
                owner,
                permitted: &components.permitted,
                at: 2,
                sample: AudioSample(14),
                stage: 2,
                permissions: &mask,
                clock: Some(&clock),
            },
            || control.spend_plan_work(1),
            |_, _, _| {
                callbacks += 1;
                Ok(())
            },
        ),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(callbacks, 1);
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
}
