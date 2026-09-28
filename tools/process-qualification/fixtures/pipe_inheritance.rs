//! Standalone macOS probe. The driver includes the real production launch source.
use std::fs;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

mod production {
    include!(env!("DEADPAN_SPAWN_SOURCE"));

    // This module includes the real private static, so this observation proves
    // the production boundary is held rather than inferring it from scheduling.
    pub fn launch_lock_is_held() -> bool {
        matches!(
            SPAWN_LOCK.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        )
    }
}

unsafe extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct ReleasePipe(PathBuf);
impl ReleasePipe {
    fn release(&self) {
        fs::write(self.0.join("release"), b"").unwrap();
    }
}
impl Drop for ReleasePipe {
    fn drop(&mut self) {
        let _ = fs::write(self.0.join("release"), b"");
    }
}

fn launch(command: &mut Command, serialized: bool) -> OwnedChild {
    OwnedChild(
        if serialized {
            production::spawn(command)
        } else {
            command.spawn()
        }
        .unwrap(),
    )
}

fn main() {
    let mode = std::env::args().nth(1).unwrap();
    if mode == "keeper" {
        thread::sleep(Duration::from_secs(60));
        return;
    }
    assert!(mode == "raw" || mode == "serialized");
    let serialized = mode == "serialized";
    let root = PathBuf::from(std::env::var_os("DEADPAN_PIPE_PROBE_ROOT").unwrap());
    if serialized {
        assert_eq!(
            production::spawn(&mut Command::new(root.join("missing-executable")))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
    let paused = ReleasePipe(root.clone());
    fs::write(root.join("arm"), b"").unwrap();
    let first = thread::spawn(move || {
        launch(
            Command::new("/usr/bin/true")
                .env_clear()
                .stderr(Stdio::piped()),
            serialized,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while !root.join("ready").exists() {
        assert!(Instant::now() < deadline, "pipe interposition did not run");
        thread::park_timeout(Duration::from_millis(1));
    }
    assert_eq!(production::launch_lock_is_held(), serialized);
    let (attempt_tx, attempt_rx) = mpsc::sync_channel(0);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let second = thread::spawn(move || {
        attempt_tx.send(()).unwrap();
        let child = launch(
            Command::new(std::env::current_exe().unwrap())
                .arg("keeper")
                .env_clear(),
            serialized,
        );
        // Sending a failed receiver's OwnedChild drops and reaps it here.
        let _ = result_tx.send(child);
    });
    attempt_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let early = result_rx.recv_timeout(if serialized {
        Duration::from_millis(200)
    } else {
        Duration::from_secs(2)
    });
    let keeper = if serialized {
        assert!(
            matches!(early, Err(mpsc::RecvTimeoutError::Timeout)),
            "second launch completed while another launch's pipe was inheritable"
        );
        println!("serialized: second launch stays pending while the first pipe is held");
        None
    } else {
        Some(early.expect("raw negative control did not launch inside the open pipe window"))
    };
    paused.release();
    let mut keeper =
        keeper.unwrap_or_else(|| result_rx.recv_timeout(Duration::from_secs(2)).unwrap());
    second.join().unwrap();
    let mut first = first.join().unwrap();
    let mut stderr = first.0.stderr.take().unwrap();
    assert!(first.0.wait().unwrap().success());
    let fd = stderr.as_raw_fd();
    // Darwin F_GETFL=3, F_SETFL=4, O_NONBLOCK=4. Only this probe uses raw FFI.
    unsafe {
        let flags = fcntl(fd, 3);
        assert!(flags >= 0);
        assert_eq!(fcntl(fd, 4, flags | 4), 0);
    }
    assert!(
        keeper.0.try_wait().unwrap().is_none(),
        "keeper exited before EOF observation"
    );
    let mut byte = [0];
    if serialized {
        assert_eq!(stderr.read(&mut byte).unwrap(), 0);
        println!("serialized: original reaped; stderr is EOF while unrelated keeper remains alive");
    } else {
        assert_eq!(
            stderr.read(&mut byte).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        println!("raw: original reaped; stderr is WouldBlock while unrelated keeper remains alive");
    }
    keeper.0.kill().unwrap();
    keeper.0.wait().unwrap();
    assert_eq!(stderr.read(&mut byte).unwrap(), 0);
    println!("{mode}: keeper reaped; stderr is EOF");
}
