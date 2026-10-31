//! OLD semantic matrix with explicit, independently checked new resource fees.
use super::*;
use serde_json::Value;
use std::alloc::Layout;

const GOLDEN: &str = include_str!("../../tests/fixtures/authored-tile-baseline-v1/snapshot.json");

// The actual frozen Node fields, with only its old tile container restored.
// This is a shallow layout witness, never an estimate of BTreeMap node bytes.
#[allow(dead_code)]
struct OldNodeLayout<'plan, 'work> {
    parent: usize,
    context: Context<'plan>,
    boundary: deadpan_plan::AdmittedAudioTreatmentBoundary<
        'plan,
        sound_context_admission::Issuer<'work>,
        sound_context_admission::Lease<'work>,
    >,
    filter: Option<crate::Equalizer>,
    tiles: BTreeMap<i64, Arc<WideBlock>>,
}

fn number(value: &Value, field: &str) -> u64 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("missing numeric {field}: {value}"))
}

fn arc_frames() -> u64 {
    // Independent Arc shell calculation from the pinned two-counter layout.
    let (layout, _) = Layout::new::<[usize; 2]>()
        .extend(Layout::new::<WideBlock>())
        .unwrap();
    u64::try_from(layout.pad_to_align().size())
        .unwrap()
        .div_ceil(8)
}

fn query_fees(trace: &Value, product: &str) -> [u64; 2] {
    for field in ["nested", "invalid", "overflow"] {
        assert_eq!(trace[field], false, "invalid {product} receipt: {trace}");
    }
    assert_eq!(number(trace, "active_depth"), 0);
    assert!(number(trace, "max_active_depth") <= 1);
    ["plan", "cache"].map(|name| {
        let lane = &trace[name];
        assert_eq!(
            number(lane, "constructor_calls"),
            number(lane, "completed_queries")
                .checked_add(number(lane, "failed_queries"))
                .unwrap()
        );
        assert_eq!(
            number(lane, "accepted_work"),
            number(lane, "canonical_units")
                .checked_add(number(lane, "additional_units"))
                .unwrap()
                .checked_add(number(lane, "partial_work"))
                .unwrap()
                .checked_add(number(lane, "host_eviction_work"))
                .unwrap(),
            "actual {product} deltas retain Plan successful/partial work and explicit host eviction work"
        );
        assert_eq!(number(lane, "issuer_checks"), number(lane, "issuer_work"));
        assert_eq!(
            number(lane, "partial_work"),
            0,
            "{product} partial constructor refusal needs separate OLD-work qualification: {trace}"
        );
        number(lane, "additional_units")
            .checked_add(number(lane, "issuer_work"))
            .unwrap()
    })
}

fn host_eviction_receipt(trace: &Value) -> [u64; 2] {
    let lanes = trace["all_host_eviction_work"].as_array().unwrap();
    assert_eq!(lanes.len(), 2);
    [lanes[0].as_u64().unwrap(), lanes[1].as_u64().unwrap()]
}

fn retained_product_receipt(trace: &Value, product: &str) -> [u64; 2] {
    for field in ["nested", "invalid", "overflow"] {
        assert_eq!(
            trace[field], false,
            "invalid retained {product} receipt: {trace}"
        );
    }
    assert_eq!(number(trace, "active_depth"), 0);
    assert!(number(trace, "max_active_depth") <= 1);
    ["plan", "cache"].map(|lane_name| {
        let lane = &trace[lane_name];
        assert_eq!(
            number(lane, "constructor_calls"),
            number(lane, "completed_queries")
                .checked_add(number(lane, "failed_queries"))
                .unwrap(),
            "retained {product} constructor accounting: {trace}"
        );
        let accepted = number(lane, "accepted_work");
        let canonical = number(lane, "canonical_units");
        let additional = number(lane, "additional_units");
        let partial = number(lane, "partial_work");
        let eviction = number(lane, "host_eviction_work");
        assert_eq!(
            accepted,
            canonical
                .checked_add(additional)
                .unwrap()
                .checked_add(partial)
                .unwrap()
                .checked_add(eviction)
                .unwrap(),
            "retained {product} separates forwarded admission from host eviction: {trace}"
        );
        assert_eq!(
            partial, 0,
            "this paired OLD matrix has no accounting for failed retained constructor prefixes: {trace}"
        );
        if number(lane, "completed_queries") == 0 {
            assert_eq!(canonical, 0, "no completed retained query can contribute canonical work");
            assert_eq!(additional, 0, "no completed retained query can contribute additional work");
        }
        additional
    })
}

fn retained_query_additional(trace: &Value, host_evictions: [u64; 2]) -> [u64; 2] {
    let mut additional = [0_u64; 2];
    let mut local_evictions = [0_u64; 2];
    for (field, product) in [
        ("retained_fade_work_trace", "fade"),
        ("retained_owners_work_trace", "owners"),
    ] {
        let receipt = &trace[field];
        let product_additional = retained_product_receipt(receipt, product);
        let product_evictions =
            ["plan", "cache"].map(|lane_name| number(&receipt[lane_name], "host_eviction_work"));
        assert_eq!(
            host_eviction_receipt(receipt),
            host_evictions,
            "retained {product} eviction snapshot is the same global host receipt"
        );
        for lane in 0..2 {
            additional[lane] = additional[lane]
                .checked_add(product_additional[lane])
                .unwrap();
            local_evictions[lane] = local_evictions[lane]
                .checked_add(product_evictions[lane])
                .unwrap();
        }
    }
    for lane in 0..2 {
        assert!(
            local_evictions[lane] <= host_evictions[lane],
            "retained-scope evictions are included in the global receipt"
        );
    }
    additional
}

