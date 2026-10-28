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
        self.wet_allowed_inspected(layer, || Ok(()))
            .expect("no-op Tail inspection cannot fail")
    }

    pub(super) fn wet_allowed_inspected(
        &self,
        layer: usize,
        mut inspect: impl FnMut() -> Result<(), StageAudioError>,
    ) -> Result<bool, StageAudioError> {
        inspect()?;
        if !self.original {
            return Ok(true);
        }
        for (clock, _) in &self.layers[layer + 1..] {
            inspect()?;
            if clock.is_explicit_tail_provider() {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn visit_members(
    routes: &[Route<'_>],
    owner: &Owner<'_>,
    active: impl Fn(usize) -> bool,
    wet: bool,
    mut inspect: impl FnMut() -> Result<(), StageAudioError>,
    mut visit: impl FnMut(usize, usize) -> Result<bool, StageAudioError>,
) -> Result<(), StageAudioError> {
    for &(id, layer) in &owner.members {
        inspect()?;
        if active(id)
            && (!wet || routes[id].wet_allowed_inspected(layer, &mut inspect)?)
            && !visit(id, layer)?
        {
            break;
        }
    }
    Ok(())
}

// Preserve the older test bridge independently of the fallible production
// visitation. It supplies no completion authority or allocation proof.
#[cfg(test)]
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
    permissions_inspected(routes, owner, active, wet, || Ok(()))
}

pub(super) fn permissions_inspected(
    routes: &[Route<'_>],
    owner: &Owner<'_>,
    active: impl Fn(usize) -> bool,
    wet: bool,
    inspect: impl FnMut() -> Result<(), StageAudioError>,
) -> Result<Permissions, StageAudioError> {
    let mut permissions = Permissions::default();
    visit_members(routes, owner, active, wet, inspect, |id, _| {
        permissions.insert(routes[id].voice)?;
        Ok(true)
    })?;
    Ok(permissions)
}

pub(super) fn first_clock<'plan>(
    routes: &[Route<'plan>],
    owner: &Owner<'plan>,
    active: impl Fn(usize) -> bool,
    wet: bool,
) -> Option<AudioOwnerClock<'plan>> {
    first_clock_ref_inspected(routes, owner, active, wet, || Ok(()))
        .expect("no-op first-clock inspection cannot fail")
        .cloned()
}

pub(super) fn first_clock_ref_inspected<'routes, 'plan>(
    routes: &'routes [Route<'plan>],
    owner: &Owner<'plan>,
    active: impl Fn(usize) -> bool,
    wet: bool,
    inspect: impl FnMut() -> Result<(), StageAudioError>,
) -> Result<Option<&'routes AudioOwnerClock<'plan>>, StageAudioError> {
    let mut clock = None;
    visit_members(routes, owner, active, wet, inspect, |id, layer| {
        clock = Some(&routes[id].layers[layer].0);
        Ok(false)
    })?;
    Ok(clock)
}

pub(super) struct WetContext<'plan> {
    pub(super) permissions: Permissions,
    pub(super) clock: Option<AudioOwnerClock<'plan>>,
}

pub(super) struct WetContextRef<'routes, 'plan> {
    pub(super) permissions: Permissions,
    pub(super) clock: Option<&'routes AudioOwnerClock<'plan>>,
}

pub(super) fn wet_context_ref_inspected<'routes, 'plan>(
    routes: &'routes [Route<'plan>],
    owner: &Owner<'plan>,
    active: impl Fn(usize) -> bool,
    inspect: impl FnMut() -> Result<(), StageAudioError>,
) -> Result<WetContextRef<'routes, 'plan>, StageAudioError> {
    let mut context = WetContextRef {
        permissions: Permissions::default(),
        clock: None,
    };
    visit_members(routes, owner, active, true, inspect, |id, layer| {
        context.permissions.insert(routes[id].voice)?;
        context.clock.get_or_insert(&routes[id].layers[layer].0);
        Ok(true)
    })?;
    Ok(context)
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
    visit_members(
        routes,
        owner,
        active,
        true,
        || Ok(()),
        |id, layer| {
            context.permissions.insert(routes[id].voice)?;
            context
                .clock
                .get_or_insert_with(|| routes[id].layers[layer].0.clone());
            Ok(true)
        },
    )?;
    Ok(context)
}
