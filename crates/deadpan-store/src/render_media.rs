//! Durable byte retention for render retries, independent of media admission.
//!
//! A manifest is opaque bounded data here. Neither deserialized references nor
//! a successful retention prove that an MP4 is valid or ready for publication.
//! All preparation belongs on a connection-free worker, under one caller-owned
//! absolute deadline. The SQLite writer only rechecks opaque freshness guards.

use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use deadpan_core::{GeneratedContentId, GeneratedObjectRef};
use deadpan_jobs::Sha256;
use deadpan_jobs::render::RenderAttemptIdentity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Sha256Hasher};
use thiserror::Error;

use crate::object_storage::{
    ObjectControl, ObjectFreshnessGuard, ObjectIdentity, ObjectLimits, ObjectStorage,
    ObjectStorageError, VerifiedObject,
};
use crate::{ProjectStore, StoreError};

pub const MAX_RENDER_MOVIE_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub const MAX_RENDER_MANIFEST_BYTES: u64 = 256 * 1024;
pub const MAX_RENDER_NAMESPACE_ENTRIES: u32 = 100_000;
pub const MAX_RENDER_READ_BYTES: usize = 64 * 1024;

/// A BLAKE3 byte identity scoped to Media/RenderCandidates, never Generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "GeneratedObjectRef")]
pub struct RenderObjectRef(GeneratedObjectRef);

impl RenderObjectRef {
    pub fn new(content: GeneratedContentId, byte_length: u64) -> Result<Self, RenderMediaError> {
        if byte_length == 0 || byte_length > MAX_RENDER_MOVIE_BYTES {
            return Err(RenderMediaError::InvalidIdentity);
        }
        Ok(Self(
            GeneratedObjectRef::new(content, byte_length)
                .map_err(|_| RenderMediaError::InvalidIdentity)?,
        ))
    }

    pub fn content(&self) -> &GeneratedContentId {
        self.0.content()
    }
    pub fn byte_length(&self) -> u64 {
        self.0.byte_length()
    }
    fn identity(&self) -> ObjectIdentity<'_> {
        ObjectIdentity::from(&self.0)
    }
}

impl TryFrom<GeneratedObjectRef> for RenderObjectRef {
    type Error = RenderMediaError;
    fn try_from(value: GeneratedObjectRef) -> Result<Self, Self::Error> {
        Self::new(value.content().clone(), value.byte_length())
    }
}

/// Persistable byte claims. Only a prepared token proves their current durable
/// ownership, and only the separate finished-file verifier admits media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RenderCandidateMediaWire")]
pub struct RenderCandidateMedia {
    movie: RenderObjectRef,
    movie_sha256: Sha256,
    manifest: RenderObjectRef,
    manifest_sha256: Sha256,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderCandidateMediaWire {
    movie: RenderObjectRef,
    movie_sha256: Sha256,
    manifest: RenderObjectRef,
    manifest_sha256: Sha256,
}

impl TryFrom<RenderCandidateMediaWire> for RenderCandidateMedia {
    type Error = RenderMediaError;
    fn try_from(value: RenderCandidateMediaWire) -> Result<Self, Self::Error> {
        Self::new(
            value.movie,
            value.movie_sha256,
            value.manifest,
            value.manifest_sha256,
        )
    }
}

impl RenderCandidateMedia {
    pub fn new(
        movie: RenderObjectRef,
        movie_sha256: Sha256,
        manifest: RenderObjectRef,
        manifest_sha256: Sha256,
    ) -> Result<Self, RenderMediaError> {
        if manifest.byte_length() > MAX_RENDER_MANIFEST_BYTES {
            return Err(RenderMediaError::InvalidIdentity);
        }
        Ok(Self {
            movie,
            movie_sha256,
            manifest,
            manifest_sha256,
        })
    }

    pub fn movie(&self) -> &RenderObjectRef {
        &self.movie
    }
    pub fn movie_sha256(&self) -> &Sha256 {
        &self.movie_sha256
    }
    pub fn manifest(&self) -> &RenderObjectRef {
        &self.manifest
    }
    pub fn manifest_sha256(&self) -> &Sha256 {
        &self.manifest_sha256
    }

