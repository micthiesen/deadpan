//! Prepare-only OLD retained fades. Capture only after the current fade checkpoint publishes.
use super::*;
use crate::plan::audio_owners::tests::legacy_snapshot_v1 as snapshot;
use crate::{AudioBoundDomain, AudioOwnerInput, AudioOwnerRetainedInput};
use serde_json::json;
use snapshot::{fade_query_baseline, policy_binding_baseline};
use std::{fs::OpenOptions, io::Write, path::Path};

#[path = "retained_fade_cases.rs"]
pub(in crate::plan) mod cases;

fn limits_value(value: AudioQueryLimits) -> Value {
    json!({"maximum_spans":value.maximum_spans,"maximum_work":value.maximum_work})
}
fn samples_value(value: &Range<AudioSample>) -> Value {
    json!({"start":value.start.0,"end":value.end.0})
}
fn ledger<const M: bool>(policy: &Policy<M>) -> Value {
    let state = policy.0.borrow();
    json!({"live":state.live,"peak":state.peak,"reservations":state.reservations,
        "accepted_work":state.forwarded,"work_callbacks":state.work_calls})
}
fn observation(
    label: &str,
    retained: &AudioOwnerRetainedInput<'_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Value {
    let output = retained.fades(samples.clone(), limits);
    json!({"name":label,"request":{"samples":samples_value(&samples),"limits":limits_value(limits)},
        "output":fade_query_baseline::result_snapshot(&output)})
}
fn window_observations(
    label: &str,
    retained: &AudioOwnerRetainedInput<'_>,
    samples: Range<AudioSample>,
) -> Vec<Value> {
    let defaults = AudioQueryLimits::default();
    let output = retained.fades(samples.clone(), defaults);
    let mut values = vec![json!({"name":format!("{label}/default"),"request":{
        "samples":samples_value(&samples),"limits":limits_value(defaults)},
        "output":fade_query_baseline::result_snapshot(&output)})];
    if let Ok(query) = &output {
        values.push(observation(
            &format!("{label}/literal_returned_work_limit"),
            retained,
            samples.clone(),
            AudioQueryLimits {
                maximum_work: query.work,
                ..defaults
            },
        ));
        if query.work > 0 {
            values.push(observation(
                &format!("{label}/one_short_returned_work_limit"),
                retained,
                samples.clone(),
                AudioQueryLimits {
                    maximum_work: query.work - 1,
                    ..defaults
                },
            ));
        }
        if query.spans.is_empty() {
            values.push(observation(
                &format!("{label}/empty_minimum_positive_span_limit"),
                retained,
                samples.clone(),
                AudioQueryLimits {
                    maximum_spans: 1,
                    ..defaults
                },
            ));
        } else {
            let count = query.spans.len();
            values.push(observation(
                &format!("{label}/literal_output_span_count_limit"),
                retained,
                samples.clone(),
                AudioQueryLimits {
                    maximum_spans: count,
                    ..defaults
                },
            ));
            if count > 1 {
                values.push(observation(
                    &format!("{label}/one_short_positive_output_span_limit"),
                    retained,
                    samples,
                    AudioQueryLimits {
                        maximum_spans: count - 1,
                        ..defaults
                    },
                ));
            }
        }
    }
    values
}

fn mapped_support(
    retained: &AudioOwnerRetainedInput<'_>,
    at: AudioSample,
) -> Result<Option<Range<AudioSample>>, PlanError> {
    let reference = match retained.domain() {
        AudioBoundDomain::Root(domain) => domain.root_samples(),
        AudioBoundDomain::Point(domain) => {
            let samples = domain.reference_samples();
            AudioSample(samples.start.0)..AudioSample(samples.end.0)
        }
        AudioBoundDomain::Empty => return Ok(None),
    };
    let q = retained.reference_at(at)?;
    let step = retained.reference_samples_per_output_sample();
    let output = |reference: AudioSample| -> Result<AudioSample, PlanError> {
        let delta = ExactRatio::integer(reference.0)
            .checked_sub(q)?
            .checked_div(step)?
            .ceil()?;
        let value = i128::from(at.0)
            .checked_add(delta)
            .ok_or(TimeError::Overflow)?;
        Ok(AudioSample(
            i64::try_from(value).map_err(|_| TimeError::Overflow)?,
        ))
    };
    Ok(Some(output(reference.start)?..output(reference.end)?))
}

