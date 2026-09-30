use super::namespace::{Discovery, Lease};
use super::wire::{Budget, Reader, Writer};
use super::*;
use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;

enum State {
    Reading(Reader),
    Awaiting,
    Writing(Writer),
}
struct Connection {
    ticket: ConnectionTicket,
    stream: UnixStream,
    state: State,
    deadline: Instant,
    request_id: Uuid,
    reserved: usize,
}

/// Poll this on the project service thread. Network I/O is nonblocking and
/// bounded per poll. At most four admitted payloads may be outstanding; the
/// owner must finish or reject each ticket. This is not a command replay cache.
pub struct Endpoint {
    listener: OwnedFd,
    namespace: Lease,
    discovery: Discovery,
    owner: WriterOwnerHandle,
    connections: Vec<Connection>,
    serial: u64,
    round_robin: usize,
}
impl Endpoint {
    pub fn bind(store: &mut ProjectStore) -> Result<Self, HostError> {
        let owner = store
            .writer_owner_handle()
            .map_err(|_| HostError::stale())?;
        let owner_id = Uuid::new_v4();
        let namespace = Lease::create(owner_id)?;
        let setup = (|| {
            let listener = wire::socket().map_err(|_| HostError::io())?;
            let address =
                rustix::net::SocketAddrUnix::new(namespace.path()).map_err(|_| HostError::io())?;
            rustix::net::bind(&listener, &address).map_err(|_| HostError::io())?;
            let socket = namespace.protect_socket()?;
            rustix::net::listen(
                &listener,
                i32::try_from(MAX_CONNECTIONS).expect("small connection cap"),
            )
            .map_err(|_| HostError::io())?;
            let discovery = Discovery {
                version: VERSION,
                owner_id,
                secret: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
                package_identity: owner.package_identity(),
                directory: namespace.directory_identity(),
                socket,
            };
            let bytes = serde_json::to_vec(&discovery).map_err(|_| HostError::invalid())?;
            store
                .publish_host_discovery(&owner, &bytes)
                .map_err(|_| HostError::stale())?;
            Ok((listener, discovery))
        })();
        let (listener, discovery) = match setup {
            Ok(value) => value,
            Err(error) => {
                namespace.cleanup(None);
                return Err(error);
            }
        };
        Ok(Self {
            listener,
            namespace,
            discovery,
            owner,
            connections: Vec::new(),
            serial: 0,
            round_robin: 0,
        })
    }
    pub fn owner_id(&self) -> Uuid {
        self.discovery.owner_id
    }
    pub fn owner_handle(&self) -> &WriterOwnerHandle {
        &self.owner
    }
    pub fn package_identity(&self) -> &PackageIdentity {
        &self.discovery.package_identity
    }

