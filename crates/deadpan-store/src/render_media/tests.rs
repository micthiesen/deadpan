use std::fs;
use std::io::Cursor;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

use deadpan_jobs::{AttemptId, CancellationToken, RequestId};

use super::*;
use crate::object_storage::StorageNamespace;

const MOVIE: &[u8] = b"opaque complete movie bytes";
const MANIFEST: &[u8] = br#"{"schema":1,"claim":"still requires media verification"}"#;

fn package() -> tempfile::TempDir {
    let package = tempfile::tempdir().unwrap();
    fs::create_dir(package.path().join("Media")).unwrap();
    package
}

fn handles(package: &Path) -> (RenderReadHandle, RenderWriteHandle) {
    let reader = RenderReadHandle {
        storage: Arc::new(
            ObjectStorage::open(package, StorageNamespace::RenderCandidates).unwrap(),
        ),
        closed: Arc::new(AtomicBool::new(false)),
    };
    let writer = RenderWriteHandle {
        reader: reader.clone(),
    };
    (reader, writer)
}

fn identity() -> RenderAttemptIdentity {
    RenderAttemptIdentity {
        job_id: RequestId::new("render-job").unwrap(),
        attempt_id: AttemptId::new("encoding-attempt").unwrap(),
        cancellation_token: CancellationToken::new("render-cancel").unwrap(),
        expected_sequence: 1,
    }
}

