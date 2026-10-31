//! Allocation callbacks prove requested Layout charge, not allocator/RSS peaks.
//! The ordinary expected matrix is frozen OLD output and is never normalized.
use super::*;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::flattened_query_baseline as flat;
use crate::{AdmittedAudioQuery, AudioQuery, AudioSpan};

fn flat_snapshot() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/flattened-query-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn flat_window(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}
fn flat_limits(observation: &Value) -> AudioQueryLimits {
    let value = &observation["request"]["limits"];
    AudioQueryLimits {
        maximum_spans: usize::try_from(value["maximum_spans"].as_u64().unwrap()).unwrap(),
        maximum_work: usize::try_from(value["maximum_work"].as_u64().unwrap()).unwrap(),
    }
}
fn flat_request(observation: &Value) -> Range<AudioSample> {
    flat_window(&observation["request"]["samples"])
}
fn flat_ordinary(
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Result<AudioQuery, PlanError> {
    match parent {
        Parent::Root(plan) => plan.audio(samples, limits),
        Parent::Domain(domain) => domain.audio(samples, limits),
    }
}
fn flat_admitted<A: AudioSoundContextAdmission>(
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    policy: &mut A,
) -> Result<AdmittedAudioQuery<A::Issuer, A::Lease>, A::Error> {
    match parent {
        Parent::Root(plan) => plan.audio_admitted(samples, limits, policy),
        Parent::Domain(domain) => domain.audio_admitted(samples, limits, policy),
    }
}
fn flat_visit(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let frozen = flat_snapshot();
    let mut count = 0;
    flat::visit_cases(|parent, descriptor| {
        let expected = frozen["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        // Confirm checked Domain/Root identity and fixture provenance before
        // using any old observation associated with this parent.
        for (key, value) in descriptor.as_object().unwrap() {
            assert_eq!(&expected[key], value, "parent descriptor field {key}");
        }
        visit(parent, expected);
        count += 1;
    });
    assert_eq!(count, 6);
}
fn flat_assert_error(error: PlanError, expected: &Value) {
    match error {
        PlanError::InvalidAudioLimits => assert_eq!(
            expected,
            &serde_json::json!({
                "variant":"InvalidAudioLimits"
            })
        ),
        PlanError::AudioRangeOutOfRange => assert_eq!(
            expected,
            &serde_json::json!({
                "variant":"AudioRangeOutOfRange"
            })
        ),
        PlanError::AudioQueryLimit(label) => assert_eq!(
            expected,
            &serde_json::json!({
                "variant":"AudioQueryLimit", "budget":label
            })
        ),
        error => panic!("unclassified frozen flattened error: {error}"),
    }
}
fn flat_assert_query<const M: bool>(
    value: &AdmittedAudioQuery<usize, Lease>,
    old: &Value,
    policy: &Policy<M>,
) {
    let observed = flat::query_snapshot(value.query());
    assert_eq!(
        observed.as_object().unwrap().len(),
        old.as_object().unwrap().len()
    );
    for (key, expected) in old.as_object().unwrap() {
        if key == "work" {
            assert_eq!(observed[key].as_u64().unwrap(), value.work() as u64);
        } else {
            assert_eq!(&observed[key], expected, "flattened query field {key}");
        }
    }
    let canonical = usize::try_from(old["work"].as_u64().unwrap()).unwrap();
    assert_eq!(value.canonical_units(), canonical);
    assert_eq!(
        value
            .canonical_units()
            .checked_add(value.additional_units()),
        Some(value.work())
    );
    assert_eq!(value.query().work, value.work());
    assert_eq!(value.was_forwarded(), M);
    assert_eq!(
        policy.0.borrow().forwarded,
        if M { value.work() } else { 0 }
    );
    if M {
        assert!(value.additional_units() >= policy.0.borrow().reservations);
        assert!(policy.0.borrow().live > 0);
    } else {
        assert_eq!(observed, *old);
        assert_eq!(value.additional_units(), 0);
        assert_eq!(policy.0.borrow().reservations, 0);
        assert_eq!(policy.0.borrow().work_calls, 0);
    }
}
fn flat_defaults(case: &Value) -> impl Iterator<Item = &Value> {
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

#[test]
fn flattened_ordinary_and_unmetered_match_all_57_frozen_observations() {
    assert_eq!(flat::matrix(), flat_snapshot());
    let mut count = 0;
    flat_visit(|parent, case| {
        for observation in case["observations"].as_array().unwrap() {
            let mut policy = Policy::<false>::new(0);
            let result = flat_admitted(
                parent,
                flat_request(observation),
                flat_limits(observation),
                &mut policy,
            );
            match result {
                Ok(value) => {
                    assert_eq!(observation["output"]["result"], "Ok");
                    flat_assert_query(&value, &observation["output"]["query"], &policy);
                    drop(value);
                }
                Err(error) => {
                    assert_eq!(observation["output"]["result"], "Error");
                    flat_assert_error(error, &observation["output"]["error"]);
                }
            }
            assert_eq!(policy.0.borrow().forwarded, 0);
            assert_eq!(policy.0.borrow().reservations, 0);
            assert_eq!(policy.0.borrow().work_calls, 0);
            assert_eq!(policy.0.borrow().live, 0);
            count += 1;
        }
    });
    assert_eq!(count, 57);
}

#[test]
fn flattened_metered_full_crop_and_empty_preserve_every_old_field_and_canonical_work() {
    let mut count = 0;
    flat_visit(|parent, case| {
        for observation in flat_defaults(case) {
            let mut policy = Policy::<true>::new(usize::MAX);
            let value = flat_admitted(
                parent,
                flat_request(observation),
                flat_limits(observation),
                &mut policy,
            )
            .unwrap();
            flat_assert_query(&value, &observation["output"]["query"], &policy);
            let total = value.work();
            let accounting = Rc::clone(&policy.0);
            drop(policy);
            assert!(
                accounting.borrow().live > 0,
                "carrier outlives mutable policy"
            );
            drop(value);
            assert_eq!(accounting.borrow().live, 0);
            assert_eq!(accounting.borrow().forwarded, total, "work is not refunded");
            count += 1;
        }
    });
    assert_eq!(count, 18);
}

#[test]
fn flattened_actual_requested_layout_peak_fits_exactly_and_one_byte_short_refuses() {
    flat_visit(|parent, case| {
        for observation in flat_defaults(case) {
            let request = flat_request(observation);
            let limits = flat_limits(observation);
            let mut measure = Policy::<true>::new(usize::MAX);
            let value = flat_admitted(parent, request.clone(), limits, &mut measure).unwrap();
            let peak = measure.0.borrow().peak;
            let retained = measure.0.borrow().live;
            let total = value.work();
            assert!(peak >= retained && retained > 0);
            drop(value);
            assert_eq!(measure.0.borrow().live, 0);
            let mut exact = Policy::<true>::new(peak);
            let value = flat_admitted(parent, request.clone(), limits, &mut exact).unwrap();
            assert_eq!(value.work(), total);
            assert_eq!(exact.0.borrow().peak, peak);
            assert_eq!(exact.0.borrow().live, retained);
            drop(value);
            assert_eq!(exact.0.borrow().live, 0);
            assert_eq!(exact.0.borrow().forwarded, total);
            let mut short = Policy::<true>::new(peak - 1);
            assert!(matches!(
                flat_admitted(parent, request, limits, &mut short),
                Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
            ));
            assert!(short.0.borrow().peak < peak);
            assert!(short.0.borrow().forwarded > 0);
            assert!(short.0.borrow().forwarded <= total);
            assert_eq!(short.0.borrow().live, 0);
        }
    });
}

#[test]
fn flattened_exact_and_one_short_total_work_and_legacy_error_priority() {
    flat_visit(|parent, case| {
        for observation in flat_defaults(case) {
            let request = flat_request(observation);
            let mut measure = Policy::<true>::new(usize::MAX);
            let value =
                flat_admitted(parent, request.clone(), Default::default(), &mut measure).unwrap();
            let total = value.work();
            let canonical = value.canonical_units();
            drop(value);
            let mut exact = Policy::<true>::new(usize::MAX);
            let value = flat_admitted(
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
            assert_eq!(exact.0.borrow().forwarded, total);
            drop(value);
            let mut short = Policy::<true>::new(usize::MAX);
            assert!(matches!(
                flat_admitted(
                    parent,
                    request.clone(),
                    AudioQueryLimits {
                        maximum_work: total - 1,
                        ..Default::default()
                    },
                    &mut short
                ),
                Err(PlanError::AudioQueryLimit("structural work"))
            ));
            assert!(short.0.borrow().forwarded < total);
            assert_eq!(short.0.borrow().live, 0);
            // Empty ordinary queries use zero work, but zero is still an invalid
            // public limit; nonempty exact canonical behavior is frozen above.
            if canonical > 0 {
                let mut legacy = Policy::<false>::new(0);
                let value = flat_admitted(
                    parent,
                    request,
                    AudioQueryLimits {
                        maximum_work: canonical,
                        ..Default::default()
                    },
                    &mut legacy,
                )
                .unwrap();
                assert_eq!(value.work(), canonical);
            }
        }
        for observation in case["observations"].as_array().unwrap().iter().filter(|o| {
            matches!(
                o["name"].as_str().unwrap(),
                "invalid_zero_work"
                    | "invalid_zero_spans"
                    | "outside_support_before"
                    | "outside_support_after"
            )
        }) {
            let mut policy = Policy::<true>::new(0);
            let error = match flat_admitted(
                parent,
                flat_request(observation),
                flat_limits(observation),
                &mut policy,
            ) {
                Err(error) => error,
                Ok(_) => panic!("expected frozen validation failure"),
            };
            flat_assert_error(error, &observation["output"]["error"]);
            assert_eq!(policy.0.borrow().reservations, 0);
            assert_eq!(policy.0.borrow().work_calls, 0);
        }
        let bounds = flat_window(&case["window"]);
        let invalid = AudioQueryLimits {
            maximum_spans: 0,
            ..Default::default()
        };
        let mut policy = Policy::<true>::new(0);
        assert!(matches!(
            flat_admitted(
                parent,
                AudioSample(bounds.start.0 - 1)..bounds.start,
                invalid,
                &mut policy
            ),
            Err(PlanError::InvalidAudioLimits)
        ));
        assert!(matches!(
            flat_admitted(parent, bounds.end..bounds.start, invalid, &mut policy),
            Err(PlanError::InvalidAudioLimits)
        ));
        assert!(matches!(
            flat_admitted(
                parent,
                bounds.end..bounds.start,
                Default::default(),
                &mut policy
            ),
            Err(PlanError::AudioRangeOutOfRange)
        ));
        assert_eq!(policy.0.borrow().work_calls, 0);
        let full = &case["observations"][0];
        let spans = full["output"]["query"]["spans"].as_array().unwrap().len();
        if spans > 1 {
            let mut policy = Policy::<true>::new(usize::MAX);
            assert!(matches!(
                flat_admitted(
                    parent,
                    flat_request(full),
                    AudioQueryLimits {
                        maximum_spans: spans - 1,
                        ..Default::default()
                    },
                    &mut policy
                ),
                Err(PlanError::AudioQueryLimit("span count"))
            ));
            assert!(policy.0.borrow().forwarded > 0);
            assert_eq!(policy.0.borrow().live, 0);
        }
    });
}

#[test]
fn flattened_every_reservation_and_work_refusal_releases_partial_values_without_refunds() {
    flat_visit(|parent, case| {
        for observation in flat_defaults(case) {
            let request = flat_request(observation);
            let mut baseline = Policy::<true>::new(usize::MAX);
            let value =
                flat_admitted(parent, request.clone(), Default::default(), &mut baseline).unwrap();
            let reservations = baseline.0.borrow().reservations;
            let calls = baseline.0.borrow().work_calls;
            let total = value.work();
            drop(value);
            for index in 1..=reservations {
                let mut refusing = Policy::<true>::new(usize::MAX);
                refusing.0.borrow_mut().refuse_reservation = Some(index);
                assert!(matches!(
                    flat_admitted(parent, request.clone(), Default::default(), &mut refusing),
                    Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
                ));
                assert_eq!(refusing.0.borrow().reservations, index);
                assert!(refusing.0.borrow().forwarded > 0);
                assert!(refusing.0.borrow().forwarded <= total);
                assert_eq!(refusing.0.borrow().live, 0);
            }
            let mut previous = 0;
            let mut prefixes = std::collections::BTreeSet::new();
            for index in 1..=calls {
                let mut refusing = Policy::<true>::new(usize::MAX);
                refusing.0.borrow_mut().refuse_work_call = Some(index);
                assert!(matches!(
                    flat_admitted(parent, request.clone(), Default::default(), &mut refusing),
                    Err(PlanError::AudioQueryLimit("test wide work refusal"))
                ));
                let state = refusing.0.borrow();
                assert_eq!(state.work_calls, index);
                assert_eq!(state.live, 0);
                assert!(state.forwarded >= previous && state.forwarded <= total);
                if index == 1 {
                    assert_eq!(state.forwarded, 0);
                }
                prefixes.insert(state.forwarded);
                previous = state.forwarded;
            }
            // Cancel only at actually observed successful prefixes, including
            // zero. No full allowance or refunded debit is manufactured.
            for prefix in prefixes {
                let mut cancelled = Policy::<true>::new(usize::MAX);
                cancelled.0.borrow_mut().cancel_after = Some(prefix);
                assert!(matches!(
                    flat_admitted(parent, request.clone(), Default::default(), &mut cancelled),
                    Err(PlanError::AudioQueryLimit("test wide cancellation"))
                ));
                assert_eq!(cancelled.0.borrow().forwarded, prefix);
                assert_eq!(cancelled.0.borrow().live, 0);
            }
        }
    });
}

#[test]
fn flattened_issuer_rotations_release_old_and_new_ledgers_at_every_callback() {
    flat_visit(|parent, case| {
        for observation in flat_defaults(case) {
            let request = flat_request(observation);
            let mut baseline = Policy::<true>::new(usize::MAX);
            let value =
                flat_admitted(parent, request.clone(), Default::default(), &mut baseline).unwrap();
            let reservations = baseline.0.borrow().reservations;
            let calls = baseline.0.borrow().work_calls;
            let total = value.work();
            drop(value);
            for reservation in [false, true] {
                for index in 1..=if reservation { reservations } else { calls } {
                    let mut policy = Policy::<true>::new(usize::MAX);
                    let old = Rc::clone(&policy.0);
                    if reservation {
                        policy.0.borrow_mut().rotate_reservation = Some(index);
                    } else {
                        policy.0.borrow_mut().rotate_work_call = Some(index);
                    }
                    assert!(matches!(
                        flat_admitted(parent, request.clone(), Default::default(), &mut policy),
                        Err(PlanError::AudioMetadataIssuerMismatch)
                    ));
                    assert!(!Rc::ptr_eq(&old, &policy.0));
                    assert_eq!(old.borrow().live, 0);
                    assert_eq!(policy.0.borrow().live, 0);
                    if reservation {
                        assert_eq!(old.borrow().reservations, index);
                        assert!(old.borrow().forwarded > 0);
                    } else {
                        assert_eq!(old.borrow().work_calls, index);
                    }
                    assert!(old.borrow().forwarded <= total);
                    assert_eq!(policy.0.borrow().forwarded, 0);
                }
            }
        }
    });
}

#[test]
fn flattened_empty_query_admits_both_header_identities_before_ownership() {
    flat_visit(|parent, case| {
        let at = flat_window(&case["window"]).start;
        let plan = parent.plan();
        let expected =
            crate::audio_allocated::admission::identity(plan.metadata.project_id.as_str())
                .unwrap()
                .size()
                + crate::audio_allocated::admission::identity(plan.metadata.revision_id.as_str())
                    .unwrap()
                    .size();
        let mut policy = Policy::<true>::new(expected);
        let value = flat_admitted(parent, at..at, Default::default(), &mut policy).unwrap();
        assert!(value.query().spans.is_empty());
        assert_eq!(value.query().spans.capacity(), 0);
        assert_eq!(value.canonical_units(), 0);
        assert_eq!(value.additional_units(), 4);
        assert_eq!(policy.0.borrow().reservations, 2);
        assert_eq!(policy.0.borrow().live, expected);
        assert_eq!(policy.0.borrow().peak, expected);
        drop(value);
        assert_eq!(policy.0.borrow().live, 0);
        let mut short = Policy::<true>::new(expected - 1);
        assert!(matches!(
            flat_admitted(parent, at..at, Default::default(), &mut short),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(short.0.borrow().reservations, 2);
        assert_eq!(short.0.borrow().live, 0);
    });
}

#[derive(Debug)]
struct FlatLayoutEvent {
    size: usize,
    align: usize,
    before: usize,
    after: usize,
    accepted: bool,
}
// Observes only; all lease accounting remains the existing Policy implementation.
struct FlatLayoutObserver<'a> {
    policy: &'a mut Policy<true>,
    events: Vec<FlatLayoutEvent>,
}
impl AudioSoundContextAdmission for FlatLayoutObserver<'_> {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.policy.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.policy.spend(units)
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let before = self.policy.0.borrow().live;
        let result = self.policy.reserve(layout);
        self.events.push(FlatLayoutEvent {
            size: layout.size(),
            align: layout.align(),
            before,
            after: self.policy.0.borrow().live,
            accepted: result.is_ok(),
        });
        result
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.policy.merge(target, source)
    }
}

fn flat_arc_str_layout(text: &str) -> Layout {
    Layout::new::<[usize; 2]>()
        .extend(Layout::for_value(text))
        .unwrap()
        .0
        .pad_to_align()
}

#[test]
fn flattened_asset_layout_delta_matches_each_independent_owning_edge() {
    let mut receipts = Vec::new();
    let mut layouts = Vec::new();
    for asset in [
        "catalog-sound".to_owned(),
        "long-asset-".to_owned() + &"x".repeat(96),
    ] {
        let plan = crate::audio_allocated::query_storage_tests::fixture_with_asset(
            deadpan_core::AssetId::new(&asset).unwrap(),
        );
        let samples = AudioSample(0)..plan.audio_duration().unwrap();
        let ordinary = plan.audio(samples.clone(), Default::default()).unwrap();
        assert_eq!(ordinary.spans.len(), 2);
        assert!(ordinary.spans.iter().all(|span| matches!(
            &span.content,
            crate::AudioContent::Source { source, .. } if source.asset.as_str() == asset
        )));
        let mut policy = Policy::<true>::new(usize::MAX);
        let mut observer = FlatLayoutObserver {
            policy: &mut policy,
            events: Vec::new(),
        };
        let query = plan
            .audio_admitted(samples, Default::default(), &mut observer)
            .unwrap();
        let mut receipt = std::collections::BTreeMap::<(usize, usize), usize>::new();
        for event in &observer.events {
            assert!(event.accepted);
            *receipt.entry((event.size, event.align)).or_default() += 1;
        }
        let layout = flat_arc_str_layout(&asset);
        layouts.push((layout.size(), layout.align()));
        receipts.push(receipt);
        drop(query);
        assert_eq!(observer.policy.0.borrow().live, 0);
    }
    assert_ne!(layouts[0], layouts[1]);
    let mut expected = receipts.remove(0);
    // Only the two Source asset edges change. Subtracting the unchanged
    // allocations prevents equal padded layouts from hiding a missing clone.
    *expected.get_mut(&layouts[0]).unwrap() -= 2;
    *expected.entry(layouts[1]).or_default() += 2;
    expected.retain(|_, count| *count != 0);
    let mut actual = receipts.remove(0);
    actual.retain(|_, count| *count != 0);
    assert_eq!(actual, expected);
}

#[test]
fn flattened_outer_growth_retains_old_buffer_prior_spans_and_current_span() {
    let mut checked = false;
    flat_visit(|parent, case| {
        if case["name"] != "repeat_root_all_five_spans" {
            return;
        }
        let mut policy = Policy::<true>::new(usize::MAX);
        let mut observer = FlatLayoutObserver {
            policy: &mut policy,
            events: Vec::new(),
        };
        let value = flat_admitted(
            parent,
            flat_window(&case["window"]),
            Default::default(),
            &mut observer,
        )
        .unwrap();
        assert_eq!(value.query().spans.len(), 5);
        assert_eq!(value.query().spans.capacity(), 8);
        let span_bytes: Vec<_> = value
            .query()
            .spans
            .iter()
            .map(|span| crate::audio_allocated::measure(span, usize::MAX).unwrap().0)
            .collect();
        let unit = std::mem::size_of::<AudioSpan>();
        let align = std::mem::align_of::<AudioSpan>();
        let mut remaining = observer.events.iter();
        for (current_index, old_capacity, new_capacity) in
            [(0, 0, 1), (1, 1, 2), (2, 2, 4), (4, 4, 8)]
        {
            let live_contents: usize = span_bytes[..=current_index].iter().sum();
            let event = remaining
                .find(|event| {
                    event.accepted
                        && event.align == align
                        && event.size == new_capacity * unit
                        && event.before >= old_capacity * unit + live_contents
                })
                .unwrap();
            assert_eq!(event.after, event.before + event.size);
        }
        assert!(observer.events.iter().all(|event| event.accepted));
        let peak = observer.policy.0.borrow().peak;
        let retained = observer.policy.0.borrow().live;
        assert!(
            peak > retained,
            "double-buffer construction exceeds retained layout charge"
        );
        drop(value);
        assert_eq!(observer.policy.0.borrow().live, 0);
        checked = true;
    });
    assert!(checked);
}

#[test]
fn flattened_retained_storage_observer_is_billed_separately_including_failed_inspection() {
    flat_visit(|parent, case| {
        for observation in flat_defaults(case) {
            let mut policy = Policy::<true>::new(usize::MAX);
            let value = flat_admitted(
                parent,
                flat_request(observation),
                Default::default(),
                &mut policy,
            )
            .unwrap();
            let constructor_work = value.work();
            let retained = policy.0.borrow().live;
            let (bytes, inspected) = value
                .query()
                .measure_allocated_storage(usize::MAX)
                .into_parts();
            policy.spend(inspected).unwrap();
            let bytes = bytes.unwrap();
            assert!(bytes > 0 && bytes <= retained);
            assert!(inspected > 0);
            assert_eq!(policy.0.borrow().forwarded, constructor_work + inspected);
            assert_eq!(
                policy.0.borrow().live,
                retained,
                "observer creates no admission"
            );
            let (bytes, partial) = value.query().measure_allocated_storage(1).into_parts();
            policy.spend(partial).unwrap();
            assert!(matches!(
                bytes,
                Err(PlanError::AudioQueryLimit("retained audio heap traversal"))
            ));
            assert_eq!(partial, 1);
            assert_eq!(
                policy.0.borrow().forwarded,
                constructor_work + inspected + partial
            );
            drop(value);
            assert_eq!(policy.0.borrow().live, 0);
            assert_eq!(
                policy.0.borrow().forwarded,
                constructor_work + inspected + partial
            );
        }
    });
}

#[test]
fn flattened_live_carrier_is_visible_to_later_required_query_reservations() {
    flat_visit(|parent, case| {
        let request = flat_window(&case["window"]);
        let mut measure = Policy::<true>::new(usize::MAX);
        let value =
            flat_admitted(parent, request.clone(), Default::default(), &mut measure).unwrap();
        let retained = measure.0.borrow().live;
        let solo_peak = measure.0.borrow().peak;
        drop(value);
        let mut exact = Policy::<true>::new(retained + solo_peak);
        let first = flat_admitted(parent, request.clone(), Default::default(), &mut exact).unwrap();
        let second =
            flat_admitted(parent, request.clone(), Default::default(), &mut exact).unwrap();
        assert_eq!(exact.0.borrow().peak, retained + solo_peak);
        drop(second);
        assert_eq!(exact.0.borrow().live, retained);
        let first_snapshot = flat::query_snapshot(first.query());
        drop(first);
        assert_eq!(exact.0.borrow().live, 0);
        let mut short = Policy::<true>::new(retained + solo_peak - 1);
        let first = flat_admitted(parent, request.clone(), Default::default(), &mut short).unwrap();
        assert!(matches!(
            flat_admitted(parent, request, Default::default(), &mut short),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(flat::query_snapshot(first.query()), first_snapshot);
        assert_eq!(short.0.borrow().live, retained);
        drop(first);
        assert_eq!(short.0.borrow().live, 0);
    });
}

#[test]
fn flattened_narrow_extraction_rejects_actual_preserve_and_core_bound_walks() {
    use crate::plan::audio::AudioWalkSpan;
    let mut stage = 0;
    let mut core = 0;
    // The existing checked wide visitor includes actual retained bindings that
    // the six flattened parents intentionally flatten through instead of stop.
    super::visit_cases(&super::snapshot(), |parent, case| {
        let ordinary = flat_ordinary(parent, super::range(case), Default::default()).unwrap();
        let (plan, seed) = match parent {
            Parent::Root(plan) => (plan, None),
            Parent::Domain(domain) => (domain.plan, Some(&domain.seed)),
        };
        for span in &ordinary.spans {
            for stops in [(true, false), (false, true)] {
                let mut policy = Policy::<true>::new(usize::MAX);
                let mut local = Budget {
                    remaining: AudioQueryLimits::default().maximum_work,
                    lookup: Default::default(),
                };
                let mut budget = admission::Budget::new(&mut local, &mut policy);
                let walked = admission::outer::<_, Policy<true>>(plan.audio_walk_inner_admitted(
                    span.samples.start,
                    &mut budget,
                    seed,
                    stops.0,
                    stops.1,
                    (None, None, None, None),
                ))
                .unwrap();
                if matches!(walked.span(), AudioWalkSpan::Leaf(_)) {
                    drop(walked);
                } else {
                    let result =
                        admission::outer::<_, Policy<true>>(walked.flattened_leaf(&mut budget));
                    match result {
                        Err(PlanError::InvalidPlan("flattened audio retained a stage")) => {
                            stage += 1
                        }
                        Err(PlanError::InvalidPlan(
                            "flattened audio retained a metered binding",
                        )) => core += 1,
                        Err(error) => panic!("unexpected flattened extraction error: {error}"),
                        Ok(_) => panic!("flattened extraction accepted a retained stop"),
                    }
                }
                assert_eq!(policy.0.borrow().live, 0);
            }
        }
    });
    // The reusable nested-Preserve fixture also exercises an actual Stage stop.
    flat_visit(|parent, case| {
        if case["name"] != "source_repeat_nested_preserve_root" {
            return;
        }
        let plan = parent.plan();
        let mut policy = Policy::<true>::new(usize::MAX);
        let mut local = Budget {
            remaining: AudioQueryLimits::default().maximum_work,
            lookup: Default::default(),
        };
        let mut budget = admission::Budget::new(&mut local, &mut policy);
        let walked = admission::outer::<_, Policy<true>>(plan.audio_walk_inner_admitted(
            flat_window(&case["window"]).start,
            &mut budget,
            None,
            true,
            false,
            (None, None, None, None),
        ))
        .unwrap();
        assert!(matches!(walked.span(), AudioWalkSpan::Stage(_)));
        assert!(matches!(
            admission::outer::<_, Policy<true>>(walked.flattened_leaf(&mut budget)),
            Err(PlanError::InvalidPlan("flattened audio retained a stage"))
        ));
        stage += 1;
        assert_eq!(policy.0.borrow().live, 0);
    });
    assert!(stage > 0);
    assert!(
        core > 0,
        "existing bound fixtures must exercise metered Core rejection"
    );
}
