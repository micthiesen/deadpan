//! Refuting tests for the opaque, exact-value one-InsertTime replay witness.
//! No test manufactures a witness or observes it through a mutable API.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use deadpan_core::*;
use serde_json::{Value, json};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn hold(duration: i64) -> BeatNode {
    BeatNode::hold(
        "Held",
        HoldRecipe {
            duration: frames(duration),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn tree(children: Vec<NodeId>, mut nodes: BTreeMap<NodeId, BeatNode>) -> ProjectDocument {
    nodes.insert(id("root"), BeatNode::sequence("Root", children));
    ProjectDocument::from_json(&json!({
        "schema_version": DOCUMENT_SCHEMA_VERSION, "project_id":"one-leaf", "revision_id":"entry",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root", "nodes":nodes, "assets":{}, "marks":{}, "overrides":{}
    }).to_string()).unwrap()
}
fn apply(document: &ProjectDocument, allocation: &str, command: Command) -> ProjectDocument {
    let (edit, next) = apply_with_result(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(allocation),
            command,
        },
    )
    .unwrap();
    assert_eq!(edit.forward.apply(document).unwrap(), next);
    assert_eq!(edit.inverse.apply(&next).unwrap(), *document);
    next
}
fn picture_context() -> CapturedFraming {
    CapturedFraming::new(vec![CapturedCanvas {
        width: 16,
        height: 16,
        fit: CapturedFit::Fit,
        layers: vec![Some(
            FramingPose::new(
                ExactRatio::ZERO,
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::ONE,
            )
            .unwrap(),
        )],
    }])
    .unwrap()
}
fn fixture(bound: bool) -> ProjectDocument {
    let mut first = hold(12);
    first.framing = Some(
        Framing::static_pose(
            FramingPose::new(
                ExactRatio::ZERO,
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::ONE,
            )
            .unwrap(),
        )
        .unwrap(),
    );
    first.audio_treatments = first
        .audio_treatments
        .with_trim(GainDb::new(-3_000).unwrap())
        .unwrap();
    let document = tree(
        vec![id("group"), id("tail")],
        BTreeMap::from([
            (
                id("group"),
                BeatNode::sequence("Group", vec![id("first"), id("empty"), id("second")]),
            ),
            (id("first"), first),
            (id("empty"), BeatNode::sequence("Empty", vec![])),
            (id("second"), hold(12)),
            (id("tail"), hold(8)),
        ]),
    );
    let document = apply(
        &document,
        "marked",
        Command::SetMark {
            id: MarkId::new("native-mark-m").unwrap(),
            owner: id("first"),
            label: "m".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: id("first"),
                    position: ExactRatio::integer(9),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::DeleteOwned,
        },
    );
    if !bound {
        return document;
    }
    apply(
        &document,
        "seed-bindings",
        Command::InsertTime {
            at: ProjectFrame(32),
            hold: HoldRecipe {
                duration: frames(2),
                picture_context: None,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: id("seed-pause"),
            identities: SplitIdentities { nodes: vec![] },
            timing: AudioTimingId {
                allocation: revision("seed-bindings"),
                ordinal: 0,
            },
        },
    )
}
fn context(parent: &str, cursor: i64) -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: id(parent),
        cursor: ProjectFrame(cursor),
        selected_child: Some(id("first")),
        visual_selection: None,
    }
}
fn pause(duration: u32) -> SemanticInstruction {
    SemanticInstruction::InsertPause {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(duration).unwrap(),
        },
        black: false,
    }
}
fn motion(count: u32) -> SemanticInstruction {
    SemanticInstruction::MoveFrames {
        forward: true,
        count: NonZeroU32::new(count).unwrap(),
    }
}
fn cut() -> SemanticInstruction {
    SemanticInstruction::CutFrames {
        operation: FrameCut::new(1).unwrap(),
        register: RegisterName::new('b').unwrap(),
    }
}
fn allocate(request: SemanticAllocationRequest) -> SemanticAllocation {
    match request {
        SemanticAllocationRequest::InsertPause {
            step_index,
            required_split_ids,
        } => SemanticAllocation::InsertPause {
            new_revision: revision(&format!("leaf-{step_index}")),
            id: id(&format!("pause-{step_index}")),
            split: SplitIdentities {
                nodes: (0..required_split_ids)
                    .map(|index| id(&format!("pause-split-{step_index}-{index}")))
                    .collect(),
            },
        },
        SemanticAllocationRequest::InsertOccurrencePause {
            step_index,
            isolation_nodes,
            isolation_marks,
            split_nodes,
            container,
            gap,
        } => SemanticAllocation::InsertOccurrencePause {
            new_revision: revision(&format!("leaf-{step_index}")),
            id: id(&format!("pause-{step_index}")),
            identities: OccurrencePauseIdentities {
                isolation: OccurrenceIdentities {
                    nodes: (0..isolation_nodes)
                        .map(|n| id(&format!("occurrence-{step_index}-{n}")))
                        .collect(),
                    marks: (0..isolation_marks)
                        .map(|n| MarkId::new(format!("occurrence-mark-{step_index}-{n}")).unwrap())
                        .collect(),
                },
                split: SplitIdentities {
                    nodes: (0..split_nodes)
                        .map(|n| id(&format!("occurrence-split-{step_index}-{n}")))
                        .collect(),
                },
                container: container.then(|| id(&format!("container-{step_index}"))),
                gap: gap.then(|| id(&format!("gap-{step_index}"))),
            },
        },
        SemanticAllocationRequest::Cut {
            step_index,
            required_split_ids,
        } => SemanticAllocation::Cut {
            new_revision: revision(&format!("leaf-{step_index}")),
            capture_revision: revision(&format!("capture-{step_index}")),
            split_identities: SplitIdentities {
                nodes: (0..required_split_ids)
                    .map(|index| id(&format!("cut-split-{step_index}-{index}")))
                    .collect(),
            },
        },
        SemanticAllocationRequest::Yank { step_index } => SemanticAllocation::Yank {
            capture_revision: revision(&format!("capture-{step_index}")),
        },
        SemanticAllocationRequest::ParameterEdit { step_index } => {
            SemanticAllocation::ParameterEdit {
                new_revision: revision(&format!("leaf-{step_index}")),
            }
        }
        other => panic!("unexpected fixture allocation {other:?}"),
    }
}
fn plan_in(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
) -> SemanticPlan {
    plan_in_namespace(document, context, instructions, bank, "")
}
fn plan_in_namespace(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    namespace: &str,
) -> SemanticPlan {
    plan_semantic_with_speech(
        document,
        context,
        &SemanticProgram::new(instructions).unwrap(),
        SemanticRegisterBank {
            entries: bank,
            version: 7,
        },
        revision(&format!("{namespace}outer")),
        |request| {
            let allocation = allocate(request);
            if namespace.is_empty() {
                return Ok(allocation);
            }
            let SemanticAllocation::InsertPause {
                new_revision,
                id: node,
                split,
            } = allocation
            else {
                panic!("fresh namespace is used only by a later pause")
            };
            Ok(SemanticAllocation::InsertPause {
                new_revision: revision(&format!("{namespace}{new_revision}")),
                id: id(&format!("{namespace}{node}")),
                split: SplitIdentities {
                    nodes: split
                        .nodes
                        .into_iter()
                        .map(|node| id(&format!("{namespace}{node}")))
                        .collect(),
                },
            })
        },
        |_, _| panic!("no Original register"),
        |_| panic!("no speech analysis"),
        |_, _| {
            Ok(PauseProvider {
                video: HoldVideo::Background,
                picture_context: Some(picture_context()),
                audio: HoldAudio::Silence,
            })
        },
    )
    .unwrap()
}
fn plan(document: &ProjectDocument, duration: u32) -> SemanticPlan {
    plan_in(
        document,
        &context("group", 5),
        vec![pause(duration)],
        &BTreeMap::new(),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct Visit {
    before: String,
    after: String,
    request: Option<String>,
    step: String,
    value: Option<String>,
}
#[derive(Debug, PartialEq, Eq)]
struct Replayed {
    document: ProjectDocument,
    edit: EditTransaction,
    register_writes: BTreeMap<RegisterName, Arc<RegisterValue>>,
    document_wire: String,
    edit_wire: String,
}
fn host_refusal() -> EditError {
    EditError {
        code: EditErrorCode::SelectionUnavailable,
        message: "host refused this exact staged value".into(),
        current_revision: None,
    }
}
fn replay(
    document: &ProjectDocument,
    request: &CommandRequest,
    candidate: Option<&OneLeafReplay>,
    legacy: bool,
    refuse: bool,
) -> (Result<Replayed, EditError>, Vec<Visit>) {
    let mut visits = Vec::new();
    let mut visit = |value: CompoundVisit<'_>| {
        visits.push(Visit {
            before: value.before.to_compact_json().unwrap(),
            after: value.after.to_compact_json().unwrap(),
            request: value
                .request
                .map(|request| serde_json::to_string(request).unwrap()),
            step: serde_json::to_string(value.step).unwrap(),
            value: value
                .value
                .map(|value| serde_json::to_string(value).unwrap()),
        });
        if refuse { Err(host_refusal()) } else { Ok(()) }
    };
    let result = if legacy {
        replay_compound::<EditError>(document, request, &mut visit)
    } else {
        replay_compound_with_one_leaf::<EditError>(document, request, &mut visit, candidate)
    };
    let result = result.map(|outcome| Replayed {
        document_wire: outcome.document.to_compact_json().unwrap(),
        edit_wire: serde_json::to_string(&outcome.edit).unwrap(),
        document: outcome.document,
        edit: outcome.edit,
        register_writes: outcome.register_writes,
    });
    (result, visits)
}
fn parity(
    document: &ProjectDocument,
    request: &CommandRequest,
    candidate: &OneLeafReplay,
) -> (Result<Replayed, EditError>, Vec<Visit>) {
    let cached = replay(document, request, Some(candidate), false, false);
    assert_eq!(cached, replay(document, request, None, false, false));
    assert_eq!(cached, replay(document, request, None, true, false));
    cached
}
fn modify_document(document: &ProjectDocument, change: impl FnOnce(&mut Value)) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    change(&mut wire);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn modify_leaf(request: &CommandRequest, change: impl FnOnce(&mut LeafEdit)) -> CommandRequest {
    let mut changed = request.clone();
    let Command::Compound { transaction } = &request.command else {
        panic!("fixture compound")
    };
    let mut steps = transaction.steps().to_vec();
    let ResolvedStep::Edit { edit } = &mut steps[0] else {
        panic!("fixture Edit")
    };
    change(edit);
    changed.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version(),
            transaction.inputs().clone(),
            steps,
        )
        .unwrap(),
    };
    changed
}
fn change_insert(
    edit: &mut LeafEdit,
    change: impl FnOnce(&mut HoldRecipe, &mut SplitIdentities, &mut AudioTimingId),
) {
    let mut command = edit.command.as_command().clone();
    let Command::InsertTime {
        hold,
        identities,
        timing,
        ..
    } = &mut command
    else {
        panic!("fixture InsertTime")
    };
    change(hold, identities, timing);
    edit.command = AtomicCommand::new(command).unwrap();
}
fn assert_inverse(entry: &ProjectDocument, replayed: &Replayed) {
    assert_eq!(
        replayed.edit.forward.apply(entry).unwrap(),
        replayed.document
    );
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        *entry
    );
    assert_eq!(
        replayed.edit.forward.restores(entry, &replayed.document),
        Ok(true)
    );
    assert_eq!(
        replayed.edit.inverse.restores(&replayed.document, entry),
        Ok(true)
    );
}