fn limits() -> RenderMediaLimits {
    RenderMediaLimits::new(1024, 1024, 2048, 8192, 20).unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn digest(bytes: &[u8]) -> Sha256 {
    checksum(Sha256Hasher::digest(bytes).into()).unwrap()
}

fn reference(bytes: &[u8]) -> RenderObjectRef {
    RenderObjectRef::new(
        GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
        u64::try_from(bytes.len()).unwrap(),
    )
    .unwrap()
}

fn claimed_media() -> RenderCandidateMedia {
    RenderCandidateMedia::new(
        reference(MOVIE),
        digest(MOVIE),
        reference(MANIFEST),
        digest(MANIFEST),
    )
    .unwrap()
}

fn object_path(package: &Path, reference: &RenderObjectRef) -> PathBuf {
    package
        .join("Media/RenderCandidates")
        .join(format!("blake3-{}", reference.content().digest()))
}

fn retain(
    writer: &RenderWriteHandle,
    limits: RenderMediaLimits,
) -> Result<PreparedRenderRetention, RenderMediaError> {
    writer.prepare_retention(
        &identity(),
        &mut Cursor::new(MOVIE),
        MOVIE.len() as u64,
        &digest(MOVIE),
        MANIFEST,
        limits,
        &AtomicBool::new(false),
        deadline(),
    )
}

#[test]
fn references_are_strict_bounded_claims() {
    let media = claimed_media();
    let json = serde_json::to_value(&media).unwrap();
    assert_eq!(
        serde_json::from_value::<RenderCandidateMedia>(json.clone()).unwrap(),
        media
    );
    let mut unknown = json.clone();
    unknown["verified"] = true.into();
    assert!(serde_json::from_value::<RenderCandidateMedia>(unknown).is_err());
    let mut too_large = json.clone();
    too_large["manifest"]["byte_length"] = (MAX_RENDER_MANIFEST_BYTES + 1).into();
    assert!(serde_json::from_value::<RenderCandidateMedia>(too_large).is_err());
    let mut zero = json.clone();
    zero["movie"]["byte_length"] = 0.into();
    assert!(serde_json::from_value::<RenderCandidateMedia>(zero).is_err());
    let mut wrong_algorithm = json;
    wrong_algorithm["movie"]["content"]["algorithm"] = "sha256".into();
    assert!(serde_json::from_value::<RenderCandidateMedia>(wrong_algorithm).is_err());
    assert!(RenderMediaLimits::new(MAX_RENDER_MOVIE_BYTES + 1, 1, 1, 1, 1).is_err());
    assert!(RenderMediaLimits::new(1, MAX_RENDER_MANIFEST_BYTES + 1, 1, 1, 1).is_err());
    assert!(RenderMediaLimits::new(1, 1, 2, 2, MAX_RENDER_NAMESPACE_ENTRIES + 1).is_err());
}

#[test]
fn read_only_open_never_creates_render_namespace() {
    let package = package();
    let (reader, _) = handles(package.path());
    assert!(
        reader
            .snapshot(
                &claimed_media(),
                limits(),
                &AtomicBool::new(false),
                deadline()
            )
            .is_err()
    );
    assert!(!package.path().join("Media/RenderCandidates").exists());
    assert!(!package.path().join(".render-candidates.lock").exists());
}

#[test]
fn retention_deduplicates_at_exact_namespace_budget_and_snapshots_independently() {
    let package = package();
    let (reader, writer) = handles(package.path());
    let exact = (MOVIE.len() + MANIFEST.len()) as u64;
    let budget = RenderMediaLimits::new(1024, 1024, exact, exact, 2).unwrap();
    let retained = retain(&writer, budget).unwrap();
    assert_eq!(retained.media(), &claimed_media());
    assert_eq!(retained.identity(), &identity());
    retained
        .validate_for(
            &reader.storage,
            &reader.closed,
            &AtomicBool::new(false),
            deadline(),
        )
        .unwrap();
    let duplicate = retain(&writer, budget).unwrap();
    assert_eq!(duplicate.media(), retained.media());
    assert_eq!(
        fs::read_dir(package.path().join("Media/RenderCandidates"))
            .unwrap()
            .count(),
        2
    );
    let snapshot = reader
        .snapshot(
            retained.media(),
            budget,
            &AtomicBool::new(false),
            deadline(),
        )
        .unwrap();
    assert_eq!(snapshot.manifest_bytes(), MANIFEST);
    fs::remove_file(object_path(package.path(), retained.media().movie())).unwrap();
    let mut bytes = vec![0; MOVIE.len()];
    assert_eq!(
        snapshot
            .read_at(0, &mut bytes, &AtomicBool::new(false), deadline())
            .unwrap(),
        MOVIE.len()
    );
    assert_eq!(bytes, MOVIE);
    assert!(
        retained
            .validate_for(
                &reader.storage,
                &reader.closed,
                &AtomicBool::new(false),
                deadline()
            )
            .is_err()
    );
    assert!(!package.path().join("Media/Generated").exists());
}

#[test]
fn token_rejects_another_session_even_with_identical_package_and_bytes() {
    let package = package();
    let (reader, writer) = handles(package.path());
    let retained = retain(&writer, limits()).unwrap();
    let (replacement, _) = handles(package.path());
    assert!(matches!(
        retained.validate_for(
            &replacement.storage,
            &replacement.closed,
            &AtomicBool::new(false),
            deadline()
        ),
        Err(RenderMediaError::WrongSession)
    ));
    let snapshot = reader
        .snapshot(
            retained.media(),
            limits(),
            &AtomicBool::new(false),
            deadline(),
        )
        .unwrap();
    reader.closed.store(true, Ordering::Release);
    assert_eq!(
        retained
            .validate_for(
                &reader.storage,
                &reader.closed,
                &AtomicBool::new(false),
                deadline()
            )
            .unwrap_err()
            .code(),
        "StorageSessionClosed"
    );
    assert_eq!(
        snapshot
            .read_at(0, &mut [0; 1], &AtomicBool::new(false), deadline())
            .unwrap_err()
            .code(),
        "StorageSessionClosed"
    );
    assert!(
        replacement
            .snapshot(
                retained.media(),
                limits(),
                &AtomicBool::new(false),
                deadline()
            )
            .is_ok()
    );
}

#[test]
fn wrong_hash_truncation_extra_bytes_and_combined_budget_publish_nothing() {
    for (bytes, declared, hash, budget) in [
        (MOVIE, MOVIE.len() as u64, digest(b"wrong"), limits()),
        (
            &MOVIE[..MOVIE.len() - 1],
            MOVIE.len() as u64,
            digest(MOVIE),
            limits(),
        ),
        (MOVIE, MOVIE.len() as u64 - 1, digest(MOVIE), limits()),
        (
            MOVIE,
            MOVIE.len() as u64,
            digest(MOVIE),
            RenderMediaLimits::new(1024, 1024, 1, 8192, 20).unwrap(),
        ),
    ] {
        let package = package();
        let (_, writer) = handles(package.path());
        assert!(
            writer
                .prepare_retention(
                    &identity(),
                    &mut Cursor::new(bytes),
                    declared,
                    &hash,
                    MANIFEST,
                    budget,
                    &AtomicBool::new(false),
                    deadline()
                )
                .is_err()
        );
        let directory = package.path().join("Media/RenderCandidates");
        assert!(!directory.exists() || fs::read_dir(directory).unwrap().next().is_none());
    }
}

#[test]
fn pending_and_orphan_bytes_and_entry_count_consume_namespace_capacity() {
    for (orphan, bytes, entries) in [
        (b"orphan".as_slice(), 1_u64, 10_u32),
        (b"".as_slice(), 8192, 1),
    ] {
        let package = package();
        let (_, writer) = handles(package.path());
        let directory = package.path().join("Media/RenderCandidates");
        fs::create_dir(&directory).unwrap();
        let pending = directory.join(".pending-interrupted");
        fs::write(&pending, orphan).unwrap();
        let budget = RenderMediaLimits::new(1024, 1024, 2048, bytes, entries).unwrap();
        assert_eq!(
            retain(&writer, budget).err().unwrap().code(),
            "RenderMediaNamespaceCapacity"
        );
        assert_eq!(fs::read(&pending).unwrap(), orphan);
        assert_eq!(fs::read_dir(directory).unwrap().count(), 1);
    }
}

#[test]
fn symlinks_hardlinks_and_unsafe_namespace_entries_are_rejected() {
    let package = package();
    let elsewhere = tempfile::tempdir().unwrap();
    let (_, writer) = handles(package.path());
    symlink(
        elsewhere.path(),
        package.path().join("Media/RenderCandidates"),
    )
    .unwrap();
    assert!(retain(&writer, limits()).is_err());
    assert!(fs::read_dir(elsewhere.path()).unwrap().next().is_none());
    fs::remove_file(package.path().join("Media/RenderCandidates")).unwrap();
    fs::create_dir(package.path().join("Media/RenderCandidates")).unwrap();
    let foreign = package.path().join("foreign");
    fs::write(&foreign, b"preserve me").unwrap();
    let linked = package
        .path()
        .join("Media/RenderCandidates/.pending-foreign");
    fs::hard_link(&foreign, &linked).unwrap();
    assert!(retain(&writer, limits()).is_err());
    assert_eq!(fs::read(foreign).unwrap(), b"preserve me");
    assert!(linked.exists());
}

#[test]
fn manifest_failure_preserves_movie_already_published() {
    let package = package();
    let (_, writer) = handles(package.path());
    fs::create_dir(package.path().join("Media/RenderCandidates")).unwrap();
    let path = object_path(package.path(), &reference(MANIFEST));
    fs::write(&path, vec![b'x'; MANIFEST.len()]).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(retain(&writer, limits()).is_err());
    assert_eq!(
        fs::read(object_path(package.path(), &reference(MOVIE))).unwrap(),
        MOVIE
    );
    assert_eq!(fs::read(path).unwrap(), vec![b'x'; MANIFEST.len()]);
}

#[test]
fn snapshot_rehashes_both_objects_and_checks_declared_sha256() {
    let package = package();
    let (reader, writer) = handles(package.path());
    let retained = retain(&writer, limits()).unwrap();
    let mut forged = retained.media().clone();
    forged.movie_sha256 = digest(b"wrong movie");
    assert_eq!(
        reader
            .snapshot(&forged, limits(), &AtomicBool::new(false), deadline())
            .err()
            .unwrap()
            .code(),
        "RenderMediaSha256Mismatch"
    );
    forged = retained.media().clone();
    forged.manifest_sha256 = digest(b"wrong manifest");
    assert_eq!(
        reader
            .snapshot(&forged, limits(), &AtomicBool::new(false), deadline())
            .err()
            .unwrap()
            .code(),
        "RenderMediaSha256Mismatch"
    );
    let path = object_path(package.path(), retained.media().movie());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&path, vec![b'z'; MOVIE.len()]).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(
        reader
            .snapshot(
                retained.media(),
                limits(),
                &AtomicBool::new(false),
                deadline()
            )
            .is_err()
    );
    assert!(
        retained
            .validate_for(
                &reader.storage,
                &reader.closed,
                &AtomicBool::new(false),
                deadline()
            )
            .is_err()
    );
}

