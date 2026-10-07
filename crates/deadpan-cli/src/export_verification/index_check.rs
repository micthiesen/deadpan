//! Neighbor discrimination at the declared encoder-fidelity bound. Only
//! reference planes enter this decision: a wrong decoded picture cannot enlarge
//! its own noise allowance and hide an otherwise distinguishable index shift.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexObservability {
    /// The reference luma error balls are disjoint at the declared fidelity.
    /// This is not a claim that either source identity was decoded exactly.
    Observable,
    /// Both source identities can produce the same luma within that fidelity.
    Unobservable,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct NeighborIndexCheck {
    pub status: IndexObservability,
    /// RMS distance between the two reference luma planes, in code values.
    pub reference_luma_rms: f64,
    /// Per-reference error bound derived from the declared minimum luma PSNR.
    /// Separation must exceed twice this value for an observable comparison.
    pub max_error_luma_rms: f64,
}

pub(super) fn compare_references(
    reference: &[u16],
    neighbor: &[u16],
    peak: u16,
    min_luma_psnr_db: f64,
) -> NeighborIndexCheck {
    debug_assert_eq!(reference.len(), neighbor.len());
    let squared = reference
        .iter()
        .zip(neighbor)
        .map(|(&left, &right)| {
            let difference = u64::from(left.abs_diff(right));
            difference * difference
        })
        .sum::<u64>();
    let reference_luma_rms = (squared as f64 / reference.len().max(1) as f64).sqrt();
    let max_error_luma_rms = f64::from(peak) * 10_f64.powf(-min_luma_psnr_db / 20.0);
    NeighborIndexCheck {
        // At equality the two closed error balls still touch. Identical or
        // nearby references cannot establish which source identity was encoded.
        status: if reference_luma_rms > 2.0 * max_error_luma_rms {
            IndexObservability::Observable
        } else {
            IndexObservability::Unobservable
        },
        reference_luma_rms,
        max_error_luma_rms,
    }
}
