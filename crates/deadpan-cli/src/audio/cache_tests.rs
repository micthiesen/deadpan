use std::error::Error;
use std::fs;
use std::path::PathBuf;

use deadpan_core::{ColorPolicy, FrameRate, NodeId, PresentationBasis};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_store::original_media::OriginalOwnership;
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};

use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn active() -> AtomicBool {
    AtomicBool::new(false)
}

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(name)
}

fn project(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("cache.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("cli-source-cache")?,
        revision("initial"),
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    Ok((path.clone(), ProjectStore::create(&path, &document)?))
}

fn register(store: &mut ProjectStore, path: &Path, name: &str) -> Result<(u64, u64)> {
    let original = store
        .retain_original(
            path,
            OriginalOwnership::Linked { bookmark: None },
            limits(),
            &active(),
        )?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits(), &active())?;
    let audio = AudioSession::open_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        1,
        AudioSessionLimits::default(),
        &active(),
    )?;
    let pcm_bytes = audio
        .index()
        .decoded_samples()
        .checked_mul(u64::from(audio.index().stream().channel_layout.channels()))
        .and_then(|samples| samples.checked_mul(4))
        .ok_or("decoded fixture PCM size overflow")?;
    let index_frames = u64::try_from(audio.index().frames().len())?;
    let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio))?;
    store.register_source(
        &SourceRegistration {
            expected_revision: store.snapshot()?.revision_id().clone(),
            new_revision: revision(&format!("registered-{name}")),
            original: original.object().content().clone(),
            new_asset_id: asset(name),
            label: format!("Sound {name}"),
            insertion: Some(SourceInsertionRequest {
                parent: node("root"),
                index: 0,
                node: node(name),
                label: format!("Source {name}"),
                purpose: Default::default(),
            }),
        },
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    Ok((pcm_bytes, index_frames))
}

fn read(session: &mut ProjectAudioSession, name: &str) -> Result<Vec<[f32; 2]>> {
    Ok(session
        .read_definition(
            AudioDefinitionSelector::Node { node: node(name) },
            SignalSample(0),
            128,
            &active(),
        )?
        .samples)
}

#[test]
fn alternating_registered_sources_reuses_private_pcm_after_paths_disappear() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let first = scratch.path().join("first.mp4");
    let second = scratch.path().join("second.mp4");
    fs::copy(fixture("cfr-bframes.mp4"), &first)?;
    fs::copy(fixture("offset-bframes.mp4"), &second)?;
    let (first_bytes, first_frames) = register(&mut store, &first, "first")?;
    let (second_bytes, second_frames) = register(&mut store, &second, "second")?;
    let mut session = ProjectAudioSession::open(&path)?;
    let expected_first = read(&mut session, "first")?;
    let expected_second = read(&mut session, "second")?;
    assert_ne!(expected_first, expected_second);
    assert_eq!(session.sources.retained.len(), 2);
    assert_eq!(session.sources.cache_bytes, first_bytes + second_bytes);
    assert_eq!(
        session.sources.cache_index_frames,
        first_frames + second_frames
    );
    fs::remove_file(first)?;
    fs::remove_file(second)?;
    for _ in 0..3 {
        // The former single-source cache must reopen here and fails. Successful
        // reads prove both native decodes survive, not just equal fingerprints.
        assert_eq!(read(&mut session, "first")?, expected_first);
        assert_eq!(read(&mut session, "second")?, expected_second);
    }
    Ok(())
}

#[test]
fn failed_cold_source_and_cancelled_hits_preserve_other_cache_entries() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let first = scratch.path().join("first.mp4");
    let second = scratch.path().join("second.mp4");
    fs::copy(fixture("cfr-bframes.mp4"), &first)?;
    fs::copy(fixture("offset-bframes.mp4"), &second)?;
    let (first_bytes, first_frames) = register(&mut store, &first, "first")?;
    let (second_bytes, second_frames) = register(&mut store, &second, "second")?;
    let mut session = ProjectAudioSession::open(&path)?;
    let expected = read(&mut session, "first")?;
    let before_bytes = session.sources.cache_bytes;
    assert_eq!(before_bytes, first_bytes);
    assert_eq!(session.sources.cache_index_frames, first_frames);
    fs::remove_file(&second)?;
    assert!(read(&mut session, "second").is_err());
    assert_eq!(session.sources.retained.len(), 1);
    assert_eq!(session.sources.cache_bytes, before_bytes);
    assert_eq!(session.sources.cache_index_frames, first_frames);
    let document = session.sources.document.clone();
    assert!(matches!(
        session.sources.source(
            document.project_id(),
            document.revision_id(),
            &asset("first"),
            &AtomicBool::new(true),
        ),
        Err(PreparationError::Cancelled)
    ));
    assert_eq!(session.sources.cache_bytes, before_bytes);
    assert_eq!(session.sources.cache_index_frames, first_frames);
    fs::remove_file(first)?;
    assert_eq!(read(&mut session, "first")?, expected);
    fs::copy(fixture("offset-bframes.mp4"), &second)?;
    assert_ne!(read(&mut session, "second")?, expected);
    assert_eq!(session.sources.retained.len(), 2);
    assert_eq!(session.sources.cache_bytes, first_bytes + second_bytes);
    assert_eq!(
        session.sources.cache_index_frames,
        first_frames + second_frames
    );
    Ok(())
}

