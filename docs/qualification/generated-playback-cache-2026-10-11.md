# Generated playback decoder reuse, 2026-10-11

Committed playback now retains the Original decoder and its prepared return
picture while a Generated Hold is on screen. Re-entering the same accepted
Hold reuses its Generated decoder. This removes repeated admission at those
transitions; DP-16 remains partial because cold Generated entry preparation
and full-resolution device measurements are still owed.

## Implementation and bounds

`worker::playback_cache` parks one inactive provider. There is at most one
Original decoder, its existing single look-ahead companion, and one Generated
decoder. Only the active decoder produces the requested picture. The Original
scanner continues across Generated pictures and can prepare a cut or loop
return while they play.

Choosing a slot grants no media authority. The ordinary readers still check
their complete keys, source receipts and Generated handle liveness before use.
Different project sessions discard both slots; changed artifacts, asset
records, color policy and source interpretation use the existing readmission
rules. Proposals, copied views and endpoints keep their ordinary reader policy.

A stopped reply releases the inactive slot and Original companion. Cancellation
also wakes the worker with a release request, so stopping without another
picture does not retain extra playback decoders. Teardown runs on the worker
outside the mailbox lock and retains the active picture's decoder.

## Verification

The real-media fixture uses `generated_joins` with a registered Original,
actual boundary conditioning, durable attempts, host bundle qualification and
explicit acceptance. Only the model is replaced by the synthetic worker.
The source is the repository's 320×180 `cfr-bframes.mp4` at 30000/1001 fps.
Tests copy its accepted package through verified portable export.

Two explicit release tests pass over three selection-loop laps of Edit
`[0,30)`: Original pictures 12–26, then the 15-frame accepted Hold.

- The direct worker path admits the Original once, reuses the same Generated
  index on all three entries, checks all 15 Generated pictures on each lap,
  and serves both Original returns through prepared decoder swaps.
- The actual preview-worker thread returns all 90 picture identities and
  complete pixel buffers exactly equal to an independent single-slot reader.
  That reference admits the Original three times, exposing the work avoided
  by the cache.
- A stopped Original request recovers its retained decoder. Releasing playback
  drops the Generated index's final strong reference and the Original companion.
- All **41 existing preview-worker tests pass**. The default run skips the two
  fixture-dependent tests, which were run explicitly above, plus the existing
  large-package seek-storm test, which was not rerun.
- Strict workspace/all-target Clippy and rustfmt pass.

The first fixture attempt lacked a qualified conditioning Original and was
correctly rejected. A structural fixture then required relevance reconciliation;
after that correction, wrapping its group in a Repeat changed the Hold's exit
context and reverted it to the fallback. The test rejected that fixture because
it contained no Generated entries. The final test loops the unchanged accepted
edit instead. All failed runs are retained. A separate CLI copy passed the
Group/Repeat transactions and full history validation; this does not count as
Generated playback evidence.

Review was performed by the sole implementing agent, as requested. No native
window, audio-device playback, GPU timing or listening check was run for this
change. Those are not established by the picture-buffer tests.

## Small-fixture observations

On the M5 Max / 128 GiB / macOS 26.5.2 reference Mac, release Rust 1.97.1 with
the qualified FFmpeg 8.0.3 prefix:

| Picture request | First passing run | Separate resource run |
| --- | ---: | ---: |
| Cold Generated entry | 13.599 ms | 14.840 ms |
| Generated re-entry, laps 2 / 3 | 0.616 / 0.533 ms | 0.727 / 0.577 ms |
| Prepared Original return, laps 2 / 3 | 0.103 / 0.064 ms | 0.113 / 0.068 ms |

The resource run passes both tests in 0.96 seconds, with 99.3 MB maximum RSS
and 74.4 MB peak footprint. These are whole-test observations, including fixture
copies and the reference reader, not full-resolution realtime acceptance.
Return preparation is awaited explicitly for deterministic correctness under
test load. The first Generated admission still verifies all retained objects
and measures the sampled master synchronously on the preview worker.

The [evidence manifest](../../tools/media-qualification/evidence/2026-10-11-generated-playback-cache/manifest.json)
retains source and executable hashes, fixture pins, commands, complete test
logs, resource observations, review notes and failed attempts. The reproducible
fixture and copied test executable remain in
`/tmp/deadpan-generated-lookahead-20261011`. The test module documents the two
environment variables for exporting and consuming the accepted fixture.

Executed release-test SHA-256:
`ecd28bbd19c7a5475e24bf8bf1ff32a45c0bf6ce95fc7f5b97f3e077f683e62b`.
