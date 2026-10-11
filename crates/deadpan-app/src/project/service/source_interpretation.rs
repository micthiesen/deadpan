//! Revision-bound source corrections preserve the immutable receipt and clocks.

use super::*;

impl Service {
    pub(super) fn interpret_source(
        &mut self,
        session: u64,
        expected: RevisionId,
        asset: AssetId,
        interpretation: Option<deadpan_core::SourceInterpretation>,
    ) -> Result<()> {
        self.check_context(session, &expected)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let source = workspace
            .sources
            .get(&asset)
            .ok_or("The source is not registered")?;
        let video = source
            .receipt
            .snapshot()
            .video()
            .ok_or("The source has no qualified picture")?;
        video
            .interpretation_support()
            .validate(interpretation.map(Into::into).unwrap_or_default())
            .map_err(display)?;
        let request = CommandRequest {
            project_id: workspace.document.project_id().clone(),
            expected_revision: expected,
            new_revision: revision(),
            command: Command::SetSourceInterpretation {
                asset,
                interpretation,
            },
        };
        let outcome = self.writer()?.commit(&request).map_err(display)?;
        self.capture_preparation_notices(&outcome.generation_preparation_notices);
        let mut saved = super::super::SourceInterpretationSaved {
            session,
            project: request.project_id,
            base: request.expected_revision,
            revision: outcome.revision_id,
            refresh_error: None,
        };
        self.source_interpretation_saved = Some(saved.clone());
        // No selection-changing receipt: both cursors, pane and beat stay put.
        if let Err(error) = self.refresh() {
            let error = format!(
                "Source interpretation saved as {}, but the preview could not refresh: {error}. Reopen this project before editing or undoing.",
                saved.revision
            );
            saved.refresh_error = Some(error.clone());
            self.source_interpretation_saved = Some(saved);
            return Err(error);
        }
        self.message = Some("Source interpretation saved. Undo with u.".into());
        Ok(())
    }
}
