use std::collections::BTreeMap;

use deadpan_core::*;
use proptest::prelude::*;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Pause",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn empty() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap()
}
fn edit(document: &ProjectDocument, command: Command, name: &str) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap();
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    result
}
fn tree(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(id, beat)| (node(id), beat))
        .collect();
    nodes.insert(
        node("group"),
        BeatNode::sequence("Group", children.iter().map(|id| node(id)).collect()),
    );
    edit(
        &empty(),
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                root: node("group"),
                nodes,
            },
        },
        "insert",
    )
}
fn target(coordinate: Anchor) -> AnchorTarget {
    AnchorTarget {
        boundary: BoundaryAnchor {
            coordinate,
            bias: InsertionBias::Right,
        },
        occurrence: None,
    }
}
fn local(id: &str, position: ExactRatio) -> AnchorTarget {
    target(Anchor::Local {
        node: node(id),
        position,
    })
}
fn request(document: &ProjectDocument, selector: BoundarySelector) -> SelectionRequest {
    SelectionRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        role: MediaRole::Linked,
        selector,
    }
}
fn point(document: &ProjectDocument, target: &AnchorTarget) -> ResolvedBoundary {
    let index = AnchorIndex::new(document).unwrap();
    let result = index
        .resolve(&request(
            document,
            BoundarySelector::Point {
                target: target.clone(),
            },
        ))
        .unwrap();
    assert_eq!(result.revision_id, *document.revision_id());
    let ResolvedSelectionKind::Point { point } = result.selection else {
        panic!()
    };
    point
}
fn retime(child: &str, start: i64, end: i64, frames: i64) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        kind: NodeKind::Retime {
            child: node(child),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            duration: duration(frames),
            pitch: PitchPolicy::Preserve,
        },
    }
}
fn play(document: &ProjectDocument, repeat: &str, ordinal: u32) -> RepeatInstance {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(repeat)].kind else {
        panic!()
    };
    RepeatInstance {
        node: node(repeat),
        iteration: iterations.at(ordinal).unwrap(),
    }
}

