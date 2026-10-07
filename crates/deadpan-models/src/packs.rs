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
//!
//! A pack may also be imported offline from a folder or an uncompressed tar
//! archive that holds its files; every imported byte is verified exactly like a
//! download. Licenses that require acceptance must be accepted by the caller
//! before any byte is staged, and the receipt records that acceptance.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::Digest;
use thiserror::Error;

pub mod archive;
pub mod constraints;
pub mod updates;

pub const MANIFEST_SCHEMA: u32 = 4;
/// Largest single pack file accepted from a manifest.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 64;
const MAX_LICENSES: usize = 8;
const MAX_TEXT_BYTES: usize = 512;
const MAX_TERMS_BYTES: usize = 2048;
/// Deepest relative file path a pack may name, in components.
const MAX_PATH_COMPONENTS: usize = 4;
/// The HTTP identity of pack downloads.
pub const USER_AGENT: &str = "OpenAI File Downloader, XaiImageApiFetch/1.0";
/// Space kept free beyond the remaining download.
pub const FREE_SPACE_MARGIN: u64 = 256 * 1024 * 1024;
const RECEIPT: &str = "receipt.json";
const ALLOWED_HOSTS: [&str; 1] = ["huggingface.co"];

/// Full license texts the approved manifests name, compiled into the build.
const LICENSE_TEXTS: [(&str, &str); 2] = [
    (
        "ltx-2-community-license.txt",
        include_str!("../../../models/licenses/ltx-2-community-license.txt"),
    ),
    (
        "gemma-terms-of-use.txt",
        include_str!("../../../models/licenses/gemma-terms-of-use.txt"),
    ),
];

/// The full text of a license a manifest names in `text`.
pub fn license_text(name: &str) -> Option<&'static str> {
    LICENSE_TEXTS
        .iter()
        .find(|(file, _)| *file == name)
        .map(|(_, text)| *text)
}

/// The approved pack with this identifier, any version.
pub fn approved_pack(pack_id: &str) -> Option<PackManifest> {
    approved_packs()
        .into_iter()
        .find(|pack| pack.pack_id == pack_id)
}

