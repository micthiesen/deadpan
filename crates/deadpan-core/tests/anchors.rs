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
        audio_edges: Default::default(),
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
                source_qualification: None,
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
                        audio_edges: Default::default(),
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
                                audio_mapping: SourceAudioMapping::FitBeat,
                                video_mapping: SourceVideoMapping::FitBeat,
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

fn short_audio_document(rate: FrameRate) -> (ProjectDocument, SourceSpan) {
    let mut json = serde_json::to_value(source_document()).unwrap();
    json["presentation_basis"]["frame_rate"] = serde_json::to_value(rate).unwrap();
    // Keep the original two-second asset available, but select only one second
    // of audio under the two-second picture. Preserve the negative source origin.
    json["nodes"]["source"]["kind"]["source"]["audio"]["span"]["end"]["ticks"] =
        serde_json::json!(0);
    let document = ProjectDocument::from_json(&json.to_string()).unwrap();
    let NodeKind::Source { source } = &document.nodes()[&node("source")].kind else {
        panic!()
    };
    let span = source.audio.as_ref().unwrap().span;
    (document, span)
}

#[test]
fn natural_audio_duration_is_independent_of_picture_and_preserves_alignment() {
    let rate = FrameRate::new(30, 1).unwrap();
    let (original, span) = short_audio_document(rate);
    // The old fit behavior remains explicit and unchanged.
    assert_eq!(
        point(
            &original,
            &source_target(SourceMoment::AudioSample {
                sample: 0,
                sample_rate: 48000
            })
        )
        .frame,
        ProjectFrame(60)
    );
    for (offset, expected) in [(0, [0, 15, 30]), (24000, [15, 30, 45])] {
        let document = edit(
            &original,
            Command::SetSourceAudioMapping {
                node: node("source"),
                mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
                offset: AudioSample(offset),
            },
            "natural",
        );
        for (sample, frame) in [-44100, -22050, 0].into_iter().zip(expected) {
            let resolved = point(
                &document,
                &source_target(SourceMoment::AudioSample {
                    sample,
                    sample_rate: 44100,
                }),
            );
            assert_eq!(resolved.exact_frame, ExactRatio::integer(frame));
        }
        assert_eq!(document.duration().unwrap(), original.duration().unwrap());
        for timestamp in [-90000, 0, 90000] {
            let target = source_target(SourceMoment::Timestamp {
                stream: SourceStream::Video,
                timestamp: SourceTimestamp {
                    ticks: timestamp,
                    time_base: SourceTimeBase::new(1, 90000).unwrap(),
                },
            });
            assert_eq!(
                point(&document, &target).exact_frame,
                point(&original, &target).exact_frame
            );
        }
    }
}

#[test]
fn natural_video_mapping_keeps_exact_source_time_and_independent_audio() {
    let original = source_document();
    let video = original.assets()[&AssetId::new("asset").unwrap()]
        .video
        .unwrap();
    let mapped = edit(
        &original,
        Command::SetSourceVideoMapping {
            node: node("source"),
            mapping: SourceVideoMapping::natural_rate(
                video,
                original.presentation_basis().frame_rate,
                EndpointPolicy::HoldAdjacent,
            )
            .unwrap(),
        },
        "natural-video",
    );
    for ticks in [-90000, -45000, 0, 90000] {
        let target = source_target(SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks,
                time_base: SourceTimeBase::new(1, 90000).unwrap(),
            },
        });
        // Original elapsed seconds × rational project fps, from a negative origin.
        let expected = ExactRatio::new(i128::from(ticks + 90000), 3003).unwrap();
        assert_eq!(point(&mapped, &target).exact_frame, expected);
        let equivalent = source_target(SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks: ticks / 3,
                time_base: SourceTimeBase::new(1, 30000).unwrap(),
            },
        });
        assert_eq!(point(&mapped, &equivalent).exact_frame, expected);
    }
    for sample in [-48000, 0, 48000] {
        let target = source_target(SourceMoment::AudioSample {
            sample,
            sample_rate: 48000,
        });
        assert_eq!(point(&mapped, &target), point(&original, &target));
    }
    assert_eq!(mapped.duration().unwrap(), duration(60));
}

