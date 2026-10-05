//! Immutable, history-neutral copies of selected ordinary Sequence contents.
//! Complete owners retain their recipe clocks; transparent windows select output.

mod children;
mod placement;
mod rename;
mod sound_clocks;
mod wire;

pub use children::SequenceChildrenPlan;
pub(crate) use placement::apply;

use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Caller-owned pools. The first authored node is the inserted neutral Sequence;
/// remaining nodes follow source ID order, then endpoint windows in output order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlicePasteIdentities {
    pub authored: OccurrenceIdentities,
    pub aliases: Vec<NodeId>,
}

/// Counts for a fresh paste. Destination aggregate limits are checked separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SliceIdentityRequirements {
    pub nodes: usize,
    pub marks: usize,
    pub aliases: usize,
    /// Imported records only. Each command first uses or reserves its documented
    /// destination timing slots, then these consecutive imported ordinals.
    pub timings: usize,
}

/// A temporal interval, one exact child, or an inclusive direct-child span of
/// an ordinary Sequence. Identities distinguish empty siblings at one boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SliceCaptureSelection {
    Range { range: FrameRange },
    Child { node: NodeId },
    Children { first: NodeId, last: NodeId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SlicePart {
    pub root: NodeId,
    pub mapping: FrameRange,
    pub source_start: ProjectFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SliceWire {
    project_id: ProjectId,
    revision_id: RevisionId,
    capture_timing: AudioTimingId,
    presentation_basis: PresentationBasis,
    parent: NodeId,
    range: FrameRange,
    // Missing only while reading the original range-only wire. Normalize before
    // validation; current Range output keeps that original serialized shape.
    #[serde(
        default,
        skip_serializing_if = "range_selection",
        deserialize_with = "read_selection"
    )]
    selection: Option<SliceCaptureSelection>,
    source_duration: FrameDuration,
    parts: Vec<SlicePart>,
    #[serde(deserialize_with = "crate::audio_gain::node_map")]
    nodes: BTreeMap<NodeId, BeatNode>,
    #[serde(default, deserialize_with = "crate::sound_events::beat_sounds_map")]
    beat_sounds: BTreeMap<NodeId, BTreeMap<SoundId, BeatSound>>,
    #[serde(deserialize_with = "crate::document::unique_map")]
    assets: BTreeMap<AssetId, AssetRecord>,
    #[serde(deserialize_with = "crate::document::unique_map")]
    marks: BTreeMap<MarkId, Mark>,
    #[serde(deserialize_with = "crate::document::unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(deserialize_with = "crate::document::unique_map")]
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(deserialize_with = "crate::document::unique_map")]
    audio_lineage: BTreeMap<NodeId, AudioLineageId>,
    audio_bindings: AudioBindingState,
    /// Targets the captured framing follows. Pasting adds a target only where
    /// the destination lacks it; an existing target, like an asset, is shared.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "crate::document::unique_map"
    )]
    targets: BTreeMap<crate::TargetId, crate::AttentionTarget>,
}

/// A closed editable forest. Its source revision is provenance, not a live link.
/// Copying contents excludes unselected ancestors and independent root sounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CapturedEditSlice(SliceWire);

fn range_selection(selection: &Option<SliceCaptureSelection>) -> bool {
    matches!(selection, None | Some(SliceCaptureSelection::Range { .. }))
}

fn read_selection<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<SliceCaptureSelection>, D::Error> {
    // Unlike Option's reader, an explicitly present null is not a legacy range.
    SliceCaptureSelection::deserialize(deserializer).map(Some)
}

impl<'de> Deserialize<'de> for CapturedEditSlice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let json = wire::read(deserializer)?;
        Self::from_json(&json).map_err(serde::de::Error::custom)
    }
}

