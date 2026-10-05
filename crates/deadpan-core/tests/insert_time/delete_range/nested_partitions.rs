use super::*;

fn partition(label: &str, child: &str, start: i64, end: i64) -> BeatNode {
    BeatNode {
        label: label.into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(end - start),
            mapping: range(start, end),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Partition,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn nested(depth: usize, leaf: BeatNode) -> ProjectDocument {
    let original = tree(&["leaf", "tail"], vec![("leaf", leaf), ("tail", source(2))]);
    let mut wire = json!(original);
    let mut child = "leaf".to_owned();
    let mut length = 12;
    for index in 1..=depth {
        let name = format!("window-{index}");
        wire["nodes"][&name] = json!(partition(&name, &child, 1, length - 1));
        child = name;
        length -= 2;
    }
    wire["nodes"]["root"]["kind"]["children"][0] = json!(child);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn exact_deletion(before: &ProjectDocument, start: i64, end: i64) -> CommandRequest {
    let required = before
        .range_deletion(&id("root"), range(start, end))
        .unwrap()
        .required_ids;
    let mut command = deletion(before, "root", start, end, "deleted");
    let Command::DeleteRange { identities, .. } = &mut command.command else {
        unreachable!()
    };
    *identities = pool("deleted", required);
    command
}

fn physical_window(document: &ProjectDocument, root: &NodeId) -> (NodeId, i64) {
    let mut current = root;
    let mut start = 0;
    while let NodeKind::Retime {
        child,
        mapping,
        purpose: RetimePurpose::Partition,
        ..
    } = &document.nodes()[current].kind
    {
        start += mapping.start().0;
        current = child;
    }
    (current.clone(), start)
}

fn assert_context_copies(before: &BeatNode, after: &ProjectDocument, count: usize) {
    let copies: Vec<_> = after
        .nodes()
        .values()
        .filter(|node| {
            node.label == before.label
                && node.framing == before.framing
                && node.audio_treatments == before.audio_treatments
        })
        .collect();
    assert_eq!(copies.len(), count, "{}", before.label);
    for copy in copies {
        let mut expected = before.clone();
        let mut actual = (*copy).clone();
        if let NodeKind::Retime { child, .. } = &mut expected.kind {
            *child = id("renamed-child");
        }
        if let NodeKind::Retime { child, .. } = &mut actual.kind {
            *child = id("renamed-child");
        }
        assert_eq!(actual, expected);
    }
}

#[test]
fn two_and_three_windows_split_one_child_with_exact_pools_and_full_context() {
    // Asserts the authored reference representation (every reanchor step
    // and complete timing tables). tests/timing_representation.rs proves the
    // compact storage resolves and renders identically.
    deadpan_core::with_reference_timing_representation(|| {
        for depth in [2, 3] {
            let mut leaf = source(12);
            leaf.label = "Retained physical recipe".into();
            let before = nested(depth, leaf);
            let query = before.range_deletion(&id("root"), range(2, 4)).unwrap();
            assert_eq!((query.start_index, query.end_index), (0, 1));
            assert_eq!(query.required_ids, 2 * (depth + 1));
            // Establish that this is exactly an already-copyable endpoint family.
            CapturedEditSlice::capture(
                &before,
                &id("root"),
                range(2, 4),
                AudioTimingId {
                    allocation: revision("copy"),
                    ordinal: 0,
                },
            )
            .unwrap();
            let after = edit(&before, exact_deletion(&before, 2, 4));
            let selected = children(&after);
            assert_eq!(selected.len(), 3);
            let physical_start = i64::try_from(depth).unwrap();
            for (node, expected_start, expected_length, reanchors) in [
                (&selected[0], physical_start, 2, 0),
                (&selected[1], physical_start + 4, 8 - 2 * physical_start, 1),
            ] {
                let (physical, start) = physical_window(&after, node);
                assert_eq!(start, expected_start);
                assert_eq!(
                    after.node_duration(node).unwrap(),
                    duration(expected_length)
                );
                assert_eq!(after.nodes()[&physical], before.nodes()[&id("leaf")]);
                let binding = &after.audio_bindings().bindings()[&physical];
                assert_eq!(binding.lattice.reference.physical, id("leaf"));
                assert_eq!(binding.lattice.reference.timing.ordinal, 0);
                assert_eq!(binding.reanchors.len(), reanchors);
            }
            for index in 1..depth {
                assert_context_copies(&before.nodes()[&id(&format!("window-{index}"))], &after, 2);
            }
            assert_eq!(after.nodes()[&id("tail")], before.nodes()[&id("tail")]);
            assert_eq!(
                after.audio_bindings().bindings()[&id("tail")]
                    .reanchors
                    .len(),
                1
            );
            assert!(
                after
                    .audio_bindings()
                    .timings()
                    .keys()
                    .all(|clock| clock.ordinal <= 1)
            );
        }
    })
}

#[test]
fn different_nested_endpoints_keep_their_independent_contexts_and_outer_suffix() {
    let before = tree(
        &["left", "middle", "right", "tail"],
        vec![
            ("left", partition("Left window", "left-inner", 1, 9)),
            ("left-inner", partition("Left context", "left-leaf", 1, 11)),
            ("left-leaf", source(12)),
            ("middle", BeatNode::hold("Removed middle", recipe(2))),
            ("right", partition("Right window", "right-middle", 1, 7)),
            (
                "right-middle",
                partition("Right context", "right-inner", 1, 9),
            ),
            ("right-inner", partition("Right inner", "right-leaf", 1, 11)),
            ("right-leaf", BeatNode::hold("Held context", recipe(12))),
            ("tail", source(2)),
        ],
    );
    let query = before.range_deletion(&id("root"), range(2, 13)).unwrap();
    assert_eq!(
        (query.start_index, query.end_index, query.required_ids),
        (0, 3, 7)
    );
    let after = edit(&before, exact_deletion(&before, 2, 13));
    assert_eq!(after.duration().unwrap(), duration(7));
    assert_eq!(children(&after).len(), 3);
    assert!(!after.nodes().contains_key(&id("middle")));
    for (slot, expected_start, expected_length, recipe_name) in
        [(0, 2, 2, "left-leaf"), (1, 6, 3, "right-leaf")]
    {
        let child = &children(&after)[slot];
        let (physical, start) = physical_window(&after, child);
        assert_eq!(start, expected_start);
        assert_eq!(
            after.node_duration(child).unwrap(),
            duration(expected_length)
        );
        assert_eq!(after.nodes()[&physical], before.nodes()[&id(recipe_name)]);
        assert_eq!(
            after.audio_bindings().bindings()[&physical].reanchors.len(),
            slot
        );
    }
    assert_context_copies(&before.nodes()[&id("right-middle")], &after, 1);
    assert_context_copies(&before.nodes()[&id("right-inner")], &after, 1);
}

#[test]
fn nested_hold_windows_keep_framing_gain_and_owned_recipe_clocks() {
    let mut hold = recipe(12);
    hold.video = HoldVideo::Freeze {
        asset: AssetId::new("media").unwrap(),
        timestamp: source_span().start(),
    };
    let original = nested(3, BeatNode::hold("Complete Freeze", hold));
    let mut wire = json!(original);
    let framing = Framing::creep(
        FramingPose::identity(),
        FramingPose::new(ExactRatio::ZERO, ExactRatio::ZERO, ExactRatio::integer(2)).unwrap(),
        FramingCurve::Linear,
    )
    .unwrap();
    wire["nodes"]["window-3"]["framing"] = json!(framing);
    wire["nodes"]["window-2"]["audio_treatments"] =
        json!(AudioTreatments::from_clip_gain(ClipGain::default()));
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    // A treated outer window must itself remain a complete effect owner.
    assert_eq!(
        before
            .range_deletion(&id("root"), range(2, 4))
            .unwrap()
            .required_ids,
        11
    );
    let after = edit(&before, exact_deletion(&before, 2, 4));
    for name in ["window-3", "window-2", "window-1", "leaf"] {
        assert_context_copies(&before.nodes()[&id(name)], &after, 2);
    }
    for node in &children(&after)[..2] {
        let (physical, _) = physical_window(&after, node);
        assert_eq!(
            after.audio_bindings().bindings()[&physical]
                .lattice
                .reference
                .physical,
            id("leaf")
        );
    }
}

#[test]
fn nested_deletion_identity_bounds_and_timing_rejections_are_atomic() {
    let before = nested(3, source(12));
    let original = before.to_json().unwrap();
    for (parent, start, end) in [
        ("root", 2, 2),
        ("root", -1, 4),
        ("root", 2, 9),
        ("window-2", 2, 4),
    ] {
        assert!(apply(&before, &deletion(&before, parent, start, end, "bad")).is_err());
    }
    for nodes in [
        pool("deleted", 7).nodes,
        vec![id("duplicate"); 8],
        vec![id("leaf"); 8],
    ] {
        let mut request = exact_deletion(&before, 2, 4);
        let Command::DeleteRange { identities, .. } = &mut request.command else {
            unreachable!()
        };
        identities.nodes = nodes;
        assert!(apply(&before, &request).is_err());
    }
    let mut exhausted = exact_deletion(&before, 2, 4);
    let Command::DeleteRange { timing, .. } = &mut exhausted.command else {
        unreachable!()
    };
    timing.ordinal = u32::MAX;
    assert_eq!(
        apply(&before, &exhausted).unwrap_err().code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(before.to_json().unwrap(), original);
}

#[test]
fn recursive_deletion_does_not_broaden_source_replacement_or_pause_admission() {
    let before = nested(3, source(12));
    assert!(before.range_deletion(&id("root"), range(2, 4)).is_ok());
    assert!(before.source_replacement(&id("root"), range(2, 4)).is_err());
    assert!(
        before
            .source_splice_interior(&id("root"), &id("window-3"), duration(2))
            .is_err()
    );
    assert!(apply(&before, &insertion(&before, "pause", 2, 1)).is_err());
    // Standalone Split retains its existing transparent context behavior.
    let split = request(
        &before,
        "split",
        Command::Split {
            node: id("window-3"),
            at: duration(2),
            identities: pool("split", 4),
        },
    );
    let after = edit(&before, split);
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(children(&after).len(), 3);
}

#[test]
fn nested_windows_do_not_admit_general_retime_or_repeat_interiors() {
    for kind in [
        NodeKind::Repeat {
            child: id("provider"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
            escalation: None,
        },
        NodeKind::Retime {
            child: id("provider"),
            duration: duration(12),
            mapping: range(0, 6),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
    ] {
        let mut leaf = source(12);
        leaf.kind = kind;
        let mut wire = json!(nested(3, source(12)));
        wire["nodes"]["leaf"] = json!(leaf);
        wire["nodes"]["provider"] = json!(source(6));
        let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
        assert!(before.range_deletion(&id("root"), range(2, 4)).is_err());
        assert!(apply(&before, &deletion(&before, "root", 2, 4, "bad")).is_err());
        let after = edit(&before, exact_deletion(&before, 0, 6));
        assert_eq!(children(&after), [id("tail")]);
    }
}

#[test]
fn nested_cut_preserves_root_mark_bias_pins_and_unresolved_intent() {
    let mut wire = json!(nested(3, source(12)));
    let mark = |position, bias| Mark {
        owner: id("root"),
        label: "Boundary".into(),
        boundary: BoundaryAnchor {
            coordinate: Anchor::Local {
                node: id("root"),
                position: ExactRatio::integer(position),
            },
            bias,
        },
        loss_policy: AnchorLossPolicy::KeepUnresolved,
        state: MarkState::Bound,
        fragments: vec![],
    };
    let mut pinned = mark(5, InsertionBias::Right);
    pinned.boundary.coordinate = Anchor::Sequence {
        frame: ProjectFrame(5),
    };
    let mut dormant = mark(3, InsertionBias::Right);
    dormant.state = MarkState::Unresolved {
        reason: MarkLossReason::HostMissing,
    };
    wire["marks"] = json!(BTreeMap::from([
        ("left", mark(2, InsertionBias::Left)),
        ("right", mark(4, InsertionBias::Right)),
        ("lost", mark(3, InsertionBias::Right)),
        ("suffix", mark(5, InsertionBias::Right)),
        ("pin", pinned),
        ("dormant", dormant),
    ]));
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, exact_deletion(&before, 2, 4));
    for (name, position) in [("left", 2), ("right", 2), ("suffix", 3)] {
        let mark = &after.marks()[&MarkId::new(name).unwrap()];
        assert_eq!(mark.state, MarkState::Bound);
        assert_eq!(
            mark.boundary.coordinate,
            Anchor::Local {
                node: id("root"),
                position: ExactRatio::integer(position)
            }
        );
    }
    assert!(matches!(
        after.marks()[&MarkId::new("lost").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    for name in ["pin", "dormant"] {
        let key = MarkId::new(name).unwrap();
        assert_eq!(after.marks()[&key], before.marks()[&key]);
    }
}

#[test]
fn nested_generated_provider_still_requires_a_whole_owned_cut() {
    let original = nested(3, BeatNode::hold("Generated", recipe(12)));
    let rate = original.presentation_basis().frame_rate;
    let object = |digit: char| {
        GeneratedObjectRef::new(
            GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
            100,
        )
        .unwrap()
    };
    let sampled = object('b');
    let native = object('c');
    let artifact = GeneratedArtifact {
        sampled_asset: AssetId::new("sampled").unwrap(),
        sampled_object: sampled.clone(),
        native_asset: AssetId::new("native").unwrap(),
        native_object: native.clone(),
        provenance: object('d'),
        sampling: BridgeSamplingMap::new(
            rate,
            rate,
            duration(12),
            duration(12),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap(),
        content_aspect: None,
    };
    let mut wire = json!(original);
    for (name, object) in [("sampled", sampled), ("native", native)] {
        wire["assets"][name] = json!(AssetRecord {
            label: name.into(),
            content_hash: object.content().to_string(),
            video: Some(source_span()),
            audio: None,
            still_image: false,
            frame_count: Some(duration(12)),
            source_qualification: None,
        });
    }
    wire["nodes"]["leaf"]["kind"]["recipe"]["video"] = json!(HoldVideo::Generated {
        accepted: Box::new(AcceptedGeneration {
            artifact,
            fallback: HoldFallback::Background
        }),
    });
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(before.range_deletion(&id("root"), range(2, 4)).is_err());
    assert!(apply(&before, &deletion(&before, "root", 2, 4, "bad")).is_err());
    let after = edit(&before, exact_deletion(&before, 0, 6));
    assert_eq!(children(&after), [id("tail")]);
}

#[test]
fn nested_endpoint_cut_transforms_the_independent_root_bus_once() {
    let original = nested(3, source(12));
    let natural_frames =
        SourceAudioMapping::natural_rate(source_span(), original.presentation_basis().frame_rate)
            .unwrap()
            .duration_frames(original.duration().unwrap())
            .unwrap();
    let mut wire = json!(original);
    wire["assets"]["media"]["audio"] = json!(source_span());
    wire["assets"]["media"]["source_qualification"] = json!("b".repeat(64));
    let sound = SoundId::new("independent").unwrap();
    wire["sounds"] = json!(BTreeMap::from([(
        sound.clone(),
        SoundEvent {
            owner: id("root"),
            label: "Independent root bus".into(),
            source: SourceAudio {
                asset: AssetId::new("media").unwrap(),
                span: source_span()
            },
            mapping: SourceAudioMapping::SelectedPlacement {
                start: ExactRatio::ZERO,
                frames: natural_frames,
                selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(8)).unwrap(),
            },
            offset: AudioSample(0),
            gain_millidecibels: 0,
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        }
    )]));
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, exact_deletion(&before, 2, 4));
    assert_eq!(after.sounds(), before.sounds());
    let route = &after.sound_routes()[&sound];
    assert_eq!(route.recipe_extent, duration(8));
    assert_eq!(route.edits.len(), 1);
    assert_eq!(
        route.edits[0].operation,
        RootSoundOperation::Delete { range: range(2, 4) }
    );
    assert_eq!(route.edits[0].grid.frame_origin, ExactRatio::ZERO);
    assert_eq!(
        route.edits[0].grid.frame_rate,
        before.presentation_basis().frame_rate
    );
}
