//! Per-user backup cadence and retention settings shared by the CLI and app.
//!
//! The file lives beside other managed application settings, independent of
//! any project package. A missing file means defaults. Invalid or unreadable
//! files are errors so callers can preserve existing backups rather than
//! pruning with a policy they cannot trust.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use deadpan_store::backups::BackupPolicy;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const FILE_NAME: &str = "backups.json";
const MAX_FILE_BYTES: u64 = 4096;
const SCHEMA: u32 = 1;

pub const MIN_INTERVAL_MINUTES: u32 = 1;
pub const MAX_INTERVAL_MINUTES: u32 = 1440;
pub const MIN_BACKUP_COUNT: u32 = 8;
pub const MAX_BACKUP_COUNT: u32 = 256;
pub const MIN_BUDGET_MIB: u32 = 256;
pub const MAX_BUDGET_MIB: u32 = 65_536;

/// The validated settings a person can change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    interval_minutes: u32,
    max_count: u32,
    budget_mib: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSettings {
    schema: u32,
    interval_minutes: u32,
    max_count: u32,
    budget_mib: u32,
}

impl Settings {
    pub fn new(
        interval_minutes: u32,
        max_count: u32,
        budget_mib: u32,
    ) -> Result<Self, SettingsError> {
        if !(MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES).contains(&interval_minutes) {
            return Err(SettingsError::InvalidValue(format!(
                "interval_minutes must be {MIN_INTERVAL_MINUTES}..={MAX_INTERVAL_MINUTES}"
            )));
        }
        if !(MIN_BACKUP_COUNT..=MAX_BACKUP_COUNT).contains(&max_count) {
            return Err(SettingsError::InvalidValue(format!(
                "max_count must be {MIN_BACKUP_COUNT}..={MAX_BACKUP_COUNT}"
            )));
        }
        if !(MIN_BUDGET_MIB..=MAX_BUDGET_MIB).contains(&budget_mib) {
            return Err(SettingsError::InvalidValue(format!(
                "budget_mib must be {MIN_BUDGET_MIB}..={MAX_BUDGET_MIB}"
            )));
        }
        Ok(Self {
            interval_minutes,
            max_count,
            budget_mib,
        })
    }

    pub fn interval_minutes(&self) -> u32 {
        self.interval_minutes
    }

    pub fn max_count(&self) -> u32 {
        self.max_count
    }

    pub fn budget_mib(&self) -> u32 {
        self.budget_mib
    }

    /// The regular application policy, retaining the existing age and reason
    /// buckets while applying the user's cadence and hard caps.
    pub fn policy(&self) -> BackupPolicy {
        BackupPolicy {
            interval: Duration::from_secs(u64::from(self.interval_minutes) * 60),
            max_count: self.max_count as usize,
            max_total_bytes: u64::from(self.budget_mib) * 1024 * 1024,
            ..BackupPolicy::default()
        }
    }

    /// Safe fallback while settings cannot be read. It keeps the default
    /// cadence but makes the retention union include every existing backup.
    pub fn policy_without_pruning(&self) -> BackupPolicy {
        BackupPolicy {
            keep_recent: usize::MAX,
            keep_safety: usize::MAX,
            keep_manual: usize::MAX,
            max_count: usize::MAX,
            max_total_bytes: u64::MAX,
            ..self.policy()
        }
    }

    fn file_settings(&self) -> FileSettings {
        FileSettings {
            schema: SCHEMA,
            interval_minutes: self.interval_minutes,
            max_count: self.max_count,
            budget_mib: self.budget_mib,
        }
    }

    pub fn load_current() -> Result<Loaded, SettingsError> {
        Self::load_from(&default_path()?)
    }

