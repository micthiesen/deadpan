//! Post-freeze observations for the independently captured OLD tile matrix.
//! Original producer bytes and executable pins are retained in tile-oracle.
//! Every value still comes from actual renderer calls, with setup labelled.
use super::eq_admission_fixture as fixture;
use super::legacy_node_baseline as baseline;
use super::*;
use deadpan_core::{
    AssetRecord, BeatNode, ColorPolicy, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    PresentationBasis, ProjectDocument, SourceSpan, SourceTimeBase, SourceTimestamp,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::io::{Cursor, Write};

thread_local! {
    static OBSERVED_INDEX_WORK: Cell<usize> = const { Cell::new(0) };
}

fn track_index_work<R>(action: impl FnOnce() -> R) -> (R, usize) {
    let (result, accepted) = tile_index::with_fee_trace(action);
    OBSERVED_INDEX_WORK.with(|value| value.set(value.get().checked_add(accepted).unwrap()));
    (result, accepted)
}
use std::sync::atomic::Ordering;

fn range(value: &Range<AudioSample>) -> [i64; 2] {
    [value.start.0, value.end.0]
}

fn block(value: &WideBlock) -> Value {
    json!({
        "pcm_format": "stereo f64::to_bits (u64), complete output",
        "pcm_bits": value.samples.iter().map(|frame| frame.map(f64::to_bits)).collect::<Vec<_>>(),
        "samples_len": value.samples.len(), "samples_capacity": value.samples.capacity(),
        "suppressed": value.suppressed.iter().map(range).collect::<Vec<_>>(),
        "suppressed_capacity": value.suppressed.capacity(),
        "dependencies": value.dependencies, "dependencies_len": value.dependencies.len(),
        "dependencies_node_capacity": "unknown: std BTreeMap safe API is opaque",
        "relative_depth": value.relative_depth,
    })
}

fn counters(renderer: &StageAudio, graph: &Graph<'_, '_, '_>, control: WorkControl<'_>) -> Value {
    let work = control.work.borrow();
    let captures = span_start::trace();
    json!({
        "old_total_controller_work": work.plan_work, "prepared_frames": work.prepared_frames,
        "observed_new_index_work": OBSERVED_INDEX_WORK.with(Cell::get),
        "domain_policy_trace": crate::stages::original_request::domain_policy_trace(),
        "flattened_work_trace": crate::stages::original_request::flattened_work_trace::snapshot(),
        "span_start_trace": {
            "direct_point_work": captures.direct_point_work,
            "direct_point_queries": captures.direct_point_queries,
            "certified_work": captures.certified_work,
            "certified_captures": captures.certified_captures,
        },
        "prepared_stages": work.prepared_stages, "source_checks": work.source_checks,
        "observed": work.observed, "cache_plan_work": work.cache_plan_work,
        "request_metadata": work.sound_context_frames,
        "active_frames": renderer.active_frames, "graph_resident": graph.resident,
        "graph_nodes_capacity": graph.nodes.capacity(),
        "graph_node_capacity_frames": graph.node_capacity_frames,
        "node_shallow_bytes": std::mem::size_of::<Node<'_, '_>>(),
        "tile_map_shallow_bytes": std::mem::size_of::<tile_index::TileIndex>(),
        "wide_block_shallow_bytes": std::mem::size_of::<WideBlock>(),
        "request_vec_capacities": {"projected": work.projected.capacity(),
            "projected_active": work.projected_active.capacity(),
            "projected_preflight": work.projected_preflight.capacity(),
            "intrinsic_preflight": work.intrinsic_preflight.capacity()},
        "nodes": graph.nodes.iter().enumerate().map(|(id, node)| json!({
            "id": id, "parent": node.parent, "filter_present": node.filter.is_some(),
            "tile_entries": node.tiles.entries().len(),
            "tiles_node_capacity": {"capacity": node.tiles.capacity(), "capacity_frames": node.tiles.capacity_frames()},
            "tiles": node.tiles.entries().iter().map(|(key, block)| json!({
                "key": key, "arc_strong_count": Arc::strong_count(block),
                "samples_len": block.samples.len(), "samples_capacity": block.samples.capacity(),
                "suppressed": block.suppressed.iter().map(range).collect::<Vec<_>>(),
                "suppressed_capacity": block.suppressed.capacity(),
                "dependencies": block.dependencies, "relative_depth": block.relative_depth,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

struct Provider {
    prepared: Option<crate::PreparedSource>,
    expected: Option<AssetRecord>,
    reads: usize,
    context_reads: usize,
}

impl Provider {
    fn tone() -> Self {
        Self {
            prepared: None,
            expected: None,
            reads: 0,
            context_reads: 0,
        }
    }

    fn qualified() -> Self {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
        )
        .unwrap();
        let identity: [u8; 32] = Sha256::digest(&bytes).into();
        let cancelled = AtomicBool::new(false);
        let session = AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(identity, bytes.len() as u64).unwrap(),
            0,
            AudioSessionLimits::default(),
            &cancelled,
        )
        .unwrap();
        let index = session.index().clone();
        assert_eq!(index.valid_samples(), 8197);
        let prepared = crate::PreparedSource::with_layout(
            session,
            &index,
            AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            },
            &cancelled,
        )
        .unwrap();
        let expected = AssetRecord {
            label: "Verified repository stereo PCM fixture".into(),
            content_hash: identity.iter().map(|byte| format!("{byte:02x}")).collect(),
            video: None,
            audio: Some(source_span()),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        };
        Self {
            prepared: Some(prepared),
            expected: Some(expected),
            reads: 0,
            context_reads: 0,
        }
    }
}

impl AudioSourceProvider for Provider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.reads += 1;
        assert!(!cancelled.load(Ordering::Relaxed));
        assert_eq!(project.as_str(), "tile-oracle");
        assert_eq!(revision.as_str(), "revision");
        assert_eq!(asset.as_str(), "media");
        self.prepared
            .as_ref()
            .ok_or_else(|| PreparationError::SourceUnavailable("Tone fixture opened media".into()))
    }

    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.context_reads += 1;
        assert_eq!(Some(expected), self.expected.as_ref());
        self.source(project, revision, asset, cancelled)
    }
}

fn source_span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 8197,
            time_base,
        },
    )
    .unwrap()
}

fn document(nested: bool, asset: Option<&AssetRecord>) -> ProjectDocument {
    let id = |name: &str| NodeId::new(name).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("tile-oracle").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let recipe_plan = fixture::fixture();
    let recipe_owner = fixture::owner(&recipe_plan);
    let recipe = recipe_owner.treatments().unwrap().clone();
    let mut root = BeatNode::sequence("Outer EQ", vec![id("inner")]);
    root.audio_treatments = recipe.clone();
    let mut inner = BeatNode::sequence("Inner EQ", vec![id("voice")]);
    if nested {
        inner.audio_treatments = recipe;
    }
    let voice = BeatNode::hold(
        "Voice",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(if asset.is_some() { 8197 } else { 20_480 }).unwrap(),
            video: HoldVideo::Background,
            audio: asset.map_or(
                HoldAudio::Tone {
                    frequency_hz: 440,
                    level: deadpan_core::GainDb::new(-6000).unwrap(),
                },
                |_| HoldAudio::RoomTone {
                    source: SourceAudio {
                        asset: AssetId::new("media").unwrap(),
                        span: source_span(),
                    },
                },
            ),
        },
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), root),
        (id("inner"), inner),
        (id("voice"), voice),
    ]))
    .unwrap();
    if let Some(asset) = asset {
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            AssetId::new("media").unwrap(),
            asset.clone(),
        )]))
        .unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn tile<'plan, 'work, const M: bool>(
    label: &str,
    renderer: &mut StageAudio,
    provider: &mut Provider,
    graph: &mut Graph<'plan, '_, 'work>,
    request: (usize, i64, usize),
    control: WorkControl<'work>,
) -> Value {
    let (id, key, depth) = request;
    let before = counters(renderer, graph, control);
    let reads = [provider.reads, provider.context_reads];
    let filters = graph
        .nodes
        .iter()
        .map(|node| node.filter.is_some())
        .collect::<Vec<_>>();
    let (result, accepted_index_work) = track_index_work(|| {
        renderer.eq_tile_with_index_work::<M>(provider, graph, id, key, control, depth)
    });
    let output = match &result {
        Ok(value) => json!({ "block": block(value),
            "returned_arc_strong_count": Arc::strong_count(value),
            "same_allocation_as_cache": graph.nodes[id].tiles.entries().iter().find(|(cached_key, _)| *cached_key == key).is_some_and(|(_, cached)| Arc::ptr_eq(cached, value)),
        }),
        Err(error) => json!({"error": format!("{error:?}")}),
    };
    // No returned Arc is retained past this point. The cache counts below are
    // actual counts after the returned clone has been physically dropped.
    drop(result);
    let after = counters(renderer, graph, control);
    json!({"label": label, "actual_eq_tile_calls": 1, "key": key, "depth": depth,
        "before": before, "result": output, "after_return_drop": after,
        "accepted_new_index_work": accepted_index_work,
        "provider_reads_delta": [provider.reads - reads[0], provider.context_reads - reads[1]],
        "existing_filter_presence_preserved": filters.iter().enumerate().all(|(index, present)|
            *present == graph.nodes[index].filter.is_some()),
    })
}

