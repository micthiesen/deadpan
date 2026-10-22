//! One explicit body Play retains its fixed window and physical sample clocks.
use super::*;
use crate::{
    AudioTimingId, CommandRequest, HoldDurationVisibility, OccurrenceIdentities, PitchPolicy,
    RetimePurpose,
};

#[path = "nested_fixed_retime_duration.rs"]
mod nested;

fn plan(
    d: &ProjectDocument,
    c: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    allocate: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic(
        d,
        c,
        &program(instructions),
        SemanticRegisterBank {
            entries: bank,
            version: 3,
        },
        revision(&format!("{}-fixed-next", d.revision_id())),
        allocate,
        no_original,
    )
}

fn fixed(pitch: PitchPolicy, crop: i64, output: i64) -> ProjectDocument {
    let mut stage = BeatNode::sequence("Fixed", vec![]);
    stage.kind = NodeKind::Retime {
        child: node("body"),
        duration: FrameDuration::new(output).unwrap(),
        mapping: range(crop, 8),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    tree(
        &["lead", "outer", "tail"],
        vec![
            ("lead", hold(3)),
            ("outer", repeated("outer", "stage", 3)),
            ("stage", stage),
            (
                "body",
                BeatNode::sequence("Body", vec![node("before"), node("held"), node("speech")]),
            ),
            ("before", hold(2)),
            ("held", hold(2)),
            ("speech", hold(6)),
            ("tail", hold(3)),
        ],
    )
}

fn entry_fixed(d: &ProjectDocument, ordinal: u32) -> SemanticContext {
    let NodeKind::Repeat { iterations, .. } = &d.nodes()[&node("outer")].kind else {
        panic!()
    };
    let iteration = iterations.at(ordinal).unwrap();
    SemanticContext {
        selected_child: Some(node("outer")),
        scoped: Some(SemanticScopedContext {
            root: node("outer"),
            target: ScopedNodeTarget {
                node: node("held"),
                repeats: vec![RepeatEditStep {
                    repeat: node("outer"),
                    branch: RepeatEditBranch::Play {
                        iteration: iteration.clone(),
                    },
                }],
            },
            presentation: InstancePath {
                node: node("held"),
                repeats: vec![RepeatInstance {
                    node: node("outer"),
                    iteration,
                }],
            },
        }),
        ..context("root", 10)
    }
}

fn visible(d: &ProjectDocument, c: &SemanticContext) -> HoldDurationVisibility {
    let scoped = c.scoped.as_ref().unwrap();
    d.fixed_occurrence_hold_duration_visibility(&c.parent, &scoped.root, &scoped.presentation)
        .unwrap()
}

fn request(d: &ProjectDocument, instance: &InstancePath, length: i64) -> CommandRequest {
    let duration = FrameDuration::new(length).unwrap();
    let needs = d
        .occurrence_hold_duration_requirements(instance, duration)
        .unwrap();
    CommandRequest {
        project_id: d.project_id().clone(),
        expected_revision: d.revision_id().clone(),
        new_revision: revision("fixed-occurrence-leaf"),
        command: Command::EditOccurrence {
            instance: instance.clone(),
            edit: crate::OccurrenceEdit::SetHoldDuration { duration },
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| node(&format!("fixed-occurrence-copy-{index}")))
                    .collect(),
                marks: (0..needs.marks)
                    .map(|index| MarkId::new(format!("fixed-occurrence-mark-{index}")).unwrap())
                    .collect(),
            },
        },
    }
}

fn reversible(d: &ProjectDocument, p: &SemanticPlan) {
    let replay =
        crate::replay_compound::<EditError>(d, p.request.as_ref().unwrap(), |_| Ok(())).unwrap();
    assert_eq!(replay.document, p.document);
    assert_eq!(replay.edit.inverse.apply(&p.document).unwrap(), *d);
    assert_eq!(
        ProjectDocument::from_json(&p.document.to_json().unwrap()).unwrap(),
        p.document
    );
}

