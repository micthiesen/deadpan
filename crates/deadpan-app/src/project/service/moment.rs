//! A copied Original range is admitted against its captured identity and target.

use deadpan_core::{ProjectFrame, SourceQualificationId};

use super::splice::Request;
use super::*;

pub(super) struct PendingMoment {
    request: Request,
    qualification: SourceQualificationId,
    cursor: ProjectFrame,
}

impl Service {
    pub(super) fn paste_moment(&mut self, request: super::super::MomentPaste) -> Result<()> {
        let super::super::MomentPaste {
            expected_session: session,
            expected_revision,
            asset,
            qualification,
            ordinals,
            scope,
            parent,
            destination,
        } = request;
        self.check_context(session, &expected_revision)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let view = scope.resolve(workspace)?;
        if view.owner != &parent {
            return Err("Paste target is outside the captured Sequence scope".into());
        }
        let source = workspace
            .sources
            .get(&asset)
            .cloned()
            .ok_or("Copied Original is no longer registered")?;
        if source.receipt.id() != &qualification {
            return Err("Copied Original qualification has changed; select and copy again".into());
        }
        let (request, cursor) = Request::capture(
            workspace,
            &parent,
            &destination,
            &asset,
            ordinals,
            &source.label,
        )?;
        let moment = PendingMoment {
            request,
            qualification,
            cursor,
        };
        if let Some((cached_asset, token)) = self.cached.take() {
            if cached_asset == asset {
                let cancelled = AtomicBool::new(false);
                let outcome = moment
                    .request
                    .commit(self.writer()?, Some(&token), &cancelled);
                match outcome {
                    Ok(commit) => {
                        self.cached = Some((cached_asset, token));
                        self.complete_moment(&moment, scope, commit.revision_id);
                        return Ok(());
                    }
                    Err(StoreError::OriginalMedia(_) | StoreError::Io(_)) => {}
                    Err(error) => {
                        self.cached = Some((cached_asset, token));
                        return Err(display(error));
                    }
                }
            } else {
                self.cached = Some((cached_asset, token));
            }
        }
        if self.active.is_some() {
            return Err("Paste needs source preparation; wait for the active import".into());
        }
        let streams = Streams::Exact {
            video: true,
            audio: source
                .receipt
                .snapshot()
                .audio()
                .map(|audio| audio.stream().stream_index),
        };
        let record = self
            .writer()?
            .original_record(source.original.object().content())
            .map_err(display)?
            .ok_or("Original ownership record is missing")?;
        self.begin(
            PathBuf::from(&source.label),
            streams,
            None,
            None,
            scope,
            Work::Qualify { record, streams },
        )?;
        self.active
            .as_mut()
            .ok_or("Paste preparation did not start")?
            .moment = Some(moment);
        if let Some(status) = &mut self.import {
            status.stage = ImportStage::PreparingInsertion;
        }
        Ok(())
    }

    pub(super) fn register_moment(
        &mut self,
        active: Pending,
        prepared: PreparedSourceRegistration,
    ) -> Result<()> {
        let moment = active.moment.ok_or("Missing captured Original moment")?;
        if prepared.receipt().id() != &moment.qualification {
            return Err("Fresh source qualification differs from the copied Original".into());
        }
        let commit = moment
            .request
            .commit(self.writer()?, Some(&prepared), &active.cancelled)
            .map_err(display)?;
        let asset = moment
            .request
            .asset()
            .ok_or("Original paste has no asset")?
            .clone();
        self.cached = Some((asset.clone(), prepared));
        self.complete_moment(&moment, active.scope, commit.revision_id);
        if let Some(status) = &mut self.import {
            status.stage = ImportStage::Complete;
            status.asset = Some(asset);
        }
        Ok(())
    }

    fn complete_moment(
        &mut self,
        moment: &PendingMoment,
        scope: SequenceScope,
        revision: RevisionId,
    ) {
        self.complete_slice_placement(
            &moment.request,
            moment.cursor,
            scope,
            revision,
            "Original moment",
        );
    }

    pub(super) fn complete_slice_placement(
        &mut self,
        request: &Request,
        cursor: ProjectFrame,
        scope: SequenceScope,
        revision: RevisionId,
        label: &str,
    ) {
        // SQLite already committed. Preserve its receipt even if rebuilding
        // the workspace fails, and never label a saved cold paste as a failed
        // import or invite an implicit second edit.
        self.committed = Some(CommittedEdit {
            scoped: None,
            revision,
            selected_node: request.node().cloned(),
            preserve_cursor: false,
            cursor: Some(cursor),
            scope,
            sound: None,
            range_selection: None,
        });
        #[cfg(test)]
        {
            self.render_preview_refresh_failure = self
                .shared
                .splice_commit_refresh_failure
                .swap(false, Ordering::AcqRel);
        }
        self.message = Some(match self.refresh() {
            Ok(()) => format!("{label} pasted and saved. Undo with u."),
            Err(error) => {
                format!(
                    "{label} saved, but the preview could not refresh: {error}. Reopen the project to view the saved edit."
                )
            }
        });
    }
}
