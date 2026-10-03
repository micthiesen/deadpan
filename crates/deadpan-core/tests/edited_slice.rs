use deadpan_core::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[path = "edited_slice/placement.rs"]
mod placement;

#[path = "edited_slice/child.rs"]
mod child;

#[path = "edited_slice/children.rs"]
mod children;

#[path = "edited_slice/replacement_children.rs"]
mod replacement_children;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn duration(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: duration(frames),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn tree(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    ProjectDocument::from_json(&json!({
        "schema_version": DOCUMENT_SCHEMA_VERSION, "project_id":"slice", "revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root", "nodes":nodes, "assets":{}, "marks":{}, "overrides":{},
    }).to_string()).unwrap()
}
fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    }
}
fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
    index: usize,
) -> CommandRequest {
    let required = slice.identity_requirements().unwrap();
    request(
        document,
        name,
        Command::SpliceSlice {
            parent: id("root"),
            index,
            slice: slice.clone(),
            timing: timing(name),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|n| id(&format!("{name}-node-{n}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|n| id(&format!("{name}-alias-{n}")))
                    .collect(),
            },
        },
    )
}
fn edit(document: &ProjectDocument, request: &CommandRequest) -> ProjectDocument {
    let tx = apply(document, request).unwrap();
    let after = tx.forward.apply(document).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
fn marked(document: &ProjectDocument, marks: Vec<(&str, Mark)>) -> ProjectDocument {
    let mut value = serde_json::to_value(document).unwrap();
    value["marks"] = serde_json::to_value(
        marks
            .into_iter()
            .map(|(id, mark)| (MarkId::new(id).unwrap(), mark))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}
fn mark(owner: &str, position: i64, bias: InsertionBias) -> Mark {
    Mark {
        owner: id(owner),
        label: position.to_string(),
        boundary: BoundaryAnchor {
            coordinate: Anchor::Local {
                node: id(owner),
                position: ExactRatio::integer(position),
            },
            bias,
        },
        loss_policy: AnchorLossPolicy::KeepUnresolved,
        state: MarkState::Bound,
        fragments: Vec::new(),
    }
}
fn copied_marks<'a>(document: &'a ProjectDocument, prefix: &str) -> Vec<&'a Mark> {
    document
        .marks()
        .iter()
        .filter(|(id, _)| id.as_str().starts_with(prefix))
        .map(|(_, mark)| mark)
        .collect()
}

#[test]
fn history_neutral_capture_survives_source_deletion_and_repeated_paste() {
    let before = tree(&["held"], vec![("held", hold(10))]);
    let snapshot = before.clone();
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 7), timing("capture")).unwrap();
    assert_eq!(before, snapshot);
    assert_eq!(slice.duration(), duration(5));
    assert_eq!(slice.identity_requirements().unwrap().nodes, 3);
    let wire = slice.to_json().unwrap();
    assert_eq!(CapturedEditSlice::from_json(&wire).unwrap(), slice);
    slice.validate_capture(&before).unwrap();
    let deleted = edit(
        &before,
        &request(
            &before,
            "deleted",
            Command::DeleteRange {
                parent: id("root"),
                range: range(0, 10),
                identities: SplitIdentities::default(),
                timing: timing("deleted"),
            },
        ),
    );
    let once = edit(&deleted, &paste(&deleted, &slice, "first", 0));
    let twice = edit(&once, &paste(&once, &slice, "second", 1));
    assert_eq!(slice.to_json().unwrap(), wire);
    assert_eq!(twice.duration().unwrap(), duration(10));
    assert!(!twice.nodes().contains_key(&id("held")));
    let first: BTreeSet<_> = once
        .nodes()
        .keys()
        .filter(|node| **node != id("root"))
        .cloned()
        .collect();
    let second: BTreeSet<_> = twice
        .nodes()
        .keys()
        .filter(|node| node.as_str().starts_with("second"))
        .cloned()
        .collect();
    assert!(first.is_disjoint(&second));
    let request = paste(&twice, &slice, "third", 2);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
}

#[test]
fn capture_provenance_rejects_other_revisions_and_valid_forged_content() {
    let before = tree(&["held"], vec![("held", hold(10))]);
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 7), timing("capture")).unwrap();
    let mut wire = serde_json::to_value(&slice).unwrap();
    wire["nodes"]["held"]["label"] = json!("Forged selected owner");
    let forged = CapturedEditSlice::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        forged.validate_capture(&before).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
    let mut changed = serde_json::to_value(&before).unwrap();
    changed["revision_id"] = json!("another-revision");
    let changed = ProjectDocument::from_json(&changed.to_string()).unwrap();
    assert_eq!(
        slice.validate_capture(&changed).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    assert_eq!(
        CapturedEditSlice::from_json(&slice.to_json().unwrap())
            .unwrap()
            .validate_capture(&before),
        Ok(())
    );
}

