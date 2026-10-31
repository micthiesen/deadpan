//! Prepare-only OLD retained reader witness; execute only through pinned parent wrappers.
use super::*;
use deadpan_plan::{AudioOwnerClock, AudioOwnerClockOrigin, AudioOwnerRetainedInput};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::Ordering;

#[path = "retained_host_cases.rs"]
mod cases;

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
enum Mode {
    OrdinaryProbe,
    AdmittedFalseProbe,
    AdmittedTrueProbe,
    ActualSelectedShared,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::OrdinaryProbe => "ordinary_probe",
            Self::AdmittedFalseProbe => "actual_admitted_probe_unmetered",
            Self::AdmittedTrueProbe => "actual_admitted_probe_metered",
            Self::ActualSelectedShared => "actual_selected_then_shared",
        }
    }
}
#[derive(Clone, Copy)]
enum Fault {
    None,
    InitialCancel,
    ExpiredDeadline,
    ProviderFailure,
    ProviderCancel,
    Depth,
    ZeroFrames,
    TooManyFrames,
    Overflow,
    Resident,
    Remaining(usize),
    CacheLane,
    CacheMinimumAllowance,
}
impl Fault {
    fn name(self) -> &'static str {
        match self {
            Self::None => "default",
            Self::InitialCancel => "cancelled_before_read",
            Self::ExpiredDeadline => "expired_deadline_before_read",
            Self::ProviderFailure => "provider_failure_behavior",
            Self::ProviderCancel => "provider_cancel_behavior",
            Self::Depth => "depth_one_above_limit",
            Self::ZeroFrames => "zero_frames",
            Self::TooManyFrames => "output_frames_one_above_limit",
            Self::Overflow => "signed_end_overflow",
            Self::Resident => "one_resident_frame",
            Self::Remaining(_) => "injected_prior_plan_work",
            Self::CacheLane => "actual_cache_key_scope",
            Self::CacheMinimumAllowance => "cache_scope_at_query_allowance",
        }
    }
}
fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + TIMEOUT,
        work,
    }
}
fn dependencies(value: &Dependencies) -> Value {
    json!(
        value
            .iter()
            .map(|(asset, digest)| (asset.as_str(), digest))
            .collect::<BTreeMap<_, _>>()
    )
}
fn ranges(value: &[Range<AudioSample>]) -> Value {
    json!(
        value
            .iter()
            .map(|r| [r.start.0, r.end.0])
            .collect::<Vec<_>>()
    )
}
fn read_block(value: &ReadBlock) -> Value {
    let ReadBlock {
        start,
        samples,
        dependencies: deps,
        relative_depth,
        suppressed,
        exhausted,
    } = value;
    json!({"start":start.0,"f32_bits":samples.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "dependencies":dependencies(deps),"relative_depth":relative_depth,
        "suppressed":ranges(suppressed),"exhausted":ranges(exhausted)})
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
    json!({"kind":kind,"debug":format!("{value:?}"),"display":value.to_string(),"is_cancelled":value.is_cancelled()})
}
fn prepared_block(value: &PreparedBlock) -> Value {
    let PreparedBlock {
        samples,
        dependencies: deps,
        relative_depth,
    } = value;
    let pcm = samples
        .read_window(0..samples.len(), &AtomicBool::new(false))
        .unwrap();
    let storage = match samples {
        PreparedPcm::Memory(_) => "Memory",
        PreparedPcm::File { .. } => "File",
        PreparedPcm::Silence(_) => "Silence",
    };
    json!({"storage":storage,"frames":samples.len(),"resident_frames":samples.resident_frames(),
        "disk_bytes":samples.disk_bytes(),"interleaved_f32_bits":pcm.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
        "dependencies":dependencies(deps),"relative_depth":relative_depth})
}
fn plan_identity(plan: &RenderPlan) -> Value {
    json!({"project_id":plan.metadata().project_id,"revision_id":plan.metadata().revision_id,
        "presentation_basis":plan.metadata().presentation_basis})
}
fn projection_key(value: &stage_projection::ProjectionCacheKey) -> Value {
    use stage_projection::ProjectionCacheKey;
    match value {
        ProjectionCacheKey::Opaque(identity) => {
            json!({"kind":"Opaque","token_debug":format!("{identity:?}")})
        }
        ProjectionCacheKey::Source(plan, key) => {
            json!({"kind":"Source","plan":plan_identity(plan),"complete_key_debug":format!("{key:?}")})
        }
        ProjectionCacheKey::Owned(plan, key) => {
            json!({"kind":"Owned","plan":plan_identity(plan),"complete_key_debug":format!("{key:?}")})
        }
    }
}
fn work_snapshot(value: &RefCell<ReadWork>) -> Value {
    let value = value.borrow();
    let ReadWork {
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
    let keys = projected
        .iter()
        .map(|(key, _)| key)
        .chain(projected_active.iter())
        .chain(projected_preflight.iter().map(|(key, _)| key))
        .collect::<Vec<_>>();
    let key_equalities = keys
        .iter()
        .map(|a| keys.iter().map(|b| *a == *b).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    json!({
        "prepared_stages":prepared_stages,"prepared_frames":prepared_frames,
        "source_checks":source_checks,"observed":dependencies(observed),
        "preflight_assets":preflight_assets.iter().map(AssetId::as_str).collect::<Vec<_>>(),
        "plan_work":plan_work,"cache_plan_work":cache_plan_work,"cache_key_scopes":cache_key_scopes,
        "projected":projected.iter().map(|(key,block)|json!({"key":projection_key(key),"block":prepared_block(block)})).collect::<Vec<_>>(),
        "projected_capacity":projected.capacity(),
        "projected_active":projected_active.iter().map(projection_key).collect::<Vec<_>>(),
        "projected_active_capacity":projected_active.capacity(),
        "projected_preflight":projected_preflight.iter().map(|(key,depth)|json!({"key":projection_key(key),"depth":depth})).collect::<Vec<_>>(),
        "projected_preflight_capacity":projected_preflight.capacity(),
        "intrinsic_preflight":intrinsic_preflight.iter().map(|(plan,stage,depth)|json!({"plan":plan_identity(plan),"stage":stage,"depth":depth})).collect::<Vec<_>>(),
        "intrinsic_preflight_capacity":intrinsic_preflight.capacity(),
        "projection_key_equality_order":"projected, then active, then preflight; each vector preserves actual order",
        "projection_key_equalities":key_equalities,
        "projected_resident_frames":projected_resident_frames,
        "sound_context_frames":sound_context_frames,"projected_disk_bytes":projected_disk_bytes,
    })
}
fn controller_snapshot(value: &StageAudio) -> Value {
    json!({
        "plan":plan_identity(&value.plan),"limits":{
            "maximum_input_frames":value.limits.maximum_input_frames,
            "maximum_output_frames":value.limits.maximum_output_frames,
            "maximum_resident_frames":value.limits.maximum_resident_frames,
            "maximum_cached_stages":value.limits.maximum_cached_stages,
            "maximum_depth":value.limits.maximum_depth,
            "maximum_prepared_stages":value.limits.maximum_prepared_stages,
            "maximum_prepared_frames":value.limits.maximum_prepared_frames,
            "maximum_memory_stage_frames":value.limits.maximum_memory_stage_frames,
            "maximum_disk_bytes":value.limits.maximum_disk_bytes},
        "active_frames":value.active_frames,"active_disk_bytes":value.active_disk_bytes,
        "cached_resident_frames":value.cached_resident_frames(),"cached_disk_bytes":value.cached_disk_bytes(),
        "cache":value.cache.iter().map(|stage|json!({"plan":plan_identity(&stage.plan),
            "same_current_plan":Arc::ptr_eq(&stage.plan,&value.plan),
            "key_debug":format!("{:?}",stage.key),"block":prepared_block(&stage.block)})).collect::<Vec<_>>(),
        "cache_capacity":value.cache.capacity(),"original_input_ids":value.original_inputs.keys().collect::<Vec<_>>(),
        "next_original_input":value.next_original_input,
        "sound_processing_plan_ids":value.sound_processing_plans.iter().map(|(id,plan)|json!({"id":id,"plan":plan_identity(plan)})).collect::<Vec<_>>(),
        "feeding_tail":value.feeding_tail,"has_monitor":value.preparation_monitor.is_some(),
        "has_persistent_cache":value.persistent_cache.is_some(),
    })
}
struct ObservedProvider<'a> {
    prepared: crate::PreparedSource,
    calls: usize,
    events: Vec<Value>,
    work: &'a RefCell<ReadWork>,
    fault: Fault,
}
impl AudioSourceProvider for ObservedProvider<'_> {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.calls += 1;
        let before = work_snapshot(self.work);
        let cancelled_before = cancelled.load(Ordering::Relaxed);
        let outcome = match self.fault {
            Fault::ProviderFailure => "SourceUnavailable",
            Fault::ProviderCancel => {
                cancelled.store(true, Ordering::Relaxed);
                "Cancelled"
            }
            _ => "PreparedSource",
        };
        self.events.push(json!({"ordinal":self.calls,"project_id":project,"revision_id":revision,
            "asset":asset,"cancelled_before":cancelled_before,
            "cancelled_after":cancelled.load(Ordering::Relaxed),"work_before_provider_result":before,
            "outcome":outcome,"prepared_provenance":self.prepared.provenance(),
            "prepared_index":self.prepared.index(),"matrix_layout":self.prepared.matrix_layout()}));
        match self.fault {
            Fault::ProviderFailure => Err(PreparationError::SourceUnavailable(
                "retained OLD provider sentinel".into(),
            )),
            Fault::ProviderCancel => Err(PreparationError::Cancelled),
            _ => Ok(&self.prepared),
        }
    }
}
fn owner_at<'a>(plan: &'a RenderPlan, name: &str, at: AudioSample) -> AudioOwnerClock<'a> {
    plan.audio_gain_owners(
        at..AudioSample(at.0.checked_add(1).unwrap()),
        Default::default(),
    )
    .unwrap()
    .spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == id(name))
        .unwrap()
        .clone()
}
fn domain(value: &AudioDomain<'_>, plan: &RenderPlan) -> Value {
    json!({"belongs_to_expected_plan":value.belongs_to(plan),"definition":value.definition(),
        "placement":value.placement(),"root_samples":value.root_samples(),"root_extent":value.root_extent(),
        "visible_samples":value.visible_samples(),"instance":value.instance(),"gap_after":value.gap_after()})
}
fn owner_value(value: &AudioOwnerClock<'_>, plan: &RenderPlan) -> Value {
    let origin = match value.origin() {
        AudioOwnerClockOrigin::Current => json!({"kind":"Current"}),
        AudioOwnerClockOrigin::Retained {
            definition,
            instance,
            kind,
            gap_after,
        } => {
            json!({"kind":"Retained","definition":definition,"instance":instance,"owner_kind":format!("{kind:?}"),"gap_after":gap_after})
        }
    };
    json!({"belongs_to_expected_plan":value.belongs_to(plan),"definition":value.definition(),
        "instance":value.instance(),"kind":format!("{:?}",value.kind()),"gap_after":value.gap_after(),
        "sampling":value.sampling(),"origin":origin,"treatments":value.treatments(),
        "explicit_tail_provider":value.is_explicit_tail_provider(),"escalation_millidecibels":value.escalation_millidecibels()})
}
fn input_value(value: &deadpan_plan::AudioOwnerInput<'_>, plan: &RenderPlan) -> Value {
    json!({"observed_at":value.observed_at(),"work":value.work(),"scope":format!("{:?}",value.scope()),
        "owner":owner_value(value.owner(),plan),"context":domain(value.context(),plan)})
}
fn retained_value(
    value: &AudioOwnerRetainedInput<'_>,
    input: &deadpan_plan::AudioOwnerInput<'_>,
    plan: &RenderPlan,
) -> Value {
    let raw = match value.domain() {
        AudioBoundDomain::Empty => json!({"kind":"Empty"}),
        AudioBoundDomain::Root(raw) => json!({"kind":"Root","domain":domain(raw,plan)}),
        AudioBoundDomain::Point(raw) => {
            json!({"kind":"Point","belongs_to_expected_plan":raw.belongs_to(plan),
            "definition":raw.definition(),"root":raw.root(),"placement":raw.placement(),
            "reference_grid":raw.reference_grid(),"reference_sampling":raw.reference_sampling(),
            "reference_samples":raw.reference_samples()})
        }
    };
    json!({"belongs_to_expected_plan":value.belongs_to(plan),"retained_work":value.work(),
        "bound_work":value.bound().work(),"bound_is_silent_hold":value.bound().is_silent_hold(),
        "reference_at_observation":value.reference_at(input.observed_at()).map_err(|e|format!("{e:?}")),
        "reference_samples_per_output_sample":value.reference_samples_per_output_sample(),"raw_domain":raw,
        "private_plan_fields":"Full private processing/editorial/Bound snapshots are in the separately frozen Plan witness; no production serialization API is added here."})
}
struct ReadRequest {
    start: i64,
    frames: u32,
    authored: bool,
    fault: Fault,
}
fn read(
    renderer: &mut StageAudio,
    provider: &mut ObservedProvider<'_>,
    retained: &AudioOwnerRetainedInput<'_>,
    control: WorkControl<'_>,
    request: ReadRequest,
) -> Value {
    let ReadRequest {
        start,
        frames,
        authored,
        fault,
    } = request;
    let before_injection = work_snapshot(control.work);
    let mut injected_prior = None;
    let mut request_start = start;
    let mut request_frames = frames;
    let mut depth = 0;
    let mut read_control = control;
    match fault {
        Fault::InitialCancel => control.cancelled.store(true, Ordering::Relaxed),
        Fault::ExpiredDeadline => read_control.deadline = Instant::now() - Duration::from_secs(1),
        Fault::Resident => renderer.limits.maximum_resident_frames = 1,
        Fault::Depth => depth = renderer.limits.maximum_depth + 1,
        Fault::ZeroFrames => request_frames = 0,
        Fault::TooManyFrames => request_frames = MAX_OUTPUT_FRAMES + 1,
        Fault::Overflow => {
            request_start = i64::MAX;
            request_frames = 1;
        }
        Fault::Remaining(remaining) => {
            let prior = MAX_PLAN_WORK_PER_READ.checked_sub(remaining).unwrap();
            if prior < control.work.borrow().plan_work {
                return json!({"status":"NotRun","reason":"observed allowance would lower already accepted setup work",
                    "remaining":remaining,"before_injection":before_injection});
            }
            control.work.borrow_mut().plan_work = prior;
            injected_prior = Some(prior);
        }
        Fault::CacheMinimumAllowance => {
            let prior = MAX_PLAN_WORK_PER_READ - AudioQueryLimits::default().maximum_work;
            control.work.borrow_mut().cache_plan_work = prior;
            injected_prior = Some(prior);
        }
        _ => {}
    }
    let cache_scope = matches!(fault, Fault::CacheLane | Fault::CacheMinimumAllowance)
        .then(|| control.cache_key_scope());
    let before = work_snapshot(control.work);
    let controller_before = controller_snapshot(renderer);
    let before_calls = provider.calls;
    pairs::before_read(retained, request_start, request_frames, authored, fault);
    let result = renderer.read_retained_controlled(
        provider,
        retained,
        AudioSample(request_start),
        request_frames,
        read_control,
        (depth, authored),
    );
    let output = match &result {
        Ok(block) => json!({"status":"Ok","block":read_block(block)}),
        Err(value) => json!({"status":"Error","error":error(value)}),
    };
    drop(result);
    let after_return_drop = work_snapshot(control.work);
    let controller_after_return_drop = controller_snapshot(renderer);
    drop(cache_scope);
    json!({"request":{"start":request_start,"frames":request_frames,"depth":depth,"authored":authored,
            "fault":fault.name(),"timeout_seconds":TIMEOUT.as_secs()},
        "injected_prior_work_note":"Only explicit boundary scenarios raise the existing counter; no accepted work is refunded.",
        "injected_prior_work":injected_prior,"before_injection":before_injection,
        "before":before,"controller_before":controller_before,"output":output,
        "after_return_drop":after_return_drop,"after_scope_drop":work_snapshot(control.work),
        "controller_after_return_drop":controller_after_return_drop,
        "provider_calls_delta":provider.calls-before_calls})
}
fn scenario(
    fixture: &cases::Fixture,
    mode: Mode,
    authored: bool,
    start: i64,
    frames: u32,
    fault: Fault,
) -> Value {
    let plan = Arc::new(RenderPlan::compile(&fixture.current).unwrap());
    let owner = owner_at(&plan, fixture.owner, AudioSample(fixture.observed_at));
    let input = plan
        .audio_owner_input(&owner, AudioSample(fixture.observed_at), Default::default())
        .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut provider = ObservedProvider {
        prepared: cases::prepared(fixture.wav),
        calls: 0,
        events: Vec::new(),
        work: &work,
        fault,
    };
    let before_setup = work_snapshot(&work);
    let input_snapshot = input_value(&input, &plan);
    let mut retained_snapshot = Value::Null;
    let setup;
    let read = match mode {
        Mode::OrdinaryProbe => {
            match input.retained_probe_at(AudioSample(fixture.observed_at), Default::default()) {
                Ok(probe) => {
                    setup = json!({"status":if probe.input().is_some(){"Some"}else{"None"},"probe_work":probe.work(),"was_forwarded":false,
                        "source":"ordinary public retained probe, constructed without ReadWork admission"});
                    probe.input().map(|retained| {
                        retained_snapshot = retained_value(retained, &input, &plan);
                        read(
                            &mut renderer,
                            &mut provider,
                            retained,
                            control,
                            ReadRequest {
                                start,
                                frames,
                                authored,
                                fault,
                            },
                        )
                    })
                }
                Err(e) => {
                    setup = json!({"status":"Error","error":format!("{e:?}")});
                    None
                }
            }
        }
        Mode::AdmittedFalseProbe => {
            match input.retained_probe_at_admitted(
                AudioSample(fixture.observed_at),
                Default::default(),
                &mut sound_context_admission::Unmetered,
            ) {
                Ok(probe) => {
                    setup = json!({"status":if probe.input().is_some(){"Some"}else{"None"},"probe_work":probe.work(),
                        "canonical":probe.canonical_units(),"additional":probe.additional_units(),"was_forwarded":probe.was_forwarded()});
                    probe.input().map(|retained| {
                        retained_snapshot = retained_value(retained, &input, &plan);
                        read(
                            &mut renderer,
                            &mut provider,
                            retained,
                            control,
                            ReadRequest {
                                start,
                                frames,
                                authored,
                                fault,
                            },
                        )
                    })
                }
                Err(e) => {
                    setup = json!({"status":"Error","error":error(&e)});
                    None
                }
            }
        }
        Mode::AdmittedTrueProbe => {
            let result = {
                let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
                input.retained_probe_at_admitted(
                    AudioSample(fixture.observed_at),
                    Default::default(),
                    &mut admission,
                )
            };
            match result {
                Ok(probe) => {
                    setup = json!({"status":if probe.input().is_some(){"Some"}else{"None"},"probe_work":probe.work(),
                        "canonical":probe.canonical_units(),"additional":probe.additional_units(),"was_forwarded":probe.was_forwarded()});
                    probe.input().map(|retained| {
                        retained_snapshot = retained_value(retained, &input, &plan);
                        read(
                            &mut renderer,
                            &mut provider,
                            retained,
                            control,
                            ReadRequest {
                                start,
                                frames,
                                authored,
                                fault,
                            },
                        )
                    })
                }
                Err(e) => {
                    setup = json!({"status":"Error","error":error(&e)});
                    None
                }
            }
        }
        Mode::ActualSelectedShared => {
            // The production sharing helper's precondition is a paid input capture.
            let result = control.spend_plan_work(input.work()).and_then(|()| {
                let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
                authored_eq::Context::finish_paid_owner_input_admitted(
                    &input,
                    &owner,
                    AudioSample(fixture.observed_at),
                    control,
                    &mut admission,
                )
            });
            match result {
                Ok(context) => {
                    setup = json!({"status":if context.retained().is_some(){"Some"}else{"DomainFallback"},
                        "actual_helper":"Context::finish_paid_owner_input_admitted","paid_input_work":input.work(),
                        "same_request_issuer":context.issuer()==sound_context_admission::issuer(control)});
                    context.retained().map(|retained| {
                        retained_snapshot = retained_value(retained, &input, &plan);
                        read(
                            &mut renderer,
                            &mut provider,
                            retained,
                            control,
                            ReadRequest {
                                start,
                                frames,
                                authored,
                                fault,
                            },
                        )
                    })
                }
                Err(e) => {
                    setup = json!({"status":"Error","error":error(&e)});
                    None
                }
            }
        }
    };
    let after_handle_drop = work_snapshot(&work);
    assert_eq!(
        work.borrow().sound_context_frames,
        0,
        "actual setup/reader guards must drop"
    );
    let controller_after_handle_drop = controller_snapshot(&renderer);
    drop(renderer);
    let after_renderer_drop = work_snapshot(&work);
    json!({"mode":mode.name(),"authored":authored,"input":input_snapshot,"retained":retained_snapshot,
        "before_setup":before_setup,"setup":setup,"read":read,"provider_events":provider.events,
        "provider_calls":provider.calls,"after_handle_drop":after_handle_drop,
        "controller_after_handle_drop":controller_after_handle_drop,"after_renderer_drop":after_renderer_drop,
        "cancelled_at_end":cancelled.load(Ordering::Relaxed)})
}
fn matrix() -> Value {
    let mut cases = Vec::new();
    for fixture in cases::fixtures() {
        let mut operations = Vec::new();
        for mode in [
            Mode::OrdinaryProbe,
            Mode::AdmittedFalseProbe,
            Mode::AdmittedTrueProbe,
            Mode::ActualSelectedShared,
        ] {
            for authored in [false, true] {
                let mut first_units = None;
                for &(start, frames) in &fixture.windows {
                    let value = scenario(&fixture, mode, authored, start, frames, Fault::None);
                    if first_units.is_none() && value["read"]["output"]["status"] == "Ok" {
                        let before = value["read"]["before"]["plan_work"].as_u64().unwrap();
                        let after = value["read"]["after_return_drop"]["plan_work"]
                            .as_u64()
                            .unwrap();
                        first_units =
                            Some((start, frames, usize::try_from(after - before).unwrap()));
                    }
                    operations.push(value);
                }
                let (start, frames) = fixture.windows[0];
                for fault in [
                    Fault::InitialCancel,
                    Fault::ExpiredDeadline,
                    Fault::ProviderFailure,
                    Fault::ProviderCancel,
                    Fault::Depth,
                    Fault::ZeroFrames,
                    Fault::TooManyFrames,
                    Fault::Overflow,
                    Fault::Resident,
                    Fault::CacheLane,
                    Fault::CacheMinimumAllowance,
                ] {
                    operations.push(scenario(&fixture, mode, authored, start, frames, fault));
                }
                if let Some((start, frames, units)) = first_units {
                    let mut exact = scenario(
                        &fixture,
                        mode,
                        authored,
                        start,
                        frames,
                        Fault::Remaining(units),
                    );
                    exact["boundary_derivation"] = json!({"observed_default_start":start,"observed_default_frames":frames,"observed_default_read_units":units,"offset":0});
                    operations.push(exact);
                    if units > 0 {
                        let mut short = scenario(
                            &fixture,
                            mode,
                            authored,
                            start,
                            frames,
                            Fault::Remaining(units - 1),
                        );
                        short["boundary_derivation"] = json!({"observed_default_start":start,"observed_default_frames":frames,"observed_default_read_units":units,"offset":-1});
                        operations.push(short);
                    }
                }
            }
        }
        cases.push(json!({"name":fixture.name,"checked_source":fixture.checked_source,"reference_document":fixture.reference,
            "current_document":fixture.current,"owner":fixture.owner,"observed_at":fixture.observed_at,
            "windows":fixture.windows,"wav":cases::wav_descriptor(fixture.wav),"operations":operations}));
    }
    json!({"schema":"old-retained-host-read-baseline-v1","producer":"actual-read_retained_controlled-before-retained-fade-admission",
        "resource_scope":"ReadWork snapshots are exhaustive fields. Observer snapshots and setup queries are outside the measured read delta. Ordinary fade allocations remain unmetered.",
        "provenance_scope":"Ordinary, actual admitted false/true probe and actual selected sharing helper are distinct. Metered setup uses the SAME ReadWork and keeps its original carrier live through the read.",
        "identity_encoding":"Private projection keys use complete Debug payload and within-snapshot equality matrix, never raw pointer addresses. No cross-request opaque identity claim.",
        "private_plan_provenance":"Full documents/public input/raw-domain/bound work are retained here; exact private retained values are covered by the separate immutable Plan witness.",
        "cases":cases})
}
fn body(mut value: Value) -> Value {
    for name in ["variant", "source_sha256", "published_fade_checkpoint"] {
        value.as_object_mut().unwrap().remove(name);
    }
    value
}
#[test]
#[ignore = "manual OLD native producer; parent pins source/binary and executes serially before host changes"]
fn capture_old_retained_host_read_baseline_v1() {
    let target = std::env::var("DEADPAN_RETAINED_HOST_OUTPUT").expect("explicit new output path");
    let path = Path::new(&target);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_RETAINED_HOST_VARIANT").expect("explicit variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_RETAINED_HOST_SOURCE_SHA256")
        .expect("actual source manifest identity");
    assert_eq!(source.len(), 64);
    assert!(source.bytes().all(|v| v.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_RETAINED_HOST_PUBLISHED_CHECKPOINT")
        .expect("published fade checkpoint");
    assert!(!checkpoint.trim().is_empty());
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut value = retained_request::with_legacy_retained_queries(matrix);
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    value["published_fade_checkpoint"] = json!(checkpoint);
    serde_json::to_writer_pretty(&mut output, &value).unwrap();
    output.write_all(b"\n").unwrap();
    output.sync_all().unwrap();
}
#[test]
#[ignore = "external OLD complete native regression scaffold; requires independent captured body"]
fn retained_host_reads_match_external_old_v1() {
    let path =
        std::env::var_os("DEADPAN_RETAINED_HOST_EXPECTED").expect("independent OLD capture path");
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let actual = retained_request::with_legacy_retained_queries(matrix);
    assert_eq!(actual, body(expected));
}

#[test]
fn retained_host_reads_match_frozen_old_v1() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-host-read-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(
        retained_request::with_legacy_retained_queries(matrix),
        expected
    );
}

#[path = "retained_host_admission.rs"]
mod admission_tests;

#[path = "retained_host_pairs.rs"]
mod pairs;
