//! Ownership transforms for retained sampling clocks. Historical aliases name
//! the immutable timing table; only live arguments follow an owned tree copy.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AudioBindingState, AudioBirthClause, AudioBirthSurvivors, AudioClockRoot,
    AudioPlacementTemplate, AudioRecipeKind, AudioReferenceClock, AudioRepeatArgument,
    AudioRepeatValue, AudioTimingId, DocumentError, DocumentErrorCode, FrozenAudioKind,
    FrozenAudioLayout, MAX_AUDIO_BINDING_ENTRIES, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId,
    NodeKind, OwnedAudioBinding, ProjectDocument,
};

/// Owner and Repeat-gap binding capture is retired (Original placement is
/// derived from the tree); the capture below is removed with its types.
const CAPTURE_OWNER_BINDINGS: bool = false;

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

/// The recipe inventory used by `Capture::walk`, without allocating a clock.
/// Includes dormant Repeat gaps and Preserve-input recipes, just as capture does.
pub(crate) fn has_unbound_recipes(document: &ProjectDocument) -> bool {
    document.nodes().iter().any(|(id, node)| match &node.kind {
        NodeKind::Source { .. } | NodeKind::Hold { .. } => {
            !document.audio_bindings().bindings().contains_key(id)
        }
        NodeKind::Retime {
            duration,
            mapping,
            pitch,
            ..
        } => {
            pitch.processes(mapping.duration() == *duration)
                && !document.audio_bindings().bindings().contains_key(id)
        }
        NodeKind::Repeat { gap, .. } => {
            gap.as_ref().is_some_and(|gap| gap.duration.frames() > 0)
                && !document.audio_bindings().gap_bindings().contains_key(id)
        }
        NodeKind::Sequence { .. } => false,
    })
}

/// [`capture_for_insertion`] whose new table may be a provisional slice
/// projecting the new bindings' aliases plus `aliases` (see
/// [`LayoutScope`]). The caller must name every alias any later template of
/// this command reads through the table, or extend it with
/// [`rescope_timing`] before validating such a template.
pub(crate) fn capture_for_insertion_scoped(
    document: &ProjectDocument,
    timing: AudioTimingId,
    aliases: BTreeSet<NodeId>,
) -> Result<(AudioBindingState, Option<FrozenAudioLayout>), DocumentError> {
    let capture = capture_with_placements(
        document,
        timing,
        true,
        PlacementCapture::None,
        Some(LayoutScope {
            aliases,
            placement_owners: None,
        }),
    )?;
    Ok((capture.state, capture.phase_only_layout))
}

/// The aliases a provisional timing table must project, beyond the new
/// bindings' own. Only `capture_with_placements` builds such a table, and
/// only when `command_work::scoped_timing_captures` permits it; otherwise the
/// complete layout is captured as before.
pub(crate) struct LayoutScope<'a> {
    pub(crate) aliases: BTreeSet<NodeId>,
    /// Owners whose captured placements will become reanchor steps.
    pub(crate) placement_owners: Option<&'a BTreeSet<NodeId>>,
}

/// Every alias the placement `template` reads through its timing table:
/// those `compact_new_timings` retains, so the final slice keeps them too.
pub(crate) fn template_aliases(template: &AudioPlacementTemplate, aliases: &mut BTreeSet<NodeId>) {
    aliases.insert(template.reference.physical.clone());
    match &template.reference.root {
        AudioClockRoot::ProjectRootRoundEven => {}
        AudioClockRoot::PreserveInputPointCeil { stage } => {
            aliases.insert(stage.clone());
        }
        AudioClockRoot::DefinitionPointCeil { root } => {
            aliases.insert(root.clone());
        }
        AudioClockRoot::GapDefinitionPointCeil { repeat } => {
            aliases.insert(repeat.clone());
        }
    }
    for argument in &template.arguments {
        aliases.insert(argument.reference_repeat.clone());
    }
    for clause in &template.births {
        aliases.insert(clause.definition_root.clone());
        if let AudioBirthSurvivors::CapturedRepeat { repeat } = &clause.survivors {
            aliases.insert(repeat.clone());
        }
    }
}

/// Whether every provisional table in `state` projects every alias that a
/// template naming it reads.
pub(crate) fn provisional_tables_project_references(state: &AudioBindingState) -> bool {
    if !state.has_provisional_timings() {
        return true;
    }
    state.owners().all(|(_, _, binding)| {
        binding.placements().all(
            |template| match state.timings.get(&template.reference.timing) {
                Some(layout) if layout.is_provisional() => {
                    let mut aliases = BTreeSet::new();
                    template_aliases(template, &mut aliases);
                    aliases.iter().all(|alias| layout.projects(alias))
                }
                _ => true,
            },
        )
    })
}

