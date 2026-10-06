//! Interrupted model-pack downloads over the production HTTPS transport.
//!
//! A local rustls server serves a deterministic file and abruptly closes
//! connections mid-body. The only test-specific pieces are the trusted test
//! CA and an adapter that sends `https://huggingface.co/...` to that server;
//! TLS, ureq, range requests, `Content-Range` parsing, `.part` resume and
//! size/SHA-256 verification are the production code.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::packs::{
    Download, HttpsTransport, PackError, PackFile, PackManifest, PackStore, Transport,
    approved_packs,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::Digest;

const CA: &[u8] = include_bytes!("../../tests/fixtures/test-ca.der");
const LEAF: &[u8] = include_bytes!("../../tests/fixtures/test-localhost.der");
const LEAF_KEY: &[u8] = include_bytes!("../../tests/fixtures/test-localhost.key.der");
const MIB: u64 = 1024 * 1024;
const FILE_PATH: &str = "example/resolve/main/model.bin";

/// One HTTP request the server answered.
#[derive(Debug, Clone)]
struct Record {
    path: String,
    /// The `Range: bytes=N-` start, or `None` without a range header.
    range: Option<u64>,
    status: u16,
    /// The `If-Range` validator the client sent.
    if_range: Option<String>,
    /// Body bytes handed to TLS before the connection ended.
    sent: u64,
}

#[derive(Default)]
struct Behavior {
    /// Body bytes each successive connection sends before an abrupt close;
    /// connections after the queue empties send their whole body.
    drops: VecDeque<u64>,
    /// Answer 200 with the whole file regardless of any range.
    ignore_range: bool,
    /// Flip the byte at this absolute file offset.
    corrupt_at: Option<u64>,
    /// The resource's strong `ETag`; an `If-Range` naming another tag gets
    /// the whole file, as HTTP requires.
    etag: Option<String>,
}

struct Shared {
    content: Vec<u8>,
    behavior: Mutex<Behavior>,
    records: Mutex<Vec<Record>>,
}

/// A local HTTPS file server on 127.0.0.1.
struct Server {
    port: u16,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    fn start(content: Vec<u8>) -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = Arc::new(
            rustls::ServerConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(LEAF.to_vec())],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(LEAF_KEY.to_vec())),
                )
                .unwrap(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared = Arc::new(Shared {
            content,
            behavior: Mutex::new(Behavior::default()),
            records: Mutex::new(Vec::new()),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let accept = {
            let shared = Arc::clone(&shared);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let shared = Arc::clone(&shared);
                    let config = Arc::clone(&config);
                    std::thread::spawn(move || {
                        // Client disconnects (cancellation) end the handler.
                        let _ = serve(stream, config, &shared);
                    });
                }
            })
        };
        Self {
            port,
            shared,
            stop,
            accept: Some(accept),
        }
    }

    fn set(&self, change: impl FnOnce(&mut Behavior)) {
        change(&mut self.shared.behavior.lock().unwrap());
    }

    fn records(&self) -> Vec<Record> {
        self.shared.records.lock().unwrap().clone()
    }

    /// A production transport trusting only the test CA, sending pack URLs
    /// to this server.
    fn transport(&self) -> LocalHuggingFace {
        LocalHuggingFace {
            inner: HttpsTransport::with_trusted_roots("deadpan-interrupted-download-test", &[CA]),
            port: self.port,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
    }
}