#[test]
fn pasted_contents_exclude_root_sound_bus_and_append_one_exact_route_edit() {
    let initial = tree(
        &["lead", "held", "tail"],
        vec![("lead", hold(1)), ("held", hold(6)), ("tail", hold(3))],
    );
    let asset = AssetId::new("catalog-sound").unwrap();
    let sound = SoundId::new("effect").unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 16_016,
            time_base,
        },
    )
    .unwrap();
    let registered = edit(
        &initial,
        &request(
            &initial,
            "registered",
            Command::AddAsset {
                id: asset.clone(),
                asset: AssetRecord {
                    label: "Effect".into(),
                    content_hash: "a".repeat(64),
                    audio: Some(span),
                    video: None,
                    frame_count: None,
                    still_image: false,
                    source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
                },
            },
        ),
    );
    let placed = edit(
        &registered,
        &request(
            &registered,
            "placed",
            Command::SetSound {
                id: sound.clone(),
                event: SoundEvent {
                    owner: id("root"),
                    label: "Effect".into(),
                    source: SourceAudio {
                        asset: asset.clone(),
                        span,
                    },
                    mapping: SourceAudioMapping::SelectedPlacement {
                        start: ExactRatio::ZERO,
                        frames: ExactRatio::integer(10),
                        selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(10))
                            .unwrap(),
                    },
                    offset: AudioSample(0),
                    gain_millidecibels: -1200,
                    start_edge: AudioEdgePolicy::Hard,
                    end_edge: AudioEdgePolicy::Hard,
                    overflow: SoundOverflowPolicy::Reject,
                },
            },
        ),
    );
    let issuer = SoundHoldIssuer::Node {
        instance: InstancePath {
            node: id("held"),
            repeats: Vec::new(),
        },
    };
    let allowed = edit(
        &placed,
        &request(
            &placed,
            "allowed",
            Command::SetSoundAllowance {
                sound: sound.clone(),
                issuer: issuer.clone(),
                allowed: true,
            },
        ),
    );
    let before = edit(
        &allowed,
        &request(
            &allowed,
            "routed",
            Command::InsertTime {
                at: ProjectFrame(1),
                hold: HoldRecipe {
                    duration: duration(1),
                    picture_context: None,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
                id: id("prior-pause"),
                identities: SplitIdentities::default(),
                timing: timing("routed"),
            },
        ),
    );
    let snapshot = before.clone();
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(3, 5), timing("capture-routed"))
            .unwrap();
    assert_eq!(before, snapshot);
    slice.validate_capture(&before).unwrap();
    let wire = serde_json::to_value(&slice).unwrap();
    for key in ["sounds", "sound_routes", "sound_allowances"] {
        assert!(wire.get(key).is_none());
    }
    assert!(wire["assets"].as_object().unwrap().is_empty());

    // Lead1, prior-pause1, held6, tail3: insertion index2 is global frame2.
    let after = edit(&before, &paste(&before, &slice, "sound-paste", 2));
    assert_eq!(after.duration().unwrap(), duration(13));
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sounds().len(), 1);
    assert_eq!(after.sound_allowances(), before.sound_allowances());
    assert_eq!(after.sound_allowances()[&sound].len(), 1);
    assert!(after.sound_allowances()[&sound].contains(&issuer));
    assert_eq!(after.sound_routes().len(), 1);
    let old_route = &before.sound_routes()[&sound];
    let route = &after.sound_routes()[&sound];
    assert_eq!(old_route.edits.len(), 1);
    assert_eq!(route.edits.len(), 2);
    assert_eq!(&route.edits[..1], &old_route.edits);
    assert_eq!(route.recipe_extent, duration(10));
    assert_eq!(route.recipe_grid, old_route.recipe_grid);
    assert_eq!(
        route.edits[1].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(2),
            duration: duration(2),
        }
    );
    assert_eq!(route.edits[1].grid, route.recipe_grid);
    assert_eq!(route.edits[1].cuts, RootSoundCutEdges::default());
    let exact =
        |a, b| ExactFrameRange::new(ExactRatio::integer(a), ExactRatio::integer(b)).unwrap();
    let query = route
        .compile()
        .unwrap()
        .query(exact(0, 13), Default::default())
        .unwrap();
    assert_eq!(
        query.slices,
        vec![
            SoundRouteSlice {
                destination: exact(0, 1),
                recipe: Some(exact(0, 1))
            },
            SoundRouteSlice {
                destination: exact(1, 4),
                recipe: None
            },
            SoundRouteSlice {
                destination: exact(4, 13),
                recipe: Some(exact(1, 10))
            },
        ]
    );
    // Independent 48k / (30000/1001) boundaries. The two chronological
    // shifts sum to 4804 samples, unlike one freshly rounded 3-frame shift.
    let rate = route.recipe_grid.frame_rate;
    assert_eq!(route.recipe_grid.frame_origin, ExactRatio::ZERO);
    for (frame, sample) in [
        (1, 1602),
        (2, 3203),
        (3, 4805),
        (4, 6406),
        (10, 16016),
        (13, 20821),
    ] {
        assert_eq!(
            rate.audio_boundary(ProjectFrame(frame)).unwrap(),
            AudioSample(sample)
        );
    }
    let prior_shift = rate.audio_boundary(ProjectFrame(2)).unwrap().0
        - rate.audio_boundary(ProjectFrame(1)).unwrap().0;
    let paste_shift = rate.audio_boundary(ProjectFrame(4)).unwrap().0
        - rate.audio_boundary(ProjectFrame(2)).unwrap().0;
    assert_eq!((prior_shift, paste_shift), (1601, 3203));
    assert_eq!(16016 + prior_shift + paste_shift, 20820);
}