#[test]
fn nested_retime_boundaries_round_once_and_reject_cropped_content() {
    let document = tree(
        &["prefix", "outer"],
        vec![
            ("prefix", hold(11)),
            ("outer", retime("inner", 1, 8, 13)),
            ("inner", retime("leaf", 2, 17, 9)),
            ("leaf", hold(20)),
        ],
    );
    // leaf 7 -> inner 3 -> outer 26/7 -> project 103/7.
    let resolved = point(&document, &local("leaf", ExactRatio::integer(7)));
    assert_eq!(resolved.exact_frame, ExactRatio::new(103, 7).unwrap());
    assert_eq!(resolved.frame, ProjectFrame(15));
    let index = AnchorIndex::new(&document).unwrap();
    assert_eq!(
        index
            .resolve_target(&local("leaf", ExactRatio::integer(1)))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
    // Inside the leaf and first retime but cropped by the outer mapping.
    assert_eq!(
        index
            .resolve_target(&local("leaf", ExactRatio::integer(3)))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
    assert_eq!(
        point(&document, &local("outer", ExactRatio::integer(13))).frame,
        ProjectFrame(24)
    );
}

#[test]
fn occurrences_follow_stable_plays_and_groups_but_never_choose_a_play() {
    let initial = tree(&["leaf"], vec![("leaf", hold(7))]);
    let repeated = edit(
        &initial,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: node("leaf"),
            id: node("repeat"),
            plays: 4,
            gap: Some(HoldRecipe {
                duration: duration(2),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }),
        },
        "wrap",
    );
    let index = AnchorIndex::new(&repeated).unwrap();
    assert_eq!(
        index
            .resolve_target(&local("leaf", ExactRatio::integer(3)))
            .unwrap_err()
            .code,
        AnchorErrorCode::OccurrenceRequired
    );
    let path = InstancePath {
        node: node("leaf"),
        repeats: vec![play(&repeated, "repeat", 2)],
    };
    let anchored = target(Anchor::Occurrence {
        instance: path.clone(),
        position: ExactRatio::integer(3),
    });
    assert_eq!(point(&repeated, &anchored).frame, ProjectFrame(21));
    let scoped_local = AnchorTarget {
        occurrence: Some(path),
        ..local("leaf", ExactRatio::integer(3))
    };
    assert_eq!(point(&repeated, &scoped_local).frame, ProjectFrame(21));
    let moved = edit(
        &repeated,
        Command::MovePlays {
            node: node("repeat"),
            start: 2,
            end: 3,
            destination: 0,
        },
        "move",
    );
    assert_eq!(point(&moved, &anchored).frame, ProjectFrame(3));
    let grouped = edit(
        &moved,
        Command::Group {
            parent: node("group"),
            start: 0,
            end: 1,
            id: node("wrapper"),
            label: "Wrapper".into(),
        },
        "grouped",
    );
    assert_eq!(point(&grouped, &anchored).frame, ProjectFrame(3));
    let shrunk = edit(
        &repeated,
        Command::SetRepeat {
            node: node("repeat"),
            plays: 2,
            gap: None,
        },
        "shrink",
    );
    assert_eq!(
        AnchorIndex::new(&shrunk)
            .unwrap()
            .resolve_target(&anchored)
            .unwrap_err()
            .code,
        AnchorErrorCode::OccurrenceInvalid
    );
    let grown = edit(
        &shrunk,
        Command::SetRepeat {
            node: node("repeat"),
            plays: 4,
            gap: None,
        },
        "grow",
    );
    assert_eq!(
        AnchorIndex::new(&grown)
            .unwrap()
            .resolve_target(&anchored)
            .unwrap_err()
            .code,
        AnchorErrorCode::OccurrenceInvalid
    );
    // Index borrows the original immutable revision, independent of later edits.
    assert_eq!(
        index.resolve_target(&anchored).unwrap().frame,
        ProjectFrame(21)
    );
}

#[test]
fn nested_occurrences_require_exact_complete_order_and_scope() {
    let document = tree(&["leaf"], vec![("leaf", hold(3))]);
    let inner = edit(
        &document,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: node("leaf"),
            id: node("inner"),
            plays: 2,
            gap: None,
        },
        "inner",
    );
    let outer = edit(
        &inner,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: node("inner"),
            id: node("outer"),
            plays: 3,
            gap: None,
        },
        "outer",
    );
    let steps = vec![play(&outer, "outer", 2), play(&outer, "inner", 1)];
    let scoped = |repeats| {
        target(Anchor::Occurrence {
            instance: InstancePath {
                node: node("leaf"),
                repeats,
            },
            position: ExactRatio::ONE,
        })
    };
    assert_eq!(
        point(&outer, &scoped(steps.clone())).frame,
        ProjectFrame(16)
    );
    for bad in [
        vec![steps[1].clone()],
        vec![steps[1].clone(), steps[0].clone()],
        vec![steps[0].clone(), steps[1].clone(), steps[1].clone()],
        vec![steps[0].clone(); MAX_DOCUMENT_DEPTH + 1],
    ] {
        assert_eq!(
            AnchorIndex::new(&outer)
                .unwrap()
                .resolve_target(&scoped(bad))
                .unwrap_err()
                .code,
            AnchorErrorCode::OccurrenceInvalid
        );
    }
    let mut double = scoped(steps.clone());
    double.occurrence = Some(InstancePath {
        node: node("leaf"),
        repeats: steps,
    });
    assert_eq!(
        AnchorIndex::new(&outer)
            .unwrap()
            .resolve_target(&double)
            .unwrap_err()
            .code,
        AnchorErrorCode::InvalidAnchor
    );
}

#[test]
fn billions_of_plays_and_unused_overflowing_gap_do_not_expand() {
    let document = tree(&["leaf"], vec![("leaf", hold(3))]);
    let repeated = edit(
        &document,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: node("leaf"),
            id: node("repeat"),
            plays: u32::MAX,
            gap: None,
        },
        "repeat",
    );
    let anchored = target(Anchor::Occurrence {
        instance: InstancePath {
            node: node("leaf"),
            repeats: vec![play(&repeated, "repeat", u32::MAX - 1)],
        },
        position: ExactRatio::integer(3),
    });
    assert_eq!(
        point(&repeated, &anchored).frame,
        ProjectFrame(i64::from(u32::MAX) * 3)
    );
    let document = tree(&["leaf"], vec![("leaf", hold(i64::MAX))]);
    let repeated = edit(
        &document,
        Command::WrapRepeat {
            anchor_policy: WrapAnchorPolicy::First,
            node: node("leaf"),
            id: node("repeat"),
            plays: 1,
            gap: Some(HoldRecipe {
                duration: duration(i64::MAX),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }),
        },
        "repeat",
    );
    let anchored = target(Anchor::Occurrence {
        instance: InstancePath {
            node: node("leaf"),
            repeats: vec![play(&repeated, "repeat", 0)],
        },
        position: ExactRatio::integer(i64::MAX),
    });
    assert_eq!(point(&repeated, &anchored).frame, ProjectFrame(i64::MAX));
}