struct CancelOnRead<'a> {
    cancelled: &'a AtomicBool,
}
impl Read for CancelOnRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        buffer[0] = b'a';
        self.cancelled.store(true, Ordering::Release);
        Ok(1)
    }
}

#[test]
fn cancellation_and_absolute_deadline_cannot_return_prepared_bytes() {
    let package = package();
    let (reader, writer) = handles(package.path());
    let cancelled = AtomicBool::new(false);
    let mut input = CancelOnRead {
        cancelled: &cancelled,
    };
    assert_eq!(
        writer
            .prepare_retention(
                &identity(),
                &mut input,
                1,
                &digest(b"a"),
                MANIFEST,
                limits(),
                &cancelled,
                deadline()
            )
            .err()
            .unwrap()
            .code(),
        "OperationCancelled"
    );
    assert!(
        fs::read_dir(package.path().join("Media/RenderCandidates"))
            .unwrap()
            .next()
            .is_none()
    );
    cancelled.store(false, Ordering::Release);
    assert_eq!(
        writer
            .prepare_retention(
                &identity(),
                &mut Cursor::new(MOVIE),
                MOVIE.len() as u64,
                &digest(MOVIE),
                MANIFEST,
                limits(),
                &cancelled,
                Instant::now()
            )
            .err()
            .unwrap()
            .code(),
        "DeadlineExceeded"
    );
    let retained = retain(&writer, limits()).unwrap();
    let snapshot = reader
        .snapshot(retained.media(), limits(), &cancelled, deadline())
        .unwrap();
    assert_eq!(
        snapshot
            .read_at(0, &mut [0; 1], &cancelled, Instant::now())
            .unwrap_err()
            .code(),
        "DeadlineExceeded"
    );
    assert!(
        snapshot
            .read_at(MOVIE.len() as u64 + 1, &mut [0; 1], &cancelled, deadline())
            .is_err()
    );
    assert!(
        snapshot
            .read_at(
                0,
                &mut vec![0; MAX_RENDER_READ_BYTES + 1],
                &cancelled,
                deadline()
            )
            .is_err()
    );
    assert_eq!(
        snapshot
            .read_at(MOVIE.len() as u64, &mut [0; 1], &cancelled, deadline())
            .unwrap(),
        0
    );
}

