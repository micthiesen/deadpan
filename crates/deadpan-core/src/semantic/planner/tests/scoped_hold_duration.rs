use super::*;

#[path = "scoped_hold_duration/fixed_retime_duration.rs"]
mod fixed_retime_duration;

fn duration(frames: u32) -> SemanticInstruction {
    SemanticInstruction::SetHoldDuration {
        length: crate::PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn plan(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    allocator: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_framing(
        document,
        context,
        &program(instructions),
        SemanticRegisterBank {
            entries: bank,
            version: 3,
        },
        revision("duration-outer"),
        allocator,
        no_original,
        |_| Err(speech_unavailable()),
        |_, _| panic!("duration/black sampled provider"),
        None,
        state,
    )
}

#[test]
fn scoped_hold_duration_grow_shrink_first_last_plays_map_actual_hold_and_exact_start_with_one_inverse()
 {
    let before = fixture();
    for outer in [0, 2] {
        for inner in [0, 1] {
            for frames in [2, 6] {
                let mut context = entry();
                let within = presentation(outer, inner);
                let scoped = context.scoped.as_mut().unwrap();
                scoped.presentation = within.clone();
                scoped.target.repeats = within
                    .repeats
                    .iter()
                    .map(|step| RepeatEditStep {
                        repeat: step.node.clone(),
                        branch: RepeatEditBranch::Play {
                            iteration: step.iteration.clone(),
                        },
                    })
                    .collect();
                let start = 18 * i64::from(outer) + 2 + 7 * i64::from(inner);
                context.cursor = ProjectFrame(start + 3);
                let plan = plan(
                    &before,
                    &context,
                    vec![duration(frames)],
                    &BTreeMap::new(),
                    super::super::allocate,
                )
                .unwrap();
                assert_eq!(
                    plan.document.duration().unwrap().frames(),
                    54 + i64::from(frames) - 4
                );
                assert_eq!(plan.context.cursor, ProjectFrame(start));
                assert_eq!(plan.context.parent, context.parent);
                assert_eq!(plan.context.selected_child, context.selected_child);
                let mapped = plan.context.scoped.as_ref().unwrap();
                assert!(
                    mapped
                        .target
                        .matches_instance(&plan.document, &mapped.presentation)
                        .unwrap()
                );
                assert_ne!(mapped.target.node, node("a"));
                assert_eq!(
                    plan.document
                        .node_duration(&mapped.target.node)
                        .unwrap()
                        .frames(),
                    i64::from(frames)
                );
                assert_eq!(
                    plan.document.nodes()[&node("a")],
                    before.nodes()[&node("a")]
                );
                let request = plan.request.as_ref().unwrap();
                let replay =
                    crate::replay_compound::<EditError>(&before, request, |_| Ok(())).unwrap();
                assert_eq!(replay.document, plan.document);
                assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), before);
                assert_eq!(
                    ProjectDocument::from_json(&plan.document.to_json().unwrap()).unwrap(),
                    plan.document
                );
                let Command::Compound { transaction } = &request.command else {
                    panic!()
                };
                let leaf = transaction.steps()[0].edit().unwrap();
                let prepared =
                    crate::prepare_occurrence_hold_duration(&before, &leaf.request(&before))
                        .unwrap();
                assert_eq!(prepared.target, mapped.target);
                assert_eq!(prepared.presentation, mapped.presentation);
                assert_eq!(
                    prepared.boundary.as_ref().unwrap().frame,
                    ProjectFrame(start)
                );
                prepared
                    .isolation()
                    .validate(&before, &leaf.request(&before), &prepared.document)
                    .unwrap();
                assert!(
                    prepared
                        .map_target(&prepared.document, &context.scoped.as_ref().unwrap().target)
                        .is_err()
                );
                assert!(prepared.map_instance(&prepared.document, &within).is_err());
            }
        }
    }
}