fn read<'plan, 'work, const M: bool>(
    label: &str,
    renderer: &mut StageAudio,
    provider: &mut Provider,
    graph: &mut Graph<'plan, '_, 'work>,
    context: &Context<'plan>,
    request: (i64, u32, usize),
    control: WorkControl<'work>,
) -> Value {
    let (start, frames, depth) = request;
    let before = counters(renderer, graph, control);
    let reads = [provider.reads, provider.context_reads];
    let (result, accepted_index_work) = track_index_work(|| {
        renderer.read_eq_program_with_index_work::<M>(
            provider,
            graph,
            Program {
                context,
                stop: None,
                parent: usize::MAX,
                depth,
            },
            AudioSample(start),
            frames,
            control,
        )
    });
    let output = match result {
        Ok(value) => json!({"block": block(&value)}),
        Err(error) => json!({"error": format!("{error:?}")}),
    };
    json!({"label": label, "actual_read_eq_program_calls": 1, "start": start,
        "frames": frames, "depth": depth, "before": before, "result": output,
        "after_return_drop": counters(renderer, graph, control),
        "accepted_new_index_work": accepted_index_work,
        "provider_reads_delta": [provider.reads - reads[0], provider.context_reads - reads[1]],
    })
}

fn scenario<'plan, const M: bool>(
    plan: &'plan Arc<RenderPlan>,
    incoming: &Context<'plan>,
    owner_name: &str,
    at: i64,
    provider: &mut Provider,
    growth: bool,
    errors: bool,
) -> Value {
    OBSERVED_INDEX_WORK.with(|value| value.set(0));
    crate::stages::original_request::flattened_work_trace::reset();
    span_start::reset_trace();
    crate::stages::original_request::reset_domain_policy_trace();
    let owner = match incoming {
        Context::Retained(input) => input
            .owners(AudioSample(at)..AudioSample(at + 1), Default::default())
            .unwrap()
            .spans()
            .iter()
            .flat_map(|span| span.owners())
            .find(|owner| owner.instance().node.as_str() == owner_name)
            .unwrap()
            .clone(),
        Context::Domain(domain) => domain
            .gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
            .unwrap()
            .spans()
            .iter()
            .flat_map(|span| span.owners())
            .find(|owner| owner.instance().node.as_str() == owner_name)
            .unwrap()
            .clone(),
        Context::Root => baseline::observed(plan, owner_name, at, false),
    };
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut graph = Graph {
        plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        skip: &[],
    };
    let before_setup = counters(&renderer, &graph, control);
    let id = renderer
        .eq_node(
            &mut graph,
            Program {
                context: incoming,
                stop: None,
                parent: usize::MAX,
                depth: 0,
            },
            &owner,
            AudioSample(at),
            fixture::stage(&owner),
            control,
        )
        .unwrap();
    let setup = json!({"actual_eq_node_calls": 1, "observation_query_outside_measured_work": true,
        "before": before_setup, "after": counters(&renderer, &graph, control),
        "source_owner": baseline::clock(plan, &owner, AudioSample(at)),
        "incoming_context": baseline::context(plan, incoming, AudioSample(at)), "id": id});
    let mut operations = Vec::new();
    for key in [-2 * OUTPUT as i64, -(OUTPUT as i64), 0, OUTPUT as i64] {
        operations.push(tile::<M>(
            "cold_tile",
            &mut renderer,
            provider,
            &mut graph,
            (id, key, 1),
            control,
        ));
    }
    for key in [0, -(OUTPUT as i64), 0] {
        operations.push(tile::<M>(
            "repeated_cache_hit",
            &mut renderer,
            provider,
            &mut graph,
            (id, key, 1),
            control,
        ));
    }
    if growth {
        for index in 2..14 {
            operations.push(tile::<M>(
                "ordered_index_growth",
                &mut renderer,
                provider,
                &mut graph,
                (id, index * OUTPUT as i64, 1),
                control,
            ));
        }
    }
    operations.push(read::<M>(
        "read_crosses_start_suppression",
        &mut renderer,
        provider,
        &mut graph,
        incoming,
        (-16, 64, 0),
        control,
    ));
    operations.push(read::<M>(
        "read_warm_actual_owner",
        &mut renderer,
        provider,
        &mut graph,
        incoming,
        (at, 64, 0),
        control,
    ));
    if errors {
        cancelled.store(true, Ordering::Relaxed);
        operations.push(tile::<M>(
            "cancel_before_depth_and_cache_hit",
            &mut renderer,
            provider,
            &mut graph,
            (id, 0, usize::MAX),
            control,
        ));
        operations.push(read::<M>(
            "cancel_before_zero_frames",
            &mut renderer,
            provider,
            &mut graph,
            incoming,
            (0, 0, usize::MAX),
            control,
        ));
        cancelled.store(false, Ordering::Relaxed);
        let expired = WorkControl {
            deadline: Instant::now() - Duration::from_secs(1),
            ..control
        };
        operations.push(tile::<M>(
            "deadline_before_depth_and_cache_hit",
            &mut renderer,
            provider,
            &mut graph,
            (id, 0, usize::MAX),
            expired,
        ));
        operations.push(tile::<M>(
            "depth_before_cache_hit",
            &mut renderer,
            provider,
            &mut graph,
            (id, 0, renderer_depth_limit()),
            control,
        ));
        let relative = graph.nodes[id]
            .tiles
            .entries()
            .iter()
            .find(|(key, _)| *key == 0)
            .unwrap()
            .1
            .relative_depth;
        renderer.limits.maximum_depth = relative;
        operations.push(tile::<M>(
            "cached_relative_depth_refusal",
            &mut renderer,
            provider,
            &mut graph,
            (id, 0, relative),
            control,
        ));
        if relative > 1 {
            // Explicit resource input, not claimed work performed by the DSP.
            // Restore only the injected counter after this isolated probe.
            let saved = work.borrow().plan_work;
            work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ;
            let mut observation = tile::<M>(
                "cached_relative_depth_with_no_plan_work_remaining",
                &mut renderer,
                provider,
                &mut graph,
                (id, 0, relative),
                control,
            );
            observation["imposed_controller_work_before_call"] = json!(MAX_PLAN_WORK_PER_READ);
            observation["resource_probe_requires_no_new_error_parity"] = json!(true);
            assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
            work.borrow_mut().plan_work = saved;
            operations.push(observation);
        }
        renderer.limits = StageLimits::default();
        for key in [i64::MIN, i64::MAX] {
            operations.push(tile::<M>(
                "cold_key_checked_arithmetic",
                &mut renderer,
                provider,
                &mut graph,
                (id, key, 1),
                control,
            ));
        }
        operations.push(read::<M>(
            "range_before_depth",
            &mut renderer,
            provider,
            &mut graph,
            incoming,
            (0, 0, usize::MAX),
            control,
        ));
        operations.push(read::<M>(
            "valid_range_depth_refusal",
            &mut renderer,
            provider,
            &mut graph,
            incoming,
            (0, 1, usize::MAX),
            control,
        ));
        operations.push(read::<M>(
            "read_end_overflow",
            &mut renderer,
            provider,
            &mut graph,
            incoming,
            (i64::MAX, 1, 0),
            control,
        ));
        for refusal in ["input", "output", "prepared", "resident", "cycle"] {
            renderer.limits = StageLimits::default();
            let mut taken = None;
            match refusal {
                "input" => renderer.limits.maximum_input_frames = INPUT as u32 - 1,
                "output" => renderer.limits.maximum_output_frames = OUTPUT as u32 - 1,
                "prepared" => {
                    renderer.limits.maximum_prepared_frames = work.borrow().prepared_frames
                }
                "resident" => {
                    renderer.limits.maximum_resident_frames = renderer.active_frames as u32
                }
                "cycle" => taken = graph.nodes[id].filter.take(),
                _ => unreachable!(),
            }
            operations.push(tile::<M>(
                refusal,
                &mut renderer,
                provider,
                &mut graph,
                (id, 30 * OUTPUT as i64, 1),
                control,
            ));
            if let Some(filter) = taken {
                graph.nodes[id].filter = Some(filter);
            }
        }
        renderer.limits = StageLimits::default();
        operations.push(tile::<M>(
            "retry_after_refusals",
            &mut renderer,
            provider,
            &mut graph,
            (id, 30 * OUTPUT as i64, 1),
            control,
        ));
    }
    let final_state = counters(&renderer, &graph, control);
    let resident = graph.resident;
    drop(graph);
    renderer.active_frames -= resident;
    json!({"setup": setup, "operations": operations, "final": final_state,
        "after_graph_drop": {"active_frames": renderer.active_frames,
            "request_metadata": work.borrow().sound_context_frames},
        "provider_reads_total": [provider.reads, provider.context_reads]})
}

