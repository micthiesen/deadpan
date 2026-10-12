use super::*;
use std::os::unix::fs::{FileExt, PermissionsExt, symlink};

fn key(value: u8) -> CacheKey {
    CacheKey([value; 32])
}
fn pcm(range: std::ops::Range<usize>) -> io::Result<Vec<f32>> {
    Ok(range
        .flat_map(|n| [n as f32 / 10000.0, -(n as f32) / 10000.0])
        .collect())
}

#[test]
fn restart_reads_exact_private_pcm_and_clear_preserves_the_result() {
    let root = tempfile::tempdir().unwrap();
    let cache = PcmCache::at(root.path()).unwrap();
    cache.store(key(1), 5000, pcm).unwrap();
    drop(cache);
    let cache = PcmCache::at(root.path()).unwrap();
    let mut actual: Vec<f32> = Vec::new();
    assert!(
        cache
            .restore(key(1), 5000, |block| {
                actual.extend(block.iter().flatten());
                Ok(())
            })
            .unwrap()
    );
    assert_eq!(actual, pcm(0..5000).unwrap());
    let entries = cache.entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].bytes, HEADER as u64 + 5000 * 8);
    assert_eq!(
        cache.cleanup(Duration::ZERO, true).unwrap().removed.len(),
        1
    );
    assert_eq!(cache.entries().unwrap().len(), 1);
    assert_eq!(
        cache.cleanup(Duration::ZERO, false).unwrap().removed.len(),
        1
    );
    assert_eq!(actual, pcm(0..5000).unwrap());
    assert!(
        !cache
            .restore(key(1), 5000, |_| panic!("cleared cache read"))
            .unwrap()
    );
}

#[test]
fn cleanup_and_replacement_skip_an_active_reader() {
    let root = tempfile::tempdir().unwrap();
    let cache = PcmCache::at(root.path()).unwrap();
    cache.store(key(2), 5000, pcm).unwrap();
    let mut checked = false;
    cache
        .restore(key(2), 5000, |_| {
            if !checked {
                let removed = cache.cleanup(Duration::ZERO, false)?;
                assert!(removed.removed.is_empty());
                assert_eq!(removed.kept_in_use, vec![key(2).filename()]);
                assert_eq!(
                    cache.store(key(2), 5000, pcm).unwrap_err().kind(),
                    io::ErrorKind::WouldBlock
                );
                checked = true;
            }
            Ok(())
        })
        .unwrap();
    assert!(checked);
    assert_eq!(
        cache.entries().unwrap().len(),
        1,
        "aborted replacement cleaned staging"
    );
}

#[test]
fn incomplete_writes_and_cancelled_reads_never_publish() {
    let root = tempfile::tempdir().unwrap();
    let cache = PcmCache::at(root.path()).unwrap();
    let result = cache.store(key(3), 5000, |range| {
        if range.start > 0 {
            Err(io::Error::other("cancelled"))
        } else {
            pcm(range)
        }
    });
    assert!(result.is_err());
    assert!(cache.entries().unwrap().is_empty());
    cache.store(key(3), 5000, pcm).unwrap();
    assert!(
        cache
            .restore(key(3), 5000, |_| Err(io::Error::other("cancelled")))
            .is_err()
    );
    assert_eq!(cache.entries().unwrap().len(), 1);
    assert_eq!(
        cache.cleanup(Duration::ZERO, false).unwrap().removed.len(),
        1
    );
}

