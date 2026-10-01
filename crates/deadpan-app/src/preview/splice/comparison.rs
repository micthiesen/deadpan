//! Site-relative comparison in absolute Edit clocks. Root sounds keep their
//! global positions; this mapping makes no claim about individual sound owners.

use deadpan_core::{FrameRate, ProjectFrame};

use crate::project::splice::Movement;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Site {
    Removal,
    Insertion,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Pair {
    pub saved: (i64, i64),
    pub proposed: (i64, i64),
}

impl Pair {
    pub fn side(self, before: bool) -> (i64, i64) {
        if before { self.saved } else { self.proposed }
    }

    pub fn samples(self, rate: FrameRate) -> Result<Self, String> {
        let convert = |(a, b)| -> Result<_, String> {
            Ok((
                rate.audio_boundary(ProjectFrame(a))
                    .map_err(|e| e.to_string())?
                    .0,
                rate.audio_boundary(ProjectFrame(b))
                    .map_err(|e| e.to_string())?
                    .0,
            ))
        };
        Ok(Self {
            saved: convert(self.saved)?,
            proposed: convert(self.proposed)?,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Comparison {
    pub affected: Pair,
    limits: Pair,
    totals: Pair,
    pub timing_unchanged: bool,
}

impl Comparison {
    pub fn frame_context(self, radius: i64) -> Pair {
        let context = |range: (i64, i64), limits: (i64, i64)| {
            (
                range.0.saturating_sub(radius).max(limits.0),
                range.1.saturating_add(radius).min(limits.1),
            )
        };
        Pair {
            saved: context(self.affected.saved, self.limits.saved),
            proposed: context(self.affected.proposed, self.limits.proposed),
        }
    }

    pub fn new(
        inserted: (i64, i64),
        replaced: Option<(i64, i64)>,
        movement: Option<&Movement>,
        site: Site,
        saved_frames: i64,
        proposed_frames: i64,
    ) -> Self {
        let totals = Pair {
            saved: (0, saved_frames),
            proposed: (0, proposed_frames),
        };
        let mut result = Self {
            affected: Pair {
                saved: replaced.unwrap_or((inserted.0, inserted.0)),
                proposed: inserted,
            },
            limits: totals,
            totals,
            timing_unchanged: false,
        };
        if let Some(movement) = movement {
            let (a, b) = (
                movement.source_before.start().0,
                movement.source_before.end().0,
            );
            let d = movement.destination_before.0;
            let join = movement.removal_after.0;
            if d == a || d == b {
                // Reparenting can change inherited treatments without changing
                // timing. Compare the same complete interval on both sides.
                result.affected = Pair {
                    saved: (a, b),
                    proposed: (a, b),
                };
                result.timing_unchanged = true;
            } else {
                result.affected = match site {
                    Site::Removal => Pair {
                        saved: (a, b),
                        proposed: (join, join),
                    },
                    Site::Insertion => Pair {
                        saved: (d, d),
                        proposed: inserted,
                    },
                };
                // Do not inspect through the other edit site. Each cap belongs
                // to its own complete before/after clock.
                match (site, d < a) {
                    (Site::Removal, true) => {
                        result.limits.saved.0 = d;
                        result.limits.proposed.0 = inserted.1;
                    }
                    (Site::Insertion, true) => {
                        result.limits.saved.1 = a;
                        result.limits.proposed.1 = b;
                    }
                    (Site::Removal, false) => {
                        result.limits.saved.1 = d;
                        result.limits.proposed.1 = inserted.0;
                    }
                    (Site::Insertion, false) => {
                        result.limits.saved.0 = b;
                        result.limits.proposed.0 = a;
                    }
                }
            }
        }
        result
    }

    pub fn windows(self, rate: FrameRate, lead: i64, follow: i64) -> Result<(Pair, bool), String> {
        let affected = self.affected.samples(rate)?;
        let limits = self.limits.samples(rate)?;
        let totals = self.totals.samples(rate)?;
        let window = |range: (i64, i64), cap: (i64, i64)| {
            (
                range.0.saturating_sub(lead.max(0)).max(cap.0),
                range.1.saturating_add(follow.max(0)).min(cap.1),
            )
        };
        let windows = Pair {
            saved: window(affected.saved, limits.saved),
            proposed: window(affected.proposed, limits.proposed),
        };
        let shortened = windows.saved != window(affected.saved, totals.saved)
            || windows.proposed != window(affected.proposed, totals.proposed);
        Ok((windows, shortened))
    }
}

/// Prefixes and suffixes keep their offset from this site's appropriate join.
/// The exact empty seam maps to the counterpart's start, not its suffix.
pub(super) fn map_comparison(value: i64, from: (i64, i64), to: (i64, i64)) -> Result<i64, String> {
    if from.0 > from.1 || to.0 > to.1 {
        return Err("Invalid comparison interval".into());
    }
    if value <= from.0 {
        return value
            .checked_sub(from.0)
            .and_then(|offset| to.0.checked_add(offset))
            .ok_or_else(|| "Comparison coordinate overflow".into());
    }
    if value >= from.1 {
        return value
            .checked_sub(from.1)
            .and_then(|offset| to.1.checked_add(offset))
            .ok_or_else(|| "Comparison coordinate overflow".into());
    }
    let offset = value
        .checked_sub(from.0)
        .ok_or("Comparison offset overflow")?;
    let retained =
        to.1.checked_sub(to.0)
            .ok_or("Comparison duration overflow")?;
    to.0.checked_add(offset.min(retained.saturating_sub(1).max(0)))
        .ok_or_else(|| "Comparison coordinate overflow".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{FrameRange, NodeId};

    fn moved(a: i64, b: i64, d: i64) -> Movement {
        Movement {
            source_parent: NodeId::new("source").unwrap(),
            source_before: FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap(),
            destination_before: ProjectFrame(d),
            removal_after: ProjectFrame(if d < a { b } else { a }),
        }
    }

    #[test]
    fn independent_move_sites_map_prefixes_interiors_suffixes_and_empty_seams() {
        let movement = moved(20, 30, 60);
        let insertion = Comparison::new((50, 60), None, Some(&movement), Site::Insertion, 120, 120);
        let removal = Comparison::new((50, 60), None, Some(&movement), Site::Removal, 120, 120);
        for (pair, witnesses) in [
            (insertion.affected, vec![(59, 49), (60, 50), (61, 61)]),
            (
                removal.affected,
                vec![(19, 19), (20, 20), (29, 20), (30, 20), (31, 21)],
            ),
        ] {
            for (old, new) in witnesses {
                assert_eq!(map_comparison(old, pair.saved, pair.proposed).unwrap(), new);
            }
        }
        let movement = moved(70, 80, 10);
        let removal = Comparison::new((10, 20), None, Some(&movement), Site::Removal, 120, 120);
        assert_eq!(
            map_comparison(69, removal.affected.saved, removal.affected.proposed).unwrap(),
            79
        );
        assert_eq!(
            map_comparison(81, removal.affected.saved, removal.affected.proposed).unwrap(),
            81
        );
    }

    #[test]
    fn context_stops_at_other_site_in_both_directions() {
        let rate = FrameRate::new(30, 1).unwrap();
        for (movement, inserted, expected) in [
            (
                moved(20, 30, 60),
                (50, 60),
                [((0, 60), (0, 50)), ((30, 120), (20, 120))],
            ),
            (
                moved(70, 80, 10),
                (10, 20),
                [((10, 120), (20, 120)), ((0, 70), (0, 80))],
            ),
        ] {
            for (index, site) in [Site::Removal, Site::Insertion].into_iter().enumerate() {
                let comparison = Comparison::new(inserted, None, Some(&movement), site, 120, 120);
                let (windows, shortened) = comparison.windows(rate, 1_000_000, 1_000_000).unwrap();
                let (saved, proposed) = expected[index];
                assert_eq!(windows.saved, (saved.0 * 1600, saved.1 * 1600));
                assert_eq!(windows.proposed, (proposed.0 * 1600, proposed.1 * 1600));
                assert!(shortened);
            }
        }
    }

    #[test]
    fn boundary_reparenting_keeps_identical_frames_and_samples() {
        let rate = FrameRate::new(30_000, 1001).unwrap();
        for d in [2, 4] {
            for site in [Site::Removal, Site::Insertion] {
                let movement = moved(2, 4, d);
                let comparison = Comparison::new((2, 4), None, Some(&movement), site, 8, 8);
                assert!(comparison.timing_unchanged);
                let pair = comparison.affected.samples(rate).unwrap();
                for sample in 0..12_000 {
                    assert_eq!(
                        map_comparison(sample, pair.saved, pair.proposed).unwrap(),
                        sample
                    );
                }
            }
        }
    }

    #[test]
    fn ntsc_move_uses_complete_absolute_boundaries() {
        let rate = FrameRate::new(30_000, 1001).unwrap();
        let movement = moved(2, 4, 6);
        let pair = Comparison::new((4, 6), None, Some(&movement), Site::Insertion, 10, 10)
            .affected
            .samples(rate)
            .unwrap();
        assert_eq!(pair.saved, (9610, 9610));
        assert_eq!(pair.proposed, (6406, 9610));
        assert_eq!(
            map_comparison(9510, pair.saved, pair.proposed).unwrap(),
            6306
        );
        assert_eq!(
            map_comparison(9610, pair.saved, pair.proposed).unwrap(),
            6406
        );
    }
}
