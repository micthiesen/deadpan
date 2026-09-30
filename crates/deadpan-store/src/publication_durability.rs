//! Fail-closed publication phase barrier. SQLite FULL remains the transaction
//! contract; these direct flushes must succeed before a rename permit is issued.
use crate::StoreError;
use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags, Stat, fstat, fsync, openat, statat};
use std::{
    ffi::OsString,
    os::fd::OwnedFd,
    path::{Component, Path},
};

struct Link {
    parent: OwnedFd,
    name: OsString,
    identity: Stat,
}
pub(crate) struct PublicationDurability {
    chain: Vec<Link>,
    package: OwnedFd,
    database: Stat,
    wal: Option<Stat>,
    #[cfg(test)]
    fault: Option<Step>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    BeforeIdentity,
    DatabaseFlush,
    WalFlush,
    DirectoryFlush,
    AfterIdentity,
}
fn io(error: rustix::io::Errno) -> StoreError {
    StoreError::Io(error.into())
}
fn invalid(message: &str) -> StoreError {
    StoreError::Publication(message.into())
}
fn same(a: &Stat, b: &Stat) -> bool {
    a.st_dev == b.st_dev && a.st_ino == b.st_ino
}
const DIRECTORY: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const FILE: OFlags = OFlags::RDWR
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC)
    .union(OFlags::NONBLOCK);
