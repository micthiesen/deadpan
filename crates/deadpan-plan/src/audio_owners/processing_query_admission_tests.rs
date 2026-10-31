//! Frozen logical output and actual requested-Layout accounting are independent.
use super::*;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::capture_baseline as capture;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::processing_query_baseline as processing;
use crate::{AdmittedAudioProcessingQuery, AudioProcessingSpan, AudioSignalSpan};

#[path = "processing_query_identity_tests.rs"]
mod source_identity;

fn processing_snapshot() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/processing-query-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn processing_window(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}
fn processing_request(observation: &Value) -> Range<AudioSample> {
    processing_window(&observation["request"]["samples"])
}
fn processing_limits(observation: &Value) -> AudioQueryLimits {
    let limits = &observation["request"]["limits"];
    AudioQueryLimits {
        maximum_spans: usize::try_from(limits["maximum_spans"].as_u64().unwrap()).unwrap(),
        maximum_work: usize::try_from(limits["maximum_work"].as_u64().unwrap()).unwrap(),
    }
}
fn processing_admitted<'plan, A: AudioSoundContextAdmission>(
    parent: Parent<'_, 'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    policy: &mut A,
) -> Result<AdmittedAudioProcessingQuery<'plan, A::Issuer, A::Lease>, A::Error> {
    match parent {
        Parent::Root(plan) => plan.audio_processing_admitted(samples, limits, policy),
        Parent::Domain(domain) => domain.processing_admitted(samples, limits, policy),
    }
}
fn processing_visit(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let frozen = processing_snapshot();
    let mut count = 0;
    processing::visit_cases(|parent, descriptor| {
        let expected = frozen["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        for (key, value) in descriptor.as_object().unwrap() {
            assert_eq!(&expected[key], value, "processing parent provenance {key}");
        }
        visit(parent, expected);
        count += 1;
    });
    assert_eq!(count, 33);
}
fn processing_defaults(case: &Value) -> impl Iterator<Item = &Value> {
    case["observations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|observation| {
            matches!(
                observation["name"].as_str().unwrap(),
                "default_full" | "default_clipped" | "default_empty"
            )
        })
}
fn processing_full(case: &Value) -> &Value {
    case["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|observation| observation["name"] == "default_full")
        .unwrap()
}
fn processing_error(error: PlanError) -> Value {
    match error {
        PlanError::InvalidAudioLimits => serde_json::json!({"variant":"InvalidAudioLimits"}),
        PlanError::AudioRangeOutOfRange => serde_json::json!({"variant":"AudioRangeOutOfRange"}),
        PlanError::AudioQueryLimit(budget) => {
            serde_json::json!({"variant":"AudioQueryLimit","budget":budget})
        }
        PlanError::Document(error) => {
            serde_json::json!({"variant":"Document","code":error.code,"message":error.message})
        }
        error => panic!("unclassified frozen processing error: {error}"),
    }
}

fn assert_processing_metered(
    value: &AdmittedAudioProcessingQuery<'_, usize, Lease>,
    old: &Value,
    parent: Parent<'_, '_>,
    policy: &Policy<true>,
    events: &[Value],
    case: &str,
) -> usize {
    let observed = processing::query_snapshot(parent.plan(), value.query());
    capture::assert_processing_with_paired_core_work(
        &observed,
        value.canonical_units(),
        old,
        events,
        case,
    );
    assert_eq!(
        value.canonical_units(),
        usize::try_from(old["work"].as_u64().unwrap()).unwrap()
    );
    assert_eq!(
        value
            .canonical_units()
            .checked_add(value.additional_units()),
        Some(value.work())
    );
    assert_eq!(value.query().work, value.work());
    assert!(value.was_forwarded());
    assert_eq!(policy.0.borrow().forwarded, value.work());
    assert!(value.additional_units() >= policy.0.borrow().reservations);
    assert!(policy.0.borrow().live > 0);
    observed["spans"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|span| span["content"]["tag"] == "Bound")
        .count()
}

#[test]
fn processing_ordinary_and_false_admission_match_all_446_frozen_observations() {
    assert_eq!(processing::matrix(), processing_snapshot());
    let mut count = 0;
    processing_visit(|parent, case| {
        for observation in case["observations"].as_array().unwrap() {
            let mut policy = Policy::<false>::new(0);
            match processing_admitted(
                parent,
                processing_request(observation),
                processing_limits(observation),
                &mut policy,
            ) {
                Ok(value) => {
                    assert_eq!(observation["output"]["result"], "Ok");
                    let old = &observation["output"]["query"];
                    assert_eq!(
                        processing::query_snapshot(parent.plan(), value.query()),
                        *old
                    );
                    assert_eq!(value.work(), old["work"].as_u64().unwrap() as usize);
                    assert_eq!(value.canonical_units(), value.work());
                    assert_eq!(value.additional_units(), 0);
                    assert!(!value.was_forwarded());
                    drop(value);
                }
                Err(error) => {
                    assert_eq!(observation["output"]["result"], "Error");
                    assert_eq!(processing_error(error), observation["output"]["error"]);
                }
            }
            let state = policy.0.borrow();
            assert_eq!(
                (
                    state.work_calls,
                    state.forwarded,
                    state.reservations,
                    state.live
                ),
                (0, 0, 0, 0)
            );
            count += 1;
        }
    });
    assert_eq!(count, 446);
}

#[test]
fn processing_metered_full_crop_empty_match_frozen_metadata_and_paired_core_work() {
    let mut count = 0;
    let mut complete = 0;
    let mut paired_bindings = 0;
    processing_visit(|parent, case| {
        for observation in processing_defaults(case) {
            let mut policy = Policy::<true>::new(usize::MAX);
            let name = case["name"].as_str().unwrap();
            let (value, events) = capture::observe_case(name, "admitted_processing_query", || {
                processing_admitted(
                    parent,
                    processing_request(observation),
                    processing_limits(observation),
                    &mut policy,
                )
            });
            let value = value.unwrap();
            let nested = assert_processing_metered(
                &value,
                &observation["output"]["query"],
                parent,
                &policy,
                &events,
                name,
            );
            if nested == 0 {
                complete += 1;
            }
            paired_bindings += nested;
            let total = value.work();
            let ledger = Rc::clone(&policy.0);
            drop(policy);
            assert!(ledger.borrow().live > 0);
            drop(value);
            assert_eq!(ledger.borrow().live, 0);
            assert_eq!(ledger.borrow().forwarded, total);
            count += 1;
        }
    });
    assert_eq!(count, 99);
    assert_eq!(complete, 74);
    assert_eq!(paired_bindings, 29);
}

#[test]
fn processing_each_parent_fits_exact_new_total_work_and_requested_layout_peak() {
    processing_visit(|parent, case| {
        let request = processing_request(processing_full(case));
        let mut baseline = Policy::<true>::new(usize::MAX);
        let value = processing_admitted(parent, request.clone(), Default::default(), &mut baseline)
            .unwrap();
        let total = value.work();
        let peak = baseline.0.borrow().peak;
        assert!(peak >= baseline.0.borrow().live && baseline.0.borrow().live > 0);
        drop(value);
        assert_eq!(baseline.0.borrow().live, 0);
        let mut exact = Policy::<true>::new(peak);
        let value = processing_admitted(
            parent,
            request.clone(),
            AudioQueryLimits {
                maximum_work: total,
                ..Default::default()
            },
            &mut exact,
        )
        .unwrap();
        assert_eq!(value.work(), total);
        assert_eq!(exact.0.borrow().peak, peak);
        drop(value);
        assert_eq!(exact.0.borrow().live, 0);
        assert_eq!(exact.0.borrow().forwarded, total);
        let mut short_work = Policy::<true>::new(usize::MAX);
        match processing_admitted(
            parent,
            request.clone(),
            AudioQueryLimits {
                maximum_work: total - 1,
                ..Default::default()
            },
            &mut short_work,
        ) {
            Err(PlanError::AudioQueryLimit(_)) => {}
            Err(PlanError::Document(error)) => {
                assert_eq!(error.code, deadpan_core::DocumentErrorCode::LimitExceeded);
            }
            Err(error) => panic!("unrelated one-short processing error: {error}"),
            Ok(_) => panic!("one-short total work produced a complete query"),
        }
        assert_eq!(short_work.0.borrow().live, 0);
        assert!(short_work.0.borrow().forwarded < total);
        let mut short_bytes = Policy::<true>::new(peak - 1);
        assert!(matches!(
            processing_admitted(parent, request, Default::default(), &mut short_bytes),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(short_bytes.0.borrow().live, 0);
        assert!(short_bytes.0.borrow().forwarded > 0);
    });
}

#[test]
fn processing_scalar_validation_precedes_any_admission_callback() {
    processing_visit(|parent, case| {
        for observation in case["observations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|observation| {
                matches!(
                    observation["output"]["error"]["variant"].as_str(),
                    Some("InvalidAudioLimits" | "AudioRangeOutOfRange")
                )
            })
        {
            let mut policy = Policy::<true>::new(0);
            let result = processing_admitted(
                parent,
                processing_request(observation),
                processing_limits(observation),
                &mut policy,
            );
            match result {
                Err(error) => assert_eq!(processing_error(error), observation["output"]["error"]),
                Ok(_) => panic!("invalid processing request succeeded"),
            }
            let state = policy.0.borrow();
            assert_eq!(
                (
                    state.work_calls,
                    state.forwarded,
                    state.reservations,
                    state.live
                ),
                (0, 0, 0, 0)
            );
        }
    });
}

#[derive(Debug)]
struct ProcessingLayoutEvent {
    size: usize,
    align: usize,
    before: usize,
    after: usize,
    work: usize,
}
struct ProcessingObserver<'a> {
    policy: &'a mut Policy<true>,
    layouts: Vec<ProcessingLayoutEvent>,
    spends: Vec<usize>,
}
impl AudioSoundContextAdmission for ProcessingObserver<'_> {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.policy.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.policy.spend(units)?;
        self.spends.push(units);
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let before = self.policy.0.borrow().live;
        let lease = self.policy.reserve(layout)?;
        let state = self.policy.0.borrow();
        self.layouts.push(ProcessingLayoutEvent {
            size: layout.size(),
            align: layout.align(),
            before,
            after: state.live,
            work: state.forwarded,
        });
        Ok(lease)
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.policy.merge(target, source)
    }
}
fn processing_focused(case: &Value) -> bool {
    matches!(
        case["name"].as_str().unwrap(),
        "wide/stable_repeat_plays_default_gap_and_overrides/root"
            | "additional/exposed_preserve_and_silent_hold/root"
            | "wide/actual_source_split_with_preexisting_lattice_retained_root_round_even_clipped_window/root"
            | "wide/split_actual_physical_capture_domain/domain"
    )
}

