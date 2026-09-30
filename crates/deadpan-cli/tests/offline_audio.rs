#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_cli::audio::{
    MAX_OFFLINE_AUDIO_FRAMES, OfflineAudioError, OfflineAudioSession, ProjectAudioSession,
};
use deadpan_core::{
    AssetId, AudioSample, AudioTreatments, ClipGain, ColorPolicy, Command, CommandRequest,
    FrameRange, FrameRate, GainDb, NodeId, PresentationBasis, ProjectDocument, ProjectFrame,
    ProjectId, RevisionId,
};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn active() -> AtomicBool {
    AtomicBool::new(false)
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn treatments(muted: bool) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-6_000).unwrap(), muted, vec![], vec![]).unwrap(),
    )
}

fn commit(store: &mut ProjectStore, next: &str, command: Command) -> Result {
    let before = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision(next),
        command,
    })?;
    Ok(())
}

fn write_declared_stereo_fixture(path: &Path) -> Result {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav");
    let pcm_wave = fs::read(fixture)?;
    assert_eq!(&pcm_wave[..4], b"RIFF");
    assert_eq!(&pcm_wave[8..16], b"WAVEfmt ");
    assert_eq!(&pcm_wave[16..20], &16_u32.to_le_bytes());
    assert_eq!(&pcm_wave[20..24], &[1, 0, 2, 0]);
    assert_eq!(&pcm_wave[24..28], &48_000_u32.to_le_bytes());
    assert_eq!(&pcm_wave[32..36], &[4, 0, 16, 0]);
    assert_eq!(&pcm_wave[36..40], b"data");
    // The repository recipe declares left/right PCM, but ordinary WAV16 has
    // no speaker mask. Retain every PCM byte and encode that known recipe in a
    // WAVEFORMATEXTENSIBLE header. Production must measure the explicit mask.
    let mut declared = Vec::with_capacity(pcm_wave.len() + 24);
    declared.extend_from_slice(b"RIFF");
    declared.extend_from_slice(&u32::try_from(pcm_wave.len() + 16)?.to_le_bytes());
    declared.extend_from_slice(b"WAVEfmt ");
    declared.extend_from_slice(&40_u32.to_le_bytes());
    declared.extend_from_slice(&0xfffe_u16.to_le_bytes());
    declared.extend_from_slice(&pcm_wave[22..36]);
    declared.extend_from_slice(&22_u16.to_le_bytes());
    declared.extend_from_slice(&16_u16.to_le_bytes());
    declared.extend_from_slice(&3_u32.to_le_bytes()); // SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT
    declared.extend_from_slice(&[
        1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
    ]); // KSDATAFORMAT_SUBTYPE_PCM
    declared.extend_from_slice(&pcm_wave[36..]);
    fs::write(path, declared)?;
    Ok(())
}

/// Actual registered 8197-frame stereo PCM, enclosed by six project frames at
/// 30000/1001. Its measured endpoint and final silent fraction stay distinct.
fn project(parent: &Path) -> Result<(PathBuf, PathBuf, ProjectStore)> {
    let package = parent.join("offline.deadpan");
    let linked = parent.join("linked-stereo.wav");
    write_declared_stereo_fixture(&linked)?;
    let document = ProjectDocument::new(
        ProjectId::new("offline-audio")?,
        revision("initial"),
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let mut store = ProjectStore::create(&package, &document)?;
    let cancelled = active();
    let limits = OriginalMediaLimits::new(2_000_000, Duration::from_secs(10))?;
    let original = store
        .retain_original(
            &linked,
            OriginalOwnership::Linked { bookmark: None },
            limits,
            &cancelled,
        )?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits, &cancelled)?;
    let audio = AudioSession::open_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )?;
    assert_eq!(audio.index().valid_samples(), 8_197);
    assert_eq!(
        audio.index().stream().channel_layout,
        AudioChannelLayout::Native {
            channels: 2,
            mask: 3
        }
    );
    let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio))?;
    store.register_source(
        &SourceRegistration {
            expected_revision: store.snapshot()?.revision_id().clone(),
            new_revision: revision("import"),
            original: original.object().content().clone(),
            new_asset_id: AssetId::new("measured")?,
            label: "Measured stereo".into(),
            insertion: Some(SourceInsertionRequest {
                parent: node("root"),
                index: 0,
                node: node("clip"),
                label: "Registered stereo".into(),
                purpose: Default::default(),
            }),
        },
        &decoded,
        None,
        limits,
        &cancelled,
    )?;
    commit(
        &mut store,
        "gain",
        Command::SetAudioTreatments {
            node: node("clip"),
            treatments: treatments(false),
        },
    )?;
    Ok((package, linked, store))
}