fn renderer_depth_limit() -> usize {
    StageLimits::default().maximum_depth + 1
}

pub(super) fn observations<const M: bool>() -> Value {
    let plain = fixture::fixture();
    let tone_document = document(false, None);
    let tone = Arc::new(RenderPlan::compile(&tone_document).unwrap());
    let nested_document = document(true, None);
    let nested = Arc::new(RenderPlan::compile(&nested_document).unwrap());
    let mut qualified = Provider::qualified();
    let media_document = document(false, qualified.expected.as_ref());
    let media = Arc::new(RenderPlan::compile(&media_document).unwrap());
    let repeat = baseline::frozen("stable_repeat_plays_default_gap_and_overrides", "voice");
    let retained_plan = baseline::frozen("hidden_resumed_retained_domain_vs_live_root", "group");
    let live = baseline::observed(&retained_plan, "group", 1, false);
    let input = retained_plan
        .audio_owner_input(&live, AudioSample(1), Default::default())
        .unwrap();
    let probe = input
        .retained_probe_at(AudioSample(1), Default::default())
        .unwrap();
    let retained = probe
        .into_input()
        .expect("actual existing retained capture")
        .into_shared();
    let domain = Context::Domain(Arc::new(input.context().clone()));
    let cases = json!([
        {"name": "short_tone_suppression_and_errors", "observation": scenario::<M>(&plain, &Context::Root, "root", 128, &mut Provider::tone(), false, true)},
        {"name": "long_tone_btree_growth", "document": tone_document, "observation": scenario::<M>(&tone, &Context::Root, "root", 128, &mut Provider::tone(), true, false)},
        {"name": "nested_equalizers", "document": nested_document, "observation": scenario::<M>(&nested, &Context::Root, "root", 128, &mut Provider::tone(), false, true)},
        {"name": "qualified_roomtone_nonempty_dependencies", "document": media_document, "observation": scenario::<M>(&media, &Context::Root, "root", 128, &mut qualified, false, false)},
        {"name": "repeat_voice_suppression_merge", "observation": scenario::<M>(&repeat, &Context::Root, "voice", 0, &mut Provider::tone(), false, false)},
        {"name": "actual_physical_capture_domain", "observation": scenario::<M>(&retained_plan, &domain, "group", 1, &mut Provider::tone(), false, false)},
        {"name": "actual_retained_capture", "observation": scenario::<M>(&retained_plan, &Context::Retained(retained), "group", 1, &mut Provider::tone(), false, false)},
    ]);
    assert!(
        cases[1]["observation"]["final"]["nodes"][0]["tile_entries"]
            .as_u64()
            .is_some_and(|entries| entries >= 16),
        "actual BTreeMap growth required"
    );
    assert!(
        cases[2]["observation"]["final"]["nodes"]
            .as_array()
            .unwrap()
            .len()
            > 1,
        "actual nested eq_node recursion required"
    );
    assert!(
        cases[2]["observation"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|operation| operation["result"]["block"]["relative_depth"]
                .as_u64()
                .is_some_and(|depth| depth > 1)),
        "actual nested tile depth required"
    );
    assert!(
        cases[3]["observation"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|operation| operation["result"]["block"]["dependencies_len"]
                .as_u64()
                .is_some_and(|len| len > 0)),
        "actual qualified provider must produce nonempty dependencies"
    );
    cases
}

