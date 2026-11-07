//! Proof of one authored affine Retime boundary change, before sound-program
//! installation. This is not an unchanged-clock alias or command admission.

use crate::{
    AudioTimingId, BeatNode, BeatSound, Command, DocumentError, DocumentErrorCode, ExactFrameRange,
    ExactRatio, FrameDuration, FrameRange, FrozenAudioLayout, MAX_AUDIO_BINDING_ENTRIES,
    MAX_DOCUMENT_DEPTH, NodeId, NodeKind, PitchPolicy, ProjectDocument, ProjectFrame,
    RetimePurpose, SoundClockJournal, SoundId, ValidatedDocument,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{self, Write};

/// Which side of the edited stage an existing beat-owned voice enters.
/// PlacementOnly voices can move when a preceding branch changes length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundRetimeCohortKind {
    ChildInput,
    StageOutput,
    AncestorOutput,
    PlacementOnly,
}

/// Exact affine geometry. Selected input and effective output are separate
/// from the integral recipe count used by native DSP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundRetimeStage<'a> {
    stage: &'a NodeId,
    child: &'a NodeId,
    selection: ExactFrameRange,
    extent: ExactRatio,
    recipe_duration: FrameDuration,
    pitch: PitchPolicy,
}
impl<'a> SoundRetimeStage<'a> {
    pub fn stage(&self) -> &'a NodeId {
        self.stage
    }
    pub fn child(&self) -> &'a NodeId {
        self.child
    }
    pub fn selection(&self) -> ExactFrameRange {
        self.selection
    }
    pub fn extent(&self) -> ExactRatio {
        self.extent
    }
    pub fn recipe_duration(&self) -> FrameDuration {
        self.recipe_duration
    }
    pub fn pitch(&self) -> PitchPolicy {
        self.pitch
    }
    /// Map an output-local boundary to input-local geometry. This neither
    /// relabels PCM nor performs Preserve nor defines an output lattice.
    pub fn input_at(&self, output: ExactRatio) -> Result<ExactRatio, DocumentError> {
        if output.compare_integer(0).is_lt() || output.compare(self.extent).is_gt() {
            return Err(invalid("Retime proof boundary is outside its output"));
        }
        Ok(self.selection.start.checked_add(
            output
                .checked_mul(self.selection.end.checked_sub(self.selection.start)?)?
                .checked_div(self.extent)?,
        )?)
    }
}

/// One event with its complete old journal borrowed from admitted input.
/// Paths retain physical Repeat/override nodes without expanding plays.
#[derive(Debug)]
pub struct SoundRetimeCohort<'a> {
    owner: &'a NodeId,
    sound: &'a SoundId,
    event: &'a BeatSound,
    kind: SoundRetimeCohortKind,
    journal: Option<&'a SoundClockJournal>,
    before_extent: ExactRatio,
    after_extent: ExactRatio,
    before_ancestors: Vec<&'a NodeId>,
    after_ancestors: Vec<&'a NodeId>,
}
impl<'a> SoundRetimeCohort<'a> {
    pub fn owner(&self) -> &'a NodeId {
        self.owner
    }
    pub fn sound(&self) -> &'a SoundId {
        self.sound
    }
    pub fn event(&self) -> &'a BeatSound {
        self.event
    }
    pub fn kind(&self) -> SoundRetimeCohortKind {
        self.kind
    }
    pub fn journal(&self) -> Option<&'a SoundClockJournal> {
        self.journal
    }
    pub fn before_extent(&self) -> ExactRatio {
        self.before_extent
    }
    pub fn after_extent(&self) -> ExactRatio {
        self.after_extent
    }
    /// Nearest parent first, including every physical Repeat and processor.
    pub fn before_ancestors(&self) -> &[&'a NodeId] {
        &self.before_ancestors
    }
    pub fn after_ancestors(&self) -> &[&'a NodeId] {
        &self.after_ancestors
    }
}

