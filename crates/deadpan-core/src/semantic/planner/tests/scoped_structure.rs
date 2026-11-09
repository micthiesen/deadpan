//! Split, delete, wrap and Repeat count/gap intent resolve the beat and play
//! chosen at each invocation, as one `EditOccurrence` built like the native edit.

use super::*;
use crate::{OccurrenceEdit, PauseLength, PauseProvider, PauseSite};

fn plan(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    allocator: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_framing(
        document,
        context,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 3,
        },
        revision(&format!("{}-outer", document.revision_id())),
        allocator,
        no_original,
        |_| Err(speech_unavailable()),
        |_: &ProjectDocument, site: PauseSite| {
            assert!(matches!(site, PauseSite::RepeatGap { .. }));
            Ok(PauseProvider {
                video: crate::HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            })
        },
        None,
        state,
    )
}

/// One-play authority over `node` in the given outer and inner plays.
fn chosen(node_name: &str, outer: u32, inner: Option<u32>, cursor: i64) -> SemanticContext {
    let mut context = entry();
    let mut within = presentation(outer, inner.unwrap_or(0));
    within.node = node(node_name);
    if inner.is_none() {
        within.repeats.pop();
    }
    let scoped = context.scoped.as_mut().unwrap();
    scoped.target = ScopedNodeTarget {
        node: within.node.clone(),
        repeats: within
            .repeats
            .iter()
            .map(|step| RepeatEditStep {
                repeat: step.node.clone(),
                branch: RepeatEditBranch::Play {
                    iteration: step.iteration.clone(),
                },
            })
            .collect(),
    };
    scoped.presentation = within;
    context.cursor = ProjectFrame(cursor);
    context
}

fn leaf(plan: &SemanticPlan, before: &ProjectDocument) -> Command {
    let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
        panic!("one Compound")
    };
    assert_eq!(transaction.steps().len(), 1);
    transaction.steps()[0]
        .edit()
        .unwrap()
        .request(before)
        .command
}

fn check_replay(before: &ProjectDocument, plan: &SemanticPlan) {
    let request = plan.request.as_ref().unwrap();
    let replay = crate::replay_compound::<EditError>(before, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), *before);
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert!(
        mapped
            .target
            .matches_instance(&plan.document, &mapped.presentation)
            .unwrap()
    );
}

/// The shared test allocator with names unique to one invocation.
fn renamed(
    prefix: &'static str,
) -> impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError> {
    move |request| match super::super::allocate(request)? {
        SemanticAllocation::Isolation {
            new_revision,
            identities,
        } => Ok(SemanticAllocation::Isolation {
            new_revision: revision(&format!("{prefix}-{}", new_revision.as_str())),
            identities: crate::OccurrenceIdentities {
                nodes: identities
                    .nodes
                    .iter()
                    .map(|id| node(&format!("{prefix}-{}", id.as_str())))
                    .collect(),
                marks: identities
                    .marks
                    .iter()
                    .map(|id| crate::MarkId::new(format!("{prefix}-{}", id.as_str())).unwrap())
                    .collect(),
            },
        }),
        other => Ok(other),
    }
}

fn plays(frames: u32) -> NonZeroU32 {
    NonZeroU32::new(frames).unwrap()
}

#[test]
fn scoped_split_builds_the_native_occurrence_command_and_continues_on_the_right_fragment() {
    let before = fixture();
    // `a` in outer play 1, inner play 1 runs from 27 to 31.
    let context = chosen("a", 1, Some(1), 29);
    let mut requested = Vec::new();
    let plan = plan(
        &before,
        &context,
        vec![SemanticInstruction::Split],
        |request| {
            requested.push(request.clone());
            super::super::allocate(request)
        },
    )
    .unwrap();
    let instance = presentation(1, 1);
    let needs = before
        .occurrence_structure_requirements(
            &instance,
            &OccurrenceEdit::Split {
                at: FrameDuration::new(2).unwrap(),
                identities: SplitIdentities { nodes: vec![] },
            },
        )
        .unwrap();
    // The beat's one node, both Partitions, a root context and a container.
    let extra = 4;
    assert_eq!(
        requested,
        vec![SemanticAllocationRequest::Isolation {
            step_index: 0,
            nodes: needs.nodes + extra,
            marks: needs.marks,
        }]
    );
    let names = |range: std::ops::Range<usize>| {
        range
            .map(|n| node(&format!("isolate-0-{n}")))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        leaf(&plan, &before),
        Command::EditOccurrence {
            instance: instance.clone(),
            edit: OccurrenceEdit::Split {
                at: FrameDuration::new(2).unwrap(),
                identities: SplitIdentities {
                    nodes: names(needs.nodes..needs.nodes + extra),
                },
            },
            identities: crate::OccurrenceIdentities {
                nodes: names(0..needs.nodes),
                marks: (0..needs.marks)
                    .map(|n| crate::MarkId::new(format!("isolate-0-{n}")).unwrap())
                    .collect(),
            },
        }
    );
    check_replay(&before, &plan);
    assert_eq!(plan.context.cursor, ProjectFrame(29));
    assert_eq!(plan.context.parent, context.parent);
    assert_eq!(plan.context.selected_child, context.selected_child);
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert_eq!(
        plan.document.node_duration(&mapped.target.node).unwrap(),
        FrameDuration::new(2).unwrap()
    );
    assert_eq!(
        plan.document.duration().unwrap(),
        before.duration().unwrap()
    );
    // Other plays keep the shared definition.
    assert_eq!(
        before.nodes()[&node("a")],
        plan.document.nodes()[&node("a")]
    );
}

