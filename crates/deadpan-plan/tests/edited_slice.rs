//! Synthetic picture-plan witnesses. These do not qualify decoded or GPU output.

use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{Picture, PictureSample, RenderPlan};

#[path = "edited_slice/delete_range.rs"]
mod delete_range;
#[path = "edited_slice/move_range.rs"]
mod move_range;
#[path = "edited_slice/placement.rs"]
mod placement;
#[path = "edited_slice/structural_capture.rs"]
mod structural_capture;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

// Independent scalar oracle for this fixture's integer linear endpoints.
// The product contract rounds progress to Q32 before interpolating the pose.
fn linear_scale_at(local: ExactRatio, frames: i64, end: i64) -> ExactRatio {
    let q = 1_i128 << 32;
    let numerator = local.numerator() * q;
    let denominator = local.denominator() * i128::from(frames);
    let whole = numerator / denominator;
    let remainder = numerator % denominator;
    let round_up = 2 * remainder > denominator || (2 * remainder == denominator && whole % 2 != 0);
    let progress = whole + i128::from(round_up);
    ratio(q + i128::from(end - 1) * progress, q)
}

fn clock() -> SourceTimeBase {
    SourceTimeBase::new(1, 30_000).unwrap()
}

fn timestamp(ticks: i64) -> SourceTimestamp {
    SourceTimestamp {
        ticks,
        time_base: clock(),
    }
}

fn asset() -> AssetId {
    AssetId::new("original").unwrap()
}

fn source(frames: i64, start: i64, end: i64) -> BeatNode {
    BeatNode {
        label: "Source".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Stream {
                    asset: asset(),
                    span: SourceSpan::new(timestamp(start), timestamp(end)).unwrap(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: None,
                link: LinkRelation::Independent,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        },
    }
}

fn captured_geometry() -> CapturedFraming {
    CapturedFraming {
        canvases: vec![
            CapturedCanvas {
                width: 960,
                height: 540,
                fit: CapturedFit::Fit,
                layers: vec![
                    Some(FramingPose::new(ratio(1, 3), ratio(2, 3), ratio(3, 2)).unwrap()),
                    None,
                ],
            },
            CapturedCanvas {
                width: 640,
                height: 480,
                fit: CapturedFit::Fit,
                layers: vec![Some(FramingPose::identity())],
            },
        ],
    }
}

fn freeze_recipe(frames: i64, ticks: i64) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Freeze {
            asset: asset(),
            timestamp: timestamp(ticks),
        },
        picture_context: Some(captured_geometry()),
        audio: HoldAudio::Silence,
    }
}

fn freeze(frames: i64, ticks: i64) -> BeatNode {
    BeatNode::hold("Freeze", freeze_recipe(frames, ticks))
}

fn creep(mut node: BeatNode, end_scale: i64) -> BeatNode {
    node.framing = Some(
        Framing::creep(
            FramingPose::identity(),
            FramingPose::new(ratio(1, 2), ratio(1, 2), ExactRatio::integer(end_scale)).unwrap(),
            FramingCurve::Linear,
        )
        .unwrap(),
    );
    node
}

fn document(roots: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("edited-slice-picture").unwrap(),
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
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, mut node)| {
            node.label = name.into();
            (id(name), node)
        })
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("root", roots.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset(),
        AssetRecord {
            source_qualification: None,
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(SourceSpan::new(timestamp(-10_000), timestamp(100_000)).unwrap()),
            audio: None,
            still_image: false,
            frame_count: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap()
    .forward
    .apply(document)
    .unwrap()
}

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}

fn capture(document: &ProjectDocument, parent: &str, start: i64, end: i64) -> CapturedEditSlice {
    let slice =
        CapturedEditSlice::capture(document, &id(parent), range(start, end), timing("capture"))
            .unwrap();
    // A stored register has no live reference to the source document.
    CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap()
}

fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    index: usize,
    name: &str,
) -> ProjectDocument {
    let requirements = slice.identity_requirements().unwrap();
    let identities = SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..requirements.nodes)
                .map(|n| id(&format!("{name}-node-{n}")))
                .collect(),
            marks: (0..requirements.marks)
                .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                .collect(),
        },
        aliases: (0..requirements.aliases)
            .map(|n| id(&format!("{name}-alias-{n}")))
            .collect(),
    };
    edit(
        document,
        name,
        Command::SpliceSlice {
            parent: id("root"),
            index,
            slice: slice.clone(),
            identities,
            timing: timing(name),
        },
    )
}

