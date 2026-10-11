//! Bounded bidirectional byte relay. TLS remains between yt-dlp and YouTube.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};

const BUFFER: usize = 32 * 1024;
const POLL: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 100_000_000,
};

pub(super) fn header(stream: &mut TcpStream, stopped: &AtomicBool) -> io::Result<Vec<u8>> {
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    stream.set_write_timeout(Some(Duration::from_millis(100)))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut header = Vec::with_capacity(super::protocol::HEADER_LIMIT);
    let mut buffer = [0u8; 1024];
    while !stopped.load(Ordering::Acquire) && Instant::now() < deadline {
        let capacity = (super::protocol::HEADER_LIMIT - header.len()).min(buffer.len());
        if capacity == 0 {
            break;
        }
        match stream.read(&mut buffer[..capacity]) {
            Ok(0) => break,
            Ok(count) => header.extend_from_slice(&buffer[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
        if let Some(end) = header.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            if end + 4 == header.len() {
                return Ok(header);
            }
            return Err(io::Error::other("data before CONNECT acceptance"));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "incomplete proxy request",
    ))
}

struct Direction {
    bytes: Box<[u8; BUFFER]>,
    start: usize,
    end: usize,
    eof: bool,
    shutdown: bool,
}

impl Direction {
    fn new() -> Self {
        Self {
            bytes: Box::new([0; BUFFER]),
            start: 0,
            end: 0,
            eof: false,
            shutdown: false,
        }
    }

    fn pending(&self) -> bool {
        self.start < self.end
    }
    fn readable(&self) -> bool {
        !self.eof && self.end < BUFFER
    }
    fn finished(&self) -> bool {
        self.eof && !self.pending()
    }

    fn transfer(&mut self, mut source: &TcpStream, mut target: &TcpStream) -> io::Result<usize> {
        let mut progress = 0;
        if self.pending() {
            match target.write(&self.bytes[self.start..self.end]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(count) => {
                    self.start += count;
                    progress += count;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        if !self.pending() {
            self.start = 0;
            self.end = 0;
        }
        if self.readable() {
            match source.read(&mut self.bytes[self.end..]) {
                Ok(0) => self.eof = true,
                Ok(count) => {
                    self.end += count;
                    progress += count;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        if self.finished() && !self.shutdown {
            let _ = target.shutdown(Shutdown::Write);
            self.shutdown = true;
        }
        Ok(progress)
    }
}

pub(super) fn relay(client: TcpStream, server: TcpStream, stopped: &AtomicBool) -> io::Result<()> {
    client.set_nonblocking(true)?;
    server.set_nonblocking(true)?;
    let mut outbound = Direction::new();
    let mut inbound = Direction::new();
    let mut last_progress = Instant::now();
    while !stopped.load(Ordering::Acquire) {
        let progress = outbound.transfer(&client, &server)? + inbound.transfer(&server, &client)?;
        if outbound.finished() && inbound.finished() {
            return Ok(());
        }
        if progress > 0 {
            last_progress = Instant::now();
            continue;
        }
        if last_progress.elapsed() >= Duration::from_secs(60) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut client_events = PollFlags::empty();
        let mut server_events = PollFlags::empty();
        if outbound.readable() {
            client_events |= PollFlags::IN;
        }
        if inbound.pending() {
            client_events |= PollFlags::OUT;
        }
        if inbound.readable() {
            server_events |= PollFlags::IN;
        }
        if outbound.pending() {
            server_events |= PollFlags::OUT;
        }
        let mut fds = [
            PollFd::new(&client, client_events),
            PollFd::new(&server, server_events),
        ];
        // An inactive half-closed descriptor may keep reporting HUP. Exclude
        // it rather than spinning while the opposite direction is idle.
        let fds = if client_events.is_empty() {
            &mut fds[1..]
        } else if server_events.is_empty() {
            &mut fds[..1]
        } else {
            &mut fds[..]
        };
        match poll(fds, Some(&POLL)) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