#[test]
fn revoked_worker_lock_serializes_replacement_session_until_guard_drops() {
    let package = package();
    let (old, _) = handles(package.path());
    let (replacement, _) = handles(package.path());
    let cancelled = AtomicBool::new(false);
    let lock = old
        .storage
        .lock_render_namespace(old.control(&cancelled, deadline()))
        .unwrap();
    old.closed.store(true, Ordering::Release);
    let short = Instant::now() + Duration::from_millis(20);
    assert!(matches!(
        replacement
            .storage
            .lock_render_namespace(replacement.control(&cancelled, short)),
        Err(ObjectStorageError::DeadlineExceeded)
    ));
    drop(lock);
    assert!(
        replacement
            .storage
            .lock_render_namespace(replacement.control(&cancelled, deadline()))
            .is_ok()
    );
}

#[test]
fn namespace_replacement_and_lock_replacement_are_detected() {
    for replace_lock in [false, true] {
        let package = package();
        let (reader, _) = handles(package.path());
        let cancelled = AtomicBool::new(false);
        let lock = reader
            .storage
            .lock_render_namespace(reader.control(&cancelled, deadline()))
            .unwrap();
        let path = package.path().join(if replace_lock {
            ".render-candidates.lock"
        } else {
            "Media/RenderCandidates"
        });
        fs::rename(&path, path.with_extension("retained")).unwrap();
        if replace_lock {
            fs::write(&path, []).unwrap();
        } else {
            fs::create_dir(&path).unwrap();
        }
        assert!(lock.recheck().is_err());
    }
}