/// Audio-only repeats (`:repeat N role=audio`) are root sound events of the
/// Original's own audio over a mute range of the beat. A copy captures the
/// mute but not root sounds, so its paste would be silent where the repeats
/// were: refuse a capture that would take a muted stretch while leaving
/// behind a sound of the Original heard over it. Placed catalog sounds
/// (audio-only assets) and sounds over unmuted beats are unaffected.
fn refuse_left_behind_repeats(
    document: &ProjectDocument,
    children: &[NodeId],
    durations: &BTreeMap<NodeId, FrameDuration>,
    parent_start: ProjectFrame,
    range: FrameRange,
) -> Result<(), EditError> {
    if document.sounds().is_empty() {
        return Ok(());
    }
    // Captured muted stretches, in absolute Edit frames.
    let mut muted = Vec::new();
    let mut start = parent_start.0;
    for child in children {
        let length = durations[child].frames();
        if start < range.end().0
            && start + length > range.start().0
            && let Some(clip) = document.nodes()[child].audio_treatments.clip_gain()
        {
            for local in clip.mute_ranges() {
                let low = local.start().checked_add(ExactRatio::integer(start));
                let high = local.end().checked_add(ExactRatio::integer(start));
                if let (Ok(low), Ok(high)) = (low, high) {
                    muted.push((low, high));
                }
            }
        }
        start += length;
    }
    if muted.is_empty() {
        return Ok(());
    }
    let rate = document.presentation_basis().frame_rate;
    let (first, last) = (
        ExactRatio::integer(range.start().0),
        ExactRatio::integer(range.end().0),
    );
    for event in document.sounds().values() {
        let from_original = document
            .assets()
            .get(&event.source.asset)
            .is_some_and(|asset| asset.video.is_some());
        if !from_original {
            continue;
        }
        let heard = event
            .mapping
            .selection_frames_with_offset(FrameDuration::ZERO, event.offset, rate)
            .map_err(DocumentError::from)?;
        let overlaps = |low: ExactRatio, high: ExactRatio| {
            heard.start.compare(high).is_lt() && heard.end.compare(low).is_gt()
        };
        if overlaps(first, last) && muted.iter().any(|(low, high)| overlaps(*low, *high)) {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!(
                    "this selection includes \"{}\", a sound repeated from the Original over a muted beat; a copy would keep the mute but leave the repeat behind. Undo or delete the repeat first, or select around it",
                    event.label
                ),
            ));
        }
    }
    Ok(())
}

impl CapturedEditSlice {
    pub(crate) fn check_destination(&self, document: &ProjectDocument) -> Result<(), EditError> {
        if self.project_id() != document.project_id() {
            return Err(EditError::new(
                EditErrorCode::ProjectConflict,
                "slice belongs to another project",
            ));
        }
        if self.presentation_basis() != document.presentation_basis() {
            return Err(invalid(
                "slice presentation basis differs from the destination",
            ));
        }
        Ok(())
    }
    /// Select a nonempty global Edit range under an ordinary Sequence. Partial
    /// endpoints use the existing Source/Hold/fragment admission; complete middle
    /// composites remain structural. `timing` is only a scratch capture name.
    pub fn capture(
        document: &ProjectDocument,
        parent: &NodeId,
        range: FrameRange,
        timing: AudioTimingId,
    ) -> Result<Self, EditError> {
        Self::capture_selection(
            document,
            parent,
            &SliceCaptureSelection::Range { range },
            timing,
        )
    }

