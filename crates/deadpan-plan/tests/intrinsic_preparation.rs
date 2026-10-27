use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{AudioIntrinsicPreparation, AudioQueryLimits, PlanError, RenderPlan};

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn source(frames: i64) -> BeatNode {
    let base = SourceTimeBase::new(1, 48_000).unwrap();
    let samples = FrameRate::new(30_000, 1001)
        .unwrap()
        .audio_boundary(ProjectFrame(frames))
        .unwrap();
    let mut node = BeatNode::sequence("Source", Vec::new());
    node.kind = NodeKind::Source {
        source: SourceNode {
            edit_window: None,
            duration: duration(frames),
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(SourceAudio {
                asset: AssetId::new("original").unwrap(),
                span: SourceSpan::new(
                    SourceTimestamp {
                        ticks: 0,
                        time_base: base,
                    },
                    SourceTimestamp {
                        ticks: samples.0,
                        time_base: base,
                    },
                )
                .unwrap(),
            }),
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
    node
}

fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Silence",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

fn retime(child: &str, input: i64, output: i64, pitch: PitchPolicy) -> BeatNode {
    let mut node = BeatNode::sequence("Retime", Vec::new());
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: duration(output),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    node
}

fn repeat(child: &str, plays: u32) -> BeatNode {
    let mut node = BeatNode::sequence("Repeat", Vec::new());
    node.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(revision("plays"), plays).unwrap(),
        gap: None,
        escalation: None,
    };
    node
}

fn document(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let base = ProjectDocument::new(
        ProjectId::new("intrinsic-discovery").unwrap(),
        revision("live"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(base).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let base = SourceTimeBase::new(1, 48_000).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("original").unwrap(),
        AssetRecord {
            label: "Original fixture audio".into(),
            content_hash: "c".repeat(64),
            audio: Some(
                SourceSpan::new(
                    SourceTimestamp {
                        ticks: 0,
                        time_base: base,
                    },
                    SourceTimestamp {
                        ticks: 96_000,
                        time_base: base,
                    },
                )
                .unwrap(),
            ),
            video: None,
            frame_count: None,
            still_image: false,
            source_qualification: Some(SourceQualificationId::new("d".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn discover(document: &ProjectDocument) -> AudioIntrinsicPreparation {
    RenderPlan::compile(document)
        .unwrap()
        .audio_intrinsic_preparation(AudioQueryLimits::default())
        .unwrap()
}

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}

fn bind_source(current: &ProjectDocument, historical: &ProjectDocument) -> ProjectDocument {
    let clock = timing("old");
    let binding = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: clock.clone(),
            layout: FrozenAudioLayout::capture(historical).unwrap(),
        }],
        BTreeMap::from([(
            id("leaf"),
            OwnedAudioBinding {
                lattice: AudioPlacementTemplate {
                    reference_local_offset: ExactRatio::ZERO,
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: AudioRecipeKind::Node,
                        timing: clock,
                        root: AudioClockRoot::ProjectRootRoundEven,
                        physical: id("leaf"),
                    },
                    arguments: Vec::new(),
                    births: Vec::new(),
                },
                reanchors: Vec::new(),
                resume: None,
            },
        )]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(binding).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn ordinary_source_sequence_and_repeat_are_free_without_expanding_plays() {
    for plays in [2, 1024] {
        let doc = document(
            &["group"],
            vec![
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("repeat"), id("tail")]),
                ),
                ("repeat", repeat("leaf", plays)),
                ("leaf", source(4)),
                ("tail", hold(2)),
            ],
        );
        assert_eq!(
            discover(&doc),
            AudioIntrinsicPreparation::NotRequired { work: 6 }
        );
    }
}

#[test]
fn unity_preserve_partition_and_nonunity_follow_speed_are_transparent() {
    let mut partition = retime("leaf", 4, 4, PitchPolicy::Preserve);
    let NodeKind::Retime { purpose, .. } = &mut partition.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let doc = document(&["view"], vec![("view", partition), ("leaf", source(8))]);
    assert!(discover(&doc).is_intrinsic_free());
    let doc = document(
        &["view"],
        vec![
            ("view", retime("leaf", 8, 4, PitchPolicy::FollowSpeed)),
            ("leaf", source(8)),
        ],
    );
    assert!(discover(&doc).is_intrinsic_free());
}

#[test]
fn nonunity_preserve_and_unity_shift_require_intrinsic_preparation() {
    for (input, pitch) in [
        (8, PitchPolicy::Preserve),
        (4, PitchPolicy::shifted(3).unwrap()),
    ] {
        let doc = document(
            &["view"],
            vec![
                ("view", retime("leaf", input, 4, pitch)),
                ("leaf", source(8)),
            ],
        );
        assert!(matches!(
            discover(&doc),
            AudioIntrinsicPreparation::MayBeRequired { .. }
        ));
    }
}

#[test]
fn sparse_override_processing_is_not_hidden_by_a_transparent_default_play() {
    let doc = document(
        &["repeat"],
        vec![("repeat", repeat("leaf", 2)), ("leaf", source(4))],
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["alternate"] =
        serde_json::to_value(retime("alt-leaf", 8, 4, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["alt-leaf"] = serde_json::to_value(source(8)).unwrap();
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: IterationId {
                allocation: revision("plays"),
                ordinal: 1,
            },
            root: id("alternate"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(!discover(&doc).is_intrinsic_free());
}

#[test]
fn a_bound_historical_definition_is_scanned_behind_an_ordinary_current_source() {
    let current = document(&["leaf"], vec![("leaf", source(8))]);
    let historical = document(
        &["leaf", "view"],
        vec![
            ("leaf", source(8)),
            ("view", retime("hidden-input", 8, 4, PitchPolicy::Preserve)),
            ("hidden-input", source(8)),
        ],
    );
    assert!(discover(&current).is_intrinsic_free());
    let bound = bind_source(&current, &historical);
    assert!(!discover(&bound).is_intrinsic_free());

    let historical = document(
        &["leaf", "view"],
        vec![
            ("leaf", source(8)),
            (
                "view",
                retime("hidden-input", 8, 4, PitchPolicy::FollowSpeed),
            ),
            ("hidden-input", source(8)),
        ],
    );
    assert!(discover(&bind_source(&current, &historical)).is_intrinsic_free());
}

#[test]
fn retained_audio_context_keeps_its_own_historical_processing_inventory() {
    let historical = document(
        &["view"],
        vec![
            ("view", retime("leaf", 8, 4, PitchPolicy::Preserve)),
            ("leaf", source(8)),
        ],
    );
    let context = FrozenAudioContext::capture(&historical).unwrap();
    let plan = RenderPlan::compile_audio_context(&context).unwrap();
    assert!(
        !plan
            .audio_intrinsic_preparation(AudioQueryLimits::default())
            .unwrap()
            .is_intrinsic_free()
    );
}

#[test]
fn work_exhaustion_retains_spent_visits_and_never_claims_intrinsic_free() {
    let doc = document(&["leaf"], vec![("leaf", source(4))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let complete = plan
        .audio_intrinsic_preparation(AudioQueryLimits::default())
        .unwrap();
    assert_eq!(complete, AudioIntrinsicPreparation::NotRequired { work: 3 });
    for maximum_work in 1..complete.work() {
        let partial = plan
            .audio_intrinsic_preparation(AudioQueryLimits {
                maximum_work,
                ..AudioQueryLimits::default()
            })
            .unwrap();
        assert_eq!(
            partial,
            AudioIntrinsicPreparation::WorkLimit { work: maximum_work }
        );
        assert!(!partial.is_intrinsic_free());
    }
    assert_eq!(
        plan.audio_intrinsic_preparation(AudioQueryLimits {
            maximum_work: complete.work(),
            ..AudioQueryLimits::default()
        })
        .unwrap(),
        complete,
    );
    let historical = document(
        &["leaf", "view"],
        vec![
            ("leaf", source(8)),
            ("view", retime("hidden-input", 8, 4, PitchPolicy::Preserve)),
            ("hidden-input", source(8)),
        ],
    );
    let current = document(&["leaf"], vec![("leaf", source(8))]);
    let bound = RenderPlan::compile(&bind_source(&current, &historical)).unwrap();
    let positive = bound
        .audio_intrinsic_preparation(AudioQueryLimits::default())
        .unwrap();
    assert!(matches!(
        positive,
        AudioIntrinsicPreparation::MayBeRequired { .. }
    ));
    let short = bound
        .audio_intrinsic_preparation(AudioQueryLimits {
            maximum_work: positive.work() - 1,
            ..AudioQueryLimits::default()
        })
        .unwrap();
    assert_eq!(
        short,
        AudioIntrinsicPreparation::WorkLimit {
            work: positive.work() - 1
        }
    );
}

#[test]
fn invalid_limits_fail_before_discovery() {
    let doc = document(&["leaf"], vec![("leaf", source(4))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    for limits in [
        AudioQueryLimits {
            maximum_work: 0,
            ..AudioQueryLimits::default()
        },
        AudioQueryLimits {
            maximum_work: 65_537,
            ..AudioQueryLimits::default()
        },
        AudioQueryLimits {
            maximum_spans: 0,
            ..AudioQueryLimits::default()
        },
        AudioQueryLimits {
            maximum_spans: 4097,
            ..AudioQueryLimits::default()
        },
    ] {
        assert!(matches!(
            plan.audio_intrinsic_preparation(limits),
            Err(PlanError::InvalidAudioLimits)
        ));
    }
}

fn sound_source() -> SourceAudio {
    let base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("catalog").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: base,
            },
            SourceTimestamp {
                ticks: 1024,
                time_base: base,
            },
        )
        .unwrap(),
    }
}

fn with_sounds(doc: &ProjectDocument, root: bool) -> ProjectDocument {
    let mut wire = serde_json::to_value(doc).unwrap();
    let source = sound_source();
    wire["assets"][source.asset.as_str()] = serde_json::to_value(AssetRecord {
        label: "Catalog sound".into(),
        content_hash: "a".repeat(64),
        audio: Some(source.span),
        video: None,
        frame_count: None,
        still_image: false,
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    })
    .unwrap();
    let mapping =
        SourceAudioMapping::natural_rate(source.span, doc.presentation_basis().frame_rate).unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        id("owner"),
        BTreeMap::from([(
            SoundId::new("beat").unwrap(),
            BeatSound {
                label: "Beat sound".into(),
                source: source.clone(),
                mapping,
                offset: AudioSample(13),
                treatments: AudioTreatments::default(),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]),
    )]))
    .unwrap();
    if root {
        let sound = SoundId::new("root-sound").unwrap();
        wire["sounds"] = serde_json::to_value(BTreeMap::from([(
            sound.clone(),
            SoundEvent {
                owner: id("root"),
                label: "Root sound".into(),
                source,
                mapping,
                offset: AudioSample(17),
                treatments: AudioTreatments::default(),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]))
        .unwrap();
        wire["sound_routes"] = serde_json::to_value(BTreeMap::from([(
            sound,
            RootSoundRoute::identity(doc.duration().unwrap(), doc.presentation_basis().frame_rate),
        )]))
        .unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn routed_root_and_beat_source_recipes_do_not_invent_intrinsic_processing() {
    let doc = document(&["owner"], vec![("owner", source(4))]);
    let doc = with_sounds(&doc, true);
    let plan = RenderPlan::compile(&doc).unwrap();
    assert!(
        plan.root_sound(&SoundId::new("root-sound").unwrap())
            .unwrap()
            .routed_input()
            .is_some()
    );
    let event = &plan.beat_sounds()[&id("owner")][&SoundId::new("beat").unwrap()];
    let voice = plan
        .source_voice_occurrence(
            InstancePath {
                node: id("owner"),
                repeats: Vec::new(),
            },
            deadpan_plan::AudioSourceVoiceRecipe {
                source: event.source.clone(),
                mapping: event.mapping,
                offset: event.offset,
            },
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert!(voice.processing_projection().is_none());
    assert!(
        plan.audio_intrinsic_preparation(AudioQueryLimits::default())
            .unwrap()
            .is_intrinsic_free()
    );

    // Independent sounds do not expose a hidden pitch-preserving stretch flag:
    // Core refuses a mapping with a different duration instead of inventing DSP.
    let mut wire = serde_json::to_value(&doc).unwrap();
    wire["beat_sounds"]["owner"]["beat"]["mapping"] =
        serde_json::to_value(SourceAudioMapping::Duration {
            frames: ExactRatio::integer(2),
        })
        .unwrap();
    assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
}

#[test]
fn every_sound_journal_timing_is_scanned_not_only_the_first_clock() {
    let current = document(
        &["owner", "hidden"],
        vec![("owner", source(4)), ("hidden", hold(4))],
    );
    let current = with_sounds(&current, false);
    let historical = document(
        &["owner", "hidden"],
        vec![
            ("owner", source(4)),
            (
                "hidden",
                retime("hidden-input", 8, 4, PitchPolicy::Preserve),
            ),
            ("hidden-input", source(8)),
        ],
    );
    let first = timing("first-clock");
    let second = timing("later-clock");
    let state = AudioBindingState::new_with_sound_clocks(
        vec![
            AudioTimingRecord {
                id: first.clone(),
                layout: FrozenAudioLayout::capture(&current).unwrap(),
            },
            AudioTimingRecord {
                id: second.clone(),
                layout: FrozenAudioLayout::capture(&historical).unwrap(),
            },
        ],
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::from([(
            id("owner"),
            BTreeMap::from([(
                SoundId::new("beat").unwrap(),
                SoundClockJournal::new(
                    id("owner"),
                    vec![
                        SoundClockReference::new(first, id("owner"), id("owner")),
                        SoundClockReference::new(second, id("owner"), id("owner")),
                    ],
                )
                .unwrap(),
            )]),
        )]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(&current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(discover(&current).is_intrinsic_free());
    assert!(matches!(
        discover(&doc),
        AudioIntrinsicPreparation::MayBeRequired { .. }
    ));
}

#[test]
fn owned_preserve_input_is_detected_when_effective_live_clock_is_unity() {
    let mut wire = serde_json::to_value(document(
        &["stage"],
        vec![
            ("stage", retime("group", 4, 2, PitchPolicy::Preserve)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("left"), id("pause"), id("right")]),
            ),
            ("left", hold(4)),
            ("pause", hold(2)),
            ("right", hold(4)),
        ],
    ))
    .unwrap();
    let range = |start: i64, end: i64| ExactFrameRange {
        start: ExactRatio::integer(start),
        end: ExactRatio::integer(end),
    };
    let run = |destination, source, name: &str| PreserveInputRun {
        destination,
        source,
        provider: PreserveInputProvider::Current {
            target: InstancePath {
                node: id(name),
                repeats: Vec::new(),
            },
        },
    };
    wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([(
        id("stage"),
        DerivedRetimeClock {
            extent: ExactRatio::integer(10),
            child_start: ExactRatio::ZERO,
            child_end: ExactRatio::integer(10),
            preserve_input: Some(
                PreserveInputProjection::new(vec![
                    run(range(0, 2), range(0, 2), "left"),
                    run(range(2, 4), range(0, 2), "right"),
                ])
                .unwrap(),
            ),
        },
    )]))
    .unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let clock = &doc.derived_clocks()[&id("stage")];
    assert_eq!(
        clock.child_end.checked_sub(clock.child_start).unwrap(),
        clock.extent
    );
    assert!(!discover(&doc).is_intrinsic_free());

    // Timing-only capture drops the owned input, but its integral processing
    // recipe remains visible through the retained-layout predicate.
    let layout = FrozenAudioLayout::capture(&doc).unwrap();
    assert!(layout.retime_processes(&id("stage"), &layout.nodes()[&id("stage")]));
}
