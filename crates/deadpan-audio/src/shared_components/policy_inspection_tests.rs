use super::*;

#[test]
fn borrowed_policy_clocks_and_literal_short_circuit_probe_counts() {
    let plan = tail_plan();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let components = layout(&plan, "tail", false, 0, control(&cancelled, &work));
    let owner = components
        .owners
        .values()
        .find(|owner| owner.clock.instance().node.as_str() == "root")
        .unwrap();
    let mut probes = 0;
    let first = first_clock_ref_inspected(
        &components.routes,
        owner,
        |id| id < 2,
        true,
        || {
            probes += 1;
            Ok(())
        },
    )
    .unwrap()
    .unwrap();
    // Original: member + header + explicit Tail. Sound: member + header.
    assert_eq!(probes, 5);
    assert!(std::ptr::eq(first, &components.routes[1].layers[0].0));
    let mut probes = 0;
    let context = wet_context_ref_inspected(
        &components.routes,
        owner,
        |id| id < 2,
        || {
            probes += 1;
            Ok(())
        },
    )
    .unwrap();
    // The third inactive member is still inspected, without a Tail probe.
    assert_eq!(probes, 6);
    assert!(!context.permissions.contains(0));
    assert!(context.permissions.contains(1));
    assert!(!context.permissions.contains(2));
    assert!(std::ptr::eq(
        context.clock.unwrap(),
        &components.routes[1].layers[0].0
    ));
    let mut probes = 0;
    let dry = first_clock_ref_inspected(
        &components.routes,
        owner,
        |_| true,
        false,
        || {
            probes += 1;
            Ok(())
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(probes, 1);
    assert!(std::ptr::eq(dry, &components.routes[0].layers[0].0));
    assert!(
        components
            .routes
            .iter()
            .all(|route| route.pcm.capacity() == 0)
    );
}

#[test]
fn inspected_policy_refusal_keeps_exact_performed_work_without_later_probes() {
    let plan = tail_plan();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let components = layout(&plan, "tail", false, 0, control(&cancelled, &work));
    let owner = components
        .owners
        .values()
        .find(|owner| owner.clock.instance().node.as_str() == "root")
        .unwrap();
    for available in 0..=6 {
        work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - available;
        let mut attempts = 0;
        let result = wet_context_ref_inspected(
            &components.routes,
            owner,
            |id| id < 2,
            || {
                attempts += 1;
                control(&cancelled, &work).spend_plan_work(1)
            },
        );
        assert_eq!(attempts, if available == 6 { 6 } else { available + 1 });
        assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        assert_eq!(result.is_ok(), available == 6);
    }
    work.borrow_mut().plan_work = 0;
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        permissions_inspected(
            &components.routes,
            owner,
            |_| true,
            false,
            || control(&cancelled, &work).spend_plan_work(1)
        )
        .is_err()
    );
    assert_eq!(work.borrow().plan_work, 0);
    assert_eq!(work.borrow().source_checks, 0);
    assert_eq!(work.borrow().prepared_frames, 0);
}
