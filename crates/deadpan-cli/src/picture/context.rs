//! Bounded reads for temporal conditioning analysis. Never a preview proxy.

use std::time::Instant;

use deadpan_core::SourceFrameIndex;

use super::*;

/// The reader borrows the already qualified provider for one analysis window.
/// It cannot switch assets or bypass the session's immutable receipt admission.
pub(crate) struct ContextPictureReader<'a> {
    retained: &'a mut RetainedSource,
    generated: &'a deadpan_store::generated_media::GeneratedReadHandle,
    stats: &'a mut PictureSessionStats,
    cancelled: &'a AtomicBool,
    deadline: Instant,
    reads_left: usize,
}

impl ProjectPictureSession {
    /// Capture the same durable temporal descriptor used by store relevance,
    /// from this session's already queried immutable context and receipts.
    pub(crate) fn context_input_binding(
        &self,
        context: &deadpan_plan::ScopedHoldContext,
        capture: deadpan_store::generation_inputs::GenerationCaptureSpec,
        region: Option<&deadpan_core::TargetId>,
    ) -> Result<deadpan_store::generation_inputs::GenerationInputBinding, String> {
        deadpan_store::generation_inputs::GenerationInputCapture::from_context(
            &self.document,
            context,
            capture,
            &self.store.generation_pictures(),
        )
        .and_then(|binding| {
            deadpan_store::generation_inputs::GenerationInputCapture::with_region(
                binding,
                &self.document,
                region,
            )
        })
        .map_err(|error| error.to_string())
    }

    pub(crate) fn context_picture_reader<'a>(
        &'a mut self,
        picture: &Picture,
        cancelled: &'a AtomicBool,
        deadline: Instant,
        maximum_reads: usize,
    ) -> Result<ContextPictureReader<'a>, String> {
        control(cancelled, deadline)?;
        if maximum_reads == 0 || maximum_reads > 512 {
            return Err("Temporal context exceeds its picture-read budget.".into());
        }
        // Admission is deliberately shared with export/conditioning, including
        // accepted object provenance and complete Original index comparison.
        // The admission picture is bounded separately from the window reads.
        match self
            .prepare_picture(picture, cancelled)
            .map_err(|e| e.to_string())?
        {
            PreparedPicture::Frame { .. } | PreparedPicture::Generated { .. } => {}
            PreparedPicture::Background => {
                return Err("Authored black has no media index.".into());
            }
        }
        control(cancelled, deadline)?;
        Ok(ContextPictureReader {
            retained: self
                .retained
                .as_mut()
                .ok_or("Context provider was not retained.")?,
            generated: &self.generated,
            stats: &mut self.stats,
            cancelled,
            deadline,
            reads_left: maximum_reads,
        })
    }
}

impl ContextPictureReader<'_> {
    pub(crate) fn index(&self) -> &SourceFrameIndex {
        self.retained.source.index().index()
    }

    pub(crate) fn info(&self) -> &deadpan_source::SourceStreamInfo {
        self.retained.source.info()
    }

    pub(crate) fn frame(&mut self, id: SourceFrameId) -> Result<Rgba8Frame, String> {
        control(self.cancelled, self.deadline)?;
        self.reads_left = self
            .reads_left
            .checked_sub(1)
            .ok_or("Temporal context exceeds its picture-read budget.")?;
        let timeout = self
            .deadline
            .saturating_duration_since(Instant::now())
            .min(FRAME_TIMEOUT);
        if matches!(self.retained.origin, RetainedOrigin::Generated(_)) {
            self.generated
                .check_live(self.cancelled)
                .map_err(|e| e.to_string())?;
        }
        let decoded = self
            .retained
            .source
            .frame(id, timeout, self.cancelled)
            .map_err(|e| e.to_string())?;
        let mut frame = source_to_render_frame(decoded, self.info()).map_err(|e| e.to_string())?;
        if let RetainedOrigin::Generated(artifact) = &self.retained.origin {
            if let Some(aspect) = artifact.content_aspect {
                frame = shared::fill_canvas_aspect(frame, aspect).map_err(|e| e.to_string())?;
            }
            self.generated
                .check_live(self.cancelled)
                .map_err(|e| e.to_string())?;
        }
        self.stats.decoded_frames += 1;
        control(self.cancelled, self.deadline)?;
        Ok(frame)
    }
}

fn control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), String> {
    check_cancel(cancelled).map_err(|e| e.to_string())?;
    if Instant::now() >= deadline {
        return Err("Temporal context preparation exceeded its deadline.".into());
    }
    Ok(())
}
