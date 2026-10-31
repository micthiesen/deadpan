//! Actual policy values and accepted callbacks against the immutable OLD body.
use super::*;
use crate::AdmittedAudioPolicyQuery;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::{
    policy_binding_baseline as policy_trace, policy_query_baseline as oracle,
    processing_query_baseline as processing,
};
use std::{collections::BTreeSet, sync::OnceLock};

fn frozen() -> &'static Value {
    static BODY: OnceLock<Value> = OnceLock::new();
    BODY.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/policy-query-baseline-v1/snapshot.json"
        ))
        .unwrap()
    })
}

fn samples(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}

fn request_limits(value: &Value) -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_work: value["maximum_work"].as_u64().unwrap().try_into().unwrap(),
        maximum_spans: value["maximum_spans"].as_u64().unwrap().try_into().unwrap(),
    }
}

fn policy_admitted<A: AudioSoundContextAdmission>(
    parent: Parent<'_, '_>,
    range: Range<AudioSample>,
    limits: AudioQueryLimits,
    admission: &mut A,
) -> Result<AdmittedAudioPolicyQuery<A::Issuer, A::Lease>, A::Error> {
    match parent {
        Parent::Root(plan) => plan.audio_policy_admitted(range, limits, admission),
        Parent::Domain(domain) => domain.policy_admitted(range, limits, admission),
    }
}

