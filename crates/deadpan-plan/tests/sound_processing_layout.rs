use std::collections::{BTreeMap, BTreeSet};

use deadpan_core::*;
use deadpan_plan::{AudioSignalContent, RenderPlan};

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
            ticks: 48_010,
            time_base,
        },
    )
    .unwrap()
}

fn asset(label: &str, qualified: bool) -> AssetRecord {
    AssetRecord {
        label: label.into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(source_span()),
        still_image: false,
        frame_count: None,
        source_qualification: qualified
            .then(|| SourceQualificationId::new("b".repeat(64)).unwrap()),
    }
}

fn sequence(label: &str, children: &[&str]) -> BeatNode {
    BeatNode::sequence(label, children.iter().map(|child| id(child)).collect())
}

fn hold(label: &str, frames: i64) -> BeatNode {
    BeatNode::hold(
        label,
        HoldRecipe {
            duration: FrameDuration::new(frames).unwrap(),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

fn preserve(label: &str, child: &str, output: i64, input: i64) -> BeatNode {
    BeatNode {
        label: label.into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: FrameDuration::new(output).unwrap(),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
    }
}

fn repeated(child: &str, allocation: &str, count: u32) -> BeatNode {
    BeatNode {
        label: allocation.into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(revision(allocation), count).unwrap(),
            gap: None,
        },
    }
}

fn document(rate: FrameRate) -> ProjectDocument {
    let mut wire = serde_json::to_value(
        ProjectDocument::new(
            ProjectId::new("sound-processing-layout").unwrap(),
            revision("live"),
            PresentationBasis {
                width: 64,
                height: 64,
                frame_rate: rate,
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap(),
    )
    .unwrap();
    let nodes = BTreeMap::from([
        (id("root"), sequence("Root", &["outer_repeat"])),
        (
            id("outer_repeat"),
            repeated("outer_stage", "outer-plays", 2),
        ),
        (
            id("outer_stage"),
            preserve("Outer Preserve", "outer_seq", 4, 8),
        ),
        (id("outer_seq"), sequence("Outer input", &["inner_repeat"])),
        (
            id("inner_repeat"),
            repeated("inner_stage", "inner-plays", 4),
        ),
        (
            id("inner_stage"),
            preserve("Inner Preserve", "inner_seq", 2, 4),
        ),
        (
            id("inner_seq"),
            sequence("Inner input", &["owner_a", "owner_b"]),
        ),
        (id("owner_a"), hold("Owner A", 2)),
        (id("owner_b"), hold("Owner B", 2)),
        (
            id("alt_stage"),
            preserve("Override Preserve", "alt_seq", 4, 8),
        ),
        (id("alt_seq"), sequence("Override input", &["alt_repeat"])),
        (
            id("alt_repeat"),
            repeated("alt_inner_stage", "alt-inner-plays", 4),
        ),
        (
            id("alt_inner_stage"),
            preserve("Override inner Preserve", "alt_inner_seq", 2, 4),
        ),
        (
            id("alt_inner_seq"),
            sequence("Override inner input", &["alt_owner_a", "alt_owner_b"]),
        ),
        (id("alt_owner_a"), hold("Override owner A", 2)),
        (id("alt_owner_b"), hold("Override owner B", 2)),
    ]);
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("outer_repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: IterationId {
                allocation: revision("outer-plays"),
                ordinal: 1,
            },
            root: id("alt_stage"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([
        (AssetId::new("sound").unwrap(), asset("sound", true)),
        (AssetId::new("other").unwrap(), asset("other", true)),
        (
            AssetId::new("unqualified").unwrap(),
            asset("unqualified", false),
        ),
    ]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn sound_recipe(rate: FrameRate) -> (SourceAudio, SourceAudioMapping) {
    let source = SourceAudio {
        asset: AssetId::new("sound").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 10,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
            SourceTimestamp {
                ticks: 3_210,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
        )
        .unwrap(),
    };
    let mapping = SourceAudioMapping::natural_rate(source.span, rate).unwrap();
    (source, mapping)
}

fn document_with_sound_clock(
    rate: FrameRate,
) -> (ProjectDocument, AudioTimingId, FrozenAudioLayout) {
    let document = document(rate);
    let (source, mapping) = sound_recipe(rate);
    let sound_id = SoundId::new("event").unwrap();
    let owner = id("owner_a");
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        owner.clone(),
        BTreeMap::from([(
            sound_id.clone(),
            BeatSound {
                label: "event".into(),
                source,
                mapping,
                offset: AudioSample(0),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]),
    )]))
    .unwrap();
    let with_sound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let layout = FrozenAudioLayout::capture(&with_sound).unwrap();
    let timing = AudioTimingId {
        allocation: revision("clock-capture"),
        ordinal: 0,
    };
    let state = AudioBindingState::new_with_sound_clocks(
        vec![AudioTimingRecord {
            id: timing.clone(),
            layout: layout.clone(),
        }],
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::from([(
            owner,
            BTreeMap::from([(
                sound_id,
                SoundClockJournal::new(
                    id("outer_repeat"),
                    vec![SoundClockReference::new(
                        timing.clone(),
                        id("outer_repeat"),
                        id("owner_a"),
                    )],
                )
                .unwrap(),
            )]),
        )]),
    )
    .unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        timing,
        layout,
    )
}

fn play(allocation: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision(allocation),
        ordinal,
    }
}

#[test]
fn sound_processing_plan_resolves_nested_repeat_override_and_preserve_history() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = document(rate);
    let live = RenderPlan::compile(&document).unwrap();
    let layout = FrozenAudioLayout::capture(&document).unwrap();
    let plan = live
        .compile_sound_processing_layout(&layout, &BTreeSet::from([AssetId::new("sound").unwrap()]))
        .unwrap();

    assert!(
        live.beat_sound_clock_layouts(&id("owner_a"), &SoundId::new("event").unwrap())
            .unwrap()
            .is_empty()
    );

    assert_eq!(plan.metadata().project_id, live.metadata().project_id);
    assert_eq!(plan.metadata().revision_id, live.metadata().revision_id);
    assert_eq!(
        plan.audio_context_assets()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([AssetId::new("sound").unwrap()])
    );
    assert!(!plan.has_audio_treatments());
    assert!(plan.picture(ProjectFrame(0)).is_err());

    let (source, mapping) = sound_recipe(rate);
    let occurrence = plan
        .source_voice_occurrence(
            InstancePath {
                node: id("alt_owner_a"),
                repeats: vec![
                    RepeatInstance {
                        node: id("outer_repeat"),
                        iteration: play("outer-plays", 1),
                    },
                    RepeatInstance {
                        node: id("alt_repeat"),
                        iteration: play("alt-inner-plays", 0),
                    },
                ],
            },
            deadpan_plan::AudioSourceVoiceRecipe {
                source,
                mapping,
                offset: AudioSample(0),
            },
            Default::default(),
        )
        .unwrap();
    assert_eq!(occurrence.instance().node, id("alt_owner_a"));
    assert_eq!(occurrence.instance().repeats.len(), 2);
    assert!(occurrence.processing_projection().is_some());
    let processing = occurrence
        .processing(occurrence.samples(), Default::default())
        .unwrap();
    assert!(matches!(
        processing.spans.first().map(|span| &span.content),
        Some(AudioSignalContent::ProjectedStage(_))
    ));
}

#[test]
fn beat_sound_clock_accessor_returns_borrowed_layouts_in_journal_order() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let (document, timing, expected) = document_with_sound_clock(rate);
    let plan = RenderPlan::compile(&document).unwrap();
    let clocks = plan
        .beat_sound_clock_layouts(&id("owner_a"), &SoundId::new("event").unwrap())
        .unwrap();
    assert_eq!(clocks.len(), 1);
    assert_eq!(clocks[0].0, &timing);
    assert_eq!(clocks[0].1, &expected);
}

#[test]
fn sound_clock_scope_mints_only_recipe_matched_nested_occurrence_aliases() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let (document, _, layout) = document_with_sound_clock(rate);
    let plan = RenderPlan::compile(&document).unwrap();
    let sound = SoundId::new("event").unwrap();
    let event = &plan.beat_sounds()[&id("owner_a")][&sound];
    let recipe = deadpan_plan::AudioSourceVoiceRecipe {
        source: event.source.clone(),
        mapping: event.mapping,
        offset: event.offset,
    };
    let instance = InstancePath {
        node: id("owner_a"),
        repeats: vec![
            RepeatInstance {
                node: id("outer_repeat"),
                iteration: play("outer-plays", 0),
            },
            RepeatInstance {
                node: id("inner_repeat"),
                iteration: play("inner-plays", 1),
            },
        ],
    };
    let current = plan
        .source_voice_occurrence(instance.clone(), recipe.clone(), Default::default())
        .unwrap();
    let scopes = plan
        .beat_sound_clock_scopes(&id("owner_a"), &sound, deadpan_core::MAX_DOCUMENT_NODES)
        .unwrap();
    assert_eq!(scopes.len(), 1);
    assert_eq!(
        scopes[0].remap_instance(current.instance()).unwrap(),
        instance
    );

    let historical_plan = plan
        .compile_sound_processing_layout(&layout, &BTreeSet::from([event.source.asset.clone()]))
        .unwrap();
    let (binding, _) = scopes[0]
        .bind_historical_plan(&historical_plan, deadpan_core::MAX_DOCUMENT_NODES)
        .unwrap();
    let historical = historical_plan
        .source_voice_occurrence(instance, recipe, Default::default())
        .unwrap();
    let (alias, _) = binding
        .alias_occurrence(&historical, &current, deadpan_core::MAX_DOCUMENT_NODES)
        .unwrap();
    let placements = [historical.extent(), current.extent()];
    assert!(
        current
            .routed_gate_fades_from(
                &historical,
                &alias,
                &placements,
                (event.start_edge, event.end_edge),
                current.samples(),
                Default::default(),
            )
            .is_ok()
    );

    // Equal plan contents do not let an occurrence borrow another plan's token.
    let foreign_history = historical_plan.clone();
    let foreign_original = foreign_history
        .source_voice_occurrence(
            historical.instance().clone(),
            historical.recipe().clone(),
            Default::default(),
        )
        .unwrap();
    assert!(
        binding
            .alias_occurrence(
                &foreign_original,
                &current,
                deadpan_core::MAX_DOCUMENT_NODES
            )
            .is_err()
    );
    assert!(
        current
            .routed_gate_fades_from(
                &foreign_original,
                &alias,
                &placements,
                (event.start_edge, event.end_edge),
                current.samples(),
                Default::default(),
            )
            .is_err()
    );
    let foreign_live = plan.clone();
    let foreign_current = foreign_live
        .source_voice_occurrence(
            current.instance().clone(),
            current.recipe().clone(),
            Default::default(),
        )
        .unwrap();
    assert!(
        binding
            .alias_occurrence(
                &historical,
                &foreign_current,
                deadpan_core::MAX_DOCUMENT_NODES
            )
            .is_err()
    );

    let different_recipe = deadpan_plan::AudioSourceVoiceRecipe {
        source: event.source.clone(),
        mapping: event.mapping,
        offset: AudioSample(1),
    };
    let wrong_current = plan
        .source_voice_occurrence(
            current.instance().clone(),
            different_recipe,
            Default::default(),
        )
        .unwrap();
    assert!(
        binding
            .alias_occurrence(
                &historical,
                &wrong_current,
                deadpan_core::MAX_DOCUMENT_NODES
            )
            .is_err()
    );
}

