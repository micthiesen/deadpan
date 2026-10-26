use super::*;
use crate::{
    Anchor, AnchorLossPolicy, BoundaryAnchor, ExactRatio, FramingPose, InsertionBias, InstancePath,
    IterationId, IterationOrder, Mark, MarkState, RepeatEditBranch, RepeatEditStep, RepeatInstance,
    ScopedNodeTarget, SemanticScopedContext, SemanticZoomCenter,
    framing_zoom::{ZoomCurve, ZoomKind},
};

#[path = "scoped_pause.rs"]
mod scoped_pause;

#[path = "scoped_hold_duration.rs"]
mod scoped_hold_duration;

#[path = "scoped_hold_audio.rs"]
mod scoped_hold_audio;

#[path = "scoped_gain.rs"]
mod scoped_gain;

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
        normalized_range: None,
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
    resolve_with_state(document, context, instructions, bank, allocator, state)
}

fn resolve_with_state(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    allocator: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    framing: impl FnMut(
        &ProjectDocument,
        &SemanticFramingSite,
    ) -> Result<SemanticFramingState, EditError>,
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
        framing,
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

fn ranged_context(anchor: i64, head: i64) -> SemanticContext {
    SemanticContext {
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(anchor),
            head: ProjectFrame(head),
            extending: false,
        }),
        ..entry()
    }
}

/// A pure test provider for this fixture's four-frame `a` occurrences. Real
/// hosts obtain this map from the decoder-free compiled picture plan.
fn ranged_state(
    staged: &ProjectDocument,
    site: &SemanticFramingSite,
) -> Result<SemanticFramingState, EditError> {
    let mut result = state(staged, site)?;
    if let Some(range) = site.root_range {
        let instance = site.presentation.as_ref().unwrap();
        let start = 18 * i64::from(instance.repeats[0].iteration.ordinal)
            + 2
            + 7 * i64::from(instance.repeats[1].iteration.ordinal);
        if range.start().0 < start
            || range.end().0 > start + 4
            || site.frame.0 < start
            || site.frame.0 >= start + 4
        {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "test range lies outside the concrete owner",
            ));
        }
        result.normalized_range = Some((
            ExactRatio::new(i128::from(range.start().0 - start), 4).unwrap(),
            ExactRatio::new(i128::from(range.end().0 - start), 4).unwrap(),
        ));
    }
    Ok(result)
}

#[test]
fn scoped_time_ranges_reach_the_host_exactly_and_keep_one_isolation_and_inverse() {
    let document = fixture();
    for (anchor, head) in [(28, 30), (30, 28)] {
        let context = ranged_context(anchor, head);
        let mut requests = Vec::new();
        let plan = resolve_with_state(
            &document,
            &context,
            vec![zoom(2)],
            &BTreeMap::new(),
            |request| {
                requests.push(request.clone());
                allocator("ranged", request)
            },
            |staged, site| {
                assert_eq!(
                    site.root_range,
                    Some(FrameRange::new(ProjectFrame(28), ProjectFrame(30)).unwrap())
                );
                ranged_state(staged, site)
            },
        )
        .unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(plan.context.visual_selection, context.visual_selection);
        assert_eq!(plan.context.cursor, context.cursor);
        assert_eq!(
            plan.trace[0].resolved_range,
            Some(FrameRange::new(ProjectFrame(28), ProjectFrame(30)).unwrap())
        );
        let mapped = plan.context.scoped.as_ref().unwrap();
        assert!(
            mapped
                .target
                .matches_instance(&plan.document, &mapped.presentation)
                .unwrap()
        );
        assert_ne!(mapped.target.node, node("a"));
        assert!(plan.document.nodes()[&node("a")].framing.is_none());
        let framing = plan.document.nodes()[&mapped.target.node]
            .framing
            .as_ref()
            .unwrap();
        for (local, scale) in [(1, 1), (3, 2), (5, 2), (7, 1)] {
            assert_eq!(
                framing
                    .evaluate(
                        ExactRatio::new(local, 2).unwrap(),
                        FrameDuration::new(4).unwrap()
                    )
                    .unwrap()
                    .scale,
                ExactRatio::integer(scale)
            );
        }
        let replay =
            crate::replay_compound::<EditError>(&document, plan.request.as_ref().unwrap(), |_| {
                Ok(())
            })
            .unwrap();
        assert_eq!(replay.document, plan.document);
        assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), document);
    }
}