fn visit_seed(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let mut count = 0;
    processing::visit_cases(|parent, descriptor| {
        let case = frozen()["ordinary_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        let descriptor = oracle::descriptor_snapshot(descriptor);
        for (key, value) in descriptor.as_object().unwrap() {
            assert_eq!(&case[key], value, "actual checked parent provenance {key}");
        }
        visit(parent, case);
        count += 1;
    });
    assert_eq!(count, 33);
}

fn no_callbacks(policy: &Policy<false>) {
    let state = policy.0.borrow();
    assert_eq!(
        (
            state.forwarded,
            state.work_calls,
            state.reservations,
            state.live
        ),
        (0, 0, 0, 0)
    );
}

fn false_ordinary(parent: Parent<'_, '_>, row: &Value) {
    let mut admission = Policy::<false>::new(0);
    let observer = policy_trace::Observation::start();
    let result = policy_admitted(
        parent,
        samples(&row["request"]["samples"]),
        request_limits(&row["request"]["limits"]),
        &mut admission,
    );
    let events = observer.finish();
    match result {
        Ok(query) => {
            assert_eq!(row["output"]["result"], "Ok", "{}", row["name"]);
            assert_eq!(
                oracle::query_snapshot(query.query()),
                row["output"]["policy"]
            );
            assert_eq!(query.work(), query.canonical_units());
            assert_eq!(query.work(), query.query().work);
            assert_eq!(query.additional_units(), 0);
            assert!(!query.was_forwarded());
            drop(query);
        }
        Err(error) => assert_eq!(oracle::failed(&error), row["output"], "{}", row["name"]),
    }
    assert_eq!(
        Value::Array(events),
        row["actual_domain_second_query_events"]
    );
    no_callbacks(&admission);
}

fn false_sealed(parent: Parent<'_, '_>, row: &Value) {
    let Parent::Domain(domain) = parent else {
        panic!("sealed OLD observation requires its actual Domain");
    };
    let mut admission = Policy::<false>::new(0);
    let first_request = &row["output"]["first_processing_request"];
    let first = domain
        .processing_for_policy_admitted(
            samples(&first_request["samples"]),
            request_limits(&first_request["limits"]),
            &mut admission,
        )
        .unwrap();
    let first_body = processing::query_snapshot(parent.plan(), first.query().query());
    assert_eq!(first_body, row["output"]["first_processing"]["query"]);
    let cloned = domain.clone();
    let caller = if row["finish_caller"] == "actual_equal_cloned_domain_object" {
        &cloned
    } else {
        domain
    };
    let observer = policy_trace::Observation::start();
    let result = first.finish_policy_admitted(
        caller,
        samples(&row["request"]["samples"]),
        request_limits(&row["request"]["limits"]),
        &mut admission,
    );
    let events = observer.finish();
    match result {
        Ok(completed) => {
            let (returned, policy) = completed.into_queries();
            assert_eq!(row["output"]["finish"]["result"], "Ok");
            let returned_body = processing::query_snapshot(parent.plan(), returned.query());
            assert_eq!(returned_body, first_body);
            assert_eq!(returned_body, row["output"]["finish"]["processing"]);
            assert_eq!(
                oracle::query_snapshot(policy.query()),
                row["output"]["finish"]["policy"]
            );
            assert_eq!(policy.work(), policy.canonical_units());
            assert_eq!(policy.additional_units(), 0);
            assert!(!policy.was_forwarded());
            drop(policy);
            drop(returned);
        }
        Err(error) => assert_eq!(oracle::failed(&error), row["output"]["finish"]),
    }
    assert_eq!(
        Value::Array(events),
        row["actual_domain_second_query_events"]
    );
    no_callbacks(&admission);
}

fn false_row(parent: Parent<'_, '_>, row: &Value) {
    if row["output"].get("first_processing").is_some() {
        false_sealed(parent, row);
    } else {
        false_ordinary(parent, row);
    }
}

fn assert_true(
    query: &AdmittedAudioPolicyQuery<usize, Lease>,
    old: &Value,
    admission: &Policy<true>,
    before: (usize, usize),
) {
    let observed = oracle::query_snapshot(query.query());
    assert_eq!(
        observed.as_object().unwrap().len(),
        old.as_object().unwrap().len()
    );
    for field in ["suppressed", "contents", "lookup"] {
        assert_eq!(
            observed[field], old[field],
            "complete actual policy field {field}"
        );
    }
    assert_eq!(
        query.canonical_units(),
        old["work"].as_u64().unwrap() as usize
    );
    assert_eq!(observed["work"].as_u64().unwrap() as usize, query.work());
    assert_eq!(query.query().work, query.work());
    assert_eq!(
        query
            .canonical_units()
            .checked_add(query.additional_units()),
        Some(query.work())
    );
    assert!(query.was_forwarded());
    let state = admission.0.borrow();
    assert_eq!(state.forwarded.checked_sub(before.0), Some(query.work()));
    assert!(query.additional_units() >= state.reservations - before.1);
    if !query.query().contents.is_empty() || !query.query().suppressed.is_empty() {
        assert!(
            state.live > 0,
            "nonempty owned output must retain its actual charge"
        );
    }
}

fn true_ordinary(parent: Parent<'_, '_>, row: &Value) {
    assert_eq!(row["output"]["result"], "Ok");
    let mut admission = Policy::<true>::new(usize::MAX);
    let query = policy_admitted(
        parent,
        samples(&row["request"]["samples"]),
        request_limits(&row["request"]["limits"]),
        &mut admission,
    )
    .unwrap_or_else(|error| {
        panic!(
            "metered policy request {} / old work {}: {error:?}",
            row["request"], row["output"]["policy"]["work"]
        )
    });
    assert_true(&query, &row["output"]["policy"], &admission, (0, 0));
    let work = query.work();
    let live = admission.0.borrow().live;
    let ledger = Rc::clone(&admission.0);
    drop(admission);
    assert_eq!(ledger.borrow().live, live);
    drop(query);
    assert_eq!(ledger.borrow().live, 0);
    assert_eq!(ledger.borrow().forwarded, work);
}

fn defaults(case: &Value) -> impl Iterator<Item = &Value> {
    case["observations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            matches!(
                row["name"].as_str().unwrap(),
                "default_full" | "default_clipped" | "default_empty"
            )
        })
}

#[test]
fn policy_false_matches_all_973_seed_outputs_errors_work_lookup_and_second_events() {
    let mut count = 0;
    visit_seed(|parent, case| {
        for row in case["observations"].as_array().unwrap() {
            false_ordinary(parent, row);
            count += 1;
        }
    });
    assert_eq!(count, 973);
}

