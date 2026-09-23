//! Root-beat selection follows the requested project-frame boundary.

use super::BeatRow;

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

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::NodeId;

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
}
