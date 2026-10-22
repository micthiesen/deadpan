//! One duration intent over a captured, disjoint selection of Repeat bodies.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, ExactFrameRange, ExactRatio, InsertionBias,
    InstancePath, RepeatEditBranch, RepeatInstance, ScopedIsolationRecord, ScopedNodeTarget,
};

/// Invocation authority only. The first target remains the primary inspected
/// Hold; counterparts are retained through execution, never rebuilt by slots.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSelectedHoldDurationContext {
    pub root: NodeId,
    pub target: ScopedNodeTarget,
    pub presentation: Option<InstancePath>,
    #[serde(deserialize_with = "bounded_also")]
    pub also: Vec<ScopedNodeTarget>,
}

fn bounded_also<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ScopedNodeTarget>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<ScopedNodeTarget>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("1..=1023 further selected Hold targets")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if values.len() >= crate::MAX_SCOPED_TARGETS - 1 {
                    return Err(serde::de::Error::custom(
                        "selected Hold targets exceed the bound",
                    ));
                }
                values.push(value);
            }
            if values.is_empty() {
                return Err(serde::de::Error::custom(
                    "selected Hold duration needs further targets",
                ));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

fn targets(
    authority: &SemanticSelectedHoldDurationContext,
) -> impl Iterator<Item = &ScopedNodeTarget> {
    std::iter::once(&authority.target).chain(&authority.also)
}

/// Inspect one compact path against an index built once for the whole set.
/// This independently checks complete Repeat authority, including aliases.
fn body_dormant(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    parents: &BTreeMap<NodeId, NodeId>,
) -> Result<bool, EditError> {
    if target.repeats.is_empty() || target.repeats.len() > crate::MAX_DOCUMENT_DEPTH {
        return Err(unavailable(
            "selected Hold duration requires bounded explicit Repeat ancestry",
        ));
    }
    if !matches!(
        document.nodes().get(&target.node).map(|node| &node.kind),
        Some(NodeKind::Hold { .. })
    ) {
        return Err(unavailable("every selected duration target must be a Hold"));
    }
    let mut current = target.node.clone();
    let mut remaining = target.repeats.len();
    let mut dormant = false;
    for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
        if &current == document.root() {
            return if remaining == 0 {
                Ok(dormant)
            } else {
                Err(unavailable(
                    "selected Hold target has extra Repeat authority",
                ))
            };
        }
        let parent = parents
            .get(&current)
            .ok_or_else(|| unavailable("selected Hold target has no project ancestry"))?;
        match &document.nodes()[parent].kind {
            NodeKind::Sequence { .. } => {}
            NodeKind::Repeat {
                child, iterations, ..
            } => {
                remaining = remaining
                    .checked_sub(1)
                    .ok_or_else(|| unavailable("selected Hold target omits a Repeat ancestor"))?;
                let step = &target.repeats[remaining];
                if &step.repeat != parent {
                    return Err(unavailable(
                        "selected Hold target names the wrong Repeat ancestor",
                    ));
                }
                match &step.branch {
                    RepeatEditBranch::Default => {
                        if &current != child {
                            return Err(unavailable(
                                "selected Default must follow its literal body",
                            ));
                        }
                        let owned = document
                            .overrides()
                            .get(parent)
                            .map_or(0, |entries| entries.len());
                        dormant |= u64::try_from(owned)
                            .is_ok_and(|count| count == u64::from(iterations.len()));
                    }
                    RepeatEditBranch::Play { iteration } => {
                        if iterations.position(iteration).is_none() {
                            return Err(unavailable("selected Hold target names a retired Play"));
                        }
                        let body = document
                            .overrides()
                            .get(parent)
                            .and_then(|entries| entries.get(iteration))
                            .unwrap_or(child);
                        if body != &current {
                            return Err(unavailable(
                                "selected Hold duration supports bodies, not owned gaps",
                            ));
                        }
                    }
                }
            }
            _ => {
                return Err(unavailable(
                    "selected Hold duration requires Sequence/Repeat bodies without Retime or Partition ancestry",
                ));
            }
        }
        current = parent.clone();
    }
    Err(EditError::new(
        EditErrorCode::LimitExceeded,
        "selected Hold ancestry exceeds the depth bound",
    ))
}

fn uniform_level(authority: &SemanticSelectedHoldDurationContext) -> Result<usize, EditError> {
    for (index, primary) in authority.target.repeats.iter().enumerate() {
        let RepeatEditBranch::Play { iteration } = &primary.branch else {
            continue;
        };
        let mut plays = BTreeSet::from([iteration]);
        if authority.also.iter().all(|other| {
            let Some(step) = other.repeats.get(index) else {
                return false;
            };
            let RepeatEditBranch::Play { iteration } = &step.branch else {
                return false;
            };
            step.repeat == primary.repeat
                && plays.insert(iteration)
                && other.repeats[..index] == authority.target.repeats[..index]
        }) {
            return Ok(index);
        }
    }
    Err(unavailable(
        "selected Hold durations need distinct Plays of one Repeat under the same captured prefix",
    ))
}

