//! Local AI Hold generation from the project: the bridge provider, the
//! conditioning inputs prepared from a Hold's boundary pictures, and the
//! supervised worker attempt that turns them into a qualified bundle.
//!
//! The pinned LTX MLX runtime qualified in `tools/model-qualification` runs
//! with the selected signed model-pack manifest. Generation records that
//! pack/runtime identity in its durable request and only proposes a
//! candidate; acceptance is a separate explicit edit.

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod acceptance;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod attempt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod command;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod conditioning;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod joins;
pub mod runtime;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod variants;

use deadpan_jobs::{BridgeCapability, NativeDimensions, ProviderSelection};

/// The shipped provider's native raster.
pub const NATIVE_WIDTH: u32 = 768;
pub const NATIVE_HEIGHT: u32 = 320;
/// The worker refuses longer bridges.
pub const MAX_BRIDGE_PROJECT_FRAMES: i64 = 180;

/// The capability of the pinned LTX MLX worker: still two-sided
/// bridges at 24 fps, 8k+1 native frames from 9 to 97, at 768×320.
pub fn development_capability() -> BridgeCapability {
    deadpan_models::packs::approved_pack(runtime::BRIDGE_PACK)
        .expect("compiled bridge pack")
        .constraints
        .bridge
        .expect("bridge pack has declared frame and image constraints")
        .capability()
        .expect("compiled bridge constraints validate")
}

pub fn native_dimensions() -> NativeDimensions {
    NativeDimensions::new(NATIVE_WIDTH, NATIVE_HEIGHT).expect("constant dimensions")
}

/// The compiled bridge pack and runtime identity, used by scripted tests and
/// explicit development runs without an installed pack selection.
pub fn development_provider(seed: u64) -> ProviderSelection {
    serde_json::from_value(serde_json::json!({
        "pack_id": "ltx-2.3-q4-bridge",
        "pack_version": "1",
        "runtime_id": "ltx-mlx",
        "runtime_version": "0.15.8+deadpan1",
        "seed": seed,
    }))
    .expect("constant provider")
}

/// Whether two attempts use the same immutable pack/runtime selection. Seeds
/// identify variants and do not affect request compatibility.
pub fn same_provider_identity(left: &ProviderSelection, right: &ProviderSelection) -> bool {
    left.pack_id == right.pack_id
        && left.pack_version == right.pack_version
        && left.runtime_id == right.runtime_id
        && left.runtime_version == right.runtime_version
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use deadpan_jobs::ProviderPackVersion;

    #[test]
    fn rollback_to_a_different_pack_version_starts_a_distinct_request_identity() {
        let selected = development_provider(7);
        let mut request = selected.clone();
        request.pack_version = ProviderPackVersion::new("2").unwrap();
        let mut variant = request.clone();
        variant.seed = 8;
        assert!(same_provider_identity(&request, &variant));

        assert!(!same_provider_identity(&request, &selected));
    }
}
