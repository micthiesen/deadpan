//! Pinned downloader helpers: yt-dlp, its bundled EJS scripts and Deno.
//!
//! Development stand-in for the signed release bundle (DP-22). This build
//! accepts exactly the pinned upstream release files below. Installation
//! downloads over HTTPS into a private staging directory, verifies exact size
//! and SHA-256 (and, for Deno, the extracted executable's size and SHA-256),
//! then publishes one complete `<root>/<name>/<version>` directory by rename.
//! A published version is never overwritten; every use re-verifies the
//! executable bytes before running them. The release application must ship
//! these helpers inside its signed bundle instead of downloading them.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_models::packs::{HttpsTransport, Transport};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::CliError;

/// User agent for helper downloads made by this client.
pub const USER_AGENT: &str = "OpenAI File Downloader, XaiImageApiFetch/1.0";
/// The yt-dlp-ejs scripts embedded in the pinned official yt-dlp executable,
/// as its own `--verbose` diagnostics report them.
pub const EJS_VERSION: &str = "0.8.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Packaging {
    /// The download is the executable.
    Executable,
    /// The download is a ZIP archive whose only entry is the executable.
    ZipEntry { entry: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HelperPin {
    pub name: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub url: &'static str,
    pub download_sha256: &'static str,
    pub download_bytes: u64,
    pub packaging: Packaging,
    pub executable: &'static str,
    pub executable_sha256: &'static str,
    pub executable_bytes: u64,
}

/// yt-dlp's official macOS standalone build (universal2, PyInstaller one-file).
/// Hash from the release's SHA2-256SUMS.
pub const YT_DLP: HelperPin = HelperPin {
    name: "yt-dlp",
    version: "2026.08.19",
    license: "Unlicense",
    url: "https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp_macos",
    download_sha256: "0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202",
    download_bytes: 37_146_048,
    packaging: Packaging::Executable,
    executable: "yt-dlp_macos",
    executable_sha256: "0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202",
    executable_bytes: 37_146_048,
};

/// Deno for Apple Silicon. Archive hash from the release's `.sha256sum`;
/// the extracted executable was measured from that verified archive.
pub const DENO: HelperPin = HelperPin {
    name: "deno",
    version: "2.9.7",
    license: "MIT",
    url: "https://github.com/denoland/deno/releases/download/v2.9.7/deno-aarch64-apple-darwin.zip",
    download_sha256: "5cd46d6268f6f78f5d88bdc7159d20bd44cdaa4b3303474839f87ec6fe7ae25c",
    download_bytes: 38_469_316,
    packaging: Packaging::ZipEntry { entry: "deno" },
    executable: "deno",
    executable_sha256: "b73737579d5a84c160e3316487594783fa5c15f4e13252a6a07050b755317f1a",
    executable_bytes: 80_982_000,
};

pub const BUNDLE: [HelperPin; 2] = [YT_DLP, DENO];

/// `~/Library/Application Support/Deadpan/helpers` on macOS.
pub fn default_root() -> Result<PathBuf, CliError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or_else(|| CliError::Usage("HOME is not set; pass --root".into()))?;
    Ok(home.join("Library/Application Support/Deadpan/helpers"))
}

fn supported_platform() -> Result<(), CliError> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Ok(())
    } else {
        Err(helper_error(
            "DownloaderUnsupportedPlatform",
            "the pinned downloader helpers are Apple Silicon macOS builds",
        ))
    }
}

pub(crate) fn helper_error(code: &'static str, message: impl ToString) -> CliError {
    CliError::Import(super::ImportError::new(code, message.to_string()))
}

impl HelperPin {
    pub fn directory(&self, root: &Path) -> PathBuf {
        root.join(self.name).join(self.version)
    }

    pub fn path(&self, root: &Path) -> PathBuf {
        self.directory(root).join(self.executable)
    }

