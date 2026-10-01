//! Service-issued Move previews use the existing sealed picture/PCM boundary.
//! Explicit source ordinals and PCM phases are independent of the proposed plan.
use super::*;
use crate::project::slice::{CaptureRequest, CopyId};
use crate::project::splice::{Operation, PreparedMedia};
use deadpan_audio::{ResampleRecipe, StageAudio};
use deadpan_core::{
    AudioEdgePolicy, CapturedCanvas, CapturedFit, CapturedFraming, ClipGain, ExactFrameRange,
    ExactRatio, FrameRange, Framing, FramingPose, GainDb, SoundEvent, SoundId, SoundOverflowPolicy,
    SourceAudio, SourceAudioMapping, SourceTimeBase, SourceTimestamp, SplitIdentities,
};

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn move_proposal(
    harness: &Harness,
    before: &Workspace,
    source_scope: SequenceScope,
    selected: FrameRange,
    destination_scope: SequenceScope,
    destination: Destination,
) -> Proposal {
    let source_parent = source_scope.resolve(before).unwrap().owner.clone();
    let capture = CaptureRequest {
        id: CopyId {
            session: before.session,
            project: before.document.project_id().clone(),
            source_revision: before.document.revision_id().clone(),
            request: 1,
        },
        scope: source_scope,
        parent: source_parent,
        selection: deadpan_core::SliceCaptureSelection::Range { range: selected },
    };
    let update = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(capture.clone()),
    );
    let reply = update.captured_slice.unwrap();
    assert_eq!(reply.id, capture.id);
    let copied = reply.result.unwrap();
    let parent = destination_scope.resolve(before).unwrap().owner.clone();
    Proposal {
        id: ProposalId {
            session: before.session,
            project: before.document.project_id().clone(),
            base_revision: before.document.revision_id().clone(),
            draft: 1,
            change: 1,
        },
        operation: Operation::Move,
        source: Source::Edited {
            copied,
            range: selected,
        },
        scope: destination_scope,
        parent,
        destination,
    }
}

fn media(prepared: &Prepared) -> Arc<crate::project::slice::MediaView> {
    let PreparedMedia::Edited(media) = &prepared.media else {
        panic!("Move needs sealed edited admission")
    };
    prepared
        .snapshot
        .validate_edit_slice_view(media.admitted())
        .unwrap();
    assert!(prepared.snapshot.validate_original_proposal().is_err());
    media.clone()
}

fn proposed_picture(prepared: &Prepared, frame: i64) -> Work {
    Work::EditedProposed {
        base: prepared.base.clone(),
        snapshot: prepared.snapshot.clone(),
        media: media(prepared),
        frame: ProjectFrame(frame),
    }
}

fn assert_picture_equal(actual: &Picture, expected: &Picture) {
    assert_eq!(actual.id, expected.id);
    assert_eq!(actual.canvas, expected.canvas);
    assert_eq!(actual.framing, expected.framing);
    assert_eq!(actual.framing_gap, expected.framing_gap);
    assert_eq!(actual.picture_context, expected.picture_context);
    let actual = actual.frame.as_ref().unwrap();
    let expected = expected.frame.as_ref().unwrap();
    assert_eq!(actual.metadata(), expected.metadata());
    assert_eq!(actual.bytes(), expected.bytes());
}

fn assert_decoded_original(
    worker: &PreviewWorker,
    prepared: &Prepared,
    output: i64,
    ordinal: u64,
    asset: &AssetId,
    serial: &mut u64,
) -> Picture {
    *serial += 1;
    let actual = decode(worker, *serial, proposed_picture(prepared, output));
    *serial += 1;
    let original = decode(
        worker,
        *serial,
        Work::Project {
            workspace: prepared.base.clone(),
            view: ProjectView::Source {
                asset: asset.clone(),
                frame: SourceFrameId(ordinal),
            },
        },
    );
    assert_eq!(actual.id, SourceFrameId(ordinal));
    assert_eq!(
        actual.frame.as_ref().unwrap().metadata().pts.ticks,
        i64::try_from(ordinal).unwrap() * 1001
    );
    assert_eq!(
        actual.frame.as_ref().unwrap().metadata(),
        original.frame.as_ref().unwrap().metadata()
    );
    assert_eq!(
        actual.frame.as_ref().unwrap().bytes(),
        original.frame.as_ref().unwrap().bytes()
    );
    actual
}