#[test]
#[ignore = "parent-owned OLD tile oracle; source and executable pins required"]
fn produce_legacy_authored_tile_baseline() {
    let output = std::path::PathBuf::from(
        std::env::var_os("DEADPAN_AUTHORED_TILE_OUTPUT")
            .expect("explicit new /tmp output required"),
    );
    assert!(
        output.is_absolute() && (output.starts_with("/tmp") || output.starts_with("/private/tmp"))
    );
    let variant =
        std::env::var("DEADPAN_AUTHORED_TILE_VARIANT").expect("explicit variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source =
        std::env::var("DEADPAN_AUTHORED_TILE_SOURCE_SHA256").expect("source authority required");
    assert!(source.len() == 64 && source.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let (cases, _) = crate::stages::original_request::with_legacy_flattened_queries(|| {
        crate::stages::original_request::with_domain_policy_trace(true, observations::<true>)
    });
    let result = json!({"schema": "AuthoredTileBaselineV1",
        "producer": "postfreeze_eq_tile_and_read_eq_program_observations",
        "variant": variant, "source_sha256": source,
        "provenance_fields_excluded_only_from_cross_variant_equality": ["variant", "source_sha256"],
        "tile_output_frames": OUTPUT, "tile_input_frames": INPUT, "tile_radius": RADIUS,
        "cases": cases,
        "limits": {"btree_node_capacity": "unknown; no stable safe observation API",
            "native_internal_allocations": "opaque; existing preparation bound only",
            "arc_observation": "actual strong counts only; no allocator bytes inferred",
            "controller_work": "actual current counters including all prior admission charges"}});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    file.write_all(serde_json::to_string_pretty(&result).unwrap().as_bytes())
        .unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
