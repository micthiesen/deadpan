//! A native recovery decision stays bound to the files the person inspected.

use std::io::Read as _;
use std::os::unix::fs::MetadataExt as _;

use super::*;

pub(super) const DATABASE_FILES: [&str; 4] = [
    "project.sqlite",
    "project.sqlite-wal",
    "project.sqlite-shm",
    "project.sqlite-journal",
];

/// A verified backup offered for a package that cannot open. The private file
/// observations prevent a delayed confirmation from replacing a changed package
/// or restoring a different backup. This is not a writable store or a lock held
/// while the person decides.
#[derive(Debug, Clone)]
pub struct DamagedRecovery {
    pub preview: BackupPreview,
    pub requires_project_confirmation: bool,
    package: PathBuf,
    package_identity: (u64, u64),
    files: Vec<Option<Stamp>>,
    backup: Stamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

fn stamp(path: &Path) -> Result<Option<Stamp>, BackupError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Err(StoreError::UnsafePath(path.to_owned()).into());
    }
    Ok(Some(Stamp {
        device: metadata.dev(),
        inode: metadata.ino(),
        bytes: metadata.len(),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
    }))
}

fn identity(package: &Path) -> Result<(u64, u64), BackupError> {
    let metadata = fs::symlink_metadata(package)?;
    if !metadata.is_dir() {
        return Err(StoreError::UnsafePath(package.to_owned()).into());
    }
    Ok((metadata.dev(), metadata.ino()))
}

fn files(package: &Path) -> Result<Vec<Option<Stamp>>, BackupError> {
    DATABASE_FILES
        .into_iter()
        .chain(["manifest.json"])
        .map(|name| stamp(&package.join(name)))
        .collect()
}

pub(super) fn manifest_project(package: &Path) -> Option<ProjectId> {
    #[derive(serde::Deserialize)]
    struct Manifest {
        project_id: ProjectId,
    }
    // An unreadable manifest requires explicit identity confirmation. Never
    // block on a FIFO or follow a symlink while trying to discover that identity.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(
            i32::try_from((rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::NOFOLLOW).bits())
                .ok()?,
        )
        .open(package.join("manifest.json"))
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 64 * 1024 {
        return None;
    }
    serde_json::from_slice::<Manifest>(&bytes)
        .ok()
        .map(|manifest| manifest.project_id)
}

/// Verify the selected backup without replacing anything. A project which now
/// opens is refused: use ordinary backup restore, which first saves its state.
pub fn inspect_damaged_database(package: &Path, id: &str) -> Result<DamagedRecovery, BackupError> {
    let package = package.canonicalize()?;
    let _writer = crate::acquire_lock(&package)?;
    if ProjectStore::open(&package, crate::AccessMode::ReadOnly).is_ok() {
        return Err(BackupError::Verification(
            "this project opens now; reopen it and use its normal backup controls".into(),
        ));
    }
    let lock = lock_file(&directory(&package))?;
    lock.lock_shared()?;
    let info = list_backups(&package)?
        .into_iter()
        .find(|info| info.id == id)
        .ok_or_else(|| BackupError::NotFound(id.into()))?;
    let package_identity = identity(&package)?;
    let observed = files(&package)?;
    let backup = stamp(&info.path)?.ok_or_else(|| BackupError::NotFound(id.into()))?;
    let preview = verify_backup(&info)?;
    let manifest = manifest_project(&package);
    if manifest
        .as_ref()
        .is_some_and(|project| *project != preview.project_id)
    {
        return Err(BackupError::OtherProject);
    }
    let captured = DamagedRecovery {
        preview,
        requires_project_confirmation: manifest.is_none(),
        package,
        package_identity,
        files: observed,
        backup,
    };
    captured.check(&captured.package, &info)?;
    Ok(captured)
}

impl DamagedRecovery {
    pub(super) fn check(&self, package: &Path, info: &BackupInfo) -> Result<(), BackupError> {
        if package != self.package
            || identity(package)? != self.package_identity
            || files(package)? != self.files
            || info.id != self.preview.info.id
            || stamp(&info.path)?.as_ref() != Some(&self.backup)
        {
            return Err(BackupError::Verification(
                "the project or selected backup changed after inspection; inspect it again".into(),
            ));
        }
        Ok(())
    }
}

/// Apply exactly the inspected recovery. Unreadable manifest identity must be
/// confirmed explicitly; the caller must not infer it from the chosen backup.
pub fn restore_damaged_database(
    captured: &DamagedRecovery,
    confirmed_project: Option<&ProjectId>,
) -> Result<DamagedReplacement, BackupError> {
    replace_damaged_database_captured(
        &captured.package,
        &captured.preview.info.id,
        confirmed_project,
        Some(captured),
    )
}