#[test]
fn one_leaf_replay_exact_interior_result_visitor_and_aggregate_are_unchanged() {
    let entry = fixture(true);
    let planned = plan(&entry, 3);
    let candidate = planned
        .one_leaf_replay()
        .expect("one ordinary eligible leaf");
    let request = planned.request.as_ref().unwrap();
    let (result, visits) = parity(&entry, request, candidate);
    let result = result.unwrap();
    assert_eq!(result.document, planned.document);
    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0].before, entry.to_compact_json().unwrap());
    assert!(visits[0].request.is_some());
    assert_inverse(&entry, &result);
    let rebuilt = ProjectDocument::from_json(&entry.to_compact_json().unwrap()).unwrap();
    assert_eq!(entry, rebuilt);
    assert_eq!(parity(&rebuilt, request, candidate).0.unwrap(), result);
}

#[test]
fn one_leaf_replay_same_revision_changed_values_and_swapped_witnesses_cannot_retarget() {
    let entry = fixture(true);
    let planned = plan(&entry, 3);
    let request = planned.request.as_ref().unwrap();
    let candidate = planned.one_leaf_replay().unwrap();
    for changed in [
        modify_document(&entry, |wire| {
            wire["nodes"]["first"]["label"] = json!("changed label")
        }),
        modify_document(&entry, |wire| {
            wire["marks"]["native-mark-m"]["label"] = json!("n")
        }),
    ] {
        assert_eq!(changed.project_id(), entry.project_id());
        assert_eq!(changed.revision_id(), entry.revision_id());
        assert_ne!(changed, entry);
        let (result, _) = parity(&changed, request, candidate);
        let result = result.unwrap();
        assert_ne!(result.document, planned.document);
        assert_inverse(&changed, &result);
    }
    let changed_entry = modify_document(&entry, |wire| {
        wire["nodes"]["first"]["label"] = json!("another witness entry")
    });
    let other = plan(&changed_entry, 3);
    assert_eq!(
        other.request, planned.request,
        "same request identifiers do not bind the entry value"
    );
    assert_eq!(
        parity(&entry, request, other.one_leaf_replay().unwrap())
            .0
            .unwrap()
            .document,
        planned.document
    );
    let other_request = plan(&entry, 5);
    assert_ne!(other_request.request, planned.request);
    assert_eq!(
        parity(&entry, request, other_request.one_leaf_replay().unwrap())
            .0
            .unwrap()
            .document,
        planned.document
    );
}

