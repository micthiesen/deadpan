//! Owned worker teardown, with group-membership confirmation on Darwin.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::io;
use std::process::Child;
use std::time::{Duration, Instant};

use rustix::process::{
    Pid, Signal, WaitId, WaitIdOptions, kill_process, kill_process_group, waitid,
};

/// Stop an owned worker group without reaping its leader.
///
/// The caller must create `child` as its process-group leader and retain sole
/// reaping ownership throughout this call. Every signal follows a non-reaping
/// ownership check, so an already-reaped PID cannot target a reused identity.
/// Success requires an exited leader and no other group members. Darwin signals
/// a snapshot of group members; even a successful SIGKILL can miss a later fork.
/// This is bounded cleanup, not containment of processes that leave the group.
#[cfg(target_os = "macos")]
pub fn terminate_owned_group(child: &Child, deadline: Instant) -> io::Result<()> {
    terminate_until(
        || exited_leader_has_no_other_members(child),
        || kill_process_group(Pid::from_child(child), Signal::KILL),
        || deadline.checked_duration_since(Instant::now()),
        std::thread::park_timeout,
    )
}

/// Request termination of an owned Linux worker group without reaping.
///
/// The caller must create the child as group leader and retain sole reaping
/// ownership. Unlike Darwin's membership-confirmed teardown, this confirms only
/// the signal request, not descendant exit. Ownership errors prevent signalling.
#[cfg(target_os = "linux")]
pub fn signal_owned_group(child: &Child) -> io::Result<()> {
    owned_child_has_exited(child)?;
    match kill_process_group(Pid::from_child(child), Signal::KILL) {
        Ok(()) => Ok(()),
        Err(rustix::io::Errno::SRCH) if owned_child_has_exited(child)? => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Last-resort leader teardown when group cleanup could not be confirmed.
///
/// The caller must retain sole reaping ownership. This checks
/// ownership before every signal and confirms exit without reaping. It does not
/// inspect or stop descendants and cannot turn a failed group cleanup into a
/// successful worker result. Group inspection failure cannot prevent this
/// independent, checked attempt to stop the leader.
pub fn terminate_owned_leader(child: &Child, deadline: Instant) -> io::Result<()> {
    terminate_until(
        || owned_child_has_exited(child),
        || kill_process(Pid::from_child(child), Signal::KILL),
        || deadline.checked_duration_since(Instant::now()),
        std::thread::park_timeout,
    )
}

fn terminate_until(
    mut inspect: impl FnMut() -> io::Result<bool>,
    mut signal: impl FnMut() -> Result<(), rustix::io::Errno>,
    mut remaining: impl FnMut() -> Option<Duration>,
    mut pause: impl FnMut(Duration),
) -> io::Result<()> {
    let mut permission_denied = false;
    loop {
        // Inspection checks waitid ownership even when the child is still alive.
        // Never signal after it reports ECHILD or another inspection failure.
        if inspect()? {
            return Ok(());
        }
        let Some(left) = remaining().filter(|left| !left.is_zero()) else {
            return Err(io::Error::new(
                if permission_denied {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::TimedOut
                },
                "worker did not finish cleanup before its deadline",
            ));
        };
        match signal() {
            Ok(()) | Err(rustix::io::Errno::SRCH) => permission_denied = false,
            // An exited member can produce EPERM even if live members received
            // SIGKILL. It is only harmless when the next inspection proves it.
            Err(rustix::io::Errno::PERM) => permission_denied = true,
            Err(error) => return Err(error.into()),
        }
        pause(left.min(Duration::from_millis(2)));
    }
}

/// Whether an unreaped exited child has no other member in its process group.
///
/// The caller must have created this child as its group leader and retain sole
/// reaping ownership. This function never signals or reaps it. An inaccessible
/// group, unknown membership, or another member never qualifies as empty. It is
/// a snapshot, not a sandbox or a guarantee against processes joining later.
#[cfg(target_os = "macos")]
pub fn exited_leader_has_no_other_members(child: &Child) -> io::Result<bool> {
    if !owned_child_has_exited(child)? {
        return Ok(false);
    }
    // Two slots suffice: any non-leader or full buffer fails closed, regardless
    // of how many additional members might have been truncated.
    let mut members = [0; 2];
    let bytes = group_members(child.id(), &mut members)?;
    only_leader(child.id(), bytes, members)
}

fn owned_child_has_exited(child: &Child) -> io::Result<bool> {
    let pid = Pid::from_child(child);
    if pid == Pid::INIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "init is not a worker",
        ));
    }
    Ok(waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )?
    .is_some())
}

