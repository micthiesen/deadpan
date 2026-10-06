//! Signed downloader updates with compatibility checks and rollback (§15.2).
//!
//! A downloader update is a [`SignedManifest`] of kind `downloader` whose
//! payload is a [`DownloaderManifest`]: a monotonic serial, the minimum app
//! version, the platform, the embedded yt-dlp-ejs version and one release each
//! of yt-dlp and Deno with exact URLs, sizes, SHA-256 hashes, yt-dlp's Mach-O
//! content pin and Deno's signer requirement.
//!
//! Updates live under the managed helper root, never inside the application
//! bundle:
//!
//! ```text
//! <root>/<name>/<version>/<executable>     versioned, never overwritten
//! <root>/updates/manifests/<serial>.json   the exact signed envelope
//! <root>/updates/state.json                active/previous selection
//! <root>/updates/.lock                     one updater at a time
//! ```
//!
//! `update` verifies the signature and compatibility, installs each release
//! into its own versioned directory (an existing version is reused only when
//! its bytes verify), retains the envelope, runs the real probe on the new
//! set and only then switches `state.json` by an atomic rename. The previous
//! selection and the baseline stay installed. `rollback` switches back.
//!
//! Selection on every use re-verifies the retained envelope against the
//! compiled keys. An active update is used only while it is compatible: this
//! build trusts its key, satisfies its minimum app version, and none of its
//! helpers is older than this build's baseline (unless the owner explicitly
//! accepted that against the same baseline). Otherwise the baseline is used
//! and the reason is reported. A bad signature or damaged files under a
//! trusted key are integrity failures and refuse instead of silently
//! switching copies.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use deadpan_models::packs::Transport;
use deadpan_models::updates::{
    SignedManifest, TrustedKey, UpdateError, UpdateKind, app_satisfies, compare_versions,
    trusted_keys,
};
use serde::{Deserialize, Serialize};

use super::acquire::ProbeReport;
use super::helpers::{
    BUNDLE, DENO, HelperRelease, HelperSource, Helpers, ReleasePackaging, YT_DLP, helper_error,
};
use crate::CliError;

pub const MANIFEST_SCHEMA: u32 = 1;
/// The only platform the pinned helper builds support.
pub const PLATFORM: &str = "macos-aarch64";
const STATE_SCHEMA: u32 = 1;
const UPDATES: &str = "updates";
const STATE: &str = "state.json";
const MANIFESTS: &str = "manifests";
const MAX_RELEASE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 16 * 1024;

