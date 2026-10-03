//! Copied contents survive navigation; pending yanks belong to one exact intent.

use super::*;
use crate::project::{registers, slice};

#[derive(Clone, Debug)]
pub(super) enum Content {
    Original(moment::Copied),
    Edited(Arc<slice::Captured>),
    Macro(Arc<deadpan_core::SemanticProgram>),
}

pub(super) const MACRO_PASTE_ERROR: &str =
    "This register contains a macro, which cannot be pasted as copied content.";

impl Content {
    pub fn available_label(&self, workspace: Option<&Workspace>) -> String {
        let label = self.label();
        if matches!(self, Self::Macro(_)) {
            return label;
        }
        if workspace.is_none_or(|workspace| self.check(workspace).is_err()) {
            format!("{label} · unavailable in this revision")
        } else {
            label
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Original(copied) => format!(
                "Copied Original [{}..{})",
                copied.ordinals.start, copied.ordinals.end
            ),
            Self::Edited(copied) => {
                let range = copied.slice().range();
                if range.duration() == deadpan_core::FrameDuration::ZERO {
                    if matches!(
                        copied.slice().selection(),
                        deadpan_core::SliceCaptureSelection::Children { .. }
                    ) {
                        return "Copied empty contents · 0 frames · structure only".into();
                    }
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
            Self::Macro(program) => {
                let count = program.instructions().len();
                format!(
                    "Macro · {count} instruction{}",
                    if count == 1 { "" } else { "s" }
                )
            }
        }
    }

    pub fn source(&self) -> Result<crate::project::splice::Source, String> {
        Ok(match self {
            Self::Original(copied) => crate::project::splice::Source::Original {
                asset: copied.identity.asset.clone(),
                qualification: copied.identity.qualification.clone(),
                ordinals: copied.ordinals.clone(),
            },
            Self::Edited(copied) => crate::project::splice::Source::Edited {
                copied: copied.clone(),
                range: copied.slice().range(),
            },
            Self::Macro(_) => return Err(MACRO_PASTE_ERROR.into()),
        })
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
            Self::Macro(_) => return Err(MACRO_PASTE_ERROR.into()),
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

enum Unsettled {
    Original(slice::CopyId),
    Edit(slice::CopyId),
    Cut(slice::CaptureRequest),
}

#[derive(Default)]
pub(super) struct Register {
    content: Option<Content>,
    named: std::collections::BTreeMap<char, Content>,
    selected: Option<char>,
    selected_explicit: bool,
    session: Option<(u64, deadpan_core::ProjectId)>,
    version: Option<u64>,
    pending: Option<Pending>,
    pending_original: Option<(slice::CopyId, moment::Selection)>,
    // A rejected newer intent cannot cancel an already accepted durable write.
    unsettled: Option<Unsettled>,
}

impl Register {
    pub fn select(&mut self, name: char) -> Result<(), String> {
        self.selected = match name {
            '"' => None,
            'a'..='z' | 'A'..='Z' => Some(name.to_ascii_lowercase()),
            _ => return Err("Choose a register from a–z, or \" for the default copy.".into()),
        };
        self.selected_explicit = true;
        Ok(())
    }

    pub fn selected(&self) -> Option<char> {
        self.selected
    }

    pub fn bank_version(&self) -> Option<u64> {
        self.version
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.selected_explicit = false;
    }

    /// Distinguish an explicit default-register choice from no override for dot.
    pub fn selected_override(&self) -> Option<Option<char>> {
        self.selected_explicit.then_some(self.selected)
    }

    pub fn selected_content(&self) -> Option<&Content> {
        self.selected
            .map_or_else(|| self.content(), |name| self.named.get(&name))
    }

    pub fn entries(&self) -> impl Iterator<Item = (char, &Content)> {
        self.content
            .iter()
            .map(|value| ('"', value))
            .chain(self.named.iter().map(|(name, value)| (*name, value)))
    }

    /// A failed new write consumes its one-shot name but preserves every slot.
    pub fn begin_write(&mut self) -> Option<char> {
        self.supersede();
        self.selected_explicit = false;
        self.selected.take()
    }

    #[cfg(test)]
    fn store(&mut self, destination: Option<char>, content: Content) {
        if let Some(name) = destination {
            self.named.insert(name, content.clone());
        }
        self.content = Some(content);
    }

    pub fn content(&self) -> Option<&Content> {
        self.content.as_ref()
    }

    pub fn original(&self) -> Option<&moment::Copied> {
        match self.content.as_ref()? {
            Content::Original(copied) => Some(copied),
            Content::Edited(_) | Content::Macro(_) => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.unsettled.is_some()
    }

    /// Even a rejected newer yank must not be completed by an older reply.
    pub fn supersede(&mut self) {
        self.pending = None;
        self.pending_original = None;
    }

    #[cfg(test)]
    pub fn original_copied(&mut self, copied: moment::Copied) {
        let destination = self.begin_write();
        self.store(destination, Content::Original(copied));
        self.unsettled = None;
    }

    pub fn expect_original(&mut self, id: slice::CopyId, selection: moment::Selection) {
        self.unsettled = Some(Unsettled::Original(id.clone()));
        self.pending_original = Some((id, selection));
    }

    pub fn receive_original(
        &mut self,
        update: registers::OriginalUpdate,
    ) -> Option<(moment::Selection, Result<(), String>)> {
        if matches!(&self.unsettled, Some(Unsettled::Original(id)) if id == &update.id) {
            self.unsettled = None;
        }
        if self
            .pending_original
            .as_ref()
            .is_none_or(|(id, _)| id != &update.id)
        {
            return None;
        }
        let (_, selection) = self.pending_original.take().expect("matched Original copy");
        Some((selection, update.result))
    }

    #[cfg(any(test, feature = "ui-harness"))]
    pub fn expect(&mut self, request: slice::CaptureRequest, selection: edit_range::Selection) {
        let destination = self.begin_write();
        let mut request = request;
        request.register = destination;
        self.expect_to(request, selection);
    }

    pub fn expect_to(&mut self, request: slice::CaptureRequest, selection: edit_range::Selection) {
        self.unsettled = Some(Unsettled::Edit(request.id.clone()));
        self.pending = Some(Pending {
            request,
            intent: Intent::Yank(selection),
        });
    }

    #[cfg(any(test, feature = "ui-harness"))]
    pub fn expect_cut(&mut self, request: slice::CaptureRequest) {
        let destination = self.begin_write();
        let mut request = request;
        request.register = destination;
        self.expect_cut_to(request);
    }

    pub fn expect_cut_to(&mut self, request: slice::CaptureRequest) {
        self.unsettled = Some(Unsettled::Cut(request.clone()));
        self.pending = Some(Pending {
            request,
            intent: Intent::Cut,
        });
    }

    pub fn reconcile(&mut self, workspace: Option<&Workspace>) {
        let session = workspace.map(|value| (value.session, value.document.project_id().clone()));
        if workspace.is_none() || (self.session.is_some() && self.session != session) {
            *self = Self::default();
        }
        self.session = session;
    }

    /// Durable state is independent of pending UI confirmation. A superseded
    /// successful copy still saved its named slot; only newer bank versions
    /// may replace this snapshot. Missing current media does not erase a slot.
    pub fn install_bank(&mut self, bank: &registers::Bank) {
        if self.session.as_ref() != Some(&(bank.session, bank.project.clone()))
            || self.version.is_some_and(|version| version >= bank.version)
        {
            return;
        }
        self.content = None;
        self.named.clear();
        for (&name, value) in &bank.entries {
            let content = match value {
                registers::Value::Original {
                    asset,
                    qualification,
                    ordinals,
                } => Content::Original(moment::Copied {
                    identity: moment::Identity {
                        session: bank.session,
                        asset: asset.clone(),
                        qualification: qualification.clone(),
                    },
                    ordinals: ordinals.clone(),
                }),
                registers::Value::Edited(copied) => Content::Edited(copied.clone()),
                registers::Value::Macro(program) => Content::Macro(program.clone()),
            };
            if name == '"' {
                self.content = Some(content);
            } else {
                self.named.insert(name, content);
            }
        }
        self.version = Some(bank.version);
    }

    pub fn receive(
        &mut self,
        update: slice::CaptureUpdate,
    ) -> Option<(edit_range::Selection, Result<(), String>)> {
        if matches!(&self.unsettled, Some(Unsettled::Edit(id)) if id == &update.id) {
            self.unsettled = None;
        }
        if self.pending.as_ref().is_none_or(|pending| {
            pending.request.id != update.id || !matches!(pending.intent, Intent::Yank(_))
        }) {
            return None;
        }
        let pending = self.pending.take().expect("matched copy request");
        let result = update
            .result
            .and_then(|copied| Self::accept(&pending.request, copied));
        let Intent::Yank(selection) = pending.intent else {
            unreachable!("matched yank intent")
        };
        Some((selection, result))
    }

    pub fn receive_cut(
        &mut self,
        update: slice::CutUpdate,
    ) -> Option<Result<slice::CutReceipt, String>> {
        if matches!(&self.unsettled, Some(Unsettled::Cut(request)) if request == &update.request) {
            self.unsettled = None;
        }
        if self.pending.as_ref().is_none_or(|pending| {
            pending.request != update.request || !matches!(pending.intent, Intent::Cut)
        }) {
            return None;
        }
        self.pending.take().expect("matched cut request");
        Some(update.result.and_then(|receipt| {
            Self::accept(&update.request, receipt.copied.clone())?;
            Ok(receipt)
        }))
    }

    fn accept(request: &slice::CaptureRequest, copied: Arc<slice::Captured>) -> Result<(), String> {
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
    pub(super) fn cancel_register_choice(&mut self) {
        if self.copied.selected().is_some() {
            self.copied.clear_selection();
            self.message = Some("Register choice cancelled.".into());
        }
    }

    pub(super) fn select_register(&mut self, name: char) {
        self.bindings.clear();
        if self.workspace.is_none() {
            self.error = Some("Open a project before selecting a copy register.".into());
            return;
        }
        match self.copied.select(name) {
            Ok(()) => {
                let label = self.copied.selected_content().map_or_else(
                    || "empty".into(),
                    |content| content.available_label(self.workspace.as_deref()),
                );
                self.error = None;
                self.message = Some(match self.copied.selected() {
                    Some(name)
                        if matches!(self.copied.selected_content(), Some(Content::Macro(_))) =>
                    {
                        format!(
                            "Register {name} · {label}. {} then {name} runs it. A copy or cut replaces it. Esc cancels.",
                            self.editor_key(EditorKey::MacroExecute),
                        )
                    }
                    Some(name) => format!(
                        "Register {name} · {label}. Next copy, picture cut, paste or :splice uses it. Esc cancels."
                    ),
                    None => format!("Default copy · {label}."),
                });
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn register_status(&self) -> Option<String> {
        let name = self.copied.selected()?;
        let kind = match self.copied.selected_content() {
            Some(Content::Original(_)) => "Original",
            Some(Content::Edited(_)) => "Edit",
            Some(content @ Content::Macro(_)) => {
                return Some(format!("Register {name} · {}", content.label()));
            }
            None => "empty",
        };
        let unavailable = self.copied.selected_content().is_some_and(|content| {
            self.workspace
                .as_deref()
                .is_none_or(|workspace| content.check(workspace).is_err())
        });
        Some(format!(
            "Register {name} · {kind}{}",
            if unavailable { " · unavailable" } else { "" }
        ))
    }

    pub(super) fn copy_slice(&mut self) {
        self.copy_slice_to(self.copied.selected());
    }

    pub(super) fn copy_slice_to(&mut self, destination: Option<char>) {
        if self.record_macro_yank(destination, None) {
            return;
        }
        self.copied.begin_write();
        if self.view == View::Source {
            self.copy_moment(destination);
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
                persisted_version: None,
            },
            register: destination,
            scope: self.sequence_scope.clone(),
            parent,
            selection,
        };
        match self
            .service
            .submit(ProjectRequest::CaptureEditSlice(request.clone()))
        {
            Ok(()) => {
                self.copied.expect_to(request, self.edit_range.clone());
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
                self.message = Some(format!(
                    "Edit slice copied. :splice previews placement; {} pastes or replaces an Edit selection.",
                    self.editor_pair(EditorKey::PasteAfter, EditorKey::PasteBefore, "/")
                ));
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
                        "Cut saved and copied: Edit [{}..{}) · {} f. {} pastes; :splice previews placement. Undo with {}.",
                        range.start().0,
                        range.end().0,
                        range.duration().frames(),
                        self.editor_pair(EditorKey::PasteAfter, EditorKey::PasteBefore, "/"),
                        self.editor_key(EditorKey::Undo),
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
                persisted_version: None,
            },
            register: None,
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

    fn bank(version: u64, ordinals: std::ops::Range<u64>) -> registers::Bank {
        registers::Bank {
            session: 7,
            project: ProjectId::new("copy-project").unwrap(),
            version,
            entries: ['"', 'a']
                .into_iter()
                .map(|name| {
                    (
                        name,
                        registers::Value::Original {
                            asset: original().identity.asset,
                            qualification: original().identity.qualification,
                            ordinals: ordinals.clone(),
                        },
                    )
                })
                .collect(),
        }
    }

    fn macro_program() -> Arc<deadpan_core::SemanticProgram> {
        Arc::new(
            deadpan_core::SemanticProgram::new(vec![
                deadpan_core::SemanticInstruction::MoveFrames {
                    forward: true,
                    count: std::num::NonZeroU32::new(3).unwrap(),
                },
                deadpan_core::SemanticInstruction::CutFrames {
                    operation: deadpan_core::FrameCut::new(2).unwrap(),
                    register: deadpan_core::RegisterName::unnamed(),
                },
            ])
            .unwrap(),
        )
    }

    #[test]
    fn macro_only_bank_has_no_copy_alias_or_media_provenance_and_cannot_be_pasted() {
        let program = macro_program();
        let bank = registers::Bank {
            session: 7,
            project: ProjectId::new("copy-project").unwrap(),
            version: 4,
            entries: std::collections::BTreeMap::from([(
                'a',
                registers::Value::Macro(program.clone()),
            )]),
        };
        let mut register = Register {
            session: Some((bank.session, bank.project.clone())),
            ..Register::default()
        };
        assert_eq!(register.bank_version(), None);
        register.install_bank(&bank);
        assert_eq!(register.bank_version(), Some(4));
        assert_eq!(register.entries().count(), 1);
        assert!(register.content().is_none());
        assert!(register.original().is_none());
        register.select('a').unwrap();
        let content = register.selected_content().unwrap();
        assert!(matches!(content, Content::Macro(stored) if Arc::ptr_eq(stored, &program)));
        assert_eq!(content.label(), "Macro · 2 instructions");
        assert_eq!(content.available_label(None), "Macro · 2 instructions");
        assert_eq!(content.source().unwrap_err(), MACRO_PASTE_ERROR);
        register.reconcile(None);
        assert_eq!(register.bank_version(), None);
        assert_eq!(register.entries().count(), 0);
    }

    #[test]
    fn macro_replaces_named_copy_without_changing_default_and_newer_copy_replaces_macro() {
        let mut register = Register {
            session: Some((7, ProjectId::new("copy-project").unwrap())),
            ..Register::default()
        };
        register.install_bank(&bank(1, 2..8));
        register.select('a').unwrap();
        let mut macros = bank(2, 2..8);
        macros
            .entries
            .insert('a', registers::Value::Macro(macro_program()));
        register.install_bank(&macros);
        assert_eq!(register.selected(), Some('a'));
        assert_eq!(register.original().unwrap().ordinals, 2..8);
        assert!(matches!(
            register.selected_content(),
            Some(Content::Macro(_))
        ));
        assert_eq!(register.bank_version(), Some(2));
        register.install_bank(&bank(3, 12..18));
        assert!(
            matches!(register.selected_content(), Some(Content::Original(copied)) if copied.ordinals == (12..18))
        );
        assert_eq!(register.original().unwrap().ordinals, 12..18);
        register.install_bank(&macros);
        assert_eq!(register.bank_version(), Some(3));
        assert!(matches!(
            register.selected_content(),
            Some(Content::Original(_))
        ));
    }

    #[test]
    fn typed_macro_is_never_returned_as_an_original_copy() {
        let register = Register {
            content: Some(Content::Macro(macro_program())),
            ..Register::default()
        };
        assert!(register.original().is_none());
        assert_eq!(
            register.content().unwrap().source().unwrap_err(),
            MACRO_PASTE_ERROR
        );
        let source = Content::Original(original()).source().unwrap();
        assert!(
            matches!(source, crate::project::splice::Source::Original { ordinals, .. } if ordinals == (2..8))
        );
    }

    #[test]
    fn durable_snapshots_advance_independently_of_pending_confirmation() {
        let mut register = Register {
            session: Some((7, ProjectId::new("copy-project").unwrap())),
            ..Register::default()
        };
        let first = request(1);
        register.expect_original(first.id.clone(), moment::Selection::default());
        register.select('b').unwrap();
        register.install_bank(&bank(2, 12..18));
        assert_eq!(register.original().unwrap().ordinals, 12..18);
        assert_eq!(register.selected(), Some('b'));
        assert!(register.is_pending());
        register.install_bank(&bank(1, 2..8));
        assert_eq!(register.original().unwrap().ordinals, 12..18);
        assert!(
            register
                .receive_original(registers::OriginalUpdate {
                    id: first.id,
                    result: Ok(()),
                })
                .unwrap()
                .1
                .is_ok()
        );
        assert_eq!(register.original().unwrap().ordinals, 12..18);
        assert_eq!(register.selected(), Some('b'));
        assert!(!register.is_pending());
        let mut stale_session = bank(3, 2..8);
        stale_session.session = 8;
        register.install_bank(&stale_session);
        assert_eq!(register.original().unwrap().ordinals, 12..18);
        register.reconcile(None);
        register.install_bank(&bank(4, 2..8));
        assert!(register.content().is_none());
    }

    #[test]
    fn explicit_default_register_overrides_repeat_and_is_consumed_once() {
        let mut register = Register::default();
        assert_eq!(register.selected_override(), None);
        register.select('"').unwrap();
        assert_eq!(register.selected_override(), Some(None));
        assert_eq!(register.begin_write(), None);
        assert_eq!(register.selected_override(), None);
        register.select('b').unwrap();
        assert_eq!(register.selected_override(), Some(Some('b')));
        register.clear_selection();
        assert_eq!(register.selected_override(), None);
    }

    #[test]
    fn failed_newer_copy_keeps_earlier_durable_success_without_consuming_old_selection() {
        let mut register = Register {
            session: Some((7, ProjectId::new("copy-project").unwrap())),
            ..Register::default()
        };
        let old = request(1);
        register.expect_original(old.id.clone(), moment::Selection::default());
        let next = request(2);
        register.expect(next.clone(), edit_range::Selection::default());
        register.install_bank(&bank(1, 2..8));
        assert!(
            register
                .receive_original(registers::OriginalUpdate {
                    id: old.id,
                    result: Ok(())
                })
                .is_none()
        );
        assert!(register.is_pending());
        assert!(register.receive(failure(&next)).unwrap().1.is_err());
        assert_eq!(register.original().unwrap().ordinals, 2..8);
        assert_eq!(register.entries().count(), 2);
    }

    #[test]
    fn rejected_newer_intent_cannot_unblock_placement_before_older_save_reply() {
        let mut register = Register::default();
        let id = request(1).id;
        register.expect_original(id.clone(), moment::Selection::default());
        register.begin_write();
        assert!(register.is_pending());
        assert!(
            register
                .receive_original(registers::OriginalUpdate { id, result: Ok(()) })
                .is_none(),
            "superseded confirmation cannot consume a selection"
        );
        assert!(!register.is_pending());
    }

    #[test]
    fn named_original_copies_are_independent_and_success_also_updates_default() {
        let mut register = Register::default();
        register.select('A').unwrap();
        register.original_copied(original());
        assert_eq!(register.selected(), None);
        let mut other = original();
        other.ordinals = 12..18;
        register.select('b').unwrap();
        register.original_copied(other);
        assert_eq!(register.original().unwrap().ordinals, 12..18);
        register.select('a').unwrap();
        assert!(
            matches!(register.selected_content(), Some(Content::Original(value)) if value.ordinals == (2..8))
        );
        register.select('B').unwrap();
        assert!(
            matches!(register.selected_content(), Some(Content::Original(value)) if value.ordinals == (12..18))
        );
        assert_eq!(register.entries().count(), 3);
        register.select('z').unwrap();
        assert!(
            register.selected_content().is_none(),
            "an empty named slot never falls back to unnamed"
        );
        assert_eq!(register.original().unwrap().ordinals, 12..18);
        assert!(register.select('9').is_err());
        assert_eq!(register.selected(), Some('z'));
        register.select('"').unwrap();
        assert!(register.selected_content().is_some());
    }

    #[test]
    fn failed_named_capture_keeps_slots_and_newer_selection() {
        let mut register = Register::default();
        register.select('a').unwrap();
        register.original_copied(original());
        register.select('a').unwrap();
        let pending = request(1);
        register.expect(pending.clone(), edit_range::Selection::default());
        assert_eq!(register.selected(), None);
        register.select('b').unwrap();
        assert!(register.receive(failure(&pending)).unwrap().1.is_err());
        assert_eq!(register.selected(), Some('b'));
        assert!(register.selected_content().is_none());
        register.select('a').unwrap();
        assert!(
            matches!(register.selected_content(), Some(Content::Original(value)) if value.ordinals == (2..8))
        );
        assert_eq!(register.entries().count(), 2);
    }

    #[test]
    fn rejected_write_consumes_name_and_close_clears_every_slot() {
        let mut register = Register::default();
        for name in 'a'..='z' {
            register.select(name).unwrap();
            register.original_copied(original());
        }
        assert_eq!(register.entries().count(), 27);
        register.select('a').unwrap();
        assert_eq!(register.begin_write(), Some('a'));
        assert_eq!(register.selected(), None);
        assert_eq!(register.entries().count(), 27);
        register.select('z').unwrap();
        register.expect_cut(request(1));
        register.reconcile(None);
        assert_eq!(register.entries().count(), 0);
        assert_eq!(register.selected(), None);
        assert!(!register.is_pending());
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
