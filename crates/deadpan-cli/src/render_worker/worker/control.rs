//! A bounded control reader that remains responsive while native preparation runs.

use std::fs::File;
use std::io::{self, Read};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use deadpan_jobs::CancellationToken;
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

use super::super::protocol::{RenderHostMessage, RenderIdentity, read_host_message};

const POLL_INTERVAL: Duration = Duration::from_millis(2);

pub(crate) struct ControlReader {
    file: File,
    stopped: Arc<AtomicBool>,
    deadline: Instant,
    interrupted_for_shutdown: bool,
    bytes_read: usize,
}

impl ControlReader {
    pub(crate) fn new(file: File, stopped: Arc<AtomicBool>, deadline: Instant) -> io::Result<Self> {
        let flags = fcntl_getfl(&file)?;
        fcntl_setfl(&file, flags | OFlags::NONBLOCK)?;
        Ok(Self {
            file,
            stopped,
            deadline,
            interrupted_for_shutdown: false,
            bytes_read: 0,
        })
    }

    pub(super) fn set_deadline(&mut self, deadline: Instant) {
        self.deadline = deadline;
        self.bytes_read = 0;
    }
}

impl Read for ControlReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            if Instant::now() >= self.deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "render control deadline expired",
                ));
            }
            match self.file.read(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if self.stopped.load(Ordering::Acquire) {
                        self.interrupted_for_shutdown = true;
                        // Drain available control bytes before honoring stop.
                        // Interrupted would be retried by read_frame forever.
                        return Err(io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            "render control reader stopped",
                        ));
                    }
                    thread::park_timeout(POLL_INTERVAL);
                }
                // An interrupted read says nothing about queued bytes. Only
                // EOF or WouldBlock proves this drain reached its boundary.
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Ok(count) => {
                    self.bytes_read = self.bytes_read.saturating_add(count);
                    return Ok(count);
                }
                Err(error) => return Err(error),
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ControlEnd {
    Stopped,
    Cancelled,
}

pub(crate) struct ControlPump {
    stopped: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<ControlEnd, String>>>,
}

impl ControlPump {
    pub(super) fn start(
        reader: ControlReader,
        identity: RenderIdentity,
        token: CancellationToken,
        deadline: Instant,
    ) -> io::Result<Self> {
        Self::start_with(reader, deadline, move |reader| {
            match read_host_message(reader) {
                Ok(Some(RenderHostMessage::Cancel {
                    identity: received,
                    cancellation_token,
                    ..
                })) if received == identity && cancellation_token == token => {
                    Ok(ControlEnd::Cancelled)
                }
                Ok(Some(RenderHostMessage::Cancel { .. })) => {
                    Err("render cancellation identity or token does not match".to_owned())
                }
                Ok(Some(RenderHostMessage::Prepare { .. })) => {
                    Err("render worker received a second Prepare message".to_owned())
                }
                Ok(None) => Err("render host closed its control stream".to_owned()),
                Err(error) => Err(format!("invalid render control: {error}")),
            }
        })
    }

    /// Share the nonblocking lifetime and shutdown drain with a strictly
    /// validated protocol-specific reader. The closure consumes one bounded
    /// message and never retains this reader or the host's input bytes.
    pub(crate) fn start_with(
        mut reader: ControlReader,
        deadline: Instant,
        receive: impl FnOnce(&mut ControlReader) -> Result<ControlEnd, String> + Send + 'static,
    ) -> io::Result<Self> {
        reader.set_deadline(deadline);
        let stopped = Arc::clone(&reader.stopped);
        let cancelled = Arc::new(AtomicBool::new(false));
        let thread_cancelled = Arc::clone(&cancelled);
        let thread = thread::Builder::new()
            .name("deadpan-render-control".to_owned())
            .spawn(move || {
                let result = receive(&mut reader);
                if reader.interrupted_for_shutdown {
                    if reader.bytes_read != 0 {
                        thread_cancelled.store(true, Ordering::Release);
                        return Err("render host left an incomplete control frame".to_owned());
                    }
                    return Ok(ControlEnd::Stopped);
                }
                thread_cancelled.store(true, Ordering::Release);
                result
            })?;
        Ok(Self {
            stopped,
            cancelled,
            thread: Some(thread),
        })
    }

    pub(crate) fn cancelled(&self) -> &AtomicBool {
        &self.cancelled
    }

    pub(crate) fn finish(&mut self) -> Result<ControlEnd, String> {
        self.stopped.store(true, Ordering::Release);
        let Some(thread) = self.thread.take() else {
            return Ok(ControlEnd::Stopped);
        };
        thread.thread().unpark();
        thread
            .join()
            .map_err(|_| "render control reader panicked".to_owned())?
    }
}