fn limited_windows(
    prepared: &Prepared,
    asset: &AssetId,
    starts: &[i64],
) -> Vec<deadpan_audio::LimitedAudioBlock> {
    let cancelled = AtomicBool::new(false);
    let mut source = ProposedOriginal::open(prepared, asset, &cancelled);
    let mut limited = LimitedAudio::new(prepared.plan.clone());
    let mut blocks = Vec::new();
    for &start in starts {
        blocks.push(
            limited
                .read(
                    &mut source,
                    AudioSample(start),
                    256,
                    Duration::from_secs(60),
                    &cancelled,
                )
                .unwrap(),
        );
    }
    assert!(source.reads > 0);
    assert!(
        blocks
            .iter()
            .flat_map(|block| block.samples.iter().flatten())
            .any(|sample| sample.abs() > 1.0e-7)
    );
    // A fresh preparation begins at the last, independently chosen site. Warm
    // re-reads then reverse the order; neither read order changes canonical PCM.
    let mut cold = LimitedAudio::new(prepared.plan.clone());
    let last = starts.len() - 1;
    assert_eq!(
        cold.read(
            &mut source,
            AudioSample(starts[last]),
            256,
            Duration::from_secs(60),
            &cancelled
        )
        .unwrap(),
        blocks[last]
    );
    for (index, &start) in starts.iter().enumerate().rev() {
        assert_eq!(
            limited
                .read(
                    &mut source,
                    AudioSample(start),
                    256,
                    Duration::from_secs(60),
                    &cancelled
                )
                .unwrap(),
            blocks[index]
        );
    }
    blocks
}

fn commit_and_compare(
    harness: &Harness,
    request: Proposal,
    prepared: &Prepared,
    worker: &PreviewWorker,
    pictures: &[(i64, Picture)],
    starts: &[i64],
    pcm: &[deadpan_audio::LimitedAudioBlock],
) -> Arc<Workspace> {
    unchanged(&prepared.base);
    let update = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    assert!(update.error.is_none(), "{:?}", update.error);
    assert!(update.splice_commit.as_ref().unwrap().result.is_ok());
    let after = update.workspace.unwrap();
    assert_eq!(*after.document, *prepared.snapshot.document);
    for (serial, (frame, expected)) in pictures.iter().enumerate() {
        let actual = decode(
            worker,
            100 + u64::try_from(serial).unwrap(),
            Work::Project {
                workspace: after.clone(),
                view: ProjectView::Sequence {
                    frame: ProjectFrame(*frame),
                },
            },
        );
        assert_picture_equal(&actual, expected);
    }
    let mut reader = deadpan_cli::audio::ProjectAudioSession::open_revision(
        &after.path,
        after.document.revision_id(),
    )
    .unwrap();
    for (&start, expected) in starts.iter().zip(pcm) {
        assert_eq!(
            &reader
                .read_limited(AudioSample(start), 256, &AtomicBool::new(false))
                .unwrap(),
            expected
        );
    }
    after
}

fn boundary(frame: i64) -> i64 {
    // Independent B(f) for positive NTSC frames, with no iterative rounding.
    let numerator = frame * 8008;
    let whole = numerator / 5;
    let remainder = numerator % 5;
    whole + i64::from(remainder * 2 > 5)
}

