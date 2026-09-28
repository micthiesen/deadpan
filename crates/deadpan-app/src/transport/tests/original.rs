use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{AssetId, ExactRatio, NodeId, ProjectDocument};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::{SourceQualificationReceipt, SourceRegistration};

use super::*;

/// Produce actual retained evidence through the normal sealed qualification
/// path. Transport itself never opens media, a device, or a GPU.
fn qualified_receipt(name: &str) -> Arc<SourceQualificationReceipt> {
    let scratch = tempfile::tempdir().unwrap();
    let document = ProjectDocument::new_automatic(
        ProjectId::new("transport-fixture").unwrap(),
        RevisionId::new("initial").unwrap(),
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut store =
        ProjectStore::create(&scratch.path().join("original.deadpan"), &document).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(name)
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
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length()).unwrap(),
        2_000_000,
        Duration::from_secs(10),
        &cancelled,
    )
    .unwrap();
    let asset = AssetId::new("original").unwrap();
    let video = SourceSession::open_input(
        input.clone(),
        asset.clone(),
        SourceSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let audio = AudioSession::open_input(
        input,
        video.info().audio_streams[0].stream_index,
        AudioSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio)).unwrap();
    store
        .register_source(
            &SourceRegistration {
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new("registered").unwrap(),
                original: original.object().content().clone(),
                new_asset_id: asset.clone(),
                label: name.into(),
                insertion: None,
            },
            &decoded,
            None,
            limits,
            &cancelled,
        )
        .unwrap();
    Arc::new(
        store
            .registered_source(&RevisionId::new("registered").unwrap(), &asset)
            .unwrap(),
    )
}

fn original(rate: FrameRate, receipt: Arc<SourceQualificationReceipt>) -> Arc<Original> {
    Arc::new(Original::new(rate, AssetId::new("original").unwrap(), receipt).unwrap())
}

