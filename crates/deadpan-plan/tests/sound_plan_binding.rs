//! Shared immutable layout admission keeps recipe and media authority checks.

use std::collections::{BTreeMap, BTreeSet};

use deadpan_core::*;
use deadpan_plan::{AudioSourceVoiceRecipe, PlanError, RenderPlan};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn asset_id(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn sound() -> SoundId {
    SoundId::new("event").unwrap()
}

fn span(end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn hold() -> BeatNode {
    BeatNode::hold(
        "Silent owner",
        HoldRecipe {
            duration: FrameDuration::new(4).unwrap(),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

fn fixture(padding: usize) -> ProjectDocument {
    let rate = FrameRate::new(30, 1).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("sound-plan-binding").unwrap(),
        revision("live"),
        PresentationBasis {
            width: 64,
            height: 64,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut children = vec![id("owner")];
    children.extend((0..padding).map(|index| id(&format!("padding-{index}"))));
    let mut nodes = children
        .iter()
        .map(|child| (child.clone(), hold()))
        .collect::<BTreeMap<_, _>>();
    nodes.insert(id("scope"), BeatNode::sequence("Scope", children));
    nodes.insert(id("root"), BeatNode::sequence("Root", vec![id("scope")]));
    let record = AssetRecord {
        label: "Sound contract".into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(span(48_000)),
        still_image: false,
        frame_count: None,
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    };
    let source = SourceAudio {
        asset: asset_id("sound"),
        span: span(1024),
    };
    let event = BeatSound {
        label: "Independent event".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
        source,
        offset: AudioSample(0),
        treatments: Default::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([
        (asset_id("sound"), record.clone()),
        (asset_id("other"), record),
    ]))
    .unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        id("owner"),
        BTreeMap::from([(sound(), event)]),
    )]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let timing = AudioTimingId {
        allocation: revision("capture"),
        ordinal: 0,
    };
    let bindings = AudioBindingState::new_with_sound_clocks(
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
                    id("scope"),
                    vec![SoundClockReference::new(timing, id("scope"), id("owner"))],
                )
                .unwrap(),
            )]),
        )]),
    )
    .unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn selected() -> BTreeSet<AssetId> {
    BTreeSet::from([asset_id("sound")])
}

#[test]
fn shared_sound_plan_binding_skips_full_layout_work_but_detached_equal_layout_does_not() {
    let plan = RenderPlan::compile(&fixture(32)).unwrap();
    let scopes = plan
        .beat_sound_clock_scopes(&id("owner"), &sound(), MAX_DOCUMENT_NODES)
        .unwrap();
    let scope = &scopes[0];
    let layout = scope.historical_layout();
    let shared = plan
        .compile_sound_processing_layout(layout, &selected())
        .unwrap();
    let detached = FrozenAudioLayout::from_json(&layout.to_json().unwrap()).unwrap();
    assert_eq!(&detached, layout);
    assert!(!std::ptr::eq(&*detached, &**layout));
    let separate = plan
        .compile_sound_processing_layout(&detached, &selected())
        .unwrap();
    let old_work = RenderPlan::sound_processing_layout_work(layout, &selected()).unwrap() * 2;
    let (_, shared_work) = scope
        .bind_historical_plan(&shared, MAX_DOCUMENT_NODES)
        .unwrap();
    let (_, separate_work) = scope
        .bind_historical_plan(&separate, MAX_DOCUMENT_NODES)
        .unwrap();
    assert_eq!(shared_work, 15);
    assert_eq!(separate_work, old_work + 3);
    assert!(shared_work < old_work);
    assert!(scope.bind_historical_plan(&shared, shared_work).is_ok());
    assert!(matches!(
        scope.bind_historical_plan(&shared, shared_work - 1),
        Err(PlanError::AudioQueryLimit("sound plan binding work"))
    ));
    assert!(matches!(
        scope.bind_historical_plan(&separate, shared_work),
        Err(PlanError::AudioQueryLimit("sound plan binding work"))
    ));
    assert!(scope.bind_historical_plan(&separate, separate_work).is_ok());
    assert!(scope.bind_historical_plan(&shared, 0).is_err());
}

#[test]
fn small_shared_sound_plan_binding_keeps_the_cheaper_full_execution() {
    let plan = RenderPlan::compile(&fixture(0)).unwrap();
    let scopes = plan
        .beat_sound_clock_scopes(&id("owner"), &sound(), MAX_DOCUMENT_NODES)
        .unwrap();
    let layout = scopes[0].historical_layout();
    assert_eq!(layout.nodes().len(), 3);
    let retained = plan
        .compile_sound_processing_layout(layout, &selected())
        .unwrap();
    let old_work = RenderPlan::sound_processing_layout_work(layout, &selected()).unwrap() * 2;
    let (_, work) = scopes[0]
        .bind_historical_plan(&retained, MAX_DOCUMENT_NODES)
        .unwrap();
    assert_eq!(old_work, 12);
    assert_eq!(work, old_work + 1);
    assert!(work < 15);
}

