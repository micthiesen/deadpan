//! Selected Repeat bodies keep independent Preserve input clocks below a fixed stage.
use super::nested_fixed_retime::{repeated_fixed_fixture, stage_input};
use super::*;

const PLAYS: [(u32, u32, i64); 9] = [
    (0, 0, 0),
    (0, 1, 4),
    (0, 2, 8),
    (1, 0, 12),
    (1, 1, 16),
    (1, 2, 20),
    (2, 0, 24),
    (2, 1, 28),
    (2, 2, 32),
];

fn instance(document: &ProjectDocument, top: u32, inner: u32, node: &str) -> InstancePath {
    let repeat = |name: &str, play| {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id(name)].kind else {
            unreachable!()
        };
        RepeatInstance {
            node: id(name),
            iteration: iterations.at(play).unwrap(),
        }
    };
    InstancePath {
        node: id(node),
        repeats: vec![repeat("top", top), repeat("outer", inner)],
    }
}

fn target(presentation: &InstancePath, axis: usize, shared_other: bool) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: presentation.node.clone(),
        repeats: presentation
            .repeats
            .iter()
            .enumerate()
            .map(|(index, step)| RepeatEditStep {
                repeat: step.node.clone(),
                branch: if index != axis && shared_other {
                    RepeatEditBranch::Default
                } else {
                    RepeatEditBranch::Play {
                        iteration: step.iteration.clone(),
                    }
                },
            })
            .collect(),
    }
}

fn context(document: &ProjectDocument, axis: usize, shared_other: bool) -> SemanticContext {
    let first = instance(document, 1, 1, "pause");
    let second = instance(
        document,
        if axis == 0 { 2 } else { 1 },
        if axis == 1 { 2 } else { 1 },
        "pause",
    );
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
            root: id("top"),
            target: target(&first, axis, shared_other),
            also: vec![target(&second, axis, shared_other)],
            presentation: Some(first),
        })),
        parent: id("root"),
        cursor: ProjectFrame(17),
        selected_child: Some(id("top")),
        visual_selection: None,
    }
}

fn resize(
    document: &ProjectDocument,
    context: &SemanticContext,
    frames: u32,
    key: &str,
) -> SemanticPlan {
    let planned = plan_semantic(
        document,
        context,
        &SemanticProgram::new(vec![SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: std::num::NonZeroU32::new(frames).unwrap(),
            },
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        RevisionId::new(format!("{key}-final")).unwrap(),
        |request| {
            let SemanticAllocationRequest::Isolation {
                step_index,
                nodes,
                marks,
            } = request
            else {
                panic!("duration identity allocation only")
            };
            Ok(SemanticAllocation::Isolation {
                new_revision: RevisionId::new(format!("{key}-leaf-{step_index}")).unwrap(),
                identities: OccurrenceIdentities {
                    nodes: (0..nodes)
                        .map(|n| id(&format!("{key}-node-{step_index}-{n}")))
                        .collect(),
                    marks: (0..marks)
                        .map(|n| MarkId::new(format!("{key}-mark-{step_index}-{n}")).unwrap())
                        .collect(),
                },
            })
        },
        |_, _| panic!("no Original resolver"),
    )
    .unwrap();
    let request = planned.request.as_ref().unwrap();
    let replay = replay_compound::<EditError>(document, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&planned.document).unwrap(),
        *document
    );
    assert_eq!(
        ProjectDocument::from_json(&planned.document.to_json().unwrap()).unwrap(),
        planned.document
    );
    let proof = derive_scoped_isolation(document, request, &planned.document).unwrap();
    let before = context.selected_hold_duration.as_ref().unwrap();
    let after = planned.context.selected_hold_duration.as_ref().unwrap();
    assert_eq!(proof.map_forward(&before.target).unwrap(), after.target);
    assert_eq!(proof.map_forward(&before.also[0]).unwrap(), after.also[0]);
    assert_eq!(
        proof
            .map_instance_forward(before.presentation.as_ref().unwrap())
            .unwrap(),
        *after.presentation.as_ref().unwrap()
    );
    for (old, new) in std::iter::once(&before.target)
        .chain(&before.also)
        .zip(std::iter::once(&after.target).chain(&after.also))
    {
        assert_eq!(
            old.repeats
                .iter()
                .filter(|s| matches!(s.branch, RepeatEditBranch::Default))
                .count(),
            new.repeats
                .iter()
                .filter(|s| matches!(s.branch, RepeatEditBranch::Default))
                .count()
        );
    }
    assert_eq!(planned.context.cursor, ProjectFrame(17));
    assert_eq!(planned.document.duration().unwrap().frames(), 40);
    planned
}

