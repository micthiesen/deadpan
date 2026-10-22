//! An Edit selection owns one exact revision and ordinary Sequence scope.

use deadpan_core::{FrameRange, ProjectId, SemanticObjectSelection, SemanticVisualSelection};

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
    kind: Option<SelectionKind>,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SelectionKind {
    Time {
        anchor: u64,
        head: u64,
    },
    // Geometry is checked against the immutable workspace on capture/restore.
    // Authored operations use the object identity, never this display range.
    Object {
        selection: SemanticObjectSelection,
        range: FrameRange,
    },
    Node {
        node: NodeId,
        range: FrameRange,
    },
    Nodes {
        nodes: Vec<NodeId>,
    },
    MarkRange {
        start: deadpan_core::NamedMarkTarget,
        end: deadpan_core::NamedMarkTarget,
        range: FrameRange,
    },
}

impl Selection {
    pub(super) fn rebase_source_interpretation(
        &mut self,
        saved: &crate::project::SourceInterpretationSaved,
    ) {
        if let Some(identity) = &mut self.identity
            && identity.session == saved.session
            && identity.project == saved.project
            && identity.revision == saved.base
        {
            identity.revision = saved.revision.clone();
        }
    }
    pub(super) fn object(&self) -> Option<&SemanticObjectSelection> {
        match &self.kind {
            Some(SelectionKind::Object { selection, .. }) => Some(selection),
            _ => None,
        }
    }
    pub(super) fn node(&self) -> Option<&NodeId> {
        match &self.kind {
            Some(SelectionKind::Node { node, .. }) => Some(node),
            _ => None,
        }
    }
    pub(super) fn nodes(&self) -> Option<&[NodeId]> {
        match &self.kind {
            Some(SelectionKind::Nodes { nodes }) => Some(nodes),
            _ => None,
        }
    }
    pub(super) fn has_bounds(&self) -> bool {
        self.kind.is_some()
    }

    pub(super) fn semantic(&self) -> Result<Option<SemanticVisualSelection>, String> {
        self.kind
            .as_ref()
            .map(|kind| match kind {
                SelectionKind::Time { anchor, head } => Ok(SemanticVisualSelection::Time {
                    anchor: ProjectFrame(
                        i64::try_from(*anchor).map_err(|error| error.to_string())?,
                    ),
                    head: ProjectFrame(i64::try_from(*head).map_err(|error| error.to_string())?),
                    extending: self.active,
                }),
                SelectionKind::Object { selection, .. } => Ok(SemanticVisualSelection::Object {
                    selection: selection.clone(),
                    extending: self.active,
                }),
                SelectionKind::Node { node, .. } => {
                    Ok(SemanticVisualSelection::Node { node: node.clone() })
                }
                SelectionKind::Nodes { nodes } => Ok(SemanticVisualSelection::Nodes {
                    nodes: nodes.clone(),
                }),
                SelectionKind::MarkRange { start, end, .. } => {
                    Ok(SemanticVisualSelection::MarkRange {
                        start: start.clone(),
                        end: end.clone(),
                    })
                }
            })
            .transpose()
    }

