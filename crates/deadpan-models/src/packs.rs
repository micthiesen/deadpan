//! Approved model packs and their verified local installation.
//!
//! Pack manifests ship inside the application, so code signing covers which
//! weights Deadpan will accept. Installation downloads into a private staging
//! area with resumable range requests, checks free space first, and verifies
//! every file's exact size and SHA-256 before the pack can be activated. A host
//! smoke-tests a staged pack before activating it; activation renames one
//! complete version directory into place, so earlier installed versions remain
//! the known-good fallback until explicitly removed. Packs live in a global
//! directory shared by projects, and removing one never touches project media.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::Digest;
use thiserror::Error;

pub const MANIFEST_SCHEMA: u32 = 1;
/// Largest single pack file accepted from a manifest.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 16;
const MAX_TEXT_BYTES: usize = 512;
/// Space kept free beyond the remaining download.
const FREE_SPACE_MARGIN: u64 = 256 * 1024 * 1024;
const RECEIPT: &str = "receipt.json";
const ALLOWED_HOSTS: [&str; 1] = ["huggingface.co"];

/// The packs this build accepts.
pub fn approved_packs() -> Vec<PackManifest> {
    [include_str!("../../../models/packs/whisper-base-en-2.json")]
        .into_iter()
        .map(|text| {
            let manifest: PackManifest =
                serde_json::from_str(text).expect("approved manifest parses");
            manifest.validate().expect("approved manifest validates");
            manifest
        })
        .collect()
}

