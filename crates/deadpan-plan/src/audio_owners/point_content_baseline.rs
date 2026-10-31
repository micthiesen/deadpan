//! Read-only OLD Point content-query observations; no public admission wrapper.
use super::*;
use crate::{
    AdmittedAudioBoundDomain, AudioBoundaryRule, AudioSignalQuery, AudioSignalSpan, SignalSample,
};
use std::ops::Range;

const CHECKPOINT: &str = "af34e1e1143e9abc11f49b77ee49c602c116aac4";

thread_local! {
    static POINT_ROWS: RefCell<Option<Vec<Value>>> = const { RefCell::new(None) };
    static DIRECT: RefCell<Option<Value>> = const { RefCell::new(None) };
}

pub(super) struct DirectScope(Option<Option<Value>>);
impl DirectScope {
    pub(super) fn new(origin: &'static str, fixture: &Value, span: usize) -> Self {
        let active = POINT_ROWS.with(|slot| slot.borrow().is_some());
        Self(active.then(|| {
            DIRECT.with(|slot| {
                slot.replace(Some(json!({"kind":"direct_processing","origin":origin,
                "fixture":fixture,"span_index":span})))
            })
        }))
    }
}
impl Drop for DirectScope {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            DIRECT.with(|slot| *slot.borrow_mut() = previous);
        }
    }
}
struct CaptureSession;
impl CaptureSession {
    fn new() -> Self {
        POINT_ROWS.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Vec::new());
        });
        DIRECT.with(|slot| assert!(slot.borrow().is_none()));
        Self
    }
    fn finish(self) -> Vec<Value> {
        POINT_ROWS.with(|slot| slot.borrow_mut().take().unwrap())
    }
}
impl Drop for CaptureSession {
    fn drop(&mut self) {
        POINT_ROWS.with(|slot| *slot.borrow_mut() = None);
        DIRECT.with(|slot| *slot.borrow_mut() = None);
    }
}

fn recording() -> Recording {
    Recording {
        policy: Policy::<true>::new(usize::MAX),
        events: Vec::new(),
    }
}
fn content_snapshot(plan: &RenderPlan, value: &AudioSignalContent<'_>) -> Value {
    match value {
        AudioSignalContent::Leaf(content) => json!({"tag":"Leaf","value":content}),
        AudioSignalContent::Stage(stage) => {
            json!({"tag":"Stage","value":stage.point_content_old_snapshot(plan)})
        }
        AudioSignalContent::Bound(bound) => {
            json!({"tag":"Bound","value":bound.capture_baseline_snapshot(plan)})
        }
        AudioSignalContent::ProjectedStage(_) => {
            panic!(
                "Structural Point content produced a projection; add a complete borrowed private projection snapshot before capture"
            )
        }
    }
}
fn span_snapshot(plan: &RenderPlan, value: &AudioSignalSpan<'_>) -> Value {
    let AudioSignalSpan {
        definition,
        samples,
        allocated_samples,
        signal_extent,
        instance,
        gap_after,
        transform,
        grid,
        sampling,
        retimes,
        content,
    } = value;
    json!({"definition":definition,"samples":samples,"allocated_samples":allocated_samples,
        "signal_extent":signal_extent,"instance":instance,"gap_after":gap_after,
        "transform":transform,"grid":grid,"sampling":sampling,"retimes":retimes,
        "content":content_snapshot(plan,content)})
}
fn query_snapshot(plan: &RenderPlan, value: &AudioSignalQuery<'_>) -> Value {
    let AudioSignalQuery {
        project_id,
        revision_id,
        definition,
        samples,
        spans,
        lookup,
        work,
    } = value;
    let crate::LookupStats {
        visited_nodes,
        sequence_comparisons,
        iteration_run_comparisons,
    } = lookup;
    json!({"project_id":project_id,"revision_id":revision_id,"definition":definition,
        "samples":samples,"span_capacity":spans.capacity(),"work":work,
        "lookup":{"visited_nodes":visited_nodes,"sequence_comparisons":sequence_comparisons,
            "iteration_run_comparisons":iteration_run_comparisons},
        "spans":spans.iter().map(|span|span_snapshot(plan,span)).collect::<Vec<_>>()})
}
fn query_result(plan: &RenderPlan, value: &Result<AudioSignalQuery<'_>, PlanError>) -> Value {
    match value {
        Ok(value) => json!({"result":"Ok","query":query_snapshot(plan,value)}),
        Err(error) => json!({"result":"Error","error":snapshot::error_json(error),
            "ordinary_partial_work":"not exposed by the ordinary constructor error"}),
    }
}
fn raw_snapshot(plan: &RenderPlan, value: &AdmittedAudioBoundDomain<'_, usize, Lease>) -> Value {
    json!({"raw_domain":raw_value(plan,value.domain()),
        "canonical_units":value.canonical_units(),"additional_units":value.additional_units(),
        "total_units":value.total_units(),"was_forwarded":value.was_forwarded()})
}

