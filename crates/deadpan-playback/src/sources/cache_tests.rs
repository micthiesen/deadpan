use super::*;

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use deadpan_core::{ColorPolicy, FrameRate, NodeId, PresentationBasis};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::SourceRegistration;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn active() -> AtomicBool {
    AtomicBool::new(false)
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()
        .unwrap()
}

fn test_limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}

fn empty_document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("playback-cache").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap()
}

fn register_assets(
    store: &mut ProjectStore,
    parent: &std::path::Path,
    count: usize,
) -> TestResult<Vec<AssetId>> {
    let original_bytes = fs::read(fixture())?;
    let assets = (0..count)
        .map(|index| AssetId::new(format!("sound-{index:02}")).unwrap())
        .collect::<Vec<_>>();
    for (index, asset) in assets.iter().enumerate() {
        // Empty MP4 free atoms preserve the AAC stream while giving each
        // registration a distinct original-byte identity and receipt.
        let mut bytes = original_bytes.clone();
        for _ in 0..=index {
            bytes.extend_from_slice(&8_u32.to_be_bytes());
            bytes.extend_from_slice(b"free");
        }
        let source_path = parent.join(format!("sound-{index:02}.mp4"));
        fs::write(&source_path, bytes)?;
        let original = store
            .retain_original(
                &source_path,
                OriginalOwnership::Managed,
                test_limits(),
                &active(),
            )?
            .record;
        let mut input =
            store.snapshot_original(original.object().content(), test_limits(), &active())?;
        let audio = AudioSession::open_verified(
            &mut input,
            SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
            1,
            AudioSessionLimits::default(),
            &active(),
        )?;
        let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio))?;
        store.register_source(
            &SourceRegistration {
                expected_revision: store.snapshot()?.revision_id().clone(),
                new_revision: RevisionId::new(format!("registered-{index:02}"))?,
                original: original.object().content().clone(),
                new_asset_id: asset.clone(),
                label: format!("Sound {index:02}"),
                insertion: None,
            },
            &decoded,
            None,
            test_limits(),
            &active(),
        )?;
    }
    Ok(assets)
}

fn captured(store: &ProjectStore) -> Arc<Snapshot> {
    let document = Arc::new(store.snapshot().unwrap());
    let sources = document
        .assets()
        .keys()
        .map(|asset| {
            let receipt = Arc::new(
                store
                    .registered_source(document.revision_id(), asset)
                    .unwrap(),
            );
            let original = store
                .original_record(receipt.original().content())
                .unwrap()
                .unwrap();
            (asset.clone(), SourceEntry { receipt, original })
        })
        .collect();
    Arc::new(Snapshot::committed(
        1,
        document,
        sources,
        store.original_import_handle().unwrap(),
    ))
}

fn read(
    sources: &mut Sources,
    snapshot: &Snapshot,
    asset: &AssetId,
) -> TestResult<SourceContentIdentity> {
    let prepared = sources.source(
        snapshot.document.project_id(),
        snapshot.document.revision_id(),
        asset,
        &active(),
    )?;
    Ok(prepared.index().content())
}