#[test]
fn retained_context_and_cache_hits_recheck_the_captured_contracts() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let first = scratch.path().join("first.mp4");
    let second = scratch.path().join("second.mp4");
    fs::copy(fixture("cfr-bframes.mp4"), &first)?;
    fs::copy(fixture("offset-bframes.mp4"), &second)?;
    let (first_bytes, first_frames) = register(&mut store, &first, "first")?;
    let document = store.snapshot()?;
    let context = FrozenAudioContext::capture(&document)?;
    register(&mut store, &second, "second")?;
    let mut session = ProjectAudioSession::open_context(&path, &context)?;
    let expected = read(&mut session, "first")?;
    assert_eq!(session.sources.cache_bytes, first_bytes);
    assert_eq!(session.sources.cache_index_frames, first_frames);
    let current = store.snapshot()?;
    assert!(
        session
            .sources
            .source(
                document.project_id(),
                current.revision_id(),
                &asset("first"),
                &active(),
            )
            .is_err()
    );
    let mut wrong = document.assets()[&asset("first")].clone();
    wrong.label = "different retained contract".into();
    assert!(matches!(
        session.sources.source_for_context(
            document.project_id(),
            document.revision_id(),
            &asset("first"),
            &wrong,
            &active(),
        ),
        Err(PreparationError::IndexMismatch)
    ));
    // A corrupted cache admission record must fail even after PCM exists.
    let original = session.sources.retained[&asset("first")].original.clone();
    let other = store.registered_source(current.revision_id(), &asset("second"))?;
    session
        .sources
        .retained
        .get_mut(&asset("first"))
        .unwrap()
        .original = store.original_record(other.original().content())?.unwrap();
    assert!(matches!(
        session.sources.source(
            document.project_id(),
            document.revision_id(),
            &asset("first"),
            &active(),
        ),
        Err(PreparationError::IndexMismatch)
    ));
    session
        .sources
        .retained
        .get_mut(&asset("first"))
        .unwrap()
        .original = original;
    let qualification = session.sources.retained[&asset("first")]
        .qualification
        .clone();
    session
        .sources
        .retained
        .get_mut(&asset("first"))
        .unwrap()
        .qualification = other.id().clone();
    assert!(matches!(
        session.sources.source(
            document.project_id(),
            document.revision_id(),
            &asset("first"),
            &active(),
        ),
        Err(PreparationError::IndexMismatch)
    ));
    session
        .sources
        .retained
        .get_mut(&asset("first"))
        .unwrap()
        .qualification = qualification;
    let content = session.sources.retained[&asset("first")].content;
    session
        .sources
        .retained
        .get_mut(&asset("first"))
        .unwrap()
        .content = other.snapshot().audio().unwrap().content();
    assert!(matches!(
        session.sources.source(
            document.project_id(),
            document.revision_id(),
            &asset("first"),
            &active(),
        ),
        Err(PreparationError::IndexMismatch)
    ));
    session
        .sources
        .retained
        .get_mut(&asset("first"))
        .unwrap()
        .content = content;
    fs::remove_file(first)?;
    assert_eq!(read(&mut session, "first")?, expected);
    assert_eq!(session.sources.cache_bytes, first_bytes);
    assert_eq!(session.sources.cache_index_frames, first_frames);
    Ok(())
}