/// When `state`'s table `timing` is a provisional slice that does not
/// project every alias its referencing templates read, replace it with a
/// slice of `document`'s capture that does (or the complete capture).
/// `document` must be the document that table was captured from.
pub(crate) fn rescope_timing(
    document: &ProjectDocument,
    state: &mut AudioBindingState,
    timing: &AudioTimingId,
) -> Result<(), DocumentError> {
    let Some(layout) = state.timings.get(timing) else {
        return Ok(());
    };
    if !layout.is_provisional() {
        return Ok(());
    }
    let mut required = BTreeSet::new();
    for (_, _, binding) in state.owners() {
        for template in binding.placements() {
            if &template.reference.timing == timing {
                template_aliases(template, &mut required);
            }
        }
    }
    if required.iter().all(|alias| layout.projects(alias)) {
        return Ok(());
    }
    let replacement = match FrozenAudioLayout::capture_scoped(document, &required)? {
        Some(layout) => {
            crate::command_work::note_provisional_capture(true);
            layout
        }
        None => FrozenAudioLayout::capture(document)?,
    };
    state.timings.insert(timing.clone(), replacement);
    Ok(())
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
    capture_with_placements(document, timing, true, PlacementCapture::Root, None)
}

/// [`capture_for_composite_insertion`] for a caller that appends reanchor
/// steps only for `affected` owners' placements: the new table may then be a
/// provisional slice projecting exactly what those steps and the new bindings
/// read.
pub(crate) fn capture_for_composite_insertion_scoped(
    document: &ProjectDocument,
    timing: AudioTimingId,
    affected: &BTreeSet<NodeId>,
) -> Result<CompositeInsertionCapture, DocumentError> {
    capture_with_placements(
        document,
        timing,
        true,
        PlacementCapture::Root,
        Some(LayoutScope {
            aliases: BTreeSet::new(),
            placement_owners: Some(affected),
        }),
    )
}

/// Capture affected physical recipes on their current clocks, including a
/// processing stage's selected input clock. Existing insertion callers retain
/// their root-only movement policy.
pub(crate) fn capture_for_hold_duration_edit(
    document: &ProjectDocument,
    timing: AudioTimingId,
    affected: &BTreeSet<NodeId>,
) -> Result<CompositeInsertionCapture, DocumentError> {
    capture_with_placements(
        document,
        timing,
        true,
        PlacementCapture::RecipeClocks(affected),
        Some(LayoutScope {
            aliases: BTreeSet::new(),
            placement_owners: Some(affected),
        }),
    )
}

#[derive(Clone, Copy)]
enum PlacementCapture<'a> {
    None,
    Root,
    RecipeClocks(&'a BTreeSet<NodeId>),
}

fn capture(
    document: &ProjectDocument,
    timing: AudioTimingId,
    retain_timing: bool,
) -> Result<(AudioBindingState, Option<FrozenAudioLayout>), DocumentError> {
    let capture = capture_with_placements(
        document,
        timing,
        retain_timing,
        PlacementCapture::None,
        None,
    )?;
    Ok((capture.state, capture.phase_only_layout))
}

