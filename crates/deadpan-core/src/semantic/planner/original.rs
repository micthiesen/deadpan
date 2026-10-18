//! Admission and motion on measured presentation ordinals. These never pass
//! through the independent project-frame cursor or Visual selection.

use super::*;

pub(super) fn validate_original_context(
    document: &ProjectDocument,
    context: &SemanticContext,
    index: Option<&crate::SourceFrameIndex>,
) -> Result<(), EditError> {
    let Some(original) = &context.original else {
        return Ok(());
    };
    let index = index.ok_or_else(|| invalid("Original macros require a measured picture index"))?;
    let asset = document
        .assets()
        .get(&original.asset)
        .ok_or_else(|| invalid("the macro Original asset is missing"))?;
    let video = asset
        .video
        .ok_or_else(|| invalid("the macro Original has no measured video"))?;
    if asset.source_qualification.as_ref() != Some(&original.qualification)
        || index.asset() != &original.asset
        || index.time_base() != video.start().time_base
        || index.frames().first().map(|frame| frame.pts) != Some(video.start().ticks)
        || index.terminal_end() != video.end().ticks
        || asset.frame_count.is_some_and(|count| {
            u64::try_from(count.frames()).ok() != u64::try_from(index.frames().len()).ok()
        })
    {
        return Err(invalid(
            "the macro Original differs from its qualified measured picture index",
        ));
    }
    if original.ordinal.0
        > u64::try_from(index.frames().len())
            .map_err(|_| invalid("Original picture count is not representable"))?
    {
        return Err(invalid(
            "the macro Original cursor is outside its measured picture index",
        ));
    }
    Ok(())
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
{
    pub(super) fn move_original(&mut self, forward: bool, count: u32) -> Result<(), EditError> {
        let end = self.original_frame_count()?;
        let original = self
            .context
            .original
            .as_mut()
            .ok_or_else(|| invalid("Original context is missing"))?;
        original.ordinal.0 = if forward {
            original.ordinal.0.saturating_add(u64::from(count)).min(end)
        } else {
            original.ordinal.0.saturating_sub(u64::from(count))
        };
        Ok(())
    }

    pub(super) fn move_original_scope(&mut self, end: bool) -> Result<(), EditError> {
        let ordinal = if end { self.original_frame_count()? } else { 0 };
        self.context
            .original
            .as_mut()
            .ok_or_else(|| invalid("Original context is missing"))?
            .ordinal = crate::SourceFrameId(ordinal);
        Ok(())
    }

    fn original_frame_count(&self) -> Result<u64, EditError> {
        let index = self
            .original_index
            .as_ref()
            .ok_or_else(|| invalid("Original macros require a measured picture index"))?;
        u64::try_from(index.frames().len())
            .map_err(|_| invalid("Original picture count is not representable"))
    }
}
