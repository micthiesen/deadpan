//! Ownership transforms for retained sampling clocks. Historical aliases name
//! the immutable timing table; only live arguments follow an owned tree copy.

use std::collections::{BTreeMap, BTreeSet};

use crate::{AudioPlacementTemplate, NodeId, ProjectDocument};

/// Transparent Split and occurrence isolation already copy complete raw owned
/// subtrees. Carry their clock expressions through the same physical ID map.
pub(crate) fn inherit(document: &mut ProjectDocument, mapping: &BTreeMap<NodeId, NodeId>) {
    let copied: Vec<_> = mapping
        .iter()
        .filter_map(|(old, new)| {
            document.audio_bindings.bindings.get(old).map(|binding| {
                let mut binding = binding.clone();
                remap_template(&mut binding.lattice, mapping);
                if let Some(resume) = &mut binding.resume {
                    for term in &mut resume.phase.terms {
                        remap_template(&mut term.placement, mapping);
                    }
                }
                (new.clone(), binding)
            })
        })
        .collect();
    document.audio_bindings.bindings.extend(copied);
}

fn remap_template(template: &mut AudioPlacementTemplate, mapping: &BTreeMap<NodeId, NodeId>) {
    for argument in &mut template.arguments {
        if let crate::AudioRepeatValue::Live { repeat } = &mut argument.value
            && let Some(mapped) = mapping.get(repeat)
        {
            *repeat = mapped.clone();
        }
    }
    for clause in &mut template.births {
        if let Some(mapped) = mapping.get(&clause.repeat) {
            clause.repeat = mapped.clone();
        }
    }
}

