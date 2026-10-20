//! Pause intent follows the invocation's exact Play authority after every leaf.

use super::*;
use crate::{PauseLength, PauseProvider, PauseSite};

fn pause(black: bool) -> SemanticInstruction {
    SemanticInstruction::InsertPause {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(2).unwrap(),
        },
        black,
    }
}

fn plan(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    allocator: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    resolver: impl FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_framing(
        document,
        context,
        &program(instructions),
        SemanticRegisterBank {
            entries: bank,
            version: 3,
        },
        revision("scoped-pause-outer"),
        allocator,
        no_original,
        |_| Err(speech_unavailable()),
        resolver,
        None,
        state,
    )
}

fn background() -> PauseProvider {
    PauseProvider {
        video: crate::HoldVideo::Background,
        picture_context: None,
        audio: HoldAudio::Silence,
    }
}

#[test]
fn scoped_pause_first_last_play_edges_use_explicit_authority_and_exact_hold_continuation() {
    let before = fixture();
    for outer in [0, 2] {
        for inner in [0, 1] {
            for local in [0, 1, 4] {
                for black in [false, true] {
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
                    let at = 18 * i64::from(outer) + 2 + 7 * i64::from(inner) + local;
                    context.cursor = ProjectFrame(at);
                    let mut calls = 0;
                    let result = plan(
                        &before,
                        &context,
                        vec![pause(black)],
                        &BTreeMap::new(),
                        super::super::allocate,
                        |staged, site| {
                            calls += 1;
                            assert_eq!(staged, &before);
                            assert_eq!(
                                site,
                                PauseSite::ScopedBoundary {
                                    at: ProjectFrame(at),
                                    within: within.clone()
                                }
                            );
                            Ok(background())
                        },
                    )
                    .unwrap();
                    assert_eq!(calls, usize::from(!black));
                    assert_eq!(result.document.duration().unwrap().frames(), 56);
                    assert_eq!(result.context.cursor, ProjectFrame(at));
                    assert_eq!(result.context.selected_child, Some(node("outer")));
                    let after = result.context.scoped.as_ref().unwrap();
                    assert_eq!(after.root, node("outer"));
                    assert_eq!(after.target.node, node("pause-0"));
                    assert_eq!(
                        after.presentation.repeats[0].iteration,
                        iteration("outer", outer)
                    );
                    assert_eq!(
                        after.presentation.repeats[1].iteration,
                        iteration("inner", inner)
                    );
                    assert!(
                        after
                            .target
                            .matches_instance(&result.document, &after.presentation)
                            .unwrap()
                    );
                    let request = result.request.as_ref().unwrap();
                    let replay =
                        crate::replay_compound::<EditError>(&before, request, |_| Ok(())).unwrap();
                    assert_eq!(replay.document, result.document);
                    assert_eq!(replay.edit.inverse.apply(&result.document).unwrap(), before);
                    assert_eq!(
                        ProjectDocument::from_json(&result.document.to_json().unwrap()).unwrap(),
                        result.document
                    );
                    let Command::Compound { transaction } = &request.command else {
                        panic!()
                    };
                    let Command::InsertOccurrenceTime {
                        target,
                        within: actual,
                        all_plays,
                        ai,
                        ..
                    } = transaction.steps()[0].edit().unwrap().command.as_command()
                    else {
                        panic!()
                    };
                    assert_eq!(
                        target.as_ref(),
                        Some(&context.scoped.as_ref().unwrap().target)
                    );
                    assert_eq!(actual.as_ref(), Some(&within));
                    assert!(!*all_plays && !*ai);
                }
            }
        }
    }
}

#[test]
fn scoped_pause_maps_nested_calls_second_pause_and_zoom_to_inserted_hold_without_reisolating() {
    let before = fixture();
    let context = entry();
    let body = vec![pause(true), zoom(2)];
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(body)),
    ]);
    let frozen = bank.clone();
    let mut allocations = Vec::new();
    let result = plan(
        &before,
        &context,
        vec![pause(true), call('a', 1)],
        &bank,
        |request| {
            allocations.push(request.clone());
            super::super::allocate(request)
        },
        |_, _| panic!("black pause sampled a picture"),
    )
    .unwrap();
    assert_eq!(bank, frozen);
    assert_eq!(result.document.duration().unwrap().frames(), 58);
    let Command::Compound { transaction } = &result.request.as_ref().unwrap().command else {
        panic!()
    };
    assert_eq!(transaction.steps().len(), 3);
    let pauses: Vec<_> = result
        .trace
        .iter()
        .filter(|row| matches!(row.instruction, SemanticInstruction::InsertPause { .. }))
        .collect();
    assert_eq!(pauses.len(), 2);
    assert_eq!(pauses[0].after.scoped, pauses[1].before.scoped);
    let second = pauses[1].after.scoped.as_ref().unwrap();
    assert_eq!(second.target.node, node("pause-1"));
    let after = result.context.scoped.as_ref().unwrap();
    assert_eq!(after, second);
    assert!(
        result.document.nodes()[&after.target.node]
            .framing
            .is_some()
    );
    assert!(matches!(
        allocations[1],
        SemanticAllocationRequest::InsertOccurrencePause {
            isolation_nodes: 0,
            isolation_marks: 0,
            ..
        }
    ));
    assert!(matches!(
        allocations[2],
        SemanticAllocationRequest::Isolation {
            nodes: 0,
            marks: 0,
            ..
        }
    ));
    assert_eq!(result.context.cursor, context.cursor);
    assert_eq!(
        result.document.nodes()[&node("a")],
        before.nodes()[&node("a")]
    );
    let wire = serde_json::to_string(&program(vec![pause(false), pause(true)])).unwrap();
    for forbidden in [
        "outer",
        "inner",
        "presentation",
        "target",
        "pause-0",
        "scoped",
    ] {
        assert!(!wire.contains(forbidden));
    }
}

