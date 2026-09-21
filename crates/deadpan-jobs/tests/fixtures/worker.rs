//! Standard-library-only hostile subprocess fixture, compiled by integration tests.
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::{env, fs, process, thread, time::Duration};

fn packet() -> Vec<u8> {
    let mut header = [0; 4];
    io::stdin().read_exact(&mut header).unwrap();
    let length = u32::from_be_bytes(header) as usize;
    assert!(length <= 256 * 1024);
    let mut bytes = vec![0; length];
    io::stdin().read_exact(&mut bytes).unwrap();
    bytes
}

fn hang() {
    thread::sleep(Duration::from_secs(60));
}

fn main() {
    let mode = env::args().nth(1).unwrap();
    if mode == "no-read" || mode == "descendant" {
        hang();
        return;
    }
    let request = packet();
    fs::write("worker.pid", process::id().to_string()).unwrap();
    fs::write("received.json", request).unwrap();
    if mode == "orphan" || mode == "escaped" {
        let mut command = process::Command::new(env::current_exe().unwrap());
        command.arg("descendant");
        if mode == "escaped" {
            command.process_group(0);
        }
        let child = command.spawn().unwrap();
        fs::write("descendant.pid", child.id().to_string()).unwrap();
        return;
    }
    if mode == "no-terminal" {
        return;
    }
    if mode == "huge" {
        io::stdout().write_all(&u32::MAX.to_be_bytes()).unwrap();
        io::stdout().flush().unwrap();
        hang();
        return;
    }
    if mode == "truncated" {
        io::stdout().write_all(&12_u32.to_be_bytes()).unwrap();
        io::stdout().write_all(b"{").unwrap();
        return;
    }
    if mode == "cancel" {
        fs::write("listening", b"ready").unwrap();
        fs::write("cancellation.json", packet()).unwrap();
    }
    if mode == "stderr" {
        io::stderr().write_all(&vec![b'd'; 256 * 1024]).unwrap();
    }
    if mode == "environment" {
        assert!(env::var_os("PATH").is_none());
        assert!(env::var_os("HOME").is_none());
        assert_eq!(env::var("DEADPAN_WORKER_ALLOWED").unwrap(), "private");
        fs::write(
            "location.txt",
            env::current_dir().unwrap().to_string_lossy().as_bytes(),
        )
        .unwrap();
    }
    let response = fs::read("responses.bin").unwrap();
    if mode == "flood" {
        loop {
            if io::stdout().write_all(&response).is_err() {
                break;
            }
        }
        return;
    }
    if mode == "fragmented" {
        for byte in response {
            io::stdout().write_all(&[byte]).unwrap();
        }
    } else {
        io::stdout().write_all(&response).unwrap();
    }
    io::stdout().flush().unwrap();
    if mode == "hang-after-terminal" {
        hang();
    }
    if mode == "exit-failure" {
        process::exit(7);
    }
}
