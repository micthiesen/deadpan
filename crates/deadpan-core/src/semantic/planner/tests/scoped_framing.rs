use super::*;
use crate::{
    Anchor, AnchorLossPolicy, BoundaryAnchor, ExactRatio, FramingPose, InsertionBias, InstancePath,
    IterationId, IterationOrder, Mark, MarkState, RepeatEditBranch, RepeatEditStep, RepeatInstance,
    ScopedNodeTarget, SemanticScopedContext, SemanticZoomCenter,
    framing_zoom::{ZoomCurve, ZoomKind},
};

#[path = "scoped_pause.rs"]
mod scoped_pause;

fn repeated(id: &str, child: &str, plays: u32) -> BeatNode {
    let mut result = BeatNode::sequence(id, vec![]);
    result.kind = NodeKind::Repeat {
        child: node(child),
        iterations: IterationOrder::new(revision(id), plays).unwrap(),
        gap: None,
        escalation: None,
    };
    result
}

fn fixture() -> ProjectDocument {
    let mut document = tree(
        &["outer"],
        vec![
            ("outer", repeated("outer", "body", 3)),
            (
                "body",
                BeatNode::sequence("body", vec![node("lead"), node("inner"), node("tail")]),
            ),
            ("lead", hold(2)),
            ("inner", repeated("inner", "inside", 2)),
            (
                "inside",
                BeatNode::sequence("inside", vec![node("a"), node("b")]),
            ),
            ("a", hold(4)),
            ("b", hold(3)),
            ("tail", hold(2)),
        ],
    );
    document.marks.insert(
        MarkId::new("owned").unwrap(),
        Mark {
            owner: node("a"),
            label: "owned".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("a"),
                    position: ExactRatio::ONE,
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
            fragments: vec![],
        },
    );
    document.validate().unwrap();
    document
}

fn iteration(id: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision(id),
        ordinal,
    }
}

fn presentation(outer: u32, inner: u32) -> InstancePath {
    InstancePath {
        node: node("a"),
        repeats: vec![
            RepeatInstance {
                node: node("outer"),
                iteration: iteration("outer", outer),
            },
            RepeatInstance {
                node: node("inner"),
                iteration: iteration("inner", inner),
            },
        ],
    }
}

fn entry() -> SemanticContext {
    let presentation = presentation(1, 1);
    SemanticContext {
        selected_child: Some(node("outer")),
        scoped: Some(SemanticScopedContext {
            root: node("outer"),
            target: ScopedNodeTarget {
                node: node("a"),
                repeats: presentation
                    .repeats
                    .iter()
                    .map(|actual| RepeatEditStep {
                        repeat: actual.node.clone(),
                        branch: RepeatEditBranch::Play {
                            iteration: actual.iteration.clone(),
                        },
                    })
                    .collect(),
            },
            presentation,
        }),
        ..context("root", 28)
    }
}

fn zoom(scale: i64) -> SemanticInstruction {
    SemanticInstruction::Zoom {
        kind: ZoomKind::Smash {
            scale: ExactRatio::integer(scale),
        },
        center: SemanticZoomCenter::Keep,
    }
}

fn state(
    staged: &ProjectDocument,
    site: &SemanticFramingSite,
) -> Result<SemanticFramingState, EditError> {
    let instance = site
        .presentation
        .as_ref()
        .expect("scoped resolver needs an exact presentation");
    assert_eq!(site.node, instance.node);
    instance.validate(staged).unwrap();
    let duration = staged.node_duration(&site.node).unwrap();
    Ok(SemanticFramingState {
        current: staged.nodes()[&site.node]
            .framing
            .as_ref()
            .map_or(Ok(FramingPose::identity()), |framing| {
                framing.evaluate(ExactRatio::new(1, 2).unwrap(), duration)
            })
            .unwrap(),
        duration,
        original_center: site
            .original_center
            .then_some([ExactRatio::new(1, 2).unwrap(); 2]),
    })
}

