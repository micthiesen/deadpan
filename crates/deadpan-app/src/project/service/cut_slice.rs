//! Capture privately, save one deletion, then publish the copied structure.

use super::*;
use crate::project::slice::{CaptureRequest, CutReceipt, CutUpdate};
use deadpan_core::{AudioTimingId, ProjectFrame, SliceCaptureSelection, SplitIdentities};

impl Service {
    pub(super) fn cut_edit_slice_command(&mut self, request: CaptureRequest) {
        let result = match self.last_cut.as_ref() {
            Some((previous, receipt)) if previous == &request => Ok(receipt.clone()),
            Some((previous, _)) if previous.id == request.id => {
                Err("The cut identity was already used for a different selection".into())
            }
            _ => self.cut_edit_slice(&request),
        };
        self.cut_slice = Some(CutUpdate { request, result });
    }

    fn cut_edit_slice(&mut self, capture: &CaptureRequest) -> Result<CutReceipt> {
        self.check_context(capture.id.session, &capture.id.source_revision)?;
        // Historical capture resolves the full scope and exact child identity.
        // No register reply is emitted before the transaction succeeds.
        let copied = self.capture_edit_slice(capture)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let view = capture.scope.resolve(workspace)?;
        let new_revision = revision();
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let (command, selected_node) = match &capture.selection {
            SliceCaptureSelection::Range { range } => {
                let preflight = workspace
                    .document
                    .range_deletion(&capture.parent, *range)
                    .map_err(display)?;
                (
                    Command::DeleteRange {
                        parent: capture.parent.clone(),
                        range: *range,
                        identities: SplitIdentities {
                            nodes: (0..preflight.required_ids).map(|_| node()).collect(),
                        },
                        timing,
                    },
                    None,
                )
            }
            SliceCaptureSelection::Child { node } => {
                let index = view
                    .children
                    .iter()
                    .position(|child| child == node)
                    .ok_or("The cut child is outside the captured Sequence")?;
                let selected = view
                    .children
                    .get(index + 1)
                    .or_else(|| {
                        index
                            .checked_sub(1)
                            .and_then(|index| view.children.get(index))
                    })
                    .cloned();
                (
                    Command::DeleteRipple {
                        node: node.clone(),
                        timing,
                    },
                    selected,
                )
            }
        };
        let request = CommandRequest {
            project_id: capture.id.project.clone(),
            expected_revision: capture.id.source_revision.clone(),
            new_revision,
            command,
        };
        let outcome = self.writer()?.commit(&request).map_err(display)?;
        let cursor = copied.slice().range().start();
        let mut receipt = CutReceipt {
            copied,
            committed: CommittedEdit {
                revision: outcome.revision_id,
                selected_node,
                preserve_cursor: false,
                cursor: Some(cursor),
                scope: capture.scope.clone(),
                sound: None,
                range_selection: None,
            },
            refresh_error: None,
        };
        // Retain durable success before optional workspace preparation. A
        // saved cut remains pasteable even if the old view cannot refresh.
        self.last_cut = Some((capture.clone(), receipt.clone()));
        self.committed = Some(receipt.committed.clone());
        #[cfg(test)]
        {
            self.render_preview_refresh_failure = self
                .shared
                .render_commit_refresh_failure
                .swap(false, Ordering::AcqRel);
        }
        match self.refresh() {
            Ok(()) => {
                if matches!(capture.selection, SliceCaptureSelection::Range { .. }) {
                    receipt.committed.selected_node = self.cut_join_child(&capture.scope, cursor);
                }
                self.message = Some(
                    "Cut saved and copied. Paste or preview placement with :splice; Undo restores the cut."
                        .into(),
                );
            }
            Err(error) => {
                let message = format!(
                    "Cut saved and copied, but the preview could not refresh: {error}. Reopen this project before editing or undoing."
                );
                receipt.refresh_error = Some(message.clone());
                self.message = Some(message);
            }
        }
        self.error = None;
        self.committed = Some(receipt.committed.clone());
        self.last_cut = Some((capture.clone(), receipt.clone()));
        Ok(receipt)
    }

    fn cut_join_child(&self, scope: &SequenceScope, cursor: ProjectFrame) -> Option<NodeId> {
        let workspace = self.workspace.as_ref()?;
        let view = scope.resolve(workspace).ok()?;
        let cursor = u64::try_from(cursor.0).ok()?;
        let mut start = view.start;
        for child in view.children {
            let duration = u64::try_from(workspace.plan.node_duration(child)?.frames()).ok()?;
            let end = start.checked_add(duration)?;
            if start <= cursor && cursor < end {
                return Some(child.clone());
            }
            start = end;
        }
        view.children.last().cloned()
    }
}
