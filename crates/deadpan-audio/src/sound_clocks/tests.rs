use super::*;

use deadpan_core::{
    AssetId, AssetRecord, AudioBindingState, AudioEdgePolicy, AudioSample, AudioTimingId,
    AudioTimingRecord, BeatNode, BeatSound, ColorPolicy, FrameDuration, FrameRate,
    FrozenAudioLayout, HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis,
    ProjectDocument, ProjectId, RevisionId, SoundClockJournal, SoundClockReference, SoundId,
    SoundOverflowPolicy, SourceAudio, SourceAudioMapping, SourceQualificationId, SourceSpan,
    SourceTimeBase, SourceTimestamp,
};
use std::{
    cell::RefCell,
    sync::Arc,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn source_span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 10,
            time_base,
        },
        SourceTimestamp {
            ticks: 4_810,
            time_base,
        },
    )
    .unwrap()
}

fn asset(label: &str) -> AssetRecord {
    AssetRecord {
        label: label.into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(source_span()),
        still_image: false,
        frame_count: None,
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    }
}

fn beat_sound(label: &str, asset: &str) -> BeatSound {
    let source = SourceAudio {
        asset: AssetId::new(asset).unwrap(),
        span: source_span(),
    };
    BeatSound {
        label: label.into(),
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            FrameRate::new(30_000, 1_001).unwrap(),
        )
        .unwrap(),
        source,
        offset: AudioSample(0),
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Automatic,
        overflow: SoundOverflowPolicy::Reject,
    }
}

fn document(event_assets: &[&str], clocks_per_event: usize) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1_001).unwrap();
    let owner = id("owner");
    let root = id("root");
    let document = ProjectDocument::new(
        ProjectId::new("sound-clock-cache").unwrap(),
        revision("live"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )
    .unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (root, BeatNode::sequence("Root", vec![owner.clone()])),
        (
            owner.clone(),
            BeatNode::hold(
                "Owner",
                HoldRecipe {
                    duration: FrameDuration::new(100).unwrap(),
                    video: HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(
        event_assets
            .iter()
            .map(|name| (AssetId::new((*name).to_owned()).unwrap(), asset(name)))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        owner.clone(),
        event_assets
            .iter()
            .enumerate()
            .map(|(index, asset_name)| {
                let sound = SoundId::new(format!("sound-{index}")).unwrap();
                (sound, beat_sound(&format!("Sound {index}"), asset_name))
            })
            .collect::<BTreeMap<_, _>>(),
    )]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let layout = FrozenAudioLayout::capture(&document).unwrap();

    let mut timing_records = Vec::new();
    let mut clocks_by_sound = BTreeMap::new();
    for (event_index, _) in event_assets.iter().enumerate() {
        let sound = SoundId::new(format!("sound-{event_index}")).unwrap();
        let mut clocks = Vec::with_capacity(clocks_per_event);
        for ordinal in 0..clocks_per_event {
            let timing = AudioTimingId {
                allocation: revision(&format!("timing-{event_index}-{ordinal}")),
                ordinal: 0,
            };
            clocks.push(SoundClockReference::new(
                timing.clone(),
                owner.clone(),
                owner.clone(),
            ));
            timing_records.push(AudioTimingRecord {
                id: timing,
                layout: layout.clone(),
            });
        }
        clocks_by_sound.insert(
            sound,
            SoundClockJournal::new(owner.clone(), clocks).unwrap(),
        );
    }
    let bindings = AudioBindingState::new_with_sound_clocks(
        timing_records,
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::from([(owner, clocks_by_sound)]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(5),
        work,
    }
}

fn prepare(
    stage: &mut StageAudio,
    plan: &RenderPlan,
) -> Result<SoundProcessingPlans, StageAudioError> {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    stage.prepare_sound_processing_plans(plan, control(&cancelled, &work))
}

#[test]
fn one_sound_reuses_its_first_clock_plan_for_long_journal_and_retry() {
    let document = document(&["asset-a"], 80);
    let plan = RenderPlan::compile(&document).unwrap();
    let mut stage = StageAudio::new(Arc::new(plan.clone()));

    let prepared = prepare(&mut stage, &plan).unwrap();
    assert_eq!(prepared.len(), 1);
    assert_eq!(stage.sound_processing_plans.len(), 1);
    let first = prepared.values().next().unwrap();
    assert_eq!(first.audio_context_assets().unwrap().len(), 1);
    assert!(
        first
            .audio_context_assets()
            .unwrap()
            .contains_key(&AssetId::new("asset-a").unwrap())
    );

    let repeated = prepare(&mut stage, &plan).unwrap();
    assert_eq!(repeated.len(), 1);
    assert_eq!(stage.sound_processing_plans.len(), 1);
    assert!(Arc::ptr_eq(repeated.values().next().unwrap(), first));

    let mut limited_stage = StageAudio::new(Arc::new(plan.clone()));
    let cancelled = AtomicBool::new(false);
    let limited_work = RefCell::new(ReadWork {
        plan_work: MAX_PLAN_WORK_PER_READ - 1,
        ..Default::default()
    });
    assert!(matches!(
        limited_stage.prepare_sound_processing_plans(&plan, control(&cancelled, &limited_work)),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert!(limited_stage.sound_processing_plans.is_empty());

    assert_eq!(prepare(&mut limited_stage, &plan).unwrap().len(), 1);
    assert_eq!(limited_stage.sound_processing_plans.len(), 1);
}

#[test]
fn distinct_first_clocks_keep_each_sound_asset_contract_separate() {
    let document = document(&["asset-a", "asset-b"], 80);
    let plan = RenderPlan::compile(&document).unwrap();
    let mut stage = StageAudio::new(Arc::new(plan.clone()));
    let prepared = prepare(&mut stage, &plan).unwrap();

    assert_eq!(prepared.len(), 2);
    assert_eq!(stage.sound_processing_plans.len(), 2);
    let retained: Vec<_> = stage.sound_processing_plans.iter().collect();
    assert_ne!(retained[0].0, retained[1].0);
    for (_, plan) in retained {
        let assets = plan.audio_context_assets().unwrap();
        assert_eq!(assets.len(), 1);
        assert!(
            assets.contains_key(&AssetId::new("asset-a").unwrap())
                || assets.contains_key(&AssetId::new("asset-b").unwrap())
        );
    }
    assert_eq!(
        stage
            .sound_processing_plans
            .values()
            .map(|plan| plan
                .audio_context_assets()
                .unwrap()
                .keys()
                .next()
                .unwrap()
                .clone())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            AssetId::new("asset-a").unwrap(),
            AssetId::new("asset-b").unwrap()
        ])
    );
    for (index, asset_name) in ["asset-a", "asset-b"].iter().enumerate() {
        let sound = SoundId::new(format!("sound-{index}")).unwrap();
        let clocks = plan.beat_sound_clock_layouts(&id("owner"), &sound).unwrap();
        let (first_clock, _) = clocks.first().unwrap();
        let retained = &prepared[*first_clock];
        assert_eq!(
            retained
                .audio_context_assets()
                .unwrap()
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([AssetId::new((*asset_name).to_owned()).unwrap()]),
        );
    }
}
