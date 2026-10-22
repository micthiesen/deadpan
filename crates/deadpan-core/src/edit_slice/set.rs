//! One immutable, source-validated collection of whole authored children.

mod placement;
pub(crate) use placement::apply;

use super::*;
use serde::{Deserializer, Serializer};

pub const MAX_EDIT_SET_COMPONENTS: usize = 64;

/// One member's fresh authored and historical node identities. Marks are
/// allocated once for the complete union, not independently for each member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditSetComponentIdentities {
    pub nodes: Vec<NodeId>,
    pub aliases: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditSetPasteIdentities {
    pub group: NodeId,
    pub components: Vec<EditSetComponentIdentities>,
    /// One identity per original logical mark, in original MarkId order.
    pub marks: Vec<MarkId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditSetIdentityRequirements {
    /// Member node/alias/imported timing requirements; marks is always zero
    /// because the complete union owns the one shared mark pool below.
    pub components: Vec<SliceIdentityRequirements>,
    pub marks: usize,
    /// One dedicated outer command slot, then imported member records and
    /// one suffix slot for each positive-duration seam insertion. The outer
    /// sound journal must not name a private member's intermediate snapshot.
    pub timings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetWire {
    scope: NodeId,
    #[serde(deserialize_with = "bounded_components")]
    components: Vec<CapturedEditSlice>,
    #[serde(deserialize_with = "crate::document::unique_map")]
    marks: BTreeMap<MarkId, Mark>,
}

/// Closed components captured from one immutable document. Components retain
/// their own immediate parents and exact source clocks. The union carries one
/// logical mark when its physical bindings span more than one component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedEditSet {
    wire: SetWire,
    duration: FrameDuration,
    assets: BTreeMap<AssetId, AssetRecord>,
}

impl Serialize for CapturedEditSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CapturedEditSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let json = super::wire::read(deserializer)?;
        Self::from_json(&json).map_err(serde::de::Error::custom)
    }
}

fn bounded_components<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<CapturedEditSlice>, D::Error> {
    struct Components;
    impl<'de> serde::de::Visitor<'de> for Components {
        type Value = Vec<CapturedEditSlice>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("1..=64 whole-child capture components")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut input: A,
        ) -> Result<Self::Value, A::Error> {
            let mut output = Vec::new();
            while let Some(component) = input.next_element()? {
                if output.len() == MAX_EDIT_SET_COMPONENTS {
                    return Err(serde::de::Error::custom("edit set component limit"));
                }
                output.push(component);
            }
            if output.is_empty() {
                return Err(serde::de::Error::custom(
                    "edit set needs at least one component",
                ));
            }
            Ok(output)
        }
    }
    deserializer.deserialize_seq(Components)
}

