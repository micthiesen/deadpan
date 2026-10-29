//! Independent OLD span-start observations, before any wide-query certificate.
//! Every measured input is one actual ordinary direct call. Setup is separate.
use super::wide_baseline::Parent as WideParent;
use super::*;
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

fn output_path(path: &Path) -> PathBuf {
    assert!(path.is_absolute(), "span-start output must be absolute");
    let parent = path
        .parent()
        .expect("output parent")
        .canonicalize()
        .unwrap();
    assert!(
        parent.starts_with(Path::new("/tmp").canonicalize().unwrap()),
        "span-start output must remain under canonical /tmp"
    );
    parent.join(path.file_name().expect("output filename"))
}

fn gain_query<'plan>(
    parent: WideParent<'_, 'plan>,
    samples: Range<AudioSample>,
) -> Result<AudioOwnerQuery<'plan>, PlanError> {
    match parent {
        WideParent::Root(plan) => plan.audio_gain_owners(samples, Default::default()),
        WideParent::Domain(domain) => domain.gain_owners(samples, Default::default()),
    }
}

fn support(value: AudioOwnerSupport) -> &'static str {
    match value {
        AudioOwnerSupport::Active => "Active",
        AudioOwnerSupport::Inactive => "Inactive",
    }
}

fn measure(
    parent: WideParent<'_, '_>,
    selected: &AudioOwnerClock<'_>,
    at: AudioSample,
    name: &str,
) -> (Value, Vec<Value>) {
    // The recorder observes this call's actual discarded walk, selected slot,
    // complete seed and available work receipts. No extra input call is made.
    let (result, events) =
        capture_baseline::observe_case(name, "old_span_start_direct_input", || match parent {
            WideParent::Root(plan) => plan.audio_owner_input(selected, at, Default::default()),
            WideParent::Domain(domain) => domain.owner_input(selected, at, Default::default()),
        });
    let result = match result {
        Ok(value) => json!({
            "result":"Ok", "input":input(&value),
            "canonical_total_work":value.work()
        }),
        Err(error) => json!({
            "result":"Error", "error":error_json(&error),
            "canonical_total_work":Value::Null,
            "work_observability":"OLD public errors have no total-work receipt; only actual recorded capture events are available"
        }),
    };
    (result, events)
}