#[test]
fn scoped_hold_duration_reuses_existing_override_and_maps_duration_zoom_pause_nested_calls() {
    let original = fixture();
    let framed = resolve(
        &original,
        &entry(),
        vec![zoom(2)],
        &BTreeMap::new(),
        |request| allocator("owned", request),
    )
    .unwrap();
    let before = framed.document;
    let mut context = framed.context;
    context.cursor = ProjectFrame(30);
    let mut allocations = Vec::new();
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![duration(6), zoom(3)])),
    ]);
    let plan = plan(
        &before,
        &context,
        vec![
            call('a', 1),
            SemanticInstruction::InsertPause {
                length: crate::PauseLength::Frames {
                    frames: NonZeroU32::new(2).unwrap(),
                },
                black: true,
            },
        ],
        &bank,
        |request| {
            allocations.push(request.clone());
            super::super::allocate(request)
        },
    )
    .unwrap();
    assert!(matches!(
        allocations[0],
        SemanticAllocationRequest::Isolation {
            nodes: 0,
            marks: 0,
            ..
        }
    ));
    assert!(matches!(
        allocations[1],
        SemanticAllocationRequest::Isolation {
            nodes: 0,
            marks: 0,
            ..
        }
    ));
    assert_eq!(plan.context.cursor, ProjectFrame(27));
    assert_eq!(
        plan.context.scoped.as_ref().unwrap().target.node,
        node("pause-2")
    );
    assert_eq!(
        plan.document.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 4
    );
    let duration = plan
        .trace
        .iter()
        .find(|row| matches!(row.instruction, SemanticInstruction::SetHoldDuration { .. }))
        .unwrap();
    let zoom = plan
        .trace
        .iter()
        .find(|row| matches!(row.instruction, SemanticInstruction::Zoom { .. }))
        .unwrap();
    assert_eq!(duration.after.scoped, zoom.before.scoped);
    assert_eq!(
        bank[&name('b')],
        macro_value(vec![self::duration(6), self::zoom(3)])
    );
    let replay =
        crate::replay_compound::<EditError>(&before, plan.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), before);
}

#[test]
fn scoped_hold_duration_noop_mixed_nonhold_and_retime_fail_before_allocation() {
    let before = fixture();
    assert!(
        plan(
            &before,
            &entry(),
            vec![duration(4)],
            &BTreeMap::new(),
            |_| panic!("noop allocated")
        )
        .unwrap_err()
        .message
        .contains("already has that duration")
    );
    let within = presentation(1, 1);
    assert!(
        before
            .occurrence_hold_duration_requirements(&within, FrameDuration::ZERO)
            .is_err()
    );
    let mut mixed = entry();
    mixed.scoped.as_mut().unwrap().target.repeats[0].branch = RepeatEditBranch::Default;
    assert!(
        plan(
            &before,
            &mixed,
            vec![duration(6)],
            &BTreeMap::new(),
            |_| panic!("mixed allocated")
        )
        .is_err()
    );
    let mut group = entry();
    group.scoped.as_mut().unwrap().target.node = node("inside");
    group.scoped.as_mut().unwrap().presentation.node = node("inside");
    assert_eq!(
        plan(
            &before,
            &group,
            vec![duration(6)],
            &BTreeMap::new(),
            |_| panic!("group allocated")
        )
        .unwrap_err()
        .code,
        EditErrorCode::WrongNodeKind
    );
    let mut retimed = before.clone();
    let child = retimed.nodes.get_mut(&node("inside")).unwrap();
    let NodeKind::Sequence { children } = &mut child.kind else {
        panic!()
    };
    children[0] = node("retime");
    let mut wrapper = BeatNode::sequence("Retime", vec![]);
    wrapper.kind = NodeKind::Retime {
        child: node("a"),
        duration: FrameDuration::new(4).unwrap(),
        mapping: range(0, 4),
        pitch: crate::PitchPolicy::FollowSpeed,
        purpose: crate::RetimePurpose::Edit,
    };
    retimed.nodes.insert(node("retime"), wrapper);
    retimed.validate().unwrap();
    assert!(
        plan(
            &retimed,
            &entry(),
            vec![duration(6)],
            &BTreeMap::new(),
            |_| panic!("Retime allocated")
        )
        .unwrap_err()
        .message
        .contains("Retime")
    );
    // The raw occurrence wrapper also refuses this unqualified clock, rather
    // than applying an integral root-suffix translation beneath the Retime.
    let target = entry().scoped.unwrap().target;
    let needs = retimed
        .scoped_edit_requirements(
            &target,
            &crate::ScopedNodeEdit::Rename {
                label: "resized".into(),
            },
        )
        .unwrap();
    let SemanticAllocation::Isolation {
        new_revision,
        identities,
    } = super::super::allocate(SemanticAllocationRequest::Isolation {
        step_index: 0,
        nodes: needs.nodes,
        marks: needs.marks,
    })
    .unwrap()
    else {
        panic!()
    };
    let error = crate::apply(
        &retimed,
        &CommandRequest {
            project_id: retimed.project_id().clone(),
            expected_revision: retimed.revision_id().clone(),
            new_revision,
            command: Command::EditOccurrence {
                instance: within,
                edit: crate::OccurrenceEdit::SetHoldDuration {
                    duration: FrameDuration::new(6).unwrap(),
                },
                identities,
            },
        },
    )
    .unwrap_err();
    assert!(error.message.contains("Retime"), "{error}");
}

