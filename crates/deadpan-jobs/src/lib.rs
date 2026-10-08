//! Bounded worker messages and a pure host-side job lifecycle.
//!
//! On macOS and Linux, the supervisor owns subprocess groups, bounded pipes,
//! and deadlines. The artifact adapter provides descriptor-relative containment
//! and hashed snapshots. Media validation, promotion, persistence, recovery, and
//! scheduling remain host responsibilities. A worker's completed manifest is
//! untrusted until the host validates it and explicitly advances the lifecycle.

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod adversarial;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod artifact;
pub mod extension_plan;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod faces;
pub mod generation_inputs;
mod generation_operation;
pub mod generation_plan;
mod hold_prompt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod landmarks;
pub mod lifecycle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod process;
pub mod protocol;
pub mod render;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod supervisor;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod tracking;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod transcription;

pub use extension_plan::*;
pub use generation_inputs::*;
pub use generation_operation::*;
pub use generation_plan::*;
pub use hold_prompt::*;
pub use lifecycle::*;
pub use protocol::*;