/// A downloader update manifest: the payload of a signed envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloaderManifest {
    pub schema: u32,
    /// Monotonic; a lower serial than any installed one is a replay.
    pub serial: u64,
    /// `YYYY-MM-DD`.
    pub issued: String,
    pub min_app_version: String,
    pub platform: String,
    pub ejs_version: String,
    pub helpers: Vec<HelperRelease>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn bounded(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(reason: impl Into<String>) -> CliError {
    helper_error("UpdateManifestInvalid", reason.into())
}

impl DownloaderManifest {
    /// Parse and validate the structure (not compatibility with this build).
    pub fn parse(text: &str) -> Result<Self, CliError> {
        let manifest: Self =
            serde_json::from_str(text).map_err(|error| invalid(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), CliError> {
        if self.schema != MANIFEST_SCHEMA {
            return Err(invalid(format!("unsupported schema {}", self.schema)));
        }
        if self.serial == 0 {
            return Err(invalid("serial must be positive"));
        }
        let date = self.issued.as_bytes();
        if date.len() != 10
            || date[4] != b'-'
            || date[7] != b'-'
            || !date
                .iter()
                .enumerate()
                .all(|(at, byte)| at == 4 || at == 7 || byte.is_ascii_digit())
        {
            return Err(invalid("issued must be YYYY-MM-DD"));
        }
        if app_satisfies(&self.min_app_version).is_none() {
            return Err(invalid("min_app_version must be a dotted numeric version"));
        }
        if !safe(&self.platform) || !safe(&self.ejs_version) {
            return Err(invalid("platform and ejs_version must be safe identifiers"));
        }
        if self
            .notes
            .as_deref()
            .is_some_and(|notes| !bounded(notes, 1024))
        {
            return Err(invalid("notes must be bounded text"));
        }
        let names: Vec<&str> = self
            .helpers
            .iter()
            .map(|release| release.name.as_str())
            .collect();
        if names != [YT_DLP.name, DENO.name] {
            return Err(invalid("helpers must be exactly yt-dlp then deno"));
        }
        for release in &self.helpers {
            let problem = |reason: &str| invalid(format!("{}: {reason}", release.name));
            if !safe(&release.version) || compare_versions(&release.version, "0").is_none() {
                return Err(problem("version must be a dotted numeric version"));
            }
            if !bounded(&release.license, 64) {
                return Err(problem("license must be bounded text"));
            }
            if !release.url.starts_with("https://github.com/")
                || !bounded(&release.url, 512)
                || release.url.contains(['\\', ' ', '#', '?'])
            {
                return Err(problem("url must be an https://github.com/ release URL"));
            }
            if !hash(&release.download_sha256)
                || !hash(&release.executable_sha256)
                || release
                    .content_sha256
                    .as_deref()
                    .is_some_and(|value| !hash(value))
            {
                return Err(problem("hashes must be lowercase SHA-256"));
            }
            if !(1..=MAX_RELEASE_BYTES).contains(&release.download_bytes)
                || !(1..=MAX_RELEASE_BYTES).contains(&release.executable_bytes)
            {
                return Err(problem("sizes must be between 1 byte and 512 MiB"));
            }
            if !safe(&release.executable) {
                return Err(problem("executable must be a safe file name"));
            }
            match &release.packaging {
                ReleasePackaging::Executable => {
                    if release.download_sha256 != release.executable_sha256
                        || release.download_bytes != release.executable_bytes
                    {
                        return Err(problem(
                            "a direct executable download must equal the executable",
                        ));
                    }
                }
                ReleasePackaging::ZipEntry { entry } => {
                    if !safe(entry) {
                        return Err(problem("archive entry must be a safe file name"));
                    }
                }
            }
            if release
                .signer
                .as_deref()
                .is_some_and(|signer| !bounded(signer, 512))
            {
                return Err(problem("signer must be bounded text"));
            }
        }
        // A bundle re-signs yt-dlp and needs its content pin; Deno keeps its
        // publisher's Developer ID signature, which every launch checks.
        if self.helpers[0].content_sha256.is_none() {
            return Err(invalid("yt-dlp needs a Mach-O content pin"));
        }
        if self.helpers[1].signer.is_none() {
            return Err(invalid("deno needs a signer requirement"));
        }
        Ok(())
    }

    pub fn release(&self, name: &str) -> Option<&HelperRelease> {
        self.helpers.iter().find(|release| release.name == name)
    }

    /// Helpers older than this build's baseline, as `name version < baseline`.
    pub fn below_baseline(&self) -> Vec<String> {
        BUNDLE
            .iter()
            .filter_map(|pin| {
                let release = self.release(pin.name)?;
                compare_versions(&release.version, pin.version)
                    .is_some_and(|ordering| ordering.is_lt())
                    .then(|| {
                        format!(
                            "{} {} < baseline {}",
                            pin.name, release.version, pin.version
                        )
                    })
            })
            .collect()
    }

    /// Why this build cannot use the manifest, if it cannot.
    fn incompatibility(&self) -> Option<String> {
        if app_satisfies(&self.min_app_version) != Some(true) {
            return Some(format!(
                "it requires Deadpan {} or later (this build is {})",
                self.min_app_version,
                deadpan_models::updates::APP_VERSION
            ));
        }
        if self.platform != PLATFORM {
            return Some(format!("it is for {}, not {PLATFORM}", self.platform));
        }
        None
    }
}

/// The baseline versions this build ships, used to scope an explicit
/// below-baseline acceptance.
fn baseline_versions() -> Vec<String> {
    BUNDLE
        .iter()
        .map(|pin| format!("{} {}", pin.name, pin.version))
        .collect()
}

/// Which helper set is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selected {
    Baseline,
    Update { serial: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateState {
    pub schema: u32,
    pub active: Selected,
    pub previous: Option<Selected>,
    /// The highest serial ever installed here; lower serials are replays.
    pub highest_serial: u64,
    /// The baseline against which the owner explicitly accepted an active
    /// update older than it. A different baseline supersedes that choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub below_baseline_accepted: Option<Vec<String>>,
    /// An active serial below the highest retained one that the owner chose
    /// explicitly (`--allow-downgrade` or `rollback`). Any other older active
    /// serial is treated as a replay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older_serial_accepted: Option<u64>,
}

impl UpdateState {
    fn initial() -> Self {
        Self {
            schema: STATE_SCHEMA,
            active: Selected::Baseline,
            previous: None,
            highest_serial: 0,
            below_baseline_accepted: None,
            older_serial_accepted: None,
        }
    }
}

/// Refuse a directory another local user could change, or a link in its
/// place. A missing directory is fine.
fn private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let user = rustix::process::getuid().as_raw();
    if !metadata.file_type().is_dir()
        || (metadata.uid() != user && metadata.uid() != 0)
        || metadata.mode() & 0o022 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is not a private directory of this user", path.display()),
        ));
    }
    Ok(())
}

