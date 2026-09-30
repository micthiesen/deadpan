use super::*;
use rustix::net::{AddressFamily, SocketFlags, SocketType};
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;

pub(super) fn socket() -> io::Result<OwnedFd> {
    deadpan_native_process::with_descriptor_creation_guard(|| {
        let fd = rustix::net::socket_with(AddressFamily::UNIX, SocketType::STREAM, flags(), None)?;
        configure(&fd)?;
        Ok(fd)
    })
}
pub(super) fn accept(listener: impl AsFd) -> io::Result<UnixStream> {
    deadpan_native_process::try_with_descriptor_creation_guard(|| {
        let fd = rustix::net::accept_with(listener, flags())?;
        configure(&fd)?;
        Ok(UnixStream::from(fd))
    })?
    .ok_or_else(|| io::Error::from(io::ErrorKind::WouldBlock))
}
fn flags() -> SocketFlags {
    #[cfg(target_os = "linux")]
    {
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK
    }
    #[cfg(target_os = "macos")]
    {
        SocketFlags::empty()
    }
}
fn configure(fd: impl AsFd) -> io::Result<()> {
    rustix::io::fcntl_setfd(&fd, rustix::io::FdFlags::CLOEXEC)?;
    let previous = rustix::fs::fcntl_getfl(&fd)?;
    rustix::fs::fcntl_setfl(&fd, previous | rustix::fs::OFlags::NONBLOCK)?;
    #[cfg(target_os = "macos")]
    rustix::net::sockopt::set_socket_nosigpipe(&fd, true)?;
    Ok(())
}

pub(super) struct Budget {
    bytes: usize,
    calls: usize,
}
impl Budget {
    pub(super) fn new() -> Self {
        Self {
            bytes: POLL_BYTES,
            calls: POLL_SYSCALLS,
        }
    }
    pub(super) fn call(&mut self) -> bool {
        if self.calls == 0 || self.bytes == 0 {
            return false;
        }
        self.calls -= 1;
        true
    }
}

