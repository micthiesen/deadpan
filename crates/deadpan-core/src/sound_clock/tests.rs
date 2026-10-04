use super::*;
use crate::*;

mod edits;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn sound(value: &str) -> SoundId {
    SoundId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn clock(ordinal: u32) -> AudioTimingId {
    AudioTimingId {
        allocation: revision("clock"),
        ordinal,
    }
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(value: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(value),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn fixture() -> ProjectDocument {
    let mut doc = ProjectDocument::new(
        ProjectId::new("sound-clocks").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    doc.nodes = BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("top")]),
        ),
        (node("lead"), hold(1)),
        (
            node("top"),
            BeatNode::sequence("Group", vec![node("owner"), node("sibling")]),
        ),
        (node("owner"), hold(4)),
        (node("sibling"), hold(4)),
    ]);
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 1000,
            time_base,
        },
    )
    .unwrap();
    let asset = AssetId::new("effect").unwrap();
    doc.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "Effect".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    doc.beat_sounds.insert(
        node("owner"),
        BTreeMap::from([(
            sound("effect"),
            BeatSound {
                label: "Effect".into(),
                source: SourceAudio { asset, span },
                mapping: SourceAudioMapping::natural_rate(span, doc.presentation_basis.frame_rate)
                    .unwrap(),
                offset: AudioSample(37),
                gain_millidecibels: -1500,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Automatic,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]),
    );
    doc.validate().unwrap();
    doc
}
fn record(doc: &ProjectDocument, ordinal: u32) -> AudioTimingRecord {
    AudioTimingRecord {
        id: clock(ordinal),
        layout: FrozenAudioLayout::capture(doc).unwrap(),
    }
}
fn clocks(ids: Vec<AudioTimingId>) -> SoundClocks {
    BTreeMap::from([(
        node("owner"),
        BTreeMap::from([(sound("effect"), SoundClockJournal::new(ids).unwrap())]),
    )])
}
fn state(records: Vec<AudioTimingRecord>, journals: SoundClocks) -> AudioBindingState {
    AudioBindingState::new_with_sound_clocks(records, BTreeMap::new(), BTreeMap::new(), journals)
        .unwrap()
}

#[test]
fn sound_only_clocks_roundtrip_and_retain_distinct_equal_layouts() {
    let before = fixture();
    let mut after = before.clone();
    after.nodes.insert(node("lead"), hold(3));
    after.audio_bindings = state(
        vec![record(&before, 0), record(&before, 1)],
        clocks(vec![clock(0), clock(1)]),
    );
    after.validate().unwrap();
    assert!(after.audio_bindings.bindings().is_empty());
    assert_eq!(
        after.audio_bindings.sound_clocks()[&node("owner")][&sound("effect")].clocks(),
        &[clock(0), clock(1)]
    );
    let decoded = ProjectDocument::from_json(&after.to_json().unwrap()).unwrap();
    assert_eq!(decoded, after);
    assert_eq!(
        FrozenAudioLayout::capture(&decoded).unwrap().duration(),
        frames(11)
    );
    crate::audio_binding_lifecycle::prune(&mut after);
    assert_eq!(after.audio_bindings.timings().len(), 2);
    assert!(AudioBindingState::default().sound_clocks().is_empty());
    assert!(
        !AudioBindingState::default()
            .to_json()
            .unwrap()
            .contains("sound_clocks")
    );
}

#[test]
fn pruning_tracks_exact_event_addresses_and_shared_physical_clocks() {
    let mut doc = fixture();
    let event = doc.beat_sounds[&node("owner")][&sound("effect")].clone();
    doc.beat_sounds
        .get_mut(&node("owner"))
        .unwrap()
        .insert(sound("second"), event);
    let mut journals = clocks(vec![clock(0)]);
    journals.get_mut(&node("owner")).unwrap().insert(
        sound("second"),
        SoundClockJournal::new(vec![clock(0)]).unwrap(),
    );
    doc.audio_bindings = state(vec![record(&doc, 0)], journals);
    doc.beat_sounds
        .get_mut(&node("owner"))
        .unwrap()
        .remove(&sound("effect"));
    crate::audio_binding_lifecycle::prune(&mut doc);
    assert_eq!(
        doc.audio_bindings.sound_clocks()[&node("owner")]
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![sound("second")]
    );
    assert_eq!(doc.audio_bindings.timings().len(), 1);
    doc.validate().unwrap();
    doc.beat_sounds.clear();
    crate::audio_binding_lifecycle::prune(&mut doc);
    assert!(doc.audio_bindings.is_empty());

    let mut doc = fixture();
    let mut physical = capture_unbound_audio_bindings(&doc, clock(0)).unwrap();
    physical.sound_clocks = clocks(vec![clock(0)]);
    doc.audio_bindings = physical;
    doc.validate().unwrap();
    doc.beat_sounds.clear();
    crate::audio_binding_lifecycle::prune(&mut doc);
    assert!(doc.audio_bindings.sound_clocks().is_empty());
    assert_eq!(doc.audio_bindings.timings().len(), 1);
    assert!(!doc.audio_bindings.bindings().is_empty());
    doc.validate().unwrap();
}

#[test]
fn missing_addresses_roots_and_timing_references_refuse() {
    let doc = fixture();
    assert!(
        AudioBindingState::new_with_sound_clocks(
            vec![],
            BTreeMap::new(),
            BTreeMap::new(),
            clocks(vec![clock(0)])
        )
        .is_err()
    );
    assert!(
        AudioBindingState::new_with_sound_clocks(
            vec![record(&doc, 0)],
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new()
        )
        .is_err()
    );
    let bindings = state(vec![record(&doc, 0)], clocks(vec![clock(0)]));
    let mut missing = doc.clone();
    missing.beat_sounds.clear();
    assert!(
        bindings
            .validate_for(&missing)
            .unwrap_err()
            .message
            .contains("no beat sound")
    );
    let mut root = doc.clone();
    let events = root.beat_sounds.remove(&node("owner")).unwrap();
    root.beat_sounds.insert(node("root"), events);
    let mut journals = BTreeMap::new();
    journals.insert(
        node("root"),
        clocks(vec![clock(0)]).remove(&node("owner")).unwrap(),
    );
    let bindings = state(vec![record(&root, 0)], journals);
    assert!(
        bindings
            .validate_for(&root)
            .unwrap_err()
            .message
            .contains("root-owned")
    );
}

#[test]
fn journal_and_relation_wire_are_strict_and_bounded() {
    assert!(SoundClockJournal::new(vec![]).is_err());
    assert!(SoundClockJournal::new(vec![clock(0), clock(0)]).is_err());
    let journal = SoundClockJournal::new(vec![clock(0)]).unwrap();
    assert!(journal.with_appended(clock(0)).is_err());
    assert_eq!(
        journal.with_appended(clock(1)).unwrap().clocks(),
        &[clock(0), clock(1)]
    );
    let excessive: Vec<_> = (0..=u32::try_from(MAX_SOUND_CLOCKS).unwrap())
        .map(clock)
        .collect();
    assert!(SoundClockJournal::new(excessive.clone()).is_err());
    assert!(
        serde_json::from_value::<SoundClockJournal>(serde_json::to_value(excessive).unwrap())
            .is_err()
    );
    let clock_json = serde_json::to_string(&clock(0)).unwrap();
    for json in [
        r#"{"owner":{}}"#.to_owned(),
        r#"{"owner":{"effect":[]}}"#.to_owned(),
        format!(r#"{{"owner":{{"effect":[{clock_json},{clock_json}]}}}}"#),
        format!(r#"{{"owner":{{"effect":[{clock_json}],"effect":[{clock_json}]}}}}"#),
        format!(r#"{{"owner":{{"effect":[{clock_json}]}},"owner":{{"effect":[{clock_json}]}}}}"#),
        r#"{"owner":{"effect":[{"allocation":"clock","ordinal":0,"extra":true}]}}"#.to_owned(),
        "null".to_owned(),
    ] {
        assert!(super::from_json(&json).is_err(), "accepted {json}");
    }
    let mut too_many = BTreeMap::new();
    for index in 0..=MAX_DOCUMENT_SOUNDS {
        too_many.insert(
            node(&format!("owner-{index}")),
            BTreeMap::from([(sound("effect"), journal.clone())]),
        );
    }
    assert!(super::from_json(&serde_json::to_string(&too_many).unwrap()).is_err());
    assert!(super::reference_count(&too_many).is_err());
    assert!(super::from_json(&" ".repeat(MAX_SOUND_CLOCK_BYTES + 1)).is_err());
}

#[test]
fn physical_binding_admission_does_not_accept_sequence_owner_clocks() {
    let doc = fixture();
    let binding = OwnedAudioBinding {
        lattice: AudioPlacementTemplate {
            reference: AudioReferenceClock {
                timing: clock(0),
                root: AudioClockRoot::ProjectRootRoundEven,
                physical: node("top"),
                recipe: AudioRecipeKind::Node,
            },
            reference_local_offset: ExactRatio::ZERO,
            arguments: vec![],
            births: vec![],
            gap_after: None,
        },
        resume: None,
        reanchors: vec![],
    };
    assert!(
        AudioBindingState::new_with_sound_clocks(
            vec![record(&doc, 0)],
            BTreeMap::from([(node("top"), binding)]),
            BTreeMap::new(),
            clocks(vec![clock(0)]),
        )
        .is_err()
    );
}

#[test]
fn processing_comparison_ignores_policy_but_rejects_topology_and_duration_changes() {
    let doc = fixture();
    let before = FrozenAudioLayout::capture(&doc).unwrap();
    let mut current = doc.clone();
    current.nodes.insert(node("lead"), hold(2));
    current.nodes.get_mut(&node("owner")).unwrap().label = "Renamed".into();
    current
        .nodes
        .get_mut(&node("owner"))
        .unwrap()
        .audio_edges
        .node_start = AudioEdgePolicy::Hard;
    let mut wire = serde_json::to_value(FrozenAudioLayout::capture(&current).unwrap()).unwrap();
    wire["nodes"]["owner"]["kind"]["audio"] = serde_json::json!({"type":"room_tone"});
    let current = FrozenAudioLayout::from_json(&wire.to_string()).unwrap();
    assert!(
        before
            .validate_sound_clock_owner(&current, &node("owner"), 100)
            .unwrap()
            < 100
    );
    assert!(
        before
            .validate_sound_clock_owner(&current, &node("owner"), 1)
            .is_err()
    );
    assert!(
        before
            .validate_sound_clock_owner(&current, &node("missing"), 100)
            .is_err()
    );
    let mut altered = doc.clone();
    altered.nodes.insert(node("sibling"), hold(5));
    assert!(
        before
            .validate_sound_clock_owner(
                &FrozenAudioLayout::capture(&altered).unwrap(),
                &node("owner"),
                100
            )
            .is_err()
    );
    let mut reordered = doc.clone();
    reordered.nodes.insert(
        node("top"),
        BeatNode::sequence("Group", vec![node("sibling"), node("owner")]),
    );
    assert!(
        before
            .validate_sound_clock_owner(
                &FrozenAudioLayout::capture(&reordered).unwrap(),
                &node("owner"),
                100
            )
            .is_err()
    );
    let mut moved = doc;
    moved.nodes.insert(
        node("root"),
        BeatNode::sequence("Root", vec![node("lead"), node("owner"), node("top")]),
    );
    moved.nodes.insert(
        node("top"),
        BeatNode::sequence("Group", vec![node("sibling")]),
    );
    assert!(
        before
            .validate_sound_clock_owner(
                &FrozenAudioLayout::capture(&moved).unwrap(),
                &node("owner"),
                100
            )
            .is_err()
    );
}

fn repeated_fixture() -> ProjectDocument {
    let mut doc = fixture();
    doc.nodes.insert(
        node("top"),
        BeatNode {
            kind: NodeKind::Retime {
                child: node("repeat"),
                duration: frames(6),
                mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(8)).unwrap(),
                pitch: PitchPolicy::Preserve,
                purpose: RetimePurpose::Edit,
            },
            ..BeatNode::sequence("Processed", vec![])
        },
    );
    doc.nodes.insert(
        node("repeat"),
        BeatNode {
            kind: NodeKind::Repeat {
                child: node("body"),
                iterations: IterationOrder::new(revision("plays"), 1_000_000_000).unwrap(),
                gap: Some(HoldRecipe {
                    duration: frames(1),
                    video: HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                }),
            },
            ..BeatNode::sequence("Repeat", vec![])
        },
    );
    doc.nodes.insert(
        node("body"),
        BeatNode::sequence("Body", vec![node("owner"), node("sibling")]),
    );
    doc.nodes.insert(node("override"), hold(8));
    doc.nodes.insert(node("gap"), hold(2));
    let iteration = |ordinal| IterationId {
        allocation: revision("plays"),
        ordinal,
    };
    doc.overrides.insert(
        node("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration(4),
            root: node("override"),
        }])
        .unwrap(),
    );
    doc.gap_overrides.insert(
        node("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration(3),
            root: node("gap"),
        }])
        .unwrap(),
    );
    doc.validate().unwrap();
    doc
}

