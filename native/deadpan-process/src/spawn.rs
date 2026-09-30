// Cooperative subprocess creation on platforms without atomic pipe CLOEXEC.

use std::io;
use std::process::{Child, Command};

/// Spawn a configured command without overlapping another participating launch.
///
/// macOS creates standard-library pipes before setting their close-on-exec flags.
/// Serialize the complete spawn call, including that setup, so another launch
/// using this boundary cannot inherit a transient pipe endpoint. Other platforms
/// use their ordinary spawn path.
///
/// This is cooperative within this process. Direct standard-library or foreign
/// process launches do not participate. It neither changes the command's streams,
/// environment, or process group nor contains children that retain inherited I/O.
/// Do not attach `CommandExt::pre_exec` callbacks: they run in the forked child
/// while the launch lock is held and must not reenter this boundary or other
/// arbitrary Rust code. Configure the command with its ordinary builder methods.
/// The lock covers creation only; no waits or pipe pumping run under it.
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    with_descriptor_creation_guard(|| command.spawn())
}

/// Close the macOS descriptor-creation race against participating subprocess
/// launches. Create descriptors and set CLOEXEC inside this closure; perform
/// connects, polling, waits and other potentially blocking work afterward.
/// This does not coordinate foreign or direct standard-library launches.
/// The closure must not call `spawn` or recursively acquire this guard.
pub fn with_descriptor_creation_guard<T>(create: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    #[cfg(target_os = "macos")]
    let _launch = SPAWN_LOCK
        .lock()
        .map_err(|_| io::Error::other("process launch lock poisoned"))?;
    create()
}

/// Try to create descriptors without waiting for a participating macOS launch.
/// Returns `None` on contention without invoking the closure. As with the
/// blocking guard, set CLOEXEC before returning and keep connects and waits
/// outside the closure. Other platforms invoke the closure immediately.
pub fn try_with_descriptor_creation_guard<T>(
    create: impl FnOnce() -> io::Result<T>,
) -> io::Result<Option<T>> {
    #[cfg(target_os = "macos")]
    let _launch = match SPAWN_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
        Err(std::sync::TryLockError::Poisoned(_)) => {
            return Err(io::Error::other("process launch lock poisoned"));
        }
    };
    create().map(Some)
}

#[cfg(target_os = "macos")]
static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn contended_try_guard_skips_descriptor_creation() {
        let _held = SPAWN_LOCK.lock().unwrap();
        let mut invoked = false;
        let result = try_with_descriptor_creation_guard(|| {
            invoked = true;
            Ok(17)
        })
        .unwrap();
        assert_eq!(result, None);
        assert!(!invoked);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn try_guard_runs_the_atomic_platform_path() {
        assert_eq!(
            try_with_descriptor_creation_guard(|| Ok(17)).unwrap(),
            Some(17)
        );
        assert_eq!(
            try_with_descriptor_creation_guard::<()>(|| {
                Err(io::Error::from(io::ErrorKind::InvalidInput))
            })
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
