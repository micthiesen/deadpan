use super::namespace::{Discovery, Lease};
use super::wire::{Budget, Reader, Writer};
use super::*;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

/// A discovered owner, with its private directory descriptors retained. This
/// contains no SQLite connection or writer-lock descriptor. Each call uses one
/// connection; a failed delivery is never retried by this transport.
pub struct Client {
    package: PathBuf,
    discovery: Discovery,
    namespace: Lease,
    connected: Option<UnixStream>,
}
impl Client {
    pub fn discover(package: &Path) -> Result<Option<Self>, HostError> {
        // Match ProjectStore's user-facing alias handling once. All later
        // discovery rechecks use this pinned path and the actual package
        // identity; retargeting the original alias cannot retarget the client.
        let package = package.canonicalize().map_err(|_| HostError::stale())?;
        let Some(record) =
            deadpan_store::host_owner::read_discovery(&package).map_err(|_| HostError::stale())?
        else {
            return Ok(None);
        };
        let discovery: Discovery =
            serde_json::from_slice(&record.bytes).map_err(|_| HostError::invalid())?;
        discovery.validate()?;
        if record.package_identity != discovery.package_identity {
            return Err(HostError::stale());
        }
        let namespace = Lease::open(&discovery)?;
        let mut client = Self {
            package,
            discovery,
            namespace,
            connected: None,
        };
        client.connected = Some(client.connect(Instant::now() + CONNECT_TIMEOUT)?);
        Ok(Some(client))
    }
    pub fn owner_id(&self) -> Uuid {
        self.discovery.owner_id
    }
    pub fn package_identity(&self) -> &PackageIdentity {
        &self.discovery.package_identity
    }

    pub fn request(&mut self, payload: Value) -> Result<Value, HostError> {
        self.request_until(Uuid::new_v4(), payload, Instant::now() + CLIENT_TIMEOUT)
    }
    /// Supply the already allocated request identity when semantic callers need
    /// correlation. Reusing it does not grant retry/idempotency semantics here.
    pub fn request_until(
        &mut self,
        request_id: Uuid,
        payload: Value,
        deadline: Instant,
    ) -> Result<Value, HostError> {
        if request_id.is_nil() {
            return Err(HostError::invalid());
        }
        if Instant::now() >= deadline {
            return Err(HostError::timeout());
        }
        self.recheck()?;
        let request = Request {
            version: VERSION,
            owner_id: self.owner_id(),
            secret: self.discovery.secret.clone(),
            package_identity: self.discovery.package_identity,
            request_id,
            payload,
        };
        let mut writer = Writer::new(&request)?;
        drop(request);
        let mut stream = match self.connected.take() {
            Some(stream) => stream,
            None => self.connect(deadline)?,
        };
        loop {
            match writer.poll(&mut stream, &mut Budget::new(), deadline) {
                Ok(true) => break,
                Ok(false) => pause(deadline),
                Err(error) => {
                    return Err(if writer.started() {
                        HostError::uncertain()
                    } else {
                        error
                    });
                }
            }
        }
        drop(writer);
        let mut reader = Reader::default();
        let mut available = MAX_FRAME_BYTES;
        let bytes = loop {
            match reader.poll(&mut stream, &mut Budget::new(), &mut available, deadline) {
                Ok(Some(bytes)) => break bytes,
                Ok(None) => pause(deadline),
                Err(_) => return Err(HostError::uncertain()),
            }
        };
        wire::parsed_reservation(&bytes).map_err(|_| HostError::uncertain())?;
        let response: Response =
            serde_json::from_slice(&bytes).map_err(|_| HostError::uncertain())?;
        if response.version != VERSION
            || response.owner_id != self.owner_id()
            || response.package_identity != self.discovery.package_identity
            || response.request_id != request_id
        {
            return Err(HostError::uncertain());
        }
        // After complete authenticated response identity, losing discovery no
        // longer erases an observed committed receipt. Recheck before each next
        // request instead. The caller owns the receipt's semantic validation.
        match response.result {
            ResponseResult::Ok { payload } => Ok(payload),
            ResponseResult::Error { error } => {
                if error.code.len() > 128 || error.message.len() > 16 * 1024 {
                    return Err(HostError::uncertain());
                }
                Err(error)
            }
        }
    }
    fn recheck(&self) -> Result<(), HostError> {
        self.namespace.recheck(&self.discovery)?;
        let Some(record) = deadpan_store::host_owner::read_discovery(&self.package)
            .map_err(|_| HostError::stale())?
        else {
            return Err(HostError::stale());
        };
        let fresh: Discovery =
            serde_json::from_slice(&record.bytes).map_err(|_| HostError::stale())?;
        fresh.validate()?;
        if record.package_identity != self.discovery.package_identity
            || fresh.package_identity != self.discovery.package_identity
            || fresh.owner_id != self.owner_id()
            || fresh.directory != self.discovery.directory
            || fresh.socket != self.discovery.socket
            || !namespace::same_secret(&fresh.secret, &self.discovery.secret)
        {
            return Err(HostError::stale());
        }
        Ok(())
    }
    fn connect(&self, deadline: Instant) -> Result<UnixStream, HostError> {
        if Instant::now() >= deadline {
            return Err(HostError::timeout());
        }
        self.namespace.recheck(&self.discovery)?;
        let fd = wire::socket().map_err(|_| HostError::io())?;
        let address = rustix::net::SocketAddrUnix::new(self.namespace.path())
            .map_err(|_| HostError::invalid())?;
        match rustix::net::connect(&fd, &address) {
            Ok(()) => {}
            Err(
                rustix::io::Errno::INPROGRESS | rustix::io::Errno::AGAIN | rustix::io::Errno::INTR,
            ) => {}
            Err(_) => return Err(HostError::unavailable()),
        }
        let stream = UnixStream::from(fd);
        loop {
            if Instant::now() >= deadline {
                return Err(HostError::timeout());
            }
            if stream
                .take_error()
                .map_err(|_| HostError::unavailable())?
                .is_some()
            {
                return Err(HostError::unavailable());
            }
            if stream.peer_addr().is_ok() {
                break;
            }
            pause(deadline);
        }
        self.namespace.recheck(&self.discovery)?;
        Ok(stream)
    }
}
fn pause(deadline: Instant) {
    if let Some(left) = deadline.checked_duration_since(Instant::now()) {
        std::thread::park_timeout(left.min(Duration::from_millis(2)));
    }
}
