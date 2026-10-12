use super::*;

fn instance(node: &str, plays: &[(&str, u32)]) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: plays
            .iter()
            .map(|(name, ordinal)| RepeatInstance {
                node: id(name),
                iteration: IterationId {
                    allocation: revision(name),
                    ordinal: *ordinal,
                },
            })
            .collect(),
    }
}

#[test]
fn scoped_pause_owns_both_edges_of_the_named_play_and_nested_child() {
    for nested in [false, true] {
        let before = fixture(nested);
        for play in 0..3 {
            let start = i64::from(play) * if nested { 13 } else { 6 };
            let mut targets = vec![(
                instance(if nested { "body" } else { "a" }, &[("outer", play)]),
                start,
                if nested { 11 } else { 4 },
            )];
            if nested {
                targets.extend([
                    (instance("inner", &[("outer", play)]), start, 9),
                    (instance("a", &[("outer", play), ("inner", 0)]), start, 4),
                    (
                        instance("a", &[("outer", play), ("inner", 1)]),
                        start + 5,
                        4,
                    ),
                    (instance("b", &[("outer", play)]), start + 9, 2),
                ]);
            }
            for (within, start, length) in targets {
                for local in 0..=length {
                    let at = ProjectFrame(start + local);
                    let request = request_within(&before, at, "scoped", Some(within.clone()));
                    let request = serde_json::from_str::<CommandRequest>(
                        &serde_json::to_string(&request).unwrap(),
                    )
                    .unwrap();
                    let (transaction, after) =
                        apply_with_result(&before, &request).unwrap_or_else(|error| {
                            panic!("nested={nested} within={within:?} at={at:?}: {error}")
                        });
                    assert_eq!(transaction.duration_delta, 1);
                    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
                    assert_eq!(
                        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
                        after
                    );
                    let proof = derive_scoped_isolation(&before, &request, &after).unwrap();
                    proof.record().validate(&before, &request, &after).unwrap();
                    let location = AnchorIndex::new(&after)
                        .unwrap()
                        .locate_boundary(
                            &BoundaryLocationRequest {
                                project_id: after.project_id().clone(),
                                expected_revision: after.revision_id().clone(),
                                position: ExactRatio::new(i128::from(at.0) * 2 + 1, 2).unwrap(),
                                bias: InsertionBias::Right,
                            },
                            BoundaryQueryLimits::default(),
                        )
                        .unwrap();
                    let pause_instance = location.scopes.last().unwrap().instance.clone();
                    assert_eq!(pause_instance.node, id("scoped-pause"));
                    assert_eq!(pause_instance.repeats[0].iteration.ordinal, play);
                    let position = AnchorIndex::new(&after)
                        .unwrap()
                        .resolve_target(&AnchorTarget {
                            boundary: BoundaryAnchor {
                                coordinate: Anchor::Occurrence {
                                    instance: pause_instance,
                                    position: ExactRatio::ZERO,
                                },
                                bias: InsertionBias::Right,
                            },
                            occurrence: None,
                        })
                        .unwrap();
                    assert_eq!(position.exact_frame, ExactRatio::integer(at.0));
                    assert_eq!(after.nodes()[&id("a")], before.nodes()[&id("a")]);
                    assert_eq!(after.overrides()[&id("outer")].len(), 1);
                }
            }
        }
    }
}

#[test]
fn scoped_pause_rejects_foreign_boundaries_instances_and_retimed_ancestors() {
    let before = fixture(false);
    let within = instance("a", &[("outer", 1)]);
    for at in [0, 5, 11, 19] {
        assert!(
            before
                .pause_insertion_within(ProjectFrame(at), &within)
                .is_err()
        );
    }
    for within in [
        instance("a", &[]),
        instance("a", &[("outer", 3)]),
        instance("tail", &[("outer", 1)]),
    ] {
        assert!(
            before
                .pause_insertion_within(ProjectFrame(7), &within)
                .is_err()
        );
    }
    let mut retimed = before.clone();
    let NodeKind::Repeat { child, .. } = &mut retimed.nodes.get_mut(&id("outer")).unwrap().kind
    else {
        panic!()
    };
    *child = id("retime");
    let mut wrapper = BeatNode::sequence("Retime", vec![]);
    wrapper.kind = NodeKind::Retime {
        child: id("a"),
        duration: FrameDuration::new(3).unwrap(),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    retimed.nodes.insert(id("retime"), wrapper);
    retimed.validate().unwrap();
    assert!(
        retimed
            .pause_insertion_within(ProjectFrame(6), &within)
            .is_err()
    );
}
