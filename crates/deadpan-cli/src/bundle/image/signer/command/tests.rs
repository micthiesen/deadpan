use super::*;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

fn assert_reaped(pid: u32) {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let pid = Pid::from_raw(i32::try_from(pid).unwrap()).unwrap();
    assert_eq!(
        waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT
        )
        .unwrap_err(),
        rustix::io::Errno::CHILD,
    );
}

#[test]
fn cancellation_after_spawn_confirms_group_cleanup_and_reaps_the_leader() {
    let cancelled = AtomicBool::new(false);
    let control = Control::new(&cancelled, Instant::now() + Duration::from_secs(5));
    let pid = Cell::new(None);
    let error = capture_with(Command::new("/bin/sleep").arg("60"), control, |child| {
        pid.set(Some(child.id()));
        cancelled.store(true, Ordering::Release);
        Ok(false)
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_reaped(pid.get().unwrap());
}

#[test]
fn a_hung_command_respects_the_shared_deadline_and_is_reaped() {
    let cancelled = AtomicBool::new(false);
    let control = Control::new(&cancelled, Instant::now() + Duration::from_millis(100));
    let pid = Cell::new(None);
    let error = capture_with(Command::new("/bin/sleep").arg("60"), control, |child| {
        pid.set(Some(child.id()));
        owned_exit(child)
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_reaped(pid.get().unwrap());
}

#[test]
fn an_observation_error_still_cleans_and_reaps_its_owned_command() {
    let cancelled = AtomicBool::new(false);
    let control = Control::new(&cancelled, Instant::now() + Duration::from_secs(5));
    let pid = Cell::new(None);
    let error = capture_with(Command::new("/bin/sleep").arg("60"), control, |child| {
        pid.set(Some(child.id()));
        Err(io::Error::other("injected ownership observation failure"))
    })
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected ownership observation failure")
    );
    assert_reaped(pid.get().unwrap());
}

#[test]
fn overflowing_capture_stops_and_reaps_the_entire_command_group() {
    let cancelled = AtomicBool::new(false);
    let control = Control::new(&cancelled, Instant::now() + Duration::from_secs(5));
    let pid = Cell::new(None);
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        "/usr/bin/head -c 131072 /dev/zero; exec /bin/sleep 60",
    ]);
    let error = capture_with(&mut command, control, |child| {
        pid.set(Some(child.id()));
        owned_exit(child)
    })
    .unwrap_err();
    assert!(error.to_string().contains("output exceeded its bound"));
    assert_reaped(pid.get().unwrap());
}

#[test]
fn cancellation_or_deadline_precedes_even_executable_lookup() {
    for (cancelled, deadline, expected) in [
        (
            true,
            Instant::now() + Duration::from_secs(5),
            io::ErrorKind::Interrupted,
        ),
        (false, Instant::now(), io::ErrorKind::TimedOut),
    ] {
        let cancelled = AtomicBool::new(cancelled);
        let error = capture(
            &mut Command::new("/no/such/native-signature-command"),
            Control::new(&cancelled, deadline),
        )
        .unwrap_err();
        assert_eq!(error.kind(), expected);
    }
}
