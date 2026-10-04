use super::*;
use crate::*;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn hold(value: i64) -> BeatNode {
    BeatNode::hold(
        "Held",
        HoldRecipe {
            duration: frames(value),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn tree(children: &[&str], entries: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut result = ProjectDocument::new(
        ProjectId::new("repeat").unwrap(),
        revision("base"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
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
    result.validate().unwrap();
    result
}
fn repeated(child: &str, plays: u32, gap: Option<HoldRecipe>) -> BeatNode {
    let mut value = BeatNode::sequence("Inner", vec![]);
    value.kind = NodeKind::Repeat {
        child: node(child),
        iterations: IterationOrder::new(revision("old-plays"), plays).unwrap(),
        gap,
        escalation: None,
    };
    value
}
fn wrapped(
    document: &ProjectDocument,
    parent: &str,
    selection: SliceCaptureSelection,
    plays: u32,
) -> Command {
    let plan = document
        .repeat_selection(&node(parent), &selection, plays)
        .unwrap();
    Command::RepeatSelection {
        parent: node(parent),
        selection,
        plays,
        identities: RepeatSelectionIdentities {
            repeat: node("wrapped"),
            group: plan.needs_group.then(|| node("body")),
            split: SplitIdentities {
                nodes: (0..plan.required_split_ids)
                    .map(|n| node(&format!("split-{n}")))
                    .collect(),
            },
        },
        timing: AudioTimingId {
            allocation: revision("edit"),
            ordinal: 0,
        },
    }
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    let new_revision = match &command {
        Command::RepeatSelection { timing, .. } | Command::SetRepeatPlays { timing, .. } => {
            timing.allocation.clone()
        }
        _ => revision("setup"),
    };
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision,
        command,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    let request = request(document, command);
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
fn set(document: &ProjectDocument, plays: u32, allocation: &str) -> ProjectDocument {
    edit(
        document,
        Command::SetRepeatPlays {
            node: node("wrapped"),
            plays,
            timing: AudioTimingId {
                allocation: revision(allocation),
                ordinal: 0,
            },
        },
    )
}
fn child(value: &str) -> SliceCaptureSelection {
    SliceCaptureSelection::Child { node: node(value) }
}

#[test]
fn exact_forest_repeat_retains_empty_endpoints_in_its_body() {
    let before = tree(
        &["prefix", "left", "body", "right", "suffix"],
        vec![
            ("prefix", hold(1)),
            ("left", BeatNode::sequence("Left", vec![])),
            ("body", hold(3)),
            ("right", BeatNode::sequence("Right", vec![])),
            ("suffix", hold(2)),
        ],
    );
    let selection = SliceCaptureSelection::Children {
        first: node("left"),
        last: node("right"),
    };
    let plan = before
        .repeat_selection(&node("root"), &selection, 3)
        .unwrap();
    assert!(plan.needs_group);
    assert_eq!(plan.required_split_ids, 0);
    // The fixture's body name is also the helper's fresh group name.
    let mut command = wrapped(&before, "root", selection, 3);
    let Command::RepeatSelection { identities, .. } = &mut command else {
        panic!()
    };
    identities.group = Some(node("forest-body"));
    let after = edit(&before, command);
    let NodeKind::Sequence { children } = &after.nodes()[&node("forest-body")].kind else {
        panic!()
    };
    assert_eq!(*children, [node("left"), node("body"), node("right")]);
    assert_eq!(after.duration().unwrap(), frames(12));
    let empty = SliceCaptureSelection::Children {
        first: node("left"),
        last: node("left"),
    };
    assert!(before.repeat_selection(&node("root"), &empty, 2).is_err());
}
fn selected(start: i64, end: i64) -> SliceCaptureSelection {
    SliceCaptureSelection::Range {
        range: range(start, end),
    }
}
fn repeat(document: &ProjectDocument) -> (&NodeId, &IterationOrder) {
    let NodeKind::Repeat {
        child, iterations, ..
    } = &document.nodes()[&node("wrapped")].kind
    else {
        panic!()
    };
    (child, iterations)
}

#[test]
fn exact_child_identity_and_range_body_are_distinct_even_at_shared_empty_boundaries() {
    let document = tree(
        &["empty-left", "a", "empty-middle", "b", "empty-right"],
        vec![
            ("empty-left", BeatNode::sequence("Empty", vec![])),
            ("a", hold(3)),
            ("empty-middle", BeatNode::sequence("Empty", vec![])),
            ("b", hold(4)),
            ("empty-right", BeatNode::sequence("Empty", vec![])),
        ],
    );
    let whole = edit(&document, wrapped(&document, "root", child("a"), 3));
    assert_eq!(repeat(&whole).0, &node("a"));
    assert_eq!(whole.nodes()[&node("a")], document.nodes()[&node("a")]);
    assert_eq!(whole.nodes().len(), document.nodes().len() + 1);
    assert_eq!(whole.duration().unwrap(), frames(13));
    let ranged = edit(&document, wrapped(&document, "root", selected(0, 7), 2));
    assert_eq!(repeat(&ranged).0, &node("body"));
    assert_eq!(
        ranged.nodes()[&node("body")].kind,
        NodeKind::Sequence {
            children: vec![node("a"), node("empty-middle"), node("b")]
        }
    );
    assert_eq!(
        ranged.nodes()[&node("root")].kind,
        NodeKind::Sequence {
            children: vec![node("empty-left"), node("wrapped"), node("empty-right")]
        }
    );
    assert!(
        document
            .repeat_selection(&node("root"), &child("empty-left"), 2)
            .is_err()
    );
}

#[test]
fn partial_composite_ranges_keep_complete_contexts_and_compact_nested_iterations() {
    let mut retime = BeatNode::sequence("Rate", vec![]);
    retime.kind = NodeKind::Retime {
        child: node("held"),
        duration: frames(10),
        mapping: range(1, 6),
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    for (target, descendants) in [
        (hold(10), vec![]),
        (
            BeatNode::sequence("Group", vec![node("held"), node("tail")]),
            vec![("held", hold(6)), ("tail", hold(4))],
        ),
        (
            repeated("held", 1_000_000_000, None),
            vec![("held", hold(2))],
        ),
        (retime, vec![("held", hold(8))]),
    ] {
        let mut entries = vec![("target", target.clone()), ("suffix", hold(3))];
        entries.extend(descendants);
        let document = tree(&["target", "suffix"], entries);
        let after = edit(&document, wrapped(&document, "root", selected(2, 7), 3));
        assert_eq!(
            after.duration().unwrap().frames(),
            document.duration().unwrap().frames() + 10
        );
        assert_eq!(after.nodes()[&node("target")], target);
        assert_eq!(after.durations().unwrap()[repeat(&after).0], frames(5));
        assert_eq!(repeat(&after).1.len(), 3);
        assert!(after.nodes().len() < 30);
        assert!(!after.audio_bindings().is_empty());
        let suffix = &after.audio_bindings().bindings()[&node("suffix")];
        assert_eq!(suffix.reanchors.len(), 1);
    }
}

#[test]
fn first_play_marks_keep_concrete_occurrences_and_suffix_resume_is_captured_once() {
    let document = tree(
        &["prefix", "a", "b", "suffix"],
        vec![
            ("prefix", hold(1)),
            ("a", hold(3)),
            ("b", hold(4)),
            ("suffix", hold(5)),
        ],
    );
    let document = edit(
        &document,
        Command::SetMark {
            id: MarkId::new("mark").unwrap(),
            owner: node("root"),
            label: "Mark".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: InstancePath {
                        node: node("b"),
                        repeats: vec![],
                    },
                    position: ExactRatio::ONE,
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    );
    let after = edit(&document, wrapped(&document, "root", selected(1, 8), 3));
    let mark = &after.marks()[&MarkId::new("mark").unwrap()];
    let Anchor::Occurrence { instance, position } = &mark.boundary.coordinate else {
        panic!()
    };
    assert_eq!(*position, ExactRatio::ONE);
    assert_eq!(instance.node, node("b"));
    assert_eq!(
        instance.repeats,
        vec![RepeatInstance {
            node: node("wrapped"),
            iteration: repeat(&after).1.at(0).unwrap()
        }]
    );
    assert_eq!(mark.binding_count(), 1);
    for owner in ["a", "b"] {
        let resolved = after
            .audio_bindings()
            .resolve(
                &node(owner),
                &InstancePath {
                    node: node(owner),
                    repeats: vec![instance.repeats[0].clone()],
                },
                10_000,
            )
            .unwrap();
        assert_eq!(
            resolved.lattice.grid_rule,
            AudioBindingGridRule::RootRoundEven
        );
    }
    assert_eq!(
        after.audio_bindings().bindings()[&node("suffix")]
            .reanchors
            .len(),
        1
    );
    let one = edit(&document, wrapped(&document, "root", selected(1, 8), 1));
    assert_eq!(one.duration().unwrap(), document.duration().unwrap());
    assert!(
        one.audio_bindings().bindings()[&node("suffix")]
            .reanchors
            .is_empty()
    );
}

fn with_sound(mut document: ProjectDocument) -> ProjectDocument {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 48_000,
            time_base,
        },
    )
    .unwrap();
    let asset = AssetId::new("sound-source").unwrap();
    document.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "Effect".into(),
            content_hash: "a".repeat(64),
            audio: Some(span),
            video: None,
            frame_count: None,
            still_image: false,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    let mapping =
        SourceAudioMapping::natural_rate(span, document.presentation_basis().frame_rate).unwrap();
    document.sounds.insert(
        SoundId::new("effect").unwrap(),
        SoundEvent {
            owner: node("root"),
            label: "Effect".into(),
            source: SourceAudio { asset, span },
            mapping,
            offset: AudioSample(137),
            gain_millidecibels: -3000,
            start_edge: AudioEdgePolicy::Automatic,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        },
    );
    document.validate().unwrap();
    document
}

#[test]
fn root_sound_routes_and_concrete_allowances_follow_only_the_retained_first_play() {
    let mut document = with_sound(tree(
        &["a", "suffix"],
        vec![("a", hold(40)), ("suffix", hold(40))],
    ));
    let sound = SoundId::new("effect").unwrap();
    document.sound_allowances.insert(
        sound.clone(),
        SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {
            instance: InstancePath {
                node: node("a"),
                repeats: vec![],
            },
        }])
        .unwrap(),
    );
    document.validate().unwrap();
    let after = edit(&document, wrapped(&document, "root", child("a"), 3));
    assert_eq!(after.sounds(), document.sounds());
    assert_eq!(after.sound_routes()[&sound].edits.len(), 1);
    assert_eq!(
        after.sound_routes()[&sound].edits[0].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(40),
            duration: frames(80)
        }
    );
    let allowances = &after.sound_allowances()[&sound];
    assert_eq!(allowances.len(), 1);
    assert_eq!(
        allowances.iter().next().unwrap().instance().repeats,
        vec![RepeatInstance {
            node: node("wrapped"),
            iteration: repeat(&after).1.at(0).unwrap()
        }]
    );
    let shrunk = set(&after, 1, "shrink");
    assert_eq!(shrunk.sound_routes()[&sound].edits.len(), 2);
    assert_eq!(
        shrunk.sound_routes()[&sound].edits[1].operation,
        RootSoundOperation::Delete {
            range: range(40, 120)
        }
    );
    assert_eq!(repeat(&shrunk).1.at(0), repeat(&after).1.at(0));
    assert_eq!(shrunk.sound_allowances()[&sound], *allowances);
    let grown = set(&shrunk, 2, "grow");
    assert_eq!(grown.sound_routes()[&sound].edits.len(), 3);
    assert_eq!(
        grown.sound_routes()[&sound].edits[2].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(40),
            duration: frames(40)
        }
    );
    assert_ne!(repeat(&grown).1.at(1), repeat(&after).1.at(1));
    assert_eq!(
        grown.audio_bindings().bindings()[&node("suffix")]
            .reanchors
            .len(),
        3
    );
    let unchanged = set(&grown, 2, "same");
    let mut expected = grown.clone();
    expected.revision_id = revision("same");
    assert_eq!(unchanged, expected);
}

#[test]
fn count_setter_preserves_gap_and_sparse_survivors_and_activates_old_terminal_gap() {
    let recipe = HoldRecipe {
        duration: frames(2),
        picture_context: None,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    };
    let mut document = tree(
        &["wrapped", "suffix"],
        vec![
            ("wrapped", repeated("a", 2, Some(recipe.clone()))),
            ("a", hold(3)),
            ("suffix", hold(10)),
        ],
    );
    let first = repeat(&document).1.at(0).unwrap();
    let last = repeat(&document).1.at(1).unwrap();
    document.nodes.insert(node("override"), hold(5));
    document.nodes.insert(node("gap-override"), hold(7));
    document.overrides.insert(
        node("wrapped"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: first.clone(),
            root: node("override"),
        }])
        .unwrap(),
    );
    document.gap_overrides.insert(
        node("wrapped"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: last.clone(),
            root: node("gap-override"),
        }])
        .unwrap(),
    );
    document.validate().unwrap();
    let after = set(&document, 3, "grow");
    assert_eq!(after.durations().unwrap()[&node("wrapped")], frames(20)); // 5 + 2 + 3 + 7 + 3
    assert_eq!(repeat(&after).1.at(0), Some(first.clone()));
    assert_eq!(repeat(&after).1.at(1), Some(last));
    assert!(
        matches!(&after.nodes()[&node("wrapped")].kind, NodeKind::Repeat { gap: Some(gap), .. } if gap == &recipe)
    );
    let shrunk = set(&after, 1, "shrink");
    assert_eq!(shrunk.durations().unwrap()[&node("wrapped")], frames(5));
    assert_eq!(
        shrunk.overrides()[&node("wrapped")].get(&first),
        Some(&node("override"))
    );
    assert!(!shrunk.nodes().contains_key(&node("gap-override")));
}

