//! Ordinary N read_signal producer witnesses; fixture observation storage is unadmitted.
use super::*;
use crate::persistent_pcm::PcmCache;
use crate::stages::read_signal_policy_before_trace as before_trace;
use deadpan_plan::{
    AudioDefinition, AudioDefinitionSelector, AudioRootPlacement, AudioSignal, AudioSignalTape,
    AudioSignalTapeRun, AudioSourceVoiceRecipe, AudioStage,
};

const CHECKPOINT_N: &str = "1423c8edd378902c4233a64209f4ec6c765e9026";
std::thread_local! { static FOCUSED_N: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

struct Focus(bool);
impl Drop for Focus {
    fn drop(&mut self) {
        FOCUSED_N.with(|value| value.set(self.0));
    }
}
fn focus() -> Focus {
    Focus(FOCUSED_N.with(|value| value.replace(true)))
}
pub(super) fn provider_position() -> Option<Value> {
    if after::active() {
        return after::provider_position();
    }
    before_trace::position()
}

struct MaskObservations;
impl Drop for MaskObservations {
    fn drop(&mut self) {
        point_suppression::ops::trace::disable();
        point_suppression::work_trace::disable();
    }
}
fn mask_observations() -> MaskObservations {
    point_suppression::ops::trace::reset();
    point_suppression::work_trace::reset();
    MaskObservations
}
enum ActionCapture {
    Ordinary(before_trace::CaptureGuard),
    After(after::Action),
}
fn begin_capture(name: &str, entry: &'static str) -> ActionCapture {
    match after::begin_action(name, entry) {
        Some(value) => ActionCapture::After(value),
        None => ActionCapture::Ordinary(before_trace::begin()),
    }
}
fn finish_capture(capture: ActionCapture) -> Value {
    let guard = match capture {
        ActionCapture::Ordinary(guard) => guard,
        ActionCapture::After(action) => return after::end_action(action),
    };
    let mut value = before_trace::take();
    drop(guard);
    assert_eq!(value["dropped"], 0);
    assert_eq!(value["invalid"], false);
    assert_eq!(value["active_at_take"], json!([]));
    assert_eq!(value["key_failed"], false);
    assert_eq!(value["active_key_streams_at_take"], json!([]));
    assert!(value["key_result_pending_at_take"].is_null());
    value["existing_mask_ops"] =
        serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
    value["existing_public_owner_work"] =
        serde_json::to_value(point_suppression::work_trace::snapshot()).unwrap();
    value["supplemental_counter_scope"] = json!(
        "actual existing operations; cumulative within action group, no N policy admission-local attribution"
    );
    value
}
fn capacity_layout<T>(capacity: usize) -> Value {
    let value = std::alloc::Layout::array::<T>(capacity).unwrap();
    json!({"capacity":capacity,"size":value.size(),"align":value.align(),
        "scope":"capacity-derived after construction; not an observed allocator request"})
}
fn mask_value(value: &SignalSuppressionMask) -> Value {
    json!({"values":value.as_slice(),"len":value.len(),"capacity":value.capacity(),
        "capacity_layout":capacity_layout::<Range<SignalSample>>(value.capacity()),
        "is_empty":value.is_empty(),"iter_size_hint":value.iter().size_hint(),
        "first":value.first(),"last":value.last(),
        "get_at_len":value.get(value.len()),
        "reverse":value.iter().rev().collect::<Vec<_>>()})
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
fn typed_error(value: &StageAudioError) -> Value {
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
        Err(e)=>json!({"status":"Error","error":typed_error(&StageAudioError::Plan(e))}),
    }})
}

