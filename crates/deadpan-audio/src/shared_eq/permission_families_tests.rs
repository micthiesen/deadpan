use super::*;
use deadpan_core::*;
use deadpan_plan::{AudioQueryLimits, AudioSourceVoiceRecipe};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn sound() -> SoundId {
    SoundId::new("event").unwrap()
}

fn fixture() -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("canonical-families").unwrap(),
        revision("before"),
        PresentationBasis {
            width: 64,
            height: 64,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("scope")])),
        (id("scope"), BeatNode::sequence("Scope", vec![id("owner")])),
        (
            id("owner"),
            BeatNode::hold(
                "Silent picture",
                HoldRecipe {
                    duration: FrameDuration::new(4).unwrap(),
                    video: HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))
    .unwrap();
    let base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: base,
        },
        SourceTimestamp {
            ticks: 1024,
            time_base: base,
        },
    )
    .unwrap();
    let asset = AssetId::new("sound").unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset.clone(),
        AssetRecord {
            label: "Sound".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        id("owner"),
        BTreeMap::from([(
            sound(),
            BeatSound {
                label: "Event".into(),
                source: SourceAudio { asset, span },
                mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
                offset: AudioSample(0),
                treatments: AudioTreatments::default(),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]),
    )]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let timing = AudioTimingId {
        allocation: revision("capture"),
        ordinal: 0,
    };
    wire["audio_bindings"] = serde_json::to_value(
        AudioBindingState::new_with_sound_clocks(
            vec![AudioTimingRecord {
                id: timing.clone(),
                layout: FrozenAudioLayout::capture(&before).unwrap(),
            }],
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::from([(
                id("owner"),
                BTreeMap::from([(
                    sound(),
                    SoundClockJournal::new(
                        id("owner"),
                        vec![SoundClockReference::new(timing, id("owner"), id("owner"))],
                    )
                    .unwrap(),
                )]),
            )]),
        )
        .unwrap(),
    )
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let result = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("split"),
            command: Command::Split {
                node: id("owner"),
                at: FrameDuration::new(2).unwrap(),
                identities: SplitIdentities {
                    nodes: ["left", "right", "copy"].map(id).to_vec(),
                },
            },
        },
    )
    .unwrap();
    result.forward.apply(&before).unwrap()
}

fn new_graph<'plan, 'work>(plan: &'plan RenderPlan) -> Graph<'plan, 'work> {
    Graph {
        plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        wet_filters: std::array::from_fn(|_| None),
        event_tiles: BTreeMap::new(),
        event_nodes: Default::default(),
        event_filters: BTreeMap::new(),
        voices: BTreeMap::new(),
        permission_families: BTreeMap::new(),
        selected_family_count: 0,
        canonical_families: FamilyRegistry::default(),
        sends: None,
        skip: Vec::new(),
        original_only: false,
        original_inputs: original_inputs::Cache::default(),
    }
}

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

fn context<'plan>(plan: &'plan RenderPlan, node: &str) -> AudioSoundOwnerContext<'plan> {
    plan.audio_sound_instance_context(
        &InstancePath {
            node: id(node),
            repeats: vec![],
        },
        AudioQueryLimits::default(),
    )
    .unwrap()
}

fn family<'plan>(
    plan: &'plan RenderPlan,
    context: &AudioSoundOwnerContext<'plan>,
    owner: &str,
    repeats: Vec<RepeatInstance>,
) -> AudioSoundPermissionFamily<'plan> {
    let owner = id(owner);
    let event = &plan.beat_sounds()[&owner][&sound()];
    let occurrence = context
        .source_voice_occurrence(
            InstancePath {
                node: owner.clone(),
                repeats,
            },
            AudioSourceVoiceRecipe {
                source: event.source.clone(),
                mapping: event.mapping,
                offset: event.offset,
            },
            AudioQueryLimits::default(),
        )
        .unwrap();
    occurrence
        .split_permission_family(
            &plan
                .beat_sound_clock_scopes(&owner, &sound(), MAX_DOCUMENT_NODES)
                .unwrap(),
            context,
            AudioQueryLimits::default(),
        )
        .unwrap()
        .0
        .unwrap()
}