    fn restore_semantic(
        &mut self,
        selection: Option<SemanticVisualSelection>,
        start: u64,
        end: u64,
        object_range: Option<FrameRange>,
    ) -> Result<(), String> {
        let restored = selection
            .as_ref()
            .map(|selection| match selection {
                SemanticVisualSelection::Time {
                    anchor,
                    head,
                    extending,
                } => {
                    let anchor = u64::try_from(anchor.0).map_err(|error| error.to_string())?;
                    let head = u64::try_from(head.0).map_err(|error| error.to_string())?;
                    if !(start..=end).contains(&anchor) || !(start..=end).contains(&head) {
                        return Err("The macro Visual selection is outside this group.".to_owned());
                    }
                    Ok((SelectionKind::Time { anchor, head }, *extending))
                }
                SemanticVisualSelection::Object {
                    selection,
                    extending,
                } => {
                    let range = object_range.ok_or("The selected group is no longer available.")?;
                    if range.start().0 < start as i64 || range.end().0 > end as i64 {
                        return Err("The selected object is outside this group.".into());
                    }
                    Ok((
                        SelectionKind::Object {
                            selection: selection.clone(),
                            range,
                        },
                        *extending,
                    ))
                }
                SemanticVisualSelection::Node { node } => {
                    let range = object_range.ok_or("The selected node is no longer available.")?;
                    if range.start().0 < start as i64 || range.end().0 > end as i64 {
                        return Err("The selected node is outside this group.".into());
                    }
                    Ok((
                        SelectionKind::Node {
                            node: node.clone(),
                            range,
                        },
                        false,
                    ))
                }
                SemanticVisualSelection::Nodes { .. } => {
                    Err("A node set needs its complete resolved members.".into())
                }
                SemanticVisualSelection::MarkRange {
                    start: first,
                    end: last,
                } => {
                    let range =
                        object_range.ok_or("The named mark range is no longer available.")?;
                    if range.start().0 < start as i64 || range.end().0 > end as i64 {
                        return Err("The named mark range is outside this group.".into());
                    }
                    Ok((
                        SelectionKind::MarkRange {
                            start: first.clone(),
                            end: last.clone(),
                            range,
                        },
                        false,
                    ))
                }
            })
            .transpose()?;
        self.active = restored.as_ref().is_some_and(|(_, active)| *active);
        self.kind = restored.map(|(kind, _)| kind);
        Ok(())
    }

    /// A typed mark-only receipt proves the timeline and this scope unchanged.
    /// Never rebase through an unrelated edit or a stale visible workspace.
    pub(super) fn rebase_mark(&mut self, saved: &crate::project::marks::Saved) {
        self.rebase_revision(
            saved.id.session,
            &saved.id.project,
            &saved.id.revision,
            &saved.revision,
        );
    }

    pub(super) fn rebase_revision(
        &mut self,
        session: u64,
        project: &deadpan_core::ProjectId,
        before: &RevisionId,
        after: &RevisionId,
    ) {
        if let Some(identity) = &mut self.identity
            && identity.session == session
            && &identity.project == project
            && &identity.revision == before
        {
            identity.revision = after.clone();
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
            self.kind = Some(SelectionKind::Time {
                anchor: at,
                head: at,
            });
            self.active = true;
        }
    }

    pub fn move_to(&mut self, at: u64) {
        if self.active {
            match &mut self.kind {
                Some(SelectionKind::Time { head, .. }) => *head = at,
                Some(
                    SelectionKind::Object { range, .. }
                    | SelectionKind::MarkRange { range, .. }
                    | SelectionKind::Node { range, .. },
                ) => {
                    self.kind = Some(SelectionKind::Time {
                        anchor: range.start().0 as u64,
                        head: at,
                    });
                }
                Some(SelectionKind::Nodes { .. }) | None => {}
            }
        }
    }

    fn range(&self) -> Option<FrameRange> {
        let (anchor, head) = match &self.kind {
            Some(SelectionKind::Time { anchor, head }) => (*anchor, *head),
            Some(
                SelectionKind::Object { range, .. }
                | SelectionKind::MarkRange { range, .. }
                | SelectionKind::Node { range, .. },
            ) => {
                return Some(*range);
            }
            Some(SelectionKind::Nodes { .. }) | None => return None,
        };
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
        self.kind = None;
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
        if let Some(SemanticVisualSelection::Nodes { nodes }) = &selection {
            self.resolve_native_node_set(nodes)?;
            return Ok(selection);
        }
        let object_range = self.object_range(selection.as_ref())?;
        let extending_head = match &selection {
            Some(SemanticVisualSelection::Time {
                head,
                extending: true,
                ..
            }) => Some(*head),
            Some(SemanticVisualSelection::Object {
                extending: true, ..
            }) => object_range.map(|range| range.end()),
            _ => None,
        };
        if extending_head
            .is_some_and(|head| u64::try_from(head.0).ok() != Some(self.sequence_cursor))
        {
            return Err("The extending Edit selection no longer ends at the cursor.".into());
        }
        let mut checked = Selection::default();
        checked.restore_semantic(
            selection.clone(),
            self.scope_start,
            self.scope_end,
            object_range,
        )?;
        Ok(selection)
    }

