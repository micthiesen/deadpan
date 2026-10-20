// Exclusive file locks that a launching child may still briefly share.

use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rustix::fs::{FlockOperation, flock};

/// How long a released lock may still look held because of a launch.
const LAUNCH_WINDOW: Duration = Duration::from_secs(2);

/// The outcome of [`lock_exclusive_after_launches`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchLock {
    Acquired,
    /// Another owner still held the lock after the launch window.
    Held,
    Cancelled,
}

/// Take an exclusive `flock` on `file` without waiting for another owner.
///
/// A child launched while any descriptor of the same open file is open shares
/// that open file until the child's exec closes the descriptor, even with
/// close-on-exec, so a lock its owner has just released can briefly look held.
/// This applies to every launch in the process, including direct and foreign
/// ones. Retry for two seconds before reporting [`LaunchLock::Held`]; an
/// actual owner holds the lock far longer. A set `cancelled` flag ends the
/// wait.
pub fn lock_exclusive_after_launches(
    file: &File,
    cancelled: Option<&AtomicBool>,
) -> io::Result<LaunchLock> {
    let deadline = Instant::now() + LAUNCH_WINDOW;
    loop {
        match flock(file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(LaunchLock::Acquired),
            Err(rustix::io::Errno::WOULDBLOCK)
                if cancelled.is_some_and(|cancelled| cancelled.load(Ordering::Acquire)) =>
            {
                return Ok(LaunchLock::Cancelled);
            }
            Err(rustix::io::Errno::WOULDBLOCK) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(rustix::io::Errno::WOULDBLOCK) => return Ok(LaunchLock::Held),
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_released_lock_still_shared_with_a_child_is_awaited_not_held() {
        let path =
            std::env::temp_dir().join(format!("deadpan-launch-lock-{}.lock", std::process::id()));
        let open = || {
            File::options()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
                .unwrap()
        };
        let owner = open();
        assert_eq!(
            lock_exclusive_after_launches(&owner, None).unwrap(),
            LaunchLock::Acquired
        );
        // Long enough that a scheduling stall cannot release it before the
        // probe, yet well inside the launch window.
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("1")
            .stdin(owner.try_clone().unwrap())
            .spawn()
            .unwrap();
        let released = Instant::now();
        drop(owner);
        let next = open();
        assert_eq!(
            flock(&next, FlockOperation::NonBlockingLockExclusive),
            Err(rustix::io::Errno::WOULDBLOCK),
            "the child still shares the released lock"
        );
        assert_eq!(
            lock_exclusive_after_launches(&next, None).unwrap(),
            LaunchLock::Acquired
        );
        assert!(
            released.elapsed() >= Duration::from_millis(100),
            "acquired only once the child closed its reference"
        );
        let contender = open();
        assert_eq!(
            lock_exclusive_after_launches(&contender, None).unwrap(),
            LaunchLock::Held
        );
        let started = Instant::now();
        assert_eq!(
            lock_exclusive_after_launches(&contender, Some(&AtomicBool::new(true))).unwrap(),
            LaunchLock::Cancelled
        );
        assert!(started.elapsed() < Duration::from_millis(500));
        child.wait().unwrap();
        drop(next);
        std::fs::remove_file(&path).unwrap();
    }
}