#[test]
fn original_vfr_uses_measured_picture_boundaries_at_a_different_project_rate() {
    let receipt = qualified_receipt("vfr.mp4");
    let rate = FrameRate::new(24, 1).unwrap();
    let original = original(rate, receipt.clone());
    let domain = Domain::Original(original.clone());
    let boundaries = (0..=original.frame_count())
        .map(|frame| domain.sample_at_boundary(frame).unwrap().0)
        .collect::<Vec<_>>();
    assert!(
        boundaries
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
    let base = original.index().time_base();
    let tick_seconds =
        ExactRatio::new(i128::from(base.numerator()), i128::from(base.denominator())).unwrap();
    for ordinal in 1..original.frame_count() {
        let expected = ExactRatio::integer(original.index().frames()[ordinal as usize].pts)
            .checked_mul(tick_seconds)
            .unwrap()
            .checked_sub(receipt.snapshot().origin_seconds())
            .unwrap()
            .checked_mul(ExactRatio::integer(48_000))
            .unwrap()
            .round_even()
            .unwrap();
        let sample = domain.sample_at_boundary(ordinal).unwrap();
        assert_eq!(i128::from(sample.0), expected);
        assert_eq!(domain.frame_at_sample(sample).unwrap(), ordinal);
        assert_eq!(
            domain.frame_at_sample(AudioSample(sample.0 - 1)).unwrap(),
            ordinal - 1
        );
    }
    assert_ne!(
        domain.sample_at_boundary(10).unwrap(),
        rate.audio_boundary(ProjectFrame(10)).unwrap()
    );
    assert_eq!(domain.sample_at_boundary(0).unwrap(), AudioSample(0));
    assert_eq!(
        domain.sample_at_boundary(original.frame_count()).unwrap(),
        original.end()
    );
    let window = Window::new(AudioSample(0), original.end(), false).unwrap();
    let mut run = Run::with_domain(identity(), domain, window, AudioSample(0)).unwrap();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    run.generation = Some(feed.restart(0).unwrap());
    let mut terminal = update(&run, original.end().0);
    terminal.phase = Phase::Ended;
    assert_eq!(
        run.receive(&terminal).unwrap(),
        Some(original.frame_count())
    );
    assert_eq!(run.picture_frame().unwrap(), original.frame_count() - 1);
}

#[test]
fn offset_original_subrange_stops_before_out_and_keeps_leading_audio() {
    let receipt = qualified_receipt("offset-bframes.mp4");
    assert_eq!(
        receipt.snapshot().origin_seconds(),
        ExactRatio::new(2971, 1500).unwrap()
    );
    let original = original(FrameRate::new(30_000, 1001).unwrap(), receipt);
    assert_eq!(original.sample_at_boundary(0).unwrap(), AudioSample(0));
    assert_eq!(original.frame_at_sample(AudioSample(1023)).unwrap(), 0);
    assert_eq!(original.sample_at_boundary(1).unwrap(), AudioSample(2626));
    let start = original.sample_at_boundary(10).unwrap();
    let end = original.sample_at_boundary(24).unwrap();
    let window = Window::new(start, end, false).unwrap();
    let mut run = Run::with_domain(
        identity(),
        Domain::Original(original.clone()),
        window,
        start,
    )
    .unwrap();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    run.generation = Some(feed.restart(start.0).unwrap());
    let mut terminal = update(&run, end.0);
    terminal.phase = Phase::Ended;
    assert_eq!(run.receive(&terminal).unwrap(), Some(24));
    assert_eq!(
        run.position().unwrap(),
        Position {
            cursor: 24,
            picture: 23
        }
    );
    assert_eq!(
        original
            .frame_at_sample(AudioSample(original.end().0 - 1))
            .unwrap(),
        original.frame_count() - 1
    );
}

#[test]
fn original_resume_checks_source_receipt_view_rate_and_loop_window() {
    let receipt = qualified_receipt("vfr.mp4");
    let rate = FrameRate::new(24, 1).unwrap();
    let original = original(rate, receipt.clone());
    let domain = Domain::Original(original.clone());
    let start = domain.sample_at_boundary(10).unwrap();
    let end = domain.sample_at_boundary(24).unwrap();
    let window = Window::new(start, end, true).unwrap();
    let delivery = AudioSample(end.0 + end.0 - start.0 + 17);
    let run = Run::with_domain(identity(), domain.clone(), window, delivery).unwrap();
    let resume = run.resume(10);
    let sample = |domain: &Domain, window: &Window| {
        resume.sample_for_domain(
            ContentRef {
                session: run.session,
                project: &run.project,
                revision: &run.revision,
                content: &run.content,
            },
            domain,
            window,
            10,
        )
    };
    assert_eq!(run.lap().unwrap(), 2);
    assert_eq!(run.content_sample().unwrap(), AudioSample(start.0 + 17));
    assert_eq!(sample(&domain, &window), Some(delivery));
    assert_eq!(
        resume.sample_for(run.session, &run.project, &run.revision, 10),
        None
    );
    let equivalent = Domain::Original(Arc::new(
        Original::new(rate, original.asset().clone(), receipt.clone()).unwrap(),
    ));
    assert_eq!(sample(&equivalent, &window), Some(delivery));
    let other_receipt = qualified_receipt("offset-bframes.mp4");
    let variants = [
        Domain::Sequence {
            rate,
            frames: original.duration().frames(),
        },
        Domain::Original(Arc::new(
            Original::new(rate, AssetId::new("other").unwrap(), receipt.clone()).unwrap(),
        )),
        Domain::Original(Arc::new(
            Original::new(
                FrameRate::new(30, 1).unwrap(),
                original.asset().clone(),
                receipt,
            )
            .unwrap(),
        )),
        Domain::Original(Arc::new(
            Original::new(rate, original.asset().clone(), other_receipt).unwrap(),
        )),
    ];
    for other in variants {
        assert_eq!(sample(&other, &window), None);
    }
    assert_eq!(
        sample(&domain, &Window::new(start, end, false).unwrap()),
        None
    );
}

#[test]
fn edge_selection_windows_exclude_audio_lead_tail_and_project_enclosure_until_context_is_added() {
    for name in ["offset-bframes.mp4", "vfr.mp4"] {
        let receipt = qualified_receipt(name);
        let original = original(FrameRate::new(24, 1).unwrap(), receipt.clone());
        let domain = Domain::Original(original.clone());
        let index = original.index();
        let tick = ExactRatio::new(
            i128::from(index.time_base().numerator()),
            i128::from(index.time_base().denominator()),
        )
        .unwrap();
        let expected = |pts| {
            AudioSample(
                i64::try_from(
                    ExactRatio::integer(pts)
                        .checked_mul(tick)
                        .unwrap()
                        .checked_sub(receipt.snapshot().origin_seconds())
                        .unwrap()
                        .checked_mul(ExactRatio::integer(48_000))
                        .unwrap()
                        .round_even()
                        .unwrap(),
                )
                .unwrap(),
            )
        };
        let first = expected(index.frames()[0].pts);
        let last = expected(index.terminal_end());
        let exact = domain
            .selection_window(0..original.frame_count(), AudioSample(0), AudioSample(0))
            .unwrap();
        assert_eq!(exact.start(), first);
        assert_eq!(exact.end(), last);
        assert_eq!(domain.sample_at_boundary(0).unwrap(), AudioSample(0));
        assert_eq!(
            domain.sample_at_boundary(original.frame_count()).unwrap(),
            original.end()
        );
        if name == "offset-bframes.mp4" {
            assert!(first.0 > 0);
        }
        if name == "vfr.mp4" {
            assert!(last < original.end());
        }
        let contextual = domain
            .selection_window(
                0..original.frame_count(),
                AudioSample(i64::MAX),
                AudioSample(i64::MAX),
            )
            .unwrap();
        assert_eq!(contextual.start(), AudioSample(0));
        assert_eq!(contextual.end(), original.end());
        assert!(
            domain
                .selection_window(0..1, AudioSample(-1), AudioSample(0))
                .is_err()
        );
        assert!(
            domain
                .selection_window(0..1, AudioSample(0), AudioSample(-1))
                .is_err()
        );
    }
}
