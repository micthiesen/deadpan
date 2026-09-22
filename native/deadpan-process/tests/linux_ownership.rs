#![cfg(target_os = "linux")]

use std::io::{self, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use deadpan_native_process::{signal_owned_group, terminate_owned_leader};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, getpgid, getpgrp, setpgid, waitid};

const PARENT_GROUP: &str = "DEADPAN_PROCESS_TEST_PARENT_GROUP";

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if terminate_owned_leader(&self.0, Instant::now() + Duration::from_secs(1)).is_ok() {
            let _ = self.0.wait();
        }
    }
}

#[test]
fn escaped_leader_helper() {
    let Ok(parent_group) = std::env::var(PARENT_GROUP) else {
        return;
    };
    let parent_group = Pid::from_raw(parent_group.parse().unwrap()).unwrap();
    let own_group = getpgrp();
    assert_eq!(
        own_group.as_raw_pid(),
        i32::try_from(std::process::id()).unwrap()
    );
    assert_ne!(own_group, parent_group);
    setpgid(None, Some(parent_group)).unwrap();
    assert_eq!(getpgrp(), parent_group);
    {
        let mut ready = io::stderr().lock();
        ready.write_all(b"ready\n").unwrap();
        ready.flush().unwrap();
    }
    // The parent retains this pipe while checking teardown. EOF also releases
    // the helper if the parent exits before it can run its checked cleanup.
    let _ = io::stdin().read(&mut [0]);
}

#[test]
fn escaped_owned_leader_requires_checked_fallback_before_reaping() {
    let parent_group = getpgrp();
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "escaped_leader_helper", "--nocapture"])
            .env(PARENT_GROUP, parent_group.as_raw_pid().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    let child_pid = Pid::from_child(&child.0);
    assert_ne!(child_pid, parent_group);
    let mut ready = child.0.stderr.take().unwrap();
    let flags = fcntl_getfl(&ready).unwrap();
    fcntl_setfl(&ready, flags | OFlags::NONBLOCK).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = 0;
    let mut reply = [0; 6];
    while received < reply.len() {
        assert!(
            Instant::now() < deadline,
            "escaped child did not become ready"
        );
        match ready.read(&mut reply[received..]) {
            Ok(0) => panic!("escaped child closed its readiness pipe"),
            Ok(count) => received += count,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::park_timeout(Duration::from_millis(2));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => panic!("read escaped child readiness: {error}"),
        }
    }
    assert_eq!(&reply, b"ready\n");
    assert_eq!(getpgid(Some(child_pid)).unwrap(), parent_group);

    // Signal only the child's original, now-empty group. The parent's group
    // remains deliberately outside every cleanup API invocation.
    let error = signal_owned_group(&child.0).unwrap_err();
    assert_eq!(
        error.raw_os_error(),
        Some(rustix::io::Errno::SRCH.raw_os_error())
    );
    assert!(
        waitid(
            WaitId::Pid(child_pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .is_none(),
        "a group signal unexpectedly terminated the escaped leader"
    );

    terminate_owned_leader(&child.0, Instant::now() + Duration::from_secs(1)).unwrap();
    let status = child.0.wait().unwrap();
    assert_eq!(status.signal(), Some(Signal::KILL.as_raw()));
    for error in [
        signal_owned_group(&child.0).unwrap_err(),
        terminate_owned_leader(&child.0, Instant::now() + Duration::from_secs(1)).unwrap_err(),
    ] {
        assert_eq!(
            error.raw_os_error(),
            Some(rustix::io::Errno::CHILD.raw_os_error())
        );
    }
}
