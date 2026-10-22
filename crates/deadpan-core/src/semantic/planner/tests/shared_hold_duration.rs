//! All-Default duration intent preserves sparse ownership and concrete clocks.
use super::*;
use crate::{
    Anchor, AnchorLossPolicy, AssetId, AssetRecord, BoundaryAnchor, ExactRatio, InsertionBias,
    InstancePath, IterationId, IterationOrder, Mark, MarkState, PlayOverride, PlayOverrides,
    RepeatEditBranch, RepeatEditStep, ScopedNodeTarget, SemanticSharedHoldDurationContext,
    SourceAudio, SourceAudioMapping, SourceQualificationId, SourceSpan, SourceTimeBase,
    SourceTimestamp,
};

fn repeat_allocation(owner: &str) -> RevisionId {
    revision(&format!("shared-repeat-{owner}"))
}

fn iteration(owner: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: repeat_allocation(owner),
        ordinal,
    }
}

fn repeated(owner: &str, child: &str, plays: u32) -> BeatNode {
    let mut result = BeatNode::sequence("Repeat", vec![]);
    result.kind = NodeKind::Repeat {
        child: node(child),
        iterations: IterationOrder::new(repeat_allocation(owner), plays).unwrap(),
        gap: None,
        escalation: None,
    };
    result
}

fn fixture(audio: bool) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("shared-duration").unwrap(),
        revision("base"),
        crate::basis::default_basis(),
        node("root"),
    )
    .unwrap();
    document.nodes.extend([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("outer"), node("tail")]),
        ),
        (node("lead"), hold(3)),
        (node("tail"), hold(3)),
        (node("outer"), repeated("outer", "inner", 3)),
        (node("inner"), repeated("inner", "body", 2)),
        (
            node("body"),
            BeatNode::sequence("Body", vec![node("before"), node("held"), node("after")]),
        ),
        (node("before"), hold(2)),
        (node("held"), hold(2)),
        (node("after"), hold(2)),
        (
            node("inner-owned"),
            BeatNode::sequence(
                "Owned inner",
                vec![
                    node("inner-before"),
                    node("inner-held"),
                    node("inner-after"),
                ],
            ),
        ),
        (node("inner-before"), hold(2)),
        (node("inner-held"), hold(4)),
        (node("inner-after"), hold(2)),
        (
            node("outer-owned"),
            BeatNode::sequence(
                "Owned outer",
                vec![
                    node("outer-before"),
                    node("outer-held"),
                    node("outer-after"),
                ],
            ),
        ),
        (node("outer-before"), hold(2)),
        (node("outer-held"), hold(7)),
        (node("outer-after"), hold(2)),
    ]);
    document.overrides.insert(
        node("inner"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration("inner", 1),
            root: node("inner-owned"),
        }])
        .unwrap(),
    );
    document.overrides.insert(
        node("outer"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration("outer", 2),
            root: node("outer-owned"),
        }])
        .unwrap(),
    );
    if audio {
        document.presentation_basis.frame_rate = crate::FrameRate::new(30_000, 1001).unwrap();
        let time_base = SourceTimeBase::new(1, 44_100).unwrap();
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 3100,
                time_base,
            },
            SourceTimestamp {
                ticks: 23_100,
                time_base,
            },
        )
        .unwrap();
        let audio = SourceAudio {
            asset: AssetId::new("audio").unwrap(),
            span,
        };
        document.assets.insert(
            audio.asset.clone(),
            AssetRecord {
                label: "Qualified audio".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(span),
                still_image: false,
                frame_count: None,
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
        );
        for owner in ["lead", "held", "inner-held", "outer-held"] {
            let NodeKind::Hold { recipe } = &mut document.nodes.get_mut(&node(owner)).unwrap().kind
            else {
                panic!()
            };
            recipe.audio = HoldAudio::RoomTone {
                source: audio.clone(),
            };
        }
        for owner in [
            "before",
            "after",
            "inner-before",
            "inner-after",
            "outer-before",
            "outer-after",
            "tail",
        ] {
            let NodeKind::Hold { recipe } = &document.nodes()[&node(owner)].kind else {
                panic!()
            };
            let duration = recipe.duration;
            let source = SourceNode {
                duration,
                edit_window: None,
                video: crate::SourceVideo::Blank,
                video_mapping: crate::SourceVideoMapping::FitBeat,
                audio: Some(audio.clone()),
                audio_mapping: SourceAudioMapping::natural_rate(
                    span,
                    document.presentation_basis().frame_rate,
                )
                .unwrap(),
                link: crate::LinkRelation::Independent,
                audio_offset: crate::AudioSample(0),
            };
            let mut beat = BeatNode::sequence("Source", vec![]);
            beat.kind = NodeKind::Source { source };
            document.nodes.insert(node(owner), beat);
        }
    }
    document.validate().unwrap();
    assert_eq!(document.duration().unwrap().frames(), 45);
    document
}

