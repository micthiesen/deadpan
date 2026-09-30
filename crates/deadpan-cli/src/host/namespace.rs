use super::*;
use std::fs::{self, File};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Discovery {
    pub(super) version: u32,
    pub(super) owner_id: Uuid,
    pub(super) secret: String,
    pub(super) package_identity: PackageIdentity,
    pub(super) directory: PackageIdentity,
    pub(super) socket: PackageIdentity,
}
impl Discovery {
    pub(super) fn validate(&self) -> Result<(), HostError> {
        if self.version != VERSION
            || self.owner_id.is_nil()
            || self.secret.len() != 64
            || !self.secret.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(HostError::invalid());
        }
        Ok(())
    }
}
pub(super) struct Lease {
    base: File,
    directory: File,
    path: PathBuf,
    identity: PackageIdentity,
}
pub(super) fn identity(metadata: &fs::Metadata) -> PackageIdentity {
    PackageIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}
fn base_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    let root = Path::new("/private/tmp");
    #[cfg(target_os = "linux")]
    let root = Path::new("/tmp");
    root.join(format!(
        "deadpan-host-{}",
        rustix::process::getuid().as_raw()
    ))
}
fn private_directory(path: &Path) -> Result<File, HostError> {
    let file = File::from(
        rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|_| HostError::stale())?,
    );
    let metadata = file.metadata().map_err(|_| HostError::stale())?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::getuid().as_raw()
        || metadata.mode() & 0o7777 != 0o700
    {
        return Err(HostError::stale());
    }
    let path_metadata = fs::symlink_metadata(path).map_err(|_| HostError::stale())?;
    if !path_metadata.is_dir() || identity(&metadata) != identity(&path_metadata) {
        return Err(HostError::stale());
    }
    Ok(file)
}
impl Lease {
    pub(super) fn create(owner: Uuid) -> Result<Self, HostError> {
        let base = base_path();
        match fs::DirBuilder::new().mode(0o700).create(&base) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(HostError::io()),
        }
        let base_file = private_directory(&base)?;
        let path = base.join(owner.simple().to_string());
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|_| HostError::io())?;
        let directory = private_directory(&path)?;
        let identity = identity(&directory.metadata().map_err(|_| HostError::io())?);
        Ok(Self {
            base: base_file,
            directory,
            path,
            identity,
        })
    }
    pub(super) fn open(discovery: &Discovery) -> Result<Self, HostError> {
        discovery.validate()?;
        let base = base_path();
        let base_file = private_directory(&base)?;
        let path = base.join(discovery.owner_id.simple().to_string());
        let directory = private_directory(&path)?;
        let actual = identity(&directory.metadata().map_err(|_| HostError::stale())?);
        if actual != discovery.directory {
            return Err(HostError::stale());
        }
        let lease = Self {
            base: base_file,
            directory,
            path,
            identity: actual,
        };
        lease.recheck(discovery)?;
        Ok(lease)
    }
    pub(super) fn path(&self) -> PathBuf {
        self.path.join("control.sock")
    }
    pub(super) fn directory_identity(&self) -> PackageIdentity {
        self.identity
    }
    pub(super) fn socket_identity(&self) -> Result<PackageIdentity, HostError> {
        let metadata = fs::symlink_metadata(self.path()).map_err(|_| HostError::stale())?;
        if !metadata.file_type().is_socket()
            || metadata.uid() != rustix::process::getuid().as_raw()
            || metadata.mode() & 0o7777 != 0o600
        {
            return Err(HostError::stale());
        }
        Ok(identity(&metadata))
    }
    pub(super) fn protect_socket(&self) -> Result<PackageIdentity, HostError> {
        fs::set_permissions(self.path(), fs::Permissions::from_mode(0o600))
            .map_err(|_| HostError::io())?;
        self.socket_identity()
    }
    pub(super) fn recheck(&self, discovery: &Discovery) -> Result<(), HostError> {
        let base = private_directory(&base_path())?;
        if identity(&base.metadata().map_err(|_| HostError::stale())?)
            != identity(&self.base.metadata().map_err(|_| HostError::stale())?)
        {
            return Err(HostError::stale());
        }
        let directory = private_directory(&self.path)?;
        if identity(&directory.metadata().map_err(|_| HostError::stale())?) != self.identity
            || identity(&self.directory.metadata().map_err(|_| HostError::stale())?)
                != self.identity
            || self.socket_identity()? != discovery.socket
        {
            return Err(HostError::stale());
        }
        Ok(())
    }
    /// Only remove our exact unique directory and socket. Never touch discovery
    /// or recurse into a namespace that another owner could have replaced.
    pub(super) fn cleanup(&self, socket: Option<&PackageIdentity>) {
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if !metadata.is_dir() || identity(&metadata) != self.identity {
            return;
        }
        if let Ok(metadata) = fs::symlink_metadata(self.path())
            && metadata.file_type().is_socket()
            && socket.is_none_or(|expected| identity(&metadata) == *expected)
        {
            let _ = fs::remove_file(self.path());
        }
        let _ = fs::remove_dir(&self.path);
    }
}

pub(super) fn same_secret(actual: &str, expected: &str) -> bool {
    if actual.len() != expected.len() {
        return false;
    }
    actual
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |different, (a, b)| different | (a ^ b))
        == 0
}