fn renderer(
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
fn provider_value(
    source: &WitnessProvider<'_>,
    events_before: usize,
    calls_before: usize,
) -> Value {
    json!({"events_before":events_before,"events_after":source.events.len(),
        "source_calls_before":calls_before,"source_calls_after":source.inner.calls,
        "events":&source.events,
        "scope":"actual cumulative counters; bounds identify this action; callback entry is not decoder I/O"})
}

#[derive(Clone, Copy)]
enum PublicTarget<'a, 'p> {
    Definition(&'a AudioDefinition<'p>),
    Tape(&'a AudioSignalTape<'p>),
    Routed(&'a AudioRoutedSignal<'p>),
}
enum PublicOutput {
    Definition(crate::DefinitionAudioBlock),
    Tape(crate::TapeAudioBlock),
    Routed(crate::RoutedSignalBlock),
}
fn tape_value(value: &AudioSignalTape<'_>, plan: &RenderPlan) -> Value {
    json!({"support":value.support(),"grid":value.grid(),
        "belongs_to_plan":value.belongs_to(plan),
        "sample_count":match value.sample_count() {
            Ok(v)=>json!({"status":"Ok","samples":v.0}),
            Err(e)=>json!({"status":"Error","error":typed_error(&StageAudioError::Plan(e))})
        },
        "private_run_inventory_unexposed":true})
}
fn input_value(target: PublicTarget<'_, '_>, plan: &RenderPlan) -> Value {
    match target {
        PublicTarget::Definition(v) => {
            json!({"kind":"Definition","definition":definition_value(v,plan),
            "signal_support":v.signal().support()})
        }
        PublicTarget::Tape(v) => json!({"kind":"Tape","tape":tape_value(v,plan)}),
        PublicTarget::Routed(v) => json!({"kind":"Routed","belongs_to_plan":v.belongs_to(plan),
        "samples":v.samples(),"route":v.route().route(),
        "input":match v.input() {
            deadpan_plan::AudioRoutedSignalInput::Source(voice)=>json!({
                "kind":"Source","source":voice.source(),
                "input_support":voice.input_signal().support(),
                "input_sample_count":voice.input_signal().sample_count().map(|v|v.0).map_err(|e|e.to_string()),
                "scope":"actual checked Source voice, private carrier fields unexposed"
            }),
            deadpan_plan::AudioRoutedSignalInput::Projected(p)=>json!({
                "kind":"Projected","projection":trace::projection_value(p,plan)
            })
        }}),
    }
}
fn output_value(value: &Result<PublicOutput, StageAudioError>) -> Value {
    match value {
        Ok(PublicOutput::Definition(v)) => json!({"status":"Ok","kind":"Definition",
            "public":v,"f32_bits":samples_as_bits(&v.samples),"samples_len":v.samples.len(),
            "sample_capacity_layout":capacity_layout::<[f32;2]>(v.samples.capacity()),
            "returned_mask":mask_value(&v.suppressed)}),
        Ok(PublicOutput::Tape(v)) => json!({"status":"Ok","kind":"Tape",
            "public":v,"f32_bits":samples_as_bits(&v.samples),"samples_len":v.samples.len(),
            "sample_capacity_layout":capacity_layout::<[f32;2]>(v.samples.capacity()),
            "returned_mask":mask_value(&v.suppressed)}),
        Ok(PublicOutput::Routed(v)) => json!({"status":"Ok","kind":"Routed",
            "public":v,"f32_bits":samples_as_bits(&v.samples),"samples_len":v.samples.len(),
            "sample_capacity_layout":capacity_layout::<[f32;2]>(v.samples.capacity()),
            "returned_mask":mask_value(&v.suppressed)}),
        Err(e) => json!({"status":"Error","error":typed_error(e)}),
    }
}
fn call_public(
    renderer: &mut StageAudio,
    source: &mut WitnessProvider<'_>,
    target: PublicTarget<'_, '_>,
    request: (i64, u32),
    name: &str,
    monitor: &crate::PreparationMonitor,
    cancelled: &AtomicBool,
) -> (Result<PublicOutput, StageAudioError>, Value) {
    let (start, frames) = request;
    let input_before = input_value(target, &renderer.plan);
    let before = controller(renderer);
    let monitor_before = self::monitor(monitor);
    let events_before = source.events.len();
    let calls_before = source.inner.calls;
    let capture = begin_capture(name, "public");
    let result = match target {
        PublicTarget::Definition(v) => renderer
            .read_definition(source, v, SignalSample(start), frames, TIMEOUT, cancelled)
            .map(PublicOutput::Definition),
        PublicTarget::Tape(v) => renderer
            .read_tape(source, v, SignalSample(start), frames, TIMEOUT, cancelled)
            .map(PublicOutput::Tape),
        PublicTarget::Routed(v) => renderer
            .read_routed_signal(source, v, SignalSample(start), frames, TIMEOUT, cancelled)
            .map(PublicOutput::Routed),
    };
    let observed = finish_capture(capture);
    let input_after = input_value(target, &renderer.plan);
    assert_eq!(input_before, input_after);
    let row = json!({"action":name,"entry":"actual public method; fresh internal ReadWork",
        "start":start,"frames":frames,"cancelled_at_end":cancelled.load(Ordering::Relaxed),
        "input_before":input_before,"input_after":input_after,
        "controller_before":before,"controller_result_live":controller(renderer),
        "monitor_before":monitor_before,"monitor_after":self::monitor(monitor),
        "provider":provider_value(source,events_before,calls_before),
        "result":output_value(&result),"trace":observed,
        "work_scope":"public internal ReadWork inaccessible after return; real observer snapshots only"});
    (result, row)
}
fn public_once(
    renderer: &mut StageAudio,
    source: &mut WitnessProvider<'_>,
    target: PublicTarget<'_, '_>,
    request: (i64, u32),
    name: &str,
    monitor: &crate::PreparationMonitor,
    cancelled: &AtomicBool,
) -> Value {
    let (result, mut row) =
        call_public(renderer, source, target, request, name, monitor, cancelled);
    drop(result);
    row["controller_after_result_drop"] = controller(renderer);
    row["result_lifetime"] = json!("borrowed snapshot then dropped before next action");
    row
}
fn public_pair(
    plan: &Arc<RenderPlan>,
    target: PublicTarget<'_, '_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    request: (i64, u32),
    name: &str,
) -> SetupResult<Value> {
    let _ops = mask_observations();
    let (mut renderer, monitor) = self::renderer(plan, StageLimits::default(), None);
    let mut source = provider(reopen()?, None, false);
    let cancelled = AtomicBool::new(false);
    let before = input_value(target, plan);
    let cold = public_once(
        &mut renderer,
        &mut source,
        target,
        request,
        &format!("{name}_cold"),
        &monitor,
        &cancelled,
    );
    let warm = public_once(
        &mut renderer,
        &mut source,
        target,
        request,
        &format!("{name}_warm_fresh_ReadWork"),
        &monitor,
        &cancelled,
    );
    let after = input_value(target, plan);
    assert_eq!(before, after);
    Ok(
        json!({"setup":"Ok","input_before":before,"input_after":after,"cold":cold,"warm":warm,
        "cache_scope":"same renderer; fresh public ReadWork; actual producer entry/miss/hit observed, not inferred"}),
    )
}
fn held_public(
    plan: &Arc<RenderPlan>,
    target: PublicTarget<'_, '_>,
    inner: NativeProvider,
    request: (i64, u32),
    name: &str,
) -> Value {
    if after::active() {
        return after::held_public(plan, target, inner, request, name);
    }
    let _ops = mask_observations();
    let (mut renderer, monitor) = self::renderer(plan, StageLimits::default(), None);
    let mut source = provider(inner, None, false);
    let (result, row) = call_public(
        &mut renderer,
        &mut source,
        target,
        request,
        name,
        &monitor,
        &AtomicBool::new(false),
    );
    let account = renderer.owned_suppression.observer();
    drop(source);
    let after_provider = output_value(&result);
    drop(renderer);
    let after_controller = output_value(&result);
    let live = account.as_ref().map(|v| v.live_frames());
    drop(result);
    json!({"action":row,"result_after_provider_drop":after_provider,
        "result_after_controller_drop":after_controller,
        "account_after_controller_drop_result_live":live,
        "account_after_result_drop":account.as_ref().map(|v|v.live_frames()),
        "scope":"actual returned owner across public internal work/provider/controller drop; no N native source-lease claim"})
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
enum PrivateTarget<'a, 'p> {
    Projected(&'a AudioStageProjection<'p>),
    Stage(&'a AudioStage<'p>),
}

fn private_input(target: PrivateTarget<'_, '_>, plan: &RenderPlan) -> Value {
    match target {
        PrivateTarget::Projected(p) => {
            json!({"kind":"Projected","projection":trace::projection_value(p,plan)})
        }
        PrivateTarget::Stage(s) => json!({"kind":"Stage","stage":s.descriptor(),
            "belongs_to_plan":s.belongs_to(plan),
            "input_support":s.input_signal().support(),"output_support":s.output_signal().support(),
            "input_sample_count":s.input_signal().sample_count().map(|v|v.0).map_err(|e|e.to_string()),
            "output_sample_count":s.output_signal().sample_count().map(|v|v.0).map_err(|e|e.to_string())}),
    }
}
fn private_once(
    renderer: &mut StageAudio,
    source: &mut WitnessProvider<'_>,
    target: PrivateTarget<'_, '_>,
    control: WorkControl<'_>,
    monitor: &crate::PreparationMonitor,
    name: &str,
) -> (Result<Prepared, StageAudioError>, Value) {
    let input_before = private_input(target, &renderer.plan);
    let before = controller(renderer);
    let work_before = work(control.work);
    let monitor_before = self::monitor(monitor);
    let events_before = source.events.len();
    let calls_before = source.inner.calls;
    let capture = begin_capture(name, "private_prepare");
    let result = match target {
        PrivateTarget::Projected(p) => renderer
            .prepare_projected(p, source, control, 1)
            .map(Prepared::Projected),
        PrivateTarget::Stage(s) => renderer
            .prepare_stage(s, source, control, 1)
            .map(Prepared::Stage),
    };
    let observed = finish_capture(capture);
    let input_after = private_input(target, &renderer.plan);
    assert_eq!(input_before, input_after);
    let row = json!({"action":name,"entry":"real private preparation with actual ReadWork; no private signal/provider construction",
    "depth":1,"input_before":input_before,"input_after":input_after,
    "controller_before":before,"controller_result_live":controller(renderer),
    "work_before":work_before,"work_after":work(control.work),
    "monitor_before":monitor_before,"monitor_after":self::monitor(monitor),
    "provider":provider_value(source,events_before,calls_before),"trace":observed,
    "result":match &result {
        Ok(v)=>json!({"status":"Ok","prepared":v.value()}),
        Err(e)=>json!({"status":"Error","error":typed_error(e)})
    }});
    (result, row)
}
#[derive(Default)]
struct PrivateOptions {
    repeat: bool,
    cancelled: bool,
    expired: bool,
    refuse_provider: bool,
}
fn private_action(
    plan: &Arc<RenderPlan>,
    target: PrivateTarget<'_, '_>,
    inner: NativeProvider,
    name: &str,
    limits: StageLimits,
    options: PrivateOptions,
) -> Value {
    if after::active() {
        return after::private_action(plan, target, inner, name, limits, options);
    }
    let _ops = mask_observations();
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(options.cancelled);
    let (mut renderer, monitor) = self::renderer(plan, limits, None);
    let mut source = provider(inner, Some(&budget), options.refuse_provider);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: if options.expired {
            Instant::now() - Duration::from_secs(1)
        } else {
            Instant::now() + TIMEOUT
        },
    };
    let (first, first_row) =
        private_once(&mut renderer, &mut source, target, control, &monitor, name);
    let second = if options.repeat && first.is_ok() {
        Some(private_once(
            &mut renderer,
            &mut source,
            target,
            control,
            &monitor,
            "same_ReadWork_memo",
        ))
    } else {
        None
    };
    let same = match (&first, &second) {
        (Ok(a), Some((Ok(b), _))) => Some(a.same(b)),
        _ => None,
    };
    let second_value = match &second {
        Some((_, row)) => json!({"attempted":true,"action":row}),
        None => {
            json!({"attempted":false,"reason":if options.repeat {"first actual preparation failed"}else{"single-action manifest entry"}})
        }
    };
    let final_work = work(&budget);
    let account = renderer.owned_suppression.observer();
    drop(source);
    drop(budget);
    let after_work = first.as_ref().ok().map(Prepared::value);
    drop(renderer);
    let after_controller = first.as_ref().ok().map(Prepared::value);
    let account_live = account.as_ref().map(|v| v.live_frames());
    drop(second);
    drop(first);
    json!({"setup":"Ok","first":first_row,"second":second_value,"same_actual_prepared_arc":same,
        "final_work":final_work,"prepared_after_ReadWork_drop":after_work,
        "prepared_after_controller_drop":after_controller,
        "account_after_controller_drop_prepared_live":account_live,
        "account_after_prepared_drop":account.as_ref().map(|v|v.live_frames()),
        "memo_scope":"second capture uses same actual control and accepted counters; no reset or new request",
        "cancelled_at_end":cancelled.load(Ordering::Relaxed)})
}

fn routed_source<'p>(
    plan: &'p RenderPlan,
    base: &AudioStageProjection<'p>,
) -> Result<AudioRoutedSignal<'p>, PlanError> {
    let owner = base.stage().input_signal();
    let query = owner.query(
        SignalSample(0)..SignalSample(1),
        AudioQueryLimits::default(),
    )?;
    let source = query
        .spans
        .iter()
        .find_map(|span| match &span.content {
            AudioSignalContent::Leaf(deadpan_plan::AudioContent::Source { source, .. }) => {
                Some(source.clone())
            }
            _ => None,
        })
        .ok_or(PlanError::NoSourceAudio)?;
    let voice = owner.source_voice(AudioSourceVoiceRecipe {
        mapping: deadpan_core::SourceAudioMapping::natural_rate(
            source.span,
            plan.metadata().presentation_basis.frame_rate,
        )?,
        source,
        offset: AudioSample(0),
    })?;
    let frames = voice.input_signal().sample_count()?.0;
    let route = SoundRoute::identity(ratio(i128::from(frames), 1))?;
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::PointCeil,
    )?;
    let count = route.nodes().len();
    AudioRoutedSignal::source(
        voice,
        AudioSoundRoute::<SignalSample>::new(route, vec![grid; count])?,
    )
}
fn routed_definition_source(plan: &RenderPlan) -> Result<AudioRoutedSignal<'_>, PlanError> {
    let definition = plan.audio_definition(AudioDefinitionSelector::Node { node: id("a") })?;
    let owner = definition.signal();
    let query = owner.query(
        SignalSample(0)..SignalSample(1),
        AudioQueryLimits::default(),
    )?;
    let source = query
        .spans
        .iter()
        .find_map(|span| match &span.content {
            AudioSignalContent::Leaf(deadpan_plan::AudioContent::Source { source, .. }) => {
                Some(source.clone())
            }
            _ => None,
        })
        .ok_or(PlanError::NoSourceAudio)?;
    let rate = plan.metadata().presentation_basis.frame_rate;
    let voice = owner.source_voice(AudioSourceVoiceRecipe {
        mapping: deadpan_core::SourceAudioMapping::natural_rate(source.span, rate)?,
        source,
        offset: AudioSample(0),
    })?;
    let route = SoundRoute::identity(owner.support().end)?;
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(deadpan_core::MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )?,
        AudioBoundaryRule::PointCeil,
    )?;
    let count = route.nodes().len();
    AudioRoutedSignal::source(
        voice,
        AudioSoundRoute::<SignalSample>::new(route, vec![grid; count])?,
    )
}
fn signed_tape<'p>(
    plan: &'p RenderPlan,
    signal: AudioSignal<'p>,
) -> Result<AudioSignalTape<'p>, PlanError> {
    let at = |n: i64| ratio(-1, 2).checked_add(ExactRatio::integer(n));
    AudioSignalTape::new(
        plan,
        at(0)?..at(10)?,
        vec![
            AudioSignalTapeRun::new(
                at(0)?..at(4)?,
                ExactRatio::ZERO..ExactRatio::integer(4),
                signal.clone(),
            ),
            AudioSignalTapeRun::new(
                at(4)?..at(6)?,
                ExactRatio::integer(4)..ExactRatio::integer(6),
                signal.clone(),
            ),
            AudioSignalTapeRun::new(
                at(6)?..at(10)?,
                ExactRatio::integer(6)..ExactRatio::integer(10),
                signal,
            ),
        ],
    )
}
fn bound_document(asset: &deadpan_core::AssetRecord) -> SetupResult<(Arc<RenderPlan>, Value)> {
    let reference: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/internal-suppression-paths-baseline-v1/snapshot.json"
    ))?;
    let case = reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "private-point-genuine-preserve-bound")
        .expect("existing genuine checked public binding fixture");
    let mut wire = case["document_before"].clone();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        deadpan_core::AssetId::new("media")?,
        asset,
    )]))?;
    let checked = ProjectDocument::from_json(&wire.to_string())?;
    let plan = Arc::new(RenderPlan::compile(&checked)?);
    Ok((
        plan,
        json!({"checked_document":wire,
        "historical_layout_nodes":case["historical_document"]["nodes"],
        "actual_binding_state":case["document_before"]["audio_bindings"],
        "source":"existing frozen checked public binding document, current media asset replaced by actual registration before checked parse/compile; no private Plan mutation",
        "reference_case":"private-point-genuine-preserve-bound"}),
    ))
}
fn bound_tape<'p>(plan: &'p RenderPlan) -> Result<AudioSignalTape<'p>, PlanError> {
    let definition = plan.audio_definition(AudioDefinitionSelector::Node { node: id("b") })?;
    let point = definition.in_point_clock(
        AudioRootPlacement::new(
            ExactRatio::ZERO,
            ExactRatio::ONE,
            ExactRatio::ZERO..definition.exact_duration(),
        )?,
        ExactRatio::ZERO,
    )?;
    let signal = point.signal_ref().clone();
    AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ExactRatio::integer(128),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ExactRatio::integer(128),
            ExactRatio::ZERO..ExactRatio::integer(128),
            signal,
        )],
    )
}
fn setup_plan_error(error: PlanError) -> Value {
    json!({"setup":"Error","error":typed_error(&StageAudioError::Plan(error)),
        "calls_not_run":"actual public construction rejected"})
}

