use super::*;
use crate::navigation::Pane;
use crate::project::marks::{Id, Location, ResolvedLocation, Saved};
use deadpan_core::{
    AssetId, ExactRatio, ProjectFrame, ProjectId, RevisionId, SourceQualificationId,
};

fn id(revision: &str) -> Id {
    Id {
        ticket: 0,
        session: 7,
        project: ProjectId::new("marks-test").unwrap(),
        revision: RevisionId::new(revision).unwrap(),
    }
}

fn edit_capture(revision: &str, frame: i64, pane: Pane) -> Capture {
    Capture {
        id: id(revision),
        location: Location::Edit {
            scope: SequenceScope::default(),
            at: ProjectFrame(frame),
            selected: None,
        },
        pane,
    }
}

fn original_capture(revision: &str, ordinal: u64, pane: Pane) -> Capture {
    Capture {
        id: id(revision),
        location: Location::Original {
            asset: AssetId::new("original").unwrap(),
            qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
            ordinal,
        },
        pane,
    }
}

fn fractional_edit(frame: i64, numerator: i128, denominator: i128) -> ResolvedLocation {
    ResolvedLocation::Edit {
        scope: SequenceScope::default(),
        selected: None,
        frame: ProjectFrame(frame),
        exact_frame: ExactRatio::new(numerator, denominator).unwrap(),
    }
}

#[test]
fn exact_edit_position_survives_pane_changes_and_expires_on_cursor_motion() {
    let sequence = edit_capture("revision-a", 20, Pane::Sequence);
    let viewer = Capture {
        pane: Pane::Viewer,
        ..sequence.clone()
    };
    let exact = fractional_edit(20, 41, 2);
    let mut state = State {
        exact: Some((sequence, exact.clone())),
        ..State::default()
    };

    state.navigated(Some(&viewer));
    assert_eq!(
        state.exact.as_ref().map(|(_, location)| location),
        Some(&exact)
    );

    let moved = edit_capture("revision-a", 21, Pane::Viewer);
    state.navigated(Some(&moved));
    assert!(state.exact.is_none());
}

#[test]
fn mark_only_revision_rebases_history_exact_position_and_open_mark_entry() {
    let original = original_capture("revision-a", 4, Pane::Sources);
    let edit = edit_capture("revision-a", 20, Pane::Sequence);
    let later_original = original_capture("revision-a", 9, Pane::Sources);
    let exact = fractional_edit(20, 41, 2);
    let mut state = State::default();
    state.history.jump(original.entry(), &edit.entry());
    state.history.jump(edit.entry(), &later_original.entry());
    state.history.complete(false, later_original.entry());
    state.exact = Some((edit.clone(), exact.clone()));
    state.entry = Some(Ok(edit.clone()));

    let mut saved_id = id("revision-a");
    saved_id.ticket = 12;
    let saved = Saved {
        id: saved_id,
        letter: 'a',
        revision: RevisionId::new("revision-b").unwrap(),
        refresh_error: None,
    };

    state.rebase(&saved);

    let backward = state.history.target(false).unwrap();
    let forward = state.history.target(true).unwrap();
    assert_eq!(backward.revision, saved.revision);
    assert_eq!(forward.revision, saved.revision);
    assert_eq!(state.exact.as_ref().unwrap().0.id.revision, saved.revision);
    assert_eq!(state.exact.as_ref().unwrap().1, exact);
    assert_eq!(
        state.entry.as_ref().unwrap().as_ref().unwrap().id.revision,
        saved.revision
    );
}

#[test]
fn losing_the_workspace_reconciles_away_all_history_and_exact_position() {
    let original = original_capture("revision-a", 4, Pane::Sources);
    let edit = edit_capture("revision-a", 20, Pane::Sequence);
    let later_original = original_capture("revision-a", 9, Pane::Sources);
    let mut state = State::default();
    state.history.jump(original.entry(), &edit.entry());
    state.history.jump(edit.entry(), &later_original.entry());
    state.history.complete(false, later_original.entry());
    state.exact = Some((edit.clone(), fractional_edit(20, 41, 2)));

    state.reconcile(None);

    assert!(state.history.target(false).is_none());
    assert!(state.history.target(true).is_none());
    assert!(state.exact.is_none());
}
