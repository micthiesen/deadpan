//! Signal facade custody through the existing Plan protocol backend, never native ownership.
use super::*;
use crate::AudioSignal;

fn signal_query<const M: bool>(
    signal: &AudioSignal<'_>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    after_preserve: bool,
    backend: &mut Backend<M>,
) -> Result<AdmittedSignalPolicyQuery<usize, TestLease>, PlanError> {
    if after_preserve {
        signal.policy_after_preserve_admitted(samples, limits, backend)
    } else {
        signal.policy_admitted(samples, limits, backend)
    }
}

fn signal_admission_error(value: &PlanError) -> Value {
    match value {
        PlanError::AudioMetadataIssuerMismatch => json!({"variant":"AudioMetadataIssuerMismatch"}),
        other => other.tape_before_error_snapshot(),
    }
}
fn result_snapshot(
    result: &Result<AdmittedSignalPolicyQuery<usize, TestLease>, PlanError>,
) -> Value {
    match result {
        Ok(value) => super::super::provider_child_probe::snapshot(value),
        Err(error) => json!({"result":"Error","error":signal_admission_error(error),
            "error_local_canonical_additional":"unexposed by public Signal constructor"}),
    }
}

impl AudioSignal<'_> {
    pub(in crate::plan) fn signal_policy_unmetered_test_receipt(
        &self,
        samples: Range<SignalSample>,
        limits: AudioQueryLimits,
        after_preserve: bool,
    ) -> Value {
        let mut backend = Backend::<false>::new();
        let result = signal_query(self, samples, limits, after_preserve, &mut backend);
        let snapshot = result_snapshot(&result);
        if let Ok(query) = &result {
            assert!(matches!(query.origin, PolicyOrigin::Signal));
            assert!(!query.was_forwarded());
            assert_eq!(query.additional_units(), 0);
            assert_eq!(query.work(), query.canonical_units());
        }
        assert!(backend.0.borrow().events.is_empty());
        assert_eq!(backend.0.borrow().accepted_work, 0);
        drop(result);
        backend.released();
        json!({"returned":snapshot,"accepted_work":0,"callback_events":[],
            "backend":"existing unmetered Plan protocol Backend<false>"})
    }

    pub(in crate::plan) fn signal_policy_custody_test_receipt(
        &self,
        samples: Range<SignalSample>,
        after_preserve: bool,
    ) -> Value {
        let mut backend = Backend::<true>::new();
        let query = signal_query(
            self,
            samples.clone(),
            Default::default(),
            after_preserve,
            &mut backend,
        );
        let source_snapshot = result_snapshot(&query);
        let query = match query {
            Ok(query) => query,
            Err(_) => {
                backend.released();
                return json!({"source":source_snapshot,"handoff":"NotRun","reason":"actual public Signal policy rejected"});
            }
        };
        assert!(matches!(query.origin, PolicyOrigin::Signal));
        let pointer = query.view().suppressed.as_ptr();
        let len = query.mask.ranges.len();
        let capacity = query.mask.ranges.capacity();
        let source_id = query.mask.memory.lease.as_ref().map(|lease| lease.id);
        let source_bytes = query
            .mask
            .memory
            .lease
            .as_ref()
            .map_or(0, |lease| lease.bytes);
        let inventory_nonempty = !query.view().contents.is_empty();
        let accepted = backend.0.borrow().accepted_work;
        let mask = query.into_suppression(&mut backend).unwrap();
        assert_eq!(mask.ranges.as_ptr(), pointer);
        assert_eq!(mask.ranges.len(), len);
        assert_eq!(mask.ranges.capacity(), capacity);
        assert_eq!(
            serde_json::to_value(&mask.ranges).unwrap(),
            source_snapshot["query"]["suppressed"]
        );
        let observed = backend.0.borrow().commit.unwrap();
        assert_eq!(observed.pointer, pointer as usize);
        assert_eq!(observed.len, len);
        assert_eq!(observed.capacity, capacity);
        assert_eq!(observed.prepared_capacity, capacity);
        assert_eq!(observed.source_id, source_id);
        assert_eq!(observed.source_bytes, source_bytes);
        assert_eq!(observed.live_at_prepare, observed.live_at_commit);
        if inventory_nonempty {
            assert!(observed.live_at_prepare > source_bytes);
        }
        assert_eq!(mask.lease.as_ref().map(|lease| lease.id), source_id);
        assert_eq!(backend.0.borrow().live, source_bytes);
        assert_eq!(backend.0.borrow().accepted_work, accepted);
        assert_eq!(backend.0.borrow().prepare_calls, 1);
        assert_eq!(backend.0.borrow().commit_calls, 1);
        assert!(matches!(
            backend.0.borrow().events[observed.events_at_commit - 1],
            Event::Prepare
        ));
        if let Some(id) = source_id {
            assert!(
                !backend.0.borrow().events.iter().any(|event| matches!(event,
                Event::Merge { target, source } if *target == id || *source == id))
            );
        } else {
            assert_eq!(capacity, 0);
            assert_eq!(source_bytes, 0);
            assert!(mask.lease.is_none());
        }
        let ledger = Rc::clone(&backend.0);
        let custody = json!({"source":source_snapshot,"handoff":"Committed","len":len,"capacity":capacity,
            "source_bytes":source_bytes,"source_lease_id":source_id,"pointer_identity":true,
            "live_at_prepare":observed.live_at_prepare,"live_at_commit":observed.live_at_commit,
            "events_at_commit":observed.events_at_commit,"accepted_work":accepted,
            "prepare_calls":1,"commit_calls":1,"remaining_mask_lease_bytes":ledger.borrow().live});
        drop(backend);
        assert_eq!(
            serde_json::to_value(&mask.ranges).unwrap(),
            custody["source"]["query"]["suppressed"]
        );
        assert_eq!(ledger.borrow().live, source_bytes);
        drop(mask);
        assert_eq!(ledger.borrow().live, 0);
        assert_eq!(ledger.borrow().accepted_work, accepted);

        let mut source = Backend::<true>::new();
        let query = signal_query(
            self,
            samples.clone(),
            Default::default(),
            after_preserve,
            &mut source,
        )
        .unwrap();
        let mut foreign = Backend::<true>::new();
        let foreign_result = query.into_suppression(&mut foreign);
        assert!(matches!(
            &foreign_result,
            Err(PlanError::AudioMetadataIssuerMismatch)
        ));
        let foreign_error = signal_admission_error(&foreign_result.err().unwrap());
        assert_eq!(foreign.0.borrow().prepare_calls, 0);
        assert_eq!(foreign.0.borrow().commit_calls, 0);
        source.released();
        foreign.released();

        let mut unmetered = Backend::<false>::new();
        let query = signal_query(
            self,
            samples.clone(),
            Default::default(),
            after_preserve,
            &mut unmetered,
        )
        .unwrap();
        let unmetered_result = query.into_suppression(&mut unmetered);
        assert!(matches!(
            &unmetered_result,
            Err(PlanError::InvalidPlan(
                "unmetered Signal policy mask handoff"
            ))
        ));
        let unmetered_error = signal_admission_error(&unmetered_result.err().unwrap());
        assert_eq!(unmetered.0.borrow().prepare_calls, 0);
        assert_eq!(unmetered.0.borrow().commit_calls, 0);
        unmetered.released();

        let mut refusing = Backend::<true>::new();
        let query = signal_query(
            self,
            samples,
            Default::default(),
            after_preserve,
            &mut refusing,
        )
        .unwrap();
        let accepted = refusing.0.borrow().accepted_work;
        refusing.0.borrow_mut().refuse_prepare = true;
        let refusing_result = query.into_suppression(&mut refusing);
        assert!(matches!(
            &refusing_result,
            Err(PlanError::AudioQueryLimit(
                "Point policy test prepare refusal"
            ))
        ));
        let prepare_error = signal_admission_error(&refusing_result.err().unwrap());
        assert_eq!(refusing.0.borrow().prepare_calls, 1);
        assert_eq!(refusing.0.borrow().commit_calls, 0);
        assert_eq!(refusing.0.borrow().accepted_work, accepted);
        refusing.released();
        json!({"custody":custody,"after_mask_drop_live":ledger.borrow().live,
            "foreign_issuer":{"error":foreign_error,"source_live_after_cleanup":source.0.borrow().live,
                "destination_live":foreign.0.borrow().live,"prepare_calls":foreign.0.borrow().prepare_calls,
                "commit_calls":foreign.0.borrow().commit_calls},
            "unmetered_handoff":{"error":unmetered_error,"live_after_cleanup":unmetered.0.borrow().live,
                "prepare_calls":unmetered.0.borrow().prepare_calls,"commit_calls":unmetered.0.borrow().commit_calls},
            "prepare_refusal":{"error":prepare_error,"live_after_cleanup":refusing.0.borrow().live,
                "accepted_work":refusing.0.borrow().accepted_work,"prepare_calls":refusing.0.borrow().prepare_calls,
                "commit_calls":refusing.0.borrow().commit_calls},
            "scope":"existing Plan protocol ledger, no native Account or Storage Box"})
    }
}