fn resolve(
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
        revision(&format!("{}-outer", document.revision_id())),
        allocator,
        no_original,
        |_| Err(speech_unavailable()),
        |_, _| Err(pause_unavailable()),
        None,
        state,
    )
}

fn allocator(
    prefix: &str,
    request: SemanticAllocationRequest,
) -> Result<SemanticAllocation, EditError> {
    let SemanticAllocationRequest::Isolation {
        step_index,
        nodes,
        marks,
    } = request
    else {
        panic!("scoped zoom must never request ordinary allocation: {request:?}")
    };
    Ok(SemanticAllocation::Isolation {
        new_revision: revision(&format!("{prefix}-leaf-{step_index}")),
        identities: crate::OccurrenceIdentities {
            nodes: (0..nodes)
                .map(|index| node(&format!("{prefix}-node-{step_index}-{index}")))
                .collect(),
            marks: (0..marks)
                .map(|index| MarkId::new(format!("{prefix}-mark-{step_index}-{index}")).unwrap())
                .collect(),
        },
    })
}

#[test]
fn scoped_zoom_nested_calls_map_one_play_once_and_keep_templates_other_plays_and_one_inverse() {
    let document = fixture();
    let context = entry();
    let creep = SemanticInstruction::Zoom {
        kind: ZoomKind::Creep {
            from: None,
            to: ExactRatio::integer(3),
            curve: ZoomCurve::Linear,
        },
        center: SemanticZoomCenter::OriginalCenter,
    };
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![creep.clone()])),
    ]);
    let frozen = bank.clone();
    let mut requests = vec![];
    let plan = resolve(
        &document,
        &context,
        vec![zoom(2), call('a', 1)],
        &bank,
        |request| {
            requests.push(request.clone());
            allocator("first", request)
        },
    )
    .unwrap();
    assert_eq!(
        requests,
        vec![
            SemanticAllocationRequest::Isolation {
                step_index: 0,
                nodes: 10,
                marks: 2
            },
            SemanticAllocationRequest::Isolation {
                step_index: 1,
                nodes: 0,
                marks: 0
            },
        ]
    );
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert_ne!(mapped.target.node, node("a"));
    assert_eq!(mapped.root, node("outer"));
    assert_ne!(mapped.target.repeats[1].repeat, node("inner"));
    assert!(
        mapped
            .target
            .matches_instance(&plan.document, &mapped.presentation)
            .unwrap()
    );
    assert_eq!(plan.context.parent, context.parent);
    assert_eq!(plan.context.cursor, context.cursor);
    assert_eq!(plan.context.selected_child, context.selected_child);
    assert_eq!(plan.context.visual_selection, context.visual_selection);
    assert!(plan.register_writes.is_empty());
    assert_eq!(bank, frozen);
    assert_eq!(plan.trace.len(), 4);
    assert_eq!(plan.trace[0].before.scoped, context.scoped);
    assert_eq!(plan.trace[0].after.scoped, plan.trace[3].before.scoped);
    assert_eq!(plan.trace[1].after.scoped, plan.context.scoped);
    assert_eq!(plan.trace[3].instruction, creep);
    for (id, before) in document.nodes() {
        assert_eq!(&plan.document.nodes()[id], before, "shared template {id}");
    }
    assert_eq!(plan.document.nodes().len(), document.nodes().len() + 10);
    assert_eq!(plan.document.marks().len(), document.marks().len() + 2);
    let request = plan.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!()
    };
    for outer in 0..3 {
        for inner in 0..2 {
            let mut instance = presentation(outer, inner);
            let mut staged = document.clone();
            for step in transaction.steps() {
                let leaf = step.edit().unwrap();
                let prepared = crate::prepare_scoped_edit(&staged, &leaf.request(&staged)).unwrap();
                instance = prepared.map_instance(&staged, &instance).unwrap();
                staged = prepared.document;
            }
            assert_eq!(
                plan.document.nodes()[&instance.node].framing.is_some(),
                outer == 1 && inner == 1
            );
        }
    }
    let replay = crate::replay_compound::<EditError>(&document, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
    let restored = ProjectDocument::from_json(&plan.document.to_json().unwrap()).unwrap();
    assert!(
        mapped
            .target
            .matches_instance(&restored, &mapped.presentation)
            .unwrap()
    );
}

