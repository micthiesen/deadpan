//! Plan-protocol custody tests for admitted Tape masks; not native ownership proof.
use super::*;
use crate::AudioSignalTape;

fn full_tape(plan: &RenderPlan, tape: &AudioSignalTape<'_>) -> Range<SignalSample> {
    let route: Range<deadpan_core::ExactRatio> =
        serde_json::from_value(tape.provider_before_snapshot(plan)["route"].clone()).unwrap();
    let grid = tape.grid();
    grid.boundary(route.start).unwrap()..grid.boundary(route.end).unwrap()
}

#[test]
fn empty_tape_mask_handoff_has_no_allocation_or_lease() {
    let mut cases = 0;
    crate::audio_tape::policy_before::visit_cases(|plan, name, tape| {
        let start = full_tape(plan, tape).start;
        let mut backend = Backend::<true>::new();
        let query = tape
            .policy_admitted(start..start, Default::default(), &mut backend)
            .unwrap();
        assert_eq!(query.work(), 0, "{name}");
        assert!(query.mask.memory.lease.is_none(), "{name}");
        assert_eq!(query.mask.ranges.capacity(), 0, "{name}");
        let result = query.into_suppression(&mut backend).unwrap();
        assert!(result.ranges.is_empty(), "{name}");
        assert_eq!(result.ranges.capacity(), 0, "{name}");
        assert!(result.lease.is_none(), "{name}");
        {
            let state = backend.0.borrow();
            assert_eq!(state.prepare_calls, 1, "{name}");
            assert_eq!(state.commit_calls, 1, "{name}");
            assert_eq!(state.reservations, 0, "{name}");
            assert_eq!(state.spends, 0, "{name}");
            assert_eq!(state.live, 0, "{name}");
            assert_eq!(state.commit.unwrap().source_bytes, 0, "{name}");
            assert!(state.commit.unwrap().source_id.is_none(), "{name}");
        }
        drop(result);
        backend.released();
        cases += 1;
    });
    assert_eq!(cases, 9);
}

#[test]
fn checked_tapes_match_ordinary_policy_for_both_end_modes_and_admission_modes() {
    let mut cases = 0;
    crate::audio_tape::policy_before::visit_cases(|plan, name, tape| {
        let full = full_tape(plan, tape);
        for after_preserve in [false, true] {
            for samples in [full.clone(), full.start..full.start] {
                let empty_request = samples.is_empty();
                let ordinary = if after_preserve {
                    tape.policy_after_preserve(samples.clone(), Default::default())
                } else {
                    tape.policy(samples.clone(), Default::default())
                };
                match ordinary {
                    Ok(expected) => {
                        if empty_request {
                            assert_eq!(expected.work, 0, "{name}");
                        }
                        let expected_value = ordinary_value(&expected);
                        let expected_work = expected.work;
                        drop(expected);
                        let mut metered = Backend::<true>::new();
                        let actual = if after_preserve {
                            tape.policy_after_preserve_admitted(
                                samples.clone(),
                                Default::default(),
                                &mut metered,
                            )
                        } else {
                            tape.policy_admitted(samples.clone(), Default::default(), &mut metered)
                        }
                        .unwrap_or_else(|error| {
                            panic!("Tape {name}: admitted query failed: {error:?}")
                        });
                        assert_eq!(policy_value(actual.view()), expected_value, "{name}");
                        assert_eq!(actual.canonical_units(), expected_work, "{name}");
                        assert_eq!(
                            actual.work(),
                            actual.canonical_units() + actual.additional_units()
                        );
                        if empty_request {
                            assert_eq!(actual.work(), 0, "{name}");
                        }
                        assert!(actual.was_forwarded());
                        assert_eq!(metered.0.borrow().accepted_work, actual.work());
                        drop(actual);
                        metered.released();

                        let mut unmetered = Backend::<false>::new();
                        let actual = if after_preserve {
                            tape.policy_after_preserve_admitted(
                                samples.clone(),
                                Default::default(),
                                &mut unmetered,
                            )
                        } else {
                            tape.policy_admitted(samples, Default::default(), &mut unmetered)
                        }
                        .unwrap_or_else(|error| {
                            panic!("Tape {name}: unmetered query failed: {error:?}")
                        });
                        assert_eq!(policy_value(actual.view()), expected_value, "{name}");
                        assert_eq!(actual.canonical_units(), expected_work, "{name}");
                        assert_eq!(actual.work(), expected_work, "{name}");
                        if actual.view().suppressed.is_empty() && actual.view().contents.is_empty()
                        {
                            assert_eq!(actual.work(), 0, "empty Tape request: {name}");
                        }
                        assert_eq!(actual.additional_units(), 0);
                        assert!(!actual.was_forwarded());
                        assert!(unmetered.0.borrow().events.is_empty());
                        drop(actual);
                        unmetered.released();
                    }
                    Err(expected_error) => {
                        let expected_error = expected_error.tape_before_error_snapshot();
                        for metered in [false, true] {
                            if metered {
                                let mut backend = Backend::<true>::new();
                                let actual = if after_preserve {
                                    tape.policy_after_preserve_admitted(
                                        samples.clone(),
                                        Default::default(),
                                        &mut backend,
                                    )
                                } else {
                                    tape.policy_admitted(
                                        samples.clone(),
                                        Default::default(),
                                        &mut backend,
                                    )
                                };
                                let error = actual.err().unwrap_or_else(|| {
                                    panic!(
                                        "Tape {name}: admitted query succeeded past ordinary error"
                                    )
                                });
                                assert_eq!(
                                    error.tape_before_error_snapshot(),
                                    expected_error,
                                    "{name}"
                                );
                                backend.released();
                            } else {
                                let mut backend = Backend::<false>::new();
                                let actual = if after_preserve {
                                    tape.policy_after_preserve_admitted(
                                        samples.clone(),
                                        Default::default(),
                                        &mut backend,
                                    )
                                } else {
                                    tape.policy_admitted(
                                        samples.clone(),
                                        Default::default(),
                                        &mut backend,
                                    )
                                };
                                let error = actual.err().unwrap_or_else(|| {
                                    panic!(
                                        "Tape {name}: unmetered query succeeded past ordinary error"
                                    )
                                });
                                assert_eq!(
                                    error.tape_before_error_snapshot(),
                                    expected_error,
                                    "{name}"
                                );
                                assert!(backend.0.borrow().events.is_empty());
                                backend.released();
                            }
                        }
                    }
                }
                cases += 1;
            }
        }
    });
    assert!(cases >= 8);
}