/// Removal never leaves dangling live owners or unreferenced timing objects.
/// Phase expressions can reference more than the primary sampling lattice.
pub(crate) fn prune(document: &mut ProjectDocument) {
    document
        .audio_bindings
        .bindings
        .retain(|owner, _| document.nodes.contains_key(owner));
    let mut retained = BTreeSet::new();
    for binding in document.audio_bindings.bindings.values() {
        retained.insert(binding.lattice.reference.timing.clone());
        if let Some(resume) = &binding.resume {
            for term in &resume.phase.terms {
                retained.insert(term.placement.reference.timing.clone());
            }
        }
    }
    document
        .audio_bindings
        .timings
        .retain(|identity, _| retained.contains(identity));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    fn node(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }
    fn revision(value: &str) -> RevisionId {
        RevisionId::new(value).unwrap()
    }

    fn fixture() -> ProjectDocument {
        let mut document = ProjectDocument::new(
            ProjectId::new("owned-clocks").unwrap(),
            revision("initial"),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(30_000, 1001).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            node("root"),
        )
        .unwrap();
        document.nodes.insert(
            node("root"),
            BeatNode::sequence("Root", vec![node("repeat")]),
        );
        document.nodes.insert(
            node("repeat"),
            BeatNode {
                label: "Repeat".into(),
                audio_edges: Default::default(),
                kind: NodeKind::Repeat {
                    child: node("hold"),
                    iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
                    gap: None,
                },
            },
        );
        document.nodes.insert(
            node("hold"),
            BeatNode::hold(
                "Pause",
                HoldRecipe {
                    duration: FrameDuration::new(2).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        );
        let layout = FrozenAudioLayout::capture(&document).unwrap();
        let timing = AudioTimingId {
            allocation: revision("timing"),
            ordinal: 0,
        };
        let template = AudioPlacementTemplate {
            reference: AudioReferenceClock {
                timing: timing.clone(),
                root: AudioClockRoot::ProjectRootRoundEven,
                physical: node("hold"),
            },
            arguments: vec![AudioRepeatArgument {
                reference_repeat: node("repeat"),
                value: AudioRepeatValue::Live {
                    repeat: node("repeat"),
                },
            }],
            births: vec![AudioBirthClause {
                repeat: node("repeat"),
                survivors: AudioBirthSurvivors::CapturedRepeat {
                    repeat: node("repeat"),
                },
                definition_root: node("hold"),
            }],
        };
        document.audio_bindings = AudioBindingState::new(
            vec![AudioTimingRecord { id: timing, layout }],
            BTreeMap::from([(
                node("hold"),
                OwnedAudioBinding {
                    lattice: template.clone(),
                    resume: Some(AudioResume {
                        local_boundary: ExactRatio::ONE,
                        phase: AudioLocalPhase {
                            constant: ExactRatio::new(1, 7).unwrap(),
                            terms: vec![AudioPhaseTerm {
                                placement: template,
                                from_local: ExactRatio::ZERO,
                                to_local: ExactRatio::ONE,
                            }],
                        },
                    }),
                },
            )]),
        )
        .unwrap();
        document.validate().unwrap();
        document
    }

    fn edit(
        document: &ProjectDocument,
        name: &str,
        command: Command,
    ) -> (ProjectDocument, EditTransaction) {
        let transaction = apply(
            document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: revision(name),
                command,
            },
        )
        .unwrap();
        (transaction.forward.apply(document).unwrap(), transaction)
    }

    #[test]
    fn split_copies_live_scope_and_phase_arguments_but_keeps_historical_aliases() {
        let before = fixture();
        let (after, transaction) = edit(
            &before,
            "split",
            Command::Split {
                node: node("repeat"),
                at: FrameDuration::new(2).unwrap(),
                identities: SplitIdentities {
                    nodes: ["left", "right", "copied_repeat", "copied_hold"]
                        .map(node)
                        .to_vec(),
                },
            },
        );
        assert_eq!(after.audio_bindings.timings, before.audio_bindings.timings);
        assert_eq!(
            after.audio_bindings.bindings[&node("hold")],
            before.audio_bindings.bindings[&node("hold")]
        );
        let copied = &after.audio_bindings.bindings[&node("copied_hold")];
        for template in [
            &copied.lattice,
            &copied.resume.as_ref().unwrap().phase.terms[0].placement,
        ] {
            assert_eq!(template.reference.physical, node("hold"));
            assert_eq!(template.arguments[0].reference_repeat, node("repeat"));
            assert_eq!(
                template.arguments[0].value,
                AudioRepeatValue::Live {
                    repeat: node("copied_repeat")
                }
            );
            assert_eq!(template.births[0].repeat, node("copied_repeat"));
            assert_eq!(template.births[0].definition_root, node("hold"));
        }
        assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    }

    #[test]
    fn occurrence_isolation_retains_the_selected_old_clock_and_undo_restores_state() {
        let before = fixture();
        let iteration = match &before.nodes[&node("repeat")].kind {
            NodeKind::Repeat { iterations, .. } => iterations.at(0).unwrap(),
            _ => unreachable!(),
        };
        let (after, transaction) = edit(
            &before,
            "isolate",
            Command::EditOccurrence {
                instance: InstancePath {
                    node: node("hold"),
                    repeats: vec![RepeatInstance {
                        node: node("repeat"),
                        iteration,
                    }],
                },
                edit: OccurrenceEdit::Rename {
                    label: "Selected pause".into(),
                },
                identities: OccurrenceIdentities {
                    nodes: vec![node("selected")],
                    marks: vec![],
                },
            },
        );
        assert_eq!(
            after.audio_bindings.bindings[&node("selected")],
            before.audio_bindings.bindings[&node("hold")]
        );
        assert_eq!(after.audio_bindings.timings.len(), 1);
        assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    }

    #[test]
    fn removing_the_last_owner_removes_its_tables_in_the_same_reversible_patch() {
        let before = fixture();
        let (after, transaction) = edit(
            &before,
            "delete",
            Command::Delete {
                node: node("repeat"),
            },
        );
        assert!(after.audio_bindings.is_empty());
        assert!(after.audio_bindings.timings.is_empty());
        assert!(transaction.forward.audio_bindings.is_some());
        assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    }

    #[test]
    fn pruning_keeps_a_table_still_referenced_only_by_a_phase_term() {
        let mut before = fixture();
        let second = AudioTimingId {
            allocation: revision("second-clock"),
            ordinal: 0,
        };
        let layout = before
            .audio_bindings
            .timings
            .values()
            .next()
            .unwrap()
            .clone();
        before.audio_bindings.timings.insert(second.clone(), layout);
        before
            .audio_bindings
            .bindings
            .get_mut(&node("hold"))
            .unwrap()
            .resume
            .as_mut()
            .unwrap()
            .phase
            .terms[0]
            .placement
            .reference
            .timing = second.clone();
        before
            .nodes
            .insert(node("other"), before.nodes[&node("hold")].clone());
        let NodeKind::Sequence { children } =
            &mut before.nodes.get_mut(&node("root")).unwrap().kind
        else {
            unreachable!()
        };
        children.push(node("other"));
        before.audio_bindings.bindings.insert(
            node("other"),
            OwnedAudioBinding {
                lattice: AudioPlacementTemplate {
                    reference: AudioReferenceClock {
                        timing: second.clone(),
                        root: AudioClockRoot::DefinitionPointCeil { root: node("hold") },
                        physical: node("hold"),
                    },
                    arguments: vec![],
                    births: vec![],
                },
                resume: None,
            },
        );
        before.validate().unwrap();
        let (after, transaction) = edit(
            &before,
            "delete-other",
            Command::Delete {
                node: node("other"),
            },
        );
        assert_eq!(after.audio_bindings.bindings.len(), 1);
        assert_eq!(after.audio_bindings.timings.len(), 2);
        assert!(after.audio_bindings.timings.contains_key(&second));
        assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
        let (empty, _) = edit(
            &after,
            "delete-last",
            Command::Delete {
                node: node("repeat"),
            },
        );
        assert!(empty.audio_bindings.is_empty());
    }
}
