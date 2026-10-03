//! Persisted chronological edits of the independent root sound bus.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    AudioEdgePolicy, Command, DocumentError, DocumentErrorCode, EditError, EditErrorCode,
    ExactFrameRange, ExactRatio, FrameDuration, FrameRange, FrameRate, MIX_SAMPLE_RATE, NodeKind,
    ProjectDocument, ProjectFrame, SoundEvent, SoundId, SoundRippleMap, SoundRoute, SoundRouteNode,
    TimeError,
};

pub const MAX_ROOT_SOUND_EDITS: usize = 1024;
pub const MAX_DOCUMENT_SOUND_ROUTE_BYTES: usize = 1024 * 1024;

mod projection;
pub use projection::{RootSoundKeep, RootSoundProjection};

// A monotone edit can split existing support at at most two retained cuts.
const MAX_ROOT_SOUND_SUPPORT_INTERVALS: usize = 1 + 2 * MAX_ROOT_SOUND_EDITS;

/// A root clock always allocates at 48 kHz with ties-to-even boundaries.
/// Keep each historical origin and rate, never reconstruct a final time offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootSoundGrid {
    pub frame_origin: ExactRatio,
    pub frame_rate: FrameRate,
}

impl RootSoundGrid {
    pub fn root(frame_rate: FrameRate) -> Self {
        Self {
            frame_origin: ExactRatio::ZERO,
            frame_rate,
        }
    }