#[test]
fn processing_every_focused_callback_refusal_and_cancellation_keeps_exact_accepted_prefix() {
    let mut checked = 0;
    processing_visit(|parent, case| {
        if !processing_focused(case) {
            return;
        }
        let request = processing_request(processing_full(case));
        let mut policy = Policy::<true>::new(usize::MAX);
        let mut observer = ProcessingObserver {
            policy: &mut policy,
            layouts: Vec::new(),
            spends: Vec::new(),
        };
        let value = processing_admitted(parent, request.clone(), Default::default(), &mut observer)
            .unwrap();
        let total = value.work();
        let spends = observer.spends.clone();
        let reservation_work: Vec<_> = observer.layouts.iter().map(|event| event.work).collect();
        assert_eq!(spends.iter().sum::<usize>(), total);
        drop(value);
        assert_eq!(observer.policy.0.borrow().live, 0);
        for (index, work) in reservation_work.into_iter().enumerate() {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_reservation = Some(index + 1);
            assert!(matches!(
                processing_admitted(parent, request.clone(), Default::default(), &mut refusing),
                Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
            ));
            let state = refusing.0.borrow();
            assert_eq!(state.reservations, index + 1);
            assert_eq!(state.live, 0);
            assert_eq!(state.forwarded, work);
        }
        let mut prefix = 0;
        let mut cancellation_prefixes = std::collections::BTreeSet::new();
        for (index, units) in spends.iter().enumerate() {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_work_call = Some(index + 1);
            assert!(matches!(
                processing_admitted(parent, request.clone(), Default::default(), &mut refusing),
                Err(PlanError::AudioQueryLimit("test wide work refusal"))
            ));
            let state = refusing.0.borrow();
            assert_eq!(state.work_calls, index + 1);
            assert_eq!(state.live, 0);
            assert_eq!(state.forwarded, prefix);
            cancellation_prefixes.insert(prefix);
            prefix += units;
        }
        assert_eq!(prefix, total);
        for prefix in cancellation_prefixes {
            let mut cancelled = Policy::<true>::new(usize::MAX);
            cancelled.0.borrow_mut().cancel_after = Some(prefix);
            assert!(matches!(
                processing_admitted(parent, request.clone(), Default::default(), &mut cancelled),
                Err(PlanError::AudioQueryLimit("test wide cancellation"))
            ));
            assert_eq!(cancelled.0.borrow().live, 0);
            assert_eq!(cancelled.0.borrow().forwarded, prefix);
        }
        checked += 1;
    });
    assert_eq!(checked, 4);
}

