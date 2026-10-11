//! Downloader connections are confined to an owned authenticated CONNECT
//! gateway. The macOS child sandbox permits only that gateway's loopback port;
//! every tunnel independently admits a supported hostname and public IP.

use std::ffi::OsString;
use std::io::{self, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::ImportError;

pub(super) mod dns;
pub(super) mod ip;
mod protocol;
mod tunnel;

const MAX_CONNECTIONS: usize = 16;
const SANDBOX: &str = "/usr/bin/sandbox-exec";

/// Helper probes need no network; acquisition needs only YouTube services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Offline,
    YouTube,
}

pub(super) struct Gateway {
    port: u16,
    credentials: Arc<protocol::Credentials>,
    stopped: Arc<AtomicBool>,
    restricted: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Gateway {
    pub fn start() -> Result<Self, ImportError> {
        let listeners = loopback_listeners().map_err(unavailable)?;
        let port = listeners[0].local_addr().map_err(unavailable)?.port();
        let credentials = Arc::new(protocol::Credentials::new());
        let stopped = Arc::new(AtomicBool::new(false));
        let restricted = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let denied = Arc::clone(&restricted);
        let auth = Arc::clone(&credentials);
        let thread = std::thread::Builder::new()
            .name("deadpan-youtube-network".into())
            .spawn(move || accept(listeners, stop, denied, auth))
            .map_err(unavailable)?;
        Ok(Self {
            port,
            credentials,
            stopped,
            restricted,
            thread: Some(thread),
        })
    }

    pub fn proxy_url(&self) -> OsString {
        format!(
            "http://deadpan:{}@127.0.0.1:{}",
            self.credentials.password, self.port
        )
        .into()
    }

    pub fn failure(&self) -> Option<ImportError> {
        self.restricted.load(Ordering::Acquire).then(|| ImportError::new(
            "YouTubeNetworkRestricted",
            "the downloader requested a destination outside supported public YouTube services; update Deadpan before retrying this import",
        ))
    }

    pub fn finish(&mut self) -> Option<ImportError> {
        self.stopped.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.failure()
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn unavailable(error: io::Error) -> ImportError {
    ImportError::new(
        "DownloaderNetworkUnavailable",
        format!("could not prepare the downloader network boundary: {error}"),
    )
}

/// There is no unrestricted fallback if the host cannot apply this boundary.
pub(super) fn launch(
    executable: &Path,
    arguments: &[OsString],
    gateway: Option<&Gateway>,
) -> Result<(PathBuf, Vec<OsString>), ImportError> {
    if !cfg!(target_os = "macos") {
        return Err(ImportError::new(
            "DownloaderUnsupportedPlatform",
            "isolated YouTube acquisition requires macOS",
        ));
    }
    if !executable.is_absolute() || !executable.is_file() || !Path::new(SANDBOX).is_file() {
        return Err(ImportError::new(
            "DownloaderNetworkUnavailable",
            "the verified helper or macOS network-isolation launcher is unavailable",
        ));
    }
    let mut profile = "(version 1)(allow default)(deny network*)".to_owned();
    if let Some(gateway) = gateway {
        profile.push_str(&format!(
            "(allow network-outbound (remote tcp \"localhost:{}\"))",
            gateway.port
        ));
    }
    let mut wrapped = vec![
        "-p".into(),
        profile.into(),
        executable.as_os_str().to_owned(),
    ];
    if let Some(gateway) = gateway {
        wrapped.extend(["--proxy".into(), gateway.proxy_url()]);
    }
    wrapped.extend_from_slice(arguments);
    Ok((PathBuf::from(SANDBOX), wrapped))
}

// Seatbelt's localhost filter includes both loopback families and does not
// accept a numeric address. Own both endpoints before granting the exception;
// otherwise another process could own ::1 at the allowed IPv4 proxy port.
fn loopback_listeners() -> io::Result<[TcpListener; 2]> {
    for _ in 0..16 {
        let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = v4.local_addr()?.port();
        let v6 = match TcpListener::bind((Ipv6Addr::LOCALHOST, port)) {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
            Err(error) => return Err(error),
        };
        v4.set_nonblocking(true)?;
        v6.set_nonblocking(true)?;
        return Ok([v4, v6]);
    }
    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        "could not reserve both loopback endpoints",
    ))
}

fn accept(
    listeners: [TcpListener; 2],
    stopped: Arc<AtomicBool>,
    restricted: Arc<AtomicBool>,
    credentials: Arc<protocol::Credentials>,
) {
    let mut connections: Vec<JoinHandle<()>> = Vec::with_capacity(MAX_CONNECTIONS);
    'accept: while !stopped.load(Ordering::Acquire) {
        let mut index = 0;
        while index < connections.len() {
            if connections[index].is_finished() {
                let _ = connections.swap_remove(index).join();
            } else {
                index += 1;
            }
        }
        let mut accepted = false;
        for listener in &listeners {
            match listener.accept() {
                Ok((stream, _)) if connections.len() < MAX_CONNECTIONS => {
                    accepted = true;
                    let stop = Arc::clone(&stopped);
                    let denied = Arc::clone(&restricted);
                    let auth = Arc::clone(&credentials);
                    if let Ok(thread) = std::thread::Builder::new()
                        .name("deadpan-youtube-tunnel".into())
                        .spawn(move || serve(stream, &stop, &denied, &auth))
                    {
                        connections.push(thread);
                    }
                }
                Ok(_) => {
                    accepted = true;
                } // At capacity, close without allocating another worker.
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break 'accept,
            }
        }
        if !accepted {
            std::thread::park_timeout(Duration::from_millis(20));
        }
    }
    stopped.store(true, Ordering::Release);
    for thread in connections {
        let _ = thread.join();
    }
}

fn serve(
    client: TcpStream,
    stopped: &AtomicBool,
    restricted: &AtomicBool,
    credentials: &protocol::Credentials,
) {
    serve_with_resolver(client, stopped, restricted, credentials, |host| {
        dns::resolve(host, Duration::from_secs(5))
    });
}

fn serve_with_resolver(
    mut client: TcpStream,
    stopped: &AtomicBool,
    restricted: &AtomicBool,
    credentials: &protocol::Credentials,
    resolve: impl FnOnce(&str) -> Result<Vec<SocketAddr>, dns::LookupError>,
) {
    let Ok(header) = tunnel::header(&mut client, stopped) else {
        return;
    };
    let host = match protocol::destination(&header, credentials) {
        Ok(host) => host,
        Err(reason) => {
            let response = match reason {
                protocol::Refusal::Unauthenticated => b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_slice(),
                protocol::Refusal::Restricted => {
                    restricted.store(true, Ordering::Release);
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_slice()
                }
            };
            let _ = client.write_all(response);
            return;
        }
    };
    if stopped.load(Ordering::Acquire) {
        return;
    }
    let Ok(addresses) = resolve(&host) else {
        return;
    };
    if addresses.is_empty() || stopped.load(Ordering::Acquire) {
        return;
    }
    let addresses: Vec<_> = addresses
        .into_iter()
        .take(16)
        .filter(|address| address.port() == 443 && ip::public_address(address.ip()))
        .collect();
    if addresses.is_empty() {
        restricted.store(true, Ordering::Release);
        let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    for address in addresses {
        if stopped.load(Ordering::Acquire) || Instant::now() >= deadline {
            return;
        }
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(750));
        if timeout.is_zero() {
            return;
        }
        if let Ok(server) = TcpStream::connect_timeout(&address, timeout) {
            if client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .is_ok()
            {
                let _ = tunnel::relay(client, server, stopped);
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests;
