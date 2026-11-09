use crate::*;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn silence(duration: i64) -> HoldRecipe {
    HoldRecipe {
        duration: FrameDuration::new(duration).unwrap(),
        picture_context: None,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}
fn room_tone(start: i64, end: i64) -> HoldAudio {
    HoldAudio::RoomTone {
        source: SourceAudio {
            asset: AssetId::new("original").unwrap(),
            span: span(start, end),
        },
    }
}

fn document(children: &[&str], entries: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut result = ProjectDocument::new(
        ProjectId::new("pause-sound").unwrap(),
        revision("base"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    result.nodes.insert(
        node("root"),
        BeatNode::sequence("Root", children.iter().map(|name| node(name)).collect()),
    );
    result
        .nodes
        .extend(entries.into_iter().map(|(name, value)| (node(name), value)));
    let audio = span(0, 480_000);
    let asset = AssetId::new("original").unwrap();
    result.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            audio: Some(audio),
            video: None,
            frame_count: None,
            still_image: false,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    result.sounds.insert(
        SoundId::new("effect").unwrap(),
        SoundEvent {
            owner: node("root"),
            label: "Effect".into(),
            source: SourceAudio {
                asset,
                span: span(0, 4_800),
            },
            mapping: SourceAudioMapping::natural_rate(
                span(0, 4_800),
                FrameRate::new(30, 1).unwrap(),
            )
            .unwrap(),
            offset: AudioSample(0),
            treatments: AudioTreatments::default(),
            start_edge: AudioEdgePolicy::Automatic,
            end_edge: AudioEdgePolicy::Automatic,
            overflow: SoundOverflowPolicy::Reject,
        },
    );
    result.validate().unwrap();
    result
}

fn repeated(child: &str, plays: u32, gap: Option<HoldRecipe>) -> BeatNode {
    let mut value = BeatNode::sequence("Repeat", vec![]);
    value.kind = NodeKind::Repeat {
        child: node(child),
        iterations: IterationOrder::new(revision("plays"), plays).unwrap(),
        gap,
        escalation: None,
    };
    value
}

fn iterations(document: &ProjectDocument) -> IterationOrder {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node("repeat")].kind else {
        panic!("Repeat")
    };
    iterations.clone()
}

/// Apply through the ordinary transaction path, checking the wire and that
/// Undo restores the exact prior document.
fn edit(document: &ProjectDocument, new_revision: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(new_revision),
        command,
    };
    let restored: CommandRequest =
        serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    assert_eq!(restored, request);
    let transaction = crate::apply(document, &restored).unwrap();
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    result
}

#[test]
fn default_gap_sound_changes_every_default_gap_and_nothing_else() {
    let base = document(
        &["repeat", "suffix"],
        vec![
            ("repeat", repeated("a", 4, Some(silence(4)))),
            ("a", BeatNode::hold("Held", silence(10))),
            ("suffix", BeatNode::hold("Suffix", silence(20))),
        ],
    );
    let plays = iterations(&base);
    // An explicit branch after the second play, created through the gap
    // setter so the Repeat also carries retained gap clocks.
    let mut laddered = edit(
        &base,
        "ladder",
        Command::SetRepeatGaps {
            node: node("repeat"),
            gap: Some(silence(4)),
            branches: vec![RepeatGapHold {
                after: plays.at(1).unwrap(),
                id: node("branch"),
                hold: silence(2),
            }],
            timing: AudioTimingId {
                allocation: revision("ladder"),
                ordinal: 0,
            },
        },
    );
    let sound = SoundId::new("effect").unwrap();
    let default_gap = SoundHoldIssuer::RepeatGap {
        instance: InstancePath {
            node: node("repeat"),
            repeats: vec![],
        },
        gap_after: plays.at(0).unwrap(),
    };
    let branch = SoundHoldIssuer::Node {
        instance: InstancePath {
            node: node("branch"),
            repeats: vec![RepeatInstance {
                node: node("repeat"),
                iteration: plays.at(1).unwrap(),
            }],
        },
    };
    laddered.sound_allowances.insert(
        sound.clone(),
        SoundHoldAllowances::try_from(vec![default_gap.clone(), branch.clone()]).unwrap(),
    );
    laddered.validate().unwrap();
    let target = laddered.pause_sound_target(&node("repeat")).unwrap();
    assert_eq!(
        target,
        PauseSoundTarget::RepeatGap {
            node: node("repeat")
        }
    );

    let after = edit(&laddered, "room", target.command(room_tone(4_800, 9_600)));
    let NodeKind::Repeat {
        gap: Some(gap),
        iterations: after_plays,
        ..
    } = &after.nodes()[&node("repeat")].kind
    else {
        panic!("Repeat keeps its gap")
    };
    assert_eq!(gap.audio, room_tone(4_800, 9_600));
    assert_eq!(
        gap.duration,
        silence(4).duration,
        "gap duration is unchanged"
    );
    assert_eq!(gap.video, HoldVideo::Background);
    assert_eq!(after_plays, &plays, "stable plays survive");
    assert_eq!(after.durations().unwrap(), laddered.durations().unwrap());
    assert_eq!(
        after.audio_bindings(),
        laddered.audio_bindings(),
        "retained gap and suffix clocks are unchanged"
    );
    assert_eq!(after.gap_overrides(), laddered.gap_overrides());
    assert_eq!(
        after.nodes()[&node("branch")],
        laddered.nodes()[&node("branch")],
        "the explicit branch keeps its own silence"
    );
    assert_eq!(
        after.sound_allowances()[&sound],
        SoundHoldAllowances::try_from(vec![branch]).unwrap(),
        "only the default gap's now-obsolete permission is retired"
    );
    assert_eq!(
        target.recipe(&after).map(|recipe| &recipe.audio),
        Some(&room_tone(4_800, 9_600))
    );
    // Silence again is an ordinary reversible edit and recreates nothing.
    let silent = edit(&after, "quiet", target.command(HoldAudio::Silence));
    assert!(
        silent
            .sound_allowances()
            .get(&sound)
            .is_some_and(|values| !values.contains(&default_gap))
    );
}

#[test]
fn gap_sound_refuses_repeats_without_a_gap_and_other_kinds() {
    let base = document(
        &["repeat", "pause"],
        vec![
            ("repeat", repeated("a", 3, None)),
            ("a", BeatNode::hold("Held", silence(10))),
            ("pause", BeatNode::hold("Pause", silence(5))),
        ],
    );
    assert_eq!(base.pause_sound_target(&node("repeat")), None);
    for (target, code) in [
        ("repeat", EditErrorCode::SelectionUnavailable),
        ("pause", EditErrorCode::WrongNodeKind),
    ] {
        let error = crate::apply(
            &base,
            &CommandRequest {
                project_id: base.project_id().clone(),
                expected_revision: base.revision_id().clone(),
                new_revision: revision("refused"),
                command: Command::SetRepeatGapAudio {
                    node: node(target),
                    audio: room_tone(0, 4_800),
                },
            },
        )
        .unwrap_err();
        assert_eq!(error.code, code, "{target}");
    }
}

#[test]
fn a_split_fragment_names_only_its_own_hold() {
    let base = document(
        &["pause"],
        vec![("pause", BeatNode::hold("Pause", silence(12)))],
    );
    let split = edit(
        &base,
        "split",
        Command::Split {
            node: node("pause"),
            at: FrameDuration::new(5).unwrap(),
            identities: SplitIdentities {
                nodes: ["left", "right", "copy", "spare"].map(node).to_vec(),
            },
        },
    );
    let NodeKind::Sequence { children } = &split.nodes()[&node("root")].kind else {
        panic!("root Sequence")
    };
    assert_eq!(children.len(), 2, "two fragments");
    let targets: Vec<_> = children
        .iter()
        .map(|fragment| split.pause_sound_target(fragment).unwrap())
        .collect();
    let PauseSoundTarget::Hold {
        node: right,
        fragment: true,
    } = &targets[1]
    else {
        panic!("a fragment names its Hold: {targets:?}")
    };
    assert_ne!(targets[0].node(), right, "each fragment owns its Hold copy");

    let after = edit(&split, "room", targets[1].command(room_tone(0, 4_800)));
    assert_eq!(
        targets[1].recipe(&after).map(|recipe| &recipe.audio),
        Some(&room_tone(0, 4_800))
    );
    assert_eq!(
        targets[0].recipe(&after),
        targets[0].recipe(&split),
        "the other fragment keeps its silence"
    );
    assert_eq!(after.durations().unwrap(), split.durations().unwrap());
    assert_eq!(after.audio_bindings(), split.audio_bindings());
    assert_eq!(
        targets[1].recipe(&after).unwrap().duration,
        targets[1].recipe(&split).unwrap().duration
    );
}