#[test]
fn partial_marks_keep_exact_positions_bias_and_each_fragments_state() {
    let before = tree(&["held"], vec![("held", hold(10))]);
    let mut marks = Vec::new();
    for (name, point, bias) in [
        ("before", 1, InsertionBias::Right),
        ("in-left", 2, InsertionBias::Left),
        ("in-right", 2, InsertionBias::Right),
        ("middle", 4, InsertionBias::Right),
        ("out-left", 7, InsertionBias::Left),
        ("out-right", 7, InsertionBias::Right),
        ("after", 8, InsertionBias::Left),
    ] {
        let mut value = mark("held", point, bias);
        value.label = name.into();
        marks.push((name, value));
    }
    let mut mixed = mark("held", 0, InsertionBias::Right);
    mixed.label = "mixed".into();
    mixed.fragments = vec![
        MarkFragment {
            owner: id("held"),
            coordinate: Anchor::Local {
                node: id("held"),
                position: ExactRatio::new(39, 10).unwrap(),
            },
            state: MarkState::Bound,
        },
        MarkFragment {
            owner: id("held"),
            coordinate: Anchor::Local {
                node: id("held"),
                position: ExactRatio::new(41, 10).unwrap(),
            },
            state: MarkState::Bound,
        },
        MarkFragment {
            owner: id("held"),
            coordinate: Anchor::Local {
                node: id("held"),
                position: ExactRatio::integer(0),
            },
            state: MarkState::Unresolved {
                reason: MarkLossReason::OutsideMapping,
            },
        },
    ];
    marks.push(("mixed", mixed));
    let before = marked(&before, marks);
    let initial = before.clone();
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 7), timing("capture")).unwrap();
    assert_eq!(slice.identity_requirements().unwrap().marks, 4);
    let after = edit(&before, &paste(&before, &slice, "pasted", 1));
    let copied = copied_marks(&after, "pasted");
    assert_eq!(
        copied
            .iter()
            .map(|mark| mark.label.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["in-right", "middle", "out-left", "mixed"])
    );
    let mixed = copied.iter().find(|mark| mark.label == "mixed").unwrap();
    assert_eq!(mixed.binding_count(), 3);
    let bindings: Vec<_> = mixed.bindings().collect();
    assert_ne!(bindings[0].coordinate, bindings[1].coordinate);
    assert!(matches!(
        bindings[2].state,
        MarkState::Unresolved {
            reason: MarkLossReason::OutsideMapping
        }
    ));
    assert_eq!(
        bindings[2].coordinate,
        Anchor::Local {
            node: id("held"),
            position: ExactRatio::ZERO
        }
    );
    assert_eq!(before, initial);
}