    /// The published executable after a complete size and SHA-256 check.
    /// Symbolic links and other file types are refused.
    pub fn verified(&self, root: &Path) -> Result<Option<PathBuf>, CliError> {
        let path = self.path(root);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !metadata.file_type().is_file() || metadata.len() != self.executable_bytes {
            return Err(helper_error(
                "DownloaderHelperInvalid",
                format!(
                    "{} {} at {} is not the pinned file",
                    self.name,
                    self.version,
                    path.display()
                ),
            ));
        }
        let mut file = File::open(&path)?;
        let (digest, length) = hash(&mut file, self.executable_bytes, None)?;
        if digest != self.executable_sha256 || length != self.executable_bytes {
            return Err(helper_error(
                "DownloaderHelperInvalid",
                format!(
                    "{} {} at {} failed SHA-256 verification",
                    self.name,
                    self.version,
                    path.display()
                ),
            ));
        }
        if metadata.permissions().mode() & 0o100 == 0 {
            return Err(helper_error(
                "DownloaderHelperInvalid",
                format!("{} at {} is not executable", self.name, path.display()),
            ));
        }
        not_shared(&path, &metadata)?;
        Ok(Some(path))
    }
}

/// Refuse a helper that another local user could replace: the executable and
/// every directory above it must belong to this user or root and must not be
/// group- or world-writable (a sticky world-writable directory such as /tmp
/// cannot have its entries replaced by others and is allowed).
fn not_shared(path: &Path, metadata: &fs::Metadata) -> Result<(), CliError> {
    use std::os::unix::fs::MetadataExt;
    let user = rustix::process::getuid().as_raw();
    let shared = |metadata: &fs::Metadata, directory: bool| {
        let mode = metadata.mode();
        let sticky = directory && mode & 0o1000 != 0;
        (metadata.uid() != user && metadata.uid() != 0) || (mode & 0o022 != 0 && !sticky)
    };
    let refuse = |what: &Path| {
        helper_error(
            "DownloaderHelperInvalid",
            format!(
                "{} is writable by other users; helpers must live in a private directory",
                what.display()
            ),
        )
    };
    if shared(metadata, false) {
        return Err(refuse(path));
    }
    let directory = fs::canonicalize(path.parent().unwrap_or(Path::new("/")))?;
    for ancestor in directory.ancestors() {
        if shared(&fs::metadata(ancestor)?, true) {
            return Err(refuse(ancestor));
        }
    }
    Ok(())
}

/// Verified absolute helper paths for one acquisition.
#[derive(Debug, Clone)]
pub struct Helpers {
    pub yt_dlp: PathBuf,
    pub yt_dlp_version: String,
    pub deno: PathBuf,
    pub deno_version: String,
    /// The pinned install root to re-verify before each launch. `None` only
    /// for explicitly supplied test stand-ins, which are not pinned.
    pub pinned_root: Option<PathBuf>,
}

impl Helpers {
    pub fn resolve(root: &Path) -> Result<Self, CliError> {
        supported_platform()?;
        let missing = |pin: &HelperPin| {
            helper_error(
                "DownloaderNotInstalled",
                format!(
                    "{} {} is not installed under {}; run `deadpan-cli downloader install`",
                    pin.name,
                    pin.version,
                    root.display()
                ),
            )
        };
        Ok(Self {
            yt_dlp: YT_DLP.verified(root)?.ok_or_else(|| missing(&YT_DLP))?,
            yt_dlp_version: YT_DLP.version.into(),
            deno: DENO.verified(root)?.ok_or_else(|| missing(&DENO))?,
            deno_version: DENO.version.into(),
            pinned_root: Some(root.to_owned()),
        })
    }

