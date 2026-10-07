//! Pinned downloader helpers: yt-dlp, its bundled EJS scripts and Deno.
//!
//! Development stand-in for the signed release bundle (DP-22). This build
//! accepts exactly the pinned upstream release files below. Installation
//! downloads over HTTPS into a private staging directory, verifies exact size
//! and SHA-256 (and, for Deno, the extracted executable's size and SHA-256),
//! then publishes one complete `<root>/<name>/<version>` directory by rename.
//! A published version is never overwritten; every use re-verifies the
//! executable bytes before running them.
//!
//! A packaged `Deadpan.app` carries the same pinned releases as a read-only
//! baseline under `Contents/Resources/helpers` with a manifest written by
//! `cargo xtask bundle`. The running bundle's baseline is preferred; the managed
//! Application Support root remains the update location (§15.2). Release
//! signing may replace a helper's signature, so the manifest binds each pinned
//! upstream hash to the exact shipped bytes. See docs/PACKAGING.md.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_models::packs::{HttpsTransport, Transport};
use serde::{Deserialize, Serialize};
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
    /// Signature-independent content hash ([`super::macho_content`]) of the
    /// pinned executable. `Some` permits a bundle to ship it re-signed.
    pub content_sha256: Option<&'static str>,
    /// Code-signing requirement the upstream signature satisfies. A bundle
    /// that ships the upstream bytes also checks this before every launch.
    pub signer: Option<&'static str>,
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
    // Upstream is only ad hoc signed, so bundles re-sign it.
    content_sha256: Some("97335294737302995ed4dc5cd8a81c709a88ff52fe12a27cb7abab47ef5373c7"),
    signer: None,
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
    // Bundles keep Deno Land's hardened Developer ID signature unchanged.
    content_sha256: None,
    signer: Some(
        "anchor apple generic and identifier \"deno\" and certificate leaf[subject.OU] = \"2H4KBF436B\"",
    ),
};

pub const BUNDLE: [HelperPin; 2] = [YT_DLP, DENO];

/// How a release file becomes the executable; the owned form of [`Packaging`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReleasePackaging {
    Executable,
    ZipEntry { entry: String },
}

/// One helper release: a compiled pin, or one named by a verified signed
/// downloader update manifest ([`super::updates`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperRelease {
    pub name: String,
    pub version: String,
    pub license: String,
    pub url: String,
    pub download_sha256: String,
    pub download_bytes: u64,
    pub packaging: ReleasePackaging,
    pub executable: String,
    pub executable_sha256: String,
    pub executable_bytes: u64,
    /// Signature-independent Mach-O content hash of the executable, checked
    /// at installation; required for yt-dlp so a bundle may re-sign it.
    #[serde(default)]
    pub content_sha256: Option<String>,
    /// Code-signing requirement checked before every launch.
    #[serde(default)]
    pub signer: Option<String>,
}

impl From<&HelperPin> for HelperRelease {
    fn from(pin: &HelperPin) -> Self {
        Self {
            name: pin.name.into(),
            version: pin.version.into(),
            license: pin.license.into(),
            url: pin.url.into(),
            download_sha256: pin.download_sha256.into(),
            download_bytes: pin.download_bytes,
            packaging: match pin.packaging {
                Packaging::Executable => ReleasePackaging::Executable,
                Packaging::ZipEntry { entry } => ReleasePackaging::ZipEntry {
                    entry: entry.into(),
                },
            },
            executable: pin.executable.into(),
            executable_sha256: pin.executable_sha256.into(),
            executable_bytes: pin.executable_bytes,
            content_sha256: pin.content_sha256.map(str::to_owned),
            signer: pin.signer.map(str::to_owned),
        }
    }
}

impl HelperRelease {
    pub fn directory(&self, root: &Path) -> PathBuf {
        root.join(&self.name).join(&self.version)
    }

    pub fn path(&self, root: &Path) -> PathBuf {
        self.directory(root).join(&self.executable)
    }

    /// The published executable after a complete size and SHA-256 check.
    pub fn verified(&self, root: &Path) -> Result<Option<PathBuf>, CliError> {
        verify_file(
            &self.name,
            &self.version,
            &self.path(root),
            self.executable_bytes,
            &self.executable_sha256,
            None,
        )
    }
}

/// Manifest naming the bundled baseline's exact shipped bytes.
pub const BASELINE_MANIFEST: &str = "manifest.json";
pub const BASELINE_SCHEMA: u32 = 1;
const BASELINE_MANIFEST_LIMIT: u64 = 64 * 1024;