fn create_private(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    private_directory(path)
}

/// The highest serial among retained envelopes these keys verify. A retained
/// envelope exists only for an update whose probe passed, so this is a replay
/// floor that survives a deleted or edited `state.json`.
pub fn retained_floor(root: &Path, keys: &[TrustedKey]) -> u64 {
    let directory = updates_directory(root).join(MANIFESTS);
    if private_directory(&updates_directory(root)).is_err()
        || private_directory(&directory).is_err()
    {
        return 0;
    }
    let Ok(entries) = fs::read_dir(&directory) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let serial: u64 = entry
                .file_name()
                .to_str()?
                .strip_suffix(".json")?
                .parse()
                .ok()?;
            let bytes = read_regular(&entry.path()).ok()?;
            let envelope = SignedManifest::parse(&bytes).ok()?;
            let manifest =
                DownloaderManifest::parse(envelope.verify_with(UpdateKind::Downloader, keys).ok()?)
                    .ok()?;
            (manifest.serial == serial).then_some(serial)
        })
        .max()
        .unwrap_or(0)
}

/// Whether `bytes` are a downloader envelope these keys verify.
fn verified_envelope(bytes: &[u8], keys: &[TrustedKey]) -> bool {
    SignedManifest::parse(bytes)
        .ok()
        .is_some_and(|envelope| envelope.verify_with(UpdateKind::Downloader, keys).is_ok())
}