#[test]
fn corrupt_payloads_headers_and_lengths_reject_before_use() {
    let root = tempfile::tempdir().unwrap();
    let cache = PcmCache::at(root.path()).unwrap();
    for (index, offset) in [0, 8, 40, 48, HEADER as u64 + 17].into_iter().enumerate() {
        let key = key(index as u8);
        cache.store(key, 5000, pcm).unwrap();
        let path = root.path().join(key.filename());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let file = File::options().write(true).open(path).unwrap();
        file.write_all_at(&[0x7f], offset).unwrap();
        assert_eq!(
            cache.restore(key, 5000, |_| Ok(())).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        // A complete rebuild atomically replaces the damaged object.
        cache.store(key, 5000, pcm).unwrap();
        assert!(cache.restore(key, 5000, |_| Ok(())).unwrap());
    }
    assert!(
        cache
            .restore(key(0), 4999, |_| panic!("wrong extent"))
            .is_err()
    );
}

#[test]
fn the_shared_budget_reserves_in_progress_output() {
    let root = tempfile::tempdir().unwrap();
    let cache = PcmCache::with_budget(root.path(), 5000 * 8 + HEADER as u64).unwrap();
    let mut checked = false;
    cache
        .store(key(1), 5000, |range| {
            if !checked {
                assert_eq!(
                    cache.store(key(2), 1, pcm).unwrap_err().kind(),
                    io::ErrorKind::StorageFull
                );
                let report = cache.cleanup(Duration::ZERO, false)?;
                assert!(report.removed.is_empty());
                assert_eq!(report.kept_in_use.len(), 1);
                checked = true;
            }
            pcm(range)
        })
        .unwrap();
    assert!(checked);
    assert_eq!(cache.entries().unwrap().len(), 1);
}

#[test]
fn temporary_lock_contention_does_not_disable_a_new_cache_handle() {
    let root = tempfile::tempdir().unwrap();
    let cache = PcmCache::at(root.path()).unwrap();
    cache.store(key(1), 1, pcm).unwrap();
    let lock = cache.lock().unwrap();
    let reopened = PcmCache::at(root.path()).unwrap();
    assert_eq!(
        reopened.restore(key(1), 1, |_| Ok(())).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    drop(lock);
    assert!(reopened.restore(key(1), 1, |_| Ok(())).unwrap());
}

#[test]
fn last_use_and_abandoned_staging_obey_the_grace_period() {
    let root = tempfile::tempdir().unwrap();
    let cache = PcmCache::at(root.path()).unwrap();
    cache.store(key(1), 1, pcm).unwrap();
    let path = cache.path().join(key(1).filename());
    let old = SystemTime::now() - UNUSED_GRACE - Duration::from_secs(10);
    File::open(&path).unwrap().set_modified(old).unwrap();
    assert_eq!(cache.cleanup(UNUSED_GRACE, true).unwrap().removed.len(), 1);
    cache.restore(key(1), 1, |_| Ok(())).unwrap();
    assert!(
        cache
            .cleanup(UNUSED_GRACE, true)
            .unwrap()
            .removed
            .is_empty()
    );
    let partial = cache.path().join(format!(".tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&partial, b"interrupted").unwrap();
    File::open(&partial).unwrap().set_modified(old).unwrap();
    assert_eq!(cache.cleanup(UNUSED_GRACE, false).unwrap().removed.len(), 1);
    assert!(!partial.exists());
    assert!(path.exists());
}

#[test]
fn hostile_entries_and_replaced_roots_cannot_reach_other_files() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("original");
    std::fs::write(&sentinel, b"user media").unwrap();
    let cache = PcmCache::at(&root.path().join("cache")).unwrap();
    let entry = cache.path().join(key(1).filename());
    symlink(&sentinel, &entry).unwrap();
    assert!(cache.restore(key(1), 1, |_| panic!()).is_err());
    assert!(cache.cleanup(Duration::ZERO, false).is_err());
    std::fs::remove_file(&entry).unwrap();
    std::fs::hard_link(&sentinel, &entry).unwrap();
    assert!(cache.store(key(1), 1, pcm).is_err());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"user media");
    std::fs::rename(cache.path(), root.path().join("moved")).unwrap();
    std::fs::create_dir(cache.path()).unwrap();
    assert!(cache.store(key(2), 1, pcm).is_err());
    assert!(std::fs::read_dir(cache.path()).unwrap().next().is_none());
}