#[test]
fn one_leaf_replay_public_plan_field_mutation_does_not_modify_private_witnesses() {
    let entry = fixture(true);
    let mut planned = plan(&entry, 3);
    let original_request = planned.request.as_ref().unwrap().clone();
    let original_result = planned.document.clone();
    let changed_request = modify_leaf(&original_request, |edit| {
        change_insert(edit, |hold, _, _| hold.duration = frames(6))
    });
    planned.document = modify_document(&planned.document, |wire| {
        wire["nodes"]["pause-0"]["label"] = json!("public field mutation")
    });
    planned.request = Some(changed_request.clone());
    let witness = planned.one_leaf_replay().unwrap();
    assert_eq!(
        parity(&entry, &original_request, witness)
            .0
            .unwrap()
            .document,
        original_result
    );
    let changed = parity(&entry, &changed_request, witness).0.unwrap();
    assert_ne!(changed.document, original_result);
    assert_ne!(changed.document, planned.document);
    assert_inverse(&entry, &changed);
}

#[test]
fn one_leaf_replay_changed_leaf_payloads_fall_back_with_exact_results_or_refusals() {
    let entry = fixture(true);
    let planned = plan(&entry, 3);
    let request = planned.request.as_ref().unwrap();
    let candidate = planned.one_leaf_replay().unwrap();
    let mut changed = vec![
        modify_leaf(request, |edit| {
            change_insert(edit, |hold, _, _| hold.duration = frames(7))
        }),
        modify_leaf(request, |edit| {
            change_insert(edit, |hold, _, _| hold.picture_context = None)
        }),
        modify_leaf(request, |edit| {
            change_insert(edit, |_, identities, _| {
                identities.nodes.pop();
            })
        }),
        modify_leaf(request, |edit| {
            change_insert(edit, |_, _, timing| timing.ordinal = 3)
        }),
        modify_leaf(request, |edit| {
            edit.new_revision = revision("different-leaf")
        }),
    ];
    let mut outer = request.clone();
    outer.new_revision = revision("different-outer");
    changed.push(outer);
    for request in changed {
        let (result, visits) = parity(&entry, &request, candidate);
        if let Ok(result) = result {
            assert_inverse(&entry, &result);
        } else {
            assert!(visits.is_empty());
        }
    }
}