    /// Capture a range, one whole direct child, or an inclusive sibling span,
    /// including empty Sequence trees. The parent and all its ancestors must
    /// be ordinary Sequences.
    pub fn capture_selection(
        document: &ProjectDocument,
        parent: &NodeId,
        selection: &SliceCaptureSelection,
        timing: AudioTimingId,
    ) -> Result<Self, EditError> {
        document.validate()?;
        if let SliceCaptureSelection::Range { range } = selection {
            crate::insert_time::sequence_range::preflight_capture(document, parent, *range)?;
        }
        let parent_start = document.source_splice_boundary(parent, 0)?;
        let durations = document.durations()?;
        let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
            unreachable!()
        };
        let child_span = match selection {
            SliceCaptureSelection::Children { first, last } => {
                Some(document.sequence_children(parent, first, last)?)
            }
            _ => None,
        };
        let range = match selection {
            SliceCaptureSelection::Range { range } => *range,
            SliceCaptureSelection::Children { .. } => {
                child_span.expect("resolved sibling span").range
            }
            SliceCaptureSelection::Child { node } => {
                let index = children
                    .iter()
                    .position(|child| child == node)
                    .ok_or_else(|| {
                        EditError::new(
                            EditErrorCode::SelectionUnavailable,
                            "capture target is not a direct child of its named Sequence",
                        )
                    })?;
                let start = document.source_splice_boundary(parent, index)?;
                FrameRange::new(
                    start,
                    ProjectFrame(
                        start
                            .0
                            .checked_add(durations[node].frames())
                            .ok_or_else(overflow)?,
                    ),
                )
                .map_err(DocumentError::from)?
            }
        };
        refuse_left_behind_repeats(document, children, &durations, parent_start, range)?;
        let mut offset = parent_start.0;
        let mut parts = Vec::new();
        let mut selected = BTreeSet::new();
        for (index, child) in children.iter().enumerate() {
            let end = offset
                .checked_add(durations[child].frames())
                .ok_or_else(overflow)?;
            let start = offset.max(range.start().0);
            let stop = end.min(range.end().0);
            let included = match selection {
                SliceCaptureSelection::Range { .. } => {
                    start < stop
                        || (offset == end && offset > range.start().0 && offset < range.end().0)
                }
                SliceCaptureSelection::Child { node } => child == node,
                SliceCaptureSelection::Children { .. } => {
                    let span = child_span.expect("resolved sibling span");
                    (span.first..span.end).contains(&index)
                }
            };
            if included {
                parts.push(SlicePart {
                    root: child.clone(),
                    mapping: FrameRange::new(
                        ProjectFrame(start - offset),
                        ProjectFrame(stop - offset),
                    )
                    .map_err(DocumentError::from)?,
                    source_start: ProjectFrame(offset),
                });
                selected.extend(crate::occurrence_edit::subtree_order(document, child)?);
            }
            offset = end;
        }
        let marks = crate::marks::capture_slice_mark_bindings(document, &parts, &selected)?;
        let state = if range.duration() == FrameDuration::ZERO {
            // A validated zero-duration owned tree can contain only Sequences.
            // Prove this rather than discarding a physical owner's clock.
            if selected.iter().any(|id| {
                !matches!(document.nodes()[id].kind, NodeKind::Sequence { .. })
                    || document.audio_bindings.bindings.contains_key(id)
                    || document.audio_bindings.gap_bindings.contains_key(id)
            }) {
                return Err(invalid(
                    "empty structural capture requires a Sequence-only forest without physical audio bindings",
                ));
            }
            AudioBindingState::default()
        } else {
            capture_audio(document, &parts, &selected, range, &timing)?
        };
        let nodes: BTreeMap<_, _> = selected
            .iter()
            .map(|id| (id.clone(), document.nodes()[id].clone()))
            .collect();
        let targets: BTreeMap<_, _> = nodes
            .values()
            .filter_map(|node| match &node.framing {
                Some(crate::Framing {
                    value: crate::FramingValue::Follow { target, .. },
                    ..
                }) => document
                    .targets()
                    .get(target)
                    .map(|found| (target.clone(), found.clone())),
                _ => None,
            })
            .collect();
        let mut assets = BTreeSet::new();
        for node in nodes.values() {
            node_assets(node, &mut assets);
        }
        assets.extend(targets.values().map(|target| target.asset.clone()));
        for owner in &selected {
            if let Some(events) = document.beat_sounds().get(owner) {
                assets.extend(events.values().map(|event| event.source.asset.clone()));
            }
        }
        for mark in marks.values() {
            for binding in mark.bindings() {
                if let Anchor::Source { asset, .. } = binding.coordinate {
                    // Dormant addresses may refer to an asset which no longer exists.
                    if document.assets().contains_key(&asset) {
                        assets.insert(asset);
                    }
                }
            }
        }
        let slice = Self(SliceWire {
            targets,
            project_id: document.project_id().clone(),
            revision_id: document.revision_id().clone(),
            capture_timing: timing,
            presentation_basis: document.presentation_basis().clone(),
            parent: parent.clone(),
            range,
            selection: Some(selection.clone()),
            source_duration: document.duration()?,
            parts,
            nodes,
            beat_sounds: document
                .beat_sounds()
                .iter()
                .filter(|(owner, _)| selected.contains(*owner))
                .map(|(owner, events)| (owner.clone(), events.clone()))
                .collect(),
            assets: assets
                .into_iter()
                .map(|id| (id.clone(), document.assets()[&id].clone()))
                .collect(),
            marks,
            overrides: document
                .overrides()
                .iter()
                .filter(|(id, _)| selected.contains(*id))
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect(),
            gap_overrides: document
                .gap_overrides()
                .iter()
                .filter(|(id, _)| selected.contains(*id))
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect(),
            audio_lineage: document
                .audio_lineage()
                .iter()
                .filter(|(id, _)| selected.contains(*id))
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect(),
            audio_bindings: state,
        });
        slice.validate()?;
        slice.to_json()?;
        Ok(slice)
    }

    /// Verify this payload against its immutable source revision. Structural
    /// validation alone does not establish that the contents were selected there.
    pub fn validate_capture(&self, source: &ProjectDocument) -> Result<(), EditError> {
        if self.project_id() != source.project_id() {
            return Err(EditError::new(
                EditErrorCode::ProjectConflict,
                "slice capture source belongs to another project",
            ));
        }
        if self.revision_id() != source.revision_id() {
            return Err(EditError::new(
                EditErrorCode::RevisionConflict,
                "slice capture source is a different revision",
            ));
        }
        let captured = Self::capture_selection(
            source,
            &self.0.parent,
            self.selection(),
            self.0.capture_timing.clone(),
        )?;
        if captured != *self {
            return Err(invalid("slice payload differs from its captured selection"));
        }
        Ok(())
    }

    pub fn project_id(&self) -> &ProjectId {
        &self.0.project_id
    }
    pub fn revision_id(&self) -> &RevisionId {
        &self.0.revision_id
    }
    /// Unique caller-supplied capture identity, separate from the source revision.
    pub fn capture_timing(&self) -> &AudioTimingId {
        &self.0.capture_timing
    }
    pub fn presentation_basis(&self) -> &PresentationBasis {
        &self.0.presentation_basis
    }
    pub fn parent(&self) -> &NodeId {
        &self.0.parent
    }
    pub fn range(&self) -> FrameRange {
        self.0.range
    }
    pub fn selection(&self) -> &SliceCaptureSelection {
        self.0
            .selection
            .as_ref()
            .expect("normalized slice selector")
    }
    /// A readable outline of the captured structure for inspection before
    /// reuse: each part's root with its kind, label and length, and the
    /// direct children of a captured group. Nothing is resolved or applied.
    pub fn outline(&self) -> Vec<String> {
        let describe = |id: &NodeId| -> String {
            let Some(node) = self.0.nodes.get(id) else {
                return format!("{id} (missing)");
            };
            let kind = match &node.kind {
                NodeKind::Source { source } => {
                    format!("Original moment, {} f", source.duration.frames())
                }
                NodeKind::Sequence { children } => format!("group of {} beats", children.len()),
                NodeKind::Hold { recipe } => format!("pause, {} f", recipe.duration.frames()),
                NodeKind::Repeat { iterations, .. } => format!("Repeat ×{}", iterations.len()),
                NodeKind::Retime {
                    purpose: crate::RetimePurpose::Partition,
                    duration,
                    ..
                } => format!("fragment, {} f", duration.frames()),
                NodeKind::Retime { duration, .. } => {
                    format!("speed change, {} f", duration.frames())
                }
            };
            let mut extras = Vec::new();
            if node.framing.is_some() {
                extras.push("framed");
            }
            if !node.audio_treatments.is_empty() {
                extras.push("gain/saturation");
            }
            if !node.cutaways.is_empty() {
                extras.push("cutaways");
            }
            if !node.captions.is_empty() {
                extras.push("captions");
            }
            let extras = if extras.is_empty() {
                String::new()
            } else {
                format!(" · {}", extras.join(", "))
            };
            format!("“{}” · {kind}{extras}", node.label)
        };
        let mut lines = Vec::new();
        for part in &self.0.parts {
            lines.push(format!(
                "{} · {} f in the copy",
                describe(&part.root),
                part.mapping.duration().frames()
            ));
            if let Some(NodeKind::Sequence { children }) =
                self.0.nodes.get(&part.root).map(|node| &node.kind)
            {
                lines.extend(
                    children
                        .iter()
                        .map(|child| format!("  {}", describe(child))),
                );
            }
        }
        lines
    }

    pub fn duration(&self) -> FrameDuration {
        self.0.range.duration()
    }

    pub fn identity_requirements(&self) -> Result<SliceIdentityRequirements, EditError> {
        let inventory = rename::Inventory::new(&self.0)?;
        let context = self.context()?;
        let durations = context.durations()?;
        let windows = self
            .0
            .parts
            .iter()
            .filter(|part| part.mapping.duration() != durations[&part.root])
            .count();
        let nodes = self
            .0
            .nodes
            .len()
            .checked_add(1)
            .and_then(|count| count.checked_add(windows))
            .filter(|count| *count <= MAX_DOCUMENT_NODES)
            .ok_or_else(|| limit("slice authored identity limit"))?;
        Ok(SliceIdentityRequirements {
            nodes,
            marks: self.0.marks.len(),
            aliases: inventory.alias_count(),
            timings: self.0.audio_bindings.timings.len(),
        })
    }

    pub fn from_json(json: &str) -> Result<Self, EditError> {
        if json.len() > MAX_DOCUMENT_JSON_BYTES {
            return Err(limit("slice JSON exceeds byte limit"));
        }
        let mut slice = Self(serde_json::from_str(json).map_err(DocumentError::json)?);
        if slice.0.selection.is_none() {
            slice.0.selection = Some(SliceCaptureSelection::Range {
                range: slice.0.range,
            });
        }
        slice.validate()?;
        Ok(slice)
    }

    pub fn to_json(&self) -> Result<String, EditError> {
        let mut bytes = SliceJson(Vec::new(), false);
        serde_json::to_writer(&mut bytes, self).map_err(|error| {
            if bytes.1 {
                limit("slice JSON exceeds byte limit")
            } else {
                DocumentError::json(error).into()
            }
        })?;
        String::from_utf8(bytes.0).map_err(|_| invalid("slice JSON is not UTF-8"))
    }

    fn context(&self) -> Result<ProjectDocument, EditError> {
        Self::context_for(&self.0)
    }

    fn context_for(value: &SliceWire) -> Result<ProjectDocument, EditError> {
        if value.nodes.len() >= MAX_DOCUMENT_NODES
            || value.parts.is_empty()
            || value.parts.len() > value.nodes.len()
            || value.nodes.contains_key(&value.parent)
        {
            return Err(limit("slice forest size or root is invalid"));
        }
        let mut document = ProjectDocument::new(
            value.project_id.clone(),
            value.revision_id.clone(),
            value.presentation_basis.clone(),
            value.parent.clone(),
        )?;
        document.nodes.insert(
            value.parent.clone(),
            BeatNode::sequence(
                "Captured contents",
                value.parts.iter().map(|part| part.root.clone()).collect(),
            ),
        );
        document.nodes.extend(value.nodes.clone());
        document.beat_sounds = value.beat_sounds.clone();
        document.assets = value.assets.clone();
        document.overrides = value.overrides.clone();
        document.gap_overrides = value.gap_overrides.clone();
        document.audio_lineage = value.audio_lineage.clone();
        document.audio_bindings = value.audio_bindings.clone();
        document.targets = value.targets.clone();
        document.validate()?;
        Ok(document)
    }

    fn validate(&self) -> Result<(), EditError> {
        let value = &self.0;
        if value.range.start().0 < 0 || value.range.end().0 > value.source_duration.frames() {
            return Err(invalid("slice bounds are outside its captured project"));
        }
        let context = self.context()?;
        let durations = context.durations()?;
        match self.selection() {
            SliceCaptureSelection::Range { range } => {
                if *range != value.range || range.duration() == FrameDuration::ZERO {
                    return Err(invalid("slice requires a nonempty matching captured range"));
                }
            }
            SliceCaptureSelection::Child { node } => {
                if value.parts.len() != 1
                    || &value.parts[0].root != node
                    || value.parts[0].mapping.start() != ProjectFrame(0)
                    || value.parts[0].mapping.duration() != durations[node]
                {
                    return Err(invalid(
                        "child slice requires exactly its whole selected subtree",
                    ));
                }
                if self.duration() == FrameDuration::ZERO
                    && (value
                        .nodes
                        .values()
                        .any(|node| !matches!(node.kind, NodeKind::Sequence { .. }))
                        || value.audio_bindings != AudioBindingState::default())
                {
                    return Err(invalid(
                        "empty child slice requires a Sequence-only tree without audio bindings",
                    ));
                }
            }
            SliceCaptureSelection::Children { first, last } => {
                children::validate(value, &durations, first, last)?;
            }
        }
        let mut boundary = value.range.start().0;
        for part in &value.parts {
            let duration = durations[&part.root];
            let start = part
                .source_start
                .0
                .checked_add(part.mapping.start().0)
                .ok_or_else(overflow)?;
            let end = part
                .source_start
                .0
                .checked_add(part.mapping.end().0)
                .ok_or_else(overflow)?;
            if part.source_start.0 < 0
                || part.mapping.start().0 < 0
                || part.mapping.end().0 > duration.frames()
                || start != boundary
            {
                return Err(invalid(
                    "slice output windows are not a closed contiguous selection",
                ));
            }
            if part.mapping.duration() != duration {
                if part.mapping.duration() == FrameDuration::ZERO {
                    return Err(invalid("empty partial slice window"));
                }
                crate::insert_time::slice_physical(&context, &part.root)?;
            }
            boundary = end;
        }
        if boundary != value.range.end().0 {
            return Err(invalid("slice windows do not cover its range"));
        }
        crate::marks::validate_slice_marks(&context, &value.marks, value.source_duration)?;
        self.identity_requirements()?;
        Ok(())
    }
}

