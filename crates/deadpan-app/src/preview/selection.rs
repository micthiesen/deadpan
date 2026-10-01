//! Current-scope beat selection uses absolute project-frame boundaries.

use super::BeatRow;

pub(super) fn commit_matches_visible(
    commit: &crate::project::CommittedEdit,
    session: u64,
    project: &deadpan_core::ProjectId,
    revision: &deadpan_core::RevisionId,
) -> bool {
    &commit.revision == revision
        && commit
            .range_selection
            .as_ref()
            .is_none_or(|range| range.session == session && &range.project == project)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Completion {
    Edit,
    Registration,
    None,
}

/// The retained edit marker also suppresses later import-driven view changes,
/// even when the UI already consumed that edit in a previous mailbox update.
pub(super) fn completion(
    committed: Option<&deadpan_core::RevisionId>,
    last: Option<&deadpan_core::RevisionId>,
    registered: bool,
) -> Completion {
    match committed {
        Some(revision) if Some(revision) != last => Completion::Edit,
        Some(_) => Completion::None,
        None if registered => Completion::Registration,
        None => Completion::None,
    }
}

pub(super) fn after_refresh(
    rows: &[BeatRow],
    selected: Option<&deadpan_core::NodeId>,
    cursor: u64,
) -> Option<usize> {
    rows.iter()
        .position(|row| Some(&row.id) == selected)
        .or_else(|| at_boundary(rows, cursor))
}

/// Select a committed visible node, or its visible containing group when it is
/// deeper than the captured navigation scope. Use the command's own boundary;
/// neither a moved UI cursor nor stale selection may retarget it.
pub(super) fn after_commit(
    rows: &[BeatRow],
    selected: Option<&deadpan_core::NodeId>,
    cursor: Option<deadpan_core::ProjectFrame>,
) -> Option<usize> {
    let selected = selected?;
    rows.iter().position(|row| &row.id == selected).or_else(|| {
        let cursor = u64::try_from(cursor?.0).ok()?;
        // Unlike navigation, a committed hidden target cannot clamp to the
        // last row when its supplied position is outside the visible sequence.
        rows.iter().position(|row| {
            row.start <= cursor
                && row
                    .start
                    .checked_add(row.frames)
                    .is_some_and(|end| cursor < end)
        })
    })
}

/// Entering chooses a boundary inside the new group. Leaving selects the exited
/// group but preserves the absolute heard cursor, even when audition left its
/// parent's extent before a context-preserving stop.
pub(super) fn after_scope_change(
    rows: &[BeatRow],
    bounds: std::ops::RangeInclusive<u64>,
    cursor: u64,
    exited: Option<&deadpan_core::NodeId>,
) -> (u64, Option<usize>) {
    let cursor = if exited.is_some() {
        cursor
    } else {
        cursor.clamp(*bounds.start(), *bounds.end())
    };
    (cursor, after_refresh(rows, exited, cursor))
}

pub(super) fn step(
    rows: &[BeatRow],
    selected: Option<&deadpan_core::NodeId>,
    cursor: u64,
    forward: bool,
    count: u32,
) -> Option<usize> {
    let current = after_refresh(rows, selected, cursor)?;
    Some(crate::navigation::boundary_step(
        current as u64,
        rows.len().saturating_sub(1) as u64,
        forward,
        count,
    ) as usize)
}

pub(super) fn at_boundary(rows: &[BeatRow], cursor: u64) -> Option<usize> {
    let last = rows.last()?;
    let end = last.start.checked_add(last.frames)?;
    if cursor >= end {
        return Some(rows.len() - 1);
    }
    rows.iter().position(|row| {
        row.start <= cursor
            && row
                .start
                .checked_add(row.frames)
                .is_some_and(|end| cursor < end)
    })
}

pub(super) fn split_boundary(
    rows: &[BeatRow],
    selected: &deadpan_core::NodeId,
    cursor: u64,
) -> Option<deadpan_core::FrameDuration> {
    let row = rows.iter().find(|row| &row.id == selected)?;
    let local = cursor.checked_sub(row.start)?;
    if local == 0 || local >= row.frames {
        return None;
    }
    deadpan_core::FrameDuration::new(i64::try_from(local).ok()?).ok()
}

/// Visual placement only. Exact frame values remain the source of the label;
/// this fraction never participates in authored timing or picture resolution.
pub(super) fn cursor_marker(rows: &[BeatRow], cursor: u64) -> Option<(usize, f32)> {
    let last = rows.last()?;
    if cursor > last.start.checked_add(last.frames)? {
        return None;
    }
    let index = at_boundary(rows, cursor)?;
    let row = &rows[index];
    if row.frames == 0 {
        return None;
    }
    let local = cursor.checked_sub(row.start)?.min(row.frames);
    Some((index, (local as f64 / row.frames as f64) as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::NodeId;

    #[test]
    fn range_receipt_requires_its_exact_visible_session_project_and_revision_once() {
        use crate::project::{CommittedEdit, CommittedRangeSelection, SequenceScope};
        use deadpan_core::{FrameRange, ProjectFrame, ProjectId, RevisionId};
        let project = ProjectId::new("project").unwrap();
        let revision = RevisionId::new("saved").unwrap();
        let commit = CommittedEdit {
            revision: revision.clone(),
            selected_node: Some(NodeId::new("first-moved-child").unwrap()),
            preserve_cursor: false,
            cursor: Some(ProjectFrame(20)),
            scope: SequenceScope::default(),
            sound: None,
            range_selection: Some(CommittedRangeSelection {
                session: 7,
                project: project.clone(),
                parent: NodeId::new("root").unwrap(),
                range: FrameRange::new(ProjectFrame(20), ProjectFrame(50)).unwrap(),
            }),
        };
        assert!(commit_matches_visible(&commit, 7, &project, &revision));
        assert!(!commit_matches_visible(&commit, 8, &project, &revision));
        assert!(!commit_matches_visible(
            &commit,
            7,
            &ProjectId::new("other").unwrap(),
            &revision
        ));
        for other in ["unrefreshed", "newer"] {
            assert!(!commit_matches_visible(
                &commit,
                7,
                &project,
                &RevisionId::new(other).unwrap()
            ));
        }
        assert_eq!(completion(Some(&revision), None, false), Completion::Edit);
        assert_eq!(
            completion(Some(&revision), Some(&revision), false),
            Completion::None
        );
    }

    #[test]
    fn returning_from_a_group_preserves_a_heard_cursor_outside_its_parent() {
        let mut rows = rows(&[10, 4]);
        for row in &mut rows {
            row.start += 5;
        }
        // Audition reached another root sibling; Backspace must select the
        // exited child without seeking back to this parent's end (19).
        let exited = &rows[0].id;
        assert_eq!(
            after_scope_change(&rows, 5..=19, 31, Some(exited)),
            (31, Some(0))
        );
        assert_eq!(
            after_scope_change(&rows, 5..=19, 2, Some(exited)),
            (2, Some(0))
        );
        assert_eq!(
            after_scope_change(&rows, 5..=19, 17, Some(exited)),
            (17, Some(0))
        );
        // Entry may have a cursor outside the selected group's extent.
        assert_eq!(after_scope_change(&rows, 5..=19, 31, None), (19, Some(1)));
        assert_eq!(after_scope_change(&rows, 5..=19, 2, None), (5, Some(0)));
        assert_eq!(after_scope_change(&[], 12..=12, 31, None), (12, None));
    }

    #[test]
    fn nested_rows_keep_absolute_cursor_and_local_split_coordinates() {
        let mut rows = rows(&[4, 0, 6]);
        for row in &mut rows {
            row.start += 17;
        }
        assert_eq!(at_boundary(&rows, 16), None);
        assert_eq!(at_boundary(&rows, 17), Some(0));
        assert_eq!(at_boundary(&rows, 21), Some(2));
        assert_eq!(at_boundary(&rows, 27), Some(2));
        assert_eq!(step(&rows, Some(&rows[0].id), 18, true, 1), Some(1));
        assert_eq!(split_boundary(&rows, &rows[2].id, 24).unwrap().frames(), 3);
        assert!(split_boundary(&rows, &rows[2].id, 21).is_none());
        assert_eq!(after_refresh(&rows, Some(&rows[2].id), 18), Some(2));
        assert_eq!(
            after_refresh(&rows, Some(&NodeId::new("removed").unwrap()), 18),
            Some(0)
        );
        assert!(cursor_marker(&rows, 16).is_none());
        assert!(cursor_marker(&rows, 28).is_none());
    }

    fn rows(durations: &[u64]) -> Vec<BeatRow> {
        let mut start = 0;
        durations
            .iter()
            .enumerate()
            .map(|(index, frames)| {
                let row = BeatRow {
                    id: NodeId::new(index.to_string()).unwrap(),
                    label: index.to_string(),
                    kind: "Hold".into(),
                    start,
                    frames: *frames,
                };
                start += frames;
                row
            })
            .collect()
    }

    #[test]
    fn committed_nested_pause_selects_its_visible_group_at_the_captured_boundary() {
        use deadpan_core::ProjectFrame;
        let rows = rows(&[2, 7, 3]);
        let hold = NodeId::new("nested-hold").unwrap();
        assert_eq!(
            after_commit(&rows, Some(&hold), Some(ProjectFrame(4))),
            Some(1)
        );
        assert_eq!(
            after_commit(&rows, Some(&hold), Some(ProjectFrame(9))),
            Some(2)
        );
        for cursor in [
            None,
            Some(ProjectFrame(-1)),
            Some(ProjectFrame(12)),
            Some(ProjectFrame(100)),
        ] {
            assert_eq!(after_commit(&rows, Some(&hold), cursor), None);
        }
        assert_eq!(after_commit(&rows, None, Some(ProjectFrame(4))), None);
        assert_eq!(after_commit(&rows, Some(&rows[1].id), None), Some(1));
    }

    #[test]
    fn split_captures_only_an_interior_boundary_of_the_selected_beat() {
        assert_eq!(
            split_boundary(&rows(&[u64::MAX]), &NodeId::new("0").unwrap(), u64::MAX - 1),
            None
        );
        let rows = rows(&[3, 4, 0, 2]);
        for (cursor, expected) in [
            (0, None),
            (3, None),
            (4, Some(1)),
            (6, Some(3)),
            (7, None),
            (100, None),
        ] {
            assert_eq!(
                split_boundary(&rows, &rows[1].id, cursor).map(|at| at.frames()),
                expected
            );
        }
        assert_eq!(split_boundary(&rows, &rows[2].id, 7), None);
        assert_eq!(
            split_boundary(&rows, &NodeId::new("missing").unwrap(), 4),
            None
        );
    }

    #[test]
    fn cursor_crossings_choose_the_right_hand_root_beat() {
        let rows = rows(&[3, 4, 2]);
        for (boundary, expected) in [
            (0, 0),
            (2, 0),
            (3, 1),
            (6, 1),
            (7, 2),
            (8, 2),
            (9, 2),
            (100, 2),
        ] {
            assert_eq!(at_boundary(&rows, boundary), Some(expected));
        }
    }

    #[test]
    fn empty_and_zero_duration_structures_have_no_invented_frame() {
        assert_eq!(at_boundary(&[], 0), None);
        assert_eq!(at_boundary(&rows(&[0, 3, 0, 2]), 0), Some(1));
        assert_eq!(at_boundary(&rows(&[0, 3, 0, 2]), 3), Some(3));
        assert_eq!(at_boundary(&rows(&[0, 0]), 0), Some(1));
    }

    #[test]
    fn duration_changes_and_deletion_recompute_selection_from_committed_rows() {
        assert_eq!(at_boundary(&rows(&[3, 4]), 4), Some(1));
        assert_eq!(at_boundary(&rows(&[6, 4]), 4), Some(0));
        assert_eq!(at_boundary(&rows(&[3]), 4), Some(0));
        assert_eq!(at_boundary(&[], 4), None);
    }

    #[test]
    fn explicit_empty_row_survives_refresh_but_frame_motion_reselects() {
        let rows = rows(&[3, 0, 4]);
        assert_eq!(after_refresh(&rows, Some(&rows[1].id), 3), Some(1));
        assert_eq!(at_boundary(&rows, 3), Some(2));
        assert_eq!(at_boundary(&rows, 4), Some(2));
        let hidden = NodeId::new("hidden-child").unwrap();
        assert_eq!(after_refresh(&rows, Some(&hidden), 3), Some(2));
        assert_eq!(after_refresh(&[], Some(&hidden), 0), None);
    }

    #[test]
    fn beat_navigation_visits_empty_rows_in_both_directions_without_skipping() {
        let rows = rows(&[3, 0, 4, 2]);
        let mut index = 0;
        for expected in [1, 2, 3, 3] {
            index = step(&rows, Some(&rows[index].id), rows[index].start, true, 1).unwrap();
            assert_eq!(index, expected);
        }
        for expected in [2, 1, 0, 0] {
            index = step(&rows, Some(&rows[index].id), rows[index].start, false, 1).unwrap();
            assert_eq!(index, expected);
        }
        assert_eq!(step(&rows, Some(&rows[0].id), 0, true, 2), Some(2));
        assert_eq!(step(&rows, None, 3, false, 1), Some(1));
    }

    #[test]
    fn split_or_coalesced_import_completion_cannot_override_an_edit() {
        let revision = deadpan_core::RevisionId::new("committed").unwrap();
        assert_eq!(completion(Some(&revision), None, true), Completion::Edit);
        assert_eq!(completion(Some(&revision), None, false), Completion::Edit);
        assert_eq!(
            completion(Some(&revision), Some(&revision), true),
            Completion::None
        );
        assert_eq!(
            completion(Some(&revision), Some(&revision), false),
            Completion::None
        );
        // The next user request clears the service marker, permitting a newly
        // requested import to select its registered source normally.
        assert_eq!(
            completion(None, Some(&revision), true),
            Completion::Registration
        );
    }

    #[test]
    fn cursor_marker_maps_only_within_its_own_structural_card() {
        let rows = rows(&[132, 11, 72]);
        let (index, fraction) = cursor_marker(&rows, 137).unwrap();
        assert_eq!(index, 1);
        assert!((fraction - 5.0 / 11.0).abs() < 0.00001);
        assert_eq!(cursor_marker(&rows, 132), Some((1, 0.0)));
        assert_eq!(cursor_marker(&rows, 215), Some((2, 1.0)));
        assert_eq!(cursor_marker(&rows, 216), None);
        assert_eq!(cursor_marker(&[], 0), None);
    }

    #[test]
    fn empty_beat_has_no_invented_cursor_progress() {
        assert_eq!(cursor_marker(&rows(&[0, 0]), 0), None);
        assert_eq!(cursor_marker(&rows(&[3, 0, 4]), 3), Some((2, 0.0)));
    }
}