#[test]
fn video_mapping_keeps_source_marks_and_composes_isolated_retimed_occurrences() {
    let original = source_document();
    let endpoint = source_target(SourceMoment::Timestamp {
        stream: SourceStream::Video,
        timestamp: SourceTimestamp {
            ticks: 90000,
            time_base: SourceTimeBase::new(1, 90000).unwrap(),
        },
    });
    let marked = edit(
        &original,
        Command::SetMark {
            id: MarkId::new("video-end").unwrap(),
            owner: node("source"),
            label: "Video end".into(),
            boundary: endpoint.boundary.clone(),
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
        "marked-video",
    );
    let mapping = SourceVideoMapping::Duration {
        frames: ExactRatio::new(71, 2).unwrap(),
        endpoints: EndpointPolicy::Reject,
    };
    let mapped = edit(
        &marked,
        Command::SetSourceVideoMapping {
            node: node("source"),
            mapping,
        },
        "mapped-video",
    );
    assert_eq!(mapped.marks(), marked.marks());
    let repeated = edit(
        &original,
        Command::WrapRepeat {
            node: node("source"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
        "repeat-video",
    );
    let first = play(&repeated, "repeat", 0);
    let second = play(&repeated, "repeat", 1);
    let isolated = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: node("source"),
                repeats: vec![second.clone()],
            },
            edit: OccurrenceEdit::SetSourceVideoMapping { mapping },
            identities: OccurrenceIdentities {
                nodes: vec![node("isolated")],
                marks: vec![],
            },
        },
        "isolated-video",
    );
    let mut json = serde_json::to_value(&isolated).unwrap();
    json["nodes"]["root"]["kind"]["children"] = serde_json::json!(["retime"]);
    json["nodes"]["retime"] = serde_json::to_value(retime("repeat", 0, 120, 173)).unwrap();
    let document = ProjectDocument::from_json(&json.to_string()).unwrap();
    for (host, occurrence, frames) in [
        ("source", first, ExactRatio::integer(60)),
        ("isolated", second, ExactRatio::new(191, 2).unwrap()),
    ] {
        let mut target = endpoint.clone();
        target.occurrence = Some(InstancePath {
            node: node(host),
            repeats: vec![occurrence],
        });
        assert_eq!(
            point(&document, &target).exact_frame,
            frames
                .checked_mul(ExactRatio::new(173, 120).unwrap())
                .unwrap()
        );
    }
}

#[test]
fn endpoint_holding_does_not_rebind_video_anchors_outside_the_beat() {
    let original = source_document();
    let mapped = edit(
        &original,
        Command::SetSourceVideoMapping {
            node: node("source"),
            mapping: SourceVideoMapping::Duration {
                frames: ExactRatio::integer(120),
                endpoints: EndpointPolicy::HoldAdjacent,
            },
        },
        "long-video",
    );
    let target = |ticks| {
        source_target(SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks,
                time_base: SourceTimeBase::new(1, 90000).unwrap(),
            },
        })
    };
    assert_eq!(
        point(&mapped, &target(0)).exact_frame,
        ExactRatio::integer(60)
    );
    assert_eq!(
        AnchorIndex::new(&mapped)
            .unwrap()
            .resolve_target(&target(90000))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutOfRange
    );
}