fn capture_with_placements(
    document: &ProjectDocument,
    timing: AudioTimingId,
    retain_timing: bool,
    placements: PlacementCapture<'_>,
    scope: Option<LayoutScope<'_>>,
) -> Result<CompositeInsertionCapture, DocumentError> {
    document.validate()?;
    if !document.sounds().is_empty() {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidTree,
            "audio binding capture cannot yet retain authored sound events",
        ));
    }
    // Original audio placement is derived from the current tree: commands
    // capture no owner or Repeat-gap bindings and leave the state unchanged.
    // Independent sound clocks install their own timing tables.
    if !CAPTURE_OWNER_BINDINGS {
        return Ok(CompositeInsertionCapture {
            state: document.audio_bindings().clone(),
            phase_only_layout: None,
            node_placements: BTreeMap::new(),
            gap_placements: BTreeMap::new(),
        });
    }
    let mut capture = Capture {
        document,
        timing: &timing,
        clock: AudioClockRoot::ProjectRootRoundEven,
        arguments: Vec::new(),
        births: Vec::new(),
        bindings: BTreeMap::new(),
        gap_bindings: BTreeMap::new(),
        placements,
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
            layout.charged_nodes() + layout.charged_lineage(),
        )?;
        if layout.is_provisional() {
            charge(&mut retained, layout.charged_runs())?;
        } else {
            for node in layout.nodes().values() {
                if let FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
                    charge(&mut retained, iterations.segment_count())?;
                }
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
    // At most one provisional table exists at a time, so a byte check that
    // needs exact counting can always substitute the complete layout.
    let scope = scope.filter(|_| {
        crate::command_work::scoped_timing_captures()
            && !document.audio_bindings().has_provisional_timings()
    });
    let layout = match &scope {
        Some(scope) => {
            let mut required = scope.aliases.clone();
            for binding in capture
                .bindings
                .values()
                .chain(capture.gap_bindings.values())
            {
                for template in binding.placements() {
                    template_aliases(template, &mut required);
                }
            }
            if let Some(owners) = scope.placement_owners {
                for (owner, template) in capture
                    .node_placements
                    .iter()
                    .chain(&capture.gap_placements)
                {
                    if owners.contains(owner) {
                        template_aliases(template, &mut required);
                    }
                }
            }
            match FrozenAudioLayout::capture_scoped(document, &required)? {
                Some(layout) => layout,
                None => FrozenAudioLayout::capture(document)?,
            }
        }
        None => FrozenAudioLayout::capture(document)?,
    };
    let bindings = capture.bindings;
    let gap_bindings = capture.gap_bindings;
    let node_placements = capture.node_placements;
    let gap_placements = capture.gap_placements;
    let mut result = document.audio_bindings().clone();
    if bindings.is_empty() && gap_bindings.is_empty() {
        // A phase-only layout has no reference yet. Keep it outside the valid
        // state through intermediate Split validation; insertion installs it
        // only while composing resume terms, then prunes any unused table.
        //
        // Byte bound: the complete capture checks only the layout's own
        // length here (no state check), and `capture_scoped` returns a slice
        // only when the complete layout's exact length is within the same
        // limit, so a provisional phase-only table refuses exactly when the
        // complete one would. Later checks of the state that installs it see
        // its complete length (`wire_bound_bytes`).
        debug_assert!(
            !layout.is_provisional()
                || layout
                    .wire_bound_bytes()
                    .is_ok_and(|bytes| bytes <= crate::MAX_DOCUMENT_JSON_BYTES)
        );
        return Ok(CompositeInsertionCapture {
            state: result,
            phase_only_layout: Some(layout),
            node_placements,
            gap_placements,
        });
    }
    result.timings.insert(timing.clone(), layout);
    result.bindings.extend(bindings);
    result.gap_bindings.extend(gap_bindings);
    document.validate_bindings_in_scope(&result)?;
    // Validation charged the complete counts, so it decided as it would for
    // the complete table. Only that table can be counted exactly.
    if result.has_provisional_timings() && !result.wire_bound_fits() {
        result
            .timings
            .insert(timing, FrozenAudioLayout::capture(document)?);
    }
    result.check_wire_size()?;
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
    placements: PlacementCapture<'a>,
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
                        NodeKind::Retime { duration, mapping, pitch, .. }
                            if pitch.processes(mapping.duration() == *duration)
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
        let current = match self.placements {
            PlacementCapture::None => false,
            PlacementCapture::Root => self.clock == AudioClockRoot::ProjectRootRoundEven,
            PlacementCapture::RecipeClocks(owners) => owners.contains(id),
        };
        // Charge both retained copies before cloning any lexical path. A
        // Preserve output moves on its enclosing clock; its intrinsic recipes
        // retain their input clock. Only the Hold-duration selector collects
        // movement placements in that input coordinate system.
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
            reference_local_offset: crate::ExactRatio::ZERO,
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

thread_local! {
    static REFERENCE_REPRESENTATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run `operation` with the uncompacted timing representation: every new
/// timing table keeps the complete captured layout and every reanchor step is
/// appended, even when it cannot change a resume. Commands then produce the
/// representation used before compaction. This exists only as the oracle for
/// equivalence qualification (identical resolved clocks and PCM); production
/// code never calls it. The setting is thread-local and restored on return.
#[doc(hidden)]
pub fn with_reference_timing_representation<R>(operation: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            REFERENCE_REPRESENTATION.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(REFERENCE_REPRESENTATION.with(|flag| flag.replace(true)));
    operation()
}

pub(crate) fn compact_representation() -> bool {
    !REFERENCE_REPRESENTATION.with(std::cell::Cell::get)
}

/// Slice every timing table introduced by this transaction to the aliases its
/// binding placements name. Captures snapshot the whole structure, but a
/// placement only projects its own alias through its ancestors; retaining the
/// rest would grow every later revision with the project size. Existing tables
/// are never rewritten, and tables named by sound clocks stay complete because
/// those compile and compare whole processing subtrees.
pub(crate) fn compact_new_timings(
    before: &AudioBindingState,
    after: &mut AudioBindingState,
) -> Result<(), DocumentError> {
    if !compact_representation() {
        return Ok(());
    }
    let mut required: BTreeMap<AudioTimingId, BTreeSet<NodeId>> = after
        .timings
        .keys()
        .filter(|timing| !before.timings.contains_key(*timing))
        .map(|timing| (timing.clone(), BTreeSet::new()))
        .collect();
    if required.is_empty() {
        return Ok(());
    }
    for journals in after.sound_clocks.values() {
        for journal in journals.values() {
            for reference in journal.clocks() {
                required.remove(reference.timing());
            }
        }
    }
    for (_, _, binding) in after.owners() {
        for template in binding.placements() {
            let Some(aliases) = required.get_mut(&template.reference.timing) else {
                continue;
            };
            template_aliases(template, aliases);
        }
    }
    for (timing, aliases) in required {
        let layout = &after.timings[&timing];
        if !aliases.iter().all(|alias| layout.projects(alias)) {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "a provisional timing table does not project a referenced alias",
            ));
        }
        let sliced = layout.sliced(&aliases)?;
        // A provisional table compares against the complete layout it
        // stands in for: slicing it to a subset of its aliases equals
        // slicing the complete capture, which is always smaller here.
        if sliced.nodes().len() < layout.charged_nodes() {
            after.timings.insert(timing, sliced);
        } else if layout.is_provisional() {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidTree,
                "a provisional timing table could not be compacted",
            ));
        }
    }
    if after.has_provisional_timings() {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidTree,
            "a provisional timing table outlived its command",
        ));
    }
    Ok(())
}

