//! Check untrusted typed payloads before commands copy them. Serde's bounded
//! vectors do not protect callers that construct the public structs directly.

use crate::{Command, DocumentError, OccurrenceEdit, Subtree};

pub(crate) fn validate_command(command: &Command) -> Result<(), DocumentError> {
    let context = match command {
        Command::InsertTime { hold, .. } | Command::InsertAiTime { hold, .. } => {
            hold.picture_context.as_ref()
        }
        Command::WrapRepeat { gap, .. } | Command::SetRepeat { gap, .. } => gap
            .as_ref()
            .and_then(|recipe| recipe.picture_context.as_ref()),
        Command::SetHoldPictureContext { context, .. } => context.as_ref(),
        Command::Insert { subtree, .. }
        | Command::SetPlayOverride { subtree, .. }
        | Command::SetGapOverride { subtree, .. } => {
            return validate_subtree(subtree);
        }
        Command::EditOccurrence { edit, .. } => return validate_occurrence(edit),
        _ => None,
    };
    context.map_or(Ok(()), super::CapturedFraming::validate)
}

fn validate_occurrence(edit: &OccurrenceEdit) -> Result<(), DocumentError> {
    let context = match edit {
        OccurrenceEdit::WrapRepeat { gap, .. } | OccurrenceEdit::SetRepeat { gap, .. } => gap
            .as_ref()
            .and_then(|recipe| recipe.picture_context.as_ref()),
        OccurrenceEdit::SetHoldPictureContext { context } => context.as_ref(),
        OccurrenceEdit::Insert { subtree, .. }
        | OccurrenceEdit::SetPlayOverride { subtree, .. }
        | OccurrenceEdit::SetGapOverride { subtree, .. } => return validate_subtree(subtree),
        _ => None,
    };
    context.map_or(Ok(()), super::CapturedFraming::validate)
}

fn validate_subtree(subtree: &Subtree) -> Result<(), DocumentError> {
    if subtree.nodes.len() > crate::MAX_DOCUMENT_NODES {
        return Err(super::limit("incoming captured framing exceeds node limit"));
    }
    // The existing validated document and incoming payload each have a bounded
    // record budget. Final validation checks their exact reduced combination,
    // allowing a replacement to retire old context without double-counting it.
    super::validate_nodes(subtree.nodes.values()).map(|_| ())
}
