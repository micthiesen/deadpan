//! Ordinary before40: actual native preflight, with observation storage outside its ledger.
use super::*;
use crate::stages::intrinsic_preflight_before_trace as trace;

const BASE: &str = "c9049458df03356611ae4b34f50db5d319bb086d";

struct Observations;
impl Drop for Observations {
    fn drop(&mut self) {
        trace::take();
        point_suppression::ops::trace::disable();
        point_suppression::work_trace::disable();
    }
}
fn begin_observations() -> Observations {
    point_suppression::ops::trace::reset();
    point_suppression::work_trace::reset();
    trace::begin(Box::new(|mut event, renderer, control| {
        event["accepted_native_work"] = work(control.work);
        event["controller"] = controller(renderer);
        event["cancelled"] = json!(control.cancelled.load(Ordering::Relaxed));
        event
    }));
    Observations
}
fn finish_observations() -> Value {
    let mut value = trace::take();
    value["mask_operations"] =
        serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
    value["public_owner_work"] =
        serde_json::to_value(point_suppression::work_trace::snapshot()).unwrap();
    point_suppression::ops::trace::disable();
    point_suppression::work_trace::disable();
    assert_eq!(value["dropped"], 0);
    assert_eq!(value["invalid"], false);
    assert_eq!(value["active_at_take"], json!([]));
    value
}
struct WitnessProvider<'a> {
    inner: NativeProvider,
    budget: Option<&'a RefCell<ReadWork>>,
    events: Vec<Value>,
    fail: bool,
}
impl AudioSourceProvider for WitnessProvider<'_> {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &deadpan_core::AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        let mut event = json!({"project":project,"revision":revision,"asset":asset,
            "cancelled":cancelled.load(Ordering::Relaxed),
            "accepted_before_callback":self.budget.map(work),"will_refuse":self.fail,
            "preflight_position_at_callback_entry":trace::position(),
            "position_scope":"provider callback entry relative to preflight observations; not decoder I/O",
            "prepared_index":self.inner.prepared.index(),
            "prepared_matrix":self.inner.prepared.matrix_layout(),
            "prepared_provenance":self.inner.prepared.provenance()});
        if let Some(position) = stage_validation::provider_position() {
            event["stage_validation_position_at_callback_entry"] = position;
        }
        self.events.push(event);
        if self.fail {
            return Err(PreparationError::SourceUnavailable(
                "before40 explicit provider refusal".into(),
            ));
        }
        self.inner.source(project, revision, asset, cancelled)
    }
}
fn public_action(
    renderer: &mut StageAudio,
    provider: &mut WitnessProvider<'_>,
    projection: &Arc<AudioStageProjection<'_>>,
    name: &str,
) -> Value {
    let _capture = begin_observations();
    let native_capture = after::begin_action(name, "public");
    let provider_events_before = provider.events.len();
    let provider_source_calls_before = provider.inner.calls;
    let before = controller(renderer);
    let route = SoundRoute::identity(ratio(i128::from(projection.output_frames()), 1)).unwrap();
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::PointCeil,
    )
    .unwrap();
    let count = route.nodes().len();
    let route = AudioSoundRoute::<SignalSample>::new(route, vec![grid; count]).unwrap();
    let domain = AudioRoutedSignal::projected(Arc::clone(projection), route).unwrap();
    let frames = u32::try_from(projection.output_frames().min(256)).unwrap();
    let result = renderer.read_routed_signal(
        provider,
        &domain,
        SignalSample(0),
        frames,
        TIMEOUT,
        &AtomicBool::new(false),
    );
    after::end_action(native_capture);
    let observed = finish_observations();
    let output = public_result(&result);
    let after = controller(renderer);
    let account = renderer.owned_suppression.observer();
    let live_result = account.as_ref().map(|v| v.live_frames());
    drop(result);
    json!({"action":name,"entry":"public read_routed_signal","start":0,"frames":frames,
        "before":before,"result":output,"after_result_live":after,
        "account_result_live":live_result,"after_result_drop":controller(renderer),
        "account_after_result_drop":account.as_ref().map(|v|v.live_frames()),
        "provider_callbacks":provider.events.len(),"provider_source_calls":provider.inner.calls,
        "provider_events_before":provider_events_before,
        "provider_source_calls_before":provider_source_calls_before,
        "provider_event_bounds":[provider_events_before,provider.events.len()],
        "provider_counter_scope":"events/callbacks/source calls are cumulative across cold and warm; bounds identify this action",
        "provider_events":provider.events,"trace":observed,
        "work_scope":"public internal ReadWork inaccessible after return; callback/observer snapshots are actual",
        "result_lifetime":"snapshotted then dropped before next action"})
}
fn prepare_action(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    inner: NativeProvider,
    name: &str,
    limits: StageLimits,
    cancelled_at_entry: bool,
    fail_provider: bool,
) -> Value {
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(cancelled_at_entry);
    let mut provider = WitnessProvider {
        inner,
        budget: Some(&budget),
        events: Vec::new(),
        fail: fail_provider,
    };
    let mut renderer = StageAudio::with_limits(Arc::clone(plan), limits).unwrap();
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: Instant::now() + TIMEOUT,
    };
    let before = controller(&renderer);
    let _capture = begin_observations();
    let native_capture = after::begin_action(name, "private_first");
    let first = renderer.prepare_projected(projection, &mut provider, control, 1);
    after::end_action(native_capture);
    let first_trace = finish_observations();
    let first_value = match &first {
        Ok(b) => json!({"status":"Ok","prepared":prepared(b)}),
        Err(e) => json!({"status":"Error","error":error(e)}),
    };
    let first_work = work(&budget);
    let first_controller = controller(&renderer);
    let second = if first.is_ok() && !fail_provider && !cancelled_at_entry {
        let _capture = begin_observations();
        let native_capture = after::begin_action(name, "private_memo");
        let result = renderer.prepare_projected(projection, &mut provider, control, 1);
        after::end_action(native_capture);
        let observed = finish_observations();
        Some((result, observed))
    } else {
        None
    };
    let second_value = second.as_ref().map(|(r, t)| {
        json!({"result":match r {
        Ok(b)=>json!({"status":"Ok","prepared":prepared(b)}),
        Err(e)=>json!({"status":"Error","error":error(e)})},"trace":t})
    });
    let same_arc = match (&first, &second) {
        (Ok(a), Some((Ok(b), _))) => Some(Arc::ptr_eq(a, b)),
        _ => None,
    };
    let final_work = work(&budget);
    let after = controller(&renderer);
    let events = std::mem::take(&mut provider.events);
    let calls = provider.inner.calls;
    let account = renderer.owned_suppression.observer();
    drop(provider);
    drop(budget);
    let after_work = first.as_ref().ok().map(|b| prepared(b));
    drop(renderer);
    let after_controller = first.as_ref().ok().map(|b| prepared(b));
    let live = account.as_ref().map(|v| v.live_frames());
    drop(second);
    drop(first);
    json!({"action":name,"entry":"private prepare_projected","depth":1,
        "cancelled_at_entry":cancelled_at_entry,"first":first_value,"first_trace":first_trace,
        "before":before,"first_work":first_work,"first_controller":first_controller,
        "second_same_ReadWork":second_value,"same_prepared_arc":same_arc,"final_work":final_work,
        "after":after,"provider_events":events,"provider_source_calls":calls,
        "prepared_after_ReadWork_drop":after_work,"prepared_after_controller_drop":after_controller,
        "account_after_controller_drop_prepared_live":live,
        "account_after_prepared_drop":account.as_ref().map(|v|v.live_frames())})
}
fn observe_family(
    plan: &Arc<RenderPlan>,
    projection: &Arc<AudioStageProjection<'_>>,
    name: &str,
    provider: &mut impl FnMut() -> SetupResult<NativeProvider>,
) -> SetupResult<Value> {
    let _family = after::family_context(name);
    let identity_before = trace::projection_value(projection, plan);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut source = WitnessProvider {
        inner: provider()?,
        budget: None,
        events: Vec::new(),
        fail: false,
    };
    let cold = public_action(&mut renderer, &mut source, projection, "cold");
    let warm = public_action(
        &mut renderer,
        &mut source,
        projection,
        "warm_renderer_fresh_public_ReadWork",
    );
    let account = renderer.owned_suppression.observer();
    drop(source);
    drop(renderer);
    let memo = prepare_action(
        plan,
        projection,
        provider()?,
        "same_request_memo",
        StageLimits::default(),
        false,
        false,
    );
    Ok(
        json!({"family":name,"setup":"Ok","projection_before":identity_before,
        "projection_after":trace::projection_value(projection,plan),"public_cold":cold,"public_warm":warm,
        "account_after_public_renderer_drop":account.as_ref().map(|v|v.live_frames()),
        "private_prepare":memo}),
    )
}
fn retime_node(child: &str, output: i64, input: i64) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Plain public Preserve".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id(child),
            duration: FrameDuration::new(output).unwrap(),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn plain_nested_plan(
    asset: &deadpan_core::AssetRecord,
    source_wire: &Value,
) -> SetupResult<(Arc<RenderPlan>, Value)> {
    let empty = ProjectDocument::new(
        ProjectId::new("before40-plain-stage")?,
        RevisionId::new("checked")?,
        deadpan_core::PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1)?,
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        id("root"),
    )?;
    let mut wire = serde_json::to_value(empty)?;
    let source: BeatNode = serde_json::from_value(source_wire["nodes"]["a"].clone())?;
    let hold = BeatNode::hold(
        "SilentHold",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(128)?,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    );
    let nodes = BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("outer")])),
        (id("a"), source),
        (id("room"), hold),
        (
            id("group"),
            BeatNode::sequence("Input", vec![id("a"), id("room")]),
        ),
        (id("inner"), retime_node("group", 768, 640)),
        (id("outer"), retime_node("inner", 640, 768)),
    ]);
    wire["nodes"] = serde_json::to_value(nodes)?;
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        deadpan_core::AssetId::new("media")?,
        asset,
    )]))?;
    let checked = ProjectDocument::from_json(&wire.to_string())?;
    Ok((Arc::new(RenderPlan::compile(&checked)?), wire))
}
fn extract_stage<'p>(
    signal: deadpan_plan::AudioSignal<'p>,
) -> Result<deadpan_plan::AudioStage<'p>, PlanError> {
    let query = signal.query(
        SignalSample(0)..SignalSample(1),
        AudioQueryLimits::default(),
    )?;
    query
        .spans
        .into_iter()
        .find_map(|span| match span.content {
            AudioSignalContent::Stage(stage) => Some(stage),
            _ => None,
        })
        .ok_or(PlanError::InvalidPlan(
            "actual public query did not expose a plain Stage",
        ))
}
fn projection_for_plain<'p>(
    plan: &'p RenderPlan,
) -> Result<Arc<AudioStageProjection<'p>>, PlanError> {
    let stage = extract_stage(plan.audio_signal())?;
    let signal = stage.input_signal();
    let input_end = signal.sample_count()?.0;
    let output_end = stage.output_signal().sample_count()?.0;
    let tape = |extent: i64| {
        AudioSignalTape::new(
            plan,
            ExactRatio::ZERO..ratio(i128::from(extent), 1),
            vec![AudioSignalTapeRun::new(
                ExactRatio::ZERO..ratio(i128::from(extent), 1),
                ExactRatio::ZERO..ratio(i128::from(input_end), 1),
                signal.clone(),
            )],
        )
    };
    let input = tape(input_end)?;
    let output = tape(output_end)?;
    AudioStageProjection::new(stage, input, output, FrameDuration::new(output_end)?)
}
fn mixed_projection<'p>(
    plan: &'p RenderPlan,
    base: &AudioStageProjection<'p>,
) -> Result<Arc<AudioStageProjection<'p>>, PlanError> {
    let signal = base.stage().input_signal();
    let tape = || {
        AudioSignalTape::new(
            plan,
            ExactRatio::ZERO..ratio(640, 1),
            vec![AudioSignalTapeRun::new(
                ExactRatio::ZERO..ratio(640, 1),
                ExactRatio::ZERO..ratio(640, 1),
                signal.clone(),
            )],
        )
    };
    let mix = deadpan_plan::AudioSignalMix::new(plan, vec![tape()?, tape()?], Vec::new())?;
    let output = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ratio(768, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(768, 1),
            ExactRatio::ZERO..ratio(640, 1),
            signal,
        )],
    )?;
    AudioStageProjection::new_mixed_input(
        base.stage().clone(),
        mix,
        output,
        FrameDuration::new(768)?,
    )
}
fn setup_error(name: &str, e: &dyn std::error::Error) -> Value {
    let mut chain = Vec::new();
    let mut current = e.source();
    while let Some(value) = current {
        chain.push(value.to_string());
        current = value.source();
    }
    json!({"family":name,"setup":"Error","error":{"display":e.to_string(),"source_chain":chain,
        "typed_discriminator":"external setup error trait object; concrete type unexposed"},
        "actions":"NotRun; actual checked/store/provider setup failed"})
}

