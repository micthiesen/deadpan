//! Decode-ahead at an edit's discontinuities through the real preview
//! worker: pictures across a Repeat restart are exact and in order, and the
//! look-ahead decoder, not a keyframe seek, serves the restart.

use deadpan_core::{SplitIdentities, WrapAnchorPolicy};
use deadpan_media::lookahead::continuous;

use super::*;

fn commit(fixture: &mut Fixture, name: &str, command: Command) {
    let document = fixture.store.snapshot().unwrap();
    fixture
        .store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        })
        .unwrap();
}

/// `pyramid-bframes.mp4` (96 pictures, keyframes at 0 and 48) split at 30,
/// its second fragment repeated twice: playback jumps back from 95 to 30,
/// a keyframe seek of 31 ordinals for the serving decoder.
fn cut_fixture() -> Fixture {
    let mut fixture = Fixture::source("pyramid-bframes.mp4");
    commit(
        &mut fixture,
        "split",
        Command::Split {
            node: node("source"),
            at: FrameDuration::new(30).unwrap(),
            identities: SplitIdentities {
                nodes: (0..4).map(|i| node(&format!("split-{i}"))).collect(),
            },
        },
    );
    let document = fixture.store.snapshot().unwrap();
    let second = document
        .children(document.root())
        .nth(1)
        .expect("a second fragment")
        .clone();
    commit(
        &mut fixture,
        "repeat",
        Command::WrapRepeat {
            node: second,
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    fixture
}

fn exact(workspace: &Arc<Workspace>, frame: i64) -> Vec<u8> {
    let worker = PreviewWorker::named(egui::Context::default(), "exact").unwrap();
    let request = request(workspace, sequence(frame), 1);
    worker.submit(request.ticket, request.work);
    let picture = await_reply(&worker).picture.unwrap();
    worker.shutdown();
    picture.frame.unwrap().bytes().to_vec()
}

#[test]
fn playback_across_a_repeat_restart_is_exact_in_order_and_served_ahead() {
    let fixture = cut_fixture();
    let workspace = fixture.workspace(1);
    let plan = &workspace.plan;
    let index = workspace.sources[&asset()].video_index.clone().unwrap();
    let frames = plan.duration().frames();
    let expected: Vec<SourceFrameId> = (0..frames)
        .map(|frame| {
            plan.picture(ProjectFrame(frame))
                .unwrap()
                .picture
                .select_source_frame(&index)
                .unwrap()
                .identity
        })
        .collect();
    let cut = (1..frames as usize)
        .find(|frame| !continuous(expected[frame - 1], expected[*frame]))
        .expect("a discontinuity");
    assert_eq!(
        (expected[cut - 1], expected[cut]),
        (SourceFrameId(95), SourceFrameId(30))
    );
    let counters = &deadpan_diagnostics::PLAYBACK_PICTURES;
    let swaps = counters.lookahead_swaps.get();
    let reached = counters.lookahead_reached.get();
    let worker = PreviewWorker::new(egui::Context::default()).unwrap();
    // The editor's stopped picture opens the session before playback.
    let stopped = request(&workspace, sequence(0), 1);
    worker.submit(stopped.ticket, stopped.work);
    assert_eq!(await_reply(&worker).picture.unwrap().id, SourceFrameId(0));
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let mut served = Vec::new();
    let mut pictures = BTreeMap::new();
    for frame in 1..frames {
        if frame as usize == cut {
            // Playback at 24 fps leaves the look-ahead decoder ample time;
            // waiting makes the swap deterministic under test load.
            let deadline = Instant::now() + Duration::from_secs(15);
            while counters.lookahead_reached.get() == reached {
                assert!(Instant::now() < deadline, "the look-ahead never arrived");
                std::thread::sleep(Duration::from_millis(2));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let serial = 1 + frame as u64;
        let mut playing = request(&workspace, sequence(frame), serial);
        playing.ticket.transport = Some(generation);
        worker.submit(playing.ticket, playing.work);
        let reply = await_reply(&worker);
        assert_eq!(reply.ticket.request, serial);
        let picture = reply.picture.unwrap();
        assert_eq!(picture.tier, PictureTier::Original);
        served.push(picture.id);
        if (cut - 1..=cut + 1).contains(&(frame as usize)) {
            pictures.insert(frame, picture.frame.unwrap().bytes().to_vec());
        }
    }
    assert_eq!(served, expected[1..], "every picture, in order");
    assert!(
        counters.lookahead_swaps.get() > swaps,
        "the look-ahead decoder served the restart"
    );
    for (frame, bytes) in pictures {
        assert_eq!(
            bytes,
            exact(&workspace, frame),
            "playback picture {frame} equals an independent exact decode"
        );
    }
    // Stopping releases the look-ahead; a stopped picture stays exact.
    let stopped = request(&workspace, sequence(cut as i64), 1_000);
    worker.submit(stopped.ticket, stopped.work);
    let picture = await_reply(&worker).picture.unwrap();
    assert_eq!(picture.id, SourceFrameId(30));
    worker.shutdown();
}

#[test]
fn only_the_next_playback_picture_keeps_the_lookahead_running() {
    let workspace = Fixture::source("cfr-bframes.mp4").workspace(1);
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let mut mailbox = Mailbox::default();
    let running = || {
        let stop = Arc::new(AtomicBool::new(false));
        (Arc::clone(&stop), Some(stop))
    };

    let (stop, flag) = running();
    mailbox.lookahead = flag;
    let mut playing = request(&workspace, sequence(3), 1);
    playing.ticket.transport = Some(generation);
    mailbox.submit(playing.ticket, playing.work);
    assert!(!stop.load(Ordering::Acquire), "the next playback picture");
    let stopped = request(&workspace, sequence(4), 2);
    mailbox.submit(stopped.ticket, stopped.work);
    assert!(stop.load(Ordering::Acquire), "a stopped picture");

    let (stop, flag) = running();
    mailbox.lookahead = flag;
    let mut newer = request(&workspace, sequence(5), 3);
    mailbox.latest = Some(playing.ticket);
    newer.ticket.transport = Some(feed.restart(0).unwrap());
    mailbox.submit(newer.ticket, newer.work);
    assert!(
        stop.load(Ordering::Acquire),
        "a different output generation"
    );

    let (stop, flag) = running();
    mailbox.lookahead = flag;
    mailbox.cancel();
    assert!(stop.load(Ordering::Acquire), "cancel");

    let (stop, flag) = running();
    mailbox.lookahead = flag;
    mailbox.clear();
    assert!(stop.load(Ordering::Acquire), "clear");

    let (stop, flag) = running();
    mailbox.lookahead = flag;
    mailbox.stop();
    assert!(stop.load(Ordering::Acquire), "shutdown");
}

#[test]
fn original_and_edit_selection_loops_swap_exact_decoders_on_every_lap() {
    for original in [false, true] {
        let fixture = Fixture::source("pyramid-bframes.mp4");
        let workspace = fixture.workspace(1);
        let shared = Shared::default();
        let (mut feed, _callback) = deadpan_output::channel().unwrap();
        let generation = feed.restart(0).unwrap();
        let mut retained = None;
        let mut proposal = None;
        let mut proxy = ProxySlot::default();
        let expected = exact(&workspace, 30);
        for lap in 0..3 {
            for frame in 30..60 {
                let view = if original {
                    ProjectView::Source {
                        asset: asset(),
                        frame: SourceFrameId(frame as u64),
                    }
                } else {
                    sequence(frame)
                };
                let mut playing = request(&workspace, view, (lap * 30 + frame) as u64);
                playing.ticket.transport = Some(generation);
                playing.loop_ahead = LoopLookAhead::new(30, 60, frame + 30);
                shared.mailbox.lock().unwrap().latest = Some(playing.ticket);
                let picture = perform_with(
                    &playing,
                    &mut retained,
                    &mut proposal,
                    Some(ProxyUse::Playback {
                        slot: &mut proxy,
                        picture_period: None,
                    }),
                )
                .unwrap();
                assert_eq!(picture.id, SourceFrameId(frame as u64));
                assert_eq!(picture.tier, PictureTier::Original);
                if frame == 30 {
                    assert_eq!(picture.frame.unwrap().bytes(), expected);
                    if lap > 0 {
                        assert_eq!(
                            retained
                                .as_ref()
                                .unwrap()
                                .ahead
                                .lookahead
                                .as_ref()
                                .unwrap()
                                .stats()
                                .swaps,
                            u64::try_from(lap).unwrap()
                        );
                    }
                }
                schedule_lookahead(&shared, &playing, &mut retained);
            }
            // Readiness is tested independently of machine load. Latency is
            // measured by the real-device benchmark, not this pixel test.
            let ahead = retained.as_mut().unwrap().ahead.lookahead.as_mut().unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            while ahead.positioned() != Some(SourceFrameId(30)) {
                assert!(Instant::now() < deadline, "loop start was not prepared");
                ahead.poll();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(ahead.stats().failed, 0);
        }
        retained
            .as_mut()
            .unwrap()
            .ahead
            .lookahead
            .as_mut()
            .unwrap()
            .release();
    }
}
