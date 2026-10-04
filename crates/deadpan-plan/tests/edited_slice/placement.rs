//! Canonical coordinate witnesses, independent of media decoding and GPU output.

use super::*;

fn paste_identities(slice: &CapturedEditSlice, name: &str) -> SlicePasteIdentities {
    let required = slice.identity_requirements().unwrap();
    SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..required.nodes)
                .map(|n| id(&format!("{name}-import-{n}")))
                .collect(),
            marks: (0..required.marks)
                .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                .collect(),
        },
        aliases: (0..required.aliases)
            .map(|n| id(&format!("{name}-alias-{n}")))
            .collect(),
    }
}

fn split_identities(count: usize, name: &str) -> SplitIdentities {
    SplitIdentities {
        nodes: (0..count)
            .map(|n| id(&format!("{name}-split-{n}")))
            .collect(),
    }
}

fn insert_at(
    before: &ProjectDocument,
    parent: &str,
    target: &NodeId,
    at: i64,
    slice: &CapturedEditSlice,
    name: &str,
) -> ProjectDocument {
    let preflight = before
        .slice_splice_interior(&id(parent), target, duration(at), slice)
        .unwrap();
    assert!(preflight.required_ids > 0);
    edit(
        before,
        name,
        Command::SpliceSliceAt {
            parent: id(parent),
            target: target.clone(),
            at: duration(at),
            slice: slice.clone(),
            identities: paste_identities(slice, name),
            split_identities: split_identities(preflight.required_ids, name),
            timing: timing(name),
        },
    )
}

fn replace(
    before: &ProjectDocument,
    parent: &str,
    removed: FrameRange,
    slice: &CapturedEditSlice,
    name: &str,
) -> ProjectDocument {
    let preflight = before
        .slice_replacement(&id(parent), removed, slice)
        .unwrap();
    edit(
        before,
        name,
        Command::ReplaceSlice {
            parent: id(parent),
            range: removed,
            slice: slice.clone(),
            identities: paste_identities(slice, name),
            split_identities: split_identities(preflight.required_ids, name),
            timing: timing(name),
        },
    )
}

pub(super) fn is_partition(document: &ProjectDocument, node: &NodeId) -> bool {
    matches!(
        document.nodes()[node].kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    )
}