    pub fn validate(&self, limits: RenderMediaLimits) -> Result<(), RenderMediaError> {
        limits.check_lengths(self.movie.byte_length(), self.manifest.byte_length())
    }
}

/// Logical byte budgets, not a promise of physical free space. Namespace
/// accounting includes unreferenced and interrupted files. Private staging and
/// snapshot copies are bounded separately by maximum_combined_bytes; callers
/// must also bound concurrently retained handles and prepared snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderMediaLimits {
    maximum_movie_bytes: u64,
    maximum_manifest_bytes: u64,
    maximum_combined_bytes: u64,
    maximum_namespace_bytes: u64,
    maximum_namespace_entries: u32,
}

impl RenderMediaLimits {
    pub fn new(
        maximum_movie_bytes: u64,
        maximum_manifest_bytes: u64,
        maximum_combined_bytes: u64,
        maximum_namespace_bytes: u64,
        maximum_namespace_entries: u32,
    ) -> Result<Self, RenderMediaError> {
        if maximum_movie_bytes == 0
            || maximum_movie_bytes > MAX_RENDER_MOVIE_BYTES
            || maximum_manifest_bytes == 0
            || maximum_manifest_bytes > MAX_RENDER_MANIFEST_BYTES
            || maximum_combined_bytes == 0
            || maximum_combined_bytes > MAX_RENDER_MOVIE_BYTES + MAX_RENDER_MANIFEST_BYTES
            || maximum_namespace_bytes == 0
            || maximum_namespace_entries == 0
            || maximum_namespace_entries > MAX_RENDER_NAMESPACE_ENTRIES
        {
            return Err(RenderMediaError::InvalidLimits);
        }
        Ok(Self {
            maximum_movie_bytes,
            maximum_manifest_bytes,
            maximum_combined_bytes,
            maximum_namespace_bytes,
            maximum_namespace_entries,
        })
    }

    pub const fn maximum_movie_bytes(self) -> u64 {
        self.maximum_movie_bytes
    }
    pub const fn maximum_manifest_bytes(self) -> u64 {
        self.maximum_manifest_bytes
    }
    pub const fn maximum_combined_bytes(self) -> u64 {
        self.maximum_combined_bytes
    }
    pub const fn maximum_namespace_bytes(self) -> u64 {
        self.maximum_namespace_bytes
    }
    pub const fn maximum_namespace_entries(self) -> u32 {
        self.maximum_namespace_entries
    }