/// Retain an envelope by temporary file and rename, replacing only a torn or
/// unverifiable file left by an interrupted earlier write.
fn retain(path: &Path, bytes: &[u8], replace: bool) -> Result<(), CliError> {
    let directory = path.parent().expect("manifest directory");
    create_private(directory)?;
    let temporary = directory.join(format!(".retain.{}", std::process::id()));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    let renamed = if replace {
        fs::rename(&temporary, path)
    } else {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            &temporary,
            rustix::fs::CWD,
            path,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(io::Error::from)
    };
    if let Err(error) = renamed {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn updates_directory(root: &Path) -> PathBuf {
    root.join(UPDATES)
}

fn manifest_path(root: &Path, serial: u64) -> PathBuf {
    updates_directory(root)
        .join(MANIFESTS)
        .join(format!("{serial}.json"))
}

/// The recorded selection, `None` when no update was ever installed here.
pub fn state(root: &Path) -> Result<Option<UpdateState>, CliError> {
    let path = updates_directory(root).join(STATE);
    private_directory(&updates_directory(root)).map_err(|error| {
        helper_error(
            "DownloaderHelperInvalid",
            format!("downloader update state: {error}"),
        )
    })?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let damaged = |reason: String| {
        helper_error(
            "DownloaderHelperInvalid",
            format!("downloader update state {}: {reason}", path.display()),
        )
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_STATE_BYTES {
        return Err(damaged("not a bounded regular file".into()));
    }
    let mut bytes = Vec::new();
    File::open(&path)?
        .take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let state: UpdateState =
        serde_json::from_slice(&bytes).map_err(|error| damaged(error.to_string()))?;
    if state.schema != STATE_SCHEMA {
        return Err(damaged(format!("unsupported schema {}", state.schema)));
    }
    Ok(Some(state))
}

/// Replace the state file atomically: write, sync, rename, sync directory.
fn write_state(root: &Path, state: &UpdateState) -> Result<(), CliError> {
    let directory = updates_directory(root);
    create_private(&directory)?;
    let temporary = directory.join(format!(".{STATE}.{}", std::process::id()));
    let mut bytes = serde_json::to_vec_pretty(state)?;
    bytes.push(b'\n');
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    fs::rename(&temporary, directory.join(STATE))?;
    File::open(&directory)?.sync_all()?;
    Ok(())
}

/// One updater per root, across the app and the CLI.
fn lock(root: &Path) -> Result<File, CliError> {
    let directory = updates_directory(root);
    create_private(&directory)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join(".lock"))?;
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(file),
        Err(rustix::io::Errno::WOULDBLOCK) => Err(helper_error(
            "DownloaderUpdateBusy",
            "another downloader update or rollback is running",
        )),
        Err(error) => Err(io::Error::from(error).into()),
    }
}

/// A verified signed update usable by this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveUpdate {
    pub root: PathBuf,
    pub serial: u64,
    pub manifest: DownloaderManifest,
}

impl ActiveUpdate {
    pub fn release(&self, name: &str) -> Result<&HelperRelease, CliError> {
        self.manifest.release(name).ok_or_else(|| {
            helper_error(
                "DownloaderHelperInvalid",
                format!("downloader update {} has no {name}", self.serial),
            )
        })
    }

    /// Exact size and SHA-256 with private-directory checks, then the code
    /// signature (against the release's signer requirement when it has one).
    pub fn verified(&self, name: &str) -> Result<Option<PathBuf>, CliError> {
        let release = self.release(name)?;
        let Some(path) = release.verified(&self.root)? else {
            return Ok(None);
        };
        super::helpers::signing::verify(&path, release.signer.as_deref())?;
        Ok(Some(path))
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "serial": self.serial,
            "issued": self.manifest.issued,
            "ejs_version": self.manifest.ejs_version,
            "helpers": self.manifest.helpers.iter().map(|release| {
                serde_json::json!({ "name": release.name, "version": release.version })
            }).collect::<Vec<_>>(),
        })
    }
}

/// The helpers to use and, when an installed update was passed over, why.
#[derive(Debug, Clone)]
pub struct Selection {
    pub source: HelperSource,
    pub note: Option<String>,
}

enum Loaded {
    Usable(ActiveUpdate),
    Incompatible(String),
}