impl ProjectDocument {
    /// Preflight exact sibling replacement without collapsing equal-time owners
    /// into a picture range. The new imported forest is the only peak growth.
    pub fn slice_children_replacement(
        &self,
        parent: &NodeId,
        first: &NodeId,
        last: &NodeId,
        slice: &CapturedEditSlice,
    ) -> Result<SequenceChildrenPlan, EditError> {
        slice.check_destination(self)?;
        let selected = self.sequence_children(parent, first, last)?;
        if self
            .nodes()
            .len()
            .checked_add(slice.identity_requirements()?.nodes)
            .is_none_or(|count| count > MAX_DOCUMENT_NODES)
        {
            return Err(limit("slice replacement exceeds the temporary node limit"));
        }
        Ok(selected)
    }
}

fn capture_audio(
    document: &ProjectDocument,
    parts: &[SlicePart],
    selected: &BTreeSet<NodeId>,
    range: FrameRange,
    timing: &AudioTimingId,
) -> Result<AudioBindingState, EditError> {
    // Check before detaching voices: pruning can remove a clock-only record,
    // but its identity must not be reused for the capture-time layout.
    if document.audio_bindings.timings.contains_key(timing) {
        return Err(invalid("slice capture timing identity already exists"));
    }
    // The independent root bus is outside this ownership selection. Only a
    // private structural view is detached; the source document is unchanged.
    let mut structural = document.clone();
    structural.sounds.clear();
    structural.sound_routes.clear();
    structural.sound_allowances.clear();
    // Independent voices are captured separately from physical source bindings.
    structural.audio_bindings.sound_clocks.clear();
    crate::audio_binding_lifecycle::prune(&mut structural);
    let captured = crate::audio_binding_lifecycle::capture_for_composite_insertion(
        &structural,
        timing.clone(),
    )?;
    let mut state = captured.state;
    if let Some(layout) = captured.phase_only_layout {
        state.timings.insert(timing.clone(), layout);
    }
    let mut affected = BTreeSet::new();
    let mut pending: Vec<_> = parts.iter().map(|part| &part.root).collect();
    while let Some(id) = pending.pop() {
        affected.insert(id.clone());
        if !matches!(&document.nodes()[id].kind,
                NodeKind::Retime { duration, mapping, pitch, .. }
                if pitch.processes(*duration == mapping.duration()))
        {
            pending.extend(document.children(id));
        }
    }
    let window = ExactFrameRange::new(
        ExactRatio::integer(range.start().0),
        ExactRatio::integer(range.end().0),
    )?;
    let mut entries = state
        .owners()
        .flat_map(|(_, _, binding)| binding.placements())
        .try_fold(0usize, |count, placement| {
            count
                .checked_add(placement.entry_count())
                .ok_or_else(|| limit("slice audio entries overflow"))
        })?;
    crate::insert_time::composite::append_steps(
        &mut state,
        crate::AudioRecipeKind::Node,
        captured.node_placements,
        &affected,
        window,
        &mut entries,
    )?;
    crate::insert_time::composite::append_steps(
        &mut state,
        crate::AudioRecipeKind::RepeatGap,
        captured.gap_placements,
        &affected,
        window,
        &mut entries,
    )?;
    state.bindings.retain(|id, _| selected.contains(id));
    state.gap_bindings.retain(|id, _| selected.contains(id));
    sound_clocks::capture(document, parts, selected, timing, &mut state)?;
    let retained: BTreeSet<_> = state
        .owners()
        .flat_map(|(_, _, binding)| binding.placements())
        .map(|placement| placement.reference.timing.clone())
        .chain(state.sound_clocks.values().flat_map(|events| {
            events
                .values()
                .flat_map(|journal| journal.clocks().iter().map(|clock| clock.timing().clone()))
        }))
        .collect();
    state.timings.retain(|id, _| retained.contains(id));
    Ok(state)
}

