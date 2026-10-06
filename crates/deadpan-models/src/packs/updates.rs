//! Signed model-pack updates, the active-version pointer and rollback.
//!
//! Approved manifests compiled into the app remain the baseline. A signed
//! update ([`crate::updates::SignedManifest`] of kind `model-pack`) carries a
//! [`PackUpdate`]: a serial, the minimum app version and one complete schema-2
//! [`PackManifest`] for a pack family this build already knows, with the same
//! runtime and a runtime version this build ships. The store retains the exact
//! envelope under `.updates/<pack>/<version>.json` and re-verifies it against
//! the compiled keys whenever it builds its catalog.
//!
//! Versions install side by side as `<pack>/<version>`. The host installs an
//! update's version through the ordinary stage → smoke test → activate path
//! and only then records it in `.active/<pack>.json`, which names the active
//! version and the previous one. Nothing is deleted: the previous version
//! stays installed until the owner removes it, and [`PackStore::rollback`]
//! switches back after checking that it is still installed. Without a pointer
//! (or when its version is gone or no longer admissible) the compiled approved
//! version is selected, as before updates existed.

use std::cmp::Ordering;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{InstalledPack, Operation, PackError, PackManifest, PackStore, approved_pack};
use crate::updates::{
    MAX_SIGNED_BYTES, SignedManifest, TrustedKey, UpdateError, UpdateKind, app_satisfies,
    compare_versions, trusted_keys,
};

pub const UPDATE_SCHEMA: u32 = 1;
const POINTER_SCHEMA: u32 = 1;
const UPDATES: &str = ".updates";
const ACTIVE: &str = ".active";
const MAX_POINTER_BYTES: u64 = 4096;

fn refuse(code: &'static str, message: impl Into<String>) -> PackError {
    PackError::Update {
        code,
        message: message.into(),
    }
}

fn verification(error: UpdateError) -> PackError {
    let code = match error {
        UpdateError::UnknownKey(_) => "UpdateUntrusted",
        UpdateError::BadSignature | UpdateError::WrongKind { .. } => "UpdateSignatureInvalid",
        UpdateError::Malformed(_) | UpdateError::Key(_) => "UpdateManifestInvalid",
    };
    refuse(code, error.to_string())
}

/// The payload of a signed model-pack update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackUpdate {
    pub schema: u32,
    pub serial: u64,
    /// `YYYY-MM-DD`.
    pub issued: String,
    pub min_app_version: String,
    pub pack: PackManifest,
}

impl PackUpdate {
    /// Parse and validate structure and family compatibility.
    pub fn parse(text: &str) -> Result<Self, PackError> {
        let update: Self = serde_json::from_str(text)
            .map_err(|error| refuse("UpdateManifestInvalid", error.to_string()))?;
        update.validate()?;
        Ok(update)
    }