#[test]
fn nested_sequence_terminal_repeat_moves_ancestor_suffix_and_refuses_repeated_scope() {
    let document = tree(
        &["prefix", "group", "suffix"],
        vec![
            ("prefix", hold(2)),
            (
                "group",
                BeatNode::sequence("Group", vec![node("a"), node("b")]),
            ),
            ("a", hold(3)),
            ("b", hold(4)),
            ("suffix", hold(5)),
        ],
    );
    let after = edit(&document, wrapped(&document, "group", selected(3, 9), 2));
    assert_eq!(after.duration().unwrap(), frames(20));
    assert_eq!(
        after.audio_bindings().bindings()[&node("suffix")]
            .reanchors
            .len(),
        1
    );
    assert!(
        after.audio_bindings().bindings()[&node("prefix")]
            .reanchors
            .is_empty()
    );
    let grown = set(&after, 3, "grow-nested");
    assert_eq!(grown.duration().unwrap(), frames(26));
    assert_eq!(
        grown.audio_bindings().bindings()[&node("suffix")]
            .reanchors
            .len(),
        2
    );
    let Command::RepeatSelection { identities, .. } = wrapped(&document, "root", child("group"), 2)
    else {
        panic!()
    };
    let repeated = edit(
        &document,
        Command::RepeatSelection {
            parent: node("root"),
            selection: child("group"),
            plays: 2,
            identities,
            timing: AudioTimingId {
                allocation: revision("edit"),
                ordinal: 0,
            },
        },
    );
    assert!(
        repeated
            .repeat_selection(&node("group"), &child("a"), 2)
            .is_err()
    );
    assert!(
        crate::apply(
            &repeated,
            &request(
                &repeated,
                Command::SetRepeatPlays {
                    node: node("a"),
                    plays: 2,
                    timing: AudioTimingId {
                        allocation: revision("refused"),
                        ordinal: 0
                    },
                }
            )
        )
        .is_err()
    );
}