#[test]
fn shared_sound_plan_binding_still_checks_selected_asset_contracts() {
    let document = fixture(32);
    let plan = RenderPlan::compile(&document).unwrap();
    let scopes = plan
        .beat_sound_clock_scopes(&id("owner"), &sound(), MAX_DOCUMENT_NODES)
        .unwrap();
    let scope = &scopes[0];
    let missing = plan
        .compile_sound_processing_layout(
            scope.historical_layout(),
            &BTreeSet::from([asset_id("other")]),
        )
        .unwrap();
    assert!(
        scope
            .bind_historical_plan(&missing, MAX_DOCUMENT_NODES)
            .is_err()
    );
    for field in ["label", "content_hash", "source_qualification", "audio"] {
        let mut wire = serde_json::to_value(&document).unwrap();
        match field {
            "label" => wire["assets"]["sound"]["label"] = serde_json::json!("Changed contract"),
            "content_hash" => {
                wire["assets"]["sound"]["content_hash"] = serde_json::json!("c".repeat(64))
            }
            "source_qualification" => {
                wire["assets"]["sound"]["source_qualification"] = serde_json::json!("d".repeat(64))
            }
            "audio" => {
                wire["assets"]["sound"]["audio"] = serde_json::to_value(span(24_000)).unwrap()
            }
            _ => unreachable!(),
        }
        let foreign =
            RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
        // The exact historical allocation is deliberately shared. Only the
        // selected catalog contract differs, so layout identity cannot admit it.
        let changed = foreign
            .compile_sound_processing_layout(scope.historical_layout(), &selected())
            .unwrap();
        assert!(
            scope
                .bind_historical_plan(&changed, MAX_DOCUMENT_NODES)
                .is_err(),
            "{field}"
        );
    }
}

#[test]
fn shared_sound_plan_binding_keeps_alias_authority_on_exact_plan_handles() {
    let plan = RenderPlan::compile(&fixture(32)).unwrap();
    let scopes = plan
        .beat_sound_clock_scopes(&id("owner"), &sound(), MAX_DOCUMENT_NODES)
        .unwrap();
    let scope = &scopes[0];
    let retained = plan
        .compile_sound_processing_layout(scope.historical_layout(), &selected())
        .unwrap();
    let event = &plan.beat_sounds()[&id("owner")][&sound()];
    let recipe = AudioSourceVoiceRecipe {
        source: event.source.clone(),
        mapping: event.mapping,
        offset: event.offset,
    };
    let instance = InstancePath {
        node: id("owner"),
        repeats: Vec::new(),
    };
    let current = plan
        .source_voice_occurrence(instance.clone(), recipe.clone(), Default::default())
        .unwrap();
    let original = retained
        .source_voice_occurrence(instance.clone(), recipe.clone(), Default::default())
        .unwrap();
    let (binding, _) = scope
        .bind_historical_plan(&retained, MAX_DOCUMENT_NODES)
        .unwrap();
    assert!(
        binding
            .alias_occurrence(&original, &current, MAX_DOCUMENT_NODES)
            .is_ok()
    );
    let retained_clone = retained.clone();
    let foreign_original = retained_clone
        .source_voice_occurrence(instance.clone(), recipe.clone(), Default::default())
        .unwrap();
    assert!(
        binding
            .alias_occurrence(&foreign_original, &current, MAX_DOCUMENT_NODES)
            .is_err()
    );
    let plan_clone = plan.clone();
    let foreign_current = plan_clone
        .source_voice_occurrence(instance, recipe, Default::default())
        .unwrap();
    assert!(
        binding
            .alias_occurrence(&original, &foreign_current, MAX_DOCUMENT_NODES)
            .is_err()
    );
}

#[test]
fn derived_clock_layouts_remain_unsupported_for_sound_processing_plans() {
    let plan = RenderPlan::compile(&fixture(32)).unwrap();
    let scopes = plan
        .beat_sound_clock_scopes(&id("owner"), &sound(), MAX_DOCUMENT_NODES)
        .unwrap();
    let mut wire = serde_json::to_value(scopes[0].historical_layout()).unwrap();
    wire["nodes"]["derived-leaf"] = wire["nodes"]["owner"].clone();
    wire["nodes"]["owner"]["kind"] = serde_json::to_value(FrozenAudioKind::Retime {
        child: id("derived-leaf"),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Partition,
    })
    .unwrap();
    wire["derived_clocks"] = serde_json::json!({
        "owner": {
            "extent": ExactRatio::integer(4),
            "child_start": ExactRatio::ZERO,
            "child_end": ExactRatio::integer(4),
        },
    });
    let derived = FrozenAudioLayout::from_json(&wire.to_string()).unwrap();
    assert!(matches!(
        RenderPlan::sound_processing_layout_work(&derived, &selected()),
        Err(PlanError::InvalidPlan(
            "exact historical clocks need an owned output projection"
        ))
    ));
    // Public constructors cannot produce the compiled plan required to enter
    // a binding's same-storage branch for this unsupported clock capability.
    assert!(matches!(
        plan.compile_sound_processing_layout(&derived, &selected()),
        Err(PlanError::InvalidPlan(
            "exact historical clocks need an owned output projection"
        ))
    ));
}
