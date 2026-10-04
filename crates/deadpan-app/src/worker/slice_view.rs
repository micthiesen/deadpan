//! Picture plans and media reads for exact store-admitted edited views.

use super::*;

pub(super) enum PlanIdentity {
    Original {
        base: Arc<Workspace>,
        snapshot: Arc<deadpan_playback::Snapshot>,
    },
    Edited {
        base: Arc<Workspace>,
        snapshot: Arc<deadpan_playback::Snapshot>,
        media: Arc<MediaView>,
    },
    Copied(Arc<CopiedView>),
    Candidate(Arc<crate::project::generation::CandidatePreview>),
}

pub(super) struct PlanCache {
    pub identity: PlanIdentity,
    pub plan: RenderPlan,
}

/// Read capabilities only. A derived view never pretends to own history.
#[derive(Clone, Copy)]
pub(super) enum PictureMedia<'a> {
    Committed(&'a Workspace),
    Slice(&'a MediaView),
}

impl<'a> PictureMedia<'a> {
    pub fn session(self) -> u64 {
        match self {
            Self::Committed(workspace) => workspace.session,
            Self::Slice(media) => media.session(),
        }
    }

    pub fn sources(self) -> &'a std::collections::BTreeMap<AssetId, Arc<RegisteredSource>> {
        match self {
            Self::Committed(workspace) => &workspace.sources,
            Self::Slice(media) => media.sources(),
        }
    }

    pub fn originals(self) -> &'a deadpan_store::original_media::OriginalImportHandle {
        match self {
            Self::Committed(workspace) => &workspace.originals,
            Self::Slice(media) => media.admitted().originals(),
        }
    }

    pub fn generated(self) -> &'a deadpan_store::generated_media::GeneratedReadHandle {
        match self {
            Self::Committed(workspace) => &workspace.generated,
            Self::Slice(media) => media.admitted().generated(),
        }
    }

    pub fn check_slice_live(self, cancelled: &AtomicBool) -> Result<(), String> {
        match self {
            Self::Committed(_) => Ok(()),
            Self::Slice(media) => media
                .admitted()
                .check_live(cancelled)
                .map_err(|error| error.to_string()),
        }
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static CATALOG_CONTRACT_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn check_catalog(media: &MediaView) -> Result<(), String> {
    let admitted = media.admitted();
    if media.sources().len() != admitted.sources().len() {
        return Err("Edited picture catalog differs from its store admission.".into());
    }
    for (asset, source) in admitted.sources() {
        let registered = media
            .sources()
            .get(asset)
            .ok_or("Edited picture catalog is missing an admitted source.")?;
        if !Arc::ptr_eq(&source.receipt, &registered.receipt)
            || source.original != registered.original
            || registered.asset != *asset
        {
            return Err("Edited picture catalog differs from its store admission.".into());
        }
        let authored = admitted
            .document()
            .assets()
            .get(asset)
            .ok_or("Edited picture document is missing an admitted asset.")?;
        // Reconstructing a complete receipt scans its measured audio index.
        // Do this only when admitting a new immutable view, never per frame.
        #[cfg(test)]
        CATALOG_CONTRACT_CHECKS.with(|checked| checked.set(checked.get() + 1));
        if registered
            .receipt
            .asset_record(authored.label.clone())
            .map_err(|error| error.to_string())?
            != *authored
        {
            return Err("Edited picture asset differs from its immutable receipt.".into());
        }
    }
    Ok(())
}

pub(super) fn admit_edited<'a>(
    base: &Arc<Workspace>,
    snapshot: &Arc<deadpan_playback::Snapshot>,
    media: &Arc<MediaView>,
    cache: &'a mut Option<PlanCache>,
    cancelled: &AtomicBool,
) -> Result<&'a RenderPlan, String> {
    snapshot
        .validate_proposed_base(base.session, &base.document)
        .map_err(|error| error.to_string())?;
    snapshot
        .validate_edit_slice_view(media.admitted())
        .map_err(|error| error.to_string())?;
    if media.session() != base.session
        || !media.admitted().matches_originals(&base.originals)
        || !Arc::ptr_eq(&snapshot.document, media.admitted().document())
    {
        return Err("Edited picture belongs to another project session or document.".into());
    }
    media
        .admitted()
        .check_live(cancelled)
        .map_err(|error| error.to_string())?;
    let retained = cache.as_ref().is_some_and(|cached| {
        matches!(&cached.identity, PlanIdentity::Edited { base: old_base, snapshot: old_snapshot, media: old_media }
            if Arc::ptr_eq(base, old_base) && Arc::ptr_eq(snapshot, old_snapshot) && Arc::ptr_eq(media, old_media))
    });
    if !retained {
        check_catalog(media)?;
        let plan =
            RenderPlan::compile(media.admitted().document()).map_err(|error| error.to_string())?;
        media
            .admitted()
            .check_live(cancelled)
            .map_err(|error| error.to_string())?;
        *cache = Some(PlanCache {
            identity: PlanIdentity::Edited {
                base: base.clone(),
                snapshot: snapshot.clone(),
                media: media.clone(),
            },
            plan,
        });
    }
    Ok(&cache.as_ref().expect("admitted edited plan").plan)
}

pub(super) fn admit_copied<'a>(
    view: &Arc<CopiedView>,
    cache: &'a mut Option<PlanCache>,
    cancelled: &AtomicBool,
) -> Result<&'a RenderPlan, String> {
    let id = view.id();
    let media = view.media();
    let admitted = media.admitted();
    if id.copy.session == 0
        || id.copy.request == 0
        || id.copy.session != media.session()
        || &id.copy.project != admitted.document().project_id()
        || &id.copy.source_revision != admitted.capture_revision()
        || admitted.placement_base().is_some()
        || id.range.duration() == deadpan_core::FrameDuration::ZERO
    {
        return Err("Copied picture belongs to another source capture.".into());
    }
    admitted
        .check_live(cancelled)
        .map_err(|error| error.to_string())?;
    let retained = cache.as_ref().is_some_and(|cached| {
        matches!(&cached.identity, PlanIdentity::Copied(previous) if Arc::ptr_eq(view, previous))
    });
    if !retained {
        check_catalog(media)?;
        let plan = RenderPlan::compile(admitted.document()).map_err(|error| error.to_string())?;
        if plan.duration() != id.range.duration() {
            return Err("Copied picture duration differs from its selected source range.".into());
        }
        admitted
            .check_live(cancelled)
            .map_err(|error| error.to_string())?;
        *cache = Some(PlanCache {
            identity: PlanIdentity::Copied(view.clone()),
            plan,
        });
    }
    Ok(&cache.as_ref().expect("admitted copied plan").plan)
}

pub(super) fn copied_picture(
    view: &Arc<CopiedView>,
    frame: ProjectFrame,
    cache: &mut Option<PlanCache>,
    cancelled: &AtomicBool,
    retained: &mut Option<RetainedSession>,
) -> Result<Picture, String> {
    let plan = admit_copied(view, cache, cancelled)?;
    let picture = media_picture(
        PictureMedia::Slice(view.media()),
        view.media().admitted().document(),
        plan,
        &ProjectView::Sequence { frame },
        cancelled,
        retained,
    )?;
    view.media()
        .admitted()
        .check_live(cancelled)
        .map_err(|error| error.to_string())?;
    Ok(picture)
}