#[test]
fn count_shrink_removes_only_permissions_for_retired_plays_and_terminal_gaps() {
    let recipe = HoldRecipe {
        duration: frames(2),
        picture_context: None,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    };
    let mut document = with_sound(tree(
        &["wrapped", "suffix"],
        vec![
            ("wrapped", repeated("a", 3, Some(recipe))),
            ("a", hold(10)),
            ("suffix", hold(40)),
        ],
    ));
    let sound = SoundId::new("effect").unwrap();
    let iterations = repeat(&document).1.clone();
    let first = SoundHoldIssuer::Node {
        instance: InstancePath {
            node: node("a"),
            repeats: vec![RepeatInstance {
                node: node("wrapped"),
                iteration: iterations.at(0).unwrap(),
            }],
        },
    };
    document.sound_allowances.insert(
        sound.clone(),
        SoundHoldAllowances::try_from(vec![
            first.clone(),
            SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: node("a"),
                    repeats: vec![RepeatInstance {
                        node: node("wrapped"),
                        iteration: iterations.at(2).unwrap(),
                    }],
                },
            },
            SoundHoldIssuer::RepeatGap {
                instance: InstancePath {
                    node: node("wrapped"),
                    repeats: vec![],
                },
                gap_after: iterations.at(1).unwrap(),
            },
        ])
        .unwrap(),
    );
    document.validate().unwrap();
    let after = set(&document, 2, "shrink");
    assert_eq!(
        after.sound_allowances()[&sound],
        SoundHoldAllowances::try_from(vec![first]).unwrap()
    );
}

