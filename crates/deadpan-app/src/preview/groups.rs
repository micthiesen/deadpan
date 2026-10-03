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

pub(super) fn instruction(
    document: &ProjectDocument,
    context: &SemanticContext,
    label: Option<String>,
) -> Result<SemanticInstruction, String> {
    if let Some(label) = label {
        deadpan_core::validate_group_label(&label).map_err(|error| error.to_string())?;
        let selector = if let Some(selection) = &context.visual_selection {
            if selection.anchor == selection.head {
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
