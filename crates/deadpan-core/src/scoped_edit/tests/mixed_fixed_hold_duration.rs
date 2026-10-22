//! Literal mixed families keep a fixed output clock and exact sampled geometry.
//! Silent structural tests; audible phase is qualified by the audio integration.

use super::*;

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn fixed(crop: i64, output: i64, pitch: PitchPolicy) -> ProjectDocument {
    let mut d = ProjectDocument::new(
        ProjectId::new("mixed-fixed-duration").unwrap(),
        revision("mixed-fixed-base"),
        crate::basis::default_basis(),
        node("root"),
    )
    .unwrap();
    let repeat = |name, child, count| {
        let mut beat = repeated(name, child, count);
        let NodeKind::Repeat { gap, .. } = &mut beat.kind else {
            unreachable!()
        };
        *gap = None;
        beat
    };
    let mut stage = BeatNode::sequence("Fixed stage", vec![]);
    stage.kind = NodeKind::Retime {
        child: node("body"),
        duration: frames(output),
        mapping: range(crop, 8),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    d.nodes.extend([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("outer"), node("tail")]),
        ),
        (node("lead"), BeatNode::hold("Lead", hold(3))),
        (node("tail"), BeatNode::hold("Tail", hold(3))),
        (node("outer"), repeat("outer", "inner", 3)),
        (node("inner"), repeat("inner", "stage", 2)),
        (node("stage"), stage),
        (
            node("body"),
            BeatNode::sequence("Body", vec![node("before"), node("held"), node("speech")]),
        ),
        (node("before"), BeatNode::hold("Before", hold(2))),
        (node("held"), BeatNode::hold("Held", hold(2))),
        (node("speech"), BeatNode::hold("Speech", hold(6))),
    ]);
    d.validate().unwrap();
    d
}

fn selected(outer_play: bool) -> ScopedNodeTarget {
    let mut target = target(
        if outer_play { Some(1) } else { None },
        if outer_play { None } else { Some(1) },
    );
    target.node = node("held");
    target
}

fn presented(outer: u32, inner: u32) -> InstancePath {
    let mut instance = instance(outer, inner);
    instance.node = node("held");
    instance
}

fn request_for(
    d: &ProjectDocument,
    target: ScopedNodeTarget,
    duration: i64,
    label: &str,
) -> CommandRequest {
    let needs = d
        .scoped_hold_duration_requirements(&target, frames(duration))
        .unwrap();
    CommandRequest {
        project_id: d.project_id().clone(),
        expected_revision: d.revision_id().clone(),
        new_revision: revision(label),
        command: Command::SetScopedHoldDuration {
            target,
            duration: frames(duration),
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|i| node(&format!("{label}-node-{i}")))
                    .collect(),
                marks: (0..needs.marks)
                    .map(|i| mark(&format!("{label}-mark-{i}")))
                    .collect(),
            },
        },
    }
}

fn visible(
    d: &ProjectDocument,
    target: &ScopedNodeTarget,
    presentation: Option<&InstancePath>,
) -> Option<HoldDurationVisibility> {
    d.mixed_hold_duration_visibility(
        &node("root"),
        &SemanticMixedHoldDurationContext {
            root: node("outer"),
            target: target.clone(),
            presentation: presentation.cloned(),
        },
    )
    .unwrap()
}

fn boundary(d: &ProjectDocument, instance: &InstancePath, position: i64) -> ExactRatio {
    AnchorIndex::new(d)
        .unwrap()
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: instance.clone(),
                    position: ExactRatio::integer(position),
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .unwrap()
        .exact_frame
}

fn reversible(d: &ProjectDocument, request: &CommandRequest, p: &PreparedScopedHoldDuration) {
    assert_eq!(p.transaction.inverse.apply(&p.document).unwrap(), *d);
    assert_eq!(
        ProjectDocument::from_json(&p.document.to_json().unwrap()).unwrap(),
        p.document
    );
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(request).unwrap()).unwrap(),
        *request
    );
    let (transaction, after) = apply_with_result(d, request).unwrap();
    assert_eq!(after, p.document);
    assert_eq!(transaction, p.transaction);
    let proof = p.isolation().validate(d, request, &p.document).unwrap();
    let Command::SetScopedHoldDuration { target, .. } = &request.command else {
        panic!()
    };
    assert_eq!(proof.map_forward(target).unwrap(), p.target);
    assert_eq!(proof.map_backward(&p.target).unwrap(), *target);
}

