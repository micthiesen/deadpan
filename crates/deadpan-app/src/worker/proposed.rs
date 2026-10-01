//! A privately compiled picture plan for one genuinely admitted proposal.

use super::*;

use super::slice_view::PlanIdentity;

pub(super) fn admit<'a>(
    base: &Arc<Workspace>,
    snapshot: &Arc<deadpan_playback::Snapshot>,
    retained: &'a mut Option<PlanCache>,
    cancelled: &AtomicBool,
) -> Result<&'a RenderPlan, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Proposed picture was cancelled.".into());
    }
    snapshot
        .validate_original_proposal()
        .map_err(|error| error.to_string())?;
    snapshot
        .validate_proposed_base(base.session, &base.document)
        .map_err(|error| error.to_string())?;
    let cached = retained.as_ref().is_some_and(|previous| {
        matches!(&previous.identity, PlanIdentity::Original { base: old_base, snapshot: old_snapshot }
            if Arc::ptr_eq(base, old_base) && Arc::ptr_eq(snapshot, old_snapshot))
    });
    // Public source fields may not substitute another receipt after admission.
    // Actual media reads always use the committed workspace's handles/catalog.
    if !cached
        && (snapshot.sources.len() != base.sources.len()
            || snapshot.sources.iter().any(|(asset, entry)| {
                base.sources.get(asset).is_none_or(|registered| {
                    !Arc::ptr_eq(&entry.receipt, &registered.receipt)
                        || entry.original != registered.original
                })
            }))
    {
        return Err("Proposed picture source evidence differs from its committed base.".into());
    }
    if !cached {
        // Compilation is on the preview worker and happens once per admitted
        // snapshot. Metadata alone cannot authenticate a caller-supplied plan.
        let plan = RenderPlan::compile(&snapshot.document).map_err(|error| error.to_string())?;
        if cancelled.load(Ordering::Acquire) {
            return Err("Proposed picture was cancelled.".into());
        }
        *retained = Some(PlanCache {
            identity: PlanIdentity::Original {
                base: base.clone(),
                snapshot: snapshot.clone(),
            },
            plan,
        });
    }
    Ok(&retained.as_ref().expect("admitted proposal plan").plan)
}