#[test]
fn scoped_pause_rejects_mixed_authority_ai_and_unsupported_late_calls_atomically() {
    let before = fixture();
    let original = entry();
    let mut mixed = original.clone();
    mixed.scoped.as_mut().unwrap().target.repeats[0].branch = RepeatEditBranch::Default;
    assert!(
        plan(
            &before,
            &mixed,
            vec![pause(true)],
            &BTreeMap::new(),
            |_| panic!("mixed allocated"),
            |_, _| panic!("mixed sampled")
        )
        .is_err()
    );
    for forbidden in [
        SemanticInstruction::InsertAiPause {
            length: PauseLength::Frames {
                frames: NonZeroU32::new(2).unwrap(),
            },
        },
        SemanticInstruction::InsertReverse {
            length: PauseLength::Frames {
                frames: NonZeroU32::new(2).unwrap(),
            },
            bounce: false,
        },
        SemanticInstruction::Split,
    ] {
        let bank = BTreeMap::from([(name('a'), macro_value(vec![forbidden]))]);
        let mut allocations = 0;
        let result = plan(
            &before,
            &original,
            vec![pause(true), call('a', 1)],
            &bank,
            |request| {
                allocations += 1;
                super::super::allocate(request)
            },
            |_, _| panic!("black sampled"),
        );
        assert!(result.unwrap_err().message.contains("only Zoom"));
        assert_eq!(allocations, 1);
        assert_eq!(before, fixture());
    }
}

#[test]
fn scoped_pause_exact_pool_and_provider_failures_publish_no_partial_plan() {
    let before = fixture();
    for bad in 0..5 {
        let result = plan(
            &before,
            &entry(),
            vec![pause(true)],
            &BTreeMap::new(),
            |request| {
                let mut allocation = super::super::allocate(request)?;
                let SemanticAllocation::InsertOccurrencePause {
                    new_revision,
                    id,
                    identities,
                } = &mut allocation
                else {
                    panic!()
                };
                match bad {
                    0 => {
                        identities.isolation.nodes.pop();
                    }
                    1 => {
                        identities.isolation.marks.pop();
                    }
                    2 => {
                        identities.split.nodes.push(node("extra"));
                    }
                    3 => *id = node("a"),
                    4 => *new_revision = before.revision_id().clone(),
                    _ => unreachable!(),
                }
                Ok(allocation)
            },
            |_, _| panic!("black sampled"),
        );
        assert!(result.is_err(), "case{bad}");
    }
    let result = plan(
        &before,
        &entry(),
        vec![pause(false)],
        &BTreeMap::new(),
        |_| panic!("failed provider allocated"),
        |_, _| Err(pause_unavailable()),
    );
    assert!(result.is_err());
    assert_eq!(before, fixture());
}

#[test]
fn scoped_pause_at_inspected_owner_seams_preserves_captured_root_selection_beside_siblings() {
    let mut before = fixture();
    before.nodes.insert(node("prefix"), hold(3));
    before.nodes.insert(node("suffix"), hold(3));
    let NodeKind::Sequence { children } = &mut before.nodes.get_mut(&node("root")).unwrap().kind
    else {
        panic!()
    };
    *children = vec![node("prefix"), node("outer"), node("suffix")];
    before.validate().unwrap();
    for outer in [0, 2] {
        for local in [0, 18] {
            let instance = InstancePath {
                node: node("body"),
                repeats: vec![RepeatInstance {
                    node: node("outer"),
                    iteration: iteration("outer", outer),
                }],
            };
            let mut context = entry();
            context.cursor = ProjectFrame(3 + 18 * i64::from(outer) + local);
            let scoped = context.scoped.as_mut().unwrap();
            scoped.target = ScopedNodeTarget {
                node: node("body"),
                repeats: vec![RepeatEditStep {
                    repeat: node("outer"),
                    branch: RepeatEditBranch::Play {
                        iteration: iteration("outer", outer),
                    },
                }],
            };
            scoped.presentation = instance;
            let result = plan(
                &before,
                &context,
                vec![pause(true)],
                &BTreeMap::new(),
                super::super::allocate,
                |_, _| panic!("black sampled"),
            )
            .unwrap();
            let mut expected = context.clone();
            expected.scoped = result.context.scoped.clone();
            assert_eq!(result.context, expected);
            assert_eq!(result.context.selected_child, Some(node("outer")));
            assert_eq!(result.context.cursor, context.cursor);
        }
    }
}

#[test]
fn scoped_pause_rejects_non_freeze_picture_or_non_silent_host_provider_before_allocating() {
    let before = fixture();
    for bad_picture in [false, true] {
        let result = plan(
            &before,
            &entry(),
            vec![pause(false)],
            &BTreeMap::new(),
            |_| panic!("unsupported provider allocated"),
            |_, _| {
                let mut provider = background();
                if bad_picture {
                    provider.video = crate::HoldVideo::Accepted {
                        asset: crate::AssetId::new("unexpected-provider").unwrap(),
                        frames: range(0, 3),
                    };
                } else {
                    provider.audio = HoldAudio::Tone {
                        frequency_hz: 1000,
                        level: crate::GainDb::new(0).unwrap(),
                    };
                }
                Ok(provider)
            },
        );
        assert!(
            result
                .unwrap_err()
                .message
                .contains("silent freeze or background")
        );
    }
}