#[test]
fn fixed_occurrence_duration_fresh_pools_affine_visibility_all_pitches_and_inverse() {
    for pitch in [
        PitchPolicy::FollowSpeed,
        PitchPolicy::Preserve,
        PitchPolicy::Shift { semitones: 3 },
    ] {
        let d = fixed(pitch, 0, 6);
        for ordinal in [0, 1, 2] {
            let c = entry_fixed(&d, ordinal);
            let instance = &c.scoped.as_ref().unwrap().presentation;
            let old = visible(&d, &c);
            let offset = 3 + 6 * i64::from(ordinal);
            assert_eq!(
                old.exact.start,
                ExactRatio::integer(offset)
                    .checked_add(ExactRatio::new(3, 2).unwrap())
                    .unwrap()
            );
            assert_eq!(old.frames, range(offset + 1, offset + 3));
            for length in [1, 4] {
                let needs = d
                    .occurrence_hold_duration_requirements(
                        instance,
                        FrameDuration::new(length).unwrap(),
                    )
                    .unwrap();
                assert_eq!((needs.nodes, needs.marks), (5, 0));
                let mut allocations = vec![];
                let p = plan(
                    &d,
                    &c,
                    vec![duration(u32::try_from(length).unwrap())],
                    &BTreeMap::new(),
                    |r| {
                        allocations.push(r.clone());
                        super::super::super::allocate(r)
                    },
                )
                .unwrap();
                assert_eq!(
                    allocations,
                    [SemanticAllocationRequest::Isolation {
                        step_index: 0,
                        nodes: 5,
                        marks: 0
                    }]
                );
                assert_eq!(p.document.duration().unwrap().frames(), 24);
                assert_eq!(p.document.nodes().len(), d.nodes().len() + 5);
                assert_eq!(p.document.overrides()[&node("outer")].len(), 1);
                assert_eq!(p.document.gap_overrides(), d.gap_overrides());
                for (id, original) in d.nodes() {
                    assert_eq!(&p.document.nodes()[id], original);
                }
                let current = visible(&p.document, &p.context);
                assert_eq!(current.exact.start, old.exact.start);
                assert_eq!(
                    current.frames,
                    range(offset + 1, offset + if length == 1 { 2 } else { 4 })
                );
                assert_eq!(p.context.cursor, ProjectFrame(offset + 1));
                assert_eq!(p.trace[0].resolved_range, Some(current.frames));
                let Command::Compound { transaction } = &p.request.as_ref().unwrap().command else {
                    panic!()
                };
                let leaf = transaction.steps()[0].edit().unwrap().request(&d);
                assert!(matches!(
                    &leaf.command,
                    Command::EditOccurrence {
                        edit: crate::OccurrenceEdit::SetHoldDuration { .. },
                        ..
                    }
                ));
                let prepared = crate::prepare_occurrence_hold_duration(&d, &leaf).unwrap();
                assert_eq!(prepared.fixed_visibility(), Some(current));
                assert_eq!(
                    prepared.boundary.as_ref().unwrap().exact_frame,
                    old.exact.start
                );
                assert_eq!(
                    prepared.presentation,
                    p.context.scoped.as_ref().unwrap().presentation
                );
                prepared
                    .isolation()
                    .validate(&d, &leaf, &prepared.document)
                    .unwrap();
                let mut expected_leaf = p.document.clone();
                expected_leaf.revision_id = leaf.new_revision.clone();
                assert_eq!(prepared.document, expected_leaf);
                reversible(&d, &p);
            }
        }
    }
}