/// Load and verify a retained update. Integrity failures are errors; a
/// manifest this build cannot use is `Incompatible`.
fn load(
    root: &Path,
    serial: u64,
    state: &UpdateState,
    keys: &[TrustedKey],
) -> Result<Loaded, CliError> {
    let path = manifest_path(root, serial);
    let bytes = match read_regular(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(helper_error(
                "DownloaderHelperInvalid",
                format!(
                    "the active downloader update {serial} lost its signed manifest {}; run `deadpan-cli downloader rollback`",
                    path.display()
                ),
            ));
        }
        Err(error) => return Err(error.into()),
    };
    let envelope = SignedManifest::parse(&bytes).map_err(|error| integrity(serial, &error))?;
    let payload = match envelope.verify_with(UpdateKind::Downloader, keys) {
        Ok(payload) => payload,
        Err(UpdateError::UnknownKey(key)) => {
            return Ok(Loaded::Incompatible(format!(
                "this build no longer trusts its signing key {key}"
            )));
        }
        Err(error) => return Err(integrity(serial, &error)),
    };
    let manifest = DownloaderManifest::parse(payload)?;
    if manifest.serial != serial {
        return Err(helper_error(
            "DownloaderHelperInvalid",
            format!(
                "retained manifest {serial} names serial {}",
                manifest.serial
            ),
        ));
    }
    if let Some(reason) = manifest.incompatibility() {
        return Ok(Loaded::Incompatible(reason));
    }
    let below = manifest.below_baseline();
    if !below.is_empty()
        && state.below_baseline_accepted.as_deref() != Some(baseline_versions().as_slice())
    {
        return Ok(Loaded::Incompatible(format!(
            "this build's baseline is newer ({})",
            below.join(", ")
        )));
    }
    let floor = retained_floor(root, keys);
    if serial < floor && state.older_serial_accepted != Some(serial) {
        return Ok(Loaded::Incompatible(format!(
            "newer signed update {floor} is installed here, so selecting {serial} looks like a replay; use `downloader rollback` to choose it explicitly"
        )));
    }
    Ok(Loaded::Usable(ActiveUpdate {
        root: root.to_owned(),
        serial,
        manifest,
    }))
}

fn integrity(serial: u64, error: &UpdateError) -> CliError {
    helper_error(
        "DownloaderHelperInvalid",
        format!(
            "the active downloader update {serial} failed verification ({error}); run `deadpan-cli downloader rollback`"
        ),
    )
}