#[test]
fn processing_focused_issuer_rotation_releases_both_ledgers_at_every_callback() {
    let mut checked = 0;
    processing_visit(|parent, case| {
        if !processing_focused(case) {
            return;
        }
        let request = processing_request(processing_full(case));
        let mut policy = Policy::<true>::new(usize::MAX);
        let mut observer = ProcessingObserver {
            policy: &mut policy,
            layouts: Vec::new(),
            spends: Vec::new(),
        };
        let value = processing_admitted(parent, request.clone(), Default::default(), &mut observer)
            .unwrap();
        let spends = observer.spends.clone();
        let reservation_work: Vec<_> = observer.layouts.iter().map(|event| event.work).collect();
        drop(value);
        let mut prefix = 0;
        for (reservation, accepted_work) in [
            (
                false,
                spends
                    .iter()
                    .map(|units| {
                        prefix += units;
                        prefix
                    })
                    .collect::<Vec<_>>(),
            ),
            (true, reservation_work),
        ] {
            for (index, work) in accepted_work.into_iter().enumerate() {
                let mut rotating = Policy::<true>::new(usize::MAX);
                let old = Rc::clone(&rotating.0);
                if reservation {
                    rotating.0.borrow_mut().rotate_reservation = Some(index + 1);
                } else {
                    rotating.0.borrow_mut().rotate_work_call = Some(index + 1);
                }
                assert!(matches!(
                    processing_admitted(parent, request.clone(), Default::default(), &mut rotating),
                    Err(PlanError::AudioMetadataIssuerMismatch)
                ));
                assert!(!Rc::ptr_eq(&old, &rotating.0));
                assert_eq!(old.borrow().forwarded, work);
                assert_eq!(old.borrow().live, 0);
                assert_eq!(rotating.0.borrow().live, 0);
                assert_eq!(rotating.0.borrow().forwarded, 0);
            }
        }
        checked += 1;
    });
    assert_eq!(checked, 4);
}

