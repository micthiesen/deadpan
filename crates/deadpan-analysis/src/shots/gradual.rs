//! Gradual transitions: dissolves and fades.
//!
//! A picture `k` with half-span `h` (one of [`HALF_SPANS`]) is a candidate
//! middle of a transition when
//!
//! 1. the pictures `h` before and `h` after it differ by a cell change
//!    (`across`) of at least 24, like a cut;
//! 2. it lies near their mean: `3 · residual ≤ across`, so the window is a
//!    blend rather than motion, a cut or a flash;
//! 3. `across` is at least twice the `across` of the windows of the same
//!    half-span ending where this one starts and starting where it ends
//!    (when they exist), so steady motion or a slow pan is not a transition;
//! 4. no consecutive cell change inside the window exceeds `across / 2` and
//!    no hard cut lies in it, so a cut is not also reported as a blend; and
//! 5. the window is complete: no wider window at the same picture has an
//!    `across` above `10/9` of this one, so the window already holds the
//!    whole change.
//!
//! Candidates are taken by increasing half-span, then residual, then picture,
//! and one is accepted when its window overlaps no accepted window and its
//! final blended range (below, after length estimation, clamping and the
//! fade adjustments) overlaps no accepted range. Its
//! length `n` follows from the change inside a linear blend:
//! `across(h') / plateau = 2h' / (n + 1)`, where `plateau` is the largest
//! change across any window at `k` and `h'` the widest smaller half-span whose
//! change is at most 3/4 of it; `n` is rounded and clamped to
//! [`MIN_TRANSITION_PICTURES`]..=[`MAX_TRANSITION_PICTURES`] and the window.
//! The blended pictures are `first..first + n`, centred on `k`. A candidate
//! whose `n` would fit the window two half-spans smaller (`n < 2h''`) is not
//! accepted: so short a blend completes in a smaller window, and a wide
//! window that passes alone has gathered motion instead (added 2026-10-05
//! after the real-footage run; see docs/SHOT_DETECTION.md).
//!
//! A picture is black when its mean luma is at most 6 and its luma spread at
//! most 4 (of 255). When some picture within `2h` after `k` is black, none
//! within `2h` before it is, and `k` itself is not, the transition is a fade
//! out ending at the first black picture after `k`; the boundary is that
//! picture, so the black pictures form their own shot. The mirror case is a
//! fade in starting after the last black picture before `k`, and the
//! boundary is its first picture. Otherwise it is a dissolve and the boundary
//! is its middle picture, `first + n / 2`.

use std::collections::BTreeMap;
use std::ops::Range;

use super::{
    HALF_SPANS, MAX_TRANSITION_PICTURES, MIN_CELL_CHANGE, MIN_TRANSITION_PICTURES, PictureMeasure,
};

pub(super) const BLACK_LUMA: u8 = 6;
pub(super) const BLACK_SPREAD: u8 = 4;
const LINEAR_RATIO: u32 = 3;
const PEAK_RATIO: u32 = 2;
/// A window is complete when `across · 10 ≥ widest_across · 9`.
const COMPLETE: (u32, u32) = (10, 9);
/// The span used to estimate a transition's length lies inside the blend:
/// `across · 4 ≤ plateau · 3`.
const INSIDE: (u32, u32) = (4, 3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// A blend of two shots.
    Dissolve,
    /// A blend into black pictures.
    FadeOut,
    /// A blend out of black pictures.
    FadeIn,
}

/// A gradual transition proposal: the blended pictures and the one picture
/// that begins the next shot.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GradualTransition {
    pub kind: TransitionKind,
    /// Half-open blended pictures.
    pub range: Range<usize>,
    pub boundary: usize,
}

struct Candidate {
    span: usize,
    residual: u8,
    center: usize,
    across: u8,
}

fn candidates(measures: &[PictureMeasure], cuts: &[usize]) -> Vec<Candidate> {
    let pictures = measures.len();
    let across = |center: usize, span: usize| -> Option<u8> {
        let half = HALF_SPANS[span];
        (center >= half && center + half < pictures).then(|| measures[center].spans[span][0])
    };
    let mut candidates = Vec::new();
    for (span, &half) in HALF_SPANS.iter().enumerate() {
        if pictures <= 2 * half {
            continue;
        }
        for center in half..pictures - half {
            let [value, residual] = measures[center].spans[span];
            if value < MIN_CELL_CHANGE || LINEAR_RATIO * u32::from(residual) > u32::from(value) {
                continue;
            }
            let around = center
                .checked_sub(2 * half)
                .and_then(|before| across(before, span))
                .unwrap_or(0)
                .max(across(center + 2 * half, span).unwrap_or(0));
            if u32::from(value) < PEAK_RATIO * u32::from(around) {
                continue;
            }
            let window = center - half + 1..center + half + 1;
            let step = measures[window.clone()]
                .iter()
                .map(|measure| measure.change[0])
                .max()
                .unwrap_or(0);
            let cut = cuts.partition_point(|cut| *cut < window.start);
            if 2 * u32::from(step) > u32::from(value)
                || cuts.get(cut).is_some_and(|cut| *cut < window.end)
            {
                continue;
            }
            // Complete: no wider window at this picture holds much more change.
            let widest = (span + 1..HALF_SPANS.len())
                .filter_map(|wider| across(center, wider))
                .max()
                .unwrap_or(0);
            if u32::from(value) * COMPLETE.0 >= u32::from(widest) * COMPLETE.1 {
                candidates.push(Candidate {
                    span,
                    residual,
                    center,
                    across: value,
                });
            }
        }
    }
    candidates.sort_by_key(|candidate| (candidate.span, candidate.residual, candidate.center));
    candidates
}