#[test]
fn seventeenth_qualified_source_evicts_lru_and_evicted_source_can_reopen() -> TestResult {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir()?;
    let mut store =
        ProjectStore::create(&directory.path().join("cache.deadpan"), &empty_document())?;
    let assets = register_assets(&mut store, directory.path(), MAX_CACHED_SOURCES + 1)?;
    let snapshot = captured(&store);
    let mut sources = Sources::new(snapshot.clone());
    let expected = |index: usize| {
        snapshot.sources[&assets[index]]
            .receipt
            .snapshot()
            .audio()
            .unwrap()
            .content()
    };
    assert_ne!(expected(0), expected(1));
    for (index, asset) in assets.iter().take(MAX_CACHED_SOURCES).enumerate() {
        assert_eq!(read(&mut sources, &snapshot, asset)?, expected(index));
    }

    // Touch the oldest entry. Loading the seventeenth source must now evict
    // sound-01, while preserving sound-00 and its private decoded PCM.
    assert_eq!(read(&mut sources, &snapshot, &assets[0])?, expected(0));
    let per_source_bytes = sources.cache[&assets[0]].bytes;
    let per_source_frames = sources.cache[&assets[0]].index_frames;
    let old_bytes = sources.cache_bytes;
    let old_frames = sources.cache_index_frames;
    assert_eq!(old_bytes, per_source_bytes * MAX_CACHED_SOURCES as u64);
    assert_eq!(old_frames, per_source_frames * MAX_CACHED_SOURCES as u64);
    assert_eq!(read(&mut sources, &snapshot, &assets[16])?, expected(16));
    assert!(!sources.cache.contains_key(&assets[1]));
    assert!(sources.cache.contains_key(&assets[0]));
    assert_eq!(sources.cache.len(), MAX_CACHED_SOURCES);
    assert_eq!(sources.cache_bytes, old_bytes);
    assert_eq!(sources.cache_index_frames, old_frames);

    // Reopening sound-01 consumes capacity by evicting the next least-recently
    // used entry, sound-02, and keeps both aggregate totals bounded.
    assert_eq!(read(&mut sources, &snapshot, &assets[1])?, expected(1));
    assert!(!sources.cache.contains_key(&assets[2]));
    assert_eq!(sources.cache.len(), MAX_CACHED_SOURCES);
    assert_eq!(sources.cache_bytes, old_bytes);
    assert_eq!(sources.cache_index_frames, old_frames);
    Ok(())
}

#[test]
fn failed_cold_snapshot_and_cancelled_requests_preserve_cache_and_recency() -> TestResult {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir()?;
    let mut store =
        ProjectStore::create(&directory.path().join("cache.deadpan"), &empty_document())?;
    let assets = register_assets(&mut store, directory.path(), MAX_CACHED_SOURCES + 1)?;
    let snapshot = captured(&store);
    let mut sources = Sources::new(snapshot.clone());
    for asset in assets.iter().take(MAX_CACHED_SOURCES) {
        read(&mut sources, &snapshot, asset)?;
    }
    let resident_order = sources.recency.clone();
    let resident_bytes = sources.cache_bytes;
    let resident_frames = sources.cache_index_frames;

    let stopped = AtomicBool::new(true);
    assert!(
        sources
            .source(
                snapshot.document.project_id(),
                snapshot.document.revision_id(),
                &assets[0],
                &stopped,
            )
            .is_err()
    );
    assert!(
        sources
            .source(
                snapshot.document.project_id(),
                snapshot.document.revision_id(),
                &assets[16],
                &stopped,
            )
            .is_err()
    );
    assert_eq!(sources.recency, resident_order);
    assert_eq!(sources.cache_bytes, resident_bytes);
    assert_eq!(sources.cache_index_frames, resident_frames);
    assert_eq!(sources.cache.len(), MAX_CACHED_SOURCES);

    // Closing the owning store revokes snapshots. The cold read must fail
    // before evicting anything or charging a reservation.
    drop(store);
    assert!(
        sources
            .source(
                snapshot.document.project_id(),
                snapshot.document.revision_id(),
                &assets[16],
                &active(),
            )
            .is_err()
    );
    assert_eq!(sources.recency, resident_order);
    assert_eq!(sources.cache_bytes, resident_bytes);
    assert_eq!(sources.cache_index_frames, resident_frames);
    assert_eq!(sources.cache.len(), MAX_CACHED_SOURCES);
    Ok(())
}

#[test]
fn source_capacity_accepts_exact_numeric_limits_and_rejects_overages() {
    assert!(validate_source_capacity(MAX_CACHE_BYTES, MAX_CACHE_INDEX_FRAMES).is_ok());
    assert!(validate_source_capacity(MAX_CACHE_BYTES + 1, 1).is_err());
    assert!(validate_source_capacity(1, MAX_CACHE_INDEX_FRAMES + 1).is_err());
    assert!(validate_source_capacity(0, 1).is_err());
    assert!(validate_source_capacity(1, 0).is_err());
    assert!(source_capacity_fits(
        MAX_CACHE_BYTES - 1,
        MAX_CACHE_INDEX_FRAMES - 1,
        1,
        1,
    ));
    assert!(!source_capacity_fits(MAX_CACHE_BYTES, 0, 1, 1,));
    assert!(!source_capacity_fits(0, MAX_CACHE_INDEX_FRAMES, 1, 1,));
}