/// An authored boundary proof tied to its immutable evidence pair. It proves
/// neither PCM equivalence nor an executable sound route. Existing clock
/// correspondence and command refusals remain authoritative until a complete
/// sound processing program consumes this evidence.
#[derive(Debug)]
pub struct SoundRetimeTransition<'a> {
    before: &'a ValidatedDocument,
    after: &'a ProjectDocument,
    old_stage: Option<SoundRetimeStage<'a>>,
    new_stage: SoundRetimeStage<'a>,
    cohorts: Vec<SoundRetimeCohort<'a>>,
    no_op: bool,
    work: usize,
}
impl<'a> SoundRetimeTransition<'a> {
    /// Derive only WrapRetime or SetRetime against actual authored trees.
    ///
    /// The after document is a structural candidate before sound-program
    /// replacement. Its old journals must be intact but can fail old unchanged-
    /// subtree validation because processing intentionally changed. Full final
    /// document admission remains mandatory. Marks, Original bindings and
    /// lineage remain subject to their own edit validators.
    pub fn derive(
        before: &'a ValidatedDocument,
        after: &'a ProjectDocument,
        command: &'a Command,
        maximum_work: usize,
    ) -> Result<Self, DocumentError> {
        let mut budget = Budget::new(maximum_work)?;
        let old = before.document();
        // Charge serialized comparison inputs before deep equality, cloning or
        // structural indexing. Streaming stops at the caller's hard ceiling.
        budget.serialized(&(
            old.nodes(),
            old.audio_lineage(),
            old.overrides(),
            old.gap_overrides(),
            old.derived_clocks(),
            old.assets(),
            old.source_interpretations(),
            old.beat_sounds(),
            old.audio_bindings().sound_clocks(),
            old.sound_allowances(),
            old.sounds(),
            old.sound_routes(),
        ))?;
        budget.serialized(&(
            after.nodes(),
            after.audio_lineage(),
            after.overrides(),
            after.gap_overrides(),
            after.derived_clocks(),
            after.assets(),
            after.source_interpretations(),
            after.beat_sounds(),
            after.audio_bindings().sound_clocks(),
            after.sound_allowances(),
            after.sounds(),
            after.sound_routes(),
        ))?;
        if old.schema_version() != after.schema_version()
            || old.project_id() != after.project_id()
            || old.presentation_basis() != after.presentation_basis()
            || old.basis_state != after.basis_state
            || old.root() != after.root()
            || old.assets() != after.assets()
            || old.source_interpretations() != after.source_interpretations()
            || old.beat_sounds() != after.beat_sounds()
            || old.audio_bindings().sound_clocks() != after.audio_bindings().sound_clocks()
            || !same_timings(
                old.audio_bindings().timings(),
                after.audio_bindings().timings(),
                &mut budget,
            )?
            || old.sounds() != after.sounds()
            || old.sound_routes() != after.sound_routes()
            || old.sound_allowances != after.sound_allowances
        {
            return Err(invalid("Retime proof changed its source or sound evidence"));
        }
        let before_parents = parents(old, &mut budget)?;
        let (stage, child, parent, expected_parent, old_stage_id, no_op) = match command {
            Command::WrapRetime {
                node,
                id,
                duration,
                pitch,
            } => {
                if old.nodes().contains_key(id) || node == old.root() || duration.frames() == 0 {
                    return Err(invalid("Retime wrap requires one fresh non-root stage"));
                }
                let parent = *before_parents
                    .get(node)
                    .ok_or_else(|| invalid("Retime wrap child has no parent"))?;
                let selected = before
                    .recipe_durations()
                    .get(node)
                    .ok_or_else(|| invalid("Retime wrap child is missing"))?;
                let mapping = FrameRange::new(ProjectFrame(0), ProjectFrame(selected.frames()))?;
                let expected = BeatNode {
                    label: "Retime".into(),
                    kind: NodeKind::Retime {
                        child: node.clone(),
                        duration: *duration,
                        mapping,
                        pitch: *pitch,
                        purpose: RetimePurpose::Edit,
                    },
                    audio_treatments: Default::default(),
                    framing: None,
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                };
                if after.nodes().get(id) != Some(&expected) {
                    return Err(invalid("Retime wrap does not match its authored command"));
                }
                let mut expected_parent = old.nodes()[parent].clone();
                let mut replacements = 0;
                match &mut expected_parent.kind {
                    NodeKind::Sequence { children } => {
                        for child in children {
                            if child == node {
                                *child = id.clone();
                                replacements += 1;
                            }
                        }
                    }
                    NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. }
                        if child == node =>
                    {
                        *child = id.clone();
                        replacements += 1;
                    }
                    _ => {}
                }
                check_overrides(old, after, Some((parent, node, id, replacements == 0)))?;
                if replacements > 1 {
                    return Err(invalid("Retime wrap has no unique child edge"));
                }
                (id, node, Some(parent), Some(expected_parent), None, false)
            }
            Command::SetRetime {
                node,
                duration,
                pitch,
            } => {
                let previous = old
                    .nodes()
                    .get(node)
                    .ok_or_else(|| invalid("Retime update stage is missing"))?;
                let NodeKind::Retime {
                    child,
                    duration: old_duration,
                    pitch: old_pitch,
                    purpose: RetimePurpose::Edit,
                    ..
                } = &previous.kind
                else {
                    return Err(invalid("Retime update requires an ordinary Edit stage"));
                };
                if duration.frames() == 0 {
                    return Err(invalid("Retime output must be positive"));
                }
                let mut expected = previous.clone();
                let NodeKind::Retime {
                    duration: target,
                    pitch: policy,
                    ..
                } = &mut expected.kind
                else {
                    unreachable!()
                };
                *target = *duration;
                *policy = *pitch;
                if after.nodes().get(node) != Some(&expected) {
                    return Err(invalid("Retime update changed its input or owner recipe"));
                }
                check_overrides(old, after, None)?;
                (
                    node,
                    child,
                    None,
                    None,
                    Some(node),
                    old_duration == duration && old_pitch == pitch,
                )
            }
            _ => return Err(invalid("Retime proof requires WrapRetime or SetRetime")),
        };
        if after.nodes().len() != old.nodes().len() + usize::from(old_stage_id.is_none()) {
            return Err(invalid(
                "Retime transition added or removed unrelated nodes",
            ));
        }
        for (id, node) in old.nodes() {
            budget.spend(1)?;
            if Some(id) == old_stage_id {
                continue;
            }
            let expected = if Some(id) == parent {
                expected_parent.as_ref().expect("wrap parent")
            } else {
                node
            };
            if after.nodes().get(id) != Some(expected) {
                return Err(invalid("Retime transition changed another authored node"));
            }
        }
        check_derived(old, after, stage, old_stage_id.is_some(), no_op)?;
        // Capture validates finite exact primitive geometry and unique parents
        // without recursively validating the intentionally old sound journals.
        let old_layout = before.scope(|| FrozenAudioLayout::capture_structural(old))?;
        let new_layout = FrozenAudioLayout::capture_structural(after)?;
        let new_stage = stage_geometry(after, &new_layout, stage)?;
        let old_stage = old_stage_id
            .map(|id| stage_geometry(old, &old_layout, id))
            .transpose()?;
        if let Some(old_stage) = &old_stage {
            if old_stage.selection != new_stage.selection {
                return Err(invalid("Retime update changed its exact selected input"));
            }
        } else if new_stage.selection.start != ExactRatio::ZERO
            || new_stage.selection.end != old_layout.node_extent(child)?
        {
            return Err(invalid("Retime wrap omitted part of its exact child"));
        }
        let durations = after.recipe_structural_durations()?;
        crate::sound_events::validate(after, &durations)?;
        let after_parents = parents(after, &mut budget)?;
        let stage_ancestors = path(&before_parents, child, &mut budget)?;
        let mut cohorts = Vec::new();
        for (owner, events) in old.beat_sounds() {
            let before_ancestors = path(&before_parents, owner, &mut budget)?;
            let after_ancestors = path(&after_parents, owner, &mut budget)?;
            let kind = if owner == stage {
                SoundRetimeCohortKind::StageOutput
            } else if owner == child || before_ancestors.contains(&child) {
                SoundRetimeCohortKind::ChildInput
            } else if stage_ancestors.contains(&owner) {
                SoundRetimeCohortKind::AncestorOutput
            } else {
                SoundRetimeCohortKind::PlacementOnly
            };
            for (sound, event) in events {
                budget.spend(1 + before_ancestors.len() + after_ancestors.len())?;
                cohorts.push(SoundRetimeCohort {
                    owner,
                    sound,
                    event,
                    kind,
                    journal: old
                        .audio_bindings()
                        .sound_clocks()
                        .get(owner)
                        .and_then(|events| events.get(sound)),
                    before_extent: old_layout.node_extent(owner)?,
                    after_extent: new_layout.node_extent(owner)?,
                    before_ancestors: before_ancestors.clone(),
                    after_ancestors: after_ancestors.clone(),
                });
            }
        }
        Ok(Self {
            before,
            after,
            old_stage,
            new_stage,
            cohorts,
            no_op,
            work: budget.used,
        })
    }
    pub fn before(&self) -> &'a ValidatedDocument {
        self.before
    }
    pub fn after(&self) -> &'a ProjectDocument {
        self.after
    }
    pub fn old_stage(&self) -> Option<&SoundRetimeStage<'a>> {
        self.old_stage.as_ref()
    }
    pub fn new_stage(&self) -> &SoundRetimeStage<'a> {
        &self.new_stage
    }
    pub fn cohorts(&self) -> &[SoundRetimeCohort<'a>] {
        &self.cohorts
    }
    /// Explicit project-output events remain outside the edited child stage.
    /// Their independent saved routes are unchanged evidence, not child input.
    pub fn project_output_sounds(&self) -> &'a BTreeMap<SoundId, crate::SoundEvent> {
        self.before.sounds()
    }
    pub fn project_output_routes(&self) -> &'a BTreeMap<SoundId, crate::RootSoundRoute> {
        self.before.sound_routes()
    }
    pub fn is_no_op(&self) -> bool {
        self.no_op
    }
    pub fn work(&self) -> usize {
        self.work
    }
}
fn stage_geometry<'a>(
    document: &'a ProjectDocument,
    layout: &FrozenAudioLayout,
    stage: &'a NodeId,
) -> Result<SoundRetimeStage<'a>, DocumentError> {
    let NodeKind::Retime {
        child,
        duration,
        pitch,
        purpose: RetimePurpose::Edit,
        ..
    } = &document
        .nodes()
        .get(stage)
        .ok_or_else(|| invalid("Retime stage is absent"))?
        .kind
    else {
        return Err(invalid("Retime boundary is not an Edit stage"));
    };
    let clock = layout.retime_clock(stage)?;
    Ok(SoundRetimeStage {
        stage,
        child,
        selection: ExactFrameRange::new(clock.child_start, clock.child_end)?,
        extent: clock.extent,
        recipe_duration: *duration,
        pitch: *pitch,
    })
}
fn check_derived(
    old: &ProjectDocument,
    after: &ProjectDocument,
    stage: &NodeId,
    update: bool,
    no_op: bool,
) -> Result<(), DocumentError> {
    for (id, previous) in old.derived_clocks() {
        let Some(current) = after.derived_clocks().get(id) else {
            return Err(invalid("Retime transition removed an exact child clock"));
        };
        if id != stage || no_op {
            if current != previous {
                return Err(invalid("Retime changed another exact clock"));
            }
        } else {
            let NodeKind::Retime { duration, .. } = &after.nodes()[stage].kind else {
                unreachable!()
            };
            if !update
                || current.extent != ExactRatio::integer(duration.frames())
                || current.child_start != previous.child_start
                || current.child_end != previous.child_end
                || current.preserve_input != previous.preserve_input
            {
                return Err(invalid("Retime update changed its retained exact input"));
            }
        }
    }
    if old.derived_clocks().len() != after.derived_clocks().len() {
        return Err(invalid("Retime transition invented an exact clock"));
    }
    Ok(())
}
fn check_overrides(
    old: &ProjectDocument,
    after: &ProjectDocument,
    wrap: Option<(&NodeId, &NodeId, &NodeId, bool)>,
) -> Result<(), DocumentError> {
    for (before, current) in [
        (old.overrides(), after.overrides()),
        (old.gap_overrides(), after.gap_overrides()),
    ] {
        if before.len() != current.len() {
            return Err(invalid("Retime changed Repeat overrides"));
        }
        for (repeat, entries) in before {
            let Some(now) = current.get(repeat) else {
                return Err(invalid("Retime removed Repeat overrides"));
            };
            if entries.len() != now.len() {
                return Err(invalid("Retime changed Repeat plays"));
            }
            for (iteration, root) in entries.iter() {
                let expected = match wrap {
                    Some((parent, child, stage, true)) if parent == repeat && root == child => {
                        stage
                    }
                    _ => root,
                };
                if now.get(iteration) != Some(expected) {
                    return Err(invalid("Retime changed an unrelated Repeat branch"));
                }
            }
        }
    }
    Ok(())
}
/// Unchanged document clones retain each complete immutable layout allocation.
/// Prove that identity before traversing its potentially large historical body.
/// Independently allocated bodies still require fully metered exact comparison.
fn same_timings(
    old: &BTreeMap<AudioTimingId, FrozenAudioLayout>,
    after: &BTreeMap<AudioTimingId, FrozenAudioLayout>,
    budget: &mut Budget,
) -> Result<bool, DocumentError> {
    budget.spend(1)?;
    if old.len() != after.len() {
        return Ok(false);
    }
    for ((old_id, old_layout), (new_id, new_layout)) in old.iter().zip(after) {
        budget.serialized(&(old_id, new_id))?;
        if old_id != new_id {
            return Ok(false);
        }
        budget.spend(1)?;
        // Deref exposes the whole immutable FrozenAudioLayoutData, not a node,
        // root, shape, or accelerator. Its address is retained by both Arcs.
        if std::ptr::eq(&**old_layout, &**new_layout) {
            continue;
        }
        budget.serialized(&(old_layout, new_layout))?;
        if old_layout != new_layout {
            return Ok(false);
        }
    }
    Ok(true)
}
fn parents<'a>(
    document: &'a ProjectDocument,
    budget: &mut Budget,
) -> Result<BTreeMap<&'a NodeId, &'a NodeId>, DocumentError> {
    let mut result = BTreeMap::new();
    for parent in document.nodes().keys() {
        for child in document.children(parent) {
            budget.spend(1)?;
            if result.insert(child, parent).is_some() {
                return Err(invalid("Retime has a shared child"));
            }
        }
    }
    Ok(result)
}
fn path<'a>(
    parents: &BTreeMap<&'a NodeId, &'a NodeId>,
    owner: &'a NodeId,
    budget: &mut Budget,
) -> Result<Vec<&'a NodeId>, DocumentError> {
    let mut path = Vec::new();
    let mut node = owner;
    while let Some(parent) = parents.get(node) {
        budget.spend(1)?;
        if path.len() == MAX_DOCUMENT_DEPTH {
            return Err(limit("Retime ancestry depth limit"));
        }
        path.push(*parent);
        node = parent;
    }
    Ok(path)
}
struct Budget {
    used: usize,
    maximum: usize,
}
impl Budget {
    fn new(maximum: usize) -> Result<Self, DocumentError> {
        if maximum == 0 || maximum > MAX_AUDIO_BINDING_ENTRIES {
            return Err(limit("invalid Retime proof budget"));
        }
        Ok(Self { used: 0, maximum })
    }
    fn spend(&mut self, amount: usize) -> Result<(), DocumentError> {
        self.used = self
            .used
            .checked_add(amount)
            .filter(|used| *used <= self.maximum)
            .ok_or_else(|| limit("Retime proof work limit"))?;
        Ok(())
    }
    fn serialized(&mut self, value: &impl Serialize) -> Result<(), DocumentError> {
        struct Writer<'a>(&'a mut Budget);
        impl Write for Writer<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.spend(bytes.len()).map_err(io::Error::other)?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer(Writer(self), value)
            .map_err(|_| limit("Retime proof comparison byte limit"))
    }
}
fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests;
