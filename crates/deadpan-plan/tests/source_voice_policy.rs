use std::{collections::BTreeMap, ops::Range};

use deadpan_core::*;
use deadpan_plan::{
    AudioContent, AudioDefinitionSelector, AudioHoldIssuer, AudioHoldRule, AudioQueryLimits,
    AudioSignal, AudioSignalTape, AudioSignalTapeRun, AudioSourceVoiceRecipe, PlanError,
    RenderPlan, SignalSample,
};

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn points(start: i64, end: i64) -> Range<SignalSample> {
    SignalSample(start)..SignalSample(end)
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn source_audio(name: &str) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new(name).unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 100,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn hold(length: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            picture_context: None,
            audio,
        },
    )
}

fn source(length: i64, audio: bool, selected: bool) -> BeatNode {
    BeatNode {
        label: "Original".into(),
        framing: None,
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(length),
                video: SourceVideo::Still {
                    asset: AssetId::new("original").unwrap(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: audio.then(|| source_audio("original")),
                audio_mapping: if !audio {
                    SourceAudioMapping::FitBeat
                } else if selected {
                    SourceAudioMapping::SelectedPlacement {
                        start: ExactRatio::ZERO,
                        frames: ExactRatio::integer(100),
                        selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::ONE).unwrap(),
                    }
                } else {
                    SourceAudioMapping::Duration {
                        frames: ExactRatio::integer(100),
                    }
                },
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn document(
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("source-voice-policy").unwrap(),
        RevisionId::new("current").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    let assets = BTreeMap::from([
        (
            AssetId::new("original").unwrap(),
            AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(source_audio("original").span),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        ),
        (
            AssetId::new("sound").unwrap(),
            AssetRecord {
                label: "Catalog sound".into(),
                content_hash: "b".repeat(64),
                video: None,
                audio: Some(source_audio("sound").span),
                still_image: false,
                frame_count: None,
                source_qualification: Some(SourceQualificationId::new("c".repeat(64)).unwrap()),
            },
        ),
    ]);
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(assets).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn recipe() -> AudioSourceVoiceRecipe {
    AudioSourceVoiceRecipe {
        source: source_audio("sound"),
        mapping: SourceAudioMapping::Duration {
            frames: ExactRatio::integer(100),
        },
        offset: AudioSample(0),
    }
}

fn only_catalog_source(contents: &[AudioContent]) {
    assert!(!contents.is_empty());
    for content in contents {
        let AudioContent::Source { source, .. } = content else {
            panic!("unexpected inherited carrier content: {content:?}")
        };
        assert_eq!(source.asset, AssetId::new("sound").unwrap());
    }
}

#[test]
fn input_retains_sound_while_output_applies_only_current_silent_holds() {
    let document = document(
        &["absent", "selected", "room", "tail", "silent"],
        [
            ("absent", source(2, false, false)),
            ("selected", source(2, true, true)),
            (
                "room",
                hold(
                    2,
                    HoldAudio::RoomTone {
                        source: source_audio("original"),
                    },
                ),
            ),
            (
                "tail",
                hold(
                    2,
                    HoldAudio::Tail {
                        source: source_audio("original"),
                        maximum: frames(2),
                    },
                ),
            ),
            ("silent", hold(2, HoldAudio::Silence)),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    let voice = plan.audio_signal().source_voice(recipe()).unwrap();
    let input = voice.input_signal();
    let output = voice.output_signal();
    let input_policy = input.policy(points(0, 10), Default::default()).unwrap();
    let output_policy = output.policy(points(0, 10), Default::default()).unwrap();
    assert!(input_policy.suppressed.is_empty());
    assert_eq!(output_policy.suppressed, vec![points(8, 10)]);
    only_catalog_source(&input_policy.contents);
    only_catalog_source(&output_policy.contents);
    let input_holds = input
        .hold_policy(points(0, 10), Default::default())
        .unwrap();
    let output_holds = output
        .hold_policy(points(0, 10), Default::default())
        .unwrap();
    assert_eq!(input_holds, output_holds);
    assert_eq!(
        output_holds.rules,
        vec![AudioHoldRule {
            samples: points(8, 10),
            issuer: AudioHoldIssuer::Node {
                definition: None,
                instance: InstancePath {
                    node: id("silent"),
                    repeats: vec![]
                },
            },
        }]
    );
}

#[test]
fn preserve_excludes_own_selected_endpoints_but_keeps_current_hold_output_policy() {
    let plan = RenderPlan::compile(&document(
        &["before", "silent", "after"],
        [
            ("before", source(2, false, false)),
            ("silent", hold(2, HoldAudio::Silence)),
            ("after", source(4, false, false)),
        ],
    ))
    .unwrap();
    let mut selected = recipe();
    selected.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: ExactRatio::integer(100),
        selection: ExactFrameRange::new(ExactRatio::ONE, ExactRatio::integer(6)).unwrap(),
    };
    let voice = plan.audio_signal().source_voice(selected).unwrap();
    for (signal, before, after) in [
        (
            voice.input_signal(),
            vec![points(0, 1), points(6, 8)],
            vec![],
        ),
        (
            voice.output_signal(),
            vec![points(0, 1), points(2, 4), points(6, 8)],
            vec![points(2, 4)],
        ),
    ] {
        assert_eq!(
            signal
                .policy(points(0, 8), Default::default())
                .unwrap()
                .suppressed,
            before
        );
        assert_eq!(
            signal
                .policy_after_preserve(points(0, 8), Default::default())
                .unwrap()
                .suppressed,
            after
        );
    }
}

fn stretched_tape<'plan>(
    plan: &'plan RenderPlan,
    signal: AudioSignal<'plan>,
) -> AudioSignalTape<'plan> {
    AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ExactRatio::integer(6),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ExactRatio::integer(6),
            ExactRatio::ZERO..ExactRatio::integer(2),
            signal,
        )],
    )
    .unwrap()
}

