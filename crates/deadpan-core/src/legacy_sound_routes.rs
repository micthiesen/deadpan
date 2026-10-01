//! Closed root-sound vocabulary for frozen schemas 30 through 32.

use crate::{RootSoundOperation, RootSoundRoute, SoundId, ValueChange};
use serde::Deserializer;
use std::collections::BTreeMap;

pub(crate) fn admitted(route: &RootSoundRoute) -> bool {
    route.edits.iter().all(|edit| {
        matches!(
            edit.operation,
            RootSoundOperation::Insert { .. } | RootSoundOperation::Delete { .. }
        )
    })
}

pub(crate) fn routes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<SoundId, RootSoundRoute>, D::Error> {
    let routes: BTreeMap<SoundId, RootSoundRoute> = crate::document::unique_map(deserializer)?;
    if routes.values().any(|route| !admitted(route)) {
        return Err(serde::de::Error::custom(
            "legacy root sound operation is unsupported",
        ));
    }
    Ok(routes)
}

pub(crate) fn changes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<SoundId, ValueChange<RootSoundRoute>>, D::Error> {
    let changes: BTreeMap<SoundId, ValueChange<RootSoundRoute>> =
        crate::document::unique_map(deserializer)?;
    if changes
        .values()
        .flat_map(|change| change.before.iter().chain(change.after.iter()))
        .any(|route| !admitted(route))
    {
        return Err(serde::de::Error::custom(
            "legacy root sound operation patch is unsupported",
        ));
    }
    Ok(changes)
}