fn target() -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: node("held"),
        repeats: ["outer", "inner"]
            .into_iter()
            .map(|owner| RepeatEditStep {
                repeat: node(owner),
                branch: RepeatEditBranch::Default,
            })
            .collect(),
    }
}

fn instance(owner: &str, outer: u32, inner: Option<u32>) -> InstancePath {
    InstancePath {
        node: node(owner),
        repeats: std::iter::once(crate::RepeatInstance {
            node: node("outer"),
            iteration: iteration("outer", outer),
        })
        .chain(inner.map(|index| crate::RepeatInstance {
            node: node("inner"),
            iteration: iteration("inner", index),
        }))
        .collect(),
    }
}

fn entry(outer: Option<u32>) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("outer")),
        shared_hold_duration: Some(SemanticSharedHoldDurationContext {
            root: node("outer"),
            target: target(),
            presentation: outer.map(|outer| instance("held", outer, Some(0))),
        }),
        ..context("root", 20)
    }
}

fn duration(frames: u32) -> SemanticInstruction {
    SemanticInstruction::SetHoldDuration {
        length: crate::PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn visible(
    document: &ProjectDocument,
    context: &SemanticContext,
) -> Option<HoldDurationVisibility> {
    document
        .shared_hold_duration_visibility(
            &context.parent,
            context.shared_hold_duration.as_ref().unwrap(),
        )
        .unwrap()
}

fn reversible(before: &ProjectDocument, planned: &SemanticPlan) {
    let replay =
        crate::replay_compound::<EditError>(before, planned.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&planned.document).unwrap(),
        *before
    );
    assert_eq!(
        ProjectDocument::from_json(&planned.document.to_json().unwrap()).unwrap(),
        planned.document
    );
}

#[test]
fn shared_hold_duration_changes_only_live_defaults_and_moves_same_representative() {
    let before = fixture(false);
    let mut previous = None;
    for outer in [0, 1] {
        let context = entry(Some(outer));
        assert_eq!(
            visible(&before, &context).unwrap().frames,
            range(5 + 14 * i64::from(outer), 7 + 14 * i64::from(outer))
        );
        let mut allocations = vec![];
        let planned = plan_semantic(
            &before,
            &context,
            &program(vec![duration(4)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("outer"),
            |request| {
                allocations.push(request.clone());
                allocate(request)
            },
            no_original,
        )
        .unwrap();
        assert_eq!(
            allocations,
            vec![SemanticAllocationRequest::ParameterEdit { step_index: 0 }]
        );
        assert_eq!(planned.document.duration().unwrap().frames(), 49);
        assert_eq!(planned.document.overrides(), before.overrides());
        assert_eq!(planned.document.nodes().len(), before.nodes().len());
        for owner in [
            "inner-owned",
            "inner-before",
            "inner-held",
            "inner-after",
            "outer-owned",
            "outer-before",
            "outer-held",
            "outer-after",
        ] {
            assert_eq!(
                planned.document.nodes()[&node(owner)],
                before.nodes()[&node(owner)]
            );
        }
        let mut expected = context;
        expected.cursor = ProjectFrame(5 + 16 * i64::from(outer));
        assert_eq!(planned.context, expected);
        assert_eq!(planned.trace[0].resolved_parent, Some(node("body")));
        assert_eq!(
            planned.trace[0].resolved_range,
            Some(range(expected.cursor.0, expected.cursor.0 + 4))
        );
        if let Some(previous) = previous {
            assert_eq!(planned.document, previous);
        }
        previous = Some(planned.document.clone());
        reversible(&before, &planned);
    }
}

#[test]
fn shared_hold_duration_retains_every_live_source_roomtone_clock_with_partial_bindings() {
    for partial in [false, true] {
        let mut before = fixture(true);
        let old = crate::capture_unbound_audio_bindings(
            &before,
            AudioTimingId {
                allocation: revision("old-clock"),
                ordinal: 0,
            },
        )
        .unwrap();
        if partial {
            before.audio_bindings = old.clone();
            before.audio_bindings.bindings.remove(&node("tail"));
            before.validate().unwrap();
        }
        for frames in [1, 4] {
            let planned = plan(
                &before,
                entry(Some(1)),
                vec![duration(frames)],
                &BTreeMap::new(),
            )
            .unwrap();
            for (owner, outer, inner) in [
                ("held", 0, Some(0)),
                ("after", 0, Some(0)),
                ("held", 1, Some(0)),
                ("after", 1, Some(0)),
                ("inner-after", 1, Some(1)),
                ("outer-after", 2, None),
            ] {
                let instance = instance(owner, outer, inner);
                let before = old
                    .resolve(&instance.node, &instance, crate::MAX_DOCUMENT_NODES)
                    .unwrap();
                let after = planned
                    .document
                    .audio_bindings()
                    .resolve(&instance.node, &instance, crate::MAX_DOCUMENT_NODES)
                    .unwrap();
                assert_eq!(after.lattice.origin, before.lattice.origin);
                assert_eq!(
                    after.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
                    before.lattice.sample_boundary(ExactRatio::ZERO).unwrap()
                );
            }
            reversible(&before, &planned);
        }
    }
}

fn dormant(outer: bool) -> ProjectDocument {
    let mut document = fixture(true);
    if outer {
        for ordinal in [0, 1] {
            let id = node(&format!("outer-hidden-{ordinal}"));
            document.nodes.insert(id.clone(), hold(14));
            document
                .overrides
                .get_mut(&node("outer"))
                .unwrap()
                .insert(iteration("outer", ordinal), id);
        }
    } else {
        document.nodes.insert(node("inner-hidden"), hold(6));
        document
            .overrides
            .get_mut(&node("inner"))
            .unwrap()
            .insert(iteration("inner", 0), node("inner-hidden"));
    }
    document.validate().unwrap();
    document.audio_bindings = crate::capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: revision("dormant-clock"),
            ordinal: 0,
        },
    )
    .unwrap();
    document.validate().unwrap();
    document
}

#[test]
fn shared_hold_duration_proves_inner_or_outer_dormancy_and_preserves_all_binding_history() {
    for outer in [false, true] {
        let before = dormant(outer);
        let context = entry(None);
        assert_eq!(visible(&before, &context), None);
        let bank = BTreeMap::from([(name('a'), macro_value(vec![duration(4), duration(2)]))]);
        let planned = plan(&before, context.clone(), vec![call('a', 3)], &bank).unwrap();
        assert_eq!(planned.context, context);
        assert_eq!(
            planned.document.duration().unwrap(),
            before.duration().unwrap()
        );
        assert_eq!(planned.document.audio_bindings(), before.audio_bindings());
        assert_eq!(planned.document.overrides(), before.overrides());
        assert!(
            planned
                .trace
                .iter()
                .filter(|trace| matches!(
                    &trace.instruction,
                    SemanticInstruction::SetHoldDuration { .. }
                ))
                .all(|trace| trace.resolved_range.is_none())
        );
        reversible(&before, &planned);
        assert!(
            before
                .shared_hold_duration_visibility(
                    &node("root"),
                    entry(Some(0)).shared_hold_duration.as_ref().unwrap()
                )
                .is_err()
        );
    }
}

#[test]
fn shared_hold_duration_gap_overrides_do_not_forge_dormancy_and_forged_presentations_refuse() {
    let mut before = fixture(false);
    before.nodes.insert(node("owned-gap"), hold(1));
    before.gap_overrides.insert(
        node("outer"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration("outer", 0),
            root: node("owned-gap"),
        }])
        .unwrap(),
    );
    before.validate().unwrap();
    let error = before
        .shared_hold_duration_visibility(
            &node("root"),
            entry(None).shared_hold_duration.as_ref().unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(
        error
            .message
            .contains("needs its captured concrete presentation")
    );
    let mut contexts = vec![(
        entry(None),
        EditErrorCode::SelectionUnavailable,
        "needs its captured concrete presentation",
    )];
    let mut forged = entry(Some(0));
    forged.shared_hold_duration.as_mut().unwrap().presentation = Some(instance("held", 0, Some(1)));
    contexts.push((
        forged,
        EditErrorCode::InvalidCommand,
        "wrong Repeat or a retired iteration",
    ));
    let mut forged = entry(Some(0));
    forged.shared_hold_duration.as_mut().unwrap().target.repeats[0].branch =
        RepeatEditBranch::Play {
            iteration: iteration("outer", 0),
        };
    contexts.push((
        forged,
        EditErrorCode::SelectionUnavailable,
        "All plays at every Repeat ancestor",
    ));
    let mut forged = entry(Some(0));
    forged
        .shared_hold_duration
        .as_mut()
        .unwrap()
        .target
        .repeats
        .clear();
    contexts.push((
        forged,
        EditErrorCode::SelectionUnavailable,
        "All plays at every Repeat ancestor",
    ));
    let mut forged = entry(Some(0));
    forged.shared_hold_duration.as_mut().unwrap().target.node = node("inner-held");
    contexts.push((forged, EditErrorCode::InvalidCommand, "Default must follow"));
    let mut forged = entry(Some(0));
    forged.shared_hold_duration.as_mut().unwrap().root = node("inner");
    contexts.push((
        forged,
        EditErrorCode::SelectionUnavailable,
        "selected inspector root",
    ));
    let mut forged = entry(Some(0));
    forged.shared_hold_duration.as_mut().unwrap().target.node = node("owned-gap");
    contexts.push((
        forged,
        EditErrorCode::InvalidCommand,
        "wrong Repeat ancestor",
    ));
    for (context, code, message) in contexts {
        let error = plan_semantic(
            &before,
            &context,
            &program(vec![duration(4)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("refuse"),
            |_| panic!("forged shared authority allocated"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, code);
        assert!(error.message.contains(message), "{error:?}");
    }
}

#[test]
fn shared_hold_duration_counted_calls_revalidate_live_authority_after_each_capture() {
    let before = fixture(true);
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![duration(4), duration(3)])),
    ]);
    let snapshot = bank.clone();
    let planned = plan(&before, entry(Some(1)), vec![call('a', 2)], &bank).unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 47);
    assert_eq!(planned.context.cursor, ProjectFrame(20));
    assert_eq!(
        planned.context.shared_hold_duration,
        entry(Some(1)).shared_hold_duration
    );
    assert_eq!(bank, snapshot);
    assert_eq!(planned.document.overrides(), before.overrides());
    reversible(&before, &planned);
}