#[test]
fn whole_units_keep_hidden_marks_partial_endpoints_filter_them() {
    let partition = BeatNode {
        label: "Fragment".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("held"),
            duration: duration(8),
            mapping: range(1, 9),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Partition,
        },
    };
    let before = marked(
        &tree(&["part"], vec![("part", partition), ("held", hold(10))]),
        vec![
            ("hidden", mark("held", 0, InsertionBias::Right)),
            ("visible", mark("held", 4, InsertionBias::Right)),
        ],
    );
    let whole =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 8), timing("whole")).unwrap();
    let partial =
        CapturedEditSlice::capture(&before, &id("root"), range(1, 6), timing("partial")).unwrap();
    assert_eq!(whole.identity_requirements().unwrap().marks, 2);
    assert_eq!(partial.identity_requirements().unwrap().marks, 1);
    let after = edit(&before, &paste(&before, &whole, "pasted", 1));
    let hidden = copied_marks(&after, "pasted")
        .into_iter()
        .find(|mark| mark.label == "0")
        .unwrap();
    assert_eq!(hidden.state, MarkState::Bound);
    assert_eq!(
        AnchorIndex::new(&after)
            .unwrap()
            .resolve_target(&AnchorTarget {
                boundary: hidden.boundary.clone(),
                occurrence: None
            })
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
}

#[test]
fn pins_stay_absolute_and_resolve_loss_policy_without_rebinding_dormant_intent() {
    let before = tree(
        &["held", "tail"],
        vec![("held", hold(5)), ("tail", hold(5))],
    );
    let mut keep = mark("held", 0, InsertionBias::Right);
    keep.label = "keep".into();
    keep.boundary.coordinate = Anchor::Sequence {
        frame: ProjectFrame(9),
    };
    let mut delete = keep.clone();
    delete.label = "delete".into();
    delete.loss_policy = AnchorLossPolicy::DeleteOwned;
    let mut root = keep.clone();
    root.owner = id("root");
    root.label = "root".into();
    let before = marked(
        &before,
        vec![("keep", keep), ("delete", delete), ("root", root)],
    );
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 5), timing("capture")).unwrap();
    assert_eq!(slice.identity_requirements().unwrap().marks, 2);
    let empty = edit(
        &before,
        &request(
            &before,
            "delete-all",
            Command::DeleteRange {
                parent: id("root"),
                range: range(0, 10),
                identities: SplitIdentities::default(),
                timing: timing("delete-all"),
            },
        ),
    );
    let after = edit(&empty, &paste(&empty, &slice, "pasted", 0));
    let copied = copied_marks(&after, "pasted");
    assert_eq!(copied.len(), 1);
    assert_eq!(
        copied[0].boundary.coordinate,
        Anchor::Sequence {
            frame: ProjectFrame(9)
        }
    );
    assert_eq!(
        copied[0].state,
        MarkState::Unresolved {
            reason: MarkLossReason::OutOfRange
        }
    );
}

#[test]
fn missing_external_host_uses_loss_policy_and_wire_cannot_bind_to_omitted_parent() {
    let before = tree(
        &["held", "tail"],
        vec![("held", hold(5)), ("tail", hold(5))],
    );
    let mut value = mark("held", 2, InsertionBias::Right);
    value.boundary.coordinate = Anchor::Local {
        node: id("tail"),
        position: ExactRatio::integer(2),
    };
    let before = marked(&before, vec![("external", value)]);
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 5), timing("capture")).unwrap();
    let after = edit(&before, &paste(&before, &slice, "pasted", 2));
    assert_eq!(
        copied_marks(&after, "pasted")[0].state,
        MarkState::Unresolved {
            reason: MarkLossReason::HostMissing
        }
    );
    let mut wire: serde_json::Value = serde_json::from_str(&slice.to_json().unwrap()).unwrap();
    wire["marks"]["external"]["state"] = json!({"type":"bound"});
    wire["marks"]["external"]["boundary"]["coordinate"]["node"] = json!("root");
    assert!(CapturedEditSlice::from_json(&wire.to_string()).is_err());
}

#[test]
fn insufficient_conflicting_stale_and_overflow_requests_fail_atomically() {
    let before = tree(&["held"], vec![("held", hold(10))]);
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 7), timing("capture")).unwrap();
    let snapshot = before.clone();
    for variant in 0..6 {
        let mut request = paste(&before, &slice, "pasted", 1);
        let Command::SpliceSlice {
            identities, timing, ..
        } = &mut request.command
        else {
            panic!()
        };
        match variant {
            0 => {
                identities.authored.nodes.pop();
            }
            1 => {
                identities.aliases.pop();
            }
            2 => identities.authored.nodes[0] = id("held"),
            3 => identities.aliases[0] = identities.authored.nodes[0].clone(),
            4 => timing.ordinal = u32::MAX,
            _ => request.expected_revision = revision("stale"),
        }
        assert!(apply(&before, &request).is_err(), "variant {variant}");
        assert_eq!(before, snapshot);
    }
    for (parent, start, end) in [
        ("root", 2, 2),
        ("root", 0, 11),
        ("held", 0, 5),
        ("missing", 0, 5),
    ] {
        assert!(
            CapturedEditSlice::capture(&before, &id(parent), range(start, end), timing("bad"))
                .is_err()
        );
    }
}

