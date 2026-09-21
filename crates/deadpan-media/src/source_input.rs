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
