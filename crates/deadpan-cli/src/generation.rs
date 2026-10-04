//! Local AI Hold generation from the project: the bridge provider, the
//! conditioning inputs prepared from a Hold's boundary pictures, and the
//! supervised worker attempt that turns them into a qualified bundle.
//!
//! The only provider is the development LTX MLX runtime qualified in
//! `tools/model-qualification`; it is located through `DEADPAN_BRIDGE_*`
//! environment variables and is not a distributed runtime. Generation only
//! ever proposes a candidate; acceptance is a separate explicit edit.

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod acceptance;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod attempt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod command;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod conditioning;
pub mod runtime;

use deadpan_core::FrameRate;
use deadpan_jobs::{
    AxisLimits, BridgeCapability, DimensionLimits, FrameCountFormula, NativeDimensions,
    ProviderSelection,
};

/// The development provider's native raster.
pub const NATIVE_WIDTH: u32 = 768;
pub const NATIVE_HEIGHT: u32 = 320;
/// The worker refuses longer bridges.
pub const MAX_BRIDGE_PROJECT_FRAMES: i64 = 180;

/// The capability of the pinned development LTX MLX worker: still two-sided
/// bridges at 24 fps, 8k+1 native frames from 9 to 97, at 768×320.
pub fn development_capability() -> BridgeCapability {
    BridgeCapability::new(
        true,
        FrameRate::new(24, 1).expect("constant rate"),
        FrameCountFormula::new(8, 1, 9, 97).expect("constant formula"),
        DimensionLimits::new(
            AxisLimits::new(NATIVE_WIDTH, NATIVE_WIDTH, 64).expect("constant axis"),
            AxisLimits::new(NATIVE_HEIGHT, NATIVE_HEIGHT, 64).expect("constant axis"),
        ),
    )
}

pub fn native_dimensions() -> NativeDimensions {
    NativeDimensions::new(NATIVE_WIDTH, NATIVE_HEIGHT).expect("constant dimensions")
}

/// The pinned development pack and runtime the worker accepts.
pub fn development_provider(seed: u64) -> ProviderSelection {
    serde_json::from_value(serde_json::json!({
        "pack_id": "ltx-2.3-q4-development",
        "pack_version": "56a5866d",
        "runtime_id": "ltx-mlx-development",
        "runtime_version": "0.15.8+deadpan1",
        "seed": seed,
    }))
    .expect("constant provider")
}