fn matrix() -> Value {
    let wide: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let direct: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/direct-owner-input-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(direct["documents"], wide["documents"]);
    assert_eq!(direct["cases"].as_array().unwrap().len(), 258);
    let mut parents = Vec::new();
    let mut cases = Vec::new();
    let mut active_members = 0;
    let mut inactive_spans = 0;
    let mut ownerless_spans = 0;
    let mut repeated_members = 0;
    let mut retained_members = 0;
    let mut default_gap_members = 0;
    let mut duplicate_wide_members = 0;
    let mut duplicate_point_members = 0;
    let mut direct_crosschecks = 0;
    let mut retained_reference_parents = 0;
    wide_baseline::visit_cases(&wide, |parent, parent_case| {
        let parent_name = parent_case["name"].as_str().unwrap();
        let window = &parent_case["request"]["window"];
        let samples = AudioSample(window["start"].as_i64().unwrap())
            ..AudioSample(window["end"].as_i64().unwrap());
        // One actual wide setup call for this parent. Keep it alive through all
        // measurements, borrowing its actual spans and clocks without clones.
        let observed = gain_query(parent, samples).unwrap();
        let wide_snapshot = query(&observed, parent.plan(), |sample| sample.0);
        assert_eq!(wide_snapshot, parent_case["query"]);
        let actual_parent = match parent {
            WideParent::Root(_) => parent_case["seed_identity"].clone(),
            WideParent::Domain(value) => {
                let physical = domain(value);
                assert_eq!(physical, parent_case["seed_identity"]["measured_domain"]);
                retained_reference_parents += usize::from(
                    parent_case["seed_identity"]["source"]
                        == "audio_owner_input.retained_at.domain.Root",
                );
                json!({"tag":"ActualCheckedPhysicalDomain", "domain":physical,
                    "construction":parent_case["seed_identity"]})
            }
        };
        let mut span_observations = Vec::new();
        for (span_index, span) in observed.spans().iter().enumerate() {
            let at = span.samples().start;
            // An independent actual OLD point gain query, kept separate from
            // the direct API's own internal membership query and work total.
            let point = gain_query(parent, at..AudioSample(at.0.checked_add(1).unwrap()));
            let point_snapshot = match &point {
                Ok(value) => {
                    json!({"result":"Ok", "query":query(value,parent.plan(),|sample|sample.0)})
                }
                Err(error) => json!({"result":"Error", "error":error_json(error),
                    "canonical_total_work":Value::Null,
                    "work_observability":"OLD public query errors have no total-work receipt"}),
            };
            inactive_spans += usize::from(span.support() == AudioOwnerSupport::Inactive);
            ownerless_spans += usize::from(span.owners().is_empty());
            let capture_members =
                span.support() == AudioOwnerSupport::Active && !span.owners().is_empty();
            span_observations.push(json!({
                "span_index":span_index,"at":at.0,"samples":sample_range(span.samples(),|sample|sample.0),
                "support":support(span.support()),"original_is_silent":span.original_is_silent(),
                "ordered_private_clocks":span.owners().iter().map(|owner|clock(owner,parent.plan(),|sample|sample.0)).collect::<Vec<_>>(),
                "separate_point_gain_query":point_snapshot,
                "capture_witness":if capture_members {"one_direct_call_per_actual_member"} else {"non_capture"},
                "non_capture_reason":if span.support()==AudioOwnerSupport::Inactive {Some("Authored skips inactive support before EQ selection")}
                    else if span.owners().is_empty() {Some("actual ownerless support has no selectable member")} else {None},
                "measured_direct_api_calls":if capture_members {span.owners().len()} else {0},
            }));
            if !capture_members {
                continue;
            }
            for (member_index, selected) in span.owners().iter().enumerate() {
                active_members += 1;
                repeated_members += usize::from(!selected.instance.repeats.is_empty());
                retained_members += usize::from(matches!(
                    selected.origin,
                    AudioOwnerClockOrigin::Retained { .. }
                ));
                default_gap_members += usize::from(selected.kind == AudioOwnerKind::DefaultGap);
                // Enumerate exact private matches in actual vector order. The
                // OLD direct kernel scans only the first point query span.
                let wide_matches: Vec<_> = span
                    .owners()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, owner)| owner.same_at(selected, at).then_some(index))
                    .collect();
                let point_matches: Vec<_> = point
                    .as_ref()
                    .ok()
                    .and_then(|query| query.spans().first())
                    .map(|span| {
                        span.owners()
                            .iter()
                            .enumerate()
                            .filter_map(|(index, owner)| {
                                owner.same_at(selected, at).then_some(index)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                duplicate_wide_members += usize::from(wide_matches.len() > 1);
                duplicate_point_members += usize::from(point_matches.len() > 1);
                let first_point_clock = point_matches.first().map(|index| {
                    clock(
                        &point.as_ref().unwrap().spans()[0].owners()[*index],
                        parent.plan(),
                        |sample| sample.0,
                    )
                });
                let name = format!("{parent_name}/span-{span_index}/member-{member_index}");
                let (output, events) = measure(parent, selected, at, &name);
                if output["result"] == "Ok" {
                    assert!(
                        !point_matches.is_empty(),
                        "OLD successful membership must have a first private same_at match"
                    );
                    assert_eq!(output["input"]["owner"], json!(first_point_clock));
                    for event in &events {
                        if event["observation"]["kind"] == "actual_audio_walk_owner_input" {
                            assert_eq!(
                                event["observation"]["selected_observation"],
                                json!(first_point_clock)
                            );
                        }
                    }
                }
                // Cross-check only cases already frozen in direct258. Middle
                // members are new actual executions, never inferred outputs.
                let frozen = direct["cases"].as_array().unwrap().iter().find(|case| {
                    case["case_kind"] == "actual_checked_member"
                        && case["setup"]["parent_case"] == parent_name
                        && case["setup"]["selected_span_index"] == json!(span_index)
                        && case["setup"]["selected_member_index"] == json!(member_index)
                });
                if let Some(frozen) = frozen {
                    assert_eq!(
                        output, frozen["output"],
                        "immutable direct258 output: {name}"
                    );
                    assert_eq!(
                        events
                            .iter()
                            .map(|event| &event["observation"])
                            .collect::<Vec<_>>(),
                        frozen["actual_capture_events"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|event| &event["observation"])
                            .collect::<Vec<_>>(),
                        "immutable direct258 full capture events: {name}"
                    );
                    direct_crosschecks += 1;
                }
                cases.push(json!({
                    "name":name,"parent_case":parent_name,"span_index":span_index,"member_index":member_index,
                    "api":match parent {WideParent::Root(_)=>"RenderPlan::audio_owner_input",WideParent::Domain(_)=>"AudioDomain::owner_input"},
                    "request":{"at":at.0,"selected_private_clock":clock(selected,parent.plan(),|sample|sample.0),
                        "limits":{"maximum_spans":AudioQueryLimits::default().maximum_spans,"maximum_work":AudioQueryLimits::default().maximum_work}},
                    "private_same_at":{"wide_span_matching_member_indices":wide_matches,
                        "point_first_span_matching_member_indices":point_matches,
                        "first_point_match":first_point_clock,
                        "selection_rule":"scan the actual first point span owners in vector order; first private same_at match wins"},
                    "frozen_direct258_case":frozen.map(|case|&case["name"]),
                    "output":output,"actual_capture_events":events,"measured_direct_api_calls":1
                }));
            }
        }
        parents.push(json!({"name":parent_name,"fixture":parent_case["fixture"],
            "actual_parent":actual_parent,"setup_wide_gain_query":wide_snapshot,
            "setup_wide_gain_query_calls":1,"span_observations":span_observations}));
    });
    assert_eq!(parents.len(), 14);
    assert!(
        inactive_spans > 0
            && ownerless_spans > 0
            && repeated_members > 0
            && retained_members > 0
            && default_gap_members > 0
            && retained_reference_parents > 0
    );
    assert_eq!(active_members, cases.len());
    json!({
        "schema":"deadpan.plan.old-span-start-input.v1",
        "scope":{
            "setup":"immutable wide14 documents and actual checked physical parent reconstruction; one separately billed actual wide gain query per parent and one separately observed point gain query per actual span start",
            "measurement":"exactly one ordinary OLD direct owner-input API call per active actual wide span/member at its actual span.samples().start; no compiled metadata mutation",
            "serializers":"unchanged full legacy_snapshot_v1 clock/query/Input/Domain and capture_baseline capture-event/seed serializers",
            "clock_equality":"actual private same_at including Plan identity, definition, instance, kind, gap, play, origin, sampling slope and local value at observation; full affine anchors retained separately",
            "duplicates":"matching indices and first match recorded in actual order; zero duplicate counts mean this fixture corpus did not produce duplicates, not a uniqueness claim",
            "work":"wide and independent point setup receipts are not part of measured direct Input.work; measured Input.work and capture event receipts are actual OLD API results",
            "limits":"ample default limits only; direct258 remains the frozen exact/one-short/error-ordering oracle",
            "limitations":["no candidate API execution","no decoder/DSP/PCM or allocation-policy oracle","no invented work total for public errors",
                "PointCeil retained reference has no physical AudioDomain.owner_input API; its existing valid bound fixture is observed through its actual physical parent"]
        },
        "fixture_sources":["deadpan-plan/tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json",
            "deadpan-plan/tests/fixtures/direct-owner-input-baseline-v1/snapshot.json"],
        "documents":wide["documents"],"parents":parents,"cases":cases,
        "coverage":{"active_member_direct_calls":active_members,"inactive_non_capture_spans":inactive_spans,
            "ownerless_non_capture_spans":ownerless_spans,"nonempty_repeat_members":repeated_members,
            "retained_origin_members":retained_members,"default_gap_members":default_gap_members,
            "retained_reference_physical_domain_parents":retained_reference_parents,"wide_duplicate_match_members":duplicate_wide_members,
            "point_duplicate_match_members":duplicate_point_members,"direct258_full_output_and_capture_crosschecks":direct_crosschecks}
    })
}

#[test]
#[ignore = "parent-owned OLD span-start capture with source and executable pins"]
fn capture_old_span_start_input_baseline_v1() {
    let output = output_path(&PathBuf::from(
        std::env::var_os("DEADPAN_SPAN_START_OUTPUT").expect("explicit new /tmp output required"),
    ));
    let variant = std::env::var("DEADPAN_SPAN_START_VARIANT").expect("explicit variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_SPAN_START_SOURCE_SHA256")
        .expect("explicit source-manifest SHA256 required");
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .expect("span-start oracle output must be a new file");
    let mut document = matrix();
    document["variant"] = json!(variant);
    document["source_sha256"] = json!(source);
    serde_json::to_writer_pretty(&mut file, &document).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

/// Append-only cost supplement. The original OLD producer above stays frozen.
mod metered_point_cost_supplement {
    use super::*;
    use crate::AudioSoundContextAdmission;
    use std::{alloc::Layout, cell::RefCell, rc::Rc};

    const MEMORY_LIMIT_BYTES: usize = 1_048_576;

    #[derive(Default)]
    struct Ledger {
        live: usize,
        peak: usize,
        reserved: usize,
        released: usize,
        reservations: usize,
        merges: usize,
        drops: usize,
        forwarded: usize,
        spend_calls: usize,
    }

    struct MeteredPolicy(Rc<RefCell<Ledger>>);
    struct Lease {
        ledger: Rc<RefCell<Ledger>>,
        bytes: usize,
    }

    impl Drop for Lease {
        fn drop(&mut self) {
            let mut ledger = self.ledger.borrow_mut();
            ledger.live = ledger.live.checked_sub(self.bytes).unwrap();
            ledger.released = ledger.released.checked_add(self.bytes).unwrap();
            ledger.drops += 1;
        }
    }

    impl AudioSoundContextAdmission for MeteredPolicy {
        const METERED: bool = true;
        type Error = PlanError;
        type Issuer = usize;
        type Lease = Lease;

        fn issuer(&self) -> usize {
            Rc::as_ptr(&self.0) as usize
        }

        fn spend(&mut self, work: usize) -> Result<(), PlanError> {
            let mut ledger = self.0.borrow_mut();
            let next = ledger
                .forwarded
                .checked_add(work)
                .ok_or(TimeError::Overflow)?;
            if next > AudioQueryLimits::default().maximum_work {
                return Err(PlanError::AudioQueryLimit("span point cost global work"));
            }
            ledger.forwarded = next;
            ledger.spend_calls += 1;
            Ok(())
        }

        fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
            let mut ledger = self.0.borrow_mut();
            let live = ledger
                .live
                .checked_add(layout.size())
                .ok_or(TimeError::Overflow)?;
            if live > MEMORY_LIMIT_BYTES {
                return Err(PlanError::AudioQueryLimit("span point cost finite memory"));
            }
            ledger.live = live;
            ledger.peak = ledger.peak.max(live);
            ledger.reserved = ledger
                .reserved
                .checked_add(layout.size())
                .ok_or(TimeError::Overflow)?;
            ledger.reservations += 1;
            Ok(Lease {
                ledger: Rc::clone(&self.0),
                bytes: layout.size(),
            })
        }

        fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
            assert!(Rc::ptr_eq(&target.ledger, &self.0) && Rc::ptr_eq(&source.ledger, &self.0));
            target.bytes = target.bytes.checked_add(source.bytes).unwrap();
            source.bytes = 0;
            self.0.borrow_mut().merges += 1;
        }
    }

    fn ledger_snapshot(policy: &MeteredPolicy) -> Value {
        let ledger = policy.0.borrow();
        json!({"live_bytes":ledger.live,"peak_bytes":ledger.peak,
            "cumulative_reserved_bytes":ledger.reserved,"cumulative_released_bytes":ledger.released,
            "reservation_calls":ledger.reservations,"merge_calls":ledger.merges,
            "lease_drop_calls_including_disarmed":ledger.drops,
            "forwarded_work":ledger.forwarded,"spend_calls":ledger.spend_calls})
    }

    fn matrix() -> Value {
        let wide: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
        ))
        .unwrap();
        let frozen: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/span-start-input-baseline-v1/snapshot.json"
        ))
        .unwrap();
        assert_eq!(frozen["documents"], wide["documents"]);
        let mut cases = Vec::new();
        let mut parents = Vec::new();
        let mut inactive = 0;
        let mut ownerless = 0;
        wide_baseline::visit_cases(&wide, |parent, parent_case| {
            let parent_name = parent_case["name"].as_str().unwrap();
            let frozen_parent = frozen["parents"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["name"] == parent_name)
                .unwrap();
            let window = &parent_case["request"]["window"];
            // Parent construction and this actual wide query are SETUP, kept
            // outside the fresh measured policy and its point-only receipt.
            let wide = gain_query(
                parent,
                AudioSample(window["start"].as_i64().unwrap())
                    ..AudioSample(window["end"].as_i64().unwrap()),
            )
            .unwrap();
            let wide_snapshot = query(&wide, parent.plan(), |sample| sample.0);
            assert_eq!(wide_snapshot, parent_case["query"]);
            assert_eq!(wide_snapshot, frozen_parent["setup_wide_gain_query"]);
            parents.push(json!({"name":parent_name,"fixture":parent_case["fixture"],
                "physical_parent_construction":parent_case["seed_identity"],
                "setup_wide_gain_query":wide_snapshot,"setup_wide_gain_query_calls":1}));
            for (span_index, span) in wide.spans().iter().enumerate() {
                let at = span.samples().start;
                let frozen_span = &frozen_parent["span_observations"][span_index];
                assert_eq!(frozen_span["at"], json!(at.0));
                assert_eq!(frozen_span["support"], support(span.support()));
                inactive += usize::from(span.support() == AudioOwnerSupport::Inactive);
                ownerless += usize::from(span.owners().is_empty());
                let mut policy = MeteredPolicy(Rc::new(RefCell::new(Ledger::default())));
                let before = ledger_snapshot(&policy);
                let limits = AudioQueryLimits::default();
                // Exactly one actual OLD admitted one-sample query. No owner
                // selection or Input capture is invoked by this measurement.
                let admitted = match parent {
                    WideParent::Root(plan) => {
                        plan.audio_owner_input_candidates_at_admitted(at, limits, &mut policy)
                    }
                    WideParent::Domain(domain) => {
                        domain.owner_input_candidates_at_admitted(at, limits, &mut policy)
                    }
                };
                let (output, while_guarded) = match admitted {
                    Ok(admitted) => {
                        let actual = query(admitted.query(), parent.plan(), |sample| sample.0);
                        let point = &frozen_span["separate_point_gain_query"];
                        assert_eq!(point["result"], "Ok");
                        assert_eq!(json!(admitted.canonical_units()), point["query"]["work"]);
                        assert_eq!(
                            admitted
                                .canonical_units()
                                .checked_add(admitted.additional_units()),
                            Some(admitted.work())
                        );
                        assert!(admitted.was_forwarded());
                        assert_eq!(policy.0.borrow().forwarded, admitted.work());
                        // Compare every private field and lookup. Only the
                        // metered total-work view differs from unmetered OLD Q.
                        let mut expected = point["query"].clone();
                        expected["work"] = json!(admitted.work());
                        assert_eq!(
                            actual, expected,
                            "frozen point query facts: {parent_name}/{span_index}"
                        );
                        let output = json!({"result":"Ok","query":actual,
                            "Q":{"canonical":admitted.canonical_units(),"additional":admitted.additional_units(),
                                "total":admitted.work(),"was_forwarded":admitted.was_forwarded(),
                                "actual_forwarded_counter":policy.0.borrow().forwarded},
                            "frozen_unmetered_point_canonical":point["query"]["work"]});
                        let live = ledger_snapshot(&policy);
                        assert!(
                            policy.0.borrow().live > 0,
                            "actual query guard retains a finite lease"
                        );
                        drop(admitted);
                        (output, live)
                    }
                    Err(error) => {
                        let live = ledger_snapshot(&policy);
                        (
                            json!({"result":"Error","error":error_json(&error),
                            "Q":{"canonical":Value::Null,"additional":Value::Null,"total":Value::Null,
                                "actual_forwarded_counter":policy.0.borrow().forwarded},
                            "work_observability":"OLD error returns no complete query work receipt; the actual accepted forwarded counter is retained"}),
                            live,
                        )
                    }
                };
                let after = ledger_snapshot(&policy);
                let ledger = policy.0.borrow();
                assert_eq!(
                    ledger.live, 0,
                    "point query drop releases all finite leases"
                );
                assert_eq!(ledger.reserved, ledger.released);
                assert!(ledger.peak <= MEMORY_LIMIT_BYTES);
                assert_eq!(before["forwarded_work"], 0);
                assert_eq!(
                    while_guarded["forwarded_work"], after["forwarded_work"],
                    "Drop refunds no work"
                );
                cases.push(json!({"name":format!("{parent_name}/span-{span_index}"),
                    "parent_case":parent_name,"span_index":span_index,
                    "api":match parent {WideParent::Root(_)=>"RenderPlan::audio_owner_input_candidates_at_admitted",
                        WideParent::Domain(_)=>"AudioDomain::owner_input_candidates_at_admitted"},
                    "request":{"at":at.0,"limits":{"maximum_spans":limits.maximum_spans,"maximum_work":limits.maximum_work}},
                    "wide_support":support(span.support()),"wide_original_is_silent":span.original_is_silent(),
                    "measured_admitted_point_query_calls":1,"measured_input_capture_calls":0,
                    "policy":{"METERED":true,"global_work_limit":limits.maximum_work,"memory_limit_bytes":MEMORY_LIMIT_BYTES},
                    "output":output,"memory":{"before":before,"while_guarded_or_error_return":while_guarded,"after_drop":after}}));
            }
        });
        assert_eq!(parents.len(), 14);
        assert!(inactive > 0 && ownerless > 0);
        json!({"schema":"deadpan.plan.old-span-point-metered-cost.v1",
            "scope":{"measurement":"one existing OLD admitted one-sample candidate query per actual wide14 physical parent/span start; includes inactive and ownerless spans; no measured Input captures",
                "setup":"existing actual parent reconstruction and one actual unmetered wide query per parent are separate from each fresh metered point policy",
                "work":"Q canonical/additional/total come from the returned admitted guard; accepted forwarded work is counted by the actual METERED spend callback; no Qwide or nominal scan subtraction",
                "memory":"finite 1 MiB layout admission, actual RAII leases, merge disarms source, Drop releases live bytes and never refunds accepted work; policy observation/serialization storage is outside query allocation accounting",
                "facts":"unchanged complete private query serializer; compared with frozen OLD point fields, including vector order, support, silence, affine clocks and lookup",
                "errors":"no complete Q receipt is fabricated on errors; accepted forwarded work and released finite leases remain observable",
                "provenance_equality_exclusions":["variant","source_sha256"]},
            "documents":wide["documents"],"parents":parents,"cases":cases,
            "coverage":{"physical_parents":14,"inactive_span_starts":inactive,"ownerless_span_starts":ownerless}})
    }

    #[test]
    #[ignore = "parent-owned OLD metered point-query cost supplement"]
    fn capture_old_span_point_costs_v1() {
        let output = output_path(&PathBuf::from(
            std::env::var_os("DEADPAN_SPAN_POINT_COST_OUTPUT")
                .expect("explicit new /tmp output required"),
        ));
        let variant =
            std::env::var("DEADPAN_SPAN_POINT_COST_VARIANT").expect("explicit variant required");
        assert!(matches!(
            variant.as_str(),
            "main-inherited" | "scoped-published"
        ));
        let source = std::env::var("DEADPAN_SPAN_POINT_COST_SOURCE_SHA256")
            .expect("explicit source-manifest SHA256 required");
        assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .expect("point cost supplement output must be new");
        let mut document = matrix();
        document["variant"] = json!(variant);
        document["source_sha256"] = json!(source);
        serde_json::to_writer_pretty(&mut file, &document).unwrap();
        file.write_all(b"\n").unwrap();
        file.sync_all().unwrap();
    }
}