#[test]
fn span_cap_is_per_provider_and_tape_inventory_aggregates_multiple_contents() {
    let mut found = false;
    crate::audio_tape::policy_before::visit_cases(|plan, name, tape| {
        if found {
            return;
        }
        let samples = full_tape(plan, tape);
        let limits = AudioQueryLimits {
            maximum_spans: 1,
            ..Default::default()
        };
        let ordinary = match tape.policy(samples.clone(), limits) {
            Ok(value) => value,
            Err(_) => return,
        };
        if ordinary.contents.len() <= 1 {
            return;
        }
        let expected = ordinary_value(&ordinary);
        let contents_len = ordinary.contents.len();
        drop(ordinary);
        let mut backend = Backend::<true>::new();
        let admitted = tape.policy_admitted(samples, limits, &mut backend);
        let Ok(admitted) = admitted else {
            panic!("Tape {name}: per-provider span cap rejected successful ordinary query")
        };
        assert_eq!(policy_value(admitted.view()), expected, "{name}");
        assert!(admitted.view().contents.len() > 1, "{name}");
        assert_eq!(contents_len, admitted.view().contents.len(), "{name}");
        drop(admitted);
        backend.released();
        found = true;
    });
    assert!(
        found,
        "fixture corpus must contain multi-content Tape output"
    );
}