fn history_counts(package: &Path) -> Result<(i64, i64, i64)> {
    let connection = rusqlite::Connection::open(package.join("project.sqlite"))?;
    Ok(connection.query_row(
        "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM source_qualifications)",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}

fn canonical(
    package: &Path,
    captured: &RevisionId,
    samples: Range<AudioSample>,
) -> Result<Vec<[f32; 2]>> {
    let mut session = ProjectAudioSession::open_revision(package, captured)?;
    let mut cursor = samples.start;
    let mut output = Vec::new();
    while cursor < samples.end {
        let count = u32::try_from((samples.end.0 - cursor.0).min(256))?;
        let block = session.read_limited(cursor, count, &active())?;
        assert_eq!(block.start, cursor);
        assert_eq!(&block.revision_id, captured);
        output.extend(block.samples);
        cursor.0 += i64::from(count);
    }
    Ok(output)
}

#[test]
fn offline_fractional_range_and_read_partitions_match_the_canonical_limited_bus() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, _linked, store) = project(scratch.path())?;
    let captured = store.snapshot()?;
    let counts = history_counts(&package)?;
    let cancelled = active();
    let job_deadline = deadline();
    let mut session = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(1, 6),
        &cancelled,
        job_deadline,
    )?;
    assert_eq!(session.deadline(), job_deadline);
    assert_eq!(session.project_id(), captured.project_id());
    assert_eq!(session.revision(), captured.revision_id());
    assert_eq!(session.document(), &captured);
    assert_eq!(session.range(), range(1, 6));
    assert_eq!(session.frame_rate(), FrameRate::new(30_000, 1_001)?);
    assert_eq!(
        session.sample_range(),
        AudioSample(1_602)..AudioSample(9_610)
    );
    assert_eq!(session.sample_count(), 8_008);
    let expected = canonical(&package, captured.revision_id(), session.sample_range())?;
    assert!(expected.iter().flatten().any(|value| value.abs() > 0.001));
    assert_eq!(
        session.read(AudioSample(1_602), 8_008, &cancelled)?.samples,
        expected
    );

    // Begin fresh so this comparison also covers cold cache preparation in a
    // different partition order, including a crossing of the 8192-sample tile.
    let mut partitioned = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(1, 6),
        &cancelled,
        job_deadline,
    )?;
    let pieces = [
        (6_001_u32, 2_007_u32),
        (0, 1),
        (1, 255),
        (256, 1_025),
        (1_281, 4_720),
    ];
    let mut actual = vec![[f32::NAN; 2]; expected.len()];
    for (offset, count) in pieces {
        let start = AudioSample(1_602 + i64::from(offset));
        let block = partitioned.read(start, count, &cancelled)?;
        assert_eq!(block.start, start);
        assert_eq!(block.revision_id, *captured.revision_id());
        assert_eq!(block.stage, "limited_authored_bus_pcm");
        assert_eq!(block.samples.len(), usize::try_from(count)?);
        let offset = usize::try_from(offset)?;
        actual[offset..offset + block.samples.len()].copy_from_slice(&block.samples);
    }
    assert_eq!(actual, expected);

    // One NTSC project frame at this nonzero origin contains 1601, not 1602,
    // samples. Do not round the selected duration as though it began at zero.
    let mut one_frame = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(1, 2),
        &cancelled,
        job_deadline,
    )?;
    assert_eq!(one_frame.sample_count(), 1_601);
    assert_eq!(
        one_frame.sample_range(),
        AudioSample(1_602)..AudioSample(3_203)
    );
    assert_eq!(
        one_frame
            .read(AudioSample(1_602), 1_601, &cancelled)?
            .samples,
        expected[..1_601]
    );
    assert!(matches!(
        one_frame.read(AudioSample(1_602), 1_602, &cancelled),
        Err(OfflineAudioError::Range)
    ));

    let mut maximum = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(0, 6),
        &cancelled,
        job_deadline,
    )?;
    let maximum_block = maximum.read(AudioSample(0), MAX_OFFLINE_AUDIO_FRAMES, &cancelled)?;
    assert_eq!(maximum_block.samples.len(), 8_192);
    assert_eq!(&maximum_block.samples[1_602..], &expected[..6_590]);
    assert_eq!(store.snapshot()?, captured);
    assert_eq!(history_counts(&package)?, counts);
    Ok(())
}