#[test]
fn tape_recomputes_fractional_hold_boundaries_on_its_consuming_grid() {
    let plan = RenderPlan::compile(&document(
        &["speed"],
        [
            ("silent", hold(1, HoldAudio::Silence)),
            ("rest", source(3, false, false)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("silent"), id("rest")]),
            ),
            (
                "speed",
                BeatNode {
                    label: "Preserve".into(),
                    framing: None,
                    audio_edges: Default::default(),
                    kind: NodeKind::Retime {
                        child: id("group"),
                        duration: frames(2),
                        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
                        pitch: PitchPolicy::Preserve,
                        purpose: RetimePurpose::Edit,
                    },
                },
            ),
        ],
    ))
    .unwrap();
    let voice = plan.audio_signal().source_voice(recipe()).unwrap();
    assert_eq!(
        voice
            .output_signal()
            .policy(points(0, 2), Default::default())
            .unwrap()
            .suppressed,
        vec![points(0, 1)]
    );
    let output = stretched_tape(&plan, voice.output_signal());
    let input = stretched_tape(&plan, voice.input_signal());
    // The Hold ends at 1/2 owner frame, then 3/2 destination frames. Scaling
    // its already rounded one-sample owner mask would incorrectly silence 3.
    assert_eq!(
        output
            .policy(points(0, 6), Default::default())
            .unwrap()
            .suppressed,
        vec![points(0, 2)]
    );
    assert_eq!(
        output
            .policy_after_preserve(points(0, 6), Default::default())
            .unwrap()
            .suppressed,
        vec![points(0, 2)]
    );
    assert!(
        input
            .policy(points(0, 6), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );
    assert_eq!(
        output
            .policy(points(1, 3), Default::default())
            .unwrap()
            .suppressed,
        vec![points(1, 2)]
    );
    assert!(
        output
            .policy(points(2, 6), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );
}

#[test]
fn repeated_and_definition_issuers_survive_the_independent_provider() {
    let allocation = RevisionId::new("plays").unwrap();
    let plan = RenderPlan::compile(&document(
        &["repeat"],
        [
            ("silent", hold(2, HoldAudio::Silence)),
            (
                "repeat",
                BeatNode {
                    label: "Repeat".into(),
                    framing: None,
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("silent"),
                        iterations: IterationOrder::new(allocation.clone(), 2).unwrap(),
                        gap: None,
                    },
                },
            ),
        ],
    ))
    .unwrap();
    let voice = plan.audio_signal().source_voice(recipe()).unwrap();
    let rules = voice
        .output_signal()
        .hold_policy(points(0, 4), Default::default())
        .unwrap()
        .rules;
    assert_eq!(rules.len(), 2);
    for (ordinal, rule) in rules.into_iter().enumerate() {
        assert_eq!(
            rule.issuer,
            AudioHoldIssuer::Node {
                definition: None,
                instance: InstancePath {
                    node: id("silent"),
                    repeats: vec![RepeatInstance {
                        node: id("repeat"),
                        iteration: IterationId {
                            allocation: allocation.clone(),
                            ordinal: u32::try_from(ordinal).unwrap()
                        },
                    }],
                },
            }
        );
    }
    let selector = AudioDefinitionSelector::RepeatDefault {
        repeat: id("repeat"),
    };
    let voice = plan
        .audio_definition(selector.clone())
        .unwrap()
        .signal()
        .source_voice(recipe())
        .unwrap();
    let rules = voice
        .input_signal()
        .hold_policy(points(0, 2), Default::default())
        .unwrap()
        .rules;
    assert_eq!(
        rules[0].issuer,
        AudioHoldIssuer::Node {
            definition: Some(selector),
            instance: InstancePath {
                node: id("silent"),
                repeats: vec![]
            },
        }
    );
}

