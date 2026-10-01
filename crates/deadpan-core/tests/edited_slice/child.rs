use super::*;

fn child(document: &ProjectDocument, parent: &str, node: &str) -> CapturedEditSlice {
    CapturedEditSlice::capture_selection(
        document,
        &id(parent),
        &SliceCaptureSelection::Child { node: id(node) },
        timing("child-capture"),
    )
    .unwrap()
}

fn children<'a>(document: &'a ProjectDocument, parent: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(parent)].kind else {
        panic!("Sequence expected")
    };
    children
}

fn empty_tree() -> ProjectDocument {
    tree(
        &["a", "b", "group"],
        vec![
            ("a", BeatNode::sequence("Same empty label", Vec::new())),
            ("b", BeatNode::sequence("Same empty label", Vec::new())),
            (
                "group",
                BeatNode::sequence("Nested empties", vec![id("inner")]),
            ),
            ("inner", BeatNode::sequence("Inner", Vec::new())),
        ],
    )
}

#[test]
fn exact_child_distinguishes_same_time_siblings_and_copies_empty_structure_again() {
    let before = empty_tree();
    let a = child(&before, "root", "a");
    let b = child(&before, "root", "b");
    assert_ne!(a, b);
    assert_eq!(a.range(), range(0, 0));
    assert_eq!(a.duration(), FrameDuration::ZERO);
    assert_eq!(
        a.selection(),
        &SliceCaptureSelection::Child { node: id("a") }
    );
    assert_eq!(a.identity_requirements().unwrap().nodes, 2);
    assert_eq!(a.identity_requirements().unwrap().timings, 0);
    assert_eq!(a.identity_requirements().unwrap().aliases, 0);
    assert_eq!(
        CapturedEditSlice::from_json(&a.to_json().unwrap()).unwrap(),
        a
    );
    a.validate_capture(&before).unwrap();

    let group = child(&before, "root", "group");
    assert_eq!(group.identity_requirements().unwrap().nodes, 3);
    let once = edit(&before, &paste(&before, &group, "once", 1));
    assert_eq!(
        children(&once, "root"),
        &[id("a"), id("once-node-0"), id("b"), id("group")]
    );
    assert_eq!(once.duration().unwrap(), FrameDuration::ZERO);
    let recopy = child(&once, "root", "once-node-0");
    assert_eq!(recopy.identity_requirements().unwrap().nodes, 4);
    let twice = edit(&once, &paste(&once, &recopy, "twice", 4));
    assert_eq!(children(&twice, "root").last(), Some(&id("twice-node-0")));
    assert_eq!(twice.duration().unwrap(), FrameDuration::ZERO);
    assert!(twice.audio_bindings().is_empty());
    assert_eq!(before, empty_tree());
}

#[test]
fn zero_seam_uses_exact_slots_and_max_timing_ordinal_without_any_clock() {
    let before = empty_tree();
    let slice = child(&before, "root", "a");
    for slot in 0..=3 {
        let name = format!("slot-{slot}");
        let mut request = paste(&before, &slice, &name, slot);
        let Command::SpliceSlice { timing, .. } = &mut request.command else {
            unreachable!()
        };
        timing.ordinal = u32::MAX;
        let wire = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<CommandRequest>(&wire).unwrap(),
            request
        );
        let tx = apply(&before, &request).unwrap();
        assert_eq!(tx.duration_delta, 0);
        assert!(!tx.forward.nodes.is_empty());
        assert!(tx.forward.audio_bindings.is_none());
        let after = edit(&before, &request);
        let mut expected = children(&before, "root").to_vec();
        expected.insert(slot, id(&format!("{name}-node-0")));
        assert_eq!(children(&after, "root"), expected);
        assert_eq!(after.audio_bindings(), before.audio_bindings());
    }
    let mut wrong_allocation = paste(&before, &slice, "wrong", 0);
    let Command::SpliceSlice { timing, .. } = &mut wrong_allocation.command else {
        unreachable!()
    };
    timing.allocation = revision("not-new-revision");
    assert_eq!(
        apply(&before, &wrong_allocation).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
}

