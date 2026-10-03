//! Atomic linked range deletion in a captured ordinary Sequence scope.

use super::*;

impl Service {
    pub(super) fn delete_range(
        &mut self,
        expected_revision: RevisionId,
        scope: SequenceScope,
        parent: NodeId,
        range: deadpan_core::FrameRange,
    ) -> Result<()> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if scope.resolve(workspace)?.owner != &parent {
            return Err("The deletion parent differs from the captured Sequence scope".into());
        }
        let preflight = workspace
            .document
            .range_deletion(&parent, range)
            .map_err(display)?;
        let new_revision = revision();
        let request = CommandRequest {
            project_id: workspace.document.project_id().clone(),
            expected_revision,
            new_revision: new_revision.clone(),
            command: Command::DeleteRange {
                parent,
                range,
                identities: deadpan_core::SplitIdentities {
                    nodes: (0..preflight.required_ids).map(|_| node()).collect(),
                },
                timing: deadpan_core::AudioTimingId {
                    allocation: new_revision,
                    ordinal: 0,
                },
            },
        };
        let outcome = self.writer()?.commit(&request).map_err(display)?;
        self.committed = Some(CommittedEdit {
            scoped: None,
            revision: outcome.revision_id,
            selected_node: None,
            preserve_cursor: false,
            cursor: Some(range.start()),
            scope: scope.clone(),
            sound: None,
            range_selection: None,
        });
        #[cfg(test)]
        {
            // Reuse the existing committed-preview one-shot fault at this
            // durable boundary, without changing ordinary refresh behavior.
            self.render_preview_refresh_failure = self
                .shared
                .render_commit_refresh_failure
                .swap(false, Ordering::AcqRel);
        }
        if let Err(error) = self.refresh() {
            self.message = Some(format!(
                "Edit range cut and saved, but the preview could not refresh: {error}. Reopen this project before editing or undoing."
            ));
            return Ok(());
        }
        // Find the actual retained beat at the join after endpoint splitting.
        // Terminal and empty scopes follow the same last-row rule as navigation.
        let selected = self.workspace.as_ref().and_then(|workspace| {
            let view = scope.resolve(workspace).ok()?;
            let cursor = u64::try_from(range.start().0).ok()?;
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
        });
        self.committed
            .as_mut()
            .expect("durable deletion receipt")
            .selected_node = selected;
        self.message = Some(format!(
            "Cut Edit [{}..{}) · {} frames removed · saved",
            range.start().0,
            range.end().0,
            range.duration().frames()
        ));
        Ok(())
    }
}