#[test]
fn scoped_zoom_reuses_existing_override_and_exact_noop_never_allocates_or_authors() {
    let document = fixture();
    let context = entry();
    let none = resolve(
        &document,
        &context,
        vec![SemanticInstruction::Zoom {
            kind: ZoomKind::Off,
            center: SemanticZoomCenter::Keep,
        }],
        &BTreeMap::new(),
        |_| panic!("no-op cannot allocate"),
    )
    .unwrap();
    assert!(none.request.is_none());
    assert_eq!(none.context, context);
    assert_eq!(none.document, document);
    let first = resolve(
        &document,
        &context,
        vec![zoom(2)],
        &BTreeMap::new(),
        |request| allocator("initial", request),
    )
    .unwrap();
    let no_change = resolve(
        &first.document,
        &first.context,
        vec![zoom(2)],
        &BTreeMap::new(),
        |_| panic!("same scale cannot isolate"),
    )
    .unwrap();
    assert!(no_change.request.is_none());
    let mut requests = vec![];
    let second = resolve(
        &first.document,
        &first.context,
        vec![zoom(4)],
        &BTreeMap::new(),
        |request| {
            requests.push(request.clone());
            allocator("later", request)
        },
    )
    .unwrap();
    assert_eq!(
        requests,
        vec![SemanticAllocationRequest::Isolation {
            step_index: 0,
            nodes: 0,
            marks: 0
        }]
    );
    assert_eq!(second.context.scoped, first.context.scoped);
    assert_eq!(second.document.nodes().len(), first.document.nodes().len());
    assert_eq!(second.document.marks(), first.document.marks());
    assert_eq!(second.document.overrides(), first.document.overrides());
}

#[test]
fn scoped_zoom_refuses_default_mixed_absent_foreign_and_mismatched_authority_before_allocation() {
    let document = fixture();
    for variant in 0..10 {
        let mut context = entry();
        let scoped = context.scoped.as_mut().unwrap();
        match variant {
            0 => scoped.target.repeats.clear(),
            1 => scoped.target.repeats[0].branch = RepeatEditBranch::Default,
            2 => scoped.target.repeats[1].branch = RepeatEditBranch::Default,
            3 => scoped.root = node("inner"),
            4 => scoped.root = node("lead"),
            5 => scoped.presentation.repeats[0].iteration = iteration("outer", 0),
            6 => scoped.presentation.node = node("b"),
            7 => {
                context.visual_selection = Some(SemanticVisualSelection::Time {
                    anchor: ProjectFrame(28),
                    head: ProjectFrame(29),
                    extending: false,
                })
            }
            8 => {
                context.original = Some(SemanticOriginalContext {
                    asset: crate::AssetId::new("missing").unwrap(),
                    qualification: crate::SourceQualificationId::new("a".repeat(64)).unwrap(),
                    ordinal: crate::SourceFrameId(0),
                })
            }
            9 => {
                for step in &mut scoped.target.repeats {
                    step.branch = RepeatEditBranch::Default;
                }
            }
            _ => unreachable!(),
        }
        assert!(
            resolve(
                &document,
                &context,
                vec![zoom(2)],
                &BTreeMap::new(),
                |_| panic!("invalid authority allocated")
            )
            .is_err(),
            "variant {variant}"
        );
    }
    let isolated = resolve(
        &document,
        &entry(),
        vec![zoom(2)],
        &BTreeMap::new(),
        |request| allocator("isolated", request),
    )
    .unwrap();
    // A template address cannot claim a presentation now owned by an override.
    assert!(
        resolve(
            &isolated.document,
            &entry(),
            vec![zoom(3)],
            &BTreeMap::new(),
            |_| panic!("dormant template allocated")
        )
        .is_err()
    );
    let mut empty = fixture();
    empty
        .nodes
        .insert(node("a"), BeatNode::sequence("empty", vec![]));
    empty.marks.clear();
    empty.validate().unwrap();
    let error = resolve(&empty, &entry(), vec![zoom(2)], &BTreeMap::new(), |_| {
        panic!("an empty owner cannot allocate isolation")
    })
    .unwrap_err();
    assert!(error.message.contains("empty scoped owner"));
}