#[test]
fn scoped_hold_duration_exact_pools_late_failure_and_saved_intent_leave_no_partial_result() {
    let before = fixture();
    for bad in 0..6 {
        let result = plan(
            &before,
            &entry(),
            vec![duration(6)],
            &BTreeMap::new(),
            |request| {
                let mut allocation = super::super::allocate(request)?;
                let SemanticAllocation::Isolation {
                    new_revision,
                    identities,
                } = &mut allocation
                else {
                    panic!()
                };
                match bad {
                    0 => {
                        identities.nodes.pop();
                    }
                    1 => identities.nodes.push(node("extra")),
                    2 => {
                        identities.marks.pop();
                    }
                    3 => identities.marks.push(MarkId::new("extra").unwrap()),
                    4 => identities.nodes[0] = node("a"),
                    5 => *new_revision = before.revision_id().clone(),
                    _ => unreachable!(),
                }
                Ok(allocation)
            },
        );
        assert!(result.is_err(), "case{bad}");
    }
    let bank = BTreeMap::from([(name('a'), macro_value(vec![SemanticInstruction::Split]))]);
    let result = plan(
        &before,
        &entry(),
        vec![duration(6), call('a', 1)],
        &bank,
        super::super::allocate,
    );
    assert!(result.unwrap_err().message.contains("only Zoom"));
    assert_eq!(before, fixture());
    let wire = serde_json::to_string(&program(vec![duration(6)])).unwrap();
    for forbidden in ["target", "presentation", "outer", "inner", "instance"] {
        assert!(!wire.contains(forbidden));
    }
}