#[test]
fn exact_final_tape_vec_is_transferred_while_inventory_stays_live() {
    let mut covered = 0;
    crate::audio_tape::policy_before::visit_cases(|plan, name, tape| {
        let samples = full_tape(plan, tape);
        let ordinary = match tape.policy(samples.clone(), Default::default()) {
            Ok(value) => value,
            Err(_) => return,
        };
        let expected = ordinary_value(&ordinary);
        drop(ordinary);
        let mut backend = Backend::<true>::new();
        let query = tape
            .policy_admitted(samples, Default::default(), &mut backend)
            .unwrap_or_else(|error| panic!("Tape {name}: admitted query failed: {error:?}"));
        assert_eq!(policy_value(query.view()), expected, "{name}");
        let pointer = query.view().suppressed.as_ptr() as usize;
        let len = query.view().suppressed.len();
        let capacity = query.view().suppressed.len();
        let source_id = query.mask.memory.lease.as_ref().map(|lease| lease.id);
        if let Some(source_id) = source_id {
            let result = query.into_suppression(&mut backend).unwrap();
            assert_eq!(
                serde_json::to_value(&result.ranges).unwrap(),
                expected["suppressed"],
                "{name}"
            );
            let observation = backend.0.borrow().commit.unwrap();
            assert_eq!(observation.pointer, pointer, "{name}");
            assert_eq!(observation.len, len, "{name}");
            assert_eq!(observation.capacity, capacity, "{name}");
            assert_eq!(observation.capacity, result.ranges.capacity(), "{name}");
            assert_eq!(result.ranges.as_ptr() as usize, pointer, "{name}");
            assert_eq!(
                observation.capacity, observation.prepared_capacity,
                "{name}"
            );
            assert!(
                observation.live_at_prepare > observation.source_bytes,
                "inventory must remain charged: {name}"
            );
            assert_eq!(
                observation.live_at_commit, observation.live_at_prepare,
                "{name}"
            );
            assert_eq!(result.lease.as_ref().unwrap().id, source_id, "{name}");
            assert!(
                !backend.0.borrow().events.iter().any(|event| matches!(
                    event,
                    Event::Merge { target, source }
                        if *target == source_id || *source == source_id
                )),
                "final mask lease remains independent: {name}"
            );
            assert_eq!(
                backend.0.borrow().live,
                observation.source_bytes,
                "only transferred mask remains: {name}"
            );
            assert!(matches!(
                backend.0.borrow().events[observation.events_at_commit - 1],
                Event::Prepare
            ));
            drop(result);
            backend.released();
            covered += 1;
        } else {
            drop(query);
            backend.released();
        }
    });
    assert!(
        covered > 0,
        "checked Tape corpus must contain a positive final mask"
    );
}

#[test]
fn wrong_issuer_unmetered_and_prepare_refusal_keep_tape_error_and_cleanup() {
    let mut positive = 0;
    crate::audio_tape::policy_before::visit_cases(|plan, name, tape| {
        let samples = full_tape(plan, tape);
        let mut source = Backend::<true>::new();
        let ordinary = match tape.policy(samples.clone(), Default::default()) {
            Ok(value) => value,
            Err(_) => return,
        };
        drop(ordinary);
        let query = tape
            .policy_admitted(samples.clone(), Default::default(), &mut source)
            .unwrap_or_else(|error| panic!("Tape {name}: admitted query failed: {error:?}"));
        if query.mask.ranges.is_empty() {
            drop(query);
            source.released();
            return;
        }
        positive += 1;

        let mut other = Backend::<true>::new();
        assert!(
            matches!(
                query.into_suppression(&mut other),
                Err(PlanError::AudioMetadataIssuerMismatch)
            ),
            "{name}"
        );
        assert_eq!(other.0.borrow().prepare_calls, 0, "{name}");
        assert_eq!(other.0.borrow().commit_calls, 0, "{name}");
        source.released();
        other.released();

        let mut unmetered = Backend::<false>::new();
        let ordinary = tape.policy(samples.clone(), Default::default()).unwrap();
        drop(ordinary);
        let query = tape
            .policy_admitted(samples.clone(), Default::default(), &mut unmetered)
            .unwrap_or_else(|error| panic!("Tape {name}: unmetered query failed: {error:?}"));
        assert!(
            matches!(
                query.into_suppression(&mut unmetered),
                Err(PlanError::InvalidPlan("unmetered Tape policy mask handoff"))
            ),
            "{name}"
        );
        assert_eq!(unmetered.0.borrow().prepare_calls, 0, "{name}");
        assert_eq!(unmetered.0.borrow().commit_calls, 0, "{name}");
        unmetered.released();

        let mut refusing = Backend::<true>::new();
        let ordinary = tape.policy(samples.clone(), Default::default()).unwrap();
        drop(ordinary);
        let query = tape
            .policy_admitted(samples, Default::default(), &mut refusing)
            .unwrap_or_else(|error| panic!("Tape {name}: admitted query failed: {error:?}"));
        let accepted = refusing.0.borrow().accepted_work;
        refusing.0.borrow_mut().refuse_prepare = true;
        assert!(
            matches!(
                query.into_suppression(&mut refusing),
                Err(PlanError::AudioQueryLimit(
                    "Point policy test prepare refusal"
                ))
            ),
            "{name}"
        );
        assert_eq!(refusing.0.borrow().prepare_calls, 1, "{name}");
        assert_eq!(refusing.0.borrow().commit_calls, 0, "{name}");
        assert_eq!(refusing.0.borrow().accepted_work, accepted, "{name}");
        refusing.released();
    });
    assert!(positive > 0);
}