fn raw_query_additional(trace: &Value, host_evictions: [u64; 2]) -> [u64; 2] {
    for field in ["nested", "invalid", "overflow"] {
        assert_eq!(trace[field], false, "invalid raw-domain receipt: {trace}");
    }
    assert_eq!(number(trace, "active_depth"), 0);
    assert!(number(trace, "max_active_depth") <= 1);
    ["plan", "cache"].map(|name| {
        let lane = &trace[name];
        let completed = number(lane, "completed_queries");
        let ordinary = number(lane, "ordinary_calls");
        let failed = number(lane, "failed_queries");
        assert_eq!(
            number(lane, "constructor_calls"),
            completed
                .checked_add(ordinary)
                .unwrap()
                .checked_add(failed)
                .unwrap()
        );
        assert_eq!(number(lane, "admitted_calls"), completed);
        assert_eq!(
            number(lane, "canonical_units"),
            0,
            "fresh raw reconstruction never rebills Bound/Core canonical work"
        );
        let kinds = lane["kind_counts"].as_array().unwrap();
        assert_eq!(kinds.len(), 3);
        assert_eq!(
            kinds.iter().map(|v| v.as_u64().unwrap()).sum::<u64>(),
            completed.checked_add(ordinary).unwrap()
        );
        let additional = number(lane, "additional_units");
        let partial = number(lane, "partial_work");
        let eviction = number(lane, "host_eviction_work");
        assert_eq!(
            number(lane, "accepted_work"),
            additional
                .checked_add(partial)
                .unwrap()
                .checked_add(eviction)
                .unwrap()
        );
        assert_eq!(
            partial, 0,
            "failed raw-domain prefixes require separate OLD-work qualification: {trace}"
        );
        if completed == 0 {
            assert_eq!(additional, 0);
        }
        assert!(
            eviction <= host_evictions[usize::from(name == "cache")],
            "raw-scope evictions belong to the same global host receipt"
        );
        additional
    })
}

fn assert_raw_pair_except_host(actual: &Value, reference: &Value) {
    assert_eq!(
        actual.as_object().unwrap().len(),
        reference.as_object().unwrap().len()
    );
    for (field, expected) in reference.as_object().unwrap() {
        if matches!(field.as_str(), "plan" | "cache") {
            let actual = &actual[field];
            assert_eq!(
                actual.as_object().unwrap().len(),
                expected.as_object().unwrap().len()
            );
            for (field, expected) in expected.as_object().unwrap() {
                if !matches!(field.as_str(), "accepted_work" | "host_eviction_work") {
                    assert_eq!(&actual[field], expected, "paired raw-domain field {field}");
                }
            }
        } else {
            assert_eq!(&actual[field], expected, "paired raw-domain field {field}");
        }
    }
}

fn reconcile_paired_flat_host_receipts(actual: &mut Value, old: &Value) {
    assert_eq!(
        query_fees(actual, "paired flattened"),
        query_fees(old, "paired reference flattened")
    );
    for lane in ["plan", "cache"] {
        assert_eq!(
            number(&actual[lane], "accepted_work")
                .checked_sub(number(&actual[lane], "host_eviction_work"))
                .unwrap(),
            number(&old[lane], "accepted_work")
                .checked_sub(number(&old[lane], "host_eviction_work"))
                .unwrap(),
        );
        actual[lane]["accepted_work"] = old[lane]["accepted_work"].clone();
        actual[lane]["host_eviction_work"] = old[lane]["host_eviction_work"].clone();
    }
    actual["all_host_eviction_work"] = old["all_host_eviction_work"].clone();
    assert_eq!(
        actual, old,
        "paired flat constructors differ only in witnessed host eviction work"
    );
}

fn assert_ordinary_query_trace(trace: &Value, product: &str) {
    assert_eq!(query_fees(trace, product), [0, 0]);
    assert_eq!(number(trace, "max_active_depth"), 0);
    for name in ["plan", "cache"] {
        for value in trace[name].as_object().unwrap().values() {
            assert_eq!(value.as_u64(), Some(0), "ordinary {product} trace: {trace}");
        }
    }
}

fn take_query_trace(value: &mut Value, field: &str) -> Value {
    value
        .as_object_mut()
        .unwrap()
        .remove(field)
        .unwrap_or_else(|| panic!("missing explicit {field} instrumentation"))
}

fn compare_flattened_counter(actual: &mut Value, ordinary: &Value, imposed: bool) -> [u64; 2] {
    let mut ordinary = ordinary.clone();
    let mut fees = [0_u64; 2];
    let mut paired_host = None;
    for (field, product) in [
        ("flattened_work_trace", "flattened"),
        ("processing_work_trace", "processing"),
        ("policy_work_trace", "policy"),
        ("fade_work_trace", "fade"),
    ] {
        let trace = take_query_trace(actual, field);
        let old_trace = take_query_trace(&mut ordinary, field);
        assert_ordinary_query_trace(&old_trace, product);
        let product_fees = query_fees(&trace, product);
        let host = (
            host_eviction_receipt(&trace),
            host_eviction_receipt(&old_trace),
        );
        if let Some(previous) = paired_host {
            assert_eq!(
                host, previous,
                "all product snapshots share one actual host debit receipt"
            );
        }
        paired_host = Some(host);
        for lane in 0..2 {
            fees[lane] = fees[lane].checked_add(product_fees[lane]).unwrap();
        }
    }
    let (actual_host, ordinary_host) = paired_host.expect("all actual product receipts");
    let retained_query_work = retained_query_additional(actual, actual_host);
    let ordinary_retained_additional = retained_query_additional(&ordinary, ordinary_host);
    assert_eq!(
        ordinary_retained_additional,
        [0, 0],
        "legacy retained constructors forward no added work"
    );
    for field in ["retained_fade_work_trace", "retained_owners_work_trace"] {
        let trace = take_query_trace(actual, field);
        let ordinary_trace = take_query_trace(&mut ordinary, field);
        for lane in ["plan", "cache"] {
            assert_eq!(ordinary_trace[lane]["constructor_calls"], 0);
            assert_eq!(ordinary_trace[lane]["accepted_work"], 0);
        }
        assert_eq!(host_eviction_receipt(&trace), actual_host);
        assert_eq!(host_eviction_receipt(&ordinary_trace), ordinary_host);
    }
    let raw = take_query_trace(actual, "raw_domain_work_trace");
    let ordinary_raw = take_query_trace(&mut ordinary, "raw_domain_work_trace");
    let raw_additional = raw_query_additional(&raw, actual_host);
    assert_eq!(raw_query_additional(&ordinary_raw, ordinary_host), [0, 0]);
    for lane in ["plan", "cache"] {
        assert_eq!(ordinary_raw[lane]["admitted_calls"], 0);
        for field in ["constructor_calls", "failed_queries", "kind_counts"] {
            assert_eq!(
                raw[lane][field], ordinary_raw[lane][field],
                "same raw-domain outcomes on the actual paired reader"
            );
        }
    }
    for (index, field) in ["old_total_controller_work", "cache_plan_work"]
        .into_iter()
        .enumerate()
    {
        let (host, old_host) = paired_host.expect("all actual product receipts");
        let additional = if imposed && index == 0 {
            0
        } else {
            i128::from(fees[index]) + i128::from(host[index]) - i128::from(old_host[index])
                + i128::from(retained_query_work[index])
                + i128::from(raw_additional[index])
        };
        assert_eq!(
            i128::from(number(actual, field)),
            i128::from(number(&ordinary, field)) + additional,
            "only independent Original/retained/raw fees and one global paired host eviction receipt differ: {field}"
        );
        actual[field] = ordinary[field].clone();
    }
    assert_eq!(
        actual, &ordinary,
        "all other Original-query observations remain literal"
    );
    fees
}

