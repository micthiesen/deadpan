//! A privately compiled picture plan for one genuinely admitted proposal.

use super::*;

pub(super) struct ProposedPlan {
    base: Arc<Workspace>,
    snapshot: Arc<deadpan_playback::Snapshot>,
    plan: RenderPlan,
}

pub(super) fn admit<'a>(
    base: &Arc<Workspace>,
    snapshot: &Arc<deadpan_playback::Snapshot>,
    retained: &'a mut Option<ProposedPlan>,
    cancelled: &AtomicBool,
) -> Result<&'a RenderPlan, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Proposed picture was cancelled.".into());
    }
    snapshot
        .validate_proposed_base(base.session, &base.document)
        .map_err(|error| error.to_string())?;
    // Public source fields may not substitute another receipt after admission.
    // Actual media reads always use the committed workspace's handles/catalog.
    if snapshot.sources.len() != base.sources.len()
        || snapshot.sources.iter().any(|(asset, entry)| {
            base.sources.get(asset).is_none_or(|registered| {
                !Arc::ptr_eq(&entry.receipt, &registered.receipt)
                    || entry.original != registered.original
            })
        })
    {
        return Err("Proposed picture source evidence differs from its committed base.".into());
    }
    if retained.as_ref().is_none_or(|previous| {
        !Arc::ptr_eq(base, &previous.base) || !Arc::ptr_eq(snapshot, &previous.snapshot)
    }) {
        // Compilation is on the preview worker and happens once per admitted
        // snapshot. Metadata alone cannot authenticate a caller-supplied plan.
        let plan = RenderPlan::compile(&snapshot.document).map_err(|error| error.to_string())?;
        if cancelled.load(Ordering::Acquire) {
            return Err("Proposed picture was cancelled.".into());
        }
        *retained = Some(ProposedPlan {
            base: base.clone(),
            snapshot: snapshot.clone(),
            plan,
        });
    }
    Ok(&retained.as_ref().expect("admitted proposal plan").plan)
}
