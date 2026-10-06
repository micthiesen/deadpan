use std::io::Read;
use std::os::unix::fs::PermissionsExt;

use deadpan_core::{
    AssetId, IndexedSourceFrame, SourceFrameId, SourceFrameIndex, SourceTimeBase,
    TerminalProvenance,
};
use deadpan_media::proxy::{
    PROXY_ENCODER, PROXY_SIDECAR_SCHEMA, ProxyFidelity, ProxyFileIdentity, ProxyOriginal,
    ProxyReason,
};
use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
use deadpan_source::{
    ColorMatrix, ColorMetadata, ColorPrimaries, ColorRange, ColorTransfer, SourceStreamInfo,
};

use super::*;

fn cache(scratch: &Path) -> ProxyCache {
    ProxyCache::at(&scratch.join("Proxies")).unwrap()
}

fn key(digit: char) -> ProxyKey {
    ProxyKey::new(&digit.to_string().repeat(64), 0).unwrap()
}

/// A structurally valid sidecar for `bytes` under `key`. The cache checks
/// identity, length and hash; media verification is not its job.
pub(crate) fn sidecar(key: &ProxyKey, bytes: &[u8]) -> ProxySidecar {
    use sha2::Digest;
    let sha: [u8; 32] = sha2::Sha256::digest(bytes).into();
    let index = SourceFrameIndex::new(
        AssetId::new("proxy").unwrap(),
        SourceTimeBase::new(1, 30).unwrap(),
        vec![IndexedSourceFrame {
            identity: SourceFrameId(0),
            pts: 0,
            reported_duration: Some(1),
            keyframe: true,
            seek_from: Some(SourceFrameId(0)),
            decode_timestamp: Some(0),
        }],
        1,
        TerminalProvenance::DecodedFrameDuration,
    )
    .unwrap();
    ProxySidecar {
        schema: PROXY_SIDECAR_SCHEMA,
        recipe: PROXY_RECIPE_VERSION,
        encoder: PROXY_ENCODER.into(),
        reason: ProxyReason::Raster,
        original: ProxyOriginal {
            blake3: key.original_blake3().into(),
            sha256: "0".repeat(64),
            byte_length: 10,
            stream_index: 0,
        },
        file: ProxyFileIdentity {
            sha256: deadpan_media::proxy::hex(&sha),
            blake3: "0".repeat(64),
            byte_length: bytes.len() as u64,
        },
        info: SourceStreamInfo {
            width: 2,
            height: 2,
            stream_index: 0,
            time_base_num: 1,
            time_base_den: 30,
            sample_aspect_num: 1,
            sample_aspect_den: 1,
            rotation_quarter_turns: 0,
            color: ColorMetadata {
                range: ColorRange::Limited,
                matrix: ColorMatrix::Bt709,
                transfer: ColorTransfer::Bt709,
                primaries: ColorPrimaries::Bt709,
                mastering: None,
                content_light: None,
                ignored_static: Default::default(),
            },
            codec: "h264".into(),
            pixel_format: "yuv420p".into(),
            stream_start: Some(0),
            stream_duration: Some(1),
            container_start: None,
            container_duration: None,
            audio_streams: Vec::new(),
        },
        index: SourceIndexSnapshot::new(
            SourceContentIdentity::new(sha, bytes.len() as u64).unwrap(),
            0,
            index,
        )
        .unwrap(),
        fidelity: ProxyFidelity {
            samples: vec![0],
            mean_abs_difference_milli: 0,
            max_block_difference_milli: 0,
            bias_milli: [0; 3],
            widest_sample: 0,
            widest_luma_spread_milli: 0,
            widest_chroma_spread_milli: 0,
        },
    }
}

fn publish(cache: &ProxyCache, key: &ProxyKey, bytes: &[u8]) -> ProxyEntry {
    use std::io::Write;
    let staging = cache.stage().unwrap();
    staging.movie().write_all(bytes).unwrap();
    cache.publish(key, staging, &sidecar(key, bytes)).unwrap()
}

