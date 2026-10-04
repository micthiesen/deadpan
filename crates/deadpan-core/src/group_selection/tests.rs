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
fn child(value: &str) -> SliceCaptureSelection {
    SliceCaptureSelection::Child { node: node(value) }
}
fn selected(start: i64, end: i64) -> SliceCaptureSelection {
    SliceCaptureSelection::Range {
        range: range(start, end),
    }
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
    let mut document = ProjectDocument::new(
        ProjectId::new("group").unwrap(),
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
    document.nodes.insert(
        node("root"),
        BeatNode::sequence("Root", children.iter().map(|name| node(name)).collect()),
    );
    document
        .nodes
        .extend(entries.into_iter().map(|(name, value)| (node(name), value)));
    document.validate().unwrap();
    document
}
fn command(document: &ProjectDocument, parent: &str, selection: SliceCaptureSelection) -> Command {
    let plan = document.group_selection(&node(parent), &selection).unwrap();
    Command::GroupSelection {
        parent: node(parent),
        selection,
        label: "the \"uncomfortable\" 答え".into(),
        identities: GroupSelectionIdentities {
            group: node("grouped"),
            split: SplitIdentities {
                nodes: (0..plan.required_split_ids)
                    .map(|n| node(&format!("split-{n}")))
                    .collect(),
            },
        },
        timing: AudioTimingId {
            allocation: revision("group-edit"),
            ordinal: 0,
        },
    }
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    let new_revision = match &command {
        Command::GroupSelection { timing, .. } => timing.allocation.clone(),
        _ => revision(&format!("{}-next", document.revision_id())),
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
    let wire = serde_json::to_string(&request).unwrap();
    let decoded = serde_json::from_str(&wire).unwrap();
    assert_eq!(request, decoded);
    let tx = crate::apply(document, &decoded).unwrap();
    assert_eq!(tx.duration_delta, 0);
    let after = tx.forward.apply(document).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
fn children<'a>(document: &'a ProjectDocument, parent: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&node(parent)].kind else {
        panic!()
    };
    children
}

#[test]
fn exact_forest_groups_empty_endpoints_including_an_all_empty_span() {
    for length in [0, 3] {
        let before = tree(
            &["outside-left", "left", "body", "right", "outside-right"],
            vec![
                ("outside-left", BeatNode::sequence("Outside", vec![])),
                ("left", BeatNode::sequence("Left", vec![])),
                (
                    "body",
                    if length == 0 {
                        BeatNode::sequence("Body", vec![])
                    } else {
                        hold(length)
                    },
                ),
                ("right", BeatNode::sequence("Right", vec![])),
                ("outside-right", BeatNode::sequence("Outside", vec![])),
            ],
        );
        let selection = SliceCaptureSelection::Children {
            first: node("left"),
            last: node("right"),
        };
        let after = edit(&before, command(&before, "root", selection));
        assert_eq!(
            children(&after, "root"),
            [node("outside-left"), node("grouped"), node("outside-right")]
        );
        assert_eq!(
            children(&after, "grouped"),
            [node("left"), node("body"), node("right")]
        );
        assert_eq!(after.audio_bindings(), before.audio_bindings());
        let ungrouped = edit(
            &after,
            Command::Ungroup {
                node: node("grouped"),
            },
        );
        assert_eq!(ungrouped.nodes(), before.nodes());
    }
}

#[test]
fn exact_child_and_range_include_empty_structure_only_when_selected() {
    let before = tree(
        &["left", "a", "middle", "b", "right"],
        vec![
            ("left", BeatNode::sequence("Left", vec![])),
            ("a", hold(3)),
            ("middle", BeatNode::sequence("Middle", vec![])),
            ("b", hold(4)),
            ("right", BeatNode::sequence("Right", vec![])),
        ],
    );
    let grouped = edit(&before, command(&before, "root", selected(0, 7)));
    assert_eq!(
        children(&grouped, "root"),
        &[node("left"), node("grouped"), node("right")]
    );
    assert_eq!(
        children(&grouped, "grouped"),
        &[node("a"), node("middle"), node("b")]
    );
    assert!(
        grouped.audio_bindings().is_empty(),
        "no endpoint needs a sampling clock"
    );
    let restored = edit(
        &grouped,
        Command::Ungroup {
            node: node("grouped"),
        },
    );
    assert_eq!(restored.nodes(), before.nodes());
    let empty = edit(&before, command(&before, "root", child("middle")));
    assert_eq!(children(&empty, "grouped"), &[node("middle")]);
    assert_eq!(
        empty.nodes()[&node("middle")],
        before.nodes()[&node("middle")]
    );
    assert!(
        before
            .group_selection(&node("root"), &selected(3, 3))
            .is_err()
    );
    let all_empty = tree(
        &["empty"],
        vec![("empty", BeatNode::sequence("Empty", vec![]))],
    );
    let after = edit(&all_empty, command(&all_empty, "root", child("empty")));
    assert_eq!(after.duration().unwrap(), FrameDuration::ZERO);
}

fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 48_000,
            time_base,
        },
    )
    .unwrap()
}
fn source_document() -> ProjectDocument {
    let mut document = tree(
        &["prefix", "processed", "suffix"],
        vec![
            ("prefix", hold(2)),
            ("processed", hold(27)),
            ("suffix", hold(5)),
        ],
    );
    let asset = AssetId::new("original").unwrap();
    document.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(span()),
            audio: Some(span()),
            frame_count: Some(frames(30)),
            still_image: false,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    let mut source = hold(12);
    source.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(12),
            edit_window: None,
            video: SourceVideo::Stream {
                asset: asset.clone(),
                span: span(),
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(SourceAudio {
                asset,
                span: span(),
            }),
            audio_mapping: SourceAudioMapping::natural_rate(
                span(),
                document.presentation_basis().frame_rate,
            )
            .unwrap(),
            link: LinkRelation::Linked,
            audio_offset: AudioSample(17),
        },
    };
    source.label = "Voice".into();
    document.nodes.insert(node("voice"), source);
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: node("voice"),
        iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
        gap: None,
        escalation: None,
    };
    document.nodes.insert(node("repeat"), repeat);
    let processed = document.nodes.get_mut(&node("processed")).unwrap();
    processed.label = "Preserve owner".into();
    processed.kind = NodeKind::Retime {
        child: node("repeat"),
        purpose: RetimePurpose::Edit,
        duration: frames(27),
        mapping: range(0, 36),
        pitch: PitchPolicy::Preserve,
    };
    processed.framing = Some(
        Framing::creep(
            FramingPose::default(),
            FramingPose::new(
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::integer(2),
            )
            .unwrap(),
            FramingCurve::Linear,
        )
        .unwrap(),
    );
    document.validate().unwrap();
    document.audio_bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: revision("retained"),
            ordinal: 0,
        },
    )
    .unwrap();
    document.validate().unwrap();
    document
}
fn locate(document: &ProjectDocument, position: ExactRatio) -> BoundaryLocation {
    AnchorIndex::new(document)
        .unwrap()
        .locate_boundary(
            &BoundaryLocationRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                position,
                bias: InsertionBias::Right,
            },
            BoundaryQueryLimits::default(),
        )
        .unwrap()
}

