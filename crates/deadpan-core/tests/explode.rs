//! Explode and Duplicate: exact structure, attachments, clocks and history.
use std::collections::BTreeMap;
use std::num::NonZeroU32;

use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn mark_id(value: &str) -> MarkId {
    MarkId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn silent(duration: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: frames(duration),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}

fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name).unwrap(),
        command,
    }
}

/// Commit through the serialized request, check exact patches and history.
fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let request = request(document, name, command);
    let request: CommandRequest =
        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
    let (transaction, result) = apply_with_result(document, &request).unwrap();
    assert_eq!(transaction.forward.apply(document).unwrap(), result);
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    result
}

fn refused(document: &ProjectDocument, command: Command) -> EditError {
    apply(document, &request(document, "refused", command)).unwrap_err()
}

fn base(rate: FrameRate) -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("explode").unwrap(),
        RevisionId::new("r0").unwrap(),
        PresentationBasis {
            width: 64,
            height: 36,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap()
}

fn holds(names: &[(&str, i64)]) -> Subtree {
    let children: Vec<_> = names.iter().map(|(name, _)| id(name)).collect();
    let mut nodes: BTreeMap<_, _> = names
        .iter()
        .map(|(name, duration)| (id(name), BeatNode::hold(*name, silent(*duration))))
        .collect();
    nodes.insert(id("group"), BeatNode::sequence("Group", children));
    Subtree {
        root: id("group"),
        nodes,
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
    }
}

/// `[lead, repeat(group[h1, h2]) x plays with a gap]`.
fn repeated(plays: u32, gap: i64) -> ProjectDocument {
    let document = edit(
        &base(FrameRate::new(30_000, 1001).unwrap()),
        "r1",
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: holds(&[("h1", 2), ("h2", 3)]),
        },
    );
    let document = edit(
        &document,
        "r2",
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("lead"),
                nodes: BTreeMap::from([(id("lead"), BeatNode::hold("Lead", silent(1)))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    edit(
        &document,
        "r3",
        Command::WrapRepeat {
            node: id("group"),
            id: id("repeat"),
            plays,
            gap: (gap > 0).then(|| silent(gap)),
            anchor_policy: WrapAnchorPolicy::First,
        },
    )
}

fn play(document: &ProjectDocument, repeat: &str, index: u32) -> IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id(repeat)].kind else {
        panic!("{repeat} is not a Repeat")
    };
    iterations.at(index).unwrap()
}

fn explode_command(document: &ProjectDocument, repeat: &str, name: &str) -> Command {
    let needs = document.explode_requirements(&id(repeat)).unwrap();
    Command::Explode {
        node: id(repeat),
        identities: OccurrenceIdentities {
            nodes: (0..needs.nodes)
                .map(|n| id(&format!("{name}-n{n}")))
                .collect(),
            marks: (0..needs.marks)
                .map(|n| mark_id(&format!("{name}-m{n}")))
                .collect(),
        },
        timing: AudioTimingId {
            allocation: RevisionId::new(name).unwrap(),
            ordinal: 0,
        },
    }
}

fn explode(document: &ProjectDocument, repeat: &str, name: &str) -> ProjectDocument {
    let result = edit(document, name, explode_command(document, repeat, name));
    assert_eq!(result.duration().unwrap(), document.duration().unwrap());
    assert!(matches!(
        result.nodes()[&id(repeat)].kind,
        NodeKind::Sequence { .. }
    ));
    result
}

fn children(document: &ProjectDocument, node: &str) -> Vec<NodeId> {
    document.children(&id(node)).cloned().collect()
}

fn caption(text: &str, reveal: Option<u32>) -> Caption {
    Caption {
        range: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
        text: text.into(),
        placement: if reveal.is_some() {
            CaptionPlacement::Top
        } else {
            CaptionPlacement::Bottom
        },
        reveal: reveal.and_then(NonZeroU32::new),
    }
}

