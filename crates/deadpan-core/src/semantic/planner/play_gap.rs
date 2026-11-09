//! `SetPlayGap`: change the shared default gap after the current play of the
//! nearest Repeat inside one chosen play. The gap has no authored node, so the
//! first leaf gives this play its own copy (`IsolateGap`, through
//! `EditOccurrence` below enclosing plays) and the second applies the ordinary
//! one-play pause length or sound edit to that copy. One Compound, one Undo.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, ExactRatio, InsertionBias, InstancePath,
    PlayGapChange, RepeatEditBranch,
};

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: crate::SemanticSpeechProvider,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn set_play_gap(
        &mut self,
        trace_index: usize,
        change: PlayGapChange,
    ) -> Result<(), EditError> {
        let scoped = self.context.scoped.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "open a Repeat's contents and choose This play (]r) to change the gap after it",
            )
        })?;
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before changing this play's gap",
            ));
        }
        framing::validate_scoped_context(&self.current, &self.context)?;
        let step = scoped.target.repeats.last().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "a play's gap belongs to a Repeat; open a Repeat's contents first",
            )
        })?;
        if scoped
            .target
            .repeats
            .iter()
            .any(|step| step.branch == RepeatEditBranch::Default)
        {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "choose This play (]r) at every Repeat level to change one play's gap; :repeat gap= changes the shared gap",
            ));
        }
        let RepeatEditBranch::Play { iteration } = &step.branch else {
            unreachable!("every step names a play")
        };
        let repeat = step.repeat.clone();
        let iteration = iteration.clone();
        let mut outer = scoped.presentation.repeats.clone();
        if outer
            .pop()
            .is_none_or(|last| last.node != repeat || last.iteration != iteration)
        {
            return Err(invalid(
                "the shown play differs from the chosen play; choose the play again",
            ));
        }
        let instance = InstancePath {
            node: repeat.clone(),
            repeats: outer,
        };
        let NodeKind::Repeat {
            iterations, gap, ..
        } = &self.current.nodes()[&repeat].kind
        else {
            return Err(invalid("a play's gap belongs to a Repeat"));
        };
        let position = iterations
            .position(&iteration)
            .ok_or_else(|| invalid("the chosen play retired"))?;
        if position + 1 >= iterations.len() {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the last play has no gap after it",
            ));
        }
        if self
            .current
            .gap_overrides()
            .get(&repeat)
            .and_then(|entries| entries.get(&iteration))
            .is_some()
        {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "this play already has its own gap; select it with j and edit it like any pause",
            ));
        }
        let recipe = gap
            .as_ref()
            .filter(|recipe| recipe.duration.frames() > 0)
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "this Repeat has no gap between plays; add one with :repeat gap=",
                )
            })?;
        match &change {
            PlayGapChange::Length { length } => {
                if length.resolve(self.current.presentation_basis().frame_rate)? == recipe.duration
                {
                    return Err(invalid(
                        "this play's gap already has that length; no edit was made",
                    ));
                }
            }
            PlayGapChange::Audio { audio } => {
                if &recipe.audio == audio {
                    return Err(invalid(
                        "this play's gap already has that sound; no edit was made",
                    ));
                }
            }
        }
        let (needs_nodes, needs_marks) = if instance.repeats.is_empty() {
            (0, 0)
        } else {
            let needs = self.current.occurrence_gap_requirements(&instance)?;
            (needs.nodes, needs.marks)
        };
        self.charge_step(false)?;
        let SemanticAllocation::Isolation {
            new_revision,
            identities,
        } = (self.allocate)(SemanticAllocationRequest::Isolation {
            step_index: self.steps.len(),
            nodes: needs_nodes
                .checked_add(1)
                .ok_or_else(|| limit("gap isolation identities overflow"))?,
            marks: needs_marks,
        })?
        else {
            return Err(invalid("a play's own gap requires an Isolation allocation"));
        };
        if identities.nodes.len() != needs_nodes + 1 || identities.marks.len() != needs_marks {
            return Err(invalid(
                "a play's own gap requires its exact isolation identity pools",
            ));
        }
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
        let mut nodes = identities.nodes;
        let id = nodes.pop().expect("the pool holds the gap identity");
        let identities = crate::OccurrenceIdentities {
            nodes,
            marks: identities.marks,
        };
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let (target, presentation) =
            self.current
                .occurrence_gap_selection(&instance, &iteration, &id, &identities)?;
        let command = if instance.repeats.is_empty() {
            Command::IsolateGap {
                node: repeat,
                iteration,
                id,
                timing,
            }
        } else {
            Command::EditOccurrence {
                instance,
                edit: crate::OccurrenceEdit::IsolateGap {
                    iteration,
                    id,
                    timing,
                },
                identities,
            }
        };
        self.commit_leaf(LeafEdit::new(new_revision, command)?)?;
        let gap = SemanticScopedContext {
            root: scoped.root,
            target,
            presentation,
        };
        self.context.scoped = Some(gap.clone());
        self.context.cursor = self.gap_start(&gap.presentation)?;
        framing::validate_scoped_context(&self.current, &self.context)?;
        match change {
            PlayGapChange::Length { length } => {
                self.set_scoped_hold_duration(trace_index, length, gap)
            }
            PlayGapChange::Audio { audio } => self.set_scoped_hold_audio(trace_index, audio, gap),
        }
    }

    fn gap_start(&self, instance: &InstancePath) -> Result<ProjectFrame, EditError> {
        let start = AnchorIndex::new(&self.current)
            .map_err(|error| invalid(&error.to_string()))?
            .resolve_target(&AnchorTarget {
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Occurrence {
                        instance: instance.clone(),
                        position: ExactRatio::ZERO,
                    },
                    bias: InsertionBias::Right,
                },
                occurrence: None,
            })
            .map_err(|error| invalid(&error.to_string()))?;
        Ok(start.frame)
    }
}
