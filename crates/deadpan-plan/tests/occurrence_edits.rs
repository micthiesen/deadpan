use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{Picture, RenderPlan};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn play(allocation: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision(allocation),
        ordinal,
    }
}
fn step(node: &str, ordinal: u32) -> RepeatInstance {
    RepeatInstance {
        node: id(node),
        iteration: play(node, ordinal),
    }
}
fn gap(frames: i64) -> Option<HoldRecipe> {
    (frames > 0).then(|| HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    })
}
fn repeat(child: &str, plays: u32, gap_frames: i64, allocation: &str) -> BeatNode {
    BeatNode {
        label: allocation.into(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(revision(allocation), plays).unwrap(),
            gap: gap(gap_frames),
        },
    }
}
fn clock() -> SourceTimeBase {
    SourceTimeBase::new(1, 30_000).unwrap()
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
fn source(frames: i64, start: i64, end: i64) -> BeatNode {
    BeatNode {
        label: "Original speech".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video_mapping: SourceVideoMapping::FitBeat,
                video: SourceVideo::Stream {
                    asset: AssetId::new("video").unwrap(),
                    span: span(start, end),
                },
                audio: None,
                link: LinkRelation::Independent,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        },
    }
}
fn retime(child: &str, frames: i64, start: i64, end: i64) -> BeatNode {
    BeatNode {
        label: "Exact retime".into(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
    }
}
fn document(
    mut nodes: BTreeMap<NodeId, BeatNode>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    nodes.insert(id("root"), BeatNode::sequence("Root", vec![id("outer")]));
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("video").unwrap(),
        AssetRecord {
            label: "Original source".into(),
            content_hash: "a".repeat(64),
            video: Some(span(-1000, 10_000)),
            audio: None,
            still_image: false,
            frame_count: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn nested(plays: u32, frames: i64, outer_gap: i64) -> ProjectDocument {
    document(
        BTreeMap::from([
            (id("outer"), repeat("inner", plays, outer_gap, "outer")),
            (id("inner"), repeat("source", 3, 1, "inner")),
            (id("source"), source(frames, 100, 500)),
        ]),
        BTreeMap::new(),
    )
}
fn change(
    before: &ProjectDocument,
    name: &str,
    instance: InstancePath,
    edit: OccurrenceEdit,
    nodes: usize,
) -> (ProjectDocument, EditTransaction) {
    let transaction = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(name),
            command: Command::EditOccurrence {
                instance,
                edit,
                identities: OccurrenceIdentities {
                    nodes: (0..nodes)
                        .map(|index| id(&format!("{name}-{index}")))
                        .collect(),
                    marks: vec![],
                },
            },
        },
    )
    .unwrap();
    let after = transaction.forward.apply(before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *before);
    (after, transaction)
}
fn ticks(picture: &Picture) -> ExactRatio {
    let Picture::Source { asset, point, .. } = picture else {
        panic!("expected original source, got {picture:?}")
    };
    assert_eq!(*asset, AssetId::new("video").unwrap());
    assert_eq!(point.time_base, clock());
    point.ticks
}

#[test]
fn wrapping_one_nested_occurrence_preserves_every_other_original_source_sample() {
    let before = nested(2, 4, 2);
    let old_plan = RenderPlan::compile(&before).unwrap();
    let old_samples = (0..30)
        .map(|frame| old_plan.picture(ProjectFrame(frame)).unwrap())
        .collect::<Vec<_>>();
    let (after, transaction) = change(
        &before,
        "isolate",
        InstancePath {
            node: id("source"),
            repeats: vec![step("outer", 1), step("inner", 1)],
        },
        OccurrenceEdit::WrapRepeat {
            id: id("gag"),
            plays: 2,
            gap: gap(1),
            anchor_policy: WrapAnchorPolicy::First,
        },
        3,
    );
    let plan = RenderPlan::compile(&after).unwrap();
    assert_eq!(plan.duration().frames(), 35);
    assert_eq!(transaction.duration_delta, 5);
    assert_eq!(after.nodes().len(), 8);
    assert_eq!(after.overrides().len(), 2);
    assert_eq!(after.nodes()[&id("source")], before.nodes()[&id("source")]);
    assert_eq!(after.nodes()[&id("inner")], before.nodes()[&id("inner")]);

    // Independently expand only this small fixture. Coordinates still refer to
    // the same original source on both copies of the selected play.
    let mut frame = 0;
    for outer in 0..2 {
        for inner in 0..3 {
            let copies = if outer == 1 && inner == 1 { 2 } else { 1 };
            for copy in 0..copies {
                for local in 0..4 {
                    let sample = plan.picture(ProjectFrame(frame)).unwrap();
                    sample.instance.validate(&after).unwrap();
                    assert_eq!(
                        ticks(&sample.picture),
                        ExactRatio::integer(150 + local * 100)
                    );
                    assert_eq!(sample.instance.repeats[0], step("outer", outer));
                    assert_eq!(sample.instance.repeats[1].iteration, play("inner", inner));
                    assert_eq!(
                        sample.instance.repeats.len(),
                        if copies == 2 { 3 } else { 2 }
                    );
                    if copies == 2 {
                        assert_eq!(sample.instance.repeats[2].node, id("gag"));
                        assert_eq!(sample.instance.repeats[2].iteration, play("isolate", copy));
                    } else {
                        let old_frame = i64::from(outer) * 16 + i64::from(inner) * 5 + local;
                        let old = &old_samples[usize::try_from(old_frame).unwrap()];
                        assert_eq!(sample.picture, old.picture);
                        assert_eq!(sample.local_position, old.local_position);
                        if outer == 0 {
                            assert_eq!(sample.instance, old.instance);
                        }
                    }
                    frame += 1;
                }
                if copy + 1 < copies {
                    let sample = plan.picture(ProjectFrame(frame)).unwrap();
                    assert_eq!(sample.picture, Picture::Background);
                    assert_eq!(sample.instance.node, id("gag"));
                    assert_eq!(sample.gap_after, Some(play("isolate", copy)));
                    sample.instance.validate(&after).unwrap();
                    frame += 1;
                }
            }
            if inner < 2 {
                let sample = plan.picture(ProjectFrame(frame)).unwrap();
                assert_eq!(sample.picture, Picture::Background);
                assert_eq!(sample.gap_after, Some(play("inner", inner)));
                assert_eq!(sample.instance.repeats.len(), 1);
                sample.instance.validate(&after).unwrap();
                frame += 1;
            }
        }
        if outer == 0 {
            for _ in 0..2 {
                let sample = plan.picture(ProjectFrame(frame)).unwrap();
                assert_eq!(sample.picture, Picture::Background);
                assert_eq!(sample.instance.node, id("outer"));
                assert_eq!(sample.gap_after, Some(play("outer", 0)));
                frame += 1;
            }
        }
    }
    assert_eq!(frame, 35);
    let restored = transaction.inverse.apply(&after).unwrap();
    let restored_plan = RenderPlan::compile(&restored).unwrap();
    assert_eq!(old_plan.inspect(), restored_plan.inspect());
    for (frame, expected) in old_samples.iter().enumerate() {
        let frame = ProjectFrame(i64::try_from(frame).unwrap());
        assert_eq!(&old_plan.picture(frame).unwrap(), expected);
        assert_eq!(&restored_plan.picture(frame).unwrap(), expected);
    }
}

#[test]
fn isolation_clones_nested_overrides_and_reuses_them_with_exact_retime_and_gap_mapping() {
    let before = document(
        BTreeMap::from([
            (id("outer"), repeat("outer-retime", 2, 2, "outer")),
            (id("outer-retime"), retime("inner", 7, 2, 15)),
            (id("inner"), repeat("source", 3, 1, "inner")),
            (id("source"), source(4, -100, 700)),
            (id("alternate-retime"), retime("alternate", 6, 1, 8)),
            (id("alternate"), source(9, 1000, 2800)),
        ]),
        BTreeMap::from([(
            id("inner"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: play("inner", 1),
                root: id("alternate-retime"),
            }])
            .unwrap(),
        )]),
    );
    let old_plan = RenderPlan::compile(&before).unwrap();
    let (after, _) = change(
        &before,
        "isolate",
        InstancePath {
            node: id("alternate-retime"),
            repeats: vec![step("outer", 1), step("inner", 1)],
        },
        OccurrenceEdit::WrapRepeat {
            id: id("gag"),
            plays: 2,
            gap: gap(2),
            anchor_policy: WrapAnchorPolicy::First,
        },
        5,
    );
    let plan = RenderPlan::compile(&after).unwrap();
    assert_eq!(plan.duration().frames(), 16);
    assert_eq!(after.nodes().len(), 13);
    assert_eq!(after.overrides().len(), 3);
    // Original inner Repeat and its custom play still belong to the first outer play.
    assert_eq!(
        after.overrides()[&id("inner")],
        before.overrides()[&id("inner")]
    );
    for frame in 0..9 {
        let old = old_plan.picture(ProjectFrame(frame)).unwrap();
        let current = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(current.picture, old.picture);
        assert_eq!(current.instance, old.instance);
        assert_eq!(current.local_position, old.local_position);
    }
    // Derived from 2 + (frame + 1/2) * 13/7, followed by the selected
    // six-frame Retime's 1 + local * 7/6 mapping into original 200-tick frames.
    let expected = [
        Some((3400, 7)),
        None,
        Some((4750, 3)),
        Some((6050, 3)),
        Some((2450, 1)),
        None,
        Some((1450, 1)),
    ];
    for (local, expected) in expected.into_iter().enumerate() {
        let sample = plan
            .picture(ProjectFrame(9 + i64::try_from(local).unwrap()))
            .unwrap();
        sample.instance.validate(&after).unwrap();
        assert_eq!(sample.instance.repeats[0], step("outer", 1));
        if let Some((numerator, denominator)) = expected {
            assert_eq!(
                ticks(&sample.picture),
                ExactRatio::new(numerator, denominator).unwrap()
            );
        } else {
            assert_eq!(sample.picture, Picture::Background);
            if local == 1 {
                assert_eq!(sample.gap_after, Some(play("inner", 0)));
                assert_eq!(sample.local_position, ExactRatio::new(11, 14).unwrap());
            } else {
                assert_eq!(sample.instance.node, id("gag"));
                assert_eq!(sample.gap_after, Some(play("isolate", 0)));
                assert_eq!(sample.local_position, ExactRatio::new(17, 14).unwrap());
            }
        }
    }
    let mut existing = plan.picture(ProjectFrame(14)).unwrap().instance;
    assert_eq!(existing.node, id("gag"));
    existing.validate(&after).unwrap();
    let (renamed, _) = change(
        &after,
        "reuse",
        existing.clone(),
        OccurrenceEdit::Rename {
            label: "Selected gag".into(),
        },
        0,
    );
    assert_eq!(renamed.nodes().len(), after.nodes().len());
    assert_eq!(renamed.overrides(), after.overrides());
    assert_eq!(renamed.nodes()[&id("gag")].label, "Selected gag");
    let renamed_plan = RenderPlan::compile(&renamed).unwrap();
    for frame in 0..16 {
        assert_eq!(
            renamed_plan.picture(ProjectFrame(frame)).unwrap().picture,
            plan.picture(ProjectFrame(frame)).unwrap().picture
        );
    }
    existing.node = id("alternate-retime");
    assert!(
        existing.validate(&renamed).is_err(),
        "original subtree is not an address into the cloned outer play"
    );
}

