//! Preview proxy pictures through the real preview worker: a seek shows the
//! verified proxy first and the exact Original picture once the cursor rests.

use std::io::Write;

use deadpan_cli::proxy::cache::ProxyCache;
use deadpan_media::proxy::{ProxyReason, VerifyControl, verify_proxy};

use super::*;

const PROXY_FIXTURE: &str = "tests/fixtures/proxy/cfr-bframes.proxy.mp4";

/// A private per-test proxy cache.
fn cache() -> (tempfile::TempDir, ProxyCache) {
    let directory = tempfile::tempdir().unwrap();
    let cache = ProxyCache::at(&directory.path().join("Proxies")).unwrap();
    (directory, cache)
}

/// The main viewer's worker, reading `cache`.
fn proxy_worker(cache: &ProxyCache) -> PreviewWorker {
    let worker = PreviewWorker::new(egui::Context::default()).unwrap();
    worker.set_proxy_cache(Some(cache.clone()));
    worker
}

/// Verify the committed proxy of `cfr-bframes.mp4` against the Original and
/// publish it through the cache, as a background build would.
fn publish_proxy(fixture: &Fixture, workspace: &Workspace, cache: &ProxyCache) {
    let cancelled = AtomicBool::new(false);
    let registered = &workspace.sources[&asset()];
    let video = registered.receipt.snapshot().video().unwrap();
    let original = &registered.original;
    let mut snapshot = fixture
        .store
        .snapshot_original(
            original.object().content(),
            OriginalMediaLimits::default(),
            &cancelled,
        )
        .unwrap();
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        video.index().content(),
        2_000_000,
        Duration::from_secs(10),
        &cancelled,
    )
    .unwrap();
    let staging = cache.stage().unwrap();
    staging
        .movie()
        .write_all(
            &std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PROXY_FIXTURE)).unwrap(),
        )
        .unwrap();
    let sidecar = verify_proxy(
        staging.movie(),
        &input,
        original.object().content().digest(),
        Arc::new(video.index().clone()),
        video.interpretation(),
        ProxyReason::Requested,
        VerifyControl {
            timeout: Duration::from_secs(60),
            cancelled: &cancelled,
            pause: None,
        },
    )
    .unwrap();
    let key = deadpan_cli::proxy::proxy_key(original, video).unwrap();
    cache.publish(&key, staging, &sidecar).unwrap();
}

