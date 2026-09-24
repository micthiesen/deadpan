use std::collections::BTreeMap;

use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn timing(ordinal: u32) -> AudioTimingId {
    AudioTimingId {
        allocation: revision("capture"),
        ordinal,
    }
}
fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision("plays"),
        ordinal,
    }
}
fn hold(duration: i64) -> BeatNode {
    BeatNode::hold(
        "hold",
        HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn repeat(child: &str, count: u32) -> BeatNode {
    BeatNode {
        label: "repeat".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(revision("plays"), count).unwrap(),
            gap: None,
        },
    }
}
fn retime(child: &str, duration: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "retime".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(duration),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose: RetimePurpose::Edit,
        },
    }
}
fn document(
    children: &[&str],
    nodes: impl IntoIterator<Item = (NodeId, BeatNode)>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("capture").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes.into_iter().collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("root", children.iter().map(|value| id(value)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn install(document: &ProjectDocument, state: &AudioBindingState) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn instance(node: &str, repeats: &[(&str, u32)]) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: repeats
            .iter()
            .map(|(node, ordinal)| RepeatInstance {
                node: id(node),
                iteration: play(*ordinal),
            })
            .collect(),
    }
}

#[test]
fn captures_unplayed_defaults_and_closes_each_override_at_its_stable_play() {
    let before = document(
        &["r"],
        [(id("r"), repeat("default", 2)), (id("default"), hold(4))],
    );
    let mut wire = serde_json::to_value(before).unwrap();
    wire["nodes"]["o0"] = serde_json::to_value(hold(2)).unwrap();
    wire["nodes"]["o1"] = serde_json::to_value(hold(3)).unwrap();
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("r"),
        PlayOverrides::try_from(vec![
            PlayOverride {
                iteration: play(0),
                root: id("o0"),
            },
            PlayOverride {
                iteration: play(1),
                root: id("o1"),
            },
        ])
        .unwrap(),
    )]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let state = capture_unbound_audio_bindings(&before, timing(0)).unwrap();
    assert_eq!(state.timings().len(), 1);
    assert_eq!(state.bindings().len(), 3);
    assert!(
        state
            .bindings()
            .values()
            .all(|binding| binding.resume.is_none())
    );
    let default = &state.bindings()[&id("default")].lattice;
    assert_eq!(
        default.arguments[0].value,
        AudioRepeatValue::Live { repeat: id("r") }
    );
    assert_eq!(default.births[0].definition_root, id("default"));
    let born = state
        .resolve(&id("default"), &instance("default", &[("r", 0)]), 1000)
        .unwrap();
    assert_eq!(born.lattice.birth, Some(0));
    assert_eq!(born.lattice.origin, ExactRatio::ZERO);
    for ordinal in 0..2 {
        let owner = id(&format!("o{ordinal}"));
        let binding = &state.bindings()[&owner].lattice;
        assert_eq!(
            binding.arguments[0].value,
            AudioRepeatValue::Captured {
                iteration: play(ordinal)
            }
        );
        assert!(binding.births.is_empty());
    }
    let bound = install(&before, &state);
    let transaction = apply(
        &bound,
        &CommandRequest {
            project_id: bound.project_id().clone(),
            expected_revision: bound.revision_id().clone(),
            new_revision: revision("reorder"),
            command: Command::MovePlays {
                node: id("r"),
                start: 1,
                end: 2,
                destination: 0,
            },
        },
    )
    .unwrap();
    let reordered = transaction.forward.apply(&bound).unwrap();
    let retained = capture_unbound_audio_bindings(&reordered, timing(1)).unwrap();
    assert_eq!(retained, state);
    assert_eq!(
        retained
            .resolve(&id("o1"), &instance("o1", &[("r", 1)]), 1000)
            .unwrap()
            .lattice
            .origin,
        ExactRatio::integer(2)
    );
}