#[test]
fn seventeenth_source_evicts_lru_and_missing_evicted_source_preserves_recent_pcm() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = fs::read(fixture("cfr-bframes.mp4"))?;
    let variant = |index: usize| {
        let mut bytes = original.clone();
        // The qualified MP4 grammar admits empty free atoms only. Vary their
        // count to retain identical AAC samples under distinct byte identities.
        for _ in 0..=index {
            bytes.extend_from_slice(&8_u32.to_be_bytes());
            bytes.extend_from_slice(b"free");
        }
        bytes
    };
    let mut paths = Vec::new();
    let mut source_sizes = Vec::new();
    for index in 0..=MAX_CACHED_SOURCES {
        let name = format!("sound-{index}");
        let local = scratch.path().join(format!("{name}.mp4"));
        fs::write(&local, variant(index))?;
        source_sizes.push(register(&mut store, &local, &name)?);
        paths.push(local);
    }
    let mut session = ProjectAudioSession::open(&path)?;
    let expected = read(&mut session, "sound-0")?;
    for index in 1..MAX_CACHED_SOURCES {
        read(&mut session, &format!("sound-{index}"))?;
    }
    // A successful hit makes sound-0 recent. Sound-1, not the first-loaded
    // source, must be evicted when the seventeenth source arrives.
    assert_eq!(read(&mut session, "sound-0")?, expected);
    let before_bytes = session.sources.cache_bytes;
    let before_frames = session.sources.cache_index_frames;
    assert_eq!(
        before_bytes,
        source_sizes[..MAX_CACHED_SOURCES]
            .iter()
            .map(|size| size.0)
            .sum::<u64>()
    );
    assert_eq!(
        before_frames,
        source_sizes[..MAX_CACHED_SOURCES]
            .iter()
            .map(|size| size.1)
            .sum::<u64>()
    );
    assert!(source_sizes.iter().all(|size| *size == source_sizes[0]));
    assert_eq!(read(&mut session, "sound-16")?, expected);
    assert!(!session.sources.retained.contains_key(&asset("sound-1")));
    assert!(session.sources.retained.contains_key(&asset("sound-0")));
    assert_eq!(session.sources.retained.len(), MAX_CACHED_SOURCES);
    assert_eq!(session.sources.cache_bytes, before_bytes);
    assert_eq!(session.sources.cache_index_frames, before_frames);
    for local in &paths {
        fs::remove_file(local)?;
    }
    // The evicted source needs fresh verified original bytes. Failure happens
    // before eviction, so all resident sources and accounting remain intact.
    let before_order = session.sources.recency.clone();
    assert!(read(&mut session, "sound-1").is_err());
    assert_eq!(session.sources.retained.len(), MAX_CACHED_SOURCES);
    assert_eq!(session.sources.cache_bytes, before_bytes);
    assert_eq!(session.sources.cache_index_frames, before_frames);
    assert_eq!(session.sources.recency, before_order);
    for _ in 0..3 {
        assert_eq!(read(&mut session, "sound-0")?, expected);
        assert_eq!(read(&mut session, "sound-16")?, expected);
    }
    // Restoring the exact bytes permits an evicted source to be decoded again.
    fs::write(&paths[1], variant(1))?;
    assert_eq!(read(&mut session, "sound-1")?, expected);
    assert!(!session.sources.retained.contains_key(&asset("sound-2")));
    assert_eq!(session.sources.cache_bytes, before_bytes);
    assert_eq!(session.sources.cache_index_frames, before_frames);
    Ok(())
}

#[test]
fn byte_pressure_evicts_before_preparation_without_charging_failed_reservation() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let first = scratch.path().join("first.mp4");
    let second = scratch.path().join("second.mp4");
    fs::copy(fixture("cfr-bframes.mp4"), &first)?;
    fs::copy(fixture("offset-bframes.mp4"), &second)?;
    let (first_bytes, first_frames) = register(&mut store, &first, "first")?;
    let (second_bytes, second_frames) = register(&mut store, &second, "second")?;
    let mut session = ProjectAudioSession::open(&path)?;
    read(&mut session, "first")?;
    let expected = read(&mut session, "second")?;
    assert_eq!(session.sources.cache_bytes, first_bytes + second_bytes);
    assert_eq!(
        session.sources.cache_index_frames,
        first_frames + second_frames
    );
    // Exercise a full-budget prospective open without allocating a gigabyte
    // fixture. No reservation is published until native preparation succeeds.
    let reservation = session
        .sources
        .make_room(MAX_CACHE_BYTES - second_bytes, 1)?;
    assert_eq!(reservation, (MAX_CACHE_BYTES, second_frames + 1));
    assert!(!session.sources.retained.contains_key(&asset("first")));
    assert!(session.sources.retained.contains_key(&asset("second")));
    assert_eq!(session.sources.cache_bytes, second_bytes);
    assert_eq!(session.sources.cache_index_frames, second_frames);
    assert_eq!(session.sources.recency, VecDeque::from([asset("second")]));
    // Model a failed native preparation: there is no insertion or reservation
    // commit. The surviving real PCM remains usable and correctly charged.
    fs::remove_file(second)?;
    assert_eq!(read(&mut session, "second")?, expected);
    assert_eq!(session.sources.cache_bytes, second_bytes);
    assert_eq!(session.sources.cache_index_frames, second_frames);
    assert!(session.sources.make_room(MAX_CACHE_BYTES + 1, 1).is_err());
    assert_eq!(session.sources.cache_bytes, second_bytes);
    assert_eq!(session.sources.cache_index_frames, second_frames);
    assert_eq!(session.sources.retained.len(), 1);
    Ok(())
}