#[test]
fn policy_true_99_full_crop_empty_outputs_keep_old_canonical_and_actual_forwarding() {
    let mut count = 0;
    visit_seed(|parent, case| {
        for row in defaults(case) {
            true_ordinary(parent, row);
            count += 1;
        }
    });
    assert_eq!(count, 99);
}

#[test]
fn policy_false_matches_all_411_sealed_first_products_fresh_limits_and_identity_errors() {
    let mut count = 0;
    visit_seed(|parent, descriptor| {
        let Parent::Domain(_) = parent else { return };
        let case = frozen()["sealed_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        for row in case["observations"].as_array().unwrap() {
            false_sealed(parent, row);
            count += 1;
        }
    });
    assert_eq!(count, 411);
}

fn visit_endpoints(mut visit: impl FnMut(Parent<'_, '_>, &Value, &Value, usize)) {
    let variants = &frozen()["retained_policy_cases"]["source_endpoint"]["endpoint_variants"];
    let mut count = 0;
    for (endpoint, name) in [
        (deadpan_core::AudioSourceEndpoint::Start, "Start"),
        (deadpan_core::AudioSourceEndpoint::End, "End"),
    ] {
        let old = variants
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["endpoint"] == name)
            .unwrap();
        let (document, plan) = oracle::policy_retained_cases::source_endpoint_parent(endpoint);
        assert_eq!(
            serde_json::to_value(&document).unwrap(),
            old["fixture_source"][0]["fixture_source"]["document"]
        );
        let at = samples(&old["fixture_source"][0]["window"]).start;
        visit(Parent::Root(&plan), &old["fixture_source"][0], old, 0);
        count += 1;
        let owners = plan
            .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
            .unwrap();
        let owner = owners
            .spans()
            .iter()
            .flat_map(|span| span.owners())
            .find(|owner| owner.instance().node.as_str() == "source")
            .unwrap();
        let input = plan
            .audio_owner_input(owner, at, Default::default())
            .unwrap();
        visit(
            Parent::Domain(input.context()),
            &old["fixture_source"][1],
            old,
            1,
        );
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn dedicated_retained_dense_and_endpoint_false_observations_remain_literal() {
    let mut retained = 0;
    oracle::policy_retained_cases::visit_cases(|parent, descriptor| {
        let old = frozen()["retained_policy_cases"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["descriptor"]["name"] == descriptor["name"])
            .unwrap();
        assert_eq!(&old["descriptor"], descriptor);
        false_ordinary(parent, &old["default"]);
        for row in old["edges"]["observations"].as_array().unwrap() {
            false_ordinary(parent, row);
        }
        retained += 1;
    });
    assert_eq!(retained, 11);
    let mut dense = 0;
    oracle::policy_dense_fallback_cases::visit_cases(|parent, descriptor| {
        let old = frozen()["dense_and_fallback_cases"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        for (key, value) in descriptor.as_object().unwrap() {
            assert_eq!(&old[key], value);
        }
        for row in old["observations"].as_array().unwrap() {
            false_row(parent, row);
        }
        dense += 1;
    });
    assert_eq!(dense, 5);
    let mut surveyed = 0;
    visit_seed(|parent, descriptor| {
        let Parent::Domain(_) = parent else {
            return;
        };
        if let Some(case) =
            frozen()["dense_and_fallback_cases"]["existing_wider_first_span_gate_survey"]["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| case["name"] == descriptor["name"])
        {
            false_sealed(parent, &case["sealed_finish"]);
            surveyed += 1;
        }
    });
    assert_eq!(surveyed, 6);
    visit_endpoints(|parent, descriptor, variant, index| {
        let lane = if index == 0 { "root" } else { "domain" };
        for row in variant["fresh_limit_matrices"][lane].as_array().unwrap() {
            false_ordinary(parent, row);
        }
        false_ordinary(parent, &variant["observations"][index]);
        for row in variant["observations"][index + 2]["observations"]
            .as_array()
            .unwrap()
        {
            false_ordinary(parent, row);
        }
        let prefix = format!("{}_", descriptor["name"].as_str().unwrap());
        for row in variant["exact_endpoint_seams"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["name"].as_str().unwrap().starts_with(&prefix))
        {
            false_ordinary(parent, row);
        }
    });
}

#[test]
fn dedicated_retained_dense_and_endpoint_true_defaults_keep_complete_old_fields() {
    let mut retained = 0;
    oracle::policy_retained_cases::visit_cases(|parent, descriptor| {
        let old = frozen()["retained_policy_cases"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["descriptor"]["name"] == descriptor["name"])
            .unwrap();
        true_ordinary(parent, &old["default"]);
        retained += 1;
    });
    assert_eq!(retained, 11);
    let mut dense = 0;
    oracle::policy_dense_fallback_cases::visit_cases(|parent, descriptor| {
        if !descriptor["name"].as_str().unwrap().starts_with("dense/") {
            return;
        }
        let old = frozen()["dense_and_fallback_cases"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        let row = old["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "explicit_256_span_full")
            .unwrap();
        if descriptor["name"] == "dense/actual_256_repeat_bound/domain" {
            return;
        }
        true_ordinary(parent, row);
        if descriptor["name"] == "dense/actual_256_repeat_bound/root" {
            assert!(
                row["output"]["policy"]["contents"]
                    .as_array()
                    .unwrap()
                    .len()
                    > 256
            );
        }
        dense += 1;
    });
    assert_eq!(dense, 2);
    visit_endpoints(|parent, _, variant, index| {
        let lane = if index == 0 { "root" } else { "domain" };
        for row in variant["fresh_limit_matrices"][lane]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                matches!(
                    row["name"].as_str().unwrap(),
                    "default_full" | "default_clipped" | "default_empty"
                )
            })
        {
            true_ordinary(parent, row);
        }
    });
}

fn work_limit_error(error: PlanError, zero_limit: bool) {
    match error {
        PlanError::AudioQueryLimit(_) => {}
        PlanError::InvalidAudioLimits => assert!(zero_limit),
        PlanError::Document(error) => {
            assert_eq!(error.code, deadpan_core::DocumentErrorCode::LimitExceeded)
        }
        error => panic!("unrelated one-short work error: {error}"),
    }
}

#[test]
fn policy_actual_total_and_requested_layout_peak_fit_exactly_and_one_short_releases() {
    visit_seed(|parent, case| {
        let row = defaults(case)
            .find(|row| row["name"] == "default_full")
            .unwrap();
        let range = samples(&row["request"]["samples"]);
        let mut measured = Policy::<true>::new(usize::MAX);
        let query =
            policy_admitted(parent, range.clone(), Default::default(), &mut measured).unwrap();
        assert_true(&query, &row["output"]["policy"], &measured, (0, 0));
        let total = query.work();
        let peak = measured.0.borrow().peak;
        drop(query);
        assert_eq!(measured.0.borrow().live, 0);
        let mut exact = Policy::<true>::new(peak);
        let query = policy_admitted(
            parent,
            range.clone(),
            AudioQueryLimits {
                maximum_work: total,
                ..Default::default()
            },
            &mut exact,
        )
        .unwrap();
        assert_eq!(query.work(), total);
        assert_eq!(exact.0.borrow().peak, peak);
        drop(query);
        assert_eq!(exact.0.borrow().live, 0);
        let mut short_work = Policy::<true>::new(usize::MAX);
        match policy_admitted(
            parent,
            range.clone(),
            AudioQueryLimits {
                maximum_work: total.checked_sub(1).unwrap(),
                ..Default::default()
            },
            &mut short_work,
        ) {
            Ok(_) => panic!("one-short policy total completed"),
            Err(error) => work_limit_error(error, total == 1),
        }
        assert_eq!(short_work.0.borrow().live, 0);
        assert!(short_work.0.borrow().forwarded < total);
        if peak != 0 {
            let mut short = Policy::<true>::new(peak - 1);
            assert!(matches!(
                policy_admitted(parent, range, Default::default(), &mut short),
                Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
            ));
            assert_eq!(short.0.borrow().live, 0);
        }
    });
}

struct Observer<'a> {
    policy: &'a mut Policy<true>,
    spends: Vec<usize>,
    reservations: Vec<usize>,
    floor: usize,
}
impl AudioSoundContextAdmission for Observer<'_> {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.policy.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        assert!(
            self.policy.0.borrow().live >= self.floor,
            "first query stays live before work"
        );
        self.policy.spend(units)?;
        self.spends.push(units);
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        assert!(
            self.policy.0.borrow().live >= self.floor,
            "first query stays live before reserve"
        );
        let lease = self.policy.reserve(layout)?;
        self.reservations.push(self.policy.0.borrow().forwarded);
        Ok(lease)
    }
    fn merge(&mut self, into: &mut Lease, from: &mut Lease) {
        assert!(self.policy.0.borrow().live >= self.floor);
        self.policy.merge(into, from);
    }
}