#[test]
fn scoped_range_calls_resolve_fresh_ranges_and_occurrences_without_captured_geometry() {
    let document = fixture();
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![zoom(2)])),
    ]);
    let before = bank.clone();
    let first = resolve_with_state(
        &document,
        &ranged_context(28, 30),
        vec![call('a', 1)],
        &bank,
        |r| allocator("call-one", r),
        ranged_state,
    )
    .unwrap();
    let mut other = ranged_context(2, 4);
    other.cursor = ProjectFrame(3);
    let scoped = other.scoped.as_mut().unwrap();
    scoped.target.repeats[0].branch = RepeatEditBranch::Play {
        iteration: iteration("outer", 0),
    };
    scoped.target.repeats[1].branch = RepeatEditBranch::Play {
        iteration: iteration("inner", 0),
    };
    scoped.presentation = presentation(0, 0);
    let second = resolve_with_state(
        &document,
        &other,
        vec![call('a', 1)],
        &bank,
        |r| allocator("call-two", r),
        ranged_state,
    )
    .unwrap();
    let first_node = &first.context.scoped.as_ref().unwrap().target.node;
    let second_node = &second.context.scoped.as_ref().unwrap().target.node;
    let initial = |plan: &SemanticPlan, node: &NodeId| {
        plan.document.nodes()[node]
            .framing
            .as_ref()
            .unwrap()
            .evaluate(
                ExactRatio::new(1, 2).unwrap(),
                FrameDuration::new(4).unwrap(),
            )
            .unwrap()
            .scale
    };
    assert_eq!(initial(&first, first_node), ExactRatio::ONE);
    assert_eq!(initial(&second, second_node), ExactRatio::integer(2));
    assert_eq!(
        first.context.visual_selection,
        ranged_context(28, 30).visual_selection
    );
    assert_eq!(second.context.visual_selection, other.visual_selection);
    assert_eq!(bank, before);
    let saved = serde_json::to_string(bank[&name('b')].as_ref()).unwrap();
    assert!(!saved.contains("normalized_range") && !saved.contains("root_range"));
}

#[test]
fn scoped_ranges_require_matching_host_windows_and_refuse_unrepresentable_geometry_before_allocation()
 {
    let document = fixture();
    let context = ranged_context(28, 30);
    let error = resolve(&document, &context, vec![zoom(2)], &BTreeMap::new(), |_| {
        panic!("missing map allocated")
    })
    .unwrap_err();
    assert!(error.message.contains("requested exact range"));
    for window in [
        (ExactRatio::new(-1, 4).unwrap(), ExactRatio::ONE),
        (ExactRatio::ONE, ExactRatio::ONE),
        (ExactRatio::new(1, 4).unwrap(), ExactRatio::integer(2)),
        (
            ExactRatio::new(1, 1_000_003).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
        ),
    ] {
        assert!(
            resolve_with_state(
                &document,
                &context,
                vec![zoom(2)],
                &BTreeMap::new(),
                |_| panic!("invalid window allocated"),
                |staged, site| {
                    let mut result = state(staged, site)?;
                    result.normalized_range = Some(window);
                    Ok(result)
                }
            )
            .is_err()
        );
    }
    assert!(
        resolve_with_state(
            &document,
            &entry(),
            vec![zoom(2)],
            &BTreeMap::new(),
            |_| panic!("unsolicited window allocated"),
            |staged, site| {
                let mut result = state(staged, site)?;
                result.normalized_range = Some((ExactRatio::ZERO, ExactRatio::ONE));
                Ok(result)
            }
        )
        .is_err()
    );
    assert!(
        resolve_with_state(
            &document,
            &ranged_context(28, 28),
            vec![zoom(2)],
            &BTreeMap::new(),
            |_| panic!("empty range allocated"),
            |_, _| panic!("empty range reached host")
        )
        .is_err()
    );
    assert!(
        resolve_with_state(
            &document,
            &ranged_context(26, 30),
            vec![zoom(2)],
            &BTreeMap::new(),
            |_| panic!("cross-owner range allocated"),
            ranged_state
        )
        .is_err()
    );
}

