#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Decode-ahead on real media: pictures across cuts are the exact pictures
//! of a sequential decode, in the requested order, and the look-ahead's
//! scheduling, retargeting, cancellation and release stay bounded.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{AssetId, SourceFrameId};
use deadpan_media::lookahead::{DiscontinuityScan, LookAhead, continuous};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_session::{
    DecodePlan, IndexMeasurement, SourceSession, SourceSessionLimits,
};
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(20);

fn source_fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn identity(bytes: &[u8]) -> SourceContentIdentity {
    SourceContentIdentity::new(Sha256::digest(bytes).into(), bytes.len() as u64).unwrap()
}

fn reference(bytes: &[u8]) -> (SourceSession, Vec<blake3::Hash>) {
    let cancelled = AtomicBool::new(false);
    let mut session = SourceSession::open_verified(
        &mut Cursor::new(bytes),
        identity(bytes),
        AssetId::new("source").unwrap(),
        SourceSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let count = session.index().index().frames().len() as u64;
    let hashes = (0..count)
        .map(|frame| {
            blake3::hash(
                &session
                    .frame(SourceFrameId(frame), TIMEOUT, &cancelled)
                    .unwrap()
                    .rgba,
            )
        })
        .collect();
    (session, hashes)
}

/// A progressively admitted, threaded serving session, as the native
/// viewer opens it; its companions share the background measurement.
fn serving(bytes: &[u8], reference: &SourceSession, threads: u32) -> SourceSession {
    let mut limits = SourceSessionLimits::interactive();
    limits.decode.threads = threads;
    SourceSession::open_admitted(
        &mut Cursor::new(bytes),
        reference.shared_index(),
        reference.info(),
        limits,
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(Instant::now() < deadline, "look-ahead did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// An edit of the Original as played: fragments with backward Repeat
/// restarts, a forward jump within a group of pictures and a jump to the
/// final keyframe group, repeated.
fn edit(count: u64) -> Vec<u64> {
    let mut plan = Vec::new();
    let mut push = |range: std::ops::Range<u64>| plan.extend(range.filter(|f| *f < count));
    push(0..30);
    push(20..40);
    push(20..40);
    push(60..70);
    push(70..74);
    push(5..15);
    push(5..15);
    push(count - 20..count);
    push(31..36);
    plan
}

#[test]
fn pictures_across_cuts_are_exact_in_order_and_come_from_the_lookahead() {
    for name in ["pyramid-bframes.mp4", "cfr-bframes.mp4"] {
        let bytes = source_fixture(name);
        let (reference, expected) = reference(&bytes);
        let count = expected.len() as u64;
        let plan = edit(count);
        for threads in [1, 8] {
            let cancelled = AtomicBool::new(false);
            let mut active = serving(&bytes, &reference, threads);
            let mut lookahead = LookAhead::new(active.companion(), TIMEOUT);
            let mut scan = DiscontinuityScan::default();
            let mut served = Vec::new();
            let mut cuts = 0;
            for (frame, ordinal) in plan.iter().enumerate() {
                let id = SourceFrameId(*ordinal);
                let cut = frame > 0 && !continuous(SourceFrameId(plan[frame - 1]), id);
                if cut {
                    cuts += 1;
                    // Deterministic: the companion has had time to arrive.
                    wait_until(|| {
                        lookahead.poll();
                        lookahead.positioned() == Some(id)
                    });
                }
                if let Some(companion) = lookahead.take_for(id, active.decode_plan(id)) {
                    let previous = std::mem::replace(&mut active, companion);
                    lookahead.keep(previous);
                }
                if cut {
                    assert_eq!(
                        active.decode_plan(id),
                        DecodePlan::Current,
                        "{name}, {threads} threads: cut at {frame} served by the companion"
                    );
                }
                let picture = active.frame(id, TIMEOUT, &cancelled).unwrap();
                assert_eq!(
                    blake3::hash(&picture.rgba),
                    expected[*ordinal as usize],
                    "{name}, {threads} threads, edit picture {frame} (Original {ordinal})"
                );
                served.push(*ordinal);
                let frame = frame as i64;
                let target = scan
                    .next(frame, frame + 40, |next| {
                        Ok::<_, ()>(plan.get(next as usize).copied().map(SourceFrameId))
                    })
                    .unwrap()
                    .map(|found| found.target);
                lookahead.request(target);
            }
            assert_eq!(served, plan, "{name}: in the requested order");
            let stats = lookahead.stats();
            assert_eq!(stats.swaps, cuts, "{name}: every cut swapped decoders");
            assert!(stats.reached >= cuts, "{stats:?}");
            assert_eq!(stats.opened, 1, "one companion decoder for the Original");
            assert_eq!(stats.failed, 0, "{stats:?}");
            assert_eq!(
                active.wait_measured(TIMEOUT),
                IndexMeasurement::Verified,
                "the companion shares the complete measurement"
            );
        }
    }
}

#[test]
fn retargeting_cancellation_and_release_are_bounded() {
    let bytes = source_fixture("pyramid-bframes.mp4");
    let (reference, expected) = reference(&bytes);
    let cancelled = AtomicBool::new(false);
    let mut active = serving(&bytes, &reference, 8);
    active.frame(SourceFrameId(0), TIMEOUT, &cancelled).unwrap();
    let mut lookahead = LookAhead::new(active.companion(), TIMEOUT);
    assert!(!lookahead.holds_decoder(), "nothing opens before a request");

    // A newer target replaces an older one; the companion ends at the newer.
    lookahead.request(Some(SourceFrameId(90)));
    lookahead.request(Some(SourceFrameId(40)));
    wait_until(|| {
        lookahead.poll();
        lookahead.positioned() == Some(SourceFrameId(40))
    });
    assert_eq!(lookahead.positioning(), None);

    // The serving decoder keeps a sequential picture: no swap on a tie.
    assert!(
        lookahead
            .take_for(SourceFrameId(0), active.decode_plan(SourceFrameId(0)))
            .is_none()
    );

    // Cancel stops positioning without waiting and keeps the decoder.
    lookahead.request(Some(SourceFrameId(95)));
    assert!(lookahead.stop_flag().is_some());
    lookahead.cancel();
    wait_until(|| {
        lookahead.poll();
        lookahead.positioning().is_none()
    });
    assert!(lookahead.holds_decoder());

    // A request overtaking an unfinished job is still exact, whichever
    // decoder serves it.
    lookahead.request(Some(SourceFrameId(94)));
    let id = SourceFrameId(94);
    if let Some(companion) = lookahead.take_for(id, active.decode_plan(id)) {
        let previous = std::mem::replace(&mut active, companion);
        lookahead.keep(previous);
    }
    let picture = active.frame(id, TIMEOUT, &cancelled).unwrap();
    assert_eq!(blake3::hash(&picture.rgba), expected[94]);

    // Release closes the companion; dropping a working look-ahead joins
    // its thread within one native call.
    lookahead.release();
    assert!(!lookahead.holds_decoder());
    lookahead.request(Some(SourceFrameId(80)));
    let dropped = Instant::now();
    drop(lookahead);
    assert!(dropped.elapsed() < Duration::from_secs(5));
    // The serving decoder is unaffected.
    let picture = active
        .frame(SourceFrameId(95), TIMEOUT, &cancelled)
        .unwrap();
    assert_eq!(blake3::hash(&picture.rgba), expected[95]);
}
