//! Literal Default authority changes its whole family beneath one fixed stage.
use super::nested_fixed_retime::{repeated_fixed_fixture, stage_input};
use super::*;
use deadpan_core::{RepeatEditBranch, RepeatEditStep, ScopedNodeTarget};

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

fn target(presentation: &InstancePath, top_play: bool) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: presentation.node.clone(),
        repeats: presentation
            .repeats
            .iter()
            .enumerate()
            .map(|(index, repeat)| RepeatEditStep {
                repeat: repeat.node.clone(),
                branch: if (index == 0) == top_play {
                    RepeatEditBranch::Play {
                        iteration: repeat.iteration.clone(),
                    }
                } else {
                    RepeatEditBranch::Default
                },
            })
            .collect(),
    }
}

fn resize(
    document: &ProjectDocument,
    target: ScopedNodeTarget,
    presentation: &InstancePath,
    frames: i64,
    revision: &str,
) -> deadpan_core::PreparedScopedHoldDuration {
    let length = duration(frames);
    let needs = document
        .scoped_hold_duration_requirements(&target, length)
        .unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command: Command::SetScopedHoldDuration {
            target,
            duration: length,
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| id(&format!("{revision}-node-{index}")))
                    .collect(),
                marks: (0..needs.marks)
                    .map(|index| MarkId::new(format!("{revision}-mark-{index}")).unwrap())
                    .collect(),
            },
        },
    };
    let prepared =
        deadpan_core::prepare_scoped_hold_duration(document, &request, Some(presentation)).unwrap();
    prepared
        .isolation()
        .validate(document, &request, &prepared.document)
        .unwrap();
    assert_eq!(
        prepared
            .transaction
            .inverse
            .apply(&prepared.document)
            .unwrap(),
        *document
    );
    assert_eq!(
        ProjectDocument::from_json(&prepared.document.to_json().unwrap()).unwrap(),
        prepared.document
    );
    prepared
}

// Exact starts belong to this equal-layout fixture, not to a generic ordinal
// multiplication algorithm. Each query retains the actual stable instance.
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

fn affected(top_play: bool, top: u32, inner: u32) -> bool {
    if top_play { top == 1 } else { inner == 1 }
}

fn compare_outputs(before: &Recorded, after: &Recorded, top_play: bool) {
    assert_eq!(after.samples.len(), ntsc_boundary(40));
    assert_eq!(before.samples.len(), after.samples.len());
    for (top, inner, start) in PLAYS {
        let range = ntsc_boundary(start)..ntsc_boundary(start + 4);
        if affected(top_play, top, inner) {
            assert_ne!(
                after.samples[range.clone()],
                before.samples[range],
                "affected family output at frame {start}"
            );
        } else {
            assert_eq!(
                after.samples[range.clone()],
                before.samples[range.clone()],
                "complete unrelated output at frame {start}"
            );
            assert_eq!(
                after.suppressed[range.clone()],
                before.suppressed[range],
                "complete unrelated suppression at frame {start}"
            );
        }
    }
    let exterior = ntsc_boundary(36)..ntsc_boundary(40);
    assert_eq!(
        after.samples[exterior.clone()],
        before.samples[exterior.clone()]
    );
    assert_eq!(
        after.suppressed[exterior.clone()],
        before.suppressed[exterior]
    );
}