#[test]
fn scoped_visual_authority_rejects_nonframing_leaves_including_after_a_staged_zoom_and_inside_calls()
 {
    let document = fixture();
    let context = ranged_context(28, 30);
    let forbidden = [
        SemanticInstruction::SetAudio {
            change: crate::AudioChange::Step {
                millidecibels: 3000,
            },
        },
        SemanticInstruction::Split,
        SemanticInstruction::SetFraming { framing: None },
        motion(true, 1),
        SemanticInstruction::BeginSelection,
    ];
    for leaf in forbidden {
        let bank = BTreeMap::from([(name('a'), macro_value(vec![leaf.clone()]))]);
        let before = bank.clone();
        let error = resolve_with_state(
            &document,
            &context,
            vec![leaf],
            &bank,
            |_| panic!("nonframing range allocated"),
            ranged_state,
        )
        .unwrap_err();
        assert!(error.message.contains("supports only framing intent"));
        let mut allocated = 0;
        let error = resolve_with_state(
            &document,
            &context,
            vec![zoom(2), call('a', 1)],
            &bank,
            |request| {
                allocated += 1;
                allocator("atomic-range", request)
            },
            ranged_state,
        )
        .unwrap_err();
        assert!(error.message.contains("supports only framing intent"));
        assert_eq!(allocated, 1);
        assert_eq!(document, fixture());
        assert_eq!(bank, before);
    }
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
fn scoped_follow_creep_all_owner_kinds_use_validated_nested_plays_and_exact_isolation() {
    for kind in ["source", "hold", "sequence", "repeat", "retime"] {
        let mut document = fixture();
        let asset = crate::AssetId::new("original").unwrap();
        let clock = crate::SourceTimeBase::new(1, 30).unwrap();
        let span = crate::SourceSpan::new(
            crate::SourceTimestamp {
                ticks: 0,
                time_base: clock,
            },
            crate::SourceTimestamp {
                ticks: 100,
                time_base: clock,
            },
        )
        .unwrap();
        document.assets.insert(
            asset.clone(),
            crate::AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: Some(span),
                audio: None,
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        );
        let target = crate::TargetId::new("subject").unwrap();
        document.targets.insert(
            target.clone(),
            crate::AttentionTarget {
                label: "Subject".into(),
                asset: asset.clone(),
                span,
                region: crate::TargetPoint {
                    center: [400_000, 500_000],
                }
                .into(),
                samples: vec![],
                corrections: vec![],
                provenance: None,
                display_aspect: None,
            },
        );
        let replacement = match kind {
            "source" => NodeKind::Source {
                source: SourceNode {
                    duration: FrameDuration::new(4).unwrap(),
                    edit_window: None,
                    video: crate::SourceVideo::Stream { asset, span },
                    video_mapping: crate::SourceVideoMapping::FitBeat,
                    audio: None,
                    audio_mapping: crate::SourceAudioMapping::FitBeat,
                    link: crate::LinkRelation::Independent,
                    audio_offset: crate::AudioSample(0),
                },
            },
            "hold" => document.nodes()[&node("a")].kind.clone(),
            "sequence" => {
                document.nodes.insert(node("leaf"), hold(4));
                NodeKind::Sequence {
                    children: vec![node("leaf")],
                }
            }
            "repeat" => {
                document.nodes.insert(node("leaf"), hold(2));
                NodeKind::Repeat {
                    child: node("leaf"),
                    iterations: IterationOrder::new(revision("owner-plays"), 2).unwrap(),
                    gap: None,
                    escalation: None,
                }
            }
            "retime" => {
                document.nodes.insert(node("leaf"), hold(6));
                NodeKind::Retime {
                    child: node("leaf"),
                    duration: FrameDuration::new(4).unwrap(),
                    mapping: range(0, 6),
                    pitch: crate::PitchPolicy::FollowSpeed,
                    purpose: crate::RetimePurpose::Edit,
                }
            }
            _ => unreachable!(),
        };
        let fixed = crate::Framing {
            clock: crate::FramingClock::OwnerOutput,
            value: crate::FramingValue::Follow {
                target,
                scale: ExactRatio::integer(2),
                fallback: FramingPose::new(
                    ExactRatio::new(1, 3).unwrap(),
                    ExactRatio::new(2, 3).unwrap(),
                    ExactRatio::new(1, 2).unwrap(),
                )
                .unwrap(),
                scale_transition: None,
            },
        };
        let owner = document.nodes.get_mut(&node("a")).unwrap();
        owner.kind = replacement;
        owner.framing = Some(fixed.clone());
        document.validate().unwrap();
        let instruction = SemanticInstruction::Zoom {
            kind: ZoomKind::Creep {
                from: None,
                to: ExactRatio::integer(4),
                curve: ZoomCurve::Linear,
            },
            center: SemanticZoomCenter::Keep,
        };
        for clock in [
            crate::FramingClock::OwnerOutput,
            crate::FramingClock::RetainedOutput {
                offset: ExactRatio::new(1, 3).unwrap(),
                duration: ExactRatio::new(14, 3).unwrap(),
            },
        ] {
            let mut ranged_document = document.clone();
            ranged_document
                .nodes
                .get_mut(&node("a"))
                .unwrap()
                .framing
                .as_mut()
                .unwrap()
                .clock = clock;
            ranged_document.validate().unwrap();
            let ranged = resolve_with_state(
                &ranged_document,
                &ranged_context(28, 30),
                vec![SemanticInstruction::Zoom {
                    kind: ZoomKind::Creep {
                        from: Some(ExactRatio::integer(3)),
                        to: ExactRatio::integer(4),
                        curve: ZoomCurve::Linear,
                    },
                    center: SemanticZoomCenter::Keep,
                }],
                &BTreeMap::new(),
                |request| allocator("ranged-follow", request),
                ranged_state,
            )
            .unwrap();
            let owner = &ranged.context.scoped.as_ref().unwrap().target.node;
            let framing = ranged.document.nodes()[owner].framing.as_ref().unwrap();
            assert_eq!(framing.clock, crate::FramingClock::OwnerOutput);
            for (local, live, fallback) in [
                (0, ExactRatio::integer(2), ExactRatio::new(1, 2).unwrap()),
                (1, ExactRatio::integer(3), ExactRatio::new(3, 4).unwrap()),
                (
                    2,
                    ExactRatio::new(7, 2).unwrap(),
                    ExactRatio::new(7, 8).unwrap(),
                ),
                (3, ExactRatio::integer(4), ExactRatio::ONE),
                (4, ExactRatio::integer(4), ExactRatio::ONE),
            ] {
                let value = framing
                    .evaluate_with_follow_exact(ExactRatio::integer(local), ExactRatio::integer(4))
                    .unwrap();
                assert_eq!(value.follow.unwrap().1, live, "{kind}/{clock:?}/{local}");
                assert_eq!(value.pose.scale, fallback);
            }
            assert_eq!(
                ranged.document.nodes()[&node("a")]
                    .framing
                    .as_ref()
                    .unwrap()
                    .clock,
                clock
            );
            let replay = crate::replay_compound::<EditError>(
                &ranged_document,
                ranged.request.as_ref().unwrap(),
                |_| Ok(()),
            )
            .unwrap();
            assert_eq!(
                replay.edit.inverse.apply(&ranged.document).unwrap(),
                ranged_document
            );
            assert_eq!(
                ProjectDocument::from_json(&ranged.document.to_json().unwrap()).unwrap(),
                ranged.document
            );
        }
        let mut requests = vec![];
        let plan = resolve(
            &document,
            &entry(),
            vec![instruction.clone()],
            &BTreeMap::new(),
            |request| {
                requests.push(request.clone());
                allocator(kind, request)
            },
        )
        .unwrap();
        let mapped = plan.context.scoped.as_ref().unwrap();
        assert_ne!(mapped.target.node, node("a"));
        assert!(
            mapped
                .target
                .matches_instance(&plan.document, &mapped.presentation)
                .unwrap()
        );
        assert_eq!(requests.len(), 1);
        let SemanticAllocationRequest::Isolation { nodes, marks, .. } = requests[0] else {
            panic!("isolation")
        };
        assert_eq!(plan.document.nodes().len() - document.nodes().len(), nodes);
        assert_eq!(plan.document.marks().len() - document.marks().len(), marks);
        for (id, old) in document.nodes() {
            assert_eq!(&plan.document.nodes()[id], old);
        }
        let framing = plan.document.nodes()[&mapped.target.node]
            .framing
            .as_ref()
            .unwrap();
        assert_eq!(framing.clock, crate::FramingClock::OwnerOutput);
        for (local, live, fallback) in [
            (
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(9, 4).unwrap(),
                ExactRatio::new(9, 16).unwrap(),
            ),
            (
                ExactRatio::new(7, 2).unwrap(),
                ExactRatio::new(15, 4).unwrap(),
                ExactRatio::new(15, 16).unwrap(),
            ),
        ] {
            let value = framing
                .evaluate_with_follow_exact(local, ExactRatio::integer(4))
                .unwrap();
            assert_eq!(value.follow.unwrap().1, live);
            assert_eq!(value.pose.scale, fallback);
        }
        let request = plan.request.as_ref().unwrap();
        let Command::Compound { transaction } = &request.command else {
            panic!("Compound")
        };
        for outer in 0..3 {
            for inner in 0..2 {
                let mut instance = presentation(outer, inner);
                let mut staged = document.clone();
                for step in transaction.steps() {
                    let leaf = step.edit().unwrap();
                    let prepared =
                        crate::prepare_scoped_edit(&staged, &leaf.request(&staged)).unwrap();
                    instance = prepared.map_instance(&staged, &instance).unwrap();
                    staged = prepared.document;
                }
                assert_eq!(
                    matches!(
                        &plan.document.nodes()[&instance.node]
                            .framing
                            .as_ref()
                            .unwrap()
                            .value,
                        crate::FramingValue::Follow {
                            scale_transition: Some(_),
                            ..
                        }
                    ),
                    outer == 1 && inner == 1
                );
            }
        }
        let replay = crate::replay_compound::<EditError>(&document, request, |_| Ok(())).unwrap();
        assert_eq!(replay.document, plan.document);
        assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), document);
        assert_eq!(
            ProjectDocument::from_json(&plan.document.to_json().unwrap()).unwrap(),
            plan.document
        );
        let unchanged = resolve(
            &plan.document,
            &plan.context,
            vec![instruction.clone()],
            &BTreeMap::new(),
            |_| panic!("identical Follow retune allocated"),
        )
        .unwrap();
        assert_eq!(unchanged.document, plan.document);
        assert!(unchanged.request.is_none());
        let retuned = resolve(
            &plan.document,
            &plan.context,
            vec![SemanticInstruction::Zoom {
                kind: ZoomKind::Creep {
                    from: None,
                    to: ExactRatio::integer(3),
                    curve: ZoomCurve::Smoothstep,
                },
                center: SemanticZoomCenter::Keep,
            }],
            &BTreeMap::new(),
            |request| allocator("retuned-follow", request),
        )
        .unwrap();
        let mut expected = framing.clone();
        let crate::FramingValue::Follow {
            scale_transition: Some(transition),
            ..
        } = &mut expected.value
        else {
            panic!("animated Follow");
        };
        transition.factor = ExactRatio::new(3, 2).unwrap();
        transition.curve = ZoomCurve::Smoothstep;
        assert_eq!(
            retuned.document.nodes()[&mapped.target.node].framing,
            Some(expected)
        );
        for (id, old) in plan.document.nodes() {
            if id != &mapped.target.node {
                assert_eq!(&retuned.document.nodes()[id], old);
            }
        }
        let replay = crate::replay_compound::<EditError>(
            &plan.document,
            retuned.request.as_ref().unwrap(),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(replay.document, retuned.document);
        assert_eq!(
            replay.edit.inverse.apply(&retuned.document).unwrap(),
            plan.document
        );
        for variant in 0..7 {
            let mut context = entry();
            match variant {
                0 => {
                    context.scoped.as_mut().unwrap().target.repeats[0].branch =
                        RepeatEditBranch::Default
                }
                1 => {
                    context.scoped.as_mut().unwrap().target.repeats[1].branch =
                        RepeatEditBranch::Default
                }
                2 => context.scoped.as_mut().unwrap().target.repeats.clear(),
                3 => {
                    context.scoped.as_mut().unwrap().presentation.repeats[0].iteration =
                        iteration("outer", 0)
                }
                4 => {
                    context.visual_selection = Some(SemanticVisualSelection::Time {
                        anchor: ProjectFrame(27),
                        head: ProjectFrame(31),
                        extending: false,
                    })
                }
                5 => {
                    context.scoped = None;
                    context.selected_child = Some(node("a"));
                }
                6 => context.scoped.as_mut().unwrap().root = node("inner"),
                _ => unreachable!(),
            }
            assert!(
                resolve(
                    &document,
                    &context,
                    vec![instruction.clone()],
                    &BTreeMap::new(),
                    |_| panic!("invalid Follow authority allocated")
                )
                .is_err(),
                "{kind}/{variant}"
            );
        }
    }
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