#[test]
fn explicit_family_tokens_alias_only_after_complete_equality_in_one_graph() {
    let plan = Arc::new(RenderPlan::compile(&fixture()).unwrap());
    let root = context(&plan, "root");
    let scope = context(&plan, "scope");
    let a = family(&plan, &root, "owner", vec![]);
    let b = family(&plan, &root, "copy", vec![]);
    assert_eq!(a, b, "fixture must establish a real complete Split family");
    let other_context = family(&plan, &scope, "owner", vec![]);
    assert_ne!(a, other_context);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut graph = new_graph(&plan);
    let a = renderer
        .intern_permission_family(&mut graph, &a, control)
        .unwrap();
    let b = renderer
        .intern_permission_family(&mut graph, &b, control)
        .unwrap();
    assert!(a.same(&b));
    assert_eq!(graph.canonical_families.entries.len(), 1);
    let different = renderer
        .intern_permission_family(&mut graph, &other_context, control)
        .unwrap();
    assert!(!a.same(&different));
    let mut other_graph = new_graph(&plan);
    let other = renderer
        .intern_permission_family(
            &mut other_graph,
            &family(&plan, &root, "owner", vec![]),
            control,
        )
        .unwrap();
    assert!(
        !a.same(&other),
        "tokens from distinct registries are conservative"
    );
}

#[test]
fn forests_and_concrete_repeat_plays_remain_distinct_when_interned() {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    for (old, new) in [
        ("left", "other-left"),
        ("right", "other-right"),
        ("owner", "other-owner"),
        ("copy", "other-copy"),
    ] {
        wire["nodes"][new] = wire["nodes"][old].clone();
    }
    wire["nodes"]["other-left"]["kind"]["child"] = serde_json::json!("other-owner");
    wire["nodes"]["other-right"]["kind"]["child"] = serde_json::json!("other-copy");
    for (old, new) in [("owner", "other-owner"), ("copy", "other-copy")] {
        wire["audio_lineage"][new] = wire["audio_lineage"][old].clone();
        wire["beat_sounds"][new] = wire["beat_sounds"][old].clone();
        wire["audio_bindings"]["sound_clocks"][new] =
            wire["audio_bindings"]["sound_clocks"][old].clone();
        wire["audio_bindings"]["sound_clocks"][new]["event"]["scope"] = serde_json::json!(new);
    }
    wire["nodes"]["scope"]["kind"]["children"]
        .as_array_mut()
        .unwrap()
        .extend([
            serde_json::json!("other-left"),
            serde_json::json!("other-right"),
        ]);
    let iterations = IterationOrder::new(revision("plays"), 2).unwrap();
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("scope"),
        iterations: iterations.clone(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["plays"] = serde_json::to_value(repeat).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["plays"]);
    let map = SoundClockRepeatMap::new(vec![SoundClockRepeatStep::Introduced {
        live_repeat: id("plays"),
        plays: iterations,
    }])
    .unwrap();
    for events in wire["audio_bindings"]["sound_clocks"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        for journal in events.as_object_mut().unwrap().values_mut() {
            for clock in journal["clocks"].as_array_mut().unwrap() {
                clock["repeats"] = serde_json::to_value(&map).unwrap();
            }
        }
    }
    let plan = Arc::new(
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap(),
    );
    let root = context(&plan, "root");
    let repeats = |ordinal| {
        vec![RepeatInstance {
            node: id("plays"),
            iteration: IterationId {
                allocation: revision("plays"),
                ordinal,
            },
        }]
    };
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut graph = new_graph(&plan);
    let a = renderer
        .intern_permission_family(
            &mut graph,
            &family(&plan, &root, "owner", repeats(0)),
            control,
        )
        .unwrap();
    let b = renderer
        .intern_permission_family(
            &mut graph,
            &family(&plan, &root, "copy", repeats(0)),
            control,
        )
        .unwrap();
    let other_play = renderer
        .intern_permission_family(
            &mut graph,
            &family(&plan, &root, "copy", repeats(1)),
            control,
        )
        .unwrap();
    let other_forest = renderer
        .intern_permission_family(
            &mut graph,
            &family(&plan, &root, "other-owner", repeats(0)),
            control,
        )
        .unwrap();
    assert!(a.same(&b));
    assert!(!a.same(&other_play));
    assert!(!a.same(&other_forest));
    assert_eq!(graph.canonical_families.entries.len(), 3);
    let mut selected = Permissions::default();
    selected.insert(0).unwrap();
    let aliases = split_family_aliases(
        &[(0, a), (1, b), (2, other_play), (3, other_forest)],
        selected,
        control,
    )
    .unwrap();
    assert!(!aliases.contains(0));
    assert!(aliases.contains(1));
    assert!(
        !aliases.contains(2),
        "another Repeat play cannot be aliased"
    );
    assert!(
        !aliases.contains(3),
        "another Split forest cannot be aliased"
    );
}

