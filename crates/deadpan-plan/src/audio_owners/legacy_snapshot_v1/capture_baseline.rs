//! One-shot logical observations of the actual legacy capture/discard/probe.
//! Inactive outside an explicitly started producer; never an allocation oracle.
use super::*;
use crate::plan::audio::{AudioWalkSpan, EditorialCapture, EnvelopeConstraint};
use crate::plan::audio_domain::AudioDomainSeed;
use std::cell::RefCell;

#[path = "probe_baseline.rs"]
pub(in crate::plan) mod probe_baseline;

#[derive(Default)]
struct Recording {
    case: Option<String>,
    phase: &'static str,
    events: Vec<Value>,
}
thread_local! {
    static RECORDING: RefCell<Option<Recording>> = const { RefCell::new(None) };
}
struct Session;
impl Session {
    fn start() -> Self {
        RECORDING.with(|slot| {
            let mut slot = slot.borrow_mut();
            assert!(slot.is_none(), "nested baseline producer");
            *slot = Some(Recording::default());
        });
        Self
    }
    fn finish(self) -> Vec<Value> {
        RECORDING.with(|slot| slot.borrow_mut().take().unwrap().events)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        RECORDING.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
pub(in crate::plan) struct CaseScope {
    active: bool,
}
pub(in crate::plan) fn case_scope(name: &str) -> CaseScope {
    let active = RECORDING.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(state) = slot.as_mut() else {
            return false;
        };
        assert!(state.case.is_none(), "nested baseline case");
        state.case = Some(name.to_owned());
        state.phase = "case_setup";
        true
    });
    CaseScope { active }
}
impl Drop for CaseScope {
    fn drop(&mut self) {
        if self.active {
            RECORDING.with(|slot| {
                if let Some(state) = slot.borrow_mut().as_mut() {
                    state.case = None;
                }
            });
        }
    }
}
pub(in crate::plan) fn observe_case<T>(
    name: &str,
    label: &'static str,
    read: impl FnOnce() -> T,
) -> (T, Vec<Value>) {
    let session = Session::start();
    let value = {
        let _scope = case_scope(name);
        phase(label);
        read()
    };
    (value, session.finish())
}
pub(in crate::plan) fn core_receipt(
    resolved_work: usize,
    lattice_work: usize,
    total: usize,
    canonical: usize,
    additional: usize,
    forwarded: bool,
) {
    if !active() {
        return;
    }
    record(
        json!({"kind":"actual_capture_core_receipt","resolved_work":resolved_work,
        "lattice_work":lattice_work,"total":total,"canonical":canonical,
        "additional":additional,"forwarded":forwarded}),
    );
}
pub(in crate::plan) fn policy_failure(
    plan: &RenderPlan,
    at: AudioSample,
    captured: Option<&AudioDomainSeed>,
    work: (usize, usize),
) {
    if !active() {
        return;
    }
    record(json!({"kind":"actual_capture_policy_refusal","at":at.0,
        "capture_slot":captured.map(|v|seed(plan,v)),
        "actual_capture_work_spent":work.0-work.1}));
}
pub(in crate::plan) fn phase(value: &'static str) {
    RECORDING.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut() {
            state.phase = value;
        }
    });
}
fn active() -> bool {
    RECORDING.with(|slot| slot.borrow().as_ref().is_some_and(|s| s.case.is_some()))
}
fn record(value: Value) {
    RECORDING.with(|slot| {
        let mut slot = slot.borrow_mut();
        let state = slot.as_mut().unwrap();
        state.events.push(json!({
            "case":state.case.as_ref().unwrap(),"phase":state.phase,"observation":value,
        }));
    });
}
pub(in crate::plan) fn constraint(plan: &RenderPlan, value: &EnvelopeConstraint) -> Value {
    super::constraint(value, plan)
}
pub(in crate::plan) fn editorial(plan: &RenderPlan, value: &EditorialCapture) -> Value {
    let EditorialCapture {
        editorial,
        constraints,
        repeats: path,
    } = value;
    json!({
        "editorial":editorial.iter().map(|v|{
            let crate::plan::audio::EditorialConstraint { boundary, edges, incident_samples }=v;
            json!({"boundary":constraint(plan,boundary),"edges":edges,
                "incident_samples":incident_samples.map(|(a,b)|json!([exact(a),exact(b)]))})
        }).collect::<Vec<_>>(),
        "constraints":constraints.iter().map(|v|constraint(plan,v)).collect::<Vec<_>>(),
        "repeats":repeats(path),
    })
}
fn seed(plan: &RenderPlan, value: &AudioDomainSeed) -> Value {
    let AudioDomainSeed {
        walk,
        samples,
        visible,
        instance: path,
    } = value;
    let AudioWalkSeed {
        definition,
        bypass_binding,
        node,
        transform,
        grid: sample_grid,
        extent,
        envelope,
        constraints,
        editorial,
        suppress_entry_editorial,
        repeats: path_repeats,
        retimes,
        gap,
    } = walk;
    let bypass = bypass_binding.map(|target| match target {
        BindingTarget::Node(index) => {
            json!({"tag":"Node","node":plan.nodes[index].inspection.id.as_str()})
        }
        BindingTarget::Gap(index) => {
            json!({"tag":"Gap","node":plan.nodes[index].inspection.id.as_str()})
        }
    });
    json!({
        "samples":samples,"visible":visible,"instance":instance(path),
        "walk":{
            "definition":selector(definition.as_ref()),"bypass_binding":bypass,
            "node":plan.nodes[*node].inspection.id.as_str(),"transform":transform,
            "grid":grid(*sample_grid),"extent":exact_range(extent),
            "envelope":envelope.as_ref().map(exact_range),
            "constraints":constraints.iter().map(|v|constraint(plan,v)).collect::<Vec<_>>(),
            "editorial":editorial.iter().map(|v|{
                let crate::plan::audio::EditorialConstraint { boundary, edges, incident_samples }=v;
                json!({"boundary":constraint(plan,boundary),"edges":edges,
                    "incident_samples":incident_samples.map(|(a,b)|json!([exact(a),exact(b)]))})
            }).collect::<Vec<_>>(),
            "suppress_entry_editorial":suppress_entry_editorial,
            "repeats":repeats(path_repeats),"retimes":retimes,"gap":gap,
        },
    })
}
fn content(plan: &RenderPlan, value: &crate::AudioSignalContent<'_>) -> Value {
    use crate::AudioSignalContent::*;
    match value {
        Leaf(value) => json!({"tag":"Leaf","value":value}),
        Stage(value) => {
            assert!(value.belongs_to(plan));
            json!({"tag":"Stage","belongs_to_expected_plan":true,"descriptor":value.descriptor()})
        }
        Bound(value) => json!({"tag":"Bound","value":value.capture_baseline_snapshot(plan)}),
        ProjectedStage(value) => {
            json!({"tag":"ProjectedStage","published_projection":value.as_ref()})
        }
    }
}
pub(in crate::plan) fn processing(
    plan: &RenderPlan,
    value: &crate::AudioProcessingSpan<'_>,
) -> Value {
    let crate::AudioProcessingSpan {
        definition,
        samples,
        allocated_samples,
        project_extent,
        instance: path,
        gap_after,
        transform,
        grid: sample_grid,
        sampling,
        retimes,
        content: body,
    } = value;
    json!({
        "definition":selector(definition.as_ref()),"samples":samples,
        "allocated_samples":allocated_samples,"project_extent":exact_range(project_extent),
        "instance":instance(path),"gap_after":gap_after.as_ref().map(iteration),
        "transform":transform,"grid":grid(*sample_grid),"sampling":sampling,
        "retimes":retimes,"content":content(plan,body),
    })
}
fn span(plan: &RenderPlan, value: &AudioWalkSpan<'_>) -> Value {
    match value {
        AudioWalkSpan::Leaf(value) => {
            let crate::AudioSpan {
                definition,
                samples,
                allocated_samples,
                project_extent,
                envelope_extent,
                envelope_samples,
                envelope,
                boundaries,
                instance: path,
                gap_after,
                transform,
                grid: sample_grid,
                sampling,
                retimes,
                content,
            } = value;
            json!({"tag":"Leaf","value":{
                "definition":selector(definition.as_ref()),"samples":samples,
                "allocated_samples":allocated_samples,"project_extent":exact_range(project_extent),
                "envelope_extent":exact_range(envelope_extent),"envelope_samples":envelope_samples,
                "envelope":envelope,"boundaries":boundaries,"instance":instance(path),
                "gap_after":gap_after.as_ref().map(iteration),"transform":transform,
                "grid":grid(*sample_grid),"sampling":sampling,"retimes":retimes,"content":content,
            }})
        }
        AudioWalkSpan::Stage(value) => json!({"tag":"Stage","value":processing(plan,value)}),
    }
}
pub(in crate::plan) fn bound_domain(plan: &RenderPlan, value: &AudioBoundDomain<'_>) -> Value {
    match value {
        AudioBoundDomain::Root(value) => {
            assert!(value.belongs_to(plan));
            json!({"tag":"RootRoundEven","domain":domain(value)})
        }
        AudioBoundDomain::Point(value) => {
            assert!(value.belongs_to(plan));
            json!({"tag":"PointCeil","belongs_to_expected_plan":true,
                "definition":selector(Some(value.definition())),"root":value.root().as_str(),
                "placement":placement_json(value.placement()),"grid":grid(value.reference_grid()),
                "samples":value.reference_samples(),"sampling":value.reference_sampling(),
                "coverage":"complete public reference map; private AudioSignal provider not inspected"})
        }
        AudioBoundDomain::Empty => json!({"tag":"Empty"}),
    }
}
pub(in crate::plan) fn capture(
    plan: &RenderPlan,
    sample: AudioSample,
    owner: &AudioOwnerClock<'_>,
    captured: Option<&AudioDomainSeed>,
    walked: &Result<AudioWalkSpan<'_>, PlanError>,
    work: (usize, usize),
) {
    capture_ref(plan, sample, owner, captured, walked.as_ref(), work);
}
pub(in crate::plan) fn capture_ref(
    plan: &RenderPlan,
    sample: AudioSample,
    owner: &AudioOwnerClock<'_>,
    captured: Option<&AudioDomainSeed>,
    walked: Result<&AudioWalkSpan<'_>, &PlanError>,
    work: (usize, usize),
) {
    if !active() {
        return;
    }
    let result = match walked {
        Ok(value) => json!({"result":"Ok","discarded_span":span(plan,value)}),
        Err(error) => json!({"result":"Error","error":error_json(error)}),
    };
    record(json!({
        "kind":"actual_audio_walk_owner_input","at":sample.0,
        "selected_observation":clock(owner,plan,|v|v.0),
        "capture_slot":captured.map(|v|seed(plan,v)),
        "budget_before":work.0,"budget_remaining":work.1,
        "actual_capture_work_spent":work.0-work.1,"walk":result,
        "attestation":"same audio_walk_inner result immediately before legacy discard",
    }));
}
pub(in crate::plan) fn probe_walk(
    plan: &RenderPlan,
    at: AudioSample,
    walked: &Result<AudioWalkSpan<'_>, PlanError>,
    captured: &EditorialCapture,
    work: (usize, usize),
) {
    if !active() {
        return;
    }
    record(json!({
        "kind":"actual_retained_probe_walk","at":at.0,
        "budget_before":work.0,"budget_remaining":work.1,"actual_probe_work_spent":work.0-work.1,
        "editorial":editorial(plan,captured),
        "walk":match walked{
            Ok(value)=>json!({"result":"Ok","span":span(plan,value)}),
            Err(error)=>json!({"result":"Error","error":error_json(error)}),
        },
        "attestation":"same audio_walk_editorial result before actual retained Some/None match",
    }));
}
pub(in crate::plan) fn probe_result(
    expected_plan: &RenderPlan,
    value: Option<&crate::AudioOwnerRetainedInput<'_>>,
    work: usize,
) {
    if !active() {
        return;
    }
    record(json!({
        "kind":"actual_retained_probe_result","work":work,
        "result":if value.is_some(){"Some"}else{"None"},
        "retained_input":value.map(|v|v.capture_baseline_snapshot(expected_plan)),
    }));
    probe_baseline::returned_point(expected_plan, value, work);
}