/// The packs this build accepts.
pub fn approved_packs() -> Vec<PackManifest> {
    [
        include_str!("../../../models/packs/whisper-base-en-2.json"),
        include_str!("../../../models/packs/ltx-2.3-q4-bridge-1.json"),
    ]
    .into_iter()
    .map(|text| {
        let manifest: PackManifest = serde_json::from_str(text).expect("approved manifest parses");
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
    #[error("the {title} must be accepted before installing this pack")]
    LicenseNotAccepted { title: String },
    #[error("the offline source lacks {missing} of this pack's files, for example {example}")]
    ImportIncomplete { missing: usize, example: String },
    /// A signed pack update or the active-version selection refused, with a
    /// stable code (`UpdateUntrusted`, `UpdateDowngrade`, …).
    #[error("{message}")]
    Update { code: &'static str, message: String },
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
    /// Generated pictures between two boundary frames (an AI pause).
    BridgeHold,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackFile {
    /// A relative path of safe components, such as `model.bin` or
    /// `text_encoder/<revision>/model.safetensors`.
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    /// The `PackLicense::id` covering this file; required when a pack has
    /// more than one license.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// One license layer of a pack: model weights, a text encoder or tokenizer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackLicense {
    /// Short identifier, unique within the pack.
    pub id: String,
    pub title: String,
    /// SPDX identifier or `LicenseRef-…`.
    pub spdx: String,
    pub attribution: String,
    pub url: String,
    /// The compiled full text (see [`license_text`]), when Deadpan carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A plain summary of the terms that matter to a user, shown before
    /// installation. It never replaces the full text.
    pub terms: String,
    pub redistribution: bool,
    pub access: String,
    /// The user must explicitly accept this license before installation.
    pub acceptance_required: bool,
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
    pub constraints: constraints::PackConstraints,
    pub files: Vec<PackFile>,
    pub licenses: Vec<PackLicense>,
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

/// A relative path of safe identifier components.
fn relative_path(value: &str) -> bool {
    let components: Vec<&str> = value.split('/').collect();
    value.len() <= 256
        && components.len() <= MAX_PATH_COMPONENTS
        && components.iter().all(|component| identifier(component))
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
        self.constraints.validate(&self.operations)?;
        if self.licenses.is_empty() || self.licenses.len() > MAX_LICENSES {
            return fail("pack license count outside its bound");
        }
        for (index, license) in self.licenses.iter().enumerate() {
            if !identifier(&license.id)
                || self.licenses[..index]
                    .iter()
                    .any(|other| other.id == license.id)
            {
                return fail("license identifiers must be unique safe identifiers");
            }
            if ![
                &license.title,
                &license.spdx,
                &license.attribution,
                &license.url,
                &license.access,
            ]
            .into_iter()
            .all(|value| text(value))
                || license.terms.trim().is_empty()
                || license.terms.len() > MAX_TERMS_BYTES
                || license.terms.chars().any(|c| c.is_control() && c != '\n')
            {
                return fail("license fields must be bounded text");
            }
            if !license.url.starts_with("https://") {
                return fail("license links must use HTTPS");
            }
            if license
                .text
                .as_deref()
                .is_some_and(|name| license_text(name).is_none())
            {
                return fail("license text is not compiled into this build");
            }
        }
        if self.files.is_empty() || self.files.len() > MAX_FILES {
            return fail("pack file count outside its bound");
        }
        for (index, file) in self.files.iter().enumerate() {
            if !relative_path(&file.name) {
                return fail("pack file names must be safe relative paths");
            }
            // Staging writes `<name>.part` and the receipt beside the files;
            // a file may not also be another file's directory.
            if file.name == RECEIPT
                || file.name.ends_with(".part")
                || self.files.iter().any(|other| {
                    other.name.len() > file.name.len()
                        && other.name.starts_with(&file.name)
                        && other.name.as_bytes()[file.name.len()] == b'/'
                })
            {
                return fail("pack file names collide with staging or other files");
            }
            match &file.license {
                Some(id) if !self.licenses.iter().any(|license| &license.id == id) => {
                    return fail("pack file names an unknown license");
                }
                None if self.licenses.len() > 1 => {
                    return fail("a pack with several licenses must assign each file one");
                }
                _ => {}
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

    /// Bytes of the files one license covers.
    pub fn license_bytes(&self, license: &PackLicense) -> u64 {
        self.files
            .iter()
            .filter(|file| {
                file.license.as_deref().unwrap_or(&self.licenses[0].id) == license.id.as_str()
            })
            .map(|file| file.bytes)
            .sum()
    }

    /// Whether any license must be accepted before installation.
    pub fn acceptance_required(&self) -> bool {
        self.licenses
            .iter()
            .any(|license| license.acceptance_required)
    }

    /// Refuse when a license requiring acceptance is not in `accepted`.
    pub fn check_acceptance(&self, accepted: &[String]) -> Result<(), PackError> {
        match self
            .licenses
            .iter()
            .find(|license| license.acceptance_required && !accepted.contains(&license.id))
        {
            Some(license) => Err(PackError::LicenseNotAccepted {
                title: license.title.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Every license identifier, for a caller that has shown and accepted all.
    pub fn license_ids(&self) -> Vec<String> {
        self.licenses
            .iter()
            .map(|license| license.id.clone())
            .collect()
    }

    pub fn supports(&self, operation: Operation) -> bool {
        self.operations.contains(&operation)
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

    /// [`Self::fetch`] that resumes only while the resource still matches
    /// `validator` (HTTP `If-Range` with the strong `ETag` or `Last-Modified`
    /// of the response that started the file), and returns the response's own
    /// validator. A changed resource answers from offset zero, so a partial
    /// file is never spliced from two versions. Transports without validators
    /// ignore it.
    fn fetch_resumable(
        &self,
        url: &str,
        offset: u64,
        validator: Option<&str>,
    ) -> Result<(Download, Option<String>), PackError> {
        let _ = validator;
        Ok((self.fetch(url, offset)?, None))
    }
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

    /// The verified staging directory, laid out like the installed pack.
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

/// An active installed pack whose receipt matches its approved manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPack {
    pub manifest: PackManifest,
    pub directory: PathBuf,
}

/// An installed version whose complete bytes were checked for this attempt.
/// Retain this guard through the runtime smoke test and pointer activation.
#[derive(Debug)]
pub struct VerifiedInstalledPack {
    installed: InstalledPack,
    _lock: File,
}

impl VerifiedInstalledPack {
    pub fn installed(&self) -> &InstalledPack {
        &self.installed
    }
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
    /// License identifiers the user accepted before installation.
    #[serde(default)]
    accepted_licenses: Vec<String>,
    /// `download` or `import`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<String>,
}

/// Where an offline installation reads a pack's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportSource {
    /// A folder holding each file at its manifest path, either directly or
    /// below `<pack_id>/<pack_version>/` (the layout `export` writes).
    Directory(PathBuf),
    /// An uncompressed (ustar/pax) tar archive with the same layout.
    Archive(PathBuf),
}

impl ImportSource {
    /// A directory, or a file read as a tar archive.
    pub fn at(path: &Path) -> Result<Self, PackError> {
        let metadata = std::fs::metadata(path)?;
        Ok(if metadata.is_dir() {
            Self::Directory(path.to_path_buf())
        } else {
            Self::Archive(path.to_path_buf())
        })
    }
}

/// What the store holds of one pack version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackState {
    Installed(Box<InstalledPack>),
    /// Staged or partially downloaded bytes an install would resume from.
    Partial {
        bytes: u64,
    },
    Absent,
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Installed, partially staged, or absent.
    pub fn state(&self, manifest: &PackManifest) -> Result<PackState, PackError> {
        if let Some(installed) = self.installed(manifest)? {
            return Ok(PackState::Installed(Box::new(installed)));
        }
        let staging = self.staging(manifest);
        let bytes: u64 = manifest
            .files
            .iter()
            .map(|file| {
                std::fs::metadata(staging.join(part(&file.name)))
                    .or_else(|_| std::fs::metadata(staging.join(&file.name)))
                    .map_or(0, |metadata| metadata.len().min(file.bytes))
            })
            .sum();
        Ok(if bytes > 0 {
            PackState::Partial { bytes }
        } else {
            PackState::Absent
        })
    }

    /// Bytes still needed on this volume to finish installing.
    pub fn remaining_bytes(&self, manifest: &PackManifest) -> u64 {
        match self.state(manifest) {
            Ok(PackState::Installed(_)) => 0,
            Ok(PackState::Partial { bytes }) => manifest.total_bytes().saturating_sub(bytes),
            _ => manifest.total_bytes(),
        }
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

    /// Recheck an existing version before selecting it on an update retry.
    /// Failure leaves both its files and the active pointer untouched.
    pub fn verify_installed(
        &self,
        manifest: &PackManifest,
        cancelled: &AtomicBool,
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<Option<VerifiedInstalledPack>, PackError> {
        manifest.validate()?;
        let lock = self.lock(manifest)?;
        let Some(installed) = self.installed(manifest)? else {
            return Ok(None);
        };
        let mut completed = 0;
        for file in &manifest.files {
            verify_bytes(
                file,
                &installed.directory.join(&file.name),
                cancelled,
                |bytes| {
                    progress(InstallProgress {
                        completed_bytes: completed + bytes,
                        total_bytes: manifest.total_bytes(),
                    });
                },
            )?;
            completed += file.bytes;
        }
        Ok(Some(VerifiedInstalledPack {
            installed,
            _lock: lock,
        }))
    }

    /// Download and verify every file into staging. Partial downloads resume.
    pub fn stage(
        &self,
        manifest: &PackManifest,
        accepted_licenses: &[String],
        transport: &dyn Transport,
        available_space: impl Fn(&Path) -> io::Result<u64>,
        cancelled: &AtomicBool,
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<StagedPack, PackError> {
        manifest.validate()?;
        manifest.check_acceptance(accepted_licenses)?;
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
            if let Some(parent) = finished.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // A finished file may come from an earlier manifest with the same
            // name and size; reuse it only after its hash matches.
            if std::fs::metadata(&finished).is_ok_and(|metadata| metadata.len() == file.bytes)
                && verify_file(file, &finished, cancelled, |_| {}).is_ok()
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
                && verify_file(file, &partial, cancelled, |_| {}).is_ok()
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
            verify_file(file, &partial, cancelled, |_| {})?;
            std::fs::rename(&partial, &finished)?;
            completed += file.bytes;
        }
        self.staged(manifest, accepted_licenses, "download", staging, lock)
    }

    /// Write the receipt of a completely verified staging directory.
    fn staged(
        &self,
        manifest: &PackManifest,
        accepted_licenses: &[String],
        origin: &str,
        staging: PathBuf,
        lock: File,
    ) -> Result<StagedPack, PackError> {
        let receipt = Receipt {
            pack_id: manifest.pack_id.clone(),
            pack_version: manifest.pack_version.clone(),
            files: receipt_files(manifest),
            accepted_licenses: accepted_licenses.to_vec(),
            origin: Some(origin.into()),
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

    /// Stage a pack offline from a folder or tar archive. Every file must be
    /// present with its exact size and SHA-256; anything else in the source is
    /// ignored. Folder files are cloned on the same APFS volume.
    pub fn import(
        &self,
        manifest: &PackManifest,
        accepted_licenses: &[String],
        source: &ImportSource,
        available_space: impl Fn(&Path) -> io::Result<u64>,
        cancelled: &AtomicBool,
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<StagedPack, PackError> {
        manifest.validate()?;
        manifest.check_acceptance(accepted_licenses)?;
        let lock = self.lock(manifest)?;
        let staging = self.staging(manifest);
        std::fs::create_dir_all(&staging)?;
        let total = manifest.total_bytes();
        let mut completed = 0;
        match source {
            ImportSource::Directory(directory) => {
                let mut sources = Vec::new();
                let mut missing = Vec::new();
                for file in &manifest.files {
                    match import_candidate(manifest, directory, file) {
                        Some(path) => sources.push((file, path)),
                        None => missing.push(file.name.clone()),
                    }
                }
                if let Some(example) = missing.first() {
                    return Err(PackError::ImportIncomplete {
                        missing: missing.len(),
                        example: example.clone(),
                    });
                }
                // Clones on the staging volume need no space; copies do.
                let staging_device = device(&staging)?;
                let required: u64 = sources
                    .iter()
                    .filter(|(_, path)| device(path).ok() != Some(staging_device))
                    .map(|(file, _)| file.bytes)
                    .sum::<u64>()
                    + FREE_SPACE_MARGIN;
                let available = available_space(&staging)?;
                if available < required {
                    return Err(PackError::Space {
                        required,
                        available,
                    });
                }
                for (file, path) in sources {
                    if cancelled.load(Ordering::Acquire) {
                        return Err(PackError::Cancelled);
                    }
                    let finished = staging.join(&file.name);
                    let partial = staging.join(part(&file.name));
                    if let Some(parent) = finished.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let _ = std::fs::remove_file(&partial);
                    // On APFS this clones rather than duplicating the bytes.
                    std::fs::copy(&path, &partial)?;
                    verify_file(file, &partial, cancelled, |bytes| {
                        progress(InstallProgress {
                            completed_bytes: completed + bytes,
                            total_bytes: total,
                        });
                    })?;
                    std::fs::rename(&partial, &finished)?;
                    completed += file.bytes;
                }
            }
            ImportSource::Archive(path) => {
                let required = total + FREE_SPACE_MARGIN;
                let available = available_space(&staging)?;
                if available < required {
                    return Err(PackError::Space {
                        required,
                        available,
                    });
                }
                let mut found = vec![false; manifest.files.len()];
                archive::read(
                    path,
                    cancelled,
                    |entry| {
                        let Some(index) = manifest.files.iter().position(|file| {
                            entry.path == file.name
                                || entry.path
                                    == format!(
                                        "{}/{}/{}",
                                        manifest.pack_id, manifest.pack_version, file.name
                                    )
                        }) else {
                            return Ok(archive::Action::Skip);
                        };
                        let file = &manifest.files[index];
                        if !entry.regular {
                            return Err(PackError::Verification {
                                file: file.name.clone(),
                                reason: "archive entry is not a regular file",
                            });
                        }
                        if found[index] {
                            return Err(PackError::Verification {
                                file: file.name.clone(),
                                reason: "archive holds this file twice",
                            });
                        }
                        if entry.size != file.bytes {
                            return Err(PackError::Verification {
                                file: file.name.clone(),
                                reason: "size differs from its manifest",
                            });
                        }
                        found[index] = true;
                        let finished = staging.join(&file.name);
                        if let Some(parent) = finished.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        // Never write through whatever an earlier attempt left.
                        let partial = staging.join(part(&file.name));
                        match std::fs::remove_file(&partial) {
                            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                                return Err(error.into());
                            }
                            _ => {}
                        }
                        Ok(archive::Action::Extract(partial))
                    },
                    |extracted| {
                        let file = manifest
                            .files
                            .iter()
                            .find(|file| staging.join(part(&file.name)) == extracted)
                            .ok_or(PackError::Manifest("extracted an unknown file"))?;
                        verify_file(file, extracted, cancelled, |bytes| {
                            progress(InstallProgress {
                                completed_bytes: completed + bytes,
                                total_bytes: total,
                            });
                        })?;
                        std::fs::rename(extracted, staging.join(&file.name))?;
                        completed += file.bytes;
                        Ok(())
                    },
                )?;
                let missing: Vec<&PackFile> = manifest
                    .files
                    .iter()
                    .zip(&found)
                    .filter(|(_, found)| !**found)
                    .map(|(file, _)| file)
                    .collect();
                if let Some(example) = missing.first() {
                    return Err(PackError::ImportIncomplete {
                        missing: missing.len(),
                        example: example.name.clone(),
                    });
                }
            }
        }
        self.staged(manifest, accepted_licenses, "import", staging, lock)
    }

    /// Write one installed pack as an uncompressed tar archive that `import`
    /// accepts on another Mac (an offline pack). The archive is published by
    /// rename only after every file was written.
    pub fn export(
        &self,
        manifest: &PackManifest,
        destination: &Path,
        cancelled: &AtomicBool,
        progress: impl FnMut(InstallProgress),
    ) -> Result<(), PackError> {
        let installed = self.installed(manifest)?.ok_or(PackError::Manifest(
            "the pack is not installed, so it cannot be exported",
        ))?;
        let entries: Vec<(String, PathBuf, u64)> = manifest
            .files
            .iter()
            .map(|file| {
                (
                    format!(
                        "{}/{}/{}",
                        manifest.pack_id, manifest.pack_version, file.name
                    ),
                    installed.directory.join(&file.name),
                    file.bytes,
                )
            })
            .collect();
        archive::write(destination, &entries, cancelled, progress)
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
            if let Some(parent) = partial.parent() {
                std::fs::create_dir_all(parent)?;
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
        Ok(self.activate_guarded(staged)?.installed)
    }

    /// Install a smoke-tested version while retaining its lock through the
    /// caller's separate active-pointer selection.
    pub fn activate_guarded(&self, staged: StagedPack) -> Result<VerifiedInstalledPack, PackError> {
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
        Ok(VerifiedInstalledPack {
            installed: InstalledPack {
                manifest: staged.manifest,
                directory: destination,
            },
            _lock: staged._lock,
        })
    }

    /// Discard a staged pack that failed its smoke test.
    pub fn discard(&self, staged: StagedPack) -> Result<(), PackError> {
        std::fs::remove_dir_all(staged.directory)?;
        Ok(())
    }

    /// Discard staged and partially downloaded bytes of one version.
    pub fn discard_partial(&self, manifest: &PackManifest) -> Result<(), PackError> {
        manifest.validate()?;
        let _lock = self.lock(manifest)?;
        match std::fs::remove_dir_all(self.staging(manifest)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => Ok(result?),
        }
    }

    /// Remove one installed version. Project media never lives here. The
    /// version an update selected stays until it is rolled back.
    pub fn remove(&self, manifest: &PackManifest) -> Result<(), PackError> {
        manifest.validate()?;
        let _lock = self.lock(manifest)?;
        if self
            .pointer(&manifest.pack_id)?
            .is_some_and(|pointer| pointer.version == manifest.pack_version)
        {
            return Err(PackError::Update {
                code: "ModelPackActive",
                message: format!(
                    "{} {} is the active version; roll back first, or remove another installed version with --version <v>",
                    manifest.pack_id, manifest.pack_version
                ),
            });
        }
        match std::fs::remove_dir_all(self.active(manifest)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => Ok(result?),
        }
    }
}

fn part(name: &str) -> String {
    format!("{name}.part")
}

fn device(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(path)?.dev())
}

/// A regular file of exact size at one of the accepted import locations.
fn import_candidate(manifest: &PackManifest, directory: &Path, file: &PackFile) -> Option<PathBuf> {
    [
        directory.join(&file.name),
        directory
            .join(&manifest.pack_id)
            .join(&manifest.pack_version)
            .join(&file.name),
    ]
    .into_iter()
    .find(|candidate| {
        std::fs::symlink_metadata(candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() == file.bytes)
    })
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
    let validator_path = validator_path(partial);
    let mut offset = output.metadata()?.len();
    if offset > file.bytes {
        output.set_len(0)?;
        offset = 0;
    }
    if offset == file.bytes {
        remove_if_present(&validator_path)?;
        return Ok(());
    }
    let stored = if offset > 0 {
        read_validator(&validator_path)
    } else {
        None
    };
    let (download, validator) = transport.fetch_resumable(&file.url, offset, stored.as_deref())?;
    if download.offset != offset {
        if download.offset != 0 {
            // A range the request did not ask for: keep the partial bytes
            // and try again later rather than discarding them.
            return Err(PackError::Transport(
                "server resumed from an unexpected offset; the partial download is kept".into(),
            ));
        }
        // A full response (range ignored, or the resource changed since the
        // partial bytes were written) restarts the file here.
        output.set_len(0)?;
        offset = 0;
    }
    if offset == 0 {
        match &validator {
            Some(validator) => write_synced(&validator_path, validator.as_bytes())?,
            None => remove_if_present(&validator_path)?,
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
    remove_if_present(&validator_path)?;
    Ok(())
}

/// The resume validator beside a `.part` file. Manifest path components never
/// start with a dot, so it cannot collide with a pack file.
fn validator_path(partial: &Path) -> PathBuf {
    let name = partial
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    partial.with_file_name(format!(".{name}.validator"))
}

fn read_validator(path: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(1024)
        .read_to_string(&mut text)
        .ok()?;
    (!text.is_empty() && text.len() < 1024 && !text.chars().any(char::is_control)).then_some(text)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
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

fn verify_file(
    file: &PackFile,
    path: &Path,
    cancelled: &AtomicBool,
    progress: impl FnMut(u64),
) -> Result<(), PackError> {
    let result = verify_bytes(file, path, cancelled, progress);
    if matches!(
        &result,
        Err(PackError::Verification {
            reason: "SHA-256 differs from its manifest",
            ..
        })
    ) {
        // Only staging callers discard corrupt downloads. Installed bytes
        // are preserved when revalidation fails.
        std::fs::remove_file(path)?;
    }
    result
}

fn verify_bytes(
    file: &PackFile,
    path: &Path,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(), PackError> {
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let mut input = File::from(descriptor);
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() != file.bytes {
        return Err(PackError::Verification {
            file: file.name.clone(),
            reason: "size differs from its manifest",
        });
    }
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    let mut hashed = 0;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(PackError::Cancelled);
        }
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hashed += read as u64;
        if hashed > file.bytes {
            return Err(PackError::Verification {
                file: file.name.clone(),
                reason: "size differs from its manifest",
            });
        }
        hasher.update(&buffer[..read]);
        if hashed % (64 << 20) < read as u64 {
            progress(hashed);
        }
    }
    progress(hashed);
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != file.sha256 {
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
/// Server certificates are checked against the system trust store.
pub struct HttpsTransport {
    agent: ureq::Agent,
}

impl Default for HttpsTransport {
    fn default() -> Self {
        Self::with_user_agent(USER_AGENT)
    }
}

impl HttpsTransport {
    /// The same HTTPS-only, redirect-bounded transport with another user agent.
    pub fn with_user_agent(user_agent: &str) -> Self {
        Self::build(user_agent, ureq::tls::RootCerts::PlatformVerifier)
    }

    /// The same transport trusting only the given DER root certificates
    /// instead of the system trust store, so tests can exercise it against a
    /// local HTTPS server. Plain HTTP stays refused.
    #[cfg(test)]
    pub fn with_trusted_roots(user_agent: &str, roots: &[&[u8]]) -> Self {
        let roots = roots
            .iter()
            .map(|der| ureq::tls::Certificate::from_der(der).to_owned());
        Self::build(user_agent, ureq::tls::RootCerts::from(roots))
    }

    fn build(user_agent: &str, roots: ureq::tls::RootCerts) -> Self {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(5)
            .user_agent(user_agent)
            .tls_config(ureq::tls::TlsConfig::builder().root_certs(roots).build())
            .timeout_connect(Some(std::time::Duration::from_secs(20)))
            .timeout_recv_response(Some(std::time::Duration::from_secs(60)))
            .build()
            .into();
        Self { agent }
    }
}

impl Transport for HttpsTransport {
    fn fetch(&self, url: &str, offset: u64) -> Result<Download, PackError> {
        Ok(self.fetch_resumable(url, offset, None)?.0)
    }

    fn fetch_resumable(
        &self,
        url: &str,
        offset: u64,
        validator: Option<&str>,
    ) -> Result<(Download, Option<String>), PackError> {
        let mut request = self.agent.get(url);
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
            if let Some(validator) = validator {
                request = request.header("If-Range", validator);
            }
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
        // If-Range needs a strong entity tag, else the modification date.
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
                .filter(|value| value.len() < 1024 && !value.chars().any(char::is_control))
        };
        let validator = header("etag")
            .filter(|tag| !tag.starts_with("W/"))
            .or_else(|| header("last-modified"));
        Ok((
            Download {
                offset: start,
                body: Box::new(response.into_body().into_reader()),
            },
            validator,
        ))
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

#[cfg(all(test, target_os = "macos"))]
mod disk_full_tests;
#[cfg(test)]
mod interrupted_download_tests;
#[cfg(test)]
mod tests;