#[test]
fn nested_preserve_resets_input_scope_after_capturing_its_output() {
    let before = document(
        &["outside", "tail"],
        [
            (id("outside"), repeat("outer", 2)),
            (
                id("outer"),
                retime("input", 12, 1, 7, PitchPolicy::Preserve),
            ),
            (
                id("input"),
                BeatNode::sequence("input", vec![id("x"), id("inside")]),
            ),
            (id("x"), hold(2)),
            (id("inside"), repeat("inner", 2)),
            (id("inner"), retime("unity", 3, 1, 5, PitchPolicy::Preserve)),
            (id("unity"), retime("a", 6, 0, 6, PitchPolicy::Preserve)),
            (id("a"), hold(6)),
            (id("tail"), hold(1)),
        ],
    );
    let state = capture_unbound_audio_bindings(&before, timing(0)).unwrap();
    assert_eq!(state.bindings().len(), 5);
    let binding = |name: &str| &state.bindings()[&id(name)].lattice;
    assert_eq!(
        binding("outer").reference.root,
        AudioClockRoot::ProjectRootRoundEven
    );
    assert_eq!(
        binding("outer").arguments[0].reference_repeat,
        id("outside")
    );
    assert_eq!(
        binding("inner").reference.root,
        AudioClockRoot::PreserveInputPointCeil { stage: id("outer") }
    );
    assert_eq!(binding("inner").arguments.len(), 1);
    assert_eq!(binding("inner").arguments[0].reference_repeat, id("inside"));
    assert_eq!(
        binding("a").reference.root,
        AudioClockRoot::PreserveInputPointCeil { stage: id("inner") }
    );
    assert!(binding("a").arguments.is_empty());
    assert!(binding("a").births.is_empty());
    assert_eq!(
        binding("x").reference.root,
        AudioClockRoot::PreserveInputPointCeil { stage: id("outer") }
    );
    assert_eq!(
        binding("tail").reference.root,
        AudioClockRoot::ProjectRootRoundEven
    );
    assert!(binding("tail").arguments.is_empty());
    let source = state
        .resolve(
            &id("a"),
            &instance("a", &[("outside", 1), ("inside", 1)]),
            1000,
        )
        .unwrap();
    assert_eq!(source.lattice.grid_origin, ExactRatio::ONE);
    assert_eq!(
        source.lattice.local_support,
        ExactRatio::ONE..ExactRatio::integer(5)
    );
    install(&before, &state).validate().unwrap();
}

#[test]
fn source_capture_retains_its_current_placement_without_freezing_raw_assets() {
    let before = document(&["a"], [(id("a"), hold(4))]);
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 6400,
            time_base,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(before).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("audio").unwrap(),
        AssetRecord {
            label: "input".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    wire["nodes"]["a"] = serde_json::to_value(BeatNode {
        label: "source".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(4),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: AssetId::new("audio").unwrap(),
                    span,
                }),
                audio_mapping: SourceAudioMapping::Placement {
                    start: ExactRatio::new(-1, 3).unwrap(),
                    frames: ExactRatio::integer(4),
                },
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    })
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let state = capture_unbound_audio_bindings(&before, timing(0)).unwrap();
    assert_eq!(state.bindings().len(), 1);
    assert_eq!(
        state.bindings()[&id("a")].lattice.reference.physical,
        id("a")
    );
    assert_eq!(
        state.timings()[&timing(0)].nodes()[&id("a")].kind,
        FrozenAudioKind::Source {
            placement: Some(
                ExactFrameRange::new(
                    ExactRatio::new(-1, 3).unwrap(),
                    ExactRatio::new(11, 3).unwrap(),
                )
                .unwrap()
            ),
        },
    );
    assert!(!state.to_json().unwrap().contains("content_hash"));
    assert!(!state.to_json().unwrap().contains("6400"));
}