/// Whether a bundled helper keeps its publisher's signature or carries the
/// application's own (which changes the file bytes, never the pinned release).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineSignature {
    Upstream,
    Resigned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineHelper {
    pub name: String,
    pub version: String,
    pub executable: String,
    /// The pinned upstream executable this file was produced from.
    pub upstream_sha256: String,
    pub upstream_bytes: u64,
    /// The exact shipped file.
    pub sha256: String,
    pub bytes: u64,
    pub signature: BaselineSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineManifest {
    pub schema: u32,
    pub helpers: Vec<BaselineHelper>,
}

impl BaselineManifest {
    fn load(root: &Path) -> Result<Self, CliError> {
        let path = root.join(BASELINE_MANIFEST);
        let invalid = |problem: String| {
            helper_error(
                "DownloaderHelperInvalid",
                format!("bundled downloader manifest {}: {problem}", path.display()),
            )
        };
        let metadata = fs::symlink_metadata(&path).map_err(|error| invalid(error.to_string()))?;
        if !metadata.file_type().is_file() || metadata.len() > BASELINE_MANIFEST_LIMIT {
            return Err(invalid("not a bounded regular file".into()));
        }
        let mut bytes = Vec::new();
        File::open(&path)?
            .take(BASELINE_MANIFEST_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        let manifest: Self =
            serde_json::from_slice(&bytes).map_err(|error| invalid(error.to_string()))?;
        if manifest.schema != BASELINE_SCHEMA {
            return Err(invalid(format!("unsupported schema {}", manifest.schema)));
        }
        Ok(manifest)
    }

    /// The shipped entry for `pin`, which must name exactly that pinned release.
    fn entry(&self, pin: &HelperPin) -> Result<&BaselineHelper, CliError> {
        let entry = self
            .helpers
            .iter()
            .find(|entry| entry.name == pin.name)
            .ok_or_else(|| {
                helper_error(
                    "DownloaderHelperInvalid",
                    format!("the bundled downloader baseline has no {}", pin.name),
                )
            })?;
        let unchanged =
            entry.sha256 == entry.upstream_sha256 && entry.bytes == entry.upstream_bytes;
        if entry.version != pin.version
            || entry.executable != pin.executable
            || entry.upstream_sha256 != pin.executable_sha256
            || entry.upstream_bytes != pin.executable_bytes
            || entry.sha256.len() != 64
            || (entry.signature == BaselineSignature::Upstream) != unchanged
            || (entry.signature == BaselineSignature::Resigned && pin.content_sha256.is_none())
        {
            return Err(helper_error(
                "DownloaderHelperInvalid",
                format!(
                    "the bundled {} does not match pinned {} {}",
                    pin.name, pin.name, pin.version
                ),
            ));
        }
        Ok(entry)
    }
}

/// Where a set of pinned helpers is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperSource {
    /// Read-only baseline inside the running application bundle.
    Bundled(PathBuf),
    /// Managed install root with the compiled pins (`downloader install`).
    Managed(PathBuf),
    /// An active signed update installed under a managed root.
    Update(Box<super::updates::ActiveUpdate>),
}

impl HelperSource {
    /// The helpers an import uses: a compatible active signed update under the
    /// managed Application Support root, otherwise the baseline (the running
    /// packaged bundle's, else the managed root's compiled pins). Never
    /// depends on the working directory.
    pub fn default_source() -> Result<Self, CliError> {
        Ok(Self::default_selection()?.source)
    }

    /// [`Self::default_source`] with the reason an active update was not used.
    pub fn default_selection() -> Result<super::updates::Selection, CliError> {
        let baseline = Self::default_baseline()?;
        match default_root() {
            Ok(root) => super::updates::select(&root, baseline),
            // Without a home directory there is no update location.
            Err(_) if matches!(baseline, Self::Bundled(_)) => Ok(super::updates::Selection {
                source: baseline,
                note: None,
            }),
            Err(error) => Err(error),
        }
    }

    /// The baseline alone: the running bundle's, else the managed root's
    /// compiled pins.
    pub fn default_baseline() -> Result<Self, CliError> {
        match bundled_root() {
            Some(root) => Ok(Self::Bundled(root)),
            None => Ok(Self::Managed(default_root()?)),
        }
    }

    /// An explicit managed root: its compatible active update, otherwise its
    /// compiled pins.
    pub fn for_root(root: &Path) -> Result<Self, CliError> {
        Ok(super::updates::select(root, Self::Managed(root.to_owned()))?.source)
    }

    pub fn root(&self) -> &Path {
        match self {
            Self::Bundled(root) | Self::Managed(root) => root,
            Self::Update(update) => &update.root,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Bundled(_) => "bundled",
            Self::Managed(_) => "managed",
            Self::Update(_) => "update",
        }
    }

    /// The release this source provides for `pin`'s helper.
    pub fn release(&self, pin: &HelperPin) -> Result<HelperRelease, CliError> {
        match self {
            Self::Bundled(_) | Self::Managed(_) => Ok(HelperRelease::from(pin)),
            Self::Update(update) => update.release(pin.name).cloned(),
        }
    }

    /// The yt-dlp-ejs version embedded in this source's yt-dlp.
    pub fn ejs_version(&self) -> String {
        match self {
            Self::Bundled(_) | Self::Managed(_) => EJS_VERSION.into(),
            Self::Update(update) => update.manifest.ejs_version.clone(),
        }
    }

    /// The verified executable, `None` when its file is absent.
    pub fn verified(&self, pin: &HelperPin) -> Result<Option<PathBuf>, CliError> {
        match self {
            Self::Managed(root) => pin.verified(root),
            Self::Update(update) => update.verified(pin.name),
            Self::Bundled(root) => {
                let manifest = BaselineManifest::load(root)?;
                let entry = manifest.entry(pin)?;
                // Ancestors above the bundle (such as an admin-writable
                // /Applications) are outside the application's control; the
                // bundle's code signature seals its contents.
                let boundary = root
                    .ancestors()
                    .find(|path| path.extension().is_some_and(|extension| extension == "app"));
                let Some(path) = verify_file(
                    pin.name,
                    pin.version,
                    &pin.path(root),
                    entry.bytes,
                    &entry.sha256,
                    boundary,
                )?
                else {
                    return Ok(None);
                };
                // The manifest is not a trust root: tie the shipped file to the
                // compiled pin and to its expected signer.
                match entry.signature {
                    BaselineSignature::Upstream => {
                        // `entry` already equals the compiled pin's exact hash.
                        if let Some(requirement) = pin.signer {
                            signing::verify(&path, Some(requirement))?;
                        }
                    }
                    BaselineSignature::Resigned => {
                        let expected = pin.content_sha256.expect("checked by entry");
                        let bytes = read_bounded(&path, entry.bytes)?;
                        let content =
                            super::macho_content::content_sha256(&bytes).map_err(|error| {
                                helper_error(
                                    "DownloaderHelperInvalid",
                                    format!("bundled {}: {error}", pin.name),
                                )
                            })?;
                        if content != expected {
                            return Err(helper_error(
                                "DownloaderHelperInvalid",
                                format!(
                                    "the bundled {} code differs from pinned {} {}",
                                    pin.name, pin.name, pin.version
                                ),
                            ));
                        }
                        signing::verify(&path, signing::application_requirement()?.as_deref())?;
                    }
                }
                Ok(Some(path))
            }
        }
    }

    /// Cheap presence and manifest check for diagnostics; no hashing.
    pub fn inspect(&self, pin: &HelperPin) -> Result<PathBuf, CliError> {
        if let Self::Update(update) = self {
            let release = update.release(pin.name)?;
            let path = release.path(&update.root);
            let length = fs::symlink_metadata(&path)
                .ok()
                .filter(|metadata| metadata.file_type().is_file())
                .map(|metadata| metadata.len());
            if length != Some(release.executable_bytes) {
                return Err(helper_error(
                    "DownloaderHelperInvalid",
                    format!(
                        "{} {} of the active downloader update is missing or damaged; run `deadpan-cli downloader rollback`",
                        release.name, release.version
                    ),
                ));
            }
            return Ok(path);
        }
        let path = pin.path(self.root());
        if let Self::Bundled(root) = self {
            let manifest = BaselineManifest::load(root)?;
            let entry = manifest.entry(pin)?;
            let length = fs::symlink_metadata(&path)
                .ok()
                .filter(|metadata| metadata.file_type().is_file())
                .map(|metadata| metadata.len());
            if length != Some(entry.bytes) {
                return Err(helper_error(
                    "DownloaderHelperInvalid",
                    format!(
                        "{} is missing or damaged in the application bundle",
                        pin.name
                    ),
                ));
            }
        } else if !path.is_file() {
            return Err(helper_error(
                "DownloaderNotInstalled",
                format!("{} {} is not installed", pin.name, pin.version),
            ));
        }
        Ok(path)
    }
}

fn read_bounded(path: &Path, bytes: u64) -> Result<Vec<u8>, CliError> {
    let mut content = Vec::new();
    File::open(path)?
        .take(bytes + 1)
        .read_to_end(&mut content)?;
    if content.len() as u64 != bytes {
        return Err(helper_error(
            "DownloaderHelperInvalid",
            format!("{} changed while it was verified", path.display()),
        ));
    }
    Ok(content)
}

/// Code-signature checks for bundled helpers through the system `codesign`.
#[cfg(target_os = "macos")]
pub(crate) mod signing {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use super::{CliError, helper_error};

    const CODESIGN: &str = "/usr/bin/codesign";
    const DEADLINE: Duration = Duration::from_secs(60);

    fn run(arguments: &[&std::ffi::OsStr]) -> Result<(bool, String), CliError> {
        let mut command = Command::new(CODESIGN);
        command
            .args(arguments)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = deadpan_native_process::spawn(&mut command)?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() > DEADLINE {
                let _ = child.kill();
                let _ = child.wait();
                return Err(helper_error(
                    "DownloaderHelperInvalid",
                    "code-signature verification timed out",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let mut diagnostics = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            use std::io::Read;
            let _ = (&mut stderr)
                .take(64 * 1024)
                .read_to_string(&mut diagnostics);
        }
        Ok((status.success(), diagnostics))
    }

    /// `codesign --verify --strict`, against `requirement` when given.
    pub(crate) fn verify(path: &Path, requirement: Option<&str>) -> Result<(), CliError> {
        let tested = requirement.map(|requirement| format!("-R={requirement}"));
        let mut arguments: Vec<&std::ffi::OsStr> = vec!["--verify".as_ref(), "--strict".as_ref()];
        if let Some(tested) = &tested {
            arguments.push(tested.as_ref());
        }
        arguments.push(path.as_os_str());
        let (valid, diagnostics) = run(&arguments)?;
        if valid {
            Ok(())
        } else {
            Err(helper_error(
                "DownloaderHelperInvalid",
                format!(
                    "{} failed code-signature verification: {}",
                    path.display(),
                    diagnostics.trim()
                ),
            ))
        }
    }

    /// For a Developer ID signed application, the requirement that its own
    /// re-signed helpers carry its team's Developer ID signature. Ad hoc
    /// development bundles have no signer identity; `None` checks validity.
    pub(crate) fn application_requirement() -> Result<Option<String>, CliError> {
        static TEAM: OnceLock<Option<String>> = OnceLock::new();
        if let Some(team) = TEAM.get() {
            return Ok(team.as_ref().map(|team| requirement(team)));
        }
        let executable = std::fs::canonicalize(std::env::current_exe()?)?;
        let (_, details) = run(&[
            "-dv".as_ref(),
            "--verbose=2".as_ref(),
            executable.as_os_str(),
        ])?;
        let team = details
            .lines()
            .find_map(|line| line.strip_prefix("TeamIdentifier="))
            .map(str::trim)
            .filter(|team| !team.is_empty() && team.chars().all(|c| c.is_ascii_alphanumeric()))
            .map(str::to_owned);
        let team = TEAM.get_or_init(|| team);
        Ok(team.as_ref().map(|team| requirement(team)))
    }

    fn requirement(team: &str) -> String {
        format!("anchor apple generic and certificate leaf[subject.OU] = \"{team}\"")
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) mod signing {
    use std::path::Path;

    use super::{CliError, helper_error};

    pub(crate) fn verify(_: &Path, _: Option<&str>) -> Result<(), CliError> {
        Err(helper_error(
            "DownloaderUnsupportedPlatform",
            "bundled helpers exist only in macOS application bundles",
        ))
    }

    pub(crate) fn application_requirement() -> Result<Option<String>, CliError> {
        Ok(None)
    }
}

/// The running packaged bundle's `Contents/Resources/helpers`. Inside a
/// packaged bundle this is the source even when it is missing or damaged, so
/// verification fails instead of silently using another copy.
pub fn bundled_root() -> Option<PathBuf> {
    crate::bundle::packaged_contents()
        .map(|contents| contents.join(crate::bundle::HELPERS_DIRECTORY))
}

/// `~/Library/Application Support/Deadpan/helpers` on macOS.
pub fn default_root() -> Result<PathBuf, CliError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or_else(|| CliError::Usage("HOME is not set; pass --root".into()))?;
    Ok(home.join("Library/Application Support/Deadpan/helpers"))
}

pub fn supported_platform() -> Result<(), CliError> {
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
        verify_file(
            self.name,
            self.version,
            &self.path(root),
            self.executable_bytes,
            self.executable_sha256,
            None,
        )
    }
}

/// Exact size, SHA-256, owner-execute and private-directory checks. Directory
/// checks stop after `boundary` when one is given.
fn verify_file(
    name: &str,
    version: &str,
    path: &Path,
    bytes: u64,
    sha256: &str,
    boundary: Option<&Path>,
) -> Result<Option<PathBuf>, CliError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() || metadata.len() != bytes {
        return Err(helper_error(
            "DownloaderHelperInvalid",
            format!(
                "{name} {version} at {} is not the pinned file",
                path.display()
            ),
        ));
    }
    let mut file = File::open(path)?;
    let (digest, length) = hash(&mut file, bytes, None)?;
    if digest != sha256 || length != bytes {
        return Err(helper_error(
            "DownloaderHelperInvalid",
            format!(
                "{name} {version} at {} failed SHA-256 verification",
                path.display()
            ),
        ));
    }
    if metadata.permissions().mode() & 0o100 == 0 {
        return Err(helper_error(
            "DownloaderHelperInvalid",
            format!("{name} at {} is not executable", path.display()),
        ));
    }
    not_shared(path, &metadata, boundary)?;
    Ok(Some(path.to_owned()))
}

/// Refuse a helper that another local user could replace: the executable and
/// every directory above it must belong to this user or root and must not be
/// group- or world-writable (a sticky world-writable directory such as /tmp
/// cannot have its entries replaced by others and is allowed).
fn not_shared(
    path: &Path,
    metadata: &fs::Metadata,
    boundary: Option<&Path>,
) -> Result<(), CliError> {
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
    let boundary = boundary.map(fs::canonicalize).transpose()?;
    for ancestor in directory.ancestors() {
        if shared(&fs::metadata(ancestor)?, true) {
            return Err(refuse(ancestor));
        }
        if boundary.as_deref() == Some(ancestor) {
            break;
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
    /// The yt-dlp-ejs version embedded in this yt-dlp.
    pub ejs_version: String,
    /// Why an installed signed update was passed over for this set, shown to
    /// the user and recorded in provenance.
    pub selection_note: Option<String>,
    /// The pinned source to re-verify before each launch. `None` only for
    /// explicitly supplied test stand-ins, which are not pinned.
    pub pinned: Option<HelperSource>,
}

impl Helpers {
    /// The helpers under one explicit managed root: its compatible active
    /// update, otherwise its compiled pins.
    pub fn resolve(root: &Path) -> Result<Self, CliError> {
        Self::resolve_selection(&super::updates::select(
            root,
            HelperSource::Managed(root.to_owned()),
        )?)
    }

    /// A compatible active signed update, otherwise the baseline, keeping
    /// the reason an update was passed over.
    pub fn resolve_default() -> Result<Self, CliError> {
        Self::resolve_selection(&HelperSource::default_selection()?)
    }

    pub fn resolve_selection(selection: &super::updates::Selection) -> Result<Self, CliError> {
        Ok(Self {
            selection_note: selection.note.clone(),
            ..Self::resolve_source(&selection.source)?
        })
    }

    pub fn resolve_source(source: &HelperSource) -> Result<Self, CliError> {
        supported_platform()?;
        let missing = |pin: &HelperPin| match source {
            HelperSource::Managed(root) => helper_error(
                "DownloaderNotInstalled",
                format!(
                    "{} {} is not installed under {}; run `deadpan-cli downloader install`",
                    pin.name,
                    pin.version,
                    root.display()
                ),
            ),
            HelperSource::Bundled(root) => helper_error(
                "DownloaderHelperInvalid",
                format!(
                    "{} {} is missing from the application bundle at {}; reinstall Deadpan",
                    pin.name,
                    pin.version,
                    root.display()
                ),
            ),
            HelperSource::Update(update) => helper_error(
                "DownloaderHelperInvalid",
                format!(
                    "{} of downloader update {} is missing under {}; run `deadpan-cli downloader rollback`",
                    pin.name,
                    update.serial,
                    update.root.display()
                ),
            ),
        };
        Ok(Self {
            yt_dlp: source.verified(&YT_DLP)?.ok_or_else(|| missing(&YT_DLP))?,
            yt_dlp_version: source.release(&YT_DLP)?.version,
            deno: source.verified(&DENO)?.ok_or_else(|| missing(&DENO))?,
            deno_version: source.release(&DENO)?.version,
            ejs_version: source.ejs_version(),
            selection_note: None,
            pinned: Some(source.clone()),
        })
    }

    /// Re-verify both pinned executables immediately before a yt-dlp launch
    /// (yt-dlp starts Deno during that run). Files can still change between
    /// this check and exec only through this user's own access, which the
    /// directory permission checks restrict to this user.
    pub fn recheck(&self) -> Result<(), CliError> {
        let Some(source) = &self.pinned else {
            return Ok(());
        };
        for (pin, path) in [(&YT_DLP, &self.yt_dlp), (&DENO, &self.deno)] {
            if source.verified(pin)?.as_ref() != Some(path) {
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
    pub name: String,
    pub version: String,
    pub license: String,
    /// The pinned upstream executable.
    pub pinned_sha256: String,
    pub pinned_bytes: u64,
    pub path: PathBuf,
    pub installed: bool,
    pub verified: bool,
    pub problem: Option<String>,
}

/// Status of the compiled pins under one managed root.
pub fn status(root: &Path) -> Vec<HelperStatus> {
    status_source(&HelperSource::Managed(root.to_owned()))
}

pub fn status_source(source: &HelperSource) -> Vec<HelperStatus> {
    BUNDLE
        .iter()
        .map(|pin| {
            let release = source
                .release(pin)
                .unwrap_or_else(|_| HelperRelease::from(pin));
            let (installed, verified, problem) = match source.verified(pin) {
                Ok(Some(_)) => (true, true, None),
                Ok(None) => (false, false, None),
                Err(error) => (true, false, Some(error.to_string())),
            };
            HelperStatus {
                path: release.path(source.root()),
                name: release.name,
                version: release.version,
                license: release.license,
                pinned_sha256: release.executable_sha256,
                pinned_bytes: release.executable_bytes,
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
    progress: impl FnMut(u64),
) -> Result<bool, CliError> {
    install_release(
        &HelperRelease::from(pin),
        root,
        transport,
        cancelled,
        progress,
    )
}

/// [`install`] for any release, including one from a verified signed update
/// manifest. A release with a Mach-O content pin must also match it.
pub fn install_release(
    pin: &HelperRelease,
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
        let response = transport.fetch(&pin.url, 0).map_err(|_| {
            helper_error(
                "DownloaderNetworkFailed",
                format!(
                    "could not download {}; check the connection and retry",
                    pin.name
                ),
            )
        })?;
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
                Ok(chunk) => chunk.map_err(|_| {
                    helper_error(
                        "DownloaderNetworkFailed",
                        format!(
                            "the {} download was interrupted; check the connection and retry",
                            pin.name
                        ),
                    )
                })?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if last_data.elapsed() >= deadpan_models::packs::STALL {
                        return Err(helper_error(
                            "DownloaderNetworkFailed",
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
    let executable = published.join(&pin.executable);
    match &pin.packaging {
        ReleasePackaging::Executable => fs::rename(&download, &executable)?,
        ReleasePackaging::ZipEntry { entry } => {
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
    if let Some(expected) = &pin.content_sha256 {
        let bytes = read_bounded(&executable, pin.executable_bytes)?;
        let content = super::macho_content::content_sha256(&bytes).map_err(|error| {
            helper_error("DownloaderInstallFailed", format!("{}: {error}", pin.name))
        })?;
        if &content != expected {
            return Err(helper_error(
                "DownloaderInstallFailed",
                format!("{} code differs from its content pin", pin.name),
            ));
        }
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
        "usage: downloader install [--root <dir>] | downloader status [--root <dir>] [--probe] | downloader update --manifest <file|https-url> [--allow-downgrade] [--root <dir>] | downloader rollback [--baseline] [--root <dir>]"
            .into(),
    )
}

/// `downloader install|status|update|rollback`
pub(crate) fn run(arguments: &[&str]) -> Result<(), CliError> {
    let (command, mut options) = arguments.split_first().ok_or_else(usage)?;
    let mut root = None;
    let mut probe = false;
    let mut manifest = None;
    let mut allow_downgrade = false;
    let mut to_baseline = false;
    while let Some((option, rest)) = options.split_first() {
        match (*command, *option, rest) {
            (_, "--root", [value, rest @ ..]) if root.is_none() => {
                root = Some(PathBuf::from(value));
                options = rest;
            }
            ("status", "--probe", rest) if !probe => {
                probe = true;
                options = rest;
            }
            ("update", "--manifest", [value, rest @ ..]) if manifest.is_none() => {
                manifest = Some(*value);
                options = rest;
            }
            ("update", "--allow-downgrade", rest) if !allow_downgrade => {
                allow_downgrade = true;
                options = rest;
            }
            ("rollback", "--baseline", rest) if !to_baseline => {
                to_baseline = true;
                options = rest;
            }
            _ => return Err(usage()),
        }
    }
    let root = match root {
        Some(root) if root.is_absolute() => Some(root),
        Some(_) => return Err(CliError::Usage("--root must be an absolute path".into())),
        None => None,
    };
    // An explicit root names a managed install (its compatible active update,
    // else its compiled pins); otherwise report what an import would use.
    let (managed, baseline) = match &root {
        Some(root) => (root.clone(), HelperSource::Managed(root.clone())),
        None => (default_root()?, HelperSource::default_baseline()?),
    };
    match *command {
        "status" => {
            let selection = super::updates::select(&managed, baseline.clone())?;
            let source = selection.source.clone();
            let distribution = match &source {
                HelperSource::Bundled(_) => {
                    "read-only baseline inside the application bundle; signed updates install under the managed root"
                }
                HelperSource::Managed(_) => {
                    "managed install of the compiled pins (development baseline)"
                }
                HelperSource::Update(_) => {
                    "signed update under the managed root; the baseline stays installed for rollback"
                }
            };
            let mut report = serde_json::json!({
                "protocol": 1,
                "source": source.kind(),
                "root": source.root(),
                "baseline": { "source": baseline.kind(), "root": baseline.root() },
                "ejs": { "version": source.ejs_version(), "packaging": "embedded in the official yt-dlp executable" },
                "helpers": status_source(&source),
                "updates": super::updates::report(&managed, &selection),
                "distribution": distribution,
            });
            if probe {
                let helpers = Helpers::resolve_source(&source)?;
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
                let published = install(pin, &managed, &transport, &cancelled, |done| {
                    if done >= reported + pin.download_bytes / 10 || done == pin.download_bytes {
                        reported = done;
                        let _ = super::emit(&serde_json::json!({
                            "event": "progress", "name": pin.name,
                            "completed_bytes": done, "total_bytes": pin.download_bytes,
                        }));
                    }
                })?;
                installed.push(serde_json::json!({
                    "name": pin.name, "version": pin.version, "path": pin.path(&managed),
                    "already_installed": !published,
                }));
            }
            crate::write_json(&serde_json::json!({ "protocol": 1, "installed": installed }))
        }
        "update" => {
            supported_platform()?;
            let manifest = manifest.ok_or_else(usage)?;
            let signed = crate::update_signing::read_signed(manifest, USER_AGENT)?;
            let cancelled = super::acquire::interrupt_flag()?;
            let transport = HttpsTransport::with_user_agent(USER_AGENT);
            let mut last = std::collections::HashMap::new();
            let outcome = super::updates::update(
                &managed,
                &signed,
                super::updates::UpdateOptions {
                    keys: &deadpan_models::updates::trusted_keys(),
                    allow_downgrade,
                },
                &transport,
                &cancelled,
                &super::acquire::probe,
                &mut |event| {
                    // Bounded progress: about every tenth of each download.
                    if event["event"] == "progress" {
                        let name = event["name"].as_str().unwrap_or_default().to_owned();
                        let done = event["completed_bytes"].as_u64().unwrap_or(0);
                        let total = event["total_bytes"].as_u64().unwrap_or(1).max(1);
                        let previous = last.get(&name).copied().unwrap_or(0u64);
                        if done < previous + total / 10 && done != total {
                            return;
                        }
                        last.insert(name, done);
                    }
                    let _ = super::emit(&event);
                },
            )?;
            crate::write_json(&serde_json::json!({ "protocol": 1, "updated": outcome }))
        }
        "rollback" => {
            supported_platform()?;
            let outcome = super::updates::rollback(
                &managed,
                &baseline,
                &deadpan_models::updates::trusted_keys(),
                to_baseline,
            )?;
            crate::write_json(&serde_json::json!({
                "protocol": 1, "state": outcome.state, "recovered": outcome.recovered,
            }))
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
            content_sha256: None,
            signer: None,
        }
    }

    #[test]
    fn helper_connection_and_body_failures_are_actionable_without_private_details() {
        struct Offline(bool);
        impl Transport for Offline {
            fn fetch(&self, _: &str, _: u64) -> Result<Download, PackError> {
                if self.0 {
                    Err(PackError::Transport(
                        "https://user:secret@example.invalid/?token=private".into(),
                    ))
                } else {
                    struct BrokenBody;
                    impl io::Read for BrokenBody {
                        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                            Err(io::Error::new(
                                io::ErrorKind::ConnectionReset,
                                "private response header",
                            ))
                        }
                    }
                    Ok(Download {
                        offset: 0,
                        body: Box::new(BrokenBody),
                    })
                }
            }
        }
        let root = tempfile::tempdir().unwrap();
        let pin = raw_pin(b"helper");
        for connection in [true, false] {
            let error = install(
                &pin,
                root.path(),
                &Offline(connection),
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap_err();
            assert_eq!(error.code(), "DownloaderNetworkFailed");
            let message = error.to_string();
            assert!(message.contains("check the connection and retry"));
            for secret in ["secret", "private", "example.invalid", "header"] {
                assert!(!message.contains(secret), "{message}");
            }
            assert!(pin.verified(root.path()).unwrap().is_none());
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

    /// A bundled baseline whose shipped `tool` differs from the pinned
    /// upstream bytes only through re-signing.
    fn baseline(
        upstream: &[u8],
        shipped: &[u8],
        signature: BaselineSignature,
    ) -> (tempfile::TempDir, PathBuf, HelperPin) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory
            .path()
            .join("Deadpan.app/Contents/Resources/helpers");
        let pin = raw_pin(upstream);
        fs::create_dir_all(pin.directory(&root)).unwrap();
        fs::write(pin.path(&root), shipped).unwrap();
        fs::set_permissions(pin.path(&root), fs::Permissions::from_mode(0o755)).unwrap();
        let manifest = BaselineManifest {
            schema: BASELINE_SCHEMA,
            helpers: vec![BaselineHelper {
                name: pin.name.into(),
                version: pin.version.into(),
                executable: pin.executable.into(),
                upstream_sha256: pin.executable_sha256.into(),
                upstream_bytes: pin.executable_bytes,
                sha256: sha(shipped).into(),
                bytes: shipped.len() as u64,
                signature,
            }],
        };
        fs::write(
            root.join(BASELINE_MANIFEST),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        (directory, root, pin)
    }

    fn write_manifest(root: &Path, pin: &HelperPin, shipped: &[u8], signature: BaselineSignature) {
        let manifest = BaselineManifest {
            schema: BASELINE_SCHEMA,
            helpers: vec![BaselineHelper {
                name: pin.name.into(),
                version: pin.version.into(),
                executable: pin.executable.into(),
                upstream_sha256: pin.executable_sha256.into(),
                upstream_bytes: pin.executable_bytes,
                sha256: sha(shipped).into(),
                bytes: shipped.len() as u64,
                signature,
            }],
        };
        fs::write(
            root.join(BASELINE_MANIFEST),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }

    /// A real signed system executable as the "upstream" pin and an ad hoc
    /// re-signed copy as the shipped baseline, inside a fake `.app`.
    fn resigned_baseline() -> (tempfile::TempDir, PathBuf, HelperPin) {
        let upstream = fs::read("/usr/bin/true").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let root = directory
            .path()
            .join("Deadpan.app/Contents/Resources/helpers");
        let pin = HelperPin {
            content_sha256: Some(Box::leak(
                super::super::macho_content::content_sha256(&upstream)
                    .unwrap()
                    .into_boxed_str(),
            )),
            ..raw_pin(&upstream)
        };
        fs::create_dir_all(pin.directory(&root)).unwrap();
        let path = pin.path(&root);
        fs::write(&path, &upstream).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        for arguments in [
            vec!["--remove-signature"],
            vec!["--force", "--options", "runtime", "-s", "-"],
        ] {
            let status = std::process::Command::new("/usr/bin/codesign")
                .args(arguments)
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success());
        }
        let shipped = fs::read(&path).unwrap();
        assert_ne!(shipped, upstream);
        write_manifest(&root, &pin, &shipped, BaselineSignature::Resigned);
        (directory, root, pin)
    }

    #[test]
    fn bundled_baseline_binds_resigned_bytes_to_the_compiled_pin() {
        let (_directory, root, pin) = resigned_baseline();
        let source = HelperSource::Bundled(root.clone());
        assert_eq!(source.verified(&pin).unwrap(), Some(pin.path(&root)));
        // The managed rule would reject the re-signed bytes.
        assert!(pin.verified(&root).is_err());
        // A different pinned release never accepts this baseline.
        let other = HelperPin {
            version: "2.0",
            ..pin
        };
        assert!(source.verified(&other).is_err());
        // Re-signing requires a compiled content pin.
        let unpinned = HelperPin {
            content_sha256: None,
            ..pin
        };
        assert!(source.verified(&unpinned).is_err());
    }

    #[test]
    fn a_tampered_resigned_helper_fails_even_with_a_matching_manifest() {
        let (_directory, root, pin) = resigned_baseline();
        let source = HelperSource::Bundled(root.clone());
        let path = pin.path(&root);
        let original = fs::read(&path).unwrap();
        // Change one byte of code inside the hashed content (not the
        // signature), then rewrite the manifest to match the tampered file.
        let mut tampered = original.clone();
        let at = tampered.len() / 3;
        tampered[at] ^= 0x5a;
        fs::write(&path, &tampered).unwrap();
        write_manifest(&root, &pin, &tampered, BaselineSignature::Resigned);
        assert!(source.verified(&pin).is_err());
        // Within the first slice's code, the compiled content pin rejects it
        // even before the signature check.
        let mut code = original.clone();
        let first = u32::from_be_bytes(code[16..20].try_into().unwrap()) as usize;
        let commands = u32::from_le_bytes(code[first + 20..first + 24].try_into().unwrap());
        code[first + 32 + commands as usize + 4] ^= 0x5a;
        fs::write(&path, &code).unwrap();
        write_manifest(&root, &pin, &code, BaselineSignature::Resigned);
        let error = source.verified(&pin).unwrap_err().to_string();
        assert!(error.contains("differs from pinned"), "{error}");
        // Appending data after the signature, with a matching manifest.
        let mut appended = original;
        appended.extend_from_slice(b"MEI\x0c\x0b\x0a\x0b\x0e");
        fs::write(&path, &appended).unwrap();
        write_manifest(&root, &pin, &appended, BaselineSignature::Resigned);
        assert!(source.verified(&pin).is_err());
    }

    #[test]
    fn an_upstream_signed_helper_must_satisfy_its_signer() {
        let upstream = fs::read("/usr/bin/true").unwrap();
        let (_directory, root, pin) = baseline(&upstream, &upstream, BaselineSignature::Upstream);
        let source = HelperSource::Bundled(root.clone());
        let apple = HelperPin {
            signer: Some("anchor apple"),
            ..pin
        };
        assert!(source.verified(&apple).unwrap().is_some());
        let deno = HelperPin {
            signer: DENO.signer,
            ..pin
        };
        assert!(source.verified(&deno).is_err());
    }

    #[test]
    fn inspection_reports_a_missing_bundle_baseline() {
        let directory = tempfile::tempdir().unwrap();
        let source = HelperSource::Bundled(directory.path().join("absent"));
        assert!(source.inspect(&YT_DLP).is_err());
        assert!(source.verified(&YT_DLP).is_err());
    }

    #[test]
    fn bundled_baseline_rejects_inconsistent_or_missing_manifests() {
        let upstream = b"#!/bin/sh\necho tool\n";
        // Claims the publisher's signature but ships different bytes.
        let (_directory, root, pin) = baseline(upstream, b"different", BaselineSignature::Upstream);
        assert!(HelperSource::Bundled(root.clone()).verified(&pin).is_err());

        let (_directory, root, pin) = baseline(upstream, upstream, BaselineSignature::Upstream);
        let source = HelperSource::Bundled(root.clone());
        assert!(source.verified(&pin).unwrap().is_some());
        fs::remove_file(root.join(BASELINE_MANIFEST)).unwrap();
        assert!(source.verified(&pin).is_err());
        fs::write(
            root.join(BASELINE_MANIFEST),
            b"{\"schema\": 2, \"helpers\": []}",
        )
        .unwrap();
        assert!(source.verified(&pin).is_err());
    }

    #[test]
    fn bundled_permission_checks_stop_at_the_app() {
        let upstream = b"#!/bin/sh\necho tool\n";
        let (directory, root, pin) = baseline(upstream, upstream, BaselineSignature::Upstream);
        let source = HelperSource::Bundled(root.clone());
        // A group-writable folder above the bundle, like /Applications.
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o775)).unwrap();
        assert!(source.verified(&pin).unwrap().is_some());
        assert!(HelperSource::Managed(root.clone()).verified(&pin).is_err());
        // Inside the bundle the private-directory rule still applies.
        fs::set_permissions(pin.directory(&root), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(source.verified(&pin).is_err());
        fs::set_permissions(pin.directory(&root), fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
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