#[cfg(target_os = "macos")]
fn only_leader(leader: u32, bytes: usize, members: [libc::pid_t; 2]) -> io::Result<bool> {
    let pid_size = size_of::<libc::pid_t>();
    if bytes > size_of_val(&members) || !bytes.is_multiple_of(pid_size) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid process-list length",
        ));
    }
    Ok(bytes == 0 || (bytes == pid_size && u32::try_from(members[0]) == Ok(leader)))
}

#[allow(unsafe_code)]
#[cfg(target_os = "macos")]
fn group_members(group: u32, members: &mut [libc::pid_t; 2]) -> io::Result<usize> {
    // PROC_PGRP_ONLY in the Apple SDK's sys/proc_info.h. XNU's proc_listpids
    // enumerates allproc and zombproc under proc_list_lock, including other UIDs.
    // libproc converts syscall errors to zero, so clear/capture this thread's
    // errno to distinguish an empty list from failure. No heap sizing query.
    const PROC_PGRP_ONLY: u32 = 2;
    let capacity = libc::c_int::try_from(size_of_val(members))
        .map_err(|_| io::Error::other("process-list capacity overflow"))?;
    // SAFETY: __error returns valid thread-local errno. The initialized, aligned
    // two-pid buffer remains exclusively borrowed and alive across this bounded
    // synchronous C call; the exact byte capacity is passed. libproc retains no
    // pointer. Neither the buffer nor thread-local errno aliases another borrow.
    let (result, errno) = unsafe {
        *libc::__error() = 0;
        let result =
            libc::proc_listpids(PROC_PGRP_ONLY, group, members.as_mut_ptr().cast(), capacity);
        (result, *libc::__error())
    };
    if result < 0 || (result == 0 && errno != 0) {
        return Err(if errno == 0 {
            io::Error::other("process-list failure without errno")
        } else {
            io::Error::from_raw_os_error(errno)
        });
    }
    usize::try_from(result).map_err(|_| io::Error::other("negative process-list length"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[cfg(target_os = "macos")]
    fn observe_exit(child: &Child) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if waitid(
                WaitId::Pid(Pid::from_child(child)),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .unwrap()
            .is_some()
            {
                return;
            }
            assert!(Instant::now() < deadline, "child did not exit");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn list_interpretation_rejects_other_members_truncation_and_invalid_lengths() {
        let size = size_of::<libc::pid_t>();
        assert!(only_leader(42, 0, [0, 0]).unwrap());
        assert!(only_leader(42, size, [42, 0]).unwrap());
        assert!(!only_leader(42, size, [43, 0]).unwrap());
        assert!(!only_leader(42, size, [-1, 0]).unwrap());
        assert!(!only_leader(42, size * 2, [42, 43]).unwrap());
        assert!(!only_leader(42, size * 2, [42, 42]).unwrap());
        assert!(only_leader(42, 1, [42, 0]).is_err());
        assert!(only_leader(42, size * 3, [42, 0]).is_err());
    }

    #[test]
    fn successful_signal_requires_membership_confirmation_and_can_need_retry() {
        let signals = Cell::new(0);
        terminate_until(
            || Ok(signals.get() == 2),
            || {
                signals.set(signals.get() + 1);
                Ok(())
            },
            || Some(Duration::from_secs(1)),
            |_| {},
        )
        .unwrap();
        assert_eq!(signals.get(), 2);
    }

    #[test]
    fn signal_results_never_override_persistent_members_or_deadline() {
        for result in [
            Ok(()),
            Err(rustix::io::Errno::SRCH),
            Err(rustix::io::Errno::PERM),
        ] {
            let signals = Cell::new(0);
            let error = terminate_until(
                || Ok(false),
                || {
                    signals.set(signals.get() + 1);
                    result
                },
                || (signals.get() < 3).then_some(Duration::from_millis(2)),
                |_| {},
            )
            .unwrap_err();
            assert_eq!(signals.get(), 3);
            assert_eq!(
                error.kind(),
                if result == Err(rustix::io::Errno::PERM) {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::TimedOut
                }
            );
        }
    }

    #[test]
    fn transient_darwin_signal_errors_require_subsequent_empty_group() {
        for result in [rustix::io::Errno::SRCH, rustix::io::Errno::PERM] {
            let signalled = Cell::new(false);
            terminate_until(
                || Ok(signalled.get()),
                || {
                    signalled.set(true);
                    Err(result)
                },
                || Some(Duration::from_secs(1)),
                |_| {},
            )
            .unwrap();
            assert!(signalled.get());
        }
    }

    #[test]
    fn lost_ownership_prevents_every_subsequent_signal() {
        for previous_signals in [0, 1] {
            let signals = Cell::new(0);
            let error = terminate_until(
                || {
                    if signals.get() == previous_signals {
                        Err(rustix::io::Errno::CHILD.into())
                    } else {
                        Ok(false)
                    }
                },
                || {
                    signals.set(signals.get() + 1);
                    Ok(())
                },
                || Some(Duration::from_secs(1)),
                |_| {},
            )
            .unwrap_err();
            assert_eq!(
                error.raw_os_error(),
                Some(rustix::io::Errno::CHILD.raw_os_error())
            );
            assert_eq!(signals.get(), previous_signals);
        }
    }

    #[test]
    fn unexpected_signal_errors_fail_without_retry() {
        let mut signals = 0;
        let error = terminate_until(
            || Ok(false),
            || {
                signals += 1;
                Err(rustix::io::Errno::INVAL)
            },
            || Some(Duration::from_secs(1)),
            |_| panic!("must not pause after an unexpected signal error"),
        )
        .unwrap_err();
        assert_eq!(signals, 1);
        assert_eq!(
            error.raw_os_error(),
            Some(rustix::io::Errno::INVAL.raw_os_error())
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn native_teardown_preserves_leader_for_reaping_and_rejects_reaped_child() {
        let mut child = OwnedChild(
            Command::new("/bin/sleep")
                .arg("60")
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        terminate_owned_group(&child.0, Instant::now() + Duration::from_secs(1)).unwrap();
        assert!(exited_leader_has_no_other_members(&child.0).unwrap());
        assert!(!child.0.wait().unwrap().success());
        let error =
            terminate_owned_group(&child.0, Instant::now() + Duration::from_secs(1)).unwrap_err();
        assert_eq!(
            error.raw_os_error(),
            Some(rustix::io::Errno::CHILD.raw_os_error())
        );
    }

    #[test]
    fn native_leader_fallback_checks_ownership_without_claiming_group_cleanup() {
        let mut leader = OwnedChild(
            Command::new("/bin/sleep")
                .arg("60")
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        let mut member = OwnedChild(
            Command::new("/bin/sleep")
                .arg("60")
                .process_group(i32::try_from(leader.0.id()).unwrap())
                .spawn()
                .unwrap(),
        );
        terminate_owned_leader(&leader.0, Instant::now() + Duration::from_secs(1)).unwrap();
        assert!(owned_child_has_exited(&leader.0).unwrap());
        #[cfg(target_os = "macos")]
        assert!(!exited_leader_has_no_other_members(&leader.0).unwrap());
        assert!(member.0.try_wait().unwrap().is_none());
        assert!(!leader.0.wait().unwrap().success());
        assert_eq!(
            terminate_owned_leader(&leader.0, Instant::now() + Duration::from_secs(1))
                .unwrap_err()
                .raw_os_error(),
            Some(rustix::io::Errno::CHILD.raw_os_error())
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_group_signal_rejects_lost_reaping_ownership() {
        let mut child = OwnedChild(
            Command::new("/bin/sleep")
                .arg("60")
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        signal_owned_group(&child.0).unwrap();
        assert!(!child.0.wait().unwrap().success());
        assert_eq!(
            signal_owned_group(&child.0).unwrap_err().raw_os_error(),
            Some(rustix::io::Errno::CHILD.raw_os_error())
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn native_group_requires_exited_leader_and_no_other_member() {
        let mut leader = OwnedChild(
            Command::new("/bin/sleep")
                .arg("60")
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        assert!(!exited_leader_has_no_other_members(&leader.0).unwrap());
        let mut member = OwnedChild(
            Command::new("/bin/sleep")
                .arg("60")
                .process_group(i32::try_from(leader.0.id()).unwrap())
                .spawn()
                .unwrap(),
        );
        leader.0.kill().unwrap();
        observe_exit(&leader.0);
        assert!(!exited_leader_has_no_other_members(&leader.0).unwrap());
        member.0.kill().unwrap();
        member.0.wait().unwrap();
        assert!(exited_leader_has_no_other_members(&leader.0).unwrap());
        leader.0.wait().unwrap();
        assert!(exited_leader_has_no_other_members(&leader.0).is_err());
    }
}
