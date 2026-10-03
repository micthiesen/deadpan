//! Capture privately, save one deletion, then publish the copied structure.

use super::*;
use crate::project::semantic::{CutAttempt, LastEdit, RepeatableCut};
use crate::project::slice::{CaptureRequest, CutReceipt, CutUpdate};
use deadpan_core::{
    AudioTimingId, ProjectFrame, SemanticSelector, SliceCaptureSelection, SplitIdentities,
};

impl Service {
    pub(super) fn cut_edit_slice_command(
        &mut self,
        request: CaptureRequest,
        attempt: Option<CutAttempt>,
    ) {
        let result = match self.last_cut.as_ref() {
            Some((previous, saved_attempt, receipt))
                if previous == &request && saved_attempt == &attempt =>
            {
                Ok(receipt.clone())
            }
            Some((previous, _, _)) if previous.id == request.id => {
                Err("The cut identity was already used for a different selection".into())
            }
            _ => self.cut_edit_slice(&request, attempt.as_ref()),
        };
        self.cut_slice = Some(CutUpdate { request, result });
    }

    fn cut_edit_slice(
        &mut self,
        capture: &CaptureRequest,
        attempt: Option<&CutAttempt>,
    ) -> Result<CutReceipt> {
        self.check_register_request(&capture.id)?;
        if let Some(attempt) = attempt {
            self.check_cut_attempt(capture, attempt)?;
        }
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
        let name = super::registers::name(capture.register)?;
        let mut bank = self.prepare_register_write(
            name,
            crate::project::registers::Value::Edited(copied.clone()),
        )?;
        let (outcome, saved) = self
            .writer()?
            .cut_to_register(&request, name, copied.slice().clone(), None)
            .map_err(display)?;
        if let Some(attempt) = attempt {
            self.semantic.prove(
                capture.id.session,
                &capture.id.project,
                &capture.id.source_revision,
                &outcome.revision_id,
                semantic::Change::Replace(LastEdit {
                    operation: attempt.operation.clone(),
                    register: capture.register,
                }),
            );
        }
        bank.version = saved.version;
        self.registers = Some(Arc::new(bank));
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
        self.last_cut = Some((capture.clone(), attempt.cloned(), receipt.clone()));
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
        self.last_cut = Some((capture.clone(), attempt.cloned(), receipt.clone()));
        Ok(receipt)
    }

    fn check_cut_attempt(&mut self, capture: &CaptureRequest, attempt: &CutAttempt) -> Result<()> {
        if attempt.repeat_version.is_some()
            && !matches!(attempt.operation, RepeatableCut::Frames(_))
        {
            return Err("Repeating a selector requires its full semantic context".into());
        }
        self.observe_semantic();
        self.check_context(capture.id.session, &capture.id.source_revision)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let snapshot = self
            .semantic
            .snapshot()
            .ok_or("Repeat state is unavailable")?;
        if snapshot.head.as_ref() != Some(&capture.id.source_revision) {
            return Err("The saved project changed. Reopen it before cutting content.".into());
        }
        if let Some(version) = attempt.repeat_version
            && (snapshot.version != version
                || snapshot.edit_for(workspace)?.operation != attempt.operation)
        {
            return Err(
                "The last semantic edit changed. Start the repeat again; no edit was made.".into(),
            );
        }
        match (&attempt.operation, &capture.selection) {
            (RepeatableCut::Frames(operation), SliceCaptureSelection::Range { range }) => {
                if operation
                    .resolve(&workspace.document, &capture.parent, range.start())
                    .map_err(display)?
                    != *range
                {
                    return Err(
                        "The frame cut differs from its requested count and current cursor".into(),
                    );
                }
            }
            (
                RepeatableCut::Selector(SemanticSelector::SelectedBeat),
                SliceCaptureSelection::Child { .. },
            ) => {}
            (
                RepeatableCut::Selector(SemanticSelector::VisualSelection),
                SliceCaptureSelection::Range { range },
            ) if range.start() < range.end() => {}
            (RepeatableCut::Selector(SemanticSelector::Motion { .. }), _) => {
                return Err("A motion cut requires its entry cursor and semantic context".into());
            }
            _ => return Err("The cut selection differs from its semantic intent".into()),
        }
        Ok(())
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
