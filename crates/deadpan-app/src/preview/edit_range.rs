//! An Edit selection owns one exact revision and ordinary Sequence scope.

use deadpan_core::{FrameRange, ProjectId, SemanticVisualSelection};

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
    pub(super) fn has_bounds(&self) -> bool {
        self.bounds.is_some()
    }

    pub(super) fn semantic(&self) -> Result<Option<SemanticVisualSelection>, String> {
        self.bounds
            .map(|(anchor, head)| {
                Ok(SemanticVisualSelection {
                    anchor: ProjectFrame(i64::try_from(anchor).map_err(|error| error.to_string())?),
                    head: ProjectFrame(i64::try_from(head).map_err(|error| error.to_string())?),
                    extending: self.active,
                })
            })
            .transpose()
    }

    fn restore_semantic(
        &mut self,
        selection: Option<SemanticVisualSelection>,
        start: u64,
        end: u64,
    ) -> Result<(), String> {
        let bounds = selection
            .as_ref()
            .map(|selection| {
                let anchor =
                    u64::try_from(selection.anchor.0).map_err(|error| error.to_string())?;
                let head = u64::try_from(selection.head.0).map_err(|error| error.to_string())?;
                if !(start..=end).contains(&anchor) || !(start..=end).contains(&head) {
                    return Err("The macro Visual selection is outside this group.".to_owned());
                }
                Ok((anchor, head))
            })
            .transpose()?;
        self.bounds = bounds;
        self.active = selection.is_some_and(|selection| selection.extending);
        Ok(())
    }

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
    pub(super) fn capture_visual_selection(
        &self,
    ) -> Result<Option<SemanticVisualSelection>, String> {
        if self.edit_range.has_bounds() && self.edit_range.identity != self.edit_range_identity() {
            return Err("The Edit selection belongs to an earlier editing context.".into());
        }
        let selection = self.edit_range.semantic()?;
        if selection.as_ref().is_some_and(|selection| {
            selection.extending
                && u64::try_from(selection.head.0).ok() != Some(self.sequence_cursor)
        }) {
            return Err("The extending Edit selection no longer ends at the cursor.".into());
        }
        let mut checked = Selection::default();
        checked.restore_semantic(selection.clone(), self.scope_start, self.scope_end)?;
        Ok(selection)
    }

    pub(super) fn restore_macro_visual_selection(
        &mut self,
        selection: Option<SemanticVisualSelection>,
    ) -> Result<(), String> {
        let identity = self
            .edit_range_identity()
            .ok_or("The macro edit is no longer visible.")?;
        let mut restored = Selection::default();
        restored.restore_semantic(selection, self.scope_start, self.scope_end)?;
        restored.identity = Some(identity);
        self.edit_range = restored;
        Ok(())
    }

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
        if !self.macro_action_allowed(Action::VisualMoment) {
            return;
        }
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
        let instruction = if self.edit_range.active {
            deadpan_core::SemanticInstruction::FinishSelection
        } else {
            deadpan_core::SemanticInstruction::BeginSelection
        };
        self.edit_range.toggle(self.sequence_cursor);
        self.record_macro_local(instruction);
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
    fn semantic_selection_round_trip_preserves_absence_empty_direction_and_extension() {
        for selection in [
            None,
            Some(SemanticVisualSelection {
                anchor: ProjectFrame(10),
                head: ProjectFrame(10),
                extending: true,
            }),
            Some(SemanticVisualSelection {
                anchor: ProjectFrame(10),
                head: ProjectFrame(10),
                extending: false,
            }),
            Some(SemanticVisualSelection {
                anchor: ProjectFrame(40),
                head: ProjectFrame(10),
                extending: true,
            }),
            Some(SemanticVisualSelection {
                anchor: ProjectFrame(40),
                head: ProjectFrame(10),
                extending: false,
            }),
        ] {
            let mut native = Selection::default();
            native.restore_semantic(selection.clone(), 10, 40).unwrap();
            assert_eq!(native.semantic().unwrap(), selection);
            assert_eq!(native.has_bounds(), selection.is_some());
            native.move_to(20);
            let mut expected = selection;
            if let Some(selection) = &mut expected
                && selection.extending
            {
                selection.head = ProjectFrame(20);
            }
            assert_eq!(native.semantic().unwrap(), expected);
        }
    }

    #[test]
    fn invalid_semantic_selection_cannot_replace_retained_native_selection() {
        let mut native = Selection::default();
        native.toggle(20);
        native.move_to(30);
        native.toggle(30);
        let retained = native.clone();
        for (anchor, head) in [(-1, 20), (20, -1), (9, 20), (20, 41)] {
            assert!(
                native
                    .restore_semantic(
                        Some(SemanticVisualSelection {
                            anchor: ProjectFrame(anchor),
                            head: ProjectFrame(head),
                            extending: true,
                        }),
                        10,
                        40
                    )
                    .is_err()
            );
            assert_eq!(native, retained);
        }
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