fn local_mark(owner: &str, label: &str) -> Command {
    Command::SetMark {
        id: mark_id(label),
        owner: id(owner),
        label: label.into(),
        boundary: BoundaryAnchor {
            coordinate: Anchor::Local {
                node: id(owner),
                position: ExactRatio::ONE,
            },
            bias: InsertionBias::Right,
        },
        loss_policy: AnchorLossPolicy::KeepUnresolved,
    }
}

#[test]
fn plays_become_independent_children_with_marks_captions_and_overrides() {
    let original = repeated(3, 2);
    let second = play(&original, "repeat", 1);
    let third = play(&original, "repeat", 2);
    let document = edit(&original, "r4", local_mark("h1", "shared"));
    let document = edit(
        &document,
        "r5",
        Command::SetMark {
            id: mark_id("third-h2"),
            owner: id("h2"),
            label: "third".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: InstancePath {
                        node: id("h2"),
                        repeats: vec![RepeatInstance {
                            node: id("repeat"),
                            iteration: third.clone(),
                        }],
                    },
                    position: ExactRatio::integer(2),
                },
                bias: InsertionBias::Left,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    );
    let document = edit(
        &document,
        "r6",
        Command::SetCaptions {
            node: id("h1"),
            captions: vec![caption("always", None), caption("from two", Some(2))],
        },
    );
    // The second play already owns an override before explode.
    let document = edit(
        &document,
        "r7",
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("h2"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: second.clone(),
                }],
            },
            edit: OccurrenceEdit::Rename {
                label: "Second only".into(),
            },
            identities: OccurrenceIdentities {
                nodes: (0..3).map(|n| id(&format!("override-{n}"))).collect(),
                marks: vec![mark_id("override-mark")],
            },
        },
    );
    let override_root = document.overrides()[&id("repeat")]
        .get(&second)
        .unwrap()
        .clone();
    let needs = document.explode_requirements(&id("repeat")).unwrap();
    // One copied play (group, h1, h2), two gaps, one owned mark copy.
    assert_eq!((needs.nodes, needs.marks), (5, 1));
    let exploded = explode(&document, "repeat", "r8");
    let order = children(&exploded, "repeat");
    assert_eq!(order.len(), 5);
    assert_eq!(order[0], id("group"));
    assert_eq!(order[2], override_root);
    assert!(exploded.overrides().is_empty() && exploded.gap_overrides().is_empty());
    for gap in [&order[1], &order[3]] {
        assert!(matches!(
            &exploded.nodes()[gap].kind,
            NodeKind::Hold { recipe } if recipe.duration == frames(2)
        ));
    }
    let copy = &order[4];
    assert!(copy.as_str().starts_with("r8-n"));
    assert_eq!(
        exploded.nodes()[&override_root].label,
        document.nodes()[&override_root].label
    );
    // Captions counted plays of the exploded Repeat: play one drops the
    // delayed caption, later plays always show it.
    let captions = |node: &NodeId| {
        let h1 = exploded.children(node).next().unwrap();
        exploded.nodes()[h1]
            .captions
            .iter()
            .map(|caption| (caption.text.clone(), caption.reveal))
            .collect::<Vec<_>>()
    };
    assert_eq!(captions(&order[0]), [("always".to_string(), None)]);
    for node in [&order[2], copy] {
        assert_eq!(
            captions(node),
            [("always".to_string(), None), ("from two".to_string(), None)]
        );
    }
    // The occurrence mark follows its concrete third play, now unscoped.
    let third_mark = &exploded.marks()[&mark_id("third-h2")];
    let Anchor::Occurrence { instance, .. } = &third_mark.boundary.coordinate else {
        panic!("occurrence mark")
    };
    assert!(instance.repeats.is_empty());
    assert_eq!(exploded.children(copy).nth(1), Some(&instance.node));
    assert_eq!(third_mark.state, MarkState::Bound);
    // The shared Local mark stays with the first play; copies have fresh marks.
    assert_eq!(exploded.marks()[&mark_id("shared")].owner, id("h1"));
    assert!(exploded.marks().contains_key(&mark_id("r8-m0")));
    assert!(exploded.marks().contains_key(&mark_id("override-mark")));
    // Exact undo, then redo through the stored patch.
    let transaction = apply(
        &document,
        &request(&document, "r8", explode_command(&document, "repeat", "r8")),
    )
    .unwrap();
    assert_eq!(transaction.description, "Explode repeat");
    assert_eq!(transaction.duration_delta, 0);
}