#[test]
fn compact_repeat_overrides_and_preserve_processing_are_compared_without_play_expansion() {
    let doc = repeated_fixture();
    let before = FrozenAudioLayout::capture(&doc).unwrap();
    let mut current = doc.clone();
    current.nodes.insert(node("lead"), hold(3));
    let after = FrozenAudioLayout::capture(&current).unwrap();
    for owner in ["owner", "repeat", "body", "override", "gap"] {
        assert!(
            before
                .validate_sound_clock_owner(&after, &node(owner), 150)
                .unwrap()
                < 150
        );
    }
    current.audio_bindings = state(vec![record(&doc, 0)], clocks(vec![clock(0)]));
    current.validate().unwrap();
    for change in ["pitch", "mapping", "gap", "plays", "override"] {
        let mut changed = serde_json::to_value(&before).unwrap();
        match change {
            "pitch" => {
                changed["nodes"]["top"]["kind"]["pitch"] =
                    serde_json::to_value(PitchPolicy::FollowSpeed).unwrap()
            }
            "mapping" => {
                changed["nodes"]["top"]["kind"]["mapping"] =
                    serde_json::to_value(FrameRange::new(ProjectFrame(0), ProjectFrame(7)).unwrap())
                        .unwrap()
            }
            "gap" => {
                let mut altered = doc.clone();
                if let NodeKind::Hold { recipe } =
                    &mut altered.nodes.get_mut(&node("gap")).unwrap().kind
                {
                    recipe.duration = frames(3);
                }
                changed =
                    serde_json::to_value(FrozenAudioLayout::capture(&altered).unwrap()).unwrap();
            }
            "plays" => {
                let mut altered = doc.clone();
                if let NodeKind::Repeat { iterations, .. } =
                    &mut altered.nodes.get_mut(&node("repeat")).unwrap().kind
                {
                    *iterations =
                        IterationOrder::new(revision("different-plays"), 1_000_000_000).unwrap();
                }
                for (map, ordinal, root) in [
                    (&mut altered.overrides, 4, "override"),
                    (&mut altered.gap_overrides, 3, "gap"),
                ] {
                    map.insert(
                        node("repeat"),
                        PlayOverrides::try_from(vec![PlayOverride {
                            iteration: IterationId {
                                allocation: revision("different-plays"),
                                ordinal,
                            },
                            root: node(root),
                        }])
                        .unwrap(),
                    );
                }
                changed =
                    serde_json::to_value(FrozenAudioLayout::capture(&altered).unwrap()).unwrap();
            }
            "override" => {
                let mut altered = doc.clone();
                altered.overrides.insert(
                    node("repeat"),
                    PlayOverrides::try_from(vec![PlayOverride {
                        iteration: IterationId {
                            allocation: revision("plays"),
                            ordinal: 5,
                        },
                        root: node("override"),
                    }])
                    .unwrap(),
                );
                changed =
                    serde_json::to_value(FrozenAudioLayout::capture(&altered).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        let changed = FrozenAudioLayout::from_json(&changed.to_string()).unwrap();
        assert!(
            before
                .validate_sound_clock_owner(&changed, &node("owner"), 150)
                .is_err(),
            "accepted {change}"
        );
    }
}

#[test]
fn source_primary_placement_is_not_an_independent_sound_clock() {
    let doc = fixture();
    let mut wire = serde_json::to_value(FrozenAudioLayout::capture(&doc).unwrap()).unwrap();
    wire["nodes"]["owner"]["kind"] = serde_json::json!({"type":"source", "placement":null});
    let before = FrozenAudioLayout::from_json(&wire.to_string()).unwrap();
    wire["nodes"]["owner"]["kind"]["placement"] =
        serde_json::to_value(ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::ONE).unwrap())
            .unwrap();
    let after = FrozenAudioLayout::from_json(&wire.to_string()).unwrap();
    before
        .validate_sound_clock_owner(&after, &node("owner"), 100)
        .unwrap();
}
