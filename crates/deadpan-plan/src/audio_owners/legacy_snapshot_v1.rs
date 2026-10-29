//! Frozen pre-query-admission logical producer. Test-only, no product API.
//! Run only from the pinned OLD source/executable. Output is create_new only.
use super::*;
use crate::{AudioBoundDomain, AudioOwnerInput, AudioOwnerInputCandidates, PlanError};
use serde_json::{Value, json};

#[path = "legacy_snapshot_v1/fixtures.rs"]
mod fixtures;

#[path = "legacy_snapshot_v1/capture_baseline.rs"]
pub(in crate::plan) mod capture_baseline;

fn exact(value: ExactRatio) -> Value {
    json!({"numerator":value.numerator().to_string(),"denominator":value.denominator().to_string()})
}
fn exact_range(value: &Range<ExactRatio>) -> Value {
    json!({"start":exact(value.start),"end":exact(value.end)})
}
fn sample_range<S: Copy>(value: Range<S>, label: impl Fn(S) -> i64) -> Value {
    json!({"start":label(value.start),"end":label(value.end)})
}
fn iteration(value: &IterationId) -> Value {
    json!({"allocation":value.allocation.as_str(),"ordinal":value.ordinal})
}
fn repeats(values: &[RepeatInstance]) -> Value {
    Value::Array(
        values
            .iter()
            .map(
                |value| json!({"node":value.node.as_str(),"iteration":iteration(&value.iteration)}),
            )
            .collect(),
    )
}
fn instance(value: &InstancePath) -> Value {
    json!({"node":value.node.as_str(),"repeats":repeats(&value.repeats)})
}
fn selector(value: Option<&AudioDefinitionSelector>) -> Value {
    value.map_or(Value::Null, |value| match value {
        AudioDefinitionSelector::Node { node } => json!({"tag":"Node","node":node.as_str()}),
        AudioDefinitionSelector::RepeatDefault { repeat } => {
            json!({"tag":"RepeatDefault","repeat":repeat.as_str()})
        }
        AudioDefinitionSelector::RepeatGap { repeat } => {
            json!({"tag":"RepeatGap","repeat":repeat.as_str()})
        }
    })
}
fn kind(value: AudioOwnerKind) -> &'static str {
    match value {
        AudioOwnerKind::Node => "Node",
        AudioOwnerKind::DefaultGap => "DefaultGap",
    }
}
fn origin(value: &AudioOwnerClockOrigin) -> Value {
    match value {
        AudioOwnerClockOrigin::Current => json!({"tag":"Current"}),
        AudioOwnerClockOrigin::Retained {
            definition,
            instance: path,
            kind: owner_kind,
            gap_after,
        } => json!({
            "tag":"Retained","definition":selector(definition.as_ref()),"instance":instance(path),
            "kind":kind(*owner_kind),"gap_after":gap_after.as_ref().map(iteration)
        }),
    }
}
fn clock<S: Copy>(
    value: &AudioOwnerClock<'_, S>,
    plan: &RenderPlan,
    label: impl Fn(S) -> i64,
) -> Value {
    assert!(
        value.belongs_to(plan),
        "actual observation must retain its expected Plan brand"
    );
    json!({
        "belongs_to_expected_plan":true,"definition":selector(value.definition.as_ref()),
        "instance":instance(&value.instance),"kind":kind(value.kind),
        "gap_after":value.gap_after.as_ref().map(iteration),"play":value.play,
        "origin":origin(&value.origin),"affine":{
            "anchor":label(value.sampling.anchor()),
            "local_at_anchor":exact(value.sampling.local_at_anchor()),
            "local_frames_per_sample":exact(value.sampling.local_frames_per_sample())
        }
    })
}
fn coarse_clock(value: &AudioOwnerClock<'_>) -> Value {
    json!({"definition":selector(value.definition.as_ref()),"instance":instance(&value.instance),
           "kind":kind(value.kind),"gap_after":value.gap_after.as_ref().map(iteration)})
}
pub(super) fn query<S: Copy>(
    value: &AudioOwnerQuery<'_, S>,
    plan: &RenderPlan,
    label: impl Fn(S) -> i64 + Copy,
) -> Value {
    assert!(value.belongs_to(plan));
    json!({
        "belongs_to_expected_plan":true,"window":sample_range(value.samples(),label),
        "work":value.work(),"lookup":{"visited_nodes":value.lookup.visited_nodes,
           "sequence_comparisons":value.lookup.sequence_comparisons,
           "iteration_run_comparisons":value.lookup.iteration_run_comparisons},
        "spans":value.spans().iter().map(|span|json!({
            "samples":sample_range(span.samples(),label),
            "support":match span.support(){AudioOwnerSupport::Active=>"Active",AudioOwnerSupport::Inactive=>"Inactive"},
            "original_is_silent":span.original_is_silent(),
            "owners":span.owners().iter().map(|owner|clock(owner,plan,label)).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    })
}
fn grid<S: serde::Serialize>(value: AudioSampleGrid<S>) -> Value {
    // The public Serialize contract exposes the otherwise private parity bit.
    // Read that logical bit only; no raw bytes, pointers, capacity or Debug text.
    let parity = serde_json::to_value(&value)
        .unwrap()
        .get("rounding_parity")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    json!({"origin":exact(value.frame_origin()),"frames_per_sample":exact(value.frames_per_sample()),
      "rule":match value.boundary_rule(){AudioBoundaryRule::RoundEven=>"RootRoundEven",AudioBoundaryRule::PointCeil=>"PointCeil"},
      "rounding_parity":parity})
}
fn boundary_kind(value: deadpan_core::AudioBoundaryKind) -> &'static str {
    use deadpan_core::AudioBoundaryKind::*;
    match value {
        NodeStart => "NodeStart",
        NodeEnd => "NodeEnd",
        SourcePlacementStart => "SourcePlacementStart",
        SourcePlacementEnd => "SourcePlacementEnd",
        RepeatGapStart => "RepeatGapStart",
        RepeatGapEnd => "RepeatGapEnd",
    }
}
fn constraint(value: &crate::plan::audio::EnvelopeConstraint, plan: &RenderPlan) -> Value {
    let crate::plan::audio::EnvelopeConstraint {
        placement_support,
        range,
        node,
        repeat_count,
        gap_after,
        kinds,
    } = value;
    json!({"placement_support":placement_support,"range":exact_range(range),
       "node":plan.nodes[*node].inspection.id.as_str(),"repeat_count":repeat_count,
       "gap_after":gap_after.as_ref().map(iteration),
       "kinds":[boundary_kind(kinds.0),boundary_kind(kinds.1)]})
}
fn domain(value: &AudioDomain<'_>) -> Value {
    let AudioDomain {
        plan,
        seed,
        samples,
        visible,
        instance: path,
        placement,
    } = value;
    assert!(value.belongs_to(plan));
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
    } = seed;
    let bypass = bypass_binding.map(|target| match target {
        BindingTarget::Node(index) => {
            json!({"tag":"Node","node":plan.nodes[index].inspection.id.as_str()})
        }
        BindingTarget::Gap(index) => {
            json!({"tag":"Gap","node":plan.nodes[index].inspection.id.as_str()})
        }
    });
    json!({
       "belongs_to_expected_plan":true,"root_samples":sample_range(samples.clone(),|v|v.0),
       "visible_samples":sample_range(visible.clone(),|v|v.0),"instance":instance(path),
       "placement":placement.as_ref().map(placement_json),
       "seed":{
          "definition":selector(definition.as_ref()),"bypass_binding":bypass,
          "node":plan.nodes[*node].inspection.id.as_str(),
          "transform":{"project_origin":exact(transform.project_origin),
             "project_frames_per_local_frame":exact(transform.project_frames_per_local_frame),
             "project_frames_per_sample":exact(transform.project_frames_per_sample)},
          "grid":grid(*sample_grid),"extent":exact_range(extent),"envelope":envelope.as_ref().map(exact_range),
          "constraints":constraints.iter().map(|v|constraint(v,plan)).collect::<Vec<_>>(),
          "editorial":editorial.iter().map(|v|{
              let crate::plan::audio::EditorialConstraint{boundary,edges,incident_samples}=v;
              json!({"boundary":constraint(boundary,plan),"edges":{"start":edges.start,"end":edges.end},
                "incident_samples":incident_samples.map(|(a,b)|json!([exact(a),exact(b)]))})
          }).collect::<Vec<_>>(),
          "suppress_entry_editorial":{"start":suppress_entry_editorial.start,"end":suppress_entry_editorial.end},
          "repeats":repeats(path_repeats),
          "retimes":retimes.iter().map(|v|json!({"node":v.node.as_str(),"child_start":exact(v.child_start),
             "child_frames_per_local_frame":exact(v.child_frames_per_local_frame),"pitch":pitch(v.pitch)})).collect::<Vec<_>>(),
          "gap":gap.as_ref().map(|v|json!({"after":v.after.as_ref().map(iteration),"duration_frames":v.duration.frames()}))
       }
    })
}
fn placement_json(value: &crate::AudioRootPlacement) -> Value {
    json!({"origin":exact(value.origin()),"root_frames_per_local_frame":exact(value.root_frames_per_local_frame()),
       "local_support":exact_range(&value.local_support())})
}
fn pitch(value: PitchPolicy) -> Value {
    match value {
        PitchPolicy::Preserve => json!({"tag":"Preserve"}),
        PitchPolicy::FollowSpeed => json!({"tag":"FollowSpeed"}),
        PitchPolicy::Shift { semitones } => json!({"tag":"Shift","semitones":semitones}),
    }
}
fn input(value: &AudioOwnerInput<'_>) -> Value {
    let plan = value.context().plan;
    json!({"observed_at":value.observed_at().0,
       "scope":match value.scope(){crate::AudioOwnerInputScope::Owner=>"Owner",crate::AudioOwnerInputScope::EnclosingProcessor=>"EnclosingProcessor"},
       "owner":clock(value.owner(),plan,|v|v.0),"total_work":value.work(),"domain":domain(value.context())})
}
fn retained_probe(value: &AudioOwnerInput<'_>) -> Value {
    let at = value.observed_at();
    match value.retained_probe_at(at, Default::default()) {
        Err(error) => {
            json!({"result":"Error","error":error_json(&error),"caller_billed_before_error":0})
        }
        Ok(probe) => {
            let body=probe.input().map(|retained|{
            let reference=match retained.domain(){
                AudioBoundDomain::Root(value)=>json!({"tag":"RootRoundEven","domain":domain(value)}),
                AudioBoundDomain::Point(value)=>json!({"tag":"PointCeil","definition":selector(Some(value.definition())),
                    "root":value.root().as_str(),"placement":placement_json(value.placement()),
                    "grid":grid(value.reference_grid()),
                    "samples":sample_range(value.reference_samples(),|v|v.0),
                    "affine":{"anchor":value.reference_sampling().anchor().0,"local_at_anchor":exact(value.reference_sampling().local_at_anchor()),
                       "local_frames_per_sample":exact(value.reference_sampling().local_frames_per_sample())}}),
                AudioBoundDomain::Empty=>json!({"tag":"Empty"})
            };
            let owners=retained.owners(at..AudioSample(at.0+1),Default::default()).unwrap();
            json!({"work":retained.work(),"reference_at_observation":exact(retained.reference_at(at).unwrap()),
                "reference_samples_per_output_sample":exact(retained.reference_samples_per_output_sample()),
                "reference":reference,"observed_owners":query(&owners,value.context().plan,|v|v.0)})
          });
            json!({"result":if probe.input().is_some(){"Some"}else{"None"},"work":probe.work(),"input":body})
        }
    }
}
fn error_json(value: &PlanError) -> Value {
    match value {
        PlanError::AudioRangeOutOfRange => json!({"variant":"AudioRangeOutOfRange"}),
        PlanError::InvalidAudioLimits => json!({"variant":"InvalidAudioLimits"}),
        PlanError::InvalidPlan(message) => json!({"variant":"InvalidPlan","message":message}),
        PlanError::AudioQueryLimit(message) => {
            json!({"variant":"AudioQueryLimit","budget":message})
        }
        PlanError::Document(value) => {
            json!({"variant":"Document","code":value.code,"message":value.message})
        }
        PlanError::Time(value) => json!({"variant":"Time","code":match value{
           TimeError::InvalidRatio=>"InvalidRatio",TimeError::InvalidFrameRate=>"InvalidFrameRate",
           TimeError::InvalidSourceTimeBase=>"InvalidSourceTimeBase",TimeError::NegativeDuration=>"NegativeDuration",
           TimeError::ReversedRange=>"ReversedRange",TimeError::ZeroPlays=>"ZeroPlays",
           TimeError::EmptyRepeatChild=>"EmptyRepeatChild",TimeError::Overflow=>"Overflow"}}),
        _ => panic!("add a named logical serializer for this typed Plan error: {value}"),
    }
}
#[derive(Clone, Copy)]
enum Parent<'context, 'plan> {
    Root(&'plan RenderPlan),
    Domain(&'context AudioDomain<'plan>),
}
impl<'plan> Parent<'_, 'plan> {
    fn plan(self) -> &'plan RenderPlan {
        match self {
            Self::Root(p) => p,
            Self::Domain(d) => d.plan,
        }
    }
    fn query(
        self,
        range: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerQuery<'plan>, PlanError> {
        match self {
            Self::Root(p) => p.audio_gain_owners(range, limits),
            Self::Domain(d) => d.gain_owners(range, limits),
        }
    }
    fn input(
        self,
        owner: &AudioOwnerClock<'_>,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInput<'plan>, PlanError> {
        match self {
            Self::Root(p) => p.audio_owner_input(owner, at, limits),
            Self::Domain(d) => d.owner_input(owner, at, limits),
        }
    }
    fn candidates(
        &self,
        at: AudioSample,
        limits: AudioQueryLimits,
    ) -> Result<AudioOwnerInputCandidates<'_, 'plan>, PlanError> {
        match self {
            Self::Root(p) => p.audio_owner_input_candidates_at(at, limits),
            Self::Domain(d) => d.owner_input_candidates_at(at, limits),
        }
    }
}
fn query_result(
    result: Result<AudioOwnerQuery<'_>, PlanError>,
    plan: &RenderPlan,
    billed: usize,
) -> Value {
    match result {
        Ok(value) => {
            json!({"result":"Ok","query":query(&value,plan,|v|v.0),"successful_receipt_work":value.work()})
        }
        Err(error) => {
            json!({"result":"Error","error":error_json(&error),"caller_prefix_billed_work":billed})
        }
    }
}
fn input_result(result: Result<AudioOwnerInput<'_>, PlanError>, billed: usize) -> Value {
    match result {
        Ok(value) => json!({"result":"Ok","input":input(&value)}),
        Err(error) => {
            json!({"result":"Error","error":error_json(&error),"caller_prefix_billed_work":billed})
        }
    }
}
fn case(
    name: &str,
    parent: Parent<'_, '_>,
    window: Range<AudioSample>,
    at: AudioSample,
    selected: &str,
    selected_kind: AudioOwnerKind,
) -> Value {
    let _capture_scope = capture_baseline::case_scope(name);
    let plan = parent.plan();
    let one = at..AudioSample(at.0 + 1);
    let wide = parent.query(window.clone(), Default::default()).unwrap();
    let observed = parent.query(one.clone(), Default::default()).unwrap();
    let member = observed.spans()[0]
        .owners()
        .iter()
        .position(|v| v.instance.node.as_str() == selected && v.kind == selected_kind)
        .unwrap();
    // This clock/index comes from the independent old membership API FIRST.
    let owner = &observed.spans()[0].owners()[member];
    capture_baseline::phase("legacy_input_ample");
    let legacy = parent.input(owner, at, Default::default()).unwrap();
    let q = observed.work();
    let c = legacy.work().checked_sub(q).unwrap();
    let candidates = parent.candidates(at, Default::default()).unwrap();
    assert_eq!(
        query(candidates.query(), plan, |v| v.0),
        query(&observed, plan, |v| v.0)
    );
    capture_baseline::phase("candidate_capture_ample");
    let captured = candidates.capture(member, Default::default()).unwrap();
    assert_eq!(input(captured.input().unwrap()), input(&legacy));
    assert_eq!(captured.query_work(), q);
    assert_eq!(captured.capture_work(), c);
    let q_limits = |maximum_work| AudioQueryLimits {
        maximum_work,
        ..Default::default()
    };
    let q_exact = parent.query(one.clone(), q_limits(q));
    let q_short = parent.query(one.clone(), q_limits(q - 1));
    capture_baseline::phase("legacy_input_exact");
    let i_exact = parent.input(owner, at, q_limits(legacy.work()));
    capture_baseline::phase("legacy_input_one_short");
    let i_short = parent.input(owner, at, q_limits(legacy.work() - 1));
    capture_baseline::phase("candidate_capture_exact");
    let c_exact = parent
        .candidates(at, Default::default())
        .unwrap()
        .capture(member, q_limits(c));
    capture_baseline::phase("candidate_capture_one_short");
    let c_short = parent
        .candidates(at, Default::default())
        .unwrap()
        .capture(member, q_limits(c - 1));
    capture_baseline::phase("retained_probe_ample");
    json!({"name":name,"parent_kind":match parent{Parent::Root(_)=>"Root",Parent::Domain(_)=>"ActualSeededDomain"},
       "window":sample_range(window,|v|v.0),"observation_window":sample_range(one,|v|v.0),
       "selected_alias":selected,"selected_kind":kind(selected_kind),"selected_member_index":member,
       "ordered_window_query":query(&wide,plan,|v|v.0),"observed_query":query(&observed,plan,|v|v.0),
       "legacy_input":input(&legacy),"legacy_q":q,"legacy_c":c,"legacy_total_work":legacy.work(),
       "pre_admission_candidates_input":input(captured.input().unwrap()),"retained_probe":retained_probe(&legacy),
       "legacy_success_only_receipts":true,
       "query_exact":query_result(q_exact,plan,0),"query_one_short":query_result(q_short,plan,0),
       "input_exact":input_result(i_exact,0),"input_one_short":input_result(i_short,0),
       "candidate_capture_exact":match c_exact{Ok(value)=>json!({"result":"Ok","q":value.query_work(),"c":value.capture_work()}),
          Err(error)=>json!({"result":"Error","error":error_json(&error),"caller_prefix_billed_work":q})},
       "candidate_capture_one_short":match c_short{Ok(value)=>json!({"result":"Ok","q":value.query_work(),"c":value.capture_work()}),
          Err(error)=>json!({"result":"Error","error":error_json(&error),"caller_prefix_billed_work":q})}
    })
}

#[test]
#[ignore = "explicit old-kernel SnapshotV1 capture only; parent owns build/runtime pins"]
fn capture_legacy_snapshot_v1() {
    let Some(output) = std::env::var_os("DEADPAN_OWNER_SNAPSHOT_OUTPUT") else {
        panic!("explicit /tmp output path required");
    };
    let output = std::path::PathBuf::from(output);
    assert!(
        output.is_absolute() && (output.starts_with("/tmp") || output.starts_with("/private/tmp"))
    );
    let variant =
        std::env::var("DEADPAN_OWNER_SNAPSHOT_VARIANT").expect("main/scoped variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_OWNER_SNAPSHOT_SOURCE_SHA256")
        .expect("pinned old runtime-source authority required");
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    let body = fixtures::matrix();
    let snapshot = json!({"schema":"SnapshotV1","producer":"legacy_pre_query_admission","variant":variant,
       "runtime_source_manifest_sha256":source,"ratio_integer_encoding":"normalized base10 strings",
       "work_receipts":"legacy successful provenance; errors carry only already-billed caller prefix",
       "fixtures":body});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &snapshot).unwrap();
    use std::io::Write;
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

#[path = "owner_query_admission_tests.rs"]
mod admitted;

#[path = "legacy_snapshot_v1/wide_baseline.rs"]
pub(in crate::plan) mod wide_baseline;

#[path = "legacy_snapshot_v1/direct_input_baseline.rs"]
pub(in crate::plan) mod direct_input_baseline;

#[path = "legacy_snapshot_v1/span_start_baseline.rs"]
pub(in crate::plan) mod span_start_baseline;

#[path = "legacy_snapshot_v1/span_start_certificate_tests.rs"]
mod span_start_certificate_tests;