struct SliceJson(Vec<u8>, bool);
impl std::io::Write for SliceJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length > MAX_DOCUMENT_JSON_BYTES)
        {
            self.1 = true;
            return Err(std::io::Error::other("slice JSON byte limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn node_assets(node: &BeatNode, output: &mut BTreeSet<AssetId>) {
    output.extend(node.cutaways.iter().map(|cutaway| cutaway.asset.clone()));
    match &node.kind {
        NodeKind::Source { source } => {
            match &source.video {
                SourceVideo::Stream { asset, .. } | SourceVideo::Still { asset } => {
                    output.insert(asset.clone());
                }
                SourceVideo::Blank => {}
            }
            if let Some(audio) = &source.audio {
                output.insert(audio.asset.clone());
            }
        }
        NodeKind::Hold { recipe }
        | NodeKind::Repeat {
            gap: Some(recipe), ..
        } => hold_assets(recipe, output),
        _ => {}
    }
}
fn hold_assets(recipe: &HoldRecipe, output: &mut BTreeSet<AssetId>) {
    match &recipe.video {
        HoldVideo::Freeze { asset, .. }
        | HoldVideo::Accepted { asset, .. }
        | HoldVideo::Reverse { asset, .. }
        | HoldVideo::Play { asset, .. } => {
            output.insert(asset.clone());
        }
        HoldVideo::Generated { accepted } => {
            output.insert(accepted.artifact.sampled_asset.clone());
            output.insert(accepted.artifact.native_asset.clone());
            if let HoldFallback::Freeze { asset, .. } = &accepted.fallback {
                output.insert(asset.clone());
            }
        }
        HoldVideo::Background => {}
    }
    match &recipe.audio {
        HoldAudio::RoomTone { source } | HoldAudio::Reverse { source } => {
            output.insert(source.asset.clone());
        }
        HoldAudio::Silence | HoldAudio::Tone { .. } | HoldAudio::Tail { .. } => {}
    }
}
fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}
fn overflow() -> EditError {
    EditError::new(EditErrorCode::TimingOverflow, "slice time overflow")
}