#[test]
fn fixed_occurrence_duration_owned_and_staged_setters_allocate_zero_nodes() {
    let d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    let c = entry_fixed(&d, 1);
    let p = plan(
        &d,
        &c,
        vec![duration(4)],
        &BTreeMap::new(),
        super::super::super::allocate,
    )
    .unwrap();
    let owned = p.document.clone();
    let owned_context = p.context.clone();
    let mut calls = vec![];
    let bank = BTreeMap::from([(name('a'), macro_value(vec![duration(3), duration(4)]))]);
    let next = plan(&owned, &owned_context, vec![call('a', 2)], &bank, |r| {
        calls.push(r.clone());
        super::super::allocator("fixed-owned-next", r)
    })
    .unwrap();
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|r| matches!(
        r,
        SemanticAllocationRequest::Isolation {
            nodes: 0,
            marks: 0,
            ..
        }
    )));
    assert_eq!(next.document.nodes().len(), owned.nodes().len());
    assert_eq!(next.context.scoped, owned_context.scoped);
    assert_eq!(next.context.cursor, ProjectFrame(10));
    reversible(&owned, &next);

    let mut calls = vec![];
    let staged = plan(
        &d,
        &c,
        vec![duration(4), duration(1)],
        &BTreeMap::new(),
        |r| {
            calls.push(r.clone());
            super::super::super::allocate(r)
        },
    )
    .unwrap();
    assert!(matches!(
        calls[0],
        SemanticAllocationRequest::Isolation {
            nodes: 5,
            marks: 0,
            ..
        }
    ));
    assert!(matches!(
        calls[1],
        SemanticAllocationRequest::Isolation {
            nodes: 0,
            marks: 0,
            ..
        }
    ));
    assert_eq!(
        visible(&staged.document, &staged.context).frames,
        range(10, 11)
    );
    reversible(&d, &staged);
}

#[test]
fn fixed_occurrence_duration_crop_and_intrinsic_boundary_remain_distinct() {
    for (crop, exact_start, frames) in [
        (1, ExactRatio::new(69, 7).unwrap(), range(10, 13)),
        (3, ExactRatio::integer(9), range(9, 13)),
    ] {
        let d = fixed(PitchPolicy::FollowSpeed, crop, 6);
        let c = entry_fixed(&d, 1);
        let p = plan(
            &d,
            &c,
            vec![duration(4)],
            &BTreeMap::new(),
            super::super::super::allocate,
        )
        .unwrap();
        let current = visible(&p.document, &p.context);
        assert_eq!(current.exact.start, exact_start);
        assert_eq!(current.frames, frames);
        assert_eq!(p.context.cursor, frames.start());
        let Command::Compound { transaction } = &p.request.as_ref().unwrap().command else {
            panic!()
        };
        let prepared = crate::prepare_occurrence_hold_duration(
            &d,
            &transaction.steps()[0].edit().unwrap().request(&d),
        )
        .unwrap();
        if crop == 3 {
            assert!(prepared.boundary.is_none());
        } else {
            assert_eq!(
                prepared.boundary.as_ref().unwrap().exact_frame,
                current.exact.start
            );
        }
        reversible(&d, &p);
    }
}

#[test]
fn fixed_occurrence_duration_exact_pools_stale_and_wrong_root_refuse() {
    let d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    let c = entry_fixed(&d, 1);
    let instance = &c.scoped.as_ref().unwrap().presentation;
    let good = request(&d, instance, 4);
    for bad in 0..6 {
        let mut wrong = good.clone();
        let Command::EditOccurrence { identities, .. } = &mut wrong.command else {
            panic!()
        };
        match bad {
            0 => {
                identities.nodes.pop();
            }
            1 => identities.nodes.push(node("extra-fixed-copy")),
            2 => identities
                .marks
                .push(MarkId::new("extra-fixed-mark").unwrap()),
            3 => identities.nodes[0] = node("held"),
            4 => wrong.expected_revision = revision("stale-fixed-head"),
            5 => identities.nodes[1] = identities.nodes[0].clone(),
            _ => unreachable!(),
        }
        assert!(crate::apply(&d, &wrong).is_err(), "bad pool/stale {bad}");
        assert!(crate::prepare_occurrence_hold_duration(&d, &wrong).is_err());
    }
    let mut wrong = c.clone();
    wrong.scoped.as_mut().unwrap().root = node("stage");
    assert!(
        plan(&d, &wrong, vec![duration(4)], &BTreeMap::new(), |_| panic!(
            "wrong root allocated"
        ))
        .is_err()
    );
    assert!(
        d.fixed_occurrence_hold_duration_visibility(&node("body"), &node("outer"), instance)
            .is_err()
    );
    assert!(
        d.fixed_occurrence_hold_duration_visibility(&node("root"), &node("stage"), instance)
            .is_err()
    );
}