fn persistent(
    plan: &Arc<RenderPlan>,
    target: PublicTarget<'_, '_>,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let _ops = mask_observations();
    let directory = tempfile::tempdir()?;
    let cache = PcmCache::at(directory.path())?;
    let before = entries(&cache);
    let (mut first, monitor) = renderer(plan, StageLimits::default(), Some(cache.clone()));
    let mut source = provider(reopen()?, None, false);
    let cold = public_once(
        &mut first,
        &mut source,
        target,
        (0, 256),
        "persistent_empty_directory",
        &monitor,
        &AtomicBool::new(false),
    );
    drop(source);
    drop(first);
    let after_cold = entries(&cache);
    let (mut fresh, monitor) = renderer(plan, StageLimits::default(), Some(cache.clone()));
    let mut source = provider(reopen()?, None, false);
    let restored = public_once(
        &mut fresh,
        &mut source,
        target,
        (0, 256),
        "persistent_fresh_renderer_same_directory",
        &monitor,
        &AtomicBool::new(false),
    );
    drop(source);
    drop(fresh);
    Ok(json!({"setup":"Ok","entries_before":before,"cold":cold,
        "entries_after_cold":after_cold,"fresh_renderer":restored,"entries_after_restore":entries(&cache),
        "scope":"real public cache configuration; scan, restoration and N producer entry are observed independently; positive Cache not assumed"}))
}
fn old_2048(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    inner: NativeProvider,
) -> Value {
    let _ops = mask_observations();
    let (mut renderer, monitor) = self::renderer(plan, StageLimits::default(), None);
    let mut source = provider(inner, None, false);
    let route = SoundRoute::identity(ratio(i128::from(base.output_frames()), 1)).unwrap();
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::PointCeil,
    )
    .unwrap();
    let count = route.nodes().len();
    let domain = AudioRoutedSignal::projected(
        Arc::clone(base),
        AudioSoundRoute::<SignalSample>::new(route, vec![grid; count]).unwrap(),
    )
    .unwrap();
    let cancelled = AtomicBool::new(false);
    let warmup = public_once(
        &mut renderer,
        &mut source,
        PublicTarget::Routed(&domain),
        (0, 256),
        "old2048_default_warmup",
        &monitor,
        &cancelled,
    );
    renderer.limits.maximum_resident_frames = 2048;
    let measured = public_once(
        &mut renderer,
        &mut source,
        PublicTarget::Routed(&domain),
        (512, 256),
        "old2048_literal_measured",
        &monitor,
        &cancelled,
    );
    json!({"warmup":warmup,"measured":measured,
        "scope":"literal legal cap set before measured fresh public ReadWork; no success or target reach assumed"})
}
fn fixture_cases(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    asset: &deadpan_core::AssetRecord,
    wire: &Value,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    provenance: Value,
) -> SetupResult<Value> {
    let projection_before = trace::projection_value(base, plan);
    let mut families = BTreeMap::<&str, Value>::new();
    let mut controls = BTreeMap::<&str, Value>::new();
    let mut plain_wire_value = Value::Null;
    match plain_nested_plan(asset, wire) {
        Ok((plain_plan, plain_wire)) => {
            plain_wire_value = plain_wire.clone();
            match plain_plan.audio_definition(AudioDefinitionSelector::Node { node: id("group") }) {
                Ok(group) => {
                    let target = PublicTarget::Definition(&group);
                    let pair =
                        public_pair(&plain_plan, target, reopen, (0, 256), "definition_group")?;
                    let held = held_public(
                        &plain_plan,
                        target,
                        reopen()?,
                        (512, 128),
                        "definition_group_Hold_result_lifetime",
                    );
                    families.insert(
                        "definition_group",
                        json!({"setup":"Ok","source_and_empty_pair":pair,
                        "Hold_side_result_lifetime":held,"document":plain_wire}),
                    );
                    for (name, request, is_cancelled) in [
                        ("cancelled_public", (0, 256), true),
                        ("zero_public_frames", (0, 0), false),
                    ] {
                        let _ops = mask_observations();
                        let (mut renderer, monitor) =
                            self::renderer(&plain_plan, StageLimits::default(), None);
                        let mut source = provider(reopen()?, None, false);
                        controls.insert(
                            name,
                            public_once(
                                &mut renderer,
                                &mut source,
                                target,
                                request,
                                name,
                                &monitor,
                                &AtomicBool::new(is_cancelled),
                            ),
                        );
                    }
                }
                Err(e) => {
                    families.insert("definition_group", setup_plan_error(e));
                    controls.insert("cancelled_public",json!({"attempted":false,"reason":"group definition construction rejected"}));
                    controls.insert("zero_public_frames",json!({"attempted":false,"reason":"group definition construction rejected"}));
                }
            }
            match plain_plan.audio_definition(AudioDefinitionSelector::Node { node: id("outer") }) {
                Ok(definition) => {
                    families.insert(
                        "nested_definition",
                        public_pair(
                            &plain_plan,
                            PublicTarget::Definition(&definition),
                            reopen,
                            (0, 256),
                            "nested_definition",
                        )?,
                    );
                    families.insert(
                        "persistent_nested",
                        persistent(&plain_plan, PublicTarget::Definition(&definition), reopen)?,
                    );
                    match extract_stage(definition.signal()) {
                        Ok(stage) => {
                            families.insert(
                                "streamed_Signal",
                                private_action(
                                    &plain_plan,
                                    PrivateTarget::Stage(&stage),
                                    reopen()?,
                                    "streamed_Signal_first",
                                    StageLimits::default(),
                                    PrivateOptions {
                                        repeat: true,
                                        ..Default::default()
                                    },
                                ),
                            );
                            controls.insert(
                                "nested_depth1",
                                private_action(
                                    &plain_plan,
                                    PrivateTarget::Stage(&stage),
                                    reopen()?,
                                    "nested_depth1",
                                    StageLimits {
                                        maximum_depth: 1,
                                        ..Default::default()
                                    },
                                    PrivateOptions::default(),
                                ),
                            );
                        }
                        Err(e) => {
                            families.insert("streamed_Signal", setup_plan_error(e));
                            controls.insert("nested_depth1",json!({"attempted":false,"reason":"actual public query exposed no Stage"}));
                        }
                    }
                }
                Err(e) => {
                    families.insert("nested_definition", setup_plan_error(e));
                    families.insert(
                        "persistent_nested",
                        json!({"setup":"Skipped","reason":"outer definition rejected"}),
                    );
                    families.insert(
                        "streamed_Signal",
                        json!({"setup":"Skipped","reason":"outer definition rejected"}),
                    );
                    controls.insert(
                        "nested_depth1",
                        json!({"attempted":false,"reason":"outer definition rejected"}),
                    );
                }
            }
        }
        Err(e) => {
            families.insert("plain_document", setup_error("plain_document", e.as_ref()));
            for name in [
                "definition_group",
                "nested_definition",
                "persistent_nested",
                "streamed_Signal",
            ] {
                families.insert(name,json!({"setup":"Skipped","reason":"actual checked plain document setup failed"}));
            }
            for name in ["cancelled_public", "zero_public_frames", "nested_depth1"] {
                controls.insert(name,json!({"attempted":false,"reason":"actual checked plain document setup failed"}));
            }
        }
    }
    match base.input_tape() {
        Some(tape) => {
            let pair = public_pair(
                plan,
                PublicTarget::Tape(tape),
                reopen,
                (512, 128),
                "public_input_Tape",
            )?;
            let _ops = mask_observations();
            let (mut renderer, monitor) = self::renderer(plan, StageLimits::default(), None);
            let mut source = provider(reopen()?, None, false);
            let empty = public_once(
                &mut renderer,
                &mut source,
                PublicTarget::Tape(tape),
                (0, 256),
                "public_Tape_Source_side",
                &monitor,
                &AtomicBool::new(false),
            );
            drop(source);
            drop(renderer);
            let (mut renderer, monitor) = self::renderer(plan, StageLimits::default(), None);
            let mut source = provider(reopen()?, None, false);
            let negative = public_once(
                &mut renderer,
                &mut source,
                PublicTarget::Tape(tape),
                (-1, 1),
                "negative_public_Tape",
                &monitor,
                &AtomicBool::new(false),
            );
            families.insert(
                "public_Tape",
                json!({"setup":"Ok","Hold_side_pair":pair,"Source_side":empty}),
            );
            controls.insert("negative_public_Tape", negative);
        }
        None => {
            families.insert(
                "public_Tape",
                json!({"setup":"Skipped","reason":"actual projection input is not Tape"}),
            );
            controls.insert(
                "negative_public_Tape",
                json!({"attempted":false,"reason":"actual projection input is not Tape"}),
            );
        }
    }
    let scoped_stage_input_attempt = match routed_source(plan, base) {
        Ok(domain) => json!({"setup":"Ok","input":input_value(PublicTarget::Routed(&domain),plan),
            "calls_not_run":"original scoped construction observed only; complete definition fixture supplies the two routed reads"}),
        Err(e) => setup_plan_error(e),
    };
    let complete_definition_source = match plain_nested_plan(asset, wire) {
        Ok((source_plan, source_wire)) => match routed_definition_source(&source_plan) {
            Ok(domain) => {
                let definition = source_plan
                    .audio_definition(AudioDefinitionSelector::Node { node: id("a") })?;
                let definition_before = definition_value(&definition, &source_plan);
                let pair = public_pair(
                    &source_plan,
                    PublicTarget::Routed(&domain),
                    reopen,
                    (0, 256),
                    "routed_complete_definition_Source",
                )?;
                let definition_after = definition_value(&definition, &source_plan);
                assert_eq!(definition_before, definition_after);
                json!({"setup":"Ok","document":source_wire,
                    "definition_before":definition_before,"definition_after":definition_after,
                    "pair":pair,
                    "scope":"actual complete untransformed public Node-a definition; registered qualified WAV asset retained"})
            }
            Err(e) => json!({"setup":"Error","document":source_wire,
                "error":typed_error(&StageAudioError::Plan(e))}),
        },
        Err(e) => setup_error("complete_definition_source_checked_document", e.as_ref()),
    };
    families.insert(
        "routed_Source",
        json!({"scoped_stage_input_attempt":scoped_stage_input_attempt,
            "complete_definition_source":complete_definition_source,
            "call_bound":2,"scope":"original rejected construction retained; only complete definition fixture runs cold/warm"}),
    );
    families.insert(
        "streamed_Tape",
        private_action(
            plan,
            PrivateTarget::Projected(base),
            reopen()?,
            "streamed_Tape_first",
            StageLimits::default(),
            PrivateOptions {
                repeat: true,
                ..Default::default()
            },
        ),
    );
    match mixed_projection(plan, base) {
        Ok(mixed) => {
            families.insert(
                "Mix_control",
                private_action(
                    plan,
                    PrivateTarget::Projected(&mixed),
                    reopen()?,
                    "Mix_control",
                    StageLimits::default(),
                    PrivateOptions::default(),
                ),
            );
        }
        Err(e) => {
            families.insert("Mix_control", setup_plan_error(e));
        }
    }
    match signed_tape(plan, base.stage().input_signal()) {
        Ok(tape) => {
            let _ops = mask_observations();
            let (mut renderer, monitor) = self::renderer(plan, StageLimits::default(), None);
            let mut source = provider(reopen()?, None, false);
            families.insert(
                "signed_fractional_Tape",
                json!({"setup":"Ok","tape":tape_value(&tape,plan),
                "action":public_once(&mut renderer,&mut source,PublicTarget::Tape(&tape),(0,8),
                    "signed_fractional_Tape",&monitor,&AtomicBool::new(false))}),
            );
        }
        Err(e) => {
            families.insert("signed_fractional_Tape", setup_plan_error(e));
        }
    }
    match bound_document(asset) {
        Ok((bound_plan, bound_wire)) => match bound_tape(&bound_plan) {
            Ok(tape) => {
                let _ops = mask_observations();
                let (mut renderer, monitor) =
                    self::renderer(&bound_plan, StageLimits::default(), None);
                let mut source = provider(reopen()?, None, false);
                families.insert("genuine_Bound_attempt",json!({"setup":"Ok","document":bound_wire,
                    "tape_before":tape_value(&tape,&bound_plan),
                    "action":public_once(&mut renderer,&mut source,PublicTarget::Tape(&tape),(0,32),
                        "genuine_Bound_attempt",&monitor,&AtomicBool::new(false)),
                    "tape_after":tape_value(&tape,&bound_plan),
                    "claim":"only actual graph tags prove Bound reachability; checked rejection remains literal"}));
            }
            Err(e) => {
                families.insert(
                    "genuine_Bound_attempt",
                    json!({"setup":"Error","document":bound_wire,
                "error":typed_error(&StageAudioError::Plan(e))}),
                );
            }
        },
        Err(e) => {
            families.insert(
                "genuine_Bound_attempt",
                setup_error("qualified_public_bound_document", e.as_ref()),
            );
        }
    }
    for (name, options, limits) in [
        (
            "expired_private",
            PrivateOptions {
                expired: true,
                ..Default::default()
            },
            StageLimits::default(),
        ),
        (
            "qualified_provider_refusal",
            PrivateOptions {
                refuse_provider: true,
                ..Default::default()
            },
            StageLimits::default(),
        ),
        (
            "input_limit639",
            PrivateOptions::default(),
            StageLimits {
                maximum_input_frames: 639,
                ..Default::default()
            },
        ),
    ] {
        controls.insert(
            name,
            private_action(
                plan,
                PrivateTarget::Projected(base),
                reopen()?,
                name,
                limits,
                options,
            ),
        );
    }
    controls.insert("literal_old2048", old_2048(plan, base, reopen()?));
    assert_eq!(controls.len(), 8);
    let projection_after = trace::projection_value(base, plan);
    assert_eq!(projection_before, projection_after);
    Ok(
        json!({"provenance":provenance,"projection_before":projection_before,"projection_after":projection_after,
        "plain_checked_document":plain_wire_value,"families":families,"controls":controls,
        "normal_call_bound":19,"control_scenarios":8,"control_calls_including_warmup_bound":9,
        "dependent_skips":"first errors and setup rejections retained; counts are actual traces, not family labels"}),
    )
}
pub(super) fn fixture_visit(
    plan: &Arc<RenderPlan>,
    base: &Arc<AudioStageProjection<'_>>,
    asset: &deadpan_core::AssetRecord,
    wire: &Value,
    reopen: &mut impl FnMut() -> SetupResult<NativeProvider>,
    provenance: impl FnOnce() -> SetupResult<Value>,
) -> SetupResult<Option<Value>> {
    if after::active() {
        return after::fixture_visit(plan, base, asset, wire, reopen, provenance);
    }
    if !FOCUSED_N.with(std::cell::Cell::get) {
        return Ok(None);
    }
    Ok(Some(fixture_cases(
        plan,
        base,
        asset,
        wire,
        reopen,
        provenance()?,
    )?))
}
fn capture_body() -> Value {
    let _focus = focus();
    let actual = match super::family() {
        Ok(v) => v,
        Err(e) => setup_error("registration_and_checked_native_fixture", e.as_ref()),
    };
    json!({"schema":"native-read-signal-policy-ordinary-before-v1","actual":actual,
        "scope":{"selected":"ordinary StageAudio::read_signal policy after separate ordinary graph query/debit",
            "normal_call_bound":19,"control_scenarios":8,"control_calls_including_warmup_bound":9,"checkpoint":CHECKPOINT_N,
            "ordinary_source_and_return_storage":"actual capacities and capacity-derived Layouts separately; no expected delta",
            "admission_local_fields":"not present for ordinary N source; existing M/L/J activity stays separately scoped",
            "input_private_fields":"private Tape run inventory/opaque identities/nested heap capacities unexposed",
            "observation_storage":"JSON/test metadata and file readback buffers outside measured request ledger",
            "volatile_unsampled":["remaining duration","deadline timestamp","cache unused_seconds age","age-based listing order"],
            "no_claims":["N admitted source lease","N canonical/additional/forwarded split","native allocation request from capacity Layout",
                "signed request success","Bound reachability before actual graph receipt","positive Cache before actual nested scan",
                "product Gate or whole preparation"]}})
}
#[test]
#[ignore = "manual fresh actual ordinary N BEFORE capture; no expected fixture exists"]
fn capture_read_signal_policy_before_v1() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        capture_read_signal_policy_before_v1_historical,
    );
}
fn capture_read_signal_policy_before_v1_historical() {
    let output = std::env::var_os("DEADPAN_READ_SIGNAL_POLICY_BEFORE_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_READ_SIGNAL_POLICY_BEFORE_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_READ_SIGNAL_POLICY_BEFORE_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_READ_SIGNAL_POLICY_BEFORE_PUBLISHED_CHECKPOINT").unwrap(),
        CHECKPOINT_N
    );
    let mut body = capture_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(CHECKPOINT_N);
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
#[ignore = "external actual full body only; no placeholder oracle42"]
fn read_signal_policy_before_external_body_regression() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        read_signal_policy_before_external_body_regression_historical,
    );
}
fn read_signal_policy_before_external_body_regression_historical() {
    let path = std::env::var_os("DEADPAN_READ_SIGNAL_POLICY_BEFORE_EXPECTED").unwrap();
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    key_replay::assert_relation(&expected, &capture_body());
}

#[test]
fn read_signal_policy_frozen_before_key_relation_is_exact() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(|| {
        let bytes = include_bytes!("../tests/fixtures/read-signal-policy-baseline-v1/corpus.json");
        use sha2::Digest;
        assert_eq!(bytes.len(), 9_909_154);
        assert_eq!(
            sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "3a3d1e540a904fbd55072784f05c54eb9ab00dfeb4674dba86c961cf5ca4f7ff"
        );
        let frozen: Value = serde_json::from_slice(bytes).unwrap();
        let current = capture_body();
        key_replay::assert_relation(&frozen, &current);
        key_replay::assert_mutations_rejected(&frozen, &current);
    });
}
#[path = "read_signal_policy_key_replay.rs"]
mod key_replay;

#[path = "read_signal_policy_after_tests.rs"]
mod after;