// Exact five-frame-multiple cuts in the actual 48 kHz fixture. Each tuple is
// (old first frame, new first frame, length), authored independently of Move.
fn raw_reference(
    source: &PreparedSource,
    islands: &[(i64, i64, i64)],
    start: i64,
    count: u32,
) -> Vec<[f32; 2]> {
    let mut result = Vec::new();
    for &(old, new, length) in islands {
        let from = start.max(boundary(new));
        let to = (start + i64::from(count)).min(boundary(new + length));
        if from >= to {
            continue;
        }
        let old_sample = boundary(old) + from - boundary(new);
        result.extend(
            source
                .prepare(
                    ResampleRecipe::new(
                        0..192_192,
                        ExactRatio::integer(old_sample),
                        AudioSample(0),
                        ExactRatio::ONE,
                        AudioSample(0)..AudioSample(to - from),
                    )
                    .unwrap(),
                    AudioSample(0),
                    u32::try_from(to - from).unwrap(),
                    Duration::from_secs(60),
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    assert_eq!(result.len(), usize::try_from(count).unwrap());
    result
}

#[test]
fn moves_in_both_directions_decode_both_sites_and_match_their_committed_canonical_pcm() {
    for (selected, destination, inserted, removal, islands, checks) in [
        (
            range(20, 30),
            60,
            range(50, 60),
            20,
            vec![(0, 0, 20), (30, 20, 30), (20, 50, 10), (60, 60, 60)],
            vec![(19, 19), (20, 30), (49, 59), (50, 20), (59, 29), (60, 60)],
        ),
        (
            range(70, 80),
            10,
            range(10, 20),
            80,
            vec![(0, 0, 10), (70, 10, 10), (10, 20, 60), (80, 80, 40)],
            vec![(9, 9), (10, 70), (19, 79), (20, 10), (79, 69), (80, 80)],
        ),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let harness = Harness::with_library(Some(
            ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
        ));
        let before = initialize(&harness);
        assert_eq!(before.plan.duration().frames(), 120);
        let original = before
            .document
            .children(before.document.root())
            .next()
            .unwrap()
            .clone();
        let asset = before.sources.keys().next().unwrap().clone();
        assert_eq!(
            before.sources[&asset]
                .receipt
                .snapshot()
                .audio()
                .unwrap()
                .stream()
                .sample_rate,
            48_000
        );
        let request = move_proposal(
            &harness,
            &before,
            SequenceScope::default(),
            selected,
            SequenceScope::default(),
            Destination::Interior {
                target: original,
                at: FrameDuration::new(destination).unwrap(),
            },
        );
        let reply = command(
            &harness.service,
            ProjectRequest::PrepareSplice(request.clone()),
        );
        let proposed = prepared(&reply, &request.id);
        proposed.validate_result().unwrap();
        assert_eq!(proposed.range, inserted);
        assert_eq!(proposed.removed, None);
        let movement = proposed.movement.as_ref().unwrap();
        assert_eq!(movement.source_before, selected);
        assert_eq!(movement.destination_before, ProjectFrame(destination));
        assert_eq!(movement.removal_after, ProjectFrame(removal));
        assert_eq!(proposed.plan.duration().frames(), 120);
        assert!(
            ProjectStore::open(&before.path, AccessMode::ReadOnly)
                .unwrap()
                .snapshot_at(proposed.snapshot.document.revision_id())
                .is_err()
        );
        let index = before.sources[&asset].video_index.as_ref().unwrap();
        for &(old, new, length) in &islands {
            for offset in 0..length {
                let picture = proposed.plan.picture(ProjectFrame(new + offset)).unwrap();
                assert_eq!(
                    picture.picture.select_source_frame(index).unwrap().identity,
                    SourceFrameId(u64::try_from(old + offset).unwrap())
                );
            }
        }
        let worker = PreviewWorker::new(eframe::egui::Context::default()).unwrap();
        let mut serial = 0;
        let pictures: Vec<_> = checks
            .into_iter()
            .map(|(frame, ordinal)| {
                (
                    frame,
                    assert_decoded_original(
                        &worker,
                        &proposed,
                        frame,
                        ordinal,
                        &asset,
                        &mut serial,
                    ),
                )
            })
            .collect();
        // Both affected sites, both insertion edges, plus the known second
        // impulse inside the displaced interval in the left-moving case.
        let starts = [
            boundary(removal) - 128,
            boundary(inserted.start().0) - 128,
            boundary(inserted.end().0) - 128,
            if destination == 10 {
                boundary(40) - 128
            } else {
                boundary(60) - 128
            },
        ];
        let blocks = limited_windows(&proposed, &asset, &starts);
        let mut source = ProposedOriginal::open(&proposed, &asset, &AtomicBool::new(false));
        let mut raw = StageAudio::new(proposed.plan.clone());
        for &start in &starts {
            let expected = raw_reference(&source.source, &islands, start, 256);
            let actual = raw
                .read(
                    &mut source,
                    AudioSample(start),
                    256,
                    Duration::from_secs(60),
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                actual.samples, expected,
                "independent original sample clock at {start}"
            );
        }
        commit_and_compare(
            &harness, request, &proposed, &worker, &pictures, &starts, &blocks,
        );
        worker.shutdown();
    }
}

fn splits(name: &str) -> SplitIdentities {
    SplitIdentities {
        nodes: (0..8)
            .map(|index| node(&format!("{name}-{index}")))
            .collect(),
    }
}

fn pose(scale: i64) -> FramingPose {
    FramingPose {
        scale: ExactRatio::integer(scale),
        ..Default::default()
    }
}

fn owned_picture_context() -> CapturedFraming {
    CapturedFraming {
        canvases: vec![CapturedCanvas {
            width: 320,
            height: 180,
            fit: CapturedFit::Fit,
            layers: vec![Some(pose(5))],
        }],
    }
}

fn seed_cross_parent(harness: &Harness, initial: &Workspace) -> Arc<Workspace> {
    command(&harness.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&initial.path, AccessMode::ReadWrite).unwrap();
    let original = initial
        .document
        .children(initial.document.root())
        .next()
        .unwrap()
        .clone();
    seed_command(
        &mut store,
        Command::Split {
            node: original,
            at: FrameDuration::new(40).unwrap(),
            identities: splits("first-cut"),
        },
        "first-cut",
    );
    let right = store
        .snapshot()
        .unwrap()
        .children(initial.document.root())
        .nth(1)
        .unwrap()
        .clone();
    seed_command(
        &mut store,
        Command::Split {
            node: right,
            at: FrameDuration::new(40).unwrap(),
            identities: splits("second-cut"),
        },
        "second-cut",
    );
    seed_command(
        &mut store,
        Command::Group {
            parent: initial.document.root().clone(),
            start: 0,
            end: 2,
            id: node("a"),
            label: "Donor".into(),
        },
        "group-a",
    );
    seed_command(
        &mut store,
        Command::Group {
            parent: initial.document.root().clone(),
            start: 1,
            end: 2,
            id: node("b"),
            label: "Destination".into(),
        },
        "group-b",
    );
    let asset = initial.sources.keys().next().unwrap().clone();
    seed_command(
        &mut store,
        Command::InsertTime {
            at: ProjectFrame(40),
            id: node("owned-hold"),
            identities: splits("pause-cut"),
            timing: deadpan_core::AudioTimingId {
                allocation: RevisionId::new("owned-pause").unwrap(),
                ordinal: 0,
            },
            hold: HoldRecipe {
                duration: FrameDuration::new(5).unwrap(),
                video: HoldVideo::Freeze {
                    asset: asset.clone(),
                    timestamp: SourceTimestamp {
                        ticks: 39 * 1001,
                        time_base: SourceTimeBase::new(1, 30_000).unwrap(),
                    },
                },
                audio: HoldAudio::Silence,
                picture_context: Some(owned_picture_context()),
            },
        },
        "owned-pause",
    );
    for (owner, scale, gain) in [("a", 2, 6000), ("b", 3, -6000), ("owned-hold", 4, 0)] {
        seed_command(
            &mut store,
            Command::SetFraming {
                node: node(owner),
                framing: Some(Framing::static_pose(pose(scale)).unwrap()),
            },
            &format!("frame-{owner}"),
        );
        seed_command(
            &mut store,
            Command::SetAudioTreatments {
                node: node(owner),
                treatments: deadpan_core::AudioTreatments::from_clip_gain(
                    ClipGain::new(GainDb::new(gain).unwrap(), false, vec![], vec![]).unwrap(),
                ),
            },
            &format!("gain-{owner}"),
        );
    }
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    seed_command(
        &mut store,
        Command::SetSound {
            id: SoundId::new("root-sound").unwrap(),
            event: SoundEvent {
                owner: initial.document.root().clone(),
                label: "Fixed root impulse".into(),
                source: SourceAudio {
                    asset,
                    span: deadpan_core::SourceSpan::new(
                        SourceTimestamp {
                            ticks: 0,
                            time_base: clock,
                        },
                        SourceTimestamp {
                            ticks: 16016,
                            time_base: clock,
                        },
                    )
                    .unwrap(),
                },
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::integer(30),
                    frames: ExactRatio::integer(10),
                    selection: ExactFrameRange::new(
                        ExactRatio::integer(30),
                        ExactRatio::integer(32),
                    )
                    .unwrap(),
                },
                offset: AudioSample(0),
                gain_millidecibels: -6000,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
        "root-sound",
    );
    drop(store);
    command(&harness.service, ProjectRequest::Open(initial.path.clone()))
        .workspace
        .unwrap()
}

#[test]
fn cross_parent_forest_preview_keeps_hold_context_live_treatments_and_fixed_root_sound() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let initial = initialize(&harness);
    let before = seed_cross_parent(&harness, &initial);
    assert_eq!(before.plan.duration().frames(), 125);
    let source_scope = SequenceScope::default()
        .descend(&before, &node("a"))
        .unwrap();
    let destination_scope = SequenceScope::default()
        .descend(&before, &node("b"))
        .unwrap();
    let target = destination_scope.resolve(&before).unwrap().children[0].clone();
    let request = move_proposal(
        &harness,
        &before,
        source_scope,
        range(30, 55),
        destination_scope,
        Destination::Interior {
            target,
            at: FrameDuration::new(20).unwrap(),
        },
    );
    let reply = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    let proposed = prepared(&reply, &request.id);
    proposed.validate_result().unwrap();
    assert_eq!(proposed.range, range(80, 105));
    assert_eq!(proposed.parent, node("b"));
    assert_eq!(
        proposed
            .plan
            .node_duration(&proposed.node)
            .unwrap()
            .frames(),
        10
    );
    assert_eq!(
        proposed.movement.as_ref().unwrap().removal_after,
        ProjectFrame(30)
    );
    assert_eq!(
        proposed.movement.as_ref().unwrap().destination_before,
        ProjectFrame(105)
    );
    assert_eq!(
        proposed.snapshot.document.nodes()[&node("owned-hold")],
        before.document.nodes()[&node("owned-hold")]
    );
    assert_eq!(
        proposed.snapshot.document.sounds(),
        before.document.sounds()
    );
    assert_eq!(
        proposed.snapshot.document.sound_routes(),
        before.document.sound_routes()
    );
    let asset = before.sources.keys().next().unwrap().clone();
    let index = before.sources[&asset].video_index.as_ref().unwrap();
    // Original frame order is independent of the proposal's structural splits.
    let ordinal = |frame: i64| match frame {
        0..30 => frame,
        30..80 => frame + 20,
        80..90 => frame - 50,
        90..95 => 39,
        95..105 => frame - 55,
        _ => frame - 5,
    };
    for frame in 0..125 {
        let sample = proposed.plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(
            sample.picture.select_source_frame(index).unwrap().identity,
            SourceFrameId(u64::try_from(ordinal(frame)).unwrap())
        );
        let (owner, scale, start, length) = if frame < 60 {
            ("a", 2, 0, 60)
        } else {
            ("b", 3, 60, 65)
        };
        let layer = sample
            .framing
            .iter()
            .find(|layer| layer.instance.node == node(owner))
            .unwrap();
        assert_eq!(layer.pose.unwrap().scale, ExactRatio::integer(scale));
        assert_eq!(
            layer.local_position,
            ExactRatio::new(i128::from(2 * (frame - start) + 1), 2).unwrap()
        );
        assert_eq!(layer.duration.frames(), length);
        assert!(
            !sample
                .framing
                .iter()
                .any(|layer| layer.instance.node == node(if owner == "a" { "b" } else { "a" }))
        );
    }
    let worker = PreviewWorker::new(eframe::egui::Context::default()).unwrap();
    let mut serial = 0;
    let pictures: Vec<_> = [29, 30, 59, 60, 79, 80, 89, 90, 94, 95, 104, 105]
        .into_iter()
        .map(|frame| {
            let picture = assert_decoded_original(
                &worker,
                &proposed,
                frame,
                u64::try_from(ordinal(frame)).unwrap(),
                &asset,
                &mut serial,
            );
            if (90..95).contains(&frame) {
                assert_eq!(
                    picture.picture_context.as_deref(),
                    Some(&owned_picture_context())
                );
                assert!(
                    picture
                        .framing
                        .iter()
                        .any(|layer| layer.instance.node == node("owned-hold")
                            && layer.pose.unwrap().scale == ExactRatio::integer(4))
                );
            }
            (frame, picture)
        })
        .collect();
    let starts = [
        boundary(30) - 128,
        boundary(80) - 128,
        boundary(90) - 128,
        boundary(95) - 128,
        boundary(105) - 128,
    ];
    let blocks = limited_windows(&proposed, &asset, &starts);
    let mut source = ProposedOriginal::open(&proposed, &asset, &AtomicBool::new(false));
    let mut bus = StageAudio::new(proposed.plan.clone());
    // Interior points avoid newly authored 96-sample edges. Root sound is at
    // frame30, so neither window below contains it. The incoming ancestor gain
    // must be B's -6dB, while the unchanged prefix still uses A's +6dB.
    for (output, original, gain) in [
        (128, 128, 6000.),
        (boundary(80) + 128, boundary(30) + 128, -6000.),
    ] {
        let reference = raw_reference(&source.source, &[(0, 0, 120)], original, 128);
        let expected: Vec<_> = reference
            .iter()
            .map(|sample| {
                sample.map(|value| (f64::from(value) * 10_f64.powf(gain / 20000.)) as f32)
            })
            .collect();
        let actual = bus
            .prepare_authored_bus(
                &mut source,
                AudioSample(output),
                128,
                Duration::from_secs(60),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(expected.iter().flatten().any(|value| value.abs() > 1.0e-7));
        for (actual, expected) in actual.samples.iter().zip(expected) {
            for channel in 0..2 {
                assert!((actual[channel] - expected[channel]).abs() <= 2.0e-6);
            }
        }
    }
    // At the removal join the independent sound stays at root frame30, while
    // the structural source is now Original50. Its gain excludes parent A.
    let voice = raw_reference(&source.source, &[(0, 0, 120)], boundary(50) + 128, 128);
    let effect = raw_reference(&source.source, &[(0, 0, 120)], 128, 128);
    assert!(effect.iter().flatten().any(|value| value.abs() > 1.0e-7));
    let mixed = bus
        .prepare_authored_bus(
            &mut source,
            AudioSample(boundary(30) + 128),
            128,
            Duration::from_secs(60),
            &AtomicBool::new(false),
        )
        .unwrap();
    for ((actual, voice), effect) in mixed.samples.iter().zip(voice).zip(effect) {
        for channel in 0..2 {
            let expected = f64::from(voice[channel]) * 10_f64.powf(6000. / 20000.)
                + f64::from(effect[channel]) * 10_f64.powf(-6000. / 20000.);
            assert!((f64::from(actual[channel]) - expected).abs() <= 2.0e-6);
        }
    }
    let after = commit_and_compare(
        &harness, request, &proposed, &worker, &pictures, &starts, &blocks,
    );
    assert_eq!(after.document.sounds(), before.document.sounds());
    assert_eq!(
        after.document.sound_routes(),
        before.document.sound_routes()
    );
    worker.shutdown();
}

#[test]
fn qualified_historical_register_keeps_endpoint_pictures_but_cannot_authorize_move() {
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
    let mut request = move_proposal(
        &harness,
        &initial,
        SequenceScope::default(),
        range(20, 30),
        SequenceScope::default(),
        Destination::Interior {
            target: original.clone(),
            at: FrameDuration::new(60).unwrap(),
        },
    );
    let current = edited(
        &harness.service,
        &initial,
        ProjectEdit::SetFraming {
            node: original,
            framing: Some(Framing::static_pose(pose(2)).unwrap()),
        },
    )
    .workspace
    .unwrap();
    request.id.base_revision = current.document.revision_id().clone();
    let rejected = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    let reply = rejected.splice.unwrap();
    assert!(reply.result.err().unwrap().contains("older revision"));
    let source_view = reply.source_view.unwrap().result.unwrap();
    let worker = PreviewWorker::new(eframe::egui::Context::default()).unwrap();
    for (local, ordinal) in [(0, 20), (9, 29)] {
        let picture = decode(
            &worker,
            u64::try_from(local + 1).unwrap(),
            Work::Copied {
                view: source_view.clone(),
                frame: ProjectFrame(local),
            },
        );
        assert_eq!(picture.id, SourceFrameId(ordinal));
        assert_eq!(
            picture.frame.as_ref().unwrap().metadata().pts.ticks,
            i64::try_from(ordinal).unwrap() * 1001
        );
    }
    request.operation = Operation::Copy;
    request.id.change += 1;
    let reply = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    let copied = prepared(&reply, &request.id);
    assert_eq!(copied.plan.duration().frames(), 130);
    assert!(copied.movement.is_none());
    unchanged(&current);
    worker.shutdown();
}