// Split can add or crop neutral Partition windows and rename the right owner.
// Compare every retained creative owner and ordinary clip in composition order.
// The explicitly named ancestors remain live in their new enclosing duration.
fn assert_survivors(
    before: &ProjectDocument,
    after: &ProjectDocument,
    removed: FrameRange,
    inserted: i64,
    live_ancestors: &[&str],
) {
    let old = RenderPlan::compile(before).unwrap();
    let new = RenderPlan::compile(after).unwrap();
    let delta = inserted - removed.duration().frames();
    assert_eq!(new.duration().frames(), old.duration().frames() + delta);
    for frame in (0..old.duration().frames()).rev() {
        if frame >= removed.start().0 && frame < removed.end().0 {
            continue;
        }
        let old_sample = old.picture(ProjectFrame(frame)).unwrap();
        let new_frame = frame + i64::from(frame >= removed.end().0) * delta;
        let new_sample = new.picture(ProjectFrame(new_frame)).unwrap();
        assert_eq!(new_sample.picture, old_sample.picture, "old frame {frame}");
        assert_eq!(new_sample.picture_context, old_sample.picture_context);
        assert_eq!(new_sample.local_position, old_sample.local_position);
        assert_eq!(new_sample.gap_after, old_sample.gap_after);
        let scopes = |sample: &PictureSample, document: &ProjectDocument| {
            sample
                .framing
                .iter()
                .filter(|layer| {
                    !is_partition(document, &layer.instance.node)
                        && !live_ancestors
                            .contains(&document.nodes()[&layer.instance.node].label.as_str())
                })
                .map(|layer| {
                    (
                        document.nodes()[&layer.instance.node].label.clone(),
                        layer.local_position,
                        layer.duration,
                        layer.pose,
                        layer.instance.repeats.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(scopes(&new_sample, after), scopes(&old_sample, before));
        new_sample.instance.validate(after).unwrap();
        for layer in &new_sample.framing {
            layer.instance.validate(after).unwrap();
            if is_partition(after, &layer.instance.node) {
                assert!(layer.pose.is_none());
            }
        }
    }
}

pub(super) fn assert_live_scope(
    sample: &PictureSample,
    document: &ProjectDocument,
    name: &str,
    start: i64,
    frames: i64,
    end_scale: i64,
) {
    let layers: Vec<_> = sample
        .framing
        .iter()
        .filter(|layer| document.nodes()[&layer.instance.node].label == name)
        .collect();
    assert_eq!(layers.len(), 1);
    let local = ratio(i128::from(2 * (sample.project_frame.0 - start) + 1), 2);
    assert_eq!(layers[0].local_position, local);
    assert_eq!(layers[0].duration, duration(frames));
    assert_eq!(
        layers[0].pose.unwrap().scale,
        linear_scale_at(local, frames, end_scale)
    );
}

fn repeat(child: &str, plays: u32, allocation: &str) -> BeatNode {
    BeatNode {
        label: String::new(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(revision(allocation), plays).unwrap(),
            gap: Some(freeze_recipe(1, 7007)),
            escalation: None,
        },
    }
}

fn preserve(child: &str) -> BeatNode {
    BeatNode {
        label: String::new(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id(child),
            duration: duration(5),
            mapping: range(1, 8),
            pitch: PitchPolicy::Preserve,
        },
    }
}

pub(super) fn fixture() -> ProjectDocument {
    let before = document(
        &["lead", "scope", "donor", "tail"],
        vec![
            ("lead", source(2, -5005, -3003)),
            (
                "scope",
                creep(
                    BeatNode::sequence(
                        "",
                        vec![
                            id("target"),
                            id("destination-repeat"),
                            id("destination-retime"),
                            id("destination-hold"),
                            id("destination-suffix"),
                        ],
                    ),
                    7,
                ),
            ),
            ("target", creep(source(9, -2002, 7007), 3)),
            (
                "destination-repeat",
                creep(repeat("destination-base", 2, "destination-plays"), 2),
            ),
            ("destination-base", creep(source(2, 8008, 10010), 3)),
            (
                "destination-retime",
                creep(preserve("destination-retime-source"), 2),
            ),
            (
                "destination-retime-source",
                creep(source(9, 10010, 19019), 3),
            ),
            ("destination-hold", creep(freeze(6, 4004), 5)),
            ("destination-suffix", creep(source(4, 14014, 18018), 2)),
            (
                "donor",
                creep(
                    BeatNode::sequence("", vec![id("donor-source"), id("owned"), id("donor-hold")]),
                    9,
                ),
            ),
            ("donor-source", creep(source(4, 20020, 24024), 5)),
            (
                "owned",
                creep(
                    BeatNode::sequence("", vec![id("copied-repeat"), id("copied-retime")]),
                    2,
                ),
            ),
            (
                "copied-repeat",
                creep(repeat("copied-base", 3, "copied-plays"), 2),
            ),
            ("copied-base", creep(source(2, 0, 2002), 3)),
            ("copied-retime", creep(preserve("copied-retime-source"), 2)),
            ("copied-retime-source", creep(source(9, 10010, 19019), 3)),
            ("donor-hold", creep(freeze(6, 28028), 6)),
            ("tail", source(3, 30030, 33033)),
        ],
    );
    let subtree = |name: &str, mut node: BeatNode| {
        node.label = name.into();
        Subtree {
            root: id(name),
            nodes: BTreeMap::from([(id(name), node)]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        }
    };
    let with_play = edit(
        &before,
        "override-play",
        Command::SetPlayOverride {
            node: id("copied-repeat"),
            iteration: IterationId {
                allocation: revision("copied-plays"),
                ordinal: 1,
            },
            subtree: subtree("alternate", creep(source(3, 6006, 9009), 4)),
        },
    );
    edit(
        &with_play,
        "override-gap",
        Command::SetGapOverride {
            node: id("copied-repeat"),
            iteration: IterationId {
                allocation: revision("copied-plays"),
                ordinal: 0,
            },
            subtree: subtree("alternate-gap", creep(freeze(2, -1001), 2)),
        },
    )
}

// The selected donor starts at frame 33: Source local 2, then the complete
// owned Repeat/Preserve group, then zero or more Hold frames. All scalar
// coordinates below are derived from those fixture spans, not the old plan.
fn assert_inserted(before: &ProjectDocument, after: &ProjectDocument, insertion: i64, frames: i64) {
    let old = RenderPlan::compile(before).unwrap();
    let new = RenderPlan::compile(after).unwrap();
    let repeat_ticks = [
        1001, 3003, -2002, -2002, 13013, 15015, 17017, 14014, 1001, 3003,
    ];
    for offset in (0..frames).rev() {
        let sample = new.picture(ProjectFrame(insertion + offset)).unwrap();
        let original = old.picture(ProjectFrame(33 + offset)).unwrap();
        // The destination scope is live, and therefore checked separately.
        let mut owned_sample = sample.clone();
        owned_sample
            .framing
            .retain(|layer| after.nodes()[&layer.instance.node].label != "scope");
        assert_owned_layers(&original, before, &owned_sample, after, &["donor", "root"]);
        let (local, owner_frames, end_scale, ticks) = match offset {
            0..=1 => (
                ratio(i128::from(5 + 2 * offset), 2),
                4,
                5,
                ratio(i128::from(40040 + 1001 * (5 + 2 * offset)), 2),
            ),
            2..=11 => {
                let repeat_frame = offset - 2;
                let ticks = ratio(repeat_ticks[usize::try_from(repeat_frame).unwrap()], 2);
                assert_eq!(picture_ticks(&sample), ticks);
                if repeat_frame == 7 {
                    let gap = sample.gap_after.as_ref().unwrap();
                    assert_eq!(gap.ordinal, 1);
                    assert_ne!(gap.allocation, revision("copied-plays"));
                    assert!(sample.instance.repeats.is_empty());
                } else {
                    let ordinal = match repeat_frame {
                        0..=3 => 0,
                        4..=6 => 1,
                        _ => 2,
                    };
                    assert_eq!(sample.instance.repeats.len(), 1);
                    assert_eq!(sample.instance.repeats[0].iteration.ordinal, ordinal);
                    assert_ne!(sample.instance.repeats[0].node, id("copied-repeat"));
                    assert_ne!(
                        sample.instance.repeats[0].iteration.allocation,
                        revision("copied-plays")
                    );
                }
                assert_eq!(
                    sample.picture_context.as_deref(),
                    [2, 3, 7]
                        .contains(&repeat_frame)
                        .then_some(&captured_geometry())
                );
                continue;
            }
            12..=16 => {
                let frame = offset - 12;
                let local = ratio(i128::from(17 + 14 * frame), 10);
                assert_eq!(
                    sample.framing[1].local_position,
                    ratio(i128::from(2 * frame + 1), 2)
                );
                assert_eq!(sample.framing[1].duration, duration(5));
                assert_eq!(
                    sample.framing[1].pose.unwrap().scale,
                    linear_scale_at(ratio(i128::from(2 * frame + 1), 2), 5, 2)
                );
                (
                    local,
                    9,
                    3,
                    ratio(i128::from(100100 + 1001 * (17 + 14 * frame)), 10),
                )
            }
            _ => (
                ratio(i128::from(2 * (offset - 17) + 1), 2),
                6,
                6,
                ExactRatio::integer(28028),
            ),
        };
        assert_eq!(picture_ticks(&sample), ticks);
        assert_eq!(sample.framing[0].local_position, local);
        assert_eq!(sample.framing[0].duration, duration(owner_frames));
        assert_eq!(
            sample.framing[0].pose.unwrap().scale,
            linear_scale_at(local, owner_frames, end_scale)
        );
        assert_eq!(
            sample.picture_context.as_deref(),
            (offset >= 17).then_some(&captured_geometry())
        );
    }
}

#[test]
fn interior_insertion_retains_destination_clocks_and_copied_composite_ownership() {
    let before = fixture();
    let slice = capture(&before, "donor", 33, 52);
    let after = insert_at(&before, "scope", &id("target"), 4, &slice, "interior");
    assert_survivors(&before, &after, range(6, 6), 19, &["scope", "root"]);
    assert_inserted(&before, &after, 6, 19);
    let plan = RenderPlan::compile(&after).unwrap();
    for frame in 2..50 {
        assert_live_scope(
            &plan.picture(ProjectFrame(frame)).unwrap(),
            &after,
            "scope",
            2,
            48,
            7,
        );
    }
}

#[test]
fn shorter_equal_and_longer_replacement_preserve_both_joins_and_owned_clocks() {
    let before = fixture();
    // Source local 3 through Hold local 3, including a complete Repeat and
    // nonunity Preserve. Both endpoints require a retained owner context.
    for (frames, name) in [(17, "shorter"), (19, "equal"), (22, "longer")] {
        let slice = capture(&before, "donor", 33, 33 + frames);
        let after = replace(&before, "scope", range(5, 24), &slice, name);
        assert_survivors(&before, &after, range(5, 24), frames, &["scope", "root"]);
        assert_inserted(&before, &after, 5, frames);
        assert!(!after.nodes().contains_key(&id("destination-repeat")));
        assert!(!after.nodes().contains_key(&id("destination-retime")));
        let plan = RenderPlan::compile(&after).unwrap();
        let scope_frames = 29 + frames - 19;
        for frame in 2..2 + scope_frames {
            assert_live_scope(
                &plan.picture(ProjectFrame(frame)).unwrap(),
                &after,
                "scope",
                2,
                scope_frames,
                7,
            );
        }
    }
}

#[test]
fn copied_nested_partitions_allow_interior_insert_and_replacement_without_owner_clock_reset() {
    let initial = document(
        &["scope", "hold", "tail"],
        vec![
            ("scope", BeatNode::sequence("", vec![id("source")])),
            ("source", creep(source(9, -2002, 7007), 3)),
            ("hold", creep(freeze(4, 4004), 5)),
            ("tail", source(3, 16016, 19019)),
        ],
    );
    let split = edit(
        &initial,
        "initial-split",
        Command::Split {
            node: id("source"),
            at: duration(4),
            identities: split_identities(3, "initial-split"),
        },
    );
    let slice = capture(&split, "scope", 5, 9);
    let copied = paste(&split, &slice, 0, "nested-copy");
    let parent = "nested-copy-node-0";
    let target = copied.children(&id(parent)).next().unwrap().clone();
    assert!(is_partition(&copied, &target));
    let NodeKind::Retime { child, .. } = &copied.nodes()[&target].kind else {
        unreachable!()
    };
    assert!(is_partition(&copied, child));
    let pause = capture(&copied, "root", 13, 15);
    let inserted = insert_at(&copied, parent, &target, 1, &pause, "nested-interior");
    assert_survivors(
        &copied,
        &inserted,
        range(1, 1),
        2,
        &["root", "Copied contents"],
    );
    // Refine the surviving right-hand nested window and replace its middle
    // frame with two copied Hold frames. This traverses the same owner twice.
    let replaced = replace(&inserted, parent, range(4, 5), &pause, "nested-replace");
    assert_survivors(
        &inserted,
        &replaced,
        range(4, 5),
        2,
        &["root", "Copied contents"],
    );
    let plan = RenderPlan::compile(&replaced).unwrap();
    for (frame, original_frame) in [(0, 5), (3, 6), (6, 8)] {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        let local = ratio(i128::from(2 * original_frame + 1), 2);
        assert_eq!(
            picture_ticks(&sample),
            ratio(i128::from(-4004 + 1001 * (2 * original_frame + 1)), 2)
        );
        assert_eq!(sample.framing[0].local_position, local);
        assert_eq!(sample.framing[0].duration, duration(9));
        assert_eq!(
            sample.framing[0].pose.unwrap().scale,
            linear_scale_at(local, 9, 3)
        );
        sample.instance.validate(&replaced).unwrap();
    }
    for frame in [1, 2, 4, 5] {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(picture_ticks(&sample), ExactRatio::integer(4004));
        assert_eq!(
            sample.picture_context.as_deref(),
            Some(&captured_geometry())
        );
    }
}
