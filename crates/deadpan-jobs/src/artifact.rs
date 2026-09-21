//! Descriptor-relative containment and hash snapshots for worker artifacts.
//!
//! This module proves only that one worker-declared file was below a
//! host-selected output scope and that a frozen copy matches its declared byte
//! length and SHA-256. Media structure, dimensions, duration, candidate
//! readiness, persistence, and promotion remain separate host responsibilities.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::OwnedFd;
use std::path::Path;

use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags, Stat, fstat, openat, statat};
use sha2::{Digest, Sha256 as Sha256Hasher};
use thiserror::Error;

use crate::protocol::{WorkspaceArtifact, WorkspaceRef};

const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// A workspace directory held by descriptor so later path replacement cannot
/// redirect artifact resolution. Create this before spawning the worker and
/// retain it until all output checks finish.
#[derive(Debug)]
pub struct ArtifactWorkspace {
    root: OwnedFd,
    device: i128,
    owner: u32,
}

impl ArtifactWorkspace {
    pub fn open(path: &Path) -> Result<Self, ArtifactError> {
        let root = openat(
            CWD,
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|source| match source {
            rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => ArtifactError::UnsafeWorkspace,
            _ => ArtifactError::System {
                operation: "open workspace",
                source,
            },
        })?;
        let metadata = fstat(&root).map_err(|source| ArtifactError::System {
            operation: "inspect workspace",
            source,
        })?;
        if !FileType::from_raw_mode(metadata.st_mode).is_dir()
            || metadata.st_uid != rustix::process::geteuid().as_raw()
        {
            return Err(ArtifactError::UnsafeWorkspace);
        }
        Ok(Self {
            root,
            device: i128::from(metadata.st_dev),
            owner: metadata.st_uid,
        })
    }

    /// Freezes a worker output after its process group and pipes have stopped.
    ///
    /// `declared.reference()` must be strictly below `output_scope`; equality
    /// and component-prefix lookalikes are rejected before filesystem access.
    pub fn snapshot(
        &self,
        output_scope: &WorkspaceRef,
        declared: &WorkspaceArtifact,
        limits: ArtifactLimits,
    ) -> Result<HashedArtifactSnapshot, ArtifactError> {
        self.snapshot_after_open(output_scope, declared, limits, || {})
    }

    fn snapshot_after_open(
        &self,
        output_scope: &WorkspaceRef,
        declared: &WorkspaceArtifact,
        limits: ArtifactLimits,
        after_open: impl FnOnce(),
    ) -> Result<HashedArtifactSnapshot, ArtifactError> {
        require_below_scope(output_scope, declared.reference())?;
        if declared.byte_length() > limits.maximum_bytes {
            return Err(ArtifactError::TooLarge {
                size: declared.byte_length(),
                maximum: limits.maximum_bytes,
            });
        }

        let components: Vec<&str> = declared.reference().as_str().split('/').collect();
        let (leaf, parents) = components
            .split_last()
            .expect("WorkspaceRef always has at least one component");
        let mut opened_directory = None;
        for component in parents {
            let parent = opened_directory.as_ref().unwrap_or(&self.root);
            let directory = openat(
                parent,
                *component,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|source| component_error(declared.reference(), component, source))?;
            let metadata = fstat(&directory).map_err(|source| ArtifactError::System {
                operation: "inspect output directory",
                source,
            })?;
            self.validate_contained(&metadata, component)?;
            opened_directory = Some(directory);
        }

        let parent = opened_directory.as_ref().unwrap_or(&self.root);
        let source = self.open_artifact(parent, leaf, declared.reference())?;
        let before = fstat(&source).map_err(|source| ArtifactError::System {
            operation: "inspect artifact",
            source,
        })?;
        self.validate_source(&before, declared, limits)?;
        after_open();

        let mut source = File::from(source);
        let mut snapshot = tempfile::tempfile()?;
        let mut hasher = Sha256Hasher::new();
        let mut buffer = [0_u8; COPY_BUFFER_BYTES];
        let mut copied = 0_u64;
        loop {
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            copied = copied
                .checked_add(u64::try_from(read).expect("buffer length fits u64"))
                .ok_or(ArtifactError::SourceMutated)?;
            if copied > declared.byte_length() {
                return Err(ArtifactError::SourceMutated);
            }
            if copied > limits.maximum_bytes {
                return Err(ArtifactError::TooLarge {
                    size: copied,
                    maximum: limits.maximum_bytes,
                });
            }
            hasher.update(&buffer[..read]);
            snapshot.write_all(&buffer[..read])?;
        }

        let after = rustix::fs::fstat(&source).map_err(|source| ArtifactError::System {
            operation: "reinspect artifact",
            source,
        })?;
        self.validate_source(&after, declared, limits)?;
        if !same_file_state(&before, &after) {
            return Err(ArtifactError::SourceMutated);
        }
        if copied != declared.byte_length() {
            return Err(ArtifactError::LengthMismatch {
                declared: declared.byte_length(),
                actual: copied,
            });
        }

        let observed = encode_hex(&hasher.finalize());
        if observed != declared.sha256().as_str() {
            return Err(ArtifactError::HashMismatch {
                declared: declared.sha256().to_string(),
                observed,
            });
        }
        snapshot.seek(SeekFrom::Start(0))?;
        Ok(HashedArtifactSnapshot {
            file: snapshot,
            declaration: declared.clone(),
        })
    }