pub(super) fn transitions(measures: &[PictureMeasure], cuts: &[usize]) -> Vec<GradualTransition> {
    // Accepted windows and described ranges, each keyed by first picture;
    // neither ever overlaps another of its kind.
    let mut windows: BTreeMap<usize, usize> = BTreeMap::new();
    let mut accepted: BTreeMap<usize, GradualTransition> = BTreeMap::new();
    let overlaps = |taken: &BTreeMap<usize, usize>, start: usize, end: usize| {
        taken
            .range(..end)
            .next_back()
            .is_some_and(|(_, taken_end)| *taken_end > start)
    };
    for candidate in candidates(measures, cuts) {
        let half = HALF_SPANS[candidate.span];
        let (start, end) = (candidate.center - half, candidate.center + half + 1);
        if overlaps(&windows, start, end) {
            continue;
        }
        let length = estimate_length(measures, &candidate);
        if candidate
            .span
            .checked_sub(2)
            .is_some_and(|smaller| length < 2 * HALF_SPANS[smaller])
        {
            continue;
        }
        // A fade's range reaches past its window to the black pictures, so
        // the final range must not overlap an accepted one either.
        let transition = describe(measures, &candidate, length);
        let ranges = accepted
            .range(..transition.range.end)
            .next_back()
            .is_some_and(|(_, taken)| taken.range.end > transition.range.start);
        if ranges {
            continue;
        }
        windows.insert(start, end);
        accepted.insert(transition.range.start, transition);
    }
    accepted.into_values().collect()
}

/// The context guard examines every qualifying blend independently. Navigation
/// suppression must not hide a transition merely because a nearby candidate
/// was selected first. The caller has its own bounded measurement window.
pub(super) fn context_transitions(measures: &[PictureMeasure]) -> Vec<GradualTransition> {
    candidates(measures, &[])
        .into_iter()
        .filter_map(|candidate| {
            let length = estimate_length(measures, &candidate);
            if candidate
                .span
                .checked_sub(2)
                .is_some_and(|smaller| length < 2 * HALF_SPANS[smaller])
            {
                return None;
            }
            Some(describe(measures, &candidate, length))
        })
        .collect()
}

/// Blended pictures implied by the change across the windows at the
/// candidate's picture; see the module documentation.
fn estimate_length(measures: &[PictureMeasure], candidate: &Candidate) -> usize {
    let (center, half) = (candidate.center, HALF_SPANS[candidate.span]);
    let longest = (2 * half - 1).min(MAX_TRANSITION_PICTURES);
    let spans = &measures[center].spans;
    let plateau = u32::from(
        (0..HALF_SPANS.len())
            .filter(|span| {
                center >= HALF_SPANS[*span] && center + HALF_SPANS[*span] < measures.len()
            })
            .map(|span| spans[span][0])
            .max()
            .unwrap_or(candidate.across),
    );
    (0..candidate.span)
        .rev()
        .find(|inner| u32::from(spans[*inner][0]) * INSIDE.0 <= plateau * INSIDE.1)
        .filter(|inner| spans[*inner][0] > 0)
        .map_or(MIN_TRANSITION_PICTURES, |inner| {
            let inner_across = u32::from(spans[inner][0]);
            let scaled = 2 * HALF_SPANS[inner] as u32 * plateau;
            ((scaled + inner_across / 2) / inner_across).saturating_sub(1) as usize
        })
        .clamp(MIN_TRANSITION_PICTURES, longest)
}

fn describe(
    measures: &[PictureMeasure],
    candidate: &Candidate,
    length: usize,
) -> GradualTransition {
    let (center, half) = (candidate.center, HALF_SPANS[candidate.span]);
    let window = center - half + 1..center + half + 1;
    let mut first = (center + 1 - length.div_ceil(2)).max(window.start);
    let mut end = (first + length).min(window.end);
    // Black pictures within a window's width after or before the middle.
    let black = |picture: &usize| measures[*picture].is_black();
    let reach = 2 * half;
    let after = (center + 1..(center + reach + 1).min(measures.len())).find(black);
    let before = (center.saturating_sub(reach)..center).rev().find(black);
    match (before, after) {
        (None, Some(black)) if !measures[center].is_black() => {
            end = black;
            first = first
                .min(end - MIN_TRANSITION_PICTURES)
                .max(end - MAX_TRANSITION_PICTURES.min(end - 1));
            GradualTransition {
                kind: TransitionKind::FadeOut,
                range: first..end,
                boundary: end,
            }
        }
        (Some(black), None) if !measures[center].is_black() => {
            first = black + 1;
            end = end
                .max(first + MIN_TRANSITION_PICTURES)
                .min(first + MAX_TRANSITION_PICTURES);
            GradualTransition {
                kind: TransitionKind::FadeIn,
                range: first..end,
                boundary: first,
            }
        }
        _ => GradualTransition {
            kind: TransitionKind::Dissolve,
            range: first..end,
            boundary: first + (end - first) / 2,
        },
    }
}
