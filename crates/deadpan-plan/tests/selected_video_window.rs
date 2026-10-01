//! A wider decoding context must not widen the selected picture interval.

use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{Picture, RenderPlan};

fn node_id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn asset() -> AssetId {
    AssetId::new("video").unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn clock() -> SourceTimeBase {
    SourceTimeBase::new(1, 60_000).unwrap()
}

fn rate() -> FrameRate {
    FrameRate::new(30_000, 1001).unwrap()
}

fn span(start: i64, end: i64) -> SourceSpan {
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: clock(),
        },
        SourceTimestamp {
            ticks: end,
            time_base: clock(),
        },
    )
    .unwrap()
}

fn exact_span(start: ExactRatio, end: ExactRatio) -> ExactSourceSpan {
    ExactSourceSpan::new(
        SourcePoint {
            ticks: start,
            time_base: clock(),
        },
        SourcePoint {
            ticks: end,
            time_base: clock(),
        },
    )
    .unwrap()
}

fn beat(kind: NodeKind) -> BeatNode {
    BeatNode {
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        label: "Selected picture fixture".into(),
        kind,
    }
}

fn source(context: SourceSpan, mapping: SourceVideoMapping) -> BeatNode {
    beat(NodeKind::Source {
        source: SourceNode {
            edit_window: None,
            duration: duration(2),
            video: SourceVideo::Stream {
                asset: asset(),
                span: context,
            },
            video_mapping: mapping,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    })
}

fn background(frames: i64) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        picture_context: None,
        audio: HoldAudio::Silence,
    }
}

fn retime(child: &str, frames: i64, child_frames: i64) -> BeatNode {
    beat(NodeKind::Retime {
        purpose: RetimePurpose::Edit,
        child: node_id(child),
        duration: duration(frames),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(child_frames)).unwrap(),
        pitch: PitchPolicy::Preserve,
    })
}