#[test]
fn an_existing_intrinsic_binding_survives_a_new_wrapper_and_more_capture() {
    let before = document(&["a"], [(id("a"), hold(4))]);
    let mut state =
        serde_json::to_value(capture_unbound_audio_bindings(&before, timing(0)).unwrap()).unwrap();
    state["bindings"]["a"]["resume"] = serde_json::to_value(AudioResume {
        local_boundary: ExactRatio::ONE,
        phase: AudioLocalPhase {
            constant: ExactRatio::new(1, 7).unwrap(),
            terms: vec![],
        },
    })
    .unwrap();
    let state = AudioBindingState::from_json(&state.to_string()).unwrap();
    let bound = install(&before, &state);
    let mut wire = serde_json::to_value(bound).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("root", vec![id("wrap")])).unwrap();
    wire["nodes"]["wrap"] = serde_json::to_value(repeat("a", 3)).unwrap();
    let wrapped = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        capture_unbound_audio_bindings(&wrapped, timing(0)).unwrap(),
        state
    );
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("root", vec![id("wrap"), id("b")])).unwrap();
    wire["nodes"]["b"] = serde_json::to_value(hold(2)).unwrap();
    let expanded = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(
        capture_unbound_audio_bindings(&expanded, timing(0))
            .unwrap_err()
            .message
            .contains("already retained")
    );
    let captured = capture_unbound_audio_bindings(&expanded, timing(1)).unwrap();
    assert_eq!(captured.timings().len(), 2);
    assert_eq!(captured.bindings()[&id("a")], state.bindings()[&id("a")]);
    assert!(captured.bindings()[&id("a")].lattice.births.is_empty());
    assert_eq!(
        captured.bindings()[&id("b")].lattice.reference.timing,
        timing(1)
    );
    assert_eq!(
        captured
            .resolve(&id("b"), &instance("b", &[]), 1000)
            .unwrap()
            .lattice
            .origin,
        ExactRatio::integer(12)
    );
}

#[test]
fn captures_a_billion_plays_compactly_and_empty_projects_are_noops() {
    let empty = document(&[], []);
    assert_eq!(
        capture_unbound_audio_bindings(&empty, timing(0)).unwrap(),
        AudioBindingState::default()
    );
    let before = document(
        &["r"],
        [(id("r"), repeat("a", 1_000_000_000)), (id("a"), hold(1))],
    );
    let state = capture_unbound_audio_bindings(&before, timing(0)).unwrap();
    assert_eq!(state.timings()[&timing(0)].nodes().len(), 3);
    assert_eq!(state.bindings().len(), 1);
    assert_eq!(state.bindings()[&id("a")].lattice.arguments.len(), 1);
    let last = state
        .resolve(&id("a"), &instance("a", &[("r", 999_999_999)]), 1000)
        .unwrap();
    assert_eq!(last.lattice.origin, ExactRatio::integer(999_999_999));
}

#[test]
fn capture_rejects_unrepresented_gaps_and_path_expansion_atomically() {
    let mut r = repeat("a", 2);
    if let NodeKind::Repeat { gap, .. } = &mut r.kind {
        *gap = Some(HoldRecipe {
            duration: frames(1),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        });
    }
    let gaps = document(&["r"], [(id("r"), r), (id("a"), hold(2))]);
    let original = gaps.to_json().unwrap();
    assert!(
        capture_unbound_audio_bindings(&gaps, timing(0))
            .unwrap_err()
            .message
            .contains("gap binding ownership")
    );
    assert_eq!(gaps.to_json().unwrap(), original);
    let mut nodes = BTreeMap::new();
    for index in 0..100 {
        let child = if index == 99 {
            "leaves".into()
        } else {
            format!("r{}", index + 1)
        };
        nodes.insert(id(&format!("r{index}")), repeat(&child, 1));
    }
    let leaves: Vec<_> = (0..600).map(|index| id(&format!("leaf{index}"))).collect();
    for leaf in &leaves {
        nodes.insert(leaf.clone(), hold(1));
    }
    nodes.insert(id("leaves"), BeatNode::sequence("leaves", leaves));
    let large = document(&["r0"], nodes);
    let original = large.to_json().unwrap();
    let error = capture_unbound_audio_bindings(&large, timing(0)).unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::LimitExceeded);
    assert!(error.message.contains("capture complexity"));
    assert_eq!(large.to_json().unwrap(), original);
}
