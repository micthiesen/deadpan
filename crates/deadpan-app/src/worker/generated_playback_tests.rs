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
    let package = std::env::var_os("DEADPAN_GENERATED_PREVIEW_PACKAGE")
        .map(PathBuf::from)
        .expect("DEADPAN_GENERATED_PREVIEW_PACKAGE must name the accepted bundle fixture");
    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join("preview.deadpan");
    deadpan_store::portable::copy_portable(&package, &copy, &AtomicBool::new(false)).unwrap();
    let store = ProjectStore::open(&copy, deadpan_store::AccessMode::ReadWrite).unwrap();
    Fixture { scratch, store }
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
        cache.select(&playing, &mut retained, true);
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
    cache.select(&stopped, &mut retained, false);
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