    /// Re-verify both pinned executables immediately before a yt-dlp launch
    /// (yt-dlp starts Deno during that run). Files can still change between
    /// this check and exec only through this user's own access, which the
    /// directory permission checks restrict to this user.
    pub fn recheck(&self) -> Result<(), CliError> {
        let Some(root) = &self.pinned_root else {
            return Ok(());
        };
        for (pin, path) in [(&YT_DLP, &self.yt_dlp), (&DENO, &self.deno)] {
            if pin.verified(root)?.as_ref() != Some(path) {
                return Err(helper_error(
                    "DownloaderHelperInvalid",
                    format!("{} changed after it was verified", pin.name),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct HelperStatus {
    pub name: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub path: PathBuf,
    pub installed: bool,
    pub verified: bool,
    pub problem: Option<String>,
}

pub fn status(root: &Path) -> Vec<HelperStatus> {
    BUNDLE
        .iter()
        .map(|pin| {
            let (installed, verified, problem) = match pin.verified(root) {
                Ok(Some(_)) => (true, true, None),
                Ok(None) => (false, false, None),
                Err(error) => (true, false, Some(error.to_string())),
            };
            HelperStatus {
                name: pin.name,
                version: pin.version,
                license: pin.license,
                path: pin.path(root),
                installed,
                verified,
                problem,
            }
        })
        .collect()
}

/// Download, verify and publish one pinned helper unless already published.
/// Returns whether this call published it.
pub fn install(
    pin: &HelperPin,
    root: &Path,
    transport: &dyn Transport,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<bool, CliError> {
    if pin.verified(root)?.is_some() {
        return Ok(false);
    }
    if pin.directory(root).exists() {
        return Err(helper_error(
            "DownloaderHelperInvalid",
            format!(
                "{} exists without the pinned executable; it is never overwritten",
                pin.directory(root).display()
            ),
        ));
    }
    let staging_root = root.join(".staging");
    fs::create_dir_all(&staging_root)?;
    let staging = tempfile::Builder::new()
        .prefix(&format!("{}-{}-", pin.name, pin.version))
        .tempdir_in(&staging_root)?;
    let download = staging.path().join("download");
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&download)?;
        let response = transport.fetch(pin.url, 0)?;
        if response.offset != 0 {
            return Err(helper_error(
                "DownloaderInstallFailed",
                "server answered a full download with a partial response",
            ));
        }
        // A blocked socket read never delays cancellation or stall detection.
        let chunks = deadpan_models::packs::read_in_background(response.body);
        let mut total = 0u64;
        let mut last_data = std::time::Instant::now();
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(helper_error(
                    "DownloaderInstallCancelled",
                    "installation was cancelled",
                ));
            }
            let chunk = match chunks.recv_timeout(deadpan_models::packs::POLL) {
                Ok(chunk) => chunk?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if last_data.elapsed() >= deadpan_models::packs::STALL {
                        return Err(helper_error(
                            "DownloaderInstallFailed",
                            format!("the {} download stalled; retry the install", pin.name),
                        ));
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            last_data = std::time::Instant::now();
            total += chunk.len() as u64;
            if total > pin.download_bytes {
                return Err(helper_error(
                    "DownloaderInstallFailed",
                    format!("{} download exceeds its pinned size", pin.name),
                ));
            }
            file.write_all(&chunk)?;
            progress(total);
        }
        file.sync_all()?;
    }
    let (digest, length) = hash(
        &mut File::open(&download)?,
        pin.download_bytes,
        Some(cancelled),
    )?;
    if length != pin.download_bytes || digest != pin.download_sha256 {
        return Err(helper_error(
            "DownloaderInstallFailed",
            format!("{} download failed size or SHA-256 verification", pin.name),
        ));
    }
    let published = staging.path().join("publish");
    fs::create_dir(&published)?;
    let executable = published.join(pin.executable);
    match pin.packaging {
        Packaging::Executable => fs::rename(&download, &executable)?,
        Packaging::ZipEntry { entry } => {
            let archive = fs::read(&download)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&executable)?;
            extract_single_entry(&archive, entry, pin.executable_bytes, &mut output)?;
            output.sync_all()?;
        }
    }
    let (digest, length) = hash(
        &mut File::open(&executable)?,
        pin.executable_bytes,
        Some(cancelled),
    )?;
    if length != pin.executable_bytes || digest != pin.executable_sha256 {
        return Err(helper_error(
            "DownloaderInstallFailed",
            format!(
                "{} executable failed size or SHA-256 verification",
                pin.name
            ),
        ));
    }
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
    File::open(&published)?.sync_all()?;
    let destination = pin.directory(root);
    let parent = destination
        .parent()
        .expect("versioned helper directory has a parent");
    fs::create_dir_all(parent)?;
    // Never replace anything at the destination, including a racing empty
    // directory that a plain rename would silently overwrite.
    match rustix::fs::renameat_with(
        rustix::fs::CWD,
        &published,
        rustix::fs::CWD,
        &destination,
        rustix::fs::RenameFlags::NOREPLACE,
    ) {
        Ok(()) => {}
        // A concurrent installer published first; keep that version.
        Err(_) if pin.verified(root)?.is_some() => return Ok(false),
        Err(error) => return Err(io::Error::from(error).into()),
    }
    File::open(parent)?.sync_all()?;
    if pin.verified(root)?.is_none() {
        return Err(helper_error(
            "DownloaderInstallFailed",
            "published helper disappeared",
        ));
    }
    Ok(true)
}

fn hash(
    file: &mut File,
    maximum: u64,
    cancelled: Option<&AtomicBool>,
) -> Result<(String, u64), CliError> {
    let mut hasher = Sha256::new();
    let mut reader = file.take(maximum + 1);
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err(helper_error(
                "DownloaderInstallCancelled",
                "installation was cancelled",
            ));
        }
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        hasher.update(&buffer[..count]);
    }
    Ok((hex(&hasher.finalize()), total))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn le16(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn le32(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(u32::from_le_bytes(
        bytes.get(at..at + 4)?.try_into().ok()?,
    )))
}

/// Extract the only entry of a hash-verified ZIP archive, stored or deflated.
fn extract_single_entry(
    archive: &[u8],
    name: &str,
    expected_bytes: u64,
    output: &mut impl Write,
) -> Result<(), CliError> {
    let invalid = || {
        helper_error(
            "DownloaderInstallFailed",
            "unexpected helper archive layout",
        )
    };
    let search_start = archive.len().saturating_sub(22 + 65_535);
    let end = (search_start..=archive.len().saturating_sub(22))
        .rev()
        .find(|&at| archive.get(at..at + 4) == Some(&[0x50, 0x4b, 0x05, 0x06][..]))
        .ok_or_else(invalid)?;
    if le16(archive, end + 10) != Some(1) {
        return Err(invalid());
    }
    let directory =
        usize::try_from(le32(archive, end + 16).ok_or_else(invalid)?).map_err(|_| invalid())?;
    if archive.get(directory..directory + 4) != Some(&[0x50, 0x4b, 0x01, 0x02][..]) {
        return Err(invalid());
    }
    let method = le16(archive, directory + 10).ok_or_else(invalid)?;
    let compressed = le32(archive, directory + 20).ok_or_else(invalid)?;
    let uncompressed = le32(archive, directory + 24).ok_or_else(invalid)?;
    let name_length = le16(archive, directory + 28).ok_or_else(invalid)?;
    let local = usize::try_from(le32(archive, directory + 42).ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    if archive.get(directory + 46..directory + 46 + name_length) != Some(name.as_bytes())
        || uncompressed != expected_bytes
        || archive.get(local..local + 4) != Some(&[0x50, 0x4b, 0x03, 0x04][..])
    {
        return Err(invalid());
    }
    let data = local
        + 30
        + le16(archive, local + 26).ok_or_else(invalid)?
        + le16(archive, local + 28).ok_or_else(invalid)?;
    let data = archive
        .get(data..data + usize::try_from(compressed).map_err(|_| invalid())?)
        .ok_or_else(invalid)?;
    let copied = match method {
        0 => io::copy(&mut data.take(expected_bytes + 1), output)?,
        8 => io::copy(
            &mut flate2::read::DeflateDecoder::new(data).take(expected_bytes + 1),
            output,
        )?,
        _ => return Err(invalid()),
    };
    if copied != expected_bytes {
        return Err(invalid());
    }
    Ok(())
}

fn usage() -> CliError {
    CliError::Usage(
        "usage: downloader install [--root <dir>] | downloader status [--root <dir>] [--probe]"
            .into(),
    )
}

/// `downloader install|status`
pub(crate) fn run(arguments: &[&str]) -> Result<(), CliError> {
    let (command, mut options) = arguments.split_first().ok_or_else(usage)?;
    let mut root = None;
    let mut probe = false;
    while let Some((option, rest)) = options.split_first() {
        match (*option, rest) {
            ("--root", [value, rest @ ..]) if root.is_none() => {
                root = Some(PathBuf::from(value));
                options = rest;
            }
            ("--probe", rest) if *command == "status" && !probe => {
                probe = true;
                options = rest;
            }
            _ => return Err(usage()),
        }
    }
    let root = match root {
        Some(root) if root.is_absolute() => root,
        Some(_) => return Err(CliError::Usage("--root must be an absolute path".into())),
        None => default_root()?,
    };
    match *command {
        "status" => {
            let mut report = serde_json::json!({
                "protocol": 1,
                "root": root,
                "ejs": { "version": EJS_VERSION, "packaging": "embedded in the official yt-dlp executable" },
                "helpers": status(&root),
                "distribution": "development stand-in; the release bundle must ship signed helpers",
            });
            if probe {
                let helpers = Helpers::resolve(&root)?;
                report["probe"] = serde_json::to_value(super::acquire::probe(&helpers)?)?;
            }
            crate::write_json(&report)
        }
        "install" => {
            supported_platform()?;
            let cancelled = super::acquire::interrupt_flag()?;
            let transport = HttpsTransport::with_user_agent(USER_AGENT);
            let mut installed = Vec::new();
            for pin in &BUNDLE {
                super::emit(&serde_json::json!({
                    "event": "installing", "name": pin.name, "version": pin.version,
                    "bytes": pin.download_bytes, "license": pin.license, "url": pin.url,
                }))?;
                let mut reported = 0u64;
                let published = install(pin, &root, &transport, &cancelled, |done| {
                    if done >= reported + pin.download_bytes / 10 || done == pin.download_bytes {
                        reported = done;
                        let _ = super::emit(&serde_json::json!({
                            "event": "progress", "name": pin.name,
                            "completed_bytes": done, "total_bytes": pin.download_bytes,
                        }));
                    }
                })?;
                installed.push(serde_json::json!({
                    "name": pin.name, "version": pin.version, "path": pin.path(&root),
                    "already_installed": !published,
                }));
            }
            crate::write_json(&serde_json::json!({ "protocol": 1, "installed": installed }))
        }
        _ => Err(usage()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_models::packs::{Download, PackError};
    use std::sync::Mutex;

    struct Fixed(Vec<u8>, Mutex<u32>);

    impl Transport for Fixed {
        fn fetch(&self, _: &str, _: u64) -> Result<Download, PackError> {
            *self.1.lock().unwrap() += 1;
            Ok(Download {
                offset: 0,
                body: Box::new(io::Cursor::new(self.0.clone())),
            })
        }
    }

    fn sha(bytes: &[u8]) -> &'static str {
        Box::leak(hex(&Sha256::digest(bytes)).into_boxed_str())
    }

    fn raw_pin(bytes: &[u8]) -> HelperPin {
        HelperPin {
            name: "tool",
            version: "1.0",
            license: "MIT",
            url: "https://example.invalid/tool",
            download_sha256: sha(bytes),
            download_bytes: bytes.len() as u64,
            packaging: Packaging::Executable,
            executable: "tool",
            executable_sha256: sha(bytes),
            executable_bytes: bytes.len() as u64,
        }
    }

    /// A minimal one-entry ZIP holding `payload` deflated under `name`.
    fn zip(name: &str, payload: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        let data = encoder.finish().unwrap();
        let mut out = Vec::new();
        let header = |out: &mut Vec<u8>, central: bool| {
            out.extend_from_slice(if central {
                b"PK\x01\x02"
            } else {
                b"PK\x03\x04"
            });
            if central {
                out.extend_from_slice(&[20, 0]);
            }
            out.extend_from_slice(&[20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&[0, 0]);
            if central {
                out.extend_from_slice(&[0; 10]);
                out.extend_from_slice(&0u32.to_le_bytes());
            }
            out.extend_from_slice(name.as_bytes());
        };
        header(&mut out, false);
        out.extend_from_slice(&data);
        let directory = out.len();
        header(&mut out, true);
        let size = out.len() - directory;
        out.extend_from_slice(b"PK\x05\x06\0\0\0\0\x01\0\x01\0");
        out.extend_from_slice(&(size as u32).to_le_bytes());
        out.extend_from_slice(&(directory as u32).to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn verified_install_publishes_once_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"#!/bin/sh\necho tool\n".to_vec();
        let pin = raw_pin(&bytes);
        let transport = Fixed(bytes.clone(), Mutex::new(0));
        let cancelled = AtomicBool::new(false);
        assert!(install(&pin, root.path(), &transport, &cancelled, |_| {}).unwrap());
        assert_eq!(
            pin.verified(root.path()).unwrap(),
            Some(pin.path(root.path()))
        );
        assert!(!install(&pin, root.path(), &transport, &cancelled, |_| {}).unwrap());
        assert_eq!(*transport.1.lock().unwrap(), 1);
        assert_eq!(
            fs::metadata(pin.path(root.path()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );

        // Tampered published bytes fail verification and are not replaced.
        fs::write(pin.path(root.path()), b"#!/bin/sh\necho evil\n").unwrap();
        assert!(pin.verified(root.path()).is_err());
        assert!(install(&pin, root.path(), &transport, &cancelled, |_| {}).is_err());
    }

    #[test]
    fn helpers_others_can_modify_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"#!/bin/sh\necho tool\n".to_vec();
        let pin = raw_pin(&bytes);
        let transport = Fixed(bytes, Mutex::new(0));
        install(
            &pin,
            root.path(),
            &transport,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let path = pin.path(root.path());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o775)).unwrap();
        assert!(pin.verified(root.path()).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(pin.verified(root.path()).unwrap().is_some());
        let directory = pin.directory(root.path());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(pin.verified(root.path()).is_err());
        // A sticky world-writable directory (like /tmp) cannot have its
        // entries replaced by others.
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o1777)).unwrap();
        assert!(pin.verified(root.path()).unwrap().is_some());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Creates the destination while the download is in flight.
    struct Racing(Vec<u8>, PathBuf);

    impl Transport for Racing {
        fn fetch(&self, _: &str, _: u64) -> Result<Download, PackError> {
            fs::create_dir_all(&self.1).unwrap();
            Ok(Download {
                offset: 0,
                body: Box::new(io::Cursor::new(self.0.clone())),
            })
        }
    }

    #[test]
    fn publication_never_replaces_a_racing_directory() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"#!/bin/sh\necho tool\n".to_vec();
        let pin = raw_pin(&bytes);
        let transport = Racing(bytes, pin.directory(root.path()));
        assert!(
            install(
                &pin,
                root.path(),
                &transport,
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
        );
        assert!(pin.directory(root.path()).is_dir());
        assert!(!pin.path(root.path()).exists());
    }

    #[test]
    fn wrong_bytes_never_publish() {
        let root = tempfile::tempdir().unwrap();
        let pin = raw_pin(b"expected");
        for body in [
            b"tampered".to_vec(),
            b"expected plus".to_vec(),
            b"short".to_vec(),
        ] {
            let transport = Fixed(body, Mutex::new(0));
            assert!(
                install(
                    &pin,
                    root.path(),
                    &transport,
                    &AtomicBool::new(false),
                    |_| {}
                )
                .is_err()
            );
            assert!(!pin.directory(root.path()).exists());
        }
        let transport = Fixed(b"expected".to_vec(), Mutex::new(0));
        assert!(
            install(
                &pin,
                root.path(),
                &transport,
                &AtomicBool::new(true),
                |_| {}
            )
            .is_err()
        );
        assert!(!pin.directory(root.path()).exists());
    }

    #[test]
    fn zip_entries_extract_and_verify() {
        let root = tempfile::tempdir().unwrap();
        let payload = b"#!/bin/sh\necho deno\n".repeat(50);
        let archive = zip("deno", &payload);
        let pin = HelperPin {
            download_sha256: sha(&archive),
            download_bytes: archive.len() as u64,
            packaging: Packaging::ZipEntry { entry: "deno" },
            executable: "deno",
            executable_sha256: sha(&payload),
            executable_bytes: payload.len() as u64,
            ..raw_pin(b"")
        };
        let transport = Fixed(archive.clone(), Mutex::new(0));
        assert!(
            install(
                &pin,
                root.path(),
                &transport,
                &AtomicBool::new(false),
                |_| {}
            )
            .unwrap()
        );
        assert_eq!(fs::read(pin.path(root.path())).unwrap(), payload);

        let mut output = Vec::new();
        assert!(
            extract_single_entry(&archive, "other", payload.len() as u64, &mut output).is_err()
        );
        assert!(
            extract_single_entry(
                &archive[..archive.len() - 30],
                "deno",
                payload.len() as u64,
                &mut output
            )
            .is_err()
        );
    }

    #[test]
    fn pins_are_well_formed() {
        for pin in BUNDLE {
            assert!(pin.url.starts_with("https://github.com/"));
            assert_eq!(pin.download_sha256.len(), 64);
            assert_eq!(pin.executable_sha256.len(), 64);
            assert!(pin.download_bytes > 0 && pin.executable_bytes > 0);
        }
    }
}