    pub(super) fn restore_macro_visual_selection(
        &mut self,
        selection: Option<SemanticVisualSelection>,
    ) -> Result<(), String> {
        let identity = self
            .edit_range_identity()
            .ok_or("The macro edit is no longer visible.")?;
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("The edit is no longer visible.")?;
        let scope = self.sequence_scope.resolve(workspace)?;
        let mut restored = Selection::default();
        if let Some(SemanticVisualSelection::Nodes { nodes }) = &selection {
            self.resolve_native_node_set(nodes)?;
            restored.kind = Some(SelectionKind::Nodes {
                nodes: nodes.clone(),
            });
            restored.identity = Some(identity);
            self.edit_range = restored;
            return Ok(());
        }
        let object_range = self.object_range(selection.as_ref())?;
        restored.restore_semantic(selection, scope.start, scope.end, object_range)?;
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
        self.edit_range.kind = Some(SelectionKind::Time {
            anchor: range.range.start().0 as u64,
            head: range.range.end().0 as u64,
        });
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
        if matches!(
            self.edit_range.kind,
            Some(SelectionKind::MarkRange { .. } | SelectionKind::Node { .. })
        ) {
            self.object_range(self.edit_range.semantic().ok()?.as_ref())
                .ok()
                .flatten()
        } else {
            self.edit_range.range()
        }
    }

    pub(super) fn selected_node_set(
        &self,
    ) -> Result<Option<deadpan_core::SemanticNodeSetTarget>, String> {
        let Some(nodes) = self.edit_range.nodes() else {
            return Ok(None);
        };
        if self.edit_range.identity != self.edit_range_identity() {
            return Err("The node set belongs to an earlier editing context.".into());
        }
        self.resolve_native_node_set(nodes).map(Some)
    }