#[test]
fn all_overridden_plays_drop_the_unused_definition_and_dormant_gap() {
    let original = repeated(2, 1);
    let mut document = original.clone();
    for index in 0..2 {
        let iteration = play(&original, "repeat", index);
        document = edit(
            &document,
            &format!("o{index}"),
            Command::EditOccurrence {
                instance: InstancePath {
                    node: id("h1"),
                    repeats: vec![RepeatInstance {
                        node: id("repeat"),
                        iteration: iteration.clone(),
                    }],
                },
                edit: OccurrenceEdit::Rename {
                    label: format!("play {index}"),
                },
                identities: OccurrenceIdentities {
                    nodes: (0..3).map(|n| id(&format!("o{index}-{n}"))).collect(),
                    marks: vec![],
                },
            },
        );
    }
    // A final play's explicit gap branch renders nothing and is retired.
    let last = play(&original, "repeat", 1);
    document = edit(
        &document,
        "dormant",
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: last,
            subtree: Subtree {
                root: id("dormant"),
                nodes: BTreeMap::from([(id("dormant"), BeatNode::hold("Dormant", silent(4)))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let exploded = explode(&document, "repeat", "x");
    assert!(!exploded.nodes().contains_key(&id("group")));
    assert!(!exploded.nodes().contains_key(&id("h1")));
    assert!(!exploded.nodes().contains_key(&id("dormant")));
    assert_eq!(children(&exploded, "repeat").len(), 3);
}

#[test]
fn escalation_becomes_explicit_per_play_gain_and_scale() {
    let document = repeated(3, 1);
    let escalation = RepeatEscalation {
        gain_step: GainDb::new(2000).unwrap(),
        zoom: Some(ZoomStep {
            step: quantize_zoom_step(ExactRatio::new(1, 10).unwrap()).unwrap(),
            progression: ZoomProgression::Add,
        }),
    };
    let document = edit(
        &document,
        "esc",
        Command::SetRepeatEscalation {
            node: id("repeat"),
            escalation: Some(escalation),
        },
    );
    let exploded = explode(&document, "repeat", "x");
    let order = children(&exploded, "repeat");
    // Play one is unchanged; later plays are groups of [play, following gap].
    assert_eq!(order.len(), 4);
    assert_eq!(order[0], id("group"));
    for (index, group) in order[2..].iter().enumerate() {
        let play = u32::try_from(index + 1).unwrap();
        let node = &exploded.nodes()[group];
        assert_eq!(
            node.framing,
            Some(Framing {
                clock: FramingClock::OwnerOutput,
                value: FramingValue::Static {
                    pose: escalation.pose(play).unwrap().unwrap()
                },
            })
        );
        assert_eq!(
            node.audio_treatments,
            AudioTreatments::from_clip_gain(
                ClipGain::new(
                    GainDb::new(2000 * i32::try_from(play).unwrap()).unwrap(),
                    false,
                    vec![],
                    vec![]
                )
                .unwrap()
            )
        );
        assert_eq!(
            exploded.children(group).count(),
            if index == 0 { 2 } else { 1 }
        );
    }
    let NodeKind::Sequence { .. } = &exploded.nodes()[&id("repeat")].kind else {
        panic!()
    };
}

#[test]
fn explode_refuses_without_changing_anything() {
    let document = repeated(3, 2);
    let error = refused(
        &document,
        Command::Explode {
            node: id("group"),
            identities: OccurrenceIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new("refused").unwrap(),
                ordinal: 0,
            },
        },
    );
    assert_eq!(error.code, EditErrorCode::WrongNodeKind);
    let needs = document.explode_requirements(&id("repeat")).unwrap();
    let short = Command::Explode {
        node: id("repeat"),
        identities: OccurrenceIdentities {
            nodes: (1..needs.nodes).map(|n| id(&format!("s{n}"))).collect(),
            marks: vec![],
        },
        timing: AudioTimingId {
            allocation: RevisionId::new("refused").unwrap(),
            ordinal: 0,
        },
    };
    assert!(
        refused(&document, short)
            .message
            .contains("more node identities")
    );
    let reused = Command::Explode {
        node: id("repeat"),
        identities: OccurrenceIdentities {
            nodes: vec![id("h1")],
            marks: vec![],
        },
        timing: AudioTimingId {
            allocation: RevisionId::new("refused").unwrap(),
            ordinal: 0,
        },
    };
    assert_eq!(
        refused(&document, reused).code,
        EditErrorCode::IdentityConflict
    );
}

/// A RepeatSelection gives later plays a retained definition clock. Every
/// exploded owner must resolve exactly the clock its old play used.
#[test]
fn retained_clocks_resolve_identically_for_every_exploded_play() {
    let document = edit(
        &base(FrameRate::new(30_000, 1001).unwrap()),
        "r1",
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: holds(&[("h1", 2), ("h2", 3)]),
        },
    );
    let selection = SliceCaptureSelection::Child { node: id("h2") };
    let plan = document
        .repeat_selection(&id("group"), &selection, 3)
        .unwrap();
    let document = edit(
        &document,
        "wrap",
        Command::RepeatSelection {
            parent: id("group"),
            selection,
            plays: 3,
            identities: RepeatSelectionIdentities {
                repeat: id("repeat"),
                group: plan.needs_group.then(|| id("body")),
                split: SplitIdentities { nodes: vec![] },
            },
            timing: AudioTimingId {
                allocation: RevisionId::new("wrap").unwrap(),
                ordinal: 0,
            },
        },
    );
    let child = id("h2");
    assert!(document.audio_bindings().bindings().contains_key(&child));
    let plays: Vec<_> = (0..3).map(|n| play(&document, "repeat", n)).collect();
    let exploded = explode(&document, "repeat", "x");
    let order = children(&exploded, "repeat");
    assert_eq!(order.len(), 3);
    for (iteration, owner) in plays.iter().zip(&order) {
        let old = document
            .audio_bindings()
            .resolve(
                &child,
                &InstancePath {
                    node: child.clone(),
                    repeats: vec![RepeatInstance {
                        node: id("repeat"),
                        iteration: iteration.clone(),
                    }],
                },
                MAX_AUDIO_BINDING_ENTRIES,
            )
            .unwrap();
        let new = exploded
            .audio_bindings()
            .resolve(
                owner,
                &InstancePath {
                    node: owner.clone(),
                    repeats: vec![],
                },
                MAX_AUDIO_BINDING_ENTRIES,
            )
            .unwrap();
        let comparable = |binding: &ResolvedAudioBinding| {
            let lattice = &binding.lattice;
            (
                lattice.clock.clone(),
                lattice.grid_rule,
                lattice.grid_origin,
                lattice.frames_per_sample,
                lattice.origin,
                lattice.frames_per_local_frame,
                lattice.local_duration,
                lattice.local_support.clone(),
                lattice.instance.clone(),
                lattice.gap_after.clone(),
                lattice.gap_birth,
                binding.resume.clone(),
            )
        };
        assert_eq!(comparable(&old), comparable(&new), "play {iteration:?}");
        // No exploded binding names a vanished live Repeat.
        let binding = &exploded.audio_bindings().bindings()[owner];
        assert!(binding.lattice.births.is_empty());
        assert!(
            binding
                .lattice
                .arguments
                .iter()
                .all(|argument| matches!(argument.value, AudioRepeatValue::Captured { .. }))
        );
    }
}

#[test]
fn duplicate_copies_children_spans_and_ranges_after_themselves() {
    let document = edit(
        &base(FrameRate::new(30_000, 1001).unwrap()),
        "r1",
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: holds(&[("a", 2), ("b", 3), ("c", 4)]),
        },
    );
    let document = edit(&document, "r2", local_mark("b", "inside"));
    let duplicate = |document: &ProjectDocument, name: &str, selection: SliceCaptureSelection| {
        let timing = AudioTimingId {
            allocation: RevisionId::new(name).unwrap(),
            ordinal: 0,
        };
        let needs = document
            .duplicate_requirements(&id("group"), &selection, &timing)
            .unwrap();
        edit(
            document,
            name,
            Command::Duplicate {
                parent: id("group"),
                selection,
                identities: SlicePasteIdentities {
                    authored: OccurrenceIdentities {
                        nodes: (0..needs.slice.nodes)
                            .map(|n| id(&format!("{name}-n{n}")))
                            .collect(),
                        marks: (0..needs.slice.marks)
                            .map(|n| mark_id(&format!("{name}-m{n}")))
                            .collect(),
                    },
                    aliases: (0..needs.slice.aliases)
                        .map(|n| id(&format!("{name}-a{n}")))
                        .collect(),
                },
                split_identities: SplitIdentities {
                    nodes: (0..needs.split_nodes)
                        .map(|n| id(&format!("{name}-s{n}")))
                        .collect(),
                },
                timing,
            },
        )
    };
    let child = duplicate(
        &document,
        "d1",
        SliceCaptureSelection::Child { node: id("b") },
    );
    assert_eq!(child.duration().unwrap().frames(), 12);
    let order = children(&child, "group");
    assert_eq!(&order[..2], [id("a"), id("b")]);
    assert_eq!(order[3], id("c"));
    // The copy is independent: its owned mark has a fresh identity.
    assert_eq!(child.marks().len(), 2);
    assert!(child.marks().contains_key(&mark_id("d1-m0")));
    let span = duplicate(
        &document,
        "d2",
        SliceCaptureSelection::Children {
            first: id("a"),
            last: id("b"),
        },
    );
    assert_eq!(span.duration().unwrap().frames(), 14);
    assert_eq!(children(&span, "group")[3], id("c"));
    // A range ending inside c splits it and inserts the copy at that boundary.
    let range = duplicate(
        &document,
        "d3",
        SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(7)).unwrap(),
        },
    );
    assert_eq!(range.duration().unwrap().frames(), 15);
    // A range ending at a seam needs no Split.
    let seam = duplicate(
        &document,
        "d4",
        SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(5)).unwrap(),
        },
    );
    assert_eq!(seam.duration().unwrap().frames(), 13);
    assert_eq!(children(&seam, "group").last(), Some(&id("c")));
    // The scratch capture clock never persists.
    for document in [&child, &span, &range, &seam] {
        assert!(
            document
                .audio_bindings()
                .timings()
                .keys()
                .all(|timing| timing.ordinal != u32::MAX)
        );
    }
}

#[test]
fn exploded_copies_can_then_be_duplicated_and_edited_independently() {
    let document = repeated(2, 0);
    let exploded = explode(&document, "repeat", "x");
    let order = children(&exploded, "repeat");
    let renamed = edit(
        &exploded,
        "rename",
        Command::Rename {
            node: exploded.children(&order[1]).next().unwrap().clone(),
            label: "Only the second".into(),
        },
    );
    assert_eq!(renamed.nodes()[&id("h1")].label, "h1");
}