#[test]
fn fixed_occurrence_duration_unsupported_ancestry_keeps_atomic_refusals() {
    let d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    for bad in 0..5 {
        let mut invalid = d.clone();
        let mut c = entry_fixed(&invalid, 1);
        match bad {
            0 => {
                let NodeKind::Retime {
                    purpose,
                    duration,
                    mapping,
                    ..
                } = &mut invalid.nodes.get_mut(&node("stage")).unwrap().kind
                else {
                    panic!()
                };
                *purpose = RetimePurpose::Partition;
                *duration = mapping.duration();
            }
            1 => {
                invalid
                    .nodes
                    .insert(node("inner"), repeated("inner", "body", 2));
                let NodeKind::Retime { child, .. } =
                    &mut invalid.nodes.get_mut(&node("stage")).unwrap().kind
                else {
                    panic!()
                };
                *child = node("inner");
                let scoped = c.scoped.as_mut().unwrap();
                scoped.target.repeats.push(RepeatEditStep {
                    repeat: node("inner"),
                    branch: RepeatEditBranch::Play {
                        iteration: iteration("inner", 0),
                    },
                });
                scoped.presentation.repeats.push(RepeatInstance {
                    node: node("inner"),
                    iteration: iteration("inner", 0),
                });
            }
            2 => {
                let mut outer_stage = invalid.nodes()[&node("stage")].clone();
                let NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    ..
                } = &mut outer_stage.kind
                else {
                    panic!()
                };
                *child = node("outer");
                *duration = FrameDuration::new(18).unwrap();
                *mapping = range(0, 18);
                invalid.nodes.insert(node("above"), outer_stage);
                let NodeKind::Sequence { children } =
                    &mut invalid.nodes.get_mut(&node("root")).unwrap().kind
                else {
                    panic!()
                };
                children[1] = node("above");
            }
            3 => {
                let mut another = invalid.nodes()[&node("stage")].clone();
                let NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    ..
                } = &mut another.kind
                else {
                    panic!()
                };
                *child = node("body");
                *duration = FrameDuration::new(10).unwrap();
                *mapping = range(0, 10);
                invalid.nodes.insert(node("another"), another);
                let NodeKind::Retime { child, .. } =
                    &mut invalid.nodes.get_mut(&node("stage")).unwrap().kind
                else {
                    panic!()
                };
                *child = node("another");
            }
            4 => {
                let mut gap = invalid.nodes()[&node("stage")].clone();
                let NodeKind::Retime { child, .. } = &mut gap.kind else {
                    panic!()
                };
                *child = node("gap-held");
                invalid.nodes.insert(node("gap-stage"), gap);
                invalid.nodes.insert(node("gap-held"), hold(10));
                invalid.gap_overrides.insert(
                    node("outer"),
                    crate::PlayOverrides::try_from(vec![crate::PlayOverride {
                        iteration: iteration("outer", 1),
                        root: node("gap-stage"),
                    }])
                    .unwrap(),
                );
                let scoped = c.scoped.as_mut().unwrap();
                scoped.target.node = node("gap-held");
                scoped.presentation.node = node("gap-held");
            }
            _ => unreachable!(),
        }
        invalid.validate().unwrap();
        let snapshot = invalid.to_json().unwrap();
        assert!(
            plan(
                &invalid,
                &c,
                vec![duration(4)],
                &BTreeMap::new(),
                |_| panic!("unsupported ancestry allocated")
            )
            .is_err(),
            "ancestry {bad}"
        );
        assert_eq!(invalid.to_json().unwrap(), snapshot);
    }
}