#[derive(Debug, Error)]
pub enum PackError {
    #[error("model pack manifest is invalid: {0}")]
    Manifest(&'static str),
    #[error("not enough free space: {required} bytes needed, {available} available")]
    Space { required: u64, available: u64 },
    #[error("download failed: {0}")]
    Transport(String),
    #[error("{file} failed verification: {reason}")]
    Verification { file: String, reason: &'static str },
    #[error("model pack installation was cancelled")]
    Cancelled,
    #[error("another installation of this model pack is running")]
    Busy,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Transcribe,
    /// Voice activity detection: one speech probability per analysis hop.
    SpeechActivity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackFile {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackLicense {
    pub id: String,
    pub attribution: String,
    pub url: String,
    pub redistribution: bool,
    pub access: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    pub schema: u32,
    pub pack_id: String,
    pub pack_version: String,
    pub title: String,
    pub model_family: String,
    pub runtime_id: String,
    pub runtime_versions: Vec<String>,
    pub operations: Vec<Operation>,
    pub languages: Vec<String>,
    pub files: Vec<PackFile>,
    pub license: PackLicense,
    pub memory_bytes: u64,
    pub temporary_bytes: u64,
    pub qualification_report: String,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !value.starts_with('.')
}

fn text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_TEXT_BYTES
        && !value.chars().any(char::is_control)
}

fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl PackManifest {
    pub fn validate(&self) -> Result<(), PackError> {
        let fail = |reason| Err(PackError::Manifest(reason));
        if self.schema != MANIFEST_SCHEMA {
            return fail("unsupported manifest schema");
        }
        if !identifier(&self.pack_id) || !identifier(&self.pack_version) {
            return fail("pack identity must be a safe identifier");
        }
        if ![
            &self.title,
            &self.model_family,
            &self.runtime_id,
            &self.qualification_report,
        ]
        .into_iter()
        .all(|value| text(value))
        {
            return fail("pack descriptions must be bounded text");
        }
        if self.runtime_versions.is_empty() || !self.runtime_versions.iter().all(|v| text(v)) {
            return fail("pack needs compatible runtime versions");
        }
        if self.operations.is_empty() {
            return fail("pack supports no operation");
        }
        let license = &self.license;
        if ![
            &license.id,
            &license.attribution,
            &license.url,
            &license.access,
        ]
        .into_iter()
        .all(|value| text(value))
        {
            return fail("license fields must be bounded text");
        }
        if self.files.is_empty() || self.files.len() > MAX_FILES {
            return fail("pack file count outside its bound");
        }
        for (index, file) in self.files.iter().enumerate() {
            if !identifier(&file.name) {
                return fail("pack file names must be safe identifiers");
            }
            if self.files[..index]
                .iter()
                .any(|other| other.name == file.name)
            {
                return fail("duplicate pack file name");
            }
            if !sha256(&file.sha256) {
                return fail("pack file hash must be lowercase SHA-256");
            }
            if file.bytes == 0 || file.bytes > MAX_FILE_BYTES {
                return fail("pack file size outside its bound");
            }
            let Some(rest) = file.url.strip_prefix("https://") else {
                return fail("pack files must download over HTTPS");
            };
            let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
            if !ALLOWED_HOSTS.contains(&host) || file.url.len() > MAX_TEXT_BYTES {
                return fail("pack file host is not approved");
            }
        }
        Ok(())
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }

    /// The recognizer model of a transcription pack: its first file.
    pub fn transcription_file(&self) -> Option<&PackFile> {
        self.operations
            .contains(&Operation::Transcribe)
            .then(|| self.files.first())
            .flatten()
    }

    /// The Silero voice activity model of a pack that detects speech.
    pub fn speech_activity_file(&self) -> Option<&PackFile> {
        self.operations
            .contains(&Operation::SpeechActivity)
            .then(|| {
                self.files
                    .iter()
                    .find(|file| file.name.starts_with("ggml-silero-"))
            })
            .flatten()
    }
}

/// One downloaded response body, starting at `offset` of the whole file.
pub struct Download {
    pub offset: u64,
    pub body: Box<dyn Read + Send>,
}

/// Byte transport for pack files. A server may ignore a range request and
/// answer from offset zero; the installer then restarts that file.
pub trait Transport {
    fn fetch(&self, url: &str, offset: u64) -> Result<Download, PackError>;
}

/// Bytes downloaded and verified so far across the whole pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallProgress {
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

/// A verified pack waiting for the host's smoke test.
#[derive(Debug)]
pub struct StagedPack {
    manifest: PackManifest,
    directory: PathBuf,
    /// Exclusive install lock, held until activation or discard.
    _lock: File,
}

impl StagedPack {
    pub fn file(&self, name: &str) -> Option<PathBuf> {
        self.manifest
            .files
            .iter()
            .any(|file| file.name == name)
            .then(|| self.directory.join(name))
    }

    pub fn manifest(&self) -> &PackManifest {
        &self.manifest
    }
}

/// An active installed pack whose receipt matches its approved manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPack {
    pub manifest: PackManifest,
    pub directory: PathBuf,
}

impl InstalledPack {
    pub fn file(&self, name: &str) -> Option<PathBuf> {
        self.manifest
            .files
            .iter()
            .any(|file| file.name == name)
            .then(|| self.directory.join(name))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    pack_id: String,
    pack_version: String,
    files: Vec<ReceiptFile>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReceiptFile {
    name: String,
    sha256: String,
    bytes: u64,
}

/// The global pack directory, for example
/// `~/Library/Application Support/Deadpan/Models`.
#[derive(Debug, Clone)]
pub struct PackStore {
    root: PathBuf,
}

impl PackStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn active(&self, manifest: &PackManifest) -> PathBuf {
        self.root
            .join(&manifest.pack_id)
            .join(&manifest.pack_version)
    }

    /// Serialize installers of one pack version across processes. The lock
    /// lives beside staging, so it never moves into an installed pack.
    fn lock(&self, manifest: &PackManifest) -> Result<File, PackError> {
        let directory = self.root.join(".staging");
        std::fs::create_dir_all(&directory)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join(format!(
                "{}-{}.lock",
                manifest.pack_id, manifest.pack_version
            )))?;
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(file),
            Err(rustix::io::Errno::WOULDBLOCK) => Err(PackError::Busy),
            Err(error) => Err(io::Error::from(error).into()),
        }
    }

    fn staging(&self, manifest: &PackManifest) -> PathBuf {
        self.root
            .join(".staging")
            .join(format!("{}-{}", manifest.pack_id, manifest.pack_version))
    }

    /// The installed pack if its receipt and file sizes match the manifest.
    /// The consuming worker verifies each file's hash again before loading.
    pub fn installed(&self, manifest: &PackManifest) -> Result<Option<InstalledPack>, PackError> {
        manifest.validate()?;
        let directory = self.active(manifest);
        let receipt = match std::fs::read(directory.join(RECEIPT)) {
            Ok(bytes) if bytes.len() <= 64 * 1024 => bytes,
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let Ok(receipt) = serde_json::from_slice::<Receipt>(&receipt) else {
            return Ok(None);
        };
        if receipt.pack_id != manifest.pack_id
            || receipt.pack_version != manifest.pack_version
            || receipt.files != receipt_files(manifest)
        {
            return Ok(None);
        }
        for file in &manifest.files {
            match std::fs::symlink_metadata(directory.join(&file.name)) {
                Ok(metadata) if metadata.is_file() && metadata.len() == file.bytes => {}
                _ => return Ok(None),
            }
        }
        Ok(Some(InstalledPack {
            manifest: manifest.clone(),
            directory,
        }))
    }

    /// Download and verify every file into staging. Partial downloads resume.
    pub fn stage(
        &self,
        manifest: &PackManifest,
        transport: &dyn Transport,
        available_space: impl Fn(&Path) -> io::Result<u64>,
        cancelled: &AtomicBool,
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<StagedPack, PackError> {
        manifest.validate()?;
        let lock = self.lock(manifest)?;
        let staging = self.staging(manifest);
        std::fs::create_dir_all(&staging)?;
        let downloaded: u64 = manifest
            .files
            .iter()
            .map(|file| {
                std::fs::metadata(staging.join(part(&file.name)))
                    .or_else(|_| std::fs::metadata(staging.join(&file.name)))
                    .map_or(0, |metadata| metadata.len().min(file.bytes))
            })
            .sum();
        let required = manifest.total_bytes().saturating_sub(downloaded) + FREE_SPACE_MARGIN;
        let available = available_space(&staging)?;
        if available < required {
            return Err(PackError::Space {
                required,
                available,
            });
        }
        let total = manifest.total_bytes();
        let mut completed = 0;
        for file in &manifest.files {
            let finished = staging.join(&file.name);
            // A finished file may come from an earlier manifest with the same
            // name and size; reuse it only after its hash matches.
            if std::fs::metadata(&finished).is_ok_and(|metadata| metadata.len() == file.bytes)
                && verify_file(file, &finished, cancelled).is_ok()
            {
                completed += file.bytes;
                progress(InstallProgress {
                    completed_bytes: completed,
                    total_bytes: total,
                });
                continue;
            }
            let partial = staging.join(part(&file.name));
            // An installed version of the same pack may already hold this
            // exact file; copy it instead of downloading it again. The copy
            // is verified like a download and discarded when it differs.
            if self.copy_installed(manifest, file, &partial)?
                && verify_file(file, &partial, cancelled).is_ok()
            {
                std::fs::rename(&partial, &finished)?;
                completed += file.bytes;
                progress(InstallProgress {
                    completed_bytes: completed,
                    total_bytes: total,
                });
                continue;
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(PackError::Cancelled);
            }
            download_file(file, &partial, transport, cancelled, |bytes| {
                progress(InstallProgress {
                    completed_bytes: completed + bytes,
                    total_bytes: total,
                });
            })?;
            verify_file(file, &partial, cancelled)?;
            std::fs::rename(&partial, &finished)?;
            completed += file.bytes;
        }
        let receipt = Receipt {
            pack_id: manifest.pack_id.clone(),
            pack_version: manifest.pack_version.clone(),
            files: receipt_files(manifest),
        };
        write_synced(
            &staging.join(RECEIPT),
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        Ok(StagedPack {
            manifest: manifest.clone(),
            directory: staging,
            _lock: lock,
        })
    }

    /// Copy a same-named file of exact size from another installed version of
    /// this pack into `partial`. Returns whether a candidate was copied.
    fn copy_installed(
        &self,
        manifest: &PackManifest,
        file: &PackFile,
        partial: &Path,
    ) -> Result<bool, PackError> {
        let versions = match std::fs::read_dir(self.root.join(&manifest.pack_id)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        for entry in versions {
            let entry = entry?;
            if entry.file_name().to_str() == Some(manifest.pack_version.as_str()) {
                continue;
            }
            let candidate = entry.path().join(&file.name);
            let regular = std::fs::symlink_metadata(&candidate)
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() == file.bytes);
            if !regular {
                continue;
            }
            let _ = std::fs::remove_file(partial);
            // On APFS this clones rather than duplicating the bytes.
            std::fs::copy(&candidate, partial)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Move a smoke-tested staged pack into place as one directory rename.
    pub fn activate(&self, staged: StagedPack) -> Result<InstalledPack, PackError> {
        let destination = self.active(&staged.manifest);
        let parent = destination
            .parent()
            .ok_or(PackError::Manifest("pack directory has no parent"))?;
        std::fs::create_dir_all(parent)?;
        if destination.exists() {
            // Only an incomplete or failed copy of this exact version can be
            // here; `installed` already refused it.
            std::fs::remove_dir_all(&destination)?;
        }
        std::fs::rename(&staged.directory, &destination)?;
        File::open(parent)?.sync_all()?;
        Ok(InstalledPack {
            manifest: staged.manifest,
            directory: destination,
        })
    }

    /// Discard a staged pack that failed its smoke test.
    pub fn discard(&self, staged: StagedPack) -> Result<(), PackError> {
        std::fs::remove_dir_all(staged.directory)?;
        Ok(())
    }

    /// Remove one installed version. Project media never lives here.
    pub fn remove(&self, manifest: &PackManifest) -> Result<(), PackError> {
        manifest.validate()?;
        match std::fs::remove_dir_all(self.active(manifest)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => Ok(result?),
        }
    }
}

fn part(name: &str) -> String {
    format!("{name}.part")
}

fn receipt_files(manifest: &PackManifest) -> Vec<ReceiptFile> {
    manifest
        .files
        .iter()
        .map(|file| ReceiptFile {
            name: file.name.clone(),
            sha256: file.sha256.clone(),
            bytes: file.bytes,
        })
        .collect()
}

fn download_file(
    file: &PackFile,
    partial: &Path,
    transport: &dyn Transport,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(), PackError> {
    let mut output = OpenOptions::new()
        .create(true)
        .read(true)
        .append(false)
        .write(true)
        .truncate(false)
        .open(partial)?;
    let mut offset = output.metadata()?.len();
    if offset > file.bytes {
        output.set_len(0)?;
        offset = 0;
    }
    if offset == file.bytes {
        return Ok(());
    }
    let download = transport.fetch(&file.url, offset)?;
    if download.offset != offset {
        // A full response restarts here; any other offset restarts next time.
        output.set_len(0)?;
        offset = 0;
        if download.offset != 0 {
            return Err(PackError::Transport(
                "server resumed from an unexpected offset; the download will restart".into(),
            ));
        }
    }
    output.seek(SeekFrom::Start(offset))?;
    let chunks = read_in_background(download.body);
    let mut last_data = Instant::now();
    loop {
        if cancelled.load(Ordering::Acquire) {
            output.sync_all()?;
            return Err(PackError::Cancelled);
        }
        let chunk = match chunks.recv_timeout(POLL) {
            Ok(Ok(chunk)) => chunk,
            Ok(Err(error)) => {
                output.sync_all()?;
                return Err(PackError::Transport(error.to_string()));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if last_data.elapsed() >= STALL {
                    output.sync_all()?;
                    return Err(PackError::Transport(
                        "the download stalled; it will resume".into(),
                    ));
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        last_data = Instant::now();
        if offset + chunk.len() as u64 > file.bytes {
            output.set_len(0)?;
            return Err(PackError::Verification {
                file: file.name.clone(),
                reason: "download is longer than its manifest",
            });
        }
        output.write_all(&chunk)?;
        offset += chunk.len() as u64;
        progress(offset);
    }
    output.sync_all()?;
    if offset != file.bytes {
        return Err(PackError::Transport(format!(
            "download ended after {offset} of {} bytes; it will resume",
            file.bytes
        )));
    }
    Ok(())
}

/// How often a download checks for cancellation while waiting for data.
pub const POLL: Duration = Duration::from_millis(200);
/// A body that delivers nothing for this long is abandoned and resumed later.
pub const STALL: Duration = Duration::from_secs(60);

/// Read a response body on its own thread, so cancellation and stall
/// detection never wait on a blocked socket read. An abandoned reader ends
/// when its read returns and the receiver is gone.
pub fn read_in_background(mut body: Box<dyn Read + Send>) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (sender, receiver) = mpsc::sync_channel(4);
    std::thread::spawn(move || {
        loop {
            let mut buffer = vec![0_u8; 1 << 20];
            let result = match body.read(&mut buffer) {
                Ok(0) => return,
                Ok(read) => {
                    buffer.truncate(read);
                    Ok(buffer)
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => Err(error),
            };
            let failed = result.is_err();
            if sender.send(result).is_err() || failed {
                return;
            }
        }
    });
    receiver
}

fn verify_file(file: &PackFile, path: &Path, cancelled: &AtomicBool) -> Result<(), PackError> {
    let mut input = File::open(path)?;
    if input.metadata()?.len() != file.bytes {
        return Err(PackError::Verification {
            file: file.name.clone(),
            reason: "size differs from its manifest",
        });
    }
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(PackError::Cancelled);
        }
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != file.sha256 {
        // A corrupt download cannot be resumed into a valid file.
        std::fs::remove_file(path)?;
        return Err(PackError::Verification {
            file: file.name.clone(),
            reason: "SHA-256 differs from its manifest",
        });
    }
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), PackError> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Bytes available to an unprivileged writer on the volume holding `path`.
pub fn available_space(path: &Path) -> io::Result<u64> {
    let stats = rustix::fs::statvfs(path)?;
    Ok(stats.f_bavail.saturating_mul(stats.f_frsize))
}

/// HTTPS pack downloads with range resume. Redirects stay on HTTPS; the
/// manifest's host allowlist covers the first request, and every byte is
/// checked against the manifest hash regardless of where it came from.
pub struct HttpsTransport {
    agent: ureq::Agent,
}

impl Default for HttpsTransport {
    fn default() -> Self {
        Self::with_user_agent(concat!("Deadpan/", env!("CARGO_PKG_VERSION")))
    }
}

impl HttpsTransport {
    /// The same HTTPS-only, redirect-bounded transport with another user agent.
    pub fn with_user_agent(user_agent: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(5)
            .user_agent(user_agent)
            .timeout_connect(Some(std::time::Duration::from_secs(20)))
            .timeout_recv_response(Some(std::time::Duration::from_secs(60)))
            .build()
            .into();
        Self { agent }
    }
}

impl Transport for HttpsTransport {
    fn fetch(&self, url: &str, offset: u64) -> Result<Download, PackError> {
        let mut request = self.agent.get(url);
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let response = request
            .call()
            .map_err(|error| PackError::Transport(error.to_string()))?;
        let start = match response.status().as_u16() {
            200 => 0,
            206 => response
                .headers()
                .get("content-range")
                .and_then(|value| value.to_str().ok())
                .and_then(content_range_start)
                .ok_or_else(|| PackError::Transport("partial response lacks its range".into()))?,
            status => return Err(PackError::Transport(format!("HTTP status {status}"))),
        };
        Ok(Download {
            offset: start,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}

/// The first byte of `bytes START-END/TOTAL`.
fn content_range_start(value: &str) -> Option<u64> {
    value
        .strip_prefix("bytes ")?
        .split(['-', '/'])
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests;