#[derive(Default)]
pub(super) struct Reader {
    header: [u8; 4],
    header_read: usize,
    pub(super) length: Option<usize>,
    body: Vec<u8>,
}
impl Reader {
    /// Reserve the complete admitted frame against the endpoint's aggregate
    /// budget before allocating it. Partial headers allocate no payload bytes.
    pub(super) fn poll(
        &mut self,
        stream: &mut UnixStream,
        budget: &mut Budget,
        available: &mut usize,
        deadline: Instant,
    ) -> Result<Option<Vec<u8>>, HostError> {
        loop {
            if Instant::now() >= deadline {
                return Err(HostError::timeout());
            }
            if self.header_read == 4 && self.length.is_none() {
                let length = usize::try_from(u32::from_be_bytes(self.header))
                    .map_err(|_| HostError::limit())?;
                if length == 0 || length > MAX_FRAME_BYTES || length > *available {
                    return Err(HostError::limit());
                }
                self.body
                    .try_reserve_exact(length)
                    .map_err(|_| HostError::limit())?;
                self.length = Some(length);
                *available -= length;
            }
            if self.length.is_some_and(|length| self.body.len() == length) {
                return Ok(Some(std::mem::take(&mut self.body)));
            }
            if !budget.call() {
                return Ok(None);
            }
            if self.header_read < 4 {
                let end = (self.header_read + budget.bytes).min(4);
                match stream.read(&mut self.header[self.header_read..end]) {
                    Ok(0) => return Err(HostError::unavailable()),
                    Ok(count) => {
                        self.header_read += count;
                        budget.bytes -= count;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => return Err(HostError::io()),
                }
            } else {
                let old = self.body.len();
                let end = self
                    .length
                    .expect("admitted length")
                    .min(old + budget.bytes.min(64 * 1024));
                self.body.resize(end, 0);
                match stream.read(&mut self.body[old..end]) {
                    Ok(0) => {
                        self.body.truncate(old);
                        return Err(HostError::unavailable());
                    }
                    Ok(count) => {
                        self.body.truncate(old + count);
                        budget.bytes -= count;
                    }
                    Err(error) => {
                        self.body.truncate(old);
                        match error.kind() {
                            io::ErrorKind::WouldBlock => return Ok(None),
                            io::ErrorKind::Interrupted => {}
                            _ => return Err(HostError::io()),
                        }
                    }
                }
            }
        }
    }
}

pub(super) struct Writer {
    bytes: Vec<u8>,
    offset: usize,
}
impl Writer {
    pub(super) fn new(value: &impl Serialize) -> Result<Self, HostError> {
        Self::limited(value, MAX_FRAME_BYTES + 4)
    }
    pub(super) fn limited(value: &impl Serialize, available: usize) -> Result<Self, HostError> {
        let mut count = Count {
            length: 4,
            maximum: available.min(MAX_FRAME_BYTES + 4),
        };
        serde_json::to_writer(&mut count, value).map_err(|_| HostError::limit())?;
        let mut bytes = LimitedVec { bytes: Vec::new() };
        bytes
            .bytes
            .try_reserve_exact(count.length)
            .map_err(|_| HostError::limit())?;
        bytes.bytes.resize(4, 0);
        serde_json::to_writer(&mut bytes, value).map_err(|_| HostError::limit())?;
        // A wire-sized reply can still expand beyond the receiving parser's
        // memory budget. Refuse locally before sending any bytes, preserving
        // the server's ability to send a compact durable receipt instead.
        parsed_reservation(&bytes.bytes[4..])?;
        let length = u32::try_from(bytes.bytes.len() - 4).map_err(|_| HostError::limit())?;
        bytes.bytes[..4].copy_from_slice(&length.to_be_bytes());
        Ok(Self {
            bytes: bytes.bytes,
            offset: 0,
        })
    }
    pub(super) fn length(&self) -> usize {
        self.bytes.len()
    }
    pub(super) fn started(&self) -> bool {
        self.offset != 0
    }
    pub(super) fn poll(
        &mut self,
        stream: &mut UnixStream,
        budget: &mut Budget,
        deadline: Instant,
    ) -> Result<bool, HostError> {
        while self.offset < self.bytes.len() {
            if Instant::now() >= deadline {
                return Err(HostError::timeout());
            }
            if !budget.call() {
                return Ok(false);
            }
            let end = self
                .bytes
                .len()
                .min(self.offset + budget.bytes.min(64 * 1024));
            match stream.write(&self.bytes[self.offset..end]) {
                Ok(0) => return Err(HostError::unavailable()),
                Ok(count) => {
                    self.offset += count;
                    budget.bytes -= count;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return Err(HostError::io()),
            }
        }
        Ok(true)
    }
}
struct LimitedVec {
    bytes: Vec<u8>,
}
struct Count {
    length: usize,
    maximum: usize,
}
impl Write for Count {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.len() > self.maximum.saturating_sub(self.length) {
            return Err(io::Error::other("frame limit"));
        }
        self.length += input.len();
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Write for LimitedVec {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.len() > self.bytes.capacity().saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("frame limit"));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Reserve raw JSON plus conservative owned Value space before parsing. Flat
/// objects/arrays can allocate far more than their textual byte length.
pub(super) fn parsed_reservation(bytes: &[u8]) -> Result<usize, HostError> {
    let mut reserved = bytes
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(64 * 1024))
        .ok_or_else(HostError::limit)?;
    let mut quoted = false;
    let mut escaped = false;
    for byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else {
            if *byte == b'"' {
                quoted = true;
            }
            if matches!(*byte, b'"' | b'{' | b'[' | b',' | b':') {
                reserved = reserved.checked_add(256).ok_or_else(HostError::limit)?;
            }
        }
        if reserved > MAX_BUFFER_BYTES {
            return Err(HostError::limit());
        }
    }
    Ok(reserved)
}