impl PublicationDurability {
    pub(crate) fn open(path: &Path) -> Result<Self, StoreError> {
        if !path.is_absolute() {
            return Err(invalid("publication package path is not absolute"));
        }
        let mut current = openat(CWD, "/", DIRECTORY, Mode::empty()).map_err(io)?;
        let mut chain = Vec::new();
        for component in path.components() {
            let Component::Normal(name) = component else {
                if component == Component::RootDir {
                    continue;
                }
                return Err(invalid("publication package path is not canonical"));
            };
            let next = openat(&current, name, DIRECTORY, Mode::empty()).map_err(io)?;
            let identity = fstat(&next).map_err(io)?;
            chain.push(Link {
                parent: current,
                name: name.to_owned(),
                identity,
            });
            current = next;
        }
        let package = fstat(&current).map_err(io)?;
        if package.st_uid != rustix::process::geteuid().as_raw() || package.st_mode & 0o022 != 0 {
            return Err(invalid("publication package ownership or mode changed"));
        }
        let database_fd = openat(&current, "project.sqlite", FILE, Mode::empty()).map_err(io)?;
        let database = fstat(&database_fd).map_err(io)?;
        validate_file(&database, &package)?;
        let wal = match openat(&current, "project.sqlite-wal", FILE, Mode::empty()) {
            Ok(fd) => {
                let metadata = fstat(&fd).map_err(io)?;
                validate_file(&metadata, &package)?;
                Some(metadata)
            }
            Err(rustix::io::Errno::NOENT) => None,
            Err(error) => return Err(io(error)),
        };
        let value = Self {
            chain,
            package: current,
            database,
            wal,
            #[cfg(test)]
            fault: None,
        };
        value.check_chain()?;
        Ok(value)
    }
    pub(crate) fn capture_wal(&mut self) -> Result<(), StoreError> {
        self.check_chain()?;
        let wal = openat(&self.package, "project.sqlite-wal", FILE, Mode::empty()).map_err(io)?;
        let current = fstat(&wal).map_err(io)?;
        validate_file(&current, &fstat(&self.package).map_err(io)?)?;
        if self
            .wal
            .as_ref()
            .is_some_and(|expected| !same(expected, &current))
        {
            return Err(invalid("publication WAL identity changed"));
        }
        let db = statat(&self.package, "project.sqlite", AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        if !same(&self.database, &db) {
            return Err(invalid("publication database identity changed"));
        }
        self.wal = Some(current);
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn fail_at(&mut self, step: Step) {
        self.fault = Some(step);
    }
    pub(crate) fn barrier(&mut self) -> Result<(), StoreError> {
        #[cfg(test)]
        let fault = self.fault;
        self.barrier_with(|_step| {
            #[cfg(test)]
            if fault == Some(_step) {
                return Err(invalid("injected publication barrier failure"));
            }
            Ok(())
        })
    }
    fn barrier_with(
        &mut self,
        mut step: impl FnMut(Step) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        step(Step::BeforeIdentity)?;
        self.check_chain()?;
        let package = fstat(&self.package).map_err(io)?;
        let db = openat(&self.package, "project.sqlite", FILE, Mode::empty()).map_err(io)?;
        let wal = openat(&self.package, "project.sqlite-wal", FILE, Mode::empty()).map_err(io)?;
        let db_stat = fstat(&db).map_err(io)?;
        let wal_stat = fstat(&wal).map_err(io)?;
        validate_file(&db_stat, &package)?;
        validate_file(&wal_stat, &package)?;
        if !same(&db_stat, &self.database)
            || self
                .wal
                .as_ref()
                .is_some_and(|expected| !same(expected, &wal_stat))
        {
            return Err(invalid("publication database or WAL was replaced"));
        }
        self.check_names(&db_stat, &wal_stat)?;
        step(Step::DatabaseFlush)?;
        full_sync(&db)?;
        step(Step::WalFlush)?;
        full_sync(&wal)?;
        step(Step::DirectoryFlush)?;
        fsync(&self.package).map_err(io)?;
        full_sync(&db)?;
        full_sync(&wal)?;
        step(Step::AfterIdentity)?;
        self.check_chain()?;
        self.check_names(&db_stat, &wal_stat)?;
        self.wal = Some(wal_stat);
        Ok(())
    }
    fn check_chain(&self) -> Result<(), StoreError> {
        for link in &self.chain {
            let now = statat(&link.parent, &link.name, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
            if !same(&now, &link.identity) || !FileType::from_raw_mode(now.st_mode).is_dir() {
                return Err(invalid("publication package directory chain changed"));
            }
        }
        let package = fstat(&self.package).map_err(io)?;
        if package.st_uid != rustix::process::geteuid().as_raw() || package.st_mode & 0o022 != 0 {
            return Err(invalid("publication package ownership or mode changed"));
        }
        Ok(())
    }
    fn check_names(&self, db: &Stat, wal: &Stat) -> Result<(), StoreError> {
        for (name, expected) in [("project.sqlite", db), ("project.sqlite-wal", wal)] {
            let current = statat(&self.package, name, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
            if !same(&current, expected) {
                return Err(invalid("publication database namespace changed"));
            }
            validate_file(&current, &fstat(&self.package).map_err(io)?)?;
        }
        Ok(())
    }
}
fn validate_file(file: &Stat, package: &Stat) -> Result<(), StoreError> {
    if !FileType::from_raw_mode(file.st_mode).is_file()
        || file.st_nlink != 1
        || file.st_dev != package.st_dev
        || file.st_uid != package.st_uid
        || file.st_mode & 0o022 != 0
    {
        return Err(invalid("unsafe publication database or WAL entry"));
    }
    Ok(())
}
fn full_sync(fd: &OwnedFd) -> Result<(), StoreError> {
    #[cfg(target_os = "macos")]
    rustix::fs::fcntl_fullfsync(fd).map_err(io)?;
    #[cfg(target_os = "linux")]
    fsync(fd).map_err(io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(tempfile::TempDir, PublicationDurability), StoreError> {
        let root = tempfile::tempdir()?;
        std::fs::write(root.path().join("project.sqlite"), b"database")?;
        std::fs::write(root.path().join("project.sqlite-wal"), b"wal")?;
        let pin = PublicationDurability::open(&root.path().canonicalize()?)?;
        Ok((root, pin))
    }
    #[test]
    fn barrier_failure_at_every_step_denies_acknowledgement() -> Result<(), StoreError> {
        for fail in [
            Step::BeforeIdentity,
            Step::DatabaseFlush,
            Step::WalFlush,
            Step::DirectoryFlush,
            Step::AfterIdentity,
        ] {
            let (_root, mut pin) = fixture()?;
            let mut visited = Vec::new();
            assert!(
                pin.barrier_with(|step| {
                    visited.push(step);
                    if step == fail {
                        Err(invalid("injected barrier fault"))
                    } else {
                        Ok(())
                    }
                })
                .is_err()
            );
            assert_eq!(visited.last(), Some(&fail));
        }
        Ok(())
    }
    #[test]
    fn replacement_database_wal_and_package_fail_closed() -> Result<(), StoreError> {
        for name in ["project.sqlite", "project.sqlite-wal"] {
            let (root, mut pin) = fixture()?;
            std::fs::rename(root.path().join(name), root.path().join("old"))?;
            std::fs::write(root.path().join(name), b"replacement")?;
            assert!(pin.barrier().is_err());
        }
        let (root, mut pin) = fixture()?;
        std::fs::remove_file(root.path().join("project.sqlite-wal"))?;
        assert!(pin.barrier().is_err());
        let parent = tempfile::tempdir()?;
        let package = parent.path().join("project");
        std::fs::create_dir(&package)?;
        std::fs::write(package.join("project.sqlite"), b"database")?;
        std::fs::write(package.join("project.sqlite-wal"), b"wal")?;
        let mut pin = PublicationDurability::open(&package.canonicalize()?)?;
        std::fs::rename(&package, parent.path().join("old"))?;
        std::fs::create_dir(&package)?;
        assert!(pin.barrier().is_err());
        Ok(())
    }
}