#[test]
fn mixed_fixed_duration_both_families_keep_literal_authority_exact_geometry_and_owned_continuation()
{
    for pitch in [
        PitchPolicy::FollowSpeed,
        PitchPolicy::Preserve,
        PitchPolicy::Shift { semitones: 3 },
    ] {
        let d = fixed(0, 6, pitch);
        assert_eq!(d.duration().unwrap().frames(), 42);
        for outer_play in [false, true] {
            let target = selected(outer_play);
            let instance = presented(1, 1);
            let old = visible(&d, &target, Some(&instance)).unwrap();
            assert_eq!(
                (old.exact.start, old.exact.end, old.frames),
                (
                    ExactRatio::new(45, 2).unwrap(),
                    ExactRatio::integer(24),
                    range(22, 24)
                )
            );
            let needs = d
                .scoped_hold_duration_requirements(&target, frames(4))
                .unwrap();
            assert_eq!(
                (needs.nodes, needs.marks),
                (if outer_play { 6 } else { 5 }, 0)
            );
            let request = request_for(&d, target.clone(), 4, "mixed-grow");
            let p = prepare_scoped_hold_duration(&d, &request, Some(&instance)).unwrap();
            assert_eq!(p.isolation().steps().len(), 1);
            assert_eq!(p.document.nodes().len(), d.nodes().len() + needs.nodes);
            assert_eq!(p.document.duration().unwrap().frames(), 42);
            assert_eq!(p.transaction.duration_delta, 0);
            assert_eq!(
                p.target.repeats[if outer_play { 1 } else { 0 }].branch,
                RepeatEditBranch::Default
            );
            let after_visible = visible(&p.document, &p.target, p.presentation.as_ref()).unwrap();
            assert_eq!(
                (
                    after_visible.exact.start,
                    after_visible.exact.end,
                    after_visible.frames
                ),
                (
                    ExactRatio::new(45, 2).unwrap(),
                    ExactRatio::new(51, 2).unwrap(),
                    range(22, 25)
                )
            );
            assert_eq!(p.fixed_visibility(), Some(after_visible));
            assert_eq!(
                p.boundary.as_ref().unwrap().exact_frame,
                ExactRatio::new(45, 2).unwrap()
            );
            let mut changed = 0;
            for outer in 0..3 {
                for inner in 0..2 {
                    let original = presented(outer, inner);
                    let mapped = p.map_instance(&d, &original).unwrap();
                    let affected = if outer_play { outer == 1 } else { inner == 1 };
                    changed += usize::from(affected);
                    let entry = 3 + 12 * i64::from(outer) + 6 * i64::from(inner);
                    assert_eq!(
                        boundary(&p.document, &mapped, 0),
                        ExactRatio::new(i128::from(2 * entry + 3), 2).unwrap()
                    );
                    assert_eq!(
                        boundary(&p.document, &mapped, if affected { 4 } else { 2 }),
                        ExactRatio::new(i128::from(2 * entry + if affected { 9 } else { 6 }), 2)
                            .unwrap()
                    );
                    assert_eq!(mapped.node == p.target.node, affected);
                    assert!(
                        matches!(&p.document.nodes()[&mapped.node].kind, NodeKind::Hold { recipe } if recipe.duration == frames(if affected { 4 } else { 2 }))
                    );
                    if !affected {
                        assert_eq!(mapped, original);
                    }
                }
            }
            assert_eq!(changed, if outer_play { 2 } else { 3 });
            for (id, beat) in d.nodes() {
                assert_eq!(&p.document.nodes()[id], beat);
            }
            reversible(&d, &request, &p);

            let owned = request_for(&p.document, p.target.clone(), 1, "mixed-owned-shrink");
            let Command::SetScopedHoldDuration { identities, .. } = &owned.command else {
                panic!()
            };
            assert!(identities.nodes.is_empty() && identities.marks.is_empty());
            let q =
                prepare_scoped_hold_duration(&p.document, &owned, p.presentation.as_ref()).unwrap();
            assert!(q.isolation().steps().is_empty());
            assert_eq!(q.document.nodes().len(), p.document.nodes().len());
            assert_eq!(q.document.overrides(), p.document.overrides());
            assert_eq!(q.document.gap_overrides(), p.document.gap_overrides());
            assert_eq!(q.fixed_visibility().unwrap().frames, range(22, 23));
            assert_eq!(
                q.fixed_visibility().unwrap().exact.end,
                ExactRatio::new(93, 4).unwrap()
            );
            reversible(&p.document, &owned, &q);
        }
    }
}