fn picture_ticks(sample: &PictureSample) -> ExactRatio {
    match &sample.picture {
        Picture::Source {
            asset: actual,
            point,
            ..
        }
        | Picture::Freeze {
            asset: actual,
            point,
        } => {
            assert_eq!(actual, &asset());
            assert_eq!(point.time_base, clock());
            point.ticks
        }
        other => panic!("unexpected picture: {other:?}"),
    }
}

// Compare the complete retained composition in provider-to-root order, including
// neutral intermediate clips. Labels select fixture owners without depending on
// the implementation's fresh-ID allocation order.
fn assert_owned_layers(
    old: &PictureSample,
    before: &ProjectDocument,
    new: &PictureSample,
    after: &ProjectDocument,
    excluded: &[&str],
) {
    let expected: Vec<_> = old
        .framing
        .iter()
        .filter(|layer| !excluded.contains(&before.nodes()[&layer.instance.node].label.as_str()))
        .collect();
    let names: Vec<_> = expected
        .iter()
        .map(|layer| before.nodes()[&layer.instance.node].label.as_str())
        .collect();
    let actual: Vec<_> = new
        .framing
        .iter()
        .filter(|layer| names.contains(&after.nodes()[&layer.instance.node].label.as_str()))
        .collect();
    assert_eq!(actual.len(), expected.len());
    for (old, new) in expected.into_iter().zip(actual) {
        assert_eq!(
            before.nodes()[&old.instance.node].label,
            after.nodes()[&new.instance.node].label
        );
        assert_eq!(
            (new.local_position, new.duration, new.pose),
            (old.local_position, old.duration, old.pose)
        );
        assert_eq!(
            new.instance
                .repeats
                .iter()
                .map(|step| step.iteration.ordinal)
                .collect::<Vec<_>>(),
            old.instance
                .repeats
                .iter()
                .map(|step| step.iteration.ordinal)
                .collect::<Vec<_>>()
        );
        assert_ne!(new.instance.node, old.instance.node);
    }
    for layer in &new.framing {
        layer.instance.validate(after).unwrap();
        if !names.contains(&after.nodes()[&layer.instance.node].label.as_str()) {
            assert!(
                layer.pose.is_none(),
                "unselected ancestor framing leaked into the copy"
            );
        }
    }
    assert_eq!(new.picture, old.picture);
    assert_eq!(new.picture_context, old.picture_context);
}

fn assert_destination_unchanged(
    before: &ProjectDocument,
    after: &ProjectDocument,
    insertion: i64,
    inserted: i64,
) {
    let old = RenderPlan::compile(before).unwrap();
    let new = RenderPlan::compile(after).unwrap();
    assert_eq!(new.duration().frames(), old.duration().frames() + inserted);
    for frame in (0..old.duration().frames()).rev() {
        let expected = old.picture(ProjectFrame(frame)).unwrap();
        let actual = new
            .picture(ProjectFrame(
                frame + i64::from(frame >= insertion) * inserted,
            ))
            .unwrap();
        assert_eq!(actual.picture, expected.picture);
        assert_eq!(actual.picture_context, expected.picture_context);
        assert_eq!(actual.instance, expected.instance);
        assert_eq!(actual.gap_after, expected.gap_after);
        // The root's duration and absolute clock intentionally change on insert.
        assert_eq!(
            actual.framing[..actual.framing.len() - 1],
            expected.framing[..expected.framing.len() - 1]
        );
    }
}