#[test]
fn scoped_macro_allowlist_applies_inside_calls_and_late_failure_publishes_nothing() {
    let document = fixture();
    let context = entry();
    let forbidden = [
        motion(true, 1),
        SemanticInstruction::BeginSelection,
        SemanticInstruction::SetFraming { framing: None },
        cut(1, 'z'),
    ];
    for instruction in forbidden {
        let bank = BTreeMap::from([
            (name('a'), macro_value(vec![call('b', 1)])),
            (name('b'), macro_value(vec![instruction.clone()])),
        ]);
        assert!(
            resolve(&document, &context, vec![instruction], &bank, |_| panic!(
                "forbidden instruction allocated"
            ))
            .unwrap_err()
            .message
            .contains("only Zoom")
        );
        let frozen = bank.clone();
        let mut allocations = 0;
        let error = resolve(
            &document,
            &context,
            vec![zoom(2), call('a', 1)],
            &bank,
            |request| {
                allocations += 1;
                allocator("late", request)
            },
        )
        .unwrap_err();
        assert!(error.message.contains("only Zoom"));
        assert_eq!(allocations, 1);
        assert_eq!(bank, frozen);
        assert_eq!(document, fixture());
    }
}

#[test]
fn scoped_zoom_requires_exact_unique_node_mark_and_revision_allocations() {
    let document = fixture();
    for variant in 0..8 {
        let error = resolve(
            &document,
            &entry(),
            vec![zoom(2)],
            &BTreeMap::new(),
            |request| {
                let mut allocation = allocator("pool", request)?;
                let SemanticAllocation::Isolation {
                    new_revision,
                    identities,
                } = &mut allocation
                else {
                    unreachable!()
                };
                match variant {
                    0 => {
                        identities.nodes.pop();
                    }
                    1 => {
                        identities.marks.pop();
                    }
                    2 => identities.nodes[0] = node("a"),
                    3 => identities.nodes[1] = identities.nodes[0].clone(),
                    4 => identities.marks[0] = MarkId::new("owned").unwrap(),
                    5 => identities.marks[1] = identities.marks[0].clone(),
                    6 => *new_revision = document.revision_id().clone(),
                    7 => *new_revision = revision(&format!("{}-outer", document.revision_id())),
                    _ => unreachable!(),
                }
                Ok(allocation)
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                error.code,
                EditErrorCode::InvalidCommand | EditErrorCode::IdentityConflict
            ),
            "variant {variant}: {error:?}"
        );
        assert_eq!(document, fixture());
    }
}

#[test]
fn scoped_context_wire_is_strict_and_never_part_of_saved_zoom_intent() {
    let context = entry().scoped.unwrap();
    let mut value = serde_json::to_value(&context).unwrap();
    assert_eq!(
        serde_json::from_value::<SemanticScopedContext>(value.clone()).unwrap(),
        context
    );
    value
        .as_object_mut()
        .unwrap()
        .insert("also".into(), serde_json::json!([]));
    assert!(serde_json::from_value::<SemanticScopedContext>(value).is_err());
    let saved = serde_json::to_value(program(vec![zoom(2)])).unwrap();
    let text = saved.to_string();
    assert!(!text.contains("outer"));
    assert!(!text.contains("presentation"));
    assert!(!text.contains("iteration"));
}
