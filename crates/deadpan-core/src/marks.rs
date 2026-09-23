//! Persistent boundaries follow stable authored content, never replacement time.
//! Validation and transforms share structural indexes without recursive document
//! validation. Repeats are addressed by compact identities, never expanded.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    Anchor, AnchorError, AnchorErrorCode, AnchorIndex, BoundaryAnchor, Command, DocumentError,
    DocumentErrorCode, ExactRatio, FrameDuration, InsertionBias, InstancePath, IterationId,
    MAX_DOCUMENT_MARKS, MarkId, NodeId, NodeKind, ProjectDocument, RepeatInstance, SourceMoment,
    SourceStream, SourceTimeBase, SourceTimestamp, TimeError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorLossPolicy {
    DeleteOwned,
    KeepUnresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkLossReason {
    OwnerMissing,
    HostMissing,
    ContentMissing,
    OutsideHost,
    OccurrenceMissing,
    GapMissing,
    OutsideMapping,
    SourceUnavailable,
    OutOfRange,
    WrapAmbiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MarkState {
    Bound,
    Unresolved { reason: MarkLossReason },
}

impl<'de> Deserialize<'de> for MarkState {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        // Serde's internally tagged unit variant ignores extra payload fields.
        // An empty struct keeps the wire closed without changing the public
        // Bound variant or its canonical serialized form.
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Bound {},
            Unresolved { reason: MarkLossReason },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Bound {} => Self::Bound,
            Wire::Unresolved { reason } => Self::Unresolved { reason },
        })
    }
}

/// One physical binding of a logical mark. Ownership is the lifetime of an
/// authored node, independent of the coordinate's host or rendered visibility.
/// Bias, label and loss policy belong to the containing logical mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkFragment {
    pub owner: NodeId,
    pub coordinate: Anchor,
    pub state: MarkState,
}

pub const MAX_MARK_BINDINGS: usize = 1024;
pub const MAX_DOCUMENT_MARK_BINDINGS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mark {
    pub owner: NodeId,
    pub label: String,
    pub boundary: BoundaryAnchor,
    pub loss_policy: AnchorLossPolicy,
    pub state: MarkState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fragments: Vec<MarkFragment>,
}