    fn open_artifact(
        &self,
        parent: &OwnedFd,
        leaf: &str,
        reference: &WorkspaceRef,
    ) -> Result<OwnedFd, ArtifactError> {
        match openat(
            parent,
            leaf,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(file) => Ok(file),
            Err(open_error) => match statat(parent, leaf, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(metadata) => {
                    let file_type = FileType::from_raw_mode(metadata.st_mode);
                    if file_type.is_symlink() {
                        Err(ArtifactError::UnsafeComponent(leaf.into()))
                    } else if !file_type.is_file() {
                        Err(ArtifactError::NotRegularFile(reference.clone()))
                    } else {
                        Err(ArtifactError::System {
                            operation: "open artifact",
                            source: open_error,
                        })
                    }
                }
                Err(rustix::io::Errno::NOENT) => {
                    Err(ArtifactError::MissingArtifact(reference.clone()))
                }
                Err(source) => Err(ArtifactError::System {
                    operation: "inspect unopened artifact",
                    source,
                }),
            },
        }
    }

    fn validate_contained(&self, metadata: &Stat, component: &str) -> Result<(), ArtifactError> {
        if i128::from(metadata.st_dev) != self.device {
            return Err(ArtifactError::CrossDevice(component.into()));
        }
        if metadata.st_uid != self.owner {
            return Err(ArtifactError::UnexpectedOwner(component.into()));
        }
        Ok(())
    }

