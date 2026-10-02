//! An Edit selection owns one exact revision and ordinary Sequence scope.

use deadpan_core::{FrameRange, ProjectId};

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    session: u64,
    project: ProjectId,
    revision: RevisionId,
    scope: SequenceScope,
    parent: NodeId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Selection {
    identity: Option<Identity>,
    bounds: Option<(u64, u64)>,
    pub active: bool,
}

impl Selection {
    /// A typed mark-only receipt proves the timeline and this scope unchanged.
    /// Never rebase through an unrelated edit or a stale visible workspace.
    pub(super) fn rebase_mark(&mut self, saved: &crate::project::marks::Saved) {
        if let Some(identity) = &mut self.identity
            && identity.session == saved.id.session
            && identity.project == saved.id.project
            && identity.revision == saved.id.revision
        {
            identity.revision = saved.revision.clone();
        }
    }
    fn reconcile(&mut self, identity: Option<Identity>) {
        if self.identity != identity {
            self.clear();
            self.identity = identity;
        }
    }

    fn toggle(&mut self, at: u64) {
        if self.active {
            self.active = false;
        } else {
            self.bounds = Some((at, at));
            self.active = true;
        }
    }

    pub fn move_to(&mut self, at: u64) {
        if self.active
            && let Some((_, head)) = &mut self.bounds
        {
            *head = at;
        }
    }

    fn range(&self) -> Option<FrameRange> {
        let (anchor, head) = self.bounds?;
        if anchor == head {
            return None;
        }
        FrameRange::new(
            ProjectFrame(i64::try_from(anchor.min(head)).ok()?),
            ProjectFrame(i64::try_from(anchor.max(head)).ok()?),
        )
        .ok()
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.bounds = None;
    }
}

impl DeadpanApp {
    pub(super) fn select_committed_range(
        &mut self,
        range: &crate::project::CommittedRangeSelection,
    ) {
        let Some(identity) = self.edit_range_identity() else {
            return;
        };
        if identity.session != range.session
            || identity.project != range.project
            || identity.parent != range.parent
            || self.last_committed.as_ref() != Some(&identity.revision)
            || range.range.start().0 < self.scope_start as i64
            || range.range.end().0 > self.scope_end as i64
        {
            return;
        }
        self.edit_range.reconcile(Some(identity));
        self.edit_range.bounds = Some((range.range.start().0 as u64, range.range.end().0 as u64));
        self.edit_range.active = false;
    }

    fn edit_range_identity(&self) -> Option<Identity> {
        let workspace = self.workspace.as_ref()?;
        let scope = self.sequence_scope.resolve(workspace).ok()?;
        Some(Identity {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            revision: workspace.document.revision_id().clone(),
            scope: self.sequence_scope.clone(),
            parent: scope.owner.clone(),
        })
    }

    pub(super) fn reconcile_edit_range(&mut self) {
        self.edit_range.reconcile(self.edit_range_identity());
        if self.view != View::Sequence {
            self.edit_range.active = false;
        }
    }

    pub(super) fn selected_edit_range(&self) -> Option<FrameRange> {
        if self.edit_range.identity != self.edit_range_identity() {
            return None;
        }
        self.edit_range.range()
    }

    pub(super) fn edit_selection(&self) -> navigation::EditSelection {
        if self.edit_range.identity != self.edit_range_identity()
            || self.edit_range.bounds.is_none()
        {
            navigation::EditSelection::None
        } else if self.edit_range.range().is_some() {
            navigation::EditSelection::Range
        } else {
            navigation::EditSelection::Empty
        }
    }

    pub(super) fn routed_edit_selection(&self) -> navigation::EditSelection {
        if self.view == View::Sequence
            && !self.sound_focused()
            && self.pane != Pane::Sounds
            && !self.event_focused()
        {
            self.edit_selection()
        } else {
            navigation::EditSelection::None
        }
    }