#[derive(Clone)]
struct Request {
    name: String,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
}
impl Request {
    fn new(name: &str, start: SignalSample, end: SignalSample, limits: AudioQueryLimits) -> Self {
        Self {
            name: name.into(),
            samples: start..end,
            limits,
        }
    }
    fn snapshot(&self) -> Value {
        json!({"name":self.name,"samples":self.samples,"limits":limits_value(self.limits)})
    }
}
fn requests(
    point: &crate::AudioPointDomain<'_>,
    full: &Result<AudioSignalQuery<'_>, PlanError>,
) -> Vec<Request> {
    let support = point.reference_samples();
    let start = point.signal_at_reference(support.start).unwrap();
    let end = point.signal_at_reference(support.end).unwrap();
    let defaults = AudioQueryLimits::default();
    let mut values = vec![
        Request::new("full", start, end, defaults),
        Request::new("empty_at_start", start, start, defaults),
        Request::new("empty_at_end", end, end, defaults),
    ];
    if start < end {
        let first_end = SignalSample(start.0.checked_add(1).unwrap());
        let last_start = SignalSample(end.0.checked_sub(1).unwrap());
        let length = end.0.checked_sub(start.0).unwrap();
        let middle = SignalSample(start.0.checked_add(length / 2).unwrap());
        values.push(Request::new("first_point", start, first_end, defaults));
        values.push(Request::new("last_point", last_start, end, defaults));
        values.push(Request::new("suffix_from_middle", middle, end, defaults));
        values.push(Request::new("reversed_nonempty", end, start, defaults));
    }
    if let Some(before) = start.0.checked_sub(1) {
        values.push(Request::new(
            "one_before_actual_support",
            SignalSample(before),
            end,
            defaults,
        ));
    }
    if let Some(after) = end.0.checked_add(1) {
        values.push(Request::new(
            "one_after_actual_support",
            start,
            SignalSample(after),
            defaults,
        ));
    }
    for work in [0, 1, 2] {
        values.push(Request::new(
            &format!("full_work_{work}"),
            start,
            end,
            AudioQueryLimits {
                maximum_work: work,
                ..defaults
            },
        ));
    }
    values.push(Request::new(
        "full_one_span",
        start,
        end,
        AudioQueryLimits {
            maximum_spans: 1,
            ..defaults
        },
    ));
    if let Ok(query) = full {
        values.push(Request::new(
            "full_literal_returned_work",
            start,
            end,
            AudioQueryLimits {
                maximum_work: query.work,
                ..defaults
            },
        ));
        if let Some(work) = query.work.checked_sub(1) {
            values.push(Request::new(
                "full_one_below_returned_work",
                start,
                end,
                AudioQueryLimits {
                    maximum_work: work,
                    ..defaults
                },
            ));
        }
        values.push(Request::new(
            "full_literal_returned_spans",
            start,
            end,
            AudioQueryLimits {
                maximum_spans: query.spans.len(),
                ..defaults
            },
        ));
        if let Some(spans) = query.spans.len().checked_sub(1) {
            values.push(Request::new(
                "full_one_below_returned_spans",
                start,
                end,
                AudioQueryLimits {
                    maximum_spans: spans,
                    ..defaults
                },
            ));
        }
        for (index, span) in query.spans.iter().skip(1).take(3).enumerate() {
            let seam = span.samples.start;
            if seam > start && seam < end {
                values.push(Request::new(
                    &format!("actual_seam_{index}_crossing"),
                    SignalSample(seam.0.checked_sub(1).unwrap()),
                    SignalSample(seam.0.checked_add(1).unwrap()),
                    defaults,
                ));
            }
        }
    }
    assert!(values.len() <= 20, "bounded representative requests");
    values
}
fn request_scope(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    caller: Option<&Rc<RefCell<PolicyState>>>,
    request: &Request,
    expected_point: &Value,
) -> Value {
    let caller_before = caller_state(caller);
    let bound_before = bound.capture_baseline_snapshot(plan);
    let mut policy = recording();
    let raw_before = state(&policy.policy.0);
    let raw = bound.raw_domain_query(&mut policy).unwrap();
    let raw_live = state(&policy.policy.0);
    let construction_events = policy.events.clone();
    let raw_body = raw_snapshot(plan, &raw);
    let AudioBoundDomain::Point(point) = raw.domain() else {
        panic!("actual Point operand changed domain kind between equivalent fresh constructions");
    };
    let point_before = point.probe_baseline_snapshot(plan);
    assert_eq!(&point_before, expected_point);
    admission_tests::observe_request(plan, point, request, Some(bound));
    let ordinary = point
        .signal_ref()
        .query(request.samples.clone(), request.limits);
    let ordinary_body = query_result(plan, &ordinary);
    let point_while_ordinary_live = point.probe_baseline_snapshot(plan);
    let ordinary_live = state(&policy.policy.0);
    assert_eq!(
        ordinary_live, raw_live,
        "ordinary query does not forward admission callbacks"
    );
    assert_eq!(caller_state(caller), caller_before);
    drop(ordinary);
    let ordinary_after_drop = state(&policy.policy.0);
    assert_eq!(ordinary_after_drop, raw_live);

    let event_start = policy.events.len();
    let helper_before = state(&policy.policy.0);
    let mut local = Budget {
        remaining: request.limits.maximum_work,
        lookup: Default::default(),
    };
    let local_before = json!({"remaining":local.remaining,"lookup":local.lookup});
    let (result, canonical, additional, remaining, lookup) = {
        let mut budget = QueryBudget::new(&mut local, &mut policy);
        let result = admission::outer::<_, Recording>(point.signal_ref().structural_child_query(
            request.samples.clone(),
            request.limits,
            AudioBoundaryRule::PointCeil,
            (true, true),
            &mut budget,
        ));
        (
            result,
            budget.canonical_used,
            budget.additional_used,
            budget.remaining,
            budget.lookup,
        )
    };
    let helper_body = match &result {
        Ok(value) => json!({"result":"Ok","query":query_snapshot(plan,&value.query)}),
        Err(error) => json!({"result":"Error","error":snapshot::error_json(error)}),
    };
    let helper_while_live = state(&policy.policy.0);
    let helper_events = policy.events[event_start..].to_vec();
    assert_eq!(point.probe_baseline_snapshot(plan), point_before);
    assert_eq!(caller_state(caller), caller_before);
    drop(result);
    let helper_after_drop = state(&policy.policy.0);
    assert_eq!(
        helper_after_drop["live"], raw_live["live"],
        "only real raw carrier remains"
    );
    let raw_after_queries = raw_snapshot(plan, &raw);
    assert_eq!(raw_body, raw_after_queries);
    drop(raw);
    let raw_after_drop = state(&policy.policy.0);
    assert_eq!(raw_after_drop["live"], 0);
    assert_eq!(caller_state(caller), caller_before);
    let bound_after = bound.capture_baseline_snapshot(plan);
    assert_eq!(bound_before, bound_after);
    let mut value = json!({"request":request.snapshot(),"bound_before":bound_before,
        "bound_after":bound_after,"caller_before":caller_before,"caller_after":caller_state(caller),
        "raw":{"before":raw_before,"while_result_live":raw_live,"result":raw_body,
            "construction_events":construction_events,"issuer_after_all_results_drop":raw_after_drop},
        "ordinary":{"result":ordinary_body,"point_while_result_live":point_while_ordinary_live,
            "ledger_while_result_live":ordinary_live,"ledger_after_result_drop":ordinary_after_drop}});
    value["point_before"] = point_before;
    value["existing_internal_metered_helper_same_issuer"] = json!({
        "result":helper_body,"local_before":local_before,
        "local_after":{"remaining":remaining,"lookup":lookup},
        "canonical":canonical,"additional":additional,"events":helper_events,
        "accepted_work_delta":helper_after_drop["accepted_work"].as_u64().unwrap()
            .checked_sub(helper_before["accepted_work"].as_u64().unwrap()).unwrap(),
        "ledger_before":helper_before,"ledger_while_result_live":helper_while_live,
        "ledger_after_result_drop":helper_after_drop});
    value
}