fn focused(case: &Value) -> bool {
    matches!(
        case["name"].as_str().unwrap(),
        "wide/nonunity_shift_fractional_point_ceil_source44100/root"
            | "leaf/source_fit_beat/root"
            | "wide/stable_repeat_plays_default_gap_and_overrides/root"
            | "wide/fractional_actual_physical_capture_domain/domain"
            | "additional/exposed_preserve_and_silent_hold/domain"
    )
}

#[test]
fn policy_every_representative_callback_refusal_cancellation_and_rotation_keeps_actual_prefix() {
    let mut checked = 0;
    visit_seed(|parent, case| {
        if !focused(case) {
            return;
        }
        let range = samples(&case["window"]);
        let mut baseline = Policy::<true>::new(usize::MAX);
        let mut observer = Observer {
            policy: &mut baseline,
            spends: Vec::new(),
            reservations: Vec::new(),
            floor: 0,
        };
        let query =
            policy_admitted(parent, range.clone(), Default::default(), &mut observer).unwrap();
        let total = query.work();
        let spends = observer.spends.clone();
        let reservations = observer.reservations.clone();
        assert_eq!(spends.iter().sum::<usize>(), total);
        drop(query);
        assert_eq!(observer.policy.0.borrow().live, 0);
        for (index, accepted) in reservations.iter().copied().enumerate() {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_reservation = Some(index + 1);
            assert!(matches!(
                policy_admitted(parent, range.clone(), Default::default(), &mut refusing),
                Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
            ));
            assert_eq!(refusing.0.borrow().reservations, index + 1);
            assert_eq!(refusing.0.borrow().forwarded, accepted);
            assert_eq!(refusing.0.borrow().live, 0);
        }
        let mut before = 0;
        let mut cancellation = BTreeSet::new();
        let mut after_spends = Vec::new();
        for (index, units) in spends.iter().copied().enumerate() {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_work_call = Some(index + 1);
            assert!(matches!(
                policy_admitted(parent, range.clone(), Default::default(), &mut refusing),
                Err(PlanError::AudioQueryLimit("test wide work refusal"))
            ));
            assert_eq!(refusing.0.borrow().work_calls, index + 1);
            assert_eq!(refusing.0.borrow().forwarded, before);
            assert_eq!(refusing.0.borrow().live, 0);
            cancellation.insert(before);
            before += units;
            after_spends.push(before);
        }
        assert_eq!(before, total);
        for prefix in cancellation {
            let mut cancelled = Policy::<true>::new(usize::MAX);
            cancelled.0.borrow_mut().cancel_after = Some(prefix);
            assert!(matches!(
                policy_admitted(parent, range.clone(), Default::default(), &mut cancelled),
                Err(PlanError::AudioQueryLimit("test wide cancellation"))
            ));
            assert_eq!(cancelled.0.borrow().forwarded, prefix);
            assert_eq!(cancelled.0.borrow().live, 0);
        }
        for (reservation, values) in [(false, after_spends), (true, reservations)] {
            for (index, accepted) in values.into_iter().enumerate() {
                let mut rotating = Policy::<true>::new(usize::MAX);
                let old = Rc::clone(&rotating.0);
                if reservation {
                    rotating.0.borrow_mut().rotate_reservation = Some(index + 1);
                } else {
                    rotating.0.borrow_mut().rotate_work_call = Some(index + 1);
                }
                assert!(matches!(
                    policy_admitted(parent, range.clone(), Default::default(), &mut rotating),
                    Err(PlanError::AudioMetadataIssuerMismatch)
                ));
                assert!(!Rc::ptr_eq(&old, &rotating.0));
                assert_eq!(old.borrow().forwarded, accepted);
                assert_eq!(old.borrow().live, 0);
                assert_eq!(rotating.0.borrow().forwarded, 0);
                assert_eq!(rotating.0.borrow().live, 0);
            }
        }
        checked += 1;
    });
    assert_eq!(checked, 5);
}

