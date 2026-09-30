//! Generated-media storage and revocable readers over the shared byte-object store.
//!
//! The public names remain stable because callers deal in generated artifacts;
//! containment, hashing, publication, and immutable snapshots are implemented
//! once in the crate-private object-storage layer.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub use deadpan_core::{GeneratedContentId, GeneratedObjectRef};

pub(crate) use crate::object_storage::GeneratedStorage;
use crate::object_storage::ObjectControl;
pub use crate::object_storage::{
    ObjectLimits as GeneratedMediaLimits, ObjectStorageError as GeneratedMediaError,
    VerifiedObject as VerifiedGeneratedObject,
};

/// Cooperative byte and elapsed-time bounds for one immutable snapshot copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratedReadLimits {
    bytes: GeneratedMediaLimits,
    timeout: Duration,
}

impl GeneratedReadLimits {
    pub fn new(maximum_bytes: u64, timeout: Duration) -> Result<Self, GeneratedMediaError> {
        if maximum_bytes == 0 || timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err(GeneratedMediaError::InvalidReadLimits);
        }
        Ok(Self {
            bytes: GeneratedMediaLimits::new(maximum_bytes)?,
            timeout,
        })
    }

    pub const fn maximum_bytes(self) -> u64 {
        self.bytes.maximum_bytes()
    }

    pub const fn timeout(self) -> Duration {
        self.timeout
    }
}

/// Connection-free read authority pinned to one store session's package.
/// Clones neither borrow SQLite nor retain the writer lock. Closing either a
/// writable or read-only owning store revokes further snapshot preparation.
#[derive(Debug, Clone)]
pub struct GeneratedReadHandle {
    storage: Arc<GeneratedStorage>,
    closed: Arc<AtomicBool>,
}

impl GeneratedReadHandle {
    pub(crate) fn new(storage: Arc<GeneratedStorage>, closed: Arc<AtomicBool>) -> Self {
        Self { storage, closed }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Recheck before using a retained decoder; private snapshot bytes remain
    /// readable after revocation, but the store session no longer admits work.
    pub fn check_live(&self, cancelled: &AtomicBool) -> Result<(), GeneratedMediaError> {
        if self.is_closed() {
            return Err(GeneratedMediaError::SessionClosed);
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(GeneratedMediaError::Cancelled);
        }
        Ok(())
    }

    /// Verify and copy one object into private immutable bytes on an I/O worker.
    /// The fixed-size copy checks the deadline, cancellation and session closure
    /// between reads; these are cooperative bounds, not preemptive I/O timeouts.
    pub fn snapshot(
        &self,
        expected: &GeneratedObjectRef,
        limits: GeneratedReadLimits,
        cancelled: &AtomicBool,
    ) -> Result<VerifiedGeneratedObject, GeneratedMediaError> {
        self.check_live(cancelled)?;
        let deadline = Instant::now()
            .checked_add(limits.timeout)
            .ok_or(GeneratedMediaError::InvalidReadLimits)?;
        let control = ObjectControl::bounded(deadline, cancelled).with_closed(&self.closed);
        self.storage
            .snapshot_controlled(expected, limits.bytes, control)
    }
}