impl ProjectDocument {
    /// Validate every body target and one distinct-Play axis, then return only
    /// the primary Hold's exact visibility. None positively proves dormancy.
    pub fn selected_hold_duration_visibility(
        &self,
        parent: &NodeId,
        authority: &SemanticSelectedHoldDurationContext,
    ) -> Result<Option<HoldDurationVisibility>, EditError> {
        if authority.also.is_empty() || authority.also.len() >= crate::MAX_SCOPED_TARGETS {
            return Err(unavailable(
                "selected Hold duration needs 2..=1024 captured targets",
            ));
        }
        self.validate()?;
        scope_bounds(self, parent)?;
        let Some(NodeKind::Sequence { children }) = self.nodes().get(parent).map(|node| &node.kind)
        else {
            return Err(unavailable(
                "selected Hold duration requires an ordinary Sequence parent",
            ));
        };
        if !children.contains(&authority.root)
            || !matches!(
                self.nodes().get(&authority.root).map(|node| &node.kind),
                Some(NodeKind::Repeat { .. })
            )
        {
            return Err(unavailable(
                "selected Hold duration inspector root must be a direct Repeat child",
            ));
        }
        let parents = self
            .nodes()
            .keys()
            .flat_map(|parent| {
                self.children(parent)
                    .map(move |child| (child.clone(), parent.clone()))
            })
            .collect();
        let mut primary_dormant = false;
        for (index, target) in targets(authority).enumerate() {
            if target.repeats.first().map(|step| &step.repeat) != Some(&authority.root) {
                return Err(unavailable(
                    "selected Hold target lies outside its captured outermost Repeat",
                ));
            }
            let dormant = body_dormant(self, target, &parents)?;
            if index == 0 {
                primary_dormant = dormant;
            }
        }
        uniform_level(authority)?;
        let NodeKind::Hold { recipe } = &self.nodes()[&authority.target.node].kind else {
            unreachable!("body checked");
        };
        // A batch retains the scalar occurrence/mixed external-sound limit,
        // even if an individual member happens to be a no-op.
        crate::sound_events::validate_command(
            self,
            &Command::SetScopedHoldDuration {
                target: authority.target.clone(),
                duration: recipe.duration,
                identities: Default::default(),
            },
        )?;
        let instance = match (&authority.presentation, primary_dormant) {
            (None, true) => return Ok(None),
            (Some(instance), false) if authority.target.matches_instance(self, instance)? => {
                instance
            }
            _ => {
                return Err(unavailable(
                    "selected Hold duration requires its live primary presentation or proven dormancy",
                ));
            }
        };
        let boundary = crate::derived_clock::with_sealed(|| {
            AnchorIndex::new(self)
                .and_then(|index| {
                    index.resolve_target(&AnchorTarget {
                        boundary: BoundaryAnchor {
                            coordinate: Anchor::Occurrence {
                                instance: instance.clone(),
                                position: ExactRatio::ZERO,
                            },
                            bias: InsertionBias::Right,
                        },
                        occurrence: None,
                    })
                })
                .map_err(|error| unavailable(&error.to_string()))
        })?;
        if boundary.exact_frame.denominator() != 1 {
            return Err(unavailable(
                "selected Hold duration requires integral Repeat clocks",
            ));
        }
        let end = boundary
            .frame
            .0
            .checked_add(recipe.duration.frames())
            .ok_or_else(|| invalid("selected Hold end overflows"))?;
        Ok(Some(HoldDurationVisibility {
            exact: ExactFrameRange::new(boundary.exact_frame, ExactRatio::integer(end))?,
            frames: FrameRange::new(boundary.frame, ProjectFrame(end))
                .map_err(crate::DocumentError::from)?,
        }))
    }
}

pub(super) fn validate_context(
    document: &ProjectDocument,
    context: &SemanticContext,
) -> Result<(), EditError> {
    let Some(authority) = &context.selected_hold_duration else {
        return Ok(());
    };
    if context.original.is_some()
        || context.scoped.is_some()
        || context.hold_duration.is_some()
        || context.shared_hold_duration.is_some()
        || context.mixed_hold_duration.is_some()
        || context.visual_selection.is_some()
        || context.selected_child.as_ref() != Some(&authority.root)
    {
        return Err(unavailable(
            "selected Hold duration requires Your edit, its selected inspector root and no other authority or Visual selection",
        ));
    }
    document.selected_hold_duration_visibility(&context.parent, authority)?;
    Ok(())
}