#[test]
fn mixed_fixed_duration_cropped_intrinsic_start_uses_visible_continuation_and_rejects_disappearance()
 {
    let d = fixed(3, 6, PitchPolicy::FollowSpeed);
    for outer_play in [false, true] {
        let target = selected(outer_play);
        let instance = presented(1, 1);
        let old = visible(&d, &target, Some(&instance)).unwrap();
        assert_eq!(
            (old.exact.start, old.exact.end, old.frames),
            (
                ExactRatio::integer(21),
                ExactRatio::new(111, 5).unwrap(),
                range(21, 22)
            )
        );
        let request = request_for(&d, target.clone(), 4, "mixed-cropped-grow");
        let p = prepare_scoped_hold_duration(&d, &request, Some(&instance)).unwrap();
        assert!(p.boundary.is_none());
        let v = p.fixed_visibility().unwrap();
        assert_eq!(
            (v.exact.start, v.exact.end, v.frames),
            (
                ExactRatio::integer(21),
                ExactRatio::new(123, 5).unwrap(),
                range(21, 25)
            )
        );
        reversible(&d, &request, &p);
        // A positive authored Hold ending exactly at crop In has no pictures.
        // The public command must refuse as well as the presentation wrapper.
        let losing = request_for(&d, target, 1, "mixed-cropped-disappear");
        let snapshot = d.to_json().unwrap();
        assert!(apply(&d, &losing).is_err());
        assert!(prepare_scoped_hold_duration(&d, &losing, Some(&instance)).is_err());
        assert_eq!(d.to_json().unwrap(), snapshot);
    }
}

#[test]
fn mixed_fixed_duration_exact_pools_forged_presentation_and_stale_requests_refuse_atomically() {
    let d = fixed(0, 6, PitchPolicy::FollowSpeed);
    let snapshot = d.to_json().unwrap();
    for outer_play in [false, true] {
        let target = selected(outer_play);
        let request = request_for(&d, target, 4, "mixed-pool-check");
        for extra in [false, true] {
            let mut invalid = request.clone();
            let Command::SetScopedHoldDuration { identities, .. } = &mut invalid.command else {
                panic!()
            };
            if extra {
                identities.nodes.push(node("too-many"));
            } else {
                identities.nodes.pop();
            }
            assert!(apply(&d, &invalid).is_err());
        }
        let wrong = if outer_play {
            presented(0, 1)
        } else {
            presented(1, 0)
        };
        assert!(prepare_scoped_hold_duration(&d, &request, Some(&wrong)).is_err());
        assert!(prepare_scoped_hold_duration(&d, &request, None).is_err());
        let mut stale = request;
        stale.expected_revision = revision("stale");
        assert!(prepare_scoped_hold_duration(&d, &stale, Some(&presented(1, 1))).is_err());
    }
    assert_eq!(d.to_json().unwrap(), snapshot);
}

#[test]
fn mixed_fixed_duration_dormant_default_has_no_invented_presentation() {
    for outer_play in [false, true] {
        let mut d = fixed(0, 6, PitchPolicy::FollowSpeed);
        let (repeat, count) = if outer_play {
            ("inner", 2)
        } else {
            ("outer", 3)
        };
        let mut overrides = Vec::new();
        for ordinal in 0..count {
            let name = format!("dormant-owned-{ordinal}");
            d.nodes.insert(
                node(&name),
                BeatNode::hold("Independent owned body", hold(4 + i64::from(ordinal))),
            );
            overrides.push(PlayOverride {
                iteration: play(repeat, ordinal),
                root: node(&name),
            });
        }
        d.overrides
            .insert(node(repeat), PlayOverrides::try_from(overrides).unwrap());
        d.validate().unwrap();
        let target = selected(outer_play);
        assert!(visible(&d, &target, None).is_none());
        let request = request_for(&d, target, 4, "mixed-dormant");
        let p = prepare_scoped_hold_duration(&d, &request, None).unwrap();
        assert!(p.presentation.is_none());
        assert!(p.boundary.is_none());
        assert!(p.fixed_visibility().is_none());
        assert_eq!(p.document.duration().unwrap(), d.duration().unwrap());
        assert_eq!(p.transaction.duration_delta, 0);
        for ordinal in 0..count {
            let id = node(&format!("dormant-owned-{ordinal}"));
            assert_eq!(p.document.nodes()[&id], d.nodes()[&id]);
        }
        reversible(&d, &request, &p);
        assert!(prepare_scoped_hold_duration(&d, &request, Some(&presented(1, 1))).is_err());
    }
}