#[test]
fn selected_family_retrieval_and_alias_masks_use_constant_handle_work() {
    let plan = Arc::new(RenderPlan::compile(&fixture()).unwrap());
    let root = context(&plan, "root");
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut graph = new_graph(&plan);
    let a = graph
        .voice(Voice::Beat(
            id("owner"),
            sound(),
            InstancePath {
                node: id("owner"),
                repeats: vec![],
            },
        ))
        .unwrap();
    let b = graph
        .voice(Voice::Beat(
            id("copy"),
            sound(),
            InstancePath {
                node: id("copy"),
                repeats: vec![],
            },
        ))
        .unwrap();
    let mut selected = Permissions::default();
    selected.insert(a).unwrap();
    let first = renderer
        .selected_split_families(&mut graph, &root, selected, control)
        .unwrap();
    assert_eq!(first.len(), 1);
    let active_b = renderer
        .intern_permission_family(&mut graph, &family(&plan, &root, "copy", vec![]), control)
        .unwrap();
    let resident = graph.resident;
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 16;
    let again = renderer
        .selected_split_families(&mut graph, &root, selected, control)
        .unwrap();
    assert!(first[0].1.same(&again[0].1));
    assert_eq!(graph.resident, resident);
    let families = [(a, again[0].1.clone()), (b, active_b)];
    let aliases = split_family_aliases(&families, selected, control).unwrap();
    assert!(!aliases.contains(a));
    assert!(aliases.contains(b));
    let mut other = Permissions::default();
    other.insert(b).unwrap();
    let aliases = split_family_aliases(&families, other, control).unwrap();
    assert!(aliases.contains(a));
    assert!(!aliases.contains(b));
    assert!(
        split_family_aliases(&families, Permissions::default(), control)
            .unwrap()
            .is_empty()
    );
    assert_eq!(graph.voices.len(), 2);
    assert_eq!(graph.selected_family_count, 1);
}

#[test]
fn canonical_admission_refuses_budget_cancellation_and_resident_exhaustion() {
    let plan = Arc::new(RenderPlan::compile(&fixture()).unwrap());
    let root = context(&plan, "root");
    let family = family(&plan, &root, "owner", vec![]);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut graph = new_graph(&plan);
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ;
    assert!(matches!(
        renderer.intern_permission_family(&mut graph, &family, control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert!(graph.canonical_families.entries.is_empty());
    assert_eq!(renderer.active_frames, 0);
    work.borrow_mut().plan_work = 0;
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        renderer
            .intern_permission_family(&mut graph, &family, control)
            .is_err()
    );
    assert!(graph.canonical_families.entries.is_empty());
    cancelled.store(false, std::sync::atomic::Ordering::Relaxed);
    renderer.active_frames = u64::from(renderer.limits.maximum_resident_frames);
    let before = renderer.active_frames;
    assert!(matches!(
        renderer.intern_permission_family(&mut graph, &family, control),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(renderer.active_frames, before);
    assert_eq!(graph.resident, 0);
    assert_eq!(graph.canonical_families.entries.capacity(), 0);
    renderer.active_frames = 0;
    renderer
        .intern_permission_family(&mut graph, &family, control)
        .unwrap();
    assert!(graph.resident > 0);
    assert_eq!(renderer.active_frames, graph.resident);
    let resident = graph.resident;
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - family.comparison_work();
    assert!(
        matches!(
            renderer.intern_permission_family(&mut graph, &family, control),
            Err(StageAudioError::Limit("plan work per read"))
        ),
        "an independently supplied equal family must pay the complete typed Eq"
    );
    assert_eq!(graph.resident, resident);
    assert_eq!(graph.canonical_families.entries.len(), 1);
    // Match the owning read's existing error/success cleanup.
    renderer.active_frames -= graph.resident;
    drop(graph);
    assert_eq!(renderer.active_frames, 0);
}