fn processing_identity_layout(text: &str) -> Layout {
    Layout::new::<[usize; 2]>()
        .extend(Layout::for_value(text))
        .unwrap()
        .0
        .pad_to_align()
}

#[test]
fn processing_empty_queries_charge_only_independent_project_and_revision_layouts() {
    processing_visit(|parent, case| {
        let at = processing_window(&case["window"]).start;
        let expected = processing_identity_layout(parent.plan().metadata.project_id.as_str())
            .size()
            + processing_identity_layout(parent.plan().metadata.revision_id.as_str()).size();
        let mut policy = Policy::<true>::new(expected);
        let value = processing_admitted(parent, at..at, Default::default(), &mut policy).unwrap();
        assert!(value.query().spans.is_empty());
        assert_eq!(value.query().spans.capacity(), 0);
        assert_eq!(value.canonical_units(), 0);
        assert_eq!(policy.0.borrow().reservations, 2);
        assert_eq!(policy.0.borrow().live, expected);
        assert_eq!(policy.0.borrow().peak, expected);
        assert_eq!(
            value.additional_units(),
            if matches!(parent, Parent::Root(_)) {
                5
            } else {
                4
            }
        );
        drop(value);
        assert_eq!(policy.0.borrow().live, 0);
        let mut short = Policy::<true>::new(expected - 1);
        assert!(matches!(
            processing_admitted(parent, at..at, Default::default(), &mut short),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(short.0.borrow().live, 0);
    });
}

#[test]
fn processing_root_input_and_output_buffers_overlap_and_outer_growth_keeps_prior_spans_live() {
    let mut checked = false;
    processing_visit(|parent, case| {
        if case["name"] != "wide/stable_repeat_plays_default_gap_and_overrides/root" {
            return;
        }
        let mut policy = Policy::<true>::new(usize::MAX);
        let mut observer = ProcessingObserver {
            policy: &mut policy,
            layouts: Vec::new(),
            spends: Vec::new(),
        };
        let value = processing_admitted(
            parent,
            processing_request(processing_full(case)),
            Default::default(),
            &mut observer,
        )
        .unwrap();
        assert_eq!(value.query().spans.len(), 5);
        assert_eq!(value.query().spans.capacity(), 5);
        let spans: Vec<_> = value
            .query()
            .spans
            .iter()
            .map(|span| crate::audio_allocated::measure(span, usize::MAX).unwrap().0)
            .collect();
        let input_unit = std::mem::size_of::<AudioSignalSpan<'_>>();
        let input_align = std::mem::align_of::<AudioSignalSpan<'_>>();
        let output = Layout::array::<AudioProcessingSpan<'_>>(5).unwrap();
        let final_event = observer.layouts.last().unwrap();
        assert_eq!(
            (final_event.size, final_event.align),
            (output.size(), output.align())
        );
        assert_eq!(final_event.after, final_event.before + output.size());
        let retained = observer.policy.0.borrow().live;
        assert_eq!(
            final_event.after - retained,
            8 * input_unit,
            "the exhausted input buffer alone releases after output allocation"
        );
        let headers = processing_identity_layout(parent.plan().metadata.project_id.as_str()).size()
            + processing_identity_layout(parent.plan().metadata.revision_id.as_str()).size();
        assert!(final_event.before >= 8 * input_unit + headers + spans.iter().sum::<usize>());
        let mut events = observer.layouts.iter();
        for (current, old_cap, new_cap) in [(0, 0, 1), (1, 1, 2), (2, 2, 4), (4, 4, 8)] {
            let payload = spans[..=current].iter().sum::<usize>();
            let event = events
                .find(|event| {
                    event.size == new_cap * input_unit
                        && event.align == input_align
                        && event.before >= old_cap * input_unit + payload
                })
                .unwrap();
            assert_eq!(event.after, event.before + event.size);
        }
        assert!(
            observer.policy.0.borrow().peak >= final_event.after && final_event.after > retained
        );
        drop(value);
        assert_eq!(observer.policy.0.borrow().live, 0);
        checked = true;
    });
    assert!(checked);
}

#[test]
fn processing_retained_measurement_is_separately_billed_and_never_substitutes_for_peak() {
    processing_visit(|parent, case| {
        let mut policy = Policy::<true>::new(usize::MAX);
        let value = processing_admitted(
            parent,
            processing_request(processing_full(case)),
            Default::default(),
            &mut policy,
        )
        .unwrap();
        let total = value.work();
        let retained = policy.0.borrow().live;
        let (bytes, inspected) = value
            .query()
            .measure_allocated_storage(usize::MAX)
            .into_parts();
        policy.spend(inspected).unwrap();
        assert!(bytes.unwrap() <= retained && inspected > 0);
        assert_eq!(policy.0.borrow().live, retained);
        assert_eq!(value.work(), total);
        assert_eq!(policy.0.borrow().forwarded, total + inspected);
        let (failed, partial) = value.query().measure_allocated_storage(1).into_parts();
        policy.spend(partial).unwrap();
        assert!(matches!(
            failed,
            Err(PlanError::AudioQueryLimit("retained audio heap traversal"))
        ));
        assert_eq!(partial, 1);
        drop(value);
        assert_eq!(policy.0.borrow().live, 0);
        assert_eq!(policy.0.borrow().forwarded, total + inspected + partial);
    });
}

#[test]
fn processing_live_carrier_aggregate_room_is_reused_only_after_drop() {
    let mut checked = false;
    processing_visit(|parent, case| {
        if case["name"] != "wide/stable_repeat_plays_default_gap_and_overrides/root" {
            return;
        }
        let request = processing_request(processing_full(case));
        let mut measure = Policy::<true>::new(usize::MAX);
        let first =
            processing_admitted(parent, request.clone(), Default::default(), &mut measure).unwrap();
        let retained = measure.0.borrow().live;
        let peak = measure.0.borrow().peak;
        drop(first);
        let mut exact = Policy::<true>::new(retained + peak);
        let first =
            processing_admitted(parent, request.clone(), Default::default(), &mut exact).unwrap();
        let second =
            processing_admitted(parent, request.clone(), Default::default(), &mut exact).unwrap();
        assert_eq!(exact.0.borrow().peak, retained + peak);
        drop(second);
        assert_eq!(exact.0.borrow().live, retained);
        drop(first);
        assert_eq!(exact.0.borrow().live, 0);
        let mut short = Policy::<true>::new(retained + peak - 1);
        let first =
            processing_admitted(parent, request.clone(), Default::default(), &mut short).unwrap();
        assert!(matches!(
            processing_admitted(parent, request.clone(), Default::default(), &mut short),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(short.0.borrow().live, retained);
        assert_eq!(
            first.canonical_units(),
            processing_full(case)["output"]["query"]["work"]
                .as_u64()
                .unwrap() as usize
        );
        drop(first);
        assert_eq!(short.0.borrow().live, 0);
        let value = processing_admitted(parent, request, Default::default(), &mut short).unwrap();
        drop(value);
        assert_eq!(short.0.borrow().live, 0);
        checked = true;
    });
    assert!(checked);
}

#[test]
fn processing_positive_span_cap_refuses_after_prior_actual_spans_and_releases_them() {
    let mut checked = 0;
    processing_visit(|parent, case| {
        if !processing_focused(case) {
            return;
        }
        let full = processing_full(case);
        let count = full["output"]["query"]["spans"].as_array().unwrap().len();
        assert!(count > 1);
        let mut policy = Policy::<true>::new(usize::MAX);
        assert!(matches!(
            processing_admitted(
                parent,
                processing_request(full),
                AudioQueryLimits {
                    maximum_spans: count - 1,
                    ..Default::default()
                },
                &mut policy
            ),
            Err(PlanError::AudioQueryLimit("span count"))
        ));
        assert!(policy.0.borrow().forwarded > 0);
        assert!(policy.0.borrow().reservations > 0);
        assert_eq!(policy.0.borrow().live, 0);
        checked += 1;
    });
    assert_eq!(checked, 4);
}