#[test]
fn mixed_fixed_duration_partition_extra_stage_and_stage_between_repeats_remain_refused() {
    for bad in 0..3 {
        let mut d = fixed(0, 6, PitchPolicy::FollowSpeed);
        match bad {
            0 => {
                let NodeKind::Retime {
                    purpose,
                    duration,
                    mapping,
                    ..
                } = &mut d.nodes.get_mut(&node("stage")).unwrap().kind
                else {
                    panic!()
                };
                *purpose = RetimePurpose::Partition;
                *duration = mapping.duration();
            }
            1 => {
                let mut stage = d.nodes()[&node("stage")].clone();
                let NodeKind::Retime {
                    duration, mapping, ..
                } = &mut stage.kind
                else {
                    panic!()
                };
                *duration = frames(10);
                *mapping = range(0, 10);
                d.nodes.insert(node("extra"), stage);
                let NodeKind::Retime { child, .. } =
                    &mut d.nodes.get_mut(&node("stage")).unwrap().kind
                else {
                    panic!()
                };
                *child = node("extra");
            }
            2 => {
                let NodeKind::Repeat { child, .. } =
                    &mut d.nodes.get_mut(&node("outer")).unwrap().kind
                else {
                    panic!()
                };
                *child = node("between");
                let mut stage = d.nodes()[&node("stage")].clone();
                let NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    ..
                } = &mut stage.kind
                else {
                    panic!()
                };
                *child = node("inner");
                *duration = frames(12);
                *mapping = range(0, 12);
                d.nodes.insert(node("between"), stage);
            }
            _ => unreachable!(),
        }
        d.validate().unwrap();
        let snapshot = d.to_json().unwrap();
        for outer_play in [false, true] {
            assert!(
                d.scoped_hold_duration_requirements(&selected(outer_play), frames(4))
                    .is_err(),
                "ancestry {bad}"
            );
        }
        assert_eq!(d.to_json().unwrap(), snapshot);
    }
    for (crop, output) in [(4, 6), (0, 1)] {
        let d = fixed(crop, output, PitchPolicy::FollowSpeed);
        for outer_play in [false, true] {
            assert!(
                d.scoped_hold_duration_requirements(&selected(outer_play), frames(4))
                    .is_err()
            );
        }
    }
}

#[test]
fn mixed_fixed_duration_sparse_stable_iterations_use_physical_positions_and_owned_lengths() {
    let mut d = fixed(0, 6, PitchPolicy::FollowSpeed);
    for repeat in ["outer", "inner"] {
        let NodeKind::Repeat { iterations, .. } = &mut d.nodes.get_mut(&node(repeat)).unwrap().kind
        else {
            panic!()
        };
        *iterations = IterationOrder::from_segments([
            (revision(&format!("sparse-{repeat}")), 40, 2),
            (revision(repeat), 1, 1),
        ])
        .unwrap();
    }
    d.nodes.insert(
        node("earlier"),
        BeatNode::hold("Earlier owned body", hold(7)),
    );
    d.overrides.insert(
        node("outer"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: IterationId {
                allocation: revision("sparse-outer"),
                ordinal: 40,
            },
            root: node("earlier"),
        }])
        .unwrap(),
    );
    d.validate().unwrap();
    // Stable ordinal 1 now follows an owned 7-frame outer play, an ordinary
    // 18-frame outer play, and two 6-frame inner plays: 3 + 7 + 18 + 12 = 40.
    for outer_play in [false, true] {
        let target = selected(outer_play);
        let instance = presented(1, 1);
        let old = visible(&d, &target, Some(&instance)).unwrap();
        assert_eq!(
            (old.exact.start, old.exact.end, old.frames),
            (
                ExactRatio::new(83, 2).unwrap(),
                ExactRatio::integer(43),
                range(41, 43)
            )
        );
        let request = request_for(&d, target, 4, "mixed-sparse");
        let p = prepare_scoped_hold_duration(&d, &request, Some(&instance)).unwrap();
        assert_eq!(p.fixed_visibility().unwrap().frames, range(41, 44));
        assert_eq!(
            p.fixed_visibility().unwrap().exact.end,
            ExactRatio::new(89, 2).unwrap()
        );
        assert_eq!(
            p.document.nodes()[&node("earlier")],
            d.nodes()[&node("earlier")]
        );
        assert_eq!(p.document.duration().unwrap(), d.duration().unwrap());
        reversible(&d, &request, &p);
    }
}