impl Mark {
    /// Primary binding first, followed by retained physical fragments. Copies
    /// are bounded by document validation; no Repeat plays are enumerated.
    pub fn bindings(&self) -> impl Iterator<Item = MarkFragment> + '_ {
        std::iter::once(MarkFragment {
            owner: self.owner.clone(),
            coordinate: self.boundary.coordinate.clone(),
            state: self.state.clone(),
        })
        .chain(self.fragments.iter().cloned())
    }

    pub fn binding_count(&self) -> usize {
        1 + self.fragments.len()
    }

    /// Retain logical policies and deterministic binding order. Callers admit
    /// growth before constructing bindings; lifecycle transforms only remove
    /// entries or relocate them. Complete duplicate bindings collapse here.
    pub(crate) fn with_bindings(
        &self,
        bindings: impl IntoIterator<Item = MarkFragment>,
    ) -> Option<Self> {
        let mut distinct = Vec::new();
        for binding in bindings {
            if !distinct.contains(&binding) {
                distinct.push(binding);
            }
        }
        let mut bindings = distinct.into_iter();
        let primary = bindings.next()?;
        Some(Self {
            owner: primary.owner,
            label: self.label.clone(),
            boundary: BoundaryAnchor {
                coordinate: primary.coordinate,
                bias: self.boundary.bias,
            },
            loss_policy: self.loss_policy,
            state: primary.state,
            fragments: bindings.collect(),
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WrapAnchorPolicy {
    #[default]
    First,
    Unresolved,
}

enum Failure {
    Lost(MarkLossReason),
    Arithmetic(TimeError),
}
impl From<TimeError> for Failure {
    fn from(error: TimeError) -> Self {
        Self::Arithmetic(error)
    }
}
impl From<DocumentError> for Failure {
    fn from(error: DocumentError) -> Self {
        if error.code == DocumentErrorCode::TimingOverflow {
            Self::Arithmetic(TimeError::Overflow)
        } else {
            Self::Lost(MarkLossReason::ContentMissing)
        }
    }
}
impl From<AnchorError> for Failure {
    fn from(error: AnchorError) -> Self {
        Self::Lost(match error.code {
            AnchorErrorCode::TimingOverflow => return Self::Arithmetic(TimeError::Overflow),
            AnchorErrorCode::OccurrenceInvalid | AnchorErrorCode::OccurrenceRequired => {
                MarkLossReason::OccurrenceMissing
            }
            AnchorErrorCode::SourceUnavailable => MarkLossReason::SourceUnavailable,
            AnchorErrorCode::OutsideMapping => MarkLossReason::OutsideMapping,
            _ => MarkLossReason::OutOfRange,
        })
    }
}
impl Failure {
    fn document(self) -> DocumentError {
        match self {
            Self::Arithmetic(error) => error.into(),
            Self::Lost(reason) => DocumentError::new(
                DocumentErrorCode::InvalidAnchor,
                format!("mark cannot bind: {reason:?}"),
            ),
        }
    }
}
type Result<T> = std::result::Result<T, Failure>;
fn lost<T>(reason: MarkLossReason) -> Result<T> {
    Err(Failure::Lost(reason))
}
fn within(position: ExactRatio, end: i64, reason: MarkLossReason) -> Result<()> {
    if position.compare_integer(0).is_lt() || position.compare_integer(end).is_gt() {
        lost(reason)
    } else {
        Ok(())
    }
}

fn within_content(
    position: ExactRatio,
    end: i64,
    bias: InsertionBias,
    reason: MarkLossReason,
) -> Result<()> {
    within(position, end, reason)?;
    if (position == ExactRatio::ZERO && bias == InsertionBias::Left)
        || (position.compare_integer(end).is_eq() && bias == InsertionBias::Right)
    {
        lost(reason)
    } else {
        Ok(())
    }
}

/// Relative to a retained host. Non-repeating grouping ancestors are deliberately
/// absent so grouping and moving within that host preserve content identity.
enum ContentPoint {
    LeadingEdge,
    TrailingEdge,
    Content {
        node: NodeId,
        position: ExactRatio,
        repeats: BTreeMap<NodeId, IterationId>,
        gap: Option<IterationId>,
    },
}

struct Index<'a> {
    anchors: AnchorIndex<'a>,
    /// Positive-duration child ends permit binary boundary selection; empty
    /// sequences have edges but contain no time to capture a neighboring mark.
    sequences: BTreeMap<NodeId, Vec<(i64, NodeId)>>,
}
impl<'a> Index<'a> {
    fn new(
        document: &'a ProjectDocument,
        durations: BTreeMap<NodeId, FrameDuration>,
    ) -> std::result::Result<Self, DocumentError> {
        let mut sequences = BTreeMap::new();
        for (id, node) in document.nodes() {
            if let NodeKind::Sequence { children } = &node.kind {
                let mut end = 0;
                let mut entries = Vec::new();
                for child in children {
                    let duration = durations[child].frames();
                    end += duration; // structural validation checked the sum
                    if duration > 0 {
                        entries.push((end, child.clone()));
                    }
                }
                sequences.insert(id.clone(), entries);
            }
        }
        Ok(Self {
            anchors: AnchorIndex::from_durations(document, durations)?,
            sequences,
        })
    }
    fn duration(&self, node: &NodeId) -> Result<i64> {
        self.anchors
            .durations
            .get(node)
            .map(|duration| duration.frames())
            .ok_or(Failure::Lost(MarkLossReason::HostMissing))
    }
    fn decompose(
        &self,
        host: &NodeId,
        mut position: ExactRatio,
        bias: InsertionBias,
    ) -> Result<ContentPoint> {
        let end = self.duration(host)?;
        within(position, end, MarkLossReason::OutOfRange)?;
        if end == 0 || (position == ExactRatio::ZERO && bias == InsertionBias::Left) {
            return Ok(if bias == InsertionBias::Left {
                ContentPoint::LeadingEdge
            } else {
                ContentPoint::TrailingEdge
            });
        }
        if position.compare_integer(end).is_eq() && bias == InsertionBias::Right {
            return Ok(ContentPoint::TrailingEdge);
        }
        let mut node = host;
        let mut repeats = BTreeMap::new();
        loop {
            match &self.anchors.document.nodes()[node].kind {
                NodeKind::Sequence { .. } => {
                    let children = &self.sequences[node];
                    let selected = children.partition_point(|(end, _)| match bias {
                        InsertionBias::Left => position.compare_integer(*end).is_gt(),
                        InsertionBias::Right => !position.compare_integer(*end).is_lt(),
                    });
                    let (_, child) = children
                        .get(selected)
                        .ok_or(Failure::Lost(MarkLossReason::ContentMissing))?;
                    position =
                        position.checked_sub(ExactRatio::integer(self.anchors.parents[child].1))?;
                    node = child;
                }
                NodeKind::Retime {
                    child,
                    mapping,
                    duration,
                    ..
                } => {
                    position = position
                        .checked_mul(ExactRatio::new(
                            i128::from(mapping.duration().frames()),
                            i128::from(duration.frames()),
                        )?)?
                        .checked_add(ExactRatio::integer(mapping.start().0))?;
                    node = child;
                }
                NodeKind::Repeat { .. } => {
                    let location = self.anchors.repeats[node].locate(position, bias)?;
                    position = location.position;
                    if location.in_gap {
                        return Ok(ContentPoint::Content {
                            node: node.clone(),
                            position,
                            repeats,
                            gap: Some(location.play.iteration),
                        });
                    }
                    repeats.insert(node.clone(), location.play.iteration);
                    node = self
                        .anchors
                        .document
                        .nodes()
                        .get_key_value(&location.play.child)
                        .ok_or(Failure::Lost(MarkLossReason::HostMissing))?
                        .0;
                }
                NodeKind::Source { .. } | NodeKind::Hold { .. } => {
                    return Ok(ContentPoint::Content {
                        node: node.clone(),
                        position,
                        repeats,
                        gap: None,
                    });
                }
            }
        }
    }

    fn reconstruct(
        &self,
        host: &NodeId,
        point: ContentPoint,
        bias: InsertionBias,
        command: &Command,
    ) -> Result<ExactRatio> {
        let end = self.duration(host)?;
        let ContentPoint::Content {
            mut node,
            mut position,
            mut repeats,
            gap,
        } = point
        else {
            return Ok(if matches!(point, ContentPoint::LeadingEdge) {
                ExactRatio::ZERO
            } else {
                ExactRatio::integer(end)
            });
        };
        if !self.anchors.document.nodes().contains_key(&node) {
            return lost(MarkLossReason::ContentMissing);
        }
        if let Some(identity) = gap {
            let play = self
                .anchors
                .repeats
                .get(&node)
                .and_then(|layout| layout.play(&identity))
                .ok_or(Failure::Lost(MarkLossReason::OccurrenceMissing))?;
            if play.gap_after == FrameDuration::ZERO {
                return lost(MarkLossReason::GapMissing);
            }
            within_content(
                position,
                play.gap_after.frames(),
                bias,
                MarkLossReason::OutOfRange,
            )?;
            position = position
                .checked_add(ExactRatio::integer(play.start))?
                .checked_add(ExactRatio::integer(play.duration.frames()))?;
        } else {
            within_content(
                position,
                self.duration(&node)?,
                bias,
                MarkLossReason::OutOfRange,
            )?;
        }
        while &node != host {
            let (parent, offset) = self
                .anchors
                .parents
                .get(&node)
                .ok_or(Failure::Lost(MarkLossReason::OutsideHost))?;
            position = match &self.anchors.document.nodes()[parent].kind {
                NodeKind::Sequence { .. } => position.checked_add(ExactRatio::integer(*offset))?,
                NodeKind::Repeat { .. } => {
                    let identity = match repeats.remove(parent) {
                        Some(identity) => identity,
                        None => self.wrapped_iteration(parent, command)?,
                    };
                    let play = self.anchors.repeats[parent]
                        .play(&identity)
                        .ok_or(Failure::Lost(MarkLossReason::OccurrenceMissing))?;
                    if play.child != node {
                        return lost(MarkLossReason::ContentMissing);
                    }
                    position.checked_add(ExactRatio::integer(play.start))?
                }
                NodeKind::Retime {
                    mapping, duration, ..
                } => {
                    let selected = position.checked_sub(ExactRatio::integer(mapping.start().0))?;
                    within_content(
                        selected,
                        mapping.duration().frames(),
                        bias,
                        MarkLossReason::OutsideMapping,
                    )?;
                    selected.checked_mul(ExactRatio::new(
                        i128::from(duration.frames()),
                        i128::from(mapping.duration().frames()),
                    )?)?
                }
                _ => return lost(MarkLossReason::OutsideHost),
            };
            node = parent.clone();
        }
        if !repeats.is_empty() {
            return lost(MarkLossReason::OccurrenceMissing);
        }
        within(position, end, MarkLossReason::OutOfRange)?;
        Ok(position)
    }

    fn wrapped_iteration(&self, parent: &NodeId, command: &Command) -> Result<IterationId> {
        if let Command::WrapRepeat {
            id, anchor_policy, ..
        } = command
            && id == parent
        {
            if *anchor_policy == WrapAnchorPolicy::Unresolved {
                return lost(MarkLossReason::WrapAmbiguous);
            }
            if let NodeKind::Repeat { iterations, .. } = &self.anchors.document.nodes()[parent].kind
            {
                return iterations
                    .at(0)
                    .ok_or(Failure::Lost(MarkLossReason::OccurrenceMissing));
            }
        }
        lost(MarkLossReason::OccurrenceMissing)
    }

    fn occurrence(&self, old: &InstancePath, command: &Command) -> Result<InstancePath> {
        self.duration(&old.node)?;
        let mut previous: BTreeMap<_, _> = old
            .repeats
            .iter()
            .map(|step| (step.node.clone(), step.iteration.clone()))
            .collect();
        let mut node = &old.node;
        let mut repeats = Vec::new();
        while let Some((parent, _)) = self.anchors.parents.get(node) {
            if let NodeKind::Repeat { iterations, .. } = &self.anchors.document.nodes()[parent].kind
            {
                let identity = match previous.remove(parent) {
                    Some(identity) => identity,
                    None => self.wrapped_iteration(parent, command)?,
                };
                if iterations.position(&identity).is_none()
                    || self.anchors.repeats[parent]
                        .play(&identity)
                        .is_none_or(|play| &play.child != node)
                {
                    return lost(MarkLossReason::OccurrenceMissing);
                }
                repeats.push(RepeatInstance {
                    node: parent.clone(),
                    iteration: identity,
                });
            }
            node = parent;
        }
        if !previous.is_empty() {
            return lost(MarkLossReason::OccurrenceMissing);
        }
        repeats.reverse();
        Ok(InstancePath {
            node: old.node.clone(),
            repeats,
        })
    }

    fn validate_bound(&self, binding: &MarkFragment, bias: InsertionBias) -> Result<()> {
        if !self.anchors.document.nodes().contains_key(&binding.owner) {
            return lost(MarkLossReason::OwnerMissing);
        }
        match &binding.coordinate {
            Anchor::Local { node, position } => {
                self.decompose(node, *position, bias)?;
            }
            Anchor::Occurrence { instance, position } => {
                self.decompose(&instance.node, *position, bias)?;
                self.anchors.to_project(instance, *position, false)?;
            }
            Anchor::Sequence { frame } => within(
                ExactRatio::integer(frame.0),
                self.duration(self.anchors.document.root())?,
                MarkLossReason::OutOfRange,
            )?,
            Anchor::Source { asset, moment } => {
                let record = self
                    .anchors
                    .document
                    .assets()
                    .get(asset)
                    .ok_or(Failure::Lost(MarkLossReason::SourceUnavailable))?;
                let (stream, timestamp) = match *moment {
                    SourceMoment::Timestamp { stream, timestamp } => (stream, timestamp),
                    SourceMoment::AudioSample {
                        sample,
                        sample_rate,
                    } => (
                        SourceStream::Audio,
                        SourceTimestamp {
                            ticks: sample,
                            time_base: SourceTimeBase::new(1, sample_rate)?,
                        },
                    ),
                };
                let span = match stream {
                    SourceStream::Video => record.video,
                    SourceStream::Audio => record.audio,
                }
                .ok_or(Failure::Lost(MarkLossReason::SourceUnavailable))?;
                crate::anchor::source_fraction(timestamp, span)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_marks(
    document: &ProjectDocument,
    durations: &BTreeMap<NodeId, FrameDuration>,
) -> std::result::Result<(), DocumentError> {
    check_binding_limits(document.marks())?;
    if document.marks().is_empty() {
        return Ok(());
    }
    let index = Index::new(document, durations.clone())?;
    for mark in document.marks().values() {
        crate::document::validate_label(&mark.label)?;
        for binding in mark.bindings() {
            validate_binding(&index, &binding, mark.boundary.bias, mark.loss_policy)?;
        }
    }
    Ok(())
}

fn check_binding_limits(
    marks: &BTreeMap<MarkId, Mark>,
) -> std::result::Result<usize, DocumentError> {
    if marks.len() > MAX_DOCUMENT_MARKS {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "document exceeds 100,000 marks",
        ));
    }
    let mut total = 0_usize;
    for mark in marks.values() {
        if mark.binding_count() > MAX_MARK_BINDINGS {
            return Err(DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "logical mark exceeds 1,024 physical bindings",
            ));
        }
        total = total.checked_add(mark.binding_count()).ok_or_else(|| {
            DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "mark binding count overflow",
            )
        })?;
        if total > MAX_DOCUMENT_MARK_BINDINGS {
            return Err(DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "document exceeds 100,000 physical mark bindings",
            ));
        }
    }
    Ok(total)
}

fn validate_binding(
    index: &Index<'_>,
    binding: &MarkFragment,
    bias: InsertionBias,
    loss_policy: AnchorLossPolicy,
) -> std::result::Result<(), DocumentError> {
    match &binding.coordinate {
        Anchor::Occurrence { instance, position } => {
            instance.validate_depth()?;
            if position.compare_integer(0).is_lt() {
                return Err(Failure::Lost(MarkLossReason::OutOfRange).document());
            }
        }
        Anchor::Local { position, .. } if position.compare_integer(0).is_lt() => {
            return Err(Failure::Lost(MarkLossReason::OutOfRange).document());
        }
        Anchor::Sequence { frame } if frame.0 < 0 => {
            return Err(Failure::Lost(MarkLossReason::OutOfRange).document());
        }
        Anchor::Source {
            moment: SourceMoment::AudioSample { sample_rate: 0, .. },
            ..
        } => return Err(Failure::Lost(MarkLossReason::SourceUnavailable).document()),
        _ => {}
    }
    match binding.state {
        MarkState::Bound => index
            .validate_bound(binding, bias)
            .map_err(Failure::document)?,
        MarkState::Unresolved { .. } if loss_policy != AnchorLossPolicy::KeepUnresolved => {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidAnchor,
                "only keep_unresolved marks can retain unresolved state",
            ));
        }
        MarkState::Unresolved { .. } => {}
    }
    Ok(())
}