    pub(super) fn visual_edit_range(&mut self) {
        self.bindings.clear();
        self.reconcile_edit_range();
        if self.edit_range.identity.is_none() {
            self.error = Some("Open an edit before selecting time.".into());
            return;
        }
        if !(self.scope_start..=self.scope_end).contains(&self.sequence_cursor) {
            self.error = Some("Move into the displayed group before selecting time.".into());
            return;
        }
        self.pause_playback();
        self.edit_range.toggle(self.sequence_cursor);
        self.error = None;
        self.message = Some(if self.edit_range.active {
            format!(
                "{} and {} extend the Edit range; {} copies; {} cuts; {} finishes; {} clears.",
                self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/"),
                self.editor_pair(EditorKey::BeatNext, EditorKey::BeatPrevious, "/"),
                self.editor_key(EditorKey::Copy),
                self.editor_key(EditorKey::CutRange),
                self.editor_key(EditorKey::Visual),
                self.editor_key(EditorKey::Escape)
            )
        } else if self.selected_edit_range().is_some() {
            format!(
                "Edit range retained. {} copies; {} cuts; :splice previews replacement; {} replaces now.",
                self.editor_key(EditorKey::Copy),
                self.editor_key(EditorKey::CutRange),
                self.editor_pair(EditorKey::PasteAfter, EditorKey::PasteBefore, "/")
            )
        } else {
            format!(
                "Empty Edit range. Press {} and move to select time.",
                self.editor_key(EditorKey::Visual)
            )
        });
    }

    pub(super) fn edit_range_label(&self) -> Option<String> {
        if self.edit_selection() == navigation::EditSelection::Empty {
            return Some(format!(
                "Edit range empty · move to select time · {} clears",
                self.editor_key(EditorKey::Escape)
            ));
        }
        self.selected_edit_range().map(|range| {
            format!(
                "Edit [{}..{}) · {} f · {}",
                range.start().0,
                range.end().0,
                range.end().0 - range.start().0,
                if self.edit_range.active {
                    "extending"
                } else {
                    "selected"
                },
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(revision: &str, scope: SequenceScope) -> Identity {
        Identity {
            session: 1,
            project: ProjectId::new("project").unwrap(),
            revision: RevisionId::new(revision).unwrap(),
            scope,
            parent: NodeId::new("sequence").unwrap(),
        }
    }

    #[test]
    fn reverse_range_is_half_open_and_finished_range_does_not_follow_cursor() {
        let mut selection = Selection::default();
        selection.reconcile(Some(identity("a", SequenceScope::default())));
        selection.toggle(40);
        assert_eq!(selection.range(), None);
        selection.move_to(10);
        let expected = FrameRange::new(ProjectFrame(10), ProjectFrame(40)).unwrap();
        assert_eq!(selection.range(), Some(expected));
        selection.toggle(10);
        selection.move_to(90);
        assert_eq!(selection.range(), Some(expected));
        selection.clear();
        assert_eq!(selection.range(), None);
    }

    #[test]
    fn revision_session_project_scope_or_owner_change_discards_range() {
        let original = identity("a", SequenceScope::default());
        for changed in [
            Identity {
                revision: RevisionId::new("b").unwrap(),
                ..original.clone()
            },
            Identity {
                session: 2,
                ..original.clone()
            },
            Identity {
                project: ProjectId::new("other").unwrap(),
                ..original.clone()
            },
            Identity {
                scope: SequenceScope::test_path(vec![NodeId::new("child").unwrap()]),
                ..original.clone()
            },
            Identity {
                parent: NodeId::new("other").unwrap(),
                ..original.clone()
            },
        ] {
            let mut selection = Selection::default();
            selection.reconcile(Some(original.clone()));
            selection.toggle(10);
            selection.move_to(40);
            selection.reconcile(Some(changed));
            assert_eq!(selection.range(), None);
            assert!(!selection.active);
        }
    }
}
