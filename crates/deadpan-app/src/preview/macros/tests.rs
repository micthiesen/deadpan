use super::*;
use crate::project::CommittedEdit;
use deadpan_core::ProjectId;

fn applied(committed: bool) -> protocol::Receipt {
    let selected = Some(NodeId::new("selected-empty-group").unwrap());
    protocol::Receipt {
        id: protocol::Id {
            session: 7,
            project: ProjectId::new("project").unwrap(),
            revision: RevisionId::new("entry").unwrap(),
            bank_version: 4,
            request: 12,
        },
        bank_version: if committed { 4 } else { 5 },
        outcome: protocol::Outcome::Applied {
            scope: SequenceScope::default(),
            cursor: ProjectFrame(11),
            selected: selected.clone(),
            visual_selection: None,
            committed: committed.then(|| {
                Box::new(CommittedEdit {
                    revision: RevisionId::new("pasted").unwrap(),
                    selected_node: selected,
                    preserve_cursor: false,
                    cursor: Some(ProjectFrame(11)),
                    scope: SequenceScope::default(),
                    sound: None,
                    range_selection: None,
                })
            }),
            refresh_error: None,
        },
    }
}

#[test]
fn bank_only_recorded_yank_requires_its_new_bank_in_the_same_visible_revision() {
    let receipt = applied(false);
    let visible = Some((
        receipt.id.session,
        &receipt.id.project,
        &receipt.id.revision,
    ));
    assert!(completion_visible(&receipt, visible, Some(5)));
    for bank in [None, Some(4), Some(6)] {
        assert!(!completion_visible(&receipt, visible, bank));
    }
    assert!(receipt.committed().is_none());
    assert!(!completion_visible(&receipt, None, Some(5)));
    let foreign = ProjectId::new("other").unwrap();
    let revision = RevisionId::new("later").unwrap();
    for visible in [
        Some((8, &receipt.id.project, &receipt.id.revision)),
        Some((7, &foreign, &receipt.id.revision)),
        Some((7, &receipt.id.project, &revision)),
    ] {
        assert!(!completion_visible(&receipt, visible, Some(5)));
    }
}

#[test]
fn recorded_paste_requires_the_authored_receipt_revision_even_when_cursor_did_not_move() {
    let receipt = applied(true);
    let commit = receipt.committed().unwrap();
    assert!(completion_visible(
        &receipt,
        Some((7, &receipt.id.project, &commit.revision)),
        Some(4),
    ));
    assert!(!completion_visible(
        &receipt,
        Some((7, &receipt.id.project, &receipt.id.revision)),
        Some(4),
    ));
    assert!(!completion_visible(
        &receipt,
        Some((7, &receipt.id.project, &commit.revision)),
        Some(5),
    ));
}

#[test]
fn a_bank_only_named_call_uses_the_same_receipt_admission_as_a_recorded_yank() {
    let mut receipt = applied(false);
    receipt.outcome = protocol::Outcome::Executed {
        register: 'a',
        count: 3,
        scope: SequenceScope::default(),
        cursor: ProjectFrame(11),
        selected: None,
        visual_selection: None,
        committed: None,
        refresh_error: None,
    };
    let visible = Some((7, &receipt.id.project, &receipt.id.revision));
    assert!(completion_visible(&receipt, visible, Some(5)));
    assert!(!completion_visible(&receipt, visible, Some(4)));
}

#[test]
fn selection_only_completion_requires_its_original_revision_and_bank() {
    let mut receipt = applied(false);
    receipt.bank_version = receipt.id.bank_version;
    if let protocol::Outcome::Applied {
        visual_selection, ..
    } = &mut receipt.outcome
    {
        *visual_selection = Some(deadpan_core::SemanticVisualSelection {
            anchor: ProjectFrame(11),
            head: ProjectFrame(11),
            extending: true,
        });
    }
    let visible = Some((7, &receipt.id.project, &receipt.id.revision));
    assert!(completion_visible(&receipt, visible, Some(4)));
    assert!(!completion_visible(&receipt, visible, Some(5)));
    assert!(!completion_visible(
        &receipt,
        Some((7, &receipt.id.project, &RevisionId::new("later").unwrap())),
        Some(4)
    ));
    assert!(receipt.committed().is_none());
}

#[test]
fn a_project_switch_retains_recorded_command_ownership_until_the_next_command_entry() {
    let mut state = State {
        command_recording: true,
        ..Default::default()
    };
    state.session_changed();
    assert!(state.command_recording);
    assert!(!state.recording() && !state.is_pending());
    state.capture_command();
    assert!(!state.command_recording);
}