#[test]
fn whole_child_retains_positive_composites_and_excludes_adjacent_empty_siblings() {
    let repeated = BeatNode {
        label: "Repeated owner".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("unit"),
            iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
            gap: None,
        },
    };
    let retimed = BeatNode {
        label: "Preserve owner".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("slow-unit"),
            duration: duration(8),
            mapping: range(0, 4),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
    };
    let before = tree(
        &["left", "group", "right"],
        vec![
            ("left", BeatNode::sequence("Left", Vec::new())),
            (
                "group",
                BeatNode::sequence("Owned group", vec![id("repeat"), id("retime")]),
            ),
            ("right", BeatNode::sequence("Right", Vec::new())),
            ("repeat", repeated),
            ("unit", hold(2)),
            ("retime", retimed),
            ("slow-unit", hold(4)),
        ],
    );
    let exact = child(&before, "root", "group");
    assert_eq!(exact.duration(), duration(14));
    assert_eq!(exact.identity_requirements().unwrap().nodes, 6);
    let mut exact_wire = serde_json::to_value(&exact).unwrap();
    assert_eq!(exact_wire["parts"].as_array().unwrap().len(), 1);
    assert!(exact_wire["nodes"].get("left").is_none());
    assert!(exact_wire["nodes"].get("right").is_none());
    let ranged =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 14), timing("child-capture"))
            .unwrap();
    exact_wire.as_object_mut().unwrap().remove("selection");
    assert_eq!(exact_wire, serde_json::to_value(&ranged).unwrap());
    let empty = tree(&[], Vec::new());
    let pasted = edit(&empty, &paste(&empty, &exact, "positive", 0));
    assert_eq!(pasted.duration().unwrap(), duration(14));
    exact.validate_capture(&before).unwrap();
}

#[test]
fn owned_empty_metadata_and_marks_survive_with_fresh_identities() {
    let mut empty = BeatNode::sequence("Owned empty treatment", Vec::new());
    empty.framing = Some(
        Framing::static_pose(
            FramingPose::new(
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::integer(2),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    empty.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-3000).unwrap(), false, Vec::new(), Vec::new()).unwrap(),
    );
    let mut dormant = mark("empty", 99, InsertionBias::Left);
    dormant.state = MarkState::Unresolved {
        reason: MarkLossReason::OutOfRange,
    };
    let mut pin = mark("empty", 0, InsertionBias::Right);
    pin.boundary.coordinate = Anchor::Sequence {
        frame: ProjectFrame(2),
    };
    let before = marked(
        &tree(
            &["empty", "tail"],
            vec![("empty", empty.clone()), ("tail", hold(4))],
        ),
        vec![
            ("left", mark("empty", 0, InsertionBias::Left)),
            ("right", mark("empty", 0, InsertionBias::Right)),
            ("dormant", dormant),
            ("pin", pin),
        ],
    );
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["audio_lineage"] = json!({"root":{"allocation":"prior","origin":"old-root"}, "empty":{"allocation":"prior","origin":"old-empty"}});
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let slice = child(&before, "root", "empty");
    assert_eq!(slice.identity_requirements().unwrap().marks, 4);
    assert_eq!(slice.identity_requirements().unwrap().aliases, 1);
    let after = edit(&before, &paste(&before, &slice, "owned", 2));
    assert_eq!(after.nodes()[&id("owned-node-1")], empty);
    for (id, mark) in before.marks() {
        assert_eq!(&after.marks()[id], mark);
    }
    for (id, lineage) in before.audio_lineage() {
        assert_eq!(&after.audio_lineage()[id], lineage);
    }
    assert_ne!(
        after.audio_lineage()[&id("owned-node-1")],
        before.audio_lineage()[&id("empty")]
    );
    let copied = copied_marks(&after, "owned");
    assert_eq!(copied.len(), 4);
    assert!(copied.iter().any(|mark| mark.state
        == MarkState::Unresolved {
            reason: MarkLossReason::OutOfRange
        }
        && mark.boundary.coordinate
            == Anchor::Local {
                node: id("empty"),
                position: ExactRatio::integer(99)
            }));
    assert!(copied.iter().any(|mark| mark.boundary.coordinate
        == Anchor::Sequence {
            frame: ProjectFrame(2)
        }));
}

#[test]
fn zero_paste_preserves_existing_bound_clocks_root_bus_routes_allowances_and_marks() {
    let initial = marked(
        &tree(
            &["empty", "lead", "middle", "tail"],
            vec![
                ("empty", BeatNode::sequence("Empty", Vec::new())),
                ("lead", hold(2)),
                ("middle", hold(4)),
                ("tail", hold(4)),
            ],
        ),
        vec![("root-mark", mark("root", 8, InsertionBias::Right))],
    );
    let sounded = super::placement::with_sound(&initial);
    let positive = CapturedEditSlice::capture(
        &sounded,
        &id("root"),
        range(0, 2),
        timing("positive-capture"),
    )
    .unwrap();
    let before = edit(&sounded, &paste(&sounded, &positive, "positive", 2));
    assert!(!before.audio_bindings().is_empty());
    assert!(!before.sound_routes().is_empty());
    assert!(!before.sound_allowances().is_empty());
    let slice = child(&before, "root", "empty");
    assert_eq!(slice.identity_requirements().unwrap().timings, 0);
    for slot in [0, 2, children(&before, "root").len()] {
        let after = edit(
            &before,
            &paste(&before, &slice, &format!("zero-{slot}"), slot),
        );
        assert_eq!(after.audio_bindings(), before.audio_bindings());
        assert_eq!(after.audio_lineage(), before.audio_lineage());
        assert_eq!(after.marks(), before.marks());
        assert_eq!(after.sounds(), before.sounds());
        assert_eq!(after.sound_routes(), before.sound_routes());
        assert_eq!(after.sound_allowances(), before.sound_allowances());
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    }
}

