//! Copied contents survive navigation; pending yanks belong to one exact intent.

use super::*;
use crate::project::slice;

#[derive(Clone, Debug)]
pub(super) enum Content {
    Original(moment::Copied),
    Edited(Arc<slice::Captured>),
}

impl Content {
    pub fn label(&self) -> String {
        match self {
            Self::Original(copied) => format!(
                "Copied Original [{}..{})",
                copied.ordinals.start, copied.ordinals.end
            ),
            Self::Edited(copied) => {
                let range = copied.slice().range();
                format!(
                    "Copied Edit [{}..{}) · {} frames",
                    range.start().0,
                    range.end().0,
                    range.duration().frames()
                )
            }
        }
    }

    pub fn source(&self) -> crate::project::splice::Source {
        match self {
            Self::Original(copied) => crate::project::splice::Source::Original {
                asset: copied.identity.asset.clone(),
                qualification: copied.identity.qualification.clone(),
                ordinals: copied.ordinals.clone(),
            },
            Self::Edited(copied) => crate::project::splice::Source::Edited {
                copied: copied.clone(),
                range: copied.slice().range(),
            },
        }
    }

    pub fn check(&self, workspace: &Workspace) -> Result<(), String> {
        match self {
            Self::Original(copied) => {
                if copied.identity.session != workspace.session
                    || workspace
                        .sources
                        .get(&copied.identity.asset)
                        .is_none_or(|source| source.receipt.id() != &copied.identity.qualification)
                {
                    return Err("The captured Original slice is no longer available.".into());
                }
            }
            Self::Edited(copied) => {
                if copied.id().session != workspace.session
                    || &copied.id().project != workspace.document.project_id()
                {
                    return Err("The copied edit belongs to another project session.".into());
                }
                if copied.slice().presentation_basis() != workspace.document.presentation_basis() {
                    return Err(
                        "The copied edit uses a different canvas or frame rate. Copy it again."
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }
}

struct Pending {
    request: slice::CaptureRequest,
    selection: edit_range::Selection,
}

#[derive(Default)]
pub(super) struct Register {
    content: Option<Content>,
    pending: Option<Pending>,
}

impl Register {
    pub fn content(&self) -> Option<&Content> {
        self.content.as_ref()
    }

    pub fn original(&self) -> Option<&moment::Copied> {
        match self.content.as_ref()? {
            Content::Original(copied) => Some(copied),
            Content::Edited(_) => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Even a rejected newer yank must not be completed by an older reply.
    pub fn supersede(&mut self) {
        self.pending = None;
    }

    pub fn original_copied(&mut self, copied: moment::Copied) {
        self.supersede();
        self.content = Some(Content::Original(copied));
    }

    pub fn expect(&mut self, request: slice::CaptureRequest, selection: edit_range::Selection) {
        self.pending = Some(Pending { request, selection });
    }

    pub fn reconcile(&mut self, workspace: Option<&Workspace>) {
        if self.content.as_ref().is_some_and(|content| match content {
            Content::Original(_) => {
                workspace.is_none_or(|workspace| content.check(workspace).is_err())
            }
            Content::Edited(copied) => workspace.is_none_or(|workspace| {
                workspace.session != copied.id().session
                    || workspace.document.project_id() != &copied.id().project
            }),
        }) {
            self.content = None;
        }
        if self.pending.as_ref().is_some_and(|pending| {
            workspace.is_none_or(|workspace| {
                workspace.session != pending.request.id.session
                    || workspace.document.project_id() != &pending.request.id.project
            })
        }) {
            self.pending = None;
        }
    }

    pub fn receive(
        &mut self,
        update: slice::CaptureUpdate,
    ) -> Option<(edit_range::Selection, Result<(), String>)> {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| pending.request.id != update.id)
        {
            return None;
        }
        let pending = self.pending.take().expect("matched copy request");
        let result = update.result.and_then(|copied| {
            let request = &pending.request;
            if copied.id() != &request.id
                || copied.scope() != &request.scope
                || copied.slice().project_id() != &request.id.project
                || copied.slice().revision_id() != &request.id.source_revision
                || copied.slice().parent() != &request.parent
                || copied.slice().range() != request.range
                || copied.bounds().start() > request.range.start()
                || copied.bounds().end() < request.range.end()
            {
                return Err("Copy preparation returned a different Edit range.".into());
            }
            self.content = Some(Content::Edited(copied));
            Ok(())
        });
        Some((pending.selection, result))
    }

    pub fn copied_audio_selection(&self) -> Option<crate::project::RoomToneSelection> {
        let copied = self.original()?;
        Some(crate::project::RoomToneSelection::Original {
            asset: copied.identity.asset.clone(),
            qualification: copied.identity.qualification.clone(),
            ordinals: copied.ordinals.clone(),
        })
    }
}

impl DeadpanApp {
    pub(super) fn copy_slice(&mut self) {
        self.copied.supersede();
        if self.view == View::Source {
            self.copy_moment();
            return;
        }
        self.bindings.clear();
        self.reconcile_edit_range();
        let captured = (|| {
            if self.sound_focused() || self.pane == Pane::Sounds || self.event_focused() {
                return Err("Select picture time in Your edit before copying a slice.".into());
            }
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let range = self
                .selected_edit_range()
                .ok_or("Select a nonempty Edit range: v, then h/l, then y.")?;
            let parent = self.sequence_scope.resolve(workspace)?.owner.clone();
            Ok::<_, String>((
                workspace.session,
                workspace.document.project_id().clone(),
                workspace.document.revision_id().clone(),
                parent,
                range,
            ))
        })();
        let (session, project, source_revision, parent, range) = match captured {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(request) = self.next_serial() else {
            return;
        };
        let request = slice::CaptureRequest {
            id: slice::CopyId {
                session,
                project,
                source_revision,
                request,
            },
            scope: self.sequence_scope.clone(),
            parent,
            range,
        };
        match self
            .service
            .submit(ProjectRequest::CaptureEditSlice(request.clone()))
        {
            Ok(()) => {
                self.copied.expect(request, self.edit_range.clone());
                self.error = None;
                self.message = Some("Copying the selected Edit range…".into());
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn receive_copied(
        &mut self,
        update: Option<slice::CaptureUpdate>,
        unrefreshed_commit: bool,
    ) {
        self.copied.reconcile(self.workspace.as_deref());
        let Some((selection, result)) = update.and_then(|update| self.copied.receive(update))
        else {
            return;
        };
        match result {
            Ok(()) => {
                // A coalesced yank completion must not consume a stale view's
                // selection or replace its saved-edit reopening guidance.
                if unrefreshed_commit {
                    return;
                }
                if self.edit_range == selection {
                    self.edit_range.active = false;
                }
                self.error = None;
                self.message = Some(
                    "Edit slice copied. :splice previews placement; p/P pastes or replaces an Edit selection."
                        .into(),
                );
            }
            Err(error) => self.error = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{FrameRange, ProjectId, SourceQualificationId};

    fn request(serial: u64) -> slice::CaptureRequest {
        slice::CaptureRequest {
            id: slice::CopyId {
                session: 7,
                project: ProjectId::new("copy-project").unwrap(),
                source_revision: RevisionId::new("copy-revision").unwrap(),
                request: serial,
            },
            scope: SequenceScope::default(),
            parent: NodeId::new("copy-root").unwrap(),
            range: FrameRange::new(ProjectFrame(10), ProjectFrame(24)).unwrap(),
        }
    }

    fn original() -> moment::Copied {
        moment::Copied {
            identity: moment::Identity {
                session: 7,
                asset: AssetId::new("original").unwrap(),
                qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
            },
            ordinals: 2..8,
        }
    }

    fn failure(request: &slice::CaptureRequest) -> slice::CaptureUpdate {
        slice::CaptureUpdate {
            id: request.id.clone(),
            result: Err("copy failed".into()),
        }
    }

    #[test]
    fn stale_and_duplicate_replies_cannot_complete_a_newer_yank() {
        let mut register = Register::default();
        register.original_copied(original());
        let first = request(1);
        let next = request(2);
        register.expect(first.clone(), edit_range::Selection::default());
        register.expect(next.clone(), edit_range::Selection::default());
        assert!(register.receive(failure(&first)).is_none());
        assert!(register.is_pending());
        assert_eq!(register.original().unwrap().ordinals, 2..8);
        let (_, result) = register.receive(failure(&next)).unwrap();
        assert_eq!(result, Err("copy failed".into()));
        assert!(!register.is_pending());
        assert_eq!(register.original().unwrap().ordinals, 2..8);
        assert!(register.receive(failure(&next)).is_none());
    }

    #[test]
    fn new_original_or_rejected_yank_supersedes_pending_edited_copy() {
        let mut register = Register::default();
        let pending = request(1);
        register.expect(pending.clone(), edit_range::Selection::default());
        register.original_copied(original());
        assert!(register.receive(failure(&pending)).is_none());
        assert_eq!(register.original().unwrap().ordinals, 2..8);
        register.expect(pending.clone(), edit_range::Selection::default());
        register.supersede();
        assert!(register.receive(failure(&pending)).is_none());
        assert_eq!(register.original().unwrap().ordinals, 2..8);
    }

    #[test]
    fn closing_project_discards_register_and_pending_reply() {
        let mut register = Register::default();
        register.original_copied(original());
        let pending = request(1);
        register.expect(pending.clone(), edit_range::Selection::default());
        register.reconcile(None);
        assert!(register.content().is_none());
        assert!(!register.is_pending());
        assert!(register.receive(failure(&pending)).is_none());
    }
}