#[test]
fn invalid_video_mapping_and_non_stream_targets_reject_atomically() {
    let document = source_document();
    let request = |mapping| CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("invalid-video"),
        command: Command::SetSourceVideoMapping {
            node: node("source"),
            mapping,
        },
    };
    for frames in [
        ExactRatio::ZERO,
        ExactRatio::integer(-1),
        ExactRatio::new(i128::from(i64::MAX) + 1, 1).unwrap(),
    ] {
        let mapping = SourceVideoMapping::Duration {
            frames,
            endpoints: EndpointPolicy::Reject,
        };
        assert!(apply(&document, &request(mapping)).is_err());
        assert!(
            serde_json::from_value::<SourceVideoMapping>(serde_json::to_value(mapping).unwrap())
                .is_err()
        );
    }
    let encoded = serde_json::to_value(&document).unwrap();
    for mapping in [
        serde_json::Value::Null,
        serde_json::json!({"type":"fit_beat","endpoints":null}),
        serde_json::json!({"type":"duration","frames":{"numerator":"3","denominator":"2"}}),
        serde_json::json!({"type":"duration","frames":{"numerator":"3","denominator":"2"},"endpoints":"clamp"}),
    ] {
        let mut json = encoded.clone();
        json["nodes"]["source"]["kind"]["source"]["video_mapping"] = mapping;
        assert!(ProjectDocument::from_json(&json.to_string()).is_err());
    }
    let mut missing = encoded.clone();
    missing["nodes"]["source"]["kind"]["source"]
        .as_object_mut()
        .unwrap()
        .remove("video_mapping");
    assert!(ProjectDocument::from_json(&missing.to_string()).is_err());
    for video in [
        serde_json::json!({"type":"blank"}),
        serde_json::json!({"type":"still","asset":"asset"}),
    ] {
        let mut json = encoded.clone();
        json["assets"]["asset"]["still_image"] = serde_json::json!(true);
        json["nodes"]["source"]["kind"]["source"]["video"] = video;
        json["nodes"]["source"]["kind"]["source"]["link"] = serde_json::json!("independent");
        let no_stream = ProjectDocument::from_json(&json.to_string()).unwrap();
        assert_eq!(
            apply(&no_stream, &request(SourceVideoMapping::FitBeat))
                .unwrap_err()
                .code,
            EditErrorCode::SourceRangeInvalid
        );
        json["nodes"]["source"]["kind"]["source"]["video_mapping"] =
            serde_json::to_value(SourceVideoMapping::Duration {
                frames: ExactRatio::integer(30),
                endpoints: EndpointPolicy::Reject,
            })
            .unwrap();
        assert!(ProjectDocument::from_json(&json.to_string()).is_err());
    }
}

proptest! {
    #[test]
    fn natural_video_mapping_preserves_exact_elapsed_time(ticks in -90000i64..=90000,
        rate_num in 1u32..=120000, rate_den in 1u32..=1001) {
        let original = source_document();
        let span = original.assets()[&AssetId::new("asset").unwrap()].video.unwrap();
        let mut json = serde_json::to_value(&original).unwrap();
        let rate = FrameRate::new(rate_num, rate_den).unwrap();
        json["presentation_basis"]["frame_rate"] = serde_json::to_value(rate).unwrap();
        let original = ProjectDocument::from_json(&json.to_string()).unwrap();
        let document = edit(&original, Command::SetSourceVideoMapping { node: node("source"),
            mapping: SourceVideoMapping::natural_rate(span, rate, EndpointPolicy::HoldAdjacent).unwrap(),
        }, "natural-video");
        let target = source_target(SourceMoment::Timestamp { stream: SourceStream::Video,
            timestamp: SourceTimestamp { ticks, time_base: SourceTimeBase::new(1, 90000).unwrap() },
        });
        let expected = ExactRatio::new(i128::from(ticks + 90000) * i128::from(rate_num), 90000 * i128::from(rate_den)).unwrap();
        let result = AnchorIndex::new(&document).unwrap().resolve_target(&target);
        if expected.compare_integer(60).is_gt() {
            prop_assert_eq!(result.unwrap_err().code, AnchorErrorCode::OutOfRange);
        } else {
            prop_assert_eq!(result.unwrap().exact_frame, expected);
        }
    }
}