    fn validate(&self) -> Result<(), PackError> {
        let invalid = |reason: &str| Err(refuse("UpdateManifestInvalid", reason.to_owned()));
        if self.schema != UPDATE_SCHEMA {
            return invalid("unsupported pack update schema");
        }
        if self.serial == 0 {
            return invalid("serial must be positive");
        }
        let date = self.issued.as_bytes();
        if date.len() != 10
            || !date.iter().enumerate().all(|(at, byte)| {
                if at == 4 || at == 7 {
                    *byte == b'-'
                } else {
                    byte.is_ascii_digit()
                }
            })
        {
            return invalid("issued must be YYYY-MM-DD");
        }
        if app_satisfies(&self.min_app_version).is_none() {
            return invalid("min_app_version must be a dotted numeric version");
        }
        self.pack.validate()?;
        if compare_versions(&self.pack.pack_version, "0").is_none() {
            return invalid("pack_version must be a dotted numeric version");
        }
        let Some(baseline) = approved_pack(&self.pack.pack_id) else {
            return Err(refuse(
                "UpdateIncompatible",
                format!(
                    "{} is not a model pack this build knows; new pack families need an app update",
                    self.pack.pack_id
                ),
            ));
        };
        if self.pack.runtime_id != baseline.runtime_id
            || !self
                .pack
                .runtime_versions
                .iter()
                .any(|version| baseline.runtime_versions.contains(version))
        {
            return Err(refuse(
                "UpdateIncompatible",
                format!(
                    "{} {} needs runtime {} {:?}; this build ships {} {:?}",
                    self.pack.pack_id,
                    self.pack.pack_version,
                    self.pack.runtime_id,
                    self.pack.runtime_versions,
                    baseline.runtime_id,
                    baseline.runtime_versions
                ),
            ));
        }
        if self.pack.supports(Operation::BridgeHold) || baseline.supports(Operation::BridgeHold) {
            // The AI pause worker verifies its own compiled weight receipt.
            return Err(refuse(
                "UpdateIncompatible",
                "the AI pause runtime verifies its compiled weight receipt; bridge packs change only with an app update",
            ));
        }
        if self
            .pack
            .operations
            .iter()
            .any(|operation| !baseline.operations.contains(operation))
        {
            return Err(refuse(
                "UpdateIncompatible",
                "an update cannot add operations this build does not run for the pack",
            ));
        }
        Ok(())
    }

    fn incompatibility(&self) -> Option<String> {
        (app_satisfies(&self.min_app_version) != Some(true)).then(|| {
            format!(
                "it requires Deadpan {} or later (this build is {})",
                self.min_app_version,
                crate::updates::APP_VERSION
            )
        })
    }
}

/// Verify a signed pack update's envelope and structure, for showing its
/// size and licenses before anything is admitted. Not an admission.
pub fn inspect_update(signed: &[u8], keys: &[TrustedKey]) -> Result<PackUpdate, PackError> {
    let envelope = SignedManifest::parse(signed).map_err(verification)?;
    let payload = envelope
        .verify_with(UpdateKind::ModelPack, keys)
        .map_err(verification)?;
    PackUpdate::parse(payload)
}

/// Which version of one pack is active, and the one before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivePointer {
    pub schema: u32,
    pub version: String,
    pub previous: Option<String>,
}

fn read_bounded(path: &Path, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not a bounded regular file", path.display()),
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

/// Refuse a directory another local user could change, or a link in its
/// place. A missing directory is fine; it is created owner-private.
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
    match fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
    {
        Ok(()) => private_directory(path),
        Err(error) => Err(error),
    }
}

/// Whether `bytes` are a model-pack envelope these keys verify.
fn verified_envelope(bytes: &[u8], keys: &[TrustedKey]) -> bool {
    SignedManifest::parse(bytes)
        .ok()
        .is_some_and(|envelope| envelope.verify_with(UpdateKind::ModelPack, keys).is_ok())
}