    fn resolve_native_node_set(
        &self,
        nodes: &[NodeId],
    ) -> Result<deadpan_core::SemanticNodeSetTarget, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("The selected node set is no longer visible.")?;
        let parent = self.sequence_scope.resolve(workspace)?.owner;
        let context = deadpan_core::SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            selected_hold_duration: None,
            parent: parent.clone(),
            cursor: ProjectFrame(
                i64::try_from(self.sequence_cursor).map_err(|error| error.to_string())?,
            ),
            selected_child: self.selected_beat.clone(),
            visual_selection: None,
        };
        workspace
            .document
            .resolve_semantic_nodes(&context, nodes)
            .map_err(|error| error.to_string())
    }

    pub(super) fn node_set_details(&self) -> Option<Vec<String>> {
        let target = self.selected_node_set().ok()??;
        let workspace = self.workspace.as_ref()?;
        Some(
            target
                .members
                .iter()
                .map(|member| {
                    let label = workspace
                        .document
                        .nodes()
                        .get(&member.node)
                        .map(|node| node.label.as_str())
                        .unwrap_or("Unavailable");
                    format!(
                        "{label}\n{}\nParent {}\nEdit [{}..{}) · {} f{}",
                        member.node.as_str(),
                        member.parent.as_str(),
                        member.range.start().0,
                        member.range.end().0,
                        member.range.duration().frames(),
                        if member.range.start() == member.range.end() {
                            " · empty node"
                        } else {
                            ""
                        }
                    )
                })
                .collect(),
        )
    }

    fn object_range(
        &self,
        selection: Option<&SemanticVisualSelection>,
    ) -> Result<Option<FrameRange>, String> {
        if !matches!(
            selection,
            Some(
                SemanticVisualSelection::Object { .. }
                    | SemanticVisualSelection::MarkRange { .. }
                    | SemanticVisualSelection::Node { .. }
            )
        ) {
            return Ok(None);
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("The selected group is no longer visible.")?;
        let parent = self.sequence_scope.resolve(workspace)?.owner;
        match selection {
            Some(SemanticVisualSelection::Object { selection, .. }) => workspace
                .document
                .resolve_object_selection(parent, selection)
                .map(|target| Some(target.range))
                .map_err(|error| error.to_string()),
            Some(SemanticVisualSelection::Node { node }) => {
                let context = deadpan_core::SemanticContext {
                    original: None,
                    scoped: None,
                    hold_duration: None,
                    shared_hold_duration: None,
                    mixed_hold_duration: None,
                    selected_hold_duration: None,
                    parent: parent.clone(),
                    cursor: ProjectFrame(
                        i64::try_from(self.sequence_cursor).map_err(|error| error.to_string())?,
                    ),
                    selected_child: self.selected_beat.clone(),
                    visual_selection: None,
                };
                workspace
                    .document
                    .resolve_semantic_node(&context, node)
                    .map(|target| Some(target.range))
                    .map_err(|error| error.to_string())
            }
            Some(SemanticVisualSelection::MarkRange { start, end }) => {
                let context = deadpan_core::SemanticContext {
                    original: None,
                    scoped: None,
                    hold_duration: None,
                    shared_hold_duration: None,
                    mixed_hold_duration: None,
                    selected_hold_duration: None,
                    parent: parent.clone(),
                    cursor: ProjectFrame(
                        i64::try_from(self.sequence_cursor).map_err(|error| error.to_string())?,
                    ),
                    selected_child: self.selected_beat.clone(),
                    visual_selection: None,
                };
                workspace
                    .document
                    .resolve_semantic_mark_range(&context, start, end)
                    .map(Some)
                    .map_err(|error| error.to_string())
            }
            _ => Ok(None),
        }
    }

    pub(super) fn select_group_object(&mut self, object: deadpan_core::SemanticTextObject) {
        if !self.macro_action_allowed(Action::SelectObject(object)) {
            return;
        }
        self.bindings.clear();
        let result = self
            .capture_macro_target()
            .and_then(|capture| capture.select_object(object));
        match result {
            Ok(context) => {
                self.pause_playback();
                let prior = self.sequence_cursor;
                self.sequence_cursor = context.cursor.0 as u64;
                if let Err(error) = self.restore_macro_visual_selection(context.visual_selection) {
                    self.sequence_cursor = prior;
                    self.error = Some(error);
                    return;
                }
                self.record_macro_local(deadpan_core::SemanticInstruction::SelectObject { object });
                self.error = None;
                let noun = object.noun();
                self.message = Some(format!(
                    "{}{} selected. {} copies; {} cuts; {} repeats; {} replaces. Motion changes this into a time range; {} retains the object.",
                    noun[..1].to_uppercase(),
                    &noun[1..],
                    self.editor_key(EditorKey::Copy),
                    self.editor_key(EditorKey::CutRange),
                    self.editor_key(EditorKey::Repeat),
                    self.editor_pair(EditorKey::PasteAfter, EditorKey::PasteBefore, "/"),
                    self.editor_key(EditorKey::Visual)
                ));
                if prior != self.sequence_cursor {
                    self.request_picture(false);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn select_named_mark_range(
        &mut self,
        target: Option<Result<macros::Capture, String>>,
        start: deadpan_core::NamedMarkTarget,
        end: deadpan_core::NamedMarkTarget,
    ) {
        let result = (|| {
            if self.view != View::Sequence || self.edit_role != deadpan_core::MediaRole::Linked {
                return Err("Select named Edit marks in Your edit with role=linked.".to_owned());
            }
            let capture = target.ok_or("Open :select again to capture its editing context.")??;
            if !capture.matches(self) {
                return Err(
                    "The named-mark selection context changed. Enter the command again.".into(),
                );
            }
            capture.select_mark_range(start.clone(), end.clone())
        })();
        match result {
            Ok(context) => {
                let prior = self.sequence_cursor;
                self.sequence_cursor = context.cursor.0 as u64;
                if let Err(error) = self.restore_macro_visual_selection(context.visual_selection) {
                    self.sequence_cursor = prior;
                    self.error = Some(error);
                    return;
                }
                self.pause_playback();
                self.revoke_scoped_continuation();
                self.record_macro_local(deadpan_core::SemanticInstruction::SelectMarkRange {
                    start,
                    end,
                });
                self.error = None;
                self.message = self.edit_range_label().map(|label| {
                    format!(
                        "{label}. {} copies; {} cuts; {} repeats.",
                        self.editor_key(EditorKey::Copy),
                        self.editor_key(EditorKey::CutRange),
                        self.editor_key(EditorKey::Repeat)
                    )
                });
                if prior != self.sequence_cursor {
                    self.request_picture(false);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn select_explicit_node(
        &mut self,
        target: Option<Result<macros::Capture, String>>,
        node: NodeId,
    ) {
        let result = (|| {
            if self.view != View::Sequence || self.edit_role != deadpan_core::MediaRole::Linked {
                return Err("Select an authored node in Your edit with role=linked.".to_owned());
            }
            let capture = target.ok_or("Open :select again to capture its editing context.")??;
            if !capture.matches(self) {
                return Err("The node selection context changed. Enter the command again.".into());
            }
            capture.select_node(node.clone())
        })();
        match result {
            Ok(context) => {
                let prior = self.sequence_cursor;
                self.sequence_cursor = context.cursor.0 as u64;
                if let Err(error) = self.restore_macro_visual_selection(context.visual_selection) {
                    self.sequence_cursor = prior;
                    self.error = Some(error);
                    return;
                }
                self.selected_beat = context.selected_child;
                self.pause_playback();
                self.revoke_scoped_continuation();
                self.record_macro_local(deadpan_core::SemanticInstruction::SelectNode { node });
                self.error = None;
                self.message = self.edit_range_label().map(|label| {
                    format!(
                        "{label}. {} copies; {} cuts; {} repeats; {} replaces.",
                        self.editor_key(EditorKey::Copy),
                        self.editor_key(EditorKey::CutRange),
                        self.editor_key(EditorKey::Repeat),
                        self.editor_pair(EditorKey::PasteAfter, EditorKey::PasteBefore, "/")
                    )
                });
                if prior != self.sequence_cursor {
                    self.request_picture(false);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn select_explicit_nodes(
        &mut self,
        target: Option<Result<macros::Capture, String>>,
        nodes: Vec<NodeId>,
    ) {
        let result = (|| {
            if self.view != View::Sequence || self.edit_role != deadpan_core::MediaRole::Linked {
                return Err("Select authored node sets in Your edit with role=linked.".to_owned());
            }
            let capture = target.ok_or("Open :select again to capture its editing context.")??;
            if !capture.matches(self) {
                return Err(
                    "The node-set selection context changed. Enter the command again.".into(),
                );
            }
            capture.select_nodes(nodes.clone())
        })();
        match result {
            Ok(context) => {
                let prior = self.sequence_cursor;
                self.sequence_cursor = context.cursor.0 as u64;
                if let Err(error) = self.restore_macro_visual_selection(context.visual_selection) {
                    self.sequence_cursor = prior;
                    self.error = Some(error);
                    return;
                }
                self.selected_beat = context.selected_child;
                self.pause_playback();
                self.revoke_scoped_continuation();
                self.record_macro_local(deadpan_core::SemanticInstruction::SelectNodes { nodes });
                self.error = None;
                self.message = Some("Node set retained. y copies one union; d cuts it. Inspector lists each exact member. Clear the set before choosing a paste seam.".into());
                if prior != self.sequence_cursor {
                    self.request_picture(false);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn edit_selection(&self) -> navigation::EditSelection {
        if self.edit_range.identity != self.edit_range_identity() || self.edit_range.kind.is_none()
        {
            navigation::EditSelection::None
        } else if matches!(
            self.edit_range.kind,
            Some(
                SelectionKind::Object { .. }
                    | SelectionKind::Node { .. }
                    | SelectionKind::Nodes { .. }
            )
        ) {
            navigation::EditSelection::Object
        } else if self.selected_edit_range().is_some() {
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

    pub(super) fn routed_domain(&self) -> navigation::RoutingDomain {
        if self.sound_focused() || self.pane == Pane::Sounds || self.event_focused() {
            navigation::RoutingDomain::Sound
        } else if self.view == View::Source {
            navigation::RoutingDomain::Original
        } else {
            navigation::RoutingDomain::Edit
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
        } else if self.edit_selection() == navigation::EditSelection::Object {
            format!(
                "Object retained. {} copies; {} cuts; :splice previews replacement; {} replaces now.",
                self.editor_key(EditorKey::Copy),
                self.editor_key(EditorKey::CutRange),
                self.editor_pair(EditorKey::PasteAfter, EditorKey::PasteBefore, "/")
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
        if self.edit_range.nodes().is_some() {
            let target = self.selected_node_set().ok()??;
            return Some(format!(
                "Node set · {} members · {} f selected · {} · Inspector: members",
                target.members.len(),
                target.total_duration.frames(),
                self.beat_scope_label()
            ));
        }

        if let Some(SelectionKind::Node { node, .. }) = &self.edit_range.kind {
            let range = self.selected_edit_range()?;
            let workspace = self.workspace.as_ref()?;
            let label = &workspace.document.nodes().get(node)?.label;
            let parent = self.sequence_scope.resolve(workspace).ok()?.owner;
            return Some(format!(
                "Node: {label} · {} · scope {} · [{}..{}) · {} f · {}",
                node.as_str(),
                parent.as_str(),
                range.start().0,
                range.end().0,
                range.duration().frames(),
                if range.start() == range.end() {
                    "empty node retained"
                } else {
                    "whole node retained"
                }
            ));
        }
        if let Some(SelectionKind::MarkRange { start, end, .. }) = &self.edit_range.kind {
            return self.selected_edit_range().map(|range| {
                format!(
                    "Marks {} → {} · Edit [{}..{}) · {} f",
                    start.id.as_str(),
                    end.id.as_str(),
                    range.start().0,
                    range.end().0,
                    range.duration().frames(),
                )
            });
        }
        if self.edit_selection() == navigation::EditSelection::Object
            && let Some(SelectionKind::Object { selection, range }) = &self.edit_range.kind
        {
            let label = self
                .workspace
                .as_ref()?
                .document
                .nodes()
                .get(&selection.group)?
                .label
                .as_str();
            let noun = selection.kind.noun();
            let kind = format!("{}{}", noun[..1].to_uppercase(), &noun[1..]);
            return Some(format!(
                "{kind}: {label} · [{}..{}) · {} f · {}",
                range.start().0,
                range.end().0,
                range.duration().frames(),
                if self.edit_range.active {
                    "object selected · motion selects time"
                } else {
                    "object retained"
                }
            ));
        }
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
    fn node_sets_never_expose_a_single_interval_and_finished_motion_preserves_ids() {
        for nodes in [
            vec![NodeId::new("one").unwrap()],
            vec![
                NodeId::new("second").unwrap(),
                NodeId::new("empty").unwrap(),
                NodeId::new("first").unwrap(),
            ],
        ] {
            let mut selected = Selection {
                kind: Some(SelectionKind::Nodes {
                    nodes: nodes.clone(),
                }),
                ..Selection::default()
            };
            assert_eq!(selected.range(), None);
            selected.move_to(90);
            assert_eq!(
                selected.semantic().unwrap(),
                Some(SemanticVisualSelection::Nodes { nodes })
            );
            selected.toggle(90);
            assert_eq!(
                selected.semantic().unwrap(),
                Some(SemanticVisualSelection::Time {
                    anchor: ProjectFrame(90),
                    head: ProjectFrame(90),
                    extending: true,
                })
            );
        }
    }

    #[test]
    fn explicit_node_selection_retains_empty_identity_and_motion_but_v_starts_time() {
        for end in [10, 40] {
            let node = NodeId::new("exact-node").unwrap();
            let range = FrameRange::new(ProjectFrame(10), ProjectFrame(end)).unwrap();
            let semantic = Some(SemanticVisualSelection::Node { node: node.clone() });
            let mut selected = Selection::default();
            selected
                .restore_semantic(semantic.clone(), 0, 50, Some(range))
                .unwrap();
            assert!(!selected.active);
            assert_eq!(selected.range(), Some(range));
            assert_eq!(selected.node(), Some(&node));
            selected.move_to(49);
            assert_eq!(selected.semantic().unwrap(), semantic);
            selected.toggle(49);
            assert_eq!(
                selected.semantic().unwrap(),
                Some(SemanticVisualSelection::Time {
                    anchor: ProjectFrame(49),
                    head: ProjectFrame(49),
                    extending: true,
                })
            );
        }
    }

    #[test]
    fn explicit_node_restore_refuses_missing_or_out_of_scope_geometry_atomically() {
        let mut selected = Selection::default();
        selected.toggle(12);
        let retained = selected.clone();
        let semantic = Some(SemanticVisualSelection::Node {
            node: NodeId::new("exact").unwrap(),
        });
        for range in [
            None,
            Some(FrameRange::new(ProjectFrame(9), ProjectFrame(10)).unwrap()),
            Some(FrameRange::new(ProjectFrame(10), ProjectFrame(21)).unwrap()),
        ] {
            assert!(
                selected
                    .restore_semantic(semantic.clone(), 10, 20, range)
                    .is_err()
            );
            assert_eq!(selected, retained);
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
            Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(10),
                head: ProjectFrame(10),
                extending: true,
            }),
            Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(10),
                head: ProjectFrame(10),
                extending: false,
            }),
            Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(40),
                head: ProjectFrame(10),
                extending: true,
            }),
            Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(40),
                head: ProjectFrame(10),
                extending: false,
            }),
        ] {
            let mut native = Selection::default();
            native
                .restore_semantic(selection.clone(), 10, 40, None)
                .unwrap();
            assert_eq!(native.semantic().unwrap(), selection);
            assert_eq!(native.has_bounds(), selection.is_some());
            native.move_to(20);
            let mut expected = selection;
            if let Some(SemanticVisualSelection::Time {
                head,
                extending: true,
                ..
            }) = &mut expected
            {
                *head = ProjectFrame(20);
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
                        Some(SemanticVisualSelection::Time {
                            anchor: ProjectFrame(anchor),
                            head: ProjectFrame(head),
                            extending: true,
                        }),
                        10,
                        40,
                        None
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

    #[test]
    fn object_identity_survives_finish_and_only_active_motion_becomes_time() {
        for (start, end) in [(10, 10), (10, 40)] {
            let object = SemanticObjectSelection {
                kind: deadpan_core::SemanticTextObject::InnerGroup,
                group: NodeId::new("group").unwrap(),
            };
            let range = FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap();
            let mut selected = Selection::default();
            selected
                .restore_semantic(
                    Some(SemanticVisualSelection::Object {
                        selection: object.clone(),
                        extending: true,
                    }),
                    0,
                    50,
                    Some(range),
                )
                .unwrap();
            assert_eq!(selected.range(), Some(range));
            let mut extending = selected.clone();
            selected.toggle(end as u64);
            selected.move_to(49);
            assert_eq!(
                selected.semantic().unwrap(),
                Some(SemanticVisualSelection::Object {
                    selection: object,
                    extending: false,
                })
            );
            extending.move_to(5);
            assert_eq!(
                extending.semantic().unwrap(),
                Some(SemanticVisualSelection::Time {
                    anchor: ProjectFrame(start),
                    head: ProjectFrame(5),
                    extending: true,
                })
            );
        }
    }

    #[test]
    fn object_restore_requires_checked_geometry_and_preserves_selection_on_error() {
        let object = Some(SemanticVisualSelection::Object {
            selection: SemanticObjectSelection {
                kind: deadpan_core::SemanticTextObject::AroundGroup,
                group: NodeId::new("group").unwrap(),
            },
            extending: true,
        });
        let mut selected = Selection::default();
        selected.toggle(12);
        selected.move_to(18);
        let retained = selected.clone();
        for range in [
            None,
            Some(FrameRange::new(ProjectFrame(9), ProjectFrame(20)).unwrap()),
            Some(FrameRange::new(ProjectFrame(12), ProjectFrame(21)).unwrap()),
        ] {
            assert!(
                selected
                    .restore_semantic(object.clone(), 10, 20, range)
                    .is_err()
            );
            assert_eq!(selected, retained);
        }
    }
}