fn source_document() -> ProjectDocument {
    let span = |start, end, rate| {
        SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: SourceTimeBase::new(1, rate).unwrap(),
            },
            SourceTimestamp {
                ticks: end,
                time_base: SourceTimeBase::new(1, rate).unwrap(),
            },
        )
        .unwrap()
    };
    let video = span(-90_000, 90_000, 90_000);
    let audio = span(-48_000, 48_000, 48_000);
    let asset = AssetId::new("asset").unwrap();
    let document = edit(
        &empty(),
        Command::AddAsset {
            id: asset.clone(),
            asset: AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: Some(video),
                audio: Some(audio),
                still_image: false,
                frame_count: None,
            },
        },
        "asset",
    );
    edit(
        &document,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                root: node("source"),
                nodes: BTreeMap::from([(
                    node("source"),
                    BeatNode {
                        label: "Source".into(),
                        kind: NodeKind::Source {
                            source: SourceNode {
                                duration: duration(60),
                                video: SourceVideo::Stream {
                                    asset: asset.clone(),
                                    span: video,
                                },
                                audio: Some(SourceAudio { asset, span: audio }),
                                link: LinkRelation::Linked,
                                audio_offset: AudioSample(0),
                            },
                        },
                    },
                )]),
            },
        },
        "source",
    )
}
fn source_target(moment: SourceMoment) -> AnchorTarget {
    AnchorTarget {
        occurrence: Some(InstancePath {
            node: node("source"),
            repeats: vec![],
        }),
        ..target(Anchor::Source {
            asset: AssetId::new("asset").unwrap(),
            moment,
        })
    }
}
#[test]
fn source_pts_and_original_samples_preserve_negative_origin_and_clock_conversion() {
    let document = source_document();
    for target in [
        source_target(SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks: -45_000,
                time_base: SourceTimeBase::new(1, 90_000).unwrap(),
            },
        }),
        source_target(SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks: -1,
                time_base: SourceTimeBase::new(1, 2).unwrap(),
            },
        }),
        source_target(SourceMoment::AudioSample {
            sample: -22_050,
            sample_rate: 44_100,
        }),
    ] {
        assert_eq!(
            point(&document, &target).exact_frame,
            ExactRatio::integer(15)
        );
    }
    for (sample, expected) in [(-44_100, 0), (44_100, 60)] {
        assert_eq!(
            point(
                &document,
                &source_target(SourceMoment::AudioSample {
                    sample,
                    sample_rate: 44_100
                })
            )
            .frame,
            ProjectFrame(expected)
        );
    }
    let mut unscoped = source_target(SourceMoment::AudioSample {
        sample: 0,
        sample_rate: 44_100,
    });
    unscoped.occurrence = None;
    let index = AnchorIndex::new(&document).unwrap();
    assert_eq!(
        index.resolve_target(&unscoped).unwrap_err().code,
        AnchorErrorCode::OccurrenceRequired
    );
    assert_eq!(
        index
            .resolve_target(&source_target(SourceMoment::AudioSample {
                sample: 0,
                sample_rate: 0
            }))
            .unwrap_err()
            .code,
        AnchorErrorCode::InvalidAnchor
    );
    assert_eq!(
        index
            .resolve_target(&source_target(SourceMoment::AudioSample {
                sample: 44_101,
                sample_rate: 44_100
            }))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
}

#[test]
fn audio_offsets_use_mix_clock_and_are_not_silently_clamped() {
    let mut json = serde_json::to_value(source_document()).unwrap();
    json["nodes"]["source"]["kind"]["source"]["audio_offset"] = serde_json::json!(16016);
    let document = ProjectDocument::from_json(&json.to_string()).unwrap();
    // 16016 mix samples at 30000/1001 fps is exactly ten frames.
    assert_eq!(
        point(
            &document,
            &source_target(SourceMoment::AudioSample {
                sample: 0,
                sample_rate: 48000
            })
        )
        .frame,
        ProjectFrame(40)
    );
    assert_eq!(
        AnchorIndex::new(&document)
            .unwrap()
            .resolve_target(&source_target(SourceMoment::AudioSample {
                sample: 48000,
                sample_rate: 48000
            }))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutOfRange
    );
    json["nodes"]["source"]["kind"]["source"]["audio_offset"] = serde_json::json!(-16016);
    let document = ProjectDocument::from_json(&json.to_string()).unwrap();
    assert_eq!(
        point(
            &document,
            &source_target(SourceMoment::AudioSample {
                sample: 0,
                sample_rate: 48000
            })
        )
        .frame,
        ProjectFrame(20)
    );
}