#[test]
fn one_leaf_replay_outer_leaf_and_identity_prefix_errors_still_precede_the_visitor() {
    let entry = fixture(true);
    let planned = plan(&entry, 3);
    let request = planned.request.as_ref().unwrap();
    let candidate = planned.one_leaf_replay().unwrap();
    let mut wrong_project = request.clone();
    wrong_project.project_id = ProjectId::new("wrong-project").unwrap();
    let mut stale = request.clone();
    stale.expected_revision = revision("stale");
    let leaf_is_outer = modify_leaf(request, |edit| {
        edit.new_revision = request.new_revision.clone()
    });
    let collision = modify_leaf(request, |edit| {
        change_insert(edit, |_, identities, _| identities.nodes[0] = id("first"))
    });
    for (request, code) in [
        (wrong_project, EditErrorCode::ProjectConflict),
        (stale, EditErrorCode::RevisionConflict),
        (leaf_is_outer, EditErrorCode::IdentityConflict),
        (collision, EditErrorCode::IdentityConflict),
    ] {
        let (result, visits) = parity(&entry, &request, candidate);
        assert_eq!(result.unwrap_err().code, code);
        assert!(visits.is_empty());
    }
}

#[test]
fn one_leaf_replay_host_visitor_refusal_is_not_bypassed() {
    let entry = fixture(true);
    let planned = plan(&entry, 3);
    let request = planned.request.as_ref().unwrap();
    let cached = replay(&entry, request, planned.one_leaf_replay(), false, true);
    assert_eq!(cached, replay(&entry, request, None, false, true));
    assert_eq!(cached, replay(&entry, request, None, true, true));
    assert_eq!(cached.0.unwrap_err(), host_refusal());
    assert_eq!(cached.1.len(), 1);
}