/// Removal never leaves dangling live owners or unreferenced timing objects.
/// Phase expressions can reference more than the primary sampling lattice.
pub(crate) fn prune(document: &mut ProjectDocument) {
    // `retain` visits keys in ascending order, so a cursor over the sorted
    // node identities answers `contains_key` for each owner in one merge.
    let mut nodes = document.nodes.keys().peekable();
    document.audio_bindings.bindings.retain(|owner, _| {
        while nodes.next_if(|node| *node < owner).is_some() {}
        let kept = nodes.peek() == Some(&owner);
        debug_assert_eq!(kept, document.nodes.contains_key(owner));
        kept
    });
    document.audio_bindings.gap_bindings.retain(|owner, _| {
        matches!(document.nodes.get(owner).map(|node| &node.kind), Some(NodeKind::Repeat { gap: Some(gap), .. }) if gap.duration != crate::FrameDuration::ZERO)
    });
    document
        .audio_bindings
        .sound_clocks
        .retain(|owner, journals| {
            if !document.nodes.contains_key(owner) {
                return false;
            }
            journals.retain(|sound, _| {
                document
                    .beat_sounds
                    .get(owner)
                    .is_some_and(|events| events.contains_key(sound))
            });
            !journals.is_empty()
        });
    let state = &mut document.audio_bindings;
    let mut retained = BTreeSet::new();
    // Consecutive owners usually name the same table; skip repeated inserts.
    let mut last = None;
    for binding in state.bindings.values().chain(state.gap_bindings.values()) {
        for template in binding.placements() {
            let timing = &template.reference.timing;
            if last != Some(timing) {
                retained.insert(timing);
                last = Some(timing);
            }
        }
    }
    for journals in state.sound_clocks.values() {
        for journal in journals.values() {
            retained.extend(journal.clocks().iter().map(|reference| reference.timing()));
        }
    }
    if retained.len() == state.timings.len()
        && retained
            .iter()
            .all(|identity| state.timings.contains_key(*identity))
    {
        // As many distinct retained identities as tables, each naming one:
        // every table is retained.
        return;
    }
    let retained: BTreeSet<AudioTimingId> = retained.into_iter().cloned().collect();
    state
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
                audio_treatments: Default::default(),
                framing: None,
                label: "Repeat".into(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Repeat {
                    child: node("hold"),
                    iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
                    gap: None,
                    escalation: None,
                },
                cutaways: Vec::new(),
                captions: Vec::new(),
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
            reference_local_offset: crate::ExactRatio::ZERO,
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
}