#[test]
fn source_marks_use_the_captured_explicit_occurrence_and_exact_boundary() {
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
        SourceTimestamp {
            ticks: 10,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(tree(&["held"], vec![("held", hold(10))])).unwrap();
    wire["assets"] = json!({"video":AssetRecord {label:"Video".into(),content_hash:"a".repeat(64),video:Some(span),audio:None,still_image:false,frame_count:None,source_qualification:None}});
    wire["nodes"]["held"]["kind"] = serde_json::to_value(NodeKind::Source {
        source: SourceNode {
            edit_window: None,
            duration: duration(10),
            video: SourceVideo::Stream {
                asset: AssetId::new("video").unwrap(),
                span,
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            link: LinkRelation::Independent,
            audio_offset: AudioSample(0),
        },
    })
    .unwrap();
    let mut marks = BTreeMap::new();
    for ticks in [1, 2, 4, 7, 8] {
        let mut value = mark("held", ticks, InsertionBias::Right);
        value.boundary.coordinate = Anchor::Source {
            asset: AssetId::new("video").unwrap(),
            moment: SourceMoment::Timestamp {
                stream: SourceStream::Video,
                timestamp: SourceTimestamp {
                    ticks,
                    time_base: SourceTimeBase::new(1, 30).unwrap(),
                },
            },
        };
        marks.insert(format!("source-{ticks}"), value);
    }
    wire["marks"] = serde_json::to_value(marks).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 7), timing("capture")).unwrap();
    assert_eq!(slice.identity_requirements().unwrap().marks, 2);
    let after = edit(&before, &paste(&before, &slice, "pasted", 1));
    assert_eq!(
        copied_marks(&after, "pasted")
            .iter()
            .map(|mark| mark.label.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["2", "4"])
    );
}

#[test]
fn wire_mark_and_scoped_play_union_limits_fail_without_cloning_a_paste() {
    let before = marked(
        &tree(&["held"], vec![("held", hold(10))]),
        vec![("mark", mark("held", 4, InsertionBias::Right))],
    );
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 7), timing("capture")).unwrap();
    let mut wire: serde_json::Value = serde_json::from_str(&slice.to_json().unwrap()).unwrap();
    let binding = MarkFragment {
        owner: id("held"),
        coordinate: Anchor::Local {
            node: id("held"),
            position: ExactRatio::integer(4),
        },
        state: MarkState::Bound,
    };
    wire["marks"]["mark"]["fragments"] =
        serde_json::to_value(vec![binding; MAX_MARK_BINDINGS]).unwrap();
    assert!(CapturedEditSlice::from_json(&wire.to_string()).is_err());
    wire["marks"]["mark"]["fragments"] = json!([]);
    wire["parts"][0]["mapping"] = serde_json::to_value(range(0, 11)).unwrap();
    assert!(CapturedEditSlice::from_json(&wire.to_string()).is_err());
}

#[test]
fn existing_root_local_mark_shifts_once_while_sequence_pin_stays_absolute() {
    let mut pin = mark("root", 8, InsertionBias::Right);
    pin.boundary.coordinate = Anchor::Sequence {
        frame: ProjectFrame(8),
    };
    let before = marked(
        &tree(&["held"], vec![("held", hold(10))]),
        vec![
            ("local", mark("root", 8, InsertionBias::Right)),
            ("pin", pin),
        ],
    );
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 7), timing("capture")).unwrap();
    assert_eq!(slice.identity_requirements().unwrap().marks, 0);
    let after = edit(&before, &paste(&before, &slice, "pasted", 0));
    assert_eq!(
        after.marks()[&MarkId::new("local").unwrap()]
            .boundary
            .coordinate,
        Anchor::Local {
            node: id("root"),
            position: ExactRatio::integer(13)
        }
    );
    assert_eq!(
        after.marks()[&MarkId::new("pin").unwrap()]
            .boundary
            .coordinate,
        Anchor::Sequence {
            frame: ProjectFrame(8)
        }
    );
}