#[test]
fn combined_policy_queries_share_work_and_structural_span_bounds() {
    let plan = RenderPlan::compile(&document(
        &["a", "b"],
        [
            ("a", hold(2, HoldAudio::Silence)),
            ("b", hold(2, HoldAudio::Silence)),
        ],
    ))
    .unwrap();
    let voice = plan.audio_signal().source_voice(recipe()).unwrap();
    let input = voice
        .input_signal()
        .policy(points(0, 4), Default::default())
        .unwrap();
    let output_signal = voice.output_signal();
    let output = output_signal
        .policy(points(0, 4), Default::default())
        .unwrap();
    assert!(output.work > input.work);
    assert!(output.lookup.visited_nodes > input.lookup.visited_nodes);
    let exact = AudioQueryLimits {
        maximum_spans: 2,
        maximum_work: output.work,
    };
    assert_eq!(
        output_signal
            .policy(points(0, 4), exact)
            .unwrap()
            .suppressed,
        output.suppressed
    );
    assert!(matches!(
        output_signal.policy(
            points(0, 4),
            AudioQueryLimits {
                maximum_work: output.work - 1,
                ..exact
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        output_signal.policy(
            points(0, 4),
            AudioQueryLimits {
                maximum_spans: 1,
                ..exact
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        output_signal.hold_policy(points(-1, 1), Default::default()),
        Err(PlanError::AudioRangeOutOfRange)
    ));
    assert!(
        output_signal
            .policy(points(4, 4), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );
}

#[test]
fn retained_original_bindings_do_not_supply_the_catalog_voice_policy() {
    let document = document(&["original"], [("original", source(4, true, true))]);
    let bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    assert_eq!(
        plan.audio_signal()
            .policy(points(0, 4), Default::default())
            .unwrap()
            .suppressed,
        vec![points(1, 4)]
    );
    let voice = plan.audio_signal().source_voice(recipe()).unwrap();
    for signal in [voice.input_signal(), voice.output_signal()] {
        let policy = signal.policy(points(0, 4), Default::default()).unwrap();
        assert!(policy.suppressed.is_empty());
        only_catalog_source(&policy.contents);
    }
}