fn remove_flattened_traces(observed: &mut Value) {
    for field in [
        "flattened_work_trace",
        "processing_work_trace",
        "policy_work_trace",
        "fade_work_trace",
        "retained_fade_work_trace",
        "retained_owners_work_trace",
        "raw_domain_work_trace",
    ] {
        for phase in ["before", "after"] {
            take_query_trace(&mut observed["setup"][phase], field);
        }
        for operation in observed["operations"].as_array_mut().unwrap() {
            for phase in ["before", "after_return_drop"] {
                take_query_trace(&mut operation[phase], field);
            }
        }
        take_query_trace(&mut observed["final"], field);
    }
}

fn compare_flattened_matrix(actual: &mut Value, ordinary: &Value) {
    let cases = actual.as_array_mut().unwrap();
    let references = ordinary.as_array().unwrap();
    assert_eq!(cases.len(), references.len());
    let mut total_fees = [0_u64; 2];
    let mut raw_additional_witnessed = 0_u64;
    let mut witnessed = [0_u64; 3];
    for (case, reference) in cases.iter_mut().zip(references) {
        assert_eq!(case["name"], reference["name"]);
        let observed = &mut case["observation"];
        let expected = &reference["observation"];
        for phase in ["before", "after"] {
            compare_flattened_counter(
                &mut observed["setup"][phase],
                &expected["setup"][phase],
                false,
            );
        }
        let operations = observed["operations"].as_array_mut().unwrap();
        let expected_operations = expected["operations"].as_array().unwrap();
        assert_eq!(operations.len(), expected_operations.len());
        for (operation, reference) in operations.iter_mut().zip(expected_operations) {
            let imposed = operation
                .get("imposed_controller_work_before_call")
                .is_some();
            if imposed {
                for field in [
                    "flattened_work_trace",
                    "processing_work_trace",
                    "policy_work_trace",
                    "fade_work_trace",
                    "retained_fade_work_trace",
                    "retained_owners_work_trace",
                    "raw_domain_work_trace",
                ] {
                    assert_eq!(
                        operation["before"][field], operation["after_return_drop"][field],
                        "imposed exhaustion performs no new Original query work: {field}"
                    );
                }
            }
            for phase in ["before", "after_return_drop"] {
                compare_flattened_counter(&mut operation[phase], &reference[phase], imposed);
            }
        }
        for name in ["plan", "cache"] {
            for (index, field) in ["completed_queries", "additional_units", "issuer_work"]
                .into_iter()
                .enumerate()
            {
                witnessed[index] = witnessed[index]
                    .checked_add(number(
                        &observed["final"]["flattened_work_trace"][name],
                        field,
                    ))
                    .unwrap();
            }
        }
        for lane in ["plan", "cache"] {
            raw_additional_witnessed = raw_additional_witnessed
                .checked_add(number(
                    &observed["final"]["raw_domain_work_trace"][lane],
                    "additional_units",
                ))
                .unwrap();
        }
        let fees = compare_flattened_counter(&mut observed["final"], &expected["final"], false);
        for index in 0..2 {
            total_fees[index] = total_fees[index].checked_add(fees[index]).unwrap();
        }
        let mut reference = reference.clone();
        remove_flattened_traces(&mut reference["observation"]);
        assert_eq!(
            case, &reference,
            "complete current PCM, cache, provider, policy and error matrix"
        );
    }
    assert!(
        total_fees.into_iter().any(|fee| fee > 0),
        "the corpus must execute admitted flattened work"
    );
    assert!(witnessed.into_iter().all(|units| units > 0));
    assert!(
        raw_additional_witnessed > 0,
        "actual raw-domain construction fees are exercised"
    );
}