    fn validate_source(
        &self,
        metadata: &Stat,
        declared: &WorkspaceArtifact,
        limits: ArtifactLimits,
    ) -> Result<(), ArtifactError> {
        if !FileType::from_raw_mode(metadata.st_mode).is_file() {
            return Err(ArtifactError::NotRegularFile(declared.reference().clone()));
        }
        self.validate_contained(metadata, declared.reference().as_str())?;
        if metadata.st_nlink != 1 {
            return Err(ArtifactError::MultipleLinks(declared.reference().clone()));
        }
        let actual = u64::try_from(metadata.st_size).map_err(|_| ArtifactError::SourceMutated)?;
        if actual > limits.maximum_bytes {
            return Err(ArtifactError::TooLarge {
                size: actual,
                maximum: limits.maximum_bytes,
            });
        }
        if actual != declared.byte_length() {
            return Err(ArtifactError::LengthMismatch {
                declared: declared.byte_length(),
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactLimits {
    maximum_bytes: u64,
}

impl ArtifactLimits {
    pub fn new(maximum_bytes: u64) -> Result<Self, ArtifactError> {
        if maximum_bytes == 0 {
            return Err(ArtifactError::InvalidBudget);
        }
        Ok(Self { maximum_bytes })
    }

    pub const fn maximum_bytes(self) -> u64 {
        self.maximum_bytes
    }
}

/// An ephemeral, host-owned copy whose bytes match the worker declaration.
/// This is not proof of valid media and cannot authorize candidate acceptance.
#[derive(Debug)]
pub struct HashedArtifactSnapshot {
    file: File,
    declaration: WorkspaceArtifact,
}

impl HashedArtifactSnapshot {
    pub fn declaration(&self) -> &WorkspaceArtifact {
        &self.declaration
    }
}

impl Read for HashedArtifactSnapshot {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.read(buffer)
    }
}

impl Seek for HashedArtifactSnapshot {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}

fn require_below_scope(
    output_scope: &WorkspaceRef,
    artifact: &WorkspaceRef,
) -> Result<(), ArtifactError> {
    let scope: Vec<&str> = output_scope.as_str().split('/').collect();
    let candidate: Vec<&str> = artifact.as_str().split('/').collect();
    if candidate.len() <= scope.len() || candidate[..scope.len()] != scope {
        return Err(ArtifactError::OutsideOutputScope {
            scope: output_scope.clone(),
            artifact: artifact.clone(),
        });
    }
    Ok(())
}

fn component_error(
    reference: &WorkspaceRef,
    component: &str,
    source: rustix::io::Errno,
) -> ArtifactError {
    match source {
        rustix::io::Errno::NOENT => ArtifactError::MissingArtifact(reference.clone()),
        rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => {
            ArtifactError::UnsafeComponent(component.into())
        }
        _ => ArtifactError::System {
            operation: "open output directory",
            source,
        },
    }
}

fn same_file_state(before: &Stat, after: &Stat) -> bool {
    before.st_dev == after.st_dev
        && before.st_ino == after.st_ino
        && before.st_mode == after.st_mode
        && before.st_uid == after.st_uid
        && before.st_size == after.st_size
        && before.st_nlink == after.st_nlink
        && before.st_mtime == after.st_mtime
        && before.st_mtime_nsec == after.st_mtime_nsec
        && before.st_ctime == after.st_ctime
        && before.st_ctime_nsec == after.st_ctime_nsec
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("artifact workspace is not a host-owned real directory")]
    UnsafeWorkspace,
    #[error("artifact path component is unsafe: {0}")]
    UnsafeComponent(String),
    #[error("artifact {artifact} is not strictly below host output scope {scope}")]
    OutsideOutputScope {
        scope: WorkspaceRef,
        artifact: WorkspaceRef,
    },
    #[error("artifact does not exist: {0}")]
    MissingArtifact(WorkspaceRef),
    #[error("artifact is not a regular file: {0}")]
    NotRegularFile(WorkspaceRef),
    #[error("artifact path crossed onto another filesystem at {0}")]
    CrossDevice(String),
    #[error("artifact path has an unexpected owner at {0}")]
    UnexpectedOwner(String),
    #[error("artifact has another hard link: {0}")]
    MultipleLinks(WorkspaceRef),
    #[error("artifact size {size} exceeds host limit {maximum}")]
    TooLarge { size: u64, maximum: u64 },
    #[error("artifact length is {actual}, worker declared {declared}")]
    LengthMismatch { declared: u64, actual: u64 },
    #[error("artifact SHA-256 mismatch: declared {declared}, observed {observed}")]
    HashMismatch { declared: String, observed: String },
    #[error("artifact changed while the host was snapshotting it")]
    SourceMutated,
    #[error("artifact byte budget must be positive")]
    InvalidBudget,
    #[error("{operation} failed")]
    System {
        operation: &'static str,
        #[source]
        source: rustix::io::Errno,
    },
    #[error("artifact snapshot I/O failed")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::protocol::Sha256;

    #[test]
    fn mutation_after_open_is_rejected_deterministically() {
        let scratch = tempfile::tempdir().unwrap();
        let output = scratch.path().join("output");
        fs::create_dir(&output).unwrap();
        let artifact_path = output.join("candidate.bin");
        fs::write(&artifact_path, b"before").unwrap();
        let declaration = WorkspaceArtifact::new(
            WorkspaceRef::new("output/candidate.bin").unwrap(),
            Sha256::new(encode_hex(&Sha256Hasher::digest(b"before"))).unwrap(),
            6,
        )
        .unwrap();
        let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();
        let result = workspace.snapshot_after_open(
            &WorkspaceRef::new("output").unwrap(),
            &declaration,
            ArtifactLimits::new(64).unwrap(),
            || fs::write(&artifact_path, b"change").unwrap(),
        );
        assert!(matches!(
            result,
            Err(ArtifactError::SourceMutated | ArtifactError::HashMismatch { .. })
        ));
    }

    #[test]
    fn growth_after_open_stops_at_the_declared_length() {
        let scratch = tempfile::tempdir().unwrap();
        let output = scratch.path().join("output");
        fs::create_dir(&output).unwrap();
        let artifact_path = output.join("candidate.bin");
        fs::write(&artifact_path, b"before").unwrap();
        let declaration = WorkspaceArtifact::new(
            WorkspaceRef::new("output/candidate.bin").unwrap(),
            Sha256::new(encode_hex(&Sha256Hasher::digest(b"before"))).unwrap(),
            6,
        )
        .unwrap();
        let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();
        let result = workspace.snapshot_after_open(
            &WorkspaceRef::new("output").unwrap(),
            &declaration,
            ArtifactLimits::new(1024 * 1024).unwrap(),
            || {
                let mut file = fs::OpenOptions::new()
                    .append(true)
                    .open(&artifact_path)
                    .unwrap();
                file.write_all(b" appended bytes").unwrap();
            },
        );
        assert!(matches!(result, Err(ArtifactError::SourceMutated)));
    }

    #[test]
    fn contained_metadata_classifies_device_and_owner_changes() {
        let scratch = tempfile::tempdir().unwrap();
        let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();
        let mut metadata = fstat(&workspace.root).unwrap();
        metadata.st_dev = metadata.st_dev.wrapping_add(1);
        assert!(matches!(
            workspace.validate_contained(&metadata, "mounted"),
            Err(ArtifactError::CrossDevice(component)) if component == "mounted"
        ));

        let mut metadata = fstat(&workspace.root).unwrap();
        metadata.st_uid = metadata.st_uid.wrapping_add(1);
        assert!(matches!(
            workspace.validate_contained(&metadata, "foreign"),
            Err(ArtifactError::UnexpectedOwner(component)) if component == "foreign"
        ));
    }
}
