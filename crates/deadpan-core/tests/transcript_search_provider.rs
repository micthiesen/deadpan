//! Pure semantic search tests. The fake host supplies proven geometry; no
//! transcript parser, Store, media decoder or analysis model is involved.

#[path = "range_set/fixture.rs"]
mod fixture;

use std::{collections::BTreeMap, num::NonZeroU32, sync::Arc};

use deadpan_core::*;
use fixture::*;

fn context() -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("root"),
        cursor: ProjectFrame(0),
        selected_child: Some(node("source")),
        visual_selection: None,
    }
}

fn query() -> SemanticTranscriptQuery {
    SemanticTranscriptQuery::new("hello world").unwrap()
}

fn allocate(request: SemanticAllocationRequest) -> Result<SemanticAllocation, EditError> {
    Ok(match request {
        SemanticAllocationRequest::Yank { step_index } => SemanticAllocation::Yank {
            capture_revision: revision(&format!("search-capture-{step_index}")),
        },
        SemanticAllocationRequest::DeleteRange {
            step_index,
            required_nodes,
        } => SemanticAllocation::DeleteRange {
            new_revision: revision(&format!("search-cut-{step_index}")),
            identities: SplitIdentities {
                nodes: (0..required_nodes)
                    .map(|index| node(&format!("search-cut-{step_index}-split-{index}")))
                    .collect(),
            },
        },
        SemanticAllocationRequest::PasteEdited {
            step_index,
            requirements,
            required_split_ids,
        } => SemanticAllocation::PasteEdited {
            new_revision: revision(&format!("search-duplicate-{step_index}")),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..requirements.nodes)
                        .map(|index| node(&format!("search-duplicate-{step_index}-node-{index}")))
                        .collect(),
                    marks: (0..requirements.marks)
                        .map(|index| {
                            MarkId::new(format!("search-duplicate-{step_index}-mark-{index}"))
                                .unwrap()
                        })
                        .collect(),
                },
                aliases: (0..requirements.aliases)
                    .map(|index| node(&format!("search-duplicate-{step_index}-alias-{index}")))
                    .collect(),
            },
            split_identities: SplitIdentities {
                nodes: (0..required_split_ids)
                    .map(|index| node(&format!("search-duplicate-{step_index}-split-{index}")))
                    .collect(),
            },
        },
        other => panic!("unexpected allocation {other:?}"),
    })
}

#[derive(Default)]
struct Provider {
    replies: Vec<Vec<FrameRange>>,
    calls: Vec<(RevisionId, i64, String)>,
    stale: Option<SemanticTranscriptMatches>,
    allow_timeline: bool,
    timeline_calls: Vec<(RevisionId, i64)>,
    word_requirements: usize,
    refuse_words: bool,
}

impl SemanticSpeechProvider for Provider {
    fn require_words(&mut self) -> Result<(), EditError> {
        self.word_requirements += 1;
        if self.refuse_words {
            return Err(EditError {
                code: EditErrorCode::SelectionUnavailable,
                message: "fake host refuses transcript word authority".into(),
                current_revision: None,
            });
        }
        Ok(())
    }

