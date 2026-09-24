//! Ownership transforms for retained sampling clocks. Historical aliases name
//! the immutable timing table; only live arguments follow an owned tree copy.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AudioBindingState, AudioBirthClause, AudioBirthSurvivors, AudioClockRoot,
    AudioPlacementTemplate, AudioReferenceClock, AudioRepeatArgument, AudioRepeatValue,
    AudioTimingId, DocumentError, DocumentErrorCode, FrozenAudioKind, FrozenAudioLayout,
    MAX_AUDIO_BINDING_ENTRIES, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId, NodeKind,
    OwnedAudioBinding, PitchPolicy, ProjectDocument,
};

/// Capture the current sampling lattice of every previously unbound physical
/// node. All new bindings share one pre-edit timing layout; existing lattices
/// and resume expressions remain unchanged. Repeat defaults are captured even
/// when no current play uses them. No play order is expanded.
///
/// The caller owns timing identity allocation and document installation. This
/// pure operation neither authors a command nor changes its input. When every
/// physical node is already bound, it returns the existing state without using
/// the supplied identity or retaining another timing layout.
/// Nonempty Repeat gaps are rejected because embedded gap recipes do not yet
/// have representable binding ownership.
pub fn capture_unbound_audio_bindings(
    document: &ProjectDocument,
    timing: AudioTimingId,
) -> Result<AudioBindingState, DocumentError> {
    capture(document, timing, false).map(|(bindings, _)| bindings)
}

/// Insertion also needs the current clock for composed resume terms when all
/// physical recipes already have an older lattice.
pub(crate) fn capture_for_insertion(
    document: &ProjectDocument,
    timing: AudioTimingId,
) -> Result<(AudioBindingState, Option<FrozenAudioLayout>), DocumentError> {
    capture(document, timing, true)
}

fn capture(
    document: &ProjectDocument,
    timing: AudioTimingId,
    retain_timing: bool,
) -> Result<(AudioBindingState, Option<FrozenAudioLayout>), DocumentError> {
    document.validate()?;
    let mut capture = Capture {
        document,
        timing: &timing,
        clock: AudioClockRoot::ProjectRootRoundEven,
        arguments: Vec::new(),
        births: Vec::new(),
        bindings: BTreeMap::new(),
        entries: 0,
        visited: 0,
    };
    for binding in document.audio_bindings().bindings().values() {
        for template in std::iter::once(&binding.lattice).chain(
            binding
                .resume
                .iter()
                .flat_map(|resume| resume.phase.terms.iter().map(|term| &term.placement)),
        ) {
            charge(
                &mut capture.entries,
                1 + template.arguments.len() + template.births.len(),
            )?;
        }
    }
    capture.walk()?;
    if capture.bindings.is_empty() && (!retain_timing || document.audio_bindings().is_empty()) {
        return Ok((document.audio_bindings().clone(), None));
    }
    if document.audio_bindings().timings().contains_key(&timing) {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidTree,
            "audio capture timing identity is already retained",
        ));
    }
    // Charge the complete aggregate timing inventory before cloning any layout
    // or existing binding state. The final validator also charges path work.
    let mut retained = capture.entries;
    for layout in document.audio_bindings().timings().values() {
        charge(
            &mut retained,
            layout.nodes().len() + layout.audio_lineage().len(),
        )?;
        for node in layout.nodes().values() {
            if let FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
                charge(&mut retained, iterations.segment_count())?;
            }
        }
    }
    charge(
        &mut retained,
        document.nodes().len() + document.audio_lineage().len(),
    )?;
    for node in document.nodes().values() {
        if let NodeKind::Repeat { iterations, .. } = &node.kind {
            charge(&mut retained, iterations.segment_count())?;
        }
    }
    let layout = FrozenAudioLayout::capture(document)?;
    let bindings = capture.bindings;
    let mut result = document.audio_bindings().clone();
    if bindings.is_empty() {
        // A phase-only layout has no reference yet. Keep it outside the valid
        // state through intermediate Split validation; insertion installs it
        // only while composing resume terms, then prunes any unused table.
        return Ok((result, Some(layout)));
    }
    result.timings.insert(timing, layout);
    result.bindings.extend(bindings);
    result.validate_for(document)?;
    result.to_json()?;
    Ok((result, None))
}

struct Capture<'a> {
    document: &'a ProjectDocument,
    timing: &'a AudioTimingId,
    clock: AudioClockRoot,
    arguments: Vec<AudioRepeatArgument>,
    births: Vec<AudioBirthClause>,
    bindings: BTreeMap<NodeId, OwnedAudioBinding>,
    entries: usize,
    visited: usize,
}

enum CaptureStep<'a> {
    Node(&'a NodeId, usize),
    RepeatBranch {
        repeat: &'a NodeId,
        child: &'a NodeId,
        iteration: Option<&'a crate::IterationId>,
        depth: usize,
    },
    LeaveRepeat {
        default: bool,
    },
    RestoreClock {
        clock: AudioClockRoot,
        arguments: Vec<AudioRepeatArgument>,
        births: Vec<AudioBirthClause>,
    },
}

