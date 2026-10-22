//! Named Edit range intent shared by semantic operators and native selection.

use crate::{
    Anchor, AnchorIndex, BoundarySelector, EditError, EditErrorCode, FrameRange, MediaRole,
    NamedMarkTarget, ProjectDocument, ResolvedSelectionKind, SelectionRequest, SemanticContext,
    SemanticVisualSelection,
};

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

fn edit_authority(context: &SemanticContext) -> Result<(), EditError> {
    if context.original.is_some()
        || context.scoped.is_some()
        || context.hold_duration.is_some()
        || context.shared_hold_duration.is_some()
        || context.mixed_hold_duration.is_some()
        || context.selected_hold_duration.is_some()
    {
        return Err(unavailable(
            "named mark ranges require ordinary Your edit selection authority",
        ));
    }
    Ok(())
}

/// Scope-only resolution avoids recursively validating a context whose Visual
/// selection is itself a named mark range. Every caller retains the same clock,
/// exact endpoint and ordinary parent policy.
pub(super) fn resolve_in_scope(
    document: &ProjectDocument,
    context: &SemanticContext,
    start: &NamedMarkTarget,
    end: &NamedMarkTarget,
) -> Result<FrameRange, EditError> {
    edit_authority(context)?;
    let bounds = super::planner::scope_bounds(document, &context.parent)?;
    for target in [start, end] {
        if document.marks().get(&target.id).is_some_and(|mark| {
            mark.bindings()
                .any(|binding| matches!(binding.coordinate, Anchor::Source { .. }))
        }) {
            return Err(unavailable(
                "a named Edit range cannot use a mark on the Original clock",
            ));
        }
    }
    let anchor_error = |error: crate::AnchorError| unavailable(&error.to_string());
    let resolved = AnchorIndex::new(document)
        .map_err(anchor_error)?
        .resolve(&SelectionRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            role: MediaRole::Linked,
            selector: BoundarySelector::MarkRange {
                start: start.clone(),
                end: end.clone(),
            },
        })
        .map_err(anchor_error)?;
    let ResolvedSelectionKind::Range { start, end, frames } = resolved.selection else {
        return Err(unavailable(
            "named marks must resolve one nonempty Edit range",
        ));
    };
    if start.exact_frame.compare_integer(bounds.0.0).is_lt()
        || end.exact_frame.compare_integer(bounds.1.0).is_gt()
        || frames.start() < bounds.0
        || frames.end() > bounds.1
    {
        return Err(unavailable(
            "the named mark range is outside the current Sequence",
        ));
    }
    Ok(frames)
}

impl ProjectDocument {
    /// Resolve two named Edit boundaries against this immutable document and
    /// the invocation's checked ordinary Sequence. No cursor or authored state
    /// changes, and an unresolved address never falls back to a selected beat.
    pub fn resolve_semantic_mark_range(
        &self,
        context: &SemanticContext,
        start: &NamedMarkTarget,
        end: &NamedMarkTarget,
    ) -> Result<FrameRange, EditError> {
        edit_authority(context)?;
        super::planner::validate_context(self, context)?;
        resolve_in_scope(self, context, start, end)
    }

    /// A finished Visual selection retaining named intent, not cached frames.
    /// Later staged operators resolve these same addresses on their own revision.
    pub fn select_semantic_mark_range(
        &self,
        context: &SemanticContext,
        start: NamedMarkTarget,
        end: NamedMarkTarget,
    ) -> Result<SemanticContext, EditError> {
        let mut result = context.clone();
        // Explicit replacement must not depend on an old selection's marks
        // still resolving. All other invocation authority and scope is checked.
        result.visual_selection = None;
        let range = self.resolve_semantic_mark_range(&result, &start, &end)?;
        result.cursor = range.end();
        result.visual_selection = Some(SemanticVisualSelection::MarkRange { start, end });
        Ok(result)
    }
}