#[test]
fn sound_processing_plan_rejects_rate_foreign_missing_unqualified_and_oversize_inputs() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = document(rate);
    let live = RenderPlan::compile(&document).unwrap();
    let layout = FrozenAudioLayout::capture(&document).unwrap();

    assert!(
        live.compile_sound_processing_layout(&layout, &BTreeSet::new())
            .is_err()
    );
    assert!(
        live.compile_sound_processing_layout(
            &layout,
            &BTreeSet::from([AssetId::new("foreign").unwrap()]),
        )
        .is_err()
    );
    assert!(
        live.compile_sound_processing_layout(
            &layout,
            &BTreeSet::from([AssetId::new("unqualified").unwrap()]),
        )
        .is_err()
    );
    let oversized = (0..=MAX_DOCUMENT_SOUNDS)
        .map(|index| AssetId::new(format!("asset-{index}")).unwrap())
        .collect();
    assert!(
        live.compile_sound_processing_layout(&layout, &oversized)
            .is_err()
    );

    let foreign_layout =
        FrozenAudioLayout::capture(&self::document(FrameRate::new(25, 1).unwrap())).unwrap();
    assert!(
        live.compile_sound_processing_layout(
            &foreign_layout,
            &BTreeSet::from([AssetId::new("sound").unwrap()]),
        )
        .is_err()
    );
}

#[test]
fn sound_processing_compiler_does_not_weaken_full_context_admission() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = document(rate);
    let context = FrozenAudioContext::capture(&document).unwrap();
    let full = RenderPlan::compile_audio_context(&context).unwrap();
    assert!(full.audio_context_assets().unwrap().is_empty());

    let (source, mapping) = sound_recipe(rate);
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        id("owner_a"),
        BTreeMap::from([(
            SoundId::new("event").unwrap(),
            BeatSound {
                label: "event".into(),
                source,
                mapping,
                offset: AudioSample(0),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]),
    )]))
    .unwrap();
    let with_sound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(FrozenAudioContext::capture(&with_sound).is_err());
}
