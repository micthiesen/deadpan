use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{Picture, PlanError, RenderPlan};
use proptest::prelude::*;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn rev(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn iteration(allocation: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: rev(allocation),
        ordinal,
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
        label: "Source".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Stream {
                    asset: AssetId::new("video").unwrap(),
                    span: span(start, end),
                },
                audio: None,
                link: LinkRelation::Independent,
                audio_offset: AudioSample(0),
            },
        },
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
        label: "Repeat".into(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(rev(allocation), plays).unwrap(),
            gap: gap(gap_frames),
        },
    }
}
fn retime(child: &str, frames: i64, start: i64, end: i64) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
    }
}
fn document(
    root_child: &str,
    nodes: BTreeMap<NodeId, BeatNode>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        rev("initial"),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    let mut nodes = nodes;
    nodes.insert(id("root"), BeatNode::sequence("Root", vec![id(root_child)]));
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("video").unwrap(),
        AssetRecord {
            label: "Original video".into(),
            content_hash: "a".repeat(64),
            video: Some(span(-10_000, 100_000)),
            audio: None,
            still_image: false,
            frame_count: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn repeated(plays: u32, frames: i64, gap_frames: i64) -> ProjectDocument {
    document(
        "repeat",
        BTreeMap::from([
            (id("repeat"), repeat("base", plays, gap_frames, "original")),
            (id("base"), source(frames, 100, 500)),
        ]),
        BTreeMap::new(),
    )
}
fn transaction(document: &ProjectDocument, name: &str, command: Command) -> EditTransaction {
    apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: rev(name),
            command,
        },
    )
    .unwrap()
}
fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    transaction(document, name, command)
        .forward
        .apply(document)
        .unwrap()
}
fn override_command(ordinal: u32, frames: i64, start: i64, end: i64) -> Command {
    Command::SetPlayOverride {
        node: id("repeat"),
        iteration: iteration("original", ordinal),
        subtree: Subtree {
            root: id("alternate"),
            nodes: BTreeMap::from([(id("alternate"), source(frames, start, end))]),
            overrides: BTreeMap::new(),
        },
    }
}
fn ticks(picture: &Picture) -> ExactRatio {
    match picture {
        Picture::Source { point, .. } => {
            assert_eq!(point.time_base, clock());
            point.ticks
        }
        other => panic!("expected original source coordinates, got {other:?}"),
    }
}

#[test]
fn second_play_has_its_own_duration_and_picture_without_changing_other_source_ranges() {
    let before = repeated(3, 4, 1);
    let old_plan = RenderPlan::compile(&before).unwrap();
    let change = transaction(&before, "override", override_command(1, 2, 2000, 2400));
    let after = change.forward.apply(&before).unwrap();
    let plan = RenderPlan::compile(&after).unwrap();
    assert_eq!(old_plan.duration().frames(), 14);
    assert_eq!(plan.duration().frames(), 12);
    let mut gaps = Vec::new();
    for frame in 0..12 {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        sample.instance.validate(&after).unwrap();
        if [4, 7].contains(&frame) {
            let ordinal = u32::from(frame == 7);
            assert_eq!(sample.picture, Picture::Background);
            assert_eq!(sample.instance.node, id("repeat"));
            assert!(sample.instance.repeats.is_empty());
            assert_eq!(sample.gap_after, Some(iteration("original", ordinal)));
            gaps.push(frame);
        } else {
            assert!(sample.gap_after.is_none());
            let (ordinal, local, expected) = match frame {
                0..=3 => (0, frame, 150 + 100 * frame),
                5..=6 => (1, frame - 5, 2100 + 200 * (frame - 5)),
                8..=11 => (2, frame - 8, 150 + 100 * (frame - 8)),
                _ => unreachable!(),
            };
            assert_eq!(
                sample.instance.repeats,
                vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: iteration("original", ordinal),
                }]
            );
            assert_eq!(
                sample.local_position,
                ExactRatio::new(i128::from(2 * local + 1), 2).unwrap()
            );
            assert_eq!(ticks(&sample.picture), ExactRatio::integer(expected));
            if ordinal != 1 {
                let old = old_plan
                    .picture(ProjectFrame(if ordinal == 0 { frame } else { frame + 2 }))
                    .unwrap();
                assert_eq!(sample.instance, old.instance);
                assert_eq!(sample.picture, old.picture);
            }
        }
    }
    assert_eq!(gaps, [4, 7]);
    assert!(matches!(
        plan.picture(ProjectFrame(12)),
        Err(PlanError::FrameOutOfRange { .. })
    ));
    // Compiled plans retain one committed revision even after the document changes.
    assert_eq!(
        old_plan.picture(ProjectFrame(5)).unwrap().instance.node,
        id("base")
    );
    assert_eq!(old_plan.metadata().revision_id, *before.revision_id());
    assert_eq!(plan.metadata().revision_id, *after.revision_id());
    let restored = change.inverse.apply(&after).unwrap();
    let restored_plan = RenderPlan::compile(&restored).unwrap();
    assert_eq!(restored, before);
    assert_eq!(restored_plan.inspect(), old_plan.inspect());
    for frame in 0..14 {
        assert_eq!(
            restored_plan.picture(ProjectFrame(frame)).unwrap(),
            old_plan.picture(ProjectFrame(frame)).unwrap()
        );
    }
}

