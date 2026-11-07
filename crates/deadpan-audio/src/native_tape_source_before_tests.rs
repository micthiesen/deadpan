//! Pre-J native source observations. G destination admission remains active.
use super::*;
use crate::stages::{ReadWork, WorkControl, native_tape_source_before_trace, point_suppression};
use std::alloc::Layout;
use std::cell::RefCell;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

const CHECKPOINT: &str = "6a8bd46f0f5128e52afbe93e4fbeed28026fb262";

struct Traces;
impl Drop for Traces {
    fn drop(&mut self) {
        native_tape_source_before_trace::take();
        super::super::projected_mask_before_trace::take();
        point_suppression::ops::trace::disable();
        point_suppression::work_trace::disable();
    }
}
fn begin() -> Traces {
    native_tape_source_before_trace::begin();
    super::super::projected_mask_before_trace::begin();
    point_suppression::ops::trace::reset();
    point_suppression::work_trace::reset();
    Traces
}
fn receipts() -> Value {
    let policy = native_tape_source_before_trace::take();
    let source = serde_json::to_value(super::super::projected_mask_before_trace::take()).unwrap();
    assert_eq!(policy["dropped"], 0);
    assert_eq!(source["dropped"], 0);
    assert!(
        source["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["source_ranges_truncated"] == false)
    );
    let mut result = json!({"ordinary_policy":policy,"G_source_ready":source,
        "mask_operations":point_suppression::ops::trace::snapshot(),
        "public_owner_work":point_suppression::work_trace::snapshot()});
    if let Some(events) = pairs::take_case_trace() {
        result["J_admitted_tape_source"] = events;
    }
    result
}
fn layout<T>(capacity: usize) -> Value {
    let value = Layout::array::<T>(capacity).unwrap();
    json!({"capacity":capacity,"size":value.size(),"align":value.align()})
}
fn deps(value: &Dependencies) -> Value {
    json!(
        value
            .iter()
            .map(|(a, b)| (a.as_str(), b))
            .collect::<BTreeMap<_, _>>()
    )
}
fn error(value: &StageAudioError) -> Value {
    let kind = match value {
        StageAudioError::Range => "Range",
        StageAudioError::PreparationRange => "PreparationRange",
        StageAudioError::ForeignDomain => "ForeignDomain",
        StageAudioError::ForeignDefinition => "ForeignDefinition",
        StageAudioError::InvalidLimits => "InvalidLimits",
        StageAudioError::Limit(_) => "Limit",
        StageAudioError::Timeout => "Timeout",
        StageAudioError::Unsupported(_) => "Unsupported",
        StageAudioError::Preparation(_) => "Preparation",
        StageAudioError::Plan(_) => "Plan",
        StageAudioError::Time(_) => "Time",
        StageAudioError::Gain(_) => "Gain",
        StageAudioError::Dsp(_) => "Dsp",
        StageAudioError::Filter(_) => "Filter",
        StageAudioError::Wet(_) => "Wet",
        StageAudioError::Transfer(_) => "Transfer",
        StageAudioError::Cache(_) => "Cache",
    };
    json!({"kind":kind,"debug":format!("{value:?}"),"display":value.to_string(),"cancelled":value.is_cancelled(),
        "failed_constructor_local_work":"unexposed; accepted ReadWork is recorded independently"})
}
fn mask(value: &SignalSuppressionMask) -> Value {
    json!({"values":value.as_slice().iter().map(|r|[r.start.0,r.end.0]).collect::<Vec<_>>(),
        "len":value.len(),"capacity":value.capacity(),"owned":value.is_owned(),
        "layout":layout::<Range<SignalSample>>(value.capacity()),
        "first":value.first().map(|r|[r.start.0,r.end.0]),"last":value.last().map(|r|[r.start.0,r.end.0]),
        "reverse":value.iter().rev().map(|r|[r.start.0,r.end.0]).collect::<Vec<_>>(),
        "iter_size_hint":value.iter().size_hint()})
}
fn signal_result(value: &Result<SignalBlock, StageAudioError>) -> Value {
    match value {
        Ok(SignalBlock {
            samples,
            dependencies,
            suppressed,
            relative_depth,
        }) => json!({"status":"Ok",
            "f32_bits":samples_as_bits(samples),"sample_len":samples.len(),"sample_storage":layout::<[f32;2]>(samples.capacity()),
            "dependencies":deps(dependencies),"relative_depth":relative_depth,"suppressed":mask(suppressed)}),
        Err(e) => json!({"status":"Error","error":error(e)}),
    }
}
fn public_result(value: &Result<RoutedSignalBlock, StageAudioError>) -> Value {
    match value {
        Ok(RoutedSignalBlock {
            schema_version,
            stage,
            project_id,
            revision_id,
            allocation,
            start,
            samples,
            suppressed,
        }) => {
            json!({"status":"Ok","schema_version":schema_version,"stage":stage,"project_id":project_id,
                "revision_id":revision_id,"allocation":[allocation.start.0,allocation.end.0],"start":start.0,
                "f32_bits":samples_as_bits(samples),"sample_len":samples.len(),
                "sample_storage":layout::<[f32;2]>(samples.capacity()),"suppressed":mask(suppressed)})
        }
        Err(e) => json!({"status":"Error","error":error(e)}),
    }
}
fn prepared(value: &PreparedBlock) -> Value {
    let PreparedBlock {
        samples,
        dependencies,
        relative_depth,
    } = value;
    let storage = match samples {
        PreparedPcm::Memory(values) => json!({
            "kind":"Memory","sample_storage":layout::<[f32;2]>(values.capacity()),
            "interleaved_f32_bits":values.iter().flat_map(|v|v.iter().map(|v|v.to_bits())).collect::<Vec<_>>()
        }),
        PreparedPcm::Silence(frames) => json!({"kind":"Silence","frames":frames}),
        PreparedPcm::File { frames, .. } => {
            let pcm = samples
                .read_window(0..*frames, &AtomicBool::new(false))
                .unwrap();
            json!({"kind":"File","frames":frames,
                "interleaved_f32_bits":pcm.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
                "observation_storage":"read_window scratch outside ledger; actual file handle is opaque"})
        }
    };
    json!({"storage":storage,"frames":samples.len(),"resident_frames":samples.resident_frames(),
        "disk_bytes":samples.disk_bytes(),"dependencies":deps(dependencies),"relative_depth":relative_depth})
}
fn work(value: &RefCell<ReadWork>) -> Value {
    let value = value.borrow();
    let ReadWork {
        clock_remaining: _,
        checks_since_clock: _,
        prepared_stages,
        prepared_frames,
        source_checks,
        observed,
        preflight_assets,
        plan_work,
        cache_plan_work,
        cache_key_scopes,
        projected,
        projected_active,
        projected_preflight,
        intrinsic_preflight,
        projected_resident_frames,
        sound_context_frames,
        projected_disk_bytes,
    } = &*value;
    json!({"prepared_stages":prepared_stages,"prepared_frames":prepared_frames,"source_checks":source_checks,
        "observed":deps(observed),"preflight_assets":preflight_assets.iter().map(|v|v.as_str()).collect::<Vec<_>>(),
        "plan_work":plan_work,"cache_plan_work":cache_plan_work,"cache_key_scopes":cache_key_scopes,
        "projected_blocks":projected.iter().map(|(_,b)|prepared(b)).collect::<Vec<_>>(),
        "projected_storage":layout::<(stage_projection::ProjectionCacheKey,Arc<PreparedBlock>)>(projected.capacity()),
        "projected_active_len":projected_active.len(),"projected_active_capacity":projected_active.capacity(),
        "projected_preflight_depths":projected_preflight.iter().map(|(_,d)|d).collect::<Vec<_>>(),
        "projected_preflight_capacity":projected_preflight.capacity(),
        "intrinsic_preflight":intrinsic_preflight.iter().map(|(p,s,d)|json!({"project_id":p.metadata().project_id,
            "revision_id":p.metadata().revision_id,"stage":s,"depth":d})).collect::<Vec<_>>(),
        "intrinsic_preflight_capacity":intrinsic_preflight.capacity(),
        "projected_resident_frames":projected_resident_frames,"sound_context_frames":sound_context_frames,
        "projected_disk_bytes":projected_disk_bytes,
        "opaque_projection_identity":"key bodies private to Plan; this capture records blocks/counters, not fabricated key addresses"})
}
fn controller(value: &StageAudio) -> Value {
    json!({"active_frames":value.active_frames,"active_disk_bytes":value.active_disk_bytes,
        "cache_len":value.cache.len(),"cache_capacity":value.cache.capacity(),
        "cache":value.cache.iter().map(|v| {
            let key=match &v.key {
                PreparedKey::Preserve(stage)=>json!({"kind":"Preserve","stage":stage}),
                PreparedKey::Hold {definition,instance,gap_after,content}=>json!({"kind":"Hold",
                    "definition":definition,"instance":instance,"gap_after":gap_after,"content":content}),
            };
            json!({"project_id":v.plan.metadata().project_id,"revision_id":v.plan.metadata().revision_id,
                "same_current_plan":Arc::ptr_eq(&v.plan,&value.plan),"key":key,"block":prepared(&v.block)})
        }).collect::<Vec<_>>(),
        "original_input_ids":value.original_inputs.keys().collect::<Vec<_>>(),
        "next_original_input":value.next_original_input,
        "sound_processing_plans":value.sound_processing_plans.iter().map(|(id,p)|json!({"id":id,
            "project_id":p.metadata().project_id,"revision_id":p.metadata().revision_id})).collect::<Vec<_>>(),
        "feeding_tail":value.feeding_tail,"has_monitor":value.preparation_monitor.is_some(),
        "has_persistent_cache":value.persistent_cache.is_some(),
        "cached_resident_frames":value.cached_resident_frames(),"cached_disk_bytes":value.cached_disk_bytes(),
        "owned_account":value.owned_suppression.live_frames(),
        "limits":{"input":value.limits.maximum_input_frames,"output":value.limits.maximum_output_frames,
            "resident":value.limits.maximum_resident_frames,"cached_stages":value.limits.maximum_cached_stages,
            "depth":value.limits.maximum_depth,"prepared_stages":value.limits.maximum_prepared_stages,
            "prepared_frames":value.limits.maximum_prepared_frames,"memory_stage":value.limits.maximum_memory_stage_frames,
            "disk":value.limits.maximum_disk_bytes}})
}
struct Provider<'a> {
    inner: super::super::super::controlled_reads::Provider,
    work: Option<&'a RefCell<ReadWork>>,
    events: Vec<Value>,
}
impl AudioSourceProvider for Provider<'_> {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.events.push(json!({"project":project,"revision":revision,"asset":asset,
            "cancelled":cancelled.load(Ordering::Relaxed),"accepted_before_callback":self.work.map(work)}));
        self.inner.source(project, revision, asset, cancelled)
    }
}
fn public_case(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    name: &str,
    start: i64,
    frames: u32,
    cancel: bool,
) -> Value {
    let (_, inner) = checked_plan_with_silent_hold();
    public_case_with_provider(plan, projection, name, start, frames, cancel, inner)
}
fn public_case_with_provider(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    name: &str,
    start: i64,
    frames: u32,
    cancel: bool,
    inner: crate::stages::controlled_reads::Provider,
) -> Value {
    let mut provider = Provider {
        inner,
        work: None,
        events: Vec::new(),
    };
    let domain = routed(Arc::clone(projection));
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let before = controller(&renderer);
    let _traces = begin();
    let result = renderer.read_routed_signal(
        &mut provider,
        &domain,
        SignalSample(start),
        frames,
        TIMEOUT,
        &AtomicBool::new(cancel),
    );
    let observed = receipts();
    let output = public_result(&result);
    let account = renderer.owned_suppression.observer();
    let after = controller(&renderer);
    let calls = provider.inner.calls;
    let events = std::mem::take(&mut provider.events);
    drop(provider);
    drop(renderer);
    let held = public_result(&result);
    let held_frames = account.as_ref().map(|v| v.live_frames());
    drop(result);
    json!({"case":name,"entry":"public read_routed_signal","start":start,"frames":frames,"cancelled_at_entry":cancel,
        "before":before,"after":after,"result":output,"result_after_provider_controller_drop":held,
        "account_after_controller_drop_result_live":held_frames,
        "account_after_result_drop":account.as_ref().map(|v|v.live_frames()),"provider_calls":calls,"provider_events":events,
        "receipts":observed,"public_internal_ReadWork":"inaccessible after public method; no invented budget"})
}
fn warm_case(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    name: &str,
    start: i64,
    frames: u32,
    mode: &str,
) -> Value {
    let (_, inner) = checked_plan_with_silent_hold();
    warm_case_with_provider(plan, projection, name, start, frames, mode, inner)
}
fn warm_case_with_provider(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    name: &str,
    start: i64,
    frames: u32,
    mode: &str,
    inner: crate::stages::controlled_reads::Provider,
) -> Value {
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut provider = Provider {
        inner,
        work: Some(&budget),
        events: Vec::new(),
    };
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: Instant::now() + TIMEOUT,
    };
    let _traces = begin();
    let initial = controller(&renderer);
    let setup = renderer.prepare_projected(projection, &mut provider, control, 1);
    let setup_receipts = receipts();
    let before_work = work(&budget);
    let before = controller(&renderer);
    if let Err(e) = &setup {
        return json!({"case":name,"entry":"warm private read_projected_signal_controlled",
            "start":start,"frames":frames,"mode":mode,"initial":initial,
            "setup_error":error(e),"setup_receipts":setup_receipts,
            "work_after_failed_setup":before_work,"controller_after_failed_setup":before,
            "provider_calls":provider.inner.calls,"provider_events":provider.events,
            "result":{"status":"NotRun","reason":"actual prepare_projected failed"}});
    }
    let result = match &setup {
        Ok(_) => {
            if mode == "resident_exact" || mode == "resident_below" {
                let probe = projection
                    .output_policy()
                    .policy_after_preserve(
                        SignalSample(start)..SignalSample(start + i64::from(frames)),
                        AudioQueryLimits::default(),
                    )
                    .unwrap();
                let n = merged_suppression(probe.suppressed).len();
                let destination = if n == 0 {
                    0
                } else {
                    point_suppression::shell_frames().unwrap()
                        + point_suppression::signal_payload_frames(n).unwrap()
                };
                let resident = budget.borrow().projected_resident_frames;
                renderer.limits.maximum_resident_frames =
                    u32::try_from(resident + destination - u64::from(mode == "resident_below"))
                        .unwrap();
            }
            if mode == "depth" {
                renderer.limits.maximum_depth = 0;
            }
            if mode == "cancel" {
                cancelled.store(true, Ordering::Relaxed);
            }
            native_tape_source_before_trace::begin();
            super::super::projected_mask_before_trace::begin();
            let control = WorkControl {
                deadline: if mode == "deadline" {
                    Instant::now() - Duration::from_secs(1)
                } else {
                    control.deadline
                },
                ..control
            };
            renderer.read_projected_signal_controlled(
                projection,
                &mut provider,
                SignalSample(start),
                frames,
                control,
            )
        }
        Err(_) => unreachable!("failed setup returned above"),
    };
    let after_work = work(&budget);
    let output = signal_result(&result);
    let observed = receipts();
    let prepared_snapshot = setup.as_ref().ok().map(|v| prepared(v));
    let strong = setup.as_ref().ok().map(Arc::strong_count);
    let account = renderer.owned_suppression.observer();
    let after = controller(&renderer);
    let calls = provider.inner.calls;
    let events = std::mem::take(&mut provider.events);
    drop(provider);
    drop(budget);
    let held_after_work = signal_result(&result);
    let prepared_after_work = setup.as_ref().ok().map(|v| prepared(v));
    drop(renderer);
    let held_after_controller = signal_result(&result);
    let account_live = account.as_ref().map(|v| v.live_frames());
    drop(result);
    let account_after_result = account.as_ref().map(|v| v.live_frames());
    drop(setup);
    json!({"case":name,"entry":"warm private read_projected_signal_controlled","start":start,"frames":frames,"mode":mode,
        "initial":initial,"setup_error":Value::Null,"setup_receipts":setup_receipts,
        "before":before,"after":after,"work_before":before_work,"work_after":after_work,
        "prepared":prepared_snapshot,"prepared_strong_count_before_work_drop":strong,"prepared_after_work_drop":prepared_after_work,
        "result":output,"result_after_work_drop":held_after_work,"result_after_controller_drop":held_after_controller,
        "account_after_controller_drop_result_live":account_live,"account_after_result_drop":account_after_result,
        "account_after_prepared_drop":account.as_ref().map(|v|v.live_frames()),
        "provider_calls":calls,"provider_events":events,"receipts":observed})
}
fn source_projection<'a>(
    plan: &'a RenderPlan,
    base: &AudioStageProjection<'a>,
    output: bool,
    fit_beat: bool,
) -> Result<Arc<AudioStageProjection<'a>>, PlanError> {
    let root = plan.audio_signal();
    let query = root.query(
        SignalSample(0)..SignalSample(1),
        AudioQueryLimits::default(),
    )?;
    let source = query
        .spans
        .iter()
        .find_map(|s| match &s.content {
            AudioSignalContent::Stage(stage) => {
                let q = stage
                    .input_signal()
                    .query(
                        SignalSample(0)..SignalSample(1),
                        AudioQueryLimits::default(),
                    )
                    .ok()?;
                q.spans.iter().find_map(|s| match &s.content {
                    AudioSignalContent::Leaf(deadpan_plan::AudioContent::Source {
                        source, ..
                    }) => Some(source.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .ok_or(PlanError::NoSourceAudio)?;
    let voice = root.source_voice(deadpan_plan::AudioSourceVoiceRecipe {
        mapping: if fit_beat {
            deadpan_core::SourceAudioMapping::FitBeat
        } else {
            deadpan_core::SourceAudioMapping::natural_rate(
                source.span,
                plan.metadata().presentation_basis.frame_rate,
            )?
        },
        source,
        offset: AudioSample(0),
    })?;
    let signal = if output {
        voice.output_signal()
    } else {
        voice.input_signal()
    };
    let input = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ratio(640, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(640, 1),
            ExactRatio::ZERO..ratio(640, 1),
            signal.clone(),
        )],
    )?;
    let policy = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ratio(768, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(768, 1),
            ExactRatio::ZERO..ratio(640, 1),
            signal,
        )],
    )?;
    AudioStageProjection::new(
        base.stage().clone(),
        input,
        policy,
        FrameDuration::new(768)?,
    )
}
pub(in crate::stages) fn capture_cases() -> Value {
    super::super::projected_tape_source_request::with_legacy_projected_tape_source(
        capture_cases_legacy,
    )
}

fn capture_cases_legacy() -> Value {
    let (plan, _provider) = checked_plan_with_silent_hold();
    let projection = projected(&plan);
    let public = vec![
        public_case(&plan, &projection, "empty_policy", 0, 256, false),
        public_case(&plan, &projection, "positive_policy", 512, 256, false),
        public_case(&plan, &projection, "crop_positive", 640, 64, false),
        public_case(&plan, &projection, "outside_right", 768, 1, false),
        public_case(&plan, &projection, "outside_left", -1, 1, false),
        public_case(&plan, &projection, "empty_request", 0, 0, false),
        public_case(&plan, &projection, "cancelled_positive", 512, 256, true),
    ];
    let mut warm = Vec::new();
    for (name, start, frames, mode) in [
        ("warm_positive", 512, 256, "default"),
        ("warm_empty", 0, 256, "default"),
        ("warm_exact_positive", 512, 256, "resident_exact"),
        ("warm_below_positive", 512, 256, "resident_below"),
        ("warm_exact_empty", 0, 256, "resident_exact"),
        ("warm_cancel", 512, 256, "cancel"),
        ("warm_deadline", 512, 256, "deadline"),
        ("warm_depth", 512, 256, "depth"),
        ("warm_late_outside", 768, 1, "default"),
    ] {
        warm.push(warm_case(&plan, &projection, name, start, frames, mode));
    }
    let mut providers = Vec::new();
    for (output, fit_beat) in [(false, false), (true, false), (true, true)] {
        match source_projection(&plan,&projection,output,fit_beat) {
            Ok(p)=>providers.push(json!({"source_output":output,"fit_beat":fit_beat,"setup":"Ok","public":public_case(&plan,&p,"source_provider",512,256,false),
                "warm":warm_case(&plan,&p,"source_provider_warm",512,256,"default")})),
            Err(e)=>providers.push(json!({"source_output":output,"fit_beat":fit_beat,"setup":"Error","error":error(&StageAudioError::Plan(e))})),
        }
    }
    json!({"schema":"native-tape-source-before-v1","scope":{
        "current":"ordinary Tape source policy and G active destination copy",
        "source_policy":"ordinary inventory/ranges unadmitted; observer storage outside measured ledger",
        "native_proof":"actual checked public projection and real WAV; warm boundary rows separate from cold public requests",
        "gaps":["public request length is unsigned, so reversed request is not representable",
            "private late-range attempt retained with actual result; not a public acceptance claim",
            "Source projection attempts retained literally; no Silence or nested Projected native provider fabricated",
            "opaque Plan projection identity bodies unavailable; blocks and all ReadWork counters are recorded",
            "no query issuer/source lease exists in this ordinary native path",
            "no natural native work-ceiling exhaustion witness in this bounded fixture; no accepted counters filled",
            "source-rate FitBeat attempt keeps actual setup or preflight rejection, not promised success"]},
        "public":public,"warm":warm,"source_provider_attempts":providers,
        "qualified_wav_family":qualified_wav::capture_family()})
}
#[test]
#[ignore = "manual fresh pre-J native Tape source capture"]
fn capture_native_tape_source_before_v1() {
    let output = std::env::var_os("DEADPAN_NATIVE_TAPE_SOURCE_BEFORE_OUTPUT")
        .expect("fresh absolute /tmp path");
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_NATIVE_TAPE_SOURCE_BEFORE_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_NATIVE_TAPE_SOURCE_BEFORE_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_NATIVE_TAPE_SOURCE_BEFORE_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT
    );
    let mut body = capture_cases();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(CHECKPOINT);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&body).unwrap())
        .unwrap();
    file.write_all(b"\n").unwrap();
    file.flush().unwrap();
    file.sync_all().unwrap();
}
#[test]
#[ignore = "requires parent-frozen external full-body fixture38 after both actual captures"]
fn native_tape_source_before_external_body_regression() {
    let path = std::env::var_os("DEADPAN_NATIVE_TAPE_SOURCE_BEFORE_EXPECTED")
        .expect("frozen canonical body path");
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(capture_cases(), expected);
}

#[path = "native_tape_source_qualified_before.rs"]
mod qualified_wav;

#[test]
fn native_tape_source_frozen_before_body_is_exact() {
    let expected: Value = serde_json::from_slice(include_bytes!(
        "../tests/fixtures/native-tape-source-baseline-v1/corpus.json"
    ))
    .unwrap();
    assert_eq!(capture_cases(), expected);
}

#[path = "native_tape_source_after_tests.rs"]
mod after_j;

#[path = "native_tape_source_pairs.rs"]
mod pairs;
