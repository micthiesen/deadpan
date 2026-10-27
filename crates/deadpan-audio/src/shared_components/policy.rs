//! Structural contribution decisions shared by both numerical traversal orders.
//! The active lookup can read admitted metadata; these helpers never inspect
//! PCM, gains, native state or providers. They do not execute a demand visitor.

use super::*;

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;

pub(super) fn ordered_owners<'a, 'plan>(
    owners: &'a BTreeMap<OwnerKey, Owner<'plan>>,
) -> Vec<&'a Owner<'plan>> {
    let mut ordered: Vec<_> = owners.values().collect();
    ordered.sort_by_key(|owner| std::cmp::Reverse(owner.depth));
    ordered
}

pub(super) fn is_equalizer(
    kind: AudioTreatmentStage,
    recipe: Option<&deadpan_core::AudioTreatments>,
) -> bool {
    kind == AudioTreatmentStage::Equalizer
        && recipe
            .and_then(|recipe| recipe.equalizer())
            .is_some_and(|equalizer| !equalizer.is_unity())
}

impl Route<'_> {
    pub(super) fn wet_allowed(&self, layer: usize) -> bool {
        !self.original
            || !self.layers[layer + 1..]
                .iter()
                .any(|(clock, _)| clock.is_explicit_tail_provider())
    }
}

fn members<'a, 'plan: 'a>(
    routes: &'a [Route<'plan>],
    owner: &'a Owner<'plan>,
    active: impl Fn(usize) -> bool + 'a,
    wet: bool,
) -> impl Iterator<Item = (usize, usize)> + 'a {
    owner
        .members
        .iter()
        .copied()
        .filter(move |&(id, layer)| active(id) && (!wet || routes[id].wet_allowed(layer)))
}

pub(super) fn permissions(
    routes: &[Route<'_>],
    owner: &Owner<'_>,
    active: impl Fn(usize) -> bool,
    wet: bool,
) -> Result<Permissions, StageAudioError> {
    let mut permissions = Permissions::default();
    for (id, _) in members(routes, owner, active, wet) {
        permissions.insert(routes[id].voice)?;
    }
    Ok(permissions)
}

pub(super) fn first_clock<'plan>(
    routes: &[Route<'plan>],
    owner: &Owner<'plan>,
    active: impl Fn(usize) -> bool,
    wet: bool,
) -> Option<AudioOwnerClock<'plan>> {
    members(routes, owner, active, wet)
        .next()
        .map(|(id, layer)| routes[id].layers[layer].0.clone())
}

pub(super) struct WetContext<'plan> {
    pub(super) permissions: Permissions,
    pub(super) clock: Option<AudioOwnerClock<'plan>>,
}

/// Dynamic execution obtains these together. Constant execution keeps its
/// existing separate permission and clock walks at their numerical boundaries.
pub(super) fn wet_context<'plan>(
    routes: &[Route<'plan>],
    owner: &Owner<'plan>,
    active: impl Fn(usize) -> bool,
) -> Result<WetContext<'plan>, StageAudioError> {
    let mut context = WetContext {
        permissions: Permissions::default(),
        clock: None,
    };
    for (id, layer) in members(routes, owner, active, true) {
        context.permissions.insert(routes[id].voice)?;
        context
            .clock
            .get_or_insert_with(|| routes[id].layers[layer].0.clone());
    }
    Ok(context)
}
