//! Decode both sides of the exact actor-issued insertion before and after commit.
//! PCM uses the canonical limited bus; this does not exercise device delivery.

use super::*;
use crate::worker::{Picture, PreviewWorker, ProjectView, Ticket, Work};
use deadpan_audio::{AudioSourceProvider, LimitedAudio, PreparationError, PreparedSource};
use deadpan_core::{AssetRecord, SourceFrameId};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_store::original_media::OriginalMediaLimits;

/// The playback source cache is private to its crate. This one-source test
/// adapter admits the service's complete receipt and real original bytes, then
/// hands its canonical PCM to the same LimitedAudio implementation as playback.
struct ProposedOriginal {
    document: Arc<ProjectDocument>,
    asset: AssetId,
    source: PreparedSource,
    reads: usize,
}

impl ProposedOriginal {
    fn open(prepared: &Prepared, asset: &AssetId, cancelled: &AtomicBool) -> Self {
        let snapshot = &prepared.snapshot;
        snapshot
            .validate_proposed_base(prepared.base.session, &prepared.base.document)
            .unwrap();
        let entry = &snapshot.sources[asset];
        let record = &snapshot.document.assets()[asset];
        assert_eq!(prepared.base.document.assets().get(asset), Some(record));
        assert_eq!(
            record.source_qualification.as_ref(),
            Some(entry.receipt.id())
        );
        assert_eq!(
            record.content_hash,
            entry.receipt.original().content().to_string()
        );
        assert!(Arc::ptr_eq(
            &entry.receipt,
            &prepared.base.sources[asset].receipt
        ));
        assert_eq!(entry.original, prepared.base.sources[asset].original);
        assert_eq!(entry.original.object(), entry.receipt.original());
        let expected = entry
            .receipt
            .snapshot()
            .audio()
            .expect("qualified A/V fixture");
        assert_eq!(entry.original.sha256(), expected.content().sha256());
        let mut original = snapshot
            .originals
            .snapshot_original(&entry.original, OriginalMediaLimits::default(), cancelled)
            .unwrap();
        let session = AudioSession::open_verified(
            &mut original,
            expected.content(),
            expected.stream().stream_index,
            AudioSessionLimits::default(),
            cancelled,
        )
        .unwrap();
        Self {
            document: snapshot.document.clone(),
            asset: asset.clone(),
            source: PreparedSource::new(session, expected, cancelled).unwrap(),
            reads: 0,
        }
    }
}

impl AudioSourceProvider for ProposedOriginal {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert!(!cancelled.load(Ordering::Acquire));
        assert_eq!(project, self.document.project_id());
        assert_eq!(revision, self.document.revision_id());
        assert_eq!(asset, &self.asset);
        self.reads += 1;
        Ok(&self.source)
    }

    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(self.document.assets().get(asset), Some(expected));
        self.source(project, revision, asset, cancelled)
    }
}

