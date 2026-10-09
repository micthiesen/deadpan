//! Fresh identities for the helper and loaded FFmpeg backing objects.
//!
//! A live observation binds a loaded Mach-O UUID and kernel-reported mapped
//! vnode to an opened regular file. Paths only locate a descriptor; they never
//! establish identity. Hash that descriptor under caller byte/deadline limits,
//! revalidate before and after hashing, and retain it through the protected work
//! with another revalidation afterward. Pure serialized claims restore no live
//! authority and must be compared with fresh observations in each process.
//!
//! This is backing-object provenance under trusted installed code, not a hash
//! of resident memory or protection against hostile code in this process. UUIDs
//! are identifiers, not cryptographic digests. In-place changes predating capture
//! which preserve the UUID, or private patched memory, cannot be excluded by
//! vnode metadata and file hashing. Publication/release trust must supply the
//! installed-code guarantee. OS frameworks, kernel and driver bytes are outside
//! this fixed image inventory. Calls belong off UI/audio threads; native I/O is
//! bounded but cannot preempt a stuck filesystem or kernel call.

use std::{fs::File, path::PathBuf};

use rustix::fs::{Mode, OFlags};
use serde::{Deserialize, Serialize};

use crate::runtime_ffi;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeIdentityError {
    #[error("invalid runtime identity: {0}")]
    InvalidEvidence(&'static str),
    #[error("native runtime identity {code}: {message}")]
    Native { code: String, message: String },
    #[error("runtime identity descriptor I/O: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeImageKind {
    Helper,
    Avcodec,
    Avformat,
    Avutil,
    Swscale,
    /// Source deinterlacing dependency, observed by doctor. It is not part of
    /// the encoder capability inventory, which consumes already composed pixels.
    Avfilter,
}

impl RuntimeImageKind {
    pub const ENCODER_IMAGES: [Self; 5] = [
        Self::Helper,
        Self::Avcodec,
        Self::Avformat,
        Self::Avutil,
        Self::Swscale,
    ];

    pub fn validate(self) -> Result<(), RuntimeIdentityError> {
        Ok(())
    }

    pub(crate) const fn native(self) -> u32 {
        match self {
            Self::Helper => 0,
            Self::Avcodec => 1,
            Self::Avformat => 2,
            Self::Avutil => 3,
            Self::Swscale => 4,
            Self::Avfilter => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFileTime {
    pub seconds: i64,
    pub nanoseconds: u32,
}

impl RuntimeFileTime {
    pub fn validate(self) -> Result<(), RuntimeIdentityError> {
        if self.nanoseconds >= 1_000_000_000 {
            return Err(RuntimeIdentityError::InvalidEvidence(
                "timestamp nanoseconds exceed range",
            ));
        }
        Ok(())
    }
}

/// Path-free evidence only. Device/inode identify the current mapped object;
/// they are not a portable identity or a replacement for a fresh file hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappedImageIdentity {
    pub kind: RuntimeImageKind,
    pub device: u64,
    pub inode: u64,
    pub uuid: [u8; 16],
    pub file_size: u64,
    pub modification_time: RuntimeFileTime,
    pub change_time: RuntimeFileTime,
    pub birth_time: RuntimeFileTime,
    pub generation: u32,
}

impl MappedImageIdentity {
    pub fn validate(&self) -> Result<(), RuntimeIdentityError> {
        self.kind.validate()?;
        if self.device == 0
            || self.device > u64::from(u32::MAX)
            || self.inode == 0
            || self.uuid == [0; 16]
            || self.file_size == 0
            || self.file_size > i64::MAX.unsigned_abs()
        {
            return Err(RuntimeIdentityError::InvalidEvidence(
                "mapped object identity is empty or out of bounds",
            ));
        }
        self.modification_time.validate()?;
        self.change_time.validate()?;
        self.birth_time.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePlatform {
    pub os_build: String,
    pub hardware_model: String,
    pub cpu_family: u32,
}

impl RuntimePlatform {
    pub fn validate(&self) -> Result<(), RuntimeIdentityError> {
        for text in [&self.os_build, &self.hardware_model] {
            if text.is_empty()
                || text.len() > 63
                || !text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b".,_-".contains(&byte))
            {
                return Err(RuntimeIdentityError::InvalidEvidence(
                    "platform value is empty, oversized or malformed",
                ));
            }
        }
        if self.cpu_family == 0 {
            return Err(RuntimeIdentityError::InvalidEvidence(
                "CPU family is absent",
            ));
        }
        Ok(())
    }
}

/// Live own-process evidence. There is no constructor from serialized identity
/// and no path/address getter. Debug deliberately exposes only the durable DTO.
pub struct RuntimeImageObservation {
    identity: MappedImageIdentity,
    path: PathBuf,
    native: runtime_ffi::Image,
}

impl std::fmt::Debug for RuntimeImageObservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeImageObservation")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl RuntimeImageObservation {
    pub fn capture(kind: RuntimeImageKind) -> Result<Self, RuntimeIdentityError> {
        let native = runtime_ffi::capture(kind)?;
        let identity = native.identity(kind)?;
        let path = native.path()?;
        Ok(Self {
            identity,
            path,
            native,
        })
    }

    pub fn identity(&self) -> &MappedImageIdentity {
        &self.identity
    }

    /// Open nonblocking before checking regular-file identity. A replaced final
    /// symlink is rejected, and a different vnode never inherits loaded-image
    /// authority even when its bytes or UUID match.
    pub fn open_matching(&self) -> Result<File, RuntimeIdentityError> {
        let descriptor = rustix::fs::open(
            &self.path,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        let file = File::from(descriptor);
        self.revalidate(&file)?;
        Ok(file)
    }

    /// Re-observe the same loaded role and check the pinned descriptor's exact
    /// vnode metadata and Mach-O slice UUID. This never reopens a pathname.
    pub fn revalidate(&self, file: &File) -> Result<(), RuntimeIdentityError> {
        runtime_ffi::revalidate(&self.native, file)
    }
}

/// Fixed complete encoder inventory. Source-only avfilter is observed separately.
/// Missing swscale fails explicitly; standalone
/// encoder-only tools can request the individual roles they actually link.
pub fn capture_current() -> Result<[RuntimeImageObservation; 5], RuntimeIdentityError> {
    Ok([
        RuntimeImageObservation::capture(RuntimeImageKind::Helper)?,
        RuntimeImageObservation::capture(RuntimeImageKind::Avcodec)?,
        RuntimeImageObservation::capture(RuntimeImageKind::Avformat)?,
        RuntimeImageObservation::capture(RuntimeImageKind::Avutil)?,
        RuntimeImageObservation::capture(RuntimeImageKind::Swscale)?,
    ])
}

pub fn observe_platform() -> Result<RuntimePlatform, RuntimeIdentityError> {
    runtime_ffi::platform()
}
