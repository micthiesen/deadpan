//! Additive native OLD internal mask paths, real held owners and aggregate pressure.
use super::*;
use crate::stages::{point_content_query_trace, point_suppression, raw_domain_query_trace};
use deadpan_plan::{
    AudioDefinitionSelector, AudioPointDomain, AudioRootPlacement, ReferenceSample,
};
use std::alloc::Layout;

const CHECKPOINT: &str = "ebb779155a776f0c25ffaf407e27a7d5cf6286f3";
const PRIOR_OUTPUT_SHA256: &str =
    "a52d53b51b86311ed3794e38bfeec9b8ce5c3c9027ad8d6ee7b7516c993e18a2";

fn layout<T>(capacity: usize) -> Value {
    let layout = Layout::array::<T>(capacity).unwrap();
    json!({"capacity":capacity,"requested_layout":{"size":layout.size(),"align":layout.align()}})
}
fn structure<T>() -> Value {
    json!({"size":std::mem::size_of::<T>(),"align":std::mem::align_of::<T>()})
}
fn mask<T: Copy>(value: &[Range<T>], capacity: usize, at: fn(T) -> i64) -> Value {
    json!({"len":value.len(),"capacity":capacity,"is_empty":value.is_empty(),
        "storage":layout::<Range<T>>(capacity),
        "values":value.iter().map(|range|[at(range.start),at(range.end)]).collect::<Vec<_>>(),
        "first":value.first().map(|range|[at(range.start),at(range.end)]),
        "last":value.last().map(|range|[at(range.start),at(range.end)]),
        "get_at_len":value.get(value.len()).map(|range|[at(range.start),at(range.end)]),
        "iter_size_hint":value.iter().size_hint(),
        "reverse":value.iter().rev().map(|range|[at(range.start),at(range.end)]).collect::<Vec<_>>()})
}
fn signal_mask(value: &Vec<Range<SignalSample>>) -> Value {
    mask(value, value.capacity(), |at| at.0)
}
fn audio_mask(value: &Vec<Range<AudioSample>>) -> Value {
    mask(value, value.capacity(), |at| at.0)
}
enum Returned {
    Signal(Result<SignalBlock, StageAudioError>),
    Read(Result<ReadBlock, StageAudioError>),
    NotRun(&'static str),
}
fn returned(value: &Returned) -> Value {
    match value {
        Returned::Signal(Ok(value)) => {
            let SignalBlock {
                samples,
                dependencies: deps,
                suppressed,
                relative_depth,
            } = value;
            json!({"status":"Ok","kind":"SignalBlock",
                "f32_bits":samples.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
                "sample_storage":layout::<[f32;2]>(samples.capacity()),
                "dependencies":dependencies(deps),"relative_depth":relative_depth,
                "suppressed":signal_mask(suppressed)})
        }
        Returned::Read(Ok(value)) => {
            let ReadBlock {
                start,
                samples,
                dependencies: deps,
                relative_depth,
                suppressed,
                exhausted,
            } = value;
            json!({"status":"Ok","kind":"ReadBlock","start":start.0,
                "f32_bits":samples.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
                "sample_storage":layout::<[f32;2]>(samples.capacity()),
                "dependencies":dependencies(deps),"relative_depth":relative_depth,
                "suppressed":audio_mask(suppressed),"exhausted":audio_mask(exhausted)})
        }
        Returned::Signal(Err(value)) => {
            json!({"status":"Error","kind":"SignalBlock","error":error(value)})
        }
        Returned::Read(Err(value)) => {
            json!({"status":"Error","kind":"ReadBlock","error":error(value)})
        }
        Returned::NotRun(reason) => json!({"status":"NotRun","reason":reason}),
    }
}
fn public_result(value: &Result<PointDomainAudioBlock, StageAudioError>) -> Value {
    match value {
        Ok(value) => {
            let PointDomainAudioBlock {
                schema_version,
                stage,
                project_id,
                revision_id,
                definition,
                root,
                placement,
                reference_grid,
                reference_samples,
                start,
                samples,
                suppressed,
            } = value;
            json!({"status":"Ok","schema_version":schema_version,"stage":stage,
                "project_id":project_id,"revision_id":revision_id,"definition":definition,"root":root,
                "placement":placement,"reference_grid":reference_grid,"reference_samples":reference_samples,
                "start":start.0,"f32_bits":samples.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
                "sample_storage":layout::<[f32;2]>(samples.capacity()),
                "suppressed":mask(suppressed.as_slice(),suppressed.capacity(),|at:ReferenceSample|at.0)})
        }
        Err(value) => json!({"status":"Error","error":error(value)}),
    }
}
fn receipts() -> Value {
    let (host, overflow) = original_request::flattened_work_trace::host_eviction_units();
    json!({"raw_domain":raw_domain_query_trace::snapshot(),"point_content":point_content_query_trace::snapshot(),
        "owned_output_work":point_suppression::work_trace::snapshot(),
        "original_queries":{"flattened":original_request::flattened_work_trace::snapshot(),
            "processing":original_request::flattened_work_trace::processing_snapshot(),
            "policy":original_request::flattened_work_trace::policy_snapshot(),
            "fade":original_request::flattened_work_trace::fade_snapshot()},
        "global_host_eviction":{"accepted_units_by_lane":host,"overflow":overflow}})
}
fn reset_receipts() {
    original_request::flattened_work_trace::reset();
    raw_domain_query_trace::reset();
    point_content_query_trace::reset();
    point_suppression::work_trace::reset();
}
struct TraceSession;
impl Drop for TraceSession {
    fn drop(&mut self) {
        raw_domain_query_trace::disable();
        point_content_query_trace::disable();
        point_suppression::work_trace::disable();
    }
}
fn controller(value: &StageAudio) -> Value {
    json!({"existing_controller":controller_snapshot(value),
        "owned_account_live_frames":value.owned_suppression.live_frames()})
}
fn observer(value: &Option<point_suppression::Observer>) -> Value {
    match value {
        Some(value) => json!({"status":"Some","live_frames":value.live_frames()}),
        None => json!({"status":"None","live_frames":Value::Null}),
    }
}
struct Provider<'work> {
    inner: ObservedProvider<'work>,
    account: Option<point_suppression::Observer>,
    callback_receipts: Vec<Value>,
}
impl AudioSourceProvider for Provider<'_> {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.callback_receipts
            .push(json!({"receipts":receipts(),"owned_account_observer":observer(&self.account)}));
        self.inner.source(project, revision, asset, cancelled)
    }
}
struct PublicProvider {
    prepared: crate::PreparedSource,
    calls: usize,
    events: Vec<Value>,
}
impl AudioSourceProvider for PublicProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.calls += 1;
        self.events.push(
            json!({"ordinal":self.calls,"project_id":project,"revision_id":revision,"asset":asset,
            "cancelled_before":cancelled.load(Ordering::Relaxed),"outcome":"PreparedSource",
            "prepared_provenance":self.prepared.provenance(),"prepared_index":self.prepared.index(),
            "matrix_layout":self.prepared.matrix_layout(),"receipts":receipts()}),
        );
        Ok(&self.prepared)
    }
}
struct Checked {
    wire: Value,
    document: Result<ProjectDocument, DocumentError>,
    history: Option<Value>,
}
fn checked(children: Vec<NodeId>, mut nodes: BTreeMap<NodeId, BeatNode>) -> Checked {
    let (base, _) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    nodes.insert(id("root"), BeatNode::sequence("Root", children));
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string());
    Checked {
        wire,
        document,
        history: None,
    }
}
fn padded_source() -> BeatNode {
    let mut node = source(FrameRate::new(48_000, 1).unwrap(), 0, 64, 64);
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!("public source helper")
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: ExactRatio::integer(64),
        selection: ExactFrameRange::new(ExactRatio::integer(16), ExactRatio::integer(48)).unwrap(),
    };
    node
}
fn quiet_and_padded() -> Checked {
    checked(
        vec![id("quiet"), id("a")],
        BTreeMap::from([(id("quiet"), hold(64)), (id("a"), padded_source())]),
    )
}
fn bind_point_history(historical: Checked, current: Checked, node: &str, clock: &str) -> Checked {
    let history = historical.wire;
    let mut wire = current.wire;
    let document = (|| {
        let old = historical.document?;
        let current = current.document?;
        let layout = FrozenAudioLayout::capture(&old)?;
        let state = AudioBindingState::new(
            vec![AudioTimingRecord {
                id: AudioTimingId {
                    allocation: revision("timing"),
                    ordinal: 0,
                },
                layout,
            }],
            BTreeMap::from([(
                id(node),
                OwnedAudioBinding {
                    reanchors: Vec::new(),
                    lattice: lattice(
                        node,
                        AudioClockRoot::PreserveInputPointCeil { stage: id(clock) },
                    ),
                    resume: None,
                },
            )]),
        )?;
        wire = serde_json::to_value(current).unwrap();
        wire["audio_bindings"] = serde_json::to_value(state).unwrap();
        ProjectDocument::from_json(&wire.to_string())
    })();
    Checked {
        wire,
        document,
        history: Some(history),
    }
}
fn bound_padded() -> Checked {
    let historical = checked(
        vec![id("stage")],
        BTreeMap::from([
            (id("a"), padded_source()),
            (id("stage"), preserve("a", 64, 128)),
        ]),
    );
    let current = checked(vec![id("a")], BTreeMap::from([(id("a"), padded_source())]));
    bind_point_history(historical, current, "a", "stage")
}
fn preserve_nodes() -> BTreeMap<NodeId, BeatNode> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    BTreeMap::from([
        (id("h"), hold(16)),
        (id("a"), source(rate, 0, 48, 48)),
        (
            id("group"),
            BeatNode::sequence("Group", vec![id("h"), id("a")]),
        ),
        (id("b"), preserve("group", 64, 128)),
    ])
}
fn bound_preserve() -> Checked {
    let mut old_nodes = preserve_nodes();
    old_nodes.insert(id("clock"), preserve("b", 128, 256));
    let historical = checked(vec![id("clock")], old_nodes);
    let current = checked(vec![id("b")], preserve_nodes());
    bind_point_history(historical, current, "b", "clock")
}
fn aggregate(held: bool) -> Checked {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    if held {
        nodes.insert(id("quiet"), hold(64));
        children.push(id("quiet"));
    }
    for index in 0..514 {
        let name = id(&format!("segment-{index:03}"));
        nodes.insert(
            name.clone(),
            if index % 2 == 0 {
                hold(1)
            } else {
                source(rate, 0, 1, 1)
            },
        );
        children.push(name);
    }
    checked(children, nodes)
}
#[derive(Clone, Copy)]
enum Reader {
    Point { node: &'static str, frames: u32 },
    Bound { node: &'static str, frames: u32 },
    Aggregate { direct: bool, start: i64 },
}
struct Case {
    name: &'static str,
    setup: Checked,
    reader: Reader,
    held: bool,
    pressure: bool,
    source: &'static str,
}
fn cases_internal() -> Vec<Case> {
    vec![
        Case {
            name: "private-point-silent-hold",
            setup: quiet_and_padded(),
            reader: Reader::Point {
                node: "quiet",
                frames: 16,
            },
            held: false,
            pressure: false,
            source: "checked public Hold Point; actual private Point reader positive-mask intent",
        },
        Case {
            name: "private-point-source-padding",
            setup: quiet_and_padded(),
            reader: Reader::Point {
                node: "a",
                frames: 32,
            },
            held: false,
            pressure: false,
            source: "checked SelectedPlacement16..48 inside64-frame Source; selected endpoint mask and real WAV0..64",
        },
        Case {
            name: "private-point-genuine-preserve-bound",
            setup: bound_preserve(),
            reader: Reader::Point {
                node: "b",
                frames: 32,
            },
            held: false,
            pressure: false,
            source: "public checked b=Preserve(group[SilentHold16,Source48],64-to128), bound on historical outer Preserve input clock; explicit internal Hold mask",
        },
        Case {
            name: "direct-genuine-preserve-bound-point-transfer",
            setup: bound_preserve(),
            reader: Reader::Bound {
                node: "b",
                frames: 32,
            },
            held: false,
            pressure: false,
            source: "borrow actual Preserve Bound from ordinary public Point query; explicit SilentHold policy survives actual raw Point callback/transfer",
        },
        Case {
            name: "aggregate-514-shared-equalizers",
            setup: aggregate(false),
            reader: Reader::Aggregate {
                direct: false,
                start: 0,
            },
            held: false,
            pressure: false,
            source: "514 checked alternating one-frame SilentHold/Source children; actual general aggregate via shared equalizers",
        },
        Case {
            name: "aggregate-514-shared-input",
            setup: aggregate(false),
            reader: Reader::Aggregate {
                direct: true,
                start: 0,
            },
            held: false,
            pressure: false,
            source: "same514 checked construction; direct shared_input default selection",
        },
        Case {
            name: "held-public-mask-then-private-point",
            setup: quiet_and_padded(),
            reader: Reader::Point {
                node: "a",
                frames: 32,
            },
            held: true,
            pressure: false,
            source: "actual public SilentHold mask kept alive during later private Point/WAV read",
        },
        Case {
            name: "held-public-mask-lower-quota-private-point",
            setup: quiet_and_padded(),
            reader: Reader::Point {
                node: "a",
                frames: 32,
            },
            held: true,
            pressure: true,
            source: "lower resident ceiling to actual held account live frames; retain actual later-read refusal or success",
        },
        Case {
            name: "direct-selected-source-bound-transfer-endpoints",
            setup: bound_padded(),
            reader: Reader::Bound {
                node: "a",
                frames: 32,
            },
            held: false,
            pressure: false,
            source: "checked SelectedPlacement Source binding; raw support clipping may create transfer endpoint masks; actual errors/empty child masks preserved",
        },
        Case {
            name: "held-public-mask-then-aggregate",
            setup: aggregate(true),
            reader: Reader::Aggregate {
                direct: false,
                start: 64,
            },
            held: true,
            pressure: false,
            source: "held public SilentHold mask plus actual514-frame aggregate following64-frame prefix",
        },
    ]
}
fn point<'a>(plan: &'a RenderPlan, node: &str) -> Result<AudioPointDomain<'a>, StageAudioError> {
    let definition = plan.audio_definition(AudioDefinitionSelector::Node { node: id(node) })?;
    Ok(definition.in_point_clock(
        AudioRootPlacement::new(
            ExactRatio::ZERO,
            ExactRatio::ONE,
            ExactRatio::ZERO..definition.exact_duration(),
        )?,
        ExactRatio::ZERO,
    )?)
}
fn point_value(value: &AudioPointDomain<'_>, plan: &RenderPlan) -> Value {
    json!({"belongs_to_expected_plan":value.belongs_to(plan),"definition":value.definition(),"root":value.root(),
        "placement":value.placement(),"reference_grid":value.reference_grid(),"reference_sampling":value.reference_sampling(),
        "reference_samples":value.reference_samples()})
}
fn proof_value(proof: &Option<Result<AudioSignalQuery<'_>, StageAudioError>>) -> Value {
    match proof {
        Some(Ok(value)) => {
            json!({"status":"Ok","literal_work":value.work,"span_capacity":value.spans.capacity(),
            "public_query_inspection":value,
            "content_kinds":value.spans.iter().map(|span|match &span.content {
                AudioSignalContent::Leaf(_)=>"Leaf",AudioSignalContent::Stage(_)=>"Stage",
                AudioSignalContent::Bound(_)=>"Bound",AudioSignalContent::ProjectedStage(_)=>"ProjectedStage",
            }).collect::<Vec<_>>(),
            "scope":"ordinary owning input, outside measured work; public inspection is not a private-field serializer"})
        }
        Some(Err(value)) => json!({"status":"Error","error":error(value)}),
        None => json!({"status":"NotRequested"}),
    }
}
fn case_internal(case: Case) -> Value {
    let _session = TraceSession;
    let mut row = json!({"name":case.name,"source":case.source,"checked_document_wire":case.setup.wire,
        "historical_document":case.setup.history,"wav":cases::wav_descriptor(cases::Wav::Stereo48000),
        "scope":{"internal_masks":"actual existing Vec, unadmitted before this unit",
            "provider_account":"closed observer optional; None is unavailable, never an invented zero",
            "public_budget":"warmup public read owns inaccessible internal ReadWork; no surrogate ledger",
            "coverage":"actual outputs and scalar callbacks decide reached positive-mask paths, no synthesized ranges"}});
    let document = match case.setup.document {
        Ok(value) => value,
        Err(value) => {
            row["setup"] = json!({"status":"DocumentValidationError","error":error(&StageAudioError::from(PlanError::from(value)))});
            return row;
        }
    };
    row["document_before"] = serde_json::to_value(&document).unwrap();
    let plan = match RenderPlan::compile(&document) {
        Ok(value) => Arc::new(value),
        Err(value) => {
            row["setup"] = json!({"status":"CompileError","error":error(&value.into())});
            row["document_after"] = serde_json::to_value(&document).unwrap();
            assert_eq!(row["document_before"], row["document_after"]);
            return row;
        }
    };
    let domain = match case.reader {
        Reader::Point { node, .. } | Reader::Bound { node, .. } => match point(&plan, node) {
            Ok(value) => Some(value),
            Err(value) => {
                row["setup"] = json!({"status":"PointConstructionError","error":error(&value)});
                row["document_after"] = serde_json::to_value(&document).unwrap();
                assert_eq!(row["document_before"], row["document_after"]);
                return row;
            }
        },
        Reader::Aggregate { .. } => None,
    };
    row["setup"] = json!({"status":"Ready","plan":plan_identity(&plan),
        "point":domain.as_ref().map(|value|point_value(value,&plan))});
    let proof = domain.as_ref().map(|domain| {
        let count = match case.reader {
            Reader::Point { frames, .. } | Reader::Bound { frames, .. } => frames,
            _ => 0,
        };
        domain
            .signal_ref()
            .query(
                SignalSample(0)..SignalSample(i64::from(count)),
                query_limits(),
            )
            .map_err(StageAudioError::from)
    });
    row["ordinary_input_query"] = proof_value(&proof);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    row["controller_before_public_warmup"] = controller(&renderer);
    reset_receipts();
    let held = if case.held {
        let warm_domain = match point(&plan, "quiet") {
            Ok(value) => value,
            Err(value) => {
                row["public_warmup"] =
                    json!({"status":"PointConstructionError","error":error(&value)});
                row["document_after"] = serde_json::to_value(&document).unwrap();
                assert_eq!(row["document_before"], row["document_after"]);
                return row;
            }
        };
        let mut provider = PublicProvider {
            prepared: cases::prepared(cases::Wav::Stereo48000),
            calls: 0,
            events: Vec::new(),
        };
        let result = renderer.read_point_domain(
            &mut provider,
            &warm_domain,
            ReferenceSample(16),
            16,
            TIMEOUT,
            &AtomicBool::new(false),
        );
        row["public_warmup"] = json!({"request":{"start":16,"frames":16},"result_after_internal_work_drop":public_result(&result),
            "provider_calls":provider.calls,"provider_events":provider.events,"receipts":receipts(),
            "controller_result_alive":controller(&renderer)});
        Some(result)
    } else {
        None
    };
    if case.pressure {
        let live = renderer.owned_suppression.live_frames();
        if live > 0 {
            renderer.limits.maximum_resident_frames = u32::try_from(live).unwrap();
            row["pressure"] = json!({"derived_actual_held_account_live_frames":live,
                "maximum_resident_frames":renderer.limits.maximum_resident_frames,
                "action":"lower existing resident cap only; no accepted work counters changed"});
        } else {
            row["pressure"] = json!({"status":"NotApplied","reason":"actual public warmup left no live owned account"});
        }
    }
    reset_receipts();
    row["receipts_before_internal"] = receipts();
    let before_account = renderer.owned_suppression.observer();
    let result = {
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&cancelled, &work);
        let mut provider = Provider {
            inner: ObservedProvider {
                prepared: cases::prepared(cases::Wav::Stereo48000),
                calls: 0,
                events: Vec::new(),
                work: &work,
                fault: Fault::None,
            },
            account: before_account.as_ref().cloned(),
            callback_receipts: Vec::new(),
        };
        row["work_before"] = work_snapshot(&work);
        row["controller_before_internal"] = controller(&renderer);
        let result = match case.reader {
            Reader::Point { frames, .. } => {
                let domain = domain.as_ref().unwrap();
                let start = domain.reference_samples().start;
                row["internal_request"] = json!({"reader":"read_point_domain_controlled","start":start.0,"frames":frames,"depth":0});
                Returned::Signal(renderer.read_point_domain_controlled(
                    &mut provider,
                    domain,
                    start,
                    frames,
                    control,
                    0,
                ))
            }
            Reader::Bound { frames, .. } => {
                if let Some(Ok(query)) = &proof {
                    if let Some(bound) = query.spans.iter().find_map(|span| match &span.content {
                        AudioSignalContent::Bound(value) => Some(value),
                        _ => None,
                    }) {
                        match bound.reference_at_wide_offset(0) {
                            Ok(at) => {
                                row["internal_request"] = json!({"reader":"read_bound_at","at":at,"frames":frames,"depth":0});
                                Returned::Signal(renderer.read_bound_at(
                                    bound,
                                    &mut provider,
                                    at,
                                    (frames, 0),
                                    control,
                                ))
                            }
                            Err(value) => Returned::Signal(Err(value.into())),
                        }
                    } else {
                        Returned::NotRun(
                            "actual ordinary input query contains no Bound; no private replacement constructed",
                        )
                    }
                } else {
                    Returned::NotRun(
                        "actual ordinary input query failed; no private replacement constructed",
                    )
                }
            }
            Reader::Aggregate { direct, start } => {
                row["internal_request"] = json!({"reader":if direct {"read_shared_input"}else{"read_shared_equalizers"},
                    "start":start,"frames":514,"selection":"InputSelection::default"});
                Returned::Read(if direct {
                    renderer.read_shared_input(
                        &mut provider,
                        AudioSample(start),
                        514,
                        control,
                        crate::stages::shared_eq::InputSelection::default(),
                    )
                } else {
                    renderer.read_shared_equalizers(&mut provider, AudioSample(start), 514, control)
                })
            }
        };
        row["result_while_local_work_alive"] = returned(&result);
        row["work_while_result_alive"] = work_snapshot(&work);
        row["controller_while_result_alive"] = controller(&renderer);
        row["receipts_after_internal"] = receipts();
        row["provider_calls"] = json!(provider.inner.calls);
        row["provider_events"] = json!(provider.inner.events);
        row["provider_scalar_receipts"] = json!(provider.callback_receipts);
        row["cancelled_at_end"] = json!(cancelled.load(Ordering::Relaxed));
        drop(provider);
        row["work_after_provider_drop_result_alive"] = work_snapshot(&work);
        result
    };
    let account = renderer.owned_suppression.observer();
    row["result_after_local_work_drop"] = returned(&result);
    assert_eq!(
        row["result_while_local_work_alive"],
        row["result_after_local_work_drop"]
    );
    row["account_after_local_work_drop"] = observer(&account);
    row["controller_after_local_work_drop"] = controller(&renderer);
    if let Some(value) = &held {
        row["held_public_after_internal"] = public_result(value);
    }
    if let Some(value) = &domain {
        row["point_after_internal"] = point_value(value, &plan);
        assert_eq!(row["setup"]["point"], row["point_after_internal"]);
    }
    drop(proof);
    drop(before_account);
    drop(renderer);
    row["result_after_controller_drop"] = returned(&result);
    assert_eq!(
        row["result_while_local_work_alive"],
        row["result_after_controller_drop"]
    );
    row["account_after_controller_drop_internal_result_alive"] = observer(&account);
    if let Some(value) = &held {
        row["held_public_after_controller_drop"] = public_result(value);
    }
    drop(result);
    row["account_after_internal_result_drop"] = observer(&account);
    drop(held);
    row["account_after_public_held_result_drop"] = observer(&account);
    row["final_receipts"] = receipts();
    drop(account);
    row["account_observer_dropped"] = json!(true);
    row["document_after"] = serde_json::to_value(&document).unwrap();
    assert_eq!(row["document_before"], row["document_after"]);
    row
}
fn matrix_internal() -> Value {
    json!({"schema":"old-internal-suppression-paths-v1","published_checkpoint":CHECKPOINT,
        "prior_output_ownership_before":{"path":"tests/fixtures/owned-suppression-output-baseline-v1/snapshot.json","sha256":PRIOR_OUTPUT_SHA256},
        "actual_layouts":{"SignalBlock":structure::<SignalBlock>(),"ReadBlock":structure::<ReadBlock>(),
            "PointDomainAudioBlock":structure::<PointDomainAudioBlock>(),
            "Vec_Signal_Range":structure::<Vec<Range<SignalSample>>>(),
            "Vec_Audio_Range":structure::<Vec<Range<AudioSample>>>(),
            "Signal_Range":structure::<Range<SignalSample>>(),"Audio_Range":structure::<Range<AudioSample>>()},
        "limits":{"MAX_OUTPUT_FRAMES":MAX_OUTPUT_FRAMES,"MAX_EDGE_PREPARATION_FRAMES":MAX_EDGE_PREPARATION_FRAMES},
        "cases":cases_internal().into_iter().map(case_internal).collect::<Vec<_>>(),
        "scope":"before-only actual checked inputs/native outputs and existing scalar traces; no internal owner/account implementation or private mask synthesis"})
}
fn strip_internal(mut value: Value) -> Value {
    for key in ["variant", "source_sha256"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}
#[test]
#[ignore = "manual actual OLD internal suppression path capture; fresh/tmp and pinned source"]
fn capture_old_internal_suppression_paths_v1() {
    let output =
        std::env::var("DEADPAN_INTERNAL_SUPPRESSION_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_INTERNAL_SUPPRESSION_VARIANT").expect("actual variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_INTERNAL_SUPPRESSION_SOURCE_SHA256")
        .expect("actual source manifest");
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_INTERNAL_SUPPRESSION_PUBLISHED_CHECKPOINT")
        .expect("published checkpoint");
    assert_eq!(checkpoint, CHECKPOINT);
    let mut body = matrix_internal();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &body).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
}
#[test]
#[ignore = "external independent complete OLD internal suppression regression"]
fn internal_suppression_paths_match_external_old_v1() {
    let expected = std::env::var("DEADPAN_INTERNAL_SUPPRESSION_EXPECTED")
        .expect("independent immutable OLD body");
    let body: Value = serde_json::from_slice(&std::fs::read(expected).unwrap()).unwrap();
    assert_eq!(matrix_internal(), strip_internal(body));
}