#[test]
fn scoped_hold_duration_retains_old_suffix_lattices_and_captures_partially_unbound_root_tail() {
    for partially_bound in [false, true] {
        let mut before = fixture();
        before.presentation_basis.frame_rate = crate::FrameRate::new(30_000, 1001).unwrap();
        let NodeKind::Sequence { children } =
            &mut before.nodes.get_mut(&node("root")).unwrap().kind
        else {
            panic!()
        };
        children.push(node("root-tail"));
        before.nodes.insert(node("root-tail"), hold(3));
        if partially_bound {
            before.audio_bindings = crate::capture_unbound_audio_bindings(
                &before,
                crate::AudioTimingId {
                    allocation: revision("old-clock"),
                    ordinal: 0,
                },
            )
            .unwrap();
            before.audio_bindings.bindings.remove(&node("root-tail"));
        }
        before.validate().unwrap();
        for (outer, inner) in [(0, 0), (2, 1)] {
            let mut context = entry();
            context.scoped.as_mut().unwrap().presentation = presentation(outer, inner);
            context.scoped.as_mut().unwrap().target.repeats = presentation(outer, inner)
                .repeats
                .into_iter()
                .map(|step| RepeatEditStep {
                    repeat: step.node,
                    branch: RepeatEditBranch::Play {
                        iteration: step.iteration,
                    },
                })
                .collect();
            let start = 18 * i64::from(outer) + 2 + 7 * i64::from(inner);
            context.cursor = ProjectFrame(start + 1);
            for frames in [2, 6] {
                let plan = plan(
                    &before,
                    &context,
                    vec![duration(frames)],
                    &BTreeMap::new(),
                    super::super::allocate,
                )
                .unwrap();
                let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command
                else {
                    panic!()
                };
                let prepared = crate::prepare_occurrence_hold_duration(
                    &before,
                    &transaction.steps()[0].edit().unwrap().request(&before),
                )
                .unwrap();
                let mut suffix = presentation(outer, inner);
                suffix.node = node("b");
                let mut moved = vec![(suffix, start + 4)];
                if (outer, inner) == (0, 0) {
                    moved.push((presentation(0, 1), 9));
                    moved.push((presentation(2, 0), 38));
                }
                moved.push((
                    InstancePath {
                        node: node("root-tail"),
                        repeats: vec![],
                    },
                    54,
                ));
                for (old, old_start) in moved {
                    let current = prepared.map_instance(&before, &old).unwrap();
                    let binding = prepared
                        .document
                        .audio_bindings()
                        .resolve(&current.node, &current, crate::MAX_DOCUMENT_NODES)
                        .unwrap();
                    // At NTSC rates the current entry may round to another
                    // fractional phase. Its retained lattice still starts at
                    // the old concrete boundary, including other plays of the
                    // shared default and a previously unbound root sibling.
                    assert_eq!(binding.lattice.origin, ExactRatio::integer(old_start));
                    assert_eq!(
                        binding.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
                        i64::try_from(
                            ExactRatio::new(i128::from(old_start) * 8008, 5)
                                .unwrap()
                                .round_even()
                                .unwrap()
                        )
                        .unwrap()
                    );
                    if let Some(resume) = binding.resume {
                        assert_eq!(resume.local_boundary, ExactRatio::ZERO);
                        assert_eq!(resume.reference_local_delta, ExactRatio::ZERO);
                    }
                }
            }
        }
    }
}

#[test]
fn scoped_hold_duration_direct_repeat_ancestry_reanchors_later_plays_and_root_tail() {
    let before = tree(
        &["outer", "root-tail"],
        vec![
            ("outer", repeated("outer", "inner", 2)),
            ("inner", repeated("inner", "a", 2)),
            ("a", hold(2)),
            ("root-tail", hold(3)),
        ],
    );
    for ordinal in [0, 1] {
        let within = presentation(ordinal, ordinal);
        let mut context = entry();
        context.cursor = ProjectFrame(6 * i64::from(ordinal) + 1);
        context.scoped.as_mut().unwrap().presentation = within.clone();
        context.scoped.as_mut().unwrap().target.repeats = within
            .repeats
            .iter()
            .map(|step| RepeatEditStep {
                repeat: step.node.clone(),
                branch: RepeatEditBranch::Play {
                    iteration: step.iteration.clone(),
                },
            })
            .collect();
        for frames in [1, 3] {
            let plan = plan(
                &before,
                &context,
                vec![duration(frames)],
                &BTreeMap::new(),
                super::super::allocate,
            )
            .unwrap();
            let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
                panic!()
            };
            let prepared = crate::prepare_occurrence_hold_duration(
                &before,
                &transaction.steps()[0].edit().unwrap().request(&before),
            )
            .unwrap();
            let mut survivors = vec![(
                InstancePath {
                    node: node("root-tail"),
                    repeats: vec![],
                },
                8,
            )];
            if ordinal == 0 {
                survivors.extend([(presentation(0, 1), 2), (presentation(1, 0), 4)]);
            }
            for (old, start) in survivors {
                let mapped = prepared.map_instance(&before, &old).unwrap();
                let binding = prepared
                    .document
                    .audio_bindings()
                    .resolve(&mapped.node, &mapped, crate::MAX_DOCUMENT_NODES)
                    .unwrap();
                assert_eq!(binding.lattice.origin, ExactRatio::integer(start));
            }
        }
    }
}