/// Serves one request, then closes: cleanly after a whole body, abruptly
/// (no TLS close_notify, no remaining bytes) after a drop budget.
fn serve(stream: TcpStream, config: Arc<rustls::ServerConfig>, shared: &Shared) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let connection = rustls::ServerConnection::new(config).map_err(io::Error::other)?;
    let mut tls = rustls::StreamOwned::new(connection, stream);
    let mut head = Vec::new();
    let mut byte = [0; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16 * 1024 || tls.read(&mut byte)? == 0 {
            return Ok(());
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let path = request_line
        .strip_prefix("GET /")
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or_default()
        .to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect();
    let header = |wanted: &str| {
        headers
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|(_, value)| value.clone())
    };
    let range = header("range").and_then(|value| {
        value
            .strip_prefix("bytes=")?
            .strip_suffix('-')?
            .parse::<u64>()
            .ok()
    });
    let if_range = header("if-range");
    let (budget, ignore_range, corrupt_at, etag) = {
        let mut behavior = shared.behavior.lock().unwrap();
        (
            behavior.drops.pop_front(),
            behavior.ignore_range,
            behavior.corrupt_at,
            behavior.etag.clone(),
        )
    };
    let stale = if_range.is_some() && if_range != etag;
    let length = shared.content.len() as u64;
    let start = if ignore_range || stale {
        0
    } else {
        range.unwrap_or(0)
    };
    let status = if start > 0 { 206 } else { 200 };
    let index = {
        let mut records = shared.records.lock().unwrap();
        records.push(Record {
            path: path.clone(),
            range,
            status,
            if_range,
            sent: 0,
        });
        records.len() - 1
    };
    if path != FILE_PATH || start >= length {
        write!(
            tls,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )?;
        tls.flush()?;
        return Ok(());
    }
    let mut response = format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n",
        if status == 206 {
            "Partial Content"
        } else {
            "OK"
        },
        length - start
    );
    if let Some(etag) = &etag {
        response.push_str(&format!("ETag: {etag}\r\n"));
    }
    if status == 206 {
        response.push_str(&format!(
            "Content-Range: bytes {start}-{}/{length}\r\n",
            length - 1
        ));
    }
    response.push_str("\r\n");
    tls.write_all(response.as_bytes())?;
    let end = budget.map_or(length, |budget| (start + budget).min(length));
    let mut position = start;
    while position < end {
        let next = (position + 64 * 1024).min(end);
        let mut chunk = shared.content[position as usize..next as usize].to_vec();
        if let Some(at) = corrupt_at.filter(|at| (position..next).contains(at)) {
            chunk[(at - position) as usize] ^= 0xff;
        }
        // Count before writing so a client that finishes on these bytes
        // already sees them recorded.
        shared.records.lock().unwrap()[index].sent += next - position;
        tls.write_all(&chunk)?;
        position = next;
    }
    tls.flush()?;
    if end < length {
        // Abrupt drop mid-body: FIN after the delivered records, no close_notify.
        tls.sock.shutdown(Shutdown::Both)?;
    } else {
        tls.conn.send_close_notify();
        tls.flush()?;
    }
    Ok(())
}

/// The real HTTPS transport with Hugging Face URLs sent to the local server.
struct LocalHuggingFace {
    inner: HttpsTransport,
    port: u16,
}

impl Transport for LocalHuggingFace {
    fn fetch(&self, url: &str, offset: u64) -> Result<Download, PackError> {
        let path = url
            .strip_prefix("https://huggingface.co/")
            .expect("pack URLs are on huggingface.co");
        self.inner
            .fetch(&format!("https://127.0.0.1:{}/{path}", self.port), offset)
    }

    fn fetch_resumable(
        &self,
        url: &str,
        offset: u64,
        validator: Option<&str>,
    ) -> Result<(Download, Option<String>), PackError> {
        let path = url
            .strip_prefix("https://huggingface.co/")
            .expect("pack URLs are on huggingface.co");
        self.inner.fetch_resumable(
            &format!("https://127.0.0.1:{}/{path}", self.port),
            offset,
            validator,
        )
    }
}

/// Deterministic pseudo-random bytes (xorshift64*).
fn content(bytes: u64) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ bytes;
    let mut output = Vec::with_capacity(bytes as usize);
    while (output.len() as u64) < bytes {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        output.extend_from_slice(&state.wrapping_mul(0x2545_f491_4f6c_dd1d).to_le_bytes());
    }
    output.truncate(bytes as usize);
    output
}

fn manifest(content: &[u8]) -> PackManifest {
    let mut manifest = approved_packs().remove(0);
    manifest.pack_id = "interrupted-test".into();
    manifest.files = vec![PackFile {
        license: None,
        name: "model.bin".into(),
        url: format!("https://huggingface.co/{FILE_PATH}"),
        sha256: sha2::Sha256::digest(content)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        bytes: content.len() as u64,
    }];
    manifest
}

fn partial(store: &PackStore, manifest: &PackManifest) -> PathBuf {
    store
        .root()
        .join(".staging")
        .join(format!("{}-{}", manifest.pack_id, manifest.pack_version))
        .join("model.bin.part")
}

fn part_len(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |metadata| metadata.len())
}

fn plenty(_: &Path) -> io::Result<u64> {
    Ok(u64::MAX)
}

fn stage(
    store: &PackStore,
    manifest: &PackManifest,
    transport: &dyn Transport,
    cancelled: &AtomicBool,
    progress: impl FnMut(crate::packs::InstallProgress),
) -> Result<crate::packs::StagedPack, PackError> {
    store.stage(manifest, &[], transport, plenty, cancelled, progress)
}

fn assert_staged(staged: &crate::packs::StagedPack, content: &[u8]) {
    let file = staged.file("model.bin").unwrap();
    let staged_bytes = std::fs::read(file).unwrap();
    assert!(
        staged_bytes == content,
        "staged bytes differ from the served file"
    );
}