impl Drop for ControlPump {
    fn drop(&mut self) {
        // The owned descriptor is nonblocking; stop/unpark interrupts even an
        // incomplete frame or a live host that has sent no subsequent bytes.
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    use deadpan_jobs::{AttemptId, RequestId, write_frame};

    use super::*;
    use crate::render_worker::protocol::PROTOCOL_VERSION;

    fn identity() -> RenderIdentity {
        RenderIdentity {
            request_id: RequestId::new("test-render").expect("identifier"),
            attempt_id: AttemptId::new("attempt-1").expect("identifier"),
        }
    }

    fn pair() -> (UnixStream, ControlReader) {
        let (host, child) = UnixStream::pair().expect("control pair");
        let reader = ControlReader::new(
            File::from(OwnedFd::from(child)),
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_secs(2),
        )
        .expect("nonblocking reader");
        (host, reader)
    }

    fn start(reader: ControlReader) -> ControlPump {
        ControlPump::start(
            reader,
            identity(),
            CancellationToken::new("token").expect("token"),
            Instant::now() + Duration::from_secs(2),
        )
        .expect("control pump")
    }

    fn await_cancellation(pump: &ControlPump) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while !pump.cancelled().load(Ordering::Acquire) {
            assert!(Instant::now() < deadline, "control was not consumed");
            thread::park_timeout(Duration::from_millis(1));
        }
    }

    #[test]
    fn valid_cancel_sets_atomic_without_waiting_for_preparation() {
        let (mut host, reader) = pair();
        let mut pump = start(reader);
        write_frame(
            &mut host,
            &RenderHostMessage::Cancel {
                protocol: PROTOCOL_VERSION,
                identity: identity(),
                cancellation_token: CancellationToken::new("token").expect("token"),
            },
        )
        .expect("cancel frame");
        await_cancellation(&pump);
        assert_eq!(pump.finish(), Ok(ControlEnd::Cancelled));
    }

    #[test]
    fn terminal_shutdown_drains_an_already_queued_cancel() {
        let (mut host, reader) = pair();
        write_frame(
            &mut host,
            &RenderHostMessage::Cancel {
                protocol: PROTOCOL_VERSION,
                identity: identity(),
                cancellation_token: CancellationToken::new("token").expect("token"),
            },
        )
        .expect("cancel frame");
        let mut pump = start(reader);
        assert_eq!(pump.finish(), Ok(ControlEnd::Cancelled));
    }

    #[test]
    fn wrong_token_and_host_eof_fail_closed() {
        let (mut host, reader) = pair();
        let mut pump = start(reader);
        write_frame(
            &mut host,
            &RenderHostMessage::Cancel {
                protocol: PROTOCOL_VERSION,
                identity: identity(),
                cancellation_token: CancellationToken::new("other").expect("token"),
            },
        )
        .expect("cancel frame");
        await_cancellation(&pump);
        assert!(pump.finish().expect_err("wrong token").contains("token"));

        let (host, reader) = pair();
        let mut pump = start(reader);
        drop(host);
        await_cancellation(&pump);
        assert!(pump.finish().expect_err("EOF").contains("closed"));
    }

    #[test]
    fn queued_invalid_control_and_partial_body_cannot_be_hidden_by_finish() {
        let (mut host, reader) = pair();
        write_frame(
            &mut host,
            &RenderHostMessage::Cancel {
                protocol: PROTOCOL_VERSION,
                identity: identity(),
                cancellation_token: CancellationToken::new("wrong-token").unwrap(),
            },
        )
        .unwrap();
        let mut pump = start(reader);
        assert!(pump.finish().unwrap_err().contains("token"));

        let (mut host, reader) = pair();
        host.write_all(&100_u32.to_be_bytes()).unwrap();
        host.write_all(b"{\"op\":").unwrap();
        let mut pump = start(reader);
        assert!(pump.finish().unwrap_err().contains("incomplete"));
    }

    #[test]
    fn shutdown_joins_a_partial_frame_with_the_host_still_open() {
        let (mut host, reader) = pair();
        host.write_all(&[0, 0]).expect("partial header");
        let mut pump = start(reader);
        let before = Instant::now();
        assert!(
            pump.finish()
                .expect_err("partial frame")
                .contains("incomplete")
        );
        assert!(before.elapsed() < Duration::from_secs(1));
        assert!(pump.cancelled().load(Ordering::Acquire));
    }

    #[test]
    fn shutdown_joins_an_idle_control_stream() {
        let (_host, reader) = pair();
        let mut pump = start(reader);
        let before = Instant::now();
        assert_eq!(pump.finish(), Ok(ControlEnd::Stopped));
        assert!(before.elapsed() < Duration::from_secs(1));
        assert!(!pump.cancelled().load(Ordering::Acquire));
    }

    #[test]
    fn oversized_control_rejects_without_waiting_for_its_payload() {
        let (mut host, reader) = pair();
        let mut pump = start(reader);
        host.write_all(&u32::MAX.to_be_bytes()).expect("header");
        await_cancellation(&pump);
        assert!(pump.finish().is_err());
    }

    #[test]
    fn incomplete_initial_handshake_has_a_deadline() {
        let (_host, mut reader) = pair();
        reader.set_deadline(Instant::now());
        assert!(read_host_message(&mut reader).is_err());
    }
}