fn decode(worker: &PreviewWorker, serial: u64, work: Work) -> Picture {
    let ticket = Ticket {
        transport: None,
        source: 1,
        request: serial,
    };
    worker.submit(ticket, work);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(reply) = worker.take_reply() {
            assert_eq!(reply.ticket, ticket);
            return reply.picture.unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "splice picture response deadline"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn service_proposal_and_commit_match_decoded_pictures_and_canonical_pcm_at_both_joins() {
    verify_equivalence(false);
}

#[test]
fn interior_proposal_and_commit_match_both_joins_and_preserve_original_suffix_pcm() {
    verify_equivalence(true);
}

fn verify_equivalence(interior: bool) {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let initial = initialize(&harness);
    let original = initial
        .document
        .children(initial.document.root())
        .next()
        .unwrap()
        .clone();
    let before = if interior {
        initial
    } else {
        edited(
            &harness.service,
            &initial,
            ProjectEdit::Split {
                node: original.clone(),
                at: FrameDuration::new(30).unwrap(),
            },
        )
        .workspace
        .unwrap()
    };
    let mut request = proposal(&before, 1, 1);
    // This clock fixture contains isolated impulses, not continuous sound.
    // At 30000/1001 fps, frame 30 begins at sample 48048, beside its 48000
    // impulse. Reusing 0..30 puts the opening impulse (sample 100) after the
    // entry join and the second impulse before both joins, with its decoded
    // AAC tail present after the exit join. All four sides carry real PCM.
    request.ordinals = 0..30;
    request.destination = if interior {
        Destination::Interior {
            target: original,
            at: FrameDuration::new(30).unwrap(),
        }
    } else {
        Destination::Slot(1)
    };
    let reply = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    let proposed = prepared(&reply, &request.id);
    assert_eq!(proposed.range.start(), ProjectFrame(30));
    assert_eq!(proposed.range.end(), ProjectFrame(60));
    unchanged(&before);
    assert!(
        ProjectStore::open(&before.path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot_at(proposed.snapshot.document.revision_id())
            .is_err(),
        "proposed media is inspected before this revision exists in SQLite"
    );

    let frames = [
        proposed.range.start().0 - 1,
        proposed.range.start().0,
        proposed.range.end().0 - 1,
        proposed.range.end().0,
    ];
    let worker = PreviewWorker::new(eframe::egui::Context::default()).unwrap();
    let mut pictures = Vec::new();
    for (index, frame) in frames.into_iter().enumerate() {
        pictures.push(decode(
            &worker,
            index as u64 + 1,
            Work::Proposed {
                base: proposed.base.clone(),
                snapshot: proposed.snapshot.clone(),
                view: ProjectView::Sequence {
                    frame: ProjectFrame(frame),
                },
            },
        ));
    }
    assert_eq!(pictures[1].id, SourceFrameId(request.ordinals.start));
    assert_eq!(pictures[2].id, SourceFrameId(request.ordinals.end - 1));
    assert_ne!(
        pictures[0].id, pictures[1].id,
        "entry join changes the actual source picture"
    );
    assert_ne!(
        pictures[2].id, pictures[3].id,
        "exit join resumes the destination source"
    );

    let cancelled = AtomicBool::new(false);
    let rate = proposed.snapshot.document.presentation_basis().frame_rate;
    let final_boundary = rate
        .audio_boundary(ProjectFrame(proposed.plan.duration().frames()))
        .unwrap();
    let starts = [
        AudioSample(rate.audio_boundary(proposed.range.start()).unwrap().0 - 128),
        AudioSample(rate.audio_boundary(proposed.range.end()).unwrap().0 - 128),
        AudioSample(final_boundary.0 - 256),
    ];
    let mut source = ProposedOriginal::open(&proposed, &request.asset, &cancelled);
    let mut limited = LimitedAudio::new(proposed.plan.clone());
    let mut pcm = Vec::new();
    for (window, start) in starts.into_iter().enumerate() {
        let block = limited
            .read(&mut source, start, 256, Duration::from_secs(60), &cancelled)
            .unwrap();
        for (side_index, side) in [&block.samples[..128], &block.samples[128..]]
            .into_iter()
            .enumerate()
        {
            let peak = side
                .iter()
                .flatten()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            if window < 2 {
                assert!(
                    peak > 1.0e-7,
                    "join block at {start:?}, side {side_index} has decoded fixture audio; peak={peak}"
                );
            }
        }
        assert!(
            block
                .samples
                .iter()
                .flatten()
                .any(|sample| sample.abs() > 1.0e-7)
        );
        pcm.push(block);
    }
    let mut baseline_audio = deadpan_cli::audio::ProjectAudioSession::open_revision(
        &before.path,
        before.document.revision_id(),
    )
    .unwrap();
    let baseline_end = rate
        .audio_boundary(ProjectFrame(before.plan.duration().frames()))
        .unwrap();
    let baseline_suffix = baseline_audio
        .read_limited(AudioSample(baseline_end.0 - 256), 256, &cancelled)
        .unwrap();
    assert_eq!(
        pcm[2].samples, baseline_suffix.samples,
        "interior splitting and placement retain the original suffix sample lattice"
    );
    assert_eq!(pcm[2].gain, baseline_suffix.gain);
    assert!(
        source.reads > 0,
        "canonical proposed preparation admitted original PCM"
    );
    unchanged(&before);

    let committed = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    assert!(committed.error.is_none(), "{:?}", committed.error);
    assert!(committed.splice_commit.as_ref().unwrap().result.is_ok());
    let after = committed.workspace.unwrap();
    assert_eq!(*after.document, *proposed.snapshot.document);
    for (index, frame) in frames.into_iter().enumerate() {
        let actual = decode(
            &worker,
            index as u64 + 5,
            Work::Project {
                workspace: after.clone(),
                view: ProjectView::Sequence {
                    frame: ProjectFrame(frame),
                },
            },
        );
        let expected = &pictures[index];
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.canvas, expected.canvas);
        assert_eq!(actual.framing, expected.framing);
        assert_eq!(actual.framing_gap, expected.framing_gap);
        assert_eq!(actual.picture_context, expected.picture_context);
        assert_eq!(
            actual.frame.as_ref().unwrap().metadata(),
            expected.frame.as_ref().unwrap().metadata()
        );
        assert_eq!(
            actual.frame.as_ref().unwrap().bytes(),
            expected.frame.as_ref().unwrap().bytes()
        );
    }
    let mut committed_audio = deadpan_cli::audio::ProjectAudioSession::open_revision(
        &after.path,
        after.document.revision_id(),
    )
    .unwrap();
    for (start, expected) in starts.into_iter().zip(pcm) {
        let actual = committed_audio
            .read_limited(start, 256, &cancelled)
            .unwrap();
        assert_eq!(
            actual, expected,
            "PCM, limiter gains and owned context agree across the join"
        );
    }
    worker.shutdown();
}
