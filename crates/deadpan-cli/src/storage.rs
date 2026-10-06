//! Storage accounting and explicit cleanup for one project and for the
//! per-user caches, plus portable project copies.
//!
//! Project accounting and cleanup are [`deadpan_store::storage`]; portable
//! copies are [`deadpan_store::portable`]. This module adds the per-user
//! caches of specification Section 20.1 and the headless commands.
//!
//! Per-user storage falls into three groups:
//!
//! - **Rebuildable caches** that cleanup may remove: seek proxies
//!   (`~/Library/Caches/Deadpan/Proxies`, through the proxy cache's own
//!   locked cleanup, which never removes an entry a reader holds) and
//!   abandoned downloader-helper staging
//!   (`~/Library/Application Support/Deadpan/helpers/.staging`).
//! - **Reported only**: everything else under `~/Library/Caches/Deadpan`
//!   (development AI runtimes and qualification weights, build inputs) and
//!   installed model packs, which `models remove` manages. Cleanup never
//!   touches them.
//! - **Not on disk**: decoded PCM, picture and thumbnail caches live in
//!   process memory or anonymous temporary files that vanish with the process.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use deadpan_store::storage::{CleanupPolicy, DEFAULT_GRACE, StorageReport};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use serde::Serialize;

use crate::proxy::cache::{DEFAULT_PROXY_BUDGET_BYTES, ProxyCache, ProxyCleanupPolicy};
use crate::{CliError, write_json};

/// The per-user directories this module accounts for.
#[derive(Debug, Clone)]
pub struct UserStorage {
    /// `~/Library/Caches/Deadpan`.
    pub caches: PathBuf,
    /// `~/Library/Application Support/Deadpan`.
    pub support: PathBuf,
}

impl UserStorage {
    /// The current user's directories, or None without an absolute HOME.
    pub fn current() -> Option<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())?;
        Some(Self {
            caches: home.join("Library/Caches/Deadpan"),
            support: home.join("Library/Application Support/Deadpan"),
        })
    }

    fn proxies(&self) -> PathBuf {
        self.caches.join("Proxies")
    }

    fn helper_staging(&self) -> PathBuf {
        self.support.join("helpers/.staging")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheDirectory {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    /// What it holds and who manages it.
    pub kind: &'static str,
    /// Cleanup may remove some of it.
    pub cleanable: bool,
    /// Bytes cleanup would remove under the report's grace period.
    pub removable_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserStorageReport {
    pub schema_version: u32,
    pub grace_seconds: u64,
    pub directories: Vec<CacheDirectory>,
    pub total_bytes: u64,
    pub removable_bytes: u64,
    /// Caches that exist only in process memory or anonymous files.
    pub not_on_disk: Vec<&'static str>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UserCleanupOutcome {
    pub dry_run: bool,
    pub removed: Vec<PathBuf>,
    pub removed_bytes: u64,
    pub kept_in_use: Vec<String>,
}

const NOT_ON_DISK: [&str; 3] = [
    "decoded source PCM (bounded, per process, anonymous temporary files)",
    "decoded pictures and thumbnails (process memory)",
    "prepared and limited audio tiles (process memory)",
];

fn age(metadata: &std::fs::Metadata) -> Duration {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .unwrap_or_default()
}

/// Bytes of a tree without following symbolic links.
pub fn tree_bytes(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| tree_bytes(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

/// Each published proxy entry's bytes and last use, without the cache lock.
fn proxy_entries(root: &Path) -> Vec<(String, u64, Duration)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if name.starts_with('.') {
                return None;
            }
            let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
            if !metadata.is_dir() {
                return None;
            }
            let used = std::fs::symlink_metadata(entry.path().join("used"))
                .map_or_else(|_| age(&metadata), |used| age(&used));
            Some((name, tree_bytes(&entry.path()), used))
        })
        .collect()
}

/// The time since anything in a tree last changed: its newest modification
/// time, without following symbolic links. A running download keeps
/// writing its file, so the directory's own time is not enough.
fn newest_age(path: &Path) -> Duration {
    fn newest(path: &Path) -> Option<SystemTime> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        let own = metadata.modified().ok();
        if !metadata.is_dir() {
            return own;
        }
        std::fs::read_dir(path)
            .ok()?
            .flatten()
            .filter_map(|entry| newest(&entry.path()))
            .chain(own)
            .max()
    }
    newest(path)
        .and_then(|newest| SystemTime::now().duration_since(newest).ok())
        .unwrap_or_default()
}