impl CapturedEditSet {
    /// Capture canonical structural order beneath one ordinary navigation
    /// scope. IDs must be distinct, non-overlapping whole descendants.
    pub fn capture(
        document: &ProjectDocument,
        scope: &NodeId,
        nodes: &[NodeId],
        timing: AudioTimingId,
    ) -> Result<Self, EditError> {
        document.validate()?;
        document.source_splice_boundary(scope, 0)?;
        if nodes.is_empty() || nodes.len() > MAX_EDIT_SET_COMPONENTS {
            return Err(limit("edit set requires 1..=64 components"));
        }
        let mut previous = None;
        let mut components = Vec::with_capacity(nodes.len());
        let mut selected = BTreeSet::new();
        let mut parts = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            let (parent, path) = descendant(document, scope, node)?;
            if previous.as_ref().is_some_and(|previous| previous >= &path) {
                return Err(invalid(
                    "edit set members are not in canonical structural order",
                ));
            }
            previous = Some(path);
            let ordinal = u32::try_from(index)
                .ok()
                .and_then(|index| timing.ordinal.checked_add(index))
                .ok_or_else(|| limit("edit set capture timing ordinal overflow"))?;
            let component = CapturedEditSlice::capture_selection(
                document,
                &parent,
                &SliceCaptureSelection::Child { node: node.clone() },
                AudioTimingId {
                    allocation: timing.allocation.clone(),
                    ordinal,
                },
            )?;
            for selected_node in component.0.nodes.keys() {
                if !selected.insert(selected_node.clone()) {
                    return Err(invalid("edit set repeats or overlaps an authored subtree"));
                }
            }
            parts.extend(component.0.parts.iter().cloned());
            components.push(component);
        }
        let marks = crate::marks::capture_slice_mark_bindings(document, &parts, &selected)?;
        Self::from_wire(SetWire {
            scope: scope.clone(),
            components,
            marks,
        })
    }

    pub fn validate_capture(&self, source: &ProjectDocument) -> Result<(), EditError> {
        if self.project_id() != source.project_id() {
            return Err(EditError::new(
                EditErrorCode::ProjectConflict,
                "edit set belongs to another project",
            ));
        }
        if self.revision_id() != source.revision_id() {
            return Err(EditError::new(
                EditErrorCode::RevisionConflict,
                "edit set capture source is another revision",
            ));
        }
        let nodes = self
            .components()
            .iter()
            .map(|component| match component.selection() {
                SliceCaptureSelection::Child { node } => node.clone(),
                _ => unreachable!("closed set admits only whole children"),
            })
            .collect::<Vec<_>>();
        let captured = Self::capture(source, self.scope(), &nodes, self.capture_timing().clone())?;
        if captured != *self {
            return Err(invalid(
                "edit set payload differs from its immutable captured selection",
            ));
        }
        Ok(())
    }

    pub fn components(&self) -> &[CapturedEditSlice] {
        &self.wire.components
    }
    pub fn scope(&self) -> &NodeId {
        &self.wire.scope
    }
    pub fn project_id(&self) -> &ProjectId {
        self.components()[0].project_id()
    }
    pub fn revision_id(&self) -> &RevisionId {
        self.components()[0].revision_id()
    }
    pub fn presentation_basis(&self) -> &PresentationBasis {
        self.components()[0].presentation_basis()
    }
    pub fn capture_timing(&self) -> &AudioTimingId {
        self.components()[0].capture_timing()
    }
    pub fn duration(&self) -> FrameDuration {
        self.duration
    }
    pub fn assets(&self) -> &BTreeMap<AssetId, AssetRecord> {
        &self.assets
    }

    pub fn identity_requirements(&self) -> Result<EditSetIdentityRequirements, EditError> {
        let mut components = Vec::with_capacity(self.components().len());
        let mut timings = 1usize;
        let mut nodes = 1usize;
        let mut aliases = 0usize;
        for component in self.components() {
            let mut required = component.identity_requirements()?;
            nodes = nodes
                .checked_add(required.nodes)
                .filter(|count| *count <= MAX_DOCUMENT_NODES)
                .ok_or_else(|| limit("edit set pasted node limit"))?;
            aliases = aliases
                .checked_add(required.aliases)
                .filter(|count| *count <= MAX_AUDIO_BINDING_ENTRIES)
                .ok_or_else(|| limit("edit set pasted alias limit"))?;
            timings = timings
                .checked_add(required.timings)
                .and_then(|count| {
                    count.checked_add(usize::from(component.duration() != FrameDuration::ZERO))
                })
                .ok_or_else(|| limit("edit set timing count overflow"))?;
            required.marks = 0;
            components.push(required);
        }
        Ok(EditSetIdentityRequirements {
            components,
            marks: self.wire.marks.len(),
            timings,
        })
    }

    pub fn to_json(&self) -> Result<String, EditError> {
        let mut bytes = SliceJson(Vec::new(), false);
        serde_json::to_writer(&mut bytes, self).map_err(|error| {
            if bytes.1 {
                limit("edit set JSON exceeds byte limit")
            } else {
                DocumentError::json(error).into()
            }
        })?;
        String::from_utf8(bytes.0).map_err(|_| invalid("edit set JSON is not UTF-8"))
    }

    pub fn from_json(json: &str) -> Result<Self, EditError> {
        if json.len() > MAX_COMPOUND_CAPTURE_BYTES {
            return Err(limit("edit set JSON exceeds capture byte limit"));
        }
        // The shared bounded reader rejects duplicate keys before the typed
        // wire is decoded, including when this method is called directly.
        let mut deserializer = serde_json::Deserializer::from_str(json);
        let closed = super::wire::read(&mut deserializer).map_err(DocumentError::json)?;
        deserializer.end().map_err(DocumentError::json)?;
        Self::from_wire(serde_json::from_str(&closed).map_err(DocumentError::json)?)
    }

    fn from_wire(wire: SetWire) -> Result<Self, EditError> {
        if wire.components.is_empty() || wire.components.len() > MAX_EDIT_SET_COMPONENTS {
            return Err(limit("edit set requires 1..=64 components"));
        }
        let first = &wire.components[0];
        let mut duration = 0i64;
        let mut nodes = BTreeSet::new();
        let mut assets = BTreeMap::new();
        let mut targets = BTreeMap::new();
        let mut interpretations = BTreeMap::new();
        let mut previous_end = None;
        for (index, component) in wire.components.iter().enumerate() {
            component.validate()?;
            if !matches!(component.selection(), SliceCaptureSelection::Child { .. })
                || !component.attachments().is_owned()
            {
                return Err(invalid("edit set requires whole-child owned components"));
            }
            let ordinal = u32::try_from(index)
                .ok()
                .and_then(|index| first.capture_timing().ordinal.checked_add(index))
                .ok_or_else(|| limit("edit set capture timing ordinal overflow"))?;
            if component.project_id() != first.project_id()
                || component.revision_id() != first.revision_id()
                || component.presentation_basis() != first.presentation_basis()
                || component.0.source_duration != first.0.source_duration
                || component.capture_timing().allocation != first.capture_timing().allocation
                || component.capture_timing().ordinal != ordinal
            {
                return Err(invalid(
                    "edit set components do not share one capture snapshot and timing allocation",
                ));
            }
            if previous_end.is_some_and(|end| end > component.range().start()) {
                return Err(invalid("edit set component ranges overlap or are reversed"));
            }
            previous_end = Some(component.range().end());
            duration = duration
                .checked_add(component.duration().frames())
                .ok_or_else(overflow)?;
            for id in component.0.nodes.keys() {
                if !nodes.insert(id.clone()) {
                    return Err(invalid("edit set components overlap an authored subtree"));
                }
            }
            merge_equal(
                &mut assets,
                &component.0.assets,
                "edit set captured asset conflict",
            )?;
            merge_equal(
                &mut targets,
                &component.0.targets,
                "edit set captured target conflict",
            )?;
            merge_equal(
                &mut interpretations,
                &component.0.source_interpretations,
                "edit set captured interpretation conflict",
            )?;
        }
        if nodes.len() > MAX_DOCUMENT_NODES
            || nodes.contains(&wire.scope)
            || wire
                .components
                .iter()
                .any(|component| nodes.contains(component.parent()))
        {
            return Err(invalid(
                "edit set scope or a component parent lies inside its selected forest",
            ));
        }
        validate_marks(&wire)?;
        let result = Self {
            wire,
            duration: FrameDuration::new(duration).map_err(DocumentError::from)?,
            assets,
        };
        result.identity_requirements()?;
        result.to_json()?;
        Ok(result)
    }
}

