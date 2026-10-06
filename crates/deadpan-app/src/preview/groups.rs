//! Group commands retain their command-entry target through native text entry.

use super::*;
use deadpan_core::{ProjectDocument, SemanticContext, SemanticInstruction, SemanticSelector};

impl DeadpanApp {
    pub(super) fn group_command(
        &mut self,
        target: Option<Result<macros::Capture, String>>,
        label: Option<String>,
    ) {
        let target = target
            .unwrap_or_else(|| Err("Open the Group command again to capture its target.".into()));
        let instruction = target
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|capture| capture.group_instruction(label));
        self.cancel_repeats("a Group command was requested");
        self.apply_recorded_instruction(target, instruction);
    }
}

impl DeadpanApp {
    /// `:explode` or `:duplicate`, recorded and dot-repeatable like Ungroup.
    pub(super) fn structure_command(
        &mut self,
        target: Option<Result<macros::Capture, String>>,
        explode: bool,
    ) {
        let target =
            target.unwrap_or_else(|| Err("Open the command again to capture its target.".into()));
        let instruction = target
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|capture| capture.structure_instruction(explode));
        self.cancel_repeats(if explode {
            "an Explode command was requested"
        } else {
            "a Duplicate command was requested"
        });
        self.apply_recorded_instruction(target, instruction);
    }
}

/// Explode needs one selected direct-child Repeat and no Visual range.
/// Duplicate copies the current Visual range, or else the selected beat.
pub(super) fn structure_instruction(
    document: &ProjectDocument,
    context: &SemanticContext,
    explode: bool,
) -> Result<SemanticInstruction, String> {
    if explode {
        if context.visual_selection.is_some() {
            return Err("Clear the Visual range before exploding a selected Repeat.".into());
        }
        let selected = context
            .selected_child
            .as_ref()
            .ok_or("Select a Repeat before exploding it.")?;
        direct_child(document, context, selected)?;
        if !matches!(
            document.nodes().get(selected).map(|node| &node.kind),
            Some(NodeKind::Repeat { .. })
        ) {
            return Err("Select a Repeat before exploding it.".into());
        }
        return Ok(SemanticInstruction::Explode);
    }
    let selector = match &context.visual_selection {
        Some(deadpan_core::SemanticVisualSelection::Time { anchor, head, .. })
            if anchor == head =>
        {
            return Err("The Edit selection is empty. Move a boundary before duplicating.".into());
        }
        Some(_) => SemanticSelector::VisualSelection,
        None => {
            let selected = context
                .selected_child
                .as_ref()
                .ok_or("Select a beat or a Visual range before duplicating.")?;
            direct_child(document, context, selected)?;
            SemanticSelector::SelectedBeat
        }
    };
    Ok(SemanticInstruction::Duplicate { selector })
}

fn direct_child(
    document: &ProjectDocument,
    context: &SemanticContext,
    selected: &NodeId,
) -> Result<(), String> {
    let Some(NodeKind::Sequence { children }) =
        document.nodes().get(&context.parent).map(|node| &node.kind)
    else {
        return Err("This command needs an ordinary Sequence scope.".into());
    };
    if !children.contains(selected) {
        return Err("The captured target is not a direct child of this group.".into());
    }
    Ok(())
}

pub(super) fn instruction(
    document: &ProjectDocument,
    context: &SemanticContext,
    label: Option<String>,
) -> Result<SemanticInstruction, String> {
    if let Some(label) = label {
        deadpan_core::validate_group_label(&label).map_err(|error| error.to_string())?;
        let selector = if let Some(selection) = &context.visual_selection {
            if matches!(selection, deadpan_core::SemanticVisualSelection::Time { anchor, head, .. } if anchor == head)
            {
                return Err("The Edit selection is empty. Move a boundary before grouping.".into());
            }
            SemanticSelector::VisualSelection
        } else {
            selected_child(document, context)?;
            SemanticSelector::SelectedBeat
        };
        Ok(SemanticInstruction::Group { selector, label })
    } else {
        if context.visual_selection.is_some() {
            return Err("Clear the Visual range before ungrouping a selected Sequence.".into());
        }
        let selected = selected_child(document, context)?;
        if !matches!(
            document.nodes().get(selected).map(|node| &node.kind),
            Some(NodeKind::Sequence { .. })
        ) {
            return Err("Select a neutral Sequence group before ungrouping.".into());
        }
        // The shared planner checks framing, audio treatments and authored
        // edges atomically. They cannot be discarded or distributed by the UI.
        Ok(SemanticInstruction::Ungroup)
    }
}

fn selected_child<'a>(
    document: &ProjectDocument,
    context: &'a SemanticContext,
) -> Result<&'a NodeId, String> {
    let selected = context
        .selected_child
        .as_ref()
        .ok_or("Select a beat before grouping or ungrouping.")?;
    let Some(NodeKind::Sequence { children }) =
        document.nodes().get(&context.parent).map(|node| &node.kind)
    else {
        return Err("Group commands need an ordinary Sequence scope.".into());
    };
    if !children.contains(selected) {
        return Err("The captured Group target is not a direct child of this group.".into());
    }
    Ok(selected)
}

#[cfg(test)]
mod tests;