/// What the proxy cache's own cleanup would remove, in its order: entries
/// unused for the grace period, then the least recently used while the rest
/// exceeds its budget. Entries a reader holds are kept by the real pass.
fn proxy_removals(
    mut entries: Vec<(String, u64, Duration)>,
    grace: Duration,
    retained: &[String],
    budget: u64,
) -> Vec<(String, u64)> {
    entries.retain(|(name, _, _)| !retained.contains(name));
    entries.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    let mut total: u64 = entries.iter().map(|(_, bytes, _)| bytes).sum();
    let mut removed = Vec::new();
    for (name, bytes, used) in entries {
        if used >= grace || total > budget {
            total -= bytes;
            removed.push((name, bytes));
        }
    }
    removed
}

/// The smallest grace period per-user cache cleanup accepts: other Deadpan
/// processes write these caches without a lock this cleanup can observe.
pub const MIN_CACHE_GRACE: Duration = Duration::from_secs(3600);

/// Abandoned helper staging directories with nothing changed for `grace`.
fn stale_helper_staging(root: &Path, grace: Duration) -> Vec<(PathBuf, u64)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
            (metadata.is_dir() && newest_age(&entry.path()) >= grace)
                .then(|| (entry.path(), tree_bytes(&entry.path())))
        })
        .collect()
}

impl UserStorage {
    /// Account for every per-user directory. Creates and changes nothing.
    pub fn report(&self, grace: Duration) -> UserStorageReport {
        let mut directories = Vec::new();
        let proxies = self.proxies();
        let proxy_entries = proxy_entries(&proxies);
        directories.push(CacheDirectory {
            name: "Proxies".into(),
            bytes: tree_bytes(&proxies),
            path: proxies,
            kind: "rebuildable seek proxies",
            cleanable: true,
            removable_bytes: proxy_removals(proxy_entries, grace, &[], DEFAULT_PROXY_BUDGET_BYTES)
                .iter()
                .map(|(_, bytes)| bytes)
                .sum(),
        });
        let staging = self.helper_staging();
        directories.push(CacheDirectory {
            name: "Downloader staging".into(),
            bytes: tree_bytes(&staging),
            removable_bytes: stale_helper_staging(&staging, grace)
                .iter()
                .map(|(_, bytes)| bytes)
                .sum(),
            path: staging,
            kind: "abandoned downloader-helper downloads",
            cleanable: true,
        });
        if let Ok(entries) = std::fs::read_dir(&self.caches) {
            let mut others: Vec<_> = entries
                .flatten()
                .filter(|entry| entry.file_name() != "Proxies")
                .collect();
            others.sort_by_key(std::fs::DirEntry::file_name);
            for entry in others {
                let name = entry.file_name().to_string_lossy().into_owned();
                let kind = if name.starts_with("ltx-") {
                    "AI runtime or qualification weights (never cleaned)"
                } else {
                    "other per-user cache (never cleaned)"
                };
                directories.push(CacheDirectory {
                    name,
                    bytes: tree_bytes(&entry.path()),
                    path: entry.path(),
                    kind,
                    cleanable: false,
                    removable_bytes: 0,
                });
            }
        }
        let models = self.support.join("Models");
        directories.push(CacheDirectory {
            name: "Model packs".into(),
            bytes: tree_bytes(&models),
            path: models,
            kind: "installed model packs (managed by `models remove`)",
            cleanable: false,
            removable_bytes: 0,
        });
        let total_bytes = directories.iter().map(|directory| directory.bytes).sum();
        let removable_bytes = directories
            .iter()
            .map(|directory| directory.removable_bytes)
            .sum();
        UserStorageReport {
            schema_version: 1,
            grace_seconds: grace.as_secs(),
            directories,
            total_bytes,
            removable_bytes,
            not_on_disk: NOT_ON_DISK.to_vec(),
        }
    }

