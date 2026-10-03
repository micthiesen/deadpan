use super::*;
use deadpan_core::FrameCut;

fn project() -> ProjectId {
    ProjectId::new("semantic-project").unwrap()
}
fn revision(id: &str) -> RevisionId {
    RevisionId::new(id).unwrap()
}
fn edit(count: u32) -> LastEdit {
    LastEdit {
        operation: FrameCut::new(count).unwrap(),
        register: Some('a'),
    }
}
fn observe(state: &mut State, id: &str) {
    state.observe(1, &project(), Ok(revision(id)));
}
fn proof(state: &mut State, before: &str, after: &str, change: Change) {
    state.prove(1, &project(), &revision(before), &revision(after), change);
}
fn saved() -> State {
    let mut state = State::default();
    observe(&mut state, "base");
    proof(&mut state, "base", "cut", Change::Replace(edit(7)));
    observe(&mut state, "cut");
    assert_eq!(state.snapshot().unwrap().edit, Some(edit(7)));
    state
}

#[test]
fn exact_proofs_preserve_parameters_through_history_and_marks() {
    let mut state = saved();
    for (before, after) in [("cut", "undo"), ("undo", "redo"), ("redo", "mark")] {
        let previous_version = state.snapshot().unwrap().version;
        proof(&mut state, before, after, Change::Preserve);
        observe(&mut state, after);
        let snapshot = state.snapshot().unwrap();
        assert_eq!(snapshot.edit, Some(edit(7)));
        assert!(snapshot.version > previous_version);
    }
    proof(&mut state, "mark", "cut-again", Change::Replace(edit(12)));
    observe(&mut state, "cut-again");
    assert_eq!(state.snapshot().unwrap().edit, Some(edit(12)));
}

#[test]
fn unknown_changes_clear_and_undo_cannot_resurrect_an_older_edit() {
    let mut state = saved();
    observe(&mut state, "unsupported");
    assert!(state.snapshot().unwrap().edit.is_none());
    proof(&mut state, "unsupported", "undo", Change::Preserve);
    observe(&mut state, "undo");
    assert!(state.snapshot().unwrap().edit.is_none());
}

#[test]
fn unobserved_transition_cannot_be_hidden_by_later_mark_or_cut_proof() {
    for change in [Change::Preserve, Change::Replace(edit(20))] {
        let mut state = saved();
        proof(&mut state, "hidden-edit", "later-edit", change);
        observe(&mut state, "later-edit");
        assert!(state.snapshot().unwrap().edit.is_none());
    }
}

#[test]
fn failed_and_duplicate_notifications_do_not_advance_or_reapply_intent() {
    let mut state = saved();
    let previous = state.snapshot();
    observe(&mut state, "cut");
    assert_eq!(state.snapshot(), previous);
    proof(&mut state, "base", "cut", Change::Replace(edit(99)));
    observe(&mut state, "cut");
    assert_eq!(state.snapshot(), previous);
    observe(&mut state, "unknown");
    assert!(state.snapshot().unwrap().edit.is_none());
}

#[test]
fn head_failure_discards_candidate_and_proof_until_a_new_supported_save() {
    let mut state = saved();
    proof(&mut state, "cut", "mark", Change::Preserve);
    state.observe(1, &project(), Err("database unavailable".into()));
    let failed = state.snapshot().unwrap();
    assert!(failed.head.is_none() && failed.edit.is_none() && failed.error.is_some());
    observe(&mut state, "mark");
    assert!(state.snapshot().unwrap().edit.is_none());
    proof(&mut state, "mark", "new-cut", Change::Replace(edit(3)));
    observe(&mut state, "new-cut");
    assert_eq!(state.snapshot().unwrap().edit, Some(edit(3)));
}

#[test]
fn session_replacement_and_close_clear_proofs_and_candidates() {
    let mut state = saved();
    proof(&mut state, "cut", "next", Change::Preserve);
    state.observe(2, &project(), Ok(revision("next")));
    assert!(state.snapshot().unwrap().edit.is_none());
    state.close();
    assert!(state.snapshot().is_none());
    observe(&mut state, "base");
    assert!(state.snapshot().unwrap().edit.is_none());
}

#[test]
fn final_version_is_always_unavailable_and_never_wraps() {
    let mut state = saved();
    state.version = u64::MAX - 1;
    proof(&mut state, "cut", "next", Change::Replace(edit(3)));
    observe(&mut state, "next");
    let exhausted = state.snapshot().unwrap();
    assert_eq!(exhausted.version, u64::MAX);
    assert!(exhausted.edit.is_none() && exhausted.error.is_some());
    proof(&mut state, "next", "later", Change::Replace(edit(4)));
    observe(&mut state, "later");
    assert_eq!(state.snapshot().unwrap().version, u64::MAX);
    assert!(state.snapshot().unwrap().edit.is_none());
}
