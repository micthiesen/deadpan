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
                if range.duration() == deadpan_core::FrameDuration::ZERO {
                    return format!(
                        "Copied empty group ‘{}’ · 0 frames · structure only",
                        copied.child_label().unwrap_or("Group")
                    );
                }
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
    intent: Intent,
}

enum Intent {
    Yank(edit_range::Selection),
    Cut,
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
        self.pending = Some(Pending {
            request,
            intent: Intent::Yank(selection),
        });
    }

    pub fn expect_cut(&mut self, request: slice::CaptureRequest) {
        self.pending = Some(Pending {
            request,
            intent: Intent::Cut,
        });
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
        if self.pending.as_ref().is_none_or(|pending| {
            pending.request.id != update.id || !matches!(pending.intent, Intent::Yank(_))
        }) {
            return None;
        }
        let pending = self.pending.take().expect("matched copy request");
        let result = update
            .result
            .and_then(|copied| self.accept(&pending.request, copied));
        let Intent::Yank(selection) = pending.intent else {
            unreachable!("matched yank intent")
        };
        Some((selection, result))
    }

    pub fn receive_cut(
        &mut self,
        update: slice::CutUpdate,
    ) -> Option<Result<slice::CutReceipt, String>> {
        if self.pending.as_ref().is_none_or(|pending| {
            pending.request != update.request || !matches!(pending.intent, Intent::Cut)
        }) {
            return None;
        }
        self.pending = None;
        Some(update.result.and_then(|receipt| {
            self.accept(&update.request, receipt.copied.clone())?;
            Ok(receipt)
        }))
    }

    fn accept(
        &mut self,
        request: &slice::CaptureRequest,
        copied: Arc<slice::Captured>,
    ) -> Result<(), String> {
        if copied.id() != &request.id
            || copied.scope() != &request.scope
            || copied.slice().project_id() != &request.id.project
            || copied.slice().revision_id() != &request.id.source_revision
            || copied.slice().parent() != &request.parent
            || copied.slice().selection() != &request.selection
            || copied.bounds().start() > copied.slice().range().start()
            || copied.bounds().end() < copied.slice().range().end()
        {
            return Err("Copy preparation returned a different Edit selection.".into());
        }
        self.content = Some(Content::Edited(copied));
        Ok(())
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
            let view = self.sequence_scope.resolve(workspace)?;
            let selection = match self.edit_selection() {
                navigation::EditSelection::Empty => {
                    return Err("The Edit selection is empty. Extend it before copying.".into());
                }
                navigation::EditSelection::Range => deadpan_core::SliceCaptureSelection::Range {
                    range: self
                        .selected_edit_range()
                        .ok_or("The Edit selection changed; select it again.")?,
                },
                navigation::EditSelection::None => deadpan_core::SliceCaptureSelection::Child {
                    node: self
                        .selected_beat
                        .as_ref()
                        .filter(|node| view.children.contains(node))
                        .cloned()
                        .ok_or("Select a beat or an Edit range to copy.")?,
                },
            };
            let parent = view.owner.clone();
            Ok::<_, String>((
                workspace.session,
                workspace.document.project_id().clone(),
                workspace.document.revision_id().clone(),
                parent,
                selection,
            ))
        })();
        let (session, project, source_revision, parent, selection) = match captured {
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
            selection,
        };
        match self
            .service
            .submit(ProjectRequest::CaptureEditSlice(request.clone()))
        {
            Ok(()) => {
                self.copied.expect(request, self.edit_range.clone());
                self.error = None;
                self.message = Some("Copying the Edit selection…".into());
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

    pub(super) fn receive_cut(&mut self, update: Option<slice::CutUpdate>) {
        self.copied.reconcile(self.workspace.as_deref());
        let Some(result) = update.and_then(|update| self.copied.receive_cut(update)) else {
            return;
        };
        match result {
            Ok(receipt) => {
                if receipt.needs_refresh(self.workspace.as_deref())
                    && let Some(message) = receipt.refresh_error
                {
                    self.message = Some(message);
                } else if self.workspace.as_ref().is_some_and(|workspace| {
                    workspace.document.revision_id() == &receipt.committed.revision
                }) {
                    let range = receipt.copied.slice().range();
                    self.message = Some(format!(
                        "Cut saved and copied: Edit [{}..{}) · {} f. p/P pastes; :splice previews placement. Undo with u.",
                        range.start().0,
                        range.end().0,
                        range.duration().frames(),
                    ));
                }
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
            selection: deadpan_core::SliceCaptureSelection::Range {
                range: FrameRange::new(ProjectFrame(10), ProjectFrame(24)).unwrap(),
            },
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

    #[test]
    fn cut_requires_its_full_request_and_cannot_be_completed_by_a_yank() {
        let mut register = Register::default();
        register.original_copied(original());
        let pending = request(1);
        register.expect_cut(pending.clone());
        assert!(register.receive(failure(&pending)).is_none());
        let mut changed = pending.clone();
        changed.selection = deadpan_core::SliceCaptureSelection::Child {
            node: NodeId::new("different-child").unwrap(),
        };
        assert!(
            register
                .receive_cut(slice::CutUpdate {
                    request: changed,
                    result: Err("different cut".into()),
                })
                .is_none()
        );
        assert!(register.is_pending());
        let result = register
            .receive_cut(slice::CutUpdate {
                request: pending.clone(),
                result: Err("capture failed before deletion".into()),
            })
            .unwrap();
        assert_eq!(result.unwrap_err(), "capture failed before deletion");
        assert_eq!(register.original().unwrap().ordinals, 2..8);
        assert!(!register.is_pending());
        assert!(
            register
                .receive_cut(slice::CutUpdate {
                    request: pending,
                    result: Err("duplicate".into()),
                })
                .is_none()
        );
    }

    #[test]
    fn newer_yank_cut_and_local_rejection_supersede_pending_cut_register_intent() {
        let mut register = Register::default();
        register.original_copied(original());
        let old = request(1);
        let next = request(2);
        let late = || slice::CutUpdate {
            request: old.clone(),
            result: Err("late cut".into()),
        };
        register.expect_cut(old.clone());
        register.expect(next.clone(), edit_range::Selection::default());
        assert!(register.receive_cut(late()).is_none());
        assert!(register.receive(failure(&next)).is_some());
        register.expect_cut(old.clone());
        register.expect_cut(next);
        assert!(register.receive_cut(late()).is_none());
        register.supersede();
        assert!(register.receive_cut(late()).is_none());
        assert_eq!(register.original().unwrap().ordinals, 2..8);
    }
}