#[test]
fn exact_audio_duration_composes_through_repeat_and_retime_without_rounding() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let (original, span) = short_audio_document(rate);
    let document = edit(
        &original,
        Command::SetSourceAudioMapping {
            node: node("source"),
            mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
            offset: AudioSample(16016),
        },
        "natural",
    );
    let repeated = edit(
        &document,
        Command::WrapRepeat {
            node: node("source"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
        "repeat",
    );
    let mut json = serde_json::to_value(&repeated).unwrap();
    json["nodes"]["root"]["kind"]["children"] = serde_json::json!(["retime"]);
    json["nodes"]["retime"] = serde_json::to_value(retime("repeat", 0, 120, 173)).unwrap();
    let document = ProjectDocument::from_json(&json.to_string()).unwrap();
    let mut target = source_target(SourceMoment::AudioSample {
        sample: 0,
        sample_rate: 48000,
    });
    target.occurrence.as_mut().unwrap().repeats = vec![play(&document, "repeat", 1)];
    let expected = ExactRatio::integer(70)
        .checked_add(ExactRatio::new(30000, 1001).unwrap())
        .unwrap()
        .checked_mul(ExactRatio::new(173, 120).unwrap())
        .unwrap();
    let resolved = point(&document, &target);
    assert_eq!(resolved.exact_frame, expected);
    assert_eq!(i128::from(resolved.frame.0), expected.round_even().unwrap());
}

#[test]
fn audio_mapping_edits_keep_source_marks_fixed_and_isolate_one_repeat_play() {
    let rate = FrameRate::new(30, 1).unwrap();
    let (original, span) = short_audio_document(rate);
    let mark_id = MarkId::new("audio-end").unwrap();
    let marked = edit(
        &original,
        Command::SetMark {
            id: mark_id.clone(),
            owner: node("source"),
            label: "Audio end".into(),
            boundary: source_target(SourceMoment::AudioSample {
                sample: 0,
                sample_rate: 48000,
            })
            .boundary,
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
        "marked",
    );
    let mapping = SourceAudioMapping::natural_rate(span, rate).unwrap();
    let mapped = edit(
        &marked,
        Command::SetSourceAudioMapping {
            node: node("source"),
            mapping,
            offset: AudioSample(0),
        },
        "mapped",
    );
    assert_eq!(mapped.marks(), marked.marks());
    let repeated = edit(
        &original,
        Command::WrapRepeat {
            node: node("source"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
        "repeat",
    );
    let first_play = play(&repeated, "repeat", 0);
    let second_play = play(&repeated, "repeat", 1);
    let isolated = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: node("source"),
                repeats: vec![second_play.clone()],
            },
            edit: OccurrenceEdit::SetSourceAudioMapping {
                mapping,
                offset: AudioSample(24000),
            },
            identities: OccurrenceIdentities {
                nodes: vec![node("isolated")],
                marks: vec![],
            },
        },
        "isolated-edit",
    );
    for (host, occurrence, expected) in [("source", first_play, 60), ("isolated", second_play, 105)]
    {
        let mut target = source_target(SourceMoment::AudioSample {
            sample: 0,
            sample_rate: 48000,
        });
        target.occurrence = Some(InstancePath {
            node: node(host),
            repeats: vec![occurrence],
        });
        assert_eq!(point(&isolated, &target).frame, ProjectFrame(expected));
    }
    assert_eq!(isolated.duration().unwrap(), repeated.duration().unwrap());
}

#[test]
fn signed_audio_offsets_keep_excluded_boundaries_unavailable() {
    let rate = FrameRate::new(30, 1).unwrap();
    let (original, span) = short_audio_document(rate);
    let document = edit(
        &original,
        Command::SetSourceAudioMapping {
            node: node("source"),
            mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
            offset: AudioSample(-24000),
        },
        "negative",
    );
    let index = AnchorIndex::new(&document).unwrap();
    assert_eq!(
        index
            .resolve_target(&source_target(SourceMoment::AudioSample {
                sample: -48000,
                sample_rate: 48000,
            }))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutOfRange
    );
    for (sample, expected) in [(-24000, 0), (0, 15)] {
        assert_eq!(
            point(
                &document,
                &source_target(SourceMoment::AudioSample {
                    sample,
                    sample_rate: 48000,
                })
            )
            .frame,
            ProjectFrame(expected)
        );
    }
    let delayed = edit(
        &original,
        Command::SetSourceAudioMapping {
            node: node("source"),
            mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
            offset: AudioSample(72000),
        },
        "positive-tail",
    );
    for (sample, expected) in [(-48000, 45), (-24000, 60)] {
        assert_eq!(
            point(
                &delayed,
                &source_target(SourceMoment::AudioSample {
                    sample,
                    sample_rate: 48000,
                })
            )
            .frame,
            ProjectFrame(expected)
        );
    }
    assert_eq!(
        AnchorIndex::new(&delayed)
            .unwrap()
            .resolve_target(&source_target(SourceMoment::AudioSample {
                sample: 0,
                sample_rate: 48000
            }))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutOfRange
    );
}

#[test]
fn invalid_audio_extents_and_missing_selections_reject_atomically() {
    let (document, _) = short_audio_document(FrameRate::new(30, 1).unwrap());
    for frames in [
        ExactRatio::ZERO,
        ExactRatio::integer(-1),
        ExactRatio::new(i128::from(i64::MAX) + 1, 1).unwrap(),
    ] {
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision("invalid"),
            command: Command::SetSourceAudioMapping {
                node: node("source"),
                mapping: SourceAudioMapping::Duration { frames },
                offset: AudioSample(24000),
            },
        };
        assert!(apply(&document, &request).is_err());
        let mut json = serde_json::to_value(&document).unwrap();
        json["nodes"]["source"]["kind"]["source"]["audio_mapping"] =
            serde_json::to_value(SourceAudioMapping::Duration { frames }).unwrap();
        assert!(ProjectDocument::from_json(&json.to_string()).is_err());
    }
    let encoded = serde_json::to_value(&document).unwrap();
    for mapping in [
        serde_json::Value::Null,
        serde_json::json!({"type":"fit_beat","frames":{"numerator":"30","denominator":"1"}}),
    ] {
        let mut json = encoded.clone();
        json["nodes"]["source"]["kind"]["source"]["audio_mapping"] = mapping;
        assert!(ProjectDocument::from_json(&json.to_string()).is_err());
    }
    let mut missing = encoded.clone();
    missing["nodes"]["source"]["kind"]["source"]
        .as_object_mut()
        .unwrap()
        .remove("audio_mapping");
    assert!(ProjectDocument::from_json(&missing.to_string()).is_err());
    let mut no_audio = encoded;
    no_audio["nodes"]["source"]["kind"]["source"]["audio"] = serde_json::Value::Null;
    no_audio["nodes"]["source"]["kind"]["source"]["link"] = serde_json::json!("independent");
    let no_audio = ProjectDocument::from_json(&no_audio.to_string()).unwrap();
    let result = apply(
        &no_audio,
        &CommandRequest {
            project_id: no_audio.project_id().clone(),
            expected_revision: no_audio.revision_id().clone(),
            new_revision: revision("missing-audio"),
            command: Command::SetSourceAudioMapping {
                node: node("source"),
                mapping: SourceAudioMapping::FitBeat,
                offset: AudioSample(0),
            },
        },
    );
    assert_eq!(result.unwrap_err().code, EditErrorCode::SourceRangeInvalid);
}