#[test]
fn mixed_fixed_duration_owned_gap_stage_remains_outside_body_authority() {
    let mut d = fixed(0, 6, PitchPolicy::FollowSpeed);
    // A separately owned outer gap contains a real nested Repeat and fixed
    // stage. Its graph is valid, but it is not an outer body presentation.
    for name in ["inner", "stage", "body", "before", "held", "speech"] {
        let mut beat = d.nodes()[&node(name)].clone();
        match &mut beat.kind {
            NodeKind::Sequence { children } => {
                for child in children {
                    *child = node(&format!("gap-{child}"));
                }
            }
            NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
                *child = node(&format!("gap-{child}"));
            }
            NodeKind::Hold { .. } => {}
            NodeKind::Source { .. } => unreachable!(),
        }
        d.nodes.insert(node(&format!("gap-{name}")), beat);
    }
    d.gap_overrides.insert(
        node("outer"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play("outer", 1),
            root: node("gap-inner"),
        }])
        .unwrap(),
    );
    d.validate().unwrap();
    let target = ScopedNodeTarget {
        node: node("gap-held"),
        repeats: vec![
            RepeatEditStep {
                repeat: node("outer"),
                branch: RepeatEditBranch::Play {
                    iteration: play("outer", 1),
                },
            },
            RepeatEditStep {
                repeat: node("gap-inner"),
                branch: RepeatEditBranch::Default,
            },
        ],
    };
    target.validate(&d).unwrap();
    let snapshot = d.to_json().unwrap();
    assert!(
        d.scoped_hold_duration_requirements(&target, frames(4))
            .is_err()
    );
    assert_eq!(d.to_json().unwrap(), snapshot);
}

#[test]
fn mixed_fixed_duration_real_inserted_derived_clock_is_refused_before_isolation() {
    let mut d = ProjectDocument::new(
        ProjectId::new("mixed-fixed-derived").unwrap(),
        revision("derived-base"),
        crate::basis::default_basis(),
        node("root"),
    )
    .unwrap();
    let mut stage = BeatNode::sequence("Stage", vec![]);
    stage.kind = NodeKind::Retime {
        child: node("held"),
        duration: frames(24),
        mapping: range(0, 12),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    d.nodes.extend([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("stage")]),
        ),
        (node("stage"), stage),
        (node("held"), BeatNode::hold("Held", hold(12))),
    ]);
    d.validate().unwrap();
    let PauseInsertionTarget::Retimed(site) = d.pause_insertion_target(ProjectFrame(3)).unwrap()
    else {
        panic!()
    };
    let commit = |d: &ProjectDocument, command, label: &str| {
        apply_with_result(
            d,
            &CommandRequest {
                project_id: d.project_id().clone(),
                expected_revision: d.revision_id().clone(),
                new_revision: revision(label),
                command,
            },
        )
        .unwrap()
        .1
    };
    d = commit(
        &d,
        Command::InsertTime {
            at: ProjectFrame(3),
            hold: hold(3),
            id: node("inserted"),
            identities: SplitIdentities {
                nodes: (0..site.required_ids)
                    .map(|i| node(&format!("derived-split-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("derived-insert"),
                ordinal: 0,
            },
        },
        "derived-insert",
    );
    for (child, name) in [("stage", "inner"), ("inner", "outer")] {
        d = commit(
            &d,
            Command::WrapRepeat {
                node: node(child),
                id: node(name),
                plays: 3,
                gap: None,
                anchor_policy: Default::default(),
            },
            &format!("derived-wrap-{name}"),
        );
    }
    assert!(!d.derived_clocks().is_empty());
    let snapshot = d.to_json().unwrap();
    for outer_play in [false, true] {
        let mut target = selected(outer_play);
        target.node = node("inserted");
        for step in &mut target.repeats {
            if let RepeatEditBranch::Play { iteration } = &mut step.branch {
                let NodeKind::Repeat { iterations, .. } = &d.nodes()[&step.repeat].kind else {
                    panic!()
                };
                *iteration = iterations.at(1).unwrap();
            }
        }
        target.validate(&d).unwrap();
        assert!(
            d.scoped_hold_duration_requirements(&target, frames(4))
                .is_err()
        );
    }
    assert_eq!(d.to_json().unwrap(), snapshot);
}