fn family() -> SetupResult<Value> {
    let directory = tempfile::tempdir()?;
    let initial = ProjectDocument::new(
        ProjectId::new("native-tape-source-qualification")?,
        RevisionId::new("initial")?,
        deadpan_core::PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1)?,
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        id("root"),
    )?;
    let mut store = ProjectStore::create(&directory.path().join("fixture.deadpan"), &initial)?;
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()?;
    let cancelled = AtomicBool::new(false);
    let record = store
        .retain_original(
            &path,
            OriginalOwnership::Managed,
            media_limits(),
            &cancelled,
        )?
        .record;
    let audio = open_audio(&store, &record)?;
    let decoded = DecodedSourceQualification::for_registration(
        None,
        Some(&audio),
        Some(AudioLayoutInterpretation::StereoLeftRight),
    )?;
    let outcome = store.register_source(
        &SourceRegistration {
            expected_revision: store.snapshot()?.revision_id().clone(),
            new_revision: RevisionId::new("registered")?,
            original: record.object().content().clone(),
            new_asset_id: deadpan_core::AssetId::new("media")?,
            label: "Real stereo WAV".into(),
            insertion: None,
        },
        &decoded,
        None,
        media_limits(),
        &cancelled,
    )?;
    let receipt = store.source_qualification(&outcome.qualification)?;
    assert_eq!(receipt.id(), &outcome.qualification);
    assert_eq!(receipt.snapshot(), decoded.snapshot());
    assert_eq!(receipt.snapshot().content().sha256(), record.sha256());
    assert_eq!(
        receipt.snapshot().content().byte_length(),
        record.object().byte_length()
    );
    let registered = store.snapshot()?;
    let asset = registered
        .assets()
        .get(&outcome.asset_id)
        .expect("actual register_source asset");
    assert_eq!(asset, &receipt.asset_record("Real stereo WAV".into())?);
    let (plan, wire) = qualified_plan(asset)?;
    let base = projected(&plan);
    let source_index = audio.index().clone();
    let provider = NativeProvider {
        prepared: crate::PreparedSource::with_layout(
            audio,
            &source_index,
            receipt
                .snapshot()
                .audio_layout()
                .expect("registered audio layout"),
            &cancelled,
        )?,
        calls: 0,
    };
    assert_eq!(Some(provider.prepared.index()), receipt.snapshot().audio());
    assert_eq!(
        Some(provider.prepared.matrix_layout()),
        receipt.snapshot().audio_layout()
    );
    let actual_native_contract = json!({
        "index":provider.prepared.index(),"matrix_layout":provider.prepared.matrix_layout(),
        "provenance":provider.prepared.provenance(),
    });

    let mut first_provider = Some(provider);
    let mut reopen = || match first_provider.take() {
        Some(value) => Ok(value),
        None => native_provider(&store, &record, receipt.snapshot()),
    };
    if let Some(focused) = stage_validation::fixture_visit(
        &plan,
        &base,
        asset,
        &wire,
        &mut reopen,
        || {
            Ok(json!({"original_record":record,"registration":outcome,
                "receipt_id":receipt.id(),"receipt_original":receipt.original(),
                "qualification_snapshot":receipt.snapshot(),
                "qualification_snapshot_canonical_utf8":String::from_utf8(receipt.snapshot().to_json()?)?,
                "registered_asset":asset,"registered_project":serde_json::to_value(&registered)?,
                "checked_native_document_wire":wire,"native_provider":actual_native_contract,
                "fixture_path":"native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav",
                "qualification_scope":"actual persisted temporary registration; original qualifying native session and verified reopens"}))
        },
    )? {
        return Ok(focused);
    }
    if let Some(focused) = after::fixture_visit(&plan, &base, asset, &wire, &mut reopen, || {
        Ok(json!({"original_record":record,"registration":outcome,
                "receipt_id":receipt.id(),"receipt_original":receipt.original(),
                "qualification_snapshot":receipt.snapshot(),
                "qualification_snapshot_canonical_utf8":String::from_utf8(receipt.snapshot().to_json()?)?,
                "registered_asset":asset,"registered_project":serde_json::to_value(&registered)?,
                "checked_native_document_wire":wire,"native_provider":actual_native_contract,
                "fixture_path":"native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav",
                "qualification_scope":"actual persisted temporary registration; original qualifying native session and verified reopens"}))
    })? {
        return Ok(focused);
    }
    let mut cases = vec![observe_family(
        &plan,
        &base,
        "qualified_structural_source_hold",
        &mut reopen,
    )?];
    let mut setup_attempts = Vec::new();
    for (output, fit_beat) in [(false, false), (true, false), (true, true)] {
        let name = if fit_beat {
            "qualified_fitbeat_setup_attempt"
        } else if output {
            "qualified_source_output"
        } else {
            "qualified_source_input"
        };
        match qualified_projection(&plan, &base, output, fit_beat) {
            Ok(projection) => {
                setup_attempts.push(
                    json!({"case":name,"source_output":output,"fit_beat":fit_beat,
                    "status":"Ok","projection":trace::projection_value(&projection,&plan)}),
                );
                if !fit_beat {
                    cases.push(observe_family(&plan, &projection, name, &mut reopen)?);
                }
            }
            Err(e) => setup_attempts.push(
                json!({"case":name,"source_output":output,"fit_beat":fit_beat,
                "status":"Error","error":error(&StageAudioError::Plan(e))}),
            ),
        }
    }
    let mix = mixed_projection(&plan, &base);
    match mix {
        Ok(projection) => cases.push(observe_family(
            &plan,
            &projection,
            "qualified_mix_control",
            &mut reopen,
        )?),
        Err(e) => setup_attempts.push(json!({"case":"qualified_mix_control","status":"Error",
            "error":error(&StageAudioError::Plan(e))})),
    }
    let plain = plain_nested_plan(asset, &wire);
    let mut plain_depth_failure = Value::Null;
    match plain {
        Ok((plain_plan, plain_wire)) => {
            let root_query = plain_plan.audio_signal().query(
                SignalSample(0)..SignalSample(1),
                AudioQueryLimits::default(),
            );
            let query_value = match &root_query {
                Ok(q) => json!({"status":"Ok","query":trace::graph_value(q,&plain_plan)}),
                Err(e) => {
                    json!({"status":"Error", "error":{"kind":"Plan","display":e.to_string(),"debug":format!("{e:?}")}})
                }
            };
            match projection_for_plain(&plain_plan) {
                Ok(projection)=> {
                    setup_attempts.push(json!({"case":"plain_nonowned_nested_stage","status":"Ok",
                        "document":plain_wire,"root_query":query_value,
                        "projection":trace::projection_value(&projection,&plain_plan)}));
                    cases.push(observe_family(&plain_plan,&projection,"plain_nonowned_nested_stage",&mut reopen)?);
                    plain_depth_failure=prepare_action(&plain_plan,&projection,reopen()?,"plain_nested_depth_limit",
                        StageLimits {maximum_depth:1,..Default::default()},false,false);
                }
                Err(e)=>setup_attempts.push(json!({"case":"plain_nonowned_nested_stage","status":"Error",
                    "document":plain_wire,"root_query":query_value,"error":error(&StageAudioError::Plan(e))})),
            }
        }
        Err(e) => setup_attempts.push(setup_error("plain_nonowned_nested_stage", e.as_ref())),
    }
    let foreign_plan = Arc::new(RenderPlan::compile(&ProjectDocument::from_json(
        &wire.to_string(),
    )?)?);
    let failures = vec![
        prepare_action(
            &plan,
            &base,
            reopen()?,
            "input_frame_limit",
            StageLimits {
                maximum_input_frames: 639,
                ..Default::default()
            },
            false,
            false,
        ),
        prepare_action(
            &plan,
            &base,
            reopen()?,
            "output_frame_limit",
            StageLimits {
                maximum_output_frames: 767,
                ..Default::default()
            },
            false,
            false,
        ),
        prepare_action(
            &plan,
            &base,
            reopen()?,
            "cancelled_at_entry",
            StageLimits::default(),
            true,
            false,
        ),
        prepare_action(
            &plan,
            &base,
            reopen()?,
            "explicit_provider_refusal",
            StageLimits::default(),
            false,
            true,
        ),
        prepare_action(
            &foreign_plan,
            &base,
            reopen()?,
            "foreign_immutable_plan",
            StageLimits::default(),
            false,
            false,
        ),
        plain_depth_failure,
    ];
    Ok(
        json!({"setup":"Ok","qualification_scope":"real temporary ProjectStore registration; setup outside read ledger",
        "fixture_path":"native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav",
        "original_record":record,"registration":outcome,"receipt_id":receipt.id(),"receipt_original":receipt.original(),
        "qualification_snapshot":receipt.snapshot(),
        "qualification_snapshot_canonical_utf8":String::from_utf8(receipt.snapshot().to_json()?)?,
        "registered_asset":asset,"registered_project":serde_json::to_value(&registered)?,
        "checked_native_document_wire":wire,"native_provider":actual_native_contract,
        "setup_attempts":setup_attempts,"families":cases,"failure_attempts":failures}),
    )
}
fn observed_counts(value: &Value) -> Value {
    fn walk(value: &Value, counts: &mut BTreeMap<String, usize>) {
        match value {
            Value::Array(values) => {
                for value in values {
                    walk(value, counts);
                }
            }
            Value::Object(values) => {
                if let Some(phase) = values.get("phase").and_then(Value::as_str) {
                    if let Some(mode) = values
                        .get("value")
                        .and_then(|v| v.get("mode"))
                        .and_then(Value::as_str)
                    {
                        *counts.entry(format!("phase/{phase}/{mode}")).or_default() += 1;
                    }
                    if phase == "recursive_span_before"
                        && let Some(kind) = values
                            .get("value")
                            .and_then(|v| v.get("kind"))
                            .and_then(Value::as_str)
                    {
                        *counts.entry(format!("graph_dispatch/{kind}")).or_default() += 1;
                    }
                    if phase == "scope_enter"
                        && let Some(kind) = values.get("kind").and_then(Value::as_str)
                    {
                        *counts.entry(format!("scope_enter/{kind}")).or_default() += 1;
                    }
                }
                for value in values.values() {
                    walk(value, counts);
                }
            }
            _ => {}
        }
    }
    let mut counts = BTreeMap::new();
    walk(value, &mut counts);
    json!(counts)
}
fn capture_body() -> Value {
    let actual = match family() {
        Ok(value) => value,
        Err(e) => setup_error("registration_and_checked_fixture", e.as_ref()),
    };
    let counts = observed_counts(&actual);
    json!({"schema":"intrinsic-preflight-ordinary-before-v1","observed_counts":counts,"scope":{
        "selected":"ordinary stage_projection::preflight_intrinsic Signal/Tape/PreserveTape policies",
        "mix":"ordinary control, no future Mix admission claim",
        "observer_storage":"unadmitted JSON/test storage outside measured native ledger",
        "private_state_limits":["Plan private Bound constraints inaccessible from Audio dependency",
            "opaque memo identity bodies unavailable; actual equality/counts only",
            "private immutable Tape run inventory unavailable; actual constructed query values are complete public fields"],
        "failure_local_work":"failed constructor local canonical/additional unavailable; accepted native work captured independently",
        "gaps":["no seeded accepted budget or work-ceiling witness","actual setup/errors retained; intended variants are not assumed reached",
            "no admitted source lease or native custody transfer in this checkpoint"]},"actual":actual})
}
#[test]
#[ignore = "manual fresh ordinary before40 capture; parent freezes actual bytes later"]
fn capture_intrinsic_preflight_before_v1() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        capture_intrinsic_preflight_before_v1_historical,
    );
}