fn document(context: SourceSpan, roots: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("selected-video-window").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node_id("root"),
    )
    .unwrap();
    let mut json = serde_json::to_value(empty).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (node_id(name), node))
        .collect();
    nodes.insert(
        node_id("root"),
        BeatNode::sequence("Root", roots.iter().map(|name| node_id(name)).collect()),
    );
    json["nodes"] = serde_json::to_value(nodes).unwrap();
    json["assets"] = serde_json::to_value(BTreeMap::from([(
        asset(),
        AssetRecord {
            source_qualification: None,
            label: "Measured video".into(),
            content_hash: "a".repeat(64),
            video: Some(context),
            audio: None,
            still_image: false,
            frame_count: Some(duration(8)),
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&json.to_string()).unwrap()
}

fn index(pts: &[i64], end: i64) -> SourceFrameIndex {
    SourceFrameIndex::new(
        asset(),
        clock(),
        pts.iter()
            .enumerate()
            .map(|(ordinal, pts)| IndexedSourceFrame {
                identity: SourceFrameId(u64::try_from(ordinal).unwrap()),
                pts: *pts,
                reported_duration: None,
                keyframe: ordinal == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            })
            .collect(),
        end,
        TerminalProvenance::Explicit,
    )
    .unwrap()
}

fn natural_selection(endpoints: EndpointPolicy) -> SourceVideoMapping {
    SourceVideoMapping::SelectedPlacement {
        start: ExactRatio::integer(-2),
        frames: ExactRatio::integer(6),
        selection: ExactFrameRange {
            start: ExactRatio::ZERO,
            end: ratio(3, 2),
        },
        endpoints,
    }
}

fn vfr_index(origin: i64) -> SourceFrameIndex {
    let pts: Vec<_> = [0, 1400, 4004, 4600, 6200, 7007, 8200]
        .map(|value| origin + value)
        .into();
    index(&pts, origin + 12012)
}

fn source_fields(picture: &Picture) -> (SourcePoint, SourceSpan, ExactSourceSpan) {
    let Picture::Source {
        point,
        span,
        selection,
        ..
    } = picture
    else {
        panic!("expected source picture, got {picture:?}")
    };
    (*point, *span, *selection)
}

#[test]
fn full_context_promotion_preserves_vfr_picture_and_rounded_tail_at_signed_origins() {
    // At 30000/1001 fps, 3003 ticks in this source clock occupy exactly
    // 3/2 frames. Ties-to-even occupancy is two frames, so the second center
    // reaches the selected exclusive end. Later original pictures exist there.
    for origin in [-10010, 13013] {
        let full = span(origin, origin + 12012);
        let narrow = span(origin + 4004, origin + 7007);
        let narrow_mapping =
            SourceVideoMapping::natural_rate(narrow, rate(), EndpointPolicy::HoldAdjacent).unwrap();
        assert_eq!(
            narrow_mapping.duration_frames(duration(2)).unwrap(),
            ratio(3, 2)
        );
        assert_eq!(ratio(3, 2).round_even().unwrap(), 2);
        let promoted_mapping = natural_selection(EndpointPolicy::HoldAdjacent);
        assert_eq!(
            promoted_mapping.selection_frames(duration(2)).unwrap(),
            ExactFrameRange {
                start: ExactRatio::ZERO,
                end: ratio(3, 2)
            }
        );
        let expected_selection = exact_span(
            ExactRatio::integer(origin + 4004),
            ExactRatio::integer(origin + 7007),
        );
        assert_eq!(
            promoted_mapping
                .selection_in_source(full, duration(2))
                .unwrap(),
            expected_selection
        );
        let narrow_plan = RenderPlan::compile(&document(
            full,
            &["source"],
            vec![("source", source(narrow, narrow_mapping))],
        ))
        .unwrap();
        let promoted_plan = RenderPlan::compile(&document(
            full,
            &["source"],
            vec![("source", source(full, promoted_mapping))],
        ))
        .unwrap();
        let source_index = vfr_index(origin);
        for (frame, expected_ticks, ordinal) in [(0, origin + 5005, 3), (1, origin + 7007, 4)] {
            let narrow_picture = narrow_plan.picture(ProjectFrame(frame)).unwrap().picture;
            let promoted_picture = promoted_plan.picture(ProjectFrame(frame)).unwrap().picture;
            let (point, context, selection) = source_fields(&promoted_picture);
            assert_eq!(point.ticks, ExactRatio::integer(expected_ticks));
            assert_eq!(point.time_base, clock());
            assert_eq!(context, full);
            assert_eq!(selection, expected_selection);
            assert_eq!(source_fields(&narrow_picture).1, narrow);
            assert_eq!(source_fields(&narrow_picture).0, point);
            assert_eq!(source_fields(&narrow_picture).2, expected_selection);
            assert_eq!(
                promoted_picture
                    .select_source_frame(&source_index)
                    .unwrap()
                    .identity,
                SourceFrameId(ordinal)
            );
            assert_eq!(
                narrow_picture
                    .select_source_frame(&source_index)
                    .unwrap()
                    .identity,
                SourceFrameId(ordinal)
            );
        }
        assert_eq!(promoted_plan.duration(), duration(2));
    }
}

#[test]
fn reject_policy_never_uses_available_context_beyond_the_selected_end() {
    let origin = -10010;
    let full = span(origin, origin + 12012);
    let plan = RenderPlan::compile(&document(
        full,
        &["source"],
        vec![(
            "source",
            source(full, natural_selection(EndpointPolicy::Reject)),
        )],
    ))
    .unwrap();
    let source_index = vfr_index(origin);
    let first = plan.picture(ProjectFrame(0)).unwrap().picture;
    assert_eq!(
        first.select_source_frame(&source_index).unwrap().identity,
        SourceFrameId(3)
    );
    let tail = plan.picture(ProjectFrame(1)).unwrap().picture;
    assert_eq!(
        source_fields(&tail).0.ticks,
        ExactRatio::integer(origin + 7007)
    );
    assert!(tail.select_source_frame(&source_index).is_err());
    // The same coordinate is valid in the full Original; rejection belongs
    // specifically to the authored selection, not missing source coverage.
    assert_eq!(
        source_index
            .select(source_fields(&tail).0, EndpointPolicy::Reject)
            .unwrap()
            .identity,
        SourceFrameId(5)
    );
}

#[test]
fn public_source_pictures_reject_incomplete_context_and_inconsistent_exact_windows() {
    let origin = -10010;
    let full = span(origin, origin + 12012);
    let source_index = vfr_index(origin);
    let plan = RenderPlan::compile(&document(
        full,
        &["source"],
        vec![(
            "source",
            source(full, natural_selection(EndpointPolicy::HoldAdjacent)),
        )],
    ))
    .unwrap();
    let valid = plan.picture(ProjectFrame(1)).unwrap().picture;
    let (point, _, selected) = source_fields(&valid);
    assert_eq!(
        valid.select_source_frame(&source_index).unwrap().identity,
        SourceFrameId(4)
    );

    // The visible window is covered, but the public retained context extends
    // beyond measured media on either side. Holding cannot admit either one.
    for context in [
        span(origin - 1, origin + 12012),
        span(origin, origin + 12013),
    ] {
        let mut forged = valid.clone();
        let Picture::Source { span, .. } = &mut forged else {
            unreachable!()
        };
        *span = context;
        assert_eq!(
            source_index
                .select_in_exact_span(point, selected, EndpointPolicy::HoldAdjacent)
                .unwrap()
                .identity,
            SourceFrameId(4)
        );
        assert!(forged.select_source_frame(&source_index).is_err());
    }

    let mut narrow = valid.clone();
    let Picture::Source { span: context, .. } = &mut narrow else {
        unreachable!()
    };
    *context = span(origin + 4004, origin + 7007);
    assert_eq!(
        narrow.select_source_frame(&source_index).unwrap().identity,
        SourceFrameId(4)
    );
    let outside_context = exact_span(
        selected.start().ticks,
        selected.end().ticks.checked_add(ratio(1, 2)).unwrap(),
    );
    let Picture::Source { selection, .. } = &mut narrow else {
        unreachable!()
    };
    *selection = outside_context;
    // This forged half-tick extension is still measured and would reveal the
    // next VFR frame if the retained context did not constrain the selection.
    assert_eq!(
        source_index
            .select_in_exact_span(point, outside_context, EndpointPolicy::HoldAdjacent)
            .unwrap()
            .identity,
        SourceFrameId(5)
    );
    assert!(narrow.select_source_frame(&source_index).is_err());

    let mut wrong_clock = valid;
    let finer_clock = SourceTimeBase::new(1, 120_000).unwrap();
    let remap = |point: SourcePoint| SourcePoint {
        ticks: point.ticks.checked_mul(ExactRatio::integer(2)).unwrap(),
        time_base: finer_clock,
    };
    let Picture::Source { selection, .. } = &mut wrong_clock else {
        unreachable!()
    };
    // Even equal physical times need the index's declared exact clock.
    *selection = ExactSourceSpan::new(remap(selected.start()), remap(selected.end())).unwrap();
    assert!(wrong_clock.select_source_frame(&source_index).is_err());
}

#[test]
fn exact_selected_ticks_hold_only_intersecting_vfr_intervals_on_both_sides() {
    let full = span(-10, 10);
    let selection_frames = ExactFrameRange {
        start: ratio(3, 4),
        end: ratio(5, 4),
    };
    let expected_selection = exact_span(ratio(-15, 8), ratio(15, 8));
    let source_index = index(&[-10, -8, -6, -2, 0, 3, 4, 9], 10);
    for endpoints in [EndpointPolicy::HoldAdjacent, EndpointPolicy::Reject] {
        let mapping = SourceVideoMapping::SelectedPlacement {
            start: ratio(-1, 3),
            frames: ratio(8, 3),
            selection: selection_frames,
            endpoints,
        };
        assert_eq!(
            mapping.selection_frames(duration(2)).unwrap(),
            selection_frames
        );
        assert_eq!(
            mapping.selection_in_source(full, duration(2)).unwrap(),
            expected_selection
        );
        let plan = RenderPlan::compile(&document(
            full,
            &["source"],
            vec![("source", source(full, mapping))],
        ))
        .unwrap();
        for (frame, ticks, expected_ordinal) in [(0, ratio(-15, 4), 3), (1, ratio(15, 4), 4)] {
            let picture = plan.picture(ProjectFrame(frame)).unwrap().picture;
            assert_eq!(source_fields(&picture).0.ticks, ticks);
            assert_eq!(source_fields(&picture).2, expected_selection);
            let selected = picture.select_source_frame(&source_index);
            match endpoints {
                EndpointPolicy::HoldAdjacent => {
                    assert_eq!(selected.unwrap().identity, SourceFrameId(expected_ordinal))
                }
                EndpointPolicy::Reject => assert!(selected.is_err()),
            }
        }
    }
}

#[test]
fn nested_retimes_and_repeat_plays_preserve_selection_without_rounding_source_points() {
    let origin = -10010;
    let full = span(origin, origin + 12012);
    let narrow = span(origin + 4004, origin + 7007);
    let source_index = vfr_index(origin);
    for endpoints in [EndpointPolicy::HoldAdjacent, EndpointPolicy::Reject] {
        for (context, mapping) in [
            (full, natural_selection(endpoints)),
            (
                narrow,
                SourceVideoMapping::natural_rate(narrow, rate(), endpoints).unwrap(),
            ),
        ] {
            let document = document(
                full,
                &["lead", "repeat"],
                vec![
                    (
                        "lead",
                        beat(NodeKind::Hold {
                            recipe: background(3),
                        }),
                    ),
                    (
                        "repeat",
                        beat(NodeKind::Repeat {
                            child: node_id("outer"),
                            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
                            gap: Some(background(1)),
                        }),
                    ),
                    ("outer", retime("inner", 8, 4)),
                    ("inner", retime("source", 4, 2)),
                    ("source", source(context, mapping)),
                ],
            );
            let plan = RenderPlan::compile(&document).unwrap();
            assert_eq!(plan.duration(), duration(20));
            let expected = [
                (17017, 2),
                (19019, 3),
                (21021, 3),
                (23023, 3),
                (25025, 4),
                (27027, 4),
                (29029, 4),
                (31031, 4),
            ];
            for (play, start) in [(0, 3), (1, 12)] {
                for (local, (quarter_ticks, ordinal)) in expected.iter().enumerate() {
                    let frame = start + i64::try_from(local).unwrap();
                    let sample = plan.picture(ProjectFrame(frame)).unwrap();
                    assert_eq!(sample.instance.repeats.len(), 1);
                    assert_eq!(sample.instance.repeats[0].iteration.ordinal, play);
                    assert_eq!(
                        source_fields(&sample.picture).0.ticks,
                        ExactRatio::integer(origin)
                            .checked_add(ratio(*quarter_ticks, 4))
                            .unwrap()
                    );
                    assert_eq!(
                        source_fields(&sample.picture).2,
                        exact_span(
                            ExactRatio::integer(origin + 4004),
                            ExactRatio::integer(origin + 7007)
                        )
                    );
                    let selected = sample.picture.select_source_frame(&source_index);
                    if endpoints == EndpointPolicy::Reject && local >= 6 {
                        assert!(selected.is_err());
                    } else {
                        assert_eq!(selected.unwrap().identity, SourceFrameId(*ordinal));
                    }
                }
            }
            assert!(matches!(
                plan.picture(ProjectFrame(11)).unwrap().picture,
                Picture::Background
            ));
        }
    }
}

fn source_target(
    timestamp: SourceTimestamp,
    bias: InsertionBias,
    repeats: Vec<RepeatInstance>,
) -> AnchorTarget {
    AnchorTarget {
        boundary: BoundaryAnchor {
            coordinate: Anchor::Source {
                asset: asset(),
                moment: SourceMoment::Timestamp {
                    stream: SourceStream::Video,
                    timestamp,
                },
            },
            bias,
        },
        occurrence: Some(InstancePath {
            node: node_id("source"),
            repeats,
        }),
    }
}

#[test]
fn inverse_source_anchors_admit_exact_fractional_endpoints_and_reject_hidden_context() {
    let full = span(-10, 10);
    let document = document(
        full,
        &["source"],
        vec![(
            "source",
            source(
                full,
                SourceVideoMapping::SelectedPlacement {
                    start: ratio(-1, 3),
                    frames: ratio(8, 3),
                    selection: ExactFrameRange {
                        start: ratio(3, 4),
                        end: ratio(5, 4),
                    },
                    endpoints: EndpointPolicy::HoldAdjacent,
                },
            ),
        )],
    );
    let anchors = AnchorIndex::new(&document).unwrap();
    // One eighth of an index tick is represented in its own exact source
    // timestamp clock. Both boundaries round to frame 1, but remain distinct.
    let fine_clock = SourceTimeBase::new(1, 480_000).unwrap();
    for (ticks, bias, exact) in [
        (-15, InsertionBias::Right, ratio(3, 4)),
        (15, InsertionBias::Left, ratio(5, 4)),
    ] {
        let resolved = anchors
            .resolve_target(&source_target(
                SourceTimestamp {
                    ticks,
                    time_base: fine_clock,
                },
                bias,
                vec![],
            ))
            .unwrap();
        assert_eq!(resolved.exact_frame, exact);
        assert_eq!(resolved.frame, ProjectFrame(1));
    }
    for ticks in [-16, 16, -80, 80] {
        let error = anchors
            .resolve_target(&source_target(
                SourceTimestamp {
                    ticks,
                    time_base: fine_clock,
                },
                InsertionBias::Right,
                vec![],
            ))
            .unwrap_err();
        assert_eq!(error.code, AnchorErrorCode::OutsideMapping);
    }
}

#[test]
fn inverse_source_window_composes_enclosing_retime_and_explicit_repeat_identity() {
    let origin = 13013;
    let full = span(origin, origin + 12012);
    let document = document(
        full,
        &["repeat"],
        vec![
            (
                "repeat",
                beat(NodeKind::Repeat {
                    child: node_id("retime"),
                    iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
                    gap: Some(background(1)),
                }),
            ),
            ("retime", retime("source", 8, 2)),
            (
                "source",
                source(full, natural_selection(EndpointPolicy::HoldAdjacent)),
            ),
        ],
    );
    let anchors = AnchorIndex::new(&document).unwrap();
    let occurrence = vec![RepeatInstance {
        node: node_id("repeat"),
        iteration: IterationId {
            allocation: revision("plays"),
            ordinal: 1,
        },
    }];
    // Second play starts at 9. The selected 3/2 source frames stretch to 6,
    // while the authored two-frame beat occupies 8 frames per play.
    for (ticks, bias, expected) in [
        (origin + 4004, InsertionBias::Right, 9),
        (origin + 7007, InsertionBias::Left, 15),
    ] {
        let resolved = anchors
            .resolve_target(&source_target(
                SourceTimestamp {
                    ticks,
                    time_base: clock(),
                },
                bias,
                occurrence.clone(),
            ))
            .unwrap();
        assert_eq!(resolved.exact_frame, ExactRatio::integer(expected));
        assert_eq!(resolved.frame, ProjectFrame(expected));
    }
    for ticks in [origin + 4003, origin + 7008] {
        assert_eq!(
            anchors
                .resolve_target(&source_target(
                    SourceTimestamp {
                        ticks,
                        time_base: clock()
                    },
                    InsertionBias::Right,
                    occurrence.clone()
                ))
                .unwrap_err()
                .code,
            AnchorErrorCode::OutsideMapping
        );
    }
}

#[path = "selected_video_window/edit_window.rs"]
mod edit_window;
#[path = "selected_video_window/slip.rs"]
mod slip;

#[path = "selected_video_window/trim.rs"]
mod trim;

#[path = "selected_video_window/roll.rs"]
mod roll;
