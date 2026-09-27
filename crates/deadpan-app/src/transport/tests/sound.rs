use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use deadpan_core::{AssetId, NodeId, ProjectDocument};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::SourceRegistration;

use super::*;

fn sound() -> Arc<Sound> {
    let scratch = tempfile::tempdir().unwrap();
    let document = ProjectDocument::new_automatic(
        ProjectId::new("sound-transport").unwrap(),
        RevisionId::new("initial").unwrap(),
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut store = ProjectStore::create(&scratch.path().join("sound.deadpan"), &document).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let limits = OriginalMediaLimits::default();
    let original = store
        .retain_original(&path, OriginalOwnership::Managed, limits, &cancelled)
        .unwrap()
        .record;
    let mut snapshot = store
        .snapshot_original(original.object().content(), limits, &cancelled)
        .unwrap();
    let audio = AudioSession::open_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length()).unwrap(),
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio)).unwrap();
    let asset = AssetId::new("sound").unwrap();
    let revision = RevisionId::new("registered").unwrap();
    store
        .register_source(
            &SourceRegistration {
                expected_revision: document.revision_id().clone(),
                new_revision: revision.clone(),
                original: original.object().content().clone(),
                new_asset_id: asset.clone(),
                label: "Sound".into(),
                insertion: None,
            },
            &decoded,
            None,
            limits,
            &cancelled,
        )
        .unwrap();
    let receipt = Arc::new(store.registered_source(&revision, &asset).unwrap());
    Arc::new(Sound::new(FrameRate::new(30_000, 1001).unwrap(), asset, receipt).unwrap())
}

#[test]
fn sound_uses_exact_sample_cursor_and_never_schedules_a_picture() {
    let sound = sound();
    let domain = Domain::Sound(sound.clone());
    assert_eq!(domain.end().unwrap(), AudioSample(8197));
    assert_eq!(domain.sample_at_boundary(1703).unwrap(), AudioSample(1703));
    assert_eq!(domain.frame_at_sample(AudioSample(1703)).unwrap(), 1703);
    assert!(domain.sample_at_boundary(8198).is_err());
    assert!(domain.frame_at_sample(AudioSample(-1)).is_err());
    let window = Window::new(AudioSample(0), sound.duration_samples(), false).unwrap();
    let mut run = Run::with_domain(identity(), domain.clone(), window, AudioSample(0)).unwrap();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let mut report = update(&run, 1703);
    report.generation = Some(generation);
    assert_eq!(run.receive(&report).unwrap(), Some(1703));
    assert!(run.picture_frame().is_err());
    assert_eq!(run.picture(1703, false), None);
    let resume = run.resume(1703);
    assert_eq!(
        resume.sample_for_domain(
            run.session,
            &run.project,
            &run.revision,
            &domain,
            &window,
            1703
        ),
        Some(AudioSample(1703))
    );
    assert_eq!(
        resume.sample_for_domain(
            run.session,
            &run.project,
            &run.revision,
            &domain,
            &window,
            1704
        ),
        None
    );
    report.phase = Phase::Ended;
    report.sample = Some(sound.duration_samples());
    assert_eq!(run.receive(&report).unwrap(), Some(8197));
    assert!(run.picture_frame().is_err());
}

#[test]
fn sound_loops_keep_monotonic_delivery_and_reject_stale_or_invalid_reports() {
    let domain = Domain::Sound(sound());
    let end = domain.end().unwrap();
    let window = Window::new(AudioSample(0), end, true).unwrap();
    let mut run = Run::with_domain(identity(), domain, window, AudioSample(0)).unwrap();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let mut report = update(&run, end.0 * 2 + 37);
    report.generation = Some(generation);
    assert_eq!(run.receive(&report).unwrap(), Some(37));
    assert_eq!(run.content_sample().unwrap(), AudioSample(37));
    assert_eq!(run.lap().unwrap(), 2);
    for stale in 0..4 {
        let mut wrong = report.clone();
        match stale {
            0 => wrong.ticket += 1,
            1 => wrong.session += 1,
            2 => wrong.revision_id = RevisionId::new("stale").unwrap(),
            _ => wrong.generation = Some(feed.restart(0).unwrap()),
        }
        wrong.sample = Some(AudioSample(report.sample.unwrap().0 + 11));
        assert_eq!(run.receive(&wrong).unwrap(), None);
        assert_eq!(run.content_sample().unwrap(), AudioSample(37));
    }
    report.sample = Some(AudioSample(1));
    assert!(run.receive(&report).is_err());
    report.sample = Some(run.sample);
    report.phase = Phase::Ended;
    assert!(run.receive(&report).is_err());
    assert_eq!(run.picture(37, false), None);
}