fn true_sealed(parent: Parent<'_, '_>, row: &Value) {
    let Parent::Domain(domain) = parent else {
        panic!("sealed case requires Domain");
    };
    assert_eq!(row["output"]["finish"]["result"], "Ok");
    let range = samples(&row["request"]["samples"]);
    let first_request = &row["output"]["first_processing_request"];
    let mut admission = Policy::<true>::new(usize::MAX);
    let first = domain
        .processing_for_policy_admitted(
            samples(&first_request["samples"]),
            request_limits(&first_request["limits"]),
            &mut admission,
        )
        .unwrap();
    let first_body = processing::query_snapshot(parent.plan(), first.query().query());
    let first_ptr = first.query().query().spans.as_ptr();
    let first_work = first.query().work();
    let before = {
        let state = admission.0.borrow();
        (state.live, state.forwarded, state.reservations)
    };
    let mut observer = Observer {
        policy: &mut admission,
        spends: Vec::new(),
        reservations: Vec::new(),
        floor: before.0,
    };
    let completed = first
        .finish_policy_admitted(
            domain,
            range,
            request_limits(&row["request"]["limits"]),
            &mut observer,
        )
        .unwrap();
    assert_eq!(
        observer.spends.iter().sum::<usize>(),
        observer.policy.0.borrow().forwarded - before.1
    );
    let (returned, query) = completed.into_queries();
    assert_eq!(
        processing::query_snapshot(parent.plan(), returned.query()),
        first_body
    );
    assert_eq!(returned.query().spans.as_ptr(), first_ptr);
    assert_eq!(returned.work(), first_work);
    assert_true(
        &query,
        &row["output"]["finish"]["policy"],
        observer.policy,
        (before.1, before.2),
    );
    drop(query);
    assert_eq!(observer.policy.0.borrow().live, before.0);
    drop(returned);
    assert_eq!(observer.policy.0.borrow().live, 0);
}

