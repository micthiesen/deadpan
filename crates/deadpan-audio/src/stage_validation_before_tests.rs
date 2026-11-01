//! Before41: actual stage output validation, with observation storage outside accounting.
use super::*;
use crate::persistent_pcm::PcmCache;
use crate::stages::stage_validation_policy_before_trace as validation;

const CHECKPOINT: &str = "b6b2d19b034ff1b21dc85bb6f33c1e5d1184bd84";
thread_local! {
    static FOCUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
struct Focus;
impl Drop for Focus {
    fn drop(&mut self) {
        FOCUSED.with(|v| v.set(false));
    }
}
fn focus() -> Focus {
    FOCUSED.with(|v| assert!(!v.replace(true), "before41 fixture already selected"));
    Focus
}

struct Capture {
    _guard: validation::CaptureGuard,
}
impl Drop for Capture {
    fn drop(&mut self) {
        point_suppression::ops::trace::disable();
        point_suppression::work_trace::disable();
    }
}
fn begin_trace() -> Capture {
    point_suppression::ops::trace::reset();
    point_suppression::work_trace::reset();
    Capture {
        _guard: validation::begin(),
    }
}
fn end_trace(capture: Capture) -> Value {
    let mut value = validation::take();
    value["mask_operations"] =
        serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
    value["public_owner_work"] =
        serde_json::to_value(point_suppression::work_trace::snapshot()).unwrap();
    drop(capture);
    assert_eq!(value["dropped"], 0);
    assert_eq!(value["invalid"], false);
    assert_eq!(value["active_at_take"], json!([]));
    value
}
pub(super) fn provider_position() -> Option<Value> {
    after::provider_entry_sidehook();
    validation::position()
}

fn activity(value: crate::PreparationActivity) -> Value {
    use crate::PreparationActivity::*;
    match value {
        CheckingPlan => json!({"kind":"CheckingPlan"}),
        LoadingSource => json!({"kind":"LoadingSource"}),
        DecodingSource => json!({"kind":"DecodingSource"}),
        PreparingOutput => json!({"kind":"PreparingOutput"}),
        CheckingSavedAudio { completed, total } => {
            json!({"kind":"CheckingSavedAudio","completed":completed,"total":total})
        }
        RestoringSavedAudio { completed, total } => {
            json!({"kind":"RestoringSavedAudio","completed":completed,"total":total})
        }
        SavingAudioCache => json!({"kind":"SavingAudioCache"}),
        CheckingStretch {
            depth,
            completed,
            total,
        } => json!({"kind":"CheckingStretch","depth":depth,"completed":completed,"total":total}),
        Stretching {
            depth,
            completed,
            total,
        } => json!({"kind":"Stretching","depth":depth,"completed":completed,"total":total}),
        CheckingStretchInput {
            depth,
            completed,
            total,
        } => {
            json!({"kind":"CheckingStretchInput","depth":depth,"completed":completed,"total":total})
        }
        SavingStretch { depth } => json!({"kind":"SavingStretch","depth":depth}),
    }
}
fn monitor(value: &crate::PreparationMonitor) -> Value {
    json!({"activity":activity(value.activity()),
        "saved_audio":value.saved_audio_status().map(|v| json!({
            "key":v.key,"bytes":v.bytes,"event":saved_event(v.event),
            "regenerated_from":v.regenerated_from.map(saved_event)})),
        "scope":"actual latest monitor state, not a history"})
}
fn saved_event(value: crate::SavedAudioEvent) -> &'static str {
    use crate::SavedAudioEvent::*;
    match value {
        Missing => "Missing",
        Restored => "Restored",
        Saved => "Saved",
        Unavailable => "Unavailable",
        Damaged => "Damaged",
        Busy => "Busy",
        Full => "Full",
        BudgetSkipped => "BudgetSkipped",
    }
}
fn entries(cache: &PcmCache) -> Value {
    match cache.entries() {
        Ok(mut rows) => {
            rows.sort_by(|a, b| {
                (&a.key, a.staging, a.origin, a.bytes, a.validity)
                    .cmp(&(&b.key, b.staging, b.origin, b.bytes, b.validity))
            });
            json!({"status":"Ok","entry_multiset_by_key":rows.iter().map(|v|json!({
                "key":v.key,"bytes":v.bytes,"validity":v.validity,"origin":v.origin,
                "staging":v.staging})).collect::<Vec<_>>(),
                "sampling":"keyed listing multiset sampled here, duplicates retained; not rendering/eviction order",
                "unsampled":"API age-dependent enumeration order and volatile unused_seconds age are outside this listing sample"})
        }
        Err(e) => json!({"status":"Error","kind":format!("{:?}",e.kind()),"display":e.to_string()}),
    }
}
fn stage_error(value: &StageAudioError) -> Value {
    let mut result = error(value);
    result["typed_payload"] = match value {
        StageAudioError::Range
        | StageAudioError::PreparationRange
        | StageAudioError::ForeignDomain
        | StageAudioError::ForeignDefinition
        | StageAudioError::InvalidLimits
        | StageAudioError::Timeout => json!({"shape":"unit"}),
        StageAudioError::Limit(reason) | StageAudioError::Unsupported(reason) => {
            json!({"reason":reason})
        }
        StageAudioError::Preparation(value) => match value {
            PreparationError::InvalidRecipe(reason) => {
                json!({"variant":"InvalidRecipe","reason":reason})
            }
            PreparationError::UnsupportedLayout => {
                json!({"variant":"UnsupportedLayout","shape":"unit"})
            }
            PreparationError::InvalidSamples => json!({"variant":"InvalidSamples","shape":"unit"}),
            PreparationError::Cancelled => json!({"variant":"Cancelled","shape":"unit"}),
            PreparationError::IndexMismatch => json!({"variant":"IndexMismatch","shape":"unit"}),
            PreparationError::SourceUnavailable(source) => {
                json!({"variant":"SourceUnavailable","source":source})
            }
            PreparationError::Time(_) => json!({"variant":"Time","nested_fields_unexposed":true}),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            PreparationError::Session(_) => {
                json!({"variant":"Session","nested_fields_unexposed":true})
            }
        },
        StageAudioError::Cache(value) => json!({"error_kind":format!("{:?}",value.kind()),
            "raw_os_error":value.raw_os_error(),"display":value.to_string()}),
        StageAudioError::Plan(_)
        | StageAudioError::Time(_)
        | StageAudioError::Gain(_)
        | StageAudioError::Dsp(_)
        | StageAudioError::Filter(_)
        | StageAudioError::Wet(_)
        | StageAudioError::Transfer(_) => json!({"nested_fields_unexposed":true}),
    };
    result["error_schema_scope"] = json!(
        "exact outer kind and selected direct payloads; original Debug/display supplemental, no complete recursive typed claim"
    );
    result
}
fn definition_value(value: &deadpan_plan::AudioDefinition<'_>, plan: &RenderPlan) -> Value {
    json!({"selector":value.selector(),"root":value.root(),"duration":value.duration(),
    "exact_duration":value.exact_duration(),"belongs_to_plan":value.belongs_to(plan),
    "signal_sample_count":match value.signal().sample_count() {
        Ok(v)=>json!({"status":"Ok","samples":v.0}),
        Err(e)=>json!({"status":"Error","error":stage_error(&StageAudioError::Plan(e))}),
    }})
}
fn new_renderer(
    plan: &Arc<RenderPlan>,
    limits: StageLimits,
    cache: Option<PcmCache>,
) -> (StageAudio, Arc<crate::PreparationMonitor>) {
    let mut renderer = StageAudio::with_limits(Arc::clone(plan), limits).unwrap();
    let monitor = Arc::new(crate::PreparationMonitor::new(
        crate::PreparationActivity::PreparingOutput,
    ));
    renderer.set_preparation_monitor(Some(Arc::clone(&monitor)));
    renderer.set_persistent_cache(cache);
    (renderer, monitor)
}
fn provider(
    inner: NativeProvider,
    budget: Option<&RefCell<ReadWork>>,
    fail: bool,
) -> WitnessProvider<'_> {
    WitnessProvider {
        inner,
        budget,
        events: Vec::new(),
        fail,
    }
}
fn provider_value(value: &WitnessProvider<'_>, events_before: usize, calls_before: usize) -> Value {
    json!({"events_before":events_before,"events_after":value.events.len(),
        "source_calls_before":calls_before,"source_calls_after":value.inner.calls,
        "events":&value.events,"scope":"cumulative counters; bounds identify this action; callback entry is not decoder I/O"})
}

