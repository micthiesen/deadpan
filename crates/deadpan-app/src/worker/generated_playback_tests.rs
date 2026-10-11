//! Real accepted-media decoder handoff. Export the fixture by running the
//! CLI generated_joins test `accepted_ai_pause_meets_the_original_with_exact_hard_cuts`
//! with `DEADPAN_GENERATED_PREVIEW_EXPORT=/tmp/NEW_PACKAGE.deadpan` and
//! `DEADPAN_REQUIRE_SYNTHETIC_WORKER=1` (feature `synthetic-worker`), then run this
//! ignored test with `DEADPAN_GENERATED_PREVIEW_PACKAGE` pointing to its
//! package. The input package is copied through verified portable
//! export, never changed in place. No model or native window is opened.

use super::super::playback_cache::PlaybackCache;
use super::*;

fn fixture() -> Fixture {
    fixture_from("DEADPAN_GENERATED_PREVIEW_PACKAGE")
}

fn fixture_from(variable: &str) -> Fixture {
    let package = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{variable} must name the accepted bundle fixture"));
    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join("preview.deadpan");
    deadpan_store::portable::copy_portable(&package, &copy, &AtomicBool::new(false)).unwrap();
    let store = ProjectStore::open(&copy, deadpan_store::AccessMode::ReadWrite).unwrap();
    Fixture { scratch, store }
}

#[test]
#[ignore = "requires DEADPAN_GENERATED_TRANSITIONS_PACKAGE: two accepted Generated Holds"]
fn generated_preview_slices_equal_the_single_threaded_offline_reader() {
    let fixture = fixture_from("DEADPAN_GENERATED_TRANSITIONS_PACKAGE");
    let workspace = fixture.workspace(709);
    let cancelled = AtomicBool::new(false);
    let mut previous = None;
    let mut providers = 0;
    for frame in 0..workspace.plan.duration().frames() {
        let deadpan_plan::Picture::Accepted {
            generated: Some(artifact),
            ..
        } = workspace.plan.picture(ProjectFrame(frame)).unwrap().picture
        else {
            continue;
        };
        if previous.as_ref() == Some(&artifact) {
            continue;
        }
        let key = SessionKey::Generated {
            session: workspace.session,
            media: Arc::new(GeneratedMediaKey::new(&workspace.document, &artifact).unwrap()),
        };
        let mut preview =
            admit_generated(&workspace.generated, &workspace.document, key, &cancelled)
                .unwrap()
                .source;
        let mut offline = deadpan_cli::picture::open_generated_picture(
            &workspace.generated,
            &workspace.document,
            &artifact,
            &cancelled,
        )
        .unwrap();
        let threads = std::thread::available_parallelism().unwrap().get().min(4) as u32;
        let work = preview.decode_work();
        assert_eq!(work.codec_threads, threads);
        assert_eq!(work.slice_threading, threads > 1);
        assert!(!work.frame_threading);
        assert_eq!(offline.decode_work().codec_threads, 1);
        assert_eq!(preview.info(), offline.info());
        assert_eq!(preview.index().index(), offline.index().index());
        let frames: Vec<_> = offline
            .index()
            .index()
            .frames()
            .iter()
            .map(|frame| frame.identity)
            .collect();
        for id in frames.iter().chain(frames.iter().rev()) {
            let actual = preview.frame(*id, FRAME_TIMEOUT, &cancelled).unwrap();
            let expected = offline.frame(*id, FRAME_TIMEOUT, &cancelled).unwrap();
            assert_eq!(actual.metadata, expected.metadata);
            assert_eq!(actual.rgba, expected.rgba);
        }
        providers += 1;
        previous = Some(artifact);
    }
    assert_eq!(providers, 2);
}

