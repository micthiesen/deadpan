//! Retain one exact uncommitted placement on the project owner.

use deadpan_core::{EditTransaction, FrameRange, ProjectFrame};

use super::*;
use crate::project::splice::{
    Operation, Prepared as PreparedSplice, PreparedMedia, Proposal, ProposalId, ProposalUpdate,
    Source, SpliceCommitUpdate,
};

mod request;
pub(super) use request::Request;

pub(super) struct Draft {
    proposal: Proposal,
    request: Request,
    cursor: ProjectFrame,
    source: Option<PreparedSourceRegistration>,
    prepared: Option<Arc<PreparedSplice>>,
}

impl Service {
    pub(super) fn prepare_splice_command(&mut self, proposal: Proposal) {
        let id = proposal.id.clone();
        self.splice_source_view = None;
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
                source_view: self.splice_source_view.clone(),
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
        self.splice_source_view = None;
        let previous = self.splice_draft.take();
        let mut token = None;
        if let Some(previous) = previous {
            let same_source = same_original(&previous.proposal.source, &proposal.source);
            if same_source {
                token = previous.source;
            } else {
                self.cache_splice_source(previous.request.asset(), previous.source);
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
        // Source preparation is independent of destination validity. Its sealed
        // neutral view remains useful when a slot or endpoint cannot be placed.
        let edited = match &proposal.source {
            Source::Edited { copied, range } => Some(self.prepare_copied_view(copied, *range)?),
            Source::Original { .. } => None,
        };
        if proposal.operation == Operation::Move && edited.is_none() {
            return Err("Original always copies; Move requires a current edited selection".into());
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let view = proposal.scope.resolve(workspace)?;
        if view.owner != &proposal.parent {
            return Err("Slice destination is outside the captured Sequence scope".into());
        }
        if let Some(slice) = edited {
            let (request, cursor) = match proposal.operation {
                Operation::Copy => {
                    Request::edited(workspace, &proposal.parent, &proposal.destination, &slice)?
                }
                Operation::Move => Request::move_edited(
                    workspace,
                    &proposal.parent,
                    &proposal.destination,
                    &slice,
                )?,
            };
            let draft = Draft {
                proposal,
                request,
                cursor,
                source: None,
                prepared: None,
            };
            let store = self.store.as_ref().ok_or("Open a project first")?;
            let command = draft
                .request
                .edited_command()
                .ok_or("Missing edited placement command")?;
            let media =
                self.slice_media_view(store.preview_edit_slice(command).map_err(display)?)?;
            let snapshot = Arc::new(
                deadpan_playback::Snapshot::proposed_edit_slice(
                    &workspace.playback_snapshot(),
                    media.admitted().clone(),
                    draft.proposal.id.draft,
                    draft.proposal.id.change,
                )
                .map_err(display)?,
            );
            let prepared =
                self.finish_splice_preview(&draft, snapshot, PreparedMedia::Edited(media))?;
            self.splice = Some(ProposalUpdate {
                id: draft.proposal.id.clone(),
                source_view: self.splice_source_view.clone(),
                result: Ok(prepared.clone()),
            });
            self.splice_draft = Some(Draft {
                prepared: Some(prepared),
                ..draft
            });
            return Ok(());
        }
        let Source::Original {
            asset,
            qualification,
            ordinals,
        } = &proposal.source
        else {
            return Err("Copied Edit was not prepared".into());
        };
        if ordinals.start >= ordinals.end {
            return Err("Select a nonempty Original slice".into());
        }
        let source = workspace
            .sources
            .get(asset)
            .cloned()
            .ok_or("Copied Original is no longer registered")?;
        if source.receipt.id() != qualification || source.video_index.is_none() {
            return Err("Copied Original qualification has changed; select and copy again".into());
        }
        let (request, cursor) = Request::capture(
            workspace,
            &proposal.parent,
            &proposal.destination,
            asset,
            ordinals.clone(),
            &source.label,
        )?;
        let id = proposal.id.clone();
        let asset = asset.clone();
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
            interpretation: source.receipt.snapshot().audio_interpretation(),
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
        let Source::Original { qualification, .. } = &draft.proposal.source else {
            return Err("Original preparation cannot replace an edited slice".into());
        };
        if source.receipt().id() != qualification {
            return Err("Fresh source qualification differs from the copied Original".into());
        }
        let store = self.store.as_ref().ok_or("Open a project first")?;
        let cancelled = AtomicBool::new(false);
        let edit = draft.request.preview(store, &source, &cancelled);
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
            source_view: None,
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
        let snapshot = Arc::new(
            deadpan_playback::Snapshot::proposed(
                &workspace.playback_snapshot(),
                document,
                draft.proposal.id.draft,
                draft.proposal.id.change,
            )
            .map_err(display)?,
        );
        self.finish_splice_preview(draft, snapshot, PreparedMedia::Original)
    }

    fn finish_splice_preview(
        &self,
        draft: &Draft,
        snapshot: Arc<deadpan_playback::Snapshot>,
        media: PreparedMedia,
    ) -> Result<Arc<PreparedSplice>> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let plan = Arc::new(RenderPlan::compile(&snapshot.document).map_err(display)?);
        let object = draft
            .proposal
            .destination
            .object_target(&workspace.document, &draft.proposal.parent)?;
        let result_parent = object
            .as_ref()
            .map_or(&draft.proposal.parent, |object| &object.parent);
        let duration = match draft.request.inserted_duration() {
            Some(duration) => duration,
            None => plan
                .node_duration(
                    draft
                        .request
                        .node()
                        .ok_or("Slice request has no inserted root")?,
                )
                .ok_or("Slice preview did not produce its inserted root")?,
        };
        let end = draft
            .cursor
            .0
            .checked_add(duration.frames())
            .ok_or("Slice range overflow")?;
        let range = FrameRange::new(draft.cursor, ProjectFrame(end)).map_err(display)?;
        let result_slot = object.as_ref().map(|object| object.slot);
        let empty_slot = if duration == deadpan_core::FrameDuration::ZERO {
            match &draft.proposal.destination {
                crate::project::splice::Destination::Slot(index) => Some(*index),
                crate::project::splice::Destination::Object { .. } => result_slot,
                _ => return Err("Empty groups need an exact Sequence slot or group object".into()),
            }
        } else {
            None
        };
        let node = if let Some(slot) = result_slot.or(empty_slot) {
            snapshot
                .document
                .children(result_parent)
                .nth(slot)
                .cloned()
                .ok_or("Slice result slot has no inserted root")?
        } else {
            crate::project::splice::result_forest_first(
                &snapshot.document,
                &plan,
                result_parent,
                range,
            )?
        };
        if draft
            .request
            .node()
            .is_some_and(|expected| expected != &node)
        {
            return Err("Slice preview changed its inserted root".into());
        }
        let continuation_scope = if object.as_ref().is_some_and(|object| !object.inner_outside) {
            SequenceScope::from_historical_parent(&snapshot.document, &plan, result_parent)?
        } else {
            draft.proposal.scope.clone()
        };
        let continuation_node = object
            .as_ref()
            .filter(|object| object.inner_outside)
            .map_or_else(|| node.clone(), |object| object.selected_group.clone());
        let prepared = PreparedSplice {
            base: workspace.clone(),
            navigation_parent: draft.proposal.parent.clone(),
            snapshot,
            media,
            plan,
            node,
            parent: result_parent.clone(),
            result_slot,
            range,
            empty_slot,
            removed: draft.request.removed(),
            movement: draft.request.movement().cloned(),
            continuation_scope,
            continuation_node,
            object: match &draft.proposal.destination {
                crate::project::splice::Destination::Object { selection } => {
                    Some(selection.clone())
                }
                _ => None,
            },
        };
        prepared.validate_result()?;
        Ok(Arc::new(prepared))
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
                self.message = Some(match self.refresh() {
                    Ok(()) => "Slice placed and saved. Undo with u.".into(),
                    Err(error) => format!(
                        "Slice saved, but the preview could not refresh: {error}. Reopen the project to view the saved edit."
                    ),
                });
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
        if draft.prepared.is_none() || (draft.request.asset().is_some() && draft.source.is_none()) {
            return Err("Slice preview is still preparing".into());
        }
        // Consume only the matching ready draft. Failure cannot make it eligible
        // for a later implicit retry, or let an older update retarget this commit.
        let mut draft = self
            .splice_draft
            .take()
            .ok_or("Slice proposal is no longer available")?;
        let source = draft.source.take();
        let prepared = draft
            .prepared
            .take()
            .ok_or("Slice preview is still preparing")?;
        prepared.validate_result()?;
        let range_selection = prepared.movement.as_ref().map(|_| CommittedRangeSelection {
            session: id.session,
            project: id.project.clone(),
            parent: prepared.parent.clone(),
            range: prepared.range,
        });
        let store = self.writer()?;
        let cancelled = AtomicBool::new(false);
        let outcome = draft.request.commit(store, source.as_ref(), &cancelled);
        self.cache_splice_source(draft.request.asset(), source);
        let commit = outcome.map_err(display)?;
        self.capture_preparation_notices(&commit.generation_preparation_notices);
        Ok(CommittedEdit {
            scoped: None,
            revision: commit.revision_id,
            selected_node: Some(prepared.continuation_node.clone()),
            preserve_cursor: false,
            cursor: Some(draft.cursor),
            scope: prepared.continuation_scope.clone(),
            sound: None,
            range_selection,
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
        self.cache_splice_source(draft.request.asset(), draft.source);
        self.splice = Some(ProposalUpdate {
            id: draft.proposal.id,
            source_view: self.splice_source_view.clone(),
            result: Err(error.into()),
        });
    }

    fn cache_splice_source(
        &mut self,
        asset: Option<&AssetId>,
        source: Option<PreparedSourceRegistration>,
    ) {
        if let (Some(asset), Some(source)) = (asset, source) {
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

fn same_original(left: &Source, right: &Source) -> bool {
    matches!((left, right),
        (Source::Original { asset: left, qualification: left_qualification, .. },
         Source::Original { asset: right, qualification: right_qualification, .. })
        if left == right && left_qualification == right_qualification)
}