fn not_cancelled() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn only_complete_entries_are_visible_and_publication_swaps_atomically() {
    use std::io::Write;
    let scratch = tempfile::tempdir().unwrap();
    let cache = cache(scratch.path());
    let key = key('a');
    assert!(cache.lookup(&key).unwrap().is_none());
    // A staged, unpublished movie is never an entry; dropping removes it.
    let staging = cache.stage().unwrap();
    staging.movie().write_all(b"partial").unwrap();
    assert!(cache.lookup(&key).unwrap().is_none());
    drop(staging);
    assert_eq!(
        std::fs::read_dir(scratch.path().join("Proxies/.staging"))
            .unwrap()
            .count(),
        0
    );
    let first = publish(&cache, &key, b"first movie");
    let found = cache.lookup(&key).unwrap().unwrap();
    assert_eq!(found.sidecar(), first.sidecar());
    // A reader of the first movie keeps reading it across a replacement.
    let reader = cache.open(&found, &not_cancelled()).unwrap();
    publish(&cache, &key, b"second, longer movie");
    let mut old = String::new();
    reader.into_file().read_to_string(&mut old).unwrap();
    assert_eq!(old, "first movie");
    let replaced = cache.lookup(&key).unwrap().unwrap();
    assert_eq!(replaced.sidecar().file.byte_length, 20);
    let mut new = String::new();
    cache
        .open(&replaced, &not_cancelled())
        .unwrap()
        .into_file()
        .read_to_string(&mut new)
        .unwrap();
    assert_eq!(new, "second, longer movie");
    // The replaced entry went through the trash and is gone once unread.
    cache.cleanup(&[], ProxyCleanupPolicy::default()).unwrap();
    assert_eq!(
        std::fs::read_dir(scratch.path().join("Proxies/.trash"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn bytes_are_hashed_once_per_file_state_and_changed_bytes_are_damaged() {
    let scratch = tempfile::tempdir().unwrap();
    let cache = cache(scratch.path());
    let key = key('b');
    let entry = publish(&cache, &key, b"movie bytes");
    let directory = scratch.path().join("Proxies").join(key.directory());
    assert!(!directory.join("verified.json").exists());
    cache.open(&entry, &not_cancelled()).unwrap();
    assert!(directory.join("verified.json").exists());
    // A recorded state is not rehashed: cancellation would stop a hash.
    cache.open(&entry, &AtomicBool::new(true)).unwrap();
    // Same length, different bytes, new state: hashed again and refused.
    let movie = directory.join("proxy.mp4");
    std::fs::set_permissions(&movie, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&movie, b"MOVIE BYTES").unwrap();
    assert!(matches!(
        cache.open(&entry, &not_cancelled()),
        Err(ProxyCacheError::Damaged(_))
    ));
    // A truncated movie is damaged at lookup.
    std::fs::write(&movie, b"short").unwrap();
    assert!(matches!(
        cache.lookup(&key),
        Err(ProxyCacheError::Damaged(_))
    ));
    assert!(cache.remove(&key).unwrap());
    assert!(cache.lookup(&key).unwrap().is_none());
    assert!(!cache.remove(&key).unwrap());
}

#[test]
fn a_sidecar_for_another_key_is_refused_and_symlinks_are_not_followed() {
    use std::io::Write;
    let scratch = tempfile::tempdir().unwrap();
    let cache = cache(scratch.path());
    let staging = cache.stage().unwrap();
    staging.movie().write_all(b"x").unwrap();
    assert!(
        cache
            .publish(&key('c'), staging, &sidecar(&key('d'), b"x"))
            .is_err()
    );
    // An entry name that is a symbolic link is damaged, never traversed.
    let outside = scratch.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(
        &outside,
        scratch.path().join("Proxies").join(key('e').directory()),
    )
    .unwrap();
    assert!(matches!(
        cache.lookup(&key('e')),
        Err(ProxyCacheError::Damaged(_))
    ));
    cache
        .cleanup(
            &[],
            ProxyCleanupPolicy {
                unused_grace: Duration::ZERO,
                ..ProxyCleanupPolicy::default()
            },
        )
        .unwrap();
    assert!(outside.is_dir(), "cleanup removed the link, not its target");
}

#[test]
fn cleanup_keeps_retained_and_in_use_entries_and_enforces_grace_and_budget() {
    let scratch = tempfile::tempdir().unwrap();
    let cache = cache(scratch.path());
    let (current, old, in_use, extra) = (key('1'), key('2'), key('3'), key('4'));
    publish(&cache, &current, &[1; 100]);
    publish(&cache, &old, &[2; 100]);
    let reading = publish(&cache, &in_use, &[3; 100]);
    let reader = cache.open(&reading, &not_cancelled()).unwrap();
    // Within the unused grace nothing is evicted.
    let report = cache
        .cleanup(
            std::slice::from_ref(&current),
            ProxyCleanupPolicy::default(),
        )
        .unwrap();
    assert!(report.removed_entries.is_empty());
    // Past it, unread entries go; one being read stays.
    let immediate = ProxyCleanupPolicy {
        staging_grace: Duration::ZERO,
        unused_grace: Duration::ZERO,
        budget_bytes: DEFAULT_PROXY_BUDGET_BYTES,
    };
    let report = cache
        .cleanup(std::slice::from_ref(&current), immediate)
        .unwrap();
    assert_eq!(report.removed_entries, vec![old.directory()]);
    assert_eq!(report.kept_in_use, vec![in_use.directory()]);
    assert!(cache.lookup(&in_use).unwrap().is_some());
    assert!(!cache.remove(&in_use).unwrap(), "removal skips a reader");
    drop(reader);
    // Over budget, least recently used unretained entries go first.
    publish(&cache, &extra, &[4; 100]);
    let tight = ProxyCleanupPolicy {
        budget_bytes: 1,
        ..ProxyCleanupPolicy::default()
    };
    let report = cache
        .cleanup(std::slice::from_ref(&current), tight)
        .unwrap();
    assert_eq!(
        report.removed_entries,
        vec![in_use.directory(), extra.directory()]
    );
    assert!(cache.lookup(&current).unwrap().is_some());
    // Abandoned staging older than its grace is removed.
    std::mem::forget(cache.stage().unwrap());
    assert_eq!(
        cache
            .cleanup(std::slice::from_ref(&current), immediate)
            .unwrap()
            .removed_staging,
        1
    );
    // Entries of another recipe version are stale at once.
    std::fs::create_dir(scratch.path().join("Proxies").join(format!(
        "v{}-{}-s0",
        PROXY_RECIPE_VERSION + 1,
        "5".repeat(64)
    )))
    .unwrap();
    let report = cache
        .cleanup(
            std::slice::from_ref(&current),
            ProxyCleanupPolicy::default(),
        )
        .unwrap();
    assert_eq!(report.removed_entries.len(), 1);
}

#[test]
fn failures_are_remembered_until_published_or_forgotten() {
    let scratch = tempfile::tempdir().unwrap();
    let cache = cache(scratch.path());
    let key = key('f');
    assert_eq!(cache.failure(&key), None);
    cache.record_failure(&key, "encoder refused").unwrap();
    // Another handle (another project or process) sees the same memory.
    let other = ProxyCache::at(&scratch.path().join("Proxies")).unwrap();
    assert_eq!(other.failure(&key).as_deref(), Some("encoder refused"));
    other.forget_failure(&key).unwrap();
    assert_eq!(cache.failure(&key), None);
    cache.record_failure(&key, "again").unwrap();
    publish(&cache, &key, b"movie");
    assert_eq!(cache.failure(&key), None);
}

#[test]
fn a_replaced_cache_directory_is_never_written() {
    use std::io::Write;
    let scratch = tempfile::tempdir().unwrap();
    let cache = cache(scratch.path());
    std::fs::rename(scratch.path().join("Proxies"), scratch.path().join("Moved")).unwrap();
    std::fs::create_dir(scratch.path().join("Proxies")).unwrap();
    let staging = cache.stage().unwrap();
    staging.movie().write_all(b"x").unwrap();
    assert!(matches!(
        cache.publish(&key('9'), staging, &sidecar(&key('9'), b"x")),
        Err(ProxyCacheError::Unsafe(_))
    ));
    assert_eq!(
        std::fs::read_dir(scratch.path().join("Proxies"))
            .unwrap()
            .count(),
        0
    );
}

/// The bytes the kill test's child publishes for `step`: size and content
/// both depend on the step, so a mixed or truncated publication is visible.
fn kill_test_movie(step: u64) -> Vec<u8> {
    let length = (256 << 10) + (step as usize % 7) * (512 << 10);
    (0..length)
        .map(|index| (index as u64).wrapping_mul(31).wrapping_add(step) as u8)
        .collect()
}

/// Process kills during proxy publication (Gate G crash suite,
/// docs/BACKUPS.md#process-kills). A child keeps staging, writing and
/// publishing multi-megabyte movies, replacing six keys in turn, and is
/// SIGKILLed after a seeded random delay. Afterwards every visible entry must
/// open with its verified hash and hold exactly one step's bytes, and cleanup
/// must still work. Abandoned staging is the only allowed residue.
#[test]
fn process_kills_during_publication_leave_whole_entries_or_none() {
    const CHILD: &str = "DEADPAN_TEST_PROXY_KILL_CHILD";
    let keys: Vec<ProxyKey> = ['a', 'b', 'c', 'd', 'e', 'f']
        .into_iter()
        .map(key)
        .collect();
    if let Some(root) = std::env::var_os(CHILD) {
        let cache = ProxyCache::at(Path::new(&root)).unwrap();
        for step in 0u64.. {
            let bytes = kill_test_movie(step);
            let key = &keys[(step % 6) as usize];
            let staging = cache.stage().unwrap();
            std::io::Write::write_all(&mut staging.movie(), &bytes).unwrap();
            let mut sidecar = sidecar(key, &bytes);
            // Record the step so the parent can rebuild the expected bytes.
            sidecar.fidelity.widest_sample = step;
            cache.publish(key, staging, &sidecar).unwrap();
        }
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("Proxies");
    drop(ProxyCache::at(&root).unwrap());
    let seed = std::env::var("DEADPAN_CHAOS_SEED")
        .ok()
        .and_then(|text| u64::from_str_radix(text.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x9e0c_5eed);
    let rounds = std::env::var("DEADPAN_CHAOS_ITERATIONS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(8u64);
    let mut rng = deadpan_chaos::Rng::new(seed);
    let mut seen = 0;
    for round in 0..rounds {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "proxy::cache::tests::process_kills_during_publication_leave_whole_entries_or_none",
                "--nocapture",
            ])
            .env(CHILD, &root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(30 + rng.below(400) as u64));
        assert!(
            child.try_wait().unwrap().is_none(),
            "round {round}: the child exited before the kill"
        );
        child.kill().unwrap();
        child.wait().unwrap();
        let cache = ProxyCache::at(&root).unwrap();
        for key in &keys {
            let Some(entry) = cache.lookup(key).unwrap() else {
                continue;
            };
            seen += 1;
            let step = entry.sidecar().fidelity.widest_sample;
            let mut bytes = Vec::new();
            cache
                .open(&entry, &not_cancelled())
                .unwrap_or_else(|error| panic!("round {round}: visible entry damaged: {error}"))
                .into_file()
                .read_to_end(&mut bytes)
                .unwrap();
            assert_eq!(bytes, kill_test_movie(step), "round {round}: step {step}");
        }
        cache
            .cleanup(
                &[],
                ProxyCleanupPolicy {
                    staging_grace: Duration::ZERO,
                    ..ProxyCleanupPolicy::default()
                },
            )
            .unwrap();
    }
    assert!(seen > 0, "no round published anything");
}