#[test]
fn revision_guards_pinned_boundaries_and_quantized_ranges_are_explicit() {
    let document = tree(&["leaf"], vec![("leaf", hold(20))]);
    let index = AnchorIndex::new(&document).unwrap();
    let select = |start, end| {
        request(
            &document,
            BoundarySelector::Range {
                start: local("leaf", start),
                end: local("leaf", end),
            },
        )
    };
    let resolved = index
        .resolve(&select(
            ExactRatio::new(5, 2).unwrap(),
            ExactRatio::new(7, 2).unwrap(),
        ))
        .unwrap();
    let ResolvedSelectionKind::Range { start, end, frames } = resolved.selection else {
        panic!()
    };
    assert_eq!((start.frame, end.frame), (ProjectFrame(2), ProjectFrame(4)));
    assert_eq!(frames.duration(), duration(2));
    for (start, end) in [(5, 4), (4, 4)] {
        assert_eq!(
            index
                .resolve(&select(
                    ExactRatio::integer(start),
                    ExactRatio::integer(end)
                ))
                .unwrap_err()
                .code,
            AnchorErrorCode::InvalidRange
        );
    }
    assert_eq!(
        index
            .resolve(&select(
                ExactRatio::new(21, 10).unwrap(),
                ExactRatio::new(22, 10).unwrap()
            ))
            .unwrap_err()
            .code,
        AnchorErrorCode::InvalidRange
    );
    let at_end = target(Anchor::Sequence {
        frame: ProjectFrame(20),
    });
    assert_eq!(point(&document, &at_end).frame, ProjectFrame(20));
    for frame in [-1, 21] {
        assert_eq!(
            index
                .resolve_target(&target(Anchor::Sequence {
                    frame: ProjectFrame(frame)
                }))
                .unwrap_err()
                .code,
            AnchorErrorCode::OutOfRange
        );
    }
    let mut stale = request(&document, BoundarySelector::Point { target: at_end });
    stale.expected_revision = revision("old");
    let error = index.resolve(&stale).unwrap_err();
    assert_eq!(error.code, AnchorErrorCode::RevisionConflict);
    assert_eq!(error.current_revision, Some(document.revision_id().clone()));
    stale.project_id = ProjectId::new("other").unwrap();
    assert_eq!(
        index.resolve(&stale).unwrap_err().code,
        AnchorErrorCode::ProjectConflict
    );
    assert_eq!(
        point(&empty(), &local("root", ExactRatio::ZERO)).frame,
        ProjectFrame(0)
    );
}

#[test]
fn strict_wire_and_bias_round_trip_do_not_imply_edit_transforms() {
    let document = tree(&["leaf"], vec![("leaf", hold(10))]);
    let mut left = local("leaf", ExactRatio::new(7, 3).unwrap());
    left.boundary.bias = InsertionBias::Left;
    let selection = request(
        &document,
        BoundarySelector::Point {
            target: left.clone(),
        },
    );
    let mut json = serde_json::to_value(&selection).unwrap();
    assert_eq!(
        serde_json::from_value::<SelectionRequest>(json.clone()).unwrap(),
        selection
    );
    assert_eq!(point(&document, &left).target, left);
    json["selector"]["target"]["boundary"]["coordinate"]["seconds"] = serde_json::json!(1.0);
    assert!(serde_json::from_value::<SelectionRequest>(json).is_err());
    let overflow = local("leaf", ExactRatio::new(i128::MAX, 1).unwrap());
    assert_eq!(
        AnchorIndex::new(&document)
            .unwrap()
            .resolve_target(&overflow)
            .unwrap_err()
            .code,
        AnchorErrorCode::OutOfRange
    );
}

proptest! {
    #[test]
    fn nested_retimes_map_exactly_without_intermediate_rounding(leaf_frames in 1i64..10000, inner_frames in 1i64..10000, outer_frames in 1i64..10000, part in 0i64..1001, prefix in 1i64..10000) {
        let document = tree(&["prefix","outer"],vec![("prefix",hold(prefix)),("outer",retime("inner",0,inner_frames,outer_frames)),("inner",retime("leaf",0,leaf_frames,inner_frames)),("leaf",hold(leaf_frames))]);
        let position = ExactRatio::new(i128::from(leaf_frames)*i128::from(part),1000).unwrap();
        let expected = ExactRatio::new(i128::from(outer_frames)*i128::from(part),1000).unwrap().checked_add(ExactRatio::integer(prefix)).unwrap();
        let found = point(&document,&local("leaf",position));
        prop_assert_eq!(found.exact_frame,expected);
        prop_assert_eq!(i128::from(found.frame.0),expected.round_even().unwrap());
    }
    #[test]
    fn sequence_prefix_index_matches_expanded_boundary_sum(lengths in proptest::collection::vec(1i64..10000,1..100), pick in 0usize..100) {
        let selected = pick % lengths.len();
        let names: Vec<_> = (0..lengths.len()).map(|i| format!("h-{i}")).collect();
        let children: Vec<_> = names.iter().map(String::as_str).collect();
        let nodes = children.iter().zip(&lengths).map(|(id,d)| (*id,hold(*d))).collect();
        let document = tree(&children,nodes);
        let position = ExactRatio::new(i128::from(lengths[selected]),2).unwrap();
        let expected = position.checked_add(ExactRatio::integer(lengths[..selected].iter().sum())).unwrap();
        prop_assert_eq!(point(&document,&local(children[selected],position)).exact_frame,expected);
    }
}