fn merge_equal<K: Ord + Clone, V: PartialEq + Clone>(
    output: &mut BTreeMap<K, V>,
    input: &BTreeMap<K, V>,
    message: &str,
) -> Result<(), EditError> {
    for (key, value) in input {
        if output.get(key).is_some_and(|previous| previous != value) {
            return Err(invalid(message));
        }
        output.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn validate_marks(wire: &SetWire) -> Result<(), EditError> {
    crate::marks::check_binding_limits(&wire.marks)?;
    let mut component_marks = BTreeMap::<MarkId, Vec<MarkFragment>>::new();
    for component in &wire.components {
        for (id, mark) in &component.0.marks {
            let union = wire
                .marks
                .get(id)
                .ok_or_else(|| invalid("edit set omits a component's logical mark"))?;
            if mark.label != union.label
                || mark.boundary.bias != union.boundary.bias
                || mark.loss_policy != union.loss_policy
            {
                return Err(invalid(
                    "edit set logical mark policies differ across components",
                ));
            }
            component_marks
                .entry(id.clone())
                .or_default()
                .extend(mark.bindings());
        }
    }
    if component_marks.len() != wire.marks.len() {
        return Err(invalid(
            "edit set adds a logical mark outside its captured components",
        ));
    }
    for (id, mark) in &wire.marks {
        let bindings = &component_marks[id];
        let union = mark.bindings().collect::<Vec<_>>();
        if union.len() != bindings.len()
            || union.iter().any(|binding| {
                bindings
                    .iter()
                    .filter(|candidate| *candidate == binding)
                    .count()
                    != 1
            })
        {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "edit set cannot close this cross-component mark relationship; no bindings were dropped",
            ));
        }
    }
    Ok(())
}

fn descendant(
    document: &ProjectDocument,
    scope: &NodeId,
    target: &NodeId,
) -> Result<(NodeId, Vec<usize>), EditError> {
    if target == scope || target == document.root() || !document.nodes().contains_key(target) {
        return Err(invalid(
            "edit set target must be a live strict descendant of its scope",
        ));
    }
    let mut child = target.clone();
    let mut immediate = None;
    let mut path = Vec::new();
    for _ in 0..MAX_DOCUMENT_DEPTH {
        let parent = document
            .parent_of(&child)
            .ok_or_else(|| invalid("edit set target is outside its captured scope"))?;
        let NodeKind::Sequence { children } = &document.nodes()[&parent].kind else {
            return Err(invalid(
                "edit set target crosses a Repeat, Retime or Partition owner",
            ));
        };
        immediate.get_or_insert_with(|| parent.clone());
        path.push(
            children
                .iter()
                .position(|node| node == &child)
                .ok_or_else(|| invalid("edit set target is detached"))?,
        );
        if &parent == scope {
            path.reverse();
            return Ok((immediate.expect("a descendant has a parent"), path));
        }
        child = parent;
    }
    Err(limit("edit set ancestry limit"))
}