#[test]
fn cropped_source_and_hold_retain_owner_progress_and_full_nested_clips() {
    let before = document(
        &["prefix", "scope", "suffix"],
        vec![
            ("prefix", source(2, -2002, 0)),
            (
                "scope",
                creep(
                    BeatNode::sequence("", vec![id("source"), id("owned"), id("hold")]),
                    7,
                ),
            ),
            ("source", creep(source(9, -2002, 7007), 3)),
            (
                "owned",
                creep(BeatNode::sequence("", vec![id("neutral-clip")]), 2),
            ),
            ("neutral-clip", BeatNode::sequence("", vec![id("inner")])),
            ("inner", creep(source(4, 10010, 14014), 4)),
            ("hold", creep(freeze(6, 4004), 5)),
            ("suffix", source(3, 16016, 19019)),
        ],
    );
    let copied = capture(&before, "scope", 5, 18);
    assert_eq!(copied.duration(), duration(13));
    let after = paste(&before, &copied, 2, "paste-crop");
    assert_destination_unchanged(&before, &after, 21, 13);
    let original = RenderPlan::compile(&before).unwrap();
    let plan = RenderPlan::compile(&after).unwrap();
    let pts = [
        -2002, -1001, 1001, 4004, 6506, 7007, 8008, 9009, 10010, 13013, 16016, 19019,
    ];
    let index = SourceFrameIndex::new(
        asset(),
        clock(),
        pts.into_iter()
            .enumerate()
            .map(|(ordinal, pts)| IndexedSourceFrame {
                identity: SourceFrameId(u64::try_from(ordinal).unwrap()),
                pts,
                reported_duration: None,
                keyframe: ordinal == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            })
            .collect(),
        20020,
        TerminalProvenance::Explicit,
    )
    .unwrap();
    let expected_frames = [2, 2, 2, 3, 3, 4, 8, 8, 8, 9, 3, 3, 3];
    for offset in (0..13).rev() {
        let sample = plan.picture(ProjectFrame(21 + offset)).unwrap();
        let old = original.picture(ProjectFrame(5 + offset)).unwrap();
        assert_owned_layers(&old, &before, &sample, &after, &["scope", "root"]);
        let (local, owner_frames, end_scale, ticks) = match offset {
            0..=5 => (
                ratio(i128::from(2 * offset + 7), 2),
                9,
                3,
                ratio(i128::from(3003 + 2002 * offset), 2),
            ),
            6..=9 => (
                ratio(i128::from(2 * offset - 11), 2),
                4,
                4,
                ratio(i128::from(20020 + 1001 * (2 * offset - 11)), 2),
            ),
            _ => (
                ratio(i128::from(2 * offset - 19), 2),
                6,
                5,
                ExactRatio::integer(4004),
            ),
        };
        let provider = &sample.framing[0];
        assert_eq!(
            (provider.local_position, provider.duration),
            (local, duration(owner_frames))
        );
        assert_eq!(
            provider.pose.unwrap().scale,
            linear_scale_at(local, owner_frames, end_scale)
        );
        assert_eq!(picture_ticks(&sample), ticks);
        assert_eq!(
            sample.picture.select_source_frame(&index).unwrap().identity,
            SourceFrameId(expected_frames[usize::try_from(offset).unwrap()])
        );
        assert_eq!(
            sample.picture_context.as_deref(),
            (offset >= 10).then_some(&captured_geometry())
        );
    }
}

