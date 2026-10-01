//! Whole-output coordinate witnesses for atomic moves. No decoding or GPU claim.
use super::placement::{assert_live_scope, fixture, is_partition};
use super::*;

fn moved(
    before: &ProjectDocument,
    parent: &str,
    selected: FrameRange,
    destination: MoveRangeDestination,
    name: &str,
) -> ProjectDocument {
    let query = before
        .range_move(&id(parent), selected, &destination)
        .unwrap();
    let tx = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(name),
            command: Command::MoveRange {
                source_revision: before.revision_id().clone(),
                source_parent: id(parent),
                range: selected,
                destination,
                identities: SplitIdentities {
                    nodes: (0..query.required_ids)
                        .map(|n| id(&format!("{name}-{n}")))
                        .collect(),
                },
                timing: timing(name),
            },
        },
    )
    .unwrap();
    let result = tx.forward.apply(before).unwrap();
    assert_eq!(tx.inverse.apply(&result).unwrap(), *before);
    assert_eq!(result.duration().unwrap(), before.duration().unwrap());
    result
}

fn interior(parent: &str, target: &str, at: i64) -> MoveRangeDestination {
    MoveRangeDestination::Interior {
        parent: id(parent),
        target: id(target),
        at: duration(at),
    }
}

// These are hand-authored old intervals in final order, independent of preflight.
// Include all frames, including both cuts, terminal Holds, gaps and overrides.
fn assert_permutation(before: &ProjectDocument, after: &ProjectDocument, intervals: &[(i64, i64)]) {
    let old = RenderPlan::compile(before).unwrap();
    let new = RenderPlan::compile(after).unwrap();
    let order: Vec<_> = intervals.iter().flat_map(|&(a, b)| a..b).collect();
    assert_eq!(
        order.len(),
        usize::try_from(old.duration().frames()).unwrap()
    );
    assert_eq!(new.duration(), old.duration());
    for (output, &original) in order.iter().enumerate().rev() {
        let was = old.picture(ProjectFrame(original)).unwrap();
        let now = new
            .picture(ProjectFrame(i64::try_from(output).unwrap()))
            .unwrap();
        assert_eq!(now.picture, was.picture, "output {output}, old {original}");
        assert_eq!(now.local_position, was.local_position);
        assert_eq!(now.picture_context, was.picture_context);
        assert_eq!(now.gap_after, was.gap_after);
        assert_eq!(now.instance.repeats, was.instance.repeats);
        let owners = |sample: &PictureSample, document: &ProjectDocument| {
            sample
                .framing
                .iter()
                .filter(|layer| {
                    !is_partition(document, &layer.instance.node)
                        && !["root", "scope", "donor"]
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
        assert_eq!(owners(&now, after), owners(&was, before), "output {output}");
        now.instance.validate(after).unwrap();
        for layer in &now.framing {
            layer.instance.validate(after).unwrap();
            if is_partition(after, &layer.instance.node) {
                assert!(layer.pose.is_none());
            }
        }
    }
}

#[test]
fn three_cuts_in_one_source_preserve_every_ordinal_and_owner_clock_in_both_directions() {
    let before = document(&["voice"], vec![("voice", creep(source(9, 0, 9009), 5))]);
    let index = SourceFrameIndex::new(
        asset(),
        clock(),
        (0_u64..9)
            .map(|ordinal| IndexedSourceFrame {
                identity: SourceFrameId(ordinal),
                pts: i64::try_from(ordinal).unwrap() * 1001,
                reported_duration: None,
                keyframe: ordinal == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            })
            .collect(),
        9009,
        TerminalProvenance::Explicit,
    )
    .unwrap();
    for (selected, at, name) in [(range(2, 4), 6, "right"), (range(4, 6), 2, "left")] {
        let destination = interior("root", "voice", at);
        let query = before
            .range_move(&id("root"), selected, &destination)
            .unwrap();
        assert_eq!(query.required_ids, 7);
        let after = moved(&before, "root", selected, destination, name);
        assert_permutation(&before, &after, &[(0, 2), (4, 6), (2, 4), (6, 9)]);
        let plan = RenderPlan::compile(&after).unwrap();
        for (output, original) in [0, 1, 4, 5, 2, 3, 6, 7, 8].into_iter().enumerate() {
            let sample = plan
                .picture(ProjectFrame(i64::try_from(output).unwrap()))
                .unwrap();
            let local = ratio(i128::from(2 * original + 1), 2);
            assert_eq!(
                picture_ticks(&sample),
                local.checked_mul(ExactRatio::integer(1001)).unwrap()
            );
            assert_eq!(
                sample.picture.select_source_frame(&index).unwrap().identity,
                SourceFrameId(u64::try_from(original).unwrap())
            );
            let owner = sample
                .framing
                .iter()
                .find(|layer| after.nodes()[&layer.instance.node].label == "voice")
                .unwrap();
            assert_eq!(owner.local_position, local);
            assert_eq!(owner.duration, duration(9));
            assert_eq!(owner.pose.unwrap().scale, linear_scale_at(local, 9, 5));
        }
    }
}

#[test]
fn cross_parent_moves_keep_owned_repeat_preserve_and_hold_contexts_but_ancestors_stay_live() {
    let before = fixture();
    assert_eq!(before.duration().unwrap(), duration(59));
    for (parent, selected, destination, intervals, scope_frames, donor_start, donor_frames, name) in [
        (
            "donor",
            range(33, 52),
            interior("scope", "target", 4),
            vec![(0, 6), (33, 52), (6, 33), (52, 59)],
            48,
            50,
            6,
            "cross-left",
        ),
        (
            "scope",
            range(5, 24),
            interior("donor", "donor-source", 2),
            vec![(0, 5), (24, 33), (5, 24), (33, 59)],
            10,
            12,
            44,
            "cross-right",
        ),
    ] {
        let after = moved(&before, parent, selected, destination, name);
        assert_permutation(&before, &after, &intervals);
        for node in [
            "owned",
            "copied-repeat",
            "copied-retime",
            "destination-repeat",
            "destination-retime",
        ] {
            assert_eq!(after.nodes()[&id(node)], before.nodes()[&id(node)]);
        }
        assert_eq!(after.overrides(), before.overrides());
        assert_eq!(after.gap_overrides(), before.gap_overrides());
        let plan = RenderPlan::compile(&after).unwrap();
        for frame in 2..2 + scope_frames {
            let sample = plan.picture(ProjectFrame(frame)).unwrap();
            assert_live_scope(&sample, &after, "scope", 2, scope_frames, 7);
            assert!(
                !sample
                    .framing
                    .iter()
                    .any(|layer| layer.instance.node == id("donor"))
            );
        }
        for frame in donor_start..donor_start + donor_frames {
            let sample = plan.picture(ProjectFrame(frame)).unwrap();
            assert_live_scope(&sample, &after, "donor", donor_start, donor_frames, 9);
            assert!(
                !sample
                    .framing
                    .iter()
                    .any(|layer| layer.instance.node == id("scope"))
            );
        }
    }
}

fn mark(
    document: &ProjectDocument,
    name: &str,
    coordinate: Anchor,
    bias: InsertionBias,
) -> ProjectDocument {
    edit(
        document,
        name,
        Command::SetMark {
            id: MarkId::new(name).unwrap(),
            owner: id("root"),
            label: name.into(),
            boundary: BoundaryAnchor { coordinate, bias },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
}

#[test]
fn moved_marks_keep_identity_bias_and_child_host_while_departed_parent_marks_become_unresolved() {
    let mut before = fixture();
    for (name, host, position, bias) in [
        ("start-left", "root", 33, InsertionBias::Left),
        ("start-right", "root", 33, InsertionBias::Right),
        ("end-left", "root", 52, InsertionBias::Left),
        ("end-right", "root", 52, InsertionBias::Right),
        ("child-local", "owned", 4, InsertionBias::Right),
        ("old-parent", "donor", 7, InsertionBias::Right),
    ] {
        before = mark(
            &before,
            name,
            Anchor::Local {
                node: id(host),
                position: ExactRatio::integer(position),
            },
            bias,
        );
    }
    before = mark(
        &before,
        "absolute",
        Anchor::Sequence {
            frame: ProjectFrame(36),
        },
        InsertionBias::Right,
    );
    let after = moved(
        &before,
        "donor",
        range(33, 52),
        interior("scope", "target", 4),
        "marked-move",
    );
    assert_eq!(
        after.marks().keys().collect::<Vec<_>>(),
        before.marks().keys().collect::<Vec<_>>()
    );
    for (name, position) in [
        ("start-left", 52),
        ("start-right", 6),
        ("end-left", 25),
        ("end-right", 52),
    ] {
        let mark = &after.marks()[&MarkId::new(name).unwrap()];
        assert_eq!(mark.state, MarkState::Bound);
        assert_eq!(
            mark.boundary.coordinate,
            Anchor::Local {
                node: id("root"),
                position: ExactRatio::integer(position)
            }
        );
        assert_eq!(
            mark.boundary.bias,
            before.marks()[&MarkId::new(name).unwrap()].boundary.bias
        );
    }
    for name in ["child-local", "absolute"] {
        assert_eq!(
            after.marks()[&MarkId::new(name).unwrap()],
            before.marks()[&MarkId::new(name).unwrap()]
        );
    }
    let departed = &after.marks()[&MarkId::new("old-parent").unwrap()];
    assert_eq!(
        departed.state,
        MarkState::Unresolved {
            reason: MarkLossReason::OutsideHost
        }
    );
    assert_eq!(
        departed.boundary,
        before.marks()[&MarkId::new("old-parent").unwrap()].boundary
    );
}