    pub fn frames_per_sample(self) -> Result<ExactRatio, TimeError> {
        ExactRatio::new(
            i128::from(self.frame_rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(self.frame_rate.denominator()),
        )
    }
}

/// Policies at genuine new cuts, not at transparent query or Split boundaries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootSoundCutEdges {
    pub before: AudioEdgePolicy,
    pub after: AudioEdgePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RootSoundOperation {
    Insert {
        at: ProjectFrame,
        duration: FrameDuration,
    },
    Delete {
        range: FrameRange,
    },
    /// One direct old-to-final map, with no rounded intermediate deletion.
    Replace {
        range: FrameRange,
        duration: FrameDuration,
    },
    /// One entry-anchored combined Ripple Trim map. Roll and Slip do not move
    /// the independent root bus and are deliberately absent from this operation.
    Trim {
        range: FrameRange,
        in_frames: i64,
        out_frames: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootSoundEdit {
    pub grid: RootSoundGrid,
    pub operation: RootSoundOperation,
    pub cuts: RootSoundCutEdges,
}

/// The event retains the complete source recipe and exact selection. This
/// journal retains its original owner extent and all later physical cuts.
/// Original envelope progress follows these clocks independently of live Holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootSoundRoute {
    pub recipe_extent: FrameDuration,
    pub recipe_grid: RootSoundGrid,
    #[serde(deserialize_with = "bounded_edits")]
    pub edits: Vec<RootSoundEdit>,
}

impl RootSoundRoute {
    pub fn identity(recipe_extent: FrameDuration, rate: FrameRate) -> Self {
        Self {
            recipe_extent,
            recipe_grid: RootSoundGrid::root(rate),
            edits: Vec::new(),
        }
    }

    /// Build the checked arena once. No adjacent edits are coalesced: their
    /// distinct physical cut labels can differ even at equal final frame maps.
    pub fn compile(&self) -> Result<SoundRoute, DocumentError> {
        if self.recipe_extent == FrameDuration::ZERO || self.edits.len() > MAX_ROOT_SOUND_EDITS {
            return Err(invalid(
                "invalid root sound recipe extent or history length",
            ));
        }
        if self.recipe_grid.frame_origin != ExactRatio::ZERO {
            return Err(invalid("root sound clocks require the project origin"));
        }
        let mut nodes = vec![SoundRouteNode::Recipe {}];
        let mut extent = self.recipe_extent.frames();
        self.recipe_grid
            .frame_rate
            .audio_boundary(ProjectFrame(extent))?;
        for edit in &self.edits {
            if edit.grid != self.recipe_grid {
                return Err(invalid(
                    "root sound edits cannot change origin or frame rate",
                ));
            }
            let map = edit.operation.map(extent)?;
            extent = edit.operation.output_frames(extent)?;
            edit.grid.frame_rate.audio_boundary(ProjectFrame(extent))?;
            let input = u32::try_from(nodes.len() - 1).map_err(|_| TimeError::Overflow)?;
            nodes.push(SoundRouteNode::Ripple { input, map });
        }
        let root = u32::try_from(nodes.len() - 1).map_err(|_| TimeError::Overflow)?;
        let route = SoundRoute::new(
            ExactRatio::integer(self.recipe_extent.frames()),
            root,
            nodes,
        )?;
        if serde_json::to_vec(self).map_err(DocumentError::json)?.len()
            > MAX_DOCUMENT_SOUND_ROUTE_BYTES
        {
            return Err(limit(
                "root sound routing exceeds its serialized byte limit",
            ));
        }
        Ok(route)
    }
}

impl RootSoundOperation {
    /// One validated projection for sampled routing, retained support and edges.
    pub fn projection(self, input_frames: i64) -> Result<RootSoundProjection, DocumentError> {
        RootSoundProjection::new(self, input_frames)
    }

    pub fn output_frames(self, input_frames: i64) -> Result<i64, DocumentError> {
        Ok(self.projection(input_frames)?.output_duration().frames())
    }

    fn map(self, extent: i64) -> Result<SoundRippleMap, DocumentError> {
        self.projection(extent)?.ripple_map()
    }
}

pub(crate) fn validate(
    document: &ProjectDocument,
    duration: FrameDuration,
) -> Result<(), DocumentError> {
    let mut bytes = 0usize;
    for (id, journal) in &document.sound_routes {
        let event = document
            .sounds
            .get(id)
            .ok_or_else(|| invalid("sound route has no event"))?;
        if journal.recipe_grid.frame_rate != document.presentation_basis.frame_rate {
            return Err(invalid(
                "sound route rate differs from the locked project rate",
            ));
        }
        let route = journal.compile()?;
        if route.output_extent() != ExactRatio::integer(duration.frames()) {
            return Err(invalid(
                "sound route output differs from its current owner extent",
            ));
        }
        if !retains_selection(event, journal)? {
            return Err(invalid("sound route has no surviving selected support"));
        }
        bytes = bytes
            .checked_add(
                serde_json::to_vec(journal)
                    .map_err(DocumentError::json)?
                    .len(),
            )
            .filter(|bytes| *bytes <= MAX_DOCUMENT_SOUND_ROUTE_BYTES)
            .ok_or_else(|| limit("document sound routing exceeds its serialized byte limit"))?;
    }
    Ok(())
}

fn retains_selection(event: &SoundEvent, journal: &RootSoundRoute) -> Result<bool, DocumentError> {
    let selection = event.mapping.selection_frames_with_offset(
        journal.recipe_extent,
        event.offset,
        journal.recipe_grid.frame_rate,
    )?;
    let first = sound_sample_boundary(journal.recipe_grid, selection.start)?;
    let last = sound_sample_boundary(journal.recipe_grid, selection.end)?;
    if first == last {
        // Exact nonempty selections without an allocated sample are already
        // valid authored intent. Preserve that intent through unrelated edits,
        // using logical support only for this initially sampleless case.
        return retains_logical_selection(selection, journal);
    }
    // Keep actual old integral samples, just like the sampled route. Re-rounding
    // moved exact endpoints loses accumulated phase and can delete a surviving
    // sample. One edit adds at most two interval cuts; no audio query is needed.
    let mut support: Vec<_> = std::iter::once(first..last).collect();
    let mut extent = journal.recipe_extent.frames();
    let mut old_grid = journal.recipe_grid;
    for edit in &journal.edits {
        let projection = edit.operation.projection(extent)?;
        let new_extent = projection.output_duration().frames();
        let mut next = Vec::with_capacity(support.len() + 2);
        for keep in projection.keeps() {
            let old = keep.input.start().0..keep.input.end().0;
            let destination = keep.output.start().0..keep.output.end().0;
            let old_start = sound_sample_boundary(old_grid, ExactRatio::integer(old.start))?;
            let old_end = sound_sample_boundary(old_grid, ExactRatio::integer(old.end))?;
            let new_start =
                sound_sample_boundary(edit.grid, ExactRatio::integer(destination.start))?;
            let new_end = sound_sample_boundary(edit.grid, ExactRatio::integer(destination.end))?;
            let shift = i128::from(new_start) - i128::from(old_start);
            for interval in &support {
                let first = interval.start.max(old_start);
                let last = interval.end.min(old_end);
                if first >= last {
                    continue;
                }
                // A rounded old endpoint can translate one sample beyond the
                // admitted destination. Intersect before narrowing the label.
                let first = (i128::from(first) + shift).max(i128::from(new_start));
                let last = (i128::from(last) + shift).min(i128::from(new_end));
                if first < last {
                    push_support(
                        &mut next,
                        i64::try_from(first).map_err(|_| TimeError::Overflow)?
                            ..i64::try_from(last).map_err(|_| TimeError::Overflow)?,
                    )?;
                }
            }
        }
        support = next;
        if support.is_empty() {
            return Ok(false);
        }
        extent = new_extent;
        old_grid = edit.grid;
    }
    Ok(true)
}

fn sound_sample_boundary(grid: RootSoundGrid, frame: ExactRatio) -> Result<i64, TimeError> {
    let sample = frame
        .checked_sub(grid.frame_origin)?
        .checked_div(grid.frames_per_sample()?)?
        .round_even()?;
    i64::try_from(sample).map_err(|_| TimeError::Overflow)
}

fn retains_logical_selection(
    selection: ExactFrameRange,
    journal: &RootSoundRoute,
) -> Result<bool, DocumentError> {
    let mut support = vec![selection];
    let mut extent = journal.recipe_extent.frames();
    for edit in &journal.edits {
        let projection = edit.operation.projection(extent)?;
        let mut next = Vec::with_capacity(support.len() + 2);
        for keep in projection.keeps() {
            let start = ExactRatio::integer(keep.input.start().0);
            let end = ExactRatio::integer(keep.input.end().0);
            let shift = ExactRatio::integer(keep.output.start().0).checked_sub(start)?;
            for interval in &support {
                let first = if interval.start.compare(start).is_gt() {
                    interval.start
                } else {
                    start
                };
                let last = if interval.end.compare(end).is_lt() {
                    interval.end
                } else {
                    end
                };
                if first.compare(last).is_lt() {
                    push_support(
                        &mut next,
                        ExactFrameRange {
                            start: first.checked_add(shift)?,
                            end: last.checked_add(shift)?,
                        },
                    )?;
                }
            }
        }
        support = next;
        if support.is_empty() {
            return Ok(false);
        }
        extent = projection.output_duration().frames();
    }
    Ok(true)
}

fn push_support<T>(support: &mut Vec<T>, value: T) -> Result<(), DocumentError> {
    if support.len() >= MAX_ROOT_SOUND_SUPPORT_INTERVALS {
        return Err(limit(
            "root sound retained support exceeds its interval limit",
        ));
    }
    support.push(value);
    Ok(())
}

/// Own the bus outside the temporary structural document. Public context
/// captures still reject sound-bearing documents; only this complete command
/// path can detach, transform and reinstall the root bus atomically.
pub(crate) struct RootSoundEditCapture {
    sounds: BTreeMap<SoundId, SoundEvent>,
    routes: BTreeMap<SoundId, RootSoundRoute>,
    output_frames: i64,
}

impl RootSoundEditCapture {
    pub(crate) fn prepare(
        document: &ProjectDocument,
        command: &Command,
    ) -> Result<Option<Self>, EditError> {
        if document.sounds.is_empty() {
            return Ok(None);
        }
        if matches!(
            command,
            Command::RepeatSelection { .. } | Command::SetRepeatPlays { .. }
        ) {
            return match crate::repeat_selection::root_operation(document, command)? {
                Some(operation) => Self::prepare_operation(document, operation).map(Some),
                None => Ok(Some(Self {
                    sounds: document.sounds.clone(),
                    routes: document.sound_routes.clone(),
                    output_frames: document.duration()?.frames(),
                })),
            };
        }
        if let Command::ApplySourceTrim {
            parent,
            node,
            right,
            intent,
            ..
        } = command
        {
            let resolved = document.source_trim_edit(parent, node, right.as_ref(), *intent)?;
            return match resolved.root_operation {
                Some(operation) => Self::prepare_operation(document, operation).map(Some),
                None => Ok(Some(Self {
                    sounds: document.sounds.clone(),
                    routes: document.sound_routes.clone(),
                    output_frames: document.duration()?.frames(),
                })),
            };
        }
        let fixed_duration = match command {
            Command::GroupSelection {
                parent, selection, ..
            } => {
                document.group_selection(parent, selection)?;
                true
            }
            Command::MoveRange {
                source_parent,
                range,
                destination,
                ..
            } => {
                document.range_move(source_parent, *range, destination)?;
                true
            }
            Command::RollSources {
                parent,
                left,
                right,
                delta_frames,
                ..
            } => {
                document.source_roll(parent, left, right, *delta_frames)?;
                true
            }
            _ => false,
        };
        if fixed_duration {
            // Fixed-duration edits leave the root owner's clock unchanged.
            // Detach only for soundless structural captures, then restore exact
            // recipes/routes; current Hold gates follow their final issuers.
            return Ok(Some(Self {
                sounds: document.sounds.clone(),
                routes: document.sound_routes.clone(),
                output_frames: document.duration()?.frames(),
            }));
        }
        let operation = match command {
            Command::TrimSource {
                parent,
                node,
                edge,
                delta_frames,
                mode,
                ..
            } => {
                let resolved = document.source_trim(parent, node, *edge, *delta_frames, *mode)?;
                let Some(operation) = resolved.root_operation else {
                    return Ok(None);
                };
                operation
            }
            Command::InsertTime { at, hold, .. } => RootSoundOperation::Insert {
                at: *at,
                duration: hold.duration,
            },
            Command::SpliceSource {
                parent,
                index,
                source,
                ..
            } => RootSoundOperation::Insert {
                at: document.source_splice_boundary(parent, *index)?,
                duration: source.duration,
            },
            Command::SpliceSlice {
                parent,
                index,
                slice,
                ..
            } => {
                let at = document.source_splice_boundary(parent, *index)?;
                if slice.duration() == FrameDuration::ZERO {
                    // No root time changes when an empty structural child is
                    // added. Keep recipes and routing journals untouched.
                    return Ok(None);
                }
                RootSoundOperation::Insert {
                    at,
                    duration: slice.duration(),
                }
            }
            Command::SpliceSliceAt {
                parent,
                target,
                at,
                slice,
                ..
            } => RootSoundOperation::Insert {
                at: document
                    .slice_splice_interior(parent, target, *at, slice)?
                    .boundary,
                duration: slice.duration(),
            },
            Command::ReplaceSlice {
                parent,
                range,
                slice,
                ..
            } => {
                document.slice_replacement(parent, *range, slice)?;
                RootSoundOperation::Replace {
                    range: *range,
                    duration: slice.duration(),
                }
            }
            Command::SpliceSourceAt {
                parent,
                target,
                at,
                source,
                ..
            } => RootSoundOperation::Insert {
                at: document
                    .source_splice_interior(parent, target, *at)?
                    .boundary,
                duration: source.duration,
            },
            Command::Delete { node } | Command::DeleteRipple { node, .. } => {
                let parent = document.parent_of(node).ok_or_else(|| {
                    edit_invalid("sound ripple deletion requires a non-root Sequence child")
                })?;
                let NodeKind::Sequence { children } = &document.nodes[&parent].kind else {
                    return Err(edit_invalid(
                        "sound ripple deletion requires a Sequence child",
                    ));
                };
                let slot = children
                    .iter()
                    .position(|child| child == node)
                    .ok_or_else(|| edit_invalid("sound deletion target is not a child"))?;
                let at = document.source_splice_boundary(&parent, slot)?;
                let length = document.durations()?[node];
                if length == FrameDuration::ZERO {
                    return Ok(None);
                }
                RootSoundOperation::Delete {
                    range: FrameRange::new(
                        at,
                        ProjectFrame(
                            at.0.checked_add(length.frames())
                                .ok_or(TimeError::Overflow)
                                .map_err(DocumentError::from)?,
                        ),
                    )
                    .map_err(DocumentError::from)?,
                }
            }
            Command::DeleteRange { parent, range, .. } => {
                document.range_deletion(parent, *range)?;
                RootSoundOperation::Delete { range: *range }
            }
            Command::DeleteChildren {
                parent,
                first,
                last,
                ..
            } => {
                let selected = document.sequence_children(parent, first, last)?;
                if selected.range.duration() == FrameDuration::ZERO {
                    return Ok(None);
                }
                RootSoundOperation::Delete {
                    range: selected.range,
                }
            }
            Command::ReplaceSource {
                parent,
                range,
                source,
                ..
            } => {
                document.source_replacement(parent, *range)?;
                RootSoundOperation::Replace {
                    range: *range,
                    duration: source.duration,
                }
            }
            _ => return Ok(None),
        };
        Self::prepare_operation(document, operation).map(Some)
    }

    /// Prepare one complete operation against the original root clock. The
    /// command path and future complete-intent edits share this boundary.
    pub(crate) fn prepare_operation(
        document: &ProjectDocument,
        operation: RootSoundOperation,
    ) -> Result<Self, EditError> {
        let extent = document.duration()?;
        let projection = operation.projection(extent.frames())?;
        let output_frames = projection.output_duration().frames();
        if projection.is_identity() {
            return Ok(Self {
                sounds: document.sounds.clone(),
                routes: document.sound_routes.clone(),
                output_frames,
            });
        }
        let mut sounds = document.sounds.clone();
        let mut routes = document.sound_routes.clone();
        if output_frames == 0 {
            sounds.clear();
            routes.clear();
        } else {
            for (id, event) in &document.sounds {
                let journal = routes.entry(id.clone()).or_insert_with(|| {
                    RootSoundRoute::identity(extent, document.presentation_basis.frame_rate)
                });
                if journal.edits.len() >= MAX_ROOT_SOUND_EDITS {
                    // Preserve the existing compile-time refusal before growth.
                    return Err(
                        invalid("invalid root sound recipe extent or history length").into(),
                    );
                }
                journal.edits.push(RootSoundEdit {
                    grid: journal.recipe_grid,
                    operation,
                    cuts: RootSoundCutEdges::default(),
                });
                journal.compile()?;
                if !retains_selection(event, journal)? {
                    sounds.remove(id);
                }
            }
            routes.retain(|id, _| sounds.contains_key(id));
        }
        Ok(Self {
            sounds,
            routes,
            output_frames,
        })
    }

    pub(crate) fn structural_document(&self, document: &ProjectDocument) -> ProjectDocument {
        let mut working = document.clone();
        working.sounds.clear();
        working.sound_routes.clear();
        working.sound_allowances.clear();
        working
    }

    pub(crate) fn restore(self, document: &mut ProjectDocument) -> Result<(), EditError> {
        // Deleted physical bindings are pruned by the outer transaction after
        // bus restoration. Only inspect structural extent at this boundary.
        if document.structural_durations()?[&document.root].frames() != self.output_frames {
            return Err(edit_invalid(
                "structural edit disagrees with the captured sound ripple",
            ));
        }
        document.sounds = self.sounds;
        document.sound_routes = self.routes;
        Ok(())
    }
}

fn bounded_edits<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RootSoundEdit>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<RootSoundEdit>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bounded root sound edit history")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut edits = Vec::new();
            while let Some(edit) = seq.next_element()? {
                if edits.len() == MAX_ROOT_SOUND_EDITS {
                    return Err(serde::de::Error::custom("root sound edit history limit"));
                }
                edits.push(edit);
            }
            Ok(edits)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}
fn edit_invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

#[cfg(test)]
mod tests;