pub(super) fn observe(
    plan: &RenderPlan,
    bound: &AudioBound<'_>,
    caller: Option<&Rc<RefCell<PolicyState>>>,
) {
    if !POINT_ROWS.with(|slot| slot.borrow().is_some()) {
        return;
    }
    let context = DIRECT
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(|| {
            json!({"kind":"retained_scope","origin":ORIGIN.get().expect("actual owning origin"),
            "fixture":FIXTURE.with(|slot|slot.borrow().as_ref().expect("actual fixture").clone())})
        });
    let bound_before = bound.capture_baseline_snapshot(plan);
    let caller_before = caller_state(caller);
    let mut policy = recording();
    let raw = bound.raw_domain_query(&mut policy).unwrap();
    let raw_body = raw_snapshot(plan, &raw);
    let raw_live = state(&policy.policy.0);
    let (kind, geometry, pending) = if let AudioBoundDomain::Point(point) = raw.domain() {
        let point_before = point.probe_baseline_snapshot(plan);
        let support = point.reference_samples();
        let samples = point.signal_at_reference(support.start).unwrap()
            ..point.signal_at_reference(support.end).unwrap();
        let full = point
            .signal_ref()
            .query(samples.clone(), Default::default());
        let pending = requests(point, &full);
        let full_body = query_result(plan, &full);
        let while_full = state(&policy.policy.0);
        assert_eq!(while_full, raw_live);
        assert_eq!(point.probe_baseline_snapshot(plan), point_before);
        drop(full);
        assert_eq!(state(&policy.policy.0), raw_live);
        (
            "PointCeil",
            json!({"point":point_before,"reference_support":support,
            "signal_support":samples,"full_ordinary_result":full_body,
            "ledger_while_ordinary_result_live":while_full}),
            pending,
        )
    } else {
        (
            "NotPoint",
            json!({"result":"NotApplicable","reason":"actual raw result is not a Point domain"}),
            Vec::new(),
        )
    };
    assert_eq!(caller_state(caller), caller_before);
    drop(raw);
    let after_geometry_drop = state(&policy.policy.0);
    assert_eq!(after_geometry_drop["live"], 0);
    let mut values = Vec::new();
    if kind == "PointCeil" {
        for request in pending {
            values.push(request_scope(
                plan,
                bound,
                caller,
                &request,
                &geometry["point"],
            ));
        }
    }
    assert_eq!(bound.capture_baseline_snapshot(plan), bound_before);
    assert_eq!(caller_state(caller), caller_before);
    POINT_ROWS.with(|slot| {
        let mut rows = slot.borrow_mut();
        let rows = rows.as_mut().unwrap();
        let index = rows.len();
        rows.push(json!({"operand_index":index,"context":context,"kind":kind,
            "bound_before":bound_before,"bound_after":bound.capture_baseline_snapshot(plan),
            "caller_before":caller_before,"caller_after":caller_state(caller),
            "geometry_setup":{"raw":raw_body,"raw_ledger_while_live":raw_live,
                "raw_events":policy.events,"raw_ledger_after_drop":after_geometry_drop,
                "geometry":geometry},"requests":values}));
    });
}
fn matrix_point_content() -> Value {
    let session = CaptureSession::new();
    let setup = matrix_raw();
    let rows = session.finish();
    assert_eq!(rows.len(), 77, "actual existing raw operand visitor");
    assert_eq!(rows.iter().filter(|r| r["kind"] == "PointCeil").count(), 18);
    let requests: usize = rows
        .iter()
        .map(|r| r["requests"].as_array().unwrap().len())
        .sum();
    json!({"schema":"old-point-content-query-baseline-v1","raw_operand_setup_corpora":setup,
        "operands":rows,"actual_operand_count":77,"actual_point_count":18,"actual_request_count":requests,
        "scope":{"ordinary":"fresh signal_ref().query on actual Structural Point signal; no accepted-work receipt for ordinary allocation",
            "internal_helper":"separate existing structural_child_query lane on the same actual raw issuer, PointCeil and stops true,true",
            "lifetime":"ordinary query result is dropped before helper query; genuine admitted raw carrier stays alive through both",
            "peak":"actual cumulative recording-policy reservations only; not allocator/RSS and excludes harness JSON/test observer allocations",
            "projection":"unexpected ProjectedStage aborts before output publication; no descriptor-only substitute"}})
}
fn strip_envelope(mut value: Value) -> Value {
    for key in [
        "variant",
        "source_sha256",
        "published_point_borrow_checkpoint",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}
#[test]
#[ignore = "manual OLD Point content producer; fresh /tmp output and pinned source/checkpoint required"]
fn capture_old_point_content_query_baseline_v1() {
    let output = std::env::var("DEADPAN_POINT_CONTENT_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_POINT_CONTENT_VARIANT").expect("actual variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_POINT_CONTENT_SOURCE_SHA256")
        .expect("complete source manifest digest");
    assert!(source.len() == 64 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_POINT_CONTENT_PUBLISHED_CHECKPOINT")
        .expect("published Point borrow checkpoint");
    assert!(checkpoint.len() == 40 && checkpoint.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(checkpoint, CHECKPOINT);
    let mut body = matrix_point_content();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_point_borrow_checkpoint"] = json!(checkpoint);
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
#[ignore = "manual full external OLD Point content regression; independent captured body required"]
fn point_content_matches_explicit_external_old_v1() {
    let path =
        std::env::var("DEADPAN_POINT_CONTENT_EXPECTED").expect("independent immutable OLD body");
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(matrix_point_content(), strip_envelope(expected));
}

#[test]
fn ordinary_point_content_matches_frozen_raw_operand_witness_v1() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/point-content-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(matrix_point_content(), expected);
}

#[path = "point_content_supplement.rs"]
mod supplement;

#[path = "point_content_admission_tests.rs"]
mod admission_tests;