#[test]
fn offline_reads_keep_the_captured_revision_through_edits_history_and_link_loss() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, linked, mut store) = project(scratch.path())?;
    let captured = store.snapshot()?;
    let cancelled = active();
    let job_deadline = deadline();
    let mut retained = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(1, 6),
        &cancelled,
        job_deadline,
    )?;
    let expected = retained
        .read(AudioSample(1_602), 8_008, &cancelled)?
        .samples;
    assert!(expected.iter().flatten().any(|sample| *sample != 0.0));
    commit(
        &mut store,
        "mute",
        Command::SetAudioTreatments {
            node: node("clip"),
            treatments: treatments(true),
        },
    )?;
    let mut current = OfflineAudioSession::open_revision(
        &package,
        &revision("mute"),
        range(1, 6),
        &cancelled,
        job_deadline,
    )?;
    assert_eq!(
        current.read(AudioSample(1_602), 512, &cancelled)?.samples,
        vec![[0.0; 2]; 512]
    );
    store.undo(&revision("mute"), revision("undo-mute"))?;
    store.redo(&revision("undo-mute"), revision("redo-mute"))?;
    let current_document = store.snapshot()?;
    let counts = history_counts(&package)?;
    let history = store.history_availability()?;
    let mut historical = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(1, 6),
        &cancelled,
        job_deadline,
    )?;
    assert_eq!(
        historical
            .read(AudioSample(1_602), 8_008, &cancelled)?
            .samples,
        expected
    );
    fs::remove_file(linked)?;
    assert_eq!(
        retained
            .read(AudioSample(1_602), 8_008, &cancelled)?
            .samples,
        expected
    );
    let mut cold = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(1, 6),
        &cancelled,
        job_deadline,
    )?;
    assert!(matches!(
        cold.read(AudioSample(1_602), 256, &cancelled),
        Err(OfflineAudioError::Project(_))
    ));
    assert_eq!(store.snapshot()?, current_document);
    assert_eq!(store.history_availability()?, history);
    assert_eq!(history_counts(&package)?, counts);
    store.validate()?;
    Ok(())
}

#[test]
fn offline_capture_and_reads_reject_control_failures_and_out_of_range_blocks() -> Result {
    let missing = Path::new("/missing-offline-audio.deadpan");
    assert!(matches!(
        OfflineAudioSession::open_revision(
            missing,
            &revision("missing"),
            range(0, 1),
            &AtomicBool::new(true),
            deadline(),
        ),
        Err(OfflineAudioError::Cancelled)
    ));
    assert!(matches!(
        OfflineAudioSession::open_revision(
            missing,
            &revision("missing"),
            range(0, 1),
            &active(),
            Instant::now(),
        ),
        Err(OfflineAudioError::Deadline)
    ));
    let scratch = tempfile::tempdir()?;
    let (package, _linked, store) = project(scratch.path())?;
    let captured = store.snapshot()?;
    let counts = history_counts(&package)?;
    for invalid in [range(-1, 1), range(2, 2), range(0, 7)] {
        assert!(matches!(
            OfflineAudioSession::open_revision(
                &package,
                captured.revision_id(),
                invalid,
                &active(),
                deadline(),
            ),
            Err(OfflineAudioError::Range)
        ));
    }
    let cancelled = active();
    let mut session = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(1, 6),
        &cancelled,
        deadline(),
    )?;
    for (start, count) in [
        (1_601, 1),
        (1_602, 0),
        (1_602, 8_193),
        (9_610, 1),
        (i64::MAX, 2),
    ] {
        assert!(matches!(
            session.read(AudioSample(start), count, &cancelled),
            Err(OfflineAudioError::Range)
        ));
    }
    let expected = session.read(AudioSample(1_602), 256, &cancelled)?.samples;
    cancelled.store(true, Ordering::Release);
    assert!(matches!(
        session.read(AudioSample(1_602), 256, &cancelled),
        Err(OfflineAudioError::Cancelled)
    ));
    // The application owns its cancellation policy. Reusing a separately live
    // token proves cancellation did not publish a block or damage the cache.
    assert_eq!(
        session.read(AudioSample(1_602), 256, &active())?.samples,
        expected
    );
    assert_eq!(store.snapshot()?, captured);
    assert_eq!(history_counts(&package)?, counts);
    Ok(())
}

#[test]
fn offline_deadline_is_not_renewed_after_capture() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, _linked, store) = project(scratch.path())?;
    let captured = store.snapshot()?;
    // Open before the deliberately short budget starts: only metadata capture
    // is done inside it. No cold decode must finish within a timing assertion.
    let _warm_metadata = ProjectAudioSession::open_revision(&package, captured.revision_id())?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut session = OfflineAudioSession::open_revision(
        &package,
        captured.revision_id(),
        range(1, 2),
        &active(),
        deadline,
    )?;
    assert_eq!(session.deadline(), deadline);
    while Instant::now() < deadline {
        std::thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
    }
    assert!(matches!(
        session.read(AudioSample(1_602), 1, &active()),
        Err(OfflineAudioError::Deadline)
    ));
    assert_eq!(store.snapshot()?, captured);
    Ok(())
}
