//! Ownership transforms for retained sampling clocks. Historical aliases name
//! the immutable timing table; only live arguments follow an owned tree copy.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AudioBindingState, AudioBirthClause, AudioBirthSurvivors, AudioClockRoot,
    AudioPlacementTemplate, AudioRecipeKind, AudioReferenceClock, AudioRepeatArgument,
    AudioRepeatValue, AudioTimingId, DocumentError, DocumentErrorCode, FrozenAudioKind,
    FrozenAudioLayout, MAX_AUDIO_BINDING_ENTRIES, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId,
    NodeKind, OwnedAudioBinding, PitchPolicy, ProjectDocument,
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
/// Configured positive Repeat gaps are captured even before any play renders one.
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

/// Composite insertion moves every physical recipe in the selected root
/// suffix, including recipes whose sampling lattice was captured by an older
/// edit. These current placements are separate from that retained lattice.
#[derive(Debug)]
pub(crate) struct CompositeInsertionCapture {
    pub(crate) state: AudioBindingState,
    pub(crate) phase_only_layout: Option<FrozenAudioLayout>,
    pub(crate) node_placements: BTreeMap<NodeId, AudioPlacementTemplate>,
    pub(crate) gap_placements: BTreeMap<NodeId, AudioPlacementTemplate>,
}

pub(crate) fn capture_for_composite_insertion(
    document: &ProjectDocument,
    timing: AudioTimingId,
) -> Result<CompositeInsertionCapture, DocumentError> {
    capture_with_placements(document, timing, true, true)
}

fn capture(
    document: &ProjectDocument,
    timing: AudioTimingId,
    retain_timing: bool,
) -> Result<(AudioBindingState, Option<FrozenAudioLayout>), DocumentError> {
    let capture = capture_with_placements(document, timing, retain_timing, false)?;
    Ok((capture.state, capture.phase_only_layout))
}

fn capture_with_placements(
    document: &ProjectDocument,
    timing: AudioTimingId,
    retain_timing: bool,
    collect_root_placements: bool,
) -> Result<CompositeInsertionCapture, DocumentError> {
    document.validate()?;
    if !document.sounds().is_empty() {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidTree,
            "audio binding capture cannot yet retain authored sound events",
        ));
    }
    let mut capture = Capture {
        document,
        timing: &timing,
        clock: AudioClockRoot::ProjectRootRoundEven,
        arguments: Vec::new(),
        births: Vec::new(),
        bindings: BTreeMap::new(),
        gap_bindings: BTreeMap::new(),
        collect_root_placements,
        node_placements: BTreeMap::new(),
        gap_placements: BTreeMap::new(),
        entries: 0,
        visited: 0,
    };
    for (_, _, binding) in document.audio_bindings().owners() {
        for template in binding.placements() {
            charge(&mut capture.entries, template.entry_count())?;
        }
    }
    capture.walk()?;
    if capture.bindings.is_empty()
        && capture.gap_bindings.is_empty()
        && (!retain_timing || document.audio_bindings().is_empty())
    {
        return Ok(CompositeInsertionCapture {
            state: document.audio_bindings().clone(),
            phase_only_layout: None,
            node_placements: capture.node_placements,
            gap_placements: capture.gap_placements,
        });
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
    let gap_bindings = capture.gap_bindings;
    let node_placements = capture.node_placements;
    let gap_placements = capture.gap_placements;
    let mut result = document.audio_bindings().clone();
    if bindings.is_empty() && gap_bindings.is_empty() {
        // A phase-only layout has no reference yet. Keep it outside the valid
        // state through intermediate Split validation; insertion installs it
        // only while composing resume terms, then prunes any unused table.
        return Ok(CompositeInsertionCapture {
            state: result,
            phase_only_layout: Some(layout),
            node_placements,
            gap_placements,
        });
    }
    result.timings.insert(timing, layout);
    result.bindings.extend(bindings);
    result.gap_bindings.extend(gap_bindings);
    result.validate_for(document)?;
    result.to_json()?;
    Ok(CompositeInsertionCapture {
        state: result,
        phase_only_layout: None,
        node_placements,
        gap_placements,
    })
}

