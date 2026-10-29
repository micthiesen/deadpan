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

fn normalize_counter(actual: &mut Value, old: &Value, imposed: bool, metered: bool) {
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
    let expected_work = number(old, "old_total_controller_work")
        .checked_add(if imposed { 0 } else { additional_work })
        .unwrap();
    assert_eq!(
        number(actual, "old_total_controller_work"),
        expected_work,
        "old work plus only accepted new index comparisons/moves"
    );
    actual["old_total_controller_work"] = old["old_total_controller_work"].clone();
    actual
        .as_object_mut()
        .unwrap()
        .remove("observed_new_index_work");

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
    let cases = actual.as_array_mut().unwrap();
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
        let removed = number(old_row, "policy_work")
            .checked_sub(number(row, "policy_work"))
            .unwrap();
        if removed != 0 {
            assert_eq!(row["content_only_eligible"], true);
            assert!(
                row["samples"]["start"].as_i64().unwrap() < row["samples"]["end"].as_i64().unwrap()
            );
            assert_eq!(
                removed,
                number(row, "processing_work"),
                "only the actual already-paid first processing receipt is removed"
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
        let mut checked = row.clone();
        checked["policy_work"] = old_row["policy_work"].clone();
        checked["policy_lookup"] = old_row["policy_lookup"].clone();
        assert_eq!(
            &checked, old_row,
            "Domain identity, signed ranges, first query and request order remain literal"
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
    let removed = reconcile_domain_policy_receipts(&trace, &old_trace);
    assert_eq!(
        number(actual, "old_total_controller_work"),
        number(&old, "old_total_controller_work")
            .checked_sub(if imposed { 0 } else { removed })
            .unwrap(),
        "controller delta equals only cumulative successful actual Domain policy receipts"
    );
    actual["old_total_controller_work"] = old["old_total_controller_work"].clone();
    assert_eq!(
        actual, &old,
        "all other controller/resource/cache fields remain literal"
    );
    removed
}

fn remove_domain_policy_traces(observed: &mut Value) {
    for phase in ["before", "after"] {
        observed["setup"][phase]
            .as_object_mut()
            .unwrap()
            .remove("domain_policy_trace");
    }
    for operation in observed["operations"].as_array_mut().unwrap() {
        for phase in ["before", "after_return_drop"] {
            operation[phase]
                .as_object_mut()
                .unwrap()
                .remove("domain_policy_trace");
        }
    }
    observed["final"]
        .as_object_mut()
        .unwrap()
        .remove("domain_policy_trace");
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