#[test]
#[ignore = "requires DEADPAN_GENERATED_TRANSITIONS_PACKAGE: two accepted Holds separated by one Original picture"]
fn starting_on_generated_prepares_original_and_revokes_stale_admissions() {
    for mode in ["adopt", "cancel", "different-session"] {
        let fixture = fixture_from("DEADPAN_GENERATED_TRANSITIONS_PACKAGE");
        let workspace = fixture.workspace(707);
        let generated = (0..workspace.plan.duration().frames())
            .find(|frame| {
                matches!(
                    workspace
                        .plan
                        .picture(ProjectFrame(*frame))
                        .unwrap()
                        .picture,
                    deadpan_plan::Picture::Accepted {
                        generated: Some(_),
                        ..
                    }
                )
            })
            .unwrap();
        let original = (generated + 1..workspace.plan.duration().frames())
            .find(|frame| {
                matches!(
                    workspace
                        .plan
                        .picture(ProjectFrame(*frame))
                        .unwrap()
                        .picture,
                    deadpan_plan::Picture::Source { .. }
                )
            })
            .unwrap();
        let shared = Shared::default();
        let mut cache = PlaybackCache::default();
        let mut retained = None;
        let (mut feed, _callback) = deadpan_output::channel().unwrap();
        let generation = feed.restart(0).unwrap();
        let mut entry = request(&workspace, sequence(generated), 1);
        // Stopped preview starts with only the accepted provider admitted.
        perform(&entry, &mut retained, &mut None).unwrap();
        let generated_index = Arc::downgrade(&retained.as_ref().unwrap().source.shared_index());
        entry.ticket.transport = Some(generation);
        shared.mailbox.lock().unwrap().latest = Some(entry.ticket);
        cache.prepare(&shared, &entry, &retained);
        let stop = shared
            .mailbox
            .lock()
            .unwrap()
            .original_ahead
            .clone()
            .expect("Original admission began");
        let generated_stop = shared
            .mailbox
            .lock()
            .unwrap()
            .generated_ahead
            .clone()
            .expect("next Generated admission began");
        let deadline = Instant::now() + Duration::from_secs(15);
        while !cache.original_finished() || !cache.generated_finished() {
            assert!(
                Instant::now() < deadline,
                "both providers prepare before the first Original request"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        if mode == "cancel" {
            shared.mailbox.lock().unwrap().cancel();
            assert!(stop.load(Ordering::Acquire) && generated_stop.load(Ordering::Acquire));
        }
        let workspace = if mode == "different-session" {
            fixture.workspace(708)
        } else {
            workspace
        };
        let mut next = request(&workspace, sequence(original), 2);
        next.ticket.transport = Some(generation);
        cache.select(&next, &mut retained, true).unwrap();
        if mode == "adopt" {
            let session = retained.as_ref().expect("prepared Original adopted");
            assert!(matches!(session.key, SessionKey::Project { .. }));
            let sample = workspace.plan.picture(ProjectFrame(original)).unwrap();
            let id = sample
                .picture
                .select_source_frame(session.source.index().index())
                .unwrap()
                .identity;
            assert_eq!(
                session.source.decode_plan(id),
                deadpan_media::source_session::DecodePlan::Current
            );
        } else {
            assert!(
                retained.is_none(),
                "cancelled or stale Original was not adopted"
            );
            assert!(stop.load(Ordering::Acquire));
        }
        let actual = perform(&next, &mut retained, &mut None).unwrap();
        let expected = perform(&next, &mut None, &mut None).unwrap();
        assert_eq!(actual.id, expected.id);
        assert_eq!(
            actual.frame.unwrap().bytes(),
            expected.frame.unwrap().bytes()
        );
        cache.release(&mut retained);
        assert!(generated_stop.load(Ordering::Acquire));
        assert!(generated_index.upgrade().is_none());
    }
}

#[test]
#[ignore = "requires DEADPAN_GENERATED_TRANSITIONS_PACKAGE: two accepted Holds separated by one Original picture"]
fn next_generated_provider_prepares_while_the_current_hold_serves() {
    let fixture = fixture_from("DEADPAN_GENERATED_TRANSITIONS_PACKAGE");
    let workspace = fixture.workspace(706);
    let mut entries = Vec::new();
    let mut previous = None;
    for frame in 0..workspace.plan.duration().frames() {
        let artifact = match workspace.plan.picture(ProjectFrame(frame)).unwrap().picture {
            deadpan_plan::Picture::Accepted { generated, .. } => generated,
            _ => None,
        };
        if artifact.is_some() && artifact != previous {
            entries.push(frame);
        }
        previous = artifact;
    }
    assert_eq!(entries.len(), 2);
    let shared = Shared::default();
    let mut cache = PlaybackCache::default();
    let mut retained = None;
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let playing = |frame| {
        let mut request = request(&workspace, sequence(frame), frame as u64 + 1);
        request.ticket.transport = Some(generation);
        shared.mailbox.lock().unwrap().latest = Some(request.ticket);
        request
    };
    let wait = |cache: &PlaybackCache| {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !cache.generated_finished() {
            assert!(Instant::now() < deadline, "next Generated was not prepared");
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    let zero = playing(0);
    perform(&zero, &mut retained, &mut None).unwrap();
    let original_index = Arc::downgrade(&retained.as_ref().unwrap().source.shared_index());
    cache.prepare(&shared, &zero, &retained);
    wait(&cache);
    let first = playing(entries[0]);
    cache.select(&first, &mut retained, true).unwrap();
    perform(&first, &mut retained, &mut None).unwrap();
    let first_index = Arc::downgrade(&retained.as_ref().unwrap().source.shared_index());
    cache.prepare(&shared, &first, &retained);
    let stop = shared
        .mailbox
        .lock()
        .unwrap()
        .generated_ahead
        .clone()
        .unwrap();
    assert!(
        !stop.load(Ordering::Acquire),
        "second admission started during first Hold"
    );
    wait(&cache);
    // Continuing the first provider must not replace its successor's job.
    let next = playing(entries[0] + 1);
    cache.prepare(&shared, &next, &retained);
    assert!(Arc::ptr_eq(
        &stop,
        shared
            .mailbox
            .lock()
            .unwrap()
            .generated_ahead
            .as_ref()
            .unwrap()
    ));
    assert!(!stop.load(Ordering::Acquire));
    let gap = playing(entries[1] - 1);
    assert!(matches!(
        workspace
            .plan
            .picture(ProjectFrame(entries[1] - 1))
            .unwrap()
            .picture,
        deadpan_plan::Picture::Source { .. }
    ));
    cache.select(&gap, &mut retained, true).unwrap();
    perform(&gap, &mut retained, &mut None).unwrap();
    assert!(Arc::ptr_eq(
        &original_index.upgrade().unwrap(),
        &retained.as_ref().unwrap().source.shared_index()
    ));
    cache.prepare(&shared, &gap, &retained);
    assert!(
        first_index.upgrade().is_none(),
        "idle old provider released"
    );
    assert!(
        !stop.load(Ordering::Acquire),
        "prepared successor survives the short Original gap"
    );
    let second = playing(entries[1]);
    cache.select(&second, &mut retained, true).unwrap();
    let admitted = retained.as_ref().expect("successor already admitted");
    assert_eq!(
        admitted.source.decode_plan(SourceFrameId(0)),
        deadpan_media::source_session::DecodePlan::Current
    );
    let mut reference = None;
    let mut compared = 0;
    for frame in entries[1]..workspace.plan.duration().frames() {
        if !matches!(
            workspace.plan.picture(ProjectFrame(frame)).unwrap().picture,
            deadpan_plan::Picture::Accepted {
                generated: Some(_),
                ..
            }
        ) {
            break;
        }
        let request = playing(frame);
        let actual = perform(&request, &mut retained, &mut None).unwrap();
        let expected = perform(&request, &mut reference, &mut None).unwrap();
        assert_eq!(actual.id, expected.id);
        assert_eq!(
            actual.frame.unwrap().bytes(),
            expected.frame.unwrap().bytes()
        );
        compared += 1;
    }
    assert!(compared > 1);
    // Stopping while the first provider serves also cancels a new successor
    // admission, even without a stopped picture request.
    let first = playing(entries[0]);
    cache.select(&first, &mut retained, true).unwrap();
    perform(&first, &mut retained, &mut None).unwrap();
    cache.prepare(&shared, &first, &retained);
    let stop = shared
        .mailbox
        .lock()
        .unwrap()
        .generated_ahead
        .clone()
        .unwrap();
    cache.release(&mut retained);
    assert!(stop.load(Ordering::Acquire));
    assert!(
        original_index.upgrade().is_none(),
        "parked Original released on stop"
    );
}

#[test]
#[ignore = "requires the real accepted bundle fixture; see module documentation"]
fn generated_holds_preserve_both_verified_decoders_and_original_lookahead() {
    let fixture = fixture();
    let workspace = fixture.workspace(701);
    let shared = Shared::default();
    let mut retained = None;
    let mut cache = PlaybackCache::default();
    let mut proposal = None;
    let mut proxy = ProxySlot::default();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let mut generated_index = None;
    let mut generated_first = None;
    let mut source_first = None;
    let mut was_generated = false;
    let mut entries = 0;
    let mut returns = 0;
    let mut boundary_timings = Vec::new();
    let mut generated_pictures = std::collections::BTreeMap::new();
    tests_support::ADMISSIONS.set(0);
    // Three selection-loop laps over Original 12..27 then the accepted
    // 15-frame Hold. Do not structurally edit its conditioning context.
    for ordinal in 0..90 {
        let frame = ordinal % 30;
        let sample = workspace.plan.picture(ProjectFrame(frame)).unwrap();
        let generated = matches!(
            sample.picture,
            deadpan_plan::Picture::Accepted {
                generated: Some(_),
                ..
            }
        );
        if was_generated && !generated {
            let original = cache
                .original(&mut retained)
                .as_mut()
                .expect("Original retained through Generated Hold");
            let ahead = original
                .ahead
                .lookahead
                .as_mut()
                .expect("Original look-ahead retained");
            let deadline = Instant::now() + Duration::from_secs(15);
            while ahead.positioned() != Some(SourceFrameId(12)) {
                assert!(Instant::now() < deadline, "return picture was not prepared");
                ahead.poll();
                std::thread::sleep(Duration::from_millis(2));
            }
            returns += 1;
        }
        let mut playing = request(&workspace, sequence(frame), ordinal as u64 + 1);
        playing.ticket.transport = Some(generation);
        playing.loop_ahead = LoopLookAhead::new(0, 30, frame + 120);
        shared.mailbox.lock().unwrap().latest = Some(playing.ticket);
        let started = Instant::now();
        cache.select(&playing, &mut retained, true).unwrap();
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
        if frame == 0 || generated != was_generated {
            boundary_timings.push(serde_json::json!({
                "frame": frame, "lap": ordinal / 30, "generated": generated,
                "seconds": started.elapsed().as_secs_f64(),
            }));
        }
        assert_eq!(picture.tier, PictureTier::Original);
        if generated {
            let bytes = picture.frame.as_ref().unwrap().bytes();
            if let Some(first) = generated_pictures.get(&picture.id.0) {
                assert_eq!(
                    first, bytes,
                    "every Generated picture is unchanged on re-entry"
                );
            } else {
                generated_pictures.insert(picture.id.0, bytes.to_vec());
            }
            let index = retained.as_ref().unwrap().source.shared_index();
            if let Some(first) = &generated_index {
                assert!(
                    Arc::ptr_eq(first, &index),
                    "re-entry reused the admitted Generated decoder"
                );
            } else {
                generated_index = Some(index);
            }
            if !was_generated {
                entries += 1;
                let bytes = picture.frame.as_ref().unwrap().bytes();
                if let Some(first) = &generated_first {
                    assert_eq!(first, bytes);
                } else {
                    generated_first = Some(bytes.to_vec());
                }
            }
            assert!(matches!(
                cache.original(&mut retained).as_ref().unwrap().key,
                SessionKey::Project { .. }
            ));
        } else if picture.id == SourceFrameId(12) {
            let bytes = picture.frame.as_ref().unwrap().bytes();
            if let Some(first) = &source_first {
                assert_eq!(first, bytes);
            } else {
                source_first = Some(bytes.to_vec());
            }
        }
        schedule_lookahead(&shared, &playing, cache.original(&mut retained));
        was_generated = generated;
    }
    assert_eq!((entries, returns), (3, 2));
    assert_eq!(generated_pictures.len(), 15);
    eprintln!(
        "generated_playback_boundaries={}",
        serde_json::to_string(&boundary_timings).unwrap()
    );
    assert_eq!(
        tests_support::ADMISSIONS.get(),
        1,
        "the Original was never reopened"
    );
    assert_eq!(
        cache
            .original(&mut retained)
            .as_ref()
            .unwrap()
            .ahead
            .lookahead
            .as_ref()
            .unwrap()
            .stats()
            .swaps,
        2
    );
    // Stop back on Original restores it before releasing the other slot.
    let stopped = request(&workspace, sequence(0), 10_000);
    cache.select(&stopped, &mut retained, false).unwrap();
    let picture = perform(&stopped, &mut retained, &mut proposal).unwrap();
    assert_eq!(picture.frame.unwrap().bytes(), source_first.unwrap());
    let generated_index = generated_index.unwrap();
    let generated_weak = Arc::downgrade(&generated_index);
    drop(generated_index);
    cache.release(&mut retained);
    assert!(
        generated_weak.upgrade().is_none(),
        "Generated slot released"
    );
    assert!(
        retained
            .as_ref()
            .unwrap()
            .ahead
            .lookahead
            .as_ref()
            .unwrap()
            .positioned()
            .is_none()
    );
    assert_eq!(tests_support::ADMISSIONS.get(), 1);
}

#[test]
#[ignore = "requires the real accepted bundle fixture; see module documentation"]
fn generated_playback_worker_matches_the_independent_single_slot_reader() {
    let fixture = fixture();
    let workspace = fixture.workspace(702);
    let worker = PreviewWorker::new(egui::Context::default()).unwrap();
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let mut reference = None;
    let mut proposal = None;
    tests_support::ADMISSIONS.set(0);
    for ordinal in 0..90 {
        let frame = ordinal % 30;
        let mut playing = request(&workspace, sequence(frame), ordinal as u64 + 1);
        let expected = perform(&playing, &mut reference, &mut proposal).unwrap();
        playing.ticket.transport = Some(generation);
        worker.submit_with_lookahead(
            playing.ticket,
            playing.work,
            LoopLookAhead::new(0, 30, frame + 120),
        );
        let reply = await_reply(&worker);
        assert_eq!(reply.ticket, playing.ticket);
        let actual = reply.picture.unwrap();
        assert_eq!(actual.id, expected.id, "Edit frame {frame}");
        assert_eq!(actual.tier, PictureTier::Original);
        assert_eq!(
            actual.frame.unwrap().bytes(),
            expected.frame.unwrap().bytes(),
            "Edit frame {frame}"
        );
    }
    assert_eq!(
        tests_support::ADMISSIONS.get(),
        3,
        "the single-slot reference reopens Original after each Generated Hold"
    );
    worker.cancel();
    worker.shutdown();
}

fn prepare_generated(
    fixture: &Fixture,
    session: u64,
) -> (
    Arc<Workspace>,
    Shared,
    PlaybackCache,
    Option<RetainedSession>,
    deadpan_output::Generation,
) {
    let workspace = fixture.workspace(session);
    let shared = Shared::default();
    let mut cache = PlaybackCache::default();
    let mut retained = None;
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let mut playing = request(&workspace, sequence(0), 1);
    playing.ticket.transport = Some(generation);
    shared.mailbox.lock().unwrap().latest = Some(playing.ticket);
    cache.select(&playing, &mut retained, true).unwrap();
    perform(&playing, &mut retained, &mut None).unwrap();
    cache.prepare(&shared, &playing, &retained);
    let deadline = Instant::now() + Duration::from_secs(15);
    while !cache.generated_finished() {
        assert!(
            Instant::now() < deadline,
            "Generated admission did not finish ahead of entry"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    (workspace, shared, cache, retained, generation)
}

#[test]
#[ignore = "requires the real accepted bundle fixture; see module documentation"]
fn generated_admission_and_first_decode_finish_before_the_hold_is_requested() {
    let fixture = fixture();
    let (workspace, shared, mut cache, mut retained, generation) = prepare_generated(&fixture, 703);
    let mut entry = request(&workspace, sequence(15), 2);
    entry.ticket.transport = Some(generation);
    shared.mailbox.lock().unwrap().latest = Some(entry.ticket);
    let started = Instant::now();
    cache.select(&entry, &mut retained, true).unwrap();
    let admitted = retained
        .as_ref()
        .expect("the prefetched decoder was adopted before reading");
    assert!(matches!(admitted.key, SessionKey::Generated { .. }));
    assert_eq!(
        admitted.source.decode_plan(SourceFrameId(0)),
        deadpan_media::source_session::DecodePlan::Current
    );
    let index = admitted.source.shared_index();
    let actual = perform(&entry, &mut retained, &mut None).unwrap();
    eprintln!(
        "prepared_generated_entry_seconds={}",
        started.elapsed().as_secs_f64()
    );
    assert!(Arc::ptr_eq(
        &index,
        &retained.as_ref().unwrap().source.shared_index()
    ));
    let expected = perform(&entry, &mut None, &mut None).unwrap();
    assert_eq!(
        actual.frame.unwrap().bytes(),
        expected.frame.unwrap().bytes()
    );
    cache.release(&mut retained);
}

#[test]
#[ignore = "requires the real accepted bundle fixture; see module documentation"]
fn cancelled_or_different_project_admissions_cannot_be_adopted() {
    let fixture = fixture();
    for cancel in [false, true] {
        let (workspace, shared, mut cache, mut retained, generation) =
            prepare_generated(&fixture, 704);
        let workspace = if cancel {
            let stop = shared
                .mailbox
                .lock()
                .unwrap()
                .generated_ahead
                .clone()
                .unwrap();
            shared.mailbox.lock().unwrap().cancel();
            assert!(stop.load(Ordering::Acquire));
            workspace
        } else {
            fixture.workspace(705)
        };
        let mut entry = request(&workspace, sequence(15), 2);
        entry.ticket.transport = Some(generation);
        cache.select(&entry, &mut retained, true).unwrap();
        assert!(
            retained.is_none(),
            "stale/cancelled preparation was not reused"
        );
        // A normal request can still recover through the authoritative reader.
        perform(&entry, &mut retained, &mut None).unwrap();
        cache.release(&mut retained);
    }
}