struct Capture<'a> {
    document: &'a ProjectDocument,
    timing: &'a AudioTimingId,
    clock: AudioClockRoot,
    arguments: Vec<AudioRepeatArgument>,
    births: Vec<AudioBirthClause>,
    bindings: BTreeMap<NodeId, OwnedAudioBinding>,
    gap_bindings: BTreeMap<NodeId, OwnedAudioBinding>,
    collect_root_placements: bool,
    node_placements: BTreeMap<NodeId, AudioPlacementTemplate>,
    gap_placements: BTreeMap<NodeId, AudioPlacementTemplate>,
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
                                self.capture_recipe(id, AudioRecipeKind::RepeatGap)?;
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
                            if let Some(overrides) = document.gap_overrides().get(id) {
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
        if preserve || matches!(node.kind, NodeKind::Source { .. } | NodeKind::Hold { .. }) {
            self.capture_recipe(id, AudioRecipeKind::Node)?;
        }
        Ok(())
    }

    fn capture_recipe(
        &mut self,
        id: &NodeId,
        recipe: AudioRecipeKind,
    ) -> Result<(), DocumentError> {
        let existing = match recipe {
            AudioRecipeKind::Node => self.document.audio_bindings().bindings(),
            AudioRecipeKind::RepeatGap => self.document.audio_bindings().gap_bindings(),
        };
        let unbound = !existing.contains_key(id);
        let current =
            self.collect_root_placements && self.clock == AudioClockRoot::ProjectRootRoundEven;
        // Charge both retained copies before cloning any lexical path. A
        // Preserve output moves on its enclosing clock; its intrinsic recipes
        // retain their input clock and never receive root-movement placements.
        let entries = 1
            + self.arguments.len()
            + self.births.len()
            + usize::from(recipe == AudioRecipeKind::RepeatGap);
        if unbound {
            charge(&mut self.entries, entries)?;
        }
        if current {
            charge(&mut self.entries, entries)?;
        }
        if unbound {
            let binding = OwnedAudioBinding {
                lattice: self.placement(id, recipe),
                resume: None,
                reanchors: Vec::new(),
            };
            match recipe {
                AudioRecipeKind::Node => &mut self.bindings,
                AudioRecipeKind::RepeatGap => &mut self.gap_bindings,
            }
            .insert(id.clone(), binding);
        }
        if current {
            let placement = self.placement(id, recipe);
            match recipe {
                AudioRecipeKind::Node => &mut self.node_placements,
                AudioRecipeKind::RepeatGap => &mut self.gap_placements,
            }
            .insert(id.clone(), placement);
        }
        Ok(())
    }

    fn placement(&self, id: &NodeId, recipe: AudioRecipeKind) -> AudioPlacementTemplate {
        AudioPlacementTemplate {
            reference: AudioReferenceClock {
                timing: self.timing.clone(),
                root: self.clock.clone(),
                physical: id.clone(),
                recipe,
            },
            gap_after: (recipe == AudioRecipeKind::RepeatGap)
                .then(|| AudioRepeatValue::Live { repeat: id.clone() }),
            arguments: self.arguments.clone(),
            births: self.births.clone(),
        }
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
    inherit_map(&mut document.audio_bindings.bindings, mapping);
    inherit_map(&mut document.audio_bindings.gap_bindings, mapping);
}

fn inherit_map(
    bindings: &mut BTreeMap<NodeId, OwnedAudioBinding>,
    mapping: &BTreeMap<NodeId, NodeId>,
) {
    let copied: Vec<_> = mapping
        .iter()
        .filter_map(|(old, new)| {
            bindings.get(old).map(|binding| {
                let mut binding = binding.clone();
                for template in binding.placements_mut() {
                    remap_template(template, mapping);
                }
                (new.clone(), binding)
            })
        })
        .collect();
    bindings.extend(copied);
}