    pub fn poll(&mut self) -> Vec<Incoming> {
        if self.owner.is_closed() || self.namespace.recheck(&self.discovery).is_err() {
            self.connections.clear();
            return Vec::new();
        }
        let mut budget = Budget::new();
        while self.connections.len() < MAX_CONNECTIONS && budget.call() {
            match wire::accept(&self.listener) {
                Ok(stream) => {
                    let Some(serial) = self.serial.checked_add(1) else {
                        break;
                    };
                    self.serial = serial;
                    self.connections.push(Connection {
                        ticket: ConnectionTicket {
                            endpoint: self.owner_id(),
                            serial,
                        },
                        stream,
                        state: State::Reading(Reader::default()),
                        deadline: Instant::now() + RECEIVE_TIMEOUT,
                        request_id: Uuid::nil(),
                        reserved: 0,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let mut incoming = Vec::new();
        // Rotate servicing order so a large incoming frame cannot starve peers.
        if !self.connections.is_empty() {
            let rotation = self.round_robin % self.connections.len();
            self.connections.rotate_left(rotation);
            self.round_robin = self.round_robin.wrapping_add(1);
        }
        let mut available = DATA_BUFFER_BYTES
            .saturating_sub(self.connections.iter().map(|c| c.reserved).sum::<usize>());
        let mut retained = Vec::with_capacity(MAX_CONNECTIONS);
        for mut connection in self.connections.drain(..) {
            if Instant::now() >= connection.deadline {
                continue;
            }
            let result = match &mut connection.state {
                State::Reading(reader) => {
                    let frame = reader.poll(
                        &mut connection.stream,
                        &mut budget,
                        &mut available,
                        connection.deadline,
                    );
                    connection.reserved = reader.length.unwrap_or(0);
                    match frame {
                        Ok(Some(bytes)) => {
                            let request = (|| {
                                let reserved = wire::parsed_reservation(&bytes)?;
                                let additional = reserved.saturating_sub(connection.reserved);
                                if additional > available {
                                    return Err(HostError::limit());
                                }
                                available -= additional;
                                connection.reserved = reserved;
                                serde_json::from_slice::<Request>(&bytes)
                                    .map_err(|_| HostError::invalid())
                            })();
                            match request {
                                Ok(request) => {
                                    connection.request_id = request.request_id;
                                    if request.version != VERSION
                                        || request.request_id.is_nil()
                                        || request.owner_id != self.discovery.owner_id
                                        || request.package_identity
                                            != self.discovery.package_identity
                                        || !namespace::same_secret(
                                            &request.secret,
                                            &self.discovery.secret,
                                        )
                                    {
                                        Err(HostError::new(
                                            "HostAuthentication",
                                            "Local host request authentication failed",
                                        ))
                                    } else {
                                        connection.state = State::Awaiting;
                                        connection.deadline = Instant::now() + RESPONSE_TIMEOUT;
                                        incoming.push(Incoming {
                                            ticket: connection.ticket,
                                            request_id: request.request_id,
                                            payload: request.payload,
                                        });
                                        Ok(false)
                                    }
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Ok(None) => Ok(false),
                        Err(error) => Err(error),
                    }
                }
                State::Awaiting => Ok(false),
                State::Writing(writer) => {
                    writer.poll(&mut connection.stream, &mut budget, connection.deadline)
                }
            };
            match result {
                Ok(true) => {}
                Ok(false) => retained.push(connection),
                Err(error) => {
                    if !matches!(connection.state, State::Writing(_)) {
                        let response = Response {
                            version: VERSION,
                            owner_id: self.discovery.owner_id,
                            package_identity: self.discovery.package_identity,
                            request_id: connection.request_id,
                            result: ResponseResult::Error { error },
                        };
                        if let Ok(writer) = Writer::new(&response) {
                            connection.reserved = writer.length();
                            connection.state = State::Writing(writer);
                            connection.deadline = Instant::now() + WRITE_TIMEOUT;
                            retained.push(connection);
                        }
                    }
                }
            }
        }
        self.connections = retained;
        incoming
    }
    pub fn respond(&mut self, ticket: ConnectionTicket, payload: Value) -> Result<(), HostError> {
        self.reply(ticket, ResponseResult::Ok { payload })
    }
    pub fn reject(&mut self, ticket: ConnectionTicket, error: HostError) -> Result<(), HostError> {
        // Semantic callers may provide their own typed failures. Keep diagnostics
        // bounded; transport-generated errors themselves are always fixed text.
        if error.code.len() > 128 || error.message.len() > 16 * 1024 {
            return Err(HostError::limit());
        }
        self.reply(ticket, ResponseResult::Error { error })
    }
    fn reply(&mut self, ticket: ConnectionTicket, result: ResponseResult) -> Result<(), HostError> {
        if self.owner.is_closed() {
            return Err(HostError::stale());
        }
        let index = self
            .connections
            .iter()
            .position(|connection| connection.ticket == ticket)
            .ok_or_else(HostError::stale)?;
        let connection = &self.connections[index];
        if !matches!(connection.state, State::Awaiting) || Instant::now() >= connection.deadline {
            return Err(HostError::stale());
        }
        let response = Response {
            version: VERSION,
            owner_id: self.owner_id(),
            package_identity: self.discovery.package_identity,
            request_id: connection.request_id,
            result,
        };
        let other: usize = self
            .connections
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, c)| c.reserved)
            .sum();
        // Count before allocating while the incoming request remains charged.
        let available = DATA_BUFFER_BYTES
            .saturating_sub(other + connection.reserved)
            .max(FALLBACK_REPLY_BYTES);
        let writer = Writer::limited(&response, available)?;
        let connection = &mut self.connections[index];
        connection.reserved = writer.length();
        connection.state = State::Writing(writer);
        connection.deadline = Instant::now() + WRITE_TIMEOUT;
        Ok(())
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.namespace.cleanup(Some(&self.discovery.socket));
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