    fn check_lengths(self, movie: u64, manifest: u64) -> Result<(), RenderMediaError> {
        if movie == 0
            || manifest == 0
            || movie > self.maximum_movie_bytes
            || manifest > self.maximum_manifest_bytes
            || movie
                .checked_add(manifest)
                .is_none_or(|sum| sum > self.maximum_combined_bytes)
        {
            return Err(RenderMediaError::Capacity);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RenderReadHandle {
    storage: Arc<ObjectStorage>,
    closed: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
pub struct RenderWriteHandle {
    reader: RenderReadHandle,
}

/// One admitted host workflow across preflight, encoding and publication.
/// Dropping an unreleased lease keeps this writer session fenced. Destruction
/// alone cannot establish that owned workers stopped. Reopening creates a new
/// session but does not prove that any previously unknown process has exited.
#[must_use = "retain the lease until all workflow work has stopped"]
pub struct RenderWorkflowLease {
    claimed: Arc<AtomicBool>,
}

impl RenderWorkflowLease {
    /// Release only after the host has observed all owned workflow work stop.
    /// Journal failures may still require recovery; this is execution admission,
    /// not a declaration that publication succeeded or that recovery is complete.
    pub fn release(self) {
        self.claimed.store(false, Ordering::Release);
    }
}

/// Unforgeable within the public API: only complete durable publication and
/// independent hash verification can construct this exact-session token.
pub struct PreparedRenderRetention {
    handle: RenderReadHandle,
    identity: RenderAttemptIdentity,
    media: RenderCandidateMedia,
    movie_guard: ObjectFreshnessGuard,
    manifest_guard: ObjectFreshnessGuard,
}

impl PreparedRenderRetention {
    pub fn media(&self) -> &RenderCandidateMedia {
        &self.media
    }
    pub fn identity(&self) -> &RenderAttemptIdentity {
        &self.identity
    }

    /// Short writer-side check; no bytes are read or hashed. Call immediately
    /// before beginning the checkpoint and immediately before committing it.
    pub(crate) fn validate_for(
        &self,
        storage: &Arc<ObjectStorage>,
        closed: &Arc<AtomicBool>,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<(), RenderMediaError> {
        if !Arc::ptr_eq(storage, &self.handle.storage) || !Arc::ptr_eq(closed, &self.handle.closed)
        {
            return Err(RenderMediaError::WrongSession);
        }
        let control = self.handle.control(cancelled, deadline);
        control.check()?;
        storage.recheck_guard(&self.movie_guard)?;
        control.check()?;
        storage.recheck_guard(&self.manifest_guard)?;
        control.check()?;
        Ok(())
    }
}

/// Private, independently hashed bytes suitable for a fresh verifier input.
/// No path, mutable file descriptor, or media-validity constructor escapes.
#[derive(Debug)]
pub struct PreparedRenderSnapshot {
    handle: RenderReadHandle,
    media: RenderCandidateMedia,
    movie: VerifiedObject,
    manifest: Vec<u8>,
}

impl PreparedRenderSnapshot {
    pub fn media(&self) -> &RenderCandidateMedia {
        &self.media
    }
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest
    }

    pub fn check_live(&self, cancelled: &AtomicBool) -> Result<(), RenderMediaError> {
        self.handle.check_live(cancelled)
    }

    pub fn read_at(
        &self,
        offset: u64,
        buffer: &mut [u8],
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<usize, RenderMediaError> {
        let control = self.handle.control(cancelled, deadline);
        control.check()?;
        if buffer.len() > MAX_RENDER_READ_BYTES || offset > self.media.movie.byte_length() {
            return Err(RenderMediaError::InvalidRead);
        }
        let remaining = self.media.movie.byte_length() - offset;
        let requested = u64::try_from(buffer.len()).map_err(|_| RenderMediaError::InvalidRead)?;
        let count =
            usize::try_from(remaining.min(requested)).map_err(|_| RenderMediaError::InvalidRead)?;
        let read = self
            .movie
            .read_at(&mut buffer[..count], offset)
            .map_err(|source| RenderMediaError::Io {
                operation: "read retained render snapshot",
                source,
            })?;
        control.check()?;
        Ok(read)
    }
}

impl RenderReadHandle {
    pub fn check_live(&self, cancelled: &AtomicBool) -> Result<(), RenderMediaError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ObjectStorageError::SessionClosed.into());
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(ObjectStorageError::Cancelled.into());
        }
        Ok(())
    }

    fn control<'a>(&'a self, cancelled: &'a AtomicBool, deadline: Instant) -> ObjectControl<'a> {
        ObjectControl::bounded(deadline, cancelled).with_closed(&self.closed)
    }

    pub fn snapshot(
        &self,
        expected: &RenderCandidateMedia,
        limits: RenderMediaLimits,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<PreparedRenderSnapshot, RenderMediaError> {
        expected.validate(limits)?;
        let control = self.control(cancelled, deadline);
        control.check()?;
        let movie = self.storage.snapshot_controlled(
            expected.movie.identity(),
            ObjectLimits::new(limits.maximum_movie_bytes)?,
            control,
        )?;
        if checksum(movie.sha256())? != expected.movie_sha256 {
            return Err(RenderMediaError::Sha256Mismatch("movie"));
        }
        let mut manifest = self.storage.snapshot_controlled(
            expected.manifest.identity(),
            ObjectLimits::new(limits.maximum_manifest_bytes)?,
            control,
        )?;
        if checksum(manifest.sha256())? != expected.manifest_sha256 {
            return Err(RenderMediaError::Sha256Mismatch("manifest"));
        }
        let count = usize::try_from(expected.manifest.byte_length())
            .map_err(|_| RenderMediaError::Capacity)?;
        let mut manifest_bytes = vec![0_u8; count];
        for block in manifest_bytes.chunks_mut(MAX_RENDER_READ_BYTES) {
            control.check()?;
            manifest
                .read_exact(block)
                .map_err(|source| RenderMediaError::Io {
                    operation: "read retained render manifest",
                    source,
                })?;
            control.check()?;
        }
        Ok(PreparedRenderSnapshot {
            handle: self.clone(),
            media: expected.clone(),
            movie,
            manifest: manifest_bytes,
        })
    }
}

impl RenderWriteHandle {
    pub fn check_live(&self, cancelled: &AtomicBool) -> Result<(), RenderMediaError> {
        self.reader.check_live(cancelled)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_retention(
        &self,
        identity: &RenderAttemptIdentity,
        movie: &mut impl Read,
        expected_movie_bytes: u64,
        expected_movie_sha256: &Sha256,
        manifest: &[u8],
        limits: RenderMediaLimits,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<PreparedRenderRetention, RenderMediaError> {
        let manifest_bytes =
            u64::try_from(manifest.len()).map_err(|_| RenderMediaError::Capacity)?;
        limits.check_lengths(expected_movie_bytes, manifest_bytes)?;
        let control = self.reader.control(cancelled, deadline);
        control.check()?;
        let lock = self.reader.storage.lock_render_namespace(control)?;
        // Fail an over-capacity or unsafe namespace before copying a movie.
        // The exact reservation follows hashing so existing objects can be
        // credited without guessing a BLAKE3 identity from the supplied SHA.
        lock.reserve(
            &[],
            limits.maximum_namespace_bytes,
            limits.maximum_namespace_entries,
            control,
        )?;
        let (movie_file, movie_ref, movie_sha256) = stage(movie, expected_movie_bytes, control)?;
        if &movie_sha256 != expected_movie_sha256 {
            return Err(RenderMediaError::Sha256Mismatch("movie"));
        }
        let mut manifest_reader = manifest;
        let (manifest_file, manifest_ref, manifest_sha256) =
            stage(&mut manifest_reader, manifest_bytes, control)?;
        let media =
            RenderCandidateMedia::new(movie_ref, movie_sha256, manifest_ref, manifest_sha256)?;
        lock.reserve(
            &[media.movie.identity(), media.manifest.identity()],
            limits.maximum_namespace_bytes,
            limits.maximum_namespace_entries,
            control,
        )?;
        self.reader.storage.promote_file_controlled(
            &movie_file,
            media.movie.identity(),
            ObjectLimits::new(limits.maximum_movie_bytes)?,
            control,
        )?;
        lock.recheck()?;
        control.check()?;
        self.reader.storage.promote_file_controlled(
            &manifest_file,
            media.manifest.identity(),
            ObjectLimits::new(limits.maximum_manifest_bytes)?,
            control,
        )?;
        lock.recheck()?;
        control.check()?;
        let (movie_guard, movie_sha256) = self.reader.storage.guard_controlled(
            media.movie.identity(),
            ObjectLimits::new(limits.maximum_movie_bytes)?,
            control,
        )?;
        let (manifest_guard, manifest_sha256) = self.reader.storage.guard_controlled(
            media.manifest.identity(),
            ObjectLimits::new(limits.maximum_manifest_bytes)?,
            control,
        )?;
        if checksum(movie_sha256)? != media.movie_sha256
            || checksum(manifest_sha256)? != media.manifest_sha256
        {
            return Err(RenderMediaError::Sha256Mismatch("retained object"));
        }
        lock.recheck()?;
        let prepared = PreparedRenderRetention {
            handle: self.reader.clone(),
            identity: identity.clone(),
            media,
            movie_guard,
            manifest_guard,
        };
        prepared.validate_for(
            &self.reader.storage,
            &self.reader.closed,
            cancelled,
            deadline,
        )?;
        Ok(prepared)
    }
}

impl ProjectStore {
    /// Admit one workflow, including preparation before any durable attempt and
    /// publication after a Verified attempt. Lower-level journal APIs retain
    /// their separate exact-stage authority checks.
    pub fn acquire_render_workflow(&self) -> Result<RenderWorkflowLease, StoreError> {
        self.require_writer()?;
        self.render_workflow_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                StoreError::RenderJob(
                    "project already has a render workflow or unresolved execution cleanup".into(),
                )
            })?;
        Ok(RenderWorkflowLease {
            claimed: Arc::clone(&self.render_workflow_claimed),
        })
    }

    /// Bind an operational render workflow to this exact writable open. Equal
    /// package paths or project IDs cannot carry authority across writer reopen.
    /// This check performs no media reads or filesystem work.
    pub fn check_render_owner(&self, owner: &RenderReadHandle) -> Result<(), StoreError> {
        self.require_writer()?;
        if !Arc::ptr_eq(&self.render_storage, &owner.storage)
            || !Arc::ptr_eq(&self.render_closed, &owner.closed)
        {
            return Err(RenderMediaError::WrongSession.into());
        }
        owner.check_live(&AtomicBool::new(false))?;
        Ok(())
    }

    pub fn render_read_handle(&self) -> RenderReadHandle {
        RenderReadHandle {
            storage: Arc::clone(&self.render_storage),
            closed: Arc::clone(&self.render_closed),
        }
    }

    pub fn render_write_handle(&self) -> Result<RenderWriteHandle, StoreError> {
        self.require_writer()?;
        Ok(RenderWriteHandle {
            reader: self.render_read_handle(),
        })
    }
}

fn stage(
    input: &mut impl Read,
    expected: u64,
    control: ObjectControl<'_>,
) -> Result<(File, RenderObjectRef, Sha256), RenderMediaError> {
    control.check()?;
    let mut file = tempfile::tempfile().map_err(|source| RenderMediaError::Io {
        operation: "create private render staging file",
        source,
    })?;
    let mut blake = blake3::Hasher::new();
    let mut sha = Sha256Hasher::new();
    let mut count = 0_u64;
    let mut buffer = [0_u8; MAX_RENDER_READ_BYTES];
    loop {
        control.check()?;
        let size = usize::try_from((expected - count).min(MAX_RENDER_READ_BYTES as u64))
            .map_err(|_| RenderMediaError::Capacity)?;
        // Once the exact length is consumed, one byte proves EOF. Never read
        // an unbounded suffix just to report an invalid stream.
        let result = input.read(&mut buffer[..size.max(1)]);
        control.check()?;
        let read = result.map_err(|source| RenderMediaError::Io {
            operation: "read render retention input",
            source,
        })?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(u64::try_from(read).map_err(|_| RenderMediaError::Capacity)?)
            .ok_or(RenderMediaError::Capacity)?;
        if count > expected {
            return Err(RenderMediaError::LengthMismatch {
                expected,
                actual: count,
            });
        }
        file.write_all(&buffer[..read])
            .map_err(|source| RenderMediaError::Io {
                operation: "write private render staging file",
                source,
            })?;
        blake.update(&buffer[..read]);
        sha.update(&buffer[..read]);
        control.check()?;
    }
    if count != expected {
        return Err(RenderMediaError::LengthMismatch {
            expected,
            actual: count,
        });
    }
    let content = GeneratedContentId::new(blake.finalize().to_hex().to_string())
        .map_err(|_| RenderMediaError::InvalidIdentity)?;
    let reference = RenderObjectRef::new(content, count)?;
    control.check()?;
    Ok((file, reference, checksum(sha.finalize().into())?))
}

fn checksum(bytes: [u8; 32]) -> Result<Sha256, RenderMediaError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        hex.push(char::from(HEX[usize::from(byte >> 4)]));
        hex.push(char::from(HEX[usize::from(byte & 15)]));
    }
    Sha256::new(hex).map_err(|_| RenderMediaError::InvalidIdentity)
}

#[derive(Debug, Error)]
pub enum RenderMediaError {
    #[error("invalid render byte identity")]
    InvalidIdentity,
    #[error("invalid render retention limits")]
    InvalidLimits,
    #[error("render bytes exceed the configured movie, manifest or combined budget")]
    Capacity,
    #[error("render snapshot read exceeds the bounded read interval")]
    InvalidRead,
    #[error("render retention belongs to another store session")]
    WrongSession,
    #[error("render {0} SHA-256 does not match retained evidence")]
    Sha256Mismatch(&'static str),
    #[error("render input length is {actual}, expected {expected}")]
    LengthMismatch { expected: u64, actual: u64 },
    #[error(transparent)]
    Storage(#[from] ObjectStorageError),
    #[error("{operation} failed")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
}

impl RenderMediaError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidIdentity => "RenderMediaIdentityInvalid",
            Self::InvalidLimits => "RenderMediaLimitsInvalid",
            Self::Capacity => "RenderMediaCapacity",
            Self::InvalidRead => "RenderMediaReadInvalid",
            Self::WrongSession => "RenderMediaWrongSession",
            Self::Sha256Mismatch(_) => "RenderMediaSha256Mismatch",
            Self::LengthMismatch { .. } => "RenderMediaLengthMismatch",
            Self::Storage(error) => error.code(),
            Self::Io { source, .. } => match source.kind() {
                io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => "DiskFull",
                io::ErrorKind::ReadOnlyFilesystem => "ProjectReadOnly",
                io::ErrorKind::PermissionDenied => "PermissionDenied",
                _ => "IoFailure",
            },
        }
    }
}

#[cfg(test)]
mod tests;