#[test]
fn partial_composite_group_retains_picture_owner_clocks_and_sample_boundaries() {
    let before = source_document();
    let grouped = edit(&before, command(&before, "root", selected(4, 25)));
    assert_eq!(
        grouped.audio_bindings().timings(),
        before.audio_bindings().timings()
    );
    assert_eq!(
        grouped.audio_bindings().bindings()[&node("suffix")],
        before.audio_bindings().bindings()[&node("suffix")]
    );
    // Every root frame center reaches the same full Source recipe, local point,
    // original Preserve clock and framing owner clock through endpoint copies.
    for frame in 0..before.duration().unwrap().frames() {
        let at = ExactRatio::new(i128::from(frame) * 2 + 1, 2).unwrap();
        let original = locate(&before, at);
        let current = locate(&grouped, at);
        let a = original.scopes.last().unwrap();
        let b = current.scopes.last().unwrap();
        assert_eq!(a.position, b.position);
        assert_eq!(
            before.nodes()[&a.instance.node].kind,
            grouped.nodes()[&b.instance.node].kind
        );
        let framing = |document: &ProjectDocument, location: &BoundaryLocation| {
            location
                .scopes
                .iter()
                .filter_map(|scope| {
                    document.nodes()[&scope.instance.node]
                        .framing
                        .clone()
                        .map(|framing| (framing, scope.position, scope.duration))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(framing(&before, &original), framing(&grouped, &current));
        // Split adds transparent Partition crops around full retained owners.
        // Their output clocks differ; every authored Retime recipe and owner
        // clock must still match in order, without losing or adding an owner.
        let authored_retimes = |document: &ProjectDocument, location: &BoundaryLocation| {
            location
                .scopes
                .iter()
                .filter_map(|scope| match &document.nodes()[&scope.instance.node].kind {
                    NodeKind::Retime {
                        purpose: RetimePurpose::Edit,
                        duration,
                        mapping,
                        pitch,
                        ..
                    } => Some((scope.position, scope.duration, *duration, *mapping, *pitch)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let old_owners = authored_retimes(&before, &original);
        assert_eq!(old_owners.len(), usize::from((2..29).contains(&frame)));
        assert_eq!(old_owners, authored_retimes(&grouped, &current));
        let old_binding = before
            .audio_bindings()
            .resolve(&a.instance.node, &a.instance, 10_000)
            .unwrap();
        let new_binding = grouped
            .audio_bindings()
            .resolve(&b.instance.node, &b.instance, 10_000)
            .unwrap();
        assert_eq!(
            old_binding.lattice.sample_boundary(a.position).unwrap(),
            new_binding.lattice.sample_boundary(b.position).unwrap()
        );
        assert_eq!(old_binding.lattice.grid_rule, new_binding.lattice.grid_rule);
        assert_eq!(
            old_binding.lattice.frames_per_sample,
            new_binding.lattice.frames_per_sample
        );
    }
    let ungrouped = edit(
        &grouped,
        Command::Ungroup {
            node: node("grouped"),
        },
    );
    assert_eq!(ungrouped.audio_bindings(), grouped.audio_bindings());
    assert_eq!(ungrouped.audio_lineage(), grouped.audio_lineage());
}

fn with_sound(mut document: ProjectDocument) -> ProjectDocument {
    let asset = AssetId::new("sound").unwrap();
    document.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "Effect".into(),
            content_hash: "c".repeat(64),
            audio: Some(span()),
            video: None,
            frame_count: None,
            still_image: false,
            source_qualification: Some(SourceQualificationId::new("d".repeat(64)).unwrap()),
        },
    );
    let sound = SoundId::new("sound").unwrap();
    document.sounds.insert(
        sound.clone(),
        SoundEvent {
            owner: node("root"),
            label: "Effect".into(),
            source: SourceAudio {
                asset,
                span: span(),
            },
            mapping: SourceAudioMapping::natural_rate(
                span(),
                document.presentation_basis().frame_rate,
            )
            .unwrap(),
            offset: AudioSample(137),
            gain_millidecibels: -3000,
            start_edge: AudioEdgePolicy::Automatic,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        },
    );
    document.sound_allowances.insert(
        sound,
        SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {
            instance: InstancePath {
                node: node("a"),
                repeats: vec![],
            },
        }])
        .unwrap(),
    );
    document.validate().unwrap();
    document
}

#[test]
fn root_sound_routes_and_permissions_survive_whole_and_partial_grouping() {
    let initial = with_sound(tree(
        &["a", "suffix"],
        vec![("a", hold(40)), ("suffix", hold(40))],
    ));
    let routed = CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision("routed"),
        command: Command::RepeatSelection {
            parent: node("root"),
            selection: child("suffix"),
            plays: 2,
            identities: RepeatSelectionIdentities {
                repeat: node("routed-suffix"),
                group: None,
                split: SplitIdentities { nodes: vec![] },
            },
            timing: AudioTimingId {
                allocation: revision("routed"),
                ordinal: 0,
            },
        },
    };
    let before = crate::apply(&initial, &routed)
        .unwrap()
        .forward
        .apply(&initial)
        .unwrap();
    assert_eq!(
        before.sound_routes()[&SoundId::new("sound").unwrap()]
            .edits
            .len(),
        1
    );
    for selection in [child("a"), selected(3, 27)] {
        let grouped = edit(&before, command(&before, "root", selection));
        assert_eq!(grouped.sounds(), before.sounds());
        assert_eq!(grouped.sound_routes(), before.sound_routes());
        for issuer in grouped
            .sound_allowances()
            .values()
            .flat_map(|values| values.iter())
        {
            issuer.validate(&grouped).unwrap();
        }
        let ungrouped = edit(
            &grouped,
            Command::Ungroup {
                node: node("grouped"),
            },
        );
        assert_eq!(ungrouped.sounds(), before.sounds());
        assert_eq!(ungrouped.sound_routes(), before.sound_routes());
        assert_eq!(ungrouped.sound_allowances(), grouped.sound_allowances());
    }
    let grouped = edit(
        &before,
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 1,
            id: node("legacy"),
            label: "Group".into(),
        },
    );
    assert_eq!(grouped.sound_allowances(), before.sound_allowances());
    let after = edit(
        &grouped,
        Command::Ungroup {
            node: node("legacy"),
        },
    );
    assert_eq!(after.nodes(), before.nodes());
    assert_eq!(after.sound_allowances(), before.sound_allowances());
}

#[test]
fn partial_group_preserves_logical_mark_fragments_at_the_same_root_boundary() {
    let before = source_document();
    let marked = edit(
        &before,
        Command::SetMark {
            id: MarkId::new("inside").unwrap(),
            owner: node("root"),
            label: "Inside".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("processed"),
                    position: ExactRatio::integer(5),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    );
    let after = edit(&marked, command(&marked, "root", selected(4, 25)));
    let mark_id = MarkId::new("inside").unwrap();
    let query = |document: &ProjectDocument| {
        let resolved = AnchorIndex::new(document)
            .unwrap()
            .resolve(&SelectionRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                role: MediaRole::Linked,
                selector: BoundarySelector::Mark {
                    target: NamedMarkTarget {
                        id: mark_id.clone(),
                        occurrence: None,
                    },
                },
            })
            .unwrap();
        let ResolvedSelectionKind::Point { point } = resolved.selection else {
            panic!("named mark must resolve to a point")
        };
        point
    };
    let original = query(&marked);
    assert_eq!(original.exact_frame, ExactRatio::integer(7));
    assert_eq!(original.mark.as_ref().unwrap().bindings, vec![0]);
    let mark = &after.marks()[&mark_id];
    assert_eq!(mark.label, marked.marks()[&mark_id].label);
    assert_eq!(mark.loss_policy, AnchorLossPolicy::KeepUnresolved);
    assert_eq!(mark.boundary.bias, InsertionBias::Right);
    // Both endpoint cuts retain a complete physical owner. The outer copies
    // remain Bound but cropped out; only the middle binding is visible.
    assert_eq!(mark.binding_count(), 3);
    let index = AnchorIndex::new(&after).unwrap();
    let resolved: Vec<_> = mark
        .bindings()
        .map(|binding| {
            assert_eq!(binding.state, MarkState::Bound);
            assert_eq!(binding.owner, node("root"));
            let Anchor::Local { position, .. } = &binding.coordinate else {
                panic!("split must retain a Local mark binding")
            };
            assert_eq!(*position, ExactRatio::integer(5));
            index
                .resolve_target(&AnchorTarget {
                    boundary: BoundaryAnchor {
                        coordinate: binding.coordinate,
                        bias: mark.boundary.bias,
                    },
                    occurrence: None,
                })
                .map(|point| point.exact_frame)
                .map_err(|error| error.code)
        })
        .collect();
    assert_eq!(
        resolved,
        vec![
            Err(AnchorErrorCode::OutsideMapping),
            Ok(ExactRatio::integer(7)),
            Err(AnchorErrorCode::OutsideMapping),
        ]
    );
    let current = query(&after);
    assert_eq!(current.exact_frame, original.exact_frame);
    assert_eq!(current.frame, original.frame);
    assert_eq!(current.mark.as_ref().unwrap().id, mark_id);
    assert_eq!(current.mark.as_ref().unwrap().bindings, vec![1]);
    let ungrouped = edit(
        &after,
        Command::Ungroup {
            node: node("grouped"),
        },
    );
    assert_eq!(ungrouped.marks(), after.marks());
    assert_eq!(query(&ungrouped), current);
}

#[test]
fn added_depth_and_retained_identity_reuse_are_rejected_atomically() {
    let mut document = tree(&["entry"], vec![("entry", hold(1))]);
    document.nodes.remove(&node("entry"));
    document.nodes.insert(
        node("root"),
        BeatNode::sequence("Root", vec![node("deep-0")]),
    );
    for depth in 0..MAX_DOCUMENT_DEPTH {
        let id = node(&format!("deep-{depth}"));
        let beat = if depth + 1 == MAX_DOCUMENT_DEPTH {
            hold(1)
        } else {
            BeatNode::sequence("Level", vec![node(&format!("deep-{}", depth + 1))])
        };
        document.nodes.insert(id, beat);
    }
    document.validate().unwrap();
    let snapshot = document.clone();
    let requested = command(&document, "root", child("deep-0"));
    assert_eq!(
        crate::apply(&document, &request(&document, requested))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(document, snapshot);

    let mut document = source_document();
    document.audio_lineage.insert(
        node("voice"),
        AudioLineageId {
            allocation: revision("lineage"),
            origin: node("retired"),
        },
    );
    document.validate().unwrap();
    let mut requested = command(&document, "root", child("processed"));
    let Command::GroupSelection { identities, .. } = &mut requested else {
        panic!()
    };
    identities.group = node("retired");
    assert_eq!(
        crate::apply(&document, &request(&document, requested))
            .unwrap_err()
            .code,
        EditErrorCode::IdentityConflict
    );
}

#[test]
fn ungroup_reports_removed_mark_host_and_refuses_non_neutral_owners() {
    let before = tree(&["a"], vec![("a", hold(4))]);
    let grouped = edit(&before, command(&before, "root", child("a")));
    let marked = edit(
        &grouped,
        Command::SetMark {
            id: MarkId::new("group-mark").unwrap(),
            owner: node("root"),
            label: "Group mark".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("grouped"),
                    position: ExactRatio::ONE,
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    );
    let after = edit(
        &marked,
        Command::Ungroup {
            node: node("grouped"),
        },
    );
    assert_eq!(
        after.marks()[&MarkId::new("group-mark").unwrap()].state,
        MarkState::Unresolved {
            reason: MarkLossReason::HostMissing
        }
    );
    for variation in 0..4 {
        let mut refused = grouped.clone();
        let beat = refused.nodes.get_mut(&node("grouped")).unwrap();
        match variation {
            0 => beat.framing = Some(Framing::static_pose(FramingPose::default()).unwrap()),
            1 => beat.audio_edges.node_start = AudioEdgePolicy::Hard,
            2 => beat.audio_treatments = AudioTreatments::from_clip_gain(ClipGain::default()),
            _ => beat.audio_editorial_edges.start = true,
        }
        refused.validate().unwrap();
        let snapshot = refused.clone();
        assert!(
            crate::apply(
                &refused,
                &request(
                    &refused,
                    Command::Ungroup {
                        node: node("grouped")
                    }
                )
            )
            .is_err()
        );
        assert_eq!(refused, snapshot);
    }
}

#[test]
fn exact_pools_labels_and_scope_refuse_before_mutation() {
    let before = tree(&["a"], vec![("a", hold(8))]);
    let valid = command(&before, "root", selected(1, 7));
    let mut cases = Vec::new();
    for mode in 0..6 {
        let mut command = valid.clone();
        let Command::GroupSelection {
            identities,
            timing,
            label,
            ..
        } = &mut command
        else {
            panic!()
        };
        match mode {
            0 => {
                identities.split.nodes.pop();
            }
            1 => identities.split.nodes.push(node("surplus")),
            2 => identities.group = identities.split.nodes[0].clone(),
            3 => identities.group = node("a"),
            4 => timing.allocation = revision("other"),
            _ => *label = "x".repeat(1025),
        }
        cases.push(command);
    }
    for command in cases {
        let mut req = request(&before, command);
        req.new_revision = revision("group-edit");
        assert!(crate::apply(&before, &req).is_err());
    }
    assert!(before.group_selection(&node("a"), &child("a")).is_err());
    assert!(
        before
            .group_selection(&node("root"), &child("root"))
            .is_err()
    );
    assert!(
        before
            .group_selection(&node("root"), &selected(0, 9))
            .is_err()
    );
    assert!(validate_group_label("bad\0name").is_err());
    assert!(validate_group_label(&"é".repeat(512)).is_ok());
    assert!(validate_group_label(&"é".repeat(513)).is_err());
    assert!(validate_group_label("  ").is_ok());
    let mut wire = serde_json::to_value(valid).unwrap();
    wire["identities"]["unknown"] = true.into();
    assert!(serde_json::from_value::<Command>(wire).is_err());
    assert_eq!(before.nodes().len(), 2);
}