#[test]
fn one_leaf_replay_reference_modes_bypass_use_and_creation_and_restore_their_state() {
    let entry = fixture(true);
    let planned = plan(&entry, 3);
    let request = planned.request.as_ref().unwrap();
    let candidate = planned.one_leaf_replay().unwrap();
    with_reference_command_work(|| {
        parity(&entry, request, candidate).0.unwrap();
    });
    let normal_wire = parity(&entry, request, candidate).0.unwrap().document_wire;
    let reference_wire = with_reference_timing_representation(|| {
        parity(&entry, request, candidate).0.unwrap().document_wire
    });
    assert_ne!(
        normal_wire, reference_wire,
        "fixture must distinguish compact and complete retained timing representations"
    );
    let reference_created = with_reference_command_work(|| plan(&entry, 3));
    assert!(reference_created.one_leaf_replay().is_none());
    let full_created = with_reference_timing_representation(|| plan(&entry, 3));
    assert!(full_created.one_leaf_replay().is_none());
    assert_eq!(
        parity(&entry, request, candidate).0.unwrap().document_wire,
        normal_wire
    );
}

#[test]
fn one_leaf_replay_extra_steps_registers_ai_occurrences_and_derived_results_are_ineligible() {
    let entry = fixture(false);
    let yank = SemanticInstruction::YankBeat {
        register: RegisterName::new('b').unwrap(),
    };
    for instructions in [
        vec![pause(3), motion(5), pause(1)],
        vec![yank.clone(), pause(3)],
        vec![pause(3), yank],
        vec![cut(), pause(3)],
        vec![SemanticInstruction::InsertAiPause {
            length: PauseLength::Frames {
                frames: NonZeroU32::new(3).unwrap(),
            },
        }],
    ] {
        let planned = plan_in(&entry, &context("group", 5), instructions, &BTreeMap::new());
        assert!(planned.one_leaf_replay().is_none());
        let request = planned.request.as_ref().unwrap();
        assert_eq!(
            replay(&entry, request, None, false, false),
            replay(&entry, request, None, true, false)
        );
    }
    let bank = BTreeMap::from([(
        RegisterName::new('a').unwrap(),
        Arc::new(RegisterValue::Macro {
            program: Arc::new(SemanticProgram::new(vec![pause(3)]).unwrap()),
        }),
    )]);
    let input_plan = plan_in(
        &entry,
        &context("group", 5),
        vec![SemanticInstruction::Call {
            register: RegisterName::new('a').unwrap(),
            count: NonZeroU32::new(1).unwrap(),
        }],
        &bank,
    );
    assert!(input_plan.one_leaf_replay().is_none());
    let mut repeated = hold(12);
    repeated.kind = NodeKind::Repeat {
        child: id("first"),
        iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    let repeated = tree(
        vec![id("repeat")],
        BTreeMap::from([(id("repeat"), repeated), (id("first"), hold(12))]),
    );
    let mut root_context = context("root", 5);
    root_context.selected_child = None;
    let occurrence = plan_in(&repeated, &root_context, vec![pause(3)], &BTreeMap::new());
    assert!(occurrence.one_leaf_replay().is_none());
    let mut speed = hold(8);
    speed.kind = NodeKind::Retime {
        child: id("first"),
        duration: frames(8),
        mapping: range(0, 12),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let speed = tree(
        vec![id("speed")],
        BTreeMap::from([(id("speed"), speed), (id("first"), hold(12))]),
    );
    let derived = plan_in(&speed, &root_context, vec![pause(3)], &BTreeMap::new());
    assert!(!derived.document.derived_clocks().is_empty());
    assert!(derived.one_leaf_replay().is_none());
    let mut next_context = context("root", 1);
    next_context.selected_child = None;
    let later = plan_in_namespace(
        &derived.document,
        &next_context,
        vec![pause(1)],
        &BTreeMap::new(),
        "later-",
    );
    assert!(later.one_leaf_replay().is_none());
}

fn alias_zero_ratios(value: &mut Value) -> usize {
    match value {
        Value::Object(fields) => {
            if fields.get("numerator") == Some(&json!("0")) && fields.contains_key("denominator") {
                fields.insert("numerator".into(), json!("-0"));
                fields.insert("denominator".into(), json!("7"));
                1
            } else {
                fields.values_mut().map(alias_zero_ratios).sum()
            }
        }
        Value::Array(values) => values.iter_mut().map(alias_zero_ratios).sum(),
        _ => 0,
    }
}

#[test]
fn one_leaf_replay_signed_zero_rational_spelling_normalizes_in_entry_and_request() {
    let entry = fixture(true);
    let mut alias_wire = serde_json::to_value(&entry).unwrap();
    assert!(alias_zero_ratios(&mut alias_wire) > 0);
    let alias = ProjectDocument::from_json(&alias_wire.to_string()).unwrap();
    assert_eq!(entry, alias);
    assert_eq!(
        entry.to_compact_json().unwrap(),
        alias.to_compact_json().unwrap()
    );
    let planned = plan(&entry, 3);
    let request = planned.request.as_ref().unwrap();
    let mut request_wire = serde_json::to_value(request).unwrap();
    assert!(
        alias_zero_ratios(&mut request_wire) > 0,
        "request contains captured framing"
    );
    let alias_request: CommandRequest = serde_json::from_value(request_wire).unwrap();
    assert_eq!(*request, alias_request);
    assert_eq!(
        serde_json::to_string(request).unwrap(),
        serde_json::to_string(&alias_request).unwrap()
    );
    let (result, _) = parity(&alias, &alias_request, planned.one_leaf_replay().unwrap());
    assert_inverse(&alias, &result.unwrap());
    for literal in ["-0.0", "+0.0", "0.0"] {
        assert!(serde_json::from_str::<ExactRatio>(literal).is_err());
    }
    for floating in [json!(-0.0), json!(0.0)] {
        assert!(
            serde_json::from_value::<ExactRatio>(
                json!({"numerator":floating.clone(),"denominator":"1"})
            )
            .is_err()
        );
        let gain_error = serde_json::from_value::<GainDb>(floating.clone()).unwrap_err();
        let integer_error = serde_json::from_value::<i32>(floating.clone()).unwrap_err();
        assert_eq!(gain_error.to_string(), integer_error.to_string());
        let mut invalid = serde_json::to_value(&entry).unwrap();
        invalid["presentation_basis"]["width"] = floating;
        let geometry_error = ProjectDocument::from_json(&invalid.to_string()).unwrap_err();
        assert_eq!(geometry_error.code, DocumentErrorCode::InvalidJson);
        assert!(
            geometry_error.message.contains("expected u32"),
            "{geometry_error:?}"
        );
    }
}

#[test]
fn one_leaf_replay_numeric_looking_labels_are_distinct_authored_strings() {
    let entry = modify_document(&fixture(true), |wire| {
        wire["nodes"]["first"]["label"] = json!("-0.0")
    });
    let other = modify_document(&entry, |wire| {
        wire["nodes"]["first"]["label"] = json!("+0.0")
    });
    assert_ne!(entry, other);
    assert_eq!(entry.revision_id(), other.revision_id());
    let planned = plan(&entry, 3);
    let result = parity(
        &other,
        planned.request.as_ref().unwrap(),
        planned.one_leaf_replay().unwrap(),
    )
    .0
    .unwrap();
    assert_eq!(result.document.nodes()[&id("first")].label, "+0.0");
    assert_inverse(&other, &result);
}

#[test]
fn one_leaf_replay_explicit_defaults_roundtrip_complete_timing_values_and_wire() {
    let entry = fixture(true);
    let mut explicit = serde_json::to_value(&entry).unwrap();
    explicit["derived_clocks"] = json!({});
    explicit["sounds"] = json!({});
    explicit["audio_bindings"]["gap_bindings"] = json!({});
    explicit["audio_bindings"]["sound_clocks"] = json!({});
    for binding in explicit["audio_bindings"]["bindings"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        let binding = binding.as_object_mut().unwrap();
        binding.entry("resume").or_insert(Value::Null);
        binding.entry("reanchors").or_insert(json!([]));
        let lattice = binding.get_mut("lattice").unwrap().as_object_mut().unwrap();
        lattice.entry("arguments").or_insert(json!([]));
        lattice.entry("births").or_insert(json!([]));
        lattice
            .get_mut("reference")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .entry("root")
            .or_insert(json!({"type":"project_root_round_even"}));
    }
    let decoded = ProjectDocument::from_json(&explicit.to_string()).unwrap();
    assert_eq!(entry, decoded);
    assert_eq!(
        entry.to_compact_json().unwrap(),
        decoded.to_compact_json().unwrap()
    );
    let planned = plan(&entry, 3);
    let result = parity(
        &decoded,
        planned.request.as_ref().unwrap(),
        planned.one_leaf_replay().unwrap(),
    )
    .0
    .unwrap();
    assert_inverse(&decoded, &result);
}

#[test]
fn one_leaf_replay_transaction_constructor_limits_and_identity_refusals_remain() {
    let entry = fixture(true);
    let planned = plan(&entry, 3);
    let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
        panic!("compound")
    };
    let step = transaction.steps()[0].clone();
    let too_many = ResolvedTransaction::new(
        7,
        BTreeMap::new(),
        vec![step.clone(); MAX_COMPOUND_STEPS + 1],
    )
    .unwrap_err();
    assert_eq!(too_many.code, EditErrorCode::LimitExceeded);
    let duplicate =
        ResolvedTransaction::new(7, BTreeMap::new(), vec![step.clone(), step]).unwrap_err();
    assert_eq!(duplicate.code, EditErrorCode::IdentityConflict);
    let forbidden = AtomicCommand::new(Command::RestoreSnapshot {
        snapshot: Box::new(entry.clone()),
    })
    .unwrap_err();
    assert_eq!(forbidden.code, EditErrorCode::InvalidCommand);
    parity(
        &entry,
        planned.request.as_ref().unwrap(),
        planned.one_leaf_replay().unwrap(),
    )
    .0
    .unwrap();
}