impl Capture<'_> {
    fn walk(&mut self) -> Result<(), DocumentError> {
        let document = self.document;
        let mut pending = vec![CaptureStep::Node(document.root(), 0)];
        while let Some(step) = pending.pop() {
            // A validated owned tree has at most one pending branch per edge
            // plus one restoration per active ancestor. No event clones paths.
            if pending.len() > MAX_DOCUMENT_NODES + MAX_DOCUMENT_DEPTH {
                return Err(capture_limit());
            }
            match step {
                CaptureStep::LeaveRepeat { default } => {
                    self.arguments.pop();
                    if default {
                        self.births.pop();
                    }
                }
                CaptureStep::RestoreClock {
                    clock,
                    arguments,
                    births,
                } => {
                    self.clock = clock;
                    self.arguments = arguments;
                    self.births = births;
                }
                CaptureStep::RepeatBranch {
                    repeat,
                    child,
                    iteration,
                    depth,
                } => {
                    self.arguments.push(AudioRepeatArgument {
                        reference_repeat: repeat.clone(),
                        value: iteration.map_or_else(
                            || AudioRepeatValue::Live {
                                repeat: repeat.clone(),
                            },
                            |iteration| AudioRepeatValue::Captured {
                                iteration: iteration.clone(),
                            },
                        ),
                    });
                    if iteration.is_none() {
                        self.births.push(AudioBirthClause {
                            repeat: repeat.clone(),
                            survivors: AudioBirthSurvivors::CapturedRepeat {
                                repeat: repeat.clone(),
                            },
                            definition_root: child.clone(),
                        });
                    }
                    pending.push(CaptureStep::LeaveRepeat {
                        default: iteration.is_none(),
                    });
                    pending.push(CaptureStep::Node(child, depth));
                }
                CaptureStep::Node(id, depth) => {
                    let node = &document.nodes()[id];
                    let preserve = matches!(
                        &node.kind,
                        NodeKind::Retime { duration, mapping, pitch: PitchPolicy::Preserve, .. }
                            if mapping.duration() != *duration
                    );
                    self.capture_node(id, depth, preserve)?;
                    match &node.kind {
                        NodeKind::Sequence { children } => pending.extend(
                            children
                                .iter()
                                .rev()
                                .map(|child| CaptureStep::Node(child, depth + 1)),
                        ),
                        NodeKind::Repeat { child, gap, .. } => {
                            if gap.as_ref().is_some_and(|gap| gap.duration.frames() > 0) {
                                return Err(DocumentError::new(
                                    DocumentErrorCode::InvalidTree,
                                    "audio binding capture does not support Repeat gap binding ownership",
                                ));
                            }
                            if let Some(overrides) = document.overrides().get(id) {
                                pending.extend(overrides.iter().rev().map(|(iteration, child)| {
                                    CaptureStep::RepeatBranch {
                                        repeat: id,
                                        child,
                                        iteration: Some(iteration),
                                        depth: depth + 1,
                                    }
                                }));
                            }
                            pending.push(CaptureStep::RepeatBranch {
                                repeat: id,
                                child,
                                iteration: None,
                                depth: depth + 1,
                            });
                        }
                        NodeKind::Retime { child, .. } => {
                            if preserve {
                                pending.push(CaptureStep::RestoreClock {
                                    clock: std::mem::replace(
                                        &mut self.clock,
                                        AudioClockRoot::PreserveInputPointCeil {
                                            stage: id.clone(),
                                        },
                                    ),
                                    arguments: std::mem::take(&mut self.arguments),
                                    births: std::mem::take(&mut self.births),
                                });
                            }
                            pending.push(CaptureStep::Node(child, depth + 1));
                        }
                        NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn capture_node(
        &mut self,
        id: &NodeId,
        depth: usize,
        preserve: bool,
    ) -> Result<(), DocumentError> {
        if depth > MAX_DOCUMENT_DEPTH || self.visited == MAX_DOCUMENT_NODES {
            return Err(capture_limit());
        }
        self.visited += 1;
        let document = self.document;
        let node = &document.nodes()[id];
        if (preserve || matches!(node.kind, NodeKind::Source { .. } | NodeKind::Hold { .. }))
            && !document.audio_bindings().bindings().contains_key(id)
        {
            charge(
                &mut self.entries,
                1 + self.arguments.len() + self.births.len(),
            )?;
            self.bindings.insert(
                id.clone(),
                OwnedAudioBinding {
                    lattice: AudioPlacementTemplate {
                        reference: AudioReferenceClock {
                            timing: self.timing.clone(),
                            root: self.clock.clone(),
                            physical: id.clone(),
                        },
                        arguments: self.arguments.clone(),
                        births: self.births.clone(),
                    },
                    resume: None,
                },
            );
        }
        Ok(())
    }
}

fn charge(used: &mut usize, additional: usize) -> Result<(), DocumentError> {
    *used = used
        .checked_add(additional)
        .filter(|used| *used <= MAX_AUDIO_BINDING_ENTRIES)
        .ok_or_else(capture_limit)?;
    Ok(())
}

fn capture_limit() -> DocumentError {
    DocumentError::new(
        DocumentErrorCode::LimitExceeded,
        "audio binding capture complexity limit",
    )
}

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