    /// Remove proxies unused for `grace` and helper staging older than it.
    /// `retain` names proxy entries to keep regardless of age, such as the
    /// open project's Original. Readers' entries are always kept.
    pub fn clean(
        &self,
        grace: Duration,
        dry_run: bool,
        retain: &[crate::proxy::cache::ProxyKey],
    ) -> Result<UserCleanupOutcome, CliError> {
        let mut outcome = UserCleanupOutcome {
            dry_run,
            ..UserCleanupOutcome::default()
        };
        let proxies = self.proxies();
        let retained: Vec<String> = retain
            .iter()
            .map(crate::proxy::cache::ProxyKey::directory)
            .collect();
        if grace < MIN_CACHE_GRACE {
            return Err(CliError::Usage(
                "Cache cleanup needs a grace period of at least one hour; another Deadpan process may be writing these caches.".into(),
            ));
        }
        if dry_run {
            for (name, bytes) in proxy_removals(
                proxy_entries(&proxies),
                grace,
                &retained,
                DEFAULT_PROXY_BUDGET_BYTES,
            ) {
                outcome.removed_bytes += bytes;
                outcome.removed.push(proxies.join(name));
            }
        } else if proxies.is_dir() {
            let before: Vec<_> = proxy_entries(&proxies);
            let cache = ProxyCache::at(&proxies)
                .map_err(|error| CliError::Usage(format!("proxy cache: {error}")))?;
            let report = cache
                .cleanup(
                    retain,
                    ProxyCleanupPolicy {
                        staging_grace: grace,
                        unused_grace: grace,
                        budget_bytes: DEFAULT_PROXY_BUDGET_BYTES,
                    },
                )
                .map_err(|error| CliError::Usage(format!("proxy cache: {error}")))?;
            for name in report.removed_entries {
                outcome.removed_bytes += before
                    .iter()
                    .find(|(entry, _, _)| *entry == name)
                    .map_or(0, |(_, bytes, _)| *bytes);
                outcome.removed.push(proxies.join(name));
            }
            outcome.kept_in_use = report.kept_in_use;
        }
        for (path, bytes) in stale_helper_staging(&self.helper_staging(), grace) {
            if !dry_run {
                std::fs::remove_dir_all(&path)?;
            }
            outcome.removed_bytes += bytes;
            outcome.removed.push(path);
        }
        Ok(outcome)
    }
}

/// `--grace-hours N` (default 24) followed by the remaining flags.
fn grace<'a>(arguments: &[&'a str]) -> Result<(Duration, Vec<&'a str>), CliError> {
    let mut grace = DEFAULT_GRACE;
    let mut rest = Vec::new();
    let mut iter = arguments.iter();
    while let Some(argument) = iter.next() {
        if *argument == "--grace-hours" {
            let hours: u64 = iter
                .next()
                .and_then(|value| value.parse().ok())
                .filter(|hours| *hours <= 24 * 365)
                .ok_or_else(|| CliError::Usage("--grace-hours takes 0..8760".into()))?;
            grace = Duration::from_secs(hours * 3600);
        } else {
            rest.push(*argument);
        }
    }
    Ok((grace, rest))
}

#[derive(Serialize)]
struct ProjectStorage<'a> {
    protocol: u32,
    project: &'a StorageReport,
    user: &'a UserStorageReport,
}

/// `project storage <package> [--grace-hours N]` and
/// `project storage <package> --clean [--dry-run] [--grace-hours N]`.
pub(crate) fn run_project(arguments: &[&str]) -> Result<(), CliError> {
    let (grace, rest) = grace(arguments)?;
    let user = UserStorage::current();
    match rest.as_slice() {
        [package] => {
            let store = ProjectStore::open(Path::new(package), AccessMode::ReadOnly)?;
            let project = store.storage_report(grace)?;
            let user = user.map_or_else(
                || {
                    UserStorage {
                        caches: PathBuf::new(),
                        support: PathBuf::new(),
                    }
                    .report(grace)
                },
                |user| user.report(grace),
            );
            write_json(&ProjectStorage {
                protocol: 1,
                project: &project,
                user: &user,
            })
        }
        [package, "--clean", flags @ ..] if flags.iter().all(|flag| *flag == "--dry-run") => {
            let dry_run = !flags.is_empty();
            let mut store = match ProjectStore::open(Path::new(package), AccessMode::ReadWrite) {
                Err(StoreError::AlreadyOpen) => {
                    return Err(CliError::Usage(
                        "The project is open in Deadpan. Use its Storage panel (:storage), or close it and run cleanup again.".into(),
                    ));
                }
                store => store?,
            };
            let outcome = store.clean_storage(CleanupPolicy::everything(grace, dry_run))?;
            write_json(&serde_json::json!({ "protocol": 1, "cleanup": outcome }))
        }
        _ => Err(CliError::Usage(
            "usage: project storage <project.deadpan> [--clean [--dry-run]] [--grace-hours N]"
                .into(),
        )),
    }
}

/// `project copy-portable <package> <destination.deadpan>`.
pub(crate) fn run_copy(arguments: &[&str]) -> Result<(), CliError> {
    let [package, destination] = arguments else {
        return Err(CliError::Usage(
            "usage: project copy-portable <project.deadpan> <new-copy.deadpan>".into(),
        ));
    };
    let report = deadpan_store::portable::copy_portable(
        Path::new(package),
        Path::new(destination),
        &AtomicBool::new(false),
    )?;
    write_json(&serde_json::json!({ "protocol": 1, "portable_copy": report }))
}