fn capture_intrinsic_preflight_before_v1_historical() {
    let output = std::env::var_os("DEADPAN_INTRINSIC_PREFLIGHT_BEFORE_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let variant = std::env::var("DEADPAN_INTRINSIC_PREFLIGHT_BEFORE_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_INTRINSIC_PREFLIGHT_BEFORE_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_INTRINSIC_PREFLIGHT_BEFORE_PUBLISHED_CHECKPOINT").unwrap(),
        BASE
    );
    let mut body = capture_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(BASE);
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
#[ignore = "external frozen actual body only; no placeholder before40 fixture"]
fn intrinsic_preflight_before_external_body_regression() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        intrinsic_preflight_before_external_body_regression_historical,
    );
}

fn intrinsic_preflight_before_external_body_regression_historical() {
    let path = std::env::var_os("DEADPAN_INTRINSIC_PREFLIGHT_BEFORE_EXPECTED").unwrap();
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(capture_body(), expected);
}

#[test]
fn intrinsic_preflight_frozen_before_body_is_exact() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        intrinsic_preflight_frozen_before_body_is_exact_historical,
    );
}

fn intrinsic_preflight_frozen_before_body_is_exact_historical() {
    let expected: Value = serde_json::from_slice(include_bytes!(
        "../tests/fixtures/intrinsic-preflight-policy-baseline-v1/corpus.json"
    ))
    .unwrap();
    assert_eq!(capture_body(), expected);
}

#[path = "intrinsic_preflight_policy_after_tests.rs"]
mod after;

#[path = "stage_validation_before_tests.rs"]
mod stage_validation;