fn read_regular(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > deadpan_models::updates::MAX_SIGNED_BYTES
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not a bounded regular file", path.display()),
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(deadpan_models::updates::MAX_SIGNED_BYTES + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The helper set to use under `root`: its compatible active update,
/// otherwise `baseline`.
pub fn select(root: &Path, baseline: HelperSource) -> Result<Selection, CliError> {
    select_with_keys(root, baseline, &trusted_keys())
}

/// [`select`] against explicit trusted keys.
pub fn select_with_keys(
    root: &Path,
    baseline: HelperSource,
    keys: &[TrustedKey],
) -> Result<Selection, CliError> {
    let Some(state) = state(root)? else {
        return Ok(Selection {
            source: baseline,
            note: None,
        });
    };
    let Selected::Update { serial } = state.active else {
        return Ok(Selection {
            source: baseline,
            note: None,
        });
    };
    Ok(match load(root, serial, &state, keys)? {
        Loaded::Usable(update) => Selection {
            source: HelperSource::Update(Box::new(update)),
            note: None,
        },
        Loaded::Incompatible(reason) => Selection {
            source: baseline,
            note: Some(format!(
                "installed downloader update {serial} is not used because {reason}; the baseline is used"
            )),
        },
    })
}

/// Options for [`update`].
#[derive(Debug, Clone, Copy)]
pub struct UpdateOptions<'a> {
    /// Keys that verify the envelope: [`trusted_keys`] outside tests.
    pub keys: &'a [TrustedKey],
    /// Permit helpers older than this build's baseline, or a serial lower
    /// than one already installed.
    pub allow_downgrade: bool,
}

/// What an update did.
#[derive(Debug, Serialize)]
pub struct UpdateOutcome {
    pub serial: u64,
    pub previous: Selected,
    pub helpers: Vec<serde_json::Value>,
    pub probe: ProbeReport,
}

/// Verify, install, probe and activate one signed downloader update.
///
/// `probe` runs the new helper set (production: [`super::acquire::probe`])
/// and must report the manifest's versions; otherwise nothing is activated.
pub fn update(
    root: &Path,
    signed: &[u8],
    options: UpdateOptions<'_>,
    transport: &dyn Transport,
    cancelled: &AtomicBool,
    probe: &dyn Fn(&Helpers) -> Result<ProbeReport, CliError>,
    events: &mut dyn FnMut(serde_json::Value),
) -> Result<UpdateOutcome, CliError> {
    let envelope =
        SignedManifest::parse(signed).map_err(crate::update_signing::verification_error)?;
    let payload = envelope
        .verify_with(UpdateKind::Downloader, options.keys)
        .map_err(crate::update_signing::verification_error)?;
    let manifest = DownloaderManifest::parse(payload)?;
    if let Some(reason) = manifest.incompatibility() {
        return Err(helper_error(
            "UpdateIncompatible",
            format!(
                "downloader update {} cannot be used because {reason}",
                manifest.serial
            ),
        ));
    }
    let _lock = lock(root)?;
    let current = state(root)?.unwrap_or_else(UpdateState::initial);
    let below = manifest.below_baseline();
    if !below.is_empty() && !options.allow_downgrade {
        return Err(helper_error(
            "UpdateDowngrade",
            format!(
                "downloader update {} is older than this build's baseline ({}); pass --allow-downgrade to use it anyway",
                manifest.serial,
                below.join(", ")
            ),
        ));
    }
    let retained = manifest_path(root, manifest.serial);
    // `Some(replace)` when the envelope must still be written.
    let write = match read_regular(&retained) {
        Ok(bytes) if bytes == signed => None,
        Ok(bytes) if verified_envelope(&bytes, options.keys) => {
            return Err(helper_error(
                "UpdateManifestInvalid",
                format!(
                    "a different signed manifest with serial {} is already installed",
                    manifest.serial
                ),
            ));
        }
        // A torn or unverifiable file from an interrupted write.
        Ok(_) => Some(true),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => Some(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Some(false),
        Err(error) => return Err(error.into()),
    };
    // Even an identical retained envelope is a replay when a newer serial was
    // installed, whether `state.json` remembers it or only the retained
    // envelopes do; `rollback` is the explicit way back.
    let floor = current
        .highest_serial
        .max(retained_floor(root, options.keys));
    if manifest.serial < floor && !options.allow_downgrade {
        return Err(helper_error(
            "UpdateDowngrade",
            format!(
                "serial {} is older than installed serial {floor}; use `downloader rollback`, or pass --allow-downgrade to install it anyway",
                manifest.serial
            ),
        ));
    }
    events(serde_json::json!({
        "event": "verified", "serial": manifest.serial, "key_id": envelope.key_id,
        "issued": manifest.issued,
    }));
    let mut helpers = Vec::new();
    for release in &manifest.helpers {
        events(serde_json::json!({
            "event": "installing", "name": release.name, "version": release.version,
            "bytes": release.download_bytes, "url": release.url,
        }));
        let published =
            super::helpers::install_release(release, root, transport, cancelled, |done| {
                events(serde_json::json!({
                    "event": "progress", "name": release.name,
                    "completed_bytes": done, "total_bytes": release.download_bytes,
                }));
            })?;
        helpers.push(serde_json::json!({
            "name": release.name, "version": release.version,
            "path": release.path(root), "already_installed": !published,
        }));
    }
    let update = ActiveUpdate {
        root: root.to_owned(),
        serial: manifest.serial,
        manifest: manifest.clone(),
    };
    let resolved = Helpers::resolve_source(&HelperSource::Update(Box::new(update)))?;
    events(serde_json::json!({ "event": "probing", "serial": manifest.serial }));
    let report = probe(&resolved)?;
    if !report.matches_pins {
        return Err(helper_error(
            "UpdateProbeFailed",
            format!(
                "the updated helpers did not report the manifest's versions (yt-dlp {:?}, ejs {:?}, deno {:?}); the previous selection stays active",
                report.yt_dlp, report.ejs, report.deno
            ),
        ));
    }
    // Retained only once its probe passed, so a failed update never
    // blocks a corrected manifest with the same serial.
    if let Some(replace) = write {
        retain(&retained, signed, replace)?;
    }
    let next = UpdateState {
        schema: STATE_SCHEMA,
        active: Selected::Update {
            serial: manifest.serial,
        },
        previous: (current.active
            != Selected::Update {
                serial: manifest.serial,
            })
        .then_some(current.active)
        .or(current.previous),
        highest_serial: floor.max(manifest.serial),
        // An explicit below-baseline acceptance also keeps an older previous
        // selection usable for rollback.
        below_baseline_accepted: if below.is_empty() {
            current.below_baseline_accepted
        } else {
            Some(baseline_versions())
        },
        older_serial_accepted: (manifest.serial < floor).then_some(manifest.serial),
    };
    write_state(root, &next)?;
    events(serde_json::json!({ "event": "activated", "serial": manifest.serial }));
    Ok(UpdateOutcome {
        serial: manifest.serial,
        previous: current.active,
        helpers,
        probe: report,
    })
}

/// What a rollback selected, and the damaged state it replaced, if any.
#[derive(Debug, Serialize)]
pub struct RollbackOutcome {
    pub state: UpdateState,
    /// An unreadable `state.json` that `rollback --baseline` replaced.
    pub recovered: Option<String>,
}

/// Switch back to the previous selection, or to the baseline. The target is
/// verified completely first; nothing is deleted. `to_baseline` also
/// recovers from an unreadable `state.json`, reporting what it replaced.
pub fn rollback(
    root: &Path,
    baseline: &HelperSource,
    keys: &[TrustedKey],
    to_baseline: bool,
) -> Result<RollbackOutcome, CliError> {
    let _lock = lock(root)?;
    let (current, recovered) = match state(root) {
        Ok(Some(current)) => (current, None),
        Ok(None) => {
            return Err(helper_error(
                "DownloaderNoPrevious",
                "no downloader update was ever installed here; the baseline is already active",
            ));
        }
        Err(error) if to_baseline => (UpdateState::initial(), Some(error.to_string())),
        Err(error) => return Err(error),
    };
    let target = if to_baseline {
        Selected::Baseline
    } else {
        current.previous.ok_or_else(|| {
            helper_error(
                "DownloaderNoPrevious",
                "there is no previous downloader selection",
            )
        })?
    };
    if target == current.active && recovered.is_none() {
        return Err(helper_error(
            "DownloaderNoPrevious",
            "that selection is already active",
        ));
    }
    let floor = current.highest_serial.max(retained_floor(root, keys));
    let older_serial_accepted = match target {
        Selected::Baseline => {
            Helpers::resolve_source(baseline)?;
            current.older_serial_accepted
        }
        Selected::Update { serial } => {
            // A rollback is an explicit choice of an older serial.
            let chosen = (serial < floor).then_some(serial);
            let probe_state = UpdateState {
                active: target,
                older_serial_accepted: chosen,
                ..current.clone()
            };
            match load(root, serial, &probe_state, keys)? {
                Loaded::Usable(update) => {
                    Helpers::resolve_source(&HelperSource::Update(Box::new(update)))?;
                }
                Loaded::Incompatible(reason) => {
                    return Err(helper_error(
                        "UpdateIncompatible",
                        format!("downloader update {serial} cannot be used because {reason}"),
                    ));
                }
            }
            chosen
        }
    };
    let next = UpdateState {
        schema: STATE_SCHEMA,
        active: target,
        previous: if recovered.is_some() {
            None
        } else {
            Some(current.active)
        },
        highest_serial: floor,
        below_baseline_accepted: current.below_baseline_accepted,
        older_serial_accepted,
    };
    if recovered.is_some() {
        // Replace the unreadable file (a link or directory included).
        let path = updates_directory(root).join(STATE);
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
            fs::remove_dir_all(&path)?;
        }
    }
    write_state(root, &next)?;
    Ok(RollbackOutcome {
        state: next,
        recovered,
    })
}

/// Status JSON for `downloader status` and `doctor`.
pub fn report(root: &Path, selection: &Selection) -> serde_json::Value {
    let state = state(root);
    serde_json::json!({
        "root": root,
        "state": match &state {
            Ok(state) => serde_json::to_value(state).unwrap_or_default(),
            Err(error) => serde_json::json!({ "problem": error.to_string() }),
        },
        "active": match &selection.source {
            HelperSource::Update(update) => update.summary(),
            _ => serde_json::json!("baseline"),
        },
        "note": selection.note,
    })
}

#[cfg(test)]
mod tests;