fn normalize_counter(actual: &mut Value, old: &Value, imposed: bool, metered: bool) {
    let mut root_host_evictions = None;
    for (field, product) in [
        ("flattened_work_trace", "flattened"),
        ("processing_work_trace", "processing"),
        ("policy_work_trace", "policy"),
        ("fade_work_trace", "fade"),
    ] {
        let trace = take_query_trace(actual, field);
        assert_ordinary_query_trace(&trace, product);
        let host = host_eviction_receipt(&trace);
        if let Some(previous) = root_host_evictions {
            assert_eq!(
                host, previous,
                "all root query traces expose the same global eviction receipt"
            );
        }
        root_host_evictions = Some(host);
    }
    actual
        .as_object_mut()
        .unwrap()
        .remove("domain_policy_trace")
        .expect("actual legacy Domain policy receipts");
    let captures = actual
        .as_object_mut()
        .unwrap()
        .remove("span_start_trace")
        .unwrap();
    assert_eq!(number(&captures, "certified_captures"), 0);
    assert_eq!(number(&captures, "certified_work"), 0);
    let additional_work = number(actual, "observed_new_index_work");
    if !metered {
        assert_eq!(additional_work, 0);
    }
    let root_host_evictions = root_host_evictions.expect("all four root query receipts");
    let retained_query_work = retained_query_additional(actual, root_host_evictions);
    let raw = take_query_trace(actual, "raw_domain_work_trace");
    assert_eq!(raw_query_additional(&raw, root_host_evictions), [0, 0]);
    for lane in ["plan", "cache"] {
        assert_eq!(
            raw[lane]["admitted_calls"], 0,
            "whole Original legacy scope keeps raw reconstruction ordinary"
        );
    }
    let expected_work = number(old, "old_total_controller_work")
        .checked_add(if imposed { 0 } else { additional_work })
        .unwrap()
        .checked_add(retained_query_work[0])
        .unwrap()
        .checked_add(retained_query_work[1])
        .unwrap();
    assert_eq!(
        number(actual, "old_total_controller_work"),
        expected_work,
        "OLD reader/canonical work plus only accepted new index and retained additional work"
    );
    actual["old_total_controller_work"] = old["old_total_controller_work"].clone();
    actual
        .as_object_mut()
        .unwrap()
        .remove("observed_new_index_work");
    actual
        .as_object_mut()
        .unwrap()
        .remove("retained_fade_work_trace")
        .expect("actual retained fade admission receipt");
    actual
        .as_object_mut()
        .unwrap()
        .remove("retained_owners_work_trace")
        .expect("actual retained owner admission receipt");

    assert_eq!(
        number(old, "node_shallow_bytes"),
        std::mem::size_of::<OldNodeLayout<'_, '_>>() as u64
    );
    assert_eq!(
        number(actual, "node_shallow_bytes"),
        std::mem::size_of::<Node<'_, '_>>() as u64
    );
    assert_eq!(
        number(old, "tile_map_shallow_bytes"),
        std::mem::size_of::<BTreeMap<i64, Arc<WideBlock>>>() as u64
    );
    assert_eq!(
        number(actual, "tile_map_shallow_bytes"),
        std::mem::size_of::<tile_index::TileIndex>() as u64
    );
    let capacity = number(actual, "graph_nodes_capacity");
    assert_eq!(capacity, number(old, "graph_nodes_capacity"));
    let old_node_frames = capacity
        .checked_mul(number(old, "node_shallow_bytes"))
        .unwrap()
        .div_ceil(8);
    let new_node_frames = capacity
        .checked_mul(number(actual, "node_shallow_bytes"))
        .unwrap()
        .div_ceil(8);
    assert_eq!(number(old, "graph_node_capacity_frames"), old_node_frames);
    assert_eq!(
        number(actual, "graph_node_capacity_frames"),
        new_node_frames
    );
    let mut added_frames = new_node_frames.checked_sub(old_node_frames).unwrap();
    let nodes = actual["nodes"].as_array_mut().unwrap();
    let old_nodes = old["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), old_nodes.len());
    for (node, old_node) in nodes.iter_mut().zip(old_nodes) {
        assert_eq!(
            old_node["tiles_node_capacity"],
            "unknown: std BTreeMap safe API is opaque"
        );
        let capacity = number(&node["tiles_node_capacity"], "capacity");
        let frames = number(&node["tiles_node_capacity"], "capacity_frames");
        assert!(capacity >= number(node, "tile_entries"));
        let entry_bytes = std::mem::size_of::<(i64, Arc<WideBlock>)>() as u64;
        assert_eq!(
            frames,
            capacity.checked_mul(entry_bytes).unwrap().div_ceil(8)
        );
        added_frames = added_frames
            .checked_add(frames)
            .unwrap()
            .checked_add(
                number(node, "tile_entries")
                    .checked_mul(arc_frames())
                    .unwrap(),
            )
            .unwrap();
        node["tiles_node_capacity"] = old_node["tiles_node_capacity"].clone();
    }
    for field in ["active_frames", "graph_resident"] {
        assert_eq!(
            number(actual, field),
            number(old, field).checked_add(added_frames).unwrap(),
            "exact Node Vec layout delta + actual index Vec capacity + one Arc shell per tile: {field}"
        );
        actual[field] = old[field].clone();
    }
    for field in [
        "graph_node_capacity_frames",
        "node_shallow_bytes",
        "tile_map_shallow_bytes",
    ] {
        actual[field] = old[field].clone();
    }
    assert_eq!(
        actual, old,
        "all other controller/resource/cache fields remain literal"
    );
}

fn compare_matrix<const M: bool>() {
    let old: Value = serde_json::from_str(GOLDEN).unwrap();
    let (mut actual, _) = crate::stages::original_request::with_domain_policy_trace(true, || {
        span_start::with_direct_reference(super::legacy_tile_baseline::observations::<M>)
    });
    let (direct, _) = crate::stages::original_request::with_domain_policy_trace(false, || {
        span_start::with_direct_reference(super::legacy_tile_baseline::observations::<M>)
    });
    let (mut certified, _) = crate::stages::original_request::with_domain_policy_trace(
        false,
        super::legacy_tile_baseline::observations::<M>,
    );
    compare_domain_policy_matrix(&mut direct.clone(), &actual);
    compare_certified_matrix(&mut certified, &direct);
    let (mut ordinary, _) = crate::stages::original_request::with_legacy_original_queries(|| {
        crate::stages::original_request::with_domain_policy_trace(true, || {
            span_start::with_direct_reference(super::legacy_tile_baseline::observations::<M>)
        })
    });
    compare_flattened_matrix(&mut actual, &ordinary);
    let cases = ordinary.as_array_mut().unwrap();
    let old_cases = old["cases"].as_array().unwrap();
    assert_eq!(cases.len(), old_cases.len());
    for (case, old_case) in cases.iter_mut().zip(old_cases) {
        assert_eq!(case["name"], old_case["name"]);
        let observed = &mut case["observation"];
        let expected = &old_case["observation"];
        for phase in ["before", "after"] {
            normalize_counter(
                &mut observed["setup"][phase],
                &expected["setup"][phase],
                false,
                M,
            );
        }
        let operations = observed["operations"].as_array_mut().unwrap();
        let old_operations = expected["operations"].as_array().unwrap();
        assert_eq!(operations.len(), old_operations.len());
        let mut cumulative = 0_u64;
        for (operation, old_operation) in operations.iter_mut().zip(old_operations) {
            let fee = number(operation, "accepted_new_index_work");
            if !M {
                assert_eq!(fee, 0);
            }
            let imposed = operation
                .get("imposed_controller_work_before_call")
                .is_some();
            assert_eq!(
                number(&operation["before"], "observed_new_index_work"),
                cumulative
            );
            cumulative = cumulative.checked_add(fee).unwrap();
            assert_eq!(
                number(&operation["after_return_drop"], "observed_new_index_work"),
                cumulative
            );
            let before_work = number(&operation["before"], "old_total_controller_work");
            let after_work = number(&operation["after_return_drop"], "old_total_controller_work");
            let old_before = number(&old_operation["before"], "old_total_controller_work");
            let old_after = number(
                &old_operation["after_return_drop"],
                "old_total_controller_work",
            );
            assert_eq!(
                after_work.checked_sub(before_work).unwrap(),
                old_after
                    .checked_sub(old_before)
                    .unwrap()
                    .checked_add(fee)
                    .unwrap()
            );
            for phase in ["before", "after_return_drop"] {
                normalize_counter(&mut operation[phase], &old_operation[phase], imposed, M);
            }
            if imposed && M {
                assert_eq!(fee, 0, "failed work admission accepts no new index fee");
                assert_eq!(
                    old_operation["result"]["error"],
                    "Limit(\"nested stage depth\")"
                );
                assert_eq!(
                    operation["result"]["error"],
                    "Limit(\"plan work per read\")"
                );
                // At exhaustion a newly paid lookup may refuse before cached
                // relative-depth validation. Ample-headroom cases above remain literal.
                operation["result"] = old_operation["result"].clone();
            }
            operation
                .as_object_mut()
                .unwrap()
                .remove("accepted_new_index_work");
            assert_eq!(
                operation, old_operation,
                "complete old operation: {}",
                old_operation["label"]
            );
        }
        assert_eq!(
            number(&observed["final"], "observed_new_index_work"),
            cumulative
        );
        normalize_counter(&mut observed["final"], &expected["final"], false, M);
        assert_eq!(
            case, old_case,
            "complete OLD case after proven new resource differences"
        );
    }
}

fn reconcile_domain_policy_receipts(actual: &Value, old: &Value) -> u64 {
    let rows = actual.as_array().unwrap();
    let old_rows = old.as_array().unwrap();
    assert_eq!(rows.len(), old_rows.len(), "actual Domain request sequence");
    let mut saved = 0_u64;
    for (row, old_row) in rows.iter().zip(old_rows) {
        for value in [row, old_row] {
            assert_eq!(
                number(value, "processing_work"),
                number(value, "processing_canonical_work")
                    .checked_add(number(value, "processing_additional_work"))
                    .unwrap(),
                "actual Domain first processing receipt categories"
            );
        }
        assert_eq!(
            row["processing_canonical_work"], old_row["processing_canonical_work"],
            "Domain first-query canonical work remains literal"
        );
        for value in [row, old_row] {
            assert_eq!(
                number(value, "policy_work"),
                number(value, "policy_canonical_work")
                    .checked_add(number(value, "policy_additional_work"))
                    .unwrap(),
                "actual Domain policy receipt categories"
            );
        }
        let removed = number(old_row, "policy_canonical_work")
            .checked_sub(number(row, "policy_canonical_work"))
            .unwrap();
        assert_eq!(
            i128::from(number(row, "policy_work")) - i128::from(number(old_row, "policy_work")),
            i128::from(number(row, "policy_additional_work"))
                - i128::from(number(old_row, "policy_additional_work"))
                - i128::from(removed),
            "policy total changes only by actual additional work and the witnessed removed walk"
        );
        if removed != 0 {
            assert_eq!(row["content_only_eligible"], true);
            assert!(
                row["samples"]["start"].as_i64().unwrap() < row["samples"]["end"].as_i64().unwrap()
            );
            assert_eq!(
                removed,
                number(row, "processing_canonical_work"),
                "only the actual already-paid first canonical processing receipt is removed"
            );
        }
        for field in [
            "visited_nodes",
            "sequence_comparisons",
            "iteration_run_comparisons",
        ] {
            assert_eq!(
                number(&old_row["policy_lookup"], field)
                    .checked_sub(number(&row["policy_lookup"], field))
                    .unwrap(),
                if removed == 0 {
                    0
                } else {
                    number(&row["processing_lookup"], field)
                },
                "literal actual lookup receipt delta: {field}"
            );
        }
        assert_eq!(
            i128::from(number(row, "processing_work"))
                - i128::from(number(old_row, "processing_work")),
            i128::from(number(row, "processing_additional_work"))
                - i128::from(number(old_row, "processing_additional_work")),
            "changed total first-query work is exactly its separately recorded additional work"
        );
        let mut checked = row.clone();
        for field in [
            "policy_work",
            "policy_canonical_work",
            "policy_additional_work",
            "policy_lookup",
            "processing_work",
            "processing_additional_work",
        ] {
            checked[field] = old_row[field].clone();
        }
        assert_eq!(
            &checked, old_row,
            "Domain identity, signed ranges, canonical first query and request order remain literal"
        );
        saved = saved.checked_add(removed).unwrap();
    }
    saved
}

fn compare_domain_policy_counter(actual: &mut Value, old: &Value, imposed: bool) -> u64 {
    let trace = actual
        .as_object_mut()
        .unwrap()
        .remove("domain_policy_trace")
        .unwrap();
    let mut old = old.clone();
    let old_trace = old
        .as_object_mut()
        .unwrap()
        .remove("domain_policy_trace")
        .unwrap();
    let processing = take_query_trace(actual, "processing_work_trace");
    let old_processing = take_query_trace(&mut old, "processing_work_trace");
    let fees = query_fees(&processing, "processing");
    let old_fees = query_fees(&old_processing, "processing reference");
    let policy = take_query_trace(actual, "policy_work_trace");
    let old_policy = take_query_trace(&mut old, "policy_work_trace");
    let policy_fees = query_fees(&policy, "policy");
    let old_policy_fees = query_fees(&old_policy, "policy reference");
    let fade = take_query_trace(actual, "fade_work_trace");
    let old_fade = take_query_trace(&mut old, "fade_work_trace");
    let fade_fees = query_fees(&fade, "fade");
    let old_fade_fees = query_fees(&old_fade, "fade reference");
    let raw = take_query_trace(actual, "raw_domain_work_trace");
    let old_raw = take_query_trace(&mut old, "raw_domain_work_trace");
    let raw_fees = raw_query_additional(&raw, host_eviction_receipt(&processing));
    let old_raw_fees = raw_query_additional(&old_raw, host_eviction_receipt(&old_processing));
    assert_raw_pair_except_host(&raw, &old_raw);
    let removed = reconcile_domain_policy_receipts(&trace, &old_trace);
    for (lane, field) in ["old_total_controller_work", "cache_plan_work"]
        .into_iter()
        .enumerate()
    {
        let host = host_eviction_receipt(&processing);
        let old_host = host_eviction_receipt(&old_processing);
        assert_eq!(host_eviction_receipt(&policy), host);
        assert_eq!(host_eviction_receipt(&old_policy), old_host);
        assert_eq!(host_eviction_receipt(&fade), host);
        assert_eq!(host_eviction_receipt(&old_fade), old_host);
        assert_eq!(host_eviction_receipt(&actual["flattened_work_trace"]), host);
        assert_eq!(
            host_eviction_receipt(&old["flattened_work_trace"]),
            old_host
        );
        let difference = i128::from(fees[lane]) - i128::from(old_fees[lane])
            + i128::from(policy_fees[lane])
            - i128::from(old_policy_fees[lane])
            + i128::from(fade_fees[lane])
            - i128::from(old_fade_fees[lane])
            + i128::from(raw_fees[lane])
            - i128::from(old_raw_fees[lane])
            + i128::from(host[lane])
            - i128::from(old_host[lane]);
        let removed = if lane == 0 { i128::from(removed) } else { 0 };
        let adjustment = if imposed && lane == 0 {
            0
        } else {
            difference - removed
        };
        assert_eq!(
            i128::from(number(actual, field)),
            i128::from(number(&old, field)) + adjustment,
            "only actual policy savings and separately witnessed processing/policy/fade fees differ: {field}"
        );
        actual[field] = old[field].clone();
    }
    reconcile_paired_flat_host_receipts(
        &mut actual["flattened_work_trace"],
        &old["flattened_work_trace"],
    );
    assert_eq!(
        actual, &old,
        "all other controller/resource/cache fields remain literal"
    );
    removed
}

fn remove_domain_policy_traces(observed: &mut Value) {
    for field in [
        "domain_policy_trace",
        "processing_work_trace",
        "policy_work_trace",
        "fade_work_trace",
        "raw_domain_work_trace",
    ] {
        for phase in ["before", "after"] {
            take_query_trace(&mut observed["setup"][phase], field);
        }
        for operation in observed["operations"].as_array_mut().unwrap() {
            for phase in ["before", "after_return_drop"] {
                take_query_trace(&mut operation[phase], field);
            }
        }
        take_query_trace(&mut observed["final"], field);
    }
}

fn compare_domain_policy_matrix(actual: &mut Value, old: &Value) {
    let cases = actual.as_array_mut().unwrap();
    let old_cases = old.as_array().unwrap();
    assert_eq!(cases.len(), old_cases.len());
    let mut removed = 0_u64;
    for (case, old_case) in cases.iter_mut().zip(old_cases) {
        assert_eq!(case["name"], old_case["name"]);
        let observed = &mut case["observation"];
        let expected = &old_case["observation"];
        for phase in ["before", "after"] {
            compare_domain_policy_counter(
                &mut observed["setup"][phase],
                &expected["setup"][phase],
                false,
            );
        }
        let operations = observed["operations"].as_array_mut().unwrap();
        let old_operations = expected["operations"].as_array().unwrap();
        assert_eq!(operations.len(), old_operations.len());
        for (operation, old_operation) in operations.iter_mut().zip(old_operations) {
            let imposed = operation
                .get("imposed_controller_work_before_call")
                .is_some();
            for phase in ["before", "after_return_drop"] {
                compare_domain_policy_counter(
                    &mut operation[phase],
                    &old_operation[phase],
                    imposed,
                );
            }
        }
        removed = removed
            .checked_add(compare_domain_policy_counter(
                &mut observed["final"],
                &expected["final"],
                false,
            ))
            .unwrap();
        let mut expected = old_case.clone();
        remove_domain_policy_traces(&mut expected["observation"]);
        assert_eq!(
            case, &expected,
            "complete native output, PCM bits, suppression, source/cache/depth and errors remain literal"
        );
    }
    assert!(
        removed > 0,
        "actual matrix must exercise Domain policy query reuse"
    );
}

fn compare_certified_counter(actual: &mut Value, direct: &Value, imposed: bool) -> u64 {
    for (field, product) in [
        ("flattened_work_trace", "flattened"),
        ("processing_work_trace", "processing"),
        ("policy_work_trace", "policy"),
        ("fade_work_trace", "fade"),
    ] {
        query_fees(&actual[field], product);
        query_fees(&direct[field], product);
        assert_eq!(
            actual[field], direct[field],
            "certified capture keeps actual Original-query receipts literal: {field}"
        );
    }
    raw_query_additional(
        &actual["raw_domain_work_trace"],
        host_eviction_receipt(&actual["flattened_work_trace"]),
    );
    raw_query_additional(
        &direct["raw_domain_work_trace"],
        host_eviction_receipt(&direct["flattened_work_trace"]),
    );
    assert_eq!(
        actual["raw_domain_work_trace"], direct["raw_domain_work_trace"],
        "certified capture preserves actual raw constructor receipts"
    );
    let captures = actual
        .as_object_mut()
        .unwrap()
        .remove("span_start_trace")
        .unwrap();
    let mut direct = direct.clone();
    let queries = direct
        .as_object_mut()
        .unwrap()
        .remove("span_start_trace")
        .unwrap();
    assert_eq!(number(&queries, "certified_captures"), 0);
    assert_eq!(number(&queries, "certified_work"), 0);
    let saved_queries = number(&queries, "direct_point_queries")
        .checked_sub(number(&captures, "direct_point_queries"))
        .unwrap();
    assert_eq!(saved_queries, number(&captures, "certified_captures"));
    let removed = number(&queries, "direct_point_work")
        .checked_sub(number(&captures, "direct_point_work"))
        .unwrap();
    if saved_queries != 0 {
        assert!(removed > 0);
        assert!(number(&captures, "certified_work") > 0);
    }
    let expected = number(&direct, "old_total_controller_work")
        .checked_sub(if imposed { 0 } else { removed })
        .unwrap();
    assert_eq!(
        number(actual, "old_total_controller_work"),
        expected,
        "only actual direct one-sample query receipt work is removed"
    );
    actual["old_total_controller_work"] = direct["old_total_controller_work"].clone();
    assert_eq!(
        actual, &direct,
        "all other PCM, resource, depth, cache and controller observations remain literal"
    );
    removed
}

fn compare_certified_matrix(actual: &mut Value, direct: &Value) {
    let cases = actual.as_array_mut().unwrap();
    let references = direct.as_array().unwrap();
    assert_eq!(cases.len(), references.len());
    let mut total_removed = 0_u64;
    for (case, reference) in cases.iter_mut().zip(references) {
        assert_eq!(case["name"], reference["name"]);
        if case["name"] == "nested_equalizers" {
            let captures = &case["observation"]["final"]["span_start_trace"];
            assert!(number(captures, "certified_captures") > 0);
            assert!(number(captures, "certified_work") > 0);
        }
        let observed = &mut case["observation"];
        let expected = &reference["observation"];
        for phase in ["before", "after"] {
            assert_eq!(
                compare_certified_counter(
                    &mut observed["setup"][phase],
                    &expected["setup"][phase],
                    false
                ),
                0
            );
        }
        for (operation, old_operation) in observed["operations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(expected["operations"].as_array().unwrap())
        {
            let imposed = operation
                .get("imposed_controller_work_before_call")
                .is_some();
            for phase in ["before", "after_return_drop"] {
                compare_certified_counter(&mut operation[phase], &old_operation[phase], imposed);
            }
            let mut old_operation = old_operation.clone();
            for phase in ["before", "after_return_drop"] {
                old_operation[phase]
                    .as_object_mut()
                    .unwrap()
                    .remove("span_start_trace");
            }
            assert_eq!(
                operation, &old_operation,
                "complete native operation unchanged"
            );
        }
        total_removed = total_removed
            .checked_add(compare_certified_counter(
                &mut observed["final"],
                &expected["final"],
                false,
            ))
            .unwrap();
        let mut reference = reference.clone();
        let expected = &mut reference["observation"];
        for phase in ["before", "after"] {
            expected["setup"][phase]
                .as_object_mut()
                .unwrap()
                .remove("span_start_trace");
        }
        for operation in expected["operations"].as_array_mut().unwrap() {
            for phase in ["before", "after_return_drop"] {
                operation[phase]
                    .as_object_mut()
                    .unwrap()
                    .remove("span_start_trace");
            }
        }
        expected["final"]
            .as_object_mut()
            .unwrap()
            .remove("span_start_trace");
        assert_eq!(
            case, &reference,
            "complete certified case equals actual direct reference"
        );
    }
    assert!(
        total_removed > 0,
        "the matrix must exercise actual removed queries"
    );
}

#[test]
fn false_recursive_kernel_preserves_complete_old_matrix_and_total_work() {
    compare_matrix::<false>();
}

#[test]
fn metered_recursive_kernel_preserves_old_matrix_with_exact_new_index_fees() {
    compare_matrix::<true>();
}

#[test]
fn pending_arc_admits_exact_shell_once_and_clones_share_its_lease() {
    let plan = super::eq_admission_fixture::fixture();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = super::eq_admission_fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(plan);
    let value = WideBlock {
        samples: Vec::new(),
        suppressed: Vec::new(),
        dependencies: Dependencies::new(),
        relative_depth: 0,
    };
    let pending = tile_block::PendingArc::new(&mut renderer, value, control).unwrap();
    assert_eq!(pending.frames(), arc_frames());
    assert_eq!(work.borrow().sound_context_frames, arc_frames());
    let borrowed = pending.value.as_ref().unwrap();
    let clone = Arc::clone(borrowed);
    assert_eq!(Arc::strong_count(borrowed), 2);
    assert_eq!(work.borrow().sound_context_frames, arc_frames());
    drop(clone);
    drop(pending);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn pending_arc_drops_value_before_lease_and_refusal_returns_no_live_charge() {
    struct Probe<'a> {
        ledger: &'a RefCell<ReadWork>,
        seen: &'a std::cell::Cell<Option<u64>>,
    }
    impl Drop for Probe<'_> {
        fn drop(&mut self) {
            self.seen
                .set(Some(self.ledger.borrow().sound_context_frames));
        }
    }
    let plan = super::eq_admission_fixture::fixture();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = super::eq_admission_fixture::control(&cancelled, &work);
    let seen = std::cell::Cell::new(None);
    let mut renderer = StageAudio::new(plan);
    renderer.limits.maximum_resident_frames = 0;
    assert!(matches!(
        tile_block::PendingArc::new(
            &mut renderer,
            Probe {
                ledger: &work,
                seen: &seen
            },
            control
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(seen.get(), Some(0), "refused input is physically dropped");
    assert_eq!(work.borrow().sound_context_frames, 0);
    renderer.limits = StageLimits::default();
    seen.set(None);
    let pending = tile_block::PendingArc::new(
        &mut renderer,
        Probe {
            ledger: &work,
            seen: &seen,
        },
        control,
    )
    .unwrap();
    let charge = pending.frames();
    assert!(charge > 0);
    drop(pending);
    assert_eq!(
        seen.get(),
        Some(charge),
        "value destructor observes its still-live lease"
    );
    assert_eq!(work.borrow().sound_context_frames, 0);
}

fn graph<'plan, 'work>(plan: &'plan RenderPlan) -> Graph<'plan, 'plan, 'work> {
    Graph {
        plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        skip: &[],
    }
}

fn add<'plan, 'work>(
    renderer: &mut StageAudio,
    graph: &mut Graph<'plan, '_, 'work>,
    owner: &AudioOwnerClock<'plan>,
    control: WorkControl<'work>,
) -> usize {
    renderer
        .eq_node(
            graph,
            Program {
                context: &Context::Root,
                stop: None,
                parent: usize::MAX,
                depth: 0,
            },
            owner,
            AudioSample(128),
            super::eq_admission_fixture::stage(owner),
            control,
        )
        .unwrap()
}

#[test]
fn actual_graph_late_publication_work_refusal_restores_filter_and_retries() {
    let plan = super::eq_admission_fixture::fixture();
    let owner = super::eq_admission_fixture::owner(&plan);
    let cancelled = AtomicBool::new(false);
    // First observe the real complete cold call under ample headroom. For this
    // empty index its only new operation is the final new-entry write.
    let measurement = RefCell::new(ReadWork::default());
    let control = super::eq_admission_fixture::control(&cancelled, &measurement);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut measured_graph = graph(&plan);
    let id = add(&mut renderer, &mut measured_graph, &owner, control);
    let before = measurement.borrow().plan_work;
    let (result, fee) = tile_index::with_fee_trace(|| {
        renderer.eq_tile_with_index_work::<true>(
            &mut super::eq_admission_fixture::NoSources(0),
            &mut measured_graph,
            id,
            0,
            control,
            1,
        )
    });
    let block = result.unwrap();
    assert_eq!(fee, 1);
    assert_eq!(Arc::strong_count(&block), 2);
    let required = measurement.borrow().plan_work.checked_sub(before).unwrap();
    let prepared = measurement.borrow().prepared_frames;
    drop(block);
    let resident = measured_graph.resident;
    drop(measured_graph);
    renderer.active_frames -= resident;
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(measurement.borrow().sound_context_frames, 0);

    let work = RefCell::new(ReadWork::default());
    let retry_work = RefCell::new(ReadWork::default());
    let control = super::eq_admission_fixture::control(&cancelled, &work);
    let retry_control = super::eq_admission_fixture::control(&cancelled, &retry_work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut actual_graph = graph(&plan);
    let id = add(&mut renderer, &mut actual_graph, &owner, control);
    let active = renderer.active_frames;
    let resident = actual_graph.resident;
    // An explicit initial resource input leaves every old charge available,
    // and refuses precisely the final one-unit new-entry publication fee.
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ.checked_sub(required).unwrap() + 1;
    let initial_work = work.borrow().plan_work;
    let mut provider = super::eq_admission_fixture::NoSources(0);
    let (refusal, accepted) = tile_index::with_fee_trace(|| {
        renderer.eq_tile_with_index_work::<true>(
            &mut provider,
            &mut actual_graph,
            id,
            0,
            control,
            1,
        )
    });
    assert!(matches!(
        refusal,
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(accepted, 0);
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    assert!(
        work.borrow().plan_work > initial_work,
        "accepted old work is retained"
    );
    assert_eq!(work.borrow().prepared_frames, prepared);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.active_frames, active);
    assert_eq!(actual_graph.resident, resident);
    assert!(actual_graph.nodes[id].tiles.is_empty());
    assert_eq!(actual_graph.nodes[id].tiles.capacity(), 0);
    assert!(actual_graph.nodes[id].filter.is_some());
    assert_eq!(provider.0, 0);
    // Retry gets a fresh explicit request budget; the exhausted budget and its
    // accepted work remain untouched. The existing Graph/filter are reused.
    let retried = renderer
        .eq_tile_with_index_work::<true>(&mut provider, &mut actual_graph, id, 0, retry_control, 1)
        .unwrap();
    assert_eq!(Arc::strong_count(&retried), 2);
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    drop(retried);
    let resident = actual_graph.resident;
    drop(actual_graph);
    renderer.active_frames -= resident;
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(retry_work.borrow().sound_context_frames, 0);
}