fn remap_template(template: &mut AudioPlacementTemplate, mapping: &BTreeMap<NodeId, NodeId>) {
    for value in template
        .arguments
        .iter_mut()
        .map(|argument| &mut argument.value)
        .chain(template.gap_after.iter_mut())
    {
        if let crate::AudioRepeatValue::Live { repeat } = value
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
    document.audio_bindings.gap_bindings.retain(|owner, _| {
        matches!(document.nodes.get(owner).map(|node| &node.kind), Some(NodeKind::Repeat { gap: Some(gap), .. }) if gap.duration != crate::FrameDuration::ZERO)
    });
    let mut retained = BTreeSet::new();
    for (_, _, binding) in document.audio_bindings.owners() {
        for template in binding.placements() {
            retained.insert(template.reference.timing.clone());
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
                framing: None,
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
                    picture_context: None,
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
            gap_after: None,
            reference: AudioReferenceClock {
                recipe: crate::AudioRecipeKind::Node,
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
                    reanchors: Vec::new(),
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

    fn capture_timing(ordinal: u32) -> AudioTimingId {
        AudioTimingId {
            allocation: revision("composite-capture"),
            ordinal,
        }
    }

    fn capture_hold(duration: i64) -> BeatNode {
        BeatNode::hold("hold", capture_gap(duration))
    }

    fn capture_gap(duration: i64) -> HoldRecipe {
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(duration).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        }
    }

    fn capture_repeat(child: &str, count: u32, gap_duration: i64) -> BeatNode {
        BeatNode {
            framing: None,
            label: "repeat".into(),
            audio_edges: Default::default(),
            kind: NodeKind::Repeat {
                child: node(child),
                iterations: IterationOrder::new(revision("plays"), count).unwrap(),
                gap: (gap_duration > 0).then(|| capture_gap(gap_duration)),
            },
        }
    }

    fn capture_document(
        children: &[&str],
        nodes: impl IntoIterator<Item = (NodeId, BeatNode)>,
    ) -> ProjectDocument {
        let mut document = ProjectDocument::new(
            ProjectId::new("composite-capture").unwrap(),
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
        document.nodes.extend(nodes);
        document.nodes.insert(
            node("root"),
            BeatNode::sequence("root", children.iter().map(|name| node(name)).collect()),
        );
        document
    }

    #[test]
    fn composite_capture_refreshes_bound_placements_without_changing_owned_phase() {
        let mut before = fixture();
        let binding = before
            .audio_bindings
            .bindings
            .get_mut(&node("hold"))
            .unwrap();
        binding.reanchors.push(AudioReanchorStep {
            placement: binding.lattice.clone(),
            window: None,
        });
        let (before, _) = edit(
            &before,
            "reordered",
            Command::MovePlays {
                node: node("repeat"),
                start: 1,
                end: 2,
                destination: 0,
            },
        );
        let original = before.to_json().unwrap();
        let current = capture_for_composite_insertion(&before, capture_timing(0)).unwrap();
        assert_eq!(current.state, before.audio_bindings);
        assert_eq!(current.node_placements.len(), 1);
        assert!(current.gap_placements.is_empty());
        let placement = &current.node_placements[&node("hold")];
        assert_eq!(placement.reference.timing, capture_timing(0));
        assert_eq!(placement.reference.physical, node("hold"));
        let layout = current.phase_only_layout.as_ref().unwrap();
        assert_eq!(layout, &FrozenAudioLayout::capture(&before).unwrap());
        placement.validate(layout).unwrap();
        current.state.validate_for(&before).unwrap();
        let old_insertion = capture_for_insertion(&before, capture_timing(0)).unwrap();
        assert_eq!(old_insertion, (current.state, current.phase_only_layout));
        let retained_timing = before.audio_bindings.timings.keys().next().unwrap().clone();
        assert_eq!(
            capture_unbound_audio_bindings(&before, retained_timing.clone()).unwrap(),
            before.audio_bindings
        );
        assert_eq!(
            capture_for_composite_insertion(&before, retained_timing)
                .unwrap_err()
                .code,
            DocumentErrorCode::InvalidTree
        );
        assert_eq!(before.to_json().unwrap(), original);
    }

    #[test]
    fn composite_capture_keeps_compact_default_play_and_gap_branch_scopes() {
        let mut before = capture_document(
            &["repeat"],
            [
                (node("repeat"), capture_repeat("default", 1_000_000_000, 2)),
                (
                    node("default"),
                    BeatNode::sequence("default", vec![node("hold"), node("nested")]),
                ),
                (node("hold"), capture_hold(2)),
                (node("nested"), capture_repeat("nested_hold", 1, 1)),
                (node("nested_hold"), capture_hold(3)),
                (node("override"), capture_repeat("override_hold", 2, 2)),
                (node("override_hold"), capture_hold(4)),
                (node("gap_override"), capture_repeat("gap_hold", 1, 1)),
                (node("gap_hold"), capture_hold(5)),
            ],
        );
        let play = |ordinal| IterationId {
            allocation: revision("plays"),
            ordinal,
        };
        before.overrides.insert(
            node("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: play(4),
                root: node("override"),
            }])
            .unwrap(),
        );
        before.gap_overrides.insert(
            node("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: play(3),
                root: node("gap_override"),
            }])
            .unwrap(),
        );
        let fresh = capture_for_composite_insertion(&before, capture_timing(0)).unwrap();
        assert_eq!(
            fresh.state,
            capture_unbound_audio_bindings(&before, capture_timing(0)).unwrap()
        );
        assert!(fresh.phase_only_layout.is_none());
        assert_eq!(fresh.node_placements.len(), 4);
        assert_eq!(fresh.gap_placements.len(), 4);
        let layout = &fresh.state.timings[&capture_timing(0)];
        for placement in fresh
            .node_placements
            .values()
            .chain(fresh.gap_placements.values())
        {
            placement.validate(layout).unwrap();
        }
        let nested = &fresh.node_placements[&node("nested_hold")];
        assert_eq!(nested.arguments.len(), 2);
        assert_eq!(nested.births.len(), 2);
        assert_eq!(nested.births[0].definition_root, node("default"));
        assert_eq!(nested.births[1].definition_root, node("nested_hold"));
        for (owner, ordinal, branch) in [
            ("override_hold", 4, "override"),
            ("gap_hold", 3, "gap_override"),
        ] {
            let placement = &fresh.node_placements[&node(owner)];
            assert_eq!(placement.arguments.len(), 2);
            assert_eq!(placement.arguments[0].reference_repeat, node("repeat"));
            assert_eq!(
                placement.arguments[0].value,
                AudioRepeatValue::Captured {
                    iteration: play(ordinal)
                }
            );
            assert_eq!(placement.births.len(), 1);
            assert_eq!(placement.births[0].repeat, node(branch));
        }
        for owner in ["repeat", "nested", "override", "gap_override"] {
            let placement = &fresh.gap_placements[&node(owner)];
            assert_eq!(placement.reference.recipe, AudioRecipeKind::RepeatGap);
            assert_eq!(
                placement.gap_after,
                Some(AudioRepeatValue::Live {
                    repeat: node(owner)
                })
            );
        }
        before.audio_bindings = fresh.state;
        let bound = capture_for_composite_insertion(&before, capture_timing(1)).unwrap();
        assert_eq!(bound.state, before.audio_bindings);
        assert!(bound.phase_only_layout.is_some());
        for (owner, placement) in &bound.gap_placements {
            assert_eq!(placement.reference.timing, capture_timing(1));
            assert_eq!(placement.arguments, fresh.gap_placements[owner].arguments);
            assert_eq!(placement.births, fresh.gap_placements[owner].births);
        }
    }

    #[test]
    fn composite_capture_moves_preserve_output_but_keeps_intrinsic_recipes_on_input_clock() {
        let preserve = |child, duration, start, end| BeatNode {
            framing: None,
            label: "preserve".into(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: node(child),
                duration: FrameDuration::new(duration).unwrap(),
                mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
                pitch: PitchPolicy::Preserve,
                purpose: RetimePurpose::Edit,
            },
        };
        let mut before = capture_document(
            &["outside", "tail"],
            [
                (node("outside"), capture_repeat("outer", 2, 1)),
                (node("outer"), preserve("input", 12, 1, 7)),
                (
                    node("input"),
                    BeatNode::sequence("input", vec![node("x"), node("inside")]),
                ),
                (node("x"), capture_hold(2)),
                (node("inside"), capture_repeat("inner", 2, 1)),
                (node("inner"), preserve("unity", 3, 1, 5)),
                (node("unity"), preserve("a", 6, 0, 6)),
                (node("a"), capture_hold(6)),
                (node("tail"), capture_hold(1)),
            ],
        );
        let fresh = capture_for_composite_insertion(&before, capture_timing(0)).unwrap();
        assert_eq!(
            fresh.node_placements.keys().cloned().collect::<Vec<_>>(),
            [node("outer"), node("tail")]
        );
        assert_eq!(
            fresh.gap_placements.keys().cloned().collect::<Vec<_>>(),
            [node("outside")]
        );
        assert_eq!(fresh.state.bindings.len(), 5);
        assert_eq!(fresh.state.gap_bindings.len(), 2);
        assert_eq!(
            fresh.state.gap_bindings[&node("inside")]
                .lattice
                .reference
                .root,
            AudioClockRoot::PreserveInputPointCeil {
                stage: node("outer")
            }
        );
        assert_eq!(
            fresh.state.bindings[&node("a")].lattice.reference.root,
            AudioClockRoot::PreserveInputPointCeil {
                stage: node("inner")
            }
        );
        assert_eq!(
            fresh.state,
            capture_for_insertion(&before, capture_timing(0)).unwrap().0
        );
        before.audio_bindings = fresh.state;
        let bound = capture_for_composite_insertion(&before, capture_timing(1)).unwrap();
        assert_eq!(bound.state, before.audio_bindings);
        assert_eq!(bound.node_placements.len(), 2);
        assert_eq!(bound.gap_placements.len(), 1);
        before.audio_bindings.bindings.remove(&node("a"));
        let mixed = capture_for_composite_insertion(&before, capture_timing(2)).unwrap();
        assert!(mixed.phase_only_layout.is_none());
        assert_eq!(mixed.state.timings.len(), 2);
        assert_eq!(
            mixed.state.bindings[&node("outer")],
            before.audio_bindings.bindings[&node("outer")]
        );
        let new_intrinsic = &mixed.state.bindings[&node("a")].lattice;
        assert_eq!(new_intrinsic.reference.timing, capture_timing(2));
        assert_eq!(
            new_intrinsic.reference.root,
            AudioClockRoot::PreserveInputPointCeil {
                stage: node("inner")
            }
        );
        assert!(!mixed.node_placements.contains_key(&node("a")));
        mixed.state.validate_for(&before).unwrap();
    }

    #[test]
    fn composite_capture_rejects_aggregate_lexical_expansion_without_mutation() {
        let mut nodes = BTreeMap::new();
        for index in 0..100 {
            let child = if index == 99 {
                "leaves".into()
            } else {
                format!("r{}", index + 1)
            };
            nodes.insert(node(&format!("r{index}")), capture_repeat(&child, 1, 0));
        }
        let leaves: Vec<_> = (0..300)
            .map(|index| node(&format!("leaf{index}")))
            .collect();
        for leaf in &leaves {
            nodes.insert(leaf.clone(), capture_hold(1));
        }
        nodes.insert(node("leaves"), BeatNode::sequence("leaves", leaves));
        let before = capture_document(&["r0"], nodes);
        let original = before.to_json().unwrap();
        let error = capture_for_composite_insertion(&before, capture_timing(0)).unwrap_err();
        assert_eq!(error.code, DocumentErrorCode::LimitExceeded);
        assert!(error.message.contains("capture complexity"));
        assert_eq!(before.to_json().unwrap(), original);
    }

    #[test]
    fn split_copies_live_scope_and_phase_arguments_but_keeps_historical_aliases() {
        let mut before = fixture();
        let mut step = before.audio_bindings.bindings[&node("hold")]
            .lattice
            .clone();
        let original = before.audio_bindings.timings[&step.reference.timing].clone();
        step.reference.timing = AudioTimingId {
            allocation: revision("step_only"),
            ordinal: 0,
        };
        before
            .audio_bindings
            .timings
            .insert(step.reference.timing.clone(), original);
        before
            .audio_bindings
            .bindings
            .get_mut(&node("hold"))
            .unwrap()
            .reanchors
            .push(AudioReanchorStep {
                placement: step,
                window: None,
            });
        // Pruning must retain a clock referenced only by a chronological step.
        prune(&mut before);
        assert_eq!(before.audio_bindings.timings.len(), 2);
        before.validate().unwrap();
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
            &copied.reanchors[0].placement,
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
        let current = capture_for_composite_insertion(&after, capture_timing(0)).unwrap();
        assert_eq!(current.state, after.audio_bindings);
        let placement = &current.node_placements[&node("copied_hold")];
        assert_eq!(placement.reference.physical, node("copied_hold"));
        assert_eq!(
            placement.arguments[0].reference_repeat,
            node("copied_repeat")
        );
        assert_eq!(placement.births[0].definition_root, node("copied_hold"));
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
                reanchors: Vec::new(),
                lattice: AudioPlacementTemplate {
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: crate::AudioRecipeKind::Node,
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