pub(crate) fn transform_marks(
    before: &ProjectDocument,
    after: &ProjectDocument,
    command: &Command,
) -> std::result::Result<BTreeMap<MarkId, Mark>, DocumentError> {
    // Validate the new structure even when no marks are present. Marks may be
    // temporarily dangling here; they are transformed before full validation.
    let new_durations = after.structural_durations()?;
    if before.marks().is_empty() {
        return Ok(BTreeMap::new());
    }
    let old = Index::new(before, before.structural_durations()?)?;
    let new = Index::new(after, new_durations)?;
    let mut output = BTreeMap::new();
    for (id, mark) in before.marks() {
        let mut bindings = Vec::with_capacity(mark.binding_count());
        for original in mark.bindings() {
            let mut binding = original.clone();
            if matches!(binding.state, MarkState::Unresolved { .. }) {
                bindings.push(binding);
                continue;
            }
            let transformed = (|| -> Result<()> {
                if !after.nodes().contains_key(&binding.owner) {
                    return lost(MarkLossReason::OwnerMissing);
                }
                match &mut binding.coordinate {
                    Anchor::Local { node, position } => {
                        let point = old.decompose(node, *position, mark.boundary.bias)?;
                        *position = new.reconstruct(node, point, mark.boundary.bias, command)?;
                    }
                    Anchor::Occurrence { instance, position } => {
                        let point = old.decompose(&instance.node, *position, mark.boundary.bias)?;
                        *position =
                            new.reconstruct(&instance.node, point, mark.boundary.bias, command)?;
                        *instance = new.occurrence(instance, command)?;
                    }
                    Anchor::Source { .. } | Anchor::Sequence { .. } => {}
                }
                new.validate_bound(&binding, mark.boundary.bias)
            })();
            match transformed {
                Ok(()) => {
                    bindings.push(binding);
                }
                Err(Failure::Arithmetic(error)) => return Err(error.into()),
                Err(Failure::Lost(reason)) => {
                    if mark.loss_policy == AnchorLossPolicy::KeepUnresolved {
                        // Preserve the last bound coordinate, even if intermediate
                        // calculations succeeded before a later ancestor was lost.
                        let mut unresolved = original.clone();
                        unresolved.state = MarkState::Unresolved { reason };
                        bindings.push(unresolved);
                    }
                }
            }
        }
        if let Some(mark) = mark.with_bindings(bindings) {
            output.insert(id.clone(), mark);
        }
    }
    Ok(output)
}

