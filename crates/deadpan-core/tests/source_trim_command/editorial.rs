use super::*;

fn edges(document: &ProjectDocument, node: &str) -> AudioEditorialEdges {
    document.nodes()[&id(node)].audio_editorial_edges
}

fn marked(start: bool, end: bool) -> AudioEditorialEdges {
    AudioEditorialEdges { start, end }
}

#[test]
fn trim_marks_only_the_changed_join_and_preserves_every_existing_policy() {
    for (edge, delta) in [
        (SourceTrimEdge::In, 2),
        (SourceTrimEdge::In, -2),
        (SourceTrimEdge::Out, 2),
        (SourceTrimEdge::Out, -2),
    ] {
        let before = modify(&fixture(Some((0, 147000)), 7), |wire| {
            let source = wire["nodes"]["source"].clone();
            wire["nodes"]["previous"] = source.clone();
            wire["nodes"]["next"] = source;
            wire["nodes"]["root"]["kind"]["children"] = json!(["previous", "source", "next"]);
            wire["nodes"]["previous"]["audio_edges"] = serde_json::to_value(AudioEdgePolicies {
                node_end: AudioEdgePolicy::Hard,
                ..Default::default()
            })
            .unwrap();
        });
        let resolved = before
            .source_trim(
                &id("root"),
                &id("source"),
                edge,
                delta,
                SourceTrimMode::Ripple,
            )
            .unwrap();
        let after = edit(&before, trim(&before, "source", edge, delta));
        let target = if resolved.needs_wrapper {
            "initial-crop"
        } else {
            "source"
        };
        assert_eq!(
            edges(&after, target),
            marked(edge == SourceTrimEdge::In, edge == SourceTrimEdge::Out)
        );
        assert_eq!(
            edges(&after, "previous"),
            marked(false, edge == SourceTrimEdge::In)
        );
        assert_eq!(
            edges(&after, "next"),
            marked(edge == SourceTrimEdge::Out, false)
        );
        assert_eq!(edges(&after, "root"), marked(false, false));
        assert_eq!(
            after.nodes()[&id("previous")].audio_edges,
            before.nodes()[&id("previous")].audio_edges
        );
        if resolved.needs_wrapper {
            assert_eq!(edges(&after, "source"), marked(false, false));
        }
        assert_eq!(source(&after), &resolved.after);
    }
}

#[test]
fn trim_neighbor_crosses_ordinary_scopes_and_empty_nodes_but_stops_at_silence() {
    for silent in [false, true] {
        let before = modify(&fixture(None, 0), |wire| {
            wire["nodes"]["previous"] = wire["nodes"]["source"].clone();
            wire["nodes"]["empty"] =
                serde_json::to_value(BeatNode::sequence("Empty", vec![])).unwrap();
            wire["nodes"]["inner_empty"] = wire["nodes"]["empty"].clone();
            wire["nodes"]["group"] = serde_json::to_value(BeatNode::sequence(
                "Group",
                vec![id("inner_empty"), id("source")],
            ))
            .unwrap();
            let mut children = vec!["previous", "empty"];
            if silent {
                wire["nodes"]["silence"] = serde_json::to_value(BeatNode::hold(
                    "Silence",
                    HoldRecipe {
                        picture_context: None,
                        duration: frames(2),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                    },
                ))
                .unwrap();
                children.push("silence");
            }
            children.push("group");
            wire["nodes"]["root"]["kind"]["children"] = json!(children);
        });
        let after = edit(
            &before,
            Command::TrimSource {
                parent: id("group"),
                node: id("source"),
                edge: SourceTrimEdge::In,
                delta_frames: 1,
                mode: SourceTrimMode::Ripple,
                wrapper: Some(id("crop")),
                timing: AudioTimingId {
                    allocation: RevisionId::new("initialx").unwrap(),
                    ordinal: 0,
                },
            },
        );
        assert_eq!(edges(&after, "crop"), marked(true, false));
        assert_eq!(edges(&after, "previous"), marked(false, !silent));
        if silent {
            assert_eq!(edges(&after, "silence"), marked(false, true));
        }
        for node in ["group", "empty", "inner_empty", "root"] {
            assert_eq!(edges(&after, node), marked(false, false));
        }
    }
}

#[test]
fn retrim_keeps_one_sided_choices_and_split_retains_the_marked_owner_context() {
    let before = fixture(Some((0, 147000)), 0);
    let cropped = edit(&before, trim(&before, "source", SourceTrimEdge::In, 2));
    let crop = only_child(&cropped);
    let hard = edit(
        &cropped,
        Command::SetAudioEdge {
            node: crop.clone(),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
    );
    assert!(
        apply(
            &hard,
            &request(
                &hard,
                Command::SetAudioEdge {
                    node: crop.clone(),
                    edge: AudioBoundaryKind::NodeEnd,
                    policy: AudioEdgePolicy::Hard,
                }
            )
        )
        .is_err()
    );
    let extended = edit(&hard, trim(&hard, crop.as_str(), SourceTrimEdge::In, -3));
    assert_eq!(only_child(&extended), crop);
    assert_eq!(edges(&extended, crop.as_str()), marked(true, false));
    assert_eq!(
        extended.nodes()[&crop].audio_edges.node_start,
        AudioEdgePolicy::Hard
    );
    assert_eq!(source(&extended).duration, frames(11));
    let context = extended.nodes()[&crop].clone();
    let split = edit(
        &extended,
        Command::Split {
            node: crop.clone(),
            at: frames(4),
            identities: SplitIdentities {
                nodes: (0..12).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    assert_eq!(split.nodes()[&crop], context);
    let NodeKind::Sequence { children } = &split.nodes()[&id("root")].kind else {
        panic!()
    };
    assert_eq!(children.len(), 2);
    for child in children {
        assert_eq!(
            split.nodes()[child].audio_editorial_edges,
            marked(false, false)
        );
        let NodeKind::Retime {
            child: retained,
            purpose: RetimePurpose::Partition,
            ..
        } = &split.nodes()[child].kind
        else {
            panic!()
        };
        assert_eq!(
            split.nodes()[retained].audio_editorial_edges,
            marked(true, false)
        );
        assert_eq!(
            split.nodes()[retained].audio_edges.node_start,
            AudioEdgePolicy::Hard
        );
    }
    let capture = CapturedEditSlice::capture(
        &split,
        &id("root"),
        FrameRange::new(ProjectFrame(0), ProjectFrame(3)).unwrap(),
        AudioTimingId {
            allocation: RevisionId::new("copy").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let wire = serde_json::to_value(&capture).unwrap();
    assert_eq!(
        wire["nodes"][crop.as_str()]["audio_editorial_edges"],
        json!({"start":true,"end":false})
    );
    let decoded: CapturedEditSlice = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded, capture);
    decoded.validate_capture(&split).unwrap();
}

#[test]
fn marked_sequence_cannot_be_ungrouped_and_discard_its_editorial_edge() {
    let before = modify(&fixture(None, 0), |wire| {
        wire["nodes"]["group"] =
            serde_json::to_value(BeatNode::sequence("Group", vec![id("source")])).unwrap();
        wire["nodes"]["group"]["audio_editorial_edges"] = json!({"start":true,"end":false});
        wire["nodes"]["root"]["kind"]["children"] = json!(["group"]);
    });
    let error = apply(
        &before,
        &request(&before, Command::Ungroup { node: id("group") }),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(error.message.contains("editorial audio edges"));
}