    pub fn load_from(path: &Path) -> Result<Loaded, SettingsError> {
        // Open without following links or waiting on special files, then
        // inspect that descriptor. A path check before open can race a FIFO.
        let file = match rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        ) {
            Ok(descriptor) => File::from(descriptor),
            Err(rustix::io::Errno::NOENT) => {
                return Ok(Loaded {
                    settings: Self::default(),
                    source: Source::Default,
                });
            }
            Err(source) => {
                return Err(SettingsError::Io {
                    path: path.to_path_buf(),
                    source: source.into(),
                });
            }
        };
        let metadata = file.metadata().map_err(|source| SettingsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if !metadata.file_type().is_file() {
            return Err(SettingsError::InvalidFile {
                path: path.to_path_buf(),
                reason: "settings path is not a regular file".into(),
            });
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(SettingsError::InvalidFile {
                path: path.to_path_buf(),
                reason: format!("settings file exceeds {MAX_FILE_BYTES} bytes"),
            });
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| SettingsError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(SettingsError::InvalidFile {
                path: path.to_path_buf(),
                reason: format!("settings file exceeds {MAX_FILE_BYTES} bytes"),
            });
        }
        let settings: Settings =
            serde_json::from_slice(&bytes).map_err(|error| SettingsError::InvalidFile {
                path: path.to_path_buf(),
                reason: error.to_string(),
            })?;
        Ok(Loaded {
            settings,
            source: Source::File,
        })
    }

    pub fn save_current(&self) -> Result<SaveOutcome, SettingsError> {
        self.save_to(&default_path()?)
    }

    /// Replace `path` atomically. A directory-sync failure is reported as a
    /// warning because the rename already made the new settings visible.
    pub fn save_to(&self, path: &Path) -> Result<SaveOutcome, SettingsError> {
        let parent = path.parent().ok_or_else(|| SettingsError::InvalidFile {
            path: path.to_path_buf(),
            reason: "settings path has no parent directory".into(),
        })?;
        fs::create_dir_all(parent).map_err(|source| SettingsError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        let file_name = path
            .file_name()
            .ok_or_else(|| SettingsError::InvalidFile {
                path: path.to_path_buf(),
                reason: "settings path has no file name".into(),
            })?
            .to_string_lossy();
        let temporary = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
        let result = self.write_replace(path, &temporary);
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn write_replace(&self, path: &Path, temporary: &Path) -> Result<SaveOutcome, SettingsError> {
        let bytes = serde_json::to_vec(&self.file_settings()).map_err(|error| {
            SettingsError::InvalidValue(format!("could not encode settings: {error}"))
        })?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(temporary)
            .map_err(|source| SettingsError::Io {
                path: temporary.to_path_buf(),
                source,
            })?;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|source| SettingsError::Io {
                path: temporary.to_path_buf(),
                source,
            })?;
        file.write_all(&bytes).map_err(|source| SettingsError::Io {
            path: temporary.to_path_buf(),
            source,
        })?;
        file.sync_all().map_err(|source| SettingsError::Io {
            path: temporary.to_path_buf(),
            source,
        })?;
        drop(file);
        fs::rename(temporary, path).map_err(|source| SettingsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let warning = File::open(path.parent().expect("parent checked"))
            .and_then(|directory| directory.sync_all())
            .err()
            .map(|error| {
                format!("settings were saved, but the folder could not be synchronized: {error}")
            });
        Ok(SaveOutcome { warning })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_minutes: 15,
            max_count: 48,
            budget_mib: 4096,
        }
    }
}

impl Serialize for Settings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.file_settings().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let file = FileSettings::deserialize(deserializer)?;
        if file.schema != SCHEMA {
            return Err(serde::de::Error::custom(format!(
                "unsupported backup settings schema {}",
                file.schema
            )));
        }
        Self::new(file.interval_minutes, file.max_count, file.budget_mib)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    pub settings: Settings,
    pub source: Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Default,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveOutcome {
    /// None means the parent directory was synchronized after replacement.
    pub warning: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("Could not access backup settings at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Invalid backup settings at {path}: {reason}")]
    InvalidFile { path: PathBuf, reason: String },
    #[error("Invalid backup settings: {0}")]
    InvalidValue(String),
    #[error("HOME must name an absolute directory to locate backup settings")]
    Home,
}

pub fn default_path() -> Result<PathBuf, SettingsError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or(SettingsError::Home)?;
    Ok(home
        .join("Library/Application Support/Deadpan")
        .join(FILE_NAME))
}

#[cfg(test)]
mod tests;