#[test]
fn invalid_identity_context_counts_and_wire_are_atomic() {
    let document = tree(&["a", "suffix"], vec![("a", hold(8)), ("suffix", hold(2))]);
    let original = document.clone();
    for selection in [
        selected(4, 4),
        selected(0, 11),
        child("missing"),
        child("root"),
    ] {
        assert!(
            document
                .repeat_selection(&node("root"), &selection, 2)
                .is_err()
        );
    }
    assert!(
        document
            .repeat_selection(&node("root"), &child("a"), 0)
            .is_err()
    );
    let mut command = wrapped(&document, "root", selected(1, 6), 2);
    let Command::RepeatSelection { identities, .. } = &mut command else {
        panic!()
    };
    identities.group = None;
    assert_eq!(
        crate::apply(&document, &request(&document, command))
            .unwrap_err()
            .code,
        EditErrorCode::InvalidCommand
    );
    for invalid_id in [node("a"), node("body"), node("split-0")] {
        let mut command = wrapped(&document, "root", selected(1, 6), 2);
        let Command::RepeatSelection { identities, .. } = &mut command else {
            panic!()
        };
        identities.repeat = invalid_id;
        assert_eq!(
            crate::apply(&document, &request(&document, command))
                .unwrap_err()
                .code,
            EditErrorCode::IdentityConflict
        );
    }
    for delta in [-1, 1] {
        let mut command = wrapped(&document, "root", selected(1, 6), 2);
        let Command::RepeatSelection { identities, .. } = &mut command else {
            panic!()
        };
        if delta < 0 {
            identities.split.nodes.pop();
        } else {
            identities.split.nodes.push(node("extra"));
        }
        assert!(crate::apply(&document, &request(&document, command)).is_err());
    }
    let mut command = wrapped(&document, "root", child("a"), 2);
    let Command::RepeatSelection { timing, .. } = &mut command else {
        panic!()
    };
    timing.ordinal = u32::MAX;
    assert_eq!(
        crate::apply(&document, &request(&document, command))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    let huge = tree(&["a"], vec![("a", hold(i64::MAX))]);
    assert!(
        huge.repeat_selection(&node("root"), &child("a"), 2)
            .is_err()
    );
    let mut wire = serde_json::to_value(wrapped(&document, "root", child("a"), 2)).unwrap();
    wire["identities"]["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Command>(wire).is_err());
    assert_eq!(document, original);
}
