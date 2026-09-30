//! Retain one exact uncommitted placement on the project owner.

use deadpan_core::{AudioTimingId, EditTransaction, FrameRange, ProjectFrame};
use deadpan_store::source_registration::SourceMomentInsertionRequest;

use super::*;
use crate::project::splice::{
    Prepared as PreparedSplice, Proposal, ProposalId, ProposalUpdate, SpliceCommitUpdate,
};

pub(super) struct Draft {
    proposal: Proposal,
    request: SourceMomentInsertionRequest,
    cursor: ProjectFrame,
    source: Option<PreparedSourceRegistration>,
    prepared: Option<Arc<PreparedSplice>>,
}

impl Service {
    pub(super) fn prepare_splice_command(&mut self, proposal: Proposal) {
        let id = proposal.id.clone();
        if let Err(error) = self.prepare_splice(proposal) {
            if self
                .splice_draft
                .as_ref()
                .is_some_and(|draft| draft.proposal.id == id)
            {
                self.invalidate_splice(&error);
            }
            if let Some(active) = &self.active
                && active.splice.as_ref() == Some(&id)
            {
                active.cancelled.store(true, Ordering::Release);
            }
            self.splice = Some(ProposalUpdate {
                id,
                result: Err(error),
            });
        }
    }

    fn check_splice_context(&self, id: &ProposalId) -> Result<()> {
        if id.session == 0 || id.draft == 0 || id.change == 0 {
            return Err("Slice proposal identities must be nonzero".into());
        }
        self.check_context(id.session, &id.base_revision)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if workspace.document.project_id() != &id.project || self.pending_session_change.is_some() {
            return Err("Slice proposal project session changed".into());
        }
        Ok(())
    }