/// The worker opens a proxy only while idle, after a request wanted one.
/// Seek until a proxy picture arrives, then take its refinement.
fn until_proxy(worker: &PreviewWorker, workspace: &Arc<Workspace>, serial: &mut u64) {
    let deadline = Instant::now() + Duration::from_secs(15);
    for frame in [5, 70, 15, 80, 25, 95, 35, 110].iter().cycle() {
        assert!(Instant::now() < deadline, "the proxy never opened");
        *serial += 1;
        let seek = request(workspace, sequence(*frame), *serial);
        worker.submit(seek.ticket, seek.work);
        if await_reply(worker).picture.unwrap().tier == PictureTier::Proxy {
            let refined = await_reply(worker);
            assert_eq!(refined.ticket.request, *serial);
            assert_eq!(refined.picture.unwrap().tier, PictureTier::Original);
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn exact_picture(workspace: &Arc<Workspace>, view: ProjectView) -> Vec<u8> {
    // The thumbnail-style worker never reads proxies.
    let worker = PreviewWorker::named(egui::Context::default(), "exact").unwrap();
    let request = request(workspace, view, 1);
    worker.submit(request.ticket, request.work);
    let picture = await_reply(&worker).picture.unwrap();
    assert_eq!(picture.tier, PictureTier::Original);
    worker.shutdown();
    picture.frame.unwrap().bytes().to_vec()
}

#[test]
fn a_seek_shows_the_proxy_then_refines_to_the_exact_original_picture() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(1);
    let (_directory, cache) = cache();
    publish_proxy(&fixture, &workspace, &cache);
    let exact = exact_picture(&workspace, sequence(60));
    let worker = proxy_worker(&cache);
    // The first seek is exact and only marks the proxy as wanted.
    let first = request(&workspace, sequence(60), 1);
    worker.submit(first.ticket, first.work);
    assert_eq!(
        await_reply(&worker).picture.unwrap().tier,
        PictureTier::Original
    );
    let mut serial = 1;
    until_proxy(&worker, &workspace, &mut serial);
    serial += 1;
    let mut presentation = crate::presentation::Presentation::default();
    let seek = request(&workspace, sequence(60), serial);
    presentation.request(seek.ticket, &seek.work);
    worker.submit(seek.ticket, seek.work);

    let started = Instant::now();
    let proxy = await_reply(&worker);
    assert_eq!(proxy.ticket.request, serial);
    {
        let picture = proxy.picture.as_ref().unwrap();
        assert_eq!(picture.tier, PictureTier::Proxy);
        assert_eq!(picture.id, SourceFrameId(60));
        // Same Original moment, same clock; different (lossy) pixels.
        assert_eq!(
            picture.frame.as_ref().unwrap().metadata().pts.ticks,
            60 * 1001
        );
        assert_eq!(picture.summary.as_ref().unwrap().frame_count, 120);
    }
    assert!(presentation.receive(proxy).unwrap().is_ok());
    presentation.presented();
    assert!(presentation.refining());
    assert_eq!(
        presentation.displayed_label().as_deref(),
        Some("Showing sequence frame 61 · proxy preview")
    );
    // A proxy picture never satisfies Camera's exact-picture gate.
    assert_eq!(
        presentation.stable_sequence_ticket(1, workspace.document.revision_id(), ProjectFrame(60)),
        None
    );

    let refined = await_reply(&worker);
    // An isolated seek refines after the short rest only.
    assert!(started.elapsed() >= ISOLATED_REFINE_DELAY);
    assert_eq!(
        refined.ticket.request, serial,
        "refinement keeps the request"
    );
    {
        let picture = refined.picture.as_ref().unwrap();
        assert_eq!(picture.tier, PictureTier::Original);
        assert_eq!(picture.id, SourceFrameId(60));
        assert_eq!(picture.frame.as_ref().unwrap().bytes(), exact.as_slice());
    }
    assert!(presentation.receive(refined).unwrap().is_ok());
    assert!(!presentation.refining());
    assert!(
        presentation.needs_render(),
        "the exact picture is a new presentation identity"
    );
    presentation.presented();
    assert_eq!(
        presentation.displayed_label().as_deref(),
        Some("Showing sequence frame 61")
    );
    assert!(
        presentation
            .stable_sequence_ticket(1, workspace.document.revision_id(), ProjectFrame(60))
            .is_some()
    );

    // The Original decoder now sits at frame 60, so a step is exact at once.
    let step = request(&workspace, sequence(61), serial + 1);
    worker.submit(step.ticket, step.work);
    let step = await_reply(&worker).picture.unwrap();
    assert_eq!(step.tier, PictureTier::Original);
    assert_eq!(step.id, SourceFrameId(61));
    // A backward step needs a keyframe seek, but is still shown exactly:
    // no proxy picture flashes before it.
    let back = request(&workspace, sequence(60), serial + 2);
    worker.submit(back.ticket, back.work);
    let back = await_reply(&worker).picture.unwrap();
    assert_eq!(
        (back.tier, back.id),
        (PictureTier::Original, SourceFrameId(60))
    );
    assert_eq!(back.frame.unwrap().bytes(), exact.as_slice());
    std::thread::sleep(REFINE_DELAY * 2);
    assert!(worker.take_reply().is_none(), "nothing to refine");
    worker.shutdown();
}

#[test]
fn a_newer_seek_before_the_rest_period_cancels_refinement() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(1);
    let (_directory, cache) = cache();
    publish_proxy(&fixture, &workspace, &cache);
    let worker = proxy_worker(&cache);
    let mut opened = 0;
    until_proxy(&worker, &workspace, &mut opened);
    for (serial, frame) in [(101, 90), (102, 10), (103, 100)] {
        let seek = request(&workspace, sequence(frame), serial);
        worker.submit(seek.ticket, seek.work);
        let reply = await_reply(&worker);
        assert_eq!(reply.ticket.request, serial);
        assert_eq!(reply.picture.unwrap().tier, PictureTier::Proxy);
    }
    // Only the last request is refined; no earlier exact picture arrives.
    let refined = await_reply(&worker);
    assert_eq!(refined.ticket.request, 103);
    let refined = refined.picture.unwrap();
    assert_eq!(
        (refined.tier, refined.id),
        (PictureTier::Original, SourceFrameId(100))
    );
    std::thread::sleep(REFINE_DELAY * 2);
    assert!(worker.take_reply().is_none());
    worker.shutdown();
}

/// A seek that follows another within `REFINE_DELAY` is scrubbing: its
/// refinement waits the full rest, not the isolated one.
#[test]
fn a_moving_cursor_waits_the_full_rest_before_refining() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(1);
    let (_directory, cache) = cache();
    publish_proxy(&fixture, &workspace, &cache);
    let worker = proxy_worker(&cache);
    let mut serial = 0;
    until_proxy(&worker, &workspace, &mut serial);
    std::thread::sleep(REFINE_DELAY * 2);
    while worker.take_reply().is_some() {}
    // Backward from the Original decoder's position, so each needs a seek.
    for (offset, frame) in [(1, 3), (2, 1)] {
        let seek = request(&workspace, sequence(frame), serial + offset);
        worker.submit(seek.ticket, seek.work);
        assert_eq!(
            await_reply(&worker).picture.unwrap().tier,
            PictureTier::Proxy
        );
    }
    let proxied = Instant::now();
    std::thread::sleep(ISOLATED_REFINE_DELAY * 2);
    if proxied.elapsed() < REFINE_DELAY {
        assert!(
            worker.take_reply().is_none(),
            "a scrubbing seek must not refine after the isolated rest"
        );
    }
    let refined = await_reply(&worker);
    assert!(proxied.elapsed() >= REFINE_DELAY - ISOLATED_REFINE_DELAY);
    assert_eq!(refined.ticket.request, serial + 2);
    assert_eq!(refined.picture.unwrap().tier, PictureTier::Original);
    worker.shutdown();
}

