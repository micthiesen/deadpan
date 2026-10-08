//! A conservative, bounded context rejection heuristic, independent of shot
//! navigation. Every supplied signature represents one consecutive physical
//! source picture. Missing pictures, decoder failures and structural clock
//! jumps belong to the caller and must never be replaced with repeated samples.
//!
//! Absence of a detected transition is not proof of a single shot. Low-contrast
//! cuts and transitions outside the detector's supported lengths can be missed;
//! rapid motion and flashes can be rejected. This policy intentionally keeps
//! abrupt changes that navigation suppresses as flashes or nearby boundaries.

use std::ops::{Range, RangeInclusive};

use super::{
    GradualTransition, MAX_HALF_SPAN, MIN_CELL_CHANGE, PictureSignature, SIGNATURE_VERSION,
    ShotAnalysis, ShotError, gradual,
};

pub const CONTEXT_SHOT_RULE: &str = "deadpan-context-shots-1";
/// Includes the requested pictures and all real padding pictures.
pub const MAX_CONTEXT_SHOT_SIGNATURES: usize = 512;
/// A described fade can reach 2h from its candidate center; testing that center
/// reads signatures as far as 3h away. Thus 5 * max(h) covers every candidate
/// whose described range may touch the requested interval. Clip only at the
/// physical source's beginning/end, never an arbitrary decoder window edge.
pub const CONTEXT_SHOT_PADDING: usize = 5 * MAX_HALF_SPAN;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ContextShotCoverage {
    pub source_pictures: usize,
    pub padding_before: usize,
    pub padding_after: usize,
    pub starts_at_source_start: bool,
    pub ends_at_source_end: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextShotTransition {
    /// The first picture after an abrupt change. Both pictures of the change
    /// lie inside the requested interval, not merely in its padding.
    Abrupt {
        picture: usize,
        cell: u8,
        histogram: u8,
    },
    /// Absolute physical-source ordinals; the whole blended range is checked
    /// for overlap, including when its representative boundary lies outside.
    Gradual(GradualTransition),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ContextShotQualification {
    pub rule: &'static str,
    pub signature_version: &'static str,
    pub requested: RangeInclusive<usize>,
    /// Half-open absolute physical-source ordinals actually measured.
    pub measured: Range<usize>,
    pub checked_pictures: usize,
    pub coverage: ContextShotCoverage,
    /// `None` means no transition detected under this policy, not a guarantee.
    pub transition: Option<ContextShotTransition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ContextSeamChange {
    pub rule: &'static str,
    pub cell: u8,
    pub histogram: u8,
}

/// Compare the two actual pictures adjoining a structural/provider seam. This
/// only checks abrupt change: it cannot replace interval coverage or gradual
/// transition checks. A source clock jump remains the caller's explicit policy.
pub fn context_seam_change(
    before: &PictureSignature,
    after: &PictureSignature,
) -> Option<ContextSeamChange> {
    let [cell, histogram, _] = before.change(after, None);
    // Histogram and temporal suppression are deliberately not gates: equally
    // bright different scenes and one-picture flashes can still be unsafe.
    (cell >= MIN_CELL_CHANGE).then_some(ContextSeamChange {
        rule: CONTEXT_SHOT_RULE,
        cell,
        histogram,
    })
}

/// Required real-source coverage for an inclusive physical-ordinal interval.
/// The result can be used to bound decoding before allocating any signatures.
pub fn context_shot_window(
    requested: RangeInclusive<usize>,
    source_pictures: usize,
) -> Result<Range<usize>, ShotError> {
    if requested.is_empty() || *requested.end() >= source_pictures {
        return Err(ShotError::Invalid("context interval is outside the source"));
    }
    let start = requested.start().saturating_sub(CONTEXT_SHOT_PADDING);
    // end < source_pictures guarantees that the inclusive-to-exclusive +1 fits.
    let end = (requested.end() + 1)
        .saturating_add(CONTEXT_SHOT_PADDING)
        .min(source_pictures);
    if end - start > MAX_CONTEXT_SHOT_SIGNATURES {
        return Err(ShotError::Limit);
    }
    Ok(start..end)
}

/// Check all consecutive source signatures, with sufficient real padding.
/// `window_start` and `requested` use absolute physical source ordinals.
/// Padding shorter than the policy requires is valid only at actual source
/// edges supplied through `source_pictures`. False edge declarations cannot be
/// detected here: the caller must bind these facts to its verified source index.
pub fn qualify_context(
    signatures: &[PictureSignature],
    window_start: usize,
    source_pictures: usize,
    requested: RangeInclusive<usize>,
) -> Result<ContextShotQualification, ShotError> {
    if signatures.len() > MAX_CONTEXT_SHOT_SIGNATURES {
        return Err(ShotError::Limit);
    }
    let required = context_shot_window(requested.clone(), source_pictures)?;
    let window_end = window_start
        .checked_add(signatures.len())
        .filter(|end| *end <= source_pictures)
        .ok_or(ShotError::Invalid("context window is outside the source"))?;
    if window_start > required.start || window_end < required.end {
        return Err(ShotError::Invalid(
            "context window is missing required real padding",
        ));
    }
    let local_start = requested.start() - window_start;
    let local_end = requested.end() - window_start;
    let mut transition = (local_start + 1..=local_end).find_map(|picture| {
        context_seam_change(&signatures[picture - 1], &signatures[picture]).map(|change| {
            ContextShotTransition::Abrupt {
                picture: window_start + picture,
                cell: change.cell,
                histogram: change.histogram,
            }
        })
    });
    if transition.is_none() {
        let analysis = ShotAnalysis::from_signatures(signatures)?;
        transition = gradual::context_transitions(analysis.measures())
            .into_iter()
            .find(|candidate| {
                candidate.range.start <= local_end && candidate.range.end > local_start
            })
            .map(|candidate| {
                ContextShotTransition::Gradual(GradualTransition {
                    kind: candidate.kind,
                    range: candidate.range.start + window_start..candidate.range.end + window_start,
                    boundary: candidate.boundary + window_start,
                })
            });
    }
    Ok(ContextShotQualification {
        rule: CONTEXT_SHOT_RULE,
        signature_version: SIGNATURE_VERSION,
        requested: requested.clone(),
        measured: window_start..window_end,
        checked_pictures: signatures.len(),
        coverage: ContextShotCoverage {
            source_pictures,
            padding_before: requested.start() - window_start,
            padding_after: window_end - requested.end() - 1,
            starts_at_source_start: window_start == 0,
            ends_at_source_end: window_end == source_pictures,
        },
        transition,
    })
}

#[cfg(test)]
mod tests;