#[derive(Clone, Copy)]
enum Target<'a, 'p> {
    Projection(&'a Arc<AudioStageProjection<'p>>),
    Definition(&'a deadpan_plan::AudioDefinition<'p>),
}
enum Returned {
    Routed(RoutedSignalBlock),
    Definition(crate::DefinitionAudioBlock),
}
fn returned(value: &Result<Returned, StageAudioError>) -> Value {
    match value {
        Ok(Returned::Routed(v)) => {
            let crate::RoutedSignalBlock {
                schema_version,
                stage,
                project_id,
                revision_id,
                allocation,
                start,
                samples,
                suppressed,
            } = v;
            json!({"status":"Ok","kind":"Routed","schema_version":schema_version,"stage":stage,
                "project_id":project_id,"revision_id":revision_id,
                "allocation":[allocation.start.0,allocation.end.0],"start":start.0,
                "f32_bits":samples_as_bits(samples),"samples_len":samples.len(),
                "samples_storage":layout::<[f32;2]>(samples.capacity()),"suppressed":mask(suppressed)})
        }
        Ok(Returned::Definition(v)) => {
            let crate::DefinitionAudioBlock {
                schema_version,
                stage,
                project_id,
                revision_id,
                definition,
                root,
                start,
                samples,
                suppressed,
            } = v;
            json!({"status":"Ok","kind":"Definition","schema_version":schema_version,"stage":stage,
                "project_id":project_id,"revision_id":revision_id,"definition":definition,
                "root":root,"start":start.0,"f32_bits":samples_as_bits(samples),
                "samples_len":samples.len(),"samples_storage":layout::<[f32;2]>(samples.capacity()),
                "suppressed":mask(suppressed)})
        }
        Err(e) => json!({"status":"Error","error":stage_error(e)}),
    }
}
fn public_once(
    renderer: &mut StageAudio,
    source: &mut WitnessProvider<'_>,
    target: Target<'_, '_>,
    (start, frames): (i64, u32),
    name: &str,
    monitor: &crate::PreparationMonitor,
    cancelled: &AtomicBool,
) -> Value {
    let before = controller(renderer);
    let cancelled_at_entry = cancelled.load(Ordering::Relaxed);
    let monitor_before = crate_monitor(monitor);
    let events_before = source.events.len();
    let calls_before = source.inner.calls;
    let capture = begin_trace();
    let native_capture = after::begin_action(name, "public");
    let result = match target {
        Target::Definition(definition) => renderer
            .read_definition(
                source,
                definition,
                SignalSample(start),
                frames,
                TIMEOUT,
                cancelled,
            )
            .map(Returned::Definition),
        Target::Projection(projection) => (|| {
            let route = SoundRoute::identity(ratio(i128::from(projection.output_frames()), 1))
                .map_err(deadpan_plan::PlanError::from)?;
            let grid = AudioSampleGrid::new(
                ExactRatio::ZERO,
                ExactRatio::ONE,
                AudioBoundaryRule::PointCeil,
            )?;
            let count = route.nodes().len();
            let route = AudioSoundRoute::<SignalSample>::new(route, vec![grid; count])?;
            let domain = AudioRoutedSignal::projected(Arc::clone(projection), route)?;
            renderer
                .read_routed_signal(
                    source,
                    &domain,
                    SignalSample(start),
                    frames,
                    TIMEOUT,
                    cancelled,
                )
                .map(Returned::Routed)
        })(),
    };
    after::end_action(native_capture);
    let trace = end_trace(capture);
    let value = returned(&result);
    let live = controller(renderer);
    drop(result);
    json!({"action":name,"entry":"actual public call, fresh internal ReadWork",
        "start":start,"frames":frames,"cancelled_at_entry":cancelled_at_entry,
        "before":before,"monitor_before":monitor_before,"result":value,"trace":trace,
        "controller_result_live":live,"controller_after_result_drop":controller(renderer),
        "monitor_after":crate_monitor(monitor),"provider":provider_value(source,events_before,calls_before),
        "result_lifetime":"serialized while live, then dropped before next action",
        "public_internal_work":"inaccessible after return; observer snapshots only, no address dereference"})
}
fn crate_monitor(value: &crate::PreparationMonitor) -> Value {
    monitor(value)
}