#[test]
fn recorded_split_and_delete_replay_in_a_different_play_and_leave_the_recorded_play_alone() {
    let before = fixture();
    let program = vec![SemanticInstruction::Split, SemanticInstruction::DeleteBeat];
    let first = plan(
        &before,
        &chosen("a", 1, Some(1), 29),
        program.clone(),
        super::super::allocate,
    )
    .unwrap();
    check_replay(&before, &first);
    // Deleting the right fragment closes up its play by two frames and
    // continues on its following sibling, `b`, at the deleted start.
    assert_eq!(
        first.document.duration().unwrap().frames(),
        before.duration().unwrap().frames() - 2
    );
    assert_eq!(first.context.cursor, ProjectFrame(29));
    let mapped = first.context.scoped.as_ref().unwrap();
    assert_eq!(
        first.document.node_duration(&mapped.target.node).unwrap(),
        FrameDuration::new(3).unwrap()
    );
    // The same program in outer play 2, inner play 0, where `a` runs 36..40
    // in the edited document. Allocation names start afresh per plan, so give
    // this invocation its own.
    let second_context = chosen("a", 2, Some(0), 38);
    let second = plan(&first.document, &second_context, program, renamed("again")).unwrap();
    check_replay(&first.document, &second);
    assert_eq!(
        second.document.duration().unwrap().frames(),
        before.duration().unwrap().frames() - 4
    );
    assert_eq!(second.context.cursor, ProjectFrame(38));
    // The first play's override is untouched by the second invocation.
    let outer = &node("outer");
    for play in [iteration("outer", 0), iteration("outer", 1)] {
        assert_eq!(
            first
                .document
                .overrides()
                .get(outer)
                .and_then(|plays| plays.get(&play)),
            second
                .document
                .overrides()
                .get(outer)
                .and_then(|plays| plays.get(&play)),
        );
    }
}

#[test]
fn scoped_wrap_count_and_gaps_follow_the_native_selection_and_cursor() {
    let before = fixture();
    // `rr` on `b` (outer play 0, inner play 1: 13..16) wraps it in three plays.
    let wrapped = plan(
        &before,
        &chosen("b", 0, Some(1), 14),
        vec![SemanticInstruction::Repeat {
            selector: SemanticSelector::SelectedBeat,
            plays: plays(3),
            escalation: None,
        }],
        super::super::allocate,
    )
    .unwrap();
    check_replay(&before, &wrapped);
    let Command::EditOccurrence {
        edit: OccurrenceEdit::RepeatChild { id, plays: 3, .. },
        ..
    } = leaf(&wrapped, &before)
    else {
        panic!("one-play wrap")
    };
    let mapped = wrapped.context.scoped.as_ref().unwrap();
    assert_eq!(mapped.target.node, id);
    assert_eq!(wrapped.context.cursor, ProjectFrame(13));
    assert_eq!(
        wrapped.document.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 6
    );
    // `:repeat 4 gap=2f` then sets the new Repeat's count and gaps there only,
    // continuing on the Repeat in the same play.
    let mut next = wrapped.context.clone();
    next.cursor = ProjectFrame(14);
    let gapped = plan(
        &wrapped.document,
        &next,
        vec![SemanticInstruction::SetRepeat {
            plays: Some(plays(4)),
            gaps: Some(vec![PauseLength::Frames { frames: plays(2) }]),
            escalation: None,
            speed: None,
        }],
        renamed("gap"),
    )
    .unwrap();
    check_replay(&wrapped.document, &gapped);
    assert!(matches!(
        leaf(&gapped, &wrapped.document),
        Command::EditOccurrence {
            edit: OccurrenceEdit::SetRepeatGaps {
                plays: 4,
                gap: Some(_),
                ..
            },
            ..
        }
    ));
    assert_eq!(gapped.context.cursor, ProjectFrame(13));
    assert_eq!(
        gapped.context.scoped.as_ref().unwrap().target.node,
        mapped.target.node
    );
    // Four plays of three frames and three two-frame gaps replace nine frames.
    assert_eq!(
        gapped.document.duration().unwrap().frames(),
        wrapped.document.duration().unwrap().frames() + 3 + 6
    );
    // The same plays and gaps again refuse without allocating anything.
    let mut again = gapped.context.clone();
    again.cursor = ProjectFrame(14);
    let error = plan(
        &gapped.document,
        &again,
        vec![SemanticInstruction::SetRepeat {
            plays: Some(plays(4)),
            gaps: Some(vec![PauseLength::Frames { frames: plays(2) }]),
            escalation: None,
            speed: None,
        }],
        |_| panic!("an unchanged Repeat allocates nothing"),
    )
    .unwrap_err();
    assert!(
        error.message.contains("already has these plays and gaps"),
        "{error:?}"
    );
    // `:repeat 2` on the shown Repeat sets its count.
    let counted = plan(
        &gapped.document,
        &again,
        vec![SemanticInstruction::SetRepeatPlays { plays: plays(2) }],
        renamed("count"),
    )
    .unwrap();
    assert!(matches!(
        leaf(&counted, &gapped.document),
        Command::EditOccurrence {
            edit: OccurrenceEdit::SetRepeatPlays { plays: 2, .. },
            ..
        }
    ));
}