proptest! {
    #[test]
    fn natural_audio_mapping_preserves_fractional_sample_positions(
        sample in -48000i64..=0,
        rate_num in 1u32..=120000,
        rate_den in 1u32..=1001,
    ) {
        let rate = FrameRate::new(rate_num, rate_den).unwrap();
        let (original, span) = short_audio_document(rate);
        let mapping = SourceAudioMapping::natural_rate(span, rate).unwrap();
        let exact = ExactRatio::new(i128::from(sample + 48000) * i128::from(rate_num),
            48000 * i128::from(rate_den)).unwrap();
        let document = edit(&original, Command::SetSourceAudioMapping {
            node: node("source"), mapping, offset: AudioSample(0),
        }, "natural");
        let target = source_target(SourceMoment::AudioSample { sample, sample_rate: 48000 });
        let result = AnchorIndex::new(&document).unwrap().resolve_target(&target);
        // Extents may exceed the host. The mapping is never stretched or
        // clamped to make an unavailable boundary appear valid.
        if exact.compare_integer(60) == std::cmp::Ordering::Greater {
            prop_assert_eq!(result.unwrap_err().code, AnchorErrorCode::OutOfRange);
        } else {
            prop_assert_eq!(result.unwrap().exact_frame, exact);
        }
    }
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

#[test]
fn independent_stream_placements_preserve_exact_anchors_offsets_and_inverse_history() {
    let original = source_document();
    for (video_start, audio_start) in [(3, -5), (-3, 5)] {
        let video_start = ExactRatio::new(video_start, 2).unwrap();
        let audio_start = ExactRatio::new(audio_start, 3).unwrap();
        let audio_offset = AudioSample(137);
        let video_frames = ExactRatio::new(101, 2).unwrap();
        let audio_frames = ExactRatio::new(137, 3).unwrap();
        let marked = edit(
            &original,
            Command::SetMark {
                id: MarkId::new("source-midpoint").unwrap(),
                owner: node("source"),
                label: "Original midpoint".into(),
                boundary: source_target(SourceMoment::AudioSample {
                    sample: 0,
                    sample_rate: 48000,
                })
                .boundary,
                loss_policy: AnchorLossPolicy::KeepUnresolved,
            },
            "marked-placement",
        );
        let video = edit(
            &marked,
            Command::SetSourceVideoMapping {
                node: node("source"),
                mapping: SourceVideoMapping::Placement {
                    start: video_start,
                    frames: video_frames,
                    endpoints: EndpointPolicy::HoldAdjacent,
                },
            },
            "placed-video",
        );
        let document = edit(
            &video,
            Command::SetSourceAudioMapping {
                node: node("source"),
                mapping: SourceAudioMapping::Placement {
                    start: audio_start,
                    frames: audio_frames,
                },
                offset: audio_offset,
            },
            "placed-audio",
        );
        assert_eq!(document.marks(), marked.marks());
        assert_eq!(document.duration().unwrap(), original.duration().unwrap());
        assert_eq!(document.assets(), original.assets());
        let original_video_midpoint = source_target(SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 90000).unwrap(),
            },
        });
        assert_eq!(
            point(&document, &original_video_midpoint).exact_frame,
            video_start
                .checked_add(video_frames.checked_div(ExactRatio::integer(2)).unwrap())
                .unwrap()
        );
        let expected_audio = audio_start
            .checked_add(ExactRatio::new(685, 8008).unwrap())
            .unwrap()
            .checked_add(audio_frames.checked_div(ExactRatio::integer(2)).unwrap())
            .unwrap();
        assert_eq!(
            point(
                &document,
                &source_target(SourceMoment::AudioSample {
                    sample: 0,
                    sample_rate: 48000
                })
            )
            .exact_frame,
            expected_audio
        );
        assert_eq!(
            ProjectDocument::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );
        // Placement does not allow inverse anchors to jump into endpoint holds.
        if video_start.compare_integer(0).is_lt() {
            let target = source_target(SourceMoment::Timestamp {
                stream: SourceStream::Video,
                timestamp: SourceTimestamp {
                    ticks: -90000,
                    time_base: SourceTimeBase::new(1, 90000).unwrap(),
                },
            });
            assert_eq!(
                AnchorIndex::new(&document)
                    .unwrap()
                    .resolve_target(&target)
                    .unwrap_err()
                    .code,
                AnchorErrorCode::OutOfRange
            );
        }
    }
}