#[test]
fn proposals_playback_and_thumbnails_never_use_the_proxy() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(1);
    let (_directory, cache) = cache();
    publish_proxy(&fixture, &workspace, &cache);
    let worker = proxy_worker(&cache);
    let mut serial = 0;
    until_proxy(&worker, &workspace, &mut serial);
    // With the proxy open, a transport-tagged (audition) picture is exact.
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let mut playing = request(&workspace, sequence(50), serial + 1);
    playing.ticket.transport = Some(feed.restart(0).unwrap());
    worker.submit(playing.ticket, playing.work);
    assert_eq!(
        await_reply(&worker).picture.unwrap().tier,
        PictureTier::Original
    );
    std::thread::sleep(REFINE_DELAY * 2);
    assert!(worker.take_reply().is_none(), "nothing to refine");
    worker.shutdown();
    assert_eq!(exact_picture(&workspace, sequence(80)).len(), 320 * 180 * 4);
}

#[test]
fn a_damaged_or_removed_proxy_falls_back_to_the_original_without_an_error() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(1);
    let (_directory, cache) = cache();
    publish_proxy(&fixture, &workspace, &cache);
    let registered = &workspace.sources[&asset()];
    let video = registered.receipt.snapshot().video().unwrap();
    let key = deadpan_cli::proxy::proxy_key(&registered.original, video).unwrap();
    // Corrupt the published sidecar in place.
    let directory = cache.path().join(key.directory());
    let sidecar = directory.join("proxy.json");
    let mut permissions = std::fs::metadata(&sidecar).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o600);
    std::fs::set_permissions(&sidecar, permissions).unwrap();
    std::fs::write(&sidecar, b"{\"schema\":1}").unwrap();
    let worker = proxy_worker(&cache);
    let seek = request(&workspace, sequence(30), 1);
    worker.submit(seek.ticket, seek.work);
    let picture = await_reply(&worker).picture.unwrap();
    assert_eq!(
        (picture.tier, picture.id),
        (PictureTier::Original, SourceFrameId(30))
    );
    // The damaged entry is not retried until it changes.
    std::thread::sleep(Duration::from_millis(100));
    let seek = request(&workspace, sequence(90), 2);
    worker.submit(seek.ticket, seek.work);
    assert_eq!(
        await_reply(&worker).picture.unwrap().tier,
        PictureTier::Original
    );
    // Rebuilding replaces the damaged entry; later seeks use it.
    assert!(cache.remove(&key).unwrap());
    publish_proxy(&fixture, &workspace, &cache);
    let mut serial = 2;
    until_proxy(&worker, &workspace, &mut serial);
    worker.shutdown();
}
