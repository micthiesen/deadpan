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