#[test]
fn placement_boundaries_and_combined_audio_arithmetic_reject_atomically() {
    let document = source_document();
    for (start, frames) in [
        (ExactRatio::ZERO, ExactRatio::ZERO),
        (ExactRatio::ZERO, ExactRatio::integer(-1)),
        (
            ExactRatio::new(i128::from(i64::MIN) * 2 - 1, 2).unwrap(),
            ExactRatio::ONE,
        ),
        (
            ExactRatio::new(i128::from(i64::MAX) * 2 + 1, 2).unwrap(),
            ExactRatio::ONE,
        ),
        (
            ExactRatio::integer(i64::MAX),
            ExactRatio::new(1, 2).unwrap(),
        ),
        (
            ExactRatio::new(1, i128::MAX).unwrap(),
            ExactRatio::new(1, i128::MAX - 1).unwrap(),
        ),
    ] {
        let audio = SourceAudioMapping::Placement { start, frames };
        let video = SourceVideoMapping::Placement {
            start,
            frames,
            endpoints: EndpointPolicy::Reject,
        };
        assert!(
            serde_json::from_value::<SourceAudioMapping>(serde_json::to_value(audio).unwrap())
                .is_err()
        );
        assert!(
            serde_json::from_value::<SourceVideoMapping>(serde_json::to_value(video).unwrap())
                .is_err()
        );
        for command in [
            Command::SetSourceAudioMapping {
                node: node("source"),
                mapping: audio,
                offset: AudioSample(0),
            },
            Command::SetSourceVideoMapping {
                node: node("source"),
                mapping: video,
            },
        ] {
            assert!(
                apply(
                    &document,
                    &CommandRequest {
                        project_id: document.project_id().clone(),
                        expected_revision: document.revision_id().clone(),
                        new_revision: revision("invalid-placement"),
                        command
                    }
                )
                .is_err()
            );
        }
    }
    // The unshifted placement is valid, but adding the independent mix offset
    // overflows the exact representation. Admission rejects it before a query.
    let mapping = SourceAudioMapping::Placement {
        start: ExactRatio::new(1, i128::MAX).unwrap(),
        frames: ExactRatio::new(1, i128::MAX).unwrap(),
    };
    assert!(mapping.duration_frames(duration(60)).is_ok());
    assert!(
        apply(
            &document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: revision("overflowing-offset"),
                command: Command::SetSourceAudioMapping {
                    node: node("source"),
                    mapping,
                    offset: AudioSample(1)
                }
            }
        )
        .is_err()
    );
    for start in [
        ExactRatio::integer(i64::MIN),
        ExactRatio::new(i128::from(i64::MAX) * 2 - 1, 2).unwrap(),
    ] {
        let frames = ExactRatio::new(1, 2).unwrap();
        assert!(
            SourceAudioMapping::Placement { start, frames }
                .duration_frames(duration(60))
                .is_ok()
        );
        assert!(
            SourceVideoMapping::Placement {
                start,
                frames,
                endpoints: EndpointPolicy::Reject
            }
            .duration_frames(duration(60))
            .is_ok()
        );
    }
}