#[test]
fn mixed_fixed_retime_default_authority_preserves_every_fractional_owner() {
    for prebound in [false, true] {
        for top_play in [false, true] {
            let before = repeated_fixed_fixture(prebound);
            let mut provider = mono_original(2).1;
            let before_plan = Arc::new(RenderPlan::compile(&before).unwrap());
            let representative = instance(&before, 1, 1, "pause");
            let chosen = target(&representative, top_play);
            assert!(matches!(
                chosen.repeats[usize::from(top_play)].branch,
                RepeatEditBranch::Default
            ));
            let root_entry = ratio(16 * 8008, 5).ceil().unwrap();
            assert_eq!(root_entry, 25626);
            let old_output = record(&before, &mut provider, false, &[127, 3, 251]);
            assert_eq!(before.duration().unwrap().frames(), 40);

            for (top, inner, start) in PLAYS {
                let stage = instance(&before, top, inner, "processor");
                let input = stage_input(
                    &before_plan,
                    &mut provider,
                    usize::try_from(ratio(i128::from(start) * 8008, 5).ceil().unwrap()).unwrap(),
                    &stage,
                    &[127, 3, 251],
                );
                same_recorded(
                    &input,
                    &super::super::direct_input_reference(&provider, 1, 2),
                    0,
                    "old mixed fractional input",
                );
            }
            let needs = before
                .scoped_hold_duration_requirements(&chosen, duration(3))
                .unwrap();
            assert_eq!(
                (needs.nodes, needs.marks),
                (if top_play { 6 } else { 5 }, 0)
            );
            let grown = resize(
                &before,
                chosen.clone(),
                &representative,
                3,
                "mixed-fixed-grow",
            );
            assert_eq!(grown.isolation().steps().len(), 1);
            assert_eq!(grown.isolation().steps()[0].nodes().len(), needs.nodes);
            assert_eq!(
                grown.fixed_visibility().unwrap().frames,
                FrameRange::new(ProjectFrame(17), ProjectFrame(19)).unwrap()
            );
            assert_eq!(grown.transaction.duration_delta, 0);
            assert_eq!(grown.document.duration().unwrap().frames(), 40);
            assert_eq!(
                grown
                    .target
                    .repeats
                    .iter()
                    .filter(|step| matches!(step.branch, RepeatEditBranch::Default))
                    .count(),
                1
            );
            assert!(
                grown
                    .target
                    .matches_instance(&grown.document, grown.presentation.as_ref().unwrap())
                    .unwrap()
            );
            for (node, original) in before.nodes() {
                assert_eq!(
                    &grown.document.nodes()[node],
                    original,
                    "retained template {node}"
                );
            }

            let owned_needs = grown
                .document
                .scoped_hold_duration_requirements(&grown.target, duration(1))
                .unwrap();
            assert_eq!((owned_needs.nodes, owned_needs.marks), (0, 0));
            let shrunk = resize(
                &grown.document,
                grown.target.clone(),
                grown.presentation.as_ref().unwrap(),
                1,
                "mixed-fixed-shrink",
            );
            assert!(shrunk.isolation().steps().is_empty());
            assert_eq!(
                shrunk.fixed_visibility().unwrap().frames,
                FrameRange::new(ProjectFrame(17), ProjectFrame(18)).unwrap()
            );
            assert_eq!(shrunk.document.duration().unwrap().frames(), 40);

            for (prepared, frames) in [(&grown, 3), (&shrunk, 1)] {
                let plan = Arc::new(RenderPlan::compile(&prepared.document).unwrap());
                for (top, inner, start) in PLAYS {
                    let original_stage = instance(&before, top, inner, "processor");
                    let grown_stage = grown.map_instance(&before, &original_stage).unwrap();
                    let stage = if frames == 3 {
                        grown_stage
                    } else {
                        shrunk.map_instance(&grown.document, &grown_stage).unwrap()
                    };
                    let entry = usize::try_from(ratio(i128::from(start) * 8008, 5).ceil().unwrap())
                        .unwrap();
                    let input = stage_input(&plan, &mut provider, entry, &stage, &[127, 3, 251]);
                    let expected = super::super::direct_input_reference(
                        &provider,
                        1,
                        if affected(top_play, top, inner) {
                            frames
                        } else {
                            2
                        },
                    );
                    same_recorded(
                        &input,
                        &expected,
                        0,
                        "complete mixed family input and suppression",
                    );
                    if affected(top_play, top, inner) {
                        assert_eq!(input.samples[11211], [0.0; 2]);
                        assert!(
                            input.suppressed[11211],
                            "one-point finite support exhaustion"
                        );
                        same_recorded(
                            &stage_input(&plan, &mut provider, entry, &stage, &[1, 239]),
                            &input,
                            0,
                            "mixed family input chunk parity",
                        );
                    }
                }
                let output = record(&prepared.document, &mut provider, false, &[127, 3, 251]);
                compare_outputs(&old_output, &output, top_play);
                assert_eq!(
                    record(&prepared.document, &mut provider, false, &[1, 239]),
                    output
                );
            }
        }
    }
}