#[test]
fn fixed_occurrence_duration_hidden_unsampled_and_staged_disappearance_rollback() {
    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 1);
    let NodeKind::Hold { recipe } = &mut d.nodes.get_mut(&node("before")).unwrap().kind else {
        panic!()
    };
    recipe.duration = FrameDuration::new(4).unwrap();
    let NodeKind::Retime { mapping, .. } = &mut d.nodes.get_mut(&node("stage")).unwrap().kind
    else {
        panic!()
    };
    *mapping = range(0, 10);
    d.validate().unwrap();
    let mut c = entry_fixed(&d, 1);
    assert_eq!(visible(&d, &c).frames, range(4, 5));
    c.cursor = ProjectFrame(4);
    let snapshot = d.to_json().unwrap();
    let error = plan(
        &d,
        &c,
        vec![duration(4), duration(1)],
        &BTreeMap::new(),
        super::super::super::allocate,
    )
    .unwrap_err();
    assert!(error.message.contains("no visible sampled"), "{error}");
    assert_eq!(d.to_json().unwrap(), snapshot);
    let r = request(&d, &c.scoped.as_ref().unwrap().presentation, 1);
    assert!(crate::apply(&d, &r).is_err());
    assert!(crate::prepare_occurrence_hold_duration(&d, &r).is_err());
    for hidden in [true, false] {
        let mut invalid = d.clone();
        if hidden {
            let NodeKind::Retime { mapping, .. } =
                &mut invalid.nodes.get_mut(&node("stage")).unwrap().kind
            else {
                panic!()
            };
            *mapping = range(0, 3);
        } else {
            let NodeKind::Hold { recipe } = &mut invalid.nodes.get_mut(&node("held")).unwrap().kind
            else {
                panic!()
            };
            recipe.duration = FrameDuration::new(1).unwrap();
        }
        invalid.validate().unwrap();
        assert!(
            plan(
                &invalid,
                &c,
                vec![duration(4)],
                &BTreeMap::new(),
                |_| panic!("hidden target allocated")
            )
            .is_err()
        );
    }
}

#[test]
fn fixed_occurrence_duration_shrink_cannot_rewrite_retained_input_end() {
    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    let NodeKind::Retime { mapping, .. } = &mut d.nodes.get_mut(&node("stage")).unwrap().kind
    else {
        panic!()
    };
    *mapping = range(0, 10);
    d.validate().unwrap();
    let c = entry_fixed(&d, 1);
    let snapshot = d.to_json().unwrap();
    let r = request(&d, &c.scoped.as_ref().unwrap().presentation, 1);
    assert!(crate::apply(&d, &r).is_err());
    assert!(crate::prepare_occurrence_hold_duration(&d, &r).is_err());
    assert!(
        plan(
            &d,
            &c,
            vec![duration(4), duration(1)],
            &BTreeMap::new(),
            super::super::super::allocate
        )
        .is_err()
    );
    assert_eq!(d.to_json().unwrap(), snapshot);
}

