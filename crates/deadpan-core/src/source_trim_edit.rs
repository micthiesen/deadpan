//! One accepted complete Trim intent, resolved against one immutable entry.
use crate::{
    AudioReanchorAnchor, AudioSourceEndpoint, AudioTimingId, DocumentError, EditError,
    EditErrorCode, ExactFrameRange, ExactRatio, FrameDuration, FrameRange, MAX_DOCUMENT_NODES,
    NodeId, NodeKind, ProjectDocument, ProjectFrame, RootSoundOperation, SourceEditWindow,
    SourceTrimGeometry, SourceTrimIntent, SourceTrimOwnerGeometry, SourceTrimPhaseAnchor,
    SourceTrimPolicy, SplitIdentities, TimeError,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod apply;
pub(crate) use apply::apply;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTrimResources {
    pub target_wrapper: Option<NodeId>,
    pub right_wrapper: Option<NodeId>,
    #[serde(deserialize_with = "bounded_split")]
    pub split: SplitIdentities,
    #[serde(deserialize_with = "bounded_fillers")]
    pub fillers: Vec<NodeId>,
    pub timing: Option<AudioTimingId>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimCapture {
    None,
    Unbound,
    Placements,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimReanchorGroup {
    pub owners: BTreeSet<NodeId>,
    pub anchor: AudioReanchorAnchor,
    pub window: Option<ExactFrameRange>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimSplit {
    pub node: NodeId,
    pub boundary: ProjectFrame,
    pub required_nodes: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimEmptyMove {
    pub node: NodeId,
    pub before: ProjectFrame,
    pub after: ProjectFrame,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimRightDisposition {
    Unchanged,
    Retained,
    Removed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimResultIdentities {
    pub target: NodeId,
    pub target_output: FrameRange,
    pub right: Option<NodeId>,
    pub right_disposition: SourceTrimRightDisposition,
    pub fillers: Vec<NodeId>,
}
/// A retained whole Source candidate behind its final allocation. Overwrite
/// can retain endpoint padding after its visible selected material is consumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimFinalOwner {
    pub geometry: SourceTrimOwnerGeometry,
    pub allocation: FrameRange,
    pub output: FrameRange,
    pub needs_wrapper: bool,
    pub visible_selection: Option<SourceEditWindow>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimEditResolution {
    pub geometry: SourceTrimGeometry,
    /// Final visible B. None also represents a fully consumed eligible B;
    /// geometry.right retains its exact pre-overlay candidate in that case.
    pub right_after: Option<SourceTrimFinalOwner>,
    pub footprint: Option<FrameRange>,
    /// Existing whole children removed by the overlay. Removed copied fragments
    /// are available from the admitted transaction diff, after IDs are supplied.
    pub removed_children: Vec<NodeId>,
    pub fillers: Vec<FrameRange>,
    pub splits: Vec<SourceTrimSplit>,
    pub empty_moves: Vec<SourceTrimEmptyMove>,
    pub required_target_wrapper: bool,
    pub required_right_wrapper: bool,
    pub required_split_nodes: usize,
    pub required_filler_nodes: usize,
    pub temporary_nodes: usize,
    pub capture: SourceTrimCapture,
    pub reanchors: Vec<SourceTrimReanchorGroup>,
    pub root_operation: Option<RootSoundOperation>,
}
impl SourceTrimEditResolution {
    /// Resolve result identity roles after the command pool has been admitted.
    /// This diagnostic projection is not permission to skip command validation.
    pub fn result_identities(
        &self,
        resources: &SourceTrimResources,
    ) -> Result<SourceTrimResultIdentities, EditError> {
        if resources.target_wrapper.is_some() != self.required_target_wrapper
            || resources.right_wrapper.is_some() != self.required_right_wrapper
            || resources.fillers.len() != self.required_filler_nodes
            || resources.split.nodes.len() != self.required_split_nodes
            || resources.timing.is_some() != (self.capture != SourceTrimCapture::None)
        {
            return Err(invalid(
                "combined Trim result identities do not match the resource roles",
            ));
        }
        let mut seen = BTreeSet::new();
        if resources
            .target_wrapper
            .iter()
            .chain(resources.right_wrapper.iter())
            .chain(&resources.fillers)
            .chain(&resources.split.nodes)
            .any(|id| !seen.insert(id))
        {
            return Err(invalid("combined Trim result identities collide"));
        }
        let right_disposition = if self.right_after.is_some() {
            SourceTrimRightDisposition::Retained
        } else if self.footprint.is_some() && self.geometry.right.is_some() {
            SourceTrimRightDisposition::Removed
        } else {
            SourceTrimRightDisposition::Unchanged
        };
        Ok(SourceTrimResultIdentities {
            target: resources
                .target_wrapper
                .as_ref()
                .unwrap_or(&self.geometry.target.target)
                .clone(),
            target_output: self.geometry.target.output_after,
            right: self
                .right_after
                .as_ref()
                .map(|b| {
                    resources
                        .right_wrapper
                        .as_ref()
                        .unwrap_or(&b.geometry.target)
                        .clone()
                })
                .or_else(|| {
                    (right_disposition == SourceTrimRightDisposition::Unchanged)
                        .then(|| self.geometry.right.as_ref().map(|b| b.target.clone()))
                        .flatten()
                }),
            right_disposition,
            fillers: resources.fillers.clone(),
        })
    }
}
impl ProjectDocument {
    /// Resolve exact accepted values. Resources and all complete copied-context
    /// budgets are checked by the ordinary atomic command path before authoring.
    pub fn source_trim_edit(
        &self,
        parent: &NodeId,
        node: &NodeId,
        right: Option<&NodeId>,
        intent: SourceTrimIntent,
    ) -> Result<SourceTrimEditResolution, EditError> {
        let geometry = self.source_trim_geometry(parent, node, right, intent)?;
        resolve(self, geometry)
    }
}
fn resolve(
    document: &ProjectDocument,
    geometry: SourceTrimGeometry,
) -> Result<SourceTrimEditResolution, EditError> {
    let intent = geometry.intent;
    let mut result = SourceTrimEditResolution {
        required_target_wrapper: geometry.target.needs_wrapper,
        geometry,
        right_after: None,
        footprint: None,
        removed_children: Vec::new(),
        fillers: Vec::new(),
        splits: Vec::new(),
        empty_moves: Vec::new(),
        required_right_wrapper: false,
        required_split_nodes: 0,
        required_filler_nodes: 0,
        temporary_nodes: document.nodes().len(),
        capture: SourceTrimCapture::None,
        reanchors: Vec::new(),
        root_operation: None,
    };
    if intent.is_zero() {
        return Ok(result);
    }
    if intent.policy == SourceTrimPolicy::Overwrite
        && (intent.in_frames != 0 || intent.out_frames != 0)
    {
        overwrite(document, &mut result)?;
    } else if intent.roll_frames != 0 {
        result.right_after = result
            .geometry
            .right
            .as_ref()
            .map(|b| final_owner(b, b.allocation_after, b.output_after))
            .transpose()?;
    }
    result.required_right_wrapper = result
        .right_after
        .as_ref()
        .is_some_and(|side| side.needs_wrapper);
    result.required_filler_nodes = result.fillers.len();
    for split in &result.splits {
        result.required_split_nodes = result
            .required_split_nodes
            .checked_add(split.required_nodes)
            .ok_or_else(overflow)?;
    }
    let extra = result
        .required_split_nodes
        .checked_add(result.required_filler_nodes)
        .and_then(|n| n.checked_add(usize::from(result.required_target_wrapper)))
        .and_then(|n| n.checked_add(usize::from(result.required_right_wrapper)))
        .ok_or_else(overflow)?;
    result.temporary_nodes = peak_nodes(document.nodes().len(), extra)?;
    if intent.policy == SourceTrimPolicy::Ripple {
        result.root_operation = Some(RootSoundOperation::Trim {
            range: result.geometry.target.output_before,
            in_frames: intent.in_frames,
            out_frames: intent.out_frames,
        });
        reanchors(document, &mut result)?;
    }
    let changes_edges = intent.in_frames != 0 || intent.out_frames != 0 || intent.roll_frames != 0;
    result.capture = if !result.reanchors.is_empty() {
        SourceTrimCapture::Placements
    } else if changes_edges && crate::audio_binding_lifecycle::has_unbound_recipes(document) {
        SourceTrimCapture::Unbound
    } else {
        SourceTrimCapture::None
    };
    Ok(result)
}
fn overwrite(
    document: &ProjectDocument,
    result: &mut SourceTrimEditResolution,
) -> Result<(), EditError> {
    let g = &result.geometry;
    let a = &g.target;
    let t = a.output_before.start().0;
    let u = a.output_before.end().0;
    let j = a.output_after.start().0;
    let e = a.output_after.end().0;
    let r = g.intent.roll_frames;
    let mut end = u.max(e);
    if let Some(b) = &g.right {
        let v = b.output_before.end().0;
        let seam = add(u, r)?;
        end = end.max(seam.clamp(t, v));
        let n = t.max(seam).max(e);
        if n < v {
            let start = add(
                add(b.allocation_before.start().0, sub(n, u)?)?,
                b.physical_prefix.frames(),
            )?;
            result.right_after = Some(final_owner(
                b,
                range(start, b.allocation_after.end().0)?,
                range(n, v)?,
            )?);
        }
    }
    let footprint = range(t.min(j), end)?;
    result.footprint = Some(footprint);
    let mut occupied = vec![a.output_after];
    if let Some(b) = &result.right_after {
        occupied.push(b.output)
    }
    occupied.sort_by_key(|interval| interval.start());
    let mut cursor = footprint.start().0;
    for interval in occupied {
        let start = interval.start().0.max(footprint.start().0);
        let finish = interval.end().0.min(footprint.end().0);
        if start >= finish {
            continue;
        }
        if cursor < start {
            result.fillers.push(range(cursor, start)?)
        }
        cursor = cursor.max(finish);
    }
    if cursor < footprint.end().0 {
        result.fillers.push(range(cursor, footprint.end().0)?)
    }
    if result.fillers.len() > 2 {
        return Err(invalid(
            "combined Trim filler inventory exceeded two intervals",
        ));
    }
    let NodeKind::Sequence { children } = &document.nodes()[&g.parent].kind else {
        unreachable!()
    };
    let durations = document.durations()?;
    let mut at = g.scope_before.start().0;
    for child in children {
        let next = add(at, durations[child].frames())?;
        if child != &a.target && g.right.as_ref().is_none_or(|b| child != &b.target) {
            for boundary in [footprint.start(), footprint.end()] {
                if at < boundary.0 && boundary.0 < next {
                    result.splits.push(SourceTrimSplit {
                        node: child.clone(),
                        boundary,
                        required_nodes: crate::insert_time::split_node_count(document, child)?,
                    });
                }
            }
        }
        at = next;
    }
    if result.splits.len() > 2 {
        return Err(invalid("combined Trim has more than two exterior splits"));
    }
    let slot = a.slot;
    let first = children[..slot]
        .iter()
        .rposition(|id| durations[id] != FrameDuration::ZERO)
        .map_or(0, |n| n + 1);
    for child in &children[first..slot] {
        result.empty_moves.push(SourceTrimEmptyMove {
            node: child.clone(),
            before: ProjectFrame(t),
            after: ProjectFrame(j),
        })
    }
    let after = slot + 1;
    let count = children[after..]
        .iter()
        .take_while(|id| durations[*id] == FrameDuration::ZERO)
        .count();
    for child in &children[after..after + count] {
        result.empty_moves.push(SourceTrimEmptyMove {
            node: child.clone(),
            before: ProjectFrame(u),
            after: ProjectFrame(e),
        })
    }
    let mut at = g.scope_before.start().0;
    for child in children {
        let next = add(at, durations[child].frames())?;
        let right = g.right.as_ref().is_some_and(|b| child == &b.target);
        let near = result.empty_moves.iter().any(|m| &m.node == child);
        if child != &a.target
            && !near
            && ((right && result.right_after.is_none())
                || (!right
                    && ((at < next && footprint.start().0 <= at && next <= footprint.end().0)
                        || (at == next && footprint.start().0 < at && at < footprint.end().0))))
        {
            result.removed_children.push(child.clone());
        }
        at = next;
    }
    Ok(())
}
fn reanchors(
    document: &ProjectDocument,
    result: &mut SourceTrimEditResolution,
) -> Result<(), EditError> {
    let g = &result.geometry;
    if g.intent.in_frames != 0 {
        result.reanchors.push(owner_anchor(&g.target)?)
    }
    if g.duration_delta_frames == 0 {
        return Ok(());
    }
    let mut at = g.target.output_before.end().0;
    let mut slot = g.target.slot + 1;
    if g.intent.roll_frames != 0 {
        let b = g
            .right
            .as_ref()
            .ok_or_else(|| invalid("combined Trim lost the admitted right Source"))?;
        result.reanchors.push(owner_anchor(b)?);
        at = b.output_before.end().0;
        slot = b.slot + 1;
    }
    let end = g.project_duration_before.frames();
    if at < end {
        let owners = crate::insert_time::composite::shifted_owners(document, &g.parent, slot)?;
        result.reanchors.push(SourceTrimReanchorGroup {
            owners,
            anchor: AudioReanchorAnchor::AllocationEntry,
            window: Some(ExactFrameRange::new(
                ExactRatio::integer(at),
                ExactRatio::integer(end),
            )?),
        });
    }
    let mut seen = BTreeSet::new();
    for group in &result.reanchors {
        for owner in &group.owners {
            if !seen.insert(owner) {
                return Err(invalid("combined Trim assigned two clocks to one owner"));
            }
        }
    }
    Ok(())
}
fn final_owner(
    side: &SourceTrimOwnerGeometry,
    allocation: FrameRange,
    output: FrameRange,
) -> Result<SourceTrimFinalOwner, EditError> {
    let start = ExactRatio::integer(allocation.start().0);
    let end = ExactRatio::integer(allocation.end().0);
    let start = if side.window_after.start().compare(start).is_gt() {
        side.window_after.start()
    } else {
        start
    };
    let end = if side.window_after.end().compare(end).is_lt() {
        side.window_after.end()
    } else {
        end
    };
    let visible_selection = if start.compare(end).is_lt() {
        Some(time(SourceEditWindow::new(start, end))?)
    } else {
        None
    };
    Ok(SourceTrimFinalOwner {
        geometry: side.clone(),
        allocation,
        output,
        visible_selection,
        needs_wrapper: side.target == side.physical_source
            && (allocation.start().0 != 0 || allocation.end().0 != side.after.duration.frames()),
    })
}
fn owner_anchor(side: &SourceTrimOwnerGeometry) -> Result<SourceTrimReanchorGroup, EditError> {
    let (anchor, window) = match side.phase_anchor {
        SourceTrimPhaseAnchor::SourceStart => (
            AudioReanchorAnchor::SourceEndpoint {
                endpoint: AudioSourceEndpoint::Start,
            },
            None,
        ),
        SourceTrimPhaseAnchor::SourceEnd => (
            AudioReanchorAnchor::SourceEndpoint {
                endpoint: AudioSourceEndpoint::End,
            },
            None,
        ),
        SourceTrimPhaseAnchor::Retained { allocation } => {
            let offset = sub(
                side.output_before.start().0,
                side.allocation_before.start().0,
            )?;
            (
                AudioReanchorAnchor::AllocationEntry,
                Some(ExactFrameRange::new(
                    ExactRatio::integer(add(offset, allocation.start().0)?),
                    ExactRatio::integer(add(offset, allocation.end().0)?),
                )?),
            )
        }
    };
    Ok(SourceTrimReanchorGroup {
        owners: BTreeSet::from([side.physical_source.clone()]),
        anchor,
        window,
    })
}
fn bounded_ids<'de, D: serde::Deserializer<'de>, const N: usize>(
    d: D,
) -> Result<Vec<NodeId>, D::Error> {
    struct Visitor<const N: usize>;
    impl<'de, const N: usize> serde::de::Visitor<'de> for Visitor<N> {
        type Value = Vec<NodeId>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {N} fresh node identities")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut ids = Vec::new();
            while let Some(id) = seq.next_element()? {
                if ids.len() == N {
                    return Err(serde::de::Error::custom(
                        "combined Trim identity pool exceeds its limit",
                    ));
                }
                ids.push(id);
            }
            Ok(ids)
        }
    }
    d.deserialize_seq(Visitor::<N>)
}
fn bounded_fillers<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<NodeId>, D::Error> {
    bounded_ids::<D, 2>(d)
}
fn bounded_split<'de, D: serde::Deserializer<'de>>(d: D) -> Result<SplitIdentities, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        #[serde(deserialize_with = "bounded_nodes")]
        nodes: Vec<NodeId>,
    }
    fn bounded_nodes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<NodeId>, D::Error> {
        bounded_ids::<D, MAX_DOCUMENT_NODES>(d)
    }
    Ok(SplitIdentities {
        nodes: Wire::deserialize(d)?.nodes,
    })
}
fn range(a: i64, b: i64) -> Result<FrameRange, EditError> {
    time(FrameRange::new(ProjectFrame(a), ProjectFrame(b)))
}
fn add(a: i64, b: i64) -> Result<i64, EditError> {
    a.checked_add(b).ok_or_else(overflow)
}
fn sub(a: i64, b: i64) -> Result<i64, EditError> {
    a.checked_sub(b).ok_or_else(overflow)
}
fn time<T>(r: Result<T, TimeError>) -> Result<T, EditError> {
    r.map_err(DocumentError::from).map_err(Into::into)
}
fn overflow() -> EditError {
    DocumentError::from(TimeError::Overflow).into()
}
fn invalid(s: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, s)
}
fn limit(s: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, s)
}

fn peak_nodes(existing: usize, extra: usize) -> Result<usize, EditError> {
    let peak = existing.checked_add(extra).ok_or_else(overflow)?;
    if peak > MAX_DOCUMENT_NODES {
        return Err(limit("combined Trim exceeds the temporary node budget"));
    }
    Ok(peak)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporary_copies_must_fit_before_consumed_neighbors_are_removed() {
        assert_eq!(
            peak_nodes(MAX_DOCUMENT_NODES - 8, 8).unwrap(),
            MAX_DOCUMENT_NODES
        );
        assert_eq!(
            peak_nodes(MAX_DOCUMENT_NODES - 7, 8).unwrap_err().code,
            EditErrorCode::LimitExceeded
        );
        assert_eq!(
            peak_nodes(usize::MAX, 1).unwrap_err().code,
            EditErrorCode::TimingOverflow
        );
    }
}
