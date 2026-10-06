//! One private, verified byte snapshot shared by independently scheduled decoders.

use std::fs::File;
use std::io::{Read, Seek};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::conversion::{Deadline, snapshot};
use crate::source_index::SourceContentIdentity;
use crate::{ConversionError, InputIdentity};

#[derive(Debug, thiserror::Error)]
pub enum SourceInputError {
    #[error("invalid source snapshot limits")]
    Limits,
    #[error(transparent)]
    Snapshot(#[from] ConversionError),
}

/// Cloning shares immutable private bytes, never the caller's original file.
/// No path, writable descriptor, or cursor escapes this boundary. Native
/// decoders use positional reads and maintain their own independent positions.
#[derive(Clone)]
pub struct VerifiedSourceInput {
    file: Arc<File>,
    identity: SourceContentIdentity,
}

impl VerifiedSourceInput {
    /// Call on a media service thread. A finite local reader is required:
    /// arbitrary blocking `Read` implementations cannot be preempted.
    pub fn copy_verified(
        source: &mut impl Read,
        identity: SourceContentIdentity,
        maximum_bytes: u64,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<Self, SourceInputError> {
        if maximum_bytes == 0
            || maximum_bytes > 64 * 1024 * 1024 * 1024
            || identity.byte_length() > maximum_bytes
            || timeout.is_zero()
            || timeout > Duration::from_secs(24 * 60 * 60)
        {
            return Err(SourceInputError::Limits);
        }
        Ok(Self::copy_with_deadline(
            source,
            identity,
            &Deadline {
                end: Instant::now() + timeout,
                cancelled,
            },
        )?)
    }

    /// Share an already verified, read-only regular file without copying it.
    /// The caller must have checked that `file` holds exactly `identity`'s
    /// bytes (a full hash, or a recorded hash bound to the file's unchanged
    /// device, inode, length and modification time) and that nothing it
    /// trusts can modify it. Decoders still check every picture against its
    /// index. Used for preview proxies, never for Originals.
    pub fn from_verified_file(
        file: File,
        identity: SourceContentIdentity,
    ) -> Result<Self, SourceInputError> {
        let metadata = file
            .metadata()
            .map_err(|error| SourceInputError::Snapshot(error.into()))?;
        if !metadata.is_file() || metadata.len() != identity.byte_length() {
            return Err(SourceInputError::Limits);
        }
        Ok(Self {
            file: Arc::new(file),
            identity,
        })
    }

    /// Adopt the project store's verified private Original snapshot without
    /// copying or hashing it again. Only `deadpan-store`
    /// (`VerifiedOriginalObject::into_source_input`) calls this: it wrote
    /// `file` itself, an anonymous temporary file, while checking the
    /// complete BLAKE3 identity, SHA-256 and length against the record that
    /// `identity` must name, and no other handle to it escapes. A Rust
    /// visibility seal across crates would need the store's copy loop to move
    /// into this crate; instead the name states the contract, the length is
    /// checked here, and debug builds recompute the SHA-256. Decoders still
    /// check every picture against its index.
    #[doc(hidden)]
    pub fn from_store_verified_snapshot(
        file: File,
        identity: SourceContentIdentity,
    ) -> Result<Self, SourceInputError> {
        let metadata = file
            .metadata()
            .map_err(|error| SourceInputError::Snapshot(error.into()))?;
        if !metadata.is_file() || metadata.len() != identity.byte_length() {
            return Err(SourceInputError::Limits);
        }
        #[cfg(debug_assertions)]
        {
            use sha2::Digest;
            use std::os::unix::fs::FileExt;
            let mut hasher = sha2::Sha256::new();
            let mut buffer = vec![0_u8; 1 << 20];
            let mut offset = 0_u64;
            loop {
                let read = file
                    .read_at(&mut buffer, offset)
                    .map_err(|error| SourceInputError::Snapshot(error.into()))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                offset += read as u64;
            }
            assert_eq!(
                <[u8; 32]>::from(hasher.finalize()),
                identity.sha256(),
                "an adopted snapshot is not the verified bytes it claims"
            );
        }
        Ok(Self {
            file: Arc::new(file),
            identity,
        })
    }

    pub fn identity(&self) -> SourceContentIdentity {
        self.identity
    }

    pub(crate) fn copy_with_deadline(
        source: &mut impl Read,
        identity: SourceContentIdentity,
        deadline: &Deadline<'_>,
    ) -> Result<Self, ConversionError> {
        deadline.check()?;
        let mut file = snapshot(
            source,
            InputIdentity {
                sha256: identity.sha256(),
            },
            identity.byte_length(),
            deadline,
        )?;
        file.rewind()?;
        deadline.check()?;
        Ok(Self {
            file: Arc::new(file),
            identity,
        })
    }

    pub(crate) fn decoder_file(&self) -> std::io::Result<File> {
        self.file.try_clone()
    }
}