fn retained_body(
    input: &AudioOwnerInput<'_>,
    retained: &AudioOwnerRetainedInput<'_>,
    extra: &[(&str, i64, i64)],
    caller: Option<&Rc<RefCell<PolicyState>>>,
) -> Value {
    let at = input.observed_at();
    let delivery = input.context().root_samples();
    let support = mapped_support(retained, at);
    let mut windows = vec![
        ("current_delivery".to_owned(), delivery.clone()),
        (
            "observed_sample".to_owned(),
            at..AudioSample(at.0.checked_add(1).unwrap()),
        ),
        ("empty_at_observation".to_owned(), at..at),
        (
            "delivery_before_and_after".to_owned(),
            AudioSample(delivery.start.0.checked_sub(1).unwrap())
                ..AudioSample(delivery.end.0.checked_add(1).unwrap()),
        ),
    ];
    let support_value = match support {
        Ok(Some(support)) => {
            windows.push(("mapped_reference_support".into(), support.clone()));
            windows.push((
                "reference_support_before_and_after".into(),
                AudioSample(support.start.0.checked_sub(1).unwrap())
                    ..AudioSample(support.end.0.checked_add(1).unwrap()),
            ));
            windows.push((
                "before_reference_support_sample".into(),
                AudioSample(support.start.0.checked_sub(1).unwrap())..support.start,
            ));
            windows.push((
                "after_reference_support_sample".into(),
                support.end..AudioSample(support.end.0.checked_add(1).unwrap()),
            ));
            json!({"result":"Some","samples":samples_value(&support),
                "derivation":"actual reference domain endpoints inverted by retained.reference_at(observed_at) and actual step; request labels only"})
        }
        Ok(None) => json!({"result":"None","reason":"actual cached raw domain is Empty"}),
        Err(error) => json!({"result":"Error","error":snapshot::error_json(&error)}),
    };
    let width = i128::from(delivery.end.0) - i128::from(delivery.start.0);
    if width > 2 {
        windows.push((
            "clipped_current_delivery".into(),
            AudioSample(delivery.start.0 + 1)..AudioSample(delivery.end.0 - 1),
        ));
    }
    if width > 1 {
        let midpoint =
            AudioSample(i64::try_from(i128::from(delivery.start.0) + width / 2).unwrap());
        windows.push(("adjacent_delivery_left".into(), delivery.start..midpoint));
        windows.push(("adjacent_delivery_right".into(), midpoint..delivery.end));
        windows.push((
            "delivery_seam_sample".into(),
            midpoint..AudioSample(midpoint.0 + 1),
        ));
    }
    windows.extend(extra.iter().map(|(name, start, end)| {
        (
            format!("checked_fixture/{name}"),
            AudioSample(*start)..AudioSample(*end),
        )
    }));
    let mut observations = Vec::new();
    for (name, samples) in windows {
        observations.extend(window_observations(&name, retained, samples));
    }
    let defaults = AudioQueryLimits::default();
    let reversed = AudioSample(at.0.checked_add(1).unwrap())..at;
    let ordinary_range = delivery.clone();
    for (name, samples, limits) in [
        ("reversed", reversed.clone(), defaults),
        (
            "invalid_zero_work",
            ordinary_range.clone(),
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "invalid_zero_spans",
            ordinary_range.clone(),
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
        (
            "invalid_above_work_cap",
            ordinary_range.clone(),
            AudioQueryLimits {
                maximum_work: defaults.maximum_work + 1,
                ..defaults
            },
        ),
        (
            "invalid_above_span_cap",
            ordinary_range.clone(),
            AudioQueryLimits {
                maximum_spans: defaults.maximum_spans + 1,
                ..defaults
            },
        ),
        (
            "invalid_work_before_reversed",
            reversed.clone(),
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "invalid_spans_before_reversed",
            reversed,
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
        (
            "empty_minimum_positive_limits",
            at..at,
            AudioQueryLimits {
                maximum_work: 1,
                maximum_spans: 1,
            },
        ),
        (
            "empty_invalid_zero_work",
            at..at,
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "empty_invalid_zero_spans",
            at..at,
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
        (
            "tiny_positive_work",
            ordinary_range.clone(),
            AudioQueryLimits {
                maximum_work: 1,
                ..defaults
            },
        ),
        (
            "tiny_positive_spans",
            ordinary_range,
            AudioQueryLimits {
                maximum_spans: 1,
                ..defaults
            },
        ),
        (
            "extreme_signed_before",
            AudioSample(i64::MIN)..AudioSample(i64::MIN + 1),
            defaults,
        ),
        (
            "extreme_signed_after",
            AudioSample(i64::MAX - 1)..AudioSample(i64::MAX),
            defaults,
        ),
    ] {
        observations.push(observation(name, retained, samples, limits));
    }
    admission_tests::observe(input, retained, &observations, caller);
    copy_tests::observe(input, retained, &observations);
    overlap_tests::observe(input, retained, &observations);
    json!({"retained":retained.capture_baseline_snapshot(input.context().plan),
        "literal_bound_work_prepaid_per_span":retained.bound().work(),
        "retained_input_work":retained.work(),
        "current_delivery":samples_value(&delivery),"mapped_support":support_value,
        "observations":observations})
}

fn ordinary_handle(input: &AudioOwnerInput<'_>, extra: &[(&str, i64, i64)]) -> Value {
    match input.retained_probe_at(input.observed_at(), Default::default()) {
        Err(error) => {
            json!({"mode":"ordinary_probe","result":"Error","error":snapshot::error_json(&error),
            "partial_work":"ordinary failed retained probes expose no accepted partial-work receipt"})
        }
        Ok(probe) => {
            let body = probe
                .input()
                .map(|retained| retained_body(input, retained, extra, None));
            json!({"mode":"ordinary_probe","result":if probe.input().is_some() {"Some"} else {"None"},
                "probe":{"work":probe.work()},"input":body})
        }
    }
}

fn admitted_handle<const M: bool>(
    input: &AudioOwnerInput<'_>,
    extra: &[(&str, i64, i64)],
) -> Value {
    let mut policy = Policy::<M>::new(if M { usize::MAX } else { 0 });
    let mode = if M {
        "actual_admitted_probe_metered"
    } else {
        "actual_admitted_probe_unmetered"
    };
    let result = match input.retained_probe_at_admitted(
        input.observed_at(),
        Default::default(),
        &mut policy,
    ) {
        Err(error) => json!({"mode":mode,"result":"Error","error":snapshot::error_json(&error),
            "ledger_after_failure":ledger(&policy)}),
        Ok(probe) => {
            let before = ledger(&policy);
            let body = probe
                .input()
                .map(|retained| retained_body(input, retained, extra, Some(&policy.0)));
            let after = ledger(&policy);
            assert_eq!(
                before, after,
                "ordinary fade calls must not mutate the input-construction policy"
            );
            json!({"mode":mode,"result":if probe.input().is_some() {"Some"} else {"None"},
                "probe":{"work":probe.work(),"canonical":probe.canonical_units(),"additional":probe.additional_units(),"forwarded":probe.was_forwarded()},
                "input":body,"ledger_while_borrowed_before_fades":before,"ledger_while_borrowed_after_fades":after})
        }
    };
    assert_eq!(
        policy.0.borrow().live,
        0,
        "actual admitted probe graph must drop after its last queried borrow"
    );
    let mut result = result;
    result["ledger_after_handle_drop"] = ledger(&policy);
    result
}

fn shared_handle(input: &AudioOwnerInput<'_>, extra: &[(&str, i64, i64)]) -> Value {
    let mut policy = Policy::<true>::new(usize::MAX);
    let at = input.observed_at();
    let result = match input.retained_probe_at_admitted(at, Default::default(), &mut policy) {
        Err(error) => json!({"result":"ProbeError","error":snapshot::error_json(&error)}),
        Ok(probe) => {
            let receipt = json!({"work":probe.work(),"canonical":probe.canonical_units(),"additional":probe.additional_units(),"forwarded":probe.was_forwarded()});
            let before_share = probe
                .input()
                .map(|retained| retained.capture_baseline_snapshot(input.context().plan));
            if let Some(retained) = probe.input() {
                match retained.owners_admitted(
                    at..AudioSample(at.0.checked_add(1).unwrap()),
                    Default::default(),
                    &mut policy,
                ) {
                    Err(error) => {
                        json!({"result":"SelectionError","probe":receipt,"input_before_share":before_share,"error":snapshot::error_json(&error)})
                    }
                    Ok(owners) => {
                        let owner_snapshot = snapshot::wide_baseline::query_snapshot(
                            owners.query(),
                            input.context().plan,
                        );
                        let selected = owners.query().spans().iter().any(|span| {
                            span.owners()
                                .iter()
                                .any(|candidate| input.owner().matches_treatment_owner(candidate))
                        });
                        let selection = json!({"selected":selected,"query":owner_snapshot,"canonical":owners.canonical_units(),"additional":owners.additional_units(),"work":owners.work(),"forwarded":owners.was_forwarded()});
                        if selected {
                            // The actual selection query and its guard remain alive during sharing.
                            let shared =
                                probe.into_shared_admitted(Default::default(), &mut policy);
                            match shared {
                                Err(error) => {
                                    json!({"result":"ShareError","probe":receipt,"input_before_share":before_share,"selection":selection,"error":snapshot::error_json(&error)})
                                }
                                Ok(shared) => {
                                    drop(owners);
                                    let before = ledger(&policy);
                                    let retained =
                                        shared.retained().expect("actual accepted shared Some");
                                    let body =
                                        retained_body(input, retained, extra, Some(&policy.0));
                                    let after = ledger(&policy);
                                    assert_eq!(
                                        before, after,
                                        "ordinary fade calls must preserve the actual shared handle's input ledger"
                                    );
                                    json!({"result":"Some","probe":receipt,"input_before_share":before_share,"selection":selection,
                                        "input":body,"ledger_while_borrowed_before_fades":before,"ledger_while_borrowed_after_fades":after})
                                }
                            }
                        } else {
                            json!({"result":"RejectedSome","probe":receipt,"input_before_share":before_share,"selection":selection,
                                "scope":"actual host membership rejection; fallback Domain construction is outside this producer"})
                        }
                    }
                }
            } else {
                json!({"result":"None","probe":receipt,"input_before_share":before_share,"sharing":"not attempted for absent actual probe"})
            }
        }
    };
    assert_eq!(
        policy.0.borrow().live,
        0,
        "actual shared graph and all selection/probe guards must drop"
    );
    let mut result = result;
    result["mode"] = json!("actual_admitted_probe_selected_then_shared");
    result["ledger_after_handle_drop"] = ledger(&policy);
    result
}

fn one(descriptor: &Value, input: &AudioOwnerInput<'_>, extra: &[(&str, i64, i64)]) -> Value {
    json!({"fixture":descriptor,"setup_input":snapshot::input(input),
        "setup":{"observed_at":input.observed_at().0,"probe_limits":limits_value(Default::default())},
        "handles":[ordinary_handle(input,extra),admitted_handle::<false>(input,extra),
            admitted_handle::<true>(input,extra),shared_handle(input,extra)]})
}
pub(in crate::plan) fn visit_cases(
    mut visit: impl FnMut(&Value, &AudioOwnerInput<'_>, &[(&str, i64, i64)]),
) {
    policy_binding_baseline::visit_additional(|name, document, input, _| {
        visit(
            &json!({"name":format!("existing_additional/{name}"),"source":{
            "path":"src/audio_owners/legacy_snapshot_v1/policy_binding_baseline.rs","symbol":"visit_additional",
            "document":document}}),
            input,
            &[],
        );
    });
    cases::visit_cases(|descriptor, input, extra| visit(descriptor, input, extra));
}

pub(in crate::plan) fn matrix() -> Value {
    let mut cases = Vec::new();
    visit_cases(|descriptor, input, extra| cases.push(one(descriptor, input, extra)));
    json!({"schema":"old-retained-fade-query-baseline-v1",
        "producer":"ordinary-retained-fades-from-exact-ordinary-admitted-and-shared-input-provenance",
        "resource_scope":"Ledger describes actual retained input/probe/selection/sharing construction only; ordinary fade allocation is unmetered at this checkpoint",
        "comparison":"No reconstructed equivalent handle, nested-work normalization, synthetic query, forced Some, or fabricated expected cap",
        "cases":cases})
}

fn body(mut value: Value) -> Value {
    for name in ["variant", "source_sha256", "published_fade_checkpoint"] {
        value.as_object_mut().unwrap().remove(name);
    }
    value
}
#[test]
#[ignore = "manual next-unit OLD producer; wait for fade checkpoint publication, explicit new /tmp path"]
fn capture_old_retained_fade_query_baseline_v1() {
    let target =
        std::env::var("DEADPAN_RETAINED_FADE_OUTPUT").expect("explicit new output required");
    let path = Path::new(&target);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant =
        std::env::var("DEADPAN_RETAINED_FADE_VARIANT").expect("explicit variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_RETAINED_FADE_SOURCE_SHA256")
        .expect("complete actual OLD source manifest identity required");
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_RETAINED_FADE_PUBLISHED_CHECKPOINT")
        .expect("published fade checkpoint required");
    assert!(!checkpoint.trim().is_empty());
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .expect("output must be new");
    let mut value = matrix();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    value["published_fade_checkpoint"] = json!(checkpoint);
    serde_json::to_writer_pretty(&mut output, &value).unwrap();
    output.write_all(b"\n").unwrap();
    output.sync_all().unwrap();
}
#[test]
#[ignore = "external OLD regression scaffold only; no expected body supplied before independent capture"]
fn ordinary_retained_fades_match_explicit_external_old_v1() {
    let path = std::env::var("DEADPAN_RETAINED_FADE_EXPECTED")
        .expect("independent actual OLD capture required");
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(matrix(), body(expected));
}

#[test]
fn ordinary_retained_fades_match_frozen_old_v1() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(matrix(), expected);
}

#[path = "retained_fade_supplement.rs"]
pub(in crate::plan) mod supplement;

#[path = "retained_fade_admission_tests.rs"]
mod admission_tests;

#[path = "retained_fade_copy_tests.rs"]
mod copy_tests;

#[path = "retained_fade_overlap_tests.rs"]
mod overlap_tests;