fn affected(axis: usize, shared_other: bool, top: u32, inner: u32) -> bool {
    if axis == 0 {
        top >= 1 && (shared_other || inner == 1)
    } else {
        inner >= 1 && (shared_other || top == 1)
    }
}

#[test]
fn selected_fixed_retime_duration_preserves_all_fractional_processing_owners() {
    for prebound in [false, true] {
        for axis in [0, 1] {
            for shared_other in [false, true] {
                let before = repeated_fixed_fixture(prebound);
                let captured = context(&before, axis, shared_other);
                let mut provider = mono_original(2).1;
                let old_output = record(&before, &mut provider, false, &[127, 3, 251]);
                assert_eq!(old_output.samples.len(), ntsc_boundary(40));
                assert_eq!(ratio(16 * 8008, 5).ceil().unwrap(), 25626);
                let grown = resize(&before, &captured, 3, "selected-fixed-grow");
                let grown_request = grown.request.as_ref().unwrap();
                let grown_proof =
                    derive_scoped_isolation(&before, grown_request, &grown.document).unwrap();
                let expected_steps = match (axis, shared_other) {
                    (0, false) => 4,
                    (0, true) => 2,
                    (1, false) => 3,
                    (1, true) => 2,
                    _ => unreachable!(),
                };
                assert_eq!(grown_proof.record().steps().len(), expected_steps);
                for (node, original) in before.nodes() {
                    assert_eq!(
                        &grown.document.nodes()[node],
                        original,
                        "retained template {node}"
                    );
                }
                let shrunk = resize(&grown.document, &grown.context, 1, "selected-fixed-shrink");
                let shrunk_proof = derive_scoped_isolation(
                    &grown.document,
                    shrunk.request.as_ref().unwrap(),
                    &shrunk.document,
                )
                .unwrap();
                assert!(shrunk_proof.record().steps().is_empty());
                for (planned, frames) in [(&grown, 3), (&shrunk, 1)] {
                    let plan = Arc::new(RenderPlan::compile(&planned.document).unwrap());
                    for (top, inner, start) in PLAYS {
                        let original_stage = instance(&before, top, inner, "processor");
                        let grown_stage =
                            grown_proof.map_instance_forward(&original_stage).unwrap();
                        let stage = if frames == 3 {
                            grown_stage
                        } else {
                            shrunk_proof.map_instance_forward(&grown_stage).unwrap()
                        };
                        let entry =
                            usize::try_from(ratio(i128::from(start) * 8008, 5).ceil().unwrap())
                                .unwrap();
                        let input =
                            stage_input(&plan, &mut provider, entry, &stage, &[127, 3, 251]);
                        let expected = super::super::direct_input_reference(
                            &provider,
                            1,
                            if affected(axis, shared_other, top, inner) {
                                frames
                            } else {
                                2
                            },
                        );
                        same_recorded(
                            &input,
                            &expected,
                            0,
                            "every selected batch input cell and suppression",
                        );
                        same_recorded(
                            &stage_input(&plan, &mut provider, entry, &stage, &[1, 239]),
                            &input,
                            0,
                            "selected batch input chunk parity",
                        );
                        if affected(axis, shared_other, top, inner) {
                            assert_eq!(input.samples[11211], [0.0; 2]);
                            assert!(
                                input.suppressed[11211],
                                "finite old source support exhaustion"
                            );
                        }
                    }
                    let output = record(&planned.document, &mut provider, false, &[127, 3, 251]);
                    assert_eq!(output.samples.len(), old_output.samples.len());
                    for (top, inner, start) in PLAYS {
                        let range = ntsc_boundary(start)..ntsc_boundary(start + 4);
                        if affected(axis, shared_other, top, inner) {
                            assert_ne!(output.samples[range.clone()], old_output.samples[range]);
                        } else {
                            assert_eq!(
                                output.samples[range.clone()],
                                old_output.samples[range.clone()]
                            );
                            assert_eq!(
                                output.suppressed[range.clone()],
                                old_output.suppressed[range]
                            );
                        }
                    }
                    let exterior = ntsc_boundary(36)..ntsc_boundary(40);
                    assert_eq!(
                        output.samples[exterior.clone()],
                        old_output.samples[exterior.clone()]
                    );
                    assert_eq!(
                        output.suppressed[exterior.clone()],
                        old_output.suppressed[exterior]
                    );
                    assert_eq!(
                        record(&planned.document, &mut provider, false, &[1, 239]),
                        output
                    );
                }
            }
        }
    }
}