    fn timeline(&mut self, document: &ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError> {
        assert!(
            self.allow_timeline,
            "exact search must not require a per-picture speech timeline"
        );
        let duration = document.duration().unwrap().frames();
        self.timeline_calls
            .push((document.revision_id().clone(), duration));
        let start = if duration == 24 { 2 } else { 7 };
        Ok(Arc::new(
            SpeechTimeline::new(vec![
                SpeechRun {
                    range: range(start, start + 2),
                    word: 0,
                    sentence: 0,
                },
                SpeechRun {
                    range: range(start + 6, start + 8),
                    word: 1,
                    sentence: 1,
                },
            ])?
            .with_pauses(vec![range(2, 4)])?
            .with_shots(vec![ShotRun {
                range: range(2, 6),
                shot: 0,
            }])?,
        ))
    }

    fn transcript_search(
        &mut self,
        document: &ProjectDocument,
        context: &SemanticContext,
        query: &SemanticTranscriptQuery,
    ) -> Result<SemanticTranscriptMatches, EditError> {
        self.calls.push((
            document.revision_id().clone(),
            document.duration().unwrap().frames(),
            query.as_str().to_owned(),
        ));
        if let Some(stale) = &self.stale {
            return Ok(stale.clone());
        }
        let ranges = self.replies.remove(0);
        SemanticTranscriptMatches::new(document, context, query, ranges)
    }
}

fn plan(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    provider: &mut Provider,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_provider(
        document,
        context,
        &SemanticProgram::new(instructions).unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("search-outer"),
        allocate,
        |_, _| panic!("no Original register"),
        provider,
        |_, _| panic!("no pause"),
        None,
        |_, _| panic!("no framing"),
    )
}

#[test]
fn transcript_query_preserves_raw_string_and_rejects_empty_or_oversize_wire() {
    let query = SemanticTranscriptQuery::new("  HELLO, world!  ").unwrap();
    assert_eq!(query.as_str(), "  HELLO, world!  ");
    assert_eq!(serde_json::to_value(&query).unwrap(), "  HELLO, world!  ");
    assert_eq!(
        serde_json::from_value::<SemanticTranscriptQuery>(serde_json::json!(query.as_str()))
            .unwrap(),
        query
    );
    for text in ["".to_owned(), " \t\n".to_owned(), "é".repeat(2049)] {
        assert!(SemanticTranscriptQuery::new(text.clone()).is_err());
        assert!(
            serde_json::from_value::<SemanticTranscriptQuery>(serde_json::json!(text)).is_err()
        );
    }
    assert!(SemanticTranscriptQuery::new("x".repeat(MAX_SEMANTIC_TRANSCRIPT_QUERY_BYTES)).is_ok());
    let instruction = SemanticInstruction::SelectTranscriptSearch {
        query: query.clone(),
    };
    assert!(instruction.uses_speech());
    assert_eq!(
        serde_json::to_value(&instruction).unwrap(),
        serde_json::json!({
            "type": "select_transcript_search", "query": "  HELLO, world!  ",
        })
    );
    assert_eq!(
        serde_json::from_value::<SemanticInstruction>(serde_json::to_value(&instruction).unwrap())
            .unwrap(),
        instruction
    );
}

#[test]
fn search_selection_keeps_query_and_finished_union_while_cursor_moves_independently() {
    let document = fixture();
    let mut provider = Provider {
        replies: vec![vec![range(2, 6), range(10, 14)]],
        ..Provider::default()
    };
    let plan = plan(
        &document,
        &context(),
        vec![
            SemanticInstruction::SelectTranscriptSearch { query: query() },
            SemanticInstruction::MoveFrames {
                forward: false,
                count: NonZeroU32::new(9).unwrap(),
            },
        ],
        &mut provider,
    )
    .unwrap();
    assert_eq!(plan.context.cursor, ProjectFrame(5));
    assert_eq!(
        plan.context.visual_selection,
        Some(SemanticVisualSelection::TranscriptSearch {
            query: query(),
            ranges: vec![range(2, 6), range(10, 14)],
        })
    );
    assert_eq!(
        plan.context
            .visual_selection
            .as_ref()
            .unwrap()
            .effective_selector(),
        SemanticSelector::TranscriptSearch { query: query() }
    );
    assert_eq!(
        plan.trace[0].resolved_ranges,
        vec![range(2, 6), range(10, 14)]
    );
    assert_eq!(
        provider.calls,
        vec![(document.revision_id().clone(), 24, "hello world".into())]
    );
    assert!(plan.request.is_none());
    assert_eq!(plan.document, document);
}

#[test]
fn consuming_a_finished_search_resolves_query_again_instead_of_capturing_displayed_geometry() {
    let document = fixture();
    let mut context = context();
    context.visual_selection = Some(SemanticVisualSelection::TranscriptSearch {
        query: query(),
        ranges: vec![range(1, 2)],
    });
    let mut provider = Provider {
        replies: vec![vec![range(2, 6), range(10, 14)]],
        ..Provider::default()
    };
    let plan = plan(
        &document,
        &context,
        vec![SemanticInstruction::YankSelection {
            register: RegisterName::new('a').unwrap(),
        }],
        &mut provider,
    )
    .unwrap();
    assert_eq!(provider.calls.len(), 1);
    assert_eq!(
        plan.trace[0].resolved_ranges,
        vec![range(2, 6), range(10, 14)]
    );
    assert_eq!(
        plan.context.visual_selection,
        Some(SemanticVisualSelection::TranscriptSearch {
            query: query(),
            ranges: vec![range(2, 6), range(10, 14)],
        })
    );
    let RegisterValue::EditedSet { set } =
        plan.register_writes[&RegisterName::new('a').unwrap()].as_ref()
    else {
        panic!("search capture must retain separate components")
    };
    assert_eq!(set.duration(), frames(8));
    assert_eq!(set.components().len(), 2);
    assert_eq!(set.components()[0].range(), range(2, 6));
    assert_eq!(set.components()[1].range(), range(10, 14));
    assert_eq!(plan.document, document);
}

#[test]
fn each_search_cut_uses_staged_document_and_one_reversible_union_transaction() {
    let document = fixture();
    let frozen = document.to_json().unwrap();
    let mut provider = Provider {
        replies: vec![vec![range(2, 6), range(10, 14)], vec![range(1, 3)]],
        ..Provider::default()
    };
    let cut = SemanticInstruction::Cut {
        selector: SemanticSelector::TranscriptSearch { query: query() },
        register: RegisterName::new('a').unwrap(),
    };
    let plan = plan(&document, &context(), vec![cut.clone(), cut], &mut provider).unwrap();
    assert_eq!(
        provider.calls[0],
        (document.revision_id().clone(), 24, "hello world".into())
    );
    assert_eq!(
        provider.calls[1],
        (revision("search-cut-2"), 16, "hello world".into())
    );
    assert_eq!(plan.document.duration().unwrap(), frames(14));
    let request = plan.request.unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!("one Compound")
    };
    assert_eq!(transaction.steps().len(), 5);
    let deleted = transaction
        .steps()
        .iter()
        .filter_map(|step| step.edit())
        .map(|leaf| match leaf.command.as_command() {
            Command::DeleteRange { range, .. } => *range,
            other => panic!("unexpected leaf {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(deleted, vec![range(10, 14), range(2, 6), range(1, 3)]);
    let transaction = apply(&document, &request).unwrap();
    let after = transaction.forward.apply(&document).unwrap();
    assert_eq!(after, plan.document);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), document);
    assert_eq!(document.to_json().unwrap(), frozen);
}

#[test]
fn legacy_timeline_closure_refuses_search_before_any_allocation_or_state_change() {
    let document = fixture();
    let error = plan_semantic(
        &document,
        &context(),
        &SemanticProgram::new(vec![SemanticInstruction::Cut {
            selector: SemanticSelector::TranscriptSearch { query: query() },
            register: RegisterName::unnamed(),
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("search-outer"),
        |_| panic!("missing search provider cannot allocate"),
        |_, _| panic!("no Original register"),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("qualified search provider"));
}

#[test]
fn word_motions_and_search_share_one_provider_across_staged_edits() {
    let document = fixture();
    let mut provider = Provider {
        replies: vec![vec![range(2, 6), range(10, 14)]],
        allow_timeline: true,
        ..Provider::default()
    };
    let motion = SemanticInstruction::MoveWords {
        forward: true,
        count: NonZeroU32::new(1).unwrap(),
        end: false,
    };
    let plan = plan(
        &document,
        &context(),
        vec![
            motion.clone(),
            SemanticInstruction::Cut {
                selector: SemanticSelector::TranscriptSearch { query: query() },
                register: RegisterName::unnamed(),
            },
            motion,
        ],
        &mut provider,
    )
    .unwrap();
    assert_eq!(
        provider.timeline_calls,
        vec![
            (document.revision_id().clone(), 24),
            (revision("search-cut-2"), 16),
        ]
    );
    assert_eq!(provider.calls.len(), 1);
    assert_eq!(plan.trace[0].after.cursor, ProjectFrame(2));
    assert_eq!(plan.context.cursor, ProjectFrame(7));
}

#[test]
fn wrong_query_and_stale_revision_matches_refuse_without_partial_result() {
    let document = fixture();
    for different_query in [true, false] {
        let matched_document = if different_query {
            document.clone()
        } else {
            edit(
                &document,
                "search-different-revision",
                Command::Rename {
                    node: node("source"),
                    label: "Different staged revision".into(),
                },
            )
        };
        let matched_query = if different_query {
            SemanticTranscriptQuery::new("other query").unwrap()
        } else {
            query()
        };
        let stale = SemanticTranscriptMatches::new(
            &matched_document,
            &context(),
            &matched_query,
            vec![range(2, 6)],
        )
        .unwrap();
        let mut provider = Provider {
            stale: Some(stale),
            ..Provider::default()
        };
        let frozen = document.to_json().unwrap();
        let error = plan(
            &document,
            &context(),
            vec![SemanticInstruction::Cut {
                selector: SemanticSelector::TranscriptSearch { query: query() },
                register: RegisterName::unnamed(),
            }],
            &mut provider,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        assert!(
            error
                .message
                .contains("another staged document, scope or query")
        );
        assert_eq!(document.to_json().unwrap(), frozen);
    }
}

#[test]
fn scalar_consumers_and_invalid_host_unions_refuse_without_using_hulls() {
    let document = fixture();
    for ranges in [
        vec![],
        vec![range(2, 6), range(5, 8)],
        vec![range(22, 25)],
        vec![range(2, 6), range(6, 8)],
    ] {
        assert!(SemanticTranscriptMatches::new(&document, &context(), &query(), ranges).is_err());
    }
    let mut provider = Provider::default();
    let error = plan(
        &document,
        &context(),
        vec![SemanticInstruction::Group {
            selector: SemanticSelector::TranscriptSearch { query: query() },
            label: "No hull".into(),
        }],
        &mut provider,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(provider.calls.is_empty());
}

#[test]
fn pause_and_shot_operations_do_not_require_transcript_word_authority() {
    let document = fixture();
    let mut context = context();
    context.cursor = ProjectFrame(2);
    let count = NonZeroU32::new(1).unwrap();
    let mut instructions = vec![
        SemanticInstruction::MovePauses {
            forward: true,
            count,
        },
        SemanticInstruction::MoveShots {
            forward: true,
            count,
        },
    ];
    for object in [SpeechObject::InnerPause, SpeechObject::InnerShot] {
        instructions.push(SemanticInstruction::SelectSpeech { object });
        instructions.push(SemanticInstruction::Yank {
            selector: SemanticSelector::Speech { object },
            register: RegisterName::unnamed(),
        });
    }
    for motion in [
        SemanticMotion::Pauses {
            forward: true,
            count,
        },
        SemanticMotion::Shots {
            forward: true,
            count,
        },
    ] {
        instructions.push(SemanticInstruction::Yank {
            selector: SemanticSelector::Motion { motion },
            register: RegisterName::unnamed(),
        });
    }
    for instruction in instructions {
        let mut provider = Provider {
            allow_timeline: true,
            refuse_words: true,
            ..Provider::default()
        };
        let mut entry = context.clone();
        if matches!(
            instruction,
            SemanticInstruction::MovePauses { .. }
                | SemanticInstruction::MoveShots { .. }
                | SemanticInstruction::Yank {
                    selector: SemanticSelector::Motion { .. },
                    ..
                }
        ) {
            entry.cursor = ProjectFrame(0);
        }
        plan(&document, &entry, vec![instruction], &mut provider).unwrap();
        assert_eq!(provider.word_requirements, 0);
        assert_eq!(provider.timeline_calls.len(), 1);
        assert!(provider.calls.is_empty());
    }
}

#[test]
fn cached_pause_or_shot_timeline_still_requires_words_for_later_word_consumers() {
    let document = fixture();
    let count = NonZeroU32::new(1).unwrap();
    let mut words = vec![
        SemanticInstruction::MoveWords {
            forward: true,
            count,
            end: false,
        },
        SemanticInstruction::MoveSentences {
            forward: true,
            count,
        },
    ];
    for object in [SpeechObject::InnerWord, SpeechObject::InnerSentence] {
        words.push(SemanticInstruction::SelectSpeech { object });
        words.push(SemanticInstruction::Yank {
            selector: SemanticSelector::Speech { object },
            register: RegisterName::unnamed(),
        });
    }
    for motion in [
        SemanticMotion::Words {
            forward: true,
            count,
            end: false,
        },
        SemanticMotion::Sentences {
            forward: true,
            count,
        },
    ] {
        words.push(SemanticInstruction::Yank {
            selector: SemanticSelector::Motion { motion },
            register: RegisterName::unnamed(),
        });
    }
    for prefix in [
        SemanticInstruction::MovePauses {
            forward: true,
            count,
        },
        SemanticInstruction::MoveShots {
            forward: true,
            count,
        },
    ] {
        for instruction in &words {
            let mut provider = Provider {
                allow_timeline: true,
                ..Provider::default()
            };
            plan(
                &document,
                &context(),
                vec![prefix.clone(), instruction.clone()],
                &mut provider,
            )
            .unwrap();
            assert_eq!(provider.word_requirements, 1, "{instruction:?}");
            assert_eq!(
                provider.timeline_calls.len(),
                1,
                "the pause/shot timeline was cached"
            );
        }
    }
}

#[test]
fn all_word_selector_operators_honor_denied_authority_after_cached_nonword_analysis() {
    let document = fixture();
    let frozen = document.to_json().unwrap();
    let count = NonZeroU32::new(1).unwrap();
    let selectors = [
        SemanticSelector::Speech {
            object: SpeechObject::InnerWord,
        },
        SemanticSelector::Speech {
            object: SpeechObject::InnerSentence,
        },
        SemanticSelector::Motion {
            motion: SemanticMotion::Words {
                forward: true,
                count,
                end: false,
            },
        },
        SemanticSelector::Motion {
            motion: SemanticMotion::Sentences {
                forward: true,
                count,
            },
        },
    ];
    for selector in selectors {
        for instruction in [
            SemanticInstruction::Yank {
                selector: selector.clone(),
                register: RegisterName::unnamed(),
            },
            SemanticInstruction::Cut {
                selector: selector.clone(),
                register: RegisterName::unnamed(),
            },
            SemanticInstruction::Group {
                selector: selector.clone(),
                label: "Words".into(),
            },
            SemanticInstruction::Repeat {
                selector: selector.clone(),
                plays: count,
                escalation: None,
            },
            SemanticInstruction::Duplicate { selector },
        ] {
            let mut provider = Provider {
                allow_timeline: true,
                refuse_words: true,
                ..Provider::default()
            };
            let error = plan(
                &document,
                &context(),
                vec![
                    SemanticInstruction::MovePauses {
                        forward: true,
                        count,
                    },
                    instruction,
                ],
                &mut provider,
            )
            .unwrap_err();
            assert_eq!(error.message, "fake host refuses transcript word authority");
            assert_eq!(provider.word_requirements, 1);
            assert_eq!(provider.timeline_calls.len(), 1);
            assert_eq!(document.to_json().unwrap(), frozen);
        }
    }
}

#[test]
fn duplicate_prepares_fresh_word_and_sentence_selector_timelines() {
    let document = fixture();
    let mut context = context();
    context.cursor = ProjectFrame(2);
    let count = NonZeroU32::new(1).unwrap();
    assert!(
        SemanticProgram::new(vec![SemanticInstruction::Duplicate {
            selector: SemanticSelector::Motion {
                motion: SemanticMotion::Words {
                    forward: false,
                    count,
                    end: true
                },
            },
        }])
        .is_err(),
        "Duplicate preserves the forward-only word-end vocabulary"
    );
    for (selector, added) in [
        (
            SemanticSelector::Speech {
                object: SpeechObject::InnerWord,
            },
            2,
        ),
        (
            SemanticSelector::Speech {
                object: SpeechObject::InnerSentence,
            },
            2,
        ),
        (
            SemanticSelector::Motion {
                motion: SemanticMotion::Words {
                    forward: true,
                    count,
                    end: false,
                },
            },
            6,
        ),
        (
            SemanticSelector::Motion {
                motion: SemanticMotion::Sentences {
                    forward: true,
                    count,
                },
            },
            6,
        ),
    ] {
        let instruction = SemanticInstruction::Duplicate { selector };
        assert!(instruction.uses_speech());
        let mut provider = Provider {
            allow_timeline: true,
            ..Provider::default()
        };
        let result = plan(&document, &context, vec![instruction], &mut provider).unwrap();
        assert_eq!(provider.word_requirements, 1);
        assert_eq!(provider.timeline_calls.len(), 1);
        assert_eq!(result.document.duration().unwrap(), frames(24 + added));
        let transaction = apply(&document, result.request.as_ref().unwrap()).unwrap();
        assert_eq!(
            transaction.inverse.apply(&result.document).unwrap(),
            document
        );
    }
}

#[test]
fn exact_search_consumption_is_independent_of_word_timeline_authority_hook() {
    let document = fixture();
    let mut provider = Provider {
        replies: vec![vec![range(2, 6)]],
        refuse_words: true,
        ..Provider::default()
    };
    plan(
        &document,
        &context(),
        vec![SemanticInstruction::SelectTranscriptSearch { query: query() }],
        &mut provider,
    )
    .unwrap();
    assert_eq!(provider.word_requirements, 0);
    assert!(provider.timeline_calls.is_empty());
    assert_eq!(provider.calls.len(), 1);
}