#[test]
#[ignore = "explicit legacy capture oracle; parent owns old-source and executable pins"]
fn capture_legacy_owner_capture_baseline_v1() {
    let output = std::path::PathBuf::from(
        std::env::var_os("DEADPAN_OWNER_CAPTURE_OUTPUT")
            .expect("explicit new /tmp output path required"),
    );
    assert!(
        output.is_absolute() && (output.starts_with("/tmp") || output.starts_with("/private/tmp"))
    );
    let variant =
        std::env::var("DEADPAN_OWNER_CAPTURE_VARIANT").expect("main/scoped variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let authority = std::env::var("DEADPAN_OWNER_CAPTURE_SOURCE_SHA256")
        .expect("runtime source authority required");
    assert!(authority.len() == 64 && authority.bytes().all(|v| v.is_ascii_hexdigit()));
    let session = Session::start();
    let actual = super::fixtures::matrix();
    let frozen: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/owner-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(
        json!(actual),
        frozen["fixtures"],
        "frozen old-query cases must remain unchanged"
    );
    let names = actual
        .iter()
        .flat_map(|f| f["cases"].as_array().unwrap())
        .map(|v| v["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 14);
    // Derive two ordinary validated documents from the SAME frozen fixture.
    // This is actual legacy compilation/capture, never a fabricated Domain.
    let point = actual
        .iter()
        .find(|f| f["fixture"] == "nonunity_shift_fractional_point_ceil_source44100")
        .unwrap();
    let historical =
        ProjectDocument::from_json(&point["witnesses"]["historical_document"].to_string()).unwrap();
    let stage_plan = RenderPlan::compile(&historical).unwrap();
    let stage = super::case(
        "supplemental_unbound_shift_stage",
        Parent::Root(&stage_plan),
        AudioSample(0)..stage_plan.audio_duration().unwrap(),
        AudioSample(4),
        "stage",
        AudioOwnerKind::Node,
    );
    let mut unbound_source_wire = point["document"].clone();
    unbound_source_wire
        .as_object_mut()
        .unwrap()
        .remove("audio_bindings");
    let source_doc = ProjectDocument::from_json(&unbound_source_wire.to_string()).unwrap();
    let source_plan = RenderPlan::compile(&source_doc).unwrap();
    let source = super::case(
        "supplemental_unbound_source44100",
        Parent::Root(&source_plan),
        AudioSample(0)..source_plan.audio_duration().unwrap(),
        AudioSample(0),
        "voice",
        AudioOwnerKind::Node,
    );
    let supplemental = vec![
        json!({"document":historical,"case":stage}),
        json!({"document":source_doc,"case":source}),
    ];
    let events = session.finish();
    let supplemental_names = supplemental
        .iter()
        .map(|v| v["case"]["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    for name in names.iter().chain(&supplemental_names) {
        let rows = events
            .iter()
            .filter(|v| v["case"] == *name)
            .collect::<Vec<_>>();
        let captures = rows
            .iter()
            .filter(|v| v["observation"]["kind"] == "actual_audio_walk_owner_input")
            .count();
        assert_eq!(captures, 6, "six actual old capture calls for {name}");
        assert!(
            rows.iter()
                .any(|v| v["observation"]["kind"] == "actual_retained_probe_walk")
        );
        assert!(
            rows.iter()
                .any(|v| v["observation"]["kind"] == "actual_retained_probe_result")
        );
    }
    assert!(
        events
            .iter()
            .any(|v| v["observation"]["walk"]["discarded_span"]["tag"] == "Leaf")
    );
    assert!(
        events
            .iter()
            .any(|v| v["observation"]["walk"]["discarded_span"]["tag"] == "Stage")
    );
    assert!(
        events
            .iter()
            .any(|v| v["observation"]["walk"]["result"] == "Error"
                && !v["observation"]["capture_slot"].is_null()),
        "late error retains actual captured slot"
    );
    assert!(
        events
            .iter()
            .any(|v| v["case"] == "supplemental_unbound_shift_stage"
                && v["observation"]["walk"]["discarded_span"]["value"]["content"]["tag"]
                    == "Stage"),
        "supplement must reach the actual AudioStage descriptor"
    );
    assert!(
        events
            .iter()
            .any(|v| v["case"] == "supplemental_unbound_source44100"
                && v["observation"]["walk"]["discarded_span"]["value"]["content"]["type"]
                    == "source"),
        "supplement must reach an actual unbound Source leaf"
    );
    let body = json!({
        "schema":"OwnerCaptureBaselineV1","producer":"actual_legacy_capture_and_probe",
        "variant":variant,"runtime_source_manifest_sha256":authority,
        "ratio_integer_encoding":"normalized base10 strings",
        "fixture_authority":"unchanged frozen SnapshotV1 documents, cases and ordered work/error receipts",
        "fixtures":actual,"supplemental":supplemental,"events":events,
        "excluded":"pointers, raw bytes, Debug, timings, capacities, observer allocation counts",
    });
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &body).unwrap();
    use std::io::Write;
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