#[test]
fn metadata_pressure_evicts_before_preparation_without_charging_failed_reservation() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let first = scratch.path().join("first.mp4");
    let second = scratch.path().join("second.mp4");
    fs::copy(fixture("cfr-bframes.mp4"), &first)?;
    fs::copy(fixture("offset-bframes.mp4"), &second)?;
    let (first_bytes, first_frames) = register(&mut store, &first, "first")?;
    let (second_bytes, second_frames) = register(&mut store, &second, "second")?;
    let mut session = ProjectAudioSession::open(&path)?;
    read(&mut session, "first")?;
    let expected = read(&mut session, "second")?;
    assert_eq!(session.sources.cache_bytes, first_bytes + second_bytes);
    assert_eq!(
        session.sources.cache_index_frames,
        first_frames + second_frames
    );
    let before_order = session.sources.recency.clone();
    // A source that cannot ever fit is rejected without evicting either entry.
    assert!(
        session
            .sources
            .make_room(4, MAX_CACHE_INDEX_FRAMES + 1)
            .is_err()
    );
    assert_eq!(session.sources.recency, before_order);
    assert_eq!(session.sources.cache_bytes, first_bytes + second_bytes);
    assert_eq!(
        session.sources.cache_index_frames,
        first_frames + second_frames
    );
    // Reserve metadata pressure using the real accounting path without a
    // million-frame fixture or significant PCM pressure.
    let reservation = session
        .sources
        .make_room(4, MAX_CACHE_INDEX_FRAMES - second_frames)?;
    assert_eq!(reservation, (second_bytes + 4, MAX_CACHE_INDEX_FRAMES));
    assert!(!session.sources.retained.contains_key(&asset("first")));
    assert!(session.sources.retained.contains_key(&asset("second")));
    assert_eq!(session.sources.cache_bytes, second_bytes);
    assert_eq!(session.sources.cache_index_frames, second_frames);
    assert_eq!(session.sources.recency, VecDeque::from([asset("second")]));
    // Dropping a prospective reservation models failed/cancelled preparation:
    // no metadata or PCM charge is published, and retained PCM still works.
    fs::remove_file(second)?;
    assert_eq!(read(&mut session, "second")?, expected);
    assert_eq!(session.sources.cache_bytes, second_bytes);
    assert_eq!(session.sources.cache_index_frames, second_frames);
    Ok(())
}

#[test]
fn aggregate_reservation_is_inclusive_checked_and_requires_nonempty_pcm_and_index() {
    assert_eq!(
        reserve_source_capacity(0, 0, MAX_CACHE_BYTES, MAX_CACHE_INDEX_FRAMES).unwrap(),
        Some((MAX_CACHE_BYTES, MAX_CACHE_INDEX_FRAMES))
    );
    assert_eq!(
        reserve_source_capacity(MAX_CACHE_BYTES - 4, MAX_CACHE_INDEX_FRAMES - 1, 4, 1).unwrap(),
        Some((MAX_CACHE_BYTES, MAX_CACHE_INDEX_FRAMES))
    );
    assert_eq!(
        reserve_source_capacity(MAX_CACHE_BYTES - 4, 0, 5, 1).unwrap(),
        None
    );
    assert_eq!(
        reserve_source_capacity(0, MAX_CACHE_INDEX_FRAMES, 4, 1).unwrap(),
        None
    );
    assert_eq!(reserve_source_capacity(u64::MAX, 0, 1, 1).unwrap(), None);
    assert_eq!(reserve_source_capacity(0, u64::MAX, 1, 1).unwrap(), None);
    assert!(reserve_source_capacity(0, 0, 0, 1).is_err());
    assert!(reserve_source_capacity(0, 0, 1, 0).is_err());
    assert!(reserve_source_capacity(0, 0, MAX_CACHE_BYTES + 1, 1).is_err());
    assert!(reserve_source_capacity(0, 0, 1, MAX_CACHE_INDEX_FRAMES + 1).is_err());
}