/// `cache status [--grace-hours N]` and
/// `cache clean [--dry-run] [--grace-hours N]`.
pub(crate) fn run_cache(arguments: &[&str]) -> Result<(), CliError> {
    let (grace, rest) = grace(arguments)?;
    let user = UserStorage::current()
        .ok_or_else(|| CliError::Usage("HOME is not set to an absolute directory".into()))?;
    match rest.as_slice() {
        ["status"] => write_json(&serde_json::json!({ "protocol": 1, "user": user.report(grace) })),
        ["clean", flags @ ..] if flags.iter().all(|flag| *flag == "--dry-run") => {
            let outcome = user.clean(grace, !flags.is_empty(), &[])?;
            write_json(&serde_json::json!({ "protocol": 1, "cleanup": outcome }))
        }
        _ => Err(CliError::Usage(
            "usage: cache status | cache clean [--dry-run]; both take [--grace-hours N]".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backdate(path: &Path, seconds: u64) {
        let past = SystemTime::now() - Duration::from_secs(seconds);
        let file = std::fs::File::options()
            .read(true)
            .open(path)
            .or_else(|_| std::fs::File::open(path))
            .unwrap();
        file.set_modified(past).unwrap();
    }

    #[test]
    fn user_cleanup_removes_only_old_rebuildable_entries_under_its_own_roots() {
        let home = tempfile::tempdir().unwrap();
        let user = UserStorage {
            caches: home.path().join("Library/Caches/Deadpan"),
            support: home.path().join("Library/Application Support/Deadpan"),
        };
        // Weights and runtimes are reported, never cleaned.
        let weights = user.caches.join("ltx-qualification/model.safetensors");
        std::fs::create_dir_all(weights.parent().unwrap()).unwrap();
        std::fs::write(&weights, vec![0_u8; 4096]).unwrap();
        backdate(&weights, 400 * 24 * 3600);
        let models = user.support.join("Models/pack/weights.bin");
        std::fs::create_dir_all(models.parent().unwrap()).unwrap();
        std::fs::write(&models, vec![0_u8; 1024]).unwrap();
        // An old abandoned download, and a running one whose directory is
        // old but whose file is still being written.
        let old = user.helper_staging().join("yt-dlp-old");
        let fresh = user.helper_staging().join("yt-dlp-fresh");
        for directory in [&old, &fresh] {
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(directory.join("download"), b"partial").unwrap();
        }
        backdate(&old.join("download"), 3 * 24 * 3600);
        backdate(&old, 3 * 24 * 3600);
        backdate(&fresh, 3 * 24 * 3600);
        assert!(
            user.clean(Duration::from_secs(60), true, &[]).is_err(),
            "grace below an hour is refused"
        );

        let report = user.report(DEFAULT_GRACE);
        let named = |name: &str| {
            report
                .directories
                .iter()
                .find(|directory| directory.name == name)
                .unwrap()
                .clone()
        };
        assert_eq!(named("ltx-qualification").bytes, 4096);
        assert!(!named("ltx-qualification").cleanable);
        assert_eq!(named("Model packs").bytes, 1024);
        assert_eq!(named("Downloader staging").removable_bytes, 7);
        assert!(!report.not_on_disk.is_empty());

        let preview = user.clean(DEFAULT_GRACE, true, &[]).unwrap();
        assert_eq!(preview.removed, vec![old.clone()]);
        assert!(old.exists());
        let outcome = user.clean(DEFAULT_GRACE, false, &[]).unwrap();
        assert_eq!(outcome.removed, vec![old.clone()]);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(weights.exists());
        assert!(models.exists());
        // No proxy cache was created by reporting or cleaning.
        assert!(!user.proxies().exists());
    }

    #[test]
    fn the_cache_preview_includes_the_budget_pass_in_least_recently_used_order() {
        let day = Duration::from_secs(86_400);
        let entries = vec![
            ("recent".to_owned(), 40, Duration::from_secs(60)),
            ("stale".to_owned(), 10, 40 * day),
            ("older".to_owned(), 30, 2 * day),
            ("kept".to_owned(), 30, 3 * day),
        ];
        // Stale goes for age; then the least recently used until 50 bytes fit.
        let removed = proxy_removals(entries, 30 * day, &["kept".to_owned()], 50);
        assert_eq!(
            removed,
            vec![("stale".to_owned(), 10), ("older".to_owned(), 30)]
        );
    }
}
