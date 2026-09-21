//! Bounded worker messages and a pure host-side job lifecycle.
//!
//! On macOS and Linux, the supervisor owns subprocess groups, bounded pipes,
//! and deadlines. Filesystem containment beyond the workspace root, media/hash
//! validation, promotion, persistence, recovery, and scheduling remain host
//! responsibilities. A worker's completed manifest is untrusted until the host
//! validates it and explicitly advances the lifecycle.

pub mod lifecycle;
pub mod protocol;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod supervisor;

pub use lifecycle::*;
pub use protocol::*;
