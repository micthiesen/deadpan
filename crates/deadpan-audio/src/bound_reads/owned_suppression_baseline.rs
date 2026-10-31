//! Additive OLD returned-mask ownership and aggregate-pressure observations.
use super::*;
use deadpan_plan::{
    AudioDefinitionSelector, AudioPointDomain, AudioRootPlacement, ReferenceSample,
};
use std::alloc::Layout;

const CHECKPOINT: &str = "dd5fda5d83205aca0fc3480b7c0dae254224f133";
const PRIOR_NATIVE_SHA256: &str =
    "3d6338e4e4b0f5617647ab6e0fa4d80bf8882a87c87a55efe306002c3011dbdc";

fn structure<T>() -> Value {
    json!({"size":std::mem::size_of::<T>(),"align":std::mem::align_of::<T>()})
}
fn vector_layout<T>(capacity: usize) -> Value {
    let layout = Layout::array::<T>(capacity).unwrap();
    json!({"capacity":capacity,"requested_layout":{"size":layout.size(),"align":layout.align()}})
}
fn sample(at: AudioSample) -> i64 {
    at.0
}
fn signal_sample(at: SignalSample) -> i64 {
    at.0
}
#[expect(
    clippy::iter_nth,
    reason = "The immutable witness records actual iterator helper behavior."
)]
fn mask<T: Copy>(value: &[Range<T>], capacity: usize, at: fn(T) -> i64) -> Value {
    let range = |r: &Range<T>| [at(r.start), at(r.end)];
    let mut iterator = value.iter();
    let initial = iterator.size_hint();
    let first = iterator.next().map(range);
    let last = iterator.next_back().map(range);
    json!({"len":value.len(),"capacity":capacity,"is_empty":value.is_empty(),
        "requested_storage":vector_layout::<Range<T>>(capacity),
        "values":value.iter().map(range).collect::<Vec<_>>(),
        "helpers":{"first":value.first().map(range),"last":value.last().map(range),
            "get_at_len":value.get(value.len()).map(range),
            "forward_prefix":value.iter().take(3).map(range).collect::<Vec<_>>(),
            "reverse_prefix":value.iter().rev().take(3).map(range).collect::<Vec<_>>(),
            "nth_two":value.iter().nth(2).map(range),
            "iterator_initial_size_hint":initial,
            "iterator_first":first,"iterator_last_after_first":last,
            "iterator_remaining_len":iterator.len(),
            "iterator_remaining_size_hint":iterator.size_hint()}})
}
fn consume_mask<T: Copy>(value: Vec<Range<T>>, at: fn(T) -> i64) -> Value {
    let range = |r: Range<T>| [at(r.start), at(r.end)];
    let mut iterator = value.into_iter();
    let initial = json!({"len":iterator.len(),"size_hint":iterator.size_hint(),
        "as_slice":iterator.as_slice().iter().map(|r|[at(r.start),at(r.end)]).collect::<Vec<_>>()});
    let first = iterator.next().map(range);
    let last = iterator.next_back().map(range);
    let remaining = iterator.by_ref().map(range).collect::<Vec<_>>();
    json!({"initial":initial,"first":first,"last_after_first":last,
        "remaining_in_order":remaining,"final_len":iterator.len(),
        "final_size_hint":iterator.size_hint(),"final_as_slice_len":iterator.as_slice().len()})
}
enum Returned {
    Signal(Result<SignalBlock, StageAudioError>),
    Read(Result<ReadBlock, StageAudioError>),
}
fn returned(value: &Returned) -> Value {
    match value {
        Returned::Signal(Ok(block)) => {
            let SignalBlock {
                samples,
                dependencies: deps,
                suppressed,
                relative_depth,
            } = block;
            json!({"status":"Ok","kind":"SignalBlock",
                "f32_bits":samples.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
                "sample_storage":vector_layout::<[f32;2]>(samples.capacity()),
                "dependencies":dependencies(deps),"relative_depth":relative_depth,
                "suppressed":mask(suppressed.as_slice(),suppressed.capacity(),signal_sample)})
        }
        Returned::Read(Ok(block)) => {
            let ReadBlock {
                start,
                samples,
                dependencies: deps,
                relative_depth,
                suppressed,
                exhausted,
            } = block;
            json!({"status":"Ok","kind":"ReadBlock","start":start.0,
                "f32_bits":samples.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
                "sample_storage":vector_layout::<[f32;2]>(samples.capacity()),
                "dependencies":dependencies(deps),"relative_depth":relative_depth,
                "suppressed":mask(suppressed.as_slice(),suppressed.capacity(),sample),"exhausted":mask(exhausted.as_slice(),exhausted.capacity(),sample)})
        }
        Returned::Signal(Err(value)) => {
            json!({"status":"Error","kind":"SignalBlock","error":error(value)})
        }
        Returned::Read(Err(value)) => {
            json!({"status":"Error","kind":"ReadBlock","error":error(value)})
        }
    }
}
fn consume_returned(value: Returned) -> Value {
    match value {
        Returned::Signal(Ok(SignalBlock { suppressed, .. })) => {
            json!({"kind":"SignalBlock","suppressed":consume_mask(suppressed.into_legacy_vector(),signal_sample)})
        }
        Returned::Read(Ok(ReadBlock {
            suppressed,
            exhausted,
            ..
        })) => {
            json!({"kind":"ReadBlock","suppressed":consume_mask(suppressed.into_legacy_vector(),sample),
                "exhausted":consume_mask(exhausted,sample)})
        }
        Returned::Signal(Err(value)) | Returned::Read(Err(value)) => {
            json!({"status":"NoReturnedBlock","error":error(&value)})
        }
    }
}
#[derive(Clone, Copy)]
enum Reader {
    Point {
        node: &'static str,
        offset: i64,
        frames: u32,
    },
    SharedEqualizers,
    SharedInput,
}
struct Case {
    name: &'static str,
    document: Result<ProjectDocument, DocumentError>,
    checked_wire: Value,
    reader: Reader,
    frames: u32,
    source: Value,
}
fn aggregate_document() -> (Value, Result<ProjectDocument, DocumentError>) {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (base, _) = fixture_document_at_rate(rate);
    let names = (0..514)
        .map(|index| format!("segment-{index:03}"))
        .collect::<Vec<_>>();
    let mut nodes = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            (
                id(name),
                if index % 2 == 0 {
                    hold(1)
                } else {
                    source(rate, 0, 1, 1)
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", names.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let checked = ProjectDocument::from_json(&wire.to_string());
    (wire, checked)
}
fn cases_owned() -> Vec<Case> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let silent = document(rate, &["quiet"], vec![("quiet", hold(64))]).0;
    let plain = document(rate, &["a"], vec![("a", source(rate, 0, 64, 64))]).0;
    let fixtures = cases::fixtures();
    let bound = fixtures[1].current.clone();
    let (aggregate_wire, aggregate) = aggregate_document();
    vec![
        Case {
            name: "public-point-silent-hold",
            checked_wire: serde_json::to_value(&silent).unwrap(),
            document: Ok(silent),
            reader: Reader::Point {
                node: "quiet",
                offset: 16,
                frames: 16,
            },
            frames: 16,
            source: json!({"kind":"checked_public_Point","construction":"bound_reads document/hold helpers; public audio_definition/in_point_clock",
                "intent":"actual SilentHold suppression; no inference from zero PCM"}),
        },
        Case {
            name: "public-point-source",
            checked_wire: serde_json::to_value(&plain).unwrap(),
            document: Ok(plain),
            reader: Reader::Point {
                node: "a",
                offset: 16,
                frames: 16,
            },
            frames: 16,
            source: json!({"kind":"checked_public_Point","construction":"bound_reads document/source helpers; public audio_definition/in_point_clock",
                "intent":"genuine source and honest empty-mask result"}),
        },
        Case {
            name: "public-point-genuine-bound",
            checked_wire: serde_json::to_value(&bound).unwrap(),
            document: Ok(bound),
            reader: Reader::Point {
                node: "a",
                offset: 16,
                frames: 16,
            },
            frames: 16,
            source: json!({"kind":"unchanged_checked_retained_fixture","fixture":fixtures[1].name,
                "checked_source":fixtures[1].checked_source,
                "construction":"cases[1].current Source a via public audio_definition/in_point_clock",
                "intent":"same genuine Bound Point proven by point_content_query_tests"}),
        },
        Case {
            name: "aggregate-alternating-514-via-shared-equalizers",
            document: ProjectDocument::from_json(&aggregate_wire.to_string()),
            checked_wire: aggregate_wire.clone(),
            reader: Reader::SharedEqualizers,
            frames: 514,
            source: json!({"kind":"checked_public_core_document","construction":"514 one-frame alternating SilentHold/Source nodes at FrameRate48000; each source selects real media0..1",
                "intent":"bounded aggregate attempt beyond one256-frame tile; actual returned masks/errors decide coverage"}),
        },
        Case {
            name: "aggregate-alternating-514-via-shared-input",
            document: aggregate,
            checked_wire: aggregate_wire,
            reader: Reader::SharedInput,
            frames: 514,
            source: json!({"kind":"checked_public_core_document","construction":"same514 checked document via read_shared_input(default selection)",
                "intent":"retain actual general ReadBlock with no requested-range cap raise"}),
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
fn point_snapshot(domain: &AudioPointDomain<'_>, plan: &RenderPlan) -> Value {
    json!({"belongs_to_expected_plan":domain.belongs_to(plan),
        "definition":domain.definition(),"root":domain.root(),"placement":domain.placement(),
        "reference_grid":domain.reference_grid(),"reference_sampling":domain.reference_sampling(),
        "reference_samples":domain.reference_samples()})
}
fn case_owned(case: Case) -> Value {
    let mut row = json!({"name":case.name,"source":case.source,"checked_document_wire":case.checked_wire,
        "wav":cases::wav_descriptor(cases::Wav::Stereo48000),
        "scope":{"returned_masks":"actual old Vec storage, unadmitted; observer allocations outside ReadWork",
            "lifetime":"actual result held through local ReadWork/provider drop, then controller drop",
            "final_consumption":"actual owning mask into_iter only after all retained borrowed snapshots; no block clone"}});
    let document = match case.document {
        Ok(value) => value,
        Err(value) => {
            row["setup"] = json!({"status":"DocumentValidationError",
                "error":error(&StageAudioError::from(PlanError::from(value)))});
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
        Reader::Point { node, .. } => match point(&plan, node) {
            Ok(value) => Some(value),
            Err(value) => {
                row["setup"] = json!({"status":"PointConstructionError","error":error(&value)});
                row["document_after"] = serde_json::to_value(&document).unwrap();
                assert_eq!(row["document_before"], row["document_after"]);
                return row;
            }
        },
        Reader::SharedEqualizers | Reader::SharedInput => None,
    };
    row["setup"] = json!({"status":"Ready","plan":plan_identity(&plan),
        "point":domain.as_ref().map(|value|point_snapshot(value,&plan))});
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    row["controller_before_local_work"] = controller_snapshot(&renderer);
    let result = {
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&cancelled, &work);
        let mut provider = ObservedProvider {
            prepared: cases::prepared(cases::Wav::Stereo48000),
            calls: 0,
            events: Vec::new(),
            work: &work,
            fault: Fault::None,
        };
        row["work_before"] = work_snapshot(&work);
        row["controller_before"] = controller_snapshot(&renderer);
        let result = match case.reader {
            Reader::Point { offset, frames, .. } => {
                let domain = domain.as_ref().unwrap();
                let start = ReferenceSample(
                    domain
                        .reference_samples()
                        .start
                        .0
                        .checked_add(offset)
                        .unwrap(),
                );
                row["request"] = json!({"reader":"read_point_domain_controlled","start":start.0,
                    "frames":frames,"depth":0,"timeout_seconds":TIMEOUT.as_secs()});
                let proof_start = domain.signal_at_reference(start).unwrap();
                let proof_end = domain
                    .signal_at_reference(ReferenceSample(
                        start.0.checked_add(i64::from(frames)).unwrap(),
                    ))
                    .unwrap();
                let proof = domain
                    .signal_ref()
                    .query(proof_start..proof_end, query_limits());
                row["ordinary_content_proof_outside_measured_work"] = match proof {
                    Ok(value) => json!({"status":"Ok","literal_work":value.work,
                        "span_count":value.spans.len(),
                        "content_kinds":value.spans.iter().map(|span|match &span.content{
                            AudioSignalContent::Leaf(_) => "Leaf",
                            AudioSignalContent::Stage(_) => "Stage",
                            AudioSignalContent::Bound(_) => "Bound",
                            AudioSignalContent::ProjectedStage(_) => "ProjectedStage",
                        }).collect::<Vec<_>>()}),
                    Err(value) => {
                        json!({"status":"Error","error":error(&StageAudioError::from(value))})
                    }
                };
                Returned::Signal(renderer.read_point_domain_controlled(
                    &mut provider,
                    domain,
                    start,
                    frames,
                    control,
                    0,
                ))
            }
            Reader::SharedEqualizers => {
                row["request"] = json!({"reader":"read_shared_equalizers","start":0,
                    "frames":case.frames,"timeout_seconds":TIMEOUT.as_secs()});
                Returned::Read(renderer.read_shared_equalizers(
                    &mut provider,
                    AudioSample(0),
                    case.frames,
                    control,
                ))
            }
            Reader::SharedInput => {
                row["request"] = json!({"reader":"read_shared_input","start":0,
                    "frames":case.frames,"selection":"InputSelection::default","timeout_seconds":TIMEOUT.as_secs()});
                Returned::Read(renderer.read_shared_input(
                    &mut provider,
                    AudioSample(0),
                    case.frames,
                    control,
                    crate::stages::shared_eq::InputSelection::default(),
                ))
            }
        };
        row["result_while_local_work_alive"] = returned(&result);
        row["work_while_result_alive"] = work_snapshot(&work);
        row["controller_while_result_alive"] = controller_snapshot(&renderer);
        row["provider_calls"] = json!(provider.calls);
        row["provider_events"] = json!(provider.events);
        row["cancelled_at_end"] = json!(cancelled.load(Ordering::Relaxed));
        drop(provider);
        row["work_after_provider_drop_result_alive"] = work_snapshot(&work);
        result
    };
    row["result_after_local_work_drop"] = returned(&result);
    assert_eq!(
        row["result_while_local_work_alive"],
        row["result_after_local_work_drop"]
    );
    row["controller_after_local_work_drop"] = controller_snapshot(&renderer);
    if let Some(domain) = &domain {
        row["point_after_read"] = point_snapshot(domain, &plan);
        assert_eq!(row["setup"]["point"], row["point_after_read"]);
    }
    drop(renderer);
    row["result_after_controller_drop"] = returned(&result);
    assert_eq!(
        row["result_while_local_work_alive"],
        row["result_after_controller_drop"]
    );
    row["actual_owning_mask_iteration_after_final_snapshot"] = consume_returned(result);
    row["document_after"] = serde_json::to_value(&document).unwrap();
    assert_eq!(row["document_before"], row["document_after"]);
    row
}

#[expect(
    clippy::iter_nth,
    reason = "The immutable witness records actual iterator helper behavior."
)]
fn public_mask<T: Copy>(value: &[Range<T>], capacity: usize, at: fn(T) -> i64) -> Value {
    let range = |r: &Range<T>| [at(r.start), at(r.end)];
    let mut iterator = value.iter();
    let initial = iterator.size_hint();
    let first = iterator.next().map(range);
    let last = iterator.next_back().map(range);
    json!({"len":value.len(),"capacity":capacity,"is_empty":value.is_empty(),
        "requested_storage":vector_layout::<Range<T>>(capacity),
        "values":value.iter().map(range).collect::<Vec<_>>(),
        "helpers":{"first":value.first().map(range),"last":value.last().map(range),
            "get_at_len":value.get(value.len()).map(range),
            "forward_prefix":value.iter().take(3).map(range).collect::<Vec<_>>(),
            "reverse_prefix":value.iter().rev().take(3).map(range).collect::<Vec<_>>(),
            "nth_two":value.iter().nth(2).map(range),
            "iterator_initial_size_hint":initial,
            "iterator_first":first,"iterator_last_after_first":last,
            "iterator_remaining_len":iterator.len(),
            "iterator_remaining_size_hint":iterator.size_hint()}})
}
fn public_result(value: &Result<PointDomainAudioBlock, StageAudioError>) -> Value {
    match value {
        Ok(block) => {
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
            } = block;
            json!({"status":"Ok","kind":"PointDomainAudioBlock","schema_version":schema_version,
                "stage":stage,"project_id":project_id,"revision_id":revision_id,
                "definition":definition,"root":root,"placement":placement,
                "reference_grid":reference_grid,"reference_samples":reference_samples,
                "start":start.0,"f32_bits":samples.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
                "sample_storage":vector_layout::<[f32;2]>(samples.capacity()),
                "suppressed":public_mask(suppressed.as_slice(),suppressed.capacity(),|at:ReferenceSample|at.0)})
        }
        Err(value) => json!({"status":"Error","kind":"PointDomainAudioBlock","error":error(value)}),
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
            json!({"ordinal":self.calls,"project_id":project,"revision_id":revision,
            "asset":asset,"cancelled_before":cancelled.load(Ordering::Relaxed),
            "outcome":"PreparedSource","prepared_provenance":self.prepared.provenance(),
            "prepared_index":self.prepared.index(),"matrix_layout":self.prepared.matrix_layout()}),
        );
        Ok(&self.prepared)
    }
}
fn public_case_owned(case: Case, name: &'static str) -> Value {
    let mut row = json!({"name":name,"source":case.source,"checked_document_wire":case.checked_wire,
        "wav":cases::wav_descriptor(cases::Wav::Stereo48000),
        "scope":{"reader":"actual public StageAudio::read_point_domain final conversion",
            "work":"public method owns and drops its inaccessible internal ReadWork before returning; no external ledger substituted",
            "lifetime":"returned public block held beyond public method return and controller drop",
            "mask_observation":"borrowed values/helpers/capacity only; no block clone or owning mask iteration"}});
    let document = match case.document {
        Ok(value) => value,
        Err(value) => {
            row["setup"] = json!({"status":"DocumentValidationError",
                "error":error(&StageAudioError::from(PlanError::from(value)))});
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
    let Reader::Point {
        node,
        offset,
        frames,
    } = case.reader
    else {
        unreachable!("only the three actual Point cases feed the public observer")
    };
    let domain = match point(&plan, node) {
        Ok(value) => value,
        Err(value) => {
            row["setup"] = json!({"status":"PointConstructionError","error":error(&value)});
            row["document_after"] = serde_json::to_value(&document).unwrap();
            assert_eq!(row["document_before"], row["document_after"]);
            return row;
        }
    };
    row["setup"] = json!({"status":"Ready","plan":plan_identity(&plan),
        "point":point_snapshot(&domain,&plan)});
    let start = ReferenceSample(
        domain
            .reference_samples()
            .start
            .0
            .checked_add(offset)
            .unwrap(),
    );
    row["request"] = json!({"reader":"StageAudio::read_point_domain","start":start.0,
        "frames":frames,"timeout_seconds":TIMEOUT.as_secs()});
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let mut provider = PublicProvider {
        prepared: cases::prepared(cases::Wav::Stereo48000),
        calls: 0,
        events: Vec::new(),
    };
    row["controller_before"] = controller_snapshot(&renderer);
    let result =
        renderer.read_point_domain(&mut provider, &domain, start, frames, TIMEOUT, &cancelled);
    row["result_after_public_method_internal_work_drop"] = public_result(&result);
    row["controller_result_alive"] = controller_snapshot(&renderer);
    row["provider_calls"] = json!(provider.calls);
    row["provider_events"] = json!(provider.events);
    row["cancelled_at_end"] = json!(cancelled.load(Ordering::Relaxed));
    drop(provider);
    row["result_after_provider_drop"] = public_result(&result);
    assert_eq!(
        row["result_after_public_method_internal_work_drop"],
        row["result_after_provider_drop"]
    );
    row["point_after_read"] = point_snapshot(&domain, &plan);
    assert_eq!(row["setup"]["point"], row["point_after_read"]);
    drop(renderer);
    row["result_after_controller_drop"] = public_result(&result);
    assert_eq!(
        row["result_after_public_method_internal_work_drop"],
        row["result_after_controller_drop"]
    );
    drop(result);
    row["actual_public_result_dropped_after_controller"] = json!(true);
    row["document_after"] = serde_json::to_value(&document).unwrap();
    assert_eq!(row["document_before"], row["document_after"]);
    row
}
fn public_cases_owned() -> Vec<Value> {
    let names = [
        "public-api-point-silent-hold",
        "public-api-point-source",
        "public-api-point-genuine-bound",
    ];
    cases_owned()
        .into_iter()
        .take(3)
        .zip(names)
        .map(|(case, name)| public_case_owned(case, name))
        .collect()
}

fn matrix_owned() -> Value {
    crate::stages::internal_mask_request::with_legacy_internal_masks(matrix_owned_legacy_body)
}
fn matrix_owned_legacy_body() -> Value {
    json!({"schema":"old-owned-suppression-output-ownership-v1","published_checkpoint":CHECKPOINT,
        "prior_native":{"path":"tests/fixtures/owned-suppression-native-baseline-v1/snapshot.json",
            "sha256":PRIOR_NATIVE_SHA256},
        "actual_layouts":{"SignalBlock":structure::<SignalBlock>(),"ReadBlock":structure::<ReadBlock>(),
            "PointDomainAudioBlock":structure::<PointDomainAudioBlock>(),
            "Vec_AudioSample_Range":structure::<Vec<Range<AudioSample>>>(),
            "Vec_SignalSample_Range":structure::<Vec<Range<SignalSample>>>(),
            "Range_AudioSample":structure::<Range<AudioSample>>(),"Range_SignalSample":structure::<Range<SignalSample>>(),
            "PreparedBlock":structure::<PreparedBlock>(),"PreparedStage":structure::<PreparedStage>()},
        "existing_limits":{"single_read_MAX_OUTPUT_FRAMES":MAX_OUTPUT_FRAMES,
            "general_MAX_EDGE_PREPARATION_FRAMES":MAX_EDGE_PREPARATION_FRAMES},
        "cases":cases_owned().into_iter().map(case_owned).collect::<Vec<_>>(),
        "public_cases":public_cases_owned(),
        "coverage":"actual output/error and masklen decide reached branches; no257range or new owned-account claim"})
}
fn strip_owned(mut value: Value) -> Value {
    for key in ["variant", "source_sha256"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}
#[test]
#[ignore = "manual OLD output-mask ownership/pressure capture; fresh /tmp output and pinned source required"]
fn capture_old_owned_suppression_output_v1() {
    let output = std::env::var("DEADPAN_OWNED_SUPPRESSION_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_OWNED_SUPPRESSION_VARIANT").expect("actual variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source =
        std::env::var("DEADPAN_OWNED_SUPPRESSION_SOURCE_SHA256").expect("actual source manifest");
    assert!(source.len() == 64 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_OWNED_SUPPRESSION_PUBLISHED_CHECKPOINT")
        .expect("published checkpoint");
    assert_eq!(checkpoint, CHECKPOINT);
    let mut body = matrix_owned();
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
#[ignore = "external immutable full OLD ownership/pressure regression"]
fn owned_suppression_output_matches_external_old_v1() {
    let path = std::env::var("DEADPAN_OWNED_SUPPRESSION_EXPECTED")
        .expect("independent immutable OLD body");
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(matrix_owned(), strip_owned(expected));
}

#[test]
fn public_and_private_outputs_preserve_the_frozen_before_body() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/owned-suppression-output-baseline-v1/snapshot.json");
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let ordinary = crate::stages::point_suppression::with_legacy_point_suppression(matrix_owned);
    assert_eq!(ordinary, expected);
    assert_eq!(matrix_owned(), expected);
}