#[test]
fn nested_repeats_and_retimes_map_variable_play_centers_without_intermediate_rounding() {
    let inner_override = PlayOverrides::try_from(vec![PlayOverride {
        iteration: iteration("inner-allocation", 1),
        root: id("alternate-retime"),
    }])
    .unwrap();
    let doc = document(
        "outer",
        BTreeMap::from([
            (
                id("outer"),
                repeat("outer-retime", 2, 2, "outer-allocation"),
            ),
            (id("outer-retime"), retime("inner", 7, 2, 15)),
            (id("inner"), repeat("base", 3, 1, "inner-allocation")),
            (id("base"), source(4, -100, 700)),
            (id("alternate-retime"), retime("alternate", 6, 1, 8)),
            (id("alternate"), source(9, 1000, 2800)),
        ]),
        BTreeMap::from([(id("inner"), inner_override)]),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    assert_eq!(plan.duration().frames(), 16);
    assert_eq!(plan.node_duration(&id("inner")), Some(duration(16)));
    // Compose frame centers through both retimes before selecting source ticks.
    let expected = [
        Some((3400, 7)),
        None,
        Some((4750, 3)),
        Some((6050, 3)),
        Some((2450, 1)),
        Some((-400, 7)),
        Some((2200, 7)),
    ];
    for frame in 0..16 {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        sample.instance.validate(&doc).unwrap();
        if [7, 8].contains(&frame) {
            assert_eq!(sample.picture, Picture::Background);
            assert_eq!(sample.instance.node, id("outer"));
            assert!(sample.instance.repeats.is_empty());
            assert_eq!(sample.gap_after, Some(iteration("outer-allocation", 0)));
            continue;
        }
        let outer_ordinal = u32::from(frame >= 9);
        let local = usize::try_from(if frame >= 9 { frame - 9 } else { frame }).unwrap();
        assert_eq!(
            sample.instance.repeats[0],
            RepeatInstance {
                node: id("outer"),
                iteration: iteration("outer-allocation", outer_ordinal),
            }
        );
        if let Some((numerator, denominator)) = expected[local] {
            assert_eq!(
                ticks(&sample.picture),
                ExactRatio::new(numerator, denominator).unwrap(),
                "frame {frame}"
            );
            let inner_ordinal = if local == 0 {
                0
            } else if local < 5 {
                1
            } else {
                2
            };
            assert_eq!(
                sample.instance.repeats[1],
                RepeatInstance {
                    node: id("inner"),
                    iteration: iteration("inner-allocation", inner_ordinal),
                }
            );
            assert_eq!(
                sample.instance.node,
                id(if inner_ordinal == 1 {
                    "alternate"
                } else {
                    "base"
                })
            );
            assert!(sample.gap_after.is_none());
        } else {
            assert_eq!(sample.picture, Picture::Background);
            assert_eq!(sample.instance.node, id("inner"));
            assert_eq!(sample.instance.repeats.len(), 1);
            assert_eq!(sample.gap_after, Some(iteration("inner-allocation", 0)));
            assert_eq!(sample.local_position, ExactRatio::new(11, 14).unwrap());
        }
    }
}

fn assert_order(doc: &ProjectDocument, expected: &[(IterationId, bool)]) {
    let plan = RenderPlan::compile(doc).unwrap();
    let mut frame = 0;
    for (index, (identity, alternate)) in expected.iter().enumerate() {
        let frames = if *alternate { 4 } else { 2 };
        for local in 0..frames {
            let sample = plan.picture(ProjectFrame(frame + local)).unwrap();
            assert_eq!(
                sample.instance.node,
                id(if *alternate { "alternate" } else { "base" })
            );
            assert_eq!(sample.instance.repeats[0].iteration, *identity);
            assert_eq!(
                ticks(&sample.picture),
                ExactRatio::integer(if *alternate {
                    1050 + local * 100
                } else {
                    200 + local * 200
                })
            );
            sample.instance.validate(doc).unwrap();
        }
        frame += frames;
        if index + 1 < expected.len() {
            let sample = plan.picture(ProjectFrame(frame)).unwrap();
            assert_eq!(sample.gap_after.as_ref(), Some(identity));
            assert_eq!(sample.picture, Picture::Background);
            frame += 1;
        }
    }
    assert_eq!(plan.duration().frames(), frame);
}

#[test]
fn inserted_reordered_and_retired_plays_select_by_surviving_identity() {
    let overridden = edit(
        &repeated(3, 2, 1),
        "override",
        override_command(1, 4, 1000, 1400),
    );
    let customized = iteration("original", 1);
    let moved = edit(
        &overridden,
        "move",
        Command::MovePlays {
            node: id("repeat"),
            start: 1,
            end: 2,
            destination: 0,
        },
    );
    assert_order(
        &moved,
        &[
            (customized.clone(), true),
            (iteration("original", 0), false),
            (iteration("original", 2), false),
        ],
    );
    let inserted = edit(
        &moved,
        "insert",
        Command::InsertPlays {
            node: id("repeat"),
            index: 1,
            count: 2,
        },
    );
    assert_order(
        &inserted,
        &[
            (customized.clone(), true),
            (iteration("insert", 0), false),
            (iteration("insert", 1), false),
            (iteration("original", 0), false),
            (iteration("original", 2), false),
        ],
    );
    let shrunk = edit(
        &inserted,
        "shrink",
        Command::SetRepeat {
            node: id("repeat"),
            plays: 3,
            gap: gap(1),
        },
    );
    assert_order(
        &shrunk,
        &[
            (customized.clone(), true),
            (iteration("insert", 0), false),
            (iteration("insert", 1), false),
        ],
    );
    let moved_last = edit(
        &shrunk,
        "move-last",
        Command::MovePlays {
            node: id("repeat"),
            start: 0,
            end: 1,
            destination: 2,
        },
    );
    assert_order(
        &moved_last,
        &[
            (iteration("insert", 0), false),
            (iteration("insert", 1), false),
            (customized, true),
        ],
    );
    let retired = edit(
        &moved_last,
        "retire",
        Command::SetRepeat {
            node: id("repeat"),
            plays: 2,
            gap: gap(1),
        },
    );
    assert_order(
        &retired,
        &[
            (iteration("insert", 0), false),
            (iteration("insert", 1), false),
        ],
    );
    assert!(!retired.nodes().contains_key(&id("alternate")));
    let grown = edit(
        &retired,
        "grow",
        Command::SetRepeat {
            node: id("repeat"),
            plays: 3,
            gap: gap(1),
        },
    );
    assert_order(
        &grown,
        &[
            (iteration("insert", 0), false),
            (iteration("insert", 1), false),
            (iteration("grow", 0), false),
        ],
    );
}

#[test]
fn billion_play_override_keeps_storage_and_seeks_bounded() {
    let doc = edit(
        &repeated(1_000_000_000, 1, 1),
        "override",
        override_command(500_000_000, 3, 1000, 1300),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let storage = plan.metadata().storage;
    assert_eq!(storage.authored_nodes, 4);
    assert_eq!(storage.iteration_run_entries, 1);
    assert_eq!(storage.repeat_segment_entries, 3);
    assert_eq!(storage.sparse_override_entries, 1);
    assert_eq!(storage.referenced_plays, 1_000_000_000);
    assert_eq!(plan.duration().frames(), 2_000_000_001);
    for (frame, ordinal, expected_ticks) in [
        (0, 0, 300),
        (1_000_000_000, 500_000_000, 1050),
        (1_000_000_002, 500_000_000, 1250),
        (1_000_000_004, 500_000_001, 300),
        (2_000_000_000, 999_999_999, 300),
    ] {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(
            sample.instance.repeats[0].iteration,
            iteration("original", ordinal)
        );
        assert_eq!(ticks(&sample.picture), ExactRatio::integer(expected_ticks));
        assert!(sample.lookup.iteration_run_comparisons <= 4);
        assert_eq!(sample.lookup.visited_nodes, 3);
        sample.instance.validate(&doc).unwrap();
    }
    assert_eq!(
        plan.picture(ProjectFrame(1_000_000_003)).unwrap().gap_after,
        Some(iteration("original", 500_000_000))
    );
}

proptest! {
    #[test]
    fn sparse_source_overrides_match_independently_expanded_small_references(
        plays in 1_u32..20, base_duration in 1_i64..8, gap_duration in 0_i64..4,
        changes in prop::collection::vec((0_u32..20, 1_i64..10), 0..12),
    ) {
        let selected: BTreeMap<_, _> = changes.into_iter().map(|(index, frames)| (index % plays, frames)).collect();
        let mut nodes = BTreeMap::from([
            (id("repeat"), repeat("base", plays, gap_duration, "original")),
            (id("base"), source(base_duration, 100, 500)),
        ]);
        let entries = selected.iter().map(|(ordinal, frames)| {
            let name = format!("alternate-{ordinal}");
            nodes.insert(id(&name), source(*frames, 1000 + i64::from(*ordinal) * 100, 1037 + i64::from(*ordinal) * 100));
            PlayOverride { iteration: iteration("original", *ordinal), root: id(&name) }
        }).collect::<Vec<_>>();
        let overrides = if entries.is_empty() { BTreeMap::new() } else { BTreeMap::from([(id("repeat"), PlayOverrides::try_from(entries).unwrap())]) };
        let doc = document("repeat", nodes, overrides);
        let plan = RenderPlan::compile(&doc).unwrap();
        let mut frame = 0;
        for ordinal in 0..plays {
            let (name, frames, start, span) = if let Some(frames) = selected.get(&ordinal) {
                (format!("alternate-{ordinal}"), *frames, 1000 + i64::from(ordinal) * 100, 37)
            } else { ("base".into(), base_duration, 100, 400) };
            for local in 0..frames {
                let sample = plan.picture(ProjectFrame(frame)).unwrap();
                prop_assert_eq!(&sample.instance.node, &id(&name));
                prop_assert_eq!(&sample.instance.repeats[0].iteration, &iteration("original", ordinal));
                let expected = ExactRatio::new(i128::from(start * 2 * frames + (2 * local + 1) * span), i128::from(2 * frames)).unwrap();
                prop_assert_eq!(ticks(&sample.picture), expected);
                prop_assert_eq!(&sample.gap_after, &None);
                sample.instance.validate(&doc).unwrap();
                frame += 1;
            }
            if ordinal + 1 < plays {
                for _ in 0..gap_duration {
                    let sample = plan.picture(ProjectFrame(frame)).unwrap();
                    prop_assert_eq!(sample.picture, Picture::Background);
                    prop_assert_eq!(sample.gap_after, Some(iteration("original", ordinal)));
                    prop_assert_eq!(sample.instance.node, id("repeat"));
                    frame += 1;
                }
            }
        }
        prop_assert_eq!(plan.duration().frames(), frame);
    }
}