fn explicit_instance(target: &ScopedNodeTarget) -> Result<InstancePath, EditError> {
    Ok(InstancePath {
        node: target.node.clone(),
        repeats: target
            .repeats
            .iter()
            .map(|step| {
                let RepeatEditBranch::Play { iteration } = &step.branch else {
                    return Err(invalid(
                        "a concrete duration path requires explicit Play authority",
                    ));
                };
                Ok(RepeatInstance {
                    node: step.repeat.clone(),
                    iteration: iteration.clone(),
                })
            })
            .collect::<Result<_, EditError>>()?,
    })
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
{
    pub(super) fn set_selected_hold_duration(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
        mut authority: SemanticSelectedHoldDurationContext,
    ) -> Result<(), EditError> {
        validate_context(&self.current, &self.context)?;
        let duration = length.resolve(self.current.presentation_basis().frame_rate)?;
        let count = authority.also.len() + 1;
        let mut changed = false;
        for index in 0..count {
            let target = if index == 0 {
                &authority.target
            } else {
                &authority.also[index - 1]
            }
            .clone();
            let NodeKind::Hold { recipe } = &self.current.nodes()[&target.node].kind else {
                unreachable!("all bodies validated");
            };
            if recipe.duration == duration {
                continue;
            }
            let mixed = target
                .repeats
                .iter()
                .any(|step| step.branch == RepeatEditBranch::Default);
            let concrete = if mixed {
                None
            } else {
                Some(explicit_instance(&target)?)
            };
            let requirements = if let Some(instance) = &concrete {
                self.current
                    .occurrence_hold_duration_requirements(instance, duration)?
            } else {
                self.current
                    .scoped_hold_duration_requirements(&target, duration)?
            };
            self.charge_step(false)?;
            let SemanticAllocation::Isolation {
                new_revision,
                identities,
            } = (self.allocate)(SemanticAllocationRequest::Isolation {
                step_index: self.steps.len(),
                nodes: requirements.nodes,
                marks: requirements.marks,
            })?
            else {
                return Err(invalid(
                    "selected Hold duration requires an Isolation allocation",
                ));
            };
            self.reserve_revision(&new_revision)?;
            for node in &identities.nodes {
                self.reserve_node(node)?;
            }
            for mark in &identities.marks {
                if !self.marks.insert(mark.clone()) {
                    return Err(identity(
                        "macro reuses an authored or allocated mark identity",
                    ));
                }
            }
            let command = if let Some(instance) = concrete {
                Command::EditOccurrence {
                    instance,
                    edit: crate::OccurrenceEdit::SetHoldDuration { duration },
                    identities,
                }
            } else {
                Command::SetScopedHoldDuration {
                    target,
                    duration,
                    identities,
                }
            };
            let leaf = LeafEdit::new(new_revision, command)?;
            // Actual production execution supplies the mapping. No sampled
            // counterpart or all-Play conversion replaces a mixed address.
            let request = leaf.request(&self.current);
            let (_, after, steps) = crate::command::apply_with_isolation(&self.current, &request)?;
            let record =
                ScopedIsolationRecord::from_execution(&self.current, &request, &after, steps)?;
            let proof = record.bind_prepared(&self.current, &after)?;
            authority.target = proof.map_forward(&authority.target)?;
            authority.also = authority
                .also
                .iter()
                .map(|target| proof.map_forward(target))
                .collect::<Result<_, _>>()?;
            authority.presentation = authority
                .presentation
                .as_ref()
                .map(|instance| proof.map_instance_forward(instance))
                .transpose()?;
            self.commit_leaf(leaf)?;
            if self.current != after {
                return Err(invalid(
                    "selected Hold execution differs from its prepared proof",
                ));
            }
            changed = true;
        }
        if !changed {
            return Err(invalid(
                "the selected pauses already have that duration; no edit was made",
            ));
        }
        let visible = self
            .current
            .selected_hold_duration_visibility(&self.context.parent, &authority)?;
        if let Some(visible) = visible {
            self.context.cursor = visible.frames.start();
        }
        self.trace[trace_index].resolved_parent = self.current.parent_of(&authority.target.node);
        self.trace[trace_index].resolved_selection = Some(SliceCaptureSelection::Child {
            node: authority.target.node.clone(),
        });
        self.trace[trace_index].resolved_range = visible.map(|visible| visible.frames);
        self.context.selected_hold_duration = Some(Box::new(authority));
        super::validate_context(&self.current, &self.context)?;
        Ok(())
    }
}