    fn prepare_splice(&mut self, proposal: Proposal) -> Result<()> {
        self.check_splice_context(&proposal.id)?;
        if let Some(seen) = &self.splice_seen
            && seen.session == proposal.id.session
            && (proposal.id.draft < seen.draft
                || (proposal.id.draft == seen.draft
                    && (proposal.id.change <= seen.change
                        || proposal.id.base_revision != seen.base_revision)))
        {
            return Err("Slice proposal identity was already used or superseded".into());
        }
        // Reserve even a subsequently rejected refinement. It must not resurrect
        // the preceding ready draft or reuse a proposal revision.
        self.splice_seen = Some(proposal.id.clone());
        self.splice = None;
        let previous = self.splice_draft.take();
        let mut token = None;
        if let Some(previous) = previous {
            let same_source = previous.proposal.asset == proposal.asset
                && previous.proposal.qualification == proposal.qualification;
            if same_source {
                token = previous.source;
            } else {
                self.cache_splice_source(&previous.proposal.asset, previous.source);
            }
            if let Some(active) = &mut self.active
                && active.splice.as_ref() == Some(&previous.proposal.id)
            {
                if same_source && !active.cancelled.load(Ordering::Acquire) {
                    active.splice = Some(proposal.id.clone());
                } else {
                    active.cancelled.store(true, Ordering::Release);
                }
            }
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let view = proposal.scope.resolve(workspace)?;
        if view.owner != &proposal.parent || proposal.index > view.children.len() {
            return Err("Slice destination is outside the captured Sequence scope".into());
        }
        if proposal.ordinals.start >= proposal.ordinals.end {
            return Err("Select a nonempty Original slice".into());
        }
        let source = workspace
            .sources
            .get(&proposal.asset)
            .cloned()
            .ok_or("Copied Original is no longer registered")?;
        if source.receipt.id() != &proposal.qualification || source.video_index.is_none() {
            return Err("Copied Original qualification has changed; select and copy again".into());
        }
        let cursor = workspace
            .document
            .source_splice_boundary(&proposal.parent, proposal.index)
            .map_err(display)?;
        let new_revision = revision();
        let request = SourceMomentInsertionRequest {
            expected_revision: proposal.id.base_revision.clone(),
            new_revision: new_revision.clone(),
            asset: proposal.asset.clone(),
            parent: proposal.parent.clone(),
            index: proposal.index,
            node: node(),
            label: format!(
                "{} [{}..{})",
                source.label, proposal.ordinals.start, proposal.ordinals.end
            ),
            timing: AudioTimingId {
                allocation: new_revision,
                ordinal: 0,
            },
            ordinals: proposal.ordinals.clone(),
        };
        let id = proposal.id.clone();
        let asset = proposal.asset.clone();
        let scope = proposal.scope.clone();
        self.splice_draft = Some(Draft {
            proposal,
            request,
            cursor,
            source: None,
            prepared: None,
        });
        if token.is_none()
            && self
                .cached
                .as_ref()
                .is_some_and(|(cached_asset, _)| cached_asset == &asset)
        {
            token = self.cached.take().map(|(_, token)| token);
        }
        if let Some(token) = token
            && self.prepare_splice_source(token)?
        {
            return Ok(());
        }
        if let Some(active) = &self.active {
            if active.splice.as_ref() == Some(&id) && !active.cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            return Err(
                "Slice preview needs source preparation; wait for the active import to stop".into(),
            );
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
        let import = self.import.clone();
        let message = self.message.clone();
        self.begin(
            PathBuf::from(&source.label),
            streams,
            None,
            None,
            scope,
            Work::Qualify { record, streams },
        )?;
        // This preparation does not register an asset. Reusing the import
        // completion channel would select a source and retarget the native view.
        self.import = import;
        self.message = message;
        self.active
            .as_mut()
            .ok_or("Slice preparation did not start")?
            .splice = Some(id);
        Ok(())
    }

    /// False means the cached media token needs fresh off-thread qualification.
    fn prepare_splice_source(&mut self, source: PreparedSourceRegistration) -> Result<bool> {
        let draft = self
            .splice_draft
            .as_ref()
            .ok_or("Slice proposal was abandoned")?;
        self.check_splice_context(&draft.proposal.id)?;
        if source.receipt().id() != &draft.proposal.qualification {
            return Err("Fresh source qualification differs from the copied Original".into());
        }
        let edit = self
            .store
            .as_ref()
            .ok_or("Open a project first")?
            .preview_prepared_source_moment(&draft.request, &source, &AtomicBool::new(false));
        let edit = match edit {
            Ok(edit) => edit,
            Err(StoreError::OriginalMedia(_) | StoreError::Io(_)) => return Ok(false),
            Err(error) => return Err(display(error)),
        };
        let prepared = self.build_splice_preview(draft, edit)?;
        let draft = self
            .splice_draft
            .as_mut()
            .ok_or("Slice proposal was abandoned")?;
        draft.source = Some(source);
        draft.prepared = Some(prepared.clone());
        self.splice = Some(ProposalUpdate {
            id: draft.proposal.id.clone(),
            result: Ok(prepared),
        });
        Ok(true)
    }

    fn build_splice_preview(
        &self,
        draft: &Draft,
        edit: EditTransaction,
    ) -> Result<Arc<PreparedSplice>> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let document = Arc::new(edit.forward.apply(&workspace.document).map_err(display)?);
        let NodeKind::Source { source } = &document.nodes()[&draft.request.node].kind else {
            return Err("Slice preview did not produce its Source beat".into());
        };
        let end = draft
            .cursor
            .0
            .checked_add(source.duration.frames())
            .ok_or("Slice range overflow")?;
        let range = FrameRange::new(draft.cursor, ProjectFrame(end)).map_err(display)?;
        let plan = Arc::new(RenderPlan::compile(&document).map_err(display)?);
        let snapshot = Arc::new(
            deadpan_playback::Snapshot::proposed(
                &workspace.playback_snapshot(),
                document,
                draft.proposal.id.draft,
                draft.proposal.id.change,
            )
            .map_err(display)?,
        );
        Ok(Arc::new(PreparedSplice {
            base: workspace.clone(),
            snapshot,
            plan,
            node: draft.request.node.clone(),
            range,
        }))
    }

    pub(super) fn splice_result(&mut self, active: Pending, reply: Reply) {
        let Some(id) = active.splice else {
            return;
        };
        if active.cancelled.load(Ordering::Acquire)
            || self
                .splice_draft
                .as_ref()
                .is_none_or(|draft| draft.proposal.id != id)
        {
            return;
        }
        let result = self
            .check_splice_context(&id)
            .and_then(|()| match reply.result {
                Ok(Prepared::Qualified(source)) => {
                    self.prepare_splice_source(*source).and_then(|ready| {
                        if ready {
                            Ok(())
                        } else {
                            Err(
                                "Original changed during slice preparation; refresh the proposal"
                                    .into(),
                            )
                        }
                    })
                }
                Ok(_) => Err("Import worker returned an unrelated slice preparation".into()),
                Err(error) => Err(error),
            });
        if let Err(error) = result {
            self.invalidate_splice(&error);
        }
    }

    pub(super) fn commit_splice_command(&mut self, id: ProposalId) -> Result<()> {
        // A retransmitted exact success observes its durable receipt, never a
        // second authored insertion, including after a failed workspace refresh.
        if self.workspace.as_ref().is_some_and(|workspace| {
            workspace.session == id.session && workspace.document.project_id() == &id.project
        }) && self.pending_session_change.is_none()
            && self
                .splice_commit
                .as_ref()
                .is_some_and(|update| update.id == id && update.result.is_ok())
        {
            return Ok(());
        }
        match self.commit_splice(&id) {
            Ok(committed) => {
                self.committed = Some(committed.clone());
                self.splice_commit = Some(SpliceCommitUpdate {
                    id,
                    result: Ok(committed),
                });
                #[cfg(test)]
                {
                    self.render_preview_refresh_failure = self
                        .shared
                        .splice_commit_refresh_failure
                        .swap(false, Ordering::AcqRel);
                }
                self.refresh()?;
                self.message = Some("Slice placed and saved. Undo with u.".into());
            }
            Err(error) => {
                self.splice_commit = Some(SpliceCommitUpdate {
                    id,
                    result: Err(error),
                });
            }
        }
        Ok(())
    }

    fn commit_splice(&mut self, id: &ProposalId) -> Result<CommittedEdit> {
        self.check_splice_context(id)?;
        let draft = self
            .splice_draft
            .as_ref()
            .ok_or("Slice proposal is no longer available")?;
        if &draft.proposal.id != id {
            return Err("Slice proposal was superseded; preview the latest placement".into());
        }
        if draft.prepared.is_none() || draft.source.is_none() {
            return Err("Slice preview is still preparing".into());
        }
        // Consume only the matching ready draft. Failure cannot make it eligible
        // for a later implicit retry, or let an older update retarget this commit.
        let mut draft = self
            .splice_draft
            .take()
            .ok_or("Slice proposal is no longer available")?;
        let source = draft
            .source
            .take()
            .ok_or("Slice source is no longer prepared")?;
        let outcome = self.writer()?.commit_prepared_source_moment(
            &draft.request,
            &source,
            None,
            &AtomicBool::new(false),
        );
        self.cache_splice_source(&draft.proposal.asset, Some(source));
        let commit = outcome.map_err(display)?;
        Ok(CommittedEdit {
            revision: commit.revision_id,
            selected_node: Some(draft.request.node),
            preserve_cursor: false,
            cursor: Some(draft.cursor),
            scope: draft.proposal.scope,
            sound: None,
        })
    }

    pub(super) fn abandon_splice(&mut self, id: &ProposalId) {
        if self
            .splice_draft
            .as_ref()
            .is_some_and(|draft| &draft.proposal.id == id)
        {
            self.invalidate_splice("Slice proposal was abandoned");
        }
    }

    pub(super) fn invalidate_changed_splice(&mut self) {
        if let Some(draft) = &self.splice_draft
            && self.check_splice_context(&draft.proposal.id).is_err()
        {
            self.invalidate_splice("Project changed; refresh the slice proposal");
        }
    }

    pub(super) fn invalidate_splice(&mut self, error: &str) {
        let Some(draft) = self.splice_draft.take() else {
            return;
        };
        if let Some(active) = &self.active
            && active.splice.as_ref() == Some(&draft.proposal.id)
        {
            active.cancelled.store(true, Ordering::Release);
        }
        self.cache_splice_source(&draft.proposal.asset, draft.source);
        self.splice = Some(ProposalUpdate {
            id: draft.proposal.id,
            result: Err(error.into()),
        });
    }

    fn cache_splice_source(&mut self, asset: &AssetId, source: Option<PreparedSourceRegistration>) {
        if let Some(source) = source {
            self.cached = Some((asset.clone(), source));
        }
    }

    pub(super) fn splice_worker_disconnected(&mut self, active: &Pending) {
        if active.splice.as_ref().is_some_and(|id| {
            self.splice_draft
                .as_ref()
                .is_some_and(|draft| &draft.proposal.id == id)
        }) {
            self.invalidate_splice("Import worker stopped before completing the slice preview");
        }
    }
}