#[test]
fn dropped_connections_resume_from_the_partial_file_until_verified() {
    let started = Instant::now();
    let length = 48 * MIB;
    let file = content(length);
    let server = Server::start(file.clone());
    // Each connection drops after 5 to 9 MiB, so the file needs several.
    let mut drops = VecDeque::new();
    let mut covered = 0;
    while covered < length {
        let budget = 5 * MIB + (drops.len() as u64 * 1_437_000) % (4 * MIB);
        drops.push_back(budget);
        covered += budget;
    }
    let expected_drops = drops.len() - 1;
    server.set(|behavior| behavior.drops = drops);
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let manifest = manifest(&file);
    let transport = server.transport();
    let part = partial(&store, &manifest);
    let cancelled = AtomicBool::new(false);
    let mut before_attempts = Vec::new();
    let mut reasons = Vec::new();
    let staged = loop {
        assert!(before_attempts.len() < 40, "the download never completed");
        before_attempts.push(part_len(&part));
        match stage(&store, &manifest, &transport, &cancelled, |_| {}) {
            Ok(staged) => break staged,
            // ureq reports the mid-body close ("Peer disconnected").
            Err(PackError::Transport(reason)) => reasons.push(reason),
            Err(error) => panic!("unexpected failure: {error}"),
        }
    };
    assert_staged(&staged, &file);
    assert!(!part.exists());
    let records = server.records();
    assert_eq!(records.len(), before_attempts.len());
    assert_eq!(records.len(), expected_drops + 1, "{records:?}");
    // The first request has no range; every later one starts exactly at the
    // bytes already staged, which strictly increase.
    assert_eq!(records[0].range, None);
    assert_eq!(records[0].status, 200);
    for (record, staged_before) in records.iter().zip(&before_attempts).skip(1) {
        assert_eq!(record.path, FILE_PATH);
        assert_eq!(record.status, 206);
        assert_eq!(record.range, Some(*staged_before), "{records:?}");
    }
    assert!(before_attempts.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(
        records.iter().map(|record| record.sent).sum::<u64>(),
        length,
        "every byte was served exactly once: {records:?}"
    );
    eprintln!(
        "{} connections for {length} bytes in {:?}; failures: {reasons:?}",
        records.len(),
        started.elapsed()
    );
}

#[test]
fn cancellation_keeps_partial_bytes_and_the_next_attempt_resumes() {
    let length = 24 * MIB;
    let file = content(length);
    let server = Server::start(file.clone());
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let manifest = manifest(&file);
    let transport = server.transport();
    let part = partial(&store, &manifest);
    let cancelled = AtomicBool::new(false);
    let result = stage(&store, &manifest, &transport, &cancelled, |progress| {
        if progress.completed_bytes >= 8 * MIB {
            cancelled.store(true, Ordering::Release);
        }
    });
    assert!(matches!(result, Err(PackError::Cancelled)), "{result:?}");
    let kept = part_len(&part);
    assert!((8 * MIB..length).contains(&kept), "kept {kept} bytes");
    assert!(file[..kept as usize] == std::fs::read(&part).unwrap()[..]);
    cancelled.store(false, Ordering::Release);
    let staged = stage(&store, &manifest, &transport, &cancelled, |_| {}).unwrap();
    assert_staged(&staged, &file);
    let records = server.records();
    assert_eq!(records.len(), 2, "{records:?}");
    assert_eq!(records[0].range, None);
    assert_eq!(records[1].range, Some(kept));
    assert_eq!(records[1].status, 206);
    assert_eq!(records[1].sent, length - kept);
}

#[test]
fn a_server_ignoring_the_range_restarts_the_file_and_still_verifies() {
    let length = 16 * MIB;
    let file = content(length);
    let server = Server::start(file.clone());
    server.set(|behavior| behavior.drops = VecDeque::from([6 * MIB]));
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let manifest = manifest(&file);
    let transport = server.transport();
    let part = partial(&store, &manifest);
    let cancelled = AtomicBool::new(false);
    let first = stage(&store, &manifest, &transport, &cancelled, |_| {});
    assert!(matches!(first, Err(PackError::Transport(_))), "{first:?}");
    assert_eq!(part_len(&part), 6 * MIB);
    server.set(|behavior| behavior.ignore_range = true);
    let staged = stage(&store, &manifest, &transport, &cancelled, |_| {}).unwrap();
    assert_staged(&staged, &file);
    let records = server.records();
    assert_eq!(records.len(), 2, "{records:?}");
    assert_eq!(records[1].range, Some(6 * MIB));
    assert_eq!(records[1].status, 200);
    assert_eq!(records[1].sent, length);
}

#[test]
fn a_corrupt_resumed_byte_fails_verification_and_discards_the_part() {
    let length = 16 * MIB;
    let file = content(length);
    let server = Server::start(file.clone());
    server.set(|behavior| behavior.drops = VecDeque::from([6 * MIB]));
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let manifest = manifest(&file);
    let transport = server.transport();
    let part = partial(&store, &manifest);
    let cancelled = AtomicBool::new(false);
    let first = stage(&store, &manifest, &transport, &cancelled, |_| {});
    assert!(matches!(first, Err(PackError::Transport(_))), "{first:?}");
    server.set(|behavior| behavior.corrupt_at = Some(10 * MIB + 17));
    let second = stage(&store, &manifest, &transport, &cancelled, |_| {});
    assert!(
        matches!(second, Err(PackError::Verification { .. })),
        "{second:?}"
    );
    assert!(!part.exists(), "a mismatched part cannot be resumed");
    server.set(|behavior| behavior.corrupt_at = None);
    let staged = stage(&store, &manifest, &transport, &cancelled, |_| {}).unwrap();
    assert_staged(&staged, &file);
    let ranges: Vec<_> = server.records().iter().map(|record| record.range).collect();
    assert_eq!(ranges, [None, Some(6 * MIB), None]);
}

#[test]
fn the_production_trust_store_rejects_the_test_ca_and_plain_http() {
    let server = Server::start(content(MIB));
    let url = format!("https://127.0.0.1:{}/{FILE_PATH}", server.port);
    let system = HttpsTransport::default();
    match system.fetch(&url, 0) {
        Err(PackError::Transport(reason)) => assert!(
            reason.to_lowercase().contains("certificate"),
            "expected a certificate rejection: {reason}"
        ),
        Err(error) => panic!("unexpected failure: {error}"),
        Ok(_) => panic!("the system trust store accepted the test CA"),
    }
    let trusted = HttpsTransport::with_trusted_roots("deadpan-interrupted-download-test", &[CA]);
    let mut body = Vec::new();
    trusted
        .fetch(&url, 0)
        .unwrap()
        .body
        .read_to_end(&mut body)
        .unwrap();
    assert_eq!(body.len() as u64, MIB);
    let plain = format!("http://127.0.0.1:{}/{FILE_PATH}", server.port);
    assert!(matches!(
        trusted.fetch(&plain, 0),
        Err(PackError::Transport(_))
    ));
    // Only the trusted fetch completed a TLS handshake and request.
    assert_eq!(server.records().len(), 1);
}

/// Network check of the production system trust store against Hugging Face.
#[test]
#[ignore = "requires network access to huggingface.co"]
fn the_system_trust_store_resumes_from_hugging_face() {
    let pack = approved_packs().remove(0);
    let file = pack.speech_activity_file().unwrap();
    let offset = file.bytes - 1000;
    let download = HttpsTransport::default().fetch(&file.url, offset).unwrap();
    assert_eq!(download.offset, offset);
    let mut body = Vec::new();
    download.body.take(2000).read_to_end(&mut body).unwrap();
    assert_eq!(body.len(), 1000);
}

#[test]
fn resumes_only_while_the_entity_tag_matches() {
    let content = content(24 * MIB);
    let server = Server::start(content.clone());
    server.set(|behavior| {
        behavior.etag = Some("\"v1\"".into());
        behavior.drops = VecDeque::from([6 * MIB, 5 * MIB]);
    });
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_owned());
    let manifest = manifest(&content);
    let transport = server.transport();
    let cancelled = AtomicBool::new(false);
    // First drop: the next attempt resumes with If-Range "v1" and gets 206.
    assert!(stage(&store, &manifest, &transport, &cancelled, |_| {}).is_err());
    let kept = part_len(&partial(&store, &manifest));
    assert!(kept >= 6 * MIB);
    assert!(stage(&store, &manifest, &transport, &cancelled, |_| {}).is_err());
    let records = server.records();
    assert_eq!(records[1].range, Some(kept));
    assert_eq!(records[1].if_range.as_deref(), Some("\"v1\""));
    assert_eq!(records[1].status, 206);
    // The resource changes (same bytes, new tag): the resume request is
    // answered from zero and the file restarts instead of splicing.
    server.set(|behavior| behavior.etag = Some("\"v2\"".into()));
    let staged = stage(&store, &manifest, &transport, &cancelled, |_| {}).unwrap();
    let records = server.records();
    let last = records.last().unwrap();
    assert!(last.range.is_some_and(|range| range > 0));
    assert_eq!(last.if_range.as_deref(), Some("\"v1\""));
    assert_eq!(last.status, 200);
    assert_eq!(last.sent, content.len() as u64);
    assert_staged(&staged, &content);
    // The validator is not left in the staged pack.
    assert!(
        std::fs::read_dir(staged.directory())
            .unwrap()
            .flatten()
            .all(|entry| !entry.file_name().to_string_lossy().ends_with(".validator"))
    );
}
