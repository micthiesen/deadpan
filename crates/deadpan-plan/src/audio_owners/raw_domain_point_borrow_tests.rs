//! Focused immutable-corpus check for the borrowed Point signal surface.
use super::*;

pub(super) fn assert_matches_frozen(
    plan: &RenderPlan,
    value: &AudioBoundDomain<'_>,
    row: &Value,
) -> bool {
    let AudioBoundDomain::Point(point) = value else {
        return false;
    };
    let borrowed = point.signal_ref();
    let full_point = point.probe_baseline_snapshot(plan);
    let borrowed_snapshot = borrowed.probe_baseline_snapshot(plan);
    assert_eq!(full_point["structural_signal"], borrowed_snapshot);
    assert_eq!(
        borrowed_snapshot,
        row["ordinary"]["result"]["raw_domain"]["actual_private_point"]["structural_signal"],
        "borrowed signal must preserve every frozen Point payload field"
    );
    true
}