#[test]
fn partial_partition_copy_survives_deletion_and_each_paste_has_independent_owners() {
    let initial = document(
        &["scope", "suffix"],
        vec![
            ("scope", BeatNode::sequence("", vec![id("source")])),
            ("source", creep(source(9, -2002, 7007), 3)),
            ("suffix", source(2, 10010, 12012)),
        ],
    );
    let split = edit(
        &initial,
        "split",
        Command::Split {
            node: id("source"),
            at: duration(4),
            identities: SplitIdentities {
                nodes: (0..10).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    let slice = capture(&split, "scope", 5, 8);
    let deleted = edit(
        &split,
        "delete-original",
        Command::DeleteRipple {
            node: id("scope"),
            timing: timing("delete-original"),
        },
    );
    assert!(!deleted.nodes().contains_key(&id("source")));
    let once = paste(&deleted, &slice, 0, "first-copy");
    let twice = paste(&once, &slice, 1, "second-copy");
    let recopy = capture(&twice, "root", 0, 3);
    let thrice = paste(&twice, &recopy, 2, "copy-of-copy");
    assert_destination_unchanged(&twice, &thrice, 6, 3);
    let plan = RenderPlan::compile(&thrice).unwrap();
    let original = RenderPlan::compile(&initial).unwrap();
    let owners: Vec<_> = [0, 3, 6]
        .into_iter()
        .map(|frame| plan.picture(ProjectFrame(frame)).unwrap().instance.node)
        .collect();
    assert!(owners[0] != owners[1] && owners[1] != owners[2] && owners[0] != owners[2]);
    for start in [0, 3, 6] {
        for offset in 0..3 {
            let sample = plan.picture(ProjectFrame(start + offset)).unwrap();
            let old = original.picture(ProjectFrame(5 + offset)).unwrap();
            assert_eq!(sample.picture, old.picture);
            assert_eq!(
                sample.framing[0].local_position,
                ratio(i128::from(11 + 2 * offset), 2)
            );
            assert_eq!(sample.framing[0].duration, duration(9));
            assert_eq!(
                sample.framing[0].pose.unwrap().scale,
                linear_scale_at(ratio(i128::from(11 + 2 * offset), 2), 9, 3)
            );
            sample.instance.validate(&thrice).unwrap();
        }
    }
    let changed = edit(
        &thrice,
        "edit-first-owner",
        Command::SetFraming {
            node: owners[0].clone(),
            framing: None,
        },
    );
    let changed_plan = RenderPlan::compile(&changed).unwrap();
    assert!(
        changed_plan.picture(ProjectFrame(0)).unwrap().framing[0]
            .pose
            .is_none()
    );
    for frame in 3..11 {
        let expected = plan.picture(ProjectFrame(frame)).unwrap();
        let actual = changed_plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(actual.picture, expected.picture);
        assert_eq!(actual.framing, expected.framing);
    }
}

#[test]
fn partial_copy_of_an_already_cropped_partition_keeps_the_original_owner_clock() {
    let initial = document(
        &["scope", "suffix"],
        vec![
            ("scope", BeatNode::sequence("", vec![id("source")])),
            ("source", creep(source(9, -2002, 7007), 3)),
            ("suffix", source(2, 10010, 12012)),
        ],
    );
    let split = edit(
        &initial,
        "split",
        Command::Split {
            node: id("source"),
            at: duration(4),
            identities: SplitIdentities {
                nodes: (0..10).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    let slice = capture(&split, "scope", 5, 8);
    let deleted = edit(
        &split,
        "delete-original",
        Command::DeleteRipple {
            node: id("scope"),
            timing: timing("delete-original"),
        },
    );
    let once = paste(&deleted, &slice, 0, "first-crop");
    // Enter the pasted Sequence and refine its first endpoint. This traverses
    // both the existing Split window and the first copied interval's window.
    let refined = capture(&once, "first-crop-node-0", 1, 3);
    assert_eq!(refined.duration(), duration(2));
    let twice = paste(&once, &refined, 1, "refined-copy");
    assert_destination_unchanged(&once, &twice, 3, 2);
    let original = RenderPlan::compile(&initial).unwrap();
    let plan = RenderPlan::compile(&twice).unwrap();
    for offset in [1, 0] {
        let sample = plan.picture(ProjectFrame(3 + offset)).unwrap();
        assert_eq!(
            sample.picture,
            original.picture(ProjectFrame(6 + offset)).unwrap().picture
        );
        assert_eq!(
            picture_ticks(&sample),
            ratio(i128::from(9009 + 2002 * offset), 2)
        );
        let provider = &sample.framing[0];
        assert_eq!(
            provider.local_position,
            ratio(i128::from(13 + 2 * offset), 2)
        );
        assert_eq!(provider.duration, duration(9));
        assert_eq!(
            provider.pose.unwrap().scale,
            linear_scale_at(ratio(i128::from(13 + 2 * offset), 2), 9, 3)
        );
        assert!(sample.framing[1..].iter().all(|layer| layer.pose.is_none()));
        sample.instance.validate(&twice).unwrap();
    }
}

#[test]
fn full_repeat_overrides_gaps_and_preserve_retime_keep_exact_picture_clocks() {
    let repeat = BeatNode {
        label: String::new(),
        framing: None,
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("base"),
            iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
            gap: Some(freeze_recipe(1, 7007)),
        },
    };
    let retime = BeatNode {
        label: String::new(),
        framing: None,
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id("retime-source"),
            duration: duration(5),
            mapping: range(1, 8),
            pitch: PitchPolicy::Preserve,
        },
    };
    let mut before = document(
        &["prefix", "scope", "suffix"],
        vec![
            ("prefix", source(2, -2002, 0)),
            (
                "scope",
                creep(BeatNode::sequence("", vec![id("repeat"), id("retime")]), 7),
            ),
            ("repeat", creep(repeat, 2)),
            ("base", creep(source(2, 0, 2002), 3)),
            ("retime", creep(retime, 2)),
            ("retime-source", creep(source(9, 10010, 19019), 3)),
            ("suffix", source(3, 16016, 19019)),
        ],
    );
    let play = |ordinal| IterationId {
        allocation: revision("plays"),
        ordinal,
    };
    let subtree = |name: &str, mut node: BeatNode| {
        node.label = name.into();
        Subtree {
            root: id(name),
            nodes: BTreeMap::from([(id(name), node)]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        }
    };
    before = edit(
        &before,
        "play-override",
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: play(1),
            subtree: subtree("alternate", creep(source(3, 6006, 9009), 4)),
        },
    );
    before = edit(
        &before,
        "gap-override",
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: play(0),
            subtree: subtree("alternate-gap", creep(freeze(2, -1001), 2)),
        },
    );
    let slice = capture(&before, "scope", 2, 17);
    let after = paste(&before, &slice, 2, "paste-composites");
    assert_destination_unchanged(&before, &after, 17, 15);
    let original = RenderPlan::compile(&before).unwrap();
    let plan = RenderPlan::compile(&after).unwrap();
    let doubled_repeat_ticks = [
        1001, 3003, -2002, -2002, 13013, 15015, 17017, 14014, 1001, 3003,
    ];
    for offset in 0..15 {
        let sample = plan.picture(ProjectFrame(17 + offset)).unwrap();
        let old = original.picture(ProjectFrame(2 + offset)).unwrap();
        assert_owned_layers(&old, &before, &sample, &after, &["scope", "root"]);
        if offset < 10 {
            assert_eq!(
                picture_ticks(&sample),
                ratio(doubled_repeat_ticks[usize::try_from(offset).unwrap()], 2)
            );
            let expected_play = match offset {
                0..=3 => 0,
                4..=7 => 1,
                _ => 2,
            };
            if offset == 7 {
                let gap = sample.gap_after.unwrap();
                assert_eq!(gap.ordinal, expected_play);
                assert_ne!(gap.allocation, revision("plays"));
                assert!(sample.instance.repeats.is_empty());
            } else {
                assert!(sample.gap_after.is_none());
                assert_eq!(sample.instance.repeats.len(), 1);
                assert_eq!(sample.instance.repeats[0].iteration.ordinal, expected_play);
                assert_ne!(
                    sample.instance.repeats[0].iteration.allocation,
                    revision("plays")
                );
                assert_ne!(sample.instance.repeats[0].node, id("repeat"));
            }
            assert_eq!(
                sample.picture_context.as_deref(),
                [2, 3, 7].contains(&offset).then_some(&captured_geometry())
            );
        } else {
            let frame = offset - 10;
            let child_position = ratio(i128::from(17 + 14 * frame), 10);
            assert_eq!(sample.framing[0].local_position, child_position);
            assert_eq!(sample.framing[0].duration, duration(9));
            assert_eq!(
                sample.framing[0].pose.unwrap().scale,
                linear_scale_at(child_position, 9, 3)
            );
            assert_eq!(
                sample.framing[1].local_position,
                ratio(i128::from(2 * frame + 1), 2)
            );
            assert_eq!(sample.framing[1].duration, duration(5));
            assert_eq!(
                sample.framing[1].pose.unwrap().scale,
                linear_scale_at(ratio(i128::from(2 * frame + 1), 2), 5, 2)
            );
            assert_eq!(
                picture_ticks(&sample),
                ratio(i128::from(100100 + 1001 * (17 + 14 * frame)), 10)
            );
            assert!(sample.instance.repeats.is_empty());
        }
    }
}