#[test]
fn sealed_true_defaults_hold_first_through_every_policy_callback_and_return_it_unchanged() {
    let mut count = 0;
    visit_seed(|parent, descriptor| {
        let Parent::Domain(_) = parent else {
            return;
        };
        let case = frozen()["sealed_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        for row in defaults(case) {
            true_sealed(parent, row);
            count += 1;
        }
    });
    assert_eq!(count, 39);
}

#[test]
fn sealed_true_two_actual_stage_fallbacks_use_the_recorded_fresh_positive_cap() {
    let mut count = 0;
    oracle::policy_dense_fallback_cases::visit_cases(|parent, descriptor| {
        if !descriptor["name"]
            .as_str()
            .unwrap()
            .starts_with("fresh_span_fallback/")
        {
            return;
        }
        let old = frozen()["dense_and_fallback_cases"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        let row = old["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "sealed_fresh_span_two_reached_second_walk")
            .unwrap();
        true_sealed(parent, row);
        count += 1;
    });
    assert_eq!(count, 2);
}

#[test]
fn ordinary_finish_on_a_metered_first_query_keeps_its_existing_compatibility_behavior() {
    let mut count = 0;
    visit_seed(|parent, descriptor| {
        let Parent::Domain(domain) = parent else {
            return;
        };
        let case = frozen()["sealed_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        let row = defaults(case)
            .find(|row| row["name"] == "default_full")
            .unwrap();
        let range = samples(&row["request"]["samples"]);
        let mut admission = Policy::<true>::new(usize::MAX);
        let first = domain
            .processing_for_policy_admitted(range.clone(), Default::default(), &mut admission)
            .unwrap();
        let before = callback_counts(&admission);
        let first_body = processing::query_snapshot(parent.plan(), first.query().query());
        let (returned, policy) = first
            .finish_policy(domain, range, request_limits(&row["request"]["limits"]))
            .unwrap()
            .into_queries();
        assert_eq!(
            oracle::query_snapshot(&policy),
            row["output"]["finish"]["policy"]
        );
        assert_eq!(
            processing::query_snapshot(parent.plan(), returned.query()),
            first_body
        );
        assert_eq!(callback_counts(&admission), before);
        drop(returned);
        assert_eq!(admission.0.borrow().live, 0);
        count += 1;
    });
    assert_eq!(count, 13);
}

fn callback_counts<const M: bool>(policy: &Policy<M>) -> (usize, usize, usize) {
    let state = policy.0.borrow();
    (state.forwarded, state.work_calls, state.reservations)
}

#[test]
fn sealed_identity_foreign_issuer_and_same_typed_mixed_mode_leaf_refuse_before_fresh_callbacks() {
    let mut checked = false;
    visit_seed(|parent, case| {
        if checked {
            return;
        }
        let Parent::Domain(domain) = parent else {
            return;
        };
        let range = samples(&case["window"]);
        let mut source = Policy::<false>::new(0);
        let first = domain
            .processing_for_policy_admitted(range.clone(), Default::default(), &mut source)
            .unwrap();
        if first.query().query().spans.is_empty()
            || !first
                .query()
                .query()
                .spans
                .iter()
                .all(|span| matches!(span.content, crate::AudioSignalContent::Leaf(_)))
        {
            return;
        }
        let mut same_issuer_metered = Policy::<true>(Rc::clone(&source.0));
        let before = callback_counts(&same_issuer_metered);
        assert!(matches!(
            first.finish_policy_admitted(
                domain,
                range.clone(),
                Default::default(),
                &mut same_issuer_metered
            ),
            Err(PlanError::InvalidPlan(
                "processing accounting mode mismatch"
            ))
        ));
        assert_eq!(callback_counts(&same_issuer_metered), before);
        assert_eq!(same_issuer_metered.0.borrow().live, 0);

        let mut original = Policy::<true>::new(usize::MAX);
        let first = domain
            .processing_for_policy_admitted(range.clone(), Default::default(), &mut original)
            .unwrap();
        let mut same_issuer_false = Policy::<false>(Rc::clone(&original.0));
        let before = callback_counts(&original);
        assert!(matches!(
            first.finish_policy_admitted(
                domain,
                range.clone(),
                Default::default(),
                &mut same_issuer_false
            ),
            Err(PlanError::InvalidPlan(
                "processing accounting mode mismatch"
            ))
        ));
        assert_eq!(callback_counts(&original), before);
        assert_eq!(original.0.borrow().live, 0);

        for changed_identity in [false, true] {
            let mut original = Policy::<true>::new(usize::MAX);
            let first = domain
                .processing_for_policy_admitted(range.clone(), Default::default(), &mut original)
                .unwrap();
            let original_before = callback_counts(&original);
            let mut foreign = Policy::<true>::new(usize::MAX);
            let cloned = domain.clone();
            let result = first.finish_policy_admitted(
                if changed_identity { &cloned } else { domain },
                range.clone(),
                if changed_identity {
                    AudioQueryLimits {
                        maximum_work: 0,
                        maximum_spans: 0,
                    }
                } else {
                    Default::default()
                },
                &mut foreign,
            );
            if changed_identity {
                assert!(matches!(
                    result,
                    Err(PlanError::InvalidPlan(
                        "processing policy identity mismatch"
                    ))
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(PlanError::AudioMetadataIssuerMismatch)
                ));
            }
            assert_eq!(callback_counts(&foreign), (0, 0, 0));
            assert_eq!(callback_counts(&original), original_before);
            assert_eq!(original.0.borrow().live, 0);
        }
        let mut original = Policy::<true>::new(usize::MAX);
        let first = domain
            .processing_for_policy_admitted(range.clone(), Default::default(), &mut original)
            .unwrap();
        let before = callback_counts(&original);
        assert!(matches!(
            first.finish_policy_admitted(
                domain,
                range.start..range.start,
                AudioQueryLimits {
                    maximum_work: 0,
                    maximum_spans: 0
                },
                &mut original
            ),
            Err(PlanError::InvalidPlan(
                "processing policy identity mismatch"
            ))
        ));
        assert_eq!(callback_counts(&original), before);
        assert_eq!(original.0.borrow().live, 0);
        checked = true;
    });
    assert!(
        checked,
        "must exercise an actual nonempty Leaf first tape without a Core mode guard"
    );
}

#[test]
fn sealed_fresh_limit_and_allocation_refusals_release_first_and_never_refund_work() {
    let mut checked = false;
    visit_seed(|parent, case| {
        if checked {
            return;
        }
        let Parent::Domain(domain) = parent else {
            return;
        };
        let range = samples(&case["window"]);
        if range.is_empty() {
            return;
        }
        let mut admission = Policy::<true>::new(usize::MAX);
        let first = domain
            .processing_for_policy_admitted(range.clone(), Default::default(), &mut admission)
            .unwrap();
        let before = callback_counts(&admission);
        assert!(matches!(
            first.finish_policy_admitted(
                domain,
                range.clone(),
                AudioQueryLimits {
                    maximum_work: 0,
                    maximum_spans: 0
                },
                &mut admission
            ),
            Err(PlanError::InvalidAudioLimits)
        ));
        assert_eq!(callback_counts(&admission), before);
        assert_eq!(admission.0.borrow().live, 0);

        let mut admission = Policy::<true>::new(usize::MAX);
        let first = domain
            .processing_for_policy_admitted(range.clone(), Default::default(), &mut admission)
            .unwrap();
        let floor = admission.0.borrow().live;
        let old_work = admission.0.borrow().forwarded;
        admission.0.borrow_mut().cap = floor;
        let mut observer = Observer {
            policy: &mut admission,
            spends: Vec::new(),
            reservations: Vec::new(),
            floor,
        };
        assert!(matches!(
            first.finish_policy_admitted(domain, range, Default::default(), &mut observer),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(observer.policy.0.borrow().live, 0);
        assert_eq!(
            observer.policy.0.borrow().forwarded,
            old_work + observer.spends.iter().sum::<usize>()
        );
        checked = true;
    });
    assert!(checked);
}

#[test]
fn dense_bound_domain_full_metered_refusal_keeps_fixed_limit_and_accepted_prefix() {
    let mut checked = 0;
    oracle::policy_dense_fallback_cases::visit_cases(|parent, descriptor| {
        if descriptor["name"] != "dense/actual_256_repeat_bound/domain" {
            return;
        }
        let old = frozen()["dense_and_fallback_cases"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        let row = old["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "explicit_256_span_full")
            .unwrap();
        assert_eq!(row["output"]["policy"]["work"], 22_033);
        assert_eq!(
            row["output"]["policy"]["contents"]
                .as_array()
                .unwrap()
                .len(),
            512
        );
        let limits = request_limits(&row["request"]["limits"]);
        assert_eq!((limits.maximum_spans, limits.maximum_work), (256, 65_536));
        let mut admission = Policy::<true>::new(usize::MAX);
        assert!(matches!(
            policy_admitted(
                parent,
                samples(&row["request"]["samples"]),
                limits,
                &mut admission
            ),
            Err(PlanError::AudioQueryLimit("structural work"))
        ));
        let state = admission.0.borrow();
        assert_eq!(state.forwarded, 65_536);
        assert_eq!(state.live, 0);
        assert!(state.reservations > 0 && state.peak > 0);
        checked += 1;
    });
    assert_eq!(checked, 1);
}

#[path = "policy_point_path_tests.rs"]
mod point_path;