#[test]
fn partial_windows_preserve_the_original_external_endpoint_biases() {
    let before = marked(
        &tree(&["held"], vec![("held", hold(10))]),
        vec![
            ("left", mark("held", 0, InsertionBias::Left)),
            ("right", mark("held", 10, InsertionBias::Right)),
        ],
    );
    for (start, end, label) in [(0, 7, "0"), (2, 10, "10")] {
        let slice =
            CapturedEditSlice::capture(&before, &id("root"), range(start, end), timing("capture"))
                .unwrap();
        assert_eq!(slice.identity_requirements().unwrap().marks, 1);
        let after = edit(&before, &paste(&before, &slice, "pasted", 1));
        assert_eq!(copied_marks(&after, "pasted")[0].label, label);
        assert_eq!(copied_marks(&after, "pasted")[0].state, MarkState::Bound);
    }
}

#[test]
fn partial_copy_of_an_untreated_copy_retains_every_complete_context() {
    let before = tree(&["held"], vec![("held", hold(10))]);
    let first =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 7), timing("capture")).unwrap();
    let once = edit(&before, &paste(&before, &first, "first", 1));
    let second = CapturedEditSlice::capture(
        &once,
        &id("first-node-0"),
        range(11, 14),
        timing("second-capture"),
    )
    .unwrap();
    let twice = edit(&once, &paste(&once, &second, "second", 2));
    let third = CapturedEditSlice::capture(
        &twice,
        &id("second-node-0"),
        range(16, 17),
        timing("third-capture"),
    )
    .unwrap();
    let final_document = edit(&twice, &paste(&twice, &third, "third", 3));
    assert_eq!(final_document.duration().unwrap(), duration(19));
    assert!(
        final_document
            .nodes()
            .values()
            .filter(|node| matches!(node.kind, NodeKind::Hold { .. }))
            .all(|node| {
                matches!(&node.kind, NodeKind::Hold { recipe } if recipe.duration == duration(10))
            })
    );
}

#[test]
fn lineage_allocation_origin_pairs_rename_injectively_across_live_and_history() {
    let mut left = hold(3);
    left.label = "left".into();
    let mut right = hold(4);
    right.label = "right".into();
    let mut wire = serde_json::to_value(tree(
        &["left", "right"],
        vec![("left", left), ("right", right)],
    ))
    .unwrap();
    wire["audio_lineage"] = json!({
        "left": {"allocation":"one", "origin":"same-historical-name"},
        "right": {"allocation":"two", "origin":"same-historical-name"},
    });
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(0, 7), timing("capture")).unwrap();
    let after = edit(&before, &paste(&before, &slice, "pasted", 2));
    let copies: Vec<_> = after
        .nodes()
        .iter()
        .filter(|(node, _)| {
            node.as_str().starts_with("pasted-node-") && **node != id("pasted-node-0")
        })
        .map(|(id, _)| &after.audio_lineage()[id])
        .collect();
    assert_eq!(copies.len(), 2);
    assert_ne!(copies[0], copies[1]);
    assert_ne!(copies[0].origin, copies[1].origin);
    assert!(
        copies
            .iter()
            .all(|value| value.allocation == revision("pasted"))
    );
    for lineage in copies {
        assert!(after.audio_bindings().timings().values().any(|layout| {
            layout
                .audio_lineage()
                .values()
                .any(|value| value == lineage)
        }));
    }
}

#[test]
fn incompatible_historical_play_union_rejects_capture_without_expanding_plays() {
    let old = tree(
        &["repeat"],
        vec![
            (
                "repeat",
                BeatNode {
                    label: "Repeat".into(),
                    framing: None,
                    audio_treatments: Default::default(),
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("held"),
                        iterations: IterationOrder::new(revision("old-plays"), u32::MAX).unwrap(),
                        gap: None,
                    },
                },
            ),
            ("held", hold(1)),
        ],
    );
    let bindings = capture_unbound_audio_bindings(&old, timing("old-timing")).unwrap();
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    wire["nodes"]["repeat"]["kind"]["iterations"] =
        serde_json::to_value(IterationOrder::new(revision("new-plays"), u32::MAX).unwrap())
            .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let snapshot = before.clone();
    assert_eq!(
        CapturedEditSlice::capture(
            &before,
            &id("root"),
            range(0, i64::from(u32::MAX)),
            timing("capture")
        )
        .unwrap_err()
        .code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(before, snapshot);
}