#[test]
fn scoped_structure_refuses_without_explicit_play_and_for_unsupported_intent_atomically() {
    let before = fixture();
    let never = |_: SemanticAllocationRequest| -> Result<SemanticAllocation, EditError> {
        panic!("a refused edit allocates nothing")
    };
    // DeleteBeat has no meaning without one-play authority.
    let mut ordinary = context("root", 3);
    ordinary.selected_child = Some(node("outer"));
    let error = plan(
        &before,
        &ordinary,
        vec![SemanticInstruction::DeleteBeat],
        never,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("This play"), "{error:?}");
    // All plays at a level is not single-play authority.
    let mut shared = chosen("a", 1, Some(1), 29);
    shared.scoped.as_mut().unwrap().target.repeats[1].branch = RepeatEditBranch::Default;
    assert!(plan(&before, &shared, vec![SemanticInstruction::Split], never).is_err());
    // A split needs an interior cursor.
    let error = plan(
        &before,
        &chosen("a", 1, Some(1), 27),
        vec![SemanticInstruction::Split],
        never,
    )
    .unwrap_err();
    assert!(error.message.contains("move inside"), "{error:?}");
    // Escalation, speed steps and other selectors stay with the parent.
    for instruction in [
        SemanticInstruction::Repeat {
            selector: SemanticSelector::SelectedBeat,
            plays: plays(2),
            escalation: Some(crate::RepeatEscalation {
                gain_step: crate::GainDb::new(-600).unwrap(),
                zoom: None,
            }),
        },
        SemanticInstruction::Repeat {
            selector: SemanticSelector::VisualSelection,
            plays: plays(2),
            escalation: None,
        },
        SemanticInstruction::SetRepeat {
            plays: Some(plays(2)),
            gaps: None,
            escalation: None,
            speed: Some(crate::SpeedStep {
                step: crate::ExactRatio::new(1, 4).unwrap(),
                multiply: false,
                preserve_pitch: false,
            }),
        },
    ] {
        assert!(
            plan(
                &before,
                &chosen("a", 1, Some(1), 29),
                vec![instruction],
                never
            )
            .is_err()
        );
    }
    // Gaps need a Repeat; a plain beat must be wrapped first.
    let error = plan(
        &before,
        &chosen("a", 1, Some(1), 29),
        vec![SemanticInstruction::SetRepeat {
            plays: None,
            gaps: Some(vec![PauseLength::Frames { frames: plays(2) }]),
            escalation: None,
            speed: None,
        }],
        never,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::WrongNodeKind);
    // A later refusal rolls back an earlier staged split: no request at all.
    let error = plan(
        &before,
        &chosen("a", 1, Some(1), 29),
        vec![
            SemanticInstruction::Split,
            SemanticInstruction::SetRepeatPlays { plays: plays(2) },
        ],
        super::super::allocate,
    );
    assert!(error.is_err());
    // An allocator that returns the wrong pool sizes is refused.
    let error = plan(
        &before,
        &chosen("a", 1, Some(1), 29),
        vec![SemanticInstruction::Split],
        |request| match super::super::allocate(request)? {
            SemanticAllocation::Isolation {
                new_revision,
                mut identities,
            } => {
                identities.nodes.pop();
                Ok(SemanticAllocation::Isolation {
                    new_revision,
                    identities,
                })
            }
            other => Ok(other),
        },
    )
    .unwrap_err();
    assert!(
        error.message.contains("exact isolation identity pools"),
        "{error:?}"
    );
}