proptest! {
    #[test]
    fn signed_fractional_placements_map_independent_source_midpoints_exactly(
        start_numerator in -100i128..=100,
        denominator in 1i128..=100,
        offset in -1600i64..=1600,
    ) {
        let original = source_document();
        let start = ExactRatio::new(start_numerator, denominator).unwrap();
        let frames = ExactRatio::new(101, 2).unwrap();
        let document = edit(&original, Command::SetSourceAudioMapping {
            node: node("source"), mapping: SourceAudioMapping::Placement { start, frames }, offset: AudioSample(offset),
        }, "property-placement");
        let expected = start.checked_add(ExactRatio::new(i128::from(offset) * 5, 8008).unwrap()).unwrap().checked_add(ExactRatio::new(101, 4).unwrap()).unwrap();
        let target = source_target(SourceMoment::AudioSample { sample: 0, sample_rate: 48000 });
        let result = AnchorIndex::new(&document).unwrap().resolve_target(&target);
        if expected.compare_integer(0).is_lt() || expected.compare_integer(60).is_gt() {
            prop_assert_eq!(result.unwrap_err().code, AnchorErrorCode::OutOfRange);
        } else {
            prop_assert_eq!(result.unwrap().exact_frame, expected);
        }
    }
}

#[test]
fn audio_placement_isolates_one_play_and_retains_source_marks_through_retimes() {
    let original = source_document();
    let coordinate = source_target(SourceMoment::AudioSample {
        sample: 0,
        sample_rate: 48000,
    });
    let marked = edit(
        &original,
        Command::SetMark {
            id: MarkId::new("audio-mid").unwrap(),
            owner: node("source"),
            label: "Original sample".into(),
            boundary: coordinate.boundary.clone(),
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
        "marked-audio-placement",
    );
    let repeated = edit(
        &marked,
        Command::WrapRepeat {
            node: node("source"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
        "repeated-audio-placement",
    );
    let first = play(&repeated, "repeat", 0);
    let second = play(&repeated, "repeat", 1);
    let isolated = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: node("source"),
                repeats: vec![second.clone()],
            },
            edit: OccurrenceEdit::SetSourceAudioMapping {
                mapping: SourceAudioMapping::Placement {
                    start: ExactRatio::new(-5, 3).unwrap(),
                    frames: ExactRatio::integer(45),
                },
                offset: AudioSample(8008),
            },
            identities: OccurrenceIdentities {
                nodes: vec![node("audio-isolated")],
                marks: vec![MarkId::new("isolated-audio-mid").unwrap()],
            },
        },
        "isolated-audio-placement",
    );
    assert_eq!(
        isolated.nodes()[&node("source")],
        repeated.nodes()[&node("source")]
    );
    let mut wire = serde_json::to_value(&isolated).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["outer-rate"]);
    wire["nodes"]["inner-rate"] = serde_json::to_value(retime("repeat", 0, 120, 100)).unwrap();
    wire["nodes"]["outer-rate"] = serde_json::to_value(retime("inner-rate", 0, 100, 200)).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    for (host, play, expected) in [
        ("source", first, ExactRatio::integer(50)),
        ("audio-isolated", second, ExactRatio::new(2575, 18).unwrap()),
    ] {
        let mut target = coordinate.clone();
        target.occurrence = Some(InstancePath {
            node: node(host),
            repeats: vec![play],
        });
        assert_eq!(point(&document, &target).exact_frame, expected);
    }
    for name in ["audio-mid", "isolated-audio-mid"] {
        let mark = &document.marks()[&MarkId::new(name).unwrap()];
        assert_eq!(mark.state, MarkState::Bound);
        assert_eq!(mark.boundary, coordinate.boundary);
    }
}
