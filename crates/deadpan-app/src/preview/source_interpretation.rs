//! Command entry captures the source, including its absence before import finishes.

use super::*;
use crate::navigation::command::interpretation::{self, Input};

#[derive(Clone, Debug)]
pub(super) struct Target {
    session: u64,
    revision: RevisionId,
    asset: AssetId,
}

impl DeadpanApp {
    pub(super) fn capture_source_interpretation(&self) -> Result<Target, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project with a ready Original first.")?;
        let asset = original_asset(workspace)
            .or(self.selected_source.as_ref())
            .ok_or("Select a registered video first.")?;
        workspace
            .sources
            .get(asset)
            .and_then(|source| source.receipt.snapshot().video())
            .ok_or("The source has no qualified picture yet.")?;
        Ok(Target {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            asset: asset.clone(),
        })
    }

    pub(super) fn interpret_source(
        &mut self,
        target: Option<Result<Target, String>>,
        input: Input,
    ) {
        let result = (|| {
            let target = target.ok_or("Open :source-interpret again to capture its source.")??;
            let workspace = self.workspace.as_ref().ok_or("The project was closed.")?;
            if workspace.session != target.session
                || workspace.document.revision_id() != &target.revision
            {
                return Err("The project changed after command entry. Open :source-interpret again; no edit was made.".into());
            }
            let current = workspace
                .document
                .source_interpretations()
                .get(&target.asset)
                .copied();
            let choice = input.apply(current);
            if input == Input::Show {
                self.message = Some(format!(
                    "Source interpretation: {}. Use :source-interpret range=… chroma=… to change it, or reset.",
                    interpretation::parameters(current)
                ));
                self.error = None;
            } else if choice == current {
                self.message = Some("Source interpretation is unchanged; no edit was made.".into());
                self.error = None;
            } else {
                self.submit(ProjectRequest::InterpretSource {
                    expected_session: target.session,
                    expected_revision: target.revision,
                    asset: target.asset,
                    interpretation: choice,
                });
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    pub(super) fn source_interpretation_control(&mut self, ui: &mut egui::Ui) {
        let Ok(target) = self.capture_source_interpretation() else {
            return;
        };
        let choice = self.workspace.as_ref().and_then(|workspace| {
            workspace
                .document
                .source_interpretations()
                .get(&target.asset)
                .copied()
        });
        let parameters = interpretation::parameters(choice);
        if ui.add_enabled(!self.service.is_busy(), style::row_action(ui, "Interpret picture…", ":source-interpret"))
            .on_hover_text(format!("Current: {parameters}\nCorrect source range or chroma position. The file and timing stay intact; one Undo restores the previous choice.")).clicked() {
            self.open_command(format!("source-interpret {parameters}"), ui.ctx());
        }
    }
}