#[test]
fn maximum_play_count_isolates_one_nested_play_without_expanding_other_occurrences() {
    let before = nested(u32::MAX, 2, 0);
    let selected = u32::MAX / 2;
    let (after, _) = change(
        &before,
        "isolate",
        InstancePath {
            node: id("source"),
            repeats: vec![step("outer", selected), step("inner", 1)],
        },
        OccurrenceEdit::WrapRepeat {
            id: id("gag"),
            plays: 2,
            gap: gap(1),
            anchor_policy: WrapAnchorPolicy::First,
        },
        3,
    );
    let plan = RenderPlan::compile(&after).unwrap();
    let storage = plan.metadata().storage;
    assert_eq!(storage.authored_nodes, 8);
    assert_eq!(storage.iteration_run_entries, 4);
    assert_eq!(storage.repeat_segment_entries, 8);
    assert_eq!(storage.sparse_override_entries, 2);
    assert_eq!(plan.duration().frames(), i64::from(u32::MAX) * 8 + 3);
    let start = i64::from(selected) * 8;
    for (frame, outer, inner, expected) in [
        (0, 0, 0, 200),
        (start - 1, selected - 1, 2, 400),
        (start + 3, selected, 1, 200),
        (start + 7, selected, 1, 400),
        (start + 9, selected, 2, 200),
        (start + 11, selected + 1, 0, 200),
        (plan.duration().frames() - 1, u32::MAX - 1, 2, 400),
    ] {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(sample.instance.repeats[0], step("outer", outer));
        assert_eq!(sample.instance.repeats[1].iteration, play("inner", inner));
        assert_eq!(ticks(&sample.picture), ExactRatio::integer(expected));
        assert!(sample.lookup.visited_nodes <= 5);
        assert!(sample.lookup.iteration_run_comparisons <= 10);
        sample.instance.validate(&after).unwrap();
    }
    let gap = plan.picture(ProjectFrame(start + 5)).unwrap();
    assert_eq!(gap.picture, Picture::Background);
    assert_eq!(gap.instance.node, id("gag"));
    assert_eq!(gap.gap_after, Some(play("isolate", 0)));
}