#[test]
fn fixed_occurrence_duration_derived_clock_refuses_before_isolation() {
    let mut stage = BeatNode::sequence("Stage", vec![]);
    stage.kind = NodeKind::Retime {
        child: node("held"),
        duration: FrameDuration::new(24).unwrap(),
        mapping: range(0, 12),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let d = tree(&["stage"], vec![("stage", stage), ("held", hold(12))]);
    let crate::PauseInsertionTarget::Retimed(site) =
        d.pause_insertion_target(ProjectFrame(3)).unwrap()
    else {
        panic!()
    };
    let commit = |d: &ProjectDocument, command, label: &str| {
        crate::apply_with_result(
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
    let NodeKind::Hold { recipe } = hold(3).kind else {
        panic!()
    };
    let d = commit(
        &d,
        Command::InsertTime {
            at: ProjectFrame(3),
            hold: recipe,
            id: node("inserted"),
            identities: SplitIdentities {
                nodes: (0..site.required_ids)
                    .map(|i| node(&format!("fixed-derived-split-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("fixed-derived-insert"),
                ordinal: 0,
            },
        },
        "fixed-derived-insert",
    );
    let d = commit(
        &d,
        Command::WrapRepeat {
            node: node("stage"),
            id: node("outer"),
            plays: 3,
            gap: None,
            anchor_policy: Default::default(),
        },
        "fixed-derived-wrap",
    );
    assert!(!d.derived_clocks().is_empty());
    let mut c = entry_fixed(&d, 1);
    let scoped = c.scoped.as_mut().unwrap();
    scoped.target.node = node("inserted");
    scoped.presentation.node = node("inserted");
    let snapshot = d.to_json().unwrap();
    assert!(
        plan(&d, &c, vec![duration(4)], &BTreeMap::new(), |_| panic!(
            "derived clock allocated"
        ))
        .is_err()
    );
    assert_eq!(d.to_json().unwrap(), snapshot);
}

#[test]
fn fixed_occurrence_duration_preserves_source_recipe_lattice_after_isolation() {
    let mut d = fixed(PitchPolicy::FollowSpeed, 0, 6);
    d.presentation_basis.frame_rate = crate::FrameRate::new(30_000, 1001).unwrap();
    let time_base = crate::SourceTimeBase::new(1, 44_100).unwrap();
    let span = crate::SourceSpan::new(
        crate::SourceTimestamp {
            ticks: 3100,
            time_base,
        },
        crate::SourceTimestamp {
            ticks: 23_100,
            time_base,
        },
    )
    .unwrap();
    let audio = crate::SourceAudio {
        asset: crate::AssetId::new("fixed-duration-audio").unwrap(),
        span,
    };
    d.assets.insert(
        audio.asset.clone(),
        crate::AssetRecord {
            label: "Qualified audio coordinates".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(crate::SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    let mut speech = hold(6);
    speech.kind = NodeKind::Source {
        source: crate::SourceNode {
            duration: FrameDuration::new(6).unwrap(),
            edit_window: None,
            video: crate::SourceVideo::Blank,
            video_mapping: crate::SourceVideoMapping::FitBeat,
            audio: Some(audio),
            audio_mapping: crate::SourceAudioMapping::natural_rate(
                span,
                d.presentation_basis.frame_rate,
            )
            .unwrap(),
            link: crate::LinkRelation::Independent,
            audio_offset: crate::AudioSample(0),
        },
    };
    d.nodes.insert(node("speech"), speech);
    d.validate().unwrap();
    let c = entry_fixed(&d, 1);
    let old_instance = InstancePath {
        node: node("speech"),
        ..c.scoped.as_ref().unwrap().presentation.clone()
    };
    let old = crate::capture_unbound_audio_bindings(
        &d,
        AudioTimingId {
            allocation: revision("fixed-old-physical-clock"),
            ordinal: 0,
        },
    )
    .unwrap()
    .resolve(&old_instance.node, &old_instance, crate::MAX_DOCUMENT_NODES)
    .unwrap()
    .lattice;
    let r = request(&d, &c.scoped.as_ref().unwrap().presentation, 4);
    let prepared = crate::prepare_occurrence_hold_duration(&d, &r).unwrap();
    let mapped = prepared.map_instance(&d, &old_instance).unwrap();
    let current = prepared
        .document
        .audio_bindings()
        .resolve(&mapped.node, &mapped, crate::MAX_DOCUMENT_NODES)
        .unwrap()
        .lattice;
    assert_eq!(current.origin, ExactRatio::integer(12));
    assert_eq!(current.origin, old.origin);
    assert_eq!(current.frames_per_local_frame, old.frames_per_local_frame);
    assert_eq!(current.grid_origin, old.grid_origin);
    assert_eq!(current.frames_per_sample, old.frames_per_sample);
    assert_eq!(current.local_support, old.local_support);
    let live = crate::FrozenAudioLayout::capture(&prepared.document)
        .unwrap()
        .project(&mapped, ExactRatio::ZERO, None, crate::MAX_DOCUMENT_NODES)
        .unwrap();
    assert_eq!(live.origin, ExactRatio::new(27, 2).unwrap());
    assert_eq!(
        prepared
            .transaction
            .inverse
            .apply(&prepared.document)
            .unwrap(),
        d
    );
}