#[test]
fn child_scope_and_historical_payload_are_explicit_and_closed() {
    let before = empty_tree();
    let slice = child(&before, "root", "a");
    for (parent, node) in [
        ("root", "inner"),
        ("a", "b"),
        ("missing", "a"),
        ("root", "root"),
    ] {
        assert!(
            CapturedEditSlice::capture_selection(
                &before,
                &id(parent),
                &SliceCaptureSelection::Child { node: id(node) },
                timing("bad")
            )
            .is_err()
        );
    }
    for selection in [
        json!(null),
        json!({"type":"mystery","node":"a"}),
        json!({"type":"child","node":"a","extra":0}),
        json!({"type":"range","range":range(0,0)}),
    ] {
        let mut wire = serde_json::to_value(&slice).unwrap();
        wire["selection"] = selection;
        assert!(CapturedEditSlice::from_json(&wire.to_string()).is_err());
        let mut request = serde_json::to_value(paste(&before, &slice, "wire", 0)).unwrap();
        request["command"]["slice"] = wire;
        assert!(serde_json::from_value::<CommandRequest>(request).is_err());
    }
    let wire = slice.to_json().unwrap();
    let duplicate = wire.replacen(
        "\"selection\":",
        "\"selection\":{\"type\":\"child\",\"node\":\"a\"},\"selection\":",
        1,
    );
    assert!(CapturedEditSlice::from_json(&duplicate).is_err());
    assert!(serde_json::from_str::<CapturedEditSlice>(&duplicate).is_err());
    let mut forged = serde_json::to_value(&slice).unwrap();
    forged["selection"]["node"] = json!("b");
    assert!(CapturedEditSlice::from_json(&forged.to_string()).is_err());
    let mut forged = serde_json::to_value(&slice).unwrap();
    forged["nodes"]["a"]["label"] = json!("Forged owned metadata");
    let forged = CapturedEditSlice::from_json(&forged.to_string()).unwrap();
    assert_eq!(
        forged.validate_capture(&before).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
    let mut no_selector = serde_json::to_value(&slice).unwrap();
    no_selector.as_object_mut().unwrap().remove("selection");
    assert!(CapturedEditSlice::from_json(&no_selector.to_string()).is_err());
    assert!(
        CapturedEditSlice::capture(&before, &id("root"), range(0, 0), timing("empty-range"))
            .is_err()
    );
}

#[test]
fn old_range_wire_stays_canonical_and_explicit_range_must_match_metadata() {
    let before = tree(&["held"], vec![("held", hold(4))]);
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(1, 3), timing("range")).unwrap();
    assert_eq!(
        slice.selection(),
        &SliceCaptureSelection::Range { range: range(1, 3) }
    );
    let mut wire = serde_json::to_value(&slice).unwrap();
    assert!(wire.get("selection").is_none());
    assert_eq!(
        CapturedEditSlice::from_json(&wire.to_string()).unwrap(),
        slice
    );
    wire["selection"] = json!(SliceCaptureSelection::Range { range: range(1, 3) });
    assert_eq!(
        CapturedEditSlice::from_json(&wire.to_string()).unwrap(),
        slice
    );
    wire["selection"] = json!(SliceCaptureSelection::Range { range: range(0, 2) });
    assert!(CapturedEditSlice::from_json(&wire.to_string()).is_err());
}