/// A transparent clone changes structure, not local time. Keeping coordinates
/// exact here lets the ordinary edit transform decompose against the isolated
/// structure afterward, including ancestor-local boundaries inside that play.
pub(crate) fn clone_occurrence_marks(
    before: &ProjectDocument,
    selected: &RepeatInstance,
    mapping: &BTreeMap<NodeId, NodeId>,
    mut fresh_mark: impl FnMut() -> std::result::Result<MarkId, DocumentError>,
) -> std::result::Result<BTreeMap<MarkId, Mark>, DocumentError> {
    if before.marks().is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut total_bindings = check_binding_limits(before.marks())?;
    let mut copies = 0_usize;
    for mark in before.marks().values() {
        let owned = mark
            .bindings()
            .filter(|binding| {
                mapping.contains_key(&binding.owner)
                    && matches!(
                        binding.coordinate,
                        Anchor::Local { .. } | Anchor::Source { .. }
                    )
            })
            .count();
        copies += usize::from(owned > 0);
        total_bindings = total_bindings.checked_add(owned).ok_or_else(|| {
            DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "occurrence isolation exceeds mark binding limit",
            )
        })?;
        if total_bindings > MAX_DOCUMENT_MARK_BINDINGS {
            return Err(DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "occurrence isolation exceeds mark binding limit",
            ));
        }
    }
    if before
        .marks()
        .len()
        .checked_add(copies)
        .is_none_or(|count| count > MAX_DOCUMENT_MARKS)
    {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "occurrence isolation exceeds mark limit",
        ));
    }
    let index = Index::new(before, before.structural_durations()?)?;
    let mut output = BTreeMap::new();
    for (id, original) in before.marks() {
        let mut retained = Vec::with_capacity(original.binding_count());
        let mut copied = Vec::new();
        for original_binding in original.bindings() {
            let mut binding = original_binding.clone();
            if binding.state == MarkState::Bound
                && let Anchor::Occurrence { instance, position } = &mut binding.coordinate
            {
                let enters = instance.repeats.contains(selected);
                if let Some(owner) = mapping.get(&binding.owner) {
                    let point_enters = match index
                        .decompose(&instance.node, *position, original.boundary.bias)
                        .map_err(Failure::document)?
                    {
                        ContentPoint::Content {
                            node, repeats, gap, ..
                        } => {
                            repeats.get(&selected.node) == Some(&selected.iteration)
                                || (node == selected.node
                                    && gap.as_ref() == Some(&selected.iteration))
                        }
                        _ => false,
                    };
                    if enters || point_enters {
                        binding.owner = owner.clone();
                    }
                }
                if enters {
                    crate::occurrence_edit::remap_instance(instance, mapping);
                }
            }
            retained.push(binding);
            if matches!(
                original_binding.coordinate,
                Anchor::Local { .. } | Anchor::Source { .. }
            ) && let Some(owner) = mapping.get(&original_binding.owner)
            {
                let mut copy = original_binding;
                copy.owner = owner.clone();
                // Unresolved records retain their last coordinate and never bind as
                // a side effect of cloning, even when a matching host now exists.
                if copy.state == MarkState::Bound
                    && let Anchor::Local { node, .. } = &mut copy.coordinate
                    && let Some(host) = mapping.get(node)
                {
                    *node = host.clone();
                }
                copied.push(copy);
            }
        }
        if let Some(mark) = original.with_bindings(retained) {
            output.insert(id.clone(), mark);
        }
        if let Some(mark) = original.with_bindings(copied) {
            output.insert(fresh_mark()?, mark);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColorPolicy, FrameRate, PresentationBasis, ProjectId, RevisionId};

    #[test]
    fn binding_rebuild_deduplicates_complete_identity_and_keeps_first_primary() {
        let primary = MarkFragment {
            owner: NodeId::new("owner").unwrap(),
            coordinate: Anchor::Local {
                node: NodeId::new("host").unwrap(),
                position: ExactRatio::ONE,
            },
            state: MarkState::Bound,
        };
        let mark = Mark {
            owner: primary.owner.clone(),
            label: "Logical".into(),
            boundary: BoundaryAnchor {
                coordinate: primary.coordinate.clone(),
                bias: InsertionBias::Left,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
            fragments: vec![],
        };
        let other = MarkFragment {
            owner: NodeId::new("other").unwrap(),
            ..primary.clone()
        };
        let unresolved = MarkFragment {
            state: MarkState::Unresolved {
                reason: MarkLossReason::HostMissing,
            },
            ..primary.clone()
        };
        let rebuilt = mark
            .with_bindings([
                other.clone(),
                primary.clone(),
                other.clone(),
                unresolved.clone(),
                primary.clone(),
            ])
            .unwrap();
        assert_eq!(
            rebuilt.bindings().collect::<Vec<_>>(),
            [other, primary, unresolved]
        );
        assert_eq!(rebuilt.label, mark.label);
        assert_eq!(rebuilt.boundary.bias, mark.boundary.bias);
        assert_eq!(rebuilt.loss_policy, mark.loss_policy);
        assert!(mark.with_bindings([]).is_none());
    }

    #[test]
    fn mark_count_limit_applies_to_retained_unresolved_records_too() {
        let root = NodeId::new("root").unwrap();
        let mut document = ProjectDocument::new(
            ProjectId::new("project").unwrap(),
            RevisionId::new("revision").unwrap(),
            PresentationBasis {
                width: 1,
                height: 1,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            root.clone(),
        )
        .unwrap();
        let mark = Mark {
            fragments: Vec::new(),
            owner: root.clone(),
            label: String::new(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: root,
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Left,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Unresolved {
                reason: MarkLossReason::ContentMissing,
            },
        };
        document.marks = (0..=MAX_DOCUMENT_MARKS)
            .map(|index| (MarkId::new(format!("mark-{index}")).unwrap(), mark.clone()))
            .collect();
        assert_eq!(
            document.validate().unwrap_err().code,
            DocumentErrorCode::LimitExceeded
        );
        document
            .marks
            .remove(&MarkId::new(format!("mark-{MAX_DOCUMENT_MARKS}")).unwrap());
        document.validate().unwrap();
    }
}
