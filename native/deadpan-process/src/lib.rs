//! Narrow Darwin qualification of an otherwise failed process-group signal.
#![cfg(target_os = "macos")]

use std::io;
use std::process::Child;

use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};

/// Whether an unreaped exited child has no other member in its process group.
///
/// The caller must have created this child as its group leader and retain sole
/// reaping ownership. This function never signals or reaps it. An inaccessible
/// group, unknown membership, or another member never qualifies as empty. It is
/// a snapshot, not a sandbox or a guarantee against processes joining later.
pub fn exited_leader_has_no_other_members(child: &Child) -> io::Result<bool> {
    let pid = Pid::from_child(child);
    if pid == Pid::INIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "init is not a worker",
        ));
    }
    if waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )?
    .is_none()
    {
        return Ok(false);
    }
    // Two slots suffice: any non-leader or full buffer fails closed, regardless
    // of how many additional members might have been truncated.
    let mut members = [0; 2];
    let bytes = group_members(child.id(), &mut members)?;
    only_leader(child.id(), bytes, members)
}

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
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::{Duration, Instant};

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

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