/// Retain an envelope by temporary file and rename. An identical file is
/// kept; a different verified one is refused; a torn or unverifiable file
/// (an interrupted earlier write) is replaced.
fn retain(path: &Path, bytes: &[u8], keys: &[TrustedKey]) -> Result<(), PackError> {
    let directory = path.parent().ok_or(PackError::Manifest("no parent"))?;
    create_private(directory)?;
    let existing = match read_bounded(path, MAX_SIGNED_BYTES) {
        Ok(existing) => existing,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => Some(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if let Some(existing) = &existing {
        if existing == bytes {
            return Ok(());
        }
        if verified_envelope(existing, keys) {
            return Err(refuse(
                "UpdateManifestInvalid",
                format!(
                    "a different signed manifest is already retained at {}",
                    path.display()
                ),
            ));
        }
    }
    let temporary = directory.join(format!(".retain.{}", std::process::id()));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if existing.is_some() {
        // Replace the damaged file in one step.
        fs::rename(&temporary, path)?;
    } else if let Err(error) = rustix::fs::renameat_with(
        rustix::fs::CWD,
        &temporary,
        rustix::fs::CWD,
        path,
        rustix::fs::RenameFlags::NOREPLACE,
    ) {
        let _ = fs::remove_file(&temporary);
        return Err(io::Error::from(error).into());
    }
    File::open(directory)?.sync_all()?;
    Ok(())
}

/// Whether the pack's licenses need the user's explicit acceptance before
/// this update installs: every layer that requires it, plus any layer that
/// is new to this build or whose terms differ from the compiled pack's.
pub fn licenses_to_accept(pack: &PackManifest) -> Vec<String> {
    let baseline = approved_pack(&pack.pack_id);
    let fingerprint = |license: &super::PackLicense| {
        use sha2::Digest;
        sha2::Sha256::digest(serde_json::to_vec(license).unwrap_or_default())
    };
    pack.licenses
        .iter()
        .filter(|license| {
            license.acceptance_required
                || !baseline.as_ref().is_some_and(|baseline| {
                    baseline.licenses.iter().any(|known| {
                        known.id == license.id && fingerprint(known) == fingerprint(license)
                    })
                })
        })
        .map(|license| license.id.clone())
        .collect()
}

impl PackStore {
    fn retained_path(&self, pack_id: &str, version: &str) -> PathBuf {
        self.root
            .join(UPDATES)
            .join(pack_id)
            .join(format!("{version}.json"))
    }

    fn pointer_path(&self, pack_id: &str) -> PathBuf {
        self.root.join(ACTIVE).join(format!("{pack_id}.json"))
    }

    /// Serialize pointer changes across processes (the app and the CLI).
    fn pointer_lock(&self) -> Result<File, PackError> {
        let directory = self.root.join(ACTIVE);
        create_private(&directory)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join(".lock"))?;
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
            .map_err(io::Error::from)?;
        Ok(file)
    }

    /// The recorded active version of a pack, if an update ever set one.
    pub fn pointer(&self, pack_id: &str) -> Result<Option<ActivePointer>, PackError> {
        private_directory(&self.root.join(ACTIVE))?;
        let Some(bytes) = read_bounded(&self.pointer_path(pack_id), MAX_POINTER_BYTES)? else {
            return Ok(None);
        };
        let pointer: ActivePointer = serde_json::from_slice(&bytes)?;
        if pointer.schema != POINTER_SCHEMA {
            return Err(PackError::Manifest("unsupported active pointer schema"));
        }
        Ok(Some(pointer))
    }

    /// Replace the pointer atomically: write, sync, rename, sync directory.
    fn write_pointer(&self, pack_id: &str, pointer: &ActivePointer) -> Result<(), PackError> {
        let path = self.pointer_path(pack_id);
        let directory = path.parent().ok_or(PackError::Manifest("no parent"))?;
        create_private(directory)?;
        let temporary = directory.join(format!(".{pack_id}.{}", std::process::id()));
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(pointer)?)?;
            file.sync_all()?;
        }
        fs::rename(&temporary, &path)?;
        File::open(directory)?.sync_all()?;
        Ok(())
    }

    /// Verified retained updates this build can use, with their serials.
    fn retained_with_keys(&self, keys: &[TrustedKey]) -> Vec<(u64, PackManifest)> {
        let mut updates = Vec::new();
        let directory = self.root.join(UPDATES);
        if private_directory(&directory).is_err() {
            return updates;
        }
        let Ok(families) = fs::read_dir(directory) else {
            return updates;
        };
        for family in families.flatten() {
            if private_directory(&family.path()).is_err() {
                continue;
            }
            let Ok(entries) = fs::read_dir(family.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(Some(bytes)) = read_bounded(&entry.path(), MAX_SIGNED_BYTES) else {
                    continue;
                };
                let Ok(envelope) = SignedManifest::parse(&bytes) else {
                    continue;
                };
                let Ok(payload) = envelope.verify_with(UpdateKind::ModelPack, keys) else {
                    continue;
                };
                let Ok(update) = PackUpdate::parse(payload) else {
                    continue;
                };
                // The file name must be the manifest's own identity.
                if update.incompatibility().is_some()
                    || entry.path()
                        != self.retained_path(&update.pack.pack_id, &update.pack.pack_version)
                {
                    continue;
                }
                updates.push((update.serial, update.pack));
            }
        }
        updates
    }

    /// Every manifest this store may install or select: the compiled approved
    /// packs and the verified signed updates retained here.
    pub fn catalog(&self) -> Vec<PackManifest> {
        self.catalog_with_keys(&trusted_keys())
    }

    pub fn catalog_with_keys(&self, keys: &[TrustedKey]) -> Vec<PackManifest> {
        // Compiled packs keep their order; each family's verified updates
        // follow it in version order.
        let mut catalog = Vec::new();
        let mut retained = self.retained_with_keys(keys);
        retained.sort_by(|(_, left), (_, right)| {
            compare_versions(&left.pack_version, &right.pack_version).unwrap_or(Ordering::Equal)
        });
        for approved in super::approved_packs() {
            let family = approved.pack_id.clone();
            catalog.push(approved);
            for (_, manifest) in &retained {
                if manifest.pack_id == family
                    && !catalog.iter().any(|known| {
                        known.pack_id == manifest.pack_id
                            && known.pack_version == manifest.pack_version
                    })
                {
                    catalog.push(manifest.clone());
                }
            }
        }
        catalog
    }

    /// A specific version of a pack from the catalog.
    pub fn catalog_manifest(&self, pack_id: &str, version: &str) -> Option<PackManifest> {
        self.catalog()
            .into_iter()
            .find(|manifest| manifest.pack_id == pack_id && manifest.pack_version == version)
    }

    /// The manifest selected for a pack: the pointer's version when it is in
    /// the catalog and installed, otherwise the compiled approved version.
    pub fn selected(&self, pack_id: &str) -> Result<Option<PackManifest>, PackError> {
        self.selected_with_keys(pack_id, &trusted_keys())
    }

    pub fn selected_with_keys(
        &self,
        pack_id: &str,
        keys: &[TrustedKey],
    ) -> Result<Option<PackManifest>, PackError> {
        if let Some(pointer) = self.pointer(pack_id)?
            && let Some(manifest) = self.catalog_with_keys(keys).into_iter().find(|manifest| {
                manifest.pack_id == pack_id && manifest.pack_version == pointer.version
            })
            && self.installed(&manifest)?.is_some()
        {
            return Ok(Some(manifest));
        }
        Ok(approved_pack(pack_id))
    }

    /// Why the recorded active version is not the one in use, if it is not:
    /// its files are missing or changed size, its envelope is gone, or this
    /// build no longer trusts or accepts it. The compiled version is used.
    pub fn selection_note(&self, pack_id: &str) -> Result<Option<String>, PackError> {
        self.selection_note_with_keys(pack_id, &trusted_keys())
    }

    pub fn selection_note_with_keys(
        &self,
        pack_id: &str,
        keys: &[TrustedKey],
    ) -> Result<Option<String>, PackError> {
        let Some(pointer) = self.pointer(pack_id)? else {
            return Ok(None);
        };
        let Some(manifest) = self.catalog_with_keys(keys).into_iter().find(|manifest| {
            manifest.pack_id == pack_id && manifest.pack_version == pointer.version
        }) else {
            return Ok(Some(format!(
                "{pack_id} {} is selected but its signed update is missing, untrusted or incompatible with this build; the compiled version is used",
                pointer.version
            )));
        };
        Ok(self.installed(&manifest)?.is_none().then(|| {
            format!(
                "{pack_id} {} is selected but its files are missing or changed; the compiled version is used",
                pointer.version
            )
        }))
    }

    /// The installed selected version of a pack, for consumers.
    pub fn current(&self, pack_id: &str) -> Result<Option<InstalledPack>, PackError> {
        self.current_with_keys(pack_id, &trusted_keys())
    }

    pub fn current_with_keys(
        &self,
        pack_id: &str,
        keys: &[TrustedKey],
    ) -> Result<Option<InstalledPack>, PackError> {
        match self.selected_with_keys(pack_id, keys)? {
            Some(manifest) => self.installed(&manifest),
            None => Ok(None),
        }
    }

    /// The installed selected pack that supports `operation`, if any.
    pub fn current_for(&self, operation: Operation) -> Result<Option<InstalledPack>, PackError> {
        for manifest in super::approved_packs() {
            if !manifest.supports(operation) {
                continue;
            }
            if let Some(installed) = self.current(&manifest.pack_id)?
                && installed.manifest.supports(operation)
            {
                return Ok(Some(installed));
            }
        }
        Ok(None)
    }

    /// Verify a signed pack update and its compatibility; returns the
    /// manifest to install. Nothing is written, downloaded or selected here:
    /// [`Self::activate_update`] retains the envelope and selects the version
    /// only after the host's smoke test passed.
    ///
    /// `accepted` must name every license of [`licenses_to_accept`]: those
    /// that require acceptance and any that are new or changed against the
    /// compiled pack.
    pub fn admit_update(
        &self,
        signed: &[u8],
        keys: &[TrustedKey],
        accepted: &[String],
        allow_downgrade: bool,
    ) -> Result<PackManifest, PackError> {
        let update = inspect_update(signed, keys)?;
        if let Some(reason) = update.incompatibility() {
            return Err(refuse(
                "UpdateIncompatible",
                format!(
                    "{} {} cannot be used because {reason}",
                    update.pack.pack_id, update.pack.pack_version
                ),
            ));
        }
        let pack = &update.pack;
        if let Some(license) = licenses_to_accept(pack)
            .into_iter()
            .find(|id| !accepted.contains(id))
            .and_then(|id| pack.licenses.iter().find(|license| license.id == id))
        {
            return Err(PackError::LicenseNotAccepted {
                title: license.title.clone(),
            });
        }
        if let Some(known) = self
            .catalog_with_keys(keys)
            .into_iter()
            .find(|known| known.pack_id == pack.pack_id && known.pack_version == pack.pack_version)
            && &known != pack
        {
            return Err(refuse(
                "UpdateManifestInvalid",
                format!(
                    "{} {} is already known with different files",
                    pack.pack_id, pack.pack_version
                ),
            ));
        }
        if !allow_downgrade {
            let selected = self.selected_with_keys(&pack.pack_id, keys)?;
            if let Some(selected) = selected
                && compare_versions(&pack.pack_version, &selected.pack_version)
                    .is_none_or(Ordering::is_lt)
            {
                return Err(refuse(
                    "UpdateDowngrade",
                    format!(
                        "{} {} is older than the selected version {}; pass --allow-downgrade to install it anyway",
                        pack.pack_id, pack.pack_version, selected.pack_version
                    ),
                ));
            }
            // Every activated envelope is retained, so the highest retained
            // serial bounds replays of older signed manifests, including an
            // identical older one (rollback is the explicit way back).
            let highest = self
                .retained_with_keys(keys)
                .into_iter()
                .filter(|(_, known)| known.pack_id == pack.pack_id)
                .map(|(serial, _)| serial)
                .max()
                .unwrap_or(0);
            if update.serial < highest {
                return Err(refuse(
                    "UpdateDowngrade",
                    format!(
                        "serial {} is older than activated serial {highest}; use rollback, or pass --allow-downgrade to install it anyway",
                        update.serial
                    ),
                ));
            }
        }
        Ok(update.pack)
    }

    /// After the admitted version is installed and smoke-tested: under the
    /// pointer lock, check admission again (another process may have
    /// activated a newer serial meanwhile), check the installed files, retain
    /// the envelope and select its version, so no concurrent update or
    /// rollback can interleave between the check and the switch.
    pub fn activate_update(
        &self,
        signed: &[u8],
        keys: &[TrustedKey],
        accepted: &[String],
        allow_downgrade: bool,
    ) -> Result<InstalledPack, PackError> {
        let _lock = self.pointer_lock()?;
        let manifest = self.admit_update(signed, keys, accepted, allow_downgrade)?;
        if self.installed(&manifest)?.is_none() {
            return Err(refuse(
                "ModelPackNotInstalled",
                format!(
                    "{} {} is not installed",
                    manifest.pack_id, manifest.pack_version
                ),
            ));
        }
        retain(
            &self.retained_path(&manifest.pack_id, &manifest.pack_version),
            signed,
            keys,
        )?;
        self.select_version_locked(&manifest, keys)
    }

    /// Select an installed, smoke-tested version. The previously selected
    /// version is remembered for [`Self::rollback`] and stays installed.
    pub fn select_version(
        &self,
        manifest: &PackManifest,
        keys: &[TrustedKey],
    ) -> Result<InstalledPack, PackError> {
        let _lock = self.pointer_lock()?;
        self.select_version_locked(manifest, keys)
    }

    /// [`Self::select_version`] for a caller already holding the pointer lock.
    fn select_version_locked(
        &self,
        manifest: &PackManifest,
        keys: &[TrustedKey],
    ) -> Result<InstalledPack, PackError> {
        if !self.catalog_with_keys(keys).contains(manifest) {
            return Err(refuse(
                "UpdateIncompatible",
                format!(
                    "{} {} is not an approved or verified version",
                    manifest.pack_id, manifest.pack_version
                ),
            ));
        }
        let installed = self.installed(manifest)?.ok_or_else(|| {
            refuse(
                "ModelPackNotInstalled",
                format!(
                    "{} {} is not installed",
                    manifest.pack_id, manifest.pack_version
                ),
            )
        })?;
        // The version actually in effect before (an installed selection),
        // never a pointer to a version that is gone.
        let current = self
            .selected_with_keys(&manifest.pack_id, keys)?
            .filter(|selected| self.installed(selected).ok().flatten().is_some())
            .map(|selected| selected.pack_version);
        let previous = match current {
            Some(version) if version != manifest.pack_version => Some(version),
            _ => self
                .pointer(&manifest.pack_id)?
                .and_then(|pointer| pointer.previous)
                .filter(|previous| previous != &manifest.pack_version),
        };
        self.write_pointer(
            &manifest.pack_id,
            &ActivePointer {
                schema: POINTER_SCHEMA,
                version: manifest.pack_version.clone(),
                previous,
            },
        )?;
        Ok(installed)
    }

    /// Switch a pack back to its previous version, which must still be
    /// installed and admissible. Nothing is deleted.
    pub fn rollback(&self, pack_id: &str) -> Result<InstalledPack, PackError> {
        self.rollback_with_keys(pack_id, &trusted_keys())
    }

    pub fn rollback_with_keys(
        &self,
        pack_id: &str,
        keys: &[TrustedKey],
    ) -> Result<InstalledPack, PackError> {
        let _lock = self.pointer_lock()?;
        let pointer = self.pointer(pack_id)?.ok_or_else(|| {
            refuse(
                "ModelPackNoPrevious",
                format!("{pack_id} has no recorded previous version"),
            )
        })?;
        let previous = pointer.previous.clone().ok_or_else(|| {
            refuse(
                "ModelPackNoPrevious",
                format!("{pack_id} has no recorded previous version"),
            )
        })?;
        let manifest = self
            .catalog_with_keys(keys)
            .into_iter()
            .find(|manifest| manifest.pack_id == pack_id && manifest.pack_version == previous)
            .ok_or_else(|| {
                refuse(
                    "UpdateIncompatible",
                    format!("{pack_id} {previous} is no longer admissible in this build"),
                )
            })?;
        let installed = self.installed(&manifest)?.ok_or_else(|| {
            refuse(
                "ModelPackNotInstalled",
                format!("{pack_id} {previous} is no longer installed"),
            )
        })?;
        self.write_pointer(
            pack_id,
            &ActivePointer {
                schema: POINTER_SCHEMA,
                version: previous,
                previous: Some(pointer.version),
            },
        )?;
        Ok(installed)
    }
}

#[cfg(test)]
mod tests;