#[test]
fn fractional_placement_isolates_nested_play_and_composes_nested_retimes() {
    let base = nested(2, 4, 2);
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["rate-outer"]);
    wire["nodes"]["rate-inner"] = serde_json::to_value(retime("outer", 60, 0, 30)).unwrap();
    wire["nodes"]["rate-outer"] = serde_json::to_value(retime("rate-inner", 45, 0, 60)).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mapping = SourceVideoMapping::Placement {
        start: ExactRatio::new(3, 4).unwrap(),
        frames: ExactRatio::new(7, 2).unwrap(),
        endpoints: EndpointPolicy::HoldAdjacent,
    };
    let (after, transaction) = change(
        &before,
        "placed",
        InstancePath {
            node: id("source"),
            repeats: vec![step("outer", 1), step("inner", 1)],
        },
        OccurrenceEdit::SetSourceVideoMapping { mapping },
        3,
    );
    assert_eq!(transaction.duration_delta, 0);
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    let before_plan = RenderPlan::compile(&before).unwrap();
    let after_plan = RenderPlan::compile(&after).unwrap();
    let mut selected_path = None;
    for frame in 0..45 {
        let old = before_plan.picture(ProjectFrame(frame)).unwrap();
        let new = after_plan.picture(ProjectFrame(frame)).unwrap();
        if old.instance.repeats == vec![step("outer", 1), step("inner", 1)] {
            let expected = ExactRatio::integer(100)
                .checked_add(
                    old.local_position
                        .checked_sub(ExactRatio::new(3, 4).unwrap())
                        .unwrap()
                        .checked_mul(ExactRatio::new(800, 7).unwrap())
                        .unwrap(),
                )
                .unwrap();
            assert_eq!(ticks(&new.picture), expected);
            assert_eq!(new.local_position, old.local_position);
            selected_path = Some(new.instance);
        } else {
            assert_eq!(new.picture, old.picture);
        }
    }
    let target = AnchorTarget {
        boundary: BoundaryAnchor {
            coordinate: Anchor::Source {
                asset: AssetId::new("video").unwrap(),
                moment: SourceMoment::Timestamp {
                    stream: SourceStream::Video,
                    timestamp: SourceTimestamp {
                        ticks: 300,
                        time_base: clock(),
                    },
                },
            },
            bias: InsertionBias::Right,
        },
        occurrence: selected_path,
    };
    assert_eq!(
        AnchorIndex::new(&after)
            .unwrap()
            .resolve_target(&target)
            .unwrap()
            .exact_frame,
        ExactRatio::new(141, 4).unwrap()
    );
    assert_eq!(
        RenderPlan::compile(&transaction.inverse.apply(&after).unwrap())
            .unwrap()
            .inspect(),
        before_plan.inspect()
    );
}