#[test]
fn shared_hold_duration_million_play_work_stays_compact() {
    let mut before = fixture(true);
    for owner in ["outer", "inner"] {
        let NodeKind::Repeat { iterations, .. } =
            &mut before.nodes.get_mut(&node(owner)).unwrap().kind
        else {
            panic!()
        };
        *iterations = IterationOrder::new(repeat_allocation(owner), 1_000_000).unwrap();
    }
    before.validate().unwrap();
    let planned = plan(&before, entry(Some(1)), vec![duration(3)], &BTreeMap::new()).unwrap();
    assert_eq!(
        planned.document.duration().unwrap().frames() - before.duration().unwrap().frames(),
        999_998_000_001
    );
    assert_eq!(planned.document.nodes().len(), before.nodes().len());
    assert!(planned.document.audio_bindings().timings().len() <= 1);
    assert!(planned.document.audio_bindings().to_json().unwrap().len() < 50_000);
    reversible(&before, &planned);
}

#[test]
fn shared_hold_duration_late_noop_mixed_instruction_and_wrong_resource_fail_atomically() {
    let before = fixture(true);
    let snapshot = before.clone();
    let bank = BTreeMap::from([(name('a'), macro_value(vec![duration(4), motion(true, 1)]))]);
    let saved_bank = bank.clone();
    for (instructions, expected_allocations, message) in [
        (vec![duration(2)], 0, "already has that duration"),
        (
            vec![duration(4), duration(4)],
            1,
            "already has that duration",
        ),
        (
            vec![call('a', 1)],
            1,
            "supports only Hold duration and named duration calls",
        ),
        (
            vec![
                duration(4),
                SemanticInstruction::InsertPause {
                    length: crate::PauseLength::Frames {
                        frames: NonZeroU32::new(1).unwrap(),
                    },
                    black: true,
                },
            ],
            1,
            "supports only Hold duration and named duration calls",
        ),
    ] {
        let mut allocations = vec![];
        let error = plan_semantic(
            &before,
            &entry(Some(1)),
            &program(instructions),
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision("outer"),
            |request| {
                allocations.push(request.clone());
                allocate(request)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains(message), "{error:?}");
        assert_eq!(allocations.len(), expected_allocations);
        assert!(
            allocations
                .iter()
                .all(|request| matches!(request, SemanticAllocationRequest::ParameterEdit { .. }))
        );
        assert_eq!(before, snapshot);
        assert_eq!(bank, saved_bank);
    }
    let error = plan_semantic(
        &before,
        &entry(Some(1)),
        &program(vec![duration(4)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("wrong-resource"),
        |_| {
            Ok(SemanticAllocation::SetRepeatPlays {
                new_revision: revision("wrong-leaf"),
            })
        },
        no_original,
    )
    .unwrap_err();
    assert!(error.message.contains("parameter allocation"));
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    let error = plan_semantic(
        &before,
        &entry(Some(1)),
        &program(vec![duration(2)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("noop"),
        |_| panic!("no-op allocated"),
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(error.message.contains("already has that duration"));
}

#[test]
fn shared_hold_duration_conflicting_contexts_and_retime_ancestry_remain_unavailable() {
    let before = fixture(false);
    let mut cases = vec![];
    let mut context = entry(Some(0));
    context.visual_selection = Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(5),
        head: ProjectFrame(6),
        extending: false,
    });
    cases.push(context);
    let mut context = entry(Some(0));
    context.selected_child = Some(node("tail"));
    cases.push(context);
    let mut context = entry(Some(0));
    context.scoped = Some(SemanticScopedContext {
        root: node("outer"),
        target: target(),
        presentation: instance("held", 0, Some(0)),
    });
    cases.push(context);
    let mut context = entry(Some(0));
    context.hold_duration = Some(SemanticHoldDurationContext {
        root: node("outer"),
        target: node("held"),
        presentation: instance("held", 0, Some(0)),
    });
    cases.push(context);
    let mut context = entry(Some(0));
    context.original = Some(SemanticOriginalContext {
        asset: AssetId::new("absent").unwrap(),
        qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
        ordinal: crate::SourceFrameId(0),
    });
    cases.push(context);
    for context in cases {
        let error = plan_semantic(
            &before,
            &context,
            &program(vec![duration(4)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("conflict"),
            |_| panic!("conflicting authority allocated"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        assert!(error.message.contains("requires Your edit"), "{error:?}");
    }
    let mut retimed = before.clone();
    let mut stage = BeatNode::sequence("Stage", vec![]);
    stage.kind = NodeKind::Retime {
        child: node("inner"),
        duration: FrameDuration::new(14).unwrap(),
        mapping: range(0, 14),
        pitch: crate::PitchPolicy::FollowSpeed,
        purpose: crate::RetimePurpose::Edit,
    };
    retimed.nodes.insert(node("rate"), stage);
    let NodeKind::Repeat { child, .. } = &mut retimed.nodes.get_mut(&node("outer")).unwrap().kind
    else {
        panic!()
    };
    *child = node("rate");
    retimed.validate().unwrap();
    target().validate(&retimed).unwrap();
    assert!(
        retimed
            .shared_hold_duration_visibility(
                &node("root"),
                entry(Some(0)).shared_hold_duration.as_ref().unwrap()
            )
            .unwrap_err()
            .message
            .contains("without Retime")
    );
}

#[test]
fn shared_hold_duration_shortening_retains_tail_clamp_mark_loss_and_inverse() {
    let mut before = fixture(false);
    let NodeKind::Hold { recipe } = &mut before.nodes.get_mut(&node("held")).unwrap().kind else {
        panic!()
    };
    recipe.audio = HoldAudio::Tail {
        maximum: FrameDuration::new(2).unwrap(),
        effect: Default::default(),
        input: Default::default(),
    };
    before.marks.insert(
        crate::MarkId::new("held-mark").unwrap(),
        Mark {
            owner: node("held"),
            label: "Retained intent".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("held"),
                    position: ExactRatio::new(3, 2).unwrap(),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
            fragments: vec![],
        },
    );
    before.validate().unwrap();
    let planned = plan(&before, entry(Some(1)), vec![duration(1)], &BTreeMap::new()).unwrap();
    let NodeKind::Hold { recipe } = &planned.document.nodes()[&node("held")].kind else {
        panic!()
    };
    assert!(matches!(recipe.audio, HoldAudio::Tail { maximum, .. } if maximum.frames() == 1));
    assert!(matches!(
        planned.document.marks()[&crate::MarkId::new("held-mark").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    reversible(&before, &planned);
}

#[test]
fn shared_hold_duration_root_sound_refusal_preserves_document_context_and_bank() {
    let mut before = fixture(true);
    let NodeKind::Hold { recipe } = &before.nodes()[&node("held")].kind else {
        panic!()
    };
    let HoldAudio::RoomTone { source } = &recipe.audio else {
        panic!()
    };
    let source = source.clone();
    before.sounds.insert(
        crate::SoundId::new("root-effect").unwrap(),
        crate::SoundEvent {
            owner: node("root"),
            label: "Root effect".into(),
            mapping: SourceAudioMapping::natural_rate(
                source.span,
                before.presentation_basis().frame_rate,
            )
            .unwrap(),
            source,
            offset: crate::AudioSample(37),
            treatments: crate::AudioTreatments::default(),
            start_edge: crate::AudioEdgePolicy::Hard,
            end_edge: crate::AudioEdgePolicy::Hard,
            overflow: crate::SoundOverflowPolicy::Reject,
        },
    );
    before.validate().unwrap();
    let context = entry(Some(1));
    assert!(visible(&before, &context).is_some());
    let snapshot = before.to_json().unwrap();
    let bank = BTreeMap::from([(name('a'), macro_value(vec![duration(4)]))]);
    let saved_bank = bank.clone();
    let error = plan(&before, context.clone(), vec![call('a', 1)], &bank).unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(
        error
            .message
            .contains("cannot yet preserve authored sound intervals")
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
    assert_eq!(context, entry(Some(1)));
    assert_eq!(bank, saved_bank);
}