enum Prepared {
    Projected(Arc<PreparedBlock>),
    Stage(Arc<PreparedStage>),
}
impl Prepared {
    fn value(&self) -> Value {
        match self {
            Self::Projected(v) => json!({"kind":"Projected","block":prepared(v)}),
            Self::Stage(v) => json!({"kind":"Stage","project_id":v.plan.metadata().project_id,
                "revision_id":v.plan.metadata().revision_id,
                "key":match &v.key {
                    PreparedKey::Preserve(s)=>json!({"kind":"Preserve","stage":s}),
                    PreparedKey::Hold {definition,instance,gap_after,content}=>
                        json!({"kind":"Hold","definition":definition,"instance":instance,
                            "gap_after":gap_after,"content":content})},
                "block":prepared(&v.block)}),
        }
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Projected(a), Self::Projected(b)) => Arc::ptr_eq(a, b),
            (Self::Stage(a), Self::Stage(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
#[derive(Clone, Copy)]
enum PrepareTarget<'a, 'p> {
    Projected(&'a AudioStageProjection<'p>),
    Stage(&'a deadpan_plan::AudioStage<'p>),
}
fn private_once(
    renderer: &mut StageAudio,
    source: &mut WitnessProvider<'_>,
    target: PrepareTarget<'_, '_>,
    control: WorkControl<'_>,
    monitor: &crate::PreparationMonitor,
    name: &str,
) -> (Result<Prepared, StageAudioError>, Value) {
    let before = controller(renderer);
    let work_before = work(control.work);
    let monitor_before = crate_monitor(monitor);
    let events = source.events.len();
    let calls = source.inner.calls;
    let capture = begin_trace();
    let native_capture = after::begin_action(name, "private");
    let result = match target {
        PrepareTarget::Projected(p) => renderer
            .prepare_projected(p, source, control, 1)
            .map(Prepared::Projected),
        PrepareTarget::Stage(s) => renderer
            .prepare_stage(s, source, control, 1)
            .map(Prepared::Stage),
    };
    after::end_action(native_capture);
    let trace = end_trace(capture);
    let result_value = match &result {
        Ok(v) => json!({"status":"Ok","prepared":v.value()}),
        Err(e) => json!({"status":"Error","error":stage_error(e)}),
    };
    let row = json!({"action":name,"entry":"actual private prepare with actual default-initialized ReadWork",
        "depth":1,"before":before,"work_before":work_before,"monitor_before":monitor_before,
        "result":result_value,"trace":trace,"work_after":work(control.work),
        "controller_result_live":controller(renderer),"monitor_after":crate_monitor(monitor),
        "provider":provider_value(source,events,calls)});
    (result, row)
}
#[derive(Default)]
struct PrivateOptions {
    cancelled_at_entry: bool,
    expired: bool,
    fail: bool,
    repeat: bool,
}
fn private_action(
    plan: &Arc<RenderPlan>,
    inner: NativeProvider,
    target: PrepareTarget<'_, '_>,
    name: &str,
    limits: StageLimits,
    options: PrivateOptions,
) -> Value {
    let _family = after::family_context(name);
    let PrivateOptions {
        cancelled_at_entry,
        expired,
        fail,
        repeat,
    } = options;
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(cancelled_at_entry);
    let mut source = provider(inner, Some(&budget), fail);
    let (mut renderer, monitor) = new_renderer(plan, limits, None);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: if expired {
            Instant::now() - Duration::from_secs(1)
        } else {
            Instant::now() + TIMEOUT
        },
    };
    let (first, first_row) =
        private_once(&mut renderer, &mut source, target, control, &monitor, name);
    let second = if repeat && first.is_ok() {
        Some(private_once(
            &mut renderer,
            &mut source,
            target,
            control,
            &monitor,
            "same_work_second",
        ))
    } else {
        None
    };
    let same = match (&first, &second) {
        (Ok(a), Some((Ok(b), _))) => Some(a.same(b)),
        _ => None,
    };
    let second_row = second.as_ref().map(|(_, v)| v);
    let work_final = work(&budget);
    let result = json!({"name":name,"first":first_row,"second":second_row,"same_actual_arc":same,
        "cancelled_at_entry":cancelled_at_entry,"expired_at_entry":expired,
        "fail_provider":fail,"work_final":work_final});
    let after_calls = controller(&renderer);
    let observer = renderer.owned_suppression.observer();
    drop(source);
    drop(budget);
    let held_after_work = first.as_ref().ok().map(Prepared::value);
    drop(renderer);
    let held_after_controller = first.as_ref().ok().map(Prepared::value);
    let account_controller_gone = observer.as_ref().map(|v| v.live_frames());
    drop(second);
    drop(first);
    json!({"calls":result,"controller_after_calls":after_calls,
        "prepared_after_ReadWork_drop":held_after_work,"prepared_after_controller_drop":held_after_controller,
        "account_after_controller_drop":account_controller_gone,
        "account_after_actual_owners_drop":observer.as_ref().map(|v|v.live_frames())})
}
fn public_pair(
    plan: &Arc<RenderPlan>,
    target: Target<'_, '_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    name: &str,
) -> SetupResult<Value> {
    let _family = after::family_context(name);
    let (mut renderer, monitor) = new_renderer(plan, StageLimits::default(), None);
    let mut source = provider(reopen()?, None, false);
    let cancel = AtomicBool::new(false);
    let cold = public_once(
        &mut renderer,
        &mut source,
        target,
        (0, 256),
        "cold",
        &monitor,
        &cancel,
    );
    let warm = public_once(
        &mut renderer,
        &mut source,
        target,
        (0, 256),
        "warm_fresh_ReadWork",
        &monitor,
        &cancel,
    );
    let after = controller(&renderer);
    drop(source);
    drop(renderer);
    Ok(json!({"family":name,"cold":cold,"warm":warm,"controller_before_drop":after}))
}

pub(super) fn fixture_visit(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    asset: &deadpan_core::AssetRecord,
    wire: &Value,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    provenance: impl FnOnce() -> SetupResult<Value>,
) -> SetupResult<Option<Value>> {
    if !FOCUSED.with(std::cell::Cell::get) {
        return Ok(None);
    }
    let provenance = provenance()?;
    if let Some(focused) = after::fixture_visit(plan, base, asset, wire, reopen, &provenance)? {
        return Ok(Some(focused));
    }
    let before = trace::projection_value(base, plan);
    let structural = public_pair(
        plan,
        Target::Projection(base),
        reopen,
        "registered_structural_projected",
    )?;
    let source_projection = qualified_projection(plan, base, false, false);
    let qualified = match source_projection {
        Ok(p) => {
            let _family = after::family_context("qualified_source_input");
            let before = trace::projection_value(&p, plan);
            let (mut renderer, monitor) = new_renderer(plan, StageLimits::default(), None);
            let mut source = provider(reopen()?, None, false);
            let cold = public_once(
                &mut renderer,
                &mut source,
                Target::Projection(&p),
                (0, 256),
                "qualified_source_input_cold",
                &monitor,
                &AtomicBool::new(false),
            );
            let after = trace::projection_value(&p, plan);
            assert_eq!(before, after);
            json!({"setup":"Ok","projection_before":before,"projection_after":after,"cold":cold})
        }
        Err(e) => json!({"setup":"Error","error":stage_error(&StageAudioError::Plan(e))}),
    };
    let memo = private_action(
        plan,
        reopen()?,
        PrepareTarget::Projected(base),
        "projected_first_memo",
        StageLimits::default(),
        PrivateOptions {
            repeat: true,
            ..Default::default()
        },
    );
    let mut controls = vec![
        private_action(
            plan,
            reopen()?,
            PrepareTarget::Projected(base),
            "cancel_at_entry",
            StageLimits::default(),
            PrivateOptions {
                cancelled_at_entry: true,
                ..Default::default()
            },
        ),
        private_action(
            plan,
            reopen()?,
            PrepareTarget::Projected(base),
            "expired_at_entry",
            StageLimits::default(),
            PrivateOptions {
                expired: true,
                ..Default::default()
            },
        ),
        private_action(
            plan,
            reopen()?,
            PrepareTarget::Projected(base),
            "real_provider_refusal",
            StageLimits::default(),
            PrivateOptions {
                fail: true,
                ..Default::default()
            },
        ),
        private_action(
            plan,
            reopen()?,
            PrepareTarget::Projected(base),
            "input_limit639",
            StageLimits {
                maximum_input_frames: 639,
                ..Default::default()
            },
            PrivateOptions::default(),
        ),
    ];
    let plain = match plain_nested_plan(asset, wire) {
        Ok((plain_plan, plain_wire)) => {
            let input_before = serde_json::to_value(&plain_wire)?;
            let selector = deadpan_plan::AudioDefinitionSelector::Node { node: id("outer") };
            match plain_plan.audio_definition(selector) {
                Ok(definition) => {
                    let definition_before = definition_value(&definition, &plain_plan);
                    let public = public_pair(
                        &plain_plan,
                        Target::Definition(&definition),
                        reopen,
                        "plain_nonowned_nested_stage",
                    )?;
                    let extracted = extract_stage(definition.signal());
                    let private = match extracted {
                        Ok(stage) => {
                            controls.push(private_action(
                                &plain_plan,
                                reopen()?,
                                PrepareTarget::Stage(&stage),
                                "nested_depth1",
                                StageLimits {
                                    maximum_depth: 1,
                                    ..Default::default()
                                },
                                PrivateOptions::default(),
                            ));
                            private_action(
                                &plain_plan,
                                reopen()?,
                                PrepareTarget::Stage(&stage),
                                "plain_stage_first_cached",
                                StageLimits::default(),
                                PrivateOptions {
                                    repeat: true,
                                    ..Default::default()
                                },
                            )
                        }
                        Err(e) => {
                            json!({"setup":"Error","error":stage_error(&StageAudioError::Plan(e))})
                        }
                    };
                    let directory = tempfile::tempdir()?;
                    let cache = PcmCache::at(directory.path())?;
                    let cache_before = entries(&cache);
                    let (mut renderer, monitor) =
                        new_renderer(&plain_plan, StageLimits::default(), Some(cache.clone()));
                    let mut source = provider(reopen()?, None, false);
                    let cold = public_once(
                        &mut renderer,
                        &mut source,
                        Target::Definition(&definition),
                        (0, 256),
                        "persistent_empty_directory",
                        &monitor,
                        &AtomicBool::new(false),
                    );
                    drop(source);
                    drop(renderer);
                    let cache_after_cold = entries(&cache);
                    let (mut renderer, monitor) =
                        new_renderer(&plain_plan, StageLimits::default(), Some(cache.clone()));
                    let mut source = provider(reopen()?, None, false);
                    let restore = public_once(
                        &mut renderer,
                        &mut source,
                        Target::Definition(&definition),
                        (0, 256),
                        "persistent_fresh_renderer_same_directory",
                        &monitor,
                        &AtomicBool::new(false),
                    );
                    drop(source);
                    drop(renderer);
                    let definition_after = definition_value(&definition, &plain_plan);
                    assert_eq!(definition_before, definition_after);
                    json!({"setup":"Ok","document":plain_wire,
                        "authored_wire_before":input_before,"authored_wire_after":plain_wire,
                        "authored_wire_scope":"inert checked source document, separate from queried public handle",
                        "definition_before":definition_before,"definition_after":definition_after,
                        "public":public,"private":private,
                        "persistent":{"before":cache_before,"cold":cold,"after_cold":cache_after_cold,
                            "fresh_renderer":restore,"after_fresh_renderer":entries(&cache)},
                        "persistent_scope":"real set_persistent_cache; positive inherited Cache target/restore status must be observed"})
                }
                Err(e) => {
                    json!({"setup":"Error","document":plain_wire,"error":stage_error(&StageAudioError::Plan(e))})
                }
            }
        }
        Err(e) => setup_error("plain_nonowned_nested_stage", e.as_ref()),
    };
    let (mut renderer, monitor) = new_renderer(plan, StageLimits::default(), None);
    let mut source = provider(reopen()?, None, false);
    let warmup = public_once(
        &mut renderer,
        &mut source,
        Target::Projection(base),
        (0, 256),
        "default_warmup",
        &monitor,
        &AtomicBool::new(false),
    );
    renderer.limits.maximum_resident_frames = 2048;
    let quota = public_once(
        &mut renderer,
        &mut source,
        Target::Projection(base),
        (512, 256),
        "old_L2048_literal",
        &monitor,
        &AtomicBool::new(false),
    );
    let quota_controller = controller(&renderer);
    drop(source);
    drop(renderer);
    let after = trace::projection_value(base, plan);
    assert_eq!(before, after);
    Ok(Some(
        json!({"provenance":provenance,"projection_before":before,"projection_after":after,
        "structural":structural,"qualified_source_input":qualified,"projected_private":memo,
        "plain_nested":plain,"controls":controls,
        "old_L2048":{"warmup":warmup,"measured":quota,"controller_after":quota_controller,
            "claim":"literal legal resident cap before measured fresh public request; success/target reach not assumed"}}),
    ))
}
fn capture_body() -> Value {
    let _focus = focus();
    let body = match family() {
        Ok(v) => v,
        Err(e) => setup_error("registered_stage_validation", e.as_ref()),
    };
    json!({"schema":"stage-validation-policy-before-v1","actual":body,
        "scope":{"selected":"initial build_stage complete-output validation only",
            "ordinary_capacity":"actual outer suppressed/contents Vec capacities/Layout only; nested ID/SourceAudio heap capacities unexposed",
            "disposal":"lexical query owner ends before chunk guard exit; no allocator/free/lease claim",
            "observation_storage":"unadmitted test JSON outside ledger","later_producers":"read_signal/free suppress/physical root policy unchanged",
            "cache":"real persistent scans only; no manual cache guard or accepted-work seeds",
            "errors":"exact outer kinds and selected direct payloads; deep nested fields explicitly unexposed; Debug/display supplemental; local failed-constructor work unexposed"}})
}
#[test]
#[ignore = "manual actual BEFORE41; no generated expected fixture"]
fn capture_stage_validation_policy_before_v1() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
        capture_stage_validation_policy_before_v1_historical,
    );
}
fn capture_stage_validation_policy_before_v1_historical() {
    let output = std::env::var_os("DEADPAN_STAGE_VALIDATION_BEFORE_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_STAGE_VALIDATION_BEFORE_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_STAGE_VALIDATION_BEFORE_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_STAGE_VALIDATION_BEFORE_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT
    );
    let mut body = capture_body();
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
#[ignore = "external actual frozen body only"]
fn stage_validation_before_external_body_regression() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(
        stage_validation_before_external_body_regression_historical,
    );
}
fn stage_validation_before_external_body_regression_historical() {
    let path = std::env::var_os("DEADPAN_STAGE_VALIDATION_BEFORE_EXPECTED").unwrap();
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    key_replay::assert_relation(&expected, &capture_body());
}

#[test]
fn stage_validation_frozen_before_key_relation_is_exact() {
    crate::stages::stage_validation_policy_request::with_legacy_stage_validation_policy(|| {
        let bytes =
            include_bytes!("../tests/fixtures/stage-validation-policy-baseline-v1/corpus.json");
        use sha2::Digest;
        assert_eq!(
            sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "3f3d626f064bab86af1102e878d5cdd3f0c3efd55edbe4dc0e10673a9cd8cf9e"
        );
        let expected: Value = serde_json::from_slice(bytes).unwrap();
        key_replay::assert_relation(&expected, &capture_body());
    });
}
#[path = "stage_validation_policy_after_tests.rs"]
mod after;
#[path = "stage_validation_key_replay.rs"]
mod key_replay;