#[test]
fn zero_rejects_interior_replacement_move_and_invalid_pools_without_mutation() {
    let before = tree(
        &["empty", "held"],
        vec![
            ("empty", BeatNode::sequence("Empty", Vec::new())),
            ("held", hold(4)),
        ],
    );
    let slice = child(&before, "root", "empty");
    assert!(
        before
            .slice_splice_interior(&id("root"), &id("held"), duration(1), &slice)
            .is_err()
    );
    assert!(
        before
            .slice_replacement(&id("root"), range(1, 2), &slice)
            .is_err()
    );
    assert!(
        before
            .range_move(
                &id("root"),
                range(0, 0),
                &MoveRangeDestination::Seam {
                    parent: id("root"),
                    index: 2
                }
            )
            .is_err()
    );
    let snapshot = before.clone();
    for mode in 0..4 {
        let mut request = paste(&before, &slice, "invalid", 0);
        let Command::SpliceSlice {
            identities, index, ..
        } = &mut request.command
        else {
            unreachable!()
        };
        match mode {
            0 => identities.authored.nodes.clear(),
            1 => identities.authored.nodes[0] = id("held"),
            2 => identities.authored.nodes[1] = identities.authored.nodes[0].clone(),
            _ => *index = 3,
        }
        assert!(apply(&before, &request).is_err());
        assert_eq!(before, snapshot);
    }
    let Command::SpliceSlice {
        identities, timing, ..
    } = paste(&before, &slice, "invalid", 0).command
    else {
        unreachable!()
    };
    for command in [
        Command::SpliceSliceAt {
            parent: id("root"),
            target: id("held"),
            at: duration(1),
            slice: slice.clone(),
            identities: identities.clone(),
            split_identities: SplitIdentities::default(),
            timing: timing.clone(),
        },
        Command::ReplaceSlice {
            parent: id("root"),
            range: range(1, 2),
            slice: slice.clone(),
            identities,
            split_identities: SplitIdentities::default(),
            timing,
        },
    ] {
        assert_eq!(
            apply(&before, &request(&before, "invalid", command))
                .unwrap_err()
                .code,
            EditErrorCode::InvalidCommand
        );
        assert_eq!(before, snapshot);
    }
}

#[test]
fn child_under_repeat_clock_is_rejected_and_extra_wrapper_obeys_depth_limit() {
    let repeat = BeatNode {
        label: "Repeat".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("group"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
        },
    };
    let repeated = tree(
        &["repeat"],
        vec![
            ("repeat", repeat),
            (
                "group",
                BeatNode::sequence("Group", vec![id("empty"), id("held")]),
            ),
            ("empty", BeatNode::sequence("Empty", Vec::new())),
            ("held", hold(1)),
        ],
    );
    assert!(
        CapturedEditSlice::capture_selection(
            &repeated,
            &id("group"),
            &SliceCaptureSelection::Child { node: id("empty") },
            timing("capture")
        )
        .is_err()
    );
    let names: Vec<_> = (1..=MAX_DOCUMENT_DEPTH)
        .map(|n| format!("level-{n}"))
        .collect();
    let nodes = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            (
                name.as_str(),
                BeatNode::sequence(
                    "Empty level",
                    names
                        .get(index + 1)
                        .map(|next| vec![id(next)])
                        .unwrap_or_default(),
                ),
            )
        })
        .collect();
    let deep = tree(&[names[0].as_str()], nodes);
    let slice = child(&deep, "root", &names[0]);
    let destination = tree(&[], Vec::new());
    assert_eq!(
        apply(&destination, &paste(&destination, &slice, "too-deep", 0))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(destination, tree(&[], Vec::new()));
}

#[test]
fn empty_child_retains_dormant_source_mark_asset_without_capturing_audio() {
    let before = empty_tree();
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
    let asset = AssetId::new("mark-media").unwrap();
    let mut dormant = mark("a", 0, InsertionBias::Right);
    dormant.boundary.coordinate = Anchor::Source {
        asset: asset.clone(),
        moment: SourceMoment::AudioSample {
            sample: 12,
            sample_rate: 48_000,
        },
    };
    dormant.state = MarkState::Unresolved {
        reason: MarkLossReason::SourceUnavailable,
    };
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["assets"] = json!({asset.as_str():AssetRecord {label:"Mark media".into(),content_hash:"a".repeat(64),video:None,audio:Some(span),frame_count:None,still_image:false,source_qualification:None}});
    let before = marked(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        vec![("source-intent", dormant.clone())],
    );
    let slice = child(&before, "root", "a");
    let slice_wire = serde_json::to_value(&slice).unwrap();
    assert_eq!(slice_wire["assets"], wire["assets"]);
    assert_eq!(slice.identity_requirements().unwrap().timings, 0);
    let empty = tree(&[], Vec::new());
    let after = edit(&empty, &paste(&empty, &slice, "asset-copy", 0));
    assert_eq!(after.assets()[&asset], before.assets()[&asset]);
    let copied = copied_marks(&after, "asset-copy");
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0].boundary, dormant.boundary);
    assert_eq!(copied[0].state, dormant.state);
    assert!(after.audio_bindings().is_empty());
}
